// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder hops` against a temporary `XDG_CONFIG_HOME`: the verbs only read and write the
//! configuration file, so `--transport fake` keeps every run off the host's radio and no daemon
//! is asked.

use std::process::Command;

mod common;

use common::TempPath;

/// Run the CLI with `xdg` as its config home and its state home, over the in-process fake
/// transport.
fn svolder(args: &[&str], xdg: &std::path::Path) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SYSTEMCTL", xdg.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", xdg.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", xdg.join("units"))
        .env("XDG_CONFIG_HOME", xdg)
        .env("XDG_STATE_HOME", xdg)
        .env_remove("SVOLDER_SOCKET")
        .args(args)
        .output()
        .expect("the binary runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A fresh, empty `XDG_CONFIG_HOME` for one test.
fn config_home() -> TempPath {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    TempPath(std::env::temp_dir().join(format!("svolder-hops-cli-test-{}-{n}", std::process::id())))
}

/// The configuration file of `xdg`, which `text` is written to.
fn write_config(xdg: &std::path::Path, text: &str) {
    let dir = xdg.join("svolder");
    std::fs::create_dir_all(&dir).expect("the config directory");
    std::fs::write(dir.join("config.toml"), text).expect("the config file");
}

/// The ids `hops list --json` prints, in order.
fn listed(xdg: &std::path::Path) -> Vec<String> {
    let (code, out, err) = svolder(&["hops", "list", "--json"], xdg);
    assert_eq!(code, Some(0), "{err}");
    out.lines()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).expect("one JSON object a line");
            v["id"].as_str().expect("an id").to_string()
        })
        .collect()
}

