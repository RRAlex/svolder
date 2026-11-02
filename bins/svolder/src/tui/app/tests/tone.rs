// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone control: band steps, flat mode, mode and loudness rows, edits that switch the mode,
//! and their clicks.

use super::*;

/// `f` puts the tone control in its flat mode — the device's bypass, not a gain of 0 dB on
/// every band — from any pane of the device screen and from the tone-control screen, and
/// the mode row is the one the shimmer answers. Each press here starts from nothing asked
/// yet (`app.readings.pending` cleared), since `f` toggles ([`f_toggles_flat_and_back`]): asking
/// twice in a row from the same standing state is a second press, covered there.
#[test]
fn f_sets_the_tone_control_flat_from_the_device_and_the_tone_screens() {
    let mut app = app();
    let mode = app.profile.eq.as_ref().map(EqSpec::mode_id);
    for pane in [Pane::Controls, Pane::Tone, Pane::Status] {
        app.pane = pane;
        app.frame_flags.touched_row = None;
        app.readings.pending.clear();
        assert_eq!(
            ch(&mut app, 'f'),
            vec![Intent::EqMode("flat".into())],
            "{pane:?}"
        );
        assert_eq!(
            app.frame_flags.touched_row, mode,
            "{pane:?}: no shimmer for the mode"
        );
    }
    ch(&mut app, 'e');
    for row in [ToneRow::Mode, ToneRow::Loudness, ToneRow::Band] {
        app.tone.row = row;
        app.readings.pending.clear();
        assert_eq!(
            ch(&mut app, 'f'),
            vec![Intent::EqMode("flat".into())],
            "{row:?}"
        );
    }
    for screen in ['t', 'd', 's'] {
        ch(&mut app, screen);
        assert!(
            ch(&mut app, 'f').is_empty(),
            "f wrote on the {screen} screen"
        );
    }
}

/// `f` toggles: from EQ or Loudness it asks for Flat, and pressed again — even before the
/// device has answered the first press — it asks for the mode before: the last
/// non-Flat mode this device session noted, or EQ with none noted. A device in Flat from
/// the start (or after a restart, which is the same to a fresh `App`) has noted none, so
/// `f` there asks for EQ.
#[test]
fn f_toggles_flat_and_back() {
    fn mode_id(app: &App) -> FunctionId {
        app.profile.eq.as_ref().map(EqSpec::mode_id).unwrap()
    }

    // EQ, read from the device -> f -> Flat -> f -> EQ, without waiting for either answer.
    {
        let mut app = app();
        let id = mode_id(&app).to_string();
        hear(&mut app, raw_vo(&id, "u8", EqMode::Eq as i64));
        assert_eq!(ch(&mut app, 'f'), vec![Intent::EqMode("flat".into())]);
        assert_eq!(ch(&mut app, 'f'), vec![Intent::EqMode("eq".into())]);
    }

    // Loudness, read from the device -> f -> Flat -> f -> Loudness.
    {
        let mut app = app();
        let id = mode_id(&app).to_string();
        hear(&mut app, raw_vo(&id, "u8", EqMode::Loudness as i64));
        assert_eq!(ch(&mut app, 'f'), vec![Intent::EqMode("flat".into())]);
        assert_eq!(ch(&mut app, 'f'), vec![Intent::EqMode("loudness".into())]);
    }

    // Flat from the start, nothing else ever read or asked for -> f -> EQ.
    {
        let mut app = app();
        let id = mode_id(&app).to_string();
        hear(&mut app, raw_vo(&id, "u8", EqMode::Flat as i64));
        assert_eq!(ch(&mut app, 'f'), vec![Intent::EqMode("eq".into())]);
    }
}

/// A Mode-select step from EQ to Flat counts as `f` asking for Flat too, so a plain `f`
/// pressed right after comes back to EQ rather than asking for Flat over again.
#[test]
fn a_mode_select_step_to_flat_then_f_returns_to_the_mode_before() {
    let mut app = app();
    let mode_id = app.profile.eq.as_ref().map(EqSpec::mode_id).unwrap();
    hear(
        &mut app,
        raw_vo(&mode_id.to_string(), "u8", EqMode::Eq as i64),
    );
    ch(&mut app, 'e');
    app.tone.row = ToneRow::Mode;
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::EqMode("flat".into())],
        "the mode-select step did not ask for Flat"
    );
    assert_eq!(ch(&mut app, 'f'), vec![Intent::EqMode("eq".into())]);
}

