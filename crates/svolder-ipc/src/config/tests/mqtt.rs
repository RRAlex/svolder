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

/// The MQTT writer sets `enabled`, `host` and `commands` and leaves every key the edit does not
/// change as it is, comments and the password included.
#[test]
fn update_mqtt_writes_only_the_changed_keys() {
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
        mqtt.commands = true;
    })
    .unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# the broker"), "{text}");
    assert!(text.contains("host = \"broker.example.com\""), "{text}");
    assert!(text.contains("commands = true"), "{text}");
    assert!(text.contains("password = \"fake-secret\""));
    assert!(!text.contains("port"), "{text}");
}

/// The username and the password are written when an edit changes them, and cleared back to the
/// template's commented line when an edit takes them away.
#[test]
fn update_mqtt_writes_and_clears_the_username_and_the_password() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::update_mqtt(&path, |mqtt| {
        mqtt.host = Some("broker.example.com".into());
        mqtt.username = Some("svolder".into());
        mqtt.password = Some("fake-secret".into());
    })
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\nusername = \"svolder\""), "{text}");
    assert!(text.contains("\npassword = \"fake-secret\""), "{text}");
    let c = Config::load_from(&path).unwrap();
    assert_eq!(c.mqtt.password.as_deref(), Some("fake-secret"));

    // A password with a quote and a backslash survives the round trip.
    Config::update_mqtt(&path, |mqtt| mqtt.password = Some("a\"b\\c".into())).unwrap();
    assert_eq!(
        Config::load_from(&path).unwrap().mqtt.password.as_deref(),
        Some("a\"b\\c")
    );

    Config::update_mqtt(&path, |mqtt| mqtt.password = None).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("a\\\"b"), "{text}");
    assert!(text.contains("# password = \"\""), "{text}");
    assert_eq!(Config::load_from(&path).unwrap().mqtt.password, None);
    Config::update_mqtt(&path, |mqtt| mqtt.username = None).unwrap();
    assert_eq!(Config::load_from(&path).unwrap().mqtt.username, None);
}

/// An edit that changes nothing leaves the file byte for byte, a password included.
#[test]
fn update_mqtt_with_no_change_leaves_the_file() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let text = "schema = 1\n[mqtt]\nhost = \"h\"\nusername = \"u\"\npassword = \"p\"\n";
    std::fs::write(&path, text).unwrap();
    Config::update_mqtt(&path, |_| {}).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
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

/// The container variables give the `[mqtt]` keys their defaults, and a key in the file wins.
#[test]
fn the_mqtt_defaults_come_from_the_environment_and_the_file_wins() {
    let env = ListenDefaults {
        mqtt_enabled: Some("1".to_string()),
        mqtt_host: Some("broker.lan".to_string()),
        mqtt_port: Some("1884".to_string()),
        mqtt_username: Some("svolder".to_string()),
        ..ListenDefaults::default()
    };
    let c = Config::parse_with_listen_defaults("", &env).unwrap();
    assert!(c.mqtt.enabled);
    assert_eq!(c.mqtt.host.as_deref(), Some("broker.lan"));
    assert_eq!(c.mqtt.port, 1884);
    assert_eq!(c.mqtt.username.as_deref(), Some("svolder"));
    // A table with other keys still takes them.
    let c = Config::parse_with_listen_defaults("[mqtt]\ncommands = true\n", &env).unwrap();
    assert!(c.mqtt.enabled && c.mqtt.commands);
    assert_eq!(c.mqtt.host.as_deref(), Some("broker.lan"));
    // The file wins, key by key.
    let text = "[mqtt]\nenabled = false\nhost = \"file.lan\"\nport = 1885\nusername = \"me\"\n";
    let c = Config::parse_with_listen_defaults(text, &env).unwrap();
    assert!(!c.mqtt.enabled, "the file's false wins");
    assert_eq!(c.mqtt.host.as_deref(), Some("file.lan"));
    assert_eq!(c.mqtt.port, 1885);
    assert_eq!(c.mqtt.username.as_deref(), Some("me"));
    // One variable at a time, and a blank one is unset (ListenDefaults::from_env drops it).
    let host_only = ListenDefaults {
        mqtt_host: Some("broker.lan".to_string()),
        ..ListenDefaults::default()
    };
    let c = Config::parse_with_listen_defaults("", &host_only).unwrap();
    assert!(!c.mqtt.enabled);
    assert_eq!(c.mqtt.port, 1883);
    assert_eq!(c.mqtt.username, None);
}

/// A bad `SVOLDER_MQTT_ENABLED` or `SVOLDER_MQTT_PORT` is refused by the variable's name, and
/// the password is not among the defaults the environment gives the parse.
#[test]
fn a_bad_mqtt_variable_is_refused_by_name() {
    let bad = ListenDefaults {
        mqtt_enabled: Some("maybe".to_string()),
        ..ListenDefaults::default()
    };
    let err = Config::parse_with_listen_defaults("", &bad)
        .unwrap_err()
        .to_string();
    assert!(err.contains("SVOLDER_MQTT_ENABLED"), "{err}");
    for port in ["abc", "0", "70000"] {
        let bad = ListenDefaults {
            mqtt_port: Some(port.to_string()),
            ..ListenDefaults::default()
        };
        let err = Config::parse_with_listen_defaults("", &bad)
            .unwrap_err()
            .to_string();
        assert!(err.contains("SVOLDER_MQTT_PORT"), "{port}: {err}");
    }
    // An enabled bridge with no host, from the variable, names the variable.
    let no_host = ListenDefaults {
        mqtt_enabled: Some("1".to_string()),
        ..ListenDefaults::default()
    };
    let err = Config::parse_with_listen_defaults("", &no_host)
        .unwrap_err()
        .to_string();
    assert!(err.contains("mqtt.host"), "{err}");
}

/// A password written into a file others can read tightens it to 0600, and says so; a file that
/// is already private, or a write with no password in it, is left at its mode.
#[test]
fn a_password_write_tightens_a_readable_file_to_0600() {
    use std::os::unix::fs::PermissionsExt;
    let mode =
        |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let dir = temp_dir("config-mode");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    // No password in the edit: the mode stays.
    let tightened = Config::update_mqtt_noting(&path, |m| m.commands = true).unwrap();
    assert!(!tightened);
    assert_eq!(mode(&path), 0o644);

    let tightened =
        Config::update_mqtt_noting(&path, |m| m.password = Some("fake".into())).unwrap();
    assert!(tightened);
    assert_eq!(mode(&path), 0o600);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("password = \"fake\"")
    );

    // Already private: nothing to say.
    let tightened =
        Config::update_mqtt_noting(&path, |m| m.password = Some("other".into())).unwrap();
    assert!(!tightened);
    assert_eq!(mode(&path), 0o600);

    // A group-writable 0660 loses group too; clearing the password never loosens.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
    assert!(Config::update_mqtt_noting(&path, |m| m.password = Some("x".into())).unwrap());
    assert_eq!(mode(&path), 0o600);
}
