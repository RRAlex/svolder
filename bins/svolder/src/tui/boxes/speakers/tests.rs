// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_ipc::{HostEqOp, SpeakersCatalogue, SpeakersSaved, SpeakersSet};

use super::*;
use crate::tui::app::{Screen, ToneRow};
use crate::tui::test_support::{app_showing, computer_eq_with_speakers, press};

const FETCHED: [(&str, &str, &str); 2] = [
    ("sadie-d2", "no", "SADIE II, D2"),
    ("room-1", "with", "Control room 1"),
];

fn set(id: &str, room: SpeakersRoom, label: &str, fetched: bool, bytes: u64) -> SpeakersSet {
    SpeakersSet {
        id: id.to_string(),
        room,
        label: label.to_string(),
        licence: "Apache-2.0".to_string(),
        credit: String::new(),
        source_url: "https://sofacoustics.example/data/".to_string(),
        bytes,
        own: false,
        fetched,
        damaged: None,
        angles: None,
        taps_ms: 0.0,
        rate: 0,
        loudness_db: 0.0,
        cut: false,
    }
}

fn catalogue(fetching: bool) -> SpeakersCatalogue {
    SpeakersCatalogue {
        sets: vec![
            set(
                "sadie-d2",
                SpeakersRoom::No,
                "SADIE II, D2",
                true,
                35_000_000,
            ),
            set(
                "sadie-d1",
                SpeakersRoom::No,
                "SADIE II, D1",
                false,
                35_000_000,
            ),
            set(
                "room-1",
                SpeakersRoom::With,
                "Control room 1",
                true,
                43_000_000,
            ),
            set(
                "room-2",
                SpeakersRoom::With,
                "Control room 2",
                false,
                43_000_000,
            ),
        ],
        fetching,
        note: None,
        max_sets: 32,
    }
}

/// An app with the speakers on the tone screen's computer side, playing `sadie-d2`.
fn app(daemon: bool) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_speakers(true, Some("sadie-d2"), &FETCHED));
    app.flip_side();
    app.show(Screen::Tone);
    app.tone.row = ToneRow::Mode;
    app.session.daemon = daemon;
    app
}

/// The same with the box open and the daemon's catalogue in.
fn open(daemon: bool) -> App {
    let mut app = app(daemon);
    app.open_sets();
    app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(catalogue(true)))));
    app
}

/// The one call to the sets an answer holds.
fn one_call(intents: &[Intent]) -> &SpeakersCall {
    match intents {
        [Intent::Speakers(call)] => call,
        other => panic!("one call to the sets wanted, got {other:?}"),
    }
}

fn ids(app: &App) -> Vec<String> {
    app.sets_rows().into_iter().map(|row| row.id).collect()
}

fn move_to(app: &mut App, id: &str) {
    let at = ids(app).iter().position(|i| i == id).expect("a row");
    app.speakers_box.list.as_mut().expect("the list").row = at;
}

#[test]
fn the_box_opens_on_the_chosen_set_and_asks_the_daemon_for_the_catalogue() {
    let mut app = app(true);
    let asked = app.open_sets();
    assert_eq!(asked, [Intent::speakers(SpeakersCall::Catalogue)]);
    assert_eq!(app.speakers_box.list.as_ref().map(|l| l.row), Some(0));
    // with no daemon there is nothing to ask
    let mut app = self::app(false);
    assert!(app.open_sets().is_empty());
}

#[test]
fn before_the_catalogue_the_rows_are_the_fetched_sets_of_the_view() {
    let mut app = app(false);
    app.open_sets();
    assert_eq!(ids(&app), ["sadie-d2", "room-1"]);
    let rows = app.sets_rows();
    assert!(rows.iter().all(|row| row.fetched));
    assert!(rows[0].chosen && !rows[1].chosen);
}

#[test]
fn the_catalogue_lists_every_set_and_marks_the_chosen_one() {
    let app = open(true);
    assert_eq!(ids(&app), ["sadie-d2", "sadie-d1", "room-1", "room-2"]);
    let rows = app.sets_rows();
    assert!(rows[0].chosen && rows[0].fetched);
    assert!(!rows[1].fetched);
}

