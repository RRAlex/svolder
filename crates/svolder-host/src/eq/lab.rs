// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preamp lab: the computer EQ's controller driven through seeded random sequences of
//! events against a model of what PipeWire really plays.
//!
//! The model is a [`World`]: the config on disk, the running node (the config it was started
//! from, its id and serial, the value of every port as the node holds it), the sink's rate, a
//! `pw-cli` that can die, the state file and a clock. [`ModelChain`] and [`ModelParams`] are the
//! controller's `Chain` and `Params` over it, and [`Lab`] drives an [`EqControl`] and the
//! outside world (the command line, PipeWire, the sink) through [`Event`]s. After every event
//! and every tick of 50 ms [`Lab::check`] works out the curve the node plays (the ports it
//! holds, as f32, clamped) times the crossfeed stage it plays (its worst gain from the ports it
//! holds, again with an evaluator of its own) times the preamp it plays, and it must never be
//! over unity.
//!
//! What the model takes from PipeWire, with where it was measured:
//!
//! - A node starts from the config on disk: every slot, the crossfeed stage and the preamp take
//!   the config's values, and the node keeps that config to go back to. A restart of the chain is
//!   a new node, and so is an apply: always a new serial, and a new id or, as PipeWire gives a
//!   freed id to a later object (the host e2e saw it), on every other start the id of the node
//!   gone. Every block of seeds runs under both: [`Ids`].
//! - A change of the sink's rate rebuilds the graph: every slot goes back to the config the node
//!   started from (not the file as it is now), and the preamp keeps the value it was last set
//!   to. The node keeps its id and serial. This is the "slots revert to the applied config while
//!   the preamp keeps its live value" the host e2e's step E2 measured (+5.82 dB over unity on a
//!   private PipeWire before the cap), and spike notes' "a rate change re-instantiates the
//!   graph".
//! - A control value is an f32, and a `bq_raw` coefficient is held to plus or minus 10.
//! - A `set-param` to an id that is not the node's does nothing, and `pw-cli` says so (the
//!   refusal the session reads); one to the id of a node gone that the new node was given lands
//!   on the new node. A line is applied whole or not at all.
//! - `pw-cli` can die, and one that dies mid-line may or may not have applied the line.
//! - A new node may be listed late (a service manager returns when the unit is active, before
//!   the node is in the graph): until then it plays but cannot be found.
//! - The virtual speakers' stage is in a config only with a set chosen; its mixers play the dry
//!   sound or the two convolved paths into each ear, each path one impulse of a set scaled to
//!   -0.1 dB in its worst case, and the rule adds what the mixers the node holds make of that.
//! - A stream plays through the chain or not ([`Event::Playing`]): a look at whichever node is
//!   listed says so, as the links of a playing stream follow the chain's sink.
//! - The command line applies in the order the real one does: the config is written, the old
//!   node is stopped, the new one is started, 300 ms pass, then the state file is written.
//! - The loudness stage's two sections per channel are in every config, as the identity; a change
//!   of rate, a rebuild and a restart put them back to it, as they do the slots. The computer's
//!   volume and mute are the world's: a look reads them with the node, and a write made through
//!   the daemon also hands the controller its reading ([`Event::Volume`]). The rule counts the
//!   stage with the slots, and holds the stage alone to unity too.
//! - A scene's computer part is recalled as one change ([`Event::Scene`]): the same rules hold at
//!   every instant, and once the world is quiet the controller's view is the part, and the node
//!   plays its curve, its stages and its crossfeed.
//!
//! A sequence is a list of events and nothing else is random while it runs, so a failing seed
//! reproduces and a failing sequence shrinks.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use svolder_profiles::tone::apo::{self, Correction, FilterKind, SourceFilter};

use super::control::{CrossfeedEdit, Edit, EqControl, Keep, LoudnessEdit};
use super::crossfeed::{CrossfeedSettings, Model as CrossfeedModel};
use super::live::{LiveNode, NodeKey, SLOTS, crossfeed_params, loudness_params};
use super::loudness;
use super::loudness::LoudnessSettings;
use super::manage::{
    ApplyRequest, Chain, EqFailure, Status, check_playable, checked_request, conf_text, kept_conf,
};
use super::scene::ComputerPart;
use super::session::{Link, Session};
use super::{HostEqState, Manager, Route};
use crate::route::SinkLevel;

/// The sink's rates a sequence moves among: those of DACs up to 384 kHz.
const RATES: [u32; 8] = [
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
];

/// How much over unity a played curve may be, in dB: what f32 rounding of the coefficients
/// leaves, the same bound the safety tests hold the preamp to.
const TOLERANCE_DB: f64 = 0.01;

/// How many failing seeds of one run are shrunk and told in full.
const SHRUNK_REPORTS: usize = 3;

/// The shortest time between two starts of `pw-cli`, as the session keeps it.
const RESTART_GAP_MS: u64 = 1_000;

/// The ticks of the quiet tail at the end of a sequence (a multiple of 45: it is run as `Ticks`
/// events), more than [`QUIET_MS`] of them.
const TAIL_TICKS: u32 = 90;

/// How long the world is left alone (no event but time going by) before the liveness rule is
/// asked.
const QUIET_MS: u64 = 3_000;

/// How far, in dB, what the node plays may be from what the controller shows when it says it is
/// live.
const LIVE_TOLERANCE_DB: f64 = 0.05;

/// How long the world is left alone before the loudness's liveness rule is asked: the EQ's quiet
/// time, the second a look takes to read a volume changed from outside, and the longest glide
/// (40 dB at 0.5 dB a tick of 50 ms).
const LD_QUIET_MS: u64 = QUIET_MS + 1_000 + 4_000;

/// The frequencies the liveness rule compares at: this many steps of a log grid.
const LIVE_GRID: u32 = 96;

/// The wait the command line makes after it starts the chain.
const START_WATCH_MS: u64 = 300;

/// The idle time of the lab's controllers, in ticks: short, so that a sequence sees a held level
/// lifted and a pass-through removed.
const IDLE_TICKS: u64 = 60;

/// The virtual speakers' sets in the lab's store.
const SETS: [&str; 2] = ["set-a", "set-b"];

/// The gain of each of a set's four paths (an impulse each): a set is scaled so that its worst
/// case, the two paths into one ear in phase, is -0.1 dB, as the catalogue's are.
const SET_PATH_GAIN: f64 = 0.494_276_547_328_469_4;

/// The ports of a `bq_raw`, in the order the config lists them.
const PORTS: [&str; 6] = ["b0", "b1", "b2", "a0", "a1", "a2"];

// ---------------------------------------------------------------------------------------------
// The random numbers

/// SplitMix64: small, fast and the same everywhere, which a reproducible seed needs.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A generator for `seed`.
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    /// A number in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }

    /// A number between `low` and `high` spread evenly in the logarithm.
    fn log_range(&mut self, low: f64, high: f64) -> f64 {
        (low.ln() + (high.ln() - low.ln()) * self.unit()).exp()
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[self.below(of.len() as u64) as usize]
    }
}

// ---------------------------------------------------------------------------------------------
// The model of PipeWire

/// A chain's config as the file holds it, one channel (both are written alike).
#[derive(Debug, Clone, PartialEq)]
struct Conf {
    /// The preamp's `Mult`.
    pre: f64,
    /// Each slot's `b0 b1 b2 a0 a1 a2`.
    slots: Vec<[f64; 6]>,
    /// The crossfeed stage's ports, as `dir:b0`, `sum:Gain 1`, `mix:Gain 2` (the same values on
    /// both channels).
    stage: Vec<(String, f64)>,
    /// The loudness stage's ports, as `ld1:b0` (the same on both channels); empty in a config
    /// that has none (a test's own), which the node holds as the identity.
    ld: Vec<(String, f64)>,
}

/// `text`'s number after `key`.
fn field(text: &str, key: &str) -> Option<f64> {
    let rest = &text[text.find(key)? + key.len()..];
    rest.split_whitespace().next()?.parse().ok()
}

/// The values a rendered live config holds (the config is read back from its text, so what the
/// renderer wrote is what the node starts with).
fn parse_conf(text: &str) -> Conf {
    let mut conf = Conf {
        pre: f64::NAN,
        slots: vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; SLOTS],
        stage: Vec::new(),
        ld: Vec::new(),
    };
    for node in text.split("{ type = builtin").skip(1) {
        let name = node
            .split_whitespace()
            .skip_while(|word| *word != "name")
            .nth(2)
            .unwrap_or("");
        if name == "lpre" {
            conf.pre = field(node, "Mult = ").expect("the preamp's Mult");
        } else if let Some(path) = name
            .strip_prefix('l')
            .filter(|n| ["dir", "x", "s"].contains(n))
        {
            for port in PORTS {
                let value = field(node, &format!(" {port} = ")).expect("a coefficient");
                conf.stage.push((format!("{path}:{port}"), value));
            }
        } else if name == "lsum" {
            for gain in ["Gain 1", "Gain 2"] {
                let value = field(node, &format!("\"{gain}\" = ")).expect("a gain");
                conf.stage.push((format!("sum:{gain}"), value));
            }
        } else if name == "ldel" {
            let value = field(node, "\"Delay (s)\" = ").expect("a delay");
            conf.stage.push(("del:Delay (s)".to_string(), value));
        } else if name == "lvs" {
            for gain in ["Gain 1", "Gain 2", "Gain 3"] {
                let value = field(node, &format!("\"{gain}\" = ")).expect("a speakers gain");
                conf.stage.push((format!("vs:{gain}"), value));
            }
        } else if name == "lmix" {
            let value = field(node, "\"Gain 2\" = ").expect("the mixer's gain");
            conf.stage.push(("mix:Gain 2".to_string(), value));
        } else if name == "lld1" || name == "lld2" {
            for port in PORTS {
                let value = field(node, &format!(" {port} = ")).expect("a coefficient");
                conf.ld.push((format!("{}:{port}", &name[1..]), value));
            }
        } else if let Some(slot) = name.strip_prefix('l').and_then(|n| n.parse::<usize>().ok()) {
            for (value, port) in conf.slots[slot - 1].iter_mut().zip(PORTS) {
                *value = field(node, &format!(" {port} = ")).expect("a coefficient");
            }
        }
    }
    assert!(conf.pre.is_finite(), "the config has a preamp");
    conf
}

/// The value a port holds for `value`: an f32, and a coefficient held to plus or minus 10.
fn held_value(name: &str, value: f64) -> f32 {
    let value = value as f32;
    match name.contains(":b") || name.contains(":a") {
        true => value.clamp(-10.0, 10.0),
        false => value,
    }
}

/// The chain's node as PipeWire holds it.
#[derive(Debug, Clone)]
struct Node {
    id: u32,
    /// Its `object.serial`, which no other node is given.
    serial: u64,
    /// From when it can be found in the graph.
    listed_at_ms: u64,
    /// The config it started from, which a change of rate goes back to.
    started_from: Conf,
    /// The value of every port.
    ports: HashMap<String, f32>,
}

impl Node {
    fn start(id: u32, serial: u64, conf: &Conf, listed_at_ms: u64) -> Node {
        let mut node = Node {
            id,
            serial,
            listed_at_ms,
            started_from: conf.clone(),
            ports: HashMap::new(),
        };
        node.load_slots();
        for channel in ['l', 'r'] {
            let name = format!("{channel}pre:Mult");
            node.ports.insert(name.clone(), held_value(&name, conf.pre));
        }
        node
    }

    /// Every slot and every port of the stages (crossfeed, loudness) takes the config's values
    /// again.
    fn load_slots(&mut self) {
        // A config written without a stage (a test's own) holds the bypass.
        let bypass: Vec<(String, f64)> = crossfeed_params(&super::crossfeed::Stage::bypass())
            .into_iter()
            .filter(|(name, _)| name.starts_with('l'))
            .map(|(name, value)| (name[1..].to_string(), value))
            .collect();
        let stage = match self.started_from.stage.is_empty() {
            true => &bypass,
            false => &self.started_from.stage,
        };
        for channel in ['l', 'r'] {
            for (port, value) in stage {
                let name = format!("{channel}{port}");
                let held = held_value(&name, *value);
                self.ports.insert(name, held);
            }
        }
        // The loudness stage: the config's, or the identity a config without it holds.
        let identity: Vec<(String, f64)> = loudness_params(&loudness::Stage::identity().sections)
            .into_iter()
            .filter(|(name, _)| name.starts_with('l'))
            .map(|(name, value)| (name[1..].to_string(), value))
            .collect();
        let ld = match self.started_from.ld.is_empty() {
            true => &identity,
            false => &self.started_from.ld,
        };
        for channel in ['l', 'r'] {
            for (port, value) in ld {
                let name = format!("{channel}{port}");
                let held = held_value(&name, *value);
                self.ports.insert(name, held);
            }
        }
        for channel in ['l', 'r'] {
            for (index, values) in self.started_from.slots.iter().enumerate() {
                for (port, value) in PORTS.iter().zip(values) {
                    let name = format!("{channel}{:02}:{port}", index + 1);
                    let held = held_value(&name, *value);
                    self.ports.insert(name, held);
                }
            }
        }
    }

    fn value(&self, name: &str) -> f64 {
        f64::from(self.ports[name])
    }
}

/// What the lab keeps of a `pw-cli`'s life in the world, where the controller's session and the
/// events both reach it.
#[derive(Debug, Default)]
struct Cli {
    alive: bool,
    /// It ended since the session last asked.
    ended: bool,
    /// The next line ends it: `Some(true)` after applying the line, `Some(false)` without.
    dies_after_next_line: Option<bool>,
    /// It refused a `set-param` and the session has not read that yet.
    refused: bool,
    /// A refusal that shows after this many more reads of the flag: the session's reader task
    /// sets it a moment after `pw-cli` prints it.
    refusal_in: Option<u8>,
    /// When the session last started it, which keeps the next start [`RESTART_GAP_MS`] away.
    started_ms: Option<u64>,
}

impl Cli {
    /// It ends.
    fn end(&mut self) {
        self.alive = false;
        self.ended = true;
    }
}

