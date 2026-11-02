// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where a device verb runs: a daemon that already holds the device, or a session this
//! process opens itself.
//!
//! Both backends answer in the same currency, a [`ValueObject`] per reading, so a verb reads
//! and prints the same way whichever one it got. [`Backend`] is the dispatch between them;
//! the two halves live in [`direct`] and [`remote`].
//!
//! - [`direct`]: the half that owns a session of this process.
//! - [`launch`]: how a front end reaches a daemon, and how one is started when nothing answers.
//! - [`remote`]: the half that talks to a running daemon.
//! - [`stale`]: a daemon that holds other devices than the configuration file names.

mod direct;
pub mod launch;
mod stale;
pub use stale::{reload_if_reads, reload_if_stale};
#[cfg(not(test))]
mod remote;
/// The daemon half, whose own cases serve a stand-in daemon that the TUI's cases reach for
/// too: one stub, over the very server, client and wire format the daemon speaks.
#[cfg(test)]
pub(crate) mod remote;

use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use svolder_ble::{BleError, Transport};
use svolder_ipc::ValueObject;
use svolder_ipc::{Config, DeviceConfig, Request, Response, ScanEntry};
use svolder_profiles::units;
use svolder_profiles::{Control, ControlKind, Profile};
use svolder_proto::FunctionId;

use direct::DirectBackend;
/// The transports a direct session can be opened on, and how one is opened: what grades an
/// adapter needs, since a grade is one session per adapter rather than one per verb.
pub use direct::{Wire, open_transport};
pub(crate) use direct::{choose_configured, transport_for};
use remote::RemoteBackend;
pub use svolder_ipc::socket::daemon_version::DaemonInfo;

/// How long a daemon is given to answer a question a front end asks before it has anything
/// on screen or while it reads no keys: the stale-daemon check as a whole, a device list the
/// landing holds, and where a daemon logs.
#[cfg(not(test))]
pub(crate) const ASK_WAIT: Duration = Duration::from_secs(2);
/// Short, so a case that stalls the daemon does not wait out the real one.
#[cfg(test)]
pub(crate) const ASK_WAIT: Duration = Duration::from_millis(300);

/// Why something a backend was asked for did not happen, in the words and the code it earns.
pub use crate::recovery::Failure;
use crate::recovery::{Kind, next_step};

/// The transport double a test severs and reopens a fake device's link through.
#[cfg(test)]
pub(crate) use direct::test_support::FakeGate;

/// Everything a device verb needs to know beyond its own arguments.
pub struct Globals<'a> {
    /// The transport named by `--transport`.
    pub transport: &'a str,
    /// The adapter named by `--adapter`, for a direct `ble` transport.
    pub adapter: Option<&'a str>,
    /// The socket named by `--socket`, ahead of every other source.
    pub socket: Option<&'a str>,
    /// The device named by `-d`.
    pub device: Option<&'a str>,
    /// Whether `-v` was given.
    pub verbose: bool,
}

impl Globals<'_> {
    /// These same arguments, owning their strings.
    pub fn owned(&self) -> OwnedGlobals {
        OwnedGlobals {
            transport: self.transport.to_string(),
            adapter: self.adapter.map(str::to_string),
            socket: self.socket.map(str::to_string),
            device: self.device.map(str::to_string),
            verbose: self.verbose,
        }
    }
}

/// [`Globals`] that own their strings, for a front end that opens a backend more than once.
///
/// A verb borrows its arguments straight from the command line and is done with them when it
/// exits. The TUI outlives that: switching device opens a backend again, under arguments it
/// has changed, long after the ones it started with would have done. So it keeps a copy of
/// its own and lends a [`Globals`] out of it for each open.
#[derive(Clone)]
pub struct OwnedGlobals {
    /// The transport named by `--transport`.
    pub transport: String,
    /// The adapter named by `--adapter`, for a direct `ble` transport.
    pub adapter: Option<String>,
    /// The socket named by `--socket`, ahead of every other source.
    pub socket: Option<String>,
    /// The device to open, as `-d` names one.
    pub device: Option<String>,
    /// Whether `-v` was given.
    pub verbose: bool,
}

impl OwnedGlobals {
    /// The borrowed form every backend call takes.
    pub fn borrow(&self) -> Globals<'_> {
        Globals {
            transport: &self.transport,
            adapter: self.adapter.as_deref(),
            socket: self.socket.as_deref(),
            device: self.device.as_deref(),
            verbose: self.verbose,
        }
    }
}

/// A device verb's link to a device, either way round.
///
/// Both variants are boxed: a session and a client are both large, and a verb holds exactly
/// one backend for its whole run.
pub enum Backend {
    /// A session this process owns.
    Direct(Box<DirectBackend>),
    /// A request/response conversation with a running daemon.
    Remote(Box<RemoteBackend>),
}

/// How long a release is given to finish before the front end says it is waiting for it.
pub const RELEASE_QUIET: Duration = Duration::from_millis(500);

/// One thing a watching verb hears from its backend.
#[derive(Debug)]
pub enum Update {
    /// A reading the device reported.
    Status(ValueObject),
    /// The link to the device went down, or came back.
    Connection {
        /// Whether the link is up as of this update.
        connected: bool,
    },
    /// A preset was recalled on the device by another front end: the daemon-wide memory.
    Recalled(String),
    /// The daemon's view of the host: its player, its cover and the route to the default sink.
    /// Only a feed opened with [`Backend::open_host_feed`] carries it. Boxed: it is far larger
    /// than any other update, and every wait on the feed carries one.
    Host(Box<svolder_ipc::host::HostView>),
}

/// What [`Backend::hold`] found: the backend, the daemon's rows as they stood. The backend keeps
/// the socket of the daemon that answered, so what follows holds that daemon and no other.
pub struct Held {
    /// The backend, on no device or on the one that is waited for.
    pub backend: Backend,
    /// The daemon's device list when it was asked.
    pub rows: Vec<DeviceRow>,
}

/// One device in a front end's device list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceRow {
    /// The device's configured name, which is what a request selects it by.
    pub name: String,
    /// The advertised product name, when the device has been seen.
    pub product: Option<String>,
    /// Whether the link is up, or `None` when nothing is holding one: a configuration file
    /// says which devices exist, not how they are doing.
    pub connected: Option<bool>,
    /// Whole seconds until the daemon's next connect attempt, while it is waiting between
    /// attempts: as of when the row was asked for, so a front end that shows it asks again.
    pub retry_in_secs: Option<u64>,
    /// How strong the device's radio signal last was, where it was measured and when. A daemon
    /// lists the last one it heard; a direct session has the advertisement it opened on.
    pub signal: Option<svolder_ipc::Signal>,
    /// The name of the profile the device is read through, once a backend has resolved one.
    pub profile: Option<String>,
    /// The product id the device advertises, once it has been seen.
    pub product_id: Option<u16>,
    /// Whether the daemon holds readings the device has not confirmed since its link went down.
    pub stale: bool,
    /// Whether the device has refused the passphrase on the link the daemon holds.
    pub refused: bool,
}

