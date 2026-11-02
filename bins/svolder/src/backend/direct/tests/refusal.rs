// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a direct session does when a device on `auto` refuses its code: one drop of the link, a
//! fresh reading of the code, and no second drop for the same refusal.

use svolder_ble::Advertised;

use super::*;

fn code(text: &str) -> Passphrase {
    Passphrase::from_text(text).expect("four digits")
}

/// The next update, or a failure when none comes within a minute, so a drop that never happens
/// fails the case instead of holding it.
async fn update(backend: &mut DirectBackend) -> Option<Update> {
    tokio::time::timeout(Duration::from_secs(60), backend.next_status())
        .await
        .expect("an update within a minute")
        .expect("an update")
}

/// A gate over a fake gungnir 2 whose code is `1234`, saying `how` about it on the air.
fn gate_saying(how: Advertised) -> Arc<FakeGate> {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a profile").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let gate = Arc::new(FakeGate::new(&profile, unit, code("1234")));
    gate.advertise(how);
    gate
}

/// What `reach` makes of the device behind `gate` for a device whose entry is `entry`, or none
/// when `None`.
async fn reached(gate: &Arc<FakeGate>, entry: Option<&str>) -> (Session, Episode) {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let transport = Arc::clone(gate) as Arc<dyn Transport>;
    let device = entry.map(|passphrase| DeviceConfig {
        name: "fake".into(),
        udid: "0102030405060708".into(),
        profile: None,
        passphrase: passphrase.into(),
        inputs: Default::default(),
    });
    let (session, _, episode, _) = reach(
        &transport,
        &registry,
        device.as_ref(),
        Some("0102030405060708"),
    )
    .await
    .expect("the device is reached");
    (session, episode)
}

/// A device that refuses the code of an advertisement older than its own is read afresh once
/// when a verb opens its session, and the verb gets the accepted link: a one-shot verb is
/// retried by the same code that serves a watched session.
#[tokio::test(start_paused = true)]
async fn opening_over_a_stale_code_is_corrected_by_one_drop() {
    let gate = gate_saying(Advertised::UntilReleased(code("9999")));
    let (session, _) = reached(&gate, None).await;

    assert!(session.is_connected());
    assert!(
        session.code_accepted(),
        "the device was read under its own code"
    );
    assert_eq!(session.passphrase(), code("1234"));
    assert_eq!((gate.opened(), gate.releases()), (2, 1));
}

/// The same code again, or nothing heard, is one drop and then the refused link as it always
/// was.
#[tokio::test(start_paused = true)]
async fn opening_when_the_fresh_listen_changes_nothing_keeps_the_refused_link_after_one_drop() {
    let same = gate_saying(Advertised::Always(code("9999")));
    let (session, _) = reached(&same, None).await;
    assert!(session.refused());
    assert_eq!((same.opened(), same.releases()), (2, 1));

    let silent = gate_saying(Advertised::UntilReleased(code("9999")));
    silent.go_quiet_after_release();
    let (session, _) = reached(&silent, None).await;
    assert!(session.refused());
    assert_eq!((silent.opened(), silent.releases()), (2, 1));
}

/// A configured code, right or wrong, and `none`, are never dropped over.
#[tokio::test(start_paused = true)]
async fn opening_with_digits_or_none_never_drops() {
    for entry in ["9999", "none"] {
        let gate = gate_saying(Advertised::UntilReleased(code("5555")));
        let (session, _) = reached(&gate, Some(entry)).await;
        assert!(session.refused(), "{entry}");
        assert_eq!((gate.opened(), gate.releases()), (1, 0), "{entry}");
    }
}

/// The next update after a refusal of a write is the link going down, as a lost link's is; the
/// one after it is the link coming back, read under the code the device advertises afresh.
#[tokio::test(start_paused = true)]
async fn a_refusal_on_a_write_drops_the_link_once_and_reads_the_code_afresh() {
    let (mut backend, gate) = fake_backend().await;
    gate.change_code(code("1234"), code("5555"));
    gate.advertise(Advertised::UntilReleased(code("1234")));

    let refused = backend.set("mute", "on", 1, false).await;
    assert!(
        matches!(refused, Err(WriteError::Ble(BleError::PassphraseRejected))),
        "got {refused:?}"
    );
    let down = update(&mut backend).await;
    assert!(
        matches!(down, Some(Update::Connection { connected: false })),
        "got {down:?}"
    );
    let up = update(&mut backend).await;
    assert!(
        matches!(up, Some(Update::Connection { connected: true })),
        "got {up:?}"
    );

    assert_eq!((gate.opened(), gate.releases()), (2, 1));
    let confirmed = backend.set("mute", "on", 1, false).await;
    assert!(matches!(confirmed, Ok(Some(_))), "got {confirmed:?}");
}

