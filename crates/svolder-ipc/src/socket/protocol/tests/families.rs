// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The family envelopes and the stream rule: the shapes on the wire, which requests are streams,
//! and what a front end that predates a family reads of it.

use super::*;

fn request(text: &str) -> Request {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// The cancel and follow ops of each family are the `op` of a `walk` or `auto_eq` request, as a
/// `host_eq` request carries its own.
#[test]
fn the_family_requests_have_their_wire_shapes() {
    let cases = [
        (
            Request::Walk {
                op: WalkOp::Cancel { job: 3 },
            },
            r#"{"cmd":"walk","op":{"op":"cancel","job":3}}"#,
        ),
        (
            Request::Walk { op: WalkOp::Follow },
            r#"{"cmd":"walk","op":{"op":"follow"}}"#,
        ),
        (
            Request::AutoEq {
                op: AutoEqOp::Cancel { job: 9 },
            },
            r#"{"cmd":"auto_eq","op":{"op":"cancel","job":9}}"#,
        ),
        (
            Request::AutoEq {
                op: AutoEqOp::Follow,
            },
            r#"{"cmd":"auto_eq","op":{"op":"follow"}}"#,
        ),
    ];
    for (parsed, text) in cases {
        assert_eq!(serde_json::to_string(&parsed).unwrap(), text);
        assert_eq!(request(text), parsed);
    }
}

/// A stream is a request the server answers with events: the two that exist and the follow op of
/// each family; no other request is one, including the cancel ops.
#[test]
fn only_the_stream_requests_are_streams() {
    let streams = [
        Request::Subscribe {
            device: None,
            host: false,
        },
        Request::Subscribe {
            device: Some("amp".into()),
            host: true,
        },
        Request::HostFrames,
        Request::Walk { op: WalkOp::Follow },
        Request::AutoEq {
            op: AutoEqOp::Follow,
        },
    ];
    for r in &streams {
        assert!(r.is_stream(), "{r:?}");
    }
    let not = [
        Request::Devices,
        Request::Version,
        Request::Reload,
        Request::Scan { window_ms: 1 },
        Request::Walk {
            op: WalkOp::Cancel { job: 1 },
        },
        Request::AutoEq {
            op: AutoEqOp::Cancel { job: 1 },
        },
    ];
    for r in &not {
        assert!(!r.is_stream(), "{r:?}");
    }
}

/// A stream that is new says so with an acknowledgement when it opens; the two that exist send
/// none, so their wire is what it was.
#[test]
fn only_the_new_streams_acknowledge_their_opening() {
    assert!(Request::Walk { op: WalkOp::Follow }.stream_acks());
    assert!(
        Request::AutoEq {
            op: AutoEqOp::Follow
        }
        .stream_acks()
    );
    assert!(
        !Request::Subscribe {
            device: None,
            host: false
        }
        .stream_acks()
    );
    assert!(!Request::HostFrames.stream_acks());
    assert!(!Request::Devices.stream_acks());
}

/// The state of a job in each family's event, with the job's id beside it.
#[test]
fn a_job_event_has_its_wire_shape() {
    let cases = [
        (
            JobState::Started,
            r#"{"event":"walk","job":4,"state":{"kind":"started"}}"#,
        ),
        (
            JobState::Progress {
                payload: serde_json::json!({"seen": 2}),
            },
            r#"{"event":"walk","job":4,"state":{"kind":"progress","payload":{"seen":2}}}"#,
        ),
        (
            JobState::Done {
                payload: serde_json::Value::Null,
            },
            r#"{"event":"walk","job":4,"state":{"kind":"done"}}"#,
        ),
        (
            JobState::Done {
                payload: serde_json::json!({"device": "Gungnir"}),
            },
            r#"{"event":"walk","job":4,"state":{"kind":"done","payload":{"device":"Gungnir"}}}"#,
        ),
        (
            JobState::Failed {
                message: "no luck".into(),
                reason: Reason::Busy,
            },
            r#"{"event":"walk","job":4,"state":{"kind":"failed","message":"no luck","reason":"busy"}}"#,
        ),
        (
            JobState::Cancelled,
            r#"{"event":"walk","job":4,"state":{"kind":"cancelled"}}"#,
        ),
    ];
    for (state, text) in cases {
        let event = Event::Walk(JobEvent { job: 4, state });
        assert_eq!(serde_json::to_string(&event).unwrap(), text);
        assert_eq!(serde_json::from_str::<Event>(text).unwrap(), event);
    }
    let auto = Event::AutoEq(JobEvent {
        job: 5,
        state: JobState::Done {
            payload: serde_json::Value::Null,
        },
    });
    assert_eq!(
        serde_json::to_string(&auto).unwrap(),
        r#"{"event":"auto_eq","job":5,"state":{"kind":"done"}}"#
    );
}

/// A state this build does not know reads as unknown and the job's id survives; a progress
/// without a payload carries none; a failure without a reason reads as an unknown one.
#[test]
fn a_job_event_tolerates_what_a_later_build_adds() {
    let later = r#"{"event":"walk","job":2,"state":{"kind":"paused","why":"x"}}"#;
    assert_eq!(
        serde_json::from_str::<Event>(later).unwrap(),
        Event::Walk(JobEvent {
            job: 2,
            state: JobState::Unknown
        })
    );
    let bare = r#"{"event":"walk","job":2,"state":{"kind":"progress"}}"#;
    assert_eq!(
        serde_json::from_str::<Event>(bare).unwrap(),
        Event::Walk(JobEvent {
            job: 2,
            state: JobState::Progress {
                payload: serde_json::Value::Null
            }
        })
    );
    let old = r#"{"event":"walk","job":2,"state":{"kind":"failed","message":"m"}}"#;
    assert_eq!(
        serde_json::from_str::<Event>(old).unwrap(),
        Event::Walk(JobEvent {
            job: 2,
            state: JobState::Failed {
                message: "m".into(),
                reason: Reason::Unknown
            }
        })
    );
}

/// Only a job's last three states end it.
#[test]
fn a_job_ends_in_done_failed_or_cancelled() {
    assert!(!JobState::Started.is_final());
    assert!(
        !JobState::Progress {
            payload: serde_json::Value::Null
        }
        .is_final()
    );
    assert!(
        JobState::Done {
            payload: serde_json::json!(1)
        }
        .is_final()
    );
    assert!(JobState::Cancelled.is_final());
    assert!(
        JobState::Failed {
            message: String::new(),
            reason: Reason::Internal
        }
        .is_final()
    );
}

/// The reasons the job mechanism gives are words on the wire, and a front end built before them
/// reads each as an unknown reason.
#[test]
fn the_job_reasons_are_words() {
    for (reason, word) in [
        (Reason::NoSuchJob, "no_such_job"),
        (Reason::Busy, "busy"),
        (Reason::Internal, "internal"),
    ] {
        assert_eq!(
            serde_json::to_string(&reason).unwrap(),
            format!("\"{word}\"")
        );
        assert_eq!(
            serde_json::from_str::<Reason>(&format!("\"{word}\"")).unwrap(),
            reason
        );
    }
}

/// A 0.32.7 daemon knows neither family: its request kinds are what the guard lists, and the
/// two new ones are in the list.
#[test]
fn the_request_kinds_list_names_the_families() {
    assert!(REQUEST_CMDS.contains(&"walk"));
    assert!(REQUEST_CMDS.contains(&"auto_eq"));
}

/// Every stream op of a family acks its opening, none of the two older streams does, and a
/// client waits for the first line of every stream but a subscription.
#[test]
fn the_ack_and_wait_rules_hold_for_every_stream() {
    let families = [
        Request::Walk { op: WalkOp::Follow },
        Request::AutoEq {
            op: AutoEqOp::Follow,
        },
    ];
    for r in &families {
        assert!(r.stream_acks() && r.stream_waits(), "{r:?}");
    }
    let sub = Request::Subscribe {
        device: None,
        host: true,
    };
    assert!(!sub.stream_acks() && !sub.stream_waits());
    assert!(!Request::HostFrames.stream_acks() && Request::HostFrames.stream_waits());
    assert!(!Request::Devices.stream_waits());
}

/// The reasons the config writers give are words on the wire, each round-trips, and a reason a
/// later daemon adds still reads as unknown.
#[test]
fn the_config_reasons_are_words() {
    for (reason, word) in [
        (Reason::Taken, "taken"),
        (Reason::Missing, "missing"),
        (Reason::ReadOnly, "read_only"),
        (Reason::FileBusy, "file_busy"),
        (Reason::BadName, "bad_name"),
        (Reason::Invalid, "invalid"),
        (Reason::Stopping, "stopping"),
    ] {
        assert_eq!(
            serde_json::to_string(&reason).unwrap(),
            format!("\"{word}\"")
        );
        assert_eq!(
            serde_json::from_str::<Reason>(&format!("\"{word}\"")).unwrap(),
            reason
        );
    }
    assert_eq!(
        serde_json::from_str::<Reason>("\"a_later_word\"").unwrap(),
        Reason::Unknown
    );
}

/// A start's answer is the job's id, and the line is additive: nothing an older front end sends
/// is changed by it.
#[test]
fn the_start_answer_carries_the_job_id() {
    let answer = Response::Job { job: 9 };
    let text = r#"{"result":"job","job":9}"#;
    assert_eq!(serde_json::to_string(&answer).unwrap(), text);
    assert_eq!(serde_json::from_str::<Response>(text).unwrap(), answer);
}

/// Every refusal of a config writer maps to one reason, whatever the words.
#[test]
fn a_config_refusal_has_one_reason() {
    use crate::ConfigError;
    use svolder_profiles::tone::preset::NameRefusal;
    let path = std::path::PathBuf::from("/x/config.toml");
    let cases = [
        (
            ConfigError::Taken {
                kind: "preset",
                name: "A".into(),
            },
            Reason::Taken,
        ),
        (
            ConfigError::Missing {
                kind: "device",
                name: "A".into(),
            },
            Reason::Missing,
        ),
        (ConfigError::ReadOnly(path.clone()), Reason::ReadOnly),
        (ConfigError::Busy(path), Reason::FileBusy),
        (
            ConfigError::Name(NameRefusal::Taken("A".into())),
            Reason::Taken,
        ),
        (
            ConfigError::Name(NameRefusal::BuiltIn("Flat".into())),
            Reason::BadName,
        ),
        (ConfigError::Parse("x".into()), Reason::Invalid),
        (ConfigError::Schema(2), Reason::Invalid),
        (
            ConfigError::UnitTwice {
                first: "a".into(),
                second: "b".into(),
                udid: "u".into(),
            },
            Reason::Invalid,
        ),
        (
            ConfigError::Vanished {
                kind: "hop",
                name: "A".into(),
            },
            Reason::Internal,
        ),
    ];
    for (why, reason) in cases {
        assert_eq!(why.reason(), reason, "{why}");
    }
}
