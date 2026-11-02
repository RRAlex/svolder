// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder eq`'s preset verbs: `list`, `load`, `save`, `rename`, `delete` and `flatten`, and
//! `eq show`'s `preset` and `peak` lines.
//!
//! Two shapes of case: most of these need nothing but a fresh fake device and a scratch
//! `XDG_CONFIG_HOME` shared across a few calls (config is a file on disk, so it survives
//! between separate runs of the binary even though each run's in-process fake device does
//! not). Recalling a curve that was set moments before, though, needs the *same* device across
//! two calls, which only a real `svolderd` holding one persistent fake session gives; those
//! cases spawn one, sharing its `--config` path with the CLI calls that read and write
//! presets, exactly as [`start_daemon`] and its callers in `backend.rs` do for device state.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use svolder_ipc::{Client, Request, Response};

mod common;

use common::TempPath;

/// A fresh, empty `XDG_CONFIG_HOME` for one test.
fn scratch(what: &str) -> TempPath {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    TempPath(std::env::temp_dir().join(format!(
        "svolder-eq-presets-{what}-{}-{n}-{seq}",
        std::process::id()
    )))
}

/// Run the CLI over the in-process fake transport, `xdg` as its config and state home — a
/// fresh device every call, but the same configuration file across as many calls as a test
/// makes with the same `xdg`.
fn svolder(args: &[&str], xdg: &std::path::Path) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", xdg.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", xdg.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", xdg.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", xdg.join("units"))
        .env("XDG_CACHE_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg)
        .env("XDG_RUNTIME_DIR", xdg)
        .env("XDG_STATE_HOME", xdg)
        .args(args)
        .output()
        .expect("the binary runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ---- Tests needing no state shared between two devices: a fresh fake each call is enough. ----

#[test]
fn list_without_a_device_names_every_built_in_by_band_count() {
    let home = scratch("list-no-device");
    // With no device configured there is no profile to mark against, so the library is named
    // alone.
    let (code, out, err) = svolder(
        &["--transport", "ble", "--adapter", "hci-none", "eq", "list"],
        &home.0,
    );
    assert_eq!(code, 0, "out={out} err={err}");
    // The presets are grouped under a heading for each band count, the names padded.
    assert_eq!(
        out,
        "5 bands\n  Flat      built-in\n  Warm      built-in\n  Bass      built-in\n  \
         Presence  built-in\n3 bands\n  Flat      built-in\n  Warm      built-in\n  \
         Bass      built-in\n  Presence  built-in\n\
         svolder eq show names the preset that is on\n"
    );
}

/// Write `text` as `home`'s configuration file.
fn write_config(home: &std::path::Path, text: &str) {
    let dir = home.join("svolder");
    std::fs::create_dir_all(&dir).expect("create the config directory");
    std::fs::write(dir.join("config.toml"), text).expect("write the test config");
}

/// A configured device whose unit id no device carries, so a session opened for it would wait
/// out every scan window before it gave up. Its profile is named, which is all `fits` needs.
const ABSENT_DEVICE: &str = "schema = 1\n[[device]]\nname = \"gone\"\nudid = \"ffffffffffffffff\"\nprofile = \"gungnir2-v2\"\n";

/// With no daemon, `eq list` marks `fits` from the configured device's profile, marks nothing
/// `on`, and ends by naming the verb that says which preset is on.
#[test]
fn list_without_a_daemon_marks_what_fits_and_says_where_to_see_what_is_on() {
    let home = scratch("list-device");
    write_config(&home.0, ABSENT_DEVICE);
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    // gungnir2-v2 has 5 bands: the 5-band built-ins are listed, the 3-band ones counted.
    assert_eq!(
        out,
        "Flat      5 bands  built-in  fits\nWarm      5 bands  built-in  fits\n\
         Bass      5 bands  built-in  fits\nPresence  5 bands  built-in  fits\n\
         4 more for 3 bands; --all lists them\n\
         svolder eq show names the preset that is on\n"
    );

    // `--all` lists the rest under their own heading, `fits` marked on the ones that do.
    let (code, out, err) = svolder(&["eq", "list", "--all"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(
        out,
        "5 bands\n  Flat      built-in  fits\n  Warm      built-in  fits\n  \
         Bass      built-in  fits\n  Presence  built-in  fits\n3 bands\n  \
         Flat      built-in\n  Warm      built-in\n  Bass      built-in\n  \
         Presence  built-in\n\
         svolder eq show names the preset that is on\n"
    );
}

/// With a known device, a saved preset that fits is listed among the built-ins, the names padded
/// to the longest, and `--json` still carries every preset with its `bands`.
#[test]
fn list_with_a_device_pads_the_names_and_keeps_json_whole() {
    let home = scratch("list-padded");
    let (code, _out, err) = svolder(&["eq", "save", "HD650 warm"], &home.0);
    assert_eq!(code, 0, "{err}");
    std::fs::OpenOptions::new()
        .append(true)
        .open(home.0.join("svolder").join("config.toml"))
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(
                &[
                    b"\n",
                    ABSENT_DEVICE.trim_start_matches("schema = 1\n").as_bytes(),
                ]
                .concat(),
            )
        })
        .expect("append the device");
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(
        out.contains("Flat        5 bands  built-in  fits\n"),
        "{out}"
    );
    assert!(out.contains("HD650 warm  5 bands  fits\n"), "{out}");
    assert!(
        out.contains("4 more for 3 bands; --all lists them\n"),
        "{out}"
    );

    let (code, out, err) = svolder(&["eq", "list", "--json"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(out.lines().count(), 9, "{out}");
    assert!(
        out.contains(r#""name":"Flat","bands":3,"built_in":true,"fits":false"#),
        "{out}"
    );
}

/// A configured device that names no profile is read through the one the state file recorded
/// when it last connected, and a name no profile carries is said rather than swallowed.
#[test]
fn list_without_a_daemon_reads_the_profile_the_state_file_remembers() {
    let home = scratch("list-state");
    write_config(
        &home.0,
        "schema = 1\n[[device]]\nname = \"gone\"\nudid = \"ffffffffffffffff\"\n",
    );
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(!out.contains("fits"), "nothing names a profile yet: {out}");

    let write_state = |profile: &str| {
        let dir = home.0.join("svolder");
        std::fs::write(
            dir.join("state.toml"),
            format!(
                "[devices.ffffffffffffffff]\nlast_adapter = \"hci1\"\nlast_profile = \"{profile}\"\n"
            ),
        )
        .expect("write the state file");
    };
    write_state("gungnir2-v2");
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("Flat      5 bands  built-in  fits"), "{out}");

    write_state("no-such-profile");
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(!out.contains("fits"), "{out}");
    assert!(
        err.contains("svolder: profile \"no-such-profile\" is unknown; fits not marked"),
        "{err}"
    );
}

/// `--transport daemon` with no daemon running still lists the library.
#[test]
fn list_under_transport_daemon_with_no_daemon_still_lists() {
    let home = scratch("list-daemon-none");
    let (code, out, err) = svolder(&["--transport", "daemon", "eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("  Flat      built-in"), "{out}");
}

/// `eq list` opens no direct session: a configured device that is not there costs nothing.
#[test]
fn list_returns_at_once_when_the_configured_device_is_absent() {
    let home = scratch("list-absent");
    write_config(&home.0, ABSENT_DEVICE);
    let started = std::time::Instant::now();
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    let took = started.elapsed();
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(took < Duration::from_secs(1), "took {took:?}: {out}{err}");
    assert!(out.contains("Flat      5 bands  built-in  fits"), "{out}");
}

/// The note that closes the text listing is no part of the JSON one, where `on` is `null`.
#[test]
fn list_json_without_a_daemon_carries_no_note() {
    let home = scratch("list-json");
    write_config(&home.0, ABSENT_DEVICE);
    let (code, out, err) = svolder(&["eq", "list", "--json"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    for line in out.lines() {
        let row: serde_json::Value = serde_json::from_str(line).expect("one JSON object per line");
        assert!(row["on"].is_null(), "{line}");
    }
    assert!(
        out.contains(r#""name":"Flat","bands":5,"built_in":true,"fits":true"#),
        "{out}"
    );
}

#[test]
fn a_built_in_is_refused_for_save_rename_and_delete() {
    let home = scratch("builtin-refused");
    let (code, _out, err) = svolder(&["eq", "save", "Flat"], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("Flat") && err.contains("built-in"), "{err}");

    let (code, _out, err) = svolder(&["eq", "rename", "Flat", "Not flat"], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("Flat") && err.contains("built-in"), "{err}");

    let (code, _out, err) = svolder(&["eq", "rename", "custom", "flat"], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("built-in"), "{err}");

    let (code, _out, err) = svolder(&["eq", "delete", "presence", "--yes"], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("built-in"), "{err}");
}

#[test]
fn built_ins_load_by_their_one_word_names_in_any_case() {
    let home = scratch("builtin-load-case");
    let (code, _out, err) = svolder(&["eq", "load", "warm"], &home.0);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("svolder: loaded Warm (7 writes)"), "{err}");

    let (code, _out, err) = svolder(&["eq", "load", "BASS"], &home.0);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("svolder: loaded Bass ("), "{err}");
}

#[test]
fn save_then_rename_round_trips_through_the_config_file() {
    let home = scratch("rename");
    let (code, _out, err) = svolder(&["eq", "save", "mycurve"], &home.0);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = svolder(&["eq", "rename", "mycurve", "renamed"], &home.0);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(!out.contains("mycurve"), "{out}");
    assert!(out.contains("renamed"), "{out}");
}

#[test]
fn a_name_longer_than_forty_characters_is_refused_for_save_and_rename() {
    let home = scratch("long-name");
    let long = "x".repeat(41);
    let (code, _out, err) = svolder(&["eq", "save", &long], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("at most 40 characters"), "{err}");

    let (code, _out, err) = svolder(&["eq", "save", &"x".repeat(40)], &home.0);
    assert_eq!(code, 0, "forty characters are a name: {err}");
    let (code, _out, err) = svolder(&["eq", "rename", &"x".repeat(40), &long], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("at most 40 characters"), "{err}");
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "the file no longer loads: {err}");
    assert!(out.contains(&"x".repeat(40)), "{out}");
}

/// A name is saved, renamed and looked up without the blanks round it: `eq save " HD650 warm "`
/// keeps `HD650 warm`, which `eq rename`, `eq delete`, `eq load` and `--force` then reach by
/// that name, in any case and with blanks round it again.
#[test]
fn a_name_is_kept_without_the_blanks_round_it_and_found_by_the_trimmed_name() {
    let home = scratch("padded-name");
    let (code, _out, err) = svolder(&["eq", "save", " HD650 warm "], &home.0);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("saved HD650 warm (5 bands)"), "{err}");
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.lines().any(|l| l == "  HD650 warm"), "{out}");

    let (code, _out, err) = svolder(&["eq", "save", "hd650 WARM"], &home.0);
    assert_eq!(
        code, 1,
        "a second save under the trimmed name was not refused: {err}"
    );
    let (code, _out, err) = svolder(&["eq", "save", "HD650 warm", "--force"], &home.0);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = svolder(&["eq", "load", " hd650 warm"], &home.0);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("loaded HD650 warm"), "{err}");

    let (code, _out, err) = svolder(&["eq", "rename", "HD650 warm", " Late night "], &home.0);
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains("renamed \"HD650 warm\" to \"Late night\""),
        "{err}"
    );
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.lines().any(|l| l == "  Late night"), "{out}");
    assert!(!out.contains("HD650"), "{out}");

    let (code, _out, err) = svolder(&["eq", "delete", " late NIGHT ", "--yes"], &home.0);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("removed preset \"Late night\""), "{err}");
    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(!out.contains("Late night"), "{out}");
}

