// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use svolder_ipc::{
    ColorDepth, GraphSpan, LinkSpan, TUI_SETTINGS, Transition, TuiConfig, TuiSettingKind, TuiValue,
};
use svolder_profiles::Family;

use crate::tui::app::{App, EffectSwitch, Intent, Screen, SettingsBox, TuiChange};
use crate::tui::mouse::{self, HitMap, MouseAction};
use crate::tui::render::{self, FrameAreas};
use crate::tui::test_support::{
    click, frame, frame_with_theme, press, profile, text, theme_for, type_text,
};
use crate::tui::theme::Theme;

use super::*;

/// An app on the Device screen with the Settings box open over it, drawn from the defaults
/// every key case starts from.
fn app() -> App {
    let mut app = plain();
    press(&mut app, KeyCode::Char('s'));
    app
}

/// An app on the Device screen with no box open.
fn plain() -> App {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.config_path = home()
        .expect("the test user has a home directory")
        .join(".config/svolder/config.toml");
    app
}

/// The row of the plain `[tui]` key the file spells `name`.
fn key(name: &str) -> Setting {
    Setting::Key(
        TUI_SETTINGS
            .into_iter()
            .find(|key| key.key == name)
            .unwrap_or_else(|| panic!("no key {name}")),
    )
}

/// Put the box on `setting`'s category and row.
fn select(app: &mut App, setting: Setting) {
    let (category, (_, rows)) = CATEGORIES
        .iter()
        .enumerate()
        .find(|(_, (_, rows))| rows.contains(&setting))
        .expect("every setting is in a category");
    let row = rows
        .iter()
        .position(|row| *row == setting)
        .expect("the setting is in its category");
    app.overlays.settings = Some(SettingsBox { category, row });
}

/// Every setting the box lists, category by category.
fn every_setting() -> Vec<Setting> {
    CATEGORIES
        .iter()
        .flat_map(|(_, rows)| rows.iter().copied())
        .collect()
}

/// One frame of `app` at `width` by `height`, with where it painted the box and the hits it
/// recorded.
fn drawn(app: &App, width: u16, height: u16) -> (Buffer, FrameAreas, HitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, &Theme::new(false), &mut hits))
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), areas, hits)
}

/// The 22 rows of the box at 80×24, from its top border to its bottom one.
fn box_rows(app: &App) -> Vec<String> {
    text(&frame(app, 80, 24))[2..24].to_vec()
}

/// What a left click at `(column, row)` asks for against the frame `app` draws at 80×24.
fn clicked(app: &App, column: u16, row: u16) -> Option<MouseAction> {
    let (_, _, hits) = drawn(app, 80, 24);
    mouse::resolve(&hits, click(column, row), app)
}

/// The Settings box on Look at 80×24, as `docs/usage/tui.md` shows it: the 22 rows from the
/// top border to the bottom one.
const LOOK_MOCKUP: [&str; 22] = [
    "┌ Settings ─ tab→ ─┤ ¹Look ├─ ²Effects ─ ³Behaviour ─ ⁴Host audio ─ ⁵Connect ──┐",
    "│ ▶ Palette             ‹ amber ›           │                                  │",
    "│   Accent · DAC          (preset)          │                                  │",
    "│   Accent · amp          (preset)          │  The colour preset every screen  │",
    "│   Accent · preamp       (preset)          │  draws from: amber, ice, red or  │",
    "│   Colours               auto              │  neon. Each names an accent per  │",
    "│   Monochrome            off               │  device family and the good,     │",
    "│   Theme background      on                │  warn and alert colours.         │",
    "│   Graphs                on                │                                  │",
    "│   Graph span            5m                │  [tui] palette = \"amber\"         │",
    "│   Link span             12h               │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "└ ~/.config/svolder/config.toml ──────────────────────────────────── esc close ┘",
];

#[test]
fn s_opens_the_box_over_every_screen_and_esc_closes_it_onto_the_same_screen() {
    for screen in [
        Screen::Device,
        Screen::Tone,
        Screen::Telemetry,
        Screen::Devices,
    ] {
        let mut app = plain();
        app.show(screen);
        assert_eq!(app.screen, screen);
        assert!(app.overlays.settings.is_none());
        assert!(press(&mut app, KeyCode::Char('s')).is_empty());
        assert_eq!(app.screen, screen, "the box opens over {screen:?}");
        assert!(app.overlays.settings.is_some() && app.modal_open());

        // The screen under the box is dimmed: its header row is in the dim role.
        let buffer = frame(&app, 80, 24);
        let dim = Theme::new(false).style(Role::Dim);
        let cell = buffer.cell((1, 0)).expect("a header cell");
        assert!(
            cell.modifier.contains(dim.add_modifier) && dim.fg.is_none_or(|fg| cell.fg == fg),
            "{screen:?}: the screen under the box is not dimmed"
        );

        assert!(press(&mut app, KeyCode::Esc).is_empty());
        assert!(app.overlays.settings.is_none());
        assert_eq!(app.screen, screen, "Esc closes the box onto {screen:?}");
    }
    // The screen enum keeps the name for the key table, but no app shows it.
    let mut app = plain();
    app.show(Screen::Settings);
    assert_eq!(app.screen, Screen::Device);
    assert!(app.overlays.settings.is_some());
}

#[test]
fn the_box_fills_the_body_and_is_capped_and_centred_as_the_body_is() {
    let (_, areas, _) = drawn(&app(), 80, 24);
    assert_eq!(areas.modal, Some(Rect::new(0, 2, 80, 22)));
    let (_, [_, body]) = layout_tier::split(Rect::new(0, 0, 300, 50));
    let (_, areas, _) = drawn(&app(), 300, 50);
    assert_eq!(areas.modal, Some(body));
    assert_eq!(body.width, 256, "the wide tier caps its body");
    assert_eq!(body.x, 22, "and sets it in the middle");
}

#[test]
fn reopening_shows_the_category_and_row_it_was_closed_on() {
    let mut app = plain();
    assert_eq!(app.overlays.settings, None);
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(
        app.overlays.settings,
        Some(SettingsBox {
            category: 0,
            row: 0
        }),
        "a new app opens on Look, row 0"
    );
    press(&mut app, KeyCode::Char('2'));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('t'));
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(
        app.overlays.settings,
        Some(SettingsBox {
            category: 1,
            row: 2
        })
    );
}

#[test]
fn the_top_border_names_the_categories_and_brackets_the_current_one() {
    let mut app = app();
    assert_eq!(
        box_rows(&app)[0],
        "┌ Settings ─ tab→ ─┤ ¹Look ├─ ²Effects ─ ³Behaviour ─ ⁴Host audio ─ ⁵Connect ──┐"
    );
    press(&mut app, KeyCode::Tab);
    let top = box_rows(&app)[0].clone();
    assert!(
        top.starts_with("┌ Settings ─ tab→ ─ ¹Look ─┤ ²Effects ├─ ³Behaviour ─ ⁴Host audio ─"),
        "{top}"
    );
    assert!(top.ends_with('┐') && top.chars().count() == 80, "{top}");
}

#[test]
fn the_bottom_border_carries_the_shortened_config_path() {
    let bottom = box_rows(&app())[21].clone();
    assert_eq!(
        bottom,
        format!(
            "└ ~/.config/svolder/config.toml {} esc close ┘",
            "─".repeat(36)
        )
    );
    let mut long = app();
    long.config_path = PathBuf::from(format!(
        "/srv/{}/svolder/config.toml",
        "a-long-directory/".repeat(6)
    ));
    let bottom = box_rows(&long)[21].clone();
    assert!(
        bottom.starts_with("└ …/")
            && bottom.contains("/svolder/config.toml ─")
            && bottom.chars().count() == 80,
        "a path too long gives up its leading directories: {bottom}"
    );
    assert!(bottom.ends_with(" esc close ┘"), "{bottom}");
}

/// However little room the bottom border has, `esc close` shows; the path gives way to it, from
/// the left, and a click on it presses `Esc`.
#[test]
fn esc_close_always_shows_and_a_click_on_it_closes_the_box() {
    let mut narrow = app();
    narrow.config_path = PathBuf::from(format!("/srv/{}.toml", "a-long-file-name".repeat(8)));
    for width in [80u16, 100] {
        let rows = text(&frame(&narrow, width, 24));
        let bottom = rows[23].clone();
        assert!(bottom.ends_with(" esc close ┘"), "{width}: {bottom}");
        assert!(bottom.starts_with("└ …"), "{width}: {bottom}");
        let (_, _, hits) = drawn(&narrow, width, 24);
        let at = u16::try_from(bottom.chars().position(|c| c == 'c').expect("a c")).unwrap();
        assert_eq!(
            mouse::resolve(&hits, click(at, 23), &narrow),
            Some(MouseAction::Key(KeyCode::Esc)),
            "{width}"
        );
    }
    assert_eq!(
        clicked(&app(), 73, 23),
        Some(MouseAction::Key(KeyCode::Esc))
    );
}

/// A left click outside the box, the greyed header and its logo among it, closes it as `Esc`
/// does; a click inside on a cell with no target does nothing.
#[test]
fn a_click_outside_the_box_presses_esc_and_one_inside_on_nothing_does_not() {
    let app = app();
    for (column, row) in [(0, 0), (1, 0), (40, 0), (70, 0), (79, 1)] {
        assert_eq!(
            clicked(&app, column, row),
            Some(MouseAction::Key(KeyCode::Esc)),
            "({column}, {row})"
        );
    }
    assert_eq!(clicked(&app, 60, 12), None);
}

/// The Look category at 80×24 draws its rows and its right column.
#[test]
fn look_at_80x24_draws_the_mockup() {
    let rows = box_rows(&app());
    assert_eq!(rows, LOOK_MOCKUP);
}

/// docs/usage/tui.md shows the box on Look at 80×24, byte for byte what this build draws.
#[test]
fn usage_md_shows_the_settings_box_this_build_draws() {
    let doc = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/usage/tui.md");
    let doc = std::fs::read_to_string(&doc).expect("docs/usage/tui.md reads");
    let lines: Vec<&str> = doc.lines().collect();
    let top = lines
        .iter()
        .position(|line| line.starts_with("┌ Settings ─ tab→"))
        .expect("usage/tui.md shows the Settings box");
    assert_eq!(lines[top..top + 22], box_rows(&app()));
}

/// Each row's left column is 43 cells: the cursor, a 19-cell label field, the value with its
/// arrows and the dim key in cell 41. Only the selected row has arrows, a value longer than 13
/// cells is cut to 12 and `…`, and `on (inactive)` stays whole.
#[test]
fn a_row_is_43_cells_and_only_the_selected_row_has_arrows() {
    let mut app = app();
    app.tui.accent = Some("a-long-colour-name".into());
    app.tui.effects.enabled = true;
    app.effects = false;
    let rows = box_rows(&app);
    for row in &rows[1..7] {
        let left: String = row.chars().skip(1).take(43).collect();
        assert_eq!(left.chars().count(), 43);
        assert_eq!(
            left.matches('›').count(),
            usize::from(row.contains("▶")),
            "{row}"
        );
    }
    assert!(rows[2].contains("a-long-colou… "), "{}", rows[2]);

    select(&mut app, Setting::Effects);
    let rows = box_rows(&app);
    assert!(
        rows[1].starts_with("│ ▶ Effects             ‹ on (inactive) ›E"),
        "{}",
        rows[1]
    );
    assert!(rows[2].contains("    Eased values") || rows[2].contains("     Eased values"));
    // The key sits in cell 41 of the left column, dim.
    let buffer = frame(&app, 80, 24);
    let key = buffer.cell((41, 3)).expect("the key cell");
    assert_eq!(key.symbol(), "E");
    let dim = Theme::new(false).style(Role::Dim);
    assert!(key.modifier.contains(dim.add_modifier));
    // The sub-rows are indented by two cells inside the label field.
    assert!(rows[2].starts_with("│     Eased values "), "{}", rows[2]);
}

