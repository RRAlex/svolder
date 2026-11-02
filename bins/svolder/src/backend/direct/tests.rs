// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_ble::SessionConfig;

use super::test_support::*;
use super::*;

/// The `passphrase` verb's own path over the fake device, and what the setter it ends
/// with does for a session the device has already turned down.
///
/// Function 250 travels under the code the device expects now, so the verb is no way out
/// of a session the device is refusing: it is turned away before it writes anything, and
/// the user has to open with the code the device wants. What the setter does, wherever it
/// is reached from — this verb, or a reload correcting the configured code — is let the
/// next write be sent for the device to judge instead of refusing it here.
#[tokio::test]
async fn the_passphrase_verb_changes_the_code_and_its_setter_frees_a_refused_session() {
    let (mut backend, _gate) = fake_backend().await;
    let right = Passphrase::from_digits([1, 2, 3, 4]).expect("four digits");
    let changed = Passphrase::from_digits([5, 6, 7, 8]).expect("four digits");

    // The verb tells the device its new code and the session the same, so the write
    // after it is carried by the code the device now expects.
    backend
        .passphrase("5678")
        .await
        .expect("the device takes the new code");
    backend
        .set("input", "4", 1, false)
        .await
        .expect("the session followed the device");

    // A code the device does not expect — what a configuration with the wrong digits
    // leaves a session holding — is refused, and the refusal sticks to the session.
    backend
        .session
        .as_mut()
        .expect("a session")
        .set_passphrase(right);
    let refused = backend
        .set("input", "5", 1, false)
        .await
        .expect_err("the device refuses a frame under the wrong code");
    assert!(refused.to_string().contains("passphrase"), "got {refused}");
    let verb = backend
        .passphrase("1234")
        .await
        .expect_err("function 250 travels under the code the device expects");
    assert!(verb.to_string().contains("passphrase"), "got {verb}");

    // Correcting the code is what frees the session: the next write is sent.
    backend
        .session
        .as_mut()
        .expect("a session")
        .set_passphrase(changed);
    backend
        .set("input", "5", 1, false)
        .await
        .expect("the corrected code is tried on the next write");
}

/// Which adapter a session opens on: the one that was asked for, then the one that last
/// connected this device, then the configured one, then whichever the transport finds
/// first.
#[test]
fn the_adapter_asked_for_comes_first_then_the_last_one_then_the_config() {
    assert_eq!(
        preferred_adapter(Some("hci3"), Some("hci0"), Some("hci1".into())),
        Some("hci3".into())
    );
    assert_eq!(
        preferred_adapter(None, Some("hci0"), Some("hci1".into())),
        Some("hci1".into())
    );
    assert_eq!(
        preferred_adapter(None, Some("hci0"), None),
        Some("hci0".into())
    );
    assert_eq!(preferred_adapter(None, None, None), None);
}

/// A device the adapter holds is selected even though the scan window was quiet: a
/// selector names which device to talk to, and the connection that follows is what
/// settles whether it is still there.
#[tokio::test]
async fn a_device_the_adapter_holds_is_selected_when_the_window_is_quiet() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a profile").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let passphrase = Passphrase::from_digits([1, 2, 3, 4]).expect("four digits");
    let gate = Arc::new(FakeGate::new(&profile, unit, passphrase));
    gate.quiet_scans();
    let transport = Arc::clone(&gate) as Arc<dyn Transport>;

    let selected = select_device(
        &transport,
        &registry,
        Some("01020304"),
        Duration::from_millis(10),
    )
    .await
    .expect("the held device is selected");
    assert_eq!(selected.advert.unit_id(), unit);
    assert!(
        !selected.heard,
        "an advertisement the adapter holds was not heard by this window"
    );
}

/// The signal an advertisement carries is stamped as heard now only when a window heard it: a
/// held advertisement's strength is whatever BlueZ kept, of unknown age, so a session opened on
/// one lists the device with no signal at all.
#[tokio::test]
async fn a_session_opened_on_a_held_advertisement_stamps_no_signal() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a profile").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let passphrase = Passphrase::from_digits([1, 2, 3, 4]).expect("four digits");
    let g = Globals {
        transport: "fake",
        adapter: None,
        socket: None,
        device: Some("01020304"),
        verbose: false,
    };
    let config = Config::default();
    let mut signals = Vec::new();
    for quiet in [false, true] {
        let gate = Arc::new(FakeGate::new(&profile, unit, passphrase));
        if quiet {
            gate.quiet_scans();
        }
        let transport = Arc::clone(&gate) as Arc<dyn Transport>;
        let mut opened = open_over(&g, &config, transport, true, Wire::Fake, None)
            .await
            .expect("the device connects");
        signals.push(opened.own_row().and_then(|row| row.signal));
        opened.close().await;
    }
    assert!(
        signals[0]
            .is_some_and(|signal| signal.dbm == -50 && signal.heard_at_ms > 1_000_000_000_000),
        "a heard advertisement is stamped: {signals:?}"
    );
    assert_eq!(signals[1], None, "a held one is not");
}

