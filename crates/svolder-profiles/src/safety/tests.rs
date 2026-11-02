// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use super::*;
use crate::test_support::control;

/// A device reading raw `reading`, nothing sent since, no window and no request.
fn at(reading: i64) -> Situation {
    Situation {
        reading: Some(reading),
        base: Some(reading),
        floor_db: None,
        asked_db: None,
    }
}

/// A device that has not reported the control.
fn unread() -> Situation {
    Situation {
        reading: None,
        base: None,
        floor_db: None,
        asked_db: None,
    }
}

/// The gungnir's volume in dB at raw `raw`.
fn db(raw: i64) -> f64 {
    convert::raw_to_display(&control("gungnir2-v2", "volume"), raw).expect("a step")
}

/// The gungnir's volume step nearest `db`.
fn raw(db: f64) -> i64 {
    convert::display_to_raw(&control("gungnir2-v2", "volume"), db).expect("a step")
}

#[test]
fn three_quarters_is_the_raw_range_rounded_down() {
    assert_eq!(safe_step((0, 127)), 95);
    assert_eq!(safe_step((0, 63)), 47);
    assert_eq!(safe_step((1, 127)), 95);
    assert!(lands_at_once((0, 127), None, 95));
    assert!(!lands_at_once((0, 127), None, 96));
    assert!(lands_at_once((0, 127), Some(110), 110), "at the base");
    assert!(!lands_at_once((0, 127), Some(110), 111));
}

#[test]
fn a_raise_above_three_quarters_glides() {
    let volume = control("gungnir2-v2", "volume");
    let tui = Rules::tui();
    assert_eq!(
        judge(&volume, &at(63), 127, &tui).motion,
        Motion::Glide {
            first: Some(95),
            target: 127
        }
    );
    assert_eq!(
        judge(&volume, &at(100), 127, &tui).motion,
        Motion::Glide {
            first: None,
            target: 127
        }
    );
    assert_eq!(judge(&volume, &at(63), 95, &tui).motion, Motion::Now(95));
    assert_eq!(
        judge(&volume, &at(120), 110, &tui).motion,
        Motion::Now(110),
        "a lowering"
    );
}

#[test]
fn a_target_at_or_below_the_base_lands_at_once_even_above_three_quarters() {
    let volume = control("gungnir2-v2", "volume");
    // A lowering to 110 went out and its echo was lost: the device still reads 120.
    let lowered = Situation {
        base: Some(110),
        ..at(120)
    };
    assert_eq!(
        judge(&volume, &lowered, 110, &Rules::tui()).motion,
        Motion::Now(110)
    );
    assert_eq!(
        judge(&volume, &lowered, 115, &Rules::tui()).motion,
        Motion::Glide {
            first: None,
            target: 115
        }
    );
}

#[test]
fn only_the_volume_glides() {
    let balance = control("gungnir2-v2", "balance");
    assert_eq!(
        judge(&balance, &at(64), 127, &Rules::tui()).motion,
        Motion::Now(127)
    );
    let band = control("lokimax", "band1");
    assert_eq!(
        judge(&band, &at(0), 30, &Rules::mqtt(None)).motion,
        Motion::Now(30)
    );
}

#[test]
fn an_unread_volume_is_left_alone_by_the_tui_and_written_safe_first_by_the_daemon() {
    let volume = control("gungnir2-v2", "volume");
    assert_eq!(
        judge(&volume, &unread(), 127, &Rules::tui()).motion,
        Motion::Nothing
    );
    assert_eq!(
        judge(&volume, &unread(), 60, &Rules::tui()).motion,
        Motion::Nothing
    );
    assert_eq!(
        judge(&volume, &unread(), 127, &Rules::writer()).motion,
        Motion::Glide {
            first: Some(95),
            target: 127
        }
    );
    assert_eq!(
        judge(&volume, &unread(), 60, &Rules::writer()).motion,
        Motion::Now(60)
    );
}

#[test]
fn the_cap_applies_before_the_glide() {
    let volume = control("gungnir2-v2", "volume");
    let asked = Situation {
        asked_db: Some(0.0),
        ..at(63)
    };
    let judged = judge(&volume, &asked, 127, &Rules::mqtt(Some(-5.0)));
    let capped = roles::capped_step(&volume, -5.0).expect("a step under -5 dB");
    assert_eq!(judged.target, capped);
    assert_eq!(
        judged.capped,
        Some(Capped {
            cap_db: -5.0,
            refused: false,
            asked_db: Some(0.0),
            quietest: false
        })
    );
    assert_eq!(
        judged.motion,
        Motion::Glide {
            first: Some(95),
            target: capped
        }
    );
}