#[test]
fn tab_cycles_the_filter_and_the_rows_follow() {
    let mut app = open(true);
    press(&mut app, KeyCode::Tab);
    assert_eq!(ids(&app), ["room-1", "room-2"], "With room");
    press(&mut app, KeyCode::Tab);
    assert_eq!(ids(&app), ["sadie-d2", "sadie-d1"], "No room");
    press(&mut app, KeyCode::Tab);
    assert_eq!(ids(&app).len(), 4, "All again");
    press(&mut app, KeyCode::BackTab);
    assert_eq!(ids(&app), ["sadie-d2", "sadie-d1"]);
    assert_eq!(
        app.speakers_box.list.as_ref().map(|list| list.row),
        Some(0),
        "the focus goes back to the top"
    );
}

#[test]
fn the_choices_words_carry_their_technical_terms() {
    assert_eq!(room_words(SpeakersRoom::With), "With room \u{b7} BRIR");
    assert_eq!(room_words(SpeakersRoom::No), "No room \u{b7} HRTF");
    assert_eq!(Filter::With.words(), "With room \u{b7} BRIR");
    assert_eq!(Filter::All.words(), "All");
}

#[test]
fn enter_on_a_fetched_set_chooses_it_and_closes_the_box() {
    let mut app = open(true);
    move_to(&mut app, "room-1");
    let sent = press(&mut app, KeyCode::Enter);
    assert!(app.speakers_box.list.is_none());
    match sent.as_slice() {
        [Intent::HostEq(op)] => assert_eq!(
            **op,
            HostEqOp::Speakers {
                on: Some(true),
                set: Some("room-1".into()),
                done: true
            }
        ),
        other => panic!("one choice wanted, got {other:?}"),
    }
}

#[test]
fn enter_on_a_set_not_fetched_starts_one_fetch_and_the_box_stays() {
    let mut app = open(true);
    move_to(&mut app, "sadie-d1");
    let sent = press(&mut app, KeyCode::Enter);
    let fetch = app.speakers_box.fetch.clone().expect("a fetch");
    assert_eq!(
        one_call(&sent),
        &SpeakersCall::Fetch {
            id: "sadie-d1".into(),
            label: "SADIE II, D1".into(),
            stop: fetch.stop.clone()
        }
    );
    assert!(app.speakers_box.list.is_some());
    assert_eq!((fetch.id.as_str(), fetch.total), ("sadie-d1", 35_000_000));
    // a second one waits for the first
    move_to(&mut app, "room-2");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        app.toast()
            .is_some_and(|t| t.text.contains("still fetching SADIE II, D1"))
    );
}

#[test]
fn fetching_with_no_daemon_or_a_daemon_that_reaches_nothing_says_so() {
    let mut app = open(false);
    move_to(&mut app, "sadie-d1");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.speakers_box.fetch.is_none());
    assert!(app.toast().is_some_and(|t| t.text.contains("svolderd")));
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_speakers(true, Some("sadie-d2"), &FETCHED));
    app.flip_side();
    app.session.daemon = true;
    app.open_sets();
    app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(catalogue(false)))));
    move_to(&mut app, "sadie-d1");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        app.toast()
            .is_some_and(|t| t.text.contains("reach nothing"))
    );
}

#[test]
fn progress_rides_on_the_row_and_the_end_clears_it_and_asks_for_the_catalogue_again() {
    let mut app = open(true);
    move_to(&mut app, "sadie-d1");
    press(&mut app, KeyCode::Enter);
    app.speakers_done(SpeakersDone::Progress {
        id: "sadie-d1".into(),
        received: 17_500_000,
        total: 35_000_000,
    });
    let fetch = app.speakers_box.fetch.clone().expect("a fetch");
    assert_eq!(fetch.percent(), Some(50));
    // progress for a set that is not the one fetched changes nothing
    app.speakers_done(SpeakersDone::Progress {
        id: "other".into(),
        received: 1,
        total: 2,
    });
    assert_eq!(
        app.speakers_box.fetch.as_ref().and_then(Fetch::percent),
        Some(50)
    );
    let saved = SpeakersSaved {
        set: set(
            "sadie-d1",
            SpeakersRoom::No,
            "SADIE II, D1",
            true,
            35_000_000,
        ),
        evicted: Vec::new(),
    };
    let next = app.speakers_done(SpeakersDone::Ended {
        id: "sadie-d1".into(),
        result: Ok(Box::new(saved)),
    });
    assert!(app.speakers_box.fetch.is_none());
    assert_eq!(next, [Intent::speakers(SpeakersCall::Catalogue)]);
    assert!(
        app.toast()
            .is_some_and(|t| t.text.contains("SADIE II, D1 is ready"))
    );
}

