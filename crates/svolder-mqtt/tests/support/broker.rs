// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An MQTT broker for tests, and a client that watches it: `rumqttd` in this process, listening
//! on 127.0.0.1 and a port of its own and nowhere else, so no test ever reaches a broker outside
//! this machine or one another test is using.
//!
//! The crates whose tests drive the bridge include this one file by path, so every one of them
//! runs against the same broker.

#![allow(dead_code)]

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener};
use std::time::Duration;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::mpsc;

/// How long a broker is given to start listening. It is the longest a machine running every
/// suite at once needs; a broker that is up answers at once.
const LISTENING: Duration = Duration::from_secs(20);

/// A broker running in this process on `127.0.0.1:port`. It runs until the test binary ends:
/// `rumqttd` has no way to stop one, and every test has a broker and a port of its own.
pub struct TestBroker {
    /// The port it listens on, on 127.0.0.1.
    pub port: u16,
}

impl TestBroker {
    /// A broker that lets any client in.
    pub fn start() -> TestBroker {
        start(None)
    }

    /// A broker that lets in only `user` with `password`.
    pub fn with_login(user: &str, password: &str) -> TestBroker {
        start(Some((user, password)))
    }
}

/// How many times a watcher sends its subscription, and how long it waits for each answer: an
/// answer that comes late is taken from any of the attempts.
const SUBSCRIBE_ATTEMPTS: u32 = 10;
const SUBSCRIBE_WAIT: Duration = Duration::from_secs(3);

/// How many times `start` picks a port and tries again when the broker does not come up; with
/// `LISTENING` it bounds a start at a minute, which every case's limit exceeds.
const ATTEMPTS: usize = 3;

/// Start a broker on a free port and wait until it answers an MQTT connect. A broker that
/// fails to come up, because another test took its port first, is started again on a new port,
/// a bounded number of times.
fn start(login: Option<(&str, &str)>) -> TestBroker {
    let mut last = String::new();
    for _ in 0..ATTEMPTS {
        let port = free_port();
        match try_start(port, login) {
            Ok(()) => return TestBroker { port },
            Err(why) => last = format!("port {port}: {why}"),
        }
    }
    panic!("the test broker never came up in {ATTEMPTS} attempts; the last: {last}");
}

/// Run a broker on `port` and wait, no longer than `LISTENING`, until a probe client's publish
/// reaches this broker's own router, or an error saying why the broker is not up.
fn try_start(port: u16, login: Option<(&str, &str)>) -> Result<(), String> {
    let listen = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    let connections = rumqttd::ConnectionSettings {
        connection_timeout_ms: 5000,
        max_payload_size: 256 * 1024,
        max_inflight_count: 500,
        auth: login
            .map(|(user, password)| HashMap::from([(user.to_string(), password.to_string())])),
        external_auth: None,
        dynamic_filters: true,
    };
    let server = rumqttd::ServerSettings {
        name: "v4".to_string(),
        listen,
        tls: None,
        next_connection_delay_ms: 1,
        connections,
    };
    let config = rumqttd::Config {
        router: rumqttd::RouterConfig {
            max_connections: 64,
            max_outgoing_packet_count: 200,
            max_segment_size: 1024 * 1024,
            max_segment_count: 10,
            ..Default::default()
        },
        v4: Some(HashMap::from([("v4".to_string(), server)])),
        ..Default::default()
    };
    let mut broker = rumqttd::Broker::new(config);
    // A client inside this broker's own router listens for the probe's publish: only a connection
    // this broker's server accepted can reach it, so a probe answered by another broker on the
    // same port (another test, in this process or another, picked the same free port) proves
    // nothing.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let owner_topic = format!("owner-probe/{}/{n}", std::process::id());
    let (mut owner_tx, mut owner_rx) = broker
        .link(&format!("owner-{}-{n}", std::process::id()))
        .map_err(|e| format!("the owner link: {e}"))?;
    owner_tx
        .subscribe(owner_topic.clone())
        .map_err(|e| format!("the owner link's subscription: {e}"))?;
    // `start` returns only when the broker could not run (a server that cannot bind its port
    // ends its thread and `start` then returns), so any message on this channel is a failure.
    let (ended_tx, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = broker.start();
        let _ = ended_tx.send(format!("the broker stopped: {outcome:?}"));
    });
    let deadline = std::time::Instant::now() + LISTENING;
    let (owned_tx, owned) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // The link lives until the probe's publish arrives or the deadline passes.
        let _keep = &mut owner_tx;
        let outcome = loop {
            match owner_rx.recv_deadline(deadline) {
                Ok(Some(rumqttd::Notification::Forward(_))) => break Ok(()),
                Ok(Some(rumqttd::Notification::Disconnect(..))) => {
                    break Err("the owner link was disconnected".to_string());
                }
                Ok(Some(_)) => {}
                Ok(None) => break Err(format!("no probe publish within {LISTENING:?}")),
                Err(e) => break Err(format!("the owner link: {e}")),
            }
        };
        let _ = owned_tx.send(outcome);
    });
    let (probe_tx, probe) = std::sync::mpsc::channel();
    let login = login.map(|(user, password)| (user.to_string(), password.to_string()));
    std::thread::spawn(move || {
        let _ = probe_tx.send(probe_connack(port, login, deadline, owner_topic));
    });
    loop {
        if let Ok(why) = ended.try_recv() {
            return Err(why);
        }
        if let Ok(Err(why)) = probe.try_recv() {
            return Err(why);
        }
        match owned.recv_timeout(Duration::from_millis(10)) {
            // The publish reached this broker's router, so this broker's server answered.
            Ok(outcome) => {
                return match ended.try_recv() {
                    Ok(why) => Err(why),
                    Err(_) => outcome,
                };
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("the owner link ended without an answer".to_string());
            }
        }
    }
}

