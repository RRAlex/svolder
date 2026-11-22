// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::commands::{clamp_lines, command_target, set_value};
use super::discovery::{discovery_unit, fitting_presets, subscriptions};
use super::*;
use crate::device::Command;
use crate::recall;
use crate::test_support::shown;
use std::sync::atomic::{AtomicUsize, Ordering};
use svolder_ipc::CODE_LINK;
use svolder_ipc::Response;
use svolder_profiles::{EqMode, tone::preset};
use svolder_proto::Value;

/// A broker in this process and a client that watches it, for the tests that look at what the
/// bridge's client sends rather than at what the task holds.
#[cfg(feature = "broker-tests")]
#[path = "../../../../crates/svolder-mqtt/tests/support/broker.rs"]
mod broker;

#[test]
fn a_discovery_topic_names_the_unit_it_is_under() {
    assert_eq!(
        discovery_unit(
            "homeassistant",
            "homeassistant/number/svolder_0102030405060708/volume/config"
        ),
        Some("0102030405060708")
    );
    for other in [
        "homeassistant/number/other_0102030405060708/volume/config",
        "homeassistant/number/svolder_0102030405060708/volume/state",
        "ha/number/svolder_0102030405060708/volume/config",
        "svolder/0102030405060708/volume/set",
    ] {
        assert_eq!(discovery_unit("homeassistant", other), None, "{other}");
    }
    assert_eq!(
        discovery_unit("", "/number/svolder_01/volume/config"),
        None,
        "discovery is off"
    );
}

#[test]
fn a_read_only_bridge_subscribes_to_no_command_topic() {
    let unit = "0102030405060708";
    let mut config = MqttConfig::default();
    assert_eq!(
        subscriptions(&config, unit),
        ["homeassistant/+/svolder_0102030405060708/+/config"]
    );
    config.commands = true;
    assert_eq!(
        subscriptions(&config, unit),
        [
            "homeassistant/+/svolder_0102030405060708/+/config",
            "svolder/0102030405060708/+/set"
        ]
    );
    config.commands = false;
    config.discovery_prefix = String::new();
    assert!(subscriptions(&config, unit).is_empty());
}

#[test]
fn a_command_topic_names_the_unit_and_the_key() {
    assert_eq!(
        command_target("svolder", "svolder/0102030405060708/volume/set"),
        Some(("0102030405060708", "volume"))
    );
    for other in [
        "svolder/0102030405060708/state",
        "svolder/0102030405060708/volume/get",
        "svolder/0102030405060708/a/b/set",
        "elsewhere/0102030405060708/volume/set",
        "svolderx/0102030405060708/volume/set",
    ] {
        assert_eq!(command_target("svolder", other), None, "{other}");
    }
}

#[test]
fn a_set_request_carries_on_off_or_the_raw_number() {
    assert_eq!(set_value(&Value::Bool(true)), "on");
    assert_eq!(set_value(&Value::Bool(false)), "off");
    assert_eq!(set_value(&Value::U8(109)), "109");
    assert_eq!(set_value(&Value::I16(-12)), "-12");
}

#[test]
fn the_environment_password_wins_over_the_config() {
    let env = || Some("from-the-environment".to_string());
    assert_eq!(
        password(env(), Some("from-the-file")).as_deref(),
        Some("from-the-environment")
    );
    assert_eq!(
        password(env(), None).as_deref(),
        Some("from-the-environment")
    );
    assert_eq!(
        password(Some(String::new()), Some("from-the-file")).as_deref(),
        Some("from-the-file"),
        "an empty variable is no password"
    );
    assert_eq!(password(None, None), None);
}

#[test]
fn the_fitting_presets_are_the_built_ins_then_the_files_own_of_the_band_count() {
    let registry = svolder_profiles::Registry::builtin().expect("the built-in profiles load");
    let gungnir = registry.get("gungnir2-v2").expect("gungnir2-v2");
    let mimir = registry.get("mimir").expect("mimir");
    let user = |name: &str, bands: u8| Preset {
        name: name.to_string(),
        source: None,
        bands,
        mode: EqMode::Eq,
        loudness: 0,
        band: vec![
            svolder_profiles::tone::preset::PresetBand {
                f: 1000.0,
                g: 0.0,
                q: 1.0
            };
            usize::from(bands)
        ],
    };
    let file = [user("Five", 5), user("Three", 3), user("Five b", 5)];
    let names = |p: &Profile| -> Vec<String> {
        fitting_presets(p, &file)
            .into_iter()
            .map(|p| p.name)
            .collect()
    };
    let five = names(gungnir);
    assert_eq!(&five[five.len() - 2..], ["Five", "Five b"]);
    assert!(!five.contains(&"Three".to_string()));
    assert_eq!(names(mimir).last().map(String::as_str), Some("Three"));
    let none = svolder_profiles::Registry::builtin()
        .expect("the built-in profiles load")
        .get("fake-amp")
        .expect("fake-amp")
        .clone();
    assert!(fitting_presets(&none, &file).is_empty());
}

/// A stand-in device behind one actor channel: it applies each band and mode command to its
/// readings, publishes them through the feed as the real actor does before it answers, and
/// records the commands it was sent, in order. Each command waits for a permit of `gate`.
struct Stand {
    log: Arc<std::sync::Mutex<Vec<String>>>,
    gate: Arc<tokio::sync::Semaphore>,
    feed: watch::Sender<Feed>,
    /// The 1-based command that is answered with an error and not applied.
    fail_at: Arc<AtomicUsize>,
}

const UNIT: &str = "0102030405060708";

/// One Gungnir 2 on `unit` behind a [`Stand`] that is let through at once.
fn stand_device(unit: &str, name: &str) -> (DeviceHandle, Stand) {
    stand_device_on("gungnir2-v2", unit, name)
}

/// One device of the built-in profile `profile_name` on `unit` behind a [`Stand`] that is let
/// through at once.
fn stand_device_on(profile_name: &str, unit: &str, name: &str) -> (DeviceHandle, Stand) {
    let registry = svolder_profiles::Registry::builtin().expect("the built-in profiles load");
    let profile = Arc::new(registry.get(profile_name).expect("a built-in").clone());
    let (feed, feed_rx) = watch::channel(Feed {
        name: name.into(),
        unit: unit.into(),
        profile: Some(Arc::clone(&profile)),
        connected: true,
        ..Feed::default()
    });
    let (commands, mut inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let stand = Stand {
        log: Arc::default(),
        gate: Arc::new(tokio::sync::Semaphore::new(1000)),
        feed,
        fail_at: Arc::new(AtomicUsize::new(usize::MAX)),
    };
    let (log, gate, publish, fail_at) = (
        Arc::clone(&stand.log),
        Arc::clone(&stand.gate),
        stand.feed.clone(),
        Arc::clone(&stand.fail_at),
    );
    tokio::spawn(async move {
        let mut seen = 0;
        while let Some((command, reply)) = inbox.recv().await {
            gate.acquire().await.expect("the gate is open").forget();
            seen += 1;
            log.lock().expect("the log").push(shown(&command));
            if seen == fail_at.load(Ordering::SeqCst) {
                let _ = reply.send(Response::error(CODE_LINK, "the device said no"));
                continue;
            }
            let message = match &command {
                Command::EqSet {
                    band, param, value, ..
                } => svolder_profiles::eq_set(
                    &profile,
                    band - 1,
                    param.parse().expect("a parameter"),
                    *value,
                )
                .ok(),
                Command::EqMode { mode, .. } => {
                    svolder_profiles::eq_mode(&profile, mode.parse().expect("a mode")).ok()
                }
                // A write is reported on the control's own id, as the session folds it.
                Command::Raw { id, value, .. } => Some(svolder_proto::Message::status(
                    profile.read_as(id.parse().expect("an id")),
                    Value::U8(value.parse().expect("a strength")),
                )),
                _ => None,
            };
            let Some(message) = message else {
                let _ = reply.send(Response::Ack);
                continue;
            };
            publish.send_modify(|feed| {
                feed.readings.retain(|m| m.id != message.id);
                feed.readings.push(message.clone());
            });
            // A write the stand applied is answered with its value, as the actor does.
            let status = svolder_proto::Message::status(message.id, message.value);
            let reading = svolder_profiles::describe(&profile, &status);
            let _ = reply.send(Response::Value(svolder_ipc::ValueObject::from_reading(
                &reading, &status,
            )));
        }
    });
    let (events, _) = tokio::sync::broadcast::channel(1);
    let handle = DeviceHandle {
        name: name.into(),
        udid: unit.into(),
        writer: Writer::spawn(commands.clone(), feed_rx.clone()),
        commands,
        events,
        info: Arc::new(std::sync::Mutex::new(crate::device::LiveInfo {
            signal: None,
            unit: unit.into(),
            product_id: 0,
            product: None,
            firmware: 0,
            profile: profile_name.into(),
            connected: true,
            stale: false,
            next_attempt: None,
            passphrase_note: None,
            refused: false,
            history: Arc::default(),
            ring: Arc::default(),
        })),
        feed: feed_rx,
    };
    (handle, stand)
}

/// A task with one Gungnir 2 behind a [`Stand`].
async fn published_stand() -> (BridgeTask, Stand) {
    published_stand_on("gungnir2-v2").await
}

/// A task with one device of the built-in profile `profile_name` behind a [`Stand`].
async fn published_stand_on(profile_name: &str) -> (BridgeTask, Stand) {
    published_stand_at(profile_name, 1).await
}

/// [`published_stand_on`] with its client connecting to the broker on `port`.
async fn published_stand_at(profile_name: &str, port: u16) -> (BridgeTask, Stand) {
    let (settings, _) = (
        Settings {
            host: "127.0.0.1".into(),
            port,
            client_id: "test".into(),
            username: None,
            password: None,
            availability: "svolder/bridge/availability".into(),
        },
        (),
    );
    let (client, _events) = Client::start(settings);
    let (changes_tx, _changes) = mpsc::unbounded_channel();
    let mut task = BridgeTask {
        config: MqttConfig::default(),
        client,
        devices: BTreeMap::new(),
        changes_tx,
        writes: JoinSet::new(),
        flush_at: None,
        last_flush: None,
        last_lost: None,
        retained_commands: BTreeSet::new(),
        presets: Vec::new(),
        computer: None,
        scenes: None,
        me: Computer::new("Reframe"),
    };
    let (handle, stand) = stand_device_on(profile_name, UNIT, "fake");
    task.track(UNIT.into(), &handle);
    (task, stand)
}

/// Let every write finish, handing each back as the task's loop does, and the writers settle:
/// a switch to EQ a writer decided on after a band is a write of its own.
async fn settle(task: &mut BridgeTask) {
    while let Some(done) = task.writes.join_next().await {
        task.take_outcome(done.expect("a write task"));
    }
    for published in task.devices.values() {
        let mut idle = false;
        for _ in 0..100 {
            if published.writer.view().await.in_flight.is_empty() {
                idle = true;
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(idle, "a writer never settled");
    }
}

/// What `unit`'s writer holds now.
async fn view_of(task: &BridgeTask, unit: &str) -> crate::writer::View {
    task.devices[unit].writer.view().await
}

/// What the first device's writer holds now.
async fn view(task: &BridgeTask) -> crate::writer::View {
    view_of(task, UNIT).await
}

fn control(key: &str) -> Control {
    (UNIT.to_string(), key.to_string())
}

fn band_gain(band: usize, value: f64) -> WriteRequest {
    WriteRequest::EqBand {
        band,
        param: "g",
        value,
    }
}

/// The commands a recall of `name` sends given `readings`, as the stand-in logs them.
fn recall_log(stand: &Stand, name: &str) -> Vec<String> {
    let feed = stand.feed.borrow();
    let profile = feed.profile.clone().expect("a profile");
    let readings = feed.readings.clone();
    recall::plan(
        "fake",
        &profile,
        &fitting_presets(&profile, &[]),
        name,
        svolder_mqtt::eq::raw_of(&readings),
    )
    .expect("a plan")
    .recall
    .commands
    .iter()
    .map(|c| shown(&recall::command(c)))
    .collect()
}

#[tokio::test]
async fn a_recall_sends_exactly_the_planned_writes_in_the_planned_order() {
    let (mut task, stand) = published_stand().await;
    let want = recall_log(&stand, "Warm");
    assert!(want.len() > 5, "{want:?}");
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    settle(&mut task).await;
    assert_eq!(*stand.log.lock().expect("the log"), want);
}

#[tokio::test]
async fn a_band_command_during_a_recall_is_sent_after_it() {
    let (mut task, stand) = published_stand().await;
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
    // Held, not sent beside the recall.
    assert_eq!(view(&task).await.held, 1);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    assert_eq!(log.last().map(String::as_str), Some("eq 1 g 5"), "{log:?}");
    let feed = stand.feed.borrow();
    let eq = feed
        .profile
        .as_ref()
        .and_then(|p| p.eq.clone())
        .expect("eq");
    let gain = feed
        .readings
        .iter()
        .find(|m| {
            m.id == eq
                .band_id(0, svolder_profiles::EqParam::Gain)
                .expect("a band")
        })
        .and_then(|m| m.value.as_i64())
        .expect("a gain reading");
    assert_eq!(gain, i64::from(eq.gain_raw(5.0)));
    assert_eq!(view(&task).await.held, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mode_command_during_a_recall_wins_and_held_commands_keep_their_arrival_order() {
    let (mut task, stand) = published_stand().await;
    let recall_len = recall_log(&stand, "Warm").len();
    // The gate holds every write at the stand until the whole burst is in: a recall that ended
    // before the later commands came would leave them after it, which is another case.
    stand.gate.forget_permits(1000);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        while !view(&task).await.recall_running {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the recall never started");
    task.enqueue(
        control("eq_mode"),
        WriteRequest::EqMode {
            mode: "flat".into(),
        },
    );
    task.enqueue(control("eq_band_2_gain"), band_gain(2, 1.0));
    task.enqueue(control("eq_band_2_gain"), band_gain(2, 2.0));
    task.enqueue(
        control("eq_mode"),
        WriteRequest::EqMode {
            mode: "flat".into(),
        },
    );
    stand.gate.add_permits(1000);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    let after = &log[recall_len..];
    assert!(after.contains(&"eq 2 g 2".to_string()), "{log:?}");
    assert!(
        !after.iter().any(|line| line == "mode eq"),
        "the last word was flat: {log:?}"
    );
    let feed = stand.feed.borrow();
    let eq = feed
        .profile
        .as_ref()
        .and_then(|p| p.eq.clone())
        .expect("eq");
    let mode = svolder_mqtt::eq::raw_of(&feed.readings)(eq.mode_id());
    assert_eq!(mode, Some(EqMode::Flat as i64), "{log:?}");
}

#[tokio::test]
async fn an_explicit_flat_followed_by_a_band_ends_in_eq() {
    let (mut task, stand) = published_stand().await;
    report_mode(&stand, EqMode::Eq);
    task.enqueue(
        control("eq_mode"),
        WriteRequest::EqMode {
            mode: "flat".into(),
        },
    );
    settle(&mut task).await;
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
    settle(&mut task).await;
    assert_eq!(
        *stand.log.lock().expect("the log"),
        ["mode flat", "eq 1 g 5", "mode eq"]
    );
}

#[tokio::test]
async fn a_recall_that_starts_behind_a_band_in_flight_is_not_followed_by_a_switch() {
    let (mut task, stand) = published_stand().await;
    task.presets = vec![flat_preset("Quiet", EqMode::Loudness, 40)];
    report_mode(&stand, EqMode::Flat);
    stand.gate.forget_permits(1000);
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Quiet".into(),
        },
    );
    stand.gate.add_permits(1000);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    assert_eq!(log[0], "eq 1 g 5", "{log:?}");
    assert_ne!(log.last().map(String::as_str), Some("mode eq"), "{log:?}");
    let feed = stand.feed.borrow();
    let eq = feed
        .profile
        .as_ref()
        .and_then(|p| p.eq.clone())
        .expect("eq");
    let mode = svolder_mqtt::eq::raw_of(&feed.readings)(eq.mode_id());
    assert_eq!(mode, Some(EqMode::Loudness as i64), "{log:?}");
}

#[tokio::test]
async fn a_recall_waits_for_a_tone_write_in_flight_and_plans_from_what_it_left() {
    let (mut task, stand) = published_stand().await;
    stand.gate.forget_permits(1000);
    task.enqueue(control("eq_band_2_gain"), band_gain(2, 5.0));
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    assert!(
        view(&task).await.recall_waiting.is_some(),
        "the recall did not wait"
    );
    assert!(!view(&task).await.recall_running);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(stand.log.lock().expect("the log").is_empty());
    stand.gate.add_permits(1);
    // The band goes through alone; the recall then starts, planned from what it left, and its
    // first write waits at the gate while the plan is worked out here.
    while !view(&task).await.recall_running {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let want = recall_log(&stand, "Warm");
    stand.gate.add_permits(1000);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    assert_eq!(log[0], "eq 2 g 5");
    assert_eq!(log[1..], want[..]);
    assert!(view(&task).await.recall_waiting.is_none());
}

#[tokio::test]
async fn a_second_pick_replaces_a_recall_that_is_still_waiting() {
    let (mut task, stand) = published_stand().await;
    stand.gate.forget_permits(1000);
    task.enqueue(
        control("eq_band_1_q"),
        WriteRequest::EqBand {
            band: 1,
            param: "q",
            value: 2.0,
        },
    );
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Presence".into(),
        },
    );
    assert_eq!(
        view(&task).await.recall_waiting.as_deref(),
        Some("Presence")
    );
    stand.gate.add_permits(1000);
    settle(&mut task).await;
    assert!(view(&task).await.recall_waiting.is_none());
}

#[tokio::test]
async fn other_controls_are_not_held_by_a_recall() {
    let (mut task, stand) = published_stand().await;
    stand.gate.forget_permits(1000);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(
        control("volume"),
        WriteRequest::Set {
            key: "volume".into(),
            value: "50".into(),
        },
    );
    assert!(view(&task).await.in_flight.contains(&FunctionId::one(10)));
    assert_eq!(view(&task).await.held, 0);
    stand.gate.add_permits(1000);
    settle(&mut task).await;
}

#[tokio::test]
async fn a_tone_change_held_before_a_queued_pick_is_dropped_by_it() {
    let (mut task, stand) = published_stand().await;
    stand.gate.forget_permits(1000);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(control("eq_band_3_gain"), band_gain(3, 4.4));
    assert_eq!(view(&task).await.held, 1);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Presence".into(),
        },
    );
    assert_eq!(view(&task).await.held, 0, "not dropped");
    stand.gate.add_permits(1000);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    assert!(!log.iter().any(|c| c == "eq 3 g 4.4"), "{log:?}");
}

#[tokio::test]
async fn a_tone_change_held_after_a_queued_pick_goes_out_after_it() {
    let (mut task, stand) = published_stand().await;
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Presence".into(),
        },
    );
    task.enqueue(control("eq_band_3_gain"), band_gain(3, 4.4));
    assert_eq!(view(&task).await.held, 1);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    assert_eq!(
        log.last().map(String::as_str),
        Some("eq 3 g 4.4"),
        "{log:?}"
    );
    assert!(log.len() > 2, "{log:?}");
}

