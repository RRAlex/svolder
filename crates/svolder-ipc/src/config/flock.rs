// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An exclusive lock beside a file, for the writers that read it, change it and put it back:
//! two of them, in two processes or in one, take turns instead of one losing the other's change.

use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The words a lock still held when the wait runs out fails with.
pub(super) const BUSY: &str = "another svolder is writing this file";

/// A lock, held for as long as this lives: dropping it closes the file and lets the lock go.
#[derive(Debug)]
pub(super) struct Held {
    _file: File,
}

/// How long [`lock_beside`] waits for whoever holds the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wait {
    /// Not at all. Only the tests ask for this.
    #[cfg_attr(not(test), allow(dead_code, reason = "only the tests ask for no wait"))]
    Not,
    /// Up to this long.
    Until(Duration),
}

/// Take an exclusive `flock` on `<path>.lock`, waiting as `wait` says.
///
/// The lock file is created at mode 0600, opened with `O_NOFOLLOW` (a symbolic link there is
/// refused, not followed) and `O_CLOEXEC`, and never removed: removing it would let a second
/// writer lock a new file while the first still holds the old one. It sits beside `path` as
/// given, not canonicalised, so every writer of one path meets on one lock; `flock` belongs to
/// an open file, so two writers in one process take turns too. A lock file that cannot be opened
/// or locked is the error; a lock still held when the wait runs out is a `WouldBlock` error,
/// with [`BUSY`] as its words after [`Wait::Until`].
pub(super) fn lock_beside(path: &Path, wait: Wait) -> std::io::Result<Held> {
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(PathBuf::from(name))?;
    let deadline = match wait {
        Wait::Not => None,
        Wait::Until(longest) => Some(Instant::now() + longest),
    };
    let mut pause = Duration::from_millis(5);
    loop {
        // SAFETY: the descriptor belongs to `file`, which outlives the call; `flock` reads no
        // memory of ours.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Held { _file: file });
        }
        let error = Error::last_os_error();
        match error.kind() {
            ErrorKind::Interrupted => continue,
            ErrorKind::WouldBlock => {}
            _ => return Err(error),
        }
        let Some(deadline) = deadline else {
            return Err(error);
        };
        let now = Instant::now();
        if now >= deadline {
            return Err(Error::new(ErrorKind::WouldBlock, BUSY));
        }
        std::thread::sleep(pause.min(deadline - now));
        pause = (pause * 2).min(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests;
