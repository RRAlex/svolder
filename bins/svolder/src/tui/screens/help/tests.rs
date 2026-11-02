// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent};
use svolder_ipc::TuiConfig;

use crate::backend::DeviceRow;
use crate::tui::test_support::{
    app_showing, frame, frame_with_theme as frame_with, profile_named, text,
};

use super::*;

/// Every entry the overlay lists, section after section with a gap between them, for the
/// tests that read what it says rather than how it lays it out.
fn entries(app: &App) -> Vec<Entry> {
    let mut entries = Vec::new();
    for section in sections(app) {
        if !entries.is_empty() {
            entries.push(Entry::Gap);
        }
        entries.extend(section);
    }
    entries
}

/// One entry's key and what it does, for the tests that read the overlay's own model
/// rather than the frame it draws.
fn as_key(entry: &Entry) -> Option<(&str, &str)> {
    match entry {
        Entry::Key { label, does, .. } => Some((label, does.as_str())),
        Entry::Heading(_) | Entry::Gap => None,
    }
}

/// The keys listed under the heading `name`, up to the gap before the next heading.
fn section<'a>(entries: &'a [Entry], name: &str) -> Vec<(&'a str, &'a str)> {
    entries
        .iter()
        .skip_while(|entry| !matches!(entry, Entry::Heading(heading) if *heading == name))
        .skip(1)
        .take_while(|entry| !matches!(entry, Entry::Gap | Entry::Heading(_)))
        .filter_map(as_key)
        .collect()
}

/// Whether every screen the overlay lists takes `hint`, which is what makes it a key of
/// the "Everywhere" section rather than one screen's own.
fn every_screen(hint: &KeyHint) -> bool {
    SCREENS.iter().all(|screen| hint.screens.contains(screen))
}

/// The keys that work on every screen are named once, under "Everywhere", and no key is
/// named twice anywhere in the overlay: a reader looking a screen up reads its own keys
/// and not the ones they have already read.
#[test]
fn the_overlay_names_the_keys_that_work_everywhere_once_and_no_key_twice() {
    for screen in SCREENS {
        let mut app = app_showing(screen);
        app.overlays.help_open = true;
        let entries = entries(&app);
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for key in entries.iter().filter_map(as_key) {
            assert!(!seen.contains(&key), "{screen:?} names {key:?} twice");
            seen.push(key);
        }
        let everywhere = section(&entries, EVERYWHERE);
        for hint in keys::all().iter().filter(|hint| every_screen(hint)) {
            assert!(
                everywhere
                    .iter()
                    .any(|(label, _)| label.split(' ').any(|word| word == hint.label)),
                "{:?} is taken on every screen and is not under {EVERYWHERE}: {everywhere:?}",
                hint.label
            );
        }
    }
}

/// Ctrl-C, which quits at once from every screen, is named under "Everywhere" after the
/// keys the table holds — read on the overlay, and never a key the table lists, since a table
/// entry is a key with no modifier and `c` is a key of its own.
#[test]
fn the_overlay_names_ctrl_c_under_everywhere_and_no_table_entry_does() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let entries = entries(&app);
    let everywhere = section(&entries, EVERYWHERE);
    assert_eq!(everywhere.last(), Some(&("Ctrl-C", "quit at once")));
    assert!(
        keys::all().iter().all(|hint| hint.label != "Ctrl-C"),
        "a key table entry is labelled Ctrl-C"
    );
    let rows = text(&frame(&app, 126, 62));
    let line = rows
        .iter()
        .find(|row| row.contains("Ctrl-C"))
        .unwrap_or_else(|| panic!("the overlay does not draw Ctrl-C:\n{rows:#?}"));
    assert!(line.contains("Ctrl-C    quit at once"), "|{line}|");
}

