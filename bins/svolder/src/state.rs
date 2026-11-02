// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What svolder remembers about the devices it has connected to, between runs.
//!
//! This is state rather than configuration: nobody writes it by hand and nothing breaks when
//! it is thrown away, so it lives under `$XDG_STATE_HOME` beside the other things a program
//! learns about its own machine, and every read of it answers with an empty state rather than
//! an error when the file is missing or unreadable.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use svolder_ipc::config::state_home;

/// Where a save writes before it renames that over `path`: `state.toml.<pid>.tmp` beside it,
/// named after the process so that two svolders saving at once never write the same temporary
/// file over each other.
fn temp_file(path: &Path) -> PathBuf {
    path.with_extension(format!("toml.{}.tmp", std::process::id()))
}

/// Take the exclusive lock that serialises saves of the state file `path`, waiting for
/// whoever holds it.
///
/// The lock is an `flock` on `<path>.lock`, created at mode 0600 and never removed, the same
/// as the daemon's socket lock: removing it would let a second saver lock a new file while the
/// first still holds the old one. The returned file must be kept alive, since closing it
/// releases the lock. A lock that cannot be opened or taken is not worth refusing a save
/// over, so the save then goes ahead without one.
fn lock_beside(path: &Path) -> Option<std::fs::File> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(PathBuf::from(name))
        .ok()?;
    loop {
        // SAFETY: the descriptor belongs to `file`, which outlives the call; `flock` reads no
        // memory of ours.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Some(file);
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return None;
        }
    }
}

/// Write `text` to `path`, a file that is created at mode 0600 when it is not there.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?
        .write_all(text.as_bytes())
}

/// What svolder has learned about each device it has connected to.
///
/// The file is one table per device, named by unit id:
///
/// ```toml
/// [devices.0102030405060708]
/// last_adapter = "hci1"
/// last_profile = "gungnir2-v2"
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceState {
    /// One entry per device, keyed by unit id.
    #[serde(default)]
    devices: BTreeMap<String, Remembered>,
    /// The devices this process has `remember`ed something about, which are the entries a
    /// save puts over what the file holds by then; the rest of what it carries is a copy that
    /// another process may have moved on from.
    #[serde(skip)]
    touched: BTreeSet<String>,
}

/// What is remembered about one device.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Remembered {
    /// The adapter that last connected it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_adapter: Option<String>,
    /// The name of the profile the device was last read through, which is what says how many
    /// bands its tone control has without connecting to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_profile: Option<String>,
}

impl DeviceState {
    /// What the state file holds, or an empty state when there is nothing to read there.
    pub fn load() -> DeviceState {
        DeviceState::load_from(&state_home())
    }

    /// What the state file under the state home `base` holds, or an empty state when that
    /// file is absent, unreadable, or no longer parses.
    ///
    /// A state file this build cannot read is an empty one: what it holds is worth a faster
    /// connect and nothing more, so it is never worth refusing a command over.
    pub fn load_from(base: &Path) -> DeviceState {
        let Ok(text) = std::fs::read_to_string(DeviceState::file_in(base)) else {
            return DeviceState::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Note that `adapter` connected the device `udid` names, read through the profile named
    /// `profile` (none when the registry has no profile of that name), in place of whatever
    /// connected it before, and say whether that changed anything.
    ///
    /// Both are overwritten every time, so a profile that changed with the firmware is
    /// corrected on the next connect, and a `None` clears a name left by an earlier run. The answer is what saves the steady state a write: the
    /// same adapter and profile again leave this state exactly as it was, and a caller that is
    /// only here to record a connect has nothing to put on disk.
    pub fn remember(&mut self, udid: &str, adapter: &str, profile: Option<&str>) -> bool {
        let entry = self.devices.entry(key(udid)).or_default();
        // An entry is the process's own the moment it is asked about, changed or not.
        self.touched.insert(key(udid));
        let same = entry.last_adapter.as_deref() == Some(adapter)
            && entry.last_profile.as_deref() == profile;
        if same {
            return false;
        }
        entry.last_adapter = Some(adapter.to_string());
        entry.last_profile = profile.map(str::to_string);
        true
    }

    /// The adapter that last connected the device `udid` names.
    pub fn last_adapter(&self, udid: &str) -> Option<String> {
        self.devices.get(&key(udid))?.last_adapter.clone()
    }

    /// The name of the profile the device `udid` names was last read through.
    pub fn last_profile(&self, udid: &str) -> Option<String> {
        self.devices.get(&key(udid))?.last_profile.clone()
    }

    /// Write this state to the state file.
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&state_home())
    }

