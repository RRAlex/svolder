// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The link changes of one device over the last day, and the wall clock they are stamped with.
//!
//! A front end asks for them through the device's handle.

use std::collections::VecDeque;

use svolder_ipc::LinkChange;

/// The most link changes a device's history holds.
pub(super) const HISTORY_CAP: usize = 4096;

/// How long a link change stays in a device's history, in milliseconds.
pub(super) const HISTORY_SPAN_MS: u64 = 24 * 3600 * 1000;

/// The unix wall clock in milliseconds, which every time the history records or reports is
/// read through: a history crosses processes, so it is stamped with the clock they share.
pub(crate) fn wall_ms() -> u64 {
    #[cfg(test)]
    if let Some(ms) = TEST_CLOCK.with(std::cell::Cell::get) {
        return ms;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
thread_local! {
    /// The wall clock a test holds [`wall_ms`] at; `None` reads the real one. It is per
    /// thread: a test on a multi-thread runtime sets it on the thread that reads, or runs
    /// on a current-thread runtime.
    pub(crate) static TEST_CLOCK: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// The changes of one device's link, oldest first, at most [`HISTORY_CAP`] of them, held for
/// a day when the newest was recorded, plus the change before that day.
#[derive(Debug, Default)]
pub struct History {
    ring: VecDeque<LinkChange>,
}

impl History {
    /// Append a change at `at_ms`, then drop what the cap and the day's age no longer hold.
    /// A clock that stepped back only leaves older stamps behind newer ones; the ring never
    /// reorders them.
    pub(super) fn record(&mut self, at_ms: u64, connected: bool) {
        self.ring.push_back(LinkChange { at_ms, connected });
        let oldest = at_ms.saturating_sub(HISTORY_SPAN_MS);
        // The newest change older than the cutoff stays: it says how the link stood when the
        // day began.
        while self.ring.get(1).is_some_and(|t| t.at_ms < oldest) {
            self.ring.pop_front();
        }
        while self.ring.len() > HISTORY_CAP {
            self.ring.pop_front();
        }
    }

    /// The changes inside the last `span_secs` seconds before `now_ms` (a day at most), and
    /// the one before them, which says how the span began. A change stamped after `now_ms`
    /// is returned as it is.
    pub(super) fn within(&self, now_ms: u64, span_secs: u64) -> Vec<LinkChange> {
        let span_ms = span_secs.min(HISTORY_SPAN_MS / 1000) * 1000;
        let start = now_ms.saturating_sub(span_ms);
        let first = self
            .ring
            .iter()
            .position(|t| t.at_ms >= start)
            .unwrap_or(self.ring.len());
        self.ring
            .iter()
            .skip(first.saturating_sub(1))
            .copied()
            .collect()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.ring.len()
    }

    #[cfg(test)]
    pub(super) fn oldest_ms(&self) -> Option<u64> {
        self.ring.front().map(|t| t.at_ms)
    }
}