/// Each screen's section holds that screen's own keys and no others, under a name that
/// says what the screen is for, so the device screen and the device list cannot be read
/// for one another.
#[test]
fn each_screen_section_holds_only_that_screens_own_keys() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let entries = entries(&app);
    for (screen, name) in [
        (Screen::Device, "Controls"),
        (Screen::Tone, "EQ"),
        (Screen::Telemetry, "Telemetry"),
        (Screen::Devices, "Devices"),
        (Screen::Settings, "Settings"),
    ] {
        let listed = section(&entries, name);
        assert!(!listed.is_empty(), "the overlay has no {name} section");
        // The tone-control screen's own section ends on what the mouse does there, which no
        // key table names.
        let listed = match screen {
            Screen::Tone => {
                let (keys, gestures) = listed.split_at(listed.len() - TONE_GESTURES.len());
                assert_eq!(gestures, TONE_GESTURES, "{name} ends on no mouse gestures");
                keys.to_vec()
            }
            _ => listed,
        };
        for (label, does) in listed {
            let hint = keys::all()
                .iter()
                .find(|hint| {
                    hint.label_in(Scene::of(&app)) == label
                        && (hint.does == does || hint.long == Some(does))
                })
                .unwrap_or_else(|| panic!("{name} lists {label}, which no key table names"));
            assert!(
                !every_screen(hint),
                "{name} repeats {label}, which every screen takes"
            );
            assert!(
                hint.screens.contains(&screen),
                "{name} lists {label}, which that screen does not take"
            );
        }
    }
}

/// On a device with no tone control the tone screen's own keys are still listed, each
/// with the reason it does nothing there, rather than as keys that seem to work.
#[test]
fn the_tone_keys_say_they_need_an_eq_on_a_device_without_one() {
    let mut app = App::new(
        profile_named("lyr5"),
        "lyr".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("lyr")],
    );
    app.overlays.help_open = true;
    let entries = entries(&app);
    let tone: Vec<(&str, Option<&str>)> = entries
        .iter()
        .skip_while(|entry| !matches!(entry, Entry::Heading(h) if *h == "EQ"))
        .skip(1)
        .take_while(|entry| !matches!(entry, Entry::Gap | Entry::Heading(_)))
        .filter_map(|entry| match entry {
            Entry::Key { label, reason, .. } => Some((*label, *reason)),
            Entry::Heading(_) | Entry::Gap => None,
        })
        .collect();
    for label in ["1-N", ", . ⇧←→", "[ ] ⇧↑↓", "L-drag", "R-drag", "wheel"] {
        assert!(
            tone.contains(&(label, Some("needs an EQ"))),
            "{label} is not listed with its reason: {tone:?}"
        );
    }
}

/// What the mouse does on the curve is listed after the tone-control screen's keys, and
/// dimmed with the reason while mouse capture is off, when none of it reaches the TUI.
#[test]
fn the_curve_gestures_say_so_while_mouse_capture_is_off() {
    for (mouse, reason) in [(true, None), (false, Some("mouse capture off"))] {
        let mut app = app_showing(Screen::Tone);
        app.mouse = mouse;
        app.overlays.help_open = true;
        let reasons: Vec<(&str, Option<&str>)> = entries(&app)
            .iter()
            .filter_map(|entry| match entry {
                Entry::Key { label, reason, .. } => Some((*label, *reason)),
                Entry::Heading(_) | Entry::Gap => None,
            })
            .filter(|(label, _)| TONE_GESTURES.iter().any(|(gesture, _)| gesture == label))
            .collect();
        assert_eq!(
            reasons,
            TONE_GESTURES
                .iter()
                .map(|&(label, _)| (label, reason))
                .collect::<Vec<_>>(),
            "mouse capture {mouse}"
        );
    }
}