/// `status -d <unit id>`'s radio path is one scan, and that scan ends the moment the device
/// is heard; a prefix of the unit id still waits the window out, since it has to see every
/// device it could name. Either way the connect that follows is not another scan.
#[tokio::test(start_paused = true)]
async fn a_full_unit_id_is_reached_with_one_scan_that_stops_at_the_device() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a profile").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let passphrase = Passphrase::from_digits([1, 2, 3, 4]).expect("four digits");
    for (selector, waits_the_window) in [("0102030405060708", false), ("01020304", true)] {
        let gate = Arc::new(FakeGate::new(&profile, unit, passphrase));
        gate.takes_windows();
        let transport = Arc::clone(&gate) as Arc<dyn Transport>;

        let started = tokio::time::Instant::now();
        let (mut session, ..) = reach(&transport, &registry, None, Some(selector))
            .await
            .expect("the device is reached");
        let took = started.elapsed();

        assert_eq!(gate.listens(), 1, "{selector}: not one scan");
        assert!(session.is_connected(), "{selector}");
        assert_eq!(
            took >= crate::scanning::WINDOW,
            waits_the_window,
            "{selector} took {took:?}"
        );
        session.close().await;
    }
}

/// How long this backend's session gives a running reconnect attempt when it closes,
/// read while the session is still in hand: an attempt carries it off with it.
fn close_grace(backend: &DirectBackend) -> Duration {
    backend
        .session
        .as_ref()
        .expect("a connected backend")
        .config()
        .close_grace
}

/// A dropped link is reported the moment it drops, and the recovery on the call after
/// it: the same two updates the daemon publishes, in the same order, so `watch` prints
/// the same markers whichever backend it is running on.
///
/// The gate is closed over the drop, so the first update cannot be coming from the far
/// side of a reconnect; it is opened again before the second call, which is the one that
/// reconnects. Statuses flow once the link is back.
#[tokio::test]
async fn a_severed_link_reports_the_disconnect_then_the_reconnect() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (mut backend, gate) = fake_backend().await;
        gate.refuse_connects();
        gate.sever();

        let down = backend.next_status().await.expect("an update");
        assert!(
            matches!(down, Some(Update::Connection { connected: false })),
            "got {down:?}"
        );

        gate.allow_connects();
        let up = backend.next_status().await.expect("an update");
        assert!(
            matches!(up, Some(Update::Connection { connected: true })),
            "got {up:?}"
        );

        // The link is live again: the reconnect asked for the firmware versions again, and the
        // echo of a write comes back as a reading after them.
        for id in ["210.2.1", "210.2.2"] {
            let heard = backend.next_status().await.expect("an update");
            assert!(
                matches!(&heard, Some(Update::Status(vo)) if vo.id == id),
                "got {heard:?}"
            );
        }
        backend
            .set("mute", "on", 1, true)
            .await
            .expect("the write goes through the fresh link");
        let status = backend.next_status().await.expect("an update");
        assert!(
            matches!(&status, Some(Update::Status(vo)) if vo.key.as_deref() == Some("mute")),
            "got {status:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A reconnect a caller stops waiting for is not a reconnect abandoned.
///
/// The attempt runs in a task of its own, so a front end whose `select!` woke on a tick
/// drops this future without touching it, and finds the same attempt still running on its
/// next call. Every wait below is cut short while the device is out of reach; the one
/// after the gate opens is the one that hears the link is back.
#[tokio::test(start_paused = true)]
async fn a_reconnect_survives_a_cancelled_next_status() {
    let (mut backend, gate) = fake_backend_with(Duration::from_secs(30)).await;
    gate.refuse_connects();
    gate.sever();

    let down = backend.next_status().await.expect("an update");
    assert!(
        matches!(down, Some(Update::Connection { connected: false })),
        "got {down:?}"
    );

    for _ in 0..8 {
        let cut_short =
            tokio::time::timeout(Duration::from_millis(250), backend.next_status()).await;
        assert!(
            cut_short.is_err(),
            "the attempt answered while the device was out of reach"
        );
    }

    gate.allow_connects();
    let up = tokio::time::timeout(Duration::from_secs(5), backend.next_status())
        .await
        .expect("the reconnect answered")
        .expect("an update");
    assert!(
        matches!(up, Some(Update::Connection { connected: true })),
        "got {up:?}"
    );
}

/// Closing during a reconnect waits for the attempt to finish rather than stopping it,
/// and the device ends up released: quitting mid-connect leaves no link nobody owns.
///
/// The connect below succeeds well inside the grace, so the wait is what the connect
/// costs and the gate records the disconnect. Which of the two closes issued it — the
/// attempt's own, or the one the backend runs on the session it takes back — is what
/// [`a_cancelled_reconnect_task_closes_the_session_it_hands_back`] tells apart.
#[tokio::test(start_paused = true)]
async fn closing_during_a_reconnect_waits_for_the_attempt_and_closes_the_session() {
    let connect_time = Duration::from_secs(2);
    let (mut backend, gate) = fake_backend_with(Duration::from_secs(30)).await;
    let grace = close_grace(&backend);
    gate.slow_connects(connect_time);
    gate.sever();
    backend.next_status().await.expect("an update");
    assert!(!gate.closed(), "nothing has been disconnected yet");

    let start = tokio::time::Instant::now();
    backend.close().await;
    let waited = start.elapsed();
    assert!(
        waited >= connect_time,
        "close stopped the attempt instead of waiting for it: {waited:?}"
    );
    assert!(
        waited < grace,
        "close waited {waited:?}, past the grace it gives an attempt"
    );
    assert!(
        gate.closed(),
        "the link the attempt opened was left connected"
    );
}

/// The reconnect task itself closes a session it was carrying when the cancel flag went
/// up, so the link is released even by the paths that never close a session.
///
/// The flag is raised while the attempt is still inside its connect, and the session is
/// then taken back through [`DirectBackend::await_reconnect`], which only takes a session
/// back and never closes one. The gate records the disconnect all the same, so the close
/// can only have come from inside the task.
#[tokio::test(start_paused = true)]
async fn a_cancelled_reconnect_task_closes_the_session_it_hands_back() {
    let (mut backend, gate) = fake_backend_with(Duration::from_secs(30)).await;
    gate.slow_connects(Duration::from_secs(2));
    gate.sever();
    backend.next_status().await.expect("an update");

    backend
        .reconnecting
        .as_ref()
        .expect("a running attempt")
        .cancel
        .store(true, Ordering::SeqCst);
    backend
        .await_reconnect()
        .await
        .expect("the attempt finished")
        .expect("an update");

    assert!(
        gate.closed(),
        "the task handed the session back with the link it opened still up"
    );
    assert!(
        backend
            .session
            .as_ref()
            .is_some_and(|session| !session.is_connected()),
        "the session came back still holding a link"
    );
}

/// A reconnect that outlasts the grace is stopped rather than sat out: quitting a front
/// end mid-reconnect must not wait out the whole connect deadline.
///
/// The device below takes ten seconds to answer a connect and then refuses it, for the
/// thirty seconds the attempt is given, so the wait can only end one way; what this
/// measures is which of the two bounds ended it.
#[tokio::test(start_paused = true)]
async fn closing_during_a_slow_reconnect_stops_it_after_the_grace() {
    let (mut backend, gate) = fake_backend_with(Duration::from_secs(30)).await;
    let grace = close_grace(&backend);
    gate.slow_connects(Duration::from_secs(10));
    gate.refuse_connects();
    gate.sever();
    backend.next_status().await.expect("an update");

    let start = tokio::time::Instant::now();
    backend.close().await;
    let waited = start.elapsed();
    assert!(
        waited >= grace,
        "close did not wait for the attempt at all: {waited:?}"
    );
    assert!(
        waited < grace * 2,
        "close waited {waited:?}, which is the connect deadline and not the grace"
    );
    assert!(
        !gate.closed(),
        "the attempt got as far as a link, so this measured the wrong thing"
    );
    // Stopping the attempt hands the link its connect may still be opening to the
    // transport, which is asked to take it back down before the process leaves.
    assert_eq!(
        gate.settles(),
        1,
        "the stopped attempt left the transport unasked"
    );
    assert_eq!(
        gate.settled(),
        ["0102030405060708".parse::<UnitId>().unwrap()],
        "the close settled other devices' work, or none"
    );
    assert_eq!(
        gate.live_at_settle(),
        Some(0),
        "the close settled before the aborted attempt was dropped"
    );
}

/// A write while a reconnect attempt holds the session says so, and the readings that
/// attempt carried off still answer a read.
#[tokio::test(start_paused = true)]
async fn a_write_while_the_link_reconnects_is_refused_and_the_readings_still_answer() {
    let (mut backend, gate) = fake_backend_with(Duration::from_secs(30)).await;
    gate.refuse_connects();
    gate.sever();
    backend.next_status().await.expect("an update");

    for write in ["a literal value", "a step from the last one"] {
        let refusal = match write {
            "a literal value" => backend.set("mute", "on", 1, true).await,
            _ => backend.set("volume", "up", 1, true).await,
        }
        .expect_err("there is no link to write through");
        assert!(
            refusal.to_string().contains("reconnecting"),
            "{write}: {refusal}"
        );
    }
    assert!(
        backend.snapshot().is_err(),
        "a snapshot during a reconnect reports a read that never reached the device"
    );
    assert!(
        !backend.state().is_empty(),
        "the renderer still has the last known values to draw"
    );
}

/// A device that never comes back is still reported as gone: the disconnect comes first,
/// and the call after it is the one that spends the connect deadline and gives up.
#[tokio::test]
async fn a_device_that_does_not_come_back_reports_the_disconnect_then_gives_up() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (mut backend, gate) = fake_backend().await;
        gate.refuse_connects();
        gate.sever();

        let down = backend.next_status().await.expect("an update");
        assert!(
            matches!(down, Some(Update::Connection { connected: false })),
            "got {down:?}"
        );
        let given_up = backend.next_status().await.expect("an update");
        assert!(given_up.is_none(), "got {given_up:?}");
    })
    .await
    .expect("test timed out");
}