/// The two events that answer a fetch (the end, and the catalogue it asks for) in either order:
/// the box ends in the same state, with the set fetched and no fetch in flight.
#[test]
fn the_end_and_the_catalogue_may_come_in_either_order() {
    let fetched = {
        let mut sets = catalogue(true);
        sets.sets[1].fetched = true;
        sets
    };
    let saved = || SpeakersSaved {
        set: set(
            "sadie-d1",
            SpeakersRoom::No,
            "SADIE II, D1",
            true,
            35_000_000,
        ),
        evicted: Vec::new(),
    };
    let mut first = open(true);
    move_to(&mut first, "sadie-d1");
    press(&mut first, KeyCode::Enter);
    first.speakers_done(SpeakersDone::Ended {
        id: "sadie-d1".into(),
        result: Ok(Box::new(saved())),
    });
    first.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(fetched.clone()))));
    let mut second = open(true);
    move_to(&mut second, "sadie-d1");
    press(&mut second, KeyCode::Enter);
    second.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(fetched))));
    second.speakers_done(SpeakersDone::Ended {
        id: "sadie-d1".into(),
        result: Ok(Box::new(saved())),
    });
    for app in [&first, &second] {
        assert!(app.speakers_box.fetch.is_none());
        assert!(
            app.sets_rows()
                .iter()
                .find(|r| r.id == "sadie-d1")
                .is_some_and(|r| r.fetched)
        );
    }
    // a progress that comes after the end changes nothing
    first.speakers_done(SpeakersDone::Progress {
        id: "sadie-d1".into(),
        received: 5,
        total: 10,
    });
    assert!(first.speakers_box.fetch.is_none());
}

#[tokio::test]
async fn x_stops_the_fetch_by_pressing_its_stop_and_says_when_there_is_none() {
    let mut app = open(true);
    assert!(press(&mut app, KeyCode::Char('x')).is_empty());
    assert!(
        app.toast()
            .is_some_and(|t| t.text.contains("nothing is being fetched"))
    );
    move_to(&mut app, "sadie-d1");
    press(&mut app, KeyCode::Enter);
    let stop = app.speakers_box.fetch.clone().expect("a fetch").stop;
    // nothing pressed yet
    let waiting = tokio::time::timeout(std::time::Duration::from_millis(20), stop.0.next()).await;
    assert!(waiting.is_err(), "no press before x");
    assert!(press(&mut app, KeyCode::Char('x')).is_empty());
    tokio::time::timeout(std::time::Duration::from_secs(5), stop.0.next())
        .await
        .expect("x pressed the stop");
    // the stop ends the job: said as a stop, not as a failure
    app.speakers_done(SpeakersDone::Ended {
        id: "sadie-d1".into(),
        result: Err(CANCELLED.to_string()),
    });
    assert!(app.speakers_box.fetch.is_none());
    assert!(app.toast().is_some_and(|t| t.text.contains("stopped")));
}

#[test]
fn a_fetch_that_fails_says_why_and_keeps_the_box() {
    let mut app = open(true);
    move_to(&mut app, "sadie-d1");
    press(&mut app, KeyCode::Enter);
    app.speakers_done(SpeakersDone::Ended {
        id: "sadie-d1".into(),
        result: Err("the source changed; svolder needs an update".to_string()),
    });
    assert!(app.speakers_box.fetch.is_none() && app.speakers_box.list.is_some());
    assert!(
        app.toast()
            .is_some_and(|t| t.text.contains("the source changed"))
    );
}

#[test]
fn forgetting_asks_first_and_refuses_the_set_that_plays() {
    let mut app = open(true);
    // sadie-d2 is chosen and playing
    press(&mut app, KeyCode::Char('f'));
    assert!(matches!(
        app.speakers_box.list.as_ref().map(|l| &l.step),
        Some(Step::Choosing)
    ));
    assert!(app.toast().is_some_and(|t| t.text.contains("plays now")));
    move_to(&mut app, "room-1");
    press(&mut app, KeyCode::Char('f'));
    assert!(matches!(
        app.speakers_box.list.as_ref().map(|l| &l.step),
        Some(Step::Forgetting(id, _)) if id == "room-1"
    ));
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        [Intent::speakers(SpeakersCall::Forget {
            id: "room-1".into()
        })]
    );
    // any other key keeps it
    press(&mut app, KeyCode::Char('f'));
    assert!(press(&mut app, KeyCode::Char('n')).is_empty());
    assert!(matches!(
        app.speakers_box.list.as_ref().map(|l| &l.step),
        Some(Step::Choosing)
    ));
    // a set that is not fetched has nothing to forget
    move_to(&mut app, "sadie-d1");
    press(&mut app, KeyCode::Char('f'));
    assert!(matches!(
        app.speakers_box.list.as_ref().map(|l| &l.step),
        Some(Step::Choosing)
    ));
}

