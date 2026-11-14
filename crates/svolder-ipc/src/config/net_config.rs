// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[mqtt]`, `[mcp]` and `[web]` tables as the configuration holds them.

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
