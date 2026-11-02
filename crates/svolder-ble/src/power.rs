// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether a device that has a power state is on, followed from its status messages, so the
//! session can tell when one comes back on.

use svolder_profiles::Profile;
use svolder_proto::{Direction, Message, Value};

/// The control key of the power function in a profile.
const POWER_KEY: &str = "power";

/// The control key of the warming-up readout in a profile.
const WARMING_KEY: &str = "warming_up";

/// Which functions a model's power state follows, read off its profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PowerRule {
    /// The first function id of the power function. A status is matched on this alone.
    power: u8,
    /// The first function id of the warming-up readout, for a model whose power is the
    /// combination of both. Absent on a model whose power function says it all.
    warming: Option<u8>,
}

impl PowerRule {
    /// The rule for a profile, or `None` for a model with no power state: its profile names no
    /// `power` control.
    pub(crate) fn of(profile: &Profile) -> Option<Self> {
        let power = profile.control_by_key(POWER_KEY)?.id.first();
        let warming = profile
            .control_by_key(WARMING_KEY)
            .map(|control| control.id.first());
        Some(Self { power, warming })
    }
}

/// Where a device's power state stands, as far as its statuses have told.
///
/// The state starts on. It moves only on a status with a Boolean value, so a command, or a
/// status of another type on the same function, changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PowerWatch {
    /// Whether the device is on: not in standby, off or warming up.
    on: bool,
    /// The last Boolean the power function reported; `None` until it has reported one.
    power: Option<bool>,
    /// The last Boolean the warming-up readout reported, false until it has reported one.
    warming: bool,
}

impl Default for PowerWatch {
    fn default() -> Self {
        Self {
            on: true,
            power: None,
            warming: false,
        }
    }
}

impl PowerWatch {
    /// Fold one message in. Returns true when it takes the device from standby, off or warming
    /// up to on: once per such change, and never for a message that leaves the state as it was.
    ///
    /// Where the power function alone says it (`10` true is on, false is not), that is the
    /// state. Where the warming-up readout is part of it, the device is on when the power
    /// function reads true and the readout false, warming up when both read true, and in
    /// standby when the power function reads false. Nothing is judged until the power function
    /// has reported, whichever the readout said first, so a reading of the readout ahead of the
    /// power function does not look like a standby followed by a wake.
    pub(crate) fn observe(&mut self, rule: PowerRule, message: &Message) -> bool {
        if message.direction != Direction::Status {
            return false;
        }
        let Value::Bool(value) = message.value else {
            return false;
        };
        let first = message.id.first();
        if first == rule.power {
            self.power = Some(value);
        } else if rule.warming == Some(first) {
            self.warming = value;
        } else {
            return false;
        }
        let Some(power) = self.power else {
            return false;
        };
        let was_on = self.on;
        self.on = power && !(rule.warming.is_some() && self.warming);
        self.on && !was_on
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svolder_proto::FunctionId;

    const ALONE: PowerRule = PowerRule {
        power: 10,
        warming: None,
    };
    const AMP: PowerRule = PowerRule {
        power: 10,
        warming: Some(4),
    };

    fn report(id: u8, value: Value) -> Message {
        Message::status(FunctionId::one(id), value)
    }

    fn flag(id: u8, on: bool) -> Message {
        report(id, Value::Bool(on))
    }

    #[test]
    fn the_power_function_alone_decides_a_model_without_a_readout() {
        let mut w = PowerWatch::default();
        assert!(!w.observe(ALONE, &flag(10, true)), "on to on");
        assert!(!w.observe(ALONE, &flag(10, false)), "on to standby");
        assert!(!w.observe(ALONE, &flag(10, false)), "standby to standby");
        assert!(w.observe(ALONE, &flag(10, true)), "standby to on");
        assert!(!w.observe(ALONE, &flag(10, true)), "on to on");
    }

    #[test]
    fn a_readout_is_ignored_where_the_power_function_says_it_all() {
        let mut w = PowerWatch::default();
        assert!(!w.observe(ALONE, &flag(10, false)));
        assert!(!w.observe(ALONE, &flag(4, true)));
        assert!(w.observe(ALONE, &flag(10, true)));
    }

    #[test]
    fn warming_up_is_not_on_until_the_readout_clears() {
        let mut w = PowerWatch::default();
        assert!(!w.observe(AMP, &flag(10, true)), "first report, on");
        assert!(!w.observe(AMP, &flag(4, true)), "on to warming up");
        assert!(w.observe(AMP, &flag(4, false)), "warming up to on");
        assert!(!w.observe(AMP, &flag(10, false)), "on to standby");
        assert!(!w.observe(AMP, &flag(4, true)), "readout while in standby");
        assert!(!w.observe(AMP, &flag(10, true)), "standby to warming up");
        assert!(w.observe(AMP, &flag(4, false)), "warming up to on");
    }

    #[test]
    fn the_readout_ahead_of_the_power_function_judges_nothing() {
        let mut w = PowerWatch::default();
        assert!(!w.observe(AMP, &flag(4, false)));
        assert!(!w.observe(AMP, &flag(10, true)), "no standby came first");
        let mut w = PowerWatch::default();
        assert!(!w.observe(AMP, &flag(4, true)));
        assert!(!w.observe(AMP, &flag(10, true)), "straight to warming up");
        assert!(w.observe(AMP, &flag(4, false)));
    }

    #[test]
    fn only_a_boolean_status_counts() {
        let mut w = PowerWatch::default();
        assert!(!w.observe(ALONE, &flag(10, false)));
        for other in [Value::U8(1), Value::U16(1), Value::U32(1)] {
            assert!(!w.observe(ALONE, &report(10, other)));
        }
        assert!(!w.observe(
            ALONE,
            &Message::command(FunctionId::one(10), Value::Bool(true))
        ));
        assert!(w.observe(ALONE, &flag(10, true)));
    }

    #[test]
    fn a_function_is_matched_on_its_first_id_alone() {
        let mut w = PowerWatch::default();
        assert!(!w.observe(ALONE, &flag(10, false)));
        assert!(w.observe(
            ALONE,
            &Message::status(FunctionId::two(10, 3), Value::Bool(true))
        ));
        assert!(!w.observe(ALONE, &flag(11, false)));
    }
}
