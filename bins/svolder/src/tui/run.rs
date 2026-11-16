// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The event loop: a frame per turn (none on a quiet one), then whichever of a key, a device
//! update, a tick, or (while the device list shows a daemon's rows) the next relist arrives first.
//! A turn is what happens before the frame (the add-device walk, a find held back), the frame, then
//! the first arm to fire; an arm that does more than one thing has a handler of its own.
//!
//! Keys become the [`Intent`]s the [`App`] names, and this module carries them out against the
//! backend; what the backend reports back is folded straight into the app for the next frame to
//! draw. Every write is sent without waiting for its echo (that arrives on the update feed like
//! any other reading), so a held-down key is never held up by the write before it.
//!
//! Switching device closes the backend and opens another through [`Opener::reopen`], the same
//! path the TUI starts on, so starting on a device and switching to one cannot drift apart.
//! Nothing an open inside the loop has to say is printed (the alternate screen is up); it
//! becomes a toast instead.
//!
//! - [`connect`]: what the Settings box's Connect category asks of the loop.
//! - [`devices`]: the device list's own flows: adding, editing, removing and switching.
//! - [`events`]: folding what the device says into the app, and sending what is asked of it.
//! - [`host`]: the host's own audio as the loop reaches it.
//! - [`input`]: carrying out a key press, a click or an intent.
//! - [`jobs`]: the slow work the preset list asks of the loop: AutoEq and the computer EQ.
//! - [`look`]: how the screens draw, and what a frame just drawn earns in animation.
//! - [`open`]: what opens a device for the TUI.
//! - [`reopen`]: the open and reopen a start or a switch goes through, and where none lands.
//! - [`watching`]: what the loop is watching.
//! - [`web_box`]: what the loop does for the Web UI box.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{Event, KeyEvent, KeyEventKind, MouseEventKind};
use futures::future::LocalBoxFuture;
use futures::{Stream, StreamExt};
use ratatui::Terminal;
use ratatui::layout::Rect;

use svolder_ble::BleError;
use svolder_host::Block;
use svolder_ipc::{Config, DeviceConfigOp, PresetOp, TuiConfig};
use svolder_profiles::Registry;

use crate::backend::{
    Backend, DaemonInfo, DeviceRow, Failure, Held, OwnedGlobals, Update, WriteError,
};
use crate::recovery::Kind;
use crate::wizard::Wizard;

use super::app::curve_drag;
use super::app::divider;
use super::app::glide;
use super::app::{App, Holding, Intent, Screen};
use super::boxes::edit_line;
use super::boxes::tooltip;
use super::boxes::{device_presets, presets};
use super::decoration::title::TitleMeter;
use super::mouse::{self, HitMap, MouseAction};
use super::render;
use super::render::FrameAreas;
use super::terminal::{MouseCapture, SyncEdge};
use super::widgets::hyperlink;

mod computer;
mod connect;
mod devices;
mod events;
mod host;
mod input;
mod jobs;
mod look;
mod open;
mod reopen;
#[cfg(test)]
mod test_support;
mod watching;
mod web_box;
mod writes;

pub(in crate::tui) use connect::TokenSite;
// A re-export only the tests use is its own `#[cfg(test)]` line, so the others stay unconditional.
use computer::ComputerLink;
#[cfg(test)]
use connect::daemon_answered;
use connect::{Reconnect, daemon_answers_within, daemon_back, recv_reconnect, recv_version};
use devices::{
    Added, RELIST_BOUND, RELIST_PERIOD, Removed, Switched, add_device, preset_landed, relist_held,
    relisted, relisting, remove_device, switch, switch_device, switch_via, update_device,
    write_device, written,
};
#[cfg(test)]
use events::execute;
use events::{
    dispatch, fetched, found, heard, match_host_feed, next_status, refresh_note, send, send_held,
    take_fetched, wait_over, waiting,
};
pub(in crate::tui) use host::HostLink;
#[cfg(test)]
use input::take_intent;
use input::{apply_action, take_intents, take_key};
pub(in crate::tui) use look::Look;
pub(in crate::tui) use look::paint_over;
use look::{arm_effects, colour, spectrum_over, tick_period};
#[cfg(test)]
use open::Opened;
pub(super) use open::{Adding, Live, Open, Opener};
pub use reopen::{Landing, land};
#[cfg(test)]
pub(in crate::tui) use test_support::drawn_turn;
use watching::OnScreen;
pub(super) use watching::{Kept, Shown, Watching};

/// What the end of the update feed is reported as. A direct session ends its feed once a
/// reconnect has spent the connect deadline, and the daemon's ends when it stops publishing,
/// so by the time this is said the link is gone rather than going.
const GONE: &str = "device did not come back";
/// How long a TUI on a daemon waits for a daemon to answer on its socket again after the
/// connection to it closed, before it gives the device up as gone. A `daemon restart` waits up
/// to five seconds for the old daemon to let go of the socket and then starts the new one,
/// which the launcher gives a few seconds more; under systemd the restart waits for the old
/// service to exit, which can take its whole shutdown. Half a minute covers all of those.
const DAEMON_WAIT: Duration = Duration::from_secs(30);
/// The pause before the first look at the socket again, which doubles up to [`DAEMON_POLL_MAX`].
const DAEMON_POLL_FIRST: Duration = Duration::from_millis(100);
/// The longest pause between looks at the socket.
const DAEMON_POLL_MAX: Duration = Duration::from_secs(1);
/// What is said once the device is open again on a daemon that came back.
const RECONNECTED: &str = "reconnected to the daemon";
/// What is said once there is no device left to read from at all.
const LINK_LOST: &str = "link lost; press q and reopen";
/// What a refresh the backend would not answer is reported as.
const NO_SNAPSHOT: &str = "could not read the device again";
/// What a terminal with no more input to give is reported as.
pub(super) const INPUT_ENDED: &str = "the terminal input ended";

