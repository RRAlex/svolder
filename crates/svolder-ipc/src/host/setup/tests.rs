// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::host::path::tests::{clean, clean_setup};
use svolder_host::{
    AlsaFacts, ClockFacts, Dither, HwParams, KernelOffer, Layout, Rates, SinkOffer,
};

/// [`clean`] with [`clean_setup`] changed by `edit`.
fn route(edit: impl FnOnce(&mut RouteFacts, &mut SetupFacts)) -> RouteFacts {
    let mut route = clean();
    let mut setup = clean_setup();
    edit(&mut route, &mut setup);
    route.setup = Some(setup);
    route
}

/// The one line with `key`.
fn only(check: &SetupCheck, key: &str) -> (SetupOutcome, String) {
    let found = check.lines.iter().find(|l| l.key == key).expect(key);
    (found.outcome, found.words.clone())
}

fn said(outcome: SetupOutcome, words: &str) -> (SetupOutcome, String) {
    (outcome, words.to_string())
}

fn layout(positions: &[&str]) -> Layout {
    Layout {
        channels: positions.len() as u32,
        positions: positions.iter().map(|p| p.to_string()).collect(),
    }
}

#[test]
fn a_clean_setup_passes_all_seven_lines_in_order() {
    let check = setup_check(&route(|_, _| {}));
    let keys: Vec<_> = check.lines.iter().map(|l| l.key.as_str()).collect();
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
    assert!(check.lines.iter().all(|l| l.outcome == SetupOutcome::Pass));
    assert_eq!(check.summary, SetupSummary::Untouched);
    assert_eq!(
        check.summary_words(),
        "PipeWire config lets it through untouched"
    );
    assert_eq!(check.chip(), "config ✓");
    assert_eq!(check.sink, "Schiit Gungnir 2 Analog Stereo");
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Pass, "44.1-48 kHz all allowed")
    );
    assert_eq!(
        only(&check, "forced_rate"),
        said(SetupOutcome::Pass, "none")
    );
    assert_eq!(only(&check, "dither"), said(SetupOutcome::Pass, "off"));
    assert_eq!(
        only(&check, "format"),
        said(SetupOutcome::Pass, "S32LE in use")
    );
    assert_eq!(only(&check, "plug"), said(SetupOutcome::Pass, "hw:Audio"));
    assert_eq!(only(&check, "stereo"), said(SetupOutcome::Pass, "FL FR"));
    assert_eq!(only(&check, "filter"), said(SetupOutcome::Pass, "none"));
}

#[test]
fn the_lines_have_their_labels() {
    let check = setup_check(&route(|_, _| {}));
    let labels: Vec<_> = check.lines.iter().map(|l| l.label()).collect();
    assert_eq!(
        labels,
        [
            "rates",
            "forced rate",
            "dither",
            "24-bit output",
            "no plug",
            "stereo to stereo",
            "no filter in the sink"
        ]
    );
}

#[test]
fn rates_the_setup_would_resample_are_listed_with_what_is_allowed() {
    let check = setup_check(&route(|_, setup| {
        setup.alsa.as_mut().unwrap().kernel = Some(KernelOffer {
            formats: vec!["S32_LE".into()],
            rates: vec![44_100, 48_000, 88_200, 96_000, 176_400, 352_800],
        });
        setup.clock.as_mut().unwrap().allowed_rates = vec![48_000, 96_000];
    }));
    assert_eq!(
        only(&check, "rates"),
        said(
            SetupOutcome::Fail,
            "44.1, 88.2, 176.4, 352.8 kHz resampled (allowed: 48, 96)"
        )
    );
    assert_eq!(check.summary, SetupSummary::Changes(1));
    assert_eq!(
        check.summary_words(),
        "PipeWire config changes the sound: 1"
    );
    assert_eq!(check.chip(), "config 1");
}

#[test]
fn a_range_offer_means_the_standard_rates_inside_it() {
    let check = setup_check(&route(|_, setup| {
        setup.alsa.as_mut().unwrap().kernel = None;
        setup.clock.as_mut().unwrap().allowed_rates = vec![48_000];
    }));
    assert_eq!(
        only(&check, "rates"),
        said(
            SetupOutcome::Fail,
            "44.1, 88.2, 96, 176.4, 192, 352.8, 384 kHz resampled (allowed: 48)"
        )
    );
    let check = setup_check(&route(|_, setup| {
        setup.alsa.as_mut().unwrap().kernel = None;
        setup.offer.as_mut().unwrap().rates = Some(Rates::Range {
            min: 44_100,
            max: 96_000,
        });
        setup.clock.as_mut().unwrap().allowed_rates = vec![44_100, 48_000, 88_200, 96_000];
    }));
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Pass, "44.1-96 kHz all allowed")
    );
}

