// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the computer EQ keeps its files, and the lookup of the programs it runs.

use std::path::{Path, PathBuf};

/// The files the computer EQ writes and the directories they go in, chosen by the program that
/// owns the configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqPaths {
    /// `host-eq.conf`, the filter chain's PipeWire config.
    pub conf: PathBuf,
    /// `host-eq.toml`, the state file.
    pub state: PathBuf,
    /// Where the systemd user unit goes.
    pub unit_dir: PathBuf,
    /// Where the autostart entry goes.
    pub autostart_dir: PathBuf,
    /// The process manager's pid file, `None` where the private runtime directory cannot be
    /// used.
    pub pid_file: Option<PathBuf>,
}

/// Whether `path` is a file somebody may run.
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Where `name` is found on `PATH`, if it is.
pub fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

/// `path` made absolute against the current directory; a path already absolute, or one that
/// cannot be joined because there is no current directory, comes back as it is.
pub fn absolute(path: &Path) -> PathBuf {
    match path.is_absolute() {
        true => path.to_path_buf(),
        false => std::env::current_dir().map_or_else(|_| path.to_path_buf(), |dir| dir.join(path)),
    }
}
