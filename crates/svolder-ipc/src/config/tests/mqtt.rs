// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[mqtt]` block: its defaults, keys, bounds and its writer.

use super::*;

#[test]
fn mqtt_defaults_off_at_1883_under_svolder_and_homeassistant() {
    let c = Config::parse("").unwrap();
    assert!(!c.mqtt.enabled);
    assert_eq!(c.mqtt.host, None);
    assert_eq!(c.mqtt.port, 1883);
    assert_eq!(c.mqtt.username, None);
    assert_eq!(c.mqtt.password, None);
    assert_eq!(c.mqtt.client_id, None);
    assert_eq!(c.mqtt.base_topic, "svolder");
    assert_eq!(c.mqtt.discovery_prefix, "homeassistant");
    assert!(!c.mqtt.commands);
    assert_eq!(c.mqtt.max_volume_db, None);
}

#[test]
fn mqtt_block_parses_every_key() {
    let text = "[mqtt]\n\
                    enabled = true\n\
                    host = \"192.168.1.10\"\n\
                    port = 8883\n\
                    username = \"svolder\"\n\
                    password = \"secret\"\n\
                    client_id = \"svolder-myhost\"\n\
                    base_topic = \"forkbeard\"\n\
                    discovery_prefix = \"ha\"\n\
                    commands = true\n\
                    max_volume_db = -10.0\n";
    let c = Config::parse(text).unwrap();
    assert!(c.mqtt.enabled);
    assert_eq!(c.mqtt.host.as_deref(), Some("192.168.1.10"));
    assert_eq!(c.mqtt.port, 8883);
    assert_eq!(c.mqtt.username.as_deref(), Some("svolder"));
    assert_eq!(c.mqtt.password.as_deref(), Some("secret"));
    assert_eq!(c.mqtt.client_id.as_deref(), Some("svolder-myhost"));
    assert_eq!(c.mqtt.base_topic, "forkbeard");
    assert_eq!(c.mqtt.discovery_prefix, "ha");
    assert!(c.mqtt.commands);
    assert_eq!(c.mqtt.max_volume_db, Some(-10.0));
}

#[test]
fn a_blank_mqtt_host_username_password_and_client_id_are_no_override() {
    let text = "[mqtt]\nhost = \"\"\nusername = \"  \"\npassword = \"\"\nclient_id = \"\"\n";
    let c = Config::parse(text).unwrap();
    assert_eq!(c.mqtt.host, None);
    assert_eq!(c.mqtt.username, None);
    assert_eq!(c.mqtt.password, None);
    assert_eq!(c.mqtt.client_id, None);
}

#[test]
fn the_mqtt_password_is_never_printed_as_itself() {
    let c = Config::parse("[mqtt]\nusername = \"svolder\"\npassword = \"secret\"\n").unwrap();
    let json = serde_json::to_string(&c).unwrap();
    assert!(json.contains(r#""password":"***""#), "{json}");
    let debug = format!("{c:?}");
    assert!(
        !json.contains("secret") && !debug.contains("secret"),
        "{json}\n{debug}"
    );
    let none = serde_json::to_string(&Config::parse("[mqtt]\n").unwrap()).unwrap();
    assert!(none.contains(r#""password":null"#), "{none}");
}

#[test]
fn mqtt_enabled_requires_a_host() {
    let err = Config::parse("[mqtt]\nenabled = true\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("mqtt.host")),
        "got {err:?}"
    );
    // A blank host is the same as none at all.
    let err = Config::parse("[mqtt]\nenabled = true\nhost = \"  \"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("mqtt.host")),
        "got {err:?}"
    );
    // Disabled, no host is required at all.
    assert!(Config::parse("[mqtt]\nenabled = false\n").is_ok());
}

#[test]
fn mqtt_port_out_of_range_is_rejected_naming_the_key() {
    for text in [
        "[mqtt]\nport = 0\n",
        "[mqtt]\nport = 65536\n",
        "[mqtt]\nport = -1\n",
    ] {
        let err = Config::parse(text).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains("mqtt.port")),
            "{text:?} got {err:?}"
        );
    }
    assert!(Config::parse("[mqtt]\nport = 1\n").is_ok());
    assert!(Config::parse("[mqtt]\nport = 65535\n").is_ok());
}

#[test]
fn mqtt_topic_prefixes_hold_no_wildcard_and_no_outer_slash() {
    for (text, key) in [
        ("[mqtt]\nbase_topic = \"\"\n", "mqtt.base_topic"),
        ("[mqtt]\nbase_topic = \"svolder/+\"\n", "mqtt.base_topic"),
        ("[mqtt]\nbase_topic = \"home/#\"\n", "mqtt.base_topic"),
        ("[mqtt]\nbase_topic = \"/svolder\"\n", "mqtt.base_topic"),
        ("[mqtt]\nbase_topic = \"svolder/\"\n", "mqtt.base_topic"),
        (
            "[mqtt]\ndiscovery_prefix = \"ha/#\"\n",
            "mqtt.discovery_prefix",
        ),
        (
            "[mqtt]\ndiscovery_prefix = \"+\"\n",
            "mqtt.discovery_prefix",
        ),
        (
            "[mqtt]\ndiscovery_prefix = \"/homeassistant\"\n",
            "mqtt.discovery_prefix",
        ),
        (
            "[mqtt]\ndiscovery_prefix = \"homeassistant/\"\n",
            "mqtt.discovery_prefix",
        ),
    ] {
        let err = Config::parse(text).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains(key)),
            "{text:?} got {err:?}"
        );
    }
    let c = Config::parse("[mqtt]\nbase_topic = \"home/svolder\"\ndiscovery_prefix = \"\"\n")
        .expect("a nested base topic, and discovery off");
    assert_eq!(c.mqtt.base_topic, "home/svolder");
    assert_eq!(c.mqtt.discovery_prefix, "");
}

#[test]
fn mqtt_max_volume_db_must_be_finite() {
    for text in [
        "[mqtt]\nmax_volume_db = nan\n",
        "[mqtt]\nmax_volume_db = inf\n",
        "[mqtt]\nmax_volume_db = -inf\n",
    ] {
        let err = Config::parse(text).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains("mqtt.max_volume_db")),
            "{text:?} got {err:?}"
        );
    }
    assert_eq!(
        Config::parse("[mqtt]\nmax_volume_db = -10.0\n")
            .unwrap()
            .mqtt
            .max_volume_db,
        Some(-10.0)
    );
}

/// The MQTT writer sets `enabled`, `host`, `port` and `commands`, and a password in the file
/// is neither shown to the edit closure's writes nor changed.
#[test]
fn update_mqtt_writes_only_the_changed_keys_and_never_the_password() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "schema = 1\n[mqtt]\n# the broker\nhost = \"old.example.com\"\n\
         password = \"fake-secret\"\n",
    )
    .unwrap();

    Config::update_mqtt(&path, |mqtt| {
        mqtt.host = Some("broker.example.com".into());
        mqtt.password = Some("other-fake".into());
        mqtt.commands = true;
    })
    .unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# the broker"), "{text}");
    assert!(text.contains("host = \"broker.example.com\""), "{text}");
    assert!(text.contains("commands = true"), "{text}");
    assert!(text.contains("password = \"fake-secret\""));
    assert!(!text.contains("other-fake"));
    assert!(!text.contains("port"), "{text}");
}

/// An edit the parser would refuse (a bridge on with no host) is an error and leaves the file.
#[test]
fn update_mqtt_refuses_what_would_not_parse_and_keeps_the_file() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    assert!(Config::update_mqtt(&path, |mqtt| mqtt.enabled = true).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "schema = 1\n");
}
