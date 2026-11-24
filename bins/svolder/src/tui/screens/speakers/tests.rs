// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_ipc::{SpeakersCatalogue, SpeakersRoom, SpeakersSet};

use crate::tui::app::{App, Screen, ToneRow, Xf};
use crate::tui::boxes::speakers::{Fetch, SpeakersDone};
use crate::tui::test_support::{app_showing, computer_eq_with_speakers, frame, text};

const FETCHED: [(&str, &str, &str); 2] = [
    ("sadie-d2", "no", "SADIE II, D2"),
    ("room-1", "with", "Control room 1"),
];

fn app_on(screen: Screen, on: bool) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_speakers(on, Some("sadie-d2"), &FETCHED));
    app.flip_side();
    app.session.daemon = true;
    if screen == Screen::Tone {
        app.show(Screen::Tone);
        app.tone.row = ToneRow::Mode;
    }
    app
}

fn set(id: &str, room: SpeakersRoom, label: &str, fetched: bool, bytes: u64) -> SpeakersSet {
    SpeakersSet {
        id: id.to_string(),
        room,
        label: label.to_string(),
        licence: "Apache-2.0".to_string(),
        credit: "sample credit words".to_string(),
        source_url: "https://sofacoustics.example/data/sadie/".to_string(),
        bytes,
        own: false,
        fetched,
        damaged: None,
        angles: None,
        // a cut set is as long as the cap it was cut to
        taps_ms: if room == SpeakersRoom::With {
            500.0
        } else {
            0.0
        },
        rate: 0,
        loudness_db: 0.0,
        cut: room == SpeakersRoom::With,
    }
}

fn catalogue() -> SpeakersCatalogue {
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
                false,
                43_000_000,
            ),
        ],
        fetching: true,
        note: None,
        max_sets: 32,
    }
}

fn open(width: u16, height: u16) -> (App, Vec<String>) {
    let mut app = app_on(Screen::Tone, true);
    app.open_sets();
    app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(catalogue()))));
    let rows = text(&frame(&app, width, height));
    (app, rows)
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

#[test]
fn the_list_shows_every_set_with_its_choice_state_and_licence() {
    for (width, height) in [(80, 24), (120, 36)] {
        let (_, rows) = open(width, height);
        for needle in [
            "Virtual speakers sets",
            "Filter",
            "SADIE II, D2",
            "No room \u{b7} HRTF",
            "With room \u{b7} BRIR",
            "chosen",
            "35 MB",
            "43 MB",
            "Apache-2.0",
            "3 sets \u{b7} 1 fetched",
            "Enter fetch/use",
        ] {
            assert!(
                has(&rows, needle),
                "{width}x{height}: {needle:?} in\n{}",
                rows.join("\n")
            );
        }
    }
}

#[test]
fn the_focused_sets_licence_credit_and_source_stand_under_the_rows() {
    let (mut app, _) = open(80, 24);
    app.speakers_box.list.as_mut().expect("list").row = 2;
    let rows = text(&frame(&app, 80, 24));
    assert!(
        has(&rows, "Apache-2.0 \u{b7} sample credit words"),
        "{}",
        rows.join("\n")
    );
    assert!(
        has(
            &rows,
            "sofacoustics.example/data/sadie/ \u{b7} cut to 0.5 s"
        ),
        "{}",
        rows.join("\n")
    );
}

#[test]
fn a_fetch_draws_its_progress_on_its_row_and_in_the_status_line() {
    let (mut app, _) = open(80, 24);
    app.speakers_box.fetch = Some(Fetch {
        id: "room-1".into(),
        label: "Control room 1".into(),
        stop: crate::tui::boxes::speakers::Stop::new(),
        received: 26_660_000,
        total: 43_000_000,
        import: false,
        stopping: false,
    });
    let rows = text(&frame(&app, 80, 24));
    let row = rows
        .iter()
        .find(|r| r.contains("Control room 1") && r.contains("62%"))
        .unwrap_or_else(|| panic!("the row in\n{}", rows.join("\n")));
    assert!(
        row.contains('\u{2588}') && row.contains('\u{2591}'),
        "{row}"
    );
    assert!(
        has(&rows, "fetching Control room 1, 27 MB of 43 MB"),
        "{}",
        rows.join("\n")
    );
}

