// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the runner turns the daemon's reply into what the box says.

use svolder_ipc::socket::protocol::{PartOutcome, PartStatus};
use tokio::sync::mpsc;

use super::*;

fn recall() -> SceneOp {
    SceneOp::Recall {
        name: "Movie".into(),
        confirm: false,
    }
}

#[test]
fn an_answered_recall_carries_its_op_name_and_outcome() {
    let outcome = vec![PartOutcome {
        who: "Hall".into(),
        status: PartStatus::Left,
        notes: vec!["Hall is not connected: its part was left as it is".into()],
    }];
    let said = meant(
        recall(),
        Ok(Response::Scene {
            name: "Movie".into(),
            outcome: outcome.clone(),
        }),
    );
    assert_eq!(
        said,
        SceneDone::Answered {
            op: recall(),
            name: "Movie".into(),
            outcome
        }
    );
}

#[test]
fn the_daemons_question_comes_back_as_a_question() {
    let said = meant(
        recall(),
        Ok(Response::Ask {
            message: "it plays twice".into(),
            name: "Movie".into(),
            devices: vec!["Desk DAC".into()],
        }),
    );
    assert_eq!(
        said,
        SceneDone::Asked {
            op: recall(),
            message: "it plays twice".into()
        }
    );
}

#[test]
fn a_refusal_is_the_daemons_own_words_whatever_its_reason() {
    for reason in [
        None,
        Some(svolder_ipc::Reason::Taken),
        Some(svolder_ipc::Reason::Busy),
    ] {
        let refusal = Response::Error {
            code: 2,
            message: "a scene named Movie is already saved".into(),
            reason,
        };
        let said = meant(recall(), Err(IpcError::Refused(Box::new(refusal))));
        assert_eq!(
            said,
            SceneDone::Refused {
                op: recall(),
                words: "a scene named Movie is already saved".into()
            },
            "{reason:?}"
        );
    }
}

#[test]
fn a_reply_of_another_kind_is_refused_in_words() {
    match meant(recall(), Ok(Response::Unknown)) {
        SceneDone::Refused { words, .. } => assert!(words.contains("unexpected reply"), "{words}"),
        other => panic!("a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn with_no_daemon_the_request_is_refused_in_the_words_z_uses() {
    let (done, mut heard) = mpsc::unbounded_channel();
    run(done, None, recall());
    match heard.recv().await {
        Some(Done::Scene(said)) => assert_eq!(
            *said,
            SceneDone::Refused {
                op: recall(),
                words: NEEDS_DAEMON.into()
            }
        ),
        other => panic!("the runner's answer, got {other:?}"),
    }
}

#[tokio::test]
async fn a_daemon_that_is_not_there_is_refused_in_words() {
    let dir = crate::test_support::temp_dir("scene-none");
    let socket = dir.as_ref().join("no.sock");
    let (done, mut heard) = mpsc::unbounded_channel();
    run(done, Some(socket), recall());
    match heard.recv().await {
        Some(Done::Scene(said)) => match *said {
            SceneDone::Refused { op, words } => {
                assert_eq!(op, recall());
                assert!(!words.is_empty());
            }
            other => panic!("a refusal, got {other:?}"),
        },
        other => panic!("the runner's answer, got {other:?}"),
    }
}

/// The real daemon's refusal comes back as an `Error` reply, not an `IpcError`: its message is
/// what the box says.
#[test]
fn an_error_reply_is_the_daemons_own_words_not_an_unexpected_reply() {
    let said = meant(
        recall(),
        Ok(Response::Error {
            code: 1,
            message: "a scene named Movie is already configured".into(),
            reason: Some(svolder_ipc::Reason::Taken),
        }),
    );
    assert_eq!(
        said,
        SceneDone::Refused {
            op: recall(),
            words: "a scene named Movie is already configured".into()
        }
    );
}
