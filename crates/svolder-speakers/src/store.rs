// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The converted sets on disk: `<id>.wav` (what the convolvers play) and `<id>.toml` (what the
//! screens show and the checksum the WAV is checked against), in one directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::catalogue::Room;
use crate::ir::Angles;

/// The most sets kept; the oldest unused one goes when another is saved.
pub const MAX_SETS: usize = 32;

/// What is known about a stored set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMeta {
    /// The id (also the file name).
    pub id: String,
    /// With room or no room.
    pub room: Room,
    /// The dataset and subject, or the file's name for an own file.
    pub label: String,
    /// The licence as an SPDX identifier (`LicenseRef-` for words without one).
    pub licence: String,
    /// The dataset's own credit words; empty where none are asked for.
    pub credit: String,
    /// The page the dataset is described on; empty for an own file.
    pub source_url: String,
    /// The SHA-256 of what was downloaded or imported.
    pub source_sha256: String,
    /// The SHA-256 of the WAV (set by [`Store::save`]).
    pub wav_sha256: String,
    /// The sample rate in Hz.
    pub rate: u32,
    /// The samples of each response.
    pub taps: usize,
    /// The length in milliseconds.
    pub taps_ms: f64,
    /// Where the speakers' measurements were, when the source said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angles: Option<Angles>,
    /// How much quieter (negative) a centred signal is than with no virtual speakers, in dB.
    pub loudness_db: f64,
    /// The gain applied at import, in dB.
    pub scale_db: f64,
    /// The worst gain after scaling, in dB.
    pub worst_gain_db: f64,
    /// Whether the responses were cut to the length cap.
    pub cut: bool,
    /// Whether the source is the person's own file.
    pub own: bool,
    /// When it was stored, in Unix seconds.
    pub added: u64,
    /// When it was last used, in Unix seconds.
    pub used: u64,
}

/// Whether a stored set can be played.
#[derive(Debug, Clone, PartialEq)]
pub enum Health {
    /// The WAV is there, whole, and matches its sidecar.
    Ok,
    /// It cannot be played: why, in words.
    Damaged(String),
}

/// One set found in the directory.
#[derive(Debug, Clone, PartialEq)]
pub struct SetStatus {
    /// The id.
    pub id: String,
    /// The sidecar, when it could be read.
    pub meta: Option<SetMeta>,
    /// Whether it can be played.
    pub health: Health,
}

/// Why a set could not be stored, read or removed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StoreError {
    /// The id is not a safe file name.
    #[error("{0:?} is not a valid set id")]
    BadId(String),
    /// The disk refused.
    #[error("{what}: {reason}")]
    Io {
        /// What was being done.
        what: String,
        /// The system's words.
        reason: String,
    },
    /// The set is the one in use and cannot be removed.
    #[error("{0} is in use and cannot be removed")]
    InUse(String),
    /// The set is missing or damaged.
    #[error("{id} is missing or damaged ({reason}), fetch it again")]
    Damaged {
        /// The set.
        id: String,
        /// What is wrong.
        reason: String,
    },
}

/// Whether `id` is a safe set id: 1 to 48 of lower-case letters, digits and dashes, not
/// starting with a dash.
pub fn valid_id(id: &str) -> bool {
    (1..=48).contains(&id.len())
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A directory of stored sets.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
    /// The sets a running chain holds, or is about to hold, shared by every clone of the store.
    in_use: Arc<Mutex<BTreeSet<String>>>,
}

