// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An imported preset: the target curve, the gap and the side pane.

use super::*;

/// The cells of `buffer` inside `curve` that hold one of `symbols`.
fn cells_of(buffer: &Buffer, curve: Rect, symbols: &str) -> Vec<(u16, u16)> {
    let mut found = Vec::new();
    for y in curve.y..curve.bottom() {
        for x in curve.x..curve.right() {
            let cell = buffer.cell((x, y)).expect("a cell of the curve");
            if cell
                .symbol()
                .chars()
                .next()
                .is_some_and(|c| symbols.contains(c))
            {
                found.push((x, y));
            }
        }
    }
    found
}

/// The quadrant glyphs the target is dotted in.
const QUADRANTS: &str = "▖▗▘▙▚▛▜▝▞▟";

/// The header's text row of `app` drawn `width` by `height`: the wordmark takes the rows above it
/// where there is room for one.
fn header_row(app: &App, width: u16, height: u16) -> String {
    text(&frame_with_theme(app, width, height, &Theme::new(false)))
        .into_iter()
        .find(|row| row.contains("mouse:"))
        .expect("the header row")
}

/// The target and the gap are drawn only for an imported preset that is on the device or loaded
/// with changes: not for a band preset, not for no preset at all, and not while the imported one
/// is only being loaded.
#[test]
fn the_target_is_drawn_for_an_imported_preset_only() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let curve = tone::curve_area(Rect::new(0, 0, 80, 24), 5);
    let drawn = |app: &App| cells_of(&frame_with_theme(app, 80, 24, &theme), curve, QUADRANTS);

    assert!(
        drawn(&flat_app()).is_empty(),
        "no imported preset, no target"
    );
    let mut warm = flat_app();
    warm.presets.loaded_preset = Some("Warm".into());
    assert!(drawn(&warm).is_empty(), "a built-in has no target");
    let changed = imported_app();
    assert!(
        !drawn(&changed).is_empty(),
        "loaded with changes draws the target"
    );
    assert!(
        !drawn(&fitted_app()).is_empty(),
        "on the device draws the target"
    );
    let mut other = imported_app();
    other.presets.loaded_preset = Some("Other".into());
    assert!(
        drawn(&other).is_empty(),
        "a preset that is not in the file has none"
    );
}

/// The target is dotted in the dim target role and never in a Braille cell, which the spectrum
/// recolours, nor in the dot a band's centre is marked with.
#[test]
fn the_target_is_dim_and_not_in_braille_cells() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let app = imported_app();
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let curve = tone::curve_area(Rect::new(0, 0, 80, 24), 5);
    let want = theme.style(Role::CurveTarget).fg.expect("a colour");
    let dots = cells_of(&buffer, curve, QUADRANTS);
    assert!(!dots.is_empty());
    for (x, y) in dots {
        assert_eq!(buffer.cell((x, y)).unwrap().fg, want, "({x}, {y})");
    }
}

/// The gap is filled in the warning colour where the fit misses the target by more than 2 dB,
/// and in no column where it does not: flat bands against the preset's +5 dB bass shelf miss
/// it, and the preset's own fit does not.
#[test]
fn the_gap_is_filled_in_the_warning_colour_where_the_miss_exceeds_two_db() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let curve = tone::curve_area(Rect::new(0, 0, 80, 24), 5);
    let warn = theme.style(Role::CurveGap).fg.expect("a colour");
    let filled = |app: &App| -> Vec<(u16, u16)> {
        let buffer = frame_with_theme(app, 80, 24, &theme);
        cells_of(&buffer, curve, "░")
            .into_iter()
            .filter(|&(x, y)| buffer.cell((x, y)).unwrap().fg == warn)
            .collect()
    };
    let miss = filled(&imported_app());
    assert!(
        !miss.is_empty(),
        "the lifted band misses a +5 dB shelf and a notch"
    );
    assert!(
        filled(&fitted_app()).is_empty(),
        "the preset's own fit is within 2 dB"
    );
}