/// A click on the Mode half does what `f` does — Flat, or back to the mode before — and
/// focuses the Mode row, noting the row the cursor left for its trail.
#[test]
fn a_click_on_mode_does_what_f_does_and_focuses_it() {
    let mut app = app();
    app.pane = Pane::Tone;
    app.tone.row = ToneRow::Band;
    app.frame_flags.left_row = None;
    assert_eq!(
        app.click_tone_row(ToneRow::Mode),
        vec![Intent::EqMode("flat".into())]
    );
    assert_eq!(app.tone.row, ToneRow::Mode);
    assert!(
        app.frame_flags.left_row.is_some(),
        "the band it left did not trail"
    );

    assert_eq!(
        app.click_tone_row(ToneRow::Mode),
        vec![Intent::EqMode("eq".into())],
        "a second click did not toggle back to the mode before"
    );
}

/// A click on the Loudness half toggles the loudness mode: from EQ it asks for Loudness,
/// remembering EQ as the mode to come back to, and a second click comes back to it; from
/// Flat it remembers Flat instead.
#[test]
fn a_click_on_loudness_toggles_it_and_remembers_the_mode_it_left() {
    {
        let mut app = app();
        let mode_id = app.profile.eq.as_ref().map(EqSpec::mode_id).unwrap();
        hear(
            &mut app,
            raw_vo(&mode_id.to_string(), "u8", EqMode::Eq as i64),
        );
        assert_eq!(
            app.click_tone_row(ToneRow::Loudness),
            vec![Intent::EqMode("loudness".into())]
        );
        assert_eq!(app.tone.row, ToneRow::Loudness);
        assert_eq!(
            app.click_tone_row(ToneRow::Loudness),
            vec![Intent::EqMode("eq".into())],
            "the second click did not come back to EQ"
        );
    }
    {
        let mut app = app();
        let mode_id = app.profile.eq.as_ref().map(EqSpec::mode_id).unwrap();
        hear(
            &mut app,
            raw_vo(&mode_id.to_string(), "u8", EqMode::Flat as i64),
        );
        assert_eq!(
            app.click_tone_row(ToneRow::Loudness),
            vec![Intent::EqMode("loudness".into())]
        );
        assert_eq!(
            app.click_tone_row(ToneRow::Loudness),
            vec![Intent::EqMode("flat".into())],
            "from Flat, off did not come back to Flat"
        );
    }
}

/// A click on the Mode select's `‹` or `›` steps it exactly as `←→` do on the focused Mode
/// row, and moves the cursor nowhere — unlike a click anywhere else on the half.
/// On the device screen a click on one of the Tone pane's rows — Mode, Loudness or a band —
/// takes the keys to that pane, earning the light that runs round it; on the tone-control
/// screen, the Tone pane written large, the pane is left as it was.
#[test]
fn a_click_on_a_tone_pane_row_of_the_device_screen_focuses_the_pane() {
    for click in [
        |app: &mut App| {
            app.click_tone_row(ToneRow::Mode);
        },
        |app: &mut App| {
            app.click_tone_row(ToneRow::Loudness);
        },
        |app: &mut App| app.focus_band(2),
    ] {
        let mut app = app_on("gungnir2-v2");
        assert_eq!(app.pane, Pane::Controls);
        click(&mut app);
        assert_eq!(app.pane, Pane::Tone);
        assert!(
            app.frame_flags.pane_changed,
            "the pane landed in earned no light"
        );

        let mut app = app_on("gungnir2-v2");
        app.show(Screen::Tone);
        app.frame_flags.pane_changed = false;
        click(&mut app);
        assert_eq!(app.pane, Pane::Controls, "the tone screen moved the pane");
        assert!(!app.frame_flags.pane_changed);
    }
}

#[test]
fn a_click_on_the_mode_arrows_steps_the_select_without_moving_the_cursor() {
    let mut app = app();
    app.pane = Pane::Tone;
    app.tone.row = ToneRow::Band;
    app.frame_flags.left_row = None;
    assert_eq!(
        app.step_tone_mode(1),
        vec![Intent::EqMode("flat".into())],
        "with no reading the first press names a known mode"
    );
    assert_eq!(app.tone.row, ToneRow::Band, "the arrows moved the cursor");
    assert_eq!(app.frame_flags.left_row, None, "the arrows left a trail");

    let mode_id = app.profile.eq.as_ref().map(EqSpec::mode_id).unwrap();
    hear(
        &mut app,
        raw_vo(&mode_id.to_string(), "u8", EqMode::Eq as i64),
    );
    assert_eq!(app.step_tone_mode(-1), vec![Intent::EqMode("flat".into())]);
}

