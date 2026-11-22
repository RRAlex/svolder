// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder scene` over a stub daemon that shows the scenes and answers the requests a case
//! scripts: what each verb sends, says and exits with, and that nothing is sent where the daemon
//! has no scenes or a question is not answered.

use svolder_ipc::{CODE_USAGE, PartRow, Reason};

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon_at};

async fn stub() -> StubServer {
    let daemon = stub_daemon_at(crate::test_support::short_socket("svsc")).await;
    Client::connect(&daemon.socket)
        .await
        .expect("the stub daemon answers on its socket");
    daemon
}

fn part(who: &str, present: bool) -> PartRow {
    PartRow {
        who: who.into(),
        present,
    }
}

fn row(name: &str, parts: &[(&str, bool)]) -> SceneRow {
    SceneRow {
        name: name.into(),
        parts: parts.iter().map(|(who, here)| part(who, *here)).collect(),
    }
}

/// Two scenes, the first on, one part of the second not connected.
fn two_scenes() -> ScenesView {
    ScenesView {
        scenes: vec![
            row("Late night", &[("computer", true), ("Desk", true)]),
            row("Movie", &[("computer", true), ("Kara", false)]),
        ],
        on: Some("Late night".into()),
        recalled: None,
        known: true,
    }
}

fn outcome(who: &str, status: PartStatus, notes: &[&str]) -> PartOutcome {
    PartOutcome {
        who: who.into(),
        status,
        notes: notes.iter().map(|n| (*n).to_string()).collect(),
    }
}

/// Run `command` against `daemon` with no terminal, so nothing can be asked.
async fn run_plain(daemon: &StubServer, command: SceneCommand) -> Result<Answer, Failure> {
    run_json(daemon, false, command).await
}

async fn run_json(
    daemon: &StubServer,
    json: bool,
    command: SceneCommand,
) -> Result<Answer, Failure> {
    let mut never = |_: &str| -> std::io::Result<bool> { panic!("nothing is asked") };
    let mut asking = Asking {
        terminal: false,
        ask: &mut never,
    };
    answer(
        &daemon.socket,
        &Config::default(),
        json,
        Some(command),
        &mut asking,
    )
    .await
}

/// Run `command` at a terminal whose user answers `says`; the questions come back with it.
async fn run_asked(
    daemon: &StubServer,
    command: SceneCommand,
    says: bool,
) -> (Result<Answer, Failure>, Vec<String>) {
    let mut questions = Vec::new();
    let mut ask = |question: &str| -> std::io::Result<bool> {
        questions.push(question.to_string());
        Ok(says)
    };
    let mut asking = Asking {
        terminal: true,
        ask: &mut ask,
    };
    let done = answer(
        &daemon.socket,
        &Config::default(),
        false,
        Some(command),
        &mut asking,
    )
    .await;
    (done, questions)
}

fn load_of(name: &str, yes: bool) -> SceneCommand {
    SceneCommand::Load {
        name: name.into(),
        yes,
    }
}

/// A config with two scenes: "Desk" holds the computer and a device the config names "Desk DAC"
/// (saved as "Bifrost 3"), "Movie" holds a device no config device has (saved as "Kara").
fn config_with_scenes() -> Config {
    let device = |name: &str, udid: &str| {
        format!("[[scene.device]]\nudid = \"{udid}\"\nname = \"{name}\"\n")
    };
    let text = format!(
        "[[device]]\nname = \"Desk DAC\"\nudid = \"0a0b0c0d0e0f1011\"\n\
         [[scene]]\nname = \"Desk\"\n[scene.computer]\neq_on = false\nname = \"Flat\"\npreamp = 0.0\nfilter = []\n{}\
         [[scene]]\nname = \"Movie\"\n{}",
        device("Bifrost 3", "0A0B0C0D0E0F1011"),
        device("Kara", "1112131415161718"),
    );
    Config::parse(&text).expect("a config with scenes")
}

async fn no_daemon(command: Option<SceneCommand>, json: bool) -> Result<Answer, Failure> {
    let dir = crate::test_support::temp_dir("scene-none");
    let socket = dir.0.join("no-such.sock");
    let mut never = |_: &str| -> std::io::Result<bool> { panic!("nothing is asked") };
    let mut asking = Asking {
        terminal: false,
        ask: &mut never,
    };
    answer(&socket, &config_with_scenes(), json, command, &mut asking).await
}

#[tokio::test]
async fn with_no_daemon_list_reads_the_config_with_the_configured_names_and_no_lamp() {
    let said = no_daemon(None, false).await.expect("listed");
    assert_eq!(said.out, ["Desk  computer · Desk DAC", "Movie  Kara"]);
    assert_eq!(said.err.len(), 1, "{:?}", said.err);
    assert!(said.err[0].contains("no daemon running"), "{:?}", said.err);
    assert_eq!(said.code, 0);
}

