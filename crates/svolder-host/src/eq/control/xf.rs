// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The crossfeed in [`EqControl`]: its settings, its lifecycle and its resets.
//!
//! The crossfeed is one stage inside the computer EQ's one filter chain (see
//! [`live`](super::super::live)), so the controller that owns the chain owns it and the daemon
//! and a TUI with no daemon behave alike. Whether a chain runs and whether the EQ is on are two
//! things here: `runs` is a chain at all, [`EqControl::eq_on`] the EQ's own switch.
//!
//! - **On and off.** Crossfeed on with no chain applies a chain with the EQ flat (`flat`: the
//!   slots the identity, the preamp 0 dB) and sends the stage. The EQ off while crossfeed is on
//!   makes the chain flat live, the EQ on while it is flat sends the slots live, and both off
//!   removes the chain: none of the first two applies or removes anything.
//! - **Level.** The preamp is the EQ's less the stage's worst gain at any of the usual rates
//!   (`config_stage_cost_db`, rounded up to 0.1 dB): a line sent between a change of the sink's
//!   rate and the next look plays the stage worked out at the old rate, its delay in other whole
//!   samples. It is re-folded whenever the model, the amount, the crossover or the delay move
//!   it. One update lowers the preamp before the stage turns on or grows, and raises it after the
//!   stage turns off or shrinks, so no whole update plays above unity (PipeWire applies a live line
//!   port by port, so a half-applied line can still play above it: a documented known issue).
//! - **Resets to off**, each with a note and a reason in the view: the owner starts and finds
//!   it on ([`OffReason::Restart`]), the output the stage was turned on for is not the output any
//!   more (the 1 s look reads it from the same `pw-dump` as the node), the chain restarted (a new
//!   node id), and a graceful stop ([`EqControl::shutdown`]). A chain that runs for crossfeed
//!   alone is removed by every reset; any other has the stage turned off live, or is applied
//!   again without it when its config may hold the stage or live updates fail. A rate change is
//!   not a reset: the stage is sent again at the new rate (or turned off when it cannot play
//!   there).
//! - **Edges.** Live updates unavailable ([`LIVE_NOTE`]): a crossfeed change
//!   applies the chain with the stage in its config and `crossfeed_on` in the state, so the start
//!   reset finds it. A chain from before the stage (no crossfeed controls) is rebuilt once when
//!   crossfeed is turned on. A chain from an older svolder, or one held for a rate, refuses
//!   crossfeed in words.

use super::*;
use crate::eq::crossfeed::{CrossfeedChange, held};
use crate::eq::manage::config_stage_cost_db;
use crate::eq::speakers::OffReason as SpeakersOff;

/// What the view says when the stage was found on at the owner's start.
const RESTART_NOTE: &str = "crossfeed turned off: it is off after a restart";

impl EqControl {
    /// The rate the stage is worked out for.
    pub(super) fn rate(&self) -> u32 {
        self.sent.rate.or(self.known_rate).unwrap_or(CONFIG_RATE_HZ)
    }

    /// The rate a stage turned on now runs at: the sink's own when no chain runs (the chain
    /// started for it is built at that rate), else the rate the stage is worked out for.
    pub(super) async fn stage_start_rate(&mut self) -> u32 {
        if !self.runs
            && let Some(rate) = self.params.sink_rate().await
        {
            return rate;
        }
        self.rate()
    }

    /// Whether a chain from the stage's own point of view may be playing the stage: it is on,
    /// or a reset that has not reached the chain yet is owed.
    fn stage_may_play(&self) -> bool {
        self.xf_on || self.owed.as_ref().is_some_and(|owed| owed.plays)
    }

