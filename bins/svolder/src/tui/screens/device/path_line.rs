// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Path row's words and how they are laid in a row too narrow for them: the stages of each
//! of its two lines, the wrap at the separators onto continuation lines, and the window a slow
//! scroll shows.
//!
//! The row model ([`PathRow`]) says every word and its tone; this module only lays them out.
//! Line one is the hops joined by ` → `, line two the values joined by ` · `. A stage is one hop
//! (or one value) with the separator before it; the wrap breaks between stages, and inside a
//! stage wider than a line only between its words.

use ratatui::text::Span;
use svolder_ipc::host::{PathRow, Tone};

use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::text::{cells, wrap as wrap_words};

/// What joins one value of the values line to the next.
pub(crate) const VALUE_JOIN: &str = " · ";
/// What joins one hop of the path to the next.
pub(super) const HOP_JOIN: &str = " → ";

/// The most lines a wrapped path takes, both of its lines together.
pub(super) const MAX_LINES: usize = 5;
/// The most lines an unwrapped path takes: the hops, then the values.
pub(super) const MAX_BASE_LINES: usize = 2;

/// The colour a tone is drawn in.
pub(crate) fn role(tone: Tone) -> Role {
    match tone {
        Tone::Plain => Role::Text,
        Tone::Dim => Role::Dim,
        Tone::Good => Role::Good,
        Tone::Warn => Role::Warn,
    }
}

/// One stage of a line: a hop or a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stage {
    /// What leads the stage when it follows another on a line (empty on the first), drawn dim.
    pub(super) join: &'static str,
    /// The stage's words, each in its own colour, a blank between two of them; a line break
    /// leaves a word whole.
    pub(super) parts: Vec<(String, Role)>,
    /// Whether the stage is a hop (or short words), whose words a line break may split at their
    /// blanks; a value is never split.
    pub(super) hop: bool,
}

/// The two lines of a row as stages. The second is empty for a row with no values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RowLines {
    /// The hops, or the short words of a row with no hops.
    pub(super) hops: Vec<Stage>,
    /// The values.
    pub(super) values: Vec<Stage>,
}

impl RowLines {
    /// Both lines, the hops first, without the empty one.
    pub(super) fn lines(&self) -> impl Iterator<Item = &Vec<Stage>> {
        [&self.hops, &self.values]
            .into_iter()
            .filter(|line| !line.is_empty())
    }
}

/// The stages of `row`: each hop as its label in the plain colour then its words in theirs,
/// each value in its own; a row with no hops is its short words, plain, on the first line.
pub(super) fn row_lines(row: &PathRow) -> RowLines {
    if row.hops.is_empty() {
        let words = row.words.as_deref().unwrap_or_default();
        // the stages the words are made of (`a · b`): a line break falls between them, so the
        // dot never starts a line
        let hops = words
            .split(VALUE_JOIN)
            .map(|stage| {
                stage
                    .split(' ')
                    .filter(|word| !word.is_empty())
                    .map(|word| (word.to_string(), Role::Text))
                    .collect::<Vec<_>>()
            })
            .filter(|parts| !parts.is_empty())
            .enumerate()
            .map(|(at, parts)| Stage {
                join: if at == 0 { "" } else { VALUE_JOIN },
                parts,
                hop: true,
            })
            .collect();
        return RowLines {
            hops,
            values: Vec::new(),
        };
    }
    let hops = row
        .hops
        .iter()
        .enumerate()
        .map(|(at, hop)| Stage {
            join: if at == 0 { "" } else { HOP_JOIN },
            parts: std::iter::once((hop.label.clone(), Role::Text))
                .chain(hop.words.iter().map(|w| (w.text.clone(), role(w.tone))))
                .collect(),
            hop: true,
        })
        .collect();
    let values = row
        .values
        .iter()
        .enumerate()
        .map(|(at, value)| Stage {
            join: if at == 0 { "" } else { VALUE_JOIN },
            parts: vec![(value.text.clone(), role(value.tone))],
            hop: false,
        })
        .collect();
    RowLines { hops, values }
}

/// A piece of a line and its colour.
type Piece = (String, Role);

/// The pieces of `stage` on one line: the separator in front (dim), then the words.
fn pieces(stage: &Stage, lead: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    if !lead.is_empty() {
        out.push((lead.to_string(), Role::Dim));
    }
    for (at, (text, role)) in stage.parts.iter().enumerate() {
        if at > 0 {
            out.push((" ".to_string(), Role::Text));
        }
        out.push((text.clone(), *role));
    }
    out
}

