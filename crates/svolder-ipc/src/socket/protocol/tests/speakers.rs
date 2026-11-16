// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The virtual speakers ops and answers: their wire shapes, the stream rule, and that the
//! largest catalogue answer fits a line.

use super::*;
use crate::socket::lines::MAX_LINE;

fn set(id: &str) -> SpeakersSet {
    SpeakersSet {
        id: id.into(),
        room: SpeakersRoom::No,
        label: "Dataset, subject".into(),
        licence: "CC-BY-4.0".into(),
        credit: "Credit words that a licence asks for, repeated to make the line long enough"
            .into(),
        source_url: "https://example.org/dataset".into(),
        bytes: 1_100_000,
        own: false,
        fetched: true,
        damaged: None,
        angles: Some(SpeakersAngles {
            left_deg: 30.0,
            right_deg: 330.0,
            distance_m: 1.2,
        }),
        taps_ms: 12.5,
        rate: 48_000,
        loudness_db: -6.5,
        cut: false,
    }
}

#[test]
fn the_ops_have_their_wire_shapes() {
    let cases = [
        (SpeakersOp::Catalogue, r#"{"op":"catalogue"}"#),
        (
            SpeakersOp::Fetch {
                id: "mit-normal".into(),
            },
            r#"{"op":"fetch","id":"mit-normal"}"#,
        ),
        (
            SpeakersOp::Import {
                path: "/home/me/ir.sofa".into(),
                label: Some("Mine".into()),
                room: Some(SpeakersRoom::With),
            },
            r#"{"op":"import","path":"/home/me/ir.sofa","label":"Mine","room":"with"}"#,
        ),
        (
            SpeakersOp::Import {
                path: "/home/me/ir.wav".into(),
                label: None,
                room: None,
            },
            r#"{"op":"import","path":"/home/me/ir.wav"}"#,
        ),
        (
            SpeakersOp::Forget {
                id: "mit-normal".into(),
            },
            r#"{"op":"forget","id":"mit-normal"}"#,
        ),
        (SpeakersOp::ImportDir, r#"{"op":"import_dir"}"#),
        (SpeakersOp::Cancel { job: 4 }, r#"{"op":"cancel","job":4}"#),
        (SpeakersOp::Follow, r#"{"op":"follow"}"#),
    ];
    for (op, text) in cases {
        assert_eq!(serde_json::to_string(&op).unwrap(), text);
        assert_eq!(serde_json::from_str::<SpeakersOp>(text).unwrap(), op);
    }
}

#[test]
fn the_request_and_its_stream_rule() {
    let follow = Request::Speakers {
        op: SpeakersOp::Follow,
    };
    assert_eq!(
        serde_json::to_string(&follow).unwrap(),
        r#"{"cmd":"speakers","op":{"op":"follow"}}"#
    );
    assert!(follow.is_stream() && follow.stream_acks() && follow.stream_waits());
    for op in [
        SpeakersOp::Catalogue,
        SpeakersOp::Cancel { job: 1 },
        SpeakersOp::Fetch { id: "x".into() },
        SpeakersOp::Forget { id: "x".into() },
    ] {
        assert!(!Request::Speakers { op }.is_stream());
    }
    assert!(REQUEST_CMDS.contains(&"speakers"));
    assert!(FEATURES.contains(&FEATURE_SPEAKERS));
    assert_eq!(FEATURE_SPEAKERS, "speakers");
}

#[test]
fn the_events_and_answers_round_trip_and_an_old_reader_skips_them() {
    let event = Event::Speakers(JobEvent {
        job: 2,
        state: JobState::Progress {
            payload: serde_json::to_value(SpeakersProgress {
                id: "mit-normal".into(),
                received: 10,
                total: 100,
            })
            .unwrap(),
        },
    });
    let text = serde_json::to_string(&event).unwrap();
    assert!(text.contains(r#""event":"speakers""#), "{text}");
    assert_eq!(serde_json::from_str::<Event>(&text).unwrap(), event);

    let answer = Response::SpeakersCatalogue(Box::new(SpeakersCatalogue {
        sets: vec![set("a")],
        fetching: true,
        note: None,
        max_sets: 32,
    }));
    let text = serde_json::to_string(&answer).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), answer);
    let dir = Response::SpeakersImportDir {
        path: "/state/svolder/speakers/.import".into(),
    };
    let text = serde_json::to_string(&dir).unwrap();
    assert_eq!(
        text,
        r#"{"result":"speakers_import_dir","path":"/state/svolder/speakers/.import"}"#
    );
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), dir);
    // A set with nothing optional leaves those keys out.
    let bare = SpeakersSet {
        damaged: None,
        angles: None,
        ..set("b")
    };
    let text = serde_json::to_string(&bare).unwrap();
    assert!(
        !text.contains("damaged") && !text.contains("angles"),
        "{text}"
    );
}

#[test]
fn the_largest_catalogue_fits_a_line() {
    let sets: Vec<SpeakersSet> = (0..64).map(|n| set(&format!("set-{n}"))).collect();
    let answer = Response::SpeakersCatalogue(Box::new(SpeakersCatalogue {
        sets,
        fetching: true,
        note: None,
        max_sets: 32,
    }));
    let line = serde_json::to_string(&answer).unwrap();
    assert!(line.len() < MAX_LINE / 4, "{} bytes", line.len());
}
