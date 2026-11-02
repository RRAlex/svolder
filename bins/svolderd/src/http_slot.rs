// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's slot for one of its two HTTP servers, the MCP server or the web UI: the one
//! place the steps that bring a server in line with its config section are written, so a fix to
//! one of them reaches both.
//!
//! A slot holds at most one server and whether the daemon's own stop has begun, after which no
//! server is started at all. Bringing it in line is two steps: [`Slot::release`] keeps a server
//! that already serves the section (re-reading its token) or stops the one that does not, and
//! says whether a new one is wanted; [`Slot::start`] then starts it. The daemon runs the first
//! step of every slot before the second step of any on a reload ([`crate::handler::Daemon`]'s
//! `servers_to`), so servers that swap ports find them free; at startup, and for one slot alone,
//! the steps run one slot at a time.

use std::pin::Pin;
use std::sync::Mutex;

use crate::locks::locked;
use crate::log;

/// What a slot needs of a server it holds.
pub trait Server: Sized + Send + 'static {
    /// The config section the server is started from.
    type Config: PartialEq;
    /// The server's state as `version` reports it.
    type Status;

    /// The name that heads the server's log lines.
    const NAME: &'static str;

    /// The status of a slot that holds no server.
    fn off() -> Self::Status;
    /// The section this server was started from.
    fn config(&self) -> &Self::Config;
    /// Whether the server is serving, rather than failed.
    fn serving(&self) -> bool;
    /// The server's state as `version` reports it.
    fn status(&self) -> Self::Status;
    /// Where the token the server compares against comes from, and its tail.
    fn token_info(&self) -> Option<svolder_ipc::TokenInfo>;
    /// What reads the token again, for a server that has one to replace.
    fn refresher(&self) -> Option<Refresher>;
    /// Stop serving and close the port.
    fn stop(self) -> impl Future<Output = ()> + Send;
}

/// A running server's token, to be read again without holding the daemon's lock.
pub struct Refresher(Pin<Box<dyn Future<Output = Result<(), String>> + Send>>);

impl Refresher {
    /// A refresher that runs `work`: read the token and hand it to the server.
    pub fn new(work: impl Future<Output = Result<(), String>> + Send + 'static) -> Refresher {
        Refresher(Box::pin(work))
    }

    /// Read the token again and accept it from now on, or say why it cannot be had; the caller
    /// then stops the server, since one that kept the old token would be serving on a secret
    /// the operator has replaced.
    pub async fn refresh(self) -> Result<(), String> {
        self.0.await
    }
}

/// The server a daemon runs, and whether its own stop has begun.
pub struct Slot<S> {
    inner: Mutex<Inner<S>>,
}

struct Inner<S> {
    running: Option<S>,
    stopping: bool,
}

impl<S> Default for Slot<S> {
    fn default() -> Self {
        Slot {
            inner: Mutex::new(Inner {
                running: None,
                stopping: false,
            }),
        }
    }
}

impl<S: Server> Slot<S> {
    /// Make the slot free for the server `config` describes: one already serving that section,
    /// whatever its auth, is kept, and one with a token re-reads it (a rotated one takes effect
    /// at once); a failed one, one on a changed section and one that `enabled` turns off are
    /// stopped, their ports closed. Answers whether a new server is to be started with
    /// [`Slot::start`]: not when one was kept, when `enabled` is off, or when the daemon's stop
    /// has begun.
    pub async fn release(&self, config: &S::Config, enabled: bool) -> bool {
        let (kept, refresher) = {
            let slot = locked(&self.inner);
            if slot.stopping {
                return false;
            }
            let same = slot
                .running
                .as_ref()
                .filter(|running| running.serving() && running.config() == config);
            (same.is_some(), same.and_then(S::refresher))
        };
        if kept {
            let Some(refresher) = refresher else {
                return false;
            };
            match refresher.refresh().await {
                Ok(()) => return false,
                Err(message) => log::error(&format!("{}: {message}; stopping the server", S::NAME)),
            }
        }
        let replaced = {
            let mut slot = locked(&self.inner);
            if slot.stopping {
                return false;
            }
            slot.running.take()
        };
        if let Some(old) = replaced {
            old.stop().await;
        }
        enabled
    }

    /// Start the server `make` builds and keep it, unless the daemon's stop began meanwhile, in
    /// which case it is stopped again. A server still in the slot is stopped, so the slot never
    /// loses one it has not closed. `make` runs outside the lock: reading the token can be
    /// slow, and `version` must not wait. It answers nothing when no server can be built.
    pub async fn start<F>(&self, make: impl FnOnce() -> F)
    where
        F: Future<Output = Option<S>>,
    {
        let Some(started) = make().await else {
            return;
        };
        let to_stop = {
            let mut slot = locked(&self.inner);
            match slot.stopping {
                true => Some(started),
                false => slot.running.replace(started),
            }
        };
        if let Some(server) = to_stop {
            server.stop().await;
        }
    }

    /// Stop the server, if one runs, and close its port. Marks the stop first, so a reload
    /// running at the same time starts no server after it.
    pub async fn stop(&self) {
        let running = {
            let mut slot = locked(&self.inner);
            slot.stopping = true;
            slot.running.take()
        };
        if let Some(server) = running {
            server.stop().await;
        }
    }

    /// Where the server stands: off while the slot holds none.
    pub fn status(&self) -> S::Status {
        match &locked(&self.inner).running {
            Some(server) => server.status(),
            None => S::off(),
        }
    }

    /// Where the running server's token comes from, for the `version` reply alone.
    pub fn token_info(&self) -> Option<svolder_ipc::TokenInfo> {
        locked(&self.inner).running.as_ref().and_then(S::token_info)
    }

    /// `read` over the running server, or over none, with the slot locked.
    pub fn with<R>(&self, read: impl FnOnce(Option<&S>) -> R) -> R {
        read(locked(&self.inner).running.as_ref())
    }
}

#[cfg(test)]
mod tests;