    /// What the stage takes off the preamp ([`config_stage_cost_db`]: the stage worked out at the
    /// rate last looked at, played at any usual rate, since the sink may have moved since and a
    /// line may still carry it), and never less than the stage the last line put on the node costs
    /// at the rate it was worked out at; 0 when no stage plays. The newest answer for the
    /// settings is kept: the view asks for it on every tick.
    pub(super) fn stage_gain(&self) -> f64 {
        if self.legacy {
            return 0.0;
        }
        // The stage the last line put on the node, worked out at the rate of that line: it plays
        // until a line carries another, whatever the rate or the settings say since.
        let sent = self
            .stage_sent
            .as_ref()
            .map_or(0.0, |(settings, at)| config_stage_cost_db(settings, *at));
        if !self.stage_may_play() {
            return sent;
        }
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        let mut cache = crate::locked(&self.gain_cache);
        if let Some((settings, at, db)) = cache.as_ref()
            && *at == rate
            && *settings == self.xf
        {
            return db.max(sent);
        }
        let db = config_stage_cost_db(&self.xf, rate);
        *cache = Some((self.xf.clone(), rate, db));
        db.max(sent)
    }

    /// Whether a config may hold the stage on: the one on disk (`xf_conf`), or the one the
    /// running chain was started from, which a change of the sink's rate puts back. Taking the
    /// stage out of such a chain is an apply: a live send leaves it in the config.
    pub(super) fn conf_holds_stage(&self) -> bool {
        self.xf_conf || self.loaded_xf.is_some()
    }

    /// Whether the stage is sent to a running chain, as the EQ's points are.
    pub(super) fn stage_by_live(&self) -> bool {
        self.live && self.unavailable.is_none() && !self.legacy && !self.held
    }

    /// Whether the config the chain is applied with holds the stage on: when live updates
    /// cannot carry it (or one just failed), so the config does and the state says so.
    pub(super) fn stage_in_conf(&self) -> bool {
        self.xf_on && (self.force_conf || !self.stage_by_live())
    }

    /// Take the crossfeed from `state`: its settings, and a stage that is never on from a file.
    /// A state that says the stage plays, or that the chain is flat, was left by a run that did
    /// not stop gracefully; a change made from outside while the stage played took it off.
    pub(super) fn adopt_crossfeed(&mut self, state: &HostEqState) {
        let was_on = std::mem::take(&mut self.xf_on);
        self.xf = state.crossfeed.as_ref().map(held).unwrap_or_default();
        self.xf_kept = state.crossfeed.is_some();
        self.flat = state.flat && state.on;
        self.xf_for = None;
        self.stage_dirty = false;
        // A config that was applied with the stage may start the chain again with it on.
        self.xf_conf = state.crossfeed_on;
        if state.crossfeed_on {
            self.reset_xf(OffReason::Restart, RESTART_NOTE.to_string(), true);
        } else if self.flat && !self.ld_keeps_chain() {
            // A pass-through kept for a stream when the owner went: the crossfeed was off
            // already, so the chain is removed with no word about the crossfeed. One that runs
            // for loudness is kept.
            self.owed = Some(Owed {
                plays: false,
                tried: None,
            });
        } else if was_on {
            // The chain was replaced from outside; the stage may still play on it if it was sent
            // to the node that is there now, or the config holds it.
            let plays = self.stage_on_node || self.xf_conf;
            self.reset_xf(
                OffReason::ChainRestarted,
                "crossfeed turned off: the computer EQ was applied again from outside".to_string(),
                plays,
            );
        }
    }

    /// Set the crossfeed off in the state and the view, with `why` and `note`, and owe the chain
    /// the rest ([`EqControl::finish_reset`]); `plays` says whether the stage may be on it.
    pub(super) fn reset_xf(&mut self, why: OffReason, note: String, plays: bool) {
        // The stage goes off by itself: the sound climbs back as it does for a hand's off.
        self.arm_climb();
        self.xf_on = false;
        self.xf_for = None;
        self.stage_dirty = false;
        self.xf_note = Some(note);
        self.xf_reason = Some(why);
        self.owe(plays);
    }

