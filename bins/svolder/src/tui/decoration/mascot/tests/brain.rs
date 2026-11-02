// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The brain: events from views, presence, visits, codes and the session carry-over.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use svolder_proto::FunctionId;

use svolder_host::players::Status;

use super::super::brain::What;
use super::super::tricks::{Dance, State, Trick};
use super::super::{Code, EqSig, Link, Mascot, Place, View};
use crate::tui::test_support::ms;

/// EQ readings of functions 5.0, 5.1, ... with these raws.
fn raws(values: &[i64]) -> BTreeMap<FunctionId, i64> {
    values
        .iter()
        .enumerate()
        .map(|(n, raw)| (FunctionId::two(5, n as u8), *raw))
        .collect()
}

/// A view with a volume of `raw` over 0..=100, mute, power and an input.
fn view() -> View {
    View {
        volume: Some((50, 0, 100)),
        muted: Some(false),
        power: Some(true),
        input: Some(1),
        eq: Some(EqSig {
            preset: None,
            raws: raws(&[0, 0]),
        }),
        ..View::bare()
    }
}

/// A hidden squirrel that has seen `view()` at `t0`.
fn hidden(t0: Instant) -> Mascot {
    let mut m = Mascot::new(t0);
    m.observe(&view(), t0);
    m
}

/// A called squirrel whose arrival has finished, having seen `view()`.
fn called(t0: Instant) -> Mascot {
    let mut m = hidden(t0);
    m.code(Code::Konami, t0);
    m.observe(&view(), t0 + ms(10_000));
    m
}

fn what(m: &Mascot) -> Option<What> {
    m.what().map(|(what, _)| what)
}

fn trick(t: Trick) -> Option<What> {
    Some(What::Trick(t))
}

fn state(s: State) -> Option<What> {
    Some(What::State(s))
}

#[test]
fn the_first_view_and_a_field_appearing_fire_nothing() {
    let t0 = Instant::now();
    let mut m = Mascot::new(t0);
    m.code(Code::Konami, t0);
    m.observe(&View::bare(), t0 + ms(10_000));
    assert_eq!(what(&m), state(State::Rest));
    let mut v = View::bare();
    v.volume = Some((50, 0, 100));
    v.muted = Some(true);
    v.input = Some(3);
    m.observe(&v, t0 + ms(10_100));
    assert_eq!(what(&m), state(State::Muted), "mute stands, no trick plays");
    v.muted = None;
    m.observe(&v, t0 + ms(10_200));
    assert_eq!(what(&m), state(State::Rest));
}

#[test]
fn volume_climbs_slides_and_a_tenth_of_the_range_swings() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    let mut at = t0 + ms(20_000);
    for (raw, want) in [
        (51, Trick::Climb),
        (50, Trick::Slide),
        (62, Trick::BigSwing),
        (63, Trick::Climb),
    ] {
        v.volume = Some((raw, 0, 100));
        at += ms(2_000);
        m.observe(&v, at);
        assert_eq!(what(&m), trick(want), "at {raw}");
        assert_eq!(m.what().unwrap().1, Place::Home);
    }
}

#[test]
fn mute_sleeps_and_unmute_wakes_and_input_runs() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.muted = Some(true);
    m.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&m), state(State::Muted));
    v.muted = Some(false);
    m.observe(&v, t0 + ms(21_000));
    assert_eq!(what(&m), trick(Trick::Wake));
    v.input = Some(2);
    m.observe(&v, t0 + ms(30_000));
    assert_eq!(what(&m), trick(Trick::Messenger));
}

#[test]
fn an_eq_change_is_a_palette() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.eq = Some(EqSig {
        preset: Some("Warm".into()),
        raws: raws(&[0, 0]),
    });
    m.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&m), trick(Trick::Palette));
    v.eq = Some(EqSig {
        preset: Some("Warm".into()),
        raws: raws(&[0, 3]),
    });
    m.observe(&v, t0 + ms(30_000));
    assert_eq!(what(&m), trick(Trick::Palette));
}

