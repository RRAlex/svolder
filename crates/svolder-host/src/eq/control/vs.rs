// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The virtual speakers in [`EqControl`]: the choice of a set, its lifecycle and its resets.
//!
//! The speakers are one stage inside the computer EQ's one filter chain, after crossfeed's (see
//! [`live`](super::super::live)), so the controller that owns the chain owns them too, and the
//! daemon and a TUI with no daemon behave alike. Crossfeed and the speakers are one choice, Off,
//! Crossfeed or Virtual speakers: turning one on turns the other off in the same update.
//!
//! - **Choose is a rebuild while they play; on rebuilds when the chain lacks the set; off is
//!   live.** A set is the four convolvers' config (the WAV path), so choosing or changing the set
//!   of a chain that plays them applies the chain again, once. A set chosen while the speakers
//!   are off is only kept, so the first on after it applies the chain once (and a flat chain when
//!   the EQ is off); on with the chain already holding the set, and off, are the mixers' gains,
//!   sent live. No chain loads a WAV that is not playing.
//! - **The config holds the convolvers only while the speakers are on**, always with the stage
//!   off (the live sends turn it on), so a chain that starts from it loads the WAV only after a
//!   session that crashed with them on. Turning them off, or any reset, takes the stage out live
//!   and rewrites the config without the convolvers, with no restart: the running chain keeps
//!   its four convolvers, idle, until it next restarts. `speakers_on` in the state tells the next
//!   owner start that a crash left them on, and it rewrites the config.
//! - **The WAV is checked before every apply that names it** ([`Store::check`]): a missing or
//!   damaged one applies the chain without the stage, puts the choice Off with a note, and a
//!   WAV a running chain holds is pinned ([`Store::set_in_use`]) so a store never removes it:
//!   while the chain is applied again with another set, both are pinned, the old one until the
//!   new chain replaces it. The set the running chain holds is kept in the state
//!   (`speakers_loaded`), so the next owner pins it too. A convolver whose file cannot be opened
//!   leaves the chain alive and silent, with no exit, so the check, the pin, and a check at
//!   every rate change and restart (a rebuild that fails is tried again every second) are the
//!   only guards.
//! - **Resets to off**, each with a note and a reason in the view, as crossfeed's: the owner
//!   starts and finds them on, the output they were turned on for is not the output any more,
//!   the chain restarted, a graceful stop ([`EqControl::shutdown`]), the set is missing. A chain
//!   that runs for the space alone is removed by every reset; any other has the stage turned off
//!   live. A rate change is not a reset: the stage is sent again, since the rebuilt graph starts
//!   from its config.

use std::sync::{Arc, Mutex};

use svolder_speakers::{Health, SetMeta, Store, StoreError};

use super::*;
use crate::eq::manage::SpeakersConf;
use crate::eq::speakers::{
    CurveCache, Irs, OffReason as SpeakersOff, Room, SetInfo, SpeakersView, missing_note,
};

/// What the view says when the stage was found on at the owner's start.
const RESTART_NOTE: &str = "virtual speakers turned off: they are off after a restart";

/// How many ticks (10 s at 50 ms) the list of fetched sets is kept before the directory is read
/// again; an operation of the controller, and the catalogue service's
/// [`EqControl::sets_changed`], read it at once.
const SETS_EVERY: u64 = 200;