impl DeviceRow {
    /// One device named `name`, with no product or connection state known: what a
    /// configuration file alone says before a link has told a front end more.
    #[cfg(test)]
    pub(crate) fn for_test(name: &str) -> DeviceRow {
        DeviceRow {
            name: name.into(),
            ..DeviceRow::default()
        }
    }
}

/// Why a write did not go through.
///
/// A direct session reports a [`BleError`]; the daemon reports a code and words of its own,
/// and those travel unchanged so the CLI leaves with the very code the daemon chose. The
/// refusals the CLI makes before it sends anything ride here too, with the code they earn.
///
/// A write reports through the same table every other failure does: [`WriteError::failure`]
/// turns one into a [`Failure`], so a refused `set` names a step exactly as a refused open
/// would.
#[derive(Debug)]
pub enum WriteError {
    /// A direct session's own failure.
    Ble(BleError),
    /// A refusal, carrying the exit code to leave with.
    Refused {
        /// The exit code this refusal earns.
        code: u8,
        /// What to tell the user.
        message: String,
        /// The step that follows these words.
        next: &'static str,
    },
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Ble(e) => e.fmt(f),
            WriteError::Refused { message, .. } => f.write_str(message),
        }
    }
}

impl WriteError {
    /// This refusal as a failure: its words, the step that follows them, and the code to leave
    /// with.
    ///
    /// A session's own error takes the step its kind earns, so a write the device turned away
    /// over the passphrase names the passphrase and one the link dropped under names the link.
    pub fn failure(&self) -> Failure {
        match self {
            WriteError::Ble(e) => Failure::from_ble(e),
            WriteError::Refused {
                code,
                message,
                next,
            } => Failure {
                code: *code,
                what: message.clone(),
                next: (*next).to_string(),
            },
        }
    }
}

/// What a front end keeps current about the device it is on: how the passphrase stands against
/// its advertisement, and the device's own row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnFacts {
    /// the passphrase note, when there is one
    pub note: Option<svolder_ipc::PassphraseNote>,
    /// the device's own row, when a backend could say
    pub row: Option<DeviceRow>,
}

/// What the daemon's own list of the device says, as the facts a front end keeps current.
fn facts_of(info: Option<svolder_ipc::DeviceInfo>) -> OwnFacts {
    match info {
        Some(info) => OwnFacts {
            note: info.passphrase_note,
            row: Some(remote::device_row(&info)),
        },
        None => OwnFacts::default(),
    }
}

/// The host view a daemon's answer to a `host` request carries.
fn host_of(response: Option<Response>) -> Option<svolder_ipc::host::HostView> {
    match response? {
        Response::Host(view) => Some(view),
        _ => None,
    }
}

/// What an identify request came to, with the device the daemon (or the session) answered for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identified {
    /// The name the device goes by: the daemon's own for the one it resolved, the configured
    /// name or the unit id for a direct session.
    pub device: String,
    /// Whether a frame went out; false for a repeat inside the two seconds, which sends nothing.
    pub sent: bool,
}

/// A refusal with the exit code it earns and the step that follows it.
fn refused(code: u8, next: &'static str, message: impl Into<String>) -> WriteError {
    WriteError::Refused {
        code,
        message: message.into(),
        next,
    }
}

/// A refusal the user can fix by typing the command differently: exit 1.
fn bad_request(message: impl Into<String>) -> WriteError {
    refused(1, next_step(Kind::Usage), message)
}

/// A refusal that comes of the link being away: exit 3.
fn no_link(message: impl Into<String>) -> WriteError {
    refused(3, next_step(Kind::Dropped), message)
}

impl Backend {
    /// Where this verb runs, as `--transport` asks.
    ///
    /// `auto`, the default, prefers the daemon: it walks [`launch::reach`]'s chain (a daemon
    /// answering on the resolved socket, a socket unit systemd has but has not started, a daemon
    /// this process starts itself) and opens a direct BlueZ session only when none of those
    /// leaves a daemon to talk to. `daemon` walks the same chain but fails rather than open a
    /// session of its own. `ble` and `fake` force a direct transport without consulting the
    /// daemon, so the in-process device is never shadowed by a daemon on the same machine.
    /// `connect` false is for a verb that needs no established link, leaving the session
    /// unopened on the direct path. `-v` names the choice on stderr.
    pub async fn open(g: &Globals<'_>, connect: bool) -> Result<Backend, Failure> {
        known_transport(g.transport)?;
        let config = Backend::load_config_for(g, &Config::path())?;
        Backend::open_with(g, &config, connect).await
    }

    /// The configuration a verb opens with, read from `path`: checked for a dangling default and
    /// its warnings said.
    pub(crate) fn load_config_for(g: &Globals<'_>, path: &Path) -> Result<Config, Failure> {
        load_config(path, g.device)
    }

    /// [`Backend::open`] over a configuration the caller has already read, so a front end that
    /// opens often reads the file once.
    pub(crate) async fn open_with(
        g: &Globals<'_>,
        config: &Config,
        connect: bool,
    ) -> Result<Backend, Failure> {
        match g.transport {
            "ble" => Backend::direct(g, config, Wire::Ble, connect).await,
            "fake" => Backend::direct(g, config, Wire::Fake, connect).await,
            // `auto` and `daemon`, the two that look for a daemon before anything else.
            _ => match launch::reach(g, config).await? {
                launch::Reached::Daemon(path) => {
                    daemon_note(g, &path);
                    let remote = RemoteBackend::open(&path, g.device, connect).await?;
                    Ok(Backend::Remote(Box::new(remote)))
                }
                launch::Reached::Direct { autospawn_failed } => {
                    let mut backend = Backend::direct(g, config, Wire::Ble, connect).await?;
                    // Said once, by whoever opened this: the chain says nothing itself, so a
                    // front end drawing on the alternate screen shows it as a notice instead.
                    if let Some(failed) = autospawn_failed {
                        backend.warn(failed.line());
                    }
                    Ok(backend)
                }
            },
        }
    }

    /// Open a session on the daemon listening on `path` and nothing else: no launcher chain, so
    /// no daemon is started and no direct session stands in for one that is gone. The device
    /// list is the one the daemon sent while this opened (it always connects to the device, so
    /// the list is always read; the empty fallback only types the `Option`).
    pub async fn open_on(
        path: &std::path::Path,
        device: Option<&str>,
    ) -> Result<(Backend, Vec<DeviceRow>), Failure> {
        let remote = RemoteBackend::open(path, device, true).await?;
        let rows = remote.rows().unwrap_or_default();
        Ok((Backend::Remote(Box::new(remote)), rows))
    }

