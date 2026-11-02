// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the integration tests share: a directory of their own and the fake tools they put in
//! it. Nothing here runs a real `wpctl`, `pw-record` or bus: every tool a case runs is a
//! script it wrote into its own directory.

#![allow(dead_code, reason = "each test binary uses the part of this it needs")]

mod script;

#[allow(
    unused_imports,
    reason = "each test binary uses the part of this it needs"
)]
pub use script::install_script;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A directory under the system temp directory that one case owns, removed when the case lets
/// go of it, whether it passed or an assertion failed first.
pub struct TempDir(PathBuf);

impl TempDir {
    /// A fresh directory whose name says which case made it.
    pub fn new(label: &str) -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is past 1970")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "svolder-host-{label}-{}-{seq}-{at}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("the case's directory is made");
        Self(dir)
    }

    /// A path inside the directory.
    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// The directory itself.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The lines of a file a fake tool appends to, or none when it has not written it yet.
pub fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default()
}