#[test]
fn the_other_offers_read_as_their_values() {
    let check = setup_check(&route(|_, setup| {
        setup.alsa.as_mut().unwrap().kernel = None;
        setup.offer.as_mut().unwrap().rates = Some(Rates::One(48_000));
    }));
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Pass, "48 kHz allowed")
    );
    let check = setup_check(&route(|_, setup| {
        setup.alsa.as_mut().unwrap().kernel = None;
        setup.offer.as_mut().unwrap().rates = Some(Rates::List(vec![44_100, 48_000, 96_000]));
        setup.clock.as_mut().unwrap().allowed_rates = vec![44_100, 48_000];
    }));
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Fail, "96 kHz resampled (allowed: 44.1, 48)")
    );
}

#[test]
fn rates_over_384_khz_are_never_listed() {
    let check = setup_check(&route(|_, setup| {
        setup.alsa.as_mut().unwrap().kernel = Some(KernelOffer {
            formats: Vec::new(),
            rates: vec![48_000, 96_000, 705_600, 768_000],
        });
        setup.clock.as_mut().unwrap().allowed_rates = vec![48_000, 768_000];
    }));
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Fail, "96 kHz resampled (allowed: 48)")
    );
    // Rates above the ceiling alone judge nothing.
    let check = setup_check(&route(|_, setup| {
        setup.alsa.as_mut().unwrap().kernel = Some(KernelOffer {
            formats: Vec::new(),
            rates: vec![705_600],
        });
        setup.offer.as_mut().unwrap().rates = Some(Rates::One(768_000));
    }));
    assert_eq!(only(&check, "rates").0, SetupOutcome::Unknown);
}

#[test]
fn the_nodes_own_rates_beat_the_clocks() {
    let check = setup_check(&route(|_, setup| {
        setup.node_rates = vec![44_100];
        setup.clock.as_mut().unwrap().allowed_rates = vec![48_000];
    }));
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Fail, "48 kHz resampled (allowed: 44.1)")
    );
}

#[test]
fn rates_not_read_are_unknown() {
    let check = setup_check(&route(|_, setup| setup.clock = None));
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Unknown, "allowed rates not read")
    );
    let check = setup_check(&route(|_, setup| {
        setup.clock.as_mut().unwrap().allowed_rates.clear()
    }));
    assert_eq!(only(&check, "rates").0, SetupOutcome::Unknown);
    let check = setup_check(&route(|_, setup| {
        setup.alsa.as_mut().unwrap().kernel = None;
        setup.offer = None;
    }));
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Unknown, "the DAC's rates not read")
    );
}

#[test]
fn a_forced_rate_always_fails_its_line() {
    let check = setup_check(&route(|_, setup| {
        setup.clock.as_mut().unwrap().force_rate = Some(96_000);
    }));
    assert_eq!(
        only(&check, "forced_rate"),
        said(
            SetupOutcome::Fail,
            "96 kHz forced: every other rate is resampled"
        )
    );
    // Even when the stream plays at that rate.
    let check = setup_check(&route(|route, setup| {
        route.player.as_mut().unwrap().rate_hz = Some(48_000);
        setup.clock.as_mut().unwrap().force_rate = Some(48_000);
    }));
    assert_eq!(only(&check, "forced_rate").0, SetupOutcome::Fail);
    let check = setup_check(&route(|_, setup| setup.clock = None));
    assert_eq!(only(&check, "forced_rate").0, SetupOutcome::Unknown);
}

#[test]
fn dither_is_named_when_it_is_on() {
    let on = |method: &str, noise| {
        setup_check(&route(|_, setup| {
            setup.dither = Some(Dither {
                method: method.into(),
                noise,
            })
        }))
    };
    assert_eq!(
        only(&on("triangular", 0), "dither"),
        said(SetupOutcome::Fail, "triangular")
    );
    assert_eq!(
        only(&on("none", 4), "dither"),
        said(SetupOutcome::Fail, "noise 4")
    );
    assert_eq!(
        only(&on("triangular", 2), "dither"),
        said(SetupOutcome::Fail, "triangular, noise 2")
    );
    let unread = setup_check(&route(|_, setup| setup.dither = None));
    assert_eq!(
        only(&unread, "dither"),
        said(SetupOutcome::Unknown, "not read")
    );
}

