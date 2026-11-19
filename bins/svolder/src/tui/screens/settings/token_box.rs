// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MCP token box of the Settings modal: the token on lines of its own, and its mouse target.

use super::*;

/// Columns the token box takes, borders and all.
const TOKEN_WIDTH: u16 = 64;
/// What the box says of itself under the token.
const TOKEN_HINT: &str = "click the token to copy it; the box closes by itself after a minute";

/// The MCP token box: the token on lines as wide as they go with a cell of margin each side, so
/// a selection of them is the token and nothing else, where it comes from, and its keys. Answers
/// the cells it fills; a click on the token's cells copies it.
pub(super) fn draw_token_box(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    live_hits: &mut HitMap,
) -> Rect {
    let width = TOKEN_WIDTH.min(area.width);
    let across = usize::from(width.saturating_sub(2));
    let room = across.saturating_sub(2).max(1);
    let style = theme.style(Role::Text);
    let dim = theme.style(Role::Dim);
    let mut lines: Vec<Line<'static>> = vec![Line::default()];
    // The lines the token fills, with each one's width in cells: what a click copies.
    let mut token_lines: Vec<(usize, u16)> = Vec::new();
    match app.overlays.conn.shown(app.now) {
        Some(token) => {
            for part in widgets::text::wrap(token, room) {
                token_lines.push((lines.len(), cell_count(&part)));
                lines.push(Line::from(vec![Span::raw(" "), Span::styled(part, style)]));
            }
        }
        None => lines.push(Line::from(Span::styled(" …", dim))),
    }
    lines.push(Line::default());
    if let TokenView::Present { source, .. } = &app.overlays.conn.token {
        lines.extend(wrapped(
            &format!("from {}", source_words(source)),
            across,
            dim,
        ));
    }
    lines.extend(wrapped(TOKEN_HINT, across, dim));
    lines.push(Line::default());
    lines.push(widgets::surfaces::modal_keys(
        &[("c", "copies it"), ("Esc", "closes")],
        theme,
    ));
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    let block = widgets::surfaces::modal_block(Block::bordered(), " MCP token ", app, theme);
    let inner = block.inner(box_area);
    for (at, cells) in token_lines {
        let text = Rect {
            x: inner.x + 1,
            y: inner
                .y
                .saturating_add(u16::try_from(at).unwrap_or(u16::MAX)),
            width: cells,
            height: 1,
        };
        let seen = text.intersection(inner);
        if !seen.is_empty() {
            live_hits.push(seen, Hit::McpToken);
        }
    }
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    box_area
}

/// `text` wrapped to `across` cells, a cell in from the left, in `style`.
fn wrapped(text: &str, across: usize, style: Style) -> Vec<Line<'static>> {
    widgets::text::wrap(text, across.saturating_sub(2))
        .into_iter()
        .map(|part| Line::from(Span::styled(format!(" {part}"), style)))
        .collect()
}
