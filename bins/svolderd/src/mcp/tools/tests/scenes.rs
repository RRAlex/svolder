// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scene tools: `list_scenes`, `load_scene`, `save_scene`, `rename_scene` and `delete_scene`.

use svolder_ipc::config::Scene;
use svolder_ipc::{PartOutcome, PartRow, PartStatus, SceneOp, SceneRow, ScenesView};
use svolder_profiles::scene::DevicePart;

use super::presets::{bands, db_of};
use super::*;

const UNIT: &str = "0102030405060708";

/// A scene whose one device part sets `bands` on the scripted device.
fn loki_scene(name: &str, bands: &[(&str, i64)]) -> Scene {
    Scene {
        name: name.to_string(),
        computer: None,
        devices: vec![DevicePart {
            udid: UNIT.to_string(),
            name: "amp".to_string(),
            tone: None,
            raw_bands: Some(bands.iter().map(|(k, v)| (k.to_string(), *v)).collect()),
            controls: Vec::new(),
        }],
    }
}

/// A scene with a device part holding nothing that a guard judges.
fn plain_scene(name: &str) -> Scene {
    Scene {
        devices: vec![DevicePart {
            raw_bands: None,
            ..loki_scene(name, &[]).devices.remove(0)
        }],
        ..loki_scene(name, &[])
    }
}

fn done(who: &str) -> PartOutcome {
    PartOutcome {
        who: who.to_string(),
        status: PartStatus::Done,
        notes: Vec::new(),
    }
}

/// A fleet that answers every scene op as a recall that set every part.
fn with_scenes(fleet: &Arc<Scripted>, scenes: Vec<Scene>) {
    *fleet.scene_config.lock().unwrap() = scenes;
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|op| match op {
        SceneOp::Save { name, .. } | SceneOp::Recall { name, .. } | SceneOp::Delete { name } => {
            Response::Scene {
                name: name.clone(),
                outcome: vec![done("amp")],
            }
        }
        SceneOp::Rename { to, .. } => Response::Scene {
            name: to.clone(),
            outcome: Vec::new(),
        },
    }));
}

fn ops(fleet: &Scripted) -> Vec<SceneOp> {
    fleet.scene_ops.lock().unwrap().clone()
}

fn recall(name: &str, confirm: bool) -> CallToolRequestParams {
    call("load_scene", json!({"name": name, "confirm": confirm}))
}

/// The retry of `request` that answers the question whose state is `state` with `action`.
fn answered(
    mut request: CallToolRequestParams,
    state: &str,
    action: Value,
) -> CallToolRequestParams {
    request.request_state = Some(state.to_string());
    request.input_responses = Some([(ASK.to_string(), action)].into_iter().collect());
    request
}

fn yes() -> Value {
    json!({"action": "accept", "content": {"confirm": true}})
}

fn state_of(response: CallToolResponse) -> String {
    let CallToolResponse::InputRequired(asked) = response else {
        panic!("not a question: {response:?}")
    };
    asked.request_state.clone().expect("a state")
}

