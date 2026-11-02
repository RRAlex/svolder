// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolderd`: the svolder daemon. It keeps a session to every configured Forkbeard device
//! and serves front ends over a Unix socket.
//!
//! The daemon at a glance, one line per module:
//! - [`device`]: one actor per device, which owns its session, keeps the link up and answers
//!   commands.
//! - [`fleet`]: the view of the daemon the MCP server and the web UI work through.
//! - [`handler`]: the socket's request routing, and the daemon's state and reload.
//! - [`host`]: the computer's player, its cover and the route its sound takes, kept once for
//!   every front end.
//! - [`http`]: the hardened HTTP accept loop the MCP server and the web UI share.
//! - [`http_slot`]: the slot that holds one of those two servers and brings it in line with
//!   its config section.
//! - [`locks`]: taking a lock a panic left poisoned.
//! - [`log`]: the stderr log.
//! - [`mcp`]: the MCP server.
//! - [`mqtt_bridge`]: the task that publishes every device to an MQTT broker for Home
//!   Assistant.
//! - [`recall`]: finding and planning a tone-control preset, for the MCP server and the bridge.
//! - [`scanner`]: the adapter's scans, shared by every actor that looks for its device.
//! - [`transport`]: building the BlueZ or fake transport.
//! - [`web`]: the web UI.
//! - [`writer`]: one writer per device, which orders every write in front of its actor.
//!
//! # Start-up order
//!
//! `main` runs its steps in an order that is itself behaviour. The service manager's descriptors
//! are asked for before the daemon opens anything of its own, and the hangup stream is taken
//! before its first `.await`. One daemon per socket path is decided, by the lock, before the
//! transport opens; the refusal when another daemon answers, and the bind, come before any actor
//! scans or opens a link.

mod device;
mod fleet;
mod handler;
mod host;
mod http;
mod http_slot;
mod locks;
mod log;
mod mcp;
mod mqtt_bridge;
mod recall;
mod scanner;
mod transport;
mod web;
mod writer;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use svolder_ble::Transport;
use svolder_ipc::{Bound, Config, IpcError, checks, listen_fds, serve_bound, serve_on, socket};
use svolder_profiles::Registry;
use tokio::net::UnixListener;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::task::JoinHandle;

use handler::Daemon;

#[derive(Parser)]
#[command(
    name = "svolderd",
    version,
    long_version = svolder_proto::long_version!(),
    about = "Keep sessions to configured Schiit Forkbeard devices and serve them over a socket"
)]
struct Args {
    /// Configuration file to read
    #[arg(long, default_value_os_t = Config::path())]
    config: PathBuf,
    /// Socket to listen on. Defaults to the runtime directory's `svolder.sock`.
    #[arg(long)]
    socket: Option<String>,
    /// Transport backend: `ble` (a real BlueZ adapter) or `fake` (in-process).
    #[arg(long, env = "SVOLDER_TRANSPORT", default_value = "ble")]
    transport: String,
    /// Bluetooth adapter to use with --transport ble (e.g. hci0). Defaults to the first.
    #[arg(long, env = "SVOLDER_ADAPTER")]
    adapter: Option<String>,
    /// Log every command and status (the same as RUST_LOG=debug)
    #[arg(short = 'v', long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    log::init(args.verbose);

    // Asked for before the daemon opens anything of its own: a handed-over socket arrives as
    // one fixed descriptor, so a file or a bus connection opened first could take the number
    // a stale `LISTEN_FDS` would then have the daemon adopt as its listener.
    let handed_over = listen_fds();

    // Taken before the daemon awaits anything, and so before it reads its configuration, opens
    // a transport or binds its socket; `hangup_stream` says what that ordering is for.
    let hangups = hangup_stream();

    let (config, registry, profiles_dir) = match load(&args) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    // Kept alive to the end of `main`, so the lock inside is held for the life of the process.
    let claimed = match claim_socket(&args, &config, &registry, handed_over).await {
        Ok(claimed) => claimed,
        Err(code) => return code,
    };
    let started = start(&args, &config, registry, profiles_dir, hangups, &claimed).await;
    serve_until_stopped(claimed.listener, &claimed.path, &claimed.transport, started).await
}