    /// A backend on the daemon that answers, reading no device: what a front end holds while it
    /// has no device open, so it can still list the daemon's devices and tell it to reload.
    ///
    /// `socket` is the daemon to use, else the one `--socket` and the configuration name, looked
    /// for and never started. With `wanted` naming a device the daemon lists as not connected, the
    /// backend is on that device under the daemon's own name for it (so its events are the ones
    /// the daemon publishes), which is how a front end waits for a device the daemon has not
    /// found yet; a device the daemon does not list, or lists as connected, gives nothing.
    /// Without `wanted` the backend is on no device. Nothing is asked of the device itself.
    /// `config` is the file that names the daemon's socket, read only when `socket` is `None`.
    pub async fn hold(
        g: &Globals<'_>,
        socket: Option<&Path>,
        wanted: Option<&str>,
        config: &Path,
    ) -> Option<Held> {
        let path = match socket {
            Some(path) => path.to_path_buf(),
            None => {
                if !asks_a_daemon(g.transport) {
                    return None;
                }
                let read = Config::load_from(config).ok()?;
                launch::probe(g, &read).await?
            }
        };
        let infos = tokio::time::timeout(ASK_WAIT, remote::device_infos(&path))
            .await
            .ok()?
            .ok()?;
        let device = match wanted {
            Some(selector) => Some(remote::waited_name(&infos, selector)?),
            None => None,
        };
        let remote = RemoteBackend::open(&path, device.as_deref(), false)
            .await
            .ok()?;
        let rows = infos.iter().map(remote::device_row).collect();
        Some(Held {
            backend: Backend::Remote(Box::new(remote)),
            rows,
        })
    }

    /// Open a session of this process's own over `wire`.
    async fn direct(
        g: &Globals<'_>,
        config: &Config,
        wire: Wire,
        connect: bool,
    ) -> Result<Backend, Failure> {
        if g.verbose {
            eprintln!("svolder: direct ({})", wire.name());
        }
        let direct = DirectBackend::open(g, config, wire, connect).await?;
        Ok(Backend::Direct(Box::new(direct)))
    }

    /// A direct session on `transport`, connected: `transport` may wrap the one
    /// [`transport_for`] builds, to watch what passes through it; `wire` and `adapter` are what
    /// it opened that on.
    pub(crate) async fn direct_over(
        g: &Globals<'_>,
        config: &Config,
        transport: Arc<dyn Transport>,
        wire: Wire,
        adapter: Option<&str>,
    ) -> Result<Backend, Failure> {
        let direct = direct::open_over(g, config, transport, true, wire, adapter).await?;
        Ok(Backend::Direct(Box::new(direct)))
    }

    /// Every device a front end can act on, with its link state when a daemon holds it, asking
    /// a daemon only when one is already answering.
    ///
    /// Listing devices starts nothing: [`launch::probe`] is the launcher chain's first step on
    /// its own, so a daemon that is up answers and a machine with none is read from its
    /// configuration file. Both front ends that ask are only looking (`doctor` reports what the
    /// machine is doing; the TUI asks what there is to switch to right after [`Backend::open`]
    /// has walked the whole chain), and a second walk would, on a machine where the daemon won't
    /// start, mean two attempts to start it for a single open.
    ///
    /// `--transport` still decides who may be asked: `ble` and `fake` read the configuration
    /// without consulting a daemon, anything else is the usage error `open` reports. No
    /// Bluetooth transport is opened either way, and a configured device nothing is watching has
    /// no link state to report.
    pub async fn devices_if_running(g: &Globals<'_>) -> Result<Vec<DeviceRow>, Failure> {
        known_transport(g.transport)?;
        let config = Config::load().map_err(|e| config_error(e.to_string()))?;
        print_warnings(&config);
        Backend::devices_in(g, &config).await
    }

    /// [`Backend::devices_if_running`] over a configuration the caller has already read and
    /// checked.
    pub(crate) async fn devices_in(
        g: &Globals<'_>,
        config: &Config,
    ) -> Result<Vec<DeviceRow>, Failure> {
        if !asks_a_daemon(g.transport) {
            return Ok(rows_from_config(config));
        }
        match launch::probe(g, config).await {
            Some(path) => remote::device_rows(&path).await,
            None => Ok(rows_from_config(config)),
        }
    }

    /// The device list the daemon answered when this backend opened, as rows; `None` for a
    /// direct backend and for one opened without connecting.
    pub(crate) fn listed_rows(&self) -> Option<Vec<DeviceRow>> {
        match self {
            Backend::Direct(_) => None,
            Backend::Remote(remote) => remote.rows(),
        }
    }

    /// Every device seen within `window`, one entry each, over this backend.
    ///
    /// The caller opens the backend once, with [`Backend::open`] — which is what may start a
    /// daemon and is where a notice about that belongs — and can call this again for as many
    /// windows as a search needs without walking the launcher chain a second time.
    pub async fn scan(&mut self, window: Duration) -> Result<Vec<ScanEntry>, Failure> {
        match self {
            Backend::Direct(direct) => direct.scan(window).await,
            Backend::Remote(remote) => remote.scan(window).await,
        }
    }

    /// Every reading the device has confirmed.
    pub async fn state(&mut self) -> Result<Vec<ValueObject>, Failure> {
        let mut values = match self {
            Backend::Direct(direct) => direct.state(),
            Backend::Remote(remote) => remote.state(),
        };
        apply_only_while(self.profile(), &mut values);
        Ok(values)
    }

    /// Every reading the device holds now, read again rather than from anything cached.
    ///
    /// This is what a front end reprints after a reconnect, so a value that changed while the
    /// device was away is visible. A direct backend whose session is away with a reconnect
    /// attempt has no device to read and says so with exit code 3.
    pub async fn snapshot(&mut self) -> Result<Vec<ValueObject>, Failure> {
        let mut values = match self {
            Backend::Direct(direct) => direct.snapshot()?,
            Backend::Remote(remote) => remote.snapshot().await?,
        };
        apply_only_while(self.profile(), &mut values);
        Ok(values)
    }

