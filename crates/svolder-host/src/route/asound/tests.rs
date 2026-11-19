// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `/proc/asound` readers against real files from a USB DAC (card 3), under
//! `tests/fixtures/asound` (see the README in `tests/fixtures/pw-dump`).

use std::ffi::OsStr;
use std::path::PathBuf;

use super::*;
use crate::route::{AlsaFacts, SetupFacts};

const STREAM0: &str = include_str!("../../../tests/fixtures/asound/idle/card3/stream0");
const HW_PLAYING: &str =
    include_str!("../../../tests/fixtures/asound/playing/card3/pcm0p/sub0/hw_params");

fn root(case: &str) -> Asound {
    Asound::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/asound")
            .join(case),
    )
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn facts_of(card: Option<u32>, device: Option<u32>) -> RouteFacts {
    RouteFacts {
        setup: Some(SetupFacts {
            alsa: Some(AlsaFacts {
                path: Some("iec958:3".into()),
                card,
                device,
                ..AlsaFacts::default()
            }),
            ..SetupFacts::default()
        }),
        ..RouteFacts::default()
    }
}

fn alsa(facts: &RouteFacts) -> &AlsaFacts {
    facts
        .setup
        .as_ref()
        .and_then(|setup| setup.alsa.as_ref())
        .expect("the facts carry the ALSA hop")
}

#[test]
fn stream0_gives_the_playback_formats_and_the_eight_rates() {
    let offer = kernel_offer(STREAM0).expect("a playback section");
    assert_eq!(offer.formats, names(&["S16_LE", "S24_3LE", "S32_LE"]));
    assert_eq!(
        offer.rates,
        vec![
            44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000
        ]
    );
}

#[test]
fn a_capture_section_is_not_the_playback_offer() {
    let text = "Card : USB Audio\n\nCapture:\n  Status: Stop\n  Interface 2\n    Altset 1\n    \
                Format: S16_LE\n    Rates: 16000\n\nPlayback:\n  Status: Stop\n  Interface 1\n    \
                Altset 1\n    Format: S32_LE\n    Rates: 48000, 96000\n";
    let offer = kernel_offer(text).expect("a playback section");
    assert_eq!(offer.formats, names(&["S32_LE"]));
    assert_eq!(offer.rates, vec![48_000, 96_000]);
}

#[test]
fn a_stream_file_without_playback_formats_offers_nothing() {
    assert_eq!(kernel_offer(""), None);
    assert_eq!(kernel_offer("Card\n\nCapture:\n  Format: S16_LE\n"), None);
}

#[test]
fn a_continuous_rate_range_is_not_invented_into_a_list() {
    let text = "Playback:\n  Interface 1\n    Altset 1\n    Format: S16_LE\n    \
                Rates: 8000 - 48000 (continuous)\n";
    let offer = kernel_offer(text).expect("a format is listed");
    assert_eq!(offer.formats, names(&["S16_LE"]));
    assert!(offer.rates.is_empty());
}

#[test]
fn hw_params_of_an_open_device_gives_its_format_rate_and_channels() {
    assert_eq!(
        hw_state(HW_PLAYING),
        HwState::Open(HwParams {
            format: "S32_LE".into(),
            rate: 44_100,
            channels: 2
        })
    );
}

#[test]
fn hw_params_that_read_closed_is_closed() {
    assert_eq!(hw_state("closed\n"), HwState::Closed);
}

#[test]
fn hw_params_that_cannot_be_understood_is_not_read() {
    assert_eq!(hw_state(""), HwState::NotRead);
    assert_eq!(hw_state("format: S32_LE\n"), HwState::NotRead);
    assert_eq!(hw_state("something else\n"), HwState::NotRead);
}

#[test]
fn the_card_is_read_while_it_plays() {
    let mut facts = facts_of(Some(3), Some(0));
    fill_alsa(&mut facts, &root("playing"));
    let hop = alsa(&facts);
    assert_eq!(
        hop.hw,
        HwState::Open(HwParams {
            format: "S32_LE".into(),
            rate: 44_100,
            channels: 2
        })
    );
    assert_eq!(hop.kernel.as_ref().map(|k| k.rates.len()), Some(8));
    assert_eq!(hop.path.as_deref(), Some("iec958:3"));
}

