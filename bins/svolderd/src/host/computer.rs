// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's own controls, served beside its player: the default sink's volume and mute,
//! and the computer EQ's points, kept once for every front end.
//!
//! - The EQ is one [`EqControl`] behind an async lock. A task ticks it every 50 ms (the period
//!   its coalescing counts in) and has it read its state file again about once a second. Readers
//!   never take that lock: the view they get is the controller's own watch channel
//!   ([`EqControl::watcher`]), because a tick can sit inside a `pw-dump` or a `pw-cli` write.
//! - What goes into the host view is that watch channel thinned to at most [`VIEW_EVERY`]
//!   apart, with the last value always delivered: ten updates a second to a watcher, however
//!   fast a drag moves.
//! - The volume is read every [`VOLUME_EVERY`] while someone watches, once after a write and once
//!   for a one-shot `host` request; with nobody watching nothing is read.
//! - Nothing here writes to the computer's audio unless a request asks it to. Starting the
//!   service writes nothing; only a daemon in a container applies the chain again, once per
//!   start of the computer side (the daemon's start, or the host coming back on after a reload),
//!   when the saved state says it was on ([`Computer::start`]). The view never says on while no
//!   chain runs.
//! - Volume writes are coalesced to the newest value, at most [`VOLUME_WRITE_EVERY`] apart, and
//!   the last value is always written. The volume is 0 to 100 on the wire.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use super::Shared;
use svolder_host::eq::{Edit, EqControl, EqView, Keep, PwCliSession};
use svolder_host::{Sink, SinkState};
use svolder_ipc::HostEqOp;
use svolder_ipc::host::HostVolume;
use svolder_profiles::tone::apo::{FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{self, BothSides, Preset};
use tokio::sync::{Mutex, watch};
use tokio::time::Instant;

/// How often the EQ controller is ticked: the period its live-update coalescing counts in.
const TICK: Duration = Duration::from_millis(50);

/// How many ticks apart the controller is asked to read its state file again (about 1 s).
const RELOAD_EVERY: u64 = 20;

/// The least time between two EQ views a watcher is sent.
pub(super) const VIEW_EVERY: Duration = Duration::from_millis(100);

/// How often the sink is read while someone watches.
pub(super) const VOLUME_EVERY: Duration = Duration::from_secs(2);

/// The least time between two volume writes to the sink.
const VOLUME_WRITE_EVERY: Duration = Duration::from_millis(100);

/// The most the computer's volume is set to, in percent.
const VOLUME_MAX: u8 = 100;

/// The start of every reason the computer's controls cannot be used.
const UNAVAILABLE: &str = "computer controls unavailable";

/// Whether `message` is one of the reasons the computer's controls cannot be used.
pub(crate) fn is_unavailable(message: &str) -> bool {
    message.starts_with(UNAVAILABLE)
}

/// Why a change of the EQ was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EqRefusal {
    /// The reason in words.
    Words(String),
    /// A preset that connected devices play too: asked about, not refused. Yes repeats the
    /// request with `confirm`, which flattens `devices` first.
    Both {
        /// The preset's name.
        name: String,
        /// The configured names of the devices that play it.
        devices: Vec<String>,
    },
}

impl From<String> for EqRefusal {
    fn from(words: String) -> Self {
        EqRefusal::Words(words)
    }
}

impl std::fmt::Display for EqRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EqRefusal::Words(words) => f.write_str(words),
            EqRefusal::Both { name, devices } => write!(
                f,
                "{name} is on the {} too: the correction would play twice",
                devices.join(" and the ")
            ),
        }
    }
}

/// The most points the EQ view says there can be when no controller runs.
const POINTS_MAX: u8 = svolder_host::eq::live::SLOTS as u8;

/// What the computer's controls are made of.
pub(crate) struct Computer {
    /// The default sink, or why there is none.
    sink: Result<Arc<dyn Sink>, String>,
    /// The EQ controller and the view it publishes, or why there is none.
    eq: Result<Eq, String>,
    /// The volume writes made.
    writes: Mutex<VolumeWrites>,
    /// The newest volume asked for, with its number.
    asked: std::sync::Mutex<(u64, u8)>,
}

/// The volume writes made: the number of the last request one covered, when, and what it read
/// back.
#[derive(Default)]
struct VolumeWrites {
    covered: u64,
    at: Option<Instant>,
    read: Option<HostVolume>,
}

