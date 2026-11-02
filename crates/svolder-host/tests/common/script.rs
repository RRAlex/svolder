// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one place a test writes a script it then runs. Every test crate that stands a program in
//! for a real one includes this file, so the rule below holds for all of them.

use std::path::Path;
use std::process::Command;

/// Write an executable shell script at `path`, replacing what is there.
///
/// A `sh` of its own writes it, so this process never holds the file open for writing: a
/// thread of another case forking at that moment would otherwise carry the open descriptor
/// into its child, and running the script would fail with "text file busy" (ETXTBSY).
pub fn install_script(path: &Path, body: &str) {
    let status = Command::new("sh")
        .args([
            "-c",
            "printf '%s' \"$1\" > \"$2\" && chmod 755 \"$2\"",
            "sh",
        ])
        .arg(body)
        .arg(path)
        .status()
        .expect("sh runs");
    assert!(
        status.success(),
        "the script at {} is written",
        path.display()
    );
}
