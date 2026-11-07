// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding a program to run: whether a path is one, and where a name is on `PATH`.

#[cfg(test)]
pub(crate) use svolder_host::eq::paths::is_executable;
pub(crate) use svolder_host::eq::paths::{absolute, on_path};