/// The virtual speakers' part of the controller.
#[derive(Default)]
pub(super) struct Speakers {
    /// Where the sets are; `None` when the controller does not serve the speakers.
    pub store: Option<Arc<Store>>,
    /// The chosen set, kept while they are off.
    pub chosen: Option<String>,
    /// Whether the stage plays.
    pub on: bool,
    /// The `node.name` of the output they were turned on for.
    pub for_output: Option<String>,
    /// What the view says: a reset's sentence, or a rebuild's.
    pub note: Option<String>,
    /// Why they turned themselves off, with `note`.
    pub reason: Option<SpeakersOff>,
    /// The config the chain was last applied or kept with holds the stage ON (the live path
    /// cannot carry it).
    pub conf_on: bool,
    /// The set the config on disk holds convolvers for.
    pub kept: Option<String>,
    /// The set the running chain holds convolvers for, whether or not they play: its WAV is
    /// pinned in the store.
    pub loaded: Option<String>,
    /// The chain has the stage's controls, as last looked at.
    pub ports: bool,
    /// The tick of the last config rewrite that failed.
    pub rewrite_tried: Option<u64>,
    /// A rebuild without a set that cannot be read failed: the tick of the last try and the
    /// set's name. It is tried again every second until an apply takes.
    pub rebuild_owed: Option<(u64, String)>,
    /// The fetched sets, read from the store, and the picture of the chosen one.
    pub cache: Mutex<Cache>,
}

/// What a view of the speakers needs from disk, read at known moments and not at every view.
#[derive(Default)]
pub(super) struct Cache {
    /// The sets that can be played.
    sets: Vec<SetInfo>,
    /// The tick the list was read at.
    at: Option<u64>,
    /// The sets the last full read found missing or damaged.
    damaged: std::collections::BTreeSet<String>,
    /// The chosen set's impulse responses.
    irs: Option<(String, Irs)>,
    /// The chosen set's timing, worked out with its impulse responses and dropped with them.
    timing: Option<svolder_speakers::analysis::Timing>,
    curves: CurveCache,
}

/// One change of the virtual speakers: the fields of `HostEqOp::Speakers`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpeakersEdit {
    /// Turn them on or off; applied last, so turning on plays the set sent with it.
    pub on: Option<bool>,
    /// The id of the fetched set to choose.
    pub set: Option<String>,
    /// Whether this is the last value of a change.
    pub done: bool,
}

/// `meta`, a sidecar as the store reads it, as the engine and the view use it.
fn info_of(meta: &SetMeta) -> SetInfo {
    SetInfo {
        id: meta.id.clone(),
        room: match meta.room {
            svolder_speakers::Room::With => Room::With,
            svolder_speakers::Room::No => Room::No,
        },
        label: meta.label.clone(),
        licence: meta.licence.clone(),
        credit: Some(meta.credit.clone()).filter(|credit| !credit.is_empty()),
        source_url: meta.source_url.clone(),
        rate: meta.rate,
        taps: u32::try_from(meta.taps).unwrap_or(u32::MAX),
        angles: meta
            .angles
            .map(|angles| {
                format!(
                    "left {:.1} deg, right {:.1} deg, {:.1} m",
                    angles.left.az_deg, angles.right.az_deg, angles.left.distance_m
                )
            })
            .unwrap_or_default(),
        loudness_db: meta.loudness_db,
        cut: meta.cut,
    }
}

fn rebuilt_note(label: &str) -> String {
    format!("the computer EQ chain was rebuilt: {label} could not be read")
}

fn not_rebuilt_note(label: &str, failure: &EqFailure) -> String {
    format!("the computer EQ chain could not be rebuilt without {label}: {failure}; trying again")
}

/// Why a set cannot be used, as a refusal.
pub(super) fn unusable(id: &str, error: &StoreError) -> EqFailure {
    refusal(
        match error {
            StoreError::Damaged { reason, .. } => {
                format!("the set {id} is missing or damaged ({reason})")
            }
            other => other.to_string(),
        },
        "Fetch it again from the set list, or choose another",
    )
}

impl EqControl {
    /// This controller serves the virtual speakers from `store`: the view carries them and the
    /// op `Speakers` is taken. The store is the daemon's one shared store, so a set the chain
    /// plays is pinned for the catalogue service too.
    pub fn with_speakers(mut self, store: Arc<Store>) -> EqControl {
        self.sp.store = Some(store);
        self.pin();
        self.refresh_sets();
        self.publish();
        self
    }

    /// [`EqControl::with_speakers`] over a store of its own in `dir`: for a controller that is
    /// the only owner of the sets (the TUI with no daemon) and for tests. The daemon holds one
    /// store for the controller and the catalogue service, and passes it to `with_speakers`.
    pub fn with_speakers_dir(self, dir: &std::path::Path) -> EqControl {
        self.with_speakers(Arc::new(Store::new(dir)))
    }

