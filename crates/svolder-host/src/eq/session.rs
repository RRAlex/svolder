// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one `pw-cli` that sets the live chain's parameters while it plays.
//!
//! A drag sends many updates a second, so a process per update is out of the question: one
//! `pw-cli` is started on the first send and kept, fed `set-param` lines on a terminal of its
//! own (see `pty`: on a pipe it would hold its output back). It is started again when it has
//! exited, at most once a second. It is given nothing but `set-param` lines for the live chain's
//! node (and `quit` when it is closed); its output is read only for a refusal and for the word
//! that it has connected.
//!
//! The node's id is found through `pw-dump` and kept until [`Params::forget_node`], a write
//! `pw-cli` cannot take, or a `set-param` it refuses. Its output is read for that last one: a
//! restart of the chain gives the node a new id, and `pw-cli` answers a line for the old one
//! with `Error: "set-param: unknown global '33'"`. The session then finds the node again and
//! sends the newest value of every port it has been given, since what was refused, and what
//! sat in the pipe when a `pw-cli` died, is not known to have landed. [`Params::settle`] does
//! that without a new value to send. Whoever polls the chain's rate still forgets the node and
//! reads it again: that read is also the rate.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::net::unix::pipe::Sender;
use tokio::process::{Child, Command};
use tokio::sync::watch;

use super::live::{LiveNode, preamp_params, props_of, set_param_line};
use super::paths::{is_executable, on_path};
use crate::route::PwDump;

/// The environment variable naming the `pw-cli` to run; `pw-cli` on `PATH` when unset or empty.
pub const PW_CLI_ENV: &str = "SVOLDER_PW_CLI";

/// The shortest time between two starts of `pw-cli`.
const RESTART_GAP: Duration = Duration::from_secs(1);

/// How long one line gets to be written before the `pw-cli` that did not take it is given up on.
const WRITE_WITHIN: Duration = Duration::from_secs(1);

/// How long a fresh `pw-cli` gets to say it is connected before it is given a line anyway: a
/// line given before that is refused as an unknown global.
const READY_WITHIN: Duration = Duration::from_millis(500);

/// How long `pw-cli` gets to end after `quit` before it is killed.
const QUIT_WITHIN: Duration = Duration::from_secs(1);

/// Where the live chain's parameters are sent.
#[async_trait]
pub trait Params: Send + Sync {
    /// The live chain's node, kept from the last time it was found: read through `pw-dump`
    /// when none is kept. `None` when the chain is not in the graph.
    async fn node(&mut self) -> Option<LiveNode>;

    /// Send `params` to the node `node`, in one call: the one the caller has looked at, and
    /// never another. `false` when they did not go: no node, no `pw-cli`, one that could not be
    /// started or fed yet, or a node found under another id than `node` (the chain was replaced
    /// since the caller looked). The caller keeps what it meant to send, looks again and sends
    /// the newest value.
    async fn send(&mut self, node: u32, params: &[(String, f64)]) -> bool;

    /// The rate the default output runs at, for an apply while no chain is there to show it:
    /// `None` when it cannot be read.
    async fn sink_rate(&mut self) -> Option<u32> {
        None
    }

    /// Drop the kept node, so the next [`Params::node`] and send find it again.
    fn forget_node(&mut self);

    /// A new chain is in place (an apply, or a restart seen): the values sent to the old one
    /// are not on it, so a repair line must not carry them. Drops the kept node too.
    fn new_chain(&mut self) {
        self.forget_node();
    }

    /// Put right what `pw-cli` is known to have refused since the last call: find the node
    /// again and send the newest value of every port. `true` when nothing was refused or the
    /// repair went through. A caller with nothing to send calls it on its tick, with the id of
    /// the node it has looked at (`None` when it has looked at none: nothing is sent then).
    async fn settle(&mut self, _node: Option<u32>) -> bool {
        true
    }
}

/// What the readers of a `pw-cli`'s output have seen.
struct Seen {
    /// A `set-param` was refused.
    refused: AtomicBool,
    /// It said it is connected.
    ready: watch::Sender<bool>,
}

