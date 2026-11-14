// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use svolder_ipc::AutoEqTarget;

use crate::test_support::autoeq_script::answer_of;
use crate::test_support::stub_daemon::stub_daemon;
use crate::tui::test_support::presets::two_headphones;

use super::*;

/// A request that answers with the search for `query`, and counts the times it was awaited.
fn call_counting(query: &str, sent: &Arc<AtomicUsize>) -> Call {
    let found = answer_of(&two_headphones(), query);
    let sent = Arc::clone(sent);
    Box::pin(async move {
        sent.fetch_add(1, Ordering::SeqCst);
        Ok(Response::AutoEqSearch(Box::new(found)))
    })
}

fn ask(seq: u64, query: &str) -> Ask {
    Ask {
        seq,
        query: query.into(),
        refresh: false,
    }
}

/// A burst of questions that each wait for the typing is one send, of the last, after the
/// pause: the earlier ones are replaced and never sent.
#[tokio::test(start_paused = true)]
async fn a_burst_of_questions_is_one_send_after_the_pause() {
    let sent = Arc::new(AtomicUsize::new(0));
    let mut jobs = Jobs::default();
    for (seq, query) in [(1, "h"), (2, "hd"), (3, "hd6")] {
        jobs.search(Some(call_counting(query, &sent)), ask(seq, query), true);
    }
    assert_eq!(sent.load(Ordering::SeqCst), 0, "a question was sent early");
    assert!(jobs.ended.try_recv().is_err());

    tokio::time::advance(DEBOUNCE - Duration::from_millis(1)).await;
    assert!(
        jobs.due_at().is_some_and(|at| at > Instant::now()),
        "the pause ended early"
    );
    due(jobs.due_at()).await;
    jobs.send_due();
    assert!(jobs.due_at().is_none());
    let Done::Searched(asked, Ok(found)) = next(&mut jobs.ended).await else {
        panic!("the answer did not arrive");
    };
    assert_eq!((asked.seq, asked.query.as_str()), (3, "hd6"));
    assert!(!found.groups.is_empty());
    assert_eq!(sent.load(Ordering::SeqCst), 1);
}

/// A question sent at once clears the one waiting for the typing (it is newer), and stops the one
/// in flight: its request is dropped, and only the newer answer arrives.
#[tokio::test(start_paused = true)]
async fn a_newer_question_replaces_the_waiting_one_and_stops_the_one_in_flight() {
    struct Dropped(Arc<AtomicUsize>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(AtomicUsize::new(0));
    let guard = Dropped(Arc::clone(&dropped));
    let slow: Call = Box::pin(async move {
        let _held = guard;
        std::future::pending::<()>().await;
        unreachable!("the slow request answered")
    });
    let sent = Arc::new(AtomicUsize::new(0));
    let mut jobs = Jobs::default();
    jobs.search(Some(slow), ask(1, "h"), false);
    tokio::task::yield_now().await;
    assert_eq!(dropped.load(Ordering::SeqCst), 0);

    jobs.search(Some(call_counting("x", &sent)), ask(2, "hd"), true);
    jobs.search(Some(call_counting("hd", &sent)), ask(3, "hd"), false);
    assert!(
        jobs.due_at().is_none(),
        "the waiting question outlived a newer one"
    );
    let Done::Searched(asked, Ok(_)) = next(&mut jobs.ended).await else {
        panic!("the answer did not arrive");
    };
    assert_eq!(asked.seq, 3);
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        1,
        "the slow request kept running"
    );
    assert_eq!(sent.load(Ordering::SeqCst), 1);
}

/// The daemon's words for AutoEq being off for the fake device reach the line as they are, not as
/// a connect error.
#[tokio::test]
async fn autoeq_off_is_said_in_the_daemons_words() {
    let words = "AutoEq is off for the fake device; set SVOLDER_AUTOEQ_URL to turn it on";
    let call: Call = Box::pin(async move {
        Ok(Response::error(svolder_ipc::CODE_USAGE, words)
            .with_reason(svolder_ipc::Reason::Unreachable))
    });
    let mut jobs = Jobs::default();
    jobs.search(Some(call), ask(1, "hd"), false);
    let Done::Searched(_, Err(why)) = next(&mut jobs.ended).await else {
        panic!("the search was not refused");
    };
    assert_eq!(why, words);
}

/// A session with no daemon answers every job with the words that say AutoEq is the daemon's, and
/// reaches for nothing.
#[tokio::test]
async fn no_daemon_is_said_in_words() {
    let mut jobs = Jobs::default();
    jobs.search(None, ask(1, "hd"), false);
    let Done::Searched(_, Err(why)) = next(&mut jobs.ended).await else {
        panic!("the search was not refused");
    };
    assert!(why.starts_with(NEEDS_DAEMON), "{why}");
    assert!(why.contains(RESTART_COMMAND), "{why}");

    jobs.import(
        None,
        svolder_ipc::AutoEqVariant {
            name: "x".into(),
            source: "s".into(),
            form: "over-ear".into(),
            rig: None,
        },
        "x".into(),
        AutoEqTarget::Computer,
    );
    assert!(matches!(
        next(&mut jobs.ended).await,
        Done::Imported(_, Err(_))
    ));
}

/// An import runs on the daemon and ends as a `Done`: the preset it saved, or its refusal in its
/// own words.
#[tokio::test]
async fn an_import_ends_as_a_done_with_the_daemons_answer() {
    let daemon = stub_daemon().await;
    daemon.autoeq().saves("Sennheiser HD 650", 2);
    let mut jobs = Jobs::default();
    let variant = svolder_ipc::AutoEqVariant {
        name: "Sennheiser HD 650".into(),
        source: "oratory1990".into(),
        form: "over-ear".into(),
        rig: None,
    };
    jobs.import(
        Some(daemon.socket.clone()),
        variant.clone(),
        "Sennheiser HD 650".into(),
        AutoEqTarget::Computer,
    );
    let Done::Imported(_, Ok(saved)) = next(&mut jobs.ended).await else {
        panic!("the import did not end saved");
    };
    assert_eq!(saved.name, "Sennheiser HD 650");

    daemon.autoeq().fails(
        "Sennheiser HD 650 is listed but its file is not there",
        svolder_ipc::Reason::NotListed,
    );
    jobs.import(
        Some(daemon.socket.clone()),
        variant,
        "Sennheiser HD 650".into(),
        AutoEqTarget::Computer,
    );
    let Done::Imported(_, Err(why)) = next(&mut jobs.ended).await else {
        panic!("the refusal did not arrive");
    };
    assert!(why.contains("listed but its file"), "{why}");
}
