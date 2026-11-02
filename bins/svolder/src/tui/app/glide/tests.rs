// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use svolder_ipc::TuiConfig;

use super::*;
use crate::backend::DeviceRow;
use crate::tui::app::{Screen, ToneRow};
use crate::tui::mouse::{self, HitMap, MouseAction};
use crate::tui::render;
use crate::tui::test_support::{answer, hear_one as hear, mouse_at, press, profile, row_of};
use crate::tui::theme::{Role, Theme};

/// The volume's function on the built-in profile, 0 to 127.
const VOLUME: &str = "10";
/// The balance's, 1 to 127 with 64 for the centre.
const BALANCE: &str = "11";
/// The tone control's loudness strength, 0 to 127.
const LOUDNESS: &str = "12.52";

/// A Gungnir 2 on the device screen, the volume at `volume`, balance centred and the tone
/// control in EQ mode with the loudness at 10.
fn app_at(volume: i64) -> App {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("gungnir")],
    );
    hear(&mut app, VOLUME, volume);
    hear(&mut app, BALANCE, 64);
    hear(&mut app, "12.50", 2);
    hear(&mut app, LOUDNESS, 10);
    app
}

/// The volume's bar, 127 cells wide: one cell a raw step, cell `i` standing for `i + 1`.
fn wide() -> Rect {
    Rect::new(10, 3, 127, 1)
}

/// The column of [`wide`] whose cell stands for raw value `raw`.
fn col(raw: i64) -> u16 {
    10 + u16::try_from(raw - 1).expect("a value from 1")
}

/// The raw value a `Set` of the volume writes.
fn volume_of(intent: &Intent) -> i64 {
    match intent {
        Intent::Set { key, value, .. } if key == "volume" => value.parse().expect("a number"),
        other => panic!("not a volume write: {other:?}"),
    }
}

/// Every volume value among `intents`.
fn volumes(intents: &[Intent]) -> Vec<i64> {
    intents.iter().map(volume_of).collect()
}

/// The hits `app` draws on an 80x24 terminal, or on `size`.
fn hits_of(app: &App, size: (u16, u16)) -> HitMap {
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("a terminal");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    hits
}

/// The action a click at `(x, y)` resolves to, after the loop's own press bookkeeping.
fn click(hits: &HitMap, app: &App, x: u16, y: u16) -> Option<MouseAction> {
    mouse::resolve(
        hits,
        mouse_at(MouseEventKind::Down(MouseButton::Left), x, y),
        app,
    )
}

/// Carry out a bar action the way the loop does, answering the writes it asks for.
fn carry(app: &mut App, action: MouseAction) -> Vec<Intent> {
    match action {
        MouseAction::BarPress { bar, at, area } => press_bar(app, bar, area, at),
        MouseAction::BarTo { at, area } => to(app, area, at),
        MouseAction::BarRelease { at, area } => release(app, area.map(|area| (area, at))),
        other => panic!("not a bar action: {other:?}"),
    }
}

fn press_bar(app: &mut App, bar: Bar, area: Rect, at: u16) -> Vec<Intent> {
    super::press(app, bar, area, at)
}

// ---- the value a cell stands for ----

/// The first, middle and last cell of 10-, 20- and 40-cell bars on a 0 to 127 range stand for the
/// value at their middle; a column left of a bar is the minimum and one right of it the maximum.
#[test]
fn a_cell_stands_for_the_value_at_its_middle() {
    // Cell i of w cells: (2i + 1) * 127 / (2w), rounded to the nearest step.
    for (width, first, middle, last) in [(10, 6, 70, 121), (20, 3, 67, 124), (40, 2, 65, 125)] {
        let bar = Rect::new(5, 0, width, 1);
        assert_eq!(value_at(5, bar, 0, 127), first, "{width} cells, first");
        assert_eq!(
            value_at(5 + width / 2, bar, 0, 127),
            middle,
            "{width} cells, middle"
        );
        assert_eq!(
            value_at(5 + width - 1, bar, 0, 127),
            last,
            "{width} cells, last"
        );
    }
}

/// The ends of the range hold: columns outside the bar clamp.
#[test]
fn columns_outside_the_bar_clamp_to_the_range() {
    let bar = Rect::new(5, 0, 10, 1);
    assert_eq!(value_at(4, bar, 0, 127), 0, "left of the bar");
    assert_eq!(value_at(0, bar, 3, 127), 3, "far left of the bar");
    assert_eq!(value_at(15, bar, 0, 127), 127, "right of the bar");
    assert_eq!(value_at(200, bar, 0, 127), 127, "far right of the bar");
    assert_eq!(value_at(7, Rect::new(5, 0, 0, 1), 0, 127), 0, "no bar");
}

/// Balance's track puts its marker on one cell, the first for the minimum and the last for the
/// maximum: an odd track has a centre cell that reads the centre exactly, an even one has two
/// cells either side of it.
#[test]
fn balances_centre_cell_reads_the_centre() {
    let odd = Rect::new(2, 0, 15, 1);
    assert_eq!(marker_at(2, odd, 1, 127), 1);
    assert_eq!(marker_at(2 + 7, odd, 1, 127), 64, "the centre cell");
    assert_eq!(marker_at(2 + 14, odd, 1, 127), 127);
    let even = Rect::new(2, 0, 14, 1);
    let (left, right) = (
        marker_at(2 + 6, even, 1, 127),
        marker_at(2 + 7, even, 1, 127),
    );
    assert!(left < 64 && right > 64, "the two middle cells straddle 64");
    assert_eq!(marker_at(0, even, 1, 127), 1, "left of the track");
    assert_eq!(marker_at(99, even, 1, 127), 127, "right of the track");
}

// ---- clicks and drags on the drawn bars ----

