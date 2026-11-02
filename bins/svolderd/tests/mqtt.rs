// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MQTT bridge end to end: a real `svolderd` over the fake device, bridged to a broker
//! running in this process on 127.0.0.1 — `rumqttd`, a port of each case's own — and watched
//! from there by a client of the case's own, as Home Assistant would watch it.

mod common;

#[path = "../../../crates/svolder-mqtt/tests/support/broker.rs"]
mod broker;

use std::time::Duration;

use serde_json::Value as Json;
use svolder_ipc::{Client, MqttStatus, Request, Response};

use broker::{Seen, TestBroker, Watcher};
use common::{
    CONFIG, ChildGuard, connect, spawn_daemon_isolated, spawn_daemon_isolated_with_profiles,
    wait_until_connected,
};

/// The fake device's unit id, which every one of its topics is built from.
const UNIT: &str = "0102030405060708";

/// How long one step waits for the broker to show something. A step that is met returns at
/// once; the length is what a machine running every suite at the same time needs.
const STEP: Duration = Duration::from_secs(20);

/// How long a whole case may run: a daemon, a broker and several steps.
const TEST_TIMEOUT: Duration = Duration::from_secs(150);

/// The one-device config with an `[mqtt]` section for the broker on `port`, and `extra` lines
/// in that section.
fn bridged(port: u16, extra: &str) -> String {
    format!("{CONFIG}[mqtt]\nenabled = true\nhost = \"127.0.0.1\"\nport = {port}\n{extra}")
}

/// A daemon started on `config`, with `envs`, connected to its fake device.
async fn daemon_on(config: &str, envs: &[(&str, &str)]) -> (ChildGuard, Client) {
    let (mut child, socket) = spawn_daemon_isolated(config, envs);
    let mut client = connect(&mut child, &socket).await;
    wait_until_connected(&mut child, &mut client).await;
    (child, client)
}

/// What the daemon reports for one control of its device, as `get` answers.
async fn reading(client: &mut Client, key: &str) -> svolder_ipc::ValueObject {
    let answer = client
        .request(Request::Get {
            device: None,
            key: key.into(),
        })
        .await
        .expect("a get reply");
    match answer {
        Response::Value(vo) => vo,
        other => panic!("get {key} answered {other:?}"),
    }
}

/// Wait until the device's volume, in dB, is what `wanted` accepts, or fail naming it.
async fn until_volume(client: &mut Client, wanted: impl Fn(f64) -> bool) -> f64 {
    let deadline = tokio::time::Instant::now() + STEP;
    loop {
        let db = reading(client, "volume").await.value.expect("volume in dB");
        if wanted(db) {
            return db;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the volume stayed at {db} dB"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A watcher of everything the bridge publishes: its own topics and its discovery configs.
async fn watch(broker: &TestBroker) -> Watcher {
    Watcher::connect(broker.port, &["svolder/#", "homeassistant/#"]).await
}

/// A watcher that subscribes once the bridge has published the whole of the fake device's
/// state, so what it is given first is the broker's retained copy of everything and nothing
/// more is published but what a test causes.
async fn watch_settled(broker: &TestBroker) -> Watcher {
    let keys = settled_keys();
    let mut first = watch(broker).await;
    first
        .wait_for(&device_topic("state"), STEP, |seen| {
            let state = json(seen);
            keys.iter().all(|key| state.get(key).is_some())
        })
        .await;
    // The session asks for the firmware builds after its refresh, one at a time, and the last
    // of them is the last thing it publishes.
    first
        .wait_for(&device_topic("dsp_firmware/state"), STEP, |_| true)
        .await;
    watch(broker).await
}

/// The keys the fake device's state carries once the bridge has published everything the
/// device has read: the keys of the controls the bridge exposes for the profile the fake plays,
/// less the meters, which go out on their own topic. Until then the bridge has published the
/// placeholder profile's partial state or a state with some controls still unread.
fn settled_keys() -> Vec<String> {
    let registry = svolder_profiles::Registry::builtin().expect("the built-in profiles");
    let profile = registry
        .get("gungnir2-v2")
        .expect("the gungnir2-v2 profile");
    svolder_mqtt::discovery::exposed_controls(profile)
        .into_iter()
        .filter(|control| control.kind != svolder_profiles::ControlKind::Meter)
        .map(|control| control.key)
        .collect()
}

/// A watcher that subscribes once the bridge has published a device state `ready` accepts.
async fn watch_after(broker: &TestBroker, ready: impl Fn(&Json) -> bool) -> Watcher {
    let mut first = watch(broker).await;
    first
        .wait_for(&device_topic("state"), STEP, |seen| ready(&json(seen)))
        .await;
    watch(broker).await
}

/// The topic under the device's own base.
fn device_topic(leaf: &str) -> String {
    format!("svolder/{UNIT}/{leaf}")
}

/// One discovery topic of the fake device.
fn config_topic(component: &str, key: &str) -> String {
    format!("homeassistant/{component}/svolder_{UNIT}/{key}/config")
}

/// A payload as JSON, or a failure naming it.
fn json(seen: &Seen) -> Json {
    serde_json::from_str(&seen.payload)
        .unwrap_or_else(|e| panic!("{} is not JSON ({e}): {:?}", seen.topic, seen.payload))
}

/// A running daemon's retained topics are on the broker for whoever subscribes: the bridge and
/// the device online, the state in human units, and a read-only discovery config per control.
#[tokio::test]
async fn the_retained_configs_and_state_are_on_the_broker() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (_child, _client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;

        // Everything a fresh subscription is given, by topic: the broker's retained copies.
        let given: std::collections::BTreeMap<String, Seen> = watcher
            .collect(Duration::from_secs(1))
            .await
            .into_iter()
            .map(|seen| (seen.topic.clone(), seen))
            .collect();
        let at = |topic: &str| {
            given
                .get(topic)
                .unwrap_or_else(|| panic!("nothing retained on {topic}; given {given:#?}"))
        };
        assert!(given.values().all(|s| s.retained), "{given:#?}");
        assert_eq!(at("svolder/bridge/availability").payload, "online");
        assert_eq!(at(&device_topic("availability")).payload, "online");
        let state = json(at(&device_topic("state")));
        assert!(
            state["volume"].as_f64().is_some_and(|db| db < 0.0),
            "{state}"
        );
        assert_eq!(state["mute"], "OFF", "{state}");
        assert!(state["input"].is_string(), "{state}");

        let volume = json(at(&config_topic("sensor", "volume")));
        assert_eq!(volume["unique_id"], format!("svolder_{UNIT}_volume"));
        assert_eq!(volume["state_topic"], device_topic("state"));
        assert!(volume.get("command_topic").is_none(), "read-only: {volume}");
        let link = at(&config_topic("binary_sensor", "link"));
        assert_eq!(json(link)["device_class"], "connectivity");
    })
    .await
    .expect("test timed out");
}

/// The diagnostic entities are announced read-only too, and the firmware builds the fake device
/// reports are on their own retained topics as plain numbers. The identify button is not offered
/// without commands.
#[tokio::test]
async fn the_diagnostic_sensors_are_announced_and_the_firmware_reported() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (_child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut first = watch(&broker).await;
        // The daemon holds a firmware build once something asked the device for it: ask, as the
        // TUI's telemetry screen does.
        for id in ["210.2.1", "210.2.2"] {
            let asked = client
                .request(Request::Raw {
                    device: None,
                    id: id.into(),
                    value_type: "data".into(),
                    value: String::new(),
                    no_wait: false,
                })
                .await
                .expect("a raw reply");
            assert!(matches!(asked, Response::Value(_)), "{id}: {asked:?}");
        }
        for leaf in ["firmware/state", "dsp_firmware/state"] {
            let seen = first.wait_for(&device_topic(leaf), STEP, |_| true).await;
            assert!(
                seen.payload.parse::<u16>().is_ok(),
                "{leaf} is a number: {:?}",
                seen.payload
            );
        }
        let mut watcher = watch(&broker).await;
        let given = watcher.collect(Duration::from_secs(1)).await;
        let at = |leaf: &str| {
            given
                .iter()
                .find(|s| s.topic == device_topic(leaf))
                .unwrap_or_else(|| panic!("nothing retained on {leaf}: {given:#?}"))
        };
        assert!(at("firmware/state").retained && at("dsp_firmware/state").retained);
        // The fake advertises at -50 dBm, heard as an advert.
        assert_eq!(at("signal/state").payload, "-50");
        let attributes = json(at("signal/attributes"));
        assert_eq!(attributes["source"], "advert");
        assert!(
            attributes["heard_at"]
                .as_str()
                .is_some_and(|stamp| stamp.len() == 20 && stamp.ends_with('Z')),
            "{attributes}"
        );
        for key in ["signal", "firmware", "dsp_firmware"] {
            let config = given
                .iter()
                .find(|s| s.topic == config_topic("sensor", key))
                .unwrap_or_else(|| panic!("no {key} sensor in {given:#?}"));
            assert_eq!(json(config)["entity_category"], "diagnostic");
        }
        assert!(
            !given
                .iter()
                .any(|s| s.topic.starts_with("homeassistant/button/")),
            "no button read-only: {given:#?}"
        );
    })
    .await
    .expect("test timed out");
}