/// A name holding a newline or a tab, and a blank one, are refused before anything is written,
/// with words about the name rather than about the configuration file.
#[test]
fn a_blank_name_or_one_holding_a_control_character_is_refused() {
    let home = scratch("control-name");
    for name in ["new\nline", "tab\there"] {
        let (code, _out, err) = svolder(&["eq", "save", name], &home.0);
        assert_eq!(code, 1, "{name:?}: {err}");
        assert!(err.contains("control character"), "{name:?}: {err}");
    }
    let (code, _out, err) = svolder(&["eq", "save", "   "], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("a preset needs a name"), "{err}");
    assert!(!err.contains("config path"), "{err}");

    let (code, _out, err) = svolder(&["eq", "save", "mine"], &home.0);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = svolder(&["eq", "rename", "mine", "new\nline"], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("control character"), "{err}");
    let (code, _out, err) = svolder(&["eq", "rename", "mine", " "], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("a preset needs a name"), "{err}");

    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.lines().any(|l| l == "  mine"), "{out}");
    assert!(!out.contains("new") && !out.contains("tab"), "{out}");
}

/// `eq show --json` prints one object per reading of the tone control, then the preset that
/// is on and the peak — `null` for a curve with nothing to warn about.
#[test]
fn eq_show_json_ends_with_the_preset_and_the_peak() {
    let home = scratch("show-json");
    let (code, out, err) = svolder(&["eq", "show", "--json"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    let lines: Vec<serde_json::Value> = out
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect();
    let (readings, tail) = lines.split_last_chunk::<2>().expect("two closing objects");
    assert!(!readings.is_empty(), "{out}");
    assert!(
        readings.iter().all(|reading| reading.get("id").is_some()),
        "{out}"
    );
    assert_eq!(
        tail,
        &[
            serde_json::json!({ "preset": "custom" }),
            serde_json::json!({ "peak": null })
        ]
    );
}

#[test]
fn renaming_a_name_nothing_carries_is_refused() {
    let home = scratch("rename-missing");
    let (code, _out, err) = svolder(&["eq", "rename", "nosuch", "other"], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("nosuch"), "{err}");
}

#[test]
fn delete_with_yes_removes_it() {
    let home = scratch("delete-yes");
    let (code, _out, err) = svolder(&["eq", "save", "gone-soon"], &home.0);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = svolder(&["eq", "delete", "gone-soon", "--yes"], &home.0);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(!out.contains("gone-soon"), "{out}");
}

/// Without `--yes` and off a terminal (a spawned child's stdin is never one), `eq delete`
/// refuses outright, as `passphrase clear` does, and the preset survives.
#[test]
fn delete_without_yes_on_a_non_tty_stdin_refuses() {
    let home = scratch("delete-no-yes");
    let (code, _out, err) = svolder(&["eq", "save", "keepme"], &home.0);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = svolder(&["eq", "delete", "keepme"], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("refusing to delete a preset without confirmation"),
        "{err}"
    );

    let (code, out, err) = svolder(&["eq", "list"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("keepme"), "the refusal left it alone: {out}");
}

/// A fresh device rests in Flat mode with every band at 0 dB: there is no band to write, so a
/// flatten writes nothing, switches nothing, and says the curve is already flat — as the TUI's
/// `0` does.
#[test]
fn flatten_of_a_flat_curve_writes_nothing_and_leaves_the_mode() {
    let home = scratch("flatten-flat");
    let (code, _out, err) = svolder(&["eq", "flatten"], &home.0);
    assert_eq!(code, 0, "{err}");
    assert_eq!(err, "svolder: the curve is already flat (0 writes)\n");
}

// ---- Tests needing the same device across two calls: a real daemon holds it for them. ----

/// A `svolderd` over the fake transport, its config file shared with whatever `svolder` calls
/// a case makes against it, so `eq set`/`eq mode` writes the CLI sends through it in one call
/// are still there for the next, and a preset a CLI call saves or loads is read from and
/// written to the same file the daemon itself was started on (which it never needs to reload:
/// presets are a configuration-file concern the daemon takes no part in).
struct Daemon {
    child: Child,
    dir: TempPath,
    xdg: PathBuf,
    socket: PathBuf,
}

impl Daemon {
    /// Start the daemon and wait until its one fake device is connected, so a first call has
    /// something to read.
    async fn start(svolderd: &Path) -> Daemon {
        Daemon::start_with(
            svolderd,
            "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n",
            true,
        )
        .await
    }

    /// Start the daemon on `config`, waiting for a connected device only when `connected`.
    async fn start_with(svolderd: &Path, config_text: &str, connected: bool) -> Daemon {
        let dir = scratch("daemon");
        let xdg = dir.0.clone();
        let config_dir = xdg.join("svolder");
        std::fs::create_dir_all(&config_dir).expect("create the config directory");
        let config = config_dir.join("config.toml");
        std::fs::write(&config, config_text).expect("write the daemon's config");
        let socket = xdg.join("svolder.sock");

        let mut command = Command::new(svolderd);
        for key in [
            "SVOLDER_FAKE_PROFILE",
            "SVOLDER_FAKE_UNIT",
            "SVOLDER_FAKE_FIRMWARE",
            "SVOLDER_FAKE_PASSPHRASE",
            "SVOLDER_FAKE_FAULTS",
        ] {
            command.env_remove(key);
        }
        let child = command
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
            .env("SVOLDER_TRANSPORT", "fake")
            .env("XDG_CACHE_HOME", &xdg)
            .env("XDG_RUNTIME_DIR", &xdg)
            .env("XDG_STATE_HOME", &xdg)
            .env_remove("SVOLDER_SOCKET")
            .arg("--socket")
            .arg(&socket)
            .arg("--config")
            .arg(&config)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn svolderd");

        let daemon = Daemon {
            child,
            dir,
            xdg,
            socket,
        };

        let mut client = None;
        for _ in 0..500 {
            if daemon.socket.exists()
                && let Ok(c) = Client::connect(&daemon.socket).await
            {
                client = Some(c);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let mut client = client.expect("the daemon came up within 5s");
        if !connected {
            return daemon;
        }
        for _ in 0..500 {
            let devices = client
                .request(Request::Devices)
                .await
                .expect("a devices reply");
            if matches!(&devices, Response::Devices { devices } if devices.iter().any(|d| d.connected))
            {
                return daemon;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the daemon never connected to the fake device");
    }

    /// Run the CLI against this daemon's socket, with this daemon's own configuration file as
    /// its `XDG_CONFIG_HOME`, so `eq save`/`eq load`/`eq show` all read and write the one file
    /// this daemon was started on.
    fn cli(&self, args: &[&str]) -> (i32, String, String) {
        self.cli_in(&self.xdg, args)
    }

    /// [`Daemon::cli`] with `home` as the CLI's `XDG_CONFIG_HOME`: a home other than the
    /// daemon's makes the CLI write a configuration file the daemon does not read.
    fn cli_in(&self, home: &std::path::Path, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
            .env("SVOLDER_TRANSPORT", "daemon")
            .env("XDG_CACHE_HOME", &self.xdg)
            .env("XDG_DATA_HOME", &self.xdg)
            .env("XDG_RUNTIME_DIR", &self.xdg)
            .env("SVOLDER_SYSTEMCTL", self.xdg.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", self.xdg.join("no-wireplumber"))
            .env("SVOLDER_UNITDIR", self.xdg.join("units"))
            .env("XDG_CONFIG_HOME", home)
            .env("XDG_STATE_HOME", &self.xdg)
            .arg("--socket")
            .arg(&self.socket)
            .args(args)
            .output()
            .expect("the binary runs");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

impl Daemon {
    /// Name one more device in the daemon's own configuration file, which a daemon that reads
    /// the file again then holds: the one thing a reload visibly changes here.
    fn name_another_device(&self, name: &str, udid: &str) {
        let path = self.xdg.join("svolder").join("config.toml");
        let mut text = std::fs::read_to_string(&path).expect("the daemon's config reads");
        text.push_str(&format!(
            "\n[[device]]\nname = \"{name}\"\nudid = \"{udid}\"\n"
        ));
        std::fs::write(&path, text).expect("the daemon's config is written");
    }

    /// How many devices the daemon holds.
    async fn devices_held(&self) -> usize {
        let mut client = Client::connect(&self.socket)
            .await
            .expect("the daemon answers");
        match client.request(Request::Devices).await {
            Ok(Response::Devices { devices }) => devices.len(),
            other => panic!("devices answered {other:?}"),
        }
    }

    /// Wait until the daemon holds `n` devices, or fail saying how many it held.
    async fn until_holding(&self, n: usize) {
        let mut held = 0;
        for _ in 0..500 {
            held = self.devices_held().await;
            if held == n {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the daemon holds {held} devices, not {n}");
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = &self.dir; // dropped along with self, removing the directory
    }
}

/// Over a daemon that holds the device, `eq list` marks the preset that is on, and prints no
/// note about where to find it.
#[tokio::test]
async fn list_over_a_daemon_marks_the_preset_that_is_on() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;
        let (code, _out, err) = daemon.cli(&["eq", "load", "warm"]);
        assert_eq!(code, 0, "{err}");
        let (code, out, err) = daemon.cli(&["eq", "list"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(
            out.contains("Warm      5 bands  built-in  fits  on"),
            "{out}"
        );
        assert!(!out.contains("names the preset that is on"), "{out}");
    })
    .await
    .expect("test timed out");
}

/// The whole recall loop: boost a band and switch to EQ, save it, move the device away from
/// that curve, load it back — nonzero writes, and `eq show` reads back the saved values and
/// names the preset as on — then load it again onto the curve it just recalled: nothing left
/// to write.
#[tokio::test]
async fn save_then_load_recalls_the_saved_curve_and_a_second_load_writes_nothing() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;

        let (code, _out, err) = daemon.cli(&["eq", "set", "1", "g", "3"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = daemon.cli(&["eq", "mode", "eq"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = daemon.cli(&["eq", "save", "mycurve"]);
        assert_eq!(code, 0, "{err}");

        // Move the device away from the saved curve, back to Flat mode with a flat band 1.
        let (code, _out, err) = daemon.cli(&["eq", "set", "1", "g", "0"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = daemon.cli(&["eq", "mode", "flat"]);
        assert_eq!(code, 0, "{err}");

        let (code, _out, err) = daemon.cli(&["eq", "load", "mycurve"]);
        assert_eq!(code, 0, "{err}");
        assert!(err.contains("loaded mycurve"), "{err}");
        assert!(!err.contains("(0 writes)"), "the curve had moved: {err}");

        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("mode eq"), "{out}");
        assert!(out.contains("preset mycurve"), "{out}");
        assert!(out.contains("band 1  40.0 Hz  +3.10 dB  Q 4.30"), "{out}");

        // The device already carries this exact curve: recalling it again writes nothing.
        let (code, _out, err) = daemon.cli(&["eq", "load", "mycurve"]);
        assert_eq!(code, 0, "{err}");
        assert!(err.contains("loaded mycurve (0 writes)"), "{err}");
    })
    .await
    .expect("test timed out");
}

/// The peak line: gone at rest, present once a band is boosted in EQ mode, with the same
/// figure named twice (the peak, and the headroom it costs).
#[tokio::test]
async fn eq_show_prints_the_peak_line_once_a_band_is_boosted() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;

        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(!out.contains("peak"), "flat at rest, no peak line: {out}");

        let (code, _out, err) = daemon.cli(&["eq", "mode", "eq"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = daemon.cli(&["eq", "set", "1", "g", "6"]);
        assert_eq!(code, 0, "{err}");

        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        let peak_line = out
            .lines()
            .find(|l| l.starts_with("peak "))
            .unwrap_or_else(|| panic!("no peak line: {out}"));
        assert!(peak_line.contains("+6."), "{peak_line}");
        assert!(
            peak_line.contains("keep the volume at or below -6."),
            "{peak_line}"
        );

        // The same figure as the last of the JSON objects, after the preset's.
        let (code, out, err) = daemon.cli(&["eq", "show", "--json"]);
        assert_eq!(code, 0, "out={out} err={err}");
        let tail: Vec<&str> = out.lines().rev().take(2).collect();
        assert_eq!(tail, [r#"{"peak":6.0}"#, r#"{"preset":"custom"}"#], "{out}");
    })
    .await
    .expect("test timed out");
}

/// In Flat mode, with a band off 0 dB, a flatten brings the gain back and then switches the
/// mode to EQ so the change is heard, and says so; the device then reads EQ
/// mode with every band at 0 dB.
#[tokio::test]
async fn flatten_in_flat_mode_switches_to_eq_after_the_gains() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;

        let (code, _out, err) = daemon.cli(&["eq", "set", "1", "g", "3"]);
        assert_eq!(code, 0, "{err}");

        let (code, _out, err) = daemon.cli(&["eq", "flatten"]);
        assert_eq!(code, 0, "{err}");
        assert_eq!(
            err, "svolder: switched to EQ\nsvolder: flattened (2 writes)\n",
            "{err}"
        );

        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("mode eq"), "{out}");
        assert!(out.contains("band 1  40.0 Hz  +0.0 dB"), "{out}");
    })
    .await
    .expect("test timed out");
}

/// In Loudness mode a flatten leaves the mode alone: bands apply
/// only in EQ mode, so there is nothing here to hear yet. The gain is still brought back.
#[tokio::test]
async fn flatten_in_loudness_mode_does_not_switch() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;

        let (code, _out, err) = daemon.cli(&["eq", "mode", "loudness"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = daemon.cli(&["eq", "set", "1", "g", "3"]);
        assert_eq!(code, 0, "{err}");

        let (code, _out, err) = daemon.cli(&["eq", "flatten"]);
        assert_eq!(code, 0, "{err}");
        assert_eq!(err, "svolder: flattened (1 write)\n", "{err}");

        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("mode loudness"), "{out}");
        assert!(out.contains("band 1  40.0 Hz  +0.0 dB"), "{out}");
    })
    .await
    .expect("test timed out");
}

/// On a curve already flat in Flat mode, the device keeps its mode after a flatten.
#[tokio::test]
async fn flatten_of_a_flat_curve_keeps_flat_mode() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;

        let (code, _out, err) = daemon.cli(&["eq", "flatten"]);
        assert_eq!(code, 0, "{err}");
        assert_eq!(err, "svolder: the curve is already flat (0 writes)\n");

        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("mode flat"), "{out}");
    })
    .await
    .expect("test timed out");
}

/// `eq save`, `eq rename` and `eq delete` each tell a running daemon that reads the file to
/// reload after writing it, and say nothing of it. The daemon reads no `[[preset]]` block
/// through a device, so the reload is seen through what else the file changed: a device named
/// in it before the verb ran is held by the daemon once the verb is done.
#[tokio::test]
async fn save_rename_and_delete_tell_a_daemon_reading_the_file_to_reload() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;
        assert_eq!(daemon.devices_held().await, 1);

        for (held, (args, udid)) in (2..).zip([
            (vec!["eq", "save", "reload-check"], "0000000000000002"),
            (
                vec!["eq", "rename", "reload-check", "reload-checked"],
                "0000000000000003",
            ),
            (
                vec!["eq", "delete", "reload-checked", "--yes"],
                "0000000000000004",
            ),
        ]) {
            daemon.name_another_device(&format!("device{held}"), udid);
            let (code, _out, err) = daemon.cli(&args);
            assert_eq!(code, 0, "{args:?}: {err}");
            assert!(
                !err.contains("the daemon"),
                "{args:?} said something: {err}"
            );
            daemon.until_holding(held).await;
        }
    })
    .await
    .expect("test timed out");
}

/// A daemon that reads another file than the one the verb wrote is not told to reload: the
/// device its own file names stays unheld, and the verb says nothing of it. The same verb on the
/// file it does read is what reloads it, so the check can tell the two apart.
#[tokio::test]
async fn a_preset_verb_leaves_a_daemon_reading_another_file_alone() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;
        let elsewhere = scratch("elsewhere");
        std::fs::create_dir_all(elsewhere.0.join("svolder")).expect("a config directory");
        daemon.name_another_device("second", "0000000000000002");

        let (code, _out, err) = daemon.cli_in(&elsewhere.0, &["eq", "save", "elsewhere"]);
        assert_eq!(code, 0, "{err}");
        assert!(!err.contains("the daemon"), "{err}");
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(daemon.devices_held().await, 1, "the daemon reloaded");

        let (code, _out, err) = daemon.cli(&["eq", "save", "here"]);
        assert_eq!(code, 0, "{err}");
        daemon.until_holding(2).await;
    })
    .await
    .expect("test timed out");
}

