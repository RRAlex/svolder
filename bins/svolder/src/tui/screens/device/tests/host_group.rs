// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Host group: now-playing, the host volume, the spectrum and the transport row.

use super::*;

/// docs/usage/tui.md's and the README's device-screen mockup: the screen as a host with its own
/// audio draws it, byte for byte.
#[test]
fn the_device_screen_draws_the_host_group_and_now_playing() {
    assert_rows(
        &screen(&host_mockup_app()),
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
            "│   Host                             ││   5  10000 Hz    +0.0 dB   4.3         │",
            "│   Host volume  ███54%░░░░  Gungni… ││                                        │",
            "│   Transport    ◀◀  ▶  ▮▮  ▶▶       ││                                        │",
            "│   Host EQ      off                 ││                                        │",
            "│                                    ││                                        │",
            "└────────────────────────────────────┘└────────────────────────────────────────┘",
            "┌ Status ────────────────────────────────────────────────────────────── advice ┐",
            "│ Sample rate 48 kHz   fw 3                                                    │",
            "│ Raw: 9=u8 64   9.1=u8 0   9.2=u8 1   (3 unknown functions)                   │",
            "│ Level from: host                                                             │",
            "│ Now playing: So What — Miles Davis   mpv                                     │",
            "└──────────────────────────────────────────────────────────────────────────────┘",
        ],
    );
}

/// The Status pane grows a line for the `Now playing` row, its last, under its own two and
/// the `Level from` row: the track as `Title — Artist`, and the player's name dim after it
/// where there is room.
#[test]
fn the_now_playing_row_names_the_track_and_the_player() {
    let theme = Theme::new(false);
    let buffer = frame_with_theme(&hearing(Some(track("So What"))), 80, 24, &theme);
    let rows = text(&buffer);
    assert_eq!(
        rows[18],
        "┌ Status ────────────────────────────────────────────────────────────── advice ┐"
    );
    assert_eq!(
        rows[22],
        "│ Now playing: So What — Miles Davis   mpv                                     │"
    );
    assert_eq!(
        rows[23],
        "└──────────────────────────────────────────────────────────────────────────────┘"
    );
    let dim = theme.style(Role::Dim);
    let name = buffer.cell((39, 22)).expect("the player's name");
    assert_eq!(name.symbol(), "m");
    assert!(
        name.modifier.contains(dim.add_modifier) && dim.fg.is_none_or(|fg| name.fg == fg),
        "the player's name is not dim: {name:?}"
    );
    let word = buffer.cell((15, 22)).expect("the track's title");
    assert!(!word.modifier.contains(Modifier::DIM), "the track is dim");
}

/// Words too long for the row are a window its marquee carries along them, with the
/// player's name left out; the window starts where the marquee has got to.
#[test]
fn words_too_long_for_the_row_are_carried_by_its_marquee() {
    let long = "A Very Long Title That Goes On And On Past The End Of The Row";
    let mut app = hearing(Some(track(long)));
    let words = format!("{long} — Miles Davis");
    let room = now_playing_room((80, 24));
    assert!(words.chars().count() > room, "the words fit the row");
    let row = |app: &App| text(&screen(app))[22].clone();
    let shown: String = words.chars().take(room).collect();
    assert_eq!(row(&app), format!("│ Now playing: {shown}│"));
    app.host.marquee.offset = 3;
    let shown: String = words.chars().skip(3).take(room).collect();
    assert_eq!(row(&app), format!("│ Now playing: {shown}│"));
    assert!(
        !row(&app).contains("mpv"),
        "the player's name took room from the words"
    );

    // Words that fit with too little room left for the name after them go without it,
    // rather than with a name cut short.
    let title: String = "x".repeat(room - 4 - " — Miles Davis".chars().count());
    let app = hearing(Some(track(&title)));
    assert_eq!(
        row(&app),
        format!("│ Now playing: {title} — Miles Davis    │"),
        "a name with no room for it was drawn cut"
    );
}

/// A guard: the row already measured cells. A long title of wide glyphs stays inside its pane:
/// the pane's right border is still drawn in the row's last cell, and a wide glyph is never left
/// half-drawn against it.
#[test]
fn a_long_wide_title_keeps_the_pane_border_in_place() {
    let buffer = screen(&hearing(Some(track(&"世界".repeat(40)))));
    let symbol = |x: u16| {
        buffer
            .cell((x, 22))
            .expect("a cell of the row")
            .symbol()
            .to_string()
    };
    assert_eq!(symbol(79), "│");
    assert_ne!(symbol(78), "世", "a wide glyph reaches into the border");
    assert_ne!(symbol(78), "界", "a wide glyph reaches into the border");
}

