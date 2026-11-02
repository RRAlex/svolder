// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading `config.toml`: the shapes the file is deserialized into, [`Config::parse`] that
//! turns them into a checked configuration, and the checks each key and `[[preset]]` block
//! must pass.

use std::collections::BTreeMap;

use svolder_profiles::EqMode;
use svolder_profiles::tone::apo::{self, Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{
    Imported, LOUDNESS_MAX, NameError, Preset, PresetBand, checked_name, is_builtin, same_name,
};

use super::hops::{hop_taken, name_refusal};
use super::presets::preset_taken;
use super::{
    AutoEqConfig, ColorDepth, Config, ConfigError, DaemonConfig, DeviceConfig, EffectsConfig,
    FamilyAccents, GraphSpan, Hop, HopKind, HopWhat, HostConfig, IntroEffect, LinkSpan, McpAuth,
    McpConfig, MqttConfig, RefreshWeeks, Transition, TuiConfig, WebAuth, WebConfig, WebGuest,
    normal_origin,
};

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

/// The on-disk shape of the `[tui]` table, before validation.
#[derive(serde::Deserialize)]
#[serde(default)]
struct RawTui {
    accent: Option<String>,
    palette: String,
    accents: RawAccents,
    spectrum: RawSpectrum,
    colors: ColorDepth,
    monochrome: bool,
    theme_background: bool,
    mouse: bool,
    intro: bool,
    intro_sound: bool,
    transition: String,
    intro_effect: String,
    title_meter: bool,
    confirm_quit: bool,
    host_audio: bool,
    visualizer: bool,
    graphs: bool,
    graph_span: String,
    link_span: String,
    effects: RawEffects,
}

impl Default for RawTui {
    /// The same defaults [`TuiConfig::default`] names, read from it directly rather than
    /// repeated here: no accent override, auto colour depth, not monochrome, the trace
    /// between screens, and mouse, intro, title meter, the quit question, host audio and the
    /// visualizer all on. `accents` and `spectrum` have no on-disk default of their own
    /// beyond "empty", and `effects` collapses every enabled animation to the one switch a
    /// file would write, so the three keep their own literals.
    fn default() -> Self {
        let TuiConfig {
            accent,
            palette,
            accents: _,
            spectrum: _,
            colors,
            monochrome,
            theme_background,
            mouse,
            intro,
            intro_sound,
            transition,
            intro_effect,
            title_meter,
            confirm_quit,
            host_audio,
            visualizer,
            graphs,
            graph_span,
            link_span,
            effects: _,
        } = TuiConfig::default();
        RawTui {
            accent,
            palette,
            accents: RawAccents::default(),
            spectrum: RawSpectrum::default(),
            colors,
            monochrome,
            theme_background,
            mouse,
            intro,
            intro_sound,
            transition: transition.name().into(),
            intro_effect: intro_effect.name().into(),
            title_meter,
            confirm_quit,
            host_audio,
            visualizer,
            graphs,
            graph_span: graph_span.name().into(),
            link_span: link_span.name().into(),
            effects: RawEffects::Switch(true),
        }
    }
}

/// The on-disk shape of `effects` under `[tui]`: the one switch every older file writes, or
/// the table that names the animations one at a time.
enum RawEffects {
    /// `effects = true|false`: every animation at once.
    Switch(bool),
    /// `effects = { eased = false, ... }`: the defaults with these keys overridden.
    Table(RawEffectsTable),
}

/// The keys an `effects` table takes, in the order the error message lists them.
pub(super) const EFFECT_KEYS: [&str; 12] = [
    "enabled",
    "eased",
    "glow",
    "shimmer",
    "breathing",
    "frames",
    "fault_presence",
    "peak_hold",
    "hue_drift",
    "spectrum_drift",
    "list_fade",
    "mascot",
];

impl<'de> serde::Deserialize<'de> for RawEffects {
    /// A bool, or a table whose keys are all among [`EFFECT_KEYS`] and whose values are all
    /// bools; anything else is an error naming the key or the shape.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = RawEffects;

            fn expecting(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                out.write_str("tui.effects to be true, false or a table")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<RawEffects, E> {
                Ok(RawEffects::Switch(value))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<RawEffects, A::Error> {
                use serde::de::Error;

                let mut table = RawEffectsTable::default();
                while let Some(key) = map.next_key::<String>()? {
                    let slot = match key.as_str() {
                        "enabled" => &mut table.enabled,
                        "eased" => &mut table.eased,
                        "glow" => &mut table.glow,
                        "shimmer" => &mut table.shimmer,
                        "breathing" => &mut table.breathing,
                        "frames" => &mut table.frames,
                        "fault_presence" => &mut table.fault_presence,
                        "peak_hold" => &mut table.peak_hold,
                        "hue_drift" => &mut table.hue_drift,
                        "spectrum_drift" => &mut table.spectrum_drift,
                        "list_fade" => &mut table.list_fade,
                        "mascot" => &mut table.mascot,
                        _ => {
                            return Err(A::Error::custom(format!(
                                "tui.effects: unknown key {key:?}; the keys are {}",
                                EFFECT_KEYS.join(", ")
                            )));
                        }
                    };
                    *slot = map.next_value::<bool>().map_err(|_| {
                        A::Error::custom(format!("tui.effects.{key} must be true or false"))
                    })?;
                }
                Ok(RawEffects::Table(table))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// The on-disk shape of an `effects` table, every key of it optional.
struct RawEffectsTable {
    enabled: bool,
    eased: bool,
    glow: bool,
    shimmer: bool,
    breathing: bool,
    frames: bool,
    fault_presence: bool,
    peak_hold: bool,
    hue_drift: bool,
    spectrum_drift: bool,
    list_fade: bool,
    mascot: bool,
}

impl Default for RawEffectsTable {
    /// The defaults a table starts from, which are [`EffectsConfig::default`]'s own.
    fn default() -> Self {
        RawEffectsTable::from(EffectsConfig::default())
    }
}

impl From<EffectsConfig> for RawEffectsTable {
    fn from(effects: EffectsConfig) -> Self {
        RawEffectsTable {
            enabled: effects.enabled,
            eased: effects.eased,
            glow: effects.glow,
            shimmer: effects.shimmer,
            breathing: effects.breathing,
            frames: effects.frames,
            fault_presence: effects.fault_presence,
            peak_hold: effects.peak_hold,
            hue_drift: effects.hue_drift,
            spectrum_drift: effects.spectrum_drift,
            list_fade: effects.list_fade,
            mascot: effects.mascot,
        }
    }
}

impl From<RawEffects> for EffectsConfig {
    fn from(raw: RawEffects) -> Self {
        match raw {
            RawEffects::Switch(true) => EffectsConfig::default(),
            RawEffects::Switch(false) => EffectsConfig::off(),
            RawEffects::Table(table) => EffectsConfig {
                enabled: table.enabled,
                eased: table.eased,
                glow: table.glow,
                shimmer: table.shimmer,
                breathing: table.breathing,
                frames: table.frames,
                fault_presence: table.fault_presence,
                peak_hold: table.peak_hold,
                hue_drift: table.hue_drift,
                spectrum_drift: table.spectrum_drift,
                list_fade: table.list_fade,
                mascot: table.mascot,
            },
        }
    }
}

/// The on-disk shape of the `[tui.accents]` table, before validation.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct RawAccents {
    dac: Option<String>,
    amp: Option<String>,
    preamp: Option<String>,
}

/// The on-disk shape of the `[tui.spectrum]` table, before validation.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct RawSpectrum {
    stops: Vec<String>,
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

/// The on-disk shape of the `[mqtt]` table, before validation. `port` is kept wider than
/// `u16` and `max_volume_db` wider than "must be finite" so an out-of-range value reaches
/// `mqtt`'s own check and its message naming `mqtt.port` or `mqtt.max_volume_db`,
/// rather than an opaque error from the TOML parser.
#[derive(serde::Deserialize)]
#[serde(default)]
struct RawMqtt {
    enabled: bool,
    host: Option<String>,
    port: i64,
    username: Option<String>,
    password: Option<String>,
    client_id: Option<String>,
    base_topic: String,
    discovery_prefix: String,
    commands: bool,
    max_volume_db: Option<f64>,
}

impl Default for RawMqtt {
    /// The same defaults [`MqttConfig::default`] names.
    fn default() -> Self {
        let d = MqttConfig::default();
        RawMqtt {
            enabled: d.enabled,
            host: d.host,
            port: i64::from(d.port),
            username: d.username,
            password: d.password,
            client_id: d.client_id,
            base_topic: d.base_topic,
            discovery_prefix: d.discovery_prefix,
            commands: d.commands,
            max_volume_db: d.max_volume_db,
        }
    }
}

/// The on-disk shape of the `[mcp]` table, before validation. `listen` stays text so a bad
/// address reaches `mcp`'s own message naming `mcp.listen`.
#[derive(serde::Deserialize)]
#[serde(default)]
struct RawMcp {
    enabled: bool,
    listen: String,
    auth: McpAuth,
    writes: bool,
    max_volume_db: Option<f64>,
    origins: Vec<String>,
    hosts: Vec<String>,
    token_file: Option<String>,
}

impl Default for RawMcp {
    /// The same defaults [`McpConfig::default`] names.
    fn default() -> Self {
        let d = McpConfig::default();
        RawMcp {
            enabled: d.enabled,
            listen: d.listen.to_string(),
            auth: d.auth,
            writes: d.writes,
            max_volume_db: d.max_volume_db,
            origins: d.origins,
            hosts: d.hosts,
            token_file: d.token_file,
        }
    }
}

/// The on-disk shape of the `[web]` table, before validation; `listen` and `guest` stay text.
#[derive(serde::Deserialize)]
#[serde(default)]
struct RawWeb {
    enabled: bool,
    listen: String,
    auth: WebAuth,
    writes: bool,
    max_volume_db: Option<f64>,
    guest: String,
    url: String,
    hosts: Vec<String>,
    origins: Vec<String>,
    token_file: Option<String>,
}

impl Default for RawWeb {
    /// The same defaults [`WebConfig::default`] names.
    fn default() -> Self {
        let d = WebConfig::default();
        RawWeb {
            enabled: d.enabled,
            listen: d.listen.to_string(),
            auth: d.auth,
            writes: d.writes,
            max_volume_db: d.max_volume_db,
            guest: "none".to_string(),
            url: String::new(),
            hosts: d.hosts,
            origins: d.origins,
            token_file: d.token_file,
        }
    }
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
    /// Parse from text (the seam every test uses).
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        if raw.schema != 1 {
            return Err(ConfigError::Schema(raw.schema));
        }
        let devices = devices(raw.devices)?;
        let presets = presets(&raw.presets)?;
        let hops = hops(&raw.hops)?;
        // A blank value overrides nothing: every optional key means "as if the key were
        // absent" when it is left empty, which is the value the config template shows each of
        // them at.
        let adapter = raw.adapter.filter(|value| is_named(value));
        let default_device = raw.default_device.filter(|value| is_named(value));
        let socket = raw.socket.filter(|value| is_named(value));
        let tui = tui(raw.tui)?;
        let mqtt = mqtt(raw.mqtt)?;
        let mcp = mcp(raw.mcp)?;
        let web = web(raw.web)?;
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
            hops,
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
fn devices(raw: Vec<RawDevice>) -> Result<Vec<DeviceConfig>, ConfigError> {
    let mut devices = Vec::with_capacity(raw.len());
    for d in raw {
        // Validate the udid is a parseable 8-byte unit id.
        d.udid
            .parse::<svolder_proto::UnitId>()
            .map_err(|_| ConfigError::Parse(format!("device {}: bad udid {:?}", d.name, d.udid)))?;
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
    Ok(devices)
}

/// The `[[hop]]` blocks: each one checked, and no two sharing an id.
fn hops(raw: &[toml::Value]) -> Result<Vec<Hop>, ConfigError> {
    let mut hops: Vec<Hop> = Vec::with_capacity(raw.len());
    for value in raw {
        let hop = parse_hop(value)?;
        if hops
            .iter()
            .any(|existing| same_name(existing.id(), hop.id()))
        {
            return Err(hop_taken(hop.id()));
        }
        hops.push(hop);
    }
    Ok(hops)
}

/// The `[[preset]]` blocks: each one checked, and no two sharing a name.
fn presets(raw: &[toml::Value]) -> Result<Vec<Preset>, ConfigError> {
    let mut presets: Vec<Preset> = Vec::with_capacity(raw.len());
    for value in raw {
        let preset = parse_preset(value)?;
        if presets
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&preset.name))
        {
            return Err(preset_taken(&preset.name));
        }
        presets.push(preset);
    }
    Ok(presets)
}

/// The `[tui]` table, with `[tui.accents]`, `[tui.spectrum]` and `effects`.
fn tui(raw: RawTui) -> Result<TuiConfig, ConfigError> {
    let accent = raw.accent.filter(|value| is_named(value));
    let accents = FamilyAccents {
        dac: raw.accents.dac.filter(|value| is_named(value)),
        amp: raw.accents.amp.filter(|value| is_named(value)),
        preamp: raw.accents.preamp.filter(|value| is_named(value)),
    };
    validate_palette(&raw.palette)?;
    let transition = parse_transition(&raw.transition)?;
    let intro_effect = parse_intro_effect(&raw.intro_effect)?;
    let graph_span = parse_graph_span(&raw.graph_span)?;
    let link_span = parse_link_span(&raw.link_span)?;
    for accent in [&accent, &accents.dac, &accents.amp, &accents.preamp]
        .into_iter()
        .flatten()
    {
        validate_accent(accent)?;
    }
    for stop in &raw.spectrum.stops {
        validate_stop(stop)?;
    }
    let RawTui {
        accent: _,
        palette,
        accents: _,
        spectrum,
        colors,
        monochrome,
        theme_background,
        mouse,
        intro,
        intro_sound,
        transition: _,
        intro_effect: _,
        title_meter,
        confirm_quit,
        host_audio,
        visualizer,
        graphs,
        graph_span: _,
        link_span: _,
        effects,
    } = raw;
    Ok(TuiConfig {
        accent,
        palette,
        accents,
        spectrum: spectrum.stops,
        colors,
        monochrome,
        theme_background,
        mouse,
        intro,
        intro_sound,
        transition,
        intro_effect,
        title_meter,
        confirm_quit,
        host_audio,
        visualizer,
        graphs,
        graph_span,
        link_span,
        effects: effects.into(),
    })
}

/// The `[mqtt]` table: the broker, the volume cap and the two topic prefixes.
fn mqtt(raw: RawMqtt) -> Result<MqttConfig, ConfigError> {
    let host = raw.host.filter(|value| is_named(value));
    let username = raw.username.filter(|value| is_named(value));
    let password = raw.password.filter(|value| is_named(value));
    let client_id = raw.client_id.filter(|value| is_named(value));
    if raw.enabled && host.is_none() {
        return Err(ConfigError::Parse(
            "mqtt.host must not be empty when enabled = true".to_string(),
        ));
    }
    if !(1..=65535).contains(&raw.port) {
        return Err(ConfigError::Parse(format!(
            "mqtt.port {} must be between 1 and 65535",
            raw.port
        )));
    }
    if let Some(cap) = raw.max_volume_db
        && !cap.is_finite()
    {
        return Err(ConfigError::Parse(
            "mqtt.max_volume_db must be a finite number".to_string(),
        ));
    }
    if raw.base_topic.is_empty() {
        return Err(ConfigError::Parse(
            "mqtt.base_topic must not be empty".to_string(),
        ));
    }
    topic_prefix("mqtt.base_topic", &raw.base_topic)?;
    topic_prefix("mqtt.discovery_prefix", &raw.discovery_prefix)?;
    Ok(MqttConfig {
        enabled: raw.enabled,
        host,
        // Validated above to be in range, so this narrowing never truncates.
        port: raw.port as u16,
        username,
        password,
        client_id,
        base_topic: raw.base_topic,
        discovery_prefix: raw.discovery_prefix,
        commands: raw.commands,
        max_volume_db: raw.max_volume_db,
    })
}

/// The `[mcp]` table: the listen address, its auth, the volume cap, origins and hosts.
fn mcp(raw: RawMcp) -> Result<McpConfig, ConfigError> {
    let listen: std::net::SocketAddr = raw.listen.trim().parse().map_err(|_| {
        ConfigError::Parse(format!(
            "mcp.listen {:?} must be an address and port, such as \"127.0.0.1:8737\"",
            raw.listen
        ))
    })?;
    if raw.auth == McpAuth::None && !listen.ip().is_loopback() {
        return Err(ConfigError::Parse(
            "mcp.auth = \"none\" needs a loopback listen address".to_string(),
        ));
    }
    if let Some(cap) = raw.max_volume_db
        && !cap.is_finite()
    {
        return Err(ConfigError::Parse(
            "mcp.max_volume_db must be a finite number".to_string(),
        ));
    }
    let origins = raw
        .origins
        .iter()
        .filter(|origin| is_named(origin))
        .map(|origin| mcp_origin(origin))
        .collect::<Result<Vec<_>, _>>()?;
    let hosts = raw
        .hosts
        .iter()
        .filter(|host| is_named(host))
        .map(|host| mcp_host(host))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(McpConfig {
        enabled: raw.enabled,
        listen,
        auth: raw.auth,
        writes: raw.writes,
        max_volume_db: raw.max_volume_db,
        origins,
        hosts,
        token_file: raw.token_file.filter(|value| is_named(value)),
    })
}

/// The `[web]` table: the listen address, its auth, the guest level, the volume cap, the
/// public URL, hosts and origins.
fn web(raw: RawWeb) -> Result<WebConfig, ConfigError> {
    let listen: std::net::SocketAddr = raw.listen.trim().parse().map_err(|_| {
        ConfigError::Parse(format!(
            "web.listen {:?} must be an address and port, such as \"127.0.0.1:8738\"",
            raw.listen
        ))
    })?;
    if raw.auth == WebAuth::None && !listen.ip().is_loopback() {
        return Err(ConfigError::Parse(
            "web.auth = \"none\" needs a loopback listen address".to_string(),
        ));
    }
    let guest = match raw.guest.trim().to_ascii_lowercase().as_str() {
        "none" => WebGuest::None,
        "read" => WebGuest::Read,
        _ => {
            return Err(ConfigError::Parse(format!(
                "web.guest {:?} must be \"none\" or \"read\"",
                raw.guest
            )));
        }
    };
    if let Some(cap) = raw.max_volume_db
        && !cap.is_finite()
    {
        return Err(ConfigError::Parse(
            "web.max_volume_db must be a finite number".to_string(),
        ));
    }
    let url = web_url(&raw.url)?;
    let hosts = raw
        .hosts
        .iter()
        .filter(|host| is_named(host))
        .map(|host| host_entry("web.hosts", host, "svolder.home.lan", 8738))
        .collect::<Result<Vec<_>, _>>()?;
    let origins = raw
        .origins
        .iter()
        .filter(|origin| is_named(origin))
        .map(|origin| web_origin(origin))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WebConfig {
        enabled: raw.enabled,
        listen,
        auth: raw.auth,
        writes: raw.writes,
        max_volume_db: raw.max_volume_db,
        guest,
        url,
        hosts,
        origins,
        token_file: raw.token_file.filter(|value| is_named(value)),
    })
}

/// One `[mcp] origins` entry, trimmed. `"null"` is refused: it is the origin of any sandboxed
/// page, a `data:` URL included. An entry needs a scheme and an explicit port (`:*` for any),
/// because rmcp matches a port-less origin on every port.
fn mcp_origin(origin: &str) -> Result<String, ConfigError> {
    let origin = origin.trim();
    if origin.eq_ignore_ascii_case("null") {
        return Err(ConfigError::Parse(
            "mcp.origins: \"null\" would admit any sandboxed page".to_string(),
        ));
    }
    let Some((_, rest)) = origin.split_once("://") else {
        return Err(ConfigError::Parse(format!(
            "mcp.origins: {origin:?} needs a scheme, such as \"https://claude.ai:443\""
        )));
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    let port = match authority.strip_prefix('[') {
        Some(v6) => v6.split_once("]:").map(|(_, port)| port),
        None => authority.rsplit_once(':').map(|(_, port)| port),
    };
    match port {
        Some("*") => Ok(origin.to_string()),
        Some(port) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            Ok(origin.to_string())
        }
        _ => Err(ConfigError::Parse(format!(
            "mcp.origins: {origin:?} needs a port; rmcp matches a port-less origin on every port"
        ))),
    }
}

/// One `hosts` entry of the table `key` names (`mcp.hosts`, `web.hosts`), trimmed: a bare host
/// name or address, with a port when it is to match that port only. A scheme, a path, a
/// wildcard or a space is an error naming the key, with `example` and `port` in the words.
fn host_entry(key: &str, host: &str, example: &str, port: u16) -> Result<String, ConfigError> {
    let host = host.trim();
    if host.contains("://") || host.contains(['/', '*', ' ', '\t', '@', '?', '#']) {
        return Err(ConfigError::Parse(format!(
            "{key} entry {host:?} must be a host name or address, with an optional port \
             (such as \"{example}\" or \"{example}:{port}\")"
        )));
    }
    Ok(host.to_string())
}

/// One `[mcp] hosts` entry, trimmed.
fn mcp_host(host: &str) -> Result<String, ConfigError> {
    host_entry("mcp.hosts", host, "mcp.home.lan", 8737)
}

/// `[web] url`, or `None` while it is blank: an `http://` or `https://` address with a host, no
/// query and no fragment, ending in `/` (one is added).
fn web_url(url: &str) -> Result<Option<String>, ConfigError> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(None);
    }
    let refused = || {
        ConfigError::Parse(format!(
            "web.url {url:?} must be an http:// or https:// address with no query or fragment, \
             such as \"https://svolder.home.lan/\""
        ))
    };
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .ok_or_else(refused)?;
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() || url.contains(['?', '#', ' ']) {
        return Err(refused());
    }
    Ok(Some(match url.ends_with('/') {
        true => url.to_string(),
        false => format!("{url}/"),
    }))
}

/// One `[web] origins` entry, as [`normal_origin`] writes it. `"null"` is refused: it is the
/// origin of any sandboxed page, a `data:` URL included.
fn web_origin(origin: &str) -> Result<String, ConfigError> {
    let origin = origin.trim();
    if origin.eq_ignore_ascii_case("null") {
        return Err(ConfigError::Parse(
            "web.origins: \"null\" would admit any sandboxed page".to_string(),
        ));
    }
    normal_origin(origin).ok_or_else(|| {
        ConfigError::Parse(format!(
            "web.origins: {origin:?} must be a scheme and a host, with a port where it is not \
             the scheme's own, such as \"https://svolder.home.lan\""
        ))
    })
}

/// Refuse a topic prefix every MQTT topic would be built under that makes those topics
/// invalid or not what anyone meant: a `+` or `#` (wildcards, which no published topic may
/// hold), or a `/` at either end. An empty prefix is left to the caller to allow or refuse.
fn topic_prefix(key: &str, value: &str) -> Result<(), ConfigError> {
    if value.contains(['+', '#']) {
        return Err(ConfigError::Parse(format!(
            "{key} {value:?} must not contain + or #, which are MQTT wildcards"
        )));
    }
    if value.starts_with('/') || value.ends_with('/') {
        return Err(ConfigError::Parse(format!(
            "{key} {value:?} must not begin or end with /"
        )));
    }
    Ok(())
}

/// The keys one `[[hop]]` block may carry.
const HOP_KEYS: [&str; 5] = ["name", "kind", "device", "note", "after"];

/// Parse and validate one `[[hop]]` block: exactly one of `name` (which needs a `kind`) and
/// `device`, and the optional `note` and `after`, each a string.
///
/// Read from a raw value rather than a derived `Deserialize` so that every error can name the
/// block, and so that an unknown key is refused with the block's name in the words.
fn parse_hop(value: &toml::Value) -> Result<Hop, ConfigError> {
    let table = value
        .as_table()
        .ok_or_else(|| ConfigError::Parse("[[hop]] is not a table".into()))?;
    let text = |key: &str| -> Result<Option<&str>, ConfigError> {
        match table.get(key) {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .map(Some)
                .ok_or_else(|| ConfigError::Parse(format!("[[hop]] {key} must be a string"))),
        }
    };
    let (name, device) = (text("name")?, text("device")?);
    let called = name.or(device).unwrap_or_default().trim();
    let err = |reason: String| ConfigError::Parse(format!("[[hop]] {called:?}: {reason}"));
    for key in table.keys() {
        if !HOP_KEYS.contains(&key.as_str()) {
            return Err(err(format!("unknown key {key:?}")));
        }
    }
    let kind = text("kind")?;
    let what = match (name, device) {
        (Some(_), Some(_)) => {
            return Err(ConfigError::Parse(
                "[[hop]] names a device or a name, never both: it has both \"device\" and \"name\""
                    .into(),
            ));
        }
        (None, None) => {
            return Err(ConfigError::Parse(
                "[[hop]] needs a \"name\" (with a \"kind\") or a \"device\"".into(),
            ));
        }
        (Some(name), None) => {
            let name = checked_name(name)
                .map_err(|why| err(name_refusal(why)))?
                .to_string();
            let kind =
                kind.ok_or_else(|| err(format!("a named hop needs a kind: {}", HopKind::words())))?;
            let kind = HopKind::named(kind)
                .ok_or_else(|| err(format!("kind must be one of {}", HopKind::words())))?;
            HopWhat::Declared { kind, name }
        }
        (None, Some(device)) => {
            if kind.is_some() {
                return Err(err(
                    "a hop that links a device has no kind of its own".into()
                ));
            }
            let device = checked_name(device)
                .map_err(|why| err(name_refusal(why)))?
                .to_string();
            HopWhat::Linked { device }
        }
    };
    // A blank line says nothing, as a blank value does anywhere in the file.
    let line = |key: &str| {
        text(key).map(|line| {
            line.map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
        })
    };
    Ok(Hop {
        what,
        note: line("note")?,
        after: line("after")?,
    })
}

/// The keys one `[[preset]]` block may carry.
const PRESET_KEYS: [&str; 8] = [
    "name", "bands", "mode", "loudness", "band", "origin", "preamp", "filter",
];

/// The keys only a preset of bands carries.
const BAND_PRESET_KEYS: [&str; 4] = ["bands", "mode", "loudness", "band"];

/// The keys one `filter` entry of an imported preset may carry.
const FILTER_KEYS: [&str; 4] = ["type", "f", "g", "q"];

/// The keys one `band` entry of a preset may carry.
const BAND_KEYS: [&str; 3] = ["f", "g", "q"];

/// Parse and validate one `[[preset]]` block, naming it in every error the way a bad
/// `[[device]]` block is named in its own.
///
/// Validated here rather than left to a derived `Deserialize`: `bands`, `mode` and `loudness`
/// each need a range or word-list check serde alone would not enforce, a non-finite band value
/// (`nan`, `inf`, written as such) parses as a plain `f64` and must be rejected by hand, and an
/// unknown key must be found while `name` is still in scope, so the error can name the preset
/// it is in.
fn parse_preset(value: &toml::Value) -> Result<Preset, ConfigError> {
    let table = value
        .as_table()
        .ok_or_else(|| ConfigError::Parse("preset is not a table".into()))?;
    let written = table
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    let name = match checked_name(written) {
        Ok(name) => name.to_string(),
        Err(NameError::Blank) => return Err(ConfigError::Parse(NameError::Blank.to_string())),
        Err(why) => {
            return Err(ConfigError::Parse(format!(
                "preset {:?}: {why}",
                written.trim()
            )));
        }
    };
    let err = |reason: String| ConfigError::Parse(format!("preset {name:?}: {reason}"));

    for key in table.keys() {
        if !PRESET_KEYS.contains(&key.as_str()) {
            return Err(err(format!("unknown key {key:?}")));
        }
    }
    if is_builtin(&name) {
        return Err(err("that name is a built-in preset's".into()));
    }
    if table.contains_key("filter") {
        return parse_imported(&err, name.clone(), table);
    }
    for key in ["origin", "preamp"] {
        if table.contains_key(key) {
            return Err(err(format!(
                "{key} belongs to an imported preset, which has filter"
            )));
        }
    }

    let bands: u8 = match table.get("bands").and_then(toml::Value::as_integer) {
        Some(n @ (3 | 5)) => n as u8,
        Some(n) => return Err(err(format!("bands must be 3 or 5, not {n}"))),
        None => return Err(err("bands must be 3 or 5".into())),
    };

    let mode_word = table.get("mode").and_then(toml::Value::as_str);
    let mode: EqMode = match mode_word {
        Some(word) => word
            .parse()
            .map_err(|_| err(format!("mode must be flat, eq, or loudness, not {word:?}")))?,
        None => return Err(err("mode must be flat, eq, or loudness".into())),
    };

    let loudness: u8 = match table.get("loudness") {
        None => 0,
        Some(value) => match value.as_integer() {
            Some(n) if (0..=i64::from(LOUDNESS_MAX)).contains(&n) => n as u8,
            Some(n) => return Err(err(format!("loudness must be 0-127, not {n}"))),
            None => return Err(err("loudness must be 0-127".into())),
        },
    };

    let raw_band: Vec<&toml::Value> = match table.get("band") {
        None => Vec::new(),
        Some(value) => match value.as_array() {
            Some(array) => array.iter().collect(),
            None => return Err(err("band must be an array".into())),
        },
    };
    if raw_band.len() != usize::from(bands) {
        return Err(err(format!(
            "bands is {bands} but there are {} band entries",
            raw_band.len()
        )));
    }
    let band = raw_band
        .into_iter()
        .enumerate()
        .map(|(index, entry)| parse_preset_band(&err, index, entry))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Preset {
        name,
        bands,
        mode,
        loudness,
        band,
        source: None,
    })
}

