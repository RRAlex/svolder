// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The connection strip and the level history in the tall tier.

use super::*;

/// The wall clock the link strip's cases hold, in unix milliseconds.
const WALL: u64 = 1_800_000_000_000;

const HOUR_MS: u64 = 3_600_000;

/// The mockup app with a link history held at [`WALL`]: first seen up a day ago, down for an
/// hour three hours back, up since.
fn linked_app() -> App {
    crate::tui::widgets::graph::hold_wall_ms(WALL);
    let mut app = mockup_app();
    app.readings.link.seed([
        (WALL - 30 * HOUR_MS, true),
        (WALL - 4 * HOUR_MS, false),
        (WALL - 3 * HOUR_MS, true),
    ]);
    app
}

/// The rows of `app` at `width`×`height` that carry the `Link` row.
fn link_rows(app: &App, width: u16, height: u16) -> Vec<String> {
    text(&frame(app, width, height))
        .into_iter()
        .filter(|row| row.contains("Link"))
        .collect()
}

/// The wide tier draws the strip: connected on the top dots, the hour down on the bottom
/// ones, and the span named at its end.
#[test]
fn the_wide_tier_draws_the_connection_strip_in_the_status_pane() {
    let rows = text(&frame(&linked_app(), 120, 24));
    assert_eq!(rows.len(), 24);
    assert_eq!(
        &rows[19..],
        [
            "┌ Status ────────────────────────────────────────────────────────────────────────────────────────────────────── advice ┐",
            "│  Sample rate 48 kHz   fw 3                                                                                           │",
            "│  Raw: 9=u8 64   9.1=u8 0   9.2=u8 1   (3 unknown functions)                                                          │",
            "│  Link  ⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⣀⣀⣀⣀⣀⣀⣀⣀⡈⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉  12 h  │",
            "└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘",
        ]
    );
}

/// The tall tier draws it too, in the narrow width, and a frame short of both tiers draws
/// the screen as it always did.
#[test]
fn the_tall_tier_draws_the_strip_and_the_smallest_terminal_does_not() {
    let rows = text(&frame(&linked_app(), 80, 40));
    assert_eq!(&rows[33..], STRIP_80X40);
    crate::tui::test_support::assert_usage_md_shows(&rows[34..], "link strip mockup");

    let app = linked_app();
    let small = text(&frame(&app, 80, 24));
    assert!(small.iter().all(|row| !row.contains("Link")), "{small:#?}");
    let mut off = linked_app();
    off.tui.graphs = false;
    assert_eq!(small, text(&frame(&off, 80, 24)));
    let mut off = linked_app();
    off.tui.graphs = false;
    assert_eq!(text(&frame(&app, 119, 39)), text(&frame(&off, 119, 39)));
}

/// With graphs off the row is gone and its line goes back to the Controls pane.
#[test]
fn graphs_off_removes_the_link_row_and_gives_its_line_back() {
    let on = text(&frame(&linked_app(), 120, 24));
    let mut app = linked_app();
    app.tui.graphs = false;
    let off = text(&frame(&app, 120, 24));
    assert!(off.iter().all(|row| !row.contains("Link")));
    let status_at = |rows: &[String]| rows.iter().position(|r| r.starts_with("┌ Status"));
    assert_eq!(status_at(&on).map(|at| at + 1), status_at(&off));
    assert_eq!(on.len(), off.len());
}

/// A live connection event extends the strip: a link that drops puts a bottom dot at its end.
#[test]
fn a_live_connection_event_extends_the_strip() {
    let mut app = linked_app();
    let before = link_rows(&app, 120, 24);
    app.apply(Update::Connection { connected: false }, app.now);
    let after = link_rows(&app, 120, 24);
    assert_ne!(before, after);
    let strip = after[0]
        .trim_end_matches(['│', ' '])
        .trim_end_matches("12 h")
        .trim_end();
    assert_eq!(strip.chars().last(), Some('⢁'), "{after:?}");
}

