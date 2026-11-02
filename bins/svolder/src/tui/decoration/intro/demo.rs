// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which demo effect builds the mark, and that effect planned for a frame size.
//!
//! A new effect is a variant of [`Demo`] and of [`Scene`], its own module beside `particles`,
//! a word for `[tui] intro_effect`, and a preview page; `random` picks among every variant.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use svolder_ipc::IntroEffect;

use crate::tui::theme::Theme;

use super::pace::Pacer;
use super::particles::{self, Build};
use super::reform::Reform;
use super::sparks::Colours;

/// A demo effect: one that builds the mark, as the fade does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui) enum Demo {
    /// Sparks thrown in from the edges.
    Particles,
}

impl Demo {
    /// Every demo effect, which `random` picks among.
    pub(in crate::tui) const ALL: [Demo; 1] = [Demo::Particles];

    /// The demo effect `effect` asks for, picked from `seed` for `random`; none for the fade.
    pub(in crate::tui) fn pick(effect: IntroEffect, seed: u64) -> Option<Demo> {
        match effect {
            IntroEffect::Fade => None,
            IntroEffect::Particles => Some(Demo::Particles),
            IntroEffect::Random => Some(Demo::ALL[(seed % Demo::ALL.len() as u64) as usize]),
        }
    }

    /// How long it lasts, in seconds.
    pub(in crate::tui) fn length(self) -> f32 {
        match self {
            Demo::Particles => particles::LENGTH,
        }
    }
}

/// A demo effect planned for one frame size and seed.
#[derive(Debug)]
pub(in crate::tui) enum Scene {
    /// The sparks' build of the mark.
    Particles(Build),
}

impl Scene {
    /// `demo` planned for a frame `area` from `seed`; `None` for a frame too small for it.
    pub(in crate::tui) fn plan(demo: Demo, area: Rect, seed: u64) -> Option<Scene> {
        match demo {
            Demo::Particles => Build::new(area, seed).map(Scene::Particles),
        }
    }

    /// Paint it `t` seconds in over `buf`, a blank frame of the size it was planned for.
    pub(in crate::tui) fn frame(&self, t: f32, buf: &mut Buffer, theme: &Theme, colours: &Colours) {
        match self {
            Scene::Particles(build) => build.frame(t, buf, theme, colours),
        }
    }

    /// The exit this effect hands the loop, planned for the frame it finished on.
    pub(in crate::tui) fn exit(&self, seed: u64, colours: Colours, pacer: Pacer) -> Option<Reform> {
        match self {
            Scene::Particles(build) => Some(Reform::new(build.area(), seed, colours, pacer)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_never_picks_the_fade_and_each_word_picks_its_own() {
        for seed in 0..100 {
            assert_eq!(Demo::pick(IntroEffect::Random, seed), Some(Demo::Particles));
        }
        assert_eq!(Demo::pick(IntroEffect::Fade, 0), None);
        assert_eq!(Demo::pick(IntroEffect::Particles, 0), Some(Demo::Particles));
    }

    /// What a resize asks the driver for: the effect planned afresh at the size the frame has
    /// become, or nothing where it has no room.
    #[test]
    fn a_scene_is_planned_for_the_size_it_is_asked_for() {
        for (w, h) in [(80, 24), (120, 40)] {
            let area = Rect::new(0, 0, w, h);
            match Scene::plan(Demo::Particles, area, 1) {
                Some(Scene::Particles(build)) => assert_eq!(build.area(), area),
                None => panic!("{w}x{h} has room for the large mark"),
            }
        }
        assert!(Scene::plan(Demo::Particles, Rect::new(0, 0, 60, 20), 1).is_none());
    }
}
