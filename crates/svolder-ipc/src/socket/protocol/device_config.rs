// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The configured devices as a family of requests: `Request::DeviceConfig { op }`, as `host_eq`
//! carries its op.
//!
//! While a daemon runs it is the one writer of the `[[device]]` blocks and of `default_device`:
//! a front end adds, changes and removes a device and makes one the default through these ops,
//! and lists the devices as an editor needs them. Every op that acts on a device names it, by
//! its configured name or its whole unit id, neither case-sensitive; nothing here means "the
//! shown device". A daemon that has the family lists the feature
//! [`FEATURE_DEVICE_CRUD`](super::FEATURE_DEVICE_CRUD).
//!
//! Each op is answered with [`Response::DeviceConfig`](super::Response::DeviceConfig), the
//! configured devices as the daemon holds them after the op and the profiles a device can be
//! pinned to ([`ProfileChoice`]), or with an error whose
//! [`Reason`](super::Reason) is `taken`, `missing`, `invalid`, `read_only`, `file_busy` or
//! `stopping`.
//!
//! An op carries a passphrase, so nothing of an op is ever shown back: an op that does not parse
//! is refused in words that name the key at fault and never its value (the op is read by hand
//! here, since serde's own words for a value of the wrong type quote it), and an op printed with
//! `{:?}` says only that a passphrase is there. The hand reader takes an op as an object only
//! (a bare `["list"]` is refused) and, of a key given twice, the last.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// One op on the configured devices, as a `device_config` request carries it.
#[derive(Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DeviceConfigOp {
    /// The configured devices, in the file's order.
    List,
    /// Configure a new device. A name or a unit another device has is refused (`taken`,
    /// `invalid`).
    Add {
        /// the device's unit id, sixteen hex characters
        unit: String,
        /// the name to select it by
        name: String,
        /// the profile to pin it to; absent leaves the choice to what the device advertises
        #[serde(skip_serializing_if = "Option::is_none")]
        profile: Option<String>,
        /// `auto`, `none` or four digits; absent is `auto`
        #[serde(skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    /// Change a configured device. Each key left out is left as it is.
    Update {
        /// the device: its configured name or its unit id
        device: String,
        /// the new name
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// the profile to pin it to, or `null` to take the pin off
        #[serde(skip_serializing_if = "Option::is_none")]
        profile: Option<Option<String>>,
        /// `auto`, `none` or four digits
        #[serde(skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    /// Take a device out of the configuration; a default naming it is cleared with it.
    Remove {
        /// the device: its configured name or its unit id
        device: String,
    },
    /// Make a device the one a request that selects none acts on.
    SetDefault {
        /// the device: its configured name or its unit id
        device: String,
    },
    /// Leave no device as the default: a request that selects none then acts on the one device
    /// there is, and is ambiguous with several. Acknowledged when none is the default already.
    ClearDefault,
}

/// What stands for a passphrase in a printed op.
const HIDDEN: &str = "<hidden>";

impl std::fmt::Debug for DeviceConfigOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceConfigOp::List => f.write_str("List"),
            DeviceConfigOp::Add {
                unit,
                name,
                profile,
                passphrase,
            } => f
                .debug_struct("Add")
                .field("unit", unit)
                .field("name", name)
                .field("profile", profile)
                .field("passphrase", &passphrase.as_ref().map(|_| HIDDEN))
                .finish(),
            DeviceConfigOp::Update {
                device,
                name,
                profile,
                passphrase,
            } => f
                .debug_struct("Update")
                .field("device", device)
                .field("name", name)
                .field("profile", profile)
                .field("passphrase", &passphrase.as_ref().map(|_| HIDDEN))
                .finish(),
            DeviceConfigOp::Remove { device } => {
                f.debug_struct("Remove").field("device", device).finish()
            }
            DeviceConfigOp::SetDefault { device } => f
                .debug_struct("SetDefault")
                .field("device", device)
                .finish(),
            DeviceConfigOp::ClearDefault => f.write_str("ClearDefault"),
        }
    }
}

/// The ops' names on the wire, as a refusal lists them.
const OPS: &str = "list, add, update, remove, set_default, clear_default";

