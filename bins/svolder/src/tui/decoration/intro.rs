// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The intro: the wordmark on screen while the device is being opened.
//!
//! The TUI claims the terminal before it opens anything, so the seconds a Bluetooth connect
//! takes are spent showing the mark rather than an empty screen. At true colour the mark
//! fades in from the background and holds; below it, or with the animation layer off, it is
//! drawn once and held. A demo effect (`particles`) builds the mark instead, on the pacer's
//! clock inside synchronized output, and a full run hands its exit on for the loop's first
//! frames. The licence notice stands on the last two rows, fading in with the mark where the
//! mark fades. Any key ends it early — Ctrl-C and `q` by giving up on the device
//! rather than by getting past the mark — and the mark stays on screen until the event loop
//! draws its first frame over it.
//!
//! - [`demo`]: which demo effect builds the mark, planned for a frame size.
//! - [`pace`]: how often the intro and its exit draw a frame.
//! - [`particles`]: the large mark built from sparks.
//! - [`reform`]: the exit that carries the sparks into the header's logo.
//! - [`rng`]: the intro's own randomness, seeded once per launch.
//! - [`sparks`]: what the sparks are made of, for the build and the exit alike.

use std::io;
use std::time::Duration;

use crossterm::event::{Event, KeyEventKind};
use futures::StreamExt;
use futures::stream::Stream;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use tachyonfx::{Duration as FxDuration, EffectManager, fx};
use tokio::time::Instant;

use svolder_ipc::IntroEffect;
use svolder_proto::notice;

use super::wordmark::{self, Size};
#[cfg(test)]
use crate::tui::terminal::no_sync;
use crate::tui::terminal::{FrameSync, SyncEdge};
use crate::tui::theme::{DIM_GREY, Role, Surface, Theme};
use crate::tui::widgets;
use demo::{Demo, Scene};
use sparks::Colours;

mod demo;
mod pace;
mod particles;
mod reform;
mod rng;
mod sparks;

pub(in crate::tui) use reform::{PANES_HELD_MS, Reform};

pub use pace::Pacer;
pub use rng::launch_seed;

/// How long the mark takes to fade in, in milliseconds.
const FADE_MS: u32 = 1200;
/// How long the mark holds once it has faded in, in milliseconds.
const HOLD_MS: u32 = 400;
/// How long the mark is held when it is drawn static rather than faded in, in milliseconds.
const STATIC_MS: u32 = 500;
/// How long one frame of the fade lasts: about thirty a second, the same period the event
/// loop redraws an animation at.
const FRAME: Duration = Duration::from_millis(33);
/// The columns a frame must have for the large mark; a narrower one gets the small mark.
const LARGE_WIDTH: u16 = 70;
/// The rows a frame must have for the large mark.
const LARGE_HEIGHT: u16 = 12;
/// What the mark fades in from where the theme paints no ground of its own: the dim grey, which
/// reads against a light terminal background as against a dark one, where black would not.
const FADE_FROM: Color = DIM_GREY;
/// The one effect the intro runs, keyed in its own manager.
const FADE: u8 = 0;

/// How the intro ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroEnd {
    /// It ran to its end.
    Finished,
    /// A key ended it early.
    Skipped,
    /// One of the two keys that give up on a device ended it: the user is asking to leave,
    /// not to get past the mark.
    GaveUp,
}

/// What the intro plays, settled before it starts.
pub struct IntroPlan {
    /// Whether anything animates: the animation layer runs on a true-colour terminal.
    pub animate: bool,
    /// Which effect `[tui] intro_effect` asks for.
    pub effect: IntroEffect,
    /// The launch's seed, which the effect's scatter and its exit are drawn from.
    pub seed: u64,
    /// The pace a demo effect draws at.
    pub pacer: Pacer,
    /// What brackets each frame in synchronized output.
    pub sync: FrameSync,
}

#[cfg(test)]
impl IntroPlan {
    /// Today's fade, or the static mark, with no terminal to bracket frames for.
    pub(in crate::tui) fn fade(animate: bool) -> IntroPlan {
        IntroPlan {
            animate,
            effect: IntroEffect::Fade,
            seed: 0,
            pacer: Pacer::new(true),
            sync: no_sync(),
        }
    }

    /// Particles from `seed`, at thirty frames a second so a case's frame count does not hang
    /// on the machine's speed.
    pub(in crate::tui) fn particles(seed: u64) -> IntroPlan {
        IntroPlan {
            animate: true,
            effect: IntroEffect::Particles,
            seed,
            pacer: Pacer::new(true),
            sync: no_sync(),
        }
    }
}