/// Everything outside the controller.
#[derive(Debug)]
struct World {
    dir: PathBuf,
    clock_ms: u64,
    sink_rate: u32,
    /// The config on disk.
    conf: Option<Conf>,
    node: Option<Node>,
    next_id: u32,
    /// The id of the last node that went, which the next node may be given again.
    freed_id: Option<u32>,
    /// The serial of the last node started.
    next_serial: u64,
    /// Whether a new node may take the id of the node gone before it ([`Ids`]).
    reuse_ids: bool,
    /// How long after its start a node is listed.
    list_delay_ms: u64,
    /// The controller's own apply put the node now there (not an outside start).
    node_is_ours: bool,
    cli: Cli,
    /// Calls the chain refuses before it changes anything, by kind.
    fail_apply: u32,
    fail_remove: u32,
    fail_keep: u32,
    /// The next apply stops the old chain and fails to start the new one.
    fail_start: u32,
    /// Sends are refused (the pty does not take the line) until then.
    refuse_sends_until_ms: u64,
    /// The next line the session writes fails, and `pw-cli` is gone after it (a timeout, or a
    /// pty that took no more).
    fail_next_write: bool,
    /// A refusal shows this many reads of the flag after it happened.
    refusal_lag: u8,
    /// A counter that gives each outside write of the state file its own modified time.
    mtime: u64,
    /// What the controller and the chain wrote, for the rule that nothing is written at start.
    writes: usize,
    sends: usize,
    /// The sink's rate as the controller last read it (a look at the node, or the sink's rate
    /// for an apply).
    looked_rate: u32,
    /// A line was taken by a `pw-cli` that ended without applying it: nothing the controller can
    /// read says so, and until the node is replaced the liveness rule does not apply to it.
    lost_line: bool,
    /// What the node plays may differ from what the controller shows, and nothing the controller
    /// reads says so: the graph was rebuilt at the rate it already ran at (the slots went back),
    /// or a daemon started over a node it did not start (it shows the state file; the node plays
    /// what it was started from). Until a line carrying every slot lands, or the node is
    /// replaced, the liveness rule does not apply.
    unreconciled: bool,
    /// A rule the model saw broken, said once.
    broken: Vec<String>,
    /// The next line that lands is a repair line: a line before it failed, was refused or lost, or
    /// a `pw-cli` ended, so the session's next line carries every port, the caller's first and
    /// the rest after, whatever order the caller chose (`Outbox::line`). Such a line is the
    /// documented half-applied-line issue's, not the controller's order: the boundary check
    /// skips it, by this name.
    repair_owed: bool,
    /// The boundary check looks at repair lines too ([`REPAIR_LINES_ENV`]).
    check_repairs: bool,
    /// What happened, when a run is traced.
    trace: Option<Vec<String>>,
    /// A stream plays through the chain's node, whichever node that is.
    playing: bool,
    /// The computer's volume as the gain on the samples (1.0 full), and its mute.
    volume: f64,
    muted: bool,
    /// The next reads of the node alone (the session's `pw-dump <id>` before each line) find
    /// nothing: the read did not answer in time. The line is not written.
    fail_key_reads: u8,
    /// The level the node played at when it was last asked (its id, serial and dB), for the rule
    /// that the level never rises by more than [`LEVEL_RISE_DB`] between two states.
    level_last: Option<(u32, u64, f64)>,
    /// A rise in the level is allowed until this time on the world's clock: a hand edit, a
    /// preset load or apply, a scene recall, a stage or the EQ turned on, and the lines that
    /// follow them (see [`exempts`]).
    rise_ok_until_ms: u64,
    /// The node a line has landed on (its id and serial): the first line a node takes that turns
    /// the speakers' mixers wet gives back what the config's dry mixers play over the wet ones
    /// (the config's preamp is lower by the set's level), a dip and a rise the rule leaves out.
    first_line_of: Option<(u32, u64)>,
}

/// The most the level the chain plays at (the preamp in dB plus the speakers' level while their
/// mixers are wet) may rise between two states of the same node, every line boundary, event and
/// tick included: a change that raises it climbs at 0.32 dB a tick, and a rise bigger than this
/// is a jump. The sum is read from f32 ports, so the margin holds a hair of rounding.
const LEVEL_RISE_DB: f64 = 0.5 + 0.02;

/// How long after an event that may raise the level at once the rule is off, in ms: the lines it
/// leaves (a repeat a tick or two later) belong to it.
const RISE_GRACE_MS: u64 = 200;

/// The level `node` plays at: its preamp and the speakers' share.
fn level_of(node: &Node) -> f64 {
    let (_, pre_db) = played(node, 'l');
    pre_db + speakers_db(node, 'l')
}

/// The rule that the level never rises by more than [`LEVEL_RISE_DB`] between two states of one
/// node. `last` holds the level seen before; a node that is another one starts afresh.
fn level_step(
    last: &mut Option<(u32, u64, f64)>,
    node: &Node,
    exempt: bool,
    at: &str,
) -> Option<String> {
    let now = level_of(node);
    let before = last.replace((node.id, node.serial, now));
    let (id, serial, was) = before?;
    ((id, serial) == (node.id, node.serial) && !exempt && now - was > LEVEL_RISE_DB).then(|| {
        format!(
            "node {id} jumps {:+.3} dB in the level it plays at ({was:+.3} to {now:+.3} dB) {at}",
            now - was
        )
    })
}

/// Whether `event` may raise the level at once: a hand edit, a preset load or apply, a scene
/// recall, a stage or the EQ turned on, a node that starts afresh, and a daemon that reads or
/// takes over what another process wrote.
fn exempts(event: &Event) -> bool {
    match event {
        Event::Drag { .. }
        | Event::Add(_)
        | Event::Remove { .. }
        | Event::Load { .. }
        | Event::Cli { .. }
        | Event::Reload
        | Event::NodeRestart
        | Event::DaemonRestart
        | Event::Takeover { .. }
        | Event::Scene(_) => true,
        // Loudness never moves the level (it is no part of it): only turning it on counts as a
        // stage turned on.
        Event::Loudness { on, .. } => *on == Some(true),
        Event::Toggle(on) => *on,
        Event::Crossfeed { on, .. } => *on != Some(false),
        Event::Speakers { on, .. } => *on != Some(false),
        Event::Bypass(on) => !*on,
        _ => false,
    }
}

/// The ids the model gives new nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ids {
    /// Every new node a new id.
    Fresh,
    /// Every other new node the id of the node gone before it, as PipeWire may: only the serial
    /// tells the two apart.
    Reused,
}

/// The environment variable that, set (to anything), makes [`Ids::Reused`] the model of a lab
/// that no block of seeds chose one for: a single sequence (a `holds` case, `lab_trace`). The
/// blocks of seeds (the short runs and `make preamp-lab`) run both models whatever it says.
pub const REUSE_IDS_ENV: &str = "SVOLDER_LAB_REUSE_IDS";

/// The environment variable that, set, makes the boundary check of every profile look at repair
/// lines too (the line the session sends after one failed, was refused or lost, which carries every
/// port in the caller's order and the rest after). They are the documented half-applied-line
/// issue's shape and are skipped by default.
pub const REPAIR_LINES_ENV: &str = "SVOLDER_LAB_REPAIR_LINES";

thread_local! {
    /// The model of ids the labs made on this thread use.
    static IDS: Cell<Ids> = Cell::new(match std::env::var_os(REUSE_IDS_ENV) {
        Some(_) => Ids::Reused,
        None => Ids::Fresh,
    });
}

type Shared = Arc<Mutex<World>>;

fn world(shared: &Shared) -> MutexGuard<'_, World> {
    crate::locked(shared)
}

impl World {
    fn note(&mut self, what: impl FnOnce() -> String) {
        let at = self.clock_ms;
        if let Some(trace) = self.trace.as_mut() {
            trace.push(format!("[{at} ms] {}", what()));
        }
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("host-eq.toml")
    }

    /// What the controller and the chain have written and sent, and the state file as it is: a
    /// call that must write nothing leaves this as it was.
    fn footprint(&self) -> (usize, usize, Option<Vec<u8>>) {
        (
            self.writes,
            self.sends,
            std::fs::read(self.state_path()).ok(),
        )
    }

    /// Whether `pw-cli` is gone and the session may not start it again yet (its restart gap).
    fn cli_waits(&self) -> bool {
        !self.cli.alive
            && self
                .cli
                .started_ms
                .is_some_and(|started| self.clock_ms < started + RESTART_GAP_MS)
    }

    /// Whether something is armed that makes the next line fail once, or a refusal is
    /// on its way, or `pw-cli` is gone and cannot be started again yet (a run of refused sends
    /// ends it at every write, so the restart gap outlasts the refusal by up to a second): the
    /// controller is then not expected to have settled.
    fn disturbed(&self) -> bool {
        self.cli_waits()
            || self.fail_next_write
            || self.cli.dies_after_next_line.is_some()
            || self.fail_key_reads > 0
            || self.cli.refused
            || self.cli.refusal_in.is_some()
            || self.refuse_sends_until_ms > self.clock_ms
            || self.lost_line
            || self.unreconciled
            || self
                .node
                .as_ref()
                .is_some_and(|node| node.listed_at_ms > self.clock_ms)
    }

    fn state(&self) -> Option<HostEqState> {
        HostEqState::load(&self.state_path()).ok().flatten()
    }

    /// Start a node from the config on disk, in place of the one there, under a new id and a new
    /// serial. Under [`Ids::Reused`] every other node takes the id of the node gone before it, as
    /// PipeWire gives a freed id to a later object.
    fn start_node(&mut self, ours: bool) {
        self.next_serial += 1;
        let freed = self
            .node
            .take()
            .map(|node| node.id)
            .or(self.freed_id.take());
        let id = match freed {
            Some(id) if self.reuse_ids && self.next_serial.is_multiple_of(2) => id,
            _ => {
                self.next_id += 1;
                self.next_id
            }
        };
        self.node = self.conf.as_ref().map(|conf| {
            Node::start(
                id,
                self.next_serial,
                conf,
                self.clock_ms + self.list_delay_ms,
            )
        });
        self.node_is_ours = ours;
        self.lost_line = false;
        if ours {
            self.unreconciled = false;
        }
        let serial = self.next_serial;
        // The note's peaks are eight dense scans: worked out only for a traced run.
        let node = self.node.as_ref();
        let line = self.trace.is_some().then(|| {
            let pre = node.map(|node| 20.0 * node.value("lpre:Mult").log10());
            let peaks = node.map(|node| {
                let (sets, _) = played(node, 'l');
                RATES.map(|rate| format!("{:.2}", dense_peak(&sets, rate)))
            });
            format!("node {id} (serial {serial}) starts from the config on disk (ours: {ours}), preamp {pre:?}, slot peaks at the rates {peaks:?}")
        });
        if let Some(line) = line {
            self.note(|| line);
        }
    }

    fn listed(&self) -> Option<&Node> {
        self.node
            .as_ref()
            .filter(|node| node.listed_at_ms <= self.clock_ms)
    }

