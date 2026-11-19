// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's loudness in [`EqControl`]: its settings, the stage that follows the computer's
//! volume, and the view a screen draws from.
//!
//! - **The level.** The newest reading of the computer's volume wins: the one the look at the
//!   chain reads from its own `pw-dump` ([`Params::level`]), or the one a volume write read back
//!   and handed over through a [`LevelHint`], which never waits for the controller's lock (a tick
//!   holds it through a `pw-dump`). A hint is the default sink's own reading: the other node in
//!   the path (the chain's, or the device's behind it) keeps the share the last look gave it.
//! - **The glide.** On each tick with a chain live, the target depth is worked out from the
//!   level, the settings and [`loudness::depth_db`]; the depth played moves toward it by at most
//!   [`GLIDE_STEP_DB`] a tick, on [`loudness::DEPTH_STEP_DB`]'s grid. A step marks every slot due,
//!   so it goes as an ordinary full line: the slots and the preamp at the values they hold, which
//!   change nothing and leave them confirmed, and the stage's 24 values. Nothing is sent once it
//!   has arrived. Muted or unread, it holds where it is.
//! - **Safety.** The stage is cut-only on its f32 values at every rate and depth
//!   ([`loudness::stage`]), so it never moves the preamp and needs no ordering against any other
//!   line: every state a line, a failed line, a revert, a restart or a rate change leaves plays at
//!   or under unity. The played depth is stored only once a line went through.
//! - **The chain.** Loudness on keeps a chain (see [`EqControl::keeps_chain`]): with nothing else
//!   on it runs flat for loudness alone, is not a pass-through and stays across a graceful stop,
//!   its stage sent flat. All off removes the chain. A chain that takes no live updates, or one
//!   an older svolder built, makes loudness unavailable with a note; one from before the stage is
//!   rebuilt once when loudness is turned on.

use super::*;
use crate::eq::live::loudness_params;
use crate::eq::loudness::{
    self, Level, LoudnessChange, LoudnessSettings, LoudnessView, Stage as LdStage,
};
use crate::route::SinkLevel;
use svolder_profiles::tone::response::played_peak_db_of;

/// The most the played depth moves on one tick, in dB (10 dB a second at 50 ms).
pub(super) const GLIDE_STEP_DB: f64 = 0.5;

/// What the view says while loudness is on and the chain takes no live updates.
const LIVE_UNAVAILABLE: &str = "loudness follows the volume only on a chain that takes live \
                                changes, which this computer's PipeWire does not";

/// What the view says after a chain from before the stage was rebuilt for it.
const REBUILT_NOTE: &str = "the computer EQ chain was rebuilt once to add loudness";

/// What the view says while loudness is on and no chain runs.
const NO_CHAIN_NOTE: &str = "loudness waits for the computer EQ chain: turn loudness off and on";

/// What the view says while loudness is on over a chain without its stage.
const NO_STAGE_NOTE: &str =
    "the computer EQ chain has no loudness stage: turn loudness off and on to rebuild it";

/// A handle on the computer's volume as a write read it back, for the controller's next tick: it
/// is set without the controller's lock. Clones share one reading.
#[derive(Debug, Clone, Default)]
pub struct LevelHint(Arc<Mutex<Option<SinkLevel>>>);

impl LevelHint {
    /// The default sink reads `level` now.
    pub fn set(&self, level: SinkLevel) {
        *crate::locked(&self.0) = Some(level);
    }

    /// The default sink reads `volume` (on the cubic scale a volume control shows, 1.0 full)
    /// and `muted` now.
    pub fn volume(&self, volume: f64, muted: bool) {
        self.set(SinkLevel {
            linear: volume.max(0.0).powi(3),
            muted,
        });
    }

    /// The reading a write left that the controller has not taken yet.
    pub fn peek(&self) -> Option<SinkLevel> {
        *crate::locked(&self.0)
    }

    fn take(&self) -> Option<SinkLevel> {
        crate::locked(&self.0).take()
    }
}