/// The six uppercase keys are one entry each, in the order the key table has them, under
/// "Everywhere", each saying the mode it turns as the settings row for that mode names it.
#[test]
fn the_uppercase_toggles_are_one_entry_each_named_as_their_settings_rows() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let entries = entries(&app);
    let toggles: Vec<(&str, &str)> = section(&entries, EVERYWHERE)
        .into_iter()
        .filter(|(label, _)| label.chars().all(|c| c.is_ascii_uppercase()))
        .collect();
    assert_eq!(
        toggles,
        vec![
            ("M", "mouse capture"),
            ("E", "effects"),
            ("H", "hue drift"),
            ("R", "rainbow scroll"),
            ("F", "focus runner"),
            ("V", "title meter"),
        ]
    );
    let rows = text(&frame(&app, 126, 62));
    let line = rows
        .iter()
        .find(|row| row.contains("mouse capture"))
        .unwrap_or_else(|| panic!("the overlay does not name mouse capture:\n{rows:#?}"));
    assert!(
        !line.contains("hue drift") && !line.contains(" · "),
        "the toggles still share a line: |{line}|"
    );
}

/// The area a terminal `width` by `height` gives the overlay.
fn area(width: u16, height: u16) -> Rect {
    Rect::new(0, 0, width, height)
}

/// Two columns wherever two fit, a tall terminal included: the height is never a reason
/// to draw one long column, and it is the only reason to draw more than two.
#[test]
fn the_keys_take_two_columns_wherever_two_fit_and_more_only_for_the_height() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    for (width, height) in [(126, 62), (80, 24), (100, 80), (140, 45)] {
        let view = view(area(width, height), &app);
        assert_eq!(view.columns.len(), 2, "{width}×{height}");
    }
    assert_eq!(view(area(126, 62), &app).most, 0, "126×62 holds every key");
    let short = view(area(250, 24), &app);
    assert!(
        short.columns.len() > 2 && short.most == 0,
        "a wide, short terminal takes the columns the height needs: {} columns, {} to scroll",
        short.columns.len(),
        short.most
    );
    let rows = text(&frame(&app, 126, 62));
    assert!(
        rows.iter()
            .any(|row| row.contains("Everywhere") && row.contains("Controls")),
        "the two columns do not start on the same row:\n{rows:#?}"
    );
}

/// However tall the columns are packed, no column ends on a heading or opens on a gap, a
/// heading always has its first key under it in the same column, and a section a column
/// can hold is never split between two.
#[test]
fn a_column_never_ends_on_a_heading_and_splits_only_a_section_too_long_for_it() {
    let app = app_showing(Screen::Device);
    let sections = sections(&app);
    let total: usize = sections.iter().map(Vec::len).sum::<usize>() + sections.len();
    for height in 2..=total {
        let columns = pack(&sections, height, lead(&app, &sections));
        for column in &columns {
            assert!(column.len() <= height, "height {height}: {column:?}");
            assert!(
                !matches!(column.last(), Some(Entry::Heading(_) | Entry::Gap)),
                "height {height}: a column ends on {:?}",
                column.last()
            );
            assert!(
                !matches!(column.first(), Some(Entry::Gap)),
                "height {height}: a column opens on a gap"
            );
            for pair in column.windows(2) {
                if matches!(pair[0], Entry::Heading(_)) {
                    assert!(
                        matches!(pair[1], Entry::Key { .. }),
                        "height {height}: a heading without its first key"
                    );
                }
            }
        }
        for section in sections.iter().filter(|section| section.len() <= height) {
            let Some(Entry::Heading(name)) = section.first() else {
                panic!("a section opens on its heading");
            };
            let holder = columns
                .iter()
                .find(|column| {
                    column
                        .iter()
                        .any(|entry| matches!(entry, Entry::Heading(h) if h == name))
                })
                .expect("every heading is drawn");
            let at = holder
                .iter()
                .position(|entry| matches!(entry, Entry::Heading(h) if h == name))
                .expect("the heading is in its column");
            assert!(
                holder.len() >= at + section.len(),
                "height {height}: {name} is split though a column holds it"
            );
        }
    }
}