    /// Write the state file, as the command line does: each write has a modified time of its
    /// own, so a reader that goes by it never mistakes two writes for one.
    fn write_state_as_command_line(&mut self, state: &HostEqState) {
        state
            .save(&self.state_path())
            .expect("the state is written");
        self.mtime += 1;
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000 + self.mtime);
        let file = std::fs::File::options()
            .write(true)
            .open(self.state_path())
            .expect("the state file opens");
        file.set_modified(at).expect("its time is set");
        self.writes += 1;
    }

    /// What `Host::apply` does up to the start of the chain: the checks, the preamp and the
    /// config it will write. `rate_hz` is what the caller knows of the sink's rate.
    fn stage_apply(mut request: ApplyRequest) -> Result<(HostEqState, Conf), EqFailure> {
        let held = checked_request(&mut request, Manager::Child)?;
        let text = conf_text(
            &held,
            request.correction.preamp_db,
            "sink",
            Route::Smart,
            &request,
        );
        let state = HostEqState {
            on: true,
            name: request.name.clone(),
            origin: request.origin.clone(),
            target: "sink".to_string(),
            route: Route::Smart,
            manager: Manager::Child,
            previous_default: None,
            preamp_db: request.correction.preamp_db,
            filters: held.filters.clone(),
            slots: held.slots.clone(),
            crossfeed: request.crossfeed.clone(),
            crossfeed_on: request.crossfeed.is_some(),
            flat: request.flat,
            speakers: request
                .speakers
                .as_ref()
                .map(|speakers| speakers.id.clone()),
            speakers_on: request.speakers.is_some(),
            speakers_loaded: request
                .speakers
                .as_ref()
                .map(|speakers| speakers.id.clone()),
            crossfeed_loaded: None,
            loudness: None,
            bypass: None,
        };
        Ok((state, parse_conf(&text)))
    }

    /// Stop the chain and take its files away; the state file says off.
    fn remove_chain(&mut self) {
        if let Some(node) = self.node.take() {
            self.freed_id = Some(node.id);
        }
        self.conf = None;
        if let Some(mut state) = self.state() {
            state.on = false;
            state.crossfeed_on = false;
            state.flat = false;
            state
                .save(&self.state_path())
                .expect("the state is written");
            self.writes += 1;
        }
    }

    /// A `set-param` with `params` to node `id`, as `pw-cli` carries it out. Whether `pw-cli`
    /// is still there after it.
    fn set_param(&mut self, id: u32, params: &[(String, f64)]) {
        self.sends += 1;
        self.note(|| {
            let mut slots: Vec<&str> = params
                .iter()
                .filter(|(name, _)| name.starts_with('l') && !name.starts_with("lpre"))
                .map(|(name, _)| &name[1..3])
                .collect();
            slots.dedup();
            let pre = params
                .iter()
                .find(|(name, _)| name == "lpre:Mult")
                .map(|(_, mult)| format!("{:+.2} dB", 20.0 * mult.log10()));
            let mix = params
                .iter()
                .find(|(name, _)| name == "lmix:Gain 2")
                .map(|(_, mix)| *mix);
            format!("set-param node {id}: slots {slots:?} preamp {pre:?} stage mix {mix:?}")
        });
        let lands = !matches!(self.cli.dies_after_next_line, Some(false));
        self.lost_line |= !lands;
        // A line that lands after trouble is the repair line; one that does not leaves it owed.
        let repair = lands && std::mem::take(&mut self.repair_owed);
        self.repair_owed |= !lands;
        let rate = self.sink_rate;
        let first_line = lands
            && self.node.as_ref().is_some_and(|node| {
                node.id == id
                    && self.first_line_of.replace((node.id, node.serial))
                        != Some((node.id, node.serial))
            });
        let mut part_way = Vec::new();
        match self.node.as_mut() {
            Some(node) if node.id == id => {
                if lands {
                    if (1..=SLOTS).all(|slot| {
                        let prefix = format!("l{slot:02}:");
                        params.iter().any(|(name, _)| name.starts_with(&prefix))
                    }) {
                        self.unreconciled = false;
                    }
                    // A node may play a line taken in part, a node of the graph at a time:
                    // the loudness stage alone stays under unity once each of its sections has
                    // taken what the line gives it.
                    let section_ends: Vec<usize> = ["lld1:", "lld2:", "rld1:", "rld2:"]
                        .iter()
                        .filter_map(|section| {
                            params
                                .iter()
                                .rposition(|(name, _)| name.starts_with(section))
                        })
                        .collect();
                    // The kinds of port the line carries, to find the boundaries between them.
                    let kinds: Vec<u8> = params.iter().map(|(name, _)| port_kind(name)).collect();
                    let checked = (!repair || self.check_repairs)
                        && kinds.contains(&PORT_SLOT)
                        && kinds.contains(&PORT_STAGE);
                    for (at, (name, value)) in params.iter().enumerate() {
                        if node.ports.contains_key(name) {
                            node.ports.insert(name.clone(), held_value(name, *value));
                        }
                        // At each boundary between kinds of port and at the end of the line, as the
                        // unity rule is: the mixers' three ports are one state.
                        if (at + 1 == params.len() || kinds.get(at + 1) != Some(&kinds[at]))
                            && let Some(jump) = level_step(
                                &mut self.level_last,
                                node,
                                self.clock_ms < self.rise_ok_until_ms
                                    || repair
                                    || (first_line
                                        && params.iter().any(|(n, v)| {
                                            port_kind(n) == PORT_SPEAKERS
                                                && (n.ends_with(":Gain 2")
                                                    || n.ends_with(":Gain 3"))
                                                && *v > 0.0
                                        })),
                                &format!("part way through a line, after {name}"),
                            )
                        {
                            part_way.push(jump);
                        }
                        let last = at + 1 == params.len();
                        if (!repair || self.check_repairs)
                            && (last || (checked && kinds.get(at + 1) != Some(&kinds[at])))
                            && let Some((over, parts)) = boundary_over(node, rate)
                        {
                            part_way.push(format!(
                                "node {} plays {:+.3} dB over unity part way through a line, after {name}, at {rate} Hz, {parts} (a boundary between kinds of port, or the end of a line)",
                                node.id, over
                            ));
                        }
                        if section_ends.contains(&at) {
                            let channel = name.chars().next().unwrap_or('l');
                            let sets = loudness_sets(node, channel);
                            let peak = dense_peak(&sets, rate);
                            if peak > TOLERANCE_DB {
                                part_way.push(format!(
                                    "node {} plays the loudness stage {peak:+.3} dB over unity part way through a line, after {name}, at {rate} Hz: {sets:?}",
                                    node.id
                                ));
                            }
                        }
                    }
                }
            }
            _ => {
                // The id is not the node's: nothing is set, and `pw-cli` says so.
                match self.refusal_lag {
                    0 => self.cli.refused = true,
                    lag => self.cli.refusal_in = Some(lag),
                }
                self.repair_owed = true;
                if self.node_is_ours && self.node.is_some() {
                    self.broken
                        .push(format!("a set-param went to the stale id {id}, after the controller's own apply put node {} there",
                            self.node.as_ref().map_or(0, |node| node.id)));
                }
            }
        }
        self.broken.extend(part_way);
        if self.cli.dies_after_next_line.take().is_some() {
            self.repair_owed = true;
            self.cli.end();
            self.cli.refused = false;
            self.cli.refusal_in = None;
        }
    }
}

fn failure(what: &str) -> EqFailure {
    EqFailure {
        what: what.to_string(),
        next: "try again".to_string(),
    }
}

thread_local! {
    /// How many times the scene rule was asked on this thread, so that a test can tell that it
    /// is not vacuous.
    static SCENE_ASKED: Cell<usize> = const { Cell::new(0) };
}

const PORT_PREAMP: u8 = 0;
const PORT_SLOT: u8 = 1;
const PORT_LOUDNESS: u8 = 2;
const PORT_STAGE: u8 = 3;
const PORT_SPEAKERS: u8 = 4;

/// The kind of the port `name` (`lpre:Mult`, `l01:b0`, `lld1:b0`, `ldir:b0`, `lvs:Gain 1`).
fn port_kind(name: &str) -> u8 {
    match name.get(1..3) {
        Some("pr") => PORT_PREAMP,
        Some("ld") => PORT_LOUDNESS,
        Some("vs") => PORT_SPEAKERS,
        Some(digits) if digits.bytes().all(|b| b.is_ascii_digit()) => PORT_SLOT,
        _ => PORT_STAGE,
    }
}

/// How far over unity the node plays as it holds its ports now, on either channel, at `rate`;
/// `None` at or under unity (the rule of [`Lab::check`]).
fn boundary_over(node: &Node, rate: u32) -> Option<(f64, String)> {
    thread_local! {
        static PEAKS: std::cell::RefCell<Peaks> = std::cell::RefCell::new(Peaks::default());
    }
    let worst = ['l', 'r']
        .into_iter()
        .map(|channel| {
            let (sets, pre_db) = played(node, channel);
            let peak = PEAKS.with_borrow_mut(|peaks| peaks.of(&sets, rate));
            let stage = StageGains.of(&stage_values(node, channel), rate);
            let speakers = speakers_db(node, channel);
            (
                peak + stage + speakers + pre_db,
                format!(
                    "on {channel}: the slots and the loudness peak {peak:+.3} dB, the crossfeed stage adds up to {stage:+.3} dB and the speakers {speakers:+.3} dB under a preamp of {pre_db:+.3} dB"
                ),
            )
        })
        .fold((f64::NEG_INFINITY, String::new()), |worst, next| {
            if next.0 > worst.0 { next } else { worst }
        });
    (worst.0 > TOLERANCE_DB).then_some(worst)
}

/// The controller's `Chain` over the world.
#[derive(Clone)]
struct ModelChain {
    world: Shared,
}

#[async_trait]
impl Chain for ModelChain {
    async fn apply(&self, request: ApplyRequest) -> Result<HostEqState, EqFailure> {
        let mut world = world(&self.world);
        if world.fail_apply > 0 {
            world.fail_apply -= 1;
            return Err(failure("the apply is refused"));
        }
        let (mut state, conf) = World::stage_apply(request)?;
        // The loudness's settings stay across an apply, as the real host keeps them.
        state.loudness = world.state().and_then(|previous| previous.loudness);
        if world.fail_start > 0 {
            // The old chain is stopped and the new one does not start.
            world.fail_start -= 1;
            world.remove_chain();
            return Err(failure("the chain did not start"));
        }
        world.conf = Some(conf);
        world.start_node(true);
        world.clock_ms += START_WATCH_MS;
        state
            .save(&world.state_path())
            .expect("the state is written");
        world.writes += 1;
        Ok(state)
    }

    async fn remove(&self) -> Result<(), EqFailure> {
        let mut world = world(&self.world);
        if world.fail_remove > 0 {
            world.fail_remove -= 1;
            return Err(failure("the chain cannot be stopped"));
        }
        world.remove_chain();
        Ok(())
    }

    async fn status(&self) -> Status {
        let world = world(&self.world);
        Status {
            state: world.state(),
            running: Some(world.node.is_some()),
            unreadable: None,
        }
    }

    async fn keep_conf(&self, request: ApplyRequest) -> Result<(), EqFailure> {
        let mut world = world(&self.world);
        if world.fail_keep > 0 {
            world.fail_keep -= 1;
            return Err(failure("the config cannot be written"));
        }
        let Some(state) = world.state().filter(|state| state.on) else {
            return Ok(());
        };
        let text = kept_conf(&state, request);
        world.conf = Some(parse_conf(&text));
        world.writes += 1;
        Ok(())
    }

    fn unavailable(&self) -> Option<EqFailure> {
        None
    }

    fn state(&self) -> Option<HostEqState> {
        world(&self.world).state()
    }
}

/// The controller's `Params` over the world: the session's own code ([`Session`]: the kept node,
/// the refusal, the repair, the retry) over a `pw-cli` that lives in the world.
type ModelParams = Session<ModelLink>;

/// What the model gives the session in place of a child process and `pw-dump`: the `pw-cli` of
/// the world, its restart gap in the world's clock, and the node the world lists.
struct ModelLink {
    world: Shared,
    /// The controller dropped the node to look at the chain: the next read is that look.
    looking: bool,
    /// The computer's volume the last read of the graph saw.
    level: Option<SinkLevel>,
}

impl ModelLink {
    fn params(world: Shared) -> ModelParams {
        Session::new(ModelLink {
            world,
            looking: false,
            level: None,
        })
    }
}

#[async_trait]
impl Link for ModelLink {
    fn take_refusal(&mut self) -> bool {
        let mut world = world(&self.world);
        let refused = world.cli.alive && std::mem::take(&mut world.cli.refused);
        world.cli.refusal_in = match world.cli.refusal_in {
            Some(1) => {
                world.cli.refused = true;
                None
            }
            Some(more) => Some(more - 1),
            None => None,
        };
        refused
    }

    fn ended(&mut self) -> bool {
        std::mem::take(&mut world(&self.world).cli.ended)
    }

    fn ensure_running(&mut self) -> Option<bool> {
        let mut world = world(&self.world);
        if world.cli.alive {
            return Some(false);
        }
        if world.cli_waits() {
            return None;
        }
        world.cli = Cli {
            alive: true,
            started_ms: Some(world.clock_ms),
            ..Cli::default()
        };
        Some(true)
    }

    async fn wait_ready(&mut self) {}

    fn node_dropped(&mut self) {
        self.looking = true;
    }

    async fn write(&mut self, id: u32, params: &[(String, f64)]) -> bool {
        let mut world = world(&self.world);
        let once = std::mem::take(&mut world.fail_next_write);
        match once || world.refuse_sends_until_ms > world.clock_ms {
            true => {
                world.repair_owed = true;
                world.cli.end();
                false
            }
            false => {
                world.set_param(id, params);
                true
            }
        }
    }

    async fn read_node(&mut self) -> Option<LiveNode> {
        let looking = std::mem::take(&mut self.looking);
        let mut world = world(&self.world);
        // The dump the node is read from shows the sink's volume, whether the node is listed or
        // not.
        self.level = Some(SinkLevel {
            linear: world.volume,
            muted: world.muted,
        });
        let found = world.listed().map(|node| LiveNode {
            id: node.id,
            serial: Some(node.serial),
            rate_hz: Some(world.sink_rate),
            sink: None,
            playing: world.playing,
            params: node
                .ports
                .iter()
                .map(|(name, value)| (name.clone(), f64::from(*value)))
                .collect(),
        })?;
        if looking {
            world.looked_rate = world.sink_rate;
        }
        Some(found)
    }

    async fn sink_rate(&mut self) -> Option<u32> {
        let mut world = world(&self.world);
        world.looked_rate = world.sink_rate;
        Some(world.sink_rate)
    }

    fn level(&self) -> Option<SinkLevel> {
        self.level
    }

    async fn read_key(&mut self, id: u32) -> Option<NodeKey> {
        {
            let mut world = world(&self.world);
            if world.fail_key_reads > 0 {
                world.fail_key_reads -= 1;
                return None;
            }
        }
        self.read_node()
            .await
            .map(|node| node.key())
            .filter(|key| key.id == id)
    }
}

// ---------------------------------------------------------------------------------------------
// The independent evaluator

/// The sum in dB of the biquads `[b0, b1, b2, a1, a2]` at angular frequency `w`, written in
/// `sin^2(w/2)` below half of Nyquist's `w` and in `cos^2(w/2)` above, so a response near DC or
/// Nyquist is not lost to cancellation. (The same maths as the safety test's, which works from
/// filters; this works from the coefficients the node holds.)
fn summed_db(sets: &[[f64; 5]], w: f64) -> f64 {
    let (sign, x) = match w <= std::f64::consts::FRAC_PI_2 {
        true => (1.0, (w / 2.0).sin().powi(2)),
        false => (-1.0, (w / 2.0).cos().powi(2)),
    };
    let power = |p: [f64; 3]| {
        let sum = p[0] + p[1] + p[2];
        sum * sum - 4.0 * (p[0] * p[1] + 4.0 * p[0] * p[2] + p[1] * p[2]) * x
            + 16.0 * p[0] * p[2] * x * x
    };
    sets.iter()
        .map(|c| {
            10.0 * (power([c[0], sign * c[1], c[2]]) / power([1.0, sign * c[3], c[4]])).log10()
        })
        .sum()
}

/// The angle of the complex roots of `p0 z^2 + p1 z + p2`, when it has a pair.
fn root_angle(p0: f64, p1: f64, p2: f64) -> Option<f64> {
    let (b, c) = (p1 / p0, p2 / p0);
    let radius = c.abs().sqrt();
    let cosine = -b / (2.0 * radius);
    (c > 0.0 && cosine.abs() < 1.0).then(|| cosine.acos())
}

/// The highest `sets` rise anywhere from 0.01 Hz to Nyquist at `rate_hz`: [`dense_scan`] of the
/// slots that are not the identity.
fn dense_peak(sets: &[[f64; 5]], rate_hz: u32) -> f64 {
    // An identity slot adds exactly +0 dB at every frequency (both powers are exactly 1) and has
    // no roots, so leaving it out changes no sum by a bit; one is kept when all are, as the sum of
    // nothing is -0.
    let identity = |c: &[f64; 5]| c[0] == 1.0 && c[1..].iter().all(|v| *v == 0.0);
    let mut active: Vec<[f64; 5]> = sets.iter().filter(|c| !identity(c)).copied().collect();
    if active.is_empty() {
        active.extend(sets.first());
    }
    dense_scan(&active, rate_hz)
}

