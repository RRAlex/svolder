// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How a front end reaches a daemon, and how one is started when nothing answers.
//!
//! `--transport auto` walks [`reach`]'s chain (the socket, then the socket unit systemd has but
//! has not started, then a daemon this process starts itself, then a session of its own), so a
//! machine with systemd, one without, and one with autospawn turned off all end somewhere that
//! works. `--transport daemon` walks the same chain and insists on the daemon at the end of it.
//!
//! Everything the chain reads of the host arrives through [`Host`], so every step can be taken
//! in a test: what systemd says about the units, how the socket unit is started, which
//! `svolderd` would be spawned, where its output goes and what environment it is started with.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use svolder_ipc::checks::SystemdFacts;
use svolder_ipc::{Config, socket};

use crate::commands::daemon::{self, SOCKET_UNIT};
use crate::recovery::{Failure, Kind};

/// How long a daemon this process started is given to answer on the socket.
const AUTOSPAWN_WAIT: Duration = Duration::from_secs(2);

/// How often the socket is looked at while something is starting behind it.
const POLL: Duration = Duration::from_millis(100);

/// Where a verb runs, once the chain has decided.
#[derive(Debug, PartialEq, Eq)]
pub enum Reached {
    /// A daemon answers on this socket.
    Daemon(PathBuf),
    /// No daemon: the verb opens a session of its own.
    Direct {
        /// The daemon this process tried to start and could not, when it tried: the front end
        /// that asked says this where its messages go, which for the TUI is a notice on a
        /// screen and not a line down the middle of a frame.
        autospawn_failed: Option<AutospawnFailed>,
    },
}

/// A daemon a front end tried to start and could not.
#[derive(Debug, PartialEq, Eq)]
pub struct AutospawnFailed {
    /// What went wrong, in the chain's own words.
    pub what: String,
    /// The log the daemon's own words are in, if it got as far as saying any.
    pub log: PathBuf,
}

impl AutospawnFailed {
    /// The one line a front end says about it: a CLI verb on standard error, the TUI as the
    /// notice it shows anything else with.
    pub fn line(&self) -> String {
        format!(
            "{}; using a direct session — see {}",
            self.what,
            self.log.display()
        )
    }
}

/// What the launcher chain reads of the host.
///
/// The live answers are [`Live`]'s; a test puts its own host in its place, which is what lets
/// the chain be walked with no systemd, with units that are installed but not listening, and
/// with a daemon that is really started — in a directory of the test's own.
pub trait Host {
    /// What systemd says about the two units this project installs.
    fn units(&self) -> SystemdFacts;
    /// Start `svolderd.socket`, or say why it could not be started.
    fn start_socket(&self) -> Result<(), String>;
    /// The `svolderd` to start, or `None` when there is none to find.
    fn daemon_binary(&self) -> Option<PathBuf>;
    /// The file a daemon this process starts appends its output to.
    fn log(&self) -> PathBuf;
    /// What a daemon this process starts is given on top of what it inherits.
    fn daemon_env(&self) -> Vec<(OsString, OsString)>;
}

/// The host itself.
pub struct Live;

impl Host for Live {
    fn units(&self) -> SystemdFacts {
        daemon::Ctl::live().units()
    }

    fn start_socket(&self) -> Result<(), String> {
        daemon::Ctl::live().systemctl(&["start", daemon::SOCKET_UNIT])
    }

    fn daemon_binary(&self) -> Option<PathBuf> {
        beside_this_one().or_else(|| on_path(DAEMON))
    }

    fn log(&self) -> PathBuf {
        crate::state::log_path()
    }

    /// The two knobs that decide which devices a daemon talks to, as this machine set them.
    ///
    /// A child inherits the whole environment anyway; naming these two says which of it
    /// matters, and that they come from the environment rather than from the command line. A
    /// `--transport` or `--adapter` typed here is this command's own: the daemon it starts
    /// serves every front end on the machine and takes its adapter from the configuration file,
    /// as it would have if systemd had started it.
    fn daemon_env(&self) -> Vec<(OsString, OsString)> {
        daemon_env_from(|name| std::env::var_os(name))
    }
}

/// The transports the daemon itself takes, which are the only values worth handing it.
const DAEMON_TRANSPORTS: [&str; 2] = ["ble", "fake"];

/// What a daemon this process starts is given, out of `read`'s answers for the environment.
///
/// `SVOLDER_TRANSPORT` names where a *front end* runs — `auto` and `daemon` are the two ways of
/// asking for a daemon — and the daemon has neither: it is the thing being asked for, and it
/// refuses a name it does not know. So only a value that names a transport the daemon opens is
/// passed on, and a front end told to insist on a daemon starts one that opens the radio.
fn daemon_env_from(read: impl Fn(&str) -> Option<OsString>) -> Vec<(OsString, OsString)> {
    let mut env = Vec::new();
    if let Some(transport) = read("SVOLDER_TRANSPORT")
        .filter(|value| DAEMON_TRANSPORTS.iter().any(|known| value == known))
    {
        env.push(("SVOLDER_TRANSPORT".into(), transport));
    }
    if let Some(adapter) = read("SVOLDER_ADAPTER") {
        env.push(("SVOLDER_ADAPTER".into(), adapter));
    }
    env
}