/// The columns are filled one after the other and come out about as long as each other:
/// the second is never empty while the first runs past the box.
#[test]
fn the_columns_are_balanced() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let view = view(area(126, 62), &app);
    let lengths: Vec<usize> = view.columns.iter().map(Vec::len).collect();
    let longest = sections(&app).iter().map(Vec::len).max().unwrap_or(0);
    let (short, long) = (
        lengths.iter().min().copied().unwrap_or(0),
        lengths.iter().max().copied().unwrap_or(0),
    );
    assert!(
        long - short <= longest,
        "the columns are {lengths:?} entries long"
    );
}

/// The box an 80×24 terminal draws: the rules, the keys in two balanced columns, one blank
/// cell inside either border, and the way to the rest said on the bottom one. usage/tui.md
/// shows this very box.
#[test]
fn the_overlay_on_an_80_by_24_terminal() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let area = view(area(80, 24), &app).area;
    let boxed: Vec<String> = text(&frame(&app, 80, 24))
        [usize::from(area.y)..usize::from(area.bottom())]
        .iter()
        .map(|row| {
            row.chars()
                .skip(usize::from(area.x))
                .take(usize::from(area.width))
                .collect()
        })
        .collect();
    let wanted = [
        "┌ Keys · ? or Esc closes ────────────────────────────────────────────────────┐",
        "│ Lowercase letters act on the device, or open a screen or the Settings box. │",
        "│ Uppercase letters toggle a TUI mode and save it to the config.             │",
        "│ Symbols and navigation keys are the same on every screen.                  │",
        "│                                                                            │",
        "│ Everywhere                        Controls                                 │",
        "│ ↑↓        move                    ␣/Enter   toggle/press                   │",
        "│ ←→        change                  +/=/-     volume                         │",
        "│ m         mute                    0         centre balance                 │",
        "│ i         phase                   a         advice, or all clear           │",
        "│ o         NOS                                                              │",
        "│ Tab/⇧Tab  pane                    EQ                                       │",
        "│ f         flat on/off             1-5       band                           │",
        "│ p         presets (a i h inside)  , . ⇧←→   frequency                      │",
        "│ b         identify                [ ] ⇧↑↓   Q                              │",
        "│ r         refresh                 Enter     edit band                      │",
        "│ Esc       back, or the menu       0         flatten                        │",
        "│ c         controls                L-drag    point freq/gain                │",
        "│ e         tone                    R-drag    Q, right widens                │",
        "│ t         telemetry               wheel     Q on the curve                 │",
        "│ d         devices                 2×click   edit band value                │",
        "└──────────────────────────────────────────────────────────────────── more ↓ ┘",
    ];
    assert_eq!(boxed, wanted);
    crate::tui::test_support::assert_usage_md_shows(&boxed, "help overlay");
}

/// However far a box that scrolls is scrolled, no column shows a heading on its last row
/// with the heading's keys hidden below the window.
#[test]
fn a_scrolled_window_never_ends_a_column_on_a_heading() {
    for app in [crowded(), {
        let mut app = app_showing(Screen::Device);
        app.resize(80, 24);
        app.overlays.help_open = true;
        app
    }] {
        let view = view(area(80, 24), &app);
        assert!(view.most > 0, "the box does not scroll at 80×24");
        for at in 0..=view.most {
            for column in &view.columns {
                let last = shown(column, at, view.rows - 1, view.rows);
                assert!(
                    !matches!(last, Some(Entry::Heading(_))),
                    "scrolled {at}: a column ends on {last:?}"
                );
            }
        }
    }
}

/// No text touches the box's border: one blank cell inside it on the left and the right,
/// on every row of the box.
#[test]
fn the_text_keeps_one_cell_off_the_border_on_either_side() {
    for (width, height) in [(126, 62), (80, 24)] {
        let mut app = app_showing(Screen::Device);
        app.overlays.help_open = true;
        let buffer = frame(&app, width, height);
        let boxed = view(area(width, height), &app).area;
        for y in boxed.y + 1..boxed.bottom() - 1 {
            for x in [boxed.x + 1, boxed.right() - 2] {
                assert_eq!(
                    buffer.cell((x, y)).expect("a cell of the box").symbol(),
                    " ",
                    "{width}×{height}: row {y} touches the border at column {x}"
                );
            }
        }
    }
}