/// How the intro ended, and the exit it hands the loop's first frames, if it has one.
pub struct Ended {
    /// How it ended.
    pub end: IntroEnd,
    /// The exit a full demo effect hands over; none after a skip, the fade or the static mark.
    pub exit: Option<Reform>,
}

/// Show the wordmark until it has had its time, or until a key ends it.
///
/// A demo effect plays where the animation layer runs and the frame has room for the large
/// mark. Everywhere else today's fade, or the static mark, plays as [`play_still`] says.
pub async fn play<B, S>(
    terminal: &mut Terminal<B>,
    events: &mut S,
    theme: &Theme,
    plan: &mut IntroPlan,
) -> Ended
where
    B: Backend,
    S: Stream<Item = io::Result<Event>> + Unpin,
{
    let demo = match plan.animate {
        true => Demo::pick(plan.effect, plan.seed),
        false => None,
    };
    let large = terminal
        .size()
        .is_ok_and(|size| large_mark(Rect::new(0, 0, size.width, size.height)).is_some());
    match demo {
        Some(demo) if large => play_demo(terminal, events, theme, demo, plan).await,
        _ => Ended {
            end: play_still(terminal, events, theme, plan.animate, &mut plan.sync).await,
            exit: None,
        },
    }
}

/// Play `demo` to its end, redrawn at the pacer's pace, each frame bracketed in synchronized
/// output. A key ends it as it ends the fade, and a skip leaves the finished mark up. A resize
/// plans the effect again at the new size from the same seed, at the same instant; a frame
/// too small for the large mark shows the small one, whole, for the rest.
async fn play_demo<B, S>(
    terminal: &mut Terminal<B>,
    events: &mut S,
    theme: &Theme,
    demo: Demo,
    plan: &mut IntroPlan,
) -> Ended
where
    B: Backend,
    S: Stream<Item = io::Result<Event>> + Unpin,
{
    let colours = Colours::of(theme);
    let length = demo.length();
    let started = Instant::now();
    let end = started + Duration::from_secs_f32(length);
    let mut scene: Option<(Rect, Scene)> = None;
    let mut reading = true;
    loop {
        let Ok(size) = terminal.size() else {
            return Ended {
                end: IntroEnd::Finished,
                exit: None,
            };
        };
        let screen = Rect::new(0, 0, size.width, size.height);
        if scene.as_ref().is_none_or(|(area, _)| *area != screen) {
            scene = Scene::plan(demo, screen, plan.seed).map(|planned| (screen, planned));
        }
        let now = Instant::now();
        let t = now
            .saturating_duration_since(started)
            .as_secs_f32()
            .min(length);
        let cost = std::time::Instant::now();
        (plan.sync)(SyncEdge::Begin);
        let drawn = terminal.draw(|frame| match &scene {
            Some((_, scene)) => scene.frame(t, frame.buffer_mut(), theme, &colours),
            None => draw_still(frame.buffer_mut(), screen, theme),
        });
        (plan.sync)(SyncEdge::End);
        if drawn.is_err() {
            return Ended {
                end: IntroEnd::Finished,
                exit: None,
            };
        }
        plan.pacer.record(cost.elapsed());
        if now >= end {
            let exit = scene.and_then(|(_, scene)| scene.exit(plan.seed, colours, plan.pacer));
            return Ended {
                end: IntroEnd::Finished,
                exit,
            };
        }
        let due = (now + plan.pacer.period()).min(end);
        loop {
            tokio::select! {
                event = events.next(), if reading => match event {
                    Some(Ok(Event::Key(key))) if key.kind != KeyEventKind::Release => {
                        if crate::tui::waiting::gives_up(key) {
                            return Ended { end: IntroEnd::GaveUp, exit: None };
                        }
                        (plan.sync)(SyncEdge::Begin);
                        let _ = terminal.draw(|frame| {
                            let area = frame.area();
                            draw_still(frame.buffer_mut(), area, theme);
                        });
                        (plan.sync)(SyncEdge::End);
                        return Ended { end: IntroEnd::Skipped, exit: None };
                    }
                    Some(Ok(Event::Resize(..))) => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => reading = false,
                },
                () = tokio::time::sleep_until(due) => break,
            }
        }
    }
}

