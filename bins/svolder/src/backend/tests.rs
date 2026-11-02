// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use svolder_profiles::ProfileError;

/// Everything a device verb needs, for a case that only cares about `--transport` and
/// `--socket`.
fn globals<'a>(transport: &'a str, socket: Option<&'a str>) -> Globals<'a> {
    Globals {
        transport,
        adapter: None,
        socket,
        device: None,
        verbose: false,
    }
}

/// A unique socket path for this case, inside a directory of the case's own that is
/// removed with the socket in it when the case lets go of the directory, however it ends.
fn temp_socket() -> (crate::test_support::TempDir, std::path::PathBuf) {
    let dir = crate::test_support::temp_dir("backend-unit");
    let socket = dir.0.join("svolder.sock");
    (dir, socket)
}

/// `--transport ble` answers from the configuration without consulting a daemon, the way
/// every other verb runs direct under it.
///
/// The socket below is bound but never accepts a request, so a call that consulted it
/// would wait for a reply that never comes: this case fails on its own deadline rather
/// than on an assertion if the daemon is reached.
#[tokio::test]
async fn ble_answers_from_the_configuration_even_with_a_daemon_listening() {
    // A configuration that failed to load would come back before the transport was even
    // looked at, which would let this case pass without proving anything.
    assert!(
        Config::load().is_ok(),
        "this case needs a configuration that loads"
    );
    let (_dir, socket) = temp_socket();
    let _listener = tokio::net::UnixListener::bind(&socket).expect("bind the test socket");
    let path = socket.to_string_lossy().into_owned();
    let rows = tokio::time::timeout(
        Duration::from_secs(5),
        Backend::devices_if_running(&globals("ble", Some(&path))),
    )
    .await
    .expect("`ble` consulted the daemon instead of the configuration");
    assert!(rows.is_ok(), "the configured rows come back");
}

/// The values a front end with no verb to open a backend for accepts are the ones a verb's
/// own dispatch runs on, and anything else is refused in the words every caller uses.
#[test]
fn the_transports_a_front_door_accepts_are_the_ones_a_verb_runs_on() {
    for name in ["auto", "daemon", "ble", "fake"] {
        assert!(known_transport(name).is_ok(), "{name}");
    }
    let refused = known_transport("nope").expect_err("an unknown transport is refused");
    assert_eq!(refused.code, Kind::Usage.code());
    assert!(refused.what.contains("nope"), "{}", refused.what);
    assert_eq!(refused.next, next_step(Kind::Usage));
}

/// What opening kept for its caller is said once and then gone: the CLI prints each line on
/// standard error, the TUI shows it as a notice, and neither says it twice.
///
/// This is how a daemon the launcher chain could not start is reported. The chain says
/// nothing itself — a front end drawing on the alternate screen has no terminal to say it
/// on — so what it found rides here, to wherever the front end that asked puts its words.
#[tokio::test]
async fn what_opening_kept_is_said_once() {
    let (mut backend, _gate) = Backend::fake(Duration::from_secs(1)).await;
    assert!(backend.take_warnings().is_empty(), "an open that went well");

    let failed = launch::AutospawnFailed {
        what: "the daemon did not answer in 2s".into(),
        log: std::path::PathBuf::from("/state/svolder/svolderd.log"),
    };
    backend.warn(failed.line());
    assert!(
        failed.line().contains("using a direct session")
            && failed.line().contains("/state/svolder/svolderd.log"),
        "{}",
        failed.line()
    );
    assert_eq!(backend.take_warnings(), vec![failed.line()]);
    assert!(
        backend.take_warnings().is_empty(),
        "a line said once is not said again"
    );
    backend.close().await;
}

/// A warning opening kept survives a scan run over the same backend: [`Backend::scan`]
/// reads the transport and nothing else, so it neither clears what [`Backend::open`] kept
/// nor says it a second time.
///
/// This is the scan path's half of what [`what_opening_kept_is_said_once`] proves for the
/// verbs that connect: `svolder scan` and the search behind `svolder add` both open a
/// backend once and call [`Backend::scan`] over it, so a daemon that this process tried and
/// failed to start is still named on the way to a fall-back scan.
#[tokio::test]
async fn what_opening_kept_survives_a_scan() {
    let (mut backend, _gate) = Backend::fake(Duration::from_secs(1)).await;
    let failed = launch::AutospawnFailed {
        what: "the daemon did not answer in 2s".into(),
        log: std::path::PathBuf::from("/state/svolder/svolderd.log"),
    };
    backend.warn(failed.line());

    let seen = backend.scan(Duration::from_millis(1)).await;
    assert!(seen.is_ok(), "a scan over the fake device: {seen:?}");

    assert_eq!(backend.take_warnings(), vec![failed.line()]);
    assert!(
        backend.take_warnings().is_empty(),
        "a line said once is not said again"
    );
    backend.close().await;
}

