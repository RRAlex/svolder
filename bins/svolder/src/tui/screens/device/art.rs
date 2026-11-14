// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The cover art at the right of the Status pane's rows, beside the track's title.
//!
//! The picture takes the pane's interior height and as many columns as a square of that height
//! is wide in the terminal's own cells, and the pane's rows are fit to what it leaves, so it is
//! never over a value. It goes when the rows would have less than [`MIN_WORDS`] columns left,
//! which is before the title does at any narrower width.

use ratatui::layout::{Rect, Size};

use crate::tui::app::{App, Screen};
use crate::tui::layout_tier::Tier;

use super::rows_of;
use super::status::now_playing_drawn;

/// Columns between the picture and the rows' words.
pub(super) const GAP: u16 = 2;

/// The fewest columns the rows' words keep beside the art: what the Device row and the readouts
/// need to say what they say at their usual length.
const MIN_WORDS: u16 = 64;

/// The fewest columns and rows a picture is worth drawing in.
const MIN_COLS: u16 = 4;
const MIN_ROWS: u16 = 3;

/// The most picture the Status pane at `status` (in a frame of `tier`) can take beside its rows
/// for `app`: its interior height by the columns that make a square in the terminal's cells,
/// held to what leaves the words [`MIN_WORDS`]. `None` with no cover to show, art off, no
/// `Now playing` row to be beside, or no room.
pub(super) fn box_in(status: Rect, tier: Tier, app: &App) -> Option<Size> {
    let (cell_w, cell_h) = app.host.cover.font.filter(|(w, h)| *w > 0 && *h > 0)?;
    if app.screen != Screen::Device
        || app.session.holding.is_some()
        || app.host.cover.hash.is_none()
        || !now_playing_drawn(app)
    {
        return None;
    }
    let inner = tier.block().inner(status);
    let trail = u16::try_from(tier.trail()).unwrap_or(u16::MAX);
    let room = inner.width.saturating_sub(trail + GAP + MIN_WORDS);
    let square = u32::from(inner.height) * u32::from(cell_h);
    let square = u16::try_from(square.div_ceil(u32::from(cell_w))).unwrap_or(u16::MAX);
    let size = Size {
        width: square.min(room),
        height: inner.height,
    };
    (size.width >= MIN_COLS && size.height >= MIN_ROWS).then_some(size)
}

/// [`box_in`] for a frame `area` large: where the Status pane lands is the screen's own layout.
pub(crate) fn art_box(area: Rect, app: &App) -> Option<Size> {
    let (tier, [_, _, status]) = rows_of(area, app);
    box_in(status, tier, app)
}

/// Where the picture `app` holds is drawn in the Status pane at `status`, and the columns the
/// pane's rows give up for it (the picture's own and the gap): `None` when it has no picture or
/// the picture no longer fits the box (the terminal changed under it and the next one is on its
/// way).
pub(super) fn slot_in(status: Rect, tier: Tier, app: &App) -> Option<(Rect, u16)> {
    let picture = app.host.cover.picture.as_ref()?;
    // The picture of the track before the one the daemon names now is not drawn under it.
    if app.host.cover.hash.as_deref() != Some(picture.hash.as_str()) {
        return None;
    }
    let size = picture.size();
    let most = box_in(status, tier, app)?;
    if size.width > most.width || size.height > most.height {
        return None;
    }
    let inner = tier.block().inner(status);
    let trail = u16::try_from(tier.trail()).unwrap_or(u16::MAX);
    let slot = Rect {
        x: inner.right().saturating_sub(trail + size.width),
        y: inner.y,
        width: size.width,
        height: size.height,
    };
    Some((slot, size.width + GAP))
}

/// Where the picture `app` holds is drawn in a frame `area` large, if the frame draws it.
pub(crate) fn art_slot(area: Rect, app: &App) -> Option<Rect> {
    let (tier, [_, _, status]) = rows_of(area, app);
    slot_in(status, tier, app).map(|(slot, _)| slot)
}

/// The columns the Now playing row's words have less of for the art `app` shows at its size,
/// which the marquee that carries long words is worked out against.
pub(crate) fn art_reserved(app: &App) -> usize {
    let (width, height) = app.size;
    let (tier, [_, _, status]) = rows_of(Rect::new(0, 0, width, height), app);
    slot_in(status, tier, app).map_or(0, |(_, reserved)| usize::from(reserved))
}

/// Whether the picture is held back from this frame because something is drawn over its first
/// cell: a box (while one is open, whatever it covers), or, until the picture has been written
/// once, a toast or a tooltip over its corner. All of them `Clear` the cell the kitty protocol
/// sends the picture in, which wipes it unseen.
pub(super) fn held(frame: Rect, slot: Rect, picture: &crate::tui::art::Picture, app: &App) -> bool {
    if app.modal_open() {
        return true;
    }
    if picture.sent.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    let corner = ratatui::layout::Position::new(slot.x, slot.y);
    let toast = app.toast().is_some_and(|toast| {
        let (_, [_, body]) = crate::tui::layout_tier::split(frame);
        crate::tui::boxes::toast::area(body, &toast.text)
            .0
            .contains(corner)
    });
    // A tooltip is placed under the pointer, up to its widest line wide and two lines high.
    let reach = u16::try_from(crate::tui::boxes::tooltip::MAX_WIDTH).unwrap_or(u16::MAX);
    let tooltip = app.pointer.tooltip.is_some_and(|held| {
        held.shown
            && held.at.0 + reach >= slot.x
            && held.at.0 <= slot.right()
            && held.at.1 + 3 >= slot.y
            && held.at.1 <= slot.bottom() + 3
    });
    toast || tooltip
}