/// A click on the Volume's, the Balance's bar focuses the row and sets the value the cell stands
/// for; a click on the label only focuses the row.
#[test]
fn a_click_on_a_level_bar_focuses_its_row_and_sets_the_cell() {
    let mut app = app_at(40);
    hear(&mut app, BALANCE, 20);
    let hits = hits_of(&app, (80, 24));
    for key in ["volume", "balance"] {
        let row = row_of(&app, key);
        let area = hits.bar_area(Hit::LevelBar(row)).expect("the bar is drawn");
        let mid = area.x + area.width / 2;
        let action = click(&hits, &app, mid, area.y).expect("a click on the bar");
        assert_eq!(
            action,
            MouseAction::BarPress {
                bar: Bar::Row(row),
                at: mid,
                area
            }
        );
        app.selected = 0;
        let writes = carry(&mut app, action);
        assert_eq!(app.selected, row, "{key} row focused");
        let wanted = match key {
            "volume" => value_at(mid, area, 0, 127),
            _ => marker_at(mid, area, 1, 127),
        };
        assert_eq!(
            writes,
            vec![Intent::Set {
                key: key.into(),
                value: wanted.to_string(),
                step: 1
            }]
        );
        let _ = release(&mut app, Some((area, mid)));
        answer(&mut app);
        // Left of the bar is the label: it focuses.
        app.selected = 0;
        let label = click(&hits, &app, area.x - 1, area.y);
        assert_eq!(label, Some(MouseAction::Focus(Pane::Controls, row)));
    }
}

/// The Loudness bar sets its strength on the Tone pane, where it is as wide as the volume bar
/// beside it, and in the tone-control header; its label still toggles the mode.
#[test]
fn a_click_on_the_loudness_bar_sets_the_strength_on_both_screens() {
    for screen in [Screen::Device, Screen::Tone] {
        let mut app = app_at(40);
        app.show(screen);
        let hits = hits_of(&app, (80, 24));
        let area = hits.bar_area(Hit::LoudnessBar).expect("the bar is drawn");
        let width = match screen {
            Screen::Device => {
                let volume = Hit::LevelBar(row_of(&app, "volume"));
                hits.bar_area(volume)
                    .expect("the volume bar is drawn")
                    .width
            }
            _ => 5,
        };
        assert!(width >= 5, "{screen:?}: the volume bar is {width} cells");
        assert_eq!(area.width, width, "{screen:?}");
        let last = area.x + area.width - 1;
        let action = click(&hits, &app, last, area.y).expect("a click on the bar");
        let writes = carry(&mut app, action);
        assert_eq!(app.tone.row, ToneRow::Loudness);
        assert_eq!(writes.len(), 1, "{screen:?}");
        let wanted = value_at(last, area, 0, 127);
        assert!(
            format!("{:?}", writes[0]).contains(&wanted.to_string()),
            "{screen:?}: {:?}",
            writes[0]
        );
        // The word still toggles the loudness mode.
        let word = hits.area_of(Hit::Loudness).expect("the half is drawn");
        assert_eq!(
            click(&hits, &app, word.x, word.y),
            Some(MouseAction::ToggleTone(ToneRow::Loudness)),
            "{screen:?}"
        );
    }
}

/// The wheel over a bar nudges as it always did.
#[test]
fn the_wheel_over_a_bar_still_nudges() {
    let app = app_at(40);
    let hits = hits_of(&app, (80, 24));
    let row = row_of(&app, "volume");
    let area = hits.bar_area(Hit::LevelBar(row)).expect("the bar is drawn");
    let wheel = mouse_at(MouseEventKind::ScrollUp, area.x + 1, area.y);
    assert_eq!(
        mouse::resolve(&hits, wheel, &app),
        Some(MouseAction::Intent(Intent::Set {
            key: "volume".into(),
            value: "up".into(),
            step: 1
        }))
    );
}

/// A drag follows the pointer, clamps at both ends and goes on after the pointer leaves the row;
/// its release writes the last value.
#[test]
fn a_drag_follows_the_pointer_and_clamps_and_the_release_writes_the_last() {
    let mut app = app_at(10);
    let hits = hits_of(&app, (80, 24));
    let row = row_of(&app, "balance");
    let area = hits.bar_area(Hit::LevelBar(row)).expect("the bar is drawn");
    let action = click(&hits, &app, area.x + 2, area.y).expect("a press");
    carry(&mut app, action);
    answer(&mut app);
    // Far above and right of the bar, off the row and the pane: the maximum.
    let drag = mouse_at(MouseEventKind::Drag(MouseButton::Left), 79, 0);
    let action = mouse::resolve(&hits, drag, &app).expect("the drag goes on");
    assert_eq!(action, MouseAction::BarTo { at: 79, area });
    let writes = carry(&mut app, action);
    assert_eq!(
        writes,
        vec![Intent::Set {
            key: "balance".into(),
            value: "127".into(),
            step: 1
        }]
    );
    answer(&mut app);
    // And left of everything: the minimum.
    let drag = mouse_at(MouseEventKind::Drag(MouseButton::Left), 0, 23);
    let action = mouse::resolve(&hits, drag, &app).expect("the drag goes on");
    let writes = carry(&mut app, action);
    assert_eq!(
        writes,
        vec![Intent::Set {
            key: "balance".into(),
            value: "1".into(),
            step: 1
        }]
    );
    // Released with the write still unanswered: the value is written all the same.
    let up = mouse_at(MouseEventKind::Up(MouseButton::Left), 0, 23);
    assert!(mouse::resolve(&hits, up, &app).is_some());
    assert!(app.pointer.bar_drag.is_some());
    let _ = release(&mut app, Some((area, 0)));
    assert!(app.pointer.bar_drag.is_none());
}

