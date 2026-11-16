// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the speakers ops share: the one store of converted sets, the fetch client, and the gate
//! that serialises every write to the store.
//!
//! Everything here blocks (disk, checksums, conversion, the download), so the service runs it on
//! blocking threads. The gate is held for the whole of a fetch or an import: a store's save and
//! its eviction look at the whole directory, so two at once could take a set that is half
//! written for an orphan, and two fetches of one set would write the same temporary file. A
//! fetch that was cancelled keeps the gate until its thread notices (at the next chunk it reads,
//! or when its connection times out), and the next write waits for it.

use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use svolder_ipc::{
    CODE_USAGE, Reason, Response, SpeakersAngles, SpeakersCatalogue, SpeakersRoom, SpeakersSaved,
    SpeakersSet,
};
use svolder_speakers::{
    Cancel, Client, Entry, FetchError, Health, ImportError, MAX_SETS, MAX_SOURCE_BYTES, OFF_WORDS,
    Room, SetMeta, SetStatus, Store, StoreError, sha256_hex,
};

use crate::handler::jobs::JobError;
use crate::locks::locked;

/// Why an op did not finish: the daemon's words and the reason a front end matches on, or that it
/// was cancelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) message: String,
    pub(super) reason: Reason,
    pub(super) cancelled: bool,
}

impl Failure {
    pub(super) fn new(message: impl Into<String>, reason: Reason) -> Failure {
        Failure {
            message: message.into(),
            reason,
            cancelled: false,
        }
    }

    fn cancelled() -> Failure {
        Failure {
            message: "cancelled".into(),
            reason: Reason::Internal,
            cancelled: true,
        }
    }

    /// The answer to a request that failed this way.
    pub(super) fn answer(&self) -> Response {
        Response::error(CODE_USAGE, self.message.clone()).with_reason(self.reason)
    }

    /// The end of a job that failed this way.
    pub(super) fn job(self) -> JobError {
        match self.cancelled {
            true => JobError::Cancelled,
            false => JobError::failed(self.message, self.reason),
        }
    }

    fn of_fetch(error: &FetchError) -> Failure {
        let reason = match error {
            FetchError::Cancelled => return Failure::cancelled(),
            FetchError::NotListed(_) => Reason::NotListed,
            FetchError::Off
            | FetchError::Unreachable { .. }
            | FetchError::Checksum(_)
            | FetchError::TooLarge(_)
            | FetchError::Archive(_) => Reason::Unreachable,
            FetchError::Convert(_) => Reason::Invalid,
            FetchError::Store(inner) => return Failure::of_store(inner),
        };
        Failure::new(error.to_string(), reason)
    }

    fn of_store(error: &StoreError) -> Failure {
        let reason = match error {
            StoreError::BadId(_) => Reason::Invalid,
            StoreError::InUse(_) => Reason::Busy,
            StoreError::Damaged { .. } => Reason::Missing,
            StoreError::Io { .. } => Reason::Internal,
        };
        Failure::new(error.to_string(), reason)
    }

    fn of_import(error: &ImportError) -> Failure {
        match error {
            ImportError::Convert(error) => Failure::new(error.to_string(), Reason::Invalid),
            ImportError::Store(error) => Failure::of_store(error),
        }
    }
}

/// What the computer EQ's controller tells the service, and is told by it: which set the person
/// chose (the store's eviction never removes it, whether or not it plays now), and that the sets
/// changed (a fetch, an import or a forget, so the view of the sets is published again).
pub struct SpeakersLinks {
    /// The id of the chosen set, as the controller keeps it.
    pub chosen: Box<dyn Fn() -> Option<String> + Send + Sync>,
    /// Called after every op that changed the store.
    pub changed: Box<dyn Fn() + Send + Sync>,
}

