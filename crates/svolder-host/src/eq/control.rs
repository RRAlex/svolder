// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! [`EqControl`]: the one controller of the computer EQ, shared by the daemon and the TUI.
//!
//! It owns the points, turns the EQ on and off, recalls presets and applies edits. An edit that
//! is part of a drag (`done: false`) goes to the running chain through [`Params`] and nothing
//! else: the owner calls [`EqControl::tick`] every 50 ms, and a tick sends the newest value of
//! each point that moved since the last one, so a drag is at most 20 updates a second and
//! the last value is always sent (on the next tick, or at once when the drag ends). The end of
//! a drag (`done: true`) sends what is left, keeps the state file and the chain's config, and
//! is the only edit that writes a file.
//!
//! Live updates are checked once: after the first one the key it set is read back, and a value
//! that did not take turns live updates off for good. From then on a drag only moves the view
//! and the end of it applies the chain, which is how the EQ always worked. A `pw-cli` that fails
//! to take sends over and over has the same effect and says so (see [`EqView::unavailable`]).
//!
//! The preamp is [`played_preamp`] of the points as they stand and goes out with every update,
//! so the curve never plays above unity, even for the moment between two updates.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use tokio::sync::watch;

use serde::{Deserialize, Serialize};
use svolder_profiles::tone::apo::{self, Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{from_computer, host_correction};

use super::HostEqState;
use super::live::{CONFIG_RATE_HZ, SHAPE, SLOTS, SlotShape, preamp_params, slot_params};
use super::manage::{ApplyRequest, Chain, EqFailure, check_playable, played_preamp};
use super::points::{Change, Points};
use super::session::Params;

/// What a live edit that did not take says, once.
pub const LIVE_NOTE: &str = "this computer's PipeWire takes EQ changes when a drag ends";

/// What the view says while a chain from an older svolder plays.
pub const LEGACY_NOTE: &str =
    "an older svolder's computer EQ is playing; turning the computer EQ off and on replaces it";

/// The name of a curve a hand has changed.
pub const CUSTOM: &str = "custom";

/// How many ticks apart the chain is looked at again (one `pw-dump`): 20 ticks of 50 ms.
///
/// Time is counted in ticks, so the periods below hold only as long as the owner calls
/// [`EqControl::tick`] every 50 ms: the caller drives the period.
const WATCH_EVERY: u64 = 20;

/// How many reads of the checked port in a row must miss before live updates are given up.
const READ_BACK_MISSES: u32 = 3;

/// The window of ticks failed sends are counted in (5 s at 50 ms), and how many of them in it
/// give live updates up. One restart of `pw-cli` costs about 20 (its restart gap), so one
/// death is ridden out and a second inside the window is not.
const FAIL_WINDOW: u64 = 100;
const FAIL_LIMIT: usize = 30;

/// Where the points are kept between runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keep {
    /// In the state file at this path, which the command line reads and writes too.
    File(PathBuf),
    /// Nowhere: the controller writes no file of its own.
    Memory,
}

/// One point as a screen shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointView {
    /// The slot of the live chain the point sits in (1 to 16): it is the point's own for as long
    /// as the point exists, whatever place the table lists it in. 0 when a sender gave none.
    #[serde(default)]
    pub slot: u8,
    /// `PK`, `LSC` or `HSC`.
    pub kind: String,
    /// Frequency in Hz.
    pub f: f64,
    /// Gain in dB.
    pub g: f64,
    /// Q.
    pub q: f64,
}

/// The computer EQ as a screen shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EqView {
    /// Whether the EQ is on.
    pub on: bool,
    /// The points, in the order the table lists them.
    pub points: Vec<PointView>,
    /// The preamp in dB, never above 0.
    pub preamp_db: f64,
    /// The preset the points came from, `custom` once a hand changed them.
    pub preset: Option<String>,
    /// Whether a drag is heard as it moves.
    pub live: bool,
    /// Why it is not, once.
    pub note: Option<String>,
    /// Why the computer's controls cannot be used, as a sentence that starts
    /// `computer controls unavailable:`.
    pub unavailable: Option<String>,
    /// The most points there can be.
    pub max: u8,
    /// Whether it could not be told if the chain runs: `on` is then what the state file says.
    #[serde(default)]
    pub running_unknown: bool,
}