/// A resize in the middle of a drag keeps it on the bar the new frame draws.
#[test]
fn a_resize_mid_drag_keeps_the_drag_on_the_bar_the_new_frame_draws() {
    let mut app = app_at(10);
    let small = hits_of(&app, (80, 24));
    let row = row_of(&app, "balance");
    let before = small.bar_area(Hit::LevelBar(row)).expect("the bar");
    let action = click(&small, &app, before.x, before.y).expect("a press");
    carry(&mut app, action);
    answer(&mut app);
    app.resize(140, 40);
    let wide = hits_of(&app, (140, 40));
    let after = wide.bar_area(Hit::LevelBar(row)).expect("the bar again");
    assert_ne!(before, after, "the wide frame draws it elsewhere");
    let end = after.x + after.width - 1;
    let drag = mouse_at(MouseEventKind::Drag(MouseButton::Left), end, after.y);
    let action = mouse::resolve(&wide, drag, &app).expect("the drag goes on");
    assert_eq!(
        action,
        MouseAction::BarTo {
            at: end,
            area: after
        }
    );
    let writes = carry(&mut app, action);
    assert_eq!(writes.len(), 1);
    assert!(format!("{:?}", writes[0]).contains("127"), "{writes:?}");
}

/// Mouse capture going off lets go of the bar where the pointer last dragged it; a lost release
/// is made up for by the next press.
#[test]
fn capture_off_and_a_lost_release_let_go_of_the_bar() {
    let mut app = app_at(10);
    let area = Rect::new(10, 3, 15, 1);
    let row = row_of(&app, "balance");
    press_bar(&mut app, Bar::Row(row), area, 10);
    answer(&mut app);
    let sent = to(&mut app, area, 24);
    assert_eq!(sent.len(), 1, "127 goes out at once");
    assert!(to(&mut app, area, 20).is_empty(), "the next step is held");
    app.mouse = false;
    // The held step is written by capture going off, though 127 is unanswered.
    let writes = capture_off(&mut app);
    assert!(app.pointer.bar_drag.is_none());
    assert_eq!(
        writes,
        vec![Intent::Set {
            key: "balance".into(),
            value: "91".into(),
            step: 1
        }]
    );

    app.mouse = true;
    press_bar(&mut app, Bar::Row(row), area, 10);
    let event = mouse_at(MouseEventKind::Down(MouseButton::Left), 11, 3);
    lost(&mut app, &event);
    assert!(app.pointer.bar_drag.is_none());
}

// ---- pacing ----

/// A drag across ten cells with the device silent sends one write, holds the newest step, and
/// sends it when the answer comes; it never sends a value the drag has moved past, and the
/// release is written even where the device has not answered.
#[test]
fn a_drag_paces_its_writes_by_the_devices_answers() {
    let mut app = app_at(10);
    let area = Rect::new(10, 3, 127, 1);
    let loud = Bar::Loudness;
    // The Loudness bar, 127 cells wide for exact steps: a press on cell 20 stands for 21.
    let first = press_bar(&mut app, loud, area, 10 + 20);
    assert_eq!(first.len(), 1);
    let mut sent = 1;
    for cell in 21..30 {
        let writes = to(&mut app, area, 10 + cell);
        assert!(writes.is_empty(), "cell {cell}: {writes:?}");
        sent += writes.len();
    }
    assert_eq!(sent, 1, "one write while the device is silent");
    assert_eq!(
        app.pointer.bar_drag.as_ref().and_then(|drag| drag.held),
        Some(30),
        "the newest step is held"
    );
    assert!(confirmed(&mut app).is_empty(), "nothing before the answer");
    // The device answers: the held step goes, and only it.
    answer(&mut app);
    let writes = confirmed(&mut app);
    assert_eq!(writes.len(), 1);
    assert!(format!("{:?}", writes[0]).contains("30"));
    assert!(
        app.pointer
            .bar_drag
            .as_ref()
            .is_some_and(|drag| drag.held.is_none())
    );
    // Moved on while that one is unanswered, then released onto the very step held: written.
    to(&mut app, area, 10 + 39);
    assert_eq!(
        app.pointer.bar_drag.as_ref().and_then(|drag| drag.held),
        Some(40)
    );
    let writes = release(&mut app, Some((area, 10 + 39)));
    assert_eq!(writes.len(), 1);
    assert!(format!("{:?}", writes[0]).contains("40"), "{writes:?}");
}

// ---- the volume's glide ----

/// Run the climb of `app` from `t0` on a loop ticking every 33 ms, the device answering each
/// write `lag` after it was sent, until `until` has passed: every volume write with the time it
/// was sent at.
fn run_climb(app: &mut App, t0: Instant, lag: Duration, until: Duration) -> Vec<(Duration, i64)> {
    let mut sent = Vec::new();
    let mut due: Vec<(Duration, i64)> = Vec::new();
    let mut at = Duration::ZERO;
    while at < until {
        at += Duration::from_millis(33);
        let ready: Vec<i64> = due
            .iter()
            .filter(|(when, _)| *when <= at)
            .map(|(_, value)| *value)
            .collect();
        due.retain(|(when, _)| *when > at);
        for value in ready {
            hear(app, VOLUME, value);
        }
        app.tick(t0 + at);
        for value in volumes(&confirmed(app)) {
            sent.push((at, value));
            due.push((at + lag, value));
        }
    }
    sent
}

/// From 50%, a click at 90% writes 75% at once, then the value rises a step at a time, a tenth
/// of the range a second: about 80% at half a second, 90% (and no more) at one and a half.
#[test]
fn a_raise_above_three_quarters_writes_75_at_once_then_climbs_a_tenth_a_second() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    let writes = press_bar(&mut app, volume, wide(), col(114));
    // 75% of 0..127 is 95.25: the step below it.
    assert_eq!(volumes(&writes), vec![95]);
    assert!(app.climbing());
    answer(&mut app);
    app.tick(t0);

    let sent = run_climb(
        &mut app,
        t0,
        Duration::from_millis(50),
        Duration::from_secs(3),
    );
    let values: Vec<i64> = sent.iter().map(|(_, value)| *value).collect();
    assert_eq!(
        values,
        (96..=114).collect::<Vec<_>>(),
        "one step at a time, to 90%"
    );
    let at = |value: i64| sent.iter().find(|(_, v)| *v == value).expect("sent").0;
    // 95.25 + 0.1 * 127 * t: step 101 near half a second, 114 near one and a half.
    assert!((Duration::from_millis(350)..=Duration::from_millis(700)).contains(&at(101)));
    assert!((Duration::from_millis(1300)..=Duration::from_millis(1700)).contains(&at(114)));
    assert!(!app.climbing(), "the climb ends on its target");
}

