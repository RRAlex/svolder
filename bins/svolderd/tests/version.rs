// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolderd --version` and `-V`: the long block with the copyright and licence notice, and the
//! one-line form.
//!
//! The spawn runs over the fake transport with a home of its own, so nothing here reaches a
//! radio, a daemon or the machine's configuration.

use std::process::Command;

/// Run the binary with `arg` in an isolated environment; the stdout and the exit code.
fn run(arg: &str) -> (String, i32) {
    let home = std::env::temp_dir().join(format!("svolderd-version-test-{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("a temp home");
    let out = Command::new(env!("CARGO_BIN_EXE_svolderd"))
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_SYSTEMCTL", home.join("no-systemctl"))
        .env("XDG_CONFIG_HOME", &home)
        .env("XDG_STATE_HOME", &home)
        .env("XDG_DATA_HOME", &home)
        .env("XDG_RUNTIME_DIR", &home)
        .arg(arg)
        .output()
        .expect("the binary runs");
    let _ = std::fs::remove_dir_all(&home);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn long_version_prints_the_notice_block() {
    let (stdout, code) = run("--version");
    let expected = format!(
        "svolderd {}\nCopyright (C) 2026 the svolder authors\n\
         License GPLv3+: GNU GPL version 3 or later <https://gnu.org/licenses/gpl.html>.\n\
         This is free software: you are free to change and redistribute it.\n\
         There is NO WARRANTY, to the extent permitted by law.\n",
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(stdout, expected);
    assert_eq!(code, 0);
}

#[test]
fn short_version_prints_one_line() {
    let (stdout, code) = run("-V");
    assert_eq!(stdout, format!("svolderd {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(code, 0);
}