/// Connect a probe client to `port`, wait until `deadline` for the broker's CONNACK, publish
/// once on `topic`, then disconnect it. It runs on a runtime of its own so a caller inside a
/// runtime is not blocked.
fn probe_connack(
    port: u16,
    login: Option<(String, String)>,
    deadline: std::time::Instant,
    topic: String,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("a runtime for the probe: {e}"))?;
    runtime.block_on(async move {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = format!("probe-{}-{n}", std::process::id());
        let mut options = MqttOptions::new(id, "127.0.0.1", port);
        if let Some((user, password)) = login {
            options.set_credentials(user, password);
        }
        let (client, mut eventloop) = AsyncClient::new(options, 4);
        let mut last = "no answer".to_string();
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let until = tokio::time::Instant::now() + left;
        loop {
            match tokio::time::timeout_at(until, eventloop.poll()).await {
                Ok(Ok(Event::Incoming(Packet::ConnAck(_)))) => {
                    let _ = client.publish(topic, QoS::AtMostOnce, false, "here").await;
                    let _ = client.disconnect().await;
                    // The broker closes the connection once it has read the publish and the
                    // disconnect: poll until it does, no longer than the deadline.
                    let _ = tokio::time::timeout_at(until, async {
                        while eventloop.poll().await.is_ok() {}
                    })
                    .await;
                    return Ok(());
                }
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    last = e.to_string();
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(_) => return Err(format!("no CONNACK within {LISTENING:?}; the last: {last}")),
            }
        }
    })
}

/// A port on 127.0.0.1 that nothing listens on, which is also what a test that wants a broker
/// that is not there connects to.
pub fn free_port() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind a free port");
    listener.local_addr().expect("a local address").port()
}

/// One message a watcher saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    /// The topic it arrived on.
    pub topic: String,
    /// Its payload, as text.
    pub payload: String,
    /// Whether the broker sent it as a retained message: the copy a new subscription is given.
    pub retained: bool,
}

/// A client of the test broker that subscribes to what a test watches and hands over what
/// arrives, in order, and that can publish, as Home Assistant would.
pub struct Watcher {
    client: AsyncClient,
    seen: mpsc::UnboundedReceiver<Seen>,
    task: tokio::task::JoinHandle<()>,
}

impl Watcher {
    /// Connect to the broker on `port` and subscribe to `filters`.
    pub async fn connect(port: u16, filters: &[&str]) -> Watcher {
        Watcher::connect_as(port, filters, None).await
    }

    /// Connect with a login, for a broker that wants one.
    pub async fn connect_as(port: u16, filters: &[&str], login: Option<(&str, &str)>) -> Watcher {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = format!("watcher-{}-{n}", std::process::id());
        Watcher::connect_with(port, filters, login, &id).await
    }