/// A clock that stepped back leaves changes stamped after now: they are not drawn, and
/// nothing panics or draws outside the strip.
#[test]
fn a_clock_stepped_back_never_panics_or_overdraws() {
    let mut app = linked_app();
    app.readings.link.record(WALL + 5 * HOUR_MS, false);
    app.readings.link.record(WALL + 6 * HOUR_MS, true);
    let rows = link_rows(&app, 120, 24);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].chars().count(), 120, "{rows:?}");
    // Now before everything it knows.
    crate::tui::widgets::graph::hold_wall_ms(0);
    assert_eq!(link_rows(&app, 120, 24).len(), 1);
}

/// The Status pane of [`linked_app`] at 80×40.
const STRIP_80X40: [&str; 7] = [
    "                                                                                ",
    "┌ Status ────────────────────────────────────────────────────────────── advice ┐",
    "│                                                                              │",
    "│ Sample rate 48 kHz   fw 3                                                    │",
    "│ Raw: 9=u8 64   9.1=u8 0   9.2=u8 1   (3 unknown functions)                   │",
    "│ Link  ⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⣀⣀⣀⣀⣀⣀⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉  12 h│",
    "└──────────────────────────────────────────────────────────────────────────────┘",
];

/// A drop of half a minute in a 12 h span, far shorter than a column, still draws a bottom dot.
#[test]
fn a_short_drop_still_draws_a_bottom_dot() {
    crate::tui::widgets::graph::hold_wall_ms(WALL);
    let mut app = mockup_app();
    app.readings.link.seed([
        (WALL - 20 * HOUR_MS, true),
        (WALL - 6 * HOUR_MS, false),
        (WALL - 6 * HOUR_MS + 30_000, true),
    ]);
    let rows = link_rows(&app, 120, 24);
    assert_eq!(
        rows.iter().filter(|row| row.contains('\u{2848}')).count(),
        1,
        "{rows:?}"
    );
}

/// How far a level-history case runs: the span its graphs draw.
const HISTORY: Duration = Duration::from_secs(300);

/// `profile` hearing `channels` meters at 250 ms steps over [`HISTORY`], channel `n` at
/// `level(n, step)` of `full` raw, every instant explicit. With no `full`, a level is a
/// fraction of the meter's live part, which the meter's own scale turns into the raw reading.
fn metering(
    profile: Profile,
    channels: usize,
    full: Option<f64>,
    level: impl Fn(usize, u64) -> f64,
) -> App {
    let base = Instant::now();
    let mut app = app_on(profile);
    let steps = HISTORY.as_millis() as u64 / 250;
    for step in 0..=steps {
        let at = base + Duration::from_millis(step * 250);
        for channel in 0..channels {
            let id = format!("20.{}", channel + 1);
            let key = format!("meter_{}", channel + 1);
            let raw = match full {
                Some(full) => (level(channel, step) * full).round() as i64,
                None => {
                    let meter = app.profile.control_by_key(&key).expect("a meter");
                    let placed = app.placed(meter).expect("the meter has a scale");
                    placed.raw_at(level(channel, step))
                }
            };
            let value = ValueObject {
                value_type: "u16".into(),
                ..vo(&id, Some(&key), json!(raw), &raw.to_string())
            };
            app.apply(Update::Status(value), at);
        }
    }
    app.now = base + HISTORY + Duration::from_millis(1);
    app
}

/// What the stereo cases play: the left channel rises over the span, the right falls, and any
/// further channel holds a level of its own.
fn ramp(channel: usize, step: u64) -> f64 {
    let up = step as f64 / 1200.0;
    match channel {
        0 => up,
        1 => 1.0 - up,
        n => n as f64 / 6.0,
    }
}

/// A fake amp whose meters play [`ramp`].
fn metering_fake(name: &str, channels: usize) -> App {
    metering(profile_named(name), channels, None, ramp)
}

/// An amp whose meters carry no scale, as a user's profile may leave them: the left rises to
/// 1200, the right holds 300.
fn metering_real() -> App {
    metering(
        without_scale(profile_named("vidar2")),
        2,
        Some(1200.0),
        |channel, step| match channel {
            0 => step as f64 / 1200.0,
            _ => 0.25,
        },
    )
}