#[tokio::test]
async fn list_scenes_gives_the_daemons_view() {
    let fleet = scripted("lokimax", &[]);
    fleet.scene_view.send_replace(ScenesView {
        scenes: vec![SceneRow {
            name: "Late night".into(),
            parts: vec![PartRow {
                who: "amp".into(),
                present: true,
            }],
        }],
        on: Some("Late night".into()),
        recalled: Some("Late night".into()),
        known: true,
    });
    let answer = run(&fleet, None, call("list_scenes", json!({})), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(
        structured(&answer),
        json!({
            "scenes": [{"name": "Late night", "parts": [{"who": "amp", "present": true}]}],
            "on": "Late night",
            "recalled": "Late night",
            "known": true
        })
    );
    assert!(ops(&fleet).is_empty(), "a read sends no scene request");
}

#[tokio::test]
async fn a_plain_recall_asks_nothing_and_carries_each_part_s_outcome() {
    let fleet = scripted("lokimax", &bands([15; 6]));
    with_scenes(&fleet, vec![plain_scene("Desk")]);
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|_| Response::Scene {
        name: "Desk".into(),
        outcome: vec![
            done("computer"),
            PartOutcome {
                who: "Kara".into(),
                status: PartStatus::Left,
                notes: vec!["Kara is not connected: its part was left as it is".into()],
            },
        ],
    }));
    for can_ask in [false, true] {
        let answer = run(&fleet, None, recall("desk", false), can_ask).await;
        assert!(went(&answer), "{answer:?}");
        assert_eq!(
            structured(&answer),
            json!({
                "name": "Desk",
                "outcome": [
                    {"who": "computer", "status": "done"},
                    {"who": "Kara", "status": "left",
                     "notes": ["Kara is not connected: its part was left as it is"]}
                ]
            })
        );
    }
    assert_eq!(
        ops(&fleet),
        vec![
            SceneOp::Recall {
                name: "desk".into(),
                confirm: false
            };
            2
        ]
    );
}

