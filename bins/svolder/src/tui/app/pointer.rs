// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the mouse is doing: the drags it holds, the climb it started, its last press and the
//! tooltip it asks for.

use std::time::Instant;

use super::glide::{BarDrag, StepPace, VolumeClimb};
use crate::tui::app::curve_drag::CurveDrag;
use crate::tui::boxes::tooltip::Tooltip;

/// What the mouse is doing: the drags it holds, the climb it started, its last press and the
/// tooltip it asks for.
#[derive(Default)]
pub struct Pointer {
    /// The band's point a press on the tone-control screen's curve holds, until its button is
    /// let go: dropped by `Esc`, a screen change and a box opening over the screen as well, and
    /// never carried over to another device, whose app starts without one.
    pub curve_drag: Option<CurveDrag>,
    /// The level bar a left press holds until its button is let go
    /// ([`glide`](super::glide)).
    pub bar_drag: Option<BarDrag>,
    /// The Volume climbing toward a target above what it may jump
    /// to ([`glide`](super::glide)).
    pub climb: Option<VolumeClimb>,
    /// The pace one-notch raises of the Volume keep above three quarters of its range, from the
    /// keys and the wheel alike ([`glide`](super::glide)).
    pub(crate) step_pace: Option<StepPace>,
    /// The cell and the moment of the last left press, for a second press on that same cell soon
    /// enough after it to make a double-click
    /// ([`mouse::note_press`](super::super::mouse::note_press)):
    /// `None` before the first, and after a double-click, so a third press starts over.
    pub last_press: Option<((u16, u16), Instant)>,
    /// Whether the left press being read is the second of a double-click, as
    /// [`mouse::note_press`](super::super::mouse::note_press) found it before the press is
    /// resolved.
    pub double_click: bool,
    /// The tooltip the pointer's resting asks for, pending or showing
    /// ([`tooltip`](crate::tui::boxes::tooltip)).
    pub tooltip: Option<Tooltip>,
    /// Whether the pointer rests on the Web UI box's link, as the last frame's hits have it: the
    /// box draws every line of the link underlined while it does.
    pub over_link: bool,
}