    /// The store this controller plays sets from; the pin on the set a running chain holds lives
    /// in it, so the catalogue service must use this very store.
    #[cfg(test)]
    pub fn speakers_store(&self) -> Option<Arc<Store>> {
        self.sp.store.clone()
    }

    /// The catalogue service fetched, imported or forgot a set: the list is read again.
    pub fn sets_changed(&mut self) {
        {
            let mut cache = crate::locked(&self.sp.cache);
            cache.curves = CurveCache::default();
            cache.irs = None;
            cache.timing = None;
        }
        self.refresh_sets();
        self.publish();
    }

    /// Whether the controller serves the speakers at all.
    #[cfg(test)]
    pub fn serves_speakers(&self) -> bool {
        self.sp.store.is_some()
    }

    /// Whether a stage of the space (crossfeed or the speakers) plays.
    pub(super) fn space_on(&self) -> bool {
        self.xf_on || self.sp.on
    }

    /// Mark what a running chain needs from the store: its set's WAV is not to be removed.
    pub(super) fn pin(&self) {
        self.pin_also(None);
    }

    /// [`EqControl::pin`], and `next` with it: the set an apply is about to load, pinned from
    /// before its check until the chain that holds the old one is replaced.
    fn pin_also(&self, next: Option<&str>) {
        if let Some(store) = &self.sp.store {
            let ids: Vec<&str> = self.sp.loaded.as_deref().into_iter().chain(next).collect();
            store.set_in_use(&ids);
        }
    }

    /// Read the sets again, each one's WAV checked, and the chosen one's responses.
    pub(super) fn refresh_sets(&mut self) {
        self.read_sets(true);
    }

    /// Read the sets again: with `checked`, each one's WAV is read and checked against its
    /// sidecar; without, only the sidecars are read and the WAVs looked for (the full check is
    /// made at every choice and apply, and a failure there hides the set).
    fn read_sets(&mut self, checked: bool) {
        let Some(store) = self.sp.store.clone() else {
            return;
        };
        let mut cache = crate::locked(&self.sp.cache);
        let metas: Vec<SetMeta> = match checked {
            true => {
                let (sound, damaged): (Vec<_>, Vec<_>) = store
                    .list()
                    .into_iter()
                    .partition(|status| status.health == Health::Ok);
                cache.damaged = damaged.into_iter().map(|status| status.id).collect();
                sound.into_iter().filter_map(|status| status.meta).collect()
            }
            // A set the last check found damaged stays hidden until it is checked again.
            false => store
                .list_sidecars()
                .into_iter()
                .filter(|meta| !cache.damaged.contains(&meta.id))
                .collect(),
        };
        let sets: Vec<SetInfo> = metas.iter().map(info_of).collect();
        let wanted = self
            .sp
            .chosen
            .clone()
            .filter(|id| sets.iter().any(|set| &set.id == id));
        if cache.irs.as_ref().map(|(id, _)| id) != wanted.as_ref() {
            cache.irs = wanted.and_then(|id| {
                let bytes = std::fs::read(store.wav_path(&id).ok()?).ok()?;
                Some((id, Irs::from_wav(&bytes).ok()?))
            });
            // Once per set, here with the file read and never on a view.
            cache.timing = cache.irs.as_ref().and_then(|(id, irs)| {
                let set = sets.iter().find(|set| &set.id == id)?;
                let with_room = set.room == Room::With;
                Some(svolder_speakers::analysis::analyse(
                    &irs.taps,
                    irs.rate_hz,
                    with_room,
                ))
            });
        }
        cache.sets = sets;
        cache.at = Some(self.ticks);
    }

    /// Read the sets again when the list is old: the sidecars only, on the controller's clock.
    pub(super) fn refresh_sets_when_old(&mut self) {
        let old = crate::locked(&self.sp.cache)
            .at
            .is_none_or(|at| self.ticks >= at + SETS_EVERY);
        if self.sp.store.is_some() && old {
            self.read_sets(false);
        }
    }

