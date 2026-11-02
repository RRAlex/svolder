// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The connection to the broker: one `rumqttc` event loop (MQTT 3.1.1 over plain TCP), tried
//! again after a failure with a wait that starts at [`RETRY_MIN`] and doubles up to
//! [`RETRY_MAX`], with `offline` on the bridge's availability topic as its last will.
//!
//! The client keeps the latest payload of every retained topic it has been handed and every
//! subscription it has been asked for, and replays all of them on each connect (the first, and
//! every one after a drop) in this order: `online` for the bridge, every discovery config,
//! every other retained topic (state and availability), then the subscriptions. A caller just
//! hands over what changed, whether or not the broker is there, with no reconnect logic of its
//! own. A message that is not retained (the meters) is sent only while connected and dropped
//! otherwise.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::time::Duration;

use rumqttc::{
    AsyncClient, Event, EventLoop, LastWill, MqttOptions, Outgoing, Packet, QoS, SubAck,
    SubscribeReasonCode,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::payload::{OFFLINE, ONLINE};

/// The wait after the first failed attempt in a row. Each further failure doubles it.
pub const RETRY_MIN: Duration = Duration::from_secs(2);
/// The longest wait between two attempts, repeated for as long as the broker stays away.
pub const RETRY_MAX: Duration = Duration::from_secs(60);

/// How often the broker hears from an otherwise idle connection. A broker that hears nothing
/// for one and a half times this publishes the last will.
const KEEP_ALIVE: Duration = Duration::from_secs(30);
/// The largest packet either way. A discovery config is under a kilobyte and a state object a
/// few hundred bytes, so this is room, not a limit anything reaches.
const MAX_PACKET: usize = 256 * 1024;
/// The requests `rumqttc` queues for its event loop: room for every retained message of
/// several devices put back at once on a connect.
const REQUESTS: usize = 1024;

/// Where the client connects and who it says it is. Debug-printed, the password reads `***`.
#[derive(Clone, PartialEq, Eq)]
pub struct Settings {
    /// The broker's host name or address.
    pub host: String,
    /// The broker's port.
    pub port: u16,
    /// The MQTT client id; see [`client_id`] for the default.
    pub client_id: String,
    /// The user name, when the broker wants one.
    pub username: Option<String>,
    /// The password, sent only with a user name: MQTT 3.1.1 has no password on its own.
    pub password: Option<String>,
    /// The bridge's own availability topic: `online` on every connect, `offline` as the last
    /// will and on a stop, retained either way.
    pub availability: String,
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("client_id", &self.client_id)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("availability", &self.availability)
            .finish()
    }
}

/// The client id to connect with: the configured one, else `svolder-`, this machine's host
/// name and [`socket_tag`] of the socket the daemon serves on — `svolder-myhost-3f9a2c` — or
/// `svolder-` and the tag alone on a machine that reports no host name.
///
/// A broker drops a connection when another client presents its id, so two daemons on one
/// host that shared an id would take the connection from each other every time either
/// reconnected. Each daemon has a socket of its own, and that is what makes the id its own.
pub fn client_id(configured: Option<&str>, socket: &std::path::Path) -> String {
    if let Some(id) = configured.map(str::trim).filter(|id| !id.is_empty()) {
        return id.to_string();
    }
    let tag = socket_tag(socket);
    match host_name() {
        Some(host) => format!("svolder-{host}-{tag}"),
        None => format!("svolder-{tag}"),
    }
}

/// Six hex digits that stand for one socket path: the low 24 bits of its bytes' 32-bit
/// FNV-1a hash, the same for the same path on every run and every build.
pub fn socket_tag(socket: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let hash = socket
        .as_os_str()
        .as_bytes()
        .iter()
        .fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
        });
    format!("{:06x}", hash & 0x00ff_ffff)
}

/// This machine's host name, as the kernel reports it.
fn host_name() -> Option<String> {
    ["/proc/sys/kernel/hostname", "/etc/hostname"]
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.trim().to_string())
        .find(|name| !name.is_empty())
}