/// The tone-control screen's rows are the tone control's own: the arrows change the one the
/// cursor is on, and the keys of this screen's own reach a band's other two values.
#[test]
fn the_tone_screens_keys_change_the_row_the_cursor_is_on() {
    let mut app = app();
    ch(&mut app, 'e');
    let now = Instant::now();
    // Band 1 as the profile has it: raw 64 is 40 Hz, raw 98 is 0 dB, raw 64 is Q 4.3.
    feed(&mut app, raw_vo("12.1.2", "u8", 64), now);
    feed(&mut app, raw_vo("12.1.3", "u8", 98), now);
    feed(&mut app, raw_vo("12.1.1", "u8", 64), now);
    let semitone = 2f64.powf(1.0 / 12.0);
    assert_eq!(app.tone.row, ToneRow::Band, "the screen opens on its bands");
    // Each press is answered before the next, so each steps from the reading.
    assert_eq_set(&ch(&mut app, '.'), 1, "f", 40.0 * semitone);
    answered(&mut app);
    assert_eq_set(&ch(&mut app, ','), 1, "f", 40.0 / semitone);
    answered(&mut app);
    // One raw step either side of 98 (0 dB): 99 is +0.207 dB and 97 is -0.204 dB.
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", 6.0 / 29.0);
    answered(&mut app);
    assert_eq_set(&press(&mut app, KeyCode::Left), 1, "g", -20.0 / 98.0);
    answered(&mut app);
    assert_eq_set(&ch(&mut app, ']'), 1, "q", 4.3 * 1.1);
    answered(&mut app);
    assert_eq_set(&ch(&mut app, '['), 1, "q", 4.3 / 1.1);
    ch(&mut app, '2');
    assert_eq!((app.tone.row, app.tone.band), (ToneRow::Band, 1));
    assert!(
        press(&mut app, KeyCode::Right).is_empty(),
        "band 2 has no reading to step from"
    );
}

/// `←→` on a band move its gain exactly one of the device's own steps: from -0.20 dB (raw
/// 97) one press up is 0 dB, the step a round half decibel skipped, and one down from there
/// is -0.20 dB again. At either end of the gain's steps the arrow asks for nothing, and the
/// device screen's Tone pane steps a band the same way.
#[test]
fn one_arrow_moves_a_bands_gain_one_step() {
    let mut app = app();
    ch(&mut app, 'e');
    let now = Instant::now();
    feed(&mut app, raw_vo("12.1.3", "u8", 97), now);
    // 0 dB is raw 98 itself, so the press lands on it exactly.
    let zero = vec![Intent::EqSet {
        band: 1,
        param: "g".into(),
        value: 0.0,
    }];
    assert_eq!(press(&mut app, KeyCode::Right), zero);
    feed(&mut app, raw_vo("12.1.3", "u8", 98), now);
    assert_eq_set(&press(&mut app, KeyCode::Left), 1, "g", -20.0 / 98.0);
    feed(&mut app, raw_vo("12.1.3", "u8", 127), now);
    assert!(press(&mut app, KeyCode::Right).is_empty(), "127 is the top");
    assert_eq_set(&press(&mut app, KeyCode::Left), 1, "g", 6.0 - 6.0 / 29.0);
    feed(&mut app, raw_vo("12.1.3", "u8", 0), now);
    assert!(press(&mut app, KeyCode::Left).is_empty(), "0 is the bottom");
    assert_eq_set(
        &press(&mut app, KeyCode::Right),
        1,
        "g",
        -20.0 + 20.0 / 98.0,
    );

    let mut device = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![row("gungnir")],
    );
    feed(&mut device, raw_vo("12.1.3", "u8", 97), now);
    device.pane = Pane::Tone;
    assert_eq!(device.tone.row, ToneRow::Band);
    assert_eq!(press(&mut device, KeyCode::Right), zero);
}