/// A running EQ controller.
struct Eq {
    control: Arc<Mutex<EqControl>>,
    view: watch::Receiver<EqView>,
}

impl Computer {
    /// Controls over `sink` and `eq`; each is the reason it is missing when it is.
    pub(crate) fn new(sink: Result<Arc<dyn Sink>, String>, eq: Result<EqControl, String>) -> Self {
        Computer {
            writes: Mutex::default(),
            asked: std::sync::Mutex::default(),
            sink,
            eq: eq.map(|control| Eq {
                view: control.watcher(),
                control: Arc::new(Mutex::new(control)),
            }),
        }
    }

    /// The real computer: `wpctl` for the sink, `pw-cli` and the manager of the chain for the EQ.
    /// Whatever tool is missing says so in the view.
    pub(crate) fn real() -> Self {
        let sink = svolder_host::Wpctl::from_env()
            .map(|wpctl| Arc::new(wpctl) as Arc<dyn Sink>)
            .ok_or_else(|| "no wpctl on this computer".to_string());
        let eq = PwCliSession::from_env()
            .ok_or_else(|| "no pw-cli on this computer".to_string())
            .map(|session| {
                let host = Arc::new(svolder_host::eq::Host::live(
                    svolder_ipc::config::host_eq_paths(),
                    svolder_ipc::checks::live::in_container(),
                ));
                EqControl::new(
                    host,
                    Box::new(session),
                    Keep::File(svolder_ipc::config::host_eq_state_path()),
                )
            });
        Computer::new(sink, eq)
    }

    /// The EQ as a front end is shown it: the controller's newest view, or the reason there is
    /// no controller.
    pub(super) fn eq_view(&self) -> EqView {
        match &self.eq {
            Ok(eq) => eq.view.borrow().clone(),
            Err(why) => EqView {
                unavailable: Some(format!("{UNAVAILABLE}: {why}")),
                max: POINTS_MAX,
                ..EqView::default()
            },
        }
    }

    /// The sink as it reads now, as a front end shows it; `None` when there is no sink or it did
    /// not answer.
    pub(super) async fn read_volume(&self) -> Option<HostVolume> {
        let sink = self.sink.as_ref().ok()?;
        sink.read().await.map(volume_of)
    }

    /// Set the volume to `pct` percent and read it back. Writes are coalesced: a request that a
    /// newer one has overtaken by the time its turn comes is answered with what that write read
    /// back, and no two writes are closer than [`VOLUME_WRITE_EVERY`], so the newest value is
    /// always the one written last.
    pub(super) async fn set_volume(&self, pct: u8) -> Result<HostVolume, String> {
        if pct > VOLUME_MAX {
            return Err(format!("the computer's volume goes up to {VOLUME_MAX} %"));
        }
        let sink = self.sink()?;
        let number = {
            let mut asked = crate::locks::locked(&self.asked);
            *asked = (asked.0 + 1, pct);
            asked.0
        };
        let mut writes = self.writes.lock().await;
        if writes.covered >= number
            && let Some(read) = writes.read.clone()
        {
            return Ok(read);
        }
        if let Some(at) = writes.at {
            tokio::time::sleep_until(at + VOLUME_WRITE_EVERY).await;
        }
        let (newest, pct) = *crate::locks::locked(&self.asked);
        if !sink.set_volume(f32::from(pct) / 100.0).await {
            return Err("the computer's sound did not take the volume".to_string());
        }
        let read = self.read_back(sink).await?;
        *writes = VolumeWrites {
            covered: newest,
            at: Some(Instant::now()),
            read: Some(read.clone()),
        };
        Ok(read)
    }

    /// Mute or unmute the sink and read it back.
    pub(super) async fn set_mute(&self, muted: bool) -> Result<HostVolume, String> {
        let sink = self.sink()?;
        if !sink.set_mute(muted).await {
            return Err("the computer's sound did not take the mute".to_string());
        }
        // What a coalesced volume request would be answered with is no longer what the sink
        // reads.
        self.writes.lock().await.read = None;
        self.read_back(sink).await
    }

    fn sink(&self) -> Result<&Arc<dyn Sink>, String> {
        self.sink
            .as_ref()
            .map_err(|why| format!("{UNAVAILABLE}: {why}"))
    }

    async fn read_back(&self, sink: &Arc<dyn Sink>) -> Result<HostVolume, String> {
        sink.read()
            .await
            .map(volume_of)
            .ok_or_else(|| "the computer's sound could not be read back".to_string())
    }

