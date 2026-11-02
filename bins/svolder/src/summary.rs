// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What bare `svolder` prints when its output is being read rather than watched.
//!
//! A script, or a shell over SSH from a home-automation box, gets the one screen it can act
//! on: every configured device with the state something is holding it in, and the command to
//! run next. Nothing here prompts or opens a Bluetooth transport; it connects to the socket,
//! which on a host whose socket unit is listening is itself what starts the service behind it,
//! and starts no daemon of its own beyond that.

use svolder_ipc::checks::{self, Outcome, State};
use svolder_ipc::{Client, Config, DeviceConfig, DeviceInfo, Request, Response, socket};

use crate::backend::Globals;
use crate::format::link_down;

/// What the daemon says about the configured devices, or `None` when none is answering.
///
/// The daemon is consulted on the same terms `devices` consults it on: the resolved socket,
/// and only where `--transport` has not been pointed at a session of this process's own.
pub async fn reached(g: &Globals<'_>, config: &Config) -> Option<Vec<DeviceInfo>> {
    if matches!(g.transport, "ble" | "fake") {
        return None;
    }
    let path = socket::path::resolve(g.socket, config.socket.as_deref());
    if !svolder_ipc::is_running(&path).await {
        return None;
    }
    let mut client = Client::connect(&path).await.ok()?;
    match client.request(Request::Devices).await {
        Ok(Response::Devices { devices }) => Some(devices),
        _ => None,
    }
}

/// What the host checks say about this machine.
///
/// Only the checks a snapshot answers are run: the ones that need a radio session would have
/// this scan for every configured device, and a summary is the one screen a script reads
/// rather than something that takes seconds and touches an adapter. `svolder doctor`, which
/// the summary names, is where those live.
pub async fn health(g: &Globals<'_>, config: &Config) -> Vec<Outcome> {
    let path = Config::path();
    let socket = socket::path::resolve(g.socket, config.socket.as_deref());
    let snapshot = checks::live::live(&socket, &path).await;
    checks::run(&snapshot, &socket, false)
}

/// Print the summary: the host's health, the configured devices and their state, then the
/// command to run next.
pub fn print(config: &Config, reached: Option<&[DeviceInfo]>, health: &[Outcome]) {
    for line in lines(config, reached, health) {
        println!("{line}");
    }
}

/// The summary, one string per line.
///
/// The device lines come from the configuration rather than from the daemon, so a device that
/// is configured but not being held is still listed, with nothing holding it as its state.
/// The last line is the single next command: the TUI where there is something to open it on,
/// the guided first run where there is not.
fn lines(config: &Config, reached: Option<&[DeviceInfo]>, health: &[Outcome]) -> Vec<String> {
    let mut out = vec![health_line(health)];
    if config.devices.is_empty() {
        out.push("no devices configured".to_string());
    }
    for device in &config.devices {
        out.push(format!(
            "{}  {}  {}",
            device.name,
            device.udid,
            state_of(device, reached)
        ));
    }
    out.push(String::new());
    out.push("Run `svolder doctor` to check the host.".to_string());
    out.push(format!(
        "Next: `svolder {}`",
        match config.devices.is_empty() {
            true => "setup",
            false => "tui",
        }
    ));
    out
}

/// The host's health in one line: how many checks passed when they all did, and what went how
/// with the verb that says more when they did not.
///
/// A check that only recorded a fact is counted with the ones that passed: the line is about
/// whether anything is wrong, and a note is not.
fn health_line(health: &[Outcome]) -> String {
    let count = |state: State| health.iter().filter(|o| o.state == state).count();
    let (warnings, failures) = (count(State::Warn), count(State::Fail));
    if warnings == 0 && failures == 0 {
        return format!("host: {} checks pass", health.len());
    }
    let plural = |n: usize, word: &str| match n {
        1 => format!("1 {word}"),
        n => format!("{n} {word}s"),
    };
    let mut trouble = Vec::new();
    if warnings > 0 {
        trouble.push(plural(warnings, "warning"));
    }
    if failures > 0 {
        trouble.push(plural(failures, "failure"));
    }
    format!("host: {} — run svolder doctor", trouble.join(", "))
}

