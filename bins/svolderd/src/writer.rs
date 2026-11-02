// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One writer per device, in front of its actor: every write a front end asks for goes through
//! it, in the order it is asked for.
//!
//! The actor runs one command at a time, first in, first out, and the writer decides what
//! reaches it ([`slots`]): a write waits behind the one in flight on its own function, so a
//! burst on one control is at most one write in flight and one waiting — a [`Mode::Latest`]
//! write (a slider) replaces the one waiting, whose waiter is told it was superseded, and a
//! [`Mode::Ordered`] write is never dropped. A submitted write that waits is not overtaken by a
//! write submitted after it, whatever function that one is on, and a replacement keeps the place
//! of the write it replaced; a preset recall is no such write, and may start ahead of a write
//! waiting on another control. A recall is planned from the device's readings when it starts,
//! which waits for any write it holds in flight or waiting; every such write that arrives while
//! it runs or waits is held until it is done, and a later recall replaces one that has not
//! started, drops the held writes it rewrites and starts after the ones held before it. A
//! device's built-in setting ([`Writer::apply`]) is a recall in the same line: its band writes go
//! out as one batch, each waited for, every band going down before any goes up, and it holds the
//! writes to the bands it sets, to the tone control and to a device slot's recall (whose bands
//! the device then reports) and save (which stores the bands as they stand). A write is keyed on
//! its function as the profile reads it, so a band written under its two-id form shares the line
//! of the same band written under its own id; a device slot's recall or save is an action, sent
//! to the device without waiting for an echo it never makes. A recall of a slot goes out only once
//! no held write is in flight and no echo is awaited for a control it changes, and holds the line
//! behind it until then, so every echo from before it is in. A band written without waiting is
//! answered at its `Ack` like any write, so the next write on the band goes at once; its echo is
//! awaited on the side until it comes or the session's set timeout passes, and only such a recall,
//! or a setting (planned from the readings), waits for it. A band written again while its echo is
//! awaited cannot tell the first echo from the second: it is awaited until the set timeout after
//! the latest write. A waited band write answered with the set timeout has had its echo waited for
//! as long: its band is unsure, as one whose echo is late. A band whose echo is late is never taken
//! as reported after the recall. A recall's or a setting's own band writes are tracked the same
//! way, once its batch is done. A setting plans a band that has not reported since such a recall as
//! unread.
//! Reads, the passphrase and the actor's own housekeeping go to the actor directly.
//!
//! A glide ([`Writer::glide`]) is judged by the shared rule ([`safety::judge`]): a level that may
//! land at once is written at once; a volume above three quarters of its range climbs, one step
//! at a time, a tenth of the range a second, owned by the front end that asked, from the echo of
//! the step written at once. A glide from the owner steers it; any other write to the volume, a
//! mute, a power-off, a reading the climb did not write, a lost link or a changed profile ends
//! it, and ends a climb whose first step is still on its way before it starts: a first step still
//! waiting in the line is then not sent, and its glide is answered with the reading.
//!
//! A band write may ask to switch a tone control that reads Flat to EQ once it goes through
//! ([`Options::flat_to_eq`]).
//!
//! The writer keeps the device's recall memory and publishes its climb on two `watch` channels
//! ([`Writer::recalled`], [`Writer::climb`]).

mod slots;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use svolder_ipc::{Response, ValueObject};
use svolder_profiles::device_presets;
use svolder_profiles::roles;
use svolder_profiles::safety::{self, Climb, Motion, Rules, Situation, Step};
use svolder_profiles::tone::preset::{self, Preset};
use svolder_profiles::{Control, ControlKind, EqMode, EqParam, Profile, describe, intent};
use svolder_proto::{FunctionId, Message, Value, ValueType};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use crate::device::{Command, Feed};
use crate::log;
use crate::recall::{self, Applied, Recalled};
pub use slots::Mode;
use slots::{Action, Queued, Slots};

/// The longest a write waits to be handed to the actor, and then for the actor's answer.
const WRITE_DEADLINE: Duration = Duration::from_secs(60);

/// How long a band written without waiting is awaited for its echo; it mirrors the session's set
/// timeout (`SessionConfig::set_timeout`).
const ECHO_WAIT: Duration = Duration::from_secs(2);

/// How often a climb is paid and looked at.
const CLIMB_TICK: Duration = Duration::from_millis(20);

/// How long a step asked for and not yet read back counts as where its function is heading.
const ASKED_LIFETIME: Duration = Duration::from_secs(5);

/// What a waiter is told when the writer itself stops before its write went out.
const STOPPED: &str = "the device's writer stopped";

/// A device actor's command channel.
pub(crate) type Commands = mpsc::Sender<(Command, oneshot::Sender<Response>)>;

/// The front end a write came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A client of the daemon's socket: the CLI, the TUI.
    Socket,
    /// The MCP server.
    Mcp,
    /// The MQTT bridge.
    Mqtt,
    /// The web UI.
    Web,
}

impl Origin {
    /// The front end, as a log line names it.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Origin::Socket => "socket",
            Origin::Mcp => "mcp",
            Origin::Mqtt => "mqtt",
            Origin::Web => "web",
        }
    }
}

/// How a write ended.
#[derive(Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "an outcome is made once per write and moved once"
)]
pub enum Outcome {
    /// The actor answered.
    Done(Response),
    /// A newer write to the same function took its place before it was sent.
    Superseded,
    /// It never reached the device, for this reason.
    Cancelled(&'static str),
}

/// How a glide began.
#[derive(Debug)]
pub enum Glided {
    /// It lands at once: the actor's answer.
    Landed(Response),
    /// A climb toward `target` is on, after `first`, the answer to the step written at once when
    /// there was one.
    Climbing {
        /// The answer to the step written at once.
        first: Option<Response>,
        /// Where the climb heads.
        target: i64,
    },
    /// It could not start: why.
    Refused(Response),
    /// A newer write to the same control took its place before it was sent.
    Superseded,
}

/// A recall or a built-in setting as it starts.
pub enum Planned {
    /// The preset found and its plan, whose writes go out now.
    Plan(Recalled),
    /// The built-in setting found and its writes, which go out now; none when the device is
    /// already on it.
    Setting(Applied),
    /// Why it could not be planned.
    Refused(String),
}

/// The two answers of a recall or of a built-in setting.
pub struct Recalling {
    /// Its plan as it starts, or why there is none; dropped unanswered when a later recall took
    /// its place before it started.
    pub planned: oneshot::Receiver<Planned>,
    /// How it ended.
    pub done: oneshot::Receiver<Outcome>,
}

/// A climb as the writer publishes it: the level it climbs and where to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClimbView {
    /// The level's profile key.
    pub key: String,
    /// The raw step it climbs to.
    pub target: i64,
}

/// The front ends' end of one device's writer; its clones share it.
#[derive(Clone)]
pub struct Writer {
    asks: mpsc::UnboundedSender<Ask>,
    /// The preset last recalled on the device, by any front end.
    recalled: watch::Receiver<Option<String>>,
    /// The climb on, while one is.
    climb: watch::Receiver<Option<ClimbView>>,
}

/// How one submitted write is made: how it waits behind the one in flight on its function, and
/// whether a band written while the tone control reads Flat switches it to EQ afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// How it waits.
    pub mode: Mode,
    /// Whether a band write that goes through while the tone control reads Flat (the device's
    /// bypass, under which no band applies) is followed by a write of the mode EQ, so the edit
    /// is heard ([`Task::switch_to_eq`]). MQTT and the web ask for it; the socket does not, the
    /// TUI switching itself.
    pub flat_to_eq: bool,
}

impl From<Mode> for Options {
    /// `mode`, with no switch to EQ.
    fn from(mode: Mode) -> Self {
        Options {
            mode,
            flat_to_eq: false,
        }
    }
}

/// What a front end asks of the writer.
enum Ask {
    Submit {
        origin: Origin,
        command: Command,
        options: Options,
        done: oneshot::Sender<Outcome>,
    },
    Recall {
        origin: Origin,
        name: String,
        fitting: Vec<Preset>,
        planned: oneshot::Sender<Planned>,
        done: oneshot::Sender<Outcome>,
    },
    Apply {
        origin: Origin,
        name: String,
        planned: oneshot::Sender<Planned>,
        done: oneshot::Sender<Outcome>,
    },
    Glide {
        origin: Origin,
        key: String,
        raw: i64,
        mode: Mode,
        glided: oneshot::Sender<Glided>,
    },
    /// A front end recalled this preset by its own writes.
    Recalled(String),
    #[cfg(test)]
    View(oneshot::Sender<View>),
}

impl Writer {
    /// Start the writer of the device whose actor takes `commands` and publishes `feed`. It runs
    /// until every clone of the returned handle is dropped.
    pub fn spawn(commands: Commands, feed: watch::Receiver<Feed>) -> Writer {
        let (asks, inbox) = mpsc::unbounded_channel();
        let (recalled_tx, recalled) = watch::channel(None);
        let (climb_tx, climb) = watch::channel(None);
        tokio::spawn(Task::new(commands, feed, recalled_tx, climb_tx).run(inbox));
        Writer {
            asks,
            recalled,
            climb,
        }
    }

    /// The preset last recalled on the device, by any front end since the daemon started: set
    /// when a recall's plan starts, and by [`Writer::note_recalled`].
    pub fn recalled(&self) -> watch::Receiver<Option<String>> {
        self.recalled.clone()
    }

    /// The climb on, while one is: its level and target, published as it starts, as its owner
    /// steers it and (`None`) as it ends.
    pub fn climb(&self) -> watch::Receiver<Option<ClimbView>> {
        self.climb.clone()
    }

