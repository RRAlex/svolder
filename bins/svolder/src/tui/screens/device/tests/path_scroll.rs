// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Path row where it is wider than its room: the wrap at the separators of its two lines, the
//! slow scroll where there is no room to wrap, and the hit that gives it its words in a tooltip.

use super::host_group::{daemon_hosting, gungnir_route};
use super::*;
use crate::tui::widgets::slow_scroll::SlowScroll;

/// The hops line the mockup's DAC route reads.
const HOPS: &str = "player → computer → DAC USB 1";
/// Its values line: the rate and what the DAC does.
const VALUES: &str = "48 kHz · volume -6.0 dB · tone control · NOS filter";

/// The app of the screen's host mockup on a DAC route.
fn routed() -> App {
    daemon_hosting(gungnir_route(|_| {}))
}

/// One frame of `app` at `width` by `height`: its rows and what it put where.
fn drawn(app: &App, width: u16, height: u16) -> (Vec<String>, HitMap) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let theme = Theme::new(false);
    terminal
        .draw(|f| {
            render::draw(f, app, &theme, &mut hits);
        })
        .expect("the frame draws");
    (text(terminal.backend().buffer()), hits)
}

/// The Path row's lines of a frame: the row with its label and the lines below it that carry on
/// without one, each as the words after the Controls pane's left border and cursor column, up to
/// the pane's right border, trimmed.
fn path_lines(rows: &[String]) -> Vec<String> {
    let at = rows
        .iter()
        .position(|row| row.contains(" Path "))
        .expect("a Path row");
    let mut lines = vec![rows[at].clone()];
    for row in &rows[at + 1..] {
        let pane = row.split('│').nth(1).unwrap_or("");
        let lead = pane.chars().take_while(|c| *c == ' ').count();
        if lead > 10 && !pane.trim().is_empty() {
            lines.push(row.clone());
        } else {
            break;
        }
    }
    lines
}

/// `text` without the separators: the `·` between values and the `→` between hops (a line that
/// wraps ends with its arrow), which are the same words either way.
fn norm(text: &str) -> String {
    text.replace(" · ", " ").replace(" → ", " ")
}

/// The words of a Path row line: what follows its label, or, on a continuation, its first word,
/// up to the pane's right border.
fn words_of(line: &str, first: bool) -> String {
    let pane = line.split('│').nth(1).unwrap_or("");
    let pane = pane.trim_end();
    match first {
        true => pane.split_once("Path").unwrap().1.trim().to_string(),
        false => pane.trim().to_string(),
    }
}

/// The lines of a Path row's frame split into its hops lines and its values lines: the values
/// start at the first line that starts with the rate.
fn split_lines(lines: &[String]) -> (Vec<String>, Vec<String>) {
    let words: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(at, line)| words_of(line, at == 0))
        .collect();
    let at = words
        .iter()
        .position(|line| line.starts_with("48 kHz"))
        .expect("a values line");
    (words[..at].to_vec(), words[at..].to_vec())
}

/// A pane with spare rows wraps the values line, too long for its row, at the dots onto a line
/// under the value column, the hops line staying whole; the rows below move down.
#[test]
fn a_long_values_line_wraps_at_the_dots_under_the_value_column() {
    let app = routed();
    let (rows, hits) = drawn(&app, 120, 30);
    let lines = path_lines(&rows);
    assert_eq!(lines.len(), 3, "{lines:#?}");
    let words: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(at, line)| words_of(line, at == 0))
        .collect();
    assert_eq!(
        words,
        [HOPS, "48 kHz · volume -6.0 dB · tone control", "NOS filter"]
    );
    // The continuations start in the column the first line's words do.
    let column = |line: &str, word: &str| line.find(word).unwrap();
    let first = column(&lines[0], "player");
    assert_eq!(column(&lines[1], "48 kHz"), first);
    assert_eq!(column(&lines[2], "NOS filter"), first);
    // The row below it (the Spectrum) follows the continuation lines; the pane counts the Path row
    // at its most lines, so the blank line a gap would put before the Spectrum is not drawn.
    let at = rows.iter().position(|row| row.contains(" Path ")).unwrap();
    assert!(rows[at + 3].contains("Spectrum"), "{rows:#?}");
    // Every line of the wrap is the Path row's, for the tooltip.
    for line in 0..3u16 {
        let y = u16::try_from(at).unwrap() + line;
        let x = u16::try_from(first).unwrap();
        assert_eq!(hits.hit_at(x, y), Some(&Hit::Path), "line {line}");
    }
    assert_ne!(
        hits.hit_at(5, u16::try_from(at).unwrap() + 3),
        Some(&Hit::Path)
    );
    assert_eq!(app.drawn.path_travel.get(), None, "a wrapped path scrolls");
}

