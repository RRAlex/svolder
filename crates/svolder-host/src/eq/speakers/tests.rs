// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A 4-channel float32 WAV of `frames` (one array of four samples per frame) at `rate`.
fn wav(rate: u32, frames: &[[f32; 4]]) -> Vec<u8> {
    let data: Vec<u8> = frames
        .iter()
        .flatten()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 16).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Diracs: LL at 10 samples gain 0.5, LR at 14 gain 0.25, RL at 20 gain 0.125, RR at 30 gain
/// 0.75, in 64 frames.
fn diracs() -> Irs {
    let mut frames = vec![[0.0f32; 4]; 64];
    frames[10][0] = 0.5;
    frames[14][1] = 0.25;
    frames[20][2] = 0.125;
    frames[30][3] = 0.75;
    Irs::from_wav(&wav(48_000, &frames)).unwrap()
}

fn info(room: Room) -> SetInfo {
    SetInfo {
        id: "dummy-d2".into(),
        room,
        label: "Dummy head, D2".into(),
        licence: "CC-BY-4.0".into(),
        credit: Some("Measured by the authors".into()),
        source_url: "https://example.invalid/set".into(),
        rate: 48_000,
        taps: 24_000,
        angles: "left 30.0 deg, right 330.0 deg, 1.5 m".into(),
        loudness_db: -6.9,
        cut: false,
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_four_channel_float_wav_reads_back_its_samples() {
    let irs = diracs();
    assert_eq!(irs.rate_hz, 48_000);
    assert_eq!(irs.taps[0].len(), 64);
    assert_eq!(irs.taps[0][10], 0.5);
    assert_eq!(irs.taps[1][14], 0.25);
    assert_eq!(irs.taps[2][20], 0.125);
    assert_eq!(irs.taps[3][30], 0.75);
    assert_eq!(irs.taps[0][11], 0.0);
}

#[test]
fn a_file_that_is_not_the_stage_wav_is_refused_in_words() {
    let good = wav(48_000, &[[0.0; 4]; 8]);
    assert!(Irs::from_wav(&good).is_ok());
    assert!(Irs::from_wav(b"").unwrap_err().contains("WAV"));
    assert!(Irs::from_wav(b"RIFF....AVI ").unwrap_err().contains("WAV"));
    let mut two = good.clone();
    two[22] = 2;
    assert!(Irs::from_wav(&two).unwrap_err().contains("4 channels"));
    // The store's own reader: a 4-channel WAV in any sample format it reads is a set's file.
    let mut pcm = good.clone();
    pcm[20] = 1;
    assert!(Irs::from_wav(&pcm).is_ok());
    let mut odd = good.clone();
    odd[20] = 2;
    assert!(Irs::from_wav(&odd).unwrap_err().contains("not supported"));
    let mut short = good.clone();
    short.truncate(good.len() - 3);
    assert!(Irs::from_wav(&short).unwrap_err().contains("truncated"));
    assert!(
        Irs::from_wav(&wav(48_000, &[]))
            .unwrap_err()
            .contains("no samples")
    );
    let mut silent_rate = good;
    silent_rate[24..28].copy_from_slice(&0u32.to_le_bytes());
    assert!(Irs::from_wav(&silent_rate).unwrap_err().contains("rate"));
}

#[test]
fn the_oracle_is_each_irs_transfer_function_at_the_tone() {
    let irs = diracs();
    let hz = 1000.0;
    let w = 2.0 * std::f64::consts::PI * hz / 48_000.0;
    let r = speakers_response(&irs, 48_000, hz);
    for (got, gain, delay) in [
        (r.ll, 0.5, 10.0),
        (r.lr, 0.25, 14.0),
        (r.rl, 0.125, 20.0),
        (r.rr, 0.75, 30.0),
    ] {
        assert!(near(got[0], gain * (w * delay).cos()), "{got:?}");
        assert!(near(got[1], -gain * (w * delay).sin()), "{got:?}");
    }
}

#[test]
fn a_tone_on_one_speaker_reaches_both_ears_and_a_mono_one_adds() {
    let irs = diracs();
    let r = speakers_response(&irs, 48_000, 100.0);
    assert_eq!(r.left_only(), (r.ll, r.lr));
    assert_eq!(r.right_only(), (r.rl, r.rr));
    let (left_ear, right_ear) = r.mono();
    assert!(near(left_ear[0], r.ll[0] + r.rl[0]));
    assert!(near(left_ear[1], r.ll[1] + r.rl[1]));
    assert!(near(right_ear[0], r.lr[0] + r.rr[0]));
    assert!(near(right_ear[1], r.lr[1] + r.rr[1]));
}

#[test]
fn a_tone_the_sink_or_the_file_cannot_hold_has_no_response() {
    let irs = diracs();
    // Above the file's own Nyquist (24 kHz) the set holds nothing.
    assert_eq!(speakers_response(&irs, 96_000, 30_000.0).ll, [0.0, 0.0]);
    // Above the sink's Nyquist nothing plays either.
    assert_eq!(speakers_response(&irs, 32_000, 17_000.0).rr, [0.0, 0.0]);
    // The sink's rate does not change the magnitude of a tone below both.
    let at_48 = speakers_response(&irs, 48_000, 1000.0);
    let at_96 = speakers_response(&irs, 96_000, 1000.0);
    assert!(near(
        at_48.ll[0].hypot(at_48.ll[1]),
        at_96.ll[0].hypot(at_96.ll[1])
    ));
}

#[test]
fn the_curve_is_the_four_magnitudes_on_crossfeeds_grid() {
    let curve = SpeakersCurve::of(&diracs(), 48_000);
    assert_eq!(curve.hz.len(), crate::eq::crossfeed::CURVE_POINTS);
    assert_eq!(curve.hz[0], 20.0);
    assert_eq!(curve.hz[60], 20_000.0);
    // A Dirac has the same level at every frequency: 20 log10(gain), to 0.01 dB.
    for (series, gain) in [
        (&curve.ll_db, 0.5f64),
        (&curve.lr_db, 0.25),
        (&curve.rl_db, 0.125),
        (&curve.rr_db, 0.75),
    ] {
        assert_eq!(series.len(), 61);
        let want = (20.0 * gain.log10() * 100.0).round() / 100.0;
        assert!(
            series.iter().all(|db| (db - want).abs() < 0.011),
            "{series:?}"
        );
    }
}

#[test]
fn the_view_of_a_chosen_set_carries_what_the_sidecar_says() {
    let set = info(Room::With);
    let view = SpeakersView::of(
        Some((&set, &SpeakersCurve::of(&diracs(), 48_000))),
        true,
        std::slice::from_ref(&set),
        None,
    );
    assert!(view.on);
    assert_eq!(view.set.as_deref(), Some("dummy-d2"));
    assert_eq!(view.room.as_deref(), Some("with"));
    assert_eq!(view.room_name.as_deref(), Some("With room"));
    assert_eq!(view.term.as_deref(), Some("BRIR"));
    assert_eq!(view.label, "Dummy head, D2");
    assert_eq!(view.licence, "CC-BY-4.0");
    assert_eq!(view.credit.as_deref(), Some("Measured by the authors"));
    assert_eq!(view.source_url, "https://example.invalid/set");
    assert_eq!(view.angles, "left 30.0 deg, right 330.0 deg, 1.5 m");
    assert!(near(view.taps_ms, 500.0));
    assert_eq!(view.loudness_db, -6.9);
    assert_eq!(view.curve.hz.len(), 61);
    assert_eq!(view.sets.len(), 1);
    assert_eq!(view.sets[0].id, "dummy-d2");
    assert_eq!(view.sets[0].term, "BRIR");
}

#[test]
fn a_no_room_set_is_an_hrtf() {
    let set = info(Room::No);
    let view = SpeakersView::of(
        Some((&set, &SpeakersCurve::of(&diracs(), 48_000))),
        false,
        std::slice::from_ref(&set),
        None,
    );
    assert!(!view.on);
    assert_eq!(view.room.as_deref(), Some("no"));
    assert_eq!(view.room_name.as_deref(), Some("No room"));
    assert_eq!(view.term.as_deref(), Some("HRTF"));
}

#[test]
fn the_view_with_no_set_chosen_is_empty_but_lists_the_fetched_sets() {
    let set = info(Room::No);
    let view = SpeakersView::of(None, false, &[set], Some("a note".into()));
    assert!(!view.on);
    assert_eq!(view.set, None);
    assert_eq!(view.room, None);
    assert!(view.curve.hz.is_empty());
    assert_eq!(view.note.as_deref(), Some("a note"));
    assert_eq!(view.sets.len(), 1);
}

#[test]
fn an_old_view_without_speakers_fields_parses_to_the_defaults() {
    let view: SpeakersView = serde_json::from_str("{}").unwrap();
    assert_eq!(view, SpeakersView::default());
    let round: SpeakersView = serde_json::from_str(
        &serde_json::to_string(&SpeakersView::of(
            Some((&info(Room::With), &SpeakersCurve::of(&diracs(), 48_000))),
            true,
            &[],
            None,
        ))
        .unwrap(),
    )
    .unwrap();
    assert!(round.on);
}

#[test]
fn a_sidecar_parses_from_toml_and_ignores_the_fields_it_does_not_use() {
    let text = r#"
id = "dummy-d2"
room = "with"
label = "Dummy head, D2"
licence = "CC-BY-4.0"
source_url = "https://example.invalid/set"
source_sha256 = "00"
wav_sha256 = "11"
rate = 48000
taps = 24000
angles = "left 30.0 deg, right 330.0 deg, 1.5 m"
loudness_db = -6.9
"#;
    let set: SetInfo = toml::from_str(text).unwrap();
    assert_eq!(set.room, Room::With);
    assert_eq!(set.credit, None);
    assert_eq!(set.taps, 24_000);
}

#[test]
fn the_reasons_have_their_words_and_the_missing_note_names_the_set() {
    assert_eq!(OffReason::SetMissing.word(), "set-missing");
    assert_eq!(OffReason::OutputChanged.word(), "output-changed");
    assert_eq!(
        missing_note("Dummy head, D2"),
        "virtual speakers off: Dummy head, D2 is missing or damaged, fetch it again"
    );
}

#[test]
fn the_view_says_when_the_set_was_cut() {
    let mut set = info(Room::With);
    set.cut = true;
    let curve = SpeakersCurve::of(&diracs(), 48_000);
    let view = SpeakersView::of(Some((&set, &curve)), false, &[], None);
    assert!(view.cut);
    assert!(!SpeakersView::of(Some((&info(Room::With), &curve)), false, &[], None).cut);
    // A sidecar without the key is not cut.
    let text = "id=\"a\"\nroom=\"no\"\nlabel=\"l\"\nlicence=\"x\"\nsource_url=\"u\"\nrate=48000\ntaps=1\nangles=\"\"\nloudness_db=0.0\n";
    assert!(!toml::from_str::<SetInfo>(text).unwrap().cut);
}

#[test]
fn the_curve_is_worked_out_once_per_set_and_rate() {
    let mut cache = CurveCache::default();
    let irs = diracs();
    let first = cache.get("dummy-d2", &irs, 48_000).clone();
    assert_eq!(cache.computed(), 1);
    assert_eq!(cache.get("dummy-d2", &irs, 48_000), &first);
    assert_eq!(cache.computed(), 1, "the same set and rate is a hit");
    cache.get("dummy-d2", &irs, 96_000);
    assert_eq!(cache.computed(), 2, "another rate is another entry");
    cache.get("other", &irs, 48_000);
    assert_eq!(cache.computed(), 3);
    cache.forget("dummy-d2");
    cache.get("dummy-d2", &irs, 48_000);
    assert_eq!(cache.computed(), 4, "a forgotten set is worked out again");
}

#[test]
fn a_rate_change_adds_the_resamplers_one_source_sample_of_delay() {
    let irs = diracs();
    let hz = 5000.0;
    let same = speakers_response(&irs, 48_000, hz);
    let up = speakers_response(&irs, 96_000, hz);
    // One sample of the file's rate: a phase of -2 pi hz / 48000, magnitude untouched.
    let extra = -2.0 * std::f64::consts::PI * hz / 48_000.0;
    for (a, b) in [
        (same.ll, up.ll),
        (same.lr, up.lr),
        (same.rl, up.rl),
        (same.rr, up.rr),
    ] {
        let want = [
            a[0] * extra.cos() - a[1] * extra.sin(),
            a[0] * extra.sin() + a[1] * extra.cos(),
        ];
        assert!(near(b[0], want[0]) && near(b[1], want[1]), "{b:?} {want:?}");
    }
    // At the file's own rate nothing is added.
    assert_eq!(same, speakers_response(&irs, 48_000, hz));
}