/// How often the daemon is asked how its servers stand while the Connect category shows.
const CONNECT_POLL: Duration = Duration::from_secs(2);

/// How often a daemon session asks the daemon whether its configuration file changed since its
/// last reload, whichever screen shows.
const CONFIG_POLL: Duration = match cfg!(test) {
    true => Duration::from_millis(300),
    false => Duration::from_secs(60),
};

/// Whether the Settings box is open on the Connect category.
fn connect_shown(app: &App) -> bool {
    app.overlays
        .settings
        .is_some_and(|at| at.category == super::screens::settings::CONNECT)
}

/// How far back the link history is asked for, in seconds: a day.
const LINK_HISTORY_SECS: u64 = 24 * 3600;

/// What the loop left behind once it stopped.
///
/// What it was watching comes back out rather than being released here: the caller owns the
/// terminal guard, and restoring the terminal comes before both the message and the disconnect.
pub(super) struct LoopEnd {
    /// The device to release.
    pub watching: Watching,
    /// The exit code to leave with.
    pub code: u8,
    /// What to say once the terminal is back, if anything.
    pub why: Option<String>,
}

/// What the loop is handed: the seams a test replaces, and what it watches and draws with.
pub(super) struct LoopParts<'p, O, A, B: ratatui::backend::Backend, S, M> {
    /// How the device list opens another device.
    pub(super) opener: &'p Opener<O>,
    /// What walks someone through adding a device.
    pub(super) adder: &'p A,
    /// The device to watch.
    pub(super) watching: Watching,
    /// The terminal to draw to.
    pub(super) terminal: &'p mut Terminal<B>,
    /// The terminal's input.
    pub(super) events: S,
    /// What the mouse key turns.
    pub(super) mouse: &'p mut M,
    /// The terminal's title.
    pub(super) title: &'p mut TitleMeter,
    /// How the screens draw.
    pub(super) look: Look,
}

/// Draw, read input and updates, and act on them, until the user quits or the terminal goes.
///
/// Ends with code 0 on `q`, and 1 when `terminal` cannot be drawn to or `events` ends or fails.
/// `mouse` is what the mouse key turns; `events` is the terminal's input, read but never created
/// here, so a test can script it. `title` is the terminal's title, told every frame how loud
/// what is playing is (the device's meters while streaming, else the host sink capture).
/// `opener` is how the device list opens another device. `look` is how the screens draw (the
/// `[tui]` table, theme and animation layer it resolved to); this loop feeds it each frame's
/// drawn areas and the time since the last frame, and the Settings box changes it from
/// inside the loop.
///
/// Every one of these is borrowed, not made, here: the terminal, the input, the mouse capture
/// and title, what opens a device and what walks someone through adding one, and the look. A
/// test swaps any of them for something of its own, which is why they arrive as parts.
///
/// However the loop ends, the host sink capture is stopped before handing back what it was
/// watching: nothing of the host's audio is recorded once the TUI is leaving.
pub(super) async fn event_loop<O, A, B, S, M>(parts: LoopParts<'_, O, A, B, S, M>) -> LoopEnd
where
    O: Open,
    A: Adding,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
    M: MouseCapture,
{
    let mut end = run_turns(parts).await;
    // a reload a Settings write still owes goes out whichever way the loop ended
    look::flush_owed(&mut end.watching).await;
    let watching = &mut end.watching;
    watching.host.stop_capture(&mut watching.app);
    // The crossfeed of a controller of the loop's own is off when the loop ends.
    watching.computer.shutdown().await;
    end
}

/// The clocks the loop keeps while it turns.
struct Timers {
    /// How often the ticks fire now: the animation layer's pace while something moves.
    period: Duration,
    /// Ages what is on screen and sends what the keys held back.
    ticks: tokio::time::Interval,
    /// Asks the daemon for the device list again.
    relist: tokio::time::Interval,
    /// Asks the daemon how its servers stand while the Connect category shows.
    conn_poll: tokio::time::Interval,
    /// Asks the daemon whether its configuration file changed since its last reload, while a
    /// daemon session runs, so an edit made in another terminal reaches this one.
    config_poll: tokio::time::Interval,
    /// Asks the daemon whether the link the Web UI box shows is still open.
    web_poll: tokio::time::Interval,
}

impl Timers {
    /// The six clocks, the ticks running at `period`.
    fn new(period: Duration) -> Self {
        // A tick only ages what's on screen, so ticks missed while a write was in flight are worth
        // nothing: the next one is a period from here, not a burst of the ones gone by.
        let ticks = delayed(period);
        // The device list asks the daemon again on its own clock, running whether or not the list
        // shows. A tick that fell due while hidden is taken once as soon as the list shows, and the
        // next is a period after that, so a list that comes up is asked within a period, never a
        // burst.
        let relist = delayed(RELIST_PERIOD);
        // While the Connect category shows, the daemon is asked how its servers stand on its own
        // clock, and the category is read afresh each time it comes on screen.
        let conn_poll = delayed(CONNECT_POLL);
        // While the Web UI box shows a link, the daemon is asked each second whether a browser
        // has used it.
        let web_poll = delayed(web_box::CHECK_PERIOD);
        // The first look at the file's state is the one the session's start makes, so this clock
        // starts a period from now.
        let mut config_poll =
            tokio::time::interval_at(tokio::time::Instant::now() + CONFIG_POLL, CONFIG_POLL);
        config_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Self {
            period,
            ticks,
            relist,
            conn_poll,
            config_poll,
            web_poll,
        }
    }

    /// Puts the ticks on `wanted` once a drawn frame says the pace has changed.
    fn retune(&mut self, wanted: Duration) {
        if wanted != self.period {
            self.period = wanted;
            self.ticks = delayed(wanted);
        }
    }
}

/// An interval that, once behind, waits a full period rather than firing the ticks it missed.
fn delayed(period: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval
}

/// What the loop carries from one turn to the next.
struct Turns {
    /// The loop's clocks.
    timers: Timers,
    /// Where the last drawn frame put its clickable areas, which a click or a hover is read
    /// against.
    hits: HitMap,
    /// When the last frame was drawn.
    drawn_at: Instant,
    /// Whether a key or click asked for a device to be added; the walk answers this once the
    /// frame's own input is read, since the wizard reads the same terminal, so it runs between
    /// turns rather than inside one.
    adding: bool,
    /// Whether the turn just taken brought nothing but a block the capture read, asking for no
    /// frame of its own.
    quiet: bool,
    /// Whether the device a wait is for came up while a box was open: the reopen is held until
    /// the box has closed, so it never takes a half-typed edit or a keystroke with it.
    found_under_box: bool,
    /// Whether the Connect category showed on the turn before, so it is read once as it comes up.
    connect_was_shown: bool,
}

impl Turns {
    fn new(period: Duration) -> Self {
        Self {
            timers: Timers::new(period),
            hits: HitMap::default(),
            drawn_at: Instant::now(),
            adding: false,
            quiet: false,
            found_under_box: false,
            connect_was_shown: false,
        }
    }
}

/// The seams a test replaces, borrowed by every step of the loop.
struct Seams<'p, O, B: ratatui::backend::Backend, S, M> {
    /// Opens the backends a switch or a reconnect asks for.
    opener: &'p Opener<O>,
    /// What the frames are drawn on.
    terminal: &'p mut Terminal<B>,
    /// The terminal's input events.
    events: S,
    /// Turns the terminal's mouse reporting on and off.
    mouse: &'p mut M,
    /// The meter drawn into the terminal's title.
    title: &'p mut TitleMeter,
}