    /// The same read as [`Backend::snapshot`], as a future that holds nothing of this backend, so
    /// a caller can keep reading updates while it is in flight.
    ///
    /// A direct backend answers from what it holds at the moment of the call.
    pub fn snapshot_request(
        &self,
    ) -> futures::future::BoxFuture<'static, Result<Vec<ValueObject>, Failure>> {
        let profile = self.profile().clone();
        let answer: futures::future::BoxFuture<'static, Result<Vec<ValueObject>, Failure>> =
            match self {
                Backend::Direct(direct) => {
                    let answer = direct.snapshot();
                    Box::pin(async move { answer })
                }
                Backend::Remote(remote) => Box::pin(remote.snapshot_request()),
            };
        Box::pin(async move {
            let mut values = answer.await?;
            apply_only_while(&profile, &mut values);
            Ok(values)
        })
    }

    /// How the device's link has come and gone over the last `span_secs` seconds, as the daemon
    /// recorded it: `None` for a direct session, which has no history but its own, and for a
    /// daemon that cannot answer.
    pub async fn link_history(&mut self, span_secs: u64) -> Option<Vec<svolder_ipc::LinkChange>> {
        match self {
            Backend::Direct(_) => None,
            Backend::Remote(remote) => remote.link_history(span_secs).await,
        }
    }

    /// The device's recent readings as the daemon kept them, with its wall clock: `None` for a
    /// direct session, which has none but its own, and for a daemon that cannot answer.
    pub async fn readings(&mut self, span_secs: u64) -> Option<(u64, Vec<svolder_ipc::SeriesOut>)> {
        match self {
            Backend::Direct(_) => None,
            Backend::Remote(remote) => remote.readings(span_secs).await,
        }
    }

    /// The unit id of the device this backend is on, in the text `scan` and `devices` print.
    ///
    /// A direct session reads it from the advertisement it connected to; a remote one takes the
    /// device this backend selected from the daemon's list, or the only one listed when nothing
    /// was selected. `None` when there is no device to name: a backend
    /// opened without connecting, or a daemon that lists no device this one matches.
    ///
    /// On the remote path this answers from the device list read at open and sends nothing, so
    /// it can be asked whether the event feed is open or not.
    pub async fn unit(&mut self) -> Option<String> {
        match self {
            Backend::Direct(direct) => direct
                .session
                .as_ref()
                .map(|session| session.advertisement().unit_id().to_string()),
            Backend::Remote(remote) => remote.unit().await,
        }
    }

    /// How the device's configured passphrase disagrees with the advertisement of the device,
    /// as a best-effort note, or `None` when it does not or nothing can judge.
    ///
    /// A direct session answers from its last judgement, which it makes at open, after a waited
    /// `set` and after a reconnect; a daemon is asked over a connection of its own, so what the
    /// answer reflects is as fresh as the daemon's own last attempt. A daemon too old to judge
    /// has no note to give. Never the digits of any code: the note is a word.
    pub async fn passphrase_note(&mut self) -> Option<svolder_ipc::PassphraseNote> {
        match self {
            Backend::Direct(direct) => direct.passphrase_note(),
            Backend::Remote(remote) => remote.passphrase_note().await,
        }
    }

    /// The device's own row as the open read it: what the daemon listed, or what a direct
    /// session's advertisement gives. `None` for a backend opened without connecting.
    pub fn own_row_as_opened(&self) -> Option<DeviceRow> {
        match self {
            Backend::Direct(direct) => direct.own_row(),
            Backend::Remote(remote) => remote.own_row_as_opened(),
        }
    }

    /// The device's passphrase note and its own row as they stand now, for a front end that
    /// shows them and has to keep them current. A daemon is asked once, over a connection of
    /// this call's own, and both come from its one answer. Best effort, like
    /// [`Backend::passphrase_note`]: with a daemon that cannot be asked in time there is neither.
    pub async fn own_facts(&self) -> OwnFacts {
        match self {
            Backend::Direct(direct) => OwnFacts {
                note: direct.passphrase_note(),
                row: direct.own_row(),
            },
            Backend::Remote(remote) => facts_of(remote.own_info().await),
        }
    }

    /// The same read as [`Backend::own_facts`], as a future that holds nothing of this backend,
    /// so a caller can keep reading updates while it is in flight.
    pub fn own_facts_request(&self) -> futures::future::BoxFuture<'static, OwnFacts> {
        match self {
            Backend::Direct(direct) => {
                let facts = OwnFacts {
                    note: direct.passphrase_note(),
                    row: direct.own_row(),
                };
                Box::pin(async move { facts })
            }
            Backend::Remote(remote) => {
                let info = remote.own_info_request();
                Box::pin(async move { facts_of(info.await) })
            }
        }
    }

    /// [`Backend::passphrase_note`] as the open judged it, with no question asked: a daemon's
    /// answer is the one its device list carried when this backend opened.
    pub fn passphrase_note_as_opened(&self) -> Option<svolder_ipc::PassphraseNote> {
        match self {
            Backend::Direct(direct) => direct.passphrase_note(),
            Backend::Remote(remote) => remote.passphrase_note_as_opened(),
        }
    }

    /// Tell the daemon this front end recalled the preset `name` by its own writes
    /// ([`Request::Recalled`]); a daemon that cannot take it, and a direct session, are left
    /// as they are: the memory is a nicety, never a reason to fail the recall.
    pub async fn note_recalled(&mut self, name: &str) {
        if let Backend::Remote(remote) = self {
            remote.note_recalled(name).await;
        }
    }

    /// The preset last recalled on the device as the daemon's memory said when this backend
    /// opened; `None` for a direct session, which keeps no memory across front ends.
    pub fn recalled_as_opened(&self) -> Option<String> {
        match self {
            Backend::Direct(_) => None,
            Backend::Remote(remote) => remote.recalled_as_opened(),
        }
    }

    /// How the passphrase `entry` says, which was just written to the configuration file, stands
    /// against the advertisement of the device it is for, as a best-effort note.
    ///
    /// A daemon has taken the entry in by now, re-keyed its session and judged it, so it is asked
    /// for the device it lists under the entry's name, whichever device this backend watches. A
    /// direct session reads the file only when it opens, so for the device it is on it takes the
    /// entry in here, the way the daemon does (see [`DirectBackend::adopt_entry`]), and answers
    /// from that; for any other device it has none to judge against.
    pub async fn passphrase_note_for(
        &mut self,
        entry: &DeviceConfig,
    ) -> Option<svolder_ipc::PassphraseNote> {
        match self {
            Backend::Direct(direct) => {
                let watched = direct.configured.as_deref();
                match watched.is_some_and(|name| name.eq_ignore_ascii_case(&entry.name)) {
                    true => {
                        direct.adopt_entry(&entry.passphrase);
                        direct.passphrase_note()
                    }
                    false => None,
                }
            }
            Backend::Remote(remote) => remote.note_of(Some(&entry.name)).await,
        }
    }

    /// [`Backend::passphrase_note`] as one line that names the device and the fix, for a verb
    /// to print on stderr.
    pub async fn passphrase_note_line(&mut self) -> Option<String> {
        let note = self.passphrase_note().await?;
        let device = match self.configured_name() {
            Some(name) => name,
            None => self.unit().await?,
        };
        Some(note.line(&device))
    }

    /// The refusal a verb that needs the device to answer reports instead of printing nothing:
    /// the device has refused the passphrase on the link this backend reaches it through.
    ///
    /// A backend opened with a refused link is still a working backend, which is what a watched
    /// session wants, so opening never fails for it; a one-shot verb asks here, and a direct
    /// session and a daemon answer alike.
    pub fn refusal(&self) -> Option<Failure> {
        let refused = match self {
            Backend::Direct(direct) => direct.refused(),
            Backend::Remote(remote) => remote.refused(),
        };
        refused.then(|| Failure::from_ble(&BleError::PassphraseRejected))
    }

    /// Whatever the backend has to say that was not worth refusing to open over, once each.
    ///
    /// A daemon that has lost the link still answers from the readings it held, worth having and
    /// worth saying; so is a daemon this process tried to start and could not (why the verb is
    /// running here instead). The backend keeps such a line rather than printing it, so a CLI
    /// verb prints it where it always did and the TUI shows it as a notice instead.
    pub fn take_warnings(&mut self) -> Vec<String> {
        match self {
            Backend::Direct(direct) => std::mem::take(&mut direct.warnings),
            Backend::Remote(remote) => std::mem::take(&mut remote.warnings),
        }
    }

    /// Forget the note that a down device's readings are the last known, for a verb that prints
    /// no readings. Any other note stays.
    pub(crate) fn drop_last_known_warning(&mut self) {
        if let Backend::Remote(remote) = self {
            remote
                .warnings
                .retain(|line| !line.ends_with(remote::LAST_KNOWN));
        }
    }

    /// Keep `line` for whoever opened this backend to say, once, through
    /// [`Backend::take_warnings`].
    pub(crate) fn warn(&mut self, line: String) {
        match self {
            Backend::Direct(direct) => direct.warnings.push(line),
            Backend::Remote(remote) => remote.warnings.push(line),
        }
    }

    /// The profile this backend reads the device through, for keys, ids and tone control.
    pub fn profile(&self) -> &Profile {
        match self {
            Backend::Direct(direct) => &direct.profile,
            Backend::Remote(remote) => &remote.profile,
        }
    }

    /// Set one control from a literal value, or `up`/`down` by `step`.
    ///
    /// The confirmed reading comes back, or nothing when `no_wait` sent without waiting.
    pub async fn set(
        &mut self,
        key: &str,
        value: &str,
        step: i64,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        match self {
            Backend::Direct(direct) => direct.set(key, value, step, no_wait).await,
            Backend::Remote(remote) => {
                let request = Request::Set {
                    device: remote.device.clone(),
                    key: key.to_string(),
                    // The daemon resolves `up`/`down` against the readings it holds.
                    value: value.to_string(),
                    step,
                    no_wait,
                };
                remote.write(request).await
            }
        }
    }

    /// Send one command straight to a function id, bypassing the profile.
    pub async fn raw(
        &mut self,
        id: &str,
        value_type: &str,
        value: &str,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        match self {
            Backend::Direct(direct) => direct.raw(id, value_type, value, no_wait).await,
            Backend::Remote(remote) => {
                let request = Request::Raw {
                    device: remote.device.clone(),
                    id: id.to_string(),
                    value_type: value_type.to_string(),
                    value: value.to_string(),
                    no_wait,
                };
                remote.write(request).await
            }
        }
    }

    /// Whether [`Backend::raw`] can build a frame from these texts, answered without a backend,
    /// so a verb can refuse a value that will never encode before it reaches for a device.
    pub fn check_raw(id: &str, value_type: &str, value: &str) -> Result<(), Failure> {
        direct::raw_message(id, value_type, value)
            .map(drop)
            .map_err(|e| e.failure())
    }

    /// Set one band's frequency (Hz), gain (dB), or Q. `band` is 1-based, as it is typed.
    ///
    /// The confirmed reading comes back, or nothing when `no_wait` sent without waiting — a
    /// front end nudging a band as fast as a key repeats has no use for the echo, which it
    /// reads on the update stream like any other status.
    pub async fn eq_set(
        &mut self,
        band: usize,
        param: &str,
        value: f64,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        match self {
            Backend::Direct(direct) => direct.eq_set(band, param, value, no_wait).await,
            Backend::Remote(remote) => {
                let request = Request::EqSet {
                    device: remote.device.clone(),
                    // 1-based on the wire, so the band travels exactly as it was typed.
                    band,
                    param: param.to_string(),
                    value,
                    no_wait,
                };
                remote.write(request).await
            }
        }
    }

    /// Select the tone-control mode: `flat`, `eq`, or `loudness`.
    ///
    /// As in [`Backend::eq_set`], `no_wait` sends without waiting for the echo.
    pub async fn eq_mode(
        &mut self,
        mode: &str,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        match self {
            Backend::Direct(direct) => direct.eq_mode(mode, no_wait).await,
            Backend::Remote(remote) => {
                let request = Request::EqMode {
                    device: remote.device.clone(),
                    mode: mode.to_string(),
                    no_wait,
                };
                remote.write(request).await
            }
        }
    }

    /// Set the device's passphrase to `code` (four digits, or `none` to clear it): the only
    /// sanctioned path to function 250.
    pub async fn passphrase(&mut self, code: &str) -> Result<(), WriteError> {
        match self {
            Backend::Direct(direct) => direct.passphrase(code).await,
            Backend::Remote(remote) => {
                let request = Request::Passphrase {
                    device: remote.device.clone(),
                    digits: code.to_string(),
                };
                remote.write(request).await.map(|_| ())
            }
        }
    }

    /// Ask the device to show itself (function 200), sending no refresh and waiting for nothing.
    ///
    /// Says whether a frame went out, and for which device: `sent` is false when one went out
    /// within the last two seconds and this call sent nothing. A link the device has refused the
    /// passphrase on still carries it. A device whose link is down is the usual not-connected
    /// refusal; through a daemon it says when the daemon tries the device again.
    pub async fn identify(&mut self) -> Result<Identified, WriteError> {
        self.identify_named(None).await
    }

    /// [`Backend::identify`] for the device `device` names, which a daemon reaches as readily as
    /// the one this backend is on; `None` is the one it is on. A direct session holds one device
    /// and refuses another's name.
    pub async fn identify_named(&mut self, device: Option<&str>) -> Result<Identified, WriteError> {
        match self {
            Backend::Direct(direct) => {
                let other = device.is_some_and(|name| direct.configured.as_deref() != Some(name));
                if other {
                    return Err(bad_request(
                        "this session reaches only the device it is on; switch to the other first",
                    ));
                }
                direct.identify().await
            }
            Backend::Remote(remote) => remote.identify(device).await,
        }
    }

    /// Open the update feed, so [`Backend::next_status`] has something to read.
    ///
    /// The daemon path opens a second connection here and subscribes on that one, so this
    /// backend keeps the request connection every read and write goes over. It is done once
    /// and outside any `select!`: subscribing consumes the connection it is called on, so a
    /// cancelled subscribe would leave a half-made subscription behind. A direct session is
    /// already holding its notifications and has nothing to open.
    ///
    /// Every verb that watches calls this before its first [`Backend::next_status`]; one that
    /// never opened the feed reads nothing from it.
    pub async fn open_feed(&mut self) -> Result<(), BleError> {
        match self {
            Backend::Direct(_) => Ok(()),
            Backend::Remote(remote) => remote.open_feed().await,
        }
    }

    /// [`Backend::open_feed`] for a front end that shows the host's player and the path of the
    /// sound: the daemon's feed also carries its host view ([`Update::Host`]), and the daemon
    /// reads the computer's audio graph for as long as the feed is open. A direct session has
    /// no daemon to say it.
    pub async fn open_host_feed(&mut self) -> Result<(), BleError> {
        match self {
            Backend::Direct(_) => Ok(()),
            Backend::Remote(remote) => remote.open_host_feed().await,
        }
    }

    /// Whether the open update feed asks for the daemon's host view: never on a direct session.
    pub fn host_feed(&self) -> bool {
        match self {
            Backend::Direct(_) => false,
            Backend::Remote(remote) => remote.host_feed(),
        }
    }

    /// Open the daemon's update feed again, asking for its host view when `host`, in place of the
    /// one open. A direct session has no daemon to ask.
    pub async fn resubscribe(&mut self, host: bool) -> Result<(), BleError> {
        match self {
            Backend::Direct(_) => Ok(()),
            Backend::Remote(remote) => remote.resubscribe(host).await,
        }
    }

    /// The next thing the device has to say, or `Ok(None)` when there will be no more.
    ///
    /// Both backends report readings and both report the link going down and coming back, in
    /// that order, so a watching front end reads the same sequence whichever one it holds. A
    /// direct session reconnects inside this call, between the two connection updates.
    pub async fn next_status(&mut self) -> Result<Option<Update>, BleError> {
        match self {
            Backend::Direct(direct) => direct.next_status().await,
            Backend::Remote(remote) => remote.next_status().await,
        }
    }

    /// How long a dropped link is given to come back, for the message a watching verb prints
    /// when it does not. Only a direct session has one; the daemon's stream simply ends.
    pub fn connect_deadline(&self) -> Option<Duration> {
        match self {
            Backend::Direct(direct) => direct
                .session
                .as_ref()
                .map(|session| session.config().connect_deadline),
            Backend::Remote(_) => None,
        }
    }

    /// The device the daemon answered for but no longer lists, whose profile this backend
    /// therefore does not have: the race of a device removed between the two requests.
    pub fn unlisted_device(&self) -> Option<&str> {
        match self {
            Backend::Remote(remote) => remote.unknown_device.as_deref(),
            Backend::Direct(_) => None,
        }
    }

    /// The label of an active fault among `values`, if one is active.
    ///
    /// `values` is what [`Backend::state`] returned, so the answer is read from the same
    /// readings the verb printed rather than from a second fetch. The `Err` side is a
    /// backend with no profile to recognise a fault through: it says so rather than let a
    /// caller read `None` as "no fault".
    pub fn faulted(&self, values: &[ValueObject]) -> Result<Option<String>, String> {
        if let Some(name) = self.unlisted_device() {
            return Err(format!(
                "the daemon answered for an unknown device {name}; faults not checked"
            ));
        }
        let profile = self.profile();
        Ok(values.iter().find_map(|vo| {
            if vo.raw != serde_json::Value::Bool(true) {
                return None;
            }
            let control = profile.control_by_id(vo.id.parse::<FunctionId>().ok()?)?;
            (control.kind == ControlKind::Fault).then(|| control.label.clone())
        }))
    }

    /// Which configured device this backend is on, as the configuration file names it.
    ///
    /// The daemon answers for a device by the name it lists that device under; a direct session
    /// is opened against the `[[device]]` block chosen for it. Either way the answer is the name
    /// a writer edits and a selector matches. `None` where nothing in the file picked the device
    /// out: a device the file says nothing about.
    pub fn configured_name(&self) -> Option<String> {
        match self {
            Backend::Direct(direct) => direct.configured.clone(),
            Backend::Remote(remote) => remote.device.clone(),
        }
    }

    /// Tell whatever holds the devices to read the configuration file again, and answer with
    /// the devices it holds after that.
    ///
    /// A front end that just wrote the file sends this so a device it added is held from now on
    /// and one it took away is let go. A direct session holds the device itself and reads the
    /// file when it opens, so there is nobody to tell: it answers with no names, not a failure.
    pub async fn reload(&self) -> Result<Vec<String>, Failure> {
        match self {
            Backend::Direct(_) => Ok(Vec::new()),
            Backend::Remote(remote) => remote.reload().await,
        }
    }

    /// Whether a daemon holds the devices this backend lists: the one kind of backend whose
    /// device list says how each device is doing, and so the one worth asking again.
    pub fn lists_held_devices(&self) -> bool {
        self.through_daemon()
    }

    /// Whether this backend talks to a daemon, which keeps holding the device after the front
    /// end that asked for it has gone.
    pub fn through_daemon(&self) -> bool {
        matches!(self, Backend::Remote(_))
    }

    /// The socket of the daemon this backend talks to, `None` for a direct session.
    pub fn daemon_socket(&self) -> Option<std::path::PathBuf> {
        match self {
            Backend::Direct(_) => None,
            Backend::Remote(remote) => Some(remote.socket().to_path_buf()),
        }
    }

    /// A future that asks the daemon this backend talks to for its own version, over its own
    /// connection (owned, so it can be spawned and outlive this backend rather than borrow it).
    /// `None` for a direct session, which has no daemon to ask.
    pub fn daemon_version_request(
        &self,
    ) -> Option<impl Future<Output = Option<DaemonInfo>> + Send + 'static> {
        match self {
            Backend::Direct(_) => None,
            Backend::Remote(remote) => Some(remote.version_request()),
        }
    }

    /// A future that sends `request` to the daemon this backend talks to, over a connection of
    /// its own (owned, so it can be spawned), and answers its reply. `None` for a direct
    /// session, which has no daemon to ask, and `None` inside for no answer within the wait a
    /// version request has.
    pub fn daemon_request(
        &self,
        request: Request,
    ) -> Option<impl Future<Output = Option<Response>> + Send + 'static> {
        match self {
            Backend::Direct(_) => None,
            Backend::Remote(remote) => Some(remote.request_once(request)),
        }
    }

    /// A future that opens the daemon's analysed frames of the host's capture over a connection
    /// of their own (owned, so it can be spawned), which holds the daemon's capture for as long
    /// as the stream it gives lives: `None` for a direct session. An error is the daemon's
    /// refusal (a daemon from before the frames, or with no host audio:
    /// [`svolder_ipc::IpcError::Protocol`]) or the connection's failure.
    pub fn host_frames(
        &self,
    ) -> Option<
        impl Future<
            Output = Result<
                futures::stream::BoxStream<'static, svolder_ipc::Event>,
                svolder_ipc::IpcError,
            >,
        > + Send
        + 'static,
    > {
        match self {
            Backend::Direct(_) => None,
            Backend::Remote(remote) => Some(remote.host_frames()),
        }
    }

    /// The daemon's host view as it stands now, over a connection of its own: `None` for a
    /// direct session, for a daemon from before the host view (which answers the request as an
    /// unknown one) and for no answer in time.
    pub async fn host_view(&self) -> Option<svolder_ipc::host::HostView> {
        host_of(self.daemon_request(Request::Host)?.await)
    }

    /// [`Backend::host_view`] as a future that holds nothing of this backend, so a caller can
    /// keep reading updates while it is in flight: `None` for a direct session.
    pub fn host_view_request(
        &self,
    ) -> Option<impl Future<Output = Option<svolder_ipc::host::HostView>> + Send + 'static> {
        let asked = self.daemon_request(Request::Host)?;
        Some(async move { host_of(asked.await) })
    }

    /// The daemon's device list as it stands now, over a connection of this call's own, for a
    /// front end that shows it and has to keep it current.
    ///
    /// A direct session has no list but the configuration file's, which says which devices
    /// exist and not how they are doing, so it answers with no rows rather than with a
    /// failure: nothing about them has changed.
    pub async fn held_rows(&self) -> Result<Vec<DeviceRow>, Failure> {
        match self {
            Backend::Direct(_) => Ok(Vec::new()),
            Backend::Remote(remote) => remote.device_rows().await,
        }
    }

    /// Release whatever this backend holds: a direct session closes its link, so a Forkbeard
    /// device is free for the next central; a remote one drops its connection.
    pub async fn close(self) {
        match self {
            Backend::Direct(mut direct) => direct.close().await,
            Backend::Remote(_) => {}
        }
    }

    /// Release what this backend holds, calling `say` once if that has not finished after
    /// `after`, and go on waiting for it either way.
    ///
    /// A daemon connection has nothing to release, so `say` is never called for one.
    pub async fn close_announcing(self, after: Duration, say: impl FnOnce()) {
        let closing = self.close();
        tokio::pin!(closing);
        tokio::select! {
            () = &mut closing => return,
            () = tokio::time::sleep(after) => {}
        }
        say();
        closing.await;
    }

    /// Release what this backend holds, and say `svolder: releasing NAME…` on stderr if that
    /// takes longer than [`RELEASE_QUIET`]. The line is printed once.
    pub async fn release(self, name: &str) {
        self.close_announcing(RELEASE_QUIET, || eprintln!("svolder: releasing {name}…"))
            .await;
    }

    /// A backend over the in-process fake device, connected and refreshed, beside the gate
    /// that decides whether its link can be severed and reconnected.
    ///
    /// `connect_deadline` is how long one reconnect attempt keeps trying, which a case that
    /// drives a front end across a dropped link sets long enough to outlast the front end's
    /// own timers.
    #[cfg(test)]
    pub async fn fake(connect_deadline: Duration) -> (Backend, std::sync::Arc<FakeGate>) {
        let (direct, gate) = direct::test_support::fake_backend_with(connect_deadline).await;
        (Backend::Direct(Box::new(direct)), gate)
    }

    /// The same over the `fake-amp` profile, whose meters have a scale, so a test can play a
    /// signal on them through the gate.
    #[cfg(test)]
    pub async fn fake_amp(connect_deadline: Duration) -> (Backend, std::sync::Arc<FakeGate>) {
        let (direct, gate) =
            direct::test_support::fake_backend_on("fake-amp", connect_deadline).await;
        (Backend::Direct(Box::new(direct)), gate)
    }

    /// The same over a profile the case built, for a device with controls no built-in has.
    #[cfg(test)]
    pub async fn fake_over(
        profile: Profile,
        connect_deadline: Duration,
    ) -> (Backend, std::sync::Arc<FakeGate>) {
        let (direct, gate) =
            direct::test_support::fake_backend_over(profile, connect_deadline).await;
        (Backend::Direct(Box::new(direct)), gate)
    }

    /// Say that this backend is on the configured device called `name`, which a fake device has
    /// no configuration file to have been picked out of.
    #[cfg(test)]
    pub(crate) fn configured_as(&mut self, name: &str) {
        if let Backend::Direct(direct) = self {
            direct.configured = Some(name.to_string());
        }
    }

    /// A backend over the daemon listening at `socket`, opened the way a verb opens one over a
    /// running daemon: what a case that is about the daemon half of a path drives.
    #[cfg(test)]
    pub(crate) async fn stub_remote(socket: &std::path::Path) -> Backend {
        Backend::stub_remote_named(socket, None)
            .await
            .expect("the daemon on the socket answers")
    }

    /// The same, asking the daemon for the device `selector` names, so a case can see which
    /// device a reopen asked it about.
    #[cfg(test)]
    pub(crate) async fn stub_remote_named(
        socket: &std::path::Path,
        selector: Option<&str>,
    ) -> Result<Backend, Failure> {
        RemoteBackend::open(socket, selector, true)
            .await
            .map(|remote| Backend::Remote(Box::new(remote)))
    }
}

