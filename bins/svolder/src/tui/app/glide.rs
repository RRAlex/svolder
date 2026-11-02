// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Level bars that take the mouse, and the volume's glide.
//!
//! A left press on a level's bar (the Controls pane's Volume and Balance, the Loudness bar on the
//! Tone pane and in the tone-control screen's header) focuses its row and sets the value the
//! pressed cell stands for ([`value_at`]); dragging goes on setting the value under the pointer,
//! clamped at both ends, wherever the pointer is, until the button is let go ([`BarDrag`]).
//!
//! A bar drag paces its writes as the tone curve's drag does ([`super::curve_drag`]): while a write
//! to the function is unanswered ([`super::Readings::pending`]), the newest step is held, only
//! the latest kept, and sent once the device answers or the wait runs out ([`confirmed`]); the
//! release always writes the value it lets go on.
//!
//! The Volume never jumps loudly. A raise above [`safety::SAFE_FRACTION`] of its range goes only
//! through a [`VolumeClimb`], whose base is what the device last confirmed: the volume is written
//! up to [`safety::SAFE_FRACTION`] at once, then climbs one raw step above the last Volume reading
//! the device sent, never faster than a tenth of the range per second ([`App::tick`] pays out the
//! credit for each step). The clock gives no head start: while the device is silent the credit
//! stops at a step and a half, and a late answer resumes the climb from the answered value, one
//! step above it. A Volume step above [`safety::SAFE_FRACTION`] and above the reading is never held
//! for later: it becomes, or steers, the climb's target. Every path that writes a held step (an
//! answer, the wait running out, a key, the release) judges it again against the reading and writes
//! nothing that breaks the rule. Lowering, and raising to [`safety::SAFE_FRACTION`] or below, is
//! instant. Any key stops the climb where it is, a lower click or drag clears it, and a reading
//! that is not the last step written ends it (the volume was changed from somewhere else, or an
//! older value arrived late, and the climb does not carry on from a value it did not write).

use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use svolder_profiles::ControlKind;
use svolder_profiles::safety::{self, Climb, Motion, Rules, Situation, Step};
use svolder_proto::FunctionId;

use super::{App, Intent, LOUDNESS_MAX, Pane};
use crate::tui::mouse::Hit;

/// Which level bar a hit or a drag is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    /// The bar of this row of the Controls pane, an index [`Hit::Level`] shares.
    Row(usize),
    /// The tone control's Loudness bar, wherever it is drawn.
    Loudness,
}

impl Bar {
    /// The hit the bar's cells are drawn as.
    pub(crate) fn hit(self) -> Hit {
        match self {
            Bar::Row(row) => Hit::LevelBar(row),
            Bar::Loudness => Hit::LoudnessBar,
        }
    }
}

/// How a bar draws a value, which is how a cell is read back as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Model {
    /// A bar filled from the left: a cell stands for the value at its middle ([`value_at`]).
    Fill,
    /// A track with a marker on one cell, as Balance draws: the cells run from the range's
    /// minimum to its maximum ([`marker_at`]).
    Marker,
}

/// What a bar sets: the function, its range and how it is written.
struct Spec {
    /// The function the bar's value is read from and written to.
    id: FunctionId,
    /// The lowest and highest raw value.
    range: (i64, i64),
    /// How the bar draws a value.
    model: Model,
    /// Whether this is the Volume, whose raises glide.
    volume: bool,
    /// The key a write names the control by; `None` for the Loudness, which the tone control
    /// writes ([`App::loudness_intent`]).
    key: Option<String>,
}

/// A bar held by a press, until its button is let go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarDrag {
    /// The bar being dragged.
    pub(in crate::tui) bar: Bar,
    /// The function it sets.
    id: FunctionId,
    /// The step the pointer asked of it last that has not been written yet: held while a write
    /// to the function is unanswered, the latest one kept.
    held: Option<i64>,
    /// The value the pointer last asked for, which a release lets go on where it has no cell of
    /// its own.
    last: Option<i64>,
}

/// The Volume climbing to a target it is not allowed to jump to.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeClimb {
    /// The Volume's function.
    id: FunctionId,
    /// The climb: its target, its credit and the step it last wrote.
    climb: Climb,
    /// The moment the credit was last paid up to.
    at: Instant,
}

impl VolumeClimb {
    /// The raw value the climb is heading for.
    pub(crate) fn target(&self) -> i64 {
        self.climb.target()
    }

    /// The function the climb is on.
    pub(crate) fn id(&self) -> FunctionId {
        self.id
    }

