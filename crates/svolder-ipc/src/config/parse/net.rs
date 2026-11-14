// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[mqtt]`, `[mcp]` and `[web]` tables: their on-disk shapes, the listen defaults the
//! environment gives, and the checks on their keys.

use super::super::{
    ConfigError, McpAuth, McpConfig, MqttConfig, WebAuth, WebConfig, WebGuest, normal_origin,
};
use super::is_named;

/// The on-disk shape of the `[mqtt]` table, before validation. `port` is kept wider than
/// `u16` and `max_volume_db` wider than "must be finite" so an out-of-range value reaches
/// `mqtt`'s own check and its message naming `mqtt.port` or `mqtt.max_volume_db`,
/// rather than an opaque error from the TOML parser.
#[derive(serde::Deserialize)]
#[serde(default)]
pub(super) struct RawMqtt {
    /// Absent leaves the choice to `SVOLDER_MQTT_ENABLED`, and to off without it.
    enabled: Option<bool>,
    host: Option<String>,
    /// Absent leaves the port to `SVOLDER_MQTT_PORT`, and to 1883 without it.
    port: Option<i64>,
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
            enabled: None,
            host: d.host,
            port: None,
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
/// address reaches `mcp`'s own message naming `mcp.listen`; blank means the key is absent.
#[derive(serde::Deserialize)]
#[serde(default)]
pub(super) struct RawMcp {
    /// Absent leaves the choice to `SVOLDER_MCP_ENABLED`, and to off without it.
    enabled: Option<bool>,
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
            enabled: None,
            listen: String::new(),
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
pub(super) struct RawWeb {
    /// Absent leaves the choice to `SVOLDER_WEB_ENABLED`, and to off without it.
    enabled: Option<bool>,
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
            enabled: None,
            listen: String::new(),
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

/// What the environment gives `[web]` and `[mcp]` where the file has no key of its own. The
/// addresses they listen on, in place of loopback: `SVOLDER_WEB_LISTEN` and `SVOLDER_MCP_LISTEN`,
/// which the container image sets so a published port reaches the server. Whether they run at
/// all, in place of off: `SVOLDER_WEB_ENABLED` and `SVOLDER_MCP_ENABLED`, so a container's
/// compose file turns a server on with no configuration file written. A key in the file wins
/// over its variable, and a blank value counts as unset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListenDefaults {
    /// The web UI's default address.
    pub web: Option<String>,
    /// The MCP server's default address.
    pub mcp: Option<String>,
    /// The web UI's default `url` (`SVOLDER_WEB_URL`), the address links and QR codes carry.
    pub web_url: Option<String>,
    /// The web UI's default `enabled` (`SVOLDER_WEB_ENABLED`), as the variable spells it.
    pub web_enabled: Option<String>,
    /// The MCP server's default `enabled` (`SVOLDER_MCP_ENABLED`), as the variable spells it.
    pub mcp_enabled: Option<String>,
    /// The MQTT bridge's default `enabled` (`SVOLDER_MQTT_ENABLED`), as the variable spells it.
    pub mqtt_enabled: Option<String>,
    /// The broker's default `host` (`SVOLDER_MQTT_HOST`).
    pub mqtt_host: Option<String>,
    /// The broker's default `port` (`SVOLDER_MQTT_PORT`), as the variable spells it.
    pub mqtt_port: Option<String>,
    /// The broker's default `username` (`SVOLDER_MQTT_USERNAME`).
    pub mqtt_username: Option<String>,
}

impl ListenDefaults {
    /// Read the variables.
    pub fn from_env() -> ListenDefaults {
        let read = |name: &str| std::env::var(name).ok().filter(|v| is_named(v));
        ListenDefaults {
            web: read("SVOLDER_WEB_LISTEN"),
            mcp: read("SVOLDER_MCP_LISTEN"),
            web_url: read("SVOLDER_WEB_URL"),
            web_enabled: read("SVOLDER_WEB_ENABLED"),
            mcp_enabled: read("SVOLDER_MCP_ENABLED"),
            mqtt_enabled: read("SVOLDER_MQTT_ENABLED"),
            mqtt_host: read("SVOLDER_MQTT_HOST"),
            mqtt_port: read("SVOLDER_MQTT_PORT"),
            mqtt_username: read("SVOLDER_MQTT_USERNAME"),
        }
    }
}

/// The address a table listens on: its own `listen` key, else the environment's default, else
/// `builtin`; and whether it came from the environment.
fn listen_address(
    own: &str,
    env_default: Option<&str>,
    env_name: &str,
    table: &str,
    builtin: std::net::SocketAddr,
) -> Result<(std::net::SocketAddr, bool), ConfigError> {
    let (text, label, from_env) = if is_named(own) {
        (own, format!("{table}.listen"), false)
    } else if let Some(text) = env_default {
        (text, env_name.to_string(), true)
    } else {
        return Ok((builtin, false));
    };
    let address = text.trim().parse().map_err(|_| {
        ConfigError::Parse(format!(
            "{label} {text:?} must be an address and port, such as \"{builtin}\""
        ))
    })?;
    Ok((address, from_env))
}

/// Whether a table is on: its own `enabled` key, else the environment's default, else off. The
/// variable reads `1`, `true`, `yes` and `on` as on and `0`, `false`, `no` and `off` as off,
/// in any case; another value is refused by the variable's name.
fn enabled_switch(
    own: Option<bool>,
    env_default: Option<&str>,
    env_name: &str,
) -> Result<bool, ConfigError> {
    let Some(text) = env_default.filter(|_| own.is_none()) else {
        return Ok(own.unwrap_or(false));
    };
    match text.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::Parse(format!(
            "{env_name} {text:?} must be 1 or 0 (true, false, yes, no, on and off are read too)"
        ))),
    }
}