#[test]
fn the_format_line_judges_what_is_in_use_and_names_what_is_available() {
    let narrow = setup_check(&route(|route, _| {
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
    }));
    assert_eq!(
        only(&narrow, "format"),
        said(SetupOutcome::Fail, "S16LE in use, S32LE available")
    );
    // With no kernel list the offer says what is available.
    let offer = setup_check(&route(|route, setup| {
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
        setup.alsa.as_mut().unwrap().kernel = None;
        setup.offer.as_mut().unwrap().formats = vec!["S16LE".into(), "S24LE".into()];
    }));
    assert_eq!(
        only(&offer, "format"),
        said(SetupOutcome::Fail, "S16LE in use, S24LE available")
    );
    let nothing = setup_check(&route(|route, setup| {
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
        setup.alsa.as_mut().unwrap().kernel = None;
        setup.offer.as_mut().unwrap().formats = vec!["S16LE".into()];
    }));
    assert_eq!(
        only(&nothing, "format"),
        said(SetupOutcome::Fail, "S16LE in use")
    );
}

#[test]
fn an_idle_sink_is_judged_on_what_it_will_play() {
    let idle = setup_check(&route(|route, _| {
        route.sink.as_mut().unwrap().format = None
    }));
    assert_eq!(
        only(&idle, "format"),
        said(SetupOutcome::Pass, "S32LE (when it plays)")
    );
    let forced = setup_check(&route(|route, setup| {
        route.sink.as_mut().unwrap().format = None;
        setup.forced_format = Some("S16LE".into());
    }));
    assert_eq!(
        only(&forced, "format"),
        said(SetupOutcome::Fail, "S16LE (when it plays), S32LE available")
    );
    let unread = setup_check(&route(|route, setup| {
        route.sink.as_mut().unwrap().format = None;
        setup.offer = Some(SinkOffer::default());
    }));
    assert_eq!(only(&unread, "format").0, SetupOutcome::Unknown);
}

#[test]
fn a_float_or_unknown_format_is_judged_as_such() {
    let float = setup_check(&route(|route, _| {
        route.sink.as_mut().unwrap().format = Some("F32P".into());
    }));
    assert_eq!(
        only(&float, "format"),
        said(SetupOutcome::Pass, "F32P in use")
    );
    let odd = setup_check(&route(|route, _| {
        route.sink.as_mut().unwrap().format = Some("DSD".into());
    }));
    assert_eq!(
        only(&odd, "format"),
        said(SetupOutcome::Unknown, "format DSD not judged")
    );
}

fn with_alsa(path: Option<&str>, hw: HwState) -> SetupCheck {
    setup_check(&route(|_, setup| {
        let alsa = setup.alsa.as_mut().unwrap();
        alsa.path = path.map(str::to_string);
        alsa.hw = hw;
    }))
}

#[test]
fn the_plug_line_reads_the_device_name_while_the_card_is_closed() {
    for hw in [HwState::Closed, HwState::NotRead] {
        assert_eq!(
            only(&with_alsa(Some("plughw:1"), hw.clone()), "plug"),
            said(SetupOutcome::Fail, "plughw:1 converts")
        );
        assert_eq!(
            only(&with_alsa(Some("dmix:0"), hw.clone()), "plug"),
            said(SetupOutcome::Fail, "dmix:0 mixes")
        );
        for fine in ["hw:Audio", "front:3", "hdmi:0", "iec958:3"] {
            assert_eq!(
                only(&with_alsa(Some(fine), hw.clone()), "plug"),
                said(SetupOutcome::Pass, fine)
            );
        }
        assert_eq!(
            only(&with_alsa(Some("default"), hw.clone()), "plug"),
            said(SetupOutcome::Unknown, "default not judged")
        );
        assert_eq!(
            only(&with_alsa(None, hw), "plug"),
            said(SetupOutcome::Unknown, "ALSA device not read")
        );
    }
}

