// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bypass and the levels in [`EqControl`].
//!
//! The bypass turns every stage of the computer off as ONE change (the scene path: a single
//! apply or a single live line, a stream that plays keeps the chain as a pass-through) and
//! remembers which stages played ([`HeldStages`]). Turning it off brings back exactly those, with
//! their settings as they are now. Any stage that is turned on by any other path ends the bypass
//! and forgets the held set. The held set is saved in the state file, so it survives a restart.

use super::*;
use crate::eq::loudness::LoudnessSettings;

impl EqControl {
    /// Whether the EQ or any stage plays or is asked to: the EQ, crossfeed, the virtual
    /// speakers, loudness.
    pub(super) fn any_stage_on(&self) -> bool {
        self.eq_on() || self.xf_on || self.sp.on || self.ld.settings.on
    }

    /// What the chain plays at; `speakers` is the view of the virtual speakers.
    pub(super) fn levels(&self, speakers: Option<&SpeakersView>) -> ChainLevels {
        if !self.runs {
            return ChainLevels::of(false, 0.0, 0.0, 0.0, false);
        }
        let gain = self.stage_gain();
        let preamp = self.preamp();
        let speakers_db = match (self.sp.on, speakers) {
            (true, Some(view)) => view.loudness_db,
            _ => 0.0,
        };
        let space = match self.sp.on {
            true => speakers_db,
            false => 0.0 - gain,
        };
        ChainLevels::of(
            true,
            preamp + gain,
            space,
            preamp + speakers_db,
            self.passthrough(),
        )
        .with_climb(self.climb_to_db(), self.untouched())
    }

    /// The end of every path that may turn a stage on: a stage that plays now ends the bypass
    /// and the held set is forgotten.
    pub(super) fn end_bypass_if_on(&mut self) {
        if self.bypass.is_some() && self.any_stage_on() {
            self.bypass = None;
            // Best effort, as the save of every other change that has already happened.
            let _ = self.save_state();
        }
    }

    /// What the recall said about the change (a stage that did not stay on, a set that is not
    /// fetched) goes to the view's note, so a front end can say why.
    fn say(&mut self, recalled: Recalled) {
        if !recalled.notes.is_empty() {
            let said = recalled.notes.join("; ");
            self.note = Some(match self.note.take() {
                Some(standing) => format!("{standing}; {said}"),
                None => said,
            });
        }
    }

    /// Turn the bypass on or off. On: the stages that play are held and turned off together;
    /// with none playing and none held it is refused. Off: the held stages come back as they
    /// were; a refusal (a set no longer fetched) leaves the bypass on.
    pub async fn bypass(&mut self, on: bool) -> Result<(), EqFailure> {
        if on {
            self.arm_climb();
        }
        let result = self.bypass_inner(on).await;
        self.disarm_climb();
        self.refresh_sets();
        self.publish();
        result
    }

    async fn bypass_inner(&mut self, on: bool) -> Result<(), EqFailure> {
        let mut part = self.computer_part();
        match (on, self.bypass) {
            (true, Some(_)) | (false, None) => Ok(()),
            (true, None) => {
                let held = HeldStages {
                    eq: self.eq_on(),
                    loudness: self.ld.settings.on,
                    crossfeed: self.xf_on,
                    speakers: self.sp.on,
                };
                if held.is_empty() {
                    return Err(refusal(
                        "nothing plays on the computer to bypass",
                        "Turn the EQ or a stage on first",
                    ));
                }
                part.eq_on = false;
                part.crossfeed_on = false;
                part.speakers_on = false;
                part.loudness = part
                    .loudness
                    .map(|saved| LoudnessSettings { on: false, ..saved });
                let recalled = self.recall_inner(part).await?;
                self.bypass = Some(held);
                self.say(recalled);
                self.save_state()
            }
            (false, Some(held)) => {
                part.eq_on = held.eq;
                part.crossfeed_on = held.crossfeed;
                part.speakers_on = held.speakers;
                part.loudness = part.loudness.map(|saved| LoudnessSettings {
                    on: held.loudness,
                    ..saved
                });
                let recalled = self.recall_inner(part).await?;
                self.bypass = None;
                self.say(recalled);
                self.save_state()
            }
        }
    }
}