/// Parse and validate the keys of an imported `[[preset]]` block: `origin`, `preamp` and
/// `filter`, with none of the keys a preset of bands carries.
fn parse_imported(
    err: &dyn Fn(String) -> ConfigError,
    name: String,
    table: &toml::map::Map<String, toml::Value>,
) -> Result<Preset, ConfigError> {
    if BAND_PRESET_KEYS.iter().any(|key| table.contains_key(*key)) {
        return Err(err("an imported preset holds filter, not band".into()));
    }
    let origin = match table.get("origin") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .ok_or_else(|| err("origin must be text".into()))?
                .to_string(),
        ),
    };
    let preamp_db = match table.get("preamp") {
        None => 0.0,
        Some(value) => as_f64(value).ok_or_else(|| err("preamp must be a number".into()))?,
    };
    let entries = table
        .get("filter")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| err("filter must be an array".into()))?;
    let filters = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| parse_filter(err, index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    let correction = Correction { preamp_db, filters };
    apo::validate(&correction).map_err(|why| err(why.to_string()))?;
    Ok(Preset {
        name,
        bands: 0,
        mode: EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
        source: Some(Imported { origin, correction }),
    })
}

/// Parse one entry of an imported preset's `filter` array, naming the preset (through `err`)
/// and the filter (counted from 1) in every error. The bounds on its numbers are
/// [`apo::validate`]'s.
fn parse_filter(
    err: &dyn Fn(String) -> ConfigError,
    index: usize,
    value: &toml::Value,
) -> Result<SourceFilter, ConfigError> {
    let at = index + 1;
    let table = value
        .as_table()
        .ok_or_else(|| err(format!("filter {at}: not a table")))?;
    for key in table.keys() {
        if !FILTER_KEYS.contains(&key.as_str()) {
            return Err(err(format!("filter {at}: unknown key {key:?}")));
        }
    }
    let word = table
        .get("type")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| err(format!("filter {at}: type must be PK, LSC or HSC")))?;
    let kind = match word.to_ascii_uppercase().as_str() {
        "PK" | "LSC" | "HSC" => word.parse::<FilterKind>().ok(),
        _ => None,
    }
    .ok_or_else(|| {
        err(format!(
            "filter {at}: type must be PK, LSC or HSC, not {word:?}"
        ))
    })?;
    let get = |field: &str| -> Result<f64, ConfigError> {
        let raw = table
            .get(field)
            .ok_or_else(|| err(format!("filter {at}: {field} is required")))?;
        as_f64(raw).ok_or_else(|| err(format!("filter {at}: {field} must be a number")))
    };
    Ok(SourceFilter {
        kind,
        f: get("f")?,
        g: get("g")?,
        q: get("q")?,
    })
}

