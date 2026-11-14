// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder mqtt …` against temporary XDG homes and no daemon: the verbs write the file, the
//! password comes from standard input and is never printed, and the environment's `SVOLDER_MQTT_*`
//! defaults are read as the file's fallbacks.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

mod common;

use common::TempPath;

/// What one run printed.
struct Ran {
    code: Option<i32>,
    out: String,
    err: String,
}

/// Run the CLI with `home` as its config and state home, `stdin` piped in and `envs` set.
fn svolder(args: &[&str], home: &Path, stdin: &str, envs: &[(&str, &str)]) -> Ran {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_svolder"));
    cmd.env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SYSTEMCTL", home.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", home.join("units"))
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_STATE_HOME", home)
        .env("XDG_RUNTIME_DIR", home)
        .env_remove("SVOLDER_SOCKET")
        .env_remove("SVOLDER_WEB_ENABLED")
        .env_remove("SVOLDER_MCP_ENABLED")
        .env_remove("SVOLDER_MQTT_ENABLED")
        .env_remove("SVOLDER_MQTT_HOST")
        .env_remove("SVOLDER_MQTT_PORT")
        .env_remove("SVOLDER_MQTT_USERNAME")
        .env_remove("SVOLDER_MQTT_PASSWORD")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(args);
    for (name, value) in envs {
        cmd.env(name, value);
    }
    let mut child = cmd.spawn().expect("the binary runs");
    child
        .stdin
        .take()
        .expect("a pipe")
        .write_all(stdin.as_bytes())
        .expect("stdin is written");
    let out = child.wait_with_output().expect("the binary ends");
    Ran {
        code: out.status.code(),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn home(tag: &str) -> TempPath {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("svolder-mqtt-cli-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    TempPath(dir)
}

fn config_text(home: &Path) -> String {
    std::fs::read_to_string(home.join("svolder").join("config.toml")).unwrap_or_default()
}

#[test]
fn the_verbs_write_the_mqtt_table_and_the_password_comes_from_stdin() {
    let h = home("verbs");
    for args in [
        &["mqtt", "set", "host", "ha.lan"][..],
        &["mqtt", "set", "port", "1884"],
        &["mqtt", "set", "username", "svolder"],
    ] {
        let ran = svolder(args, &h.0, "", &[]);
        assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
    }
    let ran = svolder(&["mqtt", "password"], &h.0, "fake-secret\n", &[]);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert!(!ran.out.contains("fake-secret") && !ran.err.contains("fake-secret"));
    let ran = svolder(&["mqtt", "on"], &h.0, "", &[]);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert!(ran.out.starts_with("mqtt: on in "), "{}", ran.out);
    let text = config_text(&h.0);
    assert!(text.contains("host = \"ha.lan\""), "{text}");
    assert!(text.contains("port = 1884"), "{text}");
    assert!(text.contains("username = \"svolder\""), "{text}");
    assert!(text.contains("password = \"fake-secret\""), "{text}");
    assert!(text.contains("enabled = true"), "{text}");
    let mode = std::fs::metadata(h.0.join("svolder").join("config.toml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "a file made with a password in it");

    let ran = svolder(&["config", "show"], &h.0, "", &[]);
    assert!(!ran.out.contains("fake-secret"), "{}", ran.out);

    let ran = svolder(&["mqtt", "password", "--clear"], &h.0, "", &[]);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert!(!config_text(&h.0).contains("fake-secret"));
    let ran = svolder(&["mqtt", "set", "username", ""], &h.0, "", &[]);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert!(!config_text(&h.0).contains("username = \"svolder\""));
    let ran = svolder(&["mqtt", "off"], &h.0, "", &[]);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert!(config_text(&h.0).contains("enabled = false"));
}

#[test]
fn an_empty_password_and_a_bad_value_are_refused_and_write_nothing() {
    let h = home("refused");
    let ran = svolder(&["mqtt", "password"], &h.0, "", &[]);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(ran.err.contains("no password"), "{}", ran.err);
    let ran = svolder(&["mqtt", "set", "host", " "], &h.0, "", &[]);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    let ran = svolder(&["mqtt", "set", "port", "70000"], &h.0, "", &[]);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(ran.err.contains("70000"), "{}", ran.err);
    let ran = svolder(&["mqtt", "set", "port", "0"], &h.0, "", &[]);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(ran.err.contains("mqtt.port"), "{}", ran.err);
    // On with no host is refused by the file's own rules.
    let ran = svolder(&["mqtt", "on"], &h.0, "", &[]);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(ran.err.contains("mqtt.host"), "{}", ran.err);
    assert_eq!(config_text(&h.0), "");
}

/// A shell's `SVOLDER_MQTT_PASSWORD` is said to outrank the file when a password is written.
#[test]
fn the_password_verb_says_when_the_environment_outranks_the_file() {
    let h = home("env");
    let ran = svolder(
        &["mqtt", "password"],
        &h.0,
        "from-stdin\n",
        &[("SVOLDER_MQTT_PASSWORD", "from-env")],
    );
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert!(ran.err.contains("SVOLDER_MQTT_PASSWORD"), "{}", ran.err);
    assert!(!ran.err.contains("from-env") && !ran.out.contains("from-env"));
}

/// The environment's defaults turn the bridge on with no file at all; `svolder config show`
/// reads them as the file's fallbacks.
#[test]
fn the_environment_gives_the_mqtt_keys_their_defaults() {
    let h = home("defaults");
    let envs = [
        ("SVOLDER_MQTT_ENABLED", "1"),
        ("SVOLDER_MQTT_HOST", "ha.lan"),
        ("SVOLDER_MQTT_PORT", "1884"),
        ("SVOLDER_MQTT_USERNAME", "svolder"),
    ];
    let ran = svolder(&["config", "show", "--json"], &h.0, "", &envs);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    let json: serde_json::Value = serde_json::from_str(&ran.out).unwrap();
    assert_eq!(json["mqtt"]["enabled"], true);
    assert_eq!(json["mqtt"]["host"], "ha.lan");
    assert_eq!(json["mqtt"]["port"], 1884);
    assert_eq!(json["mqtt"]["username"], "svolder");
    let ran = svolder(
        &["config", "show", "--json"],
        &h.0,
        "",
        &[("SVOLDER_MQTT_PORT", "nope")],
    );
    assert_ne!(ran.code, Some(0));
    assert!(ran.err.contains("SVOLDER_MQTT_PORT"), "{}", ran.err);
}