/// The row measures in terminal cells: a title of wide characters that has fewer
/// characters than the row has cells, but more cells, scrolls — its tail reached at the far
/// end of the marquee's travel — leaves the player's name out, and stays inside the pane.
#[test]
fn the_now_playing_row_measures_wide_characters_in_cells() {
    let title = "ア".repeat(30);
    let words = format!("{title} — Miles Davis");
    let room = now_playing_room((80, 24));
    assert!(words.chars().count() <= room && cells(&words) > room);
    let mut app = hearing(Some(track(&title)));
    let symbol = |buffer: &Buffer, x: u16| {
        buffer
            .cell((x, 22))
            .expect("a cell of the row")
            .symbol()
            .to_string()
    };

    let start = screen(&app);
    assert_eq!(symbol(&start, 15), "ア");
    assert_eq!(
        symbol(&start, 78),
        "M",
        "the window is not the row's cells wide"
    );
    assert_eq!(symbol(&start, 79), "│");
    assert!(
        !text(&start)[22].contains("mpv"),
        "the name was drawn off the row"
    );

    app.host.marquee.offset = 1;
    let cut = screen(&app);
    assert_eq!(
        symbol(&cut, 15),
        " ",
        "a wide character cut by the edge is not blank"
    );
    assert_eq!(symbol(&cut, 16), "ア");

    let travel = u16::try_from(cells(&words) - room).expect("a short travel");
    app.host.marquee.offset = travel;
    let end = screen(&app);
    assert_eq!(
        symbol(&end, 78),
        "s",
        "the far end does not reach the words' tail"
    );
    assert_eq!(symbol(&end, 79), "│");

    let mut ticked = hearing(Some(track(&title)));
    let at = ticked.now;
    ticked.tick(at + Duration::from_secs(1));
    assert_eq!(
        ticked.host.marquee.offset, 1,
        "the tick measured the words in characters"
    );
}

/// A sink named in wide characters is cut to the row's cells, keeping the blank before the
/// pane's border.
#[test]
fn a_wide_sink_name_is_cut_to_the_rows_cells() {
    let buffer = screen(&sinking(0.54, false, "オーディオ出力"));
    let symbol = |x: u16| buffer.cell((x, 13)).expect("a cell").symbol().to_string();
    assert_eq!(symbol(29), "オ");
    assert_eq!(symbol(31), "ー");
    assert_eq!(symbol(33), "デ");
    assert_eq!(symbol(35), "…");
    assert_eq!(symbol(36), " ", "the name ran up to the border");
    assert_eq!(symbol(37), "│");
}

#[test]
fn a_window_of_cells_blanks_a_wide_character_its_edges_cut() {
    assert_eq!(cell_window("abcdef", 2, 3), "cde");
    assert_eq!(cell_window("アイウ", 0, 4), "アイ");
    assert_eq!(
        cell_window("アイウ", 1, 4),
        " イ",
        "the right edge took half of ウ"
    );
    assert_eq!(cell_window("アイウ", 1, 5), " イウ");
    assert_eq!(cell_window("aアb", 2, 2), " b");
    assert_eq!(cut_cells("アイウ", 5), "アイ…");
    assert_eq!(cut_cells("アイウ", 6), "アイウ");
    assert_eq!(cut_cells("アイウ", 4), "ア…");
}

/// The marquee sets off once the pause at the start of the words is over, a cell a tick.
#[test]
fn the_rows_marquee_moves_a_cell_a_tick_after_its_pause() {
    let long = "A Very Long Title That Goes On And On Past The End Of The Row";
    let mut app = hearing(Some(track(long)));
    let start = app.now;
    app.tick(start + Duration::from_millis(500));
    assert_eq!(
        app.host.marquee.offset, 0,
        "the words did not pause at their start"
    );
    app.tick(start + Duration::from_millis(1_000));
    assert_eq!(app.host.marquee.offset, 1);
    app.tick(start + Duration::from_millis(1_250));
    assert_eq!(app.host.marquee.offset, 2, "not one cell per 250 ms");
}

/// With a bus but no player the row reads `—`, dim; with no session bus, or with host audio
/// off, there is no row, and the Status pane keeps its own two lines and — while host
/// audio shows — the `Level from` row.
#[test]
fn the_now_playing_row_says_no_player_and_goes_without_a_bus() {
    let theme = Theme::new(false);
    let buffer = frame_with_theme(&hearing(None), 80, 24, &theme);
    assert_eq!(
        text(&buffer)[22],
        "│ Now playing: —                                                               │"
    );
    let dash = buffer.cell((15, 22)).expect("the dash");
    let dim = theme.style(Role::Dim);
    assert!(
        dash.modifier.contains(dim.add_modifier) && dim.fg.is_none_or(|fg| dash.fg == fg),
        "the dash is not dim: {dash:?}"
    );

    let mut no_bus = hearing(Some(track("So What")));
    no_bus.host.lost_bus();
    let mut off = hearing(Some(track("So What")));
    off.tui.host_audio = false;
    let unlinked = mockup_app();
    for (app, top, lines) in [
        (no_bus, 19, "three"),
        (off, 20, "two"),
        (unlinked, 20, "two"),
    ] {
        let rows = text(&screen(&app));
        assert!(
            rows.iter().all(|row| !row.contains("Now playing")),
            "{rows:#?}"
        );
        assert_eq!(
            rows[top],
            "┌ Status ────────────────────────────────────────────────────────────── advice ┐",
            "the Status pane is not {lines} lines"
        );
    }
}