    /// Write this state to the state file under the state home `base`, through a temporary
    /// file beside it.
    ///
    /// The save holds an exclusive lock on `state.toml.lock` while it reads the file as it is
    /// then, puts the entries this state `remember`ed over it, and renames the result into
    /// place, so two processes saving at once each find the other's entry and keep it. The
    /// lock file is never removed. The text goes to a temporary file in the same directory
    /// and is renamed over the target, so a reader sees either the file that was there or the
    /// whole of the new one: a file that is already there keeps the mode it had, and one that
    /// is not is created at mode 0600.
    fn save_to(&self, base: &Path) -> Result<(), String> {
        let path = DeviceState::file_in(base);
        let parent = path.parent().ok_or("the state file has no directory")?;
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        let _lock = lock_beside(&path);
        let mut merged = DeviceState::load_from(base);
        #[cfg(test)]
        tests::after_read(&path);
        for key in &self.touched {
            if let Some(entry) = self.devices.get(key) {
                merged.devices.insert(key.clone(), entry.clone());
            }
        }
        let text = toml::to_string(&merged).map_err(|e| e.to_string())?;
        let tmp = temp_file(&path);
        write_private(&tmp, &text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        match std::fs::metadata(&path) {
            Ok(meta) => {
                if let Err(e) = std::fs::set_permissions(&tmp, meta.permissions()) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(format!("{}: {e}", tmp.display()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("{}: {e}", path.display()));
            }
        }
        std::fs::rename(&tmp, &path).map_err(|e| {
            // The file is still the one that was there; the half-written one beside it is not
            // left behind to be read as a state of its own.
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", path.display())
        })
    }

    /// The state file svolder keeps under the state home `base`, which with the state home
    /// itself is `$XDG_STATE_HOME/svolder/state.toml` or `~/.local/state/svolder/state.toml`.
    fn file_in(base: &Path) -> PathBuf {
        base.join("svolder").join("state.toml")
    }
}

/// The file a daemon this process started writes its output to,
/// `$XDG_STATE_HOME/svolder/svolderd.log` or `~/.local/state/svolder/svolderd.log`.
///
/// It sits beside the state file for the same reason: a daemon nobody asked for by hand keeps
/// no journal of its own, so its lines go where the rest of what this machine learned lives,
/// and nothing breaks when the file is thrown away.
pub fn log_path() -> PathBuf {
    log_path_in(&state_home())
}

/// [`log_path`] under the state home `base`, for a caller that says where that is.
pub fn log_path_in(base: &Path) -> PathBuf {
    base.join("svolder").join("svolderd.log")
}

/// How a unit id is spelled as a key, so that a configuration file writing it in capitals and
/// an advertisement reporting it in lower case name the same device.
fn key(udid: &str) -> String {
    udid.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir as temp_state_dir;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    /// Where saves of one state file meet: how many have read the file, and a bell for the
    /// others.
    type Meeting = Arc<(Mutex<usize>, Condvar)>;

    /// The state files whose saves meet after reading, so a test can make two saves overlap.
    static MEETINGS: Mutex<Vec<(PathBuf, Meeting)>> = Mutex::new(Vec::new());

    /// Called by a save once it has read the state file under its lock. A save of a file with
    /// a meeting waits there for another save to arrive, or a fifth of a second, whichever is
    /// first: a lock that works lets only one save in, so the wait ends on the clock, and no
    /// lock lets both in and the wait ends on the second.
    pub(super) fn after_read(path: &Path) {
        let meeting = MEETINGS
            .lock()
            .unwrap()
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, m)| Arc::clone(m));
        let Some(meeting) = meeting else { return };
        let (count, bell) = &*meeting;
        let mut arrived = count.lock().unwrap();
        *arrived += 1;
        bell.notify_all();
        let _ = bell
            .wait_timeout_while(arrived, Duration::from_millis(200), |n| *n < 2)
            .unwrap();
    }

    /// Two saves from two threads, each for its own device, both started from a copy of the
    /// file that has neither entry, and with `overlap` set, made to reach the rename together.
    fn race(overlap: bool) -> DeviceState {
        let dir = temp_state_dir("state");
        let base = dir.0.clone();
        let path = DeviceState::file_in(&base);
        if overlap {
            MEETINGS
                .lock()
                .unwrap()
                .push((path, Arc::new((Mutex::new(0), Condvar::new()))));
        }
        // Both read before either saves, as two svolders started at once do.
        let loaded = Arc::new(std::sync::Barrier::new(2));
        let threads: Vec<_> = [("0102030405060708", "hci1"), ("0807060504030201", "hci2")]
            .into_iter()
            .map(|(unit, adapter)| {
                let (base, loaded) = (base.clone(), Arc::clone(&loaded));
                std::thread::spawn(move || {
                    let mut st = DeviceState::load_from(&base);
                    loaded.wait();
                    st.remember(unit, adapter, Some("gungnir2-v2"));
                    st.save_to(&base).expect("the state file is written");
                })
            })
            .collect();
        for t in threads {
            t.join().expect("a saving thread");
        }
        DeviceState::load_from(&base)
    }