/// One change of the points.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Set the fields that are given on a point (0-based). `done` is false while a drag goes on
    /// and true on its last value, and for a change that is not a drag.
    Set {
        /// The point.
        index: usize,
        /// The slot the sender holds for the point. When given the point is the one in that
        /// slot, whatever place the table lists it in now, and `index` is not looked at: a
        /// request made against a table that has been sorted since changes the point it meant,
        /// or is refused when that point is gone.
        slot: Option<u8>,
        /// Its new type.
        kind: Option<FilterKind>,
        /// Its new frequency, held to 20 Hz to 20 kHz.
        f: Option<f64>,
        /// Its new gain, held to plus or minus 12 dB.
        g: Option<f64>,
        /// Its new Q, held to 0.1 to 10.
        q: Option<f64>,
        /// Whether this is the last value of the drag.
        done: bool,
    },
    /// Add a point.
    Add(SourceFilter),
    /// Remove a point: the one in `slot` when that is given (see [`Edit::Set`]), else the one at
    /// `index` (0-based).
    Remove {
        /// The point.
        index: usize,
        /// The slot the sender holds for the point.
        slot: Option<u8>,
    },
}

/// What identifies a state file's contents without reading it.
type Stamp = (SystemTime, u64, u64);

/// What the chain was last known to run as, for the values sent to it.
#[derive(Debug, Clone, Copy, Default)]
struct Sent {
    /// The node the values went to; a restart of the chain is a new one.
    id: Option<u32>,
    /// The rate the coefficients were worked out for.
    rate: Option<u32>,
}

/// The controller.
pub struct EqControl {
    chain: Arc<dyn Chain>,
    params: Box<dyn Params>,
    keep: Keep,
    points: Points,
    /// The preamp the curve states; 0 once a hand edits it.
    stated: f64,
    preset: Option<String>,
    origin: Option<String>,
    /// The state the chain was last applied or read with: the sink, route and manager.
    base: Option<HostEqState>,
    on: bool,
    /// Whether the chain's running could not be told when it was last looked at.
    unknown: bool,
    live: bool,
    note: Option<String>,
    /// The chain that plays is one an older svolder built (it has no preamp control), so
    /// nothing is sent to it.
    legacy: bool,
    /// The chain plays a curve the sink's rate no longer allows and could not be taken out: it
    /// is watched, not sent to, until the rate plays the curve again or it can be taken out.
    held: bool,
    /// The curve the running chain was applied or restarted with: when the sink's rate changes
    /// its slots go back to these coefficients, but the preamp keeps the value last sent.
    loaded: Option<Correction>,
    /// The curve in the config on disk, which a chain that starts again plays, preamp and all.
    kept: Option<Correction>,
    /// The preamp `loaded` needs at a rate, until it changes.
    cap: Mutex<Option<(u32, f64)>>,
    /// The node in the graph has not been looked at since it was applied, adopted or started
    /// again: another process may have put it there, so what it was started from is not known
    /// and the next look takes its preamp as a bound.
    unvouched: bool,
    /// The preamp the node held when it was first looked at, in dB. It was safe for the slots
    /// the node started from, which a change of rate puts back, so no preamp above it is sent
    /// until an apply says what the chain is.
    node_cap: Option<f64>,
    unavailable: Option<String>,
    /// The slots that moved since the last send.
    dirty: BTreeSet<u8>,
    sent: Sent,
    /// The newest value sent for each port.
    last_values: HashMap<String, f64>,
    /// The port whose value is to be read back, once.
    check: Option<String>,
    verified: bool,
    /// Reads of the checked port that did not match, one after another.
    misses: u32,
    /// The values the chain held when it was last read.
    seen: HashMap<String, f64>,
    /// The chain is to be looked at on the next tick.
    watch_due: bool,
    /// The state file was read again: the first look at the chain checks the curve at the
    /// running rate and sends every slot, whatever the rate.
    resend: bool,
    /// A type change or a point added or removed waits for the end of the drag (a chain whose
    /// slots are of fixed types is rebuilt).
    rebuild: bool,
    ticks: u64,
    /// The ticks on which a send failed, newest last.
    failed: VecDeque<u64>,
    stamp: Option<Stamp>,
    /// The rate the sink was last seen running at.
    known_rate: Option<u32>,
    /// The last [`EqView`], for readers that must not wait on a tick.
    published: watch::Sender<EqView>,
}

fn stamp_of(keep: &Keep) -> Option<Stamp> {
    let Keep::File(path) = keep else { return None };
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len(), meta.ino()))
}

fn refusal(what: impl Into<String>, next: impl Into<String>) -> EqFailure {
    EqFailure {
        what: what.into(),
        next: next.into(),
    }
}

fn kind_word(kind: FilterKind) -> &'static str {
    match kind {
        FilterKind::Peaking => "PK",
        FilterKind::LowShelf => "LSC",
        FilterKind::HighShelf => "HSC",
    }
}

