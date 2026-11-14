// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Configuration file model and loader for svolder.
//!
//! - `autoeq`: the writer of the `[autoeq]` table.
//! - `connections`: the writers of the `[mcp]`, `[mqtt]` and `[web]` tables.
//! - `devices`: the writers of the `[[device]]` blocks, `default_device` and `adapter`.
//! - `edit`: how a writer changes the file: read, edit, check, put back atomically.
//! - `flock`: an exclusive lock beside a file, for the writers.
//! - `homes`: the config, state and cache homes, the host EQ paths, the tests' scratch homes.
//! - `hops`: the `[[hop]]` blocks: what the sound goes through after the DAC, and their writers.
//! - `net_config`: the `[mqtt]`, `[mcp]` and `[web]` tables as the configuration holds them.
//! - `parse`: reading `config.toml` into a checked configuration.
//! - `presets`: the `[[preset]]` blocks: finding a preset and the writers that change one.
//! - `template`: the configuration template: every key commented out at its default.
//! - `tui`: the writer of the `[tui]` table.
//! - `tui_config`: the `[tui]` table as the configuration holds it, and the enums it takes.
//! - `tui_settings`: the plain keys of the `[tui]` table, one entry each.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use svolder_profiles::Family;
use svolder_profiles::tone::preset::{Preset, same_name};

pub use crate::error::ConfigError;

mod autoeq;
mod connections;
mod devices;
pub use devices::device_name_refusal;
mod edit;
mod flock;
mod homes;
mod hops;
mod net_config;
mod parse;
mod presets;
mod resolve;
mod template;
mod tui;
mod tui_config;
mod tui_settings;

pub use homes::{
    UNITDIR_ENV, cache_home, config_home, home_dir, host_eq_paths, host_eq_state_path,
    pin_scratch_homes, state_home,
};
pub use hops::{Hop, HopKind, HopWhat};
pub use net_config::{
    McpAuth, McpConfig, MqttConfig, REDACTED, WebAuth, WebConfig, WebGuest, normal_origin,
};
pub use parse::ListenDefaults;
pub use resolve::{passphrase_from, profile_for_advert};
pub use template::DEFAULT_SPECTRUM;
pub use tui_config::{
    ColorDepth, CoverArt, EffectsConfig, FamilyAccents, GraphSpan, IntroEffect, LinkSpan,
    Transition, TuiConfig,
};
pub use tui_settings::{
    COLORS, CONFIRM_QUIT, COVER_ART, GRAPH_SPAN, GRAPHS, HOST_AUDIO, INTRO, INTRO_EFFECT,
    INTRO_SOUND, LINK_SPAN, MONOCHROME, MOUSE, PALETTE, THEME_BACKGROUND, TITLE_METER, TOOLTIPS,
    TRANSITION, TUI_SETTINGS, TuiSetting, TuiSettingKind, TuiValue, VISUALIZER,
};

use parse::DEFAULT_PALETTE;

/// The parsed `config.toml`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Config {
    /// The config schema version. Only `1` is understood.
    pub schema: u32,
    /// The Bluetooth adapter to use, e.g. `"hci0"`.
    pub adapter: Option<String>,
    /// The device to act on when a command names none. May name no configured device; see
    /// `Config::dangling_default`.
    pub default_device: Option<String>,
    /// An explicit socket path, overriding the default resolution order. Never empty: a
    /// blank `socket` field is no override at all.
    pub socket: Option<String>,
    /// The configured devices, one per TOML `[[device]]` table.
    pub devices: Vec<DeviceConfig>,
    /// The user's named tone curves, one per TOML `[[preset]]` table, in file order. The
    /// built-in presets ([`svolder_profiles::tone::preset::builtins`]) are not stored here: they
    /// are not written to the file and are computed fresh from a device's profile.
    pub presets: Vec<Preset>,
    /// What the sound goes through after the DAC, one per TOML `[[hop]]` table, in file order.
    pub hops: Vec<Hop>,
    /// What a file that configures one unit twice was read as: the later blocks are not in
    /// [`Config::devices`], and each entry says which and why. They come out as warnings.
    #[serde(skip)]
    pub dropped_duplicates: Vec<String>,
    /// How a command reaches a daemon, from the TOML `[daemon]` table.
    pub daemon: DaemonConfig,
    /// How `svolder tui` looks and behaves, from the TOML `[tui]` table.
    pub tui: TuiConfig,
    /// Whether and how `svolderd` bridges its devices to an MQTT broker, from the TOML
    /// `[mqtt]` table.
    pub mqtt: MqttConfig,
    /// Whether and how `svolderd` serves the Model Context Protocol over HTTP, from the TOML
    /// `[mcp]` table.
    pub mcp: McpConfig,
    /// Whether and how `svolderd` serves its web UI, from the TOML `[web]` table.
    pub web: WebConfig,
    /// How `svolder` keeps its copy of the AutoEq index, from the TOML `[autoeq]` table.
    pub autoeq: AutoEqConfig,
    /// Whether `svolderd` watches the computer's music player and route, from the TOML
    /// `[host]` table.
    pub host: HostConfig,
}

