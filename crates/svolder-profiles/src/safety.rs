// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The output-safety rule: how far a level change may go at once, and how the volume climbs
//! where it may not.
//!
//! [`judge`] lays three rules over one asked-for raw step, in order: the cap a front end sets
//! (`max_volume_db`, on the volume alone), the question a rise of more than [`MAX_RISE_DB`]
//! over the lowest level of the last minute asks (on every level in dB), and the motion: at
//! once, or, for the volume, a glide above [`SAFE_FRACTION`] of its raw range at
//! [`CLIMB_PER_SEC`] of the range a second. The glide counts in raw steps and the question in
//! dB, and the two never mix; negative infinity stays in the arithmetic, so a rise from silence
//! is the largest there is. [`Climb`] is the glide's stepper, with no clock of its own: the
//! TUI's volume climb and the daemon's writer each pay it the time that passed.

use std::time::Duration;

use crate::convert;
use crate::model::{Control, ControlKind};
use crate::roles;

/// The fraction of the volume's raw range a raise reaches at once; above it the volume climbs.
pub const SAFE_FRACTION: f64 = 0.75;

/// The fraction of the volume's raw range a climb gains a second.
pub const CLIMB_PER_SEC: f64 = 0.1;

/// The most credit, in raw steps, a climb keeps while it waits for the device: a step to spend,
/// and half of one for a tick that lands between two.
pub const CREDIT_CAP: f64 = 1.5;

/// The longest stretch of time one payment counts toward a climb: a loop that stalled slows the
/// climb rather than jumping it.
pub const LONGEST_STEP: Duration = Duration::from_secs(1);

/// A rise larger than this, in dB, over the lowest level of the last [`FLOOR_WINDOW`], asks
/// first where a front end asks at all.
pub const MAX_RISE_DB: f64 = 6.0;

/// How long a level stays the floor a later rise is measured from.
pub const FLOOR_WINDOW: Duration = Duration::from_secs(60);

/// The highest raw step a raise of a level over `range` reaches at once: three quarters of the
/// range, rounded down.
pub fn safe_step(range: (i64, i64)) -> i64 {
    let (lo, hi) = range;
    (lo as f64 + SAFE_FRACTION * (hi - lo) as f64).floor() as i64
}

/// Whether `target` lands at once on a volume over `range`: at or below [`safe_step`], or at or
/// below `base`, the lower of the last reading and the last value sent and not yet confirmed.
pub fn lands_at_once(range: (i64, i64), base: Option<i64>, target: i64) -> bool {
    target <= safe_step(range) || base.is_some_and(|base| target <= base)
}

/// Whether `control` is a level in dB, the kind the rise question measures.
pub fn in_db(control: &Control) -> bool {
    control.kind == ControlKind::Level
        && control.convert.is_some()
        && control
            .unit
            .as_deref()
            .is_some_and(|unit| unit.eq_ignore_ascii_case("dB"))
}

/// What a front end knows of the control it judges a change of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Situation {
    /// The last raw reading; `None` while the device has not reported one.
    pub reading: Option<i64>,
    /// The lower of the reading and the last raw value sent and not yet confirmed: what a raise
    /// is judged from, so a lowering whose echo was lost is never undone. The reading when
    /// nothing is unconfirmed.
    pub base: Option<i64>,
    /// The lowest level, in dB, of the last [`FLOOR_WINDOW`], where a front end keeps one.
    pub floor_db: Option<f64>,
    /// The value asked for, in the control's units, before it was snapped to a step.
    pub asked_db: Option<f64>,
}

/// What a value above the cap earns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverCap {
    /// It lands on the highest step at or below the cap, a lowering from above it included.
    /// When no step is at or below the cap, a raise from a known reading is refused and anything
    /// else lands on the quietest step.
    Clamp,
    /// A rise to it is refused; a lowering goes through.
    Refuse,
}

/// What a volume the device has never reported earns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unread {
    /// Nothing is written, not even a lowering.
    Skip,
    /// At most [`safe_step`] is written, and a climb goes on from its echo.
    SafeFirst,
}

