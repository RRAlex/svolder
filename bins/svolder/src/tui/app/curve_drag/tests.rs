// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use svolder_ipc::TuiConfig;

use svolder_profiles::tone::preset::SWITCHED_TO_EQ;

use crate::backend::DeviceRow;
use crate::tui::app::{ANSWER_LIFETIME, App, Intent, Screen, ToneRow};
use crate::tui::screens::tone;
use crate::tui::test_support::{answer, eq, hear_one as hear, profile};

use super::*;

/// A Gungnir 2 on the tone-control screen, the tone control in `mode` (1 flat, 2 EQ, 3
/// loudness) and every band flat on its own centre: raw 64 is the centre frequency, 98 is
/// 0 dB and 64 is Q 4.30.
fn tone_app(mode: i64) -> App {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("gungnir")],
    );
    hear(&mut app, "12.50", mode);
    hear(&mut app, "12.52", 0);
    for band in 1..=5 {
        hear(&mut app, &format!("12.{band}.2"), 64);
        hear(&mut app, &format!("12.{band}.3"), 98);
        hear(&mut app, &format!("12.{band}.1"), 64);
    }
    app.show(Screen::Tone);
    app
}

/// The curve's cells on an 80×24 terminal.
fn curve() -> Rect {
    tone::curve_area(Rect::new(0, 0, 80, 24), 5)
}

/// One band write as a drag sends it: `param` of band `band` (1-based) at the value raw
/// step `raw` stands for.
fn write(band: usize, param: EqParam, raw: u8) -> Intent {
    Intent::EqSet {
        band,
        param: param.word().into(),
        value: preset::Write::Band {
            band: band - 1,
            param,
            raw,
        }
        .value(&eq())
        .expect("a step of the band"),
    }
}

/// Where band `band`'s point is drawn on the 80×24 curve.
fn point(app: &App, band: usize) -> (u16, u16) {
    tone::points(app, curve())
        .into_iter()
        .find(|(drawn, _)| *drawn == band)
        .map(|(_, cell)| cell)
        .unwrap_or_else(|| panic!("band {band} has no point"))
}

/// A press on a band's point, or a cell off it either way, grabs that band for either
/// button; a left press two cells off every point grabs nothing, where a right one grabs
/// the band whose centre is nearest in log frequency.
#[test]
fn a_press_near_a_point_grabs_its_band_and_one_away_from_every_point_grabs_none() {
    let app = tone_app(2);
    let (x, y) = point(&app, 2);
    for gesture in [Gesture::Move, Gesture::Width] {
        for at in [(x, y), (x + 1, y), (x - 1, y + 1), (x, y - 1)] {
            assert_eq!(
                grab(&app, curve(), at, gesture),
                Some(2),
                "{gesture:?} at {at:?}"
            );
        }
    }
    // Two rows above band 3's point, on its own column: off every point.
    let off = (x, y - 2);
    assert_eq!(grab(&app, curve(), off, Gesture::Move), None);
    assert_eq!(grab(&app, curve(), off, Gesture::Width), Some(2));
    // A right press far past the last band's centre still grabs the band nearest it.
    let edge = (curve().right() - 1, curve().y);
    assert_eq!(grab(&app, curve(), edge, Gesture::Width), Some(4));
    assert_eq!(grab(&app, curve(), edge, Gesture::Move), None);
}

/// Where two points are within reach of a press, the band the cursor is on wins, whichever
/// is nearer; with the cursor on neither, the nearer one does. Band 2 at the top of its
/// window (312 Hz) and band 3 at the bottom of its own draw one column apart or on the same
/// one; band 3 is a step down so the two are a row apart.
#[test]
fn where_two_points_are_close_the_selected_band_wins_else_the_nearer() {
    let mut app = tone_app(2);
    hear(&mut app, "12.2.2", 127);
    hear(&mut app, "12.3.2", 0);
    hear(&mut app, "12.3.3", 85);
    let (two, three) = (point(&app, 1), point(&app, 2));
    assert!(
        two.0.abs_diff(three.0) <= 1 && two.1.abs_diff(three.1) == 1,
        "the fixture's points are not a row apart: {two:?} {three:?}"
    );
    for (selected, at, wanted) in [
        (0, two, 1),
        (0, three, 2),
        (1, three, 1),
        (2, two, 2),
        (1, two, 1),
    ] {
        app.tone.band = selected;
        assert_eq!(
            grab(&app, curve(), at, Gesture::Move),
            Some(wanted),
            "cursor on band {}, press at {at:?}",
            selected + 1
        );
    }
}

