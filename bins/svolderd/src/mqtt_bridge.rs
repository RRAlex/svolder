// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MQTT bridge: one task beside the device actors that publishes every device the daemon
//! holds to the broker `[mqtt]` names, announced for Home Assistant's discovery.
//!
//! It reads each device only through its actor's feed. It publishes: the device's availability
//! as the link comes and goes, a discovery config per exposed control once the device has been
//! seen, the state JSON on a changed reading (at most every [`STATE_EVERY`]), the meters
//! JSON (at most every [`METERS_EVERY`]) and the diagnostics: the signal strength with its
//! source and time, and the firmware builds the device has reported. A device removed from the
//! configuration has every retained topic of its own cleared — including any discovery config an
//! earlier run left under it that no control announces any more — so its entities leave Home
//! Assistant.
//!
//! A stop (the daemon's own, or a reload that disables the bridge or changes its section)
//! publishes `offline` for every device and for the bridge before disconnecting. A daemon that
//! dies without one leaves the broker to publish the bridge's last will, also `offline`.
//!
//! With `commands = true` it also subscribes to each device's command topics and writes what
//! arrives there through the device's actor as a `set` request: parsed against the device's
//! profile, volume capped at `max_volume_db`, then the actor's own validation and refusals; the
//! identify button's press is not a control and goes to the unit as its own command. Each
//! command goes to the device's writer ([`crate::writer`]), which keeps one write in flight per
//! control with the latest waiting behind it, holds tone-control writes during a recall and
//! sends in the order the commands arrived; the writer switches a band written while the tone
//! control reads Flat to EQ ([`crate::writer::Options::flat_to_eq`]). A volume command climbs
//! above three quarters of the range rather than jumping ([`crate::writer::Writer::glide`]).
//! Writes handed to a writer before a bridge stop still reach the device afterwards: the stop
//! does not take them back.
//!
//! A device with a tone control also gets a preset select, the loudness strength and every
//! band's gain, frequency and Q (see `svolder_mqtt::eq`). The select offers the presets that
//! fit the device, which the daemon hands over at start and again on every reload; recalling
//! one sends the writes [`crate::recall`] plans, in order, as one write under the select's key.
//!
//! The computer's EQ is a bridge-level unit of its own, `computer`: one switch and one preset
//! sensor ([`svolder_mqtt::computer`]), published while the host reports an EQ and followed
//! through its view ([`ComputerLink`]). Its command is never acted on when retained, and an ON
//! with nothing kept to apply leaves the EQ off and says why in the log.
//!
//! The task's work is written in five children, each an `impl BridgeTask` block; this file holds
//! the handle ([`Bridge`]) and the types they share:
//! - [`commands`]: a message from the broker, parsed into a write, and the end of that write.
//! - [`computer`]: the computer's EQ switch and sensor.
//! - [`connection`]: the task's loop and its farewell.
//! - [`discovery`]: following the devices and presets, subscriptions, clearing a device.
//! - [`state`]: marking devices changed and publishing availability, configs, state, meters and
//!   diagnostics.

mod commands;
mod computer;
mod connection;
mod discovery;
mod state;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use svolder_ipc::MqttStatus;
use svolder_ipc::config::MqttConfig;
use svolder_mqtt::{Client, Link, Settings, topics};
use svolder_profiles::Profile;
use svolder_profiles::tone::preset::Preset;
use svolder_proto::FunctionId;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::Instant;

use crate::device::{DeviceHandle, Feed};
use crate::host::EqSwitch;
use crate::log;
use crate::writer::{Outcome, Planned, Writer};

/// The environment variable whose value, when set and not empty, is the broker password in
/// place of the config's `password`.
const PASSWORD_VAR: &str = "SVOLDER_MQTT_PASSWORD";

/// The shortest time between two state publishes: a burst of readings — a knob turned, a
/// refresh after a connect — is one publish of where it ended.
const STATE_EVERY: Duration = Duration::from_millis(100);
/// The shortest time between two meters publishes of one device.
const METERS_EVERY: Duration = Duration::from_secs(1);
/// The longest a stop waits for the broker to take the last words before it disconnects
/// regardless.
const STOP_WITHIN: Duration = Duration::from_secs(3);

/// What the bridge reaches the computer's EQ through: the host's view as it changes, and the
/// switch that turns the EQ on and off.
#[derive(Clone)]
pub struct ComputerLink {
    /// The host's view.
    pub view: watch::Receiver<svolder_ipc::host::HostView>,
    /// Turns the EQ on or off.
    pub switch: Arc<dyn EqSwitch>,
}

/// The daemon's end of a running bridge.
pub struct Bridge {
    /// The section the bridge was started from.
    config: MqttConfig,
    ops: mpsc::UnboundedSender<Op>,
    /// Where the connection stands, as the client reports it.
    link: watch::Receiver<Link>,
    task: JoinHandle<()>,
}