    /// Note that a front end recalled the preset `name` by its own writes.
    pub fn note_recalled(&self, name: String) {
        let _ = self.asks.send(Ask::Recalled(name));
    }

    /// A writer with nothing behind it, for a test's handle that never writes.
    #[cfg(test)]
    pub fn unattended() -> Writer {
        let (asks, _) = mpsc::unbounded_channel();
        let (_, recalled) = watch::channel(None);
        let (_, climb) = watch::channel(None);
        Writer {
            asks,
            recalled,
            climb,
        }
    }

    /// Write `command` for `origin`, behind whatever is in flight on its function, as `mode`
    /// says; the order of these calls is the order the writer takes them. Answered when the actor
    /// has, or when the write is superseded or cannot be sent. With
    /// [`flat_to_eq`](Options::flat_to_eq), a band written in Flat is followed by the mode EQ.
    pub fn submit(
        &self,
        origin: Origin,
        command: Command,
        options: impl Into<Options>,
    ) -> oneshot::Receiver<Outcome> {
        let (done, answer) = oneshot::channel();
        let _ = self.asks.send(Ask::Submit {
            origin,
            command,
            options: options.into(),
            done,
        });
        answer
    }

    /// Set the level `key` to raw step `raw` for `origin` by the shared rule, as the module's docs
    /// say; the step it writes waits as `mode` says. Answered as soon as the climb is on, never
    /// at its end, so a front end that paces its own writes is not held up by it.
    pub fn glide(
        &self,
        origin: Origin,
        key: String,
        raw: i64,
        mode: Mode,
    ) -> oneshot::Receiver<Glided> {
        let (glided, answer) = oneshot::channel();
        let _ = self.asks.send(Ask::Glide {
            origin,
            key,
            raw,
            mode,
            glided,
        });
        answer
    }

    /// Recall the preset `name` among `fitting`, the presets that fit the device, as one batch
    /// (see the module's docs).
    pub fn recall(&self, origin: Origin, name: String, fitting: Vec<Preset>) -> Recalling {
        let (planned, plan) = oneshot::channel();
        let (done, answer) = oneshot::channel();
        let _ = self.asks.send(Ask::Recall {
            origin,
            name,
            fitting,
            planned,
            done,
        });
        Recalling {
            planned: plan,
            done: answer,
        }
    }

    /// Apply the built-in setting `name` of the device as one batch (see the module's docs): the
    /// answers are those of a recall, [`Planned::Setting`] for the plan.
    pub fn apply(&self, origin: Origin, name: String) -> Recalling {
        let (planned, plan) = oneshot::channel();
        let (done, answer) = oneshot::channel();
        let _ = self.asks.send(Ask::Apply {
            origin,
            name,
            planned,
            done,
        });
        Recalling {
            planned: plan,
            done: answer,
        }
    }

    /// What the writer holds once every ask before this one is taken in.
    #[cfg(test)]
    pub async fn view(&self) -> View {
        let (reply, view) = oneshot::channel();
        let _ = self.asks.send(Ask::View(reply));
        view.await.expect("the writer is running")
    }
}

/// What a writer holds, for a test to look at.
#[cfg(test)]
#[derive(Debug, Default, PartialEq)]
pub struct View {
    /// The functions with a write in flight.
    pub in_flight: Vec<FunctionId>,
    /// How many writes a recall holds.
    pub held: usize,
    /// Whether a recall is running.
    pub recall_running: bool,
    /// The preset a recall waiting to start is for.
    pub recall_waiting: Option<String>,
}

/// A write the writer holds until the actor has answered it.
struct Pending {
    /// The front end it came from.
    origin: Origin,
    /// What the actor is asked to do.
    command: Command,
    /// Who waits for the answer.
    answer: Answer,
    /// The relative step this write was worked out from, to work out again as it is sent.
    relative: Option<Relative>,
    /// The origin and arrival number of a band write submitted with `flat_to_eq`, which
    /// [`Task::switch_to_eq`] may follow once it is answered.
    switch: Option<(Origin, u64)>,
}

/// A relative step worked out to an absolute one when it arrived.
struct Relative {
    /// How far it steps, signed.
    delta: i64,
    /// The step it was worked out to: the step sent is never louder than this.
    to: i64,
    /// The function's raw range.
    range: (i64, i64),
}

/// Who waits for a write's answer.
enum Answer {
    /// A submitted write's waiter.
    Write(oneshot::Sender<Outcome>),
    /// A glide that lands at once.
    Landed(oneshot::Sender<Glided>),
    /// A glide's first step: the climb it starts from its echo ([`Task::first_answered`]).
    First(Start),
    /// A climb's own step, which nobody waits for; its answer must be an echo of the step
    /// ([`Task::answered`]).
    Step {
        /// The raw step written.
        step: i64,
        /// [`Task::epoch`] when it went out.
        epoch: u64,
        /// The reading the step was taken from: while it waits, the volume reads this or the
        /// step, and anything else is a change the climb did not write.
        before: i64,
    },
}

/// A glide's first step on its way, and the climb its echo starts.
struct Start {
    /// Who waits for the glide's answer.
    glided: oneshot::Sender<Glided>,
    /// The front end whose glide it is.
    owner: Origin,
    /// Where the climb heads.
    target: i64,
    /// The step written at once: the climb starts only from an echo of it.
    step: i64,
    /// [`Task::epoch`] when the step went out: the climb starts only if it has not moved since.
    epoch: u64,
}

impl Answer {
    /// Hand over how the write ended.
    fn send(self, outcome: Outcome) {
        match self {
            Answer::Write(waiter) => {
                let _ = waiter.send(outcome);
            }
            Answer::Landed(glided) | Answer::First(Start { glided, .. }) => {
                let _ = glided.send(match outcome {
                    Outcome::Done(response) => Glided::Landed(response),
                    Outcome::Superseded => Glided::Superseded,
                    Outcome::Cancelled(why) => Glided::Refused(Response::Error {
                        code: 3,
                        message: why.to_string(),
                    }),
                });
            }
            Answer::Step { .. } => {}
        }
    }
}

/// A volume climbing for the front end that asked.
struct Climbing {
    /// The front end whose glide it is.
    owner: Origin,
    /// The volume's function and key.
    id: FunctionId,
    key: String,
    /// The volume's raw range.
    range: (i64, i64),
    /// The stepper.
    climb: Climb,
    /// The reading the step now written was taken from: while that step waits, the reading is
    /// this or the step, and anything else is a change the climb did not write.
    before: i64,
    /// When the credit was last paid.
    at: Instant,
}

impl Climbing {
    /// A climb of `control` toward `target` for `owner`, from `written`, the step last written or
    /// asked, while the volume reads `before`; `None` for a control with no range.
    fn new(
        owner: Origin,
        control: &Control,
        target: i64,
        written: i64,
        before: i64,
    ) -> Option<Climbing> {
        Some(Climbing {
            owner,
            id: control.id,
            key: control.key.clone(),
            range: control.range?,
            climb: Climb::start(target, written),
            before,
            at: Instant::now(),
        })
    }

    /// Whether the volume's `reading` is a change the climb did not write: neither the reading
    /// its step was taken from nor the step itself.
    fn disowns(&self, reading: Option<i64>) -> bool {
        unwritten(reading, self.before, self.climb.written())
    }
}

/// Whether the volume's `reading` is a change a climb did not write while its step `step`, taken
/// from `before`, waits: neither of the two.
fn unwritten(reading: Option<i64>, before: i64, step: i64) -> bool {
    reading.is_some_and(|reading| reading != before && reading != step)
}

/// A glide's first step on its way, before the climb it starts.
struct Starting {
    /// The volume's function.
    id: FunctionId,
    /// The readings the volume may show while the step is on its way, as [`Task::shows`] lists
    /// them: where it was when the glide came, where it was heading, the step itself and the
    /// writes held. Any other reading is a change the climb did not write; a knob turned to one
    /// of these values is taken for an echo.
    shows: Vec<i64>,
}

/// What a pick is for.
enum Recipe {
    /// The tone-control preset `name` among `fitting`, the presets that fit the device.
    Preset { name: String, fitting: Vec<Preset> },
    /// The device's built-in setting `name`.
    Setting { name: String },
}

impl Recipe {
    /// The name asked for.
    fn name(&self) -> &str {
        match self {
            Recipe::Preset { name, .. } | Recipe::Setting { name } => name,
        }
    }
}

/// A recall or a built-in setting asked for and not started.
struct Pick {
    origin: Origin,
    recipe: Recipe,
    planned: oneshot::Sender<Planned>,
    done: oneshot::Sender<Outcome>,
}

/// An answer on its way back from the actor: the function the write was keyed on, the step it
/// asked of it, who waits, and the answer.
type Reply = BoxFuture<
    'static,
    (
        Option<FunctionId>,
        Option<i64>,
        Answer,
        Response,
        Option<(Origin, u64)>,
        Option<Echo>,
    ),
>;

/// A write to a band a recall of a device slot changes, whose echo that recall must not take for a
/// report after it ([`Task::note_echo`]): a write sent on its own, or a step of a recall or a
/// built-in setting.
struct Echo {
    /// When the feed last heard the band as the write went out.
    was: Option<u64>,
    /// Whether the write was sent without waiting.
    no_wait: bool,
    /// Whether an earlier write's echo was awaited, or had missed its deadline and may still
    /// come, as the write went out.
    repeated: bool,
}

/// What the answer to a band write says of its echo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    /// `Ack`: a write sent without waiting went out, and its echo is to come.
    Ack,
    /// A reading: the echo of a waited write, or what the session took for it.
    Value,
    /// No echo within the session's set timeout, or no answer at all: the device may have taken
    /// the write, and its echo may still come.
    TimedOut,
    /// A refusal, or a write that never went out: no echo is to come.
    Nothing,
}