/// The configuration for a verb that acts on the device `selector` names (or the default, with
/// none), or a failure naming what is wrong with it. A `default_device` naming no configured
/// device fails a verb that needs the default, in its own words and without the warning that
/// says the same; what the file only deserves a warning for is printed to stderr here.
fn load_config(path: &Path, selector: Option<&str>) -> Result<Config, Failure> {
    let config = Config::load_from(path).map_err(|e| config_error(e.to_string()))?;
    if let Some(failure) = dangling_default_failure(&config, selector) {
        return Err(failure);
    }
    print_warnings(&config);
    Ok(config)
}

/// Print each of the config's warnings to stderr as `svolder: <warning>`, each at most once
/// per process: a verb that reads the configuration in two places says it once.
pub(crate) fn print_warnings(config: &Config) {
    static SAID: Mutex<Vec<String>> = Mutex::new(Vec::new());
    for warning in config.warnings() {
        let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
        if !said.contains(&warning) {
            eprintln!("svolder: {warning}");
            said.push(warning);
        }
    }
}

/// The failure of a verb that would fall back on a `default_device` naming no configured
/// device: it names the device rather than reaching for another one.
fn dangling_default_failure(config: &Config, selector: Option<&str>) -> Option<Failure> {
    if selector.is_some() {
        return None;
    }
    config
        .dangling_default()
        .map(|name| config_error(svolder_ipc::config::dangling_default_message(name)))
}

