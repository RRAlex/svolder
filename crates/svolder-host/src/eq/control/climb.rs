// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The climb: when a change raises the level the chain plays at (a stage turned off, the EQ off
//! into a pass-through, the bypass), the preamp drops first, in the same line as the stage's
//! ports, to the level that was heard before, then rises at the pace the DAC's volume rule
//! allows, never above the ceiling ([`EqControl::climb_ceiling`]). A lower target ends it at
//! once. Hand edits, preset loads and scene recalls are not climbs: they keep their own rules.
//!
//! The whole of it is the preamp's value ([`EqControl::preamp`]) lagging behind its target
//! ([`EqControl::preamp_target`]), so the ordering rules of a line (a preamp that falls goes
//! before the ports, one that rises after them) hold for it unchanged.

use super::*;
use crate::eq::live::{loudness_params, speakers_params};
use crate::eq::loudness;
use svolder_profiles::safety::CLIMB_PER_SEC;

/// The range, in dB, the DAC's volume rule counts its pace over (a 0..63 one-dB volume).
const CLIMB_RANGE_DB: f64 = 64.0;

/// The rise in the preamp over what the node plays at that is a jump and not a step.
const RISE_JUMP_DB: f64 = 0.4;

/// How many ticks (2 s) a change that may start a climb waits for its first line.
const ARM_TICKS: u64 = 40;

/// How far the preamp rises on one tick of the climb, in dB: [`CLIMB_PER_SEC`] of
/// 64 dB a second at the 50 ms tick (0.32 dB, 6.4 dB a second).
pub const LEVEL_CLIMB_DB_PER_TICK: f64 = CLIMB_PER_SEC * CLIMB_RANGE_DB * 0.05;

impl EqControl {
    /// Whether a line is still owed to the chain: a value to send, the curve of a chain whose graph
    /// has not run, or a resend.
    #[cfg(test)]
    pub(crate) fn lines_pending(&self) -> bool {
        !self.dirty.is_empty() || self.stage_dirty || self.curve_owed || self.resend
    }

    /// How many presets were loaded and state files read again so far.
    #[cfg(test)]
    pub(crate) fn loads(&self) -> u64 {
        self.loads
    }

    /// The level the chain plays at as a listener hears it: the preamp sent plus the speakers'
    /// level (the total of [`ChainLevels`]); `None` with no chain.
    fn heard_db(&self) -> Option<f64> {
        self.runs.then(|| {
            let view = self.speakers_view();
            self.levels(view.as_ref()).total_db
        })
    }

    /// A change that raises the level begins: the level heard now is what its climb starts from.
    /// Called before the change, on every path that may turn a stage off; the first line the
    /// change sends starts the climb ([`EqControl::start_climb`]), and the entry point ends the
    /// arming ([`EqControl::disarm_climb`]) whether a line went or not.
    pub(super) fn arm_climb(&mut self) {
        // A change already armed keeps the level it heard before it.
        if self.climb_from.is_some() {
            return;
        }
        self.climb_from = self.heard_db().map(|db| (db, self.ticks));
    }

    /// An arming whose line did not go within [`ARM_TICKS`] ticks is dropped: a later line is not
    /// the change's.
    pub(super) fn expire_climb_arming(&mut self) {
        if self
            .climb_from
            .is_some_and(|(_, at)| self.ticks > at + ARM_TICKS)
        {
            self.climb_from = None;
        }
    }

    /// The change is over: a climb nothing started is not owed.
    pub(super) fn disarm_climb(&mut self) {
        self.climb_from = None;
    }

    /// The speakers' level the line being worked out plays at, in dB: the set's when they play,
    /// 0 with the dry sound.
    fn space_level_db(&self) -> f64 {
        match self.sp.on {
            true => self.speakers_view().map_or(0.0, |view| view.loudness_db),
            false => 0.0,
        }
    }

    /// The first line of an armed change: the preamp starts at the level heard before less what
    /// the stages add now, so the sound does not rise, and climbs from there when that is under
    /// the target.
    pub(super) fn start_climb(&mut self) {
        let Some((from, _)) = self.climb_from.take() else {
            return;
        };
        let mut start = (from - self.space_level_db()).max(-120.0);
        // The first line of a node started from an identity config: what was heard before is the
        // level of the node it replaced, and this one plays its config's preamp with the stage
        // off, so what it plays now is that preamp. The line may raise the preamp only by what the
        // speakers it turns on cut (the give-back the config's preamp was worked out for); if they
        // are not on by the time the line goes (a rate change turned them off), nothing covers the
        // rise and the climb starts from what plays.
        if self.first_line
            && let Some(held) = self.node_preamp_db().filter(|held| *held > SILENT_HELD_DB)
        {
            start = start.min(held - self.space_level_db());
        }
        self.climb = (start < self.preamp_target() - 1e-9).then_some(start);
    }

    /// The preamp the node was last given, in dB: the last line sent, else what the last look read,
    /// else the preamp its config holds.
    fn node_preamp_db(&self) -> Option<f64> {
        let port = preamp_params(0.0).first()?.0.clone();
        let mult = self
            .last_values
            .get(&port)
            .or_else(|| self.seen.get(&port))
            .copied()
            .or_else(|| self.conf_written.map(|db| 10f64.powf(db / 20.0)))?;
        (mult > 0.0).then(|| 20.0 * mult.log10())
    }