/// A press grabs its band and focuses it, taking the cursor off the header; the drag holds
/// which gesture it is.
#[test]
fn a_grab_focuses_its_band() {
    let mut app = tone_app(2);
    app.tone.row = ToneRow::Mode;
    start(&mut app, 3, Gesture::Width, (40, 6));
    assert_eq!((app.tone.band, app.tone.row), (3, ToneRow::Band));
    assert_eq!(
        app.pointer.curve_drag,
        Some(CurveDrag {
            band: 3,
            gesture: Gesture::Width,
            from: (40, 6),
            start: [Some(64), Some(98), Some(64)],
            column: Some(40),
            row: Some(6),
            held: [None; 3],
            moved: false,
        })
    );
}

/// A left drag the device answers puts the point where the pointer is — the column's
/// frequency and the row's gain, each at the device step nearest what the cell stands for —
/// written as a band edit:
/// in Flat mode with the switch to EQ after the band, said on the key line. A gain going up is
/// written after the frequency, and one coming down, on the way back, before it; the mode is
/// switched once.
#[test]
fn a_left_drag_writes_the_frequency_and_gain_of_the_cell_under_the_pointer() {
    let eq = eq();
    let mut app = tone_app(1);
    let (x, y) = point(&app, 1);
    start(&mut app, 1, Gesture::Move, (x, y));
    let at = (x + 4, y - 2);
    let hz = eq
        .frequency_raw(1, tone::frequency_at(curve(), at.0))
        .expect("a step");
    let db = eq.gain_raw(tone::gain_at(curve(), at.1));
    assert!(hz > 64 && db > eq.gain_zero, "the cell is not up and right");
    assert_eq!(
        to(&mut app, curve(), at),
        vec![
            write(2, EqParam::Frequency, hz),
            write(2, EqParam::Gain, db),
            Intent::EqMode("eq".into()),
        ]
    );
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(SWITCHED_TO_EQ)
    );
    answer(&mut app);
    // Back onto the 0 dB line is 0 dB itself; the column stands for its own centre, which
    // is near the band's own step but not on it: a cell is a coarse step.
    let back = eq
        .frequency_raw(1, tone::frequency_at(curve(), x))
        .expect("a step");
    assert_eq!(
        to(&mut app, curve(), (x, y)),
        vec![
            write(2, EqParam::Gain, eq.gain_zero),
            write(2, EqParam::Frequency, back),
        ]
    );
    answer(&mut app);
    assert!(
        to(&mut app, curve(), (x, y)).is_empty(),
        "a drag to where the band is going wrote again"
    );
}