#[test]
fn an_idle_card_reads_closed_with_its_offer() {
    let mut facts = facts_of(Some(3), Some(0));
    fill_alsa(&mut facts, &root("idle"));
    let hop = alsa(&facts);
    assert_eq!(hop.hw, HwState::Closed);
    assert_eq!(hop.kernel.as_ref().map(|k| k.formats.len()), Some(3));
}

#[test]
fn a_card_with_no_files_is_not_read() {
    let mut facts = facts_of(Some(7), Some(0));
    fill_alsa(&mut facts, &root("playing"));
    let hop = alsa(&facts);
    assert_eq!(hop.hw, HwState::NotRead);
    assert_eq!(hop.kernel, None);
}

#[test]
fn a_sink_without_a_card_reads_nothing() {
    let mut facts = facts_of(None, Some(0));
    fill_alsa(&mut facts, &root("playing"));
    let hop = alsa(&facts);
    assert_eq!(hop.hw, HwState::NotRead);
    assert_eq!(hop.kernel, None);
}

#[test]
fn a_sink_without_a_device_number_gets_the_offer_but_no_hw_params() {
    let mut facts = facts_of(Some(3), None);
    fill_alsa(&mut facts, &root("playing"));
    let hop = alsa(&facts);
    assert_eq!(hop.hw, HwState::NotRead);
    assert!(hop.kernel.is_some());
}

#[test]
fn another_pcm_device_reads_its_own_stream_file() {
    let mut facts = facts_of(Some(3), Some(1));
    facts
        .setup
        .as_mut()
        .unwrap()
        .alsa
        .as_mut()
        .unwrap()
        .subdevice = Some(1);
    fill_alsa(&mut facts, &root("second"));
    let hop = alsa(&facts);
    assert_eq!(hop.kernel.as_ref().map(|k| k.formats.len()), Some(3));
    assert_eq!(
        hop.hw,
        HwState::Open(HwParams {
            format: "S24_3LE".into(),
            rate: 48_000,
            channels: 2
        })
    );
}

#[test]
fn a_node_that_names_no_substream_reads_the_first() {
    let mut facts = facts_of(Some(3), Some(1));
    fill_alsa(&mut facts, &root("second"));
    assert_eq!(
        alsa(&facts).hw,
        HwState::NotRead,
        "sub0 of pcm1 has no file"
    );
}

#[test]
fn a_node_that_names_the_first_substream_does_not_read_the_second() {
    let mut facts = facts_of(Some(3), Some(1));
    facts
        .setup
        .as_mut()
        .unwrap()
        .alsa
        .as_mut()
        .unwrap()
        .subdevice = Some(0);
    fill_alsa(&mut facts, &root("second"));
    assert_eq!(alsa(&facts).hw, HwState::NotRead, "only sub1 has a file");
}

#[test]
fn device_zero_has_no_stream_file_when_the_card_lists_only_another() {
    let mut facts = facts_of(Some(3), Some(0));
    fill_alsa(&mut facts, &root("second"));
    assert_eq!(alsa(&facts).kernel, None);
}

#[test]
fn a_running_stream_file_reads_like_an_idle_one() {
    let text = "Card : USB Audio\n\nPlayback:\n  Status: Running\n    Interface = 1\n    \
                Altset = 3\n    Momentary freq = 44100 Hz (0x5.8333)\n  Interface 1\n    \
                Altset 3\n    Format: S32_LE\n    Rates: 44100, 48000\n";
    let offer = kernel_offer(text).expect("a format is listed");
    assert_eq!(offer.formats, names(&["S32_LE"]));
    assert_eq!(offer.rates, vec![44_100, 48_000]);
}

#[test]
fn facts_with_no_setup_or_no_alsa_hop_are_left_alone() {
    let mut none = RouteFacts::default();
    fill_alsa(&mut none, &root("playing"));
    assert_eq!(none, RouteFacts::default());
    let mut pipe = RouteFacts {
        setup: Some(SetupFacts::default()),
        ..RouteFacts::default()
    };
    fill_alsa(&mut pipe, &root("playing"));
    assert_eq!(pipe.setup, Some(SetupFacts::default()));
}

#[test]
fn the_root_is_proc_asound_unless_the_environment_names_one() {
    assert_eq!(Asound::resolve(None).root, PathBuf::from("/proc/asound"));
    assert_eq!(
        Asound::resolve(Some(OsStr::new(""))).root,
        PathBuf::from("/proc/asound")
    );
    assert_eq!(
        Asound::resolve(Some(OsStr::new("/tmp/x"))).root,
        PathBuf::from("/tmp/x")
    );
}
