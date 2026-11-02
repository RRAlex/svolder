// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Dragging a band's point on the tone-control screen's Response curve.
//!
//! A left press within about a cell of a band's point grabs that band; where two points are that
//! close, the band the cursor is on wins, and otherwise the nearer one. A press on the curve away
//! from every point grabs nothing. A right press grabs the same way, and away from every point it
//! grabs the band whose centre is nearest in log frequency. The grab focuses the band, as a click
//! on its row does.
//!
//! Dragging moves the band: a left drag puts its point where the pointer is (the column's
//! frequency on the curve's log axis, the row's gain on its dB axis), each axis on its own. The
//! frequency stays where the press found it until the pointer leaves the press's column, and the
//! gain until it leaves the press's row, so a drag straight across sets the frequency alone and
//! one straight up or down the gain alone. A right drag sets its Q from how far the pointer has
//! gone sideways since the press, one device step per cell, right widening the bell and left
//! narrowing it. Every value lands on the nearest device step inside the band's window and
//! ranges, written as a band edit like a key's ([`App::band_edit`]). A cell is a coarse step, so
//! the keys and the edit line stay the fine ones.
//!
//! A drag never queues a write per motion: while a write of one of the band's parameters is still
//! unanswered, the step the pointer asks of that parameter is held back (only the latest one
//! kept) and sent once the device answers, once the wait for it runs out, or at the release in
//! any case, which always writes where the pointer let go. The curve draws from the held step, so
//! the point follows the pointer before the device has confirmed anything.
//!
//! The drag lasts until its button is let go. `Esc`, a screen change, a box opening over the
//! screen and a device switch (which starts an app of its own) drop it too. A release the TUI
//! never saw (the button let go outside the window, or after mouse capture was turned off) is
//! made up for: the next press of that button, and capture going off, let go of the drag where
//! the pointer last dragged it, so a later click elsewhere never moves the band. A press of the
//! other button while a drag is held is ignored.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use svolder_profiles::tone::preset::{self, Plan};
use svolder_profiles::tone::response::{F_MAX, F_MIN};
use svolder_profiles::{EqParam, EqSpec};
use svolder_proto::FunctionId;

use super::{App, Intent};
use crate::tui::boxes::edit_line::{self, PARAMS};
use crate::tui::boxes::presets;
use crate::tui::screens::tone;

/// How far from a band's point a press still grabs it, in cells either way.
const REACH: u16 = 1;

/// What a drag on the curve does to the band it grabbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// A left drag: the point follows the pointer, across for the frequency and up and down for
    /// the gain.
    Move,
    /// A right drag: the band's Q, from how far the pointer has gone sideways since the press.
    Width,
}

impl Gesture {
    /// The mouse button that makes this gesture.
    pub fn button(self) -> MouseButton {
        match self {
            Gesture::Move => MouseButton::Left,
            Gesture::Width => MouseButton::Right,
        }
    }
}

/// A band's point held by a press on the curve, until its button is let go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurveDrag {
    /// The band grabbed, 0-based.
    pub band: usize,
    /// What the drag does to it.
    pub gesture: Gesture,
    /// The cell the press landed on, which a right drag measures how far it has gone from.
    pub from: (u16, u16),
    /// The band's frequency, gain and Q steps as the press found them — the steps they were on
    /// their way to — `None` for one the device has not reported: a left drag keeps the
    /// frequency and the gain on theirs until the pointer leaves the press's column or row, and
    /// a right drag counts its Q steps from its Q.
    pub start: [Option<u8>; 3],
    /// The column the press landed in, while the pointer has not left it since: until it does,
    /// a left drag keeps the band's frequency on its start, so a drag straight up or down moves
    /// the gain alone. `None` from the first motion out of it on.
    pub column: Option<u16>,
    /// The row the press landed in, while the pointer has not left it since: until it does, a
    /// left drag keeps the band's gain on its start, so a drag straight across moves the
    /// frequency alone. `None` from the first motion out of it on.
    pub row: Option<u16>,
    /// The step the pointer last asked of each of the band's parameters, in `[frequency, gain,
    /// Q]` order, that has not been written yet: held while a write of that parameter is still
    /// unanswered, the latest one kept, and `None` where nothing is held.
    pub held: [Option<u8>; 3],
    /// Whether the pointer has been dragged since the press: a press let go where it landed
    /// with no drag between writes nothing.
    pub moved: bool,
}

