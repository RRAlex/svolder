// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A step from a reading the device never reported is refused with code 4, naming the key: the
/// control can be stepped, there is only nothing to step from.
#[tokio::test]
async fn a_step_with_no_reading_to_step_from_is_refused_with_code_4() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
            Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::from_text("1234").unwrap(),
        )))
        .await;
        // The profile names a level control at a function the device never reports.
        let source = "schema = 1\nname = \"trimmed\"\n\
            [[control]]\nid = [99]\nkey = \"trim\"\nlabel = \"Trim\"\nkind = \"level\"\n\
            range = [0, 10]\n";
        actor.profile = Registry::from_sources(vec![("trimmed".into(), source.into())])
            .expect("the test profile loads")
            .get("trimmed")
            .expect("the test profile")
            .clone();
        for value in ["up", "down"] {
            let refused = actor
                .run_command(Command::Set {
                    key: "trim".into(),
                    value: value.into(),
                    step: 1,
                    no_wait: false,
                })
                .await;
            assert!(
                matches!(&refused, Response::Error { code: CODE_TIMEOUT, message, .. } if message == "no current value for trim"),
                "{value}: got {refused:?}"
            );
        }
    })
    .await
    .expect("test timed out");
}

/// The daemon answers a readout an `only_while` gates as the CLI shows it: false while its gate
/// reads false, true once the gate reads true, the raw value as the device sent it.
#[tokio::test]
async fn a_gated_readout_is_answered_false_while_its_gate_reads_false() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
            Registry::builtin().unwrap().get("tyr").unwrap(),
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        ));
        let mut actor = connected_actor_on("tyr", transport).await;
        let warming = actor
            .profile
            .control_by_key("warming_up")
            .expect("a gated readout")
            .clone();
        let warm = Message::status(warming.id, Value::Bool(true));
        assert_eq!(actor.value_object(&warm).display, "false", "the amp is off");
        let answer = actor
            .run_command(Command::Set {
                key: "power".into(),
                value: "on".into(),
                step: 1,
                no_wait: false,
            })
            .await;
        assert!(matches!(answer, Response::Value(_)), "{answer:?}");
        let shown = actor.value_object(&warm);
        assert_eq!(shown.display, "true");
        assert_eq!(shown.raw, serde_json::json!(true));
    })
    .await
    .expect("test timed out");
}

/// The next `climb` event on `events`, as its key and target.
async fn next_climb(events: &mut broadcast::Receiver<Event>) -> (String, Option<i64>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::Climb { key, target, .. }) = events.recv().await {
                return (key, target);
            }
        }
    })
    .await
    .expect("a climb event")
}

/// A recall through the writer pushes `recalled` on the device's events, notes it in the ring
/// and shows in the device's row; a climb pushes `climb` as it starts, as it is steered and as
/// it ends.
#[tokio::test]
async fn the_memory_and_the_climb_are_pushed_as_events() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        let mut events = handle.events.subscribe();
        let eq = handle
            .feed
            .borrow()
            .profile
            .as_ref()
            .and_then(|p| p.eq.clone())
            .expect("a tone control");
        let fitting: Vec<svolder_profiles::tone::preset::Preset> =
            svolder_profiles::tone::preset::fitting(&eq, &[])
                .into_iter()
                .map(|(p, _)| p)
                .collect();
        let crate::writer::Recalling { done, .. } =
            handle
                .writer
                .recall(crate::writer::Origin::Mcp, "Warm".into(), fitting);
        done.await.expect("the recall ended");
        let said = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(Event::Recalled { device, name }) = events.recv().await {
                    return (device, name);
                }
            }
        })
        .await
        .expect("a recalled event");
        assert_eq!(said.1, "Warm");
        assert_eq!(handle.describe().recalled.as_deref(), Some("Warm"));
        let Response::Readings { recalls, .. } = handle.readings(60) else {
            panic!("not a readings answer");
        };
        assert_eq!(recalls.last().map(|r| r.name.as_str()), Some("Warm"));
        let _ = handle
            .writer
            .glide(
                crate::writer::Origin::Mqtt,
                "volume".into(),
                127,
                crate::writer::Mode::Latest,
            )
            .await;
        assert_eq!(
            next_climb(&mut events).await,
            ("volume".to_string(), Some(127))
        );
        let _ = handle
            .writer
            .glide(
                crate::writer::Origin::Mqtt,
                "volume".into(),
                126,
                crate::writer::Mode::Latest,
            )
            .await;
        assert_eq!(
            next_climb(&mut events).await,
            ("volume".to_string(), Some(126))
        );
        let _ = handle
            .writer
            .submit(
                crate::writer::Origin::Socket,
                Command::Set {
                    key: "volume".into(),
                    value: "60".into(),
                    step: 1,
                    no_wait: false,
                },
                crate::writer::Mode::Ordered,
            )
            .await;
        assert_eq!(next_climb(&mut events).await, ("volume".to_string(), None));
    })
    .await
    .expect("test timed out");
}
