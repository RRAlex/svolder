// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;

use svolder_ble::adapters::{AdapterInfo, Heard, Hearing};
use svolder_ipc::DeviceConfig;

use super::*;

fn device(name: &str, udid: &str) -> DeviceConfig {
    DeviceConfig {
        name: name.to_string(),
        udid: udid.to_string(),
        profile: None,
        passphrase: "auto".to_string(),
        inputs: BTreeMap::new(),
    }
}

fn hearing(name: &str, address: &str, usb: Option<&str>, rssi: Option<i16>) -> Hearing {
    Hearing {
        adapter: AdapterInfo {
            name: name.to_string(),
            address: address.to_string(),
            usb: usb.map(str::to_string),
        },
        heard: Ok(rssi
            .map(|rssi| Heard {
                unit: "0A1B2C3D4E5F6071".to_string(),
                rssi: Some(rssi),
            })
            .into_iter()
            .collect()),
    }
}

fn the_two() -> Vec<Row> {
    let hearings = [
        hearing(
            "hci0",
            "AA:BB:CC:DD:EE:01",
            Some("UB500 (2357:0604)"),
            Some(-62),
        ),
        hearing(
            "hci2",
            "AA:BB:CC:DD:EE:02",
            Some("UB500 (2357:0604)"),
            Some(-74),
        ),
    ];
    rows(
        &hearings,
        &[device("BF3", "0A1B2C3D4E5F6071")],
        Some("hci0"),
        &[],
    )
}

#[test]
fn the_table_is_the_approved_one() {
    assert_eq!(
        text_lines(&the_two()),
        [
            "hci0  AA:BB:CC:DD:EE:01  UB500 (2357:0604)  BF3 −62 dBm  ✓ in use",
            "hci2  AA:BB:CC:DD:EE:02  UB500 (2357:0604)  BF3 −74 dBm",
        ]
    );
}

#[test]
fn a_device_an_adapter_did_not_hear_reads_as_a_dash() {
    let hearings = [hearing("hci0", "AA:BB:CC:DD:EE:01", None, None)];
    let rows = rows(&hearings, &[device("BF3", "0A1B2C3D4E5F6071")], None, &[]);
    assert_eq!(text_lines(&rows), ["hci0  AA:BB:CC:DD:EE:01    BF3 —"]);
}

#[test]
fn an_adapter_that_could_not_listen_says_why() {
    let hearings = [Hearing {
        adapter: AdapterInfo {
            name: "hci1".into(),
            address: String::new(),
            usb: None,
        }
        .clone(),
        heard: Err("adapter unavailable: off".into()),
    }];
    let rows = rows(&hearings, &[device("BF3", "0A1B2C3D4E5F6071")], None, &[]);
    assert!(text_lines(&rows)[0].contains("cannot listen: adapter unavailable: off"));
}

#[test]
fn the_json_is_one_object_per_adapter() {
    let rows = the_two();
    assert_eq!(
        json_line(&rows[0]),
        serde_json::json!({
            "name": "hci0",
            "address": "AA:BB:CC:DD:EE:01",
            "usb": "UB500 (2357:0604)",
            "in_use": true,
            "listened": true,
            "error": null,
            "heard": [{"device": "BF3", "rssi": -62}],
        })
    );
    assert_eq!(json_line(&rows[1])["in_use"], false);
}

#[test]
fn only_the_adapter_in_use_is_marked() {
    let rows = the_two();
    assert!(rows[0].in_use && !rows[1].in_use);
}

fn two_with(held: &[Held]) -> Vec<Row> {
    let hearings = [
        hearing("hci0", "AA:BB:CC:DD:EE:01", None, None),
        hearing("hci2", "AA:BB:CC:DD:EE:02", None, Some(-74)),
    ];
    rows(
        &hearings,
        &[device("BF3", "0A1B2C3D4E5F6071")],
        Some("hci0"),
        held,
    )
}

/// The adapter that holds the link says `connected` with the last strength and how long ago it
/// was heard, where one is known; the other adapter says what it hears. A connected device is not
/// heard, so the holding adapter never reads `—`.
#[test]
fn the_adapter_holding_the_link_says_connected_with_the_last_signal() {
    let held = [Held {
        device: "BF3".into(),
        rssi: Some(-62),
        age_s: Some(12),
    }];
    let lines = text_lines(&two_with(&held));
    assert!(
        lines[0].contains("BF3 connected, −62 dBm 12 s ago"),
        "{lines:?}"
    );
    assert!(lines[0].ends_with("✓ in use"), "{lines:?}");
    assert!(lines[1].contains("BF3 −74 dBm"), "{lines:?}");

    let unknown = [Held {
        device: "bf3".into(),
        rssi: None,
        age_s: None,
    }];
    assert!(text_lines(&two_with(&unknown))[0].contains("BF3 connected"));
    assert!(!text_lines(&two_with(&unknown))[0].contains('—'));
}

