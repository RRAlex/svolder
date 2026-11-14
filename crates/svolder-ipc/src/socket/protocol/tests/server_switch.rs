// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `server_switch` request on the wire: the request, its answer and its feature.

use super::*;

/// The request names its server by the table's word and carries the state asked for.
#[test]
fn the_server_switch_has_its_wire_shape() {
    let cases = [
        (
            Server::Web,
            true,
            r#"{"cmd":"server_switch","server":"web","enabled":true}"#,
        ),
        (
            Server::Mcp,
            false,
            r#"{"cmd":"server_switch","server":"mcp","enabled":false}"#,
        ),
    ];
    for (server, enabled, wire) in cases {
        let request = Request::ServerSwitch { server, enabled };
        assert_eq!(serde_json::to_string(&request).unwrap(), wire);
        assert_eq!(serde_json::from_str::<Request>(wire).unwrap(), request);
        assert!(!request.is_stream());
    }
    assert_eq!(Server::Web.table(), "web");
    assert_eq!(Server::Mcp.table(), "mcp");
}

/// A server this build does not know, or a request with no state, is refused as a bad request
/// and never read as one of the two.
#[test]
fn a_server_switch_for_an_unknown_server_does_not_parse() {
    for wire in [
        r#"{"cmd":"server_switch","server":"mqtt","enabled":true}"#,
        r#"{"cmd":"server_switch","server":"web"}"#,
        r#"{"cmd":"server_switch","enabled":true}"#,
    ] {
        assert!(serde_json::from_str::<Request>(wire).is_err(), "{wire}");
    }
}

/// The answer carries both servers' states as `version` reports them.
#[test]
fn the_server_switch_answer_carries_both_servers() {
    let answer = Response::ServerSwitch {
        web: WebStatus::Off,
        mcp: McpStatus::Off,
    };
    let wire = r#"{"result":"server_switch","web":{"state":"off"},"mcp":{"state":"off"}}"#;
    assert_eq!(serde_json::to_string(&answer).unwrap(), wire);
    assert_eq!(serde_json::from_str::<Response>(wire).unwrap(), answer);
}

/// The request is a feature a front end asks about before it sends one.
#[test]
fn server_switch_is_a_listed_feature() {
    assert_eq!(FEATURE_SERVER_SWITCH, "server_switch");
    assert!(FEATURES.contains(&FEATURE_SERVER_SWITCH));
}

/// The guest switch is a request of its own, so a daemon without it answers an unknown kind
/// rather than reading a `guest` field as part of a server switch.
#[test]
fn the_guest_switch_has_its_wire_shape() {
    for (read, wire) in [
        (true, r#"{"cmd":"guest_switch","read":true}"#),
        (false, r#"{"cmd":"guest_switch","read":false}"#),
    ] {
        let request = Request::GuestSwitch { read };
        assert_eq!(serde_json::to_string(&request).unwrap(), wire);
        assert_eq!(serde_json::from_str::<Request>(wire).unwrap(), request);
        assert!(!request.is_stream());
    }
    assert!(REQUEST_CMDS.contains(&"guest_switch"));
    assert!(serde_json::from_str::<Request>(r#"{"cmd":"guest_switch"}"#).is_err());
}

/// The guest switch is a feature a front end asks about before it sends one.
#[test]
fn guest_switch_is_a_listed_feature() {
    assert_eq!(FEATURE_GUEST_SWITCH, "guest_switch");
    assert!(FEATURES.contains(&FEATURE_GUEST_SWITCH));
}
