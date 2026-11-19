// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use super::*;

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn minutes(n: u64) -> Duration {
    Duration::from_secs(n * 60)
}

/// A connection that passed, held open.
fn passed(activity: &Activity, ip: IpAddr) -> Counted {
    let counted = activity.accepted(ip);
    counted.pass();
    counted
}

/// Two connections from one address are one client; open falls to 0 as the guards drop.
#[tokio::test(start_paused = true)]
async fn two_connections_from_one_address_are_one_client() {
    let activity = Activity::default();
    let first = passed(&activity, LOOPBACK);
    let second = passed(&activity, LOOPBACK);
    let now = activity.snapshot();
    assert_eq!((now.open, now.hour, now.day), (2, 1, 1));
    drop(first);
    assert_eq!(activity.snapshot().open, 1);
    drop(second);
    let after = activity.snapshot();
    assert_eq!((after.open, after.hour, after.day), (0, 1, 1));
}

/// A connection that never passed the check is counted nowhere, open or seen.
#[tokio::test(start_paused = true)]
async fn a_connection_that_never_passed_is_not_counted() {
    let activity = Activity::default();
    let refused = activity.accepted(LOOPBACK);
    let now = activity.snapshot();
    assert_eq!((now.open, now.hour, now.day), (0, 0, 0));
    drop(refused);
    let after = activity.snapshot();
    assert_eq!((after.open, after.hour, after.day), (0, 0, 0));
}

/// Passing twice on one connection counts it once.
#[tokio::test(start_paused = true)]
async fn passing_twice_counts_a_connection_once() {
    let activity = Activity::default();
    let counted = activity.accepted(LOOPBACK);
    counted.pass();
    counted.pass();
    assert!(counted.passed());
    assert_eq!(activity.snapshot().open, 1);
    drop(counted);
    assert_eq!(activity.snapshot().open, 0);
}

/// A client seen 61 minutes ago counts in the day and not in the hour; one seen 25 hours ago is
/// gone.
#[tokio::test(start_paused = true)]
async fn windows_follow_the_clock() {
    let activity = Activity::default();
    drop(passed(&activity, LOOPBACK));
    tokio::time::advance(minutes(61)).await;
    let later = activity.snapshot();
    assert_eq!((later.hour, later.day), (0, 1));
    tokio::time::advance(minutes(24 * 60 - 61 + 1)).await;
    let gone = activity.snapshot();
    assert_eq!((gone.hour, gone.day), (0, 0));
}

/// A connection held open reads as seen now, however long it has been open.
#[tokio::test(start_paused = true)]
async fn an_open_connection_is_always_in_both_windows() {
    let activity = Activity::default();
    let held = passed(&activity, LOOPBACK);
    tokio::time::advance(minutes(26 * 60)).await;
    let now = activity.snapshot();
    assert_eq!((now.open, now.hour, now.day), (1, 1, 1));
    drop(held);
    // Closing counts as being seen then.
    let closed = activity.snapshot();
    assert_eq!((closed.hour, closed.day), (1, 1));
}

/// `counted_secs` is the daemon's uptime, at most a day.
#[tokio::test(start_paused = true)]
async fn counted_secs_is_the_uptime_capped_at_a_day() {
    let activity = Activity::default();
    tokio::time::advance(minutes(40)).await;
    assert_eq!(activity.snapshot().counted_secs, 40 * 60);
    tokio::time::advance(minutes(30 * 60)).await;
    assert_eq!(activity.snapshot().counted_secs, svolder_ipc::DAY_SECS);
}

/// An IPv4-mapped IPv6 address is the IPv4 one.
#[tokio::test(start_paused = true)]
async fn a_mapped_address_is_the_same_client() {
    let activity = Activity::default();
    drop(passed(&activity, LOOPBACK));
    drop(passed(&activity, "::ffff:127.0.0.1".parse().unwrap()));
    assert_eq!(activity.snapshot().day, 1);
    drop(passed(&activity, "::1".parse().unwrap()));
    assert_eq!(activity.snapshot().day, 2);
}

/// The table holds 4096 clients: one more drops the oldest, and the reply says so for a day.
#[tokio::test(start_paused = true)]
async fn the_table_is_capped_and_says_so() {
    let activity = Activity::default();
    for n in 0..=MAX_CLIENTS as u32 {
        drop(passed(
            &activity,
            IpAddr::V4(Ipv4Addr::from(0x0a00_0000 + n)),
        ));
    }
    let full = activity.snapshot();
    assert_eq!(full.day as usize, MAX_CLIENTS);
    assert!(full.capped);
    tokio::time::advance(minutes(24 * 60 + 1)).await;
    let later = activity.snapshot();
    assert_eq!(later.day, 0);
    assert!(!later.capped);
}

/// No raw address is held anywhere a Debug can show it.
#[tokio::test(start_paused = true)]
async fn debug_holds_no_address() {
    let activity = Activity::default();
    let counted = passed(&activity, LOOPBACK);
    for text in [
        format!("{activity:?}"),
        format!("{counted:?}"),
        format!("{:?}", activity.snapshot()),
    ] {
        assert!(!text.contains("127.0.0.1"), "{text}");
        assert!(!text.contains("127, 0, 0, 1"), "{text}");
    }
}
