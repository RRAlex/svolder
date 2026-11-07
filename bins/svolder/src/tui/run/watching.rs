// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the loop is watching: the app and the backend it reads, the state a switch carries over
//! from one open to the next, and the screen a wait inside a turn draws on.

use super::*;

/// What an open takes over from the app it replaces: the screen shown, mouse capture, and the
/// host's state and the squirrel.
pub(in crate::tui) struct Kept {
    pub(super) screen: Screen,
    pub(super) mouse: bool,
    pub(super) host: crate::tui::host::Host,
    /// Which side the volume and the tone control were shown for.
    pub(super) side: crate::tui::app::Side,
    /// Whether `[tui] host_audio` was on when the app was replaced: what the session says now,
    /// which the opener's own table, read at the start, may not.
    pub(super) host_audio: bool,
    pub(super) mascot: crate::tui::decoration::mascot::Mascot,
    pub(super) daemon_version_warned: Option<String>,
    /// The accent colours of the user's own the Settings box holds as stops.
    pub(super) held_accents: svolder_ipc::FamilyAccents,
    /// The device's unit id and the histories its readings built, which a reopen of the same
    /// device (a daemon restart, a refresh) carries on and a switch to another device drops.
    pub(super) unit: Option<String>,
    pub(super) histories: crate::tui::widgets::graph::Histories,
    pub(super) link: crate::tui::widgets::graph::Link,
}

impl Kept {
    /// What `app` hands on to the app that replaces it.
    pub(in crate::tui) fn of(app: &App) -> Kept {
        Kept {
            screen: app.screen,
            mouse: app.mouse,
            host: app.host.clone(),
            side: app.side,
            host_audio: app.tui.host_audio,
            mascot: app.mascot.clone(),
            daemon_version_warned: app.session.daemon_version_warned.clone(),
            held_accents: app.overlays.held_accents.clone(),
            unit: app.session.unit.clone(),
            histories: app.readings.histories.clone(),
            link: app.readings.link.clone(),
        }
    }
}

/// Where a wait inside the loop draws: something that shows `app` once, as the terminal does.
///
/// A device switch holds the loop while it closes one device and opens another, so it draws
/// through this between its awaits, and reads the keys that give up on it through it too. A
/// test hands it a closure or `()`, which draw and read nothing, or a stream of its own.
pub(in crate::tui) trait Shown {
    /// Draw one frame of `app`.
    fn frame(&mut self, app: &mut App, look: &Look);

    /// The next event the terminal has to say, or `None` once it has nothing left to say. A
    /// screen with no input to read never answers.
    fn next_event(&mut self) -> LocalBoxFuture<'_, Option<std::io::Result<Event>>> {
        Box::pin(std::future::pending())
    }
}

/// Nothing is drawn: for a case that only cares what a wait ends with.
impl Shown for () {
    fn frame(&mut self, _app: &mut App, _look: &Look) {}
}

impl<F: FnMut(&mut App, &Look)> Shown for F {
    fn frame(&mut self, app: &mut App, look: &Look) {
        self(app, look);
    }
}

/// The terminal the loop draws on, as a wait inside a turn draws on it.
///
/// The input it reads is the loop's own stream, so a key pressed during a wait is read by the
/// wait and not left behind for the loop.
pub(super) struct OnScreen<'a, B: ratatui::backend::Backend, S> {
    pub(super) terminal: &'a mut Terminal<B>,
    pub(super) hits: &'a mut HitMap,
    pub(super) events: &'a mut S,
}

impl<B, S> Shown for OnScreen<'_, B, S>
where
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    fn next_event(&mut self) -> LocalBoxFuture<'_, Option<std::io::Result<Event>>> {
        Box::pin(self.events.next())
    }

    /// The frame the loop would draw, without the animation layer: it holds still while the
    /// wait lasts. A terminal that cannot be drawn to is left for the loop's own next frame to
    /// report.
    fn frame(&mut self, app: &mut App, look: &Look) {
        if let Ok(size) = self.terminal.size() {
            app.resize(size.width, size.height);
        }
        let _ = self.terminal.draw(|frame| {
            let areas = render::draw(frame, app, &look.theme, self.hits);
            spectrum_over(frame, &areas, &look.theme, &look.effects);
        });
    }
}

