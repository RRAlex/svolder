// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Who uses the daemon's HTTP servers, as counts: how many connections are open and how many
//! distinct clients were seen in the past hour and day.
//!
//! One [`Activity`] belongs to each server's slot ([`crate::http_slot::Slot`]), so it outlives a
//! reload or a restart of the server and ends with the daemon. It lives in memory only and says
//! nothing but counts: a client is held as a 64-bit hash of its canonical address under keys
//! drawn at random when the daemon starts, so no address is stored, logged, shown or recoverable,
//! and no value here prints one.
//!
//! Only a connection that passed the server's check counts, open or seen. The accept loop gives
//! each connection a [`Counted`] handle in its requests' extensions ([`crate::http`]); the
//! server calls [`Counted::pass`] on the first request its token check lets through (a server
//! with no token passes every request), and the connection stays counted until it closes. A
//! connection that sends only wrong tokens is never counted.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::net::IpAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use svolder_ipc::{DAY_SECS, ServerActivity};
use tokio::sync::watch;
use tokio::time::Instant;

use crate::locks::locked;

/// The most clients the table holds; one more drops the oldest that has no connection open.
pub const MAX_CLIENTS: usize = 4096;

const HOUR: Duration = Duration::from_secs(3_600);
const DAY: Duration = Duration::from_secs(DAY_SECS);

/// One server's counts, shared by the accept loop and the slot that reads them.
#[derive(Clone, Default)]
pub struct Activity(Arc<Shared>);

struct Shared {
    /// Hashes an address to the id the table keys on; keys random per process.
    hasher: RandomState,
    started: Instant,
    table: Mutex<Table>,
    /// Bumped when a connection is counted or closes, for a test to await instead of sleeping.
    turned: watch::Sender<u64>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            hasher: RandomState::new(),
            started: Instant::now(),
            table: Mutex::new(Table::default()),
            turned: watch::channel(0).0,
        }
    }
}

#[derive(Default)]
struct Table {
    clients: HashMap<u64, Client>,
    /// When the table last dropped a client to make room, while that is within a day.
    capped_at: Option<Instant>,
}

struct Client {
    /// The last time the client was seen: when it passed, or when its last connection closed.
    last: Instant,
    /// Its connections open now.
    open: u32,
}

impl Activity {
    /// A connection from `ip` was accepted. It counts nothing until [`Counted::pass`]; `ip` is
    /// reduced to its hash here and kept nowhere.
    pub fn accepted(&self, ip: IpAddr) -> Counted {
        Counted(Arc::new(Connection {
            activity: self.clone(),
            id: self.0.hasher.hash_one(ip.to_canonical()),
            open: OnceLock::new(),
        }))
    }

    /// Wait until `done` holds of the counts, as they change.
    #[cfg(test)]
    pub async fn until(&self, done: impl Fn(&ServerActivity) -> bool) {
        let mut turns = self.0.turned.subscribe();
        while !done(&self.snapshot()) {
            turns.changed().await.expect("the activity is alive");
        }
    }

    /// The counts now.
    pub fn snapshot(&self) -> ServerActivity {
        let now = Instant::now();
        let mut table = locked(&self.0.table);
        table.prune(now);
        let mut open = 0u32;
        let (mut hour, mut day) = (0u32, 0u32);
        for client in table.clients.values() {
            open += client.open;
            day += 1;
            if client.open > 0 || now.duration_since(client.last) < HOUR {
                hour += 1;
            }
        }
        ServerActivity {
            open,
            hour,
            day,
            counted_secs: now.duration_since(self.0.started).as_secs().min(DAY_SECS),
            capped: table.capped_at.is_some(),
        }
    }
}

impl std::fmt::Debug for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Activity")
    }
}

impl Table {
    /// Forget what is a day old and has no connection open.
    fn prune(&mut self, now: Instant) {
        self.clients
            .retain(|_, client| client.open > 0 || now.duration_since(client.last) < DAY);
        if self
            .capped_at
            .is_some_and(|at| now.duration_since(at) >= DAY)
        {
            self.capped_at = None;
        }
    }

    /// A connection of client `id` passed.
    fn opened(&mut self, id: u64, now: Instant) {
        self.prune(now);
        if !self.clients.contains_key(&id) && self.clients.len() >= MAX_CLIENTS {
            let oldest = self
                .clients
                .iter()
                .filter(|(_, client)| client.open == 0)
                .min_by_key(|(_, client)| client.last)
                .map(|(id, _)| *id);
            if let Some(oldest) = oldest {
                self.clients.remove(&oldest);
                self.capped_at = Some(now);
            }
        }
        let client = self
            .clients
            .entry(id)
            .or_insert(Client { last: now, open: 0 });
        client.last = now;
        client.open += 1;
    }

    /// A connection of client `id` closed.
    fn closed(&mut self, id: u64, now: Instant) {
        if let Some(client) = self.clients.get_mut(&id) {
            client.open = client.open.saturating_sub(1);
            client.last = now;
        }
    }
}

/// One accepted connection's handle: cheap to clone, and the connection counts as open from its
/// first [`Counted::pass`] until the last clone is dropped (the connection's task ends, or is
/// aborted with the server).
#[derive(Clone)]
pub struct Counted(Arc<Connection>);

struct Connection {
    activity: Activity,
    id: u64,
    /// Set by the first pass; its drop closes the connection's count.
    open: OnceLock<Open>,
}

struct Open;

impl Counted {
    /// The connection passed the server's check: count it, once, open and seen.
    pub fn pass(&self) {
        let connection = &self.0;
        connection.open.get_or_init(|| {
            let mut table = locked(&connection.activity.0.table);
            table.opened(connection.id, Instant::now());
            connection.activity.0.turned.send_modify(|turn| *turn += 1);
            Open
        });
    }

    /// Whether the connection has passed.
    pub fn passed(&self) -> bool {
        self.0.open.get().is_some()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if self.open.get().is_some() {
            locked(&self.activity.0.table).closed(self.id, Instant::now());
            self.activity.0.turned.send_modify(|turn| *turn += 1);
        }
    }
}

impl std::fmt::Debug for Counted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Counted")
            .field("passed", &self.passed())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