/// A load of Warm and then a flatten puts every band back on its centre frequency and centre Q
/// at 0 dB, as the Flat preset loaded does, so `eq show` reads the same and names Flat.
#[tokio::test]
async fn eq_show_names_flat_after_warm_is_flattened() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;

        let (code, _out, err) = daemon.cli(&["eq", "load", "Warm"]);
        assert_eq!(code, 0, "{err}");
        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("preset Warm"), "{out}");

        let (code, _out, err) = daemon.cli(&["eq", "flatten"]);
        assert_eq!(code, 0, "{err}");
        assert_eq!(err, "svolder: flattened (6 writes)\n", "{err}");
        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("preset Flat"), "{out}");

        let (code, _out, err) = daemon.cli(&["eq", "load", "Flat"]);
        assert_eq!(code, 0, "{err}");
        assert_eq!(err.trim(), "svolder: loaded Flat (0 writes)", "{err}");
        let (code, flat, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={flat} err={err}");
        assert_eq!(flat, out, "flatten and Flat leave the same curve");
    })
    .await
    .expect("test timed out");
}

/// A daemon that answers but holds no connected device leaves `eq list` to read `fits` from the
/// configured device's profile: the fitting presets, then the count of the rest.
#[tokio::test]
async fn list_over_a_daemon_with_no_device_marks_fits_from_the_configuration() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start_with(&svolderd, ABSENT_DEVICE, false).await;
        let (code, out, err) = daemon.cli(&["eq", "list"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("Flat      5 bands  built-in  fits\n"), "{out}");
        assert!(
            out.contains("4 more for 3 bands; --all lists them\n"),
            "{out}"
        );
        assert!(!out.contains("3 bands  built-in"), "{out}");
    })
    .await
    .expect("test timed out");
}