#[test]
fn the_overlay_lists_every_screens_keys_with_the_one_being_shown_first() {
    for (shown, order) in [
        (
            Screen::Telemetry,
            [
                "Everywhere",
                "Telemetry",
                "Controls",
                "EQ",
                "Devices",
                "Settings",
            ],
        ),
        (
            Screen::Device,
            [
                "Everywhere",
                "Telemetry",
                "Controls",
                "EQ",
                "Devices",
                "Settings",
            ],
        ),
        (
            Screen::Tone,
            [
                "Everywhere",
                "Telemetry",
                "EQ",
                "Controls",
                "Devices",
                "Settings",
            ],
        ),
        (
            Screen::Devices,
            [
                "Everywhere",
                "Telemetry",
                "Devices",
                "Controls",
                "EQ",
                "Settings",
            ],
        ),
    ] {
        let mut app = app_showing(shown);
        app.overlays.help_open = true;
        let headings: Vec<&str> = sections(&app)
            .iter()
            .filter_map(|section| match section.first() {
                Some(Entry::Heading(name)) => Some(*name),
                _ => None,
            })
            .collect();
        assert_eq!(headings, order, "{shown:?}");
    }
    let mut app = app_showing(Screen::Telemetry);
    app.overlays.help_open = true;
    let rows = text(&frame(&app, 140, 45));
    assert!(
        rows.iter().any(|row| row.contains("raw ±1")),
        "the overlay does not say what the telemetry arrows do:\n{rows:#?}"
    );
}

/// While the Settings box is open the overlay lists `Settings` right after `Everywhere` and
/// `Telemetry`, over whichever screen is under the box.
#[test]
fn the_overlay_over_the_settings_box_lists_settings_first() {
    for shown in [Screen::Device, Screen::Telemetry, Screen::Devices] {
        let mut app = app_showing(shown);
        app.overlays.help_open = true;
        app.overlays.settings = Some(crate::tui::app::SettingsBox::default());
        let headings: Vec<&str> = sections(&app)
            .iter()
            .filter_map(|section| match section.first() {
                Some(Entry::Heading(name)) => Some(*name),
                _ => None,
            })
            .collect();
        assert_eq!(
            headings.get(1..3),
            Some(&["Telemetry", "Settings"][..]),
            "{shown:?}: {headings:?}"
        );
        assert_eq!(headings.iter().filter(|h| **h == "Settings").count(), 1);
    }
}

/// The Settings box's own keys are listed under `Settings`: its categories by `Tab` and `⇧Tab`
/// and by number.
#[test]
fn the_settings_section_lists_the_boxs_category_keys() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let entries = entries(&app);
    let settings = section(&entries, "Settings");
    for wanted in [("Tab/⇧Tab", "category"), ("1-5", "category")] {
        assert!(settings.contains(&wanted), "{wanted:?} in {settings:?}");
    }
    let rows = text(&frame(&app, 140, 45));
    assert!(
        rows.iter().any(|row| row.contains("Tab/⇧Tab  category")),
        "{rows:#?}"
    );
}

/// An app on a device with no tone control and no mute of its own, whose overlay names the
/// reason each of those keys is not worth pressing — which widens its columns past what an
/// 80×24 terminal holds, so its overlay is one that scrolls.
fn crowded() -> App {
    let mut app = App::new(
        profile_named("lyr5"), // no tone control
        "lyr5".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("lyr5")], // one device
    );
    app.resize(80, 24);
    app.key(KeyEvent::from(KeyCode::Char('?')));
    app
}

/// The overlay says why `b` is dimmed on a device whose profile offers no identify.
#[test]
fn the_identify_key_says_why_it_is_dimmed_on_a_mesh_card() {
    let mut app = App::new(
        profile_named("meshcard"),
        "card".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("card")],
    );
    app.resize(140, 40);
    app.key(KeyEvent::from(KeyCode::Char('?')));
    let rows = text(&frame(&app, 140, 40));
    assert!(shows(&rows, "offers no identify"), "{rows:#?}");
    let rows = text(&frame(&crowded_gungnir(), 140, 40));
    assert!(!shows(&rows, "offers no identify"), "{rows:#?}");
}

