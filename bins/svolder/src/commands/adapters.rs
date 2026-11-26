// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `adapters`: the Bluetooth adapters this machine has, which one svolder uses, and how loudly
//! each hears the configured devices.
//!
//! The survey is also what the TUI's Bluetooth adapter list shows and what the pointer after a
//! failed connect is made from, so the three read the same facts.

use std::process::ExitCode;
use std::time::Duration;

use svolder_ble::adapters::{self, AdapterInfo, Hearing};
use svolder_ipc::{Config, DeviceConfig};

use crate::backend::{Failure, Globals};
use crate::recovery::Kind;

/// How long every adapter listens, when `--timeout` says nothing.
pub(crate) const WINDOW: Duration = Duration::from_secs(4);

/// How one adapter stands with one configured device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seen {
    /// The adapter did not hear it.
    NotHeard,
    /// The adapter heard it, at this signal when it said.
    Heard(Option<i16>),
    /// The adapter holds the link to it. A connected device stops advertising, so what is shown
    /// is the last strength known from before, and how long ago, where one is known.
    Connected {
        /// The last known signal in dBm.
        rssi: Option<i16>,
        /// How many seconds ago it was heard.
        age_s: Option<u64>,
    },
}

/// A device that is connected now, as far as the caller knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Held {
    /// The device's configured name.
    pub(crate) device: String,
    /// Its last known signal in dBm.
    pub(crate) rssi: Option<i16>,
    /// How many seconds ago that was heard.
    pub(crate) age_s: Option<u64>,
}

/// The devices of `rows` the daemon holds a link to, with the last signal each was heard at,
/// aged against `now_ms` (unix milliseconds).
pub(crate) fn held_from(rows: &[crate::backend::DeviceRow], now_ms: u64) -> Vec<Held> {
    rows.iter()
        .filter(|row| row.connected == Some(true))
        .map(|row| Held {
            device: row.name.clone(),
            rssi: row.signal.as_ref().map(|signal| signal.dbm),
            age_s: row
                .signal
                .as_ref()
                .map(|signal| now_ms.saturating_sub(signal.heard_at_ms) / 1000),
        })
        .collect()
}

/// One adapter and what it hears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// The adapter.
    pub(crate) adapter: AdapterInfo,
    /// Whether this is the one a session opens now.
    pub(crate) in_use: bool,
    /// How it stands with each configured device, in the config's order.
    pub(crate) heard: Vec<(String, Seen)>,
    /// Why it could not listen, when it could not.
    pub(crate) error: Option<String>,
}

/// The rows of `hearings` for the configured `devices`, `in_use` being the kernel name of the
/// adapter a session opens and `held` the devices connected through it.
pub(crate) fn rows(
    hearings: &[Hearing],
    devices: &[DeviceConfig],
    in_use: Option<&str>,
    held: &[Held],
) -> Vec<Row> {
    hearings
        .iter()
        .map(|hearing| {
            let used = in_use == Some(hearing.adapter.name.as_str());
            Row {
                adapter: hearing.adapter.clone(),
                in_use: used,
                heard: devices
                    .iter()
                    .map(|device| {
                        let holds = held
                            .iter()
                            .find(|held| used && held.device.eq_ignore_ascii_case(&device.name));
                        let seen = match (holds, hearing.signal_of(&device.udid)) {
                            (Some(held), _) => Seen::Connected {
                                rssi: held.rssi,
                                age_s: held.age_s,
                            },
                            (None, Some(rssi)) => Seen::Heard(rssi),
                            (None, None) => Seen::NotHeard,
                        };
                        (device.name.clone(), seen)
                    })
                    .collect(),
                error: hearing.heard.as_ref().err().cloned(),
            }
        })
        .collect()
}

/// The kernel name of the adapter a session opens, with what says so, from the first that
/// applies: `--adapter` or `SVOLDER_ADAPTER`, the adapter the state file remembers for the
/// device (a direct session opens that one), the configured `adapter`, else the first adapter.
/// Where the one named matches no adapter, the second answer says what is wrong with it.
pub(crate) fn in_use(
    named: Option<&str>,
    remembered: Option<&str>,
    configured: Option<&str>,
    present: &[AdapterInfo],
) -> (Option<String>, Option<String>) {
    let mut note = None;
    for (what, spec) in [
        ("--adapter", named),
        ("the remembered adapter", remembered),
        ("the configured adapter", configured),
    ] {
        let Some(spec) = spec else { continue };
        match adapters::resolve(spec, present) {
            Ok(name) => return (Some(name), note),
            Err(why) => {
                note.get_or_insert(format!("{what} matches no adapter: {why}"));
            }
        }
    }
    (present.first().map(|a| a.name.clone()), note)
}

