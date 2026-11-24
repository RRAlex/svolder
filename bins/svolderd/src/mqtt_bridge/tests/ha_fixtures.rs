// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Home Assistant registry fixtures the dashboard strategy's checks read
//! (`scripts/web-checks/ha/fixtures/generated/`), made from the discovery configs a bridge task
//! really publishes: the same task, the same `flush`, the same functions that announce to the
//! broker. Home Assistant's own part (which registry device and entity a config becomes) is
//! applied by [`registry`]; nothing is typed by hand.
//!
//! `SVOLDER_BLESS=1` writes the files; without it a drift fails the test.

use super::*;
use serde_json::{Map, Value as Json, json};

/// The folder of the generated fixtures.
fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/web-checks/ha/fixtures/generated")
}

/// One computer of a fixture: its host name, the devices it serves as (profile, fake unit id,
/// configured name) and its scenes.
struct Host {
    name: &'static str,
    devices: &'static [(&'static str, &'static str, &'static str)],
    scenes: &'static [&'static str],
}

const HOSTS: &[Host] = &[
    Host {
        name: "reframe",
        devices: &[("bifrost3", "0A1B2C3D4E5F6071", "Bifrost 3")],
        scenes: &["Late night", "Desk"],
    },
    // A host name that differs from its id (`living_room`), serving an amp and a two-USB DAC.
    Host {
        name: "Living-Room",
        devices: &[
            ("lyr5", "1B2C3D4E5F6071A0", "Lyr 5"),
            ("gungnir2-v2", "2C3D4E5F6071A0B1", "Gungnir"),
        ],
        scenes: &["Movie"],
    },
];

/// A task like the daemon's for the computer `host` (commands as given, discovery on, the
/// default prefix and base topic), following a computer that serves every stage and the scenes
/// `scenes`, with `devices` tracked; flushed once. Every discovery config it holds, the
/// computer's and the scenes' first, then each device's, as `(topic, body)`.
async fn announced(host: &Host, commands: bool) -> Vec<(String, String)> {
    let settings = Settings {
        host: "127.0.0.1".into(),
        port: 1,
        client_id: "test".into(),
        username: None,
        password: None,
        availability: "svolder/bridge/availability".into(),
    };
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
        me: Computer::new(host.name),
    };
    task.config.commands = commands;
    let mut view = both_computer_view(false, false, Some("a"));
    view.eq.as_mut().unwrap().loudness = loudness_computer_view(false, 40.0).eq.unwrap().loudness;
    let carried = bypass_computer_view(false, -14.8).eq.unwrap();
    view.eq.as_mut().unwrap().bypass = carried.bypass;
    view.eq.as_mut().unwrap().levels = carried.levels;
    let (_view_tx, view_rx) = watch::channel(view);
    task.follow_computer(ComputerLink {
        view: view_rx,
        switch: Arc::new(RecordingSwitch {
            loudnesses: Default::default(),
            bypasses: Default::default(),
            calls: Default::default(),
            crossfeeds: Default::default(),
            speakers: Default::default(),
            answer: Ok(()),
        }),
        slow: None,
        scenes: None,
    });
    let (_scenes_tx, scenes_rx) = watch::channel(scenes_view(host.scenes, None, true));
    task.follow_scenes(ScenesLink {
        view: scenes_rx,
        desk: Arc::new(RecordingDesk {
            ops: Default::default(),
            answer: std::sync::Mutex::new(Response::Unknown),
        }),
    });
    let mut stands = Vec::new();
    for (profile, unit, name) in host.devices {
        let (handle, stand) = stand_device_on(profile, unit, name);
        task.track((*unit).into(), &handle);
        stands.push((handle, stand));
    }
    task.flush();
    let mut all: Vec<(String, String)> = Vec::new();
    all.extend(task.computer.as_ref().unwrap().configs.clone());
    all.extend(task.scenes.as_ref().unwrap().configs.clone());
    for published in task.devices.values() {
        all.extend(published.configs.clone());
    }
    all
}

