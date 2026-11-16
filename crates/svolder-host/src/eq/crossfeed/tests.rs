// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const RATES: [u32; 4] = [44_100, 48_000, 96_000, 192_000];

fn settings(model: Model, preset: &str) -> CrossfeedSettings {
    CrossfeedSettings::from_preset(model, preset).unwrap()
}

fn bauer(preset: &str) -> CrossfeedSettings {
    settings(Model::Bauer, preset)
}

/// The response of a section `[b0, b1, b2, a1, a2]` at `f` Hz as (magnitude, phase).
fn at(section: &[f64; 5], f: f64, rate: u32) -> (f64, f64) {
    let z = response(section, 2.0 * std::f64::consts::PI * f / f64::from(rate));
    (z.abs(), z.arg())
}

fn dc(section: &[f64; 5]) -> f64 {
    (section[0] + section[1] + section[2]) / (1.0 + section[3] + section[4])
}

fn nyquist(section: &[f64; 5]) -> f64 {
    (section[0] - section[1] + section[2]) / (1.0 - section[3] + section[4])
}

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn the_models_have_their_words_and_names() {
    let words: Vec<&str> = Model::ALL.iter().map(|m| m.word()).collect();
    assert_eq!(words, ["bauer", "linkwitz", "meier", "mid-side"]);
    let names: Vec<&str> = Model::ALL.iter().map(|m| m.name()).collect();
    assert_eq!(names, ["Bauer", "Linkwitz", "Meier", "Mid/Side bass"]);
    for model in Model::ALL {
        assert_eq!(Model::from_word(model.word()), Some(model));
        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, format!("\"{}\"", model.word()));
    }
    assert_eq!(Model::from_word("nope"), None);
}

#[test]
fn the_default_is_bauer_with_the_bs2b_default_preset() {
    let d = CrossfeedSettings::default();
    assert_eq!(d.model, Model::Bauer);
    assert_eq!(d.preset.as_deref(), Some("bs2b default"));
    assert_eq!(d.crossover_hz, 700.0);
    assert!(near(feed_db(d.amount), 4.5, 1e-9));
    assert_eq!(d.delay_ms, None);
}

#[test]
fn feed_and_amount_round_trip() {
    for db in [1.0, 4.5, 6.0, 9.5, 15.0] {
        assert!(near(feed_db(amount_of_feed_db(db)), db, 1e-12), "{db}");
    }
    assert!(near(amount_of_feed_db(0.0), 0.5, 1e-12));
    for a in [0.05, 0.25, 0.373, 0.5] {
        assert!(near(amount_of_feed_db(feed_db(a)), a, 1e-12), "{a}");
    }
}

#[test]
fn the_bauer_presets_keep_their_feed_levels_exactly() {
    let spec = Model::Bauer.spec();
    let got: Vec<(&str, f64, Option<f64>)> = spec
        .presets
        .iter()
        .map(|p| (p.name, p.crossover_hz, p.feed_db()))
        .collect();
    assert_eq!(
        got,
        [
            ("Jan Meier", 650.0, Some(9.5)),
            ("Chu Moy", 700.0, Some(6.0)),
            ("bs2b default", 700.0, Some(4.5)),
        ]
    );
    assert!(
        Model::Linkwitz
            .spec()
            .presets
            .iter()
            .all(|p| p.feed_db().is_none())
    );
}

#[test]
fn bs2b_presets_give_the_documented_corner_gain_and_cross_share() {
    // (preset, Fc_hi, gain, cross share at DC)
    for (preset, fc_hi, gain, share) in [
        ("bs2b default", 995.0, 0.812, 0.373),
        ("Chu Moy", 975.0, 0.839, 0.334),
        ("Jan Meier", 895.0, 0.882, 0.251),
    ] {
        let s = bauer(preset);
        let maths = bauer_maths(feed_db(s.amount), s.crossover_hz);
        assert!(
            near(maths.fc_hi, fc_hi, 1.0),
            "{preset} Fc_hi {}",
            maths.fc_hi
        );
        assert!(
            near(maths.gain, gain, 0.001),
            "{preset} gain {}",
            maths.gain
        );
        let stage = Stage::of(&s, 48_000).unwrap();
        assert!(
            near(dc(&stage.dir) + dc(&stage.x), 1.0, 1e-6),
            "{preset} DC sum"
        );
        assert!(
            near(dc(&stage.x), share, 0.001),
            "{preset} share {}",
            dc(&stage.x)
        );
        assert!(
            near(dc(&stage.x), s.amount, 1e-6),
            "the share is the amount"
        );
    }
}

#[test]
fn bs2b_difference_equation_is_pipewires_with_a_negated_feedback_term() {
    // bs2b runs O[n] = a0 I[n] + a1 I[n-1] + b1 O[n-1]; the chain's sections run
    // y[n] = b0 x[n] + b1 x[n-1] + b2 x[n-2] - a1 y[n-1] - a2 y[n-2] (the cookbook's: the same
    // convention svolder_profiles::tone::response::coefficients returns). So the section is
    // b0 = a0, b1 = a1, a1 = -b1.
    let s = bauer("bs2b default");
    let rate = 48_000.0;
    let maths = bauer_maths(feed_db(s.amount), s.crossover_hz);
    let x_lo = (-2.0 * std::f64::consts::PI * s.crossover_hz / rate).exp();
    let a0_lo = maths.g_lo * (1.0 - x_lo);
    let stage = Stage::of(&s, 48_000).unwrap();
    assert!(near(stage.x[0], maths.gain * a0_lo, 1e-6));
    assert!(near(stage.x[3], -x_lo, 1e-6), "a1 = -b1");
    assert_eq!(stage.x[1], 0.0);
    assert_eq!(stage.x[2], 0.0);
    assert_eq!(stage.x[4], 0.0);
    // The impulse response of the section equals bs2b's recurrence.
    let (mut previous_in, mut previous_out) = (0.0, 0.0);
    let mut y1 = 0.0;
    for n in 0..64 {
        let input = if n == 0 { 1.0 } else { 0.0 };
        let theirs = maths.gain * a0_lo * input + 0.0 * previous_in + x_lo * previous_out;
        let mine = stage.x[0] * input - stage.x[3] * y1;
        assert!(near(theirs, mine, 1e-6), "sample {n}");
        previous_in = input;
        previous_out = theirs;
        y1 = mine;
    }
}