fn loudness_request(stand: &Stand, raw: u8) -> (WriteRequest, String) {
    let feed = stand.feed.borrow();
    let eq = feed
        .profile
        .as_ref()
        .and_then(|p| p.eq.clone())
        .expect("eq");
    let id = eq.loudness_id();
    (
        WriteRequest::Loudness { key: None, id, raw },
        format!("raw {id} {raw}"),
    )
}

fn flat_preset(name: &str, mode: EqMode, loudness: u8) -> Preset {
    Preset {
        name: name.into(),
        source: None,
        bands: 5,
        mode,
        loudness,
        band: vec![
            svolder_profiles::tone::preset::PresetBand {
                f: 1000.0,
                g: 0.0,
                q: 1.0
            };
            5
        ],
    }
}

#[tokio::test]
async fn a_held_loudness_survives_a_pick_that_leaves_the_strength_alone() {
    let (mut task, stand) = published_stand().await;
    stand.gate.forget_permits(1000);
    let (request, shown_request) = loudness_request(&stand, 80);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(control("eq_loudness"), request);
    assert_eq!(view(&task).await.held, 1, "the strength was not held");
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Bass".into(),
        },
    );
    assert_eq!(view(&task).await.held, 1, "the pick dropped the strength");
    stand.gate.add_permits(1000);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    let at = log.iter().position(|line| line == &shown_request);
    assert_eq!(
        log.iter().filter(|line| *line == &shown_request).count(),
        1,
        "{log:?}"
    );
    assert!(
        at.is_some_and(|at| log[at + 1..].iter().any(|line| line.starts_with("eq "))),
        "it was asked for before Bass, so it goes out before Bass's writes: {log:?}"
    );
    let strength = shown_request
        .rsplit_once(' ')
        .map_or("", |(prefix, _)| prefix);
    assert!(
        at.is_some_and(|at| !log[at + 1..].iter().any(|line| line.starts_with(strength))),
        "Bass leaves the strength alone, so none follows: {log:?}"
    );
}

#[tokio::test]
async fn a_held_loudness_is_dropped_by_a_pick_in_loudness_mode() {
    let (mut task, stand) = published_stand().await;
    task.presets = vec![flat_preset("Loud", EqMode::Loudness, 99)];
    let (request, shown_request) = loudness_request(&stand, 80);
    let (_, last) = loudness_request(&stand, 99);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(control("eq_loudness"), request);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Loud".into(),
        },
    );
    assert_eq!(view(&task).await.held, 0);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    assert!(!log.contains(&shown_request), "{log:?}");
    assert!(log.contains(&last), "{log:?}");
}

#[tokio::test]
async fn a_tone_command_arriving_while_a_pick_waits_is_held_and_goes_out_last() {
    let (mut task, stand) = published_stand().await;
    stand.gate.forget_permits(1000);
    task.enqueue(control("eq_band_2_gain"), band_gain(2, 5.0));
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 3.3));
    assert_eq!(
        view(&task).await.held,
        1,
        "dispatched beside the waiting pick"
    );
    stand.gate.add_permits(1000);
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    assert_eq!(
        log.last().map(String::as_str),
        Some("eq 1 g 3.3"),
        "{log:?}"
    );
}

#[tokio::test]
async fn holds_are_per_device() {
    let (mut task, stand) = published_stand().await;
    let other = "0807060504030201";
    let (handle, other_stand) = stand_device(other, "other");
    task.track(other.into(), &handle);
    other_stand.gate.forget_permits(1000);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    let theirs = (other.to_string(), "eq_band_1_gain".to_string());
    task.enqueue(theirs.clone(), band_gain(1, 2.0));
    let band = FunctionId::three(12, 1, 3);
    assert!(
        view_of(&task, other).await.in_flight.contains(&band),
        "held for another device's recall"
    );
    task.enqueue(
        (other.to_string(), "eq_preset".to_string()),
        WriteRequest::Preset {
            name: "Bass".into(),
        },
    );
    // The pick waits for its own device's band write, and for nothing of the first device.
    assert!(view_of(&task, other).await.recall_waiting.is_some());
    assert!(view(&task).await.recall_waiting.is_none());
    assert_eq!(view(&task).await.held, 0);
    assert_eq!(view_of(&task, other).await.held, 0);
    other_stand.gate.add_permits(1000);
    settle(&mut task).await;
    assert!(stand.log.lock().expect("the log").len() > 2);
    assert_eq!(other_stand.log.lock().expect("the log")[0], "eq 1 g 2");
}

#[tokio::test]
async fn a_recall_that_fails_partway_still_releases_the_held_writes() {
    let (mut task, stand) = published_stand().await;
    stand.fail_at.store(3, Ordering::SeqCst);
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 3.3));
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    assert_eq!(log.len(), 4, "{log:?}");
    assert_eq!(log[3], "eq 1 g 3.3");
    assert_eq!(view(&task).await.held, 0);
    assert!(view(&task).await.in_flight.is_empty());
}

#[test]
fn a_clamp_is_logged_under_the_device_and_the_preset() {
    let registry = svolder_profiles::Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2");
    let mut curve = flat_preset("HD650 warm", EqMode::Eq, 0);
    curve.band[4].f = 22000.0;
    let fitting = [curve];
    let recalled = recall::plan("fake", profile, &fitting, "hd650 warm", |_| None).expect("a plan");
    let lines = clamp_lines("fake", &recalled);
    assert_eq!(
        lines.last().map(String::as_str),
        Some("mqtt: fake: preset HD650 warm: band 5 frequency 22000 Hz clamped to 19497 Hz")
    );
}

#[tokio::test]
async fn an_imported_preset_is_recalled_as_its_fit_and_then_matches() {
    use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
    let imported = Preset {
        name: "Phones".into(),
        source: Some(svolder_profiles::tone::preset::Imported {
            origin: None,
            correction: Correction {
                preamp_db: -3.0,
                filters: vec![
                    SourceFilter {
                        kind: FilterKind::LowShelf,
                        f: 105.0,
                        g: 5.0,
                        q: 0.7,
                    },
                    SourceFilter {
                        kind: FilterKind::Peaking,
                        f: 3000.0,
                        g: -3.0,
                        q: 2.0,
                    },
                ],
            },
        }),
        bands: 0,
        mode: EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
    };
    let (mut task, stand) = published_stand().await;
    task.presets = vec![imported.clone()];
    let (profile, readings) = {
        let feed = stand.feed.borrow();
        (
            feed.profile.clone().expect("a profile"),
            feed.readings.clone(),
        )
    };
    let fitting = fitting_presets(&profile, &task.presets);
    assert_eq!(fitting.last().map(|p| p.name.as_str()), Some("Phones"));
    let want: Vec<String> = recall::plan(
        "fake",
        &profile,
        &fitting,
        "Phones",
        svolder_mqtt::eq::raw_of(&readings),
    )
    .expect("a plan")
    .recall
    .commands
    .iter()
    .map(|c| shown(&recall::command(c)))
    .collect();
    assert!(want.len() > 5, "{want:?}");

    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Phones".into(),
        },
    );
    settle(&mut task).await;
    assert_eq!(*stand.log.lock().expect("the log"), want);
    let readings = stand.feed.borrow().readings.clone();
    let eq = profile.eq.clone().expect("a tone control");
    assert!(preset::matches(
        &eq,
        svolder_mqtt::eq::raw_of(&readings),
        &imported
    ));
}

/// Make the stand's tone control read `mode`, as a device that has reported it.
fn report_mode(stand: &Stand, mode: EqMode) {
    stand.feed.send_modify(|feed| {
        let eq = feed
            .profile
            .as_ref()
            .and_then(|p| p.eq.clone())
            .expect("eq");
        let message = svolder_profiles::eq_mode(feed.profile.as_ref().expect("a profile"), mode)
            .expect("a mode message");
        feed.readings.retain(|m| m.id != eq.mode_id());
        feed.readings.push(message);
    });
}

#[tokio::test]
async fn a_band_change_in_flat_switches_the_tone_control_to_eq_after_the_band() {
    let (mut task, stand) = published_stand().await;
    report_mode(&stand, EqMode::Flat);
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
    settle(&mut task).await;
    assert_eq!(*stand.log.lock().expect("the log"), ["eq 1 g 5", "mode eq"]);
}

#[tokio::test]
async fn a_band_change_in_eq_or_loudness_mode_leaves_the_mode_alone() {
    for mode in [EqMode::Eq, EqMode::Loudness] {
        let (mut task, stand) = published_stand().await;
        report_mode(&stand, mode);
        task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
        settle(&mut task).await;
        assert_eq!(
            *stand.log.lock().expect("the log"),
            ["eq 1 g 5"],
            "{mode:?}"
        );
    }
}

#[tokio::test]
async fn a_band_change_before_the_mode_is_reported_leaves_the_mode_alone() {
    let (mut task, stand) = published_stand().await;
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
    settle(&mut task).await;
    assert_eq!(*stand.log.lock().expect("the log"), ["eq 1 g 5"]);
}

#[tokio::test]
async fn a_refused_band_change_in_flat_switches_nothing() {
    let (mut task, stand) = published_stand().await;
    report_mode(&stand, EqMode::Flat);
    stand.fail_at.store(1, Ordering::SeqCst);
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
    settle(&mut task).await;
    assert_eq!(*stand.log.lock().expect("the log"), ["eq 1 g 5"]);
}

#[tokio::test]
async fn a_loudness_change_in_flat_switches_nothing() {
    let (mut task, stand) = published_stand().await;
    report_mode(&stand, EqMode::Flat);
    let (request, _) = loudness_request(&stand, 3);
    task.enqueue(control("eq_loudness"), request);
    settle(&mut task).await;
    assert_eq!(stand.log.lock().expect("the log").len(), 1);
}

#[tokio::test]
async fn several_band_changes_in_flat_send_one_mode_write() {
    let (mut task, stand) = published_stand().await;
    report_mode(&stand, EqMode::Flat);
    for band in 1..=3 {
        task.enqueue(
            control(&format!("eq_band_{band}_gain")),
            band_gain(band, 5.0),
        );
    }
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    let modes = log.iter().filter(|line| line.starts_with("mode")).count();
    assert_eq!(modes, 1, "{log:?}");
    assert!(
        log[0].starts_with("eq "),
        "the first write is a band: {log:?}"
    );
    assert_eq!(log.last().map(String::as_str), Some("mode eq"), "{log:?}");
}

