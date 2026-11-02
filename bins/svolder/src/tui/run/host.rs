// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The host's own audio as the loop reaches it: the default sink, read and written on a task of
//! its own so no `wpctl` call ever holds up a frame, and the media players on the session bus,
//! followed once the bus has answered.
//!
//! The sink is read every [`POLL`] while the device screen shows the Host group, and at once
//! after every write; its operations run one at a time on that task, in the order they were
//! asked for, so two quick writes reach the sink in the order the keys were pressed. The bus is
//! connected to the first time host audio is on; one that was found and goes away is looked for
//! again every [`BUS_RETRY`], and so is one that took the connection and has not answered
//! ([`NoBus::Silent`]: a look at it is refused while an earlier one still waits, so none leaves
//! another thread behind); one that was never there is not. While host audio is off the looks
//! stop, and the next turn it shows again looks at once. What the players say arrives as a change
//! and is folded into the app's [`Host`](crate::tui::host::Host).
//!
//! Nothing is sent to the host but what a key, a click or the wheel on the Host group asks for:
//! a press goes out on a task of its own, so a player slow to answer holds up nothing either.
//!
//! On a session joined to a daemon whose host side is available, the daemon is the authority for
//! the player, the transport and the path ([`Host::daemon`](crate::tui::host::Host::daemon)):
//! its view arrives on the update feed, a press goes to it as a request, and the players the
//! loop's own watcher found, if it had started before the daemon spoke, are held and ignored
//! until the daemon stops speaking for the host. The sink and the capture stay the loop's own.
//!
//! The capture of the default sink's monitor is started and stopped after every frame, as the
//! app says it is needed ([`App::capture_target`]): only ever on the node the sink named, and
//! only while something drawn follows what it reads. Its blocks are folded into the app as they
//! land, and it is stopped (the `pw-record` killed and reaped) the moment nothing needs it,
//! whenever something holds the loop away from its blocks (the add-device walk, a device
//! switch), and when the loop ends. A sink the capture could not start on, or stopped on by
//! itself, is left alone for [`CAPTURE_RETRY`], so a `pw-record` that cannot run is not spawned
//! again on every frame.
//!
//! On a session joined to a daemon whose host side is available, the daemon's own capture, its
//! analysed frames, feeds the row, the level and the beats in place of the loop's own, over a
//! connection of its own that is open only while the same need holds: see [`frames`]. The loop's
//! own capture stays stopped while that connection is open or opening, and takes over again when
//! the daemon cannot give frames.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::future::BoxFuture;
use svolder_host::players::Changes;
use svolder_host::{
    Block, Button, Capture, Mpris, NoBus, NowPlaying, Players, PwRecord, Sink, SinkState, Wpctl,
};
use svolder_ipc::Response;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Instant, Interval, MissedTickBehavior};

use crate::tui::app::{App, Screen};

/// How often the sink is read while the device screen shows the Host group.
pub(super) const POLL: Duration = Duration::from_secs(1);

/// What a press the daemon did not answer is reported as.
const PRESS_UNANSWERED: &str = "the daemon did not answer the press";

/// How long a sink the capture could not start on, or stopped on by itself, is left before
/// the capture is tried on it again.
pub(super) const CAPTURE_RETRY: Duration = Duration::from_secs(5);

/// One thing asked of the sink, each answered with a read of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum SinkOp {
    /// Read it.
    Read,
    /// Set its volume to this percent.
    Volume(u8),
    /// Mute it when it plays and unmute it when it is muted.
    ToggleMute,
}

/// What finds the players: called the first time host audio is on, and again every [`BUS_RETRY`]
/// once a bus it found has gone away, or one that took the connection has not answered. It says
/// why there is no bus ([`NoBus`]) when there is none.
pub(super) type Connect =
    Box<dyn FnMut() -> BoxFuture<'static, Result<Arc<dyn Players>, NoBus>> + Send>;

/// How long after a bus that was found went away, or a look for it found none, the players are
/// looked for again.
pub(super) const BUS_RETRY: Duration = Duration::from_secs(10);

/// A task that is ended when this is dropped.
pub(super) struct Aborting<T>(pub(super) JoinHandle<T>);

