// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder tui` outside a terminal: the only `tui` invocation an integration test can make,
//! since a piped stdout is never a TTY.

use std::process::Command;

mod common;

use common::TempPath;

#[test]
fn tui_without_a_terminal_exits_one() {
    let home =
        TempPath(std::env::temp_dir().join(format!("svolder-tui-cli-test-{}", std::process::id())));
    let scratch = &home.0;
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", scratch.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", scratch.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", scratch.join("units"))
        .env("XDG_CONFIG_HOME", scratch)
        .arg("tui")
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("the TUI needs a terminal"), "{err}");
    // Every user-facing failure carries the step that follows it, this one included.
    assert!(err.contains("svolder --help"), "{err}");
}

/// `svolder tui --join <dead socket>` outside a terminal leaves with the terminal's failure and
/// makes neither the socket nor its `.lock`.
#[test]
fn joining_a_dead_socket_outside_a_terminal_creates_no_socket_or_lock() {
    let home = TempPath(
        std::env::temp_dir().join(format!("svolder-tui-join-test-{}", std::process::id())),
    );
    let scratch = &home.0;
    std::fs::create_dir_all(scratch).expect("the scratch directory is made");
    let dead = scratch.join("none.sock");
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_SOCKET", scratch.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", scratch.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", scratch.join("units"))
        .env("XDG_CONFIG_HOME", scratch)
        .env("XDG_STATE_HOME", scratch)
        .env("XDG_DATA_HOME", scratch)
        .env("XDG_RUNTIME_DIR", scratch)
        .args(["tui", "--join"])
        .arg(&dead)
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(1));
    assert!(!dead.exists(), "the socket was made");
    assert!(
        !scratch.join("none.sock.lock").exists(),
        "the lock was made"
    );
}

/// `--host` and `--web` belong to the demo: without `--demo` they are refused before anything
/// starts, naming the flag that is missing.
#[test]
fn host_and_web_without_demo_are_refused() {
    let home = TempPath(std::env::temp_dir().join(format!(
        "svolder-tui-demo-flags-test-{}",
        std::process::id()
    )));
    let scratch = &home.0;
    std::fs::create_dir_all(scratch).expect("the scratch directory is made");
    for flag in ["--host", "--web"] {
        let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_TRANSPORT", "fake")
            .env("SVOLDER_SOCKET", scratch.join("no-daemon.sock"))
            .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
            .env("SVOLDER_UNITDIR", scratch.join("units"))
            .env("XDG_CONFIG_HOME", scratch)
            .env("XDG_STATE_HOME", scratch)
            .env("XDG_DATA_HOME", scratch)
            .env("XDG_RUNTIME_DIR", scratch)
            .args(["tui", flag])
            .output()
            .expect("binary runs");
        assert_eq!(out.status.code(), Some(1), "{flag}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("--demo"), "{flag}: {err}");
        assert!(err.contains(flag), "{flag}: {err}");
    }
}