/// Each axis of a left drag works on its own: along the press's own row the gain stays on
/// the step the press found, and along its own column the frequency does — for a band off
/// 0 dB and off the step its cell stands for on either axis, which a drag taking the whole
/// cell would move — whether the press lands on the band's point or on any of the cells a
/// cell off it that grab it too. Once the pointer has left the row, the rows it reaches set
/// the gain, its own row too.
#[test]
fn a_drag_along_the_press_row_or_column_moves_only_the_other_axis() {
    let eq = eq();
    let (hz, db) = (73, eq.gain_raw(2.48));
    let fixture = || {
        let mut app = tone_app(2);
        hear(&mut app, "12.4.2", i64::from(hz));
        hear(&mut app, "12.4.3", i64::from(db));
        app
    };
    let app = fixture();
    let (x, y) = point(&app, 3);
    assert_ne!(
        eq.frequency_raw(3, tone::frequency_at(curve(), x)),
        Some(hz),
        "the fixture's band sits on its column's own step"
    );
    assert_ne!(
        eq.gain_raw(tone::gain_at(curve(), y)),
        db,
        "the fixture's band sits on its row's own step"
    );
    assert!(
        !tone::on_zero_row(curve(), y),
        "the fixture's band is on 0 dB"
    );

    for (dx, dy) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy))) {
        let press = (x.saturating_add_signed(dx), y.saturating_add_signed(dy));
        let app = fixture();
        assert_eq!(
            grab(&app, curve(), press, Gesture::Move),
            Some(3),
            "{press:?}"
        );

        let mut app = fixture();
        start(&mut app, 3, Gesture::Move, press);
        let mut writes = Vec::new();
        for across in 1..=3 {
            writes.extend(to(&mut app, curve(), (press.0 + across, press.1)));
            answer(&mut app);
        }
        writes.extend(release(&mut app, Some((curve(), (press.0 + 3, press.1)))));
        assert!(
            values(&writes, "g").is_empty(),
            "across from {press:?}: {writes:?}"
        );
        assert!(
            !values(&writes, "f").is_empty(),
            "across from {press:?}: {writes:?}"
        );

        let mut app = fixture();
        start(&mut app, 3, Gesture::Move, press);
        let mut writes = Vec::new();
        for down in 1..=2 {
            writes.extend(to(&mut app, curve(), (press.0, press.1 + down)));
            answer(&mut app);
        }
        writes.extend(release(&mut app, Some((curve(), (press.0, press.1 + 2)))));
        assert!(
            values(&writes, "f").is_empty(),
            "down from {press:?}: {writes:?}"
        );
        assert!(
            !values(&writes, "g").is_empty(),
            "down from {press:?}: {writes:?}"
        );
    }

    let mut app = fixture();
    start(&mut app, 3, Gesture::Move, (x, y));
    let mut writes = Vec::new();
    for up in 1..=2 {
        writes.extend(to(&mut app, curve(), (x, y - up)));
        answer(&mut app);
    }
    assert!(
        values(&writes, "f").is_empty(),
        "along its column: {writes:?}"
    );
    assert_eq!(values(&writes, "g").len(), 2, "{writes:?}");
    let back = eq.gain_db(eq.gain_raw(tone::gain_at(curve(), y)));
    assert_eq!(
        values(&release(&mut app, Some((curve(), (x, y)))), "g"),
        vec![back],
        "back on its own row, left once, the row sets the gain"
    );
}

/// A left drag past the curve's edges takes the edge the pointer is past, and a frequency
/// past the band's window its window's end; a gain at the bottom of the chart is the step on
/// the chart rather than the nearest one, a hundredth of a decibel below it, which the chart
/// would draw no point for. Every other row but the 0 dB one reads the step nearest its own
/// gain, and the point is drawn on the row it was dragged to.
#[test]
fn a_left_drag_keeps_its_point_on_the_band_and_the_chart() {
    for y in curve().y..curve().bottom() {
        let mut app = tone_app(2);
        let (x, from) = point(&app, 3);
        start(&mut app, 3, Gesture::Move, (x, from));
        let writes = to(&mut app, curve(), (x, y));
        let gain = writes.iter().find_map(|write| match write {
            Intent::EqSet { param, value, .. } if param == "g" => Some(*value),
            _ => None,
        });
        match y == from {
            true => assert_eq!(gain, None, "row {y} is the press's own row"),
            false => {
                // The bottom row's nearest step is the one just off the chart.
                let nearest = eq().gain_raw(tone::gain_at(curve(), y));
                let wanted = match y + 1 == curve().bottom() {
                    true => nearest + 1,
                    false => nearest,
                };
                assert_eq!(gain, Some(eq().gain_db(wanted)), "row {y}");
            }
        }
        assert_eq!(point(&app, 3).1, y, "row {y}: the point is not drawn there");
    }

    let eq = eq();
    let mut app = tone_app(2);
    let (x, y) = point(&app, 1);
    start(&mut app, 1, Gesture::Move, (x, y));
    let floor = eq.gain_raw(tone::GAIN_FLOOR_DB);
    assert!(
        eq.gain_db(floor) < tone::GAIN_FLOOR_DB,
        "the fixture's floor step"
    );
    assert_eq!(
        to(&mut app, curve(), (79, 23)),
        vec![
            write(2, EqParam::Gain, floor + 1),
            write(2, EqParam::Frequency, EqSpec::STEP_MAX),
        ]
    );
    answer(&mut app);
    assert_eq!(
        to(&mut app, curve(), (0, 0)),
        vec![
            write(2, EqParam::Frequency, 0),
            write(2, EqParam::Gain, EqSpec::STEP_MAX),
        ]
    );
}

