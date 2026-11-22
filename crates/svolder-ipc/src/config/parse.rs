// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading `config.toml`: the shapes the file is deserialized into, [`Config::parse`] that
//! turns them into a checked configuration, and the checks each key and `[[preset]]` block
//! must pass. The `[mqtt]`, `[mcp]` and `[web]` tables are read in `net`, the `[tui]` table in
//! `tui_table`, and the `[[hop]]` and `[[preset]]` blocks in `blocks`.

use std::collections::BTreeMap;

use super::scenes::read_scenes;
use super::{
    AutoEqConfig, Config, ConfigError, DaemonConfig, DeviceConfig, HostConfig, RefreshWeeks,
};

mod blocks;
mod net;
mod tui_table;

use blocks::{hops, presets};
pub use net::ListenDefaults;
use net::{RawMcp, RawMqtt, RawWeb, mcp, mqtt, web};
#[cfg(test)]
pub(super) use tui_table::EFFECT_KEYS;
pub(super) use tui_table::{DEFAULT_PALETTE, PALETTES};
use tui_table::{RawTui, tui};

/// The on-disk shape of `config.toml`, before validation.
#[derive(serde::Deserialize)]
struct RawConfig {
    #[serde(default = "one")]
    schema: u32,
    adapter: Option<String>,
    default_device: Option<String>,
    socket: Option<String>,
    #[serde(default, rename = "device")]
    devices: Vec<RawDevice>,
    // Kept as raw values, not a typed struct: a bad `[[preset]]` block is validated by
    // `parse_preset`, which names the preset in every error, including what a derived
    // `Deserialize` would only report by line and column (an unknown key, a non-finite
    // number) or would accept outright (a `loudness` or `bands` out of range).
    #[serde(default, rename = "preset")]
    presets: Vec<toml::Value>,
    // Raw for the same reason: `read_scenes` names the scene in every error.
    #[serde(default, rename = "scene")]
    scenes: Vec<toml::Value>,
    // Raw for the same reason: `parse_hop` names the hop in every error.
    #[serde(default, rename = "hop")]
    hops: Vec<toml::Value>,
    #[serde(default)]
    daemon: DaemonConfig,
    #[serde(default)]
    tui: RawTui,
    #[serde(default)]
    mqtt: RawMqtt,
    #[serde(default)]
    mcp: RawMcp,
    #[serde(default)]
    web: RawWeb,
    #[serde(default)]
    autoeq: RawAutoEq,
    #[serde(default)]
    host: HostConfig,
}

/// The on-disk shape of the `[autoeq]` table, before validation. `refresh_weeks` stays a raw
/// value: an integer or the word `"never"`, checked in `Config::parse`.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct RawAutoEq {
    refresh_weeks: Option<toml::Value>,
}

/// `refresh_weeks` as the setting: 1 to 99, or `never` in any case.
fn parse_refresh_weeks(value: Option<&toml::Value>) -> Result<RefreshWeeks, ConfigError> {
    let wrong = || {
        ConfigError::Parse(
            "autoeq.refresh_weeks must be a whole number from 1 to 99, or \"never\"".to_string(),
        )
    };
    match value {
        None => Ok(RefreshWeeks::default()),
        Some(toml::Value::String(word)) if word.trim().eq_ignore_ascii_case("never") => {
            Ok(RefreshWeeks::Never)
        }
        Some(toml::Value::Integer(weeks)) => u8::try_from(*weeks)
            .ok()
            .filter(|weeks| (1..=99).contains(weeks))
            .map(RefreshWeeks::Weeks)
            .ok_or_else(wrong),
        Some(_) => Err(wrong()),
    }
}

/// The on-disk shape of one `[[device]]` entry, before validation.
#[derive(serde::Deserialize)]
struct RawDevice {
    name: String,
    udid: String,
    profile: Option<String>,
    #[serde(default = "auto")]
    passphrase: String,
    // TOML table keys deserialize as strings; each key is parsed to a `u8` in `devices`.
    #[serde(default)]
    inputs: BTreeMap<String, String>,
}

fn one() -> u32 {
    1
}

fn auto() -> String {
    "auto".into()
}

/// Whether an optional key names anything: a key left empty overrides nothing, so the config
/// template can show every optional key at the value it has while absent.
fn is_named(value: &str) -> bool {
    !value.trim().is_empty()
}