/// Across widths the wrap breaks only between stages (or between the words of a stage too wide
/// for a line), takes five lines at most and never starts a line with `·` or `→`: whatever lines
/// show, put back together they are the two lines of the row.
#[test]
fn no_width_takes_a_sixth_line_or_starts_with_a_separator() {
    let app = routed();
    let mut wrapped = 0;
    for width in 100..=260 {
        let (rows, _) = drawn(&app, width, 30);
        let lines = path_lines(&rows);
        assert!(lines.len() <= 5, "{width}: {lines:#?}");
        let (hops, values) = split_lines(&lines);
        if lines.len() > 2 {
            wrapped += 1;
        }
        for line in hops.iter().chain(&values) {
            assert!(!line.starts_with('·'), "{width}: starts with `·`: {line:?}");
        }
        for line in hops.iter().chain(&values) {
            assert!(!line.starts_with('→'), "{width}: starts with `→`: {line:?}");
        }
        assert_eq!(norm(&hops.join(" ")), norm(HOPS), "{width}");
        assert_eq!(norm(&values.join(" ")), norm(VALUES), "{width}");
    }
    assert!(wrapped > 0, "no width wrapped");
}

/// A pane short of rows gives up the blank lines between the groups first, so the row still
/// wraps; with no room for the wrap even then the lines are cut and scroll.
#[test]
fn a_pane_without_room_cuts_the_line_and_scrolls_it() {
    let app = routed();
    let theme = Theme::new(false);
    let tier = crate::tui::layout_tier::Tier::default();
    let pane =
        |height: usize| super::super::controls::controls_pane(&app, &theme, 60, tier, height);
    let tall = pane(40);
    let path = tall.path.expect("a path row");
    assert!(
        path.wrapped && path.base == 2 && path.height == 3,
        "{path:?}"
    );
    // The gaps are drawn from spare rows only: the layouts are measured without them.
    let lines = |spaced, wrap| {
        super::super::controls::controls_lines(&app, &theme, 60, tier, spaced, wrap, &[])
            .lines
            .len()
    };
    let spaced = lines(true, true);
    assert_eq!(tall.lines.len(), spaced + 1, "the Path gap");
    // One line short of the spaced wrap: the blanks give way and the wrap stays.
    let packed = pane(spaced - 1);
    assert!(packed.path.unwrap().wrapped);
    assert!(packed.lines.len() < spaced);
    // Short of the packed wrap: the lines are cut and scroll, the blanks gone too.
    let cut = pane(lines(false, true) - 1);
    let path = cut.path.unwrap();
    assert!(!path.wrapped && path.height == 2, "{path:?}");
    assert!(path.travel.is_some_and(|travel| travel > 0), "{path:?}");
    assert_eq!(
        cut.lines.len(),
        lines(false, true) - 1,
        "the wrap's extra line came back, and the packed layout has no gap"
    );
}

/// At 80 columns the values wrap and the hops line, cut, is the one that scrolls: the pane
/// records how far it runs past its room and shows it from the scroll's offset; a row that
/// fits, or wraps whole, records nothing.
#[test]
fn a_cut_line_shows_the_window_from_the_offset() {
    let mut app = routed();
    let (rows, _) = drawn(&app, 80, 30);
    let travel = app.drawn.path_travel.get().expect("a cut path scrolls");
    let lines = path_lines(&rows);
    assert!(words_of(&lines[0], true).starts_with("player → computer"));
    assert_eq!(words_of(&lines[1], false), "48 kHz");
    assert!(travel > 0, "{travel}");
    app.host.path_scroll.offset = 3;
    let (rows, _) = drawn(&app, 80, 30);
    let lines = path_lines(&rows);
    assert!(words_of(&lines[0], true).starts_with('…'), "{lines:?}");
    // The end of the hops line shows at the last offset; the values stay where they were.
    app.host.path_scroll.offset = u16::try_from(travel).unwrap();
    let (rows, _) = drawn(&app, 80, 30);
    let lines = path_lines(&rows);
    assert!(
        words_of(&lines[0], true).ends_with("DAC USB 1"),
        "{lines:?}"
    );
    assert_eq!(words_of(&lines[1], false), "48 kHz");
    // Wide enough for the lines, or for the wrap: nothing to scroll.
    for width in [120, 300] {
        let (_, _) = drawn(&app, width, 30);
        assert_eq!(app.drawn.path_travel.get(), None, "{width}");
    }
}