/// What the daemon asks of the bridge's task.
enum Op {
    /// The devices the daemon now holds.
    Devices(Vec<Arc<DeviceHandle>>),
    /// The user's presets as the configuration now holds them.
    Presets(Vec<Preset>),
    /// Forget every device but the units kept, then say `offline` for the ones kept and for
    /// the bridge, disconnect, and answer.
    Stop {
        keep: BTreeSet<String>,
        done: oneshot::Sender<()>,
    },
}

impl Bridge {
    /// Start the bridge `config` describes, publishing `devices`, or `None` when the section
    /// does not turn one on — in which case nothing opens a socket to anything. `socket` is
    /// the one the daemon serves on, which the default client id is made from.
    pub fn start(
        config: &MqttConfig,
        devices: Vec<Arc<DeviceHandle>>,
        presets: Vec<Preset>,
        computer: Option<ComputerLink>,
        socket: &std::path::Path,
    ) -> Option<Bridge> {
        if !config.enabled {
            return None;
        }
        let host = config.host.clone()?;
        let settings = Settings {
            host,
            port: config.port,
            client_id: svolder_mqtt::client::client_id(config.client_id.as_deref(), socket),
            username: config.username.clone(),
            password: password(std::env::var(PASSWORD_VAR).ok(), config.password.as_deref()),
            availability: topics::bridge_availability(&config.base_topic),
        };
        if settings.username.is_none() && settings.password.is_some() {
            log::warn(
                "mqtt: a password with no username is not sent; MQTT 3.1.1 carries a password \
                 only with a username",
            );
        }
        log::info(&format!(
            "mqtt: connecting to {} as {}, {}",
            address(&settings.host, settings.port),
            settings.client_id,
            match config.commands {
                true => "taking commands",
                false => "read-only",
            }
        ));
        let (client, events) = Client::start(settings);
        let link = client.watch_link();
        let (ops, ops_rx) = mpsc::unbounded_channel();
        let (changes_tx, changes) = mpsc::unbounded_channel();
        let mut task = BridgeTask {
            config: config.clone(),
            client,
            devices: BTreeMap::new(),
            changes_tx,
            writes: JoinSet::new(),
            flush_at: None,
            last_flush: None,
            last_lost: None,
            retained_commands: BTreeSet::new(),
            presets,
            computer: None,
        };
        if let Some(link) = computer {
            task.follow_computer(link);
        }
        let task = tokio::spawn(task.run(events, ops_rx, changes));
        let bridge = Bridge {
            config: config.clone(),
            ops,
            link,
            task,
        };
        bridge.devices(devices);
        Some(bridge)
    }

    /// Where the bridge stands, as `version` reports it: connecting, with the whole seconds
    /// until the next attempt while it waits between attempts, or connected.
    pub fn status(&self) -> MqttStatus {
        let host = self.config.host.clone().unwrap_or_default();
        let port = self.config.port;
        match *self.link.borrow() {
            Link::Connecting => MqttStatus::Connecting {
                host,
                port,
                retry_in_secs: None,
            },
            Link::Waiting { retry_at } => {
                let left = retry_at.saturating_duration_since(Instant::now());
                MqttStatus::Connecting {
                    host,
                    port,
                    retry_in_secs: Some(left.as_secs() + u64::from(left.subsec_nanos() > 0)),
                }
            }
            Link::Connected => MqttStatus::Connected {
                host,
                port,
                commands: self.config.commands,
            },
        }
    }

    /// The section this bridge runs with.
    pub fn config(&self) -> &MqttConfig {
        &self.config
    }

    /// Publish exactly `devices` from now on: a device new to the list is announced, one that
    /// left it has its retained topics cleared.
    pub fn devices(&self, devices: Vec<Arc<DeviceHandle>>) {
        let _ = self.ops.send(Op::Devices(devices));
    }

    /// Offer `presets`, the user's own as the configuration now holds them, from now on: a
    /// device's preset select is announced again with the options they make, and its state
    /// published again.
    pub fn presets(&self, presets: Vec<Preset>) {
        let _ = self.ops.send(Op::Presets(presets));
    }