/// The search for the players, ended with the link that started it.
pub(super) type Search = Aborting<Result<Arc<dyn Players>, NoBus>>;

impl<T> Drop for Aborting<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The host's own audio as the loop reaches it.
///
/// Each field the loop waits on is a field of its own, so each `select!` arm borrows only
/// the one it reads.
#[derive(Default)]
pub(in crate::tui) struct HostLink {
    /// Where the sink's operations are sent, `None` with no sink to reach.
    ops: Option<mpsc::UnboundedSender<SinkOp>>,
    /// What each operation's read of the sink answered, in the order they were sent.
    pub(super) states: Option<mpsc::UnboundedReceiver<Option<SinkState>>>,
    /// How many operations have been sent whose read has not answered yet.
    outstanding: usize,
    /// The task running the sink's operations.
    _worker: Option<Aborting<()>>,
    /// What finds the players.
    connect: Option<Connect>,
    /// Whether the first look for the players has been made, which is once per session.
    looked: bool,
    /// Whether a bus has ever been found: only one that was, or one that was silent, is looked
    /// for again.
    seen_bus: bool,
    /// Whether a look is due that host audio being off held back: the next turn it shows again
    /// makes it.
    owed: bool,
    /// When the players are looked for again, once the bus found has gone away.
    pub(super) redial: Option<Instant>,
    /// The search for the players, while it runs.
    pub(super) connecting: Option<Search>,
    /// The players, once the bus has answered, until it goes away.
    players: Option<Arc<dyn Players>>,
    /// What the players say from the moment they were found, until the bus goes away.
    pub(super) changes: Option<Changes>,
    /// The clock the sink is read on while the device screen shows.
    pub(super) poll: Option<Interval>,
    /// The capture of the default sink's monitor, `None` on a host it cannot reach.
    pub(super) capture: Option<Box<dyn Capture>>,
    /// The sink the capture runs on, while it runs.
    capturing: Option<String>,
    /// The sink the capture last could not start on, or stopped on by itself, and when: it is
    /// not started there again until [`CAPTURE_RETRY`] has passed.
    refused: Option<(String, Instant)>,
    /// The daemon's analysed frames of the host's capture, while they are asked for or read: the
    /// source of the Spectrum row, the level and the beats then, in place of `capture`.
    pub(super) frames: frames::FramesLink,
    /// Why the frames are not asked for now, once a try has not given any.
    barred: frames::Barred,
    /// Whether a frame of the daemon has landed since the connection opened, which is when the
    /// loop's own capture stops: until then it feeds the row.
    fed: bool,
    /// Whether the daemon was the authority for the player when the loop last looked, which is
    /// what a change of authority is found against.
    deferring: bool,
    /// Why the daemon refused a press, once it said so; made with the first press sent to it.
    pub(super) refusals: Option<mpsc::UnboundedReceiver<String>>,
    /// Where the tasks that press through the daemon say it refused.
    refuse: Option<mpsc::UnboundedSender<String>>,
}

impl HostLink {
    /// A loop that reaches nothing of the host.
    pub(in crate::tui) fn none() -> HostLink {
        HostLink::default()
    }

    /// The host this session runs on, reached through `wpctl`, `pw-record` and the session
    /// bus, or nothing at all when `held_off` (`SVOLDER_HOST=off`) says so.
    pub(in crate::tui) fn session(held_off: bool) -> HostLink {
        if held_off {
            return HostLink::none();
        }
        let sink = Wpctl::from_env().map(|wpctl| Arc::new(wpctl) as Arc<dyn Sink>);
        let connect: Connect = Box::new(|| {
            Box::pin(async {
                Mpris::look_session()
                    .await
                    .map(|mpris| Arc::new(mpris) as Arc<dyn Players>)
            })
        });
        let link = HostLink::over(sink, connect);
        match PwRecord::from_env() {
            Some(capture) => link.with_capture(Box::new(capture)),
            None => link,
        }
    }

    /// This link, capturing the default sink's monitor through `capture` whenever the app
    /// needs it.
    pub(in crate::tui) fn with_capture(mut self, capture: Box<dyn Capture>) -> HostLink {
        self.capture = Some(capture);
        self
    }