/// The gap is drawn under both curves: in a column where the fill reaches the row of the fit's
/// own line, the cell on that row holds the line's Braille (or a band's handle), not the fill.
#[test]
fn the_curve_is_drawn_over_the_gap() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let app = imported_app();
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let curve = tone::curve_area(Rect::new(0, 0, 80, 24), 5);
    let zero = tone::zero_row(curve);
    let symbol = |x: u16, y: u16| buffer.cell((x, y)).unwrap().symbol().to_string();
    let is_braille = |text: &str| {
        text.chars()
            .next()
            .is_some_and(|c| ('\u{2801}'..='\u{28ff}').contains(&c))
    };
    // The bass bands sit at 0 dB under the preset's +5 dB shelf, so the fill runs down to the
    // 0 dB row, where the fit's line is drawn, in the columns the shelf covers.
    let bass = tone::column_at(curve, 50f64.log10()).unwrap();
    let covered: Vec<u16> = (curve.x..bass)
        .filter(|&x| symbol(x, zero - 1) == "░")
        .collect();
    assert!(!covered.is_empty(), "the fill reaches the fit's row");
    for x in covered {
        let there = symbol(x, zero);
        assert!(
            is_braille(&there) || there == "•",
            "column {x}: {there:?} is not the fit's line or a handle"
        );
    }
}

/// The preview of the imported preset `Phones` against the bands standing on the device now,
/// worked out as the daemon's overlay works it out: `Preview::of_bands` (which reads the shared
/// `fit::miss`) on the web's `PREVIEW_POINTS` points.
fn preview_of_the_bands_on_the_device(app: &App) -> svolder_profiles::tone::fit::Preview {
    use svolder_profiles::tone::fit::{PREVIEW_POINTS, Preview};
    use svolder_profiles::tone::response::{BandParams, log_freqs};
    let eq = app.profile.eq.as_ref().expect("a tone control");
    let correction = &app.presets.saved[0]
        .source
        .as_ref()
        .expect("an imported preset")
        .correction;
    let bands: Vec<BandParams> = (0..eq.bands.len())
        .map(|band| {
            let raw = |param| {
                let id = eq.band_id(band, param).expect("a band");
                u8::try_from(app.raw_of(id).expect("a band that is read")).expect("a step")
            };
            BandParams {
                f0: eq
                    .frequency(band, raw(svolder_profiles::EqParam::Frequency))
                    .expect("a frequency"),
                gain_db: eq.gain_db(raw(svolder_profiles::EqParam::Gain)),
                q: eq.q(raw(svolder_profiles::EqParam::Q)),
            }
        })
        .collect();
    Preview::of_bands(correction, &bands, &log_freqs(PREVIEW_POINTS))
}

/// How far those bands miss the preset, in dB RMS.
fn rms_of_the_bands_on_the_device(app: &App) -> f64 {
    preview_of_the_bands_on_the_device(app)
        .fit
        .expect("a device preview has a fit")
        .rms_db
}

/// The header says how well the bands on the device fit the imported preset — `fit 0.9 dB` —
/// after the preset's name, for an imported preset only, and a band preset's header is as it was.
/// The figure is the bands' own, as the web page's overlay says it: it moves with a band edit
/// and is not the preset's own fit, which only the bands the fit made have.
#[test]
fn the_header_names_the_fit_of_the_bands_against_an_imported_preset() {
    let app = fitted_app();
    let on = rms_of_the_bands_on_the_device(&app);
    let at = |app: &App, width| header_row(app, width, 30);
    let header = at(&app, 126);
    let want = format!("Preset Phones  fit {on:.1} dB");
    assert!(header.contains(&want), "|{header}|");
    assert!(!at(&app, 80).contains("fit "), "no room at 80 columns");

    // Loaded with changes: the bands are band 2 lifted, and the header says how far those miss.
    let changed = imported_app();
    let off = rms_of_the_bands_on_the_device(&changed);
    assert!(
        (off - on).abs() >= 0.1,
        "the two cases must differ for this to mean something: {on} {off}"
    );
    let header = at(&changed, 126);
    assert!(
        header.contains(&format!("Preset Phones*  fit {off:.1} dB")),
        "|{header}|"
    );
    let mut warm = flat_app();
    warm.presets.loaded_preset = Some("Warm".into());
    let header = at(&warm, 126);
    assert!(!header.contains("fit "), "|{header}|");
}

/// A band edit moves the header's figure at once, with no new load of the preset: the figure
/// follows the bands the device holds, a dragged point's value included, and is left out while a
/// band is unread, as the page leaves out its overlay.
#[test]
fn a_band_edit_moves_the_headers_fit_and_an_unread_band_leaves_it_out() {
    let mut app = fitted_app();
    let before = header_row(&app, 126, 30);
    // Band 2 up to +6 dB, as a key press on the gain row would leave it.
    hear(&mut app, vec![vo("12.2.3", 127)]);
    let after = header_row(&app, 126, 30);
    let rms = rms_of_the_bands_on_the_device(&app);
    assert!(after.contains(&format!("fit {rms:.1} dB")), "|{after}|");
    assert_ne!(before, after, "the edit left the header as it was");

    let mut unread = imported_app();
    let eq = unread.profile.eq.clone().expect("a tone control");
    let gain = eq
        .band_id(1, svolder_profiles::EqParam::Gain)
        .expect("a band of the profile");
    unread.readings.values.remove(&gain);
    let header = header_row(&unread, 126, 30);
    assert!(!header.contains("fit "), "|{header}|");
}