/// What the loop is watching.
pub(in crate::tui) struct Watching {
    /// The backend it reads, or nothing at all once opening a device and falling back to the
    /// one before it have both failed.
    pub(super) backend: Option<Backend>,
    /// The app the backend fills and every frame draws, boxed so the loop's futures do not carry
    /// it by value.
    pub(in crate::tui) app: Box<App>,
    /// The device the backend is on, as `-d` names one, or `None` for whichever device the
    /// configuration itself picks.
    pub(super) device: Option<String>,
    /// Whether the update feed is still worth reading.
    ///
    /// Cleared the moment the backend reports nothing more to come, and set again only by
    /// opening a device afresh. An ended feed answers instantly forever, so the arm reading it
    /// must stop being polled rather than spin on that same answer every turn.
    pub(super) feed_open: bool,
    /// Where the daemon's version lands once the request [`Opener::reopen`] spawned for this
    /// connection answers, over its own socket so the draw loop never waits on it. Taken the
    /// moment it answers, so the arm reading it stops being polled; a direct session, and a
    /// daemon's connection before the answer, both leave this `None`.
    pub(super) daemon_version: Option<tokio::sync::oneshot::Receiver<Option<DaemonInfo>>>,
    /// A web code asked of the daemon, while it is on its way.
    pub(super) web_code: Option<tokio::sync::oneshot::Receiver<super::web_box::Asked>>,
    /// A check on whether the link on show is still open, asked of the daemon, while it is on
    /// its way.
    pub(super) web_check: Option<tokio::sync::oneshot::Receiver<super::web_box::Checked>>,
    /// A copy for the Web UI box (a clipboard tool, and a browser for a click), off the loop,
    /// while it is on its way.
    pub(super) web_copy: Option<tokio::sync::oneshot::Receiver<super::web_box::Copied>>,
    /// The newest copy made while one was on its way, which gets its turn at the clipboard tool
    /// when that one ends.
    pub(super) web_copy_next: Option<super::web_box::Next>,
    /// The wait for a daemon to answer on its socket again, held from the moment a daemon's
    /// connection closes until it is over, and `None` otherwise.
    pub(super) reconnect: Option<Reconnect>,
    /// What the daemon is asked again after the feed was opened afresh for the host's view (a
    /// switch of `[tui] host_audio` in Settings), while it is on its way: the loop keeps drawing
    /// and reading keys meanwhile. Ended with the connection it was asked over.
    pub(super) host_refresh: Option<events::Fetching>,
    /// The host's own audio, which the loop reaches for the whole session: a device switch
    /// hands it on to what is watched next.
    pub(super) host: HostLink,
    /// The slow work the preset list asks for, run off the loop: the AutoEq index and files,
    /// and the computer EQ.
    pub(super) jobs: jobs::Jobs,
    /// The computer's volume and EQ, which the loop reaches for the whole session.
    pub(super) computer: Box<computer::ComputerLink>,
}

impl Watching {
    /// Reach the host this session runs on, for the loop to follow from its first turn — or
    /// nothing of it at all under `SVOLDER_HOST=off`, which the app is told so its Settings row
    /// can say why.
    pub(in crate::tui) fn reach_host(&mut self) {
        let held_off = svolder_host::host_off();
        self.app.host.held_off = held_off;
        self.app.host.linked = !held_off;
        self.host = HostLink::session(held_off);
        self.jobs = jobs::Jobs::live();
        let direct = self
            .backend
            .as_ref()
            .is_some_and(|backend| backend.daemon_socket().is_none());
        *self.computer = match (held_off, direct) {
            (false, true) => ComputerLink::live(),
            _ => ComputerLink::none(),
        };
        if held_off {
            self.app.host.computer.why =
                Some("computer controls unavailable: host audio is off".to_string());
        } else if direct {
            self.app.host.computer.eq = self.computer.view().map(Box::new);
            self.app.host.computer.why = self.computer.why.clone();
        }
    }

    /// Say that the device did not come back: the app's values can no longer be updated, so the
    /// link is drawn down as well as reported, and nothing is retrying it, so the header's
    /// spinner stops.
    pub(super) fn gone(&mut self) {
        self.app.write_failed(&Failure::of(Kind::Dropped, GONE));
        self.app.link_changed(false);
        self.app.session.feed_open = false;
        self.app.host.daemon = None;
    }

    /// Start waiting for a daemon to answer on `socket` again, up to `window`: the connection to
    /// the one that was there has closed, and a restart puts another on the same path.
    ///
    /// The wait only looks at the socket and never starts a daemon, so it cannot put one of a
    /// different kind there than the one a restart is starting. The link is drawn down meanwhile,
    /// with the header's spinner turning for the wait.
    pub(super) fn wait_for_daemon(&mut self, socket: PathBuf, window: Duration) {
        self.feed_open = false;
        self.app.link_changed(false);
        // The daemon that spoke for the host is gone: the loop's own watcher takes it over until
        // a feed that is back says its host side is available.
        self.app.host.daemon = None;
        self.host_refresh = None;
        let (tx, answer) = tokio::sync::oneshot::channel();
        let looking = socket.clone();
        let probe = tokio::spawn(async move {
            let _ = tx.send(daemon_answers_within(&looking, window).await);
        });
        self.reconnect = Some(Reconnect {
            socket,
            answer,
            probe,
        });
    }

    /// Say that there is no device left to read from: the app's values can no longer be updated,
    /// so the link is drawn down as well as reported.
    ///
    /// The toast ages out after six seconds; the header does not, which stops a later frame
    /// from claiming a link this process no longer holds. Nothing is retrying the connection
    /// either, so the header's spinner stops rather than turning for a wait that isn't happening.
    pub(super) fn lost(&mut self) {
        self.app.warn(LINK_LOST);
        self.app.link_changed(false);
        self.app.session.feed_open = false;
    }

    /// Release whatever this holds as the TUI leaves, saying `svolder: releasing NAME…` on
    /// stderr if it takes more than half a second. The caller has restored the terminal, so
    /// the line lands on the user's own screen.
    pub(in crate::tui) async fn release(self) {
        if let Some(backend) = self.backend {
            backend.release(&self.app.session.device_name).await;
        }
    }

    /// Release whatever this holds at once and silently, for a case that has done with it.
    #[cfg(test)]
    pub(in crate::tui) async fn close(self) {
        if let Some(backend) = self.backend {
            backend.close().await;
        }
    }
}