#[tokio::test]
async fn with_no_daemon_show_reads_the_config_and_says_nothing_of_on_or_connection() {
    let said = no_daemon(
        Some(SceneCommand::Show {
            name: "desk".into(),
        }),
        false,
    )
    .await
    .expect("shown");
    assert_eq!(said.out, ["Desk", "  computer", "  Desk DAC"]);
    assert_eq!(said.err.len(), 1);
    let failure = no_daemon(
        Some(SceneCommand::Show {
            name: "Nope".into(),
        }),
        false,
    )
    .await
    .expect_err("no such scene");
    assert_eq!(failure.what, "no scene named \"Nope\"");
}

#[tokio::test]
async fn with_no_daemon_json_drops_on_recalled_and_present_and_says_known_false() {
    let said = no_daemon(None, true).await.expect("listed");
    let value: serde_json::Value = serde_json::from_str(&said.out[0]).expect("json");
    assert_eq!(
        value,
        serde_json::json!({"scenes": [
            {"name": "Desk", "parts": [{"who": "computer"}, {"who": "Desk DAC"}]},
            {"name": "Movie", "parts": [{"who": "Kara"}]}
        ], "known": false})
    );
    let said = no_daemon(
        Some(SceneCommand::Show {
            name: "Movie".into(),
        }),
        true,
    )
    .await
    .expect("shown");
    let value: serde_json::Value = serde_json::from_str(&said.out[0]).expect("json");
    assert_eq!(
        value,
        serde_json::json!({"name": "Movie", "parts": [{"who": "Kara"}]})
    );
}

#[tokio::test]
async fn with_no_daemon_every_other_verb_says_scenes_are_kept_by_the_daemon() {
    for command in [
        SceneCommand::Save {
            name: "Desk".into(),
            force: false,
        },
        load_of("Desk", true),
        SceneCommand::Rename {
            old: "Desk".into(),
            new: "Den".into(),
        },
        SceneCommand::Delete {
            name: "Desk".into(),
            yes: true,
        },
    ] {
        let failure = no_daemon(Some(command), false)
            .await
            .expect_err("no daemon");
        assert_eq!(failure.code, Kind::NoDaemon.code());
        assert!(
            failure.what.contains("scenes are kept by the daemon"),
            "{failure}"
        );
    }
}

#[tokio::test]
async fn a_daemon_without_scenes_says_so_with_its_version_and_is_sent_nothing() {
    let daemon = stub().await;
    daemon.scenes();
    daemon.lists_features(&[]);
    for command in [
        SceneCommand::List,
        SceneCommand::Save {
            name: "Desk".into(),
            force: false,
        },
        load_of("Desk", true),
        SceneCommand::Delete {
            name: "Desk".into(),
            yes: true,
        },
    ] {
        let failure = run_plain(&daemon, command).await.expect_err("no scenes");
        assert_eq!(failure.what, "this daemon (9.9.9) has no scenes");
        assert_eq!(failure.code, Kind::DaemonTooOld.code());
    }
    assert!(daemon.scenes().ops().is_empty(), "no request was sent");
}

#[tokio::test]
async fn list_marks_the_scene_that_is_on_and_the_parts_not_connected() {
    let daemon = stub().await;
    daemon.scenes().shows(two_scenes());
    let said = run_plain(&daemon, SceneCommand::List)
        .await
        .expect("listed");
    assert_eq!(
        said.out,
        [
            "* Late night  computer · Desk",
            "  Movie  computer · Kara (not connected)"
        ]
    );
    assert!(said.err.is_empty());
    assert_eq!(said.code, 0);
}

#[tokio::test]
async fn list_while_which_scene_is_on_is_unknown_lights_none_and_says_so() {
    let daemon = stub().await;
    daemon.scenes().shows(ScenesView {
        on: None,
        known: false,
        ..two_scenes()
    });
    let said = run_plain(&daemon, SceneCommand::List)
        .await
        .expect("listed");
    assert!(
        said.out.iter().all(|line| line.starts_with("  ")),
        "{:?}",
        said.out
    );
    assert_eq!(said.err.len(), 1, "{:?}", said.err);
    assert!(said.err[0].contains("not known yet"), "{:?}", said.err);
}

