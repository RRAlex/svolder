// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The two family services through the daemon: the cancel op, the follow stream, the busy
//! answer, and the daemon's stop, with test-only jobs (no such kind is on the wire).

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use svolder_ipc::{
    AutoEqOp, CODE_NOT_FOUND, Event, Handler, JobEvent, JobState, Reason, Request, Response, WalkOp,
};

use super::jobs::JobError;
use super::tests::daemon;
use super::*;

fn ended() -> JobState {
    JobState::Done {
        payload: serde_json::Value::Null,
    }
}

type Events = BoxStream<'static, Event>;

async fn next(events: &mut Events) -> Event {
    tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .expect("an event in time")
        .expect("the stream is open")
}

fn walk_event(job: u64, state: JobState) -> Event {
    Event::Walk(JobEvent { job, state })
}

/// The follow op is a stream through the daemon and gives the walk's events as `Event::Walk`,
/// the AutoEq follow gives `Event::AutoEq`, and neither is answered as a plain request.
#[tokio::test]
async fn each_family_follows_its_own_jobs() {
    let daemon = daemon();
    let mut walk = daemon
        .stream(Request::Walk { op: WalkOp::Follow })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the walk stream");
    let mut auto = daemon
        .stream(Request::AutoEq {
            op: AutoEqOp::Follow,
        })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the AutoEq stream");
    let id = daemon
        .walk
        .jobs()
        .start("test", |_| async { Ok(()) })
        .unwrap();
    assert_eq!(next(&mut walk).await, walk_event(id, JobState::Started));
    assert_eq!(next(&mut walk).await, walk_event(id, ended()));
    let other = daemon
        .autoeq
        .jobs()
        .start("test", |_| async { Ok(()) })
        .unwrap();
    assert_eq!(
        next(&mut auto).await,
        Event::AutoEq(JobEvent {
            job: other,
            state: JobState::Started
        })
    );
    for request in [
        Request::Walk { op: WalkOp::Follow },
        Request::AutoEq {
            op: AutoEqOp::Follow,
        },
    ] {
        let answer = daemon.handle(request).await;
        assert!(matches!(answer, Response::Error { .. }), "{answer:?}");
    }
}

/// A cancel by id stops the job and is acknowledged; an id the daemon never ran is a typed
/// refusal; a cancel is not a stream.
#[tokio::test]
async fn cancel_stops_a_job_by_its_id() {
    let daemon = daemon();
    let mut events = daemon
        .stream(Request::Walk { op: WalkOp::Follow })
        .await
        .map_err(|e| format!("{e:?}"))
        .unwrap();
    let id = daemon
        .walk
        .jobs()
        .start("test", |job| async move {
            job.cancelled().await;
            Err::<(), _>(JobError::Cancelled)
        })
        .unwrap();
    assert_eq!(next(&mut events).await, walk_event(id, JobState::Started));
    let ack = daemon
        .handle(Request::Walk {
            op: WalkOp::Cancel { job: id },
        })
        .await;
    assert_eq!(ack, Response::Ack);
    assert_eq!(next(&mut events).await, walk_event(id, JobState::Cancelled));

    let none = daemon
        .handle(Request::AutoEq {
            op: AutoEqOp::Cancel { job: 77 },
        })
        .await;
    assert_eq!(
        none,
        Response::error(CODE_NOT_FOUND, "no job 77").with_reason(Reason::NoSuchJob)
    );
    let Err(not_stream) = daemon
        .stream(Request::Walk {
            op: WalkOp::Cancel { job: id },
        })
        .await
    else {
        panic!("a cancel is not a stream");
    };
    assert!(matches!(*not_stream, Response::Error { .. }));
}

/// The walk runs one job in the whole daemon: a second start is refused and names the first.
#[tokio::test]
async fn the_walk_is_one_job_daemon_wide() {
    let daemon = daemon();
    let gate = Arc::new(tokio::sync::Notify::new());
    let held = Arc::clone(&gate);
    daemon
        .walk
        .jobs()
        .start("the TUI", |_| async move {
            held.notified().await;
            Ok(())
        })
        .unwrap();
    let refused = daemon
        .walk
        .jobs()
        .start("the web", |_| async { Ok(()) })
        .unwrap_err();
    let answer = refused.answer("walk");
    let Response::Error {
        message, reason, ..
    } = answer
    else {
        panic!("an error answer");
    };
    assert!(message.contains("the TUI"), "{message}");
    assert_eq!(reason, Some(Reason::Busy));
    gate.notify_one();
}

/// The daemon's stop cancels a running job of each family, and returns inside the bound.
#[tokio::test]
async fn shutdown_cancels_the_running_jobs_and_returns_within_the_bound() {
    let daemon = daemon();
    let mut walk = daemon
        .stream(Request::Walk { op: WalkOp::Follow })
        .await
        .map_err(|e| format!("{e:?}"))
        .unwrap();
    let mut auto = daemon
        .stream(Request::AutoEq {
            op: AutoEqOp::Follow,
        })
        .await
        .map_err(|e| format!("{e:?}"))
        .unwrap();
    let w = daemon
        .walk
        .jobs()
        .start("test", |job| async move {
            job.cancelled().await;
            Err::<(), _>(JobError::Cancelled)
        })
        .unwrap();
    let a = daemon
        .autoeq
        .jobs()
        .start("test", |job| async move {
            let flag = job.blocking(|stop| {
                while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
            });
            flag.await?;
            Err::<(), _>(JobError::Cancelled)
        })
        .unwrap();
    assert_eq!(next(&mut walk).await, walk_event(w, JobState::Started));
    next(&mut auto).await;
    let started = std::time::Instant::now();
    daemon.shutdown().await;
    assert!(
        started.elapsed() < JOB_STOP_BOUND,
        "{:?}",
        started.elapsed()
    );
    assert_eq!(next(&mut walk).await, walk_event(w, JobState::Cancelled));
    assert_eq!(
        next(&mut auto).await,
        Event::AutoEq(JobEvent {
            job: a,
            state: JobState::Cancelled
        })
    );
    assert!(
        daemon
            .walk
            .jobs()
            .start("test", |_| async { Ok(()) })
            .is_err(),
        "nothing starts after the stop"
    );
}