/// What `start` leaves running beside the accept loop.
struct Started {
    daemon: Arc<Daemon>,
    reloader: JoinHandle<()>,
    health: JoinHandle<()>,
}

/// What the daemon holds once it owns its socket path: the path, the lock that makes it the
/// only daemon there, the transport and the listener.
struct Claimed {
    path: PathBuf,
    /// Held until the process ends; closing the file releases the lock.
    _lock: Option<std::fs::File>,
    transport: Arc<dyn Transport>,
    listener: Listener,
    /// The path a handed-over listener is bound to, when it has one.
    adopted_at: Option<PathBuf>,
}

/// Read the configuration, report its warnings, and build the profile registry, handing back the
/// folder of the user's own profiles it read.
fn load(args: &Args) -> Result<(Config, Arc<Registry>, PathBuf), ExitCode> {
    let config = match Config::load_from(&args.config) {
        Ok(config) => config,
        Err(e) => {
            log::error(&format!("{}: {e}", args.config.display()));
            return Err(ExitCode::from(1));
        }
    };
    for warning in config.warnings() {
        log::warn(&warning);
    }
    let profiles_dir = profiles_dir(&args.config);
    let registry = match Registry::with_user_dir(&profiles_dir) {
        Ok(registry) => Arc::new(registry),
        Err(e) => {
            log::error(&e.to_string());
            return Err(ExitCode::from(1));
        }
    };
    Ok((config, registry, profiles_dir))
}

/// Decide the socket path, take the lock for it, open the transport, refuse if another daemon
/// answers there, and bind (or adopt the handed-over listener).
async fn claim_socket(
    args: &Args,
    config: &Config,
    registry: &Registry,
    handed_over: Option<std::os::unix::net::UnixListener>,
) -> Result<Claimed, ExitCode> {
    // A socket unit binds the listener itself, so its address is the path the daemon serves
    // on; `--socket` and the config's `socket` key choose the path only when nothing was
    // handed over.
    let adopted_at = handed_over.as_ref().and_then(listener_path);
    let path = adopted_at
        .clone()
        .unwrap_or_else(|| socket::path::resolve(args.socket.as_deref(), config.socket.as_deref()));

    // One daemon per socket path, decided before anything else is opened. The lock is held for
    // the life of the process and is released by the kernel however the process ends, so a
    // second daemon started at the same moment finds it taken even while the first has not
    // bound its socket yet.
    let lock = if handed_over.is_some() && adopted_at.is_none() {
        None
    } else {
        match take_lock(&path) {
            Ok(lock) => lock,
            Err(AlreadyRunning) => {
                log::error("another svolderd is already running");
                return Err(ExitCode::from(3));
            }
        }
    };

    let adapter = args.adapter.clone().or_else(|| config.adapter.clone());
    let transport = match transport::make(&args.transport, adapter.as_deref(), registry).await {
        Ok(transport) => transport,
        Err(message) => {
            log::error(&message);
            return Err(ExitCode::from(3));
        }
    };

    // Refuse before any actor starts scanning: a second daemon that opened links first and
    // only then found the socket taken would have stolen the devices from the first. A
    // handed-over listener is this daemon's already, so there is nothing to refuse.
    if handed_over.is_none() && svolder_ipc::is_running(&path).await {
        log::error("another svolderd is already running");
        return Err(ExitCode::from(3));
    }

    // A socket this daemon binds itself is bound before any actor opens a link, so a path that
    // cannot be bound ends the daemon with every device untouched.
    let listener = match handed_over {
        Some(listener) => Listener::HandedOver(listener),
        None => match svolder_ipc::bind(&path).await {
            Ok(bound) => Listener::Bound(bound),
            Err(IpcError::Protocol(message)) => {
                log::error(&message);
                return Err(ExitCode::from(3));
            }
            Err(e) => {
                log::error(&format!("{}: {e}", path.display()));
                return Err(ExitCode::from(1));
            }
        },
    };
    Ok(Claimed {
        path,
        _lock: lock,
        transport,
        listener,
        adopted_at,
    })
}

