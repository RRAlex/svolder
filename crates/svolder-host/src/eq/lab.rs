// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preamp lab: the computer EQ's controller driven through seeded random sequences of
//! events against a model of what PipeWire really plays.
//!
//! The model is a [`World`]: the config on disk, the running node (the config it was started
//! from, its id, the value of every port as the node holds it), the sink's rate, a `pw-cli`
//! that can die, the state file and a clock. [`ModelChain`] and [`ModelParams`] are the
//! controller's `Chain` and `Params` over it, and [`Lab`] drives an [`EqControl`] and the
//! outside world (the command line, PipeWire, the sink) through [`Event`]s. After every event
//! and every tick of 50 ms [`Lab::check`] works out the curve the node plays (the ports it
//! holds, as f32, clamped) times the preamp it plays, with an evaluator of its own, and it must
//! never be over unity.
//!
//! What the model takes from PipeWire, with where it was measured:
//!
//! - A node starts from the config on disk: every slot and the preamp take the config's values,
//!   and the node keeps that config to go back to. A restart of the chain is a new node (a new
//!   id), and so is an apply.
//! - A change of the sink's rate rebuilds the graph: every slot goes back to the config the node
//!   started from (not the file as it is now), and the preamp keeps the value it was last set
//!   to. The node keeps its id. This is the "slots revert to the applied config while the
//!   preamp keeps its live value" the host e2e's step E2 measured (+5.82 dB over unity on a
//!   private PipeWire before the cap), and spike notes' "a rate change re-instantiates the
//!   graph".
//! - A control value is an f32, and a `bq_raw` coefficient is held to plus or minus 10.
//! - A `set-param` to an id that is not the node's does nothing, and `pw-cli` says so (the
//!   refusal the session reads). A line is applied whole or not at all.
//! - `pw-cli` can die, and one that dies mid-line may or may not have applied the line.
//! - A new node may be listed late (a service manager returns when the unit is active, before
//!   the node is in the graph): until then it plays but cannot be found.
//! - The command line applies in the order the real one does: the config is written, the old
//!   node is stopped, the new one is started, 300 ms pass, then the state file is written.
//!
//! A sequence is a list of events and nothing else is random while it runs, so a failing seed
//! reproduces and a failing sequence shrinks.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use svolder_profiles::tone::apo::{self, Correction, FilterKind, SourceFilter};

use super::control::{Edit, EqControl, Keep};
use super::live::{CONFIG_RATE_HZ, LiveNode, SLOTS, render_live_conf};
use super::manage::{ApplyRequest, Chain, EqFailure, Status, check_playable, played_preamp};
use super::points::Points;
use super::session::{Outbox, Params};
use super::{HostEqState, Manager, Route};

/// The sink's rates a sequence moves among.
const RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

/// How much over unity a played curve may be, in dB: what f32 rounding of the coefficients
/// leaves, the same bound the safety tests hold the preamp to.
const TOLERANCE_DB: f64 = 0.01;

/// How many failing seeds of one run are shrunk and told in full.
const SHRUNK_REPORTS: usize = 3;

/// The shortest time between two starts of `pw-cli`, as the session keeps it.
const RESTART_GAP_MS: u64 = 1_000;