/// With commands on the identify button is announced, and a press on its topic reaches the
/// device: the session's one send per gap is spent, so a request of the daemon's own made right
/// after it sends nothing.
#[tokio::test]
async fn an_identify_press_reaches_the_device() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (_child, mut client) = daemon_on(&bridged(broker.port, "commands = true\n"), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        let button = watcher
            .wait_for(&config_topic("button", "identify"), STEP, |s| s.retained)
            .await;
        let button = json(&button);
        assert_eq!(button["payload_press"], "PRESS");
        assert_eq!(button["entity_category"], "config");
        assert_eq!(button["command_topic"], device_topic("identify/set"));

        // Not a press: nothing is sent, so the gap is still free.
        watcher.publish(&device_topic("identify/set"), "ON").await;
        watcher.collect(Duration::from_millis(500)).await;
        watcher
            .publish(&device_topic("identify/set"), "PRESS")
            .await;
        watcher.collect(Duration::from_millis(500)).await;
        let answer = client
            .request(Request::Identify { device: None })
            .await
            .expect("an identify reply");
        assert!(
            matches!(answer, Response::Identify { sent: false, .. }),
            "the press should have spent the gap: {answer:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A change the device confirms is on the state topic within moments.
#[tokio::test]
async fn a_change_publishes_the_state() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (_child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for(&device_topic("state"), STEP, |s| s.retained)
            .await;

        let set = client
            .request(Request::Set {
                device: None,
                key: "mute".into(),
                value: "on".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .expect("a set reply");
        assert!(matches!(set, Response::Value(_)), "got {set:?}");
        let state = watcher
            .wait_for(&device_topic("state"), STEP, |s| json(s)["mute"] == "ON")
            .await;
        assert!(
            !state.retained,
            "a live publish reaches a subscriber as it happens"
        );
    })
    .await
    .expect("test timed out");
}

/// A device taken out of the file has its discovery configs, its state and its availability
/// cleared from the broker, so its entities leave Home Assistant.
#[tokio::test]
async fn a_device_taken_out_of_the_file_has_its_topics_cleared() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for(&config_topic("sensor", "volume"), STEP, |s| s.retained)
            .await;

        let without = format!(
            "schema = 1\n[mqtt]\nenabled = true\nhost = \"127.0.0.1\"\nport = {}\n",
            broker.port
        );
        std::fs::write(child.config_path(), without).expect("take the device out");
        let reloaded = client.request(Request::Reload).await.expect("a reload");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices.is_empty()),
            "got {reloaded:?}"
        );
        watcher
            .wait_for_each(
                &[
                    (&config_topic("sensor", "volume"), ""),
                    (&config_topic("binary_sensor", "link"), ""),
                    (&device_topic("state"), ""),
                    (&device_topic("signal/state"), ""),
                    (&device_topic("signal/attributes"), ""),
                    (&device_topic("availability"), ""),
                ],
                STEP,
            )
            .await;

        // A fresh subscription is given nothing of the device's.
        let mut later = watch(&broker).await;
        let kept: Vec<Seen> = later
            .collect(Duration::from_millis(500))
            .await
            .into_iter()
            .filter(|s| s.topic.contains(UNIT))
            .collect();
        assert!(kept.is_empty(), "left on the broker: {kept:#?}");
    })
    .await
    .expect("test timed out");
}

/// A discovery config an earlier run left under the device — a `number` from a run that took
/// commands, say — is cleared by a run that announces no such entity.
#[tokio::test]
async fn a_config_no_control_announces_any_more_is_cleared() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let mut watcher = watch(&broker).await;
        let earlier = config_topic("number", "volume");
        watcher.retain(&earlier, "{\"name\":\"Volume\"}").await;
        watcher
            .wait_for(&earlier, STEP, |s| !s.payload.is_empty())
            .await;

        let (_child, _client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        watcher
            .wait_for(&earlier, STEP, |s| s.payload.is_empty())
            .await;
        let mut later = watch(&broker).await;
        let kept: Vec<Seen> = later
            .collect(Duration::from_millis(500))
            .await
            .into_iter()
            .filter(|s| s.topic == earlier)
            .collect();
        assert!(kept.is_empty(), "left on the broker: {kept:#?}");
    })
    .await
    .expect("test timed out");
}

