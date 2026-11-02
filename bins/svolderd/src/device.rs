// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One tokio task per configured device, the actor: it owns that device's `Session` and its
//! readings, keeps the link up, publishes every status it reads, and runs the commands the
//! handler forwards to it.
//!
//! The actor also owns what its device is known by: the profile it converts values through, the
//! last advertisement seen, the passphrase entry and its verdict, the link history and the ring
//! of recent readings. [`spawn`] starts it and returns a [`DeviceHandle`], the one door to it;
//! the children are its parts:
//! - `answers`: the response codes and words an actor answers with.
//! - `attempt`: one discover-and-connect attempt, run in a task of its own.
//! - `commands`: answering one command.
//! - `entry`: adopting a changed config entry or profile registry.
//! - `history`: the day of link changes.
//! - `link`: keeping the link up, and waiting between attempts.
//! - `passphrase`: the code, its verdict and its refusals.
//! - `publish`: what the actor reports to the handle, the feed and the event stream.
//! - `ring`: the half hour of readings.

mod answers;
mod attempt;
mod commands;
mod entry;
mod history;
mod link;
mod passphrase;
mod publish;
mod ring;

#[cfg(test)]
pub(crate) use answers::error_for;
pub use answers::{Refused, code_for};

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use svolder_ble::{Advertisement, DeviceState, Episode, Session, Transport};
use svolder_ipc::config::DeviceConfig;
use svolder_ipc::{Event, PassphraseNote, Response};
use svolder_profiles::history::Window;
use svolder_profiles::{Profile, Registry};
use svolder_proto::{FunctionId, Message, UnitId};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::locks::locked;
use crate::log;
use crate::scanner::Scanner;
use crate::writer::Writer;
use history::History;
#[cfg(test)]
pub(crate) use history::TEST_CLOCK;
pub(crate) use history::wall_ms;

#[cfg(test)]
use answers::no_eq;
#[cfg(test)]
use attempt::{Attempt, Carried, connect_attempt, discover};
#[cfg(test)]
use commands::{build_raw, refuse};
#[cfg(test)]
use futures::StreamExt;
#[cfg(test)]
use history::HISTORY_CAP;
#[cfg(test)]
use link::AttemptEnd;
#[cfg(any(test, doc))]
use link::RETRY_SCHEDULE;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use svolder_ble::BleError;
#[cfg(test)]
use svolder_profiles::ProfileError;
#[cfg(test)]
use svolder_proto::{Passphrase, ProtoError, Value};

/// Commands queued for one device before a sender waits for room.
const COMMAND_QUEUE: usize = 32;
/// Events buffered for one subscriber before it is told it lagged.
const EVENT_QUEUE: usize = 256;

/// One thing the handler asks a device's actor to do.
pub enum Command {
    /// Every reading the device has confirmed.
    State,
    /// One control's confirmed reading.
    Get {
        /// The profile key to read.
        key: String,
    },
    /// Set one control from a literal value, or `up`/`down` by `step`.
    Set {
        /// The profile key to set.
        key: String,
        /// The value as the user wrote it.
        value: String,
        /// How far `up`/`down` moves a level control.
        step: i64,
        /// Send without waiting for the echoed status.
        no_wait: bool,
    },
    /// Set one function id directly.
    Raw {
        /// The dotted function id.
        id: String,
        /// The wire type its value carries.
        value_type: String,
        /// The value as the user wrote it.
        value: String,
        /// Send without waiting for the echoed status.
        no_wait: bool,
    },
    /// Every tone-control reading the device has confirmed.
    EqShow,
    /// Set one band's frequency, gain or Q. `band` is 1-based, as on the wire.
    EqSet {
        /// The 1-based band number.
        band: usize,
        /// `f`, `g` or `q`, spelled out or abbreviated.
        param: String,
        /// The value in Hz, dB or Q.
        value: f64,
        /// Send without waiting for the echoed status.
        no_wait: bool,
    },
    /// Select the tone-control mode.
    EqMode {
        /// `flat`, `eq` or `loudness`.
        mode: String,
        /// Send without waiting for the echoed status.
        no_wait: bool,
    },
    /// Change the device's passphrase: the one sanctioned path to function 250.
    Passphrase {
        /// The new passphrase's four digits.
        digits: String,
    },
    /// Ask the device to show itself (function 200), on the link the actor holds. Not on the
    /// `needs_link` list: a device that is away answers not connected without the wait ending.
    Identify,
    /// Adopt a freshly loaded profile registry and re-resolve this device's profile.
    Reload(Arc<Registry>),
    /// Adopt a changed entry for this device: the passphrase to encode with, the profile to
    /// name its controls through, and the labels its inputs carry. The unit id is the one
    /// thing this cannot change, since it is the device the actor is talking to.
    Configure(DeviceConfig),
    /// End the wait between connect attempts and try the device now (the web UI's Try now). A
    /// device that holds a link is left as it is.
    Wake,
    /// Close the session and stop the actor.
    Shutdown,
}

