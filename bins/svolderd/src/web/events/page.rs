// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The task that follows what belongs to the whole page: the fleet (the devices and the hops
//! after them), the computer's player, and the device walk.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::response::sse::Event;
use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_ipc::socket::protocol::ScenesView;
use svolder_ipc::{DeviceInfo, JobState, Request, WalkOp};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::{Context, host_changed, send_json};
use crate::fleet::HostWatch;
use crate::locks::locked;
use crate::web::model::{self, ComputerModel, HostModel, PageModel, RouteModel};
use crate::web::router::devices;
use crate::web::view::{self, Playing};

/// What the fleet's event says, from what it is made of: the devices' names, links and
/// families, the default device (a hop with no `after` follows it) and the configuration the
/// daemon holds (which has the hops). The snapshot is held, so one that was replaced never
/// compares equal to its successor.
#[derive(Debug)]
struct FleetKey {
    devices: Vec<(String, &'static str, &'static str, &'static str)>,
    default: Option<String>,
    config: Option<Arc<svolder_ipc::Config>>,
}

impl PartialEq for FleetKey {
    fn eq(&self, other: &Self) -> bool {
        let same_config = match (&self.config, &other.config) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        same_config && self.default == other.default && self.devices == other.devices
    }
}

/// The page's task.
pub(super) struct Task {
    cx: Arc<Context>,
    host: Option<HostWatch>,
    /// What the `host` event last said; `None` with no card.
    host_key: Option<HostKey>,
    /// When the computer EQ's state file last changed: a preset applied to the same sink changes
    /// no route, so the second's turn looks for it.
    eq_stamp: Option<SystemTime>,
    /// Woken when something outside the page's own reads changed the fleet.
    fleet_changed: watch::Receiver<u64>,
    /// What the `fleet` event last said, and what it was made of.
    fleet_key: Option<FleetKey>,
    fleet_sent: String,
    /// The scenes' view, for a viewer who may write: followed like the fleet, with no `needs` word.
    scenes: Option<watch::Receiver<ScenesView>>,
    /// What the `scenes` event last said, as JSON.
    scenes_sent: String,
    /// The walk's states, for a page that needs them and a viewer who may write: one follow of
    /// the daemon's per page. Behind a lock so the task stays `Sync`, taken for one poll at a
    /// time.
    walk: Option<std::sync::Mutex<BoxStream<'static, svolder_ipc::Event>>>,
    /// The walks this page has seen running: only their end can have written a device, an ended
    /// walk the follow replays on opening has not.
    running: std::collections::BTreeSet<svolder_ipc::JobId>,
}

impl Task {
    /// The page's task. The route reads are held for as long as it lives when `route` is set,
    /// and the host's changes are followed only by a page that needs it.
    pub(super) async fn new(cx: Arc<Context>, route: bool) -> Task {
        let mut host = cx.web.fleet.host_watch(route);
        if let Some(watch) = host.as_mut() {
            watch.changes.borrow_and_update();
        }
        if !cx.needs.host {
            host = None;
        }
        let mut fleet_changed = cx.web.fleet_changed.subscribe();
        fleet_changed.borrow_and_update();
        let host_key = match host_parts(&cx) {
            Some((card, route)) => Some(HostKey::new(&card, route, host_computer(&cx).await)),
            None => None,
        };
        let mut scenes = may_write(&cx).then(|| cx.web.fleet.scenes()).flatten();
        if let Some(view) = scenes.as_mut() {
            view.borrow_and_update();
        }
        let walk = match cx.needs.walk && may_write(&cx) {
            true => {
                let follow = Request::Walk { op: WalkOp::Follow };
                cx.web
                    .fleet
                    .stream(follow)
                    .await
                    .ok()
                    .map(std::sync::Mutex::new)
            }
            false => None,
        };
        Task {
            host,
            host_key,
            eq_stamp: eq_stamp().await,
            fleet_changed,
            fleet_key: None,
            fleet_sent: String::new(),
            scenes,
            scenes_sent: String::new(),
            walk,
            running: std::collections::BTreeSet::new(),
            cx,
        }
    }

    /// Note what the fleet is made of now, `devices`: call it before the model that carries the
    /// fleet is read, so a change in between is found.
    pub(super) async fn watch_fleet(&mut self, devices: &[DeviceInfo]) {
        self.fleet_key = Some(self.key_of(devices));
    }

    /// Take the fleet and the hops `model` carries as what the page has.
    pub(super) fn seed(&mut self, model: &PageModel) {
        self.fleet_sent = serde_json::to_string(&model.fleet).unwrap_or_default();
        self.scenes_sent = model
            .scenes
            .as_ref()
            .and_then(|view| serde_json::to_string(view).ok())
            .unwrap_or_default();
    }