#[test]
fn a_forget_the_daemon_refuses_is_said_and_the_rows_are_asked_for_again() {
    let mut app = open(true);
    let next = app.speakers_done(SpeakersDone::Forgotten {
        id: "room-1".into(),
        result: Err("Control room 1 plays now".to_string()),
    });
    assert_eq!(next, [Intent::speakers(SpeakersCall::Catalogue)]);
    assert!(app.toast().is_some_and(|t| t.text.contains("plays now")));
}

#[test]
fn the_file_prompt_imports_under_the_choice_picked_with_tab() {
    let mut app = open(true);
    let dir = std::env::temp_dir().join(format!("svolder-4-5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let file = dir.join("head.sofa");
    std::fs::write(&file, b"x").expect("a file");
    press(&mut app, KeyCode::Char('i'));
    // the prompt starts at ~/; the path typed over it
    for _ in 0..2 {
        press(&mut app, KeyCode::Backspace);
    }
    for c in file.to_string_lossy().chars() {
        press(&mut app, KeyCode::Char(c));
    }
    press(&mut app, KeyCode::Tab);
    let sent = press(&mut app, KeyCode::Enter);
    match one_call(&sent) {
        SpeakersCall::Import { path, room, .. } => {
            assert!(
                path.ends_with("/head.sofa") && !path.starts_with('~'),
                "{path}"
            );
            assert_eq!(*room, SpeakersRoom::With);
        }
        other => panic!("one import wanted, got {other:?}"),
    }
    let fetch = app.speakers_box.fetch.clone().expect("the import runs");
    assert!(fetch.import && fetch.label == "head.sofa");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_path_is_made_absolute_from_this_folder_and_must_be_a_file() {
    let dir = std::env::temp_dir().join(format!("svolder-4-5-rel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let file = dir.join("head.sofa");
    std::fs::write(&file, b"x").expect("a file");
    assert_eq!(
        resolve_path(&file.to_string_lossy()),
        Ok(file.to_string_lossy().into_owned())
    );
    // a relative path is taken from the current folder
    let here = std::env::current_dir().expect("a folder");
    let relative = file
        .strip_prefix("/")
        .expect("absolute")
        .to_string_lossy()
        .into_owned();
    let up = "../".repeat(here.components().count() - 1);
    assert_eq!(
        resolve_path(&format!("{up}{relative}")).map(|p| p.ends_with("/head.sofa")),
        Ok(true)
    );
    // not a file: said before any job starts
    let missing = resolve_path(&dir.join("none.sofa").to_string_lossy()).expect_err("no file");
    assert!(missing.starts_with("no such file: "), "{missing}");
    assert!(
        resolve_path("Downloads/none.sofa")
            .expect_err("no file")
            .contains(&here.to_string_lossy().into_owned())
    );
    // ~ is the home folder, which is not a file, and is never sent as typed
    if let Err(why) = resolve_path("~") {
        assert!(!why.contains('~'), "{why}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_prompt_path_that_is_no_file_is_refused_and_starts_nothing() {
    let mut app = open(true);
    press(&mut app, KeyCode::Char('i'));
    for c in "nowhere/head.sofa".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.speakers_box.fetch.is_none());
    assert!(app.toast().is_some_and(|t| t.text.contains("no such file")));
}

#[test]
fn the_first_catalogue_keeps_the_cursor_on_the_chosen_set() {
    // the chosen set is far down the catalogue's order
    let mut app = app(true);
    app.open_sets();
    let mut sets = catalogue(true);
    sets.sets
        .insert(0, set("a-first", SpeakersRoom::No, "A first", false, 1));
    sets.sets
        .insert(0, set("b-second", SpeakersRoom::No, "B second", false, 1));
    app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(sets))));
    let at = app.speakers_box.list.as_ref().map(|l| l.row).expect("list");
    assert_eq!(app.sets_rows()[at].id, "sadie-d2");
    // and a moved cursor keeps its own set over a newer catalogue
    move_to(&mut app, "room-1");
    let mut sets = catalogue(true);
    sets.sets
        .insert(0, set("c-new", SpeakersRoom::No, "C new", false, 1));
    app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(sets))));
    let at = app.speakers_box.list.as_ref().map(|l| l.row).expect("list");
    assert_eq!(app.sets_rows()[at].id, "room-1");
}

