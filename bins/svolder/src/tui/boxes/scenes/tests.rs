// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_ipc::socket::protocol::{PartOutcome, PartStatus, SceneOp};

use crate::tui::app::{App, Intent, Screen};
use crate::tui::boxes::toast::ToastKind;
use crate::tui::test_support::{
    app_showing, mockup_scenes, offering_scenes, press, scenes_view, type_text,
};

use super::*;

/// An app on the device screen whose daemon keeps the mockups' three scenes, with the box open.
fn open() -> App {
    let mut app = offering_scenes(app_showing(Screen::Device), mockup_scenes());
    press(&mut app, KeyCode::Char('z'));
    assert!(app.scenes.list.is_some(), "z opens the box");
    app
}

/// The toast showing, as its words and kind.
fn toast(app: &App) -> (String, ToastKind) {
    let toast = app.toast().expect("a toast is showing");
    (toast.text.clone(), toast.kind)
}

fn step(app: &App) -> Step {
    app.scenes
        .list
        .as_ref()
        .expect("the box is open")
        .step
        .clone()
}

fn row(app: &App) -> usize {
    app.scenes.list.as_ref().expect("the box is open").row
}

fn recall(name: &str, confirm: bool) -> Vec<Intent> {
    vec![Intent::Scene(SceneOp::Recall {
        name: name.into(),
        confirm,
    })]
}

fn part(who: &str, status: PartStatus, notes: &[&str]) -> PartOutcome {
    PartOutcome {
        who: who.into(),
        status,
        notes: notes.iter().map(|note| (*note).to_string()).collect(),
    }
}

#[test]
fn z_without_a_daemon_says_the_daemon_keeps_scenes_and_opens_nothing() {
    let mut app = app_showing(Screen::Device);
    app.session.daemon = false;
    assert!(press(&mut app, KeyCode::Char('z')).is_empty());
    assert!(app.scenes.list.is_none());
    assert_eq!(
        toast(&app),
        (
            "scenes are kept by the daemon: svolder daemon install".to_string(),
            ToastKind::Failure
        ),
        "the step names a verb that exists"
    );
}

#[test]
fn z_on_a_daemon_without_the_feature_says_it_keeps_no_scenes() {
    let mut app = app_showing(Screen::Device);
    app.session.daemon = true;
    app.scenes_served(false);
    press(&mut app, KeyCode::Char('z'));
    assert!(app.scenes.list.is_none());
    assert_eq!(toast(&app).0, "this svolderd keeps no scenes");
}

#[test]
fn z_before_the_view_has_arrived_opens_nothing() {
    let mut app = app_showing(Screen::Device);
    app.session.daemon = true;
    app.scenes_served(true);
    assert!(!app.scenes_offered(), "the feature alone is not enough");
    press(&mut app, KeyCode::Char('z'));
    assert!(app.scenes.list.is_none());
}

#[test]
fn z_opens_the_box_on_every_screen_with_the_cursor_on_the_scene_that_is_on() {
    for screen in [
        Screen::Device,
        Screen::Tone,
        Screen::Telemetry,
        Screen::Devices,
    ] {
        let mut app = offering_scenes(app_showing(screen), mockup_scenes());
        press(&mut app, KeyCode::Char('z'));
        assert!(app.modal_open(), "{screen:?}");
        assert_eq!(row(&app), 0, "{screen:?}: Late night is on");
    }
    let mut view = mockup_scenes();
    view.on = Some("Movie".into());
    let mut app = offering_scenes(app_showing(Screen::Device), view);
    press(&mut app, KeyCode::Char('z'));
    assert_eq!(row(&app), 2);
}

#[test]
fn the_arrows_stop_at_the_ends_and_esc_closes_the_box() {
    let mut app = open();
    press(&mut app, KeyCode::Up);
    assert_eq!(row(&app), 0);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(row(&app), 2);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(app.scenes.list.is_none());
}

#[test]
fn enter_recalls_the_focused_scene_and_the_box_stays_until_the_daemon_answers() {
    let mut app = open();
    press(&mut app, KeyCode::Down);
    assert_eq!(press(&mut app, KeyCode::Enter), recall("Desk", false));
    assert!(app.scenes.list.is_some());
}

