// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What can go wrong loading profiles or applying them to values.

use std::path::PathBuf;

/// Everything that can go wrong loading profiles or applying them to values.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    /// The file is not valid TOML, or does not match the profile schema.
    #[error("profile {name}: {source}")]
    Parse {
        /// The profile the text was read for.
        name: String,
        /// What the TOML parser objected to.
        #[source]
        source: toml::de::Error,
    },
    /// The file declares a schema version this crate does not read.
    #[error("profile {0}: unsupported schema version {1}")]
    Schema(String, u32),
    /// The profile extends a name no source defines.
    #[error("profile {0} extends unknown profile {1}")]
    UnknownParent(String, String),
    /// The extends chain loops back on itself or runs deeper than eight profiles.
    #[error("profile {0}: extends chain is circular or deeper than 8")]
    ExtendsCycle(String),
    /// The profile names a family this crate does not recognise.
    #[error("profile {0}: unknown family {1} (expected dac, amp or preamp)")]
    Family(String, String),
    /// The profile's `firmware_subsystems` list cannot be asked.
    #[error("profile {profile}, firmware_subsystems {value:?}: {reason}")]
    FirmwareSubsystems {
        /// The profile that declared the list.
        profile: String,
        /// The list as written.
        value: Vec<u8>,
        /// What is wrong with it.
        reason: String,
    },
    /// A control's fields do not describe a control that can be used.
    #[error("profile {profile}, control {key}: {reason}")]
    Control {
        /// The profile the control was declared in.
        profile: String,
        /// The control's key.
        key: String,
        /// What is wrong with it.
        reason: String,
    },
    /// The profile's `[builtin_settings]` table does not describe settings that can be applied.
    #[error("profile {profile}, builtin_settings: {reason}")]
    BuiltinSettings {
        /// The profile the table resolved into.
        profile: String,
        /// What is wrong with it.
        reason: String,
    },
    /// Something that is not one of an action's slots.
    #[error("{key} takes a slot from 1 to {slots}")]
    BadSlot {
        /// The action's key.
        key: String,
        /// How many slots the action has.
        slots: u8,
    },
    /// An action was asked for a reading.
    #[error("{key} is an action: it is sent, never read; svolder set {key} {device_word} sends it")]
    NoReading {
        /// The action's key.
        key: String,
        /// The slots the command takes, as a person types them (`1-3`).
        device_word: String,
    },
    /// The device has no built-in setting of that name.
    #[error("no built-in setting named {name}; the settings are: {}", .names.join(", "))]
    NoSetting {
        /// The name as the user wrote it.
        name: String,
        /// Every setting the device has, in the order it lists them.
        names: Vec<String>,
    },
    /// The device has no built-in settings, and no slots of its own to list.
    #[error("{device} has no device presets{}", tone_list_hint(*.tone_control))]
    NoDevicePresets {
        /// The device's name.
        device: String,
        /// Whether the device has a tone control, whose presets `svolder eq list` lists.
        tone_control: bool,
    },
    /// A control names a conversion table this crate does not carry.
    #[error("profile {0}: unknown conversion table {1}")]
    UnknownTable(String, String),
    /// The eq block does not give exactly one frequency scale pair per band.
    #[error("profile {0}: eq needs one fscale pair per band")]
    EqShape(String),
    /// The eq block's Q range starts at or below 0, where a band's response is undefined.
    #[error("profile {0}: eq q range must start above 0")]
    EqQRange(String),
    /// The eq block's gain or Q range, or a band's frequency window, is not a finite, rising
    /// range.
    #[error("profile {profile}: eq {what} must be finite and rising")]
    EqRange {
        /// The profile.
        profile: String,
        /// Which range.
        what: String,
    },
    /// The profile has no control with that key.
    #[error("no control named {key}; this device has: {}{}", .keys.join(", "), did_you_mean(.hint))]
    UnknownKey {
        /// The key as the user wrote it.
        key: String,
        /// Every key the profile has, in profile order.
        keys: Vec<String>,
        /// The key the user most likely meant, when one is near enough to guess.
        hint: Option<String>,
    },
    /// The control reports a value but never accepts one.
    #[error("control {0} is read-only")]
    NotWritable(String),
    /// The input does not name a value this control accepts.
    #[error("cannot interpret {input:?} for control {key}")]
    BadInput {
        /// The control the input was meant for.
        key: String,
        /// The input as the user wrote it.
        input: String,
    },
    /// The profile describes no tone control.
    #[error("this device has no tone control")]
    NoEq,
    /// The tone control has no band at that index.
    #[error("eq band {0} does not exist")]
    BadBand(usize),
    /// A preset does not fit this tone control, or holds a value no step can stand for.
    #[error("preset {name:?}: {reason}")]
    Preset {
        /// The preset's name.
        name: String,
        /// What is wrong with it here.
        reason: String,
    },
    /// A word did not name a band parameter or a tone-control mode.
    #[error("{0}")]
    BadEqWord(String),
    /// A relative step was asked of a control that has no range to move within.
    #[error("control {0} has no range to step through")]
    NotSteppable(String),
    /// A relative step was asked of an action, which takes a slot and never a step.
    #[error("{0} is an action: it takes a slot, never a step")]
    ActionNotSteppable(String),
    /// A profile directory or file could not be read.
    #[error("cannot read {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    /// A function address or value the protocol crate rejected.
    #[error(transparent)]
    Proto(#[from] svolder_proto::ProtoError),
}

/// The tail of a no-device-presets message: where a device with a tone control keeps its presets.
fn tone_list_hint(tone_control: bool) -> &'static str {
    if tone_control {
        "; svolder eq list lists its tone-control presets"
    } else {
        ""
    }
}

/// The tail of an unknown-key message that names the key a user most likely meant.
fn did_you_mean(hint: &Option<String>) -> String {
    hint.as_ref()
        .map(|key| format!("; did you mean {key}?"))
        .unwrap_or_default()
}
