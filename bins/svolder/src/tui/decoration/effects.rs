// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Animation layer drawn over the screens after the widgets finish: a polish pass, never the
//! only carrier of a fact. A frame's text is identical with effects on or off, so a snapshot
//! test compares that text regardless; the one exception is the squirrel, which a screen's way
//! in keeps out of sight until the panes it stands on are in ([`Effects::hold_back`]).
//!
//! Effects: under the trace transition, a pane's frame traces itself round as it appears, else
//! a sweep plays on screen change; a red flash then a slow pulse on the header while a fault
//! stands; a fade-in for readings after a reconnect; an accent glow on a row whose value just
//! changed; a grey trail on the row the cursor left; a bright band across a row the user just
//! changed, repeating at wider spacing while it rests ([`REMINDERS`]); and a light running once
//! round the contour of the frame the keys just moved to, faster and brighter the louder what
//! is playing is.
//!
//! Two independent drifts run alongside these, both following the level the app reads (the
//! device's meters while they stream, else the capture of the host's sink): spectrum-drift
//! carries the tone chart's spectrum round the hue circle, and hue-drift carries the glow along
//! the palette's spectrum. The tone chart and the theme read the current drift each frame.
//!
//! [`spectrum`] is not this layer's: the tone curve's colouring by frequency is the curve's own
//! and is applied whether or not the layer runs; the layer only carries it round the hue circle.
//!
//! Effects off (the `[tui]` toggle, a terminal below true colour, or monochrome) disarms and
//! stops everything and the event loop falls back to its slow tick. Each animation also has its
//! own switch in [`EffectsConfig`], which [`Effects::configure`] applies live.
//!
//! The layer also plays the other screen-change transitions, noise and melt
//! ([`transition`](super::transition)), which answer to the toggle alone: both draw at every
//! colour depth and in monochrome.
//!
//! The code is split by concern: this file holds the layer itself ([`Effects`], its state, the
//! switches and [`Effects::render`]); `keyed` the per-event effects (sweep, trace, light, flash,
//! fade, glow, trail); `shimmer` the band across the changed row and its [`REMINDERS`]; `drift`
//! the spectrum and glow drifts and the curve's colouring; `passage` the noise and melt
//! transitions; `reform` the panes' reform after the intro.

use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use svolder_ipc::{EffectsConfig, Transition};
use tachyonfx::{Duration as FxDuration, Effect, EffectManager};

use super::intro::Reform;
use super::transition::Passage;
use crate::tui::mouse::Selected;
use crate::tui::render::FrameId;
use crate::tui::theme::HUE_CIRCLE;

mod drift;
mod keyed;
mod passage;
mod reform;
mod shimmer;

pub use drift::spectrum;
use drift::{HUE_PERIOD, LEVEL_EASE, LEVEL_EASE_SPAN, RIPPLE_REACH, RIPPLE_SWING};
pub(crate) use keyed::SWEPT_FROM;
use keyed::restore_backgrounds;
#[cfg(test)]
pub(crate) use keyed::{ARRIVAL_SPAN, perimeter};
#[cfg(any(test, doc))]
pub(crate) use shimmer::REMINDERS;
#[cfg(test)]
pub(crate) use shimmer::{SHIMMER_CELLS_PER_S, SHIMMER_FLOOR_MS, shimmer_ms};
#[cfg(test)]
use {
    drift::is_braille,
    keyed::{CURSOR_MS, RUNNER_LEN, VALUE_MS, lap_ms},
    ratatui::style::{Modifier, Style},
    ratatui::widgets::Block,
    shimmer::SHIMMER_BAND,
};

/// What one of a screen's ways in (an arrival, or a body's sweep) brings in, and for how much
/// longer, so the squirrel's cells can wait it out ([`Effects::hold_back`]).
#[derive(Debug, Default)]
struct WayIn {
    /// The frames an arrival traces, or the body a sweep brings in.
    areas: Vec<Rect>,
    /// How much longer it runs, counted as the effects count it; zero once it is over.
    left: Duration,
}

impl WayIn {
    /// Bring `areas` in over `span`, in place of whatever this way in was bringing in before.
    fn start(&mut self, areas: Vec<Rect>, span: Duration) {
        self.areas = areas;
        self.left = span;
    }

    /// Run on by `ticked`.
    fn advance(&mut self, ticked: Duration) {
        self.left = self.left.saturating_sub(ticked);
    }

