// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The palette and colour depth the loop draws through: the theme rebuilt on a change, what the red
//! palette draws and what a screen without colour draws.

use super::*;

/// A palette change is not only written down: the theme every screen draws through is built
/// again from it, so the accent is the new palette's own on the very next frame.
#[tokio::test(start_paused = true)]
async fn a_palette_change_rebuilds_the_theme() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut look = colourful_look();
    look.theme.set_family(Family::Dac);
    assert_eq!(
        look.theme.accent_rgb(),
        (0xe0, 0xa0, 0x30),
        "amber accents a DAC in gold"
    );

    let watching = change(
        &mut look,
        watching(backend, app, None),
        TuiChange::Palette("ice".into()),
    )
    .await;

    look.theme.set_family(Family::Dac);
    assert_eq!(
        look.theme.accent_rgb(),
        (0x5f, 0xc9, 0xc9),
        "the theme still draws the palette it was built with"
    );
    watching.close().await;
}

/// A colour depth or a monochrome screen is what the animation layer is capable of at all,
/// so changing either turns the layer and every flag the app draws with off — and turning
/// it back turns them on again.
#[tokio::test(start_paused = true)]
async fn a_depth_or_monochrome_change_turns_the_animation_layer_off_and_on() {
    for (off, on) in [
        (
            TuiChange::Colors(ColorDepth::Ansi16),
            TuiChange::Colors(ColorDepth::TrueColor),
        ),
        (
            TuiChange::Set(&MONOCHROME, TuiValue::Bool(true)),
            TuiChange::Set(&MONOCHROME, TuiValue::Bool(false)),
        ),
    ] {
        let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
        let mut look = colourful_look();
        let mut watching = watching(backend, app, None);
        watching.app.set_look(&look.tui, &look.effects);
        assert!(look.effects.enabled() && watching.app.eased);

        watching = change(&mut look, watching, off.clone()).await;
        assert!(!look.effects.enabled(), "the layer runs after {off:?}");
        assert!(
            !watching.app.effects
                && !watching.app.eased
                && !watching.app.breathing
                && !watching.app.peak_hold,
            "the app still animates after {off:?}"
        );

        watching = change(&mut look, watching, on.clone()).await;
        assert!(
            look.effects.enabled(),
            "the layer is still off after {on:?}"
        );
        assert!(
            watching.app.effects
                && watching.app.eased
                && watching.app.breathing
                && watching.app.peak_hold,
            "the app does not animate again after {on:?}"
        );

        watching.close().await;
    }
}

/// A family accent takes the place of the one key that overrides all three: without that
/// the row would write a colour the screen goes on ignoring.
#[tokio::test(start_paused = true)]
async fn a_family_accent_drops_the_accent_that_overrode_every_family() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let tui = TuiConfig {
        accent: Some("blue".into()),
        ..TuiConfig::default()
    };
    let mut look = Look::new(tui, false, ColorDepth::TrueColor, scratch_config());
    let path = look.path.clone();

    let watching = change(
        &mut look,
        watching(backend, app, None),
        TuiChange::Accent(Family::Dac, Some("#5fc9c9".into())),
    )
    .await;

    assert_eq!(look.tui.accent, None, "the single accent still overrides");
    assert_eq!(look.tui.accents.dac.as_deref(), Some("#5fc9c9"));
    look.theme.set_family(Family::Dac);
    assert_eq!(look.theme.accent_rgb(), (0x5f, 0xc9, 0xc9));
    let saved = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved.tui.accent, None, "the file still carries an accent");
    assert_eq!(saved.tui.accents.dac.as_deref(), Some("#5fc9c9"));

    watching.close().await;
}