/// The rules one front end judges a change by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rules {
    /// The cap, in dB, on the volume alone.
    pub cap_db: Option<f64>,
    /// What a value above the cap earns.
    pub over_cap: OverCap,
    /// The rise, in dB, over the window's floor that asks first, on every level in dB.
    pub ask_rise_db: Option<f64>,
    /// Whether the volume glides above [`SAFE_FRACTION`].
    pub glide: bool,
    /// What a volume never reported earns.
    pub unread: Unread,
}

impl Rules {
    /// The TUI's bar: the glide alone; a volume never read is not written.
    pub fn tui() -> Rules {
        Rules {
            cap_db: None,
            over_cap: OverCap::Clamp,
            ask_rise_db: None,
            glide: true,
            unread: Unread::Skip,
        }
    }

    /// MQTT: the cap clamps, and the volume glides.
    pub fn mqtt(cap_db: Option<f64>) -> Rules {
        Rules {
            cap_db,
            over_cap: OverCap::Clamp,
            ask_rise_db: None,
            glide: true,
            unread: Unread::SafeFirst,
        }
    }

    /// The web UI: MQTT's rules; the cap clamps, nothing is asked, the volume glides, and a
    /// volume never read gets at most three quarters first.
    pub fn web(cap_db: Option<f64>) -> Rules {
        Rules::mqtt(cap_db)
    }

    /// MCP: a rise past the cap is refused, a rise of more than [`MAX_RISE_DB`] asks, and the
    /// volume glides.
    pub fn mcp(cap_db: Option<f64>) -> Rules {
        Rules {
            cap_db,
            over_cap: OverCap::Refuse,
            ask_rise_db: Some(MAX_RISE_DB),
            glide: true,
            unread: Unread::SafeFirst,
        }
    }

    /// The daemon's writer, which climbs what a front end has already capped and asked about.
    pub fn writer() -> Rules {
        Rules {
            cap_db: None,
            over_cap: OverCap::Clamp,
            ask_rise_db: None,
            glide: true,
            unread: Unread::SafeFirst,
        }
    }
}

/// What the cap did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capped {
    /// The cap, in dB.
    pub cap_db: f64,
    /// Whether the change is refused rather than clamped: [`OverCap::Refuse`] on a rise, or a
    /// raise under a cap no step is at or below.
    pub refused: bool,
    /// The value asked for, before it was snapped.
    pub asked_db: Option<f64>,
    /// Whether the cap is below every step and the change landed on the quietest step; false
    /// for a raise refused under such a cap.
    pub quietest: bool,
}

/// A rise the user is asked about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rise {
    /// The level now, in dB; `None` while the device has not reported one.
    pub from_db: Option<f64>,
    /// The level the change lands on, in dB.
    pub to_db: f64,
    /// The level it is measured from: the lower of the base and the window's floor; negative
    /// infinity for a level not reported or silent.
    pub floor_db: f64,
}

/// How a change reaches the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Not at all.
    Nothing,
    /// This raw step, at once.
    Now(i64),
    /// A climb to `target`, after writing `first` at once when there is one.
    Glide {
        /// The step written at once: [`safe_step`] when the base is below it.
        first: Option<i64>,
        /// Where the climb heads.
        target: i64,
    },
}

/// The layered verdict on one change: the cap, then the question, then the motion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Judgement {
    /// The raw step after the cap.
    pub target: i64,
    /// What the cap did, when it did anything.
    pub capped: Option<Capped>,
    /// The rise to ask about, when there is one; the caller words it and asks.
    pub question: Option<Rise>,
    /// How the change reaches the device once the question, if any, is answered yes.
    pub motion: Motion,
}

