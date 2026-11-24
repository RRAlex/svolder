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
            "speakers": null,
            "loudness": null,
            "bypass": null,
            "levels": null,
            "player": {"title": "Song", "artist": "Band", "album": "Record", "status": "playing"},
            "path": null,
            "config": null
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

/// A route to a sink named `product` (a Schiit USB sink when `schiit`), the player's stream
/// matched, with `setup` on it.
fn route_to(
    product: &str,
    schiit: bool,
    setup: Option<svolder_host::SetupFacts>,
) -> svolder_host::RouteFacts {
    use svolder_host::{PlayerMatch, RouteFacts, SinkFacts, StreamFacts};
    RouteFacts {
        sink: Some(SinkFacts {
            name: "alsa_output.test".into(),
            description: product.into(),
            rate_hz: Some(48_000),
            format: Some("S32LE".into()),
            volume_milli: Some(1000),
            muted: false,
            usb_vendor_id: schiit.then_some(svolder_ipc::host::SCHIIT_VENDOR_ID),
            usb_product: Some(product.into()),
        }),
        player: Some(StreamFacts {
            rate_hz: Some(48_000),
            format: Some("S16LE".into()),
            volume_milli: Some(1000),
            channels: Some(2),
            muted: false,
        }),
        player_match: PlayerMatch::Pid,
        others_mixing: 0,
        host_eq: false,
        stages: None,
        setup,
    }
}

/// The config facts of a sink that lets every line through.
fn passing_setup() -> svolder_host::SetupFacts {
    use svolder_host::{AlsaFacts, ClockFacts, Dither, Layout, Rates, SetupFacts, SinkOffer};
    let stereo = Layout {
        channels: 2,
        positions: vec!["FL".into(), "FR".into()],
    };
    SetupFacts {
        offer: Some(SinkOffer {
            formats: vec!["S32LE".into(), "S24LE".into(), "S16LE".into()],
            rates: Some(Rates::Range {
                min: 44_100,
                max: 384_000,
            }),
            layout: Some(stereo.clone()),
        }),
        clock: Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: vec![
                44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
            ],
            force_rate: None,
        }),
        sink_layout: Some(stereo.clone()),
        stream_layout: Some(stereo),
        dither: Some(Dither {
            method: "none".into(),
            noise: 0,
        }),
        sink_filter: Some(false),
        stream_filter: Some(false),
        alsa: Some(AlsaFacts {
            path: Some("hw:Audio".into()),
            ..AlsaFacts::default()
        }),
        ..SetupFacts::default()
    }
}

#[tokio::test]
async fn get_host_names_the_path_and_the_config_from_the_route() {
    let fleet = computer_fleet(20);
    fleet.host.lock().unwrap().route = Some(route_to("Speakers", false, Some(passing_setup())));
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["path"]["verdict"], "untouched");
    // No DAC stage on the computer's own line: end to end is the verdict.
    assert_eq!(reply["path"]["end_to_end"], "untouched");
    assert_eq!(reply["path"]["words"], json!(["untouched"]));
    let sentence = reply["path"]["sentence"].as_str().expect("a sentence");
    assert!(sentence.contains("untouched"), "{sentence}");
    assert_eq!(
        reply["config"]["summary"],
        "PipeWire config lets it through untouched"
    );
    assert_eq!(reply["config"]["outcome"], "untouched");
    assert_eq!(reply["config"]["sink"], "Speakers");
    // What comes in and what the card runs at: the judge's words (no card facts in this route).
    assert_eq!(reply["config"]["from_player"], "48 kHz · S16LE · stereo");
    assert_eq!(reply["config"]["to_dac"], "not read");
    assert_eq!(reply["config"]["limit"], svolder_ipc::host::SETUP_LIMIT);
    let lines = reply["config"]["lines"].as_array().expect("the lines");
    let keys: Vec<_> = lines.iter().map(|l| l["key"].as_str().unwrap()).collect();
    assert_eq!(
        keys,
        [
            "rates",
            "forced_rate",
            "dither",
            "format",
            "plug",
            "stereo",
            "filter"
        ]
    );
    assert!(lines.iter().all(|l| l["outcome"] == "pass"), "{lines:?}");
    assert!(lines.iter().all(|l| l["words"].is_string()));
    assert!(host_sent(&fleet).is_empty(), "a read writes nothing");

    // A line that fails is told so, and the verdict follows the judge.
    let mut changed = passing_setup();
    changed.dither.as_mut().unwrap().method = "triangular".into();
    changed.dither.as_mut().unwrap().noise = 1;
    fleet.host.lock().unwrap().route = Some(route_to("Speakers", false, Some(changed)));
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["path"]["verdict"], "changed");
    assert_eq!(reply["path"]["end_to_end"], "changed");
    assert_eq!(
        reply["config"]["summary"],
        "PipeWire config changes the sound: 1"
    );
    assert_eq!(reply["config"]["outcome"], "changes");
    assert!(
        reply["config"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["key"] == "dither" && l["outcome"] == "fail")
    );
}

#[tokio::test]
async fn path_and_config_are_null_without_a_route() {
    let fleet = computer_fleet(20);
    assert!(fleet.host.lock().unwrap().route.is_none());
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["path"], Value::Null);
    assert_eq!(reply["config"], Value::Null);
}