#[test]
fn n_offers_a_numbered_name_and_enter_saves_what_was_typed() {
    let mut app = open();
    press(&mut app, KeyCode::Char('n'));
    match step(&app) {
        Step::Naming(line) => {
            assert_eq!(line.text, "Late night 2");
            assert_eq!(line.renaming, None);
        }
        other => panic!("a name prompt, got {other:?}"),
    }
    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "3");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Scene(SceneOp::Save {
            name: "Late night 3".into(),
            force: false
        })]
    );
    assert_eq!(step(&app), Step::Choosing);
}

#[test]
fn n_on_no_scenes_offers_a_first_name() {
    let view = scenes_view(&[], None, true);
    let mut app = offering_scenes(app_showing(Screen::Device), view);
    press(&mut app, KeyCode::Char('z'));
    press(&mut app, KeyCode::Char('n'));
    match step(&app) {
        Step::Naming(line) => assert_eq!(line.text, "Scene 1"),
        other => panic!("a name prompt, got {other:?}"),
    }
}

#[test]
fn the_name_prompt_edits_at_the_cursor_and_esc_sends_nothing() {
    let mut app = open();
    press(&mut app, KeyCode::Char('n'));
    for _ in 0.."Late night 2".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "Dsk");
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    type_text(&mut app, "e");
    match step(&app) {
        Step::Naming(line) => assert_eq!((line.text.as_str(), line.caret), ("Desk", 2)),
        other => panic!("a name prompt, got {other:?}"),
    }
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert_eq!(step(&app), Step::Choosing);
    assert!(
        app.scenes.list.is_some(),
        "Esc leaves the prompt, not the box"
    );
}

#[test]
fn a_name_takes_no_more_characters_than_a_name_may_have() {
    let mut app = open();
    press(&mut app, KeyCode::Char('n'));
    type_text(&mut app, &"x".repeat(100));
    match step(&app) {
        Step::Naming(line) => assert_eq!(line.text.chars().count(), NAME_MAX),
        other => panic!("a name prompt, got {other:?}"),
    }
}

#[test]
fn u_asks_and_y_saves_over_the_scene_with_force() {
    let mut app = open();
    press(&mut app, KeyCode::Down);
    assert!(press(&mut app, KeyCode::Char('u')).is_empty());
    assert_eq!(step(&app), Step::Asking(Question::Update("Desk".into())));
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::Scene(SceneOp::Save {
            name: "Desk".into(),
            force: true
        })]
    );
    assert_eq!(step(&app), Step::Choosing);
}

#[test]
fn x_asks_and_only_y_deletes() {
    for no in [
        KeyCode::Char('n'),
        KeyCode::Enter,
        KeyCode::Esc,
        KeyCode::Char('q'),
    ] {
        let mut app = open();
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(
            step(&app),
            Step::Asking(Question::Delete("Late night".into()))
        );
        assert!(press(&mut app, no).is_empty(), "{no:?}");
        if no == KeyCode::Char('q') {
            // Another key is swallowed: the question stays.
            assert!(matches!(step(&app), Step::Asking(_)));
        } else {
            assert_eq!(step(&app), Step::Choosing, "{no:?} answers no");
        }
    }
    let mut app = open();
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::Scene(SceneOp::Delete {
            name: "Late night".into()
        })]
    );
}

#[test]
fn c_holds_the_old_name_and_enter_renames() {
    let mut app = open();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char('c'));
    match step(&app) {
        Step::Naming(line) => {
            assert_eq!(line.text, "Desk");
            assert_eq!(line.renaming.as_deref(), Some("Desk"));
        }
        other => panic!("a name prompt, got {other:?}"),
    }
    type_text(&mut app, "top");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Scene(SceneOp::Rename {
            from: "Desk".into(),
            to: "Desktop".into()
        })]
    );
}

