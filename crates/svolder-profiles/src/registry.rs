// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Loads profile files, resolves inheritance and picks the profile for a device.

use std::collections::BTreeMap;
use std::path::Path;

use svolder_proto::{FunctionClass, FunctionId, ValueType, product_name};

use crate::builtin::BUILTIN;
use crate::db_tables;
use crate::error::ProfileError;
use crate::meter::{LogScale, Zero};
use crate::model::{
    Action, BuiltinSetting, BuiltinSettings, Control, ControlKind, Convert, EqBand, EqSpec, Family,
    Profile,
};
use crate::roles;
use crate::schema::{BuiltinSettingsDef, ControlDef, ConvertDef, EqDef, KindDef, ProfileFile};

const MAX_EXTENDS_DEPTH: usize = 8;

/// The keys the MQTT bridge's own entities are published under, which no profile control may
/// take: a control with one would replace the entity, duplicate its unique id or have its
/// commands swallowed (the identify button's topic). `svolder-mqtt` asserts its own keys are
/// these.
pub const RESERVED_KEYS: [&str; 8] = [
    "link",
    "signal",
    "firmware",
    "dsp_firmware",
    "identify",
    "eq_mode",
    "eq_preset",
    "eq_loudness",
];

/// Every profile a front end can use, resolved and ready. The default is a registry with no
/// profiles in it, which still names every product the codec's table knows.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    profiles: BTreeMap<String, Profile>,
    /// Index of the last source that defined each name; breaks selection ties.
    rank: BTreeMap<String, usize>,
}

impl Registry {
    /// Parse and resolve `(name, toml)` pairs. A later entry with the same `name` merges
    /// its fields over the earlier one.
    pub fn from_sources(sources: Vec<(String, String)>) -> Result<Self, ProfileError> {
        let mut defs: BTreeMap<String, ProfileFile> = BTreeMap::new();
        let mut rank = BTreeMap::new();
        for (index, (name, text)) in sources.iter().enumerate() {
            let def = parse(name, text)?;
            let key = def.name.clone();
            rank.insert(key.clone(), index);
            let merged = match defs.remove(&key) {
                Some(base) => merge_defs(base, def),
                None => def,
            };
            defs.insert(key, merged);
        }
        let mut profiles = BTreeMap::new();
        for name in defs.keys() {
            let profile = resolve(name, &defs, 0)?;
            // A base may name a key only its children define, so only a concrete profile,
            // resolved in full, has to satisfy `only_while`, `zero_from`, `reads`, an action's
            // `changes` and the built-in settings.
            if profile.product_id.is_some() {
                check_only_while(&profile)?;
                check_meter_zeros(&profile)?;
                check_reads(&profile)?;
                check_actions(&profile)?;
                check_builtin_settings(&profile)?;
            }
            profiles.insert(name.clone(), profile);
        }
        Ok(Self { profiles, rank })
    }

    /// The profiles compiled into the binary.
    pub fn builtin() -> Result<Self, ProfileError> {
        Self::from_sources(
            BUILTIN
                .iter()
                .map(|(n, t)| (n.to_string(), t.to_string()))
                .collect(),
        )
    }