    /// A save puts its own entries over the file as it is when the save runs, not as it was
    /// when this process read it, so two svolders racing both leave their entry.
    #[test]
    fn two_racing_saves_both_leave_their_entry() {
        let back = race(false);
        assert_eq!(back.last_adapter("0102030405060708"), Some("hci1".into()));
        assert_eq!(back.last_adapter("0807060504030201"), Some("hci2".into()));
    }

    /// Two saves that both reach the read-and-rename at once are let through one at a time.
    #[test]
    fn saves_that_overlap_are_let_through_one_at_a_time() {
        let back = race(true);
        assert_eq!(back.last_adapter("0102030405060708"), Some("hci1".into()));
        assert_eq!(back.last_adapter("0807060504030201"), Some("hci2".into()));
    }

    /// A state file a save creates is private to its user, as the configuration file is, and
    /// one that is already there keeps the mode it had.
    #[test]
    fn a_new_state_file_is_private_and_an_old_one_keeps_its_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_state_dir("state");
        let path = DeviceState::file_in(&dir.0);
        let mut st = DeviceState::default();
        st.remember("0102030405060708", "hci1", None);
        st.save_to(&dir.0).expect("the state file is written");
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).expect("chmod");
        st.remember("0102030405060708", "hci2", None);
        st.save_to(&dir.0).expect("the state file is written");
        assert_eq!(mode(&path), 0o640);
    }

    /// The lock file stays beside the state file after a save, and holds nothing.
    #[test]
    fn the_lock_file_stays_after_a_save() {
        let dir = temp_state_dir("state");
        let mut st = DeviceState::default();
        st.remember("0102030405060708", "hci1", None);
        st.save_to(&dir.0).expect("the state file is written");
        let lock = dir.0.join("svolder").join("state.toml.lock");
        assert_eq!(std::fs::metadata(&lock).expect("the lock file").len(), 0);
    }

    /// The adapter that connected a device is there again on the next run.
    #[test]
    fn the_last_adapter_that_connected_a_device_is_remembered() {
        let dir = temp_state_dir("state");
        let mut st = DeviceState::load_from(&dir.0);
        st.remember("0102030405060708", "hci1", Some("gungnir2-v2"));
        st.save_to(&dir.0).expect("the state file is written");
        assert_eq!(
            DeviceState::load_from(&dir.0).last_adapter("0102030405060708"),
            Some("hci1".into())
        );
    }

    /// The file says which device it is about, under the state home svolder keeps.
    #[test]
    fn the_state_file_names_the_device_it_is_about() {
        let dir = temp_state_dir("state");
        let mut st = DeviceState::default();
        st.remember("0102030405060708", "hci1", Some("gungnir2-v2"));
        st.save_to(&dir.0).expect("the state file is written");
        let path = DeviceState::file_in(&dir.0);
        assert_eq!(path, dir.0.join("svolder").join("state.toml"));
        let text = std::fs::read_to_string(&path).expect("the state file is there");
        assert_eq!(
            text,
            "[devices.0102030405060708]\nlast_adapter = \"hci1\"\nlast_profile = \"gungnir2-v2\"\n"
        );
        assert!(
            temp_file(&path)
                .to_string_lossy()
                .ends_with(&format!("state.toml.{}.tmp", std::process::id())),
            "the temporary file a save writes is this process's own"
        );
        let mut left: Vec<PathBuf> = std::fs::read_dir(path.parent().expect("a directory"))
            .expect("the state directory is there")
            .map(|e| e.expect("an entry").path())
            .collect();
        left.sort();
        let lock = dir.0.join("svolder").join("state.toml.lock");
        assert_eq!(
            left,
            vec![path, lock],
            "only the state file and its lock file are left"
        );
    }

    /// A file written by hand, with the unit id quoted the way a person would write it, says
    /// the same thing as one svolder wrote.
    #[test]
    fn a_quoted_unit_id_names_the_same_device() {
        let dir = temp_state_dir("state");
        let path = DeviceState::file_in(&dir.0);
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("the directory");
        std::fs::write(
            &path,
            "[devices.\"0102030405060708\"]\nlast_adapter = \"hci1\"\n",
        )
        .expect("the state file is written");
        assert_eq!(
            DeviceState::load_from(&dir.0).last_adapter("0102030405060708"),
            Some("hci1".into())
        );
    }

    /// A state file that is not TOML at all remembers nothing, rather than stopping the run
    /// that read it.
    #[test]
    fn a_state_file_that_does_not_parse_remembers_nothing() {
        let dir = temp_state_dir("state");
        let path = DeviceState::file_in(&dir.0);
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("the directory");
        std::fs::write(&path, "this is not toml {{{").expect("the state file is written");
        assert_eq!(DeviceState::load_from(&dir.0), DeviceState::default());
    }

    /// The same adapter connecting the same device again changes nothing, so there is nothing
    /// to write; a different adapter is worth the write.
    #[test]
    fn remembering_the_same_adapter_again_is_not_worth_a_write() {
        let dir = temp_state_dir("state");
        let mut st = DeviceState::load_from(&dir.0);
        assert!(
            st.remember("0102030405060708", "hci1", Some("gungnir2-v2")),
            "the first adapter is news"
        );
        st.save_to(&dir.0).expect("the state file is written");

        let mut again = DeviceState::load_from(&dir.0);
        assert!(
            !again.remember("0102030405060708", "hci1", Some("gungnir2-v2")),
            "the same adapter again is not"
        );
        assert!(
            again.remember("0102030405060708", "hci2", Some("gungnir2-v2")),
            "a different adapter is"
        );
        // Only the first save ran, so the file still says what it said.
        let text = std::fs::read_to_string(DeviceState::file_in(&dir.0)).expect("the state file");
        assert_eq!(
            text,
            "[devices.0102030405060708]\nlast_adapter = \"hci1\"\nlast_profile = \"gungnir2-v2\"\n"
        );
    }

    /// A state home with no file in it remembers nothing, and says so rather than failing.
    #[test]
    fn a_state_home_with_no_file_remembers_nothing() {
        let dir = temp_state_dir("state");
        let st = DeviceState::load_from(&dir.0);
        assert_eq!(st, DeviceState::default());
        assert_eq!(st.last_adapter("0102030405060708"), None);
    }

    /// A daemon this process starts logs beside the state file, in the same directory.
    #[test]
    fn the_daemon_log_sits_beside_the_state_file() {
        let dir = temp_state_dir("state");
        assert_eq!(
            log_path_in(&dir.0),
            DeviceState::file_in(&dir.0)
                .parent()
                .expect("a directory")
                .join("svolderd.log")
        );
    }

    /// A second device joins the first rather than replacing it, and a device that moves
    /// adapters keeps only the adapter it moved to.
    #[test]
    fn the_state_keeps_every_device_and_the_latest_adapter_of_each() {
        let dir = temp_state_dir("state");
        let mut st = DeviceState::load_from(&dir.0);
        st.remember("0102030405060708", "hci0", Some("gungnir2-v2"));
        st.remember("0807060504030201", "hci2", Some("gungnir2-v2"));
        st.remember("0102030405060708", "hci1", Some("gungnir2-v2"));
        st.save_to(&dir.0).expect("the state file is written");

        let back = DeviceState::load_from(&dir.0);
        assert_eq!(back.last_adapter("0102030405060708"), Some("hci1".into()));
        assert_eq!(back.last_adapter("0807060504030201"), Some("hci2".into()));
    }

    /// A file written before profiles were remembered still loads, with no profile to give.
    #[test]
    fn a_state_file_without_a_last_profile_still_loads() {
        let dir = temp_state_dir("state");
        let path = DeviceState::file_in(&dir.0);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(
            &path,
            "[devices.0102030405060708]\nlast_adapter = \"hci1\"\n",
        )
        .expect("the old file");
        let st = DeviceState::load_from(&dir.0);
        assert_eq!(st.last_adapter("0102030405060708"), Some("hci1".into()));
        assert_eq!(st.last_profile("0102030405060708"), None);
    }

    /// The profile is overwritten when its name changes, even with the adapter unchanged.
    #[test]
    fn the_last_profile_is_refreshed_when_it_changes() {
        let mut st = DeviceState::default();
        assert!(st.remember("0102030405060708", "hci1", Some("gungnir2-v1")));
        assert!(!st.remember("0102030405060708", "hci1", Some("gungnir2-v1")));
        assert!(st.remember("0102030405060708", "hci1", Some("gungnir2-v2")));
        assert_eq!(
            st.last_profile("0102030405060708"),
            Some("gungnir2-v2".into())
        );
    }

    /// A connect through a profile the registry does not know clears the name an earlier run left.
    #[test]
    fn no_profile_after_a_profile_clears_it() {
        let mut st = DeviceState::default();
        assert!(st.remember("0102030405060708", "hci1", Some("gungnir2-v2")));
        assert!(st.remember("0102030405060708", "hci1", None));
        assert_eq!(st.last_profile("0102030405060708"), None);
        assert!(!st.remember("0102030405060708", "hci1", None));
    }
}