    /// Connect as the client `id`. A broker drops whichever connection held that id before,
    /// which is how a case takes another client's connection away from it.
    pub async fn connect_with(
        port: u16,
        filters: &[&str],
        login: Option<(&str, &str)>,
        id: &str,
    ) -> Watcher {
        let mut options = MqttOptions::new(id, "127.0.0.1", port);
        options.set_max_packet_size(256 * 1024, 256 * 1024);
        if let Some((user, password)) = login {
            options.set_credentials(user, password);
        }
        let (client, mut eventloop) = AsyncClient::new(options, 64);
        let (tx, seen) = mpsc::unbounded_channel();
        let (ready_tx, mut ready) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::SubAck(_))) => {
                        let _ = ready_tx.send(());
                    }
                    Ok(Event::Incoming(Packet::Publish(p))) => {
                        let seen = Seen {
                            topic: p.topic,
                            payload: String::from_utf8_lossy(&p.payload).into_owned(),
                            retained: p.retain,
                        };
                        if tx.send(seen).is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        });
        let filters: Vec<rumqttc::SubscribeFilter> = filters
            .iter()
            .map(|f| rumqttc::SubscribeFilter::new(f.to_string(), QoS::AtLeastOnce))
            .collect();
        if !filters.is_empty() {
            // A subscription that goes unacknowledged is sent again, a bounded number of times.
            let mut acknowledged = false;
            for _ in 0..SUBSCRIBE_ATTEMPTS {
                client
                    .subscribe_many(filters.clone())
                    .await
                    .expect("the watcher subscribes");
                if tokio::time::timeout(SUBSCRIBE_WAIT, ready.recv())
                    .await
                    .is_ok()
                {
                    acknowledged = true;
                    break;
                }
            }
            assert!(
                acknowledged,
                "the broker acknowledged the watcher's subscription"
            );
        }
        Watcher { client, seen, task }
    }

    /// The next message on `topic` that `wanted` accepts, waiting no longer than `within`, or
    /// a failure naming everything else that arrived meanwhile.
    pub async fn wait_for(
        &mut self,
        topic: &str,
        within: Duration,
        wanted: impl Fn(&Seen) -> bool,
    ) -> Seen {
        let mut other = Vec::new();
        let deadline = tokio::time::Instant::now() + within;
        loop {
            match tokio::time::timeout_at(deadline, self.seen.recv()).await {
                Ok(Some(seen)) if seen.topic == topic && wanted(&seen) => return seen,
                Ok(Some(seen)) => other.push(seen),
                Ok(None) => panic!("the watcher's connection ended waiting for {topic}"),
                Err(_) => panic!(
                    "nothing wanted arrived on {topic} within {within:?}; what did: {other:#?}"
                ),
            }
        }
    }

    /// Every message until each `(topic, payload)` of `wanted` has arrived, in whatever order
    /// the broker sends them, waiting no longer than `within`; the ones wanted, as they came.
    pub async fn wait_for_each(&mut self, wanted: &[(&str, &str)], within: Duration) -> Vec<Seen> {
        let mut found: Vec<Option<Seen>> = vec![None; wanted.len()];
        let mut other = Vec::new();
        let deadline = tokio::time::Instant::now() + within;
        while found.iter().any(Option::is_none) {
            match tokio::time::timeout_at(deadline, self.seen.recv()).await {
                Ok(Some(seen)) => {
                    let at = wanted.iter().enumerate().position(|(i, (topic, payload))| {
                        found[i].is_none() && seen.topic == *topic && seen.payload == *payload
                    });
                    match at {
                        Some(i) => found[i] = Some(seen),
                        None => other.push(seen),
                    }
                }
                Ok(None) => panic!("the watcher's connection ended waiting for {wanted:?}"),
                Err(_) => {
                    let missing: Vec<_> = wanted
                        .iter()
                        .zip(&found)
                        .filter(|(_, f)| f.is_none())
                        .map(|(w, _)| w)
                        .collect();
                    panic!("{missing:?} never arrived within {within:?}; what did: {other:#?}")
                }
            }
        }
        found.into_iter().flatten().collect()
    }

    /// Everything that arrives within `within`, for a test that checks what did not.
    pub async fn collect(&mut self, within: Duration) -> Vec<Seen> {
        let mut all = Vec::new();
        let deadline = tokio::time::Instant::now() + within;
        while let Ok(Some(seen)) = tokio::time::timeout_at(deadline, self.seen.recv()).await {
            all.push(seen);
        }
        all
    }

    /// Publish `payload` on `topic`, not retained, as Home Assistant sends a command.
    pub async fn publish(&self, topic: &str, payload: &str) {
        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload.as_bytes().to_vec())
            .await
            .expect("the watcher publishes");
    }

    /// Publish `payload` on `topic`, retained, as something else on the broker might.
    pub async fn retain(&self, topic: &str, payload: &str) {
        self.client
            .publish(topic, QoS::AtLeastOnce, true, payload.as_bytes().to_vec())
            .await
            .expect("the watcher publishes");
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}
