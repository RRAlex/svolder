// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Resolved, validated profiles. Everything the front ends need to name and convert values.

use crate::ProfileError;
use svolder_proto::{FunctionId, Message, Value, ValueType};

/// One device model at one minimum firmware build, with every control it exposes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Profile {
    /// The profile's identity, unique in a registry and used to `extends` it.
    pub name: String,
    /// The product name shown to a user.
    pub display: String,
    /// `None` for abstract bases such as `dac-common`.
    pub product_id: Option<u16>,
    /// The lowest advertised firmware build this profile describes.
    pub min_firmware: u16,
    /// The controls, parents first and in file order within each profile.
    pub controls: Vec<Control>,
    /// The commands that are not controls: a Command with empty data on an id plus a slot, never
    /// read and never set to a value. Parents first and in file order within each profile.
    #[cfg_attr(feature = "serde", serde(default))]
    pub actions: Vec<Action>,
    /// The tone-control block, when the device has one.
    pub eq: Option<EqSpec>,
    /// What kind of device this profile describes.
    pub family: Family,
    /// The firmware subsystems to ask for their version after each connect (`210.2.<sub>`),
    /// in the order asked. Empty for a device the app does not query.
    #[cfg_attr(feature = "serde", serde(default))]
    pub firmware_subsystems: Vec<u8>,
    /// Whether svolder offers the device's identify cue (function 200). False where the official
    /// app offers none, such as on a Mesh Card; `svolder raw 200` can still send it.
    #[cfg_attr(feature = "serde", serde(default = "yes"))]
    pub identify: bool,
    /// The named raw settings the device ships with, when it has any.
    #[cfg_attr(feature = "serde", serde(default))]
    pub builtin_settings: Option<BuiltinSettings>,
}

/// The default of [`Profile::identify`].
#[cfg(feature = "serde")]
fn yes() -> bool {
    true
}

/// What kind of device a profile describes; the TUI colours by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Family {
    /// A digital-to-analog converter.
    #[default]
    Dac,
    /// A power or headphone amplifier.
    Amp,
    /// A preamplifier or line-level controller.
    Preamp,
}

/// What a control is and how a front end presents it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ControlKind {
    /// One of a named set of values.
    Select,
    /// On or off.
    Toggle,
    /// A number over a range, often with a conversion to human units.
    Level,
    /// Reported by the device, never set.
    Readout,
    /// A streamed reading, such as a power meter.
    Meter,
    /// A fault the device reports, with advice for clearing it.
    Fault,
}

impl ControlKind {
    /// Whether a command may set this kind of control.
    pub fn writable(self) -> bool {
        matches!(
            self,
            ControlKind::Select | ControlKind::Toggle | ControlKind::Level
        )
    }

    /// The wire type this kind carries when a profile does not name one.
    pub fn default_value_type(self) -> ValueType {
        match self {
            ControlKind::Toggle | ControlKind::Fault => ValueType::Bool,
            ControlKind::Meter => ValueType::U16,
            ControlKind::Select | ControlKind::Level | ControlKind::Readout => ValueType::U8,
        }
    }
}

/// How a control's raw wire values map to human units.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Convert {
    /// Index into a named table from `db_tables`.
    Table(String),
    /// Linear map from `range` onto `[min, max]`.
    Linear {
        /// The value at the low end of the range.
        min: f64,
        /// The value at the high end of the range.
        max: f64,
    },
}

/// One function of a device, named and typed.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Control {
    /// The function address this control reads and writes.
    pub id: FunctionId,
    /// The stable name a user types; the same key means the same thing on every device.
    pub key: String,
    /// The name shown to a user.
    pub label: String,
    /// What the control is and how a front end presents it.
    pub kind: ControlKind,
    /// The wire type its values carry.
    pub value_type: ValueType,
    /// Named values for `Select` and `Readout`, sorted by raw value.
    pub values: Vec<(u8, String)>,
    /// Lowest and highest raw value, required for `Level`.
    pub range: Option<(i64, i64)>,
    /// The raw value that means centred, for a balance-like control.
    pub center: Option<i64>,
    /// The unit of the converted value, such as `dB`.
    pub unit: Option<String>,
    /// The mapping between raw values and human units.
    pub convert: Option<Convert>,
    /// Which channel a `Meter` reports.
    pub channel: Option<String>,
    /// The raw value a linear `Meter` (one with a `range`) reads at idle; the bar fills from
    /// here rather than from `range`'s minimum. A power amp's meter uses `scale`.
    pub floor: Option<i64>,
    /// The raw value where a linear `Meter` crosses into class-A operation. A power amp's
    /// meter marks its class-A block through `scale`.
    pub tick: Option<i64>,
    /// A `Meter`'s logarithmic scale, from the profile's `scale` table; see
    /// [`crate::meter`]. A meter has this or a `range`, never both.
    pub log_scale: Option<crate::meter::LogScale>,
    /// What a user should do about a `Fault`.
    pub advice: Option<String>,
    /// Boolean readout only: the key of a boolean control this readout depends on. It reads
    /// true only while that control reads true too.
    pub only_while: Option<String>,
    /// Boolean readout only: the device expects to be told false whenever it reports true, so
    /// the session writes false at once and a user cannot set the control.
    pub auto_off: bool,
    /// Select only: its values, ordered low to high by raw value, set how loud the output is;
    /// a change to a higher one raises the output.
    pub output_gain: bool,
    /// Toggle only: true silences the output, so setting it false lets sound through.
    pub mutes_output: bool,
    /// Toggle only: true powers the device on, which lets sound through at the output, so
    /// setting it true asks first.
    pub powers_output: bool,
    /// Select or toggle only: any change to it asks first, whatever its direction.
    pub asks_on_change: bool,
    /// Further function ids a status is read as this control from: a status whose id equals an
    /// entry or lies under it. Each entry is one to three bytes.
    #[cfg_attr(feature = "serde", serde(default))]
    pub reads: Vec<FunctionId>,
}

