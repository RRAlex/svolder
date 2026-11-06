// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Configuration file model and loader for svolder.
//!
//! - `autoeq`: the writer of the `[autoeq]` table.
//! - `connections`: the writers of the `[mcp]`, `[mqtt]` and `[web]` tables.
//! - `devices`: the writers of the `[[device]]` blocks, `default_device` and `adapter`.
//! - `edit`: how a writer changes the file: read, edit, check, put back atomically.
//! - `flock`: an exclusive lock beside a file, for the writers.
//! - `hops`: the `[[hop]]` blocks: what the sound goes through after the DAC, and their writers.
//! - `parse`: reading `config.toml` into a checked configuration.
//! - `presets`: the `[[preset]]` blocks: finding a preset and the writers that change one.
//! - `template`: the configuration template: every key commented out at its default.
//! - `tui`: the writer of the `[tui]` table.
//! - `tui_settings`: the plain keys of the `[tui]` table, one entry each.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use svolder_profiles::Family;
use svolder_profiles::tone::preset::{Preset, same_name};

pub use crate::error::ConfigError;

mod autoeq;
mod connections;
mod devices;
mod edit;
mod flock;
mod hops;
mod parse;
mod presets;
mod template;
mod tui;
mod tui_settings;

pub use hops::{Hop, HopKind, HopWhat};
pub use template::DEFAULT_SPECTRUM;
pub use tui_settings::{
    ACCENT, COLORS, CONFIRM_QUIT, GRAPH_SPAN, GRAPHS, HOST_AUDIO, INTRO, INTRO_EFFECT, INTRO_SOUND,
    LINK_SPAN, MONOCHROME, MOUSE, PALETTE, THEME_BACKGROUND, TITLE_METER, TRANSITION, TUI_SETTINGS,
    TuiSetting, TuiSettingKind, TuiValue, VISUALIZER,
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
    /// [`Config::dangling_default`].
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

/// The `[mqtt]` table: whether and how `svolderd` bridges its devices to an MQTT broker for
/// Home Assistant. Absent, or present with `enabled = false`, opens no socket at all.
///
/// The password never leaves this struct as itself: serialized — `svolder config show
/// --json` — and debug-printed it reads `***`.
#[derive(Clone, PartialEq, serde::Serialize)]
pub struct MqttConfig {
    /// Whether the bridge runs at all.
    pub enabled: bool,
    /// The broker's host. Required when `enabled`.
    pub host: Option<String>,
    /// The broker's port.
    pub port: u16,
    /// The broker username, when it wants one.
    pub username: Option<String>,
    /// The broker password, when it wants one. `SVOLDER_MQTT_PASSWORD` overrides this when
    /// `svolderd` connects, so a password need not sit in the file at all.
    #[serde(serialize_with = "redacted")]
    pub password: Option<String>,
    /// The client id to connect with. `None` defaults to `"svolder-"`, the host name and a
    /// short hash of the daemon's socket path, resolved where the connection is made.
    pub client_id: Option<String>,
    /// The topic prefix every MQTT topic is built under.
    pub base_topic: String,
    /// The discovery prefix Home Assistant watches for `<prefix>/.../config`. Empty turns
    /// discovery off entirely.
    pub discovery_prefix: String,
    /// Whether the bridge subscribes to the command topics and writes what arrives there
    /// through the daemon's existing set path.
    pub commands: bool,
    /// An optional cap, in dB, on a `volume` command's value.
    pub max_volume_db: Option<f64>,
}

/// What a password is shown as wherever the configuration is printed.
pub const REDACTED: &str = "***";

/// A password serialized as [`REDACTED`] when there is one, and as nothing when there is none.
fn redacted<S: serde::Serializer>(value: &Option<String>, s: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(_) => s.serialize_some(REDACTED),
        None => s.serialize_none(),
    }
}

impl std::fmt::Debug for MqttConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MqttConfig")
            .field("enabled", &self.enabled)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| REDACTED))
            .field("client_id", &self.client_id)
            .field("base_topic", &self.base_topic)
            .field("discovery_prefix", &self.discovery_prefix)
            .field("commands", &self.commands)
            .field("max_volume_db", &self.max_volume_db)
            .finish()
    }
}

impl Default for MqttConfig {
    /// Off, at 1883 with no credentials, under the `svolder` topic prefix and Home
    /// Assistant's own discovery prefix, read-only, with no cap.
    fn default() -> Self {
        MqttConfig {
            enabled: false,
            host: None,
            port: 1883,
            username: None,
            password: None,
            client_id: None,
            base_topic: "svolder".to_string(),
            discovery_prefix: "homeassistant".to_string(),
            commands: false,
            max_volume_db: None,
        }
    }
}

