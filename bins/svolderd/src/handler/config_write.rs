// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's writes to its configuration file, and the one snapshot of it a request reads.
//!
//! The socket, the web server and the MCP server reach config writes through this module and
//! never edit the file themselves: [`Daemon::edit_then_reload`] runs a writer of the core on the
//! blocking pool, where it may wait on the file's lock and fsync, and then has the daemon reload
//! the file; [`Daemon::save_preset`] is the one way a preset is saved.
//!
//! A request reads the configuration the daemon holds ([`Daemon::config_snapshot`]) and never
//! loads the file. The snapshot is the file as of the last reload, or as of the last write the
//! daemon made, whichever read it later: a hand edit of `config.toml` takes effect at the next
//! reload, in the socket, the web page and the MCP tools alike. A reload happens on SIGHUP, on a
//! `reload` request (which the `svolder` verbs that change the file send, and `svolder daemon
//! reload`) and after each write made here; nothing watches the file. Only a write reads the
//! file again, under the writer's lock, inside the blocking edit, and the reload that follows
//! reads it once more to act on it.
//!
//! Every read of the file takes a ticket from a counter before it reads, and a snapshot is held
//! only if its ticket is not older than the held one's: a read that started later saw the file at
//! least as new, so a slow writer or reload can never put an older list over a newer one.
//!
//! The snapshot is held with the file's modification time as it was just before the read that
//! made it, on the same ticket rule. [`Daemon::config_state`] looks the file's time up when
//! asked and says whether it differs from that stamp, so a hand edit is told and the daemon's
//! own write, whose read-back takes the stamp, is not. A timestamp is all it compares, and
//! nothing applies the edit.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use svolder_ipc::{CODE_USAGE, Config, ConfigError, Reason, Response};
use svolder_profiles::tone::preset::{self, NameRefusal, Preset};

use super::Daemon;
use crate::locks::{read, written};
use crate::log;

/// How long a write waits for the daemon's reload before it answers anyway; the reload goes on.
/// A reload that finds `[web]` changed in the file stops the web server a request may be
/// coming through, and that answer must not be what holds the stop up.
pub(super) const RELOAD_WAIT: Duration = Duration::from_secs(5);

/// An edit of the configuration file at the path the daemon reads.
pub type ConfigEdit = Box<dyn FnOnce(&Path) -> Result<(), ConfigError> + Send>;

/// Why a configuration write did not happen.
#[derive(Debug)]
pub enum ConfigWriteError {
    /// The daemon has no configuration file of its own to write.
    Unknown,
    /// The file cannot be written, in the words the writer gives for it.
    Off(String),
    /// A name the core refuses for a preset: taken, or not a name at all (the core's words).
    Name(NameRefusal),
    /// A built-in preset where only the user's own can be written, in the words that say so.
    Builtin(String),
    /// A preset the user's library does not have, in the words that say so.
    NoSuchPreset(String),
    /// A writer of the core refused.
    Refused(ConfigError),
    /// The blocking task of a write did not finish.
    Stopped,
    /// The blocking check that the file can be written did not finish.
    Unchecked,
    /// The daemon is stopping and writes nothing more.
    Stopping,
}

/// The snapshot, the ticket of the read it came from, and the file's time as of just before that
/// read.
pub(super) struct Held {
    generation: u64,
    config: Arc<Config>,
    stamp: Option<SystemTime>,
}

impl Held {
    /// The configuration read before any ticket was taken, with the file's time from before it.
    pub(super) fn first(config: Config, stamp: Option<SystemTime>) -> Held {
        Held {
            generation: 0,
            config: Arc::new(config),
            stamp,
        }
    }
}

/// The modification time of the file at `path`: `None` when there is no such file or its time
/// cannot be had. Taken just before a read of the file, so an edit that lands during the read
/// shows as a change rather than hiding behind it.
pub(crate) fn file_stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// Whether there is no file at `path`: a file that cannot be looked at for another reason (a
/// folder that cannot be read) is not told as missing.
pub(super) fn gone(path: &Path) -> bool {
    matches!(
        std::fs::metadata(path),
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound
    )
}

/// The modification time the snapshot `daemon` holds was read at; `None` for a file that was not
/// there then.
pub(super) fn held_stamp(daemon: &Daemon) -> Option<SystemTime> {
    read(&daemon.snapshot).stamp
}

/// The refusal of a reload or a write that finds the file at `path` gone: the running
/// configuration stays.
pub(super) fn missing_refusal(path: &Path) -> String {
    svolder_ipc::socket::daemon_version::config_missing_refusal(&path.display().to_string())
}

/// The next ticket: taken just before a read of the file, so a higher one read the file later.
pub(super) fn ticket(reads: &AtomicU64) -> u64 {
    reads.fetch_add(1, Ordering::SeqCst) + 1
}

