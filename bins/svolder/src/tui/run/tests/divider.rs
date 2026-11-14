// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bar between the device screen's Controls and Tone panes through the loop: dragged with the
//! mouse or nudged with the keys, saved to `[tui]` once, and read again by the next run.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::*;

/// A 120-column terminal: wide enough for two panes of 40 to move in.
const WIDE: (u16, u16) = (120, 24);

/// One mouse event of the left button at `(column, row)`.
fn left(kind: fn(MouseButton) -> MouseEventKind, column: u16, row: u16) -> io::Result<Event> {
    Ok(Event::Mouse(MouseEvent {
        kind: kind(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }))
}

/// Run the loop at 120×24 over `script` and answer the `device_split` the configuration file
/// holds after it, `None` where the run wrote no such key.
async fn saved_after(script: Vec<io::Result<Event>>) -> (Option<f64>, std::path::PathBuf) {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    let (recorder, _frames) = Recorder::new(WIDE.0, WIDE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let look = look();
    let path = look.path().to_path_buf();

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: stream::iter(script),
        mouse: &mut mouse,
        title: &mut title(),
        look,
    })
    .await;
    assert_eq!(end.code, 0, "{:?}", end.why);
    end.watching.close().await;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let split = text
        .lines()
        .find_map(|line| line.strip_prefix("device_split = "))
        .map(|value| value.parse().expect("a number"));
    (split, path)
}

/// Dragging the bar from the Controls pane's border to column 69 leaves it 70 columns wide of
/// the 119 the panes share, saved as `0.588` once the button is let go; a run that starts from
/// that file draws the panes where they were left.
#[tokio::test(start_paused = true)]
async fn a_drag_of_the_bar_is_saved_when_the_button_is_let_go_and_kept_by_the_next_run() {
    let (split, path) = saved_after(vec![
        left(MouseEventKind::Down, 59, 10),
        left(MouseEventKind::Drag, 70, 10),
        left(MouseEventKind::Drag, 69, 10),
        left(MouseEventKind::Up, 69, 10),
        key('q'),
        key('q'),
    ])
    .await;
    assert_eq!(split, Some(0.588));

    let saved = Config::load_from(&path).expect("the file loads");
    assert!((saved.tui.device_split - 0.588).abs() < 1e-9);
    let app = App::new(
        crate::tui::test_support::profile(),
        "fake".into(),
        &saved.tui,
        Vec::new(),
    );
    let frames = crate::tui::screens::device::frames_of(
        ratatui::layout::Rect::new(0, 0, WIDE.0, WIDE.1),
        &app,
    );
    assert_eq!(
        frames[0].1.width, 70,
        "the next run draws the Controls pane 70 wide"
    );
}

/// A drag that is never let go, a key pressed with the button down, saves where the bar stands,
/// held to 40 columns for the Tone pane however far the pointer went.
#[tokio::test(start_paused = true)]
async fn a_drag_taken_past_the_limit_is_held_to_forty_columns_for_the_other_pane() {
    let (split, _) = saved_after(vec![
        left(MouseEventKind::Down, 59, 10),
        left(MouseEventKind::Drag, 70, 10),
        left(MouseEventKind::Drag, 110, 10),
        key('q'),
        key('q'),
    ])
    .await;
    // The keys let go of the bar where it stands, held to 40 columns for the Tone pane: 79 of 119.
    assert_eq!(split, Some(0.664));
}

/// A burst of nudges ends in one saved split, and `|` puts the even one back: the file holds the
/// default's number again, written through the same writer.
#[tokio::test(start_paused = true)]
async fn nudges_and_the_reset_key_are_saved_through_the_settings_writer() {
    // The nudge keys let go of nothing and are not a release: each is one step of four columns
    // from the even 60, and the loop ending saves what the burst left.
    let (split, _) = saved_after(vec![key('>'), key('>'), key('>'), key('q'), key('q')]).await;
    assert_eq!(split, Some(0.605));
    let (split, _) = saved_after(vec![key('<'), key('q'), key('q')]).await;
    assert_eq!(split, Some(0.471));
    // Reset after a nudge: the even split is the file's own already, so nothing differs.
    let (split, _) = saved_after(vec![key('>'), key('|'), key('q'), key('q')]).await;
    assert_eq!(split, None);
}

/// A double-click on the bar resets it to an even split even after a drag moved it.
#[tokio::test(start_paused = true)]
async fn a_double_click_on_the_bar_puts_the_even_split_back() {
    let (split, _) = saved_after(vec![
        left(MouseEventKind::Down, 59, 10),
        left(MouseEventKind::Drag, 69, 10),
        left(MouseEventKind::Up, 69, 10),
        // A second press on the very cell of the press before it, soon enough, is the second of
        // a double-click: the bar goes back to 60 and the file's key with it.
        left(MouseEventKind::Down, 69, 10),
        left(MouseEventKind::Down, 69, 10),
        key('q'),
        key('q'),
    ])
    .await;
    assert_eq!(split, Some(0.5));
}