/// Switching one animation off reaches the animation layer and the flags the app itself
/// draws with, on a terminal that can show the layer at all.
#[tokio::test(start_paused = true)]
async fn a_switched_animation_reaches_the_layer_and_the_app() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut look = colourful_look();
    let path = look.path.clone();
    let mut watching = watching(backend, app, None);
    watching.app.set_look(&look.tui, &look.effects);
    assert!(
        watching.app.peak_hold,
        "the layer starts with every animation"
    );

    watching = change(
        &mut look,
        watching,
        TuiChange::Effect(EffectSwitch::PeakHold, false),
    )
    .await;
    assert!(!look.tui.effects.peak_hold, "the table took the switch");
    assert!(!watching.app.peak_hold, "and so did what the meters draw");
    assert!(
        look.effects.enabled() && watching.app.effects,
        "the layer itself is still running"
    );
    assert!(
        !Config::parse(&std::fs::read_to_string(&path).unwrap())
            .unwrap()
            .tui
            .effects
            .peak_hold
    );

    // The list fade follows its own switch, and is saved on its own.
    assert!(watching.app.list_fade, "the fade starts on with the layer");
    watching = change(
        &mut look,
        watching,
        TuiChange::Effect(EffectSwitch::ListFade, false),
    )
    .await;
    assert!(!watching.app.list_fade && watching.app.effects);
    assert!(
        !Config::parse(&std::fs::read_to_string(&path).unwrap())
            .unwrap()
            .tui
            .effects
            .list_fade
    );

    // The layer as a whole: nothing about the terminal changed here, so it is the switches
    // the layer was pointed at again that turn it off and on.
    watching = change(&mut look, watching, TuiChange::Effects(false)).await;
    assert!(
        !look.effects.enabled(),
        "the layer runs with its own switch off"
    );
    assert!(!watching.app.effects && !watching.app.breathing && !watching.app.list_fade);

    watching = change(&mut look, watching, TuiChange::Effects(true)).await;
    assert!(look.effects.enabled(), "the layer did not come back on");
    assert!(watching.app.effects && watching.app.breathing);

    watching.close().await;
}

/// An animation in flight when the layer is switched off goes with it: the sweep is
/// dropped rather than frozen half-drawn, so nothing is left running for the loop to hold
/// its thirty-frame tick for and the very next period is the quarter-second one.
#[tokio::test(start_paused = true)]
async fn switching_the_layer_off_drops_what_was_mid_animation() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut look = colourful_look();
    let mut watching = watching(backend, app, None);
    watching.app.set_look(&look.tui, &look.effects);

    look.effects
        .on_screen_change(Rect::new(0, 0, SIZE.0, SIZE.1));
    assert!(look.effects.active(), "the sweep was not armed");
    assert_eq!(
        tick_period(&watching.app, &look.effects),
        FAST_TICK,
        "a running sweep is redrawn a frame at a time"
    );

    watching = change(&mut look, watching, TuiChange::Effects(false)).await;
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    terminal
        .draw(|frame| look.effects.render(frame, Duration::from_millis(33)))
        .expect("the frame draws");

    assert!(
        !look.effects.active(),
        "the sweep outlived the layer it was running in"
    );
    assert_eq!(
        tick_period(&watching.app, &look.effects),
        Duration::from_millis(250),
        "the loop held its fast tick for an animation that can no longer be shown"
    );

    watching.close().await;
}

/// A look on the `red` palette at `depth`, on `transition`, with colour on.
fn red_look(depth: ColorDepth, transition: Transition) -> Look {
    let tui = TuiConfig {
        palette: "red".into(),
        transition,
        ..TuiConfig::default()
    };
    Look::new(tui, false, depth, scratch_config())
}

/// An app on the built-in profile `name` that has heard a reading of every control the
/// `red` cases look at — a volume and a balance to draw bars from, the tone control's mode
/// and bands with band 3 cut so the curve is shaped, and two meters — with a device of its
/// own on the device list.
fn heard(name: &str) -> App {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .expect("the profile is built in")
        .clone();
    let devices = vec![DeviceRow::for_test("gungnir")];
    let mut app = App::new(profile, "gungnir".into(), &TuiConfig::default(), devices);
    let reading = |id: &str, key: Option<&str>, raw: i64| ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: key.map(Into::into),
        label: "?".into(),
        value_type: "u16".into(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    };
    let mut readings = vec![
        reading("10", Some("volume"), 109),
        reading("11", Some("balance"), 70),
        reading("12.50", None, 2),
        reading("20.1", Some("meter_1"), 300),
        reading("20.2", Some("meter_2"), 900),
    ];
    for band in 1..=5 {
        let gain = if band == 3 { 90 } else { 98 };
        readings.push(reading(&format!("12.{band}.2"), None, 64));
        readings.push(reading(&format!("12.{band}.3"), None, gain));
        readings.push(reading(&format!("12.{band}.1"), None, 64));
    }
    let now = Instant::now();
    for value in readings {
        app.apply(Update::Status(value), now);
    }
    app
}