impl Store {
    /// The store in `dir`, made absolute so a WAV path from it is fit for a config file.
    pub fn new(dir: &Path) -> Store {
        Store {
            dir: std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf()),
            in_use: Arc::default(),
        }
    }

    /// Mark `ids` as the sets in use, in place of the ones marked before (none for an empty
    /// slice): [`Store::forget`] refuses them and eviction spares them.
    ///
    /// The running chain re-opens its WAV when the sink's rate changes, and a missing WAV is
    /// silence with no exit, so the controller marks the set the chain holds, and while the chain
    /// is applied again with another set, both: the old one until the new chain has replaced it,
    /// and the old one alone when the apply fails. The pin is shared by every clone of the store.
    pub fn set_in_use(&self, ids: &[&str]) {
        *self
            .in_use
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            ids.iter().map(|id| id.to_string()).collect();
    }

    /// Remove the set's files unless it is in use, the pin's lock held across the check
    /// and the removal: a pin set meanwhile waits for the removal, and a removal waits for a pin
    /// being set, so a set is never removed under a pin that was already visible. Whether the set
    /// was removed.
    fn remove_unless_in_use(&self, id: &str) -> Result<bool, StoreError> {
        let pin = self
            .in_use
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pin.contains(id) {
            return Ok(false);
        }
        #[cfg(test)]
        tests::between_check_and_removal(id);
        self.remove_files(id)?;
        Ok(true)
    }

    /// The directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the set's WAV is (or would be): absolute.
    pub fn wav_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        Ok(self.dir.join(format!("{}.wav", checked(id)?)))
    }

    fn sidecar_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        Ok(self.dir.join(format!("{}.toml", checked(id)?)))
    }

    /// Store `wav` as the set `meta` describes: the WAV first, then the sidecar that completes
    /// it, each through a temporary file renamed into place. The sidecar's WAV checksum is
    /// worked out here. Then the oldest unused sets beyond [`MAX_SETS`] go, never `chosen` and
    /// never this one; their ids are returned.
    pub fn save(
        &self,
        mut meta: SetMeta,
        wav: &[u8],
        chosen: Option<&str>,
    ) -> Result<Vec<String>, StoreError> {
        let (wav_path, sidecar_path) = (self.wav_path(&meta.id)?, self.sidecar_path(&meta.id)?);
        meta.wav_sha256 = crate::sha256_hex(wav);
        let sidecar = toml::to_string(&meta).map_err(|error| io("encoding the sidecar", error))?;
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| io("creating the speakers directory", error))?;
        write_atomic(&wav_path, wav)?;
        write_atomic(&sidecar_path, sidecar.as_bytes())?;
        self.evict(chosen, Some(&meta.id))
    }

    /// Remove files that are not a set (no readable sidecar), then the oldest unused sets until
    /// at most [`MAX_SETS`] are left, sparing `chosen`, `keep` and the sets in use; the ids of
    /// the sets removed (not of the orphan files).
    pub fn evict(
        &self,
        chosen: Option<&str>,
        keep: Option<&str>,
    ) -> Result<Vec<String>, StoreError> {
        let mut sets = Vec::new();
        for status in self.list() {
            match status.meta {
                Some(meta) if meta.id == status.id => sets.push(meta),
                _ => {
                    self.remove_unless_in_use(&status.id)?;
                }
            }
        }
        let mut gone = Vec::new();
        sets.sort_by(|a, b| (a.used, a.added, &a.id).cmp(&(b.used, b.added, &b.id)));
        let mut count = sets.len();
        for set in sets {
            if count <= MAX_SETS {
                break;
            }
            if [chosen, keep].contains(&Some(set.id.as_str())) {
                continue;
            }
            if self.remove_unless_in_use(&set.id)? {
                gone.push(set.id);
                count -= 1;
            }
        }
        Ok(gone)
    }

    /// Every set in the directory, by id, each with its health.
    pub fn list(&self) -> Vec<SetStatus> {
        let mut ids = std::collections::BTreeSet::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                for suffix in [".wav", ".toml"] {
                    if let Some(id) = name.strip_suffix(suffix).filter(|id| valid_id(id)) {
                        ids.insert(id.to_string());
                    }
                }
            }
        }
        ids.into_iter()
            .map(|id| match self.check(&id) {
                Ok(meta) => SetStatus {
                    id,
                    meta: Some(meta),
                    health: Health::Ok,
                },
                Err(StoreError::Damaged { reason, .. }) => SetStatus {
                    meta: self.read_sidecar(&id).ok(),
                    id,
                    health: Health::Damaged(reason),
                },
                Err(other) => SetStatus {
                    id,
                    meta: None,
                    health: Health::Damaged(other.to_string()),
                },
            })
            .collect()
    }

    /// The sidecars of the sets whose sidecar reads, names its set, and has a WAV file beside
    /// it, by id, without reading the WAVs: a quick look for a list that is read again often.
    /// A WAV damaged in place is listed here; [`Store::check`] (and [`Store::list`]) finds it.
    pub fn list_sidecars(&self) -> Vec<SetMeta> {
        let mut ids = std::collections::BTreeSet::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if let Some(id) = name.strip_suffix(".toml").filter(|id| valid_id(id)) {
                    ids.insert(id.to_string());
                }
            }
        }
        ids.into_iter()
            .filter_map(|id| {
                let meta = self.read_sidecar(&id).ok().filter(|meta| meta.id == id)?;
                self.wav_path(&id).ok()?.is_file().then_some(meta)
            })
            .collect()
    }

    /// The set's sidecar, once its WAV has been checked against it: present, matching the
    /// checksum, readable as a 4-channel WAV of the sidecar's rate and length. The controller
    /// calls this before every apply that names the set.
    pub fn check(&self, id: &str) -> Result<SetMeta, StoreError> {
        let damaged = |reason: &str| StoreError::Damaged {
            id: id.to_string(),
            reason: reason.to_string(),
        };
        let meta = self
            .read_sidecar(id)
            .map_err(|_| damaged("its sidecar is missing or unreadable"))?;
        if meta.id != id {
            return Err(damaged("its sidecar names another set"));
        }
        let bytes = std::fs::read(self.wav_path(id)?).map_err(|_| damaged("its WAV is missing"))?;
        if crate::sha256_hex(&bytes) != meta.wav_sha256 {
            return Err(damaged("its WAV does not match its checksum"));
        }
        let wav = match crate::wav::read_channels(&bytes, &[4]) {
            Ok(wav) => wav,
            Err(crate::ConvertError::Channels(_)) => {
                return Err(damaged(
                    "its WAV is not the four-channel file its sidecar describes",
                ));
            }
            Err(_) => return Err(damaged("its WAV header is unreadable")),
        };
        if wav.channels.iter().flatten().any(|tap| !tap.is_finite()) {
            return Err(damaged("its WAV holds values that are not numbers"));
        }
        if wav.channels.len() != 4
            || wav.rate != meta.rate
            || wav.channels.iter().any(|c| c.len() != meta.taps)
        {
            return Err(damaged(
                "its WAV is not the four-channel file its sidecar describes",
            ));
        }
        Ok(meta)
    }

    fn read_sidecar(&self, id: &str) -> Result<SetMeta, StoreError> {
        let text = std::fs::read_to_string(self.sidecar_path(id)?)
            .map_err(|error| io("reading the sidecar", error))?;
        toml::from_str(&text).map_err(|error| io("parsing the sidecar", error))
    }

    /// Mark the set as used at `now` (Unix seconds), so it is the last to be evicted. A set
    /// whose WAV is gone (a forget got there first) is left as it is; one that went while its
    /// sidecar was rewritten loses the sidecar again, so a touch never leaves a set with no WAV.
    pub fn touch(&self, id: &str, now: u64) -> Result<(), StoreError> {
        let wav = self.wav_path(id)?;
        if !wav.is_file() {
            return Ok(());
        }
        let mut meta = self.read_sidecar(id)?;
        meta.used = now;
        let text = toml::to_string(&meta).map_err(|error| io("encoding the sidecar", error))?;
        let sidecar = self.sidecar_path(id)?;
        write_atomic(&sidecar, text.as_bytes())?;
        if !wav.is_file() {
            let _ = std::fs::remove_file(&sidecar);
        }
        Ok(())
    }

    /// Remove the set's files; a set that is not there is not an error, a set in use is.
    pub fn forget(&self, id: &str) -> Result<(), StoreError> {
        match self.remove_unless_in_use(checked(id)?)? {
            true => Ok(()),
            false => Err(StoreError::InUse(id.to_string())),
        }
    }

    fn remove_files(&self, id: &str) -> Result<(), StoreError> {
        for path in [self.wav_path(id)?, self.sidecar_path(id)?] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io("removing a set", error)),
            }
        }
        Ok(())
    }
}

fn checked(id: &str) -> Result<&str, StoreError> {
    match valid_id(id) {
        true => Ok(id),
        false => Err(StoreError::BadId(id.to_string())),
    }
}

fn io(what: &str, error: impl std::fmt::Display) -> StoreError {
    StoreError::Io {
        what: what.to_string(),
        reason: error.to_string(),
    }
}

/// Write `bytes` to `path` through a temporary file beside it, renamed into place.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    use std::io::Write;
    // A name of its own for every write, so two writes of one set at once never share a file.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp-{}-{n}", std::process::id()));
    let written = std::fs::File::create(&tmp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    match written.and_then(|()| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&tmp);
            Err(io("writing a set", error))
        }
    }
}

#[cfg(test)]
mod tests;