/// The right column explains the selected row, then its config line, then the key that turns it
/// from any screen, its text two cells in from the rule and the border and two rows below the
/// top border.
#[test]
fn the_right_column_explains_names_the_config_key_and_the_uppercase_toggle() {
    let mut app = app();
    select(&mut app, Setting::Effect(EffectSwitch::Frames));
    let rows = box_rows(&app);
    for row in &rows[1..rows.len() - 1] {
        let margins: String = row
            .chars()
            .skip(45)
            .take(2)
            .chain(row.chars().skip(77).take(2))
            .collect();
        assert_eq!(margins, "    ", "{row}");
    }
    for row in &rows[1..3] {
        let column: String = row.chars().skip(45).take(34).collect();
        assert_eq!(column.trim(), "", "{row}");
    }
    let right: Vec<String> = rows[3..]
        .iter()
        .map(|row| {
            row.chars()
                .skip(47)
                .take(30)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    let wrapped = widgets::text::wrap(&explain(Setting::Effect(EffectSwitch::Frames)), 30);
    assert_eq!(right[..wrapped.len()], wrapped[..]);
    let after = wrapped.len();
    assert_eq!(right[after], "");
    assert_eq!(right[after + 1], "[tui.effects] frames = true");
    assert_eq!(right[after + 2], "F from any screen");
    assert_eq!(right[after + 3], "");

    select(&mut app, Setting::Effect(EffectSwitch::Glow));
    let all = box_rows(&app).join("\n");
    assert!(all.contains("[tui.effects] glow = true"));
    assert!(
        !all.contains("from any screen"),
        "a row no key turns names none"
    );
}

#[test]
fn config_lines_write_the_table_key_and_value() {
    let mut tui = TuiConfig::default();
    assert_eq!(
        config_line(Setting::Palette, &tui),
        "[tui] palette = \"amber\""
    );
    assert_eq!(
        config_line(Setting::Effect(EffectSwitch::Glow), &tui),
        "[tui.effects] glow = true"
    );
    assert_eq!(
        config_line(Setting::Effect(EffectSwitch::FaultPresence), &tui),
        "[tui.effects] fault_presence = true"
    );
    assert_eq!(
        config_line(Setting::Effect(EffectSwitch::ListFade), &tui),
        "[tui.effects] list_fade = true"
    );
    assert_eq!(
        config_line(Setting::HueDrift, &tui),
        "[tui.effects] hue_drift = false"
    );
    assert_eq!(
        config_line(Setting::SpectrumDrift, &tui),
        "[tui.effects] spectrum_drift = false"
    );
    assert_eq!(
        config_line(Setting::Effects, &tui),
        "[tui.effects] enabled = true"
    );
    assert_eq!(
        config_line(Setting::Accent(Family::Dac), &tui),
        "[tui.accents] dac  (not set)"
    );
    tui.accents.dac = Some("#e0341c".into());
    assert_eq!(
        config_line(Setting::Accent(Family::Dac), &tui),
        "[tui.accents] dac = \"#e0341c\""
    );
    assert_eq!(
        config_line(Setting::Accent(Family::Amp), &tui),
        "[tui.accents] amp  (not set)"
    );
    tui.accent = Some("blue".into());
    assert_eq!(
        config_line(Setting::Accent(Family::Amp), &tui),
        "[tui] accent = \"blue\"",
        "the one key that draws every family is the one a row names"
    );
    assert_eq!(
        config_line(key("transition"), &tui),
        "[tui] transition = \"trace\""
    );
    assert_eq!(
        config_line(Setting::Colors, &tui),
        "[tui] colors = \"auto\""
    );
    assert_eq!(config_line(Setting::Mouse, &tui), "[tui] mouse = true");
    assert_eq!(config_line(key("intro"), &tui), "[tui] intro = true");
    assert_eq!(
        config_line(key("intro_sound"), &tui),
        "[tui] intro_sound = false"
    );
    assert_eq!(
        config_line(key("confirm_quit"), &tui),
        "[tui] confirm_quit = true"
    );
    assert_eq!(
        config_line(key("title_meter"), &tui),
        "[tui] title_meter = true"
    );
    assert_eq!(
        config_line(Setting::HostAudio, &tui),
        "[tui] host_audio = true"
    );
    assert_eq!(
        config_line(key("visualizer"), &tui),
        "[tui] visualizer = true"
    );
    assert_eq!(
        config_line(key("monochrome"), &tui),
        "[tui] monochrome = false"
    );
}

/// The categories group the 31 `[tui]` rows, the AutoEq refresh and the 12 Connect rows, each
/// once, and every row has an explanation that with its config line fits the box at 80×24.
#[test]
fn every_setting_is_in_one_category_and_its_explanation_fits_the_box() {
    let all = every_setting();
    assert_eq!(all.len(), 44);
    for (at, setting) in all.iter().enumerate() {
        assert!(
            !all[at + 1..].contains(setting),
            "{setting:?} is listed twice"
        );
        assert!(!explain(*setting).is_empty());
    }
    let longest = CATEGORIES.iter().map(|(_, rows)| rows.len()).max();
    assert_eq!(longest, Some(13), "Effects is the longest category");
    // At 80×24 the right column's text is 30 cells wide and has the box's 20 inner rows less
    // the two above it and a blank one below.
    let tui = TuiConfig {
        accent: Some("a-long-colour-name".into()),
        ..TuiConfig::default()
    };
    for setting in all {
        let height = explanation_height(setting, &tui, 30);
        assert!(
            height <= 17,
            "{setting:?} takes {height} rows and does not fit the box at 80×24"
        );
        assert!(explain(setting).split(". ").count() <= 4, "{setting:?}");
    }
}

/// Nothing in the box draws the version, the daemon's version or the licence notice.
#[test]
fn the_box_draws_no_version_and_no_licence_notice() {
    let mut app = app();
    app.session.daemon = true;
    app.session.daemon_version = Some("9.9.9".into());
    for category in 0..CATEGORIES.len() {
        app.overlays.settings = Some(SettingsBox { category, row: 0 });
        let rows = box_rows(&app).join("\n");
        // The Connect category says what the daemon answers, and so names it.
        let named = match category == CONNECT {
            true => None,
            false => Some("daemon"),
        };
        for absent in [
            env!("CARGO_PKG_VERSION"),
            "9.9.9",
            "Copyright",
            "Licence",
            "GPL",
        ]
        .into_iter()
        .chain(named)
        {
            assert!(!rows.contains(absent), "the box draws {absent:?}");
        }
    }
}

#[test]
fn up_and_down_move_within_the_category_and_stop_at_its_ends() {
    let mut app = app();
    press(&mut app, KeyCode::Up);
    assert_eq!(
        app.overlays.settings,
        Some(SettingsBox {
            category: 0,
            row: 0
        })
    );
    for _ in 0..10 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(
        app.overlays.settings,
        Some(SettingsBox {
            category: 0,
            row: rows(0).len() - 1
        })
    );
}

#[test]
fn tab_and_the_digits_choose_the_category() {
    let mut app = app();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        app.overlays.settings,
        Some(SettingsBox {
            category: 1,
            row: 0
        }),
        "Tab moves on and starts at the first row"
    );
    for want in [2, 3, 4, 0] {
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.overlays.settings.map(|at| at.category), Some(want));
    }
    press(&mut app, KeyCode::BackTab);
    assert_eq!(
        app.overlays.settings.map(|at| at.category),
        Some(4),
        "Shift-Tab wraps round"
    );
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::BackTab);
    assert_eq!(
        app.overlays.settings,
        Some(SettingsBox {
            category: 3,
            row: 0
        })
    );
    for (digit, want) in [('1', 0), ('4', 3), ('2', 1), ('3', 2), ('5', 4)] {
        press(&mut app, KeyCode::Char(digit));
        assert_eq!(app.overlays.settings.map(|at| at.category), Some(want));
    }
    press(&mut app, KeyCode::Char('6'));
    assert_eq!(
        app.overlays.settings.map(|at| at.category),
        Some(4),
        "there is no sixth"
    );
}

#[test]
fn q_asks_to_quit_or_quits_with_confirm_quit_off() {
    let mut app = app();
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.quitting && app.overlays.settings.is_some());
    press(&mut app, KeyCode::Esc);
    assert!(
        !app.overlays.quitting && app.overlays.settings.is_some(),
        "the question gives the box back"
    );

    app.tui.confirm_quit = false;
    assert_eq!(press(&mut app, KeyCode::Char('q')), vec![Intent::Quit]);
}

/// Review focus 2: a key the box does not take never reaches the screen under it.
#[test]
fn keys_the_box_does_not_take_do_not_reach_the_screen_under_it() {
    assert!(
        !press(&mut plain(), KeyCode::Char('m')).is_empty(),
        "m mutes on the screen with no box"
    );
    for letter in ['m', 'r', 'e', 't', 'd', 'f', 'a', 'x', 'n', 'c'] {
        for screen in [
            Screen::Device,
            Screen::Tone,
            Screen::Telemetry,
            Screen::Devices,
        ] {
            let mut app = plain();
            app.show(screen);
            press(&mut app, KeyCode::Char('s'));
            let before = app.overlays.settings;
            assert_eq!(
                press(&mut app, KeyCode::Char(letter)),
                Vec::new(),
                "{letter} on {screen:?}"
            );
            assert_eq!(
                app.screen, screen,
                "{letter} switched the screen under the box"
            );
            assert_eq!(app.overlays.settings, before, "{letter} moved the box");
            assert!(
                !app.overlays.help_open && app.devices.editor.is_none() && !app.notices.show_advice
            );
        }
    }
}

/// `?` opens the help overlay over the box on every screen, the box stays open under it, and
/// `Esc` closes the overlay back onto the box.
#[test]
fn question_mark_opens_help_over_the_box_and_esc_closes_it_back_onto_the_box() {
    for screen in [
        Screen::Device,
        Screen::Tone,
        Screen::Telemetry,
        Screen::Devices,
    ] {
        let mut app = plain();
        app.show(screen);
        press(&mut app, KeyCode::Char('s'));
        let before = app.overlays.settings;
        assert!(before.is_some());
        assert_eq!(
            press(&mut app, KeyCode::Char('?')),
            Vec::new(),
            "{screen:?}"
        );
        assert!(app.overlays.help_open, "? on {screen:?}");
        assert_eq!(
            app.overlays.settings, before,
            "the box stays open under the help"
        );
        assert_eq!(app.screen, screen);
        press(&mut app, KeyCode::Esc);
        assert!(!app.overlays.help_open, "Esc closes the help on {screen:?}");
        assert_eq!(
            app.overlays.settings, before,
            "Esc left the box open on {screen:?}"
        );
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.overlays.settings, None, "a second Esc closes the box");
    }
}

/// Ctrl-C quits from the overlay over the box, as it does from the box.
#[test]
fn ctrl_c_quits_from_the_help_over_the_box() {
    let mut app = app();
    press(&mut app, KeyCode::Char('?'));
    assert!(app.overlays.help_open);
    let quit = app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(quit, vec![Intent::Quit]);
}

#[test]
fn down_then_space_toggles_the_row_the_cursor_reached() {
    let mut app = app();
    select(&mut app, Setting::Effects);
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Effects(false))]
    );
    assert_eq!(press(&mut app, KeyCode::Down), Vec::new());
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Tui(TuiChange::Effect(EffectSwitch::Eased, false))],
        "Enter toggles a switch as Space does"
    );
}

/// The rows that step through choices go round them; every other row is a switch, which `←→`
/// flips the same as `Space` or `Enter` does.
#[test]
fn the_arrows_step_the_choice_rows_and_flip_the_switches() {
    let mut app = app();
    select(&mut app, Setting::Palette);
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Palette("ice".into()))]
    );
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::Tui(TuiChange::Palette("neon".into()))],
        "the palettes come round the other way too"
    );
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        Vec::new(),
        "a choice row has nothing to toggle"
    );
    select(&mut app, key("intro"));
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Set(&INTRO, TuiValue::Bool(false)))],
        "a switch flips on → too"
    );
}

