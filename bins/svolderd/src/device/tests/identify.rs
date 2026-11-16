// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// Identify during a wait is answered not connected and leaves the schedule alone: it is not a
/// command that needs the link, so it does not end the wait.
#[tokio::test(start_paused = true)]
async fn identify_during_the_wait_leaves_the_schedule_alone() {
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

    let answer = ask(&handle, Command::Identify).await;
    assert!(
        matches!(&answer, Response::Error { code: CODE_LINK, message, .. } if message == "device not connected"),
        "got {answer:?}"
    );
    assert_eq!(
        next_attempt(&mut seen).await - last,
        RETRY_SCHEDULE[RETRY_SCHEDULE.len() - 1],
        "identify moved the next attempt"
    );
}

/// Identify on a linked actor sends one frame and answers sent; a second inside the gap answers
/// just sent.
#[tokio::test(start_paused = true)]
async fn identify_on_a_linked_actor_answers_sent_then_just_sent() {
    let transport = Arc::new(Away) as Arc<dyn Transport>;
    let mut actor = connected_actor(transport).await;
    for sent in [true, false] {
        assert_eq!(
            actor.run_command(Command::Identify).await,
            Response::Identify {
                device: "fake".into(),
                sent
            }
        );
    }
}

/// A device whose profile offers no identify answers a usage refusal that says so, on a linked
/// actor and on one with no link alike, and sends nothing.
#[tokio::test(start_paused = true)]
async fn identify_on_a_device_whose_profile_offers_none_is_refused_in_words() {
    let transport = Arc::new(Away) as Arc<dyn Transport>;
    let mut actor = connected_actor(transport).await;
    actor.profile.identify = false;
    for linked in [true, false] {
        if !linked {
            actor.session = None;
        }
        let answer = actor.run_command(Command::Identify).await;
        assert!(
            matches!(&answer, Response::Error { code: CODE_USAGE, message, .. } if message == "fake offers no identify"),
            "linked {linked}: {answer:?}"
        );
    }
}
