// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the computer's chain plays at, and what the bypass holds: the values and the words every
//! front end shows, made once here so that none of them works a level out for itself.

use serde::{Deserialize, Serialize};

/// The levels the computer's chain plays at, in dB (negative is quieter). Every field defaults
/// for a peer that sends none.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChainLevels {
    /// The EQ's share of the preamp sent: the preamp less crossfeed's cost.
    pub eq_db: f64,
    /// What the space stage does: crossfeed's cost (negative), or the chosen set's level
    /// against Off while the virtual speakers play; 0 with neither.
    pub space_db: f64,
    /// The whole chain: the preamp sent plus the speakers' level. Loudness is left out (it
    /// follows the computer's volume).
    pub total_db: f64,
    /// Where a climb in progress ends, in dB; `None` with no climb.
    pub climb_to_db: Option<f64>,
    /// Whether the chain runs as a plain pass-through (the EQ and every stage off).
    pub passthrough: bool,
    /// Whether the chain is proven untouched: a judgement only the host check may earn, so
    /// false unless the daemon says so.
    pub untouched: bool,
    /// The words for these levels ([`ChainLevels::words_for`]); empty with no chain.
    pub words: String,
}

impl ChainLevels {
    /// The levels of a chain that runs (`chain`) or not, with their words.
    pub fn of(chain: bool, eq_db: f64, space_db: f64, total_db: f64, passthrough: bool) -> Self {
        let mut levels = ChainLevels {
            eq_db,
            space_db,
            total_db,
            passthrough,
            ..ChainLevels::default()
        };
        levels.words = levels.words_for(chain);
        levels
    }

    /// These levels with a climb in progress that ends at `climb_to_db` and the chain's
    /// untouched judgement, the words made again.
    pub fn with_climb(mut self, climb_to_db: Option<f64>, untouched: bool) -> Self {
        self.climb_to_db = climb_to_db;
        self.untouched = untouched && self.passthrough;
        self.words = self.words_for(true);
        self
    }

    /// The one sentence for the levels: nothing with no chain, `plays -14.8 dB in all`, or
    /// `pass-through -7.6 dB until the music pauses` for a pass-through that holds a cut, and
    /// `pass-through 0.0 dB` for one at 0 dB.
    pub fn words_for(&self, chain: bool) -> String {
        if !chain {
            return String::new();
        }
        if self.untouched {
            return "pass-through, untouched".to_string();
        }
        if let Some(to) = self.climb_to_db {
            return format!("climbing to {} dB", db_word(to));
        }
        let total = db_word(self.total_db);
        match (self.passthrough, total == "0.0") {
            (true, true) => "pass-through 0.0 dB".to_string(),
            (true, false) => format!("pass-through {total} dB until the music pauses"),
            (false, _) => format!("plays {total} dB in all"),
        }
    }
}

/// `level` to one decimal with a true minus sign, and never `-0.0`.
pub fn db_word(level: f64) -> String {
    let rounded = (level * 10.0).round() / 10.0;
    if rounded == 0.0 {
        return "0.0".to_string();
    }
    let text = format!("{:.1}", rounded.abs());
    match rounded < 0.0 {
        true => format!("\u{2212}{text}"),
        false => text,
    }
}

/// Which stages played when the bypass turned them off, to be brought back as they were.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeldStages {
    /// The EQ.
    pub eq: bool,
    /// The loudness.
    pub loudness: bool,
    /// Crossfeed.
    pub crossfeed: bool,
    /// The virtual speakers.
    pub speakers: bool,
}

impl HeldStages {
    /// Whether no stage is held.
    pub fn is_empty(&self) -> bool {
        *self == HeldStages::default()
    }
}

/// The bypass as a screen shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BypassView {
    /// Whether the bypass is on: every stage off, the held ones waiting to come back.
    pub on: bool,
    /// The stages it brings back; all false while it is off.
    pub held: HeldStages,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_are_the_approved_ones() {
        assert_eq!(ChainLevels::of(false, 0.0, 0.0, 0.0, false).words, "");
        assert_eq!(
            ChainLevels::of(true, -7.8, -7.0, -14.8, false).words,
            "plays \u{2212}14.8 dB in all"
        );
        assert_eq!(
            ChainLevels::of(true, -7.6, 0.0, -7.6, true).words,
            "pass-through \u{2212}7.6 dB until the music pauses"
        );
    }

    #[test]
    fn a_passthrough_at_zero_db_is_not_held_by_the_music() {
        assert_eq!(
            ChainLevels::of(true, 0.0, 0.0, 0.0, true).words,
            "pass-through 0.0 dB"
        );
        assert_eq!(
            ChainLevels::of(true, 0.0, 0.0, -0.04, true).words,
            "pass-through 0.0 dB"
        );
    }

    #[test]
    fn a_level_never_reads_minus_zero() {
        assert_eq!(db_word(-0.04), "0.0");
        assert_eq!(db_word(-0.05), "\u{2212}0.1");
        assert_eq!(db_word(1.25), "1.3");
    }
}
