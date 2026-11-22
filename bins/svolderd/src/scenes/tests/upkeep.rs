// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rename and delete, and the memory of the scene last recalled: in the process, in its file,
//! and across a restart.

use svolder_ipc::Reason;
use svolder_ipc::socket::protocol::SceneOp;

use super::*;

/// The computer part both twins hold.
fn twin_part() -> svolder_host::eq::ComputerPart {
    computer_part("Desk", vec![pk(100.0, 2.0, 1.0)], 0.0)
}

/// Two scenes of one computer part, which both match what the computer plays once either has
/// been recalled: only the memory tells which of them is on.
fn twins(_rig: &Rig) -> Vec<Scene> {
    vec![
        scene("One", Some(twin_part()), Vec::new()),
        scene("Two", Some(twin_part()), Vec::new()),
    ]
}

/// The file's text.
fn kept(rig: &Rig) -> Option<String> {
    std::fs::read_to_string(&rig.state).ok()
}

/// A recall writes the state file; a rename rewrites it, a delete clears it, and the view says the
/// same each time.
#[tokio::test]
async fn the_recall_memory_follows_a_rename_and_a_delete_and_lives_in_its_file() {
    let rig = Rig::new(Vec::new(), true, "").await;
    rig.add(&twins(&rig)).await;
    assert_eq!(rig.view().recalled, None);

    rig.recall("Two").await;
    assert_eq!(kept(&rig).as_deref(), Some("recalled = \"Two\"\n"));
    let view = rig
        .view_when("Two on", |v| v.on.as_deref() == Some("Two"))
        .await;
    assert_eq!(
        view.recalled.as_deref(),
        Some("Two"),
        "the tie goes to the recalled one"
    );

    let renamed = rig
        .ask(SceneOp::Rename {
            from: "two".into(),
            to: " Second ".into(),
        })
        .await;
    assert_eq!(
        renamed,
        Response::Scene {
            name: "Second".into(),
            outcome: Vec::new()
        }
    );
    assert_eq!(kept(&rig).as_deref(), Some("recalled = \"Second\"\n"));
    let view = rig
        .view_when("renamed", |v| v.recalled.as_deref() == Some("Second"))
        .await;
    assert_eq!(
        view.scenes
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["One", "Second"]
    );
    assert_eq!(view.on.as_deref(), Some("Second"));
    let file = Config::load_from(&rig.config).expect("the file loads");
    assert!(file.scene("Second").is_some() && file.scene("Two").is_none());

    // Renaming another scene leaves the memory alone.
    rig.ask(SceneOp::Rename {
        from: "One".into(),
        to: "First".into(),
    })
    .await;
    assert_eq!(kept(&rig).as_deref(), Some("recalled = \"Second\"\n"));

    let deleted = rig
        .ask(SceneOp::Delete {
            name: "SECOND".into(),
        })
        .await;
    assert_eq!(
        deleted,
        Response::Scene {
            name: "Second".into(),
            outcome: Vec::new()
        }
    );
    assert_eq!(kept(&rig), None, "a delete clears the file");
    let view = rig.view_when("deleted", |v| v.scenes.len() == 1).await;
    assert_eq!(view.recalled, None);
    assert_eq!(view.on.as_deref(), Some("First"));
    rig.daemon.shutdown().await;
}

/// A scene that is not there is `missing`, a name taken or bad is refused as such.
#[tokio::test]
async fn rename_and_delete_refuse_what_they_cannot_do() {
    let rig = Rig::new(Vec::new(), true, "").await;
    rig.add(&twins(&rig)).await;
    let (_, reason) = refused(
        rig.ask(SceneOp::Delete {
            name: "Nope".into(),
        })
        .await,
    );
    assert_eq!(reason, Some(Reason::Missing));
    let (_, reason) = refused(
        rig.ask(SceneOp::Rename {
            from: "Nope".into(),
            to: "X".into(),
        })
        .await,
    );
    assert_eq!(reason, Some(Reason::Missing));
    let (_, reason) = refused(
        rig.ask(SceneOp::Rename {
            from: "One".into(),
            to: "two".into(),
        })
        .await,
    );
    assert_eq!(reason, Some(Reason::Taken));
    let (_, reason) = refused(
        rig.ask(SceneOp::Rename {
            from: "One".into(),
            to: "Custom".into(),
        })
        .await,
    );
    assert_eq!(reason, Some(Reason::BadName));
    let (_, reason) = refused(rig.recall("Nope").await);
    assert_eq!(reason, Some(Reason::Missing));
    rig.daemon.shutdown().await;
}

/// A new daemon over the same state file starts with the scene recalled before, and ranks it
/// first; a garbled file is no memory and no failure.
#[tokio::test]
async fn a_new_daemon_starts_with_the_recalled_scene_and_ranks_it_first() {
    let first = Rig::new(Vec::new(), true, "").await;
    first.add(&twins(&first)).await;
    first.recall("Two").await;
    first.daemon.shutdown().await;
    let (dir, config, state) = (first.dir.clone(), first.config.clone(), first.state.clone());
    std::mem::forget(first);

    let second = Rig::over(Vec::new(), true, dir.clone(), config.clone(), state.clone()).await;
    assert_eq!(second.daemon.scenes().recalled().as_deref(), Some("Two"));
    second
        .daemon
        .reload_all()
        .await
        .expect("the scenes are read");
    // The new computer plays what both scenes hold, with no recall to say which.
    second
        .daemon
        .host()
        .scene_recall(twin_part())
        .await
        .expect("the part is put on");
    let ranked = second
        .view_when("Two on at the start", |v| v.on.is_some())
        .await;
    assert_eq!(
        ranked.on.as_deref(),
        Some("Two"),
        "the tie goes to the one recalled before"
    );
    second.daemon.shutdown().await;
    std::mem::forget(second);

    std::fs::write(&state, "recalled = \"Tw\n").expect("a garbled file");
    let third = Rig::over(Vec::new(), true, dir.clone(), config, state).await;
    assert_eq!(
        third.daemon.scenes().recalled(),
        None,
        "garbled: none, and no failure"
    );
    third
        .daemon
        .reload_all()
        .await
        .expect("the scenes are read");
    third
        .daemon
        .host()
        .scene_recall(twin_part())
        .await
        .expect("the part is put on");
    let view = third.view_when("a scene on", |v| v.on.is_some()).await;
    assert_eq!(
        view.on.as_deref(),
        Some("One"),
        "so the tie goes to the file's first"
    );
    // The directory is the first rig's, which was kept for the next two.
    let _ = std::fs::remove_dir_all(dir);
}