/// A `-d` that is a configured device's unit id, or a prefix of it, names that device, so
/// its configured name is what a backend opened on it answers.
#[test]
fn a_unit_id_or_a_prefix_of_it_selects_the_configured_device() {
    let config = Config::parse(
        "schema = 1\n\
         [[device]]\nname = \"desk\"\nudid = \"0102030405060708\"\n\
         [[device]]\nname = \"amp\"\nudid = \"0102aaaaaaaaaaaa\"\n",
    )
    .expect("a valid configuration");
    let name = |selector: &str| {
        choose_configured(&config, Some(selector)).map(|device| device.name.as_str())
    };
    assert_eq!(name("desk"), Some("desk"), "the name still selects");
    assert_eq!(name("0102030405060708"), Some("desk"), "a whole unit id");
    assert_eq!(name("010203"), Some("desk"), "a prefix only one device has");
    assert_eq!(name("0102AAAA"), Some("amp"), "case aside");
    assert_eq!(
        name("0102"),
        None,
        "a prefix two devices share names neither"
    );
    assert_eq!(name(""), None, "an empty selector names nothing");
    assert_eq!(name("ffff"), None, "a unit id nothing is configured for");
}

/// A backend over the fake device, whose code is `1234`, holding a session keyed from `entry`
/// and an advertisement that says `shown` (`None` for the device's own, and `Some("4321")` for
/// one older than its code), as `open` leaves one after its connect. `generic` reads the device
/// through a profile that names no function, so the connect's refresh proves nothing.
async fn judged_backend(
    entry: &str,
    shown: Option<&str>,
    generic: bool,
) -> (DirectBackend, Arc<FakeGate>) {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = match generic {
        true => Registry::generic(0),
        false => registry.get("gungnir2-v2").expect("a profile").clone(),
    };
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let device = Passphrase::from_digits([1, 2, 3, 4]).expect("four digits");
    let gate = Arc::new(FakeGate::new(&profile, unit, device));
    let mut advert = gate.advertisement();
    if let Some(code) = shown {
        advert.manufacturer.encrypted_passphrase = Passphrase::encrypt(
            Passphrase::from_text(code).expect("four digits").encoded(),
            Passphrase::advert_key(unit),
        );
    }
    let keyed = match entry {
        "auto" => advert.manufacturer.passphrase(),
        text => Passphrase::from_text(text).expect("an entry"),
    };
    let config = SessionConfig {
        write_spacing: Duration::from_millis(1),
        firmware_delay: Duration::from_millis(1),
        drain_idle: Duration::from_millis(10),
        ..SessionConfig::default()
    };
    let mut session = Session::with_config(
        Arc::clone(&gate) as Arc<dyn Transport>,
        advert,
        profile.clone(),
        keyed,
        config,
    );
    session.connect().await.expect("the fake device connects");
    let mut backend = DirectBackend {
        transport: Arc::clone(&gate) as Arc<dyn Transport>,
        registry,
        session: Some(session),
        profile,
        configured: Some("fake".into()),
        passphrase: entry.into(),
        note: None,
        entry_waits: false,
        episode: Episode::default(),
        reconnecting: None,
        last_state: DeviceState::default(),
        sent: HashMap::new(),
        warnings: Vec::new(),
        heard_ms: None,
    };
    // The firmware answers a link that was not refused brings come before anything a case sends.
    let asked = match backend.session.as_ref().is_some_and(|s| s.refused()) {
        true => 0,
        false => backend.profile.firmware_subsystems.len(),
    };
    for _ in 0..asked {
        let heard = backend.next_status().await.expect("an update");
        assert!(
            matches!(&heard, Some(Update::Status(vo)) if vo.id.starts_with("210.2.")),
            "the connect's firmware answers come first: {heard:?}"
        );
    }
    backend.judge_passphrase();
    (backend, gate)
}

