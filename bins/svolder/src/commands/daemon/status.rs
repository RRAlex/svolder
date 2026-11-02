// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon status`: what the daemon is doing, as lines of text.
//!
//! The units come first, then whatever answers on the socket: its version, its MQTT bridge, its
//! MCP server, its web UI and the devices it holds, then the daemon that is not this session's and
//! where the log is. It reports what it found and never fails over it, because a machine with
//! nothing running is a machine it has something true to say about.

use std::path::Path;

use svolder_ipc::checks::UnitFacts;
use svolder_ipc::checks::processes::{RunningDaemon, running_daemons};
use svolder_ipc::socket::daemon_version::{self, VersionReply};
use svolder_ipc::{Client, DeviceInfo, McpStatus, MqttStatus, Request, Response, WebStatus};

use crate::backend::{ASK_WAIT, Globals};
use crate::recovery::Failure;

use super::logs::{JOURNAL, LogSaid, NOT_A_FILE};
use super::{Ctl, OWN_VERSION, SERVICE_UNIT, SOCKET_UNIT, resolved_socket, systemd_manages};

/// What the daemon is doing: the units, the daemon's own version, its MQTT bridge, its MCP
/// server, and the devices it holds.
///
/// This reports and never fails over what it found: a machine with no units and no daemon is a
/// machine this verb has something true to say about, which is the whole of what it is for.
pub async fn status(g: &Globals<'_>) -> Result<(), Failure> {
    let socket = resolved_socket(g)?;
    status_on(
        &Ctl::live(),
        &socket,
        &crate::state::log_path(),
        &running_daemons(),
    )
    .await
}

/// [`status`] over given programs, a given socket, a given log and a given set of running
/// daemons.
async fn status_on(
    ctl: &Ctl,
    socket: &Path,
    log: &Path,
    running: &[RunningDaemon],
) -> Result<(), Failure> {
    for line in status_lines(ctl, socket, log, running).await {
        println!("{line}");
    }
    Ok(())
}

/// Everything [`status`] has to say, one line each.
async fn status_lines(
    ctl: &Ctl,
    socket: &Path,
    log: &Path,
    running: &[RunningDaemon],
) -> Vec<String> {
    let units = ctl.units();
    let mut lines = Vec::new();
    if units.present {
        lines.push(format!("{SOCKET_UNIT}: {}", unit_state(units.socket_unit)));
        lines.push(format!(
            "{SERVICE_UNIT}: {}",
            unit_state(units.service_unit)
        ));
        if let Some(listen) = units.listen_stream.as_deref() {
            lines.push(format!("{SOCKET_UNIT} listens on {}", listen.display()));
        }
    } else {
        lines.push("systemd: no user session; the daemon is whatever started one".to_string());
    }

    let mut said = LogSaid::Unsaid;
    match Client::connect(socket).await {
        Ok(mut client) => match tokio::time::timeout(ASK_WAIT, version(&mut client)).await {
            Ok((version, mqtt, mcp, web, logs_to)) => {
                said = LogSaid::of(&version, logs_to);
                lines.push(format!("daemon: {version} on {}", socket.display()));
                lines.extend(daemon_version::mismatch_line(&version, OWN_VERSION));
                lines.extend(mqtt.as_ref().map(mqtt_line));
                lines.extend(mcp.as_ref().map(McpStatus::line));
                lines.extend(web.as_ref().map(WebStatus::line));
                match tokio::time::timeout(ASK_WAIT, devices(&mut client)).await {
                    Ok(device_lines) => lines.extend(device_lines),
                    Err(_) => lines.push(format!("devices: no answer within {ASK_WAIT:?}")),
                }
            }
            Err(_) => lines.push(format!(
                "daemon: no answer within {ASK_WAIT:?} on {}",
                socket.display()
            )),
        },
        Err(_) => lines.push(format!("daemon: nothing answers on {}", socket.display())),
    }
    lines.extend(also_running(
        running,
        socket,
        units.listen_stream.as_deref(),
    ));
    // What the daemon that answered says it writes, else where a daemon nobody installed
    // writes, unless systemd keeps the log.
    match said {
        LogSaid::Path(reported) if reported == JOURNAL => {
            lines.push("log: the journal".to_string());
        }
        LogSaid::Path(reported) => lines.push(format!("log: {reported}")),
        LogSaid::NoFile => lines.push(format!("log: {NOT_A_FILE}")),
        LogSaid::Unsaid if !systemd_manages(&units) => {
            lines.push(format!("log: {}", log.display()));
        }
        LogSaid::Unsaid => {}
    }
    lines
}