/// A right drag sets the band's Q from how far the pointer has gone sideways since the
/// press, one of the device's steps per cell: right widens the bell, the Q coming down, and
/// left narrows it; up and down change nothing, and past either end of its steps the Q stays
/// at that end.
#[test]
fn a_right_drag_right_lowers_the_q_a_step_a_cell() {
    let mut app = tone_app(2);
    let (x, y) = point(&app, 2);
    start(&mut app, 2, Gesture::Width, (x, y));
    assert_eq!(
        to(&mut app, curve(), (x + 3, y + 2)),
        vec![write(3, EqParam::Q, 61)]
    );
    answer(&mut app);
    assert_eq!(
        to(&mut app, curve(), (x - 2, 0)),
        vec![write(3, EqParam::Q, 66)]
    );
    answer(&mut app);
    assert_eq!(
        to(&mut app, curve(), (x + 70, y)),
        vec![write(3, EqParam::Q, 0)]
    );
    answer(&mut app);
    assert!(to(&mut app, curve(), (x + 71, y)).is_empty());
    let mut app = tone_app(2);
    hear(&mut app, "12.3.1", 120);
    start(&mut app, 2, Gesture::Width, (x, y));
    assert_eq!(
        to(&mut app, curve(), (0, y)),
        vec![write(3, EqParam::Q, EqSpec::STEP_MAX)]
    );
}

/// The value of every write of `param` among `writes`, in the order they were asked for.
fn values(writes: &[Intent], param: &str) -> Vec<f64> {
    writes
        .iter()
        .filter_map(|write| match write {
            Intent::EqSet {
                param: written,
                value,
                ..
            } if written == param => Some(*value),
            _ => None,
        })
        .collect()
}

/// Ten motions of a left drag the device answers none of the writes of send one write of
/// each parameter, at the first; every later step waits, only the latest kept, and the
/// release writes where the pointer let go: two writes a parameter in all, the last of them
/// the value under the pointer. In Flat mode the first writes switch the tone control to EQ,
/// once. The point is drawn under the pointer throughout — before the device confirms any of
/// it — and still is once the button is let go, until the device answers.
#[test]
fn a_drag_the_device_has_not_answered_holds_all_but_the_first_write() {
    let eq = eq();
    let mut app = tone_app(1);
    let (x, y) = point(&app, 2);
    start(&mut app, 2, Gesture::Move, (x, y));
    let cells: Vec<(u16, u16)> = (0..10)
        .map(|step| (x - 4 + step, if step == 0 { y - 1 } else { y - 2 }))
        .collect();
    let mut sent = Vec::new();
    for &at in &cells {
        sent.extend(to(&mut app, curve(), at));
        assert_eq!(point(&app, 2), at, "the point is not under the pointer");
    }
    let last = cells[9];
    sent.extend(release(&mut app, Some((curve(), last))));
    assert_eq!(app.pointer.curve_drag, None);
    assert_eq!(point(&app, 2), last, "the point left where it was let go");

    let hz = eq
        .frequency_raw(2, tone::frequency_at(curve(), last.0))
        .expect("a step");
    let db = eq.gain_raw(tone::gain_at(curve(), last.1));
    let (f, g) = (values(&sent, "f"), values(&sent, "g"));
    assert_eq!(f.len(), 2, "{sent:?}");
    assert_eq!(g.len(), 2, "{sent:?}");
    assert_eq!(f.last(), Some(&eq.frequency(2, hz).expect("a step")));
    assert_eq!(g.last(), Some(&eq.gain_db(db)));
    let switches: Vec<usize> = (0..sent.len())
        .filter(|&at| sent[at] == Intent::EqMode("eq".into()))
        .collect();
    assert_eq!(switches, vec![2], "{sent:?}");
}

/// A held step goes out once the device has answered the write before it — the latest step
/// held, in place of every one before it — or once the wait for that answer has run out; a
/// parameter still waiting keeps its step held, and nothing goes out twice.
#[test]
fn a_held_step_goes_out_once_the_device_answers_or_the_wait_runs_out() {
    let mut app = tone_app(2);
    let (x, y) = point(&app, 2);
    start(&mut app, 2, Gesture::Width, (x, y));
    assert_eq!(
        to(&mut app, curve(), (x + 1, y)),
        vec![write(3, EqParam::Q, 63)]
    );
    for across in 2..=4 {
        assert!(to(&mut app, curve(), (x + across, y)).is_empty());
    }
    assert!(confirmed(&mut app).is_empty(), "sent before the answer");
    hear(&mut app, "12.3.1", 63);
    assert_eq!(confirmed(&mut app), vec![write(3, EqParam::Q, 60)]);
    assert!(confirmed(&mut app).is_empty(), "sent twice");

    assert!(to(&mut app, curve(), (x + 5, y)).is_empty());
    app.tick(Instant::now() + ANSWER_LIFETIME);
    assert_eq!(confirmed(&mut app), vec![write(3, EqParam::Q, 59)]);
}

