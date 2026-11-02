// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The per-event effects: the sweep and the trace on a screen change, the light running round a
//! frame, the header's flash and pulse on a fault, the fade after a reconnect, and the glow and
//! trail on a row whose value changed or that the cursor left.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::widgets::Block;
use svolder_ipc::Transition;
use tachyonfx::{Effect, Interpolation, Motion, fx};

use super::shimmer::PRESENCE_MS;
use super::{EffectKey, Effects};
use crate::tui::decoration::intro::Reform;
use crate::tui::decoration::transition::{MELT_MS, NOISE_MS};
use crate::tui::render::FrameId;
use crate::tui::theme::{DIM_GREY, Surface, Theme};

/// How long the sweep on a screen change takes, in milliseconds.
pub(super) const SWEEP_MS: u32 = 300;
/// How many columns the sweep's leading gradient spans.
const SWEEP_GRADIENT: u16 = 10;
/// How ragged the sweep's front is. Zero is a straight edge, reading as one pass rather than
/// noise.
const SWEEP_RANDOMNESS: u16 = 0;
/// Colour the sweep and the trace bring cells in from where the theme paints no surface
/// (16 colours, monochrome, the theme background off): the dim grey, which reads on the
/// terminal's own ground whether that is light or dark, where black would not. Where the theme
/// paints a surface, the ground or the pane it sits on takes its place
/// ([`Effects::set_surfaces`]).
pub(crate) const SWEPT_FROM: Color = DIM_GREY;
/// How long a frame takes to draw itself all the way round, in milliseconds.
pub(super) const ARRIVAL_MS: u32 = 300;
/// How long a whole arrival lasts: the frame drawing itself round, then its content sweeping
/// in behind.
pub(crate) const ARRIVAL_SPAN: Duration = Duration::from_millis((ARRIVAL_MS + SWEEP_MS) as u64);
/// How long a body's sweep lasts, for a screen with no frame to trace.
const SWEEP_SPAN: Duration = Duration::from_millis(SWEEP_MS as u64);
/// How long the light takes to run once round a frame with no level to follow, in
/// milliseconds: a slow turn, the pace of a screen at rest.
const LAP_SLOW_MS: u32 = 1_200;
/// How long that same lap takes with the level at the top of its scale, in milliseconds:
/// three times round in the time silence takes to go round once.
const LAP_FAST_MS: u32 = 400;
/// The shortest the light's band is, in cells of the contour: it covers at least this many
/// cells even where the head moves less between one frame and the next.
pub(super) const RUNNER_LEN: usize = 3;
/// How long the header's flash on a new fault takes, in milliseconds.
const FLASH_MS: u32 = 600;
/// How long the readings take to fade in after a reconnect, in milliseconds.
const RECONNECT_MS: u32 = 400;
/// How long a row glows after the value on it changed, in milliseconds.
pub(super) const VALUE_MS: u32 = 300;
/// How long the trail on the row the cursor left lasts, in milliseconds.
pub(super) const CURSOR_MS: u32 = 300;

impl Effects {
    /// Record the frames the screen is drawing now, and return the areas of the ones the
    /// previous draw did not: a pane just put on the terminal, or any frame of a screen that
    /// just came back into sight.
    ///
    /// Matching is by frame id, not position, so a resize moves every contour without
    /// announcing any of them. A screen drawn over rather than replaced (an overlay) is not
    /// passed here, so the frames under it stay recorded and are not re-announced when it lifts.
    /// A call reporting no frames clears the record. The loop makes that call for a terminal too
    /// small for any screen when no overlay is open, so growing back announces every frame
    /// again; with an overlay open it makes no call and the record stays, so shrinking and growing
    /// back under an overlay announces nothing.
    ///
    /// Called on every frame regardless of whether the layer runs, so what is reported is always
    /// the delta since the last draw, not since the last animation.
    pub fn appeared(&mut self, frames: &[(FrameId, Rect)]) -> Vec<Rect> {
        let new: Vec<Rect> = frames
            .iter()
            .filter(|(id, _)| !self.arrived.contains(id))
            .map(|(_, frame)| *frame)
            .collect();
        self.arrived = frames.iter().map(|(id, _)| *id).collect();
        new
    }

    /// Trace `frames` round from their top-left corners and sweep their content in behind, for
    /// panes a screen just drew; says whether anything was armed.
    ///
    /// Arms nothing for a screen that drew no frame, on a terminal the layer does not run in (a
    /// screen change there sweeps its body in instead), or under any transition but trace. The
    /// frames switch gates the runner only, not this.
    ///
    /// One arrival runs at a time: a frame appearing while another still traces takes over the
    /// key, and the previous one stops where it is.
    pub fn on_arrival(&mut self, frames: &[Rect]) -> bool {
        if self.transition == Transition::Trace
            && self.enabled()
            && self.reform.as_ref().is_some_and(Reform::holding_panes)
        {
            self.held
                .extend(frames.iter().copied().filter(|frame| !frame.is_empty()));
            return !self.held.is_empty();
        }
        let tracing = self.transition == Transition::Trace;
        let traced: Vec<Rect> = match tracing && self.enabled() {
            true => frames
                .iter()
                .copied()
                .filter(|frame| self.runs(*frame))
                .collect(),
            false => Vec::new(),
        };
        self.arm_arrival(traced)
    }