/// Judge setting `control` to raw step `target` in `situation` by `rules`.
///
/// The cap applies to [`roles::is_volume`] alone: [`OverCap::Clamp`] lands any value above it
/// on the highest step at or below it (the quietest step when the cap is below every step,
/// where a raise from a known reading is refused), [`OverCap::Refuse`] refuses a rise above it
/// (by the snapped step or the value asked for) and lets a lowering through. The question
/// applies to every level in dB ([`in_db`]): a rise more than the rules' limit above the lower of
/// the base ([`Situation::base`]) and the window's floor. The glide applies to the volume alone:
/// a target at or below [`safe_step`] or the base lands at once, anything else climbs.
pub fn judge(control: &Control, situation: &Situation, target: i64, rules: &Rules) -> Judgement {
    // What a raise is judged from: never above the reading, so a base the device has not
    // confirmed cannot make a raise look like a lowering.
    let base = situation
        .reading
        .map(|reading| situation.base.map_or(reading, |base| base.min(reading)));
    let current_db = base
        .and_then(|raw| roles::step_value(control, raw))
        .unwrap_or(f64::NEG_INFINITY);
    let mut target = target;
    let mut capped = None;
    if let Some(cap) = roles::volume_cap(control, rules.cap_db) {
        let lands = roles::step_value(control, target);
        // A step with no value counts as over the cap, never under it.
        let over = lands.is_none_or(|value| value > cap)
            || situation.asked_db.is_some_and(|asked| asked > cap);
        let rise = lands.is_some_and(|value| value > current_db);
        match rules.over_cap {
            OverCap::Clamp if over => {
                let step = roles::capped_step(control, cap);
                // No step at or below the cap: a raise is refused, and anything else lands on
                // the quietest step, which is never a rise from any reading.
                let none_under = step.is_none();
                let raise = base.is_some() && rise;
                let step =
                    step.or_else(|| (!raise).then(|| roles::quietest_step(control)).flatten());
                let quietest = none_under && step.is_some();
                capped = Some(Capped {
                    cap_db: cap,
                    refused: step.is_none(),
                    asked_db: situation.asked_db,
                    quietest,
                });
                match step {
                    Some(step) => target = step,
                    None => {
                        return Judgement {
                            target,
                            capped,
                            question: None,
                            motion: Motion::Nothing,
                        };
                    }
                }
            }
            OverCap::Refuse if over && rise => {
                return Judgement {
                    target,
                    capped: Some(Capped {
                        cap_db: cap,
                        refused: true,
                        asked_db: situation.asked_db,
                        quietest: false,
                    }),
                    question: None,
                    motion: Motion::Nothing,
                };
            }
            _ => {}
        }
    }
    let question = match rules.ask_rise_db {
        Some(limit) if in_db(control) => convert::raw_to_display(control, target)
            .filter(|to| *to > current_db)
            .and_then(|to| {
                let floor = situation
                    .floor_db
                    .map_or(current_db, |floor| floor.min(current_db));
                (to - floor > limit + 1e-9).then(|| Rise {
                    from_db: situation
                        .reading
                        .and_then(|raw| convert::raw_to_display(control, raw)),
                    to_db: to,
                    floor_db: floor,
                })
            }),
        _ => None,
    };
    let motion = match control.range {
        Some(range) if rules.glide && roles::is_volume(control) => match situation.reading {
            None => match rules.unread {
                Unread::Skip => Motion::Nothing,
                Unread::SafeFirst if target <= safe_step(range) => Motion::Now(target),
                Unread::SafeFirst => Motion::Glide {
                    first: Some(safe_step(range)),
                    target,
                },
            },
            Some(reading) => {
                let base = base.unwrap_or(reading);
                let safe = safe_step(range);
                match lands_at_once(range, Some(base), target) {
                    true => Motion::Now(target),
                    false => Motion::Glide {
                        first: (base < safe).then_some(safe),
                        target,
                    },
                }
            }
        },
        _ => Motion::Now(target),
    };
    Judgement {
        target,
        capped,
        question,
        motion,
    }
}

/// The raw steps a climb has been paid for and not spent, fractions kept, held at
/// [`CREDIT_CAP`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Credit(f64);

impl Credit {
    /// Nothing paid for: a climb starts here.
    pub const NONE: Credit = Credit(0.0);
    /// As much as is ever kept: a rest leaves this.
    pub const FULL: Credit = Credit(CREDIT_CAP);

    /// Pay the credit `elapsed` earns along a range of `span` raw steps: [`CLIMB_PER_SEC`] of the
    /// span a second, counting at most [`LONGEST_STEP`] of it, held at [`CREDIT_CAP`].
    pub fn advance(&mut self, elapsed: Duration, span: i64) {
        let secs = elapsed.min(LONGEST_STEP).as_secs_f64();
        self.0 = (self.0 + CLIMB_PER_SEC * span as f64 * secs).min(CREDIT_CAP);
    }