/// A configured code the advertisement does not match, which the device refuses, earns a note
/// and changes nothing the backend does; `auto` never does; and the line the verb prints names
/// the device and the fix and carries no digit.
#[tokio::test]
async fn a_configured_code_that_differs_earns_a_note_and_auto_never_does() {
    let backend = judged_backend("9999", None, false).await.0;
    assert_eq!(backend.passphrase_note(), Some(PassphraseNote::Differs));
    let line = crate::backend::Backend::Direct(Box::new(backend))
        .passphrase_note_line()
        .await
        .expect("a line");
    assert!(line.starts_with("fake: "), "{line}");
    assert!(line.contains("`auto`"), "{line}");
    assert!(!line.chars().any(|c| c.is_ascii_digit()), "{line}");

    for shown in [None, Some("4321"), Some("none")] {
        let auto = judged_backend("auto", shown, false).await.0;
        assert_eq!(auto.passphrase_note(), None, "auto against {shown:?}");
    }
    let none = judged_backend("none", None, false).await.0;
    assert_eq!(none.passphrase_note(), Some(PassphraseNote::NoneConfigured));
    let digits = judged_backend("9999", Some("none"), false).await.0;
    assert_eq!(
        digits.passphrase_note(),
        Some(PassphraseNote::NoCodeAdvertised)
    );
}

/// A device that answered the connect's refresh in full has shown the code works, so an
/// advertisement older than its code leaves no note.
#[tokio::test]
async fn the_connect_burst_clears_a_note_an_old_advertisement_would_earn() {
    let proven = judged_backend("1234", Some("4321"), false).await.0;
    assert_eq!(proven.passphrase_note(), None);
}

/// The echo of a waited `set` takes the note away where the connect's refresh proved nothing,
/// and a `set` that does not wait leaves it: no echo has been read.
#[tokio::test]
async fn the_echo_of_a_waited_set_clears_the_note() {
    let (mut backend, _gate) = judged_backend("1234", Some("4321"), true).await;
    assert_eq!(backend.passphrase_note(), Some(PassphraseNote::Differs));
    backend
        .raw("10", "u8", "90", true)
        .await
        .expect("a write that waits for nothing");
    assert_eq!(backend.passphrase_note(), Some(PassphraseNote::Differs));
    backend
        .raw("10", "u8", "91", false)
        .await
        .expect("the device echoes");
    assert_eq!(backend.passphrase_note(), None);
}

/// A reconnect judges again: the refresh it takes is what shows the code works, so a note held
/// from before the link dropped is gone once the device has answered it in full.
#[tokio::test]
async fn a_reconnect_that_the_device_answers_clears_the_note() {
    let (mut backend, gate) = judged_backend("1234", Some("4321"), false).await;
    // What an earlier link that proved nothing would have left behind.
    backend.note = Some(PassphraseNote::Differs);
    gate.sever();
    assert!(matches!(
        backend.next_status().await,
        Ok(Some(Update::Connection { connected: false }))
    ));
    assert!(matches!(
        backend.next_status().await,
        Ok(Some(Update::Connection { connected: true }))
    ));
    assert_eq!(backend.passphrase_note(), None);
}