/// Build the daemon on the claimed socket, bring its devices, bridge and servers up, and start
/// the tasks that run beside the accept loop.
async fn start(
    args: &Args,
    config: &Config,
    registry: Arc<Registry>,
    profiles_dir: PathBuf,
    hangups: Option<Signal>,
    claimed: &Claimed,
) -> Started {
    let daemon = Daemon::new(
        Arc::clone(&claimed.transport),
        registry,
        args.config.clone(),
        profiles_dir,
        claimed.path.clone(),
    )
    .with_host_mode(host::HostMode::from_env())
    .shared();
    daemon.reconcile(config).await;
    daemon.host_to(&config.host);
    daemon.bridge_to(&config.mqtt, &config.presets).await;
    daemon.mcp_to(&config.mcp).await;
    daemon.web_to(&config.web).await;
    let reloader = tokio::spawn(watch_hangups(hangups, Arc::clone(&daemon)));

    // The health of the host is worth a line in the journal but nothing waits on it: it is
    // asked for beside the accept loop, so a bus or a `systemctl` that does not answer delays
    // no client.
    let health = tokio::spawn(report_health(claimed.path.clone(), args.config.clone()));

    // Which socket won, said once. A handed-over listener bound to no path — an abstract one
    // — is still the handed-over listener, and is named as the socket it is rather than as
    // the path `socket::path::resolve` fell back to for everything else.
    log::info(&if matches!(claimed.listener, Listener::HandedOver(_)) {
        let at = match claimed.adopted_at.as_deref() {
            Some(path) => format!("on {}", path.display()),
            None => "on a socket bound to no path".to_string(),
        };
        format!(
            "listening {at}, handed over by the service manager; --socket and the config's \
             socket key are not read"
        )
    } else {
        format!(
            "listening on {}, bound by this daemon",
            claimed.path.display()
        )
    });
    Started {
        daemon,
        reloader,
        health,
    }
}

/// Serve on `listener` until a stop, a signal or a lost socket file, then put the daemon down
/// and say how it ended.
async fn serve_until_stopped(
    listener: Listener,
    path: &Path,
    transport: &Arc<dyn Transport>,
    started: Started,
) -> ExitCode {
    let Started {
        daemon,
        reloader,
        health,
    } = started;
    // A socket the service manager handed over is the service manager's path to keep; one this
    // daemon bound is this daemon's, and a daemon whose file has been unlinked or replaced is
    // listening where nobody can reach it while still holding every device. The inode comes
    // from the bind itself, so nothing here reads the path before it is this daemon's.
    let watched = match &listener {
        Listener::Bound(bound) => bound.inode().map(|inode| (path.to_path_buf(), inode)),
        Listener::HandedOver(_) => None,
    };
    let until_stopped = stop_or_signal(Arc::clone(&daemon), watched);
    let outcome = match listener {
        // The socket file belongs to whoever bound it, so nothing here removes it.
        Listener::HandedOver(listener) => match adopt(listener) {
            Ok(listener) => serve_on(listener, Arc::clone(&daemon), until_stopped).await,
            Err(e) => Err(IpcError::Io(e)),
        },
        Listener::Bound(bound) => serve_bound(bound, Arc::clone(&daemon), until_stopped).await,
    };
    health.abort();
    reloader.abort();
    daemon.shutdown().await;
    // Whatever the transport is still taking off the radio (a connection an actor cut short a
    // connect attempt on) runs on a task of its own, and the runtime is about to stop: the
    // last thing the daemon does is give that work the same grace a session gives it.
    //
    // `Daemon::shutdown` waits for each actor longer than the grace an actor gives a connect
    // attempt to stand down and to close, so an actor that answered has finished by here; the
    // wait below covers whatever connects and releases the transport still has in flight, for
    // every device.
    transport.settle(svolder_ble::CLOSE_GRACE).await;

    match outcome {
        Ok(()) => {
            log::info("stopped");
            ExitCode::SUCCESS
        }
        Err(IpcError::Protocol(message)) => {
            log::error(&message);
            ExitCode::from(3)
        }
        Err(e) => {
            log::error(&format!("{}: {e}", path.display()));
            ExitCode::from(1)
        }
    }
}

