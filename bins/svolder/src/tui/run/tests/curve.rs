// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Dragging a band on the tone curve: left and right drags, catching up with the pointer, a double
//! click and a lost release.

use super::*;

/// The cells the tone-control screen draws its curve in at [`SIZE`], and the cell band
/// `band`'s point is drawn in there, on `app`.
fn curve_point(app: &App, band: usize) -> (ratatui::layout::Rect, (u16, u16)) {
    use crate::tui::screens::tone;
    let bands = app.profile.eq.as_ref().map_or(0, |eq| eq.bands.len());
    let curve = tone::curve_area(ratatui::layout::Rect::new(0, 0, SIZE.0, SIZE.1), bands);
    let point = tone::points(app, curve)
        .into_iter()
        .find(|(drawn, _)| *drawn == band)
        .map(|(_, cell)| cell)
        .unwrap_or_else(|| panic!("band {band} has no point"));
    (curve, point)
}

/// A left drag of a band's point on the curve, through the loop, moves the device's band to
/// where the button was let go: on the fake, resting in Flat, band 2 ends on the frequency
/// and the gain of that cell, and the tone control in EQ mode, as a band key would leave it.
#[tokio::test(start_paused = true)]
async fn a_left_drag_on_the_curve_moves_the_devices_band() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use svolder_profiles::{EqMode, EqParam};

    use crate::tui::screens::tone;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    assert_eq!(
        raw_of(&app, eq.mode_id()),
        Some(i64::from(EqMode::Flat as u8))
    );
    let (curve, (x, y)) = curve_point(&app, 1);
    let to = (x + 3, y - 2);
    let script = stream::iter(vec![
        mouse_at(MouseEventKind::Down(MouseButton::Left), x, y),
        mouse_at(MouseEventKind::Drag(MouseButton::Left), x + 1, y - 1),
        mouse_at(MouseEventKind::Drag(MouseButton::Left), x + 2, y - 2),
        mouse_at(MouseEventKind::Drag(MouseButton::Left), to.0, to.1),
        mouse_at(MouseEventKind::Up(MouseButton::Left), to.0, to.1),
    ])
    .chain(quit_after(Duration::from_secs(2)));
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    let app = &end.watching.app;
    let hz = eq
        .frequency_raw(1, tone::frequency_at(curve, to.0))
        .expect("a step of band 2");
    let db = eq.gain_raw(tone::gain_at(curve, to.1));
    let band = |param| eq.band_id(1, param).expect("band 2");
    assert_eq!(raw_of(app, band(EqParam::Frequency)), Some(i64::from(hz)));
    assert_eq!(raw_of(app, band(EqParam::Gain)), Some(i64::from(db)));
    assert_eq!(
        raw_of(app, eq.mode_id()),
        Some(i64::from(EqMode::Eq as u8)),
        "the device is not in EQ mode"
    );
    assert_eq!(app.pointer.curve_drag, None, "the release kept the drag");
    end.watching.close().await;
}

/// A right drag through the loop sets a band's Q from how far the pointer went sideways:
/// five cells right of band 3's point takes the fake's Q five steps down, a wider bell, and
/// what the pointer does up and down changes nothing.
#[tokio::test(start_paused = true)]
async fn a_right_drag_on_the_curve_widens_the_devices_band() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use svolder_profiles::EqParam;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    let q = eq.band_id(2, EqParam::Q).expect("band 3's Q");
    let start = raw_of(&app, q).expect("the fake reports band 3's Q");
    let (_, (x, y)) = curve_point(&app, 2);
    let mut events = vec![mouse_at(MouseEventKind::Down(MouseButton::Right), x, y)];
    for across in 1..=5 {
        events.push(mouse_at(
            MouseEventKind::Drag(MouseButton::Right),
            x + across,
            y - across % 2,
        ));
    }
    events.push(mouse_at(MouseEventKind::Up(MouseButton::Right), x + 5, y));
    let script = stream::iter(events).chain(quit_after(Duration::from_secs(2)));
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    assert_eq!(raw_of(&end.watching.app, q), Some(start - 5));
    end.watching.close().await;
}

