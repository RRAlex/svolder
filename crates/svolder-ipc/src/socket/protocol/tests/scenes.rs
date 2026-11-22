// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `scene` request family on the wire: the ops, the answer, the view an event carries, the
//! feature, and what an older or a newer peer reads of it.

use super::*;

fn view() -> ScenesView {
    ScenesView {
        scenes: vec![
            SceneRow {
                name: "Late night".into(),
                parts: vec![
                    PartRow {
                        who: "computer".into(),
                        present: true,
                    },
                    PartRow {
                        who: "Kara".into(),
                        present: false,
                    },
                ],
            },
            SceneRow {
                name: "Desk".into(),
                parts: Vec::new(),
            },
        ],
        on: Some("Desk".into()),
        recalled: None,
        known: true,
    }
}

/// Each op is a `scene` request with its `op` word, and `force` and `confirm` may be left out.
#[test]
fn each_op_is_a_scene_request_with_its_word() {
    for (request, json) in [
        (
            Request::Scene {
                op: SceneOp::Save {
                    name: "Desk".into(),
                    force: true,
                },
            },
            r#"{"cmd":"scene","op":{"op":"save","name":"Desk","force":true}}"#,
        ),
        (
            Request::Scene {
                op: SceneOp::Recall {
                    name: "Desk".into(),
                    confirm: false,
                },
            },
            r#"{"cmd":"scene","op":{"op":"recall","name":"Desk","confirm":false}}"#,
        ),
        (
            Request::Scene {
                op: SceneOp::Rename {
                    from: "Desk".into(),
                    to: "Den".into(),
                },
            },
            r#"{"cmd":"scene","op":{"op":"rename","from":"Desk","to":"Den"}}"#,
        ),
        (
            Request::Scene {
                op: SceneOp::Delete {
                    name: "Desk".into(),
                },
            },
            r#"{"cmd":"scene","op":{"op":"delete","name":"Desk"}}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&request).unwrap(), json);
        assert_eq!(serde_json::from_str::<Request>(json).unwrap(), request);
        assert_eq!(request.cmd(), "scene");
        assert!(!request.is_stream());
    }
    let bare: Request =
        serde_json::from_str(r#"{"cmd":"scene","op":{"op":"recall","name":"x"}}"#).unwrap();
    assert_eq!(
        bare,
        Request::Scene {
            op: SceneOp::Recall {
                name: "x".into(),
                confirm: false
            }
        }
    );
}

/// A `scene` request parses only with an op the family has; every other kind still parses.
#[test]
fn a_scene_request_needs_its_op() {
    for bad in [
        r#"{"cmd":"scene"}"#,
        r#"{"cmd":"scene","op":{"op":"melt","name":"x"}}"#,
        r#"{"cmd":"scene","op":{"op":"save"}}"#,
        r#"{"cmd":"scene","op":{"op":"rename","from":"a"}}"#,
    ] {
        assert!(serde_json::from_str::<Request>(bad).is_err(), "{bad}");
    }
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"cmd":"devices"}"#).unwrap(),
        Request::Devices
    );
    assert_eq!(
        serde_json::from_str::<Request>(
            r#"{"cmd":"preset_write","op":{"op":"delete","name":"x"}}"#
        )
        .unwrap(),
        Request::PresetWrite {
            op: PresetOp::Delete { name: "x".into() }
        }
    );
}

/// The kind is in the list the server tells an unknown kind by, after the one before it.
#[test]
fn the_request_kinds_list_names_scene_last() {
    assert_eq!(REQUEST_CMDS.last(), Some(&"scene"));
    assert!(REQUEST_CMDS.contains(&"host_cover"));
}

/// The answer to a recall lists each part; the answer to any other op carries no list at all.
#[test]
fn the_answer_lists_each_part_of_a_recall() {
    let recalled = Response::Scene {
        name: "Late night".into(),
        outcome: vec![
            PartOutcome {
                who: "computer".into(),
                status: PartStatus::Done,
                notes: Vec::new(),
            },
            PartOutcome {
                who: "Kara".into(),
                status: PartStatus::Left,
                notes: vec!["Kara is not connected: its part was left as it is".into()],
            },
        ],
    };
    let json = serde_json::to_string(&recalled).unwrap();
    assert_eq!(
        json,
        r#"{"result":"scene","name":"Late night","outcome":[{"who":"computer","status":"done"},{"who":"Kara","status":"left","notes":["Kara is not connected: its part was left as it is"]}]}"#
    );
    assert_eq!(serde_json::from_str::<Response>(&json).unwrap(), recalled);
    let saved = Response::Scene {
        name: "Desk".into(),
        outcome: Vec::new(),
    };
    assert_eq!(
        serde_json::to_string(&saved).unwrap(),
        r#"{"result":"scene","name":"Desk"}"#
    );
    assert_eq!(
        serde_json::from_str::<Response>(r#"{"result":"scene","name":"Desk"}"#).unwrap(),
        saved
    );
}

/// A status a later daemon gives reads as unknown, and does not end the answer.
#[test]
fn a_status_this_build_does_not_know_reads_unknown() {
    let line = r#"{"who":"x","status":"queued"}"#;
    assert_eq!(
        serde_json::from_str::<PartOutcome>(line).unwrap().status,
        PartStatus::Unknown
    );
}

/// The view is an event of its own kind, flat: its fields sit beside the `event` word.
#[test]
fn the_view_is_an_event_with_its_fields_beside_the_word() {
    let event = Event::Scenes(view());
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.starts_with(r#"{"event":"scenes","scenes":[{"name":"Late night","parts":["#));
    assert!(json.contains(r#""on":"Desk""#));
    assert!(!json.contains("recalled"), "absent when none was recalled");
    assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), event);
    let reply = Reply::Event {
        id: 4,
        event: Event::Scenes(view()),
    };
    let line = serde_json::to_string(&reply).unwrap();
    assert_eq!(serde_json::from_str::<Reply>(&line).unwrap(), reply);
}

/// An older peer, which has no `scenes` event, reads the line as an event of unknown kind, and a
/// newer peer's event of a kind this build does not have still does.
#[test]
fn an_older_peer_reads_the_event_as_unknown_and_so_does_this_one_a_newer() {
    let line = serde_json::to_string(&Event::Scenes(view())).unwrap();
    // What a build without the variant sees is the same line under a tag it has no arm for: this
    // build's own reading of a tag it has no arm for is the same path.
    let older = line.replace(r#""event":"scenes""#, r#""event":"a_kind_nobody_has""#);
    assert_eq!(
        serde_json::from_str::<Event>(&older).unwrap(),
        Event::Unknown
    );
    assert_eq!(
        serde_json::from_str::<Event>(r#"{"event":"scenes_v2","x":1}"#).unwrap(),
        Event::Unknown
    );
}

/// A view from a peer that left out the optional words reads, and `known` is the one field
/// every view carries.
#[test]
fn a_view_without_its_optional_words_reads() {
    let got: ScenesView = serde_json::from_str(r#"{"scenes":[],"known":false}"#).unwrap();
    assert_eq!(
        got,
        ScenesView {
            scenes: Vec::new(),
            on: None,
            recalled: None,
            known: false
        }
    );
}

/// The feature is the word `scenes` and the daemon lists it.
#[test]
fn the_daemon_lists_the_scenes_feature() {
    assert_eq!(FEATURE_SCENES, "scenes");
    assert!(FEATURES.contains(&FEATURE_SCENES));
}