#[test]
fn hidden_shows_input_only_and_by_screen() {
    let t0 = Instant::now();
    let mut m = hidden(t0);
    let mut v = view();
    v.volume = Some((60, 0, 100));
    v.muted = Some(true);
    v.eq = None;
    m.observe(&v, t0 + ms(1_000));
    assert!(m.showing().is_none());
    v.input = Some(2);
    m.observe(&v, t0 + ms(2_000));
    assert_eq!(m.what(), Some((What::Trick(Trick::Messenger), Place::Home)));
    assert!(m.showing().is_some());
    m.observe(&v, t0 + ms(2_000) + ms(6_001));
    assert!(m.showing().is_none(), "a visit goes after six seconds");
    v.screen_is_home = false;
    v.input = Some(3);
    m.observe(&v, t0 + ms(20_000));
    assert_eq!(
        m.what(),
        Some((What::Trick(Trick::Messenger), Place::Float))
    );
}

#[test]
fn a_called_squirrel_off_home_visits_only_for_what_a_hidden_one_does() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.screen_is_home = false;
    m.observe(&v, t0 + ms(20_000));
    assert!(m.showing().is_none(), "no event, nothing shows off home");
    // Volume, mute, the tone control and the music stay at home.
    v.volume = Some((51, 0, 100));
    v.muted = Some(true);
    v.eq = Some(EqSig {
        preset: None,
        raws: raws(&[3, 0]),
    });
    v.playing = Some((Status::Playing, Vec::new()));
    m.observe(&v, t0 + ms(21_000));
    v.muted = Some(false);
    m.observe(&v, t0 + ms(22_000));
    assert!(m.showing().is_none());
    // An input switch floats, as for a hidden squirrel.
    v.input = Some(2);
    m.observe(&v, t0 + ms(23_000));
    assert_eq!(
        m.what(),
        Some((What::Trick(Trick::Messenger), Place::Float))
    );
    // Back on the home screen the sticky state shows at home.
    v.muted = Some(true);
    v.screen_is_home = true;
    m.observe(&v, t0 + ms(30_000));
    assert_eq!(m.what(), Some((What::State(State::Muted), Place::Home)));
}

#[test]
fn a_called_squirrel_off_home_floats_a_lost_link_and_a_power_off() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.screen_is_home = false;
    m.observe(&v, t0 + ms(20_000));
    v.link = Link::Reconnecting;
    m.observe(&v, t0 + ms(21_000));
    assert_eq!(m.what(), Some((What::Trick(Trick::RowOut), Place::Float)));
    v.link = Link::Connected;
    m.observe(&v, t0 + ms(22_000));
    v.power = Some(false);
    m.observe(&v, t0 + ms(30_000));
    assert_eq!(m.what(), Some((What::State(State::Curled), Place::Float)));
}

#[test]
fn a_fault_points_until_the_advice_shows_or_it_clears() {
    for is_called in [false, true] {
        let t0 = Instant::now();
        let mut m = if is_called { called(t0) } else { hidden(t0) };
        let mut v = view();
        v.fault = true;
        m.observe(&v, t0 + ms(20_000));
        assert_eq!(what(&m), state(State::Pointing));
        m.observe(&v, t0 + ms(60_000));
        assert_eq!(what(&m), state(State::Pointing), "no deadline");
        v.advice_shown = true;
        m.observe(&v, t0 + ms(61_000));
        assert_eq!(what(&m), is_called.then_some(What::State(State::Rest)));
        v.advice_shown = false;
        v.fault = false;
        m.observe(&v, t0 + ms(62_000));
        v.fault = true;
        m.observe(&v, t0 + ms(63_000));
        v.fault = false;
        m.observe(&v, t0 + ms(64_000));
        assert_eq!(what(&m), is_called.then_some(What::State(State::Rest)));
    }
}

#[test]
fn the_link_rows_lights_a_lantern_and_comes_back() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.link = Link::Reconnecting;
    m.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&m), state(State::Rowing));
    v.link = Link::Offline;
    m.observe(&v, t0 + ms(21_000));
    assert_eq!(what(&m), state(State::Lantern));
    v.link = Link::Connected;
    m.observe(&v, t0 + ms(22_000));
    assert_eq!(what(&m), trick(Trick::RowIn));

    let mut h = hidden(t0);
    let mut v = view();
    v.link = Link::Reconnecting;
    h.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&h), trick(Trick::RowOut));
    v.link = Link::Connected;
    h.observe(&v, t0 + ms(30_000));
    assert_eq!(what(&h), trick(Trick::RowIn));
    h.observe(&v, t0 + ms(30_000) + Trick::RowIn.duration() + ms(1));
    assert_eq!(
        what(&h),
        state(State::Rest),
        "rests until the six seconds end"
    );
}

