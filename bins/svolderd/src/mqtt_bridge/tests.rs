// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::commands::{clamp_lines, command_target, set_value};
use super::discovery::{discovery_unit, fitting_presets, subscriptions};
use super::*;
use crate::device::Command;
use crate::recall;
use crate::test_support::shown;
use std::sync::atomic::{AtomicUsize, Ordering};
use svolder_ipc::Response;
use svolder_profiles::{EqMode, tone::preset};
use svolder_proto::Value;

/// A broker in this process and a client that watches it, for the tests that look at what the
/// bridge's client sends rather than at what the task holds.
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
                let _ = reply.send(Response::Error {
                    code: 3,
                    message: "the device said no".into(),
                });
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
    task.enqueue(
        control("eq_preset"),
        WriteRequest::Preset {
            name: "Warm".into(),
        },
    );
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

/// A switch that records what it was told and answers as `answer` says.
struct RecordingSwitch {
    calls: std::sync::Mutex<Vec<bool>>,
    answer: Result<(), String>,
}

#[async_trait::async_trait]
impl crate::host::EqSwitch for RecordingSwitch {
    async fn set_on(&self, on: bool) -> Result<(), String> {
        self.calls.lock().unwrap().push(on);
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
        calls: std::sync::Mutex::new(Vec::new()),
        answer,
    });
    task.follow_computer(ComputerLink {
        view: rx,
        switch: switch.clone(),
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
    task.message("svolder/computer/computer_eq/set", b"ON", false);
    settle(&mut task).await;
    assert_eq!(computer_calls(&switch), [true]);
    task.message("svolder/computer/computer_eq/set", b" off ", false);
    settle(&mut task).await;
    assert_eq!(computer_calls(&switch), [true, false]);
    // Anything else, and any other key, is dropped.
    task.message("svolder/computer/computer_eq/set", b"toggle", false);
    task.message("svolder/computer/computer_eq/set", &[0xff], false);
    task.message("svolder/computer/volume/set", b"50", false);
    task.message("svolder/computer/computer_eq_preset/set", b"Warm", false);
    settle(&mut task).await;
    assert_eq!(computer_calls(&switch), [true, false]);
}

#[tokio::test]
async fn on_with_nothing_kept_stays_off() {
    let (mut task, _tx, switch) = with_computer(computer_view(false, None, 0.0), Ok(())).await;
    task.message("svolder/computer/computer_eq/set", b"ON", false);
    settle(&mut task).await;
    assert!(computer_calls(&switch).is_empty());
    // OFF is still sent: it removes a chain the view does not know of.
    task.message("svolder/computer/computer_eq/set", b"OFF", false);
    settle(&mut task).await;
    assert_eq!(computer_calls(&switch), [false]);
}

#[tokio::test]
async fn a_command_with_the_computer_unavailable_or_without_commands_does_nothing() {
    let mut view = computer_view(false, Some("Warm"), -3.0);
    view.eq.as_mut().unwrap().unavailable =
        Some("computer controls unavailable: no pw-cli on this computer".into());
    let (mut task, _tx, switch) = with_computer(view, Ok(())).await;
    task.message("svolder/computer/computer_eq/set", b"ON", false);
    let (mut off, _tx2, switch2) =
        with_computer(computer_view(false, Some("Warm"), -3.0), Ok(())).await;
    off.config.commands = false;
    off.message("svolder/computer/computer_eq/set", b"ON", false);
    settle(&mut task).await;
    settle(&mut off).await;
    assert!(computer_calls(&switch).is_empty());
    assert!(computer_calls(&switch2).is_empty());
}

#[tokio::test]
async fn a_retained_computer_command_is_not_acted_on() {
    let (mut task, _tx, switch) =
        with_computer(computer_view(false, Some("Warm"), -3.0), Ok(())).await;
    task.message("svolder/computer/computer_eq/set", b"ON", true);
    settle(&mut task).await;
    assert!(computer_calls(&switch).is_empty());
    assert!(
        task.retained_commands
            .contains("svolder/computer/computer_eq/set")
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
    assert_eq!(published.configs.len(), 2);

    tx.send_replace(computer_view(true, Some("custom"), -3.0));
    task.changed("computer");
    task.publish_computer();
    assert_eq!(
        task.computer.as_ref().unwrap().state,
        Some(("ON", "custom".to_string()))
    );

    tx.send_replace(computer_view(false, Some("custom"), -3.0));
    task.changed("computer");
    task.publish_computer();
    assert_eq!(
        task.computer.as_ref().unwrap().state,
        Some(("OFF", String::new()))
    );

    // Unavailable: the entities stay, offline.
    let mut view = computer_view(false, None, 0.0);
    view.eq.as_mut().unwrap().unavailable = Some("computer controls unavailable: x".into());
    tx.send_replace(view);
    task.changed("computer");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert_eq!(published.available, Some(false));
    assert_eq!(published.configs.len(), 2);

    // Host off: everything is taken down.
    tx.send_replace(svolder_ipc::host::HostView::default());
    task.changed("computer");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert!(published.configs.is_empty() && published.available.is_none());
}

/// The retained configs an earlier run left under the computer, which Home Assistant would
/// remove and add again if they were cleared before being announced.
fn left_over() -> [String; 3] {
    [
        "homeassistant/switch/svolder_computer/computer_eq/config".to_string(),
        "homeassistant/sensor/svolder_computer/computer_eq_preset/config".to_string(),
        // Announced only by a bridge that does not take commands: this one clears it.
        "homeassistant/binary_sensor/svolder_computer/computer_eq/config".to_string(),
    ]
}

/// A start whose host view has not yet reported the computer clears nothing before it does:
/// the entities about to be announced are never cleared first, and the ones it will not serve
/// are cleared once it has.
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
    task.changed("computer");
    task.publish_computer();
    let published = task.computer.as_ref().unwrap();
    assert!(published.swept);
    assert_eq!(published.configs.len(), 2);
    let [switch, preset, binary] = left_over();
    let seen = watcher.collect(Duration::from_millis(700)).await;
    assert!(
        seen.iter()
            .any(|s| s.topic == binary && s.payload.is_empty()),
        "the config this bridge will not serve is cleared: {seen:#?}"
    );
    assert!(
        seen.iter()
            .all(|s| !(s.topic == switch || s.topic == preset) || !s.payload.is_empty()),
        "an entity about to be announced is cleared: {seen:#?}"
    );
}

/// A computer that never reports one (host audio off) is swept once the start has had its
/// time: nothing is cleared before, and every retained config is after.
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
    assert_eq!(cleared.len(), 3);
}

#[tokio::test]
async fn read_only_the_computer_announces_a_binary_sensor_and_subscribes_to_nothing() {
    let (mut task, _tx, _switch) =
        with_computer(computer_view(true, Some("Warm"), -3.0), Ok(())).await;
    task.config.commands = false;
    task.config.discovery_prefix = "homeassistant".into();
    task.changed("computer");
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
            .any(|t| t.contains("/binary_sensor/svolder_computer/")),
        "{topics:?}"
    );
    assert!(topics.iter().all(|t| !t.contains("/switch/")));
}