#[tokio::test]
async fn a_band_change_that_waited_out_a_recall_follows_the_mode_the_recall_left() {
    // Warm leaves the tone control in EQ: the band alone follows the recall.
    let (mut task, stand) = published_stand().await;
    report_mode(&stand, EqMode::Flat);
    let mut want = recall_log(&stand, "Warm");
    want.push("eq 1 g 5".to_string());
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
    settle(&mut task).await;
    assert_eq!(*stand.log.lock().expect("the log"), want);

    // A preset in Flat leaves the mode Flat: the mode read after the recall decides, and the
    // switch follows the band.
    let (mut task, stand) = published_stand().await;
    task.presets = vec![flat_preset("Quiet", EqMode::Flat, 0)];
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Quiet".into(),
        },
    );
    task.enqueue(control("eq_band_1_gain"), band_gain(1, 5.0));
    settle(&mut task).await;
    let log = stand.log.lock().expect("the log").clone();
    let tail = &log[log.len() - 2..];
    assert_eq!(tail, ["eq 1 g 5", "mode eq"], "{log:?}");
}

/// A slider command and a `mute ON` published just after it reach the device in that order, time
/// after time: the writer takes them as they arrive, so a mute is never overtaken.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slider_command_then_mute_on_reach_the_device_in_that_order() {
    let (mut task, stand) = published_stand().await;
    for _ in 0..20 {
        task.enqueue(
            control("volume"),
            WriteRequest::Set {
                key: "volume".into(),
                value: "50".into(),
            },
        );
        task.enqueue(
            control("mute"),
            WriteRequest::Set {
                key: "mute".into(),
                value: "on".into(),
            },
        );
        settle(&mut task).await;
    }
    let log = stand.log.lock().expect("the log").clone();
    assert_eq!(log.len(), 40, "{log:?}");
    assert!(
        log.chunks(2)
            .all(|pair| pair == ["set volume 50", "set mute on"]),
        "{log:?}"
    );
}

/// The preset select follows a recall another front end made: of two presets with one curve, the
/// one recalled is the one reported, not the first that matches.
#[tokio::test]
async fn the_select_follows_a_recall_another_front_end_made() {
    let (mut task, stand) = published_stand().await;
    let profile = stand.feed.borrow().profile.clone().expect("a profile");
    let eq = profile.eq.clone().expect("a tone control");
    let mut twin = svolder_profiles::tone::preset::builtins(&eq)
        .into_iter()
        .find(|p| p.name == "Warm")
        .expect("Warm");
    twin.name = "Warm too".into();
    task.presets = vec![twin];
    let fitting = fitting_presets(&profile, &task.presets);
    let writer = task.devices[UNIT].writer.clone();
    let crate::writer::Recalling { done, .. } =
        writer.recall(crate::writer::Origin::Mcp, "Warm too".into(), fitting);
    done.await.expect("the recall ended");
    task.publish(UNIT, Instant::now());
    let state: serde_json::Value =
        serde_json::from_str(task.devices[UNIT].state.as_deref().expect("a state")).unwrap();
    assert_eq!(state[svolder_mqtt::eq::PRESET_KEY], "Warm too", "{state}");
}

/// A closed memory watch only stops that arm: the feed's changes still reach the bridge.
#[tokio::test]
async fn a_closed_memory_watch_does_not_stop_the_feed_being_followed() {
    let (feed_tx, feed) = watch::channel(Feed::default());
    let (memory_tx, memory) = watch::channel(None);
    let (changes_tx, mut changes) = mpsc::unbounded_channel();
    tokio::spawn(watch_feed(feed, memory, "unit".into(), changes_tx));
    memory_tx.send_replace(Some("Warm".into()));
    assert_eq!(changes.recv().await.as_deref(), Some("unit"));
    drop(memory_tx);
    tokio::task::yield_now().await;
    feed_tx.send_modify(|feed| feed.name = "renamed".into());
    assert_eq!(changes.recv().await.as_deref(), Some("unit"));
}

/// The topic under the device's own base.
fn topic(leaf: &str) -> String {
    format!("svolder/{UNIT}/{leaf}")
}

/// What the bridge last published on `leaf` of the first device's diagnostic topics.
fn published_on(task: &BridgeTask, leaf: &str) -> Option<String> {
    task.devices[UNIT].diagnostics.get(&topic(leaf)).cloned()
}

fn heard(dbm: i16, source: svolder_ipc::SignalSource, heard_at_ms: u64) -> svolder_ipc::Signal {
    svolder_ipc::Signal {
        dbm,
        source,
        heard_at_ms,
    }
}

#[tokio::test]
async fn dsp_firmware_published_when_reported() {
    let (mut task, stand) = published_stand().await;
    task.publish(UNIT, Instant::now());
    assert_eq!(
        published_on(&task, "firmware/state"),
        None,
        "nothing before it reports"
    );
    assert_eq!(published_on(&task, "dsp_firmware/state"), None);

    stand.feed.send_modify(|feed| {
        feed.readings.push(svolder_proto::Message::status(
            svolder_mqtt::payload::DSP_FIRMWARE,
            Value::U16(17),
        ));
    });
    task.publish(UNIT, Instant::now());
    assert_eq!(
        published_on(&task, "dsp_firmware/state").as_deref(),
        Some("17")
    );
    assert_eq!(
        published_on(&task, "firmware/state"),
        None,
        "only what was reported"
    );

    stand.feed.send_modify(|feed| {
        feed.readings.push(svolder_proto::Message::status(
            svolder_mqtt::payload::FIRMWARE,
            Value::U16(204),
        ));
    });
    task.publish(UNIT, Instant::now());
    assert_eq!(
        published_on(&task, "firmware/state").as_deref(),
        Some("204")
    );
    assert_eq!(
        published_on(&task, "dsp_firmware/state").as_deref(),
        Some("17")
    );
}

#[tokio::test]
async fn the_signal_is_published_with_its_source_and_time_and_again_on_a_change() {
    use svolder_ipc::SignalSource::{Advert, Live};
    let (mut task, stand) = published_stand().await;
    task.publish(UNIT, Instant::now());
    assert_eq!(
        published_on(&task, "signal/state"),
        None,
        "no signal known: nothing"
    );
    assert_eq!(published_on(&task, "signal/attributes"), None);

    let at = 1_792_931_696_789;
    stand
        .feed
        .send_modify(|feed| feed.signal = Some(heard(-62, Advert, at)));
    task.publish(UNIT, Instant::now());
    assert_eq!(published_on(&task, "signal/state").as_deref(), Some("-62"));
    let attributes: serde_json::Value =
        serde_json::from_str(&published_on(&task, "signal/attributes").expect("attributes"))
            .expect("JSON");
    assert_eq!(
        attributes,
        serde_json::json!({"source": "advert", "heard_at": "2026-10-25T12:34:56Z"})
    );

    // A new stamp with the same value is a change; so is a new source.
    stand
        .feed
        .send_modify(|feed| feed.signal = Some(heard(-62, Advert, at + 61_000)));
    task.publish(UNIT, Instant::now());
    assert!(
        published_on(&task, "signal/attributes")
            .expect("attributes")
            .contains("12:35:57"),
    );
    stand
        .feed
        .send_modify(|feed| feed.signal = Some(heard(-58, Live, at + 61_000)));
    task.publish(UNIT, Instant::now());
    assert_eq!(published_on(&task, "signal/state").as_deref(), Some("-58"));
    assert!(
        published_on(&task, "signal/attributes")
            .expect("attributes")
            .contains("\"live\""),
    );

    // The link going down leaves the last signal where it was: availability says unavailable.
    stand.feed.send_modify(|feed| feed.connected = false);
    task.publish(UNIT, Instant::now());
    assert_eq!(published_on(&task, "signal/state").as_deref(), Some("-58"));
}

#[tokio::test]
async fn identify_press_reaches_the_device() {
    let (mut task, stand) = published_stand().await;
    task.config.commands = true;
    task.message(&topic("identify/set"), b"PRESS", false);
    settle(&mut task).await;
    assert_eq!(*stand.log.lock().expect("the log"), ["identify"]);
}

#[tokio::test]
async fn anything_but_a_press_or_without_commands_sends_no_identify() {
    let (mut task, stand) = published_stand().await;
    task.config.commands = true;
    task.message(&topic("identify/set"), b"ON", false);
    task.message(&topic("identify/set"), b"", false);
    task.message(&topic("identify/set"), &[0xff, 0xfe], false);
    // A retained press is never acted on.
    task.message(&topic("identify/set"), b"PRESS", true);
    task.config.commands = false;
    task.message(&topic("identify/set"), b"PRESS", false);
    settle(&mut task).await;
    assert!(stand.log.lock().expect("the log").is_empty());
}

#[test]
fn an_identify_press_answers_in_words_for_the_log() {
    assert_eq!(
        commands::identify_line("amp", true),
        "mqtt: amp: identify sent"
    );
    assert_eq!(
        commands::identify_line("amp", false),
        "mqtt: amp: identify not sent: one went out within the last two seconds"
    );
}

/// A repeat of the same signal and firmware is no message; a changed value is exactly one.
#[test]
fn a_repeated_diagnostic_is_not_published_again_and_a_changed_one_is_once() {
    use svolder_ipc::SignalSource::Advert;
    use svolder_proto::Message;
    let mut feed = Feed {
        signal: Some(heard(-62, Advert, 1_792_931_696_789)),
        readings: vec![
            Message::status(svolder_mqtt::payload::FIRMWARE, Value::U16(204)),
            Message::status(svolder_mqtt::payload::DSP_FIRMWARE, Value::U16(17)),
        ],
        ..Feed::default()
    };
    let mut known = BTreeMap::new();
    let mut next = |feed: &Feed| {
        super::state::changed_diagnostics("svolder", UNIT, feed, &mut known)
            .into_iter()
            .map(|(topic, body)| (topic.replace(&format!("svolder/{UNIT}/"), ""), body))
            .collect::<Vec<_>>()
    };
    let first = next(&feed);
    assert_eq!(
        first.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(),
        [
            "signal/attributes",
            "signal/state",
            "firmware/state",
            "dsp_firmware/state"
        ]
    );
    assert!(next(&feed).is_empty(), "the same again: nothing");

    feed.signal = Some(heard(-58, Advert, 1_792_931_696_789));
    let changed = next(&feed);
    assert_eq!(changed, [("signal/state".to_string(), "-58".to_string())]);
    assert!(next(&feed).is_empty());

    feed.signal = Some(heard(-58, Advert, 1_792_931_757_000));
    let changed = next(&feed);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].0, "signal/attributes");

    feed.readings[1] = Message::status(svolder_mqtt::payload::DSP_FIRMWARE, Value::U16(18));
    assert_eq!(
        next(&feed),
        [("dsp_firmware/state".to_string(), "18".to_string())]
    );
}

/// A Loki Max behind a [`Stand`], taking commands, its six bands read at `raw`.
async fn published_loki(raw: [u8; 6]) -> (BridgeTask, Stand) {
    let (mut task, stand) = published_stand_on("lokimax").await;
    task.config.commands = true;
    stand.feed.send_modify(|feed| {
        let profile = feed.profile.clone().expect("a profile");
        for (band, raw) in (1..=6).zip(raw) {
            let control = profile
                .control_by_key(&format!("band{band}"))
                .expect("a band");
            feed.readings
                .push(svolder_proto::Message::status(control.id, Value::U8(raw)));
        }
    });
    (task, stand)
}

#[tokio::test]
async fn a_pick_of_a_slot_sends_the_recall() {
    let (mut task, stand) = published_loki([15; 6]).await;
    task.message(&topic("eq_preset/set"), b"Preset 2", false);
    settle(&mut task).await;
    assert_eq!(*stand.log.lock().expect("the log"), ["set recall 2"]);
    // A recall is sent as the user's own press, however many times it is picked.
    task.message(&topic("eq_preset/set"), b"Preset 2", false);
    settle(&mut task).await;
    assert_eq!(
        *stand.log.lock().expect("the log"),
        ["set recall 2", "set recall 2"]
    );
}

#[tokio::test]
async fn a_pick_of_a_builtin_applies_it_as_one_batch() {
    let (mut task, stand) = published_loki([15; 6]).await;
    task.message(&topic("eq_preset/set"), b"bass boost", false);
    settle(&mut task).await;
    // Bass Boost is 28 21 17 15 15 15: three bands rise from flat, each written to its two-id
    // form, and the three that already read their target are left out.
    assert_eq!(
        *stand.log.lock().expect("the log"),
        ["raw 6.1 28", "raw 6.2 21", "raw 6.3 17"]
    );
    // The select follows the bands the device now reads.
    task.publish(UNIT, Instant::now());
    let state: serde_json::Value =
        serde_json::from_str(task.devices[UNIT].state.as_deref().expect("a state"))
            .expect("a JSON state");
    assert_eq!(state["eq_preset"], "Bass Boost");
    // Picking the setting the bands already read sends nothing.
    task.message(&topic("eq_preset/set"), b"Bass Boost", false);
    settle(&mut task).await;
    assert_eq!(stand.log.lock().expect("the log").len(), 3);
}

#[tokio::test]
async fn a_command_on_an_action_topic_is_dropped() {
    let (mut task, stand) = published_loki([15; 6]).await;
    task.message(&topic("recall/set"), b"2", false);
    task.message(&topic("save/set"), b"1", false);
    task.message(&topic("save/set"), b"Preset 1", false);
    // Nor does the select take a save, `custom`, or a slot the device does not have.
    for pick in ["save", "Save Preset 1", "custom", "Preset 4", "0", ""] {
        task.message(&topic("eq_preset/set"), pick.as_bytes(), false);
    }
    settle(&mut task).await;
    assert!(stand.log.lock().expect("the log").is_empty());
}

#[tokio::test]
async fn a_retained_preset_command_is_not_acted_on() {
    let (mut task, stand) = published_loki([15; 6]).await;
    task.message(&topic("eq_preset/set"), b"Preset 2", true);
    task.message(&topic("eq_preset/set"), b"Bass Boost", true);
    task.config.commands = false;
    task.message(&topic("eq_preset/set"), b"Preset 2", false);
    settle(&mut task).await;
    assert!(stand.log.lock().expect("the log").is_empty());
}

/// What a switch was told of the crossfeed: on or off, and a model's word with a preset's name.
type CrossfeedCall = (Option<bool>, Option<(String, String)>);

/// What a switch was told of the virtual speakers: on or off, and a set's id.
type SpeakersCall = (Option<bool>, Option<String>);

