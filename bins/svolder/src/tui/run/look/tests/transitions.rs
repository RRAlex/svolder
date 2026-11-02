// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The noise and the melt that a screen change plays, and what ends or cuts them short.

use super::*;

/// A look on `transition` over a monochrome 16-colour terminal, where the rest of the
/// animation layer shows nothing: the noise and the melt are drawn there all the same.
fn transition_look(transition: Transition) -> Look {
    let tui = TuiConfig {
        transition,
        ..TuiConfig::default()
    };
    Look::new(tui, true, ColorDepth::Ansi16, scratch_config())
}

/// The text of every cell of `buffer`, which is what a passage changes and a screen says.
fn said(buffer: &Buffer) -> Vec<String> {
    buffer
        .content()
        .iter()
        .map(|cell| cell.symbol().to_string())
        .collect()
}

/// How many cells of `frame` say something other than `app`'s own screen does: what a
/// passage over it is covering, and nothing at all once it has played out.
fn covering(frame: &Buffer, app: &App, look: &Look, size: (u16, u16)) -> usize {
    said(frame)
        .iter()
        .zip(said(&bare(app, look, size)))
        .filter(|(drawn, screen)| **drawn != *screen)
        .count()
}

/// A screen change the user asks for plays the noise from the very frame that first draws
/// the new screen — every cell covered, however long the loop waited before that frame —
/// on a monochrome terminal the rest of the layer shows nothing on; the loop redraws fast
/// while it plays; a key pressed meanwhile acts on the new screen at once; and once its
/// 250 ms are up the frame is that screen alone, the key's work on it included, and the
/// loop is back at its slow tick. The very first frame, which no key asked for, plays
/// nothing.
#[test]
fn the_noise_plays_from_the_frame_that_first_draws_the_new_screen() {
    let mut app = gungnir();
    let mut look = transition_look(Transition::Noise);
    assert!(!look.effects.enabled(), "the layer runs in monochrome");
    let first = turn(&mut app, &mut look, SIZE, Duration::ZERO);
    assert_eq!(
        covering(&first, &app, &look, SIZE),
        0,
        "the first frame played the noise"
    );

    app.key(KeyEvent::from(KeyCode::Char('e')));
    assert_eq!(app.screen, Screen::Tone);
    let cells = usize::from(SIZE.0) * usize::from(SIZE.1);
    let covered = turn(&mut app, &mut look, SIZE, Duration::from_secs(5));
    assert_eq!(
        noisy(&covered),
        cells,
        "the new screen showed bare through the noise"
    );
    assert_eq!(
        tick_period(&app, &look.effects),
        FAST_TICK,
        "the noise redraws slowly"
    );

    app.key(KeyEvent::from(KeyCode::Char('3')));
    assert_eq!(
        app.tone.band, 2,
        "a key pressed during the noise waited for it"
    );
    let thinning = turn(&mut app, &mut look, SIZE, Duration::from_millis(100));
    let covered = covering(&thinning, &app, &look, SIZE);
    assert!(
        (1..cells).contains(&covered),
        "{covered} of {cells} cells are covered 100 ms in"
    );

    let done = turn(&mut app, &mut look, SIZE, Duration::from_millis(200));
    assert_eq!(
        covering(&done, &app, &look, SIZE),
        0,
        "the frame after the noise is not the new screen alone, band 3 focused on it"
    );
    assert!(!look.effects.transitioning());
    assert_eq!(
        tick_period(&app, &look.effects),
        Duration::from_millis(250),
        "the loop held its fast tick past the noise"
    );
}