#[test]
fn power_curls_and_stretches() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.power = Some(false);
    m.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&m), state(State::Curled));
    m.observe(&v, t0 + ms(80_000));
    assert_eq!(what(&m), state(State::Curled), "sticky");
    v.power = Some(true);
    m.observe(&v, t0 + ms(81_000));
    assert_eq!(what(&m), trick(Trick::Stretch));

    let mut h = hidden(t0);
    let mut v = view();
    v.power = Some(false);
    h.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&h), state(State::Curled));
    h.observe(&v, t0 + ms(26_001));
    assert_eq!(what(&h), None);
    v.power = Some(true);
    h.observe(&v, t0 + ms(30_000));
    assert_eq!(what(&h), trick(Trick::Stretch));
}

#[test]
fn a_fault_outranks_mute_and_a_one_shot_waits_out_a_state() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.muted = Some(true);
    m.observe(&v, t0 + ms(20_000));
    v.fault = true;
    m.observe(&v, t0 + ms(21_000));
    assert_eq!(what(&m), state(State::Pointing));
    v.fault = false;
    m.observe(&v, t0 + ms(22_000));
    assert_eq!(what(&m), state(State::Muted));
    v.volume = Some((55, 0, 100));
    m.observe(&v, t0 + ms(23_000));
    assert_eq!(
        what(&m),
        state(State::Muted),
        "the climb is not shown while muted"
    );
}

#[test]
fn a_fault_visit_is_not_replaced_by_a_one_shot() {
    let t0 = Instant::now();
    let mut h = hidden(t0);
    let mut v = view();
    v.fault = true;
    h.observe(&v, t0 + ms(1_000));
    v.input = Some(9);
    h.observe(&v, t0 + ms(2_000));
    assert_eq!(what(&h), state(State::Pointing));
}

#[test]
fn each_hour_says_so_once() {
    for is_called in [false, true] {
        let t0 = Instant::now();
        let mut m = if is_called { called(t0) } else { hidden(t0) };
        let v = view();
        assert_eq!(m.observe(&v, t0 + Duration::from_secs(3599)), None);
        assert_eq!(
            m.observe(&v, t0 + Duration::from_secs(3600)).as_deref(),
            Some("1 h in. Ratatoskr stretches.")
        );
        assert_eq!(what(&m), trick(Trick::Stretch));
        assert_eq!(m.observe(&v, t0 + Duration::from_secs(3700)), None);
        assert_eq!(
            m.observe(&v, t0 + Duration::from_secs(7200)).as_deref(),
            Some("2 h in. Ratatoskr stretches.")
        );
    }
}

#[test]
fn the_tenth_advice_press_hops_without_a_fault() {
    let t0 = Instant::now();
    let mut m = hidden(t0);
    let mut v = view();
    v.presses = 9;
    m.observe(&v, t0 + ms(1_000));
    assert_eq!(what(&m), None);
    v.presses = 10;
    m.observe(&v, t0 + ms(2_000));
    assert_eq!(what(&m), trick(Trick::Hop));
    v.presses = 11;
    m.observe(&v, t0 + ms(9_000));
    assert_eq!(what(&m), None);
    v.presses = 20;
    v.fault = true;
    m.observe(&v, t0 + ms(10_000));
    assert_eq!(what(&m), state(State::Pointing), "a fault is not a hop");
}

#[test]
fn the_codes_call_dismiss_and_make_a_god() {
    let t0 = Instant::now();
    let mut m = hidden(t0);
    assert!(!m.called());
    m.code(Code::Konami, t0 + ms(1_000));
    assert!(m.called());
    assert_eq!(what(&m), trick(Trick::Arrive));
    m.observe(&view(), t0 + ms(1_000) + Trick::Arrive.duration() + ms(1));
    assert_eq!(m.what(), Some((What::State(State::Rest), Place::Home)));
    let at = t0 + ms(20_000);
    m.code(Code::Konami, at);
    assert!(!m.called());
    assert_eq!(what(&m), trick(Trick::Depart));
    m.observe(&view(), at + Trick::Depart.duration() + ms(1));
    assert_eq!(what(&m), None);

    assert!(!m.god());
    m.code(Code::Iddqd, t0 + ms(40_000));
    assert!(m.god());
    assert_eq!(what(&m), trick(Trick::Hop));
    m.code(Code::Iddqd, t0 + ms(41_000));
    assert!(!m.god());
    assert_eq!(what(&m), trick(Trick::Hop));
}

