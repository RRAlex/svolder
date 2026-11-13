// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use svolder_ipc::{AutoEqSaved, AutoEqTarget};
use svolder_profiles::EqMode;

use crate::tui::boxes::presets::{Done, Step, done};
use crate::tui::test_support::presets::{
    answer, imported, last_ask, resting, search_typing, searching, step, two_headphones,
};
use crate::tui::test_support::press_from as press;

use super::*;

/// The search box, which the step must be.
fn search_of(app: &App) -> &SearchBox {
    match step(app) {
        Some(Step::Searching(search)) => search,
        other => panic!("the list is not searching: {other:?}"),
    }
}

/// The names of the rows the box shows, with their sources.
fn rows(app: &App) -> Vec<String> {
    search_of(app)
        .rows
        .iter()
        .map(|row| format!("{} | {}", row.variant.name, measurement(row)))
        .collect()
}

/// What `answer` says the daemon saved.
fn saved(name: &str) -> AutoEqSaved {
    AutoEqSaved {
        name: name.into(),
        filters: 2,
        skipped: 0,
    }
}

/// `a` opens the box and asks the daemon at once, with the empty query, so it loads its index;
/// the box says it is fetching until the answer, and says where the index is from after it. The
/// box asks again each time it opens: the daemon holds the index, the TUI does not.
#[test]
fn a_opens_the_search_box_and_asks_the_daemon_to_load_its_index() {
    let entries = two_headphones();
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    press(&mut app, KeyCode::Char('p'));
    let intents = press(&mut app, KeyCode::Char('a'));
    let first = last_ask(&intents);
    assert_eq!(
        intents,
        [Intent::AutoEqSearch {
            ask: first.clone(),
            soon: false
        }]
    );
    assert_eq!((first.query.as_str(), first.refresh), ("", false));
    assert_eq!(search_of(&app).load, Load::Fetching);
    assert_eq!(status(search_of(&app)), "fetching the index…");

    assert!(answer(&mut app, &entries, &first).is_empty());
    assert_eq!(search_of(&app).load, Load::Ready);
    assert_eq!(
        status(search_of(&app)),
        "index from 2026-07-01 · 3 measurements"
    );

    press(&mut app, KeyCode::Esc);
    assert_eq!(step(&app), Some(&Step::Choosing));
    let again = last_ask(&press(&mut app, KeyCode::Char('a')));
    assert!(again.seq > first.seq, "the question was not numbered anew");
    assert_eq!(search_of(&app).load, Load::Fetching);
}

/// Each key asks a question that waits out the typing (`soon`), numbered in order; nothing is
/// listed until the answer comes; the answer's rows are grouped by headphone with the
/// recommended measurement first, `↑↓` move and stop at the ends, and `Backspace` asks for the
/// wider query again. Every letter, `a` and `n` among them, is typed rather than taken as a key.
#[test]
fn typing_asks_the_daemon_and_the_answers_fill_the_rows() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    assert!(
        search_of(&app).rows.is_empty(),
        "nothing typed, nothing listed"
    );
    let intents = search_typing(&mut app, &entries, "hd 6");
    let asks: Vec<&Ask> = intents
        .iter()
        .map(|intent| match intent {
            Intent::AutoEqSearch { ask, soon: true } => ask,
            other => panic!("expected a question that waits for the typing, got {other:?}"),
        })
        .collect();
    assert_eq!(
        asks.iter()
            .map(|ask| ask.query.as_str())
            .collect::<Vec<_>>(),
        ["h", "hd", "hd ", "hd 6"]
    );
    assert!(asks.windows(2).all(|pair| pair[0].seq < pair[1].seq));
    assert_eq!(rows(&app).len(), 3);

    let typed = press(&mut app, KeyCode::Char('5'));
    assert!(search_of(&app).waiting, "an answer is owed after a key");
    assert_eq!(rows(&app).len(), 3, "the old rows stay until the answer");
    answer(&mut app, &entries, &last_ask(&typed));
    assert!(!search_of(&app).waiting);
    assert_eq!(
        rows(&app),
        [
            "Sennheiser HD 650 | oratory1990 · over-ear",
            "Sennheiser HD 650 | crinacle · over-ear · GRAS 43AG-7"
        ]
    );
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(search_of(&app).row, 1, "the focus passed the last row");
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    assert_eq!(search_of(&app).row, 0);
    let wider = press(&mut app, KeyCode::Backspace);
    assert_eq!(search_of(&app).query, "hd 6");
    answer(&mut app, &entries, &last_ask(&wider));
    assert_eq!(rows(&app).len(), 3);

    for letter in "anuxh".chars() {
        press(&mut app, KeyCode::Char(letter));
    }
    assert_eq!(search_of(&app).query, "hd 6anuxh");
}

