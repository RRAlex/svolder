// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The path the screen's Path row shows for a DAC at 44.1 kHz: the verdict, then three hops.
fn stages_44() -> Vec<Stage> {
    let stage = |join, words: &str, role| Stage {
        chip: false,
        join,
        words: words.to_string(),
        role,
    };
    vec![
        stage("", "untouched", Role::Good),
        stage(PATH_GAP, "player 44.1 kHz F32LE, volume 100 %", Role::Dim),
        stage(HOP_JOIN, "computer 44.1 kHz S32LE, volume 100 %", Role::Dim),
        stage(HOP_JOIN, "DAC USB, 44.1 kHz", Role::Dim),
    ]
}

/// Each line of a wrap as the text it shows.
fn shown(lines: Vec<Vec<(String, Role)>>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| line.into_iter().map(|(text, _)| text).collect())
        .collect()
}

/// Where the wrap breaks at several widths: only at the separators, the separator first on a
/// continuation line, and nothing before the first line's own words.
#[test]
fn the_wrap_breaks_at_the_separators_at_each_width() {
    let stages = stages_44();
    let full = full_text(&stages);
    assert_eq!(cells(&full), 107);
    assert_eq!(shown(wrap(&stages, 107, 3).unwrap()), [full.as_str()]);
    assert_eq!(
        shown(wrap(&stages, 90, 3).unwrap()),
        [
            "untouched · player 44.1 kHz F32LE, volume 100 % → computer 44.1 kHz S32LE, volume 100 %",
            "→ DAC USB, 44.1 kHz",
        ]
    );
    assert_eq!(
        shown(wrap(&stages, 60, 3).unwrap()),
        [
            "untouched · player 44.1 kHz F32LE, volume 100 %",
            "→ computer 44.1 kHz S32LE, volume 100 % → DAC USB, 44.1 kHz",
        ]
    );
    // One cell less and the DAC hop goes to a line of its own.
    assert_eq!(
        shown(wrap(&stages, 58, 3).unwrap()),
        [
            "untouched · player 44.1 kHz F32LE, volume 100 %",
            "→ computer 44.1 kHz S32LE, volume 100 %",
            "→ DAC USB, 44.1 kHz",
        ]
    );
}

/// A stage is never broken: a room narrower than a stage (with its separator) wraps nothing, and
/// every line of a wrap is a whole number of stages.
#[test]
fn a_stage_is_never_broken() {
    let stages = stages_44();
    // The widest stage on a continuation line is `→ computer 44.1 kHz S32LE, volume 100 %`.
    let widest = cells("→ computer 44.1 kHz S32LE, volume 100 %");
    assert_eq!(wrap(&stages, widest - 1, usize::MAX), None);
    assert!(wrap(&stages, widest, usize::MAX).is_some());
    for room in widest..110 {
        for line in wrap(&stages, room, usize::MAX).unwrap() {
            let text: String = line.iter().map(|(text, _)| text.as_str()).collect();
            assert!(cells(&text) <= room, "{text:?} is wider than {room}");
            for (part, _) in &line {
                let whole = stages.iter().any(|stage| {
                    part == &format!("{}{}", stage.join, stage.words)
                        || part == &format!("{}{}", stage.join.trim_start(), stage.words)
                });
                assert!(whole, "{part:?} is not a whole stage at {room}");
            }
        }
    }
}

/// The wrap takes three lines at most; a path that needs a fourth is not wrapped.
#[test]
fn the_wrap_takes_three_lines_at_most() {
    let stages = stages_44();
    // 40 cells: the verdict, the player, the computer and the DAC each take a line.
    assert_eq!(wrap(&stages, 40, 3), None);
    assert_eq!(wrap(&stages, 40, 4).map(|lines| lines.len()), Some(4));
    assert_eq!(wrap(&stages, 58, 3).map(|lines| lines.len()), Some(3));
    assert_eq!(wrap(&stages, 60, 3).map(|lines| lines.len()), Some(2));
}

/// The colours stay with their stages through a wrap.
#[test]
fn the_wrap_keeps_each_stage_in_its_colour() {
    let mut stages = stages_44();
    stages[2].role = Role::Warn;
    let lines = wrap(&stages, 58, 3).unwrap();
    assert_eq!(lines[0][0].1, Role::Good);
    assert_eq!(lines[0][1].1, Role::Dim);
    assert_eq!(lines[1][0].1, Role::Warn);
}

/// The window of a scrolled line is the cells from the offset on, each stage in its colour.
#[test]
fn the_window_shows_the_cells_from_the_offset() {
    let stages = stages_44();
    let theme = Theme::new(false);
    let at = |offset, room| -> String {
        window(&stages, &theme, offset, room)
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    // A line that goes on to the right ends in `…`, one that went on to the left starts with it.
    assert_eq!(at(0, 20), "untouched · player …");
    assert_eq!(at(12, 20), "…layer 44.1 kHz F32…");
    let tail = full_width(&stages) - 20;
    assert_eq!(at(tail, 20), "…→ DAC USB, 44.1 kHz");
    let colours: Vec<_> = window(&stages, &theme, 0, 14)
        .iter()
        .map(|span| span.style)
        .collect();
    assert_eq!(
        colours,
        [
            theme.style(Role::Good),
            theme.style(Role::Dim),
            theme.style(Role::Dim)
        ]
    );
}

/// The tooltip holds the whole path in lines of at most the tooltip's width, broken at the
/// separators, and a stage wider than a line is broken at its blanks.
#[test]
fn the_tip_holds_the_whole_path_in_short_lines() {
    let stages = stages_44();
    let lines = tip_lines(&stages, 40);
    assert_eq!(
        lines,
        [
            "untouched",
            "· player 44.1 kHz F32LE, volume 100 %",
            "→ computer 44.1 kHz S32LE, volume 100 %",
            "→ DAC USB, 44.1 kHz",
        ]
    );
    assert!(lines.iter().all(|line| cells(line) <= 40), "{lines:?}");
    // At 30 the stages are wider than a line: the blanks take the break.
    let narrow = tip_lines(&stages, 30);
    assert!(narrow.iter().all(|line| cells(line) <= 30), "{narrow:?}");
    let words = |lines: &[String]| lines.join(" ").replace("· ", "").replace("→ ", "");
    assert_eq!(
        words(&narrow).split_whitespace().collect::<Vec<_>>(),
        words(&lines).split_whitespace().collect::<Vec<_>>()
    );
}