impl<'p, O, B: ratatui::backend::Backend, S, M> Seams<'p, O, B, S, M> {
    /// The screen a wait draws on and reads from, with the opener and the mouse capture beside
    /// it, so a step can hand all three to a call at once.
    fn split<'a>(
        &'a mut self,
        hits: &'a mut HitMap,
    ) -> (OnScreen<'a, B, S>, &'p Opener<O>, &'a mut M) {
        (
            OnScreen {
                terminal: &mut *self.terminal,
                hits,
                events: &mut self.events,
            },
            self.opener,
            &mut *self.mouse,
        )
    }
}

/// Switches to the device a wait is for, which ends the loop with code 0 when the switch is given
/// up on, and otherwise hands back what the loop watches.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn switch_or_end<O, B, S, M>(
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &Look,
    hits: &mut HitMap,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    let (mut screen, opener, _) = seams.split(hits);
    if switch(opener, &mut watching, None, look, &mut screen).await == Switched::GaveUp {
        return Err(LoopEnd {
            watching,
            code: 0,
            why: None,
        });
    }
    Ok(watching)
}

/// What happens before a frame: the host is told it may look for players, the Connect category is
/// read the turn it first shows, a device walk a key asked for is run, and a find a box held back
/// is taken up once the box has closed.
///
/// Hands back what the loop watches, or the end the walk or the switch earned.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn before_frame<O, A, B, S, M>(
    adder: &A,
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &mut Look,
    turns: &mut Turns,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    A: Adding,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    // A split a drag let go of, or a burst of nudges that went quiet, is written once.
    look.save_split(&mut watching.app, Instant::now());
    // Who speaks for the player is settled before players are looked for, which is done once,
    // on the first turn host audio shows with no daemon to speak for the host.
    match_host_feed(&mut watching).await;
    watching.host.follow(&mut watching.app);
    watching.host.wake(&watching.app);
    let connect_now = connect_shown(&watching.app);
    if connect_now && !turns.connect_was_shown {
        connect::read(seams.opener, &mut watching, &look.token_site);
    }
    turns.connect_was_shown = connect_now;
    if std::mem::take(&mut turns.adding) {
        match add_device(
            adder,
            seams.opener,
            &mut watching,
            &mut *seams.terminal,
            &mut seams.events,
            look,
        )
        .await
        {
            Added::Carried => {}
            Added::Ends(code, why) => {
                return Err(LoopEnd {
                    watching,
                    code,
                    why,
                });
            }
        }
    }
    if turns.found_under_box && !waiting(&watching) {
        turns.found_under_box = false;
    }
    // The first turn without a box open takes up a find the box held back.
    if turns.found_under_box && !watching.app.modal_open() {
        turns.found_under_box = false;
        watching = switch_or_end(seams, watching, look, &mut turns.hits).await?;
    }
    Ok(watching)
}