/// The cells of `pieces`.
fn width_of(pieces: &[Piece]) -> usize {
    pieces.iter().map(|(text, _)| cells(text)).sum()
}

/// The whole line as one run of pieces.
pub(super) fn flat(stages: &[Stage]) -> Vec<Piece> {
    stages
        .iter()
        .flat_map(|stage| pieces(stage, stage.join))
        .collect()
}

/// The whole line as text.
#[cfg(test)]
pub(super) fn full_text(stages: &[Stage]) -> String {
    flat(stages).into_iter().map(|(text, _)| text).collect()
}

/// The cells the whole line takes on one line.
pub(super) fn full_width(stages: &[Stage]) -> usize {
    width_of(&flat(stages))
}

/// What ends a line of hops whose next hop starts the line below: the arrow, dim.
const HOP_TAIL: &str = " →";

/// Whether the stage after `at` is a hop that follows it with an arrow.
fn hop_follows(stages: &[Stage], at: usize) -> bool {
    stages.get(at + 1).is_some_and(|next| next.join == HOP_JOIN)
}

/// The stages laid on lines of at most `room` cells: each line takes stages while they fit, and
/// the next stage starts a continuation line. A hop followed by another keeps room for the
/// ` →` that ends its line, so a continuation line starts with the next hop's words and never
/// with an arrow or a `·`. A stage wider than a line breaks between its words (a word of a hop
/// wider than a line at its blanks, a value never) onto lines indented by two cells; `None` where
/// a piece is wider than a line, and where more than `max` lines would be needed.
pub(super) fn wrap(stages: &[Stage], room: usize, max: usize) -> Option<Vec<Vec<Piece>>> {
    let mut lines: Vec<Vec<Piece>> = vec![Vec::new()];
    let mut used = 0;
    for (at, stage) in stages.iter().enumerate() {
        // the arrow that ends the line when the next hop does not follow on it
        let tail = if hop_follows(stages, at) {
            cells(HOP_TAIL)
        } else {
            0
        };
        let joined = pieces(stage, stage.join);
        let width = width_of(&joined);
        if used > 0 && used + width + tail <= room {
            used += width;
            lines.last_mut()?.extend(joined);
            continue;
        }
        let text = match used {
            0 => joined,
            _ => pieces(stage, ""),
        };
        let width = width_of(&text);
        if used > 0 {
            if stage.join == HOP_JOIN {
                lines.last_mut()?.push((HOP_TAIL.to_string(), Role::Dim));
            }
            lines.push(Vec::new());
        }
        if width + tail <= room {
            used = width;
            lines.last_mut()?.extend(text);
            continue;
        }
        // A stage wider than a line breaks between its words, the lines after the first
        // indented by two cells; a hop followed by another leaves room on its last line for
        // the arrow.
        let limit = room.saturating_sub(tail);
        let hang = CONTINUED;
        let mut current: Vec<Piece> = Vec::new();
        let mut taken = 0;
        let mut fresh = true;
        // a word wider than a line is broken at its own blanks
        let words = stage.parts.iter().flat_map(|(word, role)| {
            match stage.hop && hang + cells(word) > limit {
                true => word
                    .split(' ')
                    .map(|piece| (piece, *role))
                    .collect::<Vec<_>>(),
                false => vec![(word.as_str(), *role)],
            }
        });
        for (word, role) in words {
            let wide = cells(word);
            if hang + wide > limit {
                return None;
            }
            if !fresh && taken + 1 + wide > limit {
                lines.last_mut()?.append(&mut current);
                lines.push(Vec::new());
                current.push((" ".repeat(hang), Role::Text));
                taken = hang;
                fresh = true;
            }
            if !fresh {
                current.push((" ".to_string(), Role::Text));
                taken += 1;
            }
            current.push((word.to_string(), role));
            taken += wide;
            fresh = false;
        }
        lines.last_mut()?.append(&mut current);
        // the next stage starts a line of its own
        used = room;
    }
    (lines.len() <= max).then_some(lines)
}

/// The least cells a line carried on inside a stage is indented by, so that it never reads as a
/// stage of its own.
const CONTINUED: usize = 2;