    /// Take the speakers from `state`: the chosen set, the set the running chain holds, and a
    /// stage that is never on from a file. A state that says they play was left by a run that
    /// did not stop gracefully; a change made from outside while they played took them off.
    pub(super) fn adopt_speakers(&mut self, state: &HostEqState) {
        let was_on = std::mem::take(&mut self.sp.on);
        self.sp.chosen = state.speakers.clone();
        self.sp.for_output = None;
        // A config written while they were on holds the convolvers.
        self.sp.kept = state.speakers_on.then(|| state.speakers.clone()).flatten();
        // The set the chain was built with, which may not be the one chosen since.
        self.sp.loaded = state
            .speakers_loaded
            .clone()
            .or_else(|| self.sp.kept.clone());
        self.pin();
        if state.speakers_on {
            self.reset_sp(SpeakersOff::Restart, RESTART_NOTE.to_string(), true);
        } else if was_on {
            let plays = self.stage_on_node || self.sp.conf_on;
            self.reset_sp(
                SpeakersOff::ChainRestarted,
                "virtual speakers turned off: the computer EQ was applied again from outside"
                    .to_string(),
                plays,
            );
        }
    }

    /// Set the speakers off in the state and the view, with `why` and `note`, and owe the chain
    /// the rest ([`EqControl::finish_reset`]); `plays` says whether the stage may be on it.
    pub(super) fn reset_sp(&mut self, why: SpeakersOff, note: String, plays: bool) {
        // The stage goes off by itself: the sound climbs back as it does for a hand's off.
        self.arm_climb();
        self.sp.on = false;
        self.sp.for_output = None;
        self.sp.note = Some(note);
        self.sp.reason = Some(why);
        self.owe(plays);
    }

    /// Owe the chain a reset, keeping what an earlier one owes.
    pub(super) fn owe(&mut self, plays: bool) {
        let before = self.owed.take().is_some_and(|owed| owed.plays);
        self.owed = Some(Owed {
            plays: plays || before,
            tried: None,
        });
    }

    /// Compare the output the speakers were turned on for with the one the last look saw.
    pub(super) fn check_speakers_output(&mut self, now: &OutputSink) {
        if !self.sp.on {
            return;
        }
        match self.sp.for_output.as_deref() {
            Some(was) if was != now.name => {
                let name = now.description.clone().unwrap_or_else(|| now.name.clone());
                self.reset_sp(
                    SpeakersOff::OutputChanged,
                    format!("virtual speakers turned off: the output changed to {name}"),
                    true,
                );
            }
            Some(_) => {}
            None => self.sp.for_output = Some(now.name.clone()),
        }
    }

    /// The stage the next config holds, when the speakers are on: the set's WAV, with the stage
    /// on only when the live path cannot carry it.
    pub(super) fn speakers_conf(&self) -> Option<SpeakersConf> {
        if !self.sp.on {
            return None;
        }
        let id = self.sp.chosen.clone()?;
        let wav = self.sp.store.as_ref()?.wav_path(&id).ok()?;
        // The config holds the stage off, which the live sends turn on, unless they cannot.
        let on = self.force_conf || !self.stage_by_live();
        Some(SpeakersConf { id, wav, on })
    }

    /// The WAV of the set an apply is about to name is checked: one that cannot be played turns
    /// the speakers off with a note, so the chain is applied without the stage. The set is
    /// pinned for the apply, beside the one the running chain holds. Whether it turned them
    /// off, which an apply that then fails owes the running chain.
    pub(super) fn check_set_for_apply(&mut self) -> bool {
        if !self.sp.on {
            return false;
        }
        let (Some(store), Some(id)) = (self.sp.store.clone(), self.sp.chosen.clone()) else {
            self.sp.on = false;
            return true;
        };
        self.pin_also(Some(&id));
        let failed = store.check(&id).err();
        if let Some(error) = &failed {
            let label = self.set_label(&id);
            let why = match &error {
                StoreError::Damaged { .. } => missing_note(&label),
                other => format!("virtual speakers off: {other}"),
            };
            self.sp.on = false;
            self.sp.for_output = None;
            self.sp.note = Some(why);
            self.sp.reason = Some(SpeakersOff::SetMissing);
            self.pin();
            self.refresh_sets();
        }
        failed.is_some()
    }

