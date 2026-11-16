// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A recall sent as a raw command, waited or not, goes out and answers at once: an action has no
/// echo, and nothing of it is published or kept as a reading.
#[tokio::test(start_paused = true)]
async fn a_raw_recall_is_sent_without_an_echo_and_is_no_reading() {
    let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
        Registry::builtin().unwrap().get("lokimax").unwrap(),
        "0102030405060708".parse().unwrap(),
        3,
        Passphrase::NONE,
    ));
    let mut actor = connected_actor_on("lokimax", transport).await;
    let mut events = actor.events.subscribe();
    let recall = FunctionId::three(7, 2, 2);
    let began = tokio::time::Instant::now();
    for (command, what) in [
        (
            Command::Raw {
                id: "7.2.2".into(),
                value_type: "data".into(),
                value: String::new(),
                no_wait: false,
            },
            "a waited raw command",
        ),
        (
            Command::Set {
                key: "recall".into(),
                value: "2".into(),
                step: 1,
                no_wait: false,
            },
            "a waited set",
        ),
    ] {
        let answered = actor.run_command(command).await;
        assert!(matches!(answered, Response::Ack), "{what}: {answered:?}");
    }
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "no echo was waited for: {:?}",
        began.elapsed()
    );
    assert!(actor.state_of().get(recall).is_none());
    assert!(!actor.feed.borrow().heard_at.contains_key(&recall));
    while let Ok(event) = events.try_recv() {
        if let Event::Status { reading, .. } = event {
            assert_ne!(reading.id, "7.2.2", "an action is no status");
        }
    }
}

/// A Loki Max actor on a fake device the test keeps, with the transport the actor itself scans.
async fn loki_actor() -> (Actor, Arc<svolder_ble::FakeTransport>) {
    let registry = Registry::builtin().unwrap();
    let fake = Arc::new(svolder_ble::FakeTransport::from_profile(
        registry.get("lokimax").expect("lokimax"),
        "0102030405060708".parse().expect("a valid unit id"),
        3,
        Passphrase::from_text("1234").expect("four digits"),
    ));
    let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
        registry.get("lokimax").expect("lokimax"),
        "0102030405060708".parse().expect("a valid unit id"),
        3,
        Passphrase::NONE,
    ));
    let actor = connected_actor_over("lokimax", Arc::clone(&fake), transport).await;
    (actor, fake)
}

/// How many frames the fake device has taken.
fn frames_taken(fake: &svolder_ble::FakeTransport) -> usize {
    fake.device().lock().expect("the device").commands_seen()
}

/// A `set` of a device slot's recall goes to the device as one frame and is answered at once: an
/// action has no echo to wait for, however the command asked.
#[tokio::test(start_paused = true)]
async fn set_recall_is_sent_without_waiting_for_an_echo() {
    let (mut actor, fake) = loki_actor().await;
    let before = frames_taken(&fake);
    let began = tokio::time::Instant::now();
    let answered = actor
        .run_command(Command::Set {
            key: "recall".into(),
            value: "Preset 2".into(),
            step: 1,
            no_wait: false,
        })
        .await;
    assert!(matches!(answered, Response::Ack), "{answered:?}");
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(frames_taken(&fake), before + 1);
}

/// A read of an action is a usage error in the profile crate's words, which say it is sent and
/// how.
#[tokio::test(start_paused = true)]
async fn get_of_an_action_is_a_usage_error_in_the_profiles_words() {
    let (actor, _fake) = loki_actor().await;
    for key in ["recall", "save"] {
        let answered = actor.get(key);
        let Response::Error { code, message, .. } = &answered else {
            panic!("{key}: {answered:?}");
        };
        assert_eq!(*code, 1, "{key}");
        assert_eq!(
            message,
            &format!("{key} is an action: it is sent, never read; svolder set {key} 1-3 sends it")
        );
    }
}

/// A slot the action does not have is refused in words, and no frame is sent for it.
#[tokio::test(start_paused = true)]
async fn set_recall_with_a_slot_past_the_actions_slots_is_refused_before_any_frame() {
    let (mut actor, fake) = loki_actor().await;
    let before = frames_taken(&fake);
    for slot in ["4", "0", "Preset 4", "up"] {
        let answered = actor
            .run_command(Command::Set {
                key: "recall".into(),
                value: slot.into(),
                step: 1,
                no_wait: false,
            })
            .await;
        let Response::Error { code, message, .. } = &answered else {
            panic!("{slot}: {answered:?}");
        };
        assert_eq!(*code, 1, "{slot}");
        assert!(message.contains("recall"), "{slot}: {message}");
    }
    assert_eq!(frames_taken(&fake), before, "nothing went to the device");
}