/// From 75% to the top takes two and a half seconds.
#[test]
fn the_top_quarter_takes_two_and_a_half_seconds() {
    let mut app = app_at(95);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    let writes = press_bar(&mut app, volume, wide(), col(127));
    assert!(
        writes.is_empty(),
        "already at 75%: nothing to write at once"
    );
    app.tick(t0);
    let sent = run_climb(
        &mut app,
        t0,
        Duration::from_millis(20),
        Duration::from_secs(5),
    );
    let (took, last) = *sent.last().expect("writes");
    assert_eq!(last, 127);
    assert!(
        (Duration::from_millis(2300)..=Duration::from_millis(2800)).contains(&took),
        "took {took:?}"
    );
}

/// Lowering is instant from anywhere, and so is a raise to 75% or below.
#[test]
fn lowering_and_raising_to_three_quarters_are_instant() {
    let mut app = app_at(120);
    let bar = Bar::Row(row_of(&app, "volume"));
    assert_eq!(
        volumes(&press_bar(&mut app, bar, wide(), col(30))),
        vec![30]
    );
    assert!(app.pointer.climb.is_none());
    let _ = release(&mut app, Some((wide(), col(30))));
    answer(&mut app);
    assert_eq!(
        volumes(&press_bar(&mut app, bar, wide(), col(95))),
        vec![95]
    );
    assert!(
        app.pointer.climb.is_none(),
        "75% and below is a plain write"
    );
}

/// The device never receives a volume above what the climb allows at that moment, however late
/// it answers: the step waits behind the unanswered write and is replaced by the newest.
#[test]
fn a_slow_device_is_never_asked_for_more_than_the_climb_allows() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    let mut sent = volumes(&press_bar(&mut app, volume, wide(), col(127)));
    assert_eq!(sent, vec![95]);
    // The device is silent for a second and a half; the climb has reached ~114 by then.
    for tenth in 0..=15 {
        app.tick(t0 + Duration::from_millis(100 * tenth));
        assert!(
            confirmed(&mut app).is_empty(),
            "written behind an open write"
        );
    }
    // It answers late: one write, the climb as it stands, not every step it missed.
    answer(&mut app);
    app.tick(t0 + Duration::from_millis(1500));
    let late = volumes(&confirmed(&mut app));
    assert_eq!(late.len(), 1);
    let allowed = 95.25 + 12.7 * 1.5;
    assert!(late[0] as f64 <= allowed, "{late:?} beyond {allowed}");
    assert!(late[0] > 95);
    sent.extend(late);
    assert!(sent.iter().all(|&v| v as f64 <= allowed));
}

/// A device that never answers does not stand in the way for ever: the wait runs out, and the
/// climb ends because the reading is not what it wrote.
#[test]
fn a_device_that_never_answers_ends_the_climb_when_the_wait_runs_out() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    press_bar(&mut app, volume, wide(), col(127));
    for second in 1..=8 {
        app.tick(t0 + Duration::from_secs(second));
        let writes = confirmed(&mut app);
        assert!(writes.is_empty(), "second {second}: {writes:?}");
    }
    assert!(app.pointer.climb.is_none());
}

/// A volume changed elsewhere (the knob) ends the climb instead of being climbed from.
#[test]
fn a_reading_that_is_not_the_last_step_written_ends_the_climb() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    press_bar(&mut app, volume, wide(), col(127));
    answer(&mut app);
    hear(&mut app, VOLUME, 30);
    app.tick(t0 + Duration::from_millis(500));
    assert!(confirmed(&mut app).is_empty());
    assert!(app.pointer.climb.is_none());
}

/// Any key stops the climb where it is and then does what it does; `m` mutes at once.
#[test]
fn any_key_stops_the_climb_and_m_mutes() {
    for code in [KeyCode::Down, KeyCode::Char('m'), KeyCode::Char('x')] {
        let mut app = app_at(64);
        let t0 = app.now;
        let volume = Bar::Row(row_of(&app, "volume"));
        press_bar(&mut app, volume, wide(), col(127));
        answer(&mut app);
        app.tick(t0 + Duration::from_millis(500));
        let intents = press(&mut app, code);
        assert!(app.pointer.climb.is_none(), "{code:?}");
        assert!(app.pointer.bar_drag.is_none(), "{code:?}");
        app.tick(t0 + Duration::from_secs(3));
        assert!(confirmed(&mut app).is_empty(), "{code:?} the climb went on");
        if code == KeyCode::Char('m') {
            assert!(
                intents
                    .iter()
                    .any(|i| matches!(i, Intent::Set { key, .. } if key == "mute")),
                "{intents:?}"
            );
        }
    }
}

/// A click on the bar below the reached value lowers at once and ends the climb; a new click or
/// drag above sets a new target and the climb carries on toward it.
#[test]
fn a_lower_click_ends_the_climb_and_a_higher_one_retargets_it() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    press_bar(&mut app, volume, wide(), col(110));
    answer(&mut app);
    app.tick(t0);
    app.tick(t0 + Duration::from_millis(500));
    // Letting go mid-climb writes nothing the climb has not paid for: the reading is 95, so no
    // more than 96, and nothing at all when the answer is outstanding.
    let released = release(&mut app, Some((wide(), col(110))));
    assert!(highest(&released) <= 96, "{released:?}");
    // Retarget upward mid-climb: the climb goes on from where it is.
    assert!(press_bar(&mut app, volume, wide(), col(125)).len() <= 1);
    assert_eq!(
        app.pointer.climb.as_ref().map(VolumeClimb::target),
        Some(125)
    );
    let _ = release(&mut app, Some((wide(), col(125))));
    answer(&mut app);
    app.tick(t0 + Duration::from_millis(1000));
    let writes = volumes(&confirmed(&mut app));
    assert!(writes.iter().all(|&v| v < 125));
    answer(&mut app);
    // Below what it has reached: a plain write, the climb gone.
    let writes = press_bar(&mut app, volume, wide(), col(70));
    assert_eq!(volumes(&writes), vec![70]);
    assert!(app.pointer.climb.is_none());
    let _ = release(&mut app, Some((wide(), col(70))));
    app.tick(t0 + Duration::from_secs(3));
    answer(&mut app);
    assert!(confirmed(&mut app).is_empty());
}