/// The wait after `failures` failed attempts in a row (the first failure is `1`): 2 s, then
/// 4, 8, 16 and 32, then 60 s for every failure after that.
pub fn retry_delay(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    RETRY_MIN.saturating_mul(1 << doublings).min(RETRY_MAX)
}

/// Where the connection stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// An attempt to connect is under way.
    Connecting,
    /// The last attempt failed, or the connection dropped; the next attempt starts at
    /// `retry_at`.
    Waiting {
        /// When the next attempt starts.
        retry_at: Instant,
    },
    /// Connected, with everything retained put back on the broker and every subscription
    /// renewed.
    Connected,
}

/// What the client tells its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    /// Connected; everything retained is back on the broker and every subscription renewed.
    Connected,
    /// An attempt failed or the connection dropped, for `reason`; the next attempt is
    /// `retry_in` away.
    Lost {
        /// Why, in `rumqttc`'s words.
        reason: String,
        /// How long until the next attempt.
        retry_in: Duration,
    },
    /// The broker refused a subscription, so nothing published on it will arrive: `filter`
    /// names it, when the refusal can be matched to the subscription it answers.
    Refused {
        /// The filter refused.
        filter: Option<String>,
    },
    /// A message arrived on a subscribed topic.
    Message {
        /// The topic it arrived on.
        topic: String,
        /// Its payload, as sent.
        payload: Vec<u8>,
        /// Whether the broker handed it over as the topic's retained message — the copy a new
        /// subscription is given — rather than as it was published.
        retained: bool,
    },
}

/// Where a retained topic goes in the order a connect puts them back in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rank {
    /// A discovery config, put back before anything it describes.
    Discovery,
    /// State and availability.
    State,
}

/// One thing the owner asked for, in the order it asked.
enum Op {
    Retain {
        topic: String,
        payload: Vec<u8>,
        rank: Rank,
    },
    Clear(String),
    Publish {
        topic: String,
        payload: Vec<u8>,
    },
    Subscribe(String),
    Unsubscribe(String),
    Stop {
        within: Duration,
        done: oneshot::Sender<()>,
    },
}

/// The owner's end of the connection. Every call returns at once: the work happens on the
/// client's own task, in the order the calls were made.
pub struct Client {
    ops: mpsc::UnboundedSender<Op>,
    link: watch::Receiver<Link>,
    task: JoinHandle<()>,
}

impl Client {
    /// Start connecting, and hand back the client with the stream of what it has to say.
    pub fn start(settings: Settings) -> (Client, mpsc::UnboundedReceiver<ClientEvent>) {
        let (ops, ops_rx) = mpsc::unbounded_channel();
        let (events, events_rx) = mpsc::unbounded_channel();
        let (link, link_rx) = watch::channel(Link::Connecting);
        let task = tokio::spawn(run(settings, ops_rx, events, link));
        (
            Client {
                ops,
                link: link_rx,
                task,
            },
            events_rx,
        )
    }

    /// Where the connection stands now.
    pub fn link(&self) -> Link {
        *self.link.borrow()
    }

    /// A receiver that follows where the connection stands, for an owner that reports it from
    /// somewhere the client itself is not.
    pub fn watch_link(&self) -> watch::Receiver<Link> {
        self.link.clone()
    }

    /// Keep `payload` retained on `topic`: sent now when connected, and put back after every
    /// discovery config on each connect.
    pub fn retain(&self, topic: impl Into<String>, payload: impl Into<Vec<u8>>) {
        self.send(Op::Retain {
            topic: topic.into(),
            payload: payload.into(),
            rank: Rank::State,
        });
    }

    /// Keep a Home Assistant discovery config retained on `topic`: sent now when connected, and
    /// put back before every other retained topic on each connect.
    pub fn announce(&self, topic: impl Into<String>, payload: impl Into<Vec<u8>>) {
        self.send(Op::Retain {
            topic: topic.into(),
            payload: payload.into(),
            rank: Rank::Discovery,
        });
    }

    /// Take `topic`'s retained message off the broker with an empty retained payload, now when
    /// connected, else on the next connect, and stop putting it back.
    pub fn clear(&self, topic: impl Into<String>) {
        self.send(Op::Clear(topic.into()));
    }