/// [`in_use`] for the session `via` reaches the device: a direct session as [`in_use`] has it; a
/// daemon's by what the daemon says it holds, else by the config, never this process's
/// `--adapter` (a daemon ignores it).
pub(crate) fn in_use_via(
    via: &Via<'_>,
    named: Option<&str>,
    remembered: Option<&str>,
    configured: Option<&str>,
    present: &[AdapterInfo],
) -> (Option<String>, Option<String>) {
    match via {
        Via::Direct => in_use(named, remembered, configured, present),
        Via::Daemon { holds: Some(holds) } => (adapters::resolve(holds, present).ok(), None),
        Via::Daemon { holds: None } => in_use(None, None, configured, present),
    }
}

/// How the session whose adapter is marked in use reaches the device.
pub(crate) enum Via<'a> {
    /// This process opens the adapter itself: `--adapter`, the remembered adapter, the config.
    Direct,
    /// A daemon holds the device and opens the adapter its own command line or the config names.
    /// `holds` is what it says it holds, and `None` for a daemon that does not say, which leaves
    /// the config's answer.
    Daemon { holds: Option<&'a str> },
}

/// The adapters and what each hears, with a note when the adapter that was named is not there.
pub(crate) struct Survey {
    /// One row per adapter.
    pub(crate) rows: Vec<Row>,
    /// What is wrong with the adapter that was named, if anything.
    pub(crate) note: Option<String>,
}

/// Every adapter listening for `window`, at once, and what each heard.
///
/// Under `--transport fake` the adapters are `SVOLDER_FAKE_ADAPTERS`'s, each hearing every
/// configured device at the signal it names.
pub(crate) async fn survey(
    g: &Globals<'_>,
    config: &Config,
    window: Duration,
) -> Result<Vec<Hearing>, Failure> {
    if g.transport == "fake" {
        let units: Vec<String> = config.devices.iter().map(|d| d.udid.clone()).collect();
        let knob = std::env::var("SVOLDER_FAKE_ADAPTERS").ok();
        return adapters::fake_hearings(knob.as_deref(), &units)
            .map_err(|e| Failure::of(Kind::Usage, e));
    }
    let present = adapters::present()
        .await
        .map_err(|e| Failure::from_ble(&e))?;
    if present.is_empty() {
        return Err(Failure::of(
            Kind::NoAdapter,
            "no Bluetooth adapter found".to_string(),
        ));
    }
    Ok(adapters::hear_all(&present, window).await)
}

/// The rows for a survey: [`survey`], with the one a session opens marked and `held` shown as
/// connected through it.
pub(crate) async fn survey_rows(
    g: &Globals<'_>,
    config: &Config,
    window: Duration,
    held: &[Held],
    via: Via<'_>,
) -> Result<Survey, Failure> {
    let hearings = survey(g, config, window).await?;
    let present: Vec<AdapterInfo> = hearings.iter().map(|h| h.adapter.clone()).collect();
    // A direct session opens the adapter the state file remembers for its device; a daemon opens
    // the one its own command line or the config names, and says which, so neither this
    // process's `--adapter` nor the state file is asked under one.
    let remembered = match (g.transport, &via) {
        ("fake", _) | (_, Via::Daemon { .. }) => None,
        (_, Via::Direct) => config.devices.first().and_then(|device| {
            crate::state::DeviceState::load().last_adapter(&device.udid, config.adapter.as_deref())
        }),
    };
    let (used, note) = in_use_via(
        &via,
        g.adapter,
        remembered.as_deref(),
        config.adapter.as_deref(),
        &present,
    );
    Ok(Survey {
        rows: rows(&hearings, &config.devices, used.as_deref(), held),
        note,
    })
}

/// One adapter's standing with one device as it is written: `BF3 −62 dBm`, `BF3 —` when not
/// heard, `BF3 connected, −62 dBm 12 s ago` for the adapter that holds the link.
pub(crate) fn heard_word(device: &str, seen: Seen) -> String {
    match seen {
        Seen::NotHeard => format!("{device} —"),
        Seen::Heard(None) => format!("{device} heard"),
        Seen::Heard(Some(rssi)) => format!("{device} {}", adapters::dbm(rssi)),
        Seen::Connected { rssi, age_s } => match (rssi, age_s) {
            (Some(rssi), Some(age)) => {
                format!("{device} connected, {} {age} s ago", adapters::dbm(rssi))
            }
            (Some(rssi), None) => format!("{device} connected, {}", adapters::dbm(rssi)),
            _ => format!("{device} connected"),
        },
    }
}