impl Landing {
    /// What `response`, the answer to a band write, says of its echo.
    fn of(response: &Response) -> Landing {
        match response {
            Response::Ack => Landing::Ack,
            Response::Value(_) => Landing::Value,
            response if timed_out(response) => Landing::TimedOut,
            _ => Landing::Nothing,
        }
    }
}

/// A step of a recall or a built-in setting to a band a recall of a device slot changes, and its
/// answer, noted once the batch is done ([`Task::note_echo`]).
struct Stepped {
    id: FunctionId,
    echo: Echo,
    landing: Landing,
    /// When the step was answered.
    at: Instant,
}

/// How long a band whose echo did not come in time is not taken as reported.
enum Unsure {
    /// Until the feed hears it: it was last heard with this stamp.
    Until(Option<u64>),
    /// Until the next recall of a device slot, whatever the feed hears.
    Recall,
}

/// A band whose echo is awaited after its write was answered `Ack`, and what a recall of a device
/// slot behind it waits for.
struct Awaiting {
    /// When the feed last heard the band as the first write went out.
    was: Option<u64>,
    /// When to stop waiting.
    due: Instant,
    /// Whether the band was written again while an echo was awaited: the first echo cannot be
    /// told from the second, so only `due` ends the wait.
    repeated: bool,
}

/// A recall or a built-in setting on its way through the actor: who waits, how it ended, and its
/// steps to bands a recall of a device slot changes, as they were answered.
type Steps = BoxFuture<'static, (oneshot::Sender<Outcome>, Response, Vec<Stepped>)>;

/// The writer's own task and everything it keeps.
struct Task {
    commands: Commands,
    feed: watch::Receiver<Feed>,
    slots: Slots<Pending, Pick>,
    replies: FuturesUnordered<Reply>,
    recalls: FuturesUnordered<Steps>,
    /// The volume's climb, while one is on.
    climb: Option<Climbing>,
    /// How many times a climb has been ended ([`Task::end_climb`]): a glide's first step starts
    /// its climb only if this has not moved since the step went out.
    epoch: u64,
    /// The volume while a glide's first step is on its way and its climb has not started:
    /// whatever ends a climb ends that one before it starts.
    starting: Option<Starting>,
    /// When the climb is next paid and looked at.
    due: Instant,
    /// The last absolute step asked of each function, and when, until a reading shows it or
    /// [`ASKED_LIFETIME`] passes, or the write that asked it is answered with a value or a
    /// refusal ([`Task::settle_asked`]): where the function is heading, and what a raise is
    /// judged from when it is lower than the reading. Each carries whether a relative step
    /// worked out the step: a chain of them, which [`Task::resolved`] goes on from.
    asked: HashMap<FunctionId, (i64, Instant, bool)>,
    /// The last step handed to the actor on each function, and when, with whether a relative step
    /// worked it out, until a reading shows it, [`ASKED_LIFETIME`] passes, or its write is
    /// answered with a value or a refusal ([`Task::settle_sent`]): the only step a relative step
    /// is worked out again from as it goes out, since a step asked for may never be sent.
    sent: HashMap<FunctionId, (i64, Instant, bool)>,
    /// The epoch of the glide's first step or the climb's step that asked each function's
    /// [`Task::asked`] step, while that step still waits in the line (it is forgotten when the
    /// step is handed to the actor): a step whose epoch has moved is never sent, so it is no
    /// place for a climb to start from ([`Task::planned`]).
    stepped: HashMap<FunctionId, u64>,
    /// The profile the device was last seen with; another one ends a climb.
    profile: Option<Arc<Profile>>,
    /// Whether the actor's feed is still there to watch.
    feed_open: bool,
    /// The preset last recalled, which [`Writer::recalled`] watches.
    recalled: watch::Sender<Option<String>>,
    /// The climb as last published, which [`Writer::climb`] watches.
    shown_climb: watch::Sender<Option<ClimbView>>,
    /// How many writes and recalls have been submitted: the arrival number of the latest.
    arrived: u64,
    /// The arrival number of the latest mode write or recall submitted.
    mode_or_recall: u64,
    /// When a recall of a device slot last went out: the controls it changes, each with when the
    /// feed last heard it then. A reading stamped otherwise since is one the device reported after
    /// the recall ([`Task::unreported`]).
    after_recall: Vec<(FunctionId, Option<u64>, bool)>,
    /// The bands whose echo is awaited after a write answered: one written without waiting and
    /// answered `Ack`, and one written, waited or not, while an earlier echo was awaited or
    /// unsure (it has only a deadline), a recall's or a setting's band step among them. Each
    /// stays until the echo or the time to stop waiting ([`ECHO_WAIT`]).
    /// Their writes are answered, so the next write on the band goes at once; only a recall of a
    /// slot waits for its controls' entries ([`Slots::await_echo`]), so it goes out after every
    /// echo from before it, and a setting or a preset waits for any entry before it starts, to be
    /// planned from the readings.
    echoing: HashMap<FunctionId, Awaiting>,
    /// The bands whose echo did not come in time: awaited past [`ECHO_WAIT`], or written, waited
    /// for, and answered with the session's timeout. An echo that comes later cannot be told from
    /// a report after a recall, so a recall that goes out meanwhile marks them unreported for good.
    /// A band written once is unsure until the feed hears it ([`Unsure::Until`]); one written
    /// again while an echo was awaited or unsure stays so until the next recall
    /// ([`Unsure::Recall`], never replaced by the other), since one late echo cannot be told from
    /// the other.
    unsure: HashMap<FunctionId, Unsure>,
}

impl Task {
    fn new(
        commands: Commands,
        feed: watch::Receiver<Feed>,
        recalled: watch::Sender<Option<String>>,
        shown_climb: watch::Sender<Option<ClimbView>>,
    ) -> Task {
        let profile = feed.borrow().profile.clone();
        Task {
            commands,
            slots: Slots::default(),
            replies: FuturesUnordered::new(),
            recalls: FuturesUnordered::new(),
            climb: None,
            epoch: 0,
            starting: None,
            due: Instant::now(),
            asked: HashMap::new(),
            sent: HashMap::new(),
            stepped: HashMap::new(),
            profile,
            feed,
            feed_open: true,
            recalled,
            shown_climb,
            arrived: 0,
            mode_or_recall: 0,
            after_recall: Vec::new(),
            echoing: HashMap::new(),
            unsure: HashMap::new(),
        }
    }

    /// Publish the climb as it stands: its level and target, or none. Only a change is sent, so a
    /// receiver wakes as a climb starts, is steered or ends.
    fn show_climb(&self) {
        let now = self.climb.as_ref().map(|climbing| ClimbView {
            key: climbing.key.clone(),
            target: climbing.climb.target(),
        });
        self.shown_climb.send_if_modified(|shown| {
            let changed = *shown != now;
            if changed {
                *shown = now;
            }
            changed
        });
    }