impl EqControl {
    /// A controller over `chain` and `params`, holding what the saved state holds (or the one
    /// point of a flat curve), with the EQ as on as that state says.
    pub fn new(chain: Arc<dyn Chain>, params: Box<dyn Params>, keep: Keep) -> EqControl {
        let state = match &keep {
            Keep::File(path) => HostEqState::load(path)
                .ok()
                .flatten()
                .or_else(|| chain.state()),
            Keep::Memory => chain.state(),
        };
        let stamp = stamp_of(&keep);
        let mut control = EqControl {
            chain,
            params,
            keep,
            points: Points::lone(),
            stated: 0.0,
            preset: None,
            origin: None,
            base: None,
            on: false,
            unknown: false,
            live: true,
            note: None,
            legacy: false,
            held: false,
            loaded: None,
            kept: None,
            cap: Mutex::new(None),
            unvouched: false,
            node_cap: None,
            unavailable: None,
            dirty: BTreeSet::new(),
            sent: Sent::default(),
            last_values: HashMap::new(),
            check: None,
            verified: false,
            misses: 0,
            seen: HashMap::new(),
            watch_due: true,
            resend: false,
            rebuild: false,
            ticks: 0,
            failed: VecDeque::new(),
            stamp,
            known_rate: None,
            published: watch::channel(EqView::default()).0,
        };
        if let Some(state) = state {
            control.adopt(state);
        }
        control.publish();
        control
    }

    /// Take `state` as what the chain is.
    fn adopt(&mut self, state: HostEqState) {
        self.points = Points::from_state(&state);
        self.stated = state.preamp_db;
        self.preset = Some(state.name.clone()).filter(|name| !name.is_empty());
        self.origin = state.origin.clone();
        self.on = state.on;
        self.set_confs(Some(self.points.correction(self.stated)));
        self.base = Some(state);
        // What a command line wrote was worked out at the rate it writes at, whatever the
        // sink runs at now: the first look at the chain checks the curve at the running rate
        // and sends every slot again.
        self.fresh_chain(CONFIG_RATE_HZ);
    }

    /// The config the chain runs and the one on disk are `conf`.
    fn set_confs(&mut self, conf: Option<Correction>) {
        self.loaded = conf.clone();
        self.kept = conf;
        *crate::locked(&self.cap) = None;
    }

    /// The chain was just applied or read, with its config worked out at `rate_hz`: it holds
    /// the config's values.
    fn fresh_chain(&mut self, rate_hz: u32) {
        // The chain is a new node: the id the session kept is not it, and the values it sent
        // are not on it.
        self.params.new_chain();
        self.unknown = false;
        self.legacy = false;
        self.held = false;
        self.dirty.clear();
        self.sent = Sent {
            id: None,
            rate: Some(rate_hz),
        };
        self.last_values.clear();
        self.seen.clear();
        self.check = None;
        self.rebuild = false;
        self.watch_due = true;
        // Whatever node is in the graph at the first look is taken as it is: it may be one
        // another process started since.
        self.unvouched = true;
        self.node_cap = None;
    }

    /// The preamp the points play with.
    ///
    /// It is measured on the filters as they play (held to what the biquads are worked out for,
    /// at the rate they are worked out for), whatever the points hold.
    fn preamp(&self) -> f64 {
        // A chain from an older svolder plays the preamp it was applied with, not one measured
        // here.
        if let (true, Some(base)) = (self.legacy, &self.base) {
            return base.preamp_db;
        }
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        self.wanted_preamp(rate).min(self.cap_at(rate))
    }

    /// The highest preamp the chain may be sent at `rate`: what `loaded` needs, and what an
    /// unvouched chain held when it was first looked at.
    fn cap_at(&self, rate: u32) -> f64 {
        self.conf_preamp(rate)
            .min(self.node_cap.unwrap_or(f64::INFINITY))
    }

    /// The preamp the points as they stand need at `rate`.
    fn wanted_preamp(&self, rate: u32) -> f64 {
        played_preamp(&self.points.correction(self.stated), rate)
    }

    /// The preamp the curve the running chain was loaded with needs at `rate`; 0 when there is
    /// none. No preamp sent is above it: a rate change puts the slots back to that curve while
    /// the preamp keeps the value it was last sent.
    fn conf_preamp(&self, rate: u32) -> f64 {
        if let Some((at, db)) = *crate::locked(&self.cap)
            && at == rate
        {
            return db;
        }
        let db = self
            .loaded
            .as_ref()
            .map_or(0.0, |conf| played_preamp(conf, rate).min(0.0));
        *crate::locked(&self.cap) = Some((rate, db));
        db
    }

    /// Whether the preamp is held below what the points need by the loaded curve.
    fn capped(&self) -> bool {
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        self.wanted_preamp(rate) > self.cap_at(rate) + 1e-9
    }

    fn name(&self) -> String {
        self.preset.clone().unwrap_or_else(|| CUSTOM.to_string())
    }