/// A screen change the user asks for melts the last frame the terminal showed away from
/// the very frame that first draws the new screen: that frame is the old screen exactly,
/// however long the loop waited before it, on a monochrome terminal the rest of the layer
/// shows nothing on; the loop redraws fast while it plays; a key pressed meanwhile acts on
/// the new screen at once, which shows above the falling columns; and once its 600 ms are
/// up the frame is that screen alone and the loop is back at its slow tick.
#[test]
fn the_melt_plays_from_the_frame_that_first_draws_the_new_screen() {
    let mut app = gungnir();
    let mut look = transition_look(Transition::Melt);
    assert!(!look.effects.enabled(), "the layer runs in monochrome");
    let first = turn(&mut app, &mut look, SIZE, Duration::ZERO);
    assert_eq!(covering(&first, &app, &look, SIZE), 0);

    app.key(KeyEvent::from(KeyCode::Char('e')));
    let melting = turn(&mut app, &mut look, SIZE, Duration::from_secs(5));
    assert_eq!(
        said(&melting),
        said(&first),
        "the melt's first frame is not the screen the terminal was showing"
    );
    assert_eq!(
        tick_period(&app, &look.effects),
        FAST_TICK,
        "the melt redraws slowly"
    );

    app.key(KeyEvent::from(KeyCode::Char('3')));
    assert_eq!(
        app.tone.band, 2,
        "a key pressed during the melt waited for it"
    );
    let falling = turn(&mut app, &mut look, SIZE, Duration::from_millis(300));
    let cells = usize::from(SIZE.0) * usize::from(SIZE.1);
    let covered = covering(&falling, &app, &look, SIZE);
    assert!(
        (1..cells).contains(&covered),
        "{covered} of {cells} cells are the old screen 300 ms in"
    );
    // Every column has started to fall 150 ms in, so by 300 ms the new screen's top row
    // shows above every one of them.
    let top = said(&falling)[..usize::from(SIZE.0)].to_vec();
    let screen = said(&bare(&app, &look, SIZE))[..usize::from(SIZE.0)].to_vec();
    assert_eq!(
        top, screen,
        "the new screen does not show above the falling columns"
    );

    let done = turn(&mut app, &mut look, SIZE, Duration::from_millis(300));
    assert_eq!(
        covering(&done, &app, &look, SIZE),
        0,
        "the frame after the melt is not the new screen alone"
    );
    assert!(!look.effects.transitioning());
    assert_eq!(
        tick_period(&app, &look.effects),
        Duration::from_millis(250),
        "the loop held its fast tick past the melt"
    );
}

/// A second screen change while a transition plays cuts it short and starts its own from
/// the start — the noise covering every cell again, the melt melting the very frame the
/// first had got to — and it goes on for its own length from there, past the moment the
/// first would have ended.
#[test]
fn a_second_screen_change_cuts_the_first_short() {
    let cells = usize::from(SIZE.0) * usize::from(SIZE.1);
    for (transition, length) in [(Transition::Noise, 250), (Transition::Melt, 600)] {
        let mut app = gungnir();
        let mut look = transition_look(transition);
        turn(&mut app, &mut look, SIZE, Duration::ZERO);
        app.key(KeyEvent::from(KeyCode::Char('e')));
        turn(&mut app, &mut look, SIZE, Duration::ZERO);
        let most = Duration::from_millis(length * 4 / 5);
        let late = turn(&mut app, &mut look, SIZE, most);
        assert!(look.effects.transitioning(), "{transition:?} ended early");

        app.key(KeyEvent::from(KeyCode::Char('t')));
        let again = turn(&mut app, &mut look, SIZE, Duration::from_millis(10));
        match transition {
            Transition::Melt => assert_eq!(
                said(&again),
                said(&late),
                "the second melt did not start from the frame the first had got to"
            ),
            _ => assert_eq!(
                noisy(&again),
                cells,
                "the second noise did not start from every cell covered"
            ),
        }
        turn(&mut app, &mut look, SIZE, most);
        assert!(
            look.effects.transitioning(),
            "{transition:?} ended with the first change's length rather than its own"
        );
        let done = turn(&mut app, &mut look, SIZE, Duration::from_millis(length));
        assert!(
            !look.effects.transitioning(),
            "the second {transition:?} outlasted its own length"
        );
        assert_eq!(covering(&done, &app, &look, SIZE), 0);
    }
}

