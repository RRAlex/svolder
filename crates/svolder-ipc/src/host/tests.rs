// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The device the routes below end in.
fn dac() -> DacFacts {
    DacFacts {
        display: "Gungnir 2".into(),
        input: Some("USB".into()),
        rate_hz: Some(48_000),
        usb_ports: None,
    }
}

/// A Schiit sink matching [`dac`], at 48 kHz, full volume.
fn schiit_sink() -> SinkFacts {
    SinkFacts {
        name: "alsa_output.usb-Schiit_Audio_Gungnir_2-00.analog-stereo".into(),
        description: "Schiit Gungnir 2 Analog Stereo".into(),
        rate_hz: Some(48_000),
        format: Some("S32LE".into()),
        volume_milli: Some(1000),
        muted: false,
        usb_vendor_id: Some(0x30be),
        usb_product: Some("Gungnir 2".into()),
    }
}

/// A player's stream at 48 kHz, full volume.
fn stream() -> StreamFacts {
    StreamFacts {
        rate_hz: Some(48_000),
        format: Some("S16LE".into()),
        volume_milli: Some(1000),
        muted: false,
    }
}

/// A route that leaves the signal alone, found by the player's pid.
fn clean() -> RouteFacts {
    RouteFacts {
        sink: Some(schiit_sink()),
        player: Some(stream()),
        player_match: PlayerMatch::Pid,
        others_mixing: 0,
        host_eq: false,
    }
}

/// Every word, detail and sentence a line carries.
fn all_text(line: &PathLine) -> String {
    let mut text = line.words.join(" | ");
    for hop in &line.hops {
        text.push_str(&format!(" | {} {}", hop.label, hop.detail));
    }
    text
}

#[test]
fn untouched_when_every_hop_leaves_it_alone() {
    let line = path_line(&clean(), &dac());
    assert_eq!(line.verdict, Verdict::Untouched);
    assert_eq!(line.words, vec!["untouched".to_string()]);
    let labels: Vec<_> = line.hops.iter().map(|hop| hop.label.as_str()).collect();
    assert_eq!(labels, ["player", "computer", "DAC"]);
    assert!(line.hops.iter().all(|hop| !hop.changes));
    assert_eq!(line.hops[0].detail, "48 kHz S16LE, volume 100 %");
    assert_eq!(line.hops[1].detail, "48 kHz S32LE, volume 100 %");
    assert_eq!(line.hops[2].detail, "USB, 48 kHz");
}

#[test]
fn resampled_names_the_rates() {
    let mut route = clean();
    route.player.as_mut().unwrap().rate_hz = Some(44_100);
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["resampled 44.1 → 48 kHz".to_string()]);
    assert!(line.hops[1].changes);
    assert!(!line.hops[0].changes && !line.hops[2].changes);
    assert_eq!(line.hops[0].detail, "44.1 kHz S16LE, volume 100 %");
}

#[test]
fn rates_print_with_a_decimal_only_when_needed() {
    for (hz, said) in [
        (44_100, "44.1"),
        (48_000, "48"),
        (96_000, "96"),
        (176_400, "176.4"),
        (352_800, "352.8"),
        (22_050, "22.05"),
    ] {
        assert_eq!(khz(hz), said);
    }
}

#[test]
fn computer_volume_below_100_is_named() {
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = Some(540);
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["computer volume 54 %".to_string()]);
    // A volume just under 100 % never reads as 100 %.
    route.sink.as_mut().unwrap().volume_milli = Some(999);
    assert_eq!(
        path_line(&route, &dac()).words,
        vec!["computer volume 99.9 %".to_string()]
    );
}

#[test]
fn the_players_own_volume_and_mute_are_named() {
    let mut route = clean();
    route.player.as_mut().unwrap().volume_milli = Some(910);
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["player volume 91 %".to_string()]);
    assert!(line.hops[0].changes && !line.hops[1].changes);

    let mut route = clean();
    route.player.as_mut().unwrap().muted = true;
    let line = path_line(&route, &dac());
    assert_eq!(line.words, vec!["player muted".to_string()]);
    assert!(line.hops[0].changes);
}

