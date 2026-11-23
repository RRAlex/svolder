// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::host::EndToEnd;
use crate::host::HostView;
use svolder_host::route::ChainStages;
use svolder_host::{
    AlsaFacts, ClockFacts, Dither, HwParams, HwState, KernelOffer, Layout, PlayerMatch, Rates,
    RouteFacts, SetupFacts, SinkFacts, SinkOffer, StreamFacts,
};

/// The device the routes below end in.
fn dac() -> DacFacts {
    DacFacts {
        display: "Gungnir 2".into(),
        input: Some("USB".into()),
        rate_hz: Some(48_000),
        usb_ports: None,
        stage: Default::default(),
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
pub(in crate::host) fn clean() -> RouteFacts {
    RouteFacts {
        sink: Some(schiit_sink()),
        player: Some(stream()),
        player_match: PlayerMatch::Pid,
        others_mixing: 0,
        host_eq: false,
        stages: None,
        setup: None,
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
    assert_eq!(
        line.sentence().as_deref(),
        Some(
            "untouched from the player's output on; the player may still change the sound \
             itself (loudness normalization, ReplayGain, its own EQ); svolder judges up to 24 bits"
        )
    );
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

/// A route whose filter chain runs with the stages `stages` says.
fn chain(eq: bool, crossfeed: bool, speakers: bool) -> RouteFacts {
    let mut route = clean();
    route.host_eq = true;
    route.stages = Some(ChainStages {
        eq,
        loudness: false,
        crossfeed,
        speakers,
    });
    route
}

#[test]
fn loudness_is_named_after_the_eq_and_before_the_crossfeed() {
    let mut route = chain(true, true, false);
    route.stages.as_mut().unwrap().loudness = true;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, ["host EQ on", "loudness on", "crossfeed on"]);
    let mut alone = chain(false, false, false);
    alone.stages.as_mut().unwrap().loudness = true;
    assert_eq!(path_line(&alone, &dac()).words, ["loudness on"]);
}

#[test]
fn each_stage_in_the_chain_is_named() {
    for (route, word) in [
        (chain(true, false, false), "host EQ on"),
        (chain(false, true, false), "crossfeed on"),
        (chain(false, false, true), "virtual speakers on"),
    ] {
        let line = path_line(&route, &dac());
        assert_eq!(line.verdict, Verdict::Changed);
        assert_eq!(line.words, vec![word.to_string()]);
    }
}

#[test]
fn stages_together_are_named_in_chain_order() {
    let line = path_line(&chain(true, true, true), &dac());
    assert_eq!(
        line.words,
        ["host EQ on", "crossfeed on", "virtual speakers on"]
    );
}

#[test]
fn a_chain_kept_flat_is_a_pass_through_never_untouched() {
    let line = path_line(&chain(false, false, false), &dac());
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(line.words, vec!["EQ pass-through".to_string()]);
}

#[test]
fn no_stage_word_without_a_chain() {
    let mut route = chain(true, true, true);
    route.host_eq = false;
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Untouched);
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
            sink: "Built-in Audio".into(),
            by_player: false
        }
    );
    assert_eq!(line.words, vec!["→ Built-in Audio".to_string()]);
    assert_eq!(
        line.sentence().as_deref(),
        Some("the computer plays to Built-in Audio")
    );
    assert!(line.hops.is_empty());

    // A sink with no description is named by its node.
    route.sink.as_mut().unwrap().description.clear();
    let line = path_line(&route, &dac());
    assert_eq!(
        line.words,
        vec!["→ alsa_output.pci-0000_00_1f.3.analog-stereo".to_string()]
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
            sink: "LG TV".into(),
            by_player: true
        }
    );
    assert_eq!(line.words, vec!["player → LG TV".to_string()]);
    assert_eq!(
        line.sentence().as_deref(),
        Some("the player plays to LG TV")
    );
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

#[test]
fn an_assumed_player_keeps_its_words_in_the_untouched_sentence() {
    let mut route = clean();
    route.player_match = PlayerMatch::Assumed;
    let line = path_line(&route, &dac());
    let sentence = line.sentence().expect("a sentence");
    assert_eq!(
        sentence,
        "untouched from the player's output on (player assumed); the player may still change the \
         sound itself (loudness normalization, ReplayGain, its own EQ); svolder judges up to 24 bits"
    );
}

#[test]
fn a_changed_path_with_no_dac_stage_has_no_sentence() {
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = Some(540);
    assert_eq!(path_line(&route, &dac()).sentence(), None);
}

/// A Bifrost 3 read with its volume at `volume`, its tone control at `mode` and its balance at
/// `balance` (raw values).
fn bifrost_reading(volume: i64, mode: i64, balance: i64) -> DacFacts {
    let bifrost = builtin("bifrost3");
    let id = |key: &str| bifrost.control_by_key(key).expect(key).id;
    let (select, rate, vol, bal) = (id("input"), id("sample_rate"), id("volume"), id("balance"));
    let mode_id = bifrost.eq.as_ref().expect("an EQ").mode_id();
    DacFacts::read(&bifrost, |asked| match asked {
        asked if asked == select => Some(1),
        asked if asked == rate => Some(2),
        asked if asked == vol => Some(volume),
        asked if asked == bal => Some(balance),
        asked if asked == mode_id => Some(mode),
        _ => None,
    })
}

/// The route of a Bifrost 3 playing at 48 kHz.
fn bifrost_route() -> RouteFacts {
    RouteFacts {
        sink: Some(SinkFacts {
            usb_product: Some("Bifrost 3".into()),
            ..schiit_sink()
        }),
        ..clean()
    }
}

#[test]
fn the_dac_adds_nothing_at_0_db_flat_and_centred() {
    let facts = bifrost_reading(127, 1, 64);
    assert!(!facts.stage.changes());
    let line = path_line(&bifrost_route(), &facts);
    assert_eq!(line.dac_stage, None);
    assert!(line.hops.iter().all(|hop| !hop.changes));
    assert!(!line.sentence().unwrap().contains("then"));
}