/// A section that does not turn the bridge on opens no connection: the broker it names hears
/// nothing from the daemon.
#[tokio::test]
async fn a_section_that_is_not_enabled_publishes_nothing() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let config = format!(
            "{CONFIG}[mqtt]\nenabled = false\nhost = \"127.0.0.1\"\nport = {}\n",
            broker.port
        );
        let (_child, _client) = daemon_on(&config, &[]).await;
        let mut watcher = Watcher::connect(broker.port, &["#"]).await;
        let heard = watcher.collect(Duration::from_secs(1)).await;
        assert!(heard.is_empty(), "the broker heard {heard:#?}");
    })
    .await
    .expect("test timed out");
}

/// The meters of a device that streams them go out on their own topic, not retained, and no
/// more than once a second.
#[tokio::test]
async fn the_meters_go_out_at_most_once_a_second() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let envs = [
            ("SVOLDER_FAKE_PROFILE", "fake-amp"),
            ("SVOLDER_FAKE_SIGNAL", "1"),
        ];
        let (_child, _client) = daemon_on(&bridged(broker.port, ""), &envs).await;
        let mut watcher = Watcher::connect(broker.port, &[&device_topic("meters")]).await;
        watcher
            .wait_for(&device_topic("meters"), STEP, |_| true)
            .await;
        let started = tokio::time::Instant::now();
        let heard = watcher.collect(Duration::from_millis(3500)).await;
        let elapsed = started.elapsed();
        assert!(!heard.is_empty(), "the meters stopped");
        assert!(
            heard.len() as u128 <= elapsed.as_millis() / 1000 + 1,
            "{} publishes in {elapsed:?}",
            heard.len()
        );
        for seen in &heard {
            assert!(!seen.retained, "{seen:?}");
            assert!(
                json(seen).as_object().is_some_and(|m| !m.is_empty()),
                "{seen:?}"
            );
        }
    })
    .await
    .expect("test timed out");
}

/// Switching the device's meters off while they stream sends one payload of zeros, and then
/// nothing more while they are dead, however many readings the device goes on sending.
#[tokio::test]
async fn switching_the_meters_off_sends_one_payload_of_zeros_and_then_nothing() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let envs = [
            ("SVOLDER_FAKE_PROFILE", "fake-amp"),
            ("SVOLDER_FAKE_SIGNAL", "1"),
        ];
        let (_child, mut client) = daemon_on(&bridged(broker.port, ""), &envs).await;
        let mut watcher = Watcher::connect(broker.port, &[&device_topic("meters")]).await;
        watcher
            .wait_for(&device_topic("meters"), STEP, |_| true)
            .await;
        let set = client
            .request(Request::Set {
                device: None,
                key: "power_meters".into(),
                value: "off".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .expect("a set reply");
        assert!(matches!(set, Response::Value(_)), "got {set:?}");
        let zeros = |seen: &Seen| {
            json(seen)
                .as_object()
                .is_some_and(|m| !m.is_empty() && m.values().all(|v| v == 0))
        };
        watcher.wait_for(&device_topic("meters"), STEP, zeros).await;
        let heard = watcher.collect(Duration::from_millis(2500)).await;
        assert!(heard.is_empty(), "meters went out while dead: {heard:#?}");
    })
    .await
    .expect("test timed out");
}

/// The broker password comes from `SVOLDER_MQTT_PASSWORD` when it is set, over the file's.
#[tokio::test]
async fn the_password_in_the_environment_wins_over_the_file() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::with_login("svolder", "secret");
        let config = bridged(
            broker.port,
            "username = \"svolder\"\npassword = \"wrong\"\n",
        );
        let (_child, _client) = daemon_on(&config, &[("SVOLDER_MQTT_PASSWORD", "secret")]).await;
        let mut watcher = Watcher::connect_as(
            broker.port,
            &["svolder/bridge/availability"],
            Some(("svolder", "secret")),
        )
        .await;
        watcher
            .wait_for("svolder/bridge/availability", STEP, |s| {
                s.payload == "online"
            })
            .await;
    })
    .await
    .expect("test timed out");
}

/// With commands on, a command on a control's topic moves the fake device, in the control's
/// own units: a level in dB, a toggle `ON`/`OFF`, a select by its label, the tone-control mode
/// by its name; and the state topic follows.
#[tokio::test]
async fn a_command_moves_the_device_with_commands_on() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (_child, mut client) = daemon_on(&bridged(broker.port, "commands = true\n"), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        let number = watcher
            .wait_for(&config_topic("number", "volume"), STEP, |s| s.retained)
            .await;
        assert_eq!(json(&number)["command_topic"], device_topic("volume/set"));

        watcher.publish(&device_topic("volume/set"), "-20").await;
        let db = until_volume(&mut client, |db| (db + 20.0).abs() < 0.5).await;
        watcher
            .wait_for(&device_topic("state"), STEP, |s| {
                json(s)["volume"].as_f64() == Some(db)
            })
            .await;

        watcher.publish(&device_topic("mute/set"), "ON").await;
        watcher.publish(&device_topic("input/set"), "Coaxial").await;
        watcher
            .publish(&device_topic("eq_mode/set"), "loudness")
            .await;
        watcher
            .wait_for(&device_topic("state"), STEP, |s| {
                let state = json(s);
                state["mute"] == "ON"
                    && state["input"] == "Coaxial"
                    && state["eq_mode"] == "loudness"
            })
            .await;
        assert_eq!(
            reading(&mut client, "mute").await.raw,
            serde_json::json!(true)
        );
    })
    .await
    .expect("test timed out");
}

/// Read-only, the default: a command on a control's topic moves nothing.
#[tokio::test]
async fn a_command_moves_nothing_read_only() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (_child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        let before = reading(&mut client, "volume").await.raw;
        watcher.publish(&device_topic("volume/set"), "-20").await;
        watcher.publish(&device_topic("mute/set"), "ON").await;
        let heard = watcher.collect(Duration::from_secs(1)).await;
        assert!(
            !heard
                .iter()
                .any(|s| s.topic == device_topic("state") && !s.retained),
            "the state changed: {heard:#?}"
        );
        assert_eq!(reading(&mut client, "volume").await.raw, before);
        assert_eq!(
            reading(&mut client, "mute").await.raw,
            serde_json::json!(false)
        );
    })
    .await
    .expect("test timed out");
}

/// `max_volume_db` caps a volume command before it is sent — never above the cap — and the
/// daemon says so, while the volume's slider still reaches the device's own maximum: Home
/// Assistant ignores a state outside a `number`'s range, and the device can be turned up by
/// its knob or any other front end.
#[tokio::test]
async fn max_volume_db_caps_a_volume_command() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let config = bridged(broker.port, "commands = true\nmax_volume_db = -30.0\n");
        let (child, mut client) = daemon_on(&config, &[]).await;
        let mut watcher = watch_settled(&broker).await;
        let slider = watcher
            .wait_for(&config_topic("number", "volume"), STEP, |s| s.retained)
            .await;
        assert_eq!(
            json(&slider)["max"],
            serde_json::json!(0.0),
            "the slider's max"
        );
        watcher.publish(&device_topic("volume/set"), "-5").await;
        let db = until_volume(&mut client, |db| (db + 30.0).abs() < 0.5).await;
        assert!(db <= -30.0, "the cap let {db} dB through");
        let said = child.said();
        assert!(
            said.contains("volume -5 dB capped at max_volume_db, -30 dB"),
            "the clamp went unlogged: {said}"
        );
    })
    .await
    .expect("test timed out");
}

