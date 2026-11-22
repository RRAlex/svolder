// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Recalling a scene.

use svolder_host::eq::LEVEL_HELD_NOTE;
use svolder_ipc::socket::protocol::{PartStatus, SceneOp};

use super::*;

const KARA: &str = "0000000000000ABC";

/// The bifrost's `Bass` as a scene's tone, with `controls` set, for a scene of `name` that also
/// holds a computer part.
fn late_night(desk: &Stand) -> Scene {
    scene(
        "Late night",
        Some(computer_part("Desk", vec![pk(100.0, 2.0, 1.0)], 0.0)),
        vec![part(
            desk,
            Some(tone_of(&desk.profile, "Bass")),
            &[("input", desk.other("input"))],
        )],
    )
}

/// A recall issues the device's batch and the computer's change, answers a part each (the
/// computer's first), and the scene is then the one that is on.
#[tokio::test]
async fn a_recall_changes_the_computer_and_the_device_and_answers_each_part() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], true, "").await;
    let desk = &rig.stands[0];
    rig.add(&[late_night(desk)]).await;
    assert_eq!(rig.view().on, None);

    let answer = rig.recall("late night").await;
    let Response::Scene { name, outcome } = &answer else {
        panic!("{answer:?}");
    };
    assert_eq!(name, "Late night");
    assert_eq!(
        outcomes(answer.clone()),
        [
            ("computer".to_string(), PartStatus::Done, Vec::new()),
            ("Desk DAC".to_string(), PartStatus::Done, Vec::new()),
        ],
        "{outcome:?}"
    );
    assert_eq!(applies(&rig.chain), 1, "the computer changed once");
    assert!(rig.eq().on);
    let log = desk.log();
    let wanted = desk.other("input");
    assert_eq!(desk.raw("input"), Some(wanted), "{log:?}");
    assert!(
        log.iter().any(|line| line.starts_with("set input ")),
        "{log:?}"
    );
    assert!(log.iter().any(|line| line.starts_with("eq ")), "{log:?}");

    let view = rig.view_when("the scene on", |v| v.on.is_some()).await;
    assert_eq!(view.on.as_deref(), Some("Late night"));
    assert_eq!(view.recalled.as_deref(), Some("Late night"));
    assert!(view.known);
    rig.daemon.shutdown().await;
}

/// A part of a device that is not connected is left as it is, in words naming it, and the rest of
/// the scene is recalled.
#[tokio::test]
async fn a_device_that_is_not_connected_is_left_in_words() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let kara = format!("[[device]]\nname = \"Kara\"\nudid = \"{KARA}\"\n");
    let rig = Rig::new(vec![desk], true, &kara).await;
    let desk = &rig.stands[0];
    let mut scene = late_night(desk);
    scene.devices.push(DevicePart {
        udid: KARA.into(),
        name: "Kara at save".into(),
        tone: None,
        raw_bands: None,
        controls: vec![("input".into(), 1)],
    });
    rig.add(&[scene]).await;

    let done = outcomes(rig.recall("Late night").await);
    assert_eq!(
        done.iter()
            .map(|(who, status, _)| (who.as_str(), *status))
            .collect::<Vec<_>>(),
        [
            ("computer", PartStatus::Done),
            ("Desk DAC", PartStatus::Done),
            ("Kara", PartStatus::Left),
        ],
        "the name the config gives now"
    );
    assert_eq!(
        done[2].2,
        ["Kara is not connected: its part was left as it is"]
    );
    assert_eq!(
        desk.raw("input"),
        Some(desk.other("input")),
        "the connected part was recalled"
    );
    let view = rig.view();
    assert_eq!(view.scenes[0].parts.last().map(|p| p.present), Some(false));
    rig.daemon.shutdown().await;
}