/// How a request to the MCP server proves who sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpAuth {
    /// A bearer token, from `SVOLDER_MCP_TOKEN`, `token_file` or the generated token file.
    Token,
    /// No check at all; accepted only when the server listens on a loopback address.
    None,
}

/// The `[mcp]` table: whether and how `svolderd` serves the Model Context Protocol over HTTP.
/// Absent, or present with `enabled = false`, opens no socket at all.
///
/// The token is not part of the configuration: it lives in a file (`token_file`, or the one
/// svolder generates in its state directory) or in `SVOLDER_MCP_TOKEN`, so nothing here is a
/// secret.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct McpConfig {
    /// Whether the server runs at all.
    pub enabled: bool,
    /// The address the server listens on.
    pub listen: std::net::SocketAddr,
    /// How a request proves who sent it.
    pub auth: McpAuth,
    /// Whether the tools that change a device are offered.
    pub writes: bool,
    /// An optional cap, in dB, on a volume the server will set.
    pub max_volume_db: Option<f64>,
    /// The browser origins allowed to call the server; empty allows none.
    pub origins: Vec<String>,
    /// Host names a client may reach the server by, such as `mcp.home.lan`, added to the
    /// address the server listens on and the loopback names. An entry without a port matches
    /// any port; one with a port matches that port only. Empty adds none.
    pub hosts: Vec<String>,
    /// A file holding the token, in place of the generated one. `SVOLDER_MCP_TOKEN` outranks it.
    pub token_file: Option<String>,
}

impl Default for McpConfig {
    /// Off, listening on loopback port 8737, token-authenticated, read-only, with no cap, no
    /// browser origin and no token file.
    fn default() -> Self {
        McpConfig {
            enabled: false,
            listen: std::net::SocketAddr::from(([127, 0, 0, 1], 8737)),
            auth: McpAuth::Token,
            writes: false,
            max_volume_db: None,
            origins: Vec::new(),
            hosts: Vec::new(),
            token_file: None,
        }
    }
}

/// How a browser proves to the web UI who it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebAuth {
    /// A sign-in: the token, a pairing code or a link code, then a cookie.
    Token,
    /// No check at all; accepted only when the page listens on a loopback address.
    None,
}

/// What a visitor without a sign-in gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebGuest {
    /// The sign-in page, and 401 from the API.
    None,
    /// The device screens, read-only.
    Read,
}

/// The `[web]` table: whether and how `svolderd` serves its web UI. Absent, or present with
/// `enabled = false`, opens no socket at all. Like `[mcp]`, nothing here is a secret: the token
/// lives in a file or in `SVOLDER_WEB_TOKEN`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WebConfig {
    /// Whether the daemon serves the web UI.
    pub enabled: bool,
    /// The address it listens on.
    pub listen: std::net::SocketAddr,
    /// How a browser proves who it is.
    pub auth: WebAuth,
    /// Whether the page may change a device; off makes a read-only dashboard.
    pub writes: bool,
    /// An optional cap, in dB, on a volume the page sets.
    pub max_volume_db: Option<f64>,
    /// What a visitor without a sign-in gets.
    pub guest: WebGuest,
    /// The address links and QR codes carry, ending in `/`; worked out from `listen` while
    /// `None`.
    pub url: Option<String>,
    /// Host names a client may reach the page by, added to the listen address and the loopback
    /// names, as `[mcp] hosts`.
    pub hosts: Vec<String>,
    /// Origins a reverse proxy presents, besides the page's own, as [`normal_origin`] writes
    /// them.
    pub origins: Vec<String>,
    /// A file holding the token, in place of the generated one. `SVOLDER_WEB_TOKEN` outranks it.
    pub token_file: Option<String>,
}

impl Default for WebConfig {
    /// Off, listening on loopback port 8738, signed in by token, with writes on, no cap, no
    /// guest, no url, no extra hosts or origins, and no token file.
    fn default() -> Self {
        WebConfig {
            enabled: false,
            listen: std::net::SocketAddr::from(([127, 0, 0, 1], 8738)),
            auth: WebAuth::Token,
            writes: true,
            max_volume_db: None,
            guest: WebGuest::None,
            url: None,
            hosts: Vec::new(),
            origins: Vec::new(),
            token_file: None,
        }
    }
}