/// Two presses before the device has answered the first move two steps: each band key
/// steps from the value last asked for while a write of it is unanswered, so a key held
/// down moves at its own rate. An echo of an earlier step keeps the one asked for last; the
/// echo of the last write, or of the step asked for, ends the wait, and the next press
/// steps from the reading again. A refresh and a failed write end it too. The wheel's path
/// and the frequency keys step the same way.
#[test]
fn two_presses_before_the_echo_move_two_steps() {
    let mut app = app();
    ch(&mut app, 'e');
    let now = Instant::now();
    feed(&mut app, raw_vo("12.1.2", "u8", 64), now);
    feed(&mut app, raw_vo("12.1.3", "u8", 98), now);
    let eq = app.profile.eq.clone().expect("a tone control");
    let step = |raw: u8| eq.gain_db(raw);
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(99));
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(100));
    feed(&mut app, raw_vo("12.1.3", "u8", 99), now);
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(101));
    feed(&mut app, raw_vo("12.1.3", "u8", 100), now);
    feed(&mut app, raw_vo("12.1.3", "u8", 101), now);
    assert!(
        app.readings.pending.is_empty(),
        "the last echo left {:?}",
        app.readings.pending
    );
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(102));

    // A device that answers only the last write ends the wait with it.
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(103));
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(104));
    feed(&mut app, raw_vo("12.1.3", "u8", 104), now);
    assert!(app.readings.pending.is_empty());

    // A device that settles somewhere else ends it once every write has been answered.
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(105));
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(106));
    feed(&mut app, raw_vo("12.1.3", "u8", 105), now);
    feed(&mut app, raw_vo("12.1.3", "u8", 110), now);
    assert!(
        app.readings.pending.is_empty(),
        "{:?}",
        app.readings.pending
    );
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(111));

    // A refresh reads the device afresh, with writes still unanswered.
    press(&mut app, KeyCode::Right);
    app.replace_all(vec![raw_vo("12.1.3", "u8", 101)], now);
    assert!(
        app.readings.pending.is_empty(),
        "{:?}",
        app.readings.pending
    );
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(102));
    // So does a failed write.
    app.write_failed(&Failure {
        code: 3,
        what: "the device is not answering".into(),
        next: "try again".into(),
    });
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(102));

    // The wheel's writes are finished by the same band edit.
    app.readings.pending.clear();
    let first = app.nudge_band(0, 1);
    app.band_edit(first);
    assert_eq_set(&app.nudge_band(0, 1), 1, "g", step(103));

    // And the frequency keys step from the frequency last asked for.
    app.readings.pending.clear();
    feed(&mut app, raw_vo("12.1.2", "u8", 64), now);
    let semitone = 2f64.powf(1.0 / 12.0);
    let up = ch(&mut app, '.');
    let Intent::EqSet { value, .. } = up[0] else {
        panic!("a frequency write, got {up:?}");
    };
    let asked = eq
        .frequency(0, eq.frequency_raw(0, value).expect("a step"))
        .expect("a band");
    assert_eq_set(&ch(&mut app, '.'), 1, "f", asked * semitone);
}

/// A band step the device never answers gives up once [`ANSWER_LIFETIME`] has passed since
/// the press that asked for it last, and the next press steps from the reading again: a
/// press made in the meantime starts the time again, and a step answered in its time ends
/// the wait as it always did.
#[test]
fn a_band_step_left_unanswered_gives_up_after_its_time() {
    let mut app = app();
    ch(&mut app, 'e');
    let start = Instant::now();
    feed(&mut app, raw_vo("12.1.2", "u8", 64), start);
    feed(&mut app, raw_vo("12.1.3", "u8", 98), start);
    let eq = app.profile.eq.clone().expect("a tone control");
    let step = |raw: u8| eq.gain_db(raw);
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(99));

    let later = start + ANSWER_LIFETIME - Duration::from_millis(1);
    app.tick(later);
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(100));
    app.tick(start + ANSWER_LIFETIME);
    assert!(
        !app.readings.pending.is_empty(),
        "the second press did not start the time again"
    );
    app.tick(later + ANSWER_LIFETIME);
    assert!(
        app.readings.pending.is_empty(),
        "{:?}",
        app.readings.pending
    );
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(99));

    // Answered in its time, the wait ends on the echo and the next press steps from it.
    feed(
        &mut app,
        raw_vo("12.1.3", "u8", 99),
        later + ANSWER_LIFETIME,
    );
    assert!(
        app.readings.pending.is_empty(),
        "{:?}",
        app.readings.pending
    );
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", step(100));
}

