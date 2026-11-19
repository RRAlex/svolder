// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Path row where it is wider than its room: the wrap at the hop separators, the slow scroll
//! where there is no room to wrap, and the hit that gives it the whole path in a tooltip.

use super::host_group::{daemon_hosting, gungnir_route};
use super::*;
use crate::tui::widgets::slow_scroll::SlowScroll;

/// The whole path the mockup's DAC route reads, on one line.
const FULL: &str = "untouched · player 48 kHz S16LE, volume 100 % → computer 48 kHz S32LE, \
                    volume 100 % → DAC USB 1, 48 kHz, volume -6.0 dB, tone control, NOS filter";

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
        let inner = row.trim_start_matches('│');
        let lead = inner.chars().take_while(|c| *c == ' ').count();
        let after = inner.trim_start();
        if lead > 10 && (after.starts_with('→') || after.starts_with('·')) {
            lines.push(row.clone());
        } else {
            break;
        }
    }
    lines
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

/// A pane with spare rows wraps a path too long for its row at the hop separators onto lines
/// under the value column, the separator first, and the rows below move down.
#[test]
fn a_long_path_wraps_at_the_separators_under_the_value_column() {
    let app = routed();
    let (rows, hits) = drawn(&app, 200, 30);
    let lines = path_lines(&rows);
    assert_eq!(lines.len(), 3, "{lines:#?}");
    let words: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(at, line)| words_of(line, at == 0))
        .collect();
    assert_eq!(
        words,
        [
            "untouched · player 48 kHz S16LE, volume 100 %",
            "→ computer 48 kHz S32LE, volume 100 %",
            "→ DAC USB 1, 48 kHz, volume -6.0 dB, tone control, NOS filter",
        ]
    );
    // The continuations start in the column the first line's words do.
    let column = |line: &str, word: &str| {
        line.chars()
            .take_while(|_| true)
            .collect::<Vec<_>>()
            .windows(word.chars().count())
            .position(|w| w.iter().collect::<String>() == word)
            .unwrap()
    };
    let first = column(&lines[0], "untouched");
    assert_eq!(column(&lines[1], "→ computer"), first);
    assert_eq!(column(&lines[2], "→ DAC"), first);
    // The row below it (the Spectrum) is two lines further down than without the wrap.
    let at = rows.iter().position(|row| row.contains(" Path ")).unwrap();
    assert!(rows[at + 3].contains("Spectrum"), "{:?}", rows[at + 3]);
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

/// Across widths the wrap breaks only between stages and takes three lines at most: whatever
/// lines show, put back together they are the whole path, and a path that does not wrap is on
/// one line.
#[test]
fn no_width_breaks_a_stage_or_takes_a_fourth_line() {
    let app = routed();
    let mut wrapped = 0;
    for width in 100..=260 {
        let (rows, _) = drawn(&app, width, 30);
        let lines = path_lines(&rows);
        assert!(lines.len() <= 3, "{width}: {lines:#?}");
        if lines.len() == 1 {
            continue;
        }
        wrapped += 1;
        let mut text = String::new();
        for (at, line) in lines.iter().enumerate() {
            let words = words_of(line, at == 0);
            assert!(
                at == 0 || words.starts_with("→ ") || words.starts_with("· "),
                "{width}: a line that does not start with its separator: {words:?}"
            );
            if at > 0 {
                text.push(' ');
            }
            text.push_str(&words);
        }
        assert_eq!(text, FULL, "{width}: the lines are not the whole path");
    }
    assert!(wrapped > 0, "no width wrapped");
}

