// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use svolder_profiles::history::Aggregate;
use svolder_proto::{Message, Value};

use super::*;
use crate::test_support::profile_named;

/// A raw status of `key` at `raw`.
fn status(profile: &svolder_profiles::Profile, key: &str, value: Value) -> Message {
    Message::status(profile.control_by_key(key).expect("a control").id, value)
}

/// The ring answers every function's samples within the span (and the newest from before it), at
/// wall-clock times worked out from their ages, a bool as 0 or 1, with the profile's key and the
/// series' aggregate.
#[test]
fn the_ring_answers_its_samples_at_wall_clock_times() {
    let profile = profile_named("gungnir2-v2");
    let t0 = Instant::now();
    let mut ring = Ring::default();
    let at = |secs| t0 + Duration::from_secs(secs);
    ring.record(&profile, &status(&profile, "volume", Value::U8(50)), t0);
    ring.record(
        &profile,
        &status(&profile, "spdif_locked", Value::Bool(true)),
        at(1),
    );
    ring.record(&profile, &status(&profile, "volume", Value::U8(60)), at(2));
    let out = ring.series_out(Some(&profile), at(3), 100_000, Duration::from_secs(60));
    let volume = out
        .iter()
        .find(|s| s.key.as_deref() == Some("volume"))
        .expect("the volume");
    assert_eq!(volume.points, [(97_000, 50.0), (99_000, 60.0)]);
    assert_eq!(volume.aggregate, Aggregate::Max);
    assert_eq!(
        volume.id,
        profile.control_by_key("volume").unwrap().id.to_string()
    );
    let locked = out
        .iter()
        .find(|s| s.key.as_deref() == Some("spdif_locked"))
        .expect("the lock");
    assert_eq!(locked.points, [(98_000, 1.0)]);
    assert_eq!(locked.aggregate, Aggregate::Last);
    let recent = ring.series_out(Some(&profile), at(3), 100_000, Duration::from_millis(1500));
    let volume = recent
        .iter()
        .find(|s| s.key.as_deref() == Some("volume"))
        .expect("the volume");
    assert_eq!(volume.points, [(97_000, 50.0), (99_000, 60.0)]);
    let locked = recent
        .iter()
        .find(|s| s.key.as_deref() == Some("spdif_locked"))
        .expect("the lock, carried from before the span");
    assert_eq!(locked.points, [(98_000, 1.0)]);
}

/// A status that is no number (a data payload) is not recorded, and with no profile the keys are
/// unknown but the samples are still there.
#[test]
fn data_is_not_recorded_and_no_profile_leaves_the_keys_unknown() {
    let profile = profile_named("gungnir2-v2");
    let t0 = Instant::now();
    let mut ring = Ring::default();
    let data = Message::status("99".parse().unwrap(), Value::Data(vec![1, 2]));
    ring.record(&profile, &data, t0);
    ring.record(&profile, &status(&profile, "input", Value::U8(3)), t0);
    let out = ring.series_out(None, t0, 5_000, Duration::from_secs(60));
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].points, [(5_000, 3.0)]);
}

/// The ring answers the recalls inside the span, at wall-clock times, and forgets one older
/// than half an hour.
#[test]
fn the_ring_answers_its_recalls_inside_the_span() {
    let t0 = Instant::now();
    let mut ring = Ring::default();
    ring.recalled("Warm", t0);
    ring.recalled("Bass", t0 + Duration::from_secs(2));
    let out = ring.recalls_out(
        t0 + Duration::from_secs(3),
        100_000,
        Duration::from_secs(60),
    );
    assert_eq!(
        out,
        [
            svolder_ipc::RecallOut {
                at_ms: 97_000,
                name: "Warm".into()
            },
            svolder_ipc::RecallOut {
                at_ms: 99_000,
                name: "Bass".into()
            },
        ]
    );
    ring.recalled("Presence", t0 + Duration::from_secs(30 * 60 + 1));
    let kept = ring.recalls_out(
        t0 + Duration::from_secs(30 * 60 + 1),
        0,
        Duration::from_secs(3600),
    );
    assert_eq!(
        kept.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["Bass", "Presence"]
    );
}