/// Draws one frame and settles everything the frame decides: the tooltip, the title, the
/// animation layer, the host capture and the tick pace.
///
/// Says why when the terminal cannot be drawn to.
fn draw_frame<O, B, S, M>(
    seams: &mut Seams<'_, O, B, S, M>,
    watching: &mut Watching,
    look: &mut Look,
    turns: &mut Turns,
) -> Result<(), String>
where
    B: ratatui::backend::Backend,
{
    colour(look, &watching.app);
    watching.app.clock = svolder_ipc::clock::local_hhmm(SystemTime::now());
    watching.app.drawn.begin_frame();
    watching.app.observe_mascot(
        Instant::now(),
        svolder_ipc::clock::local_month_day(SystemTime::now()),
    );
    // The help overlay and the `Now playing` marquee are worked out against the
    // terminal, so the app is told its size before the frame that draws them.
    if let Ok(size) = seams.terminal.size() {
        watching.app.resize(size.width, size.height);
    }
    // A user-driven screen change starts its noise or melt before the frame that first
    // draws the new screen, so that frame is the transition's first rather than the new
    // screen showing bare before the transition covers it.
    if std::mem::take(&mut watching.app.frame_flags.navigated) {
        look.effects.on_navigation();
    }
    let now = Instant::now();
    let last_tick = now.saturating_duration_since(turns.drawn_at);
    turns.drawn_at = now;
    let mut areas = FrameAreas::default();
    // Draw order: the widgets; the spectrum over the curve they drew (the curve's own
    // colouring, applied whether or not the animation layer runs); whatever tachyonfx is
    // running (changes cell look, never cell content); last, any noise or melt playing.
    // The intro's exit is drawn a whole frame at a time, at its own pace.
    let synced = look.effects.reforming();
    if synced {
        (look.sync)(SyncEdge::Begin);
    }
    // With no link on show the pointer is not on one, whatever it last rested on.
    if !hyperlink::shown(&watching.app) {
        watching.app.pointer.over_link = false;
    }
    // A sixel picture that went or changed is cleared by a repaint of every cell: the terminal
    // keeps its pixels under cells that read the same.
    if let Ok(size) = seams.terminal.size()
        && watching
            .host
            .art
            .clears_before(&watching.app, Rect::new(0, 0, size.width, size.height))
    {
        repaint(seams.terminal);
    }
    let area = seams
        .terminal
        .size()
        .map(|size| Rect::new(0, 0, size.width, size.height));
    let cost = Instant::now();
    let drawn = seams.terminal.draw(|frame| {
        areas = render::draw(frame, &watching.app, &look.theme, &mut turns.hits);
        paint_over(frame, &areas, &look.theme, &mut look.effects, last_tick);
    });
    // The box the cover had in this frame sizes its picture after it, from the same layout.
    if let Ok(area) = area
        && let (Some(hash), Some(font)) = (
            watching.app.host.cover.hash.clone(),
            watching.app.host.cover.font,
        )
    {
        let size = crate::tui::screens::device::art_box(area, &watching.app);
        watching.app.drawn.keep_art_box(crate::tui::app::ArtBox {
            area,
            hash,
            font,
            size,
        });
    }
    watching.app.drawn.end_frame();
    if synced {
        (look.sync)(SyncEdge::End);
        look.effects.pace(cost.elapsed());
    }
    match drawn {
        // What the terminal shows now is the old screen the next screen change melts.
        Ok(shown) => {
            look.effects.keep_shown(shown.buffer);
            // The Web UI box's link, made a terminal hyperlink over the cells just drawn.
            let link = hyperlink::web_link(&watching.app, &turns.hits, shown.buffer);
            if !link.is_empty() {
                (look.link_out)(&link);
            }
        }
        Err(e) => {
            return Err(e.to_string());
        }
    }
    // A tooltip whose target the frame no longer draws under it goes.
    tooltip::settle(&mut watching.app, areas.tooltip_stands);
    // The title is decoration over the frame just drawn: it says what the level's one
    // source (the meters, or the host's capture) last read, and holds nothing up.
    seams.title.update(
        now,
        &watching.app.session.device_name,
        watching.app.levels(),
        look.tui.title_meter,
    );
    arm_effects(
        &mut watching.app,
        &mut look.effects,
        &areas,
        &look.theme,
        now,
    );
    // The host sink capture runs only while something drawn follows what it reads,
    // settled by the frame just drawn.
    watching.app.host.spectrum_on_screen = areas.spectrum.is_some();
    watching
        .host
        .steer_frames(&mut watching.app, watching.backend.as_ref());
    watching.host.steer(&mut watching.app);
    if let Ok(size) = seams.terminal.size() {
        watching.host.art.steer(
            &mut watching.app,
            watching.backend.as_ref(),
            Rect::new(0, 0, size.width, size.height),
        );
    }
    let wanted = tick_period(&watching.app, &look.effects);
    turns.timers.retune(wanted);
    Ok(())
}

