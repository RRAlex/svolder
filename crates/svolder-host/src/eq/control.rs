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
//! so the curve never plays above unity, even for the moment between two updates. That holds for
//! whole updates, a line PipeWire applies whole. PipeWire applies a live line port by port, so a
//! quantum can play a half-applied line (a documented known issue).
//!
//! The crossfeed is a stage of the same chain and the same controller owns it (see the module
//! `xf`): `crossfeed()` changes its settings and turns it on or off, the chain runs while the EQ
//! or the crossfeed is on, and the preamp is the EQ's less the stage's worst gain, ordered in every
//! update so that no whole update plays above unity. The crossfeed is off after the owner starts,
//! after the output changes, after the chain restarts and after a graceful stop
//! ([`EqControl::shutdown`]).
//!
//! The loudness is a stage of the same chain too (see the module `ld`): it follows the computer's
//! volume on the ordinary lines, and it is cut-only at every rate, so it never moves the preamp.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use tokio::sync::watch;

use serde::{Deserialize, Serialize};
use svolder_profiles::tone::apo::{self, Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{from_computer, host_correction};

use super::HostEqState;
use super::crossfeed::{CrossfeedSettings, CrossfeedView, OffReason, Stage};
use super::live::{
    CONFIG_RATE_HZ, NodeKey, OutputSink, SHAPE, SLOTS, SlotShape, crossfeed_params, preamp_params,
    slot_params, speakers_params,
};
use super::loudness::LoudnessView;
use super::manage::{
    ApplyRequest, CONFIG_STAGE_RATES, Chain, EqFailure, STAGE_RATE_MAX_HZ, SpeakersConf,
    check_playable, config_stage_cost_db, played_preamp,
};
use super::points::{Change, Points};
use super::session::Params;
use super::speakers::OffReason as SpeakersOff;
use super::speakers::SpeakersView;

/// What a live edit that did not take says, once.
pub const LIVE_NOTE: &str = "this computer's PipeWire takes EQ changes when a drag ends";

/// What the view says while a chain from an older svolder plays.
pub const LEGACY_NOTE: &str =
    "an older svolder's computer EQ is playing; turning the computer EQ off and on replaces it";

/// What the crossfeed's view says while the EQ's chain waits for a sample rate its curve plays at.
const HELD_NOTE: &str = "the computer EQ waits for a sample rate its curve plays at";

/// What the view says while the chain plays as a pass-through, kept for a stream that plays.
pub const PASSTHROUGH_NOTE: &str = "chain kept as a pass-through until idle";

/// What the view says while a recall plays with the preamp held at the running chain's cap: the
/// chain is applied again, with the curve's own preamp, once nothing has played through it for the
/// idle time (30 s).
pub const LEVEL_HELD_NOTE: &str = "level held until the music pauses";

/// How many ticks (30 s at 50 ms) nothing has to play through a pass-through chain before it is
/// removed, and through a chain whose level is held before it is applied again.
const IDLE_KEEP: u64 = 600;

/// The environment variable that sets the idle time in ticks, for the end-to-end run only (a
/// run cannot wait 30 s at every step).
pub const IDLE_TICKS_ENV: &str = "SVOLDER_EQ_IDLE_TICKS";

/// The name of a curve a hand has changed.
pub const CUSTOM: &str = "custom";

/// How many ticks apart the chain is looked at again (one `pw-dump`): 20 ticks of 50 ms.
///
/// Time is counted in ticks, so the periods below hold only as long as the owner calls
/// [`EqControl::tick`] every 50 ms: the caller drives the period.
const WATCH_EVERY: u64 = 20;

/// How many reads of the checked port in a row must miss before live updates are given up.
const READ_BACK_MISSES: u32 = 3;
/// How many values sent after the check was armed it remembers.
const READ_BACK_LATER_MAX: usize = READ_BACK_MISSES as usize + 1;

/// The read-back of the first live value: the port, the value it was armed with and the values
/// sent for it since.
#[derive(Debug)]
struct ReadBack {
    key: String,
    armed: f64,
    later: Vec<f64>,
}

/// How many ticks after a send a look may take the chain's values as gone back to its config's:
/// `pw-cli` applies a line a moment after it is written.
const REVERT_QUIET: u64 = 4;

/// How many reverts inside [`FAIL_WINDOW`] are answered with a new send before the crossfeed is
/// given up on.
const REVERT_LIMIT: usize = 4;

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
    /// How many times a preset was loaded or applied since the controller started, and the state
    /// file was read again after another process changed it: a front end that holds a value
    /// while a preset of the same name comes in sees the number move. 0 from a peer that has
    /// none.
    #[serde(default)]
    pub loads: u64,
    /// The crossfeed, when the controller serves it: its settings, ranges and models. `None`
    /// from a controller or a peer that has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crossfeed: Option<CrossfeedView>,
    /// The virtual speakers, when the controller serves them: the chosen set, the fetched sets
    /// and the picture. `None` from a controller or a peer that has none, and then no front end
    /// offers them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speakers: Option<SpeakersView>,
    /// The computer's loudness: its settings, the readout and the ranges. Always there from a
    /// controller; `None` from a peer that has none, and then no front end offers it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loudness: Option<LoudnessView>,
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

/// One change of the crossfeed: the fields of `HostEqOp::Crossfeed`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CrossfeedEdit {
    /// Turn it on or off; applied last, so turning on plays the numbers sent with it.
    pub on: Option<bool>,
    /// The model's wire word.
    pub model: Option<String>,
    /// A preset of the model.
    pub preset: Option<String>,
    /// The share of the other channel in percent.
    pub amount_pct: Option<f64>,
    /// The crossover in Hz.
    pub crossover_hz: Option<f64>,
    /// The total lag in ms.
    pub delay_ms: Option<f64>,
    /// Whether this is the last value of a drag; a drag is heard, not saved.
    pub done: bool,
}

/// What identifies a state file's contents without reading it.
type Stamp = (SystemTime, u64, u64);

