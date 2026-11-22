// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading the `[[scene]]` blocks: each key checked, every error naming the scene.

use svolder_host::eq::{ComputerPart, CrossfeedSettings, LoudnessSettings};
use svolder_profiles::EqMode;
use svolder_profiles::scene::DevicePart;
use svolder_profiles::tone::apo::{self, Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{
    CUSTOM, LOUDNESS_MAX, NameError, NameRefusal, Preset, PresetBand, checked_name, same_name,
};

use super::super::ConfigError;
use super::Scene;

/// The keys one `[[scene]]` block may carry.
pub(super) const SCENE_KEYS: [&str; 3] = ["name", "computer", "device"];

/// The keys of a scene's `computer` table.
pub(super) const COMPUTER_KEYS: [&str; 10] = [
    "eq_on",
    "name",
    "origin",
    "preamp",
    "filter",
    "loudness",
    "crossfeed_on",
    "crossfeed",
    "speakers_on",
    "speakers",
];

/// The keys of the computer's `loudness` table.
pub(super) const LOUDNESS_KEYS: [&str; 3] = ["on", "strength_pct", "reference_pct"];

/// The keys of the computer's `crossfeed` table.
pub(super) const CROSSFEED_KEYS: [&str; 6] = [
    "model",
    "preset",
    "edited_from",
    "amount",
    "crossover_hz",
    "delay_ms",
];

/// The keys of one `[[scene.device]]` block.
pub(super) const DEVICE_KEYS: [&str; 5] = ["udid", "name", "tone", "raw_bands", "controls"];

/// The keys of a device's `tone` table.
pub(super) const TONE_KEYS: [&str; 4] = ["name", "mode", "loudness", "band"];

/// The keys of one `filter` entry.
const FILTER_KEYS: [&str; 4] = ["type", "f", "g", "q"];

/// The keys of one `band` entry.
const BAND_KEYS: [&str; 3] = ["f", "g", "q"];

/// The keys of one `controls` or `raw_bands` entry.
const SETTING_KEYS: [&str; 2] = ["key", "step"];

type Table = toml::map::Map<String, toml::Value>;

/// The `[[scene]]` blocks: each one checked, and no two sharing a name.
pub(in crate::config) fn read_scenes(raw: &[toml::Value]) -> Result<Vec<Scene>, ConfigError> {
    let mut scenes: Vec<Scene> = Vec::with_capacity(raw.len());
    for value in raw {
        let scene = read_scene(value)?;
        if scenes
            .iter()
            .any(|other| same_name(&other.name, &scene.name))
        {
            return Err(super::write::scene_taken(&scene.name));
        }
        scenes.push(scene);
    }
    Ok(scenes)
}

/// One `[[scene]]` block, naming the scene in every error.
fn read_scene(value: &toml::Value) -> Result<Scene, ConfigError> {
    let table = value
        .as_table()
        .ok_or_else(|| ConfigError::Parse("scene is not a table".into()))?;
    let written = table
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    let name = match checked_name(written) {
        Ok(name) => name.to_string(),
        Err(NameError::Blank) => return Err(ConfigError::Parse("a scene needs a name".into())),
        Err(why) => {
            return Err(ConfigError::Parse(format!(
                "scene {:?}: {}",
                written.trim(),
                crate::error::scene_name_words(&NameRefusal::Name(why))
            )));
        }
    };
    let err = |reason: String| ConfigError::Parse(format!("scene {name:?}: {reason}"));
    unknown_key(&err, table, &SCENE_KEYS, "")?;
    if same_name(&name, CUSTOM) {
        return Err(err(crate::error::scene_name_words(&NameRefusal::Reserved(
            name.clone(),
        ))));
    }
    let computer = match table.get("computer") {
        None => None,
        Some(value) => Some(read_computer(&err, value)?),
    };
    let devices = match table.get("device") {
        None => Vec::new(),
        Some(value) => {
            let blocks = value
                .as_array()
                .ok_or_else(|| err("device must be a list of [[scene.device]] blocks".into()))?;
            blocks
                .iter()
                .enumerate()
                .map(|(at, block)| read_device(&err, at + 1, block))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    for (at, part) in devices.iter().enumerate() {
        if devices[..at]
            .iter()
            .any(|other| other.udid.eq_ignore_ascii_case(&part.udid))
        {
            return Err(err(format!(
                "device {}: unit {} is in the scene twice",
                at + 1,
                part.udid
            )));
        }
    }
    Ok(Scene {
        name,
        computer,
        devices,
    })
}

/// Refuse the first key of `table` that is not in `known`; `at` says where, for the words.
fn unknown_key(
    err: &dyn Fn(String) -> ConfigError,
    table: &Table,
    known: &[&str],
    at: &str,
) -> Result<(), ConfigError> {
    match table.keys().find(|key| !known.contains(&key.as_str())) {
        Some(key) => Err(err(format!("{at}unknown key {key:?}"))),
        None => Ok(()),
    }
}

/// The table a key holds, or the words for a value of another kind.
fn table_of<'v>(
    err: &dyn Fn(String) -> ConfigError,
    value: &'v toml::Value,
    what: &str,
) -> Result<&'v Table, ConfigError> {
    value
        .as_table()
        .ok_or_else(|| err(format!("{what} must be a table")))
}

/// A TOML number as `f64`, whether it is written as an integer or a float.
fn as_f64(value: &toml::Value) -> Option<f64> {
    match value {
        toml::Value::Integer(n) => Some(*n as f64),
        toml::Value::Float(n) => Some(*n),
        _ => None,
    }
}

/// The finite number `key` of `table` holds, `None` when it is absent.
fn number(
    err: &dyn Fn(String) -> ConfigError,
    table: &Table,
    key: &str,
    at: &str,
) -> Result<Option<f64>, ConfigError> {
    match table.get(key) {
        None => Ok(None),
        Some(value) => match as_f64(value) {
            Some(n) if n.is_finite() => Ok(Some(n)),
            Some(_) => Err(err(format!("{at}{key} must be finite"))),
            None => Err(err(format!("{at}{key} must be a number"))),
        },
    }
}

/// The text `key` of `table` holds, `None` when it is absent.
fn text(
    err: &dyn Fn(String) -> ConfigError,
    table: &Table,
    key: &str,
    at: &str,
) -> Result<Option<String>, ConfigError> {
    match table.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|word| Some(word.to_string()))
            .ok_or_else(|| err(format!("{at}{key} must be text"))),
    }
}