/// wpctl lets a volume go past 100 %: a boost is a change too, on the computer and on the player.
#[test]
fn a_boost_above_100_is_a_change() {
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = Some(1200);
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["computer volume 120 %".to_string()]);
    assert!(line.hops[1].changes);
    assert_eq!(line.hops[1].detail, "48 kHz S32LE, volume 120 %");

    let mut route = clean();
    route.player.as_mut().unwrap().volume_milli = Some(1200);
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["player volume 120 %".to_string()]);
    assert!(line.hops[0].changes && !line.hops[1].changes);
}

#[test]
fn a_volume_that_is_not_100_never_reads_100() {
    for (milli, said) in [
        (540, "54 %"),
        (910, "91 %"),
        (999, "99.9 %"),
        (1000, "100 %"),
        (1005, "100.5 %"),
        (1200, "120 %"),
        (1500, "150 %"),
        (0, "0 %"),
    ] {
        assert_eq!(percent(milli), said);
    }
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = Some(1005);
    assert_eq!(
        path_line(&route, &dac()).words,
        vec!["computer volume 100.5 %".to_string()]
    );
}

#[test]
fn muted_is_named() {
    let mut route = clean();
    route.sink.as_mut().unwrap().muted = true;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["computer muted".to_string()]);
}

#[test]
fn host_eq_on_is_named() {
    let mut route = clean();
    route.host_eq = true;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["host EQ on".to_string()]);
}

#[test]
fn others_mixing_is_named() {
    let mut route = clean();
    route.others_mixing = 2;
    assert_eq!(
        path_line(&route, &dac()).words,
        vec!["2 other sounds mixing".to_string()]
    );
    route.others_mixing = 1;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["1 other sound mixing".to_string()]);
}

#[test]
fn dac_rate_differs_is_named() {
    let mut device = dac();
    device.rate_hz = Some(44_100);
    let line = path_line(&clean(), &device);
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["DAC reads 44.1 kHz".to_string()]);
    assert!(line.hops[2].changes);
    // The words come in hop order: the computer's, then the DAC's.
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = Some(540);
    assert_eq!(
        path_line(&route, &device).words,
        vec![
            "computer volume 54 %".to_string(),
            "DAC reads 44.1 kHz".to_string()
        ]
    );
}

#[test]
fn a_dac_with_no_read_rate_is_unknown_and_not_changing() {
    let mut device = dac();
    device.rate_hz = None;
    let line = path_line(&clean(), &device);
    assert!(line.hops[2].unknown, "{line:?}");
    assert!(!line.hops[2].changes, "{line:?}");
    // A read rate matching the sink's is neither.
    let line = path_line(&clean(), &dac());
    assert!(!line.hops[2].unknown, "{line:?}");
    assert!(!line.hops[2].changes, "{line:?}");
}

#[test]
fn the_player_and_computer_hops_mark_an_unread_fact_unknown() {
    let line = path_line(&clean(), &dac());
    assert!(!line.hops[0].unknown && !line.hops[1].unknown, "{line:?}");
    // The player's rate not read.
    let mut route = clean();
    route.player.as_mut().unwrap().rate_hz = None;
    let line = path_line(&route, &dac());
    assert!(line.hops[0].unknown && !line.hops[1].unknown, "{line:?}");
    // The sink's volume not read.
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = None;
    let line = path_line(&route, &dac());
    assert!(!line.hops[0].unknown && line.hops[1].unknown, "{line:?}");
    // No player identified.
    let mut route = clean();
    route.player_match = PlayerMatch::NoPlayer;
    let line = path_line(&route, &dac());
    assert!(line.hops[0].unknown && !line.hops[0].changes, "{line:?}");
}

#[test]
fn not_this_dac_gives_no_verdict() {
    let mut route = clean();
    route.sink = Some(SinkFacts {
        name: "alsa_output.pci-0000_00_1f.3.analog-stereo".into(),
        description: "Built-in Audio".into(),
        ..SinkFacts::default()
    });
    let line = path_line(&route, &dac());
    assert_eq!(
        line.verdict,
        Verdict::NotThisDac {
            sink: "Built-in Audio".into()
        }
    );
    assert_eq!(
        line.words,
        vec!["the computer plays to Built-in Audio".to_string()]
    );
    assert!(line.hops.is_empty());

    // A sink with no description is named by its node.
    route.sink.as_mut().unwrap().description.clear();
    let line = path_line(&route, &dac());
    assert_eq!(
        line.words,
        vec!["the computer plays to alsa_output.pci-0000_00_1f.3.analog-stereo".to_string()]
    );
}