/// With `scroll_long_lines` off a cut line ends in `…` and stays still, and a wrap is still
/// taken.
#[test]
fn with_scrolling_off_the_line_is_cut_with_an_ellipsis() {
    let mut app = routed();
    app.tui.scroll_long_lines = false;
    app.host.path_scroll.offset = 12;
    let (rows, _) = drawn(&app, 80, 30);
    let words = words_of(&path_lines(&rows)[0], true);
    assert!(
        words.starts_with("player → computer") && words.ends_with('…'),
        "{words:?}"
    );
    assert_eq!(app.drawn.path_travel.get(), None);
    let (rows, _) = drawn(&app, 120, 30);
    assert_eq!(path_lines(&rows).len(), 3);
}

/// The scroll over a paused clock: 2 s on the start, a cell every 250 ms, 2 s on the end, back
/// to the start; no sleeping anywhere.
#[test]
fn the_tick_scrolls_a_cut_line_over_a_paused_clock() {
    let mut app = routed();
    drawn(&app, 80, 30);
    let travel = app.drawn.path_travel.get().unwrap();
    let t0 = Instant::now();
    app.host.path_scroll = SlowScroll::new(t0);
    let at = |app: &mut App, ms: u64| {
        app.tick(t0 + Duration::from_millis(ms));
        app.host.path_scroll.offset
    };
    assert_eq!(at(&mut app, 1_900), 0);
    assert_eq!(at(&mut app, 2_250), 1);
    assert_eq!(at(&mut app, 4_000), 8);
    let end = 2_000 + 250 * u64::try_from(travel).unwrap();
    assert_eq!(usize::from(at(&mut app, end + 1_000)), travel);
    assert_eq!(at(&mut app, end + 2_000), 0, "it jumps back after the hold");
}

/// The scroll stands still while the pointer rests on the row and while the keys are on the
/// transport row above it, and goes on from where it stood.
#[test]
fn the_scroll_stands_still_under_the_pointer_and_the_keys() {
    let mut app = routed();
    drawn(&app, 80, 30);
    let t0 = Instant::now();
    app.host.path_scroll = SlowScroll::new(t0);
    let ms = |ms| t0 + Duration::from_millis(ms);
    app.tick(ms(3_000));
    assert_eq!(app.host.path_scroll.offset, 4);
    app.pointer.over_path = true;
    app.tick(ms(9_000));
    assert_eq!(app.host.path_scroll.offset, 4, "moved under the pointer");
    app.pointer.over_path = false;
    app.set_selected(app.controls_len() - 1);
    assert_eq!(app.host_row(), Some(crate::tui::host::HostRow::Transport));
    app.tick(ms(20_000));
    assert_eq!(app.host.path_scroll.offset, 4, "moved under the keys");
    // The Controls cursor left on Transport while the keys are in the Tone pane: it scrolls.
    app.pane = Pane::Tone;
    app.tick(ms(20_000));
    app.tick(ms(20_250));
    assert_eq!(
        app.host.path_scroll.offset, 5,
        "stood still under the Tone pane's keys"
    );
    app.pane = Pane::Controls;
    app.tick(ms(20_500));
    assert_eq!(app.host.path_scroll.offset, 5);
    app.set_selected(0);
    app.tick(ms(20_750));
    assert_eq!(app.host.path_scroll.offset, 6, "did not go on");
}

