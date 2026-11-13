// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The refusals a caller matches on, and the words each one says.

use super::*;

/// What each refusal says is pinned word for word: the variants carry what a caller matches on,
/// and the text every front end shows is the text it always was.
#[test]
fn every_refusal_keeps_its_wording() {
    let taken = ConfigError::Taken {
        kind: "device",
        name: "dac".into(),
    };
    assert_eq!(
        taken.to_string(),
        "config: a device named \"dac\" is already configured"
    );
    let missing = ConfigError::Missing {
        kind: "hop",
        name: "amp".into(),
    };
    assert_eq!(
        missing.to_string(),
        "config: no hop named \"amp\" is configured"
    );
    let gone = ConfigError::Vanished {
        kind: "preset",
        name: "warm".into(),
    };
    assert_eq!(gone.to_string(), "config: preset \"warm\" went missing");
    let busy = ConfigError::Busy("/x/config.toml".into());
    assert_eq!(
        busy.to_string(),
        "config io /x/config.toml: another svolder is writing this file"
    );
    let twice = ConfigError::UnitTwice {
        first: "dac".into(),
        second: "amp".into(),
        udid: "1A2B3C4D5E6F7A8B".into(),
    };
    assert_eq!(
        twice.to_string(),
        "config: devices \"dac\" and \"amp\" are both configured for unit 1A2B3C4D5E6F7A8B; \
         a unit is configured once, so remove one of the two blocks"
    );
}

/// The writers refuse with the variant, not with text a caller has to read.
#[test]
fn the_writers_refuse_with_variants() {
    let dir = temp_dir("config-errors");
    let path = dir.join("config.toml");
    Config::add_device(&path, &device("dac", "1112131415161718")).unwrap();
    assert!(matches!(
        Config::add_device(&path, &device("DAC", "0102030405060708")),
        Err(ConfigError::Taken { kind: "device", name }) if name == "dac"
    ));
    assert!(matches!(
        Config::remove_device(&path, "nobody"),
        Err(ConfigError::Missing { kind: "device", name }) if name == "nobody"
    ));
    assert!(matches!(
        Config::update_device(&path, "nobody", &device("x", "0102030405060708")),
        Err(ConfigError::Missing { kind: "device", .. })
    ));
    assert!(matches!(
        Config::add_device(&path, &device("twin", "1112131415161718")),
        Err(ConfigError::UnitTwice { first, second, .. }) if first == "dac" && second == "twin"
    ));
    assert!(matches!(
        Config::remove_hop(&path, "nobody"),
        Err(ConfigError::Missing { kind: "hop", .. })
    ));
    assert!(matches!(
        Config::remove_preset(&path, "nobody"),
        Err(ConfigError::Missing { kind: "preset", .. })
    ));
}

/// A writer that waits out the lock refuses with `Busy`, naming the file.
#[test]
fn a_writer_that_waits_out_the_lock_refuses_with_busy() {
    let dir = temp_dir("config-errors");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let held = flock::lock_beside(&path, flock::Wait::Not).unwrap();
    let err = Config::set_adapter(&path, Some("hci1")).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Busy(busy) if *busy == path),
        "{err:?}"
    );
    drop(held);
}
