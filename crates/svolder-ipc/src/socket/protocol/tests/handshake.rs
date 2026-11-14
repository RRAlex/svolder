// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The capability handshake, the named codes and the typed reasons: what an older build on the
//! other end reads and what a newer one's words earn here.

use super::super::*;
use serde::Deserialize;

/// The shape of a 0.32.7 daemon's `version` answer on the wire, written out by hand.
const VERSION_0_32_7: &str = r#"{"msg":"response","id":3,"response":{"result":"version","version":"0.32.7","mqtt":{"state":"off"},"mcp":{"state":"off"},"config":"/home/u/.config/svolder/config.toml","log":"journal","web":{"state":"off"}}}"#;

/// The shape of the error a 0.32.7 daemon answers a request it does not know with (serde's list
/// of expected kinds is shortened).
const UNKNOWN_0_32_7: &str = r#"{"msg":"response","id":4,"response":{"result":"error","code":1,"message":"bad request: unknown variant `walk`, expected one of `devices`, `state`"}}"#;

/// The shape of the error a 0.32.7 daemon answers a command with while it holds no link.
const NOT_CONNECTED_0_32_7: &str = r#"{"msg":"response","id":5,"response":{"result":"error","code":3,"message":"device not connected"}}"#;

/// An older daemon's version answer has no `features`, and reads as listing none.
#[test]
fn a_version_answer_without_features_lists_none() {
    let Reply::Response {
        response: Response::Version {
            version, features, ..
        },
        ..
    } = serde_json::from_str(VERSION_0_32_7).expect("the 0.32.7 answer decodes")
    else {
        panic!("a version answer");
    };
    assert_eq!(version, "0.32.7");
    assert!(features.is_empty());
}

