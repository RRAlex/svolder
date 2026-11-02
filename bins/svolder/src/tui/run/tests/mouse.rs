// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Mouse clicks and the wheel through the loop: capture, control rows, the Mode control and the
//! quit question.

use super::*;

/// The mouse key turns capture the other way and tells the terminal so.
#[tokio::test(start_paused = true)]
async fn the_mouse_key_turns_capture_the_other_way() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![key('M'), key('q'), key('q')]);

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

    assert_eq!(end.code, 0);
    assert_eq!(mouse.on, Some(false), "the mouse key told the terminal");
    end.watching.close().await;
}

/// A click inside the second control row's rect, as the device screen's 80×24 mockup
/// draws it: the pane's border and cursor column, then "Phase invert", the profile's
/// second writable control.
const PHASE_ROW: (u16, u16) = (2, 4);

/// A left click on a control row focuses it, moving the `▶` cursor there in the next
/// frame — the same row `Down` would move it to from the first.
#[tokio::test(start_paused = true)]
async fn a_click_on_a_control_row_focuses_it() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![
        left_click(PHASE_ROW.0, PHASE_ROW.1),
        key('q'),
        key('q'),
    ]);

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
        any_frame(&drawn(&frames), "▶ Phase invert"),
        "the click did not move the cursor to the row it landed on"
    );
    end.watching.close().await;
}

/// The wheel over a band's row is a band edit, and the loop finishes it as a band key's
/// writes are: on the fake, which rests in Flat mode, a scroll over band 2 on the
/// tone-control screen writes the band's gain and then the switch to EQ, the key line says
/// so, and the device ends in EQ mode with the band lifted. A loop that sent the writes
/// unfinished, or only the first of them, would leave the device in Flat mode.
#[tokio::test(start_paused = true)]
async fn the_wheel_over_a_band_in_flat_mode_switches_the_device_to_eq() {
    use svolder_profiles::{EqMode, EqParam, tone::preset};

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    let gain = eq.band_id(1, EqParam::Gain).expect("band 2's gain");
    assert_eq!(
        raw_of(&app, eq.mode_id()),
        Some(i64::from(EqMode::Flat as u8))
    );
    assert_eq!(raw_of(&app, gain), Some(i64::from(eq.gain_zero)));
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    // Band 2's row on the 80×24 tone-control screen.
    let scroll = Ok(Event::Mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::ScrollUp,
        column: 10,
        row: 19,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }));
    let script = stream::iter(vec![scroll]).chain(quit_after(Duration::from_secs(2)));

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
        any_frame(&drawn(&frames), preset::SWITCHED_TO_EQ),
        "the toast never said the tone control switched"
    );
    let app = &end.watching.app;
    assert_eq!(
        raw_of(app, eq.mode_id()),
        Some(i64::from(EqMode::Eq as u8)),
        "the device is not in EQ mode"
    );
    assert!(
        raw_of(app, gain).is_some_and(|raw| raw > i64::from(eq.gain_zero)),
        "band 2's gain did not rise"
    );
    end.watching.close().await;
}

/// A click on the Mode half, off its arrows, does what `f` does: on the fake, which rests
/// in Flat mode, it asks for EQ, and the device ends up reading it.
#[tokio::test(start_paused = true)]
async fn a_click_on_mode_toggles_flat_to_eq_on_the_tone_screen() {
    use svolder_profiles::EqMode;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    assert_eq!(
        raw_of(&app, eq.mode_id()),
        Some(i64::from(EqMode::Flat as u8))
    );
    let (x, y) = coord_of(&app, "Mode");
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![left_click(x, y)]).chain(quit_after(Duration::from_secs(2)));

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
        any_frame(&drawn(&frames), "Mode ‹ EQ"),
        "the click did not move the frame to EQ mode"
    );
    assert_eq!(
        raw_of(&end.watching.app, eq.mode_id()),
        Some(i64::from(EqMode::Eq as u8)),
        "the device did not end up in EQ mode"
    );
    end.watching.close().await;
}

/// A click on the Mode select's `›` steps it exactly as `←→` do, one mode at a time,
/// rather than toggling: two clicks on the fake, resting in Flat, reach EQ and then
/// Loudness — a toggle would only ever reach EQ and back.
#[tokio::test(start_paused = true)]
async fn a_click_on_the_mode_arrow_steps_it_on_the_tone_screen() {
    use svolder_profiles::EqMode;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    assert_eq!(
        raw_of(&app, eq.mode_id()),
        Some(i64::from(EqMode::Flat as u8))
    );
    // The arrow's own column stays put across a mode change: the Mode half grows or
    // shrinks from its own end, anchored on the Loudness half beside it.
    let (x, y) = coord_of(&app, "›");
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![left_click(x, y), left_click(x, y)])
        .chain(quit_after(Duration::from_secs(2)));

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
        any_frame(&drawn(&frames), "Mode ‹ Loudness"),
        "the frame never reached Loudness mode"
    );
    assert_eq!(
        raw_of(&end.watching.app, eq.mode_id()),
        Some(i64::from(EqMode::Loudness as u8)),
        "two steps from Flat did not reach Loudness"
    );
    end.watching.close().await;
}

