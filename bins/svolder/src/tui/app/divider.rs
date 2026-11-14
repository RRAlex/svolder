// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bar between the device screen's Controls pane and its Tone pane, which takes the mouse and
//! a few keys.
//!
//! A left press on the bar holds it ([`DividerDrag`]); dragging moves the split to where the
//! pointer is, each pane held to at least [`split::MIN_SIDE`] columns, until the button is let
//! go. The split lives in the session's `[tui]` table as `device_split`, so the screen draws it
//! as it moves; nothing is written while it does. A release, and a quiet after the last of a
//! burst of nudges (`<` and `>`), say the split is due to be written
//! ([`Pointer::split_due`](super::pointer::Pointer::split_due)), and
//! the loop writes it once through the Settings writer ([`due`]). A double-click on the bar, or
//! `|`, puts it back to an even split.
//!
//! Where a frame drew no bar to move (another screen, no Tone pane, under 80 columns) there is
//! nothing here to do: [`App::drawn`] says so, and the presses and keys ask for nothing.

use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::App;
use crate::tui::screens::device::split;

/// How long after the last nudge the split is saved: the keys auto-repeat faster than that, so a
/// burst of them is one write.
pub(crate) const NUDGE_QUIET: Duration = match cfg!(test) {
    true => Duration::from_millis(50),
    false => Duration::from_millis(500),
};

/// What the last device frame drew of the two panes' room, which a press and a key are read
/// against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split {
    /// The band the two panes and the column between them fill.
    pub band: Rect,
    /// Columns between the panes.
    pub gap: u16,
}

impl Split {
    /// The columns the two panes share.
    fn shared(self) -> u16 {
        split::shared(self.band.width, self.gap)
    }

    /// The columns the Controls pane takes now, by the session's split.
    fn controls(self, fraction: f64) -> u16 {
        split::controls_columns(fraction, self.shared())
    }
}

/// The bar held by a press, until its button is let go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DividerDrag {
    /// Columns from the pointer at the press to the Controls pane's right border, so the bar
    /// does not jump to the pointer when the press lands beside it.
    grab: i32,
}

impl App {
    /// What the last frame drew of the bar, while the screen shows it.
    fn split_drawn(&self) -> Option<Split> {
        self.drawn.split.get()
    }

    /// Set the split to `fraction`, as every move of the bar does.
    fn set_split(&mut self, fraction: f64) {
        self.tui.device_split = fraction;
    }
}

/// Hold the bar for a press at column `col`.
pub(in crate::tui) fn press(app: &mut App, col: u16) {
    let Some(drawn) = app.split_drawn() else {
        return;
    };
    let border = i32::from(drawn.band.x) + i32::from(drawn.controls(app.tui.device_split)) - 1;
    app.pointer.divider_drag = Some(DividerDrag {
        grab: border - i32::from(col),
    });
    app.pointer.split_due = None;
}

/// Move the bar held to column `col`, wherever the pointer is.
pub(in crate::tui) fn to(app: &mut App, col: u16) {
    let (Some(drag), Some(drawn)) = (app.pointer.divider_drag, app.split_drawn()) else {
        return;
    };
    let columns = i32::from(col) + drag.grab - i32::from(drawn.band.x) + 1;
    app.set_split(split::moved_to(columns, drawn.shared()));
}

/// Let go of the bar: the split it stands at is due to be written at once.
pub(in crate::tui) fn release(app: &mut App) {
    if app.pointer.divider_drag.take().is_some() {
        app.pointer.split_due = Some(Instant::now());
    }
}

/// Let go of a drag whose release never reached the TUI: a left press while the bar is held is the
/// button let go of first, somewhere the mouse was not reported. Nothing for any other event.
pub(in crate::tui) fn lost(app: &mut App, event: &MouseEvent) {
    if event.kind == MouseEventKind::Down(MouseButton::Left) {
        release(app);
    }
}

/// What mouse capture turning off leaves the bar held: its button's release goes to the terminal
/// from then on, so the bar is let go where it stands. Nothing while capture is on.
pub(in crate::tui) fn capture_off(app: &mut App) {
    if !app.mouse {
        release(app);
    }
}

/// Move the bar one nudge toward the Tone pane (`dir` above zero) or the Controls pane, and owe
/// the write a quiet from `now`. Nothing where the frame drew no bar.
pub(crate) fn nudge(app: &mut App, dir: i32, now: Instant) {
    let Some(drawn) = app.split_drawn() else {
        return;
    };
    let fraction = split::nudged(app.tui.device_split, drawn.shared(), dir);
    app.set_split(fraction);
    app.pointer.split_due = Some(now + NUDGE_QUIET);
}

/// Put the bar back to an even split, to be written at once. Nothing where the frame drew no bar.
pub(crate) fn reset(app: &mut App, now: Instant) {
    if app.split_drawn().is_none() {
        return;
    }
    app.set_split(svolder_ipc::TuiConfig::default().device_split);
    app.pointer.split_due = Some(now);
}

