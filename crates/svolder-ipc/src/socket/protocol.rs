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
//!
//! - `codes`: the error codes, the fixed messages and the feature names.
//! - `request`: the requests a front end sends.
//! - `status`: the readings, recall and link-change answers, token info, and the MQTT, MCP and web
//!   statuses.
//! - `timeouts`: the deadlines and bounds both sides wait by.
//! - `autoeq`, `device_config`, `host_cover`, `host_eq`, `jobs`, `mqtt_set`, `preset_write`,
//!   `scenes`, `server_switch`, `speakers`, `walk`: the request families with their own ops and
//!   answers.

use serde::{Deserialize, Serialize};

mod activity;
mod adapter_hint;
mod autoeq;
mod codes;
mod device_config;
mod host_cover;
mod host_eq;
mod jobs;
mod mqtt_set;
mod preset_write;
mod request;
mod scenes;
mod server_switch;
mod speakers;
mod status;
mod timeouts;
mod walk;

pub use activity::{Activity, DAY_SECS, ServerActivity};
pub use adapter_hint::AdapterHint;
pub use autoeq::{
    AUTOEQ_GROUPS_MAX, AUTOEQ_QUERY_MAX, AutoEqGroup, AutoEqMeasurement, AutoEqOp, AutoEqPreview,
    AutoEqSaved, AutoEqSearch, AutoEqTarget, AutoEqVariant,
};
#[cfg(test)]
pub(crate) use codes::FEATURE_REASONS;
pub use codes::{
    CODE_COMPUTER_UNAVAILABLE, CODE_LINK, CODE_NOT_FOUND, CODE_TIMEOUT, CODE_UNKNOWN_REQUEST,
    CODE_USAGE, FEATURE_ADAPTER_HINT, FEATURE_ADAPTER_SWITCH, FEATURE_AUTOEQ, FEATURE_DEVICE_CRUD,
    FEATURE_GUEST_SWITCH, FEATURE_HOST_COVER, FEATURE_MQTT_SET, FEATURE_PRESET_WRITE,
    FEATURE_SCENES, FEATURE_SERVER_SWITCH, FEATURE_SPEAKERS, FEATURE_UNKNOWN_REQUEST_CODE,
    FEATURE_WALK, FEATURES, NO_DEVICES_CONFIGURED, NOT_CONNECTED, NOT_FOUND_YET, WEB_OFF,
    not_found_yet,
};
pub use device_config::{ConfiguredDevice, DeviceConfigOp, PassphraseKind, ProfileChoice};
pub use host_cover::{COVER_CHUNK_BYTES, CoverReader, cover_chunk};
pub use host_eq::HostEqOp;
pub use jobs::{JOB_RESULT_MAX_BYTES, JobEvent, JobId, JobState};
pub use mqtt_set::MqttOp;
pub use preset_write::PresetOp;
pub(crate) use request::REQUEST_CMDS;
pub use request::Request;
pub use scenes::{PartOutcome, PartRow, PartStatus, SceneOp, SceneRow, ScenesView};
pub use server_switch::Server;
pub use speakers::{
    SPEAKERS_OFF_PREFIX, SpeakersAngles, SpeakersCatalogue, SpeakersOp, SpeakersProgress,
    SpeakersRoom, SpeakersSaved, SpeakersSet,
};
pub use status::{
    LinkChange, McpStatus, MqttProblem, MqttStatus, READINGS_SPAN_SECS, RecallOut, SeriesOut,
    TokenInfo, TokenKind, WebStatus,
};
#[cfg(test)]
pub(crate) use timeouts::COMMAND_WORST_CASE;
pub use timeouts::{
    ACTOR_STOP_MARGIN, BRIDGE_STOP_BOUND, COMMAND_DEADLINE, DEFAULT_REQUEST_TIMEOUT,
    JOB_STOP_BOUND, MAX_SCAN_WINDOW, RELOAD_TIMEOUT, RUNTIME_STOP_BOUND, WRITE_SETTLE_BOUND,
};
pub use walk::{Connect, ConnectFailure, WALK_IDLE_BOUND, WalkOp};

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
    /// the ids of the device's key readings, the few a front end graphs first, in order:
    /// [`SERIES_SIGNAL`] always first, then the keys of the controls the device's profile marks
    /// (`key_reading`); each is an id the daemon keeps a series for. Empty from a daemon too
    /// old to say
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_readings: Vec<String>,
    /// while the connects to the device keep failing on one adapter and another adapter hears
    /// it: what each hears and the adapter to switch to; absent otherwise, and from a daemon
    /// that does not list [`FEATURE_ADAPTER_HINT`]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_hint: Option<AdapterHint>,
}

/// The id of a device's signal-strength series, in dBm: the first of every device's key
/// readings. No control can be keyed so (the profile format reserves the key).
pub const SERIES_SIGNAL: &str = "signal";

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