#[test]
fn a_click_hops_only_while_showing() {
    let t0 = Instant::now();
    let mut m = hidden(t0);
    m.click(t0 + ms(1_000));
    assert_eq!(what(&m), None);
    let mut m = called(t0);
    m.click(t0 + ms(20_000));
    assert_eq!(what(&m), trick(Trick::Hop));
}

#[test]
fn a_reopen_keeps_the_session_but_not_the_baseline() {
    let t0 = Instant::now();
    let mut m = called(t0);
    m.code(Code::Iddqd, t0 + ms(11_000));
    let mut v = view();
    v.link = Link::Reconnecting;
    m.observe(&v, t0 + ms(20_000));
    let mut m = m.reopened();
    assert!(m.called() && m.god());
    // The new device's first values fire nothing; the link was down before the reopen.
    let mut v = view();
    v.volume = Some((90, 0, 100));
    v.input = Some(7);
    m.observe(&v, t0 + ms(21_000));
    assert_eq!(
        what(&m),
        trick(Trick::RowIn),
        "neither the volume nor the input fired"
    );
}

#[test]
fn a_held_volume_key_is_one_climb_that_ends_with_the_key() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    let first = t0 + ms(20_000);
    let mut last = first;
    for step in 0..60 {
        v.volume = Some((50 + step + 1, 0, 200));
        last = first + ms(50 * step as u64);
        m.observe(&v, last);
        assert_eq!(what(&m), trick(Trick::Climb));
    }
    assert_eq!(
        m.showing().unwrap().pose,
        Trick::Climb.pose(
            Duration::ZERO,
            &super::super::looks::dress(
                super::super::looks::Costume::of_palette(""),
                None,
                m.showing().map(|s| s.elapsed).unwrap()
            )
        ),
        "the climb starts at the last step"
    );
    m.observe(&v, last + ms(300));
    let shown = m.showing().unwrap();
    let dress = super::super::looks::dress(
        super::super::looks::Costume::of_palette(""),
        None,
        shown.elapsed,
    );
    assert_eq!(shown.pose, Trick::Climb.pose(ms(300), &dress));
    assert!(m.animating());
    m.observe(&v, last + Trick::Climb.duration() - ms(1));
    assert!(m.animating());
    m.observe(&v, last + Trick::Climb.duration() + ms(1));
    assert!(!m.animating());
}

#[test]
fn a_flapping_link_keeps_one_visit_and_says_nothing() {
    let t0 = Instant::now();
    let mut m = hidden(t0);
    let mut v = view();
    let mut said = 0;
    for flip in 1..=30u64 {
        v.link = if flip % 2 == 1 {
            Link::Reconnecting
        } else {
            Link::Connected
        };
        said += usize::from(m.observe(&v, t0 + ms(2_000 * flip)).is_some());
        assert!(matches!(
            what(&m),
            Some(What::Trick(Trick::RowOut | Trick::RowIn))
        ));
    }
    assert_eq!(said, 0);
}

#[test]
fn a_device_with_no_controls_never_fires() {
    let t0 = Instant::now();
    let mut m = hidden(t0);
    let v = View::bare();
    for n in 1..=20 {
        m.observe(&v, t0 + ms(100 * n));
        assert!(m.showing().is_none());
    }
    let mut c = Mascot::new(t0);
    c.code(Code::Konami, t0);
    for n in 1..=20 {
        c.observe(&v, t0 + ms(10_000 + 100 * n));
        assert_eq!(what(&c), state(State::Rest));
    }
}