impl Seen {
    /// Take note of one line of `pw-cli`'s output.
    fn line(&self, line: &[u8]) {
        let line = String::from_utf8_lossy(line);
        if line.contains("Error: \"set-param") {
            self.refused.store(true, Ordering::SeqCst);
        } else if line.contains(" is named ") {
            self.ready.send_replace(true);
        }
    }
}

/// Read `stream` to its end, so the pipe never fills.
fn watch_output(stream: impl AsyncRead + Unpin + Send + 'static, seen: Arc<Seen>) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stream);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line).await {
                Ok(0) | Err(_) => return,
                Ok(_) => seen.line(&line),
            }
        }
    });
}

/// A running `pw-cli`.
struct Running {
    child: Child,
    stdin: Sender,
    seen: Arc<Seen>,
}

/// The real [`Params`]: one held-open `pw-cli`, and `pw-dump` to find the node.
pub struct PwCliSession {
    program: OsString,
    dump: PwDump,
    restart_gap: Duration,
    running: Option<Running>,
    started: Option<Instant>,
    node: Option<LiveNode>,
    /// The last rate a node was read with: a sink that is suspended shows none, which is not a
    /// change of rate.
    rate: Option<u32>,
    outbox: Outbox,
}

/// What the session owes the chain: the newest value given for each port, and whether the next
/// line carries them all, not only what is sent now.
#[derive(Debug, Default)]
pub(super) struct Outbox {
    newest: BTreeMap<String, f64>,
    full: bool,
}

impl Outbox {
    /// Note `params` as the newest values.
    pub(super) fn record(&mut self, params: &[(String, f64)]) {
        for (name, value) in params {
            self.newest.insert(name.clone(), *value);
        }
    }

    /// The next line carries every port.
    pub(super) fn mark_full(&mut self) {
        self.full = true;
    }

    /// A line went out.
    pub(super) fn delivered(&mut self) {
        self.full = false;
    }

    /// Whether there is something to repair: a full line is due and ports are held.
    pub(super) fn repair_due(&self) -> bool {
        self.full && !self.newest.is_empty()
    }

    /// A new chain is in place: what was sent to the old one is not on it, and nothing is owed.
    pub(super) fn new_chain(&mut self) {
        self.newest.clear();
        self.full = false;
    }

    /// The parameters of the next line for a caller giving `params`.
    pub(super) fn line(&self, params: &[(String, f64)]) -> Vec<(String, f64)> {
        if !self.full {
            return params.to_vec();
        }
        // The caller's ports in their order and every other port held after them; when the
        // caller ends with the preamp (it rises), the others go first, so none plays under a
        // preamp that has risen.
        let mut all: Vec<(String, f64)> = params
            .iter()
            .filter_map(|(name, _)| Some((name.clone(), *self.newest.get(name)?)))
            .collect();
        let rest = self
            .newest
            .iter()
            .filter(|(name, _)| !params.iter().any(|(given, _)| given == *name))
            .map(|(name, value)| (name.clone(), *value));
        let preamp = preamp_params(0.0);
        let rises = params
            .last()
            .is_some_and(|(name, _)| preamp.iter().any(|(held, _)| held == name));
        match rises {
            true => all.splice(0..0, rest).for_each(drop),
            false => all.extend(rest),
        }
        all
    }
}

impl PwCliSession {
    /// A session through the `pw-cli` at `program`, finding the node through `dump`. Nothing
    /// is started until the first send.
    pub fn new(program: impl Into<OsString>, dump: PwDump) -> Self {
        Self {
            program: program.into(),
            dump,
            restart_gap: RESTART_GAP,
            running: None,
            started: None,
            node: None,
            rate: None,
            outbox: Outbox::default(),
        }
    }

    /// The session [`PW_CLI_ENV`] and [`PwDump::from_env`] say; `None` when host audio is off
    /// or there is no `pw-cli` to run.
    pub fn from_env() -> Option<Self> {
        Self::resolve(
            crate::host_off(),
            std::env::var_os(PW_CLI_ENV).as_deref(),
            PwDump::from_env()?,
        )
    }

