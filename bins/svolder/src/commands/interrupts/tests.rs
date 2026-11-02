// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use super::*;

/// Two presses made before anything waits are two: each wait takes one, and there is no third.
#[tokio::test(start_paused = true)]
async fn two_presses_before_anyone_waits_are_two_waits() {
    let presses = Presses::new();
    presses.press();
    presses.press();
    presses.next().await;
    assert!(
        tokio::time::timeout(Duration::from_secs(1), presses.next())
            .await
            .is_ok(),
        "the second press was lost"
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(1), presses.next())
            .await
            .is_err(),
        "a third press was counted that nobody made"
    );
}

/// A signal that arrives after `listen` and before anyone waits is a press kept for the wait.
#[tokio::test]
async fn a_hangup_after_listening_is_a_press() {
    let presses = listen();
    // SAFETY: raising a signal whose handler `listen` has just installed.
    assert_eq!(unsafe { libc::raise(libc::SIGHUP) }, 0);
    tokio::time::timeout(Duration::from_secs(5), presses.next())
        .await
        .expect("the hangup was counted");
}
