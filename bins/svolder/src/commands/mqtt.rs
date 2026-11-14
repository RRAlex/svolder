// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder mqtt on|off`, `svolder mqtt set host|port|username <value>` and `svolder mqtt
//! password`: turn the daemon's MQTT bridge on or off and set the broker it logs in to, for a
//! script and for a first run in a container, as `web` and `mcp` do for their servers.
//!
//! A daemon that answers makes the change (`Request::MqttSet`): it writes the one key in the file
//! it reads and reloads, which restarts the bridge, and the verb prints where the bridge then
//! stands, the line `svolder daemon status` prints for it. This side writes nothing while a
//! daemon runs, and a daemon too old to have the request is not written around: the verb says
//! to restart it. With no daemon the verb writes the key in this side's own file, and the bridge
//! follows it when a daemon next starts.
//!
//! The password is never an argument (an argument is readable by every local user while the
//! command runs): `password` reads it from a terminal without echo, asking twice, or from the
//! first line of standard input. It is never printed, by this verb or by any other.
//!
//! The line goes to stdout and every note to stderr. The verb fails when the daemon did not act
//! on the change; a broker that refuses the login is a note, since the key is written and the
//! bridge runs.

use std::io::{BufRead, IsTerminal};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use svolder_ipc::socket::daemon_version::version_within;
use svolder_ipc::{Config, FEATURE_MQTT_SET, MqttOp, MqttStatus, Request, Response};

use crate::backend::Globals;
use crate::cli::{MqttCommand, MqttKey};
use crate::commands::daemon::status::mqtt_line;
use crate::commands::server_switch::{Done, Failed, ask_daemon, finish, places};

/// The variable whose value, set in a daemon's environment, outranks `[mqtt] password`.
const PASSWORD_VAR: &str = "SVOLDER_MQTT_PASSWORD";

/// How long the verb waits, after the daemon's answer, for the broker's first verdict: a bridge
/// that has just started says `connecting` until the broker has answered the connect.
const SETTLE: Duration = Duration::from_secs(3);

/// Carry out `mqtt` or one of its sub-commands.
pub(crate) async fn run(g: &Globals<'_>, command: MqttCommand) -> Result<ExitCode, String> {
    let op = match command {
        MqttCommand::On => MqttOp::Enabled { enabled: true },
        MqttCommand::Off => MqttOp::Enabled { enabled: false },
        MqttCommand::Set { key } => match key {
            MqttKey::Host { value } => MqttOp::Host {
                value: named(value, "the host")?,
            },
            MqttKey::Port { value } => MqttOp::Port { value },
            MqttKey::Username { value } => MqttOp::Username {
                value: Some(value).filter(|v| !v.trim().is_empty()),
            },
        },
        MqttCommand::Password { clear } => MqttOp::Password {
            value: match clear {
                true => None,
                false => Some(read_password().await?),
            },
        },
    };
    let (socket, own) = places(g)?;
    finish(apply(&socket, &own, op).await)
}

/// `value` when it names something.
fn named(value: String, what: &str) -> Result<String, String> {
    match value.trim().is_empty() {
        true => Err(format!("{what} is empty")),
        false => Ok(value),
    }
}