/// The first column of `buffer`'s row `y` from which the row reads `text`, if it does.
fn text_at(buffer: &Buffer, y: u16, text: &str) -> Option<u16> {
    let row: Vec<&str> = (0..buffer.area.width)
        .map(|x| buffer.cell((x, y)).map_or("", |cell| cell.symbol()))
        .collect();
    let wanted: Vec<String> = text.chars().map(String::from).collect();
    (0..=row.len().saturating_sub(wanted.len()))
        .find(|&x| wanted.iter().zip(&row[x..]).all(|(w, r)| w == r))
        .map(|x| x as u16)
}

/// One frame of `app` drawn on `look`'s theme, and where it put its pieces.
fn drawn_on(app: &App, look: &Look) -> (Buffer, FrameAreas) {
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|frame| areas = render::draw(frame, app, &look.theme, &mut hits))
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), areas)
}

/// On `red`, at true colour and at 16 colours, the off-white emphasis draws the level and
/// meter bars, the selected row's inverse and every pane's title, and the accent is left the
/// rule under the header, the key hints, the `▶` marker and the frame the keys are in; every
/// other frame is faint.
#[test]
fn on_red_the_emphasis_draws_bars_selection_and_titles_and_the_accent_the_focus() {
    for depth in [ColorDepth::TrueColor, ColorDepth::Ansi16] {
        let mut look = red_look(depth, Transition::None);
        let mut app = heard("gungnir2-v2");
        colour(&mut look, &app);
        let theme = &look.theme;
        let (emphasis, accent) = (theme.emphasis(), theme.accent());
        let faint = theme.style(Role::Faint).fg.expect("a faint colour");
        assert_ne!(emphasis, accent, "{depth:?}");
        let (buffer, areas) = drawn_on(&app, &look);
        let cell = |x: u16, y: u16| buffer.cell((x, y)).expect("a cell").clone();

        // The volume bar's filled cells.
        let volume = (0..SIZE.1)
            .find(|&y| text_at(&buffer, y, "Volume").is_some())
            .expect("the volume row");
        let filled: Vec<_> = (0..SIZE.0)
            .map(|x| cell(x, volume))
            .filter(|cell| cell.symbol() == "█")
            .collect();
        assert!(
            !filled.is_empty(),
            "{depth:?}: the volume bar filled nothing"
        );
        assert!(
            filled.iter().all(|cell| cell.fg == emphasis),
            "{depth:?}: the volume bar is not in the emphasis"
        );

        // The selected row, and the marker on it.
        let marker = (0..SIZE.1)
            .find_map(|y| text_at(&buffer, y, "▶").map(|x| (x, y)))
            .expect("the marker");
        assert_eq!(cell(marker.0, marker.1).fg, accent, "{depth:?}: the marker");
        let label = cell(marker.0 + 2, marker.1);
        assert_eq!(label.fg, emphasis, "{depth:?}: the selected row");
        assert!(label.modifier.contains(Modifier::REVERSED | Modifier::BOLD));

        // The titles: the focused pane's in the selected row's inverse, the others plain.
        let controls = frame_of(&areas, FrameId::Pane(Pane::Controls));
        let tone = frame_of(&areas, FrameId::Pane(Pane::Tone));
        let x = text_at(&buffer, controls.y, "Controls").expect("the Controls title");
        assert_eq!(
            cell(x, controls.y).fg,
            emphasis,
            "{depth:?}: the focused title"
        );
        assert!(cell(x, controls.y).modifier.contains(Modifier::REVERSED));
        let x = text_at(&buffer, tone.y, "Tone").expect("the Tone control title");
        assert_eq!(
            cell(x, tone.y).fg,
            emphasis,
            "{depth:?}: an unfocused title"
        );
        assert!(!cell(x, tone.y).modifier.contains(Modifier::REVERSED));

        // The frames: the one the keys are in in the accent, the other faint.
        assert_eq!(
            cell(controls.x, controls.y).fg,
            accent,
            "{depth:?}: focused"
        );
        assert_eq!(cell(tone.x, tone.y).fg, faint, "{depth:?}: unfocused frame");

        // The rule under the header, and a key of a border word.
        let rule = areas.header.bottom() - 1;
        assert_eq!(cell(0, rule).symbol(), "─", "{depth:?}: the rule starts");
        assert!(
            (0..SIZE.0)
                .filter(|&x| cell(x, rule).symbol() == "─")
                .all(|x| cell(x, rule).fg == accent),
            "{depth:?}: the rule"
        );
        let hint = text_at(&buffer, controls.y, "0").expect("the centre key");
        assert_eq!(cell(hint, controls.y).fg, accent, "{depth:?}: a key hint");

        // On the tone-control screen, the band table the keys are in and the chart.
        app.key(KeyEvent::from(KeyCode::Char('e')));
        app.tone.row = ToneRow::Band;
        let (buffer, areas) = drawn_on(&app, &look);
        let cell = |x: u16, y: u16| buffer.cell((x, y)).expect("a cell").clone();
        let bands = frame_of(&areas, FrameId::Bands);
        let chart = frame_of(&areas, FrameId::Response);
        assert_eq!(
            cell(bands.x, bands.y).fg,
            accent,
            "{depth:?}: the band table"
        );
        assert_eq!(
            cell(chart.x, chart.y).fg,
            faint,
            "{depth:?}: the chart's frame"
        );
        for (frame, title) in [(bands, "Bands"), (chart, "Response")] {
            let x = text_at(&buffer, frame.y, title).expect("the title");
            assert_eq!(cell(x, frame.y).fg, emphasis, "{depth:?}: {title}");
        }

        // What is written on a frame but is not the frame — the band table's note, the
        // telemetry's scroll mark — stays dimmed text rather than taking the accent of the
        // frame it sits on, at 16 colours as above.
        let dimmed = theme.style(Role::Text).patch(theme.style(Role::Dim));
        app.apply(
            Update::Status(ValueObject {
                heard_at_ms: None,
                id: "12.50".into(),
                key: None,
                label: "?".into(),
                value_type: "u8".into(),
                raw: json!(1),
                display: "1".into(),
                value: None,
                unit: None,
            }),
            Instant::now(),
        );
        let (buffer, areas) = drawn_on(&app, &look);
        let bands = frame_of(&areas, FrameId::Bands);
        let x = text_at(&buffer, bands.bottom() - 1, "bands apply").expect("the note");
        let note = buffer.cell((x, bands.bottom() - 1)).expect("a cell");
        assert_eq!(
            note.fg,
            dimmed.fg.expect("a text colour"),
            "{depth:?}: the note"
        );
        app.key(KeyEvent::from(KeyCode::Char('t')));
        let (buffer, areas) = drawn_on(&app, &look);
        let list = frame_of(&areas, FrameId::Screen);
        let mark = (list.y..list.bottom())
            .filter_map(|y| buffer.cell((list.right() - 1, y)))
            .find(|cell| cell.symbol() == widgets::scroll::MORE_BELOW)
            .expect("the telemetry's scroll mark");
        assert_eq!(
            mark.fg,
            dimmed.fg.expect("a text colour"),
            "{depth:?}: the mark"
        );

        // An amp's meters.
        let amp = heard("fake-amp");
        colour(&mut look, &amp);
        let (buffer, _) = drawn_on(&amp, &look);
        for meter in ["Meter L", "Meter R"] {
            let y = (0..SIZE.1)
                .find(|&y| text_at(&buffer, y, meter).is_some())
                .expect("the meter row");
            let cells: Vec<_> = (0..SIZE.0)
                .filter_map(|x| buffer.cell((x, y)))
                .filter(|cell| cell.symbol() == "█")
                .collect();
            assert!(!cells.is_empty(), "{depth:?}: {meter} filled nothing");
            assert!(
                cells.iter().all(|cell| cell.fg == look.theme.emphasis()),
                "{depth:?}: {meter} is not in the emphasis"
            );
        }
    }
}