#[tokio::test]
async fn a_recall_the_daemon_asks_about_asks_once_and_goes_on_a_yes() {
    let fleet = scripted("lokimax", &bands([15; 6]));
    with_scenes(&fleet, vec![plain_scene("Desk")]);
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|op| {
        match op {
        SceneOp::Recall { confirm: false, .. } => Response::Ask {
            message: "Desk leaves Warm on the Desk DAC and on the computer: it plays twice. Recall it anyway?"
                .into(),
            name: "Warm".into(),
            devices: vec!["amp".into()],
        },
        _ => Response::Scene {
            name: "Desk".into(),
            outcome: vec![done("amp")],
        },
    }
    }));
    // A client that cannot ask is told the daemon's words and to repeat with confirm.
    let answer = run(&fleet, None, recall("Desk", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(words.contains("it plays twice"), "{words}");
    assert!(
        words.contains("repeat this call with \\\"confirm_both_sides\\\": true"),
        "{words}"
    );
    // A plain `confirm` is no answer to it.
    let answer = run(&fleet, None, recall("Desk", true), false).await;
    assert!(is_error(&answer), "{answer:?}");
    // The yes of a client that cannot ask resends the request confirmed.
    let both = call(
        "load_scene",
        json!({"name": "Desk", "confirm_both_sides": true}),
    );
    let answer = run(&fleet, None, both, false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(
        ops(&fleet),
        [false, false, false, true].map(|confirm| SceneOp::Recall {
            name: "Desk".into(),
            confirm
        })
    );
    // A client that can ask is sent the question, whatever the model confirms; a no changes
    // nothing, the yes of the question asked recalls the scene as saved.
    fleet.scene_ops.lock().unwrap().clear();
    let state = state_of(run(&fleet, None, recall("Desk", true), true).await);
    assert_eq!(state, "scene|Desk|both|");
    for no in [json!({"action": "decline"}), json!({"action": "cancel"})] {
        let answer = run(
            &fleet,
            None,
            answered(recall("Desk", false), &state, no),
            true,
        )
        .await;
        assert!(is_error(&answer) && error_words(&answer).contains("declined"));
    }
    assert!(ops(&fleet).iter().all(|op| *op
        == SceneOp::Recall {
            name: "Desk".into(),
            confirm: false
        }));
    let answer = run(
        &fleet,
        None,
        answered(recall("Desk", false), &state, yes()),
        true,
    )
    .await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(
        ops(&fleet).last(),
        Some(&SceneOp::Recall {
            name: "Desk".into(),
            confirm: true
        })
    );
}

#[tokio::test]
async fn a_band_raised_over_6_db_asks_before_any_request_goes() {
    // Band 1 reads 0.0 dB; the scene sets it to 10.4 dB.
    let fleet = scripted("lokimax", &bands([15; 6]));
    with_scenes(
        &fleet,
        vec![loki_scene("Movie", &[("band1", 28), ("band2", 15)])],
    );
    let answer = run(&fleet, None, recall("Movie", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains(
            "raising 20 Hz on amp from 0.0 dB to 10.4 dB is more than 6 dB above its lowest \
             level in the last minute (0.0 dB)"
        ),
        "{words}"
    );
    assert!(
        !words.contains("120 Hz"),
        "only the band that asks is named: {words}"
    );
    assert!(words.contains("repeat this call with"), "{words}");
    assert!(ops(&fleet).is_empty());
    // A client that can ask is sent the question.
    let state = state_of(run(&fleet, None, recall("Movie", true), true).await);
    assert_eq!(state, "scene|Movie|bands|amp.band1=10.4@0.0");
    assert!(ops(&fleet).is_empty());
    // The yes of the question asked recalls, and the band's window starts over at that level.
    let tools = server(&fleet, None);
    let answer = tools
        .call(
            answered(recall("Movie", false), &state, yes()),
            Caller { can_ask: true },
        )
        .await
        .expect("an answer");
    assert!(went(&answer), "{answer:?}");
    assert_eq!(
        ops(&fleet),
        [SceneOp::Recall {
            name: "Movie".into(),
            confirm: false
        }]
    );
    assert_eq!(
        tools.floors.floor("amp", "band1"),
        Some(db_of(&fleet, "band1", 28))
    );
}

#[tokio::test]
async fn a_band_raised_6_db_or_less_goes_through_unasked() {
    let mut raw = [15; 6];
    raw[0] = 23;
    let fleet = scripted("lokimax", &bands(raw));
    with_scenes(&fleet, vec![loki_scene("Movie", &[("band1", 28)])]);
    let answer = run(&fleet, None, recall("Movie", false), true).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(ops(&fleet).len(), 1);
}

#[tokio::test]
async fn a_band_question_and_the_daemons_question_are_each_asked_once_in_turn() {
    let fleet = scripted("lokimax", &bands([15; 6]));
    with_scenes(&fleet, vec![loki_scene("Movie", &[("band1", 28)])]);
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|op| match op {
        SceneOp::Recall { confirm: false, .. } => Response::Ask {
            message: "Movie plays Warm twice. Recall it anyway?".into(),
            name: "Warm".into(),
            devices: vec!["amp".into()],
        },
        _ => Response::Scene {
            name: "Movie".into(),
            outcome: vec![done("amp")],
        },
    }));
    let bands_state = state_of(run(&fleet, None, recall("Movie", false), true).await);
    assert_eq!(bands_state, "scene|Movie|bands|amp.band1=10.4@0.0");
    // The yes to the bands reaches the daemon, whose question is then put.
    let both_state = state_of(
        run(
            &fleet,
            None,
            answered(recall("Movie", false), &bands_state, yes()),
            true,
        )
        .await,
    );
    assert_eq!(both_state, "scene|Movie|both|amp.band1=10.4@0.0");
    assert_eq!(
        ops(&fleet),
        [SceneOp::Recall {
            name: "Movie".into(),
            confirm: false
        }]
    );
    // The yes to that one recalls, without asking about the bands again.
    let answer = run(
        &fleet,
        None,
        answered(recall("Movie", false), &both_state, yes()),
        true,
    )
    .await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(
        ops(&fleet).last(),
        Some(&SceneOp::Recall {
            name: "Movie".into(),
            confirm: true
        })
    );
}

#[tokio::test]
async fn a_refusal_carries_the_daemons_words() {
    let fleet = scripted("lokimax", &[]);
    with_scenes(&fleet, Vec::new());
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|_| {
        Response::error(CODE_LINK, "no scene named \"Nope\"".to_string())
    }));
    let answer = run(&fleet, None, recall("Nope", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(
        error_words(&answer).contains("no scene named \\\"Nope\\\""),
        "{}",
        error_words(&answer)
    );
}

#[tokio::test]
async fn saving_over_a_taken_name_is_refused_unless_replace_and_replace_asks_once() {
    let fleet = scripted("lokimax", &[]);
    with_scenes(&fleet, vec![plain_scene("Desk")]);
    let save = |replace: bool, confirm: bool| {
        call(
            "save_scene",
            json!({"name": "desk", "replace": replace, "confirm": confirm}),
        )
    };
    // Without replace the name is refused in the daemon's words, with no question.
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|_| {
        Response::error(
            CODE_LINK,
            "a scene named \"Desk\" already exists".to_string(),
        )
        .with_reason(svolder_ipc::Reason::Taken)
    }));
    for can_ask in [false, true] {
        let answer = run(
            &fleet,
            None,
            call("save_scene", json!({"name": "desk"})),
            can_ask,
        )
        .await;
        assert!(is_error(&answer), "{answer:?}");
        let words = error_words(&answer);
        assert!(words.contains("already exists"), "{words}");
        assert!(words.contains("replace"), "{words}");
    }
    assert_eq!(
        ops(&fleet),
        vec![
            SceneOp::Save {
                name: "desk".into(),
                force: false
            };
            2
        ]
    );
    fleet.scene_ops.lock().unwrap().clear();
    with_scenes(&fleet, vec![plain_scene("Desk")]);
    // Replace asks first, once, and sends nothing until it is answered.
    let answer = run(&fleet, None, save(true, false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(words.contains("replaces what Desk holds"), "{words}");
    assert!(
        words.contains("no volume, gain, mute or power is saved"),
        "{words}"
    );
    assert!(ops(&fleet).is_empty());
    let state = state_of(run(&fleet, None, save(true, true), true).await);
    assert_eq!(state, "scene_save|Desk");
    assert!(ops(&fleet).is_empty());
    let answer = run(
        &fleet,
        None,
        answered(save(true, false), &state, yes()),
        true,
    )
    .await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(structured(&answer)["name"], json!("desk"));
    assert_eq!(
        ops(&fleet),
        [SceneOp::Save {
            name: "desk".into(),
            force: true
        }]
    );
    // The yes of a client that cannot ask goes through too.
    let answer = run(&fleet, None, save(true, true), false).await;
    assert!(went(&answer), "{answer:?}");
}

#[tokio::test]
async fn saving_under_a_new_name_asks_nothing_even_with_replace() {
    let fleet = scripted("lokimax", &[]);
    with_scenes(&fleet, Vec::new());
    for replace in [false, true] {
        let answer = run(
            &fleet,
            None,
            call(
                "save_scene",
                json!({"name": "Late night", "replace": replace}),
            ),
            true,
        )
        .await;
        assert!(went(&answer), "{answer:?}");
    }
    assert_eq!(
        ops(&fleet),
        vec![
            SceneOp::Save {
                name: "Late night".into(),
                force: false
            };
            2
        ]
    );
}

#[tokio::test]
async fn a_save_lists_the_parts_it_left_out() {
    let fleet = scripted("lokimax", &[]);
    with_scenes(&fleet, Vec::new());
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|_| Response::Scene {
        name: "Desk".into(),
        outcome: vec![PartOutcome {
            who: "Kara".into(),
            status: PartStatus::Left,
            notes: vec!["Kara is not connected: it is not in the scene".into()],
        }],
    }));
    let answer = run(
        &fleet,
        None,
        call("save_scene", json!({"name": "Desk"})),
        false,
    )
    .await;
    assert_eq!(
        structured(&answer)["outcome"],
        json!([{"who": "Kara", "status": "left",
                "notes": ["Kara is not connected: it is not in the scene"]}])
    );
}

