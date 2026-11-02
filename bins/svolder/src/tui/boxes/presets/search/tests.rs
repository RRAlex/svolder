// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use svolder_autoeq::Entry as Measurement;
use svolder_profiles::EqMode;

use crate::tui::boxes::presets::{Done, Step, done};
use crate::tui::test_support::presets::{
    PEQ, imported, index_of, resting, searching, step, two_headphones,
};
use crate::tui::test_support::{press_from as press, type_text};

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
        .map(|entry| format!("{} | {}", entry.name, measurement(entry)))
        .collect()
}

/// `a` opens the box and asks for the index when the session has none; with one it opens at
/// once and asks for nothing. Neither writes anything.
#[test]
fn a_opens_the_search_box_and_fetches_the_index_only_once() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    press(&mut app, KeyCode::Char('p'));
    let intents = press(&mut app, KeyCode::Char('a'));
    assert_eq!(intents, [Intent::AutoEqIndex { refresh: false }]);
    assert_eq!(search_of(&app).load, Load::Fetching);
    assert_eq!(status(search_of(&app), None), "fetching the index…");

    // What is typed while the index is on its way waits for it.
    type_text(&mut app, "hd 6");
    assert!(search_of(&app).rows.is_empty());
    assert!(done(&mut app, Done::Index(Ok(two_headphones()))).is_empty());
    assert_eq!(search_of(&app).load, Load::Ready);
    assert_eq!(rows(&app).len(), 3, "the typed query was not applied");

    press(&mut app, KeyCode::Esc);
    assert_eq!(step(&app), Some(&Step::Choosing));
    let intents = press(&mut app, KeyCode::Char('a'));
    assert!(intents.is_empty(), "the held index was fetched again");
    assert_eq!(search_of(&app).load, Load::Ready);
}

/// Typing filters the index live, the recommended measurement first within a headphone; `↑↓`
/// move and stop at the ends; `Backspace` widens the query again. Every letter, `a` and `n`
/// among them, is typed rather than taken as a key.
#[test]
fn typing_filters_the_index_and_the_arrows_move() {
    let mut app = searching(two_headphones());
    assert!(
        search_of(&app).rows.is_empty(),
        "nothing typed, nothing listed"
    );
    type_text(&mut app, "hd 6");
    assert_eq!(rows(&app).len(), 3);
    type_text(&mut app, "5");
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
    press(&mut app, KeyCode::Backspace);
    assert_eq!(search_of(&app).query, "hd 6");
    assert_eq!(rows(&app).len(), 3);

    for letter in "anuxh".chars() {
        press(&mut app, KeyCode::Char(letter));
    }
    assert_eq!(search_of(&app).query, "hd 6anuxh");
    assert!(search_of(&app).rows.is_empty());
}

/// `Enter` on a row asks for that measurement's file, once; the file's text imports it as a
/// preset that carries AutoEq's origin, written but not loaded, with its row focused.
#[test]
fn enter_downloads_the_focused_measurement_and_imports_it_unloaded() {
    let mut app = searching(two_headphones());
    type_text(&mut app, "hd650");
    press(&mut app, KeyCode::Down);
    let intents = press(&mut app, KeyCode::Enter);
    let [Intent::AutoEqDownload(entry)] = intents.as_slice() else {
        panic!("expected one download, got {intents:?}");
    };
    assert_eq!(entry.source, "crinacle");
    assert_eq!(
        status(search_of(&app), app.presets.autoeq.as_deref()),
        "downloading Sennheiser HD 650…"
    );
    assert!(
        press(&mut app, KeyCode::Enter).is_empty(),
        "a second download started"
    );

    let entry = entry.clone();
    let intents = done(&mut app, Done::Download(entry, Ok(PEQ.to_string())));
    let [Intent::SavePreset(saved)] = intents.as_slice() else {
        panic!("expected one preset to be saved, got {intents:?}");
    };
    assert_eq!(saved.name, "Sennheiser HD 650");
    let source = saved.source.as_ref().expect("the preset is imported");
    assert_eq!(
        source.origin.as_deref(),
        Some("AutoEq · crinacle · over-ear · GRAS 43AG-7")
    );
    assert_eq!(source.correction.filters.len(), 2);
    assert_eq!(step(&app), Some(&Step::Choosing));
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
}