/// `text` as an origin compares: `http` or `https` in lower case, `://`, the host in lower case
/// (an IPv6 address in brackets), and `:port` only where it is not the scheme's own (80, 443).
/// `None` for anything else: another scheme, no host, user info, a path, a query, a fragment,
/// or a port that is not a number. A trailing `/` is allowed and dropped.
pub fn normal_origin(text: &str) -> Option<String> {
    let (scheme, rest) = text.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let default = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    if authority.is_empty() || authority.contains(['/', '?', '#', '@', ' ', '\t']) {
        return None;
    }
    let (host, port) = match authority.strip_prefix('[') {
        Some(v6) => {
            let (inner, after) = v6.split_once(']')?;
            let port = match after {
                "" => None,
                after => Some(after.strip_prefix(':')?.parse::<u16>().ok()?),
            };
            (format!("[{inner}]"), port)
        }
        None => match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), Some(port.parse::<u16>().ok()?)),
            None => (authority.to_string(), None),
        },
    };
    if host.is_empty() || host == "[]" {
        return None;
    }
    let host = host.to_ascii_lowercase();
    Some(match port {
        Some(port) if port != default => format!("{scheme}://{host}:{port}"),
        _ => format!("{scheme}://{host}"),
    })
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

/// How `svolder tui` looks and behaves.
///
/// Every field mirrors a key under `[tui]` in `config.toml`, with the defaults shown there.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TuiConfig {
    /// One accent colour for every device family, overriding the palette's three: one of the
    /// 16 ANSI colour names (its `bright-` form too) or a `#rrggbb` value. `None` when the
    /// file names none, leaving the palette's own accents in place and the key out of
    /// anything written from this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// The palette preset the screens draw from: `amber`, `ice`, `red` or `neon`.
    pub palette: String,
    /// Accent overrides for one device family at a time, which `accent` overrides in turn.
    pub accents: FamilyAccents,
    /// The stops the glow travels, cool end to hot end, from `stops` under `[tui.spectrum]`,
    /// each a `#rrggbb` value. Empty when the file names none, leaving the palette's own stops
    /// in place and the key out of anything written from this.
    ///
    /// Serialises as the table the file carries it in — `"spectrum": { "stops": [...] }` —
    /// rather than the bare array it is held as, so a rendered configuration keeps the file's
    /// shape, the way `accents` does.
    #[serde(skip_serializing_if = "Vec::is_empty", serialize_with = "stops_table")]
    pub spectrum: Vec<String>,
    /// The colour depth to render at.
    pub colors: ColorDepth,
    /// Force monochrome rendering (selection by reverse video only), regardless of `colors`.
    pub monochrome: bool,
    /// Whether each palette paints its own dark greys under the screen, its panes and its boxes,
    /// at true and 256 colours; off leaves the terminal's own background showing.
    pub theme_background: bool,
    /// Whether mouse capture starts on.
    pub mouse: bool,
    /// Whether the wordmark intro plays while the device opens.
    pub intro: bool,
    /// Whether a short sound plays with the wordmark intro, through PipeWire's `pw-play`.
    pub intro_sound: bool,
    /// Which effect builds the wordmark intro.
    pub intro_effect: IntroEffect,
    /// How one screen gives way to the next.
    pub transition: Transition,
    /// Whether the terminal's title carries one bar per channel of the device's meters while
    /// it streams them, else of what the host plays while its capture runs.
    pub title_meter: bool,
    /// Whether `q` asks before `svolder tui` quits.
    pub confirm_quit: bool,
    /// Whether `svolder tui` reaches the host's own audio: what its media player plays, the
    /// volume of its default sink and the player's transport. `SVOLDER_HOST=off` in the
    /// environment turns it off for one process whatever this says.
    pub host_audio: bool,
    /// Whether the device screen draws a spectrum of what the host's sink plays, while host
    /// audio is on.
    pub visualizer: bool,
    /// Whether the screens draw their graphs: the Telemetry and level history, and the
    /// connection strip. Off draws every screen without them.
    pub graphs: bool,
    /// How much history the Telemetry and level graphs span.
    pub graph_span: GraphSpan,
    /// How much history the connection strip spans.
    pub link_span: LinkSpan,
    /// Which of the animations run: `effects = true|false` for all of them at once, or an
    /// `effects = { ... }` table naming them one at a time.
    pub effects: EffectsConfig,
}

/// How `svolder tui` draws one screen giving way to the next, from `transition` under `[tui]`.
///
/// Every one of them but [`Transition::None`] is part of the animation layer, so `effects`
/// switched off plays none of them whatever this says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Transition {
    /// The new screen is simply there.
    None,
    /// A pane's frame traces itself once round when it first appears, its content sweeping in
    /// behind it.
    #[default]
    Trace,
    /// The new screen shows through a field of shade glyphs that thins to nothing.
    Noise,
    /// The old screen's columns slide down off the bottom, revealing the new one beneath.
    Melt,
}

impl Transition {
    /// Every transition, in the order the configuration error and the Settings box list
    /// them.
    pub const ALL: [Transition; 4] = [
        Transition::None,
        Transition::Trace,
        Transition::Noise,
        Transition::Melt,
    ];