/// The recall list holds at most as many entries as a series does, the oldest going first.
#[test]
fn the_recall_list_is_bounded_by_the_windows_capacity() {
    let t0 = Instant::now();
    let mut ring = Ring::default();
    for i in 0..(Window::DAEMON.capacity + 50) {
        ring.recalled(&format!("preset {i}"), t0);
    }
    let out = ring.recalls_out(t0, 1_000_000, Duration::from_secs(60));
    assert_eq!(out.len(), Window::DAEMON.capacity);
    assert_eq!(out[0].name, "preset 50");
}

/// A series answers the newest sample from before the span first, stamped at its own time, then
/// the samples inside the span; one with nothing inside the span but a sample before it is
/// answered with that one point.
#[test]
fn a_series_carries_the_newest_sample_from_before_the_span() {
    let profile = profile_named("gungnir2-v2");
    let t0 = Instant::now();
    let mut ring = Ring::default();
    let at = |secs| t0 + Duration::from_secs(secs);
    ring.record(&profile, &status(&profile, "volume", Value::U8(40)), t0);
    ring.record(&profile, &status(&profile, "volume", Value::U8(50)), at(2));
    ring.record(&profile, &status(&profile, "input", Value::U8(3)), at(1));
    ring.record(&profile, &status(&profile, "volume", Value::U8(60)), at(10));
    let out = ring.series_out(Some(&profile), at(12), 100_000, Duration::from_secs(5));
    let volume = out
        .iter()
        .find(|s| s.key.as_deref() == Some("volume"))
        .expect("the volume");
    assert_eq!(volume.points, [(90_000, 50.0), (98_000, 60.0)]);
    let input = out
        .iter()
        .find(|s| s.key.as_deref() == Some("input"))
        .expect("the input is held");
    assert_eq!(input.points, [(89_000, 3.0)]);
    let held = ring.series_out(Some(&profile), at(30), 100_000, Duration::from_secs(1));
    let volume = held
        .iter()
        .find(|s| s.key.as_deref() == Some("volume"))
        .expect("the volume is held");
    assert_eq!(volume.points, [(80_000, 60.0)]);
}

/// An input reported 31 minutes ago and changed now: the ring still holds the old value, so the
/// series answers it as the point that says what held before the change.
#[test]
fn a_value_held_beyond_the_keep_is_carried_into_its_change() {
    let profile = profile_named("gungnir2-v2");
    let t0 = Instant::now();
    let mut ring = Ring::default();
    let at = |secs| t0 + Duration::from_secs(secs);
    ring.record(&profile, &status(&profile, "input", Value::U8(1)), t0);
    ring.record(
        &profile,
        &status(&profile, "input", Value::U8(3)),
        at(31 * 60),
    );
    let out = ring.series_out(
        Some(&profile),
        at(31 * 60 + 1),
        10_000_000,
        Duration::from_secs(3),
    );
    let input = out
        .iter()
        .find(|s| s.key.as_deref() == Some("input"))
        .expect("the input");
    assert_eq!(
        input.points,
        [(10_000_000 - 31 * 60_000 - 1_000, 1.0), (9_999_000, 3.0)]
    );
}

/// Recording a whole state at one instant gives each number one sample, and the same state
/// again at a later one adds no change to what the strip draws.
#[test]
fn a_whole_state_is_recorded_at_one_instant() {
    let profile = profile_named("gungnir2-v2");
    let t0 = Instant::now();
    let mut ring = Ring::default();
    let state = [
        status(&profile, "volume", Value::U8(50)),
        status(&profile, "spdif_locked", Value::Bool(true)),
    ];
    ring.record_all(&profile, &state, t0);
    ring.record_all(&profile, &state, t0 + Duration::from_secs(5));
    let out = ring.series_out(
        Some(&profile),
        t0 + Duration::from_secs(6),
        100_000,
        Duration::from_secs(60),
    );
    assert_eq!(out.len(), 2);
    for series in out {
        let values: std::collections::BTreeSet<u64> =
            series.points.iter().map(|(_, v)| *v as u64).collect();
        assert_eq!(values.len(), 1, "{series:?}");
    }
}