/// A refusal that arrives after the connect's burst showed the code works ends the proof, and
/// the backend judges after the status it reads: the verdict the old advertisement earns is back.
#[tokio::test]
async fn a_refusal_after_a_proof_brings_the_note_back() {
    let (mut backend, gate) = judged_backend("1234", Some("4321"), false).await;
    assert_eq!(backend.passphrase_note(), None);
    gate.push(&[Message::status(
        svolder_proto::function::PASSPHRASE_REJECTED,
        Value::Bool(true),
    )]);
    assert!(matches!(
        backend.next_status().await,
        Ok(Some(Update::Status(_)))
    ));
    assert_eq!(backend.passphrase_note(), Some(PassphraseNote::Differs));
}

/// A direct session takes the entry saved for its own device in, as a daemon does: an entry that
/// means the code the session proved leaves no note, however old the advertisement, and a
/// corrected entry re-keys a refused session and clears the note.
#[tokio::test]
async fn a_saved_entry_is_taken_in_by_a_direct_session_like_a_daemon_takes_it() {
    let entry = |text: &str| DeviceConfig {
        name: "fake".into(),
        udid: "0102030405060708".into(),
        profile: None,
        passphrase: text.into(),
        inputs: Default::default(),
    };
    // Proven on 1234 under an advertisement older than the code: saving the same entry again
    // says nothing.
    let (backend, _gate) = judged_backend("1234", Some("4321"), false).await;
    let mut whole = crate::backend::Backend::Direct(Box::new(backend));
    assert_eq!(whole.passphrase_note_for(&entry("1234")).await, None);

    // Refused on 9999; the corrected entry re-keys the session, clears the note, and the next
    // write goes through.
    let (mut backend, _gate) = judged_backend("9999", None, false).await;
    assert_eq!(backend.passphrase_note(), Some(PassphraseNote::Differs));
    backend.adopt_entry("auto");
    assert_eq!(backend.passphrase_note(), None);
    backend
        .raw("10", "u8", "90", false)
        .await
        .expect("the corrected code is tried on the next write");

    // An entry saved for another device is not this session's to judge.
    let (backend, _gate) = judged_backend("9999", None, false).await;
    let mut whole = crate::backend::Backend::Direct(Box::new(backend));
    let other = DeviceConfig {
        name: "other".into(),
        ..entry("9999")
    };
    assert_eq!(whole.passphrase_note_for(&other).await, None);

    // A changed code in the file that the device does not hold earns the note.
    let (backend, _gate) = judged_backend("1234", None, false).await;
    let mut whole = crate::backend::Backend::Direct(Box::new(backend));
    assert_eq!(
        whole.passphrase_note_for(&entry("9999")).await,
        Some(PassphraseNote::Differs)
    );
}

/// After the passphrase verb the session's copy of the advertisement carries the new code, so
/// what `auto` means and the verdict agree with what the device now holds.
#[tokio::test]
async fn the_passphrase_verb_updates_the_advertisement_the_session_holds() {
    let (mut backend, _gate) = judged_backend("auto", None, false).await;
    backend
        .passphrase("5678")
        .await
        .expect("the device takes it");
    let held = backend
        .session
        .as_ref()
        .expect("a session")
        .advertisement()
        .manufacturer
        .passphrase();
    assert_eq!(held.digits(), [5, 6, 7, 8]);
}

/// A waited `set` that the device refuses ends a proof too: the backend judges on the error, so
/// the verdict the old advertisement earns is back.
#[tokio::test]
async fn a_refused_waited_set_brings_the_note_back() {
    let (mut backend, gate) = judged_backend("1234", Some("4321"), false).await;
    assert_eq!(backend.passphrase_note(), None);
    gate.push(&[Message::status(
        svolder_proto::function::PASSPHRASE_REJECTED,
        Value::Bool(true),
    )]);
    let refused = backend.raw("10", "u8", "90", false).await;
    assert!(refused.is_err(), "the device refused the set");
    assert_eq!(backend.passphrase_note(), Some(PassphraseNote::Differs));
}

/// An entry saved while a reconnect attempt holds the session is kept and applied when the
/// session is back, so the reconnected session runs on the corrected code.
#[tokio::test]
async fn an_entry_saved_during_a_reconnect_is_applied_when_the_session_is_back() {
    let (mut backend, gate) = judged_backend("9999", None, false).await;
    assert_eq!(backend.passphrase_note(), Some(PassphraseNote::Differs));
    gate.sever();
    assert!(matches!(
        backend.next_status().await,
        Ok(Some(Update::Connection { connected: false }))
    ));
    assert!(backend.session.is_none(), "a reconnect holds the session");
    backend.adopt_entry("auto");
    assert!(matches!(
        backend.next_status().await,
        Ok(Some(Update::Connection { connected: true }))
    ));
    assert_eq!(backend.passphrase_note(), None);
    backend
        .raw("10", "u8", "90", false)
        .await
        .expect("the session came back on the corrected code");
}

mod refusal;

/// A step from a reading the session has not had is refused with code 4 and the inspect step;
/// with no session at all, it is the link that is missing, code 3.
#[tokio::test]
async fn a_step_with_no_reading_to_step_from_is_refused_with_code_4() {
    let (mut backend, gate) = fake_backend().await;
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("a built-in profile")
        .clone();
    // A session that has not connected holds no reading at all.
    backend.session = Some(Session::new(
        Arc::clone(&gate) as Arc<dyn Transport>,
        gate.advertisement(),
        profile,
        Passphrase::from_digits([1, 2, 3, 4]).expect("four digits"),
    ));
    for value in ["up", "down"] {
        match backend.set("volume", value, 1, false).await {
            Err(WriteError::Refused {
                code: 4,
                message,
                next,
            }) => {
                assert_eq!(message, "no current value for volume");
                assert_eq!(next, crate::recovery::INSPECT);
            }
            other => panic!("{value}: got {other:?}"),
        }
    }
    backend.session = None;
    match backend.set("volume", "up", 1, false).await {
        Err(WriteError::Refused {
            code: 3, message, ..
        }) => {
            assert_eq!(message, "no session was opened for this verb");
        }
        other => panic!("got {other:?}"),
    }
}

