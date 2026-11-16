// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The client's calls against the stub daemon's scripted sets service.

use svolder_ipc::{Reason, SpeakersOp, SpeakersRoom};

use super::*;
use crate::test_support::speakers_script::{FIRST_JOB, set};
use crate::test_support::stub_daemon::stub_daemon;

#[tokio::test]
async fn the_catalogue_is_what_the_daemon_lists() {
    let daemon = stub_daemon().await;
    daemon.speakers().lists(vec![
        set("a", SpeakersRoom::No),
        set("b", SpeakersRoom::With),
    ]);
    daemon.speakers().can_fetch(false);
    let found = catalogue(&daemon.socket).await.expect("listed");
    let ids: Vec<_> = found.sets.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert!(!found.fetching);
}

#[tokio::test]
async fn a_fetch_reports_its_progress_and_gives_the_saved_set() {
    let daemon = stub_daemon().await;
    daemon.speakers().saves(set("a", SpeakersRoom::No));
    daemon.speakers().reports(&[(10, 100), (100, 100)]);
    let mut heard = Vec::new();
    let saved = fetch(&daemon.socket, "a", None, &mut |p| {
        heard.push((p.received, p.total));
    })
    .await
    .expect("saved");
    assert_eq!(saved.set.id, "a");
    assert_eq!(heard, [(10, 100), (100, 100)]);
    assert_eq!(
        daemon.speakers().ops()[0],
        SpeakersOp::Fetch { id: "a".into() }
    );
}

#[tokio::test]
async fn a_failed_job_is_the_daemons_words_and_reason() {
    let daemon = stub_daemon().await;
    daemon
        .speakers()
        .fails("the source changed", Reason::Unreachable);
    let refused = fetch(&daemon.socket, "a", None, &mut |_| {})
        .await
        .expect_err("fails");
    assert_eq!(refused.message, "the source changed");
    assert_eq!(refused.reason, Some(Reason::Unreachable));
    assert!(!refused.interrupted && !refused.too_old);
}

#[tokio::test]
async fn an_import_sends_the_path_label_and_room() {
    let daemon = stub_daemon().await;
    daemon.speakers().saves(set("mine", SpeakersRoom::With));
    import(
        &daemon.socket,
        Path::new("/x/room.wav"),
        Some("Mine".into()),
        Some(SpeakersRoom::With),
        None,
    )
    .await
    .expect("saved");
    assert_eq!(
        daemon.speakers().ops()[0],
        SpeakersOp::Import {
            path: "/x/room.wav".into(),
            label: Some("Mine".into()),
            room: Some(SpeakersRoom::With),
        }
    );
}

#[tokio::test]
async fn a_press_cancels_the_running_job_and_the_call_ends_interrupted() {
    let daemon = stub_daemon().await;
    daemon.speakers().holds_jobs();
    let presses = Arc::new(Presses::new());
    presses.press();
    let refused = fetch(&daemon.socket, "a", Some(presses), &mut |_| {})
        .await
        .expect_err("interrupted");
    assert!(refused.interrupted, "{refused:?}");
    assert_eq!(daemon.speakers().cancelled(), [FIRST_JOB]);
}

#[tokio::test]
async fn a_forget_the_daemon_refuses_comes_back_in_its_words() {
    let daemon = stub_daemon().await;
    daemon
        .speakers()
        .refuses_forget("a plays now", Reason::Busy);
    let refused = forget(&daemon.socket, "a").await.expect_err("refused");
    assert_eq!(refused.message, "a plays now");
    assert_eq!(refused.reason, Some(Reason::Busy));
}

#[test]
fn a_daemon_that_does_not_know_the_request_is_too_old() {
    let reply = Ok(Response::error(
        svolder_ipc::CODE_UNKNOWN_REQUEST,
        "no such request",
    ));
    let refused = Refused::of(reply);
    assert!(refused.too_old);
    assert!(refused.message.contains("cannot"), "{}", refused.message);
}

#[tokio::test]
async fn a_fetch_dropped_while_its_job_runs_cancels_the_job() {
    let daemon = stub_daemon().await;
    daemon.speakers().holds_jobs();
    {
        let mut quiet = |_: &svolder_ipc::SpeakersProgress| {};
        let call = fetch(&daemon.socket, "a", None, &mut quiet);
        tokio::pin!(call);
        // The job is made, and the call is read until the follow stream is open.
        tokio::select! {
            _ = &mut call => panic!("the job was held"),
            () = daemon.speakers().job_started() => {}
        }
        // Run the call on until it waits on the stream, then drop it.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(50), &mut call).await;
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        daemon.speakers().cancel_arrived(),
    )
    .await
    .expect("the daemon was told to cancel");
    assert_eq!(daemon.speakers().cancelled(), [FIRST_JOB]);
}

#[tokio::test]
async fn a_fetch_dropped_before_the_daemon_answered_its_start_cancels_the_job_once_it_is_known() {
    let daemon = stub_daemon().await;
    daemon.speakers().holds_jobs();
    daemon.speakers().holds_starts();
    {
        let mut quiet = |_: &svolder_ipc::SpeakersProgress| {};
        let call = fetch(&daemon.socket, "a", None, &mut quiet);
        tokio::pin!(call);
        tokio::select! {
            _ = &mut call => panic!("the start was held"),
            () = daemon.speakers().job_started() => {}
        }
    }
    daemon.speakers().lets_starts_go();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        daemon.speakers().cancel_arrived(),
    )
    .await
    .expect("the daemon was told to cancel");
    assert_eq!(daemon.speakers().cancelled(), [FIRST_JOB]);
}