/// The intro rows sit together in Behaviour, the effect then the sound under the intro row;
/// the sound reads off by default, and toggles to a change that sets the one key.
#[test]
fn the_intro_rows_follow_the_intro_row_and_the_sound_toggles() {
    let rows = rows(2);
    let at = rows.iter().position(|s| *s == key("intro")).unwrap();
    assert_eq!(rows[at + 1], key("intro_effect"));
    assert_eq!(rows[at + 2], key("intro_sound"));
    let mut app = app();
    select(&mut app, key("intro_sound"));
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Set(
            &INTRO_SOUND,
            TuiValue::Bool(true)
        ))]
    );
    let mut tui = svolder_ipc::TuiConfig::default();
    TuiChange::Set(&INTRO_SOUND, TuiValue::Bool(true)).apply(&mut tui);
    assert!(tui.intro_sound);
}

/// Whether `setting` is a plain `[tui]` key that holds one word of a list rather than a switch.
fn is_word_key(setting: Setting) -> bool {
    matches!(setting, Setting::Key(key) if matches!(key.kind, TuiSettingKind::Word { .. }))
}

/// Every on/off row flips whichever way `←` or `→` is pressed on it, the same change
/// `Space` asks for.
#[test]
fn arrows_flip_every_on_off_row_either_way() {
    for setting in every_setting() {
        let mut baseline = app();
        select(&mut baseline, setting);
        let want = press(&mut baseline, KeyCode::Char(' '));
        if want.is_empty() {
            assert!(
                matches!(
                    setting,
                    Setting::AutoEqRefresh
                        | Setting::Palette
                        | Setting::Accent(_)
                        | Setting::Colors
                ) || is_word_key(setting)
                    || (setting.is_connection()
                        && !matches!(
                            setting,
                            Setting::McpEnabled
                                | Setting::McpWrites
                                | Setting::MqttEnabled
                                | Setting::MqttCommands
                        )),
                "{setting:?} did not toggle on Space"
            );
            continue;
        }
        for arrow in [KeyCode::Left, KeyCode::Right] {
            let mut app = app();
            select(&mut app, setting);
            assert_eq!(press(&mut app, arrow), want, "{setting:?} on {arrow:?}");
        }
    }
}

/// The uppercase toggles turn their row from inside the box, and the box shows the change.
#[test]
fn the_uppercase_toggles_change_their_row_and_the_box_shows_it() {
    let mut app = app();
    press(&mut app, KeyCode::Tab);
    select(&mut app, Setting::Effects);
    let intents = app.key(KeyEvent::new(KeyCode::Char('E'), KeyModifiers::SHIFT));
    assert_eq!(intents, vec![Intent::Tui(TuiChange::Effects(false))]);
    TuiChange::Effects(false).apply(&mut app.tui);
    assert!(
        box_rows(&app)[1].contains("‹ off ›"),
        "{}",
        box_rows(&app)[1]
    );
}

/// The host's two rows turn host audio and the visualizer the other way from what they
/// say, each on `Space`, and say what the settings hold.
#[test]
fn the_host_rows_turn_host_audio_and_the_visualizer() {
    for (setting, off, on) in [
        (
            Setting::HostAudio,
            TuiChange::Set(&HOST_AUDIO, TuiValue::Bool(false)),
            TuiChange::Set(&HOST_AUDIO, TuiValue::Bool(true)),
        ),
        (
            key("visualizer"),
            TuiChange::Set(&VISUALIZER, TuiValue::Bool(false)),
            TuiChange::Set(&VISUALIZER, TuiValue::Bool(true)),
        ),
    ] {
        let mut app = app();
        select(&mut app, setting);
        assert_eq!(
            press(&mut app, KeyCode::Char(' ')),
            vec![Intent::Tui(off.clone())]
        );
        off.apply(&mut app.tui);
        assert_eq!(setting.value(&app), "off", "{setting:?}");
        assert_eq!(press(&mut app, KeyCode::Char(' ')), vec![Intent::Tui(on)]);
    }
}

/// With `SVOLDER_HOST=off` holding host audio off for the process, the row still says what
/// the file holds, cut to the value field, and turns it.
#[test]
fn the_host_audio_row_says_when_the_environment_holds_it_off() {
    let mut app = app();
    app.host.held_off = true;
    assert_eq!(Setting::HostAudio.value(&app), "on (SVOLDER_HOST=off)");
    select(&mut app, Setting::HostAudio);
    assert!(
        box_rows(&app)[1].starts_with("│ ▶ Host audio          ‹ on (SVOLDER_… ›"),
        "{}",
        box_rows(&app)[1]
    );
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Set(
            &HOST_AUDIO,
            TuiValue::Bool(false)
        ))]
    );
}

/// An accent row steps from the preset's own colour through the two colours the palette
/// gives the other families, and round to the preset again.
#[test]
fn an_accent_row_steps_through_the_palettes_other_family_colours() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#b86a2a".into())
        ))],
        "the amber palette's amplifier colour comes after the preset's own"
    );
    app.tui.accents.dac = Some("#b86a2a".into());
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#f6cc7a".into())
        ))],
        "then the preamplifier's"
    );
    app.tui.accents.dac = Some("#f6cc7a".into());
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(Family::Dac, None))],
        "and round to the preset's own again"
    );
}

/// What a row says follows the settings rather than the defaults, and mouse capture says
/// what is on rather than what the file asked for.
#[test]
fn each_row_says_what_the_settings_hold_now() {
    let mut app = app();
    app.tui.palette = "neon".into();
    app.tui.colors = ColorDepth::Ansi256;
    app.tui.monochrome = true;
    app.tui.intro = false;
    app.tui.effects.hue_drift = true;
    app.tui.effects.shimmer = false;
    app.tui.accents.preamp = Some("#123456".into());
    app.mouse = false;
    let row = |app: &App, wanted: &str| {
        box_rows(app)
            .iter()
            .find(|row| row.contains(wanted))
            .unwrap_or_else(|| panic!("no row carries {wanted:?}"))
            .chars()
            .skip(1)
            .take(43)
            .collect::<String>()
            .trim_end()
            .to_string()
    };
    assert_eq!(row(&app, "Palette"), " ▶ Palette             ‹ neon ›");
    assert_eq!(
        row(&app, "Accent · preamp"),
        "   Accent · preamp       #123456"
    );
    assert_eq!(row(&app, "Colours"), "   Colours               256");
    assert_eq!(row(&app, "Monochrome"), "   Monochrome            on");
    app.overlays.settings = Some(SettingsBox {
        category: 1,
        row: 0,
    });
    assert_eq!(row(&app, "Shimmer"), "     Shimmer             off");
    assert_eq!(
        row(&app, "Hue drift"),
        "   Hue drift             on             H"
    );
    app.overlays.settings = Some(SettingsBox {
        category: 2,
        row: 0,
    });
    assert_eq!(
        row(&app, "Mouse capture"),
        " ▶ Mouse capture       ‹ off ›          M"
    );
    assert_eq!(row(&app, "Intro on launch"), "   Intro on launch       off");

    // The one key that draws all three families says so on every accent row.
    app.tui.accent = Some("blue".into());
    app.overlays.settings = Some(SettingsBox {
        category: 0,
        row: 1,
    });
    assert_eq!(
        row(&app, "Accent · DAC"),
        " ▶ Accent · DAC        ‹ blue (all) ›"
    );
    assert_eq!(
        row(&app, "Accent · amp"),
        "   Accent · amp          blue (all)"
    );
    assert_eq!(
        row(&app, "Accent · preamp"),
        "   Accent · preamp       blue (all)"
    );
}

/// A row that a key of the whole TUI also turns names that key, dimmed, in cell 41 — and the
/// key it names is the one that turns that very row, the key table being where both the row
/// and the key press read it from.
#[test]
fn a_row_a_tui_wide_key_turns_names_that_key_in_its_own_dimmed_column() {
    let mut named = Vec::new();
    for (category, (_, settings)) in CATEGORIES.iter().enumerate() {
        for (row, setting) in settings.iter().enumerate() {
            let mut app = app();
            app.overlays.settings = Some(SettingsBox { category, row });
            let buffer = frame(&app, 80, 24);
            let y = u16::try_from(row).expect("a row index") + 3;
            let key_cell = buffer.cell((41, y)).expect("a cell in the key column");
            let Some(hint) = keys::toggled_by(*setting) else {
                assert_eq!(
                    key_cell.symbol(),
                    " ",
                    "{setting:?} names a key no key turns"
                );
                continue;
            };
            named.push(hint.label);
            assert_eq!(key_cell.symbol(), hint.label, "{setting:?}");
            let dim = Theme::new(false).style(Role::Dim);
            assert!(
                key_cell.modifier.contains(dim.add_modifier)
                    && dim.fg.is_none_or(|fg| key_cell.fg == fg),
                "{setting:?}'s key is not drawn dimmed"
            );
            let KeyCode::Char(letter) = hint.code else {
                panic!("{setting:?} is turned by a key that is no letter");
            };
            assert_eq!(
                app.key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::SHIFT)),
                press(&mut app, KeyCode::Char(' ')),
                "{letter} turns another switch than the row that names it"
            );
        }
    }
    named.sort_unstable();
    assert_eq!(named, ["E", "F", "H", "M", "R", "V"]);
}

/// A click on a row selects it; on the `‹` or `›` of the selected row steps it as `←` and `→`
/// do; on a category's name switches to it; anywhere outside the box, or on the key row under
/// it, does nothing.
#[test]
fn the_mouse_selects_rows_steps_values_and_switches_categories() {
    let app = app();
    // Row 2 of Look is the amp accent, on screen line 5.
    assert_eq!(
        clicked(&app, 10, 5),
        Some(MouseAction::Setting(SettingsBox {
            category: 0,
            row: 2
        }))
    );
    // The selected row's `‹` is at cell 24 and its `›` right after "amber".
    assert_eq!(clicked(&app, 24, 3), Some(MouseAction::Key(KeyCode::Left)));
    assert_eq!(clicked(&app, 32, 3), Some(MouseAction::Key(KeyCode::Right)));
    // `²Effects` on the top border, whose text starts after `┤ ¹Look ├─ `.
    let top = box_rows(&app)[0].clone();
    let at = top.chars().position(|c| c == '²').expect("a ²") as u16;
    assert_eq!(
        clicked(&app, at, 2),
        Some(MouseAction::Setting(SettingsBox {
            category: 1,
            row: 0
        }))
    );
    assert_eq!(
        clicked(&app, 0, 0),
        Some(MouseAction::Key(KeyCode::Esc)),
        "the dimmed header is outside the box"
    );
    assert_eq!(
        clicked(&app, 40, 23),
        None,
        "the bottom border off its `esc close` is inside the box"
    );
    assert_eq!(clicked(&app, 60, 12), None, "nor is the explanation");

    // A click on a row of a later category keeps that category.
    let mut other = self::app();
    other.overlays.settings = Some(SettingsBox {
        category: 1,
        row: 0,
    });
    assert_eq!(
        clicked(&other, 10, 7),
        Some(MouseAction::Setting(SettingsBox {
            category: 1,
            row: 4
        }))
    );
}

/// Under the quit question the box is answered from the keyboard, and a click outside the
/// question presses `Esc`, which cancels it and leaves the Settings box open.
#[test]
fn the_quit_question_over_the_box_takes_the_mouse() {
    let mut app = app();
    press(&mut app, KeyCode::Char('q'));
    assert_eq!(clicked(&app, 10, 5), Some(MouseAction::Key(KeyCode::Esc)));
    let (_, _, hits) = drawn(&app, 80, 24);
    let top = hits.top_box().expect("the question is a box");
    assert_eq!(
        mouse::resolve(&hits, click(top.x + 1, top.y + 1), &app),
        None
    );
}

#[test]
fn every_preset_draws_the_box_at_every_depth() {
    for palette in ["amber", "ice", "red", "neon"] {
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
        ] {
            let theme = theme_for(palette, depth);
            let drawn = format!("{palette} at {depth:?}");
            for category in 0..CATEGORIES.len() {
                let mut app = app();
                app.overlays.settings = Some(SettingsBox { category, row: 0 });
                let buffer = frame_with_theme(&app, 80, 24, &theme);
                let selected = theme.style(Role::Selected);
                let cell = buffer.cell((4, 3)).expect("the selected row");
                assert!(
                    cell.modifier.contains(selected.add_modifier),
                    "{drawn}: the selected row is not highlighted"
                );
            }
        }
    }
}