#[test]
fn the_box_takes_every_key_so_none_reaches_the_screen_under_it() {
    let mut app = open();
    for key in ['s', 'q', 'e', 't', 'd', '?', 'M', 'r', 'm'] {
        assert!(press(&mut app, KeyCode::Char(key)).is_empty(), "{key}");
        assert!(app.scenes.list.is_some(), "{key} closed the box");
        assert_eq!(app.screen, Screen::Device);
        assert!(app.overlays.settings.is_none());
        assert!(!app.overlays.quitting && !app.overlays.help_open);
    }
}

/// A scene never writes a volume, gain select, mute or power: every key of the box answers with a
/// scene request or nothing, never a write to a device or the computer's sink.
#[test]
fn no_key_of_the_box_asks_for_anything_but_a_scene_request() {
    let keys = [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::Enter,
        KeyCode::Char('n'),
        KeyCode::Char('x'),
        KeyCode::Char('y'),
        KeyCode::Char('u'),
        KeyCode::Char('y'),
        KeyCode::Char('c'),
        KeyCode::Enter,
        KeyCode::Char('m'),
        KeyCode::Char('+'),
        KeyCode::Left,
        KeyCode::Right,
    ];
    let mut app = open();
    for key in keys {
        for intent in press(&mut app, key) {
            assert!(matches!(intent, Intent::Scene(_)), "{key:?}: {intent:?}");
        }
    }
}

#[test]
fn a_recall_that_asks_puts_the_question_in_the_last_row_and_y_confirms() {
    let mut app = open();
    press(&mut app, KeyCode::Down);
    let op = SceneOp::Recall {
        name: "Desk".into(),
        confirm: false,
    };
    let message = "Desk leaves Warm on the Desk DAC and on the computer: it plays twice.";
    assert!(
        app.scenes_done(SceneDone::Asked {
            op,
            message: message.into()
        })
        .is_empty()
    );
    assert_eq!(
        step(&app),
        Step::Asking(Question::Both {
            name: "Desk".into(),
            message: message.into()
        })
    );
    assert_eq!(press(&mut app, KeyCode::Char('y')), recall("Desk", true));
    assert_eq!(step(&app), Step::Choosing);
}

#[test]
fn an_ask_answered_no_sends_nothing() {
    let mut app = open();
    app.scenes_done(SceneDone::Asked {
        op: SceneOp::Recall {
            name: "Late night".into(),
            confirm: false,
        },
        message: "plays twice".into(),
    });
    assert!(press(&mut app, KeyCode::Char('n')).is_empty());
    assert_eq!(step(&app), Step::Choosing);
}

#[test]
fn a_recall_says_its_name_and_each_part_s_notes_in_one_toast_and_closes_the_box() {
    let mut app = open();
    app.scenes_done(SceneDone::Answered {
        op: SceneOp::Recall {
            name: "Movie".into(),
            confirm: false,
        },
        name: "Movie".into(),
        outcome: vec![
            part("computer", PartStatus::Done, &[]),
            part("Desk DAC", PartStatus::Done, &[]),
            part(
                "Hall",
                PartStatus::Left,
                &["Hall is not connected: its part was left as it is"],
            ),
        ],
    });
    assert_eq!(
        toast(&app),
        (
            "Movie recalled. Hall is not connected: its part was left as it is".to_string(),
            ToastKind::Failure
        )
    );
    assert!(app.scenes.list.is_none());
}

#[test]
fn a_whole_recall_is_an_info_toast() {
    let mut app = open();
    app.scenes_done(SceneDone::Answered {
        op: SceneOp::Recall {
            name: "Desk".into(),
            confirm: false,
        },
        name: "Desk".into(),
        outcome: vec![part("computer", PartStatus::Done, &[])],
    });
    assert_eq!(toast(&app), ("Desk recalled.".to_string(), ToastKind::Info));
}

#[test]
fn a_part_that_failed_or_was_set_in_part_with_no_note_still_says_so() {
    let outcome = [
        part("Desk DAC", PartStatus::Failed, &[]),
        part("computer", PartStatus::Partial, &[]),
    ];
    assert_eq!(
        recalled_words("Desk", &outcome),
        "Desk recalled. Desk DAC failed. computer was set in part."
    );
    assert!(left_out(&outcome));
    assert!(!left_out(&[part("computer", PartStatus::Done, &[])]));
}