/// What the controller keeps of the loudness.
#[derive(Debug)]
pub(super) struct Loudness {
    /// The settings, kept while it is off.
    pub(super) settings: LoudnessSettings,
    /// The settings are written to the state file: they were read from it or set by a request.
    pub(super) kept: bool,
    /// The node has the stage's controls, as last looked at.
    pub(super) ports: bool,
    /// The depth of the last line that went through, in dB (0, the identity, on a chain that
    /// started from its config and has been sent none).
    pub(super) played: f64,
    /// The next line goes to the target at once, not one step toward it.
    pub(super) snap: bool,
    /// Held at the identity: the owner stopped.
    pub(super) down: bool,
    /// The newest reading of the computer's volume.
    pub(super) level: Option<SinkLevel>,
    /// The reading the last look gave, by which a read made since is told.
    look: Option<SinkLevel>,
    /// The default sink's share of a hint taken since the last look.
    hinted: Option<f64>,
    /// The share of the other node in the path, from the last look after a hint.
    other: f64,
    /// Where writes put their reading.
    pub(super) hint: LevelHint,
    /// What the view says, once.
    pub(super) note: Option<String>,
    /// The newest stage worked out, for its depth and rate.
    cache: Option<(f64, u32, LdStage)>,
    /// The high sections of the last two lines that went through, newest first: `pw-cli` applies
    /// lines in order and a dead one loses at most the newest, so the node holds one of them (or
    /// its config's identity, after a rebuild).
    went: [Option<[f64; 5]>; 2],
    /// The high sections of lines that failed since the last one that went through: a failed
    /// line may have been written and may land yet.
    pub(super) failed: Vec<[f64; 5]>,
    /// The high section of the line `ld_line` made, and of the one being written.
    pending: Option<[f64; 5]>,
    writing: Option<[f64; 5]>,
    /// The last look did not find the node at a stage this controller sent (behind, another
    /// owner's, or none sent yet): the stage it saw may still be on it.
    behind: bool,
    /// The line whose cut low section was last answered with the exact stage.
    remarked: Option<u64>,
}

impl Default for Loudness {
    fn default() -> Self {
        Loudness {
            settings: LoudnessSettings::default(),
            kept: false,
            ports: false,
            played: 0.0,
            snap: false,
            down: false,
            level: None,
            look: None,
            hinted: None,
            other: 1.0,
            hint: LevelHint::default(),
            note: None,
            cache: None,
            went: [None; 2],
            failed: Vec::new(),
            pending: None,
            writing: None,
            behind: false,
            remarked: None,
        }
    }
}

/// One change of the loudness: the fields of `HostEqOp::Loudness`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoudnessEdit {
    /// Turn it on or off; applied last, so turning on plays the numbers sent with it.
    pub on: Option<bool>,
    /// The strength in percent, snapped to its step and held to its range.
    pub strength_pct: Option<f64>,
    /// The reference in percent of the volume scale, snapped and held.
    pub reference_pct: Option<f64>,
    /// Set the reference to the computer's volume as last read; `reference_pct` then wins when
    /// both are given.
    pub reference_now: bool,
    /// Whether this is the last value of a drag: a drag is not saved.
    pub done: bool,
}

impl EqControl {
    /// A handle a volume write sets the computer's volume through, as it read it back, without
    /// the controller's lock; the next tick takes it.
    pub fn level_hint(&self) -> LevelHint {
        self.ld.hint.clone()
    }

    /// Take the loudness's settings from `state`; what is played and read stays.
    pub(super) fn adopt_loudness(&mut self, state: &HostEqState) {
        self.ld.settings = state.loudness.map(|saved| saved.held()).unwrap_or_default();
        self.ld.kept = state.loudness.is_some();
    }

    /// Whether a chain runs for loudness: it is on, and the chain takes live updates.
    pub(super) fn ld_keeps_chain(&self) -> bool {
        self.ld.settings.on && self.live
    }

    /// Whether a stage keeps the chain running: crossfeed or the speakers play, or loudness is on
    /// over a chain that takes live updates. A flat chain with none of them is a pass-through.
    pub(super) fn keeps_chain(&self) -> bool {
        self.space_on() || self.ld_keeps_chain()
    }

    /// Take a reading the session made since the last one taken (a look reads the graph's
    /// volume with the node), as the newest: `fresh` for the look that just read it.
    pub(super) fn ld_look(&mut self, fresh: bool) {
        let read = self.params.level();
        if !fresh && read == self.ld.look {
            return;
        }
        if let (Some(hinted), Some(read)) = (self.ld.hinted.take(), read)
            && hinted > 1e-9
            && read.linear.is_finite()
        {
            self.ld.other = read.linear / hinted;
        }
        self.ld.look = read;
        self.ld.level = read;
    }

