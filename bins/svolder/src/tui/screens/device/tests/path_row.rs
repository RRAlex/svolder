// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Path row as the row model draws it: the four cases of the plan at 58 and at 120 columns,
//! each line's text and the colour of every run of cells, and the lines a narrow row wraps onto.

use svolder_ipc::host::{PathRow, RowHop, RowLabel, RowWord, Tone};

use super::*;
use crate::tui::screens::device::controls::path_row as drawn_row;
use ratatui::style::Style;

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

/// The row of one of the plan's cases: its label, the word on the computer hop, the values.
fn case(label: Tone, computer: &[RowWord], values: Vec<RowWord>) -> PathRow {
    PathRow {
        label: RowLabel {
            tone: label,
            words: "Path".to_string(),
        },
        hops: vec![
            hop("player", &[]),
            hop("computer", computer),
            hop("DAC USB 1", &[]),
        ],
        values,
        words: None,
    }
}

/// Untouched end to end.
fn untouched() -> PathRow {
    case(
        Tone::Good,
        &[],
        vec![word("48 kHz", Tone::Dim), word("volume 0 dB", Tone::Dim)],
    )
}

/// The DAC at -24.1 dB with its tone control on.
fn dac_changes() -> PathRow {
    case(
        Tone::Warn,
        &[],
        vec![
            word("48 kHz", Tone::Dim),
            word("volume -24.1 dB", Tone::Warn),
            word("tone control", Tone::Warn),
        ],
    )
}

/// Resampled at the computer.
fn resampled() -> PathRow {
    case(
        Tone::Warn,
        &[word("resampled", Tone::Warn)],
        vec![
            word("44.1 → 48 kHz", Tone::Dim),
            word("volume 0 dB", Tone::Dim),
        ],
    )
}

/// Untouched, with one failing config line.
fn config_fails() -> PathRow {
    case(
        Tone::Warn,
        &[],
        vec![
            word("48 kHz", Tone::Dim),
            word("volume 0 dB", Tone::Dim),
            word("config 1", Tone::Warn),
        ],
    )
}

/// The row's lines in `width` columns (the label column as the pane gives it), each as its text
/// with every run of cells in a colour marked: `{g:..}` green, `{y:..}` yellow, `{d:..}` dim,
/// plain bare.
fn marked(row: &PathRow, width: usize, wrap: bool) -> Vec<String> {
    let theme = Theme::new(false);
    let columns = widgets::rows::Columns::fit(width, 15, 0);
    drawn_row(row, &theme, columns, (wrap, None))
        .iter()
        .map(|line| {
            let mut out = String::new();
            let mut last: Option<char> = None;
            for span in &line.spans {
                let mark = if span.style == theme.style(Role::Good) {
                    Some('g')
                } else if span.style == theme.style(Role::Warn) {
                    Some('y')
                } else if span.style == theme.style(Role::Dim) {
                    Some('d')
                } else if span.style == theme.style(Role::Text) || span.style == Style::default() {
                    None
                } else {
                    panic!(
                        "{:?} is drawn in {:?}, no role of the row",
                        span.content, span.style
                    )
                };
                if mark != last {
                    if last.is_some() {
                        out.push('}');
                    }
                    if let Some(m) = mark {
                        out.push_str(&format!("{{{m}:"));
                    }
                    last = mark;
                }
                out.push_str(&span.content);
            }
            if last.is_some() {
                out.push('}');
            }
            out.trim_end().to_string()
        })
        .collect()
}

/// Lines as written here, `~` standing for the blanks that pad the label to its column and `@`
/// for the label column blank on a line below it.
fn expect(width: usize, lines: &[&str]) -> Vec<String> {
    let field = widgets::rows::Columns::fit(width, 15, 0).field();
    lines
        .iter()
        .map(|line| {
            line.replace('~', &" ".repeat(field - 4))
                .replace('@', &" ".repeat(field))
        })
        .collect()
}

#[test]
fn untouched_end_to_end_has_a_green_label_and_dim_values() {
    for width in [58, 120] {
        assert_eq!(
            marked(&untouched(), width, true),
            expect(
                width,
                &[
                    "{g:Path}~player{d: → }computer{d: → }DAC USB 1",
                    "@{d:48 kHz · volume 0 dB}"
                ]
            ),
            "{width}"
        );
    }
}

#[test]
fn the_dac_changes_make_the_label_and_those_values_yellow() {
    for width in [58, 120] {
        assert_eq!(
            marked(&dac_changes(), width, true),
            expect(
                width,
                &[
                    "{y:Path}~player{d: → }computer{d: → }DAC USB 1",
                    "@{d:48 kHz · }{y:volume -24.1 dB}{d: · }{y:tone control}"
                ]
            ),
            "{width}"
        );
    }
}