    /// Compare the output the stage was turned on for with the one the last look saw: another
    /// one is a reset. A stage turned on while no output could be read takes the first one seen.
    pub(super) fn check_output(&mut self) {
        let Some(now) = self.output.clone() else {
            return;
        };
        self.check_speakers_output(&now);
        if !self.xf_on {
            return;
        }
        match self.xf_for.as_deref() {
            Some(was) if was != now.name => {
                let name = now.description.clone().unwrap_or(now.name);
                self.reset_xf(
                    OffReason::OutputChanged,
                    format!("crossfeed turned off: the output changed to {name}"),
                    true,
                );
            }
            Some(_) => {}
            None => self.xf_for = Some(now.name),
        }
    }

    /// Do what a reset owes the chain: nothing when no chain runs, the chain removed when it
    /// runs for crossfeed alone, else the stage off. One that fails is tried again a second
    /// later (a stop tries at once).
    pub(super) async fn finish_reset(&mut self) {
        self.finish_stage_reset().await;
        // A config that holds convolvers for speakers that are off is written without them.
        self.rewrite_speakers_conf().await;
    }

    async fn finish_stage_reset(&mut self) {
        let Some(owed) = self.owed.clone() else {
            return;
        };
        if owed.tried.is_some_and(|at| self.ticks < at + WATCH_EVERY) {
            return;
        }
        if self.running().await == Some(false) {
            // No chain runs, so there is no stage and nothing to take out. The EQ's own switch
            // stays as the state says: a container applies it again at its start, and a state
            // that says on is not turned off here. A chain that ran for crossfeed alone is gone.
            self.owed = None;
            self.xf_conf = false;
            self.loaded_xf = None;
            self.sp.conf_on = false;
            self.sp.loaded = None;
            self.pin();
            if self.flat {
                self.runs = false;
                self.flat = false;
            }
            self.ld_gone();
            let _ = self.save_state();
            return;
        }
        if !owed.plays && !self.flat {
            self.owed = None;
            let _ = self.save_state();
            return;
        }
        if self.physical_off().await {
            self.owed = None;
        } else if let Some(owed) = self.owed.as_mut() {
            owed.tried = Some(self.ticks);
            if self.xf_note.is_some() || self.sp.note.is_some() {
                // The state and the view say off: say that the chain could not follow.
                self.unavailable_note();
            }
        }
    }

    fn unavailable_note(&mut self) {
        for note in [self.xf_note.as_mut(), self.sp.note.as_mut()]
            .into_iter()
            .flatten()
        {
            if !note.contains("could not be taken out") {
                note.push_str(" (the stage could not be taken out of the chain yet; trying again)");
            }
        }
    }

    /// Take the stage off the chain, whatever the way: the chain that runs for crossfeed alone is
    /// removed, a live one gets the bypass (the preamp rises after it), and one whose config may
    /// hold the stage, or that refuses the send, is applied again without it. Whether it is off.
    async fn physical_off(&mut self) -> bool {
        if self.flat && !self.keeps_chain() {
            return self.turn_off().await.is_ok();
        }
        let owed = self.owed.take();
        if self.stage_by_live() {
            // Whether the chain holds the speakers' stage is read now: the send carries it.
            self.has_speakers_ports().await;
        }
        if !self.conf_holds_stage()
            && !self.sp.conf_on
            && self.stage_by_live()
            && (self.xf_ports || self.sp.ports)
        {
            self.stage_dirty = true;
            self.look_first = true;
            if self.send_dirty().await {
                let _ = self.save().await;
                return true;
            }
            self.stage_dirty = false;
        }
        match self.apply().await {
            Ok(()) => {
                let _ = self.save().await;
                true
            }
            Err(_) => {
                self.owed = owed;
                false
            }
        }
    }

