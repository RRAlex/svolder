// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A code that differs from the advertised one earns a note, the link is made and kept all the
/// same, and the note carries no digit of either code.
#[tokio::test]
async fn a_code_that_differs_from_the_advertised_one_earns_a_note() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "9999").await;
    let info = handle.describe();
    assert_eq!(info.passphrase_note, Some(PassphraseNote::Differs));
    assert!(info.connected, "the note changes nothing about the link");
    let wire = svolder_ipc::socket::protocol::to_line(&Response::Devices {
        devices: vec![info],
    })
    .unwrap();
    // Looked at field by field, not as text: a clock reading in the line can hold any digits.
    fn leaks(value: &serde_json::Value, digits: &str) -> bool {
        match value {
            serde_json::Value::String(text) => text.contains(digits),
            serde_json::Value::Number(number) => number.to_string() == digits,
            serde_json::Value::Array(items) => items.iter().any(|item| leaks(item, digits)),
            serde_json::Value::Object(fields) => fields
                .iter()
                .any(|(key, item)| key.contains(digits) || leaks(item, digits)),
            _ => false,
        }
    }
    let sent: serde_json::Value = serde_json::from_str(&wire).unwrap();
    for digits in ["1234", "9999", "4321"] {
        assert!(!leaks(&sent, digits), "{digits} crossed the socket: {wire}");
    }
}

/// `auto` follows the advertisement, so it never earns a note, whatever the advertisement
/// says.
#[tokio::test]
async fn auto_never_earns_a_note() {
    let transport = Shows::new();
    transport.show("4321");
    let handle = spawned_on(&transport, "auto").await;
    assert_eq!(handle.describe().passphrase_note, None);
}

/// `none` while the advertisement carries a code is a note of its own.
#[tokio::test]
async fn none_while_a_code_is_advertised_earns_a_note() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "none").await;
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::NoneConfigured)
    );
}

/// Four digits while the advertisement carries no code are a note of their own words.
#[tokio::test]
async fn digits_while_no_code_is_advertised_earn_their_own_note() {
    let transport = Shows::new();
    transport.show("none");
    let handle = spawned_on(&transport, "9999").await;
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::NoCodeAdvertised)
    );
}

/// A device that answered the connect's refresh in full has shown the code works, whatever an
/// advertisement older than its code says: the verdict stands for an advertisement that
/// differs, and the note does not.
#[tokio::test]
async fn a_device_that_answers_the_connect_burst_clears_a_stale_verdict() {
    let transport = Shows::new();
    transport.show("4321");
    let handle = spawned_on(&transport, "1234").await;
    assert_eq!(handle.describe().passphrase_note, None);
}

/// The firmware versions a connect asks for come back as readings the feed carries, stamped as
/// heard from the device, so the web's facts no longer fall back to the advertised build.
#[tokio::test]
async fn the_answer_becomes_a_reading() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "1234").await;
    let mut feed = handle.feed.clone();
    let firmware = FunctionId::three(210, 2, 1);
    let dsp = FunctionId::three(210, 2, 2);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            {
                let feed = feed.borrow_and_update();
                let held = |id| {
                    feed.readings
                        .iter()
                        .find(|m| m.id == id)
                        .map(|m| m.value.clone())
                };
                if held(firmware).is_some() && held(dsp).is_some() {
                    assert_eq!(held(firmware), Some(Value::U16(3)));
                    assert_eq!(held(dsp), Some(Value::U16(3)));
                    assert!(feed.heard_at.contains_key(&firmware));
                    assert!(feed.heard_at.contains_key(&dsp));
                    return;
                }
            }
            feed.changed().await.expect("the actor is running");
        }
    })
    .await
    .expect("the firmware answers never reached the feed");
}

/// Correcting the entry takes the note away, and spoiling it again brings it back.
#[tokio::test]
async fn correcting_the_entry_clears_the_note_and_spoiling_it_raises_it_again() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "9999").await;
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::Differs)
    );
    for (entry, note) in [
        ("1234", None),
        ("9999", Some(PassphraseNote::Differs)),
        ("auto", None),
    ] {
        let answered = ask(&handle, Command::Configure(fake_config(entry))).await;
        assert!(matches!(answered, Response::Ack), "got {answered:?}");
        assert_eq!(handle.describe().passphrase_note, note, "entry {entry}");
    }
}

