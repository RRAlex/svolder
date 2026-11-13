// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_ble::{Advertisement, FakeTransport};
use svolder_ipc::{DeviceConfigOp, DeviceInfo};
use svolder_profiles::Registry;
use svolder_proto::{Passphrase, UnitId};

use super::devices::device_info_line;
use super::eq::{partial_failure_line, write_count};
use super::select::ambiguous;
use super::*;

/// One advertisement, as a fake device on `unit` broadcasts it.
fn advert(unit: &str) -> Advertisement {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a built-in profile");
    let unit: UnitId = unit.parse().expect("a valid unit id");
    let passphrase = Passphrase::from_digits([1, 2, 3, 4]).expect("four digits");
    let transport = FakeTransport::from_profile(profile, unit, 3, passphrase);
    let manufacturer = transport
        .device()
        .lock()
        .expect("device mutex")
        .manufacturer_data();
    Advertisement {
        manufacturer,
        rssi: Some(-50),
        record: None,
    }
}

/// A bare selector with several devices in range names them all, on the one line a
/// failure's words are: a front end draws them on a single key line.
#[test]
fn the_candidates_a_bare_selector_leaves_are_named_on_one_line() {
    let found = [advert("0102030405060708"), advert("0807060504030201")];
    let failure = ambiguous(&found, &Registry::builtin().expect("the built-in profiles"));
    assert_eq!(failure.code, 2, "an unresolved selector still exits 2");
    assert!(
        !failure.what.contains('\n'),
        "the words run over more than one line: {}",
        failure.what
    );
    assert!(
        failure.what.starts_with("2 devices in range: "),
        "{}",
        failure.what
    );
    for unit in ["0102030405060708", "0807060504030201"] {
        assert!(failure.what.contains(unit), "{}", failure.what);
    }
    assert_eq!(
        failure.next,
        crate::recovery::next_step(Kind::AmbiguousDevice)
    );
}