#[test]
fn the_filter_row_names_the_filter_and_the_choices_with_their_terms() {
    let (mut app, _) = open(80, 24);
    app.speakers_box.list.as_mut().expect("list").filter =
        crate::tui::boxes::speakers::Filter::With;
    let rows = text(&frame(&app, 80, 24));
    let filter = rows.iter().find(|r| r.contains("Filter")).expect("the row");
    assert!(
        filter.contains("\u{2039} With room \u{b7} BRIR \u{203a}"),
        "{filter}"
    );
    assert!(filter.contains("Tab: All"), "{filter}");
    // the folded Space line under the box names the chosen set; the list has no row for it
    assert!(
        !rows
            .iter()
            .any(|r| r.contains("SADIE II, D2") && !r.contains("Space")),
        "only the sets with a room"
    );
}

#[test]
fn the_file_prompt_and_the_forget_question_are_boxes_of_their_own() {
    let (mut app, _) = open(80, 24);
    app.sets_key(crossterm::event::KeyCode::Char('i'));
    let rows = text(&frame(&app, 80, 24));
    assert!(has(&rows, "Import a file"), "{}", rows.join("\n"));
    assert!(has(&rows, "~/"));
    assert!(has(&rows, "No room \u{b7} HRTF  (a SOFA file or a WAV)"));
    assert!(has(&rows, "Tab room"));
    app.sets_key(crossterm::event::KeyCode::Esc);
    app.speakers_box.list.as_mut().expect("list").row = 0;
    // the chosen set plays now: no question; move to a fetched one that does not
    app.host.computer.eq = Some(computer_eq_with_speakers(false, Some("sadie-d2"), &FETCHED));
    app.sets_key(crossterm::event::KeyCode::Char('f'));
    let rows = text(&frame(&app, 80, 24));
    assert!(has(&rows, "Forget SADIE II, D2?"), "{}", rows.join("\n"));
}

#[test]
fn with_no_daemon_the_status_says_fetching_needs_it() {
    let mut app = app_on(Screen::Tone, true);
    app.session.daemon = false;
    app.open_sets();
    let rows = text(&frame(&app, 80, 24));
    assert!(has(&rows, "fetching needs svolderd"), "{}", rows.join("\n"));
    assert!(has(&rows, "2 sets \u{b7} 2 fetched"));
}

#[test]
fn the_device_screens_tone_pane_has_the_space_row_and_its_two_lines() {
    for (width, height) in [(80, 24), (120, 36)] {
        let app = app_on(Screen::Device, true);
        let rows = text(&frame(&app, width, height));
        let at = |needle: &str| rows.iter().position(|r| r.contains(needle));
        let space = at("\u{2039} Virtual speakers \u{203a}")
            .unwrap_or_else(|| panic!("{width}x{height}\n{}", rows.join("\n")));
        assert!(rows[space].contains("Space"), "{}", rows[space]);
        assert!(
            rows[space + 1].contains("No room \u{b7} HRTF \u{b7} SADIE II, D2"),
            "{}",
            rows[space + 1]
        );
        assert!(
            rows[space + 2].contains("7 dB quieter \u{b7} 5.3 ms"),
            "{}",
            rows[space + 2]
        );
        assert!(
            space > at("Type").expect("the table"),
            "under the points table"
        );
    }
}