/// Log how the host looks: a Bluetooth service that does not answer, an adapter that is missing
/// or off, or a config the daemon could not read explains the device failures around it in the
/// journal.
///
/// The outcomes are information and never a gate. This runs beside the accept loop rather than
/// before it, so nothing a probe waits on holds up a client, and the daemon serves whatever the
/// outcomes say — a host that is wrong now may be right by the time a device is reached for.
async fn report_health(socket: PathBuf, config: PathBuf) {
    let snapshot = checks::live::live(&socket, &config).await;
    let socket = socket.as_path();
    let reported = [checks::BLUETOOTH_SERVICE, checks::ADAPTERS, checks::CONFIG];
    for outcome in checks::run(&snapshot, socket, false)
        .into_iter()
        .filter(|outcome| reported.contains(&outcome.check))
    {
        let line = format!("check: {} — {}", outcome.check, outcome.reason);
        match outcome.state {
            checks::State::Fail => log::error(&line),
            checks::State::Warn => log::warn(&line),
            checks::State::Pass | checks::State::Note => log::info(&line),
        }
    }
}

/// The user's profile directory: the config file's own directory, plus `profiles`. With the
/// default config path that is `~/.config/svolder/profiles`, exactly where the CLI looks.
fn profiles_dir(config: &Path) -> PathBuf {
    config
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("profiles")
}

/// Start listening for SIGHUP, or say why this daemon will not reload on one.
///
/// Registering the handler replaces SIGHUP's default action of ending the process. Doing this
/// before the daemon awaits anything means a hangup sent while it is still starting (reading
/// its configuration, opening its transport, binding its socket) is queued for
/// [`watch_hangups`] instead of killing a daemon that has not finished coming up. A failed
/// registration leaves the default action in place, so it is logged.
fn hangup_stream() -> Option<Signal> {
    match signal(SignalKind::hangup()) {
        Ok(hangups) => Some(hangups),
        Err(e) => {
            log::warn(&format!("cannot listen for SIGHUP: {e}"));
            None
        }
    }
}

/// The socket this daemon serves on: the service manager's, or one this daemon bound itself.
enum Listener {
    HandedOver(std::os::unix::net::UnixListener),
    Bound(Bound),
}

/// Another daemon holds the lock for this socket.
struct AlreadyRunning;

/// Take the lock that makes this daemon the only one for `socket`, or say another holds it.
///
/// The lock is an exclusive `flock` on `<socket>.lock`, created at mode 0600 and never removed:
/// removing it would let a second daemon lock a new file while the first still holds the old
/// one. The returned file must be kept alive, since closing it releases the lock. A socket with
/// no path (an abstract one) has no file to lock and takes none. Failing to open or lock the
/// file for any reason but a lock already held is a warning: the daemon then relies on the
/// refusal checks alone.
fn take_lock(socket: &Path) -> Result<Option<std::fs::File>, AlreadyRunning> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    let mut name = socket.as_os_str().to_owned();
    name.push(".lock");
    let lock = PathBuf::from(name);
    let opened = socket::path::ensure_parent(socket).and_then(|()| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&lock)
    });
    let file = match opened {
        Ok(file) => file,
        Err(e) => {
            log::warn(&format!(
                "{}: cannot open the lock file: {e}",
                lock.display()
            ));
            return Ok(None);
        }
    };
    // SAFETY: the descriptor belongs to `file`, which outlives the call; `flock` reads no
    // memory of ours.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(file));
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
        return Err(AlreadyRunning);
    }
    log::warn(&format!("{}: cannot take the lock: {e}", lock.display()));
    Ok(Some(file))
}

/// The path a listener is bound to, which is the one clients reach it on.
fn listener_path(listener: &std::os::unix::net::UnixListener) -> Option<PathBuf> {
    listener
        .local_addr()
        .ok()?
        .as_pathname()
        .map(Path::to_path_buf)
}