    /// Send what changes until the page goes or a rotated token signs every browser out
    /// (`signed_out` turns).
    pub(super) async fn run(
        mut self,
        mut signed_out: watch::Receiver<u64>,
        tx: mpsc::Sender<Event>,
    ) {
        let second = Duration::from_secs(1);
        let mut second = tokio::time::interval_at(Instant::now() + second, second);
        second.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                _ = signed_out.changed() => return,
                () = tx.closed() => return,
                changed = self.fleet_changed.changed() => {
                    if changed.is_err() || !self.fleet_turn(&tx, true).await {
                        return;
                    }
                }
                () = scenes_changed(&mut self.scenes) => {
                    if !self.scenes_turn(&tx).await {
                        return;
                    }
                }
                () = host_changed(&mut self.host) => {
                    if !self.host_turn(&tx).await {
                        return;
                    }
                }
                event = walk_event(&self.walk) => {
                    if !self.walk_turn(&tx, event).await {
                        return;
                    }
                }
                _ = second.tick() => {
                    if !self.fleet_turn(&tx, false).await || !self.eq_turn(&tx).await {
                        return;
                    }
                }
            }
        }
    }

    /// Send the scenes' view when it differs from what the page has. `false` when the page has
    /// gone.
    async fn scenes_turn(&mut self, tx: &mpsc::Sender<Event>) -> bool {
        let Some(watch) = self.scenes.as_mut() else {
            return true;
        };
        let view = watch.borrow_and_update().clone();
        let json = serde_json::to_string(&view).unwrap_or_default();
        if json == self.scenes_sent {
            return true;
        }
        self.scenes_sent = json;
        send_json(tx, "scenes", &view).await.is_ok()
    }

    /// Send a state of the walk, `None` once its stream has ended (the daemon stopping). A walk
    /// that ended with a device written turns the fleet for every open page. `false` when the
    /// page has gone.
    async fn walk_turn(
        &mut self,
        tx: &mpsc::Sender<Event>,
        event: Option<svolder_ipc::Event>,
    ) -> bool {
        match event {
            Some(svolder_ipc::Event::Walk(job)) => {
                let wrote = walk_wrote(&mut self.running, &job);
                if wrote {
                    self.cx.web.fleet_changed.send_modify(|turn| *turn += 1);
                }
                let told = serde_json::json!({
                    "run": super::run_stamp(), "job": job.job, "state": job.state,
                });
                send_json(tx, "walk", &told).await.is_ok()
            }
            Some(_) => true,
            None => {
                self.walk = None;
                true
            }
        }
    }

    /// What the fleet's event is made of now.
    fn key_of(&self, devices: &[DeviceInfo]) -> FleetKey {
        let web = &self.cx.web;
        FleetKey {
            default: web.fleet.resolve(None).ok().map(|info| info.name),
            config: web.fleet.config().ok(),
            devices: devices
                .iter()
                .map(|info| {
                    let link = view::link_of(info);
                    let family = view::family_of(web, info);
                    (
                        info.name.clone(),
                        link.state,
                        link.word,
                        view::family_words(family).0,
                    )
                })
                .collect(),
        }
    }

    /// Send the fleet when it changed: a device's link or family, the list, or the hops, which
    /// are re-read only when the daemon's configuration has changed or `forced` says it has.
    /// `false` when the page has gone.
    async fn fleet_turn(&mut self, tx: &mpsc::Sender<Event>, forced: bool) -> bool {
        let web = &self.cx.web;
        let devices = devices(web).await;
        let key = self.key_of(&devices);
        if !forced && self.fleet_key.as_ref() == Some(&key) {
            return true;
        }
        self.fleet_key = Some(key);
        let fleet = model::fleet_model(web, &devices).await;
        let json = serde_json::to_string(&fleet).unwrap_or_default();
        if json == self.fleet_sent {
            return true;
        }
        self.fleet_sent = json;
        send_json(tx, "fleet", &fleet).await.is_ok()
    }

    /// Send the computer's player when the computer EQ's state file has changed since the last
    /// look, for a page that follows the host. `false` when the page has gone.
    async fn eq_turn(&mut self, tx: &mpsc::Sender<Event>) -> bool {
        if self.host.is_none() {
            return true;
        }
        let stamp = eq_stamp().await;
        if stamp == self.eq_stamp {
            return true;
        }
        self.eq_stamp = stamp;
        self.host_turn(tx).await
    }

    /// Send the computer's player when what it says changed. `false` when the page has gone.
    async fn host_turn(&mut self, tx: &mpsc::Sender<Event>) -> bool {
        // One read of the route (and of the state file behind it): the key and the event say the same.
        let parts = host_parts(&self.cx);
        let computer = host_computer(&self.cx).await;
        let key = parts
            .as_ref()
            .map(|(card, route)| HostKey::new(card, route.clone(), computer.clone()));
        if key == self.host_key {
            return true;
        }
        self.host_key = key;
        let host = parts.map(|(card, route)| HostModel::new(&card, route, computer));
        send_json(tx, "host", &host).await.is_ok()
    }
}

/// Whether `job`'s state is the end of a walk this page saw running, with a device written; the
/// jobs seen running are kept in `running`. An ended walk the follow replays on opening was never
/// seen running, so it is not counted, and a final state sent again counts once.
fn walk_wrote(
    running: &mut std::collections::BTreeSet<svolder_ipc::JobId>,
    job: &svolder_ipc::JobEvent,
) -> bool {
    if job.state.is_final() {
        running.remove(&job.job)
            && matches!(&job.state, JobState::Done { payload } if !payload.is_null())
    } else {
        running.insert(job.job);
        false
    }
}