#[test]
fn a_player_playing_elsewhere_gives_no_verdict() {
    let mut route = clean();
    route.player = None;
    route.player_match = PlayerMatch::Elsewhere {
        sink: "LG TV".into(),
    };
    let line = path_line(&route, &dac());
    assert_eq!(
        line.verdict,
        Verdict::NotThisDac {
            sink: "LG TV".into()
        }
    );
    assert_eq!(line.words, vec!["the player plays to LG TV".to_string()]);
    assert!(line.hops.is_empty());
}

#[test]
fn player_assumed_is_said() {
    let mut route = clean();
    route.player_match = PlayerMatch::Assumed;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Untouched);
    assert_eq!(
        line.words,
        vec!["untouched".to_string(), "player assumed".to_string()]
    );
    assert_eq!(line.hops[0].detail, "48 kHz S16LE, volume 100 %, assumed");
}

#[test]
fn a_stream_that_is_not_the_players_is_never_resampled_in_the_words() {
    let mut route = clean();
    route.player_match = PlayerMatch::NotFound;
    route.player.as_mut().unwrap().rate_hz = Some(44_100);
    let line = path_line(&route, &dac());
    assert!(!all_text(&line).contains("resampled"), "{line:?}");
    assert_eq!(line.words, vec!["player not identified".to_string()]);
}

#[test]
fn player_not_found_still_judges_the_rest() {
    let mut route = clean();
    route.player = None;
    route.player_match = PlayerMatch::NotFound;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Unknown);
    assert_eq!(line.words, vec!["player not identified".to_string()]);
    assert_eq!(line.hops[0].detail, "not identified");

    route.host_eq = true;
    route.others_mixing = 1;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(
        line.words,
        vec!["host EQ on".to_string(), "1 other sound mixing".to_string()]
    );
}

/// A stream the route carries beside a match that found none is not taken as the player's.
#[test]
fn a_stream_without_a_match_is_not_identified() {
    for matched in [PlayerMatch::NotFound, PlayerMatch::NoPlayer] {
        let mut route = clean();
        route.player_match = matched;
        let line = path_line(&route, &dac());
        assert_eq!(line.verdict, Verdict::Unknown);
        assert_eq!(line.hops[0].detail, "not identified");
        assert!(!line.hops[0].changes);
    }
    // Its volume is not judged either.
    let mut route = clean();
    route.player_match = PlayerMatch::NotFound;
    route.player.as_mut().unwrap().volume_milli = Some(300);
    assert_eq!(path_line(&route, &dac()).verdict, Verdict::Unknown);
}

#[test]
fn no_player_is_unknown_in_its_own_words() {
    let mut route = clean();
    route.player = None;
    route.player_match = PlayerMatch::NoPlayer;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Unknown);
    assert_eq!(line.words, vec!["no player".to_string()]);
}

#[test]
fn what_cannot_be_read_is_unknown_never_untouched() {
    // The DAC's rate.
    let mut device = dac();
    device.rate_hz = None;
    let line = path_line(&clean(), &device);
    assert_eq!(line.verdict, Verdict::Unknown);
    assert_eq!(line.words, vec!["DAC rate not read".to_string()]);

    // A suspended sink, a stream with no format, volumes not reported.
    let mut route = clean();
    route.sink.as_mut().unwrap().rate_hz = None;
    route.player.as_mut().unwrap().rate_hz = None;
    route.player.as_mut().unwrap().volume_milli = None;
    route.sink.as_mut().unwrap().volume_milli = None;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Unknown);
    assert_eq!(
        line.words,
        vec![
            "player rate not read".to_string(),
            "player volume not read".to_string(),
            "computer rate not read".to_string(),
            "computer volume not read".to_string(),
        ]
    );

    // A change outranks what is unknown.
    route.host_eq = true;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["host EQ on".to_string()]);
}

