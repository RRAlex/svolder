// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's part of a scene in [`EqControl`]: captured as a value, and recalled as one
//! change.
//!
//! A recall checks every stage first (the curve, crossfeed, loudness, the virtual speakers and
//! the choice between the last two), and a refusal refuses the whole part in words before
//! anything changes. Then it sets every stage's state together and makes ONE change to the
//! chain: a single apply when the chain has to be built or rebuilt (none runs, live updates are
//! off, the speakers' set changes, a stage is missing from the chain, a config holds a stage
//! that goes off), else a single live line carrying the slots, the preamp and the stages. No
//! flat chain is made and removed in between. The preamp cap and the level held for a stream
//! (0.36.11) are those of a preset load: a curve that needs more preamp than the running chain
//! allows goes live at the cap while a stream plays, and the chain is applied again once it has
//! been idle. Crossfeed's cost is in the preamp before the stage plays, and loudness is
//! cut-only, so the computer never plays above unity at any instant of a recall.

use super::vs::unusable;
use super::*;
use crate::eq::loudness::{LoudnessChange, LoudnessSettings};
use crate::eq::scene::ComputerPart;

/// What a recall did, in words for the caller.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recalled {
    /// Sentences about what the recall did that the part does not say: a level held for a
    /// stream that plays, a chain rebuilt for a set.
    pub notes: Vec<String>,
}

/// What a recall checked and worked out before it changed anything.
struct Plan {
    points: Points,
    crossfeed: CrossfeedSettings,
    loudness: LoudnessSettings,
    chosen: Option<String>,
    /// What the checks left as it was, said in the answer.
    notes: Vec<String>,
    /// A chain is wanted: the EQ or a stage plays.
    chain: bool,
    /// Loudness is on and the chain can carry it.
    loudness_plays: bool,
}

/// Every field a recall sets, to put back when it fails.
struct Before {
    points: Points,
    stated: f64,
    preset: Option<String>,
    origin: Option<String>,
    flat: bool,
    runs: bool,
    xf: CrossfeedSettings,
    xf_kept: bool,
    xf_on: bool,
    xf_for: Option<String>,
    xf_note: Option<String>,
    xf_reason: Option<OffReason>,
    chosen: Option<String>,
    sp_on: bool,
    sp_for: Option<String>,
    sp_note: Option<String>,
    sp_reason: Option<SpeakersOff>,
    ld: LoudnessSettings,
    ld_kept: bool,
    ld_note: Option<String>,
    ld_down: bool,
    stage_hold: Option<f64>,
    lift: bool,
    played_at: u64,
}

impl EqControl {
    /// The computer's part of a scene: what plays now, as stated. The preamp is the curve's own,
    /// whatever level a hold on a playing stream keeps the chain at.
    pub fn computer_part(&self) -> ComputerPart {
        ComputerPart {
            eq_on: self.eq_on(),
            name: self.name(),
            origin: self.origin.clone(),
            preamp_db: self.stated,
            filters: self.points.filters.clone(),
            loudness: Some(self.ld.settings),
            crossfeed_on: self.xf_on,
            crossfeed: Some(self.xf.clone()),
            speakers_on: self.sp.on,
            speakers: self.sp.chosen.clone(),
        }
    }

    /// Put a scene's computer part on the computer as one change (see the module's text). A
    /// refusal leaves everything as it was.
    pub async fn recall(&mut self, part: ComputerPart) -> Result<Recalled, EqFailure> {
        let result = self.recall_inner(part).await;
        if result.is_ok() {
            self.loads += 1;
        }
        // The view names the set the speakers play once the sets are read again.
        self.refresh_sets();
        self.publish();
        result
    }

