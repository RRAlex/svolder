// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The idle advice: what `a` says when no fault stands.
//!
//! One line: how long all has been clear, then a tip from the key table and the mouse lines
//! the help overlay lists, or on every fifth press a quip for the device's family. A tip is
//! generated from the same tables the help overlay reads, so it names only keys that exist.

use std::time::{Duration, Instant};

use svolder_profiles::Family;

use super::App;
use crate::tui::decoration::mascot::looks::Season;
use crate::tui::keys;
use crate::tui::screens::device::has_fault_controls;
use crate::tui::screens::help::TONE_GESTURES;

/// Every this many presses a quip stands in for the tip.
const QUIP_EVERY: usize = 5;
/// The quip of a device whose Mode reads a tube.
const TUBE_QUIP: &str = "Tubes warm, nothing to report.";

/// How long `elapsed` is, in words: `under a minute`, `N min`, `H h M min`, `D d H h`.
fn since(elapsed: Duration) -> String {
    let minutes = elapsed.as_secs() / 60;
    let (hours, days) = (minutes / 60, minutes / 1440);
    if minutes == 0 {
        "under a minute".to_string()
    } else if hours == 0 {
        format!("{minutes} min")
    } else if days == 0 {
        format!("{hours} h {} min", minutes % 60)
    } else {
        format!("{days} d {} h", hours % 24)
    }
}

/// Every this many idle presses of `a` the mascot has a quip, where it may.
const MASCOT_QUIP_EVERY: usize = 10;

/// The mascot's quips, in the order they rotate.
const MASCOT_QUIPS: [&str; 4] = [
    "Ratatoskr ran the whole tree. Nothing to report.",
    "Ratatoskr says the eagle and the serpent are bickering again. Your gear is fine.",
    "Ratatoskr checked every branch. All clear.",
    "Ratatoskr found an acorn in the EQ and left it there.",
];

/// Whether the mascot may joke: it is on and the day is not a quiet one.
fn mascot_quip_today(app: &App) -> bool {
    let quiet = app
        .mascot
        .today
        .and_then(|(month, day)| Season::on(month, day))
        .is_some_and(Season::quiet);
    app.mascot_on() && !quiet
}

/// The line `a` shows on the `press`th idle press (counted from 1) as of `now`.
pub(super) fn idle_line(app: &App, now: Instant, press: usize) -> String {
    let from = app
        .notices
        .fault_cleared_at
        .unwrap_or(app.notices.opened_at);
    let head = if has_fault_controls(app) {
        "All clear for"
    } else {
        "Connected for"
    };
    let time = since(now.saturating_duration_since(from));
    let tail = match press % QUIP_EVERY {
        0 if press.is_multiple_of(MASCOT_QUIP_EVERY) && mascot_quip_today(app) => {
            MASCOT_QUIPS[(press / MASCOT_QUIP_EVERY - 1) % MASCOT_QUIPS.len()].to_string()
        }
        0 => quip(app, press / QUIP_EVERY - 1),
        _ => format!("tip: {}", tip(press - 1 - press / QUIP_EVERY)),
    };
    format!("{head} {time} · {tail}")
}

/// Every tip, in order: the key table's keys, then the mouse lines.
fn tips() -> Vec<String> {
    let keys = keys::all()
        .iter()
        .map(|hint| format!("{} · {}", hint.label, hint.does));
    let mouse = TONE_GESTURES
        .iter()
        .map(|(gesture, does)| format!("{gesture} · {does}"));
    keys.chain(mouse).collect()
}

/// The `n`th tip, wrapping.
fn tip(n: usize) -> String {
    let tips = tips();
    tips[n % tips.len()].clone()
}

/// The quip for the device on `app`, the `n`th of its family's list, wrapping. A device whose
/// Mode reads a tube says the tube quip instead.
fn quip(app: &App, n: usize) -> String {
    if tube_mode(app) {
        return TUBE_QUIP.to_string();
    }
    let list: &[&str] = match app.profile.family {
        Family::Dac => &["Bits are flowing. Nothing to fix."],
        Family::Amp => &["Nothing to report. Enjoy the music."],
        Family::Preamp => &["Signal's clean. Nothing to fix."],
    };
    list[n % list.len()].to_string()
}

/// Whether the control keyed `mode` reads a value whose name contains `Tube`.
fn tube_mode(app: &App) -> bool {
    app.profile
        .controls
        .iter()
        .filter(|control| control.key == "mode")
        .filter_map(|control| {
            let raw = app.raw_of(control.id)?;
            control
                .values
                .iter()
                .find(|(value, _)| i64::from(*value) == raw)
        })
        .any(|(_, name)| name.contains("Tube"))
}

#[cfg(test)]
mod tests;