/// An advertisement that later matches the entry takes the note away at the attempt that
/// reads it.
#[tokio::test]
async fn a_later_advertisement_that_matches_clears_the_note() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "9999").await;
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::Differs)
    );
    transport.show("9999");
    transport.inner.sever();
    tokio::time::timeout(Duration::from_secs(5), async {
        while handle.describe().passphrase_note.is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the note outlived an advertisement that matches");
}

/// The echo of a `set` the device answered shows the code works and takes the note away; a
/// status the device volunteers does not, and a verdict that has not changed is not said again.
#[tokio::test]
async fn the_echo_of_a_waited_set_clears_the_note_and_a_volunteered_status_does_not() {
    let mut actor = unproven_actor().await;
    assert!(actor.judge_passphrase(), "the first verdict is news");
    assert_eq!(actor.note, Some(PassphraseNote::Differs));
    assert!(
        !actor.judge_passphrase(),
        "the same verdict is not said twice"
    );

    actor.publish_status(&Message::status(FunctionId::one(10), Value::U8(100)));
    actor.judge_passphrase();
    assert_eq!(actor.note, Some(PassphraseNote::Differs));

    let answered = actor
        .run_command(Command::Raw {
            id: "10".into(),
            value_type: "u8".into(),
            value: "90".into(),
            no_wait: false,
        })
        .await;
    assert!(matches!(answered, Response::Value(_)), "got {answered:?}");
    assert_eq!(actor.note, None);
    assert_eq!(actor.info.lock().unwrap().passphrase_note, None);
}

/// After a `passphrase` change the held advertisement says the new code, which the device will
/// advertise next: an entry that names the new digits gets no note, and `auto` keeps the new code
/// rather than re-keying the live session to the old one.
#[tokio::test]
async fn a_passphrase_change_updates_the_held_advertisement() {
    let mut actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
        Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
        "0102030405060708".parse().unwrap(),
        3,
        Passphrase::from_text("1234").unwrap(),
    )))
    .await;
    let changed = actor.passphrase("5678").await;
    assert!(matches!(changed, Response::Ack), "got {changed:?}");

    let answered = actor.configure(fake_config("5678"));
    assert!(matches!(answered, Response::Ack), "got {answered:?}");
    assert_eq!(
        actor.note, None,
        "the entry names the code the device now holds"
    );

    let answered = actor.configure(fake_config("auto"));
    assert!(matches!(answered, Response::Ack), "got {answered:?}");
    assert_eq!(actor.note, None);
    let set = actor
        .run_command(Command::Set {
            key: "input".into(),
            value: "4".into(),
            step: 1,
            no_wait: false,
        })
        .await;
    assert!(
        matches!(set, Response::Value(_)),
        "auto re-keyed the session to the old code: {set:?}"
    );
}

/// A refusal that arrives on a link the connect's burst had shown to work ends that proof: the
/// actor judges after each status it reads, so the note comes back with the stale verdict.
#[tokio::test]
async fn a_refusal_after_a_proof_brings_the_note_back() {
    let transport = Shows::new();
    transport.show("4321");
    let handle = spawned_on(&transport, "1234").await;
    assert_eq!(
        handle.describe().passphrase_note,
        None,
        "proven by the burst"
    );
    transport.inner.push(&[Message::status(
        svolder_proto::function::PASSPHRASE_REJECTED,
        Value::Bool(true),
    )]);
    tokio::time::timeout(Duration::from_secs(5), async {
        while handle.describe().passphrase_note.is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the refusal did not bring the note back");
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::Differs)
    );
}

/// A waited `set` the device refuses, on a link a connect's burst had shown to work, judges at
/// once: the refusal is consumed by the `set`, so `serve_connected` never reads it as a status.
#[tokio::test]
async fn a_refused_waited_set_brings_the_note_back() {
    let registry = Arc::new(Registry::builtin().unwrap());
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let fake = Arc::new(svolder_ble::FakeTransport::from_profile(
        &profile,
        unit,
        3,
        Passphrase::from_text("1234").expect("four digits"),
    ));
    let transport = Arc::clone(&fake) as Arc<dyn Transport>;
    let mut actor = connected_actor(Arc::clone(&transport)).await;
    // Over the fake this test can push to, with the session proven on 1234.
    let advert = fake
        .scan(Duration::from_millis(1))
        .await
        .expect("the fake always scans")
        .next()
        .await
        .expect("the fake advertises itself");
    let mut session = Session::new(
        transport,
        advert.clone(),
        profile,
        Passphrase::from_text("1234").expect("four digits"),
    );
    session.connect().await.expect("the fake connects");
    actor.session = Some(session);
    let mut held = advert;
    held.manufacturer.encrypted_passphrase = Passphrase::encrypt(
        Passphrase::from_text("4321").unwrap().encoded(),
        Passphrase::advert_key(unit),
    );
    actor.advert = Some(held);
    actor.judge_passphrase();
    assert_eq!(actor.note, None, "proven by the burst");

    fake.push(&[Message::status(
        svolder_proto::function::PASSPHRASE_REJECTED,
        Value::Bool(true),
    )]);
    let answered = actor
        .run_command(Command::Raw {
            id: "10".into(),
            value_type: "u8".into(),
            value: "90".into(),
            no_wait: false,
        })
        .await;
    assert!(
        matches!(
            answered,
            Response::Error {
                code: CODE_LINK,
                ..
            }
        ),
        "got {answered:?}"
    );
    assert_eq!(actor.note, Some(PassphraseNote::Differs));
}