/// The change over a given socket and a given configuration file of this side's own: through the
/// daemon on `socket` when one answers, else written to `own_config`.
pub(crate) async fn apply(socket: &Path, own_config: &Path, op: MqttOp) -> Result<Done, Failed> {
    let request = Request::MqttSet { op: op.clone() };
    let Some(answer) = ask_daemon(
        socket,
        FEATURE_MQTT_SET,
        "change its MQTT settings",
        request,
    )
    .await?
    else {
        return alone(socket, own_config, &op).await.map_err(Failed::from);
    };
    let Response::MqttSet { status, tightened } = answer else {
        return Err(format!("the daemon answered {answer:?}").into());
    };
    let status = settled(socket, status).await;
    let line = mqtt_line(&status);
    let mut notes = Vec::new();
    let asked = match &op {
        MqttOp::Enabled { enabled } => Some(*enabled),
        _ => None,
    };
    match (asked, &status) {
        (Some(true), MqttStatus::Off)
        | (Some(false), MqttStatus::Connecting { .. })
        | (Some(false), MqttStatus::Connected { .. })
        | (Some(false), MqttStatus::Trouble { .. }) => {
            return Err(format!(
                "{line} (written, and the daemon has not acted on it: svolder daemon status)"
            )
            .into());
        }
        (_, MqttStatus::Trouble { problem, .. }) => notes.push(problem.words().to_string()),
        _ => {}
    }
    if tightened {
        notes.push(
            "the config file was readable by others: the daemon made it 0600 now that it holds \
             a password"
                .to_string(),
        );
    }
    if matches!(op, MqttOp::Password { value: Some(_) }) {
        notes.extend(password_notes(
            &Config::load_from(own_config).unwrap_or_default(),
            password_in_env(),
        ));
    }
    notes.extend(username_note(&op, username_in_env()));
    Ok(Done { line, notes })
}

/// Whether this shell's environment gives a username.
fn username_in_env() -> bool {
    std::env::var_os("SVOLDER_MQTT_USERNAME").is_some_and(|v| !v.is_empty())
}

/// What clearing the username says while the environment gives one.
pub(crate) const USERNAME_ENV_NOTE: &str =
    "SVOLDER_MQTT_USERNAME is set in this shell and gives the username where the file has none";

/// What an emptied username still has behind it: the variable that gives it where the file has
/// none, which comes straight back.
fn username_note(op: &MqttOp, given: bool) -> Option<String> {
    let cleared = matches!(op, MqttOp::Username { value: None });
    (cleared && given).then(|| USERNAME_ENV_NOTE.to_string())
}

/// The bridge's status once the broker has said something, or after [`SETTLE`]: the daemon's
/// answer says `connecting` for a bridge that has just started.
async fn settled(socket: &Path, answer: MqttStatus) -> MqttStatus {
    let mut status = answer;
    let until = tokio::time::Instant::now() + SETTLE;
    while matches!(
        status,
        MqttStatus::Connecting {
            retry_in_secs: None,
            ..
        }
    ) && tokio::time::Instant::now() < until
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
        match version_within(socket, Duration::from_secs(1)).await {
            Some(info) => match info.mqtt {
                Some(next) => status = next,
                None => break,
            },
            None => break,
        }
    }
    status
}

/// The change with no daemon on `socket`: the key written to `own_config` by this side. A daemon
/// may have started since the socket was tried and read the file before this write, so a daemon
/// that reads this file is told to reload.
async fn alone(socket: &Path, own_config: &Path, op: &MqttOp) -> Result<Done, String> {
    let (what, mut notes) = match op {
        MqttOp::Enabled { enabled } => (
            match enabled {
                true => "on".to_string(),
                false => "off".to_string(),
            },
            Vec::new(),
        ),
        MqttOp::Commands { commands } => (format!("commands = {commands}"), Vec::new()),
        MqttOp::Host { value } => (format!("host = {value}"), Vec::new()),
        MqttOp::Port { value } => (format!("port = {value}"), Vec::new()),
        MqttOp::Username { value } => (
            match value {
                Some(value) => format!("username = {value}"),
                None => "username cleared".to_string(),
            },
            Vec::new(),
        ),
        MqttOp::Password { value } => (
            match value {
                Some(_) => "password set".to_string(),
                None => "password cleared".to_string(),
            },
            Vec::new(),
        ),
    };
    let op_again = op.clone();
    let op = op.clone();
    let tightened = Config::update_mqtt_noting(own_config, move |mqtt| match op {
        MqttOp::Enabled { enabled } => mqtt.enabled = enabled,
        MqttOp::Commands { commands } => mqtt.commands = commands,
        MqttOp::Host { value } => mqtt.host = Some(value),
        MqttOp::Port { value } => mqtt.port = value,
        MqttOp::Username { value } => mqtt.username = value,
        MqttOp::Password { value } => mqtt.password = value,
    })
    .map_err(|e| e.to_string())?;
    crate::backend::reload_if_reads(socket.to_path_buf(), own_config).await;
    if tightened {
        notes.push(format!(
            "{} was readable by others: now 0600",
            own_config.display()
        ));
    }
    if what == "password set" {
        notes.extend(password_notes(
            &Config::load_from(own_config).unwrap_or_default(),
            password_in_env(),
        ));
    }
    notes.extend(username_note(&op_again, username_in_env()));
    notes.push(format!(
        "nothing answers on {}: the change applies with the next daemon",
        socket.display()
    ));
    Ok(Done {
        line: format!("mqtt: {what} in {}", own_config.display()),
        notes,
    })
}