/// The value one step sends is the one that step stands for, so on every profile with a
/// tone control the device's own conversion lands it on the neighbouring step and on no
/// other, whatever size the steps are: the whole of the gain's range, one press at a time,
/// up and down.
#[test]
fn every_gain_step_is_one_press_from_its_neighbours_on_every_profile() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let mut checked = 0;
    for name in registry.names() {
        let profile = registry.get(name).expect("a listed profile").clone();
        let Some(eq) = profile.eq.clone() else {
            continue;
        };
        let gain = eq.band_id(0, EqParam::Gain).expect("a first band");
        let mut app = App::new(profile, name.into(), &TuiConfig::default(), vec![row(name)]);
        for raw in 0u8..=127 {
            app.apply(
                Update::Status(raw_vo(&gain.to_string(), "u8", i64::from(raw))),
                Instant::now(),
            );
            for (dir, next) in [(1, raw.checked_add(1)), (-1, raw.checked_sub(1))] {
                let writes = app.nudge_band(0, dir);
                match next.filter(|next| *next <= 127) {
                    Some(next) => match writes.as_slice() {
                        [Intent::EqSet { value, .. }] => assert_eq!(
                            eq.gain_raw(*value),
                            next,
                            "{name}: {dir:+} from raw {raw} sends {value} dB"
                        ),
                        other => panic!("{name}: {dir:+} from raw {raw} asked {other:?}"),
                    },
                    None => assert!(writes.is_empty(), "{name}: {dir:+} past raw {raw}"),
                }
            }
        }
        checked += 1;
    }
    assert!(checked >= 2, "only {checked} profiles have a tone control");
}

/// The mode and the loudness sit above the bands on the tone-control screen too, so the
/// arrows that reach them on the device screen's Tone pane reach them here.
#[test]
fn the_tone_screen_reaches_the_mode_and_loudness_rows_above_band_one() {
    let mut app = app();
    ch(&mut app, 'e');
    let now = Instant::now();
    feed(&mut app, raw_vo("12.50", "u8", 1), now);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.tone.row, ToneRow::Loudness);
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Raw {
            id: "12.52".into(),
            value_type: "u8".into(),
            value: "1".into(),
        }]
    );
    press(&mut app, KeyCode::Up);
    assert_eq!(app.tone.row, ToneRow::Mode);
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::EqMode("eq".into())]
    );
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!((app.tone.row, app.tone.band), (ToneRow::Band, 0));
}

/// `Tab` on the tone screen has two stops, the band table and the header, and either
/// direction between them goes to the other one. Landing on the header puts the cursor
/// back on the half it was last on — Mode the first time — and landing on the band table
/// puts it back on the band it was on; either way it is flagged for the light that runs
/// round the pane the keys moved to, as a pane change is everywhere else.
#[test]
fn tab_on_the_tone_screen_toggles_between_the_bands_and_the_header() {
    let mut app = app();
    ch(&mut app, 'e');
    assert_eq!(app.screen, Screen::Tone);
    app.tone.row = ToneRow::Band;
    app.tone.band = 1;

    app.frame_flags.pane_changed = false;
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        app.tone.row,
        ToneRow::Mode,
        "Tab opens on Mode the first time"
    );
    assert!(app.frame_flags.pane_changed, "Tab did not raise the light");

    app.frame_flags.pane_changed = false;
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        (app.tone.row, app.tone.band),
        (ToneRow::Band, 1),
        "Tab back to the bands remembers band 2"
    );
    assert!(app.frame_flags.pane_changed);

    // Loudness is the half Tab remembers once the cursor has rested there.
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    assert_eq!(
        app.tone.row,
        ToneRow::Loudness,
        "band 2, then band 1, then Loudness"
    );
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        app.tone.row,
        ToneRow::Band,
        "Tab off Loudness reaches the bands"
    );
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        app.tone.row,
        ToneRow::Loudness,
        "Tab back to the header remembers Loudness"
    );

    // Shift-Tab is the same two-stop toggle, not the reverse of a longer cycle.
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.tone.row, ToneRow::Band);
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.tone.row, ToneRow::Loudness);
}

/// The digits pick a band up to as many as the profile has and no further, on a five-band
/// profile and a three-band one alike.
#[test]
fn digits_select_a_band_up_to_the_profiles_own_count() {
    let mut app = app();
    ch(&mut app, 'e');
    ch(&mut app, '3');
    assert_eq!(app.tone.band, 2);
    ch(&mut app, '5');
    assert_eq!(app.tone.band, 4, "the profile's last band");
    ch(&mut app, '6');
    assert_eq!(app.tone.band, 4, "the profile has five bands");
    ch(&mut app, '9');
    assert_eq!(app.tone.band, 4, "the profile has five bands");

    let mimir = Registry::builtin()
        .expect("the built-in profiles load")
        .get("mimir")
        .expect("mimir is built in")
        .clone();
    let mut three = App::new(mimir, "mimir".into(), &TuiConfig::default(), Vec::new());
    ch(&mut three, 'e');
    assert_eq!(three.screen, Screen::Tone);
    ch(&mut three, '3');
    assert_eq!(three.tone.band, 2, "the three-band profile's last band");
    ch(&mut three, '4');
    assert_eq!(three.tone.band, 2, "the profile has three bands");
}