#[test]
fn a_dac_on_another_input_gives_no_verdict() {
    // The computer's stream matches the rate the DAC reads from its optical input, and says
    // nothing of what it plays.
    for input in ["Optical", "Coaxial", "AES", "BNC"] {
        let mut device = dac();
        device.input = Some(input.into());
        let line = path_line(&clean(), &device);
        assert_eq!(
            line.verdict,
            Verdict::OtherInput {
                input: input.into()
            }
        );
        assert_eq!(line.words, vec![format!("the DAC listens to {input}")]);
        assert!(line.hops.is_empty());
    }
}

#[test]
fn the_words_join_with_a_dot() {
    let mut route = clean();
    route.player_match = PlayerMatch::Assumed;
    assert_eq!(
        path_line(&route, &dac()).words_line(),
        "untouched · player assumed"
    );
    route.host_eq = true;
    route.others_mixing = 2;
    assert_eq!(
        path_line(&route, &dac()).words_line(),
        "host EQ on · 2 other sounds mixing"
    );
}

#[test]
fn a_usb_input_is_judged_as_before() {
    for input in ["USB", "USB 1", "USB 2"] {
        let mut device = dac();
        device.input = Some(input.into());
        let line = path_line(&clean(), &device);
        assert_eq!(line.verdict, Verdict::Untouched, "{input}");
        assert_eq!(line.hops[2].detail, format!("{input}, 48 kHz"));
    }
}

#[test]
fn an_input_not_read_is_unknown_never_untouched() {
    let mut device = dac();
    device.input = None;
    let line = path_line(&clean(), &device);
    assert_eq!(line.verdict, Verdict::Unknown);
    assert_eq!(line.words, vec!["DAC input not read".to_string()]);

    // A change outranks what is unknown.
    let mut route = clean();
    route.host_eq = true;
    let line = path_line(&route, &device);
    assert_eq!(line.verdict, Verdict::Changed);
}

#[test]
fn no_sink_read_is_unknown() {
    let line = path_line(&RouteFacts::default(), &dac());
    assert_eq!(line.verdict, Verdict::Unknown);
    assert!(line.hops.is_empty());
}

#[test]
fn never_says_bit_perfect() {
    let mut routes = vec![clean(), RouteFacts::default()];
    let mut worst = clean();
    worst.player.as_mut().unwrap().rate_hz = Some(44_100);
    worst.player.as_mut().unwrap().volume_milli = Some(500);
    worst.sink.as_mut().unwrap().volume_milli = Some(300);
    worst.sink.as_mut().unwrap().muted = true;
    worst.host_eq = true;
    worst.others_mixing = 3;
    routes.push(worst);
    let mut assumed = clean();
    assumed.player_match = PlayerMatch::Assumed;
    routes.push(assumed);
    let mut elsewhere = clean();
    elsewhere.player_match = PlayerMatch::Elsewhere { sink: "x".into() };
    routes.push(elsewhere);
    for route in routes {
        for rate in [None, Some(44_100), Some(48_000)] {
            let device = DacFacts {
                rate_hz: rate,
                ..dac()
            };
            let text = all_text(&path_line(&route, &device)).to_lowercase();
            assert!(
                !text.contains("bit-perfect") && !text.contains("bit perfect"),
                "{text}"
            );
        }
    }
}

#[test]
fn a_schiit_sink_of_the_device_is_the_dac() {
    assert!(sink_is_dac(&schiit_sink(), &dac()));
    // The match ignores case and a leading "Schiit ", either way round.
    let mut sink = schiit_sink();
    sink.usb_product = Some("Schiit GUNGNIR 2".into());
    assert!(sink_is_dac(&sink, &dac()));
    let device = DacFacts {
        display: "Schiit Gungnir 2".into(),
        ..dac()
    };
    assert!(sink_is_dac(&schiit_sink(), &device));
    // The product name may hold more than the display name, or less.
    sink.usb_product = Some("Gungnir 2 Analog".into());
    assert!(sink_is_dac(&sink, &dac()));
}