/// The `[host]` table: whether the daemon reads the computer's player and the route its sound
/// takes to the DAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HostConfig {
    /// Whether the daemon reads the player and the route and tells its front ends.
    pub enabled: bool,
}

impl Default for HostConfig {
    /// The daemon watches the host.
    fn default() -> Self {
        HostConfig { enabled: true }
    }
}

/// The `[autoeq]` table: how often the cached AutoEq index is fetched again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct AutoEqConfig {
    /// `refresh_weeks`: an integer from 1 to 99, or the string `"never"`. Thirteen weeks
    /// while the key is absent.
    pub refresh_weeks: RefreshWeeks,
}

/// The age at which the cached AutoEq index is fetched again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshWeeks {
    /// Never on its own: only `--refresh` (or the TUI's refresh key) fetches it again.
    Never,
    /// After this many weeks, from 1 to 99.
    Weeks(u8),
}

impl Default for RefreshWeeks {
    /// Thirteen weeks.
    fn default() -> Self {
        RefreshWeeks::Weeks(13)
    }
}

impl serde::Serialize for RefreshWeeks {
    /// The integer, or the word `"never"`, as the file writes it.
    fn serialize<S: serde::Serializer>(&self, out: S) -> Result<S::Ok, S::Error> {
        match self {
            RefreshWeeks::Never => out.serialize_str("never"),
            RefreshWeeks::Weeks(weeks) => out.serialize_u8(*weeks),
        }
    }
}

/// The `[daemon]` table: how a command that needs a daemon gets one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    /// Whether a command that finds no daemon listening starts one of its own.
    pub autospawn: bool,
}

impl Default for DaemonConfig {
    /// A command starts a daemon of its own when it finds none listening.
    fn default() -> Self {
        DaemonConfig { autospawn: true }
    }
}

/// Whether this process may reach `path` as `mode` asks (`access(2)`, the real user's rights).
fn can_access(path: &Path, mode: libc::c_int) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(text) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `text` is a NUL-terminated string that outlives the call; `access` only reads it.
    unsafe { libc::access(text.as_ptr(), mode) == 0 }
}

/// One `[[device]]` entry.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DeviceConfig {
    /// The device's name, used to select it from a front end.
    pub name: String,
    /// The device's 8-byte unit id, as sixteen hex characters.
    pub udid: String,
    /// The profile to use for this device, when it differs from the detected one.
    pub profile: Option<String>,
    /// `"auto"`, `"none"`, or four digits. Defaults to `"auto"`.
    pub passphrase: String,
    /// Input names by their numeric key, from the TOML `[device.inputs]` table.
    pub inputs: BTreeMap<u8, String>,
}

impl Default for Config {
    /// `schema: 1` and every other field empty, matching an absent or empty config file.
    fn default() -> Self {
        Config {
            schema: 1,
            adapter: None,
            default_device: None,
            socket: None,
            devices: Vec::new(),
            presets: Vec::new(),
            hops: Vec::new(),
            dropped_duplicates: Vec::new(),
            daemon: DaemonConfig::default(),
            tui: TuiConfig::default(),
            mqtt: MqttConfig::default(),
            mcp: McpConfig::default(),
            web: WebConfig::default(),
            autoeq: AutoEqConfig::default(),
            host: HostConfig::default(),
        }
    }
}

/// The sentence for a `default_device` naming no configured device, as a warning when the
/// config loads and as the error of a verb that needs the default.
pub fn dangling_default_message(name: &str) -> String {
    format!("default_device {name:?} names no configured device")
}

impl Config {
    /// `$XDG_CONFIG_HOME/svolder/config.toml` or `~/.config/svolder/config.toml`, with the
    /// account's home directory standing in for an unset `$HOME`.
    pub fn path() -> PathBuf {
        config_home().join("svolder").join("config.toml")
    }