    /// Pay out the credit of the time up to `now` along a climb of `span` raw steps of range.
    fn advance(&mut self, now: Instant, span: i64) {
        self.climb
            .advance(now.saturating_duration_since(self.at), span);
        self.at = now;
    }
}

impl App {
    /// Move the Volume's climb on to `now`, along the range the profile gives the Volume.
    pub(super) fn advance_climb(&mut self, now: Instant) {
        let span = self
            .volume()
            .and_then(|volume| volume.range)
            .map_or(0, |(min, max)| max - min);
        if let Some(climb) = self.pointer.climb.as_mut() {
            climb.advance(now, span);
        }
    }

    /// The raw value the bar of function `id` draws at while a drag holds it, with no easing: the
    /// value the pointer asked for, or for the Volume climbing, the step last asked of the device
    /// (the target is drawn dim beside it). `None` when no drag holds the bar.
    pub(crate) fn dragged_raw(&self, id: FunctionId) -> Option<i64> {
        let drag = self
            .pointer
            .bar_drag
            .as_ref()
            .filter(|drag| drag.id == id)?;
        match self.pointer.climb.as_ref().filter(|climb| climb.id == id) {
            Some(climb) => self.planned_raw(climb.id),
            None => drag.last,
        }
    }

    /// Whether the Volume is climbing at `now`, which keeps the loop on its fast tick.
    pub fn climbing(&self) -> bool {
        self.pointer.climb.is_some()
    }
}

/// The value the middle of the cell at column `cell` stands for on a bar drawn across `bar`
/// that fills from `min` to `max`, as the nearest raw value held inside the range: a column left of
/// the bar is `min` and one right of it `max`.
fn value_at(cell: u16, bar: Rect, min: i64, max: i64) -> i64 {
    let width = i64::from(bar.width);
    let span = max.saturating_sub(min);
    if width == 0 || span <= 0 || cell < bar.x {
        return min;
    }
    let at = i64::from(cell - bar.x);
    if at >= width {
        return max;
    }
    // The middle of cell `at` is (2 * at + 1) / (2 * width) of the way along, rounded to the
    // nearest raw step.
    let twice = (2 * at + 1).saturating_mul(span);
    let raw = min + (twice + width) / (2 * width);
    raw.clamp(min, max)
}

/// The value the cell at column `cell` stands for on a track drawn across `bar` that puts a
/// marker on one cell, the first for `min` and the last for `max` (Balance's): the first step
/// whose marker is drawn on the cell, held inside the range.
fn marker_at(cell: u16, bar: Rect, min: i64, max: i64) -> i64 {
    let width = i64::from(bar.width);
    let span = max.saturating_sub(min);
    if width <= 1 || span <= 0 || cell < bar.x {
        return min;
    }
    let at = i64::from(cell - bar.x).min(width - 1);
    let last = width - 1;
    // Rounded up: the marker is drawn at the floor of its step's place, so the step that puts it
    // on this cell is the first one that reaches it.
    let raw = min + (at * span + last - 1) / last;
    raw.clamp(min, max)
}

/// What the bar `bar` sets, or `None` for a bar the app has no control for.
fn spec(app: &App, bar: Bar) -> Option<Spec> {
    match bar {
        Bar::Row(row) => {
            let control = app.rows().get(row)?.control;
            let range = control.range?;
            (control.kind == ControlKind::Level).then(|| Spec {
                id: control.id,
                range,
                model: match control.center {
                    Some(_) => Model::Marker,
                    None => Model::Fill,
                },
                volume: app.volume().is_some_and(|volume| volume.id == control.id),
                key: Some(control.key.clone()),
            })
        }
        Bar::Loudness => Some(Spec {
            id: app.profile.eq.as_ref()?.loudness_id(),
            range: (0, LOUDNESS_MAX),
            model: Model::Fill,
            volume: false,
            key: None,
        }),
    }
}

/// What the held bar `drag` sets, or `None` for a bar that is not the control it was pressed on.
fn drag_spec(app: &App, drag: &BarDrag) -> Option<Spec> {
    spec(app, drag.bar).filter(|spec| spec.id == drag.id)
}

/// Whether `want` may be asked of the Volume now without the climb: at or below
/// [`safety::SAFE_FRACTION`] of the range, or not above the [`base`], the lower of the last reading
/// the device sent and the last value sent to it. Anything else is a raise that only the climb
/// makes, one step above the base at a time.
fn permitted(app: &App, spec: &Spec, want: i64) -> bool {
    safety::lands_at_once(spec.range, base(app, spec.id), want)
}

