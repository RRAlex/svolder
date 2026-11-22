// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a recall does beside other writes: a preset recall that runs, two recalls at once, a live
//! write in the line, a device lost half way. In each the device, the memory and the lamp agree
//! at the end, and the answer says what really landed.

use svolder_ipc::socket::protocol::PartStatus;

use super::*;

/// The recall request a front end sends.
fn recall_request(name: &str) -> Request {
    Request::Scene {
        op: SceneOp::Recall {
            name: name.into(),
            confirm: false,
        },
    }
}

/// Wait until `writer` has taken in `count` asks.
async fn arrived(writer: &Writer, count: u64) {
    let mut arrivals = writer.arrivals();
    tokio::time::timeout(
        Duration::from_secs(5),
        arrivals.wait_for(|taken| *taken >= count),
    )
    .await
    .expect("the asks arrived")
    .expect("the writer is there");
}

/// A recall of a scene whose tone equals what the device reads, behind a preset recall that has
/// not landed: the scene arrived last, so the device ends on the scene's tone, as the outcome
/// says (the plan is made from readings the running recall is about to change).
#[tokio::test]
async fn a_scene_tone_equal_to_the_readings_still_lands_after_a_running_preset_recall() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    assert!(matches!(rig.save("Desk").await, Response::Scene { .. }));
    let tone = rig
        .daemon
        .config_snapshot()
        .scene("Desk")
        .expect("saved")
        .devices[0]
        .tone
        .clone()
        .expect("a tone");

    desk.gate.forget_permits(10_000);
    let fitting = preset::builtins(desk.profile.eq.as_ref().expect("a tone control"));
    let warm = desk
        .handle
        .writer
        .recall(Origin::Socket, "Warm".into(), fitting);
    let daemon = Arc::clone(&rig.daemon);
    let answer = tokio::spawn(async move {
        daemon
            .handle_from(Origin::Web, recall_request("Desk"))
            .await
    });
    // Warm is ask 1; the scene's tone and its controls follow.
    arrived(&desk.handle.writer, 2).await;
    desk.gate.add_permits(10_000);
    warm.done.await.expect("the preset recall ended");
    let done = outcomes(answer.await.expect("the recall ended"));
    assert_eq!(done[0].1, PartStatus::Done, "{done:?}");
    let eq = desk.profile.eq.as_ref().expect("a tone control").clone();
    assert!(
        preset::matches(&eq, |id| reading(&desk.readings(), id), &tone),
        "the device plays the scene's tone: {:?}",
        desk.log()
    );
    rig.daemon.shutdown().await;
}

/// Two recalls at once: X sets the input to the other value and its write waits; Y holds the
/// input the device reads now. Y arrived last, so the input ends on Y's, the memory says Y, the
/// lamp says Y, and both answers are true.
#[tokio::test]
async fn two_recalls_at_once_end_on_the_later_one() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    let (first, other) = (desk.first("input"), desk.other("input"));
    rig.add(&[
        scene("X", None, vec![part(desk, None, &[("input", other)])]),
        scene("Y", None, vec![part(desk, None, &[("input", first)])]),
    ])
    .await;
    desk.gate.forget_permits(10_000);
    let daemon = Arc::clone(&rig.daemon);
    let x = tokio::spawn(async move { daemon.handle_from(Origin::Web, recall_request("X")).await });
    arrived(&desk.handle.writer, 1).await;
    let daemon = Arc::clone(&rig.daemon);
    let y = tokio::spawn(async move {
        daemon
            .handle_from(Origin::Socket, recall_request("Y"))
            .await
    });
    arrived(&desk.handle.writer, 2).await;
    desk.gate.add_permits(10_000);
    let (x, y) = (outcomes(x.await.expect("x")), outcomes(y.await.expect("y")));
    assert_eq!(x[0].1, PartStatus::Done, "{x:?}");
    assert_eq!(y[0].1, PartStatus::Done, "{y:?}");
    assert_eq!(
        desk.raw("input"),
        Some(first),
        "Y arrived last: {:?}",
        desk.log()
    );
    let view = rig.view();
    assert_eq!(view.recalled.as_deref(), Some("Y"));
    assert_eq!(
        view.on.as_deref(),
        Some("Y"),
        "the lamp agrees with the device"
    );
    assert_eq!(rig.daemon.scenes().recalled().as_deref(), Some("Y"));
    rig.daemon.shutdown().await;
}