/// Asking which devices a daemon holds, when a verb is only looking, starts no daemon: the
/// rows come from the configuration and the machine is left as it was found.
///
/// Autospawn is on — it is on by default and nothing here turns it off — so a call that
/// walked the whole launcher chain, as [`Backend::open`] does, would have started one on
/// this very socket.
#[tokio::test]
async fn looking_at_the_devices_starts_no_daemon() {
    assert!(
        Config::load().is_ok_and(|config| config.daemon.autospawn),
        "this case needs a configuration that loads with autospawn on"
    );
    let (_dir, socket) = temp_socket();
    let path = socket.to_string_lossy().into_owned();

    let rows = tokio::time::timeout(
        Duration::from_secs(10),
        Backend::devices_if_running(&globals("auto", Some(&path))),
    )
    .await
    .expect("looking at the devices took too long");
    assert!(rows.is_ok(), "the configured rows come back");
    assert!(
        crate::test_support::daemons_on(&socket).is_empty(),
        "looking started a daemon"
    );
    assert!(!socket.exists(), "looking bound the socket");
}

/// An unrecognised `--transport` is refused here exactly as [`Backend::open`] refuses it,
/// rather than quietly answered with the configured rows.
#[tokio::test]
async fn an_unknown_transport_is_refused() {
    assert!(
        Config::load().is_ok(),
        "this case needs a configuration that loads"
    );
    let refused = Backend::devices_if_running(&globals("nope", None)).await;
    assert!(refused.is_err(), "an unknown transport was answered");
}

/// Without a daemon the rows are the configured devices, in file order, with no link
/// state: the file says which devices exist, not how they are doing.
#[test]
fn configured_devices_become_rows_with_no_link_state() {
    let config = Config::parse(
        "schema = 1\n\
             [[device]]\nname = \"gungnir\"\nudid = \"0102030405060708\"\n\
             [[device]]\nname = \"bifrost\"\nudid = \"0807060504030201\"\n",
    )
    .expect("a valid configuration");
    let rows = rows_from_config(&config);
    assert_eq!(
        rows,
        vec![
            DeviceRow {
                name: "gungnir".into(),
                product: None,
                connected: None,
                retry_in_secs: None,
                ..DeviceRow::default()
            },
            DeviceRow {
                name: "bifrost".into(),
                product: None,
                connected: None,
                retry_in_secs: None,
                ..DeviceRow::default()
            },
        ]
    );
}

/// A configuration with no devices lists none, rather than standing in for one.
#[test]
fn a_configuration_with_no_devices_lists_none() {
    let config = Config::parse("schema = 1\n").expect("a valid configuration");
    assert!(rows_from_config(&config).is_empty());
}

/// A direct session has no daemon to ask for a version, so this asks for nothing at all —
/// never a request over the wire, and never a future to spawn.
#[tokio::test]
async fn a_direct_backend_has_no_daemon_version_to_ask() {
    let (backend, _gate) = Backend::fake(Duration::from_secs(1)).await;
    assert!(backend.daemon_version_request().is_none());
}

/// Every way a write can fail names a step, and none of them moves the code it leaves
/// with: the device's own refusals take the step their kind earns, a mistyped command
/// takes the help, a link that is away takes the dropped-link step, a reading the device
/// has never sent takes the one that looks at what it does send, and a daemon error keeps
/// the code the daemon chose.
#[test]
fn every_write_failure_names_a_step_and_keeps_its_code() {
    let rejected = WriteError::Ble(BleError::PassphraseRejected).failure();
    assert_eq!(rejected.code, 3);
    assert_eq!(rejected.next, next_step(Kind::Refused));

    let garbled = WriteError::Ble(BleError::Profile(ProfileError::NoEq)).failure();
    assert_eq!(garbled.code, 1);
    assert_eq!(garbled.next, crate::recovery::INSPECT);

    let mistyped = bad_request("no control named nope").failure();
    assert_eq!(mistyped.code, 1);
    assert_eq!(mistyped.next, next_step(Kind::Usage));

    let away = no_link("the link is reconnecting").failure();
    assert_eq!(away.code, 3);
    assert_eq!(away.next, next_step(Kind::Dropped));

    let unreported = refused(4, crate::recovery::INSPECT, "no current value for volume").failure();
    assert_eq!(unreported.code, 4);
    assert_eq!(unreported.next, crate::recovery::INSPECT);

    let daemon = refused(5, next_step(Kind::DaemonError), "no such device").failure();
    assert_eq!(daemon.code, 5, "the daemon's own code travels unchanged");
    assert_eq!(daemon.next, next_step(Kind::DaemonError));
}

/// A backend whose reconnect attempt is still inside a connect that takes `connect_time`, so
/// closing it waits that long.
async fn reconnecting_for(connect_time: Duration) -> Backend {
    let (mut backend, gate) = Backend::fake(Duration::from_secs(30)).await;
    gate.slow_connects(connect_time);
    gate.sever();
    backend.open_feed().await.expect("the feed opens");
    backend.next_status().await.expect("an update");
    backend
}