    /// What it is still bringing in: nothing once it is over.
    fn areas(&self) -> &[Rect] {
        match self.left.is_zero() {
            true => &[],
            false => &self.areas,
        }
    }
}

/// Which animation a running effect is.
///
/// At most one effect runs per key: arming a key again replaces whatever was running under it,
/// so two screen changes in a row sweep once rather than sweeping over the first sweep.
///
/// The `Default` impl only satisfies [`EffectManager`]'s bound and is never read; every effect
/// is armed under the key it belongs to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum EffectKey {
    /// The sweep a screen change earns.
    #[default]
    Screen,
    /// The header's flash on a new fault.
    Fault,
    /// The slow pulse the header and the fault line keep while a fault stands.
    FaultPresence,
    /// The readings' fade-in after a reconnect.
    Reconnect,
    /// The glow on a row whose value has just changed.
    Value,
    /// The trail on the row the cursor left.
    Cursor,
    /// The bright band crossing the row the cursor is on.
    Shimmer,
    /// The frames a screen or a pane traces as it appears under the trace transition, and the
    /// content sweeping in behind them.
    Arrival,
    /// The light running round the contour of the frame the keys are in.
    Runner,
}

/// The animation layer over the screens.
pub struct Effects {
    /// Whether the frame the layer was last armed from drew a tone curve the spectrum's scroll
    /// moves: a curve that was drawn at all, and not a muted one.
    curve_scrolls: bool,
    /// Whether the terminal can show any of this: true colour, with colour on.
    capable: bool,
    /// Which animations the configuration asks for.
    config: EffectsConfig,
    /// How one screen gives way to the next; decides whether an arriving pane is traced round.
    transition: Transition,
    /// The animations running now, at most one per [`EffectKey`].
    manager: EffectManager<EffectKey>,
    /// The backgrounds of the frame as the widgets painted them, kept between frames so the
    /// restore after the animations allocates nothing.
    painted: Vec<Color>,
    /// The squirrel's cells and what the widgets drew in them, which the frame shows instead
    /// while a frame they lie in is arriving: set for each frame by [`Effects::hold_back`].
    behind: Vec<(Position, Cell)>,
    /// The frames the running arrival traces, for the squirrel to wait out.
    arriving: WayIn,
    /// The body a screen with no frame is sweeping in, for the squirrel to wait out.
    sweeping: WayIn,
    /// How far round the hue circle the tone curve's spectrum has drifted, in degrees.
    spectrum_hue: f32,
    /// How far through the ripple's own loop the glow is, `0.0..1.0` over one whole loop.
    ripple: f32,
    /// How loud what is playing was as of the last frame, or `None` where nothing reads a
    /// level.
    level: Option<f64>,
    /// The level eased frame by frame: what the glow and the runner's pace are worked out from.
    smoothed: Option<f64>,
    /// How far along the palette's spectrum the decoration is drawing, `-1.0..=1.0`.
    glow: f32,
    /// The areas the fault pulse runs over and the colour it pulses in, or `None` while nothing
    /// is faulted. Compared frame to frame so a standing fault is not re-armed (and restarted)
    /// on every frame.
    pulsing: Option<(Vec<Rect>, Color)>,
    /// Which frames the last drawn screen put on the terminal, by name. Compared against the
    /// next frame's own to tell a pane that just appeared from one already there.
    arrived: Vec<FrameId>,
    /// When the user last pressed a key, or `None` before they have. The reminder schedule is
    /// measured from this.
    keyed: Option<Instant>,
    /// How many of [`REMINDERS`] have played since that key.
    reminded: usize,
    /// Whether the user has acted since [`Effects::acted`] last answered.
    acted: bool,
    /// Which row of which list the keys selected as of the last frame, or `None` where none was
    /// drawn.
    selected: Option<(Selected, usize)>,
    /// The colours a screen sweep and a pane's trace come from: the theme's ground and its pane,
    /// else [`SWEPT_FROM`].
    swept: (Color, Color),
    /// The noise or the melt a screen change started, while it plays.
    passage: Option<Passage>,
    /// The last frame the terminal showed, kept while the melt is the transition and the
    /// layer's switch is on: the old screen the next screen change melts away.
    shown: Option<Buffer>,
    /// The intro's exit, while it plays over the loop's first frames.
    reform: Option<Reform>,
    /// The panes whose arrival trace waits for the exit to let them in.
    held: Vec<Rect>,
    /// Whether the light that follows the exit is armed.
    reform_lap: bool,
}