    /// Stop gracefully: the crossfeed and the virtual speakers turn off (and a chain that ran for
    /// them alone is removed), so that a stop leaves no stage in the path; the loudness stage is
    /// sent flat, its chain kept.
    /// The daemon's stop and a TUI with no daemon at its quit call it.
    pub async fn shutdown(&mut self) {
        // A pass-through chain kept for a stream goes with the stop.
        if self.passthrough() && self.owed.is_none() {
            self.owed = Some(Owed {
                plays: false,
                tried: None,
            });
        }
        if self.xf_on {
            self.reset_xf(
                OffReason::Stopped,
                "crossfeed turned off: svolder stopped".to_string(),
                true,
            );
        }
        if self.sp.on {
            self.reset_sp(
                SpeakersOff::Stopped,
                "virtual speakers turned off: svolder stopped".to_string(),
                true,
            );
        }
        // The loudness stage goes flat at once; its settings stay, and so does a chain that runs
        // for it (the next owner finds it and follows the volume).
        self.ld_stop_now();
        if let Some(owed) = self.owed.as_mut() {
            owed.tried = None;
        }
        self.finish_reset().await;
        // The reset's line carried it; when none went (nothing owed), the stop sends its own.
        if !self.dirty.is_empty() && self.runs && self.live_now() {
            let _ = self.send_dirty().await;
        }
        self.publish();
    }

    /// The crossfeed as a screen shows it.
    pub(super) fn crossfeed_view(&self) -> CrossfeedView {
        let output = self
            .output
            .as_ref()
            .map(|out| out.description.clone().unwrap_or_else(|| out.name.clone()));
        let mut view = CrossfeedView::of(
            &self.xf,
            self.xf_on,
            self.rate(),
            output,
            self.xf_note.clone(),
        );
        view.off_reason = self.xf_reason.map(|why| why.word().to_string());
        view.unavailable = self
            .chain
            .unavailable()
            .map(|failure| format!("computer controls unavailable: {}", failure.what))
            .or_else(|| self.legacy.then(|| LEGACY_NOTE.to_string()))
            .or_else(|| self.held.then(|| HELD_NOTE.to_string()));
        view
    }

    /// Look at the graph once: the node (so the next sends find it) and the output it plays into.
    pub(super) async fn look_output(&mut self) {
        self.params.forget_node();
        let _ = self.params.node().await;
        if let Some(output) = self.params.output() {
            self.output = Some(output);
        }
    }

    /// Whether the running chain has the stage's controls.
    pub(super) async fn has_ports(&mut self) -> bool {
        self.params.forget_node();
        let ports = self
            .params
            .node()
            .await
            .is_some_and(|node| node.has_crossfeed_controls());
        self.xf_ports = ports;
        ports
    }

    /// Change the crossfeed: its model, preset or values, and whether it plays. See
    /// [`HostEqOp::Crossfeed`'s rules](CrossfeedSettings::change): the values are held to the
    /// model's ranges, a value edit makes the preset Custom, and `on` is applied last. A drag
    /// (`done: false`) is sent on the next tick and not saved; the last value of it and every
    /// other change is sent at once and saved.
    pub async fn crossfeed(&mut self, edit: CrossfeedEdit) -> Result<(), EqFailure> {
        self.rise_ok_until = self.ticks + RISE_OK_TICKS;
        let result = self.crossfeed_inner(edit).await;
        self.disarm_climb();
        self.end_bypass_if_on();
        self.publish();
        result
    }