#[test]
fn the_dac_names_a_volume_below_0_db() {
    // Raw 117 is -3.3 dB on the DAC's table.
    let line = path_line(&bifrost_route(), &bifrost_reading(117, 1, 64));
    assert_eq!(line.verdict, Verdict::Untouched);
    assert_eq!(line.words, vec!["untouched".to_string()]);
    assert_eq!(
        line.dac_stage.as_deref(),
        Some("the Bifrost 3 then lowers it by 3.3 dB")
    );
    assert_eq!(
        line.sentence().as_deref(),
        Some(
            "untouched from the player's output on; the Bifrost 3 then lowers it by 3.3 dB; the \
             player may still change the sound itself (loudness normalization, ReplayGain, its own \
             EQ); svolder judges up to 24 bits"
        )
    );
    let dac = &line.hops[2];
    assert!(dac.changes);
    assert_eq!(dac.detail, "USB, 48 kHz, volume -3.3 dB");
}

#[test]
fn the_dac_names_its_tone_control_and_balance_and_combines_them() {
    let tone = path_line(&bifrost_route(), &bifrost_reading(127, 2, 64));
    assert_eq!(
        tone.dac_stage.as_deref(),
        Some("the Bifrost 3 then shapes it with its tone control")
    );
    let loud = path_line(&bifrost_route(), &bifrost_reading(127, 3, 64));
    assert_eq!(loud.dac_stage, tone.dac_stage);
    let balance = path_line(&bifrost_route(), &bifrost_reading(127, 1, 70));
    assert_eq!(
        balance.dac_stage.as_deref(),
        Some("the Bifrost 3 then shifts the balance")
    );
    let all = path_line(&bifrost_route(), &bifrost_reading(117, 2, 70));
    assert_eq!(
        all.dac_stage.as_deref(),
        Some(
            "the Bifrost 3 then lowers it by 3.3 dB, shapes it with its tone control and shifts \
             the balance"
        )
    );
    assert_eq!(
        all.hops[2].detail,
        "USB, 48 kHz, volume -3.3 dB, tone control, balance off centre"
    );
}

#[test]
fn a_silent_dac_volume_is_named_and_a_changed_path_gets_a_sentence_for_it() {
    let line = path_line(&bifrost_route(), &bifrost_reading(0, 1, 64));
    assert_eq!(
        line.dac_stage.as_deref(),
        Some("the Bifrost 3 then silences it")
    );
    let mut route = bifrost_route();
    route.sink.as_mut().unwrap().volume_milli = Some(540);
    let line = path_line(&route, &bifrost_reading(117, 1, 64));
    assert_eq!(line.verdict, Verdict::Changed);
    assert_eq!(
        line.sentence().as_deref(),
        Some("computer volume 54 %; the Bifrost 3 then lowers it by 3.3 dB")
    );
}

#[test]
fn the_dac_names_its_mute_phase_and_nos_filter() {
    let mut facts = bifrost_reading(127, 1, 64);
    facts.stage.muted = true;
    let line = path_line(&bifrost_route(), &facts);
    assert_eq!(line.dac_stage.as_deref(), Some("the Bifrost 3 is muted"));
    assert_eq!(line.hops[2].detail, "USB, 48 kHz, muted");
    facts.stage.phase = true;
    facts.stage.nos = true;
    let line = path_line(&bifrost_route(), &facts);
    assert_eq!(
        line.dac_stage.as_deref(),
        Some("the Bifrost 3 then inverts the phase and uses its NOS filter; it is muted")
    );
    assert_eq!(
        line.hops[2].detail,
        "USB, 48 kHz, muted, phase inverted, NOS filter"
    );
}

/// Each of the DAC's own mute, balance, phase invert and NOS filter alone makes the end-to-end
/// verdict `Changed` while the path itself stays untouched (the DAC at 0 dB).
#[test]
fn mute_balance_phase_and_nos_each_alone_change_the_verdict() {
    type Set = fn(&mut DacStage);
    let cases: [(&str, Set); 4] = [
        ("mute", |stage| stage.muted = true),
        ("balance", |stage| stage.balance = true),
        ("phase", |stage| stage.phase = true),
        ("nos", |stage| stage.nos = true),
    ];
    for (name, set) in cases {
        let mut facts = bifrost_reading(127, 1, 64);
        set(&mut facts.stage);
        let line = path_line(&bifrost_route(), &facts);
        assert_eq!(line.verdict, Verdict::Untouched, "{name}");
        assert_eq!(line.end_to_end(), EndToEnd::Changed, "{name}");
    }
}

#[test]
fn the_dac_reads_its_toggles_from_the_profile() {
    let bifrost = builtin("bifrost3");
    let id = |key: &str| bifrost.control_by_key(key).expect(key).id;
    let (mute, phase, nos) = (id("mute"), id("phase"), id("nos"));
    let facts = DacFacts::read(&bifrost, |asked| {
        Some(i64::from(asked == mute || asked == phase || asked == nos))
    });
    assert!(facts.stage.muted && facts.stage.phase && facts.stage.nos);
}

// ---- the setup judges ----

/// A layout of the given positions.
fn layout(positions: &[&str]) -> Layout {
    Layout {
        channels: positions.len() as u32,
        positions: positions.iter().map(|p| p.to_string()).collect(),
    }
}

/// Setup facts under which [`clean`]'s route stays untouched: a wide offer, the clock allowing
/// the sink's rate, stereo on both sides, no dither, no filter, an ALSA `hw:` device.
pub(in crate::host) fn clean_setup() -> SetupFacts {
    SetupFacts {
        offer: Some(SinkOffer {
            formats: vec!["S32LE".into(), "S24LE".into(), "S16LE".into()],
            rates: Some(Rates::Range {
                min: 44_100,
                max: 384_000,
            }),
            layout: Some(layout(&["FL", "FR"])),
        }),
        forced_format: None,
        node_rates: Vec::new(),
        clock: Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: vec![44_100, 48_000, 88_200, 96_000, 176_400, 192_000],
            force_rate: None,
        }),
        sink_layout: Some(layout(&["FL", "FR"])),
        stream_layout: Some(layout(&["FL", "FR"])),
        dither: Some(Dither {
            method: "none".into(),
            noise: 0,
        }),
        sink_filter: Some(false),
        stream_filter: Some(false),
        alsa: Some(AlsaFacts {
            path: Some("hw:Audio".into()),
            card: Some(3),
            device: Some(0),
            subdevice: Some(0),
            hw: HwState::NotRead,
            kernel: Some(KernelOffer {
                formats: vec!["S32_LE".into()],
                rates: vec![44_100, 48_000],
            }),
        }),
    }
}