/// Balance and Loudness never climb, whatever the value.
#[test]
fn balance_and_loudness_never_climb() {
    let mut app = app_at(64);
    let balance = Bar::Row(row_of(&app, "balance"));
    let area = Rect::new(10, 3, 15, 1);
    let writes = press_bar(&mut app, balance, area, 24);
    assert_eq!(writes.len(), 1);
    assert!(app.pointer.climb.is_none());
    let _ = release(&mut app, Some((area, 24)));
    let writes = press_bar(&mut app, Bar::Loudness, wide(), col(127));
    assert_eq!(writes.len(), 1);
    assert!(app.pointer.climb.is_none());
}

/// A resize in the middle of a climb keeps its target and its pace.
#[test]
fn a_resize_mid_climb_keeps_the_target() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    press_bar(&mut app, volume, wide(), col(120));
    answer(&mut app);
    app.resize(140, 40);
    app.tick(t0 + Duration::from_millis(500));
    assert_eq!(
        app.pointer.climb.as_ref().map(VolumeClimb::target),
        Some(120)
    );
    assert_eq!(volumes(&confirmed(&mut app)), vec![96]);
}

/// A loop that stalled does not jump the climb: it is one step above the reading, whatever the
/// time that passed.
#[test]
fn a_stalled_loop_slows_the_climb_instead_of_jumping_it() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    press_bar(&mut app, volume, wide(), col(127));
    answer(&mut app);
    app.tick(t0 + Duration::from_secs(60));
    let writes = volumes(&confirmed(&mut app));
    assert_eq!(writes, vec![96], "one step above the reading, no more");
}

/// While climbing, the target's cell is drawn dim on the bar and the filled part keeps its
/// shades.
#[test]
fn the_target_cell_is_drawn_dim_while_climbing() {
    let mut app = app_at(64);
    let volume = Bar::Row(row_of(&app, "volume"));
    let hits = hits_of(&app, (80, 24));
    let area = hits
        .bar_area(Hit::LevelBar(row_of(&app, "volume")))
        .expect("the bar");
    let theme = Theme::new(false);
    let draw = |app: &App| {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a terminal");
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                render::draw(f, app, &theme, &mut hits);
            })
            .expect("the frame draws");
        terminal.backend().buffer().clone()
    };
    let dim = crate::tui::widgets::rows::own_colours(theme.style(Role::Dim));
    let style_at = |buffer: &ratatui::buffer::Buffer, x: u16| {
        let style = buffer.cell((x, area.y)).expect("a cell").style();
        style.add_modifier
    };
    // The row's own colour shows through a style with none of its own, so the weight is what
    // tells the dim cell apart at 16 colours.
    let dim = dim.add_modifier;
    let before = draw(&app);
    let last = area.x + area.width - 1;
    assert_ne!(style_at(&before, last), dim, "not dim before the climb");

    press_bar(&mut app, volume, area, area.x + area.width - 1);
    assert!(app.pointer.climb.is_some());
    let after = draw(&app);
    assert_eq!(style_at(&after, last), dim, "the target's cell is dim");
    assert_eq!(after.cell((last, area.y)).expect("a cell").symbol(), "█");
    assert_eq!(
        style_at(&after, area.x),
        style_at(&before, area.x),
        "the filled part keeps its shades"
    );
}

/// The bar draws the dragged value at once, before the device has confirmed anything, and the
/// Volume's bar draws the step the climb has reached rather than its target.
#[test]
fn a_held_bar_draws_the_dragged_value_at_once() {
    let mut app = app_at(10);
    let balance = app.profile.control_by_key("balance").expect("balance").id;
    let row = row_of(&app, "balance");
    let area = Rect::new(10, 3, 15, 1);
    assert_eq!(app.dragged_raw(balance), None);
    press_bar(&mut app, Bar::Row(row), area, 24);
    assert_eq!(
        app.dragged_raw(balance),
        Some(127),
        "the reading is still 64"
    );
    let _ = release(&mut app, Some((area, 24)));
    assert_eq!(
        app.dragged_raw(balance),
        None,
        "let go, the readings draw again"
    );

    let volume_id = app.volume().expect("volume").id;
    let volume = Bar::Row(row_of(&app, "volume"));
    press_bar(&mut app, volume, wide(), col(127));
    assert_eq!(app.dragged_raw(volume_id), Some(95), "75%, not the target");
}

// ---- the glide waits for the device ----