/// Parse and validate one entry of a preset's `band` array, naming both the preset (through
/// `err`) and the band (counted from 1, as [`preset::Clamp`](svolder_profiles::tone::preset::Clamp)
/// counts them) in every error.
fn parse_preset_band(
    err: &dyn Fn(String) -> ConfigError,
    index: usize,
    value: &toml::Value,
) -> Result<PresetBand, ConfigError> {
    let table = value
        .as_table()
        .ok_or_else(|| err(format!("band {}: not a table", index + 1)))?;
    for key in table.keys() {
        if !BAND_KEYS.contains(&key.as_str()) {
            return Err(err(format!("band {}: unknown key {key:?}", index + 1)));
        }
    }
    let get = |field: &str| -> Result<f64, ConfigError> {
        let raw = table
            .get(field)
            .ok_or_else(|| err(format!("band {}: {field} is required", index + 1)))?;
        let n = as_f64(raw)
            .ok_or_else(|| err(format!("band {}: {field} must be a number", index + 1)))?;
        if !n.is_finite() {
            return Err(err(format!(
                "band {}: {field} must be finite, not {n}",
                index + 1
            )));
        }
        Ok(n)
    };
    Ok(PresetBand {
        f: get("f")?,
        g: get("g")?,
        q: get("q")?,
    })
}

