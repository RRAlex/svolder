// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The readings the app has heard: how the backend's updates are folded in, how often each
//! function reports, the bars that ease to a changed value, and the faults standing.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use svolder_ipc::ValueObject;
use svolder_profiles::{Control, ControlKind};
use svolder_proto::FunctionId;

use crate::backend::Update;

use crate::tui::boxes::presets::Loading;
use crate::tui::widgets::graph::{self, Histories, Link};

use super::{App, Pending, Screen};

/// Weight the newest interval carries in an update-rate estimate.
const RATE_ALPHA: f64 = 0.3;
/// How many estimated intervals of silence a rate holds for before it starts to fall.
const RATE_DECAY_AFTER: f64 = 3.0;

/// How long a bar takes to ease from the value a function read to the one it reads now, in
/// milliseconds.
const EASING_MS: u64 = 200;

/// One reading, with when it arrived and how often it has been arriving.
#[derive(Debug, Clone)]
pub struct Reading {
    /// The reading itself, as the backend handed it over.
    pub value: ValueObject,
    /// When this reading arrived.
    pub received: Instant,
    /// How often this function has been reporting.
    pub rate: RateEstimate,
}

/// How often one function is reporting, as an exponentially weighted interval.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RateEstimate {
    last: Option<Instant>,
    interval: Option<f64>,
}

/// One value on its way from what a function read to what it reads now.
///
/// An easing is drawing, never a fact: the number beside a bar is always the value the device
/// confirmed, and only the bar's own length is on its way there.
#[derive(Debug, Clone, Copy)]
pub struct Easing {
    /// The raw value the bar is coming from.
    pub from: i64,
    /// The raw value it is going to, which is the one the device confirmed.
    pub to: i64,
    /// When it set off.
    pub started: Instant,
}

/// What the device has said and what is still on its way to it: the readings, their histories
/// and the writes the device has not answered.
pub struct Readings {
    /// Every reading heard, by function address.
    pub values: BTreeMap<FunctionId, Reading>,
    /// The history of every numeric reading and of the host's levels, for the graphs.
    pub histories: Histories,
    /// The changes of the device's link, in wall-clock time, for the connection strip.
    pub link: Link,
    /// The values still easing to what the device confirmed, by function address.
    pub(super) easings: BTreeMap<FunctionId, Easing>,
    /// The recall whose writes are still on their way to the device, while one is.
    pub loading: Option<Loading>,
    /// The tone-control steps asked for whose writes the device has not all answered yet, by
    /// function: what the band keys, `0`, the edit line and a recall plan from in place of the
    /// reading ([`App::planned_raw`]), so two presses before the first echo move two steps, a key
    /// held down moves at its own rate rather than the device's, and a key pressed while a recall
    /// is being answered follows on from the recall. A band edit notes each of its writes here
    /// ([`App::band_edit`]), a recall each of its own ([`App::note_writes`]); ended by a reading of
    /// the last function asked for (or that answers the last outstanding write), a refresh, a
    /// failed write, or [`ANSWER_LIFETIME`](super::ANSWER_LIFETIME) passing with no such reading.
    pub pending: BTreeMap<FunctionId, Pending>,
    /// The functions a slot recall changes, from the recall the TUI sent until each one reports
    /// again: the readings they hold are from before it, so a built-in setting is planned as
    /// though they were unread ([`device_presets`](crate::tui::boxes::device_presets)). Ended
    /// per function by any report of it, and altogether by a refresh.
    pub unreported: BTreeSet<FunctionId>,
    /// The last value a level bar's write asked of each function, which a raise of the Volume is
    /// judged against beside the last reading ([`glide`](super::glide)): a lowering the
    /// device has not confirmed is not undone by the older, louder reading.
    pub(crate) sent: BTreeMap<FunctionId, i64>,
}