/// With colour off, no palette draws a colour anywhere on any screen — the tone control's
/// mode arrows and `red`'s emphasis on the titles of the frames it draws faint included — at
/// whatever depth the terminal reports.
#[test]
fn with_colour_off_no_screen_draws_a_colour() {
    for palette in ["amber", "ice", "red", "neon"] {
        for depth in [ColorDepth::TrueColor, ColorDepth::Ansi16] {
            let tui = TuiConfig {
                palette: palette.into(),
                transition: Transition::None,
                ..TuiConfig::default()
            };
            let look = Look::new(tui, true, depth, scratch_config());
            for key in [None, Some('e'), Some('d'), Some('t'), Some('s'), Some('?')] {
                let mut app = heard("gungnir2-v2");
                if let Some(key) = key {
                    app.key(KeyEvent::from(KeyCode::Char(key)));
                }
                let (buffer, _) = drawn_on(&app, &look);
                let coloured = buffer
                    .content()
                    .iter()
                    .find(|cell| cell.fg != Color::Reset || cell.bg != Color::Reset);
                assert_eq!(
                    coloured, None,
                    "{palette} at {depth:?}, after {key:?}: a cell in colour"
                );
            }
        }
    }
}

/// On `red` the empty cells of a focused row's bar take the emphasis's dim shade — a grey
/// at true colour, the white dimmed at 16 colours — the shade [`Theme::bar_dim`] names, so
/// the bar reads as part of the off-white row it sits on.
#[test]
fn on_red_a_focused_bars_empty_cells_take_the_emphasis_dim_shade() {
    for (depth, wanted) in [
        (
            ColorDepth::TrueColor,
            Style::default().fg(Color::Rgb(0x61, 0x60, 0x5e)),
        ),
        (
            ColorDepth::Ansi16,
            Style::default().fg(Color::Gray).add_modifier(Modifier::DIM),
        ),
    ] {
        let mut look = red_look(depth, Transition::None);
        let mut app = heard("gungnir2-v2");
        // Input, Phase invert, NOS filter, Mute, then Volume.
        app.selected = 4;
        colour(&mut look, &app);
        assert_eq!(look.theme.bar_dim(), wanted, "{depth:?}");
        let (buffer, _) = drawn_on(&app, &look);
        let volume = (0..SIZE.1)
            .find(|&y| text_at(&buffer, y, "Volume").is_some())
            .expect("the volume row");
        assert!(
            text_at(&buffer, volume, "▶").is_some(),
            "{depth:?}: the cursor is not on the volume row"
        );
        let empty: Vec<_> = (0..SIZE.0)
            .filter_map(|x| buffer.cell((x, volume)))
            .filter(|cell| cell.symbol() == "░")
            .collect();
        assert!(!empty.is_empty(), "{depth:?}: the volume bar is full");
        for cell in empty {
            assert_eq!(
                (cell.fg, cell.modifier),
                (wanted.fg.expect("a colour"), wanted.add_modifier),
                "{depth:?}: an empty cell of the focused volume bar"
            );
        }
    }
}