/// The highest Volume value among `intents`, 0 for none.
fn highest(intents: &[Intent]) -> i64 {
    intents
        .iter()
        .filter_map(|intent| match intent {
            Intent::Set { key, value, .. } if key == "volume" => value.parse().ok(),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// A silent device with the last reading 64: the click at the top writes 75%, the bar is dragged
/// back a little and held for 2.6 s (the climb on a clock would be at the top by then), and then
/// the wait expires, or a key is pressed. No write is ever above 95 plus one step.
fn silent_then(finish: impl FnOnce(&mut App, Instant) -> Vec<Intent>) {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    let mut sent = press_bar(&mut app, volume, wide(), col(127));
    assert_eq!(volumes(&sent), vec![95]);
    let mut at = Duration::ZERO;
    while at < Duration::from_millis(2600) {
        at += Duration::from_millis(33);
        app.tick(t0 + at);
        sent.extend(confirmed(&mut app));
    }
    sent.extend(to(&mut app, wide(), col(110)));
    let end = finish(&mut app, t0 + at);
    sent.extend(end);
    assert!(
        highest(&sent) <= 96,
        "a raise above 95 plus one step reached the device: {sent:?}"
    );
}

#[test]
fn a_held_step_never_jumps_a_silent_device_when_the_wait_runs_out() {
    silent_then(|app, now| {
        let mut sent = Vec::new();
        for tenth in 1..=30 {
            app.tick(now + Duration::from_millis(100 * tenth));
            sent.extend(confirmed(app));
        }
        sent
    });
}

#[test]
fn a_held_step_never_jumps_a_silent_device_when_m_is_pressed() {
    silent_then(|app, _| press(app, KeyCode::Char('m')));
}

#[test]
fn a_held_step_never_jumps_a_silent_device_when_a_key_is_pressed() {
    silent_then(|app, _| press(app, KeyCode::Char('x')));
}

/// A 2 s late answer is followed by one step above the answered value, not by the height the
/// clock reached while the device was silent.
#[test]
fn a_late_answer_resumes_the_climb_one_step_above_it() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    assert_eq!(
        volumes(&press_bar(&mut app, volume, wide(), col(127))),
        vec![95]
    );
    let mut at = Duration::ZERO;
    while at < Duration::from_secs(2) {
        at += Duration::from_millis(33);
        app.tick(t0 + at);
        assert!(confirmed(&mut app).is_empty());
    }
    answer(&mut app);
    app.tick(t0 + at + Duration::from_millis(33));
    assert_eq!(volumes(&confirmed(&mut app)), vec![96]);
}

/// A device that answers each write in 50 ms still climbs 75% to 100% in about 2.5 s.
#[test]
fn a_responsive_device_climbs_in_two_and_a_half_seconds() {
    let mut app = app_at(95);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    let mut due: Vec<(Duration, i64)> = Vec::new();
    let mut at = Duration::ZERO;
    app.tick(t0);
    for value in volumes(&press_bar(&mut app, volume, wide(), col(127))) {
        due.push((at + Duration::from_millis(50), value));
    }
    let mut done = None;
    while at < Duration::from_secs(6) && done.is_none() {
        at += Duration::from_millis(33);
        for (_, value) in due.iter().filter(|(when, _)| *when <= at) {
            hear(&mut app, VOLUME, *value);
        }
        due.retain(|(when, _)| *when > at);
        app.tick(t0 + at);
        for value in volumes(&confirmed(&mut app)) {
            due.push((at + Duration::from_millis(50), value));
            if value == 127 {
                done = Some(at);
            }
        }
    }
    let took = done.expect("the climb reached the top");
    assert!(
        (Duration::from_millis(2200)..=Duration::from_millis(2800)).contains(&took),
        "took {took:?}"
    );
}

/// Whatever order the readings come in, no volume write is above the last reading plus one step
/// (or above 75% itself).
#[test]
fn readings_out_of_order_never_let_a_write_exceed_the_reading_plus_one() {
    let mut app = app_at(64);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    let mut sent = volumes(&press_bar(&mut app, volume, wide(), col(127)));
    let script = [95, 96, 90, 97, 96, 99, 98];
    let mut at = Duration::ZERO;
    for heard in script {
        at += Duration::from_millis(400);
        hear(&mut app, VOLUME, heard);
        let reading = heard;
        app.tick(t0 + at);
        for write in volumes(&confirmed(&mut app)) {
            assert!(
                write <= 95 || write <= reading + 1,
                "{write} after a reading of {reading}"
            );
            sent.push(write);
        }
    }
    assert!(sent.iter().all(|&w| w <= 100), "{sent:?}");
}

/// The Balance marker lands on the clicked cell, at 80x24 and at 140x40. The text after the
/// track changes width with the value, which can move the track by a cell, so each click is
/// repeated on the track the value it sets is drawn with until the two agree.
#[test]
fn a_balance_click_draws_the_marker_on_the_clicked_cell() {
    for size in [(80, 24), (140, 40)] {
        let setup = || {
            let mut app = app_at(40);
            app.resize(size.0, size.1);
            hear(&mut app, BALANCE, 20);
            app
        };
        let draw = |app: &App| {
            let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
            let mut hits = HitMap::default();
            terminal
                .draw(|f| {
                    render::draw(f, app, &Theme::new(false), &mut hits);
                })
                .expect("draws");
            (hits, terminal.backend().buffer().clone())
        };
        let app = setup();
        let row = row_of(&app, "balance");
        let first = hits_of(&app, size)
            .bar_area(Hit::LevelBar(row))
            .expect("the track");
        for index in 0..first.width {
            let mut area = first;
            let mut settled = None;
            for _ in 0..4 {
                let mut probe = setup();
                let cell = area.x + index;
                let writes = press_bar(&mut probe, Bar::Row(row), area, cell);
                let Some(Intent::Set { value, .. }) = writes.first() else {
                    panic!("no write at cell {cell}: {writes:?}");
                };
                hear(&mut probe, BALANCE, value.parse().expect("a number"));
                let _ = release(&mut probe, Some((area, cell)));
                let (hits, buffer) = draw(&probe);
                let drawn = hits.bar_area(Hit::LevelBar(row)).expect("the track");
                if drawn == area {
                    settled = Some((area, buffer));
                    break;
                }
                area = drawn;
            }
            let Some((area, buffer)) = settled else {
                continue;
            };
            let marker = (area.x..area.x + area.width)
                .find(|&x| buffer.cell((x, area.y)).expect("cell").symbol() == "┃")
                .expect("a marker");
            assert_eq!(marker, area.x + index, "{size:?}: clicked cell {index}");
        }
    }
}

/// For every track width the layouts draw, the step a cell stands for is drawn on that cell.
#[test]
fn a_cell_of_a_balance_track_is_the_one_its_value_is_drawn_on() {
    for width in 9u16..=25 {
        let bar = Rect::new(3, 0, width, 1);
        for cell in 3..3 + width {
            let value = marker_at(cell, bar, 1, 127);
            let drawn = (value - 1) * i64::from(width - 1) / 126;
            assert_eq!(drawn, i64::from(cell - 3), "width {width}, cell {cell}");
        }
    }
}

// ---- the climb stops where the user cannot see it ----

/// A climb in progress on a responsive device, then `stop` happens: no further Volume write.
fn stops_climb(stop: impl FnOnce(&mut App, &HitMap)) {
    let mut app = app_at(95);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    app.tick(t0);
    press_bar(&mut app, volume, wide(), col(127));
    let _ = release(&mut app, Some((wide(), col(127))));
    let hits = hits_of(&app, (80, 24));
    let early = run_climb(
        &mut app,
        t0,
        Duration::from_millis(20),
        Duration::from_millis(500),
    );
    assert!(!early.is_empty(), "the climb is under way");
    stop(&mut app, &hits);
    let late = run_climb(
        &mut app,
        t0 + Duration::from_millis(500),
        Duration::from_millis(20),
        Duration::from_secs(4),
    );
    assert!(late.is_empty(), "wrote after the stop: {late:?}");
}

/// A press resolved as the loop does: the climb stops unless it was on the Volume bar.
fn pressed(app: &mut App, hits: &HitMap, x: u16, y: u16) -> Option<MouseAction> {
    let action = click(hits, app, x, y);
    press_elsewhere(app, action.as_ref());
    action
}

#[test]
fn a_click_on_a_tab_stops_the_climb() {
    stops_climb(|app, hits| {
        let tab = hits.area_of(Hit::Tab(Screen::Tone)).expect("the Tone tab");
        let action = pressed(app, hits, tab.x, tab.y);
        if let Some(MouseAction::Screen(screen)) = action {
            app.show(screen);
        }
    });
}

#[test]
fn a_click_on_another_row_stops_the_climb() {
    stops_climb(|app, hits| {
        let other = row_of(app, "balance");
        let area = hits
            .area_of(Hit::Row(Pane::Controls, other))
            .expect("a row");
        let action = pressed(app, hits, area.x + 1, area.y);
        assert!(matches!(action, Some(MouseAction::Focus(..))), "{action:?}");
    });
}

#[test]
fn a_box_opening_stops_the_climb() {
    stops_climb(|app, _| {
        press(app, KeyCode::Char('s'));
        assert!(app.overlays.settings.is_some());
    });
}

#[test]
fn a_screen_change_by_key_stops_the_climb() {
    stops_climb(|app, _| {
        press(app, KeyCode::Char('t'));
    });
}

/// A press on the Volume bar itself keeps the climb: above sets a new target.
#[test]
fn a_press_on_the_volume_bar_keeps_the_climb() {
    let mut app = app_at(95);
    let volume = Bar::Row(row_of(&app, "volume"));
    press_bar(&mut app, volume, wide(), col(110));
    let action = MouseAction::BarPress {
        bar: volume,
        at: col(120),
        area: wide(),
    };
    press_elsewhere(&mut app, Some(&action));
    assert!(app.pointer.climb.is_some());
}

// ---- a lowering the device has not confirmed ----

/// Confirmed at 120, a click at 21 whose echo is lost, the wait expires, then a click at 118: the
/// raise is judged from 21, so nothing above 75% (or 21 plus a step) is written.
#[test]
fn a_raise_after_a_lowering_with_a_lost_echo_climbs_from_the_lowering() {
    let mut app = app_at(120);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    let low = press_bar(&mut app, volume, wide(), col(21));
    assert_eq!(volumes(&low), vec![21]);
    let _ = release(&mut app, Some((wide(), col(21))));
    // No echo: the wait expires and the bar still reads 120.
    app.tick(t0 + Duration::from_secs(6));
    assert!(app.readings.pending.is_empty());
    let mut sent = press_bar(&mut app, volume, wide(), col(118));
    sent.extend(release(&mut app, Some((wide(), col(118)))));
    assert!(
        highest(&sent) <= 95,
        "a raise from a stale reading: {sent:?}"
    );
    // The climb started from 21: its first write is 75%, not 121.
    assert_eq!(volumes(&sent), vec![95]);
}

/// A quick drag down to 50 and back up, released before 50 is answered: nothing above 75% (or 50
/// plus a step) is written.
#[test]
fn a_drag_down_and_back_up_before_the_answer_writes_nothing_above_the_base() {
    let mut app = app_at(120);
    let t0 = app.now;
    let volume = Bar::Row(row_of(&app, "volume"));
    let mut sent = press_bar(&mut app, volume, wide(), col(50));
    sent.extend(to(&mut app, wide(), col(118)));
    sent.extend(release(&mut app, Some((wide(), col(118)))));
    for tenth in 1..=20 {
        app.tick(t0 + Duration::from_millis(100 * tenth));
        sent.extend(confirmed(&mut app));
    }
    assert!(highest(&sent) <= 95, "{sent:?}");
}

/// Once the device has confirmed the value sent, a later raise made elsewhere (the knob) is the
/// base: a small lowering click above 75% writes its value at once rather than dipping to the
/// older sent value and climbing back.
#[test]
fn a_confirmed_send_leaves_the_reading_as_the_base() {
    let mut app = app_at(120);
    let volume = Bar::Row(row_of(&app, "volume"));
    let mut lowered = press_bar(&mut app, volume, wide(), col(118));
    lowered.extend(release(&mut app, Some((wide(), col(118)))));
    assert_eq!(volumes(&lowered), vec![118]);
    answer(&mut app);
    assert!(
        app.readings.sent.is_empty(),
        "the confirmed send is still the base"
    );
    // The knob raises the volume on the device itself.
    hear(&mut app, VOLUME, 125);
    let mut clicked = press_bar(&mut app, volume, wide(), col(122));
    clicked.extend(release(&mut app, Some((wide(), col(122)))));
    assert_eq!(volumes(&clicked), vec![122], "the lowering dipped first");
}

/// The wheel stops the climb where it stands, as a press elsewhere does; moves and releases do
/// not.
#[test]
fn the_wheel_stops_the_climb() {
    let mut app = app_at(95);
    let volume = Bar::Row(row_of(&app, "volume"));
    press_bar(&mut app, volume, wide(), col(127));
    let drag = mouse_at(MouseEventKind::Drag(MouseButton::Left), 0, 0);
    mouse_event_unmatched(&mut app, drag.kind);
    assert!(app.pointer.climb.is_some(), "a drag is not a press");
    let hits = hits_of(&app, (80, 24));
    let area = hits
        .bar_area(Hit::LevelBar(row_of(&app, "volume")))
        .expect("the bar");
    let wheel = mouse_at(MouseEventKind::ScrollUp, area.x, area.y);
    let action = mouse::resolve(&hits, wheel, &app);
    assert!(matches!(action, Some(MouseAction::Intent(_))));
    super::mouse_event(&mut app, wheel.kind, action.as_ref());
    assert!(app.pointer.climb.is_none());
}

fn mouse_event_unmatched(app: &mut App, kind: MouseEventKind) {
    super::mouse_event(app, kind, None);
}

// ---- the keys and the wheel above three quarters ----

/// A one-notch raise of the Volume, as `+`, `→` on its row and the wheel send it.
fn up() -> Intent {
    Intent::Set {
        key: "volume".into(),
        value: "up".into(),
        step: 1,
    }
}

/// Above three quarters, a key held down at the terminal's repeat rate (30 a second) raises the
/// Volume no faster than the glide: over 3.3 s, a tenth of the range a second and the step and a
/// half a rest leaves, never more.
#[test]
fn a_held_key_above_three_quarters_steps_no_faster_than_the_glide() {
    let mut app = app_at(100);
    let start = app.now;
    let gone = (0..100u64)
        .filter(|&n| app.paces(&up(), start + Duration::from_millis(33 * n)))
        .count();
    assert!((40..=43).contains(&gone), "{gone} of 100 presses went");
}

/// Below three quarters, on the way down and on another control, every press goes at once: from
/// 50, the 45 raises that keep the reading and the raises ahead of it below 95.
#[test]
fn below_three_quarters_and_down_every_press_goes() {
    let mut app = app_at(50);
    let start = app.now;
    assert!(
        (0..45u64).all(|n| app.paces(&up(), start + Duration::from_millis(n))),
        "50 + 45 is 95"
    );
    let mut app = app_at(120);
    let down = Intent::Set {
        key: "volume".into(),
        value: "down".into(),
        step: 1,
    };
    assert!((0..100u64).all(|n| app.paces(&down, start + Duration::from_millis(n))));
    let balance = Intent::Set {
        key: "balance".into(),
        value: "up".into(),
        step: 1,
    };
    assert!((0..100u64).all(|n| app.paces(&balance, start + Duration::from_millis(n))));
}

/// After a rest, the first raise above three quarters goes at once, and the next waits its turn.
#[test]
fn after_a_rest_the_first_raise_above_three_quarters_goes_at_once() {
    let mut app = app_at(95);
    let start = app.now;
    assert!(app.paces(&up(), start));
    assert!(!app.paces(&up(), start + Duration::from_millis(10)));
    assert!(app.paces(&up(), start + Duration::from_secs(5)));
    let mut app = app_at(94);
    let gone = (0..10u64)
        .filter(|&n| app.paces(&up(), start + Duration::from_millis(n)))
        .count();
    assert_eq!(
        gone, 2,
        "94 + 1 is 95, then the glide's pace: one step of a rested credit"
    );
}

/// The raises sent ahead of the Volume's echo count toward three quarters, since the reading lags
/// the keys: from 90, five go at once (90 + 5 is 95), then the glide's pace; once the reading
/// moves, the raises are counted from it afresh.
#[test]
fn presses_ahead_of_the_reading_count_toward_three_quarters() {
    let mut app = app_at(90);
    let start = app.now;
    let gone = (0..10u64)
        .filter(|&n| app.paces(&up(), start + Duration::from_millis(n)))
        .count();
    assert_eq!(
        gone, 6,
        "five below three quarters and one of a rested credit"
    );
    hear(&mut app, VOLUME, 80);
    assert!(
        (10..20u64).all(|n| app.paces(&up(), start + Duration::from_millis(n))),
        "80 + 10 is 90"
    );
}

/// A reading that rises covers only as many of the raises sent as it rose by: from 92, three go
/// at once (92 + 3 is 95); the reading at 93 still has two on its way, so the next press is judged
/// from 95 and spends the rested credit, and the one after it has none left.
#[test]
fn a_rise_of_the_reading_covers_only_the_raises_it_shows() {
    let mut app = app_at(92);
    let start = app.now;
    assert!((0..3u64).all(|n| app.paces(&up(), start + Duration::from_millis(n))));
    hear(&mut app, VOLUME, 93);
    assert!(app.paces(&up(), start + Duration::from_millis(3)));
    assert!(!app.paces(&up(), start + Duration::from_millis(4)));
}

/// A raise by several notches costs as many steps of credit, all or nothing: from 100 a rested
/// credit of a step and a half pays for one notch but not for two at once.
#[test]
fn a_raise_by_several_notches_costs_that_many_steps() {
    let mut app = app_at(100);
    let start = app.now;
    let by = |step| Intent::Set {
        key: "volume".into(),
        value: "up".into(),
        step,
    };
    assert!(!app.paces(&by(2), start), "two notches, a step and a half");
    assert!(app.paces(&by(1), start), "the refusal kept the credit");
    assert!(!app.paces(&by(1), start + Duration::from_millis(10)));
}