/// [`clean`] with its setup facts, changed by `edit`.
fn judged(edit: impl FnOnce(&mut RouteFacts, &mut SetupFacts)) -> RouteFacts {
    let mut route = clean();
    let mut setup = clean_setup();
    edit(&mut route, &mut setup);
    route.setup = Some(setup);
    route
}

/// The words and verdict of `route` against the test device.
fn judge(route: &RouteFacts) -> (Verdict, Vec<String>) {
    let line = path_line(route, &dac());
    (line.verdict, line.words)
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| w.to_string()).collect()
}

#[test]
fn clean_setup_facts_keep_the_path_untouched() {
    let (verdict, said) = judge(&judged(|_, _| {}));
    assert_eq!(verdict, Verdict::Untouched);
    assert_eq!(said, words(&["untouched"]));
}

#[test]
fn facts_an_older_daemon_did_not_send_add_no_word() {
    // The setup facts are skipped; the two formats an older daemon does send are still judged.
    let mut route = clean();
    route.sink.as_mut().unwrap().format = Some("S16LE".into());
    route.player.as_mut().unwrap().format = Some("S32LE".into());
    assert!(route.setup.is_none());
    assert_eq!(
        judge(&route),
        (Verdict::Changed, words(&["cut to 16 bits (S32 → S16)"]))
    );
    route.sink.as_mut().unwrap().format = Some("S32LE".into());
    assert_eq!(judge(&route), (Verdict::Untouched, words(&["untouched"])));
}

#[test]
fn a_narrower_sink_cuts_the_bits() {
    let route = judged(|route, _| {
        route.player.as_mut().unwrap().format = Some("S32LE".into());
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
    });
    let (verdict, said) = judge(&route);
    assert_eq!(verdict, Verdict::Changed);
    assert_eq!(said, words(&["cut to 16 bits (S32 → S16)"]));
    let line = path_line(&route, &dac());
    assert!(line.hops[1].changes);
}

#[test]
fn format_names_lose_their_endian_and_planar_suffix() {
    let route = judged(|route, _| {
        route.player.as_mut().unwrap().format = Some("S24P".into());
        route.sink.as_mut().unwrap().format = Some("S16BE".into());
    });
    assert_eq!(judge(&route).1, words(&["cut to 16 bits (S24 → S16)"]));
}

#[test]
fn an_s32_stream_into_s24_has_no_word() {
    let route = judged(|route, _| {
        route.player.as_mut().unwrap().format = Some("S32LE".into());
        route.sink.as_mut().unwrap().format = Some("S24LE".into());
    });
    assert_eq!(judge(&route), (Verdict::Untouched, words(&["untouched"])));
    let route = judged(|route, _| {
        route.player.as_mut().unwrap().format = Some("S32LE".into());
        route.sink.as_mut().unwrap().format = Some("S24_32LE".into());
    });
    assert_eq!(judge(&route).0, Verdict::Untouched);
}

#[test]
fn a_float_stream_into_a_narrow_sink_is_unknown() {
    let route = judged(|route, _| {
        route.player.as_mut().unwrap().format = Some("F32LE".into());
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
    });
    assert_eq!(
        judge(&route),
        (
            Verdict::Unknown,
            words(&["float stream into a 16-bit sink"])
        )
    );
    let route = judged(|route, _| {
        route.player.as_mut().unwrap().format = Some("F32P".into());
        route.sink.as_mut().unwrap().format = Some("S24LE".into());
    });
    assert_eq!(judge(&route).0, Verdict::Untouched);
}

#[test]
fn a_format_name_svolder_does_not_know_is_unknown() {
    let route = judged(|route, _| {
        route.player.as_mut().unwrap().format = Some("S8X".into());
    });
    assert_eq!(
        judge(&route),
        (Verdict::Unknown, words(&["format S8X not judged"]))
    );
    let route = judged(|route, _| {
        route.sink.as_mut().unwrap().format = Some("DSD".into());
    });
    assert_eq!(judge(&route).1, words(&["format DSD not judged"]));
}

#[test]
fn channels_that_differ_are_named() {
    let down = judged(|_, setup| setup.sink_layout = Some(layout(&["MONO"])));
    assert_eq!(
        judge(&down),
        (Verdict::Changed, words(&["channels 2 → 1 (mixed)"]))
    );
    let up = judged(|_, setup| {
        setup.sink_layout = Some(layout(&["FL", "FR", "FC", "LFE", "RL", "RR"]));
    });
    assert_eq!(judge(&up).1, words(&["channels 2 → 6 (upmixed)"]));
    let remapped = judged(|_, setup| setup.sink_layout = Some(layout(&["FL", "FC"])));
    assert_eq!(judge(&remapped).1, words(&["channels remapped"]));
}

#[test]
fn channel_positions_compare_as_sets() {
    let route = judged(|_, setup| setup.sink_layout = Some(layout(&["FR", "FL"])));
    assert_eq!(judge(&route).0, Verdict::Untouched);
}

