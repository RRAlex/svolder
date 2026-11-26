// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn server(open: u32, hour: u32, day: u32, counted_secs: u64, capped: bool) -> ServerActivity {
    ServerActivity {
        open,
        hour,
        day,
        counted_secs,
        capped,
    }
}

/// A `version` reply from a daemon that has no activity decodes with none; one with it round-trips.
#[test]
fn a_version_reply_without_activity_decodes_to_none_and_one_with_it_round_trips() {
    let old = r#"{"result":"version","version":"0.37.1"}"#;
    let Response::Version { activity, .. } = serde_json::from_str(old).expect("decodes") else {
        panic!("a version answer");
    };
    assert_eq!(activity, None);

    let sent = Activity {
        mcp: Some(server(2, 3, 5, 86_400, false)),
        web: None,
    };
    let line = serde_json::to_string(&Response::Version {
        version: "0.37.2".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
        features: Vec::new(),
        config_changed: false,
        config_missing: false,
        adapter: None,
        adapter_pinned: false,
        activity: Some(sent),
    })
    .unwrap();
    let Response::Version { activity, .. } = serde_json::from_str(&line).expect("decodes") else {
        panic!("a version answer");
    };
    assert_eq!(activity, Some(sent));

    let none = serde_json::to_string(&Response::Version {
        version: "0.37.2".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
        features: Vec::new(),
        config_changed: false,
        config_missing: false,
        adapter: None,
        adapter_pinned: false,
        activity: None,
    })
    .unwrap();
    assert!(!none.contains("activity"), "{none}");
}

/// A server activity says `capped` only when it is, and decodes without it.
#[test]
fn capped_is_sent_only_when_true() {
    let plain = serde_json::to_string(&server(0, 1, 1, 10, false)).unwrap();
    assert!(!plain.contains("capped"), "{plain}");
    let back: ServerActivity =
        serde_json::from_str(r#"{"open":0,"hour":1,"day":1,"counted_secs":10}"#).unwrap();
    assert!(!back.capped);
}

/// The long line names both windows once the daemon has seen a day.
#[test]
fn the_line_names_both_windows_when_a_day_is_observed() {
    assert_eq!(
        server(2, 3, 5, 86_400, false).line(),
        "2 connections open; 3 clients in the past hour, 5 in the past day"
    );
    assert_eq!(
        server(1, 1, 1, 90_000, false).line(),
        "1 connection open; 1 client in the past hour, 1 in the past day"
    );
}

/// Between an hour and a day the day window is said as what it is: since the daemon started.
#[test]
fn the_line_says_since_the_daemon_started_for_a_day_not_yet_seen() {
    assert_eq!(
        server(0, 3, 5, 5 * 3600 + 60, false).line(),
        "0 connections open; 3 clients in the past hour, 5 since the daemon started 5 h ago"
    );
    // Under an hour there is only the one window.
    assert_eq!(
        server(2, 5, 5, 40 * 60, false).line(),
        "2 connections open; 5 clients since the daemon started 40 min ago"
    );
    assert_eq!(
        server(0, 0, 0, 30, false).line(),
        "0 connections open; 0 clients since the daemon started 30 s ago"
    );
}

/// A capped table says "at least" for the day figure.
#[test]
fn a_capped_line_says_at_least() {
    assert_eq!(
        server(2, 3, 4096, 86_400, true).line(),
        "2 connections open; 3 clients in the past hour, at least 4096 in the past day"
    );
    assert_eq!(
        server(0, 7, 7, 600, true).line(),
        "0 connections open; at least 7 clients since the daemon started 10 min ago"
    );
}

/// The short form is for a 48-cell box value, and never claims a window not observed.
#[test]
fn the_short_form_follows_the_same_windows() {
    assert_eq!(
        server(2, 3, 5, 86_400, false).short(),
        "2 open · 3 past hour · 5 past day"
    );
    assert_eq!(
        server(2, 3, 5, 5 * 3600, false).short(),
        "2 open · 3 past hour · 5 in the last 5 h"
    );
    assert_eq!(
        server(2, 5, 5, 40 * 60, false).short(),
        "2 open · 5 in the last 40 min"
    );
    assert_eq!(
        server(0, 3, 4096, 86_400, true).short(),
        "0 open · 3 past hour · 4096+ past day"
    );
}
