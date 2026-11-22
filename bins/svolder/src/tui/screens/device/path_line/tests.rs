// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use svolder_ipc::host::{RowHop, RowLabel, RowWord};

/// A word of the row model.
fn word(text: &str, tone: Tone) -> RowWord {
    RowWord {
        text: text.to_string(),
        tone,
    }
}

/// A hop of the row model.
fn hop(label: &str, words: &[RowWord]) -> RowHop {
    RowHop {
        label: label.to_string(),
        words: words.to_vec(),
    }
}

/// A row with these hops and values, its label plain.
fn row(hops: Vec<RowHop>, values: Vec<RowWord>) -> PathRow {
    PathRow {
        label: RowLabel {
            tone: Tone::Plain,
            words: "Path".to_string(),
        },
        hops,
        values,
        words: None,
    }
}

/// The three hops of a computer path to a DAC, and a values line of three values.
fn long_row() -> PathRow {
    row(
        vec![
            hop("player", &[]),
            hop("computer", &[word("resampled", Tone::Warn)]),
            hop("DAC USB 1", &[]),
        ],
        vec![
            word("44.1 → 48 kHz", Tone::Dim),
            word("volume -24.1 dB", Tone::Warn),
            word("tone control", Tone::Warn),
        ],
    )
}

/// Each line of a wrap as the text it shows.
fn shown(lines: Vec<Vec<Piece>>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| line.into_iter().map(|(text, _)| text).collect())
        .collect()
}

/// The hops line and the values line are the model's words in its tones, the separators dim.
#[test]
fn the_stages_are_the_models_words_in_its_tones() {
    let lines = row_lines(&long_row());
    assert_eq!(
        full_text(&lines.hops),
        "player → computer resampled → DAC USB 1"
    );
    assert_eq!(
        full_text(&lines.values),
        "44.1 → 48 kHz · volume -24.1 dB · tone control"
    );
    let roles: Vec<_> = flat(&lines.hops)
        .into_iter()
        .map(|(_, role)| role)
        .collect();
    assert_eq!(
        roles,
        [
            Role::Text, // player
            Role::Dim,  // " → "
            Role::Text, // computer
            Role::Text, // " "
            Role::Warn, // resampled
            Role::Dim,  // " → "
            Role::Text, // DAC USB 1
        ]
    );
}

/// A row with no hops is its short words, plain, and has no values line.
#[test]
fn a_row_with_no_hops_is_its_short_words() {
    let mut elsewhere = row(Vec::new(), Vec::new());
    elsewhere.words = Some("→ DELL S2721QS".to_string());
    let lines = row_lines(&elsewhere);
    assert_eq!(full_text(&lines.hops), "→ DELL S2721QS");
    assert!(lines.values.is_empty());
    assert_eq!(lines.lines().count(), 1);
    let none = row_lines(&row(Vec::new(), Vec::new()));
    assert_eq!(none.lines().count(), 0);
}

/// Where the wrap breaks the hops line: only at the arrows, a hop followed by another ends its
/// line with ` →` (it needs two cells of room for it) and the next line starts with the next
/// hop's words, never with an arrow.
#[test]
fn the_hops_wrap_at_the_arrows_with_the_arrow_ending_the_line() {
    let lines = row_lines(&long_row());
    let full = full_text(&lines.hops);
    assert_eq!(cells(&full), 39);
    assert_eq!(shown(wrap(&lines.hops, 39, 5).unwrap()), [full.as_str()]);
    assert_eq!(
        shown(wrap(&lines.hops, 30, 5).unwrap()),
        ["player → computer resampled →", "DAC USB 1"]
    );
    // `player → computer resampled` is 27 cells: at 28 its arrow does not fit, so it waits
    assert_eq!(
        shown(wrap(&lines.hops, 28, 5).unwrap()),
        ["player →", "computer resampled →", "DAC USB 1"]
    );
    assert_eq!(
        shown(wrap(&lines.hops, 20, 5).unwrap()),
        ["player →", "computer resampled →", "DAC USB 1"]
    );
    assert_eq!(
        shown(wrap(&lines.hops, 17, 5).unwrap()),
        ["player →", "computer", "  resampled →", "DAC USB 1"]
    );
}

/// The values wrap at the dots and a continuation never starts with one.
#[test]
fn the_values_wrap_at_the_dots_and_never_start_with_one() {
    let lines = row_lines(&long_row());
    for room in 15..50 {
        for line in shown(wrap(&lines.values, room, usize::MAX).unwrap()) {
            assert!(!line.trim_start().starts_with('·'), "{line:?} at {room}");
            assert!(cells(&line) <= room, "{line:?} at {room}");
        }
    }
    assert_eq!(
        shown(wrap(&lines.values, 30, 5).unwrap()),
        ["44.1 → 48 kHz", "volume -24.1 dB · tone control"]
    );
}