    /// The host reached through `sink` — none for a host with no sink to reach — and the
    /// players `connect` finds. Made inside the runtime the loop runs on, which runs the task
    /// the sink's operations go through.
    pub(in crate::tui) fn over(sink: Option<Arc<dyn Sink>>, connect: Connect) -> HostLink {
        let mut link = HostLink {
            connect: Some(connect),
            ..HostLink::default()
        };
        if let Some(sink) = sink {
            let (ops, asked) = mpsc::unbounded_channel();
            let (answers, states) = mpsc::unbounded_channel();
            link.ops = Some(ops);
            link.states = Some(states);
            link._worker = Some(Aborting(tokio::spawn(serve(sink, asked, answers))));
            let mut poll = tokio::time::interval(POLL);
            poll.set_missed_tick_behavior(MissedTickBehavior::Delay);
            link.poll = Some(poll);
        }
        link
    }

    /// Hand the player to whoever speaks for it now: the daemon once its host side is
    /// available, and the loop's own watcher — the players it held, else nothing until it finds
    /// some — once the daemon stops, or the session is on a daemon with none.
    pub(super) fn follow(&mut self, app: &mut App) {
        let daemon = app.host.daemon_owns();
        if daemon == self.deferring {
            return;
        }
        self.deferring = daemon;
        // Another daemon may give what the last could not.
        self.barred = frames::Barred::No;
        if !daemon {
            self.drop_frames(app);
            match &self.players {
                Some(players) => app.host.found_bus(players.now_playing(), app.now),
                None => app.host.lost_bus(),
            }
        }
    }

    /// Start looking for the players, the first time `app` shows host audio and has no daemon
    /// to speak for the host: once per session.
    pub(super) fn wake(&mut self, app: &App) {
        if !app.host_shown() || app.host.daemon_owns() {
            return;
        }
        // The first look, or one that came due while host audio was off.
        if !std::mem::replace(&mut self.looked, true) || std::mem::take(&mut self.owed) {
            self.dial();
        }
    }

    /// Start a look for the players, unless one is under way.
    fn dial(&mut self) {
        if self.connecting.is_none()
            && let Some(connect) = self.connect.as_mut()
        {
            self.connecting = Some(Aborting(tokio::spawn(connect())));
        }
    }

    /// The time to look for the bus again has come: look, if host audio still shows. If it does
    /// not, no timer is kept running for nothing: the look is owed, and [`HostLink::wake`] makes it
    /// the turn host audio shows again.
    pub(super) fn redialled(&mut self, app: &App) {
        self.redial = None;
        if app.host_shown() {
            self.dial();
        } else {
            self.owed = true;
        }
    }

    /// Whether the sink is due a read on the poll's clock: host audio shows, the device screen
    /// is up, and nothing asked of the sink is still waiting on its read.
    pub(super) fn polls(&self, app: &App) -> bool {
        app.host_shown()
            && app.screen == Screen::Device
            && self.ops.is_some()
            && self.outstanding == 0
    }

    /// Whether something asked of the sink is waiting on its read.
    pub(super) fn waiting(&self) -> bool {
        self.outstanding > 0
    }

    /// Ask `op` of the sink, and say whether it was asked: never on a host with no sink to
    /// reach.
    pub(super) fn send(&mut self, op: SinkOp) -> bool {
        let sent = self.ops.as_ref().is_some_and(|ops| ops.send(op).is_ok());
        if sent {
            self.outstanding += 1;
        }
        sent
    }

    /// One read answered: fold it into `app`, saying whether it answered the last thing asked.
    pub(super) fn answered(&mut self, state: Option<SinkState>, app: &mut App) {
        self.outstanding = self.outstanding.saturating_sub(1);
        app.host.fold_read(state, self.outstanding == 0);
    }

    /// Press `button` on the followed player, on a task of its own so a player slow to answer
    /// holds up nothing, and say whether there were players to press it on.
    pub(super) fn press(&self, button: Button) -> bool {
        let Some(players) = self.players.clone() else {
            return false;
        };
        tokio::spawn(async move { players.press(button).await });
        true
    }

