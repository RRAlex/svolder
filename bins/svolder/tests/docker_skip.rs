// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `make docker` must never fail for lack of Docker: `scripts/docker.sh` looks for the
//! `docker` binary and its daemon itself and prints a `SKIP:` line and exits 0 when either is
//! missing, so a checkout with no Docker still passes `make check`.
//!
//! This builds a `PATH` holding nothing but symlinks to the host's own `sh`, `make`, `sed` and
//! `grep` — resolved off the real `PATH` before the child ever sees the narrower one — and runs
//! `make -C <workspace root> docker` inside it. With no `docker` reachable, the target's own
//! lookup finds nothing and must report the first SKIP line and exit 0.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;

use common::TempPath;

/// The absolute path `name` resolves to on this process's own `PATH`, or `None` if nothing on
/// it does — a `which`-style lookup with no dependency on a `which` binary being there to ask.
fn resolve(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[test]
fn docker_target_skips_cleanly_without_docker() {
    // `make` itself is what this test drives; without it there is nothing to run, so it prints
    // why and returns rather than asserting anything about a target that never ran.
    let Some(make) = resolve("make") else {
        println!(
            "docker_target_skips_cleanly_without_docker: SKIP — make is not on this host's PATH"
        );
        return;
    };
    let sh = resolve("sh").expect("sh is on this host's PATH");
    let sed = resolve("sed").expect("sed is on this host's PATH");
    let grep = resolve("grep").expect("grep is on this host's PATH");

    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("bins/svolder sits two levels under the workspace root")
        .to_path_buf();

    let narrow = TempPath(std::env::temp_dir().join(format!(
        "svolder-docker-skip-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
    )));
    let narrow_path = &narrow.0;
    fs::create_dir_all(narrow_path).expect("create the temp PATH directory");
    for (name, target) in [("sh", &sh), ("make", &make), ("sed", &sed), ("grep", &grep)] {
        symlink(target, narrow_path.join(name)).expect("symlink a tool into the temp PATH");
    }

    let output = Command::new(&make)
        .arg("-C")
        .arg(&workspace_root)
        .arg("docker")
        .env("PATH", narrow_path)
        .output()
        .expect("run make docker");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "make docker did not exit 0 with no docker on PATH ({}); stdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains("SKIP: docker not found"),
        "make docker did not print the SKIP line with no docker on PATH; stdout:\n{stdout}"
    );
}