    /// What `transition` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            Transition::None => "none",
            Transition::Trace => "trace",
            Transition::Noise => "noise",
            Transition::Melt => "melt",
        }
    }

    /// The transition `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<Transition> {
        Transition::ALL
            .into_iter()
            .find(|transition| transition.name() == word)
    }
}

/// Which effect builds the wordmark as `svolder tui` starts, from `intro_effect` under `[tui]`.
///
/// Every one of them but [`IntroEffect::Fade`] is a demo effect. Like the fade, a demo effect
/// is drawn only where the animation layer runs; elsewhere the mark is drawn static whatever
/// this says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IntroEffect {
    /// One of the demo effects, picked afresh each launch; never the fade.
    #[default]
    Random,
    /// The mark fades in and holds.
    Fade,
    /// Sparks thrown in from the edges build the mark.
    Particles,
}

impl IntroEffect {
    /// Every effect, in the order the configuration error and the Settings box list them.
    pub const ALL: [IntroEffect; 3] = [
        IntroEffect::Random,
        IntroEffect::Fade,
        IntroEffect::Particles,
    ];

    /// What `intro_effect` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            IntroEffect::Random => "random",
            IntroEffect::Fade => "fade",
            IntroEffect::Particles => "particles",
        }
    }

    /// The effect `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<IntroEffect> {
        IntroEffect::ALL
            .into_iter()
            .find(|effect| effect.name() == word)
    }
}

/// A span of history for the Telemetry and level graphs, from `graph_span` under `[tui]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum GraphSpan {
    /// 1 minute.
    #[serde(rename = "1m")]
    M1,
    /// 2 minutes.
    #[serde(rename = "2m")]
    M2,
    /// 3 minutes.
    #[serde(rename = "3m")]
    M3,
    /// 5 minutes.
    #[default]
    #[serde(rename = "5m")]
    M5,
    /// 10 minutes.
    #[serde(rename = "10m")]
    M10,
    /// 15 minutes.
    #[serde(rename = "15m")]
    M15,
    /// 30 minutes.
    #[serde(rename = "30m")]
    M30,
    /// 60 minutes.
    #[serde(rename = "60m")]
    M60,
}

impl GraphSpan {
    /// Every span, shortest first, in the order the configuration error and the Settings box
    /// list them.
    pub const ALL: [GraphSpan; 8] = [
        GraphSpan::M1,
        GraphSpan::M2,
        GraphSpan::M3,
        GraphSpan::M5,
        GraphSpan::M10,
        GraphSpan::M15,
        GraphSpan::M30,
        GraphSpan::M60,
    ];

    /// What `graph_span` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            GraphSpan::M1 => "1m",
            GraphSpan::M2 => "2m",
            GraphSpan::M3 => "3m",
            GraphSpan::M5 => "5m",
            GraphSpan::M10 => "10m",
            GraphSpan::M15 => "15m",
            GraphSpan::M30 => "30m",
            GraphSpan::M60 => "60m",
        }
    }

    /// Every name, joined as prose: `1m, 2m or 3m`.
    pub fn list() -> String {
        let names: Vec<&str> = GraphSpan::ALL.iter().map(|span| span.name()).collect();
        match names.split_last() {
            Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
            None => String::new(),
        }
    }

    /// The span `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<GraphSpan> {
        GraphSpan::ALL.into_iter().find(|span| span.name() == word)
    }

    /// How long the span is.
    pub fn duration(self) -> std::time::Duration {
        let minutes = match self {
            GraphSpan::M1 => 1,
            GraphSpan::M2 => 2,
            GraphSpan::M3 => 3,
            GraphSpan::M5 => 5,
            GraphSpan::M10 => 10,
            GraphSpan::M15 => 15,
            GraphSpan::M30 => 30,
            GraphSpan::M60 => 60,
        };
        std::time::Duration::from_secs(minutes * 60)
    }
}

/// A span of history for the connection strip, from `link_span` under `[tui]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum LinkSpan {
    /// 1 hour.
    #[serde(rename = "1h")]
    H1,
    /// 3 hours.
    #[serde(rename = "3h")]
    H3,
    /// 6 hours.
    #[serde(rename = "6h")]
    H6,
    /// 12 hours.
    #[default]
    #[serde(rename = "12h")]
    H12,
    /// 24 hours.
    #[serde(rename = "24h")]
    H24,
}

impl LinkSpan {
    /// Every span, shortest first, in the order the configuration error and the Settings box
    /// list them.
    pub const ALL: [LinkSpan; 5] = [
        LinkSpan::H1,
        LinkSpan::H3,
        LinkSpan::H6,
        LinkSpan::H12,
        LinkSpan::H24,
    ];