    /// Trace each of `frames` round and sweep its content in behind, as the one arrival,
    /// replacing whatever arrival was running; says whether there was a frame to trace.
    pub(super) fn arm_arrival(&mut self, frames: Vec<Rect>) -> bool {
        if frames.is_empty() {
            return false;
        }
        let traced: Vec<Effect> = frames
            .iter()
            .map(|frame| arrival(*frame, self.swept.1))
            .collect();
        // Each trace carries its own frame's area; the group holding them has none of its own.
        self.manager
            .add_unique_effect(EffectKey::Arrival, fx::parallel(&traced));
        self.arriving.start(frames, ARRIVAL_SPAN);
        true
    }

    /// Take the colours the sweep and the trace come from from `theme`: its ground for the
    /// sweep and its pane for the trace, or [`SWEPT_FROM`] for a surface it paints none of.
    /// Called every frame, before anything is armed, so a theme changed in Settings is followed.
    pub fn set_surfaces(&mut self, theme: &Theme) {
        let from = |surface| theme.surface(surface).unwrap_or(SWEPT_FROM);
        self.swept = (from(Surface::Ground), from(Surface::Pane));
    }

    /// Sweep `body` in from the left, for a frame showing a screen the one before it did not:
    /// the trace's own way in for a screen with no frame to draw round, which no other
    /// transition plays.
    pub fn on_screen_change(&mut self, body: Rect) {
        if self.transition != Transition::Trace || !self.runs(body) {
            return;
        }
        self.arm(
            EffectKey::Screen,
            body,
            fx::sweep_in(
                Motion::LeftToRight,
                SWEEP_GRADIENT,
                SWEEP_RANDOMNESS,
                self.swept.0,
                (SWEEP_MS, Interpolation::QuadOut),
            ),
        );
        self.sweeping.start(vec![body], SWEEP_SPAN);
    }

    /// Flash `header` red, for a fault the device has just started reporting.
    ///
    /// The fault row says the same thing without this, so a terminal the animation layer is
    /// off in loses nothing but the flash.
    pub fn on_fault(&mut self, header: Rect) {
        if !self.runs(header) {
            return;
        }
        self.arm(
            EffectKey::Fault,
            header,
            fx::fade_from_fg(Color::Red, FLASH_MS),
        );
    }

    /// Pulse `areas` slowly in `alert` for as long as the device keeps reporting a fault, and
    /// stop the moment it reports none (an empty `areas`).
    ///
    /// Called every frame with wherever the fault is shown (the header, and the Status pane's
    /// fault line where it draws one); a frame that repeats the last one's areas leaves the
    /// pulse running rather than restarting it. The rows name the fault in plain text
    /// throughout — this only makes them pulse.
    pub fn on_faults(&mut self, areas: &[Rect], alert: Color) {
        let rows: Vec<Rect> = match self.enabled() && self.config.fault_presence {
            true => areas
                .iter()
                .copied()
                .filter(|row| !row.is_empty())
                .collect(),
            false => Vec::new(),
        };
        let wanted = (!rows.is_empty()).then_some((rows, alert));
        if wanted == self.pulsing {
            return;
        }
        match &wanted {
            Some((rows, alert)) => {
                let pulses: Vec<Effect> = rows.iter().map(|row| pulse(*alert, *row)).collect();
                self.manager
                    .add_unique_effect(EffectKey::FaultPresence, fx::parallel(&pulses));
            }
            None => self.manager.cancel_unique_effect(EffectKey::FaultPresence),
        }
        self.pulsing = wanted;
    }

    /// Fade `body` up out of grey, for the readings that arrive after a link comes back.
    pub fn on_reconnect(&mut self, body: Rect) {
        if !self.runs(body) {
            return;
        }
        self.arm(
            EffectKey::Reconnect,
            body,
            fx::fade_from_fg(DIM_GREY, RECONNECT_MS),
        );
    }

    /// Glow `row` in `accent`, for a row whose value the device has just confirmed as
    /// changed: [`Effects::on_value_changes`] for one row.
    #[cfg(test)]
    pub fn on_value_change(&mut self, row: Rect, accent: Color) {
        self.on_value_changes(&[row], accent);
    }