/// A cap below every step of the device sends the quietest step, and the daemon's log says the
/// cap is below every step rather than that the volume was capped at it.
#[tokio::test]
async fn a_cap_below_every_step_logs_the_quietest_step() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let config = bridged(broker.port, "commands = true\nmax_volume_db = -300.0\n");
        let (child, mut client) = daemon_on(&config, &[("SVOLDER_FAKE_PROFILE", "freya2")]).await;
        let watcher = watch_after(&broker, |state| state.get("volume").is_some()).await;
        watcher.publish(&device_topic("volume/set"), "-5").await;
        let db = until_volume(&mut client, |db| db <= -79.9).await;
        assert!(db <= -79.9, "{db}");
        let said = child.said();
        assert!(
            said.contains("is below every step; the quietest step was used"),
            "the quietest step went unlogged: {said}"
        );
        assert!(!said.contains("capped at max_volume_db"), "{said}");
    })
    .await
    .expect("test timed out");
}

/// A slider dragged to the top climbs rather than jumps: the volume reaches three quarters of its
/// range at once and the top a tenth of the range a second after, one raw step at a time.
#[tokio::test]
async fn a_slider_at_the_top_climbs_through_the_daemon() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(
            &bridged(broker.port, "commands = true\n"),
            &[("RUST_LOG", "debug")],
        )
        .await;
        let watcher = watch_settled(&broker).await;
        // The clock starts before the publish, so a late poll under load can only lengthen
        // what is measured: the climb from three quarters takes 2.52 s at best.
        let started = tokio::time::Instant::now();
        watcher.publish(&device_topic("volume/set"), "0").await;
        until_volume(&mut client, |db| db == 0.0).await;
        let took = started.elapsed();
        assert!(
            took >= Duration::from_millis(2400),
            "the climb took {took:?}"
        );
        let writes = child.said().matches("fake -> 10 ").count();
        assert!(writes >= 30, "{writes} volume writes");
    })
    .await
    .expect("test timed out");
}

/// A control a profile puts on the passphrase function or in the firmware-update family does not
/// load, so the daemon stops at start naming the profile, the key and the function: nothing of
/// either function is ever announced to the broker or written for a command on its topic.
#[tokio::test]
async fn a_control_on_a_restricted_function_stops_the_daemon_naming_it() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        for (id, key, function) in [("[250]", "danger", "250"), ("[210, 1]", "flash", "210.1")] {
            let profile = format!(
                "schema = 1\nname = \"gungnir2-v2\"\n\
                 [[control]]\nid = {id}\nkey = \"{key}\"\nlabel = \"Mine\"\nkind = \"select\"\n"
            );
            let (mut child, _socket) =
                spawn_daemon_isolated_with_profiles(CONFIG, &[("gungnir2-v2", &profile)], &[]);
            let status = child.wait_for_exit().await;
            assert!(!status.success(), "{function}: the daemon started");
            let said = child.said();
            assert!(
                said.contains(&format!(
                    "profile gungnir2-v2, control {key}: function {function} cannot be a control"
                )),
                "{function}: {said}"
            );
        }
    })
    .await
    .expect("test timed out");
}

/// A burst of commands on one topic, as a slider dragged in Home Assistant sends, coalesces:
/// one write in flight at a time, the latest payload the one sent next, and the device ends
/// where the burst ended.
#[tokio::test]
async fn a_burst_on_one_topic_coalesces_to_the_latest() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(
            &bridged(broker.port, "commands = true\n"),
            &[("RUST_LOG", "debug")],
        )
        .await;
        let watcher = watch_settled(&broker).await;
        let burst: Vec<String> = (0..30).map(|n| format!("-{}", 60 - n)).collect();
        for value in &burst {
            watcher.publish(&device_topic("volume/set"), value).await;
        }
        until_volume(&mut client, |db| (db + 31.0).abs() < 0.5).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let sent = child.said().matches("fake -> 10 ").count();
        assert!(
            (1..burst.len()).contains(&sent),
            "{sent} writes for a burst of {}",
            burst.len()
        );
    })
    .await
    .expect("test timed out");
}

/// What a case wants the bridge's state to be.
type Wanted = Box<dyn Fn(&MqttStatus) -> bool>;

/// The `version` reply says where the bridge stands: off with no section, connecting — and,
/// between attempts, how long until the next — while no broker answers, and connected,
/// read-only or taking commands.
#[tokio::test]
async fn the_version_reply_says_where_the_bridge_stands() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let away = broker::free_port();
        let cases: [(String, Wanted); 4] = [
            (CONFIG.to_string(), Box::new(|s| *s == MqttStatus::Off)),
            (
                bridged(away, ""),
                Box::new(move |s| {
                    matches!(s, MqttStatus::Connecting { host, port, retry_in_secs: Some(1..=2) }
                        if host == "127.0.0.1" && *port == away)
                }),
            ),
            (
                bridged(broker.port, ""),
                Box::new(|s| {
                    matches!(
                        s,
                        MqttStatus::Connected {
                            commands: false,
                            ..
                        }
                    )
                }),
            ),
            (
                bridged(broker.port, "commands = true\n"),
                Box::new(|s| matches!(s, MqttStatus::Connected { commands: true, .. })),
            ),
        ];
        for (config, wanted) in cases {
            let (_child, mut client) = daemon_on(&config, &[]).await;
            let deadline = tokio::time::Instant::now() + STEP;
            loop {
                let answer = client
                    .request(Request::Version)
                    .await
                    .expect("a version reply");
                let Response::Version {
                    mqtt: Some(mqtt), ..
                } = &answer
                else {
                    panic!("no bridge in {answer:?}");
                };
                if wanted(mqtt) {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the bridge stayed at {mqtt:?} for {config}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    })
    .await
    .expect("test timed out");
}

/// Send `signal` to the daemon, the way a service manager or `kill` does.
fn signal(child: &ChildGuard, signal: &str) {
    let sent = std::process::Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(child.pid().to_string())
        .status()
        .expect("run kill");
    assert!(sent.success(), "kill -{signal} failed: {sent:?}");
}