/// An empty query is asked about too, so the status line stays current, and lists nothing at
/// once.
#[test]
fn clearing_the_query_lists_nothing_and_asks_again() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    let intents = search_typing(&mut app, &entries, "h");
    assert!(!intents.is_empty());
    assert!(!search_of(&app).rows.is_empty());
    let cleared = press(&mut app, KeyCode::Backspace);
    assert!(
        search_of(&app).rows.is_empty(),
        "the rows outlived the query"
    );
    assert_eq!(last_ask(&cleared).query, "");
    // Backspace on an empty line changes nothing and asks nothing.
    answer(&mut app, &entries, &last_ask(&cleared));
    assert!(press(&mut app, KeyCode::Backspace).is_empty());
}

/// An answer for a question that is not the box's last is dropped, in either order: a slow
/// answer to an old question that comes after a newer question's answer shows nothing, and one
/// that comes before it is replaced by it.
#[test]
fn an_answer_for_an_older_question_is_dropped_in_either_order() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    let first = last_ask(&press(&mut app, KeyCode::Char('h')));
    let second = last_ask(&press(&mut app, KeyCode::Char('d')));

    // The newer answer first, then the older one: the older is dropped.
    answer(&mut app, &entries, &second);
    let shown = rows(&app);
    assert!(!shown.is_empty());
    let stale = crate::test_support::autoeq_script::answer_of(&entries, "zzz");
    assert!(done(&mut app, Done::Searched(first.clone(), Ok(stale))).is_empty());
    assert_eq!(rows(&app), shown, "the older answer replaced the newer one");
    assert!(!search_of(&app).waiting);

    // The older answer first: it is dropped, and the box keeps waiting for the newer one.
    let mut app = searching(&entries);
    let first = last_ask(&press(&mut app, KeyCode::Char('h')));
    let second = last_ask(&press(&mut app, KeyCode::Char('d')));
    answer(&mut app, &entries, &first);
    assert!(
        search_of(&app).rows.is_empty(),
        "the older answer was shown"
    );
    assert!(search_of(&app).waiting, "the newer answer is still owed");
    answer(&mut app, &entries, &second);
    assert!(!search_of(&app).rows.is_empty());

    // A failure of an older question is dropped the same way.
    let mut app = searching(&entries);
    let first = last_ask(&press(&mut app, KeyCode::Char('h')));
    press(&mut app, KeyCode::Char('d'));
    done(&mut app, Done::Searched(first, Err("old failure".into())));
    assert_eq!(search_of(&app).load, Load::Ready);
}

/// An answer that arrives after the box was closed, or after it was closed and opened again, is
/// dropped: a switch of device or side closes the list, so no answer lands on the other one's.
#[test]
fn an_answer_after_the_box_closed_or_opened_again_is_dropped() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    let old = last_ask(&press(&mut app, KeyCode::Char('h')));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert!(app.presets.preset_list.is_none());
    assert!(answer(&mut app, &entries, &old).is_empty());
    assert!(app.presets.preset_list.is_none());

    // Opened again: the new box's own question is the only one it takes.
    press(&mut app, KeyCode::Char('p'));
    let fresh = last_ask(&press(&mut app, KeyCode::Char('a')));
    answer(&mut app, &entries, &old);
    assert_eq!(search_of(&app).load, Load::Fetching);
    answer(&mut app, &entries, &fresh);
    assert_eq!(search_of(&app).load, Load::Ready);
}

