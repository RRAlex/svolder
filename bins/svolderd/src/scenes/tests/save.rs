// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saving the settings as a scene.

use svolder_ipc::Reason;
use svolder_ipc::socket::protocol::SceneOp;

use super::*;

const KARA: &str = "0000000000000ABC";

/// A save of a Bifrost 3 and the computer writes one `[[scene]]` through the config writer, and
/// the snapshot and the view follow it; a configured device that is not connected is not in it.
#[tokio::test]
async fn a_save_writes_one_scene_with_the_computer_and_every_connected_device() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let kara = format!("[[device]]\nname = \"Kara\"\nudid = \"{KARA}\"\n");
    let rig = Rig::new(vec![desk], true, &kara).await;

    let answer = rig.save("  Late night ").await;
    assert_eq!(
        outcomes(answer),
        [(
            "Kara".to_string(),
            svolder_ipc::socket::protocol::PartStatus::Left,
            vec!["Kara is not connected: it is not in the scene".to_string()]
        )],
        "the device that is not connected is named"
    );
    let config = Config::load_from(&rig.config).expect("the file loads");
    assert_eq!(config.scenes.len(), 1, "one block in the file");
    let saved = &config.scenes[0];
    assert!(saved.computer.is_some(), "the computer's part");
    assert_eq!(saved.devices.len(), 1, "Kara is not connected: no part");
    assert_eq!(saved.devices[0].udid, BIFROST);
    assert_eq!(saved.devices[0].controls[..1], [("phase".to_string(), 0)]);
    assert!(
        saved.devices[0]
            .controls
            .contains(&("input".to_string(), 1)),
        "{:?}",
        saved.devices[0].controls
    );
    assert_eq!(
        rig.daemon.config_snapshot().scenes.len(),
        1,
        "the snapshot follows the write"
    );
    let view = rig.view();
    assert_eq!(view.scenes.len(), 1);
    assert_eq!(
        view.scenes[0]
            .parts
            .iter()
            .map(|p| p.who.as_str())
            .collect::<Vec<_>>(),
        ["computer", "Desk DAC"]
    );
    assert_eq!(
        view.on.as_deref(),
        Some("Late night"),
        "what was saved is what is on"
    );
    rig.daemon.shutdown().await;
}

/// A taken name is refused unless `force`, which saves over the scene; a name no scene can
/// carry is refused as one.
#[tokio::test]
async fn a_taken_name_needs_force_and_a_bad_one_is_refused() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], true, "").await;
    rig.save("Late night").await;
    let (_, reason) = refused(rig.save("LATE NIGHT").await);
    assert_eq!(reason, Some(Reason::Taken));

    rig.stands[0].handle.writer.note_recalled("x".into());
    let over = rig
        .ask(SceneOp::Save {
            name: "late night".into(),
            force: true,
        })
        .await;
    assert_eq!(
        over,
        Response::Scene {
            name: "Late night".into(),
            outcome: Vec::new()
        },
        "the scene keeps the spelling it has"
    );
    assert_eq!(rig.daemon.config_snapshot().scenes.len(), 1);

    let (_, reason) = refused(rig.save("custom").await);
    assert_eq!(reason, Some(Reason::BadName));
    let (_, reason) = refused(rig.save("   ").await);
    assert_eq!(reason, Some(Reason::BadName));
    rig.daemon.shutdown().await;
}

/// A band that is unread refuses the save in words, and so does a control; a read device saves.
#[tokio::test]
async fn a_save_waits_while_a_part_is_unread() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    let whole = desk.readings();

    let eq = desk.profile.eq.as_ref().expect("a tone control").clone();
    let mode = eq.mode_id();
    desk.feed
        .send_modify(|feed| feed.readings.retain(|m| m.id != mode));
    let (words, reason) = refused(rig.save("Late night").await);
    assert_eq!(words, "Desk DAC is still reading its bands");
    assert_eq!(reason, Some(Reason::Busy));

    desk.read_again(whole.clone());
    desk.forget("phase");
    let (words, reason) = refused(rig.save("Late night").await);
    assert_eq!(words, "Desk DAC is still reading its phase invert");
    assert_eq!(reason, Some(Reason::Busy));
    assert!(
        rig.daemon.config_snapshot().scenes.is_empty(),
        "nothing was written"
    );

    desk.read_again(whole);
    assert!(matches!(
        rig.save("Late night").await,
        Response::Scene { .. }
    ));
    rig.daemon.shutdown().await;
}

/// A save waits while a device's writer runs a recall, and works once it is done.
#[tokio::test]
async fn a_save_waits_while_a_writer_runs_a_recall() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let desk = &rig.stands[0];
    desk.gate.forget_permits(10_000);
    let fitting = {
        let eq = desk.profile.eq.as_ref().expect("a tone control");
        preset::builtins(eq)
    };
    let recalling = desk
        .handle
        .writer
        .recall(Origin::Socket, "Bass".into(), fitting);
    desk.handle.writer.view().await;
    let (words, reason) = refused(rig.save("Late night").await);
    assert_eq!(words, "Desk DAC is still applying a change");
    assert_eq!(reason, Some(Reason::Busy));

    desk.gate.add_permits(10_000);
    recalling.done.await.expect("the recall ended");
    desk.handle.writer.view().await;
    assert!(matches!(
        rig.save("Late night").await,
        Response::Scene { .. }
    ));
    rig.daemon.shutdown().await;
}

/// With no device connected and no computer there is nothing to save.
#[tokio::test]
async fn a_save_with_nothing_to_save_is_refused() {
    let rig = Rig::new(Vec::new(), false, "").await;
    let (words, reason) = refused(rig.save("Late night").await);
    assert!(words.starts_with("nothing to save"), "{words}");
    assert_eq!(reason, Some(Reason::NothingToSave));
}