impl Command {
    /// Whether this command has to reach the device to mean anything. Reads answer from the
    /// last known state, and `Raw` carries its own function id, so neither is on the list.
    /// `Identify` is not on it either: it is a cue, not a change, so it never ends the wait
    /// between connect attempts; the actor refuses it itself while there is no link.
    fn needs_link(&self) -> bool {
        matches!(
            self,
            Command::Set { .. }
                | Command::EqSet { .. }
                | Command::EqMode { .. }
                | Command::Passphrase { .. }
        )
    }

    /// Whether this command is a reason to stop waiting between connect attempts and try the
    /// device now, against the entry the actor is running as `config`.
    ///
    /// True for a command that needs the link (the sender is at the device), or a changed
    /// config entry (passphrase, profile, input labels) since the next attempt uses it, or a
    /// `Wake`, which asks for exactly that.
    /// `Reload` alone, or a `Configure` that repeats the current entry, does not count — a
    /// bare profile reload would otherwise restart every waiting device's schedule.
    fn ends_the_wait(&self, config: &DeviceConfig) -> bool {
        self.needs_link()
            || matches!(self, Command::Wake)
            || matches!(self, Command::Configure(entry) if entry != config)
    }
}

/// What `devices` reports for one device, updated by its actor as the link changes.
#[derive(Debug, Clone)]
pub struct LiveInfo {
    /// The device's unit id.
    pub unit: String,
    /// The advertised product id, once one has been seen.
    pub product_id: u16,
    /// The advertised product name, once one has been seen.
    pub product: Option<String>,
    /// The advertised firmware build, once one has been seen.
    pub firmware: u16,
    /// The name of the profile the actor resolved for this device.
    pub profile: String,
    /// True while the actor holds a link.
    pub connected: bool,
    /// True when the readings are from before a disconnect.
    pub stale: bool,
    /// When the actor runs its next discover-and-connect attempt, while it is waiting
    /// between attempts. `None` while a link is up and while an attempt is running.
    pub next_attempt: Option<tokio::time::Instant>,
    /// How the configured passphrase disagrees with the advertisement the actor holds, while
    /// it does and the device has not shown the code works.
    pub passphrase_note: Option<PassphraseNote>,
    /// True while the link the actor holds has been refused its code and no changed code has
    /// lifted it.
    pub refused: bool,
    /// How strong the device's signal last was, where that was heard and when. Kept as it was
    /// when the link goes away, and replaced by the next advertisement that carries one.
    pub signal: Option<svolder_ipc::Signal>,
    /// Every change of the link the actor has made within the last day.
    pub history: Arc<Mutex<History>>,
    /// The last half hour of the device's readings.
    pub ring: Arc<Mutex<ring::Ring>>,
}