#[test]
fn hops_add_list_remove_round_trip() {
    let home = config_home();
    let (code, _out, err) = svolder(
        &[
            "hops",
            "add",
            "HD 650",
            "--kind",
            "headphones",
            "--note",
            "balanced",
        ],
        &home.0,
    );
    assert_eq!(code, Some(0), "{err}");

    let (code, out, err) = svolder(&["hops", "list", "--json"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        out,
        "{\"id\":\"HD 650\",\"kind\":\"headphones\",\"name\":\"HD 650\",\"note\":\"balanced\",\
         \"after\":null,\"device\":null,\"live\":false}\n"
    );

    // `list` is what a bare `hops` runs, and the text names the hop.
    let (code, out, err) = svolder(&["hops"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert!(
        out.contains("HD 650") && out.contains("headphones"),
        "{out}"
    );

    let (code, _out, err) = svolder(&["hops", "remove", "hd 650"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let (code, out, err) = svolder(&["hops", "list", "--json"], &home.0);
    assert_eq!((code, out.as_str()), (Some(0), ""), "{err}");

    let (code, _out, err) = svolder(&["hops", "remove", "HD 650"], &home.0);
    assert_ne!(code, Some(0));
    assert!(err.contains("HD 650"), "{err}");
}

#[test]
fn hops_link_names_a_configured_device() {
    let home = config_home();
    write_config(
        &home.0,
        "schema = 1\n\n[[device]]\nname = \"Bifrost 3\"\nudid = \"0102030405060708\"\n\n\
         [[device]]\nname = \"Vali\"\nudid = \"1112131415161718\"\nprofile = \"fake-amp\"\n",
    );
    let (code, _out, err) = svolder(&["hops", "link", "Vali", "--after", "Bifrost 3"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let (code, out, err) = svolder(&["hops", "list", "--json"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        out,
        "{\"id\":\"Vali\",\"kind\":\"amp\",\"name\":\"Vali\",\"note\":null,\
         \"after\":\"Bifrost 3\",\"device\":\"Vali\",\"live\":true}\n"
    );

    // A device with no profile of its own is "other" until a connection says better.
    let (code, _out, err) = svolder(&["hops", "link", "bifrost 3", "--note", "dac"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let (_, out, _) = svolder(&["hops", "list", "--json"], &home.0);
    let second: serde_json::Value = serde_json::from_str(out.lines().nth(1).unwrap()).unwrap();
    assert_eq!(
        second["device"], "Bifrost 3",
        "the configured spelling: {out}"
    );
    assert_eq!(second["kind"], "other");
    assert_eq!(second["note"], "dac");

    let (code, out, err) = svolder(&["hops", "link", "Nope"], &home.0);
    assert_ne!(code, Some(0), "{out}");
    assert!(
        err.contains("Nope") && err.contains("Bifrost 3") && err.contains("Vali"),
        "{err}"
    );
    let (code, _out, err) = svolder(&["hops", "link", "Vali"], &home.0);
    assert_ne!(code, Some(0));
    assert!(err.contains("already"), "{err}");
}

#[test]
fn hops_move_and_edit() {
    let home = config_home();
    for (name, kind) in [("A", "amp"), ("B", "headphones"), ("C", "speakers")] {
        let (code, _out, err) = svolder(
            &["hops", "add", name, "--kind", kind, "--note", "n"],
            &home.0,
        );
        assert_eq!(code, Some(0), "{err}");
    }
    assert_eq!(listed(&home.0), ["A", "B", "C"]);

    let (code, _out, err) = svolder(&["hops", "move", "C", "1"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(listed(&home.0), ["C", "A", "B"]);
    let (code, _out, err) = svolder(&["hops", "move", "C", "99"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(listed(&home.0), ["A", "B", "C"], "past the end is last");
    let (code, _out, err) = svolder(&["hops", "move", "C", "0"], &home.0);
    assert_ne!(code, Some(0), "positions count from 1");
    assert!(err.contains("1"), "{err}");

    let (code, _out, err) = svolder(
        &["hops", "edit", "A", "--kind", "speakers", "--no-note"],
        &home.0,
    );
    assert_eq!(code, Some(0), "{err}");
    let (_, out, _) = svolder(&["hops", "list", "--json"], &home.0);
    let first: serde_json::Value = serde_json::from_str(out.lines().next().unwrap()).unwrap();
    assert_eq!(
        (first["kind"].as_str(), first["note"].is_null()),
        (Some("speakers"), true)
    );

    // Edit keeps what it is not told to change, and renames in place.
    let (code, _out, err) = svolder(
        &["hops", "edit", "b", "--name", "B2", "--after", "Bifrost 3"],
        &home.0,
    );
    assert_eq!(code, Some(0), "{err}");
    let (_, out, _) = svolder(&["hops", "list", "--json"], &home.0);
    let second: serde_json::Value = serde_json::from_str(out.lines().nth(1).unwrap()).unwrap();
    assert_eq!(second["id"], "B2");
    assert_eq!(second["kind"], "headphones");
    assert_eq!(second["note"], "n");
    assert_eq!(second["after"], "Bifrost 3");

    let (code, _out, err) = svolder(&["hops", "edit", "A", "--note", "x", "--no-note"], &home.0);
    assert_ne!(code, Some(0), "{err}");
    let (code, _out, err) = svolder(&["hops", "edit", "nobody", "--kind", "amp"], &home.0);
    assert_ne!(code, Some(0));
    assert!(err.contains("nobody"), "{err}");
}

#[test]
fn hops_add_refuses_a_bad_kind_and_a_taken_name() {
    let home = config_home();
    let (code, _out, err) = svolder(&["hops", "add", "X", "--kind", "tube"], &home.0);
    assert_ne!(code, Some(0));
    assert!(err.contains("amp, headphones, speakers, other"), "{err}");
    let (code, _out, err) = svolder(&["hops", "add", "X", "--kind", "amp"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let (code, _out, err) = svolder(&["hops", "add", "x ", "--kind", "amp"], &home.0);
    assert_ne!(code, Some(0));
    assert!(err.contains("already"), "{err}");
}

#[test]
fn a_bare_hops_lists_and_takes_json() {
    let home = config_home();
    let (code, out, err) = svolder(&["hops", "--json"], &home.0);
    assert_eq!((code, out.as_str()), (Some(0), ""), "{err}");
    let (code, _out, err) = svolder(&["hops", "add", "X", "--kind", "amp"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let (code, out, err) = svolder(&["hops", "--json"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.starts_with("{\"id\":\"X\",\"kind\":\"amp\""), "{out}");
}

#[test]
fn a_linked_hop_reads_in_text_live_or_not() {
    let home = config_home();
    write_config(
        &home.0,
        "schema = 1\n\n[[device]]\nname = \"Bifrost 3\"\nudid = \"0102030405060708\"\n\n\
         [[device]]\nname = \"Vali\"\nudid = \"1112131415161718\"\nprofile = \"fake-amp\"\n\n\
         [[hop]]\ndevice = \"Vali\"\nafter = \"Bifrost 3\"\nnote = \"warm\"\n\n\
         [[hop]]\ndevice = \"Ghost\"\nafter = \"Bifrost 3\"\n",
    );
    let (code, out, err) = svolder(&["hops"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        out,
        "1. Vali (linked amp) after Bifrost 3: warm\n\
         2. Ghost (linked, no such device) after Bifrost 3\n"
    );
    assert!(err.contains("Ghost"), "the warning is said: {err}");
    let (code, out, err) = svolder(&["hops", "--json"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let ghost: serde_json::Value = serde_json::from_str(out.lines().nth(1).unwrap()).unwrap();
    assert_eq!(
        (ghost["live"].as_bool(), ghost["kind"].as_str()),
        (Some(false), Some("other"))
    );
}

#[test]
fn a_linked_hop_has_no_name_or_kind_to_edit() {
    let home = config_home();
    write_config(
        &home.0,
        "schema = 1\n\n[[device]]\nname = \"Vali\"\nudid = \"1112131415161718\"\n\n\
         [[hop]]\ndevice = \"Vali\"\n",
    );
    for flag in [["--name", "X"], ["--kind", "amp"]] {
        let (code, _out, err) = svolder(&["hops", "edit", "Vali", flag[0], flag[1]], &home.0);
        assert_ne!(code, Some(0), "{flag:?}");
        assert!(err.contains("links a device"), "{err}");
    }
    let (code, _out, err) = svolder(&["hops", "edit", "Vali", "--note", "warm"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let (_, out, _) = svolder(&["hops", "--json"], &home.0);
    assert!(out.contains("\"note\":\"warm\""), "{out}");
}

#[test]
fn hops_take_the_kind_in_lower_case_only() {
    let home = config_home();
    let (code, _out, err) = svolder(&["hops", "add", "X", "--kind", "Amp"], &home.0);
    assert_ne!(code, Some(0));
    assert!(err.contains("amp, headphones, speakers, other"), "{err}");
}

#[test]
fn config_show_lists_the_hops() {
    let home = config_home();
    let (code, _out, err) = svolder(
        &[
            "hops",
            "add",
            "HD 650",
            "--kind",
            "headphones",
            "--note",
            "balanced",
            "--after",
            "dac",
        ],
        &home.0,
    );
    assert_eq!(code, Some(0), "{err}");
    let (code, _out, err) = svolder(&["hops", "link", "Nope"], &home.0);
    assert_ne!(code, Some(0), "{err}");
    let (code, out, err) = svolder(&["config", "show"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert!(
        out.contains("[[hop]]\nname = HD 650\nkind = headphones\nnote = balanced\nafter = dac\n"),
        "{out}"
    );
}