/// The daemon's file name, which is what is looked for beside this binary and on `PATH`.
const DAEMON: &str = "svolderd";

/// The `svolderd` installed beside the running `svolder`, which is where `make install` and
/// every distribution package put the pair.
fn beside_this_one() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let beside = exe.parent()?.join(DAEMON);
    beside.is_file().then_some(beside)
}

/// The first `name` on `PATH` that is a file, for a `svolder` that was not installed beside its
/// daemon.
fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Where this verb runs: the daemon if one can be reached, else a session of this process's own.
///
/// The chain, in order:
///
/// 1. a daemon answering on the resolved socket — which is also a systemd socket unit that is
///    listening, since connecting to it is what starts the daemon behind it;
/// 2. a socket unit that is installed but not listening: it is started, and the socket is tried
///    again;
/// 3. a daemon this process starts itself, unless `autospawn = false` under `[daemon]` or the
///    socket unit is systemd's to start — a daemon of this process's own would race it for the
///    socket;
/// 4. a direct session.
///
/// `--transport daemon` walks steps 1 to 3 and fails rather than take step 4.
pub async fn reach(g: &super::Globals<'_>, config: &Config) -> Result<Reached, Failure> {
    reach_on(&Live, g, config).await
}

/// The daemon that is already answering, and nothing else: step 1 of the chain on its own.
///
/// This is what a verb that only wants to look uses. `doctor` asks what the daemon is holding
/// and reports it, which is no reason to start one — a check that changed the machine it was
/// checking would be reporting on its own work. Connecting is the one thing it does: on a host
/// whose socket unit is listening, that connect is itself what starts the service behind it,
/// which is systemd's arrangement rather than a step of the chain.
pub async fn probe(g: &super::Globals<'_>, config: &Config) -> Option<PathBuf> {
    let path = socket::path::resolve(g.socket, config.socket.as_deref());
    svolder_ipc::is_running(&path).await.then_some(path)
}

/// [`reach`] over any host, so every step can be taken without one.
async fn reach_on(
    host: &dyn Host,
    g: &super::Globals<'_>,
    config: &Config,
) -> Result<Reached, Failure> {
    let path = socket::path::resolve(g.socket, config.socket.as_deref());
    let insist = g.transport == "daemon";
    if svolder_ipc::is_running(&path).await {
        return Ok(Reached::Daemon(path));
    }

    let failed = |what: String| AutospawnFailed {
        what,
        log: host.log(),
    };
    let units = host.units();
    let socket_unit = units.socket_unit.filter(|_| units.present);
    if socket_unit.is_some_and(|unit| unit.installed && !unit.active) {
        match host.start_socket() {
            Ok(()) => {
                if svolder_ipc::is_running(&path).await {
                    return Ok(Reached::Daemon(path));
                }
                // The unit is started, so the listener is bound: one more look is all a socket
                // that was a moment behind the command needs.
                tokio::time::sleep(POLL).await;
                if svolder_ipc::is_running(&path).await {
                    return Ok(Reached::Daemon(path));
                }
            }
            Err(why) => note(g, &format!("{} could not be started: {why}", SOCKET_UNIT)),
        }
    }

    // A socket unit systemd knows about is systemd's to start, whether it is listening
    // somewhere else or failed a moment ago: a daemon of this process's own would bind the
    // path the unit owns and hold it against the service behind it.
    let systemd_owns_socket = socket_unit.is_some_and(|unit| unit.installed);
    let mut autospawn_failed = None;
    if config.daemon.autospawn && !systemd_owns_socket {
        match autospawn(host, &path).await {
            Ok(()) => {
                note(g, &format!("started {DAEMON} on {}", path.display()));
                return Ok(Reached::Daemon(path));
            }
            // A daemon that would not start is not a reason to refuse the command: step 4
            // still has a session of this process's own to run it in, and the caller is told
            // what happened rather than left to wonder. `--transport daemon`, which has no
            // step 4, reports it as the failure it is.
            Err(failure) => autospawn_failed = Some(failed(failure.what)),
        }
    }

    if insist {
        let mut what = format!("no daemon answers on {}", path.display());
        if let Some(failed) = &autospawn_failed {
            what.push_str(&format!("; {} (see {})", failed.what, failed.log.display()));
        }
        return Err(Failure::of(Kind::NoDaemon, what));
    }
    // Nothing is said here. A daemon that would not start is worth saying whether or not `-v`
    // was given, but where it is said belongs to the front end that asked: a CLI verb puts it
    // on standard error, and the TUI — which opens with the alternate screen already up —
    // shows it as a notice instead of down the middle of a frame.
    Ok(Reached::Direct { autospawn_failed })
}

