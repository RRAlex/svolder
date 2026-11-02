// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_ipc::Response;

use super::*;
use crate::test_support::stub_daemon::{CANONICAL, stub_daemon};

/// A daemon that answers `identify` with `reply`, and the words the verb makes of it.
async fn said_over(reply: Response) -> Result<String, Failure> {
    let daemon = stub_daemon().await;
    daemon.identifies(reply);
    let mut backend = Backend::stub_remote(&daemon.socket).await;
    let said = identify_on(&mut backend).await;
    daemon.stop().await;
    said
}

#[tokio::test]
async fn a_sent_request_says_sent_and_a_repeat_says_to_wait() {
    let sent = said_over(Response::Identify {
        device: CANONICAL.into(),
        sent: true,
    })
    .await
    .expect("the request went out");
    assert_eq!(sent, "identify sent to fake");
    let just = said_over(Response::Identify {
        device: CANONICAL.into(),
        sent: false,
    })
    .await
    .expect("a repeat is no failure");
    assert_eq!(just, "identify just sent to fake; wait 2 s");
}

/// A daemon that is not holding the link refuses with the not-connected words; the verb adds
/// when the daemon tries the device again, as every listing does.
#[tokio::test]
async fn a_device_that_is_down_says_when_the_daemon_tries_again() {
    let daemon = stub_daemon().await;
    daemon.identifies(Response::Error {
        code: 3,
        message: "device not connected".into(),
    });
    daemon.canonical_waits(12);
    let mut backend = Backend::stub_remote(&daemon.socket).await;
    let failure = identify_on(&mut backend).await.expect_err("not connected");
    daemon.stop().await;
    assert_eq!(failure.code, 3);
    assert_eq!(failure.what, "device not connected, next attempt in 12 s");
}

#[tokio::test]
async fn identify_against_an_old_daemon_says_restart() {
    let daemon = stub_daemon().await;
    let mut backend = Backend::stub_remote(&daemon.socket).await;
    let failure = identify_on(&mut backend)
        .await
        .expect_err("an old daemon does not know the request");
    daemon.stop().await;
    assert_eq!(
        failure.what,
        "this daemon is too old to identify; restart it"
    );
    assert_eq!(failure.next, "Run `svolder daemon restart`");
    assert_eq!(failure.code, 1);
}

/// A direct session sends one frame and says so; a repeat inside the gap sends none.
#[tokio::test(start_paused = true)]
async fn a_direct_session_sends_once_per_gap() {
    let (mut backend, _gate) = Backend::fake(std::time::Duration::from_secs(5)).await;
    backend.configured_as("dac");
    assert_eq!(
        identify_on(&mut backend).await.expect("sent"),
        "identify sent to dac"
    );
    assert_eq!(
        identify_on(&mut backend)
            .await
            .expect("a repeat is no failure"),
        "identify just sent to dac; wait 2 s"
    );
}

/// A daemon that has never found the device answers the open with "has not been found yet"; the
/// verb says the not-connected words for it, with the wait from the daemon's device list.
#[tokio::test]
async fn a_device_the_daemon_has_not_found_says_not_connected_with_the_wait() {
    let daemon = stub_daemon().await;
    daemon.unfound(true);
    let failure = Backend::stub_remote_named(&daemon.socket, None)
        .await
        .err()
        .expect("the daemon has not found the device");
    daemon.stop().await;
    assert_eq!(failure.what, "fake has not been found yet");
    let step = failure.next.clone();
    let rows = [DeviceRow {
        retry_in_secs: Some(12),
        ..DeviceRow::for_test(CANONICAL)
    }];
    let said = not_connected(failure, None, &rows);
    assert_eq!(said.code, 3);
    assert_eq!(said.what, "device not connected, next attempt in 12 s");
    assert_eq!(
        said.next, step,
        "the step that says the daemon keeps looking stands"
    );

    let other = [DeviceRow::for_test("a"), DeviceRow::for_test("b")];
    let failure = Failure {
        code: 3,
        what: "b has not been found yet".into(),
        next: String::new(),
    };
    let none = not_connected(failure, Some("c"), &other);
    assert_eq!(none.what, "device not connected", "no wait is known");
}

/// The words name the device as the daemon resolved it, not as the selector spelled it.
#[tokio::test]
async fn the_words_carry_the_name_the_daemon_answered_with() {
    let said = said_over(Response::Identify {
        device: "bifrost".into(),
        sent: true,
    })
    .await
    .expect("the request went out");
    assert_eq!(said, "identify sent to bifrost");
}

/// A daemon's refusal of a device whose profile offers no identify is the verb's failure, in the
/// daemon's words and with a usage code, not the not-connected words.
#[tokio::test]
async fn a_device_that_offers_no_identify_says_so() {
    let refusal = Response::Error {
        code: 1,
        message: "fake offers no identify".into(),
    };
    let failure = said_over(refusal).await.expect_err("refused");
    assert_eq!(failure.code, 1);
    assert_eq!(failure.what, "fake offers no identify");
}

/// A direct session over such a profile refuses the same way and sends nothing.
#[tokio::test(start_paused = true)]
async fn a_direct_session_over_a_profile_without_identify_refuses() {
    let registry = svolder_profiles::Registry::builtin().expect("the built-in profiles");
    let profile = registry.get("meshcard").expect("a built-in").clone();
    assert!(!profile.identify);
    let (mut backend, _gate) = Backend::fake_over(profile, std::time::Duration::from_secs(5)).await;
    backend.configured_as("card");
    let failure = identify_on(&mut backend).await.expect_err("refused");
    assert_eq!(failure.code, 1);
    assert_eq!(failure.what, "card offers no identify");
}

/// The open's note that a down device's readings are the last known is about readings, which this
/// verb does not print: it is dropped, and other notes stay.
#[tokio::test]
async fn a_down_device_is_not_said_to_have_stale_readings_to_an_identify() {
    let daemon = stub_daemon().await;
    daemon.canonical_waits(12);
    let mut said = Backend::stub_remote(&daemon.socket).await;
    let first = said.take_warnings();
    assert_eq!(first.len(), 1, "{first:?}");
    assert!(
        first[0].ends_with("readings are the last known"),
        "{first:?}"
    );
    let mut backend = Backend::stub_remote(&daemon.socket).await;
    backend.warn("a note of another kind".into());
    backend.drop_last_known_warning();
    let warnings = backend.take_warnings();
    daemon.stop().await;
    assert_eq!(warnings, ["a note of another kind"]);
}

/// With no `-d` and several devices listed, the wait comes from the row of the device the
/// daemon's words name.
#[test]
fn the_wait_comes_from_the_device_the_daemon_names() {
    let rows = [
        DeviceRow {
            retry_in_secs: Some(7),
            ..DeviceRow::for_test("a")
        },
        DeviceRow {
            retry_in_secs: Some(12),
            ..DeviceRow::for_test("B")
        },
    ];
    let failure = Failure {
        code: 3,
        what: "b has not been found yet".into(),
        next: String::new(),
    };
    let said = not_connected(failure, None, &rows);
    assert_eq!(said.what, "device not connected, next attempt in 12 s");
}