/// Show the wordmark until it has had its time, or until a key ends it.
///
/// With `animate`, the mark fades in over [`FADE_MS`] and holds for [`HOLD_MS`] after it,
/// redrawn every [`FRAME`]; without it (below true colour, in monochrome, or with the
/// animation layer off) the mark is drawn once and simply held for [`STATIC_MS`], with no
/// frames in between to redraw. Either way a key press ends it at once: with
/// [`IntroEnd::GaveUp`] for the two keys that give up on a device (Ctrl-C and `q`, read the same
/// way by the wait after the intro), and with [`IntroEnd::Skipped`] for every other key. A
/// resize draws the mark again at the size the terminal now is; no other event draws anything.
///
/// The frame is left with the mark on it, for the caller to hold until it has something else
/// to draw. A terminal with no room for even the small mark is left alone and the intro is
/// over before it starts.
async fn play_still<B, S>(
    terminal: &mut Terminal<B>,
    events: &mut S,
    theme: &Theme,
    animate: bool,
    sync: &mut FrameSync,
) -> IntroEnd
where
    B: Backend,
    S: Stream<Item = io::Result<Event>> + Unpin,
{
    let showing = match animate {
        true => Duration::from_millis(u64::from(FADE_MS) + u64::from(HOLD_MS)),
        false => Duration::from_millis(u64::from(STATIC_MS)),
    };
    // A fade is redrawn thirty times a second; a mark nothing animates is drawn once and held,
    // so its next frame is due only when the hold is over.
    let tick = match animate {
        true => FRAME,
        false => showing,
    };
    // The fade is one effect over the mark's own cells, run from this manager rather than the
    // screens' animation layer: it is over before the event loop that owns that one starts.
    let mut effects = EffectManager::<u8>::default();
    if animate {
        effects.add_unique_effect(FADE, fx::fade_from_fg(fade_from(theme), FADE_MS));
    }
    let started = Instant::now();
    let mut drawn_at = started;
    // A stream with nothing left to read answers the moment it is asked, so the arm reading it
    // stops being polled rather than spinning the loop.
    let mut reading = true;
    loop {
        let Ok(size) = terminal.size() else {
            return IntroEnd::Finished;
        };
        let screen = Rect::new(0, 0, size.width, size.height);
        let Some((_, mark)) = centred(screen, theme) else {
            return IntroEnd::Finished;
        };
        let notice = notice(screen, mark, theme);
        // The fade covers the notice with the mark, so both come in together.
        let faded = notice.as_ref().map_or(mark, |(_, area)| mark.union(*area));
        let now = Instant::now();
        let since = now.saturating_duration_since(drawn_at);
        drawn_at = now;
        sync(SyncEdge::Begin);
        let drawn = terminal.draw(|frame| {
            draw_still(frame.buffer_mut(), screen, theme);
            effects.process_effects(FxDuration::from(since), frame.buffer_mut(), faded);
        });
        sync(SyncEdge::End);
        if drawn.is_err() {
            return IntroEnd::Finished;
        }
        // The next frame is due a whole tick after this one, whatever arrives meanwhile: an
        // event worth no redraw leaves the deadline where it is, so a moving mouse cannot draw
        // frames of its own or hold the fade up.
        let due = now + tick;
        loop {
            tokio::select! {
                event = events.next(), if reading => match event {
                    // A release is the other half of a press, and the press is what skips —
                    // or, for the two keys that give up on a device, what leaves.
                    Some(Ok(Event::Key(key))) if key.kind != KeyEventKind::Release => {
                        return match crate::tui::waiting::gives_up(key) {
                            true => IntroEnd::GaveUp,
                            false => IntroEnd::Skipped,
                        };
                    }
                    // A terminal that changed size is drawn again at the size it now is.
                    Some(Ok(Event::Resize(..))) => break,
                    Some(Ok(_)) => {}
                    // Nothing left to read input from, so there is nothing left to skip with.
                    Some(Err(_)) | None => reading = false,
                },
                () = tokio::time::sleep_until(due) => break,
            }
        }
        if started.elapsed() >= showing {
            return IntroEnd::Finished;
        }
    }
}

/// The finished intro frame: the ground, the mark the frame has room for, centred, and the
/// notice on the last two rows.
fn draw_still(buf: &mut Buffer, screen: Rect, theme: &Theme) {
    widgets::surfaces::paint_surfaces(buf, &[], theme);
    let Some((lines, mark)) = centred(screen, theme) else {
        return;
    };
    Paragraph::new(lines).render(mark, buf);
    if let Some((lines, area)) = notice(screen, mark, theme) {
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .render(area, buf);
    }
}

/// What the mark fades in from: the theme's ground where it paints one, else [`FADE_FROM`].
fn fade_from(theme: &Theme) -> Color {
    theme.surface(Surface::Ground).unwrap_or(FADE_FROM)
}

/// The mark a frame `area` has room for, and the rows it is centred in.
///
/// The large mark needs a frame at least [`LARGE_WIDTH`] by [`LARGE_HEIGHT`]; a smaller one
/// gets the small mark, and a frame with no room for even that gets nothing at all.
fn centred(area: Rect, theme: &Theme) -> Option<(Vec<Line<'static>>, Rect)> {
    let size = match large_mark(area).is_some() {
        true => Size::Large,
        false => Size::Small,
    };
    if wordmark::width(size) > area.width || wordmark::height(size) > area.height {
        return None;
    }
    Some((wordmark::lines(size, theme), placed(area, size)))
}

