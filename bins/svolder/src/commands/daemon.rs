// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `daemon` verbs: what the daemon is doing, what it has said, and how it is installed,
//! restarted, stopped and taken out again.
//!
//! Two machines are served by the same six verbs. On one, systemd owns the daemon: the units
//! this project installs say what it does, `systemctl --user` starts and stops it, and
//! `journalctl` has its log. On the other there are no units at all (no systemd, or a user who
//! never installed them), and the daemon is the one a front end started itself: the socket says
//! whether it is up, a `stop` request stops it, and its log is the file
//! [`crate::state::log_path`] names. Every verb asks which machine it is on and does the right
//! one of the two.
//!
//! Everything that runs a program runs it through [`Ctl`], which gives it a deadline and looks
//! it up on a `PATH` a test can point somewhere else, so no case here needs a systemd.
//!
//! This module holds what the verbs share: [`Ctl`], the facts about the units that decide which
//! machine this is, and the failures they report. The verbs themselves are in four children:
//!
//! - [`status`]: what the daemon is doing, and the units around it.
//! - [`logs`]: what the daemon has said, read from the journal or the log file.
//! - [`lifecycle`]: `restart` and `stop`.
//! - [`install`]: writing the units, enabling them, and taking them out again.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use svolder_ipc::checks::SystemdFacts;
use svolder_ipc::{Config, checks, socket};

use crate::backend::Globals;
use crate::cli::DaemonCommand;
use crate::recovery::{Failure, Kind};

pub(crate) mod install;
pub(crate) mod lifecycle;
pub(crate) mod logs;
pub(crate) mod status;

/// Carry out one of the `daemon` sub-commands.
pub(crate) async fn run(g: &Globals<'_>, command: DaemonCommand) -> ExitCode {
    match command {
        DaemonCommand::Status => crate::done(status::status(g).await),
        DaemonCommand::Logs { follow } => crate::done(logs::logs(g, follow).await),
        DaemonCommand::Restart { if_running } => {
            crate::done(lifecycle::restart(g, if_running).await)
        }
        DaemonCommand::Stop => crate::done(lifecycle::stop(g).await),
        DaemonCommand::Install { on_demand } => {
            // A machine that writes no unit files — one that generates them, or a container —
            // is not a machine this verb failed on: it said so and changed nothing.
            crate::done(install::install(on_demand).await.map(|_| ()))
        }
        DaemonCommand::Uninstall => crate::done(install::uninstall()),
    }
}

/// The units this project installs, named once for every verb that talks about them.
pub use svolder_ipc::checks::live::{SERVICE_UNIT, SOCKET_UNIT};

/// What the socket unit's `ListenStream` says for a socket in the session's runtime directory:
/// systemd's own specifier for it, so the installed unit follows the runtime directory wherever
/// the session puts it.
const RUNTIME_LISTEN_STREAM: &str = "%t/svolder.sock";

/// This svolder's own version, which a daemon's is compared with.
const OWN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long a `systemctl` is given to answer before it is given up on.
const TIMEOUT: Duration = Duration::from_secs(5);

/// How often a program that has not finished is looked at again.
const POLL: Duration = Duration::from_millis(20);

/// Running `systemctl`, `journalctl` and `tail`.
///
/// One place runs them all, so every verb gives its program the same deadline and looks it up
/// the same way — and a test that names a `systemctl` of its own, or puts programs of its own
/// on a `PATH` of its own, drives every verb without a systemd anywhere near it.
pub struct Ctl {
    /// The `systemctl` to run: whatever [`checks::live::systemctl_program`] names.
    systemctl: OsString,
    /// The `PATH` the programs are looked up on, `None` for this process's own.
    path: Option<OsString>,
    /// Whether the programs run in a process group of their own, out of reach of the Ctrl-C
    /// the terminal sends to this process's group.
    own_group: bool,
}

impl Ctl {
    /// The programs this machine has, and the `systemctl` its environment names.
    pub fn live() -> Ctl {
        Ctl {
            systemctl: checks::live::systemctl_program(),
            path: None,
            own_group: false,
        }
    }

    /// The same programs, each run in a process group of its own, so a Ctrl-C pressed at the
    /// terminal does not reach a `systemctl` that is part way through its work.
    pub(crate) fn in_own_group(mut self) -> Ctl {
        self.own_group = true;
        self
    }

    /// The programs in `dir`, and nothing else.
    #[cfg(test)]
    fn in_dir(dir: &std::path::Path) -> Ctl {
        Ctl {
            systemctl: "systemctl".into(),
            path: Some(dir.as_os_str().to_os_string()),
            own_group: false,
        }
    }

