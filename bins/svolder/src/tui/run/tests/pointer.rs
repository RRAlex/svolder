// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A pointer resting on a tab or on the web box's link: the tooltip, the note of the link and the
//! hyperlink written after the frame.

use super::*;

/// A pointer resting on a tab shows its tooltip once the delay has passed, and the loop draws
/// it without another event arriving: the pending tooltip shortens the tick. The delay is read
/// from the wall clock, so this case runs in real time.
#[tokio::test]
async fn a_pointer_resting_on_a_tab_shows_its_tooltip_through_the_loop() {
    use crossterm::event::MouseEventKind;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    let (x, y) = coord_of(&app, "EQ");
    let script = stream::iter(vec![mouse_at(MouseEventKind::Moved, x, y)])
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
    let frames = drawn(&frames);
    assert!(
        any_frame(&frames, "EQ · the tone control · e"),
        "the loop never drew the tooltip"
    );
    assert!(
        !frames
            .iter()
            .take(2)
            .any(|frame| frame.contains("the tone control")),
        "it shows before the delay"
    );
    end.watching.close().await;
}

/// Pointer motion that leaves the tooltip as it was draws no frame: thirty more moves within the
/// one target the pointer rests on cost no more frames than the first alone.
#[tokio::test(start_paused = true)]
async fn motion_within_one_target_draws_no_frame() {
    use crossterm::event::MouseEventKind;

    async fn frames_for(moves: u16) -> usize {
        let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
        app.mouse = true;
        let (x, y) = coord_of(&app, "Telemetry");
        let events: Vec<_> = (0..=moves)
            .map(|n| mouse_at(MouseEventKind::Moved, x + n % 4, y))
            .collect();
        let script = stream::iter(events).chain(quit_after(Duration::from_millis(50)));
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
        end.watching.close().await;
        drawn(&frames).len()
    }

    let (many, one) = (frames_for(30).await, frames_for(0).await);
    assert!(
        many <= one + 2,
        "{many} frames for thirty moves, {one} for one (a tick or two of slack)"
    );
}

/// The pointer resting on the Web UI box's link is noted for the next frame, which underlines the
/// link's lines; leaving it, or the box closing under it, clears the note.
#[tokio::test(start_paused = true)]
async fn the_pointer_over_the_web_boxs_link_is_noted_and_cleared() {
    use crossterm::event::MouseEventKind;

    async fn over_after(on_link: bool, close: bool) -> bool {
        let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
        app.mouse = true;
        app.overlays.settings = Some(crate::tui::app::SettingsBox {
            category: crate::tui::screens::settings::CONNECT,
            row: 0,
        });
        let until = std::time::Instant::now() + Duration::from_secs(90);
        app.overlays.conn.modal = Some(crate::tui::boxes::connections::Modal::Web(
            crate::tui::boxes::web_box::WebBox {
                link: Some(crate::tui::boxes::web_box::Code {
                    text: "http://192.168.1.20:8738/#login=abc".into(),
                    until,
                }),
                ..Default::default()
            },
        ));
        let (x, y) = coord_of(&app, "http://192.168.1.20");
        let at = if on_link { (x + 3, y) } else { (x + 3, y - 3) };
        let esc = || Ok(Event::Key(KeyEvent::from(crossterm::event::KeyCode::Esc)));
        let keys = if close {
            vec![esc(), esc()]
        } else {
            Vec::new()
        };
        let script = stream::iter(vec![mouse_at(MouseEventKind::Moved, at.0, at.1)])
            .chain(stream::iter(keys))
            .chain(match close {
                true => quit_after(Duration::from_millis(50)),
                // `q` would close the box; `x` is swallowed by it, and the input ends after.
                false => Box::pin(
                    stream::once(async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        key('x')
                    })
                    .chain(stream::iter(vec![key('x')])),
                ),
            });
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
        // The box swallows other keys while it is open, which ends the loop with the input.
        assert_eq!(end.code, u8::from(!close), "{:?}", end.why);
        let over = end.watching.app.pointer.over_link;
        end.watching.close().await;
        over
    }

    assert!(
        over_after(true, false).await,
        "the pointer on the link was not noted"
    );
    assert!(
        !over_after(false, false).await,
        "the pointer off the link was noted"
    );
    assert!(!over_after(true, true).await, "the note outlived the box");
}

/// A press anywhere ends the pointer's rest on the link, as the next move finds it again.
#[tokio::test(start_paused = true)]
async fn a_press_clears_the_pointers_rest_on_the_link() {
    use crossterm::event::{MouseButton, MouseEventKind};

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    app.overlays.settings = Some(crate::tui::app::SettingsBox {
        category: crate::tui::screens::settings::CONNECT,
        row: 0,
    });
    app.overlays.conn.modal = Some(crate::tui::boxes::connections::Modal::Web(
        crate::tui::boxes::web_box::WebBox {
            link: Some(crate::tui::boxes::web_box::Code {
                text: "http://192.168.1.20:8738/#login=abc".into(),
                until: std::time::Instant::now() + Duration::from_secs(90),
            }),
            ..Default::default()
        },
    ));
    let (x, y) = coord_of(&app, "http://192.168.1.20");
    let script = stream::iter(vec![
        mouse_at(MouseEventKind::Moved, x + 3, y),
        mouse_at(MouseEventKind::Down(MouseButton::Right), x + 3, y - 3),
    ])
    .chain(quit_after(Duration::from_millis(50)));
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
    assert!(!end.watching.app.pointer.over_link);
    end.watching.close().await;
}

/// The loop makes the Web UI box's link a hyperlink after the frame that draws it, and writes
/// nothing of the kind while no link is on show.
#[tokio::test(start_paused = true)]
async fn the_loop_writes_the_web_boxs_link_as_a_hyperlink_after_drawing_it() {
    async fn written(with_link: bool) -> String {
        let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
        app.overlays.settings = Some(crate::tui::app::SettingsBox {
            category: crate::tui::screens::settings::CONNECT,
            row: 0,
        });
        let link = with_link.then(|| crate::tui::boxes::web_box::Code {
            text: "http://192.168.1.20:8738/#login=abc".into(),
            until: std::time::Instant::now() + Duration::from_secs(90),
        });
        app.overlays.conn.modal = Some(crate::tui::boxes::connections::Modal::Web(
            crate::tui::boxes::web_box::WebBox {
                link,
                ..Default::default()
            },
        ));
        let esc = || Ok(Event::Key(KeyEvent::from(crossterm::event::KeyCode::Esc)));
        let script = stream::iter(vec![esc(), esc()]).chain(quit_after(Duration::from_millis(50)));
        let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
        let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
        let mut mouse = SpyMouse::default();
        let bytes = Arc::new(Mutex::new(Vec::<u8>::new()));
        let mut look = look();
        let record = Arc::clone(&bytes);
        look.link_out = Box::new(move |out| record.lock().unwrap().extend_from_slice(out));
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
        assert_eq!(end.code, 0, "{:?}", end.why);
        end.watching.close().await;
        let bytes = bytes.lock().unwrap().clone();
        String::from_utf8(bytes).expect("text")
    }

    let out = written(true).await;
    assert!(
        out.contains("\x1b]8;id=svolder-web-link;http://192.168.1.20:8738/#login=abc\x1b\\"),
        "{out:?}"
    );
    assert!(out.contains("\x1b]8;;\x1b\\"), "{out:?}");
    assert_eq!(written(false).await, "", "a hyperlink with no link on show");
}