/// What a switch was told of the loudness: on or off, and a strength in percent.
type LoudnessCall = (Option<bool>, Option<f64>);

/// A switch that records what it was told and answers as `answer` says.
struct RecordingSwitch {
    loudnesses: std::sync::Mutex<Vec<LoudnessCall>>,
    calls: std::sync::Mutex<Vec<bool>>,
    crossfeeds: std::sync::Mutex<Vec<CrossfeedCall>>,
    speakers: std::sync::Mutex<Vec<SpeakersCall>>,
    answer: Result<(), String>,
}

#[async_trait::async_trait]
impl crate::host::EqSwitch for RecordingSwitch {
    async fn set_on(&self, on: bool) -> Result<(), String> {
        self.calls.lock().unwrap().push(on);
        self.answer.clone()
    }

    async fn crossfeed(
        &self,
        on: Option<bool>,
        choice: Option<(String, String)>,
    ) -> Result<(), String> {
        self.crossfeeds.lock().unwrap().push((on, choice));
        self.answer.clone()
    }

    async fn speakers(&self, on: Option<bool>, set: Option<String>) -> Result<(), String> {
        self.speakers.lock().unwrap().push((on, set));
        self.answer.clone()
    }

    async fn loudness(&self, on: Option<bool>, strength_pct: Option<f64>) -> Result<(), String> {
        self.loudnesses.lock().unwrap().push((on, strength_pct));
        self.answer.clone()
    }
}

fn computer_view(on: bool, preset: Option<&str>, gain: f64) -> svolder_ipc::host::HostView {
    svolder_ipc::host::HostView {
        available: true,
        eq: Some(svolder_host::eq::EqView {
            on,
            points: vec![svolder_host::eq::PointView {
                slot: 0,
                kind: "PK".into(),
                f: 1000.0,
                g: gain,
                q: 1.0,
            }],
            preset: preset.map(str::to_string),
            max: 16,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A task that follows a computer whose view is `view`, with commands on.
async fn with_computer(
    view: svolder_ipc::host::HostView,
    answer: Result<(), String>,
) -> (
    BridgeTask,
    watch::Sender<svolder_ipc::host::HostView>,
    Arc<RecordingSwitch>,
) {
    with_computer_at(1, view, answer).await
}

/// [`with_computer`] with the task's client connecting to the broker on `port`.
async fn with_computer_at(
    port: u16,
    view: svolder_ipc::host::HostView,
    answer: Result<(), String>,
) -> (
    BridgeTask,
    watch::Sender<svolder_ipc::host::HostView>,
    Arc<RecordingSwitch>,
) {
    let (mut task, _stand) = published_stand_at("gungnir2-v2", port).await;
    task.config.commands = true;
    let (tx, rx) = watch::channel(view);
    let switch = Arc::new(RecordingSwitch {
        loudnesses: std::sync::Mutex::new(Vec::new()),
        calls: std::sync::Mutex::new(Vec::new()),
        crossfeeds: std::sync::Mutex::new(Vec::new()),
        speakers: std::sync::Mutex::new(Vec::new()),
        answer,
    });
    task.follow_computer(ComputerLink {
        view: rx,
        switch: switch.clone(),
        slow: None,
        scenes: None,
    });
    (task, tx, switch)
}

fn computer_calls(switch: &RecordingSwitch) -> Vec<bool> {
    switch.calls.lock().unwrap().clone()
}

#[tokio::test]
async fn on_applies_the_kept_points_and_off_removes() {
    let (mut task, _tx, switch) =
        with_computer(computer_view(false, Some("Warm"), -3.0), Ok(())).await;
    task.message("svolder/computer_reframe/computer_eq/set", b"ON", false);
    settle(&mut task).await;
    assert_eq!(computer_calls(&switch), [true]);
    task.message("svolder/computer_reframe/computer_eq/set", b" off ", false);
    settle(&mut task).await;
    assert_eq!(computer_calls(&switch), [true, false]);
    // Anything else, and any other key, is dropped.
    task.message("svolder/computer_reframe/computer_eq/set", b"toggle", false);
    task.message("svolder/computer_reframe/computer_eq/set", &[0xff], false);
    task.message("svolder/computer_reframe/volume/set", b"50", false);
    task.message(
        "svolder/computer_reframe/computer_eq_preset/set",
        b"Warm",
        false,
    );
    settle(&mut task).await;
    assert_eq!(computer_calls(&switch), [true, false]);
}

#[tokio::test]
async fn on_with_nothing_kept_stays_off() {
    let (mut task, _tx, switch) = with_computer(computer_view(false, None, 0.0), Ok(())).await;
    task.message("svolder/computer_reframe/computer_eq/set", b"ON", false);
    settle(&mut task).await;
    assert!(computer_calls(&switch).is_empty());
    // OFF is still sent: it removes a chain the view does not know of.
    task.message("svolder/computer_reframe/computer_eq/set", b"OFF", false);
    settle(&mut task).await;
    assert_eq!(computer_calls(&switch), [false]);
}

#[tokio::test]
async fn a_command_with_the_computer_unavailable_or_without_commands_does_nothing() {
    let mut view = computer_view(false, Some("Warm"), -3.0);
    view.eq.as_mut().unwrap().unavailable =
        Some("computer controls unavailable: no pw-cli on this computer".into());
    let (mut task, _tx, switch) = with_computer(view, Ok(())).await;
    task.message("svolder/computer_reframe/computer_eq/set", b"ON", false);
    let (mut off, _tx2, switch2) =
        with_computer(computer_view(false, Some("Warm"), -3.0), Ok(())).await;
    off.config.commands = false;
    off.message("svolder/computer_reframe/computer_eq/set", b"ON", false);
    settle(&mut task).await;
    settle(&mut off).await;
    assert!(computer_calls(&switch).is_empty());
    assert!(computer_calls(&switch2).is_empty());
}

#[tokio::test]
async fn a_retained_computer_command_is_not_acted_on() {
    let (mut task, _tx, switch) =
        with_computer(computer_view(false, Some("Warm"), -3.0), Ok(())).await;
    task.message("svolder/computer_reframe/computer_eq/set", b"ON", true);
    settle(&mut task).await;
    assert!(computer_calls(&switch).is_empty());
    assert!(
        task.retained_commands
            .contains("svolder/computer_reframe/computer_eq/set")
    );
}

#[tokio::test]
async fn the_computer_publishes_its_state_and_preset_and_follows_the_view() {
    let (mut task, tx, _switch) =
        with_computer(computer_view(true, Some("Warm"), -3.0), Ok(())).await;
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.state, Some(("ON", "Warm".to_string())));
    assert_eq!(published.available, Some(true));
    assert_eq!(published.configs.len(), 4);

    tx.send_replace(computer_view(true, Some("custom"), -3.0));
    task.changed("computer_reframe");
    task.publish_computer();
    assert_eq!(
        task.computer.as_ref().unwrap().state,
        Some(("ON", "custom".to_string()))
    );

    tx.send_replace(computer_view(false, Some("custom"), -3.0));
    task.changed("computer_reframe");
    task.publish_computer();
    assert_eq!(
        task.computer.as_ref().unwrap().state,
        Some(("OFF", String::new()))
    );

    // Unavailable: the entities stay, offline.
    let mut view = computer_view(false, None, 0.0);
    view.eq.as_mut().unwrap().unavailable = Some("computer controls unavailable: x".into());
    tx.send_replace(view);
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.available, Some(false));
    assert_eq!(published.configs.len(), 4);

    // Host off: everything is taken down.
    tx.send_replace(svolder_ipc::host::HostView::default());
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert!(published.configs.is_empty() && published.available.is_none());
}

/// A computer view whose controller serves crossfeed, as `on` and `preset` say, with the model
/// table of the host crate.
fn crossfeed_computer_view(
    on: bool,
    model: &str,
    preset: Option<&str>,
) -> svolder_ipc::host::HostView {
    use svolder_host::eq::crossfeed::{Model, ModelView};
    let mut view = computer_view(false, None, 0.0);
    view.eq.as_mut().unwrap().crossfeed = Some(svolder_host::eq::CrossfeedView {
        on,
        model: model.to_string(),
        preset: preset.map(str::to_string),
        models: Model::ALL.into_iter().map(ModelView::of).collect(),
        ..Default::default()
    });
    view
}

fn crossfeed_calls(switch: &RecordingSwitch) -> Vec<CrossfeedCall> {
    switch.crossfeeds.lock().unwrap().clone()
}

#[tokio::test]
async fn the_crossfeed_is_announced_and_its_states_follow_the_view() {
    let (mut task, tx, _switch) = with_computer(
        crossfeed_computer_view(true, "bauer", Some("Chu Moy")),
        Ok(()),
    )
    .await;
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.configs.len(), 6);
    assert!(published.configs.contains_key(
        "homeassistant/select/svolder_computer_reframe/computer_crossfeed_preset/config"
    ));
    assert_eq!(
        published.crossfeed_state,
        Some(("ON", "Bauer · Chu Moy".to_string()))
    );

    tx.send_replace(crossfeed_computer_view(false, "meier", None));
    task.changed("computer_reframe");
    task.publish_computer();
    assert_eq!(
        task.computer.as_ref().unwrap().crossfeed_state,
        Some(("OFF", "Custom".to_string()))
    );

    // A controller that no longer serves crossfeed takes its entities down, not the EQ's.
    tx.send_replace(computer_view(false, None, 0.0));
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.configs.len(), 4);
    assert_eq!(published.crossfeed_state, None);
}

#[tokio::test]
async fn host_audio_off_clears_the_crossfeed_with_the_computer() {
    let (mut task, tx, _switch) =
        with_computer(crossfeed_computer_view(true, "bauer", None), Ok(())).await;
    task.publish_computer();
    assert!(task.computer.as_ref().unwrap().crossfeed_state.is_some());
    tx.send_replace(svolder_ipc::host::HostView::default());
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert!(published.configs.is_empty() && published.crossfeed_state.is_none());
}

#[tokio::test]
async fn crossfeed_commands_reach_the_controller_as_one_change_each() {
    let (mut task, _tx, switch) = with_computer(
        crossfeed_computer_view(false, "bauer", Some("bs2b default")),
        Ok(()),
    )
    .await;
    task.message(
        "svolder/computer_reframe/computer_crossfeed/set",
        b"ON",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_crossfeed/set",
        b" off ",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_crossfeed_preset/set",
        "Bauer · Chu Moy".as_bytes(),
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_crossfeed_preset/set",
        b"Linkwitz",
        false,
    );
    settle(&mut task).await;
    let mut calls = crossfeed_calls(&switch);
    calls.sort();
    let mut want: Vec<CrossfeedCall> = vec![
        (Some(true), None),
        (Some(false), None),
        (None, Some(("bauer".into(), "Chu Moy".into()))),
        (
            None,
            Some((
                "linkwitz".into(),
                svolder_host::eq::crossfeed::Model::Linkwitz.spec().presets[0]
                    .name
                    .into(),
            )),
        ),
    ];
    want.sort();
    assert_eq!(calls, want);
    // The EQ's own switch was not touched.
    assert!(computer_calls(&switch).is_empty());
}

#[tokio::test]
async fn custom_and_unknown_crossfeed_commands_change_nothing() {
    let (mut task, _tx, switch) = with_computer(
        crossfeed_computer_view(true, "bauer", Some("bs2b default")),
        Ok(()),
    )
    .await;
    for body in [&b"Custom"[..], b"custom", b"Bauer", b"nonsense", &[0xff]] {
        task.message(
            "svolder/computer_reframe/computer_crossfeed_preset/set",
            body,
            false,
        );
    }
    task.message(
        "svolder/computer_reframe/computer_crossfeed/set",
        b"toggle",
        false,
    );
    settle(&mut task).await;
    assert!(crossfeed_calls(&switch).is_empty());
}

#[tokio::test]
async fn a_crossfeed_command_the_host_cannot_take_does_nothing() {
    // No crossfeed in the view, the controller's crossfeed unavailable, host audio off, no
    // commands, and a retained command: none reaches the controller.
    let mut unavailable = crossfeed_computer_view(false, "bauer", None);
    unavailable
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap()
        .unavailable = Some("computer controls unavailable: x".into());
    for view in [
        computer_view(false, None, 0.0),
        unavailable,
        svolder_ipc::host::HostView::default(),
    ] {
        let (mut task, _tx, switch) = with_computer(view, Ok(())).await;
        task.message(
            "svolder/computer_reframe/computer_crossfeed/set",
            b"ON",
            false,
        );
        task.message(
            "svolder/computer_reframe/computer_crossfeed_preset/set",
            b"Linkwitz",
            false,
        );
        settle(&mut task).await;
        assert!(crossfeed_calls(&switch).is_empty());
    }
    let (mut task, _tx, switch) =
        with_computer(crossfeed_computer_view(false, "bauer", None), Ok(())).await;
    task.message(
        "svolder/computer_reframe/computer_crossfeed/set",
        b"ON",
        true,
    );
    task.config.commands = false;
    task.message(
        "svolder/computer_reframe/computer_crossfeed/set",
        b"ON",
        false,
    );
    settle(&mut task).await;
    assert!(crossfeed_calls(&switch).is_empty());
}

#[tokio::test]
async fn a_refused_crossfeed_command_is_followed_by_the_views_state() {
    let (mut task, _tx, switch) = with_computer(
        crossfeed_computer_view(false, "bauer", Some("bs2b default")),
        Err("no".into()),
    )
    .await;
    task.publish_computer();
    task.message(
        "svolder/computer_reframe/computer_crossfeed/set",
        b"ON",
        false,
    );
    settle(&mut task).await;
    assert_eq!(crossfeed_calls(&switch), [(Some(true), None)]);
    // The refusal marks the computer for a publish, and the state stays the view's.
    assert!(task.computer.as_ref().unwrap().dirty);
    task.publish_computer();
    assert_eq!(
        task.computer.as_ref().unwrap().crossfeed_state,
        Some(("OFF", "Bauer · bs2b default".to_string()))
    );
}

/// The retained configs an earlier run left under the computer, which Home Assistant would
/// remove and add again if they were cleared before being announced.
#[cfg(feature = "broker-tests")]
fn left_over() -> [String; 4] {
    [
        "homeassistant/switch/svolder_computer_reframe/computer_eq/config".to_string(),
        "homeassistant/sensor/svolder_computer_reframe/computer_eq_preset/config".to_string(),
        // Announced only by a bridge that does not take commands: this one clears it.
        "homeassistant/binary_sensor/svolder_computer_reframe/computer_eq/config".to_string(),
        // The crossfeed's, which a view without a crossfeed does not announce: cleared too.
        "homeassistant/switch/svolder_computer_reframe/computer_crossfeed/config".to_string(),
    ]
}