    /// A command for `program`, looked up on this [`Ctl`]'s `PATH`.
    fn command(&self, program: &OsStr) -> Command {
        let mut command = Command::new(program);
        if let Some(path) = self.path.as_ref() {
            command.env("PATH", path);
        }
        if self.own_group {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        command
    }

    /// A [`Ctl`] that runs the `systemctl` at `systemctl`, for the tests of what drives one.
    #[cfg(test)]
    pub(crate) fn with_systemctl(systemctl: impl Into<OsString>) -> Ctl {
        Ctl {
            systemctl: systemctl.into(),
            path: None,
            own_group: false,
        }
    }

    /// What `systemctl --user <args>` printed, or `None` when it could not be run, failed, or
    /// took longer than [`TIMEOUT`].
    pub(crate) fn show(&self, args: &[&str]) -> Option<String> {
        let mut child = self
            .command(&self.systemctl)
            .arg("--user")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let status = wait_within(&mut child, TIMEOUT).ok()?;
        // The output is a few hundred bytes of properties, far inside the pipe's own buffer,
        // so the child is never waiting on a reader that has not started.
        let output = child.wait_with_output().ok()?;
        status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Run `systemctl --user <args>` to the end, or say what stopped it.
    ///
    /// Its own output is taken rather than left to land wherever this process's does: a front
    /// end with the alternate screen up would otherwise get a line down the middle of a frame.
    /// A `systemctl` that refuses says why in its own words, and those are worth more than
    /// anything said about it here, so they are carried into the failure the caller reports.
    pub fn systemctl(&self, args: &[&str]) -> Result<(), String> {
        let spelled = format!(
            "{} --user {}",
            self.systemctl.to_string_lossy(),
            args.join(" ")
        );
        let mut child = self
            .command(&self.systemctl)
            .arg("--user")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{spelled}: {e}"))?;
        let waited = wait_within(&mut child, TIMEOUT);
        // A refusal is a line or two on either stream, far inside the pipe's own buffer, so
        // the child is never waiting on a reader that has not started.
        let said = child.wait_with_output().ok().map(said).unwrap_or_default();
        match waited {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!("{spelled}: {status}{said}")),
            Err(why) => Err(format!("{spelled}: {why}{said}")),
        }
    }

    /// What systemd says about the two units, and whether there is a systemd to say it.
    pub fn units(&self) -> SystemdFacts {
        self.asked_units().0
    }

    /// The same, and whether `systemctl` answered at all.
    ///
    /// A `systemctl` that could not be run says the same thing about the units as a systemd
    /// that has never heard of them — nothing — and the two are told apart here rather than
    /// guessed at by whatever reads the facts: one is a machine whose daemon nobody but
    /// svolder started, the other is a machine this verb can say nothing about.
    fn asked_units(&self) -> (SystemdFacts, bool) {
        match self.show(&[
            "show",
            SOCKET_UNIT,
            SERVICE_UNIT,
            "-p",
            checks::live::UNIT_PROPERTIES,
        ]) {
            Some(text) => (checks::live::user_units(&text), true),
            None => (checks::live::absent(), false),
        }
    }
}

/// What a program said on either stream, as one line to put after the failure that names it,
/// and nothing at all when it said nothing.
fn said(output: std::process::Output) -> String {
    let mut words = String::new();
    for stream in [output.stdout, output.stderr] {
        let text = String::from_utf8_lossy(&stream);
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            words.push_str(if words.is_empty() { ": " } else { "; " });
            words.push_str(line);
        }
    }
    words
}

/// Wait for `child` for at most `timeout`, killing it when that runs out.
fn wait_within(child: &mut Child, timeout: Duration) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no answer within {timeout:?}"));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Whether systemd is the one that starts and stops the daemon on this machine.
///
/// A unit file that is there is enough: a service that is installed but stopped is still
/// systemd's to start, and `systemctl` is how it is asked.
fn systemd_manages(units: &SystemdFacts) -> bool {
    units.present
        && [units.socket_unit, units.service_unit]
            .into_iter()
            .flatten()
            .any(|unit| unit.installed)
}

/// Whether neither unit is running, or on its way there: systemd then holds nothing on the
/// socket, so whatever answers there is a daemon a front end started. While the socket unit is
/// running it listens, and a connect would start the service.
fn units_stopped(units: &SystemdFacts) -> bool {
    ![units.socket_unit, units.service_unit]
        .into_iter()
        .flatten()
        .any(|unit| unit.running)
}

/// The socket every verb here talks about: the one a device verb would reach the daemon on.
fn resolved_socket(g: &Globals<'_>) -> Result<PathBuf, Failure> {
    let config = Config::load().map_err(|e| Failure::of(Kind::ConfigUnreadable, e.to_string()))?;
    Ok(socket::path::resolve(g.socket, config.socket.as_deref()))
}

/// Something this verb could not do, in its own words and with the step that follows.
fn trouble(what: String, next: &str) -> Failure {
    Failure {
        code: 1,
        what,
        next: next.to_string(),
    }
}

#[cfg(test)]
pub(crate) mod test_support;