#[tokio::test]
async fn a_rename_asks_nothing() {
    let fleet = scripted("lokimax", &[]);
    with_scenes(&fleet, vec![plain_scene("Desk")]);
    for can_ask in [false, true] {
        let answer = run(
            &fleet,
            None,
            call("rename_scene", json!({"from": "Desk", "to": "Study"})),
            can_ask,
        )
        .await;
        assert!(went(&answer), "{answer:?}");
        assert_eq!(structured(&answer), json!({"name": "Study"}));
    }
    assert_eq!(
        ops(&fleet),
        vec![
            SceneOp::Rename {
                from: "Desk".into(),
                to: "Study".into()
            };
            2
        ]
    );
}

#[tokio::test]
async fn a_delete_asks_once() {
    let fleet = scripted("lokimax", &[]);
    with_scenes(&fleet, vec![plain_scene("Desk")]);
    let delete = |confirm: bool| call("delete_scene", json!({"name": "desk", "confirm": confirm}));
    let answer = run(&fleet, None, delete(false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(
        error_words(&answer).contains("deleting the scene Desk"),
        "{}",
        error_words(&answer)
    );
    assert!(ops(&fleet).is_empty());
    let state = state_of(run(&fleet, None, delete(true), true).await);
    assert_eq!(state, "scene_delete|Desk");
    let no = run(
        &fleet,
        None,
        answered(delete(false), &state, json!({"action": "decline"})),
        true,
    )
    .await;
    assert!(is_error(&no) && error_words(&no).contains("declined"));
    assert!(ops(&fleet).is_empty());
    let answer = run(&fleet, None, answered(delete(false), &state, yes()), true).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(
        ops(&fleet),
        [SceneOp::Delete {
            name: "desk".into()
        }]
    );
    let answer = run(&fleet, None, delete(true), false).await;
    assert!(went(&answer), "{answer:?}");
}

#[tokio::test]
async fn deleting_a_scene_that_is_not_there_is_the_daemons_refusal_with_no_question() {
    let fleet = scripted("lokimax", &[]);
    with_scenes(&fleet, Vec::new());
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|_| {
        Response::error(CODE_LINK, "no scene named \"Gone\"".to_string())
    }));
    let answer = run(
        &fleet,
        None,
        call("delete_scene", json!({"name": "Gone"})),
        true,
    )
    .await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(error_words(&answer).contains("no scene named"));
    assert_eq!(ops(&fleet).len(), 1);
}

