// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers several of the crate's test modules share: a temporary directory a case owns, the
//! stub daemon a backend or a command talks to, the count of daemons a case left running and the
//! one way a test writes a script it runs.
//! A helper that several modules' tests use lives in the narrowest `test_support` module that
//! covers them all; this is the one for the whole crate.

pub(crate) mod stub_daemon;

#[path = "../../../crates/svolder-host/tests/common/script.rs"]
mod script;

pub(crate) use script::install_script;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A temporary runtime directory, removed when the test that made it ends.
pub(crate) struct TempDir(pub(crate) PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// A directory of this test's own, standing in for the whole of one user's runtime, state
/// and configuration.
pub(crate) fn temp_dir(what: &str) -> TempDir {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let at = NEXT.fetch_add(1, Ordering::Relaxed);
    // The time stamp tells apart two test binaries that share a pid in two PID namespaces, so
    // neither removes, or reuses, the other's directory.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let dir = std::env::temp_dir().join(format!(
        "svolder-{what}-{}-{at}-{secs:x}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("the temporary directory is made");
    TempDir(dir)
}

/// Every running `svolderd` that was started on `socket`, by pid.
///
/// The environment is what says which daemon is whose: the whole suite runs at once and
/// other cases have daemons of their own, so a daemon counts here only when it was handed
/// this case's socket. A process that has stopped has no environment left to read, so a
/// daemon that raced for the socket and lost is not counted as one that is running.
pub(crate) fn daemons_on(socket: &Path) -> Vec<u32> {
    let needle = format!("SVOLDER_SOCKET={}", socket.display());
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let environ = std::fs::read(entry.path().join("environ")).ok()?;
            environ
                .split(|byte| *byte == 0)
                .any(|value| value == needle.as_bytes())
                .then_some(pid)
        })
        .collect()
}