/// A daemon killed outright says nothing, and the broker publishes the bridge's last will:
/// `offline`, retained, on the bridge's availability topic.
#[tokio::test]
async fn a_killed_daemon_leaves_the_last_will() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, _client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for("svolder/bridge/availability", STEP, |s| {
                s.payload == "online"
            })
            .await;
        signal(&child, "KILL");
        watcher
            .wait_for("svolder/bridge/availability", STEP, |s| {
                s.payload == "offline"
            })
            .await;
        let mut later = Watcher::connect(broker.port, &["svolder/bridge/availability"]).await;
        let kept = later
            .wait_for("svolder/bridge/availability", STEP, |s| s.retained)
            .await;
        assert_eq!(kept.payload, "offline");
    })
    .await
    .expect("test timed out");
}

/// A stop — a `stop` request, or SIGTERM from a service manager — publishes `offline` for the
/// device and for the bridge before the daemon disconnects and exits.
#[tokio::test]
async fn a_stop_says_offline_for_every_device_and_the_bridge() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        for by_signal in [false, true] {
            let broker = TestBroker::start();
            let (mut child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
            let mut watcher = watch_settled(&broker).await;
            watcher
                .wait_for(&device_topic("availability"), STEP, |s| {
                    s.payload == "online"
                })
                .await;
            match by_signal {
                true => signal(&child, "TERM"),
                false => {
                    let answer = client.request(Request::Stop).await.expect("a stop reply");
                    assert!(matches!(answer, Response::Stopping), "got {answer:?}");
                }
            }
            watcher
                .wait_for_each(
                    &[
                        (&device_topic("availability"), "offline"),
                        ("svolder/bridge/availability", "offline"),
                    ],
                    STEP,
                )
                .await;
            let status = child.wait_for_exit().await;
            assert!(status.success(), "the daemon exited with {status:?}");
            assert!(
                child.said().contains("mqtt: disconnected from 127.0.0.1:"),
                "{}",
                child.said()
            );
        }
    })
    .await
    .expect("test timed out");
}

/// SIGHUP with a changed `[mqtt]` section restarts the bridge on the new one: read-only to
/// taking commands, the sensors give way to live controls — the old configs cleared, the new
/// ones announced — and a command then moves the device.
#[tokio::test]
async fn sighup_with_a_changed_section_restarts_the_bridge() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for(&config_topic("sensor", "volume"), STEP, |s| s.retained)
            .await;

        std::fs::write(
            child.config_path(),
            bridged(broker.port, "commands = true\n"),
        )
        .expect("turn commands on");
        signal(&child, "HUP");
        watcher
            .wait_for_each(
                &[
                    ("svolder/bridge/availability", "offline"),
                    ("svolder/bridge/availability", "online"),
                    (&config_topic("sensor", "volume"), ""),
                ],
                STEP,
            )
            .await;
        // What a fresh subscription is given now: the live control, and no sensor beside it.
        let mut later = watch(&broker).await;
        let given: Vec<Seen> = later
            .collect(Duration::from_secs(1))
            .await
            .into_iter()
            .filter(|s| s.topic.ends_with("/volume/config"))
            .collect();
        assert_eq!(
            given.iter().map(|s| s.topic.as_str()).collect::<Vec<_>>(),
            [config_topic("number", "volume")],
            "{given:#?}"
        );
        later.publish(&device_topic("volume/set"), "-20").await;
        until_volume(&mut client, |db| (db + 20.0).abs() < 0.5).await;
        let answer = client
            .request(Request::Version)
            .await
            .expect("a version reply");
        assert!(
            matches!(
                &answer,
                Response::Version {
                    mqtt: Some(MqttStatus::Connected { commands: true, .. }),
                    ..
                }
            ),
            "got {answer:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// SIGHUP with the section as it was keeps the bridge's connection — nothing goes offline —
/// and hands it the device set the file now names.
#[tokio::test]
async fn sighup_with_the_same_section_keeps_the_connection() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, _client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for("svolder/bridge/availability", STEP, |s| s.retained)
            .await;

        let second = "[[device]]\nname = \"second\"\nudid = \"0908070605040302\"\n";
        let config = bridged(broker.port, "").replacen(CONFIG, &format!("{CONFIG}{second}"), 1);
        std::fs::write(child.config_path(), config).expect("add a second device");
        signal(&child, "HUP");
        // Everything heard from the hangup on, until the second device is announced and for a
        // moment after.
        let mut heard: Vec<Seen> = Vec::new();
        let deadline = tokio::time::Instant::now() + STEP;
        while !heard
            .iter()
            .any(|s| s.topic == "svolder/0908070605040302/availability")
        {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the second device was never announced: {heard:#?}"
            );
            heard.extend(watcher.collect(Duration::from_millis(100)).await);
        }
        heard.extend(watcher.collect(Duration::from_millis(500)).await);
        assert!(
            !heard
                .iter()
                .any(|s| s.topic == "svolder/bridge/availability"),
            "the bridge reconnected: {heard:#?}"
        );
    })
    .await
    .expect("test timed out");
}

/// SIGHUP with the section taken out stops the bridge: `offline` for the device and the
/// bridge, and `version` says it is off.
#[tokio::test]
async fn sighup_without_the_section_stops_the_bridge() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for(&device_topic("availability"), STEP, |s| {
                s.payload == "online"
            })
            .await;

        std::fs::write(child.config_path(), CONFIG).expect("take the section out");
        signal(&child, "HUP");
        watcher
            .wait_for_each(
                &[
                    (&device_topic("availability"), "offline"),
                    ("svolder/bridge/availability", "offline"),
                ],
                STEP,
            )
            .await;
        let deadline = tokio::time::Instant::now() + STEP;
        loop {
            let answer = client
                .request(Request::Version)
                .await
                .expect("a version reply");
            if matches!(
                &answer,
                Response::Version {
                    mqtt: Some(MqttStatus::Off),
                    ..
                }
            ) {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "got {answer:?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("test timed out");
}

/// A command someone published retained is never acted on: the broker hands it to the
/// bridge's command subscription on every connect, the first and each one after a drop, and
/// the device stays where it is each time. It is said once, and the broker keeps its message.
#[tokio::test]
async fn a_retained_command_is_never_acted_on() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let watcher = watch(&broker).await;
        watcher.retain(&device_topic("volume/set"), "-5").await;
        // The watcher's own copy coming back is the broker holding it.
        let mut own = Watcher::connect(broker.port, &[&device_topic("volume/set")]).await;
        own.wait_for(&device_topic("volume/set"), STEP, |s| s.retained)
            .await;

        let id = format!("svolder-retained-{}", std::process::id());
        let config = bridged(
            broker.port,
            &format!("commands = true\nclient_id = \"{id}\"\n"),
        );
        let (child, mut client) = daemon_on(&config, &[]).await;
        let before = reading(&mut client, "volume").await.raw;
        let settled = watch_settled(&broker).await;
        drop(settled);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            reading(&mut client, "volume").await.raw,
            before,
            "the first connect"
        );

        // Take the bridge's connection away: a client with its id replaces it on the broker,
        // and the bridge connects again after its wait.
        let kicker = Watcher::connect_with(broker.port, &["kick/#"], None, &id).await;
        drop(kicker);
        let deadline = tokio::time::Instant::now() + STEP;
        while child.said().matches("mqtt: connected to").count() < 2 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the bridge never connected again: {}",
                child.said()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            reading(&mut client, "volume").await.raw,
            before,
            "the reconnect"
        );

        let said = child.said();
        assert_eq!(
            said.matches("ignored a retained command on svolder/0102030405060708/volume/set")
                .count(),
            1,
            "{said}"
        );
        let mut later = Watcher::connect(broker.port, &[&device_topic("volume/set")]).await;
        let kept = later
            .wait_for(&device_topic("volume/set"), STEP, |s| s.retained)
            .await;
        assert_eq!(kept.payload, "-5", "the broker's message was touched");
    })
    .await
    .expect("test timed out");
}