/// The steps a drag holds back while the device answers the write before them go out once
/// it has, with the button still down: a right drag five cells along, never let go, still
/// takes the fake's Q the five steps down — the first motion's write answered, the latest of
/// the four held behind it sent then.
#[tokio::test(start_paused = true)]
async fn a_drag_catches_up_with_the_pointer_once_the_device_answers() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use svolder_profiles::EqParam;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    let q = eq.band_id(2, EqParam::Q).expect("band 3's Q");
    let start = raw_of(&app, q).expect("the fake reports band 3's Q");
    let (_, (x, y)) = curve_point(&app, 2);
    let mut events = vec![mouse_at(MouseEventKind::Down(MouseButton::Right), x, y)];
    for across in 1..=5 {
        events.push(mouse_at(
            MouseEventKind::Drag(MouseButton::Right),
            x + across,
            y,
        ));
    }
    let script = stream::iter(events).chain(quit_after(Duration::from_secs(2)));
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    // The quit question that ends the case drops the drag, writing nothing: whatever moved
    // the Q was sent while the button was still down.
    assert_eq!(raw_of(&end.watching.app, q), Some(start - 5));
    end.watching.close().await;
}

/// A double-click on band 2's gain, through the loop, opens its edit line on the gain field:
/// `3` typed over it and `Enter` write +3 dB to the fake — at the step nearest it — and, the
/// fake resting in Flat, switch it to EQ.
#[tokio::test(start_paused = true)]
async fn a_double_click_on_a_bands_gain_opens_the_line_on_it() {
    use crossterm::event::KeyCode;
    use svolder_profiles::{EqMode, EqParam};

    use crate::tui::mouse::Hit;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    let mut scratch = Terminal::new(ratatui::backend::TestBackend::new(SIZE.0, SIZE.1))
        .expect("a test terminal opens");
    let mut hits = HitMap::default();
    scratch
        .draw(|frame| {
            render::draw(frame, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let gain = hits
        .area_of(Hit::BandField(1, 1))
        .expect("band 2's gain is a target");
    let (x, y) = (gain.x + 3, gain.y);
    let press = || left_click(x, y);
    let keys =
        [KeyCode::Char('3'), KeyCode::Enter].map(|code| Ok(Event::Key(KeyEvent::from(code))));
    let script = stream::iter(vec![press(), press()])
        .chain(stream::iter(keys))
        .chain(quit_after(Duration::from_secs(2)));
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    assert!(
        any_frame(&drawn(&frames), "Esc cancels"),
        "the double-click opened no edit line"
    );
    let app = &end.watching.app;
    let band_gain = eq.band_id(1, EqParam::Gain).expect("band 2's gain");
    assert_eq!(
        raw_of(app, band_gain),
        Some(i64::from(eq.gain_raw(3.0))),
        "the 3 typed was not the gain"
    );
    assert_eq!(raw_of(app, eq.mode_id()), Some(i64::from(EqMode::Eq as u8)));
    end.watching.close().await;
}

/// A drag whose release never arrived is let go where it last was by the next press of its
/// button: band 1 dragged to a cell with no release, then a click on band 5's row, ends on
/// that cell's frequency and gain — nothing of the click's own cell reaches it — and the
/// click focuses band 5 as any click on its row does.
#[tokio::test(start_paused = true)]
async fn a_click_after_a_lost_release_lets_the_drag_go_where_it_was() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use svolder_profiles::EqParam;

    use crate::tui::screens::tone;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    let (curve, (x, y)) = curve_point(&app, 0);
    let to = (x + 2, y - 2);
    let (row_x, row_y) = coord_of(&app, "10000 Hz");
    let script = stream::iter(vec![
        mouse_at(MouseEventKind::Down(MouseButton::Left), x, y),
        mouse_at(MouseEventKind::Drag(MouseButton::Left), to.0, to.1),
        mouse_at(MouseEventKind::Down(MouseButton::Left), row_x, row_y),
        mouse_at(MouseEventKind::Up(MouseButton::Left), row_x, row_y),
    ])
    .chain(quit_after(Duration::from_secs(2)));
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    let app = &end.watching.app;
    let band = |param| eq.band_id(0, param).expect("band 1");
    let hz = eq
        .frequency_raw(0, tone::frequency_at(curve, to.0))
        .expect("a step of band 1");
    let db = eq.gain_raw(tone::gain_at(curve, to.1));
    assert_eq!(raw_of(app, band(EqParam::Frequency)), Some(i64::from(hz)));
    assert_eq!(raw_of(app, band(EqParam::Gain)), Some(i64::from(db)));
    assert_eq!(app.tone.band, 4, "the click did not focus band 5");
    end.watching.close().await;
}