#[test]
fn a_refusal_is_the_daemon_s_words_and_the_box_stays() {
    let mut app = open();
    app.scenes_done(SceneDone::Refused {
        op: SceneOp::Save {
            name: "Desk".into(),
            force: false,
        },
        words: "a scene named Desk is already saved".into(),
    });
    assert_eq!(
        toast(&app),
        (
            "a scene named Desk is already saved".to_string(),
            ToastKind::Failure
        )
    );
    assert!(app.scenes.list.is_some());
}

#[test]
fn saves_renames_and_deletes_are_told_in_a_toast() {
    let said = |op: SceneOp, name: &str| {
        let mut app = open();
        app.scenes_done(SceneDone::Answered {
            op,
            name: name.into(),
            outcome: Vec::new(),
        });
        toast(&app).0
    };
    let save = |force| SceneOp::Save {
        name: "x".into(),
        force,
    };
    assert_eq!(said(save(false), "Late night 2"), "Saved Late night 2.");
    assert_eq!(said(save(true), "Desk"), "Updated Desk.");
    let rename = SceneOp::Rename {
        from: "Desk".into(),
        to: "Study".into(),
    };
    assert_eq!(said(rename, "Study"), "Renamed to Study.");
    let delete = SceneOp::Delete {
        name: "Movie".into(),
    };
    assert_eq!(said(delete, "Movie"), "Deleted Movie.");
}

/// The write's answer and the stream's view come in either order: the cursor lands on the new
/// scene either way.
#[test]
fn the_cursor_follows_a_saved_scene_whichever_of_the_answer_and_the_view_comes_first() {
    let saved = SceneOp::Save {
        name: "Evening".into(),
        force: false,
    };
    let with_evening = || {
        let mut view = mockup_scenes();
        view.scenes.push(svolder_ipc::socket::protocol::SceneRow {
            name: "Evening".into(),
            parts: Vec::new(),
        });
        view
    };
    // The answer first, then the view.
    let mut app = open();
    app.scenes_done(SceneDone::Answered {
        op: saved.clone(),
        name: "Evening".into(),
        outcome: Vec::new(),
    });
    assert_eq!(row(&app), 0, "not listed yet");
    app.scenes_view(with_evening());
    assert_eq!(row(&app), 3);
    // The view first, then the answer.
    let mut app = open();
    app.scenes_view(with_evening());
    assert_eq!(row(&app), 0);
    app.scenes_done(SceneDone::Answered {
        op: saved,
        name: "Evening".into(),
        outcome: Vec::new(),
    });
    assert_eq!(row(&app), 3);
}

#[test]
fn a_view_with_fewer_scenes_holds_the_cursor_inside_it() {
    let mut app = open();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    app.scenes_view(scenes_view(&[("Desk", &[("computer", true)])], None, true));
    assert_eq!(row(&app), 0);
    assert_eq!(app.scenes.focused(), Some("Desk"));
}

#[test]
fn a_daemon_that_stops_listing_scenes_takes_the_box_away() {
    let mut app = open();
    app.scenes_served(false);
    assert!(app.scenes.list.is_none() && app.scenes.view.is_none());
    assert!(!app.scenes_offered());
}

// ---- fix 1 ----

#[test]
fn z_with_the_feature_listed_and_no_view_yet_says_scenes_are_coming_not_that_there_are_none() {
    let mut app = app_showing(Screen::Device);
    app.session.daemon = true;
    app.scenes_served(true);
    press(&mut app, KeyCode::Char('z'));
    assert!(app.scenes.list.is_none());
    assert_eq!(toast(&app).0, READING);
    // A daemon that has not said what it lists yet is not one that keeps none either.
    let mut app = app_showing(Screen::Device);
    app.session.daemon = true;
    press(&mut app, KeyCode::Char('z'));
    assert_eq!(toast(&app).0, READING);
}