/// A press let go where it landed, with no drag between, writes nothing; one let go on
/// another cell writes that cell, drag or none; and with no curve drawn to let go on, only
/// the steps held are written.
#[test]
fn a_release_writes_where_the_pointer_let_go_and_only_after_a_move() {
    let eq = eq();
    let mut app = tone_app(2);
    let (x, y) = point(&app, 2);
    start(&mut app, 2, Gesture::Move, (x + 1, y));
    assert!(release(&mut app, Some((curve(), (x + 1, y)))).is_empty());
    assert_eq!(app.pointer.curve_drag, None);

    start(&mut app, 2, Gesture::Move, (x, y));
    let hz = eq
        .frequency_raw(2, tone::frequency_at(curve(), x + 2))
        .expect("a step");
    assert_eq!(
        release(&mut app, Some((curve(), (x + 2, y)))),
        vec![write(3, EqParam::Frequency, hz)]
    );

    let mut app = tone_app(2);
    start(&mut app, 2, Gesture::Width, (x, y));
    to(&mut app, curve(), (x + 1, y));
    to(&mut app, curve(), (x + 3, y));
    assert_eq!(release(&mut app, None), vec![write(3, EqParam::Q, 61)]);
}

/// A press of the drag's own button while it is still held is its release lost: the drag is
/// let go where the pointer last dragged it, the step held there written; a press of the
/// other button, or any event but a press, leaves the drag held. Mouse capture turning off
/// lets it go the same way, and capture on leaves it.
#[test]
fn a_lost_release_and_capture_off_let_go_where_the_drag_last_was() {
    let press = |button| MouseEvent {
        kind: MouseEventKind::Down(button),
        column: 3,
        row: 20,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    for turn_off in [false, true] {
        let mut app = tone_app(2);
        let (x, y) = point(&app, 2);
        start(&mut app, 2, Gesture::Width, (x, y));
        assert_eq!(
            to(&mut app, curve(), (x + 1, y)),
            vec![write(3, EqParam::Q, 63)]
        );
        assert!(to(&mut app, curve(), (x + 4, y)).is_empty(), "not held");
        assert!(lost(&mut app, &press(MouseButton::Left)).is_empty());
        let moved = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Right),
            ..press(MouseButton::Right)
        };
        assert!(lost(&mut app, &moved).is_empty());
        assert!(capture_off(&mut app).is_empty(), "capture is on");
        assert!(app.pointer.curve_drag.is_some(), "let go of too soon");
        let let_go = match turn_off {
            true => {
                app.mouse = false;
                capture_off(&mut app)
            }
            false => lost(&mut app, &press(MouseButton::Right)),
        };
        assert_eq!(
            let_go,
            vec![write(3, EqParam::Q, 60)],
            "capture off: {turn_off}"
        );
        assert_eq!(app.pointer.curve_drag, None);
    }
}

/// `Esc` drops a drag and does nothing else — the screen stays — and a box opening over the
/// screen, or a screen change, drops it too.
#[test]
fn esc_a_box_and_a_screen_change_drop_the_drag() {
    let mut app = tone_app(2);
    start(&mut app, 1, Gesture::Move, (20, 6));
    assert!(app.key(KeyEvent::from(KeyCode::Esc)).is_empty());
    assert_eq!(app.pointer.curve_drag, None, "Esc kept the drag");
    assert_eq!(
        app.screen,
        Screen::Tone,
        "Esc left the screen during a drag"
    );
    app.key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Device, "Esc with no drag is Esc");

    for code in [
        KeyCode::Char('?'),
        KeyCode::Char('p'),
        KeyCode::Enter,
        KeyCode::Char('q'),
    ] {
        let mut app = tone_app(2);
        start(&mut app, 1, Gesture::Move, (20, 6));
        app.key(KeyEvent::from(code));
        assert!(app.modal_open(), "{code:?} opened no box");
        assert_eq!(
            app.pointer.curve_drag, None,
            "{code:?}: the drag outlived the box"
        );
    }

    let mut app = tone_app(2);
    start(&mut app, 1, Gesture::Move, (20, 6));
    app.key(KeyEvent::from(KeyCode::Char('t')));
    assert_eq!(
        app.pointer.curve_drag, None,
        "a screen change kept the drag"
    );
}