/// With host audio shown the Status pane says where the level the glow and the title meter
/// read comes from, under its own two lines: `meters` while the device's scaled meters
/// stream, `host` while the capture of the host's sink runs and no meter streams, and `—`,
/// dim, with neither. With host audio off there is no row.
#[test]
fn the_level_from_row_says_where_the_level_comes_from() {
    let theme = Theme::new(false);
    let row = |app: &App| text(&frame_with_theme(app, 80, 24, &theme))[21].clone();

    let mut capturing = hearing(Some(track("So What")));
    capturing.host.capturing = true;
    assert_eq!(
        row(&capturing),
        "│ Level from: host                                                             │"
    );

    let nowhere = hearing(Some(track("So What")));
    let buffer = frame_with_theme(&nowhere, 80, 24, &theme);
    assert_eq!(
        text(&buffer)[21],
        "│ Level from: —                                                                │"
    );
    let dash = buffer.cell((14, 21)).expect("the dash");
    let dim = theme.style(Role::Dim);
    assert!(
        dash.modifier.contains(dim.add_modifier) && dim.fg.is_none_or(|fg| dash.fg == fg),
        "the dash is not dim: {dash:?}"
    );

    let mut amp = app_on(profile_named("fake-amp"));
    amp.host.linked = true;
    amp.host.capturing = true;
    amp.host.found_bus(Some(track("So What")), amp.now);
    hear(
        &mut amp,
        vec![ValueObject {
            value_type: "u16".into(),
            ..vo("20.1", None, json!(900), "900")
        }],
    );
    let rows = text(&frame_with_theme(&amp, 80, 24, &theme));
    assert!(
        rows.iter()
            .any(|row| row.starts_with("│ Level from: meters     ")),
        "{rows:#?}"
    );

    let mut off = capturing;
    off.tui.host_audio = false;
    let rows = text(&screen(&off));
    assert!(
        rows.iter().all(|row| !row.contains("Level from")),
        "{rows:#?}"
    );
}

/// The Host group sits right under the device's own rows at 80×24, under a heading of its
/// own: the sink's volume as a bar from 0 to 100 % with the volume inside it, and the
/// sink's name dim after it, cut to what the row leaves.
#[test]
fn the_host_group_sits_under_the_devices_rows_at_80x24() {
    let theme = Theme::new(false);
    let buffer = frame_with_theme(&sinking(0.54, false, "USB DAC"), 80, 24, &theme);
    let rows = text(&buffer);
    let pane = |row: &str| row.chars().take(38).collect::<String>();
    assert_eq!(
        rows[10..14].iter().map(|row| pane(row)).collect::<Vec<_>>(),
        [
            "│   Balance      L ────┃───── R  0   │",
            "│                                    │",
            "│   Host                             │",
            "│   Host volume  ███54%░░░░  USB DAC │",
        ]
    );
    let dim = theme.style(Role::Dim);
    for (x, y, what) in [(4, 12, "the heading"), (30, 13, "the sink's name")] {
        let cell = buffer.cell((x, y)).expect("a cell");
        assert!(
            cell.modifier.contains(dim.add_modifier) && dim.fg.is_none_or(|fg| cell.fg == fg),
            "{what} is not dim: {cell:?}"
        );
    }

    let short = text(&screen(&sinking(0.54, false, "HDMI")));
    assert_eq!(pane(&short[13]), "│   Host volume  ███54%░░░░  HDMI    │");
}

/// A muted sink says so after its bar, in the warn role, before its name, which gives way
/// where the row has no room left for it; a sink something else boosted past 100 % fills
/// the bar and says the volume it reads.
#[test]
fn the_host_volume_row_says_muted_and_a_boost() {
    let theme = Theme::new(false);
    let buffer = frame_with_theme(&sinking(0.54, true, "USB DAC"), 80, 24, &theme);
    let row: String = text(&buffer)[13].chars().take(38).collect();
    assert_eq!(row, "│   Host volume  ███54%░░░░  muted   │");
    let muted = buffer.cell((29, 13)).expect("the word");
    assert_eq!(muted.symbol(), "m");
    assert_eq!(
        muted.fg,
        theme.style(Role::Warn).fg.expect("the warn colour")
    );

    let wide: Vec<String> = text(&frame(&sinking(0.54, true, "USB DAC"), 130, 30));
    assert!(
        wide.iter()
            .any(|row| row.contains("█54%░") && row.contains("muted")),
        "{wide:#?}"
    );
    let boosted = text(&screen(&sinking(1.2, false, "USB DAC")));
    assert_eq!(
        boosted[13].chars().take(38).collect::<String>(),
        "│   Host volume  ███120%███  USB DAC │"
    );
}

/// Before the sink has read, and on a host with no sink to read, the row reads `—`, dim.
#[test]
fn without_a_sink_the_host_volume_row_reads_a_dash_dim() {
    let theme = Theme::new(false);
    let buffer = frame_with_theme(&hearing(None), 80, 24, &theme);
    let row: String = text(&buffer)[13].chars().take(38).collect();
    assert_eq!(row, "│   Host volume  —                   │");
    let dash = buffer.cell((17, 13)).expect("the dash");
    assert_eq!(dash.symbol(), "—");
    let stale = theme.style(Role::Stale);
    assert!(
        dash.modifier.contains(stale.add_modifier) && stale.fg.is_none_or(|fg| dash.fg == fg),
        "the row is not dim: {dash:?}"
    );
}

