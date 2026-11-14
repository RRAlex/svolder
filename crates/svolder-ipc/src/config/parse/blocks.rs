// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[[hop]]` and `[[preset]]` blocks: each one read and checked key by key.

use svolder_profiles::EqMode;
use svolder_profiles::tone::apo::{self, Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{
    Imported, LOUDNESS_MAX, NameError, Preset, PresetBand, checked_name, is_builtin, same_name,
};

use super::super::hops::{hop_taken, name_refusal};
use super::super::presets::preset_taken;
use super::super::{ConfigError, Hop, HopKind, HopWhat};

/// The `[[hop]]` blocks: each one checked, and no two sharing an id.
pub(super) fn hops(raw: &[toml::Value]) -> Result<Vec<Hop>, ConfigError> {
    let mut hops: Vec<Hop> = Vec::with_capacity(raw.len());
    for value in raw {
        let hop = parse_hop(value)?;
        if hops
            .iter()
            .any(|existing| same_name(existing.id(), hop.id()))
        {
            return Err(hop_taken(hop.id()));
        }
        hops.push(hop);
    }
    Ok(hops)
}

/// The `[[preset]]` blocks: each one checked, and no two sharing a name.
pub(super) fn presets(raw: &[toml::Value]) -> Result<Vec<Preset>, ConfigError> {
    let mut presets: Vec<Preset> = Vec::with_capacity(raw.len());
    for value in raw {
        let preset = parse_preset(value)?;
        if presets
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&preset.name))
        {
            return Err(preset_taken(&preset.name));
        }
        presets.push(preset);
    }
    Ok(presets)
}

/// The keys one `[[hop]]` block may carry.
const HOP_KEYS: [&str; 5] = ["name", "kind", "device", "note", "after"];

/// Parse and validate one `[[hop]]` block: exactly one of `name` (which needs a `kind`) and
/// `device`, and the optional `note` and `after`, each a string.
///
/// Read from a raw value rather than a derived `Deserialize` so that every error can name the
/// block, and so that an unknown key is refused with the block's name in the words.
fn parse_hop(value: &toml::Value) -> Result<Hop, ConfigError> {
    let table = value
        .as_table()
        .ok_or_else(|| ConfigError::Parse("[[hop]] is not a table".into()))?;
    let text = |key: &str| -> Result<Option<&str>, ConfigError> {
        match table.get(key) {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .map(Some)
                .ok_or_else(|| ConfigError::Parse(format!("[[hop]] {key} must be a string"))),
        }
    };
    let (name, device) = (text("name")?, text("device")?);
    let called = name.or(device).unwrap_or_default().trim();
    let err = |reason: String| ConfigError::Parse(format!("[[hop]] {called:?}: {reason}"));
    for key in table.keys() {
        if !HOP_KEYS.contains(&key.as_str()) {
            return Err(err(format!("unknown key {key:?}")));
        }
    }
    let kind = text("kind")?;
    let what = match (name, device) {
        (Some(_), Some(_)) => {
            return Err(ConfigError::Parse(
                "[[hop]] names a device or a name, never both: it has both \"device\" and \"name\""
                    .into(),
            ));
        }
        (None, None) => {
            return Err(ConfigError::Parse(
                "[[hop]] needs a \"name\" (with a \"kind\") or a \"device\"".into(),
            ));
        }
        (Some(name), None) => {
            let name = checked_name(name)
                .map_err(|why| err(name_refusal(why)))?
                .to_string();
            let kind =
                kind.ok_or_else(|| err(format!("a named hop needs a kind: {}", HopKind::words())))?;
            let kind = HopKind::named(kind)
                .ok_or_else(|| err(format!("kind must be one of {}", HopKind::words())))?;
            HopWhat::Declared { kind, name }
        }
        (None, Some(device)) => {
            if kind.is_some() {
                return Err(err(
                    "a hop that links a device has no kind of its own".into()
                ));
            }
            let device = checked_name(device)
                .map_err(|why| err(name_refusal(why)))?
                .to_string();
            HopWhat::Linked { device }
        }
    };
    // A blank line says nothing, as a blank value does anywhere in the file.
    let line = |key: &str| {
        text(key).map(|line| {
            line.map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
        })
    };
    Ok(Hop {
        what,
        note: line("note")?,
        after: line("after")?,
    })
}

