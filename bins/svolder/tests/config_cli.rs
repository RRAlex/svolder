// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `config show`, `add`, `remove` and `devices` against a temporary `XDG_CONFIG_HOME`, never a
//! real adapter or daemon: `--transport fake` keeps every spawn off the host's radio, and
//! neither verb under test opens a transport at all.

use std::process::Command;

mod common;

use common::TempPath;

/// Run the CLI with `xdg` as its config home and its state home, over the in-process fake
/// transport.
///
/// The `systemctl` named is not a program and the unit directory is under the test's own home,
/// so nothing a verb does about the daemon reaches the machine's own systemd.
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
    TempPath(std::env::temp_dir().join(format!(
        "svolder-config-cli-test-{}-{n}",
        std::process::id()
    )))
}

#[test]
fn add_then_show_round_trips_the_device() {
    let home = config_home();
    let (code, _out, err) = svolder(
        &[
            "add",
            "0102030405060708",
            "--name",
            "fake",
            "--profile",
            "gungnir2-v2",
        ],
        &home.0,
    );
    assert_eq!(code, Some(0), "{err}");

    let (code, out, err) = svolder(&["config", "show"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    assert!(out.contains("fake"), "{out}");
    assert!(out.contains("0102030405060708"), "{out}");
    assert!(out.contains("gungnir2-v2"), "{out}");
    assert!(out.contains("auto"), "{out}");

    let (code, out, err) = svolder(&["config", "show", "--json"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("one JSON object");
    assert_eq!(v["devices"][0]["name"], "fake");
    assert_eq!(v["devices"][0]["udid"], "0102030405060708");
    assert_eq!(v["devices"][0]["passphrase"], "auto");
}

#[test]
fn a_duplicate_name_add_exits_one() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add", "0102030405060708", "--name", "fake"], &home.0);
    assert_eq!(code, Some(0), "{err}");

    let (code, _out, err) = svolder(&["add", "0807060504030201", "--name", "fake"], &home.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("fake"), "{err}");

    // The rejected add never touched the file: the one device it already had is still the
    // only one, under its original udid.
    let (code, out, err) = svolder(&["config", "show", "--json"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("one JSON object");
    assert_eq!(v["devices"].as_array().map(Vec::len), Some(1), "{out}");
    assert_eq!(v["devices"][0]["udid"], "0102030405060708");
}

#[test]
fn a_bad_udid_add_exits_one() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add", "nothex", "--name", "x"], &home.0);
    assert_eq!(code, Some(1), "{err}");
}

#[test]
fn devices_without_a_daemon_lists_the_configured_device() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add", "0102030405060708", "--name", "fake"], &home.0);
    assert_eq!(code, Some(0), "{err}");

    let (code, out, err) = svolder(&["devices"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    assert!(out.contains("fake"), "{out}");
    assert!(out.contains("0102030405060708"), "{out}");
    assert!(out.contains("no daemon"), "{out}");
}

#[test]
fn devices_with_no_config_prints_a_hint() {
    let home = config_home();
    let (code, _out, err) = svolder(&["devices"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert!(err.contains("svolder add"), "{err}");
}

#[test]
fn a_duplicate_udid_add_exits_one() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add", "0102030405060708", "--name", "fake"], &home.0);
    assert_eq!(code, Some(0), "{err}");

    let (code, _out, err) = svolder(&["add", "0102030405060708", "--name", "other"], &home.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("fake"),
        "the clash names the device that has it: {err}"
    );

    // The rejected add never touched the file: still one device, still named `fake`.
    let (code, out, err) = svolder(&["config", "show", "--json"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("one JSON object");
    assert_eq!(v["devices"].as_array().map(Vec::len), Some(1), "{out}");
    assert_eq!(v["devices"][0]["name"], "fake");
}

/// With no `--name`, the device names itself: `add` scans, finds the unit id it was given, and
/// configures it as the product it advertises.
#[test]
fn an_add_without_a_name_takes_the_product_it_advertises() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add", "0102030405060708"], &home.0);
    assert_eq!(code, Some(0), "{err}");

    let (code, out, err) = svolder(&["config", "show", "--json"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("one JSON object");
    assert_eq!(v["devices"][0]["name"], "Gungnir 2");
    assert_eq!(v["devices"][0]["udid"], "0102030405060708");
}

/// A unit id nothing in range advertises names nothing, so `add` asks for a name and leaves the
/// configuration alone.
#[test]
fn an_add_of_a_udid_nothing_advertises_asks_for_a_name() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add", "0807060504030201"], &home.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains("no name for 0807060504030201; pass --name NAME, or run `svolder scan`"),
        "{err}"
    );
    assert!(
        !home.0.join("svolder").join("config.toml").exists(),
        "a failed add wrote a configuration file"
    );
}

/// With nothing to add, `add` names the verb that finds a device and prints the line this one
/// wants.
#[test]
fn an_add_with_no_udid_points_at_scan() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add"], &home.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("svolder scan"), "{err}");
}

#[test]
fn remove_takes_the_device_out_again() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add", "0102030405060708", "--name", "fake"], &home.0);
    assert_eq!(code, Some(0), "{err}");

    let (code, _out, err) = svolder(&["remove", "fake"], &home.0);
    assert_eq!(code, Some(0), "{err}");

    let (code, out, err) = svolder(&["config", "show", "--json"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("one JSON object");
    assert_eq!(v["devices"].as_array().map(Vec::len), Some(0), "{out}");
}

/// A name no device carries is the command line's mistake, so the message names the verb that
/// lists the names there are.
#[test]
fn removing_a_device_that_is_not_configured_exits_one() {
    let home = config_home();
    let (code, _out, err) = svolder(&["remove", "nosuchdevice"], &home.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        err.contains(r#"no device named "nosuchdevice" is configured"#),
        "{err}"
    );
    assert!(err.contains("`svolder devices`"), "{err}");
}

/// The file `add` creates for the first time is the commented template, so the keys a user has
/// not set are there to read and uncomment.
#[test]
fn the_first_add_writes_the_template_around_the_device() {
    let home = config_home();
    let (code, _out, err) = svolder(&["add", "0102030405060708", "--name", "fake"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let written = std::fs::read_to_string(home.0.join("svolder").join("config.toml"))
        .expect("the config file was written");
    assert!(written.contains("# default_device"), "{written}");
    assert!(written.contains("[[device]]"), "{written}");
}

/// `config show` prints the `[mqtt]` block with the broker password as `***`, in the text and
/// in `--json` alike, so a pasted configuration never hands out the broker login.
#[test]
fn config_show_prints_the_mqtt_password_as_stars() {
    let home = config_home();
    let dir = home.0.join("svolder");
    std::fs::create_dir_all(&dir).expect("create the config directory");
    std::fs::write(
        dir.join("config.toml"),
        "schema = 1\n[mqtt]\nenabled = true\nhost = \"127.0.0.1\"\nusername = \"svolder\"\n\
         password = \"secret\"\ncommands = true\n",
    )
    .expect("write the config");

    let (code, out, err) = svolder(&["config", "show"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    assert!(!out.contains("secret"), "{out}");
    let block = out
        .split("\n\n")
        .find(|block| block.starts_with("[mqtt]"))
        .unwrap_or_else(|| panic!("no [mqtt] block: {out}"));
    assert_eq!(
        block.trim_end(),
        "[mqtt]\nenabled = true\nhost = 127.0.0.1\nport = 1883\nusername = svolder\n\
         password = ***\nbase_topic = svolder\ndiscovery_prefix = homeassistant\ncommands = true"
    );

    let (code, out, err) = svolder(&["config", "show", "--json"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    assert!(!out.contains("secret"), "{out}");
    let json: serde_json::Value = serde_json::from_str(&out).expect("one JSON object");
    assert_eq!(json["mqtt"]["password"], "***", "{json}");
    assert_eq!(json["mqtt"]["username"], "svolder", "{json}");
}

/// A config whose `default_device` names nothing configured.
const DANGLING: &str = "schema = 1\ndefault_device = \"ghost\"\n\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n";

const DANGLING_WARNING: &str = "default_device \"ghost\" names no configured device";

fn write_config(home: &std::path::Path, text: &str) {
    let dir = home.join("svolder");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), text).unwrap();
}

#[test]
fn a_dangling_default_shows_with_one_warning() {
    let home = config_home();
    write_config(&home.0, DANGLING);
    let (code, out, err) = svolder(&["config", "show"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    assert!(out.contains("default_device = ghost"), "{out}");
    assert_eq!(
        err.matches(&format!("svolder: {DANGLING_WARNING}")).count(),
        1,
        "{err}"
    );
}

#[test]
fn adding_the_named_device_clears_the_warning() {
    let home = config_home();
    write_config(&home.0, "schema = 1\ndefault_device = \"ghost\"\n");
    let (code, out, err) = svolder(&["add", "0102030405060708", "--name", "ghost"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
    let (code, _out, err) = svolder(&["config", "show"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    assert!(!err.contains("names no configured device"), "{err}");
}

#[test]
fn a_device_verb_on_a_dangling_default_fails_rather_than_guessing() {
    let home = config_home();
    write_config(&home.0, DANGLING);
    let (code, out, err) = svolder(&["status"], &home.0);
    assert_eq!(code, Some(1), "out={out} err={err}");
    assert_eq!(err.matches(DANGLING_WARNING).count(), 1, "{err}");
    // A named device needs no default.
    let (code, out, err) = svolder(&["-d", "fake", "status"], &home.0);
    assert_eq!(code, Some(0), "out={out} err={err}");
}

#[test]
fn doctor_says_a_dangling_default_once() {
    let home = config_home();
    write_config(&home.0, DANGLING);
    let (_code, out, err) = svolder(&["--transport", "fake", "doctor"], &home.0);
    assert_eq!(
        err.matches(DANGLING_WARNING).count(),
        1,
        "out={out} err={err}"
    );
}

#[test]
fn a_mistyped_effects_key_is_a_config_error() {
    let home = config_home();
    write_config(
        &home.0,
        "schema = 1\n[tui]\neffects = { breathng = false }\n",
    );
    let (code, _out, err) = svolder(&["config", "show"], &home.0);
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains(r#"unknown key "breathng""#), "{err}");
}

/// A `[web]` table that changes anything is printed by `config show`, every key, the optional
/// ones only when set; the defaults print nothing.
#[test]
fn config_show_prints_the_web_table_when_it_is_set() {
    let home = config_home();
    let dir = home.0.join("svolder");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), "schema = 1\n").unwrap();
    let (_, out, _) = svolder(&["config", "show"], &home.0);
    assert!(!out.contains("[web]"), "{out}");
    std::fs::write(
        dir.join("config.toml"),
        "schema = 1\n[web]\nenabled = true\nguest = \"read\"\nurl = \"https://s.lan/\"\n",
    )
    .unwrap();
    let (code, out, err) = svolder(&["config", "show"], &home.0);
    assert_eq!(code, Some(0), "{err}");
    let block: Vec<&str> = out
        .lines()
        .skip_while(|line| *line != "[web]")
        .take_while(|line| !line.is_empty())
        .collect();
    assert_eq!(
        block,
        [
            "[web]",
            "enabled = true",
            "listen = 127.0.0.1:8738",
            "auth = token",
            "writes = true",
            "guest = read",
            "url = https://s.lan/",
        ]
    );
}
