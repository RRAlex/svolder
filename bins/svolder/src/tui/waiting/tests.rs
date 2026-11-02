// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::future::pending;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use futures::StreamExt;
use tokio::time::Instant;

use super::*;
use crate::tui::decoration::intro::test_event_stream;
use crate::tui::test_support::{ANSWERS_IN, key_press};

#[tokio::test(start_paused = true)]
async fn the_wait_ends_on_the_device_it_was_waiting_for() {
    let (mut events, _tx) = test_event_stream();
    let started = Instant::now();
    let opening = async {
        tokio::time::sleep(ANSWERS_IN).await;
        "gungnir"
    };
    assert_eq!(wait_for_device(opening, &mut events).await, Some("gungnir"));
    assert_eq!(started.elapsed(), ANSWERS_IN);
}

/// The terminal is in raw mode while the device is being opened, so Ctrl-C is a key like
/// any other: it, and the key the loop itself quits on, are what give up on a device that
/// is taking too long.
#[tokio::test(start_paused = true)]
async fn ctrl_c_and_q_give_up_on_a_device_that_has_not_answered() {
    for ending in [
        key_press(KeyCode::Char('c'), KeyModifiers::CONTROL),
        key_press(KeyCode::Char('q'), KeyModifiers::NONE),
    ] {
        let (mut events, tx) = test_event_stream();
        tx.send(ending).expect("the stream takes the key");
        assert_eq!(wait_for_device(pending::<u8>(), &mut events).await, None);
    }
}

/// One input error before the give-up key does not disarm it: the wait ends on the key,
/// not on the device that answers later.
#[tokio::test(start_paused = true)]
async fn ctrl_c_and_q_still_give_up_after_an_input_error() {
    for ending in [
        key_press(KeyCode::Char('c'), KeyModifiers::CONTROL),
        key_press(KeyCode::Char('q'), KeyModifiers::NONE),
    ] {
        let (mut events, tx) = test_event_stream();
        tx.send(Err(std::io::Error::other("a hiccup")))
            .expect("the stream takes the error");
        tx.send(ending).expect("the stream takes the key");
        let opening = async {
            tokio::time::sleep(ANSWERS_IN).await;
            7u8
        };
        assert_eq!(wait_for_device(opening, &mut events).await, None);
    }
}

/// Runs a wait on the device over `sent`, and answers how many of them the wait read.
async fn reads_of(sent: Vec<std::io::Result<Event>>) -> (Option<u8>, usize) {
    let (events, tx) = test_event_stream();
    for event in sent {
        tx.send(event).expect("the stream takes the event");
    }
    let read = std::cell::Cell::new(0);
    let mut events = events.inspect(|_| read.set(read.get() + 1));
    let opening = async {
        tokio::time::sleep(ANSWERS_IN).await;
        7u8
    };
    let answer = wait_for_device(opening, &mut events).await;
    (answer, read.get())
}

/// One read error, as a terminal that has gone away reports it.
fn failure() -> std::io::Result<Event> {
    Err(std::io::Error::other("the terminal went away"))
}

#[tokio::test(start_paused = true)]
async fn a_stream_that_only_fails_stops_being_read() {
    let (answer, read) = reads_of((0..20).map(|_| failure()).collect()).await;
    assert_eq!(answer, Some(7));
    assert_eq!(read, STOP_AFTER_ERRORS as usize);
}

#[tokio::test(start_paused = true)]
async fn an_event_between_errors_resets_the_count() {
    let mut sent = Vec::new();
    for _ in 0..3 {
        for _ in 0..STOP_AFTER_ERRORS - 1 {
            sent.push(failure());
        }
        sent.push(Ok(Event::Resize(100, 30)));
    }
    let total = sent.len();
    let (answer, read) = reads_of(sent).await;
    assert_eq!(answer, Some(7));
    assert_eq!(read, total);
}

/// Everything else a terminal reports — other keys, the release half of one that would
/// have ended the wait, a click, a resize, and the stream failing outright — is read and
/// dropped, and the wait stays the device's own.
#[tokio::test(start_paused = true)]
async fn every_other_event_is_read_and_dropped() {
    let (mut events, tx) = test_event_stream();
    let released = KeyEvent {
        kind: KeyEventKind::Release,
        ..KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
    };
    for event in [
        key_press(KeyCode::Esc, KeyModifiers::NONE),
        key_press(KeyCode::Char('x'), KeyModifiers::NONE),
        Ok(Event::Key(released)),
        Ok(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })),
        Ok(Event::Resize(100, 30)),
        Err(std::io::Error::other("the terminal went away")),
    ] {
        tx.send(event).expect("the stream takes the event");
    }
    let started = Instant::now();
    let opening = async {
        tokio::time::sleep(ANSWERS_IN).await;
        7u8
    };
    assert_eq!(wait_for_device(opening, &mut events).await, Some(7));
    assert_eq!(
        started.elapsed(),
        ANSWERS_IN,
        "the wait ended on something other than the device"
    );
}
