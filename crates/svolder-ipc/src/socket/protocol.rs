// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wire protocol: request and response types, and serialization helpers.
//!
//! A daemon and a front end of different builds talk to each other, so a peer tolerates what
//! it does not know. Every field added after the first release is `#[serde(default)]` (and
//! `skip_serializing_if` where absent keeps the old meaning), so an older peer's message still
//! decodes. A peer ignores a field it does not know. A message kind a peer does not know
//! (a response, an event, a reply) decodes as `Unknown` and is skipped, not fatal. A request
//! kind a daemon does not know is answered with an error and the connection stays open.

use serde::{Deserialize, Serialize};

/// One reading, carrying everything a front end needs to print it as text or JSON: the
/// function it came from, the profile's name and label for it, the raw wire value, and the
/// converted value with its unit when the control has one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueObject {
    /// "10", "210.2.1"
    pub id: String,
    /// profile key, None for an unnamed function
    pub key: Option<String>,
    /// "Volume", or "?"
    pub label: String,
    /// "u8", "bool", ...
    pub value_type: String,
    /// 109, true, or "aa bb" for Data
    pub raw: serde_json::Value,
    /// "109 (-6.0 dB)", "on"
    pub display: String,
    /// converted numeric value, when the control has one
    pub value: Option<f64>,
    /// "dB"
    pub unit: Option<String>,
    /// when the daemon last heard this reading from the device, in unix milliseconds; a reading
    /// kept from before a disconnect keeps its old stamp. Absent when the daemon does not know,
    /// and from a daemon too old to stamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heard_at_ms: Option<u64>,
}

/// One configured device's identity and live connection state (for `devices`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// config name, or the unit id when unnamed
    pub name: String,
    /// unit id
    pub unit: String,
    /// advertised product id, which a profile registry selects on
    pub product_id: u16,
    /// product name
    pub product: Option<String>,
    /// firmware version
    pub firmware: u16,
    /// the profile the daemon resolved for this device
    pub profile: String,
    /// true if connected
    pub connected: bool,
    /// true if stale
    pub stale: bool,
    /// whole seconds until the daemon's next connect attempt, while it is waiting between
    /// attempts; absent while a link is up or an attempt is running
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_in_secs: Option<u64>,
    /// how the configured passphrase disagrees with the advertisement the daemon holds, as a
    /// best-effort note; absent when it does not, and from a daemon too old to judge
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase_note: Option<crate::PassphraseNote>,
    /// true while the link the daemon holds to the device is one the device has refused the
    /// passphrase on; absent otherwise, and from a daemon too old to say
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refused: bool,
    /// the preset last recalled on the device, by any front end, since the daemon started;
    /// absent when none has been, and from a daemon too old to keep one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recalled: Option<String>,
    /// how strong the device's radio signal last was, where it was heard from and when; absent
    /// when the daemon has never heard the device advertise with a signal strength, and from a
    /// daemon too old to keep one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<crate::socket::signal::Signal>,
}

/// One device seen in a scan (the `scan` verb's row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanEntry {
    /// unit id
    pub unit: String,
    /// advertised product id, which a profile registry selects on
    #[serde(default)]
    pub product_id: u16,
    /// product name
    pub product: Option<String>,
    /// firmware version
    pub firmware: u16,
    /// true if passphrase required
    pub passphrase: bool,
    /// received signal strength in dBm, when the adapter reports it
    #[serde(default)]
    pub rssi: Option<i16>,
}

