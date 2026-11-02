// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::writer::Glided;

/// A daemon with no devices over the fake transport, whose socket and files are paths
/// nothing reads: only its MQTT bridge is asked about.
pub(super) fn daemon() -> Daemon {
    let registry = Arc::new(Registry::builtin().expect("the built-in profiles load"));
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let transport = svolder_ble::FakeTransport::from_profile(
        &profile,
        "0102030405060708".parse().expect("a valid unit id"),
        3,
        svolder_proto::Passphrase::from_text("1234").expect("four digits"),
    );
    Daemon::new(
        Arc::new(transport),
        registry,
        PathBuf::from("/nonexistent/config.toml"),
        PathBuf::from("/nonexistent/profiles"),
        PathBuf::from("/nonexistent/svolder.sock"),
    )
}

/// A section for a broker on 127.0.0.1 at a port nothing listens on: the bridge connects
/// nowhere, and says it is connecting.
fn section() -> MqttConfig {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind a free port")
        .local_addr()
        .expect("a local address")
        .port();
    MqttConfig {
        enabled: true,
        host: Some("127.0.0.1".to_string()),
        port,
        ..MqttConfig::default()
    }
}

/// Once the daemon's stop has begun, a reload starts no bridge: neither one that reaches
/// `bridge_to` after the stop was marked, nor one whose handover the stop overtook and
/// that only reaches the point of starting the next bridge afterwards.
#[tokio::test]
async fn a_reload_after_the_stop_began_starts_no_bridge() {
    let daemon = daemon();
    let mqtt = section();
    daemon.bridge_to(&mqtt, &[]).await;
    assert!(
        matches!(daemon.mqtt_status(), MqttStatus::Connecting { .. }),
        "{:?}",
        daemon.mqtt_status()
    );

    daemon.shutdown().await;
    assert_eq!(daemon.mqtt_status(), MqttStatus::Off);
    daemon.bridge_to(&mqtt, &[]).await;
    assert_eq!(
        daemon.mqtt_status(),
        MqttStatus::Off,
        "a reload after the stop"
    );
    daemon.start_bridge(&mqtt, Vec::new(), &[]);
    assert_eq!(
        daemon.mqtt_status(),
        MqttStatus::Off,
        "a handover the stop overtook"
    );
}

/// A device handle whose actor answers every command with an acknowledgement after `delay`
/// and stays up for as long as the handle does.
fn slow_device(name: &str, delay: Duration) -> Arc<DeviceHandle> {
    let (commands, mut inbox) =
        tokio::sync::mpsc::channel::<(Command, oneshot::Sender<Response>)>(4);
    tokio::spawn(async move {
        while let Some((_, reply)) = inbox.recv().await {
            tokio::time::sleep(delay).await;
            let _ = reply.send(Response::Ack);
        }
    });
    let (events, _) = tokio::sync::broadcast::channel(4);
    let (_feed, feed) = tokio::sync::watch::channel(device::Feed::default());
    Arc::new(DeviceHandle {
        name: name.to_string(),
        udid: "0102030405060708".to_string(),
        commands,
        writer: crate::writer::Writer::unattended(),
        events,
        info: Arc::new(std::sync::Mutex::new(device::LiveInfo {
            signal: None,
            unit: "0102030405060708".to_string(),
            product_id: 0,
            product: None,
            firmware: 0,
            profile: String::new(),
            connected: true,
            stale: false,
            next_attempt: None,
            passphrase_note: None,
            refused: false,
            history: Arc::default(),
            ring: Arc::default(),
        })),
        feed,
    })
}

/// A reload asks every actor at once: with several actors that each take a second to answer,
/// it takes about one second, not the sum of them.
#[tokio::test(start_paused = true)]
async fn a_reload_asks_every_actor_at_once() {
    let daemon = daemon();
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = (0..5)
        .map(|i| slow_device(&format!("dev{i}"), Duration::from_secs(1)))
        .collect();
    let registry = Arc::clone(&daemon.registry.read().unwrap_or_else(|e| e.into_inner()));
    let started = tokio::time::Instant::now();
    daemon.reload(registry).await;
    let took = started.elapsed();
    assert!(
        took < Duration::from_millis(1500),
        "the reload took {took:?}, the sum of five one-second answers"
    );
}

/// A transport that records the window of every scan it is asked for and hears nothing.
struct WindowRecorder(std::sync::Mutex<Vec<Duration>>);

#[async_trait::async_trait]
impl Transport for WindowRecorder {
    async fn scan(
        &self,
        window: Duration,
    ) -> Result<svolder_ble::Advertisements, svolder_ble::BleError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(window);
        Ok(stream::empty().boxed())
    }

    async fn connect(
        &self,
        _unit: svolder_proto::UnitId,
    ) -> Result<Box<dyn svolder_ble::Link>, svolder_ble::BleError> {
        Err(svolder_ble::BleError::NotFound)
    }
}