    async fn recall_inner(&mut self, part: ComputerPart) -> Result<Recalled, EqFailure> {
        self.finish_reset().await;
        let plan = self.recall_checked(&part).await?;
        let before = self.recall_set(&part, &plan);
        let mut notes = plan.notes.clone();
        let result = self.recall_change(&part, &plan, &before, &mut notes).await;
        match result {
            Ok(()) => {
                // A stage the part turns on that is not on (a chain that restarted under the
                // recall's own line, a set the apply found unplayable) says why.
                if part.crossfeed_on && !self.xf_on {
                    notes.push(
                        self.xf_note.clone().unwrap_or_else(|| {
                            "crossfeed turned off: it did not stay on".to_string()
                        }),
                    );
                }
                if part.speakers_on && !self.sp.on {
                    notes.push(self.sp.note.clone().unwrap_or_else(|| {
                        "virtual speakers turned off: they did not stay on".to_string()
                    }));
                }
                if let (true, Some(id), Some(store)) =
                    (part.speakers_on, &self.sp.chosen, &self.sp.store)
                {
                    // A set that is used: the store evicts the least recently used.
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.as_secs());
                    let _ = store.touch(id, now);
                }
                Ok(Recalled { notes })
            }
            Err(failure) => {
                self.recall_undo(before).await;
                Err(failure)
            }
        }
    }

    /// Every check a recall makes, with nothing changed.
    async fn recall_checked(&mut self, part: &ComputerPart) -> Result<Plan, EqFailure> {
        if part.crossfeed_on && part.speakers_on {
            return Err(refusal(
                "the part has crossfeed and the virtual speakers on together, which are one choice",
                "Save the scene again with one of them off",
            ));
        }
        // The curve: the checks of a preset's load.
        let correction = host_correction(&from_computer(
            &part.name,
            Correction {
                preamp_db: part.preamp_db,
                filters: part.filters.clone(),
            },
        ))
        .map_err(|why| refusal(why, "Choose a preset of 16 filters or fewer"))?;
        apo::validate(&correction).map_err(|why| {
            refusal(
                format!("the correction cannot be applied: {why}"),
                "Fix the band or the filter it names",
            )
        })?;
        if part.eq_on {
            check_playable(&correction, self.sent.rate.unwrap_or(CONFIG_RATE_HZ))?;
        }
        // Loudness: the numbers are held to their ranges, as a change from outside is.
        let loudness = match part.loudness {
            Some(saved) => self.ld.settings.change(LoudnessChange {
                on: Some(saved.on),
                strength_pct: Some(saved.strength_pct),
                reference_pct: Some(saved.reference_pct),
            })?,
            None => self.ld.settings.change(LoudnessChange {
                on: Some(false),
                ..LoudnessChange::default()
            })?,
        };
        let loudness_plays = loudness.on
            && self.live
            && self.chain.unavailable().is_none()
            && !self.legacy
            && !self.held
            && self.unavailable.is_none();
        let chain = part.eq_on || part.crossfeed_on || part.speakers_on || loudness_plays;
        if chain && let Some(failure) = self.chain.unavailable() {
            return Err(failure);
        }
        let stages = part.crossfeed_on || part.speakers_on;
        if stages && self.legacy {
            return Err(refusal(
                "an older svolder's computer EQ is playing, and crossfeed and virtual speakers \
                 cannot join it",
                "Turn the computer EQ off and on, which replaces it",
            ));
        }
        if stages && self.held {
            return Err(refusal(
                HELD_NOTE,
                "Turn the computer EQ off, or change the output",
            ));
        }
        // Crossfeed: the values held to the model's ranges, and a stage the rate can play.
        let crossfeed = match (&part.crossfeed, part.crossfeed_on) {
            (Some(saved), _) => crate::eq::crossfeed::held(saved),
            (None, false) => self.xf.clone(),
            (None, true) => {
                return Err(refusal(
                    "the part turns crossfeed on and holds no crossfeed settings",
                    "Save the scene again",
                ));
            }
        };
        if part.crossfeed_on {
            if let Some(refused) = over_stage_rate("crossfeed runs", self.stage_start_rate().await)
            {
                return Err(refused);
            }
            Stage::of(&crossfeed, self.rate()).map_err(|why| {
                refusal(
                    format!("the crossfeed cannot be applied: {why}"),
                    "Change the crossfeed's crossover or model",
                )
            })?;
        }
        // The speakers: a set that is on must be fetched and usable; a choice that is only kept
        // while they are off is taken when it can be and left as it was when it cannot.
        let store = self.sp.store.clone();
        let mut notes = Vec::new();
        let chosen = match (&part.speakers, &store) {
            (Some(id), Some(store)) => match store.check(id) {
                Ok(_) => Some(id.clone()),
                Err(error) if part.speakers_on => return Err(unusable(id, &error)),
                Err(_) => {
                    notes.push(format!(
                        "the virtual speakers' set {id} is not fetched: their choice was left as it was"
                    ));
                    self.sp.chosen.clone()
                }
            },
            (None, _) if part.speakers_on => {
                return Err(refusal(
                    "the part turns the virtual speakers on and names no set",
                    "Save the scene again",
                ));
            }
            (_, None) if part.speakers_on => {
                return Err(refusal(
                    "virtual speakers are not available here",
                    "Save the scene again with them off",
                ));
            }
            _ => self.sp.chosen.clone(),
        };
        if part.speakers_on
            && let Some(refused) =
                over_stage_rate("virtual speakers run", self.stage_start_rate().await)
        {
            return Err(refused);
        }
        Ok(Plan {
            points: points_of(correction.filters.clone()),
            crossfeed,
            loudness,
            chosen,
            notes,
            chain,
            loudness_plays,
        })
    }

    /// Set every stage's state at once; what it was is returned.
    fn recall_set(&mut self, part: &ComputerPart, plan: &Plan) -> Before {
        let before = Before {
            points: std::mem::replace(&mut self.points, plan.points.clone()),
            stated: std::mem::replace(&mut self.stated, part.preamp_db),
            preset: self.preset.replace(part.name.clone()),
            origin: std::mem::replace(&mut self.origin, part.origin.clone()),
            flat: self.flat,
            runs: self.runs,
            xf: std::mem::replace(&mut self.xf, plan.crossfeed.clone()),
            xf_kept: self.xf_kept,
            xf_on: self.xf_on,
            xf_for: self.xf_for.take(),
            xf_note: self.xf_note.take(),
            xf_reason: self.xf_reason.take(),
            chosen: std::mem::replace(&mut self.sp.chosen, plan.chosen.clone()),
            sp_on: self.sp.on,
            sp_for: self.sp.for_output.take(),
            sp_note: self.sp.note.take(),
            sp_reason: self.sp.reason.take(),
            ld: std::mem::replace(&mut self.ld.settings, plan.loudness),
            ld_kept: self.ld.kept,
            ld_note: self.ld.note.take(),
            ld_down: self.ld.down,
            stage_hold: self.stage_hold,
            lift: self.lift,
            played_at: self.played_at,
        };
        // Each recall holds the preamp for its own line or not at all.
        self.stage_hold = None;
        self.xf_kept |= part.crossfeed.is_some();
        self.xf_on = part.crossfeed_on;
        self.sp.on = part.speakers_on;
        self.ld.kept |= part.loudness.is_some();
        if plan.loudness_plays {
            self.ld.down = false;
        }
        // The chain plays flat when only a stage keeps it.
        self.flat = plan.chain && !part.eq_on;
        // A stage that stays on keeps the output it was turned on for.
        if part.crossfeed_on && before.xf_on {
            self.xf_for = before.xf_for.clone();
        }
        if part.speakers_on && before.sp_on {
            self.sp.for_output = before.sp_for.clone();
        }
        before
    }

    /// Make the one change to the chain, and keep the result.
    async fn recall_change(
        &mut self,
        part: &ComputerPart,
        plan: &Plan,
        before: &Before,
        notes: &mut Vec<String>,
    ) -> Result<(), EqFailure> {
        if !plan.chain {
            return self.recall_nothing_plays().await;
        }
        let rebuild = !self.runs || self.recall_needs_rebuild(part, plan, before).await;
        let mut applied = rebuild;
        if rebuild {
            self.dirty.clear();
            self.apply().await?;
        } else {
            self.dirty.extend(1..=SLOTS as u8);
            self.stage_dirty =
                part.crossfeed_on || before.xf_on || part.speakers_on || before.sp_on;
            self.look_first = self.stage_dirty;
            if (part.crossfeed_on && !before.xf_on) || (part.speakers_on && !before.sp_on) {
                self.look_output().await;
                self.recall_output();
            }
            // A curve that needs more preamp than the chain's cap goes live at the cap while a
            // stream plays, and is applied again once the stream has paused; with none playing
            // it is applied now, so its own preamp plays.
            // A stage that costs less than the one it replaces, over slots that change in the
            // same line, holds the preamp low enough for both orders of the line (the line sets
            // that hold itself): a level held for a stream, to be lifted with it.
            let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
            let node = self.params.node().await;
            let hold = match self.stage_dirty {
                true => self.hold_for_stage(rate, node.as_ref()),
                false => None,
            };
            let held = hold.is_some() || self.capped();
            let went = (!held || self.look_playing().await.is_some()) && self.send_dirty().await;
            if went {
                // The held level is lifted once the music pauses only while the EQ plays (the
                // lift is the EQ's: a chain that plays flat for a stage is not applied again), so
                // only then is it promised.
                if held && (part.eq_on || hold.is_some()) {
                    self.lift = true;
                    self.played_at = self.ticks;
                    notes.push(LEVEL_HELD_NOTE.to_string());
                }
            } else {
                self.dirty.clear();
                self.stage_dirty = false;
                // The live line did not take: the one apply holds the stage in its config, so
                // no second apply follows for a stage's send.
                self.force_conf = part.crossfeed_on || part.speakers_on;
                self.apply().await?;
                applied = true;
            }
        }
        if applied {
            if before.runs && part.speakers_on && before.chosen != self.sp.chosen {
                notes.push("the computer EQ chain was rebuilt to load the set".to_string());
            }
            self.recall_after_apply().await?;
        }
        self.save().await
    }

    /// Nothing is to play: the chain goes, as it does when the EQ's last reason for it goes (kept
    /// as a pass-through while a stream plays through it), and the settings are kept.
    async fn recall_nothing_plays(&mut self) -> Result<(), EqFailure> {
        if !self.runs {
            return self.save_settings().await;
        }
        let running = self.running().await;
        if running == Some(true) && self.keep_passthrough().await? {
            return Ok(());
        }
        self.turn_off().await
    }

    /// Whether the running chain has to be built again for the part: it cannot take the change
    /// live, lacks a stage the part turns on, loads another set, or holds in its config a stage
    /// that goes off.
    async fn recall_needs_rebuild(
        &mut self,
        part: &ComputerPart,
        plan: &Plan,
        before: &Before,
    ) -> bool {
        if !self.stage_by_live() {
            return true;
        }
        if (before.xf_on && !part.crossfeed_on && self.conf_holds_stage())
            || (before.sp_on && !part.speakers_on && self.sp.conf_on)
        {
            return true;
        }
        if part.speakers_on
            && (self.sp.loaded != self.sp.chosen || !self.has_speakers_ports().await)
        {
            return true;
        }
        if part.crossfeed_on && !self.has_ports().await {
            return true;
        }
        plan.loudness_plays && !self.has_loudness_ports().await
    }

    /// A chain was just built: the output the stages were turned on for is read, and the stage
    /// that is on goes to it by a live line unless its config holds it.
    async fn recall_after_apply(&mut self) -> Result<(), EqFailure> {
        if self.xf_on || self.sp.on {
            self.look_output().await;
            self.recall_output();
        }
        if self.stage_dirty && self.stage_by_live() && !self.send_dirty().await {
            // The send did not take: the chain holds the stage in its config instead.
            self.stage_dirty = false;
            self.force_conf = true;
            self.apply().await?;
        }
        Ok(())
    }

    /// The output a stage that is on plays into is the one last seen.
    fn recall_output(&mut self) {
        let name = self.output.as_ref().map(|out| out.name.clone());
        if self.xf_on && self.xf_for.is_none() {
            self.xf_for = name.clone();
        }
        if self.sp.on && self.sp.for_output.is_none() {
            self.sp.for_output = name;
        }
    }

    /// A recall failed: every field it set goes back, and a chain it built for the part is
    /// taken out again.
    async fn recall_undo(&mut self, before: Before) {
        self.force_conf = false;
        self.stage_dirty = false;
        if !before.runs && self.runs {
            // A chain built for this recall goes again; one that cannot be removed now is owed
            // its removal.
            if self.turn_off().await.is_err() {
                self.owe(true);
                if let Some(owed) = self.owed.as_mut() {
                    owed.tried = Some(self.ticks);
                }
            }
        }
        self.points = before.points;
        self.stated = before.stated;
        self.preset = before.preset;
        self.origin = before.origin;
        self.flat = before.flat && self.runs;
        self.xf = before.xf;
        self.xf_kept = before.xf_kept;
        self.xf_on = before.xf_on && self.runs;
        self.xf_for = before.xf_for;
        self.xf_note = before.xf_note;
        self.xf_reason = before.xf_reason;
        self.sp.chosen = before.chosen;
        self.sp.on = before.sp_on && self.runs;
        self.sp.for_output = before.sp_for;
        self.sp.note = before.sp_note;
        self.sp.reason = before.sp_reason;
        self.ld.settings = before.ld;
        self.ld.kept = before.ld_kept;
        self.ld.note = before.ld_note;
        self.ld.down = before.ld_down;
        (self.stage_hold, self.lift, self.played_at) =
            (before.stage_hold, before.lift, before.played_at);
        self.pin();
        // A line the recall handed to `pw-cli` may land yet: the state as it stands goes with
        // the next line.
        self.take_back_line();
        if self.runs && self.stage_by_live() && (self.xf_ports || self.sp.ports) {
            self.stage_dirty = true;
        }
    }
}