/// An app on a DAC that offers identify, with the help overlay open.
fn crowded_gungnir() -> App {
    let mut app = App::new(
        profile_named("gungnir2-v2"),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("gungnir")],
    );
    app.resize(140, 40);
    app.key(KeyEvent::from(KeyCode::Char('?')));
    app
}

/// Whether the frame drew `text` anywhere.
fn shows(rows: &[String], text: &str) -> bool {
    rows.iter().any(|row| row.contains(text))
}

#[test]
fn a_key_that_is_not_worth_pressing_says_why() {
    let mut app = crowded();
    app.resize(140, 40);
    let rows = text(&frame(&app, 140, 40));
    assert!(
        rows.iter().any(|row| row.contains("needs an EQ")),
        "the overlay does not say why the tone key is dimmed:\n{rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("needs a mute")),
        "the overlay does not say why the mute key is dimmed:\n{rows:#?}"
    );
}

/// The phase and NOS keys are listed only for a device that has the control: a DAC with both
/// names them, an amp names neither, where the mute and tone keys stay listed, dimmed.
#[test]
fn the_phase_and_nos_keys_are_listed_only_where_the_device_has_them() {
    let amp = crowded();
    let rows = text(&frame(&amp, 140, 60));
    assert!(!shows(&rows, "phase"), "an amp lists phase:\n{rows:#?}");
    assert!(!shows(&rows, "NOS"), "an amp lists NOS:\n{rows:#?}");
    assert!(shows(&rows, "needs a mute"));
    let mut dac = app_showing(Screen::Device);
    dac.overlays.help_open = true;
    let rows = text(&frame(&dac, 140, 60));
    assert!(
        rows.iter()
            .any(|row| row.contains("i ") && row.contains("phase")),
        "a DAC does not list phase:\n{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("o ") && row.contains("NOS")),
        "a DAC does not list NOS:\n{rows:#?}"
    );
    for hint in keys::all()
        .iter()
        .filter(|hint| matches!(hint.when, When::HasPhase | When::HasNos))
    {
        assert!(keys::hidden_in(hint, Scene::of(&amp)));
        assert!(!keys::hidden_in(hint, Scene::of(&dac)));
    }
}

#[test]
fn the_overlay_leaves_the_screen_under_it_in_sight() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let rows = text(&frame(&app, 140, 40));
    let boxed = rows
        .iter()
        .position(|row| row.contains("Keys"))
        .expect("the overlay is drawn");
    assert!(boxed > 0, "the overlay starts on the frame's own first row");
    app.overlays.help_open = false;
    let under = text(&frame(&app, 140, 40));
    assert_eq!(
        rows[boxed + 1].chars().next(),
        under[boxed + 1].chars().next(),
        "the overlay covered the screen's own first column: |{}|",
        rows[boxed + 1]
    );
}

#[test]
fn an_overlay_with_more_keys_than_its_box_scrolls_and_says_where_the_rest_are() {
    let mut app = crowded();
    let top = text(&frame(&app, 80, 24));
    assert!(
        !shows(&top, "category"),
        "every key fits, so nothing is left to scroll to:\n{top:#?}"
    );
    assert!(
        shows(&top, MORE_DOWN) && !shows(&top, MORE_UP),
        "the box does not say the rest of the keys are below it:\n{top:#?}"
    );

    app.key(KeyEvent::from(KeyCode::End));
    let end = text(&frame(&app, 80, 24));
    assert!(
        shows(&end, "␣/Enter"),
        "the last screen's own keys are still not drawn at the end:\n{end:#?}"
    );
    assert!(
        shows(&end, MORE_UP) && !shows(&end, MORE_DOWN),
        "the box does not say where the keys it has passed are:\n{end:#?}"
    );

    app.key(KeyEvent::from(KeyCode::Home));
    assert_eq!(text(&frame(&app, 80, 24)), top, "Home is not the top");
    for _ in 0..app.overlays.help_scroll + 60 {
        app.key(KeyEvent::from(KeyCode::Down));
    }
    assert_eq!(
        text(&frame(&app, 80, 24)),
        end,
        "the arrows do not reach the end, or go past it"
    );
}

