// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's tools: `get_host`, `set_host_volume`, `host_mute` and `set_host_eq`.

use super::*;

#[tokio::test]
async fn get_host_reads_volume_eq_and_player() {
    let fleet = computer_fleet(20);
    let reply = run(&fleet, None, call("get_host", json!({})), false).await;
    assert_eq!(
        structured(&reply),
        json!({
            "volume": {"percent": 20, "muted": false, "sink": "Speakers"},
            "eq": {
                "on": true, "preset": "Warm", "preamp_db": -3.0,
                "points": [{"type": "PK", "frequency_hz": 1000.0, "gain_db": -3.0, "q": 1.0}]
            },
            "crossfeed": null,
            "player": {"title": "Song", "artist": "Band", "album": "Record", "status": "playing"}
        })
    );
    assert!(host_sent(&fleet).is_empty(), "a read writes nothing");

    let mut off = computer_view();
    off.eq.as_mut().unwrap().unavailable =
        Some("computer controls unavailable: no pw-cli on this computer".into());
    off.eq.as_mut().unwrap().running_unknown = true;
    off.playing = None;
    off.volume = None;
    *fleet.host.lock().unwrap() = off;
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["eq"], Value::Null);
    assert_eq!(reply["player"], Value::Null);
    assert_eq!(reply["volume"], Value::Null);
    assert_eq!(
        reply["unavailable"],
        "computer controls unavailable: no pw-cli on this computer"
    );
}

#[tokio::test]
async fn an_unverified_eq_state_is_marked_so() {
    let fleet = computer_fleet(20);
    fleet
        .host
        .lock()
        .unwrap()
        .eq
        .as_mut()
        .unwrap()
        .running_unknown = true;
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["eq"]["on"], true);
    assert_eq!(reply["eq"]["unverified"], true);
}

#[tokio::test]
async fn the_four_writes_are_listed_only_with_writes() {
    let fleet = scripted("mesh-eq-5", &[]);
    let names = |writes| listed(&fleet, writes);
    for tool in [
        "set_host_volume",
        "host_mute",
        "set_host_eq",
        "set_host_crossfeed",
    ] {
        assert!(!names(false).contains(&tool.to_string()), "{tool}");
        assert!(names(true).contains(&tool.to_string()), "{tool}");
    }
    assert!(names(false).contains(&"get_host".to_string()));
    let tools = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, false, None);
    for (tool, arguments) in [
        ("set_host_volume", json!({"percent": 10})),
        ("host_mute", json!({"muted": true})),
        ("set_host_eq", json!({"off": true})),
        ("set_host_crossfeed", json!({"on": true})),
    ] {
        let reply = tools
            .call(call(tool, arguments), Caller { can_ask: false })
            .await
            .unwrap();
        assert!(is_error(&reply), "{tool}");
        assert!(error_words(&reply).contains("[mcp] writes"), "{tool}");
    }
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn a_volume_above_100_is_refused_as_the_cap() {
    let fleet = computer_fleet(20);
    for bad in [json!(101), json!(250), json!(-1), json!(50.5)] {
        let reply = run(
            &fleet,
            None,
            call("set_host_volume", json!({"percent": bad, "confirm": true})),
            false,
        )
        .await;
        assert!(is_error(&reply), "{bad}");
    }
    let reply = run(
        &fleet,
        None,
        call("set_host_volume", json!({"percent": 101, "confirm": true})),
        false,
    )
    .await;
    assert!(error_words(&reply).contains("up to 100 %"));
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn a_rise_of_more_than_six_db_on_the_cubic_scale_asks() {
    let say = |pct| call("set_host_volume", json!({"percent": pct}));
    // 60 * log10(25 / 20) = 5.8 dB passes; 60 * log10(26 / 20) = 6.8 dB asks.
    let fleet = computer_fleet(20);
    let reply = run(&fleet, None, say(25), false).await;
    assert!(went(&reply), "{}", error_words(&reply));
    assert_eq!(host_sent(&fleet), [Request::HostVolume { pct: 25 }]);

    let fleet = computer_fleet(20);
    let reply = run(&fleet, None, say(26), false).await;
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("repeat this call with"));
    assert!(
        host_sent(&fleet).is_empty(),
        "nothing is written before the answer"
    );

    // 21 % to 26 % is 5.5 dB, to 27 % 6.6 dB.
    let fleet = computer_fleet(21);
    assert!(went(&run(&fleet, None, say(26), false).await));
    let fleet = computer_fleet(21);
    assert!(is_error(&run(&fleet, None, say(27), false).await));

    // The retry with confirm goes through, and the new level is the floor after it.
    let fleet = computer_fleet(20);
    let tools = server(&fleet, None);
    let confirmed = call("set_host_volume", json!({"percent": 40, "confirm": true}));
    let reply = tools
        .call(confirmed, Caller { can_ask: false })
        .await
        .unwrap();
    assert!(went(&reply));
    assert_eq!(host_sent(&fleet), [Request::HostVolume { pct: 40 }]);
    let reply = tools
        .call(say(42), Caller { can_ask: false })
        .await
        .unwrap();
    assert!(went(&reply), "a small step after the agreed rise passes");

    // A client that can ask gets the question, and its yes goes through.
    let fleet = computer_fleet(20);
    let reply = run(&fleet, None, say(60), true).await;
    let CallToolResponse::InputRequired(asked) = &reply else {
        panic!("not a question: {reply:?}");
    };
    assert!(asks(&reply));
    let state = asked.request_state.clone().unwrap();
    let mut retry = say(60);
    retry.request_state = Some(state);
    retry.input_responses = Some(
        [(
            ASK.to_string(),
            json!({"action": "accept", "content": {"confirm": true}}),
        )]
        .into_iter()
        .collect(),
    );
    let reply = server(&fleet, None)
        .call(retry, Caller { can_ask: true })
        .await
        .unwrap();
    assert!(went(&reply), "{}", error_words(&reply));
    assert_eq!(host_sent(&fleet), [Request::HostVolume { pct: 60 }]);
}