/// Nothing is a screen change for the noise or the melt but a screen the user moved to:
/// with the layer's own switch off a screen change plays neither, and neither the help
/// overlay nor the quit question opening or closing, nor a resize, plays anything; a
/// resize while one plays ends it.
#[test]
fn no_transition_plays_with_the_layer_off_on_a_modal_or_a_resize() {
    for transition in [Transition::Noise, Transition::Melt] {
        let mut app = gungnir();
        let mut off = Look::new(
            TuiConfig {
                transition,
                effects: EffectsConfig::off(),
                ..TuiConfig::default()
            },
            true,
            ColorDepth::Ansi16,
            scratch_config(),
        );
        turn(&mut app, &mut off, SIZE, Duration::ZERO);
        app.key(KeyEvent::from(KeyCode::Char('e')));
        let frame = turn(&mut app, &mut off, SIZE, Duration::ZERO);
        assert_eq!(
            covering(&frame, &app, &off, SIZE),
            0,
            "{transition:?} with E off"
        );
        assert!(
            !off.effects.transitioning(),
            "{transition:?} plays with the layer off"
        );

        let mut look = transition_look(transition);
        let mut app = gungnir();
        turn(&mut app, &mut look, SIZE, Duration::ZERO);
        for (what, key) in [
            ("the help overlay opening", KeyCode::Char('?')),
            ("the help overlay closing", KeyCode::Esc),
            ("the quit question", KeyCode::Char('q')),
            ("the quit question answered no", KeyCode::Esc),
        ] {
            app.key(KeyEvent::from(key));
            let frame = turn(&mut app, &mut look, SIZE, Duration::ZERO);
            assert_eq!(
                covering(&frame, &app, &look, SIZE),
                0,
                "{what} played {transition:?}"
            );
        }
        assert_eq!(app.screen, Screen::Device, "a modal moved the screen");
        let wider = turn(&mut app, &mut look, (100, 30), Duration::ZERO);
        assert_eq!(
            covering(&wider, &app, &look, (100, 30)),
            0,
            "a resize played {transition:?}"
        );

        app.key(KeyEvent::from(KeyCode::Char('e')));
        let playing = turn(&mut app, &mut look, (100, 30), Duration::ZERO);
        assert!(covering(&playing, &app, &look, (100, 30)) > 0);
        let resized = turn(&mut app, &mut look, SIZE, Duration::from_millis(10));
        assert_eq!(
            covering(&resized, &app, &look, SIZE),
            0,
            "{transition:?} went on over a resized terminal"
        );
        assert!(
            !look.effects.transitioning(),
            "a resize left {transition:?} running"
        );
    }
}

/// A toast is neither covered by the noise a screen change plays nor changed by any effect or
/// dimming: its cells come out of the painted frame as the widgets drew them, over a modal
/// box too.
#[test]
fn the_noise_and_the_dimming_leave_a_toast_as_drawn() {
    let mut app = gungnir();
    let mut look = transition_look(Transition::Noise);
    turn(&mut app, &mut look, SIZE, Duration::ZERO);
    app.key(KeyEvent::from(KeyCode::Char('e')));
    app.key(KeyEvent::from(KeyCode::Char('?')));
    assert!(app.overlays.help_open);
    app.warn("settings not saved: disk full");
    let covered = turn(&mut app, &mut look, SIZE, Duration::from_secs(5));
    let plain = bare(&app, &look, SIZE);
    assert!(noisy(&covered) > 0, "the noise did not play");
    let (toast, _) = crate::tui::boxes::toast::area(
        Rect::new(0, 0, SIZE.0, SIZE.1),
        "settings not saved: disk full",
    );
    for y in toast.top()..toast.bottom() {
        for x in toast.left()..toast.right() {
            assert_eq!(covered.cell((x, y)), plain.cell((x, y)), "({x},{y})");
        }
    }
}