/// A request from a front end. `device` selects a configured device (name, product name, or
/// unit-id prefix); `None` means the single/default device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    /// List devices
    Devices,
    /// Get device state
    State {
        /// device selector
        device: Option<String>,
    },
    /// Get a single value
    Get {
        /// device selector
        device: Option<String>,
        /// key to retrieve
        key: String,
    },
    /// Set a value
    Set {
        /// device selector
        device: Option<String>,
        /// key to set
        key: String,
        /// value as string
        value: String,
        /// step size
        step: i64,
        /// don't wait for response
        no_wait: bool,
    },
    /// Set by raw function ID and type
    Raw {
        /// device selector
        device: Option<String>,
        /// function ID
        id: String,
        /// value type
        value_type: String,
        /// value as string
        value: String,
        /// don't wait for response
        no_wait: bool,
    },
    /// The connection transitions of a device within the last `span_secs` seconds, plus the
    /// one before them: how the link has come and gone, for a front end's connection strip
    History {
        /// device selector
        device: Option<String>,
        /// how far back to look, in seconds; the daemon holds at most a day
        span_secs: u64,
    },
    /// The last `span_secs` seconds of a device's readings (at most
    /// [`READINGS_SPAN_SECS`]), its recalls and its link changes, for a front end's graphs to
    /// start from. Each series and the link's changes begin with the newest point from before
    /// the span, when there is one, stamped at its own time, which says what held when the span
    /// began
    Readings {
        /// device selector
        device: Option<String>,
        /// how far back to look, in seconds
        span_secs: u64,
    },
    /// Note that a front end recalled the preset `name` on a device by its own writes, so every
    /// other front end shows it as the one that is on while the device still matches it
    Recalled {
        /// device selector
        device: Option<String>,
        /// the preset's name
        name: String,
    },
    /// A pairing code for the web UI: six digits, good for one sign-in within five minutes
    WebPair,
    /// A link code for the web UI: one sign-in within five minutes, carried in a link's fragment
    WebLink,
    /// Whether a link code for the web UI is still open, which spends nothing: a front end that
    /// shows a link asks it to learn that a browser has used the code
    WebLinkOpen {
        /// the code, as `web_link` answered it
        code: String,
    },
    /// Show EQ state
    EqShow {
        /// device selector
        device: Option<String>,
    },
    /// Set EQ parameter
    EqSet {
        /// device selector
        device: Option<String>,
        /// band index
        band: usize,
        /// parameter name
        param: String,
        /// parameter value
        value: f64,
        /// don't wait for response; absent on the wire means wait
        #[serde(default)]
        no_wait: bool,
    },
    /// Set EQ mode
    EqMode {
        /// device selector
        device: Option<String>,
        /// mode name
        mode: String,
        /// don't wait for response; absent on the wire means wait
        #[serde(default)]
        no_wait: bool,
    },
    /// Set passphrase
    Passphrase {
        /// device selector
        device: Option<String>,
        /// passphrase digits
        digits: String,
    },
    /// Ask a device to show itself (function 200), over the link the daemon holds; at most one
    /// frame goes out every two seconds per device
    Identify {
        /// device selector
        device: Option<String>,
    },
    /// Scan for devices
    Scan {
        /// scan window in milliseconds
        window_ms: u64,
    },
    /// Subscribe to device events
    Subscribe {
        /// device selector
        device: Option<String>,
        /// whether the stream also carries the host's view (`Event::Host`, once after the
        /// connection snapshot and then on each change) and holds the daemon's route reads
        /// open for as long as it lives; a daemon from before it ignores the field and never
        /// sends the events
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        host: bool,
    },
    /// Re-read the configuration file and the profiles, reconciling the running devices with
    /// what the file now says
    Reload,
    /// The version of the daemon answering
    Version,
    /// Stop the daemon, as a termination signal would
    Stop,
    /// What the daemon knows of the computer's music player and the route its sound takes to
    /// the default sink
    Host,
    /// Press one of the followed player's transport buttons; answered with an acknowledgement
    /// when the player took it
    HostPress {
        /// `previous`, `play`, `pause` or `next`; any other word is the daemon's error to give
        button: String,
    },
    /// Turn the connection into a stream of the daemon's analysed audio frames
    /// (`Event::HostCapture` once and on each change of the capture, `Event::HostFrame` while it
    /// runs), holding the daemon's capture of the default sink for as long as the connection
    /// lives. A daemon from before it, or one with no host audio, answers an error instead.
    HostFrames,
}

/// The longest window a daemon listens for on a `scan` request. A longer one asked for is
/// listened for this long, and the front end that asks says so before it does.
pub const MAX_SCAN_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);