/// The on/off flag `key` of `table` holds, `default` when it is absent.
fn flag(
    err: &dyn Fn(String) -> ConfigError,
    table: &Table,
    key: &str,
    default: bool,
    at: &str,
) -> Result<bool, ConfigError> {
    match table.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| err(format!("{at}{key} must be true or false"))),
    }
}

/// A scene's `computer` table.
fn read_computer(
    err: &dyn Fn(String) -> ConfigError,
    value: &toml::Value,
) -> Result<ComputerPart, ConfigError> {
    let at = "computer: ";
    let table = table_of(err, value, "computer")?;
    unknown_key(err, table, &COMPUTER_KEYS, at)?;
    let eq_on = table
        .get("eq_on")
        .and_then(toml::Value::as_bool)
        .ok_or_else(|| err(format!("{at}eq_on must be true or false")))?;
    let name = text(err, table, "name", at)?.ok_or_else(|| err(format!("{at}name is required")))?;
    let origin = text(err, table, "origin", at)?;
    let preamp_db =
        number(err, table, "preamp", at)?.ok_or_else(|| err(format!("{at}preamp is required")))?;
    let entries = table
        .get("filter")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| err(format!("{at}filter must be an array")))?;
    let filters = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| read_filter(err, index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    if !filters.is_empty() {
        apo::validate(&Correction {
            preamp_db,
            filters: filters.clone(),
        })
        .map_err(|why| err(format!("{at}{why}")))?;
    } else if preamp_db.abs() > 30.0 {
        return Err(err(format!(
            "{at}Preamp {preamp_db} dB is outside -30 to +30 dB"
        )));
    }
    let loudness = match table.get("loudness") {
        None => None,
        Some(value) => Some(read_loudness(err, value)?),
    };
    let crossfeed = match table.get("crossfeed") {
        None => None,
        Some(value) => Some(read_crossfeed(err, value)?),
    };
    Ok(ComputerPart {
        eq_on,
        name,
        origin,
        preamp_db,
        filters,
        loudness,
        crossfeed_on: flag(err, table, "crossfeed_on", false, at)?,
        crossfeed,
        speakers_on: flag(err, table, "speakers_on", false, at)?,
        speakers: text(err, table, "speakers", at)?,
    })
}

