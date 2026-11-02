// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's analysed frames of the host's capture, as a joined loop reads them: a connection
//! of their own, opened while the daemon speaks for the host and something drawn wants a capture,
//! and dropped the moment neither holds.
//!
//! While the connection is open or opening the loop's own `pw-record` keeps running, so the
//! Spectrum row, the level and the beats have data however slow the daemon is to answer; the
//! daemon's first frame lands, and the own capture stops: the daemon's capture feeds them from then
//! on, and the host is recorded once. Both captures are passive (`node.rate = 0`), so the short
//! overlap holds nothing of the graph. A daemon that cannot give frames (one from before them,
//! which refuses the request, or one with no host audio) is left alone until another daemon speaks
//! for the host, and the loop's own capture feeds the row meanwhile; a daemon whose capture cannot
//! run (`Unavailable`) or one that does not answer within [`OPEN_WITHIN`] is asked again after
//! [`FRAMES_RETRY`], a connection that broke after [`CAPTURE_RETRY`], and a bar set for a time
//! keeps until then however briefly the frames are not wanted. Frames land as a block of the loop's
//! own capture does, through [`Frame::block`](svolder_host::Frame::block), and a beat in one pulses
//! what is drawn.

use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_ipc::host::CaptureKind;
use svolder_ipc::{Event, IpcError};
use tokio::time::Instant;

use super::{Aborting, CAPTURE_RETRY, HostLink};
use crate::backend::Backend;
use crate::tui::app::App;

/// How long a daemon whose capture said it cannot run is left before the frames are asked for
/// again.
pub(super) const FRAMES_RETRY: Duration = Duration::from_secs(30);

/// How long the daemon has to answer a request for its frames: a local socket answers in
/// milliseconds, and the loop's own capture keeps feeding the row while the answer is waited
/// for, so a slow daemon costs nothing but this long a connection that is not yet open.
const OPEN_WITHIN: Duration = Duration::from_secs(3);

/// What opening the frames came to.
pub(in crate::tui::run) enum Opened {
    /// The connection, its first event the capture's state.
    Stream(BoxStream<'static, Event>),
    /// The daemon answered that it has no frames to give.
    Refused,
    /// The connection could not be made.
    Failed,
    /// The connection was made and no answer came in time: asked again as rarely as a capture
    /// that cannot run.
    Mute,
}

/// How the frames connection stands.
#[derive(Default)]
pub(in crate::tui::run) enum FramesLink {
    /// No connection, none being made.
    #[default]
    Off,
    /// The request is on its way.
    Opening(Aborting<Opened>),
    /// The connection is open and its events are read.
    Open(BoxStream<'static, Event>),
}

impl FramesLink {
    /// Whether the connection is open or being opened.
    pub(in crate::tui::run) fn active(&self) -> bool {
        !matches!(self, FramesLink::Off)
    }
}

/// Why the frames are not asked for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::tui::run) enum Barred {
    /// They may be asked for.
    #[default]
    No,
    /// The daemon cannot give them: until another daemon speaks for the host.
    Daemon,
    /// Until this time.
    Until(Instant),
}

/// What the next turn of the frames connection brings.
pub(in crate::tui::run) enum Step {
    /// The request for the connection answered.
    Opened(Opened),
    /// The next event of the open connection, `None` once it has ended.
    Event(Option<Box<Event>>),
}

/// The next thing the frames connection brings, or never while there is none.
pub(in crate::tui::run) async fn next_step(frames: &mut FramesLink) -> Step {
    match frames {
        FramesLink::Off => std::future::pending().await,
        FramesLink::Opening(opening) => {
            Step::Opened((&mut opening.0).await.unwrap_or(Opened::Failed))
        }
        FramesLink::Open(events) => Step::Event(events.next().await.map(Box::new)),
    }
}

/// What a request for the frames came to.
async fn open(
    asked: impl std::future::Future<Output = Result<BoxStream<'static, Event>, IpcError>>,
) -> Opened {
    match tokio::time::timeout(OPEN_WITHIN, asked).await {
        Ok(Ok(events)) => Opened::Stream(events),
        Ok(Err(IpcError::Protocol(_))) => Opened::Refused,
        Ok(Err(_)) => Opened::Failed,
        Err(_) => Opened::Mute,
    }
}

impl HostLink {
    /// Open the daemon's frames connection while the daemon speaks for the host and a capture is
    /// wanted ([`App::capture_target`]), and drop it when either stops holding; called after
    /// every frame, before [`HostLink::steer`], which keeps the loop's own capture running until
    /// the daemon's first frame has landed.
    pub(in crate::tui::run) fn steer_frames(&mut self, app: &mut App, backend: Option<&Backend>) {
        let wanted = backend.is_some_and(Backend::through_daemon)
            && app.host.daemon_owns()
            && app.capture_target().is_some();
        if !wanted {
            // a bar set for a time keeps until then, however briefly the frames are not wanted
            self.drop_frames(app);
            return;
        }
        if self.frames.active() {
            return;
        }
        let barred = match self.barred {
            Barred::No => false,
            Barred::Daemon => true,
            Barred::Until(until) => Instant::now() < until,
        };
        if barred {
            return;
        }
        let Some(asked) = backend.and_then(Backend::host_frames) else {
            return;
        };
        self.barred = Barred::No;
        self.frames = FramesLink::Opening(Aborting(tokio::spawn(open(asked))));
    }