/// `b` on the tone screen asks the device to identify itself and moves nothing: the balance is
/// a row of the device screen, reached there.
#[test]
fn b_identifies_on_the_tone_screen_and_moves_nothing() {
    let mut app = app();
    ch(&mut app, 'e');
    ch(&mut app, '2');
    app.frame_flags.pane_changed = false;
    assert_eq!(ch(&mut app, 'b'), vec![Intent::Identify(None)]);
    assert_eq!(app.screen, Screen::Tone, "b left the tone screen");
    assert_eq!(app.tone.band, 1, "b moved the band cursor");
    assert!(
        !app.frame_flags.pane_changed,
        "b moved the keys to another pane"
    );
}

/// The mode and the loudness are rows of the device screen's Tone pane, reached by moving
/// the pane's cursor off the bands and changed by the arrows that change every other row.
#[test]
fn the_tone_panes_mode_and_loudness_rows_are_stepped_by_the_arrows() {
    let mut app = app();
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Tone);
    assert_eq!(app.tone.row, ToneRow::Band, "the pane opens on its bands");
    press(&mut app, KeyCode::Up);
    assert_eq!(app.tone.row, ToneRow::Loudness);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.tone.row, ToneRow::Mode);

    let now = Instant::now();
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::EqMode("flat".into())],
        "with no reading the first press names a known mode"
    );
    for (raw, next) in [(1, "eq"), (2, "loudness")] {
        feed(&mut app, raw_vo("12.50", "u8", raw), now);
        assert_eq!(
            press(&mut app, KeyCode::Right),
            vec![Intent::EqMode(next.into())]
        );
    }
    feed(&mut app, raw_vo("12.50", "u8", 3), now);
    assert!(
        press(&mut app, KeyCode::Right).is_empty(),
        "loudness is the last mode there is"
    );
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::EqMode("eq".into())],
        "the row steps back as well as on"
    );

    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Loudness);
    let raw_set = |value: &str| Intent::Raw {
        id: "12.52".into(),
        value_type: "u8".into(),
        value: value.into(),
    };
    assert_eq!(press(&mut app, KeyCode::Right), vec![raw_set("1")]);
    feed(&mut app, raw_vo("12.52", "u8", 0), now);
    assert!(
        press(&mut app, KeyCode::Left).is_empty(),
        "0 is the bottom of the loudness range"
    );
    feed(&mut app, raw_vo("12.52", "u8", 127), now);
    assert!(press(&mut app, KeyCode::Right).is_empty(), "127 is the top");
    assert_eq!(press(&mut app, KeyCode::Left), vec![raw_set("126")]);

    // And a band's own row is the gain the wheel over it nudges.
    press(&mut app, KeyCode::Down);
    assert_eq!((app.tone.row, app.tone.band), (ToneRow::Band, 0));
    feed(&mut app, raw_vo("12.1.3", "u8", 98), now);
    assert_eq_set(&press(&mut app, KeyCode::Right), 1, "g", 6.0 / 29.0);
}

/// Two quick presses on the tone control's mode row or loudness row, before the device has
/// echoed the first, step twice: the second reads the value the first asked for.
#[test]
fn two_quick_presses_on_the_mode_and_loudness_rows_step_twice() {
    let mut app = app();
    press(&mut app, KeyCode::Tab);
    let now = Instant::now();
    app.tone.row = ToneRow::Mode;
    feed(&mut app, raw_vo("12.50", "u8", 1), now);
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::EqMode("eq".into())]
    );
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::EqMode("loudness".into())],
        "the second press steps from the mode the first asked for"
    );

    app.tone.row = ToneRow::Loudness;
    let raw_set = |value: &str| Intent::Raw {
        id: "12.52".into(),
        value_type: "u8".into(),
        value: value.into(),
    };
    feed(&mut app, raw_vo("12.52", "u8", 10), now);
    assert_eq!(press(&mut app, KeyCode::Right), vec![raw_set("11")]);
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![raw_set("12")],
        "the second press steps from the strength the first asked for"
    );
}

/// The tone-control screen keeps the band keys and nothing else: `m` is the device's mute
/// there as it is everywhere, and the two keys the loudness had are gone.
#[test]
fn the_tone_screen_leaves_mute_to_the_global_key_and_has_no_loudness_keys() {
    let mut app = app();
    ch(&mut app, 'e');
    assert_eq!(ch(&mut app, 'm'), vec![set("mute", "on")]);
    assert!(ch(&mut app, 'l').is_empty());
    assert!(shift(&mut app, 'L').is_empty());
}