#[test]
fn a_fetch_that_made_room_says_so_in_one_toast() {
    let mut app = open(true);
    move_to(&mut app, "sadie-d1");
    press(&mut app, KeyCode::Enter);
    let saved = SpeakersSaved {
        set: set("sadie-d1", SpeakersRoom::No, "SADIE II, D1", true, 1),
        evicted: vec!["old".into()],
    };
    app.speakers_done(SpeakersDone::Ended {
        id: "sadie-d1".into(),
        result: Ok(Box::new(saved)),
    });
    let said = app.toast().map(|t| t.text.clone()).expect("a toast");
    assert_eq!(
        said,
        "SADIE II, D1 is ready; the oldest unused set went to make room"
    );
}

#[test]
fn a_damaged_set_of_the_persons_own_is_to_be_imported_again_not_fetched() {
    let mut app = open(true);
    let mut sets = catalogue(true);
    sets.sets[1].own = true;
    sets.sets[1].damaged = Some("its file is missing".into());
    app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(sets))));
    move_to(&mut app, "sadie-d1");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.speakers_box.fetch.is_none());
    assert!(
        app.toast()
            .is_some_and(|t| t.text.contains("import the file again"))
    );
}

/// The sets of the view after another front end fetched `sadie-d1`.
const AFTER_FETCH: [(&str, &str, &str); 2] = [
    ("sadie-d2", "no", "SADIE II, D2"),
    ("sadie-d1", "no", "SADIE II, D1"),
];

#[test]
fn a_change_of_the_views_sets_while_the_list_is_open_asks_for_the_catalogue_again() {
    let mut app = open(true);
    assert_eq!(
        app.owed_sets_call(),
        None,
        "nothing owed after the box's own ask"
    );
    // a change that leaves the sets as they were owes nothing
    app.host.computer.eq = Some(computer_eq_with_speakers(false, Some("sadie-d2"), &FETCHED));
    app.note_computer_view();
    assert_eq!(app.owed_sets_call(), None);
    app.host.computer.eq = Some(computer_eq_with_speakers(
        true,
        Some("sadie-d2"),
        &AFTER_FETCH,
    ));
    app.note_computer_view();
    assert_eq!(app.owed_sets_call(), Some(SpeakersCall::Catalogue));
    assert_eq!(app.owed_sets_call(), None, "owed once");
    // with the list closed nothing is asked
    let mut app = open(true);
    press(&mut app, KeyCode::Esc);
    app.host.computer.eq = Some(computer_eq_with_speakers(
        true,
        Some("sadie-d2"),
        &AFTER_FETCH,
    ));
    app.note_computer_view();
    assert_eq!(app.owed_sets_call(), None);
}

#[test]
fn a_set_fetched_elsewhere_is_chosen_not_fetched_again_before_the_catalogue_answers() {
    let mut app = open(true);
    app.host.computer.eq = Some(computer_eq_with_speakers(
        true,
        Some("sadie-d2"),
        &AFTER_FETCH,
    ));
    app.note_computer_view();
    move_to(&mut app, "sadie-d1");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        [Intent::eq(HostEqOp::Speakers {
            on: Some(true),
            set: Some("sadie-d1".into()),
            done: true,
        })]
    );
    assert!(app.speakers_box.fetch.is_none(), "no download");
}

#[test]
fn the_set_that_plays_is_refused_in_the_daemons_words_and_a_forget_is_told_by_label() {
    let mut app = open(true);
    press(&mut app, KeyCode::Char('f'));
    assert!(
        app.toast()
            .is_some_and(|t| t.text == svolder_host::eq::speakers::plays_now_words("SADIE II, D2"))
    );
    let next = app.speakers_done(SpeakersDone::Forgotten {
        id: "sadie-d2".into(),
        result: Ok(()),
    });
    assert_eq!(next, [Intent::speakers(SpeakersCall::Catalogue)]);
    assert!(
        app.toast()
            .is_some_and(|t| t.text == "SADIE II, D2 forgotten"),
        "{:?}",
        app.toast().map(|t| t.text.clone())
    );
}

#[test]
fn with_fetching_off_the_catalogues_own_words_are_said() {
    let mut app = app(true);
    app.open_sets();
    let mut off = catalogue(false);
    off.note = Some("fetching sets is off: the sets' base address is the discard port".into());
    app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(off))));
    move_to(&mut app, "sadie-d1");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.toast().is_some_and(
        |t| t.text == "fetching sets is off: the sets' base address is the discard port"
    ));
}
