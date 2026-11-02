// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The question `q` asks before the TUI quits, over whatever screen is showing.
//!
//! It is a modal box like the help overlay: everything around it is dimmed, the box is drawn in
//! the theme's own colours, and it takes every key until it is answered. `Enter`, `y` or a
//! second `q` quits and `Esc` or `n` stays; Ctrl-C has already quit before it is asked.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::tui::app::App;
use crate::tui::theme::Theme;
use crate::tui::widgets;

/// What the box asks.
const QUESTION: &str = " Quit svolder? [Y/n]";
/// What it adds on a daemon's session, where quitting leaves the device connected.
const DAEMON: &str = " the daemon keeps the device connected";
/// What the box calls itself.
const TITLE: &str = " Quit ";
/// The keys that answer it, and what each does.
const KEYS: [(&str, &str); 2] = [("Enter/y/q", "quits"), ("Esc/n", "stays")];
/// Blank cells the box keeps inside its right border after its longest line, as the one space
/// every line opens with keeps off its left.
const TRAIL: u16 = 1;

/// Draw the question over `area`, and answer the box it drew in.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) -> Rect {
    let mut lines = vec![Line::from(Span::raw(QUESTION))];
    if app.session.daemon {
        lines.push(Line::from(Span::raw(DAEMON)));
    }
    lines.push(Line::default());
    lines.push(widgets::surfaces::modal_keys(&KEYS, theme));
    let widest = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(widest)
        .unwrap_or(area.width)
        .saturating_add(2 + TRAIL)
        .min(area.width);
    let height = u16::try_from(lines.len() + 2)
        .unwrap_or(area.height)
        .min(area.height);
    let box_area = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(
        Paragraph::new(lines).block(widgets::surfaces::modal_block(
            Block::bordered(),
            TITLE,
            app,
            theme,
        )),
        box_area,
    );
    box_area
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    use crate::tui::app::Screen;
    use crate::tui::mouse::HitMap;
    use crate::tui::render::{self, FrameAreas};
    use crate::tui::test_support::app_showing;
    use crate::tui::test_support::{assert_drawn_in_focus, modal_themes};

    use super::*;

    /// One frame of `app` at 80×24 drawn with `theme`, and where it put the modal box.
    fn frame(app: &App, theme: &Theme) -> (Buffer, FrameAreas) {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let mut hits = HitMap::default();
        let mut areas = FrameAreas::default();
        terminal
            .draw(|f| areas = render::draw(f, app, theme, &mut hits))
            .expect("the frame draws");
        (terminal.backend().buffer().clone(), areas)
    }

    /// The box's rows as text.
    fn boxed(buffer: &Buffer, area: Rect) -> Vec<String> {
        (area.top()..area.bottom())
            .map(|y| {
                (area.left()..area.right())
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect()
            })
            .collect()
    }

    /// `q` asks, in a box of its own over the screen it was pressed on, with the keys that
    /// answer it; a daemon's session says the device stays connected.
    #[test]
    fn q_asks_in_a_box_over_the_screen() {
        let mut app = app_showing(Screen::Device);
        app.key(KeyEvent::from(KeyCode::Char('q')));
        let (buffer, areas) = frame(&app, &Theme::new(false));
        let area = areas.modal.expect("the question is a modal box");
        assert_eq!(
            boxed(&buffer, area),
            [
                "┌ Quit ─────────────────────────┐",
                "│ Quit svolder? [Y/n]           │",
                "│                               │",
                "│ Enter/y/q quits · Esc/n stays │",
                "└───────────────────────────────┘",
            ]
        );

        app.session.daemon = true;
        let (buffer, areas) = frame(&app, &Theme::new(false));
        let area = areas.modal.expect("the question is a modal box");
        assert_eq!(
            boxed(&buffer, area),
            [
                "┌ Quit ─────────────────────────────────┐",
                "│ Quit svolder? [Y/n]                   │",
                "│ the daemon keeps the device connected │",
                "│                                       │",
                "│ Enter/y/q quits · Esc/n stays         │",
                "└───────────────────────────────────────┘",
            ]
        );
    }

    /// The question is drawn as the other modal boxes are: in the theme's own colours, with
    /// everything around it dimmed.
    #[test]
    fn the_question_is_drawn_in_the_themes_colours() {
        for (depth, theme) in modal_themes() {
            let mut app = app_showing(Screen::Tone);
            app.session.daemon = true;
            app.key(KeyEvent::from(KeyCode::Char('q')));
            let (buffer, areas) = frame(&app, &theme);
            let area = areas.modal.expect("the question is a modal box");
            assert_drawn_in_focus(&buffer, area, &theme, depth);
        }
    }
}