    /// Take the reading a write left, after any read the session made before it: the default
    /// sink's share replaced, the other node's kept.
    pub(super) fn ld_take_hint(&mut self) {
        self.ld_look(false);
        if let Some(hint) = self.ld.hint.take() {
            self.ld.hinted = Some(hint.linear);
            self.ld.level = Some(SinkLevel {
                linear: hint.linear * self.ld.other,
                muted: hint.muted,
            });
        }
    }

    /// The depth the stage is to play, in dB; `None` while it holds (muted, or no volume read).
    fn ld_target(&self) -> Option<f64> {
        if self.ld.down || !self.ld.settings.on {
            return Some(0.0);
        }
        let level = self.ld.level?;
        if level.muted || !level.linear.is_finite() {
            return None;
        }
        Some(loudness::quantise_db(loudness::depth_db(
            level.linear,
            &self.ld.settings,
        )))
    }

    /// The depth the next line carries: a step from the depth played toward the target, or the
    /// target itself when it goes at once.
    fn ld_line_depth(&self) -> f64 {
        let played = self.ld.played;
        match self.ld_target() {
            None => played,
            Some(target) if self.ld.snap => target,
            Some(target) => loudness::quantise_db(
                played + (target - played).clamp(-GLIDE_STEP_DB, GLIDE_STEP_DB),
            ),
        }
    }

    /// The stage for `depth` at `rate`, worked out once for each.
    fn ld_stage(&mut self, depth: f64, rate: u32) -> LdStage {
        if let Some((at, at_rate, stage)) = self.ld.cache
            && at == depth
            && at_rate == rate
        {
            return stage;
        }
        let stage = loudness::stage(depth, rate);
        self.ld.cache = Some((depth, rate, stage));
        stage
    }

    /// The stage's ports for the next line, with the depth they carry, when the node has them.
    ///
    /// A node may take a line in part: one section with the other still as it was. Each section
    /// alone is not cut-only (the high shelf rises to `G_hi` at Nyquist; the low shelf alone never
    /// rises over 1), so the ports go in an order that keeps every partial state made of whole
    /// sections under unity (a section's own ports can still tear: a documented known issue),
    /// whatever the node holds: one of the last two lines that went through (`pw-cli` applies
    /// lines in order and a dead one loses at most the newest), a line that failed since (it may
    /// land yet), the stage the last look saw, until a look finds the node at a stage this
    /// controller sent, or its config's identity (a rebuild no look has seen yet).
    /// - Every line goes low section first, that section cut for the highest of the high sections
    ///   the node may hold and the new one. The node then plays under unity after the low section,
    ///   whatever high one it holds, and after the high one too; and the session's repair line,
    ///   which sets the ports by name (the low section first), is as safe. On a move to a
    ///   shallower stage the low section is cut a little deeper than the stage's own: by the last
    ///   lines' high shelves over the new one, about 0.15 dB on a glide (more over a node found
    ///   behind). A look that finds the cut on the node once the stage has arrived sends the exact
    ///   stage (`ld_settle`).
    /// - The identity with no other high section the node may hold, and the stop's identity (no
    ///   line follows it, and no repair), goes exact, high section first: any low section alone is
    ///   at most 1 (a section's own ports can still tear, a known issue).
    pub(super) fn ld_line(&mut self, rate: u32) -> Option<(f64, Vec<(String, f64)>)> {
        if !self.ld.ports {
            return None;
        }
        let depth = self.ld_line_depth();
        let stage = self.ld_stage(depth, rate);
        // The high sections the node may hold: those of the last two lines that went through and
        // of the lines that failed since, and the one the last look saw until a look finds the
        // node at a stage this controller sent.
        let mut held: Vec<[f64; 5]> = self.ld.went.iter().flatten().copied().collect();
        held.extend(self.ld.failed.iter().copied());
        if self.ld.behind {
            held.extend(
                ['l', 'r']
                    .into_iter()
                    .filter_map(|c| held_high(&self.seen, c)),
            );
        }
        held.retain(|high| !same_section(high, &stage.sections[1]));
        self.ld.pending = Some(stage.sections[1]);
        if stage.is_identity() && (self.ld.down || held.is_empty()) {
            return Some((depth, sections_in_order(&stage.sections, 2)));
        }
        let low = cut_for(stage.sections[0], stage.sections[1], &held, rate);
        Some((depth, sections_in_order(&[low, stage.sections[1]], 1)))
    }