impl std::fmt::Display for ConfigWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigWriteError::Unknown => {
                f.write_str("svolder's configuration file is unknown here")
            }
            ConfigWriteError::Off(words) => f.write_str(words),
            ConfigWriteError::Name(why) => write!(f, "{why}"),
            ConfigWriteError::Builtin(words) | ConfigWriteError::NoSuchPreset(words) => {
                f.write_str(words)
            }
            ConfigWriteError::Refused(why) => write!(f, "{why}"),
            ConfigWriteError::Stopped => f.write_str("the configuration write stopped"),
            ConfigWriteError::Unchecked => f.write_str("the configuration could not be checked"),
            ConfigWriteError::Stopping => f.write_str(super::STOPPING),
        }
    }
}

/// What a configuration access that did not finish says on the socket.
const STOPPED: &str = "the configuration access stopped";

impl ConfigWriteError {
    /// Why the write did not happen, in the word a front end matches on: the one mapping from a
    /// refusal of a configuration write to a socket [`Reason`] (a core writer's own refusal goes
    /// through [`ConfigError::reason`]).
    pub fn reason(&self) -> Reason {
        match self {
            ConfigWriteError::Unknown | ConfigWriteError::Off(_) => Reason::ReadOnly,
            ConfigWriteError::Name(NameRefusal::Taken(_)) => Reason::Taken,
            ConfigWriteError::Name(_) | ConfigWriteError::Builtin(_) => Reason::BadName,
            ConfigWriteError::NoSuchPreset(_) => Reason::Missing,
            ConfigWriteError::Refused(why) => why.reason(),
            ConfigWriteError::Stopped | ConfigWriteError::Unchecked => Reason::Internal,
            ConfigWriteError::Stopping => Reason::Stopping,
        }
    }

    /// The socket's answer to a request whose configuration write was refused: code 1 with the
    /// refusal's words (a write or check that did not finish says so in one sentence) and its
    /// [`ConfigWriteError::reason`].
    pub fn answer(&self) -> Response {
        let words = match self {
            ConfigWriteError::Stopped | ConfigWriteError::Unchecked => STOPPED.to_string(),
            other => other.to_string(),
        };
        Response::error(CODE_USAGE, words).with_reason(self.reason())
    }
}

impl Daemon {
    /// The configuration as the daemon holds it: what every request reads.
    pub(crate) fn config_snapshot(&self) -> Arc<Config> {
        Arc::clone(&read(&self.snapshot).config)
    }

    /// The daemon whose first snapshot was read when the file's time was `stamp`, taken by the
    /// caller before that read (the daemon's own stamp is taken when it is built, after it).
    pub fn with_config_stamp(self, stamp: Option<SystemTime>) -> Self {
        written(&self.snapshot).stamp = stamp;
        self
    }

    /// The line that says the file changed since the last reload, naming it as this daemon reads
    /// it, or `None` while it did not.
    pub(crate) async fn config_changed_words(&self) -> Option<String> {
        let path = self.config_path.display().to_string();
        let (changed, missing) = self.config_state().await;
        match (changed, missing) {
            (_, true) => Some(svolder_ipc::socket::daemon_version::config_missing_line(
                &path,
            )),
            (true, false) => Some(svolder_ipc::socket::daemon_version::config_changed_line(
                &path,
            )),
            (false, false) => None,
        }
    }

    /// Whether the file's modification time now differs from the snapshot's, and whether the file
    /// the snapshot was read from is gone (a daemon that never read a file holds none to miss),
    /// from one look at the file.
    pub(crate) async fn config_state(&self) -> (bool, bool) {
        let path = self.config_path.clone();
        let held = read(&self.snapshot).stamp;
        let looked = tokio::task::spawn_blocking(move || {
            let meta = std::fs::metadata(&path);
            let missing = held.is_some()
                && matches!(&meta, Err(e) if e.kind() == std::io::ErrorKind::NotFound);
            let now = meta.and_then(|m| m.modified()).ok();
            (now != held, missing)
        })
        .await;
        looked.unwrap_or((false, false))
    }

    /// Hold `config`, read under `generation` ([`ticket`]) with the file's `stamp` from before
    /// that read, as the snapshot requests read from now on, unless a read that started later is
    /// held already.
    pub(super) fn hold_snapshot(&self, config: Config, generation: u64, stamp: Option<SystemTime>) {
        let mut held = written(&self.snapshot);
        if generation >= held.generation {
            *held = Held {
                generation,
                config: Arc::new(config),
                stamp,
            };
            // The scenes' view is made of this snapshot.
            self.scenes.poke();
        }
    }

