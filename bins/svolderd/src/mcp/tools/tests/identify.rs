// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `identify`, `list_devices` and the list of tools.

use super::*;

#[test]
fn mcp_identify_listed_only_with_writes() {
    let fleet = scripted("gungnir2-v2", &[]);
    assert!(!listed(&fleet, false).contains(&"identify".to_string()));
    assert!(listed(&fleet, true).contains(&"identify".to_string()));
    let tool = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, true, None)
        .list()
        .into_iter()
        .find(|t| t.name == "identify")
        .expect("listed with writes");
    assert_eq!(
        tool.annotations.expect("annotations").read_only_hint,
        Some(false)
    );
    let properties = tool.input_schema["properties"]
        .as_object()
        .expect("properties")
        .clone();
    assert_eq!(properties.keys().collect::<Vec<_>>(), ["device"]);
    assert_eq!(tool.input_schema["required"], json!([]));
}

#[tokio::test]
async fn mcp_identify_is_refused_naming_the_setting_while_writes_are_off() {
    let fleet = scripted("gungnir2-v2", &[]);
    let off = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, false, None);
    let reply = off
        .call(call("identify", json!({})), Caller { can_ask: true })
        .await
        .expect("an answer");
    assert!(is_error(&reply));
    assert!(
        error_words(&reply).contains("[mcp] writes"),
        "{}",
        error_words(&reply)
    );
    assert_eq!(*fleet.identifies.lock().unwrap(), 0, "nothing was sent");
}

#[tokio::test]
async fn mcp_identify_answers_sent() {
    let fleet = scripted("gungnir2-v2", &[]);
    let reply = run(&fleet, None, call("identify", json!({})), false).await;
    assert!(went(&reply), "{reply:?}");
    assert_eq!(structured(&reply), json!({"device": "amp", "sent": true}));
    assert_eq!(*fleet.identifies.lock().unwrap(), 1);

    // Inside the session's gap nothing goes out: the answer says so, and it is not an error.
    *fleet.identify.lock().unwrap() = Some(Response::Identify {
        device: "amp".into(),
        sent: false,
    });
    let reply = run(
        &fleet,
        None,
        call("identify", json!({"device": "amp"})),
        false,
    )
    .await;
    assert!(went(&reply), "{reply:?}");
    assert_eq!(structured(&reply), json!({"device": "amp", "sent": false}));
}

/// A device whose profile offers no identify answers a tool error saying so, and the daemon is
/// never asked.
#[tokio::test]
async fn mcp_identify_is_refused_for_a_profile_that_offers_none() {
    let fleet = scripted("meshcard", &[]);
    let reply = run(&fleet, None, call("identify", json!({})), false).await;
    assert!(is_error(&reply), "{reply:?}");
    assert!(
        error_words(&reply).contains("amp offers no identify"),
        "{}",
        error_words(&reply)
    );
    assert_eq!(*fleet.identifies.lock().unwrap(), 0);
}

#[tokio::test]
async fn mcp_identify_says_so_when_the_device_is_down() {
    let fleet = scripted("gungnir2-v2", &[]);
    *fleet.identify.lock().unwrap() = Some(
        Response::error(CODE_LINK, svolder_ipc::NOT_CONNECTED)
            .with_reason(svolder_ipc::Reason::NotConnected),
    );
    let reply = run(&fleet, None, call("identify", json!({})), false).await;
    assert!(is_error(&reply));
    assert!(
        error_words(&reply).contains(svolder_ipc::NOT_CONNECTED),
        "{}",
        error_words(&reply)
    );
    let reply = run(
        &fleet,
        None,
        call("identify", json!({"surprise": 1})),
        false,
    )
    .await;
    assert!(error_words(&reply).contains("surprise"));
}

#[tokio::test]
async fn list_devices_carries_the_signal_when_there_is_one() {
    let fleet = scripted("gungnir2-v2", &[]);
    let reply = run(&fleet, None, call("list_devices", json!({})), false).await;
    assert_eq!(
        structured(&reply),
        json!({"devices": [{"name": "amp", "product": "Amp", "connected": true}]}),
        "no signal known: no key"
    );
    *fleet.signal.lock().unwrap() = Some(svolder_ipc::Signal {
        dbm: -62,
        source: svolder_ipc::SignalSource::Advert,
        heard_at_ms: 1_792_931_696_789,
    });
    let reply = run(&fleet, None, call("list_devices", json!({})), false).await;
    assert_eq!(
        structured(&reply)["devices"][0]["signal"],
        json!({"dbm": -62, "source": "advert", "heard_at_ms": 1_792_931_696_789u64})
    );
}