#[test]
fn a_schiit_sink_of_another_model_is_not_the_dac() {
    let mut sink = schiit_sink();
    sink.usb_product = Some("Bifrost 3".into());
    assert!(!sink_is_dac(&sink, &dac()));
    // Whatever the product, an empty name matches nothing.
    sink.usb_product = Some(String::new());
    assert!(!sink_is_dac(&sink, &dac()));
    sink.usb_product = None;
    assert!(!sink_is_dac(&sink, &dac()));
    assert!(!sink_is_dac(&schiit_sink(), &DacFacts::default()));
}

/// The display name must be the product name or a run of its whole words: a name that is only
/// the start of a longer one belongs to another model.
#[test]
fn a_display_name_matches_whole_words_only() {
    let sink_of = |product: &str| SinkFacts {
        usb_product: Some(product.into()),
        ..schiit_sink()
    };
    let device = |display: &str| DacFacts {
        display: display.into(),
        ..dac()
    };
    assert!(!sink_is_dac(&sink_of("Modius"), &device("Modi")));
    assert!(!sink_is_dac(&sink_of("Bifrost 3"), &device("Bifrost")));
    assert!(!sink_is_dac(&sink_of("Gungnir"), &device("Gungnir 2")));
    assert!(sink_is_dac(&sink_of("Modi"), &device("Modi")));
    assert!(sink_is_dac(
        &sink_of("Schiit Modi Multibit"),
        &device("Modi")
    ));
    assert!(sink_is_dac(
        &sink_of("Gungnir 2"),
        &device("Schiit Gungnir 2")
    ));
    assert!(sink_is_dac(
        &sink_of("Bifrost 3 USB Audio"),
        &device("Bifrost 3")
    ));
}

#[test]
fn a_sink_of_another_maker_is_not_the_dac() {
    let mut sink = schiit_sink();
    sink.usb_vendor_id = Some(0x0414);
    assert!(!sink_is_dac(&sink, &dac()));
    sink.usb_vendor_id = None;
    assert!(!sink_is_dac(&sink, &dac()));
}