    fn control(&self) -> Result<&Arc<Mutex<EqControl>>, String> {
        self.eq
            .as_ref()
            .map(|eq| &eq.control)
            .map_err(|why| format!("{UNAVAILABLE}: {why}"))
    }

    /// The start of the computer side: in a container, apply the chain again when the saved
    /// state says it was on and none runs; then, whatever the place, take the EQ as off in the
    /// view when it is said to be on and no chain runs. Outside a container nothing is applied.
    async fn start(&self, container: bool) {
        let Ok(control) = self.control() else { return };
        let mut control = control.lock().await;
        if container {
            match control.reapply_if_on().await {
                Ok(true) => crate::log::info("host: the computer EQ was applied again"),
                Ok(false) => {}
                Err(why) => crate::log::warn(&format!(
                    "host: the computer EQ was not applied again: {why}"
                )),
            }
        }
        control.sync_running().await;
    }

    /// Carry out one change of the EQ. A `Preset` is looked up in `presets`; if a device
    /// `holders` names for it plays the same one, the change is refused with a question unless it
    /// is confirmed, in which case `flatten` flattens those devices first. `SaveAs` is not the
    /// controller's: the daemon answers it.
    pub(super) async fn eq_op<F, Fut>(
        &self,
        op: HostEqOp,
        presets: &[Preset],
        holders: impl Fn(&str) -> Vec<String>,
        flatten: F,
    ) -> Result<(), EqRefusal>
    where
        F: FnOnce(Vec<String>) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let control = self.control()?;
        match op {
            HostEqOp::Mode { on } => {
                let mut control = control.lock().await;
                Ok(control.set_on(on).await.map_err(|e| e.to_string())?)
            }
            HostEqOp::Preset {
                name,
                confirm,
                device: _,
            } => {
                let found = presets
                    .iter()
                    .find(|candidate| preset::same_name(&candidate.name, &name))
                    .ok_or_else(|| format!("no preset named {name:?}"))?;
                let correction = preset::host_correction(found)?;
                let held = holders(&found.name);
                let other = held.first().map(|_| found.name.as_str());
                let mut flattened = Vec::new();
                if let BothSides::Ask { name, .. } = preset::both_sides(found, other) {
                    if !confirm {
                        return Err(EqRefusal::Both {
                            name,
                            devices: held,
                        });
                    }
                    flatten(held.clone()).await?;
                    flattened = held;
                }
                let origin = found.source.as_ref().and_then(|s| s.origin.clone());
                let mut control = control.lock().await;
                Ok(control
                    .load(found.name.clone(), origin, correction)
                    .await
                    .map_err(|e| flattened_note(e.to_string(), &flattened, &found.name))?)
            }
            HostEqOp::Apply {
                name,
                origin,
                correction,
                sink,
            } => {
                if sink.is_some() {
                    return Err(
                        "the daemon puts the computer EQ in front of the default sink; \
                         choose another sink without a daemon"
                            .to_string()
                            .into(),
                    );
                }
                let mut control = control.lock().await;
                Ok(control
                    .load(name, origin, correction)
                    .await
                    .map_err(|e| e.to_string())?)
            }
            HostEqOp::Set {
                index,
                slot,
                kind,
                f,
                g,
                q,
                done,
            } => {
                let edit = Edit::Set {
                    index: index_for(index, slot)?,
                    slot,
                    kind: kind.as_deref().map(kind_of).transpose()?,
                    f,
                    g,
                    q,
                    done,
                };
                Ok(self.edit(control, edit).await?)
            }
            HostEqOp::Add { kind, f, g, q } => {
                let kind = kind_of(&kind)?;
                Ok(self
                    .edit(control, Edit::Add(SourceFilter { kind, f, g, q }))
                    .await?)
            }
            HostEqOp::Remove { index, slot } => {
                let edit = Edit::Remove {
                    index: index_for(index, slot)?,
                    slot,
                };
                Ok(self.edit(control, edit).await?)
            }
            HostEqOp::SaveAs { .. } => Err("the daemon saves a preset itself".to_string().into()),
        }
    }

    async fn edit(&self, control: &Arc<Mutex<EqControl>>, edit: Edit) -> Result<(), String> {
        let mut control = control.lock().await;
        control.edit(edit).await.map_err(|e| e.to_string())
    }

    /// Tick the controller every 50 ms and have it read its state file again about once a
    /// second, until the task is ended.
    pub(super) async fn tick(&self) {
        let Ok(control) = self.control() else {
            return std::future::pending().await;
        };
        let mut every = tokio::time::interval(TICK);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut ticks = 0u64;
        loop {
            every.tick().await;
            ticks += 1;
            let mut control = control.lock().await;
            control.tick().await;
            if ticks.is_multiple_of(RELOAD_EVERY) {
                control.reload_if_changed();
            }
        }
    }

    /// Each EQ view worth sending, passed to `send` no closer than [`VIEW_EVERY`] apart, the
    /// newest value always; the first at once.
    pub(super) async fn mirror(&self, send: impl Fn(EqView)) {
        let Ok(eq) = &self.eq else {
            return send(self.eq_view());
        };
        let mut view = eq.view.clone();
        send(view.borrow_and_update().clone());
        let mut last: Option<Instant> = None;
        loop {
            if view.changed().await.is_err() {
                return;
            }
            if let Some(at) = last {
                tokio::time::sleep_until(at + VIEW_EVERY).await;
            }
            send(view.borrow_and_update().clone());
            last = Some(Instant::now());
        }
    }
}

