// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a key does to the device through the loop: a set, a preset load, a gain step, a typed value
//! and a refused write.

use super::*;

/// Read the update feed into `app` until `id` reads `raw`, or until the deadline passes.
async fn hear_until(backend: &mut Backend, app: &mut App, id: FunctionId, raw: i64) {
    let feed = async {
        while raw_of(app, id) != Some(raw) {
            match backend.next_status().await.expect("the feed keeps reading") {
                Some(update) => app.apply(update, Instant::now()),
                None => panic!("the feed ended before {id} read {raw}"),
            }
        }
    };
    tokio::time::timeout(ECHO_DEADLINE, feed)
        .await
        .unwrap_or_else(|_| panic!("{id} never read {raw}"));
}

/// A `Set` reaches the device: the echo of the write comes back on the update feed, and
/// the app's reading for that control is what the device confirmed.
#[tokio::test(start_paused = true)]
async fn a_set_intent_reaches_the_device_and_its_echo_becomes_the_reading() {
    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    assert_eq!(
        raw_of(&app, INPUT),
        Some(1),
        "the fake device starts on input 1"
    );

    let intent = Intent::Set {
        key: "input".into(),
        value: "4".into(),
        step: 1,
    };
    execute(intent, &mut backend, &mut app)
        .await
        .expect("the write goes through");

    hear_until(&mut backend, &mut app, INPUT, 4).await;
    assert!(
        app.toast().is_none(),
        "a write that went through said nothing"
    );
}

/// A preset loaded from the list reaches the device as the writes its plan asks for, sent
/// without waiting as every key's writes are: the header says it is loading until the
/// device echoes the last of them, and then names it, the device's readings being the
/// preset's curve.
#[tokio::test(start_paused = true)]
async fn a_preset_load_reaches_the_device_and_its_last_echo_ends_the_loading() {
    use crossterm::event::KeyCode;
    use svolder_profiles::tone::preset;

    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    let bass = preset::builtins(&eq)
        .into_iter()
        .find(|preset| preset.name == "Bass")
        .expect("a built-in");
    assert!(!preset::matches(&eq, |id| raw_of(&app, id), &bass));

    app.key(KeyEvent::from(KeyCode::Char('p')));
    for _ in 0..2 {
        app.key(KeyEvent::from(KeyCode::Down));
    }
    let intents = app.key(KeyEvent::from(KeyCode::Enter));
    assert!(!intents.is_empty(), "the load wrote nothing");
    let last = app
        .readings
        .loading
        .as_ref()
        .expect("the load is loading")
        .last;
    for intent in intents {
        execute(intent, &mut backend, &mut app)
            .await
            .expect("the write goes through");
    }
    assert!(
        app.readings.loading.is_some(),
        "the load ended before the device answered"
    );

    let feed = async {
        while app.readings.loading.is_some() {
            match backend.next_status().await.expect("the feed keeps reading") {
                Some(update) => app.apply(update, Instant::now()),
                None => panic!("the feed ended before the load was confirmed"),
            }
        }
    };
    tokio::time::timeout(ECHO_DEADLINE, feed)
        .await
        .unwrap_or_else(|_| panic!("{last} was never echoed"));
    assert!(
        preset::matches(&eq, |id| raw_of(&app, id), &bass),
        "the device does not carry the preset"
    );
    assert_eq!(
        crate::tui::boxes::presets::standing(&app).map(|standing| standing.name),
        Some("Bass".to_string())
    );
    backend.close().await;
}

/// `←→` on a band walk the device's gain one of its own steps a press: down from 0 dB to
/// -0.20 dB, and one press back up lands on 0 dB again rather than on the +0.21 dB a round
/// half decibel would have rounded to.
#[tokio::test(start_paused = true)]
async fn one_arrow_moves_the_devices_gain_one_step_and_back_onto_0_db() {
    use crossterm::event::KeyCode;

    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.show(crate::tui::app::Screen::Tone);
    let gain = FunctionId::three(12, 1, 3);
    assert_eq!(
        raw_of(&app, gain),
        Some(98),
        "the fake's band 1 starts at 0 dB"
    );
    for (code, wanted) in [(KeyCode::Left, 97), (KeyCode::Right, 98)] {
        for intent in app.key(KeyEvent::from(code)) {
            execute(intent, &mut backend, &mut app)
                .await
                .expect("the write goes through");
        }
        hear_until(&mut backend, &mut app, gain, wanted).await;
    }
    backend.close().await;
}