#[test]
fn positions_that_name_none_are_no_remap() {
    let stream_unk = |sink: &[&str]| {
        judged(|_, setup| {
            setup.stream_layout = Some(layout(&["UNK", "UNK"]));
            setup.sink_layout = Some(layout(sink));
        })
    };
    // Spotify's real shape: two unpositioned channels into a stereo sink.
    assert_eq!(
        judge(&stream_unk(&["FL", "FR"])),
        (Verdict::Untouched, words(&["untouched"]))
    );
    assert_eq!(judge(&stream_unk(&["UNK", "UNK"])).0, Verdict::Untouched);
    // PipeWire pairs channels one to one when either side names none: NA and AUXn too.
    assert_eq!(judge(&stream_unk(&["AUX0", "AUX1"])).0, Verdict::Untouched);
    let aux = judged(|_, setup| setup.sink_layout = Some(layout(&["AUX0", "AUX1"])));
    assert_eq!(judge(&aux).0, Verdict::Untouched);
    let na = judged(|_, setup| setup.sink_layout = Some(layout(&["NA", "NA"])));
    assert_eq!(judge(&na).0, Verdict::Untouched);
    // One unpositioned channel is a mono source: it goes to both.
    let mono = judged(|_, setup| setup.stream_layout = Some(layout(&["UNK"])));
    assert_eq!(judge(&mono).0, Verdict::Untouched);
    // Named positions still compare as sets, and a real difference is still a remap or a mix.
    let swapped = judged(|_, setup| setup.sink_layout = Some(layout(&["FR", "FL"])));
    assert_eq!(judge(&swapped).0, Verdict::Untouched);
    let rear = judged(|_, setup| setup.sink_layout = Some(layout(&["RL", "RR"])));
    assert_eq!(judge(&rear).1, words(&["channels remapped"]));
    let up = stream_unk(&["FL", "FR", "FC", "LFE", "RL", "RR"]);
    assert_eq!(judge(&up).1, words(&["channels 2 → 6 (upmixed)"]));
    // A layout mixing named and unnamed positions keeps the set comparison.
    let mixed = judged(|_, setup| setup.stream_layout = Some(layout(&["FL", "UNK"])));
    assert_eq!(judge(&mixed).1, words(&["channels remapped"]));
    // Equal sets of a mix are no proof of a copy: unknown.
    let same_mix = judged(|_, setup| {
        setup.stream_layout = Some(layout(&["FL", "UNK"]));
        setup.sink_layout = Some(layout(&["UNK", "FL"]));
    });
    assert_eq!(
        judge(&same_mix),
        (Verdict::Unknown, words(&["channels partly unnamed"]))
    );
}

#[test]
fn mono_among_several_channels_names_none() {
    // channelmix-ops.c L212-215.
    for stream in [&["MONO", "MONO"], &["FL", "MONO"]] {
        let route = judged(|_, setup| setup.stream_layout = Some(layout(stream)));
        assert_eq!(judge(&route).0, Verdict::Untouched);
    }
}

#[test]
fn one_channel_into_stereo_says_what_pipewire_does() {
    let one = |position: &str| judged(|_, setup| setup.stream_layout = Some(layout(&[position])));
    for position in ["MONO", "UNK", "NA", "AUX0"] {
        assert_eq!(judge(&one(position)).0, Verdict::Untouched, "{position}");
    }
    assert_eq!(
        judge(&one("FC")),
        (
            Verdict::Changed,
            words(&["mono centre 3 dB lower on both channels"])
        )
    );
    for position in ["FL", "FR"] {
        assert_eq!(
            judge(&one(position)).1,
            words(&["one channel silent"]),
            "{position}"
        );
    }
    assert_eq!(judge(&one("LFE")).1, words(&["channels remapped"]));
    // The sink's positions decide first.
    let into = |position: &str, sink: &[&str]| {
        judged(|_, setup| {
            setup.stream_layout = Some(layout(&[position]));
            setup.sink_layout = Some(layout(sink));
        })
    };
    for position in ["FC", "FL", "MONO"] {
        assert_eq!(
            judge(&into(position, &["AUX0", "AUX1"])).0,
            Verdict::Untouched,
            "{position}"
        );
    }
    assert_eq!(
        judge(&into("MONO", &["RL", "RR"])).1,
        words(&["channels remapped"])
    );
}

#[test]
fn mono_into_stereo_is_no_change_and_the_detail_says_so() {
    let route = judged(|_, setup| setup.stream_layout = Some(layout(&["MONO"])));
    let line = path_line(&route, &dac());
    assert_eq!(line.verdict, Verdict::Untouched);
    assert_eq!(line.words, words(&["untouched"]));
    assert_eq!(
        line.hops[1].detail,
        "48 kHz S32LE, volume 100 %, mono on both channels"
    );
}

#[test]
fn a_layout_not_read_on_either_side_is_unknown() {
    for side in 0..2 {
        let route = judged(|_, setup| match side {
            0 => setup.stream_layout = None,
            _ => setup.sink_layout = None,
        });
        assert_eq!(
            judge(&route),
            (Verdict::Unknown, words(&["channels not read"]))
        );
    }
}

#[test]
fn dither_on_the_sink_is_named() {
    let on = judged(|_, setup| {
        setup.dither = Some(Dither {
            method: "triangular".into(),
            noise: 0,
        })
    });
    assert_eq!(
        judge(&on),
        (Verdict::Changed, words(&["dither on (triangular)"]))
    );
    let noise = judged(|_, setup| {
        setup.dither = Some(Dither {
            method: "none".into(),
            noise: 4,
        })
    });
    assert_eq!(judge(&noise).1, words(&["dither noise 4"]));
    let absent = judged(|_, setup| setup.dither = None);
    assert_eq!(
        judge(&absent),
        (Verdict::Unknown, words(&["dither not read"]))
    );
}

#[test]
fn a_filter_inside_a_converter_is_named() {
    let sink = judged(|_, setup| setup.sink_filter = Some(true));
    assert_eq!(
        judge(&sink),
        (Verdict::Changed, words(&["a filter inside the sink"]))
    );
    let stream = judged(|_, setup| setup.stream_filter = Some(true));
    assert_eq!(
        judge(&stream).1,
        words(&["a filter on the player's stream"])
    );
    let both = judged(|_, setup| {
        setup.sink_filter = Some(true);
        setup.stream_filter = Some(true);
    });
    assert_eq!(
        judge(&both).1,
        words(&[
            "a filter inside the sink",
            "a filter on the player's stream"
        ])
    );
    let unread = judged(|_, setup| setup.sink_filter = None);
    assert_eq!(
        judge(&unread),
        (Verdict::Unknown, words(&["filter not read"]))
    );
    let unread = judged(|_, setup| setup.stream_filter = None);
    assert_eq!(judge(&unread).1, words(&["filter not read"]));
}

fn open(format: &str, rate: u32, channels: u32) -> HwState {
    HwState::Open(HwParams {
        format: format.into(),
        rate,
        channels,
    })
}

fn with_alsa(path: Option<&str>, hw: HwState) -> RouteFacts {
    judged(|_, setup| {
        let alsa = setup.alsa.as_mut().unwrap();
        alsa.path = path.map(str::to_string);
        alsa.hw = hw;
    })
}

