// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Settings box on its own: opening and closing, the borders, the Look category, the keys that
//! move and flip its rows, and the look rows' values.

use super::*;

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
    assert_eq!(config_line(key("tooltips"), &tui), "[tui] tooltips = true");
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

/// The categories group the 35 `[tui]` rows, the AutoEq refresh, the Bluetooth adapter and the 13
/// Connect rows, each once, and every row has an explanation that with its config line fits the box at 80×24.
#[test]
fn every_setting_is_in_one_category_and_its_explanation_fits_the_box() {
    let all = every_setting();
    assert_eq!(all.len(), 50);
    for (at, setting) in all.iter().enumerate() {
        assert!(
            !all[at + 1..].contains(setting),
            "{setting:?} is listed twice"
        );
        assert!(!explain(*setting).is_empty());
    }
    let longest = CATEGORIES.iter().map(|(_, rows)| rows.len()).max();
    assert_eq!(longest, Some(14), "Effects is the longest category");
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

/// Whether `setting` is a plain `[tui]` key that holds one word of a list or a number rather than
/// a switch.
fn is_word_key(setting: Setting) -> bool {
    matches!(
        setting,
        Setting::Key(key) if matches!(key.kind, TuiSettingKind::Word { .. } | TuiSettingKind::Number { .. })
    )
}

/// Every on/off row flips whichever way `←` or `→` is pressed on it, the same change
/// `Space` asks for.
#[test]
fn arrows_flip_every_on_off_row_either_way() {
    for setting in every_setting() {
        let mut baseline = app();
        baseline.overlays.conn.mqtt.host = Some("broker.example.com".into());
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
            app.overlays.conn.mqtt.host = Some("broker.example.com".into());
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
                // a choice fills the column with its own words
                if setting.switch_sides(&app).is_some() {
                    continue;
                }
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
    assert_eq!(lines.len(), 14, "{lines:?}");
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
    assert_eq!(effects.len(), 14);
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

/// The Behaviour category lists `Tooltips` after `Mouse capture`, and Space flips it.
#[test]
fn the_tooltips_row_sits_after_mouse_capture_and_flips() {
    let behaviour = rows(2);
    let mouse = behaviour
        .iter()
        .position(|row| *row == Setting::Mouse)
        .unwrap();
    let row = behaviour[mouse + 1];
    assert_eq!(row.label(), "Tooltips");
    let mut app = app();
    select(&mut app, row);
    assert_eq!(row.value(&app), "on");
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Set(
            &TOOLTIPS,
            TuiValue::Bool(false)
        ))]
    );
}

/// The Effects category lists `Scroll long lines` after `Transition`, on by default, and Space
/// flips it; the key it writes is `[tui] scroll_long_lines`.
#[test]
fn the_scroll_long_lines_row_sits_after_transition_and_flips() {
    let effects = rows(1);
    let transition = effects
        .iter()
        .position(|row| *row == Setting::Key(&TRANSITION))
        .unwrap();
    let row = effects[transition + 1];
    assert_eq!(row, Setting::Key(&SCROLL_LONG_LINES));
    assert_eq!(row.label(), "Scroll long lines");
    let mut app = app();
    select(&mut app, row);
    assert_eq!(row.value(&app), "on");
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Set(
            &SCROLL_LONG_LINES,
            TuiValue::Bool(false)
        ))]
    );
    assert_eq!(
        config_line(row, &TuiConfig::default()),
        "[tui] scroll_long_lines = true"
    );
}

/// The Scroll long lines row draws as a choice like `Target`, `‹ off | on ›`, whether or not the
/// cursor is on it, and the side in force follows the setting.
#[test]
fn the_scroll_long_lines_row_draws_as_a_choice() {
    let mut app = app();
    let row = Setting::Key(&SCROLL_LONG_LINES);
    for focused in [false, true] {
        select(
            &mut app,
            if focused {
                row
            } else {
                Setting::Key(&TRANSITION)
            },
        );
        let line = text(&drawn(&app, 80, 24).0)
            .into_iter()
            .find(|line| line.contains("Scroll long lines"))
            .expect("the row is in the Effects list");
        assert!(line.contains("‹ off | on ›"), "{line}");
    }
    app.tui.scroll_long_lines = false;
    assert_eq!(row.value(&app), "off");
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
/// category; the AutoEq refresh and the Bluetooth adapter close Behaviour.
#[test]
fn the_graph_rows_close_the_look_category() {
    let look = rows(0);
    assert_eq!(
        &look[look.len() - 3..],
        &[key("graphs"), key("graph_span"), key("link_span")]
    );
    assert!(!look.contains(&Setting::AutoEqRefresh));
    let behaviour = rows(2);
    assert_eq!(
        &behaviour[behaviour.len() - 2..],
        &[Setting::AutoEqRefresh, Setting::Adapter]
    );
}

/// The VU reference row steps by one dB on `←` and `→`, either arrow the one way it points, stops
/// at -30 and -6 without asking for anything, and Space (which turns switches) asks for nothing.
#[test]
fn the_vu_reference_row_steps_by_one_and_stops_at_its_ends() {
    let reference = key("vu_reference");
    let set = |db| {
        vec![Intent::Tui(TuiChange::Set(
            match reference {
                Setting::Key(key) => key,
                _ => unreachable!(),
            },
            TuiValue::Number(db),
        ))]
    };
    let mut app = app();
    select(&mut app, reference);
    assert_eq!(press(&mut app, KeyCode::Right), set(-17));
    assert_eq!(press(&mut app, KeyCode::Left), set(-19));
    assert!(press(&mut app, KeyCode::Char(' ')).is_empty());
    for (db, arrow) in [(-30, KeyCode::Left), (-6, KeyCode::Right)] {
        app.tui.vu_reference = db;
        assert!(press(&mut app, arrow).is_empty(), "past {db}");
    }
    app.tui.vu_reference = -6;
    assert_eq!(press(&mut app, KeyCode::Left), set(-7));
    assert_eq!(reference.value(&app), "-6 dBFS");
}