/// A path that is not cut, or not in sight, scrolls nothing and sits at its start: a frame that
/// wraps it, one that fits it, and a screen that is not the device's.
#[test]
fn a_path_not_cut_or_not_in_sight_does_not_scroll() {
    let mut app = routed();
    let t0 = Instant::now();
    for (width, height) in [(120, 30), (300, 30)] {
        drawn(&app, width, height);
        app.host.path_scroll = SlowScroll::new(t0);
        app.host.path_scroll.offset = 7;
        app.tick(t0 + Duration::from_secs(30));
        assert_eq!(app.host.path_scroll.offset, 0, "{width}x{height}");
    }
    drawn(&app, 80, 30);
    assert!(app.drawn.path_travel.get().is_some());
    // The next frame's start forgets what the last found: another screen draws no path.
    app.drawn.begin_frame();
    app.host.path_scroll.offset = 7;
    app.tick(t0 + Duration::from_secs(60));
    assert_eq!(app.host.path_scroll.offset, 0);
}

/// The Path row is a target of its own on every line it takes: a tooltip, and a click that opens
/// the config box.
#[test]
fn the_path_row_is_a_target_for_the_tooltip() {
    let app = routed();
    let (rows, hits) = drawn(&app, 150, 30);
    let at = u16::try_from(rows.iter().position(|row| row.contains(" Path ")).unwrap()).unwrap();
    assert_eq!(hits.hit_at(20, at), Some(&Hit::Path));
    assert_ne!(
        hits.hit_at(20, at - 1),
        Some(&Hit::Path),
        "the Transport row"
    );
}

/// The tooltip on the Path row says the label's words and both lines of the row, however far the
/// row is cut, in lines the tooltip's width holds and broken only at the separators.
#[test]
fn the_tooltip_says_the_whole_path() {
    let mut app = routed();
    app.mouse = true;
    let (rows, hits) = drawn(&app, 80, 30);
    let at = u16::try_from(rows.iter().position(|row| row.contains(" Path ")).unwrap()).unwrap();
    app.pointer.tooltip = Some(crate::tui::boxes::tooltip::Tooltip {
        hit: Hit::Path,
        at: (20, at),
        since: Instant::now(),
        shown: true,
    });
    let lines = crate::tui::boxes::tooltip::standing(&app, &hits).expect("a tooltip");
    assert_eq!(
        lines,
        [
            "Path: the DAC changes the sound",
            "player → computer → DAC USB 1",
            "48 kHz · volume -6.0 dB · tone control",
            "NOS filter",
        ]
    );
    assert!(lines.iter().all(|line| widgets::text::cells(line) <= 40));
}

/// At 120x45 with the default 50/50 split the Controls pane leaves the Path words 41 cells: the
/// values wrap onto a second line and the whole row shows, with no `…`.
#[test]
fn the_whole_path_shows_at_120x45_with_the_default_split() {
    let app = routed();
    let (rows, _) = drawn(&app, 120, 45);
    let lines = path_lines(&rows);
    assert_eq!(lines.len(), 3, "{lines:#?}");
    let (hops, values) = split_lines(&lines);
    assert_eq!(hops.join(" "), HOPS);
    assert_eq!(norm(&values.join(" ")), norm(VALUES));
    assert!(lines.iter().all(|line| !line.contains('…')), "{lines:#?}");
    assert_eq!(app.drawn.path_travel.get(), None);
}

/// The mockup's app on a daemon that speaks for the host, its route `route` (`None`: not read
/// yet) and something playing where `playing`.
fn hosting(route: Option<svolder_host::RouteFacts>, playing: bool) -> App {
    let mut app = routed();
    app.host.fold_daemon(
        svolder_ipc::host::HostView {
            volume: None,
            eq: None,
            available: true,
            playing: playing.then(|| svolder_ipc::host::PlayingView {
                title: "So What".into(),
                artist: "Miles Davis".into(),
                player: "mpv".into(),
                status: "playing".into(),
                ..Default::default()
            }),
            route,
            now_ms: 0,
        },
        app.now,
    );
    app
}

/// The screen row each of the Controls pane's rows above the Path row is drawn on.
fn rows_above_the_path(app: &App, width: u16, height: u16) -> Vec<(&'static str, usize)> {
    let (rows, _) = drawn(app, width, height);
    ["Input", "Volume", "Balance", "Transport", "Path"]
        .into_iter()
        .map(|label| {
            let at = rows
                .iter()
                .position(|row| row.contains(&format!(" {label} ")))
                .unwrap_or_else(|| panic!("no {label} row: {rows:#?}"));
            (label, at)
        })
        .collect()
}

