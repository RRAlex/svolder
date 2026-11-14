// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! When the squirrel enters and leaves: the hour, the thirty-minute zap, every reason to appear,
//! the keys held by a box, and the effects switch. All on paused time: every method is told its
//! instant.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use super::super::brain::What;
use super::super::fx::Entrance;
use super::super::tricks::{State, Trick};
use super::super::{Code, Link, Mascot, Place, View};
use crate::tui::test_support::ms;

const HOUR: Duration = Duration::from_secs(3600);
const HALF: Duration = Duration::from_secs(1800);

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn with_effects(t0: Instant, effects: bool) -> Mascot {
    let mut m = Mascot::new(t0);
    m.effects = effects;
    m.observe(&View::bare(), t0);
    m
}

fn entering(m: &Mascot) -> Option<Entrance> {
    m.showing()
        .and_then(|s| s.fx)
        .filter(|fx| !fx.out)
        .map(|fx| fx.kind)
}

fn leaving(m: &Mascot) -> Option<Entrance> {
    m.showing()
        .and_then(|s| s.fx)
        .filter(|fx| fx.out)
        .map(|fx| fx.kind)
}

/// Observe `view` every ten seconds from `from` up to `to`: the loop never goes quiet longer.
fn walk(m: &mut Mascot, view: &View, from: Instant, to: Instant) {
    let mut at = from;
    while at < to {
        at += secs(10);
        m.observe(view, at.min(to));
    }
}

fn bare() -> View {
    View::bare()
}

#[test]
fn the_hour_calls_the_squirrel_out_with_an_entrance_and_it_stays() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    assert_eq!(m.observe(&bare(), t0 + secs(3599)), None);
    assert!(m.what().is_none());
    let toast = m.observe(&bare(), t0 + HOUR);
    assert!(m.called());
    assert!(entering(&m).is_some(), "it enters");
    assert_eq!(toast.as_deref(), Some("1 h in. Ratatoskr stretches."));
    let kind = entering(&m).unwrap();
    // The entrance lands, then the stretch plays, then it rests, and it stays.
    m.observe(&bare(), t0 + HOUR + kind.duration() + ms(1));
    assert!(m.showing().unwrap().fx.is_none());
    for at in [secs(10), secs(600), secs(1700)] {
        m.observe(&bare(), t0 + HOUR + at);
        assert_eq!(m.what().map(|(_, place)| place), Some(Place::Home));
        assert!(m.showing().unwrap().fx.is_none());
    }
}

#[test]
fn it_zaps_out_and_back_in_every_half_hour_from_then_on() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    m.observe(&bare(), t0 + HOUR);
    let mut landed = t0 + HOUR + entering(&m).unwrap().duration();
    for round in 0..3 {
        m.observe(&bare(), landed + HALF - ms(100));
        assert!(m.showing().unwrap().fx.is_none(), "round {round}: early");
        let zap = landed + HALF;
        m.observe(&bare(), zap);
        let out = leaving(&m).unwrap_or_else(|| panic!("round {round}: no exit"));
        // Out, then gone for a moment, then in again.
        m.observe(&bare(), zap + out.duration() + ms(50));
        assert!(m.what().is_none(), "round {round}: gone between");
        let mut back = None;
        for step in 1..80 {
            let at = zap + out.duration() + ms(50 * step);
            m.observe(&bare(), at);
            if let Some(kind) = entering(&m) {
                back = Some((kind, at));
                break;
            }
        }
        let (kind, at) = back.unwrap_or_else(|| panic!("round {round}: it never came back"));
        let done = at + kind.duration() + ms(50);
        m.observe(&bare(), done);
        assert!(m.showing().unwrap().fx.is_none());
        landed = done;
        // The next zap counts from the landing, within one observation's slack.
        assert!(m.called());
    }
}

