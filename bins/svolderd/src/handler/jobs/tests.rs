// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The mechanism, driven by test-only jobs: none of these kinds is on the wire.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::StreamExt;
use svolder_ipc::{JobEvent, JobState, Reason};
use tokio::sync::Notify;

use super::*;

const BOUND: Duration = Duration::from_secs(5);

fn progress(n: u64) -> JobState {
    JobState::Progress {
        payload: serde_json::json!(n),
    }
}

/// The state of a job that finished with no result.
fn ended() -> JobState {
    JobState::Done {
        payload: serde_json::Value::Null,
    }
}

/// The next event of a stream, within the bound.
async fn next(events: &mut futures::stream::BoxStream<'static, JobEvent>) -> JobEvent {
    tokio::time::timeout(BOUND, events.next())
        .await
        .expect("an event in time")
        .expect("the stream is open")
}

/// Every state of one job up to and including its last.
async fn until_final(events: &mut futures::stream::BoxStream<'static, JobEvent>) -> Vec<JobState> {
    let mut seen = Vec::new();
    loop {
        let event = next(events).await;
        let last = event.state.is_final();
        seen.push(event.state);
        if last {
            return seen;
        }
    }
}

/// A job that says two things and ends: its id, then started, both progresses, done, in order.
#[tokio::test]
async fn a_job_starts_reports_in_order_and_is_done() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    let id = jobs
        .start("test", |job| async move {
            job.progress(serde_json::json!(1));
            job.progress(serde_json::json!(2));
            Ok(())
        })
        .expect("the slot is free");
    let seen = until_final(&mut events).await;
    assert_eq!(
        seen,
        vec![JobState::Started, progress(1), progress(2), ended()]
    );
    assert_eq!(jobs.running(), 0);
    let _ = id;
}

/// The id of a job is the one its events carry, and no id of a run repeats.
#[tokio::test]
async fn ids_never_repeat_within_a_run() {
    let jobs = Jobs::new(Slots::Up(4));
    let mut events = jobs.follow();
    let mut ids = Vec::new();
    for _ in 0..40 {
        let id = jobs
            .start("test", |_| async { Ok(()) })
            .expect("a free slot");
        ids.push(id);
        let event = next(&mut events).await;
        assert_eq!(event.job, id);
        until_final_of(&mut events, id).await;
    }
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "every id once");
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "ids only grow");
}

/// Events of `id` until its last, others skipped.
async fn until_final_of(events: &mut futures::stream::BoxStream<'static, JobEvent>, id: JobId) {
    loop {
        let event = next(events).await;
        if event.job == id && event.state.is_final() {
            return;
        }
    }
}