/// A TOML number as `f64`, whether it is written as an integer or a float.
fn as_f64(value: &toml::Value) -> Option<f64> {
    match value {
        toml::Value::Integer(n) => Some(*n as f64),
        toml::Value::Float(n) => Some(*n),
        _ => None,
    }
}

/// The palette presets `svolder tui` ships, in the order the error lists them.
pub(super) const PALETTES: [&str; 4] = ["amber", "ice", "red", "neon"];

/// The palette a config that names none draws from.
pub(super) const DEFAULT_PALETTE: &str = "amber";

/// A `[tui]` `palette` names one of the presets that ship.
fn validate_palette(palette: &str) -> Result<(), ConfigError> {
    if PALETTES.contains(&palette) {
        return Ok(());
    }
    Err(ConfigError::Parse(format!(
        "tui.palette must be one of {}",
        PALETTES.join(", ")
    )))
}

/// A `[tui]` `transition` names one of the four ways a screen gives way to the next.
fn parse_transition(word: &str) -> Result<Transition, ConfigError> {
    Transition::named(word).ok_or_else(|| {
        let words: Vec<&str> = Transition::ALL.iter().map(|t| t.name()).collect();
        ConfigError::Parse(format!(
            "tui.transition must be one of {}",
            words.join(", ")
        ))
    })
}

