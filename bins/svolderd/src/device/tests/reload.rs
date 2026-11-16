// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The profile a reload resolves differs from the one the live session was opened with, and
/// the session follows it: the next refresh burst waits on the new profile's functions.
#[tokio::test]
async fn a_reload_hands_a_live_session_the_profile_it_now_resolves() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
            Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        )))
        .await;
        let session = actor.session.as_ref().expect("a live session");
        assert_eq!(session.profile().name, "gungnir2-v2");

        // The entry names another profile and the profiles are reloaded: the actor's
        // re-resolution lands on it.
        actor.config.profile = Some("gungnir2-v1".into());
        actor.reload(Arc::new(Registry::builtin().unwrap()));
        assert_eq!(actor.profile.name, "gungnir2-v1");
        let session = actor.session.as_ref().expect("a live session");
        assert_eq!(session.profile().name, "gungnir2-v1");
    })
    .await
    .expect("test timed out");
}

/// A profile change that lands while a connect attempt is in flight, the session being inside
/// the attempt, reaches that session when the attempt hands it back, even when the attempt
/// found nothing and brings no advertisement.
#[tokio::test]
async fn a_reload_during_an_attempt_reaches_the_session_it_hands_back() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let transport = Arc::new(ParkedScan::new(&started, &release)) as Arc<dyn Transport>;
        let mut actor = connected_actor(transport).await;
        let (commands, mut rx) = mpsc::channel(COMMAND_QUEUE);

        let mut changed = fake_config("1234");
        changed.profile = Some("gungnir2-v1".into());
        let driver = async {
            // The attempt is parked in its scan, with the session inside it.
            started.notified().await;
            let (reply, answered) = oneshot::channel();
            commands
                .send((Command::Configure(changed), reply))
                .await
                .map_err(|_| ())
                .expect("the actor is running");
            let acknowledged = answered.await.expect("the actor answered");
            assert!(
                matches!(acknowledged, Response::Ack),
                "got {acknowledged:?}"
            );
            // The scan finds nothing: the attempt fails and hands the session back as it was.
            release.notify_one();
        };
        let (end, ()) = tokio::join!(actor.attempt(&mut rx), driver);
        assert!(
            matches!(end, AttemptEnd::Failed(_)),
            "the attempt found nothing"
        );
        assert_eq!(actor.profile.name, "gungnir2-v1");
        let session = actor.session.as_ref().expect("the session came back");
        assert_eq!(session.profile().name, "gungnir2-v1");
    })
    .await
    .expect("test timed out");
}

/// An attempt that finds nothing hands a session back as it was while the actor's profile is
/// still the placeholder it names before any advertisement has been seen.
#[tokio::test]
async fn an_attempt_that_finds_nothing_leaves_the_session_the_placeholder_profile_unsent() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let transport = Arc::new(ParkedScan::new(&started, &release)) as Arc<dyn Transport>;
        let mut actor = connected_actor(transport).await;
        actor.advert = None;
        actor.profile = Registry::generic(0);
        let (_commands, mut rx) = mpsc::channel(COMMAND_QUEUE);
        let driver = async {
            started.notified().await;
            release.notify_one();
        };
        let (end, ()) = tokio::join!(actor.attempt(&mut rx), driver);
        assert!(
            matches!(end, AttemptEnd::Failed(_)),
            "the attempt found nothing"
        );
        let session = actor.session.as_ref().expect("the session came back");
        assert_eq!(session.profile().name, "gungnir2-v2");
    })
    .await
    .expect("test timed out");
}

/// An entry whose passphrase is no passphrase leaves the actor without a link: the attempt
/// fails after discovery, and every command that needs the link answers code 3, "not
/// connected", the code of a connection that failed.
#[tokio::test]
async fn an_unusable_passphrase_leaves_the_device_not_connected_with_code_3() {
    let registry = Arc::new(Registry::builtin().unwrap());
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
        &profile,
        unit,
        3,
        Passphrase::from_text("1234").expect("four digits"),
    )) as Arc<dyn Transport>;
    let shared: Arc<dyn Transport> = transport;
    let handle = spawn(
        Arc::clone(&shared),
        registry,
        fake_config("12"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // The attempt has found the device once the actor names its profile.
    tokio::time::timeout(Duration::from_secs(5), async {
        while handle.describe().profile.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the attempt never discovered the device");

    let set = ask(
        &handle,
        Command::Set {
            key: "mute".into(),
            value: "on".into(),
            step: 1,
            no_wait: false,
        },
    )
    .await;
    assert!(
        matches!(&set, Response::Error { code: CODE_LINK, message, .. } if message == "device not connected"),
        "got {set:?}"
    );
    assert!(!handle.describe().connected);
}