/// A scene that would leave one preset on a device and on the computer asks first, changing
/// nothing; `confirm` recalls it as saved, and no device is flattened.
#[tokio::test]
async fn a_preset_on_both_sides_asks_and_confirm_recalls_as_saved() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], true, "").await;
    let desk = &rig.stands[0];
    let warm = tone_of(&desk.profile, "Warm");
    let both = scene(
        "Late night",
        Some(computer_part("warm", vec![pk(100.0, 3.0, 1.0)], 0.0)),
        vec![part(desk, Some(warm), &[])],
    );
    rig.add(&[both]).await;

    let asked = rig.recall("Late night").await;
    assert_eq!(
        asked,
        Response::Ask {
            message: "Late night leaves Warm on the Desk DAC and on the computer: it plays twice. \
                      Recall it anyway?"
                .into(),
            name: "Warm".into(),
            devices: vec!["Desk DAC".into()],
        }
    );
    assert!(desk.log().is_empty(), "nothing was sent: {:?}", desk.log());
    assert_eq!(applies(&rig.chain), 0);
    assert_eq!(
        rig.view().recalled,
        None,
        "an unanswered question remembers nothing"
    );

    let answer = rig
        .ask(SceneOp::Recall {
            name: "Late night".into(),
            confirm: true,
        })
        .await;
    assert_eq!(
        outcomes(answer)
            .iter()
            .map(|(_, status, _)| *status)
            .collect::<Vec<_>>(),
        [PartStatus::Done, PartStatus::Done]
    );
    assert_eq!(applies(&rig.chain), 1);
    let log = desk.log();
    let eq = desk.profile.eq.as_ref().expect("a tone control").clone();
    let warm = tone_of(&desk.profile, "Warm");
    assert!(
        preset::matches(&eq, |id| reading(&desk.readings(), id), &warm),
        "the device plays Warm as saved: {log:?}"
    );
    rig.daemon.shutdown().await;
}

/// A computer part under another name asks nothing, nor does a preset that changes nothing, nor
/// `custom` on both sides.
#[tokio::test]
async fn only_the_same_non_neutral_preset_on_both_sides_asks() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], true, "").await;
    let desk = &rig.stands[0];
    let part = |tone: Preset| part(desk, Some(tone), &[]);
    let mut custom = tone_of(&desk.profile, "Warm");
    custom.name = "Custom".into();
    rig.add(&[
        scene(
            "Other name",
            Some(computer_part("Mine", vec![pk(100.0, 3.0, 1.0)], 0.0)),
            vec![part(tone_of(&desk.profile, "Warm"))],
        ),
        scene(
            "Flat both",
            Some(computer_part("Flat", vec![pk(100.0, 0.0, 1.0)], 0.0)),
            vec![part(tone_of(&desk.profile, "Flat"))],
        ),
        scene(
            "Custom both",
            Some(computer_part("custom", vec![pk(100.0, 3.0, 1.0)], 0.0)),
            vec![part(custom)],
        ),
    ])
    .await;
    for name in ["Other name", "Flat both", "Custom both"] {
        let answer = rig.recall(name).await;
        assert!(
            matches!(answer, Response::Scene { .. }),
            "{name}: {answer:?}"
        );
    }
    rig.daemon.shutdown().await;
}

/// Recall `name` and expect a scene answer, not the question.
async fn recalled(rig: &Rig, name: &str) {
    let answer = rig.recall(name).await;
    assert!(
        matches!(answer, Response::Scene { .. }),
        "{name}: {answer:?}"
    );
}