/// A device renamed in the file is the same device to the broker: the bridge follows the actor
/// the new name has for the same unit id, so the device comes back online under its new name
/// and a change made on it then publishes.
#[tokio::test]
async fn a_device_renamed_in_the_file_goes_on_publishing() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for(&device_topic("availability"), STEP, |s| s.payload == "online")
            .await;

        let renamed = bridged(broker.port, "").replacen("name = \"fake\"", "name = \"dac\"", 1);
        std::fs::write(child.config_path(), renamed).expect("rename the device");
        signal(&child, "HUP");
        watcher
            .wait_for(&config_topic("sensor", "volume"), STEP, |s| {
                !s.payload.is_empty() && json(s)["device"]["name"] == "dac"
            })
            .await;
        let deadline = tokio::time::Instant::now() + STEP;
        loop {
            let answer = client.request(Request::Devices).await.expect("a devices reply");
            if matches!(&answer, Response::Devices { devices } if devices.iter().any(|d| d.name == "dac" && d.connected))
            {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "got {answer:?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let mut later = Watcher::connect(broker.port, &[&device_topic("availability")]).await;
        later
            .wait_for(&device_topic("availability"), STEP, |s| s.payload == "online")
            .await;

        let set = client
            .request(Request::Set {
                device: None,
                key: "mute".into(),
                value: "on".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .expect("a set reply");
        assert!(matches!(set, Response::Value(_)), "got {set:?}");
        watcher
            .wait_for(&device_topic("state"), STEP, |s| json(s)["mute"] == "ON")
            .await;
    })
    .await
    .expect("test timed out");
}

/// A reload that changes the section and takes the device out in the same edit clears the
/// device's topics as a reload that only took it out does: the bridge that stops forgets it
/// first, so no ghost of it stays in Home Assistant.
#[tokio::test]
async fn a_device_taken_out_with_the_section_changed_is_cleared() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for(&config_topic("sensor", "volume"), STEP, |s| s.retained)
            .await;

        let without = format!(
            "schema = 1\n[mqtt]\nenabled = true\nhost = \"127.0.0.1\"\nport = {}\ncommands = true\n",
            broker.port
        );
        std::fs::write(child.config_path(), without).expect("take the device out");
        let reloaded = client.request(Request::Reload).await.expect("a reload");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices.is_empty()),
            "got {reloaded:?}"
        );
        watcher
            .wait_for_each(
                &[
                    (&config_topic("sensor", "volume"), ""),
                    (&device_topic("state"), ""),
                    (&device_topic("availability"), ""),
                ],
                STEP,
            )
            .await;
        let mut later = watch(&broker).await;
        let kept: Vec<Seen> = later
            .collect(Duration::from_millis(500))
            .await
            .into_iter()
            .filter(|s| s.topic.contains(UNIT))
            .collect();
        assert!(kept.is_empty(), "left on the broker: {kept:#?}");
    })
    .await
    .expect("test timed out");
}

/// While a reload hands over from one section's bridge to the next one's, `version` says the
/// bridge is connecting, never that it is off.
#[tokio::test]
async fn a_bridge_being_restarted_is_never_reported_off() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(&bridged(broker.port, ""), &[]).await;
        let mut watcher = watch_settled(&broker).await;
        watcher
            .wait_for("svolder/bridge/availability", STEP, |s| s.retained)
            .await;

        std::fs::write(
            child.config_path(),
            bridged(broker.port, "commands = true\n"),
        )
        .expect("turn commands on");
        signal(&child, "HUP");
        let deadline = tokio::time::Instant::now() + STEP;
        let mut seen = Vec::new();
        loop {
            let answer = client
                .request(Request::Version)
                .await
                .expect("a version reply");
            let Response::Version {
                mqtt: Some(mqtt), ..
            } = answer
            else {
                panic!("no bridge in {answer:?}");
            };
            if matches!(mqtt, MqttStatus::Connected { commands: true, .. }) {
                break;
            }
            seen.push(mqtt);
            assert!(tokio::time::Instant::now() < deadline, "{seen:?}");
        }
        assert!(!seen.contains(&MqttStatus::Off), "reported off: {seen:?}");
    })
    .await
    .expect("test timed out");
}

/// A device taken out of the file before this run ever saw it — switched off, say — has the
/// discovery configs an earlier run left under it cleared too, so no ghost of it stays in Home
/// Assistant.
#[tokio::test]
async fn a_device_taken_out_before_it_was_seen_leaves_no_configs() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        // Its unit id sorts before the fake device's, so the bridge subscribes to its discovery
        // topics first.
        let away = "0000000000000001";
        let earlier = format!("homeassistant/sensor/svolder_{away}/volume/config");
        // A config left under the device that is there, which the bridge clears once it has it.
        let marker = format!("homeassistant/sensor/svolder_{UNIT}/left_over/config");
        let mut watcher = watch(&broker).await;
        watcher.retain(&earlier, "{\"name\":\"Volume\"}").await;
        watcher.retain(&marker, "{\"name\":\"Left over\"}").await;
        watcher
            .wait_for(&earlier, STEP, |s| !s.payload.is_empty())
            .await;

        // The fake transport never advertises the second unit id, so its device is configured
        // and never seen.
        let second = format!("[[device]]\nname = \"away\"\nudid = \"{away}\"\n");
        let config = bridged(broker.port, "").replacen(CONFIG, &format!("{CONFIG}{second}"), 1);
        let (child, mut client) = daemon_on(&config, &[]).await;
        watch_settled(&broker).await;
        // Nothing observable says the bridge holds the away device's leftover, so this rests on
        // two assumptions: rumqttd delivers retained messages in the order of the subscriptions,
        // and the bridge subscribes in unit order (the away unit id sorts first). Then the
        // bridge has the away device's once it has cleared the marker.
        watcher
            .wait_for_each(
                &[
                    (&format!("svolder/{away}/availability"), "offline"),
                    (&marker, ""),
                ],
                STEP,
            )
            .await;

        std::fs::write(child.config_path(), bridged(broker.port, "")).expect("take it out");
        let reloaded = client.request(Request::Reload).await.expect("a reload");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices == &["fake"]),
            "got {reloaded:?}"
        );
        watcher
            .wait_for_each(
                &[
                    (&earlier, ""),
                    (&format!("svolder/{away}/availability"), ""),
                ],
                STEP,
            )
            .await;
        let mut later = Watcher::connect(broker.port, &["homeassistant/#", "svolder/#"]).await;
        let kept: Vec<Seen> = later
            .collect(Duration::from_millis(500))
            .await
            .into_iter()
            .filter(|s| s.topic.contains(away))
            .collect();
        assert!(kept.is_empty(), "left on the broker: {kept:#?}");
    })
    .await
    .expect("test timed out");
}

