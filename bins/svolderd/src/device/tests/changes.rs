// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// Connecting, losing the link and regaining it record three transitions with their
/// wall-clock times, in order; saying the same state again records nothing.
#[tokio::test(start_paused = true)]
async fn the_actor_records_each_change_of_the_link() {
    let actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
        Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
        "0102030405060708".parse().unwrap(),
        3,
        Passphrase::from_text("1234").unwrap(),
    )))
    .await;
    actor.info.lock().unwrap().connected = false;
    let handle = DeviceHandle {
        name: "fake".into(),
        udid: "0102030405060708".into(),
        commands: mpsc::channel(1).0,
        writer: crate::writer::Writer::unattended(),
        events: actor.events.clone(),
        info: Arc::clone(&actor.info),
        feed: watch::channel(Feed::default()).1,
    };
    assert!(marks(&handle, 3600).is_empty(), "never connected: nothing");

    clock_at(1_000_000);
    actor.set_connected(true);
    actor.set_connected(true);
    clock_at(1_060_000);
    actor.set_connected(false);
    clock_at(1_120_000);
    actor.set_connected(true);
    actor.set_connected(true);

    assert_eq!(
        marks(&handle, 3600),
        vec![(1_000_000, true), (1_060_000, false), (1_120_000, true)]
    );
}

/// A panic while the device's info was locked poisons the lock, and every later change is still
/// taken: what `devices` reports never freezes at the moment of the panic, and the device still
/// answers to its unit id.
#[tokio::test(start_paused = true)]
async fn a_poisoned_info_lock_still_takes_every_change() {
    let actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
        Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
        "0102030405060708".parse().unwrap(),
        3,
        Passphrase::from_text("1234").unwrap(),
    )))
    .await;
    let info = Arc::clone(&actor.info);
    let panicked = std::thread::spawn(move || {
        let _held = info.lock().unwrap();
        panic!("a panic while the device's info is locked");
    })
    .join();
    assert!(panicked.is_err() && actor.info.is_poisoned());
    let handle = DeviceHandle {
        name: "fake".into(),
        udid: "0102030405060708".into(),
        commands: mpsc::channel(1).0,
        writer: crate::writer::Writer::unattended(),
        events: actor.events.clone(),
        info: Arc::clone(&actor.info),
        feed: watch::channel(Feed::default()).1,
    };
    actor.set_connected(false);
    assert!(!handle.describe().connected, "the lost link was dropped");
    actor.set_next_attempt(Some(tokio::time::Instant::now() + Duration::from_secs(5)));
    assert_eq!(handle.describe().retry_in_secs, Some(5));
    assert!(
        handle.matches("0102"),
        "a poisoned lock made the device unmatchable"
    );
}

/// The ring holds at most 4096 transitions, and nothing older than a day survives an insert.
#[test]
fn the_history_ring_is_capped_in_count_and_age() {
    let mut history = History::default();
    for i in 0..5000u64 {
        history.record(1_000 + i, i % 2 == 0);
    }
    assert_eq!(history.len(), HISTORY_CAP);
    assert_eq!(history.oldest_ms(), Some(1_000 + 5000 - HISTORY_CAP as u64));

    let day = 24 * 3600 * 1000;
    let mut history = History::default();
    history.record(10, true);
    history.record(20, false);
    history.record(day + 15, true);
    assert_eq!(history.len(), 3, "the entry at 10 ms stays as the anchor");
    assert_eq!(history.oldest_ms(), Some(10));
    history.record(day + 25, false);
    assert_eq!(history.len(), 3, "the one at 20 ms is the anchor now");
    assert_eq!(history.oldest_ms(), Some(20));
}

/// A span returns the transitions inside it plus the last one before it; a span over a day
/// is a day; a clock stepped back leaves a later-stamped transition in the reply as it is.
#[test]
fn a_history_span_carries_the_state_at_its_start() {
    let mut history = History::default();
    history.record(1_000_000, true);
    history.record(2_000_000, false);
    history.record(3_000_000, true);
    let now = 3_500_000;
    let at = |span_secs| -> Vec<u64> {
        history
            .within(now, span_secs)
            .iter()
            .map(|t| t.at_ms)
            .collect()
    };
    assert_eq!(at(400), vec![3_000_000,]);
    assert_eq!(at(1000), vec![2_000_000, 3_000_000]);
    assert_eq!(at(2000), vec![1_000_000, 2_000_000, 3_000_000]);
    assert_eq!(at(0), vec![3_000_000], "the state now, at least");
    // Far beyond a day clamps to a day: the ring is empty before 1_000_000 anyway, so the
    // clamp shows against a now a day and a bit later.
    let late = 1_000_000 + 25 * 3600 * 1000;
    let clamped: Vec<u64> = history
        .within(late, 10 * 24 * 3600)
        .iter()
        .map(|t| t.at_ms)
        .collect();
    assert_eq!(
        clamped,
        vec![3_000_000],
        "only the last one sits before the day's start"
    );

    // A clock stepped back: now is earlier than the newest transition.
    let back: Vec<u64> = history
        .within(1_500_000, 600)
        .iter()
        .map(|t| t.at_ms)
        .collect();
    assert_eq!(back, vec![1_000_000, 2_000_000, 3_000_000]);
    assert!(history.within(0, 600).iter().all(|t| t.at_ms > 0));
}

/// A link that came up and stayed up for more than a day keeps its anchor: a day-long span
/// still starts with the state the link was in.
#[test]
fn the_history_keeps_the_change_before_the_day() {
    let hour = 3600 * 1000;
    let t0 = 1_000 * hour;
    let mut history = History::default();
    history.record(t0, true);
    history.record(t0 + 25 * hour, false);
    let reply: Vec<(u64, bool)> = history
        .within(t0 + 26 * hour, 24 * 3600)
        .iter()
        .map(|t| (t.at_ms, t.connected))
        .collect();
    assert_eq!(reply, vec![(t0, true), (t0 + 25 * hour, false)]);
}