    async fn crossfeed_inner(&mut self, edit: CrossfeedEdit) -> Result<(), EqFailure> {
        self.finish_reset().await;
        let change = CrossfeedChange {
            model: edit.model,
            preset: edit.preset,
            amount_pct: edit.amount_pct,
            crossover_hz: edit.crossover_hz,
            delay_ms: edit.delay_ms,
        };
        if edit.on.is_none() && change == CrossfeedChange::default() {
            return Err(refusal(
                "the crossfeed change names nothing",
                "Name on, a model, a preset or a value",
            ));
        }
        if let Some(failure) = self.chain.unavailable() {
            return Err(failure);
        }
        if self.legacy {
            return Err(refusal(
                "an older svolder's computer EQ is playing, and crossfeed cannot join it",
                "Turn the computer EQ off and on, which replaces it",
            ));
        }
        if self.held {
            return Err(refusal(
                HELD_NOTE,
                "Turn the computer EQ off, or change the output",
            ));
        }
        let rate = self.rate();
        let next = self.xf.change(&change, rate).map_err(|why| {
            refusal(
                why,
                "The view lists the models, presets and ranges the crossfeed takes",
            )
        })?;
        let want = edit.on.unwrap_or(self.xf_on);
        if want
            && let Some(refused) = over_stage_rate("crossfeed runs", self.stage_start_rate().await)
        {
            return Err(refused);
        }
        if want {
            Stage::of(&next, rate).map_err(|why| {
                refusal(
                    format!("the crossfeed cannot be applied: {why}"),
                    "Change the crossfeed's crossover or model",
                )
            })?;
        }
        let was = (
            self.xf.clone(),
            self.xf_kept,
            self.xf_note.take(),
            self.xf_reason.take(),
        );
        let result = match (self.xf_on, want) {
            (false, false) => {
                // Settings only: kept, and saved when the change is final.
                self.xf = next;
                self.xf_kept = true;
                match edit.done {
                    true => self.save_settings().await,
                    false => Ok(()),
                }
            }
            (true, true) => self.xf_change(next, edit.done).await,
            (false, true) => self.xf_start(next).await,
            (true, false) => {
                self.arm_climb();
                self.xf_stop(next).await
            }
        };
        if result.is_err() {
            (self.xf, self.xf_kept, self.xf_note, self.xf_reason) = was;
            // A line the change handed to `pw-cli` may land yet (the session repairs a line that
            // failed with the next one): the stage as it stands now goes with the next line, so
            // the chain does not keep a stage the preamp is no longer worked out for.
            if self.runs && self.xf_ports && self.stage_by_live() {
                self.stage_dirty = true;
            }
        }
        result
    }

    /// Save the settings (and the state) when there is a state to hold them: a chain that was
    /// never applied has none, and the settings are kept until one is.
    pub(super) async fn save_settings(&mut self) -> Result<(), EqFailure> {
        match self.runs {
            true => self.save().await,
            false => self.save_state(),
        }
    }

    /// New values while the stage plays: sent on the next tick for a drag, now for the last
    /// value (by applying the chain when live updates cannot carry them).
    async fn xf_change(&mut self, next: CrossfeedSettings, done: bool) -> Result<(), EqFailure> {
        self.xf = next;
        self.xf_kept = true;
        if self.stage_by_live() {
            self.stage_dirty = true;
        }
        if !done {
            return Ok(());
        }
        self.look_first = true;
        if self.stage_by_live() && self.send_dirty().await {
            return self.save().await;
        }
        self.stage_dirty = false;
        self.apply().await?;
        self.save().await
    }

    /// Turn the stage on with `next`: the chain is there first (applied flat when none runs,
    /// rebuilt once when it is from before the stage), then the stage goes in.
    async fn xf_start(&mut self, next: CrossfeedSettings) -> Result<(), EqFailure> {
        let before = (
            std::mem::replace(&mut self.xf, next),
            self.xf_kept,
            self.flat,
            self.runs,
        );
        self.xf_kept = true;
        self.xf_on = true;
        // Crossfeed and the virtual speakers are one choice: the speakers go off in the same
        // update (the send that turns the stage on carries their bypass).
        let speakers_were_on = std::mem::take(&mut self.sp.on);
        let speakers_before = (
            self.sp.note.clone(),
            self.sp.reason,
            self.sp.for_output.clone(),
        );
        if speakers_were_on {
            self.sp.for_output = None;
            self.sp.note = Some("virtual speakers turned off: crossfeed is on".to_string());
        }
        let result = self.xf_start_inner().await;
        if result.is_err() {
            self.xf_on = false;
            if speakers_were_on {
                self.speakers_back(speakers_before);
            }
            self.stage_dirty = false;
            self.force_conf = false;
            // A flat chain applied for this start goes again, so the EQ the user had turned off
            // stays off; one that cannot be removed now is owed its removal.
            let applied_flat = !before.3 && self.runs;
            (self.xf, self.xf_kept, self.flat, self.runs) = before;
            if applied_flat {
                self.runs = true;
                self.flat = true;
                if self.turn_off().await.is_err() {
                    self.owed = Some(Owed {
                        plays: true,
                        tried: Some(self.ticks),
                    });
                }
            }
        }
        result
    }

