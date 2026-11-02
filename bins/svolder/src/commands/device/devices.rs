// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The verbs about which devices there are: `scan` for the ones in range, `devices` for the
//! configured ones, and `add` and `remove`, which write the `[[device]]` blocks.

use std::process::ExitCode;
use std::time::Duration;

use svolder_ipc::{
    Client, Config, DeviceConfig, DeviceInfo, MAX_SCAN_WINDOW, Request, Response, ScanEntry, socket,
};
use svolder_proto::UnitId;

use crate::backend::{Failure, Globals};
use crate::recovery::Kind;
use crate::scanning;

use super::{config_failure, opened, reload_daemon};

/// List every device seen within `window`: one text line or one JSON object per device.
///
/// Under each text line stands the `add` command that configures that very device, indented
/// under it and ready to paste: a scan is what someone runs before they have configured
/// anything, and the udid is sixteen characters nobody should have to retype. The JSON output
/// is one object per device and nothing else, for whatever is reading it.
pub async fn scan(g: &Globals<'_>, window: Duration, json: bool) -> Result<ExitCode, String> {
    let mut backend = match opened(g, false).await {
        Ok(backend) => backend,
        Err(failure) => return Ok(failure.report()),
    };
    // A daemon listens for at most one window; say so rather than let a longer one look honoured.
    let window = if backend.through_daemon() && window > MAX_SCAN_WINDOW {
        eprintln!(
            "svolder: the daemon scans for at most {} s",
            MAX_SCAN_WINDOW.as_secs()
        );
        MAX_SCAN_WINDOW
    } else {
        window
    };
    let seen = match backend.scan(window).await {
        Ok(seen) => seen,
        Err(failure) => return Ok(failure.report()),
    };
    for entry in &seen {
        if json {
            println!(
                "{}",
                serde_json::to_string(entry).map_err(|e| e.to_string())?
            );
            continue;
        }
        for line in found_lines(entry) {
            println!("{line}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// One device heard as the two lines that describe it: what it is, and the command that
/// configures it.
///
/// The guided first run prints these too when it has nobody to ask, so a device reads the same
/// way whether it was found by `scan` or by `setup`.
pub(crate) fn found_lines(entry: &ScanEntry) -> [String; 2] {
    let name = entry.product.as_deref().unwrap_or("unknown");
    let pass = if entry.passphrase {
        "passphrase set"
    } else {
        "no passphrase"
    };
    // How loud a device is says which adapter to keep and how far away it is, so it is on
    // the line whenever the adapter reported it.
    let signal = match entry.rssi {
        Some(rssi) => format!("  {rssi} dBm"),
        None => String::new(),
    };
    [
        format!(
            "{}  {name}  fw {}  {pass}{signal}",
            entry.unit, entry.firmware
        ),
        format!("  {}", add_line(entry)),
    ]
}

/// The `add` command that configures the device this entry describes.
///
/// A device that advertised a product name is named after it, quoted because most product
/// names have a space in them; one that advertised none leaves the placeholder standing, since
/// `add` will not name a device for the user.
fn add_line(entry: &ScanEntry) -> String {
    match &entry.product {
        Some(product) => format!("svolder add {} --name {product:?}", entry.unit),
        None => format!("svolder add {} --name NAME", entry.unit),
    }
}

/// List the configured devices and their connection state, never opening a Bluetooth
/// transport itself.
///
/// Under `--transport auto` or `daemon`, with a daemon answering on the resolved socket, the
/// listing comes from `Request::Devices`. Under `ble` and `fake` no daemon is asked, and with
/// none answering the listing falls back to the configured devices, each reported as
/// `no daemon`; a configuration with none prints a hint instead.
pub async fn devices(g: &Globals<'_>, json: bool) -> Result<ExitCode, String> {
    if let Err(failure) = crate::backend::known_transport(g.transport) {
        return Ok(failure.report());
    }
    let config = Config::load().map_err(|e| e.to_string())?;
    crate::backend::print_warnings(&config);
    if crate::backend::asks_a_daemon(g.transport) {
        let path = socket::path::resolve(g.socket, config.socket.as_deref());
        if svolder_ipc::is_running(&path).await {
            crate::backend::daemon_note(g, &path);
            return devices_over_daemon(&path, json).await;
        }
    }
    if g.verbose {
        eprintln!("svolder: from the configuration");
    }
    devices_from_config(&config, json)
}

/// Ask the daemon for `Request::Devices` and print what it answers.
async fn devices_over_daemon(path: &std::path::Path, json: bool) -> Result<ExitCode, String> {
    let mut client = match Client::connect(path).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("svolder: {e}");
            return Ok(ExitCode::from(3));
        }
    };
    match client.request(Request::Devices).await {
        Ok(Response::Devices { devices }) => {
            if devices.is_empty() {
                eprintln!("svolder: {NO_DEVICES_HINT}");
                return Ok(ExitCode::SUCCESS);
            }
            for info in &devices {
                print_device_info(info, json)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Ok(Response::Error { code, message }) => {
            eprintln!("svolder: {message}");
            Ok(ExitCode::from(code))
        }
        Ok(other) => {
            eprintln!("svolder: the daemon sent an unexpected reply: {other:?}");
            Ok(ExitCode::from(3))
        }
        Err(e) => {
            eprintln!("svolder: {e}");
            Ok(ExitCode::from(3))
        }
    }
}

/// Print one `DeviceInfo` as text or JSON.
fn print_device_info(info: &DeviceInfo, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string(info).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX));
    println!("{}", device_info_line(info, now_ms));
    Ok(())
}

/// One device the daemon holds as the line `devices` prints: its name, what it is, its
/// firmware, its link — with when the daemon tries it again while it waits to — and its signal
/// strength with where it was heard and how long ago, as of `now_ms` (unix milliseconds); with no
/// clock to age it by, the strength is said without an age rather than with a false one.
pub(super) fn device_info_line(info: &DeviceInfo, now_ms: Option<u64>) -> String {
    let product = info.product.as_deref().unwrap_or("unknown");
    let state = match info.connected {
        true => "connected".to_string(),
        false => crate::format::link_down("disconnected", info.retry_in_secs),
    };
    let flags = crate::format::link_flags(info.stale, info.refused)
        .map(|flags| format!(" \u{b7} {flags}"))
        .unwrap_or_default();
    let signal = match (info.signal.as_ref(), now_ms) {
        (None, _) => "\u{2014}".to_string(),
        (Some(signal), Some(now_ms)) => signal.describe(now_ms, '-'),
        (Some(signal), None) => signal.describe_undated('-'),
    };
    format!(
        "{}  {product}  fw {}  {state}{flags}  signal {signal}",
        info.name, info.firmware
    )
}

/// What `devices` says when there is nothing to list, whichever path answered.
const NO_DEVICES_HINT: &str =
    "no devices configured; add one with `svolder add <udid> --name NAME`";

/// List the configured devices with `no daemon` as their state, or a hint when none are
/// configured.
fn devices_from_config(config: &Config, json: bool) -> Result<ExitCode, String> {
    if config.devices.is_empty() {
        eprintln!("svolder: {NO_DEVICES_HINT}");
        return Ok(ExitCode::SUCCESS);
    }
    for device in &config.devices {
        if json {
            let row = serde_json::json!({
                "name": device.name,
                "unit": device.udid,
                "state": "no daemon",
            });
            println!(
                "{}",
                serde_json::to_string(&row).map_err(|e| e.to_string())?
            );
        } else {
            println!("{}  {}  no daemon", device.name, device.udid);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Add a device to the configuration file, created from the template when there is none yet.
///
/// Without a udid there is nothing to add (the device has to be found first): on a terminal
/// this runs the guided scan, which finds the device and asks what to call it; off one it
/// names the verb that prints the command it wants. Whatever the command line said about the
/// device is what the wizard starts from, so a `--name` given here is the name it offers rather
/// than something it ignores. With a udid and no `--name`, the device names itself from one
/// scan window: the product it advertises is what it is configured as. The writer refuses a
/// name another device already carries, whichever of the two it came from, and re-parses the
/// file before it replaces it, so a configuration that would not load is never left behind.
pub async fn add(
    g: &Globals<'_>,
    udid: Option<&str>,
    name: Option<&str>,
    profile: Option<&str>,
    passphrase: &str,
) -> ExitCode {
    let Some(udid) = udid else {
        if crate::wizard::on_a_terminal() {
            let given = crate::wizard::Given {
                name: name.map(str::to_string),
                profile: profile.map(str::to_string),
                passphrase: Some(passphrase.to_string()),
            };
            return crate::wizard::adding(g, given).await;
        }
        return scan_first().report();
    };
    let unit = match udid.parse::<UnitId>() {
        Ok(unit) => unit,
        Err(e) => return Failure::of(Kind::Usage, e.to_string()).report(),
    };

    let path = Config::path();
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(e) => return config_failure(e).report(),
    };
    // The writer refuses a name that is taken; a unit id that is taken is this verb's to
    // refuse, since a second block for one device is two names for the same thing.
    if let Some(clash) = config
        .devices
        .iter()
        .find(|d| d.udid.parse::<UnitId>() == Ok(unit))
    {
        return Failure::of(
            Kind::Usage,
            format!("udid {udid:?} is already configured as {:?}", clash.name),
        )
        .report();
    }
    let name = match name_for(g, udid, unit, name).await {
        Ok(name) => name,
        Err(failure) => return failure.report(),
    };
    let device = DeviceConfig {
        name: name.clone(),
        udid: udid.to_string(),
        profile: profile.map(str::to_string),
        passphrase: passphrase.to_string(),
        inputs: std::collections::BTreeMap::new(),
    };
    match Config::add_device(&path, &device) {
        Ok(()) => {
            eprintln!("svolder: added {name} ({udid}) to {}", path.display());
            reload_daemon(g, &config).await;
            ExitCode::SUCCESS
        }
        Err(e) => config_failure(e).report(),
    }
}

/// What to call the device `unit` names: the name that was asked for, else the product it
/// advertises in the windows a search for it runs.
///
/// A device that is not in range names nothing, and neither does one whose advertisement
/// carries no product name, so both are answered by asking for a name — beside the verb that
/// would show what is there to be named. Anything else that stopped the search — no adapter,
/// a daemon that would not answer — is reported as itself, since a `--name` would not help.
async fn name_for(
    g: &Globals<'_>,
    udid: &str,
    unit: UnitId,
    name: Option<&str>,
) -> Result<String, Failure> {
    if let Some(name) = name {
        return Ok(name.to_string());
    }
    let heard = match scanning::find(g, Some(udid)).await {
        Ok(heard) => heard,
        Err(failure) if failure.code == Kind::NothingInRange.code() => {
            return Err(unnamed(udid));
        }
        Err(failure) => return Err(failure),
    };
    heard
        .iter()
        .find(|entry| entry.unit.parse::<UnitId>() == Ok(unit))
        .and_then(|entry| entry.product.clone())
        .ok_or_else(|| unnamed(udid))
}

/// What a device that named itself to nobody earns: the two ways to give it a name.
fn unnamed(udid: &str) -> Failure {
    Failure::of(
        Kind::Usage,
        format!("no name for {udid}; pass --name NAME, or run `svolder scan`"),
    )
}

/// What `add` with nothing to add answers: the verb that finds a device in range and prints
/// the `add` line for it.
fn scan_first() -> Failure {
    Failure {
        code: Kind::Usage.code(),
        what: "add needs the unit id of a device".to_string(),
        next: "Run `svolder scan` and use the `add` line it prints under the device".to_string(),
    }
}

/// Take the device named `name` out of the configuration file, leaving everything else in it
/// as it was.
///
/// A name no `[[device]]` block carries is the command line's mistake rather than the file's,
/// so it is answered with the verb that lists the names there are.
pub async fn remove(g: &Globals<'_>, name: &str) -> ExitCode {
    let path = Config::path();
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(e) => return config_failure(e).report(),
    };
    if !config
        .devices
        .iter()
        .any(|device| device.name.eq_ignore_ascii_case(name))
    {
        return Failure::of(
            Kind::Usage,
            format!("no device named {name:?} is configured; run `svolder devices` to list them"),
        )
        .report();
    }
    match Config::remove_device(&path, name) {
        Ok(()) => {
            eprintln!("svolder: removed {name} from {}", path.display());
            reload_daemon(g, &config).await;
            ExitCode::SUCCESS
        }
        Err(e) => config_failure(e).report(),
    }
}