#[tokio::test]
async fn with_writes_off_only_list_scenes_is_offered() {
    let fleet = scripted("lokimax", &[]);
    let read = listed(&fleet, false);
    assert!(read.contains(&"list_scenes".to_string()), "{read:?}");
    for tool in ["load_scene", "save_scene", "rename_scene", "delete_scene"] {
        assert!(
            !read.contains(&tool.to_string()),
            "{tool} is listed: {read:?}"
        );
        assert!(listed(&fleet, true).contains(&tool.to_string()));
        let answer = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, false, None)
            .call(call(tool, json!({"name": "Desk"})), Caller::default())
            .await
            .expect("an answer");
        assert!(is_error(&answer), "{answer:?}");
        assert!(error_words(&answer).contains("[mcp] writes"), "{tool}");
    }
    assert!(ops(&fleet).is_empty());
}

/// A scene never writes a volume, gain, mute or power: the tools offer no way to ask for one,
/// and every call is one scene op and nothing else.
#[tokio::test]
async fn the_scene_tools_send_scene_ops_only() {
    let fleet = scripted("lokimax", &bands([15; 6]));
    with_scenes(&fleet, vec![plain_scene("Desk")]);
    run(&fleet, None, recall("Desk", false), false).await;
    run(
        &fleet,
        None,
        call("save_scene", json!({"name": "Late night"})),
        false,
    )
    .await;
    assert!(fleet.sets.lock().unwrap().is_empty());
    assert!(fleet.glides.lock().unwrap().is_empty());
    assert!(fleet.submitted.lock().unwrap().is_empty());
    let tools = server(&fleet, None).list();
    for name in [
        "load_scene",
        "save_scene",
        "rename_scene",
        "delete_scene",
        "list_scenes",
    ] {
        let tool = tools.iter().find(|t| t.name == name).expect("listed");
        let properties = tool.input_schema["properties"]
            .as_object()
            .expect("properties");
        assert!(
            properties
                .keys()
                .all(|k| !["volume", "gain", "mute", "power"].contains(&k.as_str())),
            "{name}: {properties:?}"
        );
        assert!(tool.description.as_deref().is_some_and(|d| !d.is_empty()));
    }
}

