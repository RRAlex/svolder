// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Particles intro as a first launch plays it, recorded frame by frame.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Size};

use crate::tui::decoration::intro::{IntroEnd, Reform};
use crate::tui::run::Look;

/// A frame drawn less than this before the next one is left out of the recording.
const DUPLICATE: Duration = Duration::from_millis(10);

/// What a recording holds: each flushed frame with the instant it was flushed at.
type Frames = Rc<RefCell<Vec<(tokio::time::Instant, Buffer)>>>;

/// A test backend that keeps a copy of its buffer at every flush, stamped with the tokio clock.
struct Recording {
    /// The backend that does the drawing.
    inner: TestBackend,
    /// Where the copies go.
    frames: Frames,
}

impl Backend for Recording {
    type Error = std::convert::Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()?;
        self.frames
            .borrow_mut()
            .push((tokio::time::Instant::now(), self.inner.buffer().clone()));
        Ok(())
    }
}

/// The Particles intro as a first launch plays it, at thirty frames a second from `seed`, and
/// the exit it hands the loop: each frame the intro drew, a frame drawn less than 10 ms before
/// the next one left out (the intro draws its last frame twice, at 3.597 s and 3.600 s).
///
/// Runs on a paused tokio clock, so the intro's sleeps cost no time.
pub(in crate::tui::screenshots) async fn intro(look: &Look, seed: u64) -> (Vec<Buffer>, Reform) {
    let frames: Frames = Rc::default();
    let backend = Recording {
        inner: TestBackend::new(super::SIZE.0, super::SIZE.1),
        frames: Rc::clone(&frames),
    };
    let mut terminal = Terminal::new(backend).expect("a terminal");
    // Kept alive so the stream stays open.
    let (mut events, _tx) = crate::tui::decoration::intro::test_event_stream();
    let mut plan = crate::tui::decoration::intro::IntroPlan::particles(seed);
    let ended =
        crate::tui::decoration::intro::play(&mut terminal, &mut events, &look.theme, &mut plan)
            .await;
    assert_eq!(ended.end, IntroEnd::Finished, "the intro runs to its end");
    let exit = ended
        .exit
        .expect("a full Particles intro hands over its exit");
    let recorded = frames.borrow();
    let kept = recorded
        .iter()
        .enumerate()
        .filter(|(n, (at, _))| {
            recorded
                .get(n + 1)
                .is_none_or(|(next, _)| *next - *at >= DUPLICATE)
        })
        .map(|(_, (_, buffer))| buffer.clone())
        .collect();
    (kept, exit)
}