impl Config {
    /// Parse from text (the seam every test uses), with the listen defaults the environment
    /// names.
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        Self::parse_with_listen_defaults(text, &ListenDefaults::from_env())
    }

    /// [`Config::parse`] with the listen defaults given rather than read from the environment.
    pub fn parse_with_listen_defaults(
        text: &str,
        listen: &ListenDefaults,
    ) -> Result<Config, ConfigError> {
        Self::parse_as(text, false, listen)
    }

    /// [`Config::parse`] that also refuses what the daemon only warns about: two `[[device]]`
    /// blocks for one unit. The writers check what they are about to write with this, so a
    /// file never grows a duplicate.
    pub fn parse_strict(text: &str) -> Result<Config, ConfigError> {
        Self::parse_as(text, true, &ListenDefaults::from_env())
    }

    /// The parse, strict about a unit configured twice or not: a lenient one keeps the first
    /// block for the unit, leaves the later ones out of [`Config::devices`] and says so in
    /// [`Config::warnings`], so a file written before the rule existed still starts the daemon.
    fn parse_as(text: &str, strict: bool, listen: &ListenDefaults) -> Result<Config, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| toml_error(&e, text))?;
        if raw.schema != 1 {
            return Err(ConfigError::Schema(raw.schema));
        }
        let (devices, dropped_duplicates) = devices(raw.devices, strict)?;
        let presets = presets(&raw.presets)?;
        let scenes = read_scenes(&raw.scenes)?;
        let hops = hops(&raw.hops)?;
        // A blank value overrides nothing: every optional key means "as if the key were
        // absent" when it is left empty, which is the value the config template shows each of
        // them at.
        let adapter = raw.adapter.filter(|value| is_named(value));
        let default_device = raw.default_device.filter(|value| is_named(value));
        let socket = raw.socket.filter(|value| is_named(value));
        let tui = tui(raw.tui)?;
        let mqtt = mqtt(raw.mqtt, listen)?;
        let mcp = mcp(raw.mcp, listen)?;
        let web = web(raw.web, listen)?;
        let autoeq = AutoEqConfig {
            refresh_weeks: parse_refresh_weeks(raw.autoeq.refresh_weeks.as_ref())?,
        };
        Ok(Config {
            schema: raw.schema,
            autoeq,
            adapter,
            default_device,
            socket,
            devices,
            presets,
            scenes,
            hops,
            dropped_duplicates,
            daemon: raw.daemon,
            tui,
            mqtt,
            mcp,
            web,
            host: raw.host,
        })
    }
}

/// The `[[device]]` entries: each udid, passphrase and input key checked.
///
/// A unit another block already has is refused when `strict`; otherwise the later block is left
/// out and the words that say so come back beside the devices.
fn devices(
    raw: Vec<RawDevice>,
    strict: bool,
) -> Result<(Vec<DeviceConfig>, Vec<String>), ConfigError> {
    let mut devices = Vec::with_capacity(raw.len());
    let mut dropped = Vec::new();
    for d in raw {
        // Validate the udid is a parseable 8-byte unit id, and that no earlier block has it.
        let unit = d
            .udid
            .parse::<svolder_proto::UnitId>()
            .map_err(|_| ConfigError::Parse(format!("device {}: bad udid {:?}", d.name, d.udid)))?;
        if let Some(first) = Config::device_for_unit_in(&devices, &unit) {
            if strict {
                return Err(ConfigError::UnitTwice {
                    first: first.name.clone(),
                    second: d.name,
                    udid: d.udid,
                });
            }
            dropped.push(unit_dropped(&first.name, &d.name, &d.udid));
            continue;
        }
        validate_passphrase(&d.name, &d.passphrase)?;
        let mut inputs = BTreeMap::new();
        for (raw_key, value) in d.inputs {
            let key = raw_key.parse::<u8>().map_err(|_| {
                ConfigError::Parse(format!(
                    "device {}: input key {raw_key:?} is not a number 0-255",
                    d.name
                ))
            })?;
            inputs.insert(key, value);
        }
        devices.push(DeviceConfig {
            name: d.name,
            udid: d.udid,
            profile: d.profile,
            passphrase: d.passphrase,
            inputs,
        });
    }
    Ok((devices, dropped))
}

/// What the daemon and the commands warn with when they load a file that has a unit twice.
fn unit_dropped(first: &str, second: &str, udid: &str) -> String {
    format!(
        "devices {first:?} and {second:?} are both configured for unit {udid}; \
         {second:?} is left out and {first:?} is used. Run `svolder remove {second}` or edit \
         the file to take one of the two blocks out"
    )
}

/// A passphrase field is `auto`, `none`, or four digits.
fn validate_passphrase(name: &str, p: &str) -> Result<(), ConfigError> {
    let ok = p == "auto" || p == "none" || (p.len() == 4 && p.bytes().all(|b| b.is_ascii_digit()));
    if ok {
        Ok(())
    } else {
        Err(ConfigError::Parse(format!(
            "device {name}: passphrase must be auto, none, or four digits"
        )))
    }
}

/// The refusal for a file the TOML parser rejected: its message and where it stopped, without
/// the lines of the file that the parser's own text quotes (a quoted line may be the passphrase)
/// and without the value that did not fit.
pub(super) fn toml_error(e: &toml::de::Error, text: &str) -> ConfigError {
    let message = crate::error::without_value(e.message());
    ConfigError::Parse(match e.span() {
        Some(span) => {
            let (line, column) = crate::error::line_and_column(text, span.start);
            format!("TOML parse error at line {line}, column {column}: {message}")
        }
        None => format!("TOML parse error: {message}"),
    })
}