#[test]
fn music_dances_while_the_player_plays() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.playing = Some((Status::Playing, Vec::new()));
    m.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&m), state(State::Music(Dance::Bob)));
    v.playing = Some((Status::Paused, Vec::new()));
    m.observe(&v, t0 + ms(21_000));
    assert_eq!(what(&m), state(State::Rest));
    let mut h = hidden(t0);
    let mut v = view();
    v.playing = Some((Status::Playing, Vec::new()));
    h.observe(&v, t0 + ms(1_000));
    assert!(h.showing().is_none());
}

#[test]
fn the_costume_follows_the_palette_and_god_overrides_it() {
    use super::super::looks::Costume;
    let t0 = Instant::now();
    let mut m = called(t0);
    m.palette = "neon".into();
    assert_eq!(m.showing().unwrap().paint.costume, Costume::Neon);
    m.code(Code::Iddqd, t0 + ms(11_000));
    assert_eq!(m.showing().unwrap().paint.costume, Costume::God);
}

#[test]
fn eq_readings_arriving_after_connect_fire_nothing_until_one_changes() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.eq = None;
    m.observe(&v, t0 + ms(20_000));
    for (n, count) in [1usize, 2, 3].into_iter().enumerate() {
        v.eq = Some(EqSig {
            preset: None,
            raws: raws(&[4; 3][..count]),
        });
        m.observe(&v, t0 + ms(21_000 + 1_000 * n as u64));
        assert_eq!(what(&m), state(State::Rest), "{count} readings heard");
    }
    v.eq = Some(EqSig {
        preset: None,
        raws: raws(&[4, 9, 4]),
    });
    m.observe(&v, t0 + ms(30_000));
    assert_eq!(what(&m), trick(Trick::Palette));
    v.eq = Some(EqSig {
        preset: Some("Warm".into()),
        raws: raws(&[4, 9, 4]),
    });
    m.observe(&v, t0 + ms(40_000));
    assert_eq!(what(&m), trick(Trick::Palette), "a loaded preset");
}

#[test]
fn a_click_hops_over_a_sticky_state_but_not_over_pointing() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.muted = Some(true);
    m.observe(&v, t0 + ms(20_000));
    let at = t0 + ms(21_000);
    m.click(at);
    assert_eq!(what(&m), trick(Trick::Hop));
    m.observe(&v, at + Trick::Hop.duration() + ms(1));
    assert_eq!(what(&m), state(State::Muted));
    v.fault = true;
    m.observe(&v, t0 + ms(30_000));
    m.click(t0 + ms(31_000));
    assert_eq!(what(&m), state(State::Pointing));
}

#[test]
fn a_forced_one_shot_gives_the_pointing_back_while_the_fault_stands() {
    let t0 = Instant::now();
    let mut h = hidden(t0);
    let mut v = view();
    v.fault = true;
    h.observe(&v, t0 + ms(1_000));
    h.code(Code::Iddqd, t0 + ms(2_000));
    assert_eq!(what(&h), trick(Trick::Hop));
    h.observe(&v, t0 + ms(2_000) + ms(6_001));
    assert_eq!(what(&h), state(State::Pointing));
    v.advice_shown = true;
    h.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&h), None);
}

#[test]
fn a_swing_is_a_tenth_of_the_range_whatever_the_range() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.volume = Some((40, 0, 79));
    m.observe(&v, t0 + ms(20_000));
    for (raw, want, at) in [
        (47, Trick::Climb, 30_000),
        (55, Trick::BigSwing, 40_000),
        (48, Trick::Slide, 50_000),
        (40, Trick::BigSwing, 60_000),
    ] {
        v.volume = Some((raw, 0, 79));
        m.observe(&v, t0 + ms(at));
        assert_eq!(what(&m), trick(want), "to {raw}");
    }
}

#[test]
fn a_genre_picks_the_dance() {
    let of = |words: &[&str]| {
        super::super::dance_of(&words.iter().map(|w| (*w).to_string()).collect::<Vec<_>>())
    };
    assert_eq!(of(&["Rock"]), Dance::Headbang);
    assert_eq!(of(&["METAL"]), Dance::Headbang);
    assert_eq!(of(&["Deep House"]), Dance::Bounce);
    assert_eq!(of(&["Classical"]), Dance::Sway);
    assert_eq!(of(&["Hip-Hop"]), Dance::Nod);
    assert_eq!(of(&["Hip Hop"]), Dance::Nod);
    assert_eq!(of(&["Polka"]), Dance::Bob);
    assert_eq!(of(&[]), Dance::Bob);
    assert_eq!(of(&["Polka", "Jazz"]), Dance::Sway);
}