impl ScanEntry {
    /// The row an advertisement makes: what its manufacturer data says, the signal strength it
    /// was heard at, and the product named as `registry` names it, so a product the codec does
    /// not know is named by its profile. Every scan row, the daemon's and a front end's, is
    /// built here.
    pub fn from_advert(
        manufacturer: &svolder_proto::ManufacturerData,
        rssi: Option<i16>,
        registry: &svolder_profiles::Registry,
    ) -> ScanEntry {
        ScanEntry {
            unit: manufacturer.unit_id.to_string(),
            product_id: manufacturer.product_id,
            product: registry
                .product_name(manufacturer.product_id)
                .map(str::to_string),
            firmware: manufacturer.firmware_build,
            passphrase: manufacturer.passphrase().is_well_formed(),
            rssi,
        }
    }
}

/// Why a [`Response::Error`] happened, in a word a front end can match on instead of the
/// message's text. A front end built before a reason existed ignores the field, and one that
/// meets a reason it does not know reads [`Reason::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// The daemon holds the device but has never found it; it keeps looking.
    NotFoundYet,
    /// The daemon holds no link to the device now.
    NotConnected,
    /// The request names a job the daemon does not hold: it never ran one by that id, or has
    /// forgotten it.
    NoSuchJob,
    /// The work asked for is already going on, and the daemon runs one of it at a time.
    Busy,
    /// The daemon failed in a way a front end can only report.
    Internal,
    /// The name asked for is already used by another block of the same kind (a preset, a device,
    /// a hop).
    Taken,
    /// The block the request names is not in the configuration.
    Missing,
    /// The configuration file cannot be written: it is read-only, or the daemon has no file of
    /// its own.
    ReadOnly,
    /// Another writer held the configuration file's lock until the wait ran out; asking again
    /// may work.
    FileBusy,
    /// The text cannot be a name: blank, too long, a control character, or one the daemon keeps
    /// for itself (a built-in preset's).
    BadName,
    /// The configuration the write would make is not valid: it does not parse, or two devices
    /// ask for one unit.
    Invalid,
    /// The daemon is stopping and starts nothing.
    Stopping,
    /// The job named is not waiting for an answer: it is working on the last one, or it has
    /// ended.
    NotWaiting,
    /// The measurement asked for is not in the AutoEq index, or its file is not there (404).
    NotListed,
    /// AutoEq's repository could not be reached, or what it sent could not be used: the fetch
    /// failed or ran out of time, the file was too large or not text, or no index was had.
    Unreachable,
    /// A save of a scene found nothing to save: no device is connected and the computer's
    /// controls are not there.
    NothingToSave,
    /// A reason a later daemon gives that this build does not know.
    #[serde(other)]
    Unknown,
}