/// Say under `-v` which daemon a verb is talking to.
pub(crate) fn daemon_note(g: &Globals<'_>, socket: &std::path::Path) {
    if g.verbose {
        eprintln!("svolder: daemon at {}", socket.display());
    }
}

/// The configured devices as rows, in the order the file lists them.
pub(crate) fn rows_from_config(config: &Config) -> Vec<DeviceRow> {
    config
        .devices
        .iter()
        .map(|device| DeviceRow {
            name: device.name.clone(),
            ..DeviceRow::default()
        })
        .collect()
}

/// A failure the user can fix by typing the command differently: exit 1.
///
/// What `--transport` and the fake device's environment knobs are given is the command line's
/// to say, so a value neither of them can use is answered by the help rather than by the file.
fn usage(message: String) -> Failure {
    Failure::of(Kind::Usage, message)
}

/// A failure the user can fix in the configuration file: exit 1.
fn config_error(message: String) -> Failure {
    Failure::of(Kind::ConfigUnreadable, message)
}

/// Open a session straight to the device this sighting names, with `passphrase`, and let it go
/// again.
///
/// This is what the setup wizard asks before writing a device into the file: whether that
/// device, over that adapter, with that passphrase, lets a session in — the only way to know is
/// to open one. The device is not in any configuration yet, so a config is made for it here,
/// holding it alone; `-d` is dropped along with the rest of the file, since the device to reach
/// is the one just picked, not the one a flag names. `auto` and `daemon` open a session of this
/// process's own like `ble` does, for the same reason: a daemon knows nothing of a device the
/// file does not carry.
///
/// The session is closed before this answers, so the device is free for whatever connects next
/// (the daemon, or the wizard's own next step).
pub(crate) async fn try_connect(
    g: &Globals<'_>,
    seen: &ScanEntry,
    passphrase: &str,
) -> Result<(), Failure> {
    let config = Config {
        devices: vec![svolder_ipc::DeviceConfig {
            name: seen.unit.clone(),
            udid: seen.unit.clone(),
            profile: None,
            passphrase: passphrase.to_string(),
            inputs: std::collections::BTreeMap::new(),
        }],
        ..Config::default()
    };
    let mut alone = g.owned();
    alone.device = None;
    let wire = match g.transport {
        "fake" => Wire::Fake,
        _ => Wire::Ble,
    };
    let backend = Backend::direct(&alone.borrow(), &config, wire, true).await?;
    let refusal = backend.refusal();
    backend.close().await;
    refusal.map_or(Ok(()), Err)
}