#[test]
fn an_open_card_decides_the_plug_line_whatever_the_name() {
    let open = |format: &str, rate, channels| {
        HwState::Open(HwParams {
            format: format.into(),
            rate,
            channels,
        })
    };
    assert_eq!(
        only(
            &with_alsa(Some("hw:Audio"), open("S32_LE", 48_000, 2)),
            "plug"
        ),
        said(SetupOutcome::Pass, "hw:Audio")
    );
    assert_eq!(
        only(
            &with_alsa(Some("plughw:1"), open("S32_LE", 48_000, 2)),
            "plug"
        )
        .0,
        SetupOutcome::Pass
    );
    assert_eq!(
        only(
            &with_alsa(Some("iec958:3"), open("S16_LE", 48_000, 2)),
            "plug"
        ),
        said(SetupOutcome::Fail, "ALSA converts S32LE → S16_LE")
    );
}

#[test]
fn a_sink_that_is_not_alsa_passes_the_plug_line_as_not_applicable() {
    let check = setup_check(&route(|_, setup| setup.alsa = None));
    assert_eq!(
        only(&check, "plug"),
        said(SetupOutcome::Pass, "not an ALSA sink")
    );
}

#[test]
fn the_stereo_line_counts_the_sinks_channels() {
    let six = setup_check(&route(|_, setup| {
        setup.sink_layout = Some(layout(&["FL", "FR", "FC", "LFE", "RL", "RR"]));
    }));
    assert_eq!(
        only(&six, "stereo"),
        said(SetupOutcome::Fail, "the sink has 6 channels")
    );
    let one = setup_check(&route(|_, setup| {
        setup.sink_layout = Some(layout(&["MONO"]))
    }));
    assert_eq!(
        only(&one, "stereo"),
        said(SetupOutcome::Fail, "the sink has 1 channel")
    );
    let idle = setup_check(&route(|_, setup| setup.sink_layout = None));
    assert_eq!(
        only(&idle, "stereo"),
        said(SetupOutcome::Pass, "FL FR (when it plays)")
    );
    let unread = setup_check(&route(|_, setup| {
        setup.sink_layout = None;
        setup.offer.as_mut().unwrap().layout = None;
    }));
    assert_eq!(only(&unread, "stereo").0, SetupOutcome::Unknown);
}

#[test]
fn the_filter_line_is_the_sinks_alone() {
    let filtered = setup_check(&route(|_, setup| setup.sink_filter = Some(true)));
    assert_eq!(
        only(&filtered, "filter"),
        said(SetupOutcome::Fail, "a filter inside the sink")
    );
    let unread = setup_check(&route(|_, setup| setup.sink_filter = None));
    assert_eq!(
        only(&unread, "filter"),
        said(SetupOutcome::Unknown, "not read")
    );
    let stream = setup_check(&route(|_, setup| setup.stream_filter = Some(true)));
    assert_eq!(only(&stream, "filter").0, SetupOutcome::Pass);
}

#[test]
fn the_summary_counts_fails_before_unknowns() {
    let unread = setup_check(&route(|_, setup| {
        setup.dither = None;
        setup.sink_filter = None;
    }));
    assert_eq!(unread.summary, SetupSummary::Unread(2));
    assert_eq!(unread.summary_words(), "PipeWire config not fully read: 2");
    assert_eq!(unread.chip(), "config ?");
    let mixed = setup_check(&route(|_, setup| {
        setup.dither = None;
        setup.sink_filter = Some(true);
        setup.clock.as_mut().unwrap().force_rate = Some(96_000);
    }));
    assert_eq!(mixed.summary, SetupSummary::Changes(2));
    assert_eq!(
        mixed.summary_words(),
        "PipeWire config changes the sound: 2"
    );
    assert_eq!(mixed.chip(), "config 2");
}

#[test]
fn an_older_daemons_route_or_a_route_with_no_sink_is_not_read() {
    let older = setup_check(&clean());
    assert_eq!(older.summary, SetupSummary::OtherVersion);
    assert_eq!(
        older.summary_words(),
        "PipeWire config not read (svolderd's version differs)"
    );
    // No config facts this build reads: an older or a newer svolderd sent the route, so the words
    // name no direction.
    assert!(!older.summary_words().contains("is older"));
    assert_eq!(older.chip(), "config ?");
    assert_eq!(older.sink, "Schiit Gungnir 2 Analog Stereo");
    for route in [clean(), RouteFacts::default()] {
        let check = setup_check(&route);
        assert_eq!(check.lines.len(), 7);
        assert!(
            check
                .lines
                .iter()
                .all(|l| l.outcome == SetupOutcome::Unknown)
        );
    }
    let no_sink = RouteFacts {
        setup: Some(clean_setup()),
        ..RouteFacts::default()
    };
    assert_eq!(setup_check(&no_sink).summary, SetupSummary::NotRead);
}

