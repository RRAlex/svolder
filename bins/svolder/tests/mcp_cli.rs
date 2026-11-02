// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder mcp token` against temporary XDG homes. A token is compared with `assert!(a == b)`
//! and never formatted, so a failing test cannot print one.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

mod common;

use common::TempPath;

/// Run the CLI with `home` as its config and state home. `SVOLDER_MCP_TOKEN` is removed
/// unless `env_token` sets it.
fn svolder(args: &[&str], home: &Path, env_token: Option<&str>) -> (Option<i32>, String, String) {
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
        .env_remove("SVOLDER_MCP_TOKEN")
        .args(args);
    if let Some(token) = env_token {
        cmd.env("SVOLDER_MCP_TOKEN", token);
    }
    let out = cmd.output().expect("the binary runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn home(tag: &str) -> TempPath {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("svolder-mcp-cli-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    TempPath(dir)
}

fn write_config(home: &Path, text: &str) {
    let dir = home.join("svolder");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), text).unwrap();
}

#[test]
fn token_prints_43_characters_alone_and_the_same_twice() {
    let h = home("print");
    let (code, a, err) = svolder(&["mcp", "token"], &h.0, None);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(a.len(), 44, "a token and a newline");
    assert!(a.ends_with('\n'));
    let token = a.trim_end();
    assert_eq!(token.len(), 43);
    assert!(
        token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    );
    let (_, b, _) = svolder(&["mcp", "token"], &h.0, None);
    assert!(a == b);
    let file = h.0.join("svolder").join("mcp-token");
    assert_eq!(
        std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn rotate_prints_a_different_token_and_keeps_it() {
    let h = home("rotate");
    let (_, a, _) = svolder(&["mcp", "token"], &h.0, None);
    let (code, b, err) = svolder(&["mcp", "token", "--rotate"], &h.0, None);
    assert_eq!(code, Some(0), "{err}");
    assert!(
        err.contains("no daemon is running, so none uses the new token yet"),
        "{err}"
    );
    assert_eq!(b.trim_end().len(), 43);
    assert!(a != b);
    let (_, c, _) = svolder(&["mcp", "token"], &h.0, None);
    assert!(b == c);
}

#[test]
fn the_environment_token_is_printed_and_rotate_refuses_naming_it() {
    let h = home("env");
    let (code, out, _) = svolder(&["mcp", "token"], &h.0, Some("an-outside-token"));
    assert_eq!(code, Some(0));
    assert_eq!(out, "an-outside-token\n");
    assert!(!h.0.join("svolder").join("mcp-token").exists());
    let (code, out, err) = svolder(
        &["mcp", "token", "--rotate"],
        &h.0,
        Some("an-outside-token"),
    );
    assert_ne!(code, Some(0));
    assert_eq!(out, "");
    assert!(err.contains("SVOLDER_MCP_TOKEN"), "{err}");
    assert!(!err.contains("an-outside-token"));
}

#[test]
fn a_token_file_is_read_refused_when_open_and_never_rotated() {
    let h = home("file");
    let secret = h.0.join("secret");
    std::fs::write(&secret, "token-from-a-file\n").unwrap();
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
    write_config(
        &h.0,
        &format!("[mcp]\ntoken_file = \"{}\"\n", secret.display()),
    );
    let (code, out, err) = svolder(&["mcp", "token"], &h.0, None);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, "token-from-a-file\n");

    let (code, out, err) = svolder(&["mcp", "token", "--rotate"], &h.0, None);
    assert_ne!(code, Some(0));
    assert_eq!(out, "");
    assert!(err.contains(&secret.display().to_string()), "{err}");

    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).unwrap();
    let (code, out, err) = svolder(&["mcp", "token"], &h.0, None);
    assert_ne!(code, Some(0));
    assert_eq!(out, "", "nothing on stdout when it refuses");
    assert!(err.contains("0644"), "{err}");
    assert!(!err.contains("token-from-a-file"));
}

#[test]
fn config_show_prints_the_mcp_block_with_no_token_in_it() {
    let h = home("show");
    let secret = h.0.join("secret");
    std::fs::write(&secret, "token-from-a-file\n").unwrap();
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
    write_config(
        &h.0,
        &format!(
            "[mcp]\nenabled = true\nwrites = true\ntoken_file = \"{}\"\n",
            secret.display()
        ),
    );
    let (_, text, _) = svolder(&["config", "show"], &h.0, Some("an-outside-token"));
    assert!(
        text.contains(
            "[mcp]\nenabled = true\nlisten = 127.0.0.1:8737\nauth = token\nwrites = true"
        ),
        "{text}"
    );
    let (_, json, _) = svolder(
        &["config", "show", "--json"],
        &h.0,
        Some("an-outside-token"),
    );
    assert!(json.contains("\"mcp\":{"), "{json}");
    for out in [&text, &json] {
        assert!(!out.contains("token-from-a-file") && !out.contains("an-outside-token"));
    }
}

#[test]
fn a_non_loopback_listen_with_auth_none_is_a_config_error() {
    let h = home("badcfg");
    write_config(
        &h.0,
        "[mcp]\nauth = \"none\"\nlisten = \"192.168.1.10:8737\"\n",
    );
    let (code, out, err) = svolder(&["mcp", "token"], &h.0, None);
    assert_ne!(code, Some(0));
    assert_eq!(out, "");
    assert!(
        err.contains("mcp.auth = \"none\" needs a loopback listen address"),
        "{err}"
    );
}