    /// The name a person knows the set by.
    pub(super) fn set_label(&self, id: &str) -> String {
        crate::locked(&self.sp.cache)
            .sets
            .iter()
            .find(|set| set.id == id)
            .map_or_else(|| id.to_string(), |set| set.label.clone())
    }

    /// What the chain holds, after an apply with `conf` in its config.
    pub(super) fn speakers_applied(&mut self, conf: Option<&SpeakersConf>) {
        let id = conf.map(|conf| conf.id.clone());
        self.sp.conf_on = conf.is_some_and(|conf| conf.on);
        self.sp.kept = id.clone();
        self.sp.loaded = id;
        // A chain built with the convolvers lists the stage's controls.
        self.sp.ports = self.sp.loaded.is_some();
        // A chain just built opens no WAV that is gone.
        self.sp.rebuild_owed = None;
        self.pin();
    }

    /// What the config on disk holds, after it was kept with `conf`.
    pub(super) fn speakers_kept(&mut self, conf: Option<&SpeakersConf>) {
        self.sp.kept = conf.map(|conf| conf.id.clone());
        self.sp.conf_on = conf.is_some_and(|conf| conf.on);
    }

    /// The chain is gone or replaced by another process: it holds what its config on disk says.
    pub(super) fn speakers_restarted(&mut self) {
        self.sp.loaded = self.sp.kept.clone();
        self.pin();
    }

    /// No chain runs: nothing holds a WAV and no stage plays.
    pub(super) fn speakers_gone(&mut self) {
        self.sp.on = false;
        self.sp.for_output = None;
        self.sp.conf_on = false;
        self.sp.loaded = None;
        self.sp.kept = None;
        self.sp.ports = false;
        self.sp.rebuild_owed = None;
        self.pin();
    }

    /// Whether the running chain has the stage's controls, from a fresh look at the graph.
    pub(super) async fn has_speakers_ports(&mut self) -> bool {
        self.params.forget_node();
        let ports = self
            .params
            .node()
            .await
            .is_some_and(|node| node.has_speakers_controls());
        self.sp.ports = ports;
        if ports && self.sp.loaded.is_none() {
            // Another process built it: whatever set it holds is taken to be the chosen one.
            self.sp.loaded = self.sp.chosen.clone();
            self.pin();
        } else if !ports && self.sp.loaded.is_some() {
            self.sp.loaded = None;
            self.pin();
        }
        ports
    }

    /// The config on disk holds convolvers for a set that is not playing (the speakers went off
    /// and the chain was not restarted): it is written again without them. A write that fails is
    /// tried again a second later.
    pub(super) async fn rewrite_speakers_conf(&mut self) {
        if self.sp.kept.is_none() || self.sp.on || !self.runs {
            return;
        }
        if self
            .sp
            .rewrite_tried
            .is_some_and(|at| self.ticks < at + WATCH_EVERY)
        {
            return;
        }
        // A config write that fails leaves `kept` as it was, and `save` says so only for the
        // state file: what the config holds decides.
        let _ = self.save().await;
        self.sp.rewrite_tried = self.sp.kept.is_some().then_some(self.ticks);
    }

