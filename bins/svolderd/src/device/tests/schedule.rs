// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The retry schedule paces the attempts a device that is not there earns: the waits are
/// 2, 5, 15, 30 and 60 seconds, and the last one repeats for as long as the device stays
/// away.
///
/// Every attempt scans the adapter, so a fixed short retry is what keeps a radio
/// discovering for as long as a switched-off device is configured. The seconds are
/// spelled out here rather than read from [`RETRY_SCHEDULE`], since they are what
/// `docs/usage/daemon.md` promises and what the whole change is for.
#[tokio::test(start_paused = true)]
async fn the_retry_schedule_paces_the_attempts_a_missing_device_earns() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let _handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    let mut previous = next_attempt(&mut seen).await;
    let mut gaps = Vec::new();
    for _ in 0..6 {
        let at = next_attempt(&mut seen).await;
        gaps.push(at - previous);
        previous = at;
    }
    assert_eq!(
        gaps,
        [2, 5, 15, 30, 60, 60].map(Duration::from_secs),
        "the attempts were not paced by the schedule"
    );
}

/// A command that needs the link ends the wait at once and starts the schedule again, so
/// a user acting on a device that has just been switched back on never waits out a step.
#[tokio::test(start_paused = true)]
async fn a_command_that_needs_the_link_ends_the_wait_and_restarts_the_schedule() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // Five failures in, the actor is waiting out the schedule's last and longest step.
    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;
    assert_eq!(
        handle.describe().retry_in_secs,
        Some(60),
        "the wait the device reports is not the schedule's last step"
    );

    // The command is refused, as it is whenever the link is down, and the attempt it
    // asks for runs there and then rather than a minute later.
    let refused = ask(
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
        matches!(&refused, Response::Error { code: CODE_LINK, message, .. } if message == "device not connected"),
        "got {refused:?}"
    );
    let woken = next_attempt(&mut seen).await;
    assert!(
        woken - last < Duration::from_secs(1),
        "the command left the actor waiting for {:?}",
        woken - last
    );
    assert_eq!(
        next_attempt(&mut seen).await - woken,
        RETRY_SCHEDULE[0],
        "the schedule did not start again from its first step"
    );
}

/// Try now ends the wait at once and starts the schedule again, and is answered `Ack`.
#[tokio::test(start_paused = true)]
async fn a_wake_ends_the_wait_and_restarts_the_schedule() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");
    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;
    assert_eq!(ask(&handle, Command::Wake).await, Response::Ack);
    let woken = next_attempt(&mut seen).await;
    assert!(
        woken - last < Duration::from_secs(1),
        "the wake left the actor waiting"
    );
    assert_eq!(next_attempt(&mut seen).await - woken, RETRY_SCHEDULE[0]);
}

/// A read during a wait is answered from the last known state and leaves the schedule
/// alone: nothing about it says the device is back.
#[tokio::test(start_paused = true)]
async fn a_read_during_the_wait_leaves_the_schedule_alone() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;

    let state = ask(&handle, Command::State).await;
    assert!(
        matches!(
            &state,
            Response::Error {
                code: CODE_LINK,
                ..
            }
        ),
        "got {state:?}"
    );
    assert_eq!(
        next_attempt(&mut seen).await - last,
        RETRY_SCHEDULE[RETRY_SCHEDULE.len() - 1],
        "the read moved the next attempt"
    );
}

/// A changed entry ends the wait and starts the schedule again: the next attempt is the
/// one that uses it, so a passphrase the user has just corrected is tried there and then
/// rather than at the end of a step that may be a minute long.
#[tokio::test(start_paused = true)]
async fn a_changed_entry_during_the_wait_ends_it_and_restarts_the_schedule() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // Five failures in, the actor is waiting out the schedule's last and longest step.
    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;

    let acknowledged = ask(&handle, Command::Configure(fake_config("1234"))).await;
    assert!(
        matches!(acknowledged, Response::Ack),
        "got {acknowledged:?}"
    );
    let woken = next_attempt(&mut seen).await;
    assert!(
        woken - last < Duration::from_secs(1),
        "the changed entry left the actor waiting for {:?}",
        woken - last
    );
    assert_eq!(
        next_attempt(&mut seen).await - woken,
        RETRY_SCHEDULE[0],
        "the schedule did not start again from its first step"
    );
}

/// An entry that says what the actor is already doing, and a reload of the profiles,
/// leave the wait alone: neither says anything about the device being back.
///
/// This is what keeps the schedule meaningful at all, since the handler hands every device
/// it keeps a `Configure` on every reload — a reload of the profiles alone would otherwise
/// put every waiting device back to the schedule's first step.
#[tokio::test(start_paused = true)]
async fn an_unchanged_entry_and_a_profiles_reload_leave_the_schedule_alone() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;

    let same = ask(&handle, Command::Configure(fake_config("auto"))).await;
    assert!(matches!(same, Response::Ack), "got {same:?}");
    let reloaded = ask(
        &handle,
        Command::Reload(Arc::new(Registry::builtin().unwrap())),
    )
    .await;
    assert!(matches!(reloaded, Response::Ack), "got {reloaded:?}");
    assert_eq!(
        next_attempt(&mut seen).await - last,
        RETRY_SCHEDULE[RETRY_SCHEDULE.len() - 1],
        "an entry that changed nothing moved the next attempt"
    );
}

/// A link that comes up starts the schedule again, so the device that dropped once and
/// came back is tried at the first step rather than at the step its last absence reached.
#[tokio::test(start_paused = true)]
async fn a_connect_restarts_the_schedule() {
    let (transport, mut seen, reachable) = CountedScan::new();
    let transport = Arc::new(transport);
    let shared = Arc::clone(&transport) as Arc<dyn Transport>;
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // Three failures in, the actor is three steps down the schedule; the device is put
    // on the air for the attempt that follows.
    for _ in 0..3 {
        next_attempt(&mut seen).await;
    }
    reachable.store(true, Ordering::SeqCst);
    next_attempt(&mut seen).await;
    for _ in 0..500 {
        if handle.describe().connected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let live = handle.describe();
    assert!(
        live.connected,
        "the actor never connected to the fake device"
    );
    assert_eq!(
        live.retry_in_secs, None,
        "a connected device is not waiting for anything"
    );

    // The device goes away again: the link ends, and the attempt that follows it is the
    // schedule's first step away from the one that found nothing.
    reachable.store(false, Ordering::SeqCst);
    transport.inner.sever();
    let dropped = next_attempt(&mut seen).await;
    assert_eq!(
        next_attempt(&mut seen).await - dropped,
        RETRY_SCHEDULE[0],
        "the schedule did not start again from its first step"
    );
}
