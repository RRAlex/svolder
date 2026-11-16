// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A span past the ring's half hour is cut to it: the link changes the answer carries are those
/// inside 1,800 s and the one before them, never the older ones.
#[tokio::test]
async fn a_readings_span_past_the_ring_is_cut_to_its_half_hour() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        let now = history::wall_ms();
        history::TEST_CLOCK.with(|clock| clock.set(Some(now)));
        let link = Arc::clone(&locked(&handle.info).history);
        *locked(&link) = history::History::default();
        for (ago_s, up) in [(3000, true), (1900, false), (100, true)] {
            locked(&link).record(now - ago_s * 1000, up);
        }
        let Response::Readings { link, bin_ms, .. } = handle.readings(u64::MAX) else {
            panic!("not a readings answer");
        };
        assert_eq!(bin_ms, 1000);
        let ago: Vec<u64> = link.iter().map(|c| (now - c.at_ms) / 1000).collect();
        assert_eq!(ago, [1900, 100]);
        history::TEST_CLOCK.with(|clock| clock.set(None));
    })
    .await
    .expect("test timed out");
}

/// A readings span with nothing reported in it still says what held at its start: the series
/// carry their newest sample from before the span, and the link its last change.
#[test]
fn a_readings_span_carries_what_held_at_its_start() {
    let handle = bare_handle("dac", "UNIT", None);
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("a built-in")
        .clone();
    let volume = profile.control_by_key("volume").expect("a volume").id;
    let now = std::time::Instant::now();
    let ring = Arc::clone(&locked(&handle.info).ring);
    locked(&ring).record(
        &profile,
        &svolder_proto::Message::status(volume, svolder_proto::Value::U8(50)),
        now - Duration::from_secs(10),
    );
    let link = Arc::clone(&locked(&handle.info).history);
    locked(&link).record(history::wall_ms() - 10_000, false);
    let Response::Readings { series, link, .. } = handle.readings(1) else {
        panic!("not a readings answer");
    };
    let [volume] = series.as_slice() else {
        panic!("expected the volume alone: {series:?}");
    };
    assert_eq!(volume.points.len(), 1, "{volume:?}");
    assert_eq!(volume.points[0].1, 50.0);
    let [change] = link.as_slice() else {
        panic!("expected the change before the span: {link:?}");
    };
    assert!(!change.connected);
}

/// Every point of the series for `key` in the daemon's readings answer, oldest first.
fn points_of(handle: &DeviceHandle, key: &str) -> Vec<f64> {
    let profile = handle.feed.borrow().profile.clone().expect("a profile");
    let id = profile
        .control_by_key(key)
        .expect("a control")
        .id
        .to_string();
    let Response::Readings { series, .. } = handle.readings(600) else {
        panic!("not a readings answer");
    };
    series
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.points.into_iter().map(|(_, v)| v).collect())
        .unwrap_or_default()
}

/// Every point of the volume series in the daemon's readings answer.
fn volume_points(handle: &DeviceHandle) -> Vec<f64> {
    points_of(handle, "volume")
}

/// The number of the feed's readings that are numbers (a bool counts as one).
fn numeric_readings(handle: &DeviceHandle) -> usize {
    handle
        .feed
        .borrow()
        .readings
        .iter()
        .filter(|m| matches!(m.value, svolder_proto::Value::Bool(_)) || m.value.as_i64().is_some())
        .count()
}

/// A connect's own refresh reaches the readings: a device that connects and reports nothing
/// more has its volume, and every other number the session holds, in the answer.
#[tokio::test]
async fn a_connect_records_what_the_refresh_read() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        let Response::Readings { series, .. } = handle.readings(600) else {
            panic!("not a readings answer");
        };
        let held = numeric_readings(&handle);
        assert!(held > 1, "the fake holds {held} numbers");
        assert!(!volume_points(&handle).is_empty(), "no volume: {series:?}");
        assert_eq!(series.len(), held, "a series for each number held");
    })
    .await
    .expect("test timed out");
}

/// Sever the link and wait for the actor to connect again.
async fn reconnected(transport: &Shows, handle: &DeviceHandle) {
    let before = transport.inner.connects();
    transport.inner.sever();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if transport.inner.connects() > before && handle.describe().connected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the actor never reconnected");
    // The actor records the connect's readings before it marks the link up, so the link
    // being up means they are in the ring.
}

/// A reconnect with the values unchanged records them again at the volume held, and one that
/// found the volume changed while the link was down records the new value after the old.
#[tokio::test]
async fn a_reconnect_records_what_the_refresh_read() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        let held = *volume_points(&handle).last().expect("the connect's volume");
        reconnected(&transport, &handle).await;
        let after = volume_points(&handle);
        assert_eq!(after.last(), Some(&held), "{after:?}");
        assert!(after.iter().all(|v| *v == held), "{after:?}");

        let profile = handle.feed.borrow().profile.clone().expect("a profile");
        let id = profile.control_by_key("volume").expect("a volume").id;
        locked(&transport.inner.device()).set_raw(id, svolder_proto::Value::U8(held as u8 + 3));
        reconnected(&transport, &handle).await;
        let changed = volume_points(&handle);
        assert_eq!(changed.last(), Some(&(held + 3.0)), "{changed:?}");
        assert_eq!(changed.iter().filter(|v| **v == held + 3.0).count(), 1);
    })
    .await
    .expect("test timed out");
}

/// The strip's marks for the readings the daemon answers now.
fn marks_now(handle: &DeviceHandle) -> Vec<(u64, String)> {
    let profile = handle.feed.borrow().profile.clone().expect("a profile");
    let Response::Readings {
        now_ms,
        series,
        recalls,
        link,
        ..
    } = handle.readings(600)
    else {
        panic!("not a readings answer");
    };
    let past = crate::web::view::Past {
        now_ms,
        series: &series,
        recalls: &recalls,
        link: &link,
    };
    crate::web::view::strip(&profile, &past, 600)
        .marks
        .into_iter()
        .map(|m| (m.t, m.text))
        .collect()
}

/// A reconnect that finds the sample rate changed while the link was down draws exactly one
/// mark, in the second of the reconnect; an input that is as it was draws none.
#[tokio::test]
async fn a_change_while_away_marks_the_reconnect_once() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        assert!(marks_now(&handle).is_empty(), "{:?}", marks_now(&handle));
        let held = *points_of(&handle, "sample_rate").last().expect("a rate");
        let profile = handle.feed.borrow().profile.clone().expect("a profile");
        let id = profile.control_by_key("sample_rate").expect("a rate").id;
        locked(&transport.inner.device()).set_raw(id, svolder_proto::Value::U8(held as u8 + 1));
        // The connect's second is over, so the reconnect's reading is a bin of its own.
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let before = history::wall_ms() / 1000;
        reconnected(&transport, &handle).await;
        let after = history::wall_ms() / 1000;
        // Let the reconnect's second end, so the strip covers it.
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let marks = marks_now(&handle);
        let [(t, _)] = marks.as_slice() else {
            panic!("expected one mark: {marks:?}");
        };
        assert!(
            (before..=after).contains(t),
            "{t} outside {before}..={after}: {marks:?}"
        );
    })
    .await
    .expect("test timed out");
}