/// A live write to a held control that is in the line when the recall arrives lands before the
/// recall's, so the scene's value stands at the end.
#[tokio::test]
async fn a_live_write_in_the_line_does_not_overtake_the_recall() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    let (first, other) = (desk.first("input"), desk.other("input"));
    rig.add(&[scene(
        "Home",
        None,
        vec![part(desk, None, &[("input", first)])],
    )])
    .await;
    desk.gate.forget_permits(10_000);
    let live = desk.handle.writer.submit(
        Origin::Mcp,
        Command::Set {
            key: "input".into(),
            value: other.to_string(),
            step: 1,
            no_wait: false,
        },
        crate::writer::Mode::Ordered,
    );
    let daemon = Arc::clone(&rig.daemon);
    let answer = tokio::spawn(async move {
        daemon
            .handle_from(Origin::Web, recall_request("Home"))
            .await
    });
    arrived(&desk.handle.writer, 2).await;
    desk.gate.add_permits(10_000);
    live.await.expect("the live write was answered");
    let done = outcomes(answer.await.expect("the recall ended"));
    assert_eq!(done[0].1, PartStatus::Done, "{done:?}");
    assert_eq!(desk.raw("input"), Some(first), "{:?}", desk.log());
    assert_eq!(rig.view().on.as_deref(), Some("Home"));
    rig.daemon.shutdown().await;
}

/// A control the device already reads, with nothing in the line, is not written again.
#[tokio::test]
async fn a_control_already_there_is_not_written() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    let first = desk.first("input");
    rig.add(&[scene(
        "Home",
        None,
        vec![part(desk, None, &[("input", first)])],
    )])
    .await;
    let done = outcomes(rig.recall("Home").await);
    assert_eq!(done[0].1, PartStatus::Done);
    assert!(
        desk.log().iter().all(|line| !line.starts_with("set input")),
        "{:?}",
        desk.log()
    );
    rig.daemon.shutdown().await;
}

/// A device lost in the middle of a recall: its part is failed, in the writer's or the device's
/// words, and the other parts are recalled all the same.
#[tokio::test]
async fn a_device_lost_half_way_is_failed_and_the_others_are_recalled() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let amp = stand("lyr5", "Amp", LYR, &[]);
    let rig = Rig::new(vec![desk, amp], false, "").await;
    let (desk, amp) = (&rig.stands[0], &rig.stands[1]);
    desk.fail_at.store(2, std::sync::atomic::Ordering::SeqCst);
    rig.add(&[scene(
        "Movie",
        None,
        vec![
            part(
                desk,
                Some(tone_of(&desk.profile, "Bass")),
                &[("input", desk.other("input"))],
            ),
            part(amp, None, &[("input", amp.other("input"))]),
        ],
    )])
    .await;
    let done = outcomes(rig.recall("Movie").await);
    assert_eq!(done[0].0, "Desk DAC");
    assert_eq!(done[0].1, PartStatus::Failed, "{done:?}");
    assert!(
        done[0]
            .2
            .iter()
            .any(|note| note.contains("the device went away")),
        "{:?}",
        done[0].2
    );
    assert_eq!(done[1], ("Amp".to_string(), PartStatus::Done, Vec::new()));
    assert_eq!(amp.raw("input"), Some(amp.other("input")));
    let view = rig.view();
    assert_eq!(
        view.on, None,
        "a part that failed is not a scene that is on"
    );
    assert_eq!(view.recalled.as_deref(), Some("Movie"));
    rig.daemon.shutdown().await;
}

/// With host audio off the computer part cannot be applied, so no question is asked about a
/// preset it would play twice; the part is left.
#[tokio::test]
async fn no_question_is_asked_while_the_computer_cannot_play_it() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], false, "[host]\nenabled = false\n").await;
    let desk = &rig.stands[0];
    rig.add(&[scene(
        "Warm both",
        Some(computer_part("warm", vec![pk(100.0, 3.0, 1.0)], 0.0)),
        vec![part(desk, Some(tone_of(&desk.profile, "Warm")), &[])],
    )])
    .await;
    let done = outcomes(rig.recall("Warm both").await);
    assert_eq!(done[0].0, "computer");
    assert_eq!(done[0].1, PartStatus::Left);
    assert_eq!(done[1].1, PartStatus::Done);
    rig.daemon.shutdown().await;
}