#[test]
fn the_pane_row_for_a_space_that_is_off_has_no_detail_lines() {
    let app = app_on(Screen::Device, false);
    let rows = text(&frame(&app, 80, 24));
    let space = rows
        .iter()
        .position(|r| r.contains("\u{2039} Off \u{203a}"))
        .unwrap_or_else(|| panic!("{}", rows.join("\n")));
    assert!(rows[space].contains("Space"));
    assert!(!rows[space + 1].contains("HRTF"), "{}", rows[space + 1]);
}

#[test]
fn a_click_on_the_pane_rows_arrow_steps_the_choice() {
    use crate::tui::mouse::{HitMap, MouseAction, resolve};
    use crate::tui::test_support::click;
    use ratatui::{Terminal, backend::TestBackend};
    let app = app_on(Screen::Device, false);
    let theme = crate::tui::theme::Theme::new(false);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a terminal");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            crate::tui::render::draw(f, &app, &theme, &mut hits);
        })
        .expect("the frame draws");
    let rows = text(terminal.backend().buffer());
    let y = rows
        .iter()
        .position(|r| r.contains("\u{2039} Off \u{203a}"))
        .expect("the Space row");
    let x = rows[y]
        .chars()
        .position(|c| c == '\u{203a}')
        .expect("the arrow");
    let action = resolve(
        &hits,
        click(
            u16::try_from(x).expect("a column"),
            u16::try_from(y).expect("a row"),
        ),
        &app,
    );
    let Some(MouseAction::Crossfeed { row, step }) = action else {
        panic!("a step of the Space row, got {action:?}");
    };
    assert_eq!((row, step), (Xf::Space, 1));
    let mut app = app;
    // a click on the arrow only highlights; Enter plays the highlighted choice
    assert!(app.click_crossfeed(row, step).is_empty());
    let sent = app.plays_apply();
    assert!(matches!(
        sent.as_slice(),
        [crate::tui::app::Intent::HostEq(op)]
            if matches!(**op, svolder_ipc::HostEqOp::Crossfeed { on: Some(true), .. })
    ));
}

#[test]
fn after_x_the_row_and_the_status_say_the_fetch_is_stopping() {
    let (mut app, _) = open(80, 24);
    app.speakers_box.fetch = Some(Fetch {
        id: "room-1".into(),
        label: "Control room 1".into(),
        stop: crate::tui::boxes::speakers::Stop::new(),
        received: 1_000_000,
        total: 43_000_000,
        import: false,
        stopping: false,
    });
    app.speakers_box.list.as_mut().expect("list").row = 2;
    app.sets_key(crossterm::event::KeyCode::Char('x'));
    let rows = text(&frame(&app, 80, 24));
    assert!(has(&rows, "stopping Control room 1"), "{}", rows.join("\n"));
    assert!(
        rows.iter()
            .any(|r| r.contains("Control room 1") && r.contains("stopping"))
    );
}

/// The looking stills of the Virtual speakers screens as text, for an eye check (and for the
/// docs' pictures): `target/screenshots/speakers/<name>.txt`. By hand:
/// `cargo test -p svolder --bin svolder -- --ignored speakers_stills`.
#[test]
#[ignore = "writes files under target/screenshots/speakers"]
fn speakers_stills() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/screenshots/speakers");
    std::fs::create_dir_all(&dir).expect("a folder");
    let put = |name: &str, app: &App, size: (u16, u16)| {
        let rows = text(&frame(app, size.0, size.1));
        std::fs::write(dir.join(format!("{name}.txt")), rows.join("\n")).expect("written");
    };
    for size in [(80, 24), (120, 36)] {
        let tag = format!("{}x{}", size.0, size.1);
        let pane = app_on(Screen::Device, true);
        put(&format!("pane-{tag}"), &pane, size);
        let mut tone = app_on(Screen::Tone, true);
        tone.focus_crossfeed(Xf::Space);
        put(&format!("tone-unfolded-{tag}"), &tone, size);
        tone.focus_strip(crate::tui::app::Stage::Space);
        put(&format!("tone-folded-{tag}"), &tone, size);
        let (mut list, _) = open(size.0, size.1);
        list.speakers_box.fetch = Some(Fetch {
            id: "room-1".into(),
            label: "Control room 1".into(),
            stop: crate::tui::boxes::speakers::Stop::new(),
            received: 26_660_000,
            total: 43_000_000,
            import: false,
            stopping: false,
        });
        put(&format!("sets-{tag}"), &list, size);
        list.sets_key(crossterm::event::KeyCode::Char('i'));
        put(&format!("import-{tag}"), &list, size);
    }
}