#[test]
fn mqtt_clamps_to_its_cap_a_lowering_from_above_included() {
    let volume = control("gungnir2-v2", "volume");
    // The knob left the device at -5 dB; -6 dB under a cap of -10 dB lands at the cap.
    let knob = Situation {
        asked_db: Some(-6.0),
        ..at(raw(-5.0))
    };
    let judged = judge(&volume, &knob, raw(-6.0), &Rules::mqtt(Some(-10.0)));
    assert_eq!(judged.target, 97);
    assert!(judged.capped.is_some_and(|capped| !capped.refused));
    assert_eq!(judged.motion, Motion::Now(97));
    // A request above the cap whose nearest step is under it is capped all the same.
    let near = Situation {
        asked_db: Some(-9.9),
        ..at(63)
    };
    assert!(
        judge(&volume, &near, raw(-9.9), &Rules::mqtt(Some(-10.0)))
            .capped
            .is_some()
    );
}

#[test]
fn mcp_refuses_a_rise_past_its_cap_and_lets_a_lowering_through() {
    let volume = control("gungnir2-v2", "volume");
    let rules = Rules::mcp(Some(-10.0));
    let rise = judge(&volume, &at(63), 120, &rules);
    assert_eq!(rise.capped.map(|capped| capped.refused), Some(true));
    assert_eq!(rise.motion, Motion::Nothing);
    let above = raw(-5.0);
    let lowering = judge(&volume, &at(above), above - 3, &rules);
    assert_eq!(lowering.capped, None);
    assert_eq!(lowering.motion, Motion::Now(above - 3));
    // A request under the cap whose step lands above it is refused too.
    let snapped = Situation {
        asked_db: Some(-10.1),
        ..at(63)
    };
    assert!(
        judge(&volume, &snapped, 98, &rules)
            .capped
            .is_some_and(|capped| capped.refused)
    );
}

#[test]
fn a_rise_of_more_than_six_db_asks_and_silence_counts_as_the_largest() {
    let rules = Rules::mcp(None);
    let volume = control("gungnir2-v2", "volume");
    let within = (61..=127)
        .filter(|r| db(*r) - db(60) <= MAX_RISE_DB)
        .max()
        .expect("a step");
    assert_eq!(judge(&volume, &at(60), within, &rules).question, None);
    let rise = judge(&volume, &at(60), within + 1, &rules)
        .question
        .expect("a question");
    assert_eq!(rise.from_db, Some(db(60)));
    assert_eq!(rise.to_db, db(within + 1));
    assert!(
        judge(&volume, &at(0), 5, &rules).question.is_some(),
        "from silence"
    );
    let rise = judge(&volume, &unread(), 5, &rules)
        .question
        .expect("a question");
    assert_eq!(rise.from_db, None);
    assert_eq!(rise.floor_db, f64::NEG_INFINITY);
}

#[test]
fn the_question_is_measured_from_the_lowest_level_of_the_window() {
    let rules = Rules::mcp(None);
    let volume = control("gungnir2-v2", "volume");
    let from = 80;
    let to = (81..=127)
        .find(|r| db(*r) - db(from) >= 4.0)
        .expect("a step");
    assert_eq!(judge(&volume, &at(from), to, &rules).question, None);
    let windowed = Situation {
        floor_db: Some(db(from) - 4.0),
        ..at(from)
    };
    let rise = judge(&volume, &windowed, to, &rules)
        .question
        .expect("a question");
    assert_eq!(rise.floor_db, db(from) - 4.0);
}

#[test]
fn a_lowering_or_the_same_level_never_asks() {
    let rules = Rules::mcp(None);
    let volume = control("gungnir2-v2", "volume");
    assert_eq!(judge(&volume, &at(100), 100, &rules).question, None);
    assert_eq!(judge(&volume, &at(100), 20, &rules).question, None);
}

#[test]
fn the_question_covers_every_level_in_db_and_the_cap_the_volume_alone() {
    let band = control("lokimax", "band1");
    assert!(in_db(&band));
    let judged = judge(&band, &at(0), 30, &Rules::mcp(Some(-10.0)));
    assert!(judged.question.is_some(), "-12 dB to +12 dB");
    assert_eq!(judged.capped, None, "the cap is the volume's");
    let balance = control("gungnir2-v2", "balance");
    assert!(!in_db(&balance));
    assert_eq!(
        judge(&balance, &at(64), 127, &Rules::mcp(Some(-10.0))).question,
        None
    );
}

