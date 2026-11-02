// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The row widgets a control screen is made of.
//!
//! Every widget draws one control's label and the value its last reading carried; a control
//! nothing has reported yet draws [`MISSING`] where its value goes. Nothing here decodes a
//! byte of its own: raw values come from the reading, converted values from the
//! [`ValueObject`](svolder_ipc::ValueObject) the backend built through the profile, and a bar
//! is drawn only for a control whose profile names the range it runs over.
//!
//! The rows of one pane share their columns, which [`rows::Columns::fit`] works out from the
//! pane's width so every value starts in the same place.
//!
//! The modules below hold the widgets by kind; this file keeps the sizes and marks more than one
//! of them draws with:
//!
//! - [`rows`]: the select, toggle, level and balance rows, their bar and their shared columns.
//! - [`meters`]: the amp meter rows: a bar over the profile's scale, with its peak and tick.
//! - [`actions`]: the words on a pane's top border that press keys, and their mouse targets.
//! - [`footer`]: the setup wizard's key line and the mouse targets of its keys.
//! - [`surfaces`]: the dimming, fading and grounds that boxes and screens are painted on.
//! - [`scroll`]: where a taller list's window starts, and the marks for the rows it hides.
//! - [`text`]: widths, padding, cutting, wrapping and typed-line windows, in terminal cells.
//! - [`graph`]: one-row braille history graphs of the readings.
//! - [`marquee`]: carries words too wide for their row past it.
//! - [`spinner`]: the glyph in front of a wait the TUI states.
//! - [`hyperlink`]: the Web UI box's link as a terminal hyperlink.
//! - [`passphrase_row`]: the Passphrase row as a three-way stepper.

pub(super) mod actions;
pub(super) mod footer;
pub(super) mod graph;
pub(super) mod hyperlink;
pub(super) mod marquee;
pub(super) mod meters;
pub(super) mod passphrase_row;
pub(super) mod rows;
pub(super) mod scroll;
pub(super) mod spinner;
pub(super) mod surfaces;
pub(super) mod text;

/// What a row draws where a value would be, until a reading arrives.
pub const MISSING: &str = "—";
/// Columns [`rows::with_cursor`] puts in front of a row.
pub const CURSOR_WIDTH: usize = 3;
/// The most cells a level bar, or a balance track, is drawn across.
pub const BAR_MAX: usize = 15;
/// The most in the wide tier, whose Controls pane is wider.
pub const BAR_MAX_WIDE: usize = 25;
/// Cells the tone-control screen's header draws the loudness bar across; the device screen's
/// Tone pane draws it as wide as its Controls pane's bars.
pub const LOUDNESS_WIDTH: usize = 5;
/// What a meter marks the class-A tick with, on the cell its profile's threshold falls on, and
/// what the volume bar marks the loudest step a boosting tone curve stays clear of clipping at
/// with: a glyph, so the mark reads at every colour depth, on every palette and in monochrome.
const TICK: &str = "┃";