/// The band a press at `at` on `curve` grabs for `gesture`, or `None` for a press that grabs
/// nothing.
///
/// A band whose point is drawn within [`REACH`] cells of the press either way is grabbed; where
/// more than one is, the band the cursor is on wins, and otherwise the nearer one — the lower
/// band where two are as near. Away from every point a left press grabs nothing, and a right
/// press the band whose centre is nearest in log frequency.
pub(crate) fn grab(app: &App, curve: Rect, at: (u16, u16), gesture: Gesture) -> Option<usize> {
    let (x, y) = at;
    let near: Vec<(usize, u32)> = tone::points(app, curve)
        .into_iter()
        .filter_map(|(band, (px, py))| {
            let (dx, dy) = (px.abs_diff(x), py.abs_diff(y));
            (dx <= REACH && dy <= REACH)
                .then(|| (band, u32::from(dx).pow(2) + u32::from(dy).pow(2)))
        })
        .collect();
    if near.iter().any(|&(band, _)| band == app.tone.band) {
        return Some(app.tone.band);
    }
    if let Some(&(band, _)) = near
        .iter()
        .min_by_key(|&&(band, distance)| (distance, band))
    {
        return Some(band);
    }
    match gesture {
        Gesture::Move => None,
        Gesture::Width => tone::nearest_centre(app, curve, x),
    }
}

impl CurveDrag {
    /// Note that the pointer is at `at`: the press's column or row, once left, is left for good,
    /// the band's frequency or gain then following the pointer.
    fn pointer_at(&mut self, at: (u16, u16)) {
        if self.column != Some(at.0) {
            self.column = None;
        }
        if self.row != Some(at.1) {
            self.row = None;
        }
    }
}

/// Hold `band` for `gesture` from a press at `at`, focusing it as a click on its row does.
pub(in crate::tui) fn start(app: &mut App, band: usize, gesture: Gesture, at: (u16, u16)) {
    let step = |param| {
        app.profile
            .eq
            .as_ref()
            .and_then(|eq| eq.band_id(band, param))
            .and_then(|id| app.planned_raw(id))
            .and_then(|raw| u8::try_from(raw).ok())
    };
    let drag = CurveDrag {
        band,
        gesture,
        from: at,
        start: [
            step(EqParam::Frequency),
            step(EqParam::Gain),
            step(EqParam::Q),
        ],
        column: Some(at.0),
        row: Some(at.1),
        held: [None; 3],
        moved: false,
    };
    app.focus_band(drag.band);
    app.pointer.curve_drag = Some(drag);
}

/// Drag the held point to `at` on `curve`, and answer the writes that takes now: what a key's
/// band edit writes ([`App::band_edit`]), in the order a preset's recall writes a band in, and
/// nothing for a step the band is on, or on its way to, already. A parameter a write is still
/// unanswered for is not written now: its step is held instead, in place of any held before it,
/// for [`confirmed`] or [`release`] to send. Nothing without a drag held.
pub(in crate::tui) fn to(app: &mut App, curve: Rect, at: (u16, u16)) -> Vec<Intent> {
    let (Some(mut drag), Some(eq)) = (app.pointer.curve_drag.clone(), app.profile.eq.clone())
    else {
        return Vec::new();
    };
    drag.pointer_at(at);
    let mut now = [None; 3];
    let mut held = drag.held;
    for (slot, target) in targets(&eq, &drag, curve, at).into_iter().enumerate() {
        let (Some(target), Some(id)) = (target, eq.band_id(drag.band, PARAMS[slot])) else {
            continue;
        };
        held[slot] = None;
        if app.planned_raw(id) == Some(i64::from(target)) {
            continue;
        }
        match app.readings.pending.contains_key(&id) {
            true => held[slot] = Some(target),
            false => now[slot] = Some(target),
        }
    }
    drag.held = held;
    drag.moved = true;
    let band = drag.band;
    app.pointer.curve_drag = Some(drag);
    write(app, &eq, band, now)
}