/// The tall tier keeps the Host group under the device's rows, and with host audio off, or
/// with no host linked, the Controls pane has no Host group at all.
#[test]
fn the_host_group_shows_in_the_tall_tier_and_goes_with_host_audio() {
    let tall = text(&frame(&sinking(0.54, false, "USB DAC"), 80, 45));
    let heading = tall
        .iter()
        .position(|row| row.starts_with("│   Host                    "))
        .expect("the tall tier has the Host group");
    assert!(
        tall[heading - 1].starts_with("│                  "),
        "{tall:#?}"
    );
    assert!(tall[heading + 1].contains("Host volume  ███"), "{tall:#?}");

    let mut off = sinking(0.54, false, "USB DAC");
    off.tui.host_audio = false;
    for app in [off, mockup_app()] {
        let rows = text(&screen(&app));
        assert!(rows.iter().all(|row| !row.contains("Host")), "{rows:#?}");
    }
}

/// A click on the Host volume row focuses it under the index the keys reach it by, after
/// the device's own rows, and the wheel over its bar steps it.
#[test]
fn the_host_volume_row_takes_a_click_and_the_wheel() {
    let app = sinking(0.54, false, "USB DAC");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    use crate::tui::app::Intent;
    use crate::tui::mouse::{self, MouseAction};

    let at = |kind, column| MouseEvent {
        kind,
        column,
        row: 13,
        modifiers: KeyModifiers::NONE,
    };
    let click = MouseEventKind::Down(MouseButton::Left);
    let own = app.rows().len();
    let focus = Some(MouseAction::Focus(Pane::Controls, own));
    assert_eq!(
        mouse::resolve(&hits, at(click, 4), &app),
        focus,
        "the label"
    );
    assert_eq!(mouse::resolve(&hits, at(click, 20), &app), focus, "the bar");
    assert_eq!(
        mouse::resolve(&hits, at(MouseEventKind::ScrollUp, 20), &app),
        Some(MouseAction::Intent(Intent::HostVolume(56)))
    );
    assert_eq!(
        mouse::resolve(&hits, at(MouseEventKind::ScrollDown, 20), &app),
        Some(MouseAction::Intent(Intent::HostVolume(52)))
    );
    let heading = MouseEvent {
        row: 12,
        ..at(click, 4)
    };
    assert_eq!(
        mouse::resolve(&hits, heading, &app),
        None,
        "the heading is a row a click focuses"
    );
}

/// The most rows a built-in profile lists — kraken's four switches and five meters — leave
/// room for the whole Host group, its last row included, and the `Level from` and `Now
/// playing` rows at 80×24.
#[test]
fn the_host_group_fits_the_busiest_profile_at_80x24() {
    let mut app = app_on(profile_named("kraken"));
    app.host.linked = true;
    app.host.found_bus(Some(track("So What")), app.now);
    app.host.fold_read(
        Some(svolder_host::SinkState {
            volume: 0.54,
            muted: false,
            name: "USB DAC".into(),
            target: None,
        }),
        true,
    );
    let rows = text(&screen(&app));
    for wanted in [
        "Meter 5",
        "Host volume",
        "Transport",
        "Faults:",
        "Level from: —",
        "Now playing: So What",
    ] {
        assert!(
            rows.iter().any(|row| row.contains(wanted)),
            "{wanted} is not on the 80x24 screen: {rows:#?}"
        );
    }
}

/// The mockup's app with the capture of the sink's monitor running and one block landed
/// at the app's own clock, band `n` at `n` sevenths of the scale — the eight glyphs in turn,
/// then the full block for every band past the top.
fn spectral() -> App {
    let mut app = sinking(0.54, false, "USB DAC");
    app.host.capturing = true;
    let bands = std::array::from_fn(|band| band as f32 / 7.0);
    let block = svolder_host::Block {
        bands,
        left: 0.5,
        right: 0.5,
    };
    app.heard_block(&block, app.now);
    app
}

/// One frame of `app` in a terminal `width` by `height`, and where it says the Spectrum
/// row landed.
fn spectrum_of(app: &App, width: u16, height: u16) -> (Vec<String>, Option<Rect>) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = render::FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, &Theme::new(false), &mut hits))
        .expect("the frame draws");
    (text(terminal.backend().buffer()), areas.spectrum)
}