/// A failure is the status line with the daemon's words, with no rows; a refresh that fell back
/// on the cached copy says so in a toast.
#[test]
fn a_failed_question_is_the_status_line_and_a_fallback_is_a_toast() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    let typed = press(&mut app, KeyCode::Char('h'));
    done(
        &mut app,
        Done::Searched(last_ask(&typed), Err("could not fetch the index".into())),
    );
    assert_eq!(status(search_of(&app)), "could not fetch the index");
    assert_eq!(
        search_of(&app).load,
        Load::Failed("could not fetch the index".into())
    );

    let ctrl_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    let intents = app.key(ctrl_r);
    let refresh = last_ask(&intents);
    assert!(refresh.refresh);
    assert_eq!(refresh.query, "h");
    assert_eq!(
        intents,
        [Intent::AutoEqSearch {
            ask: refresh.clone(),
            soon: false
        }],
        "a refresh waits for no typing"
    );
    assert_eq!(status(search_of(&app)), "fetching the index…");
    let mut found = crate::test_support::autoeq_script::answer_of(&entries, "h");
    found.note = Some("could not refresh the index; using the copy from 2026-07-01".into());
    done(&mut app, Done::Searched(refresh, Ok(found)));
    assert_eq!(search_of(&app).load, Load::Ready);
    assert!(
        app.toast()
            .is_some_and(|t| t.text.starts_with("could not refresh"))
    );

    press(&mut app, KeyCode::Esc);
    assert!(
        app.key(ctrl_r).is_empty(),
        "Ctrl-R did something on the list"
    );
}

/// The daemon's cap on the headphones it answers with is said on the status line.
#[test]
fn headphones_past_the_daemons_cap_are_said() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    let typed = press(&mut app, KeyCode::Char('h'));
    let mut found = crate::test_support::autoeq_script::answer_of(&entries, "h");
    found.more = 12;
    done(&mut app, Done::Searched(last_ask(&typed), Ok(found)));
    assert_eq!(
        status(search_of(&app)),
        "index from 2026-07-01 · 3 measurements · 12 more headphones: narrow it"
    );
}

/// `Enter` waits for the answer to the query on the line: until it comes the rows may be for
/// another query. Then it asks the daemon to import the focused measurement, once, for the
/// device on show, under the headphone's own name; the daemon's answer has the presets read
/// again, which says it in a toast and focuses the new row. The preset is saved, not loaded.
#[test]
fn enter_has_the_daemon_import_the_focused_measurement_unloaded() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    let typed = press(&mut app, KeyCode::Char('h'));
    assert!(
        press(&mut app, KeyCode::Enter).is_empty(),
        "an import started on rows that wait for an answer"
    );
    answer(&mut app, &entries, &last_ask(&typed));
    search_typing(&mut app, &entries, "d650");
    press(&mut app, KeyCode::Down);
    let intents = press(&mut app, KeyCode::Enter);
    let [
        Intent::AutoEqImport {
            variant,
            name,
            target,
        },
    ] = intents.as_slice()
    else {
        panic!("expected one import, got {intents:?}");
    };
    assert_eq!(variant.source, "crinacle");
    assert_eq!(name, "Sennheiser HD 650");
    assert_eq!(
        target,
        &AutoEqTarget::Device {
            device: app.session.device_name.clone()
        }
    );
    assert_eq!(status(search_of(&app)), "importing Sennheiser HD 650…");
    assert!(
        press(&mut app, KeyCode::Enter).is_empty(),
        "a second import started"
    );

    let intents = done(
        &mut app,
        Done::Imported("Sennheiser HD 650".into(), Ok(saved("Sennheiser HD 650"))),
    );
    assert_eq!(intents, [Intent::ReadPresets]);
    assert_eq!(step(&app), Some(&Step::Choosing));
    // The loop reads the file the daemon wrote; the list then shows the preset and says it.
    let mut kept = imported("Sennheiser HD 650");
    if let Some(source) = kept.source.as_mut() {
        source.origin = Some("AutoEq · crinacle · over-ear · GRAS 43AG-7".into());
    }
    crate::tui::boxes::presets::reread(&mut app, vec![kept]);
    assert_eq!(
        app.presets.preset_list.as_ref().map(|list| list.row),
        Some(4),
        "the new row is not focused"
    );
    assert_eq!(app.presets.loaded_preset, None, "the import was loaded");
    assert_eq!(
        app.toast().map(|toast| toast.text.clone()),
        Some("imported \"Sennheiser HD 650\" (2 filters, preamp -3.0 dB)".to_string())
    );
    assert_eq!(app.presets.landed, None);
}

