// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `systemctl --user`, as the computer EQ's unit manager runs it.

use std::ffi::OsString;
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// The environment variable that names the `systemctl` to run.
const SYSTEMCTL_ENV: &str = "SVOLDER_SYSTEMCTL";

/// How long a `systemctl` is given to answer before it is given up on.
const TIMEOUT: Duration = Duration::from_secs(5);

/// How often a program that has not finished is looked at again.
const POLL: Duration = Duration::from_millis(20);

/// The user's systemd, reached through `systemctl --user`.
#[derive(Debug, Clone)]
pub struct UserSystemd {
    /// The `systemctl` to run.
    systemctl: OsString,
}

impl UserSystemd {
    /// The `systemctl` the environment names (`SVOLDER_SYSTEMCTL`), else the one on `PATH`.
    pub fn from_env() -> UserSystemd {
        UserSystemd::with_program(
            std::env::var_os(SYSTEMCTL_ENV)
                .filter(|named| !named.is_empty())
                .unwrap_or_else(|| "systemctl".into()),
        )
    }

    /// The `systemctl` at `program`.
    pub fn with_program(program: impl Into<OsString>) -> UserSystemd {
        UserSystemd {
            systemctl: program.into(),
        }
    }

    /// Whether the user manager answers: `systemctl --user` can be run and reads.
    pub fn user_manager_answers(&self) -> bool {
        self.show(&["show", "-p", "Version"]).is_some()
    }

    /// What `systemctl --user <args>` printed, or `None` when it could not be run, failed, or
    /// took longer than the deadline.
    pub fn show(&self, args: &[&str]) -> Option<String> {
        let mut child = std::process::Command::new(&self.systemctl)
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
    /// A `systemctl` that refuses says why in its own words, and those are carried into the
    /// failure the caller reports.
    pub fn systemctl(&self, args: &[&str]) -> Result<(), String> {
        let spelled = format!(
            "{} --user {}",
            self.systemctl.to_string_lossy(),
            args.join(" ")
        );
        let mut child = std::process::Command::new(&self.systemctl)
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
}

/// Wait for `child` to finish, for at most `timeout`; it is killed when it does not.
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