/// The state once the device has reported its tone control: the preset select, the loudness
/// and every band's three parameters are in it.
fn tone_reported(state: &Json) -> bool {
    state.get("eq_preset").is_some()
        && state.get("eq_loudness").is_some()
        && (1..=5).all(|band| {
            ["gain", "frequency", "q"]
                .iter()
                .all(|param| state.get(format!("eq_band_{band}_{param}")).is_some())
        })
}

/// The next state `wanted` accepts.
async fn state_where(watcher: &mut Watcher, wanted: impl Fn(&Json) -> bool) -> Json {
    json(
        &watcher
            .wait_for(&device_topic("state"), STEP, |s| wanted(&json(s)))
            .await,
    )
}

/// A tone control's entities are announced as `select`/`number` with commands and `sensor` read
/// only, and the state carries the preset, the loudness and every band in units.
#[tokio::test]
async fn the_tone_control_is_announced_and_reported() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        for commands in [true, false] {
            let broker = TestBroker::start();
            let extra = if commands { "commands = true\n" } else { "" };
            let (_child, _client) = daemon_on(&bridged(broker.port, extra), &[]).await;
            let mut watcher = watch_after(&broker, tone_reported).await;
            let state = json(
                &watcher
                    .wait_for(&device_topic("state"), STEP, |s| tone_reported(&json(s)))
                    .await,
            );
            assert!(state["eq_preset"].is_string(), "{state}");
            assert!(state["eq_band_3_gain"].is_f64() || state["eq_band_3_gain"].is_i64());

            let (preset, number) = match commands {
                true => ("select", "number"),
                false => ("sensor", "sensor"),
            };
            let given: std::collections::BTreeMap<String, Seen> = watcher
                .collect(Duration::from_secs(1))
                .await
                .into_iter()
                .map(|seen| (seen.topic.clone(), seen))
                .collect();
            let at = |component: &str, key: &str| {
                json(
                    given
                        .get(&config_topic(component, key))
                        .unwrap_or_else(|| panic!("no {component} {key} in {given:#?}")),
                )
            };
            let select = at(preset, "eq_preset");
            assert_eq!(select["name"], "EQ preset");
            if commands {
                let options = select["options"].as_array().expect("options");
                assert_eq!(options.last().expect("an option"), "custom");
                assert!(options.iter().any(|o| o == "Warm"), "{options:?}");
            }
            let gain = at(number, "eq_band_2_gain");
            assert_eq!(gain["unit_of_measurement"], "dB");
            assert_eq!(gain.get("command_topic").is_some(), commands, "{gain}");
            let loudness = at(number, "eq_loudness");
            assert_eq!(loudness["name"], "Loudness strength");
        }
    })
    .await
    .expect("test timed out");
}

/// A band parameter, the loudness strength and a preset move the device through the actor, and
/// the select follows the curve: the preset recalled while it matches, `custom` once a band
/// moves off it. A `custom` command and a name no preset has are dropped, saying so.
#[tokio::test]
async fn a_tone_command_moves_the_device_and_the_select_follows() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, _client) = daemon_on(
            &bridged(broker.port, "commands = true\n"),
            &[("RUST_LOG", "debug")],
        )
        .await;
        let mut watcher = watch_after(&broker, tone_reported).await;
        watcher
            .publish(&device_topic("eq_loudness/set"), "42")
            .await;
        state_where(&mut watcher, |s| s["eq_loudness"] == 42).await;

        watcher
            .publish(&device_topic("eq_preset/set"), " warm ")
            .await;
        state_where(&mut watcher, |s| s["eq_preset"] == "Warm").await;

        watcher
            .publish(&device_topic("eq_band_2_gain/set"), "-3.5")
            .await;
        let moved = state_where(&mut watcher, |s| {
            s["eq_band_2_gain"]
                .as_f64()
                .is_some_and(|g| (g + 3.5).abs() < 0.3)
        })
        .await;
        assert_eq!(moved["eq_preset"], "custom", "{moved}");

        watcher
            .publish(&device_topic("eq_band_2_frequency/set"), "250")
            .await;
        state_where(&mut watcher, |s| {
            s["eq_band_2_frequency"]
                .as_f64()
                .is_some_and(|f| (f - 250.0).abs() < 5.0)
        })
        .await;
        watcher.publish(&device_topic("eq_band_2_q/set"), "2").await;
        state_where(&mut watcher, |s| {
            s["eq_band_2_q"]
                .as_f64()
                .is_some_and(|q| (q - 2.0).abs() < 0.1)
        })
        .await;

        // A band changed while the tone control is in flat switches it to EQ.
        watcher.publish(&device_topic("eq_mode/set"), "flat").await;
        state_where(&mut watcher, |s| s["eq_mode"] == "flat").await;
        watcher
            .publish(&device_topic("eq_band_2_gain/set"), "-1")
            .await;
        state_where(&mut watcher, |s| s["eq_mode"] == "eq").await;

        for (topic, payload) in [
            ("eq_preset/set", "custom"),
            ("eq_preset/set", "No such preset"),
            ("eq_band_2_gain/set", "loud"),
            ("eq_loudness/set", "128"),
            ("eq_band_6_gain/set", "1"),
        ] {
            watcher.publish(&device_topic(topic), payload).await;
        }
        let deadline = tokio::time::Instant::now() + STEP;
        while child.said().matches("dropped").count() < 5 {
            assert!(tokio::time::Instant::now() < deadline, "{}", child.said());
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let said = child.said();
        assert!(
            said.contains("\"custom\" is not one of the presets"),
            "{said}"
        );
        assert!(said.contains("\"No such preset\" is not one of"), "{said}");
    })
    .await
    .expect("test timed out");
}