    /// Built-in profiles plus every `*.toml` in `dir`, which may add profiles or replace built-ins by name.
    /// A missing directory is not an error.
    pub fn with_user_dir(dir: &Path) -> Result<Self, ProfileError> {
        let mut sources: Vec<(String, String)> = BUILTIN
            .iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect();
        if dir.is_dir() {
            let entries =
                std::fs::read_dir(dir).map_err(|e| ProfileError::Io(dir.to_path_buf(), e))?;
            let mut paths: Vec<_> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                .collect();
            paths.sort();
            for path in paths {
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| ProfileError::Io(path.clone(), e))?;
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                sources.push((name, text));
            }
        }
        Self::from_sources(sources)
    }

    /// The profile with this exact name.
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    /// Every profile name, in ascending order.
    pub fn names(&self) -> Vec<&str> {
        self.profiles.keys().map(String::as_str).collect()
    }

    /// The profile for a product id whose `min_firmware` is the highest not above `firmware`.
    ///
    /// Ties go to the profile defined by the later source, so a user file beats a built-in,
    /// and then to the later name, so the answer never depends on iteration order.
    pub fn select(&self, product_id: u16, firmware: u16) -> Option<&Profile> {
        let rank = |p: &Profile| self.rank.get(&p.name).copied().unwrap_or(0);
        self.profiles
            .values()
            .filter(|p| p.product_id == Some(product_id) && p.min_firmware <= firmware)
            .max_by(|a, b| {
                (a.min_firmware, rank(a))
                    .cmp(&(b.min_firmware, rank(b)))
                    .then_with(|| a.name.cmp(&b.name))
            })
    }

    /// What a device advertising `product_id` is called: the codec's own name for the products
    /// it knows, else the `display` of the profile made for that product id — the fake amp, or
    /// a product a user's profile adds — else nothing.
    ///
    /// This is the one place a front end or the daemon turns an advertisement's product id into
    /// a name, so a device reads the same way on every line that names it. Where several
    /// profiles cover the product id, the one [`Registry::select`] would pick for the newest
    /// firmware names it.
    pub fn product_name(&self, product_id: u16) -> Option<&str> {
        product_name(product_id).or_else(|| {
            self.select(product_id, u16::MAX)
                .map(|profile| profile.display.as_str())
        })
    }

    /// A profile with no controls, named after the product when known.
    pub fn generic(product_id: u16) -> Profile {
        let display = product_name(product_id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("Product {product_id}"));
        Profile {
            name: format!("generic-{product_id}"),
            display,
            product_id: Some(product_id),
            min_firmware: 0,
            controls: Vec::new(),
            actions: Vec::new(),
            eq: None,
            family: Family::Dac,
            firmware_subsystems: Vec::new(),
            identify: true,
            builtin_settings: None,
        }
    }
}

fn parse(name: &str, text: &str) -> Result<ProfileFile, ProfileError> {
    let def: ProfileFile = toml::from_str(text).map_err(|source| ProfileError::Parse {
        name: name.to_string(),
        source,
    })?;
    if def.schema != 1 {
        return Err(ProfileError::Schema(def.name.clone(), def.schema));
    }
    Ok(def)
}

/// Lay `over` on top of `base`: `over`'s schema, its `Some` fields, its controls replacing
/// those of `base` that share a key or appended when they do not, and its `eq` when it has one.
fn merge_defs(base: ProfileFile, over: ProfileFile) -> ProfileFile {
    let mut controls = base.controls;
    for control in over.controls {
        match controls
            .iter()
            .position(|c| c.key.eq_ignore_ascii_case(&control.key))
        {
            Some(i) => controls[i] = control,
            None => controls.push(control),
        }
    }
    ProfileFile {
        schema: over.schema,
        name: over.name,
        display: over.display.or(base.display),
        product_id: over.product_id.or(base.product_id),
        min_firmware: over.min_firmware.or(base.min_firmware),
        extends: over.extends.or(base.extends),
        family: over.family.or(base.family),
        firmware_subsystems: over.firmware_subsystems.or(base.firmware_subsystems),
        identify: over.identify.or(base.identify),
        controls,
        eq: over.eq.or(base.eq),
        builtin_settings: over.builtin_settings.or(base.builtin_settings),
    }
}

fn resolve(
    name: &str,
    defs: &BTreeMap<String, ProfileFile>,
    depth: usize,
) -> Result<Profile, ProfileError> {
    if depth > MAX_EXTENDS_DEPTH {
        return Err(ProfileError::ExtendsCycle(name.to_string()));
    }
    let def = defs
        .get(name)
        .ok_or_else(|| ProfileError::UnknownParent(String::new(), name.to_string()))?;
    let base = match &def.extends {
        Some(parent) => match defs.get(parent) {
            Some(_) => resolve(parent, defs, depth + 1)?,
            None => {
                return Err(ProfileError::UnknownParent(
                    def.name.clone(),
                    parent.clone(),
                ));
            }
        },
        None => Profile {
            name: def.name.clone(),
            display: def.name.clone(),
            product_id: None,
            min_firmware: 0,
            controls: Vec::new(),
            actions: Vec::new(),
            eq: None,
            family: Family::default(),
            firmware_subsystems: Vec::new(),
            identify: true,
            builtin_settings: None,
        },
    };
    merge(base, def)
}