/// The catalogue's own labels and licences: the widest licence is 29 cells.
fn real_catalogue() -> SpeakersCatalogue {
    let with = |id, room, label: &str, licence: &str, fetched| SpeakersSet {
        licence: licence.to_string(),
        cut: false,
        ..set(id, room, label, fetched, 1_500_000)
    };
    SpeakersCatalogue {
        sets: vec![
            with(
                "mit-normal",
                SpeakersRoom::No,
                "MIT, normal pinna",
                "LicenseRef-MIT-Media-Lab-free",
                false,
            ),
            with(
                "sadie-d2",
                SpeakersRoom::No,
                "SADIE II, D2",
                "Apache-2.0",
                true,
            ),
            with(
                "hutubs-pp1",
                SpeakersRoom::No,
                "HUTUBS, subject pp1",
                "CC-BY-4.0",
                false,
            ),
            with(
                "cipic-003",
                SpeakersRoom::No,
                "CIPIC, subject 003",
                "LicenseRef-CIPIC-any-purpose",
                false,
            ),
            with(
                "sadie-d2-brir",
                SpeakersRoom::With,
                "SADIE II, D2 room",
                "Apache-2.0",
                false,
            ),
        ],
        fetching: true,
        note: None,
        max_sets: 32,
    }
}

#[test]
fn every_name_is_whole_at_80_and_120_columns_with_and_without_the_download_bar() {
    for (width, height) in [(80, 24), (120, 36)] {
        for downloading in [false, true] {
            let mut app = app_on(Screen::Tone, true);
            app.open_sets();
            app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(real_catalogue()))));
            if downloading {
                app.speakers_box.fetch = Some(Fetch {
                    id: "hutubs-pp1".into(),
                    label: "HUTUBS, subject pp1".into(),
                    stop: crate::tui::boxes::speakers::Stop::new(),
                    received: 750_000,
                    total: 1_500_000,
                    import: false,
                    stopping: false,
                });
            }
            let rows = text(&frame(&app, width, height));
            let shown = rows.join("\n");
            for label in [
                "MIT, normal pinna",
                "SADIE II, D2",
                "HUTUBS, subject pp1",
                "CIPIC, subject 003",
                "SADIE II, D2 room",
            ] {
                assert!(
                    has(&rows, label),
                    "{width}x{height} bar {downloading}: {label:?} in\n{shown}"
                );
            }
            if downloading {
                assert!(has(&rows, "50%"), "the bar: {shown}");
            }
            if width == 120 {
                // With room enough the licences stand whole in their column too.
                let row = rows
                    .iter()
                    .find(|r| r.contains("MIT, normal pinna"))
                    .expect("the row");
                assert!(row.contains("LicenseRef-MIT-Media-Lab-free"), "{row}");
            }
        }
    }
}

#[test]
fn a_cut_sets_length_comes_from_the_set() {
    let mut app = app_on(Screen::Tone, true);
    app.open_sets();
    let mut catalogue = real_catalogue();
    catalogue.sets[1].cut = true;
    catalogue.sets[1].taps_ms = 300.0;
    app.speakers_done(SpeakersDone::Catalogue(Ok(Box::new(catalogue))));
    app.speakers_box.list.as_mut().expect("list").row = 1;
    let rows = text(&frame(&app, 80, 24));
    assert!(has(&rows, "cut to 300.0 ms"), "{}", rows.join("\n"));
}