/// The computer's `loudness` table.
fn read_loudness(
    err: &dyn Fn(String) -> ConfigError,
    value: &toml::Value,
) -> Result<LoudnessSettings, ConfigError> {
    let at = "computer loudness: ";
    let table = table_of(err, value, "computer loudness")?;
    unknown_key(err, table, &LOUDNESS_KEYS, at)?;
    let mut settings = LoudnessSettings::default();
    settings.on = flag(err, table, "on", settings.on, at)?;
    settings.strength_pct =
        number(err, table, "strength_pct", at)?.unwrap_or(settings.strength_pct);
    settings.reference_pct =
        number(err, table, "reference_pct", at)?.unwrap_or(settings.reference_pct);
    Ok(settings)
}

/// The computer's `crossfeed` table.
fn read_crossfeed(
    err: &dyn Fn(String) -> ConfigError,
    value: &toml::Value,
) -> Result<CrossfeedSettings, ConfigError> {
    let at = "computer crossfeed: ";
    let table = table_of(err, value, "computer crossfeed")?;
    unknown_key(err, table, &CROSSFEED_KEYS, at)?;
    for key in ["amount", "crossover_hz", "delay_ms"] {
        number(err, table, key, at)?;
    }
    value
        .clone()
        .try_into::<CrossfeedSettings>()
        .map_err(|why| {
            err(format!(
                "{at}{}",
                crate::error::without_value(why.message())
            ))
        })
}

/// One entry of a `filter` array, counted from 1 in the words.
fn read_filter(
    err: &dyn Fn(String) -> ConfigError,
    index: usize,
    value: &toml::Value,
) -> Result<SourceFilter, ConfigError> {
    let at = format!("computer filter {}: ", index + 1);
    let table = value
        .as_table()
        .ok_or_else(|| err(format!("{at}not a table")))?;
    unknown_key(err, table, &FILTER_KEYS, &at)?;
    let word = table
        .get("type")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| err(format!("{at}type must be PK, LSC or HSC")))?;
    let kind = match word.to_ascii_uppercase().as_str() {
        "PK" | "LSC" | "HSC" => word.parse::<FilterKind>().ok(),
        _ => None,
    }
    .ok_or_else(|| err(format!("{at}type must be PK, LSC or HSC, not {word:?}")))?;
    let get = |field: &str| {
        number(err, table, field, &at)?.ok_or_else(|| err(format!("{at}{field} is required")))
    };
    Ok(SourceFilter {
        kind,
        f: get("f")?,
        g: get("g")?,
        q: get("q")?,
    })
}

/// One `[[scene.device]]` block, counted from 1 in the words.
fn read_device(
    err: &dyn Fn(String) -> ConfigError,
    index: usize,
    value: &toml::Value,
) -> Result<DevicePart, ConfigError> {
    let at = format!("device {index}: ");
    let table = value
        .as_table()
        .ok_or_else(|| err(format!("{at}not a table")))?;
    unknown_key(err, table, &DEVICE_KEYS, &at)?;
    let udid =
        text(err, table, "udid", &at)?.ok_or_else(|| err(format!("{at}udid is required")))?;
    udid.parse::<svolder_proto::UnitId>()
        .map_err(|_| err(format!("{at}bad udid {udid:?}")))?;
    let name = text(err, table, "name", &at)?.unwrap_or_default();
    let at = format!(
        "device {index} ({}): ",
        if name.is_empty() { &udid } else { &name }
    );
    let tone = match table.get("tone") {
        None => None,
        Some(value) => Some(read_tone(err, &at, value)?),
    };
    let raw_bands = match table.get("raw_bands") {
        None => None,
        Some(value) => Some(read_settings(err, &at, "raw_bands", value)?),
    };
    let controls = match table.get("controls") {
        None => Vec::new(),
        Some(value) => read_settings(err, &at, "controls", value)?,
    };
    if tone.is_some() && raw_bands.is_some() {
        return Err(err(format!(
            "{at}a part holds a tone or built-in bands, not both"
        )));
    }
    let mut seen: Vec<&str> = Vec::new();
    for (key, _) in raw_bands.iter().flatten().chain(controls.iter()) {
        if seen.iter().any(|other| other.eq_ignore_ascii_case(key)) {
            return Err(err(format!("{at}key {key:?} is listed twice")));
        }
        seen.push(key);
    }
    Ok(DevicePart {
        udid,
        name,
        tone,
        raw_bands,
        controls,
    })
}

