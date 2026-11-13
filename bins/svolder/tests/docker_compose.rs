// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What `docker-compose.yml` promises a first run, and that the file a release attaches
//! (`packaging/release-compose.sh`) keeps it: a cap on the container's log, which Docker's
//! default `json-file` driver does not have, and the switches that turn the web page and the MCP
//! server on, there to uncomment.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;

use common::TempPath;

/// The workspace root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The repository's compose file.
fn compose() -> String {
    fs::read_to_string(root().join("docker-compose.yml")).expect("read docker-compose.yml")
}

/// The compose file `release-compose.sh` writes for version 9.9.9.
fn released() -> String {
    let dir = TempPath(std::env::temp_dir().join(format!(
        "svolder-docker-compose-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
    )));
    fs::create_dir_all(&dir.0).expect("create the temp directory");
    let out = dir.0.join("docker-compose.yml");
    let run = Command::new("sh")
        .arg(root().join("packaging/release-compose.sh"))
        .arg("9.9.9")
        .arg(&out)
        .current_dir(&dir.0)
        .output()
        .expect("run packaging/release-compose.sh");
    assert!(run.status.success(), "{run:?}");
    fs::read_to_string(out).expect("read the written compose file")
}

/// The lines of the service's `logging:` block, without their indentation: from the key to the
/// next line indented no deeper than it.
fn logging_block(text: &str) -> Vec<String> {
    let mut lines = text.lines().skip_while(|line| *line != "    logging:");
    let Some(head) = lines.next() else {
        return Vec::new();
    };
    std::iter::once(head)
        .chain(lines.take_while(|line| line.starts_with("      ")))
        .filter(|line| !line.trim_start().starts_with('#'))
        .map(|line| line.trim().to_string())
        .collect()
}

/// What the block must be: the driver named, a size per file and a number of files.
const LOGGING: [&str; 5] = [
    "logging:",
    "driver: json-file",
    "options:",
    "max-size: \"10m\"",
    "max-file: \"3\"",
];

#[test]
fn the_compose_file_caps_the_containers_log() {
    assert_eq!(logging_block(&compose()), LOGGING);
}

#[test]
fn the_released_compose_file_keeps_the_cap_and_names_the_published_image() {
    let text = released();
    assert_eq!(logging_block(&text), LOGGING, "{text}");
    assert!(
        text.contains("\n    image: ghcr.io/rralex/svolder:9.9.9\n"),
        "{text}"
    );
    assert!(!text.contains("svolder:local"), "{text}");
}

/// The lines of the service's `environment:` block, comments included, without indentation.
fn environment_block(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|line| *line != "    environment:")
        .skip(1)
        .take_while(|line| line.starts_with("      "))
        .map(|line| line.trim().to_string())
        .collect()
}

/// Each switch is in both files, commented out, with one line of words above it.
#[test]
fn the_compose_files_carry_the_two_switches_commented_out() {
    for text in [compose(), released()] {
        let lines = environment_block(&text);
        for switch in ["SVOLDER_WEB_ENABLED", "SVOLDER_MCP_ENABLED"] {
            let at = lines
                .iter()
                .position(|line| *line == format!("# {switch}: \"1\""))
                .unwrap_or_else(|| panic!("{switch} is not in {lines:?}"));
            let words = &lines[at - 1];
            assert!(
                words.starts_with("# Turns the ") && words.contains("config.toml wins"),
                "{words}"
            );
        }
    }
}

/// `svolder config show --json` with `env` set and `config` as the configuration file (none
/// when `None`), under a home of its own: whether `[web]` and `[mcp]` are enabled.
fn enabled_under(env: &[(&str, &str)], config: Option<&str>) -> (bool, bool) {
    let home = common::TempDir::new("docker-compose-switch");
    if let Some(text) = config {
        fs::create_dir_all(home.join("config/svolder")).expect("create the config directory");
        fs::write(home.join("config/svolder/config.toml"), text).expect("write the config");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .args(["config", "show", "--json"])
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env("SVOLDER_HOST", "off")
        .envs(env.iter().copied())
        .output()
        .expect("run svolder config show");
    assert!(out.status.success(), "{out:?}");
    let shown: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON on stdout");
    let enabled = |table: &str| shown[table]["enabled"].as_bool().expect("an enabled key");
    (enabled("web"), enabled("mcp"))
}

/// What the compose file's switches do to the program the image runs: each turns its server on
/// with no configuration file, and an `enabled` in the file wins over it.
#[test]
fn the_switches_turn_the_servers_on_and_the_file_wins() {
    assert_eq!(enabled_under(&[], None), (false, false));
    assert_eq!(
        enabled_under(&[("SVOLDER_WEB_ENABLED", "1")], None),
        (true, false)
    );
    assert_eq!(
        enabled_under(&[("SVOLDER_MCP_ENABLED", "1")], None),
        (false, true)
    );
    let both = [("SVOLDER_WEB_ENABLED", "1"), ("SVOLDER_MCP_ENABLED", "1")];
    assert_eq!(enabled_under(&both, None), (true, true));
    let off = "schema = 1\n[web]\nenabled = false\n[mcp]\nenabled = false\n";
    assert_eq!(enabled_under(&both, Some(off)), (false, false));
}