    /// Whether the file's modification time now differs from the one the snapshot was read at:
    /// an edit made by hand since the last reload or write of the daemon's own, a file that
    /// appeared or went, or an older file put back. The time is looked up now, off the async
    /// threads; nothing watches the file and nothing is applied.
    #[cfg(test)]
    pub(crate) async fn config_changed(&self) -> bool {
        self.config_state().await.0
    }

    /// Whether the file the snapshot was read from is gone ([`Daemon::config_state`]).
    #[cfg(test)]
    pub(crate) async fn config_missing(&self) -> bool {
        self.config_state().await.1
    }

    /// The snapshot, for a write of `what` (`presets`, `hops`): refused with why when the file
    /// cannot be written, which is judged off the async threads.
    pub(crate) async fn config_for_write(
        &self,
        what: &str,
    ) -> Result<Arc<Config>, ConfigWriteError> {
        let path = self.config_path.clone();
        match tokio::task::spawn_blocking(move || Config::writable(&path)).await {
            Ok(Ok(())) => Ok(self.config_snapshot()),
            Ok(Err(why)) => Err(ConfigWriteError::Off(format!(
                "Saving {what} is off: {why}"
            ))),
            Err(_) => Err(ConfigWriteError::Unchecked),
        }
    }

    /// Reload, and log why when it fails: the write that asked for it has happened either way.
    async fn reload_logged(&self) {
        match self.reload_all().await {
            Ok(_) => {}
            Err(_) if self.is_stopping() => {
                log::info(
                    "the reload after a configuration write was left out: the daemon is stopping",
                );
            }
            Err(message) => log::warn(&format!(
                "the reload after a configuration write failed: {message}"
            )),
        }
    }

    /// Run `edit` on the file the daemon reads, off the async threads, then have the daemon
    /// reload it. The reload is waited for no longer than the daemon's reload wait (a daemon not
    /// made shared, which only tests do, waits for it without bound); one that fails is logged,
    /// since the write itself has happened. Once the daemon's stop has begun nothing is written.
    pub(crate) async fn edit_then_reload(&self, edit: ConfigEdit) -> Result<(), ConfigWriteError> {
        let hold = Arc::clone(&self.writes).read_owned().await;
        // Judged with the hold in hand: the stop marks first and then waits for every hold, so
        // a write that sees no mark here is one the stop waits for, and none starts after it.
        if self.is_stopping() {
            return Err(ConfigWriteError::Stopping);
        }
        // The write, the snapshot and the reload are one task a caller that goes away cannot
        // split: a write that landed is always announced to the snapshot, so it never reads as
        // a hand edit, and always applied.
        match self.me.get().and_then(std::sync::Weak::upgrade) {
            Some(daemon) => {
                let wait = self.reload_wait;
                tokio::spawn(async move {
                    daemon.write_and_hold(edit, hold).await?;
                    let again = Arc::clone(&daemon);
                    let reloading = tokio::spawn(async move { again.reload_logged().await });
                    let _ = tokio::time::timeout(wait, reloading).await;
                    Ok(())
                })
                .await
                .unwrap_or(Err(ConfigWriteError::Stopped))
            }
            None => {
                self.write_and_hold(edit, hold).await?;
                self.reload_logged().await;
                Ok(())
            }
        }
    }

    /// Run `edit` on the file off the async threads, with `hold` kept until it is done, and make
    /// the file as written the snapshot, which follows the write even when the reload after it
    /// fails (the file is read again where the write was made).
    async fn write_and_hold(
        &self,
        edit: ConfigEdit,
        hold: tokio::sync::OwnedRwLockReadGuard<()>,
    ) -> Result<(), ConfigWriteError> {
        let path = self.config_path.clone();
        let reads = Arc::clone(&self.reads);
        let held = held_stamp(self);
        let written = tokio::task::spawn_blocking(move || {
            // Held until the thread is done, whether or not the caller still waits for it.
            let _hold = hold;
            // A file that was read once and is gone is not made again from the template by a
            // write: the reload after it would read that one change as the whole configuration.
            if held.is_some() && gone(&path) {
                return Err(ConfigWriteError::Off(missing_refusal(&path)));
            }
            // Judged again under the file's own lock, where a deletion between the check above
            // and the write is seen as well.
            let wrote = match held.is_some() {
                true => Config::while_file_must_exist(|| edit(&path)),
                false => edit(&path),
            };
            match wrote {
                Err(ConfigError::Gone(_)) => {
                    return Err(ConfigWriteError::Off(missing_refusal(&path)));
                }
                other => other.map_err(ConfigWriteError::Refused)?,
            }
            let generation = ticket(&reads);
            let stamp = file_stamp(&path);
            let read_back = Config::load_from(&path);
            // A file deleted since the write is not an empty configuration to hold.
            if held.is_some() && gone(&path) {
                return Err(ConfigWriteError::Off(missing_refusal(&path)));
            }
            Ok(read_back.map(|config| (generation, config, stamp)))
        })
        .await;
        match written {
            Ok(Ok(Ok((generation, config, stamp)))) => {
                self.hold_snapshot(config, generation, stamp)
            }
            Ok(Ok(Err(why))) => log::warn(&format!(
                "the configuration just written could not be read back: {why}"
            )),
            Ok(Err(why)) => return Err(why),
            Err(_) => return Err(ConfigWriteError::Stopped),
        }
        Ok(())
    }