/// What the MQTT bridge reads one device through, kept by its actor: the name the file gives
/// it, its profile once the device has been seen, its firmware, its link, and every reading it
/// has confirmed. The bridge only ever reads this; what it writes goes through the command
/// channel, as every other request does.
#[derive(Debug, Clone, Default)]
pub struct Feed {
    /// The configured name.
    pub name: String,
    /// The unit id, as the sixteen hex characters the topics are built from.
    pub unit: String,
    /// The profile the actor names and converts values with. `None` until the device has been
    /// seen, since until then the actor holds a placeholder that describes nothing the device
    /// has.
    pub profile: Option<Arc<Profile>>,
    /// The advertised firmware build, once the device has been seen.
    pub firmware: Option<u16>,
    /// How strong the device's signal last was, where that was measured and when: what the
    /// bridge's signal sensor publishes. Kept as it was while the link is gone.
    pub signal: Option<svolder_ipc::Signal>,
    /// True while the actor holds a link.
    pub connected: bool,
    /// Every reading the device has confirmed, in function-id order: what `state` answers
    /// with.
    pub readings: Vec<Message>,
    /// When the actor last heard each reading from the device, in unix milliseconds: what a
    /// reading's `heard_at_ms` is answered from. A reading kept from before a disconnect keeps
    /// its stamp.
    pub heard_at: BTreeMap<FunctionId, u64>,
}

/// The handler's end of one device actor.
pub struct DeviceHandle {
    /// The configured name.
    pub name: String,
    /// The configured unit id, which is the device this actor talks to and the one thing a
    /// reload cannot change without starting another actor.
    pub udid: String,
    /// Commands, each with the channel its response comes back on.
    pub commands: mpsc::Sender<(Command, oneshot::Sender<Response>)>,
    /// Every write to the device goes through here, in the order it is asked for, before the
    /// actor's channel.
    pub writer: Writer,
    /// Events this device publishes; subscribe for a fresh receiver.
    pub events: broadcast::Sender<Event>,
    /// Identity and link state, as the actor last left them.
    pub info: Arc<Mutex<LiveInfo>>,
    /// What the MQTT bridge reads the device through; clone it for a receiver of one's own.
    pub feed: watch::Receiver<Feed>,
}

impl DeviceHandle {
    /// Whether `selector` names this device: its configured name, its product name, or a
    /// prefix of its unit id, none of them case-sensitive.
    ///
    /// An empty selector names nothing: every unit id starts with the empty string, so without
    /// this it would match whatever device happened to be checked first instead of nothing.
    pub fn matches(&self, selector: &str) -> bool {
        if selector.is_empty() {
            return false;
        }
        if self.name.eq_ignore_ascii_case(selector) {
            return true;
        }
        let info = locked(&self.info).clone();
        let lower = selector.to_ascii_lowercase();
        info.product
            .as_deref()
            .is_some_and(|p| p.eq_ignore_ascii_case(selector))
            || info.unit.to_ascii_lowercase().starts_with(&lower)
    }

    /// The same actor under the name the configuration file now spells it with.
    ///
    /// Everything that reaches the actor is shared with the handle this was made from — the
    /// command channel, the events, the identity and link state it publishes — so a rename
    /// costs the device nothing: the link stays up, and only what the daemon calls it changes.
    pub fn renamed(&self, name: &str) -> DeviceHandle {
        DeviceHandle {
            name: name.to_string(),
            udid: self.udid.clone(),
            commands: self.commands.clone(),
            writer: self.writer.clone(),
            events: self.events.clone(),
            info: Arc::clone(&self.info),
            feed: self.feed.clone(),
        }
    }

    /// The `history` reply for this device: its link changes within the last `span_secs`
    /// seconds and the one before them, empty for a device that has never had a link.
    pub fn history(&self, span_secs: u64) -> Response {
        let now_ms = wall_ms();
        let history = Arc::clone(&locked(&self.info).history);
        let transitions = locked(&history).within(now_ms, span_secs);
        Response::History {
            device: self.name.clone(),
            now_ms,
            transitions,
        }
    }