/// The lines of the row's tooltip, each at most `width` cells: each of the row's lines wrapped at
/// its separators as far as it can be, and a stage wider than a line broken at its blanks.
pub(super) fn tip_lines(row: &RowLines, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for stages in row.lines() {
        for line in wrap(stages, width, usize::MAX).unwrap_or_else(|| loose(stages)) {
            let text: String = line.into_iter().map(|(text, _)| text).collect();
            let text = text.trim_start().to_string();
            if cells(&text) <= width {
                lines.push(text);
            } else if let Some(body) = text.strip_suffix(HOP_TAIL) {
                // the arrow stays on the hop's last line, never alone on a line
                let mut broken = wrap_words(body, width.saturating_sub(cells(HOP_TAIL)));
                if let Some(last) = broken.last_mut() {
                    last.push_str(HOP_TAIL);
                }
                lines.extend(broken);
            } else {
                lines.extend(wrap_words(&text, width));
            }
        }
    }
    lines
}

/// Every stage on a line of its own, for a line whose stages do not fit.
fn loose(stages: &[Stage]) -> Vec<Vec<Piece>> {
    stages
        .iter()
        .enumerate()
        .map(|(at, stage)| {
            let mut line = pieces(stage, "");
            if hop_follows(stages, at) {
                line.push((HOP_TAIL.to_string(), Role::Dim));
            }
            line
        })
        .collect()
}

/// The spans of `stages` on one line as they stand from cell `offset` on, `room` cells of them:
/// the window a scrolled line shows, with a `…` in its first cell where the line goes on to the
/// left of it and in its last where it goes on to the right.
pub(super) fn window(
    stages: &[Stage],
    theme: &Theme,
    offset: usize,
    room: usize,
) -> Vec<Span<'static>> {
    let all = flat(stages);
    let total = width_of(&all);
    let before = usize::from(offset > 0 && room > 1);
    let after = usize::from(offset + room < total && room > 1);
    let (from, until) = (offset + before, offset + room - after);
    let mut spans = Vec::new();
    if before == 1 {
        spans.push(Span::styled("…", theme.style(Role::Dim)));
    }
    let mut at = 0;
    for (text, role) in all {
        let mut shown = String::new();
        for c in text.chars() {
            let width = cells(c.encode_utf8(&mut [0; 4]));
            if at >= from && at + width <= until {
                shown.push(c);
            }
            at += width;
        }
        if !shown.is_empty() {
            spans.push(Span::styled(shown, theme.style(role)));
        }
    }
    if after == 1 {
        spans.push(Span::styled("…", theme.style(Role::Dim)));
    }
    spans
}

/// `cut`, but a last stage that is the config tag (`config N`) stays whole: the stages before it
/// are cut to what it leaves, their `…` just before its separator.
pub(super) fn cut_keeping_config(
    stages: &[Stage],
    theme: &Theme,
    room: usize,
) -> Vec<Span<'static>> {
    let tagged = stages.last().filter(|stage| {
        stage
            .parts
            .first()
            .is_some_and(|(text, _)| text.starts_with("config "))
    });
    let (Some(tag), true) = (tagged, stages.len() > 1) else {
        return cut(stages, theme, room);
    };
    let tag = pieces(tag, tag.join);
    let tag_width = width_of(&tag);
    if full_width(stages) <= room || tag_width + 2 > room {
        return cut(stages, theme, room);
    }
    // The words before the tag do not fit what it leaves, so they end in the `…`.
    let mut spans = cut(&stages[..stages.len() - 1], theme, room - tag_width);
    spans.extend(
        tag.into_iter()
            .map(|(text, role)| Span::styled(text, theme.style(role))),
    );
    spans
}

/// The spans of `stages` on one line cut to `room` cells, the tail replaced by `…` where the
/// words run out of room.
pub(super) fn cut(stages: &[Stage], theme: &Theme, room: usize) -> Vec<Span<'static>> {
    let all = flat(stages);
    if width_of(&all) <= room {
        return all
            .into_iter()
            .map(|(text, role)| Span::styled(text, theme.style(role)))
            .collect();
    }
    let Some(keep) = room.checked_sub(1) else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    let mut used = 0;
    for (text, role) in all {
        let mut shown = String::new();
        for c in text.chars() {
            let width = cells(c.encode_utf8(&mut [0; 4]));
            if used + width > keep {
                break;
            }
            shown.push(c);
            used += width;
        }
        let whole = shown == text;
        if !shown.is_empty() {
            spans.push(Span::styled(shown, theme.style(role)));
        }
        if !whole {
            break;
        }
    }
    spans.push(Span::styled("…", theme.style(Role::Dim)));
    spans
}

#[cfg(test)]
mod tests;
