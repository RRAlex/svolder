// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The adapter hint: what a daemon says of a device that keeps failing to connect on one
//! adapter while another adapter hears it.
//!
//! A daemon publishes it with the device's row (`devices`) while the condition holds, and lists
//! [`FEATURE_ADAPTER_HINT`](super::FEATURE_ADAPTER_HINT). A front end draws the sentence and
//! offers the switch; a daemon without the feature sends none and the front end shows what it
//! showed before.

use serde::{Deserialize, Serialize};

use crate::socket::signal::SIGNAL_MINUS;

/// Why a device cannot connect on its adapter and which other adapter hears it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterHint {
    /// the device's configured name
    pub device: String,
    /// the kernel name of the adapter the connects fail on, `hci2`
    pub failing: String,
    /// whether that adapter hears the device at all
    #[serde(default)]
    pub own_heard: bool,
    /// how loudly it hears it, in dBm, when it said: `None` with `own_heard` set is a device
    /// heard with no signal reading
    #[serde(default)]
    pub own_dbm: Option<i16>,
    /// the kernel name of the adapter that hears the device, `hci0`
    pub better: String,
    /// how loudly that adapter hears it, in dBm, when it said
    #[serde(default)]
    pub better_dbm: Option<i16>,
    /// what to name in the config: that adapter's address, or its name where BlueZ gave none
    pub take: String,
    /// the words of the last failed connect
    pub failure: String,
    /// how many connects in a row have failed
    pub tries: u32,
}

/// A signal as it is written: `−74 dBm`.
fn dbm(rssi: i16) -> String {
    format!(
        "{} dBm",
        rssi.to_string().replacen('-', &SIGNAL_MINUS.to_string(), 1)
    )
}

impl AdapterHint {
    /// The wire's two fields of one adapter's hearing: `None` is not heard, `Some(None)` heard
    /// with no reading, `Some(Some(dbm))` heard at that signal.
    pub fn own_from(own: Option<Option<i16>>) -> (bool, Option<i16>) {
        (own.is_some(), own.flatten())
    }

    /// What each adapter hears: `hci2 hears BF3 at −80 dBm and cannot hold a link; hci0 hears it
    /// at −74 dBm`, without a closing stop.
    pub fn hears(&self) -> String {
        let own = match (self.own_heard, self.own_dbm) {
            (_, Some(rssi)) => format!(
                "{} hears {} at {} and cannot hold a link",
                self.failing,
                self.device,
                dbm(rssi)
            ),
            (true, None) => format!(
                "{} hears {} and cannot hold a link",
                self.failing, self.device
            ),
            (false, None) => format!("{} does not hear {}", self.failing, self.device),
        };
        let other = match self.better_dbm {
            Some(rssi) => format!("{} hears it at {}", self.better, dbm(rssi)),
            None => format!("{} hears it", self.better),
        };
        format!("{own}; {other}")
    }

    /// The step that follows, in the config's words: `try adapter = "AA:BB:CC:DD:EE:01"`.
    pub fn step(&self) -> String {
        format!("try adapter = \"{}\"", self.take)
    }

    /// The hint as a command prints it under a device that is not connected: what each adapter
    /// hears, and on a line of its own the step and where the adapters are listed.
    pub fn lines(&self) -> [String; 2] {
        [
            format!("{}:", self.hears()),
            format!(
                "{} (svolder adapters, or Settings \u{2192} Bluetooth adapter)",
                self.step()
            ),
        ]
    }

    /// The failure's line: the device, the last failure's words and how many connects failed on
    /// which adapter.
    pub fn failed(&self) -> String {
        let tries = match self.tries {
            1 => "1 try".to_string(),
            n => format!("{n} tries"),
        };
        format!(
            "{}: {}, {tries} on {}.",
            self.device, self.failure, self.failing
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint() -> AdapterHint {
        AdapterHint {
            device: "BF3".into(),
            failing: "hci2".into(),
            own_heard: true,
            own_dbm: Some(-80),
            better: "hci0".into(),
            better_dbm: Some(-74),
            take: "AA:BB:CC:DD:EE:01".into(),
            failure: "no connection within 20 s (le-connection-abort-by-local)".into(),
            tries: 4,
        }
    }

    #[test]
    fn the_hint_says_what_each_adapter_hears_and_the_step() {
        let hint = hint();
        assert_eq!(
            hint.hears(),
            "hci2 hears BF3 at −80 dBm and cannot hold a link; hci0 hears it at −74 dBm"
        );
        assert_eq!(hint.step(), "try adapter = \"AA:BB:CC:DD:EE:01\"");
        assert_eq!(
            hint.failed(),
            "BF3: no connection within 20 s (le-connection-abort-by-local), 4 tries on hci2."
        );
    }

    #[test]
    fn the_command_line_prints_the_hint_in_two_lines() {
        assert_eq!(
            hint().lines(),
            [
                "hci2 hears BF3 at −80 dBm and cannot hold a link; hci0 hears it at −74 dBm:",
                "try adapter = \"AA:BB:CC:DD:EE:01\" (svolder adapters, or Settings → Bluetooth adapter)"
            ]
        );
    }

    #[test]
    fn an_adapter_that_hears_nothing_or_no_reading_is_worded_so() {
        let mut hint = hint();
        (hint.own_heard, hint.own_dbm) = AdapterHint::own_from(None);
        hint.better_dbm = None;
        assert_eq!(hint.hears(), "hci2 does not hear BF3; hci0 hears it");
        (hint.own_heard, hint.own_dbm) = AdapterHint::own_from(Some(None));
        assert!(hint.hears().starts_with("hci2 hears BF3 and cannot hold"));
    }

    #[test]
    fn the_hint_round_trips_in_all_three_hearings() {
        for own in [None, Some(None), Some(Some(-80))] {
            let mut hint = hint();
            (hint.own_heard, hint.own_dbm) = AdapterHint::own_from(own);
            let text = serde_json::to_string(&hint).unwrap();
            let back = serde_json::from_str::<AdapterHint>(&text).unwrap();
            assert_eq!(back, hint);
            assert_eq!((back.own_heard, back.own_dbm), AdapterHint::own_from(own));
        }
    }
}