    /// Take the asks in order and carry them out, until every [`Writer`] is dropped; then answer
    /// every write and recall still waiting.
    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<Ask>) {
        loop {
            let echo_due = self.echo_due();
            tokio::select! {
                ask = inbox.recv() => match ask {
                    Some(ask) => self.ask(ask).await,
                    None => break,
                },
                Some((id, asked, answer, response, switch, echo)) = self.replies.next(),
                    if !self.replies.is_empty() => {
                    self.answered(id, asked, answer, response, switch, echo).await;
                }
                Some((done, response, stepped)) = self.recalls.next(),
                    if !self.recalls.is_empty() => {
                    // The steps' echoes are noted before the line moves on, as a write's are.
                    for step in stepped {
                        self.note_echo(step.id, &step.echo, step.landing, step.at);
                    }
                    let _ = done.send(Outcome::Done(response));
                    let actions = self.slots.recall_done();
                    self.act(actions).await;
                }
                changed = self.feed.changed(), if self.feed_open => match changed {
                    Ok(()) => {
                        self.feed_changed();
                        self.release_echoes().await;
                    }
                    Err(_) => {
                        self.feed_open = false;
                        self.end_climb("the device's actor stopped");
                    }
                },
                () = tokio::time::sleep_until(self.due), if self.climb.is_some() => {
                    self.climb_tick().await;
                }
                () = tokio::time::sleep_until(echo_due), if !self.echoing.is_empty() => {
                    self.release_echoes().await;
                }
            }
            self.show_climb();
        }
        self.end_climb("the device's writer stopped");
        self.show_climb();
        let (writes, picks) = self.slots.drain();
        for pending in writes {
            pending.answer.send(Outcome::Cancelled(STOPPED));
        }
        for pick in picks {
            let _ = pick.done.send(Outcome::Cancelled(STOPPED));
        }
    }

    async fn ask(&mut self, ask: Ask) {
        match ask {
            Ask::Submit {
                origin,
                command,
                options,
                done,
            } => {
                if self.ends_climb(&command) {
                    self.end_climb("another write");
                }
                self.arrived += 1;
                if matches!(command, Command::EqMode { .. }) {
                    self.mode_or_recall = self.arrived;
                }
                let switch = (options.flat_to_eq && matches!(command, Command::EqSet { .. }))
                    .then_some((origin, self.arrived));
                let pending = Pending {
                    origin,
                    command,
                    answer: Answer::Write(done),
                    relative: None,
                    switch,
                };
                self.submit(pending, options.mode).await;
            }
            Ask::Glide {
                origin,
                key,
                raw,
                mode,
                glided,
            } => self.glide(origin, key, raw, mode, glided).await,
            Ask::Recalled(name) => {
                self.recalled.send_replace(Some(name));
            }
            Ask::Recall {
                origin,
                name,
                fitting,
                planned,
                done,
            } => {
                self.arrived += 1;
                self.mode_or_recall = self.arrived;
                let loudness = self
                    .feed
                    .borrow()
                    .profile
                    .as_ref()
                    .and_then(|profile| profile.eq.as_ref())
                    .map(|eq| eq.loudness_id());
                // A recall writes every band parameter and the mode, and the loudness strength
                // only for a preset in Loudness mode, so a held strength survives any other.
                let writes_strength = preset::find(&fitting, &name)
                    .is_some_and(|found| found.mode == EqMode::Loudness);
                let said = name.clone();
                let pick = Pick {
                    origin,
                    recipe: Recipe::Preset { name, fitting },
                    planned,
                    done,
                };
                let actions = self
                    .slots
                    .pick(pick, |id| !writes_strength && Some(id) == loudness);
                let dropped = actions
                    .iter()
                    .filter(|action| matches!(action, Action::Supersede(_)))
                    .count();
                if dropped > 0 {
                    log::info(&format!(
                        "{}: preset {said} supersedes {dropped} held tone change(s)",
                        self.name()
                    ));
                }
                self.act(actions).await;
            }
            Ask::Apply {
                origin,
                name,
                planned,
                done,
            } => {
                self.arrived += 1;
                let profile = self.feed.borrow().profile.clone();
                let said = name.clone();
                let pick = Pick {
                    origin,
                    recipe: Recipe::Setting { name },
                    planned,
                    done,
                };
                // A setting rewrites its own bands alone: every other write held is kept.
                let actions = self.slots.pick(pick, |id| {
                    profile
                        .as_deref()
                        .is_some_and(|profile| !device_presets::rewrites(profile, id))
                });
                let dropped = actions
                    .iter()
                    .filter(|action| matches!(action, Action::Supersede(_)))
                    .count();
                if dropped > 0 {
                    log::info(&format!(
                        "{}: setting {said} supersedes {dropped} held band change(s)",
                        self.name()
                    ));
                }
                self.act(actions).await;
            }
            #[cfg(test)]
            Ask::View(reply) => {
                let _ = reply.send(self.view());
            }
        }
    }

    /// One write answered: the climb starts from a glide's first step, every other waiter is
    /// told, and the next write on the function goes.
    async fn answered(
        &mut self,
        id: Option<FunctionId>,
        asked: Option<i64>,
        answer: Answer,
        response: Response,
        switch: Option<(Origin, u64)>,
        echo: Option<Echo>,
    ) {
        let answered_with_value = matches!(&response, Response::Value(_));
        let landing = Landing::of(&response);
        if let Some(id) = id {
            self.settle_asked(id, asked, &response);
            self.settle_sent(id, asked, &response);
        }
        match answer {
            Answer::First(start) => self.first_answered(id, start, response),
            Answer::Step { step, epoch, .. } => {
                let echoed =
                    matches!(&response, Response::Value(vo) if vo.raw.as_i64() == Some(step));
                if !echoed && epoch == self.epoch {
                    self.end_climb("a step answered with another value");
                }
            }
            answer => answer.send(Outcome::Done(response)),
        }
        if let Some(id) = id {
            // The echo is noted before the line moves on, so a recall behind the write sees it.
            if let Some(echo) = echo {
                self.note_echo(id, &echo, landing, Instant::now());
            }
            let actions = self.slots.answered(id);
            self.act(actions).await;
        }
        if let Some((origin, number)) = switch
            && answered_with_value
        {
            self.switch_to_eq(origin, number).await;
        }
    }

    /// After a band write that arrived as `number` from `origin` went through: write the mode EQ,
    /// ordered, from the same origin, when the tone control reads Flat, unless a mode write or a
    /// recall arrived after the band (it says what the user wants last, and a recall writes its
    /// own mode) or a mode write is in flight, waiting or held (it decides the mode). Several
    /// bands in Flat earn one switch: the first one's is a mode write the others see.
    async fn switch_to_eq(&mut self, origin: Origin, number: u64) {
        let mode_id = {
            let feed = self.feed.borrow();
            let Some(eq) = feed
                .profile
                .as_ref()
                .and_then(|profile| profile.eq.as_ref())
            else {
                return;
            };
            let mode_id = eq.mode_id();
            let flat = reading_of(&feed.readings, mode_id)
                .and_then(|raw| u8::try_from(raw).ok())
                .and_then(EqMode::from_raw)
                == Some(EqMode::Flat);
            if !flat {
                return;
            }
            mode_id
        };
        if self.mode_or_recall > number || self.slots.pending(mode_id) || self.slots.holds(mode_id)
        {
            return;
        }
        log::info(&format!(
            "{}: {}: switching to EQ",
            origin.word(),
            self.name()
        ));
        self.arrived += 1;
        self.mode_or_recall = self.arrived;
        // Nobody waits for the switch, but a refusal or a failure is logged.
        let (done, outcome) = oneshot::channel();
        let who = format!("{}: {}", origin.word(), self.name());
        tokio::spawn(async move {
            match outcome.await {
                Ok(Outcome::Done(Response::Error { message, .. })) => {
                    log::warn(&format!(
                        "{who}: the switch to EQ was not written: {message}"
                    ));
                }
                Ok(Outcome::Cancelled(why)) => {
                    log::warn(&format!("{who}: the switch to EQ was not written: {why}"));
                }
                _ => {}
            }
        });
        let pending = Pending {
            origin,
            command: Command::EqMode {
                mode: EqMode::Eq.name().to_string(),
                no_wait: false,
            },
            answer: Answer::Write(done),
            relative: None,
            switch: None,
        };
        self.submit(pending, Mode::Ordered).await;
    }

    /// A write that asked function `id` for step `asked` was answered. While that is still the
    /// step last asked of the function, a refusal forgets it, and so does an echo: the actor
    /// publishes the reading before it answers, so the reading shows where the device is — the
    /// step, or the knob's value the session took for it. An `Ack`, a write sent without waiting
    /// whose echo is still on its way, keeps it, and so does a timeout: the write may have landed
    /// with no echo to show it (see [`timed_out`]). A step a later write asked for since is that
    /// write's to settle.
    fn settle_asked(&mut self, id: FunctionId, asked: Option<i64>, response: &Response) {
        let still = asked.is_some() && self.asked.get(&id).map(|(raw, _, _)| *raw) == asked;
        if still && !matches!(response, Response::Ack) && !timed_out(response) {
            self.asked.remove(&id);
            self.stepped.remove(&id);
        }
    }

    /// A write that handed function `id` step `sent` to the actor was answered: the same rule as
    /// [`Task::settle_asked`], for the last step sent.
    fn settle_sent(&mut self, id: FunctionId, sent: Option<i64>, response: &Response) {
        let still = sent.is_some() && self.sent.get(&id).map(|(raw, _, _)| *raw) == sent;
        if still && !matches!(response, Response::Ack) && !timed_out(response) {
            self.sent.remove(&id);
        }
    }

    /// A glide's first step was answered. The climb starts only from an echo of the step it
    /// asked for, and only while nothing has ended a climb since the step went out (a mute, a
    /// power-off, another write to the volume, a lost link or a changed profile moves
    /// [`Task::epoch`]); the glide is then told it is on. A step answered with another value — the
    /// knob turned while it waited — or after the epoch moved lands where it is and starts
    /// nothing; a refused step is the glide's answer.
    fn first_answered(&mut self, id: Option<FunctionId>, start: Start, response: Response) {
        let Start {
            glided,
            owner,
            target,
            step,
            epoch,
        } = start;
        let current = epoch == self.epoch;
        if current {
            self.starting = None;
        }
        let echo = match &response {
            Response::Value(vo) => vo.raw.as_i64(),
            _ => None,
        };
        if echo.is_none() {
            let _ = glided.send(Glided::Refused(response));
            return;
        }
        // A current epoch means nothing has started or ended a climb since the step went out.
        let climbing = id
            .filter(|_| current && echo == Some(step))
            .and_then(|id| self.control_at(id))
            .and_then(|control| Climbing::new(owner, &control, target, step, step));
        match climbing {
            Some(climbing) => {
                self.climb = Some(climbing);
                self.due = Instant::now();
                let _ = glided.send(Glided::Climbing {
                    first: Some(response),
                    target,
                });
            }
            None => {
                let _ = glided.send(Glided::Landed(response));
            }
        }
    }

    /// Judge setting the level `key` to `raw` for `origin` by [`Rules::writer`], and land it,
    /// start a climb, or steer `origin`'s own.
    async fn glide(
        &mut self,
        origin: Origin,
        key: String,
        raw: i64,
        mode: Mode,
        glided: oneshot::Sender<Glided>,
    ) {
        let Some(control) = self
            .control_named(&key)
            .filter(|control| control.kind == ControlKind::Level)
        else {
            let _ = glided.send(Glided::Refused(Response::Error {
                code: 1,
                message: format!("{key} is not a level of {}", self.name()),
            }));
            return;
        };
        let reading = self.reading(control.id);
        let situation = Situation {
            reading,
            base: self.base(control.id, reading),
            floor_db: None,
            asked_db: None,
        };
        let motion = safety::judge(&control, &situation, raw, &Rules::writer()).motion;
        let owned = self
            .climb
            .as_ref()
            .is_some_and(|climbing| climbing.id == control.id && climbing.owner == origin);
        let silenced = silent(&self.feed.borrow());
        match motion {
            // A device that reads itself muted or off is not climbed on: the step that may land at
            // once is written, as any other write, and a glide with none is answered as it is.
            Motion::Glide {
                first: Some(step), ..
            } if silenced => {
                self.end_climb("a glide on a device that reports itself muted or off");
                let pending = Pending {
                    origin,
                    command: set_to(&control.key, step),
                    answer: Answer::Landed(glided),
                    relative: None,
                    switch: None,
                };
                self.submit(pending, mode).await;
            }
            Motion::Glide { first: None, .. } if silenced => {
                self.end_climb("a glide on a device that reports itself muted or off");
                let _ = glided.send(match self.current(control.id) {
                    Some(response) => Glided::Landed(response),
                    None => Glided::Refused(no_reading(&key)),
                });
            }
            Motion::Glide { target, .. } if owned => {
                if let Some(climbing) = self.climb.as_mut() {
                    climbing.climb.steer(target);
                }
                let _ = glided.send(Glided::Climbing {
                    first: None,
                    target,
                });
            }
            Motion::Glide {
                first: Some(step),
                target,
            } => {
                self.end_climb("a new glide");
                // Until its echo is back, what ends a climb ends this one before it starts.
                self.starting = Some(Starting {
                    id: control.id,
                    shows: self.shows(control.id, reading, step),
                });
                let pending = Pending {
                    origin,
                    command: set_to(&control.key, step),
                    answer: Answer::First(Start {
                        glided,
                        owner: origin,
                        target,
                        step,
                        epoch: self.epoch,
                    }),
                    relative: None,
                    switch: None,
                };
                self.submit(pending, mode).await;
            }
            Motion::Glide {
                first: None,
                target,
            } => {
                self.end_climb("a new glide");
                let climbing = self.planned(control.id).and_then(|written| {
                    let before = self.reading(control.id).unwrap_or(written);
                    Climbing::new(origin, &control, target, written, before)
                });
                match climbing {
                    Some(climbing) => {
                        self.climb = Some(climbing);
                        self.due = Instant::now();
                        let _ = glided.send(Glided::Climbing {
                            first: None,
                            target,
                        });
                    }
                    None => {
                        let _ = glided.send(Glided::Refused(no_reading(&key)));
                    }
                }
            }
            Motion::Now(target) => {
                if self.climbing_id() == Some(control.id) {
                    self.end_climb("a glide that lands at once");
                }
                let pending = Pending {
                    origin,
                    command: set_to(&control.key, target),
                    answer: Answer::Landed(glided),
                    relative: None,
                    switch: None,
                };
                self.submit(pending, mode).await;
            }
            Motion::Nothing => {
                let _ = glided.send(Glided::Refused(no_reading(&key)));
            }
        }
    }

    /// Pay the climb the time that passed and take its next step, if the device has answered
    /// the last one and the credit pays for it.
    async fn climb_tick(&mut self) {
        let now = Instant::now();
        self.due = now + CLIMB_TICK;
        let Some(mut climbing) = self.climb.take() else {
            return;
        };
        let (lo, hi) = climbing.range;
        climbing
            .climb
            .advance(now.saturating_duration_since(climbing.at), hi - lo);
        climbing.at = now;
        if silent(&self.feed.borrow()) {
            log::debug(&format!(
                "{}: the volume's climb ended: the device reports itself muted or off",
                self.name()
            ));
            self.end_climb("the device reports itself muted or off");
            return;
        }
        let reading = self.reading(climbing.id);
        let base = self.base(climbing.id, reading);
        let pending = self.slots.pending(climbing.id);
        if pending && climbing.disowns(reading) {
            log::debug(&format!(
                "{}: the volume's climb ended: a reading it did not write",
                self.name()
            ));
            self.end_climb("a reading it did not write");
            return;
        }
        match climbing
            .climb
            .next(reading, base, pending, safety::safe_step(climbing.range))
        {
            Step::Wait => self.climb = Some(climbing),
            Step::Done => log::debug(&format!("{}: the volume's climb is done", self.name())),
            Step::Broken => log::debug(&format!(
                "{}: the volume's climb ended: a reading it did not write",
                self.name()
            )),
            Step::Write(step) => {
                climbing.before = reading.unwrap_or(step);
                let pending = Pending {
                    origin: climbing.owner,
                    command: set_to(&climbing.key, step),
                    answer: Answer::Step {
                        step,
                        epoch: self.epoch,
                        before: climbing.before,
                    },
                    relative: None,
                    switch: None,
                };
                if !climbing.climb.reached() {
                    self.climb = Some(climbing);
                }
                self.submit(pending, Mode::Latest).await;
            }
        }
    }

    /// The feed changed: forget what a reading now shows, and end the climb on a lost link or a
    /// profile that changed.
    fn feed_changed(&mut self) {
        let feed = self.feed.borrow();
        let now = Instant::now();
        for steps in [&mut self.asked, &mut self.sent] {
            steps.retain(|id, (raw, at, _)| {
                reading_of(&feed.readings, *id) != Some(*raw)
                    && now.saturating_duration_since(*at) < ASKED_LIFETIME
            });
        }
        self.stepped.retain(|id, _| self.asked.contains_key(id));
        self.unsure.retain(|id, mark| match mark {
            Unsure::Until(was) => feed.heard_at.get(id).copied() == *was,
            Unsure::Recall => true,
        });
        let connected = feed.connected;
        let profile = feed.profile.clone();
        let silenced = silent(&feed);
        let strange = self.starting_strangely(&feed.readings);
        drop(feed);
        if strange {
            self.end_climb("a reading it did not write");
        }
        if silenced {
            self.end_climb("the device reports itself muted or off");
        }
        if !connected {
            self.end_climb("the link is down");
        }
        let changed = match (&self.profile, &profile) {
            (Some(was), Some(now)) => !Arc::ptr_eq(was, now) && **was != **now,
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.end_climb("the profile changed");
        }
        self.profile = profile;
    }

    /// Whether the volume reads, in `readings`, something a glide's first step on its way did not
    /// write: a value it may not show ([`Starting::shows`]).
    fn starting_strangely(&self, readings: &[Message]) -> bool {
        self.starting.as_ref().is_some_and(|starting| {
            reading_of(readings, starting.id)
                .is_some_and(|reading| !starting.shows.contains(&reading))
        })
    }

    /// Whether the reading now is a change that the climb the step `pending` on `id` belongs to
    /// did not write: the check [`Task::feed_changed`] and [`Task::climb_tick`] make as the feed
    /// changes and the climb is paid, made again as the step is sent, for the order in which the
    /// writer's task takes an answer and the change that comes with it, and for a climb's last
    /// step, whose climb is over when it is sent.
    fn knob_turned(&self, id: FunctionId, pending: &Pending) -> bool {
        match &pending.answer {
            Answer::First(Start { epoch, .. }) if *epoch == self.epoch => {
                self.starting_strangely(&self.feed.borrow().readings)
            }
            Answer::Step {
                step,
                epoch,
                before,
            } if *epoch == self.epoch => unwritten(self.reading(id), *before, *step),
            _ => false,
        }
    }

    /// Whether `command` ends the climb — the one on, or the one a glide's first step is about
    /// to start: a write to its function, whoever sends it, or a mute set on or a power set off
    /// on the device.
    fn ends_climb(&self, command: &Command) -> bool {
        let Some(climbing) = self.climbing_id() else {
            return false;
        };
        let feed = self.feed.borrow();
        let Some(profile) = feed.profile.as_deref() else {
            return false;
        };
        key_of(profile, command).is_some_and(|(id, _)| id == climbing) || silences(profile, command)
    }

    /// The function the climb is on, or the one a glide's first step is about to start a climb
    /// on.
    fn climbing_id(&self) -> Option<FunctionId> {
        self.climb
            .as_ref()
            .map(|climbing| climbing.id)
            .or(self.starting.as_ref().map(|starting| starting.id))
    }

    /// End the climb, if one is on, saying why in the log, and keep a glide's first step on its
    /// way from starting one: the epoch moves.
    fn end_climb(&mut self, why: &str) {
        self.epoch += 1;
        self.starting = None;
        if self.climb.take().is_some() {
            log::debug(&format!("{}: the volume's climb ended: {why}", self.name()));
        }
    }

    /// Function `id`'s raw reading.
    fn reading(&self, id: FunctionId) -> Option<i64> {
        reading_of(&self.feed.borrow().readings, id)
    }

    /// Where function `id` is heading: the step last asked of it while no reading has shown it,
    /// unless that is a glide's first step or a climb's step that waits in the line and will never
    /// be sent, else its reading.
    fn planned(&self, id: FunctionId) -> Option<i64> {
        self.fresh_asked(id)
            .filter(|_| {
                self.stepped
                    .get(&id)
                    .is_none_or(|epoch| *epoch == self.epoch)
            })
            .or_else(|| self.reading(id))
    }

    /// What the readings of function `id` may show while a glide's first step, `step`, is on its
    /// way: the `reading`, where the function is heading, the step itself, the step last handed to
    /// the actor and the step of every write that waits for it and will be sent, whose echoes can
    /// come first.
    fn shows(&self, id: FunctionId, reading: Option<i64>, step: i64) -> Vec<i64> {
        let waiting = self
            .slots
            .waiting_for(id)
            .filter(|pending| !self.outlived(pending))
            .filter_map(|pending| self.asked_of(id, &pending.command));
        [reading, self.planned(id), Some(step)]
            .into_iter()
            .flatten()
            .chain(self.fresh_sent(id).map(|(raw, _)| raw))
            .chain(waiting)
            .collect()
    }

    /// The lowest of `reading`, the step last asked of `id` and the step last handed to the actor
    /// on it, those two while no reading has shown them: what a raise is judged from, so a
    /// lowering whose echo was lost, or is still on its way, is never undone.
    fn base(&self, id: FunctionId, reading: Option<i64>) -> Option<i64> {
        reading.map(|reading| {
            [
                self.fresh_asked(id),
                self.fresh_sent(id).map(|(raw, _)| raw),
            ]
            .into_iter()
            .flatten()
            .fold(reading, i64::min)
        })
    }

    /// The step last asked of `id`, while it is younger than [`ASKED_LIFETIME`].
    fn fresh_asked(&self, id: FunctionId) -> Option<i64> {
        self.asked
            .get(&id)
            .filter(|(_, at, _)| at.elapsed() < ASKED_LIFETIME)
            .map(|(raw, _, _)| *raw)
    }

    /// The step last handed to the actor on `id`, with whether a relative step worked it out,
    /// while it is younger than [`ASKED_LIFETIME`].
    fn fresh_sent(&self, id: FunctionId) -> Option<(i64, bool)> {
        self.sent
            .get(&id)
            .filter(|(_, at, _)| at.elapsed() < ASKED_LIFETIME)
            .map(|(raw, _, chain)| (*raw, *chain))
    }

    /// Whether the step last asked of `id` was worked out from a relative step.
    fn chained(&self, id: FunctionId) -> bool {
        self.asked.get(&id).is_some_and(|(_, _, chain)| *chain)
    }

    /// The control at `id`, as the profile has it now.
    fn control_at(&self, id: FunctionId) -> Option<Control> {
        self.feed
            .borrow()
            .profile
            .as_deref()?
            .control_by_id(id)
            .cloned()
    }

    /// The control named `key`, as the profile has it now.
    fn control_named(&self, key: &str) -> Option<Control> {
        self.feed
            .borrow()
            .profile
            .as_deref()?
            .control_by_key(key)
            .cloned()
    }

    /// Key `pending` on the function it writes and put it in the slots, a relative step worked
    /// out first ([`Task::resolved`]). A write that names no function the profile has goes to
    /// the actor as it is, which answers it in its own words.
    async fn submit(&mut self, pending: Pending, mode: Mode) {
        let relative = relative(&pending.command);
        let pending = self.resolved(pending);
        let keyed = self
            .feed
            .borrow()
            .profile
            .as_deref()
            .and_then(|profile| key_of(profile, &pending.command));
        let Some((id, held)) = keyed else {
            self.send(None, pending).await;
            return;
        };
        if let Some(raw) = self.asked_of(id, &pending.command) {
            self.asked.insert(id, (raw, Instant::now(), relative));
            match &pending.answer {
                Answer::First(Start { epoch, .. }) | Answer::Step { epoch, .. } => {
                    self.stepped.insert(id, *epoch);
                }
                _ => {
                    self.stepped.remove(&id);
                }
            }
        }
        // A relative step is worked out from where its function is heading, so none may be
        // dropped.
        let mode = if relative { Mode::Ordered } else { mode };
        // A recall of a device slot goes out only once the echoes of the band writes before it
        // are in, so a report that predates it is never taken for one that follows it.
        let echoes = self.recall_changes(id);
        let barrier = self.recalls_bands(id);
        let actions = self.slots.submit(
            id,
            Queued {
                mode,
                held,
                barrier,
                echoes,
                item: pending,
            },
        );
        self.act(actions).await;
    }

    /// The raw step `command` asks of function `id`, when it names one inside the function's
    /// range: a `set` is read as the actor reads it, so `min` and `-40dB` name steps as `50`
    /// does, and a write the device can only refuse is never where a step starts.
    fn asked_of(&self, id: FunctionId, command: &Command) -> Option<i64> {
        let feed = self.feed.borrow();
        let profile = feed.profile.as_deref()?;
        let raw = match command {
            Command::Set { key, value, .. } => intent(profile, key, value).ok()?.value.as_i64()?,
            Command::Raw { value, .. } => value.trim().parse().ok()?,
            _ => return None,
        };
        let (lo, hi) = profile.control_by_id(id)?.range?;
        (lo..=hi).contains(&raw).then_some(raw)
    }

    /// `pending` with a relative step worked out to an absolute one, so two steps asked for
    /// before the first's echo move two steps. It starts from the step last asked of the
    /// function while that is below the reading (a lowering on its way: never louder) or, for a
    /// chain of quick relative steps, held at two steps above the reading at most, so the chain
    /// stalls there rather than falling back. A lowering is never above the reading less its
    /// step, whatever was asked. An absolute step more than two steps above the reading, and
    /// none asked, start it from the reading, so a write the device refused,
    /// dropped or answered with another value never moves where a step starts. A level with no
    /// reading yet keeps its relative step, for the actor to refuse in its own words. The step
    /// keeps its delta and arrival step for [`Task::refreshed`], which works it out again as it
    /// goes out. The step is worked out by [`roles::next_step`].
    fn resolved(&self, pending: Pending) -> Pending {
        let Command::Set {
            key,
            value,
            step,
            no_wait,
        } = &pending.command
        else {
            return pending;
        };
        let delta = match value.to_ascii_lowercase().as_str() {
            "up" => *step,
            "down" => step.saturating_neg(),
            _ => return pending,
        };
        let found = self
            .feed
            .borrow()
            .profile
            .as_deref()
            .and_then(|profile| profile.control_by_key(key))
            .filter(|control| control.kind == ControlKind::Level)
            .and_then(|control| Some((control.id, control.range?)));
        let Some((id, (lo, hi))) = found else {
            return pending;
        };
        let Some(reading) = self.reading(id) else {
            return pending;
        };
        let last = self.fresh_asked(id).map(|raw| (raw, self.chained(id)));
        let to = roles::next_step(last, reading, delta, (lo, hi));
        let command = Command::Set {
            key: key.clone(),
            value: to.to_string(),
            step: *step,
            no_wait: *no_wait,
        };
        let relative = Some(Relative {
            delta,
            to,
            range: (lo, hi),
        });
        Pending {
            command,
            relative,
            ..pending
        }
    }

    /// `pending` as it goes out: a relative step worked out again from the reading now and the
    /// step last handed to the actor, by the rule [`Task::resolved`] uses ([`roles::next_step`]),
    /// so a knob turned while it waited is stepped from, not undone, and a step asked for that
    /// never went out is not stepped from. The step sent is never louder than the step it was
    /// worked out to when it arrived, whichever its sign, which is what holds a chain of quick
    /// steps where it stalls and a lowering on its way where it is.
    fn refreshed(&mut self, id: FunctionId, pending: Pending) -> Pending {
        let Some(Relative {
            delta,
            to,
            range: (lo, hi),
        }) = pending.relative
        else {
            return pending;
        };
        let Some(reading) = self.reading(id) else {
            return pending;
        };
        let last = self.fresh_sent(id);
        let step = roles::next_step(last, reading, delta, (lo, hi)).min(to);
        let Command::Set {
            key,
            step: size,
            no_wait,
            ..
        } = &pending.command
        else {
            return pending;
        };
        if step == to {
            return pending;
        }
        if let Some(asked) = self.asked.get_mut(&id)
            && asked.0 == to
        {
            asked.0 = step;
        }
        let command = Command::Set {
            key: key.clone(),
            value: step.to_string(),
            step: *size,
            no_wait: *no_wait,
        };
        Pending { command, ..pending }
    }

    async fn act(&mut self, actions: Vec<Action<Pending, Pick>>) {
        let mut todo: VecDeque<_> = actions.into();
        while let Some(action) = todo.pop_front() {
            if let Action::Send(id, pending) = &action
                && self.knob_turned(*id, pending)
            {
                self.end_climb("a reading it did not write");
            }
            match action {
                Action::Send(id, pending) if self.outlived(&pending) => {
                    self.settle_asked(
                        id,
                        self.asked_of(id, &pending.command),
                        &Response::Error {
                            code: 3,
                            message: "the climb ended".to_string(),
                        },
                    );
                    let ended = match self.current(id) {
                        Some(response) => Outcome::Done(response),
                        None => Outcome::Cancelled("the climb ended"),
                    };
                    pending.answer.send(ended);
                    for released in self.slots.answered(id).into_iter().rev() {
                        todo.push_front(released);
                    }
                }
                Action::Send(id, pending) => {
                    let pending = self.refreshed(id, pending);
                    self.send(Some(id), pending).await;
                }
                Action::Supersede(pending) => {
                    log::debug(&format!(
                        "{}: a {} write superseded by a newer one",
                        self.name(),
                        pending.origin.word()
                    ));
                    pending.answer.send(Outcome::Superseded);
                }
                Action::Start(pick) => self.start(pick),
                Action::Replace(pick) => {
                    log::debug(&format!(
                        "{}: a {} recall of {} replaced by a later one",
                        self.name(),
                        pick.origin.word(),
                        pick.recipe.name()
                    ));
                    let _ = pick.done.send(Outcome::Superseded);
                }
            }
        }
    }

    /// Whether `pending` is a climb's step, or a glide's first step, from a climb that has ended
    /// since: it is not sent, so a step that waited cannot override a write or a knob turn that
    /// ended its climb.
    fn outlived(&self, pending: &Pending) -> bool {
        match &pending.answer {
            Answer::Step { epoch, .. } | Answer::First(Start { epoch, .. }) => *epoch != self.epoch,
            _ => false,
        }
    }

    /// Function `id`'s reading as the actor would answer a write of it.
    fn current(&self, id: FunctionId) -> Option<Response> {
        let feed = self.feed.borrow();
        let status = feed.readings.iter().find(|m| m.id == id)?;
        let reading = describe(feed.profile.as_deref()?, status);
        let mut shown = ValueObject::from_reading(&reading, status);
        shown.heard_at_ms = feed.heard_at.get(&id).copied();
        Some(Response::Value(shown))
    }

    /// Hand `pending` to the actor, in the order of these calls, and wait for the answer on
    /// [`Task::replies`]: the actor's, or why there is none.
    async fn send(&mut self, id: Option<FunctionId>, pending: Pending) {
        let relative = pending.relative.is_some();
        let Pending {
            command,
            answer,
            switch,
            ..
        } = pending;
        if let Some(id) = id {
            self.note_recall(id);
        }
        let no_wait = matches!(
            &command,
            Command::Set { no_wait: true, .. } | Command::Raw { no_wait: true, .. }
        );
        let echo = id.and_then(|id| {
            let repeated = self.outstanding(id)?;
            Some(Echo {
                was: self.feed.borrow().heard_at.get(&id).copied(),
                no_wait,
                repeated,
            })
        });
        let device = self.name();
        let asked = id.and_then(|id| self.asked_of(id, &command));
        if let Some((id, raw)) = id.zip(asked) {
            self.sent.insert(id, (raw, Instant::now(), relative));
            // A glide's first step or a climb's step handed to the actor goes to the device
            // whatever the epoch does: it is where its function is heading again.
            let stepped = match &answer {
                Answer::First(Start { epoch, .. }) | Answer::Step { epoch, .. } => Some(*epoch),
                _ => None,
            };
            if stepped.is_some()
                && self.stepped.get(&id).copied() == stepped
                && self.fresh_asked(id) == Some(raw)
            {
                self.stepped.remove(&id);
            }
        }
        let (reply, answered) = oneshot::channel();
        let handed =
            tokio::time::timeout(WRITE_DEADLINE, self.commands.send((command, reply))).await;
        let failed = match handed {
            Ok(Ok(())) => None,
            Ok(Err(_)) => Some(not_running(&device)),
            Err(_) => Some(not_taken(&device)),
        };
        self.replies.push(Box::pin(async move {
            let response = match failed {
                Some(failed) => failed,
                None => wait(&device, answered).await,
            };
            (id, asked, answer, response, switch, echo)
        }));
    }

    /// Whether function `id` is a control a recall of a device slot changes.
    fn feeds_recall(&self, id: FunctionId) -> bool {
        let feed = self.feed.borrow();
        feed.profile
            .as_deref()
            .is_some_and(|profile| changed_by_recall(profile, id))
    }

    /// For a write to function `id` going out, when it is a band a recall of a device slot
    /// changes: whether an earlier write's echo is still outstanding on it, awaited or past its
    /// deadline. Either makes the first echo ambiguous, so from here on only a deadline ends the
    /// wait for an awaited one. `None` for any other function.
    fn outstanding(&mut self, id: FunctionId) -> Option<bool> {
        if !self.feeds_recall(id) {
            return None;
        }
        let awaited = self.echoing.get_mut(&id).is_some_and(|awaiting| {
            awaiting.repeated = true;
            true
        });
        Some(awaited || self.unsure.contains_key(&id))
    }

    /// Whether the feed has heard function `id` since it last had the stamp `was`.
    fn heard_since(&self, id: FunctionId, was: Option<u64>) -> bool {
        self.feed.borrow().heard_at.get(&id).copied() != was
    }

    /// When the first band still awaited for its echo stops being waited for.
    fn echo_due(&self) -> Instant {
        self.echoing
            .values()
            .map(|awaiting| awaiting.due)
            .min()
            .unwrap_or_else(Instant::now)
    }

    /// A write to band `id` that `echo` describes was answered at `at` as `landing` says. A band
    /// written without waiting and answered `Ack` is awaited for its echo, unless the feed has
    /// heard it since the write went out. A band written while an earlier echo was outstanding,
    /// waited or not, is awaited until [`ECHO_WAIT`] after this answer whatever the feed hears,
    /// since the first echo cannot be told from the second: so is one whose earlier entry has
    /// expired since the write went out, as the earlier echo may be what answered it. A waited
    /// write answered with the session's timeout, the feed not having heard the band since, was
    /// waited for as long as an echo is awaited: the band is unsure, as at a missed deadline. A
    /// write that was refused or that never went out has no echo to come and leaves the record as
    /// it is, and so does a waited write answered with a reading, which is its echo.
    fn note_echo(&mut self, id: FunctionId, echo: &Echo, landing: Landing, at: Instant) {
        let due = at + ECHO_WAIT;
        let answered = match echo.no_wait {
            true => landing == Landing::Ack,
            false => matches!(landing, Landing::Value | Landing::TimedOut),
        };
        if let Some(awaiting) = self.echoing.get_mut(&id) {
            if answered {
                awaiting.due = awaiting.due.max(due);
            }
            return;
        }
        if !answered {
            return;
        }
        let heard = self.heard_since(id, echo.was);
        if echo.repeated || (echo.no_wait && !heard) {
            self.echoing.insert(
                id,
                Awaiting {
                    was: echo.was,
                    due,
                    repeated: echo.repeated,
                },
            );
            self.slots.await_echo(id);
        } else if landing == Landing::TimedOut && !heard {
            self.unsure.entry(id).or_insert(Unsure::Until(echo.was));
        }
    }

    /// Let go of every band whose echo has come or whose time is up, so the recalls behind it can
    /// go; a band whose echo did not come is noted as unsure.
    async fn release_echoes(&mut self) {
        let now = Instant::now();
        let released: Vec<(FunctionId, bool, bool)> = self
            .echoing
            .iter()
            .filter_map(|(id, awaiting)| {
                let heard = !awaiting.repeated && self.heard_since(*id, awaiting.was);
                (heard || awaiting.due <= now).then_some((*id, heard, awaiting.repeated))
            })
            .collect();
        for (id, heard, repeated) in released {
            self.echoing.remove(&id);
            if !heard {
                // A band written once is unsure until the feed hears it, as it stands now; a
                // band written again until the next recall, as its two echoes cannot be told
                // apart, and a mark of that kind is never weakened.
                let sticky = repeated || matches!(self.unsure.get(&id), Some(Unsure::Recall));
                let mark = match sticky {
                    true => Unsure::Recall,
                    false => Unsure::Until(self.feed.borrow().heard_at.get(&id).copied()),
                };
                self.unsure.insert(id, mark);
            }
            let actions = self.slots.echo_in(id);
            self.act(actions).await;
        }
    }

    /// Whether a write to function `id` is a recall of a device slot: an action that changes
    /// controls.
    fn recalls_bands(&self, id: FunctionId) -> bool {
        self.feed
            .borrow()
            .profile
            .as_deref()
            .and_then(|profile| profile.action_of(id))
            .is_some_and(|(action, _)| !action.changes.is_empty())
    }

    /// The controls a recall of a device slot at function `id` changes, whose awaited echoes it
    /// waits for; none for any other write.
    fn recall_changes(&self, id: FunctionId) -> Vec<FunctionId> {
        let feed = self.feed.borrow();
        let Some(profile) = feed.profile.as_deref() else {
            return Vec::new();
        };
        let Some((action, _)) = profile.action_of(id) else {
            return Vec::new();
        };
        action
            .changes
            .iter()
            .filter_map(|key| profile.control_by_key(key))
            .map(|control| control.id)
            .collect()
    }

    /// Note, when the write to function `id` is a recall of a device slot, when the controls it
    /// changes were last heard, so a report after it can be told from one before it.
    fn note_recall(&mut self, id: FunctionId) {
        let feed = self.feed.borrow();
        let Some(profile) = feed.profile.as_deref() else {
            return;
        };
        let Some((action, _)) = profile.action_of(id) else {
            return;
        };
        if action.changes.is_empty() {
            return;
        }
        let heard: Vec<_> = action
            .changes
            .iter()
            .filter_map(|key| profile.control_by_key(key))
            .map(|control| {
                let unsure = self.unsure.contains_key(&control.id);
                (control.id, feed.heard_at.get(&control.id).copied(), unsure)
            })
            .collect();
        drop(feed);
        self.after_recall = heard;
        self.unsure.clear();
    }

    /// Whether a recall of a device slot went out and control `id`, one it changes, has not been
    /// heard from since: its reading is from before the recall, and says nothing of what the
    /// device holds now.
    fn unreported(&self, feed: &Feed, id: FunctionId) -> bool {
        self.after_recall.iter().any(|(changed, was, unsure)| {
            *changed == id && (*unsure || feed.heard_at.get(&id).copied() == *was)
        })
    }

    /// Plan the recall or the built-in setting `pick` asks for from the device's readings now,
    /// answer its plan, and send its writes one after the other on [`Task::recalls`].
    fn start(&mut self, pick: Pick) {
        let Pick {
            recipe,
            planned,
            done,
            ..
        } = pick;
        let feed = self.feed.borrow().clone();
        let unseen = || "the device has not been seen yet".to_string();
        let plan = match (recipe, feed.profile.as_deref()) {
            (_, None) => Err(unseen()),
            (Recipe::Preset { name, fitting }, Some(profile)) => {
                recall::plan(&feed.name, profile, &fitting, &name, |id| {
                    reading_of(&feed.readings, id)
                })
                .map(|recalled| {
                    self.recalled
                        .send_replace(Some(recalled.preset.name.clone()));
                    let steps = recalled.recall.commands.iter().map(recall::command);
                    (steps.collect::<Vec<Command>>(), Planned::Plan(recalled))
                })
            }
            (Recipe::Setting { name }, Some(profile)) => {
                // A band that has not reported since a recall of a slot is planned as unread.
                recall::plan_setting(&feed.name, profile, &name, |id| {
                    reading_of(&feed.readings, id).filter(|_| !self.unreported(&feed, id))
                })
                .map(|applied| {
                    let steps = applied.writes.iter().map(recall::setting_command);
                    (steps.collect::<Vec<Command>>(), Planned::Setting(applied))
                })
            }
        };
        match plan {
            Ok((steps, plan)) => {
                let _ = planned.send(plan);
                // A step to a band a recall of a slot changes is tracked as a write is: whether
                // an echo is outstanding on it is judged now, as nothing else writes the band
                // while the batch runs.
                let steps: Vec<(Command, Option<(FunctionId, bool)>)> = steps
                    .into_iter()
                    .map(|command| {
                        let band = feed
                            .profile
                            .as_deref()
                            .and_then(|profile| key_of(profile, &command))
                            .and_then(|(id, _)| Some((id, self.outstanding(id)?)));
                        (command, band)
                    })
                    .collect();
                let commands = self.commands.clone();
                let watched = self.feed.clone();
                let device = feed.name;
                self.recalls.push(Box::pin(async move {
                    let (response, stepped) =
                        recall_steps(&commands, &device, &watched, steps).await;
                    (done, response, stepped)
                }));
            }
            Err(why) => {
                let _ = planned.send(Planned::Refused(why.clone()));
                self.recalls.push(Box::pin(async move {
                    (
                        done,
                        Response::Error {
                            code: 1,
                            message: why,
                        },
                        Vec::new(),
                    )
                }));
            }
        }
    }

    /// The device's configured name.
    fn name(&self) -> String {
        self.feed.borrow().name.clone()
    }

    #[cfg(test)]
    fn view(&self) -> View {
        View {
            in_flight: self.slots.flying(),
            held: self.slots.held(),
            recall_running: self.slots.running(),
            recall_waiting: self
                .slots
                .waiting()
                .map(|pick| pick.recipe.name().to_string()),
        }
    }
}