/// A rig of the Desk DAC and the Modi, a computer, and scenes that put `Warm` on the Modi
/// (`Modi warm`), `Warm` on the computer (`Computer warm`) and `Bass` on the Modi (`Modi bass`).
async fn desk_and_modi() -> Rig {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let modi = stand("bifrost3", "Modi", KARA, &[]);
    let rig = Rig::new(vec![desk, modi], true, "").await;
    let (desk, modi) = (&rig.stands[0], &rig.stands[1]);
    let warm = || computer_part("Warm", vec![pk(100.0, 3.0, 1.0)], 0.0);
    rig.add(&[
        scene(
            "Modi warm",
            None,
            vec![part(modi, Some(tone_of(&modi.profile, "Warm")), &[])],
        ),
        scene(
            "Modi bass",
            None,
            vec![part(modi, Some(tone_of(&modi.profile, "Bass")), &[])],
        ),
        scene("Computer warm", Some(warm()), vec![]),
        scene(
            "Late night",
            Some(warm()),
            vec![part(desk, Some(tone_of(&desk.profile, "Bass")), &[])],
        ),
    ])
    .await;
    rig
}

/// A device the scene has no part for (it was away when the scene was saved) and that plays the
/// preset the computer is left on makes the question.
#[tokio::test]
async fn a_device_the_scene_has_no_part_for_counts_with_what_it_plays() {
    let rig = desk_and_modi().await;
    recalled(&rig, "Modi warm").await;
    assert_eq!(
        rig.recall("Late night").await,
        Response::Ask {
            message: "Late night leaves Warm on the Modi and on the computer: it plays twice. \
                      Recall it anyway?"
                .into(),
            name: "Warm".into(),
            devices: vec!["Modi".into()],
        }
    );
    assert_eq!(applies(&rig.chain), 0, "nothing changed");
    rig.daemon.shutdown().await;
}

/// A tone the device cannot play leaves it on what it plays, which still counts.
#[tokio::test]
async fn a_tone_the_device_cannot_play_counts_with_what_it_plays() {
    let rig = desk_and_modi().await;
    let modi = &rig.stands[1];
    let mut short = tone_of(&modi.profile, "Bass");
    short.band.pop();
    short.bands -= 1;
    recalled(&rig, "Modi warm").await;
    rig.add(&[scene(
        "Short",
        Some(computer_part("Warm", vec![pk(100.0, 3.0, 1.0)], 0.0)),
        vec![part(modi, Some(short), &[])],
    )])
    .await;
    let answer = rig.recall("Short").await;
    let Response::Ask { devices, name, .. } = answer else {
        panic!("{answer:?}");
    };
    assert_eq!(
        (name.as_str(), devices),
        ("Warm", vec!["Modi".to_string(); 1])
    );
    rig.daemon.shutdown().await;
}

/// A scene with no computer part leaves the computer as it plays, which counts.
#[tokio::test]
async fn a_scene_with_no_computer_part_counts_what_the_computer_plays() {
    let rig = desk_and_modi().await;
    recalled(&rig, "Computer warm").await;
    let answer = rig.recall("Modi warm").await;
    let Response::Ask { devices, name, .. } = answer else {
        panic!("{answer:?}");
    };
    assert_eq!(
        (name.as_str(), devices),
        ("Warm", vec!["Modi".to_string(); 1])
    );
    assert_eq!(
        outcomes(
            rig.ask(SceneOp::Recall {
                name: "Modi warm".into(),
                confirm: true,
            })
            .await
        )
        .len(),
        1,
        "confirmed, it is recalled as saved"
    );
    rig.daemon.shutdown().await;
}

/// No question when no preset plays on both sides.
#[tokio::test]
async fn nothing_that_doubles_asks_nothing() {
    let rig = desk_and_modi().await;
    // The Modi plays Bass, the computer is left on Warm: two different presets.
    recalled(&rig, "Modi bass").await;
    recalled(&rig, "Late night").await;
    // The computer plays Warm and a scene with no computer part leaves the Modi on Bass.
    recalled(&rig, "Modi bass").await;
    rig.daemon.shutdown().await;
}