/// A command with empty data on a function id plus a slot number: recall or save a device slot.
/// It has no reading and no value; a front end lists it apart from the controls.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Action {
    /// The function address before the slot byte: one or two bytes.
    pub id: FunctionId,
    /// The stable name a user types.
    pub key: String,
    /// The name shown to a user.
    pub label: String,
    /// How many slots the action has, numbered from 1.
    pub slots: u8,
    /// Whether the action overwrites what the device holds, so a front end asks before it.
    pub overwrites: bool,
    /// The keys of the controls whose readings the device sets after the action.
    pub changes: Vec<String>,
}

/// One named setting of a device's built-in table: a raw step for each of the table's controls.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuiltinSetting {
    /// The name shown to a user.
    pub name: String,
    /// The raw step of each control, in the order of [`BuiltinSettings::keys`].
    pub raw: Vec<i64>,
}

/// A device's built-in table: the level controls it sets, the id each is written to and the
/// named settings, as raw steps.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuiltinSettings {
    /// The keys of the controls a setting sets.
    pub keys: Vec<String>,
    /// The id each control is written to, in the order of `keys`.
    pub ids: Vec<FunctionId>,
    /// The settings, in the order the device lists them.
    pub settings: Vec<BuiltinSetting>,
}

impl Action {
    /// The slots as a person types them: `1-3`, or `1` for an action with one slot.
    pub fn slots_word(&self) -> String {
        match self.slots {
            1 => "1".to_string(),
            last => format!("1-{last}"),
        }
    }

    /// The command for `slot`: empty data on the action's id plus the slot byte. A slot the
    /// action does not have is refused.
    pub fn command(&self, slot: u8) -> Result<Message, ProfileError> {
        if !(1..=self.slots).contains(&slot) {
            return Err(ProfileError::BadSlot {
                key: self.key.clone(),
                slots: self.slots,
            });
        }
        let mut bytes = self.id.as_slice().to_vec();
        bytes.push(slot);
        Ok(Message::command(
            FunctionId::new(&bytes)?,
            Value::Data(Vec::new()),
        ))
    }
}

impl Control {
    /// Whether a value of this control reads true. A boolean carries its own bit; an integer on
    /// a boolean control reads true only when it is 1, so 0 and 2 read false.
    pub fn reads_true(&self, raw: &svolder_proto::Value) -> bool {
        match raw {
            svolder_proto::Value::Bool(on) => *on,
            other => other.as_i64() == Some(1),
        }
    }

    /// The words a boolean control's states are written in, by what its key says it is.
    pub fn flag_words(&self) -> FlagWords {
        match self.key.as_str() {
            key if key.ends_with("_connected") => FlagWords::Connected,
            key if key.ends_with("_locked") => FlagWords::Locked,
            _ => FlagWords::Plain,
        }
    }

    /// The name this control gives a raw value, when it names that one.
    pub fn value_name(&self, raw: u8) -> Option<&str> {
        self.values
            .iter()
            .find(|(v, _)| *v == raw)
            .map(|(_, name)| name.as_str())
    }
}

/// The words a boolean readout's two states are written in: what its key says it is, which every
/// front end that writes the state out takes from here, so they say the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagWords {
    /// A key ending `_connected`: `connected` and `not connected`.
    Connected,
    /// A key ending `_locked`: `locked` and `unlocked`.
    Locked,
    /// Any other boolean: `yes` and `no`.
    Plain,
}