/// A DAC hearing the host's capture play [`ramp`] on its left and right over [`HISTORY`].
fn hosting() -> App {
    let mut app = sinking(0.54, false, "USB DAC");
    app.host.capturing = true;
    let base = Instant::now();
    for step in 0..=HISTORY.as_millis() as u64 / 250 {
        let block = svolder_host::Block {
            bands: [0.0; 16],
            left: ramp(0, step) as f32,
            right: ramp(1, step) as f32,
        };
        app.heard_block(&block, base + Duration::from_millis(step * 250));
    }
    app.now = base + HISTORY + Duration::from_millis(1);
    app
}

/// The rows of a frame of `app` that carry a level graph: braille outside the `Link` row.
fn graph_rows(app: &App, width: u16, height: u16) -> Vec<String> {
    text(&frame(app, width, height))
        .into_iter()
        .filter(|row| row.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)))
        .filter(|row| !row.contains("Link"))
        .filter(|row| !row.contains('\u{2588}') && !row.contains('\u{2591}'))
        .collect()
}

/// The level rows of a stereo amp at 80×40.
const STEREO_80: [&str; 2] = [
    "│   L/R  ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿ │",
    "│        ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉ │",
];

/// The level rows of a stereo amp at 120×40.
const STEREO_120: [&str; 2] = [
    "│    L/R  ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣠⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿  │",
    "│         ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠋⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉  │",
];

/// The level rows of a mono amp at 80×40.
const MONO_80: [&str; 1] =
    ["│   M    ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣠⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿ │"];

/// The level rows of a mono amp at 120×40.
const MONO_120: [&str; 1] = [
    "│    M    ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿  │",
];

/// The level rows of a five-channel amp at 80×40.
const FIVE_80: [&str; 5] = [
    "│   1    ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿ │",
    "│   2    ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀ │",
    "│   3    ⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤ │",
    "│   4    ⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶ │",
    "│   5    ⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶ │",
];

/// The level rows of a five-channel amp at 120×40.
const FIVE_120: [&str; 5] = [
    "│    1    ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣠⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿  │",
    "│    2    ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣄⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀  │",
    "│    3    ⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤  │",
    "│    4    ⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶  │",
    "│    5    ⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶  │",
];

/// The level rows of a real amp with unscaled meters at 80×40.
const REAL_80: [&str; 2] = [
    "│   L/R  ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣠⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣴⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿ │",
    "│        ⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛ │",
];

/// The level rows of a real amp with unscaled meters at 120×40.
const REAL_120: [&str; 2] = [
    "│    L/R  ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣴⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿  │",
    "│         ⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛  │",
];

/// The level rows of the host at 80×40.
const HOST_80: [&str; 2] = [
    "│   host ⣀⣀⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣾⣿⣿⣿⣿ ││                                        │",
    "│        ⣿⣿⣿⣿⡿⠿⠿⠿⠿⠿⠿⠿⠿⠟⠛⠛⠛⠛⠛⠛⠛⠛⠋⠉⠉⠉⠉ ││                                        │",
];

/// The level rows of the host at 120×40.
const HOST_120: [&str; 2] = [
    "│    host ⣀⣀⣀⣀⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣾⣿⣿⣿⣿⣿⣿  │ │                                                                   │",
    "│         ⣿⣿⣿⣿⣿⣿⡿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠟⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠉⠉⠉⠉⠉⠉  │ │                                                                   │",
];

/// Each source's level history, mirrored where it has a left and a right and a row a channel
/// otherwise, after the meter group (the Host group for the host), at both tall widths.
#[test]
fn the_tall_tier_draws_the_level_history_of_each_source() {
    let cases = [
        (
            metering_fake("fake-amp", 2),
            STEREO_80.to_vec(),
            STEREO_120.to_vec(),
        ),
        (
            metering_fake("fake-mono", 1),
            MONO_80.to_vec(),
            MONO_120.to_vec(),
        ),
        (
            metering_fake("fake-five", 5),
            FIVE_80.to_vec(),
            FIVE_120.to_vec(),
        ),
        (metering_real(), REAL_80.to_vec(), REAL_120.to_vec()),
        (hosting(), HOST_80.to_vec(), HOST_120.to_vec()),
    ];
    for (app, narrow, wide) in cases {
        assert_eq!(graph_rows(&app, 80, 40), narrow);
        assert_eq!(graph_rows(&app, 120, 40), wide);
    }
    crate::tui::test_support::assert_usage_md_shows(
        &STEREO_80.map(String::from),
        "level history mockup",
    );
}