/// A device's `tone` table: a curve of bands, no source.
fn read_tone(
    err: &dyn Fn(String) -> ConfigError,
    at: &str,
    value: &toml::Value,
) -> Result<Preset, ConfigError> {
    let outer = at;
    let at = format!("{at}tone: ");
    let table = table_of(err, value, &format!("{outer}tone"))?;
    unknown_key(err, table, &TONE_KEYS, &at)?;
    let name =
        text(err, table, "name", &at)?.ok_or_else(|| err(format!("{at}name is required")))?;
    let mode_word = table
        .get("mode")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| err(format!("{at}mode must be flat, eq, or loudness")))?;
    let mode: EqMode = mode_word.parse().map_err(|_| {
        err(format!(
            "{at}mode must be flat, eq, or loudness, not {mode_word:?}"
        ))
    })?;
    let loudness = match table.get("loudness") {
        None => 0,
        Some(value) => match value.as_integer() {
            Some(n) if (0..=i64::from(LOUDNESS_MAX)).contains(&n) => n as u8,
            Some(n) => return Err(err(format!("{at}loudness must be 0-127, not {n}"))),
            None => return Err(err(format!("{at}loudness must be 0-127"))),
        },
    };
    let entries = table
        .get("band")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| err(format!("{at}band must be an array")))?;
    let band = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| read_band(err, &at, index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    let bands = u8::try_from(band.len())
        .ok()
        .filter(|&bands| bands > 0)
        .ok_or_else(|| err(format!("{at}band needs between 1 and 255 entries")))?;
    Ok(Preset {
        name,
        bands,
        mode,
        loudness,
        band,
        source: None,
    })
}

/// One entry of a tone's `band` array, counted from 1 in the words.
fn read_band(
    err: &dyn Fn(String) -> ConfigError,
    at: &str,
    index: usize,
    value: &toml::Value,
) -> Result<PresetBand, ConfigError> {
    let at = format!("{at}band {}: ", index + 1);
    let table = value
        .as_table()
        .ok_or_else(|| err(format!("{at}not a table")))?;
    unknown_key(err, table, &BAND_KEYS, &at)?;
    let get = |field: &str| {
        number(err, table, field, &at)?.ok_or_else(|| err(format!("{at}{field} is required")))
    };
    Ok(PresetBand {
        f: get("f")?,
        g: get("g")?,
        q: get("q")?,
    })
}

/// A `controls` or `raw_bands` array: each entry a setting's profile key and its raw step.
fn read_settings(
    err: &dyn Fn(String) -> ConfigError,
    at: &str,
    what: &str,
    value: &toml::Value,
) -> Result<Vec<(String, i64)>, ConfigError> {
    let entries = value
        .as_array()
        .ok_or_else(|| err(format!("{at}{what} must be an array")))?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let at = format!("{at}{what} {}: ", index + 1);
            let table = entry
                .as_table()
                .ok_or_else(|| err(format!("{at}not a table")))?;
            unknown_key(err, table, &SETTING_KEYS, &at)?;
            let key = text(err, table, "key", &at)?
                .filter(|key| !key.trim().is_empty())
                .ok_or_else(|| err(format!("{at}key is required")))?;
            let step = table
                .get("step")
                .and_then(toml::Value::as_integer)
                .ok_or_else(|| err(format!("{at}step must be a whole number")))?;
            Ok((key, step))
        })
        .collect()
}