/// Say what the chain did, for a run that asked with `-v`.
fn note(g: &super::Globals<'_>, what: &str) {
    if g.verbose {
        eprintln!("svolder: {what}");
    }
}

/// Start a daemon of this process's own on `path` and wait for it to answer there.
///
/// The daemon starts in a session of its own, with output appended to the log this machine keeps
/// for it, so it outlives the command that started it and says in one place why it stopped if
/// it did. Two front ends starting one at the same moment end with one daemon between them: the
/// second to reach the socket finds it taken and refuses to run, and its client reaches the
/// first one's daemon on the next look.
///
/// `host` says which daemon is started, where it logs and what it is started with; [`Live`] is
/// this machine's answer to all three.
pub(crate) async fn autospawn(host: &dyn Host, path: &Path) -> Result<(), Failure> {
    let log = host.log();
    let failed = |what: String| Failure::of(Kind::AutospawnFailed, what);
    let Some(binary) = host.daemon_binary() else {
        return Err(failed(format!(
            "no {DAEMON} beside {} or on PATH",
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf))
                .unwrap_or_default()
                .display()
        )));
    };
    let output = open_log(&log).map_err(|e| failed(format!("{}: {e}", log.display())))?;

    let mut command = Command::new(&binary);
    command
        .env("SVOLDER_SOCKET", path)
        .envs(host.daemon_env())
        // A handover this process was given is this process's: a daemon that inherited
        // `LISTEN_FDS` would adopt whatever descriptor 3 happens to be here.
        .env_remove("LISTEN_FDS")
        .env_remove("LISTEN_PID")
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            output
                .try_clone()
                .map_err(|e| failed(format!("{}: {e}", log.display())))?,
        ))
        .stderr(Stdio::from(output));
    // SAFETY: the closure runs in the child between fork and exec and calls nothing but
    // `setsid`, which is async-signal-safe. It puts the daemon in a session of its own, so
    // that the terminal this front end was started from cannot end it: a Ctrl-C, a closed
    // window or a shell that kills its job leaves the daemon serving.
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| match libc::setsid() {
            -1 => Err(std::io::Error::last_os_error()),
            _ => Ok(()),
        });
    }
    let mut child = command
        .spawn()
        .map_err(|e| failed(format!("{}: {e}", binary.display())))?;

    let deadline = Instant::now() + AUTOSPAWN_WAIT;
    // The status of a daemon that stopped with the code it exits with when another daemon holds
    // the socket: that other daemon may still be starting, so this keeps looking for it.
    let mut refused = None;
    loop {
        if svolder_ipc::is_running(path).await {
            // A daemon that has already stopped — the loser of a race for the socket — is
            // reaped here rather than left as a child nobody waited for.
            let _ = child.try_wait();
            return Ok(());
        }
        if refused.is_none()
            && let Ok(Some(status)) = child.try_wait()
        {
            if status.code() != Some(ALREADY_RUNNING_EXIT) {
                return Err(failed(stopped_without_answering(status, path)));
            }
            refused = Some(status);
        }
        if Instant::now() >= deadline {
            // One last look before it is ended: a daemon that answered inside the last window
            // is the daemon this was for, and killing it here would throw away the very thing
            // it was asked to do.
            if svolder_ipc::is_running(path).await {
                let _ = child.try_wait();
                return Ok(());
            }
            // A refused daemon has been reaped already, and its pid may belong to another
            // process by now.
            if let Some(status) = refused {
                return Err(failed(stopped_without_answering(status, path)));
            }
            // Whatever it is doing, it is not serving this socket, and nothing else will ever
            // wait for it: a process left behind here would hold its devices against the next
            // daemon and answer to nobody.
            end(&mut child).await;
            return Err(failed(format!(
                "the daemon did not answer in {AUTOSPAWN_WAIT:?}"
            )));
        }
        tokio::time::sleep(POLL).await;
    }
}

/// The exit code of a daemon that found another daemon already holding its socket.
const ALREADY_RUNNING_EXIT: i32 = 3;

/// The message for a daemon that stopped without ever answering.
fn stopped_without_answering(status: std::process::ExitStatus, path: &Path) -> String {
    format!(
        "the daemon stopped ({status}) without answering on {}",
        path.display()
    )
}

/// How long a daemon asked to stop is given before it is killed outright.
const TERM_WAIT: Duration = Duration::from_secs(1);

/// End a daemon that never answered: ask it to stop, and kill it if it will not.
async fn end(child: &mut std::process::Child) {
    // SAFETY: `kill` takes a pid and a signal and has no preconditions of its own; the pid is
    // this process's own child, which has not been waited for, so the number is still its own.
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let deadline = Instant::now() + TERM_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) if Instant::now() >= deadline => break,
            Ok(None) => tokio::time::sleep(POLL).await,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The log file, open for appending, with its directory made if it was not there.
///
/// Appending is what keeps one daemon's last words when the next one is started: the file is
/// this machine's record of every daemon nobody started by hand.
fn open_log(log: &Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
}

#[cfg(test)]
mod tests;
