// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::host::HostView;
use crate::socket::protocol::{SceneRow, ScenesView, ValueObject};
use crate::socket::signal::{Signal, SignalSource};
use svolder_host::eq::EqView;
use svolder_host::{PlayerMatch, RouteFacts, SinkFacts, StreamFacts};
use svolder_profiles::{Profile, Registry};

/// The folder of the golden files the extension's tests read.
fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contrib/gnome-shell/tests/fixtures")
}

/// Compare `views`, one JSON object per line, with the golden file `name`; with `SVOLDER_BLESS=1`
/// the file is written instead. A drift fails the test and says how to bless it.
pub(super) fn golden(name: &str, views: &[PanelView]) {
    let text: String = views
        .iter()
        .map(|view| format!("{}\n", serde_json::to_string(view).unwrap()))
        .collect();
    let path = fixtures().join(name);
    if std::env::var("SVOLDER_BLESS").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let held = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; SVOLDER_BLESS=1 writes it", path.display()));
    assert_eq!(
        held, text,
        "{name} drifted from what the model prints; SVOLDER_BLESS=1 rewrites it"
    );
}

/// Every number string a person reads has the proper minus; a negative that rounds to zero has
/// none.
#[test]
fn number_text_uses_the_proper_minus() {
    assert_eq!(number_text(-24.1, 1), "\u{2212}24.1");
    assert_eq!(number_text(-24.1, 0), "\u{2212}24");
    assert_eq!(number_text(0.0, 1), "0.0");
    assert_eq!(number_text(-0.04, 1), "0.0");
    assert_eq!(number_text(12.5, 1), "12.5");
}

const NOW: u64 = 1_000_000_000;

fn bifrost() -> Profile {
    Registry::builtin()
        .unwrap()
        .get("bifrost3")
        .expect("a built-in profile")
        .clone()
}

/// One reading of the Bifrost 3 by function id.
fn reading(id: &str, raw: serde_json::Value) -> ValueObject {
    ValueObject {
        id: id.into(),
        key: None,
        label: String::new(),
        value_type: "u8".into(),
        raw,
        display: String::new(),
        value: None,
        unit: None,
        heard_at_ms: None,
    }
}

/// The readings of a Bifrost 3 on `input` (1 USB, 2 Optical), at `rate` (1 is 44.1 kHz), its
/// volume at `volume` (127 is 0 dB), `mute` and tone `mode` (0 flat, 1 eq).
fn readings(input: u8, rate: u8, volume: u8, mute: bool) -> Vec<ValueObject> {
    vec![
        reading("2", input.into()),
        reading("5", mute.into()),
        reading("6", rate.into()),
        reading("10", volume.into()),
    ]
}

fn info(connected: bool, signal: Option<Signal>) -> PanelLink {
    PanelLink {
        name: "Bifrost 3".into(),
        connected,
        signal,
    }
}

fn advert(dbm: i16, age_secs: u64) -> Signal {
    Signal {
        dbm,
        source: SignalSource::Advert,
        heard_at_ms: NOW - age_secs * 1000,
    }
}

/// A route to the Bifrost 3's sink at `rate_hz`, the player's stream at `player_hz`.
fn route(rate_hz: u32, player_hz: u32) -> RouteFacts {
    RouteFacts {
        sink: Some(SinkFacts {
            name: "alsa_output.usb-Schiit_Audio_Bifrost_3-00.analog-stereo".into(),
            description: "Schiit Bifrost 3 Analog Stereo".into(),
            rate_hz: Some(rate_hz),
            format: Some("S32LE".into()),
            volume_milli: Some(1000),
            muted: false,
            usb_vendor_id: Some(0x30be),
            usb_product: Some("Bifrost 3".into()),
        }),
        player: Some(StreamFacts {
            rate_hz: Some(player_hz),
            format: Some("S16LE".into()),
            volume_milli: Some(1000),
            muted: false,
        }),
        player_match: PlayerMatch::Pid,
        others_mixing: 0,
        host_eq: false,
        stages: None,
        setup: None,
    }
}

fn host(route: Option<RouteFacts>, eq_on: bool) -> HostView {
    HostView {
        available: true,
        route,
        eq: Some(EqView {
            on: eq_on,
            preset: eq_on.then(|| "Late night".to_string()),
            ..Default::default()
        }),
        ..HostView::default()
    }
}

fn scenes(on: Option<&str>, names: &[&str]) -> ScenesView {
    ScenesView {
        scenes: names
            .iter()
            .map(|name| SceneRow {
                name: (*name).into(),
                parts: Vec::new(),
            })
            .collect(),
        on: on.map(str::to_string),
        recalled: None,
        known: true,
    }
}

/// The parts one case varies.
struct Case {
    readings: Vec<ValueObject>,
    info: PanelLink,
    host: Option<HostView>,
    scenes: Option<ScenesView>,
}

