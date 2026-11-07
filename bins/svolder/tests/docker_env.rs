// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `.env` part of `scripts/docker.sh` (`--env-only`, which needs no Docker): a key missing
//! from an existing file is appended, a line already there is never rewritten, and a last line
//! with no newline is not run into the first key appended after it.

use std::fs;
use std::path::Path;
use std::process::Command;

mod common;

use common::TempPath;

/// Run the script's env part in a fresh directory holding `env` as its `.env` (none when
/// `None`), with `runtime` as the caller's runtime directory; returns the file afterwards.
fn run_env_part(env: Option<&str>, runtime: &Path, extra: &[(&str, &str)]) -> String {
    let dir = TempPath(std::env::temp_dir().join(format!(
        "svolder-docker-env-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
    )));
    fs::create_dir_all(&dir.0).expect("create the temp directory");
    if let Some(text) = env {
        fs::write(dir.0.join(".env"), text).expect("write the .env");
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/docker.sh");
    let status = Command::new("sh")
        .arg(script)
        .arg("--env-only")
        .current_dir(&dir.0)
        .env("XDG_RUNTIME_DIR", runtime)
        .env_remove("DOCKER_HOST_AUDIO")
        .envs(extra.iter().copied())
        .output()
        .expect("run scripts/docker.sh --env-only");
    assert!(status.status.success(), "{status:?}");
    fs::read_to_string(dir.0.join(".env")).expect("read the .env")
}

#[test]
fn a_last_line_with_no_newline_keeps_its_value() {
    let out = run_env_part(Some("BLUETOOTH_GID=77"), Path::new("/nonexistent"), &[]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "BLUETOOTH_GID=77", "{out}");
    assert!(lines.contains(&"HOST_RUNTIME_DIR=/nonexistent"), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("HOST_UID=")), "{out}");
}

#[test]
fn existing_lines_are_never_rewritten_and_nothing_is_added_twice() {
    let before =
        "BLUETOOTH_GID=5\nHOST_UID=9\nHOST_RUNTIME_DIR=/x\nCOMPOSE_FILE=docker-compose.yml\n";
    let out = run_env_part(
        Some(before),
        Path::new("/nonexistent"),
        &[("DOCKER_HOST_AUDIO", "1")],
    );
    assert_eq!(out, before);
}

#[test]
fn the_override_is_enabled_only_with_a_socket_or_the_switch() {
    let none = run_env_part(None, Path::new("/nonexistent"), &[]);
    assert!(!none.contains("COMPOSE_FILE"), "{none}");
    let forced = run_env_part(
        None,
        Path::new("/nonexistent"),
        &[("DOCKER_HOST_AUDIO", "1")],
    );
    assert!(
        forced.contains("COMPOSE_FILE=docker-compose.yml:docker-compose.host-audio.yml\n"),
        "{forced}"
    );
}