    /// Send `payload` on `topic`, not retained, when connected; dropped otherwise.
    pub fn publish(&self, topic: impl Into<String>, payload: impl Into<Vec<u8>>) {
        self.send(Op::Publish {
            topic: topic.into(),
            payload: payload.into(),
        });
    }

    /// Subscribe to `filter`, now when connected, and again on every connect.
    pub fn subscribe(&self, filter: impl Into<String>) {
        self.send(Op::Subscribe(filter.into()));
    }

    /// Stop subscribing to `filter`.
    pub fn unsubscribe(&self, filter: impl Into<String>) {
        self.send(Op::Unsubscribe(filter.into()));
    }

    /// Publish `offline` for the bridge, disconnect, and end, taking no longer than `within`.
    ///
    /// Everything asked for before this goes out first, so a caller that retains `offline`
    /// for each of its devices and then stops has said so for all of them before the
    /// disconnect. A disconnect the broker has read cancels the last will; a client that is
    /// not connected when it stops says nothing, since the broker has already published the
    /// will for the connection it lost.
    pub async fn stop(self, within: Duration) {
        let (done, stopped) = oneshot::channel();
        self.send(Op::Stop { within, done });
        // The task bounds its own farewell by `within`; the margin is for the task to reply.
        let _ = tokio::time::timeout(within + Duration::from_secs(1), stopped).await;
        self.task.abort();
    }

    /// Hand one request to the task. A task that has ended takes nothing, which only happens
    /// once the client is stopping.
    fn send(&self, op: Op) {
        let _ = self.ops.send(op);
    }
}

/// What the event-loop task reports to the client's task.
enum Polled {
    /// The broker accepted the connection.
    ConnAck,
    /// A message arrived.
    Message {
        topic: String,
        payload: Vec<u8>,
        retained: bool,
    },
    /// A subscription went out under packet id `pkid`.
    Subscribing(u16),
    /// The broker answered the subscription sent under `pkid`, refusing it or not.
    SubAck { pkid: u16, refused: bool },
    /// An attempt failed or the connection dropped; the next attempt starts at `retry_at`.
    Lost { reason: String, retry_at: Instant },
    /// The wait is over and the next attempt has started.
    Retrying,
}

/// Everything the client puts back on the broker on a connect.
#[derive(Default)]
struct Held {
    /// Discovery configs, by topic.
    configs: BTreeMap<String, Vec<u8>>,
    /// Every other retained topic, by topic.
    states: BTreeMap<String, Vec<u8>>,
    /// Topics cleared, cleared again on every connect: a clear that was sent can still be
    /// lost with the connection it went out on, and an empty retained message on a topic
    /// already empty changes nothing. Retaining a topic again takes it off the list.
    clears: BTreeSet<String>,
    /// Filters to subscribe to on every connect.
    subscriptions: BTreeSet<String>,
    /// The subscriptions sent and not yet answered.
    unacked: Unacked,
}

/// The subscriptions sent and not yet answered, so a refusal can name the filter it refuses:
/// `rumqttc` gives a subscription its packet id as it writes it, in the order it was asked
/// for, and the broker's answer carries only that id.
#[derive(Debug, Default)]
struct Unacked {
    /// Asked for and not written yet, oldest first.
    queued: VecDeque<String>,
    /// Written, by packet id.
    written: HashMap<u16, String>,
}

impl Unacked {
    /// A subscription to `filter` was handed to `rumqttc`.
    fn queued(&mut self, filter: String) {
        self.queued.push_back(filter);
    }

    /// The oldest subscription asked for went out under `pkid`.
    fn written(&mut self, pkid: u16) {
        if let Some(filter) = self.queued.pop_front() {
            self.written.insert(pkid, filter);
        }
    }

    /// The broker answered the subscription sent under `pkid`: the filter it was for.
    fn answered(&mut self, pkid: u16) -> Option<String> {
        self.written.remove(&pkid)
    }

