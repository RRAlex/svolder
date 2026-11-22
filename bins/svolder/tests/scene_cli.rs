// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder scene …` as the binary runs it, against temporary XDG homes and no daemon: every verb
//! says scenes are kept by the daemon and exits 3, and the verb is listed in the help.

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

/// Run the CLI with `home` as its config, state and runtime home and no daemon behind it.
fn svolder(args: &[&str], home: &Path) -> Ran {
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_SYSTEMCTL", home.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.join("no-wireplumber"))
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_STATE_HOME", home)
        .env("XDG_RUNTIME_DIR", home)
        .env_remove("SVOLDER_SOCKET")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(args)
        .output()
        .expect("the binary runs");
    Ran {
        code: out.status.code(),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn home(tag: &str) -> TempPath {
    let dir = common::short_join(&common::short_unique(&format!("scene-{tag}")));
    std::fs::create_dir_all(&dir).unwrap();
    TempPath(dir)
}

#[test]
fn with_no_daemon_list_and_show_read_the_config_and_every_other_verb_exits_3() {
    let dir = home("none");
    std::fs::create_dir_all(dir.0.join("svolder")).unwrap();
    std::fs::write(
        dir.0.join("svolder/config.toml"),
        "schema = 1\n[[scene]]\nname = \"Desk\"\n[scene.computer]\neq_on = false\nname = \"Flat\"\npreamp = 0.0\nfilter = []\n",
    )
    .unwrap();
    let ran = svolder(&["scene"], &dir.0);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert_eq!(ran.out, "Desk  computer\n");
    assert!(ran.err.contains("no daemon running"), "{}", ran.err);
    let ran = svolder(&["scene", "show", "desk", "--json"], &dir.0);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert_eq!(
        ran.out,
        "{\"name\":\"Desk\",\"parts\":[{\"who\":\"computer\"}]}\n"
    );
    for args in [
        &["scene", "save", "Desk"][..],
        &["scene", "load", "Desk", "--yes"],
        &["scene", "rename", "Desk", "Movie"],
        &["scene", "delete", "Desk", "--yes"],
    ] {
        let ran = svolder(args, &dir.0);
        assert_eq!(ran.code, Some(3), "{args:?}: {}", ran.err);
        assert!(ran.out.is_empty(), "{args:?}: {}", ran.out);
        assert!(
            ran.err
                .contains("svolder: scenes are kept by the daemon, and none is running"),
            "{args:?}: {}",
            ran.err
        );
    }
}

#[test]
fn the_help_lists_the_scene_verb_and_its_verbs() {
    let dir = home("help");
    let top = svolder(&["--help"], &dir.0);
    assert!(
        top.out
            .lines()
            .any(|line| line.trim_start().starts_with("scene ")
                && line.contains("List, save, load, rename or delete scenes")),
        "{}",
        top.out
    );
    let ran = svolder(&["scene", "--help"], &dir.0);
    for verb in ["list", "show", "save", "load", "rename", "delete", "--json"] {
        assert!(ran.out.contains(verb), "{verb} in {}", ran.out);
    }
}