fn merge(mut base: Profile, def: &ProfileFile) -> Result<Profile, ProfileError> {
    base.name = def.name.clone();
    if let Some(display) = &def.display {
        base.display = display.clone();
    } else if def.extends.is_none() {
        base.display = def.name.clone();
    }
    if def.product_id.is_some() {
        base.product_id = def.product_id;
    }
    if let Some(min) = def.min_firmware {
        base.min_firmware = min;
    }
    if let Some(family) = &def.family {
        base.family = parse_family(&def.name, family)?;
    }
    if let Some(subsystems) = &def.firmware_subsystems {
        check_firmware_subsystems(&def.name, subsystems)?;
        base.firmware_subsystems = subsystems.clone();
    }
    if let Some(identify) = def.identify {
        base.identify = identify;
    }
    for control_def in &def.controls {
        let (key, id) = (control_def.key.clone(), FunctionId::new(&control_def.id)?);
        // A key names one thing: a child's entry replaces the parent's of that key, in whichever
        // list the parent had it.
        let same_key = |k: &str| k.eq_ignore_ascii_case(&key);
        if control_def.kind == KindDef::Action {
            let action = build_action(&def.name, control_def)?;
            base.controls.retain(|c| !same_key(&c.key));
            match base.actions.iter().position(|a| same_key(&a.key)) {
                Some(i) => base.actions[i] = action,
                None => base.actions.push(action),
            }
        } else {
            let control = build_control(&def.name, control_def)?;
            base.actions.retain(|a| !same_key(&a.key));
            match base.controls.iter().position(|c| same_key(&c.key)) {
                Some(i) => base.controls[i] = control,
                None => base.controls.push(control),
            }
        }
        let taken = base
            .controls
            .iter()
            .map(|c| (c.id, &c.key))
            .chain(base.actions.iter().map(|a| (a.id, &a.key)))
            .any(|(other, other_key)| other == id && !same_key(other_key));
        if taken {
            return Err(ProfileError::Control {
                profile: def.name.clone(),
                key,
                reason: "duplicate function id".to_string(),
            });
        }
    }
    if let Some(eq) = &def.eq {
        base.eq = Some(build_eq(&def.name, eq)?);
    }
    if let Some(table) = &def.builtin_settings {
        base.builtin_settings = Some(build_builtin_settings(table)?);
    }
    Ok(base)
}

/// The most firmware subsystems a profile may ask after, which no device comes near.
const MAX_FIRMWARE_SUBSYSTEMS: usize = 4;

/// A `firmware_subsystems` list holds numbers from 1 (subsystem 0 is none), each once, and at
/// most [`MAX_FIRMWARE_SUBSYSTEMS`] of them: each is a frame sent after every connect.
fn check_firmware_subsystems(profile: &str, subsystems: &[u8]) -> Result<(), ProfileError> {
    let fail = |reason: &str| ProfileError::FirmwareSubsystems {
        profile: profile.to_string(),
        value: subsystems.to_vec(),
        reason: reason.to_string(),
    };
    if subsystems.contains(&0) {
        return Err(fail("subsystems are numbered from 1"));
    }
    if subsystems.len() > MAX_FIRMWARE_SUBSYSTEMS {
        return Err(fail("at most 4 subsystems may be listed"));
    }
    let mut seen = subsystems.to_vec();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != subsystems.len() {
        return Err(fail("a subsystem may be listed once"));
    }
    Ok(())
}