/// A job asked to stop between its steps ends as cancelled, and the id that was cancelled is the
/// one the event names.
#[tokio::test]
async fn a_job_is_cancelled_mid_way() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    let steps = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&steps);
    let id = jobs
        .start("test", |job| async move {
            loop {
                if job.stopped() {
                    return Err::<(), _>(JobError::Cancelled);
                }
                counted.fetch_add(1, Ordering::SeqCst);
                job.progress(serde_json::json!("step"));
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .unwrap();
    assert_eq!(next(&mut events).await.state, JobState::Started);
    assert!(matches!(
        next(&mut events).await.state,
        JobState::Progress { .. }
    ));
    jobs.cancel(id).expect("a running job");
    let last = loop {
        let event = next(&mut events).await;
        assert_eq!(event.job, id);
        if event.state.is_final() {
            break event.state;
        }
    };
    assert_eq!(last, JobState::Cancelled);
    let at_end = steps.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        steps.load(Ordering::SeqCst),
        at_end,
        "no step after the stop"
    );
}

/// Work done on a blocking thread sees the stop flag between its steps, and the job ends as
/// cancelled once the thread has.
#[tokio::test]
async fn blocking_work_stops_at_its_next_step() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    let steps = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&steps);
    let id = jobs
        .start("test", |job| async move {
            job.blocking(move |stop| {
                while !stop.load(Ordering::SeqCst) {
                    counted.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
            .await?;
            Err::<(), _>(JobError::Cancelled)
        })
        .unwrap();
    assert_eq!(next(&mut events).await.state, JobState::Started);
    while steps.load(Ordering::SeqCst) < 2 {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    jobs.cancel(id).unwrap();
    assert_eq!(until_final(&mut events).await, vec![JobState::Cancelled]);
}

/// A job that fails says why, with the reason it chose.
#[tokio::test]
async fn a_failure_carries_its_reason() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    jobs.start("test", |_| async {
        Err::<(), _>(JobError::failed("no link", Reason::NotConnected))
    })
    .unwrap();
    assert_eq!(
        until_final(&mut events).await,
        vec![
            JobState::Started,
            JobState::Failed {
                message: "no link".into(),
                reason: Reason::NotConnected
            }
        ]
    );
}

/// A job that panics fails with an internal reason and frees its slot.
#[tokio::test]
async fn a_panicking_job_fails_and_frees_its_slot() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    jobs.start("test", |_| async {
        if true {
            panic!("a test job panics");
        }
        Ok(())
    })
    .unwrap();
    let seen = until_final(&mut events).await;
    assert!(
        matches!(
            seen.last(),
            Some(JobState::Failed {
                reason: Reason::Internal,
                ..
            })
        ),
        "{seen:?}"
    );
    assert!(
        jobs.start("test", |_| async { Ok(()) }).is_ok(),
        "the slot is free"
    );
}

/// A subscriber that comes after a job ended reads its last state first; one that comes while
/// it runs reads where it stands, and then the rest.
#[tokio::test]
async fn a_late_subscriber_gets_the_last_state() {
    let jobs = Jobs::new(Slots::Up(2));
    let gate = Arc::new(Notify::new());
    let held = Arc::clone(&gate);
    let running = jobs
        .start("test", |job| async move {
            job.progress(serde_json::json!("halfway"));
            held.notified().await;
            Ok(())
        })
        .unwrap();
    let mut early = jobs.follow();
    assert_eq!(next(&mut early).await.state, JobState::Started);
    assert_eq!(
        next(&mut early).await.state,
        JobState::Progress {
            payload: serde_json::json!("halfway")
        }
    );
    let done = jobs
        .start("test", |_| async {
            Err::<(), _>(JobError::failed("odd", Reason::Internal))
        })
        .unwrap();
    // Wait for the failure to land.
    loop {
        let event = next(&mut early).await;
        if event.job == done && event.state.is_final() {
            break;
        }
    }
    let mut late = jobs.follow();
    let first = next(&mut late).await;
    assert_eq!(first.job, done);
    assert!(matches!(first.state, JobState::Failed { .. }), "{first:?}");
    let second = next(&mut late).await;
    assert_eq!(second.job, running);
    assert_eq!(
        second.state,
        JobState::Progress {
            payload: serde_json::json!("halfway")
        }
    );
    gate.notify_one();
    assert_eq!(next(&mut late).await.state, ended());
}

/// A finished job is remembered up to a count; one past it is forgotten, and asking to cancel
/// it is asking about a job the daemon does not hold.
#[tokio::test]
async fn only_the_last_finished_jobs_are_remembered() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    let mut ids = Vec::new();
    for _ in 0..KEPT_FINISHED + 3 {
        let id = jobs.start("test", |_| async { Ok(()) }).unwrap();
        until_final_of(&mut events, id).await;
        ids.push(id);
    }
    let mut late = jobs.follow();
    let mut remembered = Vec::new();
    for _ in 0..KEPT_FINISHED {
        remembered.push(next(&mut late).await.job);
    }
    assert_eq!(remembered, ids[3..], "the newest are kept, oldest first");
    assert_eq!(jobs.cancel(ids[0]), Err(NoSuchJob));
    assert_eq!(
        jobs.cancel(ids[ids.len() - 1]),
        Ok(()),
        "an ended job needs no stopping"
    );
    assert_eq!(jobs.cancel(9_999), Err(NoSuchJob));
}

/// A family that runs one job at a time refuses a second start, naming whose job is in the way,
/// and takes one when the first has ended.
#[tokio::test]
async fn a_second_start_in_a_single_slot_family_is_busy() {
    let jobs = Jobs::new(Slots::One);
    let gate = Arc::new(Notify::new());
    let held = Arc::clone(&gate);
    let mut events = jobs.follow();
    let first = jobs
        .start("the TUI", |_| async move {
            held.notified().await;
            Ok(())
        })
        .unwrap();
    let busy = jobs.start("the web", |_| async { Ok(()) });
    assert_eq!(
        busy,
        Err(Refusal::Busy {
            origin: "the TUI".into(),
            job: first,
            slots: 1
        })
    );
    gate.notify_one();
    until_final_of(&mut events, first).await;
    assert!(jobs.start("the web", |_| async { Ok(()) }).is_ok());
}

/// A family with a cap runs that many at once and refuses the next.
#[tokio::test]
async fn a_capped_family_runs_that_many_at_once() {
    let jobs = Jobs::new(Slots::Up(2));
    let gate = Arc::new(Notify::new());
    let start = |origin: &str| {
        let held = Arc::clone(&gate);
        jobs.start(origin, |_| async move {
            held.notified().await;
            Ok(())
        })
    };
    assert!(start("a").is_ok());
    assert!(start("b").is_ok());
    assert!(matches!(start("c"), Err(Refusal::Busy { origin, .. }) if origin == "a"));
    gate.notify_waiters();
}