#[test]
fn the_check_survives_the_wire_with_snake_case_words() {
    let check = setup_check(&route(|_, setup| {
        setup.dither = Some(Dither {
            method: "triangular".into(),
            noise: 0,
        });
        setup.sink_filter = None;
    }));
    let json = serde_json::to_value(&check).unwrap();
    assert_eq!(json["summary"], serde_json::json!({"changes": 1}));
    assert_eq!(json["lines"][2]["key"], "dither");
    assert_eq!(json["lines"][2]["outcome"], "fail");
    assert_eq!(json["lines"][6]["outcome"], "unknown");
    let back: SetupCheck = serde_json::from_value(json).unwrap();
    assert_eq!(back, check);
}

#[test]
fn no_word_says_bit_perfect_and_none_holds_a_stage_word() {
    // The five words core/path-words.js (`STAGE_WORDS`) picks out of the path.
    const STAGE_WORDS: [&str; 5] = [
        "host EQ on",
        "loudness on",
        "crossfeed on",
        "virtual speakers on",
        "EQ pass-through",
    ];
    let checks = [
        setup_check(&route(|_, _| {})),
        setup_check(&route(|route, setup| {
            route.sink.as_mut().unwrap().format = Some("S16LE".into());
            setup.clock.as_mut().unwrap().force_rate = Some(96_000);
            setup.clock.as_mut().unwrap().allowed_rates = vec![96_000];
            setup.dither = Some(Dither {
                method: "triangular".into(),
                noise: 2,
            });
            setup.sink_layout = Some(layout(&["MONO"]));
            setup.sink_filter = Some(true);
            setup.alsa = Some(AlsaFacts {
                path: Some("plughw:1".into()),
                ..AlsaFacts::default()
            });
        })),
        setup_check(&clean()),
    ];
    for check in checks {
        let mut texts = vec![check.summary_words(), check.chip(), check.sink.clone()];
        texts.extend(
            check
                .lines
                .iter()
                .map(|l| format!("{} {}", l.label(), l.words)),
        );
        for text in texts {
            let lower = text.to_lowercase();
            assert!(
                !lower.contains("bit-perfect") && !lower.contains("bit-exact"),
                "{text}"
            );
            assert!(STAGE_WORDS.iter().all(|w| !text.contains(w)), "{text}");
        }
    }
}

#[test]
fn the_checks_hold_for_the_sinks_own_clock_rate() {
    // A clock that is not forced reads as no force at all, never as a rate of 0.
    let check = setup_check(&route(|_, setup| {
        setup.clock = Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: vec![44_100, 48_000],
            force_rate: None,
        });
    }));
    assert_eq!(
        only(&check, "forced_rate"),
        said(SetupOutcome::Pass, "none")
    );
}

// ---- fix 1 ----

#[test]
fn dmix_fails_the_plug_line_even_when_the_card_matches() {
    let hw = HwState::Open(HwParams {
        format: "S32_LE".into(),
        rate: 48_000,
        channels: 2,
    });
    assert_eq!(
        only(&with_alsa(Some("dmix:0"), hw), "plug"),
        said(SetupOutcome::Fail, "dmix:0 mixes")
    );
}

#[test]
fn an_hw_format_the_judge_cannot_read_is_never_a_pass() {
    let open = |format: &str| {
        HwState::Open(HwParams {
            format: format.into(),
            rate: 48_000,
            channels: 2,
        })
    };
    assert_eq!(
        only(&with_alsa(Some("plughw:1"), open("FLOAT_LE")), "plug"),
        said(SetupOutcome::Fail, "plughw:1 converts")
    );
    assert_eq!(
        only(&with_alsa(Some("hw:Audio"), open("FLOAT_LE")), "plug"),
        said(SetupOutcome::Unknown, "ALSA format FLOAT_LE not judged")
    );
    assert_eq!(
        only(&with_alsa(Some("hw:Audio"), open("")), "plug"),
        said(SetupOutcome::Unknown, "ALSA format not read")
    );
}

