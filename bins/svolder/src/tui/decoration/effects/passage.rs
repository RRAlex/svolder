// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The screen-change transitions the layer plays besides the sweep and the trace (noise and
//! melt), and the capture of the frame they play over.

use std::time::Duration;

use ratatui::buffer::Buffer;
use svolder_ipc::Transition;

use super::Effects;
use crate::tui::decoration::transition::Passage;
use crate::tui::theme::Theme;

impl Effects {
    /// Let one screen give way to the next as `transition` says from here. A transition already
    /// under way when this changes plays out (none lasts much more than half a second). Setting
    /// anything but melt drops the kept frame.
    pub fn set_transition(&mut self, transition: Transition) {
        self.transition = transition;
        if transition != Transition::Melt {
            self.shown = None;
        }
    }

    /// Keep `frame`, the whole of what the terminal just showed, for the next screen change's
    /// melt. Kept only while melt is the transition and the layer's switch is on, so a copy is
    /// made only where one will actually be melted.
    pub fn keep_shown(&mut self, frame: &Buffer) {
        match self.config.enabled && self.transition == Transition::Melt {
            true => match &mut self.shown {
                Some(kept) if kept.area == frame.area => kept.content.clone_from(&frame.content),
                _ => self.shown = Some(frame.clone()),
            },
            false => self.shown = None,
        }
    }

    /// Start, from its very start, the transition a user-driven screen change plays: the noise,
    /// or the melt of the last frame shown, when the `[tui]` word names one of them and the
    /// layer's switch is on.
    ///
    /// Unlike every other animation here, neither needs true colour: both draw at every depth
    /// and in monochrome, so the switch is all they ask. A second screen change while one is
    /// playing cuts the first short and starts the new one from scratch (a melt melts the frame
    /// the first one had reached); under any other word it cuts the first short and starts
    /// nothing — the trace is armed instead from the new screen's own frame
    /// ([`Effects::on_arrival`]).
    pub fn on_navigation(&mut self) {
        self.passage = match (self.config.enabled, self.transition) {
            (true, Transition::Noise) => Some(Passage::noise()),
            (true, Transition::Melt) => self.shown.take().map(Passage::melt),
            _ => None,
        };
    }

    /// Whether a noise or a melt is playing; like every other animation, this keeps the loop
    /// redrawing fast.
    pub fn transitioning(&self) -> bool {
        self.config.enabled && self.passage.is_some()
    }

    /// Paint the noise or melt under way over `buf` (the whole frame just drawn, with
    /// everything else already painted over it), `last_tick` on from the frame before.
    ///
    /// This is the one thing the layer draws that changes what a cell says, so it comes last and
    /// lasts only a fraction of a second: once played out, the frame is the screen exactly as
    /// the widgets drew it, and the passage is dropped on that same frame.
    pub fn paint_transition(&mut self, buf: &mut Buffer, theme: &Theme, last_tick: Duration) {
        let Some(passage) = self.passage.as_mut() else {
            return;
        };
        if !self.config.enabled || !passage.paint(buf, theme, last_tick) {
            self.passage = None;
        }
    }
}