/// A release that outlasts the quiet period says so, once, and is still waited for.
#[tokio::test(start_paused = true)]
async fn a_slow_release_says_so_once_and_is_still_waited_for() {
    let backend = reconnecting_for(Duration::from_secs(2)).await;
    let mut said = 0;
    let start = tokio::time::Instant::now();

    backend
        .close_announcing(Duration::from_millis(500), || said += 1)
        .await;

    assert_eq!(said, 1, "a slow release must say it is releasing, once");
    assert!(
        start.elapsed() >= Duration::from_secs(2),
        "the release was not waited for: {:?}",
        start.elapsed()
    );
}

/// A release that finishes inside the quiet period prints nothing.
#[tokio::test(start_paused = true)]
async fn a_quick_release_says_nothing() {
    let (backend, _gate) = Backend::fake(Duration::from_secs(30)).await;
    let mut said = 0;

    backend
        .close_announcing(Duration::from_millis(500), || said += 1)
        .await;

    assert_eq!(said, 0, "a quick release must be silent");
}

/// Over a daemon there is nothing to release, so nothing is said however long the quiet period.
#[tokio::test]
async fn a_release_over_a_daemon_says_nothing() {
    let daemon = crate::test_support::stub_daemon::stub_daemon().await;
    let backend = Backend::stub_remote(&daemon.socket).await;
    let mut said = 0;

    backend.close_announcing(Duration::ZERO, || said += 1).await;

    assert_eq!(said, 0, "a daemon connection releases nothing");
}

/// A profile with a warm-up readout that depends on the power toggle.
fn warmup_profile() -> Profile {
    let source = "schema = 1\nname = \"t\"\n\
        [[control]]\nid = [4]\nkey = \"warming_up\"\nlabel = \"Warming up\"\nkind = \"readout\"\n\
        value_type = \"bool\"\nonly_while = \"power\"\n\
        [[control]]\nid = [10]\nkey = \"power\"\nlabel = \"Power\"\nkind = \"toggle\"\n";
    svolder_profiles::Registry::from_sources(vec![("t".into(), source.into())])
        .expect("the test profile loads")
        .get("t")
        .expect("the test profile exists")
        .clone()
}

fn value_object(profile: &Profile, id: u8, value: svolder_proto::Value) -> ValueObject {
    let m = svolder_proto::Message::status(FunctionId::one(id), value);
    ValueObject::from_reading(&svolder_profiles::describe(profile, &m), &m)
}

/// The derived value follows the control named, whatever shape the readout's own value takes.
#[test]
fn a_gated_readout_shows_false_while_its_gate_reads_false() {
    use svolder_proto::Value;
    let profile = warmup_profile();
    for (warming, power, shown) in [
        (Value::Bool(true), Value::Bool(true), "true"),
        (Value::U8(1), Value::Bool(true), "true"),
        (Value::Bool(true), Value::Bool(false), "false"),
        (Value::U8(1), Value::Bool(false), "false"),
        (Value::Bool(false), Value::Bool(true), "false"),
        (Value::U8(2), Value::Bool(true), "false"),
    ] {
        let mut values = vec![
            value_object(&profile, 4, warming.clone()),
            value_object(&profile, 10, power.clone()),
        ];
        apply_only_while(&profile, &mut values);
        assert_eq!(values[0].display, shown, "{warming:?} with power {power:?}");
        assert_eq!(
            values[1].display,
            if power.bool_value() { "on" } else { "off" }
        );
    }
}

/// With no reading for the gate, the readout does not show.
#[test]
fn a_gated_readout_without_a_gate_reading_shows_false() {
    let profile = warmup_profile();
    let mut values = vec![value_object(&profile, 4, svolder_proto::Value::Bool(true))];
    apply_only_while(&profile, &mut values);
    assert_eq!(values[0].display, "false");
}

/// Opening on a socket where nothing listens fails and starts nothing: no daemon is spawned
/// (its socket would appear at the path) and no direct session stands in.
#[tokio::test]
async fn opening_on_a_socket_nothing_listens_on_only_connects() {
    let path = std::env::temp_dir().join(format!("svolder-open-on-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let opened = tokio::time::timeout(
        Duration::from_secs(5),
        Backend::open_on(&path, Some("fake")),
    )
    .await
    .expect("a connect to nothing fails promptly");
    assert!(opened.is_err());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!path.exists(), "something bound the socket");
}

/// The setup wizard's check of a code answers for what the device does: a code it refuses is a
/// refusal with the step that follows one, and a code it takes is none.
#[tokio::test]
async fn the_setup_check_of_a_code_reports_a_refusal() {
    let seen = ScanEntry {
        unit: "0102030405060708".into(),
        product_id: 2,
        product: Some("Gungnir 2".into()),
        firmware: 3,
        passphrase: true,
        rssi: None,
    };
    let g = globals("fake", None);
    for accepted in ["1234", "auto"] {
        let outcome = try_connect(&g, &seen, accepted).await;
        assert!(outcome.is_ok(), "{accepted}: {:?}", outcome.err());
    }
    let refused = try_connect(&g, &seen, "9999")
        .await
        .expect_err("the device refuses 9999");
    assert_eq!(refused.code, Kind::Refused.code());
    assert_eq!(refused.next, crate::recovery::next_step(Kind::Refused));
}