/// The Spectrum row is the Host group's last, under Host EQ, in the wide and the tall
/// tiers: a block glyph a band, as tall as the band is loud, from the lowest band on the
/// left. The 80×24 layout leaves it out, and so does the visualizer switched off; with no
/// capture running it reads `—`, dim. Each frame that draws it says where it landed.
#[test]
fn the_spectrum_row_draws_a_blocks_bands_in_the_wide_and_tall_tiers() {
    let (tall, landed) = spectrum_of(&spectral(), 80, 45);
    let at = tall
        .iter()
        .position(|row| row.contains("Spectrum"))
        .expect("the tall tier draws the row");
    assert_eq!(
        tall[at].chars().take(38).collect::<String>(),
        "│   Spectrum     ▁▂▃▄▅▆▇█████████    │"
    );
    assert!(tall[at - 1].contains("Host EQ"), "{tall:#?}");
    assert_eq!(landed.map(|row| row.y), u16::try_from(at).ok());

    let (wide, landed) = spectrum_of(&spectral(), 130, 30);
    assert!(
        wide.iter().any(|row| row.contains("Spectrum     ▁▂▃▄▅▆▇█")),
        "{wide:#?}"
    );
    assert!(
        landed.is_some(),
        "the wide tier did not say where the row landed"
    );

    let (small, landed) = spectrum_of(&spectral(), 80, 24);
    assert!(
        small.iter().all(|row| !row.contains("Spectrum")),
        "{small:#?}"
    );
    assert_eq!(landed, None);
    let mut off = spectral();
    off.tui.visualizer = false;
    let (rows, landed) = spectrum_of(&off, 80, 45);
    assert!(
        rows.iter().all(|row| !row.contains("Spectrum")),
        "{rows:#?}"
    );
    assert_eq!(landed, None, "the row was drawn with the visualizer off");

    let theme = Theme::new(false);
    let mut idle = spectral();
    idle.host.capture_stopped();
    let buffer = frame_with_theme(&idle, 80, 45, &theme);
    let rows = text(&buffer);
    let at = rows
        .iter()
        .position(|row| row.contains("Spectrum"))
        .expect("the row with no capture");
    assert_eq!(
        rows[at].chars().take(38).collect::<String>(),
        "│   Spectrum     —                   │"
    );
    let dash = buffer
        .cell((16, u16::try_from(at).expect("a row")))
        .expect("the dash");
    let stale = theme.style(Role::Stale);
    assert!(
        dash.modifier.contains(stale.add_modifier) && stale.fg.is_none_or(|fg| dash.fg == fg),
        "the row is not dim: {dash:?}"
    );
}

/// Between blocks the bands fall back at one rate — a seventh of the scale, one glyph, in a
/// seventh of the fall — until the row settles on the lowest glyph; a block landing takes
/// the bands straight back up.
#[test]
fn the_spectrum_row_falls_back_between_blocks() {
    let row = |app: &App| {
        let (rows, _) = spectrum_of(app, 80, 45);
        let row = rows
            .into_iter()
            .find(|row| row.contains("Spectrum"))
            .expect("the row");
        row.chars().skip(17).take(16).collect::<String>()
    };
    let mut app = spectral();
    let landed = app.now;
    app.now = landed + host::FALL / 7;
    assert_eq!(row(&app), "▁▁▂▃▄▅▆▇▇▇▇▇▇▇▇▇");
    app.now = landed + host::FALL;
    assert_eq!(row(&app), "▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁", "the row never settled");
    let block = svolder_host::Block {
        bands: [1.0; svolder_host::fft::BANDS],
        left: 1.0,
        right: 1.0,
    };
    app.heard_block(&block, landed + host::FALL);
    assert_eq!(
        row(&app),
        "████████████████",
        "the block did not land at once"
    );
}

/// One analysed frame of the daemon, every band at `level` (a byte), flagged a beat or not.
fn analysed(level: u8, beat: bool) -> svolder_host::Frame {
    svolder_host::Frame {
        capture_ms: 0,
        left: [level; svolder_host::analysis::BANDS],
        right: [level; svolder_host::analysis::BANDS],
        levels: [level; 4],
        beat,
        bpm: beat.then_some(120),
        phase: 0,
    }
}

/// Whether the Spectrum row's band cells are drawn bold in `app`'s frame.
fn spectrum_is_bold(app: &App) -> bool {
    let mut terminal = Terminal::new(TestBackend::new(80, 45)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = render::FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, &Theme::new(false), &mut hits))
        .expect("the frame draws");
    let row = areas.spectrum.expect("the row is drawn");
    let buffer = terminal.backend().buffer();
    let bold = |x: u16| {
        buffer
            .cell((x, row.y))
            .expect("a cell of the row")
            .modifier
            .contains(Modifier::BOLD)
    };
    // the band cells start after the label: the first block glyph of the row
    let first = (row.x..row.right())
        .find(|x| {
            let cell = buffer.cell((*x, row.y)).expect("a cell");
            cell.symbol().chars().all(|c| "▁▂▃▄▅▆▇█".contains(c)) && cell.symbol() != " "
        })
        .expect("the row draws a band");
    bold(first)
}

/// A beat the daemon's frames carry pulses the Spectrum row: its bands are drawn bold for
/// [`host::BEAT_PULSE`] from the beat and in their own weight before and after, and a frame
/// without a beat does not pulse it.
#[test]
fn a_beat_pulses_the_spectrum_row() {
    let mut app = sinking(0.54, false, "USB DAC");
    app.host.capturing = true;
    let start = app.now;
    app.heard_frame(&analysed(200, false), start);
    assert!(
        !spectrum_is_bold(&app),
        "a frame with no beat pulsed the row"
    );

    let beat = start + Duration::from_millis(500);
    app.heard_frame(&analysed(200, true), beat);
    assert!(spectrum_is_bold(&app), "the beat did not pulse the row");
    // the frame lands its bands like a block does: the row is the frame's own
    let (rows, _) = spectrum_of(&app, 80, 45);
    assert!(
        rows.iter()
            .any(|row| row.contains("Spectrum")
                && row.chars().any(|glyph| "▂▃▄▅▆▇█".contains(glyph))),
        "the frame's bands were not drawn: {rows:#?}"
    );
    app.now = beat + host::BEAT_PULSE / 2;
    assert!(spectrum_is_bold(&app), "the pulse ended early");
    app.now = beat + host::BEAT_PULSE;
    assert!(!spectrum_is_bold(&app), "the pulse never ended");
}