/// A terminal as wide as the wide tier keeps the left column at 43 cells and grows the right.
#[test]
fn a_wide_terminal_grows_the_right_column_only() {
    let (buffer, areas, _) = drawn(&app(), 126, 40);
    let rows = text(&buffer);
    let body = areas.modal.expect("the box is a modal");
    // The right column's first line is two rows under the top border.
    let row = &rows[usize::from(body.y) + 3];
    assert_eq!(row.chars().nth(44), Some('│'), "{row}");
    assert!(
        row.contains("The colour preset every screen draws from: amber, ice, red or"),
        "{row}"
    );
}

#[test]
fn the_colour_depths_step_through_the_four_the_config_names() {
    let mut app = app();
    select(&mut app, Setting::Colors);
    for (from, to) in [
        (ColorDepth::Auto, ColorDepth::Ansi16),
        (ColorDepth::Ansi16, ColorDepth::Ansi256),
        (ColorDepth::Ansi256, ColorDepth::TrueColor),
        (ColorDepth::TrueColor, ColorDepth::Auto),
    ] {
        app.tui.colors = from;
        assert_eq!(
            press(&mut app, KeyCode::Right),
            vec![Intent::Tui(TuiChange::Colors(to))],
            "{from:?} does not step to {to:?}"
        );
    }
}

/// The transition row steps through the four words the config names, round the other way
/// on `←`, and has nothing to toggle.
#[test]
fn the_transition_row_steps_through_the_four_words() {
    let mut app = app();
    select(&mut app, key("transition"));
    for (from, to) in [
        (Transition::None, Transition::Trace),
        (Transition::Trace, Transition::Noise),
        (Transition::Noise, Transition::Melt),
        (Transition::Melt, Transition::None),
    ] {
        app.tui.transition = from;
        assert_eq!(
            press(&mut app, KeyCode::Right),
            vec![Intent::Tui(TuiChange::Set(
                &TRANSITION,
                TuiValue::Word(to.name())
            ))],
            "{from:?} does not step to {to:?}"
        );
        app.tui.transition = to;
        assert_eq!(
            press(&mut app, KeyCode::Left),
            vec![Intent::Tui(TuiChange::Set(
                &TRANSITION,
                TuiValue::Word(from.name())
            ))],
            "{to:?} does not step back to {from:?}"
        );
    }
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        Vec::new(),
        "a choice row has nothing to toggle"
    );
}

#[test]
fn the_path_row_writes_the_home_directory_as_a_tilde() {
    let home = Path::new("/home/you");
    assert_eq!(
        shorten(
            Path::new("/home/you/.config/svolder/config.toml"),
            Some(home)
        ),
        "~/.config/svolder/config.toml"
    );
    assert_eq!(
        shorten(Path::new("/etc/svolder/config.toml"), Some(home)),
        "/etc/svolder/config.toml",
        "a path outside the home directory is written in full"
    );
    assert_eq!(
        shorten(Path::new("/home/you/.config/svolder/config.toml"), None),
        "/home/you/.config/svolder/config.toml",
        "and so is every path with no home directory to measure against"
    );
}

/// A path too long for its room gives up whole directories from the front, then the front
/// of a file name too long on its own; one that fits is left alone.
#[test]
fn a_path_too_long_for_its_room_is_cut_from_the_left() {
    let path = "~/a/very/long/path/svolder/config.toml";
    assert_eq!(fitted(path, 80), path);
    assert_eq!(fitted(path, path.chars().count()), path);
    assert_eq!(fitted(path, 22), "…/svolder/config.toml");
    assert_eq!(fitted(path, 27), "…/path/svolder/config.toml");
    assert_eq!(fitted(path, 20), "…/config.toml");
    assert_eq!(fitted(path, 10), "…nfig.toml");
    assert_eq!(fitted(path, 1), "…");
    assert_eq!(fitted(path, 0), "");
    assert_eq!(fitted("/é/ü/config.toml", 13), "…/config.toml");
}

/// The Effects row says `on (inactive)` when the switch is on but the terminal cannot show the
/// animation layer, `on` when the layer runs, and `off` when it is switched off.
#[test]
fn the_effects_row_says_when_the_layer_is_on_but_inert() {
    let mut app = app();
    app.tui.effects.enabled = true;
    app.effects = false;
    assert_eq!(Setting::Effects.value(&app), "on (inactive)");
    app.effects = true;
    assert_eq!(Setting::Effects.value(&app), "on");
    app.tui.effects.enabled = false;
    app.effects = false;
    assert_eq!(Setting::Effects.value(&app), "off");
}

/// The Effects category's rows fade by their distance from the focused row, the explanation
/// beside them keeping its colours; nothing fades with the switch off or below true colour.
#[test]
fn the_effects_rows_fade_with_the_distance_from_the_focus() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let mut app = app();
    select(&mut app, Setting::Effects);
    let plain = frame_with_theme(&app, 80, 24, &theme);
    app.list_fade = true;
    let faded = frame_with_theme(&app, 80, 24, &theme);
    let rows = rows(1);
    let lines: Vec<(u16, usize)> = text(&plain)
        .iter()
        .enumerate()
        .filter_map(|(y, line)| {
            let left = line.strip_prefix('│')?.split('│').next()?;
            let at = rows.iter().position(|row| left.contains(row.label()))?;
            Some((y as u16, at))
        })
        .collect();
    assert_eq!(lines.len(), 13, "{lines:?}");
    crate::tui::test_support::assert_list_faded(&plain, &faded, &lines, 2..LEFT_WIDTH, &theme);

    let q = theme_for("amber", ColorDepth::Ansi256);
    app.list_fade = false;
    let off = frame_with_theme(&app, 80, 24, &q);
    app.list_fade = true;
    assert_eq!(off, frame_with_theme(&app, 80, 24, &q));
}

/// The Effects category lists `List fade` after `Peak hold`, says what it does, and Space flips
/// it.
#[test]
fn the_list_fade_row_sits_after_peak_hold_and_flips() {
    let effects = rows(1);
    assert_eq!(effects.len(), 13);
    let peak = effects
        .iter()
        .position(|row| *row == Setting::Effect(EffectSwitch::PeakHold))
        .unwrap();
    assert_eq!(effects[peak + 1], Setting::Effect(EffectSwitch::ListFade));
    let row = Setting::Effect(EffectSwitch::ListFade);
    assert_eq!(row.label(), "List fade");
    assert_eq!(
        explain(row),
        "Rows of a list dim a little with their distance from the selected row, as btop does."
    );
    let mut app = app();
    select(&mut app, row);
    assert_eq!(row.value(&app), "on");
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Effect(
            EffectSwitch::ListFade,
            false
        ))]
    );
}

/// The Effects category lists `Mascot` after `List fade`, among the indented switches the master
/// turns off, and Space flips it.
#[test]
fn the_mascot_row_sits_after_list_fade_and_flips() {
    let effects = rows(1);
    let fade = effects
        .iter()
        .position(|row| *row == Setting::Effect(EffectSwitch::ListFade))
        .unwrap();
    let row = Setting::Effect(EffectSwitch::Mascot);
    assert_eq!(effects[fade + 1], row);
    assert_eq!(row.label(), "Mascot");
    let mut app = app();
    select(&mut app, row);
    assert_eq!(row.value(&app), "on");
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Effect(EffectSwitch::Mascot, false))]
    );
    assert_eq!(
        config_line(row, &TuiConfig::default()),
        "[tui.effects] mascot = true"
    );
}

/// With the cursor on the Effects row, the first glyph of the label six rows down and of the one
/// ten rows down are the text colour #f2f0ea blended 0.165 and 0.33 of the way to the dim grey:
/// (225, 223, 218) and (208, 206, 202), as in the widget test.
#[test]
fn a_label_fades_to_the_literal_colours_six_and_ten_rows_away() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let mut app = app();
    select(&mut app, Setting::Effects);
    app.list_fade = true;
    let faded = frame_with_theme(&app, 80, 24, &theme);
    let lines = text(&faded);
    for (distance, rgb) in [(6, (225, 223, 218)), (10, (208, 206, 202))] {
        let label = rows(1)[distance].label();
        let (y, line) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| {
                line.strip_prefix('\u{2502}').is_some_and(|l| {
                    l.split('\u{2502}')
                        .next()
                        .is_some_and(|l| l.contains(label))
                })
            })
            .unwrap_or_else(|| panic!("no row names {label}"));
        let x = line
            .chars()
            .position(|c| c == label.chars().next().unwrap())
            .unwrap();
        let x = line
            .find(label)
            .map_or(x, |byte| line[..byte].chars().count());
        assert_eq!(
            faded[(x as u16, y as u16)].fg,
            ratatui::style::Color::Rgb(rgb.0, rgb.1, rgb.2),
            "{label}"
        );
    }
}

/// The graphs row turns the switch on `Space` and either arrow, and the two span rows step
/// through their sets, round the other way on `←`, wrapping at the ends, with nothing to
/// toggle.
#[test]
fn the_graph_rows_turn_and_step() {
    let mut app = app();
    select(&mut app, key("graphs"));
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Set(&GRAPHS, TuiValue::Bool(false)))]
    );
    assert_eq!(config_line(key("graphs"), &app.tui), "[tui] graphs = true");

    select(&mut app, key("graph_span"));
    for (from, to) in GraphSpan::ALL
        .iter()
        .zip(GraphSpan::ALL.iter().cycle().skip(1))
    {
        app.tui.graph_span = *from;
        assert_eq!(
            press(&mut app, KeyCode::Right),
            vec![Intent::Tui(TuiChange::Set(
                &GRAPH_SPAN,
                TuiValue::Word((*to).name())
            ))]
        );
        app.tui.graph_span = *to;
        assert_eq!(
            press(&mut app, KeyCode::Left),
            vec![Intent::Tui(TuiChange::Set(
                &GRAPH_SPAN,
                TuiValue::Word((*from).name())
            ))]
        );
    }
    assert_eq!(press(&mut app, KeyCode::Char(' ')), Vec::new());
    app.tui.graph_span = GraphSpan::M15;
    assert_eq!(key("graph_span").value(&app), "15m");
    assert_eq!(
        config_line(key("graph_span"), &app.tui),
        "[tui] graph_span = \"15m\""
    );

    select(&mut app, key("link_span"));
    for (from, to) in LinkSpan::ALL
        .iter()
        .zip(LinkSpan::ALL.iter().cycle().skip(1))
    {
        app.tui.link_span = *from;
        assert_eq!(
            press(&mut app, KeyCode::Right),
            vec![Intent::Tui(TuiChange::Set(
                &LINK_SPAN,
                TuiValue::Word((*to).name())
            ))]
        );
        app.tui.link_span = *to;
        assert_eq!(
            press(&mut app, KeyCode::Left),
            vec![Intent::Tui(TuiChange::Set(
                &LINK_SPAN,
                TuiValue::Word((*from).name())
            ))]
        );
    }
    assert_eq!(press(&mut app, KeyCode::Char(' ')), Vec::new());
    app.tui.link_span = LinkSpan::H24;
    assert_eq!(key("link_span").value(&app), "24h");
    assert_eq!(
        config_line(key("link_span"), &app.tui),
        "[tui] link_span = \"24h\""
    );
}

/// The three graph rows sit side by side in Look, after the visual rows, and close the
/// category; the AutoEq refresh closes Behaviour.
#[test]
fn the_graph_rows_close_the_look_category() {
    let look = rows(0);
    assert_eq!(
        &look[look.len() - 3..],
        &[key("graphs"), key("graph_span"), key("link_span")]
    );
    assert!(!look.contains(&Setting::AutoEqRefresh));
    assert_eq!(rows(2).last(), Some(&Setting::AutoEqRefresh));
}

// The Connect category. Every token here is a fixed fake one, compared with `assert!` so a
// failing case never prints it.

use std::time::Duration;

use svolder_ipc::config::McpAuth;
use svolder_ipc::token::TokenSource;
use svolder_ipc::{McpStatus, MqttStatus};

use crate::backend::DaemonInfo;
use crate::tui::app::ConnChange;
use crate::tui::boxes::connections::{self, Field, Line, Modal, TokenView};