/// One line per svolderd of this user's that listens somewhere this session does not reach,
/// naming where it listens and the command that stops that one.
///
/// A daemon holds the devices of whoever reaches its socket, so one on a path nothing here
/// resolves to (a session started with its own home, runtime directory or `SVOLDER_SOCKET`) is
/// holding devices this session can neither see nor stop, and has to be named to be stopped.
/// Nothing is printed where there is no such daemon.
///
/// `listen` is the path the socket unit binds, when systemd has one: the daemon behind it is the
/// one the lines above are about.
fn also_running(running: &[RunningDaemon], socket: &Path, listen: Option<&Path>) -> Vec<String> {
    running
        .iter()
        .filter_map(|daemon| {
            let told = daemon.elsewhere_than(socket, listen)?;
            Some(format!(
                "also running: svolderd pid {pid} on {path} — stop it with: \
                 svolder --socket {path} daemon stop",
                pid = daemon.pid,
                path = told.display()
            ))
        })
        .collect()
}

/// The daemon's version, or what it said instead, and where its MQTT bridge, its MCP server and
/// its web UI stand when it says, and where it logs: a daemon too old to have one says nothing.
pub(super) async fn version(
    client: &mut Client,
) -> (
    String,
    Option<MqttStatus>,
    Option<McpStatus>,
    Option<WebStatus>,
    Option<String>,
) {
    match daemon_version::version_on(client).await {
        Ok(VersionReply { info, log }) => (info.version, info.mqtt, info.mcp, info.web, log),
        Err(why) => (why.to_string(), None, None, None, None),
    }
}

/// The bridge's line: `mqtt: off`; `mqtt: connecting to HOST:PORT`, with the wait before the
/// next attempt while there is one; `mqtt: connected to HOST:PORT`, read-only or taking
/// commands; or `mqtt: unknown` for a state a newer daemon reports that this one does not
/// know.
pub(crate) fn mqtt_line(status: &MqttStatus) -> String {
    match status {
        MqttStatus::Off => "mqtt: off".to_string(),
        MqttStatus::Connecting {
            host,
            port,
            retry_in_secs,
        } => match retry_in_secs {
            Some(secs) => format!("mqtt: connecting to {host}:{port} (retry in {secs} s)"),
            None => format!("mqtt: connecting to {host}:{port}"),
        },
        MqttStatus::Connected {
            host,
            port,
            commands,
        } => format!(
            "mqtt: connected to {host}:{port} ({})",
            match commands {
                true => "commands on",
                false => "read-only",
            }
        ),
        MqttStatus::Unknown => "mqtt: unknown".to_string(),
    }
}

/// One line per device the daemon holds, or one saying it holds none.
async fn devices(client: &mut Client) -> Vec<String> {
    match client.request(Request::Devices).await {
        Ok(Response::Devices { devices }) if devices.is_empty() => {
            vec!["devices: none configured".to_string()]
        }
        Ok(Response::Devices { devices }) => devices.iter().map(device_line).collect(),
        Ok(other) => vec![format!("devices: an unexpected answer ({other:?})")],
        Err(e) => vec![format!("devices: not listed ({e})")],
    }
}

/// One device's line: what it is called, what it is, and whether the daemon is holding it.
///
/// A device the daemon is waiting to try again for says when it will: the daemon backs its
/// reconnect off while a device stays away, so how long the wait has grown to is the
/// difference between a device that has just dropped and one that has been gone all day.
fn device_line(device: &DeviceInfo) -> String {
    let link = match device.connected {
        true => "connected".to_string(),
        false => crate::format::link_down("not connected", device.retry_in_secs),
    };
    format!(
        "device {}: {link} ({})",
        device.name,
        device.product.as_deref().unwrap_or(&device.unit)
    )
}

/// One unit's state in the words `systemctl` answers in.
fn unit_state(unit: Option<UnitFacts>) -> String {
    let Some(unit) = unit else {
        return "not installed".to_string();
    };
    format!(
        "{}, {}, {}",
        match unit.installed {
            true => "installed",
            false => "not loaded",
        },
        match unit.enabled {
            true => "enabled",
            false => "disabled",
        },
        match unit.active {
            true => "active",
            false => "inactive",
        }
    )
}

#[cfg(test)]
mod tests;