/// The held steps whose parameter the device has answered every write of — or whose wait has
/// run out ([`App::tick`]) — written now, and nothing else: what the loop sends after every
/// reading and every tick while a drag is held.
pub(in crate::tui) fn confirmed(app: &mut App) -> Vec<Intent> {
    let (Some(drag), Some(eq)) = (app.pointer.curve_drag.clone(), app.profile.eq.clone()) else {
        return Vec::new();
    };
    let mut now = [None; 3];
    let mut held = drag.held;
    for (slot, step) in drag.held.into_iter().enumerate() {
        let (Some(step), Some(id)) = (step, eq.band_id(drag.band, PARAMS[slot])) else {
            continue;
        };
        if !app.readings.pending.contains_key(&id) {
            now[slot] = Some(step);
            held[slot] = None;
        }
    }
    if now == [None; 3] {
        return Vec::new();
    }
    if let Some(drag) = app.pointer.curve_drag.as_mut() {
        drag.held = held;
    }
    write(app, &eq, drag.band, now)
}

/// Let go of the point being dragged, the button released at a cell of the curve — `end`, the
/// curve's cells and the cell under the pointer, or `None` where there is no cell to read — and
/// answer the writes that leaves: where the pointer let go, and every step still held, written
/// now whether or not the device has answered the writes before them. With no cell, the drag is
/// let go where the pointer last dragged it: the steps it asked for there are written or held
/// already, and the held ones are written now. A press let go where it landed, with no drag
/// between, writes nothing.
pub(in crate::tui) fn release(app: &mut App, end: Option<(Rect, (u16, u16))>) -> Vec<Intent> {
    let (Some(mut drag), Some(eq)) = (app.pointer.curve_drag.take(), app.profile.eq.clone()) else {
        return Vec::new();
    };
    let mut last = drag.held;
    if let Some((curve, at)) = end
        && (drag.moved || at != drag.from)
    {
        drag.pointer_at(at);
        for (slot, target) in targets(&eq, &drag, curve, at).into_iter().enumerate() {
            if target.is_some() {
                last[slot] = target;
            }
        }
    }
    write(app, &eq, drag.band, last)
}

/// Let go of a drag whose release never reached the TUI, found by `event`: a press of the drag's
/// own button while the drag is still held is that button let go of first, somewhere the mouse
/// was not reported — outside the window, or over a terminal that lost it — so the drag is
/// released where the pointer last dragged it ([`release`] with no cell of its own), before the
/// press is read as a press of its own. Nothing for any other event, or with no drag held.
pub(in crate::tui) fn lost(app: &mut App, event: &MouseEvent) -> Vec<Intent> {
    let MouseEventKind::Down(button) = event.kind else {
        return Vec::new();
    };
    if app
        .pointer
        .curve_drag
        .as_ref()
        .is_none_or(|drag| drag.gesture.button() != button)
    {
        return Vec::new();
    }
    release(app, None)
}

/// What mouse capture turning off leaves a drag held: its button's release goes to the terminal
/// from then on, so the drag is let go where the pointer last dragged it, as a lost release is
/// ([`lost`]). Nothing while capture is on, or with no drag held.
pub(in crate::tui) fn capture_off(app: &mut App) -> Vec<Intent> {
    match app.mouse {
        true => Vec::new(),
        false => release(app, None),
    }
}

/// The raw step the tone curve draws function `id` at: the step the edit line previews for it
/// ([`edit_line::preview_raw`]), the step a drag holds back for it, which is where the pointer
/// put the point, and otherwise the one it is on its way to ([`App::planned_raw`]) — so the
/// curve shows where a band is going before the device has confirmed it, while the band table
/// reads what the device confirmed. The tone control's clipping peak is worked out from the
/// same steps.
pub(crate) fn drawn_raw(app: &App, id: FunctionId) -> Option<i64> {
    if let Some(raw) = edit_line::preview_raw(app, id) {
        return Some(raw);
    }
    let held = app.pointer.curve_drag.as_ref().and_then(|drag| {
        let eq = app.profile.eq.as_ref()?;
        let slot = PARAMS
            .iter()
            .position(|&param| eq.band_id(drag.band, param) == Some(id))?;
        drag.held[slot]
    });
    held.map(i64::from).or_else(|| app.planned_raw(id))
}