    /// The `default_device` name when it names no configured device (case aside), else `None`.
    pub(crate) fn dangling_default(&self) -> Option<&str> {
        let name = self.default_device.as_deref()?;
        let known = self
            .devices
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(name));
        (!known).then_some(name)
    }

    /// The failure of a verb that needs the default when `Config::dangling_default` names
    /// something: the plain sentence, or, when the name is a block that was left out as a
    /// duplicate of another for its unit, that line with its cure.
    pub fn dangling_default_failure(&self) -> Option<String> {
        let name = self.dangling_default()?;
        let left_out = format!("{name:?} is left out").to_ascii_lowercase();
        Some(
            match self
                .dropped_duplicates
                .iter()
                .find(|line| line.to_ascii_lowercase().contains(&left_out))
            {
                Some(line) => format!("{}: {line}", dangling_default_message(name)),
                None => dangling_default_message(name),
            },
        )
    }

    /// Whether a `[[device]]` block is named `name`, matching case and the blanks round it
    /// aside ([`same_name`]).
    fn configures(&self, name: &str) -> bool {
        self.devices
            .iter()
            .any(|device| same_name(&device.name, name))
    }

    /// Whether this process can write the configuration at `path` as the writers do, or why
    /// not in words: the directory it sits in (the nearest that exists, for a file whose
    /// directory is still to be made) must be writable, since the atomic rename and the lock
    /// file go there; and a file already there must be writable by this user and keep its
    /// owner's write bit, since a file made read-only (`chmod 444`) is read-only on purpose even
    /// to root, whom `access` alone lets through. A missing file is writable where its
    /// directory is.
    pub fn writable(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let mut dir = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        while !dir.exists() {
            match dir.parent().filter(|up| !up.as_os_str().is_empty()) {
                Some(up) => dir = up,
                None => break,
            }
        }
        if !can_access(dir, libc::W_OK | libc::X_OK) {
            return Err(format!("{}: its directory is not writable", path.display()));
        }
        match std::fs::metadata(path) {
            Ok(meta) if meta.permissions().mode() & 0o200 == 0 => Err(format!(
                "{} is read-only ({:04o})",
                path.display(),
                meta.permissions().mode() & 0o777
            )),
            Ok(_) if !can_access(path, libc::W_OK) => {
                Err(format!("{} is not writable by this user", path.display()))
            }
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// What a reader of this config should be told once, one line each and without any prefix:
    /// a `default_device` that names no configured device, a hop that links or follows a device
    /// no `[[device]]` block configures, a web guest setting that cannot
    /// apply or that a wildcard listen leaves open to DNS rebinding, and a web UI and an MCP
    /// server that would listen on one port.
    pub fn warnings(&self) -> Vec<String> {
        let mut said: Vec<String> = self
            .dangling_default()
            .map(dangling_default_message)
            .into_iter()
            .collect();
        said.extend(self.dropped_duplicates.iter().cloned());
        for hop in &self.hops {
            if let HopWhat::Linked { device } = &hop.what
                && !self.configures(device)
            {
                said.push(format!(
                    "[[hop]] device {device:?} names no configured device"
                ));
            }
            if let Some(after) = hop.after.as_deref().filter(|after| !self.configures(after)) {
                said.push(format!(
                    "[[hop]] {:?}: after {after:?} names no configured device",
                    hop.id()
                ));
            }
        }
        let web = &self.web;
        if web.enabled && web.guest == WebGuest::Read && web.auth == WebAuth::None {
            said.push(
                "web.guest = \"read\" has no effect with web.auth = \"none\": every visitor is \
                 signed in"
                    .to_string(),
            );
        }
        if web.enabled
            && web.guest == WebGuest::Read
            && web.auth != WebAuth::None
            && web.listen.ip().is_unspecified()
            && web.hosts.is_empty()
        {
            said.push(
                "web.guest = \"read\" on a wildcard listen with no web.hosts answers any Host \
                 name: a page on the web can read the view through DNS rebinding; list the names \
                 in web.hosts"
                    .to_string(),
            );
        }
        let (a, b) = (web.listen, self.mcp.listen);
        let overlap = a.ip() == b.ip() || a.ip().is_unspecified() || b.ip().is_unspecified();
        if web.enabled && self.mcp.enabled && a.port() == b.port() && overlap {
            said.push(format!(
                "[web] listen {a} and [mcp] listen {b} share port {}; the second to start \
                 cannot listen",
                a.port()
            ));
        }
        said
    }

    /// Load and validate the file at [`Config::path`]; `Ok(Config::default())` when it is absent.
    pub fn load() -> Result<Config, ConfigError> {
        Self::load_from(&Self::path())
    }

    /// Load and validate the file at `path`. An absent file reads as an empty one: every key
    /// at its default, those the environment names ([`ListenDefaults`]) included.
    pub fn load_from(path: &Path) -> Result<Config, ConfigError> {
        Self::load_from_with(path, &ListenDefaults::from_env())
    }

    /// [`Config::load_from`] with the environment's defaults given rather than read.
    pub fn load_from_with(path: &Path, env: &ListenDefaults) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse_with_listen_defaults(&text, env),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Self::parse_with_listen_defaults("", env)
            }
            Err(e) => Err(ConfigError::Io(path.to_path_buf(), e)),
        }
    }
}

/// The refusal for a `kind` block (`device`, `hop` or `preset`) no block of the file carries.
fn missing(kind: &'static str, name: &str) -> ConfigError {
    ConfigError::Missing {
        kind,
        name: name.to_string(),
    }
}

/// The refusal for a block a writer found and then no longer holds.
fn vanished(kind: &'static str, name: &str) -> ConfigError {
    ConfigError::Vanished {
        kind,
        name: name.to_string(),
    }
}

#[cfg(test)]
mod tests;