/// The whole 80×40 frame of the stereo amp with its level history shown.
const STEREO_FRAME_80: [&str; 40] = [
    " svolder  Fake amp  ● connected                                        mouse:on ",
    "─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─",
    "┌ Controls ───────────────────────────────────────── mute ─ 0 centre ─ refresh ┐",
    "│                                                                              │",
    "│ ▶ Mute         [ ] —                                                         │",
    "│   Power meters [ ] —                                                         │",
    "│                                                                              │",
    "│   Meter L      ▓▓▓┃███████████  100%                                         │",
    "│                ▓▓▓┃███████████                                               │",
    "│   Meter R      ▓▓▓┃░░░░░░░░░░░    0%                                         │",
    "│                ▓▓▓┃░░░░░░░░░░░                                               │",
    "│                                                                              │",
    "│   L/R  ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿ │",
    "│        ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉ │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "│                                                                              │",
    "└──────────────────────────────────────────────────────────────────────────────┘",
    "                                                                                ",
    "┌ Status ────────────────────────────────────────────────────────────── advice ┐",
    "│                                                                              │",
    "│ Faults: none                                                                 │",
    "│ Raw: none                                                                    │",
    "│ Link                                                                     12 h│",
    "└──────────────────────────────────────────────────────────────────────────────┘",
];

#[test]
fn the_stereo_amp_frame_at_80x40_is_pinned() {
    assert_eq!(
        text(&frame(&metering_fake("fake-amp", 2), 80, 40)),
        STEREO_FRAME_80
    );
}

/// The graphs sit after the meters, and the host's after the Host group, whose Spectrum row
/// is its last.
#[test]
fn the_level_history_comes_after_its_group() {
    let amp = text(&frame(&metering_fake("fake-amp", 2), 80, 40));
    let meter_r = amp
        .iter()
        .position(|row| row.contains("Meter R"))
        .expect("Meter R");
    let at = amp.iter().position(|row| row.contains("L/R")).expect("L/R");
    assert_eq!(at, meter_r + 3, "a blank row after Meter R's two rows");
    let host = text(&frame(&hosting(), 80, 40));
    let group = host
        .iter()
        .position(|row| row.contains("Transport"))
        .expect("Transport");
    let at = host
        .iter()
        .position(|row| row.contains("host "))
        .expect("host graph");
    assert!(at > group, "{host:#?}");
}

/// A shared scale keeps a mirrored pair honest: a right channel that holds a quarter of the
/// left's peak draws low, where its own scale would stretch it.
#[test]
fn unscaled_meters_share_one_scale() {
    let rows = graph_rows(&metering_real(), 80, 40);
    let cells: Vec<char> = rows[1]
        .chars()
        .filter(|c| ('\u{2800}'..='\u{28ff}').contains(c))
        .collect();
    // The right channel holds a constant level, which on the shared scale is a flat two dots
    // down from the midline; scaled by itself it would draw one dot.
    assert!(
        !cells.is_empty() && cells.iter().all(|c| *c == '\u{281b}'),
        "{}",
        rows[1]
    );
}