    /// Glow each of `rows` in `accent`, for rows whose values the device has just confirmed as
    /// changed on the same frame: one glow per row, all together under the one key a glow runs
    /// under, so a row changing alongside another does not cut the other's glow short. A row
    /// listed more than once (a tone band's row, reported under each of its three functions)
    /// glows once. No row arms nothing, so a frame with no change leaves a glow still playing.
    ///
    /// The rows draw their new values in plain text on the same frame, so a terminal the
    /// animation layer is off in loses only the glow.
    pub fn on_value_changes(&mut self, rows: &[Rect], accent: Color) {
        if !self.config.eased {
            return;
        }
        let mut lit: Vec<Rect> = Vec::new();
        for row in rows {
            if self.runs(*row) && !lit.contains(row) {
                lit.push(*row);
            }
        }
        match lit.as_slice() {
            [] => {}
            [row] => self.arm(EffectKey::Value, *row, fx::fade_from_fg(accent, VALUE_MS)),
            rows => {
                let glows: Vec<Effect> = rows
                    .iter()
                    .map(|row| fx::fade_from_fg(accent, VALUE_MS).with_area(*row))
                    .collect();
                self.manager
                    .add_unique_effect(EffectKey::Value, fx::parallel(&glows));
            }
        }
    }

    /// Trail `row` out of grey, for the row the cursor has just left.
    pub fn on_cursor_move(&mut self, row: Rect) {
        if !self.runs(row) || !self.config.glow {
            return;
        }
        self.arm(
            EffectKey::Cursor,
            row,
            fx::fade_from_fg(DIM_GREY, CURSOR_MS),
        );
    }

    /// Run a light once round the contour of `frame` in `glow`, for the frame the keys just
    /// moved to.
    ///
    /// `glow` is the caller's own colour, worked out from the palette's spectrum at the stop
    /// [`Effects::level_position`] answers (the level itself where one streams, else the
    /// frame's drifted position), so the band is always a brighter form of the colour the
    /// contour is already drawn in. The layer's own loudness sets the pace: [`LAP_SLOW_MS`]
    /// round a silent screen, [`LAP_FAST_MS`] at the top of the level's scale.
    ///
    /// Runs nothing with the `frames` switch off (this light's own, independent of the
    /// transition), on a terminal the layer does not run in, or round an empty frame. One lap
    /// runs at a time: a frame lit while the light is still on its way round takes over, so
    /// repeated Tab presses leave one light running rather than a lap per press.
    pub fn on_lap(&mut self, frame: Rect, glow: Color) {
        if self.reform.is_some() || !self.runs(frame) || !self.config.frames {
            return;
        }
        self.arm(EffectKey::Runner, frame, lap(frame, self.loudness(), glow));
    }

    /// Run the light once round `frame` in `glow`, for the screen the user has just arrived on,
    /// once the way in has played out: the trace has drawn the frames, or the noise or the melt
    /// has cleared, so the light runs over a finished screen rather than under the transition.
    /// With no transition the light starts at once.
    ///
    /// Gated and replaced exactly as [`Effects::on_lap`] is.
    pub fn on_arrival_lap(&mut self, frame: Rect, glow: Color) {
        if self.reform.is_some() {
            return;
        }
        let wait = match self.transition {
            Transition::Trace => ARRIVAL_MS,
            Transition::Noise => NOISE_MS,
            Transition::Melt => MELT_MS,
            Transition::None => 0,
        };
        if !self.runs(frame) || !self.config.frames {
            return;
        }
        let light = lap(frame, self.loudness(), glow);
        let light = match wait {
            0 => light,
            wait => fx::delay(wait, light),
        };
        self.arm(EffectKey::Runner, frame, light);
    }
}

/// The border cells of `rect`, clockwise from its top-left corner (top, right side, bottom,
/// left), each cell exactly once. Used to trace a frame's arrival. An empty rect has no border;
/// one a single cell wide or tall is all border.
pub(crate) fn perimeter(rect: Rect) -> Vec<(u16, u16)> {
    if rect.is_empty() {
        return Vec::new();
    }
    let (left, right) = (rect.x, rect.right() - 1);
    let (top, bottom) = (rect.y, rect.bottom() - 1);
    let mut cells: Vec<(u16, u16)> = (left..=right).map(|x| (x, top)).collect();
    cells.extend((top + 1..=bottom).map(|y| (right, y)));
    if bottom > top {
        cells.extend((left..right).rev().map(|x| (x, bottom)));
    }
    if right > left {
        cells.extend((top + 1..bottom).rev().map(|y| (left, y)));
    }
    cells
}

/// Put back each background `painted` records, the terminal's own included: with no surface
/// painted, a sweep would otherwise leave its grey behind the text until its front passed.
pub(super) fn restore_backgrounds(buffer: &mut Buffer, painted: &[Color]) {
    for (cell, bg) in buffer.content.iter_mut().zip(painted) {
        cell.bg = *bg;
    }
}