/// The text under `key`: none when the key is left out or `null`, and a refusal that names the
/// key, not what it held, when it holds anything but text.
fn text(keys: &mut Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match keys.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(format!("`{key}` must be a string")),
    }
}

/// The text under `key`, which the op cannot do without.
fn needed(keys: &mut Map<String, Value>, key: &str) -> Result<String, String> {
    text(keys, key)?.ok_or_else(|| format!("`{key}` is needed, as a string"))
}

/// The op `keys` spell: its name under `op`, then its own keys; a key the op does not have is
/// passed over, as every request's is.
fn op_of(mut keys: Map<String, Value>) -> Result<DeviceConfigOp, String> {
    let op = match keys.remove("op") {
        Some(Value::String(op)) => op,
        _ => return Err(format!("`op` must be one of {OPS}")),
    };
    Ok(match op.as_str() {
        "list" => DeviceConfigOp::List,
        "add" => DeviceConfigOp::Add {
            unit: needed(&mut keys, "unit")?,
            name: needed(&mut keys, "name")?,
            profile: text(&mut keys, "profile")?,
            passphrase: text(&mut keys, "passphrase")?,
        },
        "update" => DeviceConfigOp::Update {
            device: needed(&mut keys, "device")?,
            name: text(&mut keys, "name")?,
            // A key that is there, whatever it holds: `null` takes the pin off, and only a key
            // left out leaves the profile as it is.
            profile: match keys.contains_key("profile") {
                true => Some(text(&mut keys, "profile")?),
                false => None,
            },
            passphrase: text(&mut keys, "passphrase")?,
        },
        "remove" => DeviceConfigOp::Remove {
            device: needed(&mut keys, "device")?,
        },
        "set_default" => DeviceConfigOp::SetDefault {
            device: needed(&mut keys, "device")?,
        },
        "clear_default" => DeviceConfigOp::ClearDefault,
        _ => return Err(format!("`op` must be one of {OPS}")),
    })
}

impl<'de> Deserialize<'de> for DeviceConfigOp {
    fn deserialize<D: Deserializer<'de>>(from: D) -> Result<Self, D::Error> {
        match Value::deserialize(from)? {
            Value::Object(keys) => op_of(keys).map_err(D::Error::custom),
            _ => Err(D::Error::custom(format!(
                "a device_config op is an object whose `op` is one of {OPS}"
            ))),
        }
    }
}

/// Which kind of passphrase a device is configured with; the passphrase itself never leaves
/// the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassphraseKind {
    /// The daemon works it out from what the device advertises.
    Auto,
    /// The device takes none.
    None,
    /// Four digits are configured.
    Set,
    /// A kind a later daemon gives that this build does not know.
    #[serde(other)]
    Unknown,
}

impl PassphraseKind {
    /// The kind of a `passphrase` value as the configuration holds it.
    pub fn of(configured: &str) -> PassphraseKind {
        match configured {
            "" | "auto" => PassphraseKind::Auto,
            "none" => PassphraseKind::None,
            _ => PassphraseKind::Set,
        }
    }
}

/// One profile an editor offers to pin a device to: a name an `add` or an `update` takes as its
/// `profile`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileChoice {
    /// the profile's name, as a `[[device]]` block's `profile` spells it
    pub name: String,
    /// the product the profile describes, as a person reads it; absent when the profile names
    /// none
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// true for a profile that describes no product of its own: a base other profiles extend
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub base: bool,
}

/// One configured device as an editor needs it: what its `[[device]]` block says, without the
/// passphrase, and where its link stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfiguredDevice {
    /// the unit id, as the file spells it
    pub unit: String,
    /// the configured name
    pub name: String,
    /// the profile the device is pinned to; absent when the daemon picks it from what the device
    /// advertises
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// which kind of passphrase is configured
    pub passphrase: PassphraseKind,
    /// true for the device `default_device` names
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    /// true while the daemon holds a link to it
    pub connected: bool,
    /// when the daemon last lost its link to the device, in unix milliseconds, while it holds
    /// none; absent while connected, and when the daemon has not had a link to it since it
    /// started (or within the day of link changes it keeps)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_connected_since_ms: Option<u64>,
}