/// The profile registry, or a failure pointing at the profiles it could not read.
pub(crate) fn registry() -> Result<svolder_profiles::Registry, Failure> {
    crate::registry().map_err(|e| Failure::of(Kind::UnknownProduct, e))
}

/// Whether `--transport` names somewhere this build can run.
///
/// This is the one list of them: [`Backend::open`] and [`Backend::devices_if_running`] ask it
/// before they dispatch, and a front end with no verb to open a backend for asks it on its own,
/// so an unusable value is refused in the same words whichever caller was given it.
pub fn known_transport(name: &str) -> Result<(), Failure> {
    match name {
        "auto" | "daemon" | "ble" | "fake" => Ok(()),
        other => Err(unknown_transport(other)),
    }
}

/// Whether `--transport` lets a verb consult a running daemon: `auto` and `daemon` do, `ble`
/// and `fake` run without one.
pub fn asks_a_daemon(transport: &str) -> bool {
    matches!(transport, "auto" | "daemon")
}

/// A `--transport` value that names nowhere to run, in the one wording every caller of it
/// uses.
fn unknown_transport(name: &str) -> Failure {
    usage(format!(
        "unknown transport {name:?}; use auto, daemon, ble, or fake"
    ))
}

/// What a stream of single statuses knows of the booleans a readout's `only_while` can name,
/// so each status can be shown with the derived value a full listing shows, and a readout can
/// be shown again when the control it depends on changes under it.
#[derive(Debug, Default)]
pub(crate) struct OnlyWhile {
    /// Whether the last reading of each function, by id, read true.
    truth: std::collections::BTreeMap<String, bool>,
    /// The last reading of each gated readout, by id, and the derived value shown for it.
    readouts: std::collections::BTreeMap<String, (ValueObject, bool)>,
}