// ---- `eq import`, and what lists, loads and shows of an imported preset. ----

/// A hand-written Equalizer APO file: a bass shelf, peaks, a treble shelf, and a low-pass
/// filter that is skipped.
const APO_FILE: &str = "Preamp: -4.0 dB\r\n\
    Filter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70\r\n\
    Filter 2: ON PK Fc 200 Hz Gain -2.1 dB Q 0.90\r\n\
    Filter 3: ON PK Fc 1500 Hz Gain -3.0 dB Q 2.00\r\n\
    Filter 4: ON PK Fc 3200 Hz Gain 4.2 dB Q 2.50\r\n\
    Filter 5: ON HSC Fc 10000 Hz Gain -4.0 dB Q 0.70\r\n\
    Filter 6: ON LP Fc 18000 Hz\r\n";

/// A five-band and a three-band device, and a second five-band one.
const THREE_DEVICES: &str = "schema = 1\n\
    [[device]]\nname = \"a\"\nudid = \"ffffffffffffff01\"\nprofile = \"gungnir2-v2\"\n\
    [[device]]\nname = \"b\"\nudid = \"ffffffffffffff02\"\nprofile = \"mimir\"\n\
    [[device]]\nname = \"c\"\nudid = \"ffffffffffffff03\"\nprofile = \"gungnir2-v2\"\n";