    /// The set a chain whose graph was rebuilt (a new rate, a restart) is about to re-open: a WAV
    /// that cannot be played now leaves the chain alive and silent, so the chain is applied again
    /// without the stage. Whether it was.
    pub(super) async fn rebuild_for_missing_set(&mut self) -> bool {
        let (Some(store), Some(id)) = (self.sp.store.clone(), self.sp.loaded.clone()) else {
            return false;
        };
        let Err(error) = store.check(&id) else {
            return false;
        };
        let label = self.set_label(&id);
        let why = match &error {
            StoreError::Damaged { .. } => missing_note(&label),
            other => format!("virtual speakers off: {other}"),
        };
        let was_on = self.sp.on;
        if was_on {
            self.sp.on = false;
            self.sp.for_output = None;
            self.sp.reason = Some(SpeakersOff::SetMissing);
            self.sp.note = Some(why);
        }
        // The set's files are gone, so there is nothing to pin: the chain is applied again
        // without the stage (the choice is not one a config may name).
        self.sp.loaded = None;
        self.pin();
        self.refresh_sets();
        match self.apply().await {
            Ok(()) => {
                if !was_on {
                    self.note = Some(rebuilt_note(&label));
                }
                let _ = self.save().await;
                true
            }
            Err(failure) => {
                self.note = Some(not_rebuilt_note(&label, &failure));
                self.sp.rebuild_owed = Some((self.ticks, label));
                false
            }
        }
    }

    /// A rebuild without a set that cannot be read that failed is tried again, once a second,
    /// until an apply takes: the chain that holds the set plays nothing.
    pub(super) async fn retry_rebuild(&mut self) {
        let Some((at, label)) = self.sp.rebuild_owed.clone() else {
            return;
        };
        if !self.runs {
            self.sp.rebuild_owed = None;
            return;
        }
        if self.ticks < at + WATCH_EVERY {
            return;
        }
        match self.apply().await {
            Ok(()) => {
                self.note = Some(rebuilt_note(&label));
                let _ = self.save().await;
            }
            Err(failure) => {
                self.note = Some(not_rebuilt_note(&label, &failure));
                self.sp.rebuild_owed = Some((self.ticks, label));
            }
        }
    }

    /// The chosen set's level against Off in dB (at most 0), from the catalogue alone: 0 with none
    /// chosen or the catalogue not read yet.
    pub(super) fn chosen_set_level_db(&self) -> f64 {
        let cache = crate::locked(&self.sp.cache);
        self.sp
            .chosen
            .as_ref()
            .and_then(|id| cache.sets.iter().find(|set| &set.id == id))
            .map_or(0.0, |set| set.loudness_db.min(0.0))
    }

    /// The speakers as a screen shows it, `None` when the controller does not serve them.
    pub(super) fn speakers_view(&self) -> Option<SpeakersView> {
        self.sp.store.as_ref()?;
        let rate = self.rate();
        let mut cache = crate::locked(&self.sp.cache);
        let Cache {
            sets,
            irs,
            curves,
            timing,
            ..
        } = &mut *cache;
        let timing = timing.clone();
        let chosen = self.sp.chosen.as_ref().and_then(|id| {
            let info = sets.iter().find(|set| &set.id == id)?;
            let irs = irs.as_ref().filter(|(held, _)| held == id)?;
            Some((info, curves.get(id, &irs.1, rate).clone()))
        });
        let mut view = SpeakersView::of(
            chosen.as_ref().map(|(info, curve)| (*info, curve)),
            self.sp.on,
            sets,
            self.sp.note.clone(),
        );
        view.off_reason = self.sp.reason.map(|why| why.word().to_string());
        view.timing = chosen.as_ref().and(timing);
        view.unavailable = self
            .chain
            .unavailable()
            .map(|failure| format!("computer controls unavailable: {}", failure.what))
            .or_else(|| self.legacy.then(|| LEGACY_NOTE.to_string()))
            .or_else(|| self.held.then(|| HELD_NOTE.to_string()));
        Some(view)
    }

    /// Change the virtual speakers: the chosen set and whether they play. See [`HostEqOp::Speakers`'s
    /// rules](SpeakersEdit): `on` is applied last, a set that is not fetched is refused, and
    /// turning them on turns crossfeed off in the same update (and the other way round).
    pub async fn speakers(&mut self, edit: SpeakersEdit) -> Result<(), EqFailure> {
        self.rise_ok_until = self.ticks + RISE_OK_TICKS;
        let result = self.speakers_inner(edit).await;
        self.disarm_climb();
        self.refresh_sets();
        self.end_bypass_if_on();
        self.publish();
        result
    }