impl Case {
    fn view(&self) -> PanelView {
        let profile = bifrost();
        let presets: Vec<svolder_profiles::tone::preset::Preset> = Vec::new();
        PanelView::build(&PanelInput {
            profile: Some(&profile),
            link: Some(&self.info),
            readings: &self.readings,
            host: self.host.as_ref(),
            scenes: self.scenes.as_ref(),
            presets: &presets,
            now_ms: NOW,
        })
    }
}

/// Untouched end to end: the same rate on every hop and the DAC at 0 dB.
fn untouched() -> Case {
    Case {
        readings: readings(1, 1, 127, false),
        info: info(true, Some(advert(-58, 10))),
        host: Some(host(Some(route(44_100, 44_100)), false)),
        scenes: Some(scenes(None, &["Desk"])),
    }
}

fn device(view: &PanelView) -> &PanelDevice {
    view.device.as_ref().expect("a device")
}

fn path(view: &PanelView) -> &PanelPath {
    view.path.as_ref().expect("a path")
}

fn tones(view: &PanelView) -> Vec<PanelTone> {
    path(view).segments.iter().map(|s| s.tone).collect()
}

#[test]
fn untouched_is_green_end_to_end_with_no_words() {
    let view = untouched().view();
    golden("untouched.jsonl", std::slice::from_ref(&view));
    assert_eq!(path(&view).end_to_end, "untouched");
    assert_eq!(tones(&view), [PanelTone::Good; 3]);
    assert_eq!(path(&view).off, "");
    assert_eq!(path(&view).tone, PanelLabelTone::Good);
    assert_eq!(path(&view).label, "Path: untouched end to end");
    let device = device(&view);
    assert!(device.present);
    assert_eq!(device.rate_text.as_deref(), Some("44.1"));
    let volume = device.volume.as_ref().unwrap();
    assert_eq!(volume.text.as_deref(), Some("0.0"));
    assert_eq!(volume.muted, Some(false));
    assert_eq!(device.input.as_ref().unwrap().on.as_deref(), Some("USB"));
    assert_eq!(view.scenes.choices[0].run, ["scene", "load", "--", "Desk"]);
    let line = serde_json::to_string(&view).unwrap();
    assert!(!line.contains("\u{2212}-"), "{line}");
}

#[test]
fn a_lowered_dac_volume_and_tone_are_named_under_the_dac() {
    let mut case = untouched();
    // Raw 67 is -24.08 dB; the tone control's mode (id 12) reads 2, the EQ.
    case.readings = readings(1, 1, 67, false);
    let mode_id = bifrost().eq.unwrap().mode_id().to_string();
    case.readings.push(reading(&mode_id, 2.into()));
    let view = case.view();
    golden("dac_changed.jsonl", std::slice::from_ref(&view));
    assert_eq!(path(&view).end_to_end, "changed");
    assert_eq!(
        tones(&view),
        [PanelTone::Good, PanelTone::Good, PanelTone::Warn]
    );
    let off = &path(&view).off;
    assert_eq!(off, "DAC: volume \u{2212}24.1 dB \u{b7} tone control");
    let volume = device(&view).volume.as_ref().unwrap();
    assert!(volume.text.as_ref().unwrap().starts_with(MINUS));
    assert!((volume.db.unwrap() + 24.082).abs() < 1e-9);
    assert_eq!(volume.min_db, Some(-96.33));
    assert_eq!(path(&view).label, "Path: the DAC changes the sound");
}

/// Setup facts under which the route stays untouched (a wide offer, the clock allowing the
/// sink's rate, stereo on both sides, no filter, an ALSA `hw:` device) with the clock allowing `allowed_rates` of the DAC's 44.1, 48 and 96 kHz.
fn setup_allowing(allowed_rates: &[u32]) -> svolder_host::SetupFacts {
    use svolder_host::{AlsaFacts, ClockFacts, Dither, HwState, KernelOffer, Layout, Rates};
    let stereo = || Layout {
        channels: 2,
        positions: vec!["FL".into(), "FR".into()],
    };
    svolder_host::SetupFacts {
        offer: Some(svolder_host::SinkOffer {
            formats: vec!["S32LE".into(), "S24LE".into(), "S16LE".into()],
            rates: Some(Rates::Range {
                min: 44_100,
                max: 384_000,
            }),
            layout: Some(stereo()),
        }),
        clock: Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: allowed_rates.to_vec(),
            force_rate: None,
        }),
        sink_layout: Some(stereo()),
        stream_layout: Some(stereo()),
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
                rates: vec![44_100, 48_000, 96_000],
            }),
        }),
        ..Default::default()
    }
}