/// Response to a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "the host view is the one big answer, built and matched by value by every front end"
)]
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
        /// the signal strengths the daemon heard from the device in the span, in dBm, as
        /// `(unix milliseconds, dBm)`, oldest first, none from before the span. The daemon hears
        /// one when a listen of its own catches the device's advertisement, so there is none
        /// while a link is up. Empty from a daemon too old to keep them
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        signal: Vec<(u64, f64)>,
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
    /// The reply to a request that starts a job: the job's id, which the family's stream names
    /// in every event of the job. A front end built before jobs never sends such a request.
    Job {
        /// the job's id
        job: JobId,
    },
    /// The reply to an `auto_eq` search.
    AutoEqSearch(Box<AutoEqSearch>),
    /// The reply to an `auto_eq` upload: the preset that was saved.
    AutoEqSaved(AutoEqSaved),
    /// The reply to a `speakers` catalogue.
    SpeakersCatalogue(Box<SpeakersCatalogue>),
    /// The reply to a `speakers` import_dir.
    SpeakersImportDir {
        /// the directory, absolute
        path: String,
    },
    /// The reply to a `device_config` request: the configured devices, in the file's order, as
    /// they stand after the op, and the profiles an op takes.
    DeviceConfig {
        /// one entry per `[[device]]` block
        devices: Vec<ConfiguredDevice>,
        /// every profile a device can be pinned to, by ascending name: what an `add` or an
        /// `update` takes as its `profile`, and nothing else. Empty from a daemon built before
        /// the list, and when the profiles cannot be read, which refuses every pin too.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        profiles: Vec<ProfileChoice>,
    },
    /// The reply to a `server_switch` request: where the two servers stand after it.
    ServerSwitch {
        /// the web UI
        web: WebStatus,
        /// the MCP server
        mcp: McpStatus,
    },
    /// The reply to a `preset_write` request: the preset's name as the file now spells it (the
    /// deleted preset's, for a delete).
    PresetWritten {
        /// the name
        name: String,
    },
    /// The reply to a `scene` request: the scene's name as the file now spells it (the deleted
    /// scene's, for a delete) and how each part fared: for a recall, each part; for a save, each
    /// configured device that is not connected, as a part `left` out; empty for rename and
    /// delete.
    Scene {
        /// the name
        name: String,
        /// one entry per part of a recalled scene
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        outcome: Vec<PartOutcome>,
    },
    /// The reply to an `mqtt_set` request: where the bridge stands after it.
    MqttSet {
        /// the MQTT client
        status: MqttStatus,
        /// whether the write narrowed a group- or world-readable configuration file to 0600
        /// because a password went into it
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        tightened: bool,
    },
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
    /// The version of the daemon answering, and where its MQTT client, its MCP server and its web
    /// UI stand.
    Version {
        /// the daemon's version, as its package declares it
        version: String,
        /// the daemon's MQTT client; absent from a daemon too old to have one
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
        /// what the daemon supports beyond the basics, by name ([`FEATURES`]); absent from a
        /// daemon too old to list any, which reads as supporting none
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        features: Vec<String>,
        /// whether the configuration file's modification time differs from the one the daemon's
        /// snapshot was read at, which is a hand edit not yet reloaded; absent when it does not,
        /// and from a daemon too old to say, which reads as unchanged
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        config_changed: bool,
        /// whether the configuration file the daemon read is gone (`config_changed` stands too);
        /// absent while it is there, and from a daemon too old to say
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        config_missing: bool,
        /// the kernel name of the Bluetooth adapter the daemon holds its devices through (`hci0`);
        /// absent from a daemon that does not say, and when it has no adapter name to give
        #[serde(default, skip_serializing_if = "Option::is_none")]
        adapter: Option<String>,
        /// whether the daemon's command line (`--adapter` or `SVOLDER_ADAPTER`) names the adapter,
        /// so the file's `adapter` does not move it; absent while it does not
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        adapter_pinned: bool,
        /// how many clients the MCP server and the web UI have had, as counts; absent from a
        /// daemon too old to count
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity: Option<Activity>,
    },
    /// The reply to `stop`, sent before the daemon ends its accept loop.
    Stopping,
    /// The reply to `host`: the player and the route as they stand.
    Host(crate::host::HostView),
    /// The reply to `host_cover`: one chunk of the current cover (see [`cover_chunk`]).
    HostCover {
        /// the cover's hash, as the host view gave it
        hash: String,
        /// `image/jpeg`, `image/png` or `image/webp`
        mime: String,
        /// the whole picture's length in bytes, at most 4 MiB
        total: u64,
        /// where this chunk starts in the picture
        offset: u64,
        /// the chunk's bytes, unpadded base64url
        data: String,
    },
    /// Error response
    Error {
        /// error code
        code: u8,
        /// error message
        message: String,
        /// why it happened, where the daemon says; absent from a daemon too old to
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<Reason>,
    },
    /// The reply to a `host_eq` preset recall that is asked about, not refused: the preset is
    /// also played by connected devices, and would play twice. Nothing was changed. Yes sends
    /// the same request again with `confirm`, which flattens `devices` first; no sends nothing.
    Ask {
        /// the question's facts in the daemon's words (`<name> is on the <device> too: the
        /// correction would play twice`), for a client that does not read the fields
        message: String,
        /// the preset's name
        name: String,
        /// the configured names of the devices that play it, each whole
        devices: Vec<String>,
    },
    /// A kind this build does not know, from a peer newer than it.
    #[serde(other)]
    Unknown,
}

/// An event pushed to a subscriber.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "the host view is the one big event, built and matched by value by every front end"
)]
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
    /// A change in a job of the device walk. Only on a `walk` follow stream.
    Walk(JobEvent),
    /// A change in a job of AutoEq. Only on an `auto_eq` follow stream.
    AutoEq(JobEvent),
    /// A change in a job of the virtual speakers' sets. Only on a `speakers` follow stream.
    Speakers(JobEvent),
    /// The scenes changed: one was saved, renamed, deleted or recalled, the file was reloaded, or
    /// the sound settings moved so that another scene is on or none. Sent to a subscriber of a
    /// daemon that lists [`FEATURE_SCENES`], and only to one that may change the sound.
    Scenes(ScenesView),
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

impl Response {
    /// An error answer with `code` (one of the `CODE_*` constants) and `message`, and no reason.
    pub fn error(code: u8, message: impl Into<String>) -> Response {
        Response::Error {
            code,
            message: message.into(),
            reason: None,
        }
    }

    /// The answer to a request kind this build does not know, `cmd` being its wire name:
    /// [`CODE_UNKNOWN_REQUEST`] and words that name it.
    pub fn unknown_request(cmd: &str) -> Response {
        Response::error(CODE_UNKNOWN_REQUEST, format!("unknown request `{cmd}`"))
    }

    /// This answer with `reason` as the reason it is an error; any other answer is unchanged.
    #[must_use]
    pub fn with_reason(self, reason: Reason) -> Response {
        match self {
            Response::Error { code, message, .. } => Response::Error {
                code,
                message,
                reason: Some(reason),
            },
            other => other,
        }
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
