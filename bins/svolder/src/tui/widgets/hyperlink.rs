// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Web UI box's link as a terminal hyperlink (OSC 8).
//!
//! The screen's buffer holds cells, which cannot carry an escape sequence, so the link is made
//! after a frame is drawn: the cells the link's lines fill are written again with the sequence
//! that opens the link before them and the one that closes it after them, in the colours the
//! frame gave them. Every line carries the same `id=`, which makes a terminal treat the wrapped
//! lines as one link. A terminal that knows nothing of OSC 8 ignores both sequences and shows
//! the same cells it already showed; one that knows it opens the link on Ctrl+click and offers
//! it in its own context menu (VTE: Ptyxis, GNOME Terminal), which an application that tracks
//! the mouse, as this one does, shows only on Shift+right-click.

use std::io::Write;

use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::tui::app::App;
use crate::tui::boxes::connections::Modal;
use crate::tui::mouse::{Hit, HitMap};

/// The `id=` every line of the link carries.
const ID: &str = "svolder-web-link";

/// Whether the Web UI box shows a link now.
pub fn shown(app: &App) -> bool {
    matches!(
        &app.overlays.conn.modal,
        Some(Modal::Web(web)) if web.link.as_ref().is_some_and(|link| link.until > app.now)
    )
}

/// The bytes that make the link on show a hyperlink over the frame `buffer`, which was drawn
/// with `hits`: empty when no link is on show, when its text has a control character in it, or
/// when the cells under the link's hit rectangles do not read as the link (something is drawn
/// over it).
pub fn web_link(app: &App, hits: &HitMap, buffer: &Buffer) -> Vec<u8> {
    let Some(Modal::Web(web)) = &app.overlays.conn.modal else {
        return Vec::new();
    };
    let Some(link) = web.link.as_ref().filter(|link| link.until > app.now) else {
        return Vec::new();
    };
    let lines: Vec<Rect> = hits.areas_of(Hit::WebLink).collect();
    around(buffer, &lines, &link.text)
}

/// The bytes that write the cells of `lines` in `buffer` again as the hyperlink `url`, the
/// cursor left where it was; empty unless the cells read as `url` from the first line to the
/// last, and `url` has no control character.
pub fn around(buffer: &Buffer, lines: &[Rect], url: &str) -> Vec<u8> {
    if lines.is_empty() || url.chars().any(char::is_control) {
        return Vec::new();
    }
    let read: String = lines
        .iter()
        .flat_map(|line| (line.x..line.right()).map(move |x| (x, line.y)))
        .map(|at| buffer.cell(at).map_or("", |cell| cell.symbol()))
        .collect();
    if read != url {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut backend = CrosstermBackend::new(&mut out);
    let sent = (|| {
        backend.write_all(b"\x1b7")?;
        for line in lines {
            write!(backend, "\x1b]8;id={ID};{url}\x1b\\")?;
            let cells = (line.x..line.right())
                .filter_map(|x| buffer.cell((x, line.y)).map(|cell| (x, line.y, cell)));
            backend.draw(cells)?;
            write!(backend, "\x1b]8;;\x1b\\")?;
        }
        backend.write_all(b"\x1b8")?;
        Write::flush(&mut backend)
    })();
    match sent {
        Ok(()) => out,
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
