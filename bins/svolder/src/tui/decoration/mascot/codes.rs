// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The codes typed into the help overlay and the Esc menu, and the matcher that reads them.

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;

/// A code typed into the help overlay or the Esc menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    Konami,
    Iddqd,
}

/// How long a key may wait for the next one before the code starts over.
const GAP: Duration = Duration::from_secs(2);

/// The keys that type [`Code::Konami`].
pub(crate) const KONAMI_KEYS: [KeyCode; 10] = [
    KeyCode::Up,
    KeyCode::Up,
    KeyCode::Down,
    KeyCode::Down,
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Char('b'),
    KeyCode::Char('a'),
];

/// The keys that type [`Code::Iddqd`].
const IDDQD_KEYS: [KeyCode; 5] = [
    KeyCode::Char('i'),
    KeyCode::Char('d'),
    KeyCode::Char('d'),
    KeyCode::Char('q'),
    KeyCode::Char('d'),
];

/// Every code and the keys that type it.
const CODES: [(Code, &[KeyCode]); 2] = [(Code::Konami, &KONAMI_KEYS), (Code::Iddqd, &IDDQD_KEYS)];

/// The keys typed so far toward a code and the instant of the last one.
#[derive(Debug, Clone, Default)]
pub(crate) struct Codes {
    typed: Vec<KeyCode>,
    last: Option<Instant>,
}

impl Codes {
    /// Take `key`, typed at `now`. Returns the code it completes. A key that breaks the
    /// sequence restarts from the longest tail of the keys typed (this one included) that begins
    /// a code; a key typed more than two seconds after the last starts from nothing.
    pub(crate) fn feed(&mut self, key: KeyCode, now: Instant) -> Option<Code> {
        if self.stale(now) {
            self.typed.clear();
        }
        self.last = Some(now);
        self.typed.push(key);
        // A key that breaks the sequence restarts from the longest tail of what was typed that
        // still begins a code.
        let start = (0..self.typed.len())
            .find(|&from| {
                CODES
                    .iter()
                    .any(|(_, keys)| keys.starts_with(&self.typed[from..]))
            })
            .unwrap_or(self.typed.len());
        self.typed.drain(..start);
        let done = CODES
            .iter()
            .find(|(_, keys)| **keys == *self.typed)
            .map(|(code, _)| *code);
        if done.is_some() {
            self.typed.clear();
        }
        done
    }

    /// Whether exactly `i d d` has been typed and is still fresh at `now`.
    pub(crate) fn holds_idd(&self, now: Instant) -> bool {
        !self.stale(now)
            && self.typed == [KeyCode::Char('i'), KeyCode::Char('d'), KeyCode::Char('d')]
    }

    /// Forget what has been typed.
    pub(crate) fn clear(&mut self) {
        self.typed.clear();
        self.last = None;
    }

    /// Whether more than the gap has passed since the last key.
    fn stale(&self, now: Instant) -> bool {
        self.last
            .is_some_and(|last| now.saturating_duration_since(last) > GAP)
    }
}