    /// Whether a drag goes to the chain as it moves.
    fn live_now(&self) -> bool {
        self.live && self.unavailable.is_none() && !self.legacy && !self.held
    }

    /// The computer EQ as a screen shows it.
    pub fn view(&self) -> EqView {
        EqView {
            on: self.on,
            points: self
                .points
                .filters
                .iter()
                .zip(self.points.slots.iter())
                .map(|(filter, slot)| PointView {
                    slot: *slot,
                    kind: kind_word(filter.kind).to_string(),
                    f: filter.f,
                    g: filter.g,
                    q: filter.q,
                })
                .collect(),
            preamp_db: self.preamp(),
            preset: self.preset.clone(),
            live: self.live_now(),
            note: self
                .note
                .clone()
                .or_else(|| self.legacy.then(|| LEGACY_NOTE.to_string())),
            unavailable: self.unavailable.clone().or_else(|| {
                self.chain
                    .unavailable()
                    .map(|failure| format!("computer controls unavailable: {}", failure.what))
            }),
            max: SLOTS as u8,
            running_unknown: self.unknown,
        }
    }

    /// A handle on the newest [`EqView`] that never waits for the controller: it is replaced
    /// after every change, so a reader (the daemon's watchers) is not held up by a tick that is
    /// inside a `pw-dump` or a `pw-cli` write. The owner may keep the controller behind a lock
    /// and serve readers from this.
    pub fn watcher(&self) -> watch::Receiver<EqView> {
        self.published.subscribe()
    }

    fn publish(&self) {
        let view = self.view();
        self.published.send_if_modified(|held| {
            let changed = *held != view;
            if changed {
                *held = view;
            }
            changed
        });
    }

    /// The request that puts the points in front of the sink.
    fn request(&self) -> ApplyRequest {
        ApplyRequest {
            name: self.name(),
            origin: self.origin.clone(),
            correction: self.points.correction(self.stated),
            sink: None,
            slots: self.points.slots.clone(),
            rate_hz: self.known_rate,
        }
    }

    /// Apply the points: the chain is built, or rebuilt, with them.
    async fn apply(&mut self) -> Result<(), EqFailure> {
        // With no chain there is no rate known: the sink's own is read, and the config is
        // written at it. Unread, it is written at the default and the first look at the chain
        // sends every slot again.
        if self.known_rate.is_none() {
            self.known_rate = self.params.sink_rate().await;
        }
        let state = self.chain.apply(self.request()).await?;
        self.set_confs(Some(Correction {
            preamp_db: state.preamp_db,
            filters: state.filters.clone(),
        }));
        self.base = Some(state);
        self.on = true;
        self.unavailable = None;
        self.failed.clear();
        self.fresh_chain(self.known_rate.unwrap_or(CONFIG_RATE_HZ));
        self.stamp = stamp_of(&self.keep);
        // What an earlier refusal said no longer holds; the live-update note still does.
        if self.note.as_deref() != Some(LIVE_NOTE) {
            self.note = None;
        }
        Ok(())
    }

    /// Keep the points: the state file and the config the chain restarts from.
    async fn save(&mut self) -> Result<(), EqFailure> {
        self.save_state()?;
        if self.on {
            // A chain that restarts finds the points as they are; failing to say so changes
            // nothing that plays now.
            let request = self.request();
            // The config is written with the preamp it needs at its own rate, which is what a
            // chain that starts from it plays.
            let rate = request.rate_hz.unwrap_or(CONFIG_RATE_HZ);
            let written = Correction {
                preamp_db: played_preamp(&request.correction, rate),
                filters: request.correction.filters.clone(),
            };
            if self.chain.keep_conf(request).await.is_ok() {
                self.kept = Some(written);
            }
        }
        Ok(())
    }

    /// Keep the points in the state file, when the controller keeps one.
    fn save_state(&mut self) -> Result<(), EqFailure> {
        if let (Keep::File(path), Some(base)) = (&self.keep, &self.base) {
            let state = HostEqState {
                on: self.on,
                name: self.name(),
                origin: self.origin.clone(),
                // A curve the rate no longer plays has no preamp to measure: the one saved while
                // it still played stands.
                preamp_db: match self.held {
                    true => base.preamp_db,
                    false => self.preamp(),
                },
                filters: self.points.filters.clone(),
                slots: self.points.slots.clone(),
                ..base.clone()
            };
            state
                .save(path)
                .map_err(|e| refusal(e, "Check that the directory can be written to"))?;
            self.base = Some(state);
            self.stamp = stamp_of(&self.keep);
        }
        Ok(())
    }

    /// Turn the EQ on or off. On applies whenever no chain runs, whatever the saved state says
    /// (the kept points, or the one point); off saves the points as they stand, removes the
    /// chain and keeps them.
    pub async fn set_on(&mut self, on: bool) -> Result<(), EqFailure> {
        let result = self.set_on_inner(on).await;
        self.publish();
        result
    }