#[tokio::test]
async fn list_with_no_scene_says_how_to_save_one() {
    let daemon = stub().await;
    daemon.scenes().shows(ScenesView {
        scenes: Vec::new(),
        on: None,
        recalled: None,
        known: true,
    });
    let said = run_plain(&daemon, SceneCommand::List)
        .await
        .expect("listed");
    assert_eq!(
        said.out,
        ["No scenes yet. Save one with `svolder scene save NAME`."]
    );
}

#[tokio::test]
async fn list_json_is_the_daemon_s_view() {
    let daemon = stub().await;
    daemon.scenes().shows(two_scenes());
    let said = run_json(&daemon, true, SceneCommand::List)
        .await
        .expect("listed");
    assert_eq!(said.out.len(), 1);
    let back: ScenesView = serde_json::from_str(&said.out[0]).expect("a view");
    assert_eq!(back, two_scenes());
}

#[tokio::test]
async fn show_finds_a_scene_in_any_case_and_says_whether_it_is_on() {
    let daemon = stub().await;
    daemon.scenes().shows(two_scenes());
    let said = run_plain(
        &daemon,
        SceneCommand::Show {
            name: " MOVIE ".into(),
        },
    )
    .await
    .expect("shown");
    assert_eq!(said.out, ["Movie", "  computer", "  Kara (not connected)"]);
    let said = run_json(
        &daemon,
        true,
        SceneCommand::Show {
            name: "late night".into(),
        },
    )
    .await
    .expect("shown");
    let value: serde_json::Value = serde_json::from_str(&said.out[0]).expect("json");
    assert_eq!(value["name"], "Late night");
    assert_eq!(value["on"], true);
    assert_eq!(value["parts"][1]["who"], "Desk");
    assert_eq!(value["parts"][1]["present"], true);
}

#[tokio::test]
async fn show_while_which_scene_is_on_is_unknown_says_so_in_a_note_and_in_json() {
    let daemon = stub().await;
    daemon.scenes().shows(ScenesView {
        on: None,
        known: false,
        ..two_scenes()
    });
    let said = run_plain(
        &daemon,
        SceneCommand::Show {
            name: "Movie".into(),
        },
    )
    .await
    .expect("shown");
    assert_eq!(said.err.len(), 1, "{:?}", said.err);
    assert!(said.err[0].contains("not known yet"), "{:?}", said.err);
    let said = run_json(
        &daemon,
        true,
        SceneCommand::Show {
            name: "Movie".into(),
        },
    )
    .await
    .expect("shown");
    let value: serde_json::Value = serde_json::from_str(&said.out[0]).expect("json");
    assert_eq!(value["known"], false);
}

#[tokio::test]
async fn show_of_a_scene_there_is_not_refuses_with_the_name() {
    let daemon = stub().await;
    daemon.scenes().shows(two_scenes());
    let failure = run_plain(
        &daemon,
        SceneCommand::Show {
            name: "Desk".into(),
        },
    )
    .await
    .expect_err("no such scene");
    assert_eq!(failure.what, "no scene named \"Desk\"");
    assert_eq!(failure.code, CODE_USAGE);
}

#[tokio::test]
async fn save_sends_the_name_and_force_and_prints_each_part_left_out_as_a_note() {
    let daemon = stub().await;
    for _ in 0..2 {
        daemon.scenes().answers(Response::Scene {
            name: "Late night".into(),
            outcome: vec![outcome(
                "Kara",
                PartStatus::Left,
                &["Kara is not connected: it is not in the scene"],
            )],
        });
    }
    let said = run_plain(
        &daemon,
        SceneCommand::Save {
            name: "late night".into(),
            force: false,
        },
    )
    .await
    .expect("saved");
    assert_eq!(said.out, ["Saved \"Late night\"."]);
    assert_eq!(said.err, ["Kara is not connected: it is not in the scene"]);
    run_plain(
        &daemon,
        SceneCommand::Save {
            name: "Late night".into(),
            force: true,
        },
    )
    .await
    .expect("saved over");
    assert_eq!(
        daemon.scenes().ops(),
        [
            SceneOp::Save {
                name: "late night".into(),
                force: false
            },
            SceneOp::Save {
                name: "Late night".into(),
                force: true
            }
        ]
    );
}

#[tokio::test]
async fn a_refused_save_carries_the_daemon_s_words_and_code() {
    let daemon = stub().await;
    let words = "config: a scene named \"Desk\" is already configured";
    daemon
        .scenes()
        .answers(Response::error(CODE_USAGE, words).with_reason(Reason::Taken));
    let failure = run_plain(
        &daemon,
        SceneCommand::Save {
            name: "Desk".into(),
            force: false,
        },
    )
    .await
    .expect_err("taken");
    assert_eq!(failure.what, words);
    assert_eq!(failure.code, CODE_USAGE);
    assert_eq!(failure.reason, Some(Reason::Taken));
}