/// A start whose host view has not yet reported the computer clears nothing before it does:
/// the entities about to be announced are never cleared first, and the ones it will not serve
/// are cleared once it has.
#[cfg(feature = "broker-tests")]
#[tokio::test]
async fn a_computer_not_yet_reported_is_not_swept_before_it_is() {
    let broker = broker::TestBroker::start();
    let mut watcher = broker::Watcher::connect(broker.port, &["homeassistant/#"]).await;
    for topic in left_over() {
        watcher.retain(&topic, "{\"name\":\"left over\"}").await;
    }
    let (mut task, tx, _switch) =
        with_computer_at(broker.port, svolder_ipc::host::HostView::default(), Ok(())).await;
    task.publish_computer();
    assert!(!task.computer.as_ref().unwrap().swept);
    let held = watcher.collect(Duration::from_millis(700)).await;
    let cleared: Vec<_> = held.iter().filter(|s| s.payload.is_empty()).collect();
    assert!(
        cleared.is_empty(),
        "cleared before it reported: {cleared:#?}"
    );

    tx.send_replace(computer_view(true, Some("Warm"), -3.0));
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert!(published.swept);
    assert_eq!(published.configs.len(), 4);
    let [switch, preset, binary, crossfeed] = left_over();
    let seen = watcher.collect(Duration::from_millis(700)).await;
    assert!(
        seen.iter()
            .any(|s| s.topic == binary && s.payload.is_empty()),
        "the config this bridge will not serve is cleared: {seen:#?}"
    );
    assert!(
        seen.iter()
            .any(|s| s.topic == crossfeed && s.payload.is_empty()),
        "a crossfeed the host does not serve is cleared: {seen:#?}"
    );
    assert!(
        seen.iter()
            .all(|s| !(s.topic == switch || s.topic == preset) || !s.payload.is_empty()),
        "an entity about to be announced is cleared: {seen:#?}"
    );
}

/// A computer that never reports one (host audio off) is swept once the start has had its
/// time: nothing is cleared before, and every retained config is after.
#[cfg(feature = "broker-tests")]
#[tokio::test]
async fn a_computer_that_stays_unreported_is_swept_after_the_start_settles() {
    let broker = broker::TestBroker::start();
    let mut watcher = broker::Watcher::connect(broker.port, &["homeassistant/#"]).await;
    for topic in left_over() {
        watcher.retain(&topic, "{\"name\":\"left over\"}").await;
    }
    let (mut task, _tx, _switch) =
        with_computer_at(broker.port, svolder_ipc::host::HostView::default(), Ok(())).await;
    task.publish_computer();
    assert!(!task.computer.as_ref().unwrap().swept);
    assert!(task.flush_at.is_some(), "a flush is owed at the deadline");
    let held = watcher.collect(Duration::from_millis(700)).await;
    assert!(
        held.iter().all(|s| !s.payload.is_empty()),
        "cleared early: {held:#?}"
    );

    tokio::time::sleep(super::computer::START_SETTLES).await;
    task.flush();
    assert!(task.computer.as_ref().unwrap().swept);
    let cleared = watcher
        .wait_for_each(
            &left_over().each_ref().map(|t| (t.as_str(), "")),
            Duration::from_secs(5),
        )
        .await;
    assert_eq!(cleared.len(), 4);
}

#[tokio::test]
async fn read_only_the_computer_announces_a_binary_sensor_and_subscribes_to_nothing() {
    let (mut task, _tx, _switch) =
        with_computer(computer_view(true, Some("Warm"), -3.0), Ok(())).await;
    task.config.commands = false;
    task.config.discovery_prefix = "homeassistant".into();
    task.changed("computer_reframe");
    task.publish_computer();
    let topics: Vec<String> = task
        .computer
        .as_ref()
        .unwrap()
        .configs
        .keys()
        .cloned()
        .collect();
    assert!(
        topics
            .iter()
            .any(|t| t.contains("/binary_sensor/svolder_computer_reframe/")),
        "{topics:?}"
    );
    assert!(topics.iter().all(|t| !t.contains("/switch/")));
}

#[test]
fn a_failed_attempt_comes_to_the_problem_its_port_and_trouble_say() {
    use svolder_mqtt::Trouble;
    assert_eq!(
        problem(Trouble::LoginRefused, 1883),
        Some(MqttProblem::Refused)
    );
    assert_eq!(
        problem(Trouble::LoginRefused, 8883),
        Some(MqttProblem::Refused)
    );
    assert_eq!(
        problem(Trouble::Unreachable, 1883),
        Some(MqttProblem::Unreachable)
    );
    assert_eq!(
        problem(Trouble::Unreachable, 8883),
        Some(MqttProblem::Unreachable)
    );
    assert_eq!(
        problem(Trouble::Dropped, 1883),
        Some(MqttProblem::Unreachable)
    );
    assert_eq!(problem(Trouble::Dropped, 8883), Some(MqttProblem::Tls));
    assert_eq!(problem(Trouble::Other, 1883), None);
}

/// The countdown `version` reports is the time to the next attempt in whole seconds, rounded up:
/// a wait with a fraction of a second left still reads as that second, and a wait that has run
/// out reads 0.
#[tokio::test(start_paused = true)]
async fn the_countdown_to_the_next_attempt_rounds_up_to_whole_seconds() {
    use svolder_mqtt::Trouble;
    use svolder_mqtt::client::Link;
    let now = Instant::now();
    let (set, link) = watch::channel(Link::Waiting {
        retry_at: now + Duration::from_millis(2500),
        trouble: Trouble::Unreachable,
    });
    let (ops, _ops_rx) = mpsc::unbounded_channel();
    let bridge = Bridge {
        config: MqttConfig {
            host: Some("broker".into()),
            ..MqttConfig::default()
        },
        ops,
        link,
        task: tokio::spawn(async {}),
    };
    let retry = |bridge: &Bridge| match bridge.status() {
        MqttStatus::Trouble { retry_in_secs, .. } => retry_in_secs,
        other => panic!("not in trouble: {other:?}"),
    };
    assert_eq!(retry(&bridge), Some(3));
    tokio::time::advance(Duration::from_millis(600)).await;
    assert_eq!(retry(&bridge), Some(2), "1.9 s left");
    tokio::time::advance(Duration::from_millis(900)).await;
    assert_eq!(retry(&bridge), Some(1), "1.0 s left");
    tokio::time::advance(Duration::from_millis(1000)).await;
    assert_eq!(retry(&bridge), Some(0), "the wait has run out");
    drop(set);
}

/// A computer view whose controller serves the virtual speakers: two fetched sets, `chosen` the
/// id of the chosen one, playing when `on`.
fn speakers_computer_view(on: bool, chosen: Option<&str>) -> svolder_ipc::host::HostView {
    use svolder_host::eq::speakers::SetView;
    let set = |id: &str, label: &str, with: bool| SetView {
        id: id.into(),
        room: if with { "with" } else { "no" }.into(),
        room_name: if with { "With room" } else { "No room" }.into(),
        term: if with { "BRIR" } else { "HRTF" }.into(),
        label: label.into(),
        licence: "CC-BY-4.0".into(),
        credit: None,
        source_url: "https://example.org/set".into(),
    };
    let mut view = computer_view(false, None, 0.0);
    view.eq.as_mut().unwrap().speakers = Some(svolder_host::eq::SpeakersView {
        on,
        set: chosen.map(str::to_string),
        licence: "CC-BY-4.0".into(),
        sets: vec![set("a", "Set A", false), set("b", "Set B", true)],
        ..Default::default()
    });
    view
}

fn speakers_calls(switch: &RecordingSwitch) -> Vec<SpeakersCall> {
    switch.speakers.lock().unwrap().clone()
}

#[tokio::test]
async fn the_speakers_are_announced_and_their_states_follow_the_view() {
    let (mut task, tx, _switch) =
        with_computer(speakers_computer_view(true, Some("a")), Ok(())).await;
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.configs.len(), 6);
    assert!(published.configs.contains_key(
        "homeassistant/select/svolder_computer_reframe/computer_speakers_set/config"
    ));
    let state = published.speakers_state.clone().unwrap();
    assert_eq!(
        (state.on, state.set.as_str()),
        ("ON", "Set A · No room · HRTF")
    );
    assert!(state.attributes.contains("CC-BY-4.0"));

    tx.send_replace(speakers_computer_view(false, Some("b")));
    task.changed("computer_reframe");
    task.publish_computer();
    let state = task
        .computer
        .as_ref()
        .unwrap()
        .speakers_state
        .clone()
        .unwrap();
    assert_eq!(
        (state.on, state.set.as_str()),
        ("OFF", "Set B · With room · BRIR")
    );

    // A controller that no longer serves the speakers takes its entities down, not the EQ's.
    tx.send_replace(computer_view(false, None, 0.0));
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.configs.len(), 4);
    assert_eq!(published.speakers_state, None);
}

#[tokio::test]
async fn speakers_commands_reach_the_controller_as_one_change_each() {
    let (mut task, _tx, switch) =
        with_computer(speakers_computer_view(false, Some("a")), Ok(())).await;
    task.message(
        "svolder/computer_reframe/computer_speakers/set",
        b"ON",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_speakers/set",
        b" off ",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_speakers_set/set",
        " set b \u{b7} with room \u{b7} brir ".as_bytes(),
        false,
    );
    settle(&mut task).await;
    let mut calls = speakers_calls(&switch);
    calls.sort();
    let mut want: Vec<SpeakersCall> = vec![
        (Some(true), None),
        (Some(false), None),
        (None, Some("b".into())),
    ];
    want.sort();
    assert_eq!(calls, want);
    assert!(computer_calls(&switch).is_empty() && crossfeed_calls(&switch).is_empty());
}

#[tokio::test]
async fn speakers_commands_the_host_cannot_take_do_nothing() {
    // ON with no set chosen, an unknown set, a payload that is no text or no switch word, a
    // controller without speakers, one whose speakers are unavailable, host audio off.
    let (mut task, _tx, switch) = with_computer(speakers_computer_view(false, None), Ok(())).await;
    task.message(
        "svolder/computer_reframe/computer_speakers/set",
        b"ON",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_speakers/set",
        b"toggle",
        false,
    );
    for body in [&b"nonsense"[..], b"Set A", &[0xff]] {
        task.message(
            "svolder/computer_reframe/computer_speakers_set/set",
            body,
            false,
        );
    }
    settle(&mut task).await;
    assert!(speakers_calls(&switch).is_empty());
    let mut unavailable = speakers_computer_view(false, Some("a"));
    unavailable
        .eq
        .as_mut()
        .unwrap()
        .speakers
        .as_mut()
        .unwrap()
        .unavailable = Some("computer controls unavailable: x".into());
    for view in [
        computer_view(false, None, 0.0),
        unavailable,
        svolder_ipc::host::HostView::default(),
    ] {
        let (mut task, _tx, switch) = with_computer(view, Ok(())).await;
        task.message(
            "svolder/computer_reframe/computer_speakers/set",
            b"OFF",
            false,
        );
        task.message(
            "svolder/computer_reframe/computer_speakers_set/set",
            "Set A \u{b7} No room \u{b7} HRTF".as_bytes(),
            false,
        );
        settle(&mut task).await;
        assert!(speakers_calls(&switch).is_empty());
    }
}

/// [`speakers_computer_view`] with the crossfeed served too, on or off as `crossfeed_on` says.
fn both_computer_view(
    crossfeed_on: bool,
    speakers_on: bool,
    chosen: Option<&str>,
) -> svolder_ipc::host::HostView {
    let mut view = speakers_computer_view(speakers_on, chosen);
    view.eq.as_mut().unwrap().crossfeed = crossfeed_computer_view(crossfeed_on, "bauer", None)
        .eq
        .unwrap()
        .crossfeed;
    view
}

#[tokio::test]
async fn a_set_that_goes_leaves_the_select_unknown_and_the_attributes_empty() {
    let (mut task, tx, _switch) =
        with_computer(speakers_computer_view(false, Some("a")), Ok(())).await;
    task.publish_computer();
    let state = task
        .computer
        .as_ref()
        .unwrap()
        .speakers_state
        .clone()
        .unwrap();
    assert!(state.attributes_payload().contains("CC-BY-4.0"));
    // The chosen set is forgotten: the view keeps the choice's id away and lists only "b".
    let mut view = speakers_computer_view(false, None);
    view.eq
        .as_mut()
        .unwrap()
        .speakers
        .as_mut()
        .unwrap()
        .sets
        .remove(0);
    tx.send_replace(view);
    task.changed("computer_reframe");
    task.publish_computer();
    let state = task
        .computer
        .as_ref()
        .unwrap()
        .speakers_state
        .clone()
        .unwrap();
    assert_eq!(
        (state.set_payload(), state.attributes_payload()),
        ("None", "{}")
    );
}

#[tokio::test]
async fn the_set_list_changing_announces_the_select_again_with_the_new_options() {
    let (mut task, tx, _switch) = with_computer(speakers_computer_view(false, None), Ok(())).await;
    task.publish_computer();
    let topic = "homeassistant/select/svolder_computer_reframe/computer_speakers_set/config";
    let before = task.computer.as_ref().unwrap().configs[topic].clone();
    assert!(!before.contains("Set C"));
    let mut view = speakers_computer_view(false, None);
    let speakers = view.eq.as_mut().unwrap().speakers.as_mut().unwrap();
    let mut third = speakers.sets[0].clone();
    (third.id, third.label) = ("c".into(), "Set C".into());
    speakers.sets.push(third);
    tx.send_replace(view);
    task.changed("computer_reframe");
    task.publish_computer();
    let after = task.computer.as_ref().unwrap().configs[topic].clone();
    assert!(after.contains("Set C · No room · HRTF"), "{after}");
}

#[tokio::test]
async fn the_speakers_going_on_shows_the_crossfeed_switch_off_in_the_same_pass() {
    let (mut task, tx, _switch) =
        with_computer(both_computer_view(true, false, Some("a")), Ok(())).await;
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.crossfeed_state.as_ref().map(|s| s.0), Some("ON"));
    assert_eq!(published.speakers_state.as_ref().map(|s| s.on), Some("OFF"));
    tx.send_replace(both_computer_view(false, true, Some("a")));
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.crossfeed_state.as_ref().map(|s| s.0), Some("OFF"));
    assert_eq!(published.speakers_state.as_ref().map(|s| s.on), Some("ON"));
}