    /// What `link_span` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            LinkSpan::H1 => "1h",
            LinkSpan::H3 => "3h",
            LinkSpan::H6 => "6h",
            LinkSpan::H12 => "12h",
            LinkSpan::H24 => "24h",
        }
    }

    /// Every name, joined as prose: `1h, 3h or 6h`.
    pub fn list() -> String {
        let names: Vec<&str> = LinkSpan::ALL.iter().map(|span| span.name()).collect();
        match names.split_last() {
            Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
            None => String::new(),
        }
    }

    /// The span `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<LinkSpan> {
        LinkSpan::ALL.into_iter().find(|span| span.name() == word)
    }

    /// How long the span is.
    pub fn duration(self) -> std::time::Duration {
        let hours = match self {
            LinkSpan::H1 => 1,
            LinkSpan::H3 => 3,
            LinkSpan::H6 => 6,
            LinkSpan::H12 => 12,
            LinkSpan::H24 => 24,
        };
        std::time::Duration::from_secs(hours * 3600)
    }
}

/// The `effects` key under `[tui]`: the animation layer as a whole, and one switch per
/// animation.
///
/// `effects = false` turns every one of them off and `effects = true` is these defaults; a
/// table starts from the defaults and overrides only the keys it names, `enabled` among them.
///
/// Written back in the same three shapes it is read in, by this type's own
/// [`Serialize`](serde::Serialize) implementation, so a rendered config keeps the file's shape
/// rather than becoming a table where the file said one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectsConfig {
    /// Whether the animation layer runs at all.
    pub enabled: bool,
    /// Whether a changed value's bar glides to its new length and its row glows.
    pub eased: bool,
    /// Whether the row the cursor left keeps a fading trail.
    pub glow: bool,
    /// Whether a highlight sweeps the row you change, and the selected row while you rest.
    pub shimmer: bool,
    /// Whether panel contours pulse with the level: the amp's meters, else what the host plays.
    pub breathing: bool,
    /// Whether a light runs once round the frame the keys move or arrive to. The key keeps
    /// its name, so a file that switches it off goes on doing so; how a pane arrives on the
    /// screen is [`TuiConfig::transition`]'s.
    pub frames: bool,
    /// Whether a standing fault's row and the header border pulse.
    pub fault_presence: bool,
    /// Whether a marker lingers at a meter's recent peak.
    pub peak_hold: bool,
    /// Whether the decoration drifts along the palette's spectrum, glowing hotter as the level
    /// rises; off by default.
    pub hue_drift: bool,
    /// Whether the tone curve's colours scroll round the hue circle (along the palette's own ramp
    /// on `red` and `neon`) rather than resting red at the lowest frequency and violet at the
    /// highest (or on that ramp); off by default.
    pub spectrum_drift: bool,
    /// Whether the rows of a list dim with their distance from the selected row.
    pub list_fade: bool,
    /// Whether Ratatoskr, the squirrel, may come out; it stays hidden until it is called.
    pub mascot: bool,
}

impl Default for EffectsConfig {
    /// The animation layer on with every animation running, and the hue drift and the
    /// spectrum's own scroll — both modes rather than animations — off.
    fn default() -> Self {
        EffectsConfig {
            enabled: true,
            eased: true,
            glow: true,
            shimmer: true,
            breathing: true,
            frames: true,
            fault_presence: true,
            peak_hold: true,
            hue_drift: false,
            spectrum_drift: false,
            list_fade: true,
            mascot: true,
        }
    }
}

impl serde::Serialize for EffectsConfig {
    /// Write the shape the file writes: `true` for the defaults, `false` for everything off,
    /// and the whole table for anything else, exactly as [`Config::parse`] reads it back.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        if *self == EffectsConfig::default() {
            return serializer.serialize_bool(true);
        }
        if *self == EffectsConfig::off() {
            return serializer.serialize_bool(false);
        }
        let mut table = serializer.serialize_struct("EffectsConfig", 12)?;
        table.serialize_field("enabled", &self.enabled)?;
        table.serialize_field("eased", &self.eased)?;
        table.serialize_field("glow", &self.glow)?;
        table.serialize_field("shimmer", &self.shimmer)?;
        table.serialize_field("breathing", &self.breathing)?;
        table.serialize_field("frames", &self.frames)?;
        table.serialize_field("fault_presence", &self.fault_presence)?;
        table.serialize_field("peak_hold", &self.peak_hold)?;
        table.serialize_field("hue_drift", &self.hue_drift)?;
        table.serialize_field("spectrum_drift", &self.spectrum_drift)?;
        table.serialize_field("list_fade", &self.list_fade)?;
        table.serialize_field("mascot", &self.mascot)?;
        table.end()
    }
}