/// A hand-edited part key that differs in case from the profile's is skipped by the plan's own
/// words: the part is partial with that one note, not failed with two.
#[tokio::test]
async fn a_part_key_in_another_case_is_skipped_by_the_plans_note() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], false, "").await;
    let desk = &rig.stands[0];
    rig.add(&[scene("Odd", None, vec![part(desk, None, &[("Input", 99)])])])
        .await;
    let done = outcomes(rig.recall("Odd").await);
    assert_eq!(done[0].1, PartStatus::Partial, "{:?}", done[0].2);
    assert_eq!(done[0].2.len(), 1, "{:?}", done[0].2);
    assert!(
        desk.log()
            .iter()
            .all(|line| !line.to_ascii_lowercase().starts_with("set input ")),
        "{:?}",
        desk.log()
    );
    rig.daemon.shutdown().await;
}

/// A device that goes silent mid-recall is answered for within the 120 s the daemon promises:
/// the part has one deadline, not one per wait (the tone's end, then each control's answer).
#[tokio::test(start_paused = true)]
async fn a_silent_device_is_answered_for_within_the_daemons_worst_case() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], false, "").await;
    let desk = &rig.stands[0];
    rig.add(&[scene(
        "Quiet",
        None,
        vec![part(
            desk,
            Some(tone_of(&desk.profile, "Bass")),
            &[("input", desk.other("input"))],
        )],
    )])
    .await;
    // A preset recall under way holds the writer, so the part's tone and control wait behind it.
    desk.gate.forget_permits(10_000);
    let fitting = preset::builtins(desk.profile.eq.as_ref().expect("a tone control"));
    let _held = desk
        .handle
        .writer
        .recall(Origin::Socket, "Warm".into(), fitting);
    let began = tokio::time::Instant::now();
    let done = outcomes(rig.recall("Quiet").await);
    let took = began.elapsed();
    assert!(
        took < Duration::from_secs(120),
        "the part was answered after {took:?}"
    );
    assert_eq!(done[0].1, PartStatus::Failed, "{done:?}");
    desk.gate.add_permits(10_000);
    rig.daemon.shutdown().await;
}

/// A scene recall as a front end sends it.
fn scene_recall_request(name: &str) -> Request {
    Request::Scene {
        op: SceneOp::Recall {
            name: name.into(),
            confirm: false,
        },
    }
}

/// Two recalls reach the device in the order they arrived even when the first waits for the fit
/// of an imported preset before it asks the both-sides question: the second one's input lands
/// last, as it arrived last.
#[tokio::test]
async fn recalls_reach_the_device_in_arrival_order_when_the_first_waits_on_a_fit() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], true, "").await;
    let desk = &rig.stands[0];
    let slow = crate::recall::fits::tests::imported("slow-fit-scene-order", 2.5);
    Config::add_preset(&rig.config, &slow).expect("a preset is saved");
    let (other, first) = (desk.other("input"), desk.first("input"));
    rig.add(&[
        scene(
            "First",
            Some(computer_part(
                "slow-fit-scene-order",
                vec![pk(100.0, 3.0, 1.0)],
                0.0,
            )),
            vec![part(desk, None, &[("input", other)])],
        ),
        scene("Second", None, vec![part(desk, None, &[("input", first)])]),
    ])
    .await;
    assert_eq!(
        desk.raw("input"),
        Some(first),
        "the stand reads the first value"
    );
    let (a, b) = (Arc::clone(&rig.daemon), Arc::clone(&rig.daemon));
    let one = tokio::spawn(async move {
        a.handle_from(Origin::Web, scene_recall_request("First"))
            .await
    });
    // The first recall is waiting for its fit when the second arrives.
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    let two = tokio::spawn(async move {
        b.handle_from(Origin::Web, scene_recall_request("Second"))
            .await
    });
    let one = outcomes(one.await.expect("the first recall ended"));
    let two = outcomes(two.await.expect("the second recall ended"));
    assert!(
        one.iter()
            .chain(&two)
            .all(|part| part.1 == PartStatus::Done),
        "{one:?} {two:?}"
    );
    assert_eq!(
        desk.raw("input"),
        Some(first),
        "the second recall arrived last and wrote last: {:?}",
        desk.log()
    );
    assert_eq!(rig.view().recalled.as_deref(), Some("Second"));
    rig.daemon.shutdown().await;
}