/// No cell of any screen `red` draws — the device screen with either pane focused, the
/// tone-control screen shaped and flat, Settings, the help overlay, the device list and the
/// telemetry, an amp's meters — is blue or cyan, at true colour with the animation layer
/// running (the trace mid-way round and the noise between two screens) and at 16 colours.
#[test]
fn on_red_no_screen_draws_a_blue_or_cyan_cell() {
    let bluish = |buffer: &Buffer| {
        buffer.content().iter().find_map(|cell| {
            [cell.fg, cell.bg]
                .into_iter()
                .filter(|colour| *colour != Color::Reset)
                .find(|colour| crate::tui::theme::bluish(palette::rgb_of(*colour)))
                .map(|colour| format!("{:?} on {:?}", colour, cell.symbol()))
        })
    };
    for depth in [ColorDepth::TrueColor, ColorDepth::Ansi16] {
        for transition in [Transition::Trace, Transition::Noise] {
            let mut scenes: Vec<(&str, App)> = Vec::new();
            scenes.push(("device", heard("gungnir2-v2")));
            let mut app = heard("gungnir2-v2");
            app.pane = Pane::Tone;
            scenes.push(("device, tone pane", app));
            for (what, key) in [
                ("tone", 'e'),
                ("settings", 's'),
                ("devices", 'd'),
                ("telemetry", 't'),
                ("help", '?'),
            ] {
                let mut app = heard("gungnir2-v2");
                app.key(KeyEvent::from(KeyCode::Char(key)));
                scenes.push((what, app));
            }
            let mut flat = heard("gungnir2-v2");
            flat.key(KeyEvent::from(KeyCode::Char('e')));
            flat.key(KeyEvent::from(KeyCode::Char('0')));
            scenes.push(("tone, flattened", flat));
            scenes.push(("amp", heard("fake-amp")));
            for (what, mut app) in scenes {
                let mut look = red_look(depth, transition);
                // The first frame is the screen as it arrives; the screen change the key
                // asked for, if it asked for one, plays over the second.
                let navigated = std::mem::take(&mut app.frame_flags.navigated);
                colour(&mut look, &app);
                let first = turn(&mut app, &mut look, SIZE, Duration::ZERO);
                app.frame_flags.navigated = navigated;
                let drawn = format!("{what} at {depth:?} under {transition:?}");
                let second = turn(&mut app, &mut look, SIZE, Duration::from_millis(120));
                if navigated && transition == Transition::Noise {
                    assert!(noisy(&second) > 0, "{drawn}: no noise played");
                }
                for (when, frame) in [("first", first), ("120 ms on", second)] {
                    assert_eq!(bluish(&frame), None, "{drawn}, {when}: a blue or cyan cell");
                }
            }
        }
    }
}