/// The function `command` writes, as the profile reads it, and whether a recall holds it; `None`
/// for a command that names none the profile has, and for anything but a write. A write to a
/// control's alternative id is keyed on the control's own, and a `set` of an action is keyed on
/// the command the slot makes.
fn key_of(profile: &Profile, command: &Command) -> Option<(FunctionId, bool)> {
    let id = match command {
        Command::Set { key, value, .. } => match profile.control_by_key(key) {
            Some(control) => control.id,
            None => intent(profile, key, value).ok()?.id,
        },
        Command::Raw { id, .. } => profile.read_as(id.parse().ok()?),
        Command::EqSet { band, param, .. } => {
            let param: EqParam = param.parse().ok()?;
            profile.eq.as_ref()?.band_id(band.checked_sub(1)?, param)?
        }
        Command::EqMode { .. } => profile.eq.as_ref()?.mode_id(),
        _ => return None,
    };
    Some((id, held_by_recall(profile, id)))
}

/// Whether a recall or a built-in setting holds writes to function `id` while it runs: the tone
/// control's functions, the bands a built-in setting sets and the ids it writes them to, the
/// controls an action's `changes` names, and the actions on a device's slots: a recall makes the
/// device set bands, and a save stores them.
fn held_by_recall(profile: &Profile, id: FunctionId) -> bool {
    profile.eq.as_ref().is_some_and(|eq| id.first() == eq.id)
        || device_presets::rewrites(profile, id)
        || changed_by_recall(profile, id)
        || profile.action_of(id).is_some()
}