/// Two presses sent before the device has echoed the first move its gain two steps: the
/// second steps from the one asked for, not from the reading the first has not changed yet.
#[tokio::test(start_paused = true)]
async fn two_presses_before_the_echo_move_the_devices_gain_two_steps() {
    use crossterm::event::KeyCode;

    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.show(crate::tui::app::Screen::Tone);
    let gain = FunctionId::three(12, 1, 3);
    assert_eq!(raw_of(&app, gain), Some(98));
    for _ in 0..2 {
        for intent in app.key(KeyEvent::from(KeyCode::Right)) {
            execute(intent, &mut backend, &mut app)
                .await
                .expect("the write goes through");
        }
    }
    hear_until(&mut backend, &mut app, gain, 100).await;
    backend.close().await;
}

/// A gain typed into the edit line reaches the device at the step nearest it, and that step
/// is what the band's row then reads: `1.5` is step 105, +1.45 dB. On the fake, which rests
/// in Flat mode, the write also switches the tone control to EQ.
#[tokio::test(start_paused = true)]
async fn a_gain_typed_into_the_edit_line_reaches_the_device_at_its_nearest_step() {
    use crossterm::event::KeyCode;

    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.show(crate::tui::app::Screen::Tone);
    let mut intents = Vec::new();
    for code in [
        KeyCode::Char('2'),
        KeyCode::Enter,
        KeyCode::Tab,
        KeyCode::Char('1'),
        KeyCode::Char('.'),
        KeyCode::Char('5'),
        KeyCode::Enter,
    ] {
        intents = app.key(KeyEvent::from(code));
    }
    assert!(app.tone.edit_line.is_none(), "the line stayed open");
    for intent in intents {
        execute(intent, &mut backend, &mut app)
            .await
            .expect("the write goes through");
    }
    hear_until(&mut backend, &mut app, FunctionId::three(12, 2, 3), 105).await;
    hear_until(&mut backend, &mut app, FunctionId::two(12, 50), 2).await;
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    assert_eq!(
        crate::tui::boxes::edit_line::gain_text(eq.gain_db(105)),
        "+1.45",
        "what the row reads"
    );
    backend.close().await;
}

/// A write the backend refuses becomes the notice the key line shows, with the step it
/// earns on the advice line, and the device hears nothing: function 210.1 is the
/// firmware-update family.
#[tokio::test(start_paused = true)]
async fn a_refused_write_becomes_the_notice() {
    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let refusal = execute(refused_write(), &mut backend, &mut app)
        .await
        .expect_err("the firmware-update family is never written to");
    assert!(refusal.to_string().contains("210.1"), "{refusal}");

    let mut watching = watching(backend, app, None);
    dispatch(refused_write(), &mut watching).await;
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        Some(refusal.to_string())
    );
    assert_eq!(
        watching.app.notices.advice.as_deref(),
        Some(crate::recovery::next_step(Kind::Usage)),
        "a refused write named no step"
    );
    watching.close().await;
}

/// A recall the backend refuses leaves the bands trusted: the app on a Loki Max assumed the
/// device would move them, and takes that back when the send fails or there is no link to send
/// on.
#[tokio::test(start_paused = true)]
async fn a_recall_that_fails_to_send_leaves_the_bands_trusted() {
    use crossterm::event::KeyCode;
    for link in [true, false] {
        let (backend, _gate, _) = fake(SHORT_DEADLINE).await;
        let mut app = App::new(
            crate::tui::test_support::profile_named("lokimax"),
            "loki".into(),
            &TuiConfig::default(),
            Vec::new(),
        );
        let ids: Vec<FunctionId> = (1..=6)
            .map(|band| {
                app.profile
                    .control_by_key(&format!("band{band}"))
                    .expect("a band")
                    .id
            })
            .collect();
        for (id, raw) in ids.iter().zip([28, 21, 17, 15, 15, 15]) {
            crate::tui::test_support::presets::hear_id(&mut app, *id, raw);
        }
        let intents = {
            app.key(KeyEvent::from(KeyCode::Char('p')));
            for _ in 0..9 {
                app.key(KeyEvent::from(KeyCode::Up));
            }
            app.key(KeyEvent::from(KeyCode::Enter))
        };
        assert_eq!(intents.len(), 1, "the recall");
        assert!(ids.iter().all(|id| app.readings.unreported.contains(id)));
        let mut watching = watching(backend, app, None);
        if !link {
            let backend = watching.backend.take().expect("the backend is open");
            backend.close().await;
        }
        // The fake device is not a Loki Max, so it refuses the key: either way the send fails.
        dispatch(intents[0].clone(), &mut watching).await;
        assert!(
            watching.app.readings.unreported.is_empty(),
            "link {link}: the bands are still unreported"
        );
        watching.close().await;
    }
}