/// The climb's steps with the device echoing each at once, paid every `tick` until it ends: each
/// step and when it was written.
fn climbed(from: i64, target: i64, tick: Duration) -> Vec<(i64, Duration)> {
    let range = (0, 127);
    let mut climb = Climb::start(target, from);
    let mut reading = from;
    let mut at = Duration::ZERO;
    let mut steps = Vec::new();
    loop {
        match climb.next(Some(reading), Some(reading), false, safe_step(range)) {
            Step::Write(step) => {
                steps.push((step, at));
                reading = step;
                if climb.reached() {
                    return steps;
                }
            }
            Step::Wait => {
                climb.advance(tick, range.1 - range.0);
                at += tick;
            }
            Step::Done | Step::Broken => return steps,
        }
        assert!(
            at < Duration::from_secs(10),
            "the climb never ended: {steps:?}"
        );
    }
}

#[test]
fn a_climb_writes_three_quarters_at_once_then_a_tenth_of_the_range_a_second() {
    let steps = climbed(63, 127, Duration::from_millis(10));
    assert_eq!(steps[0], (95, Duration::ZERO));
    let written: Vec<i64> = steps.iter().map(|(step, _)| *step).collect();
    assert_eq!(written, (95..=127).collect::<Vec<_>>());
    let last = steps.last().expect("a step").1.as_secs_f64();
    assert!((2.4..2.6).contains(&last), "the top quarter took {last} s");
}

#[test]
fn a_reading_that_is_not_the_step_written_breaks_the_climb_and_a_pending_write_waits() {
    let mut climb = Climb::start(127, 100);
    assert_eq!(
        climb.next(Some(100), Some(100), true, 95),
        Step::Wait,
        "unanswered"
    );
    assert_eq!(
        climb.next(Some(90), Some(90), false, 95),
        Step::Broken,
        "turned down"
    );
    assert_eq!(
        climb.next(None, None, false, 95),
        Step::Broken,
        "no reading"
    );
}

#[test]
fn credit_stops_at_a_step_and_a_half() {
    let mut climb = Climb::start(127, 100);
    climb.advance(Duration::from_secs(30), 127);
    assert_eq!(
        climb.next(Some(100), Some(100), false, 95),
        Step::Write(101)
    );
    assert_eq!(
        climb.next(Some(101), Some(101), false, 95),
        Step::Wait,
        "half a step left"
    );
    let mut credit = Credit::NONE;
    credit.advance(Duration::from_millis(50), 127);
    assert!(!credit.spend(), "0.635 of a step");
    credit.advance(Duration::from_millis(50), 127);
    assert!(credit.spend(), "1.27 steps");
}

#[test]
fn steering_moves_the_target_and_passing_it_ends_the_climb() {
    let mut climb = Climb::start(110, 100);
    climb.steer(102);
    climb.advance(Duration::from_secs(1), 127);
    assert_eq!(
        climb.next(Some(100), Some(100), false, 95),
        Step::Write(101)
    );
    climb.advance(Duration::from_secs(1), 127);
    assert_eq!(
        climb.next(Some(101), Some(101), false, 95),
        Step::Write(102)
    );
    assert!(climb.reached());
    climb.advance(Duration::from_secs(1), 127);
    assert_eq!(climb.next(Some(102), Some(102), false, 95), Step::Done);
}

#[test]
fn what_lets_sound_through_is_named_by_its_control() {
    let gain = control("hati", "gain");
    assert_eq!(
        lets_sound_through(&gain, Some(2), Some(1)),
        Some(Through::GainUp)
    );
    assert_eq!(lets_sound_through(&gain, Some(0), Some(1)), None);
    assert_eq!(
        lets_sound_through(&gain, Some(0), None),
        Some(Through::GainUp),
        "unread"
    );
    let mute = control("gungnir2-v2", "mute");
    assert_eq!(
        lets_sound_through(&mute, Some(0), Some(1)),
        Some(Through::Unmute)
    );
    assert_eq!(lets_sound_through(&mute, Some(1), Some(0)), None);
    let power = control("tyr", "power");
    assert_eq!(
        lets_sound_through(&power, Some(1), Some(0)),
        Some(Through::PowerOn)
    );
    assert_eq!(lets_sound_through(&power, Some(0), Some(1)), None);
    let preout = control("lyr5", "preout");
    assert_eq!(
        lets_sound_through(&preout, Some(0), Some(1)),
        Some(Through::AnyChange)
    );
    let input = control("gungnir2-v2", "input");
    assert_eq!(lets_sound_through(&input, Some(3), Some(1)), None);
}

#[test]
fn a_toggle_set_to_a_value_not_known_lets_sound_through() {
    let mute = control("gungnir2-v2", "mute");
    assert_eq!(
        lets_sound_through(&mute, None, Some(1)),
        Some(Through::Unmute)
    );
    let power = control("tyr", "power");
    assert_eq!(
        lets_sound_through(&power, None, Some(0)),
        Some(Through::PowerOn)
    );
}