#[test]
fn a_view_without_what_it_lacks_is_a_short_line() {
    let line = serde_json::to_string(&HostView::default()).unwrap();
    assert_eq!(line, r#"{"available":false,"now_ms":0}"#);
    let back: HostView = serde_json::from_str("{}").unwrap();
    assert_eq!(back, HostView::default());
}

#[test]
fn a_view_round_trips() {
    let view = HostView {
        available: true,
        playing: Some(PlayingView {
            title: "Song".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            player: "Spotify".into(),
            status: "playing".into(),
            length_us: Some(200_000_000),
            progress: Some(Progress {
                position_us: 5_000_000,
                rate_milli: 1000,
                read_at_ms: 99,
            }),
            cover: Some("0123456789abcdef".into()),
        }),
        route: Some(clean()),
        now_ms: 1_700_000_000_000,
        volume: None,
        eq: None,
    };
    let back: HostView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
    assert_eq!(back, view);
}

/// A sample rate's label in hertz.
#[test]
fn a_rate_label_is_read_in_hertz() {
    for (label, hz) in [
        ("44.1 kHz", Some(44_100)),
        ("48 kHz", Some(48_000)),
        ("176.4 kHz", Some(176_400)),
        ("352.8 kHz", Some(352_800)),
        ("384 kHz", Some(384_000)),
        ("—", None),
        ("", None),
        ("48 Hz", None),
        ("fast kHz", None),
    ] {
        assert_eq!(rate_hz(label), hz, "{label}");
    }
}

#[test]
fn a_view_with_a_way_this_build_does_not_know_still_decodes() {
    let view: HostView = serde_json::from_str(
        r#"{"available": true, "route": {"player_match": "from_the_future"}}"#,
    )
    .expect("the view decodes");
    let route = view.route.expect("a route");
    assert_eq!(route.player_match, PlayerMatch::NotFound);
    let line = path_line(&route, &dac());
    assert_ne!(line.verdict, Verdict::Untouched);
}

/// The built-in profile `name`.
fn builtin(name: &str) -> svolder_profiles::Profile {
    svolder_profiles::Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .expect("a built-in")
        .clone()
}

/// A DAC whose profile has no input control takes USB alone: its input reads `USB` and the
/// path is judged on the rest, so a matching rate reads `untouched`.
#[test]
fn a_dac_with_no_input_control_is_usb_only_and_can_read_untouched() {
    for name in ["eitr2", "meshcard"] {
        let profile = builtin(name);
        assert!(profile.control_by_key("input").is_none(), "{name}");
        let rate = profile.control_by_key("sample_rate").expect("a rate").id;
        let facts = DacFacts::read(&profile, |id| (id == rate).then_some(2));
        assert_eq!(facts.input.as_deref(), Some("USB"), "{name}");
        assert_eq!(facts.rate_hz, Some(48_000), "{name}");
        let sink = SinkFacts {
            usb_product: Some(profile.display.clone()),
            ..schiit_sink()
        };
        let route = RouteFacts {
            sink: Some(sink),
            ..clean()
        };
        let line = path_line(&route, &facts);
        assert_eq!(line.verdict, Verdict::Untouched, "{name}");
        assert_eq!(line.words, vec!["untouched".to_string()], "{name}");
    }
}

/// A DAC whose profile has an input control and has not reported it still reads `DAC input
/// not read`.
#[test]
fn a_dac_with_an_input_control_that_has_not_reported_it_is_unknown() {
    let profile = builtin("gungnir2-v2");
    let rate = profile.control_by_key("sample_rate").expect("a rate").id;
    let facts = DacFacts::read(&profile, |id| (id == rate).then_some(2));
    assert_eq!(facts.input, None);
    let line = path_line(&clean(), &facts);
    assert_eq!(line.verdict, Verdict::Unknown);
    assert_eq!(line.words, vec!["DAC input not read".to_string()]);
}

/// What the DAC `profile` reads when it listens to `input` (its select value), at 48 kHz, with
/// the USB 1 and USB 2 ports reading `one` and `two` (a port not given has not reported).
fn gungnir_reading(
    profile: &svolder_profiles::Profile,
    input: i64,
    one: Option<bool>,
    two: Option<bool>,
) -> DacFacts {
    let id = |key: &str| profile.control_by_key(key).expect(key).id;
    let (select, rate) = (id("input"), id("sample_rate"));
    let (port_one, port_two) = (id("usb1_connected"), id("usb2_connected"));
    DacFacts::read(profile, |asked| match asked {
        asked if asked == select => Some(input),
        asked if asked == rate => Some(2),
        asked if asked == port_one => one.map(i64::from),
        asked if asked == port_two => two.map(i64::from),
        _ => None,
    })
}

/// A DAC on its USB 1 while the computer is on its USB 2 plays nothing of the computer's: the
/// line says so and gives no verdict, whichever way round the ports are.
#[test]
fn a_dac_on_the_usb_port_the_computer_is_not_on_gives_no_verdict() {
    let profile = builtin("gungnir2-v2");
    let facts = gungnir_reading(&profile, 1, Some(false), Some(true));
    assert_eq!(facts.input.as_deref(), Some("USB 1"));
    let line = path_line(&clean(), &facts);
    assert_eq!(
        line.verdict,
        Verdict::OtherInput {
            input: "USB 1".into()
        }
    );
    assert_eq!(
        line.words,
        vec![
            "the DAC listens to USB 1".to_string(),
            "the computer is on USB 2".to_string()
        ]
    );
    assert!(line.hops.is_empty());

    let facts = gungnir_reading(&profile, 2, Some(true), Some(false));
    let line = path_line(&clean(), &facts);
    assert_eq!(
        line.words_line(),
        "the DAC listens to USB 2 · the computer is on USB 1"
    );
}

/// A DAC on the one USB port that reads connected is judged on the rest, as before.
#[test]
fn a_dac_on_the_only_connected_usb_port_is_judged_as_before() {
    let profile = builtin("gungnir2-v2");
    for (input, one, two) in [(1, true, false), (2, false, true)] {
        let facts = gungnir_reading(&profile, input, Some(one), Some(two));
        let line = path_line(&clean(), &facts);
        assert_eq!(line.verdict, Verdict::Untouched, "input {input}");
        assert_eq!(line.words, vec!["untouched".to_string()], "input {input}");
    }
}

/// When both ports read connected, or a port's reading is missing, the computer's port cannot be
/// told, so the path is never `untouched`: it is unknown, in words that name the doubt.
#[test]
fn usb_ports_that_cannot_be_told_apart_are_never_untouched() {
    let profile = builtin("gungnir2-v2");
    for (one, two, said) in [
        (Some(true), Some(true), "both USB ports connected"),
        (Some(true), None, "USB ports not read"),
        (None, Some(false), "USB ports not read"),
        (None, None, "USB ports not read"),
        (Some(false), Some(false), "no USB port reads connected"),
    ] {
        let facts = gungnir_reading(&profile, 1, one, two);
        let line = path_line(&clean(), &facts);
        assert_eq!(line.verdict, Verdict::Unknown, "{one:?} {two:?}");
        assert_eq!(line.words, vec![said.to_string()], "{one:?} {two:?}");
        assert_eq!(line.hops.len(), 3, "the hops are still shown");
    }
    // A change outranks the doubt, as it does for anything else unknown.
    let facts = gungnir_reading(&profile, 1, Some(true), Some(true));
    let mut route = clean();
    route.host_eq = true;
    let line = path_line(&route, &facts);
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["host EQ on".to_string()]);
}