/// What the chain was last known to run as, for the values sent to it.
#[derive(Debug, Clone, Copy, Default)]
struct Sent {
    /// The node the values went to; a restart of the chain is another one, under a new id or
    /// the same one (see [`NodeKey`]).
    node: Option<NodeKey>,
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
    /// Whether a chain runs, the EQ's or the one that runs for crossfeed alone (`flat`); the EQ's
    /// own switch is [`EqControl::eq_on`].
    runs: bool,
    /// Whether the chain's running could not be told when it was last looked at.
    unknown: bool,
    live: bool,
    note: Option<String>,
    /// Why the settings file kept beside the state was ignored at start, for a caller that logs
    /// it ([`EqControl::settings_unread`]).
    settings_unread: Option<String>,
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
    /// What the crossfeed stage `loaded` holds on takes off the config's preamp, in dB (0 when the
    /// config holds it off, which a live chain's always does), and the same for `kept`.
    loaded_stage_db: f64,
    kept_stage_db: f64,
    /// The crossfeed stage `loaded` holds on, whatever the settings say since (a change of the
    /// sink's rate puts it back on the chain), and the same for `kept`.
    loaded_xf: Option<CrossfeedSettings>,
    kept_xf: Option<CrossfeedSettings>,
    /// The crossfeed stage the last line that carried one put on the node, with the rate it was
    /// worked out at, which may not be the rate the node runs at now; `None` after a line with
    /// the bypass and on a node that started from its config.
    stage_sent: Option<(CrossfeedSettings, u32)>,
    /// The preamp `loaded` needs at a rate, until it changes.
    cap: Mutex<Option<(u32, f64)>>,
    /// The node in the graph has not been looked at since it was applied, adopted or started
    /// again: another process may have put it there, so what it was started from is not known
    /// and the next look that finds its graph run takes its preamp as a bound (a graph that has
    /// not run lists every control at 0).
    unvouched: bool,
    /// The preamp the node held when it was first looked at, in dB. It suits the slots the node
    /// starts from, which a change of rate puts back, so no preamp above it is sent until an
    /// apply says what the chain is.
    node_cap: Option<f64>,
    /// The last look found the node's graph not run: what was sent to it is not on it, and the
    /// graph starts from its config when it runs.
    not_run: bool,
    /// The node is the chain this controller applied, not one another process started since.
    own_chain: bool,
    unavailable: Option<String>,
    /// The slots that moved since the last send.
    dirty: BTreeSet<u8>,
    sent: Sent,
    /// The newest value sent for each port.
    last_values: HashMap<String, f64>,
    /// The ports of `last_values` known to have taken the value sent: a look found them at it,
    /// or a look saw its line, or a later one, land.
    confirmed: HashSet<String>,
    /// How many lines have gone through, and for each port of `last_values` the number of the
    /// line that last changed its value and the tick it went on.
    lines: u64,
    line_of: HashMap<String, (u64, u64)>,
    /// The tick of the last look that compared the chain with what was sent.
    compared_at: u64,
    /// The newest line whose ports were sent again because a look still found them off their
    /// values a look after they went (see `reverted`).
    chased: u64,
    /// The port whose value is to be read back, once.
    check: Option<ReadBack>,
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
    /// The chain that plays was started by someone else (this controller was just made over a
    /// state that says on): the first look at it sends the saved state when it holds other values.
    foreign: bool,
    /// The presets loaded and the state files read again so far, for [`EqView::loads`].
    loads: u64,
    /// The wait for the chain's end.
    exit: exit::ExitWatch,
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
    /// The crossfeed's settings, kept while it is off.
    xf: CrossfeedSettings,
    /// The settings are written to the state file: they were read from it or set by a request.
    xf_kept: bool,
    /// Whether the stage plays.
    xf_on: bool,
    /// The `node.name` of the output the stage was turned on for.
    xf_for: Option<String>,
    /// The output the computer plays into, as the last look at the graph saw it.
    output: Option<OutputSink>,
    /// What the view says about the crossfeed: a reset's sentence, or a rebuild's.
    xf_note: Option<String>,
    /// Why the crossfeed turned itself off, with `xf_note`.
    xf_reason: Option<OffReason>,
    /// The chain runs for crossfeed or the virtual speakers alone (or as a pass-through): the
    /// EQ plays flat, its points kept.
    flat: bool,
    /// The tick at which a stream was last seen playing through the pass-through chain.
    played_at: u64,
    /// How many ticks nothing must play through a pass-through chain before it is removed, and
    /// through a chain whose level is held before it is applied again.
    idle_keep: u64,
    /// The chain is to be applied again with the points as they stand once nothing has played
    /// through it for `idle_keep` ticks: a recall went to it live while a stream played, its
    /// preamp held at the cap of the curve the chain was started with (or this controller started
    /// over a chain another run left holding a lower preamp than the saved state needs). Judged
    /// only once the node has been looked at: a cap that no longer holds the preamp down drops it.
    lift: bool,
    /// The config on disk may hold the stage on, so a chain that starts again plays it.
    xf_conf: bool,
    /// The stage, with the preamp that goes with it, is to be sent with the next update.
    stage_dirty: bool,
    /// The chain has the stage's controls, as last looked at; a chain from before the stage
    /// does not.
    xf_ports: bool,
    /// A reset the chain still owes.
    owed: Option<Owed>,
    /// The next apply holds the stage in its config whatever the live path says: a live send
    /// of the stage failed.
    force_conf: bool,
    /// The stage was last sent ON to the node the chain has now (and not taken off since): it may
    /// play there whatever the config holds.
    stage_on_node: bool,
    /// The tick at which the stage goes out once more: PipeWire does not play a stage sent while
    /// the chain's stream is being replaced (about a second), though the ports then hold the
    /// values and read back right, so every stage send is repeated once a second later.
    resend_at: Option<u64>,
    /// The send under way is that repeat, which is not repeated again.
    resending: bool,
    /// A send since the last repeat carried the stage, so the repeat carries it too.
    resend_stage: bool,
    /// The tick of the last send that went through.
    sent_at: u64,
    /// The ticks on which a look found the chain's values gone back to its config's, newest last.
    reverts: VecDeque<u64>,
    /// The next send looks at the node afresh first: a stage that is turned on, changed or
    /// turned off reaches the chain at the rate it runs at now, not the one last seen (a drag's
    /// updates go on what the last look saw, at most a second old).
    look_first: bool,
    /// The newest worst gain worked out, with the settings and rate it is for.
    gain_cache: Mutex<Option<(CrossfeedSettings, u32, f64)>>,
    /// The virtual speakers (see the module `vs`).
    sp: vs::Speakers,
    /// The loudness's settings (see the module `ld`).
    ld: ld::Loudness,
}

/// A crossfeed reset whose part on the chain is still to be done: the state, the view and the
/// reason are already set.
#[derive(Debug, Clone)]
struct Owed {
    /// Whether the stage may be playing on the chain (a chain that restarted plays its config,
    /// which holds the stage only when `xf_conf` says so).
    plays: bool,
    /// The tick of the last try that failed.
    tried: Option<u64>,
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

/// The word a point's type goes by on the wire and on screens: `PK`, `LSC` or `HSC`.
pub fn kind_word(kind: FilterKind) -> &'static str {
    match kind {
        FilterKind::Peaking => "PK",
        FilterKind::LowShelf => "LSC",
        FilterKind::HighShelf => "HSC",
    }
}