/// Where the large mark stands in a frame `area`, centred; `None` for a frame too small for it.
fn large_mark(area: Rect) -> Option<Rect> {
    (area.width >= LARGE_WIDTH && area.height >= LARGE_HEIGHT).then(|| placed(area, Size::Large))
}

/// `size`'s mark centred in `area`, which has room for it.
fn placed(area: Rect, size: Size) -> Rect {
    let (width, height) = (wordmark::width(size), wordmark::height(size));
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// The copyright and licence notice on the last two rows of `area`, centred and dim, and the
/// cells it fills; `None` where those rows are the mark's own.
fn notice(area: Rect, mark: Rect, theme: &Theme) -> Option<(Vec<Line<'static>>, Rect)> {
    let rows = Rect {
        y: area.bottom().checked_sub(2)?,
        height: 2,
        ..area
    };
    if rows.y < mark.bottom() {
        return None;
    }
    let room = usize::from(area.width);
    let lines = [notice::COPYRIGHT, notice::LICENCE]
        .into_iter()
        .map(|text| {
            Line::from(Span::styled(
                widgets::text::cut_to_cells(text, room),
                theme.style(Role::Dim),
            ))
        })
        .collect();
    Some((lines, rows))
}

/// A stream of terminal events a case sends by hand, and the sender that feeds it.
///
/// Nothing arrives until the case sends it, and the stream stays open while the sender lives,
/// so a case with no key to send leaves whatever is reading it with only its own clock. The
/// wait for a device reads the same stream this one does, and drives it the same way.
#[cfg(test)]
pub(in crate::tui) fn test_event_stream() -> (
    futures::stream::BoxStream<'static, io::Result<Event>>,
    tokio::sync::mpsc::UnboundedSender<io::Result<Event>>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let events =
        futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|e| (e, rx)) });
    (Box::pin(events), tx)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::tui::test_support::text;

    /// Today's fade, or the static mark, as every case not about a demo effect plays it.
    async fn fade<B, S>(
        terminal: &mut Terminal<B>,
        events: &mut S,
        theme: &Theme,
        animate: bool,
    ) -> IntroEnd
    where
        B: Backend,
        S: Stream<Item = io::Result<Event>> + Unpin,
    {
        play(terminal, events, theme, &mut IntroPlan::fade(animate))
            .await
            .end
    }

    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use crate::tui::terminal::SyncEdge;

    const SEED: u64 = 0x5EED;

    fn truecolor() -> Theme {
        Theme::from_config(
            &svolder_ipc::TuiConfig::default(),
            false,
            svolder_ipc::ColorDepth::TrueColor,
        )
    }

    fn still(w: u16, h: u16, theme: &Theme) -> ratatui::buffer::Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        draw_still(&mut buf, area, theme);
        buf
    }

    #[tokio::test(start_paused = true)]
    async fn particles_runs_three_point_six_seconds_and_hands_over_its_exit() {
        let theme = truecolor();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let (mut events, _tx) = test_event_stream();
        let started = Instant::now();
        let ended = play(
            &mut terminal,
            &mut events,
            &theme,
            &mut IntroPlan::particles(SEED),
        )
        .await;
        let took = started.elapsed();
        assert_eq!(ended.end, IntroEnd::Finished);
        assert!(
            took >= Duration::from_millis(3600) && took < Duration::from_millis(3700),
            "{took:?}"
        );
        assert!(ended.exit.is_some());
        assert_eq!(terminal.backend().buffer(), &still(80, 24, &theme));
    }

    /// A test backend whose size a case changes while the intro runs, as a user resizing the
    /// window does; the size is shared so the case can set it while the intro holds the terminal.
    struct Resizable {
        inner: TestBackend,
        size: Rc<Cell<(u16, u16)>>,
    }

    impl Resizable {
        fn new(w: u16, h: u16) -> (Resizable, Rc<Cell<(u16, u16)>>) {
            let size = Rc::new(Cell::new((w, h)));
            let backend = Resizable {
                inner: TestBackend::new(w, h),
                size: Rc::clone(&size),
            };
            (backend, size)
        }

        /// Bring the inner backend to the shared size, before anything touches its cells.
        fn follow(&mut self) {
            let (w, h) = self.size.get();
            self.inner.resize(w, h);
        }
    }

    impl Backend for Resizable {
        type Error = std::convert::Infallible;

        fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
        where
            I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
        {
            self.follow();
            self.inner.draw(content)
        }

        fn hide_cursor(&mut self) -> Result<(), Self::Error> {
            self.inner.hide_cursor()
        }

        fn show_cursor(&mut self) -> Result<(), Self::Error> {
            self.inner.show_cursor()
        }

        fn get_cursor_position(&mut self) -> Result<ratatui::layout::Position, Self::Error> {
            self.inner.get_cursor_position()
        }

        fn set_cursor_position<P: Into<ratatui::layout::Position>>(
            &mut self,
            position: P,
        ) -> Result<(), Self::Error> {
            self.inner.set_cursor_position(position)
        }

        fn clear(&mut self) -> Result<(), Self::Error> {
            self.follow();
            self.inner.clear()
        }

        fn clear_region(
            &mut self,
            clear_type: ratatui::backend::ClearType,
        ) -> Result<(), Self::Error> {
            self.follow();
            self.inner.clear_region(clear_type)
        }

        fn size(&self) -> Result<ratatui::layout::Size, Self::Error> {
            let (width, height) = self.size.get();
            Ok(ratatui::layout::Size::new(width, height))
        }

        fn window_size(&mut self) -> Result<ratatui::backend::WindowSize, Self::Error> {
            self.inner.window_size()
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            self.inner.flush()
        }
    }

    /// Play particles from 80x24, resize the terminal to `to` at one second, and send the
    /// resize event; the run's end, the instant it took, and the terminal it left.
    async fn played_through_a_resize(to: (u16, u16)) -> (Ended, Duration, Terminal<Resizable>) {
        let theme = truecolor();
        let (backend, size) = Resizable::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let (mut events, tx) = test_event_stream();
        let resize = async {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            size.set(to);
            tx.send(Ok(Event::Resize(to.0, to.1))).unwrap();
        };
        let started = Instant::now();
        let mut plan = IntroPlan::particles(SEED);
        let (ended, ()) = tokio::join!(play(&mut terminal, &mut events, &theme, &mut plan), resize);
        (ended, started.elapsed(), terminal)
    }

    #[tokio::test(start_paused = true)]
    async fn a_resize_under_the_large_mark_finishes_on_the_small_one_with_no_exit() {
        let (ended, took, terminal) = played_through_a_resize((60, 20)).await;
        assert_eq!(ended.end, IntroEnd::Finished);
        assert!(
            took >= Duration::from_millis(3600) && took < Duration::from_millis(3700),
            "{took:?}"
        );
        assert!(ended.exit.is_none());
        assert_eq!(
            terminal.backend().inner.buffer(),
            &still(60, 20, &truecolor())
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_resize_to_a_larger_frame_hands_over_an_exit_planned_for_it() {
        let (ended, took, _terminal) = played_through_a_resize((120, 40)).await;
        assert_eq!(ended.end, IntroEnd::Finished);
        assert!(
            took >= Duration::from_millis(3600) && took < Duration::from_millis(3700),
            "{took:?}"
        );
        let mut exit = ended.exit.expect("the exit is handed over");
        // The exit stays on a frame of the size it was planned for and ends on any other, so
        // playing over a 120x40 frame shows it was planned there and not for 80x24.
        let mut frame = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 120, 40));
        let logo = crate::tui::render::Logo::Word(Rect::new(4, 1, 8, 1));
        exit.paint(&mut frame, Some(logo), &[], Color::Reset);
        assert!(exit.planned() && exit.playing());
    }

    #[tokio::test(start_paused = true)]
    async fn a_key_mid_build_skips_to_the_finished_mark_with_no_exit() {
        let theme = truecolor();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let (mut events, tx) = test_event_stream();
        let key = async {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            tx.send(Ok(Event::Key(KeyCode::Esc.into()))).unwrap();
        };
        let mut plan = IntroPlan::particles(SEED);
        let (ended, ()) = tokio::join!(play(&mut terminal, &mut events, &theme, &mut plan), key);
        assert_eq!(ended.end, IntroEnd::Skipped);
        assert!(ended.exit.is_none());
        assert_eq!(terminal.backend().buffer(), &still(80, 24, &theme));
    }

    #[tokio::test(start_paused = true)]
    async fn q_mid_build_gives_up_and_a_click_does_nothing() {
        let theme = truecolor();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let (mut events, tx) = test_event_stream();
        tx.send(Ok(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })))
        .unwrap();
        tx.send(Ok(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        ))))
        .unwrap();
        let ended = play(
            &mut terminal,
            &mut events,
            &theme,
            &mut IntroPlan::particles(SEED),
        )
        .await;
        assert_eq!(ended.end, IntroEnd::GaveUp);
    }

    #[tokio::test(start_paused = true)]
    async fn a_frame_under_seventy_by_twelve_gets_the_fade_on_the_small_mark() {
        let theme = truecolor();
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
        let (mut events, _tx) = test_event_stream();
        let started = Instant::now();
        let ended = play(
            &mut terminal,
            &mut events,
            &theme,
            &mut IntroPlan::particles(SEED),
        )
        .await;
        assert!(
            started.elapsed() < Duration::from_millis(2000),
            "the fade's 1.6 s, not 3.6 s"
        );
        assert!(ended.exit.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn without_the_animation_layer_particles_is_the_static_mark() {
        let theme = truecolor();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let (mut events, _tx) = test_event_stream();
        let mut plan = IntroPlan::particles(SEED);
        plan.animate = false;
        let started = Instant::now();
        play(&mut terminal, &mut events, &theme, &mut plan).await;
        assert!(started.elapsed() < Duration::from_millis(700));
    }

    #[tokio::test(start_paused = true)]
    async fn every_frame_of_either_path_is_bracketed_in_synchronized_output() {
        for mut plan in [IntroPlan::particles(SEED), IntroPlan::fade(true)] {
            let marks = Rc::new(RefCell::new(Vec::new()));
            let seen = Rc::clone(&marks);
            plan.sync = Box::new(move |edge| seen.borrow_mut().push(edge));
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            let (mut events, _tx) = test_event_stream();
            play(&mut terminal, &mut events, &truecolor(), &mut plan).await;
            let marks = marks.borrow();
            assert!(marks.len() >= 2 && marks.len() % 2 == 0);
            for pair in marks.chunks(2) {
                assert_eq!(pair, [SyncEdge::Begin, SyncEdge::End]);
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_skipped_demo_brackets_its_final_draw_too() {
        let marks = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&marks);
        let theme = truecolor();
        let mut plan = IntroPlan::particles(SEED);
        plan.sync = Box::new(move |edge| seen.borrow_mut().push(edge));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let (mut events, tx) = test_event_stream();
        let before_key = Rc::new(Cell::new(0));
        let key = async {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            before_key.set(marks.borrow().len());
            tx.send(Ok(Event::Key(KeyCode::Esc.into()))).unwrap();
        };
        let (ended, ()) = tokio::join!(play(&mut terminal, &mut events, &theme, &mut plan), key);
        assert_eq!(ended.end, IntroEnd::Skipped);
        let marks = marks.borrow();
        assert!(before_key.get() >= 2, "frames were drawn before the key");
        assert_eq!(
            marks.len(),
            before_key.get() + 2,
            "one pair for the final draw"
        );
        for pair in marks.chunks(2) {
            assert_eq!(pair, [SyncEdge::Begin, SyncEdge::End]);
        }
    }

    /// The shading character the large mark's face is drawn in.
    const FACE: char = '▓';
    /// A character only the small mark is drawn with.
    const SMALL_FACE: char = '█';

    #[tokio::test(start_paused = true)]
    async fn the_intro_ends_by_itself_after_the_fade_and_the_hold() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let (mut events, _tx) = test_event_stream();
        let started = Instant::now();
        let end = fade(&mut terminal, &mut events, &Theme::new(false), true).await;
        assert_eq!(end, IntroEnd::Finished);
        let took = started.elapsed();
        assert!(
            took >= Duration::from_millis(1600) && took < Duration::from_millis(2000),
            "{took:?}"
        );
        assert!(
            text(terminal.backend().buffer())
                .iter()
                .any(|row| row.contains(FACE)),
            "the mark is not on the screen the intro left"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn any_key_skips_the_intro() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let (mut events, tx) = test_event_stream();
        tx.send(Ok(Event::Key(KeyCode::Esc.into())))
            .expect("the stream takes the key");
        let end = fade(&mut terminal, &mut events, &Theme::new(false), true).await;
        assert_eq!(end, IntroEnd::Skipped);
    }

    /// The two keys that give up on a device say so rather than only skipping the mark, so one
    /// press over the intro ends the run the way one press over the wait after it does.
    #[tokio::test(start_paused = true)]
    async fn ctrl_c_and_q_give_up_over_the_mark() {
        for key in [
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        ] {
            let mut terminal =
                Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
            let (mut events, tx) = test_event_stream();
            tx.send(Ok(Event::Key(key)))
                .expect("the stream takes the key");
            let started = Instant::now();
            assert_eq!(
                fade(&mut terminal, &mut events, &Theme::new(false), true).await,
                IntroEnd::GaveUp,
                "{key:?} only skipped the mark"
            );
            assert!(
                started.elapsed() < Duration::from_millis(u64::from(FADE_MS)),
                "the mark held the key up for its whole fade"
            );
        }
    }

    /// The release half of a press that would have given up is the other half of a press
    /// already acted on, and ends nothing at all.
    #[tokio::test(start_paused = true)]
    async fn a_released_key_neither_skips_nor_gives_up() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let (mut events, tx) = test_event_stream();
        tx.send(Ok(Event::Key(KeyEvent {
            kind: KeyEventKind::Release,
            ..KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
        })))
        .expect("the stream takes the key");
        assert_eq!(
            fade(&mut terminal, &mut events, &Theme::new(false), false).await,
            IntroEnd::Finished
        );
    }

    #[tokio::test(start_paused = true)]
    async fn below_truecolor_the_mark_is_held_static_for_half_a_second() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let (mut events, _tx) = test_event_stream();
        let started = Instant::now();
        assert_eq!(
            fade(&mut terminal, &mut events, &Theme::new(false), false).await,
            IntroEnd::Finished
        );
        let took = started.elapsed();
        assert!(took < Duration::from_millis(700), "{took:?}");
        assert!(
            text(terminal.backend().buffer())
                .iter()
                .any(|row| row.contains(FACE)),
            "the mark is not on the screen the intro left"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_frame_too_small_for_the_large_mark_gets_the_small_one() {
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).expect("a test terminal opens");
        let (mut events, _tx) = test_event_stream();
        assert_eq!(
            fade(&mut terminal, &mut events, &Theme::new(false), false).await,
            IntroEnd::Finished
        );
        let rows = text(terminal.backend().buffer());
        // The last two rows are the notice's.
        let drawn = rows[..18]
            .iter()
            .filter(|row| !row.trim().is_empty())
            .count();
        assert_eq!(drawn, 2, "the small mark takes two rows of the frame");
        assert!(
            rows.iter().any(|row| row.contains(SMALL_FACE)),
            "the small mark was not drawn: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains(FACE)),
            "the large mark does not fit a sixty-column frame: {rows:?}"
        );
    }

    /// The copyright and licence lines stand on the terminal's last two rows, each centred and
    /// dim, in the intro that holds the mark static and in the one that fades it.
    #[tokio::test(start_paused = true)]
    async fn the_last_two_rows_carry_the_notice_centred_and_dim() {
        let dim = Theme::new(false).style(Role::Dim);
        for animate in [false, true] {
            let mut terminal =
                Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
            let (mut events, _tx) = test_event_stream();
            fade(&mut terminal, &mut events, &Theme::new(false), animate).await;
            let buffer = terminal.backend().buffer().clone();
            let rows = text(&buffer);
            for (row, line) in [(22, notice::COPYRIGHT), (23, notice::LICENCE)] {
                let at = (80 - line.chars().count()) / 2;
                assert_eq!(
                    rows[row],
                    format!(
                        "{}{line}{}",
                        " ".repeat(at),
                        " ".repeat(80 - at - line.chars().count())
                    )
                );
                let cell = buffer
                    .cell((
                        u16::try_from(at).expect("a column"),
                        u16::try_from(row).expect("a row"),
                    ))
                    .expect("a cell");
                assert!(
                    cell.modifier.contains(dim.add_modifier),
                    "animate {animate}, row {row}"
                );
            }
            assert!(
                rows[..22]
                    .iter()
                    .all(|row| !row.contains("Copyright") && !row.contains("GNU")),
                "the notice is only on the last two rows"
            );
        }
    }

    /// The notice fades in with the mark where the mark fades, and is drawn at once where it
    /// does not: the first frame of a fading intro has the notice's cells faded out, and the
    /// first frame of a static one has them as they stay.
    #[tokio::test(start_paused = true)]
    async fn the_notice_fades_with_the_mark_and_is_drawn_at_once_where_the_mark_is() {
        let first_frame = |animate: bool| async move {
            let mut terminal =
                Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
            let (mut events, tx) = test_event_stream();
            tx.send(Ok(Event::Key(KeyCode::Esc.into())))
                .expect("the stream takes the key");
            fade(&mut terminal, &mut events, &Theme::new(false), animate).await;
            let buffer = terminal.backend().buffer().clone();
            let at = (80 - notice::COPYRIGHT.chars().count()) / 2;
            let cell = buffer
                .cell((u16::try_from(at).expect("a column"), 22))
                .expect("a cell");
            (cell.symbol().to_string(), cell.fg)
        };
        let (symbol, faded) = first_frame(true).await;
        assert_eq!(symbol, "C");
        assert_eq!(faded, FADE_FROM, "the notice is not part of the fade");
        let (symbol, drawn) = first_frame(false).await;
        assert_eq!(symbol, "C");
        assert_ne!(drawn, FADE_FROM, "the static intro faded the notice");
    }

    /// With the theme background on, the first frame of the fade has the mark's cells in the
    /// ground colour over the ground; with it off they start from the dim grey, which reads on a
    /// light terminal as on a dark one.
    #[tokio::test(start_paused = true)]
    async fn the_mark_fades_in_from_the_ground_colour_while_the_theme_paints_one() {
        use ratatui::style::Color;
        use svolder_ipc::{ColorDepth, TuiConfig};

        let first_frame = |background: bool| async move {
            let config = TuiConfig {
                theme_background: background,
                ..TuiConfig::default()
            };
            let theme = Theme::from_config(&config, false, ColorDepth::TrueColor);
            let mut terminal =
                Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
            let (mut events, tx) = test_event_stream();
            tx.send(Ok(Event::Key(KeyCode::Esc.into())))
                .expect("the stream takes the key");
            fade(&mut terminal, &mut events, &theme, true).await;
            let buffer = terminal.backend().buffer().clone();
            let (lines, mark) = centred(Rect::new(0, 0, 80, 24), &theme).expect("a mark");
            drop(lines);
            let lit = (mark.top()..mark.bottom())
                .flat_map(|y| (mark.left()..mark.right()).map(move |x| (x, y)))
                .map(|at| buffer.cell(at).expect("a cell").clone())
                .find(|cell| !cell.symbol().trim().is_empty())
                .expect("a glyph of the mark");
            (lit.fg, lit.bg, buffer.cell((0, 0)).expect("a cell").bg)
        };
        let (fg, bg, corner) = first_frame(true).await;
        assert_eq!(
            fg,
            Color::Rgb(0x07, 0x06, 0x05),
            "the mark starts at the ground"
        );
        assert_eq!(bg, fg, "over the ground");
        assert_eq!(corner, fg, "the whole frame is the ground");
        let (fg, _, corner) = first_frame(false).await;
        assert_eq!(fg, FADE_FROM, "the mark starts at the dim grey without it");
        assert_ne!(fg, Color::Black, "the mark must not start at black");
        assert_eq!(corner, Color::Reset);
    }

    /// The notice stands off a frame that has no rows to spare below the mark.
    #[test]
    fn the_notice_is_left_off_where_the_mark_reaches_its_rows() {
        let theme = Theme::new(false);
        let area = Rect::new(0, 0, 80, 4);
        let (_, mark) = centred(area, &theme).expect("the small mark fits");
        assert_eq!(notice(area, mark, &theme), None);
        let tall = Rect::new(0, 0, 80, 24);
        let (_, mark) = centred(tall, &theme).expect("the large mark fits");
        let (_, rows) = notice(tall, mark, &theme).expect("the notice has its rows");
        assert_eq!(rows, Rect::new(0, 22, 80, 2));
        assert!(
            mark.union(rows)
                .contains(ratatui::layout::Position::new(0, 23))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_frame_with_no_room_for_the_small_mark_shows_nothing() {
        let mut terminal = Terminal::new(TestBackend::new(20, 10)).expect("a test terminal opens");
        let (mut events, _tx) = test_event_stream();
        let started = Instant::now();
        assert_eq!(
            fade(&mut terminal, &mut events, &Theme::new(false), true).await,
            IntroEnd::Finished
        );
        assert_eq!(started.elapsed(), Duration::ZERO, "nothing is held at all");
        assert!(
            text(terminal.backend().buffer())
                .iter()
                .all(|row| row.trim().is_empty()),
            "a frame with no room for the mark is left alone"
        );
    }

    /// An ended or broken stream answers the instant it is asked, so an intro that kept
    /// reading it would spin through its whole hold at full tilt rather than sleeping.
    #[tokio::test(start_paused = true)]
    async fn an_input_stream_that_fails_leaves_the_intro_on_its_own_clock() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let (mut events, tx) = test_event_stream();
        tx.send(Err(io::Error::other("the terminal went away")))
            .expect("the stream takes the failure");
        let started = Instant::now();
        assert_eq!(
            fade(&mut terminal, &mut events, &Theme::new(false), false).await,
            IntroEnd::Finished
        );
        assert_eq!(
            started.elapsed(),
            Duration::from_millis(u64::from(STATIC_MS)),
            "the hold is what ends the intro, not the stream"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_click_does_not_skip_the_intro() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let (mut events, tx) = test_event_stream();
        tx.send(Ok(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::NONE,
        })))
        .expect("the stream takes the click");
        assert_eq!(
            fade(&mut terminal, &mut events, &Theme::new(false), false).await,
            IntroEnd::Finished,
            "only a key ends the intro early"
        );
    }
}
