// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the loop draws and applies on top of the screens, by topic: settings changes, the tick
//! period, the animation layer and the passages between screens. The helpers here are the ones
//! several topics use; the tests are in the topic files listed below.

use crossterm::event::KeyCode;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use serde_json::json;
use svolder_ipc::config::{
    HOST_AUDIO, INTRO, INTRO_SOUND, MONOCHROME, THEME_BACKGROUND, TITLE_METER, TRANSITION,
    VISUALIZER,
};
use svolder_ipc::{DeviceConfig, EffectsConfig, Transition, TuiValue, ValueObject};
use svolder_profiles::{Family, Registry};

use crate::backend::{DeviceRow, Update};
use crate::tui::app::{EffectSwitch, Pane, ToneRow};
use crate::tui::decoration::effects::{ARRIVAL_SPAN, REMINDERS, SWEPT_FROM, shimmer_ms};
use crate::tui::mouse::{HitMap, Selected, Selection};
use crate::tui::palette;
use crate::tui::render::{self, FrameAreas, FrameId, Logo};
use crate::tui::test_support::{fg, with_range_on};

use super::super::test_support::*;
use super::super::*;
use super::*;

// settings changes, their screen and file
mod settings;
// palette, colour depth and the red palette
mod theme;
// the tick period
mod ticks;
// the scrolling spectrum
mod spectrum;
// the glow on the contours, goldens
mod glow;
// frame trace and the runner light
mod runner;
// the shimmer, glow and trail armed per frame
mod shimmer;
// the noise and the melt
mod transitions;
// the exit and the squirrel
mod mascot;

/// Carry out one settings change against `look` and hand back what the loop is watching.
async fn change(look: &mut Look, watching: Watching, change: TuiChange) -> Watching {
    let mut mouse = SpyMouse::default();
    let flow = take_intent(
        Intent::Tui(change),
        &opener_over(never_opens()),
        watching,
        &mut mouse,
        look,
        &mut (),
    )
    .await;
    match flow {
        Flow::Continue(watching) => watching,
        Flow::Adding(_) | Flow::Ended(_) => {
            panic!("a settings change neither ends the loop nor adds a device")
        }
    }
}

/// A look that draws in full colour, which is what a case about the theme or about the
/// animation layer's capability needs.
fn colourful_look() -> Look {
    Look::new(
        TuiConfig::default(),
        false,
        ColorDepth::TrueColor,
        scratch_config(),
    )
}

/// A capable animation layer for the no-colour theme [`animated`] draws on. It ignores
/// `shows_effects` on purpose: that rule would turn the layer off on a screen with no colour,
/// and the cases that draw there measure what the layer paints against it.
fn capable_over_no_colour(config: &EffectsConfig) -> Effects {
    Effects::new(true, config)
}

/// One frame of `app` drawn with whatever `effects` is running over it, the way the loop
/// draws one: the widgets first, then the animation layer.
///
/// The screen is drawn in no colour at all, so every cell an animation painted stands out
/// against it.
fn animated(app: &App, effects: &mut Effects, last_tick: Duration) -> Buffer {
    animated_with(app, effects, &Theme::new(true), last_tick)
}

/// The same on `theme`, for a case about how an animation's own colour reads against the
/// colour the screen underneath was drawn in.
fn animated_with(app: &App, effects: &mut Effects, theme: &Theme, last_tick: Duration) -> Buffer {
    debug_assert!(
        theme.monochrome() || shows_effects(theme),
        "the layer never shows at this depth"
    );
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, app, theme, &mut hits);
            effects.render(frame, last_tick);
        })
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

/// Where one frame of `app` put its rows, as the loop reads them back off the frame it
/// just drew.
fn row_areas(app: &App) -> FrameAreas {
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|frame| areas = render::draw(frame, app, &Theme::new(true), &mut hits))
        .expect("the frame draws");
    areas
}

/// The Gungnir 2 the animation tests watch, with no device to switch to.
fn gungnir() -> App {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("gungnir2-v2 is built in")
        .clone();
    App::new(profile, "gungnir".into(), &TuiConfig::default(), Vec::new())
}

/// Let whatever the layer is running finish, so that what it is running after the next
/// frame is what that frame armed: comfortably longer than the shimmer's own crossing
/// ([`shimmer_ms`] of the whole screen's width), the longest burst anything here arms on its
/// own.
fn run_out(effects: &mut Effects) {
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    terminal
        .draw(|frame| {
            effects.render(
                frame,
                Duration::from_millis(u64::from(shimmer_ms(SIZE.0)) + 500),
            )
        })
        .expect("the frame draws");
    assert!(!effects.active(), "an animation outlasted its own burst");
}

/// One turn of the loop over `app` in a fresh terminal `size`, taken through the seam the
/// README screenshots draw through: the frame that came out.
fn turn(app: &mut App, look: &mut Look, size: (u16, u16), last_tick: Duration) -> Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(size.0, size.1)).expect("a test terminal opens");
    drawn_turn(&mut terminal, app, look, Instant::now(), last_tick).0
}

/// What `app` looks like drawn with no passage over it: the screen alone.
fn bare(app: &App, look: &Look, size: (u16, u16)) -> Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(size.0, size.1)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, app, &look.theme, &mut hits);
        })
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

/// How many cells of `buffer` are one of the noise's shade glyphs.
fn noisy(buffer: &Buffer) -> usize {
    buffer
        .content()
        .iter()
        .filter(|cell| ["▓", "▒", "░"].contains(&cell.symbol()))
        .count()
}

/// The frame `areas` put under `id`.
fn frame_of(areas: &FrameAreas, id: FrameId) -> Rect {
    areas
        .frames
        .iter()
        .find(|(frame, _)| *frame == id)
        .map(|(_, rect)| *rect)
        .expect("the frame is drawn")
}