/// Whether `id` is a control that an action's `changes` names, or an alternative id the profile
/// reads as one.
fn changed_by_recall(profile: &Profile, id: FunctionId) -> bool {
    profile
        .actions
        .iter()
        .flat_map(|action| &action.changes)
        .filter_map(|key| profile.control_by_key(key))
        .any(|control| {
            control.id == id
                || control
                    .reads
                    .iter()
                    .any(|entry| id.as_slice().starts_with(entry.as_slice()))
        })
}

/// A waited `set` of `key` to raw step `raw`.
fn set_to(key: &str, raw: i64) -> Command {
    Command::Set {
        key: key.to_string(),
        value: raw.to_string(),
        step: 1,
        no_wait: false,
    }
}

/// Whether `command` silences the device: a `mutes_output` toggle set on, or a
/// `powers_output` toggle set off.
fn silences(profile: &Profile, command: &Command) -> bool {
    let (control, value) = match command {
        Command::Set { key, value, .. } => {
            let Some(control) = profile.control_by_key(key) else {
                return false;
            };
            (control, intent(profile, key, value).ok().map(|m| m.value))
        }
        Command::Raw {
            id,
            value_type,
            value,
            ..
        } => {
            let Some(control) = id.parse().ok().and_then(|id| profile.control_by_id(id)) else {
                return false;
            };
            let value = value_type
                .parse::<ValueType>()
                .ok()
                .and_then(|value_type| Value::from_text(value_type, value).ok());
            (control, value)
        }
        _ => return false,
    };
    let Some(value) = value else {
        return false;
    };
    (control.mutes_output && control.reads_true(&value))
        || (control.powers_output && !control.reads_true(&value))
}