impl FlagWords {
    /// The word for a state that reads true.
    pub fn on(self) -> &'static str {
        match self {
            FlagWords::Connected => "connected",
            FlagWords::Locked => "locked",
            FlagWords::Plain => "yes",
        }
    }

    /// The word for a state that reads false.
    pub fn off(self) -> &'static str {
        match self {
            FlagWords::Connected => "not connected",
            FlagWords::Locked => "unlocked",
            FlagWords::Plain => "no",
        }
    }
}

/// One band of the tone control: where it sits and how fast its frequency moves.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EqBand {
    /// The frequency in Hz at raw value 64.
    pub center: f64,
    /// Frequency scale below the centre.
    pub fscale_low: f64,
    /// Frequency scale above the centre.
    pub fscale_high: f64,
}

/// The tone-control block: its function family, its bands and its value ranges.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EqSpec {
    /// First byte of every EQ function address.
    pub id: u8,
    /// Second byte of the mode function.
    pub mode_sub: u8,
    /// Second byte of the loudness function.
    pub loudness_sub: u8,
    /// Frequency range factor shared by every band.
    pub range: f64,
    /// The bands, in band order.
    pub bands: Vec<EqBand>,
    /// Lowest and highest band gain in dB.
    pub gain_range: (f64, f64),
    /// The raw gain value that means 0 dB.
    pub gain_zero: u8,
    /// Lowest and highest band Q.
    pub q_range: (f64, f64),
    /// The Q at raw value 64.
    pub q_center: f64,
}

impl Profile {
    /// The control with this key, matched without regard to case.
    pub fn control_by_key(&self, key: &str) -> Option<&Control> {
        self.controls
            .iter()
            .find(|c| c.key.eq_ignore_ascii_case(key))
    }

    /// The error for a key this profile lacks: it lists the profile's keys and, when one is
    /// near, says which key was probably meant. A key an app release renamed (`standby` is now
    /// `power`, `preout_mute` is now `preout`) names its successor; otherwise the nearest key
    /// within an edit distance of two, and under half the typed key's length, is named.
    pub fn unknown_key(&self, key: &str) -> ProfileError {
        let keys: Vec<String> = self
            .controls
            .iter()
            .map(|c| c.key.clone())
            .chain(self.actions.iter().map(|a| a.key.clone()))
            .collect();
        let renamed = RENAMED_KEYS
            .iter()
            .find(|(old, _)| old.eq_ignore_ascii_case(key))
            .and_then(|(_, new)| self.control_by_key(new))
            .map(|c| c.key.clone());
        let hint = renamed.or_else(|| {
            let lower = key.to_ascii_lowercase();
            keys.iter()
                .map(|k| (edit_distance(&lower, &k.to_ascii_lowercase()), k))
                .filter(|(d, _)| *d <= 2 && d * 2 < lower.chars().count())
                .min_by_key(|(d, _)| *d)
                .map(|(_, k)| k.clone())
        });
        ProfileError::UnknownKey {
            key: key.to_string(),
            keys,
            hint,
        }
    }

    /// The control a readout's `only_while` names, when it has one.
    pub fn gate_of(&self, control: &Control) -> Option<&Control> {
        control
            .only_while
            .as_deref()
            .and_then(|key| self.control_by_key(key))
    }

    /// The control at this function address.
    pub fn control_by_id(&self, id: FunctionId) -> Option<&Control> {
        self.controls.iter().find(|c| c.id == id)
    }

    /// The action with this key, matched without regard to case.
    pub fn action_by_key(&self, key: &str) -> Option<&Action> {
        self.actions
            .iter()
            .find(|a| a.key.eq_ignore_ascii_case(key))
    }

    /// The action a command on `id` is, and its slot: the action whose id is `id` less its last
    /// byte, when that byte is one of the action's slots.
    pub fn action_of(&self, id: FunctionId) -> Option<(&Action, u8)> {
        let (&slot, head) = id.as_slice().split_last()?;
        self.actions
            .iter()
            .find(|a| a.id.as_slice() == head)
            .filter(|a| (1..=a.slots).contains(&slot))
            .map(|a| (a, slot))
    }

    /// The control id a status on `id` is read as: the control at `id` itself, else the control
    /// with a `reads` entry that `id` equals or lies under, else `id` unchanged.
    pub fn read_as(&self, id: FunctionId) -> FunctionId {
        if self.control_by_id(id).is_some() {
            return id;
        }
        self.controls
            .iter()
            .find(|c| {
                c.reads
                    .iter()
                    .any(|entry| id.as_slice().starts_with(entry.as_slice()))
            })
            .map_or(id, |c| c.id)
    }
}

/// Control keys a release of the official app renamed, as (old, new).
const RENAMED_KEYS: [(&str, &str); 2] = [("standby", "power"), ("preout_mute", "preout")];

/// The Levenshtein distance between two strings, counted in characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != *cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests;