/// The message of the [`Response::Error`] a daemon answers a device request with while its
/// configuration names no device at all. A front end that reads exactly these words knows the
/// daemon is up and empty, and says so in its own terms.
pub const NO_DEVICES_CONFIGURED: &str = "no devices are configured";

/// How the words end that a daemon answers a device request with, as code 3, for a device it has
/// never found: [`not_found_yet`] puts the device's name before them. A front end that reads
/// them in an answer knows the daemon is still looking for the device, which a lost link is not.
pub const NOT_FOUND_YET: &str = " has not been found yet";

/// The words for the device `device`, which the daemon has never found: its name, then
/// [`NOT_FOUND_YET`]. The daemon says them and the front ends that read them take them apart
/// again through this and that constant, so the two sides cannot come to differ.
pub fn not_found_yet(device: &str) -> String {
    format!("{device}{NOT_FOUND_YET}")
}

/// The message of the code-3 [`Response::Error`] a daemon answers a command with while it holds no
/// link to the device. A front end that reads exactly these words knows the device is away, and
/// adds when the daemon tries it again.
pub const NOT_CONNECTED: &str = "device not connected";

/// The words a daemon answers a web code request with while its web UI is off, naming the fix.
pub const WEB_OFF: &str = "the web UI is off: turn it on in the TUI's Settings, Connect, Web UI, \
                           or set [web] enabled = true and run svolder daemon restart";

/// Response to a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    /// The readings a `state` or `eq_show` request returns.
    Values {
        /// the name of the device the daemon resolved the request to
        device: String,
        /// list of values
        values: Vec<ValueObject>,
    },
    /// The confirmed reading a `get`, `set`, `raw`, `eq_set` or `eq_mode` request returns.
    Value(ValueObject),
    /// List of devices
    Devices {
        /// list of devices
        devices: Vec<DeviceInfo>,
    },
    /// The connection history a `history` request returns.
    History {
        /// the name of the device the daemon resolved the request to
        device: String,
        /// the daemon's wall clock when it answered, in unix milliseconds
        now_ms: u64,
        /// the transitions inside the span and the one before it, oldest first
        transitions: Vec<LinkChange>,
    },
    /// The readings a `readings` request returns.
    Readings {
        /// the name of the device the daemon resolved the request to
        device: String,
        /// the daemon's wall clock when it answered, in unix milliseconds
        now_ms: u64,
        /// the width of one sample's bin, in milliseconds
        bin_ms: u64,
        /// one series per function that has had a numeric reading up to the span's end: the
        /// newest point from before the span first (older than the span), then the points in it
        series: Vec<SeriesOut>,
        /// the presets recalled in the span, oldest first
        recalls: Vec<RecallOut>,
        /// the link's changes in the span and the one before them, oldest first
        link: Vec<LinkChange>,
    },
    /// A pairing code, and how long it lasts.
    WebPair {
        /// the six digits
        code: String,
        /// whole seconds it stays good
        expires_in_secs: u64,
    },
    /// A link code, and how long it lasts.
    WebLink {
        /// the code, 22 base64url characters
        code: String,
        /// whole seconds it stays good
        expires_in_secs: u64,
    },
    /// Whether a link code is still open: it has not been used and has not lasted its time.
    WebLinkOpen {
        /// true while a browser can still sign in with it
        open: bool,
    },
    /// Scan results
    Scan {
        /// list of scan entries
        entries: Vec<ScanEntry>,
    },
    /// The reply to a no-wait `set`/`raw` and to `passphrase`.
    Ack,
    /// The reply to `identify`.
    Identify {
        /// the name of the device the daemon resolved the request to
        device: String,
        /// true when a frame went out; false when one went out within the last two seconds and
        /// this request sent nothing
        sent: bool,
    },
    /// The devices the daemon runs after a `reload`, in the order the file names them.
    Reloaded {
        /// the configured name of every device now running
        devices: Vec<String>,
    },
    /// The version of the daemon answering, and where its MQTT bridge, its MCP server and its web
    /// UI stand.
    Version {
        /// the daemon's version, as its package declares it
        version: String,
        /// the daemon's MQTT bridge; absent from a daemon too old to have one
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mqtt: Option<MqttStatus>,
        /// the daemon's MCP server; absent from a daemon too old to have one
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mcp: Option<McpStatus>,
        /// the configuration file the daemon reads, as a path on the daemon's side; absent from
        /// a daemon too old to say, and what tells a front end the daemon reads another file
        /// than its own (a container's)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        config: Option<String>,
        /// where the daemon's log goes: a file's path on the daemon's side, or `journal`;
        /// absent from a daemon too old to say and from one logging to a terminal
        #[serde(default, skip_serializing_if = "Option::is_none")]
        log: Option<String>,
        /// where the running MCP server's token comes from and its last characters, never the
        /// token; present only while a server runs with a token, and only in this reply
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mcp_token: Option<TokenInfo>,
        /// the daemon's web UI; absent from a daemon too old to have one
        #[serde(default, skip_serializing_if = "Option::is_none")]
        web: Option<WebStatus>,
        /// where the running web UI's token comes from and its last characters, never the
        /// token; present only while the web UI runs with a token, and only in this reply
        #[serde(default, skip_serializing_if = "Option::is_none")]
        web_token: Option<TokenInfo>,
    },
    /// The reply to `stop`, sent before the daemon ends its accept loop.
    Stopping,
    /// The reply to `host`: the player and the route as they stand.
    Host(crate::host::HostView),
    /// Error response
    Error {
        /// error code
        code: u8,
        /// error message
        message: String,
    },
    /// A kind this build does not know, from a peer newer than it.
    #[serde(other)]
    Unknown,
}