/// A tone outside a band's window is clamped by the writer, and the outcome says so in its words
/// and is partial.
#[tokio::test]
async fn a_clamped_tone_is_a_partial_outcome_with_the_writers_words() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], false, "").await;
    let desk = &rig.stands[0];
    let mut tone = tone_of(&desk.profile, "Bass");
    tone.band[0].f = 20_000.0;
    rig.add(&[scene("Bright", None, vec![part(desk, Some(tone), &[])])])
        .await;
    let done = outcomes(rig.recall("Bright").await);
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].0, "Desk DAC");
    assert_eq!(done[0].1, PartStatus::Partial, "{:?}", done[0].2);
    assert!(
        done[0].2.iter().any(|note| note.contains("clamped to")),
        "{:?}",
        done[0].2
    );
    rig.daemon.shutdown().await;
}

/// A curve the part names as `custom` shows as `Custom` in every note.
#[tokio::test]
async fn custom_shows_title_cased_in_a_note() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], false, "").await;
    let desk = &rig.stands[0];
    let mut tone = tone_of(&desk.profile, "Bass");
    tone.name = "Custom".into();
    tone.band.pop();
    tone.bands -= 1;
    rig.add(&[scene("Odd", None, vec![part(desk, Some(tone), &[])])])
        .await;
    let done = outcomes(rig.recall("Odd").await);
    assert_eq!(done[0].1, PartStatus::Partial, "{:?}", done[0].2);
    assert!(
        done[0].2.iter().all(|note| !note.contains("custom")),
        "{:?}",
        done[0].2
    );
    rig.daemon.shutdown().await;
}

/// The computer's notes are the outcome's: a level held for a stream that plays says so, and the
/// part still counts as set.
#[tokio::test]
async fn the_computer_outcome_passes_the_held_level_words() {
    let rig = Rig::new(Vec::new(), true, "").await;
    rig.chain.restarts_node_of(&rig.params);
    rig.add(&[
        scene(
            "Hot",
            Some(computer_part("Hot", vec![pk(100.0, 10.0, 1.0)], 0.0)),
            Vec::new(),
        ),
        scene(
            "Mild",
            Some(computer_part("Mild", vec![pk(100.0, 2.0, 1.0)], 0.0)),
            Vec::new(),
        ),
    ])
    .await;
    assert_eq!(outcomes(rig.recall("Hot").await)[0].1, PartStatus::Done);
    rig.params.set_playing(true);
    let done = outcomes(rig.recall("Mild").await);
    assert_eq!(
        done,
        [(
            "computer".to_string(),
            PartStatus::Done,
            vec![LEVEL_HELD_NOTE.to_string()]
        )]
    );
    rig.daemon.shutdown().await;
}

/// A computer part the chain refuses is left, in the controller's words, and the devices' parts
/// are recalled all the same.
#[tokio::test]
async fn a_refused_computer_part_is_left_and_holds_nothing_back() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], true, "").await;
    let desk = &rig.stands[0];
    rig.add(&[scene(
        "Too many",
        Some(computer_part("Desk", vec![pk(100.0, 2.0, 1.0); 17], 0.0)),
        vec![part(desk, None, &[("input", desk.other("input"))])],
    )])
    .await;
    let done = outcomes(rig.recall("Too many").await);
    assert_eq!(done[0].0, "computer");
    assert_eq!(done[0].1, PartStatus::Left);
    assert!(done[0].2[0].contains("16"), "{:?}", done[0].2);
    assert_eq!(
        done[1],
        ("Desk DAC".to_string(), PartStatus::Done, Vec::new())
    );
    assert_eq!(desk.raw("input"), Some(desk.other("input")));
    assert_eq!(applies(&rig.chain), 0, "the chain was left as it was");
    rig.daemon.shutdown().await;
}