/// Where an amp's meters stream they are the level the borders follow, but the Spectrum row is
/// still drawn from the daemon's frames: a beat pulses the row (and puts the loop on its fast
/// tick for it) and leaves the borders' shading alone.
#[test]
fn a_beat_pulses_the_spectrum_row_where_meters_stream() {
    let mut app = app_with_meter_reading(9_000);
    app.host.linked = true;
    app.host.capturing = true;
    app.host.spectrum_on_screen = true;
    assert!(
        app.meters_streaming(),
        "the meters are not the level's source"
    );
    app.heard_frame(&analysed(200, false), app.now);
    assert!(!spectrum_is_bold(&app) && !app.pulsing());
    app.heard_frame(&analysed(200, true), app.now);
    assert!(spectrum_is_bold(&app), "the beat did not pulse the row");
    assert!(app.pulsing(), "the loop would not redraw for the pulse");
    assert_eq!(
        app.pulse(),
        0.0,
        "the borders were lifted off the meters' level"
    );
    app.now += host::BEAT_PULSE;
    assert!(!spectrum_is_bold(&app) && !app.pulsing());
}

/// The Spectrum row is on the screen only where the frame put it: a box open over the
/// device screen, another screen, and a short wide terminal whose Controls pane the
/// busiest profile fills leave it out of sight, and those frames say so.
#[test]
fn a_spectrum_row_out_of_sight_is_not_on_the_screen() {
    let mut helped = spectral();
    helped.overlays.help_open = true;
    assert_eq!(
        spectrum_of(&helped, 80, 45).1,
        None,
        "under the help overlay"
    );
    let mut elsewhere = spectral();
    elsewhere.screen = Screen::Telemetry;
    assert_eq!(
        spectrum_of(&elsewhere, 80, 45).1,
        None,
        "on the telemetry screen"
    );

    // Kraken with two toggles more, the busiest pane the built-in profiles could grow to.
    let mut busy = profile_named("kraken");
    for (n, key) in ["extra_1", "extra_2"].into_iter().enumerate() {
        let mut extra = busy
            .control_by_key("power")
            .expect("kraken has power")
            .clone();
        extra.key = key.into();
        extra.id = format!("{}", 90 + n).parse().expect("a valid function id");
        busy.controls.push(extra);
    }
    let mut kraken = app_on(busy);
    kraken.host.linked = true;
    kraken.host.capturing = true;
    kraken.host.found_bus(Some(track("So What")), kraken.now);
    let (rows, landed) = spectrum_of(&kraken, 120, 24);
    assert!(
        rows.iter().any(|row| row.contains("Transport")),
        "{rows:#?}"
    );
    assert!(
        rows.iter().all(|row| !row.contains("Spectrum")),
        "the pane had room after all: {rows:#?}"
    );
    assert_eq!(
        landed, None,
        "a row the pane cut off was reported on the screen"
    );
    let (_, landed) = spectrum_of(&kraken, 120, 45);
    assert!(landed.is_some(), "the tall tier has room for it");
}

/// At 100 columns and wider the header carries the wordmark, a row taller, and the busiest
/// profile with a session bus still has every Host row inside its Controls pane, Transport
/// included: outside the tall tier the group's heading sits right under the device's rows.
#[test]
fn every_host_row_fits_the_busiest_profile_under_the_wordmark_header() {
    let mut app = app_on(profile_named("kraken"));
    app.host.linked = true;
    app.host.found_bus(Some(track("So What")), app.now);
    app.host.fold_read(
        Some(svolder_host::SinkState {
            volume: 0.54,
            muted: false,
            name: "USB DAC".into(),
            target: None,
        }),
        true,
    );
    for (width, height) in [(100, 24), (100, 25), (126, 24)] {
        let rows = text(&frame(&app, width, height));
        let bottom = rows
            .iter()
            .position(|row| row.starts_with('└'))
            .expect("the Controls pane's bottom border");
        // What a pane row says between its borders, the cursor column and the padding left
        // out.
        let said = |row: &str| {
            row.strip_prefix('│')
                .and_then(|row| row.strip_suffix('│'))
                .map(|inner| inner.trim().to_string())
        };
        for label in ["Host", "Host volume", "Transport"] {
            let at = rows
                .iter()
                .position(|row| {
                    said(row).is_some_and(|said| {
                        said == label || said.starts_with(&format!("{label}  "))
                    })
                })
                .unwrap_or_else(|| panic!("{label:?} is not drawn at {width}×{height}: {rows:#?}"));
            assert!(
                at < bottom,
                "{label:?} fell out of the Controls pane at {width}×{height}: {rows:#?}"
            );
        }
        assert!(
            rows.iter().any(|row| row.contains("Now playing: So What")),
            "{width}×{height}: {rows:#?}"
        );
    }
}