/// The value a raise of the Volume is judged from: the lower of the last reading and the last
/// value a write asked for, so a lowering not yet confirmed (or whose echo was lost) is never
/// undone by the older, louder reading.
fn base(app: &App, id: FunctionId) -> Option<i64> {
    let reading = app.raw_of(id)?;
    Some(
        app.readings
            .sent
            .get(&id)
            .map_or(reading, |&sent| sent.min(reading)),
    )
}

/// What the Volume sets, or `None` for a device without one.
fn volume_spec(app: &App) -> Option<Spec> {
    let control = app.volume()?;
    Some(Spec {
        id: control.id,
        range: control.range?,
        model: Model::Fill,
        volume: true,
        key: Some(control.key.clone()),
    })
}

/// The value column `col` of a bar drawn across `area` stands for.
fn value_for(spec: &Spec, area: Rect, col: u16) -> i64 {
    let (min, max) = spec.range;
    match spec.model {
        Model::Fill => value_at(col, area, min, max),
        Model::Marker => marker_at(col, area, min, max),
    }
}

/// The write that sets the bar's function to `raw`, noted as asked for ([`App::planned_raw`]).
fn write(app: &mut App, spec: &Spec, raw: i64) -> Intent {
    let intent = match &spec.key {
        Some(key) => Intent::Set {
            key: key.clone(),
            value: raw.to_string(),
            step: 1,
        },
        None => match app.profile.eq.as_ref() {
            Some(eq) => app.loudness_intent(eq, raw),
            None => Intent::Refresh,
        },
    };
    app.expect_raw(spec.id, raw);
    app.readings.sent.insert(spec.id, raw);
    intent
}

/// A left press at column `col` of the bar `bar`, drawn across `area`: focus its row, hold the
/// bar, and set the value the cell stands for.
pub(in crate::tui) fn press(app: &mut App, bar: Bar, area: Rect, col: u16) -> Vec<Intent> {
    let Some(spec) = spec(app, bar) else {
        return Vec::new();
    };
    match bar {
        Bar::Row(row) => app.focus(Pane::Controls, row),
        Bar::Loudness => app.focus_loudness(),
    }
    app.pointer.bar_drag = Some(BarDrag {
        bar,
        id: spec.id,
        held: None,
        last: None,
    });
    to(app, area, col)
}

/// Drag the held bar to column `col` of the bar drawn across `area`, and answer the writes that
/// takes now: the value under the pointer if the device has answered every write to the function,
/// and otherwise nothing, the step held instead. Nothing without a bar held.
pub(in crate::tui) fn to(app: &mut App, area: Rect, col: u16) -> Vec<Intent> {
    let Some(mut drag) = app.pointer.bar_drag.take() else {
        return Vec::new();
    };
    let Some(spec) = drag_spec(app, &drag) else {
        return Vec::new();
    };
    let want = value_for(&spec, area, col);
    drag.last = Some(want);
    let writes = aim(app, &spec, &mut drag.held, want, false);
    app.pointer.bar_drag = Some(drag);
    writes
}

/// Ask for `want` of the bar's function: the Volume through the climb where `want` is above
/// what it may jump to, everything else as a step paced by the device's answers. `forced` writes
/// a step even while the device has not answered the one before it.
fn aim(app: &mut App, spec: &Spec, held: &mut Option<i64>, want: i64, forced: bool) -> Vec<Intent> {
    if spec.volume {
        // A volume that has not been read is not raised from, or lowered blind.
        let motion = app.volume().map(|volume| {
            let situation = Situation {
                reading: app.raw_of(spec.id),
                base: base(app, spec.id),
                floor_db: None,
                asked_db: None,
            };
            safety::judge(volume, &situation, want, &Rules::tui()).motion
        });
        match motion {
            None | Some(Motion::Nothing) => return Vec::new(),
            Some(Motion::Glide { .. }) => return climb_toward(app, spec, held, want),
            Some(Motion::Now(_)) => app.pointer.climb = None,
        }
    }
    instant(app, spec, held, want, forced)
}

/// Write `want` now, or hold it while a write to the function is unanswered (and `forced` is not
/// set), replacing any step held before. Nothing for a step the function is on, or on its way to,
/// already.
fn instant(
    app: &mut App,
    spec: &Spec,
    held: &mut Option<i64>,
    want: i64,
    forced: bool,
) -> Vec<Intent> {
    *held = None;
    if app.planned_raw(spec.id) == Some(want) {
        return Vec::new();
    }
    if app.readings.pending.contains_key(&spec.id) && !forced {
        *held = Some(want);
        return Vec::new();
    }
    vec![write(app, spec, want)]
}

