// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The runner against the stub daemon's scripted sets service.

use std::time::Duration;

use svolder_ipc::{SpeakersOp, SpeakersRoom};
use tokio::sync::mpsc;

use super::*;
use crate::test_support::speakers_script::{FIRST_JOB, set};
use crate::test_support::stub_daemon::stub_daemon;
use crate::tui::boxes::speakers::Stop;

/// The next thing the runner said, within a few seconds.
async fn next(heard: &mut mpsc::UnboundedReceiver<Done>) -> SpeakersDone {
    match tokio::time::timeout(Duration::from_secs(5), heard.recv()).await {
        Ok(Some(Done::Speakers(what))) => *what,
        other => panic!("something said by the runner, got {other:?}"),
    }
}

#[tokio::test]
async fn a_fetch_says_its_progress_then_its_end() {
    let daemon = stub_daemon().await;
    daemon.speakers().saves(set("a", SpeakersRoom::No));
    daemon.speakers().reports(&[(10, 100), (100, 100)]);
    let (done, mut heard) = mpsc::unbounded_channel();
    let _running = run(
        done,
        Some(daemon.socket.to_path_buf()),
        SpeakersCall::Fetch {
            id: "a".into(),
            label: "A".into(),
            stop: Stop::new(),
        },
    );
    let mut progress = Vec::new();
    loop {
        match next(&mut heard).await {
            SpeakersDone::Progress {
                received, total, ..
            } => progress.push((received, total)),
            SpeakersDone::Ended { id, result } => {
                assert_eq!(id, "a");
                assert_eq!(result.expect("saved").set.id, "a");
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(progress, [(10, 100), (100, 100)]);
}

#[tokio::test]
async fn a_stop_pressed_cancels_the_job_on_the_daemon_and_ends_as_a_stop() {
    let daemon = stub_daemon().await;
    daemon.speakers().holds_jobs();
    let stop = Stop::new();
    let (done, mut heard) = mpsc::unbounded_channel();
    let _running = run(
        done,
        Some(daemon.socket.to_path_buf()),
        SpeakersCall::Fetch {
            id: "a".into(),
            label: "A".into(),
            stop: stop.clone(),
        },
    );
    daemon.speakers().job_started().await;
    stop.press();
    daemon.speakers().cancel_arrived().await;
    assert_eq!(daemon.speakers().cancelled(), [FIRST_JOB]);
    match next(&mut heard).await {
        SpeakersDone::Ended { result, .. } => {
            assert_eq!(result.expect_err("stopped"), CANCELLED);
        }
        other => panic!("the end, got {other:?}"),
    }
}

#[tokio::test]
async fn a_fetch_dropped_while_it_runs_cancels_the_job_on_the_daemon() {
    let daemon = stub_daemon().await;
    daemon.speakers().holds_jobs();
    let (done, _heard) = mpsc::unbounded_channel();
    let running = run(
        done,
        Some(daemon.socket.to_path_buf()),
        SpeakersCall::Fetch {
            id: "a".into(),
            label: "A".into(),
            stop: Stop::new(),
        },
    );
    daemon.speakers().job_started().await;
    // the loop lets go of it
    drop(running);
    tokio::time::timeout(Duration::from_secs(5), daemon.speakers().cancel_arrived())
        .await
        .expect("the cancel arrived");
    assert_eq!(daemon.speakers().cancelled(), [FIRST_JOB]);
}

#[tokio::test]
async fn the_catalogue_and_a_forget_go_to_the_daemon() {
    let daemon = stub_daemon().await;
    daemon.speakers().lists(vec![set("a", SpeakersRoom::No)]);
    let (done, mut heard) = mpsc::unbounded_channel();
    assert!(
        run(
            done.clone(),
            Some(daemon.socket.to_path_buf()),
            SpeakersCall::Catalogue
        )
        .is_none()
    );
    match next(&mut heard).await {
        SpeakersDone::Catalogue(Ok(found)) => assert_eq!(found.sets[0].id, "a"),
        other => panic!("the catalogue, got {other:?}"),
    }
    run(
        done,
        Some(daemon.socket.to_path_buf()),
        SpeakersCall::Forget { id: "a".into() },
    );
    match next(&mut heard).await {
        SpeakersDone::Forgotten { id, result } => {
            assert_eq!(id, "a");
            assert!(result.is_ok());
        }
        other => panic!("the forget, got {other:?}"),
    }
    assert!(
        daemon
            .speakers()
            .ops()
            .contains(&SpeakersOp::Forget { id: "a".into() })
    );
}

#[tokio::test]
async fn with_no_daemon_every_call_says_so() {
    let (done, mut heard) = mpsc::unbounded_channel();
    run(done, None, SpeakersCall::Catalogue);
    match next(&mut heard).await {
        SpeakersDone::Catalogue(Err(why)) => assert_eq!(why, NEEDS_DAEMON),
        other => panic!("a refusal, got {other:?}"),
    }
}

/// On quit the loop leaves a fetch with `finish`: the daemon's job is cancelled by the time it
/// returns, so the runtime that goes next does not race the cancel.
#[tokio::test]
async fn leaving_a_fetch_cancels_its_job_before_it_returns() {
    let daemon = stub_daemon().await;
    daemon.speakers().holds_jobs();
    let (done, _heard) = mpsc::unbounded_channel();
    let running = run(
        done,
        Some(daemon.socket.to_path_buf()),
        SpeakersCall::Fetch {
            id: "a".into(),
            label: "A".into(),
            stop: Stop::new(),
        },
    )
    .expect("a fetch is kept");
    daemon.speakers().job_started().await;
    running.finish(Duration::from_secs(2)).await;
    assert_eq!(daemon.speakers().cancelled(), [FIRST_JOB]);
}

/// Quitting the TUI during a fetch goes through `Watching::release`, which cancels the job.
#[tokio::test]
async fn releasing_the_session_cancels_the_fetch_in_flight() {
    let daemon = stub_daemon().await;
    daemon.speakers().holds_jobs();
    let (backend, _gate, app) = crate::tui::run::test_support::fake(Duration::from_secs(2)).await;
    let mut watching = crate::tui::run::test_support::watching(backend, app, None);
    let (done, _heard) = mpsc::unbounded_channel();
    watching.jobs.fetching = run(
        done,
        Some(daemon.socket.to_path_buf()),
        SpeakersCall::Fetch {
            id: "a".into(),
            label: "A".into(),
            stop: Stop::new(),
        },
    );
    daemon.speakers().job_started().await;
    watching.release().await;
    assert_eq!(daemon.speakers().cancelled(), [FIRST_JOB]);
}