/// Make the next frame write every cell: the screen is cleared and what the terminal is held to
/// show is forgotten. Not [`Terminal::clear`], which asks the terminal where its cursor is and
/// waits for the answer, which a loop that reads the terminal's input must never do.
fn repaint<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) {
    let _ = terminal
        .backend_mut()
        .clear_region(ratatui::backend::ClearType::All);
    // Both buffers blank: the last frame is forgotten, so the next one is written whole.
    terminal.swap_buffers();
}

/// The loop [`event_loop`] runs: until it ends, each turn is the steps before a frame, the frame
/// (unless the turn before it was quiet), and then whichever arm of the race fires first.
///
/// A block the host sink capture read is folded into the app without a frame of its own: what
/// it carries is drawn by the next frame something else asks for, so the capture never makes
/// the loop draw faster than the screen already does. The one exception is a block that sets
/// the Spectrum row moving from rest, which does ask for a frame — that frame puts the loop on
/// its fast tick for as long as the row keeps moving. Whatever holds the loop away from its
/// blocks (the add-device walk, a device switch closing one device and opening another) stops
/// the capture first and restarts it afterward where it is still wanted.
async fn run_turns<O, A, B, S, M>(parts: LoopParts<'_, O, A, B, S, M>) -> LoopEnd
where
    O: Open,
    A: Adding,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
    M: MouseCapture,
{
    let LoopParts {
        opener,
        adder,
        watching,
        terminal,
        events,
        mouse,
        title,
        mut look,
    } = parts;
    let mut end = turn_loop(
        Seams {
            opener,
            terminal,
            events,
            mouse,
            title,
        },
        adder,
        watching,
        &mut look,
    )
    .await;
    // A split moved a moment ago is written whichever way the loop ended.
    look.save_split_owed(&mut end.watching.app);
    look.settle(&mut end.watching).await;
    end
}

/// The turns of the loop, one after another, until the loop ends.
async fn turn_loop<O, A, B, S, M>(
    mut seams: Seams<'_, O, B, S, M>,
    adder: &A,
    mut watching: Watching,
    look: &mut Look,
) -> LoopEnd
where
    O: Open,
    A: Adding,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
    M: MouseCapture,
{
    let mut turns = Turns::new(tick_period(&watching.app, &look.effects));
    loop {
        watching = match before_frame(adder, &mut seams, watching, look, &mut turns).await {
            Ok(next) => next,
            Err(end) => return end,
        };
        // A turn that brought nothing but a block the capture read draws no frame: the next
        // frame something else asks for draws what it carried.
        if !std::mem::take(&mut turns.quiet)
            && let Err(why) = draw_frame(&mut seams, &mut watching, look, &mut turns)
        {
            return LoopEnd {
                watching,
                code: 1,
                why: Some(why),
            };
        }
        watching = match race(&mut seams, watching, look, &mut turns).await {
            Ok(next) => next,
            Err(end) => return end,
        };
    }
}

/// One turn's race: whichever of the loop's sources fires first is carried out, and what is
/// watched afterward comes back, or how the loop ends.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn race<O, B, S, M>(
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &mut Look,
    turns: &mut Turns,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
    M: MouseCapture,
{
    let owed = watching.reload_owed.as_ref().map(|owed| owed.0);
    watching.settle_search();
    let search_due = watching.jobs.due_at();
    tokio::select! {
        event = seams.events.next() => {
            watching = match on_terminal_event(event, seams, watching, look, turns).await {
                Ok(next) => next,
                Err(end) => return Err(end),
            };
        }
        update = next_status(watching.backend.as_mut()), if watching.feed_open => {
            watching = match on_update(update, seams, watching, look, turns).await {
                Ok(next) => next,
                Err(end) => return Err(end),
            };
        }
        _ = turns.timers.ticks.tick() => on_tick(&mut watching).await,
        _ = turns.timers.relist.tick(), if relisting(&watching) || waiting(&watching) => {
            watching = match on_relist(seams, watching, look, turns).await {
                Ok(next) => next,
                Err(end) => return Err(end),
            };
        }
        answered = recv_reconnect(&mut watching.reconnect), if watching.reconnect.is_some() => {
            watching = match on_daemon_back(answered, seams, watching, look, turns).await {
                Ok(next) => next,
                Err(end) => return Err(end),
            };
        }
        answered = fetched(&mut watching.host_refresh), if watching.host_refresh.is_some() => {
            watching.host_refresh = None;
            if let Some(answered) = answered {
                take_fetched(&mut watching.app, answered, Instant::now());
            }
        }
        version = recv_version(&mut watching.daemon_version), if watching.daemon_version.is_some() => {
            connect::answered(&mut watching, version, &seams.opener.path, &look.token_site);
        }
        asked = web_box::recv(&mut watching.web_code), if watching.web_code.is_some() => {
            web_code_back(&mut watching, look, asked);
        }
        checked = web_box::recv_check(&mut watching.web_check),
            if watching.web_check.is_some() =>
        {
            web_check_back(&mut watching, checked);
        }
        _ = turns.timers.web_poll.tick(), if web_box::checking(&watching, Instant::now()) => {
            web_box::check(&mut watching, Instant::now());
        }
        done = web_box::recv_copy(&mut watching.web_copy), if watching.web_copy.is_some() => {
            watching.web_copy = None;
            web_box::copy_ended(&mut watching, done);
        }
        // While the Connect category shows, the daemon is asked again every few seconds:
        // a bridge connects and a server binds after the reload that started them.
        _ = turns.timers.conn_poll.tick(), if connect_shown(&watching.app) && watching.daemon_version.is_none() => {
            watching.daemon_version = connect::ask_daemon(watching.backend.as_ref());
        }
        // A hand edit of the file made elsewhere is told on this slower clock, whichever
        // screen shows; a direct session has no daemon to ask, and `ask_daemon` says nothing.
        _ = turns.timers.config_poll.tick(), if watching.app.session.daemon && watching.daemon_version.is_none() => {
            watching.daemon_version = connect::ask_daemon(watching.backend.as_ref());
        }
        // A Settings write ended: a changed file owes the daemon a reload, a refused one is said.
        done = look.writes.recv(), if look.writes.busy() => {
            look.writes.landed();
            look::write_ended(&mut watching, look.path(), done);
        }
        // Settings wrote `[tui]` or `[autoeq]` and the writes paused: a daemon that reads this
        // file reloads once, so the file's change is not the daemon's to announce.
        _ = tokio::time::sleep_until(owed.unwrap_or_else(tokio::time::Instant::now)), if owed.is_some() => {
            look::send_owed(&mut watching);
        }
        answer = computer::answered(&mut watching.computer.answers),
            if watching.computer.answers.is_some() =>
        {
            watching = match on_computer_answer(*answer, seams, watching, look, turns).await {
                Ok(next) => next,
                Err(end) => return Err(end),
            };
        }
        view = computer::next_view(&mut watching.computer.views),
            if watching.computer.views.is_some() =>
        {
            turns.quiet = computer_view_back(&mut watching, view);
        }
        // The typing paused: the last search question goes to the daemon.
        () = jobs::due(search_due), if search_due.is_some() => {
            watching.jobs.send_due();
        }
        done = jobs::next(&mut watching.jobs.ended) => {
            watching = match on_job_done(done, seams, watching, look, turns).await {
                Ok(next) => next,
                Err(end) => return Err(end),
            };
        }
        _ = host::tick(&mut watching.host.poll), if watching.host.polls(&watching.app) => {
            watching.host.send(host::SinkOp::Read);
        }
        state = host::answer(&mut watching.host.states), if watching.host.waiting() => {
            watching.host.answered(state, &mut watching.app);
        }
        () = host::redial(&mut watching.host.redial), if watching.host.redial.is_some() => {
            watching.host.redialled(&watching.app);
        }
        found = host::connected(&mut watching.host.connecting), if watching.host.connecting.is_some() => {
            watching.host.found(found, &mut watching.app);
        }
        change = host::next_change(&mut watching.host.changes), if watching.host.changes.is_some() => {
            watching.host.changed(change, &mut watching.app);
        }
        why = host::refused(&mut watching.host.refusals),
            if watching.host.refusals.is_some() => {
            watching.app.notify(why);
        }
        block = host::next_block(&mut watching.host.capture), if watching.host.capturing() => {
            turns.quiet = on_capture_block(block, &mut watching, look, turns.timers.period);
        }
        step = host::next_frames(&mut watching.host.frames), if watching.host.listening() => {
            turns.quiet = on_capture_frames(step, &mut watching, look, turns.timers.period);
        }
        // A step toward the cover art finished: the next frame draws what it brought.
        done = watching.host.art.next() => {
            watching.host.art.fold(done, &mut watching.app);
        }
    }
    Ok(watching)
}

/// Takes the answer to the pairing-code ask, which is no longer pending.
fn web_code_back(watching: &mut Watching, look: &mut Look, asked: Option<web_box::Asked>) {
    watching.web_code = None;
    if let Some(asked) = asked {
        web_box::take_answer(watching, look, asked, Instant::now());
    }
}

/// Takes the answer to the web check, which is no longer pending.
fn web_check_back(watching: &mut Watching, checked: Option<web_box::Checked>) {
    watching.web_check = None;
    if let Some(checked) = checked {
        web_box::checked(watching, checked, Instant::now());
    }
}

/// Folds a computed EQ view into the app, and whether the frame can be skipped because the view
/// is the one already held.
fn computer_view_back(watching: &mut Watching, view: Box<svolder_host::eq::EqView>) -> bool {
    let quiet = watching.app.host.computer.eq.as_deref() == Some(&*view);
    let before = watching.app.computer_points();
    watching.app.host.computer.eq = Some(view);
    watching.app.follow_point(&before);
    watching.app.note_computer_view();
    quiet
}

/// Carries out what the terminal said: a key, the pointer, a mouse action, or the end of input.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn on_terminal_event<O, B, S, M>(
    event: Option<std::io::Result<Event>>,
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &mut Look,
    turns: &mut Turns,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
    M: MouseCapture,
{
    match event {
        // A release is the other half of a press this loop has already acted on.
        Some(Ok(Event::Key(key))) if key.kind != KeyEventKind::Release => {
            look.effects.on_key(Instant::now());
            let (mut screen, opener, mouse) = seams.split(&mut turns.hits);
            let flow = take_key(key, opener, watching, mouse, look, &mut screen).await;
            carried(flow, &mut turns.adding)
        }
        // The pointer resting somewhere is no action: it only asks for a tooltip, and
        // stops no drag or climb.
        Some(Ok(Event::Mouse(event)))
            if watching.app.mouse && event.kind == MouseEventKind::Moved =>
        {
            let before = watching.app.pointer.tooltip.map(|t| (t.hit, t.shown));
            tooltip::hover(&mut watching.app, &turns.hits, &event, Instant::now());
            let over = turns.hits.hit_at(event.column, event.row) == Some(&mouse::Hit::WebLink);
            let was_over = std::mem::replace(&mut watching.app.pointer.over_link, over);
            // Motion that changes nothing visible (the pointer still resting on the
            // same target) asks for no frame: a terminal reports hundreds a second.
            turns.quiet = before == watching.app.pointer.tooltip.map(|t| (t.hit, t.shown))
                && was_over == over;
            Ok(watching)
        }
        // Mouse support turns a position back into the intent a key asks for, and this
        // loop carries it out exactly the same way.
        Some(Ok(Event::Mouse(event))) if watching.app.mouse => {
            // A press, a drag, a release or the wheel hides the tooltip, and ends the
            // pointer's rest on the link.
            tooltip::hover(&mut watching.app, &turns.hits, &event, Instant::now());
            watching.app.pointer.over_link = false;
            // A click, on an action or not, ends the intro's exit.
            if matches!(event.kind, MouseEventKind::Down(_)) {
                look.effects.end_reform();
            }
            // A press of a held drag's own button is its release lost on the way: let
            // the drag go where it last was before reading the press afresh.
            let lost = curve_drag::lost(&mut watching.app, &event);
            send(lost, &mut watching).await;
            let lost = glide::lost(&mut watching.app, &event);
            send(lost, &mut watching).await;
            divider::lost(&mut watching.app, &event);
            // A mouse action ends any code half typed with the keys.
            watching.app.mascot.codes.clear();
            // The present moment, which a climb a press starts counts from.
            watching.app.now = Instant::now();
            mouse::note_press(&mut watching.app, &event, Instant::now());
            let action = mouse::resolve(&turns.hits, event, &watching.app);
            // A press or a wheel step anywhere but on the Volume bar stops the climb.
            glide::mouse_event(&mut watching.app, event.kind, action.as_ref());
            let Some(action) = action else {
                return Ok(watching);
            };
            look.effects.on_key(Instant::now());
            let (mut screen, opener, mouse) = seams.split(&mut turns.hits);
            let flow = apply_action(action, opener, watching, mouse, look, &mut screen).await;
            carried(flow, &mut turns.adding)
        }
        Some(Ok(_)) => Ok(watching),
        // Nothing left to read input from: there is no loop to keep turning.
        Some(Err(_)) | None => Err(LoopEnd {
            watching,
            code: 1,
            why: Some(INPUT_ENDED.to_string()),
        }),
    }
}

/// Folds a device update into the app, or opens the device a wait is for once it is up.
///
/// A find, whether held for an open box or taken up by a switch, ends the turn before `send_held`.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn on_update<O, B, S, M>(
    update: Result<Option<Update>, BleError>,
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &Look,
    turns: &mut Turns,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    // The device a wait is for coming up is the whole of what it waits on: it is
    // opened in full, as a reconnect opens it, so its real profile and readings
    // replace the panel without a key from the user.
    if found(&watching, &update) {
        if watching.app.modal_open() {
            turns.found_under_box = true;
            return Ok(watching);
        }
        return switch_or_end(seams, watching, look, &mut turns.hits).await;
    }
    let socket = watching.backend.as_ref().and_then(Backend::daemon_socket);
    match socket {
        // A daemon's feed ends when its connection closes, which a restart does:
        // wait for the next one rather than give the device up.
        Some(socket) if matches!(update, Ok(None)) => {
            watching.wait_for_daemon(socket, seams.opener.daemon_wait);
        }
        _ => {
            if let Some(backend) = watching.backend.as_mut() {
                let now = Instant::now();
                watching.feed_open = heard(update, backend, &mut watching.app, now).await;
            }
        }
    }
    send_held(&mut watching).await;
    Ok(watching)
}