/// The token every Connect case holds: 43 characters ending `3fQ`, not a real one.
const FAKE_TOKEN: &str = "fake-token-0123456789-abcdefghijklmnop-x3fQ";

/// An app with the Settings box open on the Connect category, as it is once the loop has read
/// the file, the token's tail and the daemon's answer.
fn connect_app() -> App {
    let mut app = app();
    app.overlays.settings = Some(SettingsBox {
        category: CONNECT,
        row: 0,
    });
    app.session.daemon = true;
    app.overlays.conn.token = TokenView::Present {
        masked: connections::mask(FAKE_TOKEN),
        source: TokenSource::Generated("/home/user/.local/state/svolder/mcp-token".into()),
    };
    app.overlays.conn.mcp_status = Some(McpStatus::Listening {
        listen: "127.0.0.1:8737".parse().unwrap(),
        auth: McpAuth::Token,
        writes: false,
    });
    app.overlays.conn.mqtt_status = Some(MqttStatus::Off);
    app
}

/// What the daemon says of itself, about this session's own configuration file.
fn daemon_with_config(config: Option<&str>) -> DaemonInfo {
    DaemonInfo {
        version: "0.0.0".into(),
        mqtt: None,
        mcp: None,
        config: config.map(str::to_string),
        mcp_token: None,
        web: None,
        web_token: None,
    }
}

/// The text of the box's rows at 80×24 joined, for a case that asks whether something shows.
fn shown(app: &App) -> String {
    box_rows(app).join("\n")
}

/// The box's right column at 80×24 as one line of words, wrapped lines rejoined with a space.
fn right_words(app: &App) -> String {
    box_rows(app)[1..21]
        .iter()
        .map(|row| row.chars().skip(45).take(33).collect::<String>())
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The Connect category at 80×24 on its first row, with the token masked.
const CONNECT_MOCKUP: [&str; 22] = [
    "┌ Settings ─ tab→ ─ ¹Look ─ ²Effects ─ ³Behaviour ─ ⁴Host audio ─┤ ⁵Connect ├──┐",
    "│ ▶ Web UI              ‹ … ›               │                                  │",
    "│   MCP server            off               │                                  │",
    "│   MCP listen            loopback:8737     │  The web UI serves this          │",
    "│   MCP writes            read-only         │  screen's controls to a browser  │",
    "│   MCP token             ••••••3fQ         │  on this computer or your        │",
    "│   MCP status            listening         │  network. Enter opens its box:   │",
    "│   MQTT bridge           off               │  on and off, how far it          │",
    "│   MQTT broker host      not set           │  reaches, a link or a pairing    │",
    "│   MQTT broker port      1883              │  code to sign a browser in, and  │",
    "│   MQTT commands         read-only         │  its token.                      │",
    "│   MQTT password         not set           │                                  │",
    "│   MQTT status           off               │  [web] enabled = false, listen   │",
    "│                                           │  = \"127.0.0.1:8738\"              │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "└ ~/.config/svolder/config.toml ──────────────────────────────────── esc close ┘",
];

#[test]
fn connect_at_80x24_draws_the_mockup() {
    assert_eq!(box_rows(&connect_app()), CONNECT_MOCKUP);
}

/// The token row is masked in the row and in the right column, and the right column says where
/// the token comes from and which keys act on it.
#[test]
fn the_token_row_is_masked_and_names_its_source_and_keys() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    let all = shown(&app);
    assert!(all.contains("••••••3fQ"), "the masked token is not drawn");
    assert!(
        !all.contains("fake-token"),
        "the box draws the token while it is hidden"
    );
    assert!(all.contains("from svolder's own file"), "{all}");
    assert!(all.contains("copies it to the clipboard"), "{all}");
}

/// A token of fewer than 16 characters is masked without a tail.
#[test]
fn a_short_token_shows_no_tail() {
    assert_eq!(connections::mask("shorttoken"), "••••••");
    assert_eq!(connections::mask(FAKE_TOKEN), "••••••3fQ");
}

/// The token is drawn in the right column while it is revealed and not a frame past its ten
/// seconds, whatever the loop has done since: the draw reads `app.now`, and a tick drops the
/// text.
#[test]
fn a_revealed_token_is_drawn_for_ten_seconds_on_app_now() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    let start = app.now;
    app.overlays
        .conn
        .reveal(FAKE_TOKEN.to_string(), start + connections::REVEAL_FOR);
    assert!(
        right_words(&app).replace(' ', "").contains(FAKE_TOKEN),
        "the revealed token is not drawn"
    );
    assert!(
        shown(&app).contains("(revealed)"),
        "the row does not say it is revealed"
    );

    app.now = start + Duration::from_secs(9);
    assert!(
        right_words(&app).replace(' ', "").contains(FAKE_TOKEN),
        "gone before its ten seconds"
    );

    app.now = start + Duration::from_secs(10);
    let all = shown(&app);
    assert!(
        !all.contains("fake-token") && !all.contains("3fQ\n"),
        "the token is still drawn at ten seconds"
    );

    app.now = start;
    app.tick(start + Duration::from_secs(11));
    assert!(
        app.overlays.conn.revealed.is_none(),
        "a tick keeps the text past its time"
    );
    app.now = start;
    assert!(
        !shown(&app)
            .replace(['│', ' ', '\n'], "")
            .contains(FAKE_TOKEN),
        "the token is drawn after the tick dropped it"
    );
}

/// A `Debug` dump of the category never carries the token.
#[test]
fn the_category_never_prints_the_token() {
    let mut app = connect_app();
    app.overlays
        .conn
        .reveal(FAKE_TOKEN.to_string(), app.now + connections::REVEAL_FOR);
    let dump = format!("{:?}", app.overlays.conn);
    assert!(
        !dump.contains("fake-token"),
        "a Debug dump carries the token"
    );
}

/// The status rows say what the daemon answers, in a word on the row and its own line in the
/// right column.
#[test]
fn the_status_rows_say_what_the_daemon_answers() {
    let mut app = connect_app();
    select(&mut app, Setting::McpStatus);
    assert!(right_words(&app).contains("mcp: listening on 127.0.0.1:8737 (token, read-only)"));
    app.overlays.conn.mcp_status = Some(McpStatus::Failed {
        listen: "127.0.0.1:8737".parse().unwrap(),
        error: "address in use".into(),
    });
    assert_eq!(Setting::McpStatus.value(&app), "failed");
    assert!(right_words(&app).contains("cannot listen on 127.0.0.1:8737: address in use"));

    app.overlays.conn.mqtt_status = Some(MqttStatus::Connected {
        host: "broker.example.com".into(),
        port: 1883,
        commands: false,
    });
    select(&mut app, Setting::MqttStatus);
    assert_eq!(Setting::MqttStatus.value(&app), "connected");
    assert!(right_words(&app).contains("mqtt: connected to broker.example.com:1883 (read-only)"));

    app.session.daemon = false;
    assert_eq!(Setting::McpStatus.value(&app), "no daemon");
    assert_eq!(Setting::MqttStatus.value(&app), "no daemon");
}

/// The password shows as set or not set and nothing more: the view cannot hold it.
#[test]
fn the_password_row_says_set_or_not_set_and_never_the_password() {
    let mut config = svolder_ipc::Config::parse(
        "schema = 1\n[mqtt]\nhost = \"broker.example.com\"\npassword = \"fake-secret-pass\"\n",
    )
    .expect("the config parses");
    let mut app = connect_app();
    app.overlays.conn.load(&config);
    select(&mut app, Setting::MqttPassword);
    assert_eq!(Setting::MqttPassword.value(&app), "set");
    assert!(!format!("{:?}", app.overlays.conn).contains("fake-secret"));
    assert!(!shown(&app).contains("fake-secret"));
    assert!(shown(&app).contains("SVOLDER_MQTT_PASSWORD"));

    config.mqtt.password = None;
    app.overlays.conn.load(&config);
    if std::env::var_os("SVOLDER_MQTT_PASSWORD").is_none() {
        assert_eq!(Setting::MqttPassword.value(&app), "not set");
    }
    assert!(
        press(&mut app, KeyCode::Enter).is_empty(),
        "the password row asked for a write"
    );
}

/// Each switch turns the other way from what the file holds, on `Space` and on either arrow.
#[test]
fn the_connect_switches_ask_for_the_other_way() {
    let mut app = connect_app();
    for (setting, want) in [
        (Setting::McpEnabled, ConnChange::McpEnabled(true)),
        (Setting::McpWrites, ConnChange::McpWrites(true)),
        (Setting::MqttEnabled, ConnChange::MqttEnabled(true)),
        (Setting::MqttCommands, ConnChange::MqttCommands(true)),
    ] {
        select(&mut app, setting);
        for key in [
            KeyCode::Char(' '),
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Enter,
        ] {
            assert_eq!(
                press(&mut app, key),
                vec![Intent::Connection(want.clone())],
                "{setting:?} on {key:?}"
            );
        }
    }
}

/// A typed row opens an edit line on what the file holds; `Enter` saves a valid value as one
/// change, an invalid one is refused with the line still up, and `Esc` cancels.
#[test]
fn the_typed_rows_open_an_edit_line_and_save_one_change() {
    let mut app = connect_app();
    select(&mut app, Setting::McpListen);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.overlays.conn.modal,
        Some(Modal::Editing(Line {
            field: Field::Listen,
            text: "127.0.0.1:8737".into(),
            caret: 14
        }))
    );
    let drawn = shown(&app);
    assert!(
        drawn.contains("MCP listen") && drawn.contains("127.0.0.1:8737"),
        "{drawn}"
    );

    for _ in 0..4 {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "x");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        app.overlays.conn.modal.is_some(),
        "a bad address closed the line"
    );
    assert!(app.toast().is_some(), "a bad address was not refused");

    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "9001");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::McpListen(
            "127.0.0.1:9001".parse().unwrap()
        ))]
    );
    assert!(app.overlays.conn.modal.is_none());

    select(&mut app, Setting::MqttHost);
    press(&mut app, KeyCode::Enter);
    type_text(&mut app, "broker.example.com");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::MqttHost(
            "broker.example.com".into()
        ))]
    );

    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    for _ in 0..4 {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "0");
    press(&mut app, KeyCode::Enter);
    assert!(app.overlays.conn.modal.is_some(), "port 0 was taken");
    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "8883");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::MqttPort(8883))]
    );

    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(
        app.overlays.conn.modal.is_none() && app.overlays.settings.is_some(),
        "Esc closed the box too"
    );
}

/// An unchanged value closes the line and asks for nothing.
#[test]
fn saving_an_unchanged_value_asks_for_nothing() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.overlays.conn.modal.is_none());
}

/// `v`, `c` and `R` on the token row ask for what they say; `R` asks first, and only a yes
/// replaces the token.
#[test]
fn the_token_keys_reveal_copy_and_rotate_after_a_question() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    assert_eq!(
        press(&mut app, KeyCode::Char('v')),
        vec![Intent::RevealToken]
    );
    assert_eq!(press(&mut app, KeyCode::Char('c')), vec![Intent::CopyToken]);

    assert!(press(&mut app, KeyCode::Char('R')).is_empty());
    assert_eq!(app.overlays.conn.modal, Some(Modal::Rotating));
    assert!(shown(&app).contains("Replace the MCP token? [y/N]"));
    for no in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
        assert!(press(&mut app, no).is_empty());
        assert!(app.overlays.conn.modal.is_none(), "{no:?} did not cancel");
        press(&mut app, KeyCode::Char('R'));
    }
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::RotateToken]
    );
    assert!(app.overlays.conn.modal.is_none());

    // `v` while the token shows hides it.
    app.overlays
        .conn
        .reveal(FAKE_TOKEN.to_string(), app.now + connections::REVEAL_FOR);
    assert!(press(&mut app, KeyCode::Char('v')).is_empty());
    assert!(app.overlays.conn.revealed.is_none());
}

