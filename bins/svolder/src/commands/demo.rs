// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder tui --demo`: the TUI on a fake device, on a daemon of its own, in one command.
//!
//! The daemon, its configuration, its state and its socket live in a private directory made for
//! the run, and the TUI process reads its own configuration, state and runtime directory from
//! there too, so
//! nothing of the user's is read or written: not their configuration, their state, their
//! socket or their daemon. The TUI opens that daemon only by connecting to its socket (see
//! [`crate::tui::TuiOptions::demo`]), so no launcher chain starts another one.
//!
//! [`Demo::start`] runs from `main` before the async runtime exists, so the environment is
//! changed while this is the only thread. Leaving stops the daemon and removes the directory,
//! whether the TUI ended with `q`, with an error, with a panic or with a signal. A TUI killed
//! outright cannot clean up, so the daemon asks the kernel to end it when this process does, and
//! the next demo removes the directory it left.

use std::ffi::OsString;
use std::fs::File;
use std::io::{IsTerminal, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use tokio::signal::unix::{SignalKind, signal};

use crate::backend::Globals;
use crate::recovery::{Failure, Kind};
use crate::tui;

/// The configuration the demo daemon reads: the fake device, under the unit id it reports.
const CONFIG: &str = "schema = 1\n[[device]]\nname = \"demo\"\nudid = \"0102030405060708\"\n";

/// What the private directories are called, ahead of their random part. Short, so the command
/// that names the socket fits on one line of a toast at 80 columns.
const PREFIX: &str = "sd-";

/// The file every private directory holds from the moment it is made, and its one line: what
/// marks a directory as a demo's, so the sweep never removes one that is not.
const MARKER: &str = ".svolder-demo";
const MARKER_LINE: &str = "svolder tui --demo private directory\n";

/// How many random hex digits follow [`PREFIX`].
const SUFFIX_DIGITS: usize = 6;

/// The socket's name inside the private directory.
const SOCKET_FILE: &str = "sv.sock";

/// How long the daemon is given to answer on its socket.
const START_WAIT: Duration = Duration::from_secs(10);

/// How long a daemon asked to stop is given before it is killed outright.
const STOP_WAIT: Duration = Duration::from_secs(3);

/// How often the daemon is looked at while it starts or stops.
const POLL: Duration = Duration::from_millis(20);

/// How old a leftover directory must be before the next demo removes it, so a directory another
/// demo is still starting in is never taken.
const STALE_AFTER: Duration = Duration::from_secs(60);

/// What a demo daemon inherits of this process's environment, and nothing besides: the fake
/// device's profile, faults, meter signal, Loki Max report form and silent recall, and the locale.
const PASSED_ON: [&str; 8] = [
    "PATH",
    "LANG",
    "LC_ALL",
    "SVOLDER_FAKE_PROFILE",
    "SVOLDER_FAKE_FAULTS",
    "SVOLDER_FAKE_SIGNAL",
    "SVOLDER_FAKE_LOKI_REPORTS",
    "SVOLDER_FAKE_SILENT_RECALL",
];

/// Set by a signal that arrives while the daemon is starting.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn note_signal(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// The demo's daemon and the directory it lives in. Dropping it stops the daemon and removes the
/// directory, so every way out of the demo, a panic included, leaves neither behind.
pub struct Demo {
    /// The daemon, once it is started.
    child: Option<Child>,
    /// The private directory: mode 0700, a short random name.
    dir: PathBuf,
    /// Whether the directory is left in place when this is dropped: a daemon that failed to
    /// start leaves its log there for the message that names it.
    keep: bool,
}

/// Why the demo did not start.
pub enum StartError {
    /// A signal arrived while the daemon was starting: the same quiet end the plain TUI gives an
    /// interrupt while it opens its device.
    Interrupted,
    /// Anything else, in the words and with the step to report.
    Failed(Failure),
}

impl StartError {
    /// Say what happened on standard error and hand back the exit code.
    pub fn report(self) -> ExitCode {
        match self {
            StartError::Interrupted => {
                eprintln!("svolder: {}", tui::waiting::INTERRUPTED);
                ExitCode::SUCCESS
            }
            StartError::Failed(failure) => failure.report(),
        }
    }
}

impl From<Failure> for StartError {
    fn from(failure: Failure) -> Self {
        StartError::Failed(failure)
    }
}

impl Demo {
    /// Make the private directory, start the `svolderd` beside this binary in it over the fake
    /// transport, wait for it to answer, and point this process's environment at the directory.
    ///
    /// Must be called while this is the only thread.
    pub fn start() -> Result<Demo, StartError> {
        if !std::io::stdout().is_terminal() {
            return Err(Failure::of(
                Kind::Usage,
                "the TUI needs a terminal; its output is being read",
            )
            .into());
        }
        let binary = daemon_beside_this_one()?;
        let base = std::env::temp_dir();
        sweep(&base);
        let dir = private_dir(&base).map_err(|e| failed(format!("{}: {e}", base.display())))?;
        // From here the directory is the guard's, whatever ends the start.
        let mut demo = Demo {
            child: None,
            dir,
            keep: false,
        };
        demo.populate().map_err(|e| failed(e.to_string()))?;
        // Kept until [`run`] has the runtime's own handlers, so there is no moment in which a
        // signal ends this process with the daemon running and the directory made.
        listen_for_signals();
        demo.child = Some(demo.spawn(&binary)?);
        demo.wait_until_up()?;
        isolate(&demo.dir);
        Ok(demo)
    }

    /// The socket the daemon serves.
    pub fn socket(&self) -> PathBuf {
        self.dir.join(SOCKET_FILE)
    }

    /// The homes the daemon reads, with its configuration written.
    fn populate(&self) -> std::io::Result<()> {
        for home in ["config", "state", "data"] {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(self.dir.join(home))?;
        }
        let config = self.dir.join("config").join("svolder");
        std::fs::DirBuilder::new().mode(0o700).create(&config)?;
        std::fs::write(config.join("config.toml"), CONFIG)
    }

    /// Start the daemon, which the kernel ends when this process ends. Its host is the fake one
    /// (a fake player and route, touching no session), so the demo's web page has a Now playing
    /// card; the TUI's own host stays off.
    fn spawn(&self, binary: &Path) -> Result<Child, Failure> {
        let log = File::create(self.dir.join("daemon.log"))
            .map_err(|e| failed(format!("{}: {e}", self.dir.display())))?;
        let mut command = Command::new(binary);
        command
            .env_clear()
            .envs(PASSED_ON.iter().filter_map(|name| {
                std::env::var_os(name).map(|value| (OsString::from(name), value))
            }))
            .env("SVOLDER_TRANSPORT", "fake")
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_HOST", "fake")
            .env("SVOLDER_SYSTEMCTL", self.dir.join("no-systemctl"))
            .env("SVOLDER_UNITDIR", self.dir.join("units"))
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_RUNTIME_DIR", &self.dir)
            .arg("--socket")
            .arg(self.socket())
            .arg("--config")
            .arg(self.dir.join("config").join("svolder").join("config.toml"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                log.try_clone()
                    .map_err(|e| failed(format!("{}: {e}", self.dir.display())))?,
            ))
            .stderr(Stdio::from(log));
        // SAFETY: the closure runs in the child between fork and exec and calls only `getpid`,
        // `getppid`, `prctl` and `_exit`, which are async-signal-safe. `PR_SET_PDEATHSIG` has the
        // kernel send the daemon SIGTERM when the thread that started it ends, which is how it
        // goes when this process is killed outright; the parent is looked at afterwards, since
        // one that ended before the call would never send it.
        unsafe {
            use std::os::unix::process::CommandExt;
            let parent = libc::getpid();
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    libc::_exit(1);
                }
                Ok(())
            });
        }
        command
            .spawn()
            .map_err(|e| failed(format!("{}: {e}", binary.display())))
    }

    /// Wait for the daemon to answer on its socket, or say what became of it.
    fn wait_until_up(&mut self) -> Result<(), StartError> {
        let socket = self.socket();
        let deadline = Instant::now() + START_WAIT;
        loop {
            if UnixStream::connect(&socket).is_ok() {
                return Ok(());
            }
            if INTERRUPTED.load(Ordering::SeqCst) {
                return Err(StartError::Interrupted);
            }
            let log = self.dir.join("daemon.log");
            let exited = self
                .child
                .as_mut()
                .and_then(|c| c.try_wait().ok().flatten());
            if exited.is_some() || Instant::now() >= deadline {
                let said = std::fs::read_to_string(&log)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                // The directory stays, with the log the message names.
                self.keep = true;
                let what = match exited {
                    Some(status) => format!("the demo daemon stopped ({status}): {said}"),
                    None => format!("the demo daemon did not answer in {START_WAIT:?}: {said}"),
                };
                return Err(Failure {
                    code: Kind::AutospawnFailed.code(),
                    what,
                    next: format!("The demo daemon's log is {}", log.display()),
                }
                .into());
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Demo {
    /// Ask the daemon to stop, give it [`STOP_WAIT`], then end it, and remove the directory unless a
    /// failed start keeps it for its log.
    fn drop(&mut self) {
        // A child that already exited is reaped by the check, and its pid is not signalled: the
        // kernel may have given the number to another process.
        if let Some(mut child) = self.child.take()
            && matches!(child.try_wait(), Ok(None))
        {
            // SAFETY: `kill` takes a pid and a signal and has no preconditions of its own; the
            // pid is this process's own child, which `try_wait` just found not yet waited for,
            // so the number is still its own.
            unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            let deadline = Instant::now() + STOP_WAIT;
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(POLL);
            }
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// Take SIGINT, SIGTERM and SIGHUP into [`INTERRUPTED`].
fn listen_for_signals() {
    for number in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        // SAFETY: `signal` is given a signal number and a handler that only stores to an
        // atomic, which is async-signal-safe.
        unsafe { libc::signal(number, note_signal as extern "C" fn(libc::c_int) as usize) };
    }
}

/// The failure for a demo that could not start.
fn failed(what: String) -> Failure {
    Failure::of(Kind::AutospawnFailed, what)
}

/// The `svolderd` that stands beside this `svolder`, which is the daemon of the same build.
/// There is no fallback to `PATH`: that would be another version, or the user's own install.
fn daemon_beside_this_one() -> Result<PathBuf, Failure> {
    let exe = std::env::current_exe().map_err(|e| failed(format!("this binary's path: {e}")))?;
    let beside = exe.with_file_name("svolderd");
    if beside.is_file() {
        Ok(beside)
    } else {
        Err(failed(format!(
            "the demo needs {} beside this svolder; build it: cargo build -p svolderd",
            beside.display()
        )))
    }
}

/// A directory under `base` with a short random name and mode 0700, made new: an existing one is
/// an error, so a name someone else made is never used.
fn private_dir(base: &Path) -> std::io::Result<PathBuf> {
    let mut last = None;
    for _ in 0..8 {
        let mut random = [0u8; 3];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let name: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let dir = base.join(format!("{PREFIX}{name}"));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {
                std::fs::write(dir.join(MARKER), MARKER_LINE)?;
                return Ok(dir);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no free name")))
}

/// Whether `name` is what [`private_dir`] makes: [`PREFIX`] and exactly [`SUFFIX_DIGITS`]
/// lowercase hex digits.
fn demo_name(name: &str) -> bool {
    name.strip_prefix(PREFIX).is_some_and(|suffix| {
        suffix.len() == SUFFIX_DIGITS
            && suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Remove the demo directories under `base` that an earlier run left.
///
/// A directory is a demo's only when it is a real directory (never a symlink), named exactly as
/// [`private_dir`] names them, this user's, mode 0700, and holding [`MARKER`] with its line.
/// Of those, the ones older than [`STALE_AFTER`] with no daemon holding the lock beside their
/// socket are removed; nothing else is touched.
fn sweep(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    let me = unsafe { libc::geteuid() };
    for entry in entries.flatten() {
        let path = entry.path();
        if !demo_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let ours = meta.is_dir() && meta.uid() == me && meta.mode() & 0o777 == 0o700;
        let marked = std::fs::read_to_string(path.join(MARKER)).is_ok_and(|m| m == MARKER_LINE);
        let old = meta
            .modified()
            .ok()
            .and_then(|at| SystemTime::now().duration_since(at).ok())
            .is_some_and(|age| age >= STALE_AFTER);
        if ours && marked && old && no_daemon_holds(&path) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// Whether the lock a daemon takes beside its socket can be taken, which it cannot while that
/// daemon lives. A directory with no lock file has no daemon.
fn no_daemon_holds(dir: &Path) -> bool {
    let Ok(lock) = File::open(dir.join(format!("{SOCKET_FILE}.lock"))) else {
        return true;
    };
    // SAFETY: the descriptor belongs to `lock`, which outlives the call; `flock` reads no memory
    // of ours. The lock is released when `lock` closes.
    unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
}

/// Point this process at the demo's homes, so it reads and writes nothing of the user's: the
/// configuration, state and runtime directory under `dir`, a `systemctl` and unit directory that are not there, no
/// radio, no host audio (whose controls would change the user's own volume and players), and no
/// socket but the one it is given.
fn isolate(dir: &Path) {
    // SAFETY: `Demo::start` is called from `main` before the runtime or any other thread is
    // started, so nothing else reads or writes the environment while this does.
    unsafe {
        for (name, value) in isolated_env(dir) {
            std::env::set_var(name, value);
        }
        std::env::remove_var("SVOLDER_SOCKET");
    }
}

/// What [`isolate`] sets, by name.
fn isolated_env(dir: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("XDG_CONFIG_HOME", dir.join("config")),
        ("XDG_STATE_HOME", dir.join("state")),
        ("XDG_DATA_HOME", dir.join("data")),
        ("XDG_RUNTIME_DIR", dir.to_path_buf()),
        ("SVOLDER_SYSTEMCTL", dir.join("no-systemctl")),
        ("SVOLDER_UNITDIR", dir.join("units")),
        ("SVOLDER_ADAPTER", PathBuf::from("hci-none")),
        ("SVOLDER_TRANSPORT", PathBuf::from("fake")),
        ("SVOLDER_HOST", PathBuf::from("off")),
    ]
}

/// Run the TUI on the daemon listening on `socket` and nothing else (`svolder tui --join`): a
/// second front end for a demo or any private daemon. No daemon is started and no direct session
/// stands in, so with none there the TUI opens on the Devices screen saying so.
pub async fn join(g: &Globals<'_>, no_color: bool, socket: PathBuf) -> ExitCode {
    let socket_arg = socket.to_string_lossy().into_owned();
    let globals = Globals {
        transport: "auto",
        adapter: None,
        socket: Some(&socket_arg),
        device: g.device,
        verbose: g.verbose,
    };
    tui::run(&globals, join_options(no_color, socket)).await
}

/// What `svolder tui --join` runs the TUI with: the socket it was told to join, and no demo.
pub(crate) fn join_options(no_color: bool, socket: PathBuf) -> tui::TuiOptions {
    tui::TuiOptions {
        no_color,
        demo: None,
        join: Some(socket),
        reloaded: None,
        checked: false,
    }
}

/// Run the TUI against the demo daemon on `socket`.
pub async fn run(g: &Globals<'_>, no_color: bool, socket: PathBuf) -> ExitCode {
    let socket_arg = socket.to_string_lossy().into_owned();
    // The demo's own command line: the daemon on its socket, whatever `--transport`,
    // `--adapter` or `$SVOLDER_SOCKET` the user's shell carries.
    let globals = Globals {
        transport: "auto",
        adapter: None,
        socket: Some(&socket_arg),
        device: g.device,
        verbose: g.verbose,
    };
    let options = tui::TuiOptions {
        no_color,
        demo: Some(socket),
        join: None,
        reloaded: None,
        checked: false,
    };
    // The runtime's own handlers replace the start's; a signal that came in between is in
    // [`INTERRUPTED`], and ends the run before the TUI takes the terminal.
    let (Ok(mut term), Ok(mut hup), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::hangup()),
        signal(SignalKind::interrupt()),
    ) else {
        return failed("cannot listen for signals".to_string()).report();
    };
    if INTERRUPTED.load(Ordering::SeqCst) {
        return ExitCode::SUCCESS;
    }
    // A signal drops the TUI, which gives the terminal back.
    tokio::select! {
        code = tui::run(&globals, options) => code,
        _ = term.recv() => ExitCode::SUCCESS,
        _ = hup.recv() => ExitCode::SUCCESS,
        _ = int.recv() => ExitCode::SUCCESS,
    }
}

#[cfg(test)]
mod tests;
