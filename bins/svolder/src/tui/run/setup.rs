// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the loop does for the PipeWire config box where no daemon speaks for the host: it reads
//! `pw-dump` (and the ALSA card's own files) itself, once when the box opens and every
//! [`READ_PERIOD`](crate::tui::boxes::setup::READ_PERIOD) while it is open, off the loop on a task
//! of its own. The read is the same read-only one the daemon makes: `pw-dump` and files, never a
//! write. With a daemon the route is the daemon's and nothing is read here.

use svolder_host::{Asound, NowPlaying, PwDump, RouteFacts, fill_alsa, route};
use tokio::sync::oneshot;

use super::Watching;

/// What one read found: the route, or `None` where there was no graph to read.
pub(super) type Read = Option<Box<RouteFacts>>;

/// What reads the graph.
#[derive(Debug, Clone)]
pub(super) struct Reader {
    /// The `pw-dump` to run.
    pub(super) dump: PwDump,
    /// The ALSA tree the card's files are read from.
    pub(super) asound: Asound,
}

impl Reader {
    /// The reader the environment names: `None` where host audio is off for this process.
    fn from_env() -> Option<Self> {
        Some(Self {
            dump: PwDump::from_env()?,
            asound: Asound::from_env(),
        })
    }

    /// The route to the default sink for the player being followed, with the ALSA hop filled.
    pub(super) async fn read(&self, player: Option<NowPlaying>) -> Read {
        let dump = self.dump.read().await?;
        let pid = player.as_ref().and_then(|player| player.pid);
        let identity = player
            .as_ref()
            .map_or("", |player| player.identity.as_str());
        let mut facts = route(&dump, pid, identity)?;
        fill_alsa(&mut facts, &self.asound);
        Some(Box::new(facts))
    }
}

/// What the loop holds for the box's own reads.
#[derive(Debug, Default)]
pub(super) struct SetupLink {
    /// What reads the graph, where a test has put its own; the environment's otherwise.
    pub(super) reader: Option<Reader>,
    /// The read in flight, which lands here.
    pub(super) pending: Option<oneshot::Receiver<Read>>,
}

/// Whether the box is open on a session with no daemon to give the route.
fn reading_here(watching: &Watching) -> bool {
    watching.app.overlays.setup.is_some() && watching.app.host.daemon.is_none()
}

/// Whether the box was just opened and has not asked for its first read.
pub(super) fn first_due(watching: &Watching) -> bool {
    watching.setup.pending.is_none()
        && reading_here(watching)
        && watching.app.setup_box().is_some_and(|setup| !setup.asked)
}

/// Whether the box is open and no read of its own is in flight, for the clock that asks again.
pub(super) fn polling(watching: &Watching) -> bool {
    watching.setup.pending.is_none() && reading_here(watching)
}

/// Start a read of the graph for the box, unless one is in flight; the answer lands on
/// [`SetupLink::pending`].
pub(super) fn read(watching: &mut Watching) {
    if watching.setup.pending.is_some() {
        return;
    }
    if let Some(setup) = watching.app.overlays.setup.as_mut() {
        setup.asked = true;
    }
    let reader = watching.setup.reader.clone().or_else(Reader::from_env);
    let Some(reader) = reader else {
        answered(watching, None);
        return;
    };
    let player = watching.app.host.playing.clone();
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        let _ = tx.send(reader.read(player).await);
    });
    watching.setup.pending = Some(rx);
}

/// Take a read in: the route replaces the box's own, and a read that found no graph keeps the
/// last route and says so.
pub(super) fn answered(watching: &mut Watching, found: Read) {
    watching.setup.pending = None;
    let Some(setup) = watching.app.overlays.setup.as_mut() else {
        return;
    };
    match found {
        Some(route) => {
            setup.own = Some(route);
            setup.failed = false;
        }
        None => setup.failed = true,
    }
}

/// Wait for the read on `rx`, while one is in flight; the `select!` arm reading this is guarded
/// to never poll it with none.
pub(super) async fn recv(rx: &mut Option<oneshot::Receiver<Read>>) -> Option<Read> {
    match rx {
        Some(receiver) => receiver.await.ok(),
        None => std::future::pending().await,
    }
}

/// Wait for the read in flight and take its answer in (tests, and nothing else, wait).
#[cfg(test)]
pub(super) async fn settle(watching: &mut Watching) {
    if let Some(rx) = watching.setup.pending.take() {
        answered(watching, rx.await.ok().flatten());
    }
}

#[cfg(test)]
mod tests;