    /// A crossfeed start that turned the speakers off failed: they play again as they were when
    /// the chain still holds their set, and stay off in words when it was rebuilt without it.
    fn speakers_back(&mut self, before: (Option<String>, Option<SpeakersOff>, Option<String>)) {
        if self.sp.loaded.is_some() && self.sp.loaded == self.sp.chosen && self.sp.ports {
            self.sp.on = true;
            (self.sp.note, self.sp.reason, self.sp.for_output) = before;
            return;
        }
        self.sp.for_output = None;
        self.sp.note = Some(
            "virtual speakers turned off: the computer EQ chain was rebuilt without them".into(),
        );
        self.sp.reason = Some(SpeakersOff::ChainRestarted);
        let _ = self.save_state();
    }

    async fn xf_start_inner(&mut self) -> Result<(), EqFailure> {
        if !self.runs {
            // The EQ is off: the chain plays flat for the crossfeed alone.
            self.flat = true;
            self.apply().await?;
        } else if !(self.stage_by_live() && self.has_ports().await) {
            // A chain from before the stage cannot take it live, and one that takes no live
            // updates holds it in its config: the chain is built again, once.
            let from_before = self.stage_by_live();
            self.apply().await?;
            if from_before {
                self.xf_note =
                    Some("the computer EQ chain was rebuilt once to add crossfeed".into());
            }
        }
        self.look_output().await;
        self.xf_for = self.output.as_ref().map(|out| out.name.clone());
        // The stage goes to the chain by a live send unless the config holds it.
        self.stage_dirty = !self.xf_conf;
        if self.stage_dirty && self.stage_by_live() && !self.send_dirty().await {
            // The send did not take: the chain holds the stage in its config instead.
            self.stage_dirty = false;
            self.force_conf = true;
            self.apply().await?;
        }
        self.save().await
    }

    /// Turn the stage off with `next` kept as the settings: a chain that runs for crossfeed alone
    /// is removed (both are off now), any other has the stage taken out live, or is applied
    /// again without it.
    async fn xf_stop(&mut self, next: CrossfeedSettings) -> Result<(), EqFailure> {
        let before = std::mem::replace(&mut self.xf, next);
        self.xf_kept = true;
        let result = self.xf_stop_inner().await;
        if result.is_err() {
            self.xf = before;
        }
        result
    }

    async fn xf_stop_inner(&mut self) -> Result<(), EqFailure> {
        if self.flat && !self.ld_keeps_chain() {
            // With a stream playing through the chain it stays as a pass-through (the stage goes
            // to the bypass live); else it is removed. Loudness on keeps it: the stage goes off
            // as from any chain that stays.
            let was_for = self.xf_for.take();
            self.xf_on = false;
            match self.keep_passthrough().await {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(failure) => return Err(failure),
            }
            // Removes the chain, then says off in the state; the stage is off in the settings
            // first, so the removal is not told as a reset.
            let removed = self.turn_off().await;
            if removed.is_err() {
                self.xf_on = true;
                self.xf_for = was_for;
            }
            return removed;
        }
        // The stage off in the settings: the update that takes it off the chain puts the preamp
        // back up after it.
        self.xf_on = false;
        self.xf_for = None;
        if !self.conf_holds_stage() && self.stage_by_live() && self.has_ports().await {
            self.stage_dirty = true;
            self.look_first = true;
            if self.send_dirty().await {
                return self.save().await;
            }
            self.stage_dirty = false;
        }
        match self.apply().await {
            Ok(()) => self.save().await,
            Err(failure) => {
                // The chain runs as it was: the stage plays still.
                self.xf_on = true;
                Err(failure)
            }
        }
    }
}
