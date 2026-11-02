// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The broker client against a real broker, `rumqttd` in this process on 127.0.0.1: what a
//! connect puts on the broker, what a lost connection leaves there, what a reconnect puts
//! back, and how long the client waits between attempts.

#[path = "support/broker.rs"]
mod broker;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use svolder_mqtt::{Client, ClientEvent, Link, Settings};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};

use broker::{TestBroker, Watcher, free_port};

/// How long one step of a case may wait before it is a failure rather than slow.
const STEP: Duration = Duration::from_secs(10);

/// The bridge's availability topic in these cases.
const AVAILABILITY: &str = "svolder/bridge/availability";

/// Settings for a client of the broker on 127.0.0.1:`port`.
fn settings(port: u16) -> Settings {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    Settings {
        host: "127.0.0.1".to_string(),
        port,
        client_id: format!("svolder-client-test-{}-{n}", std::process::id()),
        username: None,
        password: None,
        availability: AVAILABILITY.to_string(),
    }
}

/// Wait until the client's link is `wanted`.
async fn until_link(client: &Client, wanted: impl Fn(Link) -> bool) -> Link {
    let deadline = tokio::time::Instant::now() + STEP;
    loop {
        let link = client.link();
        if wanted(link) {
            return link;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the link never became what was wanted; it is {link:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The next event the client reports that `wanted` accepts.
async fn next_event(
    events: &mut mpsc::UnboundedReceiver<ClientEvent>,
    wanted: impl Fn(&ClientEvent) -> bool,
) -> ClientEvent {
    let deadline = tokio::time::Instant::now() + STEP;
    loop {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) if wanted(&event) => return event,
            Ok(Some(_)) => {}
            Ok(None) => panic!("the client's events ended"),
            Err(_) => panic!("no wanted event within {STEP:?}"),
        }
    }
}

/// A connect puts `online` on the bridge's topic, then every retained topic the client was
/// handed before there was a connection, and subscribes: a command published afterwards
/// arrives as a message.
#[tokio::test]
async fn a_connect_publishes_online_and_every_retained_topic_and_subscribes() {
    let broker = TestBroker::start();
    let (client, mut events) = Client::start(settings(broker.port));
    client.announce(
        "homeassistant/sensor/svolder_U/volume/config",
        "{\"name\":\"Volume\"}",
    );
    client.retain("svolder/U/state", "{\"volume\":-20.0}");
    client.retain("svolder/U/availability", "online");
    client.subscribe("svolder/U/+/set");
    next_event(&mut events, |e| *e == ClientEvent::Connected).await;
    assert_eq!(client.link(), Link::Connected);

    // A watcher that subscribes afterwards is given each one as the broker's retained copy.
    let mut watcher = Watcher::connect(broker.port, &["svolder/#", "homeassistant/#"]).await;
    let given = watcher
        .wait_for_each(
            &[
                (AVAILABILITY, "online"),
                (
                    "homeassistant/sensor/svolder_U/volume/config",
                    "{\"name\":\"Volume\"}",
                ),
                ("svolder/U/state", "{\"volume\":-20.0}"),
                ("svolder/U/availability", "online"),
            ],
            STEP,
        )
        .await;
    assert!(given.iter().all(|s| s.retained), "{given:#?}");

    watcher.publish("svolder/U/volume/set", "-30").await;
    let arrived = next_event(&mut events, |e| matches!(e, ClientEvent::Message { .. })).await;
    assert_eq!(
        arrived,
        ClientEvent::Message {
            topic: "svolder/U/volume/set".to_string(),
            payload: b"-30".to_vec(),
            retained: false,
        }
    );

    // A retained message is handed over as one, so its owner can tell it from a fresh one.
    watcher.retain("svolder/U/mute/set", "ON").await;
    let retained = next_event(
        &mut events,
        |e| matches!(e, ClientEvent::Message { topic, .. } if topic == "svolder/U/mute/set"),
    )
    .await;
    assert!(
        matches!(
            retained,
            ClientEvent::Message {
                retained: false,
                ..
            }
        ),
        "a publish that is live when it arrives is not the retained copy: {retained:?}"
    );
    let (again, mut again_events) = Client::start(settings(broker.port));
    again.subscribe("svolder/U/+/set");
    let copy = next_event(&mut again_events, |e| {
        matches!(e, ClientEvent::Message { .. })
    })
    .await;
    assert_eq!(
        copy,
        ClientEvent::Message {
            topic: "svolder/U/mute/set".to_string(),
            payload: b"ON".to_vec(),
            retained: true,
        }
    );
    again.stop(STEP).await;
    client.stop(STEP).await;
}

/// A message that is not retained goes out while connected and nowhere at all otherwise; a
/// cleared topic leaves the broker.
#[tokio::test]
async fn a_publish_is_not_retained_and_a_clear_empties_the_topic() {
    let broker = TestBroker::start();
    let (client, mut events) = Client::start(settings(broker.port));
    client.retain("svolder/U/state", "{\"volume\":-20.0}");
    next_event(&mut events, |e| *e == ClientEvent::Connected).await;
    let mut watcher = Watcher::connect(broker.port, &["svolder/U/#"]).await;
    watcher
        .wait_for("svolder/U/state", STEP, |s| s.retained)
        .await;

    client.publish("svolder/U/meters", "{\"meter_1\":-12.0}");
    let meters = watcher.wait_for("svolder/U/meters", STEP, |_| true).await;
    assert_eq!(meters.payload, "{\"meter_1\":-12.0}");
    client.clear("svolder/U/state");
    watcher
        .wait_for("svolder/U/state", STEP, |s| s.payload.is_empty())
        .await;

    // A fresh subscription is given nothing: the meters were never retained, and the state
    // has been cleared.
    let mut later = Watcher::connect(broker.port, &["svolder/U/#"]).await;
    let given = later.collect(Duration::from_millis(500)).await;
    assert!(
        given.is_empty(),
        "a fresh subscription was given {given:#?}"
    );
    client.stop(STEP).await;
}

/// A connection that drops without a goodbye leaves the last will, `offline` retained, on the
/// bridge's topic; the reconnect after the wait puts `online` and every retained topic back,
/// including one the broker had lost meanwhile.
#[tokio::test]
async fn a_lost_connection_leaves_the_will_and_a_reconnect_puts_everything_back() {
    let broker = TestBroker::start();
    let proxy = Proxy::start(broker.port).await;
    let (client, mut events) = Client::start(settings(proxy.port));
    client.announce(
        "homeassistant/switch/svolder_U/mute/config",
        "{\"name\":\"Mute\"}",
    );
    client.retain("svolder/U/state", "{\"mute\":\"OFF\"}");
    next_event(&mut events, |e| *e == ClientEvent::Connected).await;
    let mut watcher = Watcher::connect(broker.port, &["svolder/#", "homeassistant/#"]).await;
    watcher
        .wait_for(AVAILABILITY, STEP, |s| s.payload == "online")
        .await;

    proxy.refuse(true);
    proxy.cut();
    watcher
        .wait_for(AVAILABILITY, STEP, |s| s.payload == "offline")
        .await;
    let lost = next_event(&mut events, |e| matches!(e, ClientEvent::Lost { .. })).await;
    let ClientEvent::Lost { retry_in, .. } = lost else {
        unreachable!()
    };
    assert!(
        retry_in <= Duration::from_secs(2) && retry_in > Duration::from_secs(1),
        "the first wait is two seconds, got {retry_in:?}"
    );
    assert!(
        matches!(client.link(), Link::Waiting { .. }),
        "{:?}",
        client.link()
    );

    // What the broker holds is taken away while the client is gone, and a change is made
    // that has nowhere to go yet.
    watcher.retain("svolder/U/state", "").await;
    watcher
        .retain("homeassistant/switch/svolder_U/mute/config", "")
        .await;
    client.retain("svolder/U/state", "{\"mute\":\"ON\"}");
    proxy.refuse(false);

    next_event(&mut events, |e| *e == ClientEvent::Connected).await;
    watcher
        .wait_for(AVAILABILITY, STEP, |s| s.payload == "online")
        .await;
    watcher
        .wait_for("homeassistant/switch/svolder_U/mute/config", STEP, |s| {
            s.payload == "{\"name\":\"Mute\"}"
        })
        .await;
    watcher
        .wait_for("svolder/U/state", STEP, |s| {
            s.payload == "{\"mute\":\"ON\"}"
        })
        .await;
    client.stop(STEP).await;
}

/// A clear sent while connected is sent again on every connect, so one the connection lost
/// on its way to the broker is not lost for good.
#[tokio::test]
async fn a_clear_is_sent_again_on_a_reconnect() {
    let broker = TestBroker::start();
    let proxy = Proxy::start(broker.port).await;
    let (client, mut events) = Client::start(settings(proxy.port));
    client.announce(
        "homeassistant/sensor/svolder_U/volume/config",
        "{\"name\":\"Volume\"}",
    );
    next_event(&mut events, |e| *e == ClientEvent::Connected).await;
    let mut watcher = Watcher::connect(broker.port, &["homeassistant/#"]).await;
    let topic = "homeassistant/sensor/svolder_U/volume/config";
    watcher
        .wait_for(topic, STEP, |s| !s.payload.is_empty())
        .await;
    client.clear(topic);
    watcher
        .wait_for(topic, STEP, |s| s.payload.is_empty())
        .await;

    // The broker holds the config again, as it would had the clear never reached it.
    watcher.retain(topic, "{\"name\":\"Volume\"}").await;
    watcher
        .wait_for(topic, STEP, |s| !s.payload.is_empty())
        .await;
    proxy.cut();
    next_event(&mut events, |e| *e == ClientEvent::Connected).await;
    watcher
        .wait_for(topic, STEP, |s| s.payload.is_empty())
        .await;
    client.stop(STEP).await;
}

/// Nothing listening is tried again after two seconds, then four; a broker that appears on the
/// port is connected to on the attempt after.
#[tokio::test]
async fn a_broker_that_is_not_there_is_tried_again_after_two_seconds_then_four() {
    let port = free_port();
    let (client, mut events) = Client::start(settings(port));
    let mut waits = Vec::new();
    for _ in 0..2 {
        let lost = next_event(&mut events, |e| matches!(e, ClientEvent::Lost { .. })).await;
        let ClientEvent::Lost { retry_in, .. } = lost else {
            unreachable!()
        };
        waits.push(retry_in);
    }
    let rounded: Vec<u64> = waits
        .iter()
        .map(|w| (w.as_millis() as u64 + 500) / 1000)
        .collect();
    assert_eq!(rounded, [2, 4], "the waits were {waits:?}");
    let Link::Waiting { retry_at } = client.link() else {
        panic!("waiting between attempts, got {:?}", client.link());
    };
    assert!(retry_at > tokio::time::Instant::now());

    let broker = TestBroker::start();
    let _proxy = Proxy::on(port, broker.port).await;
    next_event(&mut events, |e| *e == ClientEvent::Connected).await;
    until_link(&client, |l| l == Link::Connected).await;
    client.stop(STEP).await;
}

/// A stop sends what was asked for before it, then `offline` for the bridge, then goes: the
/// broker keeps `offline` on both topics.
#[tokio::test]
async fn a_stop_says_offline_after_everything_asked_before_it() {
    let broker = TestBroker::start();
    let (client, mut events) = Client::start(settings(broker.port));
    client.retain("svolder/U/availability", "online");
    next_event(&mut events, |e| *e == ClientEvent::Connected).await;
    let mut watcher = Watcher::connect(broker.port, &["svolder/#"]).await;
    watcher
        .wait_for(AVAILABILITY, STEP, |s| s.payload == "online")
        .await;

    client.retain("svolder/U/availability", "offline");
    client.stop(STEP).await;
    watcher
        .wait_for("svolder/U/availability", STEP, |s| s.payload == "offline")
        .await;
    watcher
        .wait_for(AVAILABILITY, STEP, |s| s.payload == "offline")
        .await;

    let mut later = Watcher::connect(broker.port, &["svolder/#"]).await;
    let mut kept: Vec<(String, String)> = later
        .collect(Duration::from_millis(500))
        .await
        .into_iter()
        .map(|s| (s.topic, s.payload))
        .collect();
    kept.sort();
    assert_eq!(
        kept,
        [
            ("svolder/U/availability".to_string(), "offline".to_string()),
            (AVAILABILITY.to_string(), "offline".to_string()),
        ]
    );
}

/// A TCP relay between a client and the broker that a case can cut and close, which is a
/// lost connection and a broker that is away as both ends see them.
struct Proxy {
    port: u16,
    cut: watch::Sender<u64>,
    refusing: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Proxy {
    /// A relay on a port of its own to the broker on `upstream`.
    async fn start(upstream: u16) -> Proxy {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the relay");
        Proxy::serve(listener, upstream)
    }

    /// A relay on `port` to the broker on `upstream`.
    async fn on(port: u16, upstream: u16) -> Proxy {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("bind the relay on the port the client tries");
        Proxy::serve(listener, upstream)
    }

    fn serve(listener: TcpListener, upstream: u16) -> Proxy {
        let port = listener.local_addr().expect("a local address").port();
        let (cut, _) = watch::channel(0u64);
        let refusing = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn({
            let cut = cut.clone();
            let refusing = Arc::clone(&refusing);
            async move {
                while let Ok((mut inbound, _)) = listener.accept().await {
                    if refusing.load(Ordering::SeqCst) {
                        drop(inbound);
                        continue;
                    }
                    let mut cut = cut.subscribe();
                    tokio::spawn(async move {
                        let Ok(mut outbound) = TcpStream::connect(("127.0.0.1", upstream)).await
                        else {
                            return;
                        };
                        tokio::select! {
                            _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound) => {}
                            _ = cut.changed() => {}
                        }
                    });
                }
            }
        });
        Proxy {
            port,
            cut,
            refusing,
            task,
        }
    }

    /// Drop every connection the relay carries now.
    fn cut(&self) {
        self.cut.send_modify(|n| *n += 1);
    }

    /// Close each new connection as it arrives while `on`, which is a broker that is away.
    fn refuse(&self, on: bool) {
        self.refusing.store(on, Ordering::SeqCst);
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.cut();
        self.task.abort();
    }
}