#[test]
fn a_held_box_makes_the_hour_wait_then_enter() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    let held = View {
        keys_held: true,
        ..bare()
    };
    m.observe(&held, t0 + HOUR);
    m.observe(&held, t0 + HOUR + secs(30));
    assert!(m.what().is_none());
    let release = t0 + HOUR + secs(31);
    m.observe(&bare(), release);
    assert!(m.called());
    let fx = m.showing().unwrap().fx.expect("it enters on release");
    assert!(!fx.out);
    assert!(fx.progress < 0.05, "from its start: {}", fx.progress);
}

#[test]
fn a_zap_waits_for_a_held_box_too() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    m.observe(&bare(), t0 + HOUR);
    let landed = t0 + HOUR + entering(&m).unwrap().duration() + ms(1);
    m.observe(&bare(), landed);
    let held = View {
        keys_held: true,
        ..bare()
    };
    walk(&mut m, &held, landed, landed + HALF + secs(5));
    assert!(m.showing().unwrap().fx.is_none(), "the squirrel stays put");
    m.observe(&bare(), landed + HALF + secs(6));
    assert!(leaving(&m).is_some());
}

#[test]
fn the_typed_code_waits_for_the_box_it_was_typed_in() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    let held = View {
        keys_held: true,
        ..bare()
    };
    m.observe(&held, t0 + ms(100));
    m.code(Code::Konami, t0 + ms(200));
    m.observe(&held, t0 + ms(300));
    assert!(m.what().is_none(), "nothing shows under the box");
    m.observe(&bare(), t0 + ms(5_000));
    assert!(m.called());
    assert!(entering(&m).is_some());
}

#[test]
fn every_reason_to_appear_takes_an_entrance_and_an_exit() {
    let t0 = Instant::now();
    let on = View {
        power: Some(true),
        input: Some(1),
        ..bare()
    };
    let off = View {
        power: Some(false),
        ..on.clone()
    };
    let cases = [
        (
            "fault",
            on.clone(),
            View {
                fault: true,
                ..on.clone()
            },
        ),
        (
            "link lost",
            on.clone(),
            View {
                link: Link::Reconnecting,
                ..on.clone()
            },
        ),
        (
            "link offline",
            on.clone(),
            View {
                link: Link::Offline,
                ..on.clone()
            },
        ),
        ("power off", on.clone(), off.clone()),
        ("power on", off, on.clone()),
        (
            "input",
            on.clone(),
            View {
                input: Some(2),
                ..on.clone()
            },
        ),
    ];
    for (name, before, after) in cases {
        let mut m = Mascot::new(t0);
        m.effects = true;
        m.observe(&before, t0);
        m.observe(&after, t0 + ms(100));
        assert!(entering(&m).is_some(), "{name}: no entrance");
    }
    // A returned link after a lost one, which then leaves by an exit.
    let mut m = Mascot::new(t0);
    m.effects = true;
    let lost = View {
        link: Link::Reconnecting,
        ..on.clone()
    };
    m.observe(&on, t0);
    m.observe(&lost, t0 + ms(100));
    walk(&mut m, &lost, t0 + ms(100), t0 + secs(30));
    m.observe(&on, t0 + secs(31));
    assert!(entering(&m).is_some(), "a returned link enters");
    let mut seen_exit = false;
    for step in 1..400 {
        m.observe(&on, t0 + secs(31) + ms(step * 50));
        seen_exit |= leaving(&m).is_some();
    }
    assert!(seen_exit, "the visitor leaves by an exit");
    assert!(m.what().is_none());
}

#[test]
fn a_visitor_that_is_already_showing_enters_only_once() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    let mut v = View {
        input: Some(1),
        ..bare()
    };
    m.observe(&v, t0);
    v.input = Some(2);
    m.observe(&v, t0 + ms(100));
    let first = entering(&m).unwrap();
    m.observe(&v, t0 + ms(100) + first.duration() + ms(10));
    v.input = Some(3);
    m.observe(&v, t0 + ms(100) + first.duration() + ms(20));
    assert!(entering(&m).is_none(), "already there");
    assert_eq!(
        m.what().map(|(w, _)| w),
        Some(What::Trick(Trick::Messenger))
    );
}