/// A tick ages what is on screen, and sends what the keys held back.
async fn on_tick(watching: &mut Watching) {
    watching.app.tick(Instant::now());
    super::start::say_daemon_gone(
        &mut watching.app,
        crate::commands::demo::daemon_gone(),
        std::time::Instant::now(),
    );
    web_box::tick(watching, Instant::now());
    send_held(watching).await;
}

/// The device list's own clock: a wait takes the device up if it is there, else the list is
/// asked again.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn on_relist<O, B, S, M>(
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &Look,
    turns: &mut Turns,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    // A wait does not rest on the feed alone: the same tick takes the subscription
    // up again while it is closed, and asks the daemon's list whether the device is
    // up, so a missed event or a daemon that restarted still ends it.
    if waiting(&watching) && !watching.app.modal_open() && wait_over(&mut watching).await {
        watching = switch_or_end(seams, watching, look, &mut turns.hits).await?;
    } else if relisting(&watching) {
        relist_held(&mut watching).await;
    }
    Ok(watching)
}

/// Opens the device again once a daemon that closed its connection answers, or gives it up.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn on_daemon_back<O, B, S, M>(
    answered: bool,
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &Look,
    turns: &mut Turns,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    let socket = watching
        .reconnect
        .take()
        .map(|wait| wait.socket.clone())
        .unwrap_or_default();
    let (mut screen, opener, _) = seams.split(&mut turns.hits);
    match daemon_back(answered, socket, opener, watching, look, &mut screen).await {
        Flow::Continue(next) => Ok(next),
        Flow::Adding(next) => Ok(next),
        Flow::Ended(end) => Err(end),
    }
}