    /// Spend one step, if one is paid for.
    pub fn spend(&mut self) -> bool {
        match self.0 >= 1.0 {
            true => {
                self.0 -= 1.0;
                true
            }
            false => false,
        }
    }
}

/// A volume climbing to a target it may not jump to, one raw step above the base at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct Climb {
    target: i64,
    credit: Credit,
    written: i64,
}

/// What a climb does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing yet: a write is unanswered, or the next step is not paid for.
    Wait,
    /// Write this raw step.
    Write(i64),
    /// The target is reached or passed: the climb is over.
    Done,
    /// The reading is not the last step written — the volume was changed from somewhere else,
    /// or an older value arrived late — and the climb does not carry on from a value it did not
    /// write.
    Broken,
}

impl Climb {
    /// A climb to `target` from `written`, the step last written (or read), with no credit.
    pub fn start(target: i64, written: i64) -> Climb {
        Climb {
            target,
            credit: Credit::NONE,
            written,
        }
    }

    /// Where the climb heads.
    pub fn target(&self) -> i64 {
        self.target
    }

    /// The step last written, which the reading must show for the climb to go on.
    pub fn written(&self) -> i64 {
        self.written
    }

    /// Head for `target` instead.
    pub fn steer(&mut self, target: i64) {
        self.target = target;
    }

    /// Pay the credit `elapsed` earns along a range of `span` raw steps ([`Credit::advance`]).
    pub fn advance(&mut self, elapsed: Duration, span: i64) {
        self.credit.advance(elapsed, span);
    }

    /// Whether the last step written is the target.
    pub fn reached(&self) -> bool {
        self.written >= self.target
    }

    /// The next step, given the `reading`, the `base` (the lower of the reading and a value sent
    /// and unconfirmed), whether a write to the volume is `pending`, and `safe`
    /// ([`safe_step`]): below `safe` the step is `safe` itself, at once; from there one step above
    /// the base once the credit pays for it, never past the target.
    pub fn next(
        &mut self,
        reading: Option<i64>,
        base: Option<i64>,
        pending: bool,
        safe: i64,
    ) -> Step {
        if pending {
            return Step::Wait;
        }
        let Some(reading) = reading.filter(|reading| *reading == self.written) else {
            return Step::Broken;
        };
        let level = base.map_or(reading, |base| base.min(reading));
        let step = if level < safe {
            safe
        } else if self.credit.spend() {
            level + 1
        } else {
            return Step::Wait;
        };
        if step > self.target {
            return Step::Done;
        }
        self.written = step;
        Step::Write(step)
    }
}

/// How a change lets sound through that was not before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Through {
    /// A gain or mode select raised, or moved where either side cannot be read.
    GainUp,
    /// A mute released.
    Unmute,
    /// A device powered on.
    PowerOn,
    /// A change of a control that asks on any change.
    AnyChange,
}

/// Whether setting `control` to `new` from `current` lets sound through, and how: a select a
/// profile marks `output_gain` set higher (or either side unknown), a `mutes_output` toggle set
/// anywhere but on, a `powers_output` toggle set anywhere but off, any change of an
/// `asks_on_change` control. A toggle's on is 1 and off is 0, and a value not known counts as the
/// one that lets sound through. The classification alone: whether to ask, and the words, are the
/// front end's.
pub fn lets_sound_through(
    control: &Control,
    new: Option<i64>,
    current: Option<i64>,
) -> Option<Through> {
    if control.output_gain {
        let rises = match (current, new) {
            (Some(current), Some(new)) => new > current,
            _ => true,
        };
        return rises.then_some(Through::GainUp);
    }
    if control.mutes_output && new != Some(1) {
        return Some(Through::Unmute);
    }
    if control.powers_output && new != Some(0) {
        return Some(Through::PowerOn);
    }
    control.asks_on_change.then_some(Through::AnyChange)
}

#[cfg(test)]
mod tests;