/// A pane short of rows gives up the blank lines between the groups first, so the path still
/// wraps; with no room for the wrap even then the line is cut and scrolls.
#[test]
fn a_pane_without_room_cuts_the_line_and_scrolls_it() {
    let app = routed();
    let theme = Theme::new(false);
    let tier = crate::tui::layout_tier::Tier::default();
    let pane =
        |height: usize| super::super::controls::controls_pane(&app, &theme, 94, tier, height);
    let tall = pane(40);
    let path = tall.path.expect("a path row");
    assert!(path.wrapped && path.height == 3, "{path:?}");
    let spaced = tall.lines.len();
    // One line short of the spaced wrap: the blanks give way and the wrap stays.
    let packed = pane(spaced - 1);
    assert!(packed.path.unwrap().wrapped);
    assert!(packed.lines.len() < spaced);
    // Short of the packed wrap: the path is cut and scrolls, the blanks gone too.
    let cut = pane(packed.lines.len() - 1);
    let path = cut.path.unwrap();
    assert!(!path.wrapped && path.height == 1, "{path:?}");
    assert!(path.travel.is_some_and(|travel| travel > 0), "{path:?}");
    assert_eq!(
        cut.lines.len(),
        packed.lines.len() - 2,
        "the wrap's two lines came back"
    );
}

/// A cut line that is in sight records how far it runs past its room, and shows the cells from
/// the scroll's offset; a line that fits, or wraps, records nothing.
#[test]
fn a_cut_line_shows_the_window_from_the_offset() {
    let mut app = routed();
    let (rows, _) = drawn(&app, 150, 30);
    let travel = app.drawn.path_travel.get().expect("a cut path scrolls");
    let line = &path_lines(&rows)[0];
    assert!(
        words_of(line, true).starts_with("untouched · player 48 kHz S16LE, volume 100 % → comp")
    );
    assert!(travel > 30, "{travel}");
    app.host.path_scroll.offset = 12;
    let (rows, _) = drawn(&app, 150, 30);
    assert!(
        words_of(&path_lines(&rows)[0], true)
            .starts_with("…layer 48 kHz S16LE, volume 100 % → computer"),
        "{:?}",
        path_lines(&rows)[0]
    );
    // The end of the line shows at the last offset.
    app.host.path_scroll.offset = u16::try_from(travel).unwrap();
    let (rows, _) = drawn(&app, 150, 30);
    assert!(words_of(&path_lines(&rows)[0], true).ends_with("tone control, NOS filter"));
    // Wide enough for one line, or for the wrap: nothing to scroll.
    for width in [200, 300] {
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
    let (rows, _) = drawn(&app, 150, 30);
    let words = words_of(&path_lines(&rows)[0], true);
    assert!(
        words.starts_with("untouched · player") && words.ends_with('…'),
        "{words:?}"
    );
    assert_eq!(app.drawn.path_travel.get(), None);
    let (rows, _) = drawn(&app, 200, 30);
    assert_eq!(path_lines(&rows).len(), 3);
}

/// The scroll over a paused clock: 2 s on the start, a cell every 250 ms, 2 s on the end, back
/// to the start; no sleeping anywhere.
#[test]
fn the_tick_scrolls_a_cut_line_over_a_paused_clock() {
    let mut app = routed();
    drawn(&app, 150, 30);
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
    drawn(&app, 150, 30);
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
    for (width, height) in [(200, 30), (300, 30)] {
        drawn(&app, width, height);
        app.host.path_scroll = SlowScroll::new(t0);
        app.host.path_scroll.offset = 7;
        app.tick(t0 + Duration::from_secs(30));
        assert_eq!(app.host.path_scroll.offset, 0, "{width}x{height}");
    }
    drawn(&app, 150, 30);
    assert!(app.drawn.path_travel.get().is_some());
    // The next frame's start forgets what the last found: another screen draws no path.
    app.drawn.begin_frame();
    app.host.path_scroll.offset = 7;
    app.tick(t0 + Duration::from_secs(60));
    assert_eq!(app.host.path_scroll.offset, 0);
}

/// The Path row is a target of its own on every line it takes, a tooltip target that no click
/// acts on.
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

/// The tooltip on the Path row says the whole path, however far the row is cut, in lines the
/// tooltip's width holds and broken only at the separators.
#[test]
fn the_tooltip_says_the_whole_path() {
    let mut app = routed();
    app.mouse = true;
    let (rows, hits) = drawn(&app, 100, 30);
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
            "untouched",
            "· player 48 kHz S16LE, volume 100 %",
            "→ computer 48 kHz S32LE, volume 100 %",
            "→ DAC USB 1, 48 kHz, volume -6.0 dB,",
            "tone control, NOS filter",
        ]
    );
    assert!(lines.iter().all(|line| widgets::text::cells(line) <= 40));
}