/// No line of a wrap starts with `·` or `→`, at any width, and none is wider than its room.
#[test]
fn no_wrapped_line_starts_with_a_separator() {
    let lines = row_lines(&long_row());
    for room in 12..45 {
        for stages in [&lines.hops, &lines.values] {
            for line in shown(wrap(stages, room, usize::MAX).unwrap_or_default()) {
                assert!(cells(&line) <= room, "{line:?} at {room}");
                let line = line.trim_start();
                assert!(
                    !line.starts_with('·') && !line.starts_with('→'),
                    "{line:?} at {room}"
                );
            }
        }
    }
}

/// A hop wider than a line breaks between its words, the next line indented under them.
#[test]
fn a_hop_wider_than_a_line_breaks_between_its_words() {
    let wide = row(
        vec![hop(
            "computer",
            &[
                word("resampled", Tone::Warn),
                word("volume 80 %", Tone::Warn),
                word("channels remapped", Tone::Warn),
            ],
        )],
        Vec::new(),
    );
    let lines = row_lines(&wide);
    assert_eq!(
        shown(wrap(&lines.hops, 30, 5).unwrap()),
        ["computer resampled volume 80 %", "  channels remapped"]
    );
    // A word wider than the line breaks at its blanks; one that cannot is no wrap.
    assert!(wrap(&lines.hops, 16, 5).is_some());
    assert_eq!(wrap(&lines.hops, 8, 5), None);
}

/// The wrap takes the lines it is allowed at most; a line that needs more is not wrapped.
#[test]
fn the_wrap_takes_no_more_lines_than_it_is_allowed() {
    let lines = row_lines(&long_row());
    assert_eq!(wrap(&lines.hops, 20, 2), None);
    assert_eq!(wrap(&lines.hops, 20, 3).map(|l| l.len()), Some(3));
}

/// The colours stay with their words through a wrap, the dim separator included.
#[test]
fn the_wrap_keeps_each_word_in_its_colour() {
    let lines = row_lines(&long_row());
    let wrapped = wrap(&lines.hops, 20, 5).unwrap();
    assert_eq!(
        wrapped[0],
        [
            ("player".to_string(), Role::Text),
            (" →".to_string(), Role::Dim),
        ]
    );
    assert_eq!(
        wrapped[1],
        [
            ("computer".to_string(), Role::Text),
            (" ".to_string(), Role::Text),
            ("resampled".to_string(), Role::Warn),
            (" →".to_string(), Role::Dim),
        ]
    );
}

/// The window of a scrolled line is the cells from the offset on, each word in its colour.
#[test]
fn the_window_shows_the_cells_from_the_offset() {
    let lines = row_lines(&long_row());
    let theme = Theme::new(false);
    let at = |offset, room| -> String {
        window(&lines.hops, &theme, offset, room)
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    assert_eq!(at(0, 20), "player → computer r…");
    assert_eq!(at(7, 20), "… computer resample…");
    let tail = full_width(&lines.hops) - 20;
    assert_eq!(at(tail, 20), "…sampled → DAC USB 1");
    let spans = window(&lines.hops, &theme, 0, 9);
    assert_eq!(spans[0].style, theme.style(Role::Text));
    assert_eq!(spans[1].style, theme.style(Role::Dim));
}

/// A cut line ends in `…` where its words run out of room, and keeps its colours up to it.
#[test]
fn a_cut_line_ends_in_an_ellipsis() {
    let lines = row_lines(&long_row());
    let theme = Theme::new(false);
    let spans = cut(&lines.values, &theme, 20);
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "44.1 → 48 kHz · vol…");
    assert_eq!(cells(&text), 20);
    assert_eq!(spans.last().unwrap().style, theme.style(Role::Dim));
    let whole: String = cut(&lines.values, &theme, 60)
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(whole, full_text(&lines.values));
}

/// The tooltip holds each line of the row in lines of at most the tooltip's width.
#[test]
fn the_tip_holds_both_lines_in_short_lines() {
    let lines = row_lines(&long_row());
    assert_eq!(
        tip_lines(&lines, 40),
        [
            "player → computer resampled → DAC USB 1",
            "44.1 → 48 kHz · volume -24.1 dB",
            "tone control",
        ]
    );
    assert!(tip_lines(&lines, 22).iter().all(|line| cells(line) <= 22));
}