/// The function address the Volume sits on.
const VOLUME: FunctionId = FunctionId::one(10);

/// A burst of one-notch raises at a reading above three quarters of the Volume's range reaches
/// the device as one write: the rest are paced by the glide.
#[tokio::test(start_paused = true)]
async fn a_burst_of_raises_above_three_quarters_writes_once() {
    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let set = Intent::Set {
        key: "volume".into(),
        value: "100".into(),
        step: 1,
    };
    execute(set, &mut backend, &mut app)
        .await
        .expect("the write goes through");
    hear_until(&mut backend, &mut app, VOLUME, 100).await;
    let mut watching = watching(backend, app, None);
    for _ in 0..10 {
        let up = Intent::Set {
            key: "volume".into(),
            value: "up".into(),
            step: 1,
        };
        dispatch(up, &mut watching).await;
        // The device answers each write before the next press, as a real one does within a
        // few milliseconds.
        let backend = watching.backend.as_mut().expect("the backend is open");
        let feed = async {
            while let Some(update) = backend.next_status().await.expect("the feed keeps reading") {
                watching.app.apply(update, Instant::now());
            }
        };
        let _ = tokio::time::timeout(Duration::from_millis(50), feed).await;
    }
    assert_eq!(raw_of(&watching.app, VOLUME), Some(101));
    watching.close().await;
}

/// An identify reaches the device: the toast says it was sent, a second inside the gap says it
/// was just sent, and neither writes a function the device echoes.
#[tokio::test(start_paused = true)]
async fn an_identify_intent_says_sent_and_then_just_sent() {
    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    backend.configured_as("dac");
    execute(Intent::Identify(None), &mut backend, &mut app)
        .await
        .expect("the request goes through");
    assert_eq!(
        app.toast().map(|t| t.text.as_str()),
        Some("identify sent to dac")
    );
    execute(Intent::Identify(None), &mut backend, &mut app)
        .await
        .expect("a repeat is no failure");
    assert_eq!(
        app.toast().map(|t| t.text.as_str()),
        Some("identify just sent to dac; wait 2 s")
    );
}

/// Over a daemon the request names the device of the Devices row it was asked from.
#[tokio::test]
async fn an_identify_intent_for_a_row_names_that_device_to_the_daemon() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = crate::test_support::stub_daemon::stub_daemon().await;
        daemon.identifies(svolder_ipc::Response::Identify {
            device: "other".into(),
            sent: true,
        });
        let mut backend = Backend::stub_remote(&daemon.socket).await;
        let (_, _gate, mut app) = fake(SHORT_DEADLINE).await;
        execute(
            Intent::Identify(Some("other".into())),
            &mut backend,
            &mut app,
        )
        .await
        .expect("the daemon answers");
        assert_eq!(
            app.toast().map(|t| t.text.as_str()),
            Some("identify sent to other")
        );
        assert_eq!(daemon.asked_for().last(), Some(&Some("other".to_string())));
    })
    .await
    .expect("test timed out");
}

/// A direct session holds one device and says so for another's name.
#[tokio::test(start_paused = true)]
async fn a_direct_session_refuses_to_identify_another_device() {
    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    backend.configured_as("dac");
    let refused = execute(
        Intent::Identify(Some("other".into())),
        &mut backend,
        &mut app,
    )
    .await;
    assert!(refused.is_err());
}