impl EffectsConfig {
    /// Every switch off, as `effects = false` means.
    pub fn off() -> EffectsConfig {
        EffectsConfig {
            enabled: false,
            eased: false,
            glow: false,
            shimmer: false,
            breathing: false,
            frames: false,
            fault_presence: false,
            peak_hold: false,
            hue_drift: false,
            spectrum_drift: false,
            list_fade: false,
            mascot: false,
        }
    }
}

/// The `[tui.accents]` table: an accent colour per device family, each in the same form as
/// `accent`, and each `None` when the file names none.
///
/// A family that overrides nothing is left out of what this serialises to, the way `accent`
/// is: a rendered configuration names the keys a hand-written file would name and no others.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct FamilyAccents {
    /// The accent a DAC draws with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dac: Option<String>,
    /// The accent an amplifier draws with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amp: Option<String>,
    /// The accent a preamplifier draws with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp: Option<String>,
}

impl FamilyAccents {
    /// The accent `family` overrides its palette colour with, if any.
    pub fn get(&self, family: Family) -> Option<&str> {
        match family {
            Family::Dac => self.dac.as_deref(),
            Family::Amp => self.amp.as_deref(),
            Family::Preamp => self.preamp.as_deref(),
        }
    }

    /// Set (or, with `None`, clear) the accent of `family`.
    pub fn set(&mut self, family: Family, accent: Option<String>) {
        match family {
            Family::Dac => self.dac = accent,
            Family::Amp => self.amp = accent,
            Family::Preamp => self.preamp = accent,
        }
    }
}

impl Default for TuiConfig {
    /// The `amber` palette with no accent overridden and no spectrum of its own, auto colour
    /// depth, not monochrome, the trace between screens, and the theme background, mouse, intro,
    /// title meter, the quit question, host audio, the visualizer and effects all on.
    fn default() -> Self {
        TuiConfig {
            accent: None,
            palette: DEFAULT_PALETTE.into(),
            accents: FamilyAccents::default(),
            spectrum: Vec::new(),
            colors: ColorDepth::Auto,
            monochrome: false,
            theme_background: true,
            mouse: true,
            intro: true,
            intro_sound: false,
            intro_effect: IntroEffect::default(),
            transition: Transition::default(),
            title_meter: true,
            confirm_quit: true,
            host_audio: true,
            visualizer: true,
            graphs: true,
            graph_span: GraphSpan::default(),
            link_span: LinkSpan::default(),
            effects: EffectsConfig::default(),
        }
    }
}

/// The colour depth `svolder tui` renders at, from `colors` under `[tui]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorDepth {
    /// Detect the depth from `COLORTERM`/`TERM`.
    #[default]
    Auto,
    /// Force the 16 ANSI colours.
    #[serde(rename = "16")]
    Ansi16,
    /// Force the 256-colour palette.
    #[serde(rename = "256")]
    Ansi256,
    /// Force 24-bit truecolor.
    #[serde(rename = "true")]
    TrueColor,
}

impl ColorDepth {
    /// What `colors` under `[tui]` writes this as, which is the name it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            ColorDepth::Auto => "auto",
            ColorDepth::Ansi16 => "16",
            ColorDepth::Ansi256 => "256",
            ColorDepth::TrueColor => "true",
        }
    }
}

/// Write `stops` as the `[tui.spectrum]` table itself rather than as the array it is held as:
/// `{ "stops": [...] }`, the shape the file spells and the shape a reader of `config show
/// --json` can hand back to the file.
fn stops_table<S: serde::Serializer>(stops: &[String], out: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;

    let mut table = out.serialize_struct("Spectrum", 1)?;
    table.serialize_field("stops", stops)?;
    table.end()
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

/// `$XDG_CONFIG_HOME`, else `$HOME/.config`, else the account's home directory (from the
/// password database) plus `.config`; the relative `.config` only when the account has no home.
pub fn config_home() -> PathBuf {
    config_home_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
        passwd_home,
    )
}