/// Whether this shell's environment names a password, which a daemon started from it would use
/// over the file's.
fn password_in_env() -> bool {
    std::env::var_os(PASSWORD_VAR).is_some_and(|v| !v.is_empty())
}

/// What a password just written still needs, said once: a username to be sent with, and which
/// variable outranks the file.
fn password_notes(config: &Config, in_env: bool) -> Vec<String> {
    let mut notes = Vec::new();
    if config.mqtt.username.is_none() {
        notes.push(
            "no username is set, so the password is not sent: svolder mqtt set username NAME"
                .to_string(),
        );
    }
    if in_env {
        notes.push(format!(
            "{PASSWORD_VAR} is set in this shell, and a daemon with it set uses it over the file"
        ));
    }
    notes
}

/// The password to set: typed twice without echo when standard input is a terminal, else the
/// first line of standard input.
async fn read_password() -> Result<String, String> {
    let read = tokio::task::spawn_blocking(|| {
        let stdin = std::io::stdin();
        if !stdin.is_terminal() {
            return password_from(&mut stdin.lock());
        }
        let first = prompt("MQTT password: ")?;
        let again = prompt("Again: ")?;
        if first != again {
            return Err("the two entries differ: nothing is changed".to_string());
        }
        named(first, "the password")
    });
    // Ctrl-C ends the process while the reading thread still holds the terminal without echo:
    // the echo is put back first.
    tokio::select! {
        read = read => read.map_err(|e| e.to_string())?,
        _ = tokio::signal::ctrl_c() => {
            crate::wizard::cli::Echo::restore_held();
            eprintln!();
            // The reading thread cannot be stopped, and the runtime would wait for it: the
            // process leaves by itself, as the wizard's does.
            std::process::exit(130)
        }
    }
}

/// One line typed at the terminal without echo, after `text`.
fn prompt(text: &str) -> Result<String, String> {
    use std::io::Write;
    eprint!("{text}");
    let _ = std::io::stderr().flush();
    let hidden = crate::wizard::cli::Echo::off(0);
    let mut line = String::new();
    let read = std::io::stdin().read_line(&mut line);
    if hidden.is_some() {
        eprintln!();
    }
    drop(hidden);
    match read {
        Ok(0) | Err(_) => Err("no password read".to_string()),
        Ok(_) => Ok(line.trim_end_matches(['\n', '\r']).to_string()),
    }
}

/// The first line of `input` without its line end; none, or an empty one, is no password.
fn password_from(input: &mut impl BufRead) -> Result<String, String> {
    let mut line = String::new();
    match input.read_line(&mut line) {
        Ok(_) => {
            let line = line.trim_end_matches(['\n', '\r']).to_string();
            match line.is_empty() {
                true => Err(
                    "no password on standard input (--clear takes the password away)".to_string(),
                ),
                false => Ok(line),
            }
        }
        Err(e) => Err(format!("standard input: {e}")),
    }
}

#[cfg(test)]
mod tests;
