// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The walk's ops on the wire: their shapes, the answer that never echoes what it carries, and
//! the feature that says a daemon has them.

use super::*;

fn request(text: &str) -> Request {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// Each op is the `op` of a `walk` request; every op but `start` and `follow` names its job.
#[test]
fn the_walk_ops_have_their_wire_shapes() {
    let cases = [
        (WalkOp::Start, r#"{"cmd":"walk","op":{"op":"start"}}"#),
        (
            WalkOp::Answer {
                job: 4,
                choice: 1,
                text: None,
            },
            r#"{"cmd":"walk","op":{"op":"answer","job":4,"choice":1}}"#,
        ),
        (
            WalkOp::Answer {
                job: 4,
                choice: 0,
                text: Some("desk".into()),
            },
            r#"{"cmd":"walk","op":{"op":"answer","job":4,"choice":0,"text":"desk"}}"#,
        ),
        (
            WalkOp::Back { job: 4 },
            r#"{"cmd":"walk","op":{"op":"back","job":4}}"#,
        ),
        (
            WalkOp::Cancel { job: 4 },
            r#"{"cmd":"walk","op":{"op":"cancel","job":4}}"#,
        ),
        (WalkOp::Follow, r#"{"cmd":"walk","op":{"op":"follow"}}"#),
    ];
    for (op, text) in cases {
        let parsed = Request::Walk { op };
        assert_eq!(serde_json::to_string(&parsed).unwrap(), text);
        assert_eq!(request(text), parsed);
    }
    // A choice left out is the first one.
    assert_eq!(
        request(r#"{"cmd":"walk","op":{"op":"answer","job":4,"text":"1234"}}"#),
        Request::Walk {
            op: WalkOp::Answer {
                job: 4,
                choice: 0,
                text: Some("1234".into()),
            }
        }
    );
}

/// An answer may carry a passphrase: one that does not parse is refused naming the key at fault
/// and never what it held, as a bare op and inside a request line, and an op printed with `{:?}`
/// hides its text.
#[test]
fn an_answer_never_shows_what_it_carries() {
    for line in [
        r#"{"cmd":"walk","op":{"op":"answer","job":4,"text":4321}}"#,
        r#"{"id":7,"request":{"cmd":"walk","op":{"op":"answer","job":4,"text":4321}}}"#,
        r#"{"cmd":"walk","op":{"op":"answer","job":"4321","text":"1234"}}"#,
    ] {
        let refused = match line.starts_with(r#"{"id""#) {
            true => serde_json::from_str::<RequestLine>(line).map(drop),
            false => serde_json::from_str::<Request>(line).map(drop),
        }
        .expect_err(line)
        .to_string();
        assert!(!refused.contains("4321"), "{line}: {refused}");
        assert!(!refused.contains("1234"), "{line}: {refused}");
        assert!(!refused.contains("missing field `request`"), "{refused}");
    }
    let op = WalkOp::Answer {
        job: 4,
        choice: 0,
        text: Some("4321".into()),
    };
    assert!(!format!("{op:?}").contains("4321"), "{op:?}");
}

/// Only the follow op is a stream.
#[test]
fn only_follow_is_a_walk_stream() {
    assert!(WalkOp::Follow.is_stream());
    for op in [
        WalkOp::Start,
        WalkOp::Back { job: 1 },
        WalkOp::Answer {
            job: 1,
            choice: 0,
            text: None,
        },
    ] {
        assert!(!op.is_stream(), "{op:?}");
        assert!(!Request::Walk { op }.is_stream());
    }
}

/// A daemon whose walk starts lists the feature `walk`, and an answer to a walk that is not
/// waiting is refused with its own word.
#[test]
fn the_walk_feature_and_its_reason() {
    assert!(FEATURES.contains(&FEATURE_WALK));
    assert_eq!(FEATURE_WALK, "walk");
    assert_eq!(
        serde_json::to_string(&Reason::NotWaiting).unwrap(),
        r#""not_waiting""#
    );
}

/// How a passphrase test went travels as a tagged word.
#[test]
fn a_connect_outcome_has_its_wire_shape() {
    let cases = [
        (Connect::Accepted, r#"{"outcome":"accepted"}"#),
        (Connect::Refused, r#"{"outcome":"refused"}"#),
        (
            Connect::Failed {
                kind: ConnectFailure::NotInRange,
            },
            r#"{"outcome":"failed","kind":"not_in_range"}"#,
        ),
    ];
    for (outcome, text) in cases {
        assert_eq!(serde_json::to_string(&outcome).unwrap(), text);
        assert_eq!(serde_json::from_str::<Connect>(text).unwrap(), outcome);
    }
    assert_eq!(
        serde_json::from_str::<Connect>(r#"{"outcome":"later"}"#).unwrap(),
        Connect::Unknown
    );
}
