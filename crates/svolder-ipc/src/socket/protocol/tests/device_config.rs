// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `device_config` family on the wire: its ops, its answer, and what an update leaves out.

use super::*;

fn request(text: &str) -> Request {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// Each op is the `op` of a `device_config` request, and names the device it acts on.
#[test]
fn the_device_config_ops_have_their_wire_shapes() {
    let cases = [
        (
            DeviceConfigOp::List,
            r#"{"cmd":"device_config","op":{"op":"list"}}"#,
        ),
        (
            DeviceConfigOp::Add {
                unit: "0102030405060708".into(),
                name: "dac".into(),
                profile: None,
                passphrase: None,
            },
            r#"{"cmd":"device_config","op":{"op":"add","unit":"0102030405060708","name":"dac"}}"#,
        ),
        (
            DeviceConfigOp::Add {
                unit: "0102030405060708".into(),
                name: "dac".into(),
                profile: Some("bifrost3".into()),
                passphrase: Some("1234".into()),
            },
            r#"{"cmd":"device_config","op":{"op":"add","unit":"0102030405060708","name":"dac","profile":"bifrost3","passphrase":"1234"}}"#,
        ),
        (
            DeviceConfigOp::Update {
                device: "dac".into(),
                name: Some("desk".into()),
                profile: Some(Some("bifrost3".into())),
                passphrase: Some("none".into()),
            },
            r#"{"cmd":"device_config","op":{"op":"update","device":"dac","name":"desk","profile":"bifrost3","passphrase":"none"}}"#,
        ),
        (
            DeviceConfigOp::Remove {
                device: "dac".into(),
            },
            r#"{"cmd":"device_config","op":{"op":"remove","device":"dac"}}"#,
        ),
        (
            DeviceConfigOp::SetDefault {
                device: "dac".into(),
            },
            r#"{"cmd":"device_config","op":{"op":"set_default","device":"dac"}}"#,
        ),
        (
            DeviceConfigOp::ClearDefault,
            r#"{"cmd":"device_config","op":{"op":"clear_default"}}"#,
        ),
    ];
    for (op, text) in cases {
        let parsed = Request::DeviceConfig { op };
        assert_eq!(serde_json::to_string(&parsed).unwrap(), text);
        assert_eq!(request(text), parsed);
        assert!(!parsed.is_stream());
    }
    assert!(REQUEST_CMDS.contains(&"device_config"));
}

/// An update changes only what it carries: a key left out is left as it is, and a `profile` of
/// `null` takes the pin off, back to the profile the daemon detects.
#[test]
fn an_update_tells_a_key_left_out_from_a_profile_taken_off() {
    let bare = request(r#"{"cmd":"device_config","op":{"op":"update","device":"dac"}}"#);
    assert_eq!(
        bare,
        Request::DeviceConfig {
            op: DeviceConfigOp::Update {
                device: "dac".into(),
                name: None,
                profile: None,
                passphrase: None,
            }
        }
    );
    assert_eq!(
        serde_json::to_string(&bare).unwrap(),
        r#"{"cmd":"device_config","op":{"op":"update","device":"dac"}}"#
    );
    let unpinned = r#"{"cmd":"device_config","op":{"op":"update","device":"dac","profile":null}}"#;
    let parsed = request(unpinned);
    assert_eq!(
        parsed,
        Request::DeviceConfig {
            op: DeviceConfigOp::Update {
                device: "dac".into(),
                name: None,
                profile: Some(None),
                passphrase: None,
            }
        }
    );
    assert_eq!(serde_json::to_string(&parsed).unwrap(), unpinned);
}

/// An op with no device is malformed, not a request about "the shown device".
#[test]
fn an_op_without_its_device_does_not_parse() {
    for text in [
        r#"{"cmd":"device_config","op":{"op":"remove"}}"#,
        r#"{"cmd":"device_config","op":{"op":"set_default"}}"#,
        r#"{"cmd":"device_config","op":{"op":"update","name":"x"}}"#,
        r#"{"cmd":"device_config","op":{"op":"add","name":"x"}}"#,
    ] {
        assert!(serde_json::from_str::<Request>(text).is_err(), "{text}");
    }
}

/// The answer lists each configured device as an editor needs it, and never a passphrase: only
/// which kind is configured.
#[test]
fn the_answer_has_its_wire_shape_and_no_passphrase() {
    let answer = Response::DeviceConfig {
        devices: vec![
            ConfiguredDevice {
                unit: "0102030405060708".into(),
                name: "dac".into(),
                profile: Some("bifrost3".into()),
                passphrase: PassphraseKind::Set,
                default: true,
                connected: false,
                not_connected_since_ms: Some(1_700_000_000_000),
            },
            ConfiguredDevice {
                unit: "1112131415161718".into(),
                name: "amp".into(),
                profile: None,
                passphrase: PassphraseKind::Auto,
                default: false,
                connected: true,
                not_connected_since_ms: None,
            },
        ],
        profiles: vec![
            ProfileChoice {
                name: "bifrost3".into(),
                display: Some("Bifrost 3".into()),
                base: false,
            },
            ProfileChoice {
                name: "dac-common".into(),
                display: None,
                base: true,
            },
        ],
    };
    let text = r#"{"result":"device_config","devices":[{"unit":"0102030405060708","name":"dac","profile":"bifrost3","passphrase":"set","default":true,"connected":false,"not_connected_since_ms":1700000000000},{"unit":"1112131415161718","name":"amp","passphrase":"auto","connected":true}],"profiles":[{"name":"bifrost3","display":"Bifrost 3"},{"name":"dac-common","base":true}]}"#;
    assert_eq!(serde_json::to_string(&answer).unwrap(), text);
    assert_eq!(serde_json::from_str::<Response>(text).unwrap(), answer);
    // A daemon built before the list answers the devices alone, and an answer with no profile
    // to offer is written the same way.
    let before = r#"{"result":"device_config","devices":[]}"#;
    let none = Response::DeviceConfig {
        devices: Vec::new(),
        profiles: Vec::new(),
    };
    assert_eq!(serde_json::from_str::<Response>(before).unwrap(), none);
    assert_eq!(serde_json::to_string(&none).unwrap(), before);
    assert_eq!(PassphraseKind::of("1234"), PassphraseKind::Set);
    assert_eq!(PassphraseKind::of("none"), PassphraseKind::None);
    assert_eq!(PassphraseKind::of("auto"), PassphraseKind::Auto);
    assert_eq!(PassphraseKind::of(""), PassphraseKind::Auto);
    let later = r#"{"unit":"u","name":"n","passphrase":"keyring","connected":false}"#;
    assert_eq!(
        serde_json::from_str::<ConfiguredDevice>(later)
            .unwrap()
            .passphrase,
        PassphraseKind::Unknown
    );
}

/// The family is a feature a front end asks about before it sends an op.
#[test]
fn device_crud_is_a_listed_feature() {
    assert_eq!(FEATURE_DEVICE_CRUD, "device_crud");
    assert!(FEATURES.contains(&FEATURE_DEVICE_CRUD));
}

/// A device's row carries its key readings in order, and a row from a daemon too old to list
/// them reads as listing none.
#[test]
fn a_devices_row_carries_the_key_readings() {
    let old = r#"{"name":"dac","unit":"0102030405060708","product_id":2,"product":null,"firmware":3,"profile":"gungnir2-v2","connected":true,"stale":false}"#;
    let mut row: DeviceInfo = serde_json::from_str(old).unwrap();
    assert!(row.key_readings.is_empty());
    assert_eq!(serde_json::to_string(&row).unwrap(), old);
    row.key_readings = vec![SERIES_SIGNAL.into(), "volume".into()];
    assert!(
        serde_json::to_string(&row)
            .unwrap()
            .ends_with(r#","key_readings":["signal","volume"]}"#)
    );
}

/// Nothing a `device_config` request carries comes back in the words that refuse it: a value of
/// the wrong type (a passphrase typed as a number), an op that is none, a line that is no object
/// are each refused naming the key, never the value. The server answers a line it cannot read
/// with these words.
#[test]
fn a_request_that_does_not_parse_is_refused_without_its_content() {
    let lines = [
        r#"{"cmd":"device_config","op":{"op":"add","unit":"0102030405060708","name":"amp","passphrase":4321}}"#,
        r#"{"cmd":"device_config","op":{"op":"add","unit":"0102030405060708","name":4321}}"#,
        r#"{"cmd":"device_config","op":{"op":"add","unit":4321,"name":"amp"}}"#,
        r#"{"cmd":"device_config","op":{"op":"add","unit":"u","name":"amp","profile":[4321]}}"#,
        r#"{"cmd":"device_config","op":{"op":"update","device":"dac","passphrase":4321}}"#,
        r#"{"cmd":"device_config","op":{"op":"update","device":"dac","passphrase":{"x":"4321"}}}"#,
        r#"{"cmd":"device_config","op":{"op":"update","device":4321}}"#,
        r#"{"cmd":"device_config","op":{"op":"remove","device":true,"note":"4321"}}"#,
        r#"{"cmd":"device_config","op":{"op":"4321"}}"#,
        r#"{"cmd":"device_config","op":{"op":4321}}"#,
        r#"{"cmd":"device_config","op":4321}"#,
        r#"{"cmd":"device_config","op":"4321"}"#,
    ];
    for line in lines {
        let refused = serde_json::from_str::<Request>(line).expect_err(line);
        let words = refused.to_string();
        assert!(!words.contains("4321"), "{line}: {words}");
        // The line as the server reads it: the request under `request`, beside its id.
        let wrapped = format!(r#"{{"id":7,"request":{line}}}"#);
        let refused = serde_json::from_str::<RequestLine>(&wrapped).expect_err(line);
        let words = refused.to_string();
        assert!(!words.contains("4321"), "{line}: {words}");
        assert!(
            !words.contains("missing field `request`"),
            "the line reached the op: {words}"
        );
    }
    // The wrapping is a line the server takes when the op is sound.
    let sound = r#"{"id":7,"request":{"cmd":"device_config","op":{"op":"list"}}}"#;
    assert_eq!(
        serde_json::from_str::<RequestLine>(sound).unwrap().request,
        Request::DeviceConfig {
            op: DeviceConfigOp::List
        }
    );
    let words = serde_json::from_str::<Request>(lines[0])
        .unwrap_err()
        .to_string();
    assert!(words.contains("passphrase"), "the key is named: {words}");
}

/// A request printed for a log or a panic shows that a passphrase is there, never which.
#[test]
fn a_printed_op_hides_the_passphrase() {
    let ops = [
        DeviceConfigOp::Add {
            unit: "0102030405060708".into(),
            name: "amp".into(),
            profile: None,
            passphrase: Some("4321".into()),
        },
        DeviceConfigOp::Update {
            device: "amp".into(),
            name: None,
            profile: None,
            passphrase: Some("4321".into()),
        },
    ];
    for op in ops {
        let printed = format!("{:?}", Request::DeviceConfig { op });
        assert!(!printed.contains("4321"), "{printed}");
        assert!(printed.contains("passphrase"), "{printed}");
        assert!(printed.contains("amp"), "{printed}");
    }
}