#[test]
fn a_focused_name_of_40_characters_gives_a_suggestion_a_name_may_be() {
    for len in [38, 39, 40] {
        let long = "x".repeat(len);
        let view = scenes_view(&[(long.as_str(), &[("computer", true)])], None, true);
        let mut app = offering_scenes(app_showing(Screen::Device), view);
        press(&mut app, KeyCode::Char('z'));
        press(&mut app, KeyCode::Char('n'));
        match step(&app) {
            Step::Naming(line) => {
                assert!(
                    line.text.chars().count() <= NAME_MAX,
                    "{len}: {}",
                    line.text
                );
                assert!(line.text.ends_with(" 2"), "{len}: {}", line.text);
                assert_ne!(line.text, long);
            }
            other => panic!("a name prompt, got {other:?}"),
        }
    }
}

#[test]
fn a_recall_where_every_part_failed_does_not_say_recalled() {
    let outcome = [
        part(
            "computer",
            PartStatus::Failed,
            &["the computer's EQ refused it"],
        ),
        part("Desk DAC", PartStatus::Left, &[]),
    ];
    let words = recalled_words("Movie", &outcome);
    assert_eq!(
        words,
        "Movie was not recalled. the computer's EQ refused it Desk DAC was left as it is."
    );
    assert!(!words.contains("Movie recalled"));
    // One part set in part is a recall.
    let some = [
        part("computer", PartStatus::Partial, &["a setting is gone"]),
        part("Desk DAC", PartStatus::Failed, &[]),
    ];
    assert!(recalled_words("Movie", &some).starts_with("Movie recalled."));
}

#[test]
fn the_menu_cursor_stays_on_a_row_when_the_scenes_row_goes() {
    let mut app = offering_scenes(app_showing(Screen::Device), mockup_scenes());
    press(&mut app, KeyCode::Esc);
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.overlays.menu, Some(3), "on Quit");
    app.scenes_served(false);
    assert_eq!(app.overlays.menu, Some(2), "Quit is now the third entry");
}

#[test]
fn while_a_request_runs_the_box_says_so_and_ignores_every_key_but_esc() {
    let mut app = open();
    press(&mut app, KeyCode::Down);
    assert_eq!(press(&mut app, KeyCode::Enter), recall("Desk", false));
    assert_eq!(app.scenes.pending.as_deref(), Some("Recalling Desk…"));
    for key in [
        KeyCode::Enter,
        KeyCode::Char('n'),
        KeyCode::Char('u'),
        KeyCode::Char('c'),
        KeyCode::Char('x'),
        KeyCode::Down,
    ] {
        assert!(press(&mut app, key).is_empty(), "{key:?}");
        assert_eq!(step(&app), Step::Choosing, "{key:?}");
    }
    assert_eq!(row(&app), 1, "the cursor did not move");
    // The answer ends the wait: the next Enter is the next recall.
    app.scenes_done(SceneDone::Refused {
        op: SceneOp::Recall {
            name: "Desk".into(),
            confirm: false,
        },
        words: "no".into(),
    });
    assert_eq!(app.scenes.pending, None);
    assert_eq!(press(&mut app, KeyCode::Enter), recall("Desk", false));
}

#[test]
fn every_op_the_box_sends_waits_for_its_answer() {
    let mut app = open();
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(app.scenes.pending.as_deref(), Some("Deleting Late night…"));
    let mut app = open();
    press(&mut app, KeyCode::Char('u'));
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(app.scenes.pending.as_deref(), Some("Saving Late night…"));
    let mut app = open();
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.scenes.pending.as_deref(), Some("Saving Late night 2…"));
    let mut app = open();
    press(&mut app, KeyCode::Char('c'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.scenes.pending.as_deref(), Some("Renaming Late night…"));
}

#[test]
fn the_both_sides_confirm_waits_too_and_a_daemon_that_goes_clears_the_wait() {
    let mut app = open();
    app.scenes_done(SceneDone::Asked {
        op: SceneOp::Recall {
            name: "Desk".into(),
            confirm: false,
        },
        message: "plays twice".into(),
    });
    assert_eq!(press(&mut app, KeyCode::Char('y')), recall("Desk", true));
    assert_eq!(app.scenes.pending.as_deref(), Some("Recalling Desk…"));
    app.scenes_served(false);
    assert_eq!(app.scenes.pending, None);
}

