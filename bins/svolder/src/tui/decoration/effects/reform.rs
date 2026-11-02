// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The reform of the panes after the intro's exit, and the light that follows it.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use svolder_ipc::Transition;
use tachyonfx::fx;

use super::keyed::{ARRIVAL_MS, SWEEP_MS, lap};
use super::{EffectKey, Effects};
use crate::tui::decoration::intro::{PANES_HELD_MS, Reform};
use crate::tui::render::Logo;

impl Effects {
    /// Take the intro's exit, to play over the loop's first frames; refused while the layer is
    /// off, where nothing it would paint could show.
    pub(in crate::tui) fn hand_over(&mut self, reform: Reform) {
        if self.enabled() {
            self.reform = Some(reform);
            self.held.clear();
            self.reform_lap = false;
        }
    }

    /// Whether the intro's exit is playing.
    pub fn reforming(&self) -> bool {
        self.reform.is_some()
    }

    /// How long the loop waits between frames while the exit plays: the intro's own pace.
    pub fn reform_period(&self) -> Option<Duration> {
        self.reform.as_ref().map(Reform::period)
    }

    /// Take what the frame just drawn under the exit cost, for its pace.
    pub fn pace(&mut self, cost: Duration) {
        if let Some(reform) = self.reform.as_mut() {
            reform.record(cost);
        }
    }

    /// End the intro's exit at once, as a key, a click or a resize does; any pane it still held
    /// starts its trace.
    pub fn end_reform(&mut self) {
        if self.reform.take().is_some() {
            self.release_held();
        }
    }

    /// Paint the exit over `buf`, the whole frame as drawn, flying into `logo`. Under trace,
    /// `frames` are held as bare pane grey until the panes' turn. The exit is dropped on the
    /// frame it ends on, and any pane it still held starts its trace.
    pub fn paint_reform(&mut self, buf: &mut Buffer, logo: Option<Logo>, frames: &[Rect]) {
        let Some(reform) = self.reform.as_mut() else {
            return;
        };
        let held = match self.transition == Transition::Trace && reform.holding_panes() {
            true => frames,
            false => &[],
        };
        reform.paint(buf, logo, held, self.swept.1);
        if !reform.playing() {
            self.reform = None;
            self.release_held();
        }
    }

    /// Whether the light that follows the exit is still to be armed: the exit has planned
    /// itself and the light is not yet on its way.
    pub fn wants_reform_lap(&self) -> bool {
        self.reform.as_ref().is_some_and(Reform::planned) && !self.reform_lap
    }

    /// Run the light once round `frame` in `glow` once the exit has played out, and, under
    /// trace, the panes' arrival with it. Gated as [`Effects::on_lap`] is.
    pub fn on_reform_lap(&mut self, frame: Rect, glow: Color) {
        if self.reform.is_none() {
            return;
        }
        self.reform_lap = true;
        if !self.runs(frame) || !self.config.frames {
            return;
        }
        let wait = self.reform_lap_wait();
        let light = lap(frame, self.loudness(), glow);
        let light = match wait {
            0 => light,
            wait => fx::delay(wait, light),
        };
        self.arm(EffectKey::Runner, frame, light);
    }

    /// How long the light waits, in milliseconds: until the exit has played out and, under
    /// trace, the panes' hold, arrival and sweep have too.
    pub(in crate::tui) fn reform_lap_wait(&self) -> u32 {
        let Some(reform) = self.reform.as_ref() else {
            return 0;
        };
        let panes = match self.transition {
            Transition::Trace => PANES_HELD_MS + ARRIVAL_MS + SWEEP_MS,
            _ => 0,
        };
        reform
            .remaining_ms()
            .max(panes.saturating_sub(reform.elapsed_ms()))
    }

    /// Start the arrival trace of every pane the exit held, replacing any arrival trace
    /// already running (one arrival at a time).
    pub(super) fn release_held(&mut self) {
        let held = std::mem::take(&mut self.held);
        self.arm_arrival(held);
    }
}