    async fn set_on_inner(&mut self, on: bool) -> Result<(), EqFailure> {
        let running = self.running().await;
        if on {
            return match (self.on, running) {
                (true, Some(true)) => Ok(()),
                // A chain that may be running is taken away first, so two never stack.
                (_, None) => {
                    self.chain.remove().await?;
                    self.apply().await
                }
                _ => self.apply().await,
            };
        }
        // Off removes the chain unless it is known not to run; a remove changes nothing when
        // there is none.
        if !self.on && running == Some(false) {
            return Ok(());
        }
        self.turn_off().await
    }

    /// Take the chain out of the path and keep the points, with the state saying off.
    async fn turn_off(&mut self) -> Result<(), EqFailure> {
        if self.on {
            self.save_state()?;
        }
        self.chain.remove().await?;
        self.params.forget_node();
        self.on = false;
        self.unknown = false;
        self.legacy = false;
        self.held = false;
        self.dirty.clear();
        if let Some(base) = self.base.as_mut() {
            base.on = false;
        }
        let saved = self.save_state();
        // With no chain there is no rate to know: the next one is checked and written at the
        // default, and the first look at it checks the curve at the rate the sink runs at.
        self.known_rate = None;
        self.sent.rate = None;
        self.set_confs(None);
        saved
    }

    /// Whether the chain runs, `None` when that cannot be told. With no state saved nothing was
    /// ever applied, which is known not to run.
    async fn running(&self) -> Option<bool> {
        let status = self.chain.status().await;
        match (status.running, status.state.is_some()) {
            (Some(running), _) => Some(running),
            (None, false) => Some(false),
            (None, true) => None,
        }
    }

    /// Look at whether the chain runs when the saved state says on. One known not to run takes
    /// the EQ as off in the view (nothing is written; the next edit or turn-on applies). One
    /// that cannot be told stays as the state says, and the view says it is unknown. Whether it
    /// was taken off.
    pub async fn sync_running(&mut self) -> bool {
        let mut off = false;
        if self.on {
            match self.running().await {
                Some(false) => {
                    self.on = false;
                    off = true;
                }
                Some(true) => self.unknown = false,
                None => self.unknown = true,
            }
        }
        self.publish();
        off
    }

    /// Apply the kept points when the saved state says on and no chain runs: what a container
    /// does after a restart. Nothing else applies without being asked. Whether it applied.
    pub async fn reapply_if_on(&mut self) -> Result<bool, EqFailure> {
        // Only a chain known not to run is applied again: one that may run is left alone.
        if !self.on || self.running().await != Some(false) {
            return Ok(false);
        }
        let result = self.apply().await.map(|()| true);
        if let Err(failure) = &result {
            self.note = Some(failure.to_string());
        }
        self.publish();
        result
    }

    /// Put a preset's correction on the computer: refused above 16 filters, with the words of
    /// the refusal. On a chain that runs and takes live updates the values go in as they are
    /// and nothing is heard but the change.
    pub async fn load(
        &mut self,
        name: String,
        origin: Option<String>,
        correction: Correction,
    ) -> Result<(), EqFailure> {
        let result = self.load_inner(name, origin, correction).await;
        self.publish();
        result
    }

    async fn load_inner(
        &mut self,
        name: String,
        origin: Option<String>,
        correction: Correction,
    ) -> Result<(), EqFailure> {
        let correction = host_correction(&from_computer(&name, correction))
            .map_err(|why| refusal(why, "Choose a preset of 16 filters or fewer"))?;
        apo::validate(&correction).map_err(|why| {
            refusal(
                format!("the correction cannot be applied: {why}"),
                "Fix the band or the filter it names",
            )
        })?;
        check_playable(&correction, self.sent.rate.unwrap_or(CONFIG_RATE_HZ))?;
        let held = Points::from_state(&HostEqState {
            on: true,
            name: String::new(),
            origin: None,
            target: String::new(),
            route: super::Route::Smart,
            manager: super::Manager::Child,
            previous_default: None,
            preamp_db: 0.0,
            filters: correction.filters.clone(),
            slots: Vec::new(),
        });
        let before = (
            std::mem::replace(&mut self.points, held),
            self.stated,
            self.preset.clone(),
            self.origin.clone(),
        );
        self.stated = correction.preamp_db;
        self.preset = Some(name);
        self.origin = origin;
        let result = self.put().await;
        if result.is_err() {
            (self.points, self.stated, self.preset, self.origin) = before;
        }
        result
    }