#[test]
fn a_resampling_computer_is_the_one_yellow_hop() {
    for width in [58, 120] {
        assert_eq!(
            marked(&resampled(), width, true),
            expect(
                width,
                &[
                    "{y:Path}~player{d: → }computer {y:resampled}{d: → }DAC USB 1",
                    "@{d:44.1 → 48 kHz · volume 0 dB}"
                ]
            ),
            "{width}"
        );
    }
}

#[test]
fn a_failing_config_check_is_the_last_value_in_yellow() {
    for width in [58, 120] {
        assert_eq!(
            marked(&config_fails(), width, true),
            expect(
                width,
                &[
                    "{y:Path}~player{d: → }computer{d: → }DAC USB 1",
                    "@{d:48 kHz · volume 0 dB · }{y:config 1}"
                ]
            ),
            "{width}"
        );
    }
}

/// What each line of a row carries after the label column, whatever width the row is drawn at.
fn bodies(row: &PathRow, width: usize, wrap: bool) -> Vec<String> {
    let field = widgets::rows::Columns::fit(width, 15, 0).field();
    marked(row, width, wrap)
        .iter()
        .map(|line| {
            let plain: String = line
                .chars()
                .filter(|c| !matches!(c, '{' | '}'))
                .collect::<String>()
                // the colour marks `g:`, `y:` and `d:` are two characters each
                .replace("g:", "")
                .replace("y:", "")
                .replace("d:", "");
            plain
                .chars()
                .skip(field)
                .collect::<String>()
                .trim()
                .to_string()
        })
        .collect()
}

/// A line the row wraps onto never starts with a separator: not `·`, not `→`, at any width.
#[test]
fn a_wrapped_line_never_starts_with_a_separator() {
    for row in [untouched(), dac_changes(), resampled(), config_fails()] {
        for width in 24..130 {
            for line in bodies(&row, width, true) {
                assert!(
                    !line.starts_with('·') && !line.starts_with('→'),
                    "{width}: {line:?}"
                );
            }
        }
    }
}

/// Narrow, the hops wrap at the arrows (the arrow ends its line) and the values at the dots; the
/// label stays on the first line and every line is its own, the values never joined to the hops.
#[test]
fn a_narrow_row_wraps_each_line_on_its_own_separators() {
    let lines = bodies(&dac_changes(), 38, true);
    assert_eq!(
        lines,
        [
            "player → computer →",
            "DAC USB 1",
            "48 kHz · volume -24.1 dB",
            "tone control"
        ]
    );
    // the continuation of the values keeps its colour
    let marks = marked(&dac_changes(), 38, true);
    assert!(marks[3].ends_with("{y:tone control}"), "{marks:?}");
}

/// A row that cannot wrap is cut with `…` on each of its lines, or scrolled as one.
#[test]
fn a_row_that_cannot_wrap_is_cut_or_scrolled_line_by_line() {
    let cut = bodies(&dac_changes(), 36, false);
    assert_eq!(cut.len(), 2);
    assert!(cut[0].ends_with('…') && cut[1].ends_with('…'), "{cut:?}");
    let theme = Theme::new(false);
    let columns = widgets::rows::Columns::fit(36, 15, 0);
    let scrolled = drawn_row(&dac_changes(), &theme, columns, (false, Some(5)));
    assert_eq!(scrolled.len(), 2);
    let text: String = scrolled[1]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(text.contains('…'), "{text}");
}

/// The computer's own screen has two hops, the player and the computer, and no DAC.
#[test]
fn the_computer_screen_has_two_hops() {
    let row = PathRow {
        label: RowLabel {
            tone: Tone::Plain,
            words: "Path".to_string(),
        },
        hops: vec![
            hop("player", &[]),
            hop("computer", &[word("muted", Tone::Warn)]),
        ],
        values: vec![word("48 kHz", Tone::Dim)],
        words: None,
    };
    let lines = marked(&row, 120, true);
    assert_eq!(
        lines,
        expect(
            120,
            &["Path~player{d: → }computer {y:muted}", "@{d:48 kHz}"]
        )
    );
}

/// A row with no hops shows the short words, plain, on one line.
#[test]
fn a_row_with_no_hops_is_one_plain_line() {
    let row = PathRow {
        label: RowLabel {
            tone: Tone::Plain,
            words: "Path: the computer plays to DELL S2721QS".to_string(),
        },
        hops: Vec::new(),
        values: Vec::new(),
        words: Some("→ DELL S2721QS".to_string()),
    };
    assert_eq!(
        marked(&row, 120, true),
        expect(120, &["Path~→ DELL S2721QS"])
    );
}