/// A refusal that a fresh listen does not change gets no second drop: the refused link is kept,
/// and nothing is written to it.
#[tokio::test(start_paused = true)]
async fn the_same_code_again_gives_one_drop_and_then_a_kept_refused_link() {
    let (mut backend, gate) = fake_backend().await;
    gate.change_code(code("1234"), code("5555"));
    gate.advertise(Advertised::Always(code("1234")));
    let _ = backend.set("mute", "on", 1, false).await;

    for connected in [false, true] {
        let update = update(&mut backend).await;
        assert!(
            matches!(update, Some(Update::Connection { connected: c }) if c == connected),
            "got {update:?}"
        );
    }
    let quiet = tokio::time::timeout(Duration::from_secs(60), backend.next_status()).await;
    assert!(quiet.is_err(), "a second drop: {quiet:?}");
    assert_eq!((gate.opened(), gate.releases()), (2, 1));
    let again = backend.set("mute", "on", 1, false).await;
    assert!(
        matches!(again, Err(WriteError::Ble(BleError::PassphraseRejected))),
        "got {again:?}"
    );
}

/// A device that is not heard again in the window is reconnected as it would have been.
#[tokio::test(start_paused = true)]
async fn no_advertisement_in_the_window_gives_one_drop_and_then_a_kept_refused_link() {
    let (mut backend, gate) = fake_backend().await;
    gate.change_code(code("1234"), code("5555"));
    gate.go_quiet_after_release();
    let _ = backend.set("mute", "on", 1, false).await;

    for _ in 0..2 {
        update(&mut backend).await;
    }
    let quiet = tokio::time::timeout(Duration::from_secs(60), backend.next_status()).await;
    assert!(quiet.is_err(), "a second drop: {quiet:?}");
    assert_eq!((gate.opened(), gate.releases()), (2, 1));
}

/// A configured code that is refused keeps its link and nothing listens for another.
#[tokio::test(start_paused = true)]
async fn a_refused_code_typed_by_the_user_is_never_dropped() {
    let (mut backend, gate) = judged_backend("9999", None, false).await;
    let quiet = tokio::time::timeout(Duration::from_secs(60), backend.next_status()).await;
    assert!(quiet.is_err(), "a drop: {quiet:?}");
    assert_eq!((gate.opened(), gate.releases()), (1, 0));
}

/// A link the episode left standing that drops on its own and comes back refused under the same
/// code is the same refusal: it is reconnected, and not dropped a second time.
#[tokio::test(start_paused = true)]
async fn a_link_that_drops_on_its_own_is_the_same_refusal() {
    let (mut backend, gate) = fake_backend().await;
    gate.change_code(code("1234"), code("5555"));
    gate.advertise(Advertised::Always(code("1234")));
    let _ = backend.set("mute", "on", 1, false).await;
    for _ in 0..2 {
        update(&mut backend).await;
    }
    assert_eq!((gate.opened(), gate.releases()), (2, 1));

    gate.sever();
    for _ in 0..2 {
        update(&mut backend).await;
    }
    let quiet = tokio::time::timeout(Duration::from_secs(60), backend.next_status()).await;
    assert!(quiet.is_err(), "a second drop: {quiet:?}");
    assert_eq!((gate.opened(), gate.releases()), (3, 1));
}

/// A watched session refused after its one drop, because the device was not heard again in the
/// window, recovers when the link drops on its own and the device advertises its own code: the
/// reconnect reads that code, is accepted, and the episode is over.
#[tokio::test(start_paused = true)]
async fn a_power_cycle_ends_a_spent_episode_on_a_watched_session() {
    let (mut backend, gate) = fake_backend().await;
    gate.change_code(code("1234"), code("5555"));
    gate.go_quiet_after_release();
    let _ = backend.set("mute", "on", 1, false).await;
    for _ in 0..2 {
        update(&mut backend).await;
    }
    assert!(
        backend.refused(),
        "the drop heard nothing and is kept refused"
    );

    gate.advertise(Advertised::Truthfully);
    gate.speak_again();
    gate.sever();
    for _ in 0..2 {
        update(&mut backend).await;
    }
    assert!(!backend.refused(), "the reconnect read the new code");
    assert_eq!(backend.episode, Episode::default());
    let confirmed = backend.set("mute", "on", 1, false).await;
    assert!(matches!(confirmed, Ok(Some(_))), "got {confirmed:?}");
}