    /// The `readings` reply for this device: the last `span_secs` seconds (at most
    /// [`svolder_ipc::READINGS_SPAN_SECS`]) of its readings, binned by the second, and its link's
    /// changes in that span, and the presets recalled on it in that span. Each series and the
    /// link's changes start with the newest point from before the span, which says what held
    /// when it began.
    pub fn readings(&self, span_secs: u64) -> Response {
        let now_ms = wall_ms();
        let now = std::time::Instant::now();
        let span_secs = span_secs.min(svolder_ipc::READINGS_SPAN_SECS);
        let (ring, history) = {
            let info = locked(&self.info);
            (Arc::clone(&info.ring), Arc::clone(&info.history))
        };
        let profile = self.feed.borrow().profile.clone();
        let series = locked(&ring).series_out(
            profile.as_deref(),
            now,
            now_ms,
            std::time::Duration::from_secs(span_secs),
        );
        let link = locked(&history).within(now_ms, span_secs);
        Response::Readings {
            device: self.name.clone(),
            now_ms,
            bin_ms: u64::try_from(Window::DAEMON.bin.as_millis()).unwrap_or(1000),
            series,
            recalls: locked(&ring).recalls_out(
                now,
                now_ms,
                std::time::Duration::from_secs(span_secs),
            ),
            link,
        }
    }

    /// This device's row for the `devices` verb.
    ///
    /// The moment of the next connect attempt is reported as the whole seconds left until
    /// it, rounded up, so a device that is waiting out a long step of the retry schedule says
    /// how long it is waiting for rather than only that it is not connected.
    pub fn describe(&self) -> svolder_ipc::DeviceInfo {
        let info = locked(&self.info).clone();
        let retry_in_secs = info.next_attempt.map(|at| {
            let left = at.saturating_duration_since(tokio::time::Instant::now());
            left.as_secs() + u64::from(left.subsec_nanos() > 0)
        });
        svolder_ipc::DeviceInfo {
            name: self.name.clone(),
            unit: info.unit,
            product_id: info.product_id,
            product: info.product,
            firmware: info.firmware,
            profile: info.profile,
            connected: info.connected,
            stale: info.stale,
            retry_in_secs,
            passphrase_note: info.passphrase_note,
            refused: info.refused,
            recalled: self.writer.recalled().borrow().clone(),
            signal: info.signal,
        }
    }
}

/// Start the actor for one configured device and hand back the handle to reach it.
///
/// `scanner` is shared by every actor and the handler's own `scan`: an actor looking for its
/// device shares the listen that is running with every other actor looking for theirs.
pub fn spawn(
    transport: Arc<dyn Transport>,
    registry: Arc<Registry>,
    config: DeviceConfig,
    scanner: Arc<Scanner>,
) -> Result<DeviceHandle, String> {
    let unit: UnitId = config
        .udid
        .parse()
        .map_err(|_| format!("device {}: bad udid {:?}", config.name, config.udid))?;
    let (commands, rx) = mpsc::channel(COMMAND_QUEUE);
    let (events, _) = broadcast::channel(EVENT_QUEUE);
    let info = Arc::new(Mutex::new(LiveInfo {
        unit: unit.to_string(),
        product_id: 0,
        product: None,
        firmware: 0,
        profile: String::new(),
        connected: false,
        stale: true,
        next_attempt: None,
        passphrase_note: None,
        refused: false,
        signal: None,
        history: Arc::default(),
        ring: Arc::default(),
    }));
    let (feed, feed_rx) = watch::channel(Feed {
        name: config.name.clone(),
        unit: unit.to_string(),
        ..Feed::default()
    });
    let udid = config.udid.clone();
    let actor = Actor {
        name: config.name.clone(),
        unit,
        profile: Registry::generic(0),
        advert: None,
        config,
        transport,
        registry,
        scanner,
        events: events.clone(),
        info: Arc::clone(&info),
        feed,
        session: None,
        last_state: DeviceState::default(),
        last_failure: None,
        note: None,
        failures: 0,
        farewell: None,
        episode: Episode::default(),
    };
    let name = actor.name.clone();
    tokio::spawn(actor.run(rx));
    let writer = Writer::spawn(commands.clone(), feed_rx.clone());
    tokio::spawn(forward(
        feed_rx.clone(),
        writer.recalled(),
        writer.climb(),
        events.clone(),
        Arc::clone(&locked(&info).ring),
    ));
    Ok(DeviceHandle {
        name,
        udid,
        commands,
        writer,
        events,
        info,
        feed: feed_rx,
    })
}

