// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Waiting on work with the alternate screen up: the keys that give up, and the spinner's clock.
//!
//! The terminal is in raw mode while the TUI opens a device or the setup wizard walks a step, so
//! Ctrl-C arrives as a key and a wait has to read it. [`waiting`] is that one wait, shared by the
//! start, the wizard and the device list's own waits.

use std::time::Instant;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use futures::stream::Stream;

use super::widgets::spinner;

/// What the frame between the last step and the device screen says.
pub(super) fn connecting(name: &str) -> String {
    format!("connecting to {name}…")
}

/// What is said when the user gives up on a device that has not answered yet.
pub(crate) const INTERRUPTED: &str = "interrupted before the device answered";

/// Wait for `opening` to answer, reading `events` while it does not.
///
/// The terminal is in raw mode by now, so Ctrl-C arrives as a key rather than as a signal:
/// without this the user would be held on a frozen frame for as long as the connect takes,
/// with nothing to end it. Ctrl-C and `q` — the key that quits the loop itself — end the wait
/// and answer `None`; every other event is read and dropped, and the device's own answer comes
/// back as `Some`.
pub(super) async fn wait_for_device<T, S>(
    opening: impl Future<Output = T>,
    events: &mut S,
) -> Option<T>
where
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    let mut reading = true;
    waiting(opening, events, &mut reading, |signal| match signal {
        WaitSignal::GaveUp => Interrupted::Stop,
        // Nothing is drawn while this wait runs, so there is nothing to turn a spinner behind.
        WaitSignal::Tick(_) => Interrupted::Carry,
    })
    .await
}

/// What a wait does when the user asks to leave part-way through it.
pub(super) enum Interrupted {
    /// Stop waiting: the work is dropped where it stands.
    Stop,
    /// Carry on waiting: what is under way is seen through, and the key honoured after it.
    Carry,
}

/// What [`waiting`] tells whoever it is waiting on behalf of, as it comes: one of the two keys
/// that give up on the wait, or the spinner's own clock turning to another moment.
pub(super) enum WaitSignal {
    /// One of the two keys that give up on the wait was pressed.
    GaveUp,
    /// The spinner's clock ticked to this moment, for the glyph a caller with something on the
    /// screen to turn it behind draws next; a caller with nothing on the screen leaves it be.
    Tick(Instant),
}

/// How many read errors in a row, with no event between them, a wait takes for a stream that
/// will only ever fail, and stops reading it.
pub(super) const STOP_AFTER_ERRORS: u32 = 5;

/// Wait for `work`, reading `events` while it has not answered and turning the spinner's clock
/// every hundred milliseconds while it runs.
///
/// Every wait this binary makes with the alternate screen up comes through here: the device the
/// TUI opens on, and each wait the setup wizard draws. The terminal is in raw mode, so Ctrl-C and
/// `q` (the give-up keys) arrive as keys, and what they and a spinner tick do is `on`'s to say: a
/// wait that stops on a key answers `None` with the work dropped, one that carries on answers
/// with the work's own value once it has one. Every other event is read and dropped.
///
/// `reading` is whether the terminal has anything left to say, kept by the caller: a stream that
/// has ended answers the moment it is asked, so the arm reading it stops being polled rather
/// than spinning the wait, and is not asked again by whatever waits next. A single read error is
/// dropped like any other event, so Ctrl-C and `q` still work after it; a stream that yields
/// [`STOP_AFTER_ERRORS`] errors in a row, with no event between them, is treated as ended.
pub(super) async fn waiting<T, S>(
    work: impl Future<Output = T>,
    events: &mut S,
    reading: &mut bool,
    mut on: impl FnMut(WaitSignal) -> Interrupted,
) -> Option<T>
where
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    tokio::pin!(work);
    let mut errors_in_a_row = 0;
    let mut ticks = tokio::time::interval(spinner::STEP);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            done = &mut work => return Some(done),
            event = events.next(), if *reading => match event {
                Some(Ok(Event::Key(key))) if gives_up(key) => {
                    errors_in_a_row = 0;
                    if matches!(on(WaitSignal::GaveUp), Interrupted::Stop) {
                        return None;
                    }
                }
                Some(Ok(_)) => errors_in_a_row = 0,
                Some(Err(_)) => {
                    errors_in_a_row += 1;
                    if errors_in_a_row >= STOP_AFTER_ERRORS {
                        *reading = false;
                    }
                }
                None => *reading = false,
            },
            tick = ticks.tick() => {
                on(WaitSignal::Tick(tick.into()));
            }
        }
    }
}

/// Whether `key` is one of the two that give up on a device that has not answered: Ctrl-C, or
/// the `q` the event loop itself quits on.
///
/// A release is the other half of a press that has already been acted on.
pub(super) fn gives_up(key: KeyEvent) -> bool {
    if key.kind == KeyEventKind::Release {
        return false;
    }
    match key.code {
        KeyCode::Char('c') => key.modifiers.contains(KeyModifiers::CONTROL),
        KeyCode::Char('q') => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