#[test]
fn the_adapter_named_for_a_session_wins_over_the_one_the_transport_reports() {
    assert_eq!(
        adapter_taken(Some("hci1"), Some("hci0")),
        Some("hci1".to_string())
    );
}

#[test]
fn the_transports_adapter_is_taken_when_none_was_named() {
    assert_eq!(adapter_taken(None, Some("hci2")), Some("hci2".to_string()));
}

#[test]
fn no_adapter_is_recorded_when_none_was_named_or_reported() {
    assert_eq!(adapter_taken(None, None), None);
}

/// A step after a lowering sent without waiting starts from the lowering, not from the level the
/// session still shows before the lowering's echo: a `down` is one below it and an `up` one
/// above it, never a step from the old, louder level.
#[tokio::test]
async fn a_step_after_a_lowering_sent_without_waiting_starts_from_the_lowering() {
    for (way, expected) in [("down", 49), ("up", 51)] {
        let (mut backend, _gate) = fake_backend().await;
        backend
            .set("volume", "120", 1, false)
            .await
            .expect("the device echoes");
        backend
            .set("volume", "50", 1, true)
            .await
            .expect("a write that waits for nothing");
        backend
            .set("volume", way, 1, true)
            .await
            .expect("a write that waits for nothing");
        // The device echoes the lowering, then the step.
        let mut volume = Vec::new();
        while volume.len() < 2 {
            let status = tokio::time::timeout(
                Duration::from_secs(2),
                backend.session.as_mut().expect("a session").next_status(),
            )
            .await
            .expect("an echo in time")
            .expect("the link holds")
            .expect("a status");
            if status.id.as_slice() == [10] {
                volume.push(status.value.as_i64());
            }
        }
        assert_eq!(volume, [Some(50), Some(expected)], "{way}");
    }
}

/// Two quick steps after a lowering sent without waiting never undo it: the second goes on from
/// the first, so the two move two steps, never louder than the lowering plus two.
#[tokio::test]
async fn two_quick_steps_after_a_lowering_sent_without_waiting_never_undo_it() {
    for (first, second, expected) in [
        ("down", "down", [50, 49, 48]),
        ("up", "up", [50, 51, 52]),
        ("down", "up", [50, 49, 50]),
        ("up", "down", [50, 51, 50]),
    ] {
        let (mut backend, _gate) = fake_backend().await;
        backend
            .set("volume", "120", 1, false)
            .await
            .expect("the device echoes");
        for way in ["50", first, second] {
            backend
                .set("volume", way, 1, true)
                .await
                .expect("a write that waits for nothing");
        }
        let mut volume = Vec::new();
        while volume.len() < 3 {
            let status = tokio::time::timeout(
                Duration::from_secs(2),
                backend.session.as_mut().expect("a session").next_status(),
            )
            .await
            .expect("an echo in time")
            .expect("the link holds")
            .expect("a status");
            if status.id.as_slice() == [10] {
                volume.push(status.value.as_i64().expect("a number"));
            }
        }
        assert_eq!(volume, expected, "{first}, {second}");
    }
}

/// The volume echoes the device sends next, `count` of them.
async fn volume_echoes(backend: &mut DirectBackend, count: usize) -> Vec<i64> {
    let mut volume = Vec::new();
    while volume.len() < count {
        let status = tokio::time::timeout(
            Duration::from_secs(2),
            backend.session.as_mut().expect("a session").next_status(),
        )
        .await
        .expect("an echo in time")
        .expect("the link holds")
        .expect("a status");
        if status.id.as_slice() == [10] {
            volume.push(status.value.as_i64().expect("a number"));
        }
    }
    volume
}

/// Quick steps before any echo chain: each goes on from the step before it, held at two steps
/// above the reading, as the daemon's writer chains them.
#[tokio::test]
async fn two_quick_steps_on_a_direct_session_move_two_steps() {
    for (steps, expected) in [(2, vec![64, 65]), (4, vec![64, 65, 66, 66])] {
        let (mut backend, _gate) = fake_backend().await;
        backend
            .set("volume", "63", 1, false)
            .await
            .expect("the device echoes");
        for _ in 0..steps {
            backend
                .set("volume", "up", 1, true)
                .await
                .expect("a write that waits for nothing");
        }
        assert_eq!(
            volume_echoes(&mut backend, steps).await,
            expected,
            "{steps}"
        );
    }
}

/// A waited lowering whose echo never comes may have landed: a step after it goes on from the
/// lowering, not from the louder reading the session still shows.
#[tokio::test]
async fn a_step_after_a_waited_lowering_that_timed_out_steps_from_the_lowering() {
    let (mut backend, gate) = fake_backend().await;
    backend
        .set("volume", "120", 1, false)
        .await
        .expect("the device echoes");
    gate.hear_nothing(true);
    match backend.set("volume", "50", 1, false).await {
        Err(WriteError::Ble(BleError::Timeout(_))) => {}
        other => panic!("got {other:?}"),
    }
    gate.hear_nothing(false);
    backend
        .set("volume", "down", 1, true)
        .await
        .expect("a write that waits for nothing");
    assert_eq!(volume_echoes(&mut backend, 1).await, [49]);
}