/// Start or steer the Volume's climb toward `want`, a raise [`permitted`] does not allow, and
/// answer the write that takes now. The step is never held for later: it is the climb's target.
fn climb_toward(app: &mut App, spec: &Spec, held: &mut Option<i64>, want: i64) -> Vec<Intent> {
    *held = None;
    let Some(planned) = app.planned_raw(spec.id) else {
        return Vec::new();
    };
    match app
        .pointer
        .climb
        .as_mut()
        .filter(|climb| climb.id == spec.id)
    {
        Some(climb) => climb.climb.steer(want),
        None => {
            app.pointer.climb = Some(VolumeClimb {
                id: spec.id,
                climb: Climb::start(want, planned),
                at: app.now,
            });
        }
    }
    flush_climb(app)
}

/// The climb's next step ([`Climb::next`]), if the device has answered the writes before it. Below
/// [`safety::SAFE_FRACTION`] of the range it is [`safety::safe_step`] itself, at once. Above, it is
/// one step above the last reading the device sent, once the rate has paid for the step
/// ([`safety::CREDIT_CAP`]), held at the target. A reading that is not the last step written ends
/// the climb, and so does writing its target.
fn flush_climb(app: &mut App) -> Vec<Intent> {
    let Some(mut climb) = app.pointer.climb.clone() else {
        return Vec::new();
    };
    let Some(spec) = volume_spec(app).filter(|spec| spec.id == climb.id) else {
        app.pointer.climb = None;
        return Vec::new();
    };
    let pending = app.readings.pending.contains_key(&climb.id);
    let reading = app.raw_of(climb.id);
    match climb.climb.next(
        reading,
        base(app, climb.id),
        pending,
        safety::safe_step(spec.range),
    ) {
        Step::Wait => {
            app.pointer.climb = Some(climb);
            Vec::new()
        }
        Step::Done | Step::Broken => {
            app.pointer.climb = None;
            Vec::new()
        }
        Step::Write(step) => {
            let intent = write(app, &spec, step);
            app.pointer.climb = (!climb.climb.reached()).then_some(climb);
            vec![intent]
        }
    }
}

/// The pace raises of the Volume keep above [`safety::SAFE_FRACTION`] of its range, from the keys
/// and the wheel alike: the glide's own rate, so a key held down climbs no faster than a bar's
/// glide. Its credit is its own, not the bar's: bar drags interleaved with wheel notches can
/// approach twice the glide's rate. The writes go out without waiting for their echoes, so the
/// raises sent that the reading does not show yet count as well as the reading: a rise of the
/// reading covers that many of them, and a fall forgets them.
#[derive(Debug, Clone, PartialEq)]
pub struct StepPace {
    /// The steps paid for and not spent.
    credit: safety::Credit,
    /// The moment the credit was last paid up to.
    at: Instant,
    /// The raises let through that the reading has not shown yet: steps on their way, which the
    /// reading's rises pay off one for one.
    ahead: i64,
    /// The reading `ahead` is counted from.
    seen: Option<i64>,
}

impl App {
    /// Whether `intent` may go to the device at `now`. A raise of the Volume that lands above
    /// [`safety::SAFE_FRACTION`] of its range — counted from the reading and the raises let
    /// through ahead of it — spends a step of the glide's credit, a tenth of the range a second,
    /// for each notch it raises by, and is dropped, whole, while they are not paid for; every
    /// other intent goes.
    pub(crate) fn paces(&mut self, intent: &Intent, now: Instant) -> bool {
        let Intent::Set { key, value, step } = intent else {
            return true;
        };
        if !value.eq_ignore_ascii_case("up") {
            return true;
        }
        let Some((range, reading)) = self
            .volume()
            .filter(|volume| volume.key.eq_ignore_ascii_case(key))
            .and_then(|volume| Some((volume.range?, self.raw_of(volume.id)?)))
        else {
            return true;
        };
        let pace = self.pointer.step_pace.get_or_insert(StepPace {
            credit: safety::Credit::FULL,
            at: now,
            ahead: 0,
            seen: None,
        });
        pace.credit
            .advance(now.saturating_duration_since(pace.at), range.1 - range.0);
        pace.at = now;
        if let Some(seen) = pace.seen.filter(|&seen| reading > seen) {
            pace.ahead = (pace.ahead - (reading - seen)).max(0);
        } else if pace.seen != Some(reading) {
            pace.ahead = 0;
        }
        pace.seen = Some(reading);
        let step = (*step).max(1);
        let mut credit = pace.credit;
        let goes = reading + pace.ahead + step <= safety::safe_step(range)
            || (0..step).all(|_| credit.spend());
        if goes {
            pace.credit = credit;
            pace.ahead += step;
        }
        goes
    }
}