#[tokio::test]
async fn a_staircase_of_small_steps_asks_at_the_sum() {
    let fleet = computer_fleet(20);
    let tools = server(&fleet, None);
    let step = |pct| call("set_host_volume", json!({"percent": pct}));
    for pct in [23, 25] {
        let reply = tools
            .call(step(pct), Caller { can_ask: false })
            .await
            .unwrap();
        assert!(went(&reply), "{pct}: {}", error_words(&reply));
    }
    let reply = tools
        .call(step(32), Caller { can_ask: false })
        .await
        .unwrap();
    assert!(is_error(&reply), "20 to 32 is 12 dB over the floor");
}

#[tokio::test]
async fn a_lowering_never_asks() {
    let fleet = computer_fleet(80);
    for pct in [40, 0] {
        let reply = run(
            &fleet,
            None,
            call("set_host_volume", json!({"percent": pct})),
            true,
        )
        .await;
        assert!(went(&reply), "{pct}: {}", error_words(&reply));
    }
    assert_eq!(
        host_sent(&fleet),
        [
            Request::HostVolume { pct: 40 },
            Request::HostVolume { pct: 0 }
        ]
    );
}

#[tokio::test]
async fn host_mute_mutes_and_reads_back() {
    let fleet = computer_fleet(20);
    let reply = run(
        &fleet,
        None,
        call("host_mute", json!({"muted": true})),
        false,
    )
    .await;
    assert_eq!(structured(&reply)["volume"]["muted"], true);
    assert_eq!(host_sent(&fleet), [Request::HostMute { muted: true }]);
}

#[tokio::test]
async fn set_host_eq_off_removes_and_a_preset_applies() {
    let fleet = computer_fleet(20);
    let reply = run(
        &fleet,
        None,
        call("set_host_eq", json!({"off": true})),
        false,
    )
    .await;
    assert_eq!(structured(&reply)["eq"]["on"], false);
    let reply = run(
        &fleet,
        None,
        call("set_host_eq", json!({"preset": "Warm"})),
        false,
    )
    .await;
    let eq = structured(&reply)["eq"].clone();
    assert_eq!(
        (eq["on"].clone(), eq["preset"].clone()),
        (json!(true), json!("Warm"))
    );
    assert_eq!(
        host_sent(&fleet),
        [
            Request::HostEq {
                op: HostEqOp::Mode { on: false }
            },
            Request::HostEq {
                op: HostEqOp::Preset {
                    name: "Warm".into(),
                    device: Some("amp".into()),
                    confirm: false
                }
            },
        ]
    );
    for bad in [json!({}), json!({"preset": "Warm", "off": true})] {
        let reply = run(&fleet, None, call("set_host_eq", bad), false).await;
        assert!(is_error(&reply));
    }
}