    async fn speakers_inner(&mut self, edit: SpeakersEdit) -> Result<(), EqFailure> {
        self.finish_reset().await;
        if edit.on.is_none() && edit.set.is_none() {
            return Err(refusal(
                "the virtual speakers change names nothing",
                "Name on, or a set to choose",
            ));
        }
        let Some(store) = self.sp.store.clone() else {
            return Err(refusal(
                "virtual speakers are not available here",
                "Leave them off",
            ));
        };
        if let Some(failure) = self.chain.unavailable() {
            return Err(failure);
        }
        if self.legacy {
            return Err(refusal(
                "an older svolder's computer EQ is playing, and virtual speakers cannot join it",
                "Turn the computer EQ off and on, which replaces it",
            ));
        }
        if self.held {
            return Err(refusal(
                HELD_NOTE,
                "Turn the computer EQ off, or change the output",
            ));
        }
        let target = match &edit.set {
            Some(id) => {
                store.check(id).map_err(|error| unusable(id, &error))?;
                Some(id.clone())
            }
            None => self.sp.chosen.clone(),
        };
        let want = edit.on.unwrap_or(self.sp.on);
        if want && target.is_none() {
            return Err(refusal(
                "no set is chosen for the virtual speakers",
                "Choose a set from the list first",
            ));
        }
        if want
            && let Some(refused) =
                over_stage_rate("virtual speakers run", self.stage_start_rate().await)
        {
            return Err(refused);
        }
        if want && let Some(id) = &target {
            // A set that is chosen and not playing is not checked until now.
            store.check(id).map_err(|error| unusable(id, &error))?;
        }
        let was = (
            self.sp.chosen.clone(),
            self.sp.note.take(),
            self.sp.reason.take(),
            self.xf_on,
            self.xf_for.clone(),
            self.xf_note.clone(),
        );
        let result = match (self.sp.on, want) {
            (false, false) => {
                self.sp.chosen = target;
                self.save_settings().await
            }
            (true, true) if target == self.sp.chosen => Ok(()),
            (true, true) => self.sp_choose(target).await,
            (false, true) => self.sp_start(target).await,
            (true, false) => {
                self.arm_climb();
                self.sp_stop(target).await
            }
        };
        if result.is_err() {
            (
                self.sp.chosen,
                self.sp.note,
                self.sp.reason,
                self.xf_on,
                self.xf_for,
                self.xf_note,
            ) = was;
            // A line the change handed to `pw-cli` may land yet (the session repairs a line that
            // failed with the next one): the stage as it stands goes with the next line.
            if self.runs && self.sp.ports && self.stage_by_live() {
                self.stage_dirty = true;
            }
        } else if (edit.set.is_some() || edit.on == Some(true))
            && let Some(id) = &self.sp.chosen
        {
            // A set chosen or turned on is used: the store evicts the least recently used.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs());
            let _ = store.touch(id, now);
        }
        result
    }

    /// Turn the stage on with `target`, the crossfeed off in the same update.
    async fn sp_start(&mut self, target: Option<String>) -> Result<(), EqFailure> {
        let before = (
            std::mem::replace(&mut self.sp.chosen, target),
            self.flat,
            self.runs,
        );
        let crossfeed_was_on = self.xf_on;
        if crossfeed_was_on {
            self.xf_on = false;
            self.xf_for = None;
            self.xf_note = Some("crossfeed turned off: virtual speakers are on".to_string());
        }
        self.sp.on = true;
        let result = self.sp_start_inner(crossfeed_was_on).await;
        if result.is_err() {
            self.sp.on = false;
            self.stage_dirty = false;
            self.force_conf = false;
            self.xf_on = crossfeed_was_on;
            let applied_flat = !before.2 && self.runs;
            (self.sp.chosen, self.flat, self.runs) = before;
            if applied_flat {
                // A flat chain applied for this start goes again.
                self.runs = true;
                self.flat = true;
                if self.turn_off().await.is_err() {
                    self.owe(true);
                    if let Some(owed) = self.owed.as_mut() {
                        owed.tried = Some(self.ticks);
                    }
                }
            } else if crossfeed_was_on && self.runs {
                // The crossfeed plays still, unless the chain was applied: it is sent again.
                self.stage_dirty = true;
            }
        }
        result
    }