#[test]
fn with_effects_off_it_pops_in_whole_and_zaps_without_animation() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, false);
    m.observe(&bare(), t0 + HOUR);
    assert!(m.called());
    assert_eq!(m.what(), Some((What::Trick(Trick::Stretch), Place::Home)));
    assert!(m.showing().unwrap().fx.is_none());
    m.observe(&bare(), t0 + HOUR + secs(10));
    assert_eq!(m.what(), Some((What::State(State::Rest), Place::Home)));
    // The zap still comes: gone for a moment, back whole, never with an effect.
    walk(&mut m, &bare(), t0 + HOUR + secs(10), t0 + HOUR + HALF);
    let mut gone = false;
    for step in 0..400 {
        m.observe(&bare(), t0 + HOUR + HALF + ms(step * 50));
        assert!(m.showing().is_none_or(|s| s.fx.is_none()));
        gone |= m.what().is_none();
    }
    assert!(gone);
    assert!(m.what().is_some());
}

#[test]
fn the_code_enters_and_a_second_code_leaves_with_effects_on() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    m.code(Code::Konami, t0 + ms(1_000));
    let kind = entering(&m).expect("the code enters");
    m.observe(&bare(), t0 + ms(1_000) + kind.duration() + secs(10));
    assert_eq!(m.what().map(|(w, _)| w), Some(What::State(State::Rest)));
    let at = t0 + secs(60);
    m.code(Code::Konami, at);
    m.observe(&bare(), at + Trick::Hop.duration());
    assert!(leaving(&m).is_some(), "the dismissal leaves by an exit");
    m.observe(&bare(), at + secs(30));
    assert!(m.what().is_none());
    assert!(!m.called());
}

#[test]
fn a_dismissal_stops_the_zaps() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    m.observe(&bare(), t0 + HOUR);
    m.code(Code::Konami, t0 + HOUR + secs(20));
    for step in 0..60 {
        m.observe(&bare(), t0 + HOUR + secs(30) + HALF * step / 20);
    }
    assert!(m.what().is_none());
}

#[test]
fn the_pick_varies_and_never_repeats_twice_in_a_row() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    let mut kinds = Vec::new();
    for round in 0..80u64 {
        let at = t0 + secs(20 * round + 10);
        m.code(Code::Konami, at);
        kinds.extend(entering(&m));
        m.observe(&bare(), at + secs(5));
        let at = at + secs(10);
        m.code(Code::Konami, at);
        m.observe(&bare(), at + secs(5));
    }
    assert_eq!(kinds.len(), 80);
    assert!(kinds.windows(2).all(|w| w[0] != w[1]));
    let seen: BTreeSet<_> = kinds.iter().map(|k| format!("{k:?}")).collect();
    assert_eq!(seen.len(), Entrance::ALL.len(), "{seen:?}");
}

#[test]
fn a_costume_favours_its_own_entrance() {
    let t0 = Instant::now();
    let tally = |today: Option<(u8, u8)>, palette: &str| {
        let mut m = with_effects(t0, true);
        m.today = today;
        m.palette = palette.into();
        let mut counts = std::collections::BTreeMap::new();
        for round in 0..300u64 {
            let at = t0 + secs(10 * round + 1);
            m.code(Code::Konami, at);
            if let Some(kind) = entering(&m) {
                *counts.entry(kind).or_insert(0u32) += 1;
            }
            m.observe(&bare(), at + secs(3));
            m.code(Code::Konami, at + secs(5));
            m.observe(&bare(), at + secs(8));
        }
        counts
    };
    let pirate = tally(Some((9, 19)), "amber");
    let most = pirate.iter().max_by_key(|(_, n)| **n).unwrap().0;
    assert_eq!(*most, Entrance::Longship);
    let halloween = tally(Some((10, 31)), "amber");
    assert_eq!(
        *halloween.iter().max_by_key(|(_, n)| **n).unwrap().0,
        Entrance::Lightning
    );
    // Every entrance still comes in a themed costume.
    assert_eq!(pirate.len(), Entrance::ALL.len());
}