/// A subscriber that never reads does not hold the job up, nor another subscriber: the job
/// finishes, a reader keeping pace sees every step, and the one that fell behind is brought
/// up to date with where the job stands.
#[tokio::test]
async fn a_slow_subscriber_blocks_nothing() {
    let jobs = Jobs::with_backlog(Slots::One, 8);
    let mut slow = jobs.follow();
    let mut quick = jobs.follow();
    let reader = tokio::spawn(async move {
        let mut seen = Vec::new();
        loop {
            let event = next(&mut quick).await;
            let last = event.state.is_final();
            seen.push(event.state);
            if last {
                return seen;
            }
        }
    });
    let id = jobs
        .start("test", |job| async move {
            for n in 0..60 {
                job.progress(serde_json::json!(n));
                tokio::task::yield_now().await;
            }
            Ok(())
        })
        .unwrap();
    let seen = tokio::time::timeout(BOUND, reader)
        .await
        .expect("the job and the quick reader finish with the slow one unread")
        .unwrap();
    assert_eq!(seen.len(), 62, "started, 60 steps, done");
    assert_eq!(seen.last(), Some(&ended()));
    // The slow one reads now: it lost events, and is told where the job stands.
    let mut tail = None;
    let mut read = 0;
    for _ in 0..80 {
        let event = next(&mut slow).await;
        read += 1;
        assert_eq!(event.job, id);
        if event.state.is_final() {
            tail = Some(event.state);
            break;
        }
    }
    assert_eq!(tail, Some(ended()));
    assert!(
        read < 62,
        "the slow reader skipped events and was resynced, read {read}"
    );
}

/// Stopping all jobs asks each to stop and waits for them: a cooperative job ends cancelled.
#[tokio::test]
async fn stopping_all_cancels_a_running_job_and_waits_for_it() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    let id = jobs
        .start("test", |job| async move {
            job.cancelled().await;
            Err::<(), _>(JobError::Cancelled)
        })
        .unwrap();
    assert_eq!(next(&mut events).await.state, JobState::Started);
    let started = tokio::time::Instant::now();
    let stuck = jobs.stop_all(BOUND).await;
    assert_eq!(stuck, 0);
    assert!(started.elapsed() < BOUND);
    assert_eq!(
        next(&mut events).await,
        JobEvent {
            job: id,
            state: JobState::Cancelled
        }
    );
}

/// A job that does not look at its stop is waited for only as long as the bound, then given up
/// on and reported cancelled: stopping returns.
#[tokio::test]
async fn stopping_all_goes_on_after_the_bound_for_a_job_that_ignores_it() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    let id = jobs
        .start("test", |_| async {
            std::future::pending::<()>().await;
            Ok(())
        })
        .unwrap();
    assert_eq!(next(&mut events).await.state, JobState::Started);
    let bound = Duration::from_millis(80);
    let started = tokio::time::Instant::now();
    let stuck = jobs.stop_all(bound).await;
    let took = started.elapsed();
    assert_eq!(stuck, 1);
    assert!(took >= bound && took < BOUND, "{took:?}");
    assert_eq!(
        next(&mut events).await,
        JobEvent {
            job: id,
            state: JobState::Cancelled
        }
    );
    assert_eq!(jobs.running(), 0);
}

/// After the stop no job starts: a start that comes later is refused.
#[tokio::test]
async fn nothing_starts_after_stopping_all() {
    let jobs = Jobs::new(Slots::One);
    assert_eq!(jobs.stop_all(BOUND).await, 0);
    assert_eq!(
        jobs.start("test", |_| async { Ok(()) }),
        Err(Refusal::Stopping)
    );
}

/// The answers a family gives for its refusals: busy names who holds the slot, and a job
/// that is not held is a typed refusal.
#[test]
fn the_refusals_answer_in_words_and_reasons() {
    let busy = Refusal::Busy {
        origin: "the TUI".into(),
        job: 7,
        slots: 1,
    }
    .answer("walk");
    let Response::Error {
        message, reason, ..
    } = busy
    else {
        panic!("busy is an error answer");
    };
    assert!(
        message.contains("walk") && message.contains("the TUI") && message.contains('7'),
        "{message}"
    );
    assert_eq!(reason, Some(Reason::Busy));
    let none = NoSuchJob.answer(9);
    assert!(
        matches!(
            none,
            Response::Error {
                reason: Some(Reason::NoSuchJob),
                ..
            }
        ),
        "{none:?}"
    );
}