    /// Put the points as they stand on the chain, live when it can be and by applying when not,
    /// and keep them.
    async fn put(&mut self) -> Result<(), EqFailure> {
        // A curve softer than the applied one goes in by applying, so its own preamp plays.
        let live = self.on && self.live_now() && SHAPE == SlotShape::Raw && !self.capped();
        if live {
            self.dirty.extend(1..=SLOTS as u8);
            if self.send_dirty().await {
                return self.save().await;
            }
        }
        self.apply().await?;
        self.save().await
    }

    /// Change the points. An edit while the EQ is off turns it on first.
    pub async fn edit(&mut self, edit: Edit) -> Result<(), EqFailure> {
        let result = self.edit_inner(edit).await;
        self.publish();
        result
    }

    async fn edit_inner(&mut self, edit: Edit) -> Result<(), EqFailure> {
        // An edit that cannot be made (a point that is gone, a value that is no number, a
        // seventeenth point) or that makes a curve the chain cannot play is refused before
        // anything turns on.
        let target = match &edit {
            Edit::Set {
                index,
                slot,
                f,
                g,
                q,
                ..
            } => {
                if [f, g, q]
                    .into_iter()
                    .flatten()
                    .any(|value| !value.is_finite())
                {
                    return Err(refuse_edit("a point's values are numbers".to_string()));
                }
                Some(self.locate(*index, *slot)?)
            }
            Edit::Remove { index, slot } => Some(self.locate(*index, *slot)?),
            Edit::Add(_) => None,
        };
        let target = match target {
            Some(index) => Some((index, self.slot_of(index)?)),
            None => None,
        };
        let mut edited = self.points.clone();
        let (slot, done, rebuild) = Self::make(&mut edited, edit, target)?;
        self.playable(&edited)?;
        if !self.on {
            // Turned on with the edited points, so an edit that mends a curve the chain
            // cannot play turns it on.
            let held = std::mem::replace(&mut self.points, edited.clone());
            if let Err(failure) = self.set_on_inner(true).await {
                self.points = held;
                return Err(failure);
            }
        }
        self.points = edited;
        if done {
            // The table lists the points by frequency; a drag never re-sorts under the finger.
            self.points.sort();
        }
        self.stated = 0.0;
        self.preset = Some(CUSTOM.to_string());
        self.origin = None;
        self.rebuild |= rebuild;
        if self.live_now() && !self.rebuild {
            self.dirty.insert(slot);
        } else {
            self.dirty.clear();
        }
        if !done {
            return Ok(());
        }
        if self.live_now() && !self.rebuild && self.send_dirty().await {
            return self.save().await;
        }
        self.apply().await?;
        self.save().await
    }

    /// `edit` made on `points`: the slot it touches, whether it is final (not a drag's
    /// update) and whether it needs the chain rebuilt.
    fn make(
        points: &mut Points,
        edit: Edit,
        target: Option<(usize, u8)>,
    ) -> Result<(u8, bool, bool), EqFailure> {
        Ok(match edit {
            Edit::Set {
                kind,
                f,
                g,
                q,
                done,
                ..
            } => {
                let (index, slot) = target.expect("a set has a target");
                let change = points.set(index, kind, f, g, q).map_err(refuse_edit)?;
                (
                    slot,
                    done,
                    SHAPE == SlotShape::Typed && change == Change::Kind,
                )
            }
            Edit::Add(filter) => {
                let index = points.add(filter).map_err(refuse_edit)?;
                (points.slots[index], true, SHAPE == SlotShape::Typed)
            }
            Edit::Remove { .. } => {
                let (index, _) = target.expect("a remove has a target");
                let slot = points.remove(index).map_err(refuse_edit)?;
                (slot, true, SHAPE == SlotShape::Typed)
            }
        })
    }

    /// Whether the chain can play `points` at the rate it runs at: refused in the words of
    /// [`check_playable`] when not.
    fn playable(&self, points: &Points) -> Result<(), EqFailure> {
        check_playable(
            &points.correction(0.0),
            self.sent.rate.unwrap_or(CONFIG_RATE_HZ),
        )
    }

    /// The index of the point a request means: the one in `slot` when the sender gave a slot,
    /// else `index`. A slot no point holds is refused, so a request made against a table that
    /// has changed since never lands on another point.
    fn locate(&self, index: usize, slot: Option<u8>) -> Result<usize, EqFailure> {
        let Some(slot) = slot else { return Ok(index) };
        self.points
            .slots
            .iter()
            .position(|held| *held == slot)
            .ok_or_else(|| {
                refusal(
                    format!("the point in slot {slot} has moved or is gone"),
                    "Read the points again",
                )
            })
    }

    /// The slot of the point at `index`.
    fn slot_of(&self, index: usize) -> Result<u8, EqFailure> {
        self.points
            .slots
            .get(index)
            .copied()
            .ok_or_else(|| refuse_edit(format!("there is no point {}", index + 1)))
    }

