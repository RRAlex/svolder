// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device screen's layout: tiers, sizes, panes, borders and the waiting panel.

use super::*;

#[test]
fn the_device_screen_draws_the_controls_the_tone_summary_and_the_status() {
    assert_rows(
        &screen(&mockup_app()),
        &[
            " svolder  Gungnir 2 · fw 3 · 0102…08  ● connected                 mouse:on 14:02",
            "─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─",
            "┌ Controls ───────── mute ─ 0 centre ┐┌ Tone control ────────── flat ─ presets ┐",
            "│ ▶ Input        ‹ 3 · Optical ›     ││   Mode      ‹ EQ ›                     │",
            "│   Phase invert [ ] off             ││   Loudness  ░░░░░░░░░░ 0               │",
            "│   NOS filter   [x] on              ││   Preset    custom                     │",
            "│   Mute         [ ] off             ││                                        │",
            "│                                    ││ Band   Freq      Gain      Q           │",
            "│   Volume       █109/127┃░  -6.0 dB ││ ▶ 1   40.0 Hz    +0.0 dB   4.3         │",
            "│                                    ││   2    160 Hz   -2.45 dB   2.1         │",
            "│   Balance      L ────┃───── R  0   ││   3    632 Hz    +0.0 dB   4.3         │",
            "│                                    ││   4   2515 Hz   +1.45 dB   1.0         │",
            "│                                    ││   5  10000 Hz    +0.0 dB   4.3         │",
            "│                                    ││                                        │",
            "│                                    ││                                        │",
            "│                                    ││                                        │",
            "│                                    ││                                        │",
            "│                                    ││                                        │",
            "│                                    ││                                        │",
            "└────────────────────────────────────┘└────────────────────────────────────────┘",
            "┌ Status ────────────────────────────────────────────────────────────── advice ┐",
            "│ Sample rate 48 kHz   fw 3                                                    │",
            "│ Raw: 9=u8 64   9.1=u8 0   9.2=u8 1   (3 unknown functions)                   │",
            "└──────────────────────────────────────────────────────────────────────────────┘",
        ],
    );
}

/// With no key row the panes run down to the terminal's last row: at 80×24 the Controls and
/// Tone panes are 18 rows tall, a row more than they were, and the Status pane ends on row 24.
#[test]
fn the_panes_run_down_to_the_last_row_at_80_by_24() {
    let panes: Vec<(u16, u16)> = frames_of(Rect::new(0, 0, 80, 24), &mockup_app())
        .iter()
        .map(|(_, pane)| (pane.y, pane.height))
        .collect();
    assert_eq!(panes, [(2, 18), (2, 18), (20, 4)]);
}

/// The mockup docs/usage/tui.md carries is the frame the test above pins, and the README shows
/// each of the six pictures and its text twin, all of which exist.
#[test]
fn the_docs_show_the_device_screen_this_build_draws() {
    let rows = text(&screen(&host_mockup_app()));
    crate::tui::test_support::assert_usage_md_shows(&rows, "device-screen mockup");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("the README reads");
    // The stills' extension is set here once; the animation is the one AVIF.
    let still = "webp";
    let pictures = ["dac", "amp", "tone", "telemetry", "devices"]
        .map(|name| (name, format!("docs/images/{name}.{still}")))
        .into_iter()
        .chain([("svolder", "docs/images/svolder.avif".to_string())]);
    for (name, picture) in pictures {
        let twin = format!("docs/images/{name}.txt");
        for file in [picture, twin] {
            assert!(readme.contains(&file), "README.md does not show {file}");
            assert!(root.join(&file).is_file(), "{file} is missing");
        }
    }
}

