// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the terminal's answers look like once the input reader has parsed them as keys, and
//! what is left of the input once they are taken out.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;

use super::*;

fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

fn plain(c: char) -> Event {
    key(KeyCode::Char(c), KeyModifiers::NONE)
}

fn alt(c: char) -> Event {
    key(KeyCode::Char(c), KeyModifiers::ALT)
}

fn esc() -> Event {
    key(KeyCode::Esc, KeyModifiers::NONE)
}

/// The keys the reader makes of the string `ESC _ G i=31;OK ESC \`, one read: `Alt+_`, the
/// letters, `Alt+\`.
fn apc_as_read() -> Vec<Event> {
    let mut keys = vec![alt('_')];
    keys.extend("Gi=31;OK".chars().map(plain));
    keys.push(alt('\\'));
    keys
}

/// The same for `ESC P > | xterm(388) ESC \`.
fn dcs_as_read() -> Vec<Event> {
    let mut keys = vec![key(
        KeyCode::Char('P'),
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    )];
    keys.extend(">|xterm(388)".chars().map(plain));
    keys.push(alt('\\'));
    keys
}

/// What `swallow` lets through of `events`, which arrive `gap` apart.
fn passed(swallow: &mut Swallow, events: &[Event], gap: Duration) -> Vec<Event> {
    let start = Instant::now();
    events
        .iter()
        .enumerate()
        .filter(|(at, event)| swallow.admit(event, start + gap * *at as u32))
        .map(|(_, event)| event.clone())
        .collect()
}

const FAST: Duration = Duration::from_micros(50);

#[test]
fn an_apc_answer_is_taken_out_whole() {
    let mut swallow = Swallow::default();
    assert_eq!(passed(&mut swallow, &apc_as_read(), FAST), []);
}

#[test]
fn a_dcs_answer_is_taken_out_whole() {
    let mut swallow = Swallow::default();
    assert_eq!(passed(&mut swallow, &dcs_as_read(), FAST), []);
}

#[test]
fn the_keys_around_an_answer_pass() {
    let mut swallow = Swallow::default();
    let mut events = vec![plain('a')];
    events.extend(apc_as_read());
    events.push(plain('b'));
    assert_eq!(
        passed(&mut swallow, &events, FAST),
        [plain('a'), plain('b')]
    );
}

#[test]
fn an_answer_sent_with_its_escape_and_its_opener_apart_is_taken_out_too() {
    // A read that ended between the two bytes: the bare Escape came first.
    let mut swallow = Swallow::default();
    let mut events = vec![esc(), plain('_')];
    events.extend("Gi=31;OK".chars().map(plain));
    events.push(esc());
    events.push(plain('\\'));
    events.push(plain('b'));
    assert_eq!(passed(&mut swallow, &events, FAST), [esc(), plain('b')]);
}

#[test]
fn an_escape_then_an_underscore_typed_by_a_hand_is_two_keys() {
    let mut swallow = Swallow::default();
    let events = [esc(), plain('_'), plain('a')];
    assert_eq!(
        passed(&mut swallow, &events, Duration::from_millis(200)),
        events
    );
}

#[test]
fn a_chord_that_opens_a_string_nobody_ends_lets_the_next_keys_through() {
    // Alt+P pressed by a hand: the string never ends, and the next key comes much later.
    let mut swallow = Swallow::default();
    let events = [
        key(KeyCode::Char('P'), KeyModifiers::ALT | KeyModifiers::SHIFT),
        plain('q'),
    ];
    assert_eq!(
        passed(&mut swallow, &events, Duration::from_millis(300)),
        [plain('q')]
    );
}

#[test]
fn ordinary_keys_and_other_chords_pass() {
    let mut swallow = Swallow::default();
    let events = [
        plain('P'),
        plain('_'),
        alt('x'),
        alt('\\'),
        key(KeyCode::Enter, KeyModifiers::NONE),
        key(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ];
    assert_eq!(passed(&mut swallow, &events, FAST), events);
}

#[test]
fn a_second_answer_is_taken_out_as_the_first_was() {
    let mut swallow = Swallow::default();
    let mut events = apc_as_read();
    events.extend(dcs_as_read());
    events.push(plain('q'));
    assert_eq!(passed(&mut swallow, &events, FAST), [plain('q')]);
}

#[test]
fn mouse_and_size_events_inside_an_answer_pass() {
    let mut swallow = Swallow::default();
    let resize = Event::Resize(80, 24);
    let mut events = vec![alt('_'), plain('G')];
    events.push(resize.clone());
    events.push(alt('\\'));
    assert_eq!(passed(&mut swallow, &events, FAST), [resize]);
}

#[tokio::test]
async fn the_stream_hands_on_what_the_swallow_lets_through() {
    let mut events = vec![plain('a')];
    events.extend(apc_as_read());
    events.push(plain('b'));
    let inner = futures::stream::iter(events.into_iter().map(Ok));
    let got: Vec<Event> = Filtered::new(inner)
        .map(|event| event.expect("no error"))
        .collect()
        .await;
    assert_eq!(got, [plain('a'), plain('b')]);
}

#[tokio::test]
async fn the_stream_hands_on_an_error_and_its_end() {
    let inner = futures::stream::iter(vec![Ok(plain('a')), Err(std::io::Error::other("gone"))]);
    let mut filtered = Filtered::new(inner);
    assert!(matches!(filtered.next().await, Some(Ok(_))));
    assert!(matches!(filtered.next().await, Some(Err(_))));
    assert!(filtered.next().await.is_none());
}