/// The mockup's app linked to a host with a sink and a player in `status`.
fn transporting(status: svolder_host::Status) -> App {
    let mut app = sinking(0.54, false, "USB DAC");
    let playing = svolder_host::NowPlaying {
        genre: Vec::new(),
        status,
        ..track("So What")
    };
    app.host.found_bus(Some(playing), app.now);
    app
}

/// The Transport row draws its four buttons in the plain geometric glyphs, the one
/// matching what the player does in the accent, bold: play while it plays, pause while it
/// is paused.
#[test]
fn the_transport_row_lights_the_button_the_player_is_on() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let accent = theme.style(Role::Cursor).fg.expect("the accent");
    for (status, lit) in [
        (svolder_host::Status::Playing, 21),
        (svolder_host::Status::Paused, 24),
    ] {
        let buffer = frame_with_theme(&transporting(status), 80, 24, &theme);
        let row: String = text(&buffer)[14].chars().take(38).collect();
        assert_eq!(row, "│   Transport    ◀◀  ▶  ▮▮  ▶▶       │");
        for x in [17, 21, 24, 28] {
            let cell = buffer.cell((x, 14)).expect("a button");
            let on = cell.fg == accent && cell.modifier.contains(Modifier::BOLD);
            assert_eq!(on, x == lit, "{status:?}: the button at {x} is lit {on}");
        }
    }
    let stopped = frame_with_theme(&transporting(svolder_host::Status::Stopped), 80, 24, &theme);
    for x in [17, 21, 24, 28] {
        assert_ne!(
            stopped.cell((x, 14)).expect("a button").fg,
            accent,
            "lit while stopped"
        );
    }
}

/// On the focused Transport row the button under its cursor is drawn out of the row's
/// highlight, and with no player the whole row is dim.
#[test]
fn the_transport_rows_cursor_stands_out_and_no_player_dims_it() {
    let theme = Theme::new(false);
    let mut app = transporting(svolder_host::Status::Playing);
    app.selected = app.rows().len() + 1;
    app.host.button = 3;
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    assert!(text(&buffer)[14].starts_with("│ ▶ Transport    ◀◀  ▶  ▮▮  ▶▶"));
    let reversed = |x: u16| {
        buffer
            .cell((x, 14))
            .expect("a button")
            .modifier
            .contains(Modifier::REVERSED)
    };
    assert!(
        !reversed(28),
        "the button under the cursor is in the row's highlight"
    );
    assert!(reversed(17) && reversed(24), "the row is not highlighted");

    let mut none = sinking(0.54, false, "USB DAC");
    none.host.found_bus(None, none.now);
    let buffer = frame_with_theme(&none, 80, 24, &theme);
    let stale = theme.style(Role::Stale);
    let glyph = buffer.cell((17, 14)).expect("a button");
    assert!(
        glyph.modifier.contains(stale.add_modifier) && stale.fg.is_none_or(|fg| glyph.fg == fg),
        "the row is not dim with no player: {glyph:?}"
    );
}

/// A click anywhere on a button, or on the cell either side of it it shares with no other
/// button, lands on that button.
#[test]
fn a_click_on_a_transport_button_lands_on_it() {
    let app = transporting(svolder_host::Status::Playing);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    for (x, button) in [
        (17, 0),
        (18, 0),
        (19, 0),
        (20, 1),
        (21, 1),
        (22, 1),
        (23, 2),
        (24, 2),
        (26, 2),
        (27, 3),
        (29, 3),
    ] {
        assert_eq!(
            hits.hit_at(x, 14),
            Some(&Hit::Transport(button)),
            "column {x}"
        );
    }
    assert_eq!(
        hits.hit_at(5, 14),
        Some(&Hit::Row(Pane::Controls, app.rows().len() + 1)),
        "the row's own label focuses it"
    );
}

/// A route to a Gungnir 2 sink at 48 kHz, the player on the same rate and every volume at
/// 100 %, with `edit` applied.
fn gungnir_route(edit: impl FnOnce(&mut svolder_host::RouteFacts)) -> svolder_host::RouteFacts {
    let mut route = svolder_host::RouteFacts {
        sink: Some(svolder_host::SinkFacts {
            name: "alsa_output.usb-Schiit_Audio_Gungnir_2-00.analog-stereo".into(),
            description: "Gungnir 2 Analog Stereo".into(),
            rate_hz: Some(48_000),
            format: Some("S32LE".into()),
            volume_milli: Some(1000),
            muted: false,
            usb_vendor_id: Some(0x30be),
            usb_product: Some("Gungnir 2".into()),
        }),
        player: Some(svolder_host::StreamFacts {
            rate_hz: Some(48_000),
            format: Some("S16LE".into()),
            volume_milli: Some(1000),
            muted: false,
        }),
        player_match: svolder_host::PlayerMatch::Pid,
        others_mixing: 0,
        host_eq: false,
    };
    edit(&mut route);
    route
}