/// The fit leaves the header where there is no room for it whole: the name is cut first.
#[test]
fn the_fit_gives_way_in_a_crowded_header() {
    let mut app = fitted_app();
    app.session.product = "Forkbeard Gungnir 2 Limited Anniversary Edition".into();
    let header = header_row(&app, 80, 24);
    assert!(!header.contains("fit "), "|{header}|");
    assert!(header.trim_end().ends_with("mouse:on"), "|{header}|");
}

/// The side pane is only in the wide tier: the source and the filters, the preamp and the fit
/// line. The tiers below have the chart alone, as wide as ever.
#[test]
fn the_side_pane_is_in_the_wide_tier_only() {
    let app = fitted_app();
    let wide = frame_with_theme(&app, 126, 40, &Theme::new(false));
    let rows = text(&wide).join("\n");
    for wanted in [
        "Correction",
        "Imported · 2 filters",
        "LSC   105",
        "+5.0",
        "Preamp -3.0 dB",
        "fit ",
    ] {
        assert!(rows.contains(wanted), "no {wanted:?} in\n{rows}");
    }
    for (width, height) in [(80, 24), (119, 39), (100, 45)] {
        let rows = text(&frame_with_theme(&app, width, height, &Theme::new(false))).join("\n");
        assert!(!rows.contains("Correction"), "{width}×{height}\n{rows}");
        assert!(!rows.contains("Imported ·"), "{width}×{height}");
    }
    // A band preset has no side pane at any size.
    let rows = text(&frame_with_theme(&flat_app(), 126, 40, &Theme::new(false))).join("\n");
    assert!(!rows.contains("Correction"));
}

/// The side pane takes columns from the chart, and everything read back from the layout — the
/// curve's cells, the frames — follows the chart.
#[test]
fn the_curve_is_narrower_beside_the_side_pane() {
    let area = Rect::new(0, 0, 126, 40);
    let plain = flat_app();
    let with = fitted_app();
    let paint = |app: &App| {
        let mut terminal = Terminal::new(TestBackend::new(126, 40)).expect("a terminal");
        let mut hits = HitMap::default();
        let mut areas = render::FrameAreas::default();
        terminal
            .draw(|f| areas = render::draw(f, app, &Theme::new(false), &mut hits))
            .expect("the frame draws");
        areas
    };
    let (a, b) = (paint(&plain), paint(&with));
    let (a, b) = (a.curve.expect("a curve"), b.curve.expect("a curve"));
    assert_eq!(a, tone::curve_area(area, 5));
    assert_eq!(b.width + tone::fit::SIDE_WIDTH + 1, a.width);
    assert_eq!(b.x, a.x);
}

/// The imported preset's view is worked out once and kept: the same one comes back until the
/// presets are read again.
#[test]
fn the_target_view_is_kept_between_frames_and_dropped_with_the_presets() {
    let app = imported_app();
    let first = app.target_view("Phones").expect("an imported preset");
    let again = app.target_view("Phones").expect("kept");
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert!(app.target_view("Warm").is_none(), "not an imported preset");
    assert!(
        app.target_view("Phones").is_some(),
        "the miss did not drop it"
    );
    app.tone.target_cache.clear();
    let after = app.target_view("Phones").expect("worked out again");
    assert!(!std::sync::Arc::ptr_eq(&first, &after));
}

/// The target is read at any frequency off the points it is kept at: the preset's +5 dB shelf
/// is flat above its corner and its own response where the shelf bends, and nothing outside the
/// audio band.
#[test]
fn the_target_reads_back_the_correction_at_any_frequency() {
    let app = imported_app();
    let view = app.target_view("Phones").expect("an imported preset");
    let source = app.presets.saved[0].source.as_ref().unwrap();
    for hz in [20.0, 37.0, 105.0, 800.0, 3000.0, 9000.0, 20000.0] {
        let want = svolder_profiles::tone::response::correction_db(&source.correction, &[hz])[0];
        let got = view.db_at(hz).expect("inside the band");
        assert!((got - want).abs() < 0.05, "{hz} Hz: {got} against {want}");
    }
    assert_eq!(view.db_at(10.0), None);
    assert_eq!(view.db_at(30000.0), None);
}