    /// The session with `named` (the value of [`PW_CLI_ENV`]) as its `pw-cli`, else the one on
    /// `PATH`: `None` when `off`, or when there is no such executable.
    pub fn resolve(off: bool, named: Option<&OsStr>, dump: PwDump) -> Option<Self> {
        if off {
            return None;
        }
        let program = match named.filter(|named| !named.is_empty()) {
            Some(named) => {
                let path = std::path::Path::new(named);
                // A name with no directory is looked up on `PATH`, as a shell would.
                match path.components().count() > 1 {
                    true => is_executable(path).then(|| path.to_path_buf())?,
                    false => on_path(&named.to_string_lossy())?,
                }
            }
            None => on_path("pw-cli")?,
        };
        Some(Self::new(program, dump))
    }

    /// The same session with at least `gap` between two starts of `pw-cli`.
    pub fn with_restart_gap(mut self, gap: Duration) -> Self {
        self.restart_gap = gap;
        self
    }

    /// Whether a `pw-cli` is there to be fed: the one running, or one started now when none
    /// is, or has exited, and the gap since the last start has passed.
    fn ensure_running(&mut self) -> bool {
        if let Some(running) = self.running.as_mut() {
            match running.child.try_wait() {
                Ok(None) => return true,
                _ => self.running = None,
            }
        }
        if self
            .started
            .is_some_and(|started| started.elapsed() < self.restart_gap)
        {
            return false;
        }
        self.started = Some(Instant::now());
        // `pw-cli` is given a terminal, not pipes: on a pipe it holds its output back in blocks,
        // and its refusals would come long after the lines they answer.
        let Ok(pty) = super::pty::open() else {
            return false;
        };
        let Ok(streams) = (0..3)
            .map(|_| pty.slave.try_clone().map(std::process::Stdio::from))
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        let mut streams = streams.into_iter();
        let child = Command::new(&self.program)
            .stdin(streams.next().expect("three streams"))
            .stdout(streams.next().expect("three streams"))
            .stderr(streams.next().expect("three streams"))
            .kill_on_drop(true)
            .spawn();
        let Ok(child) = child else {
            return false;
        };
        let seen = Arc::new(Seen {
            refused: AtomicBool::new(false),
            ready: watch::channel(false).0,
        });
        watch_output(pty.from_child, seen.clone());
        self.running = Some(Running {
            child,
            stdin: pty.to_child,
            seen,
        });
        // What a `pw-cli` that ended may have left unread in its pipe is sent again.
        self.outbox.mark_full();
        true
    }

    /// Write one line to the running `pw-cli`; a line it cannot take ends it.
    async fn write(&mut self, line: &str) -> bool {
        let Some(running) = self.running.as_mut() else {
            return false;
        };
        let text = format!("{line}\n");
        let written = tokio::time::timeout(WRITE_WITHIN, async {
            running.stdin.write_all(text.as_bytes()).await?;
            running.stdin.flush().await
        })
        .await;
        match written {
            Ok(Ok(())) => true,
            _ => {
                self.running = None;
                self.node = None;
                false
            }
        }
    }

    /// Tell `pw-cli` to quit and wait for it to end, killing it if it does not.
    pub async fn close(&mut self) {
        if self.running.is_none() {
            return;
        }
        self.write("quit").await;
        if let Some(mut running) = self.running.take() {
            drop(running.stdin);
            if tokio::time::timeout(QUIT_WITHIN, running.child.wait())
                .await
                .is_err()
            {
                let _ = running.child.kill().await;
            }
        }
    }
}

