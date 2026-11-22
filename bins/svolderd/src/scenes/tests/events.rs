// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scenes' view as events, and which scene is on while devices read and reconnect.

use svolder_ipc::Handler;

use super::*;

/// `Event::Scenes` reaches a device-filtered subscription once after the snapshot, and then on a
/// recall, on a setting moved elsewhere (the scene is no longer on) and on a reload.
#[tokio::test]
async fn the_view_is_sent_on_a_recall_a_moved_setting_and_a_reload() {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let rig = Rig::new(vec![desk], true, "").await;
    let desk = &rig.stands[0];
    rig.add(&[scene(
        "Late night",
        Some(computer_part("Desk", vec![pk(100.0, 2.0, 1.0)], 0.0)),
        vec![part(
            desk,
            Some(tone_of(&desk.profile, "Bass")),
            &[("input", desk.other("input"))],
        )],
    )])
    .await;

    let mut events = rig.daemon.subscribe(Some("Desk DAC".into())).await;
    let first = events.next().await.expect("the snapshot");
    assert!(matches!(first, Event::Connection { .. }), "{first:?}");
    let view = next_scenes(&mut events).await;
    assert_eq!(view.scenes.len(), 1);
    assert_eq!(view.on, None);

    rig.recall("Late night").await;
    loop {
        let view = next_scenes(&mut events).await;
        if view.on.as_deref() == Some("Late night") {
            assert_eq!(view.recalled.as_deref(), Some("Late night"));
            break;
        }
    }

    rig.daemon
        .handle(Request::Set {
            device: Some("Desk DAC".into()),
            key: "input".into(),
            value: desk.first("input").to_string(),
            step: 1,
            no_wait: false,
        })
        .await;
    loop {
        let view = next_scenes(&mut events).await;
        if view.on.is_none() {
            assert_eq!(
                view.recalled.as_deref(),
                Some("Late night"),
                "recalled, no longer on"
            );
            break;
        }
    }

    Config::add_scene(
        &rig.config,
        &scene(
            "Another",
            None,
            vec![part(desk, None, &[("input", desk.other("input"))])],
        ),
    )
    .expect("a scene is added");
    rig.daemon.reload_all().await.expect("the reload");
    loop {
        let view = next_scenes(&mut events).await;
        if view.scenes.len() == 2 {
            break;
        }
    }
    rig.daemon.shutdown().await;
}

/// A subscription that names no device gets its error and no scenes.
#[tokio::test]
async fn a_subscription_that_names_no_device_gets_no_scenes() {
    let rig = Rig::new(vec![stand("bifrost3", "Desk DAC", BIFROST, &[])], false, "").await;
    let mut events = rig.daemon.subscribe(Some("Nope".into())).await;
    assert!(matches!(events.next().await, Some(Event::Error { .. })));
    assert!(events.next().await.is_none());
    rig.daemon.shutdown().await;
}

/// Two scenes, each holding one device's part and both matching, in the file's order `first`
/// then `second`. The amp then reconnects unread (connected, every reading gone). Returns the view
/// before, with the amp unread, and after it has read again.
async fn amp_reconnects(amp_first: bool) -> (ScenesView, ScenesView, ScenesView) {
    let desk = stand("bifrost3", "Desk DAC", BIFROST, &[]);
    let amp = stand("lyr5", "Amp", LYR, &[]);
    let rig = Rig::new(vec![desk, amp], false, "").await;
    let (desk, amp) = (&rig.stands[0], &rig.stands[1]);
    let desk_input = desk.raw("input").expect("the desk reads its input");
    let on_desk = scene(
        "Desk one",
        None,
        vec![part(desk, None, &[("input", desk_input)])],
    );
    let amp_input = amp.raw("input").expect("the amp reads its input");
    let on_amp = scene(
        "Amp zero",
        None,
        vec![part(amp, None, &[("input", amp_input)])],
    );
    match amp_first {
        true => rig.add(&[on_amp, on_desk]).await,
        false => rig.add(&[on_desk, on_amp]).await,
    }
    // Every look at the view works it out from the feeds as they are now.
    let before = rig.view();
    let whole = amp.readings();
    amp.forget_all();
    let unread = rig.view();
    amp.read_again(whole);
    let after = rig.view();
    rig.daemon.shutdown().await;
    (before, unread, after)
}

/// An amp that reconnects unread, whose scene ranks below the one that is on, changes nothing: the
/// scene is still on and still known.
#[tokio::test]
async fn an_unread_device_leaves_a_scene_ranked_above_its_own_on() {
    let (before, unread, after) = amp_reconnects(false).await;
    assert_eq!(
        (before.on.as_deref(), before.known),
        (Some("Desk one"), true)
    );
    assert_eq!(
        (unread.on.as_deref(), unread.known),
        (Some("Desk one"), true),
        "{unread:?}"
    );
    assert_eq!((after.on.as_deref(), after.known), (Some("Desk one"), true));
}

/// The same amp, whose scene ranks above: nothing is known to be on until the amp has read.
#[tokio::test]
async fn an_unread_device_darkens_the_scenes_it_could_outrank() {
    let (before, unread, after) = amp_reconnects(true).await;
    assert_eq!(
        (before.on.as_deref(), before.known),
        (Some("Amp zero"), true)
    );
    assert_eq!(
        (unread.on, unread.known),
        (None, false),
        "reading, never none"
    );
    assert_eq!((after.on.as_deref(), after.known), (Some("Amp zero"), true));
}