#[tokio::test]
async fn the_same_preset_on_the_dac_asks_for_confirm() {
    let say = |confirm| call("set_host_eq", json!({"preset": "Warm", "confirm": confirm}));
    let fleet = computer_fleet(20);
    *fleet.played_twice.lock().unwrap() = Some("amp".into());
    let reply = run(&fleet, None, say(false), false).await;
    assert!(is_error(&reply));
    let words = error_words(&reply);
    assert!(words.contains("would play twice") && words.contains("repeat this call with"));
    assert!(host_sent(&fleet).is_empty());

    let reply = run(&fleet, None, say(true), false).await;
    assert!(went(&reply), "{}", error_words(&reply));
    assert!(matches!(
        host_sent(&fleet).as_slice(),
        [Request::HostEq {
            op: HostEqOp::Preset { confirm: true, .. }
        }]
    ));

    // A client that can ask is sent the question, and its yes repeats with confirm.
    let fleet = computer_fleet(20);
    *fleet.played_twice.lock().unwrap() = Some("amp".into());
    let reply = run(&fleet, None, say(false), true).await;
    let CallToolResponse::InputRequired(asked) = &reply else {
        panic!("not a question: {reply:?}");
    };
    let mut retry = say(false);
    retry.request_state = asked.request_state.clone();
    retry.input_responses = Some(
        [(
            ASK.to_string(),
            json!({"action": "accept", "content": {"confirm": true}}),
        )]
        .into_iter()
        .collect(),
    );
    let reply = run(&fleet, None, retry, true).await;
    assert!(went(&reply), "{}", error_words(&reply));
}

#[tokio::test]
async fn a_built_in_without_a_device_is_refused_with_the_reason() {
    let mut profile = Registry::builtin()
        .unwrap()
        .get("mesh-eq-5")
        .unwrap()
        .clone();
    profile.eq = None;
    let fleet = scripted_over(profile, &[]);
    let reply = run(
        &fleet,
        None,
        call("set_host_eq", json!({"preset": "Warm"})),
        false,
    )
    .await;
    assert!(is_error(&reply));
    let words = error_words(&reply);
    assert!(
        words.contains("built-in") && words.contains("tone control"),
        "{words}"
    );
    assert!(host_sent(&fleet).is_empty());

    // A saved preset needs no device.
    let reply = run(
        &fleet,
        None,
        call("set_host_eq", json!({"preset": "Mine"})),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
}

#[tokio::test]
async fn host_unmute_asks_and_mute_never_does() {
    let say = |muted, confirm| call("host_mute", json!({"muted": muted, "confirm": confirm}));
    let fleet = computer_fleet(80);
    let reply = run(&fleet, None, say(true, false), false).await;
    assert!(went(&reply), "{}", error_words(&reply));
    let reply = run(&fleet, None, say(false, false), false).await;
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("lets sound through at a volume of 80 %"));
    assert_eq!(host_sent(&fleet), [Request::HostMute { muted: true }]);
    let reply = run(&fleet, None, say(false, true), false).await;
    assert!(went(&reply), "{}", error_words(&reply));
    assert_eq!(structured(&reply)["volume"]["muted"], false);

    // A client that can ask is sent the question first.
    let fleet = computer_fleet(80);
    let reply = run(&fleet, None, say(false, false), true).await;
    assert!(asks(&reply));
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn an_unread_volume_counts_as_the_lowest_and_a_rise_from_it_asks() {
    let fleet = computer_fleet(20);
    fleet.host.lock().unwrap().volume = None;
    let say = |pct| call("set_host_volume", json!({"percent": pct}));
    let reply = run(&fleet, None, say(30), false).await;
    assert!(is_error(&reply), "{}", error_words(&reply));
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn a_host_write_the_daemon_refuses_comes_back_as_the_tool_error() {
    let fleet = computer_fleet(20);
    *fleet.host_error.lock().unwrap() = Some("the computer has no sink".to_string());
    let reply = run(
        &fleet,
        None,
        call("set_host_volume", json!({"percent": 10, "confirm": true})),
        false,
    )
    .await;
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("the computer has no sink"));
}
