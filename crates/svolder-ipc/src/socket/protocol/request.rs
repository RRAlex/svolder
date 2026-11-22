// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The requests a front end sends, and the `cmd` of each.

use serde::{Deserialize, Serialize};

#[cfg(doc)]
use super::READINGS_SPAN_SECS;
use super::{
    AutoEqOp, DeviceConfigOp, HostEqOp, MqttOp, PresetOp, SceneOp, Server, SpeakersOp, WalkOp,
};

/// The `cmd` of every [`Request`] kind, in the enum's order. The server tells a kind it does not
/// have from a malformed one of a kind it has by this list; a test reads the enum's own source
/// and fails when a variant is missing from it.
pub(crate) const REQUEST_CMDS: &[&str] = &[
    "devices",
    "state",
    "get",
    "set",
    "raw",
    "history",
    "readings",
    "recalled",
    "web_pair",
    "web_link",
    "web_link_open",
    "eq_show",
    "eq_set",
    "eq_mode",
    "passphrase",
    "identify",
    "scan",
    "subscribe",
    "reload",
    "version",
    "stop",
    "host",
    "host_press",
    "host_volume",
    "host_mute",
    "host_eq",
    "host_frames",
    "walk",
    "auto_eq",
    "speakers",
    "device_config",
    "server_switch",
    "guest_switch",
    "mqtt_set",
    "preset_write",
    "host_cover",
    "scene",
];

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
    /// Set the computer's volume, 0 to 100 percent; answered with an acknowledgement once the
    /// sink took it, and the new level arrives with the host's view
    HostVolume {
        /// the volume in percent, never above 100
        pct: u8,
    },
    /// Mute or unmute the computer's sink; answered with an acknowledgement once the sink took
    /// it
    HostMute {
        /// whether the sink is muted
        muted: bool,
    },
    /// Change the computer's EQ; answered with an acknowledgement or an error with the reason.
    /// The point indexes it carries count from 1, as the bands do
    HostEq {
        /// what to do
        op: HostEqOp,
    },
    /// Turn the connection into a stream of the daemon's analysed audio frames
    /// (`Event::HostCapture` once and on each change of the capture, `Event::HostFrame` while it
    /// runs), holding the daemon's capture of the default sink for as long as the connection
    /// lives. A daemon from before it, or one with no host audio, answers an error instead.
    HostFrames,
    /// One step of the device walk: stop one of its jobs by id, or follow them. The ops that
    /// start work come with the walk
    Walk {
        /// what to do
        op: WalkOp,
    },
    /// One step of AutoEq: stop one of its jobs by id, or follow them. The ops that start work
    /// come with AutoEq
    AutoEq {
        /// what to do
        op: AutoEqOp,
    },
    /// One step of the virtual speakers' sets: list them, fetch or import one (jobs), forget one,
    /// stop a job, or follow the jobs
    Speakers {
        /// what to do
        op: SpeakersOp,
    },
    /// List, add, change or remove a configured device, or make one the default; answered with
    /// the configured devices as they stand after it. The daemon writes its configuration file
    /// and reloads it
    DeviceConfig {
        /// what to do
        op: DeviceConfigOp,
    },
    /// Turn the web UI or the MCP server on or off; answered with where both stand after it.
    /// The daemon writes the server's `enabled` in its configuration file and reloads it
    ServerSwitch {
        /// which server
        server: Server,
        /// on or off
        enabled: bool,
    },
    /// Let visitors without a sign-in read the web UI's device screens (`read: true`, `[web]
    /// guest = "read"`) or not (`false`, `guest = "none"`); answered as `server_switch` is, with
    /// where both servers stand after it. The daemon writes `guest` in its configuration file and
    /// reloads it
    GuestSwitch {
        /// whether guests may read
        read: bool,
    },
    /// Change one key of the `[mqtt]` table: on or off, the host, the port, the username or the
    /// password; answered with where the bridge stands after it. The daemon writes the key in
    /// its configuration file and reloads it
    MqttSet {
        /// what to change
        op: MqttOp,
    },
    /// Save, rename or delete one of the user's presets; answered with `preset_written`
    /// carrying the name as the file now spells it. The daemon judges the name, writes its
    /// configuration file and reloads it
    PresetWrite {
        /// what to do
        op: PresetOp,
    },
    /// One chunk of the followed player's current cover, by the hash the host view names it
    /// with; answered with `host_cover` carrying the chunk, or an error when the hash is not
    /// the current cover's. A chunk is 512 KiB of picture at most, so a picture takes several
    /// asks
    HostCover {
        /// the cover's hash
        hash: String,
        /// where the chunk starts in the picture; the start when left out
        #[serde(default)]
        offset: u64,
    },
    /// Save, recall, rename or delete one scene of sound settings; answered with `scene`
    /// carrying the name as the file now spells it and, for a recall, how each part fared. The
    /// daemon judges the name, writes its configuration file and reloads it.
    Scene {
        /// what to do
        op: SceneOp,
    },
}

impl Request {
    /// Whether the server answers the request with a stream of events rather than one
    /// response: a subscription, the host's frames, and the follow op of each family of jobs.
    /// The server and a client branch on this and never on the kind.
    pub fn is_stream(&self) -> bool {
        match self {
            Request::Subscribe { .. } | Request::HostFrames => true,
            Request::Walk { op } => op.is_stream(),
            Request::AutoEq { op } => op.is_stream(),
            Request::Speakers { op } => op.is_stream(),
            _ => false,
        }
    }

    /// Whether a stream opens with a `Response::Ack` under the request's id, before its first
    /// event, so a client knows the daemon took the request even when no event comes for a
    /// while. A subscription and the host's frames send none, so their wire is unchanged.
    pub(crate) fn stream_acks(&self) -> bool {
        self.is_stream() && !matches!(self, Request::Subscribe { .. } | Request::HostFrames)
    }

    /// Whether a client waits for the daemon's first line before it takes the connection for an
    /// open stream: every stream but a subscription, which is answered with nothing and may stay
    /// silent until something changes.
    pub(crate) fn stream_waits(&self) -> bool {
        self.is_stream() && !matches!(self, Request::Subscribe { .. })
    }

    /// The wire name of the request's kind, such as `host_frames`: what a message about the
    /// request calls it.
    pub fn cmd(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|value| {
                value
                    .get("cmd")
                    .and_then(|cmd| cmd.as_str().map(String::from))
            })
            .unwrap_or_default()
    }
}