/// A token from the environment or a file is not rotated: the key says so, naming the source,
/// and no question is asked.
#[test]
fn rotating_an_outside_token_is_refused_naming_the_source() {
    for (source, named) in [
        (TokenSource::Env("SVOLDER_MCP_TOKEN"), "SVOLDER_MCP_TOKEN"),
        (
            TokenSource::File("/run/secrets/mcp".into()),
            "/run/secrets/mcp",
        ),
    ] {
        let mut app = connect_app();
        app.overlays.conn.token = TokenView::Present {
            masked: connections::mask(FAKE_TOKEN),
            source,
        };
        select(&mut app, Setting::McpToken);
        assert!(press(&mut app, KeyCode::Char('R')).is_empty());
        assert!(
            app.overlays.conn.modal.is_none(),
            "{named}: a question was asked"
        );
        let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
        assert!(toast.contains(named), "{named}: {toast}");
    }
}

/// Where the daemon reads another configuration, every Connect row is read-only and the right
/// column says where that file is.
#[test]
fn a_daemon_reading_another_config_makes_every_row_read_only() {
    let mut app = connect_app();
    let own = app.config_path.clone();
    app.overlays.conn.daemon_said(
        &daemon_with_config(Some("/config/svolder/config.toml")),
        &own,
    );
    assert_eq!(
        app.overlays.conn.elsewhere.as_deref(),
        Some("/config/svolder/config.toml")
    );
    for setting in CATEGORIES[CONNECT].1 {
        select(&mut app, *setting);
        let mut keys = vec![KeyCode::Char(' '), KeyCode::Enter, KeyCode::Left];
        if *setting == Setting::McpToken {
            keys.extend([KeyCode::Char('v'), KeyCode::Char('c'), KeyCode::Char('R')]);
        }
        for key in keys {
            // The Web UI row opens its box, whose own writes are refused as these rows' are.
            if *setting == Setting::WebUi {
                assert!(
                    press(&mut app, key).is_empty(),
                    "the Web UI box asked for a link"
                );
                assert!(
                    press(&mut app, KeyCode::Enter).is_empty(),
                    "the Web UI box wrote on a daemon that reads another file"
                );
                let _ = press(&mut app, KeyCode::Esc);
                continue;
            }
            assert!(
                press(&mut app, key).is_empty(),
                "{setting:?} on {key:?} asked for something"
            );
            assert!(
                app.overlays.conn.modal.is_none(),
                "{setting:?} on {key:?} opened a box"
            );
        }
        assert!(
            right_words(&app).contains("the daemon reads /config/svolder/config.toml"),
            "{setting:?}: the right column does not say where the daemon's file is"
        );
    }
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("/config/svolder/config.toml"), "{toast}");
}

/// A daemon reading this very file, spelled the same or resolving to it, leaves the rows
/// editable; so does one too old to say.
#[test]
fn a_daemon_reading_the_same_config_leaves_the_rows_editable() {
    let mut app = connect_app();
    let own = app.config_path.clone();
    let spelled = own.display().to_string();
    app.overlays
        .conn
        .daemon_said(&daemon_with_config(Some(&spelled)), &own);
    assert!(app.overlays.conn.elsewhere.is_none());
    app.overlays
        .conn
        .daemon_said(&daemon_with_config(None), &own);
    assert!(app.overlays.conn.elsewhere.is_none());
    select(&mut app, Setting::McpEnabled);
    assert!(!press(&mut app, KeyCode::Char(' ')).is_empty());
}

/// Moving to the Connect category with Tab or its digit works like any other, and a box open on
/// it swallows the keys that would leave.
#[test]
fn the_connect_box_swallows_every_key_while_open() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    for key in [KeyCode::Tab, KeyCode::Char('1'), KeyCode::Down] {
        press(&mut app, key);
    }
    assert_eq!(
        app.overlays.settings.map(|at| at.category),
        Some(CONNECT),
        "a key left the category with a line open"
    );
    // The rows under a box take no click: a click outside the edit line cancels it, and
    // one inside it asks for nothing.
    let (_, _, hits) = drawn(&app, 80, 24);
    assert_eq!(
        mouse::resolve(&hits, click(5, 3), &app),
        Some(MouseAction::Key(KeyCode::Esc)),
        "a row took a click under the box"
    );
    let top = hits.top_box().expect("the edit line is a box");
    assert!(top.width < 80, "the edit line is a box of its own: {top:?}");
    assert_eq!(
        mouse::resolve(&hits, click(top.x + 1, top.y + 1), &app),
        None
    );
}

/// A click outside the Connect edit line cancels the edit and leaves the Settings box open.
#[test]
fn a_click_outside_the_connect_edit_line_cancels_it_and_keeps_the_box() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    assert!(
        app.overlays.conn.modal.is_some(),
        "Enter opened no edit line"
    );
    let (_, _, hits) = drawn(&app, 80, 24);
    let Some(MouseAction::Key(code)) = mouse::resolve(&hits, click(0, 0), &app) else {
        panic!("a click outside did not press a key");
    };
    assert_eq!(code, KeyCode::Esc);
    press(&mut app, code);
    assert!(app.overlays.conn.modal.is_none(), "the edit stayed open");
    assert!(
        app.overlays.settings.is_some(),
        "the box closed with the edit"
    );
}

/// Where the daemon's token is not the one this session reads, the row shows the daemon's tail,
/// the right column says where its token lives, and `v`, `c` and `R` refuse with that reason.
#[test]
fn a_token_the_daemon_does_not_use_is_labelled_and_its_keys_refuse() {
    use svolder_ipc::{TokenInfo, TokenKind};

    let mut app = connect_app();
    let mut info = daemon_with_config(None);
    info.mcp_token = Some(TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: "9xZ".into(),
    });
    let own = app.config_path.clone();
    app.overlays.conn.daemon_said(&info, &own);
    select(&mut app, Setting::McpToken);
    assert_eq!(Setting::McpToken.value(&app), "••••••9xZ");
    assert!(
        right_words(&app).contains("in the daemon's environment (SVOLDER_MCP_TOKEN)"),
        "{}",
        right_words(&app)
    );
    assert!(!shown(&app).contains("3fQ"), "this session's tail is drawn");
    for key in ['v', 'c', 'R'] {
        assert!(press(&mut app, KeyCode::Char(key)).is_empty(), "{key}");
        assert!(app.overlays.conn.modal.is_none());
        let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
        assert!(toast.contains("SVOLDER_MCP_TOKEN"), "{key}: {toast}");
    }

    info.mcp_token = Some(TokenInfo {
        kind: TokenKind::State,
        path: Some("/var/lib/svolder/mcp-token".into()),
        tail: "3fQ".into(),
    });
    app.overlays.conn.daemon_said(&info, &own);
    assert!(
        right_words(&app)
            .contains("at /var/lib/svolder/mcp-token, not the file this session reads"),
        "{}",
        right_words(&app)
    );
}

/// With no daemon answering the row is this session's own reading, marked as such, and the
/// token keys work on it.
#[test]
fn with_no_daemon_the_row_is_the_sessions_own_and_marked() {
    let mut app = connect_app();
    app.overlays.conn.daemon_missing();
    select(&mut app, Setting::McpToken);
    assert_eq!(Setting::McpToken.value(&app), "••••••3fQ");
    assert!(
        right_words(&app).contains("(daemon not running)"),
        "{}",
        right_words(&app)
    );
    assert_eq!(
        press(&mut app, KeyCode::Char('v')),
        vec![Intent::RevealToken]
    );
    assert_eq!(press(&mut app, KeyCode::Char('c')), vec![Intent::CopyToken]);
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(app.overlays.conn.modal, Some(Modal::Rotating));
}

/// The token keys are in the key table, so the help overlay lists them.
#[test]
fn the_token_keys_are_in_the_key_table() {
    let hints = crate::tui::keys::hints_for(Screen::Settings);
    for label in ["v", "c", "R"] {
        assert!(
            hints
                .iter()
                .any(|hint| hint.label == label && hint.does.contains("token")),
            "{label} is not listed for the Settings box"
        );
    }
}

/// An edit line opened before the daemon says it reads another file saves nothing: Enter on it
/// is refused with the same words as any key on a read-only row.
#[test]
fn an_edit_line_opened_before_an_elsewhere_answer_does_not_save() {
    let mut app = connect_app();
    select(&mut app, Setting::McpListen);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Backspace);
    assert!(matches!(app.overlays.conn.modal, Some(Modal::Editing(_))));
    let own = app.config_path.clone();
    app.overlays.conn.daemon_said(
        &daemon_with_config(Some("/config/svolder/config.toml")),
        &own,
    );
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.overlays.conn.modal.is_none());
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("read-only here"), "{toast}");
}

/// The question before the token is replaced is answered against what is known when it is
/// answered: a daemon that has said it uses another token since the question went up turns a
/// yes into a refusal.
#[test]
fn a_rotate_question_answered_after_a_differs_answer_does_not_rotate() {
    use svolder_ipc::{TokenInfo, TokenKind};

    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(app.overlays.conn.modal, Some(Modal::Rotating));
    let mut info = daemon_with_config(None);
    info.mcp_token = Some(TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: "9xZ".into(),
    });
    let own = app.config_path.clone();
    app.overlays.conn.daemon_said(&info, &own);
    assert!(press(&mut app, KeyCode::Char('y')).is_empty());
    assert!(app.overlays.conn.modal.is_none());
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("not this session's token"), "{toast}");
}

/// The AutoEq refresh row steps `never`, `1 week`, `2 weeks` … `99 weeks` one at a time with the
/// arrows and stops at both ends instead of wrapping; Space turns nothing.
#[test]
fn the_autoeq_refresh_row_steps_and_stops_at_the_ends() {
    use svolder_ipc::config::RefreshWeeks::{Never, Weeks};
    let step = |app: &mut App, key| match press(app, key).as_slice() {
        [Intent::AutoEqRefresh(weeks)] => Some(*weeks),
        [] => None,
        other => panic!("unexpected {other:?}"),
    };
    let mut app = app();
    select(&mut app, Setting::AutoEqRefresh);
    assert_eq!(
        Setting::AutoEqRefresh.value(&app),
        "13 weeks",
        "the default"
    );

    app.presets.autoeq_weeks = Weeks(13);
    assert_eq!(step(&mut app, KeyCode::Right), Some(Weeks(14)));
    assert_eq!(step(&mut app, KeyCode::Left), Some(Weeks(12)));

    app.presets.autoeq_weeks = Weeks(99);
    assert_eq!(step(&mut app, KeyCode::Right), None, "99 weeks is the top");
    assert_eq!(step(&mut app, KeyCode::Left), Some(Weeks(98)));

    app.presets.autoeq_weeks = Weeks(1);
    assert_eq!(step(&mut app, KeyCode::Left), Some(Never));
    assert_eq!(step(&mut app, KeyCode::Right), Some(Weeks(2)));

    app.presets.autoeq_weeks = Never;
    assert_eq!(step(&mut app, KeyCode::Left), None, "never is the bottom");
    assert_eq!(step(&mut app, KeyCode::Right), Some(Weeks(1)));
    assert_eq!(press(&mut app, KeyCode::Char(' ')), Vec::new());
}

/// The row reads the refresh the way the file holds it, and its right column says where.
#[test]
fn the_autoeq_refresh_row_reads_the_file_s_value() {
    use svolder_ipc::config::RefreshWeeks::{Never, Weeks};
    let mut app = app();
    for (weeks, value, line) in [
        (Never, "never", "[autoeq] refresh_weeks = \"never\""),
        (Weeks(1), "1 week", "[autoeq] refresh_weeks = 1"),
        (Weeks(2), "2 weeks", "[autoeq] refresh_weeks = 2"),
        (Weeks(99), "99 weeks", "[autoeq] refresh_weeks = 99"),
    ] {
        app.presets.autoeq_weeks = weeks;
        assert_eq!(Setting::AutoEqRefresh.value(&app), value);
        assert_eq!(super::autoeq_line(weeks), line);
    }
    select(&mut app, Setting::AutoEqRefresh);
    let rows = box_rows(&app).join("\n");
    assert!(rows.contains("AutoEq refresh"), "{rows}");
    assert!(rows.contains("[autoeq] refresh_weeks = 99"), "{rows}");
}