/// Tells the user what a change of the computer's controls ended with, and carries out what it
/// asks next: the question of a preset a device plays too, or the DAC's load of a preset the
/// computer has let go of.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn on_computer_answer<O, B, S, M>(
    answer: computer::Answer,
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &mut Look,
    turns: &mut Turns,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
    M: MouseCapture,
{
    let intents = match answer {
        computer::Answer::Said(why) => {
            // A volume the daemon would not take stands no longer.
            watching.app.host.asked = None;
            let flattened = watching.app.presets.computer_flattened.take();
            match flattened {
                Some(name) => watching.app.warn(format!(
                    "{why} The DAC was flattened; put the curve back with Enter on {name} in \
                     the presets box, or `svolder eq load {name}`."
                )),
                None => watching.app.warn(why),
            }
            Vec::new()
        }
        computer::Answer::Asked { name, .. } => {
            presets::computer::asked(&mut watching.app, &name);
            Vec::new()
        }
        computer::Answer::Moved(name) => presets::computer::moved(&mut watching.app, &name),
    };
    let (mut screen, opener, mouse) = seams.split(&mut turns.hits);
    let flow = take_intents(intents, opener, watching, mouse, look, &mut screen).await;
    carried(flow, &mut turns.adding)
}

/// Carries out what a finished job asks of the loop.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
async fn on_job_done<O, B, S, M>(
    done: presets::Done,
    seams: &mut Seams<'_, O, B, S, M>,
    mut watching: Watching,
    look: &mut Look,
    turns: &mut Turns,
) -> Result<Watching, LoopEnd>
where
    O: Open,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
    M: MouseCapture,
{
    let (mut screen, opener, mouse) = seams.split(&mut turns.hits);
    let intents = folded(done, opener, &mut watching);
    let flow = take_intents(intents, opener, watching, mouse, look, &mut screen).await;
    carried(flow, &mut turns.adding)
}