/// The held steps whose function the device has answered every write of — or whose wait has run
/// out ([`App::tick`]) — and the climb's next step, written now: what the loop sends after every
/// reading and every tick.
pub(in crate::tui) fn confirmed(app: &mut App) -> Vec<Intent> {
    let mut writes = Vec::new();
    let held = app
        .pointer
        .bar_drag
        .clone()
        .filter(|drag| drag.held.is_some());
    if let Some(drag) = held
        && let Some(spec) = drag_spec(app, &drag)
        && let Some(held) = drag.held
        && !app.readings.pending.contains_key(&spec.id)
    {
        if let Some(drag) = app.pointer.bar_drag.as_mut() {
            drag.held = None;
        }
        if spec.volume {
            // Judged again: the reading may be nowhere near where the step was held from.
            writes.extend(aim(app, &spec, &mut None, held, false));
        } else if app.planned_raw(spec.id) != Some(held) {
            writes.push(write(app, &spec, held));
        }
    }
    writes.extend(flush_climb(app));
    writes
}

/// Let go of the held bar, the button released at column `col` of the bar drawn across `area` —
/// `end`, or `None` where there is no cell to read — and answer the write that leaves: the value
/// the pointer let go on, written now whether or not the device has answered the writes before
/// it. With no cell, the value the pointer last asked for.
pub(in crate::tui) fn release(app: &mut App, end: Option<(Rect, u16)>) -> Vec<Intent> {
    let Some(mut drag) = app.pointer.bar_drag.take() else {
        return Vec::new();
    };
    let Some(spec) = drag_spec(app, &drag) else {
        return Vec::new();
    };
    let want = match end {
        Some((area, col)) => Some(value_for(&spec, area, col)),
        None => drag.last,
    };
    match want {
        Some(want) => aim(app, &spec, &mut drag.held, want, true),
        None => Vec::new(),
    }
}

/// Let go of a drag whose release never reached the TUI: a left press while a bar is held is the
/// button let go of first, somewhere the mouse was not reported, so the bar is released where the
/// pointer last dragged it before the press is read afresh. Nothing for any other event.
pub(in crate::tui) fn lost(app: &mut App, event: &MouseEvent) -> Vec<Intent> {
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => release(app, None),
        _ => Vec::new(),
    }
}

/// What mouse capture turning off leaves a bar held: its button's release goes to the terminal
/// from then on, so the bar is let go where the pointer last dragged it. Nothing while capture is
/// on.
pub(in crate::tui) fn capture_off(app: &mut App) -> Vec<Intent> {
    match app.mouse {
        true => Vec::new(),
        false => release(app, None),
    }
}

/// A mouse event in the run loop: a press or a wheel step stops the climb where it stands
/// unless the press is on the Volume bar itself ([`press_elsewhere`]); the wheel steps one notch
/// after that. Moves, drags and releases leave it alone.
pub(in crate::tui) fn mouse_event(
    app: &mut App,
    kind: MouseEventKind,
    action: Option<&crate::tui::mouse::MouseAction>,
) {
    if matches!(
        kind,
        MouseEventKind::Down(_) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    ) {
        press_elsewhere(app, action);
    }
}

/// A mouse press stops the climb where it stands unless it is on the Volume bar itself (`action`
/// is what the press resolved to, `None` for a press on nothing): a tab, a border word, another
/// row, a box, the dead cells between. Releases and drags of a bar held are not presses.
fn press_elsewhere(app: &mut App, action: Option<&crate::tui::mouse::MouseAction>) {
    use crate::tui::mouse::MouseAction;
    let on_volume = match action {
        Some(MouseAction::BarPress { bar, .. }) => spec(app, *bar).is_some_and(|spec| spec.volume),
        _ => false,
    };
    if !on_volume {
        app.pointer.climb = None;
    }
}

/// A key press stops the climb where it is and lets go of a held bar, writing the step held for
/// it. A held Volume step is written only if it is [`permitted`] still: a key never sends a
/// raise the climb has not paid for. Called before the key does what it does.
pub(super) fn stop(app: &mut App) -> Vec<Intent> {
    app.pointer.climb = None;
    let Some(drag) = app.pointer.bar_drag.take() else {
        return Vec::new();
    };
    match (drag_spec(app, &drag), drag.held) {
        (Some(spec), Some(held)) if !spec.volume || permitted(app, &spec, held) => {
            instant(app, &spec, &mut None, held, true)
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
