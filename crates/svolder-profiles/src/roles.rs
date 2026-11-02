// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device rules every front end shares: which control is the output volume, how a level's
//! raw steps map to the values a cap is stated in, and what a relative step needs before it
//! is built. Pure functions over a profile and plain numbers; the verdict a front end gives
//! (refuse, clamp) stays with the front end.

use svolder_proto::Message;

use crate::codec::{relative, steppable_control};
use crate::convert;
use crate::error::ProfileError;
use crate::model::{Action, Control, ControlKind, Profile};

/// Whether `control` is the output volume: the level keyed `volume`, ignoring ASCII case.
pub fn is_volume(control: &Control) -> bool {
    control.kind == ControlKind::Level && control.key.eq_ignore_ascii_case("volume")
}

/// The profile's output volume, when it has one.
pub fn volume(profile: &Profile) -> Option<&Control> {
    profile
        .control_by_key("volume")
        .filter(|control| is_volume(control))
}

/// The key of the recall action.
pub const RECALL_KEY: &str = "recall";

/// The profile's recall action, when it has one: the action keyed `recall`, ignoring ASCII case.
pub fn recall_action(profile: &Profile) -> Option<&Action> {
    profile.action_by_key(RECALL_KEY)
}

/// The key of the save action.
pub const SAVE_KEY: &str = "save";

/// The profile's save action, when it has one: the action keyed `save`, ignoring ASCII case.
pub fn save_action(profile: &Profile) -> Option<&Action> {
    profile.action_by_key(SAVE_KEY)
}

/// The cap that applies to `control`: `max_db` when `control` is the volume, else none.
pub fn volume_cap(control: &Control, max_db: Option<f64>) -> Option<f64> {
    max_db.filter(|_| is_volume(control))
}

/// One raw step's value in the control's own units: its conversion's, or the raw number
/// counted from the control's centre when it has none.
pub fn step_value(control: &Control, raw: i64) -> Option<f64> {
    match control.convert {
        Some(_) => convert::raw_to_display(control, raw),
        None => Some((raw - origin(control)) as f64),
    }
}

/// The raw step of `control` whose value is the highest at or below `cap`, in the control's
/// own units: what a command above the cap is sent as; steps of equal value go to the lower raw.
/// `None` when no step is that low, or the control has no range.
pub fn capped_step(control: &Control, cap: f64) -> Option<i64> {
    let (lo, hi) = control.range?;
    (lo..=hi)
        .filter_map(|raw| step_value(control, raw).map(|v| (raw, v)))
        .filter(|(_, v)| *v <= cap)
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(raw, _)| raw)
}

/// The raw step of `control` with the lowest value, in the control's own units: where a volume
/// lands under a cap below every step; steps of equal value go to the lower raw. `None` when
/// no step has a value, or the control has no range.
pub fn quietest_step(control: &Control) -> Option<i64> {
    let (lo, hi) = control.range?;
    (lo..=hi)
        .filter_map(|raw| step_value(control, raw).map(|v| (raw, v)))
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(raw, _)| raw)
}

pub use crate::units::origin;

/// The command that nudges the level `key` by `delta` from `current` (its last reported raw
/// value), after the checks every front end makes first: the key names a control, the control
/// is a level, and it has a range to step through. Those refusals come before the reading is
/// looked at, so a toggle is turned away for being a toggle rather than for having no reading.
///
/// `Ok(None)` means the key is steppable but there is no reading to step from; the caller
/// words that refusal, since its error type and code are its own.
pub fn relative_step(
    profile: &Profile,
    key: &str,
    current: Option<i64>,
    delta: i64,
) -> Result<Option<Message>, ProfileError> {
    steppable(profile, key)?;
    current
        .map(|current| relative(profile, key, current, delta))
        .transpose()
}

/// The level `key` names, when it can be stepped: the key names a control, the control is a
/// level (an action is not), and it has a range to step through. These are the refusals
/// [`relative_step`] makes before it looks at a reading.
pub fn steppable<'a>(profile: &'a Profile, key: &str) -> Result<&'a Control, ProfileError> {
    let control = steppable_control(profile, key)?;
    if control.kind != ControlKind::Level || control.range.is_none() {
        return Err(ProfileError::NotSteppable(control.key.clone()));
    }
    Ok(control)
}

/// The raw step a relative step of `delta` takes on a level that reads `reading`, inside
/// `range`, given `last`, the step last asked or sent of it and whether a relative step worked
/// it out. It starts from `last` while that is below the reading (a lowering on its way: never
/// louder), or, for a chain of relative steps, held at two steps above the reading at most, so
/// the chain stalls there rather than falling back; an absolute step more than two steps above
/// the reading, and none, start it from the reading. A lowering is also never above the reading
/// less its step, whatever `last` says, so a lost echo cannot turn a `down` into a rise.
pub fn next_step(last: Option<(i64, bool)>, reading: i64, delta: i64, (lo, hi): (i64, i64)) -> i64 {
    let reach = reading.saturating_add(delta.saturating_abs().saturating_mul(2));
    let from = match last {
        Some((raw, true)) => raw.min(reach),
        Some((raw, false)) if raw <= reach => raw,
        _ => reading,
    };
    let step = from.saturating_add(delta).clamp(lo, hi);
    match delta < 0 {
        true => step.min(reading.saturating_add(delta).clamp(lo, hi)),
        false => step,
    }
}

#[cfg(test)]
mod tests;