/// Points every home svolder resolves at a scratch directory, so a unit test never reads or
/// writes the user's own config, state, data, cache or runtime directory, however the tests
/// were started.
///
/// The scratch root is a private directory (mode 0700, owned by this user) named
/// `svolder-unit-<pid>` under the temporary directory, holding `config`, `state`, `data`,
/// `cache` and `run`. A value of `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_DATA_HOME`,
/// `XDG_CACHE_HOME` or `XDG_RUNTIME_DIR` that is already a strict descendant of the temporary
/// directory is kept; any other value, or none, becomes the root's directory of that name.
/// `make test` points `TMPDIR` at a directory of its own and the homes beside it, so each test
/// binary pins its own homes under that `TMPDIR`. When no safe root can be made, the homes are
/// left as they are. `SVOLDER_MCP_TOKEN` and `SVOLDER_WEB_TOKEN` are removed, and
/// `SVOLDER_WIREPLUMBER` and `SVOLDER_AUTOEQ_URL` get the values `make test` gives them when
/// unset. The scratch roots of runs whose lock nobody holds are removed first.
///
/// It runs before `main`, where a panic would abort the test binary without a word, so every
/// call in it either cannot panic (the environment setters get valid names and values) or has
/// its error dropped.
///
/// # Safety
///
/// Call it only while the process has one thread: it sets environment variables.
#[doc(hidden)]
pub unsafe fn pin_scratch_homes() {
    let temp = std::env::temp_dir();
    sweep_finished_scratch_homes(&temp);
    if let Some((root, lock)) = scratch_root(&temp, std::process::id()) {
        // The lock is held until the process ends: its descriptor is never closed.
        std::mem::forget(lock);
        for (name, leaf) in [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_RUNTIME_DIR", "run"),
        ] {
            let dir = match std::env::var_os(name) {
                Some(kept) if is_strictly_under(Path::new(&kept), &temp) => PathBuf::from(kept),
                _ => root.join(leaf),
            };
            let _ = create_private_dir(&dir);
            // SAFETY: the caller guarantees one thread.
            unsafe { std::env::set_var(name, &dir) };
        }
        if std::env::var_os("SVOLDER_WIREPLUMBER").is_none() {
            // SAFETY: the caller guarantees one thread.
            unsafe { std::env::set_var("SVOLDER_WIREPLUMBER", root.join("no-wireplumber")) };
        }
    }
    // SAFETY: the caller guarantees one thread.
    unsafe { std::env::remove_var("SVOLDER_MCP_TOKEN") };
    // SAFETY: the caller guarantees one thread.
    unsafe { std::env::remove_var("SVOLDER_WEB_TOKEN") };
    if std::env::var_os("SVOLDER_AUTOEQ_URL").is_none() {
        // SAFETY: the caller guarantees one thread.
        unsafe { std::env::set_var("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/") };
    }
}

/// Whether `dir` is absolute, has no `..` component and lies strictly below `temp`.
pub(crate) fn is_strictly_under(dir: &Path, temp: &Path) -> bool {
    dir.is_absolute()
        && dir != temp
        && dir.starts_with(temp)
        && !dir
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}

/// Creates `dir` (and its parents) with mode 0700.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// The file in a scratch root that its run holds a lock on for as long as it lives.
const LIVE_LOCK: &str = "live.lock";

/// Takes `file`'s lock unless another holds it, without waiting: whether this process holds it
/// now.
fn locked_now(file: &std::fs::File) -> bool {
    use std::os::fd::AsRawFd;
    // SAFETY: `flock` only reads the descriptor, which `file` keeps open for the call.
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
}

/// Whether `path` is still the file `open` was opened from: a sweep may have unlinked a lock
/// file between its opening and its locking.
fn is_file_at(open: &std::fs::File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (open.metadata(), std::fs::symlink_metadata(path)) {
        (Ok(held), Ok(there)) => held.dev() == there.dev() && held.ino() == there.ino(),
        _ => false,
    }
}

/// A private scratch root for process `pid` under `temp` and the lock on it this run holds:
/// `svolder-unit-<pid>`, else `svolder-unit-<pid>-<n>`. A new one is created with mode 0700; an
/// existing one is taken only when it is a real directory (not a symlink) owned by this user with
/// mode 0700 whose lock nobody holds, so a run with the same pid in another PID namespace keeps
/// its own. `None` when no candidate is safe.
pub(crate) fn scratch_root(temp: &Path, pid: u32) -> Option<(PathBuf, std::fs::File)> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};

    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    let candidates = std::iter::once(format!("svolder-unit-{pid}"))
        .chain((1..=8).map(|n| format!("svolder-unit-{pid}-{n}")));
    for name in candidates {
        let root = temp.join(name);
        let made = std::fs::DirBuilder::new().mode(0o700).create(&root).is_ok();
        let safe = made
            || std::fs::symlink_metadata(&root).is_ok_and(|meta| {
                meta.is_dir() && meta.uid() == uid && meta.mode() & 0o777 == 0o700
            });
        if !safe {
            continue;
        }
        // Never follow a symlink, never wait on a FIFO: this runs before `main`.
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join(LIVE_LOCK));
        if let Ok(lock) = lock
            && locked_now(&lock)
            && is_file_at(&lock, &root.join(LIVE_LOCK))
        {
            return Some((root, lock));
        }
    }
    None
}