/// The steps a drag to `at` on `curve` asks of its band, as `[frequency, gain, Q]` with `None`
/// for a parameter it leaves alone.
///
/// A left drag asks for the frequency the column stands for on the log axis and the gain the row
/// stands for on the dB axis ([`tone::frequency_at`], [`tone::gain_at`]), each at the device step
/// nearest it that the chart still draws a point at ([`frequency_step`], [`gain_step`]): a
/// frequency outside the band's window is its window's end, and the row the chart draws 0 dB on
/// stands for 0 dB itself, so a point dragged back onto that line is flat again rather than a
/// fraction of a decibel off it. While the pointer has not left the column the press landed in,
/// the frequency is the one the press found, and likewise the gain for the press's row: a cell
/// stands for its own centre, and a band is seldom on it — nor is the press always on the
/// band's point, a cell off it grabbing it too — so the axis the pointer has not moved along is
/// kept where it was.
///
/// A right drag asks for the Q the press found, one step down for every cell the pointer has gone
/// right of the press and one up for every cell left of it — a lower Q is a wider bell — held
/// inside the steps the Q takes.
fn targets(eq: &EqSpec, drag: &CurveDrag, curve: Rect, at: (u16, u16)) -> [Option<u8>; 3] {
    match drag.gesture {
        Gesture::Move => [
            match drag.column {
                Some(_) => drag.start[0],
                None => frequency_step(eq, drag.band, tone::frequency_at(curve, at.0)),
            },
            match (drag.row, tone::on_zero_row(curve, at.1)) {
                (Some(_), _) => drag.start[1],
                (None, true) => Some(eq.gain_zero),
                (None, false) => Some(gain_step(eq, tone::gain_at(curve, at.1))),
            },
            None,
        ],
        Gesture::Width => {
            let right = i64::from(at.0) - i64::from(drag.from.0);
            let q = drag.start[2].and_then(|q| {
                u8::try_from((i64::from(q) - right).clamp(0, i64::from(EqSpec::STEP_MAX))).ok()
            });
            [None, None, q]
        }
    }
}

/// Band `band`'s frequency step nearest `hz`, inside its window, and inside the log axis the
/// chart draws: where the nearest lands just past [`F_MIN`] or [`F_MAX`], the step beside it on
/// the axis, so a point dragged to the end of the curve is still drawn there. `None` for a band
/// the tone control does not have.
fn frequency_step(eq: &EqSpec, band: usize, hz: f64) -> Option<u8> {
    let raw = eq.frequency_raw(band, hz)?;
    Some(match eq.frequency(band, raw)? {
        drawn if drawn < F_MIN => raw.saturating_add(1).min(EqSpec::STEP_MAX),
        drawn if drawn > F_MAX => raw.saturating_sub(1),
        _ => raw,
    })
}

/// The gain step nearest `db`, inside the gains the chart draws: where the nearest lands just
/// past the chart's lowest or highest gain ([`tone::GAIN_FLOOR_DB`], [`tone::GAIN_CEILING_DB`]),
/// the step beside it on the chart, so a point dragged to the bottom of the curve is still drawn
/// there rather than a hundredth of a decibel below it.
fn gain_step(eq: &EqSpec, db: f64) -> u8 {
    let raw = eq.gain_raw(db);
    match eq.gain_db(raw) {
        drawn if drawn < tone::GAIN_FLOOR_DB => raw.saturating_add(1).min(EqSpec::STEP_MAX),
        drawn if drawn > tone::GAIN_CEILING_DB => raw.saturating_sub(1),
        _ => raw,
    }
}

/// Write `targets` — `[frequency, gain, Q]` steps, `None` for one left alone — to band `band` as
/// a band edit: planned from the steps the band is on its way to ([`App::planned_raw`]), so a
/// step it is on or asked for already is not written again, and finished by [`App::band_edit`],
/// which in Flat mode switches the tone control to EQ after them.
fn write(app: &mut App, eq: &EqSpec, band: usize, targets: [Option<u8>; 3]) -> Vec<Intent> {
    let plan = Plan {
        writes: preset::band_plan(eq, |id| app.planned_raw(id), band, targets),
        clamped: Vec::new(),
    };
    let writes = presets::plan_intents(app, eq, &plan);
    app.band_edit(writes)
}

#[cfg(test)]
mod tests;