/// A scan window longer than [`MAX_SCAN_WINDOW`] is listened for that long, and one within it
/// is listened for as asked.
#[tokio::test]
async fn a_scan_window_is_held_to_the_maximum() {
    let recorder = Arc::new(WindowRecorder(std::sync::Mutex::new(Vec::new())));
    let registry = Arc::new(Registry::builtin().expect("the built-in profiles load"));
    let daemon = Daemon::new(
        Arc::clone(&recorder) as Arc<dyn Transport>,
        registry,
        PathBuf::from("/nonexistent/config.toml"),
        PathBuf::from("/nonexistent/profiles"),
        PathBuf::from("/nonexistent/svolder.sock"),
    );
    for window in [
        Duration::from_secs(5),
        MAX_SCAN_WINDOW,
        Duration::from_secs(3600),
    ] {
        daemon.scan(window).await;
    }
    let heard = recorder.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        heard,
        [Duration::from_secs(5), MAX_SCAN_WINDOW, MAX_SCAN_WINDOW]
    );
}

/// A default naming no configured device is refused with the CLI's sentence, even when one
/// device is running, rather than answered by that device.
#[tokio::test]
async fn a_dangling_default_is_refused_even_with_one_device() {
    let daemon = daemon();
    {
        let mut set = daemon.devices.write().unwrap_or_else(|e| e.into_inner());
        set.devices = vec![slow_device("gungnir", Duration::ZERO)];
        set.default_device = Some("ghost".to_string());
    }
    let refused = daemon.resolve(None).err().expect("refused");
    assert_eq!(refused.code, 1);
    assert_eq!(
        refused.message,
        svolder_ipc::config::dangling_default_message("ghost")
    );
    assert!(daemon.resolve(Some("gungnir")).is_ok());
}

/// A device handle whose actor takes `queue_delay` before it reads its first command, answers
/// that one at once, and answers every later command after `reply_delay`.
fn stalled_device(queue_delay: Duration, reply_delay: Duration) -> Arc<DeviceHandle> {
    let (commands, mut inbox) =
        tokio::sync::mpsc::channel::<(Command, oneshot::Sender<Response>)>(1);
    // The queue's one slot is taken, so the next send waits for the actor to read.
    let (filler, _) = oneshot::channel();
    commands
        .try_send((Command::Shutdown, filler))
        .expect("the queue has room for one");
    tokio::spawn(async move {
        tokio::time::sleep(queue_delay).await;
        let _ = inbox.recv().await;
        while let Some((_, reply)) = inbox.recv().await {
            tokio::time::sleep(reply_delay).await;
            let _ = reply.send(Response::Ack);
        }
    });
    let handle = slow_device("slow", Duration::ZERO);
    Arc::new(DeviceHandle {
        name: handle.name.clone(),
        udid: handle.udid.clone(),
        commands,
        writer: crate::writer::Writer::unattended(),
        events: handle.events.clone(),
        info: Arc::clone(&handle.info),
        feed: handle.feed.clone(),
    })
}

/// A slow send and a slow reply share one deadline measured from the call: a send that takes
/// three seconds leaves a reply that takes three more only the two seconds that remain.
#[tokio::test(start_paused = true)]
async fn ask_within_spends_one_deadline_on_the_send_and_the_reply() {
    let daemon = daemon();
    let device = stalled_device(Duration::from_secs(3), Duration::from_secs(3));
    let started = tokio::time::Instant::now();
    let response = daemon
        .ask_within(&device, Command::Shutdown, Duration::from_secs(5))
        .await;
    let took = started.elapsed();
    assert!(
        matches!(response, Response::Error { code: 4, .. }),
        "got {response:?}"
    );
    assert_eq!(took, Duration::from_secs(5), "answered at {took:?}");
}

/// The deadline an actor is given to shut down outlasts the grace it gives a connect attempt
/// to stand down, so an actor closing mid-connect answers before the daemon's own settle step
/// runs rather than being given up on while it is still reclaiming.
#[tokio::test(start_paused = true)]
async fn the_shutdown_deadline_outlasts_the_close_grace() {
    assert!(SHUTDOWN_DEADLINE > svolder_ble::FAREWELL_BOUND);

    let daemon = daemon();
    let device = slow_device("reclaiming", svolder_ble::CLOSE_GRACE);
    let response = daemon
        .ask_within(&device, Command::Shutdown, SHUTDOWN_DEADLINE)
        .await;
    assert!(matches!(response, Response::Ack), "got {response:?}");
}

/// An actor that closes after twice the close grace, as one stopped mid-connect can, is waited
/// for: the daemon's shutdown returns only once it has answered, so the settle step that
/// follows never runs under an actor still reclaiming.
#[tokio::test(start_paused = true)]
async fn shutdown_waits_for_an_actor_that_reclaims_for_twice_the_close_grace() {
    let daemon = daemon();
    let answered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let slow = slow_device("reclaiming", svolder_ble::CLOSE_GRACE * 2);
    let (commands, mut inbox) =
        tokio::sync::mpsc::channel::<(Command, oneshot::Sender<Response>)>(4);
    let flag = Arc::clone(&answered);
    tokio::spawn(async move {
        while let Some((_, reply)) = inbox.recv().await {
            tokio::time::sleep(svolder_ble::CLOSE_GRACE * 2).await;
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = reply.send(Response::Ack);
        }
    });
    let device = Arc::new(DeviceHandle {
        name: slow.name.clone(),
        udid: slow.udid.clone(),
        commands,
        writer: crate::writer::Writer::unattended(),
        events: slow.events.clone(),
        info: Arc::clone(&slow.info),
        feed: slow.feed.clone(),
    });
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![device];
    daemon.shutdown().await;
    assert!(
        answered.load(std::sync::atomic::Ordering::SeqCst),
        "the shutdown returned before the actor answered"
    );
}