/// Every palette tier draws the target and the gap in a way that reads: true colour and 256
/// colours in the dim and warning colours, 16 colours in the dim attribute and the terminal's
/// yellow, and monochrome with no colour at all, the gap dim so it is not a solid bar.
#[test]
fn the_target_and_the_gap_read_at_every_depth_and_in_monochrome() {
    let app = imported_app();
    let curve = tone::curve_area(Rect::new(0, 0, 80, 24), 5);
    let mono = Theme::from_config(
        &TuiConfig {
            monochrome: true,
            ..TuiConfig::default()
        },
        false,
        ColorDepth::TrueColor,
    );
    let tiers = [
        ("true colour", theme_for("amber", ColorDepth::TrueColor)),
        ("256 colours", theme_for("amber", ColorDepth::Ansi256)),
        ("16 colours", theme_for("amber", ColorDepth::Ansi16)),
        ("monochrome", mono),
    ];
    for (what, theme) in tiers {
        let buffer = frame_with_theme(&app, 80, 24, &theme);
        let target = cells_of(&buffer, curve, QUADRANTS);
        let gap = cells_of(&buffer, curve, "░");
        assert!(!target.is_empty(), "{what}: no target");
        assert!(!gap.is_empty(), "{what}: no gap");
        for (x, y) in target {
            let cell = buffer.cell((x, y)).unwrap();
            assert_eq!(
                cell.fg,
                theme.style(Role::CurveTarget).fg.unwrap_or(cell.fg),
                "{what}"
            );
            assert_eq!(
                cell.modifier.contains(Modifier::DIM),
                theme
                    .style(Role::CurveTarget)
                    .add_modifier
                    .contains(Modifier::DIM),
                "{what}"
            );
        }
    }
    let sixteen = theme_for("amber", ColorDepth::Ansi16);
    assert_eq!(
        sixteen.style(Role::CurveGap).fg,
        sixteen.style(Role::Warn).fg,
        "the gap is the warning colour"
    );
    assert_ne!(sixteen.style(Role::CurveGap).fg, None);
    assert!(
        sixteen
            .style(Role::CurveTarget)
            .add_modifier
            .contains(Modifier::DIM)
    );
}

/// A preset rewritten under the same name — by another front end, or `svolder eq import` — shows
/// its new target: the view is keyed by the correction as well as the name.
#[test]
fn the_same_name_with_new_filters_shows_the_new_target() {
    use svolder_profiles::tone::apo::{FilterKind, SourceFilter};
    let mut app = imported_app();
    let before = app.target_view("Phones").expect("an imported preset");
    let source = app.presets.saved[0].source.as_mut().expect("imported");
    source.correction.filters = vec![SourceFilter {
        kind: FilterKind::Peaking,
        f: 1000.0,
        g: 4.0,
        q: 1.0,
    }];
    let after = app.target_view("Phones").expect("still imported");
    assert!(!std::sync::Arc::ptr_eq(&before, &after), "a stale view");
    assert!(after.db_at(1000.0).unwrap() > 3.5);
    assert!(before.db_at(1000.0).unwrap() < 1.0);
    assert_eq!(after.filters.len(), 1);
}

/// The side pane's fit line is the bands on the device's too, in the header's words and on the
/// header's points: after a band edit the pane and the header say one figure, and the pane's
/// worst miss and volume to lower are the bands' own.
#[test]
fn the_side_pane_says_the_fit_of_the_bands_on_the_device() {
    let app = imported_app();
    let preview = preview_of_the_bands_on_the_device(&app);
    let words = preview.fit_line().expect("a device preview has a fit");
    let rows = text(&frame_with_theme(&app, 126, 30, &Theme::new(false))).join(" ");
    let squeezed = rows.split_whitespace().collect::<Vec<_>>().join(" ");
    // The pane wraps the line, so its first words are looked for whole.
    let fit = preview.fit.as_ref().expect("a device preview has a fit");
    let first = format!(
        "fit {:.1} dB RMS · worst {:.1} dB",
        fit.rms_db, fit.worst_db
    );
    assert!(squeezed.contains(&first), "{first:?} not in |{squeezed}|");
    let own = svolder_profiles::tone::preset::fit_for(
        app.profile.eq.as_ref().unwrap(),
        app.presets.saved[0].source.as_ref().unwrap(),
    )
    .line();
    assert_ne!(
        words, own,
        "the fixture must differ from the preset's own fit"
    );
    assert!(
        !squeezed.contains(&own),
        "the preset's own fit is on the pane"
    );
}