/// The set store, the fetch client and the write gate: one of each in the daemon.
pub(super) struct Core {
    pub(super) client: Client,
    pub(super) store: Arc<Store>,
    gate: Mutex<()>,
    links: OnceLock<SpeakersLinks>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn wire_room(room: Room) -> SpeakersRoom {
    match room {
        Room::With => SpeakersRoom::With,
        Room::No => SpeakersRoom::No,
    }
}

pub(crate) fn room_of(room: SpeakersRoom) -> Room {
    match room {
        SpeakersRoom::With => Room::With,
        SpeakersRoom::No => Room::No,
    }
}

/// A catalogue entry that is not on this machine.
fn listed(entry: &Entry, damaged: Option<String>) -> SpeakersSet {
    SpeakersSet {
        id: entry.id.into(),
        room: wire_room(entry.room),
        label: entry.label.into(),
        licence: entry.licence.into(),
        credit: entry.credit.into(),
        source_url: entry.source_page.into(),
        bytes: entry.bytes,
        own: false,
        fetched: false,
        damaged,
        angles: None,
        taps_ms: 0.0,
        rate: 0,
        loudness_db: 0.0,
        cut: false,
    }
}

/// A set that is stored, with what its sidecar says. `damaged` is why it cannot be played.
fn stored(meta: &SetMeta, bytes: u64, damaged: Option<String>) -> SpeakersSet {
    SpeakersSet {
        id: meta.id.clone(),
        room: wire_room(meta.room),
        label: meta.label.clone(),
        licence: meta.licence.clone(),
        credit: meta.credit.clone(),
        source_url: meta.source_url.clone(),
        bytes,
        own: meta.own,
        fetched: damaged.is_none(),
        damaged,
        angles: meta.angles.map(|found| SpeakersAngles {
            left_deg: found.left.az_deg,
            right_deg: found.right.az_deg,
            distance_m: found.left.distance_m,
        }),
        taps_ms: meta.taps_ms,
        rate: meta.rate,
        loudness_db: meta.loudness_db,
        cut: meta.cut,
    }
}

/// The set a stored status describes, whether or not it can be played.
fn of_status(status: &SetStatus, entry: Option<&Entry>) -> SpeakersSet {
    let bytes = entry.map_or(0, |entry| entry.bytes);
    let damaged = match &status.health {
        Health::Ok => None,
        Health::Damaged(reason) => Some(reason.clone()),
    };
    match (&status.meta, entry, damaged) {
        // A damaged set is listed as not fetched, under its catalogue entry when it has one.
        (_, Some(entry), Some(why)) => listed(entry, Some(why)),
        (Some(meta), _, damaged) => stored(meta, bytes, damaged),
        (None, _, damaged) => SpeakersSet {
            id: status.id.clone(),
            room: SpeakersRoom::No,
            label: status.id.clone(),
            licence: String::new(),
            credit: String::new(),
            source_url: String::new(),
            bytes: 0,
            own: false,
            fetched: false,
            damaged: Some(damaged.unwrap_or_else(|| "its files cannot be read".into())),
            angles: None,
            taps_ms: 0.0,
            rate: 0,
            loudness_db: 0.0,
            cut: false,
        },
    }
}

impl Core {
    pub(super) fn new(client: Client, store: impl Into<Arc<Store>>) -> Core {
        Core {
            client,
            store: store.into(),
            gate: Mutex::new(()),
            links: OnceLock::new(),
        }
    }

    /// Link the controller; the first link stays.
    pub(super) fn link(&self, links: SpeakersLinks) {
        let _ = self.links.set(links);
    }

    pub(super) fn chosen(&self) -> Option<String> {
        self.links.get().and_then(|links| (links.chosen)())
    }

    fn changed(&self) {
        if let Some(links) = self.links.get() {
            (links.changed)();
        }
    }

    /// The write gate, for the tests that hold it while an op waits.
    #[cfg(test)]
    pub(super) fn gate(&self) -> std::sync::MutexGuard<'_, ()> {
        locked(&self.gate)
    }

    /// The catalogue's sets in its order, then every other stored set by id.
    pub(super) fn catalogue(&self) -> SpeakersCatalogue {
        let statuses = self.store.list();
        let mut sets: Vec<SpeakersSet> = svolder_speakers::entries()
            .iter()
            .map(
                |entry| match statuses.iter().find(|status| status.id == entry.id) {
                    Some(status) => of_status(status, Some(entry)),
                    None => listed(entry, None),
                },
            )
            .collect();
        sets.extend(
            statuses
                .iter()
                .filter(|status| svolder_speakers::find(&status.id).is_none())
                .map(|status| of_status(status, None)),
        );
        let fetching = !self.client.is_off();
        SpeakersCatalogue {
            sets,
            fetching,
            note: (!fetching).then(|| OFF_WORDS.to_string()),
            max_sets: MAX_SETS,
        }
    }