    /// The connection is gone, and with it every subscription still unanswered on it.
    fn reset(&mut self) {
        self.queued.clear();
        self.written.clear();
    }
}

impl Held {
    /// Record one request, and carry it out on `live` when there is a connection.
    fn apply(&mut self, op: Op, live: Option<&AsyncClient>) {
        match op {
            Op::Retain {
                topic,
                payload,
                rank,
            } => {
                self.clears.remove(&topic);
                if let Some(client) = live {
                    retained(client, &topic, payload.clone());
                }
                let (keep, drop) = match rank {
                    Rank::Discovery => (&mut self.configs, &mut self.states),
                    Rank::State => (&mut self.states, &mut self.configs),
                };
                drop.remove(&topic);
                keep.insert(topic, payload);
            }
            Op::Clear(topic) => {
                self.configs.remove(&topic);
                self.states.remove(&topic);
                if let Some(client) = live {
                    retained(client, &topic, Vec::new());
                }
                self.clears.insert(topic);
            }
            Op::Publish { topic, payload } => {
                if let Some(client) = live {
                    let _ = client.try_publish(topic, QoS::AtMostOnce, false, payload);
                }
            }
            Op::Subscribe(filter) => {
                if let Some(client) = live {
                    subscribe(client, &mut self.unacked, &filter);
                }
                self.subscriptions.insert(filter);
            }
            Op::Unsubscribe(filter) => {
                if self.subscriptions.remove(&filter)
                    && let Some(client) = live
                {
                    let _ = client.try_unsubscribe(filter);
                }
            }
            // The task takes a stop for itself before it gets here.
            Op::Stop { .. } => {}
        }
    }

    /// Put everything back on a fresh connection: `online` first, then every discovery config,
    /// every other retained topic and every clear, then the subscriptions.
    fn replay(&mut self, client: &AsyncClient, availability: &str) {
        retained(client, availability, ONLINE.as_bytes().to_vec());
        for (topic, payload) in self.configs.iter().chain(self.states.iter()) {
            retained(client, topic, payload.clone());
        }
        for topic in &self.clears {
            retained(client, topic, Vec::new());
        }
        self.unacked.reset();
        for filter in &self.subscriptions {
            subscribe(client, &mut self.unacked, filter);
        }
    }
}

/// Queue one subscription, and note it as unanswered once `rumqttc` has taken it.
fn subscribe(client: &AsyncClient, unacked: &mut Unacked, filter: &str) {
    if client
        .try_subscribe(filter.to_string(), QoS::AtLeastOnce)
        .is_ok()
    {
        unacked.queued(filter.to_string());
    }
}

/// Whether a broker's answer to a subscription refuses it.
fn refuses(ack: &SubAck) -> bool {
    ack.return_codes.contains(&SubscribeReasonCode::Failure)
}

/// Queue one retained message. A queue that is full drops it: a retained topic is put back
/// on the next connect whatever happens to it now.
fn retained(client: &AsyncClient, topic: &str, payload: Vec<u8>) {
    let _ = client.try_publish(topic, QoS::AtLeastOnce, true, payload);
}

/// The connection options `settings` describe, with the bridge's `offline` as the last will.
fn options(settings: &Settings) -> MqttOptions {
    let mut options = MqttOptions::new(&settings.client_id, &settings.host, settings.port);
    options
        .set_keep_alive(KEEP_ALIVE)
        .set_clean_session(true)
        .set_max_packet_size(MAX_PACKET, MAX_PACKET)
        .set_last_will(LastWill::new(
            &settings.availability,
            OFFLINE,
            QoS::AtLeastOnce,
            true,
        ));
    if let Some(user) = settings.username.as_deref() {
        options.set_credentials(user, settings.password.as_deref().unwrap_or_default());
    }
    options
}