/// A row at 80 columns' room (21 cells for the words): resampled and a failing config check,
/// which needs six lines wrapped whole.
fn resampled_failing() -> PathRow {
    case(
        Tone::Warn,
        &[word("resampled", Tone::Warn)],
        vec![
            word("44.1 → 48 kHz", Tone::Dim),
            word("volume 0 dB", Tone::Dim),
            word("config 1", Tone::Warn),
        ],
    )
}

/// The DAC at -24.1 dB with its tone control on, and a failing config check.
fn dac_changes_failing() -> PathRow {
    let mut row = dac_changes();
    row.values.push(word("config 1", Tone::Warn));
    row
}

/// When both lines do not fit the five lines, the values line still wraps whole (the config tag
/// with it) and the hops line is the one cut on a single line.
#[test]
fn the_values_keep_their_lines_and_the_hops_are_cut_when_both_do_not_fit() {
    let cases = [
        (
            resampled_failing(),
            vec!["44.1 → 48 kHz", "volume 0 dB", "config 1"],
        ),
        (
            dac_changes_failing(),
            vec!["48 kHz", "volume -24.1 dB", "tone control", "config 1"],
        ),
    ];
    for (row, values) in cases {
        for scroll in [None, Some(0)] {
            let theme = Theme::new(false);
            let columns = widgets::rows::Columns::fit(34, 15, 0);
            let lines = drawn_row(&row, &theme, columns, (true, scroll));
            let text: Vec<String> = lines
                .iter()
                .map(|l| {
                    l.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                })
                .collect();
            assert_eq!(lines.len(), 1 + values.len(), "{text:#?}");
            assert!(
                text[0].trim_end().ends_with('…') || scroll.is_some(),
                "{text:#?}"
            );
            assert!(text[0].contains("player → computer"), "{text:#?}");
            let wrapped: Vec<&str> = text[1..].iter().map(|line| line.trim()).collect();
            assert_eq!(wrapped, values, "{text:#?}");
        }
    }
}

/// Values that need more lines than are left cut both lines; with scrolling off the trailing
/// `config N` stays whole.
#[test]
fn a_cut_values_line_keeps_a_trailing_config_tag_whole() {
    let mut row = resampled_failing();
    row.values = vec![
        word("44.1 → 48 kHz", Tone::Dim),
        word("volume 80 %", Tone::Warn),
        word("tone control", Tone::Warn),
        word("balance off centre", Tone::Warn),
        word("NOS filter", Tone::Warn),
        word("config 1", Tone::Warn),
    ];
    let theme = Theme::new(false);
    let columns = widgets::rows::Columns::fit(34, 15, 0);
    let lines = drawn_row(&row, &theme, columns, (true, None));
    assert_eq!(lines.len(), 2);
    let values: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(values.trim_end().ends_with("… · config 1"), "{values:?}");
    let tag = lines[1].spans.last().unwrap();
    assert_eq!(tag.style, theme.style(Role::Warn));
}

/// The untouched path of a DAC through the whole screen's buffer, so the pane's own restyling is
/// covered: the label cells green, the arrows and the values dim, the hop names plain.
#[test]
fn the_screen_draws_the_untouched_row_in_its_colours() {
    use super::host_group::{daemon_hosting, gungnir_route};
    use serde_json::json;
    let theme = Theme::new(false);
    let mut app = daemon_hosting(gungnir_route(|_| {}));
    hear(
        &mut app,
        vec![
            vo("10", Some("volume"), json!(127), "0.0 dB"),
            vo("11", Some("balance"), json!(64), "0"),
            vo("4", Some("nos"), json!(0), "off"),
            vo("12.50", None, json!(1), "1"),
        ],
    );
    let buffer = frame_with_theme(&app, 160, 45, &theme);
    let rows = text(&buffer);
    let y = rows
        .iter()
        .position(|r| r.contains(" Path "))
        .expect("a Path row");
    let x_of = |row: &str, word: &str| {
        u16::try_from(row[..row.find(word).unwrap()].chars().count()).unwrap()
    };
    // a cell is drawn in a role when it has the role's colour and attributes
    let is = |role: Role, x: u16, y: usize| {
        let want = theme.style(role);
        let cell = &buffer[(x, u16::try_from(y).unwrap())];
        want.fg.is_none_or(|fg| cell.fg == fg) && cell.modifier.contains(want.add_modifier)
    };
    let label = x_of(&rows[y], "Path");
    assert!(is(Role::Good, label, y) && is(Role::Good, label + 3, y));
    assert!(is(Role::Dim, x_of(&rows[y], "→"), y));
    assert!(!is(Role::Dim, x_of(&rows[y], "player"), y));
    assert!(!is(Role::Good, x_of(&rows[y], "computer"), y));
    assert!(is(Role::Dim, x_of(&rows[y + 1], "48 kHz"), y + 1));
    assert!(is(Role::Dim, x_of(&rows[y + 1], "volume"), y + 1));
}