/// `text` as Home Assistant's `slugify` makes an object id: lower case, every run of characters
/// outside `[a-z0-9]` one `_`, none at either end. The names here are ASCII, which is all this
/// handles.
fn slug(text: &str) -> String {
    assert!(text.is_ascii(), "{text}: only ASCII names are fixtures");
    let mut out = String::new();
    for c in text.to_ascii_lowercase().chars() {
        match c {
            'a'..='z' | '0'..='9' => out.push(c),
            _ if !out.ends_with('_') => out.push('_'),
            _ => {}
        }
    }
    out.trim_matches('_').to_string()
}

/// What Home Assistant's registries hold after it has read `configs` (`(topic, body)` pairs,
/// parents before the devices that are `via_device` of them, as the bridge announces them), in
/// the shapes the frontend gives a dashboard strategy: `{devices, entities, states}`.
///
/// Rules applied, all Home Assistant's own for an MQTT entity that sits in a device:
/// - the device is identified by `device.identifiers` (the integration is `mqtt`), and its
///   `via_device`, an identifier, becomes the registry id of the device that holds it;
/// - the entity has the device's name as its prefix (`has_entity_name`), so its entity id is
///   `<domain>.<slug(device name + " " + entity name)>`, the domain being the topic's component;
///   no `object_id` is sent, so nothing else decides it;
/// - the entity's registry name is its own name, its category the config's `entity_category`
///   (none when absent), and it is not hidden.
fn registry(configs: &[(String, String)]) -> Json {
    let mut devices = Map::new();
    let mut entities = Map::new();
    let mut states = Map::new();
    let mut by_identifier: BTreeMap<String, String> = BTreeMap::new();
    let (mut computers, mut others) = (0, 0);
    for (topic, body) in configs {
        let config: Json = serde_json::from_str(body).unwrap();
        let component = topic.split('/').nth(1).unwrap();
        let block = &config["device"];
        let identifier = block["identifiers"][0].as_str().unwrap().to_string();
        let id = by_identifier
            .entry(identifier.clone())
            .or_insert_with(|| {
                let computer = identifier.starts_with("svolder_computer_");
                let id = if computer {
                    computers += 1;
                    format!("c{computers}")
                } else {
                    others += 1;
                    format!("d{others}")
                };
                let via = block["via_device"]
                    .as_str()
                    .map(|via| by_identifier_of(&devices, via));
                assert!(
                    block["via_device"].is_null() || via.as_ref().is_some_and(Option::is_some),
                    "{topic}: its via_device is announced after it"
                );
                devices.insert(
                    id.clone(),
                    json!({
                        "id": id,
                        "identifiers": [["mqtt", identifier]],
                        "name": block["name"],
                        "name_by_user": null,
                        "via_device_id": via.flatten(),
                        "manufacturer": block["manufacturer"],
                        "model": block["model"],
                        "sw_version": block.get("sw_version").cloned().unwrap_or(Json::Null),
                    }),
                );
                id
            })
            .clone();
        let name = config["name"].as_str().unwrap();
        let device_name = block["name"].as_str().unwrap();
        // Home Assistant drops a device name an entity name repeats (or equals), which would
        // change the entity id: no fixture entity may do that.
        let (lower, device_lower) = (name.to_lowercase(), device_name.to_lowercase());
        assert!(
            !lower.starts_with(&device_lower),
            "{name} repeats its device name {device_name}: Home Assistant would drop the prefix"
        );
        let friendly = format!("{device_name} {name}");
        let entity_id = format!("{component}.{}", slug(&friendly));
        assert!(
            !entities.contains_key(&entity_id),
            "{entity_id} would be given a numeric suffix by Home Assistant"
        );
        entities.insert(
            entity_id.clone(),
            json!({
                "entity_id": entity_id,
                "name": name,
                "device_id": id,
                "platform": "mqtt",
                "unique_id": config["unique_id"],
                "entity_category": config.get("entity_category").cloned().unwrap_or(Json::Null),
                "hidden": false,
            }),
        );
        // What the frontend reads off a state that carries no value yet: the attributes the
        // config gives the entity.
        let mut attributes = Map::new();
        attributes.insert("friendly_name".into(), friendly.into());
        for key in [
            "options",
            "min",
            "max",
            "step",
            "unit_of_measurement",
            "device_class",
            "mode",
        ] {
            if let Some(value) = config.get(key) {
                attributes.insert(key.into(), value.clone());
            }
        }
        states.insert(
            entity_id.clone(),
            json!({"entity_id": entity_id, "state": "unknown", "attributes": attributes}),
        );
    }
    let sorted = |map: Map<String, Json>| -> Json {
        Json::Object(
            map.into_iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        )
    };
    json!({
        "devices": sorted(devices),
        "entities": sorted(entities),
        "states": sorted(states),
    })
}

