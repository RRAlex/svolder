// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `scripts/spdx-check.sh` on small trees: the licence lines, a `#!` line that stays first, and
//! an executable file that starts with one.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::TempDir;

const LICENCE: &str = "# SPDX-FileCopyrightText: 2026 the svolder authors\n\
                       # SPDX-License-Identifier: GPL-3.0-or-later\n";

fn script() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/spdx-check.sh");
    assert!(path.exists(), "{} is missing", path.display());
    path
}

fn write(dir: &TempDir, name: &str, body: &str, mode: u32) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    path
}

fn check(files: &[&Path]) -> (bool, String) {
    let out: Output = Command::new("sh")
        .arg(script())
        .args(files)
        .output()
        .expect("run sh");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn a_file_with_the_header_after_a_shebang_passes() {
    let dir = TempDir::new("spdx-good");
    let f = write(
        &dir,
        "good file.sh",
        &format!("#!/bin/sh\n{LICENCE}"),
        0o755,
    );
    let (ok, text) = check(&[&f]);
    assert!(ok, "{text}");
    assert!(text.is_empty(), "{text}");
}

#[test]
fn a_shebang_below_line_one_fails() {
    let dir = TempDir::new("spdx-displaced");
    let f = write(
        &dir,
        "displaced.sh",
        &format!("{LICENCE}#!/bin/sh\n"),
        0o644,
    );
    let (ok, text) = check(&[&f]);
    assert!(!ok, "{text}");
    assert!(text.contains("#! line"), "{text}");
    assert!(text.contains("displaced.sh"), "{text}");
}

#[test]
fn an_executable_file_without_a_shebang_fails() {
    let dir = TempDir::new("spdx-exec");
    let f = write(&dir, "tool.py", LICENCE, 0o755);
    let (ok, text) = check(&[&f]);
    assert!(!ok, "{text}");
    assert!(text.contains("without a #! first line"), "{text}");
    assert!(text.contains("tool.py"), "{text}");
}

#[test]
fn a_file_without_the_licence_fails() {
    let dir = TempDir::new("spdx-missing");
    let f = write(&dir, "bare.sh", "#!/bin/sh\necho hi\n", 0o644);
    let (ok, text) = check(&[&f]);
    assert!(!ok, "{text}");
    assert!(text.contains("SPDX-License-Identifier"), "{text}");
    assert!(text.contains("bare.sh"), "{text}");
}