    /// Stop the bridge, keeping `devices`: every device it publishes that is not among them
    /// has its topics cleared as if it had been taken out of the file, then `offline` goes out
    /// for each one kept and for the bridge, and it disconnects — taking no longer than
    /// [`STOP_WITHIN`] and a moment.
    pub async fn stop(self, devices: &[Arc<DeviceHandle>]) {
        let keep = devices
            .iter()
            .map(|handle| handle.feed.borrow().unit.clone())
            .collect();
        let (done, stopped) = oneshot::channel();
        if self.ops.send(Op::Stop { keep, done }).is_ok() {
            let _ = tokio::time::timeout(STOP_WITHIN + Duration::from_secs(1), stopped).await;
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The broker password: `from_env`, [`PASSWORD_VAR`]'s value, when it names one, else the
/// config's.
fn password(from_env: Option<String>, configured: Option<&str>) -> Option<String> {
    from_env
        .filter(|p| !p.is_empty())
        .or_else(|| configured.map(str::to_string))
}

/// `host:port`, the way the log and `daemon status` name the broker.
fn address(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

/// The bridge's own task and everything it keeps.
struct BridgeTask {
    config: MqttConfig,
    client: Client,
    /// Every device published, by unit id.
    devices: BTreeMap<String, PublishedDevice>,
    /// Where each device's feed watcher says a device changed, by unit id.
    changes_tx: mpsc::UnboundedSender<String>,
    /// The writes handed to the devices' writers, each coming back as a recall's plan or a
    /// write's end.
    writes: JoinSet<WriteOutcome>,
    /// When the devices marked dirty are published next.
    flush_at: Option<Instant>,
    /// When they were last published.
    last_flush: Option<Instant>,
    /// Why the last attempt to reach the broker failed, so a repeat of it is logged quietly.
    last_lost: Option<String>,
    /// The command topics a retained command was found on and ignored, each said once.
    retained_commands: BTreeSet<String>,
    /// The user's presets, from which each device's fitting ones are chosen.
    presets: Vec<Preset>,
    /// The computer's EQ, when the daemon hands the bridge one.
    computer: Option<computer::Published>,
}

/// What a write handed to a device's writer comes back as.
#[allow(
    clippy::large_enum_variant,
    reason = "an outcome is made once per write and moved once"
)]
enum WriteOutcome {
    /// A recall started: its control, and its plan or why there is none.
    Planned(Control, Planned),
    /// A write ended: its control, and how it ended.
    Written(Control, Outcome),
    /// A recall a later pick replaced before it started.
    Replaced,
    /// The computer EQ's switch was turned: whether it was asked on, and how it ended.
    Computer(bool, Result<(), String>),
}

/// One control of one device: its unit id and its profile key.
type Control = (String, String);

/// A command ready for a device's actor, as a `set` or an `eq mode` request carries it.
#[derive(Debug, Clone, PartialEq)]
enum WriteRequest {
    /// `set <key> <value>`.
    Set { key: String, value: String },
    /// `eq mode <mode>`.
    EqMode { mode: String },
    /// `eq set <band> <param> <value>`, the band counted from 1.
    EqBand {
        band: usize,
        param: &'static str,
        value: f64,
    },
    /// The loudness strength, as the tone screen writes it: through the profile's control on
    /// that function when it has one, else as a raw `u8`.
    Loudness {
        key: Option<String>,
        id: FunctionId,
        raw: u8,
    },
    /// Recall the preset of this name.
    Preset { name: String },
    /// Recall this slot of the device's own.
    Slot { slot: u8 },
    /// Apply the built-in setting of this name.
    Setting { name: String },
    /// Ask the unit to show itself (the identify button's press).
    Identify,
}

/// What a device's discovery configs were built from.
struct ConfigsFrom {
    profile: Arc<Profile>,
    name: String,
    firmware: Option<u16>,
    /// The preset names the select offered, in order.
    presets: Vec<String>,
}

/// One device the bridge publishes, and what it has published for it.
struct PublishedDevice {
    feed: watch::Receiver<Feed>,
    /// Where the device's writes go: its writer.
    writer: Writer,
    /// The task that turns a change of the feed into a message on `changes_tx`.
    watcher: JoinHandle<()>,
    /// Whether the feed has changed since the device was last published.
    dirty: bool,
    /// The availability last published: `true` for `online`.
    available: Option<bool>,
    /// What the discovery configs were last built from: the profile, the name and the
    /// firmware. Built again only when one of them changes. The profile is held, not just
    /// its address, so a profile freed and another allocated where it was is never taken
    /// for the same one.
    configs_from: Option<ConfigsFrom>,
    /// Every discovery config published, by topic.
    configs: BTreeMap<String, String>,
    /// Discovery configs found on the broker under this device before its profile was known,
    /// to clear once it is if no control announces them.
    leftovers: BTreeSet<String>,
    /// The state JSON last published.
    state: Option<String>,
    /// The diagnostic topics' payloads last published (the signal and its attributes, the
    /// firmware builds), by topic. A topic is there only once its value was known.
    diagnostics: BTreeMap<String, String>,
    /// The meters JSON last published, and when.
    meters: Option<(String, Instant)>,
}

/// Tell the bridge each time one device's feed or its recall memory changes, until either goes
/// or the bridge does.
async fn watch_feed(
    mut feed: watch::Receiver<Feed>,
    mut recalled: watch::Receiver<Option<String>>,
    unit: String,
    changes: mpsc::UnboundedSender<String>,
) {
    // A closed memory watch only stops that arm: the feed goes on being followed, and the
    // feed's end is the device's.
    let mut memory_open = true;
    loop {
        tokio::select! {
            changed = feed.changed() => if changed.is_err() { return },
            changed = recalled.changed(), if memory_open => {
                if changed.is_err() {
                    memory_open = false;
                    continue;
                }
            }
        }
        if changes.send(unit.clone()).is_err() {
            return;
        }
    }
}

/// Wait until `at`; the caller only polls this with a deadline in hand.
async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests;