/// The longest span a `readings` request is answered for, in seconds: what the daemon keeps.
pub const READINGS_SPAN_SECS: u64 = 30 * 60;

/// One function's readings in a `readings` answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesOut {
    /// the function id, dotted
    pub id: String,
    /// the profile key, None for a function the profile does not name
    pub key: Option<String>,
    /// how the samples in one bin were joined
    pub aggregate: svolder_profiles::history::Aggregate,
    /// `(unix milliseconds, value)`, oldest first; a bool reads 0 or 1
    pub points: Vec<(u64, f64)>,
}

/// One recall in a `readings` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallOut {
    /// when the recall started, in unix milliseconds
    pub at_ms: u64,
    /// the preset's name
    pub name: String,
}

/// One change of a device's link, as the daemon saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkChange {
    /// the daemon's wall clock at the change, in unix milliseconds
    pub at_ms: u64,
    /// the link state from then on
    pub connected: bool,
}

/// Where a daemon's MQTT bridge stands, as `version` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MqttStatus {
    /// No `[mqtt]` section turns the bridge on, so the daemon opens no connection to a broker.
    Off,
    /// Connecting to the broker, or waiting to try it again.
    Connecting {
        /// the broker's host, as the config names it
        host: String,
        /// the broker's port
        port: u16,
        /// whole seconds until the next attempt, while waiting between attempts; absent while
        /// an attempt is under way
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_in_secs: Option<u64>,
    },
    /// Connected to the broker.
    Connected {
        /// the broker's host, as the config names it
        host: String,
        /// the broker's port
        port: u16,
        /// whether the bridge takes commands back
        commands: bool,
    },
    /// A state this client does not know, from a daemon newer than it: read as this, so the
    /// rest of the reply still decodes.
    #[serde(other)]
    Unknown,
}

/// Where a daemon's MCP or web token comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    /// The server's token variable (`SVOLDER_MCP_TOKEN` or `SVOLDER_WEB_TOKEN`) in the daemon's
    /// environment.
    Environment,
    /// The file the server's `token_file` (`[mcp]` or `[web]`) names.
    File,
    /// The file svolder generated in the daemon's state home.
    State,
}