/// A `[tui]` `intro_effect` names one of the effects that build the wordmark.
fn parse_intro_effect(word: &str) -> Result<IntroEffect, ConfigError> {
    IntroEffect::named(word).ok_or_else(|| {
        let words: Vec<&str> = IntroEffect::ALL.iter().map(|e| e.name()).collect();
        ConfigError::Parse(format!(
            "tui.intro_effect must be one of {}",
            words.join(", ")
        ))
    })
}

/// A `[tui]` `graph_span` names one of the fixed spans of the Telemetry and level graphs.
fn parse_graph_span(word: &str) -> Result<GraphSpan, ConfigError> {
    GraphSpan::named(word).ok_or_else(|| {
        let words: Vec<&str> = GraphSpan::ALL.iter().map(|span| span.name()).collect();
        ConfigError::Parse(format!(
            "tui.graph_span must be one of {}",
            words.join(", ")
        ))
    })
}

/// A `[tui]` `link_span` names one of the fixed spans of the connection strip.
fn parse_link_span(word: &str) -> Result<LinkSpan, ConfigError> {
    LinkSpan::named(word).ok_or_else(|| {
        let words: Vec<&str> = LinkSpan::ALL.iter().map(|span| span.name()).collect();
        ConfigError::Parse(format!("tui.link_span must be one of {}", words.join(", ")))
    })
}