#[test]
fn the_animation_asks_for_frames_only_while_it_runs() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    m.observe(&bare(), t0 + HOUR);
    assert!(m.animating());
    m.observe(&bare(), t0 + HOUR + secs(60));
    assert!(!m.animating());
}

/// A squirrel the hour called out and that has landed, and the instant it landed.
fn landed(t0: Instant) -> (Mascot, Instant) {
    let mut m = with_effects(t0, true);
    m.observe(&bare(), t0 + HOUR);
    let at = t0 + HOUR + entering(&m).unwrap().duration() + ms(1);
    m.observe(&bare(), at);
    (m, at)
}

/// The instant the zap goes out and the entrance it will come back by, for a squirrel landed at
/// `at`: walks the loop's own pace until the exit begins.
fn zap_begins(m: &mut Mascot, at: Instant) -> Instant {
    let mut now = at;
    while leaving(m).is_none() {
        now += ms(50);
        m.observe(&bare(), now);
        assert!(now < at + HALF + secs(5), "no zap");
    }
    now
}

#[test]
fn a_fault_visitor_leaves_by_its_exit_when_the_fault_clears() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    let faulted = View {
        fault: true,
        ..bare()
    };
    m.observe(&faulted, t0 + ms(100));
    let kind = entering(&m).expect("it enters");
    m.observe(&faulted, t0 + ms(100) + kind.duration() + secs(5));
    assert_eq!(m.what().map(|(w, _)| w), Some(What::State(State::Pointing)));
    let clear = t0 + secs(20);
    m.observe(&bare(), clear);
    assert_eq!(leaving(&m), Some(kind), "it leaves by the matching exit");
    m.observe(&bare(), clear + kind.duration() + ms(50));
    assert!(m.what().is_none());
}

#[test]
fn an_event_in_the_zap_gap_joins_the_zaps_entrance() {
    let t0 = Instant::now();
    let (mut m, at) = landed(t0);
    let base = View {
        input: Some(1),
        ..bare()
    };
    m.observe(&base, at + ms(10));
    // The zap comes back by a short drop; an event's own pick would be the longer longship.
    m.forced = Some(Entrance::Drop);
    let mut now = at + ms(10);
    while leaving(&m).is_none() {
        now += ms(50);
        m.observe(&base, now);
    }
    m.forced = Some(Entrance::Longship);
    while m.what().is_some() {
        now += ms(50);
        m.observe(&base, now);
    }
    // The change falls at the end of the gap, with the squirrel still away.
    for _ in 0..24 {
        now += ms(50);
        m.observe(&base, now);
    }
    assert!(m.what().is_none());
    now += ms(50);
    m.observe(
        &View {
            input: Some(3),
            ..bare()
        },
        now,
    );
    let mut starts = 0;
    let mut last: Option<f64> = None;
    let mut seen = false;
    for _ in 0..200 {
        now += ms(50);
        m.observe(
            &View {
                input: Some(3),
                ..bare()
            },
            now,
        );
        let progress = m
            .showing()
            .and_then(|s| s.fx)
            .filter(|fx| !fx.out)
            .map(|fx| fx.progress);
        // A new entrance starts from the beginning again, after one that was further along.
        if let (Some(p), Some(before)) = (progress, last)
            && p + 0.2 < before
        {
            starts += 1;
        }
        starts += usize::from(progress.is_some() && last.is_none() && !seen);
        seen |= progress.is_some();
        last = progress;
    }
    assert!(starts <= 1, "{starts} entrances");
}