/// A name a preset carries already opens the name prompt, prefilled `NAME (source)`; `Enter`
/// saves under it, and a name still taken is refused with the prompt up.
#[test]
fn an_existing_name_opens_the_prompt_prefilled_with_the_source() {
    let mut app = searching(two_headphones());
    app.presets.saved = vec![imported("Sennheiser HD 600")];
    type_text(&mut app, "hd600");
    let intents = press(&mut app, KeyCode::Enter);
    let [Intent::AutoEqDownload(entry)] = intents.as_slice() else {
        panic!("expected one download, got {intents:?}");
    };
    let entry = entry.clone();
    assert!(done(&mut app, Done::Download(entry, Ok(PEQ.to_string()))).is_empty());
    let Some(Step::Naming(line)) = step(&app) else {
        panic!("the name prompt did not open: {:?}", step(&app));
    };
    assert_eq!(line.text, "Sennheiser HD 600 (oratory1990)");
    assert!(line.import.is_some());

    let intents = press(&mut app, KeyCode::Enter);
    let [Intent::SavePreset(saved)] = intents.as_slice() else {
        panic!("expected one preset to be saved, got {intents:?}");
    };
    assert_eq!(saved.name, "Sennheiser HD 600 (oratory1990)");
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// Taking the same name in the prompt is refused as any rename is, and `Esc` drops the import.
#[test]
fn the_import_name_prompt_refuses_a_taken_name_and_esc_drops_the_import() {
    let mut app = searching(two_headphones());
    app.presets.saved = vec![
        imported("Sennheiser HD 600"),
        imported("Sennheiser HD 600 (oratory1990)"),
    ];
    type_text(&mut app, "hd600");
    let intents = press(&mut app, KeyCode::Enter);
    let Intent::AutoEqDownload(entry) = &intents[0] else {
        panic!("expected a download");
    };
    let entry = entry.clone();
    done(&mut app, Done::Download(entry, Ok(PEQ.to_string())));
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        matches!(step(&app), Some(Step::Naming(_))),
        "the prompt closed"
    );
    press(&mut app, KeyCode::Esc);
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// A failed download, and a file that is not a correction, are toasts with the box still up; a
/// result that comes after `Esc` is dropped.
#[test]
fn a_failed_download_or_a_bad_file_is_a_toast_and_a_late_result_is_dropped() {
    let mut app = searching(two_headphones());
    type_text(&mut app, "hd650");
    let intents = press(&mut app, KeyCode::Enter);
    let Intent::AutoEqDownload(entry) = &intents[0] else {
        panic!("expected a download");
    };
    let entry: Measurement = entry.clone();
    done(
        &mut app,
        Done::Download(entry.clone(), Err("no network".into())),
    );
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("no network".to_string())
    );
    assert!(search_of(&app).downloading.is_none());

    press(&mut app, KeyCode::Enter);
    done(
        &mut app,
        Done::Download(entry.clone(), Ok("not a filter".into())),
    );
    let said = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(
        said.starts_with("the file for Sennheiser HD 650 is not a parametric EQ"),
        "{said}"
    );
    assert!(matches!(step(&app), Some(Step::Searching(_))));

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    assert!(done(&mut app, Done::Download(entry, Ok(PEQ.to_string()))).is_empty());
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// `Ctrl-R` asks for the index again whatever its age, and only in the search box; a failed
/// fetch is the status line, and a refresh that fell back on the cache says so in a toast.
#[test]
fn ctrl_r_refreshes_the_index_and_the_status_line_says_where_it_stands() {
    let mut app = searching(two_headphones());
    let held = app.presets.autoeq.clone();
    assert_eq!(
        status(search_of(&app), held.as_deref()),
        "index from 2026-07-01 · 3 measurements"
    );
    let ctrl_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    let intents = app.key(ctrl_r);
    assert_eq!(intents, [Intent::AutoEqIndex { refresh: true }]);
    assert_eq!(
        status(search_of(&app), held.as_deref()),
        "fetching the index…"
    );

    done(
        &mut app,
        Done::Index(Err("could not fetch the index".into())),
    );
    assert_eq!(
        status(search_of(&app), held.as_deref()),
        "could not fetch the index"
    );
    assert_eq!(
        search_of(&app).load,
        Load::Failed("could not fetch the index".into())
    );

    let mut fresh = two_headphones();
    fresh.note = Some("could not refresh the index; using the copy from 2026-07-01".into());
    done(&mut app, Done::Index(Ok(fresh)));
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

/// A query no headphone matches lists nothing and says so on the box.
#[test]
fn a_query_with_no_match_lists_nothing() {
    let mut app = searching(index_of(&[]));
    type_text(&mut app, "zzz");
    assert!(search_of(&app).rows.is_empty());
    assert!(press(&mut app, KeyCode::Enter).is_empty());
}