/// No volume, gain select, mute or power command reaches any device while a scene is recalled:
/// every command a stand-in took is a tone write or a held control's.
#[tokio::test]
async fn no_volume_gain_mute_or_power_reaches_any_device() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[("volume", 40)]);
    let amp = stand("lyr5", "Amp", LYR, &[("volume", 30)]);
    let rig = Rig::new(vec![desk, amp], false, "").await;
    let (desk, amp) = (&rig.stands[0], &rig.stands[1]);
    let held = |on: &Stand| -> Vec<String> {
        scene::held_controls(&on.profile)
            .iter()
            .map(|control| control.key.clone())
            .collect()
    };
    assert!(
        !held(amp)
            .iter()
            .any(|key| key == "gain" || key == "headphone_mute" || key == "volume"),
        "{:?}",
        held(amp)
    );
    let mut amp_part = part(amp, None, &[("input", amp.other("input"))]);
    if let Some(mode) = amp
        .profile
        .control_by_key("mode")
        .filter(|c| c.kind == ControlKind::Select)
    {
        let last = mode
            .values
            .last()
            .map(|(raw, _)| i64::from(*raw))
            .expect("a value");
        amp_part.controls.push(("mode".into(), last));
    }
    rig.add(&[scene(
        "Movie",
        None,
        vec![
            part(
                desk,
                Some(tone_of(&desk.profile, "Warm")),
                &[("input", desk.other("input"))],
            ),
            amp_part,
        ],
    )])
    .await;
    let done = outcomes(rig.recall("Movie").await);
    assert_eq!(
        done.iter()
            .map(|(_, status, _)| *status)
            .collect::<Vec<_>>(),
        [PartStatus::Done, PartStatus::Done],
        "{done:?}"
    );
    let forbidden = [
        "volume",
        "gain",
        "headphone_mute",
        "preout",
        "power",
        "mute",
    ];
    for stand in [desk, amp] {
        let log = stand.log();
        assert!(!log.is_empty(), "something was recalled: {log:?}");
        for line in &log {
            let named = forbidden
                .iter()
                .any(|key| line.starts_with(&format!("set {key} ")));
            assert!(!named, "{line}");
            let known = line.starts_with("eq ")
                || line.starts_with("mode ")
                || line.starts_with("set input ")
                || line.starts_with("set mode ");
            assert!(known, "an unplanned command: {line}");
        }
    }
    assert_eq!(desk.raw("volume"), Some(40));
    assert_eq!(amp.raw("volume"), Some(30));
    rig.daemon.shutdown().await;
}

/// A Loki part sets its raw bands by the value form: the bands going down first, each waited for,
/// and a band write that arrives meanwhile waits for them.
#[tokio::test]
async fn a_loki_part_goes_out_as_values_cuts_first_and_holds_a_band_write() {
    let loki = stand("lokimax", "Loki", LOKI, &[("band4", 20)]);
    let rig = Rig::new(vec![loki], false, "").await;
    let loki = &rig.stands[0];
    let mut bands = part(loki, None, &[]);
    bands.raw_bands = Some(vec![
        ("band1".into(), 28),
        ("band2".into(), 15),
        ("band3".into(), 15),
        ("band4".into(), 15),
        ("band5".into(), 15),
        ("band6".into(), 15),
    ]);
    rig.add(&[scene("Bass", None, vec![bands])]).await;

    loki.gate.forget_permits(10_000);
    let daemon = Arc::clone(&rig.daemon);
    let recalling = tokio::spawn(async move {
        daemon
            .handle_from(
                Origin::Socket,
                Request::Scene {
                    op: SceneOp::Recall {
                        name: "Bass".into(),
                        confirm: false,
                    },
                },
            )
            .await
    });
    // The band write is asked once the batch is in the writer's line.
    let mut busy = loki.handle.writer.busy();
    tokio::time::timeout(Duration::from_secs(5), busy.wait_for(|busy| *busy))
        .await
        .expect("the batch started")
        .expect("the writer is there");
    let band = loki.handle.writer.submit(
        Origin::Web,
        svolder_test_set("band5", "3"),
        crate::writer::Mode::Ordered,
    );
    assert_eq!(
        loki.handle.writer.view().await.held,
        1,
        "the band write waits"
    );
    loki.gate.add_permits(10_000);
    let done = outcomes(recalling.await.expect("the recall ended"));
    assert_eq!(done, [("Loki".to_string(), PartStatus::Done, Vec::new())]);
    band.await.expect("the band write was answered");
    let log = loki.log();
    assert_eq!(log.len(), 3, "{log:?}");
    assert!(
        log[0].starts_with("raw ") && log[0].ends_with(" 15"),
        "the cut first: {log:?}"
    );
    assert!(
        log[1].starts_with("raw ") && log[1].ends_with(" 28"),
        "then the raise: {log:?}"
    );
    assert_eq!(log[2], "set band5 3", "the band write after the batch");
    rig.daemon.shutdown().await;
}