/// Whether anything is holding this device's link, in the words the summary shows it in.
///
/// A daemon that is not answering leaves every device not connected: the configuration says
/// which devices exist, not how they are doing. A device the daemon is waiting to try again
/// for says when it will, as `svolder daemon status` does.
fn state_of(device: &DeviceConfig, reached: Option<&[DeviceInfo]>) -> String {
    let held = reached.and_then(|devices| devices.iter().find(|info| info.name == device.name));
    match held {
        Some(info) if info.connected => "connected".to_string(),
        Some(info) => link_down("not connected", info.retry_in_secs),
        None => "not connected".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One device, as the configuration names it.
    fn configured(name: &str) -> DeviceConfig {
        DeviceConfig {
            name: name.to_string(),
            udid: "0102030405060708".to_string(),
            profile: None,
            passphrase: "auto".to_string(),
            inputs: Default::default(),
        }
    }

    /// The daemon's view of one device.
    fn held(name: &str, connected: bool) -> DeviceInfo {
        DeviceInfo {
            signal: None,
            name: name.to_string(),
            unit: "0102030405060708".to_string(),
            product_id: 1,
            product: Some("Gungnir 2".to_string()),
            firmware: 3,
            profile: "gungnir2-v2".to_string(),
            connected,
            stale: false,
            retry_in_secs: None,
            passphrase_note: None,
            refused: false,
            recalled: None,
        }
    }

    /// A configuration with `devices` in it and nothing else set.
    fn config(devices: Vec<DeviceConfig>) -> Config {
        Config {
            devices,
            ..Config::default()
        }
    }

    /// One check's outcome, as the checks hand it over.
    fn checked(state: State) -> Outcome {
        Outcome {
            check: "Daemon",
            state,
            reason: "because".into(),
            fix: None,
        }
    }

    /// A healthy host, as the checks report one.
    fn healthy() -> Vec<Outcome> {
        vec![checked(State::Pass), checked(State::Note)]
    }

    /// With nothing configured there is nothing to list, and the next command is the one that
    /// configures the first device.
    #[test]
    fn an_empty_configuration_points_at_the_first_run() {
        let lines = lines(&config(Vec::new()), None, &healthy());
        assert_eq!(
            lines.get(1).map(String::as_str),
            Some("no devices configured")
        );
        assert_eq!(
            lines.last().map(String::as_str),
            Some("Next: `svolder setup`")
        );
    }

    /// A configured device is listed whether or not a daemon is holding it, and the next
    /// command is the one that opens it.
    #[test]
    fn a_configured_device_is_listed_and_the_tui_follows() {
        let lines = lines(&config(vec![configured("amp")]), None, &healthy());
        assert_eq!(
            lines.get(1).map(String::as_str),
            Some("amp  0102030405060708  not connected")
        );
        assert_eq!(
            lines.last().map(String::as_str),
            Some("Next: `svolder tui`")
        );
    }

    /// The state comes from the daemon where one answered: the device it holds a link to reads
    /// connected, and one it has lost reads as the rest do.
    #[test]
    fn the_daemon_s_own_state_is_what_a_held_device_shows() {
        let config = config(vec![configured("amp"), configured("dac")]);
        let daemon = [held("amp", true), held("dac", false)];
        let lines = lines(&config, Some(&daemon), &healthy());
        assert!(lines[1].ends_with("  connected"), "{}", lines[1]);
        assert!(lines[2].ends_with("  not connected"), "{}", lines[2]);
    }

    /// A device the daemon is waiting to try again for says when it will, in the words
    /// `svolder daemon status` uses.
    #[test]
    fn a_device_the_daemon_is_waiting_on_says_when_it_tries_again() {
        let config = config(vec![configured("dac")]);
        let mut away = held("dac", false);
        away.retry_in_secs = Some(15);
        let lines = lines(&config, Some(&[away]), &healthy());
        assert_eq!(
            lines[1],
            "dac  0102030405060708  not connected, next attempt in 15 s"
        );
    }

    /// The first line is the host's health: what passed when nothing is wrong, and what went
    /// how, with the verb that says more, when something is.
    #[test]
    fn the_first_line_is_what_the_host_checks_said() {
        let lines = lines(&config(Vec::new()), None, &healthy());
        assert_eq!(
            lines.first().map(String::as_str),
            Some("host: 2 checks pass")
        );

        let troubled = vec![
            checked(State::Warn),
            checked(State::Warn),
            checked(State::Fail),
            checked(State::Pass),
        ];
        assert_eq!(
            health_line(&troubled),
            "host: 2 warnings, 1 failure — run svolder doctor"
        );
        assert_eq!(
            health_line(&[checked(State::Warn)]),
            "host: 1 warning — run svolder doctor"
        );
    }

    /// Every summary names the one verb that looks the host over, so a run that shows nothing
    /// working still says where to look.
    #[test]
    fn the_summary_points_at_the_host_check() {
        for devices in [Vec::new(), vec![configured("amp")]] {
            let lines = lines(&config(devices), None, &healthy());
            assert!(
                lines.iter().any(|line| line.contains("svolder doctor")),
                "{lines:?}"
            );
        }
    }
}