/// `history` for a device the daemon does not run is the usual unknown-device refusal, and
/// for one that has never been connected an empty history, not an error.
#[tokio::test]
async fn history_refuses_an_unknown_device_and_is_empty_for_a_quiet_one() {
    let daemon = daemon();
    let refused = daemon
        .handle(Request::History {
            device: Some("nope".into()),
            span_secs: 60,
        })
        .await;
    assert!(
        matches!(refused, Response::Error { code: 2, .. }),
        "got {refused:?}"
    );

    let quiet = slow_device("quiet", Duration::ZERO);
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![quiet];
    let reply = daemon
        .handle(Request::History {
            device: Some("quiet".into()),
            span_secs: 60,
        })
        .await;
    assert!(
        matches!(
            &reply,
            Response::History { device, transitions, .. }
                if device == "quiet" && transitions.is_empty()
        ),
        "got {reply:?}"
    );
}

/// A device handle on a gungnir2-v2 whose writer is the real one, over an actor that answers
/// every command with an acknowledgement once `gate` lets it; the gate starts shut.
fn gated_device(name: &str) -> (Arc<DeviceHandle>, Arc<tokio::sync::Semaphore>) {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = Arc::new(registry.get("gungnir2-v2").expect("a built-in").clone());
    let (commands, mut inbox) =
        tokio::sync::mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let opened = Arc::clone(&gate);
    tokio::spawn(async move {
        while let Some((_, reply)) = inbox.recv().await {
            opened.acquire().await.expect("the gate").forget();
            let _ = reply.send(Response::Ack);
        }
    });
    let (events, _) = tokio::sync::broadcast::channel(4);
    let (_feed, feed) = tokio::sync::watch::channel(device::Feed {
        name: name.to_string(),
        profile: Some(profile),
        connected: true,
        ..device::Feed::default()
    });
    let slow = slow_device(name, Duration::ZERO);
    let handle = Arc::new(DeviceHandle {
        name: name.to_string(),
        udid: slow.udid.clone(),
        writer: crate::writer::Writer::spawn(commands.clone(), feed.clone()),
        commands,
        events,
        info: Arc::clone(&slow.info),
        feed,
    });
    (handle, gate)
}

/// The presets that fit `device`'s tone control.
fn fitting_of(device: &DeviceHandle) -> Vec<Preset> {
    let eq = device
        .feed
        .borrow()
        .profile
        .as_ref()
        .and_then(|profile| profile.eq.clone())
        .expect("a tone control");
    svolder_profiles::tone::preset::fitting(&eq, &[])
        .into_iter()
        .map(|(preset, _)| preset)
        .collect()
}

fn band_write() -> Request {
    Request::EqSet {
        device: None,
        band: 1,
        param: "g".into(),
        value: 5.0,
        no_wait: false,
    }
}

/// A band write a socket client sends goes through the device's writer: while a recall runs, it
/// is held there, and it is answered once the recall is done.
#[tokio::test]
async fn a_socket_band_write_is_held_by_the_writer_during_a_recall() {
    let daemon = daemon();
    let (device, gate) = gated_device("amp");
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![Arc::clone(&device)];
    let recalling = device.writer.recall(
        crate::writer::Origin::Mqtt,
        "Warm".into(),
        fitting_of(&device),
    );
    let mut call = Box::pin(daemon.handle_from(crate::writer::Origin::Socket, band_write()));
    assert!(futures::poll!(call.as_mut()).is_pending());
    assert_eq!(device.writer.view().await.held, 1);
    gate.add_permits(1000);
    let response = call.await;
    assert!(matches!(response, Response::Ack), "got {response:?}");
    assert!(matches!(
        recalling.done.await,
        Ok(Outcome::Done(Response::Ack))
    ));
}

/// A write for a device whose writer has stopped is answered that the device is not running.
#[tokio::test]
async fn a_write_for_a_device_whose_writer_has_stopped_says_it_is_not_running() {
    let daemon = daemon();
    let stopped = slow_device("amp", Duration::ZERO);
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![stopped];
    let response = daemon
        .handle_from(crate::writer::Origin::Socket, band_write())
        .await;
    let Response::Error { code: 3, message } = &response else {
        panic!("got {response:?}");
    };
    assert!(message.contains("is not running"), "{message}");
}

/// A write an actor never answers is given up on, with the actor's silence as the answer, once
/// the deadline has passed.
#[tokio::test(start_paused = true)]
async fn a_write_an_actor_never_answers_is_answered_when_the_deadline_passes() {
    let daemon = daemon();
    let (device, _gate) = gated_device("amp");
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![device];
    let started = tokio::time::Instant::now();
    let response = daemon
        .handle_from(crate::writer::Origin::Socket, band_write())
        .await;
    let Response::Error { code: 4, message } = &response else {
        panic!("got {response:?}");
    };
    assert!(message.contains("did not answer"), "{message}");
    assert_eq!(started.elapsed(), ACTOR_DEADLINE);
}