    /// Save `preset` as the user's preset called `name`, and reload. A file that cannot be
    /// written is refused first; then the name is judged by the core ([`preset::new_name`])
    /// against the presets of the snapshot, and a name a preset of the user's already has is
    /// refused unless `force`, which writes the new curve over that preset. The writer judges
    /// the name again under the file's lock. Answers with the name as the file now spells it.
    ///
    /// `Ok` means the file is written, not that the daemon applied it: a reload that fails or
    /// outlasts the wait is only logged, and the snapshot already holds the write.
    pub(crate) async fn save_preset(
        &self,
        name: &str,
        mut preset: Preset,
        force: bool,
    ) -> Result<String, ConfigWriteError> {
        let held = self.config_for_write("presets").await?;
        let own = held
            .preset(name)
            .filter(|_| force)
            .map(|existing| existing.name.clone());
        let wanted = preset::new_name(
            name,
            held.presets.iter().map(|p| p.name.as_str()),
            own.as_deref(),
        )
        .map_err(ConfigWriteError::Name)?
        .to_string();
        preset.name.clone_from(&wanted);
        let edit: ConfigEdit = match own {
            Some(own) => Box::new(move |path| Config::update_preset(path, &own, &preset)),
            None => Box::new(move |path| Config::add_preset(path, &preset)),
        };
        self.edit_then_reload(edit).await?;
        Ok(wanted)
    }

    /// Rename the user's preset `from` to `to`, and reload. A built-in is refused, so is a preset
    /// the library does not have, and the new name is judged by the core ([`preset::new_name`])
    /// with the preset's own name set aside, so it may take its own name in another spelling.
    /// Answers with the name as the file now spells it. See [`Daemon::save_preset`] for what
    /// `Ok` means.
    pub(crate) async fn rename_preset(
        &self,
        from: &str,
        to: &str,
    ) -> Result<String, ConfigWriteError> {
        if preset::is_builtin(from) {
            return Err(ConfigWriteError::Builtin(format!(
                "{:?} is a built-in preset; it cannot be renamed",
                from.trim()
            )));
        }
        let held = self.config_for_write("presets").await?;
        let old = held
            .preset(from)
            .map(|found| found.name.clone())
            .ok_or_else(|| no_such_preset(from))?;
        let new = preset::new_name(to, held.presets.iter().map(|p| p.name.as_str()), Some(&old))
            .map_err(ConfigWriteError::Name)?
            .to_string();
        let to = new.clone();
        self.edit_then_reload(Box::new(move |path| Config::rename_preset(path, &old, &to)))
            .await?;
        Ok(new)
    }

    /// Delete the user's preset `name`, and reload. A built-in is refused, so is a preset the
    /// library does not have. Answers with the name as the file spelled it.
    pub(crate) async fn delete_preset(&self, name: &str) -> Result<String, ConfigWriteError> {
        if preset::is_builtin(name) {
            return Err(ConfigWriteError::Builtin(format!(
                "{:?} is a built-in preset; it cannot be deleted",
                name.trim()
            )));
        }
        let held = self.config_for_write("presets").await?;
        let gone = held
            .preset(name)
            .map(|found| found.name.clone())
            .ok_or_else(|| no_such_preset(name))?;
        let taken = gone.clone();
        self.edit_then_reload(Box::new(move |path| Config::remove_preset(path, &taken)))
            .await?;
        Ok(gone)
    }
}

/// What writing a curve read off a device over the preset called `name` is refused with when
/// that preset is imported: the one rule the socket's save and the web page's share.
pub(crate) fn imported_overwrite(presets: &[Preset], name: &str) -> Option<String> {
    preset::find(presets, name)
        .filter(|found| found.is_imported())
        .map(|found| preset::imported_update_refusal(&found.name))
}

/// The refusal for a preset name the library does not have.
pub(super) fn no_such_preset(name: &str) -> ConfigWriteError {
    ConfigWriteError::NoSuchPreset(format!("no preset named {:?}", name.trim()))
}

#[cfg(test)]
mod tests;
