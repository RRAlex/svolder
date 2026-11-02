// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A device's signal strength as it crosses the socket, and the one way every front end words it.
//!
//! The daemon keeps the last strength it heard for each device with where it came from and when,
//! and `devices` carries it. The CLI, the TUI and the web page all word it through
//! [`Signal::describe`], so the same reading reads the same everywhere.

use serde::{Deserialize, Serialize};

/// Where a signal strength was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSource {
    /// From an advertisement the device sent while it was being looked for, before the link.
    Advert,
    /// From the link itself while it was up. The daemon does not produce this: BlueZ gives no
    /// signal strength for a connected device to an unprivileged user. A daemon of another
    /// build may send it, and it is worded as it is.
    Live,
    /// A source this build does not know: a daemon of a newer build named one. It only ever comes
    /// from decoding, so an older client reads the signal it came with instead of failing the
    /// whole answer.
    #[serde(other)]
    Unknown,
}

impl SignalSource {
    /// The one word for the source: what serde writes it as and what a front end says.
    pub fn word(self) -> &'static str {
        match self {
            SignalSource::Advert => "advert",
            SignalSource::Live => "live",
            SignalSource::Unknown => "unknown",
        }
    }
}

/// How strong a device's radio signal was, where that was measured and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signal {
    /// the received signal strength in dBm
    pub dbm: i16,
    /// where it was measured
    pub source: SignalSource,
    /// when the daemon heard it, in unix milliseconds
    pub heard_at_ms: u64,
}

/// The minus sign the TUI and the web page write a negative strength with: the proper one, not the
/// hyphen the CLI and JSON use.
pub const SIGNAL_MINUS: char = '\u{2212}';

/// How long a live signal still counts as current, in seconds; one older says how old it is.
const LIVE_FRESH_SECS: u64 = 30;

impl Signal {
    /// The signal in words: `-58 dBm live`, or `-62 dBm advert, 3 min ago`.
    ///
    /// An advert's age always shows; a live signal's shows once it is older than half a minute.
    /// `minus` is the character written for a negative sign: `'-'` for text a shell reads and
    /// `'\u{2212}'` where a proper minus is wanted. A stamp after `now_ms` reads as no age.
    pub fn describe(&self, now_ms: u64, minus: char) -> String {
        let dbm = self.dbm_text(minus);
        let age_secs = now_ms.saturating_sub(self.heard_at_ms) / 1000;
        let age = format!("{} ago", age_words(age_secs));
        match self.source {
            SignalSource::Live if age_secs < LIVE_FRESH_SECS => format!("{dbm} dBm live"),
            source => format!("{dbm} dBm {}, {age}", source.word()),
        }
    }

    /// The signal in a few words for a list column: `-58 dBm`, or `-62 dBm 3 min` once it has an
    /// age to say (an advert's always, a live signal's after half a minute). It does not say
    /// where the strength was measured; [`Signal::describe`] does. `minus` is as there.
    pub fn describe_short(&self, now_ms: u64, minus: char) -> String {
        let dbm = self.dbm_text(minus);
        let age_secs = now_ms.saturating_sub(self.heard_at_ms) / 1000;
        match self.source {
            SignalSource::Live if age_secs < LIVE_FRESH_SECS => format!("{dbm} dBm"),
            _ => format!("{dbm} dBm {}", age_words(age_secs)),
        }
    }

    /// The signal in words without its age (`-62 dBm advert`, `-58 dBm live`), for a caller with
    /// no clock to measure the age by. `minus` is as in [`Signal::describe`].
    pub fn describe_undated(&self, minus: char) -> String {
        format!("{} dBm {}", self.dbm_text(minus), self.source.word())
    }

    /// The strength as a number with the sign written as `minus`.
    fn dbm_text(&self, minus: char) -> String {
        if self.dbm < 0 {
            format!("{minus}{}", self.dbm.unsigned_abs())
        } else {
            self.dbm.to_string()
        }
    }
}

/// `signal` in words ([`Signal::describe`]), or an em dash when there is none.
pub fn describe_signal(signal: Option<&Signal>, now_ms: u64, minus: char) -> String {
    signal.map_or_else(|| "\u{2014}".to_string(), |s| s.describe(now_ms, minus))
}

/// `signal` in the short form ([`Signal::describe_short`]), or an em dash when there is none.
pub fn describe_signal_short(signal: Option<&Signal>, now_ms: u64, minus: char) -> String {
    signal.map_or_else(
        || "\u{2014}".to_string(),
        |s| s.describe_short(now_ms, minus),
    )
}

/// A span of `secs` seconds in the largest unit that keeps it readable: seconds under 90 s,
/// minutes under 90 min, hours under 36 h, days after. Each count is rounded down.
fn age_words(secs: u64) -> String {
    const MIN: u64 = 60;
    const HOUR: u64 = 60 * MIN;
    const DAY: u64 = 24 * HOUR;
    if secs < 90 {
        format!("{secs} s")
    } else if secs < 90 * MIN {
        format!("{} min", secs / MIN)
    } else if secs < 36 * HOUR {
        format!("{} h", secs / HOUR)
    } else {
        let days = secs / DAY;
        format!("{days} {}", if days == 1 { "day" } else { "days" })
    }
}

#[cfg(test)]
mod tests;