impl OnlyWhile {
    /// Remember every reading in a listing, whose gated readouts are already derived.
    pub(crate) fn note_all(&mut self, profile: &Profile, values: &[ValueObject]) {
        for vo in values {
            self.truth.insert(vo.id.clone(), vo.reads_true());
        }
        for vo in values {
            if let Some(control) = gated(profile, vo) {
                let shown = units::shows_true(profile, control, vo.reads_true(), |gate| {
                    self.truth.get(&gate.id.to_string()).copied()
                });
                self.readouts.insert(vo.id.clone(), (vo.clone(), shown));
            }
        }
    }

    /// Remember one status and show it as false when it is a gated readout whose control last
    /// read false or has not been heard. Returns the readouts that name this status's control
    /// and whose derived value it changed, each already derived, to be shown after it.
    pub(crate) fn apply(&mut self, profile: &Profile, vo: &mut ValueObject) -> Vec<ValueObject> {
        self.truth.insert(vo.id.clone(), vo.reads_true());
        if let Some(control) = gated(profile, vo) {
            let shown = units::shows_true(profile, control, vo.reads_true(), |gate| {
                self.truth.get(&gate.id.to_string()).copied()
            });
            vo.display = shown.to_string();
            self.readouts.insert(vo.id.clone(), (vo.clone(), shown));
        }
        let Some(key) = vo.key.as_deref() else {
            return Vec::new();
        };
        let open = vo.reads_true();
        let mut changed = Vec::new();
        for control in &profile.controls {
            let names_this = control
                .only_while
                .as_deref()
                .is_some_and(|gate| gate.eq_ignore_ascii_case(key));
            if !names_this {
                continue;
            }
            if let Some((last, shown)) = self.readouts.get_mut(&control.id.to_string()) {
                let now = units::shows_true(profile, control, last.reads_true(), |_| Some(open));
                if now != *shown {
                    *shown = now;
                    last.display = now.to_string();
                    changed.push(last.clone());
                }
            }
        }
        changed
    }
}

/// The control of a reading that is a readout an `only_while` gates.
fn gated<'p>(profile: &'p Profile, vo: &ValueObject) -> Option<&'p Control> {
    let control = profile.control_by_key(vo.key.as_deref()?)?;
    profile.gate_of(control).map(|_| control)
}

/// Show a readout that has an `only_while` as false while the control it names does not read
/// true. The raw value stays as the device sent it.
fn apply_only_while(profile: &Profile, values: &mut [ValueObject]) {
    let gated: Vec<(usize, bool)> = values
        .iter()
        .enumerate()
        .filter_map(|(i, vo)| {
            let control = gated(profile, vo)?;
            let shown = units::shows_true(profile, control, vo.reads_true(), |gate| {
                let id = gate.id.to_string();
                Some(values.iter().any(|g| g.id == id && g.reads_true()))
            });
            Some((i, shown))
        })
        .collect();
    for (i, on) in gated {
        values[i].display = on.to_string();
    }
}

#[cfg(test)]
mod tests;