#[test]
fn the_focused_pane_is_the_one_whose_title_stands_out() {
    let theme = Theme::new(false);
    let glow = theme.style(Role::Rule).fg.expect("the glow");
    let mut app = mockup_app();
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let controls = buffer.cell((2, 2)).expect("the Controls title");
    assert!(
        controls
            .modifier
            .contains(Modifier::BOLD | Modifier::REVERSED),
        "the focused pane's title is not bold and inverse"
    );
    assert_eq!(controls.fg, theme.accent(), "nor in the selected colour");
    assert_eq!(
        buffer.cell((40, 2)).expect("the Tone control title").fg,
        glow,
        "an unfocused title follows the glow"
    );

    app.pane = Pane::Tone;
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    assert_eq!(
        buffer.cell((40, 2)).expect("the Tone control title").fg,
        theme.accent(),
        "the focus moved but its title did not"
    );
    assert_eq!(buffer.cell((2, 2)).expect("the Controls title").fg, glow);
}

#[test]
fn a_device_without_tone_control_gets_a_full_width_controls_pane() {
    let mut profile = profile_named("gungnir2-v2");
    profile.eq = None;
    let mut app = app_on(profile);
    hear(
        &mut app,
        vec![vo("2", Some("input"), json!(3), "3 (Optical)")],
    );
    let rows = text(&screen(&app));
    assert_eq!(
        rows[2],
        format!("┌ Controls {} mute ─ 0 centre ─ refresh ┐", "─".repeat(41)),
        "the Controls pane spans the terminal"
    );
    assert!(
        rows.iter().all(|row| !row.contains("Tone control")),
        "a device with no tone control has no Tone pane"
    );
}

/// At 126×62 — wide and tall — the Controls pane is 50 columns with longer bars, a
/// column apart from the Tone pane; every pane keeps two cells of blank inside its border
/// and a blank row at its top; the Controls pane's groups sit a row apart, and so do the
/// panes and the Status pane under them, which ends on the terminal's last row.
#[test]
fn the_wide_and_tall_tiers_give_the_device_screen_room_and_air() {
    let rows = text(&frame(&mockup_app(), 126, 62));
    let wanted = [
        "┌ Controls ─────────── mute ─ 0 centre ─ refresh ┐ ┌ Tone control ─────────────────────────────────────────── flat ─ presets ┐",
        "│                                                │ │                                                                         │",
        "│  ▶ Input        ‹ 3 · Optical ›                │ │    Mode      ‹ EQ ›                                                     │",
        "│    Phase invert [ ] off                        │ │    Loudness  ░░░░░░░░░░░░░░░░░░░░ 0                                     │",
        "│    NOS filter   [x] on                         │ │    Preset    custom                                                     │",
        "│    Mute         [ ] off                        │ │                                                                         │",
        "│                                                │ │  Band   Freq      Gain      Q                                           │",
        "│    Volume       ██████109/127████░┃░  -6.0 dB  │ │  ▶ 1   40.0 Hz    +0.0 dB   4.3                                         │",
        "│                                                │ │    2    160 Hz   -2.45 dB   2.1                                         │",
        "│    Balance      L ─────────┃────────── R  0    │ │    3    632 Hz    +0.0 dB   4.3                                         │",
    ];
    assert_eq!(&rows[3..13], wanted);
    let bottom = [
        "└────────────────────────────────────────────────┘ └─────────────────────────────────────────────────────────────────────────┘",
        &" ".repeat(126),
        "┌ Status ──────────────────────────────────────────────────────────────────────────────────────────────────────────── advice ┐",
        &format!("│{}│", " ".repeat(124)),
        "│  Sample rate 48 kHz   fw 3                                                                                                 │",
        "│  Raw: 9=u8 64   9.1=u8 0   9.2=u8 1   (3 unknown functions)                                                                │",
        "│  Link                                                                                                                12 h  │",
        "└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘",
    ];
    assert_eq!(&rows[54..62], bottom);
}