/// Take a handed-over listener into this runtime, which needs it in non-blocking mode.
fn adopt(listener: std::os::unix::net::UnixListener) -> std::io::Result<UnixListener> {
    listener.set_nonblocking(true)?;
    UnixListener::from_std(listener)
}

/// Resolve on the first `stop` request, SIGINT, SIGTERM, or the socket file going out from
/// under the listener — the four things that end the accept loop, and so the daemon.
async fn stop_or_signal(daemon: Arc<Daemon>, watched: Option<(PathBuf, u64)>) {
    tokio::select! {
        () = shutdown() => {}
        () = daemon.stop_requested() => log::info("stop requested"),
        () = socket_taken(watched) => {}
    }
}

/// How often the socket file is looked at to see that it is still the one this daemon bound.
const SOCKET_CHECK: std::time::Duration = std::time::Duration::from_secs(1);

/// Resolve once `path` is no longer the socket file this daemon bound, saying so.
///
/// A listener goes on serving even after its directory entry is removed or reassigned to
/// another inode (systemd binding a socket unit over a daemon a front end had started does
/// this), leaving the daemon answering nobody while still holding every device against
/// whatever replaced it. The inode comes from the bind itself, so nothing here reads a path
/// before it is this daemon's; `None` means a socket handed over by the service manager, whose
/// path is its own to keep, or a bind whose inode the filesystem would not give.
async fn socket_taken(watched: Option<(PathBuf, u64)>) {
    let Some((path, bound)) = watched else {
        std::future::pending::<()>().await;
        return;
    };
    loop {
        tokio::time::sleep(SOCKET_CHECK).await;
        // A path that is gone, or that another inode now answers for, is a path this daemon no
        // longer holds. A path that could not be asked about at all says nothing either way,
        // and a filesystem that answers that once is no reason to put the devices down.
        let taken = match svolder_ipc::socket::server::inode(&path) {
            Ok(None) => true,
            Ok(Some(ino)) => ino != bound,
            Err(_) => false,
        };
        if taken {
            log::error(&format!(
                "{} is no longer the socket this daemon bound; stopping so whatever holds that \
                 path can hold the devices too",
                path.display()
            ));
            return;
        }
    }
}

/// Resolve on the first SIGINT or SIGTERM, whichever the service manager sends.
async fn shutdown() {
    let term = async {
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            // With no SIGTERM handler, Ctrl-C alone ends the daemon.
            Err(e) => {
                log::warn(&format!("cannot listen for SIGTERM: {e}"));
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => log::info("interrupted"),
        _ = term => log::info("terminated"),
    }
}

/// Re-read the configuration and the profiles on every SIGHUP, the same as a `reload` request.
///
/// The stream comes from [`hangup_stream`], taken before the daemon started, so a hangup the
/// daemon was too early to serve is waiting here rather than lost.
async fn watch_hangups(hangups: Option<Signal>, daemon: Arc<Daemon>) {
    let Some(mut hangups) = hangups else {
        return;
    };
    while hangups.recv().await.is_some() {
        log::info("reloading the configuration and the profiles");
        match daemon.reload_all().await {
            Ok(devices) => log::info(&format!("reloaded, serving {}", listed(&devices))),
            Err(message) => log::error(&message),
        }
    }
}

/// A device list as one log line: the names, comma-separated, or that there are none.
fn listed(devices: &[String]) -> String {
    if devices.is_empty() {
        "no devices".to_string()
    } else {
        devices.join(", ")
    }
}

/// Unit tests run with scratch homes, as `make test` gives them, so none reads or writes the
/// user's own config, state, data, cache or runtime directory, however the tests were started.
#[cfg(test)]
#[used]
#[unsafe(link_section = ".init_array")]
static SCRATCH_HOMES: extern "C" fn() = {
    extern "C" fn pin() {
        // SAFETY: the ELF initialisers run before `main`, while the process has one thread.
        unsafe { svolder_ipc::config::pin_scratch_homes() }
    }
    pin
};

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