impl PwCliSession {
    /// Whether a refusal from the running `pw-cli` has been read and not yet acted on. The
    /// session learns of a refusal from the output's reader, a moment after `pw-cli` wrote it, so
    /// a caller that must act after the refusal has arrived (a test) waits on this.
    #[doc(hidden)]
    pub fn refusal_read(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|running| running.seen.refused.load(Ordering::SeqCst))
    }

    /// Whether the running `pw-cli` refused a `set-param` since this was last asked; the kept
    /// node is then dropped and the next line carries every port.
    fn take_refusal(&mut self) -> bool {
        let refused = self
            .running
            .as_ref()
            .is_some_and(|running| running.seen.refused.swap(false, Ordering::SeqCst));
        if refused {
            self.node = None;
            self.outbox.mark_full();
        }
        refused
    }

    /// Wait for a `pw-cli` just started to say it is connected, for at most [`READY_WITHIN`].
    async fn wait_ready(&mut self) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let mut ready = running.seen.ready.subscribe();
        let _ = tokio::time::timeout(READY_WITHIN, ready.wait_for(|ready| *ready)).await;
    }

    /// Give `params` (or every port, after a restart or a refusal) to `pw-cli`.
    async fn deliver(&mut self, id: u32, params: &[(String, f64)]) -> bool {
        self.outbox.record(params);
        // A line that fails is tried once more, after the node is found again and `pw-cli` is
        // started if it is gone.
        for _ in 0..2 {
            self.take_refusal();
            let Some(node) = self.node().await else {
                return false;
            };
            if node.id != id {
                self.outbox.mark_full();
                return false;
            }
            let started = self.started;
            if !self.ensure_running() {
                return false;
            }
            if self.started != started {
                self.wait_ready().await;
            }
            let line = set_param_line(node.id, &self.outbox.line(params));
            if self.write(&line).await {
                self.outbox.delivered();
                return true;
            }
            self.outbox.mark_full();
        }
        false
    }
}

#[async_trait]
impl Params for PwCliSession {
    async fn node(&mut self) -> Option<LiveNode> {
        self.take_refusal();
        if self.node.is_none() {
            let mut node = props_of(&self.dump.read().await?)?;
            match node.rate_hz {
                Some(rate) => self.rate = Some(rate),
                None => node.rate_hz = self.rate,
            }
            self.node = Some(node);
        }
        self.node.clone()
    }

    async fn sink_rate(&mut self) -> Option<u32> {
        super::live::default_sink_rate(&self.dump.read().await?)
    }

    async fn send(&mut self, node: u32, params: &[(String, f64)]) -> bool {
        let sent = self.deliver(node, params).await;
        if !sent {
            self.outbox.mark_full();
        }
        sent
    }

    fn forget_node(&mut self) {
        self.node = None;
    }

    fn new_chain(&mut self) {
        self.node = None;
        self.outbox.new_chain();
    }

    async fn settle(&mut self, node: Option<u32>) -> bool {
        self.take_refusal();
        match (self.outbox.repair_due(), node) {
            (false, _) => true,
            (true, None) => false,
            (true, Some(node)) => self.send(node, &[]).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // What the real pw-cli (1.6.2) printed, taken from a run on a private instance.
    const UNKNOWN_GLOBAL: &str = "Error: \"set-param: unknown global '33'\"\n";
    const NOT_IMPLEMENTED: &str =
        "Error: \"set-param not implemented on object 36 type:PipeWire:Interface:Port\"\n";
    const HARMLESS: &str = "Error: \"unsupported type PipeWire:Interface:SecurityContext\"\n";
    const CONNECTED: &str = ">> remote 0 is named 'svolder-e2e-0'\n";

    fn seen() -> Seen {
        Seen {
            refused: AtomicBool::new(false),
            ready: watch::channel(false).0,
        }
    }

    #[test]
    fn both_real_refusals_are_read_and_the_harmless_error_is_not() {
        for line in [UNKNOWN_GLOBAL, NOT_IMPLEMENTED] {
            let seen = seen();
            seen.line(line.as_bytes());
            assert!(seen.refused.load(Ordering::SeqCst), "{line}");
        }
        let seen = seen();
        seen.line(HARMLESS.as_bytes());
        seen.line(CONNECTED.as_bytes());
        assert!(!seen.refused.load(Ordering::SeqCst));
        assert!(*seen.ready.borrow());
    }
}