/// A click on the Loudness half but its bar toggles the loudness mode and, clicked again,
/// comes back to the mode it left: on the fake, resting in Flat, on then off reaches
/// Loudness and then Flat again, never EQ.
#[tokio::test(start_paused = true)]
async fn a_click_on_loudness_toggles_it_and_back_on_the_tone_screen() {
    use svolder_profiles::EqMode;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.show(crate::tui::app::Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    assert_eq!(
        raw_of(&app, eq.mode_id()),
        Some(i64::from(EqMode::Flat as u8))
    );
    let (x, y) = coord_of(&app, "Loudness");
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![left_click(x, y), left_click(x, y)])
        .chain(quit_after(Duration::from_secs(2)));

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
        any_frame(&drawn(&frames), "Mode ‹ Loudness"),
        "the first click never turned loudness on"
    );
    assert_eq!(
        raw_of(&end.watching.app, eq.mode_id()),
        Some(i64::from(EqMode::Flat as u8)),
        "the second click did not come back to Flat, the mode the first one left"
    );
    end.watching.close().await;
}

/// The Tone pane's Mode row on the device screen takes the same click as the tone
/// screen's own header: on the fake, resting in Flat, it asks for EQ, and the keys go to the
/// Tone pane with it.
#[tokio::test(start_paused = true)]
async fn a_click_on_mode_toggles_it_on_the_device_screens_tone_pane() {
    use svolder_profiles::EqMode;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    assert_eq!(
        raw_of(&app, eq.mode_id()),
        Some(i64::from(EqMode::Flat as u8))
    );
    let (x, y) = coord_of(&app, "Mode");
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![left_click(x, y)]).chain(quit_after(Duration::from_secs(2)));

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
    assert_eq!(
        raw_of(&end.watching.app, eq.mode_id()),
        Some(i64::from(EqMode::Eq as u8)),
        "the device did not end up in EQ mode"
    );
    assert_eq!(
        end.watching.app.pane,
        crate::tui::app::Pane::Tone,
        "the click left the keys in the Controls pane"
    );
    end.watching.close().await;
}

/// The same click with capture off does nothing at all: the loop never resolves a mouse
/// event while `app.mouse` is false.
#[tokio::test(start_paused = true)]
async fn a_click_with_capture_off_does_nothing() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = false;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![
        left_click(PHASE_ROW.0, PHASE_ROW.1),
        key('q'),
        key('q'),
    ]);

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
        !any_frame(&drawn(&frames), "▶ Phase invert"),
        "a click was acted on with capture off"
    );
    assert!(
        any_frame(&drawn(&frames), "▶ Input"),
        "the selection moved even though capture is off"
    );
    end.watching.close().await;
}

/// With `i d d` typed in the menu, a click on its Quit entry quits: the mouse clears the code
/// matcher first, so the `q` the click stands for is not swallowed as the code's last key.
#[tokio::test(start_paused = true)]
async fn a_click_on_quit_after_i_d_d_in_the_menu_quits() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.mascot.capable = true;
    app.tui.confirm_quit = false;
    app.overlays.menu = Some(0);
    let (x, y) = coord_of(&app, "Quit");
    let script =
        stream::iter(vec![key('i'), key('d'), key('d'), left_click(x, y)]).chain(stream::pending());
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
    end.watching.close().await;
}

/// A left click on the painted squirrel, through the loop, plays its hop.
#[tokio::test(start_paused = true)]
async fn a_left_click_on_the_painted_squirrel_plays_the_hop() {
    use crate::tui::decoration::mascot::tricks::Trick;
    use crate::tui::decoration::mascot::{Code, Place, View, What};

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.mascot.capable = true;
    let began = std::time::Instant::now()
        .checked_sub(Duration::from_secs(10))
        .expect("the clock has run ten seconds");
    app.mascot.code(Code::Konami, began);
    app.mascot.observe(&View::bare(), std::time::Instant::now());
    assert_eq!(
        app.mascot.what(),
        Some((
            What::State(crate::tui::decoration::mascot::tricks::State::Rest),
            Place::Home
        ))
    );
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(SIZE.0, SIZE.1))
        .expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let painted = hits
        .area_of(crate::tui::mouse::Hit::Mascot)
        .expect("the squirrel is painted at home");
    let script = stream::iter(vec![left_click(painted.x, painted.y)]).chain(stream::pending());
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

    assert_eq!(end.code, 1, "{:?}", end.why);
    // The hop was started, not "still playing when the loop ends": the loop clocks the mascot
    // with the real `Instant`, so how long the hop has played by then depends on the machine.
    assert_eq!(
        end.watching.app.mascot.last_visit(),
        Some(What::Trick(Trick::Hop))
    );
    end.watching.close().await;
}