#[tokio::test]
async fn a_refused_speakers_command_is_followed_by_the_views_state() {
    let (mut task, _tx, switch) =
        with_computer(speakers_computer_view(false, Some("a")), Err("no".into())).await;
    task.publish_computer();
    task.message(
        "svolder/computer_reframe/computer_speakers/set",
        b"ON",
        false,
    );
    settle(&mut task).await;
    assert_eq!(speakers_calls(&switch), [(Some(true), None)]);
    assert!(task.computer.as_ref().unwrap().dirty);
    task.publish_computer();
    assert_eq!(
        task.computer
            .as_ref()
            .unwrap()
            .speakers_state
            .as_ref()
            .map(|s| s.on),
        Some("OFF")
    );
}

#[tokio::test]
async fn a_retained_speakers_command_and_one_with_commands_off_do_nothing() {
    let (mut task, _tx, switch) =
        with_computer(speakers_computer_view(false, Some("a")), Ok(())).await;
    task.message(
        "svolder/computer_reframe/computer_speakers/set",
        b"ON",
        true,
    );
    task.message(
        "svolder/computer_reframe/computer_speakers_set/set",
        "Set B \u{b7} With room \u{b7} BRIR".as_bytes(),
        true,
    );
    task.config.commands = false;
    task.message(
        "svolder/computer_reframe/computer_speakers/set",
        b"ON",
        false,
    );
    settle(&mut task).await;
    assert!(speakers_calls(&switch).is_empty());
}

/// A computer view whose controller serves loudness, as `on` and `strength` say.
fn loudness_computer_view(on: bool, strength: f64) -> svolder_ipc::host::HostView {
    let mut view = computer_view(false, None, 0.0);
    view.eq.as_mut().unwrap().loudness = Some(svolder_host::eq::LoudnessView {
        on,
        strength_pct: strength,
        ..Default::default()
    });
    view
}

fn loudness_calls(switch: &RecordingSwitch) -> Vec<LoudnessCall> {
    switch.loudnesses.lock().unwrap().clone()
}

#[tokio::test]
async fn the_loudness_is_announced_and_its_states_follow_the_view() {
    let (mut task, tx, _switch) = with_computer(loudness_computer_view(true, 35.0), Ok(())).await;
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.configs.len(), 6);
    assert!(
        published
            .configs
            .contains_key("homeassistant/switch/svolder_computer_reframe/computer_loudness/config")
    );
    assert!(published.configs.contains_key(
        "homeassistant/number/svolder_computer_reframe/computer_loudness_strength/config"
    ));
    assert_eq!(published.loudness_state, Some(("ON", "35".to_string())));

    tx.send_replace(loudness_computer_view(false, 50.0));
    task.changed("computer_reframe");
    task.publish_computer();
    assert_eq!(
        task.computer.as_ref().unwrap().loudness_state,
        Some(("OFF", "50".to_string()))
    );

    // A view without loudness takes its entities down, not the EQ's.
    tx.send_replace(computer_view(false, None, 0.0));
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.configs.len(), 4);
    assert_eq!(published.loudness_state, None);
}

#[tokio::test]
async fn host_audio_off_clears_the_loudness_with_the_computer() {
    let (mut task, tx, _switch) = with_computer(loudness_computer_view(true, 35.0), Ok(())).await;
    task.publish_computer();
    assert!(task.computer.as_ref().unwrap().loudness_state.is_some());
    tx.send_replace(svolder_ipc::host::HostView::default());
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert!(published.configs.is_empty() && published.loudness_state.is_none());
}

#[tokio::test]
async fn loudness_commands_reach_the_controller_as_one_change_each() {
    let (mut task, _tx, switch) = with_computer(loudness_computer_view(false, 50.0), Ok(())).await;
    task.message(
        "svolder/computer_reframe/computer_loudness/set",
        b"ON",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_loudness/set",
        b" off ",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_loudness_strength/set",
        b"35",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_loudness_strength/set",
        b" 37.0 ",
        false,
    );
    settle(&mut task).await;
    assert_eq!(
        loudness_calls(&switch),
        [
            (Some(true), None),
            (Some(false), None),
            (None, Some(35.0)),
            (None, Some(37.0)),
        ]
    );
    // Nothing else of the computer was touched.
    assert!(computer_calls(&switch).is_empty());
    assert!(crossfeed_calls(&switch).is_empty());
}

#[tokio::test]
async fn bad_loudness_commands_change_nothing() {
    let (mut task, _tx, switch) = with_computer(loudness_computer_view(true, 35.0), Ok(())).await;
    for body in [&b"loud"[..], b"", b"NaN", &[0xff]] {
        task.message(
            "svolder/computer_reframe/computer_loudness_strength/set",
            body,
            false,
        );
    }
    task.message(
        "svolder/computer_reframe/computer_loudness/set",
        b"toggle",
        false,
    );
    settle(&mut task).await;
    assert!(loudness_calls(&switch).is_empty());
}

#[tokio::test]
async fn a_loudness_command_while_loudness_cannot_follow_the_volume_still_reaches_the_controller() {
    let mut view = loudness_computer_view(true, 50.0);
    view.eq
        .as_mut()
        .unwrap()
        .loudness
        .as_mut()
        .unwrap()
        .unavailable = Some("computer controls unavailable: x".into());
    let (mut task, _tx, switch) = with_computer(view, Ok(())).await;
    task.message(
        "svolder/computer_reframe/computer_loudness/set",
        b"OFF",
        false,
    );
    task.message(
        "svolder/computer_reframe/computer_loudness_strength/set",
        b"35",
        false,
    );
    settle(&mut task).await;
    assert_eq!(
        loudness_calls(&switch),
        [(Some(false), None), (None, Some(35.0))]
    );
}

#[tokio::test]
async fn a_loudness_command_the_host_cannot_take_does_nothing() {
    // No loudness in the view, host audio off, no commands, and a retained command: none
    // reaches the controller.
    for view in [
        computer_view(false, None, 0.0),
        svolder_ipc::host::HostView::default(),
    ] {
        let (mut task, _tx, switch) = with_computer(view, Ok(())).await;
        task.message(
            "svolder/computer_reframe/computer_loudness/set",
            b"ON",
            false,
        );
        task.message(
            "svolder/computer_reframe/computer_loudness_strength/set",
            b"35",
            false,
        );
        settle(&mut task).await;
        assert!(loudness_calls(&switch).is_empty());
    }
    let (mut task, _tx, switch) = with_computer(loudness_computer_view(false, 50.0), Ok(())).await;
    task.message(
        "svolder/computer_reframe/computer_loudness/set",
        b"ON",
        true,
    );
    task.message(
        "svolder/computer_reframe/computer_loudness_strength/set",
        b"35",
        true,
    );
    task.config.commands = false;
    task.message(
        "svolder/computer_reframe/computer_loudness/set",
        b"ON",
        false,
    );
    settle(&mut task).await;
    assert!(loudness_calls(&switch).is_empty());
    assert!(
        task.retained_commands
            .contains("svolder/computer_reframe/computer_loudness_strength/set")
    );
}

#[tokio::test]
async fn a_refused_loudness_command_is_followed_by_the_views_state() {
    let (mut task, _tx, switch) =
        with_computer(loudness_computer_view(false, 50.0), Err("no".into())).await;
    task.publish_computer();
    task.message(
        "svolder/computer_reframe/computer_loudness/set",
        b"ON",
        false,
    );
    settle(&mut task).await;
    assert_eq!(loudness_calls(&switch), [(Some(true), None)]);
    task.publish_computer();
    assert_eq!(
        task.computer.as_ref().unwrap().loudness_state,
        Some(("OFF", "50".to_string()))
    );
}

/// A route on a sink named `product` (a Schiit one when `schiit`), the player's stream at
/// `player_hz`, with `setup` on it.
fn path_route(
    product: &str,
    schiit: bool,
    player_hz: u32,
    setup: Option<svolder_host::SetupFacts>,
) -> svolder_host::RouteFacts {
    use svolder_host::{PlayerMatch, RouteFacts, SinkFacts, StreamFacts};
    RouteFacts {
        sink: Some(SinkFacts {
            name: "alsa_output.test".into(),
            description: product.into(),
            rate_hz: Some(48_000),
            format: Some("S32LE".into()),
            volume_milli: Some(1000),
            muted: false,
            usb_vendor_id: schiit.then_some(svolder_ipc::host::SCHIIT_VENDOR_ID),
            usb_product: schiit.then(|| product.to_string()),
        }),
        player: Some(StreamFacts {
            rate_hz: Some(player_hz),
            format: Some("S32LE".into()),
            volume_milli: Some(1000),
            muted: false,
        }),
        player_match: PlayerMatch::Pid,
        others_mixing: 0,
        host_eq: false,
        stages: None,
        setup,
    }
}

/// The config facts of a sink that lets every line through, with a filter in the sink when
/// `filter` is set.
fn path_setup(filter: bool) -> svolder_host::SetupFacts {
    use svolder_host::{AlsaFacts, ClockFacts, Dither, Layout, Rates, SetupFacts, SinkOffer};
    let stereo = Layout {
        channels: 2,
        positions: vec!["FL".into(), "FR".into()],
    };
    SetupFacts {
        offer: Some(SinkOffer {
            formats: vec!["S32LE".into(), "S24LE".into(), "S16LE".into()],
            rates: Some(Rates::Range {
                min: 44_100,
                max: 384_000,
            }),
            layout: Some(stereo.clone()),
        }),
        clock: Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: vec![
                44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
            ],
            force_rate: None,
        }),
        sink_layout: Some(stereo.clone()),
        stream_layout: Some(stereo),
        dither: Some(Dither {
            method: "none".into(),
            noise: 0,
        }),
        sink_filter: Some(filter),
        stream_filter: Some(false),
        alsa: Some(AlsaFacts {
            path: Some("hw:Audio".into()),
            ..AlsaFacts::default()
        }),
        ..SetupFacts::default()
    }
}

/// A computer view whose host reports `route`, with the EQ view every computer entity needs.
fn path_view(route: Option<svolder_host::RouteFacts>) -> svolder_ipc::host::HostView {
    svolder_ipc::host::HostView {
        route,
        ..computer_view(false, None, 0.0)
    }
}

/// A task that follows a computer whose view is `view`, beside a Gungnir 2 reading USB 1 at
/// 48 kHz, holding the route reads through `slow` when it is given.
async fn with_dac_computer(
    view: svolder_ipc::host::HostView,
    slow: Option<crate::host::SlowHolds>,
) -> (
    BridgeTask,
    watch::Sender<svolder_ipc::host::HostView>,
    Stand,
) {
    with_dac_computer_at(1, view, slow).await
}

/// [`with_dac_computer`] with the task's client connecting to the broker on `port`.
async fn with_dac_computer_at(
    port: u16,
    view: svolder_ipc::host::HostView,
    slow: Option<crate::host::SlowHolds>,
) -> (
    BridgeTask,
    watch::Sender<svolder_ipc::host::HostView>,
    Stand,
) {
    let (mut task, stand) = published_stand_at("gungnir2-v2", port).await;
    let profile = svolder_profiles::Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    stand.feed.send_modify(|feed| {
        feed.readings = vec![
            crate::web::view::test_support::read(&profile, "input", "USB 1"),
            crate::web::view::test_support::read(&profile, "sample_rate", "2"),
            crate::web::view::test_support::read(&profile, "usb1_connected", "true"),
            crate::web::view::test_support::read(&profile, "usb2_connected", "false"),
        ];
    });
    let (tx, rx) = watch::channel(view);
    let switch = Arc::new(RecordingSwitch {
        loudnesses: std::sync::Mutex::new(Vec::new()),
        calls: std::sync::Mutex::new(Vec::new()),
        crossfeeds: std::sync::Mutex::new(Vec::new()),
        speakers: std::sync::Mutex::new(Vec::new()),
        answer: Ok(()),
    });
    task.follow_computer(ComputerLink {
        view: rx,
        switch,
        slow,
        scenes: None,
    });
    (task, tx, stand)
}

/// What the computer last published for the path sensor and the config sensor, as attribute
/// JSON.
fn path_said(task: &BridgeTask) -> (&'static str, serde_json::Value) {
    let said = task.computer.as_ref().unwrap().path_state.clone().unwrap();
    (said.state, serde_json::from_str(&said.attributes).unwrap())
}

fn config_said(task: &BridgeTask) -> (Option<&'static str>, serde_json::Value) {
    let said = task
        .computer
        .as_ref()
        .unwrap()
        .config_state
        .clone()
        .unwrap();
    (said.state, serde_json::from_str(&said.attributes).unwrap())
}

#[tokio::test]
async fn an_untouched_path_to_the_dac_reads_untouched_with_a_config_that_is_off() {
    let route = path_route("Schiit Gungnir 2", true, 48_000, Some(path_setup(false)));
    let (mut task, _tx, _stand) = with_dac_computer(path_view(Some(route)), None).await;
    task.publish_computer();
    let (state, attributes) = path_said(&task);
    assert_eq!(state, "untouched");
    assert_eq!(attributes["dac"], "Gungnir 2");
    assert_eq!(attributes["words"][0], "untouched");
    assert!(
        attributes["sentence"]
            .as_str()
            .unwrap()
            .starts_with("untouched")
    );
    let (problem, attributes) = config_said(&task);
    assert_eq!(problem, Some("OFF"));
    assert_eq!(
        attributes["summary"],
        "PipeWire config lets it through untouched"
    );
    assert_eq!(attributes["lines"].as_array().unwrap().len(), 7);
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.configs.len(), 4);
    assert!(
        published
            .configs
            .contains_key("homeassistant/sensor/svolder_computer_reframe/computer_path/config")
    );
    assert!(published.configs.contains_key(
        "homeassistant/binary_sensor/svolder_computer_reframe/computer_config/config"
    ));
}