    /// Send a press to the daemon, `ask` being the request to it, on a task of its own: which
    /// says why when the daemon refused or did not answer.
    pub(super) fn press_through(
        &mut self,
        ask: impl Future<Output = Option<Response>> + Send + 'static,
    ) {
        let refuse = match &self.refuse {
            Some(refuse) => refuse.clone(),
            None => {
                let (refuse, refusals) = mpsc::unbounded_channel();
                self.refusals = Some(refusals);
                self.refuse = Some(refuse.clone());
                refuse
            }
        };
        tokio::spawn(async move {
            let why = match ask.await {
                Some(Response::Ack) => return,
                Some(Response::Error { message, .. }) => message,
                Some(_) | None => PRESS_UNANSWERED.to_string(),
            };
            let _ = refuse.send(why);
        });
    }

    /// The search for the players ended with `found`: follow them, or say there are none. While
    /// the daemon speaks for the host the players are held and nothing is said.
    pub(super) fn found(&mut self, found: Result<Arc<dyn Players>, NoBus>, app: &mut App) {
        self.connecting = None;
        let held = app.host.daemon_owns();
        match found {
            Ok(players) => {
                self.seen_bus = true;
                self.redial = None;
                // The changes are asked for before what plays now is read, so nothing that
                // changes between the two is missed.
                self.changes = Some(players.changes());
                if !held {
                    app.host.found_bus(players.now_playing(), app.now);
                }
                self.players = Some(players);
            }
            Err(why) => {
                // A bus that took the connection counts as one seen: it is looked for until it
                // answers, whatever the looks in between find.
                self.seen_bus |= why == NoBus::Silent;
                if !held {
                    app.host.lost_bus();
                }
                self.look_again();
            }
        }
    }

    /// Look for a bus again in [`BUS_RETRY`], if one was found once or took the connection: a
    /// bus that is not there, and was never, is not waited for.
    fn look_again(&mut self) {
        if self.seen_bus {
            self.redial = Some(Instant::now() + BUS_RETRY);
        }
    }

    /// The players said `change`: what the followed one plays now, or, with `None`, that the
    /// bus has gone and there is nothing left to follow.
    pub(super) fn changed(&mut self, change: Option<Option<NowPlaying>>, app: &mut App) {
        let held = app.host.daemon_owns();
        match change {
            Some(playing) if !held => app.host.fold_playing(playing, app.now),
            Some(_) => {}
            None => {
                self.changes = None;
                self.players = None;
                self.look_again();
                if !held {
                    app.host.lost_bus();
                }
            }
        }
    }

    /// Start the capture on the sink `app` needs it on, move it to another sink — its readings
    /// starting again from nothing there — or stop it where `app` needs none
    /// ([`App::capture_target`]). A sink the capture could not start
    /// on, or stopped on by itself, is left alone until [`CAPTURE_RETRY`] has passed; a sink
    /// that stops being needed forgets that, so needing it again tries again at once.
    pub(super) fn steer(&mut self, app: &mut App) {
        let feeds = self.daemon_feeds();
        let Some(capture) = self.capture.as_mut() else {
            return;
        };
        // The daemon's capture feeds the row once its first frame has landed: the host is not
        // recorded twice. Before that the own capture runs, so the row never waits on a daemon.
        let wanted = match feeds {
            true => None,
            false => app.capture_target().map(str::to_string),
        };
        let Some(sink) = wanted else {
            self.refused = None;
            if self.capturing.take().is_some() {
                capture.stop();
                app.host.capture_stopped();
            }
            return;
        };
        if self.capturing.as_deref() == Some(sink.as_str()) {
            return;
        }
        let now = Instant::now();
        let refused = self.refused.as_ref().is_some_and(|(refused, at)| {
            *refused == sink && now.saturating_duration_since(*at) < CAPTURE_RETRY
        });
        if refused {
            return;
        }
        if capture.start(&sink) {
            self.refused = None;
            // What another sink played is not this one's: a capture that moves starts its
            // levels and bands from nothing.
            if self.capturing.replace(sink).is_some() {
                app.host.capture_stopped();
            }
            app.host.capturing = true;
        } else {
            self.refused = Some((sink, now));
            self.capturing = None;
            app.host.capture_stopped();
        }
    }

