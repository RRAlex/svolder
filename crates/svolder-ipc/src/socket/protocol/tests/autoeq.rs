// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq ops and answers: their wire shapes, and that the largest answer fits a line.

use super::*;
use crate::socket::lines::MAX_LINE;

fn variant() -> AutoEqVariant {
    AutoEqVariant {
        name: "Sennheiser HD 650".into(),
        source: "oratory1990".into(),
        form: "over-ear".into(),
        rig: None,
    }
}

#[test]
fn the_ops_have_their_wire_shapes() {
    let cases = [
        (
            AutoEqOp::Search {
                query: "hd650".into(),
                refresh: false,
            },
            r#"{"op":"search","query":"hd650","refresh":false}"#,
        ),
        (
            AutoEqOp::Preview {
                variant: Box::new(variant()),
                target: AutoEqTarget::Device {
                    device: "Gungnir".into(),
                },
                compare: None,
            },
            r#"{"op":"preview","variant":{"name":"Sennheiser HD 650","source":"oratory1990","form":"over-ear"},"target":{"side":"device","device":"Gungnir"}}"#,
        ),
        (
            AutoEqOp::Import {
                variant: Box::new(variant()),
                name: Some("HD 650".into()),
                force: true,
                target: AutoEqTarget::Computer,
            },
            r#"{"op":"import","variant":{"name":"Sennheiser HD 650","source":"oratory1990","form":"over-ear"},"name":"HD 650","force":true,"target":{"side":"computer"}}"#,
        ),
        (
            AutoEqOp::Upload {
                text: "Preamp: -1 dB".into(),
                name: "mine".into(),
                force: false,
            },
            r#"{"op":"upload","text":"Preamp: -1 dB","name":"mine","force":false}"#,
        ),
    ];
    for (op, line) in cases {
        let request = Request::AutoEq { op };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            format!(r#"{{"cmd":"auto_eq","op":{line}}}"#)
        );
        let back: Request = serde_json::from_str(&to_line(&request).unwrap()).unwrap();
        assert_eq!(back, request);
        assert!(!request.is_stream());
    }
}

/// Only the follow op is a stream; the ops with a result are plain requests.
#[test]
fn the_defaults_fill_what_a_front_end_leaves_out() {
    let op: AutoEqOp = serde_json::from_str(r#"{"op":"search","query":"x"}"#).unwrap();
    assert_eq!(
        op,
        AutoEqOp::Search {
            query: "x".into(),
            refresh: false
        }
    );
    let op: AutoEqOp = serde_json::from_str(
        r#"{"op":"import","variant":{"name":"a","source":"b","form":"in-ear","rig":"711"},"target":{"side":"computer"}}"#,
    )
    .unwrap();
    assert!(matches!(
        op,
        AutoEqOp::Import {
            name: None,
            force: false,
            ..
        }
    ));
}

#[test]
fn the_daemon_lists_the_feature_and_the_new_reasons_are_words() {
    assert!(FEATURES.contains(&"autoeq"));
    assert_eq!(
        serde_json::to_string(&Reason::NotListed).unwrap(),
        r#""not_listed""#
    );
    assert_eq!(
        serde_json::to_string(&Reason::Unreachable).unwrap(),
        r#""unreachable""#
    );
}

#[test]
fn the_search_and_saved_answers_round_trip() {
    let search = Response::AutoEqSearch(Box::new(AutoEqSearch {
        groups: vec![AutoEqGroup {
            key: "sennheiserhd650".into(),
            name: "Sennheiser HD 650".into(),
            variants: vec![AutoEqMeasurement {
                variant: variant(),
                words: "oratory1990 · over-ear".into(),
            }],
        }],
        more: 3,
        measurements: 5000,
        fetched_unix: 1_700_000_000,
        note: None,
    }));
    let line = to_line(&search).unwrap();
    assert!(line.contains(r#""result":"auto_eq_search""#), "{line}");
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), search);
    let saved = Response::AutoEqSaved(AutoEqSaved {
        name: "HD 650".into(),
        filters: 10,
        skipped: 1,
    });
    let line = to_line(&saved).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), saved);
}

/// A large search answer (every group at the cap, eight measurements each, 40-character names)
/// is inside the 1 MiB line; the daemon trims what would not be.
#[test]
fn the_largest_search_answer_fits_a_line() {
    let long = "x".repeat(40);
    let measurement = AutoEqMeasurement {
        variant: AutoEqVariant {
            name: long.clone(),
            source: long.clone(),
            form: "over-ear".into(),
            rig: Some(long.clone()),
        },
        words: format!("{long} · over-ear · {long}"),
    };
    let group = AutoEqGroup {
        key: long.clone(),
        name: long,
        variants: vec![measurement; 8],
    };
    let answer = Response::AutoEqSearch(Box::new(AutoEqSearch {
        groups: vec![group; AUTOEQ_GROUPS_MAX],
        more: 10_000,
        measurements: 10_000,
        fetched_unix: 0,
        note: Some("n".repeat(300)),
    }));
    let line = to_line(&answer).unwrap();
    assert!(line.len() < MAX_LINE, "{} bytes", line.len());
}
