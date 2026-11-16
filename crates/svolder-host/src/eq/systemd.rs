// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `systemctl --user`, as the computer EQ's unit manager runs it.

use std::ffi::OsString;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

/// The environment variable that names the `systemctl` to run.
const SYSTEMCTL_ENV: &str = "SVOLDER_SYSTEMCTL";

/// How long a `systemctl` is given to answer before it is given up on.
const TIMEOUT: Duration = Duration::from_secs(5);

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
    pub async fn user_manager_answers(&self) -> bool {
        self.show(&["show", "-p", "Version"]).await.is_some()
    }

    /// What `systemctl --user <args>` printed, or `None` when it could not be run, failed, or
    /// took longer than the deadline; the child is killed when it runs out of time.
    pub async fn show(&self, args: &[&str]) -> Option<String> {
        let child = Command::new(&self.systemctl)
            .arg("--user")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        let output = tokio::time::timeout(TIMEOUT, child.wait_with_output())
            .await
            .ok()?
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Run `systemctl --user <args>` to the end, or say what stopped it.
    ///
    /// Its own output is taken rather than left to land wherever this process's does: a front
    /// end with the alternate screen up would otherwise get a line down the middle of a frame.
    /// A `systemctl` that refuses says why in its own words, and those are carried into the
    /// failure the caller reports. The child is killed when it runs out of time.
    pub async fn systemctl(&self, args: &[&str]) -> Result<(), String> {
        let spelled = format!(
            "{} --user {}",
            self.systemctl.to_string_lossy(),
            args.join(" ")
        );
        let child = Command::new(&self.systemctl)
            .arg("--user")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("{spelled}: {e}"))?;
        match tokio::time::timeout(TIMEOUT, child.wait_with_output()).await {
            Ok(Ok(output)) if output.status.success() => Ok(()),
            Ok(Ok(output)) => {
                let status = output.status;
                Err(format!("{spelled}: {status}{}", said(output)))
            }
            Ok(Err(e)) => Err(format!("{spelled}: {e}")),
            Err(_) => Err(format!("{spelled}: no answer within {TIMEOUT:?}")),
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