/// What the row says of what it hears: every configured device, or why it could not listen.
pub(crate) fn heard_text(row: &Row) -> String {
    if let Some(why) = &row.error {
        return format!("cannot listen: {why}");
    }
    row.heard
        .iter()
        .map(|(device, seen)| heard_word(device, *seen))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The table, one line per adapter, its columns padded to line up.
pub(crate) fn text_lines(rows: &[Row]) -> Vec<String> {
    let widest = |of: &dyn Fn(&Row) -> usize| rows.iter().map(of).max().unwrap_or(0);
    let name = widest(&|r| r.adapter.name.chars().count());
    let address = widest(&|r| r.adapter.address.chars().count());
    let usb = widest(&|r| r.adapter.usb.as_deref().map_or(0, |u| u.chars().count()));
    let heard = widest(&|r| heard_text(r).chars().count());
    rows.iter()
        .map(|row| {
            let mark = if row.in_use { "✓ in use" } else { "" };
            format!(
                "{:name$}  {:address$}  {:usb$}  {:heard$}  {mark}",
                row.adapter.name,
                row.adapter.address,
                row.adapter.usb.as_deref().unwrap_or(""),
                heard_text(row),
            )
            .trim_end()
            .to_string()
        })
        .collect()
}

/// One adapter as a JSON object: its name, address, USB product, whether it is in use, whether it
/// could listen (`listened`, with the reason in `error` when it could not, which is not the same
/// as hearing nothing), and the devices it heard or holds: `device`, `rssi`, and for a device it
/// holds `connected: true` with the age of that last strength in `age_s`.
pub(crate) fn json_line(row: &Row) -> serde_json::Value {
    let heard: Vec<serde_json::Value> = row
        .heard
        .iter()
        .filter_map(|(device, seen)| match *seen {
            Seen::NotHeard => None,
            Seen::Heard(rssi) => Some(serde_json::json!({ "device": device, "rssi": rssi })),
            Seen::Connected { rssi, age_s } => Some(serde_json::json!({
                "device": device,
                "connected": true,
                "rssi": rssi,
                "age_s": age_s,
            })),
        })
        .collect();
    serde_json::json!({
        "name": row.adapter.name,
        "address": row.adapter.address,
        "usb": row.adapter.usb,
        "in_use": row.in_use,
        "listened": row.error.is_none(),
        "error": row.error,
        "heard": heard,
    })
}

/// Print every adapter, one text line or one JSON object each. Nothing is connected to and
/// nothing is written: every adapter only listens. A device a running daemon holds is shown
/// connected through the adapter in use.
pub(crate) async fn run(g: &Globals<'_>, window: Duration, json: bool) -> Result<ExitCode, String> {
    let config = match Config::load() {
        Ok(config) => config,
        Err(e) => return Err(e.to_string()),
    };
    let daemon_rows = crate::backend::Backend::devices_in(g, &config).await;
    let daemon = daemon_rows
        .as_ref()
        .is_ok_and(|rows| rows.iter().any(|row| row.connected.is_some()));
    let held = match daemon_rows {
        Ok(rows) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
            held_from(&rows, now)
        }
        Err(_) => Vec::new(),
    };
    let holds = match (daemon, crate::backend::launch::probe(g, &config).await) {
        (true, Some(socket)) => svolder_ipc::socket::daemon_version::version_within(
            &socket,
            svolder_ipc::socket::daemon_version::VERSION_WAIT,
        )
        .await
        .and_then(|info| info.adapter),
        _ => None,
    };
    let via = match daemon {
        true => Via::Daemon {
            holds: holds.as_deref(),
        },
        false => Via::Direct,
    };
    let survey = match survey_rows(g, &config, window, &held, via).await {
        Ok(survey) => survey,
        Err(failure) => return Ok(failure.report()),
    };
    if let Some(note) = &survey.note {
        eprintln!("svolder: {note}");
    }
    if json {
        for row in &survey.rows {
            println!("{}", json_line(row));
        }
    } else {
        for line in text_lines(&survey.rows) {
            println!("{line}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests;