/// The mockup's app on a daemon that speaks for the host, with `route` read, the DAC on its
/// first USB input, which is the only USB port connected.
fn daemon_hosting(route: svolder_host::RouteFacts) -> App {
    let mut app = host_mockup_app();
    hear(
        &mut app,
        vec![
            vo("2", Some("input"), json!(1), "1 (USB 1)"),
            vo("7.1", Some("usb1_connected"), json!(true), "true"),
            vo("7.2", Some("usb2_connected"), json!(false), "false"),
        ],
    );
    app.host.fold_daemon(
        svolder_ipc::host::HostView {
            available: true,
            playing: Some(svolder_ipc::host::PlayingView {
                title: "So What".into(),
                artist: "Miles Davis".into(),
                player: "mpv".into(),
                status: "playing".into(),
                ..Default::default()
            }),
            route: Some(route),
            now_ms: 0,
        },
        app.now,
    );
    app
}

/// The Path row sits under the Transport row, the verdict first, in the pane's width; what does
/// not fit is cut with an ellipsis at the pane's right edge.
#[test]
fn the_path_row_sits_under_the_transport_with_the_verdict_first() {
    let rows = text(&screen(&daemon_hosting(gungnir_route(|route| {
        route.host_eq = true
    }))));
    assert_eq!(
        rows[14],
        "│   Transport    ◀◀  ▶  ▮▮  ▶▶       ││                                        │"
    );
    assert_eq!(
        rows[15],
        "│   Path         host EQ on · playe… ││                                        │"
    );
    assert_eq!(
        rows[16],
        "│   Host EQ      off                 ││                                        │"
    );

    let rows = text(&screen(&daemon_hosting(gungnir_route(|_| {}))));
    assert_eq!(
        rows[15],
        "│   Path         untouched · player… ││                                        │"
    );
}

/// A DAC listening to an input the computer does not feed has no verdict on the computer's
/// stream, even at the rate the stream has.
#[test]
fn the_path_row_says_when_the_dac_listens_elsewhere() {
    let mut app = daemon_hosting(gungnir_route(|_| {}));
    hear(
        &mut app,
        vec![vo("2", Some("input"), json!(3), "3 (Optical)")],
    );
    let line = app.host_path().expect("a path");
    assert_eq!(line.words_line(), "the DAC listens to Optical");
    assert!(line.hops.is_empty());
    let rows = text(&screen(&app));
    assert!(rows[15].contains("the DAC listens to"), "{}", rows[15]);
}

/// A DAC on a USB port the computer is not on has no verdict on the computer's stream, and one
/// whose ports cannot be told apart is never `untouched`: the boolean port readings reach the
/// verdict through the TUI as they do through the daemon.
#[test]
fn the_path_row_judges_the_usb_ports() {
    let mut app = daemon_hosting(gungnir_route(|_| {}));
    hear(
        &mut app,
        vec![
            vo("7.1", Some("usb1_connected"), json!(false), "false"),
            vo("7.2", Some("usb2_connected"), json!(true), "true"),
        ],
    );
    let line = app.host_path().expect("a path");
    assert_eq!(
        line.words_line(),
        "the DAC listens to USB 1 · the computer is on USB 2"
    );
    assert!(line.hops.is_empty());

    hear(
        &mut app,
        vec![vo("7.1", Some("usb1_connected"), json!(true), "true")],
    );
    let line = app.host_path().expect("a path");
    assert_eq!(line.words_line(), "both USB ports connected");
}

/// With the room the row has, every hop follows the verdict: the player, the computer and the
/// DAC, each with what is known of the signal there.
#[test]
fn the_path_row_names_every_hop_after_the_verdict() {
    let app = daemon_hosting(gungnir_route(|route| route.host_eq = true));
    let line = app.host_path().expect("a path");
    let columns = widgets::rows::Columns::fit(120, host::PATH.len(), 0, widgets::BAR_MAX);
    let (row, dim) = path_row(&line, &Theme::new(false), columns);
    let said: String = row.spans.iter().map(|span| span.content.as_ref()).collect();
    assert_eq!(
        said.trim_end(),
        "Path host EQ on · player 48 kHz S16LE, volume 100 % → computer 48 kHz S32LE, \
         volume 100 % → DAC USB 1, 48 kHz"
    );
    assert!(!dim);
}

/// An Eitr 2 has no input choice and takes USB alone, so its Path row reads `untouched` once
/// the rate matches, with no input ever reported.
#[test]
fn the_path_row_of_a_dac_with_no_input_control_reads_untouched() {
    let mut app = app_on(profile_named("eitr2"));
    app.host.linked = true;
    hear(
        &mut app,
        vec![vo("9", Some("sample_rate"), json!(2), "2 (48 kHz)")],
    );
    let mut route = gungnir_route(|_| {});
    route.sink.as_mut().expect("a sink").usb_product = Some("Eitr 2".into());
    app.host.fold_daemon(
        svolder_ipc::host::HostView {
            available: true,
            playing: Some(svolder_ipc::host::PlayingView {
                title: "So What".into(),
                player: "mpv".into(),
                status: "playing".into(),
                ..Default::default()
            }),
            route: Some(route),
            now_ms: 0,
        },
        app.now,
    );
    let line = app.host_path().expect("a path");
    assert_eq!(line.words_line(), "untouched");
}