/// The highest `sets` rise anywhere from 0.01 Hz to Nyquist at `rate_hz`, DC and Nyquist
/// included: a log grid in frequency, a second log grid in the distance from Nyquist, each
/// biquad's own pole and zero angles, and every local maximum zoomed in on by repeated scans.
fn dense_scan(sets: &[[f64; 5]], rate_hz: u32) -> f64 {
    use std::f64::consts::PI;
    let rate = f64::from(rate_hz);
    let total = |w: f64| summed_db(sets, w);
    let mut ws = vec![0.0, PI];
    let (low, high) = (0.01f64, rate / 2.0 * 0.99999);
    let steps = 3072;
    ws.extend(
        (0..=steps)
            .map(|i| 2.0 * PI * low * (high / low).powf(f64::from(i) / f64::from(steps)) / rate),
    );
    let nyquist_steps = 2048;
    let (near, far) = (PI * 1e-8, PI / 2.0);
    ws.extend(
        (0..=nyquist_steps)
            .map(|i| PI - near * (far / near).powf(f64::from(i) / f64::from(nyquist_steps))),
    );
    for c in sets {
        ws.extend(root_angle(c[0], c[1], c[2]));
        ws.extend(root_angle(1.0, c[3], c[4]));
    }
    ws.retain(|w| w.is_finite());
    ws.sort_by(f64::total_cmp);
    ws.dedup();
    let values: Vec<f64> = ws.iter().map(|&w| total(w)).collect();
    let last = ws.len() - 1;
    let mut best = f64::NEG_INFINITY;
    for i in 0..=last {
        best = best.max(values[i]);
        let top = i == 0 || values[i] > values[i - 1];
        if !(top && (i == last || values[i] >= values[i + 1])) {
            continue;
        }
        let (mut from, mut to) = (ws[i.saturating_sub(1)], ws[(i + 1).min(last)]);
        for _ in 0..3 {
            let scan = 32;
            let mut here = (f64::NEG_INFINITY, from);
            for k in 0..=scan {
                let w = from + (to - from) * f64::from(k) / f64::from(scan);
                let value = total(w);
                if value > here.0 {
                    here = (value, w);
                }
            }
            best = best.max(here.0);
            let width = (to - from) / f64::from(scan);
            (from, to) = ((here.1 - width).max(0.0), (here.1 + width).min(PI));
        }
    }
    best
}

/// Whether a biquad `[b0, b1, b2, a1, a2]` is stable (a numerator equal to the denominator is
/// the identity and always is).
fn stable(c: &[f64; 5]) -> bool {
    (c[0] == 1.0 && c[1] == c[3] && c[2] == c[4]) || (c[4].abs() < 1.0 && c[3].abs() < 1.0 + c[4])
}

/// The biquad `[b0, b1, b2, a1, a2]` the node's `bq_raw` named `name` plays (its six ports, over
/// `a0`).
fn raw_set(node: &Node, name: &str) -> [f64; 5] {
    let at = |port: &str| node.value(&format!("{name}:{port}"));
    let a0 = at("a0");
    [
        at("b0") / a0,
        at("b1") / a0,
        at("b2") / a0,
        at("a1") / a0,
        at("a2") / a0,
    ]
}

/// What the node plays on one channel: the biquads (the slots, then the loudness stage's two
/// sections) and the preamp in dB.
fn played(node: &Node, channel: char) -> (Vec<[f64; 5]>, f64) {
    let sets = (1..=SLOTS)
        .map(|slot| raw_set(node, &format!("{channel}{slot:02}")))
        .chain(loudness_sets(node, channel))
        .collect();
    (
        sets,
        20.0 * node.value(&format!("{channel}pre:Mult")).log10(),
    )
}

/// The loudness stage's two sections the node plays on one channel.
fn loudness_sets(node: &Node, channel: char) -> Vec<[f64; 5]> {
    ["ld1", "ld2"]
        .iter()
        .map(|name| raw_set(node, &format!("{channel}{name}")))
        .collect()
}

/// The values of the crossfeed stage's ports on one channel, in the order [`stage_gain_db`]
/// reads them: the three paths' `b0 b1 b2 a0 a1 a2`, the sum's two gains, the delay, the final
/// mixer's gain for the delayed path.
fn stage_values(node: &Node, channel: char) -> Vec<f64> {
    let mut values = Vec::new();
    for path in ["dir", "x", "s"] {
        for port in PORTS {
            values.push(node.value(&format!("{channel}{path}:{port}")));
        }
    }
    for port in ["sum:Gain 1", "sum:Gain 2", "del:Delay (s)", "mix:Gain 2"] {
        values.push(node.value(&format!("{channel}{port}")));
    }
    values
}

/// The most the stage the ports `values` hold adds at `rate_hz`, in dB (never below 0), from
/// the ports alone: one output is `dir·C + mix·z^-n·(sum2·s·C + sum1·x·O)`, so the two inputs
/// aligned to add make `|dir + mix·sum2·z^-n·s| + |mix·sum1·x|` at each frequency, over a grid of
/// 1 Hz steps. The delay plays in whole samples, truncated; a delay within a thousandth of a
/// sample of a whole one (PipeWire truncates its f32 product, this an f64 one) plays as the worse
/// of the two samples beside it.
fn stage_gain_db(values: &[f64], rate_hz: u32) -> f64 {
    let section = |at: usize| {
        let c = &values[at * 6..at * 6 + 6];
        [
            c[0] / c[3],
            c[1] / c[3],
            c[2] / c[3],
            c[4] / c[3],
            c[5] / c[3],
        ]
    };
    let (dir, x, s) = (section(0), section(1), section(2));
    let (sum1, sum2, delay, mix) = (values[18], values[19], values[20], values[21]);
    let rate = f64::from(rate_hz);
    let product = delay * rate;
    let mut samples = vec![(product - 1e-3).floor().max(0.0)];
    if (product + 1e-3).floor() != samples[0] {
        samples.push((product + 1e-3).floor());
    }
    let at = |c: &[f64; 5], w: f64| {
        let z = |n: f64| ((-w * n).cos(), (-w * n).sin());
        let (z1, z2) = (z(1.0), z(2.0));
        let num = (c[0] + c[1] * z1.0 + c[2] * z2.0, c[1] * z1.1 + c[2] * z2.1);
        let den = (1.0 + c[3] * z1.0 + c[4] * z2.0, c[3] * z1.1 + c[4] * z2.1);
        let d = den.0 * den.0 + den.1 * den.1;
        (
            (num.0 * den.0 + num.1 * den.1) / d,
            (num.1 * den.0 - num.0 * den.1) / d,
        )
    };
    let mut worst = 1.0f64;
    for f in 1..=(rate_hz / 2).max(1) {
        let w = 2.0 * std::f64::consts::PI * f64::from(f) / rate;
        let (hd, hs, hx) = (at(&dir, w), at(&s, w), at(&x, w));
        for n in &samples {
            let d = ((-w * n).cos(), (-w * n).sin());
            let side = (
                mix * sum2 * (hs.0 * d.0 - hs.1 * d.1),
                mix * sum2 * (hs.0 * d.1 + hs.1 * d.0),
            );
            let direct = (hd.0 + side.0, hd.1 + side.1);
            worst = worst.max(direct.0.hypot(direct.1) + (mix * sum1 * hx.0.hypot(hx.1)));
        }
    }
    20.0 * worst.log10()
}

// ---------------------------------------------------------------------------------------------
// The events

/// One thing that happens: a hand on the controller, the command line, PipeWire, the sink.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A hand drags a point: `steps` updates, `gap_ticks` ticks after each, then the release.
    Drag {
        pick: usize,
        kind: Option<FilterKind>,
        f: Option<f64>,
        g: Option<f64>,
        q: Option<f64>,
        steps: u8,
        gap_ticks: u8,
    },
    /// A point is added.
    Add(SourceFilter),
    /// A point is removed.
    Remove { pick: usize },
    /// A preset is loaded.
    Load {
        filters: Vec<SourceFilter>,
        stated: f64,
    },
    /// The EQ is turned on or off.
    Toggle(bool),
    /// The command line applies a curve (at 48 kHz, through the process manager) while the
    /// daemon runs: the config is written, the chain started, `gap_ticks` ticks go by, and the
    /// state file is written.
    Cli {
        filters: Vec<SourceFilter>,
        stated: f64,
        gap_ticks: u8,
    },
    /// The daemon reads the state file now.
    Reload,
    /// The chain's node starts again from the config on disk.
    NodeRestart,
    /// The sink changes its rate.
    Rate(u32),
    /// `pw-cli` dies: now, or after the next line (which it applies, or not).
    CliDeath { after_line: Option<bool> },
    /// A hand changes the crossfeed: it goes on or off, takes the model at `model` of the list,
    /// an amount in percent or a total delay in ms, as a drag (`done` false) or a release.
    Crossfeed {
        on: Option<bool>,
        model: Option<usize>,
        amount_pct: Option<f64>,
        delay_ms: Option<f64>,
        done: bool,
    },
    /// `pw-cli` takes no line for `ms`.
    RefuseSends { ms: u32 },
    /// The chain refuses the next call of this kind (0 apply, 1 remove, 2 keep, 3 start).
    Fail { kind: u8 },
    /// New nodes are listed `ms` after they start.
    ListDelay { ms: u32 },
    /// Time passes, in ticks of 50 ms.
    Ticks(u32),
    /// The daemon restarts: a new controller over the same files and the same chain.
    DaemonRestart,
    /// The next line the session writes fails once, and `pw-cli` ends with it.
    FailNextWrite,
    /// From now on a refusal shows this many reads of the flag late.
    RefusalLag(u8),
    /// The EQ's slots and the stage's ports go back to the config's values at the same node and
    /// rate, the preamp as last set (PipeWire rebuilt the slots alone).
    SlotRevert,
    /// A stream starts or stops playing through the chain.
    Playing(bool),
    /// A hand changes the virtual speakers: the set at `set` of the lab's two (`set-a`, `set-b`),
    /// and on or off, applied last.
    Speakers {
        on: Option<bool>,
        set: Option<usize>,
    },
    /// The computer's volume goes to `pct` percent of the volume scale: through the daemon
    /// (`daemon`), whose write hands the controller what it read back at once, or from outside,
    /// which the next look reads.
    Volume { pct: f64, daemon: bool },
    /// The computer's volume moves by `delta` percent (a volume key), held to 0-100.
    VolumeStep { delta: f64, daemon: bool },
    /// The sink is muted or unmuted, through the daemon or from outside.
    Mute { muted: bool, daemon: bool },
    /// The daemon crashed and a new one takes the chain over (as [`Event::DaemonRestart`]), and
    /// its first line does not land: the read of the node before it does not answer in time (the
    /// line is not written, and the next one goes on the next tick), or (`lost`) a `pw-cli` takes
    /// it and dies without applying it.
    Takeover { lost: bool },
    /// A hand changes the loudness: on or off, the strength, the reference as a number or the
    /// volume now, as a drag (`done` false) or a release.
    Loudness {
        on: Option<bool>,
        strength_pct: Option<f64>,
        reference_pct: Option<f64>,
        reference_now: bool,
        done: bool,
    },
    /// A scene's computer part is recalled: the EQ, loudness, crossfeed and the virtual speakers
    /// set as one change.
    Scene(Box<ComputerPart>),
    /// The bypass goes on or off: every stage off as one change, or brought back.
    Bypass(bool),
}

fn filter(rng: &mut Rng, hand: bool) -> SourceFilter {
    let kind = rng.pick(&[
        FilterKind::Peaking,
        FilterKind::LowShelf,
        FilterKind::HighShelf,
    ]);
    match hand {
        true => SourceFilter {
            kind,
            f: rng.log_range(20.0, 20_000.0),
            g: rng.range(-12.0, 12.0),
            q: rng.log_range(0.1, 10.0),
        },
        false => SourceFilter {
            kind,
            f: rng.log_range(10.0, 24_000.0),
            g: match rng.chance(70) {
                true => rng.range(-12.0, 12.0),
                false => rng.range(-30.0, 30.0),
            },
            q: rng.log_range(0.05, 20.0),
        },
    }
}

fn curve(rng: &mut Rng) -> (Vec<SourceFilter>, f64) {
    let count = match rng.chance(60) {
        true => 1 + rng.below(4) as usize,
        false => 1 + rng.below(16) as usize,
    };
    let hand = rng.chance(60);
    let filters = (0..count).map(|_| filter(rng, hand)).collect();
    let stated = match rng.below(10) {
        0..=5 => 0.0,
        6..=8 => rng.range(-15.0, 0.0),
        _ => rng.range(0.0, 6.0),
    };
    (filters, stated)
}

/// A sequence of `len` events for `seed`.
pub fn generate(seed: u64, len: usize) -> Vec<Event> {
    let mut rng = Rng::new(seed);
    // Each sequence leans a little on its own mix, so some are all drags and some all
    // restarts.
    (0..len)
        .map(|_| {
            let roll = rng.below(100);
            event_for(&mut rng, roll)
        })
        .collect()
}

/// The rolls (of the 100 of [`event_for`]) of the events the targeted profile raises: a command
/// line apply, a node that starts again, a refusal read late, a write that fails once, and the
/// drag, listed four times so that half of the draw is a drag (the lines a replaced chain
/// refuses are a drag's).
const TARGETED_ROLLS: [u64; 8] = [53, 59, 96, 94, 20, 25, 30, 35];

/// A sequence of `len` events for `seed` from the targeted profile: [`generate`]'s mix with
/// nearly half of the events taken from [`TARGETED_ROLLS`]: the events whose compound can leave a
/// chain replaced while a line is refused or fails (a command line apply, a node that starts
/// again, a refusal read late and a write that fails once) and the drag that sends the line. A
/// separate stream: the seeds of [`generate`] do not depend on it.
pub fn generate_targeted(seed: u64, len: usize) -> Vec<Event> {
    let mut rng = Rng::new(seed ^ 0x7A12_6E7E_D00D_F00D);
    (0..len)
        .map(|_| {
            let roll = match rng.chance(45) {
                true => rng.pick(&TARGETED_ROLLS),
                false => rng.below(100),
            };
            event_for(&mut rng, roll)
        })
        .collect()
}

