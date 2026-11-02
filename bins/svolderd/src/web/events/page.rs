// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The task that follows what belongs to the whole page: the fleet (the devices and the hops
//! after them) and the computer's player.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::response::sse::Event;
use svolder_ipc::DeviceInfo;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::{Context, host_changed, send_json};
use crate::fleet::HostWatch;
use crate::web::model::{self, PageModel};
use crate::web::router::devices;
use crate::web::view::{self, playing_of};

/// What the fleet's event says, from what it is made of: the devices' names, links and
/// families, and when the configuration file (which holds the hops) last changed.
#[derive(Debug, PartialEq, Eq)]
struct FleetKey {
    devices: Vec<(String, &'static str, &'static str, &'static str)>,
    config: Option<SystemTime>,
}

/// The page's task.
pub(super) struct Task {
    cx: Arc<Context>,
    host: Option<HostWatch>,
    /// What the `host` event last said (`Playing::key`); `None` with no card.
    host_key: Option<String>,
    /// Woken when something outside the page's own reads changed the fleet.
    fleet_changed: watch::Receiver<u64>,
    /// What the `fleet` event last said, and what it was made of.
    fleet_key: Option<FleetKey>,
    fleet_sent: String,
}

impl Task {
    /// The page's task. The route reads are held for as long as it lives when `route` is set,
    /// and the host's changes are followed only by a page that needs it.
    pub(super) fn new(cx: Arc<Context>, route: bool) -> Task {
        let mut host = cx.web.fleet.host_watch(route);
        if let Some(watch) = host.as_mut() {
            watch.changes.borrow_and_update();
        }
        if !cx.needs.host {
            host = None;
        }
        let mut fleet_changed = cx.web.fleet_changed.subscribe();
        fleet_changed.borrow_and_update();
        let host_key = host_key(&cx);
        Task {
            host,
            host_key,
            fleet_changed,
            fleet_key: None,
            fleet_sent: String::new(),
            cx,
        }
    }

    /// Note what the fleet is made of now, `devices`: call it before the model that carries the
    /// fleet is read, so a change in between is found.
    pub(super) fn watch_fleet(&mut self, devices: &[DeviceInfo]) {
        self.fleet_key = Some(self.key_of(devices));
    }

    /// Take the fleet and the hops `model` carries as what the page has.
    pub(super) fn seed(&mut self, model: &PageModel) {
        self.fleet_sent = serde_json::to_string(&model.fleet).unwrap_or_default();
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
                () = host_changed(&mut self.host) => {
                    if !self.host_turn(&tx).await {
                        return;
                    }
                }
                _ = second.tick() => {
                    if !self.fleet_turn(&tx, false).await {
                        return;
                    }
                }
            }
        }
    }

    /// What the fleet's event is made of now.
    fn key_of(&self, devices: &[DeviceInfo]) -> FleetKey {
        let web = &self.cx.web;
        FleetKey {
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
            config: web
                .fleet
                .config_path()
                .and_then(|path| std::fs::metadata(path).ok())
                .and_then(|meta| meta.modified().ok()),
        }
    }

    /// Send the fleet when it changed: a device's link or family, the list, or the hops, which
    /// are re-read only when the configuration file has changed or `forced` says it has.
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

    /// Send the computer's player when what it says changed. `false` when the page has gone.
    async fn host_turn(&mut self, tx: &mpsc::Sender<Event>) -> bool {
        let key = host_key(&self.cx);
        if key == self.host_key {
            return true;
        }
        self.host_key = key;
        let host = model::host_model(&self.cx.web, self.cx.access);
        send_json(tx, "host", &host).await.is_ok()
    }
}

/// What the computer's player says now, by `Playing::key`.
fn host_key(cx: &Context) -> Option<String> {
    playing_of(&cx.web.fleet.host_view(), cx.access, None).map(|card| card.key())
}