#[test]
fn the_genre_sets_the_dance_and_pause_rests() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.playing = Some((Status::Playing, vec!["Jazz".to_string()]));
    m.observe(&v, t0 + ms(20_000));
    assert_eq!(what(&m), state(State::Music(Dance::Sway)));
    let pose = m.showing().expect("shown").pose;
    assert!(pose.parts.iter().any(|part| part.layer == "phones"));
    v.playing = Some((Status::Paused, vec!["Jazz".to_string()]));
    m.observe(&v, t0 + ms(21_000));
    assert_eq!(what(&m), state(State::Rest));
}

/// Feed `level` at `at` while a track plays.
fn hear(m: &mut Mascot, level: Option<f64>, at: Instant) {
    let mut v = view();
    v.level = level;
    v.playing = Some((Status::Playing, Vec::new()));
    m.observe(&v, at);
}

#[test]
fn a_level_rising_past_its_average_is_a_beat_at_most_every_200_ms() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let t = t0 + ms(20_000);
    for i in 0..10 {
        hear(&mut m, Some(0.2), t + ms(i * 50));
    }
    assert_eq!(m.beat().count, 0);
    hear(&mut m, Some(0.6), t + ms(500));
    assert_eq!(m.beat().count, 1);
    assert_eq!(m.beat().since, ms(0));
    // Within 200 ms of the last beat, another peak does not count.
    hear(&mut m, Some(0.1), t + ms(550));
    hear(&mut m, Some(0.7), t + ms(600));
    assert_eq!(m.beat().count, 1);
    assert_eq!(m.beat().since, ms(100));
    hear(&mut m, Some(0.1), t + ms(650));
    hear(&mut m, Some(0.7), t + ms(750));
    assert_eq!(m.beat().count, 2);
    // A quiet level is no beat, however much it rises.
    let mut q = called(t0);
    for i in 0..10 {
        hear(&mut q, Some(0.001), t + ms(i * 50));
    }
    hear(&mut q, Some(0.04), t + ms(500));
    assert_eq!(q.beat().count, 0);
}

#[test]
fn with_no_level_the_beat_keeps_120_bpm() {
    let t0 = Instant::now();
    let mut m = called(t0);
    hear(&mut m, None, t0 + ms(20_000));
    let a = m.beat();
    hear(&mut m, None, t0 + ms(20_500));
    let b = m.beat();
    assert_eq!(b.count, a.count + 1);
    assert_eq!(b.since, a.since);
}

#[test]
fn with_no_level_the_plain_bob_sits_still_and_the_other_dances_keep_the_beat() {
    let t0 = Instant::now();
    let poses = |genre: &[&str]| {
        let mut m = called(t0);
        let mut v = view();
        v.playing = Some((
            Status::Playing,
            genre.iter().map(|w| (*w).to_string()).collect(),
        ));
        (0..8)
            .map(|n| {
                m.observe(&v, t0 + ms(20_000 + 125 * n));
                m.showing().expect("shown").pose
            })
            .collect::<Vec<_>>()
    };
    let bob = poses(&[]);
    assert!(bob.iter().all(|pose| *pose == bob[0]), "the bob sits still");
    let rock = poses(&["Rock"]);
    assert!(rock.iter().any(|pose| *pose != rock[0]), "a dance moves");
}

#[test]
fn a_dance_hit_is_met_by_every_idle_tick() {
    let t0 = Instant::now();
    for genre in ["Rock", "House", "Jazz", "Rap"] {
        for phase in (0..250).step_by(25) {
            let mut m = called(t0);
            let mut v = view();
            v.playing = Some((Status::Playing, vec![genre.to_string()]));
            let mut hits = 0;
            for tick in 0..4 {
                m.observe(&v, t0 + ms(20_000 + phase + 250 * tick));
                let pose = m.showing().expect("shown").pose;
                let head = pose.parts.iter().find(|part| part.layer == "head");
                if pose.dy != 0 || head.is_some_and(|head| (head.dx, head.dy) != (0, 0)) {
                    hits += 1;
                }
            }
            assert!(hits > 0, "{genre} at phase {phase}");
        }
    }
}