/// The refusal of `auth = "none"` on an address that is not loopback, or `Ok`. An address the
/// environment supplied is refused only for a table that is enabled, and the refusal names the
/// variable, since the file has no `listen` to point at.
fn refuse_open_auth(
    table: &str,
    env_name: &str,
    listen: std::net::SocketAddr,
    from_env: bool,
    enabled: bool,
) -> Result<(), ConfigError> {
    if listen.ip().is_loopback() || (from_env && !enabled) {
        return Ok(());
    }
    Err(ConfigError::Parse(if from_env {
        format!(
            "{table}.auth = \"none\" needs a loopback listen address, and {env_name} gives \
             {listen}: write a loopback `listen` in the [{table}] table, or unset {env_name}"
        )
    } else {
        format!("{table}.auth = \"none\" needs a loopback listen address")
    }))
}

/// The `[mqtt]` table: the broker, the volume cap and the two topic prefixes.
///
/// `enabled`, `host`, `port` and `username` take the environment's default where the file has no
/// key of its own, as `[web]` does; the password's variable is read where the daemon connects.
pub(super) fn mqtt(raw: RawMqtt, env: &ListenDefaults) -> Result<MqttConfig, ConfigError> {
    let enabled = enabled_switch(
        raw.enabled,
        env.mqtt_enabled.as_deref(),
        "SVOLDER_MQTT_ENABLED",
    )?;
    let host = raw
        .host
        .filter(|value| is_named(value))
        .or_else(|| env.mqtt_host.clone());
    let username = raw
        .username
        .filter(|value| is_named(value))
        .or_else(|| env.mqtt_username.clone());
    let port = match (raw.port, env.mqtt_port.as_deref()) {
        (Some(port), _) => port,
        (None, Some(text)) => text.trim().parse::<i64>().map_err(|_| {
            ConfigError::Parse(format!(
                "SVOLDER_MQTT_PORT {text:?} must be a port between 1 and 65535"
            ))
        })?,
        (None, None) => i64::from(MqttConfig::default().port),
    };
    let password = raw.password.filter(|value| is_named(value));
    let client_id = raw.client_id.filter(|value| is_named(value));
    if enabled && host.is_none() {
        return Err(ConfigError::Parse(
            "mqtt.host must not be empty when enabled = true".to_string(),
        ));
    }
    if !(1..=65535).contains(&port) {
        let label = match raw.port {
            Some(_) => "mqtt.port",
            None => "SVOLDER_MQTT_PORT",
        };
        return Err(ConfigError::Parse(format!(
            "{label} {port} must be between 1 and 65535"
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
        enabled,
        host,
        // Validated above to be in range, so this narrowing never truncates.
        port: port as u16,
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
pub(super) fn mcp(raw: RawMcp, env: &ListenDefaults) -> Result<McpConfig, ConfigError> {
    let enabled = enabled_switch(
        raw.enabled,
        env.mcp_enabled.as_deref(),
        "SVOLDER_MCP_ENABLED",
    )?;
    let (listen, from_env) = listen_address(
        &raw.listen,
        env.mcp.as_deref(),
        "SVOLDER_MCP_LISTEN",
        "mcp",
        McpConfig::default().listen,
    )?;
    if raw.auth == McpAuth::None {
        refuse_open_auth("mcp", "SVOLDER_MCP_LISTEN", listen, from_env, enabled)?;
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
        enabled,
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
pub(super) fn web(raw: RawWeb, env: &ListenDefaults) -> Result<WebConfig, ConfigError> {
    let enabled = enabled_switch(
        raw.enabled,
        env.web_enabled.as_deref(),
        "SVOLDER_WEB_ENABLED",
    )?;
    let env_url = env.web_url.as_deref();
    let (listen, from_env) = listen_address(
        &raw.listen,
        env.web.as_deref(),
        "SVOLDER_WEB_LISTEN",
        "web",
        WebConfig::default().listen,
    )?;
    if raw.auth == WebAuth::None {
        refuse_open_auth("web", "SVOLDER_WEB_LISTEN", listen, from_env, enabled)?;
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
    let url = match (is_named(&raw.url), env_url) {
        (false, Some(env)) => web_url(env, "SVOLDER_WEB_URL")?,
        _ => web_url(&raw.url, "web.url")?,
    };
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
        enabled,
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
fn web_url(url: &str, label: &str) -> Result<Option<String>, ConfigError> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(None);
    }
    let refused = || {
        ConfigError::Parse(format!(
            "{label} {url:?} must be an http:// or https:// address with no query or fragment, \
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