    /// Fetch the catalogue set `id`, telling `progress` the bytes received and expected.
    pub(super) fn fetch(
        &self,
        id: &str,
        cancel: &Cancel,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<SpeakersSaved, Failure> {
        let _gate = locked(&self.gate);
        if cancel.is_cancelled() {
            return Err(Failure::cancelled());
        }
        let saved = self
            .client
            .fetch(
                &self.store,
                id,
                now(),
                self.chosen().as_deref(),
                cancel,
                progress,
            )
            .map_err(|error| Failure::of_fetch(&error))?;
        self.changed();
        let bytes = svolder_speakers::find(id).map_or(0, |entry| entry.bytes);
        Ok(SpeakersSaved {
            set: stored(&saved.meta, bytes, None),
            evicted: saved.evicted,
        })
    }

    /// Convert the file at `path` and keep it as the person's own set. A cancel that comes while
    /// the conversion runs cannot stop it: the set it made is removed again (unless the same file
    /// was imported before) and the job ends cancelled.
    pub(super) fn import(
        &self,
        path: &Path,
        label: Option<&str>,
        room: Option<SpeakersRoom>,
        cancel: &Cancel,
    ) -> Result<SpeakersSaved, Failure> {
        let bytes = read_source(path)?;
        let id = format!("own-{}", &sha256_hex(&bytes)[..8]);
        let _gate = locked(&self.gate);
        if cancel.is_cancelled() {
            return Err(Failure::cancelled());
        }
        let existed = self.store.check(&id).is_ok();
        let label = label.map(str::to_string).unwrap_or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let room = room.map_or(Room::No, room_of);
        let chosen = self.chosen();
        let saved =
            svolder_speakers::import(&self.store, &bytes, &label, room, now(), chosen.as_deref())
                .map_err(|error| Failure::of_import(&error))?;
        if cancel.is_cancelled() {
            if !existed {
                let _ = self.store.forget(&saved.meta.id);
            }
            // The save may have made room by evicting an older set.
            self.changed();
            return Err(Failure::cancelled());
        }
        self.changed();
        Ok(SpeakersSaved {
            set: stored(&saved.meta, 0, None),
            evicted: saved.evicted,
        })
    }

    /// Remove the set `id`: refused while a fetch or import runs, while the set plays, and when
    /// there is no such set.
    pub(super) fn forget(&self, id: &str) -> Result<(), Failure> {
        let Ok(_gate) = self.gate.try_lock() else {
            return Err(Failure::new(
                "a fetch or an import is running; try again when it ends",
                Reason::Busy,
            ));
        };
        if !svolder_speakers::valid_id(id) {
            return Err(Failure::of_store(&StoreError::BadId(id.into())));
        }
        let Some(found) = self.store.list().into_iter().find(|status| status.id == id) else {
            return Err(Failure::new(
                format!("{id} is not stored on this machine"),
                Reason::Missing,
            ));
        };
        let label = found.meta.map_or_else(|| id.to_string(), |meta| meta.label);
        self.store.forget(id).map_err(|error| match error {
            StoreError::InUse(_) => Failure::new(
                svolder_host::eq::speakers::plays_now_words(&label),
                Reason::Busy,
            ),
            other => Failure::of_store(&other),
        })?;
        self.changed();
        Ok(())
    }
}

/// The file's bytes: a regular file of at most [`MAX_SOURCE_BYTES`]. A pipe or a device is
/// refused before it is opened (opening a pipe with no writer would wait for ever), and the open
/// itself does not wait.
fn read_source(path: &Path) -> Result<Vec<u8>, Failure> {
    use std::os::unix::fs::OpenOptionsExt;
    let refuse = |why: String| Failure::new(why, Reason::Invalid);
    let unreadable =
        |error: std::io::Error| refuse(format!("{} cannot be read: {error}", path.display()));
    let before = std::fs::metadata(path).map_err(unreadable)?;
    if !before.is_file() {
        return Err(refuse(format!("{} is not a regular file", path.display())));
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(unreadable)?;
    let meta = file.metadata().map_err(unreadable)?;
    if !meta.is_file() {
        return Err(refuse(format!("{} is not a regular file", path.display())));
    }
    if meta.len() > MAX_SOURCE_BYTES as u64 {
        return Err(refuse("the file is larger than 64 MiB".into()));
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(refuse("the file is larger than 64 MiB".into()));
    }
    Ok(bytes)
}