/// Run the computer's controls for one run of the host until the task is ended: tick the EQ,
/// mirror its view into the host's at most ten times a second, and read the volume while someone
/// watches.
pub(super) async fn run(
    shared: Arc<Shared>,
    generation: u64,
    computer: Arc<Computer>,
    container: bool,
) {
    let put_eq = |eq: EqView| {
        shared.current(generation, || {
            shared.view.send_if_modified(|view| {
                let same = view.eq.as_ref() == Some(&eq);
                if !same {
                    view.eq = Some(eq);
                }
                !same
            });
        });
    };
    tokio::join!(
        computer.start(container),
        computer.tick(),
        computer.mirror(put_eq),
        follow_volume(&shared, generation, &computer),
    );
}

/// Put `volume` in the host view, when it is not already there.
pub(super) fn put_volume(shared: &Shared, volume: Option<HostVolume>) {
    shared.view.send_if_modified(|view| {
        let same = view.volume == volume;
        if !same {
            view.volume = volume;
        }
        !same
    });
}

/// Read the sink at once for the first watcher and then every [`VOLUME_EVERY`], and drop what
/// was read when the last one goes; with nobody watching nothing is read.
async fn follow_volume(shared: &Shared, generation: u64, computer: &Computer) {
    let mut watchers = shared.watchers.subscribe();
    loop {
        if *watchers.borrow_and_update() == 0 {
            shared.current(generation, || put_volume(shared, None));
            if watchers.changed().await.is_err() {
                return;
            }
            continue;
        }
        let read = computer.read_volume().await;
        shared.current(generation, || put_volume(shared, read));
        // Another watcher joining, or one of several leaving, changes nothing: only the last
        // one going ends the reads.
        let next = tokio::time::sleep(VOLUME_EVERY);
        tokio::pin!(next);
        loop {
            tokio::select! {
                () = &mut next => break,
                changed = watchers.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    if *watchers.borrow_and_update() == 0 {
                        break;
                    }
                }
            }
        }
    }
}

/// The sink as a front end shows it: the volume in whole percent, boosted or not.
fn volume_of(state: SinkState) -> HostVolume {
    HostVolume {
        pct: (state.volume.max(0.0) * 100.0).round() as u16,
        muted: state.muted,
        sink: state.name,
    }
}

/// `why` a recall was refused, with the devices that were flattened for it said, so the caller
/// knows what to put back; `why` as it is when none were.
fn flattened_note(why: String, devices: &[String], name: &str) -> String {
    match devices.is_empty() {
        true => why,
        false => format!(
            "{why} The {} was flattened; put its curve back with `svolder eq load {name}`.",
            devices.join(" and the ")
        ),
    }
}

/// A point index from the wire (from 1) as the controller counts them.
fn zero_based(index: u8) -> Result<usize, String> {
    usize::from(index)
        .checked_sub(1)
        .ok_or_else(|| "points count from 1".to_string())
}

/// The 0-based index a request means: with a slot the index is not looked at (the controller
/// finds the point by its slot), else it counts from 1.
fn index_for(index: u8, slot: Option<u8>) -> Result<usize, String> {
    match slot {
        Some(_) => Ok(usize::from(index.max(1)) - 1),
        None => zero_based(index),
    }
}

/// A point's type from its word on the wire.
fn kind_of(word: &str) -> Result<FilterKind, String> {
    FilterKind::from_str(word).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