/// At 300×60 the body is held to 256 columns in the middle of the terminal, while the
/// header and its rule keep the whole width.
#[test]
fn a_very_wide_terminal_centres_the_body_and_keeps_the_header_full_width() {
    let rows = text(&frame(&mockup_app(), 300, 60));
    assert_eq!(
        rows[2],
        format!(
            "─┤ Controls ├── EQ ── Telemetry ── Devices {} ? help ─ esc menu ─",
            "─".repeat(300 - 63)
        ),
        "the tab strip is not full width"
    );
    assert!(rows[1].ends_with("mouse:on 14:02"), "|{}|", rows[1]);
    assert!(
        rows[59].trim().starts_with('└') && rows[59].trim().ends_with('┘'),
        "the body does not run to the last row: |{}|",
        rows[59]
    );
    let chars: Vec<char> = rows[3].chars().collect();
    assert_eq!(chars[21], ' ', "the body starts before column 22");
    assert_eq!(chars[22], '┌', "the Controls pane does not open the body");
    assert_eq!((chars[71], chars[72], chars[73]), ('┐', ' ', '┌'));
    assert_eq!(chars[277], '┐', "the Tone pane does not end the body");
    assert_eq!(chars[278], ' ', "the body runs past 256 columns");
    let status: Vec<char> = rows[54].chars().collect();
    assert_eq!((status[22], status[277]), ('┌', '┐'), "|{}|", rows[54]);
}

/// At 256 columns the body takes the whole width: the cap is reached, not passed.
#[test]
fn a_terminal_256_columns_wide_draws_its_body_across_the_whole_width() {
    let rows = text(&frame(&mockup_app(), 256, 60));
    let chars: Vec<char> = rows[3].chars().collect();
    assert_eq!(chars[0], '┌', "the Controls pane does not start the body");
    assert_eq!(chars[255], '┐', "the Tone pane does not end the body");
}

/// Below both tiers the device screen takes no tier padding, gap or row: 119×39 is wider and
/// taller than 80×24, and its panes are drawn as the smallest terminal draws them. Its Controls
/// pane still sets its groups a blank line apart, as it does wherever it has the lines for them.
#[test]
fn a_terminal_short_of_both_tiers_adds_no_tier_padding_but_spaces_its_groups() {
    let rows = text(&frame(&mockup_app(), 119, 39));
    assert!(rows[3].starts_with("┌ Controls ───────── mute ─ 0 centre ┐┌ Tone control"));
    assert!(rows[4].starts_with("│ ▶ Input        ‹ 3 · Optical ›     ││   Mode"));
    assert!(rows[8].starts_with("│                                    ││"));
    assert!(rows[9].starts_with("│   Volume       █109/127┃░  -6.0 dB ││"));
    assert!(rows[10].starts_with("│                                    ││"));
    assert!(rows[11].starts_with("│   Balance      L ────┃───── R  0   ││"));
    assert!(
        rows[34].starts_with("└────"),
        "the panes and Status are a row apart"
    );
    assert!(rows[35].starts_with("┌ Status"));
    assert!(
        rows[38].starts_with("└────"),
        "the Status pane does not end on the last row"
    );
}

/// The frames the animation layer traces and the fault line it pulses follow the tiers:
/// each frame is the contour the screen drew, and the fault line is the Status pane's
/// first line of text, under the blank row the tall tier adds.
#[test]
fn the_frames_and_the_fault_line_follow_the_tiers() {
    let mut app = faulty_mockup_app();
    hear(
        &mut app,
        vec![vo(
            "230.1",
            Some("fault_overcurrent"),
            json!(true),
            "active",
        )],
    );
    for (width, height) in [(126, 62), (200, 60), (300, 60)] {
        let buffer = frame(&app, width, height);
        let area = Rect::new(0, 0, width, height);
        for (id, pane) in frames_of(area, &app) {
            for (x, y, corner) in [
                (pane.x, pane.y, "┌"),
                (pane.right() - 1, pane.y, "┐"),
                (pane.x, pane.bottom() - 1, "└"),
                (pane.right() - 1, pane.bottom() - 1, "┘"),
            ] {
                assert_eq!(
                    buffer.cell((x, y)).expect("a corner").symbol(),
                    corner,
                    "{width}×{height}: {id:?} at {pane:?}"
                );
            }
        }
        let line = fault_line(area, &app).expect("a fault line");
        let drawn: String = (line.x..line.right())
            .map(|x| buffer.cell((x, line.y)).expect("a cell").symbol())
            .collect();
        assert!(
            drawn.contains("Faults: Over-current"),
            "{width}×{height}: the fault line is |{drawn}|"
        );
    }
}

