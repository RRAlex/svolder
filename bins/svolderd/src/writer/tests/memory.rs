// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A recall sets the memory as its plan starts, whoever asked for it, in the preset's own
/// spelling; a socket client's note of its own recall sets it too.
#[tokio::test]
async fn the_memory_is_the_last_recall_any_front_end_made() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let fitting = fitting_of(&stand);
    let recalled = writer.recalled();
    assert_eq!(*recalled.borrow(), None);
    let Recalling { planned, done } = writer.recall(Origin::Mqtt, "warm".into(), fitting.clone());
    assert!(matches!(planned.await, Ok(Planned::Plan(_))));
    assert_eq!(recalled.borrow().as_deref(), Some("Warm"));
    done.await.expect("an answer");
    writer.note_recalled("Bass".into());
    writer.view().await;
    assert_eq!(recalled.borrow().as_deref(), Some("Bass"));
    let Recalling { planned, .. } = writer.recall(Origin::Mcp, "Nope".into(), fitting);
    assert!(matches!(planned.await, Ok(Planned::Refused(_))));
    assert_eq!(
        recalled.borrow().as_deref(),
        Some("Bass"),
        "a refused recall sets nothing"
    );
}

/// The climb is published as it starts, as its owner steers it, and as it ends.
#[tokio::test(start_paused = true)]
async fn the_climb_is_published_as_it_starts_steers_and_ends() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    let climb = writer.climb();
    assert_eq!(*climb.borrow(), None);
    glide(&writer, Origin::Mqtt, 120).await;
    writer.view().await;
    assert_eq!(
        *climb.borrow(),
        Some(ClimbView {
            key: "volume".into(),
            target: 120
        })
    );
    glide(&writer, Origin::Mqtt, 127).await;
    writer.view().await;
    assert_eq!(climb.borrow().as_ref().map(|c| c.target), Some(127));
    writer
        .submit(Origin::Socket, set("volume", "60"), Mode::Ordered)
        .await
        .expect("an answer");
    writer.view().await;
    assert_eq!(*climb.borrow(), None);
    drop(stand);
}

/// A tone named `Custom` is the name no preset can carry: recalling one (a scene's curve that no
/// preset matched) clears the memory instead of noting a preset that does not exist, so no front
/// end shows it and the recall history never lists it.
#[tokio::test]
async fn a_tone_named_custom_clears_the_memory_instead_of_noting_it() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let fitting = fitting_of(&stand);
    let recalled = writer.recalled();
    let Recalling { planned, done } = writer.recall(Origin::Mqtt, "Warm".into(), fitting.clone());
    assert!(matches!(planned.await, Ok(Planned::Plan(_))));
    done.await.expect("an answer");
    assert_eq!(recalled.borrow().as_deref(), Some("Warm"));

    let mut custom = fitting[0].clone();
    custom.name = "Custom".to_string();
    let Recalling { planned, done } = writer.recall(Origin::Web, "Custom".into(), vec![custom]);
    assert!(matches!(planned.await, Ok(Planned::Plan(_))));
    done.await.expect("an answer");
    assert_eq!(
        *recalled.borrow(),
        None,
        "no preset named Custom exists to be remembered"
    );
}

/// A recall or a setting that runs or waits is published as the writer being busy, and the signal
/// ends with its last write; a plain write is not one.
#[tokio::test]
async fn the_writer_says_while_a_recall_or_a_setting_runs_or_waits() {
    let (writer, stand) = loki([15; 6]);
    let busy = writer.busy();
    assert!(!*busy.borrow());
    let plain = writer.submit(Origin::Socket, set("band5", "20"), Mode::Ordered);
    writer.view().await;
    assert!(!*busy.borrow(), "a plain write is no batch");
    plain.await.expect("an answer");

    stand.gate.forget_permits(1000);
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    writer.view().await;
    assert!(*busy.borrow(), "a setting is running");
    stand.gate.add_permits(1000);
    assert!(matches!(planned.await, Ok(Planned::Setting(_))));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    writer.view().await;
    assert!(!*busy.borrow(), "the batch ended");
}