/// A client that cannot ask answers each question with its own flag: `confirm` for the band, and
/// `confirm_both_sides` for the daemon's question, which is put after the band was agreed to.
#[tokio::test]
async fn a_client_that_cannot_ask_answers_each_question_with_its_own_flag() {
    let fleet = scripted("lokimax", &bands([15; 6]));
    with_scenes(&fleet, vec![loki_scene("Movie", &[("band1", 28)])]);
    *fleet.scene_reply.lock().unwrap() = Some(Box::new(|op| match op {
        SceneOp::Recall { confirm: false, .. } => Response::Ask {
            message: "Movie plays Warm twice. Recall it anyway?".into(),
            name: "Warm".into(),
            devices: vec!["amp".into()],
        },
        _ => Response::Scene {
            name: "Movie".into(),
            outcome: vec![done("amp")],
        },
    }));
    let load = |confirm: bool, both: bool| {
        call(
            "load_scene",
            json!({"name": "Movie", "confirm": confirm, "confirm_both_sides": both}),
        )
    };
    // 1: the band question only; nothing reaches the daemon.
    let first = run(&fleet, None, load(false, false), false).await;
    assert!(is_error(&first) && error_words(&first).contains("10.4 dB"));
    assert!(ops(&fleet).is_empty());
    // 2: the band agreed to; the daemon's words come back and nothing is recalled.
    let second = run(&fleet, None, load(true, false), false).await;
    assert!(is_error(&second), "{second:?}");
    let words = error_words(&second);
    assert!(words.contains("Movie plays Warm twice"), "{words}");
    assert!(words.contains("confirm_both_sides"), "{words}");
    assert_eq!(
        ops(&fleet),
        [SceneOp::Recall {
            name: "Movie".into(),
            confirm: false
        }]
    );
    // The flag for the daemon's question does not answer the band's.
    let alone = run(&fleet, None, load(false, true), false).await;
    assert!(is_error(&alone) && error_words(&alone).contains("10.4 dB"));
    assert_eq!(ops(&fleet).len(), 1);
    // 3: both answered; the scene is recalled as saved.
    let third = run(&fleet, None, load(true, true), false).await;
    assert!(went(&third), "{third:?}");
    assert_eq!(
        ops(&fleet).last(),
        Some(&SceneOp::Recall {
            name: "Movie".into(),
            confirm: true
        })
    );
}

/// A state echoed with no answer under it answers nothing.
#[tokio::test]
async fn an_echoed_state_with_no_answer_does_not_skip_the_band_question() {
    let fleet = scripted("lokimax", &bands([15; 6]));
    with_scenes(&fleet, vec![loki_scene("Movie", &[("band1", 28)])]);
    let mut request = recall("Movie", false);
    request.request_state = Some("scene|Movie|both|amp.band1=10.4@0.0".to_string());
    let asked = run(&fleet, None, request, true).await;
    assert_eq!(state_of(asked), "scene|Movie|bands|amp.band1=10.4@0.0");
    assert!(ops(&fleet).is_empty());
}

/// While the daemon is going away the tool says so, not that it keeps no scenes.
#[tokio::test]
async fn list_scenes_while_the_daemon_is_stopping_says_so() {
    let fleet = scripted("lokimax", &[]);
    *fleet.stopping.lock().unwrap() = true;
    let answer = run(&fleet, None, call("list_scenes", json!({})), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(
        error_words(&answer).contains("the daemon is stopping"),
        "{}",
        error_words(&answer)
    );
}