/// A log-scaled meter graphs on its own fixed scale: a window that sat a hair above the zero for
/// the whole span draws in the bottom of the graph, where the self-scaling an unscaled meter
/// gets would stretch the same readings to the full height (spec 6.4).
#[test]
fn log_scaled_meters_graph_on_their_own_fixed_scale() {
    let quiet = |scale: bool| {
        let profile = profile_named("fake-mono");
        let profile = if scale {
            with_log_scale(profile, AEGIR_SCALE)
        } else {
            without_scale(profile)
        };
        // Reads 161 for the first half of the span and 162 for the second, just above the Aegir
        // scale's zero of 160.
        metering(profile, 1, Some(1.0), |_, step| (161 + step / 600) as f64)
    };
    let upper_dots = |app: &App| -> Vec<u32> {
        graph_rows(app, 80, 40)
            .iter()
            .flat_map(|row| row.chars())
            .filter(|c| ('\u{2800}'..='\u{28ff}').contains(c))
            .map(|c| (u32::from(c) - 0x2800) & 0x1b)
            .collect()
    };
    let scaled = upper_dots(&quiet(true));
    assert!(!scaled.is_empty(), "the graph drew no cells");
    assert!(
        scaled.iter().all(|dots| *dots == 0),
        "a quiet log-scaled meter rose above the bottom quarter: {scaled:?}"
    );
    // The same readings on a meter with no log scale stretch to the graph's full height.
    assert!(upper_dots(&quiet(false)).iter().any(|dots| *dots != 0));
}

/// Graphs off removes the rows, and the 24-row tiers never had room for them.
#[test]
fn level_history_is_absent_without_graphs_and_below_the_tall_tier() {
    for app in [metering_fake("fake-amp", 2), metering_real(), hosting()] {
        for (width, height) in [(80, 24), (120, 24), (119, 39)] {
            assert!(
                graph_rows(&app, width, height).is_empty(),
                "{width}x{height}"
            );
        }
        let mut off = app;
        off.tui.graphs = false;
        assert!(graph_rows(&off, 80, 40).is_empty());
        assert!(graph_rows(&off, 120, 40).is_empty());
    }
}

/// A 250 ms peak among quiet readings draws its top dots at the 60-minute span, in one
/// column of the graph.
#[test]
fn a_meter_peak_shows_at_the_sixty_minute_span() {
    let hour = Duration::from_secs(3600);
    let build = |spike: bool| {
        let base = Instant::now();
        let mut app = app_on(profile_named("fake-mono"));
        app.tui.graph_span = svolder_ipc::GraphSpan::M60;
        for second in 0..3600u64 {
            let raw = if spike && second == 1234 { 1023 } else { 100 };
            let value = ValueObject {
                value_type: "u16".into(),
                ..vo("20.1", Some("meter_1"), json!(raw), &raw.to_string())
            };
            app.apply(Update::Status(value), base + Duration::from_secs(second));
        }
        app.now = base + hour;
        app
    };
    let tops = |app: &App| {
        graph_rows(app, 120, 40)[0]
            .chars()
            .filter(|c| ('\u{2800}'..='\u{28ff}').contains(c))
            .filter(|c| (u32::from(*c) - 0x2800) & 0x09 != 0)
            .count()
    };
    assert_eq!(tops(&build(false)), 0);
    assert_eq!(tops(&build(true)), 1);
}

/// The header, with and without the facts the Status pane gained, is the one main drew before
/// them: the rows are the literals recorded then, for the DAC mockup and for the fake amp.
#[test]
fn header_is_unchanged() {
    let facts = crate::backend::DeviceRow {
        name: "gungnir".into(),
        product: Some("Bifrost 3".into()),
        profile: Some("bifrost3".into()),
        product_id: Some(10),
        ..crate::backend::DeviceRow::default()
    };
    let mut dac = mockup_app();
    dac.session.facts = Some(facts.clone());
    assert_rows_start(
        &text(&screen(&dac)),
        &[
            " svolder  Gungnir 2 · fw 3 · 0102…08  ● connected                 mouse:on 14:02",
            "─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─",
        ],
    );
    let mut amp = metering_fake("fake-amp", 2);
    amp.session.facts = Some(facts);
    assert_rows_start(
        &text(&frame(&amp, 80, 40)),
        &[
            " svolder  Fake amp  ● connected                                        mouse:on ",
            "─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─",
        ],
    );
}

/// `rows` begin with `expected`.
fn assert_rows_start(rows: &[String], expected: &[&str]) {
    assert_eq!(&rows[..expected.len()], expected);
}
