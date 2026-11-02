// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The exit that plays before the screen, and the synchronized frames that bracket it.

use super::*;

/// The loop brackets its frames in synchronized output while the intro's exit plays, and not
/// otherwise.
#[tokio::test(start_paused = true)]
async fn the_loop_brackets_its_frames_only_while_the_exit_plays() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use ratatui::layout::Rect;
    use svolder_ipc::{ColorDepth, TuiConfig};

    use crate::tui::terminal::SyncEdge;

    for handed in [true, false] {
        let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
        let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
        let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
        let mut mouse = SpyMouse::default();
        let mut look = Look::new(
            TuiConfig::default(),
            false,
            ColorDepth::TrueColor,
            scratch_config(),
        );
        let marks = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&marks);
        look.sync = Box::new(move |edge| seen.borrow_mut().push(edge));
        if handed {
            let area = Rect::new(0, 0, SIZE.0, SIZE.1);
            look.effects
                .hand_over(crate::tui::decoration::intro::Reform::for_test(
                    area,
                    1,
                    &look.theme,
                ));
        }
        let end = event_loop(LoopParts {
            opener: &opener_over(never_opens()),
            adder: &nothing_to_add(),
            watching: watching(backend, app, None),
            terminal: &mut terminal,
            events: quit_after(Duration::from_millis(400)),
            mouse: &mut mouse,
            title: &mut title(),
            look,
        })
        .await;
        end.watching.close().await;
        let marks = marks.borrow();
        assert_eq!(marks.is_empty(), !handed, "{marks:?}");
        for pair in marks.chunks(2) {
            assert_eq!(pair, [SyncEdge::Begin, SyncEdge::End]);
        }
        // A pair for each of the loop's frames while the exit plays, which outlasts this run; the
        // wait's own frames are drawn outside the loop and take none.
        let drawn = frames.lock().expect("frames mutex").len();
        let begins = marks
            .iter()
            .filter(|edge| **edge == SyncEdge::Begin)
            .count();
        match handed {
            true => assert!(
                begins >= 2 && begins <= drawn,
                "{begins} pairs, {drawn} frames"
            ),
            false => assert_eq!(begins, 0),
        }
    }
}

/// A click on a blank cell ends the exit: the loop stops bracketing its frames after it.
#[tokio::test(start_paused = true)]
async fn a_click_ends_the_exit_and_its_synchronized_frames() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use ratatui::layout::Rect;
    use svolder_ipc::{ColorDepth, TuiConfig};

    use crate::tui::terminal::SyncEdge;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let mut look = Look::new(
        TuiConfig::default(),
        false,
        ColorDepth::TrueColor,
        scratch_config(),
    );
    let marks = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&marks);
    look.sync = Box::new(move |edge| seen.borrow_mut().push(edge));
    let area = Rect::new(0, 0, SIZE.0, SIZE.1);
    look.effects
        .hand_over(crate::tui::decoration::intro::Reform::for_test(
            area,
            1,
            &look.theme,
        ));
    let script = stream::once(async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        left_click(SIZE.0 - 1, SIZE.1 - 1)
    })
    .chain(quit_after(Duration::from_millis(400)));
    let script: BoxStream<'static, io::Result<Event>> = Box::pin(script);
    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look,
    })
    .await;
    end.watching.close().await;
    let marks = marks.borrow();
    let begins = marks.iter().filter(|m| **m == SyncEdge::Begin).count();
    assert!(begins > 0, "the exit played before the click");
    let total = drawn(&frames).len();
    assert!(
        begins * 2 < total,
        "{begins} of {total} frames bracketed: the click did not end the exit"
    );
    for pair in marks.chunks(2) {
        assert_eq!(pair, [SyncEdge::Begin, SyncEdge::End]);
    }
}