impl App {
    /// Fold in one thing the backend reported.
    ///
    /// A status replaces whatever the function said before and stamps the moment it arrived; a
    /// reading whose id names no function address at all is dropped. A link that went down
    /// leaves every value shown as last-known ([`App::rows`] reports it stale), and one that
    /// came back clears that.
    ///
    /// A value the device confirms as different from the one it confirmed before sets its bar
    /// gliding and marks its row as changed. A first value, an echo of the value already
    /// shown, and a write the device never answered all leave both alone: nothing is drawn
    /// moving that the device has not said has moved.
    ///
    /// A reading of the function a recall wrote last that ends the wait on it (see
    /// [`Readings::pending`]) is the device confirming the whole of the recall, which ends the
    /// `loading` the tone-control screen's header says; an echo of an earlier write to that
    /// function, from before the recall, does not.
    pub fn apply(&mut self, update: Update, now: Instant) {
        self.now = now;
        match update {
            Update::Status(value) => self.status(value, now, true),
            Update::Connection { connected } => {
                self.frame_flags.reconnected |= connected && !self.session.connected;
                // A link that is back answers the step the failure that took it away named,
                // so the advice line stops offering it.
                if connected && !self.session.connected {
                    self.notices.advice = None;
                }
                self.link_changed(connected);
                if !connected {
                    self.pointer.climb = None;
                }
            }
            // Another front end's recall: the header names it from now on, and the session's own
            // `loading` and `*` work on top of it as they do on one of its own.
            Update::Recalled(name) => self.presets.loaded_preset = Some(name),
            // The daemon's view of the host, which `[tui] host_audio = false` overrules.
            Update::Host(view) => {
                if self.host_shown() {
                    self.host.fold_daemon(*view, now);
                }
            }
        }
    }

    /// Sets whether the link is up and records the change in the strip's history at the wall
    /// clock's now. Every path that changes the link goes through here, so the header and the
    /// strip never disagree.
    pub fn link_changed(&mut self, connected: bool) {
        self.session.connected = connected;
        self.readings.link.record(graph::wall_ms(), connected);
    }

    /// Fold in one reading as of `now`, as [`App::apply`] says a status is folded in; `live`
    /// for one the device sent on its feed, and not for one of the values a refresh read.
    /// Only a live reading of a scaled meter, while the meters are live, says the meters are
    /// streaming.
    fn status(&mut self, value: ValueObject, now: Instant, live: bool) {
        let Ok(id) = value.id.parse::<FunctionId>() else {
            return;
        };
        self.readings.unreported.remove(&id);
        if let Entry::Occupied(mut pending) = self.readings.pending.entry(id) {
            let answered = pending.get_mut().answered(value.raw.as_i64());
            if answered {
                pending.remove();
            }
        }
        // The device now reads the last value sent, so the reading alone is the base a raise is
        // judged from again: a change made elsewhere afterwards (a key, the wheel, the knob) is
        // not held back by a value the device has already moved past.
        if value
            .raw
            .as_i64()
            .is_some_and(|raw| self.readings.sent.get(&id) == Some(&raw))
        {
            self.readings.sent.remove(&id);
        }
        if self
            .readings
            .loading
            .as_ref()
            .is_some_and(|loading| loading.last == id && !self.readings.pending.contains_key(&id))
        {
            self.readings.loading = None;
        }
        if self.fault_goes_active(id, &value) {
            self.frame_flags.pending_flash = Some(now);
        }
        let was_fault = self.fault_standing(id);
        let focused = if self.screen == Screen::Telemetry {
            self.telemetry_focus()
        } else {
            None
        };
        let moved = match self.readings.values.entry(id) {
            Entry::Occupied(mut slot) => {
                let reading = slot.get_mut();
                let before = reading.raw_i64();
                reading.rate.sample(now);
                reading.value = value;
                reading.received = now;
                match (before, reading.raw_i64()) {
                    (Some(from), Some(to)) if from != to => Some((from, to)),
                    _ => None,
                }
            }
            Entry::Vacant(slot) => {
                // A reading slipping in above the focused one moves it down a row: the focus
                // goes with it.
                if self.screen == Screen::Telemetry && focused.is_some_and(|f| *slot.key() < f) {
                    self.selected += 1;
                }
                let mut rate = RateEstimate::default();
                rate.sample(now);
                slot.insert(Reading {
                    value,
                    received: now,
                    rate,
                });
                None
            }
        };
        if was_fault && self.faults().is_empty() {
            self.notices.fault_cleared_at = Some(now);
        }
        if let Some((from, to)) = moved {
            self.changed(id, from, to, now);
        }
        let number = match &self.readings.values[&id].value.raw {
            serde_json::Value::Bool(on) => Some(f64::from(u8::from(*on))),
            raw => raw.as_f64(),
        };
        if let Some(number) = number {
            self.readings
                .histories
                .record(&self.profile, id, now, number);
        }
        if self.is_meter(id) {
            self.metered(id, now);
            let scaled = self
                .profile
                .control_by_id(id)
                .is_some_and(|control| self.placed(control).is_some());
            if live && scaled && self.meters_live() {
                self.meters.meters_heard = Some(now);
            }
        }
        if self
            .profile
            .eq
            .as_ref()
            .is_some_and(|eq| id.first() == eq.id)
        {
            self.tone_read();
        }
    }