/// A value is never split at its blanks or its arrow: one wider than a line is no wrap, where a
/// hop's words would break.
#[test]
fn a_value_wider_than_a_line_is_not_split() {
    let lines = row_lines(&long_row());
    // `44.1 → 48 kHz` is 13 cells; at 5 a hop's words would split, a value does not.
    assert_eq!(wrap(&lines.values, 5, usize::MAX), None);
    let text = |room| {
        wrap(&lines.values, room, usize::MAX)
            .map(shown)
            .unwrap_or_default()
    };
    for room in 13..40 {
        assert!(
            !text(room).iter().any(|line| line.starts_with('→')),
            "{room}"
        );
    }
}

/// The config tag stays whole in a cut values line, after the `…` of what it cuts.
#[test]
fn a_cut_values_line_keeps_the_config_tag_after_the_ellipsis() {
    let mut row = long_row();
    row.values.push(word("config 1", Tone::Warn));
    let lines = row_lines(&row);
    let theme = Theme::new(false);
    let text = |room| -> String {
        cut_keeping_config(&lines.values, &theme, room)
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    assert_eq!(text(24), "44.1 → 48 kH… · config 1");
    assert_eq!(cells(&text(24)), 24);
}

/// A hop wider than a line keeps room for the arrow on its last line too: the hop's own lines
/// are at most `room` cells with the ` →` that follows its last word.
#[test]
fn a_wide_hop_followed_by_another_still_has_room_for_its_arrow() {
    let row = row(
        vec![
            hop(
                "computer",
                &[
                    word("resampled", Tone::Warn),
                    word("volume 80 %", Tone::Warn),
                ],
            ),
            hop("DAC USB 1", &[]),
        ],
        Vec::new(),
    );
    let lines = row_lines(&row);
    for room in 14..40 {
        let wrapped = shown(wrap(&lines.hops, room, usize::MAX).unwrap_or_default());
        assert!(
            wrapped.iter().all(|line| cells(line) <= room),
            "{wrapped:?} at {room}"
        );
        assert!(
            !wrapped.iter().any(|line| line.starts_with('→')),
            "{wrapped:?}"
        );
    }
    assert_eq!(
        shown(wrap(&lines.hops, 22, 5).unwrap()),
        ["computer resampled", "  volume 80 % →", "DAC USB 1"]
    );
}

/// The words of a row with no hops break between its stages (`a · b`), so a line never starts
/// with the dot, at any width.
#[test]
fn short_words_wrap_between_their_stages_never_at_a_dot() {
    let mut elsewhere = row(Vec::new(), Vec::new());
    elsewhere.words = Some("the DAC listens to USB 1 · the computer is on USB 2".to_string());
    let lines = row_lines(&elsewhere);
    assert_eq!(
        full_text(&lines.hops),
        "the DAC listens to USB 1 · the computer is on USB 2"
    );
    assert_eq!(
        shown(wrap(&lines.hops, 24, 5).unwrap()),
        ["the DAC listens to USB 1", "the computer is on USB 2"]
    );
    // the words are plain, the dot between the stages dim
    let roles: Vec<_> = flat(&lines.hops)
        .into_iter()
        .filter(|(text, _)| text.contains('·') || text == "the")
        .collect();
    assert_eq!(
        roles,
        [
            ("the".to_string(), Role::Text),
            (" · ".to_string(), Role::Dim),
            ("the".to_string(), Role::Text),
        ]
    );
    for room in 12..60 {
        for line in shown(wrap(&lines.hops, room, usize::MAX).unwrap_or_default()) {
            assert!(!line.trim_start().starts_with('·'), "{line:?} at {room}");
        }
    }
}

/// The tooltip's last resort, a hop with a word wider than the tooltip, never leaves its arrow
/// alone on a line: it ends the hop's last line.
#[test]
fn the_tip_never_leaves_a_lone_arrow() {
    let long = "x".repeat(39);
    let row = row(
        vec![
            hop("computer", &[word(&long, Tone::Warn)]),
            hop("DAC USB 1", &[]),
        ],
        Vec::new(),
    );
    let tip = tip_lines(&row_lines(&row), 40);
    assert!(tip.iter().all(|line| cells(line) <= 40), "{tip:?}");
    assert!(!tip.iter().any(|line| line.starts_with('→')), "{tip:?}");
    assert!(tip.iter().any(|line| line.ends_with(" →")), "{tip:?}");
    assert_eq!(tip.last().map(String::as_str), Some("DAC USB 1"));
}