/// A write that is turned away leaves the step sent before it alone: a lowering still on its way
/// is stepped from after a refused write, never undone by working from the reading it has not
/// changed yet.
#[tokio::test]
async fn a_step_after_a_refused_write_goes_on_from_the_lowering_before_it() {
    for (way, expected) in [("up", 51), ("down", 49)] {
        let (mut backend, _gate) = fake_backend().await;
        backend
            .set("volume", "120", 1, false)
            .await
            .expect("the device echoes");
        backend
            .set("volume", "50", 1, true)
            .await
            .expect("a write that waits for nothing");
        assert!(backend.set("volume", "200", 1, true).await.is_err());
        backend
            .set("volume", way, 1, true)
            .await
            .expect("a write that waits for nothing");
        assert_eq!(
            volume_echoes(&mut backend, 2).await,
            [50, expected],
            "{way}"
        );
    }
}

/// The volume statuses `count` reads of the session bring, read as the TUI loop reads them.
async fn read_volume(backend: &mut DirectBackend, count: usize) -> Vec<i64> {
    let mut volume = Vec::new();
    while volume.len() < count {
        let update = tokio::time::timeout(Duration::from_secs(2), backend.next_status())
            .await
            .expect("a status in time")
            .expect("the link holds")
            .expect("an update");
        if let Update::Status(status) = update
            && status.key.as_deref() == Some("volume")
        {
            volume.push(status.raw.as_i64().expect("a number"));
        }
    }
    volume
}

/// A step after the reading showed what was sent, and the knob turned since, goes on from the
/// knob: the note of a step that was read back or answered is gone, so it is never stepped from
/// a level above the knob's.
#[tokio::test]
async fn a_step_after_its_note_was_read_back_and_the_knob_turned_steps_from_the_knob() {
    // A waited write that was answered, then the knob.
    let (mut backend, gate) = fake_backend().await;
    backend
        .set("volume", "65", 1, false)
        .await
        .expect("an echo");
    gate.turn_knob(63);
    assert_eq!(read_volume(&mut backend, 1).await, [63]);
    backend.set("volume", "up", 1, true).await.expect("a step");
    assert_eq!(read_volume(&mut backend, 1).await, [64]);

    // Two quick steps whose echoes were both read, then the knob.
    for (start, knob, expected) in [(63u8, 40u8, 41i64), (93, 93, 94)] {
        let (mut backend, gate) = fake_backend().await;
        backend
            .set("volume", &start.to_string(), 1, false)
            .await
            .expect("an echo");
        for _ in 0..2 {
            backend.set("volume", "up", 1, true).await.expect("a step");
        }
        assert_eq!(
            read_volume(&mut backend, 2).await,
            [i64::from(start) + 1, i64::from(start) + 2]
        );
        gate.turn_knob(knob);
        assert_eq!(read_volume(&mut backend, 1).await, [i64::from(knob)]);
        backend.set("volume", "up", 1, true).await.expect("a step");
        assert_eq!(read_volume(&mut backend, 1).await, [expected], "{start}");
    }
}

/// A write that fails on a lost link never reached the device, so the lowering sent before it is
/// still the step a relative step starts from.
#[tokio::test]
async fn a_write_that_fails_on_a_lost_link_leaves_the_lowering_before_it_noted() {
    let (mut backend, gate) = fake_backend().await;
    backend
        .set("volume", "120", 1, false)
        .await
        .expect("the device echoes");
    backend
        .set("volume", "50", 1, true)
        .await
        .expect("a write that waits for nothing");
    gate.sever();
    assert!(backend.set("volume", "60", 1, true).await.is_err());
    let noted = backend.sent.get(&FunctionId::one(10)).map(|&(raw, ..)| raw);
    assert_eq!(noted, Some(50));
}

/// `open_over` takes the transport it is handed: with `connect` it selects the device on it and
/// holds a session, without it only holds the transport, as `open` does over its own.
#[tokio::test]
async fn open_over_a_given_transport_connects_or_only_holds_it() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a built-in profile");
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let passphrase = Passphrase::from_digits([1, 2, 3, 4]).expect("four digits");
    let g = Globals {
        transport: "fake",
        adapter: None,
        socket: None,
        device: None,
        verbose: false,
    };
    let config = Config::default();

    let transport: Arc<dyn Transport> = Arc::new(svolder_ble::FakeTransport::from_profile(
        profile, unit, 3, passphrase,
    ));
    let mut connected = open_over(&g, &config, Arc::clone(&transport), true, Wire::Fake, None)
        .await
        .expect("the fake device connects");
    assert!(connected.session.is_some());
    assert_eq!(connected.profile.name, "gungnir2-v2");
    connected.close().await;

    let held = open_over(&g, &config, transport, false, Wire::Fake, None)
        .await
        .expect("a transport is held without connecting");
    assert!(held.session.is_none());
}