/// The registry id of the device that holds `identifier`, if one does.
fn by_identifier_of(devices: &Map<String, Json>, identifier: &str) -> Option<String> {
    devices.iter().find_map(|(id, device)| {
        (device["identifiers"][0][1].as_str() == Some(identifier)).then(|| id.clone())
    })
}

/// The fixture file of `commands`: the licence keys first, then `hass`, two-space JSON.
async fn fixture(commands: bool) -> String {
    let mut configs = Vec::new();
    for host in HOSTS {
        configs.extend(announced(host, commands).await);
    }
    let hass = serde_json::to_string_pretty(&registry(&configs)).unwrap();
    let hass = hass.replace('\n', "\n  ");
    format!(
        "{{\n  \"licence_1\": \"SPDX-FileCopyrightText: 2026 the svolder authors\",\n  \
         \"licence_2\": \"SPDX-License-Identifier: GPL-3.0-or-later\",\n  \"hass\": {hass}\n}}\n"
    )
}

#[tokio::test]
async fn the_registry_fixtures_are_what_the_daemons_discovery_makes() {
    for (file, commands) in [("commands-on.json", true), ("commands-off.json", false)] {
        let text = fixture(commands).await;
        let path = fixtures().join(file);
        if std::env::var("SVOLDER_BLESS").as_deref() == Ok("1") {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &text).unwrap();
            continue;
        }
        let held = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; SVOLDER_BLESS=1 writes it", path.display()));
        if held != text {
            let (at, (was, now)) = held
                .lines()
                .zip(text.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .unwrap_or((held.lines().count().min(text.lines().count()), ("", "")));
            panic!(
                "{file} drifted from what the daemon announces, first at line {}:\n  held: {was}\n   now: {now}\n\
                 SVOLDER_BLESS=1 rewrites it",
                at + 1
            );
        }
    }
}

#[tokio::test]
async fn the_fixtures_hold_the_shapes_the_strategy_finds_devices_by() {
    let on: Json = serde_json::from_str(&fixture(true).await).unwrap();
    let hass = &on["hass"];
    let identifiers: Vec<&str> = hass["devices"]
        .as_object()
        .unwrap()
        .values()
        .map(|d| d["identifiers"][0][1].as_str().unwrap())
        .collect();
    assert_eq!(
        identifiers,
        [
            "svolder_computer_reframe",
            "svolder_computer_living_room",
            "svolder_0A1B2C3D4E5F6071",
            "svolder_1B2C3D4E5F6071A0",
            "svolder_2C3D4E5F6071A0B1"
        ]
    );
    let entities = hass["entities"].as_object().unwrap();
    for id in [
        "number.bifrost_3_volume",
        "select.bifrost_3_tone_control_mode",
        "switch.reframe_eq",
        "select.reframe_scene",
        "text.reframe_scene_save",
        "switch.living_room_eq",
        "select.living_room_scene",
    ] {
        assert!(entities.contains_key(id), "{id}");
    }
    // The device is under its computer.
    let devices = &hass["devices"];
    assert_eq!(devices["d1"]["via_device_id"], "c1");
    assert_eq!(devices["d2"]["via_device_id"], "c2");
    assert_eq!(devices["d3"]["via_device_id"], "c2");
    // Without commands the same keys are sensors: the entity domain moves with the setting.
    let off: Json = serde_json::from_str(&fixture(false).await).unwrap();
    let off = off["hass"]["entities"].as_object().unwrap();
    assert!(
        off.contains_key("sensor.bifrost_3_volume") && !off.contains_key("number.bifrost_3_volume")
    );
    assert!(
        off.contains_key("binary_sensor.reframe_eq") && off.contains_key("sensor.reframe_scene")
    );
    assert!(!off.contains_key("text.reframe_scene_save"));
}
