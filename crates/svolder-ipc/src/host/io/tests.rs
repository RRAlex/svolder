// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::io_lines;
use svolder_host::{AlsaFacts, HwParams, HwState, RouteFacts, SetupFacts, StreamFacts};

fn route(player: Option<StreamFacts>, hw: Option<HwState>, card: Option<u32>) -> RouteFacts {
    RouteFacts {
        player,
        setup: hw.map(|hw| SetupFacts {
            alsa: Some(AlsaFacts {
                card,
                hw,
                ..AlsaFacts::default()
            }),
            ..SetupFacts::default()
        }),
        ..RouteFacts::default()
    }
}

fn stream(rate: u32, format: &str, channels: Option<u32>) -> StreamFacts {
    StreamFacts {
        rate_hz: Some(rate),
        format: Some(format.to_string()),
        channels,
        ..StreamFacts::default()
    }
}

fn open(rate: u32, format: &str, channels: u32) -> HwState {
    HwState::Open(HwParams {
        format: format.to_string(),
        rate,
        channels,
    })
}

fn dac(format: &str, channels: u32) -> (String, String) {
    let lines = io_lines(&route(None, Some(open(44_100, format, channels)), Some(2)));
    (lines.to_dac, lines.to_dac_short)
}

#[test]
fn the_player_line_is_the_streams_negotiated_format() {
    let lines = io_lines(&route(Some(stream(44_100, "S16LE", Some(2))), None, None));
    assert_eq!(lines.from_player, "44.1 kHz · S16LE · stereo");
}

#[test]
fn a_stream_with_no_channels_says_the_rest() {
    let lines = io_lines(&route(Some(stream(48_000, "F32LE", None)), None, None));
    assert_eq!(lines.from_player, "48 kHz · F32LE");
}

#[test]
fn no_stream_is_not_read() {
    assert_eq!(io_lines(&RouteFacts::default()).from_player, "not read");
    let empty = StreamFacts::default();
    assert_eq!(
        io_lines(&route(Some(empty), None, None)).from_player,
        "not read"
    );
}

#[test]
fn an_open_card_names_rate_bits_format_channels_and_card() {
    let lines = io_lines(&route(None, Some(open(44_100, "S32_LE", 2)), Some(2)));
    assert_eq!(lines.to_dac, "44.1 kHz · 32-bit (S32_LE) · stereo · hw:2");
    assert_eq!(lines.to_dac_short, "44.1 kHz · 32-bit · stereo");
}

#[test]
fn a_card_with_no_number_leaves_hw_out() {
    let lines = io_lines(&route(None, Some(open(96_000, "S24_3LE", 2)), None));
    assert_eq!(lines.to_dac, "96 kHz · 24-bit (S24_3LE) · stereo");
}

#[test]
fn the_bit_depth_follows_each_format_family() {
    assert_eq!(
        dac("S16_LE", 2).0,
        "44.1 kHz · 16-bit (S16_LE) · stereo · hw:2"
    );
    assert_eq!(dac("S24_LE", 2).1, "44.1 kHz · 24-bit · stereo");
    assert_eq!(dac("S24_3LE", 2).1, "44.1 kHz · 24-bit · stereo");
    assert_eq!(dac("S32_LE", 2).1, "44.1 kHz · 32-bit · stereo");
    assert_eq!(dac("FLOAT_LE", 2).1, "44.1 kHz · 32-bit float · stereo");
    assert_eq!(
        dac("FLOAT_LE", 2).0,
        "44.1 kHz · 32-bit float (FLOAT_LE) · stereo · hw:2"
    );
}

#[test]
fn a_format_the_judge_cannot_read_shows_only_its_name() {
    assert_eq!(dac("DSD_U32_BE", 2).1, "44.1 kHz · DSD_U32_BE · stereo");
    assert_eq!(dac("", 2).1, "44.1 kHz · stereo");
}

#[test]
fn channels_say_mono_stereo_or_a_count() {
    assert_eq!(dac("S32_LE", 1).1, "44.1 kHz · 32-bit · mono");
    assert_eq!(dac("S32_LE", 2).1, "44.1 kHz · 32-bit · stereo");
    assert_eq!(dac("S32_LE", 6).1, "44.1 kHz · 32-bit · 6 ch");
}

#[test]
fn a_closed_card_is_not_open() {
    let lines = io_lines(&route(None, Some(HwState::Closed), Some(2)));
    assert_eq!(lines.to_dac, "not open");
    assert_eq!(lines.to_dac_short, "not open");
}

#[test]
fn a_card_the_route_does_not_say_is_not_read() {
    assert_eq!(
        io_lines(&route(None, Some(HwState::NotRead), Some(2))).to_dac,
        "not read"
    );
    assert_eq!(io_lines(&RouteFacts::default()).to_dac, "not read");
    assert_eq!(io_lines(&RouteFacts::default()).to_dac_short, "");
}