    /// Forget every reading and take `values` in their place, as of `now`.
    ///
    /// This is what a refresh folds in: the values the device holds now, rather than the ones
    /// this app happened to hear. A function that has stopped reporting altogether therefore
    /// leaves the screen rather than lingering, and the rates start again with the new
    /// readings.
    ///
    /// A fault the device was already reporting is not a fault going active: it arrives again
    /// with everything else, and the header flashes only for one that was not on before, so
    /// pressing `r` while a fault stands leaves the header still.
    ///
    /// A bar still gliding towards a value from before the refresh is dropped along with that
    /// value, so it draws at the fresh reading at once rather than finishing an easing towards a
    /// target the refresh has already overtaken.
    pub fn replace_all(&mut self, values: Vec<ValueObject>, now: Instant) {
        // A refresh reads the device afresh, so nothing is stepped from what was asked before it.
        self.readings.pending.clear();
        self.readings.unreported.clear();
        let before: Vec<FunctionId> = self.faults().iter().map(|control| control.id).collect();
        let had_faults = !before.is_empty();
        let flash = self.frame_flags.pending_flash;
        let focused = self.telemetry_focus();
        self.readings.values.clear();
        self.readings.easings.clear();
        self.now = now;
        for value in values {
            self.status(value, now, false);
        }
        let fresh = self
            .faults()
            .iter()
            .any(|control| !before.contains(&control.id));
        if !fresh {
            self.frame_flags.pending_flash = flash;
        }
        if had_faults && self.faults().is_empty() {
            self.notices.fault_cleared_at = Some(now);
        }
        if self.screen == Screen::Telemetry {
            let row = focused.and_then(|id| self.readings.values.keys().position(|had| *had == id));
            self.selected = row.unwrap_or(0);
        }
        // A tone control the new readings say nothing about has no curve left to warn about.
        self.tone_read();
    }

    /// Note that `id` now reads `to` where it read `from`: its bar glides there, and its row
    /// is one the next frame glows.
    ///
    /// A meter is left out of the easing: it reports several times a second, so its bar follows
    /// the device directly rather than chasing it a fifth of a second behind and holding the
    /// loop at its fast tick for as long as the device is streaming.
    fn changed(&mut self, id: FunctionId, from: i64, to: i64, now: Instant) {
        self.frame_flags.changed_rows.push(id);
        if !self.eased || self.is_meter(id) {
            return;
        }
        // A value that changes again mid-easing sets off from where the bar is drawn now
        // rather than jumping back to where the last easing started.
        let from = self
            .readings
            .easings
            .get(&id)
            .map_or(from, |easing| easing.at(now));
        self.readings.easings.insert(
            id,
            Easing {
                from,
                to,
                started: now,
            },
        );
    }

    /// The raw value one function's bar is drawn at as of `now`: the value gliding towards
    /// what the device confirmed, or that confirmed value itself once it has landed or where
    /// nothing is gliding. `None` for a function nothing has reported a number for, and for a
    /// meter, whose bar is drawn at the level the device last reported.
    ///
    /// Only bars read this. The text beside them is the confirmed value, so a frame never
    /// says a number the device has not sent.
    pub fn eased_raw(&self, id: FunctionId, now: Instant) -> Option<i64> {
        if self.is_meter(id) {
            return None;
        }
        let confirmed = self.readings.values.get(&id)?.raw_i64()?;
        Some(
            self.readings
                .easings
                .get(&id)
                .map_or(confirmed, |g| g.at(now)),
        )
    }

    /// Whether any bar is still on its way to the value behind it as of `now`, which is what
    /// the loop redraws fast for.
    pub fn is_easing(&self, now: Instant) -> bool {
        self.readings
            .easings
            .values()
            .any(|easing| !easing.landed(now))
    }

    /// Whether `id` is a fault control that reads active now.
    fn fault_standing(&self, id: FunctionId) -> bool {
        self.profile
            .control_by_id(id)
            .is_some_and(|control| control.kind == ControlKind::Fault)
            && self
                .readings
                .values
                .get(&id)
                .is_some_and(Reading::is_active)
    }