#[test]
fn a_failing_config_check_adds_config_n() {
    let mut case = untouched();
    let mut route = route(44_100, 44_100);
    route.setup = Some(setup_allowing(&[44_100, 48_000]));
    case.host = Some(host(Some(route), false));
    let view = case.view();
    golden("config_fail.jsonl", std::slice::from_ref(&view));
    assert!(path(&view).off.ends_with("config 1"), "{}", path(&view).off);
    assert_eq!(path(&view).label, "Path: the PipeWire config check failed");
    // The judge's label is yellow although the path itself is untouched: the LED follows it.
    assert_eq!(path(&view).end_to_end, "untouched");
    assert_eq!(path(&view).tone, PanelLabelTone::Warn);
    assert_eq!(tones(&view), [PanelTone::Good; 3]);
}

/// A sound that goes to another sink has no hops: the strip is grey and `off` carries the row's
/// words, so the popup says where the sound goes.
#[test]
fn another_sink_is_grey_with_the_rows_words() {
    let mut case = untouched();
    let mut elsewhere = route(48_000, 44_100);
    elsewhere.sink = Some(SinkFacts {
        name: "alsa_output.pci-0000_00_1f.3.hdmi-stereo".into(),
        description: "Monitor HDMI".into(),
        usb_vendor_id: None,
        usb_product: None,
        ..elsewhere.sink.unwrap()
    });
    case.host = Some(host(Some(elsewhere), false));
    let view = case.view();
    golden("not_this_dac.jsonl", std::slice::from_ref(&view));
    assert_eq!(path(&view).end_to_end, "not_this_dac");
    assert_eq!(tones(&view), [PanelTone::Unknown; 3]);
    assert_eq!(path(&view).tone, PanelLabelTone::Plain);
    assert!(
        path(&view).off.contains("Monitor HDMI"),
        "{}",
        path(&view).off
    );
}

/// A DAC listening to an input the computer does not feed: grey, with the words that name it.
#[test]
fn another_input_is_grey_with_the_rows_words() {
    let mut case = untouched();
    case.readings = readings(2, 1, 127, false);
    let view = case.view();
    golden("other_input.jsonl", std::slice::from_ref(&view));
    assert_eq!(path(&view).end_to_end, "other_input");
    assert_eq!(tones(&view), [PanelTone::Unknown; 3]);
    assert!(path(&view).off.contains("Optical"), "{}", path(&view).off);
}

/// A player that could not be identified is a grey segment, not a green one, and the verdict is
/// not claimed as untouched.
#[test]
fn an_unidentified_player_is_a_grey_segment() {
    let mut case = untouched();
    let mut route = route(44_100, 44_100);
    route.player = None;
    case.host = Some(host(Some(route), false));
    let view = case.view();
    golden("player_unknown.jsonl", std::slice::from_ref(&view));
    assert_eq!(tones(&view)[0], PanelTone::Unknown);
    assert_ne!(path(&view).end_to_end, "untouched");
    assert_eq!(path(&view).tone, PanelLabelTone::Plain);
}

/// A saved preset with no curve for the computer is not offered for it.
#[test]
fn a_preset_the_computer_cannot_play_is_not_offered() {
    use svolder_profiles::tone::preset::Preset;
    let flat_without_curve: Preset = toml::from_str(
        "name = \"No curve\"\nbands = 5\nmode = \"flat\"\nloudness = 0\nband = []\n",
    )
    .expect("a preset");
    assert!(svolder_profiles::tone::preset::host_correction(&flat_without_curve).is_err());
    let presets = [flat_without_curve];
    let profile = bifrost();
    let case = untouched();
    let view = PanelView::build(&PanelInput {
        profile: Some(&profile),
        link: Some(&case.info),
        readings: &case.readings,
        host: Some(&host(Some(route(44_100, 44_100)), false)),
        scenes: None,
        presets: &presets,
        now_ms: NOW,
    });
    let names: Vec<&str> = view
        .computer_eq
        .as_ref()
        .unwrap()
        .choices
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert!(!names.contains(&"No curve"), "{names:?}");
}

#[test]
fn a_resampling_computer_is_the_yellow_middle_segment() {
    let mut case = untouched();
    case.readings = readings(1, 2, 127, false);
    case.host = Some(host(Some(route(48_000, 44_100)), false));
    let view = case.view();
    golden("resampled.jsonl", std::slice::from_ref(&view));
    assert_eq!(
        tones(&view),
        [PanelTone::Good, PanelTone::Warn, PanelTone::Good]
    );
    assert!(
        path(&view).off.starts_with("Computer: "),
        "{}",
        path(&view).off
    );
}