/// The answer for a level with no reading to judge a glide from.
fn no_reading(key: &str) -> Response {
    Response::Error {
        code: 4,
        message: format!("no current value for {key}"),
    }
}

/// Whether the device reads its output muted (a `mutes_output` toggle on) or off (a
/// `powers_output` toggle off).
fn silent(feed: &Feed) -> bool {
    feed.profile.as_deref().is_some_and(|profile| {
        profile.controls.iter().any(|control| {
            let Some(reading) = feed.readings.iter().find(|m| m.id == control.id) else {
                return false;
            };
            let on = control.reads_true(&reading.value);
            (control.mutes_output && on) || (control.powers_output && !on)
        })
    })
}

/// Whether `command` is a relative step, `up` or `down`.
fn relative(command: &Command) -> bool {
    matches!(command, Command::Set { value, .. }
        if value.eq_ignore_ascii_case("up") || value.eq_ignore_ascii_case("down"))
}

/// The raw reading of function `id` among `readings`.
fn reading_of(readings: &[Message], id: FunctionId) -> Option<i64> {
    readings
        .iter()
        .find(|m| m.id == id)
        .and_then(|m| m.value.as_i64())
}

/// Send a recall's commands to the actor in order, waiting for each, and stop at the first the
/// device or the actor refuses, answering with how many went through. Each step that names a
/// band (with whether an echo was outstanding on it) comes back with its answer, and with when
/// `feed` last heard the band as the step went out.
async fn recall_steps(
    commands: &Commands,
    device: &str,
    feed: &watch::Receiver<Feed>,
    steps: Vec<(Command, Option<(FunctionId, bool)>)>,
) -> (Response, Vec<Stepped>) {
    let total = steps.len();
    let mut stepped = Vec::new();
    for (sent, (command, band)) in steps.into_iter().enumerate() {
        let was = band.map(|(id, _)| feed.borrow().heard_at.get(&id).copied());
        let response = ask(commands, device, command).await;
        if let Some(((id, repeated), was)) = band.zip(was) {
            stepped.push(Stepped {
                id,
                echo: Echo {
                    was,
                    no_wait: false,
                    repeated,
                },
                landing: Landing::of(&response),
                at: Instant::now(),
            });
        }
        if let Response::Error { code, message } = response {
            let stopped = Response::Error {
                code,
                message: recall::stopped_after(sent, total, &message),
            };
            return (stopped, stepped);
        }
    }
    (Response::Ack, stepped)
}