/// Taking the configuration in reads the refresh from `[autoeq]`, the default 13 weeks while
/// the table is absent.
#[test]
fn the_app_takes_the_refresh_from_the_config() {
    use svolder_ipc::config::RefreshWeeks::{Never, Weeks};
    let mut app = plain();
    assert_eq!(app.presets.autoeq_weeks, Weeks(13));
    let config = svolder_ipc::Config::parse("schema = 1\n[autoeq]\nrefresh_weeks = \"never\"\n")
        .expect("a config");
    app.take_config(&config);
    assert_eq!(app.presets.autoeq_weeks, Never);
}

/// Every row of every category as one line (category, label, value, explain and config line,
/// tab-separated, `\n` and `\t` escaped), as the file `rows.golden` holds them.
fn rows_text() -> String {
    let app = plain();
    let escaped = |text: &str| text.replace('\t', "\\t").replace('\n', "\\n");
    let mut out = String::new();
    for (category, rows) in CATEGORIES {
        for setting in rows {
            let fields = [
                category,
                setting.label(),
                &setting.value(&app),
                &explain(*setting),
                &config_line(*setting, &app.tui),
            ];
            let fields: Vec<String> = fields.iter().map(|field| escaped(field)).collect();
            out.push_str(&fields.join("\t"));
            out.push('\n');
        }
    }
    out
}

/// A new row, or a changed label, value, help or config line, shows in `rows.golden` as a
/// diff.
#[test]
fn the_rows_are_the_golden_rows() {
    assert_eq!(rows_text(), include_str!("rows.golden"));
}

/// No row is listed in two places.
#[test]
fn every_setting_is_listed_once() {
    let all = every_setting();
    for (at, setting) in all.iter().enumerate() {
        assert!(
            !all[..at].contains(setting),
            "{setting:?} is in CATEGORIES twice"
        );
    }
}

/// Every plain key a row names is an entry of the table, and every entry of the table has a
/// row: a `Setting::Key` of its own, or a named row that carries its label.
#[test]
fn the_rows_and_the_table_name_the_same_keys() {
    let rows = every_setting();
    for setting in &rows {
        if let Setting::Key(key) = setting {
            assert!(
                TUI_SETTINGS.iter().any(|entry| entry == key),
                "{key:?} is not an entry of TUI_SETTINGS"
            );
        }
    }
    for entry in TUI_SETTINGS {
        // `accent` has one named row per family and no label of its own.
        if entry.key == "accent" {
            continue;
        }
        assert!(
            rows.iter().any(|setting| match setting {
                Setting::Key(key) => key == &entry,
                named => named.label() == entry.label,
            }),
            "{entry:?} has no row"
        );
    }
}

/// A path under a home named in wide glyphs is fitted by cells: never wider than its room, and
/// a wide glyph straddling the cut is dropped whole rather than split.
#[test]
fn a_path_with_wide_glyphs_is_fitted_by_cells() {
    let path = "/home/世界/.config/svolder/config.toml";
    for room in 0..=widgets::text::cells(path) {
        let fitted = fitted(path, room);
        assert!(
            widgets::text::cells(&fitted) <= room,
            "room {room}: |{fitted}|"
        );
    }
    assert_eq!(fitted(path, 22), "…/svolder/config.toml");
    // A file name of wide glyphs alone keeps its tail, a glyph the room cannot hold whole left
    // out.
    let name = "世界世界世界";
    assert_eq!(fitted(name, 6), "…世界");
    assert_eq!(fitted(name, 4), "…界");
    assert_eq!(fitted(name, 3), "…界");
    assert_eq!(fitted(name, 2), "…");
}

/// An accent the palette's stops lack, written by hand or another palette's, stays a stop of its
/// own ahead of the preset's: Right goes on to the preset's own colour and Left comes back to it;
/// Left from it wraps to the palette's last colour and Right from there comes back.
#[test]
fn an_accent_the_cycle_lacks_stays_a_stop_to_step_back_to() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    app.tui.accents.dac = Some("#e0341c".into());
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(Family::Dac, None))],
    );
    app.tui.accents.dac = None;
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#e0341c".into())
        ))],
        "the colour of the user's own was lost"
    );
    app.tui.accents.dac = Some("#e0341c".into());
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#f6cc7a".into())
        ))],
    );
    app.tui.accents.dac = Some("#f6cc7a".into());
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#e0341c".into())
        ))],
    );
}

/// With a palette name no preset carries the only stops are the user's own colour and the
/// palette's: the arrows go between the two.
#[test]
fn an_accent_row_on_an_unknown_palette_steps_between_the_own_colour_and_the_palettes() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    app.tui.palette = "no-such-palette".into();
    app.tui.accents.dac = Some("#123456".into());
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(Family::Dac, None))],
    );
    app.tui.accents.dac = None;
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#123456".into())
        ))],
    );
}

/// A bare `accent` that a step clears is held for all three rows: once the file reads `(preset)`
/// on another family's row, the colour is still a stop there.
#[test]
fn a_bare_accent_is_held_for_every_family() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    app.tui.accent = Some("#e0341c".into());
    press(&mut app, KeyCode::Right);
    app.tui.accent = None;
    select(&mut app, Setting::Accent(Family::Amp));
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Amp,
            Some("#e0341c".into())
        ))],
        "the colour was held for the stepped family only"
    );
}

/// A bare `accent` on file is a colour of the row's own: stepping the row holds it.
#[test]
fn a_bare_accent_is_held_as_the_rows_own_colour() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    app.tui.accent = Some("#0000ff".into());
    press(&mut app, KeyCode::Right);
    assert_eq!(app.overlays.held_accents.get(Family::Dac), Some("#0000ff"));
}

// The Web UI box.

/// The Connect category opened on the Web UI row of a daemon whose web UI is off, with the box
/// opened by Enter.
fn web_box_app() -> App {
    let mut app = connect_app();
    app.overlays.conn.daemon_up = Some(true);
    app.overlays.conn.web.listen = "127.0.0.1:8738".parse().unwrap();
    app.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Off);
    select(&mut app, Setting::WebUi);
    let asked = press(&mut app, KeyCode::Enter);
    assert!(asked.is_empty(), "a link was asked of a web UI that is off");
    assert!(matches!(app.overlays.conn.modal, Some(Modal::Web(_))));
    app
}

/// The box's title is its name alone: the web UI is not called experimental there.
#[test]
fn the_web_boxs_title_does_not_say_experimental() {
    let mut app = web_box_app();
    let rows = shown(&app);
    assert!(rows.contains("Web UI"), "{rows}");
    assert!(!rows.contains("experimental"), "{rows}");
    let _ = press(&mut app, KeyCode::Esc);
}

/// Enter on Web UI writes `enabled`, ←→ on Reach writes the listen address with its port, `l`
/// and `p` ask for codes, `c` copies a link on show, `r` then `y` replaces the token, and Esc
/// closes the box and forgets its codes.
#[test]
fn the_web_boxs_keys_ask_for_what_their_rows_change() {
    let mut app = web_box_app();
    assert!(shown(&app).contains("Web UI"), "{}", shown(&app));
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::WebEnabled(true))]
    );
    let _ = press(&mut app, KeyCode::Down);
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Connection(ConnChange::WebListen(
            "0.0.0.0:8738".parse().unwrap()
        ))]
    );
    assert_eq!(press(&mut app, KeyCode::Char('l')), vec![Intent::WebLink]);
    assert_eq!(press(&mut app, KeyCode::Char('p')), vec![Intent::WebPair]);
    assert!(
        press(&mut app, KeyCode::Char('c')).is_empty(),
        "a copy with no link on show"
    );
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link = Some(crate::tui::boxes::web_box::Code {
            text: "http://192.168.1.20:8738/#login=abc".into(),
            until: std::time::Instant::now() + std::time::Duration::from_secs(60),
        });
    }
    assert_eq!(
        press(&mut app, KeyCode::Char('c')),
        vec![Intent::CopyWebLink]
    );
    assert!(press(&mut app, KeyCode::Char('r')).is_empty());
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::RotateWebToken]
    );
    let _ = press(&mut app, KeyCode::Esc);
    assert!(app.overlays.conn.modal.is_none());
}

/// Enter on the Link row asks for a link as `l` does, and on the Pairing code row for a code as
/// `p` does.
#[test]
fn enter_on_the_link_and_pairing_rows_asks_as_l_and_p_do() {
    let mut app = web_box_app();
    let _ = press(&mut app, KeyCode::Down);
    let _ = press(&mut app, KeyCode::Down);
    assert_eq!(press(&mut app, KeyCode::Enter), vec![Intent::WebLink]);
    let _ = press(&mut app, KeyCode::Down);
    assert_eq!(press(&mut app, KeyCode::Enter), vec![Intent::WebPair]);
    let _ = press(&mut app, KeyCode::Down);
    assert!(
        press(&mut app, KeyCode::Enter).is_empty(),
        "Enter on the Token row asked for something"
    );
    // With no daemon, Enter says what `l` says and asks for nothing.
    app.overlays.conn.daemon_up = Some(false);
    let _ = press(&mut app, KeyCode::Up);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
}

/// The question before the web token is replaced shows its answers, `y` among them, in the
/// style of the quit question; Enter and `y` answer Yes, `n` and Esc answer No.
#[test]
fn the_web_token_question_shows_y_and_enter_answers_yes() {
    let mut app = web_box_app();
    let _ = press(&mut app, KeyCode::Char('r'));
    let words = shown(&app);
    assert!(words.contains("[Y/n]"), "{words}");
    assert!(words.contains("Enter/y replaces"), "{words}");
    assert!(words.contains("Esc/n/q cancels"), "{words}");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::RotateWebToken]
    );
    for answer in [KeyCode::Char('n'), KeyCode::Esc] {
        let _ = press(&mut app, KeyCode::Char('r'));
        assert!(press(&mut app, answer).is_empty());
        assert!(matches!(
            &app.overlays.conn.modal,
            Some(Modal::Web(web)) if !web.rotating
        ));
    }
    let _ = press(&mut app, KeyCode::Char('r'));
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::RotateWebToken]
    );
}

/// Opening the box on a web UI that listens asks for a link at once, for its QR code.
#[test]
fn opening_the_box_on_a_listening_web_ui_asks_for_a_link() {
    let mut app = connect_app();
    app.overlays.conn.daemon_up = Some(true);
    app.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Listening {
        listen: "0.0.0.0:8738".parse().unwrap(),
        auth: svolder_ipc::config::WebAuth::Token,
        writes: true,
        guest: svolder_ipc::config::WebGuest::None,
    });
    select(&mut app, Setting::WebUi);
    assert_eq!(press(&mut app, KeyCode::Enter), vec![Intent::WebLink]);
}

/// On a direct session the code rows say the daemon serves the web UI, and `l` and `p` ask for
/// nothing.
#[test]
fn a_direct_session_asks_the_web_box_for_no_codes() {
    let mut app = web_box_app();
    app.overlays.conn.daemon_up = Some(false);
    assert!(press(&mut app, KeyCode::Char('l')).is_empty());
    assert!(press(&mut app, KeyCode::Char('p')).is_empty());
    assert!(
        shown(&app).contains("the daemon serves the web UI"),
        "{}",
        shown(&app)
    );
}

/// `q` in the Web UI box closes it as `Esc` does: no quit question, the Settings box where it was.
#[test]
fn q_closes_the_web_ui_window_as_esc_does() {
    let mut app = web_box_app();
    let at = app.overlays.settings;
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.conn.modal.is_none(), "the box stayed open");
    assert!(!app.overlays.quitting, "q asked to quit");
    assert_eq!(app.overlays.settings, at, "the Settings box moved");
}

/// The same while a link is on show.
#[test]
fn q_in_the_web_ui_window_with_a_link_showing_closes_it() {
    let mut app = web_box_with_link("0.0.0.0:8738");
    let at = app.overlays.settings;
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.conn.modal.is_none(), "the box stayed open");
    assert!(!app.overlays.quitting, "q asked to quit");
    assert_eq!(app.overlays.settings, at, "the Settings box moved");
}

/// The same while a pairing code is on show.
#[test]
fn q_in_the_web_ui_window_with_a_pairing_code_showing_closes_it() {
    let mut app = web_box_app();
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.pair = Some(crate::tui::boxes::web_box::Code {
            text: "482913".into(),
            until: app.now + std::time::Duration::from_secs(90),
        });
    }
    let at = app.overlays.settings;
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.conn.modal.is_none(), "the box stayed open");
    assert!(!app.overlays.quitting, "q asked to quit");
    assert_eq!(app.overlays.settings, at, "the Settings box moved");
}