/// A preset the configuration gains is offered after a reload, and one it loses is not: the
/// discovery config is announced again with the new options.
#[tokio::test]
async fn a_reload_republishes_the_preset_options() {
    use svolder_ipc::Config;
    use svolder_profiles::EqMode;
    use svolder_profiles::tone::preset::{Preset, PresetBand};

    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, _client) = daemon_on(&bridged(broker.port, "commands = true\n"), &[]).await;
        let mut watcher = watch_after(&broker, tone_reported).await;
        let topic = config_topic("select", "eq_preset");
        let options = |seen: &Seen| -> Vec<String> {
            json(seen)["options"]
                .as_array()
                .expect("options")
                .iter()
                .map(|o| o.as_str().expect("a name").to_string())
                .collect()
        };
        let first = watcher.wait_for(&topic, STEP, |s| s.retained).await;
        assert!(!options(&first).contains(&"Evening".to_string()));

        let preset = Preset {
            name: "Evening".into(),
            source: None,
            bands: 5,
            mode: EqMode::Eq,
            loudness: 0,
            band: vec![
                PresetBand {
                    f: 1000.0,
                    g: 1.0,
                    q: 1.0
                };
                5
            ],
        };
        let path = child.config_path();
        Config::add_preset(&path, &preset).expect("a preset is written");
        signal(&child, "HUP");
        let added = watcher
            .wait_for(&topic, STEP, |s| {
                options(s).contains(&"Evening".to_string())
            })
            .await;
        let names = options(&added);
        assert_eq!(names.last().map(String::as_str), Some("custom"));
        assert_eq!(names[names.len() - 2], "Evening");

        watcher
            .publish(&device_topic("eq_preset/set"), "evening")
            .await;
        watcher
            .wait_for(&device_topic("state"), STEP, |s| {
                json(s)["eq_preset"] == "Evening"
            })
            .await;

        Config::remove_preset(&path, "Evening").expect("the preset is removed");
        signal(&child, "HUP");
        watcher
            .wait_for(&topic, STEP, |s| {
                !options(s).contains(&"Evening".to_string())
            })
            .await;
        watcher
            .wait_for(&device_topic("state"), STEP, |s| {
                json(s)["eq_preset"] != "Evening"
            })
            .await;
    })
    .await
    .expect("test timed out");
}

/// A burst of slider moves on one band's gain is one write in flight and the latest value after
/// it, as a burst on the volume is.
#[tokio::test]
async fn a_burst_on_one_band_coalesces_to_the_latest() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, _client) = daemon_on(
            &bridged(broker.port, "commands = true\n"),
            &[("RUST_LOG", "debug")],
        )
        .await;
        let mut watcher = watch_after(&broker, tone_reported).await;
        let burst: Vec<String> = (0..30)
            .map(|n| format!("{}", n as f64 / 3.0 - 10.0))
            .collect();
        for value in &burst {
            watcher
                .publish(&device_topic("eq_band_4_gain/set"), value)
                .await;
        }
        let last: f64 = burst.last().expect("a value").parse().expect("a number");
        watcher
            .wait_for(&device_topic("state"), STEP, |s| {
                json(s)["eq_band_4_gain"]
                    .as_f64()
                    .is_some_and(|g| (g - last).abs() < 0.3)
            })
            .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let registry = svolder_profiles::Registry::builtin().expect("the built-in profiles");
        let eq = registry
            .get("gungnir2-v2")
            .and_then(|p| p.eq.clone())
            .expect("a tone control");
        let gain = eq
            .band_id(3, svolder_profiles::EqParam::Gain)
            .expect("band 4");
        let sent = child.said().matches(&format!("fake -> {gain} ")).count();
        assert!(
            (1..burst.len()).contains(&sent),
            "{sent} writes for {} values",
            burst.len()
        );
    })
    .await
    .expect("test timed out");
}

/// A Loki Max's preset select is announced with its slots, built-in settings and `custom`; a
/// pick of a built-in setting moves the bands and the select follows it; a pick of a slot the
/// device never saved recalls what the fake holds there, flat, and the select follows that; a
/// pick of `custom`, a save or a slot the device does not have, and a command on the topic of
/// `recall` or `save`, is dropped, saying so, and no recall or save frame reaches the device: the
/// bands stay where they were and slot 1 is still the one never saved.
#[tokio::test]
async fn a_loki_max_preset_select_over_the_broker_recalls_and_applies() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let broker = TestBroker::start();
        let (child, mut client) = daemon_on(
            &bridged(broker.port, "commands = true\n"),
            &[("SVOLDER_FAKE_PROFILE", "lokimax")],
        )
        .await;
        let mut watcher = watch_after(&broker, |s| s["eq_preset"] == "Flat").await;
        let select = json(
            &watcher
                .wait_for(&config_topic("select", "eq_preset"), STEP, |_| true)
                .await,
        );
        assert_eq!(select["name"], "Preset");
        let options: Vec<&str> = select["options"]
            .as_array()
            .expect("options")
            .iter()
            .map(|o| o.as_str().expect("a name"))
            .collect();
        assert_eq!(options[..4], ["Preset 1", "Preset 2", "Preset 3", "Flat"]);
        assert_eq!(options.last(), Some(&"custom"));

        watcher
            .publish(&device_topic("eq_preset/set"), "Bass Boost")
            .await;
        let boosted = state_where(&mut watcher, |s| s["eq_preset"] == "Bass Boost").await;
        assert!(
            boosted["band1"].as_f64().is_some_and(|g| g > 10.0),
            "{boosted}"
        );

        watcher
            .publish(&device_topic("eq_preset/set"), "Preset 2")
            .await;
        state_where(&mut watcher, |s| s["eq_preset"] == "Flat").await;
        watcher
            .publish(&device_topic("eq_preset/set"), "Bass Boost")
            .await;
        state_where(&mut watcher, |s| s["eq_preset"] == "Bass Boost").await;

        for payload in ["custom", "Preset 4", "save", "Save Preset 1"] {
            watcher
                .publish(&device_topic("eq_preset/set"), payload)
                .await;
        }
        watcher.publish(&device_topic("save/set"), "1").await;
        watcher.publish(&device_topic("recall/set"), "1").await;
        let deadline = tokio::time::Instant::now() + STEP;
        while child.said().matches("dropped").count() < 6 {
            assert!(tokio::time::Instant::now() < deadline, "{}", child.said());
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        // A recall frame would have set the bands from a slot never saved, which is flat.
        assert_eq!(reading(&mut client, "band1").await.raw, 28);
        // A save frame would have written these bands into slot 1; a recall over the socket
        // brings back flat, which is what an untouched slot holds.
        let recalled = client
            .request(Request::Set {
                device: None,
                key: "recall".into(),
                value: "1".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .expect("a reply");
        assert!(matches!(recalled, Response::Ack), "{recalled:?}");
        state_where(&mut watcher, |s| s["eq_preset"] == "Flat").await;
        assert_eq!(reading(&mut client, "band1").await.raw, 15);
    })
    .await
    .expect("test timed out");
}