/// The Yggdrasil names its ports the same way, and a DAC with one USB port (the Bifrost 3) or no
/// port reading keeps the rule that its USB input is the computer's.
#[test]
fn only_a_dac_with_a_reading_per_port_is_judged_by_its_ports() {
    let ygg = builtin("yggdrasil");
    let id = |key: &str| ygg.control_by_key(key).expect(key).id;
    let (select, rate) = (id("input"), id("sample_rate"));
    let (one, two) = (id("usb1_connected"), id("usb2_connected"));
    let facts = DacFacts::read(&ygg, |asked| match asked {
        asked if asked == select => Some(1),
        asked if asked == rate => Some(2),
        asked if asked == one => Some(0),
        asked if asked == two => Some(1),
        _ => None,
    });
    assert_eq!(facts.usb_ports, Some([Some(false), Some(true)]));

    let bifrost = builtin("bifrost3");
    let id = |key: &str| bifrost.control_by_key(key).expect(key).id;
    let (select, rate, usb) = (id("input"), id("sample_rate"), id("usb_connected"));
    let facts = DacFacts::read(&bifrost, |asked| match asked {
        asked if asked == select => Some(1),
        asked if asked == rate => Some(2),
        asked if asked == usb => Some(1),
        _ => None,
    });
    assert_eq!(facts.input.as_deref(), Some("USB"));
    assert_eq!(facts.usb_ports, None);
    let route = RouteFacts {
        sink: Some(SinkFacts {
            usb_product: Some("Bifrost 3".into()),
            ..schiit_sink()
        }),
        ..clean()
    };
    assert_eq!(path_line(&route, &facts).verdict, Verdict::Untouched);

    // Numbered inputs with no port readings are judged as before.
    let mut device = dac();
    device.input = Some("USB 1".into());
    assert_eq!(path_line(&clean(), &device).verdict, Verdict::Untouched);
}

/// A frame survives the trip through its wire form, and a wire frame whose bands are not
/// 240 a channel is no frame.
#[test]
fn a_frame_view_makes_the_frame_it_came_from() {
    let mut frame = svolder_host::Frame {
        capture_ms: 1_700_000_000_123,
        left: [0; 240],
        right: [0; 240],
        levels: [1, 2, 3, 4],
        beat: true,
        bpm: Some(118),
        phase: 200,
    };
    frame.left[0] = 255;
    frame.right[239] = 17;
    let view = FrameView::from(&frame);
    assert_eq!(
        (view.t, view.left.len(), view.right.len()),
        (frame.capture_ms, 240, 240)
    );
    assert_eq!(view.to_frame(), Some(frame));
    let short = FrameView {
        left: vec![0; 239],
        ..view
    };
    assert_eq!(short.to_frame(), None);
}