/// A pointer aimed at a row of the Controls pane must find it there whatever the Path row has
/// to say: the rows above the Path row keep their screen rows while the route is unread, read,
/// read with more to say (a values line, a wrap) or gone with nothing playing. The pane lays
/// itself out for the most lines the Path row can take, the rest is blank space at its end.
#[test]
fn the_rows_above_the_path_row_keep_their_screen_rows_whatever_it_says() {
    let longer = gungnir_route(|route| {
        route.host_eq = true;
        route.sink.as_mut().expect("a sink").muted = true;
    });
    for (width, height) in [(80, 24), (100, 28), (120, 30), (120, 40)] {
        let unread = rows_above_the_path(&hosting(None, true), width, height);
        let said = [
            ("read", hosting(Some(gungnir_route(|_| {})), true)),
            ("longer", hosting(Some(longer.clone()), true)),
            (
                "nothing playing",
                hosting(Some(gungnir_route(|_| {})), false),
            ),
        ];
        for (what, app) in said {
            assert_eq!(
                rows_above_the_path(&app, width, height),
                unread,
                "{what} at {width}x{height}"
            );
        }
    }
}

/// What one reading's pane lays out: its name, its row, host, switch and Path tops, and how it was
/// laid (the blanks kept, the path wrapped).
type Layout = (
    String,
    (Vec<u16>, Vec<u16>, Option<u16>, Option<u16>),
    (bool, bool),
);

/// The Controls pane at `height` lines in `tier` with `room` columns, for each reading the Path
/// row can show: the rows, host rows, switch and Path row's top line of each, and how it was laid
/// (the blanks kept, the path wrapped).
fn panes_per_reading(
    tier: crate::tui::layout_tier::Tier,
    room: usize,
    height: usize,
    capturing: bool,
) -> Vec<Layout> {
    let longer = gungnir_route(|route| {
        route.host_eq = true;
        route.sink.as_mut().expect("a sink").muted = true;
    });
    let readings = [
        ("unread", hosting(None, true)),
        ("read", hosting(Some(gungnir_route(|_| {})), true)),
        ("longer", hosting(Some(longer), true)),
        (
            "nothing playing",
            hosting(Some(gungnir_route(|_| {})), false),
        ),
    ];
    readings
        .into_iter()
        .map(|(what, mut app)| {
            app.host.capturing = capturing;
            let pane =
                super::super::controls::controls_pane(&app, &Theme::new(false), room, tier, height);
            let rows = pane.rows.iter().map(|(_, top, _)| *top).collect();
            let laid = (pane.blanks > 0, pane.path.is_some_and(|path| path.wrapped));
            (
                what.to_string(),
                (
                    rows,
                    pane.host.clone(),
                    pane.switch,
                    pane.path.map(|path| path.top),
                ),
                laid,
            )
        })
        .collect()
}

/// At every tier and every pane height the rows above the Path row stay on their lines whatever
/// the Path row says, and the sweep reaches both ways the path is cut (the groups spaced, and
/// packed) as well as the wrap, so no arm of the layout choice goes unproved. Where the tier
/// draws no graph (a graph below the Path row still changes what fits) the reading of the host
/// capture is varied too.
#[test]
fn the_pane_keeps_the_rows_above_the_path_row_at_every_tier_and_height() {
    let mut arms = std::collections::BTreeSet::new();
    for (wide, tall, room) in [
        (false, false, 30),
        (true, false, 30),
        (true, false, 60),
        (true, true, 60),
        (false, true, 30),
    ] {
        let tier = crate::tui::layout_tier::Tier { wide, tall };
        let captures: &[bool] = if tall { &[true] } else { &[true, false] };
        for height in 1..=40 {
            for &capturing in captures {
                let all = panes_per_reading(tier, room, height, capturing);
                let (_, first, _) = &all[0];
                for (what, rows, laid) in &all {
                    assert_eq!(
                        rows, first,
                        "{what}, wide {wide} tall {tall} room {room} height {height} capturing {capturing}"
                    );
                    arms.insert(*laid);
                }
            }
        }
    }
    for (spaced, wrapped) in [(true, true), (false, true), (true, false), (false, false)] {
        assert!(
            arms.contains(&(spaced, wrapped)),
            "no height laid it spaced {spaced}, wrapped {wrapped}: {arms:?}"
        );
    }
}