#[test]
fn a_profile_that_names_the_loudness_control_is_set_by_key() {
    let mut profile = profile();
    let eq = profile.eq.clone().expect("the profile has tone control");
    profile.controls.push(Control {
        id: eq.loudness_id(),
        key: "eq_loudness".into(),
        label: "Loudness".into(),
        kind: ControlKind::Level,
        value_type: ValueType::U8,
        values: vec![],
        range: Some((0, 127)),
        center: None,
        unit: None,
        convert: None,
        channel: None,
        floor: None,
        tick: None,
        log_scale: None,
        advice: None,
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    });
    let mut app = App::new(
        profile,
        "gungnir".into(),
        &TuiConfig::default(),
        vec![row("gungnir")],
    );
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.tone.row, ToneRow::Loudness);
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![set("eq_loudness", "1")]
    );
}

/// The switch to EQ every band edit in Flat mode ends on.
fn switch_to_eq() -> Intent {
    Intent::EqMode("eq".into())
}

/// Whether `intents` are one band write, on `band` (1-based) and `param`, and then the
/// switch to EQ.
fn band_then_switch(intents: &[Intent], band: usize, param: &str) -> bool {
    matches!(
        intents,
        [Intent::EqSet { band: b, param: p, .. }, mode]
            if *b == band && p == param && *mode == switch_to_eq()
    )
}

/// Every band edit made while the tone control is in Flat mode — the device's bypass, where
/// no band is heard — writes the band and then puts the tone control in EQ mode, and the key
/// line says so: the gain, frequency and Q keys of the tone-control screen and their
/// Shift+arrow aliases, `0`, and the arrows on a band of the device screen's Tone pane.
#[test]
fn a_band_edit_in_flat_mode_writes_the_band_then_switches_to_eq() {
    use crate::tui::test_support::presets::{eq_of, hear_id, resting};
    let keys: [(KeyEvent, &str); 10] = [
        (KeyEvent::from(KeyCode::Right), "g"),
        (KeyEvent::from(KeyCode::Left), "g"),
        (KeyEvent::from(KeyCode::Char(',')), "f"),
        (KeyEvent::from(KeyCode::Char('.')), "f"),
        (KeyEvent::from(KeyCode::Char('[')), "q"),
        (KeyEvent::from(KeyCode::Char(']')), "q"),
        (KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT), "f"),
        (KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT), "f"),
        (KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT), "q"),
        (KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT), "q"),
    ];
    for (key, param) in keys {
        let mut app = resting("gungnir2-v2", EqMode::Flat);
        let intents = app.key(key);
        assert!(band_then_switch(&intents, 1, param), "{key:?}: {intents:?}");
        assert_eq!(
            app.toast().map(|toast| toast.text.as_str()),
            Some(SWITCHED_TO_EQ),
            "{key:?}"
        );
    }

    // `0`, on a curve with a band off 0 dB: its gain comes back, then the switch.
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    let eq = eq_of(&app);
    let gain = eq.band_id(2, EqParam::Gain).expect("band 3's gain");
    hear_id(&mut app, gain, i64::from(eq.gain_zero) + 10);
    let intents = ch(&mut app, '0');
    assert!(band_then_switch(&intents, 3, "g"), "0: {intents:?}");
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(SWITCHED_TO_EQ)
    );

    // The device screen's Tone pane: the arrows on a band's row.
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    app.show(Screen::Device);
    app.pane = Pane::Tone;
    app.tone.row = ToneRow::Band;
    app.tone.band = 1;
    let intents = press(&mut app, KeyCode::Right);
    assert!(band_then_switch(&intents, 2, "g"), "{intents:?}");
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(SWITCHED_TO_EQ)
    );
}