#[test]
fn away_keeps_the_device_but_not_the_dac_stage() {
    let mut case = untouched();
    case.readings = readings(1, 1, 103, false);
    case.info = info(false, Some(advert(-58, 10)));
    let view = case.view();
    let present = untouched().view();
    golden("away.jsonl", &[view.clone(), present]);
    assert!(!device(&view).present);
    // Readings held from before the drop are not claimed as what the DAC does now.
    assert_eq!(path(&view).off, "");
}

#[test]
fn muted_is_a_flag_not_a_missing_volume() {
    let mut case = untouched();
    case.readings = readings(1, 1, 127, true);
    let view = case.view();
    golden("muted.jsonl", std::slice::from_ref(&view));
    let volume = device(&view).volume.as_ref().unwrap();
    assert_eq!(volume.muted, Some(true));
    assert!(volume.text.is_some());
    assert!(path(&view).off.contains("muted"), "{}", path(&view).off);
}

#[test]
fn a_weak_signal_is_one_bar_and_an_old_one_is_old() {
    let mut weak = untouched();
    weak.info = info(true, Some(advert(-85, 10)));
    let view = weak.view();
    golden("weak_signal.jsonl", std::slice::from_ref(&view));
    let signal = device(&view).signal.as_ref().unwrap();
    assert_eq!((signal.bars, signal.weak, signal.old), (1, true, false));
    assert_eq!(signal.dbm_text, "\u{2212}85");

    let mut old = untouched();
    old.info = info(true, Some(advert(-58, 180)));
    let view = old.view();
    golden("old_signal.jsonl", std::slice::from_ref(&view));
    let signal = device(&view).signal.as_ref().unwrap();
    assert_eq!((signal.bars, signal.weak, signal.old), (3, false, true));
    assert_eq!(signal.heard_at_ms, NOW - 180_000);
}

#[test]
fn no_computer_controls_is_no_computer_eq_and_no_path() {
    let mut case = untouched();
    case.host = Some(HostView::default());
    let view = case.view();
    golden("no_computer_eq.jsonl", std::slice::from_ref(&view));
    assert!(view.computer_eq.is_none());
    assert!(view.path.is_none());
}

#[test]
fn the_computer_eq_lists_presets_with_the_verb_that_selects_them() {
    let mut case = untouched();
    case.host = Some(host(Some(route(44_100, 44_100)), true));
    let view = case.view();
    let eq = view.computer_eq.as_ref().unwrap();
    assert!(eq.on);
    assert_eq!(eq.preset.as_deref(), Some("Late night"));
    let first = &eq.choices[0];
    assert_eq!(
        first.run,
        ["host", "eq", "preset", "--", first.name.as_str()]
    );
}

#[test]
fn several_scenes_keep_the_files_order_and_mark_the_one_on() {
    let mut case = untouched();
    case.scenes = Some(scenes(Some("Night"), &["Desk", "Night", "Party"]));
    let view = case.view();
    golden("several_scenes.jsonl", std::slice::from_ref(&view));
    let names: Vec<&str> = view
        .scenes
        .choices
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Desk", "Night", "Party"]);
    assert_eq!(view.scenes.on.as_deref(), Some("Night"));
    // A scene that is not settled yet is not marked.
    let mut unsure = scenes(Some("Night"), &["Night"]);
    unsure.known = false;
    case.scenes = Some(unsure);
    assert_eq!(case.view().scenes.on, None);
}

#[test]
fn no_device_is_a_view_with_no_device() {
    let view = PanelView::build(&PanelInput {
        profile: None,
        link: None,
        readings: &[],
        host: None,
        scenes: None,
        presets: &[],
        now_ms: NOW,
    });
    golden("no_device.jsonl", std::slice::from_ref(&view));
    assert!(view.device.is_none() && view.path.is_none() && view.computer_eq.is_none());
    assert!(view.scenes.choices.is_empty());
}

#[test]
fn the_input_control_lists_its_values_by_the_verb_that_sets_them() {
    let view = untouched().view();
    let input = device(&view).input.as_ref().unwrap();
    let names: Vec<&str> = input.choices.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["USB", "Optical", "Coaxial"]);
    assert_eq!(input.choices[1].run, ["set", "input", "--", "Optical"]);
}

/// Only a number's sign becomes the proper minus: a hyphen inside a name stays a hyphen.
#[test]
fn minus_signs_leave_hyphens_in_names_alone() {
    use super::build::minus_signs;
    assert_eq!(
        minus_signs("DAC: volume -24.1 dB"),
        "DAC: volume \u{2212}24.1 dB"
    );
    assert_eq!(
        minus_signs("\u{2192} Monitor HDMI-2"),
        "\u{2192} Monitor HDMI-2"
    );
    assert_eq!(
        minus_signs("Computer: USB-3 \u{b7} volume -3.0 dB"),
        "Computer: USB-3 \u{b7} volume \u{2212}3.0 dB"
    );
    assert_eq!(minus_signs("-5 dB"), "\u{2212}5 dB");
}