/// The new answers still decode in a front end that has none of the new fields: it drops them.
#[test]
fn an_older_front_end_reads_the_new_version_and_error_answers() {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "result", rename_all = "snake_case")]
    enum Older {
        Version { version: String },
        Error { code: u8, message: String },
    }
    let version = to_line(&Response::Version {
        version: "0.33.0".into(),
        mqtt: None,
        mcp: None,
        config: None,
        log: None,
        mcp_token: None,
        web: None,
        web_token: None,
        features: FEATURES.iter().map(|f| (*f).to_string()).collect(),
        config_changed: false,
        config_missing: false,
    })
    .unwrap();
    assert!(
        version.contains(
            r#""features":["reasons","unknown_request_code","device_crud","server_switch","guest_switch","mqtt_set","walk","autoeq","host_cover","preset_write"]"#
        ),
        "{version}"
    );
    let Older::Version { version } = serde_json::from_str(&version).unwrap() else {
        panic!("a version");
    };
    assert_eq!(version, "0.33.0");

    let error =
        to_line(&Response::error(CODE_LINK, NOT_CONNECTED).with_reason(Reason::NotConnected))
            .unwrap();
    assert!(error.contains(r#""reason":"not_connected""#), "{error}");
    let Older::Error { code, message } = serde_json::from_str(&error).unwrap() else {
        panic!("an error");
    };
    assert_eq!((code, message.as_str()), (3, NOT_CONNECTED));

    // And the same mirror reads the 0.32.7 answers themselves.
    #[derive(Debug, Deserialize)]
    #[serde(tag = "msg", rename_all = "snake_case")]
    enum OlderReply {
        Response { id: u64, response: Older },
    }
    for line in [VERSION_0_32_7, UNKNOWN_0_32_7, NOT_CONNECTED_0_32_7] {
        let OlderReply::Response { id, response } =
            serde_json::from_str(line).expect("the mirror reads a 0.32.7 line");
        assert!(id > 0 && matches!(response, Older::Version { .. } | Older::Error { .. }));
    }
}

/// An error with no reason says none on the wire, so an older front end sees the shape it knew,
/// and an old daemon's error reads as one with no reason.
#[test]
fn an_error_without_a_reason_is_the_old_shape() {
    let line = to_line(&Response::error(CODE_USAGE, "no")).unwrap();
    assert!(!line.contains("reason"), "{line}");
    let Reply::Response { response, .. } = serde_json::from_str(NOT_CONNECTED_0_32_7).unwrap()
    else {
        panic!("a response");
    };
    assert_eq!(response, Response::error(CODE_LINK, NOT_CONNECTED));
}

/// A reason this build does not know decodes as `Unknown`, with the error around it intact.
#[test]
fn a_reason_this_build_does_not_know_is_unknown() {
    let line = r#"{"result":"error","code":3,"message":"x","reason":"from_the_future"}"#;
    assert_eq!(
        serde_json::from_str::<Response>(line).unwrap(),
        Response::error(3, "x").with_reason(Reason::Unknown)
    );
}

/// The reasons and the codes are the numbers and words the wire carries; none is moved.
#[test]
fn the_codes_and_reasons_are_pinned() {
    assert_eq!(
        [CODE_USAGE, CODE_NOT_FOUND, CODE_LINK, CODE_TIMEOUT],
        [1, 2, 3, 4]
    );
    assert_eq!(CODE_COMPUTER_UNAVAILABLE, 6);
    assert_eq!(CODE_UNKNOWN_REQUEST, 7);
    assert_eq!(
        serde_json::to_string(&Reason::NotFoundYet).unwrap(),
        r#""not_found_yet""#
    );
    assert_eq!(
        serde_json::to_string(&Reason::NotConnected).unwrap(),
        r#""not_connected""#
    );
}

/// The constructor builds the plain error, and a reason is added on top.
#[test]
fn the_constructors_build_the_error_and_its_reason() {
    assert_eq!(
        Response::error(CODE_TIMEOUT, "slow"),
        Response::Error {
            code: 4,
            message: "slow".into(),
            reason: None
        }
    );
    assert_eq!(
        Response::error(CODE_LINK, "x").with_reason(Reason::NotFoundYet),
        Response::Error {
            code: 3,
            message: "x".into(),
            reason: Some(Reason::NotFoundYet)
        }
    );
    // A reason on a reply that is no error has nothing to attach to.
    assert_eq!(
        Response::Ack.with_reason(Reason::NotFoundYet),
        Response::Ack
    );
}

/// The one list of what the daemon supports names the reasons, so the mechanism is used.
#[test]
fn the_daemon_lists_its_features_from_one_place() {
    assert_eq!(
        FEATURES,
        [
            FEATURE_REASONS,
            FEATURE_UNKNOWN_REQUEST_CODE,
            FEATURE_DEVICE_CRUD,
            FEATURE_SERVER_SWITCH,
            FEATURE_GUEST_SWITCH,
            FEATURE_MQTT_SET,
            FEATURE_WALK,
            FEATURE_AUTOEQ,
            FEATURE_HOST_COVER,
            FEATURE_PRESET_WRITE
        ]
    );
    assert_eq!(FEATURE_REASONS, "reasons");
}

/// A 0.32.7 daemon's unknown-request text is what the fallback still reads.
#[test]
fn the_legacy_text_of_an_unknown_request_is_still_recognised() {
    let Reply::Response {
        response:
            Response::Error {
                code,
                message,
                reason,
            },
        ..
    } = serde_json::from_str(UNKNOWN_0_32_7).unwrap()
    else {
        panic!("an error");
    };
    assert_eq!((code, reason), (CODE_USAGE, None));
    use crate::socket::daemon_version::{DaemonInfo, is_unknown_request};
    assert!(is_unknown_request(None, code, &message, "walk"));
    assert!(!is_unknown_request(None, code, &message, "state"));
    assert!(is_unknown_request(
        None,
        CODE_UNKNOWN_REQUEST,
        "unknown request `walk`",
        "walk"
    ));
    assert!(!is_unknown_request(
        None,
        CODE_USAGE,
        "bad request: missing field `device`",
        "walk"
    ));
    // A daemon that lists the reasons is never read by its words.
    let new = DaemonInfo {
        version: "0.33.0".into(),
        mqtt: None,
        mcp: None,
        config: None,
        mcp_token: None,
        web: None,
        web_token: None,
        features: vec![FEATURE_UNKNOWN_REQUEST_CODE.into()],
        config_changed: false,
        config_missing: false,
    };
    assert!(!is_unknown_request(Some(&new), code, &message, "walk"));
    assert!(is_unknown_request(
        Some(&new),
        CODE_UNKNOWN_REQUEST,
        "x",
        "walk"
    ));
}