/// Push the writer's recall memory and climb on the device's events as they change, under the
/// name the feed carries, and note each recall in the ring, until the writer stops.
async fn forward(
    feed: watch::Receiver<Feed>,
    mut recalled: watch::Receiver<Option<String>>,
    mut climb: watch::Receiver<Option<crate::writer::ClimbView>>,
    events: broadcast::Sender<Event>,
    ring: Arc<Mutex<ring::Ring>>,
) {
    let (mut recalls_open, mut climbs_open) = (true, true);
    // The key of the climb last published, which its end is pushed under.
    let mut climbing: Option<String> = None;
    while recalls_open || climbs_open {
        tokio::select! {
            changed = recalled.changed(), if recalls_open => match changed {
                Ok(()) => {
                    let name = recalled.borrow_and_update().clone();
                    if let Some(name) = name {
                        locked(&ring).recalled(&name, std::time::Instant::now());
                        let device = feed.borrow().name.clone();
                        let _ = events.send(Event::Recalled { device, name });
                    }
                }
                Err(_) => recalls_open = false,
            },
            changed = climb.changed(), if climbs_open => match changed {
                Ok(()) => {
                    let view = climb.borrow_and_update().clone();
                    let (key, target) = match view {
                        Some(view) => {
                            climbing = Some(view.key.clone());
                            (view.key, Some(view.target))
                        }
                        None => match climbing.take() {
                            Some(key) => (key, None),
                            None => continue,
                        },
                    };
                    let device = feed.borrow().name.clone();
                    let _ = events.send(Event::Climb { device, key, target });
                }
                Err(_) => climbs_open = false,
            },
        }
    }
}

/// The command channel's receiving end.
type Commands = mpsc::Receiver<(Command, oneshot::Sender<Response>)>;

/// One device's supervision loop and everything it owns.
struct Actor {
    name: String,
    unit: UnitId,
    config: DeviceConfig,
    transport: Arc<dyn Transport>,
    registry: Arc<Registry>,
    /// The profile the actor names and converts values with. The session holds a clone; this
    /// one is authoritative, so a reload can replace it without disturbing the link.
    profile: Profile,
    /// The last advertisement seen; `None` until the device has been found once, meaning
    /// there is no profile yet to look a key up in.
    advert: Option<Advertisement>,
    scanner: Arc<Scanner>,
    events: broadcast::Sender<Event>,
    info: Arc<Mutex<LiveInfo>>,
    /// What the MQTT bridge reads this device through.
    feed: watch::Sender<Feed>,
    /// The live session, absent while a connect attempt owns it.
    session: Option<Session>,
    /// The readings as of the moment the session was handed to an attempt, so `state` and
    /// `get` still answer while the attempt runs.
    last_state: DeviceState,
    /// The reason the last attempt failed, so a repeat of it is logged quietly.
    last_failure: Option<String>,
    /// How the configured passphrase last judged against the advertisement, which a change of
    /// is logged once and shown to the front ends through [`LiveInfo`].
    note: Option<PassphraseNote>,
    /// How many attempts have failed in a row, which is the step of [`RETRY_SCHEDULE`] the
    /// next wait is taken from. A link, and a command that asks for one, put it back to zero.
    failures: u32,
    /// Whoever asked the actor to shut down, answered once the session is really closed.
    farewell: Option<oneshot::Sender<Response>>,
    /// Where the refusal episode of a device on `auto` stands: whether its one drop of the link
    /// is spent. See [`Episode`].
    episode: Episode,
}

impl Actor {
    /// Supervise the device until the handler drops its command sender or asks to shut down.
    ///
    /// A `Shutdown` is answered only after the session is closed, so a caller that awaits it
    /// knows the radio has been released.
    async fn run(mut self, mut rx: Commands) {
        while self.ensure_connected(&mut rx).await {
            if !self.serve_connected(&mut rx).await {
                break;
            }
        }
        self.close().await;
        log::info(&format!("{}: actor stopped", self.name));
        if let Some(reply) = self.farewell.take() {
            let _ = reply.send(Response::Ack);
        }
    }
}

#[cfg(test)]
mod tests;