    /// Whether this reading turns a fault on that was not on before.
    fn fault_goes_active(&self, id: FunctionId, value: &ValueObject) -> bool {
        if value.raw != serde_json::Value::Bool(true) {
            return false;
        }
        let is_fault = self
            .profile
            .control_by_id(id)
            .is_some_and(|control| control.kind == ControlKind::Fault);
        is_fault
            && !self
                .readings
                .values
                .get(&id)
                .is_some_and(Reading::is_active)
    }

    /// The fault controls the device is reporting now, ordered by function address.
    pub fn faults(&self) -> Vec<&Control> {
        let mut faults: Vec<&Control> = self
            .profile
            .controls
            .iter()
            .filter(|control| control.kind == ControlKind::Fault)
            .filter(|control| {
                self.readings
                    .values
                    .get(&control.id)
                    .is_some_and(Reading::is_active)
            })
            .collect();
        faults.sort_unstable_by_key(|control| control.id);
        faults
    }

    /// The telemetry screen's rows: every reading heard, in function-address order.
    pub fn telemetry_rows(&self) -> Vec<(FunctionId, &Reading)> {
        self.readings
            .values
            .iter()
            .map(|(id, reading)| (*id, reading))
            .collect()
    }

    /// The raw value one function reads now, for the controls that carry a number.
    pub(crate) fn raw_of(&self, id: FunctionId) -> Option<i64> {
        self.readings.values.get(&id)?.raw_i64()
    }

    /// How often one function is reporting, as of the last tick.
    pub fn rate_per_s(&self, id: FunctionId) -> Option<f64> {
        self.readings.values.get(&id)?.rate.rate_per_s(self.now)
    }
}

#[cfg(test)]
impl App {
    /// Tick to the first instant at which every easing has landed, for a case about the settled
    /// screen.
    pub(crate) fn settle(&mut self) {
        let landed = self
            .readings
            .easings
            .values()
            .map(|easing| easing.started + Duration::from_millis(EASING_MS))
            .max()
            .unwrap_or(self.now)
            .max(self.now);
        self.tick(landed);
    }
}

impl Easing {
    /// Where the bar is as of `now`: the quadratic ease out over [`EASING_MS`], which sets off
    /// quickly and settles onto the value the device confirmed.
    fn at(&self, now: Instant) -> i64 {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f64();
        let along = (elapsed * 1_000.0 / EASING_MS as f64).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - along) * (1.0 - along);
        self.from + (self.to.saturating_sub(self.from) as f64 * eased).round() as i64
    }

    /// Whether the bar has reached the value behind it by `now`.
    pub(super) fn landed(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= Duration::from_millis(EASING_MS)
    }
}

impl Reading {
    /// The raw value as a whole number, for the controls that carry one.
    pub fn raw_i64(&self) -> Option<i64> {
        self.value.raw.as_i64()
    }

    /// The raw value as a whole number, a boolean as 0 or 1.
    pub fn raw_or_flag(&self) -> Option<i64> {
        self.value
            .raw
            .as_i64()
            .or_else(|| self.value.raw.as_bool().map(i64::from))
    }

    /// Whether a toggle or a fault reads true.
    pub fn is_active(&self) -> bool {
        self.value.raw == serde_json::Value::Bool(true)
    }
}

impl RateEstimate {
    /// Fold in a reading that arrived at `now`.
    fn sample(&mut self, now: Instant) {
        if let Some(last) = self.last {
            let gap = now.saturating_duration_since(last).as_secs_f64();
            self.interval = Some(match self.interval {
                Some(interval) => RATE_ALPHA * gap + (1.0 - RATE_ALPHA) * interval,
                None => gap,
            });
        }
        self.last = Some(now);
    }

    /// Readings per second as of `now`, or `None` until two have arrived.
    ///
    /// Silence longer than [`RATE_DECAY_AFTER`] estimated intervals is reported as the rate
    /// that silence itself implies, so a stream that stopped falls towards zero rather than
    /// reading as fast as it once was.
    fn rate_per_s(&self, now: Instant) -> Option<f64> {
        let (last, interval) = (self.last?, self.interval?);
        if interval <= 0.0 {
            return None;
        }
        let silence = now.saturating_duration_since(last).as_secs_f64();
        if silence > RATE_DECAY_AFTER * interval {
            Some(1.0 / silence)
        } else {
            Some(1.0 / interval)
        }
    }
}
