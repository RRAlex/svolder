// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The terminal's input with its answers taken out.
//!
//! The probe asks the terminal what it draws, and a terminal may answer after the probe gave up
//! (or answer a question another program asked). The answer to the kitty graphics query is an
//! APC string and the terminal's name and version a DCS string, which the input reader does not
//! know: it reads `ESC _` as the chord Alt+`_`, the string's letters as keys, and the closing
//! `ESC \` as Alt+`\`. [`Swallow`] takes a string from its opener to its closer out of the
//! events, and [`Filtered`] puts it in front of the reader, so every reader of the input (the
//! intro, the waits, the loop, the wizard) sees keys only.
//!
//! A hand that presses Alt+`P` opens a string nobody closes. The swallow ends there on its own:
//! it holds only while the next event follows the last within [`GAP`], and a terminal writes
//! an answer in one piece.

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::Stream;

/// How long after the last event of an answer the next may come and still be part of it.
const GAP: Duration = Duration::from_millis(100);

/// How soon after a bare Escape an opening letter must come to be the rest of an answer whose
/// Escape was read alone.
const SPLIT: Duration = Duration::from_millis(10);

/// The letters after an Escape that open a string: APC (`_`), DCS (`P`), SOS (`X`) and PM (`^`).
const OPENERS: [char; 4] = ['_', 'P', 'X', '^'];

/// Where a string is.
#[derive(Debug, Clone, Copy, Default)]
enum State {
    /// Between strings.
    #[default]
    Between,
    /// The last event was an Escape read alone, at this instant.
    Escaped(Instant),
    /// Inside a string whose last event came at this instant.
    Inside(Instant),
}

/// Takes the events of a string out of the input, one event at a time.
#[derive(Debug, Default)]
pub(super) struct Swallow {
    state: State,
}

/// The letter a key is, with only Alt (and Shift) held: `Some((letter, alt))`.
fn letter(key: &KeyEvent) -> Option<(char, bool)> {
    let KeyCode::Char(c) = key.code else {
        return None;
    };
    let held = key.modifiers - KeyModifiers::SHIFT;
    match held {
        m if m.is_empty() => Some((c, false)),
        m if m == KeyModifiers::ALT => Some((c, true)),
        _ => None,
    }
}

impl Swallow {
    /// Whether `event`, which came at `at`, is to be read: false for the events of a string.
    pub(super) fn admit(&mut self, event: &Event, at: Instant) -> bool {
        let Event::Key(key) = event else {
            return true;
        };
        let held = letter(key);
        // A string that has gone quiet is over, whether or not it was closed.
        if let State::Inside(last) = self.state
            && at.saturating_duration_since(last) > GAP
        {
            self.state = State::Between;
        }
        match self.state {
            State::Inside(_) => {
                // The closer is `ESC \`, read as Alt+`\`, or as a plain `\` when the Escape
                // was read alone.
                let closes = matches!(held, Some(('\\', _)));
                self.state = if closes {
                    State::Between
                } else {
                    State::Inside(at)
                };
                false
            }
            State::Escaped(since) if at.saturating_duration_since(since) <= SPLIT => {
                if let Some((c, false)) = held
                    && OPENERS.contains(&c)
                {
                    self.state = State::Inside(at);
                    return false;
                }
                self.after(key, at);
                true
            }
            State::Between | State::Escaped(_) => {
                if let Some((c, true)) = held
                    && OPENERS.contains(&c)
                {
                    self.state = State::Inside(at);
                    return false;
                }
                self.after(key, at);
                true
            }
        }
    }

    /// Note that `key` passed: a bare Escape may be the start of an answer read in two pieces.
    fn after(&mut self, key: &KeyEvent, at: Instant) {
        self.state = match key.code == KeyCode::Esc && key.modifiers.is_empty() {
            true => State::Escaped(at),
            false => State::Between,
        };
    }
}

/// An event stream with the terminal's answers taken out.
pub(in crate::tui) struct Filtered<S> {
    inner: S,
    swallow: Swallow,
}

impl<S> Filtered<S> {
    /// `inner` with its answers taken out.
    pub(in crate::tui) fn new(inner: S) -> Self {
        Filtered {
            inner,
            swallow: Swallow::default(),
        }
    }
}

/// The terminal's own input, as the input reader parses it, with the terminal's answers taken
/// out.
pub(in crate::tui) fn events() -> Filtered<EventStream> {
    Filtered::new(EventStream::new())
}

impl<S: Stream<Item = std::io::Result<Event>> + Unpin> Stream for Filtered<S> {
    type Item = std::io::Result<Event>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(event))) => {
                    if self.swallow.admit(&event, Instant::now()) {
                        return Poll::Ready(Some(Ok(event)));
                    }
                }
                other => return other,
            }
        }
    }
}

#[cfg(test)]
mod tests;