/// The keys one `[[preset]]` block may carry.
const PRESET_KEYS: [&str; 8] = [
    "name", "bands", "mode", "loudness", "band", "origin", "preamp", "filter",
];

/// The keys only a preset of bands carries.
const BAND_PRESET_KEYS: [&str; 4] = ["bands", "mode", "loudness", "band"];

/// The keys one `filter` entry of an imported preset may carry.
const FILTER_KEYS: [&str; 4] = ["type", "f", "g", "q"];

/// The keys one `band` entry of a preset may carry.
const BAND_KEYS: [&str; 3] = ["f", "g", "q"];

/// Parse and validate one `[[preset]]` block, naming it in every error the way a bad
/// `[[device]]` block is named in its own.
///
/// Validated here rather than left to a derived `Deserialize`: `bands`, `mode` and `loudness`
/// each need a range or word-list check serde alone would not enforce, a non-finite band value
/// (`nan`, `inf`, written as such) parses as a plain `f64` and must be rejected by hand, and an
/// unknown key must be found while `name` is still in scope, so the error can name the preset
/// it is in.
fn parse_preset(value: &toml::Value) -> Result<Preset, ConfigError> {
    let table = value
        .as_table()
        .ok_or_else(|| ConfigError::Parse("preset is not a table".into()))?;
    let written = table
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    let name = match checked_name(written) {
        Ok(name) => name.to_string(),
        Err(NameError::Blank) => return Err(ConfigError::Parse(NameError::Blank.to_string())),
        Err(why) => {
            return Err(ConfigError::Parse(format!(
                "preset {:?}: {why}",
                written.trim()
            )));
        }
    };
    let err = |reason: String| ConfigError::Parse(format!("preset {name:?}: {reason}"));

    for key in table.keys() {
        if !PRESET_KEYS.contains(&key.as_str()) {
            return Err(err(format!("unknown key {key:?}")));
        }
    }
    if is_builtin(&name) {
        return Err(err("that name is a built-in preset's".into()));
    }
    if table.contains_key("filter") {
        return parse_imported(&err, name.clone(), table);
    }
    for key in ["origin", "preamp"] {
        if table.contains_key(key) {
            return Err(err(format!(
                "{key} belongs to an imported preset, which has filter"
            )));
        }
    }

    let bands: u8 = match table.get("bands").and_then(toml::Value::as_integer) {
        Some(n @ (3 | 5)) => n as u8,
        Some(n) => return Err(err(format!("bands must be 3 or 5, not {n}"))),
        None => return Err(err("bands must be 3 or 5".into())),
    };

    let mode_word = table.get("mode").and_then(toml::Value::as_str);
    let mode: EqMode = match mode_word {
        Some(word) => word
            .parse()
            .map_err(|_| err(format!("mode must be flat, eq, or loudness, not {word:?}")))?,
        None => return Err(err("mode must be flat, eq, or loudness".into())),
    };

    let loudness: u8 = match table.get("loudness") {
        None => 0,
        Some(value) => match value.as_integer() {
            Some(n) if (0..=i64::from(LOUDNESS_MAX)).contains(&n) => n as u8,
            Some(n) => return Err(err(format!("loudness must be 0-127, not {n}"))),
            None => return Err(err("loudness must be 0-127".into())),
        },
    };

    let raw_band: Vec<&toml::Value> = match table.get("band") {
        None => Vec::new(),
        Some(value) => match value.as_array() {
            Some(array) => array.iter().collect(),
            None => return Err(err("band must be an array".into())),
        },
    };
    if raw_band.len() != usize::from(bands) {
        return Err(err(format!(
            "bands is {bands} but there are {} band entries",
            raw_band.len()
        )));
    }
    let band = raw_band
        .into_iter()
        .enumerate()
        .map(|(index, entry)| parse_preset_band(&err, index, entry))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Preset {
        name,
        bands,
        mode,
        loudness,
        band,
        source: None,
    })
}

