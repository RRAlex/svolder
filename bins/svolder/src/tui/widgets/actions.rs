// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The words on a pane's top border that act as keys: `mute`, `0 centre`, `refresh`, and the
//! mark at the far right (a count, or `more ↑`).
//!
//! A screen lists its [`Action`]s, [`actions_title`] draws as many as fit the border and says
//! where each landed, and [`push_action_hits`] turns those places into mouse targets, so a click
//! on a word presses the key it names.

use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use super::text::cells;
use crate::tui::app::Screen;
use crate::tui::keys::{self, Scene};
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::theme::{Role, Theme};

/// One word of a pane's top border: the key it presses and what it says.
///
/// A word whose key is its first letter draws that letter as the key; a word that starts with
/// its key and a space (`0 centre`, `x remove`) draws the key before the name. Either way the
/// word's first character is the key.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Action {
    /// The key a click on the word presses.
    pub code: KeyCode,
    /// What the border says, key included.
    pub word: &'static str,
}

/// What joins two words of a border, and a word to the mark at the far right.
const ACTION_JOIN: &str = " ─ ";

/// The cells of a pane's top border a right-aligned title line may take: the border `width`
/// cells wide less its two corners, the left title of `title` cells and the two `─` that keep
/// the title off the words.
pub(crate) fn action_room(width: u16, title: usize) -> u16 {
    let taken = title.saturating_add(4);
    u16::try_from(usize::from(width).saturating_sub(taken)).unwrap_or(0)
}

/// The right-aligned title line of a pane's top border: `actions` joined by ` ─ `, then `mark`
/// (a count or `more ↑`, without blanks of its own) at the far right, with one blank cell each
/// side. Words are dropped from the last one back until the line fits `room` cells; the mark
/// stays. Each drawn word is also answered with its offset from the line's start, its width
/// and its key.
///
/// A word is drawn in the key and text roles, or whole in the faint role where its key is not
/// worth pressing on `scene` (`keys::available_in` on `screen`'s own hint for it).
pub(crate) fn actions_title(
    actions: &[Action],
    mark: Option<&str>,
    room: u16,
    screen: Screen,
    scene: &Scene,
    theme: &Theme,
) -> (Line<'static>, Vec<(u16, u16, KeyCode)>) {
    let join = cells(ACTION_JOIN);
    let mark_cells = mark.map_or(0, cells);
    let width_of = |shown: usize| {
        let words: usize = actions[..shown].iter().map(|a| cells(a.word)).sum();
        let joins = shown.saturating_sub(1) * join;
        let tail = match (shown, mark) {
            (0, Some(_)) => mark_cells,
            (_, Some(_)) => join + mark_cells,
            (_, None) => 0,
        };
        2 + words + joins + tail
    };
    let shown = (0..=actions.len())
        .rev()
        .find(|shown| width_of(*shown) <= usize::from(room))
        .unwrap_or(0);
    if shown == 0 && mark.is_none() {
        return (Line::default(), Vec::new());
    }
    let rule = theme.style(Role::Rule);
    let key = theme.style(Role::KeyHint).add_modifier(Modifier::BOLD);
    let text = theme.style(Role::Text);
    let mut spans = vec![Span::raw(" ")];
    let mut hits = Vec::new();
    let mut at = 1usize;
    for (n, action) in actions[..shown].iter().enumerate() {
        if n > 0 {
            spans.push(Span::styled(ACTION_JOIN, rule));
            at += join;
        }
        let available = keys::hints_for(screen)
            .into_iter()
            .find(|hint| hint.code == action.code)
            .is_some_and(|hint| keys::available_in(hint, *scene));
        match available {
            true => {
                let (first, rest) = action
                    .word
                    .split_at(action.word.chars().next().map_or(0, char::len_utf8));
                spans.push(Span::styled(first, key));
                spans.push(Span::styled(rest, text));
            }
            false => spans.push(Span::styled(action.word, theme.style(Role::Faint))),
        }
        let width = cells(action.word);
        hits.push((
            u16::try_from(at).unwrap_or(u16::MAX),
            u16::try_from(width).unwrap_or(u16::MAX),
            action.code,
        ));
        at += width;
    }
    if let Some(mark) = mark {
        if shown > 0 {
            spans.push(Span::styled(ACTION_JOIN, rule));
        }
        spans.push(Span::styled(
            mark.to_string(),
            crate::tui::screens::device::frame_title(theme),
        ));
    }
    spans.push(Span::raw(" "));
    (Line::from(spans), hits)
}

/// Record the hits of the words of `line`, drawn right-aligned in the top border of a pane in
/// `area`, from what [`actions_title`] answered. Push them after the border's other hits so
/// the words win.
pub(crate) fn push_action_hits(
    hits: &mut HitMap,
    area: Rect,
    line: &Line<'_>,
    words: &[(u16, u16, KeyCode)],
) {
    let start = area
        .right()
        .saturating_sub(1)
        .saturating_sub(u16::try_from(line.width()).unwrap_or(u16::MAX));
    for (offset, width, code) in words {
        let word = Rect {
            x: start.saturating_add(*offset),
            y: area.y,
            width: *width,
            height: 1.min(area.height),
        };
        hits.push(word.intersection(area), Hit::Key(*code));
    }
}

#[cfg(test)]
mod tests;