/// The 16 ANSI colour names, lowercase, without a `bright-` prefix.
const ANSI_COLOUR_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// A `[tui]` `accent` is one of the 16 ANSI colour names (case-insensitive, with an optional
/// `bright-` prefix) or a `#rrggbb` value.
fn validate_accent(accent: &str) -> Result<(), ConfigError> {
    let lower = accent.to_ascii_lowercase();
    let name = lower.strip_prefix("bright-").unwrap_or(&lower);
    if ANSI_COLOUR_NAMES.contains(&name) {
        return Ok(());
    }
    if let Some(hex) = accent.strip_prefix('#')
        && hex.len() == 6
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(());
    }
    Err(ConfigError::Parse(format!(
        "accent {accent:?} is not an ANSI colour name or #rrggbb"
    )))
}

/// A `[tui.spectrum]` stop is a `#rrggbb` value: the stops are blended into one another, and
/// an ANSI colour name is a terminal slot rather than a colour there is anything to blend
/// from.
fn validate_stop(stop: &str) -> Result<(), ConfigError> {
    if let Some(hex) = stop.strip_prefix('#')
        && hex.len() == 6
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(());
    }
    Err(ConfigError::Parse(format!(
        "tui.spectrum stop {stop:?} is not a #rrggbb colour"
    )))
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
