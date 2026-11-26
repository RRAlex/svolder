// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Watching the configuration file for an edit, so a hand-edited `adapter` reaches the running
//! daemon without a restart.
//!
//! The watch is the kernel's inotify on the file's directory, read through the runtime: nothing
//! wakes until the kernel reports a finished write or a rename onto the file's name, so a quiet
//! file costs nothing. The directory is watched rather than the file because editors write a new
//! file and rename it over the old one, which a watch on the old file's inode never sees. A bare
//! creation is not followed: the file is empty until its writer closes it, and that close is the
//! event that reads it. A file that is a symlink (a stow package) is watched through the
//! directory of what it points at too, as it pointed when the watch started.

use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::time::Duration;

use tokio::io::unix::AsyncFd;

/// How long a burst of events (a write, then a rename) is given to finish before the file is read:
/// one bounded pause per edit, not a poll.
pub(crate) const SETTLE: Duration = Duration::from_millis(300);

/// The size of the part of an inotify event before its name: descriptor, mask, cookie, length.
const HEADER: usize = 16;

/// A watch on one file's directory that reports when the file is written and closed or renamed
/// onto.
pub(crate) struct ConfigWatch {
    fd: AsyncFd<OwnedFd>,
    /// the names an event may carry: the file's, and its target's when it is a symlink
    names: Vec<OsString>,
}

impl ConfigWatch {
    /// Start watching `path`'s directory. Fails where the kernel refuses an inotify instance or
    /// the directory is not there; the caller says so and goes on without the watch.
    pub(crate) fn start(path: &Path) -> io::Result<ConfigWatch> {
        let (dir, name) = split(path)?;
        let mut watched = vec![(dir, name)];
        // A symlinked file is written through its target, whose directory the link's never sees.
        if let Ok(target) = std::fs::canonicalize(path)
            && let Ok(also) = split(&target)
            && !watched.contains(&also)
        {
            watched.push(also);
        }
        // SAFETY: plain system calls; the descriptor is owned from the moment it is returned.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` is a fresh descriptor nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let mask = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO;
        for (dir, _) in &watched {
            // SAFETY: `dir` is a NUL-terminated path that lives through the call.
            if unsafe { libc::inotify_add_watch(fd.as_raw_fd(), dir.as_ptr(), mask) } < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        // SAFETY: the descriptor is open, owned by this value and stays so until it drops.
        let fd = unsafe { AsyncFd::register(fd) }?;
        let names = watched.into_iter().map(|(_, name)| name).collect();
        Ok(ConfigWatch { fd, names })
    }

    /// Wait until the file is written and closed or renamed onto. Other files of the directory are
    /// read past without waking the caller.
    pub(crate) async fn edited(&self) -> io::Result<()> {
        let mut buffer = [0u8; 4096];
        loop {
            let mut ready = self.fd.readable().await?;
            let read = ready.try_io(|fd| {
                // SAFETY: the buffer is valid for its whole length.
                let n =
                    unsafe { libc::read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
                usize::try_from(n).map_err(|_| io::Error::last_os_error())
            });
            match read {
                Ok(Ok(n)) if names(&buffer[..n], &self.names) => return Ok(()),
                Ok(Ok(_)) => {}
                Ok(Err(e)) => return Err(e),
                // The descriptor was not ready after all: wait again.
                Err(_would_block) => {}
            }
        }
    }
}

/// A path's directory as a C string, and its file's name.
fn split(path: &Path) -> io::Result<(CString, OsString)> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
        .to_os_string();
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let dir = CString::new(dir.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in the path"))?;
    Ok((dir, name))
}

/// Whether any event in `events` (what one `read` of an inotify descriptor returns) names one of
/// `files`.
fn names(events: &[u8], files: &[OsString]) -> bool {
    let mut at = 0;
    while at + HEADER <= events.len() {
        let len = u32::from_ne_bytes(events[at + 12..at + 16].try_into().unwrap_or([0; 4]));
        let end = (at + HEADER).saturating_add(len as usize).min(events.len());
        // The name is padded with NULs to the event's length.
        let name: Vec<u8> = events[at + HEADER..end]
            .iter()
            .copied()
            .take_while(|b| *b != 0)
            .collect();
        if files.contains(&OsString::from_vec(name)) {
            return true;
        }
        at = end;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One event as the kernel lays it out: the header, then the name padded with NULs.
    fn event(name: &str, pad: usize) -> Vec<u8> {
        let mut out = vec![0u8; 12];
        out.extend(((name.len() + pad) as u32).to_ne_bytes());
        out.extend(name.as_bytes());
        out.extend(std::iter::repeat_n(0u8, pad));
        out
    }

    #[test]
    fn an_event_names_the_file_it_is_about() {
        let file = [OsString::from("config.toml")];
        let mut events = event("other.toml", 6);
        assert!(!names(&events, &file));
        events.extend(event("config.toml", 5));
        assert!(names(&events, &file));
        assert!(!names(&[], &file));
        // A cut-off event is read as far as it goes and never past the buffer.
        assert!(!names(&event("config.toml", 5)[..20], &file));
    }

    /// A write to the file wakes the watch; a write to its neighbour does not.
    #[tokio::test]
    async fn a_write_to_the_file_wakes_the_watch_and_a_neighbour_does_not() {
        let dir = std::env::temp_dir().join(format!("svolderd-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "schema = 1\n").unwrap();
        let watch = ConfigWatch::start(&path).expect("inotify");
        std::fs::write(dir.join("other.toml"), "x").unwrap();
        let quiet = tokio::time::timeout(Duration::from_millis(200), watch.edited()).await;
        assert!(quiet.is_err(), "a neighbour's write woke the watch");
        std::fs::write(&path, "schema = 1\nadapter = \"hci1\"\n").unwrap();
        tokio::time::timeout(Duration::from_secs(5), watch.edited())
            .await
            .expect("woken within the deadline")
            .expect("a read");
        // A new file renamed over the old one (what an editor does) wakes it too.
        std::fs::write(dir.join("config.new"), "schema = 1\n").unwrap();
        std::fs::rename(dir.join("config.new"), &path).unwrap();
        tokio::time::timeout(Duration::from_secs(5), watch.edited())
            .await
            .expect("woken by the rename within the deadline")
            .expect("a read");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