#[test]
fn the_response_reads_a_cookbook_peak_as_svolder_profiles_writes_it() {
    use svolder_profiles::tone::apo::FilterKind;
    let c = svolder_profiles::tone::response::coefficients(
        FilterKind::Peaking,
        1000.0,
        6.0,
        1.0,
        48_000,
    );
    let (gain, _) = at(&c, 1000.0, 48_000);
    assert!(near(20.0 * gain.log10(), 6.0, 1e-6));
}

#[test]
fn every_model_has_its_dc_and_nyquist_gains() {
    for rate in RATES {
        // Linkwitz: a constant direct path and the amount crossed.
        let s = settings(Model::Linkwitz, "Default");
        let st = Stage::of(&s, rate).unwrap();
        assert!(near(dc(&st.dir), 1.0 - s.amount, 1e-6));
        assert!(near(nyquist(&st.dir), 1.0 - s.amount, 1e-6));
        assert!(near(dc(&st.x), s.amount, 1e-6));
        assert!(nyquist(&st.x).abs() < 0.02, "{rate}");
        assert_eq!(st.sum, [1.0, 0.0], "no side path");
        // Meier: the centre untouched, the side crossed.
        let s = settings(Model::Meier, "Default");
        let st = Stage::of(&s, rate).unwrap();
        assert_eq!(st.dir, [1.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(near(dc(&st.x), s.amount, 1e-6));
        assert!(near(dc(&st.s), -s.amount, 1e-6));
        assert_eq!(st.sum, [1.0, 1.0]);
        // Mid/Side bass: a second-order low pass, nothing at Nyquist.
        let s = settings(Model::MidSide, "Default");
        let st = Stage::of(&s, rate).unwrap();
        // The f32 coefficients of a 120 Hz second-order section at 192 kHz hold DC to 2e-4.
        assert!(near(dc(&st.x), s.amount, 2e-4), "{rate}");
        assert!(near(dc(&st.s), -s.amount, 2e-4), "{rate}");
        assert!(nyquist(&st.x).abs() < 1e-6);
        assert_eq!(st.delay_s, 0.0);
        // Bauer: direct + cross is 1 at DC, and the highs are boosted by the corner's shape.
        let s = bauer("bs2b default");
        let st = Stage::of(&s, rate).unwrap();
        assert!(near(dc(&st.dir) + dc(&st.x), 1.0, 1e-6));
        let m = bauer_maths(feed_db(s.amount), s.crossover_hz);
        let x_hi = (-2.0 * std::f64::consts::PI * m.fc_hi / f64::from(rate)).exp();
        let want = m.gain * (1.0 + x_hi - m.g_hi * (1.0 - x_hi)) / (1.0 + x_hi);
        assert!(near(nyquist(&st.dir), want, 1e-5), "{rate}");
    }
}

#[test]
fn the_models_own_lag_is_about_a_quarter_of_a_millisecond_for_bauer() {
    for rate in [44_100, 48_000] {
        let lag = own_lag_ms(&bauer("bs2b default"), rate);
        assert!((0.25..=0.27).contains(&lag), "{rate}: {lag}");
    }
    // The lag follows the crossover: a lower corner is later.
    let low = CrossfeedSettings {
        crossover_hz: 400.0,
        ..bauer("bs2b default")
    };
    assert!(own_lag_ms(&low, 48_000) > own_lag_ms(&bauer("bs2b default"), 48_000));
    for rate in RATES {
        for model in Model::ALL {
            let lag = own_lag_ms(&CrossfeedSettings::for_model(model), rate);
            assert!(lag.is_finite() && lag > 0.0, "{model:?} {rate}: {lag}");
        }
    }
}

#[test]
fn the_worst_gain_of_each_model_is_what_the_plan_expects() {
    for rate in RATES {
        for preset in ["bs2b default", "Chu Moy", "Jan Meier"] {
            let g = worst_gain_db(&bauer(preset), rate);
            assert!((0.0..=0.01).contains(&g), "Bauer {preset} {rate}: {g}");
        }
        let g = worst_gain_db(&settings(Model::Linkwitz, "Default"), rate);
        assert!((0.0..=0.01).contains(&g), "Linkwitz {rate}: {g}");
        let g = worst_gain_db(&settings(Model::MidSide, "Default"), rate);
        assert!(near(g, 3.0, 0.1), "M/S 50 % {rate}: {g}");
        // Meier 25 % / 650 Hz depends on the delay added to the side path: about 2.0 dB for a
        // total lag of 0.5 ms, under 1 dB at the model's own lag, never above 3.0 dB.
        let meier = |delay_ms: Option<f64>| CrossfeedSettings {
            amount: 0.25,
            crossover_hz: 650.0,
            delay_ms,
            ..settings(Model::Meier, "Default")
        };
        let g = worst_gain_db(&meier(Some(0.5)), rate);
        assert!(near(g, 2.0, 0.2), "Meier {rate}: {g}");
        let own = worst_gain_db(&meier(None), rate);
        assert!((0.3..1.0).contains(&own), "Meier own lag {rate}: {own}");
        let g = worst_gain_db(&meier(Some(1.0)), rate);
        assert!(g < 3.0, "Meier 1 ms {rate}: {g}");
    }
}

#[test]
fn the_played_delay_is_the_asked_total_above_the_models_own_lag() {
    let mut s = bauer("bs2b default");
    let own = own_lag_ms(&s, 48_000);
    // None, and anything under the own lag, add nothing.
    assert_eq!(Stage::of(&s, 48_000).unwrap().delay_s, 0.0);
    s.delay_ms = Some(0.1);
    assert_eq!(Stage::of(&s, 48_000).unwrap().delay_s, 0.0);
    s.delay_ms = Some(1.0);
    let added = Stage::of(&s, 48_000).unwrap().delay_s;
    // Whole samples, truncated: within one sample (20.8 µs) under what was asked.
    let played = added * 1000.0;
    assert!(played < 1.0 - own + 0.5 * 1000.0 / 48_000.0 && played > 1.0 - own - 0.0209);
    // Mid/Side bass has no delay whatever is asked.
    let mut m = settings(Model::MidSide, "Default");
    m.delay_ms = Some(0.8);
    assert_eq!(Stage::of(&m, 48_000).unwrap().delay_s, 0.0);
}

#[test]
fn held_keeps_every_value_in_its_models_range() {
    let wild = CrossfeedSettings {
        model: Model::Bauer,
        preset: Some("Chu Moy".into()),
        edited_from: None,
        amount: 0.9,
        crossover_hz: 5.0,
        delay_ms: Some(7.0),
    };
    let h = held(&wild);
    assert!(near(h.amount, 0.47, 1e-12));
    assert_eq!(h.crossover_hz, 300.0);
    assert_eq!(h.delay_ms, Some(1.0));
    assert_eq!(h.preset.as_deref(), Some("Chu Moy"));
    let ms = held(&CrossfeedSettings {
        model: Model::MidSide,
        preset: Some("no such preset".into()),
        edited_from: None,
        amount: 0.0,
        crossover_hz: 9000.0,
        delay_ms: Some(0.5),
    });
    assert!(near(ms.amount, 0.05, 1e-12));
    assert_eq!(ms.crossover_hz, 500.0);
    assert_eq!(ms.delay_ms, None, "Mid/Side bass has no delay");
    assert_eq!(ms.preset, None, "an unknown preset is custom");
    let nan = held(&CrossfeedSettings {
        amount: f64::NAN,
        crossover_hz: f64::NAN,
        delay_ms: Some(f64::NAN),
        ..CrossfeedSettings::default()
    });
    assert!(nan.amount.is_finite() && nan.crossover_hz.is_finite());
    assert_eq!(nan.delay_ms, None);
    let low = held(&CrossfeedSettings {
        delay_ms: Some(-1.0),
        ..CrossfeedSettings::default()
    });
    assert_eq!(low.delay_ms, Some(0.0));
    // A value in range is as written.
    let ok = bauer("Jan Meier");
    assert_eq!(held(&ok), ok);
}

#[test]
fn the_table_has_the_planned_ranges() {
    let b = Model::Bauer.spec();
    assert_eq!(
        (b.crossover_hz.min, b.crossover_hz.max, b.crossover_hz.step),
        (300.0, 2000.0, 10.0)
    );
    assert_eq!(
        (b.amount_pct.min, b.amount_pct.max, b.amount_pct.step),
        (15.0, 47.0, 1.0)
    );
    assert_eq!((b.delay_max_ms, b.delay_step_ms), (1.0, 0.01));
    assert!(b.delay_settable);
    let m = Model::MidSide.spec();
    assert_eq!((m.crossover_hz.min, m.crossover_hz.max), (40.0, 500.0));
    assert_eq!((m.amount_pct.min, m.amount_pct.max), (5.0, 50.0));
    assert!(!m.delay_settable);
    for model in [Model::Meier, Model::Linkwitz] {
        let s = model.spec();
        assert_eq!((s.crossover_hz.min, s.crossover_hz.max), (300.0, 2000.0));
        assert_eq!((s.amount_pct.min, s.amount_pct.max), (5.0, 50.0));
        assert!(s.delay_settable);
    }
    // Every preset sits inside its own model's ranges.
    for model in Model::ALL {
        let spec = model.spec();
        for preset in spec.presets {
            let s = CrossfeedSettings::from_preset(model, preset.name).unwrap();
            assert_eq!(held(&s), s, "{model:?} {}", preset.name);
        }
    }
}

#[test]
fn a_model_switch_loads_that_models_default_preset() {
    for model in Model::ALL {
        let s = CrossfeedSettings::for_model(model);
        assert_eq!(s.model, model);
        assert_eq!(
            s.preset.as_deref(),
            Some(model.spec().default_preset().name)
        );
        assert_eq!(
            model.spec().presets[model.spec().default_preset].name,
            s.preset.as_deref().unwrap()
        );
    }
    assert_eq!(
        CrossfeedSettings::for_model(Model::Bauer).preset.as_deref(),
        Some("bs2b default")
    );
    assert_eq!(CrossfeedSettings::from_preset(Model::Bauer, "Nope"), None);
    // The presets run from the mildest (the most dB below the direct sound) to the strongest.
    let feeds: Vec<f64> = Model::Bauer
        .spec()
        .presets
        .iter()
        .filter_map(|p| p.feed_db())
        .collect();
    assert!(feeds.windows(2).all(|w| w[0] > w[1]), "{feeds:?}");
}

#[test]
fn bypass_is_the_bit_exact_identity() {
    let b = Stage::bypass();
    assert_eq!(b.dir, [1.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(b.x, [0.0; 5]);
    assert_eq!(b.s, [0.0; 5]);
    assert_eq!(b.mix, 0.0);
    assert_eq!(b.delay_s, 0.0);
    // Run: y = x on every sample.
    for sample in [0.0f32, 1.0, -0.37, 0.123_456_79, f32::MIN_POSITIVE] {
        let direct = b.dir[0] as f32 * sample;
        let crossed = b.x[0] as f32 * sample;
        assert_eq!(direct + b.mix as f32 * crossed, sample);
    }
    assert_eq!(held_check(&b), Ok(()));
}

fn held_check(stage: &Stage) -> Result<(), String> {
    stage.check()
}

#[test]
fn a_coefficient_past_ten_or_an_unstable_section_is_refused_with_words() {
    let mut big = Stage::bypass();
    big.dir[0] = 11.0;
    let e = big.check().unwrap_err();
    assert!(e.contains("10"), "{e}");
    let mut unstable = Stage::bypass();
    unstable.x = [0.5, 0.0, 0.0, -1.5, 0.0];
    let e = unstable.check().unwrap_err();
    assert!(e.contains("stable"), "{e}");
    // Every table preset at every rate passes.
    for rate in RATES {
        for model in Model::ALL {
            for preset in model.spec().presets {
                let s = CrossfeedSettings::from_preset(model, preset.name).unwrap();
                assert!(
                    Stage::of(&s, rate).is_ok(),
                    "{model:?} {} {rate}",
                    preset.name
                );
            }
        }
    }
    // And the corners of every range.
    for rate in RATES {
        for model in Model::ALL {
            let spec = model.spec();
            for hz in [spec.crossover_hz.min, spec.crossover_hz.max] {
                for pct in [spec.amount_pct.min, spec.amount_pct.max] {
                    let s = CrossfeedSettings {
                        model,
                        preset: None,
                        edited_from: None,
                        amount: pct / 100.0,
                        crossover_hz: hz,
                        delay_ms: None,
                    };
                    assert!(Stage::of(&s, rate).is_ok(), "{model:?} {hz} {pct} {rate}");
                }
            }
        }
    }
}

#[test]
fn settings_round_trip_through_toml_and_json() {
    let s = CrossfeedSettings {
        model: Model::MidSide,
        preset: None,
        edited_from: None,
        amount: 0.35,
        crossover_hz: 150.0,
        delay_ms: None,
    };
    let text = toml::to_string_pretty(&s).unwrap();
    assert_eq!(toml::from_str::<CrossfeedSettings>(&text).unwrap(), s);
    let custom_delay = CrossfeedSettings {
        delay_ms: Some(0.5),
        ..CrossfeedSettings::default()
    };
    let json = serde_json::to_string(&custom_delay).unwrap();
    assert_eq!(
        serde_json::from_str::<CrossfeedSettings>(&json).unwrap(),
        custom_delay
    );
    // An empty table is the default settings, but without a preset name (custom).
    let empty: CrossfeedSettings = toml::from_str("").unwrap();
    assert_eq!(empty.model, Model::Bauer);
    assert_eq!(empty.preset, None);
}

#[test]
fn the_view_lists_the_models_and_shows_the_totals() {
    let s = bauer("bs2b default");
    let view = CrossfeedView::of(&s, true, 48_000, Some("Speakers".into()), None);
    assert!(view.on);
    assert_eq!(view.model, "bauer");
    assert_eq!(view.preset.as_deref(), Some("bs2b default"));
    assert!(near(view.amount_pct, 37.3, 0.1));
    assert!(
        near(view.amount_db, -8.6, 0.1),
        "the crossed level: {}",
        view.amount_db
    );
    assert!(near(view.feed_db, 4.5, 1e-9));
    assert_eq!(view.crossover_hz, 700.0);
    assert!(
        near(view.delay_ms, view.delay_floor_ms, 1e-12),
        "no delay asked: the floor"
    );
    assert!((0.25..=0.27).contains(&view.delay_floor_ms));
    assert!(view.delay_settable);
    assert_eq!(view.output.as_deref(), Some("Speakers"));
    assert_eq!(view.models.len(), 4);
    assert_eq!(view.models[0].word, "bauer");
    assert_eq!(view.models[0].presets.len(), 3);
    assert_eq!(view.models[0].presets[1].name, "Chu Moy");
    assert_eq!(view.models[0].presets[1].feed_db, Some(6.0));
    assert!(near(view.models[0].presets[1].amount_pct, 33.4, 0.1));
    assert_eq!(view.models[0].default_preset, "bs2b default");
    assert!(!view.models[3].delay_settable);
    // An asked delay above the floor is shown as asked, below it as the floor.
    let mut asked = s.clone();
    asked.delay_ms = Some(0.6);
    assert!(near(
        CrossfeedView::of(&asked, true, 48_000, None, None).delay_ms,
        0.6,
        1e-12
    ));
    asked.delay_ms = Some(0.1);
    let low = CrossfeedView::of(&asked, true, 48_000, None, None);
    assert!(near(low.delay_ms, low.delay_floor_ms, 1e-12));
    // Mid/Side bass has none.
    let ms = CrossfeedView::of(
        &settings(Model::MidSide, "Default"),
        false,
        48_000,
        None,
        None,
    );
    assert_eq!((ms.delay_ms, ms.delay_floor_ms), (0.0, 0.0));
    assert!(!ms.delay_settable && !ms.on);
}

#[test]
fn the_view_is_json_that_older_readers_default() {
    let view = CrossfeedView::of(&bauer("Chu Moy"), true, 44_100, None, Some("note".into()));
    let json = serde_json::to_string(&view).unwrap();
    assert_eq!(serde_json::from_str::<CrossfeedView>(&json).unwrap(), view);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    for key in [
        "on",
        "model",
        "preset",
        "amount_pct",
        "amount_db",
        "feed_db",
        "crossover_hz",
        "delay_ms",
        "delay_floor_ms",
        "delay_settable",
        "output",
        "note",
        "models",
    ] {
        assert!(v.get(key).is_some(), "{key}");
    }
    assert_eq!(CrossfeedView::default().models.len(), 0);
}

// ---- the contract fixes: curve, defaults, description, edited_from, own delay, change rules.

/// The impulse response of bs2b's own recurrence O[n] = a0 I[n] + a1 I[n-1] + b1 O[n-1], then its
/// magnitude at `hz`: a computation that shares nothing with the stage's sections.
fn bs2b_magnitude(a0: f64, a1: f64, b1: f64, rate: f64, hz: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    let (mut previous_in, mut previous_out) = (0.0, 0.0);
    for n in 0..8192 {
        let input = if n == 0 { 1.0 } else { 0.0 };
        let out = a0 * input + a1 * previous_in + b1 * previous_out;
        let angle = 2.0 * std::f64::consts::PI * hz * f64::from(n) / rate;
        re += out * angle.cos();
        im -= out * angle.sin();
        previous_in = input;
        previous_out = out;
    }
    re.hypot(im)
}

fn db(x: f64) -> f64 {
    20.0 * x.log10()
}

#[test]
fn the_curve_of_bauer_matches_bs2bs_own_recurrence() {
    for rate in [44_100u32, 48_000, 96_000] {
        let s = bauer("bs2b default");
        let view = CrossfeedView::of(&s, true, rate, None, None);
        assert_eq!(view.curve.hz.len(), CURVE_POINTS);
        assert_eq!(view.curve.same_db.len(), CURVE_POINTS);
        assert_eq!(view.curve.other_db.len(), CURVE_POINTS);
        assert_eq!(view.curve.hz[0], 20.0);
        assert!(near(*view.curve.hz.last().unwrap(), 20_000.0, 1.0));
        let r = f64::from(rate);
        let m = bauer_maths(4.5, 700.0);
        let x_lo = (-2.0 * std::f64::consts::PI * 700.0 / r).exp();
        let x_hi = (-2.0 * std::f64::consts::PI * m.fc_hi / r).exp();
        for (i, hz) in view.curve.hz.iter().enumerate().step_by(6) {
            let same = m.gain * bs2b_magnitude(1.0 - m.g_hi * (1.0 - x_hi), -x_hi, x_hi, r, *hz);
            let other = m.gain * bs2b_magnitude(m.g_lo * (1.0 - x_lo), 0.0, x_lo, r, *hz);
            assert!(
                near(view.curve.same_db[i], db(same), 0.02),
                "{rate} same {hz}"
            );
            assert!(
                near(view.curve.other_db[i], db(other), 0.02),
                "{rate} other {hz}"
            );
        }
    }
}

#[test]
fn the_curve_of_linkwitz_and_the_bass_models_follow_their_formulas() {
    let rate = 48_000.0;
    let w = |hz: f64| 2.0 * std::f64::consts::PI * hz / rate;
    // Linkwitz: the same ear is flat at 1 - A; the other is A over a one-pole low pass.
    let s = settings(Model::Linkwitz, "Default");
    let v = CrossfeedView::of(&s, true, 48_000, None, None);
    let p = (-2.0 * std::f64::consts::PI * s.crossover_hz / rate).exp();
    for (i, hz) in v.curve.hz.iter().enumerate() {
        assert!(near(v.curve.same_db[i], db(1.0 - s.amount), 0.011), "{hz}");
        let mag = s.amount * (1.0 - p) / (1.0 + p * p - 2.0 * p * w(*hz).cos()).sqrt();
        assert!(near(v.curve.other_db[i], db(mag), 0.011), "{hz}");
    }
    // Mid/Side bass at the crossover: -3 dB of the amount (Butterworth), and the same ear dips by
    // (1 - A·LP): |1 - 0.5·(0.707 e^-j45deg)| from the analogue prototype.
    let ms = settings(Model::MidSide, "Default");
    let v = CrossfeedView::of(&ms, true, 48_000, None, None);
    for (i, hz) in v.curve.hz.iter().enumerate().filter(|(_, hz)| **hz < 100.0) {
        let x = hz / ms.crossover_hz;
        let (re, im) = (1.0 - x * x, 2f64.sqrt() * x);
        let lp = (1.0 / (re * re + im * im).sqrt(), -im.atan2(re));
        let other = ms.amount * lp.0;
        let same = ((1.0 - ms.amount * lp.0 * lp.1.cos()).powi(2)
            + (ms.amount * lp.0 * lp.1.sin()).powi(2))
        .sqrt();
        assert!(near(v.curve.other_db[i], db(other), 0.05), "{hz}");
        assert!(near(v.curve.same_db[i], db(same), 0.05), "{hz}");
    }
    // Mono bass: at 20 Hz the other ear is half; the same ear is half plus the phase lag's
    // remainder (the loop above checks both against the analogue prototype).
    assert!(near(v.curve.other_db[0], -6.0, 0.1));
    assert!(
        (-6.0..-5.2).contains(&v.curve.same_db[0]),
        "{}",
        v.curve.same_db[0]
    );
    // The curve follows the delay: Meier's same ear moves when 0.8 ms is asked.
    let mut meier = settings(Model::Meier, "Default");
    let own = CrossfeedView::of(&meier, true, 48_000, None, None).curve;
    meier.delay_ms = Some(0.8);
    let late = CrossfeedView::of(&meier, true, 48_000, None, None).curve;
    assert_ne!(own.same_db, late.same_db);
    assert_eq!(own.other_db, late.other_db);
}

#[test]
fn an_older_daemons_view_parses_whatever_it_lacks() {
    let old = r#"{"on":true,"model":"bauer","amount_pct":37.3,"models":[{"word":"bauer","name":"Bauer",
        "presets":[{"name":"bs2b default"}],"crossover_hz":{"min":300.0}}]}"#;
    let view: CrossfeedView = serde_json::from_str(old).unwrap();
    assert!(view.on && view.model == "bauer");
    assert_eq!(view.curve, CurveView::default());
    assert_eq!(view.edited_from, None);
    assert!(!view.delay_own);
    assert_eq!(view.models[0].description, "");
    assert_eq!(view.models[0].presets[0].crossover_hz, 0.0);
    assert_eq!(view.models[0].crossover_hz.min, 300.0);
    assert_eq!(view.models[0].crossover_hz.max, 0.0);
    // A view with fields this build does not know still parses (serde ignores them).
    let newer = r#"{"on":false,"something_new":1}"#;
    assert!(serde_json::from_str::<CrossfeedView>(newer).is_ok());
}

#[test]
fn every_model_has_a_description_and_the_view_carries_it() {
    let view = CrossfeedView::of(&CrossfeedSettings::default(), false, 48_000, None, None);
    for (model, listed) in Model::ALL.iter().zip(&view.models) {
        assert!(listed.description.len() > 30, "{model:?}");
        assert_eq!(listed.description, model.spec().description);
    }
}

#[test]
fn custom_values_remember_the_preset_they_came_from() {
    let s = bauer("Chu Moy").edited();
    assert_eq!(
        (s.preset.as_deref(), s.edited_from.as_deref()),
        (None, Some("Chu Moy"))
    );
    assert_eq!(
        s.clone().edited(),
        s,
        "editing Custom again keeps its origin"
    );
    assert_eq!(held(&s), s);
    let view = CrossfeedView::of(&s, true, 48_000, None, None);
    assert_eq!(view.preset, None);
    assert_eq!(view.edited_from.as_deref(), Some("Chu Moy"));
    // A name the model does not have is dropped, a preset's own values have no origin.
    let stray = CrossfeedSettings {
        edited_from: Some("nope".into()),
        preset: None,
        ..CrossfeedSettings::default()
    };
    assert_eq!(held(&stray).edited_from, None);
    let both = CrossfeedSettings {
        edited_from: Some("Chu Moy".into()),
        ..CrossfeedSettings::default()
    };
    assert_eq!(held(&both).edited_from, None);
    assert_eq!(CUSTOM_PRESET, "Custom");
    // It round-trips through TOML and an old state has none.
    let text = toml::to_string_pretty(&s).unwrap();
    assert_eq!(toml::from_str::<CrossfeedSettings>(&text).unwrap(), s);
    assert!(
        !toml::to_string_pretty(&CrossfeedSettings::default())
            .unwrap()
            .contains("edited_from")
    );
}

fn ask(f: impl FnOnce(&mut CrossfeedChange)) -> CrossfeedChange {
    let mut change = CrossfeedChange::default();
    f(&mut change);
    change
}

#[test]
fn a_change_follows_the_documented_rules() {
    let base = CrossfeedSettings::default();
    let rate = 48_000;
    // Nothing asked: nothing changes.
    assert_eq!(
        base.change(&CrossfeedChange::default(), rate).unwrap(),
        base
    );
    // A model switch loads its default preset; the same model changes nothing, Custom included.
    let ms = base
        .change(&ask(|c| c.model = Some("mid-side".into())), rate)
        .unwrap();
    assert_eq!(ms, CrossfeedSettings::for_model(Model::MidSide));
    let custom = ms
        .change(&ask(|c| c.amount_pct = Some(30.0)), rate)
        .unwrap();
    assert_eq!(custom.preset, None);
    assert_eq!(custom.edited_from.as_deref(), Some("Default"));
    assert!(near(custom.amount, 0.30, 1e-12));
    let same = custom
        .change(&ask(|c| c.model = Some("mid-side".into())), rate)
        .unwrap();
    assert_eq!(same, custom, "a click on the selected model keeps Custom");
    // A preset loads its exact numbers; with a model, in the new model.
    let jm = base
        .change(&ask(|c| c.preset = Some("Jan Meier".into())), rate)
        .unwrap();
    assert!(near(feed_db(jm.amount), 9.5, 1e-9));
    let both = base
        .change(
            &ask(|c| {
                c.model = Some("meier".into());
                c.preset = Some("Default".into());
            }),
            rate,
        )
        .unwrap();
    assert_eq!(both, CrossfeedSettings::for_model(Model::Meier));
    // Values are rounded to the step and held to the range.
    let v = base
        .change(
            &ask(|c| {
                c.amount_pct = Some(38.34);
                c.crossover_hz = Some(724.0);
            }),
            rate,
        )
        .unwrap();
    assert!(near(v.amount, 0.38, 1e-12));
    assert_eq!(v.crossover_hz, 720.0);
    let wild = base
        .change(&ask(|c| c.amount_pct = Some(99.0)), rate)
        .unwrap();
    assert!(near(wild.amount, 0.47, 1e-12));
    // A preset and a value together: the value wins and the values are Custom.
    let mixed = base
        .change(
            &ask(|c| {
                c.preset = Some("Chu Moy".into());
                c.crossover_hz = Some(900.0);
            }),
            rate,
        )
        .unwrap();
    assert_eq!(
        (mixed.preset.clone(), mixed.edited_from.as_deref()),
        (None, Some("Chu Moy"))
    );
    assert_eq!(mixed.crossover_hz, 900.0);
    assert!(near(feed_db(mixed.amount), 6.0, 1e-9));
}

#[test]
fn a_change_that_cannot_be_made_is_refused_in_words() {
    let base = CrossfeedSettings::default();
    for (change, words) in [
        (
            ask(|c| c.model = Some("tube".into())),
            "no crossfeed model \"tube\"",
        ),
        (
            ask(|c| c.preset = Some("Nope".into())),
            "Bauer has no preset \"Nope\"",
        ),
        (
            ask(|c| c.amount_pct = Some(f64::NAN)),
            "amount is not a number",
        ),
        (
            ask(|c| c.delay_ms = Some(f64::INFINITY)),
            "delay is not a number",
        ),
        (
            ask(|c| {
                c.model = Some("mid-side".into());
                c.delay_ms = Some(0.5);
            }),
            "Mid/Side bass plays no delay",
        ),
    ] {
        let e = base.change(&change, 48_000).unwrap_err();
        assert!(e.contains(words), "{e}");
    }
    // The preset of another model is not found in this one.
    assert!(
        base.change(&ask(|c| c.preset = Some("Default".into())), 48_000)
            .is_err()
    );
}

#[test]
fn a_delay_at_or_below_the_floor_asks_for_the_models_own() {
    let base = CrossfeedSettings::default();
    let own = own_lag_ms(&base, 48_000);
    let late = base
        .change(&ask(|c| c.delay_ms = Some(0.6)), 48_000)
        .unwrap();
    assert_eq!(late.delay_ms, Some(0.6));
    assert!(!CrossfeedView::of(&late, true, 48_000, None, None).delay_own);
    for ms in [0.0, own - 0.01, own] {
        let back = late
            .change(&ask(|c| c.delay_ms = Some(ms)), 48_000)
            .unwrap();
        assert_eq!(back.delay_ms, None, "{ms}");
        let view = CrossfeedView::of(&back, true, 48_000, None, None);
        assert!(view.delay_own && near(view.delay_ms, view.delay_floor_ms, 1e-12));
    }
    // A delay rounds to 0.01 ms; a crossover that lifts the own lag past it makes it "own".
    let d = base
        .change(&ask(|c| c.delay_ms = Some(0.5049)), 48_000)
        .unwrap();
    assert_eq!(d.delay_ms, Some(0.5));
    let low = d
        .change(&ask(|c| c.crossover_hz = Some(300.0)), 48_000)
        .unwrap();
    assert_eq!(low.delay_ms, None, "the own lag at 300 Hz is above 0.5 ms");
    // Mid/Side bass: nothing to ask for, so it reads as own.
    let ms = CrossfeedView::of(
        &settings(Model::MidSide, "Default"),
        true,
        48_000,
        None,
        None,
    );
    assert!(ms.delay_own);
}

#[test]
fn off_reasons_have_their_words_and_the_view_starts_without_one() {
    let words: Vec<&str> = [
        OffReason::OutputChanged,
        OffReason::Restart,
        OffReason::ChainRestarted,
        OffReason::Stopped,
    ]
    .iter()
    .map(|r| r.word())
    .collect();
    assert_eq!(
        words,
        ["output-changed", "restart", "chain-restarted", "stopped"]
    );
    let view = CrossfeedView::of(&CrossfeedSettings::default(), false, 48_000, None, None);
    assert_eq!((view.off_reason, view.unavailable), (None, None));
}

#[test]
fn snap_rounds_to_the_step_from_the_least() {
    let r = Range {
        min: 15.0,
        max: 47.0,
        step: 1.0,
    };
    assert_eq!(r.snap(37.34), 37.0);
    assert_eq!(r.snap(37.5), 38.0);
    assert_eq!(r.snap(2.0), 15.0);
    assert_eq!(r.snap(90.0), 47.0);
    let d = Range {
        min: 0.0,
        max: 1.0,
        step: 0.01,
    };
    assert_eq!(d.snap(0.2349), 0.23);
}

#[test]
fn the_delay_plays_in_whole_samples_and_the_maths_uses_those() {
    let mut s = bauer("bs2b default");
    let own = own_lag_ms(&s, 48_000);
    // 0.27 ms asked at 48 kHz adds 0.43 sample: nothing plays, nothing is sent.
    s.delay_ms = Some(own + 0.01);
    let st = Stage::of(&s, 48_000).unwrap();
    assert_eq!((st.delay_samples, st.delay_s), (0, 0.0));
    // 0.6 ms total: 16 whole samples, sent from the middle of the step.
    s.delay_ms = Some(0.6);
    let st = Stage::of(&s, 48_000).unwrap();
    assert_eq!(st.delay_samples, 16);
    assert!(near(st.delay_s * 48_000.0, 16.5, 1e-9));
    assert_eq!(
        (st.delay_s * 48_000.0) as u32,
        16,
        "PipeWire's truncation lands on it"
    );
    // The curve and the worst gain are for the samples that play: a Meier delay inside one
    // sample of the previous step gives the same numbers.
    let mut m = settings(Model::Meier, "Default");
    let own = own_lag_ms(&m, 48_000);
    m.delay_ms = Some(own + 10.0 * 1000.0 / 48_000.0 + 0.002);
    let a = (
        worst_gain_db(&m, 48_000),
        CrossfeedView::of(&m, true, 48_000, None, None).curve,
    );
    m.delay_ms = Some(own + 10.0 * 1000.0 / 48_000.0 + 0.015);
    let b = (
        worst_gain_db(&m, 48_000),
        CrossfeedView::of(&m, true, 48_000, None, None).curve,
    );
    assert_eq!(a.0, b.0);
    assert_eq!(a.1, b.1);
}

#[test]
fn every_lowpass_corner_is_where_the_model_says() {
    for rate in RATES {
        let r = f64::from(rate);
        // Mid/Side bass: the cookbook Butterworth is 3.01 dB down at fc (A/sqrt 2), and at 2 fc it is
        // the analogue 4th-power law 1/sqrt(1 + 16) down to the prewarp's small error.
        let ms = settings(Model::MidSide, "Default");
        let st = Stage::of(&ms, rate).unwrap();
        let (at_fc, _) = at(&st.x, ms.crossover_hz, rate);
        assert!(
            near(at_fc, ms.amount / 2f64.sqrt(), 2e-4),
            "{rate}: {at_fc}"
        );
        let (at_2fc, _) = at(&st.x, 2.0 * ms.crossover_hz, rate);
        assert!(
            near(at_2fc, ms.amount / 17f64.sqrt(), 0.01),
            "{rate}: {at_2fc}"
        );
        // Linkwitz and Meier: A (1 - p) / |1 - p e^-jw| from the pole's own formula.
        for model in [Model::Linkwitz, Model::Meier] {
            let s = settings(model, "Default");
            let st = Stage::of(&s, rate).unwrap();
            let p = (-2.0 * std::f64::consts::PI * s.crossover_hz / r).exp();
            for hz in [s.crossover_hz, 2.0 * s.crossover_hz] {
                let w = 2.0 * std::f64::consts::PI * hz / r;
                let want = s.amount * (1.0 - p) / (1.0 + p * p - 2.0 * p * w.cos()).sqrt();
                let (got, _) = at(&st.x, hz, rate);
                assert!(
                    near(got, want, 1e-4),
                    "{model:?} {rate} {hz}: {got} vs {want}"
                );
            }
            // The corner is at fc: about 3 dB down there (not at fc/2 or 2 fc).
            let (low, _) = at(&st.x, 1.0, rate);
            let (corner, _) = at(&st.x, s.crossover_hz, rate);
            assert!(near(db(corner / low), -3.0, 0.5), "{model:?} {rate}");
        }
    }
}

#[test]
fn the_stage_response_matches_the_curves_and_carries_the_delay_phase() {
    let s = CrossfeedSettings {
        delay_ms: Some(0.6),
        ..CrossfeedSettings::default()
    };
    let st = Stage::of(&s, 48_000).unwrap();
    assert!(st.delay_samples > 0);
    let curve = CurveView::of(&st, 48_000);
    for (i, hz) in curve.hz.iter().enumerate() {
        let r = stage_response(&st, 48_000, *hz);
        let db = |c: [f64; 2]| 20.0 * c[0].hypot(c[1]).max(1e-6).log10();
        assert!((db(r.same) - curve.same_db[i]).abs() < 0.011, "{hz}");
        assert!((db(r.other) - curve.other_db[i]).abs() < 0.011, "{hz}");
    }
    // The cross path alone, with and without the delay: the delay turns the phase by
    // `delay_samples` samples and leaves the level alone.
    let plain = Stage {
        delay_samples: 0,
        ..st.clone()
    };
    let (a, b) = (
        stage_response(&plain, 48_000, 100.0).other,
        stage_response(&st, 48_000, 100.0).other,
    );
    let turn = f64::from(st.delay_samples) * 2.0 * PI * 100.0 / 48_000.0;
    let (pa, pb) = (a[1].atan2(a[0]), b[1].atan2(b[0]));
    assert!((pa - pb - turn).abs() < 1e-9);
    assert!((a[0].hypot(a[1]) - b[0].hypot(b[1])).abs() < 1e-12);
}

#[test]
fn the_model_descriptions_are_svolders_own_words() {
    for spec in &SPECS {
        let text = spec.description.to_lowercase();
        for word in ["softened", "speakers", "ear", "late"] {
            assert!(!text.contains(word), "{:?}: {word}", spec.model);
        }
    }
    assert!(
        SPECS[Model::Bauer as usize]
            .description
            .contains("other channel")
    );
}

#[test]
fn the_meier_rockbox_preset_has_its_numbers_and_no_extra_delay() {
    let view = CrossfeedView::of(
        &CrossfeedSettings::from_preset(Model::Meier, "Rockbox").expect("a Meier preset"),
        true,
        48_000,
        None,
        None,
    );
    assert_eq!(view.preset.as_deref(), Some("Rockbox"));
    assert!(near(view.amount_pct, 24.0, 1e-9), "{}", view.amount_pct);
    assert_eq!(view.crossover_hz, 660.0);
    assert!(
        view.delay_own,
        "no delay is added on top of the model's own"
    );
    assert!(near(view.delay_ms, view.delay_floor_ms, 1e-12));
    let meier = view.models.iter().find(|m| m.word == "meier").unwrap();
    let names: Vec<&str> = meier.presets.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Default", "Rockbox"]);
    assert_eq!(meier.default_preset, "Default");
}

/// The worst gain of `stage` played at `rate` with the delay at exactly `samples` whole samples.
fn worst_with_samples(stage: &Stage, rate: u32, samples: f64) -> f64 {
    let mut held = stage.clone();
    held.delay_s = (samples + 0.5) / f64::from(rate);
    stage_worst_db(&held, rate)
}

/// At twice (or four times) the rate a stage was worked out for, its delay `(n + 0.5) / rate`
/// lands on a whole sample: PipeWire truncates `delay * rate` worked out in f32, which may land
/// on either side of it, so the worst gain is the worse of the two.
#[test]
fn a_delay_on_a_whole_sample_of_the_played_rate_counts_both_sides() {
    let cases = [
        (35.0, 2000.0, 0.15, 96_000, 192_000),
        (50.0, 2000.0, 0.15, 48_000, 192_000),
        (20.0, 1200.0, 0.25, 96_000, 192_000),
    ];
    for (amount, hz, ms, from, to) in cases {
        let settings = CrossfeedSettings::for_model(Model::Meier)
            .change(
                &CrossfeedChange {
                    amount_pct: Some(amount),
                    crossover_hz: Some(hz),
                    delay_ms: Some(ms),
                    ..Default::default()
                },
                from,
            )
            .unwrap();
        let stage = Stage::of(&settings, from).unwrap();
        let at = stage.delay_s * f64::from(to);
        assert!((at - at.round()).abs() < 1e-3, "{at} is on a whole sample");
        let worse = worst_with_samples(&stage, to, at.round() - 1.0).max(worst_with_samples(
            &stage,
            to,
            at.round(),
        ));
        let worst = stage_worst_db(&stage, to);
        assert!(
            worst >= worse - 1e-9,
            "{amount} % {hz} Hz {ms} ms {from} -> {to}: {worst} under {worse}"
        );
    }
}

/// Away from a whole sample only the truncated one plays.
#[test]
fn a_delay_between_two_samples_counts_the_truncated_one() {
    let settings = CrossfeedSettings {
        delay_ms: Some(0.5),
        ..settings(Model::Meier, "Default")
    };
    let stage = Stage::of(&settings, 48_000).unwrap();
    let samples = (stage.delay_s * 44_100.0).floor();
    assert!((stage.delay_s * 44_100.0 - samples) > 0.01);
    assert_eq!(
        stage_worst_db(&stage, 44_100),
        worst_with_samples(&stage, 44_100, samples)
    );
}

/// The worst gain worked out point by point, with every phasor from its angle: what the nested
/// grids and the rotating phasors of [`stage_worst_over_db`] must give.
fn worst_point_by_point(stage: &Stage, rate_hz: u32) -> f64 {
    let rate = f64::from(rate_hz);
    let mut worst = 1.0f64;
    for f in 1..=rate_hz / 2 {
        let w = 2.0 * std::f64::consts::PI * f64::from(f) / rate;
        for n in played_samples(stage.delay_s, rate_hz) {
            let d = Complex {
                re: (w * n).cos(),
                im: -(w * n).sin(),
            };
            let direct = response(&stage.dir, w).add(response(&stage.s, w).mul(d));
            worst = worst.max(direct.abs() + response(&stage.x, w).abs());
        }
    }
    20.0 * worst.log10()
}

#[test]
fn the_nested_grids_give_what_each_rate_gives_alone() {
    let rates = [
        44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 32_000,
    ];
    for (model, amount, delay, from) in [
        (Model::Meier, 0.5, Some(1.0), 48_000),
        (Model::Meier, 0.35, Some(0.15), 96_000),
        (Model::Meier, 0.2, None, 44_100),
        (Model::Bauer, 0.4, Some(0.8), 192_000),
        (Model::MidSide, 0.5, None, 88_200),
    ] {
        let settings = CrossfeedSettings {
            amount,
            delay_ms: delay,
            ..CrossfeedSettings::for_model(model)
        };
        let stage = Stage::of(&settings, from).unwrap();
        let mut most = f64::MIN;
        for rate in rates {
            let alone = worst_point_by_point(&stage, rate);
            let got = stage_worst_db(&stage, rate);
            assert!(
                (got - alone).abs() < 1e-9,
                "{model:?} {from} -> {rate}: {got} vs {alone}"
            );
            most = most.max(alone);
        }
        let over = stage_worst_over_db(&stage, &rates);
        assert!(
            (over - most).abs() < 1e-9,
            "{model:?} {from}: {over} vs {most}"
        );
    }
}