/// What a job returns is its result: it rides in `done`, and a subscriber that joins after the end
/// reads it from the remembered last state, with no progress to have caught.
#[tokio::test]
async fn a_result_is_in_the_last_state_a_late_follower_reads() {
    let jobs = Jobs::new(Slots::One);
    let mut early = jobs.follow();
    let id = jobs
        .start("test", |_| async {
            Ok(serde_json::json!({"device": "Gungnir"}))
        })
        .expect("the slot is free");
    let done = JobState::Done {
        payload: serde_json::json!({"device": "Gungnir"}),
    };
    assert_eq!(until_final(&mut early).await.last(), Some(&done));
    let mut late = jobs.follow();
    assert_eq!(
        next(&mut late).await,
        JobEvent {
            job: id,
            state: done
        }
    );
}

/// A result past the bound is not sent: the job ends as failed, with the reason `internal`.
#[tokio::test]
async fn a_result_past_the_bound_fails_the_job() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    jobs.start("test", |_| async {
        Ok(serde_json::json!("x".repeat(JOB_RESULT_MAX_BYTES)))
    })
    .expect("the slot is free");
    let seen = until_final(&mut events).await;
    assert!(
        matches!(
            seen.last(),
            Some(JobState::Failed {
                reason: Reason::Internal,
                ..
            })
        ),
        "{seen:?}"
    );
    let fits = serde_json::json!("x".repeat(JOB_RESULT_MAX_BYTES - 2));
    jobs.start("test", move |_| async move { Ok(fits) })
        .expect("the slot is free");
    assert!(matches!(
        until_final(&mut events).await.last(),
        Some(JobState::Done { .. })
    ));
}

/// A result of the largest size each, many ended jobs: a late follower is replayed no more than
/// the replay bound (the newest job always), not sixteen full results.
#[tokio::test]
async fn the_replay_of_ended_jobs_is_bounded_in_bytes() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    let big = serde_json::json!("x".repeat(JOB_RESULT_MAX_BYTES - 2));
    let mut last = 0;
    for _ in 0..KEPT_FINISHED {
        let value = big.clone();
        last = jobs
            .start("test", move |_| async move { Ok(value) })
            .expect("the slot is free");
        until_final_of(&mut events, last).await;
    }
    let mut late = jobs.follow();
    let mut bytes = 0;
    let mut ids = Vec::new();
    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(200), late.next()).await
    {
        bytes += serde_json::to_string(&event).expect("serializes").len();
        ids.push(event.job);
    }
    assert!(bytes <= REPLAY_BYTES_MAX + 1024, "{bytes} bytes replayed");
    assert_eq!(ids.last(), Some(&last), "the newest job is kept");
    assert!(!ids.is_empty());
}

/// A progress payload past the cap is not sent; one within it is.
#[tokio::test]
async fn a_progress_payload_past_the_cap_is_dropped() {
    let jobs = Jobs::new(Slots::One);
    let mut events = jobs.follow();
    jobs.start("test", |job| async move {
        job.progress(serde_json::json!("x".repeat(PROGRESS_MAX_BYTES)));
        job.progress(serde_json::json!("small"));
        Ok(())
    })
    .expect("the slot is free");
    let seen = until_final(&mut events).await;
    assert_eq!(
        seen,
        vec![
            JobState::Started,
            JobState::Progress {
                payload: serde_json::json!("small")
            },
            ended()
        ]
    );
}

/// The refusal's article follows the family's first letter, and the origins have the words a
/// person is told, not the log's.
#[test]
fn a_busy_answer_reads_as_a_sentence() {
    let busy = |family| match (Refusal::Busy {
        origin: "the terminal".into(),
        job: 3,
        slots: 1,
    })
    .answer(family)
    {
        Response::Error { message, .. } => message,
        other => panic!("not an error: {other:?}"),
    };
    assert_eq!(
        busy("AutoEq job"),
        "an AutoEq job is already running (job 3, started by the terminal)"
    );
    assert_eq!(
        busy("walk"),
        "a walk is already running (job 3, started by the terminal)"
    );
    assert_eq!(crate::writer::Origin::Socket.who(), "the terminal");
    assert_eq!(crate::writer::Origin::Web.who(), "the web page");
}

/// A family with several slots that is full says how many jobs run, not that one does.
#[tokio::test]
async fn a_full_family_of_several_slots_says_how_many_run() {
    let jobs = Jobs::new(Slots::Up(2));
    for _ in 0..2 {
        jobs.start("the TUI", |_| {
            std::future::pending::<Result<(), JobError>>()
        })
        .unwrap();
    }
    let Err(busy) = jobs.start("the web", |_| async { Ok(()) }) else {
        panic!("a third job started");
    };
    let Response::Error {
        message, reason, ..
    } = busy.answer("AutoEq job")
    else {
        panic!("not an error");
    };
    assert_eq!(
        message,
        "two AutoEq jobs are running; try again when one ends"
    );
    assert_eq!(reason, Some(Reason::Busy));
}
