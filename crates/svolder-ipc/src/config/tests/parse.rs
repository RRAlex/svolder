// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Parsing a configuration file: the devices, the defaults, and what is refused.

use super::*;

const SAMPLE: &str = r#"
schema = 1
adapter = "hci0"
default_device = "bifrost"

[[device]]
name = "bifrost"
udid = "1A2B3C4D5E6F7A8B"
profile = "bifrost3"
passphrase = "auto"

[device.inputs]
3 = "TV optical"

[[device]]
name = "gungnir"
udid = "0102030405060708"
"#;

#[test]
fn parses_devices_and_defaults() {
    let c = Config::parse(SAMPLE).unwrap();
    assert_eq!(c.adapter.as_deref(), Some("hci0"));
    assert_eq!(c.default_device.as_deref(), Some("bifrost"));
    assert_eq!(c.devices.len(), 2);
    assert_eq!(c.devices[0].name, "bifrost");
    assert_eq!(c.devices[0].passphrase, "auto");
    assert_eq!(
        c.devices[0].inputs.get(&3).map(String::as_str),
        Some("TV optical")
    );
    // A device with no passphrase field defaults to "auto".
    assert_eq!(c.devices[1].passphrase, "auto");
    assert!(c.devices[1].profile.is_none());
}

#[test]
fn an_empty_file_is_a_valid_empty_config() {
    let c = Config::parse("").unwrap();
    assert!(c.devices.is_empty());
    assert_eq!(c, Config::default());
}

#[test]
fn a_future_schema_is_rejected() {
    let err = Config::parse("schema = 2").unwrap_err();
    assert!(matches!(err, ConfigError::Schema(2)));
}

#[test]
fn a_bad_udid_is_rejected() {
    let err = Config::parse("[[device]]\nname=\"x\"\nudid=\"nothex\"").unwrap_err();
    assert!(matches!(err, ConfigError::Parse(_)));
}

#[test]
fn a_blank_socket_is_no_override() {
    for text in ["socket = \"\"\n", "socket = \"   \"\n"] {
        let c = Config::parse(text).unwrap();
        assert_eq!(c.socket, None, "{text:?}");
    }
    let c = Config::parse("socket = \"/run/x.sock\"\n").unwrap();
    assert_eq!(c.socket.as_deref(), Some("/run/x.sock"));
}

#[test]
fn a_default_device_naming_nothing_configured_loads_with_a_warning() {
    let text = "default_device = \"nope\"\n\
                    [[device]]\nname=\"fake\"\nudid=\"0102030405060708\"\n";
    let config = Config::parse(text).expect("a dangling default is no parse error");
    assert_eq!(config.dangling_default(), Some("nope"));
    assert_eq!(
        config.warnings(),
        vec![r#"default_device "nope" names no configured device"#.to_string()]
    );
    let text = "default_device = \"fake\"\n\
                    [[device]]\nname=\"fake\"\nudid=\"0102030405060708\"\n";
    assert!(Config::parse(text).unwrap().warnings().is_empty());
    // The name matches case aside, as every other device selector does.
    let text = "default_device = \"FAKE\"\n\
                    [[device]]\nname=\"fake\"\nudid=\"0102030405060708\"\n";
    assert_eq!(
        Config::parse(text).unwrap().default_device.as_deref(),
        Some("FAKE")
    );
}

#[test]
fn a_non_numeric_input_key_is_rejected() {
    let text = "[[device]]\nname=\"x\"\nudid=\"0102030405060708\"\n\
                    [device.inputs]\nfoo=\"bar\"\n";
    let err = Config::parse(text).unwrap_err();
    assert!(matches!(err, ConfigError::Parse(_)));
}

#[test]
fn load_from_an_absent_path_is_the_default_config() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let c = Config::load_from(&path).unwrap();
    assert_eq!(c, Config::default());
}

#[test]
fn load_from_parses_a_scratch_file() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, SAMPLE).unwrap();
    let c = Config::load_from(&path).unwrap();
    assert_eq!(c.devices.len(), 2);
}

/// An optional key left empty overrides nothing, which is what lets the template show each
/// of them at the value it has while absent.
#[test]
fn a_blank_optional_key_is_no_override() {
    let c = Config::parse(
        "schema = 1\nadapter = \"\"\ndefault_device = \"\"\n[tui]\naccent = \"\"\n\
             [tui.accents]\ndac = \"\"\n",
    )
    .unwrap();
    assert_eq!(c.adapter, None);
    assert_eq!(c.default_device, None);
    assert_eq!(c.tui.accent, None);
    assert_eq!(c.tui.accents.dac, None);
}

/// The daemon watches the host while the file says nothing; `[host] enabled = false` turns it
/// off, and a word that is no bool is refused.
#[test]
fn host_table_defaults_to_enabled() {
    assert!(Config::parse("schema = 1\n").unwrap().host.enabled);
    assert!(Config::default().host.enabled);
    assert!(Config::parse("[host]\n").unwrap().host.enabled);
    assert!(
        Config::parse("[host]\nenabled = true\n")
            .unwrap()
            .host
            .enabled
    );
    assert!(
        !Config::parse("[host]\nenabled = false\n")
            .unwrap()
            .host
            .enabled
    );
    assert!(Config::parse("[host]\nenabled = \"maybe\"\n").is_err());
}
