// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Text measured in terminal cells: widths, padding, cutting with `…`, wrapping at blanks, the
//! window a typed line shows around its cursor, and the blank cell a focused row's style needs.
//!
//! A wide character takes two cells, so every widget that fits words to a column measures here
//! rather than counting characters; the rows, the key line and the boxes all share these.

use ratatui::style::Style;
use ratatui::text::Span;

/// The cells `text` takes on a terminal.
pub(crate) fn cells(text: &str) -> usize {
    Span::raw(text).width()
}

/// `text` followed by blanks to fill `width` cells; text that is wider is left whole.
pub(crate) fn pad_cells(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(cells(text))))
}

/// `text` as it is if it fits `room` cells, else cut to end in `…` within them. Measured in
/// cells, not characters.
pub(crate) fn cut_to_cells(text: &str, room: usize) -> String {
    if cells(text) <= room {
        return text.to_string();
    }
    if room == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut taken = 0;
    for c in text.chars() {
        let step = cells(c.encode_utf8(&mut [0; 4]));
        if taken + step + 1 > room {
            break;
        }
        out.push(c);
        taken += step;
    }
    out.push('…');
    out
}

/// `text` broken at its blanks into lines of at most `width` display cells; a word wider than a
/// line is broken where the line ends, and the rest of it starts the next line, where a
/// following word can join it. No text is no lines.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let mut runs = vec![String::new()];
        for c in word.chars() {
            let last = runs.last_mut().expect("a run to add to");
            if cells(last) + cells(c.encode_utf8(&mut [0; 4])) > width && !last.is_empty() {
                runs.push(String::new());
            }
            runs.last_mut().expect("a run to add to").push(c);
        }
        for run in runs {
            match lines.last_mut() {
                Some(line) if cells(line) + 1 + cells(&run) <= width => {
                    line.push(' ');
                    line.push_str(&run);
                }
                _ => lines.push(run),
            }
        }
    }
    lines
}

/// What a field `room` cells wide shows of a line being typed into it with the cursor before
/// character `caret`, and the display column the cursor is drawn at inside it.
///
/// A line that fits is shown whole, the cursor taking the cell after its last character. A
/// longer one is shown from wherever keeps the cursor in the field (its right edge once the
/// cursor has moved past it), with `…` standing for characters cut off at either end — never on
/// the cursor's own cell. A wide character under the cursor that ends the window is shown
/// whole, with no `…` after it.
pub fn typed_window(text: &str, caret: usize, room: usize) -> (String, usize) {
    let glyphs: Vec<(char, usize)> = text
        .chars()
        .map(|c| (c, cells(c.encode_utf8(&mut [0; 4]))))
        .collect();
    let caret = caret.min(glyphs.len());
    if room == 0 {
        return (String::new(), 0);
    }
    // The first glyph from which the caret, drawn in one cell after the text before it, fits.
    let mut start = caret;
    let mut column = 0;
    while start > 0 && column + glyphs[start - 1].1 < room {
        start -= 1;
        column += glyphs[start].1;
    }
    let mut shown: Vec<(String, usize)> = Vec::new();
    let mut used = 0;
    for &(c, width) in &glyphs[start..] {
        if used + width > room {
            break;
        }
        used += width;
        shown.push((c.to_string(), width));
    }
    let end = start + shown.len();
    if start > 0 && column > 0 {
        mark_cut(&mut shown[0]);
    }
    if end < glyphs.len()
        && column + glyphs.get(caret).map_or(1, |&(_, width)| width) < used
        && let Some(last) = shown.last_mut()
    {
        mark_cut(last);
    }
    (shown.into_iter().map(|(text, _)| text).collect(), column)
}

/// `glyph` as `…` padded with blanks to the cells it took.
fn mark_cut(glyph: &mut (String, usize)) {
    glyph.0 = format!("…{}", " ".repeat(glyph.1.saturating_sub(1)));
}

/// One blank cell appended to `spans`, in `style`, where their content leaves at least one
/// column short of `max_width` — so a focused row's selected style (the tone header's halves,
/// a list's focused row) doesn't stop flush against its last character, the way colouring
/// only the drawn glyphs would. Content already reaching `max_width` is left as is.
pub fn pad_selected(spans: &mut Vec<Span<'static>>, style: Style, max_width: usize) {
    let width: usize = spans.iter().map(Span::width).sum();
    if width < max_width {
        spans.push(Span::styled(" ", style));
    }
}

#[cfg(test)]
mod tests;