#[test]
fn an_open_card_equal_to_the_sink_is_no_word_whatever_the_name() {
    for path in ["hw:Audio", "plughw:1", "default", "front:3"] {
        let route = with_alsa(Some(path), open("S32_LE", 48_000, 2));
        assert_eq!(
            judge(&route),
            (Verdict::Untouched, words(&["untouched"])),
            "{path}"
        );
    }
}

#[test]
fn an_open_card_that_differs_says_what_alsa_converts() {
    let route = with_alsa(Some("hw:Audio"), open("S16_LE", 48_000, 2));
    assert_eq!(
        judge(&route),
        (Verdict::Changed, words(&["ALSA converts S32LE → S16_LE"]))
    );
    let route = with_alsa(Some("hw:Audio"), open("S32_LE", 44_100, 6));
    assert_eq!(
        judge(&route).1,
        words(&["ALSA converts 48 → 44.1 kHz, channels 2 → 6"])
    );
    let route = with_alsa(Some("hw:Audio"), open("S24_3LE", 48_000, 2));
    assert_eq!(judge(&route).1, words(&["ALSA converts S32LE → S24_3LE"]));
}

#[test]
fn a_closed_card_is_judged_by_its_name() {
    for hw in [HwState::Closed, HwState::NotRead] {
        let plug = with_alsa(Some("plughw:1"), hw.clone());
        assert_eq!(
            judge(&plug),
            (Verdict::Changed, words(&["ALSA plug converts (plughw:1)"]))
        );
        let plain = with_alsa(Some("plug:hw:0"), hw.clone());
        assert_eq!(judge(&plain).1, words(&["ALSA plug converts (plug:hw:0)"]));
        let dmix = with_alsa(Some("dmix:0"), hw.clone());
        assert_eq!(
            judge(&dmix),
            (Verdict::Changed, words(&["ALSA dmix mixes (dmix:0)"]))
        );
        for fine in ["hw:Audio", "front:3", "hdmi:0", "iec958:3"] {
            let route = with_alsa(Some(fine), hw.clone());
            assert_eq!(judge(&route).0, Verdict::Untouched, "{fine}");
        }
        let other = with_alsa(Some("default"), hw.clone());
        assert_eq!(
            judge(&other),
            (Verdict::Unknown, words(&["ALSA device default not judged"]))
        );
        let none = with_alsa(None, hw);
        assert_eq!(
            judge(&none),
            (Verdict::Unknown, words(&["ALSA device not read"]))
        );
    }
}

#[test]
fn a_sink_that_is_not_alsa_is_not_judged_for_alsa() {
    let route = judged(|_, setup| setup.alsa = None);
    assert_eq!(judge(&route).0, Verdict::Untouched);
}

#[test]
fn a_profile_name_is_never_a_verdict() {
    // An iec958 route that converts is named as converting, one that does not is silent.
    let converts = with_alsa(Some("iec958:3"), open("S16_LE", 48_000, 2));
    assert_eq!(judge(&converts).0, Verdict::Changed);
    let same = with_alsa(Some("iec958:3"), open("S32_LE", 48_000, 2));
    assert_eq!(judge(&same).0, Verdict::Untouched);
}

#[test]
fn a_forced_rate_that_causes_the_resample_is_named() {
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock.as_mut().unwrap().force_rate = Some(48_000);
    });
    assert_eq!(
        judge(&route),
        (
            Verdict::Changed,
            words(&["resampled 44.1 → 48 kHz (rate forced)"])
        )
    );
}

/// The computer-side judgement of `route` (no DAC hop, so a sink rate other than the test
/// device's is not a DAC word).
fn judge_computer(route: &RouteFacts) -> (Verdict, Vec<String>) {
    let line = computer_line(route);
    (line.verdict, line.words)
}

#[test]
fn a_forced_rate_over_a_sink_still_reading_the_stream_rate_is_named() {
    // The sink's Format still reads 44100 while the clock is forced to 48000: the sound is
    // resampled all the same, in the path line and the computer line alike.
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        route.sink.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock.as_mut().unwrap().force_rate = Some(48_000);
    });
    let expected = (
        Verdict::Changed,
        words(&["resampled 44.1 → 48 kHz (rate forced)"]),
    );
    assert_eq!(judge_computer(&route), expected);
    let (verdict, said) = judge(&route);
    assert_eq!(verdict, Verdict::Changed);
    assert!(said.contains(&expected.1[0]), "{said:?}");
}

#[test]
fn a_forced_rate_equal_to_the_stream_rate_is_no_word_at_any_sink_rate() {
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        route.sink.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock.as_mut().unwrap().force_rate = Some(44_100);
    });
    assert_eq!(
        judge_computer(&route),
        (Verdict::Untouched, words(&["untouched"]))
    );
}

#[test]
fn no_forced_rate_and_no_stream_rate_invent_no_word() {
    let route = judged(|route, _| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        route.sink.as_mut().unwrap().rate_hz = Some(44_100);
    });
    assert_eq!(
        judge_computer(&route),
        (Verdict::Untouched, words(&["untouched"]))
    );
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = None;
        route.sink.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock.as_mut().unwrap().force_rate = Some(48_000);
    });
    assert!(
        !judge_computer(&route)
            .1
            .iter()
            .any(|w| w.contains("forced")),
        "a stream rate not read gives no forced-rate word"
    );
}

#[test]
fn a_forced_rate_the_stream_already_has_is_no_word() {
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(48_000);
        setup.clock.as_mut().unwrap().force_rate = Some(48_000);
    });
    assert_eq!(judge(&route).0, Verdict::Untouched);
}

#[test]
fn a_forced_rate_over_a_third_sink_rate_is_named() {
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock.as_mut().unwrap().force_rate = Some(96_000);
    });
    assert_eq!(
        judge(&route).1,
        words(&["resampled 44.1 → 48 kHz (rate forced)"])
    );
}

#[test]
fn a_resample_the_force_did_not_cause_has_no_suffix() {
    let route = judged(|route, _| route.player.as_mut().unwrap().rate_hz = Some(44_100));
    assert_eq!(judge(&route).1, words(&["resampled 44.1 → 48 kHz"]));
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(48_000);
        route.sink.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock.as_mut().unwrap().force_rate = Some(48_000);
    });
    assert_eq!(
        judge_computer(&route).1,
        words(&["resampled 48 → 44.1 kHz"])
    );
}