    /// Let go of the frames connection, if there is one: the daemon's capture is no longer held
    /// by this loop, and nothing it read is left to draw.
    pub(in crate::tui::run) fn drop_frames(&mut self, app: &mut App) {
        // What the row holds is the own capture's, which keeps running, until a frame landed.
        let held = self.fed || self.capturing.is_none();
        self.fed = false;
        if std::mem::take(&mut self.frames).active() && held {
            app.host.capture_stopped();
        }
    }

    /// Whether the daemon's frames feed the row, which is when the loop's own capture is not the
    /// one that does: the connection is open and its first frame has landed.
    pub(in crate::tui::run) fn daemon_feeds(&self) -> bool {
        self.fed && matches!(self.frames, FramesLink::Open(_))
    }

    /// Whether the loop waits on the frames connection.
    pub(in crate::tui::run) fn listening(&self) -> bool {
        self.frames.active()
    }

    /// Take what the frames connection brought, and say whether it was a frame landed, which
    /// asks for no frame of its own to draw.
    pub(in crate::tui::run) fn framed(&mut self, step: Step, app: &mut App) -> bool {
        match step {
            Step::Opened(Opened::Stream(events)) => {
                self.frames = FramesLink::Open(events);
            }
            Step::Opened(Opened::Refused) => {
                self.frames = FramesLink::Off;
                self.barred = Barred::Daemon;
            }
            Step::Opened(Opened::Failed) => {
                self.frames = FramesLink::Off;
                self.barred = Barred::Until(Instant::now() + CAPTURE_RETRY);
            }
            Step::Opened(Opened::Mute) => {
                self.frames = FramesLink::Off;
                self.barred = Barred::Until(Instant::now() + FRAMES_RETRY);
            }
            Step::Event(Some(event)) => match *event {
                Event::HostCapture(view) => match view.state {
                    CaptureKind::Running => app.host.capturing = true,
                    CaptureKind::Starting => {}
                    CaptureKind::Unavailable => {
                        self.drop_frames(app);
                        self.barred = Barred::Until(Instant::now() + FRAMES_RETRY);
                    }
                },
                Event::HostFrame(view) => {
                    let Some(frame) = view.to_frame() else {
                        return false;
                    };
                    app.host.capturing = true;
                    app.heard_frame(&frame, std::time::Instant::now());
                    // The first frame hands the row over: what the own capture read stays
                    // drawn, as the daemon's frames now replace it.
                    self.fed = true;
                    self.stop_own_capture();
                    return true;
                }
                _ => {}
            },
            Step::Event(None) => {
                self.drop_frames(app);
                self.barred = Barred::Until(Instant::now() + CAPTURE_RETRY);
            }
        }
        false
    }
}