#[tokio::test]
async fn a_route_from_an_older_daemon_says_the_config_was_not_read() {
    let fleet = computer_fleet(20);
    fleet.host.lock().unwrap().route = Some(route_to("Speakers", false, None));
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(
        reply["config"]["summary"],
        "PipeWire config not read (svolderd's version differs)"
    );
    assert_eq!(reply["config"]["outcome"], "other_version");
    let lines = reply["config"]["lines"].as_array().expect("the lines");
    assert!(
        lines.iter().all(|l| l["outcome"] == "unknown"),
        "nothing is invented: {lines:?}"
    );
}

#[tokio::test]
async fn the_path_is_the_dacs_when_the_sink_is_the_default_device() {
    // The same route judged on the computer's side alone reads untouched; judged with the DAC
    // that has not said its input, it is unknown.
    let dac = scripted("gungnir2-v2", &[]);
    *dac.host.lock().unwrap() = computer_view();
    dac.host.lock().unwrap().route =
        Some(route_to("Schiit Gungnir 2", true, Some(passing_setup())));
    let reply = structured(&run(&dac, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["path"]["verdict"], "unknown", "{}", reply["path"]);
    assert_eq!(reply["path"]["dac"], "Gungnir 2");

    let other = scripted("gungnir2-v2", &[]);
    *other.host.lock().unwrap() = computer_view();
    other.host.lock().unwrap().route =
        Some(route_to("Schiit Gungnir 2", false, Some(passing_setup())));
    let reply = structured(&run(&other, None, call("get_host", json!({})), false).await);
    assert_eq!(
        reply["path"]["verdict"], "untouched",
        "a sink that is no Schiit is no DAC"
    );
}

/// The feed of a Gungnir 2 on USB 1 at 48 kHz with its volume at raw step 117 (-3.3 dB); the
/// port readings are booleans.
fn gungnir_feed(profile: &Profile, connected: bool) -> crate::device::Feed {
    use crate::web::view::test_support::read;
    crate::device::Feed {
        connected,
        readings: vec![
            read(profile, "input", "1"),
            read(profile, "sample_rate", "2"),
            read(profile, "usb1_connected", "true"),
            read(profile, "usb2_connected", "false"),
            read(profile, "volume", "117"),
        ],
        ..Default::default()
    }
}

/// A route to the Gungnir 2 at 48 kHz that the DAC's readings judge.
fn route_to_gungnir() -> svolder_host::RouteFacts {
    route_to("Schiit Gungnir 2", true, Some(passing_setup()))
}

#[tokio::test]
async fn the_path_is_judged_with_the_dac_the_sink_is_among_several_devices() {
    // The default device is a preamp; the sink is the second device, a DAC.
    let fleet = scripted("lokimax", &[]);
    let dac = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let (feed, _) = tokio::sync::watch::channel(gungnir_feed(&dac, true));
    *fleet.second.lock().unwrap() = Some((dac, feed));
    *fleet.host.lock().unwrap() = computer_view();
    fleet.host.lock().unwrap().route = Some(route_to_gungnir());
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["path"]["dac"], "Gungnir 2", "{}", reply["path"]);
    assert_eq!(reply["path"]["verdict"], "untouched", "{}", reply["path"]);
    // The DAC's own volume below 0 dB: end to end the sound is changed.
    assert_eq!(reply["path"]["end_to_end"], "changed", "{}", reply["path"]);
    let sentence = reply["path"]["sentence"].as_str().expect("a sentence");
    assert!(
        sentence.contains("the Gungnir 2 then lowers it by 3.3 dB"),
        "{sentence}"
    );
}

#[tokio::test]
async fn a_dacs_readings_and_link_decide_what_the_path_claims() {
    let fleet = scripted("gungnir2-v2", &[]);
    let profile = fleet.profile.clone();
    *fleet.host.lock().unwrap() = computer_view();
    fleet.host.lock().unwrap().route = Some(route_to_gungnir());
    let path = |fleet: &Arc<Scripted>| {
        let fleet = Arc::clone(fleet);
        async move {
            structured(&run(&fleet, None, call("get_host", json!({})), false).await)["path"].clone()
        }
    };
    // The readings are used, the boolean port readings as 0 and 1.
    fleet.feed.send_replace(gungnir_feed(&profile, true));
    let up = path(&fleet).await;
    assert_eq!(up["verdict"], "untouched", "{up}");
    assert_eq!(up["end_to_end"], "changed", "{up}");
    assert!(
        up["sentence"]
            .as_str()
            .unwrap()
            .contains("then lowers it by 3.3 dB"),
        "{up}"
    );
    // A link that is down claims nothing of the device's own stage.
    fleet.feed.send_replace(gungnir_feed(&profile, false));
    let down = path(&fleet).await;
    assert_eq!(down["end_to_end"], "untouched", "{down}");
    assert!(
        !down["sentence"].as_str().unwrap_or("").contains("then"),
        "{down}"
    );
    // The input the DAC reads decides the verdict.
    let mut optical = gungnir_feed(&profile, true);
    optical.readings[0] = crate::web::view::test_support::read(&profile, "input", "3");
    fleet.feed.send_replace(optical);
    assert_eq!(path(&fleet).await["verdict"], "other_input");
}

#[tokio::test]
async fn a_preamp_is_never_the_paths_dac() {
    // A preamp whose display name matches the sink is still no DAC.
    let fleet = scripted("lokimax", &[]);
    let display = fleet.profile.display.clone();
    *fleet.host.lock().unwrap() = computer_view();
    fleet.host.lock().unwrap().route = Some(route_to(
        &format!("Schiit {display}"),
        true,
        Some(passing_setup()),
    ));
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["path"]["dac"], Value::Null);
}