/// Ask the actor to carry out `command`, waiting no longer than [`WRITE_DEADLINE`] either to hand
/// it over or for the answer.
async fn ask(commands: &Commands, device: &str, command: Command) -> Response {
    let (reply, answered) = oneshot::channel();
    match tokio::time::timeout(WRITE_DEADLINE, commands.send((command, reply))).await {
        Ok(Ok(())) => wait(device, answered).await,
        Ok(Err(_)) => not_running(device),
        Err(_) => not_taken(device),
    }
}

/// The actor's answer, or why there is none.
async fn wait(device: &str, answered: oneshot::Receiver<Response>) -> Response {
    match tokio::time::timeout(WRITE_DEADLINE, answered).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => not_running(device),
        Err(_) => unanswered(device),
    }
}

/// The answer for an actor that took a command and did not answer it within [`WRITE_DEADLINE`].
fn unanswered(device: &str) -> Response {
    Response::Error {
        code: 4,
        message: format!("device {device} did not answer within {WRITE_DEADLINE:?}"),
    }
}

/// Whether `response` says a write was sent and no echo came back in time, so the device may have
/// applied it: the actor's own timeout ([`svolder_ble::BleError::Timeout`], which the actor
/// answers with code 4 and its words) or this writer's [`unanswered`]. The other code 4
/// answers, a level with no reading and a command the actor never took, say it went nowhere.
fn timed_out(response: &Response) -> bool {
    let Response::Error { code: 4, message } = response else {
        return false;
    };
    message.starts_with("no confirmation within ") || message.contains(" did not answer within ")
}

/// The answer for a device whose actor has stopped.
pub(crate) fn not_running(device: &str) -> Response {
    Response::Error {
        code: 3,
        message: format!("device {device} is not running"),
    }
}

/// The answer for an actor that did not take a command within [`WRITE_DEADLINE`].
fn not_taken(device: &str) -> Response {
    Response::Error {
        code: 4,
        message: format!("device {device} did not take the command within {WRITE_DEADLINE:?}"),
    }
}

#[cfg(test)]
mod tests;