/// A name a preset carries already opens the name prompt, prefilled `NAME (source)`; `Enter`
/// has the daemon import under it, and a name still taken is refused with the prompt up.
#[test]
fn an_existing_name_opens_the_prompt_prefilled_with_the_source() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    app.presets.saved = vec![imported("Sennheiser HD 600")];
    search_typing(&mut app, &entries, "hd600");
    let intents = press(&mut app, KeyCode::Enter);
    assert!(intents.is_empty(), "the daemon was asked with a taken name");
    let Some(Step::Naming(line)) = step(&app) else {
        panic!("the name prompt did not open: {:?}", step(&app));
    };
    assert_eq!(line.text, "Sennheiser HD 600 (oratory1990)");
    assert!(line.import.is_some());

    let intents = press(&mut app, KeyCode::Enter);
    let [Intent::AutoEqImport { variant, name, .. }] = intents.as_slice() else {
        panic!("expected one import, got {intents:?}");
    };
    assert_eq!(variant.name, "Sennheiser HD 600");
    assert_eq!(name, "Sennheiser HD 600 (oratory1990)");
    assert_eq!(step(&app), Some(&Step::Choosing));
    assert_eq!(
        app.toast().map(|toast| toast.text.clone()),
        Some("importing \"Sennheiser HD 600 (oratory1990)\"…".to_string())
    );
}

/// Taking the same name in the prompt is refused as any rename is, and `Esc` drops the import.
#[test]
fn the_import_name_prompt_refuses_a_taken_name_and_esc_drops_the_import() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    app.presets.saved = vec![
        imported("Sennheiser HD 600"),
        imported("Sennheiser HD 600 (oratory1990)"),
    ];
    search_typing(&mut app, &entries, "hd600");
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        matches!(step(&app), Some(Step::Naming(_))),
        "the prompt closed"
    );
    press(&mut app, KeyCode::Esc);
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// A refused import is a toast in the daemon's words with the box still up and free to try
/// another; an import that ends after `Esc` still has the presets read again, since the daemon
/// saved the preset whatever the box shows by then.
#[test]
fn a_refused_import_is_a_toast_and_one_that_ends_after_esc_still_lands() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    search_typing(&mut app, &entries, "hd650");
    press(&mut app, KeyCode::Enter);
    let intents = done(
        &mut app,
        Done::Imported(
            "Sennheiser HD 650".into(),
            Err("Sennheiser HD 650 is listed but its file is not there".into()),
        ),
    );
    assert!(intents.is_empty());
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("Sennheiser HD 650 is listed but its file is not there".to_string())
    );
    assert!(search_of(&app).importing.is_none());

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    let intents = done(
        &mut app,
        Done::Imported("Sennheiser HD 650".into(), Ok(saved("Sennheiser HD 650"))),
    );
    assert_eq!(intents, [Intent::ReadPresets]);
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// A query no headphone matches lists nothing and says so on the box.
#[test]
fn a_query_with_no_match_lists_nothing() {
    let mut app = searching(&[]);
    search_typing(&mut app, &[], "zzz");
    assert!(search_of(&app).rows.is_empty());
    assert!(press(&mut app, KeyCode::Enter).is_empty());
}

/// An import's answer resets only the box that asked for it: after `Esc` and a new box with an
/// import of its own under way, the older import's answer leaves the new one importing.
#[test]
fn an_older_imports_answer_leaves_a_reopened_boxes_import_running() {
    let entries = two_headphones();
    let mut app = searching(&entries);
    search_typing(&mut app, &entries, "hd650");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);

    press(&mut app, KeyCode::Char('p'));
    let fresh = last_ask(&press(&mut app, KeyCode::Char('a')));
    answer(&mut app, &entries, &fresh);
    app.presets.saved = vec![imported("Sennheiser HD 650")];
    search_typing(&mut app, &entries, "hd650");
    let started = press(&mut app, KeyCode::Enter);
    assert!(started.is_empty(), "the name is taken: the prompt opens");
    press(&mut app, KeyCode::Esc);
    app.presets.saved.clear();
    press(&mut app, KeyCode::Char('a'));
    let again = last_ask(&press(&mut app, KeyCode::Char('h')));
    answer(&mut app, &entries, &again);
    search_typing(&mut app, &entries, "d600");
    let intents = press(&mut app, KeyCode::Enter);
    assert!(matches!(intents.as_slice(), [Intent::AutoEqImport { .. }]));
    assert_eq!(status(search_of(&app)), "importing Sennheiser HD 600…");

    // The first box's import ends now.
    done(
        &mut app,
        Done::Imported("Sennheiser HD 650".into(), Ok(saved("Sennheiser HD 650"))),
    );
    assert_eq!(status(search_of(&app)), "importing Sennheiser HD 600…");
    assert!(matches!(step(&app), Some(Step::Searching(_))));
}