/// One frame's arrival: its contour draws itself round from the top-left corner over
/// [`ARRIVAL_MS`], and the content inside sweeps in behind over [`SWEEP_MS`] once the contour is
/// closed.
///
/// The whole contour is on screen throughout (the widgets drew it); cells whose turn has not
/// come are painted in `from`, the pane the sweep brings content out of, so the frame
/// arrives out of the terminal rather than being written over it. The sweep is held at its own
/// start for as long as the trace runs, so content waits out of sight instead of appearing and
/// then being covered again. Every cell says exactly what the widgets put there throughout.
pub(super) fn arrival(frame: Rect, from: Color) -> Effect {
    let cells = perimeter(frame);
    let trace = fx::effect_fn_buf(
        cells,
        (ARRIVAL_MS, Interpolation::Linear),
        move |cells, ctx, buf| {
            let drawn = (ctx.alpha() * cells.len() as f32).ceil() as usize;
            for (x, y) in cells.iter().skip(drawn) {
                if let Some(cell) = buf.cell_mut((*x, *y)) {
                    cell.set_fg(from);
                }
            }
        },
    );
    let content = fx::sweep_in(
        Motion::LeftToRight,
        SWEEP_GRADIENT,
        SWEEP_RANDOMNESS,
        from,
        (SWEEP_MS, Interpolation::QuadOut),
    )
    .with_area(Block::bordered().inner(frame));
    fx::parallel(&[trace, fx::prolong_start(ARRIVAL_MS, content)])
}

/// How long one lap takes at `level`, in milliseconds: [`LAP_SLOW_MS`] with no level at
/// all, [`LAP_FAST_MS`] at the top of its scale, and evenly between the two.
pub(super) fn lap_ms(level: Option<f64>) -> u32 {
    let level = level.unwrap_or(0.0).clamp(0.0, 1.0) as f32;
    LAP_SLOW_MS - ((LAP_SLOW_MS - LAP_FAST_MS) as f32 * level) as u32
}

/// One lap of the light: a band of at least [`RUNNER_LEN`] cells travelling once round
/// `frame`'s contour in `glow`, at the pace `level` asks for.
///
/// The band enters at the top-left corner and runs off it again a band's length later. It is
/// drawn as a run of cells, not a point: the band itself plus every cell the head passed since
/// the last frame, so a fast lap on a long contour (which moves the head several cells per
/// frame) still lights every contour cell on some frame rather than leaving gaps. The head cell
/// is drawn bold, giving the band a direction. Nothing is left lit once the lap completes; the
/// frame after paints nothing.
///
/// Edge case: the completing call joins every remaining unlit cell at once rather than a band's
/// worth, so a lap whose last tick landed close to completion can finish in a wider flash than
/// the usual three-cell band. That same call makes [`Effects::active`] answer false, so the loop
/// can fall back to its slow tick before the widgets redraw over it — the flash can then sit on
/// the contour for up to one slow tick. This is harmless (the next redraw of any kind clears
/// it), so it is left rather than holding the fast tick open for an effect that already finished.
pub(super) fn lap(frame: Rect, level: Option<f64>, glow: Color) -> Effect {
    let cells = perimeter(frame);
    let ms = lap_ms(level);
    // The head position left by the previous frame; None on the lap's first frame.
    let passed: Option<usize> = None;
    fx::effect_fn_buf(
        (cells, passed),
        (ms, Interpolation::Linear),
        move |(cells, passed), ctx, buf| {
            let head = (ctx.alpha() * (cells.len() + RUNNER_LEN) as f32) as usize;
            let band = head.saturating_sub(RUNNER_LEN - 1);
            let from = match *passed {
                Some(last) => band.min(last + 1),
                None => 0,
            };
            *passed = Some(head);
            for at in from..=head {
                let Some((x, y)) = cells.get(at) else {
                    continue;
                };
                if let Some(cell) = buf.cell_mut((*x, *y)) {
                    cell.set_fg(glow);
                    if at == head {
                        cell.set_style(cell.style().add_modifier(Modifier::BOLD));
                    }
                }
            }
        },
    )
}

/// One row's share of the fault pulse: `row` carried into `alert` over [`PRESENCE_MS`] and back
/// out over the same again, repeating for as long as the fault stands.
///
/// The area is set on the pulse itself, not on what groups the pulses together, so each row
/// keeps its own (the header and the fault line are nowhere near each other on screen).
fn pulse(alert: Color, row: Rect) -> Effect {
    fx::repeating(fx::ping_pong(fx::fade_to_fg(
        alert,
        (PRESENCE_MS, Interpolation::SineInOut),
    )))
    .with_area(row)
}