/// An MCP recall a later recall replaces before it started is told so, not left waiting.
#[tokio::test]
async fn an_mcp_recall_replaced_before_it_started_says_so() {
    use crate::fleet::Fleet;
    let daemon = daemon().shared();
    let (device, gate) = gated_device("amp");
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![Arc::clone(&device)];
    let fitting = fitting_of(&device);
    let running = device
        .writer
        .recall(crate::writer::Origin::Mqtt, "Warm".into(), fitting.clone());
    let fleet = Arc::new(crate::fleet::DaemonFleet::new(
        Arc::downgrade(&daemon),
        Origin::Mcp,
        Mode::Ordered,
    ));
    let info = device.describe();
    let asked = {
        let (fleet, fitting) = (Arc::clone(&fleet), fitting.clone());
        tokio::spawn(async move { fleet.recall(&info, "Bass", fitting).await })
    };
    while device.writer.view().await.recall_waiting.is_none() {
        tokio::task::yield_now().await;
    }
    let later = device
        .writer
        .recall(crate::writer::Origin::Mqtt, "Presence".into(), fitting);
    let answer = asked.await.expect("the call ends");
    assert!(
        matches!(&answer, Err(why) if why.contains("took this one's place before it started")),
        "got {:?}",
        answer.map(|(_, response)| response)
    );
    gate.add_permits(1000);
    assert!(matches!(
        running.done.await,
        Ok(Outcome::Done(Response::Ack))
    ));
    assert!(matches!(later.done.await, Ok(Outcome::Done(Response::Ack))));
}

/// A built-in setting asked of a device that has none is answered in words, through the fleet.
#[tokio::test]
async fn a_setting_asked_of_a_device_without_builtin_settings_is_refused_in_words() {
    use crate::fleet::Fleet;
    let daemon = daemon().shared();
    let (device, _gate) = gated_device("amp");
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![Arc::clone(&device)];
    let fleet = crate::fleet::DaemonFleet::new(Arc::downgrade(&daemon), Origin::Mcp, Mode::Ordered);
    let answer = fleet.apply(&device.describe(), "Flat").await;
    assert!(
        matches!(&answer, Err(why) if why.starts_with("amp has no device presets")),
        "got {:?}",
        answer.map(|(_, response)| response)
    );
}

/// A device whose actor answers every write once `gate` has a permit, and whose volume reads
/// raw step 63, so a glide has a reading to judge.
fn volume_device(name: &str) -> (Arc<DeviceHandle>, Arc<tokio::sync::Semaphore>) {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = Arc::new(registry.get("gungnir2-v2").expect("a built-in").clone());
    let (commands, mut inbox) =
        tokio::sync::mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let opened = Arc::clone(&gate);
    let m = svolder_profiles::intent(&profile, "volume", "63").expect("a step");
    let (events, _) = tokio::sync::broadcast::channel(4);
    let (feed_in, feed) = tokio::sync::watch::channel(device::Feed {
        name: name.to_string(),
        profile: Some(profile),
        connected: true,
        readings: vec![svolder_proto::Message::status(m.id, m.value)],
        ..device::Feed::default()
    });
    let known = Arc::clone(feed.borrow().profile.as_ref().expect("a profile"));
    tokio::spawn(async move {
        while let Some((command, reply)) = inbox.recv().await {
            opened.acquire().await.expect("the gate").forget();
            // The device echoes a set as a reading and answers it with the value, as a real
            // one does.
            let echoed = match command {
                Command::Set { key, value, .. } => {
                    svolder_profiles::intent(&known, &key, &value).ok()
                }
                _ => None,
            };
            let Some(m) = echoed else {
                let _ = reply.send(Response::Ack);
                continue;
            };
            let status = svolder_proto::Message::status(m.id, m.value);
            feed_in.send_modify(|feed| {
                feed.readings.retain(|r| r.id != status.id);
                feed.readings.push(status.clone());
            });
            let reading = svolder_profiles::describe(&known, &status);
            let _ = reply.send(Response::Value(svolder_ipc::ValueObject::from_reading(
                &reading, &status,
            )));
        }
    });
    let slow = slow_device(name, Duration::ZERO);
    let handle = Arc::new(DeviceHandle {
        name: name.to_string(),
        udid: slow.udid.clone(),
        writer: crate::writer::Writer::spawn(commands.clone(), feed.clone()),
        commands,
        events,
        info: Arc::clone(&slow.info),
        feed,
    });
    (handle, gate)
}