/// A device or scene the user named with the word "custom" in it keeps its own spelling in the
/// notes: only the computer part's `custom` is shown as `Custom`.
#[tokio::test]
async fn a_users_own_custom_in_a_name_is_left_alone() {
    let desk = stand("bifrost3", "My custom DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], false, "").await;
    let desk = &rig.stands[0];
    let mut tone = tone_of(&desk.profile, "Bass");
    tone.name = "my custom tone".into();
    tone.band.pop();
    tone.bands -= 1;
    rig.add(&[scene("Odd", None, vec![part(desk, Some(tone), &[])])])
        .await;
    let done = outcomes(rig.recall("Odd").await);
    assert_eq!(done[0].0, "My custom DAC");
    assert!(
        done[0].2.iter().any(|note| note.contains("my custom tone")),
        "{:?}",
        done[0].2
    );
    rig.daemon.shutdown().await;
}

/// A save names the configured device that is not connected as a part left out of the scene.
#[tokio::test]
async fn a_save_names_a_device_that_is_not_connected() {
    let kara = "[[device]]\nname = \"Kara\"\nudid = \"0000000000000ABC\"\n";
    let rig = Rig::new(
        vec![stand("bifrost3", "Desk DAC", BIFROST, &[])],
        false,
        kara,
    )
    .await;
    rig.daemon.reload_all().await.expect("the reload");
    let done = outcomes(rig.save("Late night").await);
    assert_eq!(
        done,
        [(
            "Kara".to_string(),
            PartStatus::Left,
            vec!["Kara is not connected: it is not in the scene".to_string()]
        )]
    );
    rig.daemon.shutdown().await;
}

/// The memory is compared and changed in one step: a rename or a delete of another scene leaves
/// what a recall set, and one of the recalled scene follows or forgets it.
#[tokio::test]
async fn the_memory_changes_only_for_the_scene_it_holds() {
    let rig = Rig::new(Vec::new(), false, "").await;
    let scenes = rig.daemon.scenes();
    scenes.set_recalled(Some("Two".into()));
    assert!(!scenes.renamed("One", "First"), "another scene's rename");
    assert!(!scenes.forgotten("One"), "another scene's delete");
    assert_eq!(scenes.recalled().as_deref(), Some("Two"));
    assert!(scenes.renamed(" two ", "Second"));
    assert_eq!(scenes.recalled().as_deref(), Some("Second"));
    assert!(scenes.forgotten("SECOND"));
    assert_eq!(scenes.recalled(), None);
}

/// Held toggles the device already reads, reported as booleans, with nothing in the line: nothing
/// is written (a boolean has no integer form of its own, so a reader that skips the mapping would
/// find the toggle different on every recall).
#[tokio::test]
async fn a_toggle_already_there_is_not_written() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    let toggles: Vec<(String, i64)> = scene::held_controls(&desk.profile)
        .iter()
        .filter(|control| control.kind == ControlKind::Toggle)
        .map(|control| {
            let step = scene::raw_of(&desk.readings())(control.id).expect("a reading");
            (control.key.clone(), step)
        })
        .collect();
    assert!(!toggles.is_empty(), "the Bifrost 3 holds toggles");
    let controls: Vec<(&str, i64)> = toggles
        .iter()
        .map(|(key, step)| (key.as_str(), *step))
        .collect();
    rig.add(&[scene("Same", None, vec![part(desk, None, &controls)])])
        .await;
    let done = outcomes(rig.recall("Same").await);
    assert_eq!(done[0].1, PartStatus::Done);
    assert!(desk.log().is_empty(), "{:?}", desk.log());
    rig.daemon.shutdown().await;
}

/// A write sent without waiting is answered `Ack` before its echo comes, and the feed still reads
/// the old step: a recall of a scene holding that old step must still write it, so the device ends
/// on the scene's input and the answer is true.
#[tokio::test]
async fn a_no_wait_write_whose_echo_is_late_does_not_overtake_the_recall() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    let (first, other) = (desk.first("input"), desk.other("input"));
    rig.add(&[scene(
        "Home",
        None,
        vec![part(desk, None, &[("input", first)])],
    )])
    .await;
    desk.late_echo
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let live = desk.handle.writer.submit(
        Origin::Mcp,
        Command::Set {
            key: "input".into(),
            value: other.to_string(),
            step: 1,
            no_wait: true,
        },
        crate::writer::Mode::Ordered,
    );
    live.await.expect("the no-wait write was answered");
    let done = outcomes(rig.recall("Home").await);
    desk.late_echo
        .store(false, std::sync::atomic::Ordering::SeqCst);
    desk.flush_late();
    assert_eq!(done[0].1, PartStatus::Done, "{done:?}");
    assert_eq!(
        desk.log(),
        [format!("set input {other}"), format!("set input {first}")],
        "the recall wrote the input behind the live write"
    );
    rig.daemon.shutdown().await;
}