/// The point type a word names, the one reading of it for every front end: `PK`, `LSC` or `HSC`
/// in any case (and the other spellings of [`FilterKind`]'s parser), around blanks. Refused in
/// words that say what is allowed.
pub fn parse_kind(word: &str) -> Result<FilterKind, EqFailure> {
    word.trim()
        .parse()
        .map_err(|_| refusal(format!("{word} is not a filter type"), "Use PK, LSC or HSC"))
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
        // A state with no table for a setting kept before any state existed (a no-daemon apply
        // or a crash wrote it) takes the kept one: the settings file is never deleted unread.
        let (kept, unreadable) = match &keep {
            Keep::File(path) => match super::HostEqSettings::load(path) {
                Ok(kept) => (kept, None),
                Err(why) => (None, Some(why)),
            },
            Keep::Memory => (None, None),
        };
        let state = match (&kept, state) {
            (Some(kept), Some(mut state)) => {
                kept.fill(&mut state);
                Some(state)
            }
            (_, state) => state,
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
            runs: false,
            unknown: false,
            live: true,
            note: None,
            legacy: false,
            held: false,
            loaded: None,
            kept: None,
            loaded_stage_db: 0.0,
            kept_stage_db: 0.0,
            loaded_xf: None,
            kept_xf: None,
            stage_sent: None,
            cap: Mutex::new(None),
            unvouched: false,
            node_cap: None,
            not_run: false,
            own_chain: false,
            unavailable: None,
            dirty: BTreeSet::new(),
            sent: Sent::default(),
            last_values: HashMap::new(),
            confirmed: HashSet::new(),
            lines: 0,
            line_of: HashMap::new(),
            compared_at: 0,
            chased: 0,
            check: None,
            verified: false,
            misses: 0,
            seen: HashMap::new(),
            watch_due: true,
            resend: false,
            foreign: false,
            loads: 0,
            exit: exit::ExitWatch::default(),
            rebuild: false,
            ticks: 0,
            failed: VecDeque::new(),
            stamp,
            known_rate: None,
            published: watch::channel(EqView::default()).0,
            xf: CrossfeedSettings::default(),
            xf_kept: false,
            xf_on: false,
            xf_for: None,
            output: None,
            xf_note: None,
            xf_reason: None,
            flat: false,
            played_at: 0,
            idle_keep: std::env::var(IDLE_TICKS_ENV)
                .ok()
                .and_then(|text| text.parse().ok())
                .unwrap_or(IDLE_KEEP),
            lift: false,
            xf_conf: false,
            stage_dirty: false,
            xf_ports: true,
            owed: None,
            force_conf: false,
            look_first: false,
            sent_at: 0,
            stage_on_node: false,
            resend_at: None,
            resending: false,
            resend_stage: false,
            reverts: VecDeque::new(),
            gain_cache: Mutex::new(None),
            sp: vs::Speakers::default(),
            ld: ld::Loudness::default(),
            settings_unread: None,
        };
        if let Some(state) = state {
            control.adopt(state);
            control.foreign = control.runs;
            // A chain that plays on from an earlier run may hold a lower preamp than the saved
            // state needs (the level was held when that run stopped): it is applied again once
            // idle.
            control.lift = control.runs;
        } else if let Some(settings) = kept {
            control.adopt_settings(settings);
        }
        // The file that is ignored is said once, in the view's note (the next apply clears it),
        // and kept for a caller that logs.
        if let Some(why) = unreadable {
            control.note = Some(format!("the kept settings were ignored: {why}"));
            control.settings_unread = Some(why);
        }
        control.publish();
        control
    }

    /// Why the settings file kept beside the state file was ignored at start (it did not read),
    /// `None` when it was fine or absent. The daemon logs it once.
    pub fn settings_unread(&self) -> Option<&str> {
        self.settings_unread.as_deref()
    }

    /// Take the settings kept while no state existed: the three tables, and nothing of a chain
    /// (no curve, no state to hold, nothing applied). Loudness comes back off: what it was
    /// asked to do with a chain is not a start.
    fn adopt_settings(&mut self, settings: super::HostEqSettings) {
        if let Some(crossfeed) = &settings.crossfeed {
            self.xf = super::crossfeed::held(crossfeed);
            self.xf_kept = true;
        }
        if settings.speakers.is_some() {
            self.sp.chosen = settings.speakers;
        }
        if let Some(loudness) = settings.loudness {
            self.ld.settings = loudness.held();
            self.ld.settings.on = false;
            self.ld.kept = true;
        }
    }

    /// Take `state` as what the chain is.
    fn adopt(&mut self, state: HostEqState) {
        self.points = Points::from_state(&state);
        self.stated = state.preamp_db;
        self.preset = Some(state.name.clone()).filter(|name| !name.is_empty());
        self.origin = state.origin.clone();
        self.runs = state.on;
        // Loudness first: whether it keeps a flat chain decides what the crossfeed's adopt owes.
        self.adopt_loudness(&state);
        self.adopt_crossfeed(&state);
        self.adopt_speakers(&state);
        // The stage the running chain's config holds (`crossfeed_loaded`), and the one the
        // config on disk holds while crossfeed plays (`crossfeed_on`). The state does not say
        // the rate either was worked out at: each is paid for at whichever usual rate costs the
        // most.
        let on_disk = state.crossfeed_on.then(|| self.xf.clone());
        let held_stage = state
            .crossfeed_loaded
            .iter()
            .chain(&on_disk)
            .flat_map(|settings| {
                CONFIG_STAGE_RATES.map(|rate| config_stage_cost_db(settings, rate))
            })
            .fold(0.0, f64::max);
        let held_xf = state.crossfeed_loaded.clone().or(on_disk);
        self.set_confs(
            Some(self.points.correction(self.stated)),
            held_xf,
            held_stage,
        );
        if state.crossfeed_loaded.is_some() && !state.crossfeed_on {
            // Crossfeed is off but the chain's config holds the stage: it is applied again
            // without it.
            self.xf_conf = true;
            self.owe(true);
        }
        self.base = Some(state);
        // What a command line wrote was worked out at the rate it writes at, whatever the
        // sink runs at now. On a reload the first look sends every slot again; at start it
        // does so only when the sink is not at 48 kHz (the rate change sends them).
        self.fresh_chain(CONFIG_RATE_HZ);
    }

    /// The config the chain runs and the one on disk are `conf`, with the stage `stage` that
    /// costs `stage_db`.
    fn set_confs(
        &mut self,
        conf: Option<Correction>,
        stage: Option<CrossfeedSettings>,
        stage_db: f64,
    ) {
        self.loaded = conf.clone();
        self.kept = conf;
        self.loaded_xf = stage.clone();
        self.kept_xf = stage;
        self.loaded_stage_db = stage_db;
        self.kept_stage_db = stage_db;
        *crate::locked(&self.cap) = None;
    }

    /// The chain was just applied or read, with its config worked out at `rate_hz`: it holds
    /// the config's values.
    fn fresh_chain(&mut self, rate_hz: u32) {
        // The chain is a new node: the one the session kept is not it, and the values it sent
        // are not on it.
        self.params.new_chain();
        self.unknown = false;
        self.legacy = false;
        self.held = false;
        self.dirty.clear();
        self.sent = Sent {
            node: None,
            rate: Some(rate_hz),
        };
        self.last_values.clear();
        self.confirmed.clear();
        self.stage_on_node = false;
        self.stage_sent = None;
        // Whether the new node has the loudness stage's controls is told by the first look.
        self.ld.ports = false;
        self.ld_new_node();
        self.seen.clear();
        self.check = None;
        self.rebuild = false;
        self.watch_due = true;
        // Whatever node is in the graph at the first look is taken as it is: it may be one
        // another process started since.
        self.unvouched = true;
        self.node_cap = None;
        self.not_run = false;
        self.own_chain = true;
    }

    /// The preamp the points play with.
    ///
    /// It is measured on the filters as they play (held to what the biquads are worked out for,
    /// at the rate they are worked out for), whatever the points hold.
    fn curve_preamp(&self) -> f64 {
        // A chain from an older svolder plays the preamp it was applied with, not one measured
        // here.
        if let (true, Some(base)) = (self.legacy, &self.base) {
            return base.preamp_db;
        }
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        // The caps bound the whole preamp, the config's stage included.
        self.wanted_preamp(rate)
            .min(self.cap_at(rate) + self.loaded_stage_db)
    }

    /// The preamp the chain plays: the EQ's (0 dB while it plays flat, never above what the
    /// chain's loaded config allows) less the crossfeed stage's worst gain, so that nothing
    /// plays above unity with the stage on either.
    fn preamp(&self) -> f64 {
        if let (true, Some(base)) = (self.legacy, &self.base) {
            return base.preamp_db;
        }
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        (self.wanted_total(rate)).min(self.cap_at(rate))
    }

    /// The preamp the points (or the flat EQ) and the stage need at `rate`, before any cap.
    fn wanted_total(&self, rate: u32) -> f64 {
        let eq = match self.flat {
            true => 0.0,
            false => self.wanted_preamp(rate),
        };
        eq - self.stage_gain()
    }

    /// Whether the chain runs as a plain pass-through: flat, with no stage keeping it (see
    /// `keeps_chain`). It is kept for a stream that plays and removed once none has for
    /// [`IDLE_KEEP`] ticks.
    fn passthrough(&self) -> bool {
        self.runs && self.flat && !self.keeps_chain()
    }

    /// Whether a stream plays through the chain, from a fresh look at the graph; `None` when the
    /// chain is not found. The look also says whether the chain takes the stage's controls.
    async fn look_playing(&mut self) -> Option<super::live::LiveNode> {
        self.params.forget_node();
        self.params.node().await.filter(|node| node.playing)
    }

    /// The last reason for the chain went away while a stream plays through it: the chain goes on
    /// as a pass-through (the slots the identity, the preamp 0 dB or the lower one the loaded
    /// config holds), changed live with no apply and no sink swap. Whether it was kept: not when
    /// nothing plays, or when the chain cannot be changed live.
    async fn keep_passthrough(&mut self) -> Result<bool, EqFailure> {
        let Some(node) = self.look_playing().await else {
            return Ok(false);
        };
        self.sp.ports = node.has_speakers_controls();
        if !self.stage_by_live()
            || self.conf_holds_stage()
            || self.sp.conf_on
            || !node.has_crossfeed_controls()
        {
            return Ok(false);
        }
        let was = std::mem::replace(&mut self.flat, true);
        let stage_was = std::mem::replace(&mut self.stage_dirty, true);
        self.dirty.extend(1..=SLOTS as u8);
        self.look_first = true;
        if !self.send_dirty().await {
            self.stage_dirty = stage_was;
            self.flat = was;
            self.take_back_line();
            return Ok(false);
        }
        self.played_at = self.ticks;
        self.save().await?;
        Ok(true)
    }

    /// A line that failed is taken back (what it was for did not happen), but the session repairs a
    /// failed line with the next one, so it may land yet: the points as they stand go with the
    /// next line, which puts the chain right.
    fn take_back_line(&mut self) {
        match self.runs && self.live_now() {
            true => self.dirty.extend(1..=SLOTS as u8),
            false => self.dirty.clear(),
        }
    }

    /// Remove a pass-through chain that nothing has played through for [`IDLE_KEEP`] ticks. A
    /// stream that plays at the check keeps it another period.
    async fn check_idle(&mut self) {
        if !self.passthrough()
            || self.owed.is_some()
            || self.ticks < self.played_at + self.idle_keep
        {
            return;
        }
        if self.look_playing().await.is_some() {
            self.played_at = self.ticks;
        } else if self.turn_off().await.is_err() {
            // Tried again a second later.
            self.played_at = (self.ticks + WATCH_EVERY).saturating_sub(self.idle_keep);
        }
        self.publish();
    }

    /// Whether the level is held below what the points need for a stream that plays: the view
    /// says so.
    fn level_held(&self) -> bool {
        self.lift && self.eq_on() && self.capped()
    }

    /// Apply the chain again with the points as they stand once nothing has played through it
    /// for the idle time, so a level held for a stream comes up to the curve's own. A stream
    /// that plays at the check keeps it held another period; a lift the curve no longer needs (it
    /// fits under the cap without the stage) is dropped once the node has been looked at.
    async fn check_lift(&mut self) {
        if !self.lift
            || !self.eq_on()
            || !self.live_now()
            || self.unvouched
            || self.not_run
            || self.owed.is_some()
            || self.exit.pending()
        {
            return;
        }
        if !self.capped() {
            // A stage that takes more preamp than the cap holds nothing down while it plays, but
            // the curve alone is held again once it is off: the lift is dropped only when the
            // curve fits under the cap without the stage.
            let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
            if self.wanted_preamp(rate) - self.loaded_stage_db <= self.cap_at(rate) + 1e-9 {
                self.lift = false;
            }
            return;
        }
        if self.ticks < self.played_at + self.idle_keep {
            return;
        }
        if self.look_playing().await.is_some() {
            self.played_at = self.ticks;
            return;
        }
        match self.apply().await {
            Ok(()) => {
                // The chain runs the points: failing to keep them changes nothing that plays.
                let _ = self.save().await;
            }
            // Tried again a second later.
            Err(_) => self.played_at = (self.ticks + WATCH_EVERY).saturating_sub(self.idle_keep),
        }
        self.publish();
    }

    /// Set the idle time, in ticks, for a test that cannot wait [`IDLE_KEEP`].
    #[cfg(test)]
    pub(crate) fn set_idle_ticks(&mut self, ticks: u64) {
        self.idle_keep = ticks;
    }

    /// Whether the EQ itself is on: a chain runs and it is not the flat one of the crossfeed.
    fn eq_on(&self) -> bool {
        self.runs && !self.flat
    }

    /// The highest preamp the chain may be sent at `rate` (the whole of it, the stage's share
    /// included): what `loaded` and the stage its config holds need, what an unvouched chain
    /// held when it was first looked at, and what the points as they stand need under the stage
    /// the config holds. A rate change puts the config's stage back on the node with its slots,
    /// and a line carries every slot but the stage only when the stage moved, so a line sent
    /// before the next look plays the points under the config's stage.
    fn cap_at(&self, rate: u32) -> f64 {
        let eq = match self.flat {
            true => 0.0,
            false => self.wanted_preamp(rate),
        };
        self.conf_preamp(rate)
            .min(self.node_cap.unwrap_or(f64::INFINITY))
            .min(eq - self.loaded_stage_db)
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
            .map_or(0.0, |conf| played_preamp(conf, rate).min(0.0))
            - self.loaded_stage_db;
        *crate::locked(&self.cap) = Some((rate, db));
        db
    }

    /// Whether the preamp is held below what the points need by the loaded curve.
    fn capped(&self) -> bool {
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        self.wanted_total(rate) > self.cap_at(rate) + 1e-9
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
            on: self.eq_on(),
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
            live: self.runs && self.live_now() && !self.exit.pending(),
            note: self
                .note
                .clone()
                .or_else(|| self.legacy.then(|| LEGACY_NOTE.to_string()))
                .or_else(|| self.passthrough().then(|| PASSTHROUGH_NOTE.to_string()))
                .or_else(|| self.level_held().then(|| LEVEL_HELD_NOTE.to_string())),
            unavailable: self.unavailable.clone().or_else(|| {
                self.chain
                    .unavailable()
                    .map(|failure| format!("computer controls unavailable: {}", failure.what))
            }),
            max: SLOTS as u8,
            running_unknown: self.unknown,
            loads: self.loads,
            crossfeed: Some(self.crossfeed_view()),
            speakers: self.speakers_view(),
            loudness: Some(self.loudness_view()),
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
            flat: self.flat,
            crossfeed: self.stage_in_conf().then(|| self.xf.clone()),
            speakers: self.speakers_conf(),
        }
    }

    /// The request that keeps the config on disk: as [`EqControl::request`], and a config that
    /// holds the stage on keeps holding it (the running chain was started with it, so one that
    /// starts again must not play a different level than this one).
    fn keep_request(&self) -> ApplyRequest {
        let mut request = self.request();
        if self.xf_on && self.xf_conf {
            request.crossfeed = Some(self.xf.clone());
        }
        // A config that holds the stage on keeps holding it, as the crossfeed's does.
        if self.sp.on && self.sp.conf_on {
            request.speakers = request.speakers.map(|speakers| SpeakersConf {
                on: true,
                ..speakers
            });
        }
        request
    }

    /// Apply the points: the chain is built, or rebuilt, with them.
    async fn apply(&mut self) -> Result<(), EqFailure> {
        // With no chain there is no rate known: the sink's own is read at every try, so a turn-on
        // a rate refused does not hold that rate once the sink has changed, and the config is
        // written at it. Unread, it is written at the default and the first look at the chain
        // sends every slot again.
        if !self.runs || self.known_rate.is_none() {
            self.known_rate = self.params.sink_rate().await;
        }
        // The WAV is checked before the config names it: one that cannot be played is applied
        // without the stage, and the choice goes Off.
        let turned_off = self.check_set_for_apply();
        let request = self.request();
        let conf = request.speakers.clone();
        let state = match self.chain.apply(request).await {
            Ok(state) => state,
            Err(failure) => {
                // The running chain holds the set it held.
                self.pin();
                if turned_off {
                    // It may play the stage still: taking it out is owed.
                    self.owe(true);
                }
                return Err(failure);
            }
        };
        let stage_db = match state.crossfeed_on {
            true => config_stage_cost_db(&self.xf, self.known_rate.unwrap_or(CONFIG_RATE_HZ)),
            false => 0.0,
        };
        self.set_confs(
            Some(match self.flat {
                // The config holds the identity slots and no EQ preamp.
                true => Correction {
                    preamp_db: 0.0,
                    filters: Vec::new(),
                },
                false => Correction {
                    preamp_db: state.preamp_db,
                    filters: state.filters.clone(),
                },
            }),
            state.crossfeed_on.then(|| self.xf.clone()),
            stage_db,
        );
        self.xf_conf = state.crossfeed_on;
        // A chain just built has the stage's controls, and the stage off unless its config
        // holds it: the stage that is on is sent to it.
        self.xf_ports = true;
        self.speakers_applied(conf.as_ref());
        self.stage_dirty = (self.xf_on && !state.crossfeed_on) || (self.sp.on && !self.sp.conf_on);
        self.force_conf = false;
        self.base = Some(state);
        self.runs = true;
        self.foreign = false;
        self.exit.arm(&self.chain);
        self.exit.put_right();
        self.unavailable = None;
        self.failed.clear();
        self.fresh_chain(self.known_rate.unwrap_or(CONFIG_RATE_HZ));
        // The new node holds its config's identity loudness stage, and no look after an apply
        // sends everything again: the stage that plays goes with the next line. Off, the
        // identity is where it was going.
        match self.ld.settings.on {
            true if self.ld.played > 0.0 => self.dirty.extend(1..=SLOTS as u8),
            true => {}
            false => self.ld_gone(),
        }
        // The chain plays the points' own preamp: nothing is held.
        self.lift = false;
        self.stamp = stamp_of(&self.keep);
        // What an earlier refusal said no longer holds; the live-update note still does.
        if self.note.as_deref() != Some(LIVE_NOTE) {
            self.note = None;
        }
        // The first state file the chain's apply wrote holds no settings of the ones kept before
        // it existed: it takes them over, and the file that held them goes.
        if let Keep::File(path) = &self.keep
            && super::HostEqSettings::path(path).exists()
        {
            // Best-effort: the chain is applied, and every caller reads an Err here as "not
            // applied". The save that follows an apply reports a failed write.
            let _ = self.save_state();
        }
        Ok(())
    }

    /// Keep the points: the state file and the config the chain restarts from.
    async fn save(&mut self) -> Result<(), EqFailure> {
        self.save_state()?;
        if self.runs {
            // A chain that restarts finds the points as they are; failing to say so changes
            // nothing that plays now.
            let request = self.keep_request();
            // The config is written with the preamp it needs at its own rate, which is what a
            // chain that starts from it plays.
            let rate = request.rate_hz.unwrap_or(CONFIG_RATE_HZ);
            // Never a config whose points do not play at its rate (a held chain waits for one
            // they do, and points read from the state file are checked at the first look): it
            // would hold coefficients the chain clamps to other filters, which a restart plays.
            if self.held || (!request.flat && check_playable(&request.correction, rate).is_err()) {
                return Ok(());
            }
            let written = match request.flat {
                true => Correction {
                    preamp_db: 0.0,
                    filters: Vec::new(),
                },
                false => Correction {
                    preamp_db: played_preamp(&request.correction, rate),
                    filters: request.correction.filters.clone(),
                },
            };
            let holds_stage = request.crossfeed.is_some();
            let speakers = request.speakers.clone();
            if self.chain.keep_conf(request).await.is_ok() {
                self.speakers_kept(speakers.as_ref());
                self.kept = Some(written);
                self.kept_stage_db = match holds_stage {
                    true => config_stage_cost_db(&self.xf, rate),
                    false => 0.0,
                };
                self.kept_xf = holds_stage.then(|| self.xf.clone());
                self.xf_conf = holds_stage;
            }
        }
        Ok(())
    }

    /// Keep the points in the state file, when the controller keeps one.
    fn save_state(&mut self) -> Result<(), EqFailure> {
        if let (Keep::File(path), Some(base)) = (&self.keep, &self.base) {
            // A file written by someone else since it was last read or written is about to be
            // replaced unread: the chain it describes is sent these points at the next look.
            let foreign = stamp_of(&self.keep) != self.stamp;
            let state = HostEqState {
                on: self.runs,
                name: self.name(),
                origin: self.origin.clone(),
                // A curve the rate no longer plays has no preamp to measure: the one saved while
                // it still played stands (a flat chain keeps such points without playing them).
                // A level held for a stream is not the curve's: the state keeps the curve's own
                // preamp, which the chain applied again (or started from the state) plays.
                preamp_db: match self.held || (self.flat && self.playable(&self.points).is_err()) {
                    true => base.preamp_db,
                    false if self.lift && !self.legacy && !self.flat => {
                        self.wanted_preamp(self.sent.rate.unwrap_or(CONFIG_RATE_HZ))
                    }
                    false => self.curve_preamp(),
                },
                filters: self.points.filters.clone(),
                slots: self.points.slots.clone(),
                crossfeed: self.xf_kept.then(|| self.xf.clone()),
                crossfeed_on: self.xf_on,
                crossfeed_loaded: self.loaded_xf.clone(),
                flat: self.flat,
                speakers: self.sp.chosen.clone(),
                speakers_on: self.sp.on,
                speakers_loaded: self.sp.loaded.clone(),
                loudness: self.ld.kept.then_some(self.ld.settings),
                ..base.clone()
            };
            state
                .save(path)
                .map_err(|e| refusal(e, "Check that the directory can be written to"))?;
            self.base = Some(state);
            self.stamp = stamp_of(&self.keep);
            if foreign {
                self.resend = true;
                self.watch_due = true;
            }
            // The state holds the settings now.
            let _ = std::fs::remove_file(super::HostEqSettings::path(path));
        } else if let (Keep::File(path), None) = (&self.keep, &self.base) {
            // No chain was ever applied: the settings are kept on their own, with no curve.
            super::HostEqSettings {
                crossfeed: self.xf_kept.then(|| self.xf.clone()),
                speakers: self.sp.chosen.clone(),
                loudness: self.ld.kept.then_some(self.ld.settings),
            }
            .save(path)
            .map_err(|e| refusal(e, "Check that the directory can be written to"))?;
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
        self.finish_reset().await;
        let running = self.running().await;
        if on {
            return match (self.runs, running) {
                // The chain runs for crossfeed alone: the EQ's points go in live.
                (true, Some(true)) if self.flat => self.leave_flat().await,
                (true, Some(true)) => Ok(()),
                // A chain that may be running is taken away first, so two never stack.
                (_, None) => {
                    self.chain.remove().await?;
                    self.apply_eq().await
                }
                _ => self.apply_eq().await,
            };
        }
        // The EQ off with a stage on (crossfeed, the speakers, loudness) keeps the one chain,
        // playing flat; all off removes it.
        if self.keeps_chain() && self.runs && running != Some(false) {
            return match self.flat {
                true => Ok(()),
                false => self.go_flat().await,
            };
        }
        // Off while the chain is already a pass-through is done: the idle timer goes on, and the
        // chain is not taken from under a stream by a second off.
        if self.passthrough() && running != Some(false) {
            return Ok(());
        }
        // The last reason for the chain goes away while a stream plays through it: it stays as a
        // pass-through rather than pause the player by taking its sink away.
        if self.runs && !self.flat && running == Some(true) && self.keep_passthrough().await? {
            return Ok(());
        }
        // Off removes the chain unless it is known not to run; a remove changes nothing when
        // there is none.
        if !self.runs && running == Some(false) {
            return Ok(());
        }
        self.turn_off().await
    }

    /// Apply the points as the EQ's own chain: crossfeed or not, the EQ plays.
    async fn apply_eq(&mut self) -> Result<(), EqFailure> {
        let was_flat = std::mem::replace(&mut self.flat, false);
        let result = self.apply().await;
        if result.is_err() {
            self.flat = was_flat;
        }
        result
    }

    /// The EQ on while the chain plays flat for crossfeed or the speakers: the slots take the
    /// points and the preamp the curve needs, live, with no apply (by applying when live updates
    /// cannot).
    async fn leave_flat(&mut self) -> Result<(), EqFailure> {
        // The points were kept unchecked while the chain played flat.
        self.playable(&self.points)?;
        self.flat = false;
        self.dirty.extend(1..=SLOTS as u8);
        if self.live_now() && self.send_dirty().await {
            return self.save().await;
        }
        self.dirty.clear();
        if let Err(failure) = self.apply().await {
            self.flat = true;
            self.take_back_line();
            return Err(failure);
        }
        self.save().await
    }

    /// The EQ off while crossfeed is on: the slots go to the identity and the preamp is
    /// re-sent, live, and the chain stays (by applying when live updates cannot).
    async fn go_flat(&mut self) -> Result<(), EqFailure> {
        self.flat = true;
        self.dirty.extend(1..=SLOTS as u8);
        if self.live_now() && self.send_dirty().await {
            return self.save().await;
        }
        self.dirty.clear();
        if let Err(failure) = self.apply().await {
            self.flat = false;
            self.take_back_line();
            return Err(failure);
        }
        self.save().await
    }

    /// Take the chain out of the path and keep the points, with the state saying off.
    async fn turn_off(&mut self) -> Result<(), EqFailure> {
        // A turn-off that is tried again while held saved the points at the first try.
        if self.runs && !self.held {
            self.save_state()?;
        }
        self.chain.remove().await?;
        self.exit.put_right();
        self.params.forget_node();
        if self.xf_on && self.xf_note.is_none() {
            self.xf_note = Some("crossfeed turned off: the computer EQ chain was removed".into());
        }
        if self.sp.on && self.sp.note.is_none() {
            self.sp.note =
                Some("virtual speakers turned off: the computer EQ chain was removed".into());
        }
        self.speakers_gone();
        self.runs = false;
        self.flat = false;
        self.xf_on = false;
        self.xf_for = None;
        self.xf_conf = false;
        self.stage_dirty = false;
        self.unknown = false;
        self.legacy = false;
        self.dirty.clear();
        self.ld_gone();
        if let Some(base) = self.base.as_mut() {
            base.on = false;
        }
        // Still held here, so the preamp saved is the one the curve played with.
        let saved = self.save_state();
        self.held = false;
        // With no chain there is no rate to know: the next one is checked and written at the
        // default, and the first look at it checks the curve at the rate the sink runs at.
        self.known_rate = None;
        self.sent.rate = None;
        self.set_confs(None, None, 0.0);
        self.lift = false;
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
        self.finish_reset().await;
        let mut off = false;
        if self.runs {
            match self.running().await {
                Some(false) => {
                    self.runs = false;
                    // No chain, no stage.
                    self.flat = false;
                    self.xf_on = false;
                    self.xf_for = None;
                    self.speakers_gone();
                    self.ld_gone();
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
        self.finish_reset().await;
        // Only a chain known not to run is applied again: one that may run is left alone.
        if !self.runs || self.running().await != Some(false) {
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
        if result.is_ok() {
            self.loads += 1;
        }
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
            crossfeed: None,
            crossfeed_on: false,
            flat: false,
            speakers: None,
            speakers_on: false,
            speakers_loaded: None,
            crossfeed_loaded: None,
            loudness: None,
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
            self.take_back_line();
        }
        result
    }

    /// Put the points as they stand on the chain, live when it can be and by applying when not,
    /// and keep them.
    async fn put(&mut self) -> Result<(), EqFailure> {
        // Putting points on the chain is the EQ playing them, so a flat chain stops being flat.
        let was_flat = std::mem::replace(&mut self.flat, false);
        let result = self.put_points().await;
        if result.is_err() {
            self.flat = was_flat;
        }
        result
    }

    async fn put_points(&mut self) -> Result<(), EqFailure> {
        let live = self.runs && self.live_now() && SHAPE == SlotShape::Raw;
        // A curve that needs more preamp than the chain's cap goes in by applying, so its own
        // preamp plays, unless a stream plays through the chain: an apply takes the sink from
        // under it, which pauses some players. It then goes live with the preamp held at the cap,
        // and the chain is applied again once nothing has played for the idle time.
        let held = live && self.capped();
        if live && (!held || self.look_playing().await.is_some()) {
            self.dirty.extend(1..=SLOTS as u8);
            if self.send_dirty().await {
                if held {
                    self.lift = true;
                    self.played_at = self.ticks;
                }
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
        if !self.eq_on() {
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

    /// Why the curve the running chain was loaded with cannot play at the rate it runs at, when
    /// it cannot: a rate change puts the slots back to it, so its preamp bounds every one sent,
    /// and one that cannot be measured leaves none to send.
    fn loaded_unplayable(&self) -> Option<EqFailure> {
        let loaded = self.loaded.as_ref()?;
        let why = check_playable(loaded, self.sent.rate.unwrap_or(CONFIG_RATE_HZ)).err()?;
        Some(refusal(
            format!("an earlier curve the chain still holds: {}", why.what),
            why.next,
        ))
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
                // The web core finds this refusal by "moved or is gone" (`movedPoint`), and the
                // tests of the daemon's route and of the controller pin the words.
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
    /// nothing to send). Another node found whose line also fails counts as a failed
    /// tick like any other; reaching `FAIL_LIMIT` that way takes the node changing again between
    /// the look and the send, that many times inside `FAIL_WINDOW`.
    async fn send_dirty(&mut self) -> bool {
        if self.send_dirty_once().await {
            return true;
        }
        // Another node than the one looked at (the chain was replaced since, under a new id or
        // the same one, and the kept node was dropped and found again) gets no line. It is
        // looked at, and the line goes once more.
        match self.params.node().await {
            Some(node) if self.sent.node != Some(node.key()) => self.send_dirty_once().await,
            _ => false,
        }
    }

    /// The values of every slot for the points as they stand, worked out for `rate`.
    fn slot_values(&self, rate: u32) -> Vec<(String, f64)> {
        let mut params = Vec::new();
        for slot in 1..=SLOTS as u8 {
            // A chain that plays flat has every slot the identity.
            let filter = self
                .points
                .slots
                .iter()
                .position(|held| *held == slot)
                .filter(|_| !self.flat)
                .map(|index| &self.points.filters[index]);
            params.extend(slot_params(slot, filter, rate));
        }
        params
    }

    /// Whether every preamp port `held` lists holds the preamp of the config this controller
    /// applied at `rate`, as `pw-dump` prints it (f32, 6 decimals), within the tolerance
    /// [`Self::holds_sent_preamp`] uses.
    fn holds_conf_preamp(&self, held: &HashMap<String, f64>, rate: u32) -> bool {
        let wanted = preamp_params(self.conf_preamp(rate));
        wanted.iter().all(|(name, wanted)| match held.get(name) {
            Some(held) => (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6,
            None => true,
        })
    }

    /// Whether every preamp port `held` lists holds the value this controller last sent it.
    fn holds_sent_preamp(&self, held: &HashMap<String, f64>) -> bool {
        let same = |wanted: f64, held: f64| (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6;
        preamp_params(0.0)
            .iter()
            .all(|(name, _)| match held.get(name) {
                Some(held) => self
                    .last_values
                    .get(name)
                    .is_some_and(|sent| same(*sent, *held)),
                None => true,
            })
    }

    /// Whether `node`, which this controller did not start, holds anything other than what the
    /// points and the preamp make now. A port the node does not list is not a difference.
    fn differs_from_points(&self, node: &super::live::LiveNode) -> bool {
        let rate = node.rate_hz.or(self.sent.rate).unwrap_or(CONFIG_RATE_HZ);
        let same = |wanted: f64, held: f64| (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6;
        self.slot_values(rate)
            .into_iter()
            .chain(preamp_params(self.preamp()))
            .any(|(name, wanted)| {
                node.params
                    .get(&name)
                    .is_some_and(|held| !same(wanted, *held))
            })
    }

    async fn send_dirty_once(&mut self) -> bool {
        if self.dirty.is_empty() && !self.stage_dirty {
            return true;
        }
        // Nothing goes to a node that has not been looked at: it may be one another process
        // started, and what it was started from bounds the preamp (see `watch`).
        if std::mem::take(&mut self.look_first) {
            self.params.forget_node();
        }
        let Some(node) = self.params.node().await else {
            return false;
        };
        // A rate the sink moved to since the last look changes what the stage is worked out
        // for: it is looked at (which re-sends the slots and the stage) before anything goes.
        let rate_moved = self.stage_dirty
            && node
                .rate_hz
                .is_some_and(|rate| self.sent.rate.is_some_and(|sent| sent != rate));
        if self.sent.node != Some(node.key()) || rate_moved {
            self.watch().await;
            if !self.runs || !self.live_now() || (self.dirty.is_empty() && !self.stage_dirty) {
                return false;
            }
        }
        let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
        // Every slot goes with every update, not only the ones that moved: a change of the
        // sink's rate puts the chain's slots back to its config without telling anyone, and an
        // update that carried one slot would leave the rest as that config has them, under a
        // preamp worked out for the points as they stand. A line that carries them all leaves
        // nothing of the old chain on it.
        let mut params = self.slot_values(rate);
        // The loudness stage rides every line to a node that has it, after the slots: a step of
        // its glide, or the depth it plays. It is cut-only at every rate, so it needs no order
        // against the preamp.
        let ld_line = self.ld_line(rate);
        if let Some((_, ports)) = &ld_line {
            params.extend(ports.iter().cloned());
        }
        // The stage goes with the slots, between them and the preamp: a preamp that falls
        // (the stage turns on) is before it, one that rises (the stage turns off) after.
        let sent_stage = std::mem::take(&mut self.stage_dirty);
        // The stage the node plays once this line lands, which the preamp below pays for.
        let stage_before = self.stage_sent.clone();
        if sent_stage {
            let stage = match self.xf_on {
                true => Stage::of(&self.xf, rate).ok(),
                false => None,
            };
            self.stage_sent = stage.as_ref().map(|_| (self.xf.clone(), rate));
            params.extend(crossfeed_params(&stage.unwrap_or_else(Stage::bypass)));
            // The virtual speakers' mixers go with it, so that one update turns one stage on
            // and the other off.
            if self.sp.ports {
                params.extend(speakers_params(self.sp.on));
            }
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
            // A graph that has not run holds nothing sent: it starts from its config.
            match self.not_run {
                true => applied,
                false => self
                    .last_values
                    .get(name)
                    .or_else(|| self.seen.get(name))
                    .or(applied),
            }
            .is_some_and(|held| db > held)
        });
        match rises {
            true => params.extend(preamp),
            false => params.splice(0..0, preamp).for_each(drop),
        }
        let Some(vouched) = self.sent.node else {
            self.stage_dirty |= sent_stage;
            self.stage_sent = stage_before;
            return false;
        };
        self.ld_writing();
        if !self.params.send(vouched, &params).await {
            self.stage_dirty |= sent_stage;
            self.stage_sent = stage_before;
            self.ld_failed();
            return false;
        }
        self.dirty.clear();
        if let Some((depth, _)) = ld_line {
            self.ld_sent(depth);
        }
        if sent_stage {
            self.stage_on_node = self.space_on();
            self.resend_stage = true;
        }
        // One repeat per send, a second later, and none for the repeat itself: PipeWire may not
        // play what is sent while the chain's stream is being replaced, and nothing reads that.
        self.resend_at = match std::mem::take(&mut self.resending) {
            true => None,
            false => Some(self.ticks + WATCH_EVERY),
        };
        self.sent_at = self.ticks;
        // A config whose stage costs more than the one that plays holds the preamp down by the
        // difference: the chain is applied again once nothing has played for the idle time, as
        // after a held recall.
        if self.eq_on() && !self.lift && self.loaded_stage_db > self.stage_gain() + 1e-9 {
            self.lift = true;
            self.played_at = self.ticks;
        }
        self.lines += 1;
        for (name, value) in &params {
            // A value sent again unchanged (the repeat) leaves the port confirmed.
            if self.last_values.insert(name.clone(), *value) != Some(*value) {
                self.confirmed.remove(name);
                self.line_of.insert(name.clone(), (self.lines, self.ticks));
            }
        }
        match &mut self.check {
            Some(check) => {
                if let Some((_, value)) = params.iter().find(|(name, _)| *name == check.key) {
                    check.later.push(*value);
                    let excess = check.later.len().saturating_sub(READ_BACK_LATER_MAX);
                    check.later.drain(..excess);
                }
            }
            None if !self.verified => {
                let key = self.key_to_check(&params);
                let armed = params
                    .iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, v)| *v);
                self.check = armed.map(|armed| ReadBack {
                    key,
                    armed,
                    later: Vec::new(),
                });
                self.watch_due = true;
            }
            None => {}
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
        // The volume read with the node is the newest.
        self.ld_look(true);
        self.exit.arm(&self.chain);
        // PipeWire gives a freed id to a later object, so a chain started again may be listed
        // under the old one's id: its serial tells it.
        let restarted = self.sent.node.is_some_and(|sent| sent != node.key());
        let rate_changed = node
            .rate_hz
            .is_some_and(|rate| self.sent.rate.is_some_and(|sent| sent != rate));
        let before = std::mem::replace(&mut self.seen, node.params.clone());
        if node.playing && (self.passthrough() || self.lift) {
            self.played_at = self.ticks;
        }
        if let Some(output) = self.params.output() {
            self.output = Some(output);
        }
        self.check_output();
        if restarted {
            // Another node: what was sent to the old one is not on it, and whether its graph ran
            // is told by this look alone.
            self.unvouched = true;
            self.own_chain = false;
            self.not_run = false;
            self.last_values.clear();
            self.ld_new_node();
            self.stage_on_node = false;
            self.stage_sent = None;
            if self.xf_on {
                // A chain that started again plays its config, which holds the stage only when
                // `xf_conf` says so.
                self.reset_xf(
                    OffReason::ChainRestarted,
                    "crossfeed turned off: the computer EQ chain restarted".to_string(),
                    self.xf_conf,
                );
            }
            if self.sp.on {
                self.reset_sp(
                    SpeakersOff::ChainRestarted,
                    "virtual speakers turned off: the computer EQ chain restarted".to_string(),
                    self.sp.conf_on,
                );
            }
            self.speakers_restarted();
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
        self.xf_ports = node.has_crossfeed_controls();
        self.sp.ports = node.has_speakers_controls();
        self.ld.ports = node.has_loudness_controls();
        self.ld_seen(&node.params);
        if self.sp.ports && self.sp.loaded.is_none() {
            // A chain another process built holds a set: the chosen one is taken to be it, and
            // its WAV is pinned.
            self.sp.loaded = self.sp.chosen.clone();
            self.pin();
        }
        if self.legacy {
            self.legacy = false;
            self.dirty.extend(1..=SLOTS as u8);
        }
        // A graph that has not run yet lists its controls at 0, whatever it holds, and starts from
        // its config when it runs: such a look bounds no preamp and checks nothing, and the first
        // look at it run does, and sends everything again.
        let ran = graph_ran(&node.params);
        let first_run = ran && std::mem::take(&mut self.not_run);
        self.not_run |= !ran;
        if ran && std::mem::take(&mut self.unvouched) {
            // A graph this controller applied or read at start or reload (`own_chain`) that just
            // ran holds its config's preamp (read back as `pw-dump` prints it, f32 at 6
            // decimals, so a hair off), or a preamp this controller sent since it started, which
            // bounds nothing: `cap_at` already holds the config's own preamp exactly; a lower
            // preamp bounds, a chain this controller did not start always does. Nor does a
            // preamp at the floor: that chain plays nothing, and it plays the view's preamp.
            let rate = node.rate_hz.or(self.sent.rate).unwrap_or(CONFIG_RATE_HZ);
            let own_send = self.own_chain
                && ((first_run && self.holds_sent_preamp(&node.params))
                    || self.holds_conf_preamp(&node.params, rate));
            self.node_cap =
                held_preamp_db(&node.params).filter(|held| !own_send && *held > SILENT_HELD_DB);
            // A node found with the stage on that this controller does not hold on (a run that
            // sent it before it was lost, or a chain the stage was sent to under another owner)
            // is sent the bypass with the first update.
            let stage_seen = node.params.get("lmix:Gain 2").is_some_and(|mix| *mix > 0.0);
            if stage_seen && !self.xf_on && self.xf_ports {
                self.stage_dirty = true;
            }
            let speakers_seen = node.params.get("lvs:Gain 2").is_some_and(|mix| *mix > 0.0);
            if speakers_seen && !self.sp.on && self.sp.ports {
                self.stage_dirty = true;
            }
            // A loudness stage this controller does not vouch for (one another owner left, or
            // one sent here before it was lost): the first line puts the right one on it (the
            // identity while loudness is off).
            let rate = node.rate_hz.or(self.sent.rate).unwrap_or(CONFIG_RATE_HZ);
            if self.ld.ports && !self.ld_holds_played(&node.params, rate) {
                self.dirty.extend(1..=SLOTS as u8);
            }
        }
        if first_run {
            // It started from its config: what was sent before is not on it, and everything goes
            // again, the stage with it.
            self.last_values.clear();
            self.confirmed.clear();
            self.resend = true;
            // The values sent before are not on the node the check read: the resend arms it again.
            self.check = None;
            self.misses = 0;
        }
        // A chain this controller did not start plays what the saved state says, whatever it was
        // left holding (the preamp goes first when it falls, as in every update).
        if ran && std::mem::take(&mut self.foreign) && self.differs_from_points(&node) {
            self.resend = true;
        }
        let resend = std::mem::take(&mut self.resend);
        if restarted || rate_changed || self.held || resend {
            // The rebuilt graph re-opens the set's WAV: one that cannot be read leaves the chain
            // silent with no exit, so the chain is applied again without the stage.
            if (restarted || rate_changed) && self.rebuild_for_missing_set().await {
                return;
            }
            if restarted {
                // A chain that started again plays the config on disk, and none of what was
                // sent to the old one.
                self.params.new_chain();
                // Nothing sent to the old chain is on this one: the resend arms the check again.
                self.check = None;
                self.misses = 0;
                self.loaded = self.kept.clone();
                self.loaded_stage_db = self.kept_stage_db;
                self.loaded_xf = self.kept_xf.clone();
                *crate::locked(&self.cap) = None;
            }
            if restarted || rate_changed {
                // The rebuilt graph has not been seen holding anything yet.
                self.confirmed.clear();
            }
            let before = self.sent.rate;
            self.sent = Sent {
                node: Some(node.key()),
                rate: node.rate_hz.or(self.sent.rate),
            };
            // The curve was checked at the old rate; at this one it may not be playable (a
            // coefficient past what the chain holds is clamped to another filter), so nothing
            // is sent until it is. A flat chain plays none of the points: they are not checked.
            let mut unplayable = match self.flat {
                true => None,
                false => self.playable(&self.points).err(),
            };
            // Nor is anything sent while the curve the chain was loaded with (an earlier one its
            // config still holds) cannot be measured at this rate: it bounds every preamp. The
            // chain is applied again with the points, which play here.
            if unplayable.is_none()
                && let Some(earlier) = self.loaded_unplayable()
            {
                match self.apply().await {
                    Ok(()) => {
                        let _ = self.save().await;
                        return;
                    }
                    Err(why) => {
                        unplayable = Some(refusal(
                            format!("{} (applying the points again failed: {why})", earlier.what),
                            earlier.next,
                        ))
                    }
                }
            }
            if let Some(failure) = unplayable {
                let rate = self.sent.rate;
                // The state keeps the preamp of the curve as it was playable.
                self.sent.rate = before;
                let off = self.turn_off().await;
                let at = rate.map_or_else(String::new, |rate| format!(" at {rate} Hz"));
                self.note = Some(match off {
                    Ok(()) => format!("computer EQ turned off{at}: {failure}"),
                    // Removed, and only the state could not be saved: the chain is off.
                    Err(why) if !self.runs => format!(
                        "computer EQ turned off{at}: {failure} \
                         (its state could not be saved: {why})"
                    ),
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
            // The stage goes back to what the config holds (off) when the graph is built
            // again for a new rate: it is sent again at this one, or turned off when it cannot
            // play at it (or the rate is above the stages' highest).
            let rate = self.sent.rate.unwrap_or(CONFIG_RATE_HZ);
            if self.xf_on {
                let plays = match rate > STAGE_RATE_MAX_HZ {
                    true => Err("crossfeed runs only up to 384 kHz".to_string()),
                    false => Stage::of(&self.xf, rate).map(drop),
                };
                match plays {
                    Ok(()) => self.stage_dirty = true,
                    // Above the stages' highest rate the bypass goes with this very line, before
                    // a preamp that rises.
                    Err(why) if rate > STAGE_RATE_MAX_HZ => {
                        self.reset_xf(
                            OffReason::OutputChanged,
                            format!("crossfeed turned off at {rate} Hz: {why}"),
                            true,
                        );
                        self.stage_dirty = self.xf_ports;
                    }
                    Err(why) => {
                        let at = self
                            .sent
                            .rate
                            .map_or_else(String::new, |r| format!(" at {r} Hz"));
                        self.reset_xf(
                            OffReason::OutputChanged,
                            format!("crossfeed turned off{at}: {why}"),
                            true,
                        );
                    }
                }
            }
            // The speakers' mixers are on their config's gains again: sent at once, or the
            // speakers turned off above the stages' highest rate.
            if self.sp.on && rate > STAGE_RATE_MAX_HZ {
                self.reset_sp(
                    super::speakers::OffReason::OutputChanged,
                    format!(
                        "virtual speakers turned off at {rate} Hz: they run only up to 384 kHz"
                    ),
                    true,
                );
                // Their mixers go off with this very line.
                self.stage_dirty |= self.sp.ports;
            }
            if self.sp.on {
                self.stage_dirty = true;
            }
            self.dirty.extend(1..=SLOTS as u8);
            self.watch_due = self.check.is_some();
            return;
        }
        self.sent.node = Some(node.key());
        if let Some(rate) = node.rate_hz {
            self.sent.rate = Some(rate);
        }
        if !ran || self.reverted(&node.params, &before) {
            return;
        }
        self.ld_settle(&node.params);
        if let Some(check) = self.check.take() {
            let held = node.params.get(&check.key).copied();
            // The node holding the value the check was armed with, or one sent after it to this
            // node, proves live changes land: a node a line behind a glide still shows one of
            // them.
            let landed = held.is_some_and(|held| {
                std::iter::once(&check.armed)
                    .chain(&check.later)
                    .any(|wanted| (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6)
            });
            match landed {
                true => {
                    self.verified = true;
                    self.misses = 0;
                }
                _ if self.misses + 1 < READ_BACK_MISSES => {
                    // pw-cli applies a line a moment after it is written: look again.
                    self.misses += 1;
                    self.check = Some(check);
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

    /// Whether the chain's graph went back to its config at the same node and rate (PipeWire
    /// rebuilt it): a port known to have taken its last sent value does not hold it any more, a
    /// while after that went in, with live updates known to take. Everything is then sent again (the
    /// stage with it), and a chain that does it over and over is given up on. A port is known to
    /// have taken its value when a look found it there, or when a look saw a port move to the
    /// value of the same line or a later one: `pw-cli` applies each line whole and in order, so
    /// the slots of a line whose preamp moved went in too, even when the graph was rebuilt
    /// before any look saw them. A port no look has seen at its sent value, from a line no look
    /// has seen land, is no revert: `pw-cli` may not have applied the line yet. But a rebuild
    /// puts every slot and the stage back and keeps only the preamp, so a line that left the
    /// preamp where it was (a cut, any edit under a capped preamp) leaves nothing a look can see
    /// land. When such a port is still off its value at a look although its line went before the
    /// look before this one, everything is sent again once for that line: the newest values,
    /// with no repeat of their own and not counted as a revert. `before` is what the look before
    /// this one held.
    fn reverted(&mut self, held: &HashMap<String, f64>, before: &HashMap<String, f64>) -> bool {
        let same = |wanted: f64, held: f64| (wanted - held).abs() <= 1e-4 * wanted.abs() + 1e-6;
        let compared_before = std::mem::replace(&mut self.compared_at, self.ticks);
        let at_sent: HashSet<String> = self
            .last_values
            .iter()
            .filter(|(name, wanted)| held.get(*name).is_some_and(|held| same(**wanted, *held)))
            .map(|(name, _)| name.clone())
            .collect();
        // The newest line seen to land: a port that the look before held at another value and
        // this one holds at the value that line gave it.
        let landed = at_sent
            .iter()
            .filter(|name| {
                before
                    .get(*name)
                    .is_some_and(|was| !same(self.last_values[*name], *was))
            })
            .filter_map(|name| self.line_of.get(name).map(|(line, _)| *line))
            .max();
        if let Some(landed) = landed {
            let went_in = self
                .last_values
                .keys()
                .filter(|name| {
                    self.line_of
                        .get(*name)
                        .is_some_and(|(line, _)| *line <= landed)
                })
                .cloned()
                .collect::<Vec<_>>();
            self.confirmed.extend(went_in);
        }
        // What an earlier look confirmed stays confirmed (a send that changes a port drops it),
        // whether or not this look is the one that may act on it.
        self.confirmed.extend(at_sent);
        if !self.verified
            || self.check.is_some()
            || !self.dirty.is_empty()
            || self.stage_dirty
            || self.ticks < self.sent_at + REVERT_QUIET
        {
            return false;
        }
        let gone = self.last_values.iter().any(|(name, wanted)| {
            self.confirmed.contains(name)
                && held.get(name).is_some_and(|held| !same(*wanted, *held))
        });
        if !gone {
            // The newest line, gone a look ago or more, with a port still off its value.
            let stale = self
                .last_values
                .iter()
                .filter(|(name, wanted)| held.get(*name).is_some_and(|held| !same(**wanted, *held)))
                .filter_map(|(name, _)| self.line_of.get(name))
                .filter(|(_, at)| at + REVERT_QUIET <= compared_before)
                .map(|(line, _)| *line)
                .max();
            if let Some(line) = stale.filter(|line| *line > self.chased) {
                self.chased = line;
                self.dirty.extend(1..=SLOTS as u8);
                self.stage_dirty = self.space_on();
                self.resending |= self.resend_at.is_none();
            }
            return false;
        }
        self.reverts.push_back(self.ticks);
        while self
            .reverts
            .front()
            .is_some_and(|tick| tick + FAIL_WINDOW < self.ticks)
        {
            self.reverts.pop_front();
        }
        if self.reverts.len() > REVERT_LIMIT {
            if self.xf_on {
                self.reset_xf(
                    OffReason::ChainRestarted,
                    "crossfeed turned off: the chain keeps going back to its config".to_string(),
                    true,
                );
            }
            if self.sp.on {
                self.reset_sp(
                    SpeakersOff::ChainRestarted,
                    "virtual speakers turned off: the chain keeps going back to its config"
                        .to_string(),
                    true,
                );
            }
            self.reverts.clear();
            return true;
        }
        self.dirty.extend(1..=SLOTS as u8);
        self.stage_dirty = self.space_on();
        true
    }

    /// One step of the owner's clock, which must tick every 50 ms (see `WATCH_EVERY`): send what moved, at most once for each point, and
    /// now and then look at the chain.
    pub async fn tick(&mut self) {
        self.step().await;
        self.publish();
    }

    async fn step(&mut self) {
        self.ticks += 1;
        // The newest volume first: a write's reading, after any read made before it.
        self.ld_take_hint();
        self.refresh_sets_when_old();
        self.finish_reset().await;
        self.retry_rebuild().await;
        self.check_exit().await;
        self.check_idle().await;
        self.check_lift().await;
        // A chain that is taken for an older one or that waits for its rate is looked at, not
        // sent to.
        if !self.runs || !self.live || self.unavailable.is_some() {
            // Without live updates nothing is sent and the chain is not watched, but a crossfeed
            // that plays is still held to its output: it is looked at once a second.
            if self.runs && self.space_on() && self.ticks.is_multiple_of(WATCH_EVERY) {
                self.look_output().await;
                self.check_output();
                self.finish_reset().await;
            }
            return;
        }
        if self.watch_due || self.ticks.is_multiple_of(WATCH_EVERY) {
            self.watch().await;
            self.params.settle(self.sent.node).await;
            self.finish_reset().await;
        }
        if !self.runs || !self.live_now() {
            return;
        }
        // A step of the loudness's glide goes as an ordinary full line.
        self.ld_follow();
        if self.resend_at.is_some_and(|at| self.ticks >= at) {
            // Everything goes once more, with the latest values (so never above what the points
            // and the stage need now), the stage too when a send since carried it.
            self.resend_at = None;
            // A value that waits on this very tick goes out as a first send, which earns a repeat
            // of its own; only a send of nothing new is the repeat.
            let nothing_new = self.dirty.is_empty() && !self.stage_dirty;
            self.dirty.extend(1..=SLOTS as u8);
            self.stage_dirty |= std::mem::take(&mut self.resend_stage);
            self.resending = nothing_new;
        }
        let sent = (self.dirty.is_empty() && !self.stage_dirty) || self.send_dirty().await;
        // A repeat that did not go is not owed again: what it carried stays dirty, and the send
        // that follows is an ordinary one with a repeat of its own.
        self.resending = false;
        if sent {
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
                self.loads += 1;
                true
            }
            _ => false,
        }
    }
}

/// The refusal of turning `what` on at `rate`, above the highest rate a stage runs at.
fn over_stage_rate(what: &str, rate: u32) -> Option<EqFailure> {
    (rate > STAGE_RATE_MAX_HZ).then(|| {
        refusal(
            format!(
                "{what} only up to 384 kHz, and the output runs at {} kHz",
                f64::from(rate) / 1000.0
            ),
            "Set the output to 384 kHz or lower",
        )
    })
}

/// A held preamp at or below this, in dB, is the -120 dB floor: the node plays nothing, and the
/// preamp bounds nothing sent to it.
const SILENT_HELD_DB: f64 = -119.9;

/// The preamp in dB a node holds, from its `Mult` ports (the lower of the two channels, a port at
/// or below 0 reading as the -120 dB floor); `None` when no port is listed.
fn held_preamp_db(params: &HashMap<String, f64>) -> Option<f64> {
    preamp_params(0.0)
        .iter()
        .filter_map(|(name, _)| params.get(name).copied())
        .map(|mult| match mult > 0.0 {
            true => (20.0 * mult.log10()).max(-120.0),
            false => -120.0,
        })
        .min_by(f64::total_cmp)
}

/// Whether the node's graph has run: a graph that has not lists every slot's `b0` at 0, which no
/// filter holds (the identity's is 1). A node that lists no slot holds nothing to tell by. The
/// slots are raw biquads, which list `b0`; typed slots would need another tell.
fn graph_ran(params: &HashMap<String, f64>) -> bool {
    const { assert!(matches!(SHAPE, SlotShape::Raw)) };
    let mut b0 = (1..=SLOTS)
        .flat_map(|slot| ['l', 'r'].map(|channel| format!("{channel}{slot:02}:b0")))
        .filter_map(|name| params.get(&name))
        .peekable();
    b0.peek().is_none() || b0.any(|value| *value != 0.0)
}

fn refuse_edit(why: String) -> EqFailure {
    refusal(why, "Edit a point that is there")
}

mod exit;
mod ld;
mod vs;
mod xf;

pub use ld::{LevelHint, LoudnessEdit};
pub use vs::SpeakersEdit;

#[cfg(test)]
mod tests;