/// The menu has no Scenes row on the feature alone: the view must have arrived.
#[test]
fn the_menu_has_no_scenes_row_before_the_view_arrives() {
    let mut app = app_showing(Screen::Device);
    app.session.daemon = true;
    app.scenes_served(true);
    assert_eq!(
        crate::tui::screens::menu::entries(&app),
        [
            crate::tui::screens::menu::Entry::Settings,
            crate::tui::screens::menu::Entry::Help,
            crate::tui::screens::menu::Entry::Quit
        ]
    );
}

/// The hint is hidden from the help overlay while scenes are not offered, shown once they are.
#[test]
fn the_z_hint_is_hidden_until_scenes_are_offered() {
    let hint = crate::tui::keys::all()
        .iter()
        .find(|hint| hint.code == KeyCode::Char('z'))
        .expect("z is in the table");
    let mut app = app_showing(Screen::Device);
    app.session.daemon = true;
    app.scenes_served(true);
    let hidden =
        |app: &App| crate::tui::keys::hidden_in(hint, crate::tui::keys::KeyContext::of(app));
    assert!(hidden(&app), "served, no view: hidden");
    let app = offering_scenes(app_showing(Screen::Device), mockup_scenes());
    assert!(!hidden(&app));
}

/// `reading…` while the view is not settled and `none` once it is settled with no scene on.
#[test]
fn the_scene_line_tells_unsettled_from_none() {
    use crate::tui::test_support::text;
    let line = |known: bool, on: Option<&str>| {
        let mut view = mockup_scenes();
        view.known = known;
        view.on = on.map(str::to_string);
        let app = offering_scenes(app_showing(Screen::Device), view);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("terminal");
        let mut hits = crate::tui::mouse::HitMap::default();
        terminal
            .draw(|f| {
                crate::tui::render::draw(
                    f,
                    &app,
                    &crate::tui::test_support::true_colour(),
                    &mut hits,
                );
            })
            .expect("draws");
        text(terminal.backend().buffer()).join("\n")
    };
    assert!(line(false, None).contains("Scene  reading…"));
    assert!(line(true, None).contains("Scene  none"));
    assert!(line(true, Some("Desk")).contains("Scene  Desk"));
    assert!(line(false, Some("Desk")).contains("Scene  reading…"));
}

// ---- phase B ----

/// A save answers one `left` row per configured device that is not connected: its note is shown.
#[test]
fn a_save_says_which_devices_it_left_out_in_the_daemons_words() {
    let mut app = open();
    app.scenes_done(SceneDone::Answered {
        op: SceneOp::Save {
            name: "Late night 2".into(),
            force: false,
        },
        name: "Late night 2".into(),
        outcome: vec![part(
            "Hall",
            PartStatus::Left,
            &["Hall is not connected: it is not in the scene"],
        )],
    });
    assert_eq!(
        toast(&app).0,
        "Saved Late night 2. Hall is not connected: it is not in the scene"
    );
    let mut app = open();
    app.scenes_done(SceneDone::Answered {
        op: SceneOp::Save {
            name: "Desk".into(),
            force: true,
        },
        name: "Desk".into(),
        outcome: vec![part(
            "Hall",
            PartStatus::Left,
            &["Hall is not connected: it is not in the scene"],
        )],
    });
    assert_eq!(
        toast(&app).0,
        "Updated Desk. Hall is not connected: it is not in the scene"
    );
}

/// The refusals 2.1 added are the daemon's words, as every refusal is.
#[test]
fn nothing_to_save_and_internal_refusals_are_the_daemons_words() {
    for words in [
        "nothing to save: no device is connected and the computer's controls are not there",
        "the scene could not be saved: internal error",
    ] {
        let mut app = open();
        app.scenes_done(SceneDone::Refused {
            op: SceneOp::Save {
                name: "x".into(),
                force: false,
            },
            words: words.into(),
        });
        assert_eq!(toast(&app), (words.to_string(), ToastKind::Failure));
    }
}
