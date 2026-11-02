// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The keys that end the loop: `q`, the question it asks, `Ctrl-C`, and a key after a transition.

use super::*;

/// `q` ends the loop with code 0 and hands the device back still connected, and releasing it,
/// as the TUI does once the terminal is restored, takes the link down, once.
#[tokio::test(start_paused = true)]
async fn q_ends_the_loop_with_nothing_to_report_and_the_device_to_release() {
    let (backend, gate, app) = fake(SHORT_DEADLINE).await;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: quit_after(Duration::from_millis(400)),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0);
    assert!(end.why.is_none(), "{:?}", end.why);
    let frames = drawn(&frames);
    assert!(
        any_frame(&frames, "● connected"),
        "the loop drew a live device"
    );
    // The device is the caller's to release, and it does so with the terminal restored.
    assert!(
        !gate.closed(),
        "the loop released the device before the terminal was restored"
    );
    end.watching.release().await;
    assert!(gate.closed(), "quitting left the link open");
    assert_eq!(gate.releases(), 1, "the link was released more than once");
}

/// `q` asks before the loop ends: `Esc` answers no and the loop goes on drawing, the next
/// `q` asks again, and `Enter` answers yes. A direct session says nothing about a daemon.
#[tokio::test(start_paused = true)]
async fn q_asks_esc_stays_and_enter_quits() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let esc = Ok(Event::Key(KeyEvent::from(crossterm::event::KeyCode::Esc)));
    let enter = Ok(Event::Key(KeyEvent::from(crossterm::event::KeyCode::Enter)));
    let script = stream::iter(vec![key('q'), esc, key('t'), key('q'), enter]);

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
    let frames = drawn(&frames);
    let asked = first_frame(&frames, "Quit svolder? [Y/n]");
    let telemetry = first_frame(&frames, " functions ");
    assert!(telemetry > asked, "Esc did not leave the loop running");
    assert!(
        any_frame(&frames[telemetry..], "Quit svolder? [Y/n]"),
        "the second q did not ask again"
    );
    assert!(
        !any_frame(&frames, "the daemon keeps"),
        "a direct session spoke of a daemon"
    );
    end.watching.close().await;
}

/// Under the noise and under the melt, on a monochrome terminal where the rest of the
/// animation layer draws nothing, the key pressed straight after one that changes screen is
/// carried out on the new screen once the loop has drawn a frame of the transition: `.` on
/// the tone screen reaches the device as band 1's frequency one step up. Under the noise the
/// frame the loop draws after the screen change is already the noise's first, every cell of
/// it covered, with no frame of the new screen bare before it.
#[tokio::test(start_paused = true)]
async fn a_key_after_a_transition_frame_reaches_the_device_from_the_new_screen() {
    use svolder_ipc::{ColorDepth, Transition};
    use svolder_profiles::EqParam;

    for transition in [Transition::Noise, Transition::Melt] {
        let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
        let eq = app.profile.eq.clone().expect("the fake has a tone control");
        let frequency = eq.band_id(0, EqParam::Frequency).expect("band 1");
        let before = raw_of(&app, frequency).expect("the fake reports band 1");
        let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
        let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
        let mut mouse = SpyMouse::default();
        let ctrl_c = Ok(Event::Key(KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::CONTROL,
        )));
        let tui = TuiConfig {
            transition,
            ..TuiConfig::default()
        };
        let look = Look::new(tui, true, ColorDepth::Ansi16, scratch_config());

        let end = event_loop(LoopParts {
            opener: &opener_over(never_opens()),
            adder: &nothing_to_add(),
            watching: watching(backend, app, None),
            terminal: &mut terminal,
            events: stream::iter(vec![key('e'), key('.'), ctrl_c]),
            mouse: &mut mouse,
            title: &mut title(),
            look,
        })
        .await;

        assert_eq!(end.code, 0, "{transition:?}: {:?}", end.why);
        let frames = drawn(&frames);
        if transition == Transition::Noise {
            let noise = |frame: &String| {
                frame
                    .chars()
                    .filter(|c| *c != '\n')
                    .all(|c| ['▓', '▒', '░'].contains(&c))
            };
            let first = frames
                .iter()
                .position(noise)
                .unwrap_or_else(|| panic!("no frame was the noise's first: {frames:#?}"));
            assert!(
                first > 0
                    && frames[..first]
                        .iter()
                        .all(|frame| frame.contains("Controls")),
                "a frame before the noise was not the device screen: {:#?}",
                &frames[..first]
            );
        }

        let mut watching = end.watching;
        let backend = watching.backend.as_mut().expect("the device is still open");
        let heard = async {
            while raw_of(&watching.app, frequency) == Some(before) {
                match backend.next_status().await.expect("the feed keeps reading") {
                    Some(update) => watching.app.apply(update, Instant::now()),
                    None => panic!("the feed ended before band 1 moved"),
                }
            }
        };
        tokio::time::timeout(ECHO_DEADLINE, heard)
            .await
            .unwrap_or_else(|_| {
                panic!("{transition:?}: the key pressed during it never reached the device")
            });
        assert!(
            raw_of(&watching.app, frequency) > Some(before),
            "{transition:?}: band 1 did not step up"
        );
        watching.close().await;
    }
}

/// Ctrl-C ends the loop at once, with the question it would otherwise ask never drawn.
#[tokio::test(start_paused = true)]
async fn ctrl_c_ends_the_loop_at_once() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let ctrl_c = Ok(Event::Key(KeyEvent::new(
        crossterm::event::KeyCode::Char('c'),
        crossterm::event::KeyModifiers::CONTROL,
    )));

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: stream::iter(vec![ctrl_c]),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    assert!(end.why.is_none(), "{:?}", end.why);
    assert!(
        !any_frame(&drawn(&frames), "Quit svolder?"),
        "Ctrl-C asked before it quit"
    );
    end.watching.close().await;
}

/// A daemon's session says, in the question, that the device stays connected: the TUI
/// opened through a daemon knows it is one.
#[tokio::test]
async fn the_question_on_a_daemon_session_says_the_device_stays_connected() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    let socket = daemon.socket.clone();
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        scratch_config(),
        move |_: OwnedGlobals| {
            let socket = socket.clone();
            async move { Ok((Backend::stub_remote(&socket).await, Vec::new())) }
        },
    );
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    assert!(
        watching.app.session.daemon,
        "a daemon's session does not know it is one"
    );
    watching
        .app
        .key(KeyEvent::from(crossterm::event::KeyCode::Char('q')));
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(SIZE.0, SIZE.1))
        .expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, &watching.app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer();
    let text: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
        .filter_map(|at| buffer.cell(at).map(|cell| cell.symbol().to_string()))
        .collect();
    assert!(
        text.contains("the daemon keeps the device connected"),
        "the question does not say the daemon holds on"
    );
    watching.close().await;
}