/// A hand-edited part that names the volume, a gain, a mute or a power switch writes none of them:
/// each is skipped in words and only a held control goes out.
#[tokio::test]
async fn a_hand_made_part_naming_the_volume_or_a_gain_writes_nothing_of_it() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[("volume", 40)]);
    let amp = stand("lyr5", "Amp", LYR, &[("volume", 30)]);
    let rig = Rig::new(vec![desk, amp], false, "").await;
    let (desk, amp) = (&rig.stands[0], &rig.stands[1]);
    rig.add(&[scene(
        "Sneaky",
        None,
        vec![
            part(
                desk,
                None,
                &[("VOLUME", 90), ("input", desk.other("input"))],
            ),
            part(
                amp,
                None,
                &[
                    ("volume", 90),
                    ("gain", 1),
                    ("headphone_mute", 1),
                    ("preout", 1),
                ],
            ),
        ],
    )])
    .await;
    let done = outcomes(rig.recall("Sneaky").await);
    assert_eq!(desk.log(), [format!("set input {}", desk.other("input"))]);
    assert!(amp.log().is_empty(), "{:?}", amp.log());
    assert_eq!(done[0].1, PartStatus::Partial);
    assert_eq!(done[1].1, PartStatus::Partial);
    for key in ["volume", "gain", "headphone_mute", "preout"] {
        assert!(
            done[1]
                .2
                .iter()
                .any(|note| note.starts_with(&format!("{key}:"))),
            "{key}: {:?}",
            done[1].2
        );
    }
    assert_eq!(desk.raw("volume"), Some(40));
    assert_eq!(amp.raw("volume"), Some(30));
    rig.daemon.shutdown().await;
}

/// A no-wait write whose echo has come in (the feed heard the control since) is no longer
/// suspect: a recall that wants the step the feed now reads sends nothing.
#[tokio::test]
async fn a_no_wait_write_whose_echo_came_in_is_not_written_again() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    let other = desk.other("input");
    rig.add(&[scene(
        "Away",
        None,
        vec![part(desk, None, &[("input", other)])],
    )])
    .await;
    desk.late_echo
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let live = desk.handle.writer.submit(
        Origin::Mcp,
        Command::Set {
            key: "input".into(),
            value: other.to_string(),
            step: 1,
            no_wait: true,
        },
        crate::writer::Mode::Ordered,
    );
    live.await.expect("answered");
    desk.flush_late();
    desk.late_echo
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let done = outcomes(rig.recall("Away").await);
    assert_eq!(done[0].1, PartStatus::Done, "{done:?}");
    assert_eq!(desk.log(), [format!("set input {other}")], "nothing resent");
    rig.daemon.shutdown().await;
}

/// Two no-wait writes to one control with only the first echo in: the feed has heard the control,
/// but the second write's echo is still to come, so a recall that wants the first value writes it.
#[tokio::test]
async fn two_no_wait_writes_with_only_the_first_echo_in_do_not_let_a_recall_skip() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    let values = desk
        .profile
        .control_by_key("input")
        .expect("an input")
        .values
        .iter()
        .map(|(raw, _)| i64::from(*raw))
        .collect::<Vec<_>>();
    let (second, third) = (values[1], values[2]);
    rig.add(&[scene(
        "Back",
        None,
        vec![part(desk, None, &[("input", second)])],
    )])
    .await;
    desk.late_echo
        .store(true, std::sync::atomic::Ordering::SeqCst);
    for value in [second, third] {
        let sent = desk.handle.writer.submit(
            Origin::Socket,
            Command::Set {
                key: "input".into(),
                value: value.to_string(),
                step: 1,
                no_wait: true,
            },
            crate::writer::Mode::Ordered,
        );
        sent.await.expect("answered");
    }
    desk.flush_next();
    desk.late_echo
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let done = outcomes(rig.recall("Back").await);
    assert_eq!(done[0].1, PartStatus::Done, "{done:?}");
    assert_eq!(
        desk.log(),
        [
            format!("set input {second}"),
            format!("set input {third}"),
            format!("set input {second}")
        ],
        "the recall wrote the first value again behind both"
    );
    desk.flush_late();
    rig.daemon.shutdown().await;
}