    /// Send every slot that moved, and the preamp, in one call. Whether they went (or there was
    /// nothing to send).
    async fn send_dirty(&mut self) -> bool {
        if self.send_dirty_once().await {
            return true;
        }
        // A node found under another id than the one looked at (the chain was replaced since,
        // or the kept node was dropped and found again) gets no line. It is looked at, and the
        // line goes once more.
        match self.params.node().await {
            Some(node) if self.sent.id != Some(node.id) => self.send_dirty_once().await,
            _ => false,
        }
    }

    async fn send_dirty_once(&mut self) -> bool {
        if self.dirty.is_empty() {
            return true;
        }
        // Nothing goes to a node that has not been looked at: it may be one another process
        // started, and what it was started from bounds the preamp (see `watch`).
        let Some(node) = self.params.node().await else {
            return false;
        };
        if self.sent.id != Some(node.id) {
            self.watch().await;
            if !self.on || !self.live_now() || self.dirty.is_empty() {
                return false;
            }
        }
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        // Every slot goes with every update, not only the ones that moved: a change of the
        // sink's rate puts the chain's slots back to its config without telling anyone, and an
        // update that carried one slot would leave the rest as that config has them, under a
        // preamp worked out for the points as they stand. A line that carries them all leaves
        // nothing of the old chain on it.
        let mut params = Vec::new();
        for slot in 1..=SLOTS as u8 {
            let filter = self
                .points
                .slots
                .iter()
                .position(|held| *held == slot)
                .map(|index| &self.points.filters[index]);
            params.extend(slot_params(slot, filter, rate));
        }
        // A preamp that falls goes before the slots and one that rises after them, so an update
        // that is not applied in one piece never plays a boost the preamp has not caught up with.
        let preamp = preamp_params(self.preamp());
        let applied = self.base.as_ref().map(|base| preamp_params(base.preamp_db));
        let rises = preamp.first().is_some_and(|(name, db)| {
            let applied = applied
                .as_ref()
                .and_then(|params| params.iter().find(|(key, _)| key == name))
                .map(|(_, value)| value);
            self.last_values
                .get(name)
                .or_else(|| self.seen.get(name))
                .or(applied)
                .is_some_and(|held| db > held)
        });
        match rises {
            true => params.extend(preamp),
            false => params.splice(0..0, preamp).for_each(drop),
        }
        let Some(vouched) = self.sent.id else {
            return false;
        };
        if !self.params.send(vouched, &params).await {
            return false;
        }
        self.dirty.clear();
        for (name, value) in &params {
            self.last_values.insert(name.clone(), *value);
        }
        if !self.verified && self.check.is_none() {
            self.check = Some(self.key_to_check(&params));
            self.watch_due = true;
        }
        true
    }

    /// The port of `params` to read back: the one that moved furthest from what the chain was
    /// last seen holding, so a value that did not take cannot pass for one that did.
    fn key_to_check(&self, params: &[(String, f64)]) -> String {
        let moved = |(name, value): &&(String, f64)| match self.seen.get(name) {
            Some(held) => (held - value).abs(),
            None => 0.0,
        };
        params
            .iter()
            .max_by(|a, b| moved(a).total_cmp(&moved(b)))
            .map(|(name, _)| name.clone())
            .unwrap_or_default()
    }