/// Fold what a job ended with into the app and answer with the writes it asks for; a write of the
/// preset library that has ended, whichever way, also has the list read the file again.
fn folded<O: Open>(
    done: presets::Done,
    opener: &Opener<O>,
    watching: &mut Watching,
) -> Vec<Intent> {
    let wrote = matches!(done, presets::Done::PresetWritten(_));
    let intents = presets::done(&mut watching.app, done);
    if wrote {
        preset_landed(opener, watching);
    }
    intents
}

/// Folds a block the host sink capture read into the app, and says whether the turn is quiet.
fn on_capture_block(
    block: Option<Block>,
    watching: &mut Watching,
    look: &Look,
    period: Duration,
) -> bool {
    // A block alone asks for no frame unless it sets the Spectrum row moving (which
    // puts the loop on the fast tick); the capture stopping by itself always does.
    let landed = block.is_some();
    watching.host.took(block, &mut watching.app);
    landed && tick_period(&watching.app, &look.effects) == period
}

/// Folds what the daemon's frames connection brought into the app, and says whether the turn is
/// quiet: a frame alone asks for no frame of its own, as a block of the loop's own capture asks
/// for none; the connection opening, ending or reporting its state always does.
fn on_capture_frames(
    step: host::Step,
    watching: &mut Watching,
    look: &Look,
    period: Duration,
) -> bool {
    let landed = watching.host.framed(step, &mut watching.app);
    landed && tick_period(&watching.app, &look.effects) == period
}

/// Whether the loop turns again, and with what it is watching now, or has ended.
///
/// `take_intent` and `apply_action` both take `watching` by value and hand it back inside this:
/// `Quit` moves it into the [`LoopEnd`] it returns, which `&mut Watching` could not do, so every
/// path hands it back one way or another rather than mutating in place.
enum Flow {
    /// The loop goes on, watching this.
    Continue(Watching),
    /// The loop goes on, but walks someone through adding a device first: the wizard draws on
    /// the terminal and reads the input this loop owns, so it runs between turns, not inside
    /// the arm that asked for it.
    Adding(Watching),
    /// The loop is done.
    Ended(LoopEnd),
}

/// The end a give-up key pressed during a device switch earns: the way `q` ends the loop, with
/// whatever the switch left held handed back to be released.
fn gave_up(watching: Watching) -> Flow {
    Flow::Ended(LoopEnd {
        watching,
        code: 0,
        why: None,
    })
}

/// What the loop is left with after one key or click: what it watches now, or the end it
/// earned, with `adding` raised for a walk the next turn runs first.
///
/// The end carries the device being watched, app and all: what makes it large. It is the
/// loop's own way out rather than an error, and it travels once per run.
#[allow(
    clippy::result_large_err,
    reason = "the end carries what the loop was watching, once per run"
)]
fn carried(flow: Flow, adding: &mut bool) -> Result<Watching, LoopEnd> {
    match flow {
        Flow::Continue(watching) => Ok(watching),
        Flow::Adding(watching) => {
            *adding = true;
            Ok(watching)
        }
        Flow::Ended(end) => Err(end),
    }
}

#[cfg(test)]
mod tests;