impl Effects {
    /// An animation layer over a terminal that can show it (`capable`), running the animations
    /// `config` switches on. A terminal that cannot show them makes the whole layer inert
    /// whatever the configuration says, and so does `effects = false`.
    pub fn new(capable: bool, config: &EffectsConfig) -> Effects {
        let mut effects = Effects {
            curve_scrolls: false,
            capable,
            config: EffectsConfig::off(),
            transition: Transition::default(),
            manager: EffectManager::default(),
            spectrum_hue: 0.0,
            ripple: 0.0,
            level: None,
            painted: Vec::new(),
            behind: Vec::new(),
            arriving: WayIn::default(),
            sweeping: WayIn::default(),
            smoothed: None,
            glow: 0.0,
            pulsing: None,
            arrived: Vec::new(),
            keyed: None,
            reminded: 0,
            acted: false,
            selected: None,
            swept: (SWEPT_FROM, SWEPT_FROM),
            passage: None,
            shown: None,
            reform: None,
            held: Vec::new(),
            reform_lap: false,
        };
        effects.configure(config);
        effects
    }

    /// Run the animations `config` switches on from here; affects the next event onward. A
    /// layer left switched off drops whatever was mid-animation, so nothing stays half-drawn
    /// and nothing keeps the loop redrawing fast. A terminal that cannot show the layer is not
    /// talked into it.
    ///
    /// Turning hue-drift off drops the eased level; [`Effects::render`] re-eases it from the raw
    /// reading on the next frame regardless of mode, since the runner's pace and colour also use
    /// it. An animation armed before that frame sees the level as it reads, not as it was eased.
    pub fn configure(&mut self, config: &EffectsConfig) {
        self.config = *config;
        if !self.config.hue_drift {
            self.smoothed = None;
        }
        if !self.config.enabled {
            self.passage = None;
            self.shown = None;
        }
        self.stop_unless_enabled();
    }

    /// Say whether the terminal can show the layer from here; re-answered on a change to colour
    /// depth or monochrome. Leaving it switched off drops whatever was mid-animation, as
    /// [`Effects::configure`] does: a terminal that can no longer show an animation is not left
    /// carrying one.
    pub fn set_capable(&mut self, capable: bool) {
        self.capable = capable;
        self.stop_unless_enabled();
    }

    /// Whether the terminal can show the layer at all, whatever the switches say.
    ///
    /// [`App::set_look`](crate::tui::app::App::set_look) derives the three app-drawn animations
    /// (the glide, the breathing borders and the peak marker) from this and the switches in one
    /// place, rather than each answering separately.
    pub fn capable(&self) -> bool {
        self.capable
    }

    /// Whether the animation layer runs at all: the `[tui]` switch, and the true colour and
    /// colour itself that were resolved at startup.
    pub fn enabled(&self) -> bool {
        self.capable && self.config.enabled
    }

    /// Whether an animation is running now; this is what keeps the loop redrawing fast. A layer
    /// that is off has nothing running whatever it once held, so the loop returns to its slow
    /// tick the moment the switches or the terminal turn the layer off.
    pub fn active(&self) -> bool {
        self.enabled() && self.manager.is_running()
    }

    /// Name the squirrel's cells in the frame about to be rendered, each with what the widgets
    /// drew there before it, so it comes in once the panes it stands on have drawn themselves:
    /// while the intro's exit holds a frame any of them lies in, or the way in (an arrival, or
    /// a body's sweep) brings one in, the frame shows those instead.
    pub fn hold_back(&mut self, behind: &[(Position, Cell)]) {
        self.behind.clear();
        self.behind.extend_from_slice(behind);
    }