#[test]
fn the_page_keys_move_the_overlay_one_boxful_at_a_time() {
    let mut app = crowded();
    let (page, most) = scroll_of(&app);
    assert!(page > 1 && most > 0, "{page} rows a page, {most} to scroll");

    let mut at = 0;
    while at < most {
        app.key(KeyEvent::from(KeyCode::PageDown));
        at = (at + page).min(most);
        assert_eq!(
            app.overlays.help_scroll, at,
            "a page down is a boxful, or the end"
        );
    }
    app.key(KeyEvent::from(KeyCode::PageDown));
    assert_eq!(app.overlays.help_scroll, most, "and never past the end");
    while at > 0 {
        app.key(KeyEvent::from(KeyCode::PageUp));
        at = at.saturating_sub(page);
        assert_eq!(
            app.overlays.help_scroll, at,
            "a page up is a boxful back, or the top"
        );
    }
    app.key(KeyEvent::from(KeyCode::PageUp));
    assert_eq!(app.overlays.help_scroll, 0, "and never before the top");
}

/// A terminal with room for every key of every screen says nothing about more: the box
/// scrolls only where the keys outrun it, which an 80×24 terminal now does.
#[test]
fn an_overlay_every_key_fits_in_says_nothing_about_more() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let rows = text(&frame(&app, 140, 45));
    assert!(
        shows(&rows, "category"),
        "not every key fits after all:\n{rows:#?}"
    );
    assert!(!shows(&rows, MORE_UP) && !shows(&rows, MORE_DOWN));
}

/// The box is the one thing in focus, so it is drawn in the theme's own colours rather
/// than in the grey everything around it is dimmed to: its border and title as a focused
/// pane's, the rules and what each key does in the text colour, the keys in the accent.
/// The one grey left inside it is a key that does nothing on this device, with its reason.
#[test]
fn the_overlay_is_drawn_in_the_themes_colours_and_greys_only_what_does_nothing() {
    use crate::tui::test_support::{assert_drawn_in_focus, greyed, modal_themes};

    for (depth, theme) in modal_themes() {
        let mut app = app_showing(Screen::Device);
        app.overlays.help_open = true;
        let buffer = frame_with(&app, 140, 45, &theme);
        let at = view(area(140, 45), &app).area;
        assert_drawn_in_focus(&buffer, at, &theme, depth);
        assert_eq!(
            greyed(&buffer, at, &theme),
            Vec::new(),
            "{depth}: a grey cell on a device every key works on"
        );
        let rows = text(&buffer);
        let (x, y) = rows
            .iter()
            .enumerate()
            .find_map(|(y, row)| {
                let at = row.find("Lowercase")?;
                Some((row[..at].chars().count(), y))
            })
            .expect("the first rule is drawn");
        let rule = buffer
            .cell((u16::try_from(x).unwrap_or(0), u16::try_from(y).unwrap_or(0)))
            .expect("the first cell of the rule");
        assert_eq!(
            Some(rule.fg),
            theme.style(Role::Text).fg,
            "{depth}: the rules are not in the text colour"
        );

        let mut crowded = crowded();
        crowded.resize(140, 40);
        let buffer = frame_with(&crowded, 140, 40, &theme);
        let at = view(area(140, 40), &crowded).area;
        assert_drawn_in_focus(&buffer, at, &theme, depth);
        let rows = text(&buffer);
        let faint = greyed(&buffer, at, &theme);
        assert!(
            !faint.is_empty(),
            "{depth}: the keys that do nothing are not greyed"
        );
        for (x, y, _) in faint {
            assert!(
                rows[usize::from(y)].contains(" · needs "),
                "{depth}: ({x}, {y}) is grey on a row that names no reason: |{}|",
                rows[usize::from(y)]
            );
        }
    }
}