#[test]
fn a_box_opening_holds_a_zap_under_way_and_the_squirrel_enters_on_release() {
    let t0 = Instant::now();
    let (mut m, at) = landed(t0);
    let began = zap_begins(&mut m, at);
    let held = View {
        keys_held: true,
        ..bare()
    };
    let mid = m.showing().unwrap().fx.unwrap().progress;
    let mut now = began;
    for _ in 0..400 {
        now += ms(50);
        m.observe(&held, now);
    }
    let frozen = m.showing().and_then(|s| s.fx).map(|fx| fx.progress);
    assert!(
        frozen.is_some_and(|p| p <= mid + 0.1),
        "the exit ran under the box: {frozen:?}"
    );
    // Released, the zap carries on to the end and back in.
    let mut entered = false;
    for _ in 0..400 {
        now += ms(50);
        m.observe(&bare(), now);
        entered |= entering(&m).is_some();
    }
    assert!(entered && m.what().is_some());
}

#[test]
fn a_box_opening_holds_the_gap_so_the_entrance_waits() {
    let t0 = Instant::now();
    let (mut m, at) = landed(t0);
    let mut now = zap_begins(&mut m, at);
    while m.what().is_some() {
        now += ms(50);
        m.observe(&bare(), now);
    }
    let held = View {
        keys_held: true,
        ..bare()
    };
    for _ in 0..400 {
        now += ms(50);
        m.observe(&held, now);
        assert!(m.what().is_none(), "the squirrel came back under the box");
    }
    now += ms(50);
    m.observe(&bare(), now);
    let mut came = false;
    for _ in 0..80 {
        now += ms(50);
        m.observe(&bare(), now);
        came |= entering(&m).is_some_and(|_| m.showing().unwrap().fx.unwrap().progress < 0.9);
    }
    assert!(came);
}

#[test]
fn an_entrance_under_way_holds_while_a_box_has_the_keys() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    m.observe(&bare(), t0 + HOUR);
    m.observe(&bare(), t0 + HOUR + ms(200));
    let before = m.showing().unwrap().fx.unwrap().progress;
    let held = View {
        keys_held: true,
        ..bare()
    };
    for step in 1..100 {
        m.observe(&held, t0 + HOUR + ms(200) + ms(100 * step));
    }
    let after = m.showing().unwrap().fx.unwrap().progress;
    assert!((after - before).abs() < 0.01, "{before} -> {after}");
    m.observe(&bare(), t0 + HOUR + ms(200) + ms(100 * 99) + ms(50));
    assert!(m.showing().unwrap().fx.is_some_and(|fx| fx.progress < 0.5));
}

#[test]
fn a_stopped_session_skips_the_zap_that_fell_due() {
    let t0 = Instant::now();
    let (mut m, at) = landed(t0);
    walk(&mut m, &bare(), at, at + HALF - secs(10));
    let late = at + HALF + secs(180);
    m.observe(&bare(), late);
    assert!(m.showing().unwrap().fx.is_none(), "no zap made up");
    walk(&mut m, &bare(), late, late + HALF - secs(20));
    assert!(m.showing().unwrap().fx.is_none());
    walk(
        &mut m,
        &bare(),
        late + HALF - secs(20),
        late + HALF - secs(1),
    );
    m.observe(&bare(), late + HALF + ms(100));
    assert!(
        leaving(&m).is_some(),
        "the next zap comes half an hour after the skip"
    );
}

#[test]
fn the_hours_words_survive_an_entrance_already_running() {
    let t0 = Instant::now();
    let mut m = with_effects(t0, true);
    let mut v = View {
        input: Some(1),
        ..bare()
    };
    m.observe(&v, t0 + secs(3599));
    v.input = Some(2);
    m.observe(&v, t0 + secs(3599) + ms(500));
    let toast = m.observe(&v, t0 + HOUR + ms(100));
    assert_eq!(toast.as_deref(), Some("1 h in. Ratatoskr stretches."));
}

#[test]
fn the_ship_tricks_stay_when_no_entrance_plays() {
    let t0 = Instant::now();
    let (mut m, at) = landed(t0);
    let up = at + secs(5);
    let lost = View {
        link: Link::Reconnecting,
        ..bare()
    };
    m.observe(&lost, up);
    m.observe(&lost, up + secs(2));
    m.observe(&bare(), up + secs(3));
    assert_eq!(m.what().map(|(w, _)| w), Some(What::Trick(Trick::RowIn)));
}