/// The wait the command line makes after it starts the chain.
const START_WATCH_MS: u64 = 300;

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
    };
    for node in text.split("{ type = builtin").skip(1) {
        let name = node
            .split_whitespace()
            .skip_while(|word| *word != "name")
            .nth(2)
            .unwrap_or("");
        if name == "lpre" {
            conf.pre = field(node, "Mult = ").expect("the preamp's Mult");
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
    /// From when it can be found in the graph.
    listed_at_ms: u64,
    /// The config it started from, which a change of rate goes back to.
    started_from: Conf,
    /// The value of every port.
    ports: HashMap<String, f32>,
}

impl Node {
    fn start(id: u32, conf: &Conf, listed_at_ms: u64) -> Node {
        let mut node = Node {
            id,
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

    /// Every slot takes the config's values again.
    fn load_slots(&mut self) {
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
    /// The next line ends it: `Some(true)` after applying the line, `Some(false)` without.
    dies_after_next_line: Option<bool>,
    /// It refused a `set-param` and the session has not read that yet.
    refused: bool,
    /// A refusal that shows after this many more reads of the flag: the session's reader task
    /// sets it a moment after `pw-cli` prints it.
    refusal_in: Option<u8>,
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
    /// A rule the model saw broken, said once.
    broken: Vec<String>,
    /// What happened, when a run is traced.
    trace: Option<Vec<String>>,
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

    fn state(&self) -> Option<HostEqState> {
        HostEqState::load(&self.state_path()).ok().flatten()
    }

    /// Start a node from the config on disk.
    fn start_node(&mut self, ours: bool) {
        self.next_id += 1;
        self.node = self
            .conf
            .as_ref()
            .map(|conf| Node::start(self.next_id, conf, self.clock_ms + self.list_delay_ms));
        self.node_is_ours = ours;
        let id = self.next_id;
        let pre = self
            .node
            .as_ref()
            .map(|node| 20.0 * node.value("lpre:Mult").log10());
        let peaks = self.node.as_ref().map(|node| {
            let (sets, _) = played(node, 'l');
            RATES.map(|rate| format!("{:.2}", dense_peak(&sets, rate)))
        });
        self.note(|| {
            format!("node {id} starts from the config on disk (ours: {ours}), preamp {pre:?}, slot peaks at the rates {peaks:?}")
        });
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
        let stated = request.correction.preamp_db;
        if !stated.is_finite() || stated > 30.0 {
            return Err(failure("the preamp is not within +30 dB"));
        }
        let held = Points::from_state(&HostEqState {
            on: true,
            name: String::new(),
            origin: None,
            target: String::new(),
            route: Route::Smart,
            manager: Manager::Child,
            previous_default: None,
            preamp_db: 0.0,
            filters: request.correction.filters.clone(),
            slots: request.slots.clone(),
        });
        if request.correction.filters.len() > SLOTS {
            return Err(failure("more than 16 filters"));
        }
        let mut shape = request.correction.clone();
        shape.preamp_db = 0.0;
        apo::validate(&shape).map_err(|e| failure(&e.to_string()))?;
        let rate = request.rate_hz.unwrap_or(CONFIG_RATE_HZ);
        check_playable(&request.correction, rate)?;
        request.correction.preamp_db = played_preamp(&request.correction, rate);
        let text = render_live_conf(
            &held,
            request.correction.preamp_db,
            "sink",
            Route::Smart,
            rate,
        );
        let state = HostEqState {
            on: true,
            name: request.name,
            origin: request.origin,
            target: "sink".to_string(),
            route: Route::Smart,
            manager: Manager::Child,
            previous_default: None,
            preamp_db: request.correction.preamp_db,
            filters: held.filters.clone(),
            slots: held.slots.clone(),
        };
        Ok((state, parse_conf(&text)))
    }

    /// Stop the chain and take its files away; the state file says off.
    fn remove_chain(&mut self) {
        self.node = None;
        self.conf = None;
        if let Some(mut state) = self.state() {
            state.on = false;
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
            format!("set-param node {id}: slots {slots:?} preamp {pre:?}")
        });
        let lands = !matches!(self.cli.dies_after_next_line, Some(false));
        match self.node.as_mut() {
            Some(node) if node.id == id => {
                if lands {
                    for (name, value) in params {
                        if node.ports.contains_key(name) {
                            node.ports.insert(name.clone(), held_value(name, *value));
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
                if self.node_is_ours && self.node.is_some() {
                    self.broken
                        .push(format!("a set-param went to the stale id {id}, after the controller's own apply put node {} there",
                            self.node.as_ref().map_or(0, |node| node.id)));
                }
            }
        }
        if self.cli.dies_after_next_line.take().is_some() {
            self.cli.alive = false;
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
        let (state, conf) = World::stage_apply(request)?;
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
        let rate = request.rate_hz.unwrap_or(CONFIG_RATE_HZ);
        let mut correction = request.correction;
        correction.preamp_db = played_preamp(&correction, rate);
        let points = Points::from_state(&HostEqState {
            filters: correction.filters.clone(),
            slots: request.slots.clone(),
            ..state
        });
        let text = render_live_conf(&points, correction.preamp_db, "sink", Route::Smart, rate);
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

/// The controller's `Params` over the world: the session's own bookkeeping ([`Outbox`], the
/// kept node, the restart gap, the refusal) over a `pw-cli` that lives in the world.
struct ModelParams {
    world: Shared,
    outbox: Outbox,
    node: Option<LiveNode>,
    started_ms: Option<u64>,
}

impl ModelParams {
    fn new(world: Shared) -> ModelParams {
        ModelParams {
            world,
            outbox: Outbox::default(),
            node: None,
            started_ms: None,
        }
    }

    fn take_refusal(&mut self) -> bool {
        let refused = {
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
        };
        if refused {
            self.node = None;
            self.outbox.mark_full();
        }
        refused
    }

    fn ensure_running(&mut self) -> bool {
        let mut world = world(&self.world);
        if world.cli.alive {
            return true;
        }
        if self
            .started_ms
            .is_some_and(|started| world.clock_ms < started + RESTART_GAP_MS)
        {
            return false;
        }
        self.started_ms = Some(world.clock_ms);
        world.cli = Cli {
            alive: true,
            ..Cli::default()
        };
        drop(world);
        self.outbox.mark_full();
        true
    }

    async fn deliver(&mut self, id: u32, params: &[(String, f64)]) -> bool {
        self.outbox.record(params);
        for _ in 0..2 {
            self.take_refusal();
            let Some(node) = self.node().await else {
                return false;
            };
            if node.id != id {
                self.outbox.mark_full();
                return false;
            }
            if !self.ensure_running() {
                return false;
            }
            let line = self.outbox.line(params);
            let written = {
                let mut world = world(&self.world);
                let once = std::mem::take(&mut world.fail_next_write);
                match once || world.refuse_sends_until_ms > world.clock_ms {
                    true => {
                        world.cli.alive = false;
                        false
                    }
                    false => {
                        world.set_param(node.id, &line);
                        true
                    }
                }
            };
            if written {
                self.outbox.delivered();
                return true;
            }
            self.node = None;
            self.outbox.mark_full();
        }
        false
    }
}

#[async_trait]
impl Params for ModelParams {
    async fn node(&mut self) -> Option<LiveNode> {
        self.take_refusal();
        if self.node.is_none() {
            let world = world(&self.world);
            let node = world.listed()?;
            self.node = Some(LiveNode {
                id: node.id,
                rate_hz: Some(world.sink_rate),
                params: node
                    .ports
                    .iter()
                    .map(|(name, value)| (name.clone(), f64::from(*value)))
                    .collect(),
            });
        }
        self.node.clone()
    }

    async fn sink_rate(&mut self) -> Option<u32> {
        Some(world(&self.world).sink_rate)
    }

    async fn send(&mut self, node: u32, params: &[(String, f64)]) -> bool {
        let sent = self.deliver(node, params).await;
        if !sent {
            self.outbox.mark_full();
        }
        sent
    }

    fn forget_node(&mut self) {
        self.node = None;
    }

    fn new_chain(&mut self) {
        self.node = None;
        self.outbox.new_chain();
    }

    async fn settle(&mut self, node: Option<u32>) -> bool {
        self.take_refusal();
        match (self.outbox.repair_due(), node) {
            (false, _) => true,
            (true, None) => false,
            (true, Some(node)) => self.send(node, &[]).await,
        }
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

/// The highest `sets` rise anywhere from 0.01 Hz to Nyquist at `rate_hz`, DC and Nyquist
/// included: a log grid in frequency, a second log grid in the distance from Nyquist, each
/// biquad's own pole and zero angles, and every local maximum zoomed in on by repeated scans.
fn dense_peak(sets: &[[f64; 5]], rate_hz: u32) -> f64 {
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

/// What the node plays on one channel: the biquads and the preamp in dB.
fn played(node: &Node, channel: char) -> (Vec<[f64; 5]>, f64) {
    let sets = (1..=SLOTS)
        .map(|slot| {
            let at = |port: &str| node.value(&format!("{channel}{slot:02}:{port}"));
            let a0 = at("a0");
            [
                at("b0") / a0,
                at("b1") / a0,
                at("b2") / a0,
                at("a1") / a0,
                at("a2") / a0,
            ]
        })
        .collect();
    (
        sets,
        20.0 * node.value(&format!("{channel}pre:Mult")).log10(),
    )
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
        .map(|_| match rng.below(100) {
            0..=19 => Event::Ticks(1 + rng.below(45) as u32),
            20..=37 => Event::Drag {
                pick: rng.below(16) as usize,
                kind: rng.chance(15).then(|| filter(&mut rng, true).kind),
                f: rng.chance(30).then(|| rng.log_range(20.0, 20_000.0)),
                g: rng.chance(85).then(|| rng.range(-12.0, 12.0)),
                q: rng.chance(20).then(|| rng.log_range(0.1, 10.0)),
                steps: rng.below(5) as u8,
                gap_ticks: rng.below(3) as u8,
            },
            38..=45 => {
                let (filters, stated) = curve(&mut rng);
                Event::Load { filters, stated }
            }
            46..=52 => Event::Toggle(rng.chance(50)),
            53..=58 => {
                let (filters, stated) = curve(&mut rng);
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
                Event::Add(filter(&mut rng, hand))
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
            96..=97 => Event::RefusalLag(rng.below(3) as u8),
            _ => Event::DaemonRestart,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The driver

/// A scratch directory, taken away with the run.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "svolder-lab-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Dir(path)
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

/// A run of one sequence.
struct Lab {
    shared: Shared,
    control: EqControl,
    peaks: Peaks,
    /// The daemon's own count of ticks, which says when it reads the state file.
    daemon_ticks: u64,
    _dir: Dir,
    /// The event being run, for the message of a failure.
    at: usize,
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
    let path = world(shared).state_path();
    EqControl::new(
        Arc::new(ModelChain {
            world: shared.clone(),
        }),
        Box::new(ModelParams::new(shared.clone())),
        Keep::File(path),
    )
}

impl Lab {
    /// A world with no chain, the sink at `rate`, and a controller over it.
    fn new(rate: u32) -> Lab {
        let dir = Dir::new();
        let shared: Shared = Arc::new(Mutex::new(World {
            dir: dir.0.clone(),
            clock_ms: 10_000,
            sink_rate: rate,
            conf: None,
            node: None,
            next_id: 30,
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
            trace: None,
        }));
        let control = new_control(&shared);
        Lab {
            shared,
            control,
            peaks: Peaks::default(),
            daemon_ticks: 0,
            _dir: dir,
            at: 0,
        }
    }

    /// The rule: what the node plays is never over unity.
    fn check(&mut self) -> Result<(), Failure> {
        let (broken, played) = {
            let mut world = world(&self.shared);
            let broken = world.broken.pop();
            let played = world.node.as_ref().map(|node| {
                (
                    ['l', 'r'].map(|channel| played(node, channel)),
                    world.sink_rate,
                    node.id,
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
        let Some((channels, rate, id)) = played else {
            return Ok(());
        };
        for (channel, (sets, pre_db)) in ['l', 'r'].into_iter().zip(&channels) {
            if let Some(slot) = sets.iter().position(|set| !stable(set)) {
                return fail(format!(
                    "node {id} plays an unstable biquad in {channel}{:02}: {:?}",
                    slot + 1,
                    sets[slot]
                ));
            }
            let peak = self.peaks.of(sets, rate);
            if peak + pre_db > TOLERANCE_DB {
                return fail(format!(
                    "node {id} plays {:+.3} dB over unity at {rate} Hz on {channel}: the slots peak {peak:+.3} dB under a preamp of {pre_db:+.3} dB",
                    peak + pre_db
                ));
            }
        }
        Ok(())
    }

    async fn ticks(&mut self, count: u32) -> Result<(), Failure> {
        for _ in 0..count {
            world(&self.shared).clock_ms += 50;
            self.control.tick().await;
            self.daemon_ticks += 1;
            if self.daemon_ticks.is_multiple_of(20) {
                self.control.reload_if_changed();
            }
            self.check()?;
        }
        Ok(())
    }

    async fn run(&mut self, event: &Event) -> Result<(), Failure> {
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
                };
                // The daemon is not blocked by the command line: it ticks in the wait.
                let staged = World::stage_apply(request);
                if let Ok((state, conf)) = staged {
                    {
                        let mut world = world(&self.shared);
                        world.conf = Some(conf);
                        world.start_node(false);
                    }
                    self.check()?;
                    self.ticks(u32::from(*gap_ticks)).await?;
                    world(&self.shared).write_state_as_command_line(&state);
                }
            }
            Event::Reload => {
                self.control.reload_if_changed();
            }
            Event::NodeRestart => {
                let mut world = world(&self.shared);
                if world.node.is_some() {
                    world.start_node(false);
                }
            }
            Event::Rate(rate) => {
                let mut world = world(&self.shared);
                world.sink_rate = *rate;
                if let Some(node) = world.node.as_mut() {
                    node.load_slots();
                }
            }
            Event::CliDeath { after_line } => {
                let mut world = world(&self.shared);
                match after_line {
                    None => {
                        world.cli.alive = false;
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
            Event::DaemonRestart => {
                let (writes, sends, state) = {
                    let mut world = world(&self.shared);
                    // The old daemon's `pw-cli` ends with it.
                    world.cli = Cli::default();
                    (
                        world.writes,
                        world.sends,
                        std::fs::read(world.state_path()).ok(),
                    )
                };
                self.control = new_control(&self.shared);
                self.daemon_ticks = 0;
                let world = world(&self.shared);
                let after = std::fs::read(world.state_path()).ok();
                if world.writes != writes || world.sends != sends || after != state {
                    drop(world);
                    return Err(Failure {
                        event: self.at,
                        what: "the daemon wrote or sent something while it started".to_string(),
                    });
                }
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
        other => format!("{other:?}"),
    }
}

/// The report of a failing seed: what broke and the shrunk sequence that does it.
pub fn report(seed: u64, failure: &Failure) -> String {
    let (rate, events) = sequence_of(seed);
    let shrunk = shrink(rate, &events);
    let again = run_sequence(rate, &shrunk).err();
    let mut text = format!(
        "seed {seed}: {} (event {} of {})\nshrunk to {} events, the sink starting at {rate} Hz",
        failure.what,
        failure.event + 1,
        events.len(),
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

/// Run seeds `first..first + runs`, on every core; the reports of the ones that failed.
pub fn run_seeds(first: u64, runs: u64) -> Vec<String> {
    let next = AtomicUsize::new(0);
    let reported = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(16);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::SeqCst) as u64;
                    if at >= runs {
                        return;
                    }
                    let seed = first + at;
                    let (rate, events) = sequence_of(seed);
                    if let Err(failure) = run_sequence(rate, &events) {
                        // The first few are shrunk and told in full; the rest by their seed.
                        let text = match reported.fetch_add(1, Ordering::SeqCst) < SHRUNK_REPORTS {
                            true => report(seed, &failure),
                            false => format!("seed {seed}: {}", failure.what),
                        };
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
