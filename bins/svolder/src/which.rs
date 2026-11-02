// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding a program to run: whether a path is one, and where a name is on `PATH`.

use std::path::{Path, PathBuf};

/// Whether `path` is a file somebody may run.
pub(crate) fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Where `name` is found on `PATH`, if it is.
pub(crate) fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

/// `path` made absolute against the current directory; a path already absolute, or one that
/// cannot be joined because there is no current directory, comes back as it is.
pub(crate) fn absolute(path: &Path) -> PathBuf {
    match path.is_absolute() {
        true => path.to_path_buf(),
        false => std::env::current_dir().map_or_else(|_| path.to_path_buf(), |dir| dir.join(path)),
    }
}