#[test]
fn a_clock_not_read_with_equal_rates_is_unknown_never_untouched() {
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        route.sink.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock = None;
    });
    let (verdict, said) = judge_computer(&route);
    assert_eq!(verdict, Verdict::Unknown);
    assert!(
        said.iter().any(|w| w.contains("forced rate not read")),
        "{said:?}"
    );
    // A route that carries no setup at all (an older daemon) stays as it was.
    let mut older = clean();
    older.player.as_mut().unwrap().rate_hz = Some(44_100);
    older.sink.as_mut().unwrap().rate_hz = Some(44_100);
    older.setup = None;
    assert_eq!(judge_computer(&older).0, Verdict::Untouched);
}

#[test]
fn the_new_words_come_in_the_stated_order_after_the_old_ones() {
    let route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        route.player.as_mut().unwrap().format = Some("S32LE".into());
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
        route.others_mixing = 1;
        setup.sink_layout = Some(layout(&["MONO"]));
        setup.dither = Some(Dither {
            method: "triangular".into(),
            noise: 0,
        });
        setup.sink_filter = Some(true);
        let alsa = setup.alsa.as_mut().unwrap();
        alsa.path = Some("plughw:1".into());
    });
    assert_eq!(
        judge(&route),
        (
            Verdict::Changed,
            words(&[
                "resampled 44.1 → 48 kHz",
                "1 other sound mixing",
                "cut to 16 bits (S32 → S16)",
                "channels 2 → 1 (mixed)",
                "dither on (triangular)",
                "a filter inside the sink",
                "ALSA plug converts (plughw:1)",
            ])
        )
    );
}

#[test]
fn a_change_outranks_an_unknown() {
    let route = judged(|_, setup| {
        setup.dither = None;
        setup.sink_filter = Some(true);
    });
    assert_eq!(
        judge(&route),
        (Verdict::Changed, words(&["a filter inside the sink"]))
    );
}

#[test]
fn the_new_words_never_say_bit_perfect_or_a_stage_word() {
    // The five words themes/nightdrive/hud.js 24 (`STAGE_WORDS`) picks out of the path.
    const STAGE_WORDS: [&str; 5] = [
        "host EQ on",
        "loudness on",
        "crossfeed on",
        "virtual speakers on",
        "EQ pass-through",
    ];
    let mut routes = vec![
        judged(|route, setup| {
            route.player.as_mut().unwrap().format = Some("F32LE".into());
            route.sink.as_mut().unwrap().format = Some("S16LE".into());
            setup.sink_layout = None;
            setup.dither = None;
            setup.sink_filter = None;
        }),
        judged(|route, setup| {
            route.player.as_mut().unwrap().format = Some("S8X".into());
            setup.sink_layout = Some(layout(&["MONO"]));
            setup.dither = Some(Dither {
                method: "triangular".into(),
                noise: 4,
            });
            setup.sink_filter = Some(true);
            setup.stream_filter = Some(true);
            setup.clock.as_mut().unwrap().force_rate = Some(48_000);
            route.player.as_mut().unwrap().rate_hz = Some(44_100);
        }),
    ];
    for path in ["plughw:1", "dmix:0", "default"] {
        routes.push(with_alsa(Some(path), HwState::Closed));
    }
    routes.push(with_alsa(Some("hw:0"), open("S16_LE", 44_100, 6)));
    for route in routes {
        let text = all_text(&path_line(&route, &dac()));
        let lower = text.to_lowercase();
        assert!(
            !lower.contains("bit-perfect") && !lower.contains("bit-exact"),
            "{text}"
        );
        let line = computer_line(&route);
        for word in &line.words {
            assert!(
                STAGE_WORDS.iter().all(|stage| !word.contains(stage)),
                "{word} holds a stage word"
            );
        }
    }
}

#[test]
fn computer_line_is_the_first_two_hops_without_a_dac_hop() {
    let mut route = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        setup.dither = Some(Dither {
            method: "triangular".into(),
            noise: 0,
        });
    });
    for host_eq in [false, true] {
        route.host_eq = host_eq;
        let full = path_line(&route, &dac());
        let line = computer_line(&route);
        assert_eq!(line.hops, full.hops[..2].to_vec());
        assert_eq!(line.verdict, full.verdict);
        assert_eq!(line.dac_stage, None);
    }
    let clean = computer_line(&judged(|_, _| {}));
    assert_eq!(clean.verdict, Verdict::Untouched);
    assert_eq!(clean.words, words(&["untouched"]));
    assert_eq!(clean.hops.len(), 2);
}

#[test]
fn computer_line_judges_no_sink_and_a_player_elsewhere() {
    let none = computer_line(&RouteFacts::default());
    assert_eq!(none.verdict, Verdict::Unknown);
    assert_eq!(none.words, words(&["the computer's sink was not read"]));
    let mut route = clean();
    route.player_match = PlayerMatch::Elsewhere {
        sink: "HDMI".into(),
    };
    let line = computer_line(&route);
    assert_eq!(
        line.verdict,
        Verdict::NotThisDac {
            sink: "HDMI".into(),
            by_player: true
        }
    );
}

#[test]
fn computer_line_keeps_the_dac_out_of_the_verdict() {
    // A DAC that reads another rate changes path_line's verdict, never computer_line's.
    let route = judged(|_, _| {});
    let off = DacFacts {
        rate_hz: Some(44_100),
        ..dac()
    };
    assert_eq!(path_line(&route, &off).verdict, Verdict::Changed);
    assert_eq!(computer_line(&route).verdict, Verdict::Untouched);
}

// ---- fix 1 ----

#[test]
fn a_stream_format_not_read_is_unknown_into_a_narrow_sink() {
    for with_setup in [false, true] {
        let mut route = if with_setup {
            judged(|_, _| {})
        } else {
            clean()
        };
        route.player.as_mut().unwrap().format = None;
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
        assert_eq!(
            judge(&route),
            (Verdict::Unknown, words(&["player format not read"]))
        );
        // A sink of 24 bits or more, or a float one, needs no stream format.
        for wide in ["S24LE", "S32LE", "F32P"] {
            route.sink.as_mut().unwrap().format = Some(wide.into());
            assert_eq!(judge(&route).0, Verdict::Untouched, "{wide}");
        }
    }
}