/// Removes the `svolder-unit-<pid>` and `svolder-unit-<pid>-<n>` directories of `root` whose run
/// no longer holds its lock. A symlink is never followed, and a directory another user owns is
/// left alone.
pub(crate) fn sweep_finished_scratch_homes(root: &Path) {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let Some(uid) = std::fs::metadata("/proc/self").ok().map(|m| m.uid()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix("svolder-unit-"))
            .and_then(|rest| rest.split('-').next())
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        let ours = std::fs::symlink_metadata(entry.path())
            .is_ok_and(|m| is_ours_to_sweep(m.is_dir(), m.uid(), uid));
        if !ours {
            continue;
        }
        // A live run holds its root's lock for as long as it runs, in whatever PID namespace; the
        // lock taken here is held while the root goes, and a run that locks a file this removal
        // unlinks finds it gone and takes another name. A root with no lock file (made by a run
        // that takes no lock, or in the instant between its creation and its lock) is judged by
        // its pid in `/proc`.
        let held = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(entry.path().join(LIVE_LOCK))
        {
            Ok(file) if locked_now(&file) => Some(file),
            Ok(_) => continue,
            Err(_) if Path::new("/proc").join(pid.to_string()).exists() => continue,
            Err(_) => None,
        };
        let _ = std::fs::remove_dir_all(entry.path());
        drop(held);
    }
}

/// Whether a scratch entry is this process's to remove: a real directory (never a symlink,
/// which `symlink_metadata` reports as not a directory) owned by `uid`.
fn is_ours_to_sweep(is_real_dir: bool, owner: u32, uid: u32) -> bool {
    is_real_dir && owner == uid
}

/// `$XDG_STATE_HOME`, or `~/.local/state`: where svolder keeps what this machine learned and
/// the secrets it generates, in a `svolder` directory of its own.
pub fn state_home() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().unwrap_or_default().join(".local").join("state"))
}

/// Where this machine's computer EQ keeps its saved state: `host-eq.toml` in svolder's state
/// directory.
pub fn host_eq_state_path() -> PathBuf {
    state_home().join("svolder").join("host-eq.toml")
}

/// `$XDG_CACHE_HOME`, or `~/.cache`: where svolder keeps what it can fetch again, in a
/// `svolder` directory of its own.
pub fn cache_home() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().unwrap_or_default().join(".cache"))
}

/// The user's home directory: `$HOME`, else the password database's entry for this uid.
pub fn home_dir() -> Option<PathBuf> {
    home_from(std::env::var_os("HOME"), passwd_home)
}

/// [`config_home`] with the environment and the password lookup as inputs.
fn config_home_from(
    xdg: Option<OsString>,
    home: Option<OsString>,
    passwd: impl FnOnce() -> Option<PathBuf>,
) -> PathBuf {
    match xdg.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home_from(home, passwd).unwrap_or_default().join(".config"),
    }
}

/// A non-empty `home`, else what `passwd` finds.
fn home_from(home: Option<OsString>, passwd: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    home.filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(passwd)
}

/// The home directory the password database lists for the current uid, when it lists one.
fn passwd_home() -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    let mut buf = vec![0u8; 1024];
    loop {
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: `entry` and `buf` outlive the call, `buf.len()` is its real size, and
        // `found` is read only after the call has set it.
        let rc = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                entry.as_mut_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut found,
            )
        };
        if rc == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        // SAFETY: `found` is non-null when it is read, so it points at the initialized `entry`.
        if rc != 0 || found.is_null() || unsafe { (*found).pw_dir.is_null() } {
            return None;
        }
        // SAFETY: `getpwuid_r` succeeded with a non-null result, so `entry` is initialized and
        // `pw_dir` points at a NUL-terminated string inside `buf`.
        let dir = unsafe { std::ffi::CStr::from_ptr((*found).pw_dir) };
        let bytes = dir.to_bytes();
        return (!bytes.is_empty()).then(|| PathBuf::from(OsString::from_vec(bytes.to_vec())));
    }
}

impl Config {
    /// `$XDG_CONFIG_HOME/svolder/config.toml` or `~/.config/svolder/config.toml`, with the
    /// account's home directory standing in for an unset `$HOME`.
    pub fn path() -> PathBuf {
        config_home().join("svolder").join("config.toml")
    }

    /// The `default_device` name when it names no configured device (case aside), else `None`.
    pub fn dangling_default(&self) -> Option<&str> {
        let name = self.default_device.as_deref()?;
        let known = self
            .devices
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(name));
        (!known).then_some(name)
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

    /// Load and validate the file at `path`; `Ok(Config::default())` when it is absent.
    pub fn load_from(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError::Io(path.to_path_buf(), e)),
        }
    }
}

#[cfg(test)]
mod tests;
