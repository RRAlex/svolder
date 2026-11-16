// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer EQ never goes above unity: the preamp an apply sets.

use super::*;

/// The preamp `host.apply` ends with for `correction`.
async fn applied_preamp(f: &Fixture, correction: Correction) -> f64 {
    let mut asked = request("Bound");
    asked.correction = correction;
    let state = f.host(Manager::Process).apply(asked).await.unwrap();
    // The conf written carries the preamp that was saved.
    assert!(state.preamp_db <= 0.0, "{}", state.preamp_db);
    state.preamp_db
}

/// The peak of `correction` over the audio band, in dB.
fn peak_of(correction: &Correction) -> f64 {
    response::correction_db(correction, &response::log_freqs(1024))
        .into_iter()
        .fold(f64::NEG_INFINITY, f64::max)
}

fn parsed(text: &str) -> Correction {
    apo::parse(text).unwrap().correction
}

#[tokio::test]
async fn a_file_that_states_a_boost_is_applied_at_minus_its_peak() {
    let f = Fixture::new();
    let boosted = parsed("Preamp: +6 dB\nFilter 1: ON PK Fc 1000 Hz Gain 4 dB Q 1\n");
    let applied = applied_preamp(&f, boosted.clone()).await;
    let peak = peak_of(&boosted);
    assert!(
        applied <= -peak + 1e-9 && applied > -peak - 0.1 - 1e-9,
        "{applied} {peak}"
    );
    // The text the chain is given carries it too.
    let conf = std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap();
    let mult = 10f64.powf(applied / 20.0);
    assert!(conf.contains(&format!("Mult = {mult} ")), "{conf}");
    assert!(!conf.contains("Mult = 1.99"), "{conf}");
}

#[tokio::test]
async fn a_file_with_no_preamp_and_boosting_shelves_gets_one() {
    let f = Fixture::new();
    let shelves = parsed(
        "Filter 1: ON LSC Fc 100 Hz Gain 12 dB Q 0.7\nFilter 2: ON HSC Fc 8000 Hz Gain 12 dB Q 0.7\n",
    );
    let applied = applied_preamp(&f, shelves.clone()).await;
    let peak = peak_of(&shelves);
    assert!(peak > 11.0, "{peak}");
    assert!(applied <= -peak + 1e-9, "{applied} {peak}");
}

#[tokio::test]
async fn an_all_cut_file_with_a_boosting_preamp_is_applied_at_unity() {
    let f = Fixture::new();
    let cut = parsed("Preamp: +6 dB\nFilter 1: ON PK Fc 1000 Hz Gain -4 dB Q 1\n");
    assert_eq!(applied_preamp(&f, cut).await, 0.0);
}

#[tokio::test]
async fn an_autoeq_like_file_keeps_its_preamp() {
    let f = Fixture::new();
    let normal = parsed(
        "Preamp: -6.2 dB\nFilter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.7\n\
         Filter 2: ON PK Fc 2500 Hz Gain -3 dB Q 2\nFilter 3: ON PK Fc 6000 Hz Gain 2 dB Q 3\n",
    );
    let peak = peak_of(&normal);
    assert!(
        peak < 6.2,
        "the fixture's own preamp covers its peak: {peak}"
    );
    let applied = applied_preamp(&f, normal).await;
    assert!((applied + 6.2).abs() < 0.1, "{applied}");
}

#[tokio::test]
async fn a_preamp_beyond_thirty_decibels_is_refused() {
    assert!(apo::parse("Preamp: +31 dB\nFilter 1: ON PK Fc 1000 Hz Gain 1 dB Q 1\n").is_err());
    assert!(apo::parse("Preamp: 1000 dB\nFilter 1: ON PK Fc 1000 Hz Gain 1 dB Q 1\n").is_err());
    let f = Fixture::new();
    let mut asked = request("Bad");
    asked.correction.preamp_db = 31.0;
    assert!(f.host(Manager::Process).apply(asked).await.is_err());
    assert_eq!(f.written(), Vec::<String>::new());
}

#[tokio::test]
async fn an_imported_preset_and_a_band_preset_are_bounded_the_same_way() {
    let f = Fixture::new();
    let imported = Correction {
        preamp_db: 3.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: 1000.0,
            g: 5.0,
            q: 1.0,
        }],
    };
    let applied = applied_preamp(&f, imported.clone()).await;
    assert!(applied <= -peak_of(&imported) + 1e-9, "{applied}");
    // A band preset's correction: its bands as peaking filters, with minus the peak, rounded
    // up to 0.1 dB, as its preamp.
    let band = Correction {
        preamp_db: -6.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: 100.0,
            g: 6.0,
            q: 1.0,
        }],
    };
    let applied = applied_preamp(&f, band.clone()).await;
    assert!(applied <= -peak_of(&band) + 1e-9, "{applied}");
    assert!(
        (applied - band.preamp_db).abs() < 0.11,
        "{applied} {}",
        band.preamp_db
    );
}

#[tokio::test]
async fn a_re_apply_from_the_state_file_is_bounded_too() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    // A state file written by hand, or by another version, that states a boost.
    let mut state = host.apply(request("Old")).await.unwrap();
    host.remove().await.unwrap();
    state.on = false;
    state.preamp_db = 12.0;
    state.save(&host.state).unwrap();
    let kept = host.state().unwrap();
    let again = host
        .apply(ApplyRequest {
            name: kept.name.clone(),
            origin: kept.origin.clone(),
            correction: kept.correction(),
            sink: None,
            slots: Vec::new(),
            rate_hz: None,
            flat: false,
            crossfeed: None,
            speakers: None,
        })
        .await
        .unwrap();
    assert!(again.preamp_db <= -3.0 + 1e-9, "{}", again.preamp_db);
    assert_eq!(host.state().unwrap().preamp_db, again.preamp_db);
}

#[test]
fn the_note_names_both_preamps_only_when_one_was_lowered() {
    assert_eq!(
        lowered_note(-2.0, -6.4).as_deref(),
        Some("preamp lowered from -2.0 dB to -6.4 dB so the correction never clips")
    );
    assert_eq!(lowered_note(-6.4, -6.4), None);
    assert_eq!(lowered_note(-6.4, -6.4 + 0.0), None);
}