/// Every `zero_from` must name a `u16` readout the fully resolved profile has.
fn check_meter_zeros(profile: &Profile) -> Result<(), ProfileError> {
    for control in &profile.controls {
        let Some(LogScale {
            zero: Zero::Floor(key),
            ..
        }) = &control.log_scale
        else {
            continue;
        };
        let fail = |reason: String| ProfileError::Control {
            profile: profile.name.clone(),
            key: control.key.clone(),
            reason,
        };
        match profile.control_by_key(key) {
            None => {
                return Err(fail(format!(
                    "zero_from names {key:?}, which is not a control"
                )));
            }
            Some(floor)
                if floor.kind != ControlKind::Readout || floor.value_type != ValueType::U16 =>
            {
                return Err(fail(format!(
                    "zero_from names {key:?}, which is not a u16 readout"
                )));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Every `only_while` must name a boolean control the fully resolved profile has.
fn check_only_while(profile: &Profile) -> Result<(), ProfileError> {
    for control in &profile.controls {
        let Some(target) = &control.only_while else {
            continue;
        };
        let fail = |reason: String| ProfileError::Control {
            profile: profile.name.clone(),
            key: control.key.clone(),
            reason,
        };
        match profile.control_by_key(target) {
            None => {
                return Err(fail(format!(
                    "only_while names {target:?}, which is not a control"
                )));
            }
            Some(other) if other.value_type != ValueType::Bool => {
                return Err(fail(format!(
                    "only_while names {target:?}, which is not a boolean"
                )));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Whether two function ids are one and the same, or one lies under the other.
fn overlaps(a: FunctionId, b: FunctionId) -> bool {
    a.as_slice().starts_with(b.as_slice()) || b.as_slice().starts_with(a.as_slice())
}

/// No `reads` entry may meet another control's id or entry, an action's commands or the tone
/// control's function family: each status then reads as one control only. A control's own id
/// may lie under its own entry.
fn check_reads(profile: &Profile) -> Result<(), ProfileError> {
    for control in &profile.controls {
        let fail = |reason: String| ProfileError::Control {
            profile: profile.name.clone(),
            key: control.key.clone(),
            reason,
        };
        for &entry in &control.reads {
            for other in profile.controls.iter().filter(|c| c.key != control.key) {
                if overlaps(entry, other.id) {
                    return Err(fail(format!(
                        "reads {entry}, which overlaps the id {} of {}",
                        other.id, other.key
                    )));
                }
                if let Some(theirs) = other.reads.iter().find(|&&r| overlaps(entry, r)) {
                    return Err(fail(format!(
                        "reads {entry}, which overlaps the reads {theirs} of {}",
                        other.key
                    )));
                }
            }
            if let Some(action) = profile.actions.iter().find(|a| overlaps(entry, a.id)) {
                return Err(fail(format!(
                    "reads {entry}, which overlaps the commands of the action {}",
                    action.key
                )));
            }
            if profile.eq.as_ref().is_some_and(|eq| eq.id == entry.first()) {
                return Err(fail(format!(
                    "reads {entry}, which is in the tone control's function family"
                )));
            }
        }
    }
    Ok(())
}

/// An action's commands (`id` plus a slot) may not meet a control's id or `reads` entry, another
/// action's commands or the tone control's function family, and its `changes` name controls that
/// are neither the volume nor ones that ask before they change.
fn check_actions(profile: &Profile) -> Result<(), ProfileError> {
    for action in &profile.actions {
        let fail = |reason: String| ProfileError::Control {
            profile: profile.name.clone(),
            key: action.key.clone(),
            reason,
        };
        for control in &profile.controls {
            if overlaps(action.id, control.id) {
                return Err(fail(format!(
                    "its commands on {} overlap the id {} of {}",
                    action.id, control.id, control.key
                )));
            }
            if let Some(entry) = control.reads.iter().find(|&&r| overlaps(action.id, r)) {
                return Err(fail(format!(
                    "its commands on {} overlap the reads {entry} of {}",
                    action.id, control.key
                )));
            }
        }
        if let Some(other) = profile
            .actions
            .iter()
            .find(|a| a.key != action.key && overlaps(action.id, a.id))
        {
            return Err(fail(format!(
                "its commands on {} overlap those of the action {} on {}",
                action.id, other.key, other.id
            )));
        }
        if profile
            .eq
            .as_ref()
            .is_some_and(|eq| eq.id == action.id.first())
        {
            return Err(fail(format!(
                "its commands on {} are in the tone control's function family",
                action.id
            )));
        }
        for key in &action.changes {
            let Some(control) = profile.control_by_key(key) else {
                return Err(fail(format!(
                    "changes names {key:?}, which is not a control"
                )));
            };
            if roles::is_volume(control) {
                return Err(fail(format!(
                    "changes names {key:?}, the volume, which an action never sets"
                )));
            }
            if control.asks_on_change {
                return Err(fail(format!(
                    "changes names {key:?}, which asks before it changes"
                )));
            }
        }
    }
    Ok(())
}

/// The built-in settings must be appliable: level controls only, each written to an id the
/// profile reads as that control, a raw step inside its range for every key of every setting,
/// named once; and a profile has them or an `[eq]`, never both.
fn check_builtin_settings(profile: &Profile) -> Result<(), ProfileError> {
    let Some(table) = &profile.builtin_settings else {
        return Ok(());
    };
    let fail = |reason: String| ProfileError::BuiltinSettings {
        profile: profile.name.clone(),
        reason,
    };
    if profile.eq.is_some() {
        return Err(fail(
            "a profile has [eq] or [builtin_settings], not both: a device has one preset list"
                .to_string(),
        ));
    }
    if table.keys.is_empty() {
        return Err(fail("keys is empty".to_string()));
    }
    let mut controls = Vec::new();
    for key in &table.keys {
        match profile.control_by_key(key) {
            Some(control) if roles::is_volume(control) => {
                return Err(fail(format!(
                    "{key:?} is the volume, which is never part of a built-in setting"
                )));
            }
            Some(control) if control.kind == ControlKind::Level => controls.push(control),
            _ => {
                return Err(fail(format!(
                    "{key:?} is not a level control of this profile"
                )));
            }
        }
    }
    let mut seen: Vec<String> = Vec::new();
    for key in &table.keys {
        let lower = key.to_ascii_lowercase();
        if seen.contains(&lower) {
            return Err(fail(format!("key {key:?} is listed more than once")));
        }
        seen.push(lower);
    }
    if table.ids.len() != table.keys.len() {
        return Err(fail(format!(
            "ids has {} entries for {} keys",
            table.ids.len(),
            table.keys.len()
        )));
    }
    for (id, control) in table.ids.iter().zip(&controls) {
        if profile.read_as(*id) != control.id {
            return Err(fail(format!(
                "id {id} reads as {}, not as {} (id {})",
                profile.read_as(*id),
                control.key,
                control.id
            )));
        }
    }
    if table.settings.is_empty() {
        return Err(fail("there are no settings".to_string()));
    }
    let mut names: Vec<String> = Vec::new();
    for setting in &table.settings {
        let name = setting.name.trim().to_lowercase();
        if name.is_empty() {
            return Err(fail("a setting's name is blank".to_string()));
        }
        if names.contains(&name) {
            return Err(fail(format!(
                "the setting name {:?} is used more than once",
                setting.name
            )));
        }
        names.push(name);
        if setting.raw.len() != controls.len() {
            return Err(fail(format!(
                "setting {:?} has {} values for {} keys",
                setting.name,
                setting.raw.len(),
                controls.len()
            )));
        }
        for (raw, control) in setting.raw.iter().zip(&controls) {
            let (lo, hi) = control.range.unwrap_or((i64::MIN, i64::MAX));
            if *raw < lo || *raw > hi {
                return Err(fail(format!(
                    "setting {:?}: {raw} is outside the range {lo} to {hi} of {}",
                    setting.name, control.key
                )));
            }
        }
    }
    Ok(())
}

/// Parses a profile's `family` key, naming the profile in the error a bad value produces.
fn parse_family(name: &str, family: &str) -> Result<Family, ProfileError> {
    match family {
        "dac" => Ok(Family::Dac),
        "amp" => Ok(Family::Amp),
        "preamp" => Ok(Family::Preamp),
        _ => Err(ProfileError::Family(name.to_string(), family.to_string())),
    }
}

fn build_control(profile: &str, def: &ControlDef) -> Result<Control, ProfileError> {
    let fail = |reason: &str| ProfileError::Control {
        profile: profile.to_string(),
        key: def.key.clone(),
        reason: reason.to_string(),
    };
    if RESERVED_KEYS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(&def.key))
    {
        return Err(fail(
            "this key is reserved for the MQTT bridge's own entities; choose another key",
        ));
    }
    for (name, set) in [
        ("slots", def.slots.is_some()),
        ("overwrites", def.overwrites),
        ("changes", def.changes.is_some()),
    ] {
        if set {
            return Err(fail(&format!("{name} applies only to an action")));
        }
    }
    let id = FunctionId::new(&def.id)?;
    // The passphrase and the firmware update are never a control: a control is listed, with its
    // value, to every viewer the front ends serve, and a write to either is refused anyway.
    if id.class() != FunctionClass::Normal {
        return Err(fail(&format!(
            "function {id} cannot be a control: it carries the passphrase or updates the firmware"
        )));
    }
    let kind = match def.kind {
        KindDef::Select => ControlKind::Select,
        KindDef::Toggle => ControlKind::Toggle,
        KindDef::Level => ControlKind::Level,
        KindDef::Readout => ControlKind::Readout,
        KindDef::Meter => ControlKind::Meter,
        KindDef::Fault => ControlKind::Fault,
        KindDef::Action => return Err(fail("an action is not a control")),
    };
    let value_type = match &def.value_type {
        Some(name) => name
            .parse::<ValueType>()
            .map_err(|_| fail("unknown value_type"))?,
        None => kind.default_value_type(),
    };
    let mut values = Vec::new();
    if let Some(map) = &def.values {
        for (k, v) in map {
            let raw: u8 = k.parse().map_err(|_| fail("values keys must be 0-255"))?;
            values.push((raw, v.clone()));
        }
        values.sort_by_key(|(raw, _)| *raw);
    }
    let range = def.range.map(|[lo, hi]| (lo, hi));
    if kind == ControlKind::Level {
        let (lo, hi) = range.ok_or_else(|| fail("level control needs range"))?;
        if lo > hi {
            return Err(fail("range must be low..=high"));
        }
        let (low, high) =
            value_domain(value_type).ok_or_else(|| fail("level needs an integer value_type"))?;
        if i128::from(lo) < low || i128::from(hi) > high {
            return Err(fail("range exceeds the value_type domain"));
        }
        if def.center.is_some_and(|c| c < lo || c > hi) {
            return Err(fail("center must be inside range"));
        }
    }
    let log_scale = match &def.scale {
        None => None,
        Some(scale) => {
            if kind != ControlKind::Meter {
                return Err(fail("scale applies only to a meter"));
            }
            if range.is_some() || def.floor.is_some() || def.tick.is_some() {
                return Err(fail("scale replaces range, floor and tick"));
            }
            let zero = match (scale.zero, &scale.zero_from) {
                (Some(raw), None) => Zero::Fixed(raw),
                (None, Some(key)) => Zero::Floor(key.clone()),
                _ => return Err(fail("scale needs one of zero and zero_from")),
            };
            if scale.full <= 0 {
                return Err(fail("scale's full must be above 0"));
            }
            if let Zero::Fixed(raw) = zero
                && !(0..scale.full).contains(&raw)
            {
                return Err(fail("scale's zero must be at least 0 and below full"));
            }
            let (_, high) = value_domain(value_type)
                .ok_or_else(|| fail("a meter needs an integer value_type"))?;
            if i128::from(scale.full) > high {
                return Err(fail("scale exceeds the value_type domain"));
            }
            if !(0.0..1.0).contains(&scale.class_a) {
                return Err(fail("scale's class_a must be at least 0 and below 1"));
            }
            Some(LogScale {
                zero,
                full: scale.full,
                class_a: scale.class_a,
            })
        }
    };
    if def.floor.is_some() || def.tick.is_some() {
        if kind != ControlKind::Meter {
            return Err(fail("floor and tick apply only to a meter"));
        }
        // Both are raw values on the meter's own scale, and both are drawn as a position on
        // the bar `range` sets out, so neither means anything without it.
        let (lo, hi) = range.ok_or_else(|| fail("floor and tick need range"))?;
        if def.tick.is_some_and(|t| t < lo || t > hi) {
            return Err(fail("tick must be inside range"));
        }
        if def.floor.is_some_and(|f| f >= hi) {
            return Err(fail("floor must be below the top of range"));
        }
        if def.floor.is_some_and(|f| f < lo) {
            return Err(fail("floor must be at or above the bottom of range"));
        }
        // A tick under the floor would sit where the bar is empty, so the glyph would mark a
        // reading the bar cannot show and every reading above it would be reported hot.
        if let (Some(floor), Some(tick)) = (def.floor, def.tick)
            && tick < floor
        {
            return Err(fail("tick must be at or above floor"));
        }
    }
    if def.only_while.is_some() || def.auto_off {
        if kind != ControlKind::Readout || value_type != ValueType::Bool {
            return Err(fail("only_while and auto_off apply only to a bool readout"));
        }
        if def
            .only_while
            .as_deref()
            .is_some_and(|key| key.eq_ignore_ascii_case(&def.key))
        {
            return Err(fail("only_while cannot name the readout itself"));
        }
    }
    if def.output_gain && kind != ControlKind::Select {
        return Err(fail("output_gain applies only to a select"));
    }
    if def.asks_on_change && !matches!(kind, ControlKind::Select | ControlKind::Toggle) {
        return Err(fail("asks_on_change applies only to a select or a toggle"));
    }
    if def.mutes_output && kind != ControlKind::Toggle {
        return Err(fail("mutes_output applies only to a toggle"));
    }
    if def.powers_output && kind != ControlKind::Toggle {
        return Err(fail("powers_output applies only to a toggle"));
    }
    let convert = match &def.convert {
        None => None,
        Some(ConvertDef::Table { table }) => {
            if db_tables::table(table).is_none() {
                return Err(ProfileError::UnknownTable(
                    profile.to_string(),
                    table.clone(),
                ));
            }
            Some(Convert::Table(table.clone()))
        }
        Some(ConvertDef::Linear { linear: [min, max] }) => Some(Convert::Linear {
            min: *min,
            max: *max,
        }),
    };
    let mut reads = Vec::new();
    for entry in def.reads.iter().flatten() {
        let entry = FunctionId::new(entry)?;
        if entry.class() != FunctionClass::Normal {
            return Err(fail(&format!(
                "reads {entry}: function {entry} carries the passphrase or updates the firmware"
            )));
        }
        reads.push(entry);
    }
    Ok(Control {
        id,
        key: def.key.clone(),
        label: def.label.clone(),
        kind,
        value_type,
        values,
        range,
        center: def.center,
        unit: def.unit.clone(),
        convert,
        channel: def.channel.clone(),
        floor: def.floor,
        tick: def.tick,
        log_scale,
        advice: def.advice.clone(),
        only_while: def.only_while.clone(),
        auto_off: def.auto_off,
        output_gain: def.output_gain,
        mutes_output: def.mutes_output,
        powers_output: def.powers_output,
        asks_on_change: def.asks_on_change,
        reads,
    })
}

/// A `kind = "action"` entry: a command on `id` plus a slot, with nothing a reading would carry.
fn build_action(profile: &str, def: &ControlDef) -> Result<Action, ProfileError> {
    let fail = |reason: &str| ProfileError::Control {
        profile: profile.to_string(),
        key: def.key.clone(),
        reason: reason.to_string(),
    };
    if RESERVED_KEYS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(&def.key))
    {
        return Err(fail(
            "this key is reserved for the MQTT bridge's own entities; choose another key",
        ));
    }
    // The save role is the one front ends ask about before they send, so it must say it overwrites.
    if def.key.eq_ignore_ascii_case(roles::SAVE_KEY) && !def.overwrites {
        return Err(fail(
            "an action keyed save must set overwrites = true, so a front end asks before it sends",
        ));
    }
    let id = FunctionId::new(&def.id)?;
    if id.depth() > 2 {
        return Err(fail(
            "an action's id is one or two bytes: the slot is the third, so an id of three \
             bytes leaves no room for it",
        ));
    }
    if id.class() != FunctionClass::Normal {
        return Err(fail(&format!(
            "function {id} cannot be an action: it carries the passphrase or updates the firmware"
        )));
    }
    for (name, set) in [
        ("value_type", def.value_type.is_some()),
        ("values", def.values.is_some()),
        ("range", def.range.is_some()),
        ("center", def.center.is_some()),
        ("unit", def.unit.is_some()),
        ("convert", def.convert.is_some()),
        ("channel", def.channel.is_some()),
        ("floor", def.floor.is_some()),
        ("tick", def.tick.is_some()),
        ("scale", def.scale.is_some()),
        ("advice", def.advice.is_some()),
        ("only_while", def.only_while.is_some()),
        ("auto_off", def.auto_off),
        ("output_gain", def.output_gain),
        ("mutes_output", def.mutes_output),
        ("powers_output", def.powers_output),
        ("asks_on_change", def.asks_on_change),
        ("reads", def.reads.is_some()),
    ] {
        if set {
            return Err(fail(&format!("{name} does not apply to an action")));
        }
    }
    let slots = def
        .slots
        .ok_or_else(|| fail("an action needs slots, from 1 to 255"))?;
    let slots = u8::try_from(slots)
        .ok()
        .filter(|slots| *slots >= 1)
        .ok_or_else(|| fail("slots must be from 1 to 255"))?;
    Ok(Action {
        id,
        key: def.key.clone(),
        label: def.label.clone(),
        slots,
        overwrites: def.overwrites,
        changes: def.changes.clone().unwrap_or_default(),
    })
}

/// The `[builtin_settings]` table as the model holds it; `check_builtin_settings` judges it
/// against the resolved profile.
fn build_builtin_settings(def: &BuiltinSettingsDef) -> Result<BuiltinSettings, ProfileError> {
    Ok(BuiltinSettings {
        keys: def.keys.clone(),
        ids: def
            .ids
            .iter()
            .map(|id| FunctionId::new(id))
            .collect::<Result<_, _>>()?,
        settings: def
            .settings
            .iter()
            .map(|s| BuiltinSetting {
                name: s.name.clone(),
                raw: s.raw.clone(),
            })
            .collect(),
    })
}

/// Lowest and highest value a wire type can carry, or `None` for the non-integer types.
fn value_domain(value_type: ValueType) -> Option<(i128, i128)> {
    Some(match value_type {
        ValueType::U8 => (0, i128::from(u8::MAX)),
        ValueType::U16 => (0, i128::from(u16::MAX)),
        ValueType::U32 => (0, i128::from(u32::MAX)),
        ValueType::U64 => (0, i128::from(u64::MAX)),
        ValueType::I8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
        ValueType::I16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        ValueType::I32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        ValueType::I64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        ValueType::Bool | ValueType::Data => return None,
    })
}

fn build_eq(profile: &str, def: &EqDef) -> Result<EqSpec, ProfileError> {
    if def.fscale.len() != def.bands.len() || def.bands.is_empty() {
        return Err(ProfileError::EqShape(profile.to_string()));
    }
    let q_low = def.q.range[0];
    if q_low.is_nan() || q_low <= 0.0 {
        return Err(ProfileError::EqQRange(profile.to_string()));
    }
    for (what, range) in [("gain range", def.gain.range), ("q range", def.q.range)] {
        if !(range[0].is_finite() && range[1].is_finite() && range[0] < range[1]) {
            return Err(ProfileError::EqRange {
                profile: profile.to_string(),
                what: what.to_string(),
            });
        }
    }
    let bands = def
        .bands
        .iter()
        .zip(&def.fscale)
        .map(|(center, [low, high])| EqBand {
            center: *center,
            fscale_low: *low,
            fscale_high: *high,
        })
        .collect();
    let spec = EqSpec {
        id: def.id,
        mode_sub: def.mode_sub,
        loudness_sub: def.loudness_sub,
        range: def.range,
        bands,
        gain_range: (def.gain.range[0], def.gain.range[1]),
        gain_zero: def.gain.zero,
        q_range: (def.q.range[0], def.q.range[1]),
        q_center: def.q.center,
    };
    for band in 0..spec.bands.len() {
        let low = spec.frequency(band, 0).unwrap_or(f64::NAN);
        let high = spec.frequency(band, EqSpec::STEP_MAX).unwrap_or(f64::NAN);
        if !(low.is_finite() && high.is_finite() && 0.0 < low && low < high) {
            return Err(ProfileError::EqRange {
                profile: profile.to_string(),
                what: format!("band {} frequency window", band + 1),
            });
        }
    }
    Ok(spec)
}

#[cfg(test)]
mod tests;