    async fn sp_start_inner(&mut self, crossfeed_was_on: bool) -> Result<(), EqFailure> {
        let wanted = self.sp.chosen.clone();
        if !self.runs {
            // The EQ is off: the chain plays flat for the speakers alone.
            self.flat = true;
            self.apply().await?;
        } else if !(self.stage_by_live()
            && self.sp.loaded == wanted
            && self.has_speakers_ports().await)
        {
            self.apply().await?;
            if self.sp.on {
                self.sp.note = Some("the computer EQ chain was rebuilt to load the set".into());
            }
        }
        self.sp_finish_start(crossfeed_was_on).await
    }

    /// The chain is there with the set in it: the stage goes in (the crossfeed's off with it).
    async fn sp_finish_start(&mut self, crossfeed_was_on: bool) -> Result<(), EqFailure> {
        if !self.sp.on {
            // The set could not be read at the apply: the chain plays without the stage.
            return self.save().await;
        }
        self.look_output().await;
        self.sp.for_output = self.output.as_ref().map(|out| out.name.clone());
        self.stage_dirty = !self.sp.conf_on || crossfeed_was_on;
        if self.stage_dirty && self.stage_by_live() && !self.send_dirty().await {
            // The send did not take: the chain holds the stage in its config instead.
            self.stage_dirty = false;
            self.force_conf = true;
            self.apply().await?;
        }
        self.save().await
    }

    /// Another set while the stage plays: the chain is applied again with it, once.
    async fn sp_choose(&mut self, target: Option<String>) -> Result<(), EqFailure> {
        let before = std::mem::replace(&mut self.sp.chosen, target);
        let result = self.sp_choose_inner().await;
        if result.is_err() {
            self.sp.chosen = before;
            self.pin();
        }
        result
    }

    async fn sp_choose_inner(&mut self) -> Result<(), EqFailure> {
        self.apply().await?;
        if self.sp.on {
            self.sp.note = Some("the computer EQ chain was rebuilt to load the set".into());
        }
        self.sp_finish_start(false).await
    }

    /// Turn the stage off with `target` kept as the choice: a chain that runs for the space alone
    /// is removed (or kept as a pass-through for a stream), any other has the stage taken out
    /// live and its config rewritten with no convolvers, or is applied again without them.
    async fn sp_stop(&mut self, target: Option<String>) -> Result<(), EqFailure> {
        let before = std::mem::replace(&mut self.sp.chosen, target);
        let result = self.sp_stop_inner().await;
        if result.is_err() {
            self.sp.chosen = before;
        }
        result
    }

    async fn sp_stop_inner(&mut self) -> Result<(), EqFailure> {
        // Loudness on keeps a flat chain: the stage goes off as from any chain that stays.
        if self.flat && !self.ld_keeps_chain() {
            let was_for = self.sp.for_output.take();
            self.sp.on = false;
            match self.keep_passthrough().await {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(failure) => {
                    self.sp.on = true;
                    self.sp.for_output = was_for;
                    return Err(failure);
                }
            }
            let removed = self.turn_off().await;
            if removed.is_err() {
                self.sp.on = true;
                self.sp.for_output = was_for;
            }
            return removed;
        }
        self.sp.on = false;
        self.sp.for_output = None;
        if !self.sp.conf_on && self.stage_by_live() {
            if !self.has_speakers_ports().await {
                // No convolver is in the graph: nothing plays, only the config is left to write.
                return self.save().await;
            }
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
                self.sp.on = true;
                Err(failure)
            }
        }
    }
}