/// Wait for the scenes' view to change; for ever when the page has none, or once the daemon that
/// kept it has gone.
async fn scenes_changed(scenes: &mut Option<watch::Receiver<ScenesView>>) {
    if let Some(watch) = scenes.as_mut()
        && watch.changed().await.is_ok()
    {
        return;
    }
    *scenes = None;
    std::future::pending().await
}

/// Whether the page's viewer may write: signed in, with `[web] writes` on. Only such a viewer
/// may walk, so only such a page is sent the walk's states.
fn may_write(cx: &Context) -> bool {
    cx.viewer == crate::web::Viewer::Signed && cx.web.config.writes
}

/// The walk's next event; for ever when the page does not follow the walk.
async fn walk_event(
    walk: &Option<std::sync::Mutex<BoxStream<'static, svolder_ipc::Event>>>,
) -> Option<svolder_ipc::Event> {
    match walk.as_ref() {
        Some(stream) => std::future::poll_fn(|cx| locked(stream).poll_next_unpin(cx)).await,
        None => std::future::pending().await,
    }
}

/// What the `host` event says, to tell whether it changed: the card's key (`Playing::key`), the
/// route's model and the computer's controls.
#[derive(Debug, PartialEq)]
pub(super) struct HostKey {
    card: String,
    route: Option<RouteModel>,
    computer: Option<ComputerModel>,
}

impl HostKey {
    pub(super) fn new(
        card: &Playing,
        route: Option<RouteModel>,
        computer: Option<ComputerModel>,
    ) -> HostKey {
        HostKey {
            card: card.key(),
            route,
            computer,
        }
    }
}

/// What the computer's player and route say now: the card and the route's model.
fn host_parts(cx: &Context) -> Option<(Playing, Option<RouteModel>)> {
    model::host_parts(&cx.web, cx.access)
}

/// The computer's controls for the page's shown device.
async fn host_computer(cx: &Context) -> Option<ComputerModel> {
    model::host_computer(&cx.web, cx.shown.as_ref()).await
}

/// When the computer EQ's state file last changed; `None` while there is none.
async fn eq_stamp() -> Option<SystemTime> {
    stamp_off_thread(svolder_ipc::config::host_eq_state_path()).await
}

/// [`stamp_of`] on a blocking thread.
async fn stamp_off_thread(path: std::path::PathBuf) -> Option<SystemTime> {
    tokio::task::spawn_blocking(move || stamp_of(&path))
        .await
        .ok()
        .flatten()
}

/// When `path` was last written; `None` while it is not there.
fn stamp_of(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::walk_wrote;
    use serde_json::json;
    use svolder_ipc::{JobEvent, JobState};

    fn done(job: u64, payload: serde_json::Value) -> JobEvent {
        JobEvent {
            job,
            state: JobState::Done { payload },
        }
    }

    fn progress(job: u64) -> JobEvent {
        JobEvent {
            job,
            state: JobState::Progress { payload: json!({}) },
        }
    }

    #[test]
    fn a_walk_seen_running_then_done_with_a_device_counts() {
        let mut running = BTreeSet::new();
        assert!(!walk_wrote(&mut running, &progress(1)));
        assert!(walk_wrote(&mut running, &done(1, json!({"name": "d"}))));
    }

    #[test]
    fn a_replayed_done_for_a_job_never_seen_running_does_not_count() {
        let mut running = BTreeSet::new();
        assert!(!walk_wrote(&mut running, &done(1, json!({"name": "d"}))));
    }

    #[test]
    fn a_done_sent_again_counts_once() {
        let mut running = BTreeSet::new();
        walk_wrote(&mut running, &progress(1));
        assert!(walk_wrote(&mut running, &done(1, json!({"name": "d"}))));
        assert!(!walk_wrote(&mut running, &done(1, json!({"name": "d"}))));
    }

    #[test]
    fn a_done_with_nothing_written_or_a_cancel_counts_for_nothing_and_leaves_the_set() {
        let mut running = BTreeSet::new();
        walk_wrote(&mut running, &progress(1));
        assert!(!walk_wrote(&mut running, &done(1, serde_json::Value::Null)));
        walk_wrote(&mut running, &progress(2));
        assert!(!walk_wrote(
            &mut running,
            &JobEvent {
                job: 2,
                state: JobState::Cancelled
            }
        ));
        assert!(running.is_empty());
    }

    use super::*;

    #[test]
    fn a_state_file_written_again_has_another_stamp() {
        let dir = std::env::temp_dir().join(format!("svolderd-eq-stamp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("host-eq.toml");
        assert_eq!(stamp_of(&file), None, "no file, no stamp");
        std::fs::write(&file, "on = true\n").unwrap();
        let first = stamp_of(&file);
        assert!(first.is_some());
        let later = first.unwrap() + Duration::from_secs(2);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(stamp_of(&file), Some(later));
        assert_ne!(stamp_of(&file), first);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