#[test]
fn an_hw_format_the_judge_cannot_read_is_not_equal() {
    for format in ["FLOAT_LE", "IEC958_SUBFRAME_LE", ""] {
        // The device name still rules: plug converts whatever the card shows.
        let plug = with_alsa(Some("plughw:1"), open(format, 48_000, 2));
        assert_eq!(
            judge(&plug),
            (Verdict::Changed, words(&["ALSA plug converts (plughw:1)"])),
            "{format}"
        );
        let hw = with_alsa(Some("hw:Audio"), open(format, 48_000, 2));
        assert_eq!(judge(&hw).0, Verdict::Unknown, "{format}");
    }
    let hw = with_alsa(Some("hw:Audio"), open("FLOAT_LE", 48_000, 2));
    assert_eq!(judge(&hw).1, words(&["ALSA format FLOAT_LE not judged"]));
    let hw = with_alsa(Some("hw:Audio"), open("", 48_000, 2));
    assert_eq!(judge(&hw).1, words(&["ALSA format not read"]));
}

#[test]
fn dmix_is_changed_whatever_the_card_shows() {
    let route = with_alsa(Some("dmix:0"), open("S32_LE", 48_000, 2));
    assert_eq!(
        judge(&route),
        (Verdict::Changed, words(&["ALSA dmix mixes (dmix:0)"]))
    );
}

#[test]
fn laptop_bifrost_stays_untouched_on_both_profiles() {
    // The real laptop values: stream0's three formats and eight rates, hw_params S32_LE 2ch 44100
    // on iec958:3 and on front:3, closed while idle.
    let kernel = KernelOffer {
        formats: vec!["S16_LE".into(), "S24_3LE".into(), "S32_LE".into()],
        rates: vec![
            44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
        ],
    };
    let device = DacFacts {
        rate_hz: Some(44_100),
        ..dac()
    };
    for path in ["iec958:3", "front:3"] {
        for hw in [open("S32_LE", 44_100, 2), HwState::Closed] {
            let mut route = judged(|route, setup| {
                route.sink.as_mut().unwrap().rate_hz = Some(44_100);
                route.player.as_mut().unwrap().rate_hz = Some(44_100);
                let alsa = setup.alsa.as_mut().unwrap();
                alsa.path = Some(path.into());
                alsa.kernel = Some(kernel.clone());
                alsa.hw = hw.clone();
            });
            for format in ["S16LE", "S32LE", "F32LE"] {
                route.player.as_mut().unwrap().format = Some(format.into());
                let line = path_line(&route, &device);
                assert_eq!(line.verdict, Verdict::Untouched, "{path} {format}");
            }
        }
    }
}

#[test]
fn the_24_bit_containers_compare_by_width() {
    for (sink, hw) in [
        ("S24_32LE", "S24_LE"),
        ("S24LE", "S24_3LE"),
        ("S24LE", "S24_LE"),
    ] {
        let route = judged(|route, setup| {
            route.sink.as_mut().unwrap().format = Some(sink.into());
            setup.alsa.as_mut().unwrap().hw = open(hw, 48_000, 2);
        });
        assert_eq!(judge(&route).0, Verdict::Untouched, "{sink} {hw}");
    }
}

#[test]
fn mono_into_six_channels_is_upmixed() {
    let route = judged(|_, setup| {
        setup.stream_layout = Some(layout(&["MONO"]));
        setup.sink_layout = Some(layout(&["FL", "FR", "FC", "LFE", "RL", "RR"]));
    });
    assert_eq!(judge(&route).1, words(&["channels 1 → 6 (upmixed)"]));
}

#[test]
fn a_stream_that_is_not_the_players_is_not_judged() {
    let mut route = judged(|route, setup| {
        route.player_match = PlayerMatch::NotFound;
        route.player.as_mut().unwrap().format = Some("F32LE".into());
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
        setup.stream_layout = None;
        setup.stream_filter = None;
        setup.sink_layout = Some(layout(&["MONO"]));
    });
    route.player = None;
    let said = judge(&route).1;
    assert_eq!(said, words(&["player not identified"]));
}

#[test]
fn an_unreadable_sink_format_is_named_as_the_sinks_not_alsas() {
    let route = judged(|route, setup| {
        route.sink.as_mut().unwrap().format = Some("DSD".into());
        setup.alsa.as_mut().unwrap().hw = open("S32_LE", 48_000, 2);
    });
    assert_eq!(judge(&route).1, words(&["format DSD not judged"]));
}

/// Every verdict's key, spelled out: JSON consumers (the web card, Home Assistant, MCP, the CLI's
/// `--json`) read these, so a renamed key must fail here.
#[test]
fn the_verdict_keys_are_pinned() {
    let all = [
        (Verdict::Untouched, "untouched"),
        (Verdict::Changed, "changed"),
        (
            Verdict::NotThisDac {
                sink: "DELL".into(),
                by_player: false,
            },
            "not_this_dac",
        ),
        (
            Verdict::OtherInput {
                input: "Optical".into(),
            },
            "other_input",
        ),
        (Verdict::Unknown, "unknown"),
    ];
    for (verdict, key) in &all {
        assert_eq!(verdict.key(), *key);
    }
    assert_eq!(all.len(), Verdict::KEYS.len());
    assert_eq!(
        all.iter().map(|(_, key)| *key).collect::<Vec<_>>(),
        Verdict::KEYS
    );
}

/// What `hop` of `route`'s path records as changes: each one's full words and short form.
fn recorded(route: &RouteFacts, hop: usize) -> Vec<(String, String)> {
    path_line(route, &dac()).hops[hop]
        .changed
        .iter()
        .map(|change| (change.words.clone(), change.short.clone()))
        .collect()
}

/// A recorded change as a pair of strs, for comparing.
fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(words, short)| (words.to_string(), short.to_string()))
        .collect()
}

#[test]
fn the_player_records_its_own_volume_and_mute() {
    let mut route = clean();
    route.player.as_mut().unwrap().volume_milli = Some(500);
    route.player.as_mut().unwrap().muted = true;
    assert_eq!(
        recorded(&route, 0),
        pairs(&[
            ("player volume 50 %", "volume 50 %"),
            ("player muted", "muted")
        ])
    );
    assert!(recorded(&route, 1).is_empty() && recorded(&route, 2).is_empty());
}

