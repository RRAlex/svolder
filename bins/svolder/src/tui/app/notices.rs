// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the user is told: the toast, the advice and what its idle and fault lines count from.

use std::time::Instant;

use crate::tui::boxes::toast::Toast;

/// What the user is told: the toast, the advice and what its idle and fault lines count from.
pub struct Notices {
    /// What the last thing asked for came to, drawn as a toast over the bottom right corner
    /// until it ages out or is dismissed. Read through [`App::toast`](super::App::toast).
    pub(super) toast: Option<Toast>,
    /// When [`App::toast`](super::App::toast) was set, or for a wait last said again: what ages it
    /// out.
    pub(super) toast_at: Instant,
    /// The step the last failure named, which the advice line shows once no fault is standing
    /// in front of it. It outlives the toast: the words age out of the corner, the step stays
    /// until another failure replaces it.
    pub advice: Option<String>,
    /// When this device was opened: what the idle advice counts from while no fault has stood.
    pub opened_at: Instant,
    /// When the last standing fault cleared, if one has stood and cleared in this session.
    pub fault_cleared_at: Option<Instant>,
    /// How many times `a` has said it was all clear: which tip or quip comes next.
    pub advice_presses: usize,
    /// Whether `a` has shown the step the last failure named.
    pub step_shown: bool,
    /// Whether a fault's advice is being shown.
    pub show_advice: bool,
}

impl Notices {
    /// The notices of a device opened at `now`: no toast, no advice and no fault yet.
    pub fn new(now: Instant) -> Notices {
        Notices {
            toast: None,
            toast_at: now,
            advice: None,
            opened_at: now,
            fault_cleared_at: None,
            advice_presses: 0,
            step_shown: false,
            show_advice: false,
        }
    }
}