/// A sequence of `len` events for `seed` from the speakers profile: [`generate`]'s mix with one
/// event in eight a change of the virtual speakers (a set chosen, on, off, or both). A stream of
/// its own: the seeds of the other profiles do not depend on it.
pub fn generate_speakers(seed: u64, len: usize) -> Vec<Event> {
    let mut rng = Rng::new(seed ^ 0x5BEA_CE25_0FF1_CE00);
    (0..len)
        .map(|_| match rng.chance(12) {
            true => Event::Speakers {
                on: match rng.below(3) {
                    0 => None,
                    1 => Some(true),
                    _ => Some(false),
                },
                set: rng.chance(40).then(|| rng.below(2) as usize),
            },
            false => {
                let roll = rng.below(100);
                event_for(&mut rng, roll)
            }
        })
        .collect()
}

/// The sink's rates the high-rate profile moves among: [`RATES`] and the two above 384 kHz, where
/// no stage runs but a graph may be rebuilt.
const HIGH_RATES: [u32; 10] = [
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
];

/// A sequence of `len` events for `seed` from the high-rate profile: rate changes among
/// [`HIGH_RATES`] (about a third of the draw) mixed with crossfeed changes (on, off, a model, an
/// amount, a delay; drags in flight and their ends), ticks, drags of the points, and the rest of
/// [`generate`]'s mix (faults, restarts, loads). A separate stream: the seeds of [`generate`] and
/// [`generate_targeted`] do not depend on it.
pub fn generate_highrate(seed: u64, len: usize) -> Vec<Event> {
    let mut rng = Rng::new(seed ^ 0x4869_5261_7465_5F31);
    (0..len)
        .map(|_| match rng.below(100) {
            0..=29 => Event::Rate(rng.pick(&HIGH_RATES)),
            30..=54 => event_for(&mut rng, 16),
            55..=69 => Event::Ticks(1 + rng.below(30) as u32),
            70..=79 => event_for(&mut rng, 20),
            _ => {
                let roll = rng.below(100);
                event_for(&mut rng, roll)
            }
        })
        .collect()
}

/// A sequence of `len` events for `seed` from the loudness profile: about two events in five move
/// the computer's volume (a key's step of 1 %, anywhere, or to 0 or 100 %; through the daemon or
/// from outside), with mutes, crash takeovers whose first line does not land, loudness changes (on, off, strength, reference as a number or the
/// volume now, drags and releases), rate changes among [`HIGH_RATES`], ticks, and the rest of
/// [`generate`]'s mix. A stream of its own: the seeds of the other profiles do not depend on it.
pub fn generate_loudness(seed: u64, len: usize) -> Vec<Event> {
    let mut rng = Rng::new(seed ^ 0x4C6F_7564_6E65_7373);
    (0..len)
        .map(|_| match rng.below(100) {
            0..=39 => {
                let daemon = rng.chance(60);
                match rng.below(10) {
                    0..=4 => Event::VolumeStep {
                        delta: if rng.chance(50) { 1.0 } else { -1.0 },
                        daemon,
                    },
                    5..=7 => Event::Volume {
                        pct: rng.range(0.0, 100.0),
                        daemon,
                    },
                    8 => Event::Volume { pct: 0.0, daemon },
                    _ => Event::Volume { pct: 100.0, daemon },
                }
            }
            40..=44 => Event::Mute {
                muted: rng.chance(50),
                daemon: rng.chance(60),
            },
            45..=46 => Event::Takeover {
                lost: rng.chance(50),
            },
            47..=59 => {
                let on = rng.chance(35).then(|| rng.chance(70));
                let strength_pct = rng.chance(30).then(|| rng.range(0.0, 100.0));
                let reference_pct = rng.chance(20).then(|| rng.range(1.0, 100.0));
                let reference_now = rng.chance(10);
                // A change that names nothing is refused: every one names something.
                let names = strength_pct.is_some() || reference_pct.is_some() || reference_now;
                Event::Loudness {
                    on: on.or((!names).then_some(true)),
                    strength_pct,
                    reference_pct,
                    reference_now,
                    done: rng.chance(75),
                }
            }
            60..=67 => Event::Rate(rng.pick(&HIGH_RATES)),
            68..=79 => Event::Ticks(1 + rng.below(30) as u32),
            _ => {
                let roll = rng.below(100);
                event_for(&mut rng, roll)
            }
        })
        .collect()
}

/// The part a scene recall draws: the EQ on or off with a curve, loudness on or off with a
/// strength and reference, crossfeed on or off with a model, the virtual speakers on or off with
/// a set (never both stages on at once: a part that asks for both is refused, which the unit
/// tests hold).
fn scene_part(rng: &mut Rng) -> ComputerPart {
    let (filters, preamp_db) = curve(rng);
    let crossfeed_on = rng.chance(35);
    let speakers_on = !crossfeed_on && rng.chance(35);
    let loudness = rng.chance(80).then(|| LoudnessSettings {
        on: rng.chance(50),
        strength_pct: rng.range(0.0, 100.0),
        reference_pct: rng.range(1.0, 100.0),
    });
    let crossfeed = (crossfeed_on || rng.chance(50)).then(|| {
        let mut settings = CrossfeedSettings::for_model(CrossfeedModel::ALL[rng.below(4) as usize]);
        if rng.chance(40) {
            settings.amount = rng.range(0.15, 0.47);
        }
        settings
    });
    let speakers = (speakers_on || rng.chance(40)).then(|| SETS[rng.below(2) as usize].to_string());
    ComputerPart {
        eq_on: rng.chance(70),
        name: "Scene".to_string(),
        origin: None,
        preamp_db,
        filters,
        loudness,
        crossfeed_on,
        crossfeed,
        speakers_on,
        speakers,
    }
}

/// A sequence of `len` events for `seed` from the scene profile: about one event in four a scene
/// recall with a random part (the EQ, loudness, crossfeed and the virtual speakers each on or
/// off), one in seven a move of the computer's volume or its mute, mixed with [`generate`]'s events, faults, a stream that plays or not, restarts and rate
/// changes among them. A stream of its own: the seeds of the other profiles do not depend on it.
pub fn generate_scene(seed: u64, len: usize) -> Vec<Event> {
    let mut rng = Rng::new(seed ^ 0x5CE9_E5C0_4D17_A7E1);
    (0..len)
        .map(|_| match rng.below(100) {
            0..=24 => Event::Scene(Box::new(scene_part(&mut rng))),
            // The computer's volume moves, so that loudness plays at a depth under the recalls.
            25..=39 => {
                let daemon = rng.chance(60);
                match rng.below(10) {
                    0..=3 => Event::VolumeStep {
                        delta: if rng.chance(50) { 1.0 } else { -1.0 },
                        daemon,
                    },
                    4..=7 => Event::Volume {
                        pct: rng.range(5.0, 90.0),
                        daemon,
                    },
                    8 => Event::Mute {
                        muted: rng.chance(50),
                        daemon,
                    },
                    _ => Event::Volume { pct: 100.0, daemon },
                }
            }
            _ => {
                let roll = rng.below(100);
                event_for(&mut rng, roll)
            }
        })
        .collect()
}

/// The climb profile: a stream plays through a chain with the EQ and stages on, and the events
/// that raise the level (stages off, the EQ off, the bypass) come among rate changes up to 768 kHz,
/// rebuilds, node restarts, `pw-cli` deaths and time. Its own stream of numbers, so no older
/// profile's seeds move.
pub fn generate_climb(seed: u64, len: usize) -> Vec<Event> {
    let mut rng = Rng::new(seed ^ 0xC11B_C11B_0A0A_3232);
    let mut events = vec![Event::Toggle(true), Event::Playing(true)];
    events.extend((0..len).map(|_| match rng.below(100) {
        0..=27 => Event::Ticks(1 + rng.below(30) as u32),
        28..=31 => Event::Playing(rng.chance(80)),
        32..=41 => Event::Crossfeed {
            on: Some(rng.chance(50)),
            model: rng.chance(40).then(|| rng.below(4) as usize),
            amount_pct: None,
            delay_ms: None,
            done: true,
        },
        42..=49 => Event::Speakers {
            on: Some(rng.chance(50)),
            set: rng.chance(50).then(|| rng.below(2) as usize),
        },
        50..=57 => Event::Toggle(rng.chance(50)),
        58..=65 => Event::Bypass(rng.chance(55)),
        66..=71 => Event::Loudness {
            on: Some(rng.chance(50)),
            strength_pct: None,
            reference_pct: None,
            reference_now: false,
            done: true,
        },
        72..=77 => Event::Rate(rng.pick(&HIGH_RATES)),
        78..=80 => Event::Rate(rng.pick(&RATES)),
        81..=83 => Event::SlotRevert,
        84..=86 => Event::NodeRestart,
        87..=89 => Event::CliDeath {
            after_line: match rng.below(3) {
                0 => None,
                1 => Some(true),
                _ => Some(false),
            },
        },
        _ => {
            let roll = rng.below(100);
            event_for(&mut rng, roll)
        }
    }));
    events
}

/// The event a `roll` of 0 to 99 stands for, with its fields drawn from `rng`.
fn event_for(rng: &mut Rng, roll: u64) -> Event {
    match roll {
        0..=13 => Event::Ticks(1 + rng.below(45) as u32),
        14..=15 => Event::Playing(rng.chance(50)),
        16..=19 => {
            let model = rng.chance(25).then(|| rng.below(4) as usize);
            let amount_pct = rng.chance(50).then(|| rng.range(5.0, 50.0));
            let delay_ms = rng.chance(30).then(|| rng.range(0.0, 2.0));
            let on = rng.chance(60).then(|| rng.chance(65));
            // A change that names nothing is refused: every one names something.
            let on = on.or(
                (model.is_none() && amount_pct.is_none() && delay_ms.is_none()).then_some(true),
            );
            Event::Crossfeed {
                on,
                model,
                amount_pct,
                delay_ms,
                done: rng.chance(70),
            }
        }
        20..=37 => Event::Drag {
            pick: rng.below(16) as usize,
            kind: rng.chance(15).then(|| filter(rng, true).kind),
            f: rng.chance(30).then(|| rng.log_range(20.0, 20_000.0)),
            g: rng.chance(85).then(|| rng.range(-12.0, 12.0)),
            q: rng.chance(20).then(|| rng.log_range(0.1, 10.0)),
            steps: rng.below(5) as u8,
            gap_ticks: rng.below(3) as u8,
        },
        38..=45 => {
            let (filters, stated) = curve(rng);
            Event::Load { filters, stated }
        }
        46..=52 => Event::Toggle(rng.chance(50)),
        53..=58 => {
            let (filters, stated) = curve(rng);
            Event::Cli {
                filters,
                stated,
                gap_ticks: rng.below(9) as u8,
            }
        }
        59..=64 => Event::NodeRestart,
        65..=73 => Event::Rate(rng.pick(&RATES)),
        74..=77 => {
            let hand = rng.chance(70);
            Event::Add(filter(rng, hand))
        }
        78..=81 => Event::Remove {
            pick: rng.below(16) as usize,
        },
        82..=84 => Event::CliDeath {
            after_line: match rng.below(3) {
                0 => None,
                1 => Some(true),
                _ => Some(false),
            },
        },
        85..=86 => Event::RefuseSends {
            ms: rng.pick(&[200, 1_500, 6_000]),
        },
        87..=89 => Event::Fail {
            kind: rng.below(4) as u8,
        },
        90..=91 => Event::Reload,
        92..=93 => Event::ListDelay {
            ms: rng.pick(&[0, 0, 300, 1_200]),
        },
        94..=95 => Event::FailNextWrite,
        96 => Event::RefusalLag(rng.below(3) as u8),
        97 => Event::SlotRevert,
        _ => Event::DaemonRestart,
    }
}

// ---------------------------------------------------------------------------------------------
// The driver

/// A scratch directory, taken away with the run.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        static SWEPT: std::sync::Once = std::sync::Once::new();
        let base = crate::scratch::scratch_base().expect("the scratch base");
        SWEPT.call_once(|| sweep_dead_runs(&base));
        let path = base.join(format!(
            "lab-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Dir(path)
    }
}

