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
