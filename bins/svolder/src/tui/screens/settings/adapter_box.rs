// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Bluetooth adapter list of the Settings modal: one line per adapter with its address, its
//! USB product, how loudly it hears each configured device and which one is in use, and the
//! mouse target of each line.

use super::*;
use crate::tui::boxes::connections::AdapterList;

/// The widest the list is drawn, borders and all.
const LIST_WIDTH: u16 = 100;
/// What the list says while the scan runs.
const LISTENING: &str = " listening on every adapter…";

/// The adapter list: a line per adapter under a title, the cursor's line marked `▶` and drawn
/// as a selection, and the keys under them. Answers the cells it fills; a click on a line picks
/// that adapter.
pub(super) fn draw_adapter_box(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    list: &AdapterList,
    theme: &Theme,
    live_hits: &mut HitMap,
) -> Rect {
    let dim = theme.style(Role::Dim);
    let table = list
        .rows
        .as_deref()
        .map(crate::commands::adapters::text_lines)
        .unwrap_or_default();
    let widest = table.iter().map(|line| cell_count(line)).max().unwrap_or(0);
    let width = LIST_WIDTH
        .min(area.width)
        .min(widest.saturating_add(5).max(PROMPT_WIDTH));
    let room = usize::from(width.saturating_sub(2 + 3));
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut rows_at = Vec::new();
    match (&list.rows, &list.failed) {
        (Some(_), _) => {
            for (at, text) in table.iter().enumerate() {
                let cut = widgets::text::cut_to_cells(text, room);
                let focused = at == list.at;
                let mark = if focused { "▶ " } else { "  " };
                let line = Line::from(vec![Span::raw(" "), Span::raw(mark), Span::raw(cut)]);
                rows_at.push(lines.len());
                lines.push(match focused {
                    true => line.style(theme.style(Role::Selected)),
                    false => line,
                });
            }
        }
        (None, Some(why)) => lines.push(Line::from(Span::styled(format!(" {why}"), dim))),
        (None, None) => lines.push(Line::from(Span::styled(LISTENING, dim))),
    }
    if let Some(note) = &list.note {
        for part in widgets::text::wrap(note, room) {
            lines.push(Line::from(Span::styled(format!(" {part}"), dim)));
        }
    }
    lines.push(Line::default());
    lines.push(widgets::surfaces::modal_keys(
        &[("Enter", "picks"), ("↑↓", "move"), ("Esc", "closes")],
        theme,
    ));
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    let block =
        widgets::surfaces::modal_block(Block::bordered(), " Bluetooth adapter ", app, theme);
    let inner = block.inner(box_area);
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    for (index, line) in rows_at.into_iter().enumerate() {
        let y = inner.y + u16::try_from(line).unwrap_or(u16::MAX);
        if y < inner.bottom() {
            live_hits.push(
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
                Hit::AdapterRow(index),
            );
        }
    }
    box_area
}