#[test]
fn a_terminal_below_eighty_by_twentyfour_is_told_what_it_needs() {
    let drawn = text(&frame(&mockup_app(), 79, 24)).join("\n");
    assert!(drawn.contains("terminal too small"), "{drawn}");
    assert!(drawn.contains("svolder needs 80×24"), "{drawn}");
    assert!(
        !drawn.contains("Controls"),
        "the device screen is not drawn at all: {drawn}"
    );
}

/// The words hit what their keys press: at 80 columns Controls has `mute` and `0 centre`
/// (`refresh` is dropped) and the Tone pane has `flat` and `presets`.
#[test]
fn each_drawn_word_has_a_hit_of_its_key() {
    let app = mockup_app();
    let mut terminal =
        Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let cell_of = |code: KeyCode| {
        let area = hits
            .area_of(Hit::Key(code))
            .unwrap_or_else(|| panic!("no hit for {code:?}"));
        (area.x, area.y, area.width)
    };
    assert_eq!(cell_of(KeyCode::Char('m')), (21, 2, 4));
    assert_eq!(cell_of(KeyCode::Char('0')), (28, 2, 8));
    assert_eq!(cell_of(KeyCode::Char('f')), (64, 2, 4));
    assert_eq!(cell_of(KeyCode::Char('p')), (71, 2, 7));
    assert_eq!(hits.area_of(Hit::Key(KeyCode::Char('r'))), None);
}

/// The app a start that found the daemon holding the device `amp` has not found: the placeholder
/// profile under the waiting panel, waiting since a moment the case chooses.
fn waiting_app() -> (App, String) {
    let since = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let mut app = App::new(
        svolder_profiles::Registry::generic(0),
        "amp".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.session.product = "amp".into();
    app.session.connected = false;
    // The header's spinner turns with the clock; the mockup holds it still.
    app.session.feed_open = false;
    app.session.holding = Some(crate::tui::app::Holding::Waiting { since });
    app.clock = "14:02".into();
    (app, svolder_ipc::clock::local_hhmm(since))
}

/// The waiting panel says the three lines, with the time this session began waiting, replaces the
/// Controls and Status panes with one frame, and carries no control of a placeholder profile.
/// The mockup docs/usage/tui.md carries is this frame, the clock the case fixed standing for the
/// moment the wait began.
#[test]
fn the_waiting_panel_replaces_the_panes_and_the_docs_show_it() {
    let (app, since) = waiting_app();
    let rows: Vec<String> = text(&screen(&app))
        .iter()
        .map(|row| row.replace(&since, "14:02"))
        .collect();
    let all = rows.join("\n");
    assert!(all.contains(" amp not found yet"), "{all}");
    assert!(
        all.contains("svolderd keeps looking (since 14:02)."),
        "{all}"
    );
    assert!(all.contains("Is it on and in range?"), "{all}");
    assert!(
        !all.contains("Status") && !all.contains("Tone control"),
        "{all}"
    );
    assert_eq!(
        frames_of(Rect::new(0, 0, 80, 24), &app)
            .iter()
            .map(|(_, pane)| (pane.y, pane.height))
            .collect::<Vec<_>>(),
        [(2, 22)]
    );
    crate::tui::test_support::assert_usage_md_shows(&rows, "waiting panel mockup");
}
