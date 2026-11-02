// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn the_wait_starts_at_two_seconds_and_doubles_to_a_minute() {
    let waits: Vec<u64> = (1..=8).map(|n| retry_delay(n).as_secs()).collect();
    assert_eq!(waits, [2, 4, 8, 16, 32, 60, 60, 60]);
    assert_eq!(retry_delay(u32::MAX), RETRY_MAX);
}

#[test]
fn a_refusal_names_the_subscription_it_answers() {
    let mut unacked = Unacked::default();
    unacked.queued("svolder/U/+/set".to_string());
    unacked.queued("homeassistant/+/svolder_U/+/config".to_string());
    unacked.written(7);
    unacked.written(8);
    assert_eq!(
        unacked.answered(8).as_deref(),
        Some("homeassistant/+/svolder_U/+/config")
    );
    assert_eq!(unacked.answered(7).as_deref(), Some("svolder/U/+/set"));
    assert_eq!(unacked.answered(7), None, "an answer is matched once");

    unacked.queued("svolder/U/+/set".to_string());
    unacked.reset();
    unacked.written(9);
    assert_eq!(
        unacked.answered(9),
        None,
        "a lost connection forgets what was queued"
    );

    let ack = |codes| SubAck {
        pkid: 1,
        return_codes: codes,
    };
    assert!(refuses(&ack(vec![SubscribeReasonCode::Failure])));
    assert!(!refuses(&ack(vec![SubscribeReasonCode::Success(
        QoS::AtLeastOnce
    )])));
}

#[test]
fn a_configured_client_id_is_used_as_given() {
    let socket = std::path::Path::new("/run/user/1000/svolder.sock");
    assert_eq!(client_id(Some("svolder-myhost"), socket), "svolder-myhost");
}

#[test]
fn the_default_client_id_is_svolder_the_host_name_and_the_socket() {
    let socket = std::path::Path::new("/run/user/1000/svolder.sock");
    let id = client_id(None, socket);
    match host_name() {
        Some(host) => assert_eq!(id, format!("svolder-{host}-b08577")),
        None => assert_eq!(id, "svolder-b08577"),
    }
    assert_eq!(client_id(Some("  "), socket), id, "a blank id is no id");
    assert_ne!(
        client_id(None, std::path::Path::new("/tmp/sv.sock")),
        id,
        "another daemon's socket is another id"
    );
}

#[test]
fn the_socket_tag_is_the_same_on_every_run() {
    assert_eq!(
        socket_tag(std::path::Path::new("/run/user/1000/svolder.sock")),
        "b08577"
    );
    assert_eq!(socket_tag(std::path::Path::new("/tmp/sv.sock")), "234a5d");
}

#[test]
fn the_last_will_is_offline_retained_on_the_bridge_topic() {
    let settings = Settings {
        host: "127.0.0.1".into(),
        port: 1883,
        client_id: "svolder-test".into(),
        username: Some("svolder".into()),
        password: Some("secret".into()),
        availability: "svolder/bridge/availability".into(),
    };
    assert!(!format!("{settings:?}").contains("secret"), "{settings:?}");
    let options = options(&settings);
    let will = options.last_will().expect("a last will");
    assert_eq!(will.topic, "svolder/bridge/availability");
    assert_eq!(&will.message[..], b"offline");
    assert!(will.retain);
    assert_eq!(options.client_id(), "svolder-test");
    let login = options.credentials().expect("credentials");
    assert_eq!(
        (login.username.as_str(), login.password.as_str()),
        ("svolder", "secret")
    );
}

#[test]
fn a_password_without_a_user_name_is_not_sent() {
    let settings = Settings {
        host: "127.0.0.1".into(),
        port: 1883,
        client_id: "svolder-test".into(),
        username: None,
        password: Some("secret".into()),
        availability: "svolder/bridge/availability".into(),
    };
    assert!(options(&settings).credentials().is_none());
}
