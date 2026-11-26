// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The words and numbers the daemon answers with that a front end reads back: error codes, the
//! fixed messages, and the feature names a `version` answer lists.

#[cfg(doc)]
use super::{DeviceConfigOp, Reason, Response, WalkOp};

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

/// The code of the [`Response::Error`] for a request the daemon refuses to act on: a value it
/// cannot read, a control that is read-only, a request that is known but malformed, or a device
/// that has no such function. The CLI leaves with it as its exit code.
pub const CODE_USAGE: u8 = 1;

/// The code of the [`Response::Error`] for a device that was not found: a selector that names no
/// configured device or several, or a link attempt that found nothing.
pub const CODE_NOT_FOUND: u8 = 2;

/// The code of the [`Response::Error`] for a link that is not there: the device is not
/// connected, the connection failed, or the device has not been found yet.
pub const CODE_LINK: u8 = 3;

/// The code of the [`Response::Error`] for a request that went unanswered in time: the device did
/// not confirm, an actor did not answer, or there was no reading to judge a change from.
pub const CODE_TIMEOUT: u8 = 4;

/// The code of the [`Response::Error`] a daemon answers a computer request with while the
/// computer's controls cannot be used: its message is `computer controls unavailable: <why>`.
pub const CODE_COMPUTER_UNAVAILABLE: u8 = 6;

/// The code of the [`Response::Error`] a daemon answers a request kind it does not know with, so
/// a front end tells a daemon too old for a request from one that refused it.
pub const CODE_UNKNOWN_REQUEST: u8 = 7;

/// The feature a daemon lists when its errors carry a [`Reason`] where one applies.
pub(crate) const FEATURE_REASONS: &str = "reasons";

/// The feature a daemon lists when it answers a request kind it does not know with
/// [`CODE_UNKNOWN_REQUEST`]; one that lists it never answers that with serde's words under code 1.
pub const FEATURE_UNKNOWN_REQUEST_CODE: &str = "unknown_request_code";

/// The feature a daemon lists when it takes the `device_config` requests ([`DeviceConfigOp`]):
/// while it runs, a front end adds, changes and removes a configured device through it and
/// writes no `[[device]]` block itself.
pub const FEATURE_DEVICE_CRUD: &str = "device_crud";

/// The feature a daemon lists when it takes the `server_switch` request: while it runs, a front
/// end turns the web UI and the MCP server on and off through it.
pub const FEATURE_SERVER_SWITCH: &str = "server_switch";

/// The feature a daemon lists when it takes the `guest_switch` request: while it runs, a front
/// end turns the web UI's guest read view on and off through it.
pub const FEATURE_GUEST_SWITCH: &str = "guest_switch";

/// The feature a daemon lists when it takes the `mqtt_set` request: while it runs, a front end
/// changes one `[mqtt]` key through it, the password included.
pub const FEATURE_MQTT_SET: &str = "mqtt_set";

/// The feature a daemon lists when it takes the `host_cover` request: while it runs, a front end
/// asks it for the bytes of the cover the host view names, in chunks.
pub const FEATURE_HOST_COVER: &str = "host_cover";

/// The feature a daemon lists when it runs the device walk ([`WalkOp`]'s `start`, `answer` and
/// `back`): while it runs, a front end adds a device through the daemon's walk rather than its
/// own.
pub const FEATURE_WALK: &str = "walk";
/// The feature a daemon lists when it takes the AutoEq ops (`auto_eq` search, preview, import and
/// upload): while it runs, it is the one process that reaches AutoEq's repository.
pub const FEATURE_AUTOEQ: &str = "autoeq";

/// The feature a daemon lists when it takes the `speakers` ops (list, fetch, import and forget the
/// virtual speakers' sets): while it runs, it is the one process that reaches the sets' sources.
pub const FEATURE_SPEAKERS: &str = "speakers";

/// The feature a daemon lists when it takes the `preset_write` requests
/// ([`PresetOp`](super::PresetOp)): while it runs, a front end saves, renames and deletes the
/// user's presets through it and writes no `[[preset]]` block itself.
pub const FEATURE_PRESET_WRITE: &str = "preset_write";

/// The feature a daemon lists when it takes the `scene` requests ([`SceneOp`](super::SceneOp))
/// and sends the `scenes` event: while it runs, it keeps the scenes of sound settings, and a front
/// end offers scenes only while it lists this.
pub const FEATURE_SCENES: &str = "scenes";

/// The feature a daemon lists when a reload that changes `adapter` moves it to the new adapter at
/// once: it closes its links on the old one and connects every device again through the new, with
/// no restart. A daemon without it reads `adapter` only when it starts.
pub const FEATURE_ADAPTER_SWITCH: &str = "adapter_switch";

/// The feature a daemon lists when it publishes the adapter hint with a device's row
/// ([`DeviceInfo::adapter_hint`](super::DeviceInfo::adapter_hint)): while a device keeps failing
/// to connect on one adapter and another adapter hears it, the row says so.
pub const FEATURE_ADAPTER_HINT: &str = "adapter_hint";

/// Everything this build's daemon supports, as its `version` answer lists it: the one place the
/// list is kept. A front end asks through `DaemonInfo::supports`.
pub const FEATURES: &[&str] = &[
    FEATURE_REASONS,
    FEATURE_UNKNOWN_REQUEST_CODE,
    FEATURE_DEVICE_CRUD,
    FEATURE_SERVER_SWITCH,
    FEATURE_GUEST_SWITCH,
    FEATURE_MQTT_SET,
    FEATURE_WALK,
    FEATURE_AUTOEQ,
    FEATURE_HOST_COVER,
    FEATURE_PRESET_WRITE,
    FEATURE_SPEAKERS,
    FEATURE_SCENES,
    FEATURE_ADAPTER_SWITCH,
    FEATURE_ADAPTER_HINT,
];

/// The message of the code-3 [`Response::Error`] a daemon answers a command with while it holds no
/// link to the device. A front end that reads exactly these words knows the device is away, and
/// adds when the daemon tries it again.
pub const NOT_CONNECTED: &str = "device not connected";

/// The words a daemon answers a web code request with while its web UI is off, naming the fix.
pub const WEB_OFF: &str = "the web UI is off: `svolder web on` turns it on, and so does the TUI's \
                           Settings, Connect, Web UI";