/// In Loudness mode, in EQ mode, and before the mode has been read, a band edit writes the
/// band alone and says nothing; nor does an edit that writes nothing — `0` on a flat curve
/// — switch anything, in Flat mode or not.
#[test]
fn a_band_edit_outside_flat_mode_writes_the_band_alone() {
    use crate::tui::test_support::presets::{eq_of, resting};
    for mode in [EqMode::Loudness, EqMode::Eq] {
        let mut app = resting("gungnir2-v2", mode);
        for key in [KeyCode::Right, KeyCode::Char('.'), KeyCode::Char(']')] {
            let intents = press(&mut app, key);
            assert!(
                matches!(intents.as_slice(), [Intent::EqSet { band: 1, .. }]),
                "{mode:?} {key:?}: {intents:?}"
            );
        }
        assert_eq!(
            app.toast().map(|toast| toast.text.clone()),
            None,
            "{mode:?}"
        );
    }
    let mut unread = resting("gungnir2-v2", EqMode::Flat);
    let mode = eq_of(&unread).mode_id();
    unread.readings.values.remove(&mode);
    let intents = press(&mut unread, KeyCode::Right);
    assert!(
        matches!(intents.as_slice(), [Intent::EqSet { band: 1, .. }]),
        "no mode read: {intents:?}"
    );

    let mut flat = resting("gungnir2-v2", EqMode::Flat);
    assert!(
        ch(&mut flat, '0').is_empty(),
        "a flat curve wrote something"
    );
    assert_eq!(flat.toast().map(|toast| toast.text.clone()), None);
}

/// A preset's recall writes the mode the preset holds, so loading a Flat-mode preset onto a
/// device in Flat mode writes its bands and no switch to EQ.
#[test]
fn a_recall_in_flat_mode_writes_its_own_mode_and_no_switch() {
    use crate::tui::test_support::presets::{own, resting};
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    let mut kept = own("Kept", 5, 2.0);
    kept.mode = EqMode::Flat;
    app.presets.saved = vec![kept];
    ch(&mut app, 'p');
    for _ in 0..4 {
        press(&mut app, KeyCode::Down);
    }
    let intents = press(&mut app, KeyCode::Enter);
    assert!(
        intents
            .iter()
            .any(|intent| matches!(intent, Intent::EqSet { .. })),
        "the recall wrote no band: {intents:?}"
    );
    assert!(
        !intents
            .iter()
            .any(|intent| matches!(intent, Intent::EqMode(_))),
        "a Flat preset on a device in Flat mode wrote a mode: {intents:?}"
    );
    assert_ne!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(SWITCHED_TO_EQ)
    );
}

/// The peak a clipping warning is about is worked out as the readings arrive: a band lifted
/// in EQ mode gives one, and the volume step it clips past comes from the volume's own
/// attenuation table; the mode going to Flat, the band coming back and a refresh that says
/// nothing of the tone control each take it away.
#[test]
fn the_peak_to_warn_about_follows_the_readings() {
    use crate::tui::test_support::presets::{eq_of, hear_id, resting};
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    assert_eq!(
        (app.clip_peak(), app.clip_step()),
        (None, None),
        "a flat curve"
    );
    let gain = eq.band_id(1, EqParam::Gain).expect("band 2's gain");
    hear_id(&mut app, gain, 127);
    let peak = app.clip_peak().expect("a +6 dB band boosts");
    assert!((peak - 6.0).abs() < 0.01, "peak {peak}");
    // Step 109 is -6.021 dB, the loudest at or under -6.
    assert_eq!(app.clip_step(), Some(109));

    hear_id(&mut app, eq.mode_id(), i64::from(EqMode::Flat as u8));
    assert_eq!(app.clip_peak(), None, "flat mode");
    hear_id(&mut app, eq.mode_id(), i64::from(EqMode::Eq as u8));
    assert!(app.clip_peak().is_some(), "back in EQ mode");
    hear_id(&mut app, gain, i64::from(eq.gain_zero));
    assert_eq!(app.clip_peak(), None, "the band back at 0 dB");
    hear_id(&mut app, gain, 127);
    app.replace_all(vec![volume(100)], Instant::now());
    assert_eq!(
        app.clip_peak(),
        None,
        "a refresh with no tone control in it"
    );
}

/// The band keys at the edge of what the device holds write nothing, for the frequency and the Q.
#[test]
fn the_band_steps_at_the_range_edge_send_no_write() {
    use crate::tui::test_support::presets::{eq_of, hear_id, resting};
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    let freq = eq
        .band_id(0, EqParam::Frequency)
        .expect("band 1's frequency");
    let q = eq.band_id(0, EqParam::Q).expect("band 1's Q");
    hear_id(&mut app, freq, 127);
    hear_id(&mut app, q, 127);
    assert!(ch(&mut app, '.').is_empty(), "frequency past the top");
    assert!(ch(&mut app, ']').is_empty(), "Q past the top");
    hear_id(&mut app, freq, 0);
    hear_id(&mut app, q, 0);
    assert!(ch(&mut app, ',').is_empty(), "frequency past the bottom");
    assert!(ch(&mut app, '[').is_empty(), "Q past the bottom");
    assert!(app.readings.pending.is_empty(), "no pending mark");
}