/// Write `APO_FILE` as `file` in `home`, and return its path as text.
fn apo_file(home: &std::path::Path, file: &str) -> String {
    let path = home.join(file);
    std::fs::create_dir_all(home).expect("create the home");
    std::fs::write(&path, APO_FILE).expect("write the APO file");
    path.to_string_lossy().into_owned()
}

/// `eq import FILE` saves the file under its own name, without ` ParametricEQ`, and says what
/// it saved, then one fit line per distinct tone control among the configured devices.
#[test]
fn import_saves_the_file_and_prints_a_fit_line_per_tone_control() {
    let home = scratch("import");
    write_config(&home.0, THREE_DEVICES);
    let file = apo_file(&home.0, "Test HD ParametricEQ.txt");
    let (code, out, err) = svolder(&["eq", "import", &file], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(out, "");
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(
        lines[0], "svolder: imported \"Test HD\" (5 filters, preamp -4.0 dB, 1 skipped)",
        "{err}"
    );
    assert_eq!(lines.len(), 3, "one line per distinct tone control: {err}");
    assert!(
        lines[1].starts_with("svolder: Gungnir 2 · 5 bands: fit ") && lines[1].contains(" dB RMS"),
        "{err}"
    );
    assert!(
        lines[2].starts_with("svolder: Mimir · 3 bands: fit "),
        "{err}"
    );

    let config = std::fs::read_to_string(home.0.join("svolder").join("config.toml")).unwrap();
    assert!(config.starts_with(THREE_DEVICES), "{config}");
    assert!(
        config.contains("[[preset]]\nname = \"Test HD\"\npreamp = -4.0\nfilter = [\n"),
        "{config}"
    );
    assert!(
        config.contains("{ type = \"LSC\", f = 105.0, g = 5.5, q = 0.7 },"),
        "{config}"
    );
    assert!(!config.contains("LP"), "{config}");
}

/// With no device configured an import still saves, and the one-filter count is singular.
#[test]
fn import_with_no_device_prints_no_fit_line() {
    let home = scratch("import-bare");
    let path = home.0.join("one.txt");
    std::fs::create_dir_all(&home.0).unwrap();
    std::fs::write(&path, "Filter 1: ON PK Fc 1000 Hz Gain 2 dB Q 1\n").unwrap();
    let (code, out, err) = svolder(&["eq", "import", &path.to_string_lossy()], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(err, "svolder: imported \"one\" (1 filter, preamp 0.0 dB)\n");
}

/// `eq rename` keeps an imported preset's filters byte for byte, and `eq delete` takes them out.
#[test]
fn rename_and_delete_keep_or_remove_an_imported_presets_filters() {
    let home = scratch("import-rename-delete");
    let file = apo_file(&home.0, "phones.txt");
    let config_path = home.0.join("svolder").join("config.toml");
    let (code, _out, err) = svolder(&["eq", "import", &file, "--name", "Phones"], &home.0);
    assert_eq!(code, 0, "{err}");
    let before = std::fs::read_to_string(&config_path).unwrap();
    let (code, _out, err) = svolder(&["eq", "rename", "Phones", "Cans"], &home.0);
    assert_eq!(code, 0, "{err}");
    let after = std::fs::read_to_string(&config_path).unwrap();
    assert_eq!(
        after,
        before.replace("name = \"Phones\"", "name = \"Cans\"")
    );
    let (code, _out, err) = svolder(&["eq", "delete", "Cans", "--yes"], &home.0);
    assert_eq!(code, 0, "{err}");
    let gone = std::fs::read_to_string(&config_path).unwrap();
    assert!(before.contains("type = \"LSC\""), "{before}");
    for key in ["\n[[preset", "\ntype = ", "Cans"] {
        assert!(!gone.contains(key), "{key}: {gone}");
    }
}

/// `--name` overrides the file's name; a taken name refuses until `--force`; a built-in's name
/// refuses even with `--force`; a bad file refuses with its line and saves nothing.
#[test]
fn import_names_refuses_a_taken_name_and_forces_only_over_a_user_preset() {
    let home = scratch("import-names");
    let file = apo_file(&home.0, "phones.txt");
    let config_path = home.0.join("svolder").join("config.toml");

    let (code, _out, err) = svolder(&["eq", "import", &file, "--name", "  My Phones "], &home.0);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("imported \"My Phones\""), "{err}");

    let before = std::fs::read_to_string(&config_path).unwrap();
    let (code, _out, err) = svolder(&["eq", "import", &file, "--name", "my phones"], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("already exists; use --force"), "{err}");
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);

    let (code, _out, err) = svolder(
        &["eq", "import", &file, "--name", "my phones", "--force"],
        &home.0,
    );
    assert_eq!(code, 0, "{err}");
    let after = std::fs::read_to_string(&config_path).unwrap();
    assert_eq!(after.matches("\n[[preset]]").count(), 1, "{after}");
    assert!(after.contains("name = \"my phones\""), "{after}");

    for force in [&[][..], &["--force"][..]] {
        let mut args = vec!["eq", "import", file.as_str(), "--name", "warm"];
        args.extend(force);
        let (code, _out, err) = svolder(&args, &home.0);
        assert_eq!(code, 1, "{err}");
        assert!(err.contains("built-in"), "{err}");
    }
    let long = "x".repeat(41);
    let (code, _out, err) = svolder(&["eq", "import", &file, "--name", &long], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("at most 40 characters"), "{err}");

    let bad = home.0.join("bad.txt");
    std::fs::write(&bad, "Preamp: -3 dB\nGraphicEQ: 20 1\n").unwrap();
    let (code, _out, err) = svolder(&["eq", "import", &bad.to_string_lossy()], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("line 2: GraphicEQ is not read"), "{err}");
    let missing = home.0.join("nothing.txt");
    let (code, _out, err) = svolder(&["eq", "import", &missing.to_string_lossy()], &home.0);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("cannot read"), "{err}");
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), after);
}