#[test]
fn a_stall_counts_one_second_of_credit_at_most() {
    let mut credit = Credit::NONE;
    credit.advance(Duration::from_secs(30), 5);
    assert!(
        !credit.spend(),
        "one second of a span of 5 pays half a step"
    );
    credit.advance(Duration::from_secs(1), 5);
    assert!(credit.spend(), "and a second second pays the step");
}

#[test]
fn a_raise_is_judged_from_a_base_no_higher_than_the_reading() {
    let volume = control("gungnir2-v2", "volume");
    // A lowering to -30 dB is in flight while the device still reads -5 dB: -6 dB is a rise
    // from the base, and above the cap.
    let lowering = Situation {
        base: Some(raw(-30.0)),
        asked_db: Some(-6.0),
        ..at(raw(-5.0))
    };
    let judged = judge(&volume, &lowering, raw(-6.0), &Rules::mcp(Some(-10.0)));
    assert_eq!(judged.capped.map(|capped| capped.refused), Some(true));
    assert_eq!(judged.motion, Motion::Nothing);
    // A base above the reading is not trusted: the glide starts from the reading.
    let stale = Situation {
        base: Some(127),
        ..at(96)
    };
    assert_eq!(
        judge(&volume, &stale, 127, &Rules::tui()).motion,
        Motion::Glide {
            first: None,
            target: 127
        }
    );
}

#[test]
fn a_climb_never_steps_from_a_base_above_the_reading() {
    let mut climb = Climb::start(127, 100);
    climb.advance(Duration::from_secs(1), 127);
    assert_eq!(
        climb.next(Some(100), Some(120), false, 95),
        Step::Write(101)
    );
}

/// The web UI judges by MQTT's rules: the cap clamps, nothing is asked, the volume glides, and
/// an unread volume gets at most three quarters first.
#[test]
fn the_web_judges_by_mqtts_rules() {
    for cap in [None, Some(-10.0)] {
        assert_eq!(Rules::web(cap), Rules::mqtt(cap));
    }
    assert_eq!(Rules::web(None).unread, Unread::SafeFirst);
    assert_eq!(Rules::web(Some(-10.0)).over_cap, OverCap::Clamp);
}

#[test]
fn under_a_cap_below_every_step_a_lowering_lands_on_the_quietest_step() {
    let volume = control("freya2", "volume");
    let (quietest, _) = volume.range.expect("a range");
    let rules = Rules::web(Some(-200.0));
    // A lowering from a reading, as the web sends it.
    let lowering = judge(&volume, &at(100), 40, &rules);
    assert_eq!(lowering.target, quietest);
    assert_eq!(lowering.motion, Motion::Now(quietest));
    let capped = lowering.capped.expect("clamped");
    assert!(!capped.refused);
    assert!(capped.quietest, "the words say the cap is below every step");
    // A write with no reading, as MQTT sends it.
    let blind = judge(&volume, &unread(), 40, &rules);
    assert_eq!(blind.target, quietest);
    assert_eq!(blind.motion, Motion::Now(quietest));
    assert!(blind.capped.is_some_and(|c| !c.refused && c.quietest));
    // A reading already at the quietest step: staying there is not a rise.
    let resting = judge(&volume, &at(quietest), quietest, &rules);
    assert_eq!(resting.target, quietest);
    assert!(resting.capped.is_some_and(|c| !c.refused));
}

#[test]
fn under_a_cap_below_every_step_a_raise_is_refused_with_the_caps_words() {
    let volume = control("freya2", "volume");
    let raise = judge(&volume, &at(40), 100, &Rules::web(Some(-200.0)));
    assert_eq!(raise.motion, Motion::Nothing);
    let capped = raise.capped.expect("refused");
    assert!(capped.refused);
    assert!(!capped.quietest, "nothing landed on the quietest step");
    assert_eq!(capped.cap_db, -200.0);
    // A cap with a step under it still lands on that step, not the quietest.
    let normal = judge(&volume, &at(100), 120, &Rules::web(Some(-20.0)));
    assert!(normal.capped.is_some_and(|c| !c.refused && !c.quietest));
}

#[test]
fn a_step_with_no_value_counts_as_over_the_cap() {
    // The table has 128 entries; raw steps past them have no value.
    let mut volume = control("gungnir2-v2", "volume");
    let (lo, _) = volume.range.expect("a range");
    volume.range = Some((lo, 255));
    assert_eq!(roles::step_value(&volume, 200), None);
    let judged = judge(&volume, &at(40), 200, &Rules::web(Some(-10.0)));
    let step = roles::capped_step(&volume, -10.0).expect("a step under -10 dB");
    assert_eq!(judged.target, step);
    assert!(judged.capped.is_some());
}