/// A glide through a `DaemonFleet` is the fleet's origin's: its climb is owned by it, so that
/// origin's next glide steers the climb and another origin's would start one of its own.
#[tokio::test(start_paused = true)]
async fn a_fleet_glide_climbs_for_the_origin_the_fleet_carries() {
    use crate::fleet::Fleet;
    let daemon = daemon().shared();
    let (device, gate) = volume_device("amp");
    gate.add_permits(1000);
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![Arc::clone(&device)];
    let fleet = crate::fleet::DaemonFleet::new(Arc::downgrade(&daemon), Origin::Mqtt, Mode::Latest);
    let first = fleet.glide(&device.describe(), "volume", 127).await;
    assert!(matches!(first, Glided::Climbing { .. }), "{first:?}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let steered = device
        .writer
        .glide(Origin::Mqtt, "volume".into(), 120, Mode::Latest)
        .await
        .expect("an answer");
    assert!(
        matches!(
            steered,
            Glided::Climbing {
                first: None,
                target: 120
            }
        ),
        "{steered:?}"
    );
    tokio::time::sleep(Duration::from_millis(1000)).await;
    // An owner's glide steers the climb on the pace it has, so the volume is where the one
    // climb has paid for; another origin's glide would start a climb of its own, a step behind.
    let reached = device
        .feed
        .borrow()
        .readings
        .iter()
        .find(|r| r.id.to_string() == "10")
        .map(|r| r.value.clone());
    assert_eq!(reached, Some(svolder_proto::Value::U8(111)));
}

/// A glide through a `DaemonFleet` waits as the fleet's mode says: through a `Latest` fleet a
/// write waiting behind one in flight is replaced by the next.
#[tokio::test]
async fn a_fleet_glide_waits_in_the_mode_the_fleet_carries() {
    use crate::fleet::Fleet;
    let daemon = daemon().shared();
    let (device, gate) = volume_device("amp");
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![Arc::clone(&device)];
    let fleet = Arc::new(crate::fleet::DaemonFleet::new(
        Arc::downgrade(&daemon),
        Origin::Mqtt,
        Mode::Latest,
    ));
    let info = device.describe();
    let glide = |raw: i64| {
        let (fleet, info) = (Arc::clone(&fleet), info.clone());
        tokio::spawn(async move { fleet.glide(&info, "volume", raw).await })
    };
    let in_flight = glide(70);
    while device.writer.view().await.in_flight.is_empty() {
        tokio::task::yield_now().await;
    }
    let replaced = glide(71);
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    let replacing = glide(72);
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    gate.add_permits(1000);
    let replaced = replaced.await.expect("the call ends");
    assert!(matches!(replaced, Glided::Superseded), "{replaced:?}");
    assert!(matches!(in_flight.await.unwrap(), Glided::Landed(_)));
    assert!(matches!(replacing.await.unwrap(), Glided::Landed(_)));
}

/// A `recalled` request for a device the daemon does not have is refused as any request is.
#[tokio::test]
async fn a_recalled_request_for_an_unknown_device_is_refused() {
    let daemon = daemon();
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![slow_device("quiet", Duration::ZERO)];
    let answer = daemon
        .handle_from(
            crate::writer::Origin::Socket,
            Request::Recalled {
                device: Some("nope".into()),
                name: "Warm".into(),
            },
        )
        .await;
    assert!(
        matches!(answer, Response::Error { code: 2, .. }),
        "{answer:?}"
    );
}

/// A `[web]` section turns the web UI on, a reload that turns it off closes it, and `version`
/// says so each time.
#[tokio::test]
async fn the_web_ui_follows_its_section_and_version_says_where_it_stands() {
    let daemon = daemon().shared();
    let on = svolder_ipc::config::WebConfig {
        enabled: true,
        listen: "127.0.0.1:0".parse().unwrap(),
        auth: svolder_ipc::config::WebAuth::None,
        ..svolder_ipc::config::WebConfig::default()
    };
    daemon.web_to(&on).await;
    let Response::Version {
        web: Some(svolder_ipc::WebStatus::Listening { .. }),
        ..
    } = daemon.handle_from(Origin::Socket, Request::Version).await
    else {
        panic!("the web UI is not listening");
    };
    daemon
        .web_to(&svolder_ipc::config::WebConfig::default())
        .await;
    let Response::Version { web, .. } = daemon.handle_from(Origin::Socket, Request::Version).await
    else {
        panic!("no version");
    };
    assert_eq!(web, Some(svolder_ipc::WebStatus::Off));
    daemon.shutdown().await;
}

/// A web UI that cannot bind fails without touching the devices, and the next reload with the
/// same section starts it once the port is free: a failed server is never kept.
#[tokio::test]
async fn a_failed_web_ui_recovers_on_the_next_reload_and_the_devices_stay() {
    let daemon = daemon().shared();
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let section = svolder_ipc::config::WebConfig {
        enabled: true,
        listen: held.local_addr().unwrap(),
        auth: svolder_ipc::config::WebAuth::None,
        ..svolder_ipc::config::WebConfig::default()
    };
    let devices = daemon.handle_from(Origin::Socket, Request::Devices).await;
    daemon.web_to(&section).await;
    let Response::Version { web, .. } = daemon.handle_from(Origin::Socket, Request::Version).await
    else {
        panic!("no version");
    };
    assert!(
        matches!(web, Some(svolder_ipc::WebStatus::Failed { .. })),
        "{web:?}"
    );
    assert_eq!(
        daemon.handle_from(Origin::Socket, Request::Devices).await,
        devices
    );
    drop(held);
    daemon.web_to(&section).await;
    let Response::Version { web, .. } = daemon.handle_from(Origin::Socket, Request::Version).await
    else {
        panic!("no version");
    };
    assert!(
        matches!(web, Some(svolder_ipc::WebStatus::Listening { .. })),
        "{web:?}"
    );
    assert_eq!(
        daemon.handle_from(Origin::Socket, Request::Devices).await,
        devices
    );
    daemon.shutdown().await;
}

/// One GET of `/` from the web UI at `addr`, as a browser holding `cookie` would send it: the
/// answer's whole text.
async fn web_page(addr: std::net::SocketAddr, cookie: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let request = format!(
        "GET / HTTP/1.1\r\nHost: {addr}\r\nCookie: svolder_web_{}={cookie}\r\n\
         Connection: close\r\n\r\n",
        addr.port()
    );
    stream.write_all(request.as_bytes()).await.expect("send");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await.expect("read");
    String::from_utf8_lossy(&answer).into_owned()
}

/// Where the web UI listens and the tail of the token it reports, from `version`.
async fn web_standing(daemon: &Arc<Daemon>) -> (svolder_ipc::WebStatus, Option<String>) {
    let Response::Version {
        web: Some(web),
        web_token,
        ..
    } = daemon.handle_from(Origin::Socket, Request::Version).await
    else {
        panic!("no version");
    };
    (web, web_token.map(|info| info.tail))
}

/// Replace `path`'s contents with `value`, mode 0600.
fn write_secret(path: &std::path::Path, value: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, value).expect("write the token");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
}