/// The client's own task: carry out every request, keep what a connect puts back, and turn
/// what the event loop reports into [`Link`] and [`ClientEvent`]s.
async fn run(
    settings: Settings,
    mut ops: mpsc::UnboundedReceiver<Op>,
    events: mpsc::UnboundedSender<ClientEvent>,
    link: watch::Sender<Link>,
) {
    let (client, eventloop) = AsyncClient::new(options(&settings), REQUESTS);
    let (polled_tx, mut polled) = mpsc::unbounded_channel();
    let poller = tokio::spawn(poll(eventloop, polled_tx));
    let mut held = Held::default();
    let mut connected = false;
    loop {
        tokio::select! {
            op = ops.recv() => match op {
                Some(Op::Stop { within, done }) => {
                    if connected {
                        farewell(&client, &settings.availability, &mut polled, within).await;
                    }
                    poller.abort();
                    let _ = done.send(());
                    return;
                }
                Some(op) => held.apply(op, connected.then_some(&client)),
                // The owner is gone without a stop: end, and let the broker publish the will.
                None => {
                    poller.abort();
                    return;
                }
            },
            report = polled.recv() => match report {
                Some(Polled::ConnAck) => {
                    connected = true;
                    held.replay(&client, &settings.availability);
                    link.send_replace(Link::Connected);
                    let _ = events.send(ClientEvent::Connected);
                }
                Some(Polled::Message {
                    topic,
                    payload,
                    retained,
                }) => {
                    let _ = events.send(ClientEvent::Message {
                        topic,
                        payload,
                        retained,
                    });
                }
                Some(Polled::Subscribing(pkid)) => held.unacked.written(pkid),
                Some(Polled::SubAck { pkid, refused }) => {
                    let filter = held.unacked.answered(pkid);
                    if refused {
                        let _ = events.send(ClientEvent::Refused { filter });
                    }
                }
                Some(Polled::Lost { reason, retry_at }) => {
                    connected = false;
                    held.unacked.reset();
                    link.send_replace(Link::Waiting { retry_at });
                    let retry_in = retry_at.saturating_duration_since(Instant::now());
                    let _ = events.send(ClientEvent::Lost { reason, retry_in });
                }
                Some(Polled::Retrying) => {
                    link.send_replace(Link::Connecting);
                }
                // The event loop's task only ends when this one aborts it.
                None => return,
            },
        }
    }
}

/// Say `offline` for the bridge and disconnect, then wait — no longer than `within` — for
/// the broker to close the connection, which is when it has read everything sent before.
async fn farewell(
    client: &AsyncClient,
    availability: &str,
    polled: &mut mpsc::UnboundedReceiver<Polled>,
    within: Duration,
) {
    retained(client, availability, OFFLINE.as_bytes().to_vec());
    let _ = client.try_disconnect();
    let _ = tokio::time::timeout(within, async {
        while let Some(report) = polled.recv().await {
            if matches!(report, Polled::Lost { .. }) {
                return;
            }
        }
    })
    .await;
}

/// Drive the event loop: it connects on the first poll and again on the first poll after a
/// failure, so a failure is followed by the wait [`retry_delay`] gives before polling again.
///
/// A task of its own, so nothing ever drops a poll part way through writing a packet.
async fn poll(mut eventloop: EventLoop, report: mpsc::UnboundedSender<Polled>) {
    let mut failures: u32 = 0;
    loop {
        let sent = match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                failures = 0;
                report.send(Polled::ConnAck)
            }
            Ok(Event::Incoming(Packet::Publish(publish))) => report.send(Polled::Message {
                topic: publish.topic,
                payload: publish.payload.to_vec(),
                retained: publish.retain,
            }),
            Ok(Event::Outgoing(Outgoing::Subscribe(pkid))) => {
                report.send(Polled::Subscribing(pkid))
            }
            Ok(Event::Incoming(Packet::SubAck(ack))) => report.send(Polled::SubAck {
                pkid: ack.pkid,
                refused: refuses(&ack),
            }),
            Ok(_) => Ok(()),
            Err(e) => {
                failures = failures.saturating_add(1);
                let retry_at = Instant::now() + retry_delay(failures);
                if report
                    .send(Polled::Lost {
                        reason: e.to_string(),
                        retry_at,
                    })
                    .is_err()
                {
                    return;
                }
                tokio::time::sleep_until(retry_at).await;
                report.send(Polled::Retrying)
            }
        };
        if sent.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests;