    /// Advance every running animation by `last_tick` and paint it over the frame, drifting the
    /// spectrum's hue (spectrum-drift) and the glow along the palette's spectrum (hue-drift) by
    /// the same span of time.
    ///
    /// Runs after the widgets, so it changes how the frame's cells look, never what they say,
    /// but for the squirrel's while it waits ([`Effects::hold_back`]).
    pub fn render(&mut self, frame: &mut Frame, last_tick: Duration) {
        if !self.enabled() {
            self.stop();
            return;
        }
        let release = match self.reform.as_mut() {
            Some(reform) => {
                reform.advance(last_tick);
                !reform.holding_panes()
            }
            None => false,
        };
        if release {
            self.release_held();
        }
        // The squirrel waits out the panes it stands on, its feet on a contour included: the
        // exit's hold on them, then their way in, rather than coming in piece by piece with the
        // trace and the sweep. The wait counts the whole milliseconds the effects advance by,
        // so it ends on the frame they do.
        let ticked = Duration::from_millis(u64::from(FxDuration::from(last_tick).as_millis()));
        self.arriving.advance(ticked);
        self.sweeping.advance(ticked);
        let held = self.behind.iter().any(|(at, _)| {
            self.held
                .iter()
                .chain(self.arriving.areas())
                .chain(self.sweeping.areas())
                .any(|frame| frame.contains(*at))
        });
        if held {
            let buffer = frame.buffer_mut();
            for (at, under) in &self.behind {
                if let Some(cell) = buffer.cell_mut(*at) {
                    *cell = under.clone();
                }
            }
        }
        let turn = last_tick.as_secs_f32() / HUE_PERIOD;
        if self.config.spectrum_drift {
            self.spectrum_hue = (self.spectrum_hue + HUE_CIRCLE * turn) % HUE_CIRCLE;
        }
        // Fraction of the gap this tick closes, scaled from LEVEL_EASE's span to the actual
        // `last_tick`, so the real-time ease rate is the same at a fast tick as at a slow one.
        let ease = 1.0 - (1.0 - LEVEL_EASE).powf(last_tick.as_secs_f32() / LEVEL_EASE_SPAN);
        self.smoothed = self.level.map(|level| {
            self.smoothed
                .map_or(level, |held| held + f64::from(ease) * (level - held))
        });
        // At rest with hue-drift off; a slow ripple in the cool half with no signal; with one,
        // the level carries it toward the hot end with the ripple riding on top.
        self.glow = match self.config.hue_drift {
            false => 0.0,
            true => {
                self.ripple = (self.ripple + turn).fract();
                let wave = 0.5 - 0.5 * (self.ripple * std::f32::consts::TAU).cos();
                match self.smoothed {
                    None => RIPPLE_REACH * wave,
                    Some(level) => (level as f32 + RIPPLE_SWING * (wave - 0.5)).clamp(0.0, 1.0),
                }
            }
        }
        .clamp(-1.0, 1.0);
        let area = frame.area();
        // The sweeps fade a cell's background along with its text; no animation changes a
        // background, the theme's or the terminal's own, so those come back after the effects
        // have run.
        let running = self.manager.is_running();
        if running {
            self.painted.clear();
            self.painted
                .extend(frame.buffer_mut().content().iter().map(|cell| cell.bg));
        }
        self.manager
            .process_effects(FxDuration::from(last_tick), frame.buffer_mut(), area);
        if running {
            restore_backgrounds(frame.buffer_mut(), &self.painted);
        }
    }

    /// How loud the decoration is drawing at, `0.0..=1.0`, or `None` where nothing reads a
    /// level: the eased level the glow is worked out from, or the raw reading before any frame
    /// has eased it.
    fn loudness(&self) -> Option<f64> {
        self.smoothed.or(self.level)
    }

    /// Whether an effect over `area` runs at all: false with the layer off or an empty area.
    fn runs(&self, area: Rect) -> bool {
        self.enabled() && !area.is_empty()
    }

    /// Drop whatever is running, so an off layer holds nothing: no animation is left mid-frame,
    /// and nothing keeps [`Effects::active`] holding the loop's fast tick.
    ///
    /// The fault pulse is dropped too, areas and all, so the first frame after switching back on
    /// re-arms the pulse for a fault that still stands. The eased level is dropped as well: with
    /// nothing shown for a while, it restarts from whatever the level currently reads.
    fn stop(&mut self) {
        self.manager = EffectManager::default();
        self.pulsing = None;
        self.smoothed = None;
        self.reform = None;
        self.held.clear();
        self.arriving = WayIn::default();
        self.sweeping = WayIn::default();
    }

    /// Drop whatever is running if the layer is off, leave it alone if it is on. Re-answered on
    /// a switch or terminal-capability change.
    fn stop_unless_enabled(&mut self) {
        if !self.enabled() {
            self.stop();
        }
    }

    /// Run `fx` over `area` under `key`, replacing whatever was running under that key.
    fn arm(&mut self, key: EffectKey, area: Rect, fx: Effect) {
        self.manager.add_unique_effect(key, fx.with_area(area));
    }
}

#[cfg(test)]
mod tests;