/// What a daemon says of the token its MCP or web server uses: where it comes from and its last
/// three characters (none for a short token), which lets a front end tell whether it reads the
/// same token. Never the token itself, and never anywhere but the `version` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenInfo {
    /// the source kind
    pub kind: TokenKind,
    /// the file's path, on the daemon's side, for a file source
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// the token's last three characters, or empty for a token under 16 characters
    pub tail: String,
}

/// Where a daemon's MCP server stands, as `version` reports it.
///
/// Nothing here is a secret: the address, how requests prove themselves, whether writes are
/// offered, or why the address could not be bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum McpStatus {
    /// No `[mcp]` section turns the server on, so the daemon opens no port.
    Off,
    /// Serving on `listen`.
    Listening {
        /// the address the server is bound to, with the port the system chose when the config
        /// asked for 0
        listen: std::net::SocketAddr,
        /// how a request proves who sent it
        auth: crate::config::McpAuth,
        /// whether the tools that change a device are offered
        writes: bool,
    },
    /// The server is on in the config but is not serving: the address could not be bound, or
    /// the token could not be had.
    Failed {
        /// the address the config asks for
        listen: std::net::SocketAddr,
        /// why, in words that never carry a token
        error: String,
    },
    /// A state this client does not know, from a daemon newer than it: read as this, so the
    /// rest of the reply still decodes.
    #[serde(other)]
    Unknown,
}

impl McpStatus {
    /// The status as `svolder daemon status` and the daemon's log say it: `mcp: off`;
    /// `mcp: listening on ADDR (token, read-only)`, with `no auth` for `auth = "none"` and
    /// `writes` where the write tools are offered; `mcp: cannot listen on ADDR: WHY`; or
    /// `mcp: unknown` for a state a newer daemon reports that this one does not know.
    pub fn line(&self) -> String {
        match self {
            McpStatus::Off => "mcp: off".to_string(),
            McpStatus::Listening {
                listen,
                auth,
                writes,
            } => format!(
                "mcp: listening on {listen} ({}, {})",
                match auth {
                    crate::config::McpAuth::Token => "token",
                    crate::config::McpAuth::None => "no auth",
                },
                match writes {
                    true => "writes",
                    false => "read-only",
                }
            ),
            McpStatus::Failed { listen, error } => {
                format!("mcp: cannot listen on {listen}: {error}")
            }
            McpStatus::Unknown => "mcp: unknown".to_string(),
        }
    }
}

/// Where a daemon's web UI stands, as `version` reports it. Nothing here is a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WebStatus {
    /// No `[web]` section turns it on, so the daemon opens no port.
    Off,
    /// Serving on `listen`.
    Listening {
        /// the address the server is bound to, with the port the system chose when the config
        /// asked for 0
        listen: std::net::SocketAddr,
        /// how a browser proves who it is
        auth: crate::config::WebAuth,
        /// whether the page may change a device
        writes: bool,
        /// what a visitor without a sign-in gets
        guest: crate::config::WebGuest,
    },
    /// On in the config but not serving: the address could not be bound, or the token could
    /// not be had.
    Failed {
        /// the address the config asks for
        listen: std::net::SocketAddr,
        /// why, in words that never carry a token
        error: String,
    },
    /// A state this client does not know, from a daemon newer than it.
    #[serde(other)]
    Unknown,
}

impl WebStatus {
    /// The status as `svolder daemon status` and the daemon's log say it: `web: off`;
    /// `web: listening on ADDR (token, writes)`, with `no auth` for `auth = "none"`,
    /// `read-only` where writes are off and `, guest read` where guests may look;
    /// `web: cannot listen on ADDR: WHY`; or `web: unknown`.
    pub fn line(&self) -> String {
        match self {
            WebStatus::Off => "web: off".to_string(),
            WebStatus::Listening {
                listen,
                auth,
                writes,
                guest,
            } => format!(
                "web: listening on {listen} ({}, {}{})",
                match auth {
                    crate::config::WebAuth::Token => "token",
                    crate::config::WebAuth::None => "no auth",
                },
                match writes {
                    true => "writes",
                    false => "read-only",
                },
                match guest {
                    crate::config::WebGuest::Read => ", guest read",
                    crate::config::WebGuest::None => "",
                }
            ),
            WebStatus::Failed { listen, error } => {
                format!("web: cannot listen on {listen}: {error}")
            }
            WebStatus::Unknown => "web: unknown".to_string(),
        }
    }
}