    /// Look at the chain: a restart of it or a change of the sink's rate sends every slot
    /// again, and the first live value is read back.
    async fn watch(&mut self) {
        self.watch_due = false;
        self.params.forget_node();
        let Some(node) = self.params.node().await else {
            return;
        };
        let restarted = self.sent.id.is_some_and(|id| id != node.id);
        let rate_changed = node
            .rate_hz
            .is_some_and(|rate| self.sent.rate.is_some_and(|sent| sent != rate));
        self.seen = node.params.clone();
        if restarted {
            self.unvouched = true;
        }
        if node.rate_hz.is_some() {
            self.known_rate = node.rate_hz;
        }
        if !node.has_preamp_control() {
            // A chain an older svolder built plays on, as it was applied: nothing is sent to
            // it and nothing is written; turning the EQ off and on replaces it. It is looked
            // at again: a node is listed a moment before its controls are.
            self.legacy = true;
            self.dirty.clear();
            return;
        }
        if self.legacy {
            self.legacy = false;
            self.dirty.extend(1..=SLOTS as u8);
        }
        if std::mem::take(&mut self.unvouched) {
            self.node_cap = held_preamp_db(&node.params);
        }
        let resend = std::mem::take(&mut self.resend);
        if restarted || rate_changed || self.held || resend {
            if restarted {
                // A chain that started again plays the config on disk, and none of what was
                // sent to the old one.
                self.params.new_chain();
                self.loaded = self.kept.clone();
                *crate::locked(&self.cap) = None;
            }
            let before = self.sent.rate;
            self.sent = Sent {
                id: Some(node.id),
                rate: node.rate_hz.or(self.sent.rate),
            };
            // The curve was checked at the old rate; at this one it may not be playable (a
            // coefficient past what the chain holds is clamped to another filter), so nothing
            // is sent until it is.
            if let Err(failure) = self.playable(&self.points) {
                let rate = self.sent.rate;
                // The state keeps the preamp of the curve as it was playable.
                self.sent.rate = before;
                let off = self.turn_off().await;
                let at = rate.map_or_else(String::new, |rate| format!(" at {rate} Hz"));
                self.note = Some(match off {
                    Ok(()) => format!("computer EQ turned off{at}: {failure}"),
                    Err(why) => {
                        self.sent.rate = rate;
                        self.held = true;
                        format!("computer EQ could not be turned off{at}: {failure} ({why})")
                    }
                });
                return;
            }
            if self.held {
                self.held = false;
                if self.note.as_deref() != Some(LIVE_NOTE) {
                    self.note = None;
                }
            }
            self.dirty.extend(1..=SLOTS as u8);
            self.watch_due = self.check.is_some();
            return;
        }
        self.sent.id = Some(node.id);
        if let Some(rate) = node.rate_hz {
            self.sent.rate = Some(rate);
        }
        if let Some(key) = self.check.take() {
            let wanted = self.last_values.get(&key).copied();
            let held = node.params.get(&key).copied();
            match (wanted, held) {
                (Some(wanted), Some(held))
                    if (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6 =>
                {
                    self.verified = true;
                    self.misses = 0;
                }
                _ if self.misses + 1 < READ_BACK_MISSES => {
                    // pw-cli applies a line a moment after it is written: look again.
                    self.misses += 1;
                    self.check = Some(key);
                    self.watch_due = true;
                }
                _ => {
                    self.live = false;
                    self.dirty.clear();
                    self.note = Some(LIVE_NOTE.to_string());
                }
            }
        }
    }

    /// One step of the owner's clock, which must tick every 50 ms (see `WATCH_EVERY`): send what moved, at most once for each point, and
    /// now and then look at the chain.
    pub async fn tick(&mut self) {
        self.step().await;
        self.publish();
    }

    async fn step(&mut self) {
        self.ticks += 1;
        // A chain that is taken for an older one or that waits for its rate is looked at, not
        // sent to.
        if !self.on || !self.live || self.unavailable.is_some() {
            return;
        }
        if self.watch_due || self.ticks.is_multiple_of(WATCH_EVERY) {
            self.watch().await;
            self.params.settle(self.sent.id).await;
        }
        if !self.on || !self.live_now() {
            return;
        }
        if self.dirty.is_empty() || self.send_dirty().await {
            return;
        }
        self.failed.push_back(self.ticks);
        while self
            .failed
            .front()
            .is_some_and(|tick| tick + FAIL_WINDOW < self.ticks)
        {
            self.failed.pop_front();
        }
        if self.failed.len() >= FAIL_LIMIT {
            self.params.forget_node();
            let why = match self.params.node().await {
                Some(_) => "pw-cli would not take the changes",
                None => "the computer EQ is not in PipeWire's graph",
            };
            self.unavailable = Some(format!("computer controls unavailable: {why}"));
            self.dirty.clear();
            self.failed.clear();
        }
    }

    /// Read the state file again when something else (the command line) changed it. Whether
    /// the points changed. Nothing is read while a drag has values to send.
    pub fn reload_if_changed(&mut self) -> bool {
        let changed = self.reload();
        self.publish();
        changed
    }

    fn reload(&mut self) -> bool {
        let Keep::File(path) = &self.keep else {
            return false;
        };
        let now = stamp_of(&self.keep);
        if now == self.stamp || !self.dirty.is_empty() {
            return false;
        }
        self.stamp = now;
        match HostEqState::load(path) {
            Ok(Some(state)) => {
                self.adopt(state);
                self.resend = true;
                true
            }
            _ => false,
        }
    }
}

/// The preamp in dB a node holds, from its `Mult` ports (the lower of the two channels); `None`
/// when they say nothing (a port that is not above 0).
fn held_preamp_db(params: &HashMap<String, f64>) -> Option<f64> {
    preamp_params(0.0)
        .iter()
        .filter_map(|(name, _)| params.get(name).copied())
        .filter(|mult| *mult > 0.0)
        .map(|mult| (20.0 * mult.log10()).max(-120.0))
        .min_by(f64::total_cmp)
}

fn refuse_edit(why: String) -> EqFailure {
    refusal(why, "Edit a point that is there")
}

#[cfg(test)]
mod tests;