#[test]
fn a_code_visit_plays_over_a_sticky_state_at_home() {
    let t0 = Instant::now();
    let mut v = view();
    v.muted = Some(true);
    let mut m = Mascot::new(t0);
    m.observe(&v, t0);
    m.code(Code::Konami, t0 + ms(1_000));
    assert_eq!(what(&m), trick(Trick::Arrive));
    m.observe(&v, t0 + ms(1_000) + Trick::Arrive.duration() + ms(1));
    assert_eq!(what(&m), state(State::Muted));
    m.code(Code::Iddqd, t0 + ms(20_000));
    assert_eq!(what(&m), trick(Trick::Hop));
    m.observe(&v, t0 + ms(20_000) + Trick::Hop.duration() + ms(1));
    assert_eq!(what(&m), state(State::Muted));
    // Pointing is never covered.
    v.muted = Some(false);
    v.fault = true;
    m.observe(&v, t0 + ms(40_000));
    m.code(Code::Iddqd, t0 + ms(41_000));
    assert_eq!(what(&m), state(State::Pointing));
}

#[test]
fn a_fault_with_the_advice_already_shown_does_not_point() {
    let t0 = Instant::now();
    let mut m = hidden(t0);
    let mut v = view();
    v.advice_shown = true;
    v.fault = true;
    m.observe(&v, t0 + ms(1_000));
    assert_eq!(what(&m), None);
}

#[test]
fn the_hour_speaks_only_when_the_stretch_shows() {
    let t0 = Instant::now();
    // A called squirrel in a sticky state at home does not stretch in view.
    let mut m = called(t0);
    let mut v = view();
    v.muted = Some(true);
    m.observe(&v, t0 + ms(20_000));
    assert_eq!(m.observe(&v, t0 + Duration::from_secs(3600)), None);
    // A hidden squirrel pointing at a fault does not either.
    let mut h = hidden(t0);
    let mut f = view();
    f.fault = true;
    h.observe(&f, t0 + ms(1_000));
    assert_eq!(h.observe(&f, t0 + Duration::from_secs(3600)), None);
}

#[test]
fn a_step_on_a_coarse_volume_climbs_and_a_jump_of_two_swings() {
    let t0 = Instant::now();
    let mut m = called(t0);
    let mut v = view();
    v.volume = Some((2, 0, 5));
    m.observe(&v, t0 + ms(20_000));
    for (raw, want, at) in [
        (3, Trick::Climb, 30_000),
        (2, Trick::Slide, 40_000),
        (4, Trick::BigSwing, 50_000),
        (5, Trick::Climb, 60_000),
    ] {
        v.volume = Some((raw, 0, 5));
        m.observe(&v, t0 + ms(at));
        assert_eq!(what(&m), trick(want), "to {raw}");
    }
}

#[test]
fn the_beat_holds_through_a_short_gap_in_the_level() {
    let t0 = Instant::now();
    let t = t0 + ms(20_000);
    let mut m = called(t0);
    for i in 0..10 {
        hear(&mut m, Some(0.2), t + ms(i * 50));
    }
    hear(&mut m, Some(0.6), t + ms(500));
    let count = m.beat().count;
    assert_eq!(count, 1);
    // No level for 600 ms: the level's own beat still counts.
    hear(&mut m, None, t + ms(1_100));
    assert_eq!(m.beat().count, count);
    hear(&mut m, Some(0.2), t + ms(1_150));
    assert_eq!(m.beat().count, count);
    // No level for 1.5 s: the steady beat takes over.
    hear(&mut m, None, t + ms(2_650));
    let elapsed = (t + ms(2_650)).duration_since(t0).as_millis() as u64;
    assert_eq!(m.beat().count, elapsed / 500);
}

#[test]
fn an_hour_stretch_replaced_in_the_same_observation_says_nothing() {
    let t0 = Instant::now();
    let mut m = hidden(t0);
    let mut v = view();
    v.input = Some(2);
    assert_eq!(m.observe(&v, t0 + Duration::from_secs(3600)), None);
    assert_eq!(what(&m), trick(Trick::Messenger));
}