/// A reload that leaves `[web]` as it was reads the token file again: a token that was replaced
/// signs every browser out at once and `version` shows its new tail; a file that can no longer
/// be read safely stops the server and `version` says it failed.
#[tokio::test]
async fn a_web_token_that_was_replaced_signs_every_browser_out_at_once() {
    use crate::web::auth::{mint, now_secs};
    const OLD: &str = "the-first-web-token-0123456789abcdef";
    const NEW: &str = "the-second-web-token-0123456789abcxyz";
    let dir = std::env::temp_dir().join(format!("svolderd-web-rotate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    let file = dir.join("web-token");
    write_secret(&file, OLD);
    let daemon = daemon().shared();
    let section = svolder_ipc::config::WebConfig {
        enabled: true,
        listen: "127.0.0.1:0".parse().unwrap(),
        token_file: Some(file.display().to_string()),
        ..svolder_ipc::config::WebConfig::default()
    };
    daemon.web_to(&section).await;
    let (status, tail) = web_standing(&daemon).await;
    let svolder_ipc::WebStatus::Listening { listen: addr, .. } = status else {
        panic!("not listening: {status:?}");
    };
    assert_eq!(tail.as_deref(), Some("def"));
    let old_cookie = mint(OLD, now_secs());
    assert!(
        web_page(addr, &old_cookie)
            .await
            .contains("id=\"svolder-page\"")
    );

    write_secret(&file, NEW);
    daemon.web_to(&section).await;
    let (status, tail) = web_standing(&daemon).await;
    assert_eq!(
        status,
        svolder_ipc::WebStatus::Listening {
            listen: addr,
            auth: section.auth,
            writes: section.writes,
            guest: section.guest,
        },
        "the port is kept"
    );
    assert_eq!(tail.as_deref(), Some("xyz"));
    assert!(
        !web_page(addr, &old_cookie)
            .await
            .contains("id=\"svolder-page\"")
    );
    let new_cookie = mint(NEW, now_secs());
    assert!(
        web_page(addr, &new_cookie)
            .await
            .contains("id=\"svolder-page\"")
    );

    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    daemon.web_to(&section).await;
    let (status, tail) = web_standing(&daemon).await;
    assert!(
        matches!(status, svolder_ipc::WebStatus::Failed { .. }),
        "{status:?}"
    );
    assert_eq!(tail, None);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// The daemon hands out web codes while its web UI runs, and says how to turn it on otherwise.
#[tokio::test]
async fn web_codes_come_from_the_running_web_ui() {
    let daemon = daemon().shared();
    let off = daemon.handle_from(Origin::Socket, Request::WebPair).await;
    assert_eq!(
        off,
        Response::Error {
            code: 1,
            message: svolder_ipc::WEB_OFF.to_string()
        }
    );
    let on = svolder_ipc::config::WebConfig {
        enabled: true,
        listen: "127.0.0.1:0".parse().unwrap(),
        auth: svolder_ipc::config::WebAuth::None,
        ..svolder_ipc::config::WebConfig::default()
    };
    daemon.web_to(&on).await;
    let Response::WebPair {
        code,
        expires_in_secs,
    } = daemon.handle_from(Origin::Socket, Request::WebPair).await
    else {
        panic!("no pairing code");
    };
    assert_eq!((code.len(), expires_in_secs), (6, 300));
    let Response::WebLink { code, .. } = daemon.handle_from(Origin::Socket, Request::WebLink).await
    else {
        panic!("no link code");
    };
    assert_eq!(code.len(), 22);
    daemon.shutdown().await;
}

/// The daemon answers whether a link code is open from its web UI's codes, and says how to turn
/// the web UI on while it runs none.
#[tokio::test]
async fn a_link_code_is_asked_about_without_being_spent() {
    let daemon = daemon().shared();
    let ask = |code: String| Request::WebLinkOpen { code };
    let off = daemon
        .handle_from(Origin::Socket, ask("AAAAAAAAAAAAAAAAAAAAAA".into()))
        .await;
    assert_eq!(
        off,
        Response::Error {
            code: 1,
            message: svolder_ipc::WEB_OFF.to_string()
        }
    );
    let on = svolder_ipc::config::WebConfig {
        enabled: true,
        listen: "127.0.0.1:0".parse().unwrap(),
        auth: svolder_ipc::config::WebAuth::None,
        ..svolder_ipc::config::WebConfig::default()
    };
    daemon.web_to(&on).await;
    let Response::WebLink { code, .. } = daemon.handle_from(Origin::Socket, Request::WebLink).await
    else {
        panic!("no link code");
    };
    for _ in 0..2 {
        assert_eq!(
            daemon.handle_from(Origin::Socket, ask(code.clone())).await,
            Response::WebLinkOpen { open: true }
        );
    }
    assert_eq!(
        daemon
            .handle_from(Origin::Socket, ask("AAAAAAAAAAAAAAAAAAAAAA".into()))
            .await,
        Response::WebLinkOpen { open: false }
    );
    daemon.shutdown().await;
}

/// A daemon over the fake transport that reads `config` and the profiles in `profiles` when it
/// reloads.
pub(super) fn daemon_over(config: PathBuf, profiles: PathBuf) -> Daemon {
    let base = daemon();
    Daemon::new(
        Arc::clone(&base.transport),
        read(&base.registry).clone(),
        config,
        profiles,
        PathBuf::from("/nonexistent/svolder.sock"),
    )
}

/// Two different free loopback ports: both are held while the second is chosen.
fn two_free_ports() -> (u16, u16) {
    let first = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a free port");
    let second = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a free port");
    (
        first.local_addr().expect("a local address").port(),
        second.local_addr().expect("a local address").port(),
    )
}

/// One reload that swaps the ports of `[mcp]` and `[web]` leaves both listening on the other's
/// old port: neither server starts while the other still holds the port it wants.
///
/// `two_free_ports` frees its pick before the reload binds it, so another test binding port 0 in
/// between can take one: the pick and both reloads are tried again, up to five times with fresh
/// ports, when a status says the address is in use. A swap that does not work says that every
/// time, and fails on the last try.
#[tokio::test]
async fn one_reload_swaps_the_ports_of_the_mcp_server_and_the_web_ui() {
    use svolder_ipc::{McpStatus, WebStatus};
    const TRIES: usize = 5;
    let dir = std::env::temp_dir().join(format!("svolderd-swap-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    let config = dir.join("config.toml");
    let write = |mcp: u16, web: u16| {
        std::fs::write(
            &config,
            format!(
                "[mcp]\nenabled = true\nlisten = \"127.0.0.1:{mcp}\"\nauth = \"none\"\n\n\
                 [web]\nenabled = true\nlisten = \"127.0.0.1:{web}\"\nauth = \"none\"\n"
            ),
        )
        .unwrap();
    };
    let listening =
        |port: u16| -> std::net::SocketAddr { format!("127.0.0.1:{port}").parse().unwrap() };
    let in_use = |error: &str| error.contains("in use");
    for tries_left in (0..TRIES).rev() {
        let (a, b) = two_free_ports();
        let daemon = daemon_over(config.clone(), dir.join("profiles")).shared();
        write(a, b);
        daemon.reload_all().await.expect("the first reload");
        write(b, a);
        daemon.reload_all().await.expect("the swapping reload");
        let Response::Version { mcp, web, .. } =
            daemon.handle_from(Origin::Socket, Request::Version).await
        else {
            panic!("no version");
        };
        let taken = matches!(&mcp, Some(McpStatus::Failed { error, .. }) if in_use(error))
            || matches!(&web, Some(WebStatus::Failed { error, .. }) if in_use(error));
        if taken && tries_left > 0 {
            daemon.shutdown().await;
            continue;
        }
        assert!(
            matches!(mcp, Some(McpStatus::Listening { listen, .. }) if listen == listening(b)),
            "{mcp:?}"
        );
        assert!(
            matches!(web, Some(WebStatus::Listening { listen, .. }) if listen == listening(a)),
            "{web:?}"
        );
        daemon.shutdown().await;
        break;
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The one-entry config of a device with the unit id `udid`, named `amp`.
pub(super) fn amp_entry(udid: &str) -> DeviceConfig {
    DeviceConfig {
        name: "amp".into(),
        udid: udid.into(),
        profile: None,
        passphrase: "1234".into(),
        inputs: Default::default(),
    }
}

/// Identify asked of a device the daemon is waiting to reach is answered not connected, at
/// once, and leaves the wait as it was: it ends no wait and starts no attempt, which the
/// transport's count of scans and the wait the device reports both say after the clock has moved.
#[tokio::test(start_paused = true)]
async fn identify_on_a_waiting_device_is_not_connected_and_keeps_the_wait() {
    let daemon = daemon();
    let recorder = Arc::new(WindowRecorder(std::sync::Mutex::new(Vec::new())));
    let transport: Arc<dyn Transport> = Arc::clone(&recorder) as Arc<dyn Transport>;
    let scans = || recorder.0.lock().unwrap_or_else(|e| e.into_inner()).len();
    let handle = device::spawn(
        Arc::clone(&transport),
        Arc::new(Registry::builtin().expect("the built-in profiles load")),
        amp_entry("0102030405060708"),
        Arc::new(crate::scanner::Scanner::new(transport)),
    )
    .expect("the actor starts");
    // The first attempt finds nothing and the actor settles into its first wait.
    tokio::time::sleep(Duration::from_millis(1)).await;
    let handle = Arc::new(handle);
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![Arc::clone(&handle)];
    let attempts = scans();
    assert!(
        handle.describe().retry_in_secs.is_some(),
        "the device is waiting between attempts"
    );

    let answer = daemon.handle(Request::Identify { device: None }).await;
    assert_eq!(
        answer,
        Response::Error {
            code: 3,
            message: "device not connected".into()
        }
    );
    // A command that woke the device would have it scanning again by now, and its wait started
    // over from the full step.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(scans(), attempts, "identify woke the device");
    assert_eq!(
        handle.describe().retry_in_secs,
        Some(1),
        "the wait did not run on from where it was"
    );
}

/// Identify asked of a linked device sends one frame and answers sent; asked again at once it
/// answers just sent and sends nothing more.
#[tokio::test(start_paused = true)]
async fn identify_on_a_linked_fake_answers_sent() {
    let daemon = daemon();
    let registry = Arc::new(Registry::builtin().expect("the built-in profiles load"));
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let unit: svolder_proto::UnitId = "0102030405060708".parse().expect("a valid unit id");
    let fake = svolder_ble::FakeTransport::from_profile(
        &profile,
        unit,
        3,
        svolder_proto::Passphrase::from_text("1234").expect("four digits"),
    );
    let seen = fake.device();
    let transport: Arc<dyn Transport> = Arc::new(fake);
    let handle = Arc::new(
        device::spawn(
            Arc::clone(&transport),
            registry,
            amp_entry("0102030405060708"),
            Arc::new(crate::scanner::Scanner::new(transport)),
        )
        .expect("the actor starts"),
    );
    while !handle.describe().connected {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![Arc::clone(&handle)];
    let connected = seen.lock().unwrap().commands_seen();

    for sent in [true, false] {
        let answer = daemon
            .handle(Request::Identify {
                device: Some("amp".into()),
            })
            .await;
        assert_eq!(
            answer,
            Response::Identify {
                device: "amp".into(),
                sent
            }
        );
    }
    assert_eq!(
        seen.lock().unwrap().commands_seen(),
        connected + 1,
        "one frame reached the device"
    );
}

/// Identify asked of a device that has refused the passphrase, on the link the daemon holds,
/// still goes out: the answer says sent and the fake device is handed function 200.
#[tokio::test(start_paused = true)]
async fn identify_on_a_refused_but_held_link_answers_sent_and_reaches_the_device() {
    let daemon = daemon();
    let registry = Arc::new(Registry::builtin().expect("the built-in profiles load"));
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let unit: svolder_proto::UnitId = "0102030405060708".parse().expect("a valid unit id");
    let fake = svolder_ble::FakeTransport::from_profile(
        &profile,
        unit,
        3,
        svolder_proto::Passphrase::from_text("1234").expect("four digits"),
    );
    let tap = svolder_ble::FrameTap::new();
    let transport: Arc<dyn Transport> = Arc::new(svolder_ble::TappedTransport::new(
        Arc::new(fake),
        tap.clone(),
    ));
    // The entry holds a code the device does not.
    let mut entry = amp_entry("0102030405060708");
    entry.passphrase = "9999".into();
    let handle = Arc::new(
        device::spawn(
            Arc::clone(&transport),
            registry,
            entry,
            Arc::new(crate::scanner::Scanner::new(transport)),
        )
        .expect("the actor starts"),
    );
    while !handle.describe().connected {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    daemon
        .devices
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .devices = vec![Arc::clone(&handle)];
    assert!(handle.describe().refused, "the device refused the code");

    let answer = daemon
        .handle(Request::Identify {
            device: Some("amp".into()),
        })
        .await;
    assert_eq!(
        answer,
        Response::Identify {
            device: "amp".into(),
            sent: true
        }
    );
    let identifies = tap
        .frames()
        .iter()
        .filter(|f| f.direction == svolder_ble::FrameDirection::Sent)
        .filter_map(|f| svolder_proto::decode_command(&f.bytes).ok())
        .filter(|d| {
            d.body
                .messages
                .iter()
                .any(|m| m.id == svolder_proto::FunctionId::one(200))
        })
        .count();
    assert_eq!(identifies, 1, "function 200 reached the device once");
}