    /// Stop the own capture, if it runs, without forgetting what the row holds: the daemon's
    /// frames replace it.
    fn stop_own_capture(&mut self) {
        if let Some(capture) = self.capture.as_mut()
            && self.capturing.take().is_some()
        {
            capture.stop();
        }
        self.refused = None;
    }

    /// Whether a capture runs, which is while the loop waits on its blocks.
    pub(super) fn capturing(&self) -> bool {
        self.capturing.is_some()
    }

    /// The capture read `block`, folded into `app`; or — `None` — it stopped by itself, and
    /// the sink it ran on is left alone until [`CAPTURE_RETRY`] has passed.
    pub(super) fn took(&mut self, block: Option<Block>, app: &mut App) {
        match block {
            Some(block) => app.heard_block(&block, std::time::Instant::now()),
            None => {
                if let Some(sink) = self.capturing.take() {
                    self.refused = Some((sink, Instant::now()));
                }
                app.host.capture_stopped();
            }
        }
    }

    /// Stop the capture, if one runs: the loop is over, and nothing is recorded past it.
    pub(super) fn stop_capture(&mut self, app: &mut App) {
        self.drop_frames(app);
        if let Some(capture) = self.capture.as_mut()
            && self.capturing.take().is_some()
        {
            capture.stop();
        }
        app.host.capture_stopped();
    }
}

/// The capture's next block — `None` once, when it has stopped by itself — or never while none
/// runs.
pub(super) async fn next_block(capture: &mut Option<Box<dyn Capture>>) -> Option<Block> {
    match capture {
        Some(capture) => capture.next_block().await,
        None => std::future::pending().await,
    }
}

/// Run what is asked of `sink` one at a time, in order, answering each with a read of it.
async fn serve(
    sink: Arc<dyn Sink>,
    mut asked: mpsc::UnboundedReceiver<SinkOp>,
    answers: mpsc::UnboundedSender<Option<SinkState>>,
) {
    while let Some(op) = asked.recv().await {
        match op {
            SinkOp::Read => {}
            SinkOp::Volume(percent) => {
                sink.set_volume(f32::from(percent) / 100.0).await;
            }
            SinkOp::ToggleMute => {
                sink.toggle_mute().await;
            }
        }
        if answers.send(sink.read().await).is_err() {
            return;
        }
    }
}

/// The next read the sink answered, or never with no sink to read.
pub(super) async fn answer(
    states: &mut Option<mpsc::UnboundedReceiver<Option<SinkState>>>,
) -> Option<SinkState> {
    match states {
        Some(states) => match states.recv().await {
            Some(state) => state,
            None => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

/// What the search for the players found, or never while none runs.
pub(super) async fn connected(connecting: &mut Option<Search>) -> Result<Arc<dyn Players>, NoBus> {
    match connecting {
        Some(search) => (&mut search.0).await.unwrap_or(Err(NoBus::Absent)),
        None => std::future::pending().await,
    }
}

/// When the bus is next looked for, or never while it is not to be.
pub(super) async fn redial(at: &mut Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(*at).await,
        None => std::future::pending().await,
    }
}

/// The players' next change — `None` once the bus has gone — or never while none are followed.
pub(super) async fn next_change(changes: &mut Option<Changes>) -> Option<Option<NowPlaying>> {
    match changes {
        Some(changes) => changes.next().await,
        None => std::future::pending().await,
    }
}

/// Why the daemon refused a press, or never while none has been sent.
pub(super) async fn refused(refusals: &mut Option<mpsc::UnboundedReceiver<String>>) -> String {
    match refusals {
        Some(refusals) => match refusals.recv().await {
            Some(why) => why,
            None => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

/// The poll's next tick, or never on a host with no sink to read.
pub(super) async fn tick(poll: &mut Option<Interval>) {
    match poll {
        Some(poll) => {
            poll.tick().await;
        }
        None => std::future::pending().await,
    }
}

mod frames;
pub(super) use frames::{Step, next_step as next_frames};

#[cfg(test)]
mod joined;
#[cfg(test)]
mod tests;
