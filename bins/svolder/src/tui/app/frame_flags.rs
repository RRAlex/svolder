// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one-shot flags the frame loop drains: what changed since the last frame, for the
//! effects it earns.

use std::time::Instant;

use svolder_proto::FunctionId;

/// The one-shot flags the frame loop drains: what changed since the last frame, for the
/// effects it earns. [`FrameFlags::take`] hands them over in one move.
#[derive(Default)]
pub struct FrameFlags {
    /// The functions whose confirmed value has changed since the last frame, for the glow
    /// each of their rows earns.
    pub changed_rows: Vec<FunctionId>,
    /// The row the cursor has just left, for the trail it earns; `None` when it has not
    /// moved since the last frame took this.
    pub left_row: Option<FunctionId>,
    /// The row the user's own last key wrote to, for the sweep that answers it; `None` when
    /// the keys have written nothing since the last frame took this.
    pub touched_row: Option<FunctionId>,
    /// Whether the user's own last key wrote a value from a row no function names — a Settings
    /// row, a device on the list, a preset — for the sweep that answers it on whichever list
    /// has the keys; false when the keys have written none since the last frame took this.
    pub touched_cursor: bool,
    /// Whether the screen has changed since the last frame, for the sweep it earns.
    pub screen_changed: bool,
    /// Whether the user has moved to another screen since the last frame, for the noise or the
    /// melt it earns: raised by a key or a click that shows another screen, and by nothing
    /// else — the help overlay, a palette, a device switch and the setup wizard coming back
    /// raise [`FrameFlags::screen_changed`] alone. The run loop consumes it before the draw, for
    /// `Effects::on_navigation`, so the value [`FrameFlags::take`] drains never carries it.
    pub navigated: bool,
    /// Whether the user has moved the keys to another pane since the last frame, for the
    /// light that runs round the pane they landed in. Raised by what they did — `Tab`, a
    /// click on a pane's title — and by nothing a screen does to its own focus.
    pub pane_changed: bool,
    /// Whether the link has come back since the last frame, for the fade the readings earn.
    pub reconnected: bool,
    /// When a fault last went active, for the header flash it earns.
    pub pending_flash: Option<Instant>,
}

impl FrameFlags {
    /// Drain every flag in one move: the flags as they stand, with each left at its initial value
    /// until something raises it again.
    pub fn take(&mut self) -> FrameFlags {
        std::mem::take(self)
    }
}