#[tokio::test]
async fn load_prints_each_part_and_exits_zero_when_every_part_is_done_or_left() {
    let daemon = stub().await;
    daemon.scenes().answers(Response::Scene {
        name: "Movie".into(),
        outcome: vec![
            outcome("computer", PartStatus::Done, &[]),
            outcome(
                "Kara",
                PartStatus::Left,
                &["Kara is not connected: its part was left as it is"],
            ),
        ],
    });
    let said = run_plain(&daemon, load_of("movie", false))
        .await
        .expect("recalled");
    assert_eq!(
        said.out,
        [
            "Recalled \"Movie\".",
            "computer: done",
            "Kara: left",
            "  Kara is not connected: its part was left as it is"
        ]
    );
    assert_eq!(said.code, 0);
    assert_eq!(
        daemon.scenes().ops(),
        [SceneOp::Recall {
            name: "movie".into(),
            confirm: false
        }]
    );
}

#[tokio::test]
async fn load_exits_3_when_a_part_is_partial_or_failed() {
    for status in [PartStatus::Partial, PartStatus::Failed] {
        let daemon = stub().await;
        daemon.scenes().answers(Response::Scene {
            name: "Movie".into(),
            outcome: vec![
                outcome("computer", PartStatus::Done, &[]),
                outcome("Desk", status, &["input 2 was refused"]),
            ],
        });
        let said = run_plain(&daemon, load_of("Movie", false))
            .await
            .expect("answered");
        assert_eq!(said.code, 3, "{status:?}");
        assert!(said.out.contains(&"  input 2 was refused".to_string()));
        assert_eq!(said.err, ["\"Movie\" was not set whole: Desk"]);
    }
}

#[tokio::test]
async fn load_json_is_the_name_and_the_parts_and_still_exits_3_for_a_failed_part() {
    let daemon = stub().await;
    daemon.scenes().answers(Response::Scene {
        name: "Movie".into(),
        outcome: vec![outcome("Desk", PartStatus::Failed, &["link lost"])],
    });
    let said = run_json(&daemon, true, load_of("Movie", false))
        .await
        .expect("answered");
    let value: serde_json::Value = serde_json::from_str(&said.out[0]).expect("json");
    assert_eq!(value["name"], "Movie");
    assert_eq!(value["outcome"][0]["status"], "failed");
    assert_eq!(value["outcome"][0]["notes"][0], "link lost");
    assert_eq!(said.code, 3);
}

fn both_sides() -> Response {
    Response::Ask {
        message: "Late night leaves Warm on the Desk DAC and on the computer: it plays twice."
            .into(),
        name: "Late night".into(),
        devices: vec!["Desk".into()],
    }
}

fn recalled_ok() -> Response {
    Response::Scene {
        name: "Late night".into(),
        outcome: vec![outcome("computer", PartStatus::Done, &[])],
    }
}

#[tokio::test]
async fn load_with_yes_answers_the_both_sides_question_by_sending_confirm() {
    let daemon = stub().await;
    daemon.scenes().answers(both_sides());
    daemon.scenes().answers(recalled_ok());
    let said = run_plain(&daemon, load_of("Late night", true))
        .await
        .expect("recalled");
    assert_eq!(said.out[0], "Recalled \"Late night\".");
    assert_eq!(
        said.err,
        ["Late night leaves Warm on the Desk DAC and on the computer: it plays twice."],
        "the daemon's message is a note when --yes answers it"
    );
    assert_eq!(
        daemon.scenes().ops(),
        [
            SceneOp::Recall {
                name: "Late night".into(),
                confirm: false
            },
            SceneOp::Recall {
                name: "Late night".into(),
                confirm: true
            }
        ]
    );
}

#[tokio::test]
async fn load_at_a_terminal_asks_with_the_daemon_s_message_and_a_yes_sends_confirm() {
    let daemon = stub().await;
    daemon.scenes().answers(both_sides());
    daemon.scenes().answers(recalled_ok());
    let (done, questions) = run_asked(&daemon, load_of("Late night", false), true).await;
    done.expect("recalled");
    assert_eq!(
        questions,
        [
            "Late night leaves Warm on the Desk DAC and on the computer: it plays twice. Recall it anyway? [y/N]"
        ]
    );
    assert_eq!(daemon.scenes().ops().len(), 2);
}

#[tokio::test]
async fn load_at_a_terminal_answered_no_sends_nothing_more_and_changes_nothing() {
    let daemon = stub().await;
    daemon.scenes().answers(both_sides());
    let (done, _) = run_asked(&daemon, load_of("Late night", false), false).await;
    let said = done.expect("declined");
    assert_eq!(said.code, 0);
    assert_eq!(said.err, ["nothing changed"]);
    assert_eq!(daemon.scenes().ops().len(), 1, "only the first request");
}