    /// A line with the stage `ld_line` gave is being written.
    pub(super) fn ld_writing(&mut self) {
        self.ld.writing = self.ld.pending.take();
    }

    /// The line being written failed: it may have been written, and may land yet.
    pub(super) fn ld_failed(&mut self) {
        if let Some(high) = self.ld.writing.take()
            && !self.ld.failed.iter().any(|held| same_section(held, &high))
        {
            self.ld.failed.push(high);
        }
    }

    /// The node is another one, or it started again from its config: no line written before is
    /// on it.
    pub(super) fn ld_new_node(&mut self) {
        self.ld.went = [None; 2];
        self.ld.failed.clear();
        self.ld.pending = None;
        self.ld.writing = None;
        self.ld.remarked = None;
    }

    /// A look read the node: whether it holds the stage this controller last sent. Until a look
    /// finds it there (a new owner over a node another left, a restarted node, a first line that
    /// failed or was lost), the stage the look saw is counted as one the node may hold.
    pub(super) fn ld_seen(&mut self, held: &HashMap<String, f64>) {
        let same = |wanted: f64, held: f64| (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6;
        let sent: Vec<(&String, &f64)> = self
            .last_values
            .iter()
            .filter(|(name, _)| name.contains("ld1:") || name.contains("ld2:"))
            .collect();
        let at_sent = !sent.is_empty()
            && sent
                .iter()
                .all(|(name, sent)| held.get(*name).is_some_and(|held| same(**sent, *held)));
        self.ld.behind = self.ld.ports && !at_sent;
    }

    /// A look that found the node at what was last sent, with the stage arrived: a low section
    /// cut for a high one the node no longer holds (a line toward a shallower depth) is replaced
    /// by the exact stage, once for each such line.
    pub(super) fn ld_settle(&mut self, held: &HashMap<String, f64>) {
        if !self.ld.ports || self.ld.behind || !self.dirty.is_empty() {
            return;
        }
        if self
            .ld_target()
            .is_none_or(|target| (target - self.ld.played).abs() > 1e-9)
        {
            return;
        }
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        let stage = self.ld_stage(self.ld.played, rate);
        let same = |wanted: f64, held: f64| (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6;
        let exact = loudness_params(&stage.sections)
            .iter()
            .all(|(name, wanted)| held.get(name).is_none_or(|held| same(*wanted, *held)));
        let line = self.line_of.get("lld1:b0").map(|(line, _)| *line);
        if exact || line.is_none() || line == self.ld.remarked {
            return;
        }
        self.ld.remarked = line;
        self.dirty.extend(1..=SLOTS as u8);
    }

    /// A line that carried the stage at `depth` went through.
    pub(super) fn ld_sent(&mut self, depth: f64) {
        self.ld.played = depth;
        self.ld.snap = false;
        if let Some(high) = self.ld.writing.take() {
            self.ld.went = [Some(high), self.ld.went[0]];
            self.ld.failed.clear();
        }
    }

    /// A step of the glide on a tick with the chain live: it marks every slot due, so the line
    /// that carries it is an ordinary full line. A graph that has not run is sent nothing for it:
    /// it starts from its config when it runs, and the look that finds it run sends everything
    /// again.
    pub(super) fn ld_follow(&mut self) {
        if !self.ld.ports || self.not_run {
            return;
        }
        if let Some(target) = self.ld_target()
            && (target - self.ld.played).abs() > 1e-9
        {
            self.dirty.extend(1..=SLOTS as u8);
        }
    }

    /// Whether the node's stage ports hold the stage last played, at `rate` (a port the node
    /// does not list is no difference).
    pub(super) fn ld_holds_played(&mut self, held: &HashMap<String, f64>, rate: u32) -> bool {
        let same = |wanted: f64, held: f64| (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6;
        let stage = self.ld_stage(self.ld.played, rate);
        loudness_params(&stage.sections)
            .iter()
            .all(|(name, wanted)| held.get(name).is_none_or(|held| same(*wanted, *held)))
    }

    /// No chain runs any more: nothing plays.
    pub(super) fn ld_gone(&mut self) {
        self.ld.played = 0.0;
        self.ld.snap = false;
    }

    /// A graceful stop: the stage goes to the identity at once, every slot due, so the next line
    /// (the stop's own when no other goes) carries it.
    pub(super) fn ld_stop_now(&mut self) {
        self.ld.down = true;
        self.ld.snap = true;
        if self.runs && self.ld.ports && self.ld.played > 0.0 {
            self.dirty.extend(1..=SLOTS as u8);
        }
    }

    /// Why loudness cannot follow the volume now, when it cannot.
    fn ld_unavailable(&self) -> Option<String> {
        self.chain
            .unavailable()
            .map(|failure| format!("computer controls unavailable: {}", failure.what))
            .or_else(|| self.unavailable.clone())
            .or_else(|| self.legacy.then(|| LEGACY_NOTE.to_string()))
            .or_else(|| self.held.then(|| HELD_NOTE.to_string()))
            .or_else(|| (!self.live).then(|| LIVE_UNAVAILABLE.to_string()))
    }

    /// What the view says about the loudness.
    fn ld_view_note(&self) -> Option<String> {
        if !self.ld.settings.on {
            return self.ld.note.clone();
        }
        let looked = self.sent.node.is_some();
        self.ld
            .note
            .clone()
            .or_else(|| (!self.runs).then(|| NO_CHAIN_NOTE.to_string()))
            .or_else(|| (self.runs && looked && !self.ld.ports).then(|| NO_STAGE_NOTE.to_string()))
    }

    /// The loudness as a screen shows it.
    pub(super) fn loudness_view(&self) -> LoudnessView {
        let level = self.ld.level.filter(|_| self.runs).map(|level| Level {
            linear: level.linear,
            muted: level.muted,
        });
        let mut view = LoudnessView::of(
            &self.ld.settings,
            level,
            self.ld.played,
            self.rate(),
            self.ld_view_note(),
            self.ld_unavailable(),
        );
        view.following &= self.runs && self.ld.ports && !self.ld.down && !self.not_run;
        view
    }

    /// Change the loudness's settings, and turn it on or off. A refusal leaves everything as it
    /// was.
    pub async fn loudness(&mut self, edit: LoudnessEdit) -> Result<(), EqFailure> {
        let result = self.loudness_inner(edit).await;
        self.publish();
        result
    }

    async fn loudness_inner(&mut self, edit: LoudnessEdit) -> Result<(), EqFailure> {
        if edit.on.is_none()
            && edit.strength_pct.is_none()
            && edit.reference_pct.is_none()
            && !edit.reference_now
        {
            return Err(refusal(
                "the loudness change names nothing",
                "Name on, the strength or the reference",
            ));
        }
        let mut next = self.ld.settings;
        if edit.reference_now {
            // The newest reading, a write's included; only a chain that runs is looked at, so
            // with none the volume counts as unread.
            self.ld_take_hint();
            let level = self.ld.level.filter(|_| self.runs).map(|level| Level {
                linear: level.linear,
                muted: level.muted,
            });
            next = next.reference_now(level)?;
        }
        let next = next.change(LoudnessChange {
            on: edit.on,
            strength_pct: edit.strength_pct,
            reference_pct: edit.reference_pct,
        })?;
        let turns = next.on != self.ld.settings.on;
        if turns {
            self.finish_reset().await;
        }
        let was = (
            std::mem::replace(&mut self.ld.settings, next),
            std::mem::replace(&mut self.ld.kept, true),
            self.ld.note.take(),
            self.runs,
        );
        let mut result = match (was.0.on, next.on) {
            (false, true) => self.ld_start().await,
            (true, false) => self.ld_stop().await,
            _ => Ok(()),
        };
        if result.is_ok() && edit.done {
            result = self.save_state();
        }
        if result.is_err() {
            (self.ld.settings, self.ld.kept, self.ld.note) = (was.0, was.1, was.2);
            self.ld.snap = false;
            // A flat chain applied for this turn-on goes again; one that cannot be removed now
            // is owed its removal.
            let applied_flat = !was.3 && self.runs && self.flat && !self.space_on();
            if applied_flat && self.turn_off().await.is_err() {
                self.owed = Some(Owed {
                    plays: false,
                    tried: Some(self.ticks),
                });
            }
        }
        result
    }

    /// Turn the stage on: the chain is there first (applied flat when none runs, rebuilt once
    /// when it is from before the stage), and the glide takes it from the identity on the next
    /// ticks. A chain that cannot carry it live takes nothing: the view says why.
    async fn ld_start(&mut self) -> Result<(), EqFailure> {
        if self.chain.unavailable().is_some()
            || self.legacy
            || self.held
            || !self.live
            || self.unavailable.is_some()
        {
            return Ok(());
        }
        self.ld.down = false;
        if !self.runs {
            // No chain: one plays flat for loudness alone.
            let was = std::mem::replace(&mut self.flat, true);
            if let Err(failure) = self.apply().await {
                self.flat = was;
                return Err(failure);
            }
            return self.save().await;
        }
        if !self.has_loudness_ports().await {
            // A chain from before the stage: built again, once.
            self.apply().await?;
            self.ld.note = Some(REBUILT_NOTE.to_string());
            return self.save().await;
        }
        Ok(())
    }

    /// Turn the stage off: it glides to the identity while the EQ or another stage keeps the
    /// chain. As the last reason for the chain, the chain goes as the EQ's own does when the last
    /// reason for it goes: kept as a pass-through for a stream that plays (its line carrying the
    /// identity), else removed.
    async fn ld_stop(&mut self) -> Result<(), EqFailure> {
        if !self.runs || !self.flat || !self.live || self.keeps_chain() {
            return Ok(());
        }
        self.ld.snap = true;
        if self.keep_passthrough().await? {
            return Ok(());
        }
        self.turn_off().await
    }

    /// Whether the running chain has the stage's controls, from a fresh look.
    async fn has_loudness_ports(&mut self) -> bool {
        self.params.forget_node();
        let node = self.params.node().await;
        self.ld_look(true);
        let ports = node.is_some_and(|node| node.has_loudness_controls());
        self.ld.ports = ports;
        ports
    }
}

/// The 24 ports of the stage `sections` on both channels, section `first` (1 low, 2 high) of
/// both channels first, then the other.
fn sections_in_order(sections: &[[f64; 5]; 2], first: u8) -> Vec<(String, f64)> {
    let ports = loudness_params(sections);
    let of = |n: u8| {
        let tag = format!("ld{n}:");
        ports
            .iter()
            .filter(move |(name, _)| name.contains(&tag))
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut ordered = of(first);
    ordered.extend(of(3 - first));
    ordered
}

/// Whether two sections are the same within what a read of the graph rounds.
fn same_section(a: &[f64; 5], b: &[f64; 5]) -> bool {
    a.iter()
        .zip(b)
        .all(|(a, b)| (a - b).abs() <= 1e-4 * b.abs() + 1e-6)
}

/// The high section `ports` hold on `channel` as `[b0, b1, b2, a1, a2]` (over `a0`), when they
/// list all of it.
fn held_high(ports: &HashMap<String, f64>, channel: char) -> Option<[f64; 5]> {
    let at = |port: &str| ports.get(&format!("{channel}ld2:{port}")).copied();
    let a0 = at("a0").filter(|a0| *a0 != 0.0)?;
    Some([
        at("b0")? / a0,
        at("b1")? / a0,
        at("b2")? / a0,
        at("a1")? / a0,
        at("a2")? / a0,
    ])
}

/// `low` cut so that it plays under unity with `high` and with each of `held` (as f32 values),
/// or `low` itself when it already does; a flat cut by the highest shelf's peak when no trim
/// does it.
fn cut_for(low: [f64; 5], high: [f64; 5], held: &[[f64; 5]], rate: u32) -> [f64; 5] {
    let f32_of = |value: f64| f64::from(value as f32);
    let highs: Vec<[f64; 5]> = held.iter().copied().chain([high]).collect();
    let worst = |low: &[f64; 5]| {
        highs
            .iter()
            .map(|high| played_peak_db_of(&[*low, *high], &[], rate))
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let peak = worst(&low);
    if peak <= 0.0 {
        return low;
    }
    let scale = 10f64.powf(-peak / 20.0);
    let mut cut = low;
    cut[0] = f32_of(low[0] * scale);
    cut[1] = f32_of(low[1] * scale);
    for _ in 0..4096 {
        if worst(&cut) <= 0.0 {
            return cut;
        }
        cut[0] = f64::from((cut[0] as f32).next_down());
    }
    let top = highs
        .iter()
        .map(|high| played_peak_db_of(&[*high], &[], rate))
        .fold(0.0, f64::max);
    [
        f64::from((f32_of(10f64.powf(-top / 20.0)) as f32).next_down()),
        0.0,
        0.0,
        0.0,
        0.0,
    ]
}