/// A device the daemon holds is listed with its link, and one the daemon is waiting to
/// try again for says when it will, in the words `svolder daemon status` uses.
#[test]
fn a_listed_device_says_when_the_daemon_tries_it_again() {
    let mut info = DeviceInfo {
        signal: None,
        name: "dac".into(),
        unit: "0102030405060708".into(),
        product_id: 2,
        product: Some("Gungnir 2".into()),
        firmware: 3,
        profile: "gungnir2-v2".into(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
        key_readings: Vec::new(),
    };
    let now_ms = Some(1_700_000_300_000);
    assert_eq!(
        device_info_line(&info, now_ms),
        "dac  Gungnir 2  fw 3  connected  signal \u{2014}"
    );
    info.connected = false;
    assert_eq!(
        device_info_line(&info, now_ms),
        "dac  Gungnir 2  fw 3  disconnected  signal \u{2014}"
    );
    info.retry_in_secs = Some(15);
    info.stale = true;
    assert_eq!(
        device_info_line(&info, now_ms),
        "dac  Gungnir 2  fw 3  disconnected, next attempt in 15 s \u{b7} readings stale  signal \u{2014}"
    );
    // The link's flags read as the TUI and the web read them.
    info.connected = true;
    info.retry_in_secs = None;
    info.refused = true;
    assert_eq!(
        device_info_line(&info, now_ms),
        "dac  Gungnir 2  fw 3  connected \u{b7} passphrase refused \u{b7} readings stale  signal \u{2014}"
    );
}

/// A device the daemon has heard says its signal with an ASCII minus, where it was heard and how
/// long ago.
#[test]
fn a_listed_device_says_its_signal() {
    let mut info = DeviceInfo {
        signal: Some(svolder_ipc::Signal {
            dbm: -62,
            source: svolder_ipc::SignalSource::Advert,
            heard_at_ms: 1_700_000_000_000,
        }),
        name: "dac".into(),
        unit: "0102030405060708".into(),
        product_id: 2,
        product: Some("Gungnir 2".into()),
        firmware: 3,
        profile: "gungnir2-v2".into(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
        key_readings: Vec::new(),
    };
    assert_eq!(
        device_info_line(&info, Some(1_700_000_180_000)),
        "dac  Gungnir 2  fw 3  connected  signal -62 dBm advert, 3 min ago"
    );
    // No clock: the strength with no age, not a false one.
    assert_eq!(
        device_info_line(&info, None),
        "dac  Gungnir 2  fw 3  connected  signal -62 dBm advert"
    );
    info.signal = None;
    assert!(device_info_line(&info, Some(0)).ends_with("signal \u{2014}"));
}

/// Nothing in range at all is the plain not-found, with no candidates to name.
#[test]
fn a_selector_with_nothing_in_range_is_the_plain_not_found() {
    let failure = ambiguous(&[], &Registry::builtin().expect("the built-in profiles"));
    assert_eq!(failure.code, 2);
    assert_eq!(failure.what, BleError::NotFound.to_string());
    assert_eq!(
        failure.next,
        crate::recovery::next_step(Kind::NothingInRange)
    );
}

/// `1 write`, plural everywhere else, including zero.
#[test]
fn write_count_is_singular_only_at_one() {
    assert_eq!(write_count(0), "0 writes");
    assert_eq!(write_count(1), "1 write");
    assert_eq!(write_count(2), "2 writes");
    assert_eq!(write_count(5), "5 writes");
}

/// `eq load`/`eq flatten`'s partial-failure line reads naturally whatever the plan's size
/// was: a plan of one write gets its own sentence, since `sent` is always `0` there (the
/// only index there was to fail at) and "0 of 1 write went through" reads oddly; a longer
/// plan reads as "N of M went through", pluralising `M` correctly.
#[test]
fn the_partial_failure_line_reads_naturally_at_every_count() {
    assert_eq!(
        partial_failure_line(0, 1),
        "the write failed; nothing went through"
    );
    assert_eq!(
        partial_failure_line(0, 5),
        "only 0 of 5 writes went through"
    );
    assert_eq!(
        partial_failure_line(2, 5),
        "only 2 of 5 writes went through"
    );
    assert_eq!(
        partial_failure_line(1, 2),
        "only 1 of 2 writes went through"
    );
}

/// A preset verb's reload goes to a daemon that reads the file it wrote, and to no other: not
/// one that reads another file, and none at all under a direct transport.
#[tokio::test]
async fn a_preset_verb_reloads_only_a_daemon_reading_its_file() {
    use crate::test_support::stub_daemon::stub_daemon;

    let dir = std::env::temp_dir().join(format!("svolder-preset-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a directory");
    let path = dir.join("config.toml");
    std::fs::write(&path, "").expect("the file");
    let config = Config::load_from(&path).expect("an empty file loads");
    let daemon = stub_daemon().await;
    let socket = daemon.socket.display().to_string();
    let globals = |transport| Globals {
        transport,
        adapter: None,
        socket: Some(&socket),
        device: None,
        verbose: false,
    };

    daemon.says(None, path.to_str());
    reload_daemon_for_presets(&globals("auto"), &config, &path).await;
    assert_eq!(daemon.reloads(), 1, "a daemon reading the file");
    reload_daemon_for_presets(&globals("fake"), &config, &path).await;
    reload_daemon_for_presets(&globals("ble"), &config, &path).await;
    assert_eq!(daemon.reloads(), 1, "a direct transport has no daemon");
    daemon.says(None, Some("/elsewhere/config.toml"));
    reload_daemon_for_presets(&globals("auto"), &config, &path).await;
    assert_eq!(daemon.reloads(), 1, "a daemon reading another file");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn value_lines_are_the_status_lines_inspect_prints() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a built-in profile");
    let message = svolder_proto::Message::status(
        svolder_proto::FunctionId::one(254),
        svolder_proto::Value::U8(7),
    );
    let reading = svolder_profiles::describe(profile, &message);
    let value = ValueObject::from_reading(&reading, &message);
    let lines = value_lines(&[value.clone(), value.clone()]);
    assert_eq!(lines, vec![crate::format::line(&value); 2]);
    assert!(lines[0].starts_with("Status 254 u8 7"), "{lines:?}");
    assert_eq!(value_lines(&[]), Vec::<String>::new());
}

/// `add` and `remove` go to a daemon that runs as ops and write nothing themselves; no daemon
/// leaves the caller's own writer standing; a daemon without `device_crud` is refused with the
/// step that brings it up to date; a refusal keeps its reason.
#[tokio::test]
async fn the_device_verbs_write_through_a_daemon_that_runs_and_never_behind_it() {
    use crate::test_support::stub_daemon::stub_daemon;
    use svolder_ipc::{CODE_USAGE, Reason};

    let config = Config::default();
    let daemon = stub_daemon().await;
    let socket = daemon.socket.display().to_string();
    let g = Globals {
        transport: "auto",
        adapter: None,
        socket: Some(&socket),
        device: None,
        verbose: false,
    };
    let remove = || DeviceConfigOp::Remove {
        device: "dac".into(),
    };

    // A daemon without the family: refused, and nothing was sent.
    let failure = devices::through_daemon(&g, &config, remove())
        .await
        .unwrap_err();
    assert!(failure.next.contains("svolder daemon restart"), "{failure}");
    assert_eq!(
        failure.code, 7,
        "a daemon too old is not a lost link: {failure}"
    );
    assert!(daemon.device_ops().is_empty());

    // One that takes it.
    daemon.lists_features(&["device_crud"]);
    assert_eq!(
        devices::through_daemon(&g, &config, remove()).await.ok(),
        Some(true)
    );
    assert_eq!(daemon.device_ops(), [remove()]);

    // A refusal keeps its reason and is worded from it.
    daemon.refuses_device_ops(
        svolder_ipc::Response::error(CODE_USAGE, "x").with_reason(Reason::Missing),
    );
    let failure = devices::through_daemon(&g, &config, remove())
        .await
        .unwrap_err();
    assert_eq!(failure.reason, Some(Reason::Missing));
    assert!(failure.what.contains("no longer configured"), "{failure}");

    // No daemon at all: the caller's own writer stands.
    let gone = std::env::temp_dir().join(format!("svolder-no-daemon-{}.sock", std::process::id()));
    let gone = gone.display().to_string();
    let g = Globals {
        socket: Some(&gone),
        ..g
    };
    assert_eq!(
        devices::through_daemon(&g, &config, remove()).await.ok(),
        Some(false)
    );
}

/// The AutoEq verbs ask the daemon that has the `autoeq` feature and no other: a daemon without it
/// is asked to restart (nothing was sent), a run under a transport that opens no daemon and one
/// with nothing on the socket say AutoEq needs svolderd.
#[tokio::test]
async fn the_autoeq_verbs_ask_a_daemon_that_has_the_feature() {
    use crate::test_support::stub_daemon::stub_daemon;
    let config = Config::default();
    let daemon = stub_daemon().await;
    let socket = daemon.socket.display().to_string();
    let g = Globals {
        transport: "auto",
        adapter: None,
        socket: Some(&socket),
        device: None,
        verbose: false,
    };

    let failure = autoeq::daemon(&g, &config, "search AutoEq")
        .await
        .unwrap_err();
    assert!(failure.what.contains("cannot search AutoEq"), "{failure}");
    // The code of a request the daemon does not know (docs/usage/exit-codes.md), not a refusal's.
    assert_eq!(failure.code, svolder_ipc::CODE_UNKNOWN_REQUEST);
    assert!(
        failure.to_string().contains("svolder daemon restart"),
        "{failure}"
    );

    // With the feature the daemon is the one asked; a search is its answer.
    daemon.autoeq().index(&[
        "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990",
    ]);
    assert_eq!(
        autoeq::daemon(&g, &config, "search AutoEq").await.ok(),
        Some(daemon.socket.clone())
    );
    let failure = crate::autoeq::search(&daemon.socket, "hd650", false)
        .await
        .map(|found| found.groups.len());
    assert_eq!(failure.ok(), Some(1));

    // A transport that opens no daemon never asks one.
    let ble = Globals {
        transport: "ble",
        ..g
    };
    let failure = autoeq::daemon(&ble, &config, "search AutoEq")
        .await
        .unwrap_err();
    assert!(
        failure.what.contains("`--transport ble` opens no daemon"),
        "{failure}"
    );

    // Nothing on the socket.
    let gone = std::env::temp_dir().join(format!("svolder-no-autoeq-{}.sock", std::process::id()));
    let gone = gone.display().to_string();
    let none = Globals {
        socket: Some(&gone),
        ..g
    };
    let failure = autoeq::daemon(&none, &config, "search AutoEq")
        .await
        .unwrap_err();
    assert!(
        failure
            .what
            .starts_with("AutoEq needs svolderd, which fetches it: no daemon answers on"),
        "{failure}"
    );
    assert!(failure.next.contains("svolder daemon restart"), "{failure}");
}

/// A refusal that is an interrupt leaves with 130, as the walk does, not with the usage code.
#[test]
fn an_interrupted_autoeq_import_exits_130() {
    let refused = crate::autoeq::Refused {
        message: "interrupted; the import is cancelled".into(),
        reason: None,
        too_old: false,
        interrupted: true,
    };
    let code = autoeq::refusal_exit(&refused, false);
    assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(130)));
    let other = crate::autoeq::Refused {
        interrupted: false,
        ..refused
    };
    let code = autoeq::refusal_exit(&other, false);
    assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(1)));
}

/// The missing-file words give the step themselves: no "See `svolder --help`" follows them.
#[test]
fn an_autoeq_refusal_for_a_missing_file_adds_no_step() {
    let refused = crate::autoeq::Refused {
        message: svolder_ipc::socket::daemon_version::config_missing_refusal("/x/config.toml"),
        reason: Some(svolder_ipc::Reason::ReadOnly),
        too_old: false,
        interrupted: false,
    };
    let failure = autoeq::refusal(&refused, false);
    assert_eq!(failure.what, refused.message);
    assert_eq!(failure.next, "");
}