#[tokio::test]
async fn load_without_a_terminal_or_yes_refuses_with_the_message_and_sends_no_confirm() {
    let daemon = stub().await;
    daemon.scenes().answers(both_sides());
    let failure = run_plain(&daemon, load_of("Late night", false))
        .await
        .expect_err("refused");
    assert_eq!(
        failure.what,
        "Late night leaves Warm on the Desk DAC and on the computer: it plays twice. Pass --yes to recall it anyway"
    );
    assert_ne!(failure.code, 0);
    assert_eq!(daemon.scenes().ops().len(), 1);
}

#[tokio::test]
async fn rename_sends_both_names_and_says_the_new_one() {
    let daemon = stub().await;
    daemon.scenes().answers(Response::Scene {
        name: "Evening".into(),
        outcome: Vec::new(),
    });
    let said = run_plain(
        &daemon,
        SceneCommand::Rename {
            old: "Late night".into(),
            new: "Evening".into(),
        },
    )
    .await
    .expect("renamed");
    assert_eq!(said.out, ["Renamed \"Late night\" to \"Evening\"."]);
    assert_eq!(
        daemon.scenes().ops(),
        [SceneOp::Rename {
            from: "Late night".into(),
            to: "Evening".into()
        }]
    );
}

#[tokio::test]
async fn delete_asks_with_the_daemon_s_spelling_and_a_no_sends_nothing() {
    let daemon = stub().await;
    daemon.scenes().shows(two_scenes());
    let command = SceneCommand::Delete {
        name: "movie".into(),
        yes: false,
    };
    let (done, questions) = run_asked(&daemon, command, false).await;
    assert_eq!(done.expect("declined").code, 1);
    assert_eq!(questions, ["Delete scene \"Movie\"? [y/N]"]);
    assert!(daemon.scenes().ops().is_empty());
}

#[tokio::test]
async fn delete_of_an_unknown_name_is_refused_before_anything_is_asked() {
    let daemon = stub().await;
    daemon.scenes().shows(two_scenes());
    let command = SceneCommand::Delete {
        name: "Desk".into(),
        yes: false,
    };
    let (done, questions) = run_asked(&daemon, command, true).await;
    let failure = done.expect_err("no such scene");
    assert_eq!(failure.what, "no scene named \"Desk\"");
    assert_eq!(failure.code, CODE_USAGE);
    assert!(questions.is_empty());
    assert!(daemon.scenes().ops().is_empty());
}

#[tokio::test]
async fn delete_without_a_terminal_or_yes_refuses_and_sends_nothing() {
    let daemon = stub().await;
    daemon.scenes();
    let failure = run_plain(
        &daemon,
        SceneCommand::Delete {
            name: "Movie".into(),
            yes: false,
        },
    )
    .await
    .expect_err("refused");
    assert_eq!(
        failure.what,
        "refusing to delete a scene without confirmation; pass --yes"
    );
    assert!(daemon.scenes().ops().is_empty());
}

#[tokio::test]
async fn delete_with_yes_and_after_a_terminal_yes_sends_the_delete() {
    let daemon = stub().await;
    daemon.scenes().shows(two_scenes());
    for _ in 0..2 {
        daemon.scenes().answers(Response::Scene {
            name: "Movie".into(),
            outcome: Vec::new(),
        });
    }
    let said = run_plain(
        &daemon,
        SceneCommand::Delete {
            name: "movie".into(),
            yes: true,
        },
    )
    .await
    .expect("deleted");
    assert_eq!(said.out, ["Deleted \"Movie\"."]);
    let (done, _) = run_asked(
        &daemon,
        SceneCommand::Delete {
            name: "movie".into(),
            yes: false,
        },
        true,
    )
    .await;
    done.expect("deleted");
    assert_eq!(
        daemon.scenes().ops(),
        vec![
            SceneOp::Delete {
                name: "movie".into()
            };
            2
        ]
    );
}

#[tokio::test]
async fn a_verb_with_no_name_is_list() {
    let daemon = stub().await;
    daemon.scenes().shows(two_scenes());
    let mut never = |_: &str| -> std::io::Result<bool> { panic!("nothing is asked") };
    let mut asking = Asking {
        terminal: false,
        ask: &mut never,
    };
    let said = answer(&daemon.socket, &Config::default(), false, None, &mut asking)
        .await
        .expect("listed");
    assert_eq!(said.out.len(), 2);
}