/// The Bifrost's real `stream0` formats and rates.
fn bifrost_kernel() -> KernelOffer {
    KernelOffer {
        formats: vec!["S16_LE".into(), "S24_3LE".into(), "S32_LE".into()],
        rates: vec![
            44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
        ],
    }
}

#[test]
fn the_laptops_bifrost_passes_all_seven_lines_on_both_profiles() {
    for path in ["iec958:3", "front:3"] {
        for hw in [
            HwState::Open(HwParams {
                format: "S32_LE".into(),
                rate: 44_100,
                channels: 2,
            }),
            HwState::Closed,
        ] {
            let check = setup_check(&route(|route, setup| {
                route.sink.as_mut().unwrap().rate_hz = Some(44_100);
                setup.clock.as_mut().unwrap().allowed_rates = bifrost_kernel().rates;
                let alsa = setup.alsa.as_mut().unwrap();
                alsa.path = Some(path.into());
                alsa.kernel = Some(bifrost_kernel());
                alsa.hw = hw.clone();
            }));
            assert_eq!(check.summary, SetupSummary::Untouched, "{path}");
            assert_eq!(
                only(&check, "rates"),
                said(SetupOutcome::Pass, "44.1-384 kHz all allowed")
            );
        }
    }
}

#[test]
fn a_forced_16_bit_bifrost_names_the_32_bit_format_available() {
    let check = setup_check(&route(|route, setup| {
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
        setup.alsa.as_mut().unwrap().kernel = Some(bifrost_kernel());
    }));
    assert_eq!(
        only(&check, "format"),
        said(SetupOutcome::Fail, "S16LE in use, S32LE available")
    );
    let packed = setup_check(&route(|route, setup| {
        route.sink.as_mut().unwrap().format = Some("S16LE".into());
        let kernel = KernelOffer {
            formats: vec!["S16_LE".into(), "S24_3LE".into()],
            rates: Vec::new(),
        };
        setup.alsa.as_mut().unwrap().kernel = Some(kernel);
    }));
    assert_eq!(
        only(&packed, "format"),
        said(SetupOutcome::Fail, "S16LE in use, S24LE available")
    );
}

#[test]
fn an_empty_list_of_allowed_rates_is_unknown_never_every_rate() {
    let check = setup_check(&route(|_, setup| {
        setup.node_rates.clear();
        setup.clock.as_mut().unwrap().allowed_rates.clear();
    }));
    assert_eq!(
        only(&check, "rates"),
        said(SetupOutcome::Unknown, "allowed rates not read")
    );
    assert_ne!(only(&check, "rates").0, SetupOutcome::Pass);
}

/// Every outcome and summary key, spelled out: JSON consumers read them, so a renamed key must
/// fail here.
#[test]
fn the_config_keys_are_pinned() {
    let outcomes = [
        (SetupOutcome::Pass, "pass"),
        (SetupOutcome::Fail, "fail"),
        (SetupOutcome::Unknown, "unknown"),
    ];
    for (outcome, key) in outcomes {
        assert_eq!(outcome.key(), key);
        assert_eq!(serde_json::to_value(outcome).unwrap(), key);
    }
    let summaries = [
        (SetupSummary::Untouched, "untouched"),
        (SetupSummary::Changes(2), "changes"),
        (SetupSummary::Unread(1), "unread"),
        (SetupSummary::NotRead, "not_read"),
        (SetupSummary::OtherVersion, "other_version"),
    ];
    for (summary, key) in summaries {
        assert_eq!(summary.key(), key);
    }
    assert_eq!(
        SetupSummary::KEYS,
        [
            "untouched",
            "changes",
            "unread",
            "not_read",
            "other_version"
        ]
    );
    assert_eq!(SetupOutcome::KEYS, ["pass", "fail", "unknown"]);
    assert_eq!(
        KEYS,
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
}

/// The limit sentence is one text, here, for every front end.
#[test]
fn the_limit_sentence_is_the_judges() {
    assert!(SETUP_LIMIT.starts_with("svolder judges the PipeWire config"));
    assert!(SETUP_LIMIT.ends_with("it never changes a setting."));
}