/// A `set` as a front end sends it.
fn svolder_test_set(key: &str, value: &str) -> Command {
    Command::Set {
        key: key.into(),
        value: value.into(),
        step: 1,
        no_wait: false,
    }
}

/// A scene recall that arrives behind a running preset recall waits its turn in the device's
/// writer, and a write that arrives after it lands after its batch.
#[tokio::test]
async fn a_recall_waits_behind_a_running_preset_recall_and_a_later_write_follows() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], false, "").await;
    let desk = &rig.stands[0];
    rig.add(&[scene(
        "Bass",
        None,
        vec![part(desk, Some(tone_of(&desk.profile, "Bass")), &[])],
    )])
    .await;
    desk.gate.forget_permits(10_000);
    let fitting = preset::builtins(desk.profile.eq.as_ref().expect("a tone control"));
    let first = desk
        .handle
        .writer
        .recall(Origin::Socket, "Warm".into(), fitting);
    assert!(desk.handle.writer.view().await.recall_running);
    let daemon = Arc::clone(&rig.daemon);
    let scene_recall = tokio::spawn(async move {
        daemon
            .handle_from(
                Origin::Web,
                Request::Scene {
                    op: SceneOp::Recall {
                        name: "Bass".into(),
                        confirm: false,
                    },
                },
            )
            .await
    });
    // The preset recall is ask 1, the look at the writer ask 2, the scene's tone ask 3: it waits
    // behind the running one.
    let mut arrivals = desk.handle.writer.arrivals();
    tokio::time::timeout(
        Duration::from_secs(5),
        arrivals.wait_for(|taken| *taken >= 3),
    )
    .await
    .expect("the scene's recall arrived")
    .expect("the writer is there");
    assert_eq!(
        desk.handle.writer.view().await.recall_waiting.as_deref(),
        Some("Bass")
    );
    let later = desk.handle.writer.submit(
        Origin::Mcp,
        Command::EqSet {
            band: 3,
            param: "g".into(),
            value: 1.0,
            no_wait: false,
        },
        crate::writer::Mode::Ordered,
    );
    desk.gate.add_permits(10_000);
    first.done.await.expect("the preset recall ended");
    let done = outcomes(scene_recall.await.expect("the scene recall ended"));
    assert_eq!(done[0].1, PartStatus::Done);
    later.await.expect("the later write was answered");
    let log = desk.log();
    assert_eq!(log.last().map(String::as_str), Some("eq 3 g 1"), "{log:?}");
    // The running recall's raise of band 5 comes first, the scene's cut of it after.
    let raised = log.iter().position(|line| line.starts_with("eq 5 g 2."));
    let cut = log.iter().position(|line| line == "eq 5 g 0");
    assert!(
        matches!((raised, cut), (Some(raised), Some(cut)) if raised < cut),
        "the scene waited its turn: {log:?}"
    );
    rig.daemon.shutdown().await;
}