/// The split to write now: the one the session holds, once its write is due and no drag holds the
/// bar. Taking it clears the debt.
pub(crate) fn due(app: &mut App, now: Instant) -> Option<f64> {
    let at = app.pointer.split_due?;
    if at > now || app.pointer.divider_drag.is_some() {
        return None;
    }
    app.pointer.split_due = None;
    Some(app.tui.device_split)
}

/// The split the session holds when a write is still owed, whenever it falls due: what the loop
/// writes as it ends.
pub(crate) fn owed(app: &mut App) -> Option<f64> {
    app.pointer.divider_drag = None;
    app.pointer.split_due.take()?;
    Some(app.tui.device_split)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use svolder_ipc::TuiConfig;

    use super::*;
    use crate::tui::test_support::profile;

    /// An app whose last frame drew the bar in a band `width` columns wide with no gap, as the
    /// frames at 80 to 119 columns do.
    fn app(width: u16) -> App {
        let app = App::new(
            profile(),
            "gungnir".into(),
            &TuiConfig::default(),
            Vec::new(),
        );
        app.drawn.split.set(Some(Split {
            band: Rect::new(0, 2, width, 18),
            gap: 0,
        }));
        app
    }

    /// The columns the Controls pane takes of a band `width` wide, by what the app holds.
    fn controls(app: &App, width: u16) -> u16 {
        split::controls_columns(app.tui.device_split, width)
    }

    #[test]
    fn a_drag_moves_the_bar_to_the_pointer_without_a_jump_and_stays_within_the_limits() {
        let mut app = app(140);
        // The Controls pane's border is on column 69; a press one cell to its right holds it there.
        press(&mut app, 70);
        assert_eq!(controls(&app, 140), 70, "a press moves nothing");
        to(&mut app, 90);
        assert_eq!(controls(&app, 140), 90);
        to(&mut app, 10);
        assert_eq!(
            controls(&app, 140),
            split::MIN_SIDE,
            "held to 40 on the left"
        );
        to(&mut app, 139);
        assert_eq!(
            controls(&app, 140),
            140 - split::MIN_SIDE,
            "held to 40 on the right"
        );
    }

    #[test]
    fn nothing_is_due_while_the_bar_is_held_and_one_write_is_due_when_it_is_let_go() {
        let mut app = app(100);
        press(&mut app, 49);
        for col in 50..70 {
            to(&mut app, col);
            assert_eq!(due(&mut app, Instant::now()), None, "while held, at {col}");
        }
        release(&mut app);
        let saved = due(&mut app, Instant::now()).expect("a write falls due at once");
        assert!((saved - app.tui.device_split).abs() < f64::EPSILON);
        assert_eq!(due(&mut app, Instant::now()), None, "one write, not two");
        // A release with no drag held owes nothing.
        release(&mut app);
        assert_eq!(due(&mut app, Instant::now()), None);
    }

    #[test]
    fn a_burst_of_nudges_is_one_write_after_a_quiet() {
        let mut app = app(140);
        let start = Instant::now();
        for step in 0..5 {
            nudge(&mut app, 1, start + Duration::from_millis(10 * step));
        }
        assert_eq!(controls(&app, 140), 70 + 5 * split::NUDGE);
        assert_eq!(
            due(&mut app, start + Duration::from_millis(40)),
            None,
            "mid-burst"
        );
        let quiet = start + Duration::from_millis(40) + NUDGE_QUIET;
        assert!(due(&mut app, quiet).is_some(), "after the quiet");
        assert_eq!(due(&mut app, quiet), None, "once");
        nudge(&mut app, -1, quiet);
        assert_eq!(controls(&app, 140), 70 + 4 * split::NUDGE);
    }

    #[test]
    fn the_reset_puts_an_even_split_back_and_asks_for_the_write_at_once() {
        let mut app = app(140);
        let now = Instant::now();
        nudge(&mut app, 1, now);
        reset(&mut app, now);
        assert!((app.tui.device_split - 0.5).abs() < f64::EPSILON);
        assert!(due(&mut app, now).is_some());
    }

    #[test]
    fn where_no_bar_was_drawn_nothing_moves() {
        let mut app = app(100);
        app.drawn.split.set(None);
        let now = Instant::now();
        press(&mut app, 49);
        assert!(app.pointer.divider_drag.is_none());
        nudge(&mut app, 1, now);
        reset(&mut app, now);
        assert!((app.tui.device_split - 0.5).abs() < f64::EPSILON);
        assert_eq!(due(&mut app, now + NUDGE_QUIET), None);
    }

    #[test]
    fn a_key_or_a_lost_press_lets_go_of_the_bar_where_it_stands() {
        let mut app = app(100);
        press(&mut app, 49);
        to(&mut app, 60);
        let split_then = app.tui.device_split;
        app.key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Down,
        ));
        assert!(app.pointer.divider_drag.is_none());
        assert!((app.tui.device_split - split_then).abs() < f64::EPSILON);
        assert!(due(&mut app, Instant::now()).is_some());
        press(&mut app, 59);
        lost(&mut app, &crate::tui::test_support::click(3, 3));
        assert!(app.pointer.divider_drag.is_none());
    }
}
