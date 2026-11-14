// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_ipc::{AutoEqOp, AutoEqTarget, AutoEqVariant, Reason};

use crate::test_support::stub_daemon::stub_daemon;

use super::*;
use crate::commands::interrupts::Presses;

const LINES: [&str; 3] = [
    "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990",
    "- [Sennheiser HD 650](./crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650) by crinacle on GRAS 43AG-7",
    "- [Sennheiser HD 600](./oratory1990/over-ear/Sennheiser%20HD%20600) by oratory1990",
];

fn variant() -> AutoEqVariant {
    AutoEqVariant {
        name: "Sennheiser HD 650".into(),
        source: "oratory1990".into(),
        form: "over-ear".into(),
        rig: None,
    }
}

/// Wait for `ready`, polling every few milliseconds; panics naming `what` after ten seconds.
async fn waited(what: &str, ready: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !ready() {
        assert!(std::time::Instant::now() < deadline, "never came: {what}");
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
}

/// A search is the daemon's grouped answer for the query, and says whether it was a refresh.
#[tokio::test]
async fn a_search_is_the_daemons_grouped_answer() {
    let daemon = stub_daemon().await;
    daemon.autoeq().index(&LINES);

    let found = search(&daemon.socket, "hd650", true)
        .await
        .expect("answered");
    assert_eq!(found.groups.len(), 1);
    assert_eq!(found.groups[0].variants.len(), 2);
    assert_eq!(found.measurements, 3);
    assert_eq!(daemon.autoeq().searches(), [("hd650".to_string(), true)]);
}

/// A daemon from before AutoEq answers the request as an unknown one: the words say to restart
/// it, rather than serde's.
#[test]
fn a_daemon_without_autoeq_is_asked_to_restart() {
    let old = Refused::of(Ok(Response::unknown_request("auto_eq")));
    assert!(old.line().contains(RESTART_COMMAND), "{}", old.line());
    assert_eq!(old.reason, None);
    // A 0.32.x daemon answers code 1 with serde's own words.
    let old = Refused::of(Ok(Response::error(
        svolder_ipc::CODE_USAGE,
        "bad request: unknown variant `auto_eq`, expected one of `get`, `set`, `state`",
    )));
    assert!(old.line().contains(RESTART_COMMAND), "{}", old.line());
    assert!(old.too_old);
    // The CLI's words (`reach::daemon_runs`), with the restart step after them for the TUI.
    assert_eq!(old.message, crate::reach::too_old_words(None, "use AutoEq"));
    assert_eq!(
        old.line(),
        format!("{}; run {RESTART_COMMAND}", old.message)
    );
    let other = Refused::of(Ok(Response::error(1, "a query of 61 characters")));
    assert!(!other.too_old);
    assert_eq!(other.message, "a query of 61 characters");
}

/// An import waits for its job's end and gives the saved preset; the job's refusal comes with
/// its reason.
#[tokio::test]
async fn an_import_waits_for_the_job_and_carries_its_refusal() {
    let daemon = stub_daemon().await;
    daemon.autoeq().saves("Sennheiser HD 650", 2);
    let saved = import(
        &daemon.socket,
        variant(),
        Some("HD 650".into()),
        false,
        AutoEqTarget::Computer,
        None,
    )
    .await
    .expect("saved");
    assert_eq!(
        (saved.name.as_str(), saved.filters),
        ("Sennheiser HD 650", 2)
    );
    let ops = daemon.autoeq().imports();
    let [
        AutoEqOp::Import {
            name,
            force,
            target,
            ..
        },
    ] = ops.as_slice()
    else {
        panic!("one import expected");
    };
    assert_eq!(
        (name.as_deref(), *force, target),
        (Some("HD 650"), false, &AutoEqTarget::Computer)
    );

    daemon.autoeq().fails(
        "Sennheiser HD 650 is listed but its file is not there",
        Reason::NotListed,
    );
    let refused = import(
        &daemon.socket,
        variant(),
        None,
        false,
        AutoEqTarget::Computer,
        None,
    )
    .await
    .expect_err("refused");
    assert_eq!(refused.reason, Some(Reason::NotListed));
    assert!(
        refused.message.contains("listed but its file"),
        "{refused:?}"
    );
}

/// The job's end comes on the stream whichever order it and the stream's opening fall in: a job
/// that ended before the stream opened is read from the replay, one that ends after from the
/// events.
#[tokio::test]
async fn a_job_that_ends_before_or_after_the_stream_opens_is_read() {
    let daemon = stub_daemon().await;
    daemon.autoeq().saves("A", 1);
    // Ended before the stream opened (the stub finishes it inside the request).
    let early = import(
        &daemon.socket,
        variant(),
        None,
        false,
        AutoEqTarget::Computer,
        None,
    )
    .await;
    assert_eq!(early.expect("saved").name, "A");

    // Ends after: held until the case lets it finish.
    daemon.autoeq().holds_jobs();
    let socket = daemon.socket.clone();
    let late = tokio::spawn(async move {
        import(
            &socket,
            variant(),
            None,
            false,
            AutoEqTarget::Computer,
            None,
        )
        .await
    });
    waited("the second import to reach the daemon", || {
        daemon.autoeq().imports().len() >= 2
    })
    .await;
    daemon.autoeq().finishes();
    assert_eq!(late.await.expect("joined").expect("saved").name, "A");
}

/// A call dropped while its job runs cancels the job.
#[tokio::test]
async fn a_dropped_import_cancels_its_job() {
    let daemon = stub_daemon().await;
    daemon.autoeq().saves("A", 1);
    daemon.autoeq().holds_jobs();
    let socket = daemon.socket.clone();
    let call = tokio::spawn(async move {
        import(
            &socket,
            variant(),
            None,
            false,
            AutoEqTarget::Computer,
            None,
        )
        .await
    });
    // Once the call follows its job, it holds the job's id and the drop has one to cancel.
    waited("the job's follow stream to open", || {
        daemon.autoeq().follows() > 0
    })
    .await;
    call.abort();
    let _ = call.await;
    waited("the daemon to be told of the cancel", || {
        !daemon.autoeq().cancelled().is_empty()
    })
    .await;
    assert_eq!(daemon.autoeq().cancelled().len(), 1);
}

/// A call dropped after the daemon made its job but before the answer was read cancels the job
/// once, as soon as its id is known.
#[tokio::test]
async fn a_call_dropped_before_the_start_answers_cancels_the_job_once() {
    let daemon = stub_daemon().await;
    daemon.autoeq().saves("A", 1);
    daemon.autoeq().holds_jobs();
    daemon.autoeq().holds_starts();
    let socket = daemon.socket.clone();
    let call = tokio::spawn(async move {
        import(
            &socket,
            variant(),
            None,
            false,
            AutoEqTarget::Computer,
            None,
        )
        .await
    });
    // The daemon has made the job; the answer that names it is held, so the call has no id yet.
    waited("the import to reach the daemon", || {
        !daemon.autoeq().imports().is_empty()
    })
    .await;
    call.abort();
    let _ = call.await;
    assert!(daemon.autoeq().cancelled().is_empty(), "no id is known yet");
    daemon.autoeq().lets_starts_go();
    waited("the job to be cancelled", || {
        !daemon.autoeq().cancelled().is_empty()
    })
    .await;
    assert_eq!(daemon.autoeq().cancelled().len(), 1);
    assert_eq!(daemon.autoeq().follows(), 0, "it never followed the job");
}

/// A follow stream the daemon will not open ends the call with the job cancelled before it
/// returns: a CLI's runtime ends at once, and a cancel left to a spawned task would not go.
#[tokio::test(flavor = "current_thread")]
async fn a_refused_follow_stream_cancels_the_job_before_the_call_returns() {
    let daemon = stub_daemon().await;
    daemon.autoeq().saves("A", 1);
    daemon.autoeq().holds_jobs();
    daemon.autoeq().refuses_follow();
    let refused = import(
        &daemon.socket,
        variant(),
        None,
        false,
        AutoEqTarget::Computer,
        None,
    )
    .await
    .expect_err("the stream is refused");
    assert!(!refused.interrupted, "{refused:?}");
    assert_eq!(
        daemon.autoeq().cancelled().len(),
        1,
        "the cancel had gone when the call returned"
    );
}

/// A press of the caller's interrupt while the job runs cancels the job on the daemon, once, and
/// the call ends refused: nothing is saved after the user left.
#[tokio::test]
async fn an_interrupt_cancels_the_job_once() {
    let daemon = stub_daemon().await;
    daemon.autoeq().saves("A", 1);
    daemon.autoeq().holds_jobs();
    let presses = Arc::new(Presses::new());
    let socket = daemon.socket.clone();
    let held = presses.clone();
    let call = tokio::spawn(async move {
        import(
            &socket,
            variant(),
            None,
            false,
            AutoEqTarget::Computer,
            Some(held),
        )
        .await
    });
    waited("the job's follow stream to open", || {
        daemon.autoeq().follows() > 0
    })
    .await;
    presses.press();
    presses.press();
    let refused = call.await.expect("joined").expect_err("interrupted");
    assert!(refused.interrupted, "{refused:?}");
    assert_eq!(daemon.autoeq().cancelled().len(), 1, "cancelled once");
}

/// A job that ended just before the press has saved its preset: the call says so, not that it
/// was cancelled.
#[tokio::test]
async fn a_job_that_ended_before_the_interrupt_is_saved_not_cancelled() {
    let daemon = stub_daemon().await;
    daemon.autoeq().saves("A", 1);
    daemon.autoeq().holds_jobs();
    let presses = Arc::new(Presses::new());
    let socket = daemon.socket.clone();
    let held = presses.clone();
    let call = tokio::spawn(async move {
        import(
            &socket,
            variant(),
            None,
            false,
            AutoEqTarget::Computer,
            Some(held),
        )
        .await
    });
    waited("the job's follow stream to open", || {
        daemon.autoeq().follows() > 0
    })
    .await;
    daemon.autoeq().finishes();
    presses.press();
    let saved = call.await.expect("joined").expect("the preset was saved");
    assert_eq!(saved.name, "A");
}