/// `eq list` marks an imported preset `imported` and lists it on every tone control, with no
/// band count; without a device it has a heading of its own.
#[test]
fn list_marks_an_imported_preset_for_every_tone_control() {
    let home = scratch("import-list");
    write_config(&home.0, THREE_DEVICES);
    let file = apo_file(&home.0, "Test HD.txt");
    let (code, _out, err) = svolder(&["eq", "import", &file], &home.0);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = svolder(&["eq", "list", "--device", "a"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("Test HD   imported  fits\n"), "{out}");
    let (code, out, err) = svolder(&["eq", "list", "--device", "b"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("Test HD   imported  fits\n"), "{out}");
    assert!(
        out.contains("4 more for 5 bands; --all lists them\n"),
        "{out}"
    );

    let (code, out, err) = svolder(&["eq", "list", "--all", "--device", "a"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("imported\n  Test HD   fits\n"), "{out}");

    let (code, out, err) = svolder(&["eq", "list", "--json", "--device", "a"], &home.0);
    assert_eq!(code, 0, "out={out} err={err}");
    let row = out.lines().last().expect("a row");
    assert_eq!(
        row,
        r#"{"name":"Test HD","bands":0,"built_in":false,"imported":true,"fits":true,"on":null}"#
    );
    assert!(!out.lines().next().unwrap().contains("imported"), "{out}");

    let (code, out, err) = svolder(
        &["--transport", "ble", "--adapter", "hci-none", "eq", "list"],
        &write_bare(&home.0),
    );
    assert_eq!(code, 0, "out={out} err={err}");
}

/// A scratch home holding the imported preset and no device, for a listing with nothing to fit.
fn write_bare(home: &std::path::Path) -> PathBuf {
    let bare = home.join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    let file = apo_file(&bare, "Test HD.txt");
    let (code, _out, err) = svolder(&["eq", "import", &file], &bare);
    assert_eq!(code, 0, "{err}");
    bare
}

/// A preset of the file's own, imported, is never updated with the device's curve.
#[test]
fn save_over_an_imported_preset_refuses_with_or_without_force() {
    let home = scratch("import-save");
    let file = apo_file(&home.0, "Test HD.txt");
    let (code, _out, err) = svolder(&["eq", "import", &file], &home.0);
    assert_eq!(code, 0, "{err}");
    let before = std::fs::read_to_string(home.0.join("svolder").join("config.toml")).unwrap();
    for args in [
        &["eq", "save", "test hd"][..],
        &["eq", "save", "Test HD", "--force"][..],
    ] {
        let (code, _out, err) = svolder(args, &home.0);
        assert_eq!(code, 1, "{err}");
        assert!(
            err.contains("Test HD is imported: save the curve under a new name"),
            "{err}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(home.0.join("svolder").join("config.toml")).unwrap(),
        before
    );
}

/// Over a daemon holding the device: `eq load` prints the fit line before `loaded`, the device
/// is then on that preset, `eq list` says `on`, and `eq show` adds the `fit` line.
#[tokio::test]
async fn load_of_an_imported_preset_prints_its_fit_and_show_names_it() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;
        let file = apo_file(&daemon.xdg, "Test HD.txt");
        let (code, _out, err) = daemon.cli(&["eq", "import", &file]);
        assert_eq!(code, 0, "{err}");

        let (code, _out, err) = daemon.cli(&["eq", "load", "test hd"]);
        assert_eq!(code, 0, "{err}");
        let fit = err.find("svolder: fit ").unwrap_or_else(|| panic!("{err}"));
        let loaded = err
            .find("loaded Test HD")
            .unwrap_or_else(|| panic!("{err}"));
        assert!(fit < loaded, "{err}");
        assert!(!err.contains("clamped"), "{err}");
        assert!(!err.contains("(0 writes)"), "{err}");

        let (code, out, err) = daemon.cli(&["eq", "show"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("preset Test HD\nfit "), "{out}");
        assert!(out.contains("mode eq"), "{out}");

        let (code, out, err) = daemon.cli(&["eq", "list"]);
        assert_eq!(code, 0, "out={out} err={err}");
        assert!(out.contains("Test HD   imported  fits  on\n"), "{out}");

        let (code, _out, err) = daemon.cli(&["eq", "load", "test hd"]);
        assert_eq!(code, 0, "{err}");
        assert!(err.contains("loaded Test HD (0 writes)"), "{err}");
    })
    .await
    .expect("test timed out");
}

/// A load through the daemon reports its preset, and `eq show` reads the daemon's memory: of a
/// built-in and a saved twin of it with the same curve, the one loaded last is the one shown.
#[tokio::test]
async fn eq_show_names_the_twin_that_was_loaded() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = Daemon::start(&svolderd).await;
        let (code, _out, err) = daemon.cli(&["eq", "load", "warm"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = daemon.cli(&["eq", "save", "Warm too"]);
        assert_eq!(code, 0, "{err}");
        let (_, out, _) = daemon.cli(&["eq", "show"]);
        assert!(out.contains("preset Warm\n"), "{out}");
        let (code, _out, err) = daemon.cli(&["eq", "load", "warm too"]);
        assert_eq!(code, 0, "{err}");
        let (_, out, _) = daemon.cli(&["eq", "show"]);
        assert!(out.contains("preset Warm too\n"), "{out}");
    })
    .await
    .expect("test timed out");
}