#[test]
fn the_json_of_a_held_device_is_connected_with_rssi_and_age() {
    let held = [Held {
        device: "BF3".into(),
        rssi: Some(-62),
        age_s: Some(12),
    }];
    let rows = two_with(&held);
    assert_eq!(
        json_line(&rows[0])["heard"],
        serde_json::json!([{"device": "BF3", "connected": true, "rssi": -62, "age_s": 12}])
    );
    assert_eq!(json_line(&rows[1])["heard"][0]["rssi"], -74);
}

/// An adapter that could not listen says so in the JSON, apart from one that heard nothing.
#[test]
fn the_json_tells_an_adapter_that_could_not_listen_from_one_that_heard_nothing() {
    let silent = rows(
        &[hearing("hci0", "AA:BB:CC:DD:EE:01", None, None)],
        &[device("BF3", "0A1B2C3D4E5F6071")],
        None,
        &[],
    );
    let json = json_line(&silent[0]);
    assert_eq!(json["listened"], true);
    assert_eq!(json["heard"], serde_json::json!([]));
    assert!(json["error"].is_null());

    let deaf = [Hearing {
        adapter: AdapterInfo {
            name: "hci1".into(),
            address: String::new(),
            usb: None,
        },
        heard: Err("adapter unavailable: off".into()),
    }];
    let rows = rows(&deaf, &[device("BF3", "0A1B2C3D4E5F6071")], None, &[]);
    let json = json_line(&rows[0]);
    assert_eq!(json["listened"], false);
    assert_eq!(json["error"], "adapter unavailable: off");
}

fn present() -> Vec<AdapterInfo> {
    ["hci0", "hci2"]
        .iter()
        .zip(["AA:BB:CC:DD:EE:01", "AA:BB:CC:DD:EE:02"])
        .map(|(name, address)| AdapterInfo {
            name: (*name).into(),
            address: address.into(),
            usb: None,
        })
        .collect()
}

/// The adapter in use is what a session really opens: `--adapter` first, then the remembered
/// adapter (by address or an older hciN), then the config, then the first.
#[test]
fn the_adapter_in_use_follows_what_a_session_opens() {
    let there = present();
    let used = |named, remembered, configured| in_use(named, remembered, configured, &there).0;
    assert_eq!(used(None, None, None).as_deref(), Some("hci0"));
    assert_eq!(
        used(None, None, Some("AA:BB:CC:DD:EE:02")).as_deref(),
        Some("hci2")
    );
    assert_eq!(
        used(None, Some("hci2"), Some("AA:BB:CC:DD:EE:01")).as_deref(),
        Some("hci2"),
        "the remembered adapter outranks the config, as a direct session has it"
    );
    assert_eq!(
        used(Some("aa:bb:cc:dd:ee:01"), Some("hci2"), Some("hci2")).as_deref(),
        Some("hci0")
    );
}

/// A configured address no adapter has is said, with the adapters there are, and the first
/// adapter stands in as the one in use only because nothing better is named.
#[test]
fn a_configured_address_no_adapter_has_is_said() {
    let (used, note) = in_use(None, None, Some("AA:BB:CC:DD:EE:99"), &present());
    assert_eq!(used.as_deref(), Some("hci0"));
    let note = note.expect("a note");
    assert!(
        note.starts_with("the configured adapter matches no adapter"),
        "{note}"
    );
    assert!(note.contains("AA:BB:CC:DD:EE:01"), "{note}");
}

/// Under a daemon the mark is the adapter the daemon says it holds, which neither this process's
/// `--adapter` nor the config can outrank (a pinned daemon, or one that could not open the
/// config's adapter, holds another); one that says nothing leaves the config's answer.
#[test]
fn under_a_daemon_the_adapter_in_use_is_the_one_it_holds() {
    let there = present();
    let holds = Via::Daemon {
        holds: Some("hci2"),
    };
    let (used, note) = in_use_via(
        &holds,
        Some("hci0"),
        Some("hci0"),
        Some("AA:BB:CC:DD:EE:01"),
        &there,
    );
    assert_eq!(used.as_deref(), Some("hci2"));
    assert_eq!(note, None);
    let silent = Via::Daemon { holds: None };
    let (used, _) = in_use_via(
        &silent,
        Some("hci0"),
        None,
        Some("AA:BB:CC:DD:EE:02"),
        &there,
    );
    assert_eq!(used.as_deref(), Some("hci2"), "the config's, not --adapter");
    let (used, _) = in_use_via(&Via::Direct, Some("hci0"), None, Some("hci2"), &there);
    assert_eq!(used.as_deref(), Some("hci0"));
}