/// A reconnect that reads another wrong code than the refused one is another episode: its
/// refusal is given its own drop.
#[tokio::test(start_paused = true)]
async fn another_code_read_on_a_reconnect_earns_its_own_drop() {
    let (mut backend, gate) = fake_backend().await;
    gate.change_code(code("1234"), code("5555"));
    gate.go_quiet_after_release();
    let _ = backend.set("mute", "on", 1, false).await;
    for _ in 0..2 {
        update(&mut backend).await;
    }
    assert_eq!((gate.opened(), gate.releases()), (2, 1));

    gate.advertise(Advertised::Always(code("8888")));
    gate.speak_again();
    gate.sever();
    for _ in 0..2 {
        update(&mut backend).await;
    }
    let down = update(&mut backend).await;
    assert!(
        matches!(down, Some(Update::Connection { connected: false })),
        "got {down:?}"
    );
    update(&mut backend).await;
    assert_eq!((gate.opened(), gate.releases()), (4, 2));
}

/// A changed entry ends the episode, a repeated one does not, and a device that shows its code
/// works ends it too.
#[tokio::test(start_paused = true)]
async fn the_entry_and_the_proof_end_the_episode() {
    let (mut backend, _gate) = judged_backend("auto", Some("4321"), true).await;
    let mut spent = Episode::default();
    spent.dropped(code("9999"));
    spent.listened();

    backend.episode = spent;
    backend.adopt_entry("auto");
    assert_eq!(backend.episode, spent, "the entry did not change");
    backend.adopt_entry("1234");
    assert_eq!(backend.episode, Episode::default());

    let (mut backend, _gate) = fake_backend().await;
    backend.episode = spent;
    backend.judge_passphrase();
    assert_eq!(
        backend.episode,
        Episode::default(),
        "the device proved the code"
    );
}

/// A backend asks whether its link was refused: a refused one says so and a healthy one does
/// not, with the code and step a refusal earns.
#[tokio::test(start_paused = true)]
async fn a_backend_over_a_refused_link_has_a_refusal_to_report() {
    let (refused, _gate) = judged_backend("9999", None, false).await;
    let failure = crate::backend::Backend::Direct(Box::new(refused))
        .refusal()
        .expect("a refusal");
    assert_eq!(failure.code, Kind::Refused.code());
    assert_eq!(failure.next, crate::recovery::next_step(Kind::Refused));

    let (healthy, _gate) = fake_backend().await;
    assert!(
        crate::backend::Backend::Direct(Box::new(healthy))
            .refusal()
            .is_none()
    );
}

/// A one-shot open whose second connect after the drop fails is still the refusal that started
/// it, not a lost link.
#[tokio::test(start_paused = true)]
async fn a_failed_second_connect_after_the_drop_reports_the_refusal() {
    let gate = gate_saying(Advertised::UntilReleased(code("9999")));
    gate.fail_connects_after_release();
    let registry = Registry::builtin().expect("the built-in profiles load");
    let transport = Arc::clone(&gate) as Arc<dyn Transport>;
    let failure = reach(&transport, &registry, None, Some("0102030405060708"))
        .await
        .err()
        .expect("the open fails");
    assert_eq!(failure.code, Kind::Refused.code());
    assert!(
        failure.to_string().contains("could not be reached again"),
        "{failure}"
    );
}

/// A link made after the drop that is refused too makes the code it was refused under what the
/// episode is about.
#[tokio::test(start_paused = true)]
async fn opening_over_two_wrong_codes_is_about_the_second() {
    let gate = gate_saying(Advertised::ThenAlways {
        stale: code("9999"),
        next: code("8888"),
    });
    let (session, episode) = reached(&gate, None).await;
    assert!(session.refused());
    assert_eq!(session.passphrase(), code("8888"));
    let mut expected = Episode::default();
    expected.dropped(code("8888"));
    expected.listened();
    assert_eq!(episode, expected);
}