/// An event pushed to a subscriber.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// One reading the device reported
    Status {
        /// the device the reading came from
        device: String,
        /// the reading itself
        reading: ValueObject,
    },
    /// Connection state changed
    Connection {
        /// device name
        device: String,
        /// connected state
        connected: bool,
    },
    /// Device fault
    Fault {
        /// device name
        device: String,
        /// fault label
        label: String,
    },
    /// Error event
    Error {
        /// error message
        message: String,
    },
    /// A preset was recalled on the device, by any front end: the daemon-wide memory changed
    Recalled {
        /// device name
        device: String,
        /// the preset's name
        name: String,
    },
    /// The daemon's climb of a level started, was steered, or ended
    Climb {
        /// device name
        device: String,
        /// the level's profile key
        key: String,
        /// where it climbs to; absent once it has ended
        target: Option<i64>,
    },
    /// The player or the route changed. Sent on a subscription that asked for the host
    /// (`host: true`), whatever device it names: it belongs to the daemon, not to a device.
    Host(crate::host::HostView),
    /// The daemon's capture started, ended or changed sink. The first event of a `HostFrames`
    /// stream, and sent on each change after it.
    HostCapture(crate::host::CaptureView),
    /// One analysed frame of what the computer plays, about 30 a second while the capture runs.
    /// Only on a `HostFrames` stream.
    HostFrame(crate::host::FrameView),
    /// A kind this build does not know, from a peer newer than it.
    #[serde(other)]
    Unknown,
}

/// One line the client sends: a correlation id and a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestLine {
    /// correlation id
    pub id: u64,
    /// the request
    pub request: Request,
}

/// One line the server sends back, tagged so the client can tell a reply from an event; both
/// carry the id of the request they answer (`Subscribe`'s id for every event on that stream).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "msg", rename_all = "snake_case")]
pub enum Reply {
    /// Response to a request
    Response {
        /// correlation id
        id: u64,
        /// the response
        response: Response,
    },
    /// Pushed event
    Event {
        /// correlation id
        id: u64,
        /// the event
        event: Event,
    },
    /// A kind this build does not know, from a peer newer than it.
    #[serde(other)]
    Unknown,
}

impl ValueObject {
    /// Whether the raw value reads true: a boolean's own bit, or the integer 1.
    pub fn reads_true(&self) -> bool {
        match &self.raw {
            serde_json::Value::Bool(on) => *on,
            other => other.as_i64() == Some(1),
        }
    }

    /// Build from a profile-described reading and the message it came from.
    pub fn from_reading(r: &svolder_profiles::Reading, m: &svolder_proto::Message) -> ValueObject {
        ValueObject {
            id: r.id.to_string(),
            key: r.key.clone(),
            label: r.label.clone(),
            value_type: m.value.value_type().to_string(),
            raw: raw_json(m),
            display: r.display.clone(),
            value: r.value,
            unit: r.unit.clone(),
            heard_at_ms: None,
        }
    }
}

/// The JSON `raw` field for a message: a number, a bool, or a hex string for `Data`.
pub fn raw_json(m: &svolder_proto::Message) -> serde_json::Value {
    if let Some(u) = m.value.as_u64() {
        serde_json::Value::from(u)
    } else if let Some(i) = m.value.as_i64() {
        serde_json::Value::from(i)
    } else if let svolder_proto::Value::Bool(b) = m.value {
        serde_json::Value::from(b)
    } else {
        serde_json::Value::from(m.value.to_string())
    }
}

/// Serialize a value to a single newline-terminated JSON line.
pub fn to_line<T: serde::Serialize>(value: &T) -> Result<String, crate::error::IpcError> {
    let mut s = serde_json::to_string(value)?;
    s.push('\n');
    Ok(s)
}

#[cfg(test)]
mod tests;