/// The overlay opens with the three rules every key in the table follows, so a reader who
/// has them can guess a key rather than look it up.
#[test]
fn the_overlay_opens_with_the_three_rules() {
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let rows = text(&frame(&app, 140, 40));
    let title = rows
        .iter()
        .position(|row| row.contains("Keys"))
        .expect("the overlay is drawn");
    for (at, rule) in RULES.iter().enumerate() {
        assert!(
            rows[title + 1 + at].contains(rule),
            "rule {at} is not on the row under the title: |{}|",
            rows[title + 1 + at]
        );
    }
    let blank = &rows[title + 1 + RULES.len()];
    assert!(
        !blank.contains("Device") && !blank.contains("↑↓"),
        "the rules are not set apart from the keys under them: |{blank}|"
    );
}

/// The first three lines of the overlay read the three rules of the key table, word for word.
#[test]
fn the_rules_read_as_the_key_table_follows_them() {
    assert_eq!(
        RULES,
        [
            "Lowercase letters act on the device, or open a screen or the Settings box.",
            "Uppercase letters toggle a TUI mode and save it to the config.",
            "Symbols and navigation keys are the same on every screen.",
        ]
    );
    let mut app = app_showing(Screen::Device);
    app.overlays.help_open = true;
    let rows = text(&frame(&app, 140, 45));
    let title = rows
        .iter()
        .position(|row| row.contains("Keys · ? or Esc closes"))
        .expect("the overlay is drawn");
    for (at, rule) in RULES.iter().enumerate() {
        assert!(rows[title + 1 + at].contains(rule), "{rule}");
    }
}

/// At 80×24, scrolled to the top, the section of the shown screen is on view, heading the
/// second column, for every screen.
#[test]
fn the_shown_section_is_visible_at_80_by_24_for_every_screen() {
    for (screen, heading) in [
        (Screen::Device, "Controls"),
        (Screen::Tone, "EQ"),
        (Screen::Telemetry, "Telemetry"),
        (Screen::Devices, "Devices"),
    ] {
        let mut app = app_showing(screen);
        app.overlays.help_open = true;
        assert_eq!(app.overlays.help_scroll, 0);
        let rows = text(&frame(&app, 80, 24));
        let shown = rows
            .iter()
            .any(|row| row.contains("│ Everywhere") && row.contains(&format!("  {heading} ")));
        assert!(shown, "{screen:?}: {heading} is not in view:\n{rows:#?}");
    }
}

/// Telemetry follows Everywhere in the first column when another screen is shown, and heads
/// the second column when it is the one shown.
#[test]
fn telemetry_ends_the_first_column_unless_it_is_the_screen_shown() {
    for (screen, in_first) in [(Screen::Device, true), (Screen::Telemetry, false)] {
        let mut app = app_showing(screen);
        app.overlays.help_open = true;
        let view = view(area(126, 62), &app);
        let holds = |column: &Vec<Entry>, heading: &str| {
            column
                .iter()
                .any(|entry| matches!(entry, Entry::Heading(h) if *h == heading))
        };
        assert!(holds(&view.columns[0], "Everywhere"), "{screen:?}");
        assert_eq!(holds(&view.columns[0], "Telemetry"), in_first, "{screen:?}");
    }
}

/// An entry's width is its display width, so a label or description with a wide glyph widens
/// its column by two cells for each.
#[test]
fn an_entrys_width_counts_display_cells() {
    let key = Entry::Key {
        label: "世",
        does: "界 x".into(),
        reason: None,
    };
    assert_eq!(key.label_width(), 2);
    assert_eq!(key.width(2), 2 + LABEL_GAP + 4);
    assert_eq!(Entry::Heading("世界").width(0), 4);
}