    /// A line that raises the level by more than a step and that no hand's change makes climbs
    /// from the level the node plays at: the sound does not jump when a stage goes off by itself
    /// (a chain that restarted, an output that changed) or the curve's preamp is worked out again.
    pub(super) fn climb_unless_hands(&mut self) {
        // A first line from a node's identity config is bounded whoever sent it and whenever: the
        // config's preamp was cut by the set's level for the line that turns the speakers on, so
        // the line may rise by what the speakers it carries cut and no more (the same bound as
        // [`EqControl::start_climb`]'s).
        let first = self.first_line;
        if self.climb.is_some() || (!first && self.ticks <= self.rise_ok_until) {
            return;
        }
        let Some(held) = self.node_preamp_db() else {
            return;
        };
        let from = match first {
            // The line turns the mixers wet only with their ports: only then is the set's level paid.
            true if self.sp.ports => held - self.space_level_db(),
            _ => held,
        };
        // A node at the -120 dB floor plays nothing: it is sent the preamp, not climbed to it.
        if held > SILENT_HELD_DB && self.preamp_target() - from > RISE_JUMP_DB {
            self.climb = Some(from);
        }
    }

    /// The most the preamp may climb to at `rate`: 0 dB on a chain started from an identity
    /// config (its loaded curve is flat, so a rebuild of its graph puts back a flat chain), the
    /// loaded curve's own cap on a PipeWire that takes no live changes (its config holds the
    /// curve) and the bound of a node this controller did not start. Every path that bounds the
    /// preamp reads it here.
    pub(super) fn climb_ceiling(&self, rate: u32) -> f64 {
        self.cap_at(rate)
    }

    /// One tick of the climb: the preamp rises one step toward the target, every slot due so the
    /// line that carries it is an ordinary full line; a target at or under the level, or reached,
    /// ends it.
    pub(super) fn climb_step(&mut self) {
        let Some(level) = self.climb else {
            return;
        };
        // Nothing plays through a graph that has not run, and a line to it is lost; a line that
        // has not gone (a `pw-cli` that is being started again) is not outrun: the node may hold
        // the level of the line before, and the one that follows must not be more than a step
        // above it.
        if self.not_run || !self.dirty.is_empty() || self.stage_dirty {
            return;
        }
        let target = self.preamp_target();
        let next = level + LEVEL_CLIMB_DB_PER_TICK;
        self.climb = (next < target - 1e-9).then_some(next);
        // The line that ends it carries the target itself.
        self.dirty.extend(1..=SLOTS as u8);
    }

    /// Cut a climb in progress back to `db`, as a controller that keeps restarting it would.
    #[cfg(test)]
    pub(crate) fn cut_climb_to(&mut self, db: f64) {
        if self.climb.is_some() {
            self.climb = Some(db);
        }
    }

    /// The preamp a climb in progress plays at and the one it ends at, in dB; `None` while no line
    /// can go (a chain that waits for a rate its curve plays at, or controls that are unavailable).
    #[cfg(test)]
    pub(crate) fn climb_progress(&self) -> Option<(f64, f64)> {
        self.live_now().then_some(())?;
        Some((self.climb?, self.preamp_target()))
    }

    /// Where the climb in progress ends, as the levels say it: the target preamp plus the
    /// speakers' level.
    pub(super) fn climb_to_db(&self) -> Option<f64> {
        self.climb?;
        Some(self.preamp_target() + self.space_level_db())
    }

    /// Whether the chain is proven untouched by what the controller knows: a pass-through whose
    /// climb ended at exactly 0 dB, started from an identity config that holds no stage, with
    /// the loudness stage at the identity, and the last look after the last line read every port
    /// at its identity value. The repeat of a line, which carries what that line did, is no new
    /// line to it (`fresh_at`, not `sent_at`): a look that read the ports at identity still
    /// stands after it. A judgement the host check makes again from the graph itself.
    pub(super) fn untouched(&self) -> bool {
        if !(self.passthrough()
            && self.climb.is_none()
            && self.conf_written.is_some()
            && !self.xf_conf
            && !self.sp.conf_on
            && self.ld.played == 0.0
            && !self.not_run
            && !self.unvouched
            && !self.legacy
            && !self.held
            && self.dirty.is_empty()
            && !self.stage_dirty
            && self.preamp() == 0.0)
        {
            return false;
        }
        self.looked_at
            .is_some_and(|tick| tick >= self.fresh_at + REVERT_QUIET)
            && self.ports_are_identity(&self.seen)
    }

    /// Whether every port of the chain is listed in `held` at its identity value (compared as the
    /// f32 the graph runs): the 16 slots, the preamp, the crossfeed stage's bypass, the speakers'
    /// dry mixers when the node has them and the loudness sections.
    pub(super) fn ports_are_identity(&self, held: &HashMap<String, f64>) -> bool {
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        let mut wanted: Vec<(String, f64)> = (1..=SLOTS as u8)
            .flat_map(|slot| slot_params(slot, None, rate))
            .collect();
        wanted.extend(preamp_params(0.0));
        wanted.extend(crossfeed_params(&Stage::bypass()));
        if self.sp.ports {
            wanted.extend(speakers_params(false));
        }
        if self.ld.ports {
            wanted.extend(loudness_params(&loudness::stage(0.0, rate).sections));
        }
        wanted.iter().all(|(name, value)| {
            held.get(name)
                .is_some_and(|held| *held as f32 == *value as f32)
        })
    }
}