/// A job stuck inside a step holds the stop for the bound and no longer: the rest of the
/// stop (here the bridge) still happens.
#[tokio::test(start_paused = true)]
async fn a_stuck_job_holds_the_shutdown_for_the_bound_and_no_longer() {
    let daemon = daemon();
    daemon
        .autoeq
        .jobs()
        .start("test", |_| async {
            std::future::pending::<()>().await;
            Ok(())
        })
        .unwrap();
    let started = tokio::time::Instant::now();
    daemon.shutdown().await;
    let took = started.elapsed();
    assert!(took >= JOB_STOP_BOUND, "{took:?}");
    assert!(took < JOB_STOP_BOUND + Duration::from_secs(1), "{took:?}");
    assert_eq!(daemon.mqtt_status(), svolder_ipc::MqttStatus::Off);
}

/// The web and MCP follow a family in-process through `Fleet::stream`, on the same job streams
/// the socket serves: a test job's events arrive in order, with its end; a request that is not
/// a stream is refused.
#[tokio::test]
async fn an_in_process_follower_sees_a_jobs_events_through_the_fleet() {
    use crate::fleet::{DaemonFleet, Fleet};
    use crate::writer::{Mode, Origin};

    let daemon = Arc::new(daemon());
    let fleet = DaemonFleet::new(Arc::downgrade(&daemon), Origin::Web, Mode::Latest);
    let mut events = fleet
        .stream(Request::Walk { op: WalkOp::Follow })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the fleet follows the walk");
    let id = daemon
        .walk
        .jobs()
        .start("test", |job| async move {
            job.progress(serde_json::json!(1));
            Ok(())
        })
        .unwrap();
    let progress = JobState::Progress {
        payload: serde_json::json!(1),
    };
    for state in [JobState::Started, progress, ended()] {
        assert_eq!(next(&mut events).await, walk_event(id, state));
    }
    let Err(refused) = fleet.stream(Request::Devices).await else {
        panic!("a plain request is not a stream");
    };
    assert!(matches!(*refused, Response::Error { .. }));
}

/// A blocking step that never looks at its stop flag holds the stop for the bound and no
/// longer: the stop returns, and the job is reported cancelled. Real time, as a paused clock does
/// not advance past a blocking thread, so the upper slack is wide for a loaded machine.
#[tokio::test]
async fn a_blocking_step_that_ignores_its_flag_does_not_hold_the_shutdown() {
    let daemon = daemon();
    let mut events = daemon
        .stream(Request::AutoEq {
            op: AutoEqOp::Follow,
        })
        .await
        .map_err(|e| format!("{e:?}"))
        .unwrap();
    let (release, hold) = std::sync::mpsc::channel::<()>();
    let id = daemon
        .autoeq
        .jobs()
        .start("test", |job| async move {
            job.blocking(move |_flag| {
                let _ = hold.recv();
            })
            .await?;
            Ok(())
        })
        .unwrap();
    next(&mut events).await;
    let started = std::time::Instant::now();
    daemon.shutdown().await;
    let took = started.elapsed();
    assert!(took >= JOB_STOP_BOUND, "{took:?}");
    assert!(took < JOB_STOP_BOUND + Duration::from_secs(3), "{took:?}");
    assert_eq!(
        next(&mut events).await,
        Event::AutoEq(JobEvent {
            job: id,
            state: JobState::Cancelled
        })
    );
    let _ = release.send(());
}

/// A start answers with the job's id as a typed response, the id is the one the stream names, and
/// a follower that comes after the end reads the job's result from its last state. A start past
/// the slot is the busy answer, and one after the stop says the daemon is stopping.
#[tokio::test]
async fn a_start_answers_the_id_the_stream_carries() {
    let daemon = daemon();
    let mut early = daemon
        .stream(Request::Walk { op: WalkOp::Follow })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the walk stream");
    let answer = daemon.walk.jobs().start_answer("walk", "test", |_| async {
        Ok(serde_json::json!({"device": "Gungnir"}))
    });
    let Response::Job { job } = answer else {
        panic!("a start answers with the job: {answer:?}");
    };
    let done = JobState::Done {
        payload: serde_json::json!({"device": "Gungnir"}),
    };
    assert_eq!(next(&mut early).await, walk_event(job, JobState::Started));
    assert_eq!(next(&mut early).await, walk_event(job, done.clone()));
    let mut late = daemon
        .stream(Request::Walk { op: WalkOp::Follow })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the walk stream");
    assert_eq!(next(&mut late).await, walk_event(job, done));

    daemon.walk.shutdown(Duration::from_secs(1)).await;
    let refused = daemon
        .walk
        .jobs()
        .start_answer("walk", "test", |_| async { Ok(()) });
    assert_eq!(
        refused,
        Response::error(svolder_ipc::CODE_USAGE, "the daemon is stopping")
            .with_reason(Reason::Stopping)
    );
}