#[tokio::test]
async fn a_changed_path_and_a_failing_config_line_read_changed_and_on() {
    // The player's stream is 44.1 kHz into a 48 kHz sink: resampled. A filter sits in the sink.
    let route = path_route("Schiit Gungnir 2", true, 44_100, Some(path_setup(true)));
    let (mut task, _tx, _stand) = with_dac_computer(path_view(Some(route)), None).await;
    task.publish_computer();
    let (state, attributes) = path_said(&task);
    assert_eq!(state, "changed");
    assert_eq!(attributes["dac"], "Gungnir 2");
    assert!(!attributes["words"].as_array().unwrap().is_empty());
    let (problem, attributes) = config_said(&task);
    assert_eq!(problem, Some("ON"));
    let filter = attributes["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["key"] == "filter")
        .unwrap();
    assert_eq!(filter["outcome"], "fail");
    assert_eq!(attributes["outcome"], "changes");
    assert_eq!(attributes["limit"], svolder_ipc::host::SETUP_LIMIT);
}

#[tokio::test]
async fn a_sink_that_is_no_configured_dac_reads_not_this_dac_and_names_the_sink() {
    let route = path_route("DELL S2721QS", false, 48_000, Some(path_setup(false)));
    let (mut task, _tx, _stand) = with_dac_computer(path_view(Some(route)), None).await;
    task.publish_computer();
    let (state, attributes) = path_said(&task);
    assert_eq!(state, "not_this_dac");
    assert_eq!(attributes["words"], serde_json::json!(["→ DELL S2721QS"]));
    assert_eq!(attributes["sentence"], "the computer plays to DELL S2721QS");
    assert!(attributes["dac"].is_null());
    let (problem, attributes) = config_said(&task);
    assert_eq!(problem, None, "the config of another sink is not judged");
    assert_eq!(attributes, serde_json::json!({}));
}

/// The path sensor's states are the judge's verdict keys and `not_read`, no other word.
#[test]
fn the_path_states_are_the_judges_keys_and_not_read() {
    let mut keys = svolder_ipc::host::Verdict::KEYS.to_vec();
    keys.push("not_read");
    assert_eq!(keys, svolder_mqtt::computer::path::STATES);
}

#[tokio::test]
async fn an_unread_route_or_config_is_unknown_never_clean() {
    let (mut task, tx, _stand) = with_dac_computer(path_view(None), None).await;
    task.publish_computer();
    let (state, attributes) = path_said(&task);
    assert_eq!(state, "not_read");
    assert_eq!(attributes["words"], serde_json::json!(["route not read"]));
    assert_eq!(config_said(&task).0, None);

    // A route whose config was not sent (an older daemon): the path is judged, the config is not.
    tx.send_replace(path_view(Some(path_route(
        "Schiit Gungnir 2",
        true,
        48_000,
        None,
    ))));
    task.changed("computer_reframe");
    task.publish_computer();
    assert_eq!(path_said(&task).0, "untouched");
    let (problem, attributes) = config_said(&task);
    assert_eq!(problem, None);
    assert_eq!(
        attributes["summary"],
        "PipeWire config not read (svolderd's version differs)"
    );
}

#[tokio::test]
async fn the_path_follows_the_route_and_the_dac_and_goes_with_the_computer() {
    let route = path_route("Schiit Gungnir 2", true, 48_000, Some(path_setup(false)));
    let (mut task, tx, stand) = with_dac_computer(path_view(Some(route.clone())), None).await;
    task.publish_computer();
    assert_eq!(path_said(&task).0, "untouched");

    // The DAC's sample rate step moves to 44.1 kHz: the device changed, so the computer is marked.
    let profile = svolder_profiles::Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    stand.feed.send_modify(|feed| {
        feed.readings
            .retain(|m| m.id != profile.control_by_key("sample_rate").unwrap().id);
        feed.readings.push(crate::web::view::test_support::read(
            &profile,
            "sample_rate",
            "1",
        ));
    });
    task.computer.as_mut().unwrap().dirty = false;
    task.changed(UNIT);
    assert!(
        task.computer.as_ref().unwrap().dirty,
        "a DAC reading moves the path"
    );
    task.publish_computer();
    assert_eq!(path_said(&task).0, "changed");

    // Host audio off: both entities and their states are taken down with the computer.
    tx.send_replace(svolder_ipc::host::HostView::default());
    task.changed("computer_reframe");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert!(published.path_state.is_none() && published.config_state.is_none());
    assert!(published.configs.is_empty());
}

#[tokio::test]
async fn the_slow_hold_is_kept_while_connected_and_published_and_dropped_otherwise() {
    use crate::host::{HostMode, HostService};
    use svolder_ipc::config::HostConfig;
    let service = HostService::start(&HostConfig::default(), HostMode::Off);
    let route = path_route("Schiit Gungnir 2", true, 48_000, Some(path_setup(false)));
    let (mut task, tx, _stand) =
        with_dac_computer(path_view(Some(route)), Some(service.slow_holds())).await;
    task.publish_computer();
    assert_eq!(
        service.slow_count(),
        0,
        "no hold before the broker is reached"
    );

    task.computer_link(true);
    task.publish_computer();
    assert_eq!(service.slow_count(), 1, "connected and published");
    task.changed("computer_reframe");
    task.publish_computer();
    assert_eq!(
        service.slow_count(),
        1,
        "one hold, however often it publishes"
    );

    task.computer_link(false);
    assert_eq!(service.slow_count(), 0, "a lost link lets go at once");
    task.computer_link(true);
    task.publish_computer();
    assert_eq!(service.slow_count(), 1);

    tx.send_replace(svolder_ipc::host::HostView::default());
    task.changed("computer_reframe");
    task.publish_computer();
    assert_eq!(service.slow_count(), 0, "nothing published, nothing held");

    tx.send_replace(path_view(Some(path_route(
        "Schiit Gungnir 2",
        true,
        48_000,
        None,
    ))));
    task.changed("computer_reframe");
    task.publish_computer();
    assert_eq!(service.slow_count(), 1);
    task.computer_farewell();
    assert_eq!(service.slow_count(), 0, "a stop lets go");
}

/// A config that goes from judged to unjudged resets Home Assistant's sensor to unknown: the
/// payload sent is `None`, never an empty one (which it ignores, keeping the last state).
#[cfg(feature = "broker-tests")]
#[tokio::test]
async fn a_config_that_stops_being_judged_sends_the_unknown_payload() {
    let broker = broker::TestBroker::start();
    let mut watcher = broker::Watcher::connect(broker.port, &["svolder/computer_reframe/#"]).await;
    let route = path_route("Schiit Gungnir 2", true, 48_000, Some(path_setup(false)));
    let (mut task, tx, _stand) =
        with_dac_computer_at(broker.port, path_view(Some(route)), None).await;
    task.publish_computer();
    let topic = "svolder/computer_reframe/computer_config/state";
    watcher
        .wait_for_each(&[(topic, "OFF")], Duration::from_secs(5))
        .await;
    // The sink becomes one that is no configured DAC.
    tx.send_replace(path_view(Some(path_route(
        "DELL S2721QS",
        false,
        48_000,
        Some(path_setup(false)),
    ))));
    task.changed("computer_reframe");
    task.publish_computer();
    let seen = watcher
        .wait_for_each(&[(topic, "None")], Duration::from_secs(5))
        .await;
    assert_eq!(seen.len(), 1, "the unknown payload arrives: {seen:#?}");
    let after = watcher.collect(Duration::from_millis(300)).await;
    assert!(
        after
            .iter()
            .all(|s| s.topic != topic || !s.payload.is_empty()),
        "an empty payload is ignored by Home Assistant: {after:#?}"
    );
}

#[tokio::test]
async fn the_slow_hold_is_kept_with_discovery_off_too() {
    use crate::host::{HostMode, HostService};
    use svolder_ipc::config::HostConfig;
    let service = HostService::start(&HostConfig::default(), HostMode::Off);
    let route = path_route("Schiit Gungnir 2", true, 48_000, Some(path_setup(false)));
    let (mut task, _tx, _stand) =
        with_dac_computer(path_view(Some(route)), Some(service.slow_holds())).await;
    task.config.discovery_prefix = String::new();
    task.computer_link(true);
    task.publish_computer();
    assert!(task.computer.as_ref().unwrap().configs.is_empty());
    assert_eq!(path_said(&task).0, "untouched", "the state is published");
    assert_eq!(service.slow_count(), 1, "so the route is read for it");
}

#[tokio::test]
async fn forgetting_the_dac_marks_the_computer_for_a_new_judgement() {
    let route = path_route("Schiit Gungnir 2", true, 48_000, Some(path_setup(false)));
    let (mut task, _tx, _stand) = with_dac_computer(path_view(Some(route)), None).await;
    task.publish_computer();
    assert_eq!(path_said(&task).1["dac"], "Gungnir 2");
    assert!(!task.computer.as_ref().unwrap().dirty);
    task.forget(UNIT);
    assert!(task.computer.as_ref().unwrap().dirty);
    task.publish_computer();
    let (state, attributes) = path_said(&task);
    assert_eq!(state, "not_this_dac");
    assert!(attributes["dac"].is_null());
}

/// Every discovery topic the releases before per-computer ids announced for the computer, both
/// component kinds, written out from their code (unit `computer`).
#[cfg(feature = "broker-tests")]
fn old_computer_topics() -> Vec<String> {
    let at = |component: &str, key: &str| {
        format!("homeassistant/{component}/svolder_computer/{key}/config")
    };
    vec![
        at("switch", "computer_eq"),
        at("binary_sensor", "computer_eq"),
        at("sensor", "computer_eq_preset"),
        at("switch", "computer_crossfeed"),
        at("binary_sensor", "computer_crossfeed"),
        at("select", "computer_crossfeed_preset"),
        at("sensor", "computer_crossfeed_preset"),
        at("switch", "computer_speakers"),
        at("binary_sensor", "computer_speakers"),
        at("select", "computer_speakers_set"),
        at("sensor", "computer_speakers_set"),
        at("switch", "computer_loudness"),
        at("binary_sensor", "computer_loudness"),
        at("number", "computer_loudness_strength"),
        at("sensor", "computer_loudness_strength"),
        at("sensor", "computer_path"),
        at("binary_sensor", "computer_config"),
    ]
}

/// The retained availability, state and attributes topics the earlier releases kept under the
/// unit `computer`, written out from their code.
#[cfg(feature = "broker-tests")]
fn old_computer_state_topics() -> Vec<String> {
    [
        "availability",
        "computer_eq/state",
        "computer_eq_preset/state",
        "computer_crossfeed/state",
        "computer_crossfeed_preset/state",
        "computer_loudness/state",
        "computer_loudness_strength/state",
        "computer_speakers/state",
        "computer_speakers_set/state",
        "computer_path/state",
        "computer_config/state",
        "computer_speakers_set/attributes",
        "computer_path/attributes",
        "computer_config/attributes",
    ]
    .iter()
    .map(|leaf| format!("svolder/computer/{leaf}"))
    .collect()
}

/// A device's first discovery waits while its computer still waits for its first EQ view (bounded
/// by that wait), so the computer's device is announced first.
#[tokio::test]
async fn a_devices_first_discovery_waits_for_the_computer_to_report() {
    let (mut task, tx, _switch) =
        with_computer(svolder_ipc::host::HostView::default(), Ok(())).await;
    task.config.discovery_prefix = "homeassistant".into();
    task.flush();
    assert!(!task.computer.as_ref().unwrap().swept);
    assert!(
        task.devices[UNIT].configs.is_empty(),
        "the device was announced before its computer"
    );
    tx.send_replace(computer_view(true, Some("Warm"), -3.0));
    task.changed("computer_reframe");
    task.flush();
    assert!(task.computer.as_ref().unwrap().swept);
    assert!(!task.computer.as_ref().unwrap().configs.is_empty());
    assert!(!task.devices[UNIT].configs.is_empty());
}

/// A computer that never reports an EQ (host audio off) holds its devices' first discovery for
/// the start wait only: a flush is owed at its end, and the devices are announced then.
#[tokio::test(start_paused = true)]
async fn a_device_is_announced_after_the_wait_when_the_computer_never_reports() {
    let (mut task, _tx, _switch) =
        with_computer(svolder_ipc::host::HostView::default(), Ok(())).await;
    task.config.discovery_prefix = "homeassistant".into();
    let due = task.computer.as_ref().unwrap().since + super::computer::START_SETTLES;
    task.flush();
    assert!(task.devices[UNIT].configs.is_empty());
    assert_eq!(
        task.flush_at,
        Some(due),
        "a flush is owed at the end of the wait"
    );
    tokio::time::advance(super::computer::START_SETTLES).await;
    task.flush();
    assert!(task.computer.as_ref().unwrap().swept);
    assert!(!task.devices[UNIT].configs.is_empty());
}

/// The first publish of a bridge clears the old `This computer` discovery topics, exactly the
/// list the earlier releases announced, and none of the named computer's own.
#[cfg(feature = "broker-tests")]
#[tokio::test]
async fn the_first_start_clears_the_old_this_computer_topics() {
    let broker = broker::TestBroker::start();
    let mut watcher =
        broker::Watcher::connect(broker.port, &["homeassistant/#", "svolder/#"]).await;
    let mut old = old_computer_topics();
    old.extend(old_computer_state_topics());
    for topic in &old {
        watcher.retain(topic, "{\"name\":\"This computer\"}").await;
    }
    // An older daemon's bridge availability is not this bridge's to clear.
    watcher
        .retain("svolder/bridge/availability", "online")
        .await;
    let (mut task, _tx, _switch) =
        with_computer_at(broker.port, computer_view(true, Some("Warm"), -3.0), Ok(())).await;
    task.publish_computer();
    let cleared = watcher
        .wait_for_each(
            &old.iter().map(|t| (t.as_str(), "")).collect::<Vec<_>>(),
            Duration::from_secs(5),
        )
        .await;
    assert_eq!(cleared.len(), old.len());
    let seen = watcher.collect(Duration::from_millis(500)).await;
    assert!(
        seen.iter()
            .all(|s| s.topic != "svolder/bridge/availability" || !s.payload.is_empty()),
        "the old bridge availability was cleared: {seen:#?}"
    );
    assert!(
        seen.iter()
            .filter(|s| {
                s.topic == "homeassistant/switch/svolder_computer_reframe/computer_eq/config"
            })
            .all(|s| !s.payload.is_empty()),
        "the announced EQ switch was cleared: {seen:#?}"
    );
}

fn hostname_file(tag: &str, text: Option<&str>) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("svolder-hostname-{tag}-{}", std::process::id()));
    match text {
        Some(text) => std::fs::write(&path, text).unwrap(),
        None => drop(std::fs::remove_file(&path)),
    }
    path
}

#[test]
fn the_host_name_is_read_trimmed_and_falls_back_to_localhost() {
    let named = hostname_file("named", Some("  Reframe\n"));
    assert_eq!(read_hostname(&named), "Reframe");
    let empty = hostname_file("empty", Some("\n"));
    assert_eq!(read_hostname(&empty), "localhost");
    let missing = hostname_file("missing", None);
    assert_eq!(read_hostname(&missing), "localhost");
    for path in [named, empty] {
        std::fs::remove_file(path).unwrap();
    }
}

/// A desk that records the scene operations it was given and answers each with `answer`.
struct RecordingDesk {
    ops: std::sync::Mutex<Vec<svolder_ipc::socket::protocol::SceneOp>>,
    answer: std::sync::Mutex<Response>,
}

#[async_trait::async_trait]
impl SceneDesk for RecordingDesk {
    async fn request(&self, op: svolder_ipc::socket::protocol::SceneOp) -> Response {
        self.ops.lock().unwrap().push(op);
        self.answer.lock().unwrap().clone()
    }
}