/// `q` declines the question before the web token is replaced, as `Esc` does: the question
/// goes, the Web UI box stays, no quit question.
#[test]
fn q_declines_the_web_token_question_as_esc_does() {
    let mut app = web_box_app();
    for no in [KeyCode::Char('q'), KeyCode::Esc] {
        let _ = press(&mut app, KeyCode::Char('r'));
        assert!(
            shown(&app).contains("Replace the web token?"),
            "{}",
            shown(&app)
        );
        assert!(press(&mut app, no).is_empty());
        assert!(
            !shown(&app).contains("Replace the web token?"),
            "{no:?} did not decline"
        );
        assert!(matches!(app.overlays.conn.modal, Some(Modal::Web(_))));
        assert!(!app.overlays.quitting, "{no:?} asked to quit");
    }
}

/// `q` declines the question before the MCP token is replaced, as `Esc` does, and never asks to
/// quit.
#[test]
fn q_declines_the_mcp_token_question_as_esc_does() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    assert!(press(&mut app, KeyCode::Char('R')).is_empty());
    assert_eq!(app.overlays.conn.modal, Some(Modal::Rotating));
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.conn.modal.is_none(), "q did not decline");
    assert!(!app.overlays.quitting, "q asked to quit");
    assert!(app.overlays.settings.is_some(), "the Settings box closed");
}

/// With the Web UI box closed, `q` on Connect still asks to quit.
#[test]
fn q_on_settings_with_no_window_open_still_asks_to_quit() {
    let mut app = connect_app();
    assert!(app.overlays.conn.modal.is_none());
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.quitting && app.overlays.settings.is_some());
}

/// The Web UI box with a link on show, on a listen address `listen`.
fn web_box_with_link(listen: &str) -> App {
    let mut app = web_box_app();
    app.overlays.conn.web.listen = listen.parse().unwrap();
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link = Some(crate::tui::boxes::web_box::Code {
            text: "http://192.168.1.20:8738/#login=abc".into(),
            until: app.now + std::time::Duration::from_secs(90),
        });
    }
    app
}

/// How many cells of `buffer` are painted white, which only the QR code is.
fn white_cells(buffer: &Buffer) -> usize {
    buffer
        .content()
        .iter()
        .filter(|cell| cell.style().bg == Some(ratatui::style::Color::White))
        .count()
}

/// With room, the box draws the link and its QR code black on white under its rows; without,
/// it says to widen the terminal; on a loopback listen it says a phone cannot reach it.
#[test]
fn the_web_box_draws_the_links_qr_code_where_it_has_room() {
    let app = web_box_with_link("0.0.0.0:8738");
    let big = frame(&app, 120, 60);
    let rows = text(&big).join("\n");
    assert!(rows.contains("Web UI"), "{rows}");
    assert!(
        rows.contains("http://192.168.1.20:8738/#login=abc"),
        "{rows}"
    );
    assert!(rows.contains('█'), "{rows}");
    assert!(white_cells(&big) > 100, "the QR code is not areas white");
    let small = text(&frame(&app, 80, 24)).join("\n");
    assert!(
        small.contains("widen the terminal to see the QR code"),
        "{small}"
    );
    assert_eq!(white_cells(&frame(&app, 80, 24)), 0);

    let loopback = web_box_with_link("127.0.0.1:8738");
    let shown = text(&frame(&loopback, 120, 60)).join("\n");
    let said = "a phone cannot reach this computer's loopback: choose your";
    assert!(shown.contains(said), "{shown}");
    assert_eq!(white_cells(&frame(&loopback, 120, 60)), 0);
}

/// A pairing code shows in two threes with its countdown, and with it the way round a code the
/// network used up.
#[test]
fn the_web_box_shows_a_pairing_code_and_its_fallback() {
    let mut app = web_box_app();
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.pair = Some(crate::tui::boxes::web_box::Code {
            text: "482913".into(),
            until: app.now + std::time::Duration::from_secs(118),
        });
    }
    let rows = shown(&app);
    assert!(rows.contains("482 913 · 1:58 left"), "{rows}");
    assert!(rows.contains("the link is the fallback"), "{rows}");
}

/// Replacing the web token asks first; a token svolder does not own is refused, naming where it
/// comes from, and one it does own is replaced on `y`.
#[test]
fn the_web_token_is_replaced_after_a_question_and_only_when_svolder_owns_it() {
    let mut app = web_box_app();
    app.overlays.conn.web_token = TokenView::Present {
        masked: connections::mask(FAKE_TOKEN),
        source: TokenSource::Env("SVOLDER_WEB_TOKEN"),
    };
    let _ = press(&mut app, KeyCode::Char('r'));
    assert!(
        shown(&app).contains("Replace the web token?"),
        "{}",
        shown(&app)
    );
    assert!(press(&mut app, KeyCode::Char('y')).is_empty());
    let said = app
        .toast()
        .map(|toast| toast.text.clone())
        .unwrap_or_default();
    assert!(said.contains("SVOLDER_WEB_TOKEN"), "{said}");
    app.overlays.conn.web_token = TokenView::Present {
        masked: connections::mask(FAKE_TOKEN),
        source: TokenSource::Generated("/home/user/.local/state/svolder/web-token".into()),
    };
    let _ = press(&mut app, KeyCode::Char('r'));
    assert!(press(&mut app, KeyCode::Char('n')).is_empty());
    assert!(!shown(&app).contains("Replace the web token?"));
    let _ = press(&mut app, KeyCode::Char('r'));
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::RotateWebToken]
    );
}

/// A link on this computer's own base, because none a phone reaches was found, draws the words
/// that say so instead of a QR code a phone cannot open.
#[test]
fn a_link_on_the_fallback_base_draws_words_not_a_qr_code() {
    let mut app = web_box_with_link("0.0.0.0:8738");
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link_local = true;
    }
    let big = frame(&app, 120, 60);
    let rows = text(&big).join("\n");
    assert!(rows.contains("a phone cannot open this link"), "{rows}");
    assert_eq!(white_cells(&big), 0, "a QR code was drawn");
}

/// A link that replaced one a browser used says so under its hint; a link that did not says
/// nothing of the kind.
#[test]
fn a_replaced_link_says_the_last_one_was_used() {
    let mut app = web_box_with_link("0.0.0.0:8738");
    let rows = text(&frame(&app, 120, 60)).join("\n");
    assert!(!rows.contains("was used"), "{rows}");
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link_replaced = true;
    }
    let rows = text(&frame(&app, 120, 60)).join("\n");
    assert!(
        rows.contains("the last link no longer works; a fresh one is shown"),
        "{rows}"
    );
}

/// The cell `(column, row)` where `needle` starts in the frame `app` draws at `width`×`height`.
fn find_at(app: &App, width: u16, height: u16, needle: &str) -> (u16, u16) {
    for (row, line) in text(&frame(app, width, height)).iter().enumerate() {
        if let Some(byte) = line.find(needle) {
            let column = line[..byte].chars().count();
            return (u16::try_from(column).unwrap(), u16::try_from(row).unwrap());
        }
    }
    panic!("{needle:?} is not drawn");
}

/// A left click on the link's text, on every line of it where it wraps, asks to open it; on its
/// label, on another row, or with no link on show, it asks for nothing.
#[test]
fn a_click_on_the_shown_link_asks_to_open_it() {
    let mut app = web_box_with_link("0.0.0.0:8738");
    let (column, row) = find_at(&app, 80, 24, "http://192.168.1.20");
    let open = Some(MouseAction::Intent(Intent::OpenWebLink));
    assert_eq!(clicked(&app, column, row), open);
    assert_eq!(clicked(&app, column + 10, row), open);
    let (label, _) = find_at(&app, 80, 24, "State");
    assert_eq!(
        clicked(&app, label, row - 3),
        None,
        "a click on another row"
    );
    // A link that wraps: both lines are the link.
    let long = format!("http://192.168.1.20:8738/#login={}", "a".repeat(70));
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link.as_mut().unwrap().text = long;
    }
    let (column, first) = find_at(&app, 80, 24, "http://192.168.1.20");
    assert_eq!(clicked(&app, column, first), open);
    assert_eq!(clicked(&app, column, first + 1), open, "the wrapped line");
    // No link: nothing on the row to click.
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link = None;
    }
    assert_eq!(clicked(&app, column, first), None);
}

/// The Web UI box with a link of `tail` letters after the usual base: 32 characters and `tail`.
fn web_box_with_long_link(tail: usize) -> (App, String) {
    let mut app = web_box_with_link("0.0.0.0:8738");
    let link = format!("http://192.168.1.20:8738/#login={}", "a".repeat(tail));
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link.as_mut().unwrap().text = link.clone();
    }
    (app, link)
}

/// The link sits on lines of its own, as wide as the box's text area less a cell of margin each
/// side, with no timer, label or border inside its text; the timer is on the label's line.
#[test]
fn the_link_is_on_lines_of_its_own_at_the_full_width_of_the_box() {
    let (app, link) = web_box_with_long_link(70);
    let rows = text(&frame(&app, 80, 24));
    let (column, first) = find_at(&app, 80, 24, "http://192.168.1.20");
    let cells = |row: u16| rows[usize::from(row)].chars().collect::<Vec<_>>();
    let head = cells(first);
    let column = usize::from(column);
    let width = 60;
    assert_eq!(head[column - 1], ' ', "a margin before the link");
    assert_eq!(
        head[column..column + width].iter().collect::<String>(),
        link[..width]
    );
    assert_eq!(head[column + width], ' ', "a margin after the first line");
    assert_eq!(
        head[column + width + 1],
        '│',
        "the border follows the margin"
    );
    let tail = cells(first + 1);
    let rest: String = tail[column..column + link.len() - width].iter().collect();
    assert_eq!(
        rest,
        link[width..],
        "the wrapped line is the rest of the link"
    );
    assert!(
        tail[column + link.len() - width..column + width + 1]
            .iter()
            .all(|c| *c == ' '),
        "nothing after the link's last piece"
    );
    let label = cells(first - 1).iter().collect::<String>();
    assert!(
        label.contains("Link") && label.contains("1:30 left"),
        "{label}"
    );
}

/// Under the pointer every line of a wrapped link underlines, and none does without it.
#[test]
fn every_line_of_a_wrapped_link_underlines_on_hover() {
    let (mut app, _) = web_box_with_long_link(70);
    let (column, first) = find_at(&app, 80, 24, "http://192.168.1.20");
    let underlined = |app: &App, row: u16, x: u16| {
        frame(app, 80, 24)[(x, row)]
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED)
    };
    for row in [first, first + 1] {
        assert!(
            !underlined(&app, row, column),
            "underlined with no pointer on it"
        );
    }
    app.pointer.over_link = true;
    for row in [first, first + 1] {
        assert!(underlined(&app, row, column), "row {row} did not underline");
        assert!(
            underlined(&app, row, column + 5),
            "row {row} did not underline"
        );
    }
    assert!(
        !underlined(&app, first - 1, column),
        "the label's line underlined with the link"
    );
}

/// A click on the last cell of each full-width line takes the link, and the cell after does not.
#[test]
fn a_click_on_any_cell_of_a_wrapped_links_lines_opens_it() {
    let (app, _) = web_box_with_long_link(70);
    let (column, first) = find_at(&app, 80, 24, "http://192.168.1.20");
    let open = Some(MouseAction::Intent(Intent::OpenWebLink));
    assert_eq!(clicked(&app, column + 59, first), open);
    assert_eq!(clicked(&app, column + 60, first), None);
    assert_eq!(clicked(&app, column + 41, first + 1), open);
    assert_eq!(clicked(&app, column + 42, first + 1), None);
}

/// Under a link the box says what the terminal's own link handling does with it.
#[test]
fn the_box_says_how_to_use_the_link_as_a_terminal_link() {
    let (app, _) = web_box_with_long_link(10);
    let rows = shown(&app);
    assert!(rows.contains("Ctrl+click"), "{rows}");
    assert!(rows.contains("Shift+right-click copies"), "{rows}");
}