/// Parse and validate the keys of an imported `[[preset]]` block: `origin`, `preamp` and
/// `filter`, with none of the keys a preset of bands carries.
fn parse_imported(
    err: &dyn Fn(String) -> ConfigError,
    name: String,
    table: &toml::map::Map<String, toml::Value>,
) -> Result<Preset, ConfigError> {
    if BAND_PRESET_KEYS.iter().any(|key| table.contains_key(*key)) {
        return Err(err("an imported preset holds filter, not band".into()));
    }
    let origin = match table.get("origin") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .ok_or_else(|| err("origin must be text".into()))?
                .to_string(),
        ),
    };
    let preamp_db = match table.get("preamp") {
        None => 0.0,
        Some(value) => as_f64(value).ok_or_else(|| err("preamp must be a number".into()))?,
    };
    let entries = table
        .get("filter")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| err("filter must be an array".into()))?;
    let filters = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| parse_filter(err, index, entry))
        .collect::<Result<Vec<_>, _>>()?;
    let correction = Correction { preamp_db, filters };
    apo::validate(&correction).map_err(|why| err(why.to_string()))?;
    Ok(Preset {
        name,
        bands: 0,
        mode: EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
        source: Some(Imported { origin, correction }),
    })
}

/// Parse one entry of an imported preset's `filter` array, naming the preset (through `err`)
/// and the filter (counted from 1) in every error. The bounds on its numbers are
/// [`apo::validate`]'s.
fn parse_filter(
    err: &dyn Fn(String) -> ConfigError,
    index: usize,
    value: &toml::Value,
) -> Result<SourceFilter, ConfigError> {
    let at = index + 1;
    let table = value
        .as_table()
        .ok_or_else(|| err(format!("filter {at}: not a table")))?;
    for key in table.keys() {
        if !FILTER_KEYS.contains(&key.as_str()) {
            return Err(err(format!("filter {at}: unknown key {key:?}")));
        }
    }
    let word = table
        .get("type")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| err(format!("filter {at}: type must be PK, LSC or HSC")))?;
    let kind = match word.to_ascii_uppercase().as_str() {
        "PK" | "LSC" | "HSC" => word.parse::<FilterKind>().ok(),
        _ => None,
    }
    .ok_or_else(|| {
        err(format!(
            "filter {at}: type must be PK, LSC or HSC, not {word:?}"
        ))
    })?;
    let get = |field: &str| -> Result<f64, ConfigError> {
        let raw = table
            .get(field)
            .ok_or_else(|| err(format!("filter {at}: {field} is required")))?;
        as_f64(raw).ok_or_else(|| err(format!("filter {at}: {field} must be a number")))
    };
    Ok(SourceFilter {
        kind,
        f: get("f")?,
        g: get("g")?,
        q: get("q")?,
    })
}

/// Parse and validate one entry of a preset's `band` array, naming both the preset (through
/// `err`) and the band (counted from 1, as [`preset::Clamp`](svolder_profiles::tone::preset::Clamp)
/// counts them) in every error.
fn parse_preset_band(
    err: &dyn Fn(String) -> ConfigError,
    index: usize,
    value: &toml::Value,
) -> Result<PresetBand, ConfigError> {
    let table = value
        .as_table()
        .ok_or_else(|| err(format!("band {}: not a table", index + 1)))?;
    for key in table.keys() {
        if !BAND_KEYS.contains(&key.as_str()) {
            return Err(err(format!("band {}: unknown key {key:?}", index + 1)));
        }
    }
    let get = |field: &str| -> Result<f64, ConfigError> {
        let raw = table
            .get(field)
            .ok_or_else(|| err(format!("band {}: {field} is required", index + 1)))?;
        let n = as_f64(raw)
            .ok_or_else(|| err(format!("band {}: {field} must be a number", index + 1)))?;
        if !n.is_finite() {
            return Err(err(format!(
                "band {}: {field} must be finite, not {n}",
                index + 1
            )));
        }
        Ok(n)
    };
    Ok(PresetBand {
        f: get("f")?,
        g: get("g")?,
        q: get("q")?,
    })
}

/// A TOML number as `f64`, whether it is written as an integer or a float.
fn as_f64(value: &toml::Value) -> Option<f64> {
    match value {
        toml::Value::Integer(n) => Some(*n as f64),
        toml::Value::Float(n) => Some(*n),
        _ => None,
    }
}