#[test]
fn the_computer_records_resampling_volume_eq_and_mixing() {
    let mut route = clean();
    route.player.as_mut().unwrap().rate_hz = Some(44_100);
    route.sink.as_mut().unwrap().volume_milli = Some(800);
    route.sink.as_mut().unwrap().muted = true;
    route.others_mixing = 3;
    route.host_eq = true;
    route.stages = Some(ChainStages {
        eq: true,
        loudness: true,
        crossfeed: true,
        speakers: true,
    });
    let line = path_line(&route, &dac());
    assert_eq!(
        recorded(&route, 1),
        pairs(&[
            ("resampled 44.1 → 48 kHz", "resampled"),
            ("computer volume 80 %", "volume 80 %"),
            ("computer muted", "muted"),
            ("host EQ on", "EQ"),
            ("loudness on", "EQ"),
            ("crossfeed on", "EQ"),
            ("virtual speakers on", "EQ"),
            ("3 other sounds mixing", "3 other sounds mixing"),
        ])
    );
    // The path's words are the hops' full words in hop order, as before.
    assert_eq!(line.words.len(), 8);
    assert_eq!(line.words[0], "resampled 44.1 → 48 kHz");
    route.others_mixing = 1;
    assert_eq!(
        recorded(&route, 1).last().unwrap(),
        &(
            "1 other sound mixing".to_string(),
            "1 other sound mixing".to_string()
        )
    );
}

#[test]
fn the_hops_carry_the_rates_the_judge_read() {
    let mut route = clean();
    route.player.as_mut().unwrap().rate_hz = Some(44_100);
    let line = path_line(&route, &dac());
    assert_eq!(line.hops[0].rate_hz, Some(44_100));
    assert_eq!(line.hops[1].rate_hz, Some(48_000));
    assert_eq!(line.hops[1].resamples_to, Some(48_000));
    assert_eq!(line.hops[2].rate_hz, Some(48_000));
    let clean_line = path_line(&clean(), &dac());
    assert_eq!(clean_line.hops[1].resamples_to, None);
    // The resample ends at the rate the words print: the sink's, whatever the clock is forced to.
    let forced = judged(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock.as_mut().unwrap().force_rate = Some(96_000);
    });
    let line = path_line(&forced, &dac());
    assert_eq!(line.hops[1].resamples_to, Some(48_000));
    assert_eq!(
        line.words,
        vec!["resampled 44.1 → 48 kHz (rate forced)".to_string()]
    );
    // While the sink still reads the stream's rate under a forced clock, the words say
    // `→ forced`, and so does the row.
    let reads_stream = judged(|route, setup| {
        route.sink.as_mut().unwrap().rate_hz = Some(44_100);
        route.player.as_mut().unwrap().rate_hz = Some(44_100);
        setup.clock.as_mut().unwrap().force_rate = Some(96_000);
    });
    let line = path_line(&reads_stream, &dac());
    assert_eq!(line.hops[1].resamples_to, Some(96_000));
    assert_eq!(line.words[0], "resampled 44.1 → 96 kHz (rate forced)");
}

#[test]
fn the_setup_changes_have_short_forms() {
    let short = |route: &RouteFacts| {
        recorded(route, 1)
            .into_iter()
            .map(|(_, short)| short)
            .collect::<Vec<_>>()
    };
    let cut = judged(|route, _| {
        route.player.as_mut().unwrap().format = Some("S32LE".into());
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
    });
    assert_eq!(short(&cut), ["cut to 16 bits"]);
    let mixed = judged(|_, setup| setup.sink_layout = Some(layout(&["MONO"])));
    assert_eq!(short(&mixed), ["channels mixed"]);
    let up = judged(|_, setup| {
        setup.sink_layout = Some(layout(&["FL", "FR", "FC", "LFE", "RL", "RR"]));
    });
    assert_eq!(short(&up), ["channels upmixed"]);
    let remap = judged(|_, setup| setup.sink_layout = Some(layout(&["FL", "FC"])));
    assert_eq!(short(&remap), ["channels remapped"]);
    let centre = judged(|_, setup| setup.stream_layout = Some(layout(&["FC"])));
    assert_eq!(short(&centre), ["mono centre -3 dB"]);
    let silent = judged(|_, setup| setup.stream_layout = Some(layout(&["FL"])));
    assert_eq!(short(&silent), ["one channel silent"]);
    let dither = judged(|_, setup| {
        setup.dither = Some(Dither {
            method: "triangular".into(),
            noise: 3,
        })
    });
    assert_eq!(short(&dither), ["dither", "dither"]);
    let sink = judged(|_, setup| setup.sink_filter = Some(true));
    assert_eq!(short(&sink), ["sink filter"]);
    let stream = judged(|_, setup| setup.stream_filter = Some(true));
    assert_eq!(short(&stream), ["stream filter"]);
    let dmix = with_alsa(Some("dmix:0"), open("S32_LE", 48_000, 2));
    assert_eq!(short(&dmix), ["ALSA dmix"]);
    let converts = with_alsa(Some("hw:Audio"), open("S16_LE", 48_000, 2));
    assert_eq!(short(&converts), ["ALSA converts"]);
    let plug = with_alsa(Some("plughw:1"), HwState::Closed);
    assert_eq!(short(&plug), ["ALSA plug"]);
}

#[test]
fn the_dac_records_the_rate_it_reads_and_its_stage_rides_on_the_hop() {
    let mut device = dac();
    device.rate_hz = Some(44_100);
    device.input = Some("USB 1".into());
    device.stage.volume_db = Some(-24.1);
    let line = path_line(&clean(), &device);
    let hop = &line.hops[2];
    assert_eq!(
        hop.changed,
        vec![Change {
            words: "DAC reads 44.1 kHz".into(),
            short: "reads 44.1 kHz".into()
        }]
    );
    assert_eq!(hop.input.as_deref(), Some("USB 1"));
    assert_eq!(hop.stage, Some(device.stage));
    assert_eq!(hop.rate_hz, Some(44_100));
    assert!(line.hops[0].stage.is_none() && line.hops[1].stage.is_none());
}
