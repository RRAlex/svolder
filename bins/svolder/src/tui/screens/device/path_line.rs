// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Path row's words and how they are laid in a row too narrow for them: the stages of the
//! path, the wrap at the hop separators onto continuation lines, and the window a slow scroll
//! shows.
//!
//! A stage is one hop of the path (or the verdict that leads it) with the separator before it;
//! the wrap breaks only between stages, never inside one.

use ratatui::text::Span;
use svolder_ipc::host::{PathLine, Verdict};

use crate::tui::app::Chip;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::text::{cells, wrap as wrap_words};

/// What joins the verdict to the first hop of the path.
pub(super) const PATH_GAP: &str = " · ";
/// What joins one hop of the path to the next.
pub(super) const HOP_JOIN: &str = " → ";

/// The most lines a wrapped path takes.
pub(super) const MAX_LINES: usize = 3;

/// One stage of the path: the verdict's words, or a hop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stage {
    /// What leads the stage when it follows another on a line (empty on the first).
    pub(super) join: &'static str,
    /// The stage's words, which are never broken.
    pub(super) words: String,
    /// The colour the stage is drawn in.
    pub(super) role: Role,
    /// Whether the stage is the config chip, which a cut row keeps whole.
    pub(super) chip: bool,
}

/// The stages of `path`, from the verdict to the DAC: the verdict in the colour of what it says,
/// then each hop as its label and what is known of the signal there, the hop that changes the
/// signal in the warning colour.
#[cfg(test)]
pub(super) fn stages(path: &PathLine) -> Vec<Stage> {
    stages_with(path, None)
}

/// `stages` with the config chip as a stage of its own right after the verdict, so a row cut
/// at its right edge keeps it in sight.
pub(super) fn stages_with(path: &PathLine, chip: Option<&Chip>) -> Vec<Stage> {
    let verdict = match path.verdict {
        Verdict::Untouched => Role::Good,
        Verdict::Changed => Role::Warn,
        Verdict::NotThisDac { .. } | Verdict::OtherInput { .. } | Verdict::Unknown => Role::Dim,
    };
    let mut stages = vec![Stage {
        join: "",
        words: path.words_line(),
        role: verdict,
        chip: false,
    }];
    if let Some(chip) = chip {
        stages.push(Stage {
            join: PATH_GAP,
            words: chip.words.clone(),
            role: if chip.changes { Role::Warn } else { Role::Dim },
            chip: true,
        });
    }
    for (at, hop) in path.hops.iter().enumerate() {
        let words = match hop.detail.is_empty() {
            true => hop.label.clone(),
            false => format!("{} {}", hop.label, hop.detail),
        };
        stages.push(Stage {
            join: if at == 0 { PATH_GAP } else { HOP_JOIN },
            words,
            role: if hop.changes { Role::Warn } else { Role::Dim },
            chip: false,
        });
    }
    stages
}

/// The whole path on one line.
pub(super) fn full_text(stages: &[Stage]) -> String {
    stages
        .iter()
        .map(|stage| format!("{}{}", stage.join, stage.words))
        .collect()
}

/// The cells the whole path takes on one line.
pub(super) fn full_width(stages: &[Stage]) -> usize {
    cells(&full_text(stages))
}

/// The stages laid on lines of at most `room` cells: each line takes stages while they fit, and
/// the next stage starts a continuation line with its separator first (`→ computer …`), without
/// the blank the separator has when it follows a stage. `None` where a stage is wider than a
/// line, as a stage is never broken, and where more than `max` lines would be needed.
pub(super) fn wrap(stages: &[Stage], room: usize, max: usize) -> Option<Vec<Vec<(String, Role)>>> {
    let mut lines: Vec<Vec<(String, Role)>> = vec![Vec::new()];
    let mut used = 0;
    for stage in stages {
        let joined = format!("{}{}", stage.join, stage.words);
        let width = cells(&joined);
        if used > 0 && used + width <= room {
            used += width;
            lines.last_mut()?.push((joined, stage.role));
            continue;
        }
        let text = match used {
            0 => joined,
            _ => format!("{}{}", stage.join.trim_start(), stage.words),
        };
        let width = cells(&text);
        if width > room {
            return None;
        }
        if used > 0 {
            lines.push(Vec::new());
        }
        used = width;
        lines.last_mut()?.push((text, stage.role));
    }
    (lines.len() <= max).then_some(lines)
}

/// The lines of the path's tooltip, each at most `width` cells: the path wrapped at its
/// separators as far as it can be, and a stage wider than a line broken at its blanks.
pub(super) fn tip_lines(stages: &[Stage], width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for line in wrap(stages, width, usize::MAX).unwrap_or_else(|| loose(stages)) {
        let text: String = line.into_iter().map(|(text, _)| text).collect();
        match cells(&text) <= width {
            true => lines.push(text),
            false => lines.extend(wrap_words(&text, width)),
        }
    }
    lines
}

/// Every stage on a line of its own, for a path whose stages do not fit a line.
fn loose(stages: &[Stage]) -> Vec<Vec<(String, Role)>> {
    stages
        .iter()
        .enumerate()
        .map(|(at, stage)| {
            let text = match at {
                0 => stage.words.clone(),
                _ => format!("{}{}", stage.join.trim_start(), stage.words),
            };
            vec![(text, stage.role)]
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
    let total = full_width(stages);
    let before = usize::from(offset > 0 && room > 1);
    let after = usize::from(offset + room < total && room > 1);
    let (from, until) = (offset + before, offset + room - after);
    let mut spans = Vec::new();
    if before == 1 {
        spans.push(Span::styled("…", theme.style(Role::Dim)));
    }
    let mut at = 0;
    for stage in stages {
        let text = format!("{}{}", stage.join, stage.words);
        let mut shown = String::new();
        for c in text.chars() {
            let width = cells(c.encode_utf8(&mut [0; 4]));
            if at >= from && at + width <= until {
                shown.push(c);
            }
            at += width;
        }
        if !shown.is_empty() {
            spans.push(Span::styled(shown, theme.style(stage.role)));
        }
    }
    if after == 1 {
        spans.push(Span::styled("…", theme.style(Role::Dim)));
    }
    spans
}

#[cfg(test)]
mod tests;