/// Removes the `lab-<pid>-<n>` directories of `base` whose run is gone (a killed run cannot take
/// its own away). Only real directories this user owns; a link is never followed.
fn sweep_dead_runs(base: &Path) {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let Some(uid) = std::fs::metadata("/proc/self").ok().map(|m| m.uid()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix("lab-"))
            .and_then(|rest| rest.split('-').next())
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        let ours =
            std::fs::symlink_metadata(entry.path()).is_ok_and(|m| m.is_dir() && m.uid() == uid);
        if ours && !Path::new("/proc").join(pid.to_string()).exists() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The peaks worked out so far, by coefficients and rate: a state of the world is checked once.
#[derive(Default)]
struct Peaks(HashMap<Vec<u64>, f64>);

impl Peaks {
    fn of(&mut self, sets: &[[f64; 5]], rate_hz: u32) -> f64 {
        let key: Vec<u64> = std::iter::once(u64::from(rate_hz))
            .chain(sets.iter().flatten().map(|c| c.to_bits()))
            .collect();
        if self.0.len() > 20_000 {
            self.0.clear();
        }
        *self
            .0
            .entry(key)
            .or_insert_with(|| dense_peak(sets, rate_hz))
    }
}

/// The stage gains worked out so far, by port values and rate. They are kept on the thread, for
/// every sequence it runs: a stage (a model's own settings at a rate) comes back from one seed
/// to the next, and one answer is a scan of every hertz to Nyquist.
#[derive(Default)]
struct StageGains;

impl StageGains {
    fn of(&mut self, values: &[f64], rate_hz: u32) -> f64 {
        thread_local! {
            static KEPT: std::cell::RefCell<HashMap<Vec<u64>, f64>> =
                std::cell::RefCell::new(HashMap::new());
        }
        let key: Vec<u64> = std::iter::once(u64::from(rate_hz))
            .chain(values.iter().map(|v| v.to_bits()))
            .collect();
        KEPT.with_borrow_mut(|kept| {
            if kept.len() > 2_000 {
                kept.clear();
            }
            *kept
                .entry(key)
                .or_insert_with(|| stage_gain_db(values, rate_hz))
        })
    }
}

/// The lab's sets in a store at `path`: each path of a set one impulse of [`SET_PATH_GAIN`], at
/// a place of its own in each set.
fn put_sets(path: &std::path::Path) {
    let store = svolder_speakers::Store::new(path);
    for (at, id) in SETS.iter().enumerate() {
        let mut channels = vec![vec![0.0f32; 64]; 4];
        for (n, channel) in channels.iter_mut().enumerate() {
            channel[at * 8 + n] = SET_PATH_GAIN as f32;
        }
        let wav = svolder_speakers::wav::write(48_000, &channels);
        let meta = svolder_speakers::SetMeta {
            id: (*id).to_string(),
            room: svolder_speakers::Room::No,
            label: format!("Lab set {at}"),
            licence: "CC-BY-4.0".to_string(),
            credit: "The lab".to_string(),
            source_url: "https://example.invalid/set".to_string(),
            source_sha256: "0".repeat(64),
            wav_sha256: String::new(),
            rate: 48_000,
            taps: 64,
            taps_ms: 64.0 / 48.0,
            angles: None,
            loudness_db: -6.9,
            scale_db: -3.0,
            worst_gain_db: -0.1,
            cut: false,
            own: false,
            added: 1,
            used: 1,
        };
        store.save(meta, &wav, None).expect("a lab set is kept");
    }
}

/// The most the virtual speakers' stage the node holds adds on `channel`, in dB: the dry input
/// and the two paths into the ear, each at its mixer gain, added in phase (0 dB with no stage).
fn speakers_db(node: &Node, channel: char) -> f64 {
    let gain = |n: u8| {
        node.ports
            .get(&format!("{channel}vs:Gain {n}"))
            .map(|value| f64::from(*value).abs())
    };
    match (gain(1), gain(2), gain(3)) {
        (Some(dry), Some(left), Some(right)) => {
            20.0 * (dry + (left + right) * SET_PATH_GAIN).max(1e-12).log10()
        }
        _ => 0.0,
    }
}

/// A run of one sequence.
struct Lab {
    shared: Shared,
    control: EqControl,
    peaks: Peaks,
    stages: StageGains,
    /// The daemon's own count of ticks, which says when it reads the state file.
    daemon_ticks: u64,
    _dir: Dir,
    /// The event being run, for the message of a failure.
    at: usize,
    /// When the last event that was not time going by ended, on the world's clock.
    quiet_since_ms: u64,
    /// The liveness rule has been checked since then.
    live_checked: bool,
    /// The loudness's liveness rule has been checked since then.
    ld_checked: bool,
    /// The part the last event recalled, while nothing else has happened since but time.
    scene: Option<ComputerPart>,
    /// The scene's liveness rule has been checked since then.
    scene_checked: bool,
    /// The climb in progress: when it was first seen with this target (the world's clock), the
    /// target preamp, and how long it has to reach it.
    climb_seen: Option<(u64, f64, u64, f64)>,
    /// The event being run may raise the level at once ([`exempts`]): its own ticks do not end
    /// the window.
    exempt_event: bool,
}

/// Why a sequence failed.
#[derive(Debug, Clone)]
pub struct Failure {
    /// The index of the event that was running.
    pub event: usize,
    /// What was broken.
    pub what: String,
}

fn new_control(shared: &Shared) -> EqControl {
    let (path, sets) = {
        let world = world(shared);
        (world.state_path(), world.dir.join("speakers"))
    };
    let mut control = EqControl::new(
        Arc::new(ModelChain {
            world: shared.clone(),
        }),
        Box::new(ModelLink::params(shared.clone())),
        Keep::File(path),
    )
    .with_speakers_dir(&sets);
    control.set_idle_ticks(IDLE_TICKS);
    control
}

impl Lab {
    /// A world with no chain, the sink at `rate`, and a controller over it.
    fn new(rate: u32) -> Lab {
        let dir = Dir::new();
        put_sets(&dir.0.join("speakers"));
        let shared: Shared = Arc::new(Mutex::new(World {
            dir: dir.0.clone(),
            clock_ms: 10_000,
            sink_rate: rate,
            conf: None,
            node: None,
            next_id: 30,
            freed_id: None,
            next_serial: 0,
            reuse_ids: IDS.get() == Ids::Reused,
            list_delay_ms: 0,
            node_is_ours: false,
            cli: Cli::default(),
            fail_apply: 0,
            fail_remove: 0,
            fail_keep: 0,
            fail_start: 0,
            refuse_sends_until_ms: 0,
            fail_next_write: false,
            refusal_lag: 0,
            mtime: 0,
            writes: 0,
            sends: 0,
            broken: Vec::new(),
            repair_owed: false,
            check_repairs: std::env::var_os(REPAIR_LINES_ENV).is_some(),
            looked_rate: rate,
            lost_line: false,
            unreconciled: false,
            trace: None,
            playing: false,
            volume: 1.0,
            muted: false,
            fail_key_reads: 0,
            level_last: None,
            rise_ok_until_ms: 0,
            first_line_of: None,
        }));
        let control = new_control(&shared);
        Lab {
            shared,
            control,
            peaks: Peaks::default(),
            stages: StageGains,
            daemon_ticks: 0,
            _dir: dir,
            at: 0,
            quiet_since_ms: 10_000,
            live_checked: false,
            ld_checked: false,
            scene: None,
            scene_checked: false,
            climb_seen: None,
            exempt_event: false,
        }
    }

    /// The rule: what the node plays is never over unity.
    fn check(&mut self) -> Result<(), Failure> {
        let (broken, played) = {
            let mut world = world(&self.shared);
            let exempt = world.clock_ms < world.rise_ok_until_ms;
            let jump = match world.node.take() {
                Some(node) => {
                    let jump = level_step(
                        &mut world.level_last,
                        &node,
                        exempt,
                        "after an event or a tick",
                    );
                    world.node = Some(node);
                    jump
                }
                None => {
                    world.level_last = None;
                    None
                }
            };
            world.broken.extend(jump);
            let broken = world.broken.pop();
            let played = world.node.as_ref().map(|node| {
                (
                    ['l', 'r'].map(|channel| played(node, channel)),
                    world.sink_rate,
                    node.id,
                    ['l', 'r'].map(|channel| stage_values(node, channel)),
                    ['l', 'r'].map(|channel| speakers_db(node, channel)),
                    ['l', 'r'].map(|channel| loudness_sets(node, channel)),
                )
            });
            (broken, played)
        };
        let fail = |what: String| {
            Err(Failure {
                event: self.at,
                what,
            })
        };
        if let Some(broken) = broken {
            return fail(broken);
        }
        let Some((channels, rate, id, stages, speakers, loudness)) = played else {
            return Ok(());
        };
        for ((((channel, (sets, pre_db)), stage), speakers), loudness) in ['l', 'r']
            .into_iter()
            .zip(&channels)
            .zip(&stages)
            .zip(speakers)
            .zip(&loudness)
        {
            // The loudness stage alone is cut-only, whatever the preamp.
            let cut = self.peaks.of(loudness, rate);
            if cut > TOLERANCE_DB {
                return fail(format!(
                    "node {id} plays the loudness stage alone {cut:+.3} dB over unity at {rate} Hz on {channel}: {loudness:?}"
                ));
            }
            if let Some(slot) = sets.iter().position(|set| !stable(set)) {
                return fail(format!(
                    "node {id} plays an unstable biquad in {channel}{:02}: {:?}",
                    slot + 1,
                    sets[slot]
                ));
            }
            let peak = self.peaks.of(sets, rate);
            let stage = self.stages.of(stage, rate);
            if peak + stage + speakers + pre_db > TOLERANCE_DB {
                return fail(format!(
                    "node {id} plays {:+.3} dB over unity at {rate} Hz on {channel}: the slots peak {peak:+.3} dB, the crossfeed stage adds up to {stage:+.3} dB and the speakers {speakers:+.3} dB under a preamp of {pre_db:+.3} dB",
                    peak + stage + speakers + pre_db
                ));
            }
        }
        Ok(())
    }

    async fn ticks(&mut self, count: u32) -> Result<(), Failure> {
        for _ in 0..count {
            world(&self.shared).clock_ms += 50;
            self.control.tick().await;
            self.climb_liveness()?;
            if !self.exempt_event {
                self.end_rise_window();
            }
            self.daemon_ticks += 1;
            if self.daemon_ticks.is_multiple_of(20) {
                let loads = self.control.loads();
                self.reload()?;
                if self.control.loads() != loads {
                    // What another process wrote is read as an apply is: its lines may raise.
                    let mut world = world(&self.shared);
                    world.rise_ok_until_ms = world
                        .rise_ok_until_ms
                        .max(world.clock_ms + 4 * RISE_GRACE_MS);
                }
            }
            self.check()?;
        }
        Ok(())
    }

    /// A rise in the level is allowed through an event that may make one and for as long as the
    /// controller still owes the chain a line for it (a node not listed yet, a `pw-cli` that is
    /// being started again), and a moment after.
    fn end_rise_window(&mut self) {
        let pending = self.control.lines_pending();
        let mut world = world(&self.shared);
        if world.rise_ok_until_ms == u64::MAX && !pending {
            world.rise_ok_until_ms = world.clock_ms + RISE_GRACE_MS;
        }
    }

    /// The climb's liveness rule: a climb reaches its target within (the rise over 0.32 dB) ticks
    /// and two seconds of being first seen with that target, and the second a dead `pw-cli` waits
    /// before it is started again.
    fn climb_liveness(&mut self) -> Result<(), Failure> {
        let (now, disturbed) = {
            let world = world(&self.shared);
            (world.clock_ms, world.disturbed())
        };
        match self.control.climb_progress() {
            // A fault armed or under way (a refused send, a `pw-cli` being started again) holds
            // the climb up: the budget starts again when it is over, as every liveness rule's does.
            Some(_) if disturbed => self.climb_seen = None,
            Some((level, to)) => match self.climb_seen {
                // A level under the one last seen is a climb that began again when an event other
                // than time going by ran since the budget started (a toggle that cut the preamp
                // anew): its rise is counted from its own start. A level that only falls with
                // ticks is a climb that is being cut and restarted, and keeps its budget.
                Some((since, target, budget, last))
                    if (target - to).abs() < 1e-6
                        && (level >= last - 1e-9 || self.quiet_since_ms <= since) =>
                {
                    self.climb_seen = Some((since, target, budget, level));
                    if now - since > budget {
                        return Err(Failure {
                            event: self.at,
                            what: format!(
                                "a climb to {to:+.2} dB is still at {level:+.2} dB after {} ms",
                                now - since
                            ),
                        });
                    }
                }
                _ => {
                    let ticks = ((to - level).max(0.0) / super::LEVEL_CLIMB_DB_PER_TICK).ceil();
                    self.climb_seen =
                        Some((now, to, ticks as u64 * 50 + 2_000 + RESTART_GAP_MS, level));
                }
            },
            None => self.climb_seen = None,
        }
        Ok(())
    }

    /// Run `event`, and the liveness rules once the world has been quiet long enough.
    async fn run(&mut self, event: &Event) -> Result<(), Failure> {
        // Anything but time going by may change what a recall set.
        // The daemon reading the state file changes nothing a recall set.
        if !matches!(event, Event::Ticks(_) | Event::Reload) {
            self.scene = None;
        }
        self.exempt_event = exempts(event);
        if self.exempt_event {
            world(&self.shared).rise_ok_until_ms = u64::MAX;
        }
        let ran = self.run_event(event).await;
        self.exempt_event = false;
        self.end_rise_window();
        ran?;
        match event {
            Event::Ticks(_) => {
                // Before the liveness rule, which turns on an EQ that is off with a playable
                // curve, as the part may have left it.
                self.scene_liveness()?;
                self.liveness().await?;
                self.ld_liveness()
            }
            _ => {
                self.quiet_since_ms = world(&self.shared).clock_ms;
                self.live_checked = false;
                self.ld_checked = false;
                self.scene_checked = false;
                Ok(())
            }
        }
    }

    /// The scene's liveness rule, once after [`QUIET_MS`] with no event but time and nothing
    /// armed to fail: a part a recall took is what the controller shows, and, when it says it is
    /// live, the node plays its crossfeed stage as the part says (the curve, the virtual speakers
    /// and the loudness are held to the view by the rules above).
    fn scene_liveness(&mut self) -> Result<(), Failure> {
        let (clock, disturbed, armed, mix) = {
            let world = world(&self.shared);
            (
                world.clock_ms,
                world.disturbed(),
                world.fail_apply + world.fail_remove + world.fail_keep + world.fail_start > 0,
                world
                    .node
                    .as_ref()
                    .and_then(|node| node.ports.get("lmix:Gain 2").map(|mix| f64::from(*mix))),
            )
        };
        let Some(part) = self.scene.clone() else {
            return Ok(());
        };
        if self.scene_checked || clock < self.quiet_since_ms + QUIET_MS || disturbed || armed {
            return Ok(());
        }
        self.scene_checked = true;
        // A curve the sink's rate cannot play is turned off by the look at that rate, as a preset
        // load's is ("an EQ turned off by a rate it cannot play"): the recall judged it at the rate
        // the controller last looked at, so the part is not held to the rule there.
        if part.eq_on {
            let rate = world(&self.shared).sink_rate;
            let curve = Correction {
                preamp_db: part.preamp_db,
                filters: part.filters.clone(),
            };
            if check_playable(&curve, rate).is_err() {
                return Ok(());
            }
        }
        SCENE_ASKED.with(|asked| asked.set(asked.get() + 1));
        let view = self.control.view();
        let fail = |what: String| {
            Err(Failure {
                event: self.at,
                what,
            })
        };
        if !part.matches(&view) {
            return fail(format!(
                "the recalled part is not what the controller shows: {} (the part says the EQ {}, crossfeed {}, speakers {}, loudness {:?}; the view the EQ {}, crossfeed {}, speakers {}, loudness {})",
                describe(&Event::Scene(Box::new(part.clone()))),
                if part.eq_on { "on" } else { "off" },
                if part.crossfeed_on { "on" } else { "off" },
                if part.speakers_on { "on" } else { "off" },
                part.loudness.map(|ld| ld.on),
                if view.on { "on" } else { "off" },
                if view.crossfeed.as_ref().is_some_and(|xf| xf.on) {
                    "on"
                } else {
                    "off"
                },
                if view.speakers.as_ref().is_some_and(|sp| sp.on) {
                    "on"
                } else {
                    "off"
                },
                view.loudness.as_ref().map(|ld| ld.on).unwrap_or(false),
            ));
        }
        if let (true, Some(mix)) = (view.live, mix)
            && (mix > 0.0) != part.crossfeed_on
        {
            return fail(format!(
                "the recalled part has crossfeed {}, and the node's stage mix is {mix}",
                if part.crossfeed_on { "on" } else { "off" }
            ));
        }
        Ok(())
    }

    /// Whether the loudness has anything to follow or to undo: it is on, or the node holds a stage
    /// other than the identity.
    fn ld_in_play(&self) -> bool {
        let identity = loudness::Stage::identity().sections;
        let held = world(&self.shared).node.as_ref().is_some_and(|node| {
            ['l', 'r']
                .into_iter()
                .any(|channel| loudness_sets(node, channel) != identity)
        });
        held || self.control.view().loudness.is_some_and(|ld| ld.on)
    }

    /// The loudness's liveness rule, once after [`LD_QUIET_MS`] with no event and nothing armed
    /// to fail: when the controller says it is live, the node's stage is the one the settings
    /// make of the world's volume (within [`LIVE_TOLERANCE_DB`] on the grid of [`LIVE_GRID`]
    /// frequencies), the identity while loudness is off. Muted, it holds where it was.
    fn ld_liveness(&mut self) -> Result<(), Failure> {
        let (clock, disturbed, muted, volume, rate, node) = {
            let world = world(&self.shared);
            (
                world.clock_ms,
                world.disturbed(),
                world.muted,
                world.volume,
                world.sink_rate,
                world
                    .node
                    .as_ref()
                    .map(|node| ['l', 'r'].map(|channel| loudness_sets(node, channel))),
            )
        };
        if self.ld_checked || clock < self.quiet_since_ms + LD_QUIET_MS || disturbed {
            return Ok(());
        }
        self.ld_checked = true;
        let view = self.control.view();
        let (Some(ld), Some(node), true) = (view.loudness, node, view.live) else {
            return Ok(());
        };
        if muted && ld.on {
            return Ok(());
        }
        let settings = loudness::LoudnessSettings {
            on: ld.on,
            strength_pct: ld.strength_pct,
            reference_pct: ld.reference_pct,
        };
        let want = match ld.on {
            true => loudness::quantise_db(loudness::depth_db(volume, &settings)),
            false => 0.0,
        };
        let stage = loudness::stage(want, rate);
        let top = (f64::from(rate) * 0.45).min(20_000.0);
        for (channel, sets) in ['l', 'r'].into_iter().zip(&node) {
            for step in 0..=LIVE_GRID {
                let f = 20.0 * (top / 20.0).powf(f64::from(step) / f64::from(LIVE_GRID));
                let w = 2.0 * std::f64::consts::PI * f / f64::from(rate);
                let plays = summed_db(sets, w);
                let wanted = stage.response_db(f, rate);
                if (plays - wanted).abs() > LIVE_TOLERANCE_DB {
                    return Err(Failure {
                        event: self.at,
                        what: format!(
                            "the loudness says it follows the volume ({}, {:.1} dB of depth for a volume of {:.1} %), and the node's stage plays {plays:+.3} dB at {f:.0} Hz on {channel} where {want:.1} dB of depth makes {wanted:+.3} dB, at {rate} Hz",
                            if ld.on { "on" } else { "off" },
                            ld.depth_db,
                            volume.max(0.0).cbrt() * 100.0
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// The end of every sequence, which no event stands for: the faults that were armed and not
    /// used are disarmed, the daemon reads the state file, and the world is left alone for
    /// [`TAIL_TICKS`] ticks, so the liveness rule is asked of what the sequence ended on.
    async fn quiet_tail(&mut self, at: usize) -> Result<(), Failure> {
        self.at = at;
        {
            let mut world = world(&self.shared);
            world.note(|| "the quiet tail".to_string());
            world.fail_apply = 0;
            world.fail_remove = 0;
            world.fail_keep = 0;
            world.fail_start = 0;
            world.fail_next_write = false;
            world.refuse_sends_until_ms = 0;
            world.cli.dies_after_next_line = None;
            world.refusal_lag = 0;
            world.fail_key_reads = 0;
        }
        self.run(&Event::Reload).await?;
        for _ in 0..TAIL_TICKS / 45 {
            self.run(&Event::Ticks(45)).await?;
        }
        // The scene rule is asked of a recall the sequence ended on.
        for _ in 0..4 {
            if self.scene.is_none() || self.scene_checked {
                break;
            }
            self.run(&Event::Ticks(45)).await?;
        }
        // The loudness's rule waits longer (a look and a whole glide): asked of a sequence that
        // left it something to follow or undo.
        for _ in 0..2 {
            if self.ld_checked || !self.ld_in_play() {
                break;
            }
            self.run(&Event::Ticks(45)).await?;
        }
        Ok(())
    }

    /// The daemon reads the state file: a read writes and sends nothing (what it finds is
    /// taken up at a tick).
    fn reload(&mut self) -> Result<(), Failure> {
        let before = world(&self.shared).footprint();
        self.control.reload_if_changed();
        if world(&self.shared).footprint() != before {
            return Err(Failure {
                event: self.at,
                what: "a reload of the state file wrote or sent something".to_string(),
            });
        }
        Ok(())
    }

    /// The liveness rule, once after [`QUIET_MS`] with no event and nothing armed to fail: the
    /// controller must not have stopped serving. When it says it is on and live, the node plays
    /// the points it shows, at the rate the sink runs at, under the preamp it shows (within
    /// [`LIVE_TOLERANCE_DB`] at every frequency of a grid). When it says it is live, a node with
    /// the virtual speakers' stage plays them on or off as the view says. When it is off and its
    /// curve is playable at the sink's rate, turning it on works.
    async fn liveness(&mut self) -> Result<(), Failure> {
        let (clock, disturbed) = {
            let world = world(&self.shared);
            (world.clock_ms, world.disturbed())
        };
        if self.live_checked || clock < self.quiet_since_ms + QUIET_MS || disturbed {
            return Ok(());
        }
        self.live_checked = true;
        let view = self.control.view();
        let (rate, node) = {
            let world = world(&self.shared);
            (
                world.sink_rate,
                world
                    .node
                    .as_ref()
                    .map(|node| ['l', 'r'].map(|c| played(node, c))),
            )
        };
        let filters: Vec<SourceFilter> = view
            .points
            .iter()
            .map(|point| SourceFilter {
                kind: point.kind.parse().expect("a point's kind is a keyword"),
                f: point.f,
                g: point.g,
                q: point.q,
            })
            .collect();
        let fail = |what: String| {
            Err(Failure {
                event: self.at,
                what,
            })
        };
        // The virtual speakers play as the view says, whenever it says it is live.
        let wet = world(&self.shared).node.as_ref().and_then(|node| {
            node.ports
                .get("lvs:Gain 2")
                .map(|gain| f64::from(*gain) > 0.0)
        });
        let shown = view.speakers.as_ref().is_some_and(|speakers| speakers.on);
        if view.live
            && let Some(wet) = wet
            && wet != shown
        {
            return fail(format!(
                "the EQ says it is live with the virtual speakers {}, and the node plays them {}",
                if shown { "on" } else { "off" },
                if wet { "on" } else { "off" }
            ));
        }
        match (view.on && view.live, node) {
            (true, Some(channels)) => {
                let (sets, _) = super::live::played_sets(
                    &Correction {
                        preamp_db: 0.0,
                        filters,
                    },
                    rate,
                );
                let top = (f64::from(rate) * 0.45).min(20_000.0);
                for (channel, (node_sets, pre_db)) in ['l', 'r'].into_iter().zip(&channels) {
                    for step in 0..=LIVE_GRID {
                        let f = 20.0 * (top / 20.0).powf(f64::from(step) / f64::from(LIVE_GRID));
                        let w = 2.0 * std::f64::consts::PI * f / f64::from(rate);
                        let plays = summed_db(&node_sets[..SLOTS], w) + pre_db;
                        let shown = summed_db(&sets, w) + view.preamp_db;
                        if (plays - shown).abs() > LIVE_TOLERANCE_DB {
                            return fail(format!(
                                "the EQ says it is live, and the node plays {plays:+.3} dB at {f:.0} Hz on {channel} where the view's points under its preamp of {:+.3} dB make {shown:+.3} dB, at {rate} Hz",
                                view.preamp_db
                            ));
                        }
                    }
                }
            }
            (false, None) if !view.on && !filters.is_empty() => {
                let curve = Correction {
                    preamp_db: 0.0,
                    filters,
                };
                // With a fault armed the call may be refused for it: the rule waits.
                let armed = {
                    let world = world(&self.shared);
                    world.fail_apply + world.fail_remove + world.fail_keep + world.fail_start > 0
                };
                if !armed && check_playable(&curve, rate).is_ok() {
                    let turned_on = self.control.set_on(true).await;
                    self.check()?;
                    if let Err(failure) = turned_on {
                        return Err(Failure {
                            event: self.at,
                            what: format!(
                                "the EQ is off with a curve that plays at {rate} Hz, and turning it on is refused: {failure}"
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The daemon restarts: a new controller over the same files and the same chain; its start
    /// writes and sends nothing.
    fn daemon_restart(&mut self) -> Result<(), Failure> {
        let before = {
            let mut world = world(&self.shared);
            // The old daemon's `pw-cli` ends with it.
            world.cli = Cli::default();
            world.footprint()
        };
        self.control = new_control(&self.shared);
        self.daemon_ticks = 0;
        {
            let mut world = world(&self.shared);
            if world.node.is_some() {
                world.unreconciled = true;
            }
        }
        if world(&self.shared).footprint() != before {
            return Err(Failure {
                event: self.at,
                what: "the daemon wrote or sent something while it started".to_string(),
            });
        }
        Ok(())
    }

    /// The computer's volume goes to `pct` percent (held to 0-100) and the mute to `muted`; a
    /// write through the daemon hands the controller what it read back, as the daemon does.
    fn volume_to(&mut self, pct: f64, muted: bool, daemon: bool) {
        let pct = pct.clamp(0.0, 100.0);
        {
            let mut world = world(&self.shared);
            world.volume = (pct / 100.0).powi(3);
            world.muted = muted;
            world.note(|| {
                format!("the volume is {pct:.1} %, muted: {muted}, through the daemon: {daemon}")
            });
        }
        if daemon {
            self.control.level_hint().volume(pct / 100.0, muted);
        }
    }

    async fn run_event(&mut self, event: &Event) -> Result<(), Failure> {
        match event {
            Event::Ticks(count) => self.ticks(*count).await?,
            Event::Drag {
                pick,
                kind,
                f,
                g,
                q,
                steps,
                gap_ticks,
            } => {
                let points = self.control.view().points;
                if !points.is_empty() {
                    let at = pick % points.len();
                    let from = &points[at];
                    for step in 1..=u32::from(*steps) + 1 {
                        let done = step > u32::from(*steps);
                        let blend = |from: f64, to: Option<f64>| {
                            to.map(|to| from + (to - from) * f64::from(step) / f64::from(steps + 1))
                        };
                        let edit = Edit::Set {
                            index: at,
                            slot: Some(from.slot).filter(|slot| *slot > 0),
                            kind: kind.filter(|_| done),
                            f: blend(from.f, *f),
                            g: blend(from.g, *g),
                            q: blend(from.q, *q),
                            done,
                        };
                        let _ = self.control.edit(edit).await;
                        self.check()?;
                        if !done {
                            self.ticks(u32::from(*gap_ticks)).await?;
                        }
                    }
                }
            }
            Event::Add(filter) => {
                let _ = self.control.edit(Edit::Add(filter.clone())).await;
            }
            Event::Remove { pick } => {
                let count = self.control.view().points.len();
                if count > 0 {
                    let _ = self
                        .control
                        .edit(Edit::Remove {
                            index: pick % count,
                            slot: None,
                        })
                        .await;
                }
            }
            Event::Load { filters, stated } => {
                let correction = Correction {
                    preamp_db: *stated,
                    filters: filters.clone(),
                };
                if apo::validate(&correction).is_ok() {
                    let _ = self
                        .control
                        .load("Preset".to_string(), None, correction)
                        .await;
                }
            }
            Event::Toggle(on) => {
                let _ = self.control.set_on(*on).await;
            }
            Event::Crossfeed {
                on,
                model,
                amount_pct,
                delay_ms,
                done,
            } => {
                let _ = self
                    .control
                    .crossfeed(CrossfeedEdit {
                        on: *on,
                        model: model.map(|at| CrossfeedModel::ALL[at % 4].word().to_string()),
                        preset: None,
                        amount_pct: *amount_pct,
                        crossover_hz: None,
                        delay_ms: *delay_ms,
                        done: *done,
                    })
                    .await;
            }
            Event::Cli {
                filters,
                stated,
                gap_ticks,
            } => {
                let request = ApplyRequest {
                    name: "Cli".to_string(),
                    origin: None,
                    correction: Correction {
                        preamp_db: *stated,
                        filters: filters.clone(),
                    },
                    sink: None,
                    slots: Vec::new(),
                    rate_hz: None,
                    flat: false,
                    crossfeed: None,
                    speakers: None,
                    identity: None,
                };
                // The daemon is not blocked by the command line: it ticks in the wait.
                let staged = World::stage_apply(request);
                if let Ok((mut state, conf)) = staged {
                    {
                        let mut world = world(&self.shared);
                        // The command line keeps the loudness's settings the state held when it
                        // started, as the real host's apply does.
                        state.loudness = world.state().and_then(|previous| previous.loudness);
                        world.conf = Some(conf);
                        world.start_node(false);
                    }
                    self.check()?;
                    self.ticks(u32::from(*gap_ticks)).await?;
                    world(&self.shared).write_state_as_command_line(&state);
                }
            }
            Event::Reload => self.reload()?,
            Event::NodeRestart => {
                let mut world = world(&self.shared);
                if world.node.is_some() {
                    world.start_node(false);
                }
            }
            Event::Rate(rate) => {
                let mut world = world(&self.shared);
                // A rate the controller last looked at again (a rebuild at the rate the sink
                // runs at, or a change that is undone before its next look) changes nothing it
                // can see.
                if world.looked_rate == *rate {
                    world.unreconciled = true;
                }
                world.sink_rate = *rate;
                if let Some(node) = world.node.as_mut() {
                    node.load_slots();
                }
            }
            Event::CliDeath { after_line } => {
                let mut world = world(&self.shared);
                match after_line {
                    None => {
                        world.repair_owed = true;
                        world.cli.end();
                        world.cli.refused = false;
                    }
                    Some(lands) => world.cli.dies_after_next_line = Some(*lands),
                }
            }
            Event::RefuseSends { ms } => {
                let mut world = world(&self.shared);
                world.refuse_sends_until_ms = world.clock_ms + u64::from(*ms);
            }
            Event::Fail { kind } => {
                let mut world = world(&self.shared);
                match kind {
                    0 => world.fail_apply += 1,
                    1 => world.fail_remove += 1,
                    2 => world.fail_keep += 1,
                    _ => world.fail_start += 1,
                }
            }
            Event::ListDelay { ms } => world(&self.shared).list_delay_ms = u64::from(*ms),
            Event::FailNextWrite => world(&self.shared).fail_next_write = true,
            Event::RefusalLag(lag) => world(&self.shared).refusal_lag = *lag,
            Event::Playing(playing) => world(&self.shared).playing = *playing,
            Event::Speakers { on, set } => {
                // A change that names nothing is no change.
                if on.is_some() || set.is_some() {
                    let _ = self
                        .control
                        .speakers(super::control::SpeakersEdit {
                            on: *on,
                            set: set.map(|at| SETS[at % SETS.len()].to_string()),
                            done: true,
                        })
                        .await;
                }
            }
            Event::SlotRevert => {
                if let Some(node) = world(&self.shared).node.as_mut() {
                    node.load_slots();
                }
            }
            Event::Volume { pct, daemon } => {
                let muted = world(&self.shared).muted;
                self.volume_to(*pct, muted, *daemon);
            }
            Event::VolumeStep { delta, daemon } => {
                let (now, muted) = {
                    let world = world(&self.shared);
                    (world.volume.max(0.0).cbrt() * 100.0, world.muted)
                };
                self.volume_to(now + delta, muted, *daemon);
            }
            Event::Mute { muted, daemon } => {
                let now = world(&self.shared).volume.max(0.0).cbrt() * 100.0;
                self.volume_to(now, *muted, *daemon);
            }
            Event::Loudness {
                on,
                strength_pct,
                reference_pct,
                reference_now,
                done,
            } => {
                let _ = self
                    .control
                    .loudness(LoudnessEdit {
                        on: *on,
                        strength_pct: *strength_pct,
                        reference_pct: *reference_pct,
                        reference_now: *reference_now,
                        done: *done,
                    })
                    .await;
            }
            Event::Scene(part) => {
                // A recall that says a stage it turned on was turned off again (a chain that
                // restarted under it) is not held to the rule: it said so.
                if let Ok(recalled) = self.control.recall((**part).clone()).await
                    && !recalled
                        .notes
                        .iter()
                        .any(|note| note.contains("turned off"))
                {
                    self.scene = Some((**part).clone());
                }
            }
            Event::Takeover { lost } => {
                self.daemon_restart()?;
                let mut world = world(&self.shared);
                match lost {
                    true => world.cli.dies_after_next_line = Some(false),
                    false => world.fail_key_reads = 1,
                }
            }
            Event::DaemonRestart => self.daemon_restart()?,
            Event::Bypass(on) => {
                let _ = self.control.bypass(*on).await;
            }
        }
        self.check()
    }
}

/// Run `events` on a fresh world whose sink starts at `rate`. `Ok` when nothing was broken; a
/// panic is a failure too.
pub fn run_sequence(rate: u32, events: &[Event]) -> Result<(), Failure> {
    run_traced(rate, events, None)
}

/// [`run_sequence`], writing what happens to `trace` when it is given.
pub fn run_traced(
    rate: u32,
    events: &[Event],
    trace: Option<&Mutex<Vec<String>>>,
) -> Result<(), Failure> {
    let events = events.to_vec();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let mut lab = Lab::new(rate);
            if trace.is_some() {
                world(&lab.shared).trace = Some(Vec::new());
            }
            let mut result = Ok(());
            for (at, event) in events.iter().enumerate() {
                lab.at = at;
                world(&lab.shared).note(|| format!("event {at}: {}", describe(event)));
                result = lab.run(event).await;
                if result.is_err() {
                    break;
                }
            }
            if result.is_ok() {
                result = lab.quiet_tail(events.len()).await;
            }
            if let Some(trace) = trace {
                let mut world = world(&lab.shared);
                world.note(|| format!("{:?}", result.as_ref().err()));
                *crate::locked(trace) = world.trace.take().unwrap_or_default();
            }
            result
        })
    }));
    match outcome {
        Ok(result) => result,
        Err(panic) => Err(Failure {
            event: events.len().saturating_sub(1),
            what: format!(
                "panic: {}",
                panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default()
            ),
        }),
    }
}

/// The sink rate a seed starts at.
pub fn start_rate(seed: u64) -> u32 {
    RATES[(seed.wrapping_mul(0x9E37_79B9) >> 7) as usize % RATES.len()]
}

/// The events of `seed`, as the lab runs them.
pub fn sequence_of(seed: u64) -> (u32, Vec<Event>) {
    let mut rng = Rng::new(seed ^ 0xA5A5_5A5A);
    let len = 20 + rng.below(50) as usize;
    (start_rate(seed), generate(seed, len))
}

/// The events of `seed` from the targeted profile ([`generate_targeted`]), as the lab runs them.
pub fn sequence_of_targeted(seed: u64) -> (u32, Vec<Event>) {
    let mut rng = Rng::new(seed ^ 0x5A5A_A5A5);
    let len = 20 + rng.below(50) as usize;
    (start_rate(seed), generate_targeted(seed, len))
}

/// The events of `seed` from the speakers profile ([`generate_speakers`]), as the lab runs them.
pub fn sequence_of_speakers(seed: u64) -> (u32, Vec<Event>) {
    let mut rng = Rng::new(seed ^ 0xA55A_5AA5);
    let len = 20 + rng.below(50) as usize;
    (start_rate(seed), generate_speakers(seed, len))
}

/// The events of `seed` from the high-rate profile ([`generate_highrate`]), on a sink that starts
/// at one of [`HIGH_RATES`].
pub fn sequence_of_highrate(seed: u64) -> (u32, Vec<Event>) {
    let mut rng = Rng::new(seed ^ 0x3C3C_C3C3);
    let len = 20 + rng.below(50) as usize;
    let rate = rng.pick(&HIGH_RATES);
    (rate, generate_highrate(seed, len))
}

/// The events of `seed` from the loudness profile ([`generate_loudness`]), as the lab runs them.
pub fn sequence_of_loudness(seed: u64) -> (u32, Vec<Event>) {
    let mut rng = Rng::new(seed ^ 0x6C6F_7564_5A5A);
    let len = 20 + rng.below(50) as usize;
    (start_rate(seed), generate_loudness(seed, len))
}

/// The events of `seed` from the scene profile ([`generate_scene`]), as the lab runs them.
pub fn sequence_of_scene(seed: u64) -> (u32, Vec<Event>) {
    let mut rng = Rng::new(seed ^ 0x5CE9_0A0A_F0F0);
    let len = 20 + rng.below(50) as usize;
    (start_rate(seed), generate_scene(seed, len))
}

/// The events of `seed` from the climb profile ([`generate_climb`]), as the lab runs them.
pub fn sequence_of_climb(seed: u64) -> (u32, Vec<Event>) {
    let mut rng = Rng::new(seed ^ 0xC11B_0A0A_F0F0_1111);
    let len = 20 + rng.below(50) as usize;
    (start_rate(seed), generate_climb(seed, len))
}

/// `events` with as many removed as still fail the same way: chunks first, then one by one.
pub fn shrink(rate: u32, events: &[Event]) -> Vec<Event> {
    let mut events = events.to_vec();
    let Err(first) = run_sequence(rate, &events) else {
        return events;
    };
    let kind = |failure: &Failure| {
        [
            "over unity",
            "unstable",
            "stale id",
            "while it started",
            "says it is live",
            "says it follows",
            "recalled part",
            "turning it on",
            "reload",
            "panic",
        ]
        .into_iter()
        .find(|word| failure.what.contains(word))
        .unwrap_or("")
        .to_string()
    };
    let wanted = kind(&first);
    // Everything after the failing event cannot matter.
    events.truncate(first.event + 1);
    let mut chunk = events.len().div_ceil(2);
    let mut budget = 300;
    while chunk >= 1 && budget > 0 {
        let mut at = 0;
        while at < events.len() && budget > 0 {
            let mut shorter = events.clone();
            shorter.drain(at..(at + chunk).min(events.len()));
            budget -= 1;
            match run_sequence(rate, &shorter) {
                Err(failure) if kind(&failure) == wanted => {
                    events = shorter;
                    let end = failure.event + 1;
                    events.truncate(end);
                }
                _ => at += chunk,
            }
        }
        chunk /= 2;
    }
    events
}

fn word(filter: &SourceFilter) -> String {
    let kind = match filter.kind {
        FilterKind::Peaking => "PK",
        FilterKind::LowShelf => "LSC",
        FilterKind::HighShelf => "HSC",
    };
    format!(
        "{kind} {:.1}Hz {:+.2}dB Q{:.2}",
        filter.f, filter.g, filter.q
    )
}

fn words(filters: &[SourceFilter]) -> String {
    filters.iter().map(word).collect::<Vec<_>>().join("; ")
}

/// `event` in a line.
fn describe(event: &Event) -> String {
    match event {
        Event::Load { filters, stated } => {
            format!("Load preamp {stated:+.1} [{}]", words(filters))
        }
        Event::Cli {
            filters,
            stated,
            gap_ticks,
        } => format!(
            "Cli (state written after {gap_ticks} ticks) preamp {stated:+.1} [{}]",
            words(filters)
        ),
        Event::Add(filter) => format!("Add {}", word(filter)),
        Event::Scene(part) => format!(
            "Scene eq {} preamp {:+.1} [{}] loudness {:?} crossfeed {} {:?} speakers {} {:?}",
            if part.eq_on { "on" } else { "off" },
            part.preamp_db,
            words(&part.filters),
            part.loudness,
            if part.crossfeed_on { "on" } else { "off" },
            part.crossfeed.as_ref().map(|settings| settings.model),
            if part.speakers_on { "on" } else { "off" },
            part.speakers,
        ),
        other => format!("{other:?}"),
    }
}

/// The report of a failing seed: what broke and the shrunk sequence that does it.
fn report(seed: u64, failure: &Failure, sequence: fn(u64) -> (u32, Vec<Event>)) -> String {
    let (rate, events) = sequence(seed);
    let shrunk = shrink(rate, &events);
    let again = run_sequence(rate, &shrunk).err();
    let place = match failure.event >= events.len() {
        true => format!("in the quiet tail after {} events", events.len()),
        false => format!("event {} of {}", failure.event + 1, events.len()),
    };
    let mut text = format!(
        "seed {seed}: {} ({place})\nshrunk to {} events, the sink starting at {rate} Hz",
        failure.what,
        shrunk.len()
    );
    if let Some(again) = again {
        text.push_str(&format!("\n  fails: {}", again.what));
    }
    for (at, event) in shrunk.iter().enumerate() {
        text.push_str(&format!("\n  {at:2}: {}", describe(event)));
    }
    text
}

/// Run seeds `first..first + runs` with the model of ids `ids`, on every core; the reports of the
/// ones that failed.
pub fn run_seeds(first: u64, runs: u64, ids: Ids) -> Vec<String> {
    run_block(first, runs, sequence_of, ids)
}

/// [`run_seeds`] on the speakers profile ([`generate_speakers`]).
pub fn run_speakers_seeds(first: u64, runs: u64, ids: Ids) -> Vec<String> {
    run_block(first, runs, sequence_of_speakers, ids)
}

/// [`run_seeds`] on the targeted profile ([`generate_targeted`]).
pub fn run_targeted_seeds(first: u64, runs: u64, ids: Ids) -> Vec<String> {
    run_block(first, runs, sequence_of_targeted, ids)
}

/// [`run_seeds`] on the high-rate profile ([`generate_highrate`]).
pub fn run_highrate_seeds(first: u64, runs: u64, ids: Ids) -> Vec<String> {
    run_block(first, runs, sequence_of_highrate, ids)
}

/// [`run_seeds`] on the loudness profile ([`generate_loudness`]).
pub fn run_loudness_seeds(first: u64, runs: u64, ids: Ids) -> Vec<String> {
    run_block(first, runs, sequence_of_loudness, ids)
}

/// [`run_seeds`] on the scene profile ([`generate_scene`]).
pub fn run_scene_seeds(first: u64, runs: u64, ids: Ids) -> Vec<String> {
    run_block(first, runs, sequence_of_scene, ids)
}

/// [`run_seeds`] on the climb profile ([`generate_climb`]).
pub fn run_climb_seeds(first: u64, runs: u64, ids: Ids) -> Vec<String> {
    run_block(first, runs, sequence_of_climb, ids)
}

fn run_block(
    first: u64,
    runs: u64,
    sequence: fn(u64) -> (u32, Vec<Event>),
    ids: Ids,
) -> Vec<String> {
    let next = AtomicUsize::new(0);
    let reported = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(16);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                IDS.set(ids);
                loop {
                    let at = next.fetch_add(1, Ordering::SeqCst) as u64;
                    if at >= runs {
                        return;
                    }
                    let seed = first + at;
                    let (rate, events) = sequence(seed);
                    if let Err(failure) = run_sequence(rate, &events) {
                        // The first few are shrunk and told in full; the rest by their seed.
                        let text = match reported.fetch_add(1, Ordering::SeqCst) < SHRUNK_REPORTS {
                            true => report(seed, &failure, sequence),
                            false => format!("seed {seed}: {}", failure.what),
                        };
                        let text = format!("[ids {ids:?}] {text}");
                        crate::locked(&failures).push((seed, text));
                    }
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap_or_default();
    failures.sort_by_key(|(seed, _)| *seed);
    failures.into_iter().map(|(_, text)| text).collect()
}

#[cfg(test)]
mod tests;
