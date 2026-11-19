// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The cover art at the right of the Status pane's rows, beside the track's title.
//!
//! The picture takes the pane's interior height and as many columns as a square of that height
//! is wide in the terminal's own cells, and the pane's rows are fit to what it leaves, so it is
//! never over a value. It goes when the rows would have less than [`MIN_WORDS`] columns left,
//! which is before the title does at any narrower width.
//!
//! On a terminal of [`GROW_COLS`] by [`GROW_ROWS`] or more the Status pane grows upward while a
//! cover is on show ([`growth`]): it takes only the rows the Controls and Tone panes do not
//! draw something in, and no more than the picture can use at the width the words leave it, so
//! the cover is a larger square. Below that size, or with no cover, the pane keeps its usual height.

use ratatui::layout::{Rect, Size};
use svolder_ipc::{ColorDepth, TuiConfig};

use crate::tui::app::{App, Screen};
use crate::tui::layout_tier::Tier;
use crate::tui::theme::Theme;

use super::controls::{controls_lines, room};
use super::status::now_playing_drawn;
use super::tone_pane::tone_lines_in;
use super::{panes_of, rows_of};

/// Columns between the picture and the rows' words.
pub(super) const GAP: u16 = 2;

/// The fewest columns the rows' words keep beside the art: what the Device row and the readouts
/// need to say what they say at their usual length.
const MIN_WORDS: u16 = 64;

/// The terminal size from which the Status pane grows for a cover.
pub(super) const GROW_COLS: u16 = 100;
pub(super) const GROW_ROWS: u16 = 45;

/// The fewest rows of cover worth taking the panes' blank lines for: a Controls pane whose
/// spaced lines would leave less than this to the cover is packed instead.
const USEFUL_ROWS: u16 = 8;

/// The fewest columns and rows a picture is worth drawing in.
const MIN_COLS: u16 = 4;
const MIN_ROWS: u16 = 3;

/// The terminal's cell size in pixels when `app` has a cover to show beside a `Now playing` row:
/// the track names one, and it has not failed to fetch or decode.
fn wanted(app: &App) -> Option<(u16, u16)> {
    let cell = app.host.cover.font.filter(|(w, h)| *w > 0 && *h > 0)?;
    (app.screen == Screen::Device
        && app.session.holding.is_none()
        && app.host.cover.hash.is_some()
        && app.host.cover.hash != app.host.cover.failed
        && now_playing_drawn(app))
    .then_some(cell)
}

/// The rows the Controls and Tone panes of a frame whose panes band is `panes` wide draw their
/// content in, borders and blank top row included: `spaced` sets the Controls pane's groups a
/// blank line apart as it does when it has room. The drawn-state flags the build writes are put
/// back, as a layout question is no draw.
fn panes_need(app: &App, tier: Tier, panes: Rect, spaced: bool) -> u16 {
    // Line counts do not depend on colour: the plainest theme builds them.
    let theme = Theme::from_config(&TuiConfig::default(), true, ColorDepth::Ansi16);
    let (controls, tone) = panes_of(
        Rect { height: 1, ..panes },
        app.profile.eq.is_some(),
        tier,
        app.tui.device_split,
    );
    let kept = (
        app.drawn.controls_switch.get(),
        app.drawn.controls_pending.get(),
    );
    let lines = controls_lines(app, &theme, room(controls, tier), tier, spaced, true)
        .lines
        .len();
    app.drawn.controls_switch.set(kept.0);
    app.drawn.controls_pending.set(kept.1);
    let tone = match (app.profile.eq.as_ref(), tone) {
        (Some(eq), Some(area)) => {
            let inner = tier.block().inner(area);
            let size = (usize::from(inner.width), usize::MAX);
            tone_lines_in(app, eq, &theme, size).lines.len()
        }
        _ => 0,
    };
    let frame = 2 + tier.top();
    u16::try_from(lines.max(tone))
        .unwrap_or(u16::MAX)
        .saturating_add(frame)
}

/// The rows the Status pane grows by in a frame `area` whose body is `body` tall below the
/// header, for a pane of `base` rows with `gap` rows over it: the rows the Controls and Tone
/// panes do not draw in (each pane's lines, its groups a blank line apart, with its borders),
/// held to the height the picture can use at the width the words leave it. Where the blanks
/// would leave the cover under [`USEFUL_ROWS`], the Controls pane's packed lines are the floor.
/// Nothing below [`GROW_COLS`] by [`GROW_ROWS`] or with no cover on show.
pub(super) fn growth(area: Rect, body: Rect, tier: Tier, base: u16, gap: u16, app: &App) -> u16 {
    if area.width < GROW_COLS || area.height < GROW_ROWS {
        return 0;
    }
    let Some((cell_w, cell_h)) = wanted(app) else {
        return 0;
    };
    // The interior the picture's width allows, and what the pane's own rows already have.
    let inner = tier.block().inner(Rect {
        height: base,
        ..body
    });
    let trail = u16::try_from(tier.trail()).unwrap_or(u16::MAX);
    let room = inner.width.saturating_sub(trail + GAP + MIN_WORDS);
    let tall = u32::from(room) * u32::from(cell_w) / u32::from(cell_h);
    let tall = u16::try_from(tall)
        .unwrap_or(u16::MAX)
        .saturating_sub(inner.height);
    let free = body.height.saturating_sub(gap + base);
    let by = |spaced: bool| {
        free.saturating_sub(panes_need(app, tier, body, spaced))
            .min(tall)
    };
    let spaced = by(true);
    if inner.height + spaced >= USEFUL_ROWS {
        spaced
    } else {
        by(false)
    }
}

/// The most picture the Status pane at `status` (in a frame of `tier`) can take beside its rows
/// for `app`: its interior height by the columns that make a square in the terminal's cells,
/// held to what leaves the words [`MIN_WORDS`]. `None` with no cover to show, art off, no
/// `Now playing` row to be beside, or no room.
pub(super) fn box_in(status: Rect, tier: Tier, app: &App) -> Option<Size> {
    let (cell_w, cell_h) = wanted(app)?;
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
    wanted(app)?;
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
    app.host.cover.picture.as_ref()?;
    let (tier, [_, _, status]) = rows_of(area, app);
    slot_in(status, tier, app).map(|(slot, _)| slot)
}

/// The cells the `Now playing` row keeps for the track's words beside any picture, whatever its
/// size: the box leaves the Status pane's rows [`MIN_WORDS`] columns, the row's label among them.
pub(crate) fn words_beside_art() -> usize {
    usize::from(MIN_WORDS)
        .saturating_sub(crate::tui::widgets::text::cells(super::status::NOW_PLAYING))
}

/// The columns the Now playing row's words have less of for the art `app` shows at its size,
/// which the marquee that carries long words is worked out against.
pub(crate) fn art_reserved(app: &App) -> usize {
    if app.host.cover.picture.is_none() {
        return 0;
    }
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