fn scene_row(name: &str) -> svolder_ipc::socket::protocol::SceneRow {
    svolder_ipc::socket::protocol::SceneRow {
        name: name.into(),
        parts: vec![svolder_ipc::socket::protocol::PartRow {
            who: "computer".into(),
            present: true,
        }],
    }
}

fn scenes_view(
    names: &[&str],
    on: Option<&str>,
    known: bool,
) -> svolder_ipc::socket::protocol::ScenesView {
    svolder_ipc::socket::protocol::ScenesView {
        scenes: names.iter().map(|n| scene_row(n)).collect(),
        on: on.map(str::to_string),
        recalled: on.map(str::to_string),
        known,
    }
}

fn scene_done(name: &str, status: svolder_ipc::socket::protocol::PartStatus) -> Response {
    Response::Scene {
        name: name.into(),
        outcome: vec![svolder_ipc::socket::protocol::PartOutcome {
            who: "computer".into(),
            status,
            notes: Vec::new(),
        }],
    }
}

/// A task following a computer whose host reports no EQ and the scenes of `view`, with commands as
/// `commands` says and discovery on.
async fn with_scenes(
    commands: bool,
    view: svolder_ipc::socket::protocol::ScenesView,
) -> (
    BridgeTask,
    watch::Sender<svolder_ipc::socket::protocol::ScenesView>,
    Arc<RecordingDesk>,
    Arc<RecordingSwitch>,
) {
    let (mut task, _tx, switch) =
        with_computer(svolder_ipc::host::HostView::default(), Ok(())).await;
    task.config.commands = commands;
    task.config.discovery_prefix = "homeassistant".into();
    let (tx, rx) = watch::channel(view);
    let desk = Arc::new(RecordingDesk {
        ops: std::sync::Mutex::new(Vec::new()),
        answer: std::sync::Mutex::new(Response::Unknown),
    });
    task.follow_scenes(ScenesLink {
        view: rx,
        desk: desk.clone(),
    });
    task.flush();
    (task, tx, desk, switch)
}

fn scene_ops(desk: &RecordingDesk) -> Vec<svolder_ipc::socket::protocol::SceneOp> {
    desk.ops.lock().unwrap().clone()
}

/// `answer` without its `seq`, after checking it has one.
fn without_seq(answer: &serde_json::Value) -> serde_json::Value {
    assert!(answer["seq"].is_u64(), "{answer}");
    let mut answer = answer.clone();
    answer.as_object_mut().unwrap().remove("seq");
    answer
}

fn attributes_of(task: &BridgeTask) -> serde_json::Value {
    serde_json::from_str(task.scenes.as_ref().unwrap().attributes.as_ref().unwrap()).unwrap()
}

const SCENE_SELECT: &str = "homeassistant/select/svolder_computer_reframe/scene/config";
const SCENE_TEXT: &str = "homeassistant/text/svolder_computer_reframe/scene_save/config";

#[tokio::test]
async fn the_pair_is_announced_with_no_host_eq_and_before_the_computer_has_reported() {
    let (task, _tx, _desk, _switch) =
        with_scenes(true, scenes_view(&["Late night", "Desk"], None, true)).await;
    assert!(
        !task.computer.as_ref().unwrap().swept,
        "the host has not reported"
    );
    let configs = &task.scenes.as_ref().unwrap().configs;
    assert_eq!(
        configs.keys().map(String::as_str).collect::<Vec<_>>(),
        [SCENE_SELECT, SCENE_TEXT]
    );
    let select: serde_json::Value = serde_json::from_str(&configs[SCENE_SELECT]).unwrap();
    assert_eq!(
        select["options"],
        serde_json::json!(["Late night", "Desk", "custom"])
    );
    assert_eq!(
        select["device"]["identifiers"][0],
        "svolder_computer_reframe"
    );
}

#[tokio::test]
async fn commands_off_announce_a_sensor_and_no_text_and_take_no_command() {
    let (mut task, _tx, desk, _switch) =
        with_scenes(false, scenes_view(&["Desk"], Some("Desk"), true)).await;
    let keys: Vec<_> = task
        .scenes
        .as_ref()
        .unwrap()
        .configs
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        ["homeassistant/sensor/svolder_computer_reframe/scene/config"]
    );
    task.message("svolder/computer_reframe/scene/set", b"Desk", false);
    task.message("svolder/computer_reframe/scene_save/set", b"Quiet", false);
    settle(&mut task).await;
    assert!(scene_ops(&desk).is_empty());
}

#[tokio::test]
async fn the_state_is_the_scene_on_else_custom_and_an_unknown_view_leaves_it_as_it_was() {
    let (mut task, tx, _desk, _switch) =
        with_scenes(true, scenes_view(&["Desk", "Movie"], None, false)).await;
    assert_eq!(
        task.scenes.as_ref().unwrap().state,
        None,
        "nothing is said while it is not known"
    );
    tx.send_replace(scenes_view(&["Desk", "Movie"], Some("Desk"), true));
    task.changed("scenes");
    task.flush();
    assert_eq!(task.scenes.as_ref().unwrap().state.as_deref(), Some("Desk"));
    // A reconnect read: not known, no scene on. The state stays.
    tx.send_replace(scenes_view(&["Desk", "Movie"], None, false));
    task.changed("scenes");
    task.flush();
    assert_eq!(task.scenes.as_ref().unwrap().state.as_deref(), Some("Desk"));
    assert_eq!(attributes_of(&task)["known"], false);
    // Known, and none is on: custom.
    tx.send_replace(scenes_view(&["Desk", "Movie"], None, true));
    task.changed("scenes");
    task.flush();
    assert_eq!(
        task.scenes.as_ref().unwrap().state.as_deref(),
        Some("custom")
    );
}

#[tokio::test]
async fn the_attributes_carry_the_scenes_known_recalled_and_no_outcome_before_one() {
    let (task, _tx, _desk, _switch) =
        with_scenes(true, scenes_view(&["Desk"], Some("Desk"), true)).await;
    assert_eq!(
        attributes_of(&task),
        serde_json::json!({
            "scenes": [{"name": "Desk", "parts": [{"who": "computer", "present": true}]}],
            "known": true,
            "recalled": "Desk",
        })
    );
}

#[tokio::test]
async fn the_options_follow_a_rename_once_the_view_changes() {
    let (mut task, tx, _desk, _switch) =
        with_scenes(true, scenes_view(&["Desk", "Movie"], None, true)).await;
    tx.send_replace(scenes_view(&["Desk", "Cinema"], None, true));
    task.changed("scenes");
    task.flush();
    let select: serde_json::Value =
        serde_json::from_str(&task.scenes.as_ref().unwrap().configs[SCENE_SELECT]).unwrap();
    assert_eq!(
        select["options"],
        serde_json::json!(["Desk", "Cinema", "custom"])
    );
}

#[tokio::test]
async fn choosing_a_scene_recalls_it_once_and_keeps_the_answer_as_the_outcome() {
    use svolder_ipc::socket::protocol::{PartStatus, SceneOp};
    let (mut task, _tx, desk, switch) =
        with_scenes(true, scenes_view(&["Late night", "Movie"], None, true)).await;
    *desk.answer.lock().unwrap() = scene_done("Movie", PartStatus::Partial);
    task.message("svolder/computer_reframe/scene/set", b" movie ", false);
    settle(&mut task).await;
    task.flush();
    assert_eq!(
        scene_ops(&desk),
        [SceneOp::Recall {
            name: "Movie".into(),
            confirm: false
        }]
    );
    assert_eq!(
        without_seq(&attributes_of(&task)["outcome"]),
        serde_json::json!({"name": "Movie", "parts": [{"who": "computer", "status": "partial"}]})
    );
    // The scene pair writes nothing of its own: no switch call, no write to the device.
    assert!(computer_calls(&switch).is_empty());
    assert!(switch.crossfeeds.lock().unwrap().is_empty());
    assert!(switch.speakers.lock().unwrap().is_empty());
    assert!(switch.loudnesses.lock().unwrap().is_empty());
    assert!(view(&task).await.in_flight.is_empty());
}

#[tokio::test]
async fn custom_an_unknown_name_a_bad_payload_and_a_retained_command_recall_nothing() {
    let (mut task, _tx, desk, _switch) =
        with_scenes(true, scenes_view(&["Movie"], None, true)).await;
    task.message("svolder/computer_reframe/scene/set", b"custom", false);
    task.message("svolder/computer_reframe/scene/set", b"Party", false);
    task.message("svolder/computer_reframe/scene/set", &[0xff], false);
    task.message("svolder/computer_reframe/scene/set", b"Movie", true);
    task.message("svolder/computer_reframe/scene_save/set", b"Quiet", true);
    task.message("svolder/computer_reframe/scene_save/set", b"   ", false);
    settle(&mut task).await;
    assert!(scene_ops(&desk).is_empty(), "{:?}", scene_ops(&desk));
}

#[tokio::test]
async fn a_typed_name_saves_without_force_and_a_refusal_goes_in_saved_in_the_daemons_words() {
    use svolder_ipc::socket::protocol::{Reason, SceneOp};
    let (mut task, _tx, desk, _switch) =
        with_scenes(true, scenes_view(&["Movie"], None, true)).await;
    *desk.answer.lock().unwrap() = Response::Scene {
        name: "Quiet".into(),
        outcome: Vec::new(),
    };
    task.message(
        "svolder/computer_reframe/scene_save/set",
        b"  Quiet ",
        false,
    );
    settle(&mut task).await;
    task.flush();
    assert_eq!(
        without_seq(&attributes_of(&task)["saved"]),
        serde_json::json!({"name": "Quiet"})
    );
    *desk.answer.lock().unwrap() = Response::Error {
        code: 1,
        message: "a scene named Movie exists".into(),
        reason: Some(Reason::Taken),
    };
    task.message("svolder/computer_reframe/scene_save/set", b"Movie", false);
    settle(&mut task).await;
    task.flush();
    assert_eq!(
        without_seq(&attributes_of(&task)["saved"]),
        serde_json::json!({"name": "Movie", "error": "a scene named Movie exists"})
    );
    assert_eq!(
        scene_ops(&desk),
        [
            SceneOp::Save {
                name: "Quiet".into(),
                force: false
            },
            SceneOp::Save {
                name: "Movie".into(),
                force: false
            },
        ]
    );
    assert!(attributes_of(&task).get("outcome").is_none());
}

#[tokio::test]
async fn a_recall_the_daemon_asks_about_or_refuses_is_an_outcome_with_its_words_and_no_parts() {
    use svolder_ipc::socket::protocol::Reason;
    let (mut task, _tx, desk, _switch) =
        with_scenes(true, scenes_view(&["Movie"], None, true)).await;
    *desk.answer.lock().unwrap() = Response::Ask {
        message: "Movie plays twice. Recall it anyway?".into(),
        name: "Movie".into(),
        devices: vec!["Desk".into()],
    };
    task.message("svolder/computer_reframe/scene/set", b"Movie", false);
    settle(&mut task).await;
    task.flush();
    assert_eq!(
        without_seq(&attributes_of(&task)["outcome"]),
        serde_json::json!({"name": "Movie", "error": "Movie plays twice. Recall it anyway?"})
    );
    *desk.answer.lock().unwrap() = Response::Error {
        code: 1,
        message: "busy".into(),
        reason: Some(Reason::Busy),
    };
    task.message("svolder/computer_reframe/scene/set", b"Movie", false);
    settle(&mut task).await;
    task.flush();
    assert_eq!(scene_ops(&desk).len(), 2);
    assert_eq!(
        without_seq(&attributes_of(&task)["outcome"]),
        serde_json::json!({"name": "Movie", "error": "busy"})
    );
}

/// Two identical answers (the same recall, the same refusal) are two different attribute payloads,
/// each `seq` above the one before.
#[tokio::test]
async fn a_repeated_answer_is_a_new_payload_with_a_higher_seq() {
    use svolder_ipc::socket::protocol::{PartStatus, Reason};
    let (mut task, _tx, desk, _switch) =
        with_scenes(true, scenes_view(&["Movie"], None, true)).await;
    let mut seen = Vec::new();
    for _ in 0..2 {
        *desk.answer.lock().unwrap() = scene_done("Movie", PartStatus::Done);
        task.message("svolder/computer_reframe/scene/set", b"Movie", false);
        settle(&mut task).await;
        task.flush();
        let a = attributes_of(&task);
        seen.push((a.to_string(), a["outcome"]["seq"].as_u64().unwrap()));
    }
    assert_ne!(seen[0].0, seen[1].0);
    assert!(seen[1].1 > seen[0].1, "{seen:?}");
    let mut saves = Vec::new();
    for _ in 0..2 {
        *desk.answer.lock().unwrap() = Response::Error {
            code: 1,
            message: "a scene named Movie exists".into(),
            reason: Some(Reason::Taken),
        };
        task.message("svolder/computer_reframe/scene_save/set", b"Movie", false);
        settle(&mut task).await;
        task.flush();
        let a = attributes_of(&task);
        saves.push((a.to_string(), a["saved"]["seq"].as_u64().unwrap()));
    }
    assert_ne!(saves[0].0, saves[1].0);
    assert!(saves[1].1 > saves[0].1, "{saves:?}");
}

/// The scene select's config reaches the broker before any device's, whatever the topics sort as:
/// both are owed in the first flush, held until the connect, and the client replays the parent
/// rank (the computer's) first. A DAC's `binary_sensor` link config sorts before a `select`.
#[cfg(feature = "broker-tests")]
#[tokio::test]
async fn the_scene_configs_are_put_back_before_a_devices() {
    let broker = broker::TestBroker::start();
    let mut watcher = broker::Watcher::connect(broker.port, &["homeassistant/#"]).await;
    let (mut task, _tx, _switch) =
        with_computer_at(broker.port, computer_view(true, Some("Warm"), -3.0), Ok(())).await;
    task.config.discovery_prefix = "homeassistant".into();
    task.config.commands = true;
    let (_scenes_tx, rx) = watch::channel(scenes_view(&["Desk"], None, true));
    task.follow_scenes(ScenesLink {
        view: rx,
        desk: Arc::new(RecordingDesk {
            ops: std::sync::Mutex::new(Vec::new()),
            answer: std::sync::Mutex::new(Response::Unknown),
        }),
    });
    task.flush();
    let seen = watcher.collect(Duration::from_secs(2)).await;
    let topics: Vec<&str> = seen.iter().map(|s| s.topic.as_str()).collect();
    let select = topics
        .iter()
        .position(|t| *t == SCENE_SELECT)
        .unwrap_or_else(|| panic!("no scene select: {topics:#?}"));
    let device = topics
        .iter()
        .position(|t| t.contains(&format!("svolder_{UNIT}/")))
        .unwrap_or_else(|| panic!("no device config: {topics:#?}"));
    assert!(select < device, "the device came first: {topics:#?}");
}
