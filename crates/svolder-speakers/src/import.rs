// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making a converted set a stored one: the sidecar's fields from the conversion, and the
//! import of a file the person has.

use crate::catalogue::Room;
use crate::convert::{Converted, Recipe, convert, convert_wav_pair};
use crate::error::ConvertError;
use crate::sha::sha256_hex;
use crate::store::{SetMeta, Store, StoreError};

/// The licence words of a set that comes from the person's own file.
pub const OWN_LICENCE: &str = "LicenseRef-own-file";

/// A set that was stored, and the older ones that went to make room for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    /// What was stored.
    pub meta: SetMeta,
    /// The ids evicted (the oldest unused sets beyond the limit).
    pub evicted: Vec<String>,
}

/// Why an import failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ImportError {
    /// The file is not one the converter reads.
    #[error("{0}")]
    Convert(#[from] ConvertError),
    /// The set could not be stored.
    #[error("{0}")]
    Store(#[from] StoreError),
}

/// What identifies a set apart from its numbers.
pub(crate) struct Identity<'a> {
    pub id: &'a str,
    pub room: Room,
    pub label: &'a str,
    pub licence: &'a str,
    pub credit: &'a str,
    pub source_url: &'a str,
    pub source_sha256: &'a str,
    pub own: bool,
}

/// Store `converted` under `identity`, used at `now` (Unix seconds); `chosen` is spared by the
/// eviction.
pub(crate) fn store_converted(
    store: &Store,
    converted: &Converted,
    identity: &Identity<'_>,
    now: u64,
    chosen: Option<&str>,
) -> Result<Saved, StoreError> {
    let report = &converted.report;
    let meta = SetMeta {
        id: identity.id.to_string(),
        room: identity.room,
        label: identity.label.to_string(),
        licence: identity.licence.to_string(),
        credit: identity.credit.to_string(),
        source_url: identity.source_url.to_string(),
        source_sha256: identity.source_sha256.to_string(),
        wav_sha256: String::new(),
        rate: converted.rate(),
        taps: report.taps,
        taps_ms: report.taps_ms,
        angles: converted.angles,
        loudness_db: report.loudness_db,
        scale_db: report.scale_db,
        worst_gain_db: report.worst_gain_db,
        cut: report.cut,
        own: identity.own,
        added: now,
        used: now,
    };
    let evicted = store.save(meta.clone(), &converted.wav_bytes(), chosen)?;
    let meta = store.check(&meta.id)?;
    Ok(Saved { meta, evicted })
}

/// The label a file name makes: printable characters only, at most 60 of them.
fn clean_label(label: &str) -> String {
    let label: String = label.chars().filter(|c| !c.is_control()).take(60).collect();
    match label.trim() {
        "" => "Own file".to_string(),
        text => text.to_string(),
    }
}

/// Import the SOFA file or WAV in `bytes` (a 4- or 14-channel WAV) as the person's own set.
pub fn import(
    store: &Store,
    bytes: &[u8],
    label: &str,
    room: Room,
    now: u64,
    chosen: Option<&str>,
) -> Result<Saved, ImportError> {
    let converted = convert(bytes, &Recipe::default())?;
    Ok(save_own(
        store,
        &converted,
        sha256_hex(bytes),
        label,
        room,
        now,
        chosen,
    )?)
}

/// Import two 2-channel WAVs (the left speaker's, the right speaker's) as the person's own set.
pub fn import_pair(
    store: &Store,
    left: &[u8],
    right: &[u8],
    label: &str,
    room: Room,
    now: u64,
    chosen: Option<&str>,
) -> Result<Saved, ImportError> {
    let converted = convert_wav_pair(left, right)?;
    let sha = sha256_hex(&[left, right].concat());
    Ok(save_own(store, &converted, sha, label, room, now, chosen)?)
}

fn save_own(
    store: &Store,
    converted: &Converted,
    source_sha256: String,
    label: &str,
    room: Room,
    now: u64,
    chosen: Option<&str>,
) -> Result<Saved, StoreError> {
    let id = format!("own-{}", &source_sha256[..8]);
    let label = clean_label(label);
    let identity = Identity {
        id: &id,
        room,
        label: &label,
        licence: OWN_LICENCE,
        credit: "",
        source_url: "",
        source_sha256: &source_sha256,
        own: true,
    };
    store_converted(store, converted, &identity, now, chosen)
}

#[cfg(test)]
mod tests;