/// A direct session lists its own device from the advertisement it opened on: the product and
/// its id, the profile, and the signal strength as an advert stamped when the session opened;
/// the fake's advertisement carries -50 dBm, and a stronger or weaker one replaces it.
#[tokio::test]
async fn a_direct_session_lists_its_own_device_from_its_advertisement() {
    let (mut backend, _gate) = fake_backend().await;
    let row = backend.own_row().expect("a session lists its device");
    assert_eq!(row.profile.as_deref(), Some(backend.profile.name.as_str()));
    assert!(row.product_id.is_some() && row.product.is_some(), "{row:?}");
    assert!(!row.refused && !row.stale);
    assert_eq!(
        row.connected, None,
        "a session says nothing of the link here"
    );
    assert_eq!(
        row.signal.map(|signal| (signal.dbm, signal.source)),
        Some((-50, SignalSource::Advert))
    );

    backend.heard_ms = Some(1234);
    let mut advert = backend
        .session
        .as_ref()
        .expect("a session")
        .advertisement()
        .clone();
    advert.rssi = Some(-71);
    backend
        .session
        .as_mut()
        .expect("a session")
        .set_advertisement(advert);
    assert_eq!(
        backend.own_row().and_then(|row| row.signal),
        Some(Signal {
            dbm: -71,
            source: SignalSource::Advert,
            heard_at_ms: 1234
        })
    );
}

/// A reconnect that listens and takes a fresh advertisement restamps the signal with the moment
/// it was heard, not the moment the session first opened.
#[tokio::test]
async fn a_reconnect_that_hears_the_device_again_restamps_its_signal() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (mut backend, gate) = fake_backend().await;
        backend.heard_ms = Some(1234);
        gate.sever();
        let down = backend.next_status().await.expect("an update");
        assert!(matches!(
            down,
            Some(Update::Connection { connected: false })
        ));
        let up = backend.next_status().await.expect("an update");
        assert!(matches!(up, Some(Update::Connection { connected: true })));
        let stamp = backend
            .own_row()
            .and_then(|row| row.signal)
            .map(|s| s.heard_at_ms);
        assert!(
            stamp.is_some_and(|ms| ms > 1_000_000_000_000),
            "the signal kept its old stamp: {stamp:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A direct session asks for the firmware versions after its connect like a daemon does: the
/// answers come out of `next_status` as readings of `210.2.<subsystem>`, and the session's
/// state holds them, so a front end with no daemon shows them too.
#[tokio::test]
async fn a_direct_session_hears_the_firmware_versions_it_asked_for() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let profile = Registry::builtin()
            .expect("the built-in profiles load")
            .get("gungnir2-v2")
            .expect("a built-in profile")
            .clone();
        let (mut backend, _gate) =
            fake_backend_unread_over(profile, Duration::from_millis(50)).await;
        for id in ["210.2.1", "210.2.2"] {
            let heard = backend.next_status().await.expect("an update");
            assert!(
                matches!(&heard, Some(Update::Status(vo)) if vo.id == id && vo.raw == 3),
                "got {heard:?}"
            );
        }
    })
    .await
    .expect("test timed out");
}

/// A control that addresses function 250, put on the profile past the loader (which would refuse
/// it), is turned away by the backend's own send in the passphrase verb's words, whichever verb
/// built the frame; the device is not touched, and an ordinary control still sets.
#[tokio::test(start_paused = true)]
async fn a_control_on_the_passphrase_function_is_refused_by_the_direct_send() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let mut profile = registry.get("gungnir2-v2").expect("a profile").clone();
    profile.controls.push(svolder_profiles::Control {
        id: PASSPHRASE,
        key: "danger".into(),
        label: "Danger".into(),
        kind: svolder_profiles::ControlKind::Select,
        value_type: ValueType::U8,
        values: Vec::new(),
        range: None,
        center: None,
        unit: None,
        convert: None,
        channel: None,
        floor: None,
        tick: None,
        log_scale: None,
        advice: None,
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    });
    let (mut backend, _gate) = fake_backend_over(profile, Duration::from_secs(5)).await;
    let refused = backend
        .set("danger", "1", 1, false)
        .await
        .expect_err("function 250 is never sent by a profile key");
    assert!(
        refused
            .to_string()
            .contains("function 250 changes the passphrase"),
        "got {refused}"
    );
    backend
        .set("input", "4", 1, false)
        .await
        .expect("an ordinary control still sets");
}

/// A recall or a save has no echo: sent waiting, it answers as the daemon's `Ack` does, with no
/// reading, and says nothing of a value the device never reports. The same goes for the raw
/// command on the action's id.
#[tokio::test]
async fn a_direct_set_of_an_action_waits_for_no_echo() {
    let (mut backend, gate) = fake_backend_on("lokimax", Duration::from_secs(5)).await;
    gate.hear_nothing(true);
    let set = backend.set("recall", "2", 1, false).await;
    assert!(matches!(set, Ok(None)), "a recall has no reading: {set:?}");
    let by_name = backend.set("save", "Preset 3", 1, false).await;
    assert!(
        matches!(by_name, Ok(None)),
        "a slot is named either way: {by_name:?}"
    );
    let raw = backend.raw("7.2.2", "data", "", false).await;
    assert!(
        matches!(raw, Ok(None)),
        "the action's id sent raw has no reading either: {raw:?}"
    );
    let bad = backend
        .set("recall", "4", 1, false)
        .await
        .expect_err("the third slot is the last");
    assert!(bad.to_string().contains("1 to 3"), "got {bad}");
}

/// A raw write on the short form of a band's id forgets the step last sent to the band, which
/// the session reads back under the band's own id.
#[tokio::test]
async fn a_raw_write_on_a_bands_short_id_replaces_the_step_sent_to_the_band() {
    let (mut backend, _gate) = fake_backend_on("lokimax", Duration::from_secs(5)).await;
    let band: FunctionId = "6.1.3".parse().expect("an id");
    backend.sent.insert(band, (20, Instant::now(), true));
    backend
        .raw("6.1", "u8", "22", false)
        .await
        .expect("the band takes its short id");
    assert!(
        !backend.sent.contains_key(&band),
        "the step sent before it no longer stands"
    );
}
