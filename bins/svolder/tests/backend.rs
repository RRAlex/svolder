// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where a read verb runs: a daemon when one answers on the socket, a direct session when
//! none does.
//!
//! Every run of the binary here pins `SVOLDER_SYSTEMCTL` to a program that is not there and
//! `SVOLDER_UNITDIR` to a directory under the case's own home, so no case can reach this
//! machine's user manager whatever verb it runs or fix it applies.

mod common;

#[path = "../../../crates/svolder-mqtt/tests/support/broker.rs"]
mod broker;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use svolder_ipc::{Client, DeviceInfo, Reply, Request, RequestLine, Response, ValueObject};

use common::{TempDir, TempPath, WATCH_DEADLINE, Watcher};

/// How long the daemon-backed case may take before it is a failure rather than a hang. Longer
/// than [`WATCH_DEADLINE`], so a watching case reports its own failure rather than timing out.
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the MQTT status case may take: a broker that has to start (a minute at the worst)
/// and four daemons, each given [`WATCH_DEADLINE`] to reach the line it waits for.
const MQTT_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// The config the daemon under test reads: the fake device's own unit id, named `fake`.
const CONFIG: &str = "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n";

/// With the transport left at `auto`, nothing answering on the socket and autospawn turned
/// off, the verb opens a direct BlueZ session of its own rather than reporting that no daemon
/// is running.
///
/// `autospawn = false` is what keeps this case to the last step of the chain: with it on, the
/// step before would start a daemon, which is the launcher's own cases' business and not a
/// process this one wants. The adapter names a controller that does not exist, so the run ends
/// at adapter lookup — a read-only D-Bus enumeration — with exit 3, rather than scanning the
/// host's radio.
#[test]
fn auto_falls_back_to_a_direct_ble_session_when_no_daemon() {
    let home = config_home("schema = 1\n[daemon]\nautospawn = false\n");
    let dead = home.0.join("no-daemon.sock");
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_SOCKET", &dead)
        .env("SVOLDER_SYSTEMCTL", home.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", home.0.join("units"))
        .env("XDG_CONFIG_HOME", &home.0)
        .env("XDG_STATE_HOME", &home.0)
        .env_remove("SVOLDER_TRANSPORT")
        .args(["-v", "status"])
        .output()
        .expect("the binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "err={err}");
    assert!(
        err.contains("direct (ble)"),
        "-v should name the direct BLE path: {err}"
    );
}

/// `--transport daemon` with nothing to reach says so and names the verb that installs one,
/// rather than quietly opening a session of its own.
#[test]
fn transport_daemon_with_no_daemon_to_reach_says_what_to_run() {
    let home = config_home("schema = 1\n[daemon]\nautospawn = false\n");
    let dead = home.0.join("no-daemon.sock");
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_SOCKET", &dead)
        .env("SVOLDER_SYSTEMCTL", home.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", home.0.join("units"))
        .env("XDG_CONFIG_HOME", &home.0)
        .env("XDG_STATE_HOME", &home.0)
        .args(["--transport", "daemon", "status"])
        .output()
        .expect("the binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "err={err}");
    assert!(
        err.contains("no daemon") && err.contains("svolder daemon install"),
        "the message names what to do next: {err}"
    );
}

/// `daemon status` says what it found: no systemd to ask, no daemon answering, and the log a
/// daemon nobody installed would write.
///
/// `PATH` holds nothing at all here, so the `systemctl` the verb would run is not there to be
/// run: this is the machine with no systemd, without needing one that has none.
#[test]
fn daemon_status_without_systemd_reports_the_socket_and_the_log() {
    let home = config_home("schema = 1\n");
    let dead = home.0.join("no-daemon.sock");
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("PATH", home.0.join("no-programs"))
        .env("SVOLDER_SOCKET", &dead)
        .env("SVOLDER_SYSTEMCTL", home.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", home.0.join("units"))
        .env("XDG_CONFIG_HOME", &home.0)
        .env("XDG_STATE_HOME", &home.0)
        .args(["daemon", "status"])
        .output()
        .expect("the binary runs");
    let (out_text, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert_eq!(out.status.code(), Some(0), "out={out_text} err={err}");
    assert!(
        out_text.contains("systemd: no user session"),
        "out={out_text}"
    );
    assert!(
        out_text.contains(&format!("nothing answers on {}", dead.display())),
        "out={out_text}"
    );
    assert!(
        out_text.contains(&format!(
            "log: {}",
            home.0.join("svolder").join("svolderd.log").display()
        )),
        "out={out_text}"
    );
}

/// The launcher chain says nothing of its own: a daemon it could not start is handed back to
/// the verb, which says it where the verb's own messages go.
///
/// Here the session the verb falls back to cannot open either — the adapter named does not
/// exist, which is how this case reaches no radio — so the only thing printed is that failure,
/// and not a word from inside the chain. The daemon it tried to start is the built `svolderd`,
/// told to use the same adapter, so it ends at adapter lookup (a read-only D-Bus enumeration)
/// and leaves its last words in the log. `SVOLDER_SYSTEMCTL` names a program that is not there,
/// which is this machine standing in for one with no systemd without either being asked
/// anything.
#[test]
fn the_chain_prints_nothing_of_its_own_when_a_daemon_will_not_come_up() {
    let home = config_home("schema = 1\n");
    let log = home.0.join("svolder").join("svolderd.log");
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_SYSTEMCTL", home.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", home.0.join("units"))
        .env("SVOLDER_SOCKET", home.0.join("no-daemon.sock"))
        .env("XDG_CONFIG_HOME", &home.0)
        .env("XDG_STATE_HOME", &home.0)
        .env_remove("SVOLDER_TRANSPORT")
        .args(["status"])
        .output()
        .expect("the binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "err={err}");
    assert_eq!(
        err.matches("using a direct session").count(),
        0,
        "the chain printed a line of its own: {err}"
    );
    assert!(
        err.contains("no adapter named hci-none"),
        "the failure the run reports is the session's own: {err}"
    );
    // The daemon really was started, and what it said on its way out is in the log the line
    // this run would have printed — had it had a session to fall back into — names.
    assert!(
        std::fs::read_to_string(&log).is_ok_and(|said| said.contains("hci-none")),
        "the daemon this run started left nothing in {}",
        log.display()
    );
}

/// `daemon install` writes its units where `SVOLDER_UNITDIR` says and asks the `systemctl`
/// `SVOLDER_SYSTEMCTL` names, which is what makes `make install-service SYSTEMCTL=echo
/// UNITDIR=<tmp>` a dry run that touches nothing of this machine's.
#[test]
fn daemon_install_writes_where_it_is_told_and_runs_the_systemctl_it_is_given() {
    let home = config_home("schema = 1\n");
    let units = home.0.join("units");
    let record = home.0.join("systemctl.log");
    let fake = home.0.join("systemctl");
    common::install_script(
        &fake,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"systemctl $*\" >> '{}'\nexit 0\n",
            record.display()
        ),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_SYSTEMCTL", &fake)
        .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", &units)
        .env("SVOLDER_SOCKET", home.0.join("svolder.sock"))
        .env("XDG_CONFIG_HOME", &home.0)
        .env("XDG_STATE_HOME", &home.0)
        .args(["daemon", "install"])
        .output()
        .expect("the binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "err={err}");

    for unit in ["svolderd.socket", "svolderd.service"] {
        assert!(units.join(unit).is_file(), "{unit} was not written: {err}");
    }
    let asked = std::fs::read_to_string(&record).expect("the fake systemctl was run");
    for line in [
        "systemctl --user daemon-reload",
        "systemctl --user enable --now svolderd.socket",
        "systemctl --user enable --now svolderd.service",
    ] {
        assert!(asked.contains(line), "{line} was not asked for: {asked}");
    }

    // And taking them out again removes exactly those files, through the same two variables.
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_SYSTEMCTL", &fake)
        .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", &units)
        .env("SVOLDER_SOCKET", home.0.join("svolder.sock"))
        .env("XDG_CONFIG_HOME", &home.0)
        .env("XDG_STATE_HOME", &home.0)
        .args(["daemon", "uninstall"])
        .output()
        .expect("the binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "err={}",
        String::from_utf8_lossy(&out.stderr)
    );
    for unit in ["svolderd.socket", "svolderd.service"] {
        assert!(!units.join(unit).exists(), "{unit} was left behind");
    }
}

/// `--transport fake` runs a direct session over the in-process device, and `-v` says so.
#[test]
fn the_fake_transport_runs_a_direct_session() {
    let empty = TempPath(temp_path(".empty-config"));
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", empty.0.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", empty.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", empty.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", empty.0.join("units"))
        .env("XDG_CONFIG_HOME", &empty.0)
        .args(["-v", "status"])
        .output()
        .expect("the binary runs");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "err={err}");
    assert!(
        err.contains("direct (fake)"),
        "-v should name the direct fake path: {err}"
    );
}

/// `daemon status` says where the daemon's MQTT bridge stands, on the line after the daemon's
/// own: off with no section, connecting while no broker answers — with the wait before the next
/// attempt — and connected, read-only or taking commands. The broker is the in-process one on
/// 127.0.0.1, and the one that does not answer is a port nothing listens on there.
#[tokio::test]
async fn daemon_status_says_where_the_mqtt_bridge_stands() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(MQTT_TEST_TIMEOUT, async {
        let broker = broker::TestBroker::start();
        let away = broker::free_port();
        let section = |port: u16, extra: &str| {
            format!("{CONFIG}[mqtt]\nenabled = true\nhost = \"127.0.0.1\"\nport = {port}\n{extra}")
        };
        // A wait is whole seconds, counting down from the step it is at: `2 s`, then `4 s`, and
        // so on.
        let connecting = format!("mqtt: connecting to 127.0.0.1:{away} (retry in ");
        let cases = [
            (CONFIG.to_string(), "mqtt: off".to_string()),
            (section(away, ""), connecting.clone()),
            (
                section(broker.port, ""),
                format!("mqtt: connected to 127.0.0.1:{} (read-only)", broker.port),
            ),
            (
                section(broker.port, "commands = true\n"),
                format!("mqtt: connected to 127.0.0.1:{} (commands on)", broker.port),
            ),
        ];
        for (config, wanted) in cases {
            let daemon = start_daemon_on(&svolderd, &config, &[]).await;
            let deadline = tokio::time::Instant::now() + WATCH_DEADLINE;
            loop {
                let (code, out, err) = svolder(&daemon.socket, &["daemon", "status"]);
                assert_eq!(code, Some(0), "out={out} err={err}");
                let lines: Vec<&str> = out.lines().collect();
                let at = lines.iter().position(|line| line.starts_with("daemon:"));
                let bridge = at.and_then(|at| lines.get(at + 1)).copied();
                let waiting = |line: &str| {
                    line.strip_prefix(connecting.as_str())
                        .and_then(|rest| rest.strip_suffix(" s)"))
                        .is_some_and(|secs| {
                            secs.parse::<u64>().is_ok_and(|n| (1..=60).contains(&n))
                        })
                };
                let printed = match wanted == connecting {
                    true => bridge.is_some_and(waiting),
                    false => bridge == Some(wanted.as_str()),
                };
                if printed {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "never printed {wanted:?}; out={out}"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    })
    .await
    .expect("test timed out");
}

/// With a daemon answering, the read verbs get their readings from it rather than opening a
/// session of their own.
#[tokio::test]
async fn read_verbs_run_over_the_daemon() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[]).await;

        let (code, out, err) = svolder(&daemon.socket, &["status"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.contains("Volume"), "{out}");
        assert_over_the_daemon(&daemon.socket, &err);

        let (code, out, err) = svolder(&daemon.socket, &["-d", "fake", "get", "volume"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert_eq!(out.trim(), "-26.1 dB", "out={out} err={err}");
        assert_over_the_daemon(&daemon.socket, &err);
    })
    .await
    .expect("test timed out");
}

/// A fault the daemon reports still exits 5: the CLI recognises one through the very profile
/// the daemon says it resolved.
#[tokio::test]
async fn a_fault_over_the_daemon_exits_five() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(
            &svolderd,
            &[
                ("SVOLDER_FAKE_FAULTS", "1"),
                ("SVOLDER_FAKE_PROFILE", "fake-amp"),
            ],
        )
        .await;
        let (code, out, err) = svolder(&daemon.socket, &["status"]);
        assert_eq!(code, Some(5), "out={out} err={err}");
        assert!(err.contains("active fault"), "{err}");
        assert_over_the_daemon(&daemon.socket, &err);
    })
    .await
    .expect("test timed out");
}

/// A write verb runs over the daemon too: the daemon resolves `up`/`down` and the literal,
/// and the CLI prints the reading it confirmed.
#[tokio::test]
async fn set_confirms_over_the_daemon() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[]).await;
        let (code, out, err) = svolder(&daemon.socket, &["set", "input", "4"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.contains('4'), "out={out} err={err}");
        assert_over_the_daemon(&daemon.socket, &err);
    })
    .await
    .expect("test timed out");
}

/// The built-in setting the bands read is marked `on` in `preset list`, over a daemon that holds
/// the device: the fake starts flat, `preset apply` moves it, and `get` reads what was written.
#[tokio::test]
async fn preset_list_marks_the_builtin_on_over_a_daemon() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[("SVOLDER_FAKE_PROFILE", "lokimax")]).await;
        let on = |out: &str| -> Vec<String> {
            out.lines()
                .filter(|line| line.trim_end().ends_with(" on"))
                .map(|line| line.split_whitespace().next().unwrap_or("").to_string())
                .collect()
        };
        let (code, out, err) = svolder(&daemon.socket, &["preset", "list"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert_over_the_daemon(&daemon.socket, &err);
        assert_eq!(on(&out), ["Flat"], "the fake starts flat: {out}");
        assert!(
            !out.lines()
                .any(|l| l.starts_with("Preset") && l.ends_with(" on")),
            "a slot is never marked: {out}"
        );

        let (code, out, err) = svolder(&daemon.socket, &["preset", "apply", "Bass Boost"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(err.contains("Bass Boost on (3 writes)"), "{err}");
        let (code, out, err) = svolder(&daemon.socket, &["get", "band1"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert_eq!(out.trim(), "10.4 dB");

        let (code, out, err) = svolder(&daemon.socket, &["preset", "list", "--json"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        let rows: Vec<serde_json::Value> = out
            .lines()
            .map(|line| serde_json::from_str(line).expect("one object a line"))
            .collect();
        let marked: Vec<&str> = rows
            .iter()
            .filter(|row| row["on"] == true)
            .map(|row| row["name"].as_str().expect("a name"))
            .collect();
        assert_eq!(marked, ["Bass Boost"], "{out}");
        assert!(rows[0]["on"].is_null(), "a slot has no `on`: {out}");

        let (code, out, err) = svolder(&daemon.socket, &["preset", "apply", "Bass Boost"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(err.contains("Bass Boost is already on"), "{err}");
    })
    .await
    .expect("test timed out");
}

/// A save with `--yes` keeps the bands in a slot, a save turned down off a terminal leaves it
/// alone, and a later recall brings the saved bands back: over a daemon, which holds the one
/// device all of these runs talk to.
#[tokio::test]
async fn set_save_with_yes_saves_and_a_later_recall_brings_the_bands_back() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[("SVOLDER_FAKE_PROFILE", "lokimax")]).await;
        let run = |args: &[&str]| svolder(&daemon.socket, args);
        let (code, out, err) = run(&["preset", "apply", "Smiley"]);
        assert_eq!(code, Some(0), "out={out} err={err}");

        let (code, out, err) = run(&["set", "save", "2", "--yes"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert_over_the_daemon(&daemon.socket, &err);
        assert!(err.contains("saved the bands to preset 2"), "{err}");

        let (code, out, err) = run(&["preset", "apply", "Flat"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        // Off a terminal, with no --yes, the save is refused before it is sent.
        let (code, out, err) = run(&["set", "save", "2"]);
        assert_eq!(code, Some(1), "out={out} err={err}");
        assert!(err.contains("add --yes to save without asking"), "{err}");

        let (code, out, err) = run(&["set", "recall", "2"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(err.contains("recalled preset 2"), "{err}");
        // The device reports the bands after a recall, and the daemon holds them as they come.
        let mut band1 = String::new();
        for _ in 0..100 {
            band1 = run(&["get", "band1"]).1.trim().to_string();
            if band1 == "6.4 dB" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(band1, "6.4 dB", "Smiley's first band came back");
    })
    .await
    .expect("test timed out");
}

/// `raw set` over the daemon: the id, type and value travel as the user typed them.
#[tokio::test]
async fn raw_set_over_the_daemon() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[]).await;
        let (code, out, err) = svolder(&daemon.socket, &["raw", "11", "u8", "100"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.contains("100"), "out={out} err={err}");
        assert_over_the_daemon(&daemon.socket, &err);
    })
    .await
    .expect("test timed out");
}

/// `eq set` over the daemon: the band number stays 1-based on the wire, and the daemon
/// answers with the band reading it confirmed.
#[tokio::test]
async fn eq_set_over_the_daemon() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[]).await;
        let (code, out, err) = svolder(&daemon.socket, &["eq", "set", "1", "f", "50"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.contains("Hz"), "out={out} err={err}");
        assert_over_the_daemon(&daemon.socket, &err);
    })
    .await
    .expect("test timed out");
}

/// `watch` over the daemon prints the snapshot it read before subscribing, so a watcher sees
/// the device's current values without waiting for the next spontaneous status.
///
/// The wait is for the line rather than for a fixed moment: the case ends as soon as the
/// snapshot arrives, and only a genuine failure to print one takes the whole deadline.
#[tokio::test]
async fn watch_over_the_daemon_prints_the_snapshot() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[]).await;
        let empty = TempPath(temp_path(".empty-config"));
        // The directory holds no configuration, only the child's stderr, so that the `-v`
        // line naming the path the run took can be read back once the wait has ended. Creating
        // it rather than creating it if absent keeps a reused name loud.
        std::fs::create_dir(&empty.0).expect("create the test directory");
        let log = empty.0.join("watch.err");
        let child = Command::new(env!("CARGO_BIN_EXE_svolder"))
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
            .env("SVOLDER_SYSTEMCTL", empty.0.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", empty.0.join("no-wireplumber"))
            .env("SVOLDER_UNITDIR", empty.0.join("units"))
            .env("XDG_CONFIG_HOME", &empty.0)
            .env("XDG_STATE_HOME", &empty.0)
            .env_remove("SVOLDER_TRANSPORT")
            .env_remove("SVOLDER_SOCKET")
            .arg("-v")
            .arg("--socket")
            .arg(&daemon.socket)
            .arg("watch")
            .stdout(Stdio::piped())
            .stderr(Stdio::from(
                std::fs::File::create(&log).expect("create the stderr file"),
            ))
            .spawn()
            .expect("spawn watch");
        // Reading the child's lines blocks, so it happens off the runtime; the watcher is
        // dropped there too, which kills and reaps the child however the wait ended.
        let (arrived, printed) = tokio::task::spawn_blocking(move || {
            let mut watcher = Watcher::new(child);
            let arrived = watcher.wait_for_line("Volume", WATCH_DEADLINE);
            (arrived, watcher.output().to_string())
        })
        .await
        .expect("the reader finished");
        assert!(
            arrived,
            "watch printed no snapshot within {WATCH_DEADLINE:?}: {printed:?}"
        );
        let err = std::fs::read_to_string(&log).expect("read the child's stderr");
        assert_over_the_daemon(&daemon.socket, &err);
    })
    .await
    .expect("test timed out");
}

/// Over a daemon, a Ctrl-C sent right after the first status line ends `watch` with exit 0.
#[tokio::test]
async fn watch_over_the_daemon_leaves_with_zero_on_ctrl_c() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[]).await;
        let empty = TempPath(temp_path(".empty-config-ctrl-c"));
        std::fs::create_dir(&empty.0).expect("create the test directory");
        let child = Command::new(env!("CARGO_BIN_EXE_svolder"))
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
            .env("SVOLDER_SYSTEMCTL", empty.0.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", empty.0.join("no-wireplumber"))
            .env("SVOLDER_UNITDIR", empty.0.join("units"))
            .env("HOME", &empty.0)
            .env("XDG_CONFIG_HOME", &empty.0)
            .env("XDG_STATE_HOME", &empty.0)
            .env("XDG_DATA_HOME", &empty.0)
            .env("XDG_CACHE_HOME", &empty.0)
            .env_remove("SVOLDER_TRANSPORT")
            .env_remove("SVOLDER_SOCKET")
            .arg("--socket")
            .arg(&daemon.socket)
            .arg("watch")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn watch");
        // The watcher is dropped inside the blocking task, which kills and reaps the child
        // however the case ended.
        let (arrived, status, printed) = tokio::task::spawn_blocking(move || {
            let mut watcher = Watcher::new(child);
            let arrived = watcher.wait_for_line("Volume", WATCH_DEADLINE);
            let mut status = None;
            if arrived {
                watcher.interrupt();
                status = watcher.exit_within(Duration::from_secs(10));
            }
            (arrived, status, watcher.output().to_string())
        })
        .await
        .expect("the reader finished");
        assert!(
            arrived,
            "watch printed no snapshot within {WATCH_DEADLINE:?}: {printed:?}"
        );
        let status = status.expect("watch did not leave after Ctrl-C");
        assert_eq!(status.code(), Some(0), "{status:?}");
    })
    .await
    .expect("test timed out");
}

/// A `default_device` naming nothing configured is a usage error, not a quiet fallback to
/// the one device that is configured. Both front ends read the file through `Config::parse`,
/// which is where the name is checked, so `svolderd` refuses to start on the same file.
#[test]
fn an_unknown_default_device_is_a_usage_error() {
    let guard = config_home(
        "schema = 1\ndefault_device = \"nope\"\n\
         [[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", guard.0.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", guard.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", guard.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", guard.0.join("units"))
        .env("XDG_CONFIG_HOME", &guard.0)
        .args(["status"])
        .output()
        .expect("the binary runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "err={stderr}");
    assert!(stderr.contains("default_device \"nope\""), "{stderr}");
    assert!(
        stderr.contains("Run `svolder config path`"),
        "the message names what to do next: {stderr}"
    );
}

/// A verb naming a device the daemon holds but has not found yet says so, and that the daemon
/// keeps looking, and leaves with the code 3 of a device that is not there.
///
/// The daemon's transport is the fake one, which finds only its own device: the second one the
/// configuration names is never on the air.
#[tokio::test]
async fn a_device_the_daemon_has_not_found_yet_says_so_and_what_to_check() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let config =
            format!("{CONFIG}[[device]]\nname = \"second\"\nudid = \"0908070605040302\"\n");
        let daemon = start_daemon_on(&svolderd, &config, &[]).await;
        let home = TempPath(temp_path(".home"));
        std::fs::create_dir(&home.0).expect("create the case's home");
        let socket = daemon.socket.clone();
        let out = tokio::task::spawn_blocking(move || {
            Command::new(env!("CARGO_BIN_EXE_svolder"))
                .env("SVOLDER_HOST", "off")
                .env("SVOLDER_TRANSPORT", "fake")
                .env("SVOLDER_ADAPTER", "hci-none")
                .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
                .env("SVOLDER_SYSTEMCTL", home.0.join("no-systemctl"))
                .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
                .env("SVOLDER_UNITDIR", home.0.join("units"))
                .env("XDG_CONFIG_HOME", &home.0)
                .env("XDG_STATE_HOME", &home.0)
                .env("XDG_DATA_HOME", &home.0)
                .env("XDG_RUNTIME_DIR", &home.0)
                .env_remove("SVOLDER_SOCKET")
                .env_remove("SVOLDER_TRANSPORT")
                .arg("--socket")
                .arg(&socket)
                .args(["-d", "second", "status"])
                .output()
                .expect("the binary runs")
        })
        .await
        .expect("the verb finished");
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(3), "err={err}");
        assert!(
            err.contains(
                "svolder: second has not been found yet. svolderd keeps looking: check that \
                 it is on and in range (`svolder daemon logs`)"
            ),
            "{err}"
        );
    })
    .await
    .expect("test timed out");
}

/// A direct session reads the configuration too: `-d` names a configured device, which is
/// then matched by its unit id, and that device's input names reach the readings.
#[test]
fn the_config_steers_a_direct_session() {
    let guard = config_home(
        "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n\
         [device.inputs]\n1 = \"TV\"\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", guard.0.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", guard.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", guard.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", guard.0.join("units"))
        .env("XDG_CONFIG_HOME", &guard.0)
        .args(["-d", "fake", "get", "input"])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "out={stdout} err={stderr}");
    assert_eq!(stdout.trim(), "1 (TV)", "out={stdout} err={stderr}");
}

/// A daemon that has lost the link still answers from the readings it held, and the CLI says
/// so rather than printing them as if they were current. Exit code and output are unchanged.
#[tokio::test]
async fn a_disconnected_device_over_the_daemon_is_named_on_stderr() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = TempDir::new("stub");
        let socket = dir.join("s.sock");
        let stub = stub_daemon(socket.clone()).await;
        let (code, out, err) = run_svolder(&socket, &["status"]).await;
        stub.abort();
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.contains("Volume"), "out={out} err={err}");
        assert!(
            err.contains("fake is disconnected; readings are the last known"),
            "out={out} err={err}"
        );
        assert_over_the_daemon(&socket, &err);
    })
    .await
    .expect("test timed out");
}

/// The `identify` verb prints no readings, so against a daemon that has lost the link it does not
/// say that the readings are the last known: it says the device is not connected, and nothing
/// else about the link.
#[tokio::test]
async fn identify_over_a_daemon_that_lost_the_link_says_no_word_about_stale_readings() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = TempDir::new("stub");
        let socket = dir.join("s.sock");
        let stub = stub_daemon(socket.clone()).await;
        let (code, out, err) = run_svolder(&socket, &["identify"]).await;
        stub.abort();
        assert_eq!(code, Some(3), "out={out} err={err}");
        assert!(
            err.contains(svolder_ipc::NOT_CONNECTED),
            "out={out} err={err}"
        );
        assert!(!err.contains("last known"), "out={out} err={err}");
        assert_over_the_daemon(&socket, &err);
    })
    .await
    .expect("test timed out");
}

/// A scan over the daemon asks for at most the longest window the daemon listens for, and says
/// so once when it cut the one it was given down. A window within the bound goes as it is.
#[tokio::test]
async fn a_long_scan_over_the_daemon_is_cut_to_the_daemons_longest_window() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = TempDir::new("stub");
        let socket = dir.join("s.sock");
        let stub = stub_daemon(socket.clone()).await;
        let said = "svolder: the daemon scans for at most 30 s";

        let (code, out, err) = run_svolder(&socket, &["scan", "--json", "--timeout", "90s"]).await;
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.contains("\"unit\":\"30000\""), "out={out} err={err}");
        assert_eq!(err.matches(said).count(), 1, "out={out} err={err}");

        let (code, out, err) = run_svolder(&socket, &["scan", "--json", "--timeout", "7s"]).await;
        stub.abort();
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.contains("\"unit\":\"7000\""), "out={out} err={err}");
        assert!(!err.contains(said), "out={out} err={err}");
    })
    .await
    .expect("test timed out");
}

/// A device the daemon's listing no longer carries has no profile to read a key through, and
/// `get` says so in the words `status` uses for the same race, rather than calling the key
/// unknown.
#[tokio::test]
async fn get_on_a_device_the_daemon_no_longer_lists_names_the_mismatch() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = TempDir::new("stub");
        let socket = dir.join("s.sock");
        let stub = stub_daemon_listing(socket.clone(), false).await;
        let (code, out, err) = run_svolder(&socket, &["get", "volume"]).await;
        stub.abort();
        assert_eq!(code, Some(3), "out={out} err={err}");
        assert!(out.is_empty(), "{out}");
        assert!(
            err.contains("the daemon answered for an unknown device fake; volume not checked"),
            "{err}"
        );
        assert!(!err.contains("unknown key"), "{err}");
    })
    .await
    .expect("test timed out");
}

/// `set` on a device the daemon's listing no longer carries is refused in `get`'s words: with no
/// profile a centred balance would go to the daemon as a raw step, a hard left.
#[tokio::test]
async fn set_on_a_device_the_daemon_no_longer_lists_is_refused() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = TempDir::new("stub");
        let socket = dir.join("s.sock");
        let stub = stub_daemon_listing(socket.clone(), false).await;
        let (code, out, err) = run_svolder(&socket, &["set", "balance", "5"]).await;
        stub.abort();
        assert_eq!(code, Some(3), "out={out} err={err}");
        assert!(out.is_empty(), "{out}");
        assert!(
            err.contains("the daemon answered for an unknown device fake; balance not checked"),
            "{err}"
        );
    })
    .await
    .expect("test timed out");
}

/// `doctor` names the daemon it looked at under `-v`, as `Backend::open` does.
#[tokio::test]
async fn doctor_names_the_daemon_it_found_under_v() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = TempDir::new("stub");
        let socket = dir.join("s.sock");
        let stub = stub_daemon(socket.clone()).await;
        let (_code, out, err) = run_svolder(&socket, &["doctor", "--json"]).await;
        stub.abort();
        assert_over_the_daemon(&socket, &err);
        assert_eq!(err.matches("daemon at").count(), 1, "out={out} err={err}");
    })
    .await
    .expect("test timed out");
}

/// `devices` says under `-v` where its listing came from: the daemon, or the file.
#[tokio::test]
async fn devices_names_its_source_under_v() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = TempDir::new("stub");
        let socket = dir.join("s.sock");
        let stub = stub_daemon(socket.clone()).await;
        let (code, out, err) = run_svolder(&socket, &["devices"]).await;
        stub.abort();
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert_over_the_daemon(&socket, &err);
        assert!(!err.contains("from the configuration"), "{err}");

        // Nothing answers on this socket, and `ble` never asks.
        let (code, out, err) = run_svolder(&socket, &["--transport", "ble", "devices"]).await;
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(err.contains("svolder: from the configuration\n"), "{err}");
        assert!(!err.contains("daemon at"), "{err}");
    })
    .await
    .expect("test timed out");
}

/// A profile key that addresses function 250 does not load: the verb stops in the loader's words,
/// naming the profile, the key and the function, before any frame is built, so a user profile
/// cannot put a passphrase change (or its readout) in front of a front end.
#[test]
fn a_profile_key_on_the_passphrase_function_does_not_load() {
    let guard = profile_home(
        "gungnir2-v2",
        "schema = 1\nname = \"gungnir2-v2\"\n\
         [[control]]\nid = [250]\nkey = \"danger\"\nlabel = \"Danger\"\nkind = \"select\"\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SYSTEMCTL", guard.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", guard.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", guard.0.join("units"))
        .env("XDG_CONFIG_HOME", &guard.0)
        .env_remove("SVOLDER_SOCKET")
        .args(["set", "danger", "1"])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "out={stdout} err={stderr}");
    assert!(
        stderr.contains("profile gungnir2-v2, control danger: function 250 cannot be a control"),
        "out={stdout} err={stderr}"
    );
}

/// A write the device turns away over the passphrase says so and names the verb that sets one,
/// and exits 3 as it always did.
///
/// The configuration names a passphrase the fake device does not hold, which is what the
/// device rejects: the session connects, marks the passphrase rejected, and refuses the write.
#[test]
fn a_write_the_passphrase_rejects_names_the_next_step() {
    let guard = config_home(
        "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n\
         passphrase = \"9999\"\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SYSTEMCTL", guard.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", guard.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", guard.0.join("units"))
        .env("XDG_CONFIG_HOME", &guard.0)
        .env_remove("SVOLDER_SOCKET")
        .args(["set", "input", "2"])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "out={stdout} err={stderr}");
    assert!(
        stderr.contains(
            "Give svolder the device's passcode, the one set when the device was added to the \
             app"
        ),
        "the refusal names what to do next: out={stdout} err={stderr}"
    );
    // The refusal does not send the user to the command that changes the device's code, and
    // the note on the mismatch reads ahead of it, so the two read as one account.
    assert!(!stderr.contains("passphrase set"), "err={stderr}");
    let note = stderr.find("advertises").expect("the note is said");
    let refusal = stderr.find("rejected").expect("the refusal is said");
    assert!(note < refusal, "err={stderr}");
}

/// A verb run on a device whose configured code does not match the one it advertises says so on
/// stderr, once, naming the device and the fix and none of the digits, and still does what it
/// was asked: under `auto` it says nothing.
#[test]
fn a_configured_code_that_does_not_match_is_said_on_stderr() {
    let run = |passphrase: &str, args: &[&str]| {
        let guard = config_home(&format!(
            "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n\
             passphrase = \"{passphrase}\"\n"
        ));
        let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_TRANSPORT", "fake")
            .env("SVOLDER_SYSTEMCTL", guard.0.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", guard.0.join("no-wireplumber"))
            .env("SVOLDER_UNITDIR", guard.0.join("units"))
            .env("XDG_CONFIG_HOME", &guard.0)
            .env_remove("SVOLDER_SOCKET")
            .args(args)
            .output()
            .expect("the binary runs");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };

    let (_, out, err) = run("9999", &["get", "input"]);
    let note = "svolder: fake: the passphrase does not match the code the device advertises; \
                set `passphrase` in the config to the right code, or to `auto`";
    assert_eq!(err.matches(note).count(), 1, "out={out} err={err}");
    for digits in ["1234", "9999"] {
        assert!(!err.contains(digits), "{digits} on stderr: {err}");
    }

    for passphrase in ["auto", "1234"] {
        let (code, out, err) = run(passphrase, &["get", "input"]);
        assert_eq!(code, Some(0), "{passphrase}: out={out} err={err}");
        assert!(!err.contains("advertises"), "{passphrase}: err={err}");
    }
}

/// `devices` over the daemon lists the fake device as connected, straight from
/// `Request::Devices` rather than the configuration file.
///
/// This is the one verb that names no path: it answers from the daemon without opening a
/// transport, so `-v` has no choice to report. Its listing says which path it took instead —
/// `no daemon` is the state every device gets when the configuration file is all there was.
#[tokio::test]
async fn devices_over_the_daemon_lists_fake_as_connected() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[]).await;
        let (code, out, err) = svolder(&daemon.socket, &["devices"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.contains("fake"), "{out}");
        assert!(!out.contains("no daemon"), "out={out} err={err}");
        assert!(
            out.contains("connected") && !out.contains("disconnected"),
            "out={out}"
        );
    })
    .await
    .expect("test timed out");
}

/// `devices` under `--transport ble` lists the configuration file and never connects to a
/// daemon, even with one answering on the socket.
///
/// The listener here only counts the connections it is offered, so the case proves the
/// socket was left alone rather than inferring it from what was printed. The configuration
/// path opens no Bluetooth transport, so nothing touches a radio.
#[tokio::test]
async fn devices_under_ble_reads_the_config_and_never_asks_a_daemon() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let home = config_home(CONFIG);
        let socket = home.0.join("svolder.sock");
        let listener = tokio::net::UnixListener::bind(&socket).expect("bind the counting socket");
        let asked = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&asked);
        let acceptor = tokio::spawn(async move {
            while let Ok((_stream, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        let config_dir = home.0.clone();
        let socket_arg = socket.clone();
        let out = tokio::task::spawn_blocking(move || {
            Command::new(env!("CARGO_BIN_EXE_svolder"))
                .env("SVOLDER_HOST", "off")
                .env("SVOLDER_ADAPTER", "hci-none")
                .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
                .env("SVOLDER_SYSTEMCTL", config_dir.join("no-systemctl"))
                .env("SVOLDER_WIREPLUMBER", config_dir.join("no-wireplumber"))
                .env("SVOLDER_UNITDIR", config_dir.join("units"))
                .env("XDG_CONFIG_HOME", &config_dir)
                .env("XDG_STATE_HOME", &config_dir)
                .env("XDG_DATA_HOME", &config_dir)
                .env("XDG_RUNTIME_DIR", &config_dir)
                .env_remove("SVOLDER_SOCKET")
                .env_remove("SVOLDER_TRANSPORT")
                .arg("--socket")
                .arg(&socket_arg)
                .args(["--transport", "ble", "devices"])
                .output()
                .expect("the binary runs")
        })
        .await
        .expect("the CLI run finished");
        acceptor.abort();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "out={stdout} err={stderr}");
        assert!(
            stdout.contains("fake  0102030405060708  no daemon"),
            "out={stdout} err={stderr}"
        );
        assert_eq!(
            asked.load(Ordering::SeqCst),
            0,
            "the daemon socket was contacted: out={stdout} err={stderr}"
        );
    })
    .await
    .expect("test timed out");
}

/// `devices` refuses a `--transport` no verb knows, in the words every other verb uses, rather
/// than listing the configuration.
#[tokio::test]
async fn devices_refuses_an_unknown_transport() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let home = config_home(CONFIG);
        let config_dir = home.0.clone();
        let out = tokio::task::spawn_blocking(move || {
            Command::new(env!("CARGO_BIN_EXE_svolder"))
                .env("SVOLDER_HOST", "off")
                .env("SVOLDER_ADAPTER", "hci-none")
                .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
                .env("SVOLDER_SYSTEMCTL", config_dir.join("no-systemctl"))
                .env("SVOLDER_WIREPLUMBER", config_dir.join("no-wireplumber"))
                .env("SVOLDER_UNITDIR", config_dir.join("units"))
                .env("XDG_CONFIG_HOME", &config_dir)
                .env("XDG_STATE_HOME", &config_dir)
                .env("XDG_DATA_HOME", &config_dir)
                .env("XDG_RUNTIME_DIR", &config_dir)
                .env_remove("SVOLDER_SOCKET")
                .env_remove("SVOLDER_TRANSPORT")
                .args(["--transport", "nope", "devices"])
                .output()
                .expect("the binary runs")
        })
        .await
        .expect("the CLI run finished");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "out={stdout} err={stderr}");
        assert!(
            stderr.contains("unknown transport \"nope\"; use auto, daemon, ble, or fake"),
            "err={stderr}"
        );
        assert!(stdout.is_empty(), "out={stdout}");
    })
    .await
    .expect("test timed out");
}

/// `devices` over a daemon with no devices configured prints the same hint the no-daemon path
/// does, rather than nothing at all.
///
/// Spawned by hand rather than through [`start_daemon_on`]: that helper waits for a device to
/// connect, which a daemon with none configured never does.
#[tokio::test]
async fn devices_over_the_daemon_with_none_configured_prints_the_hint() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = temp_path(".daemon-no-devices");
        std::fs::create_dir(&dir).expect("create the daemon's directory");
        let socket = dir.join("svolder.sock");
        let config = dir.join("config.toml");
        std::fs::write(&config, "schema = 1\n").expect("write the test config");
        let child = Command::new(&svolderd)
            .env("SVOLDER_TRANSPORT", "fake")
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
            .env("SVOLDER_SYSTEMCTL", dir.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", dir.join("no-wireplumber"))
            .env("SVOLDER_HOST", "off")
            .env("XDG_CONFIG_HOME", &dir)
            .env("XDG_STATE_HOME", &dir)
            .env("XDG_DATA_HOME", &dir)
            .env("XDG_RUNTIME_DIR", &dir)
            .env_remove("SVOLDER_MQTT_PASSWORD")
            .env_remove("SVOLDER_SOCKET")
            .arg("--socket")
            .arg(&socket)
            .arg("--config")
            .arg(&config)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn svolderd");
        let guard = ChildGuard {
            child,
            dir,
            socket: socket.clone(),
        };
        for _ in 0..500 {
            if socket.exists() && Client::connect(&socket).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let (code, out, err) = svolder(&guard.socket, &["devices"]);
        assert_eq!(code, Some(0), "out={out} err={err}");
        assert!(out.is_empty(), "{out}");
        assert!(err.contains("no devices configured"), "out={out} err={err}");
    })
    .await
    .expect("test timed out");
}

/// Taking a device out of the configuration tells the daemon to re-read the file, and the
/// answer says which devices it holds now.
///
/// The daemon reads a configuration file of its own, so what it holds afterwards is its own
/// business; what this case is about is that the running daemon was told at all, rather than
/// left holding a device the file no longer names until something restarted it.
#[tokio::test]
async fn removing_a_device_tells_the_running_daemon_to_re_read_the_file() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon(&svolderd, &[]).await;
        let home = config_home(CONFIG);
        let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
            .env("SVOLDER_SYSTEMCTL", home.0.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
            .env("SVOLDER_UNITDIR", home.0.join("units"))
            .env("XDG_CONFIG_HOME", &home.0)
            .env("XDG_STATE_HOME", &home.0)
            .env_remove("SVOLDER_TRANSPORT")
            .env_remove("SVOLDER_SOCKET")
            .arg("--socket")
            .arg(&daemon.socket)
            .args(["remove", "fake"])
            .output()
            .expect("the binary runs");
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "err={err}");
        assert!(err.contains("removed fake"), "{err}");
        assert!(err.contains("the daemon now holds"), "{err}");
    })
    .await
    .expect("test timed out");
}

/// `passphrase set --yes` over the fake transport sends function 250 without a prompt and
/// reports success.
#[test]
fn passphrase_set_with_yes_over_the_fake_exits_zero() {
    let (code, out, err) = svolder_fake(&["passphrase", "set", "5678", "--yes"]);
    assert_eq!(code, Some(0), "out={out} err={err}");
    assert!(
        out.contains("passphrase set") || err.contains("passphrase set"),
        "out={out} err={err}"
    );
}

/// Without `--yes` and with stdin not a TTY, `passphrase set` refuses rather than guessing.
#[test]
fn passphrase_set_without_yes_on_a_non_tty_stdin_refuses() {
    let (code, out, err) = svolder_fake(&["passphrase", "set", "5678"]);
    assert_eq!(code, Some(1), "out={out} err={err}");
    assert!(
        err.contains("refusing to change the passphrase without confirmation"),
        "{err}"
    );
}

/// `passphrase set none --yes` clears the passphrase rather than setting it.
#[test]
fn passphrase_set_none_clears_the_passphrase() {
    let (code, out, err) = svolder_fake(&["passphrase", "set", "none", "--yes"]);
    assert_eq!(code, Some(0), "out={out} err={err}");
    assert!(
        out.contains("passphrase cleared") || err.contains("passphrase cleared"),
        "out={out} err={err}"
    );
}

/// A code that is not four digits or `none` is refused, even with `--yes`.
#[test]
fn passphrase_set_of_a_bad_code_exits_one() {
    let (code, out, err) = svolder_fake(&["passphrase", "set", "12ab", "--yes"]);
    assert_eq!(code, Some(1), "out={out} err={err}");
}

/// Run the CLI over the in-process fake transport, with stdin closed (never a TTY) and no
/// daemon or configuration in the way.
fn svolder_fake(args: &[&str]) -> (Option<i32>, String, String) {
    let empty = TempPath(temp_path(".empty-config"));
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SYSTEMCTL", empty.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", empty.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", empty.0.join("units"))
        .env("XDG_CONFIG_HOME", &empty.0)
        .env_remove("SVOLDER_SOCKET")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A temporary `XDG_CONFIG_HOME` holding this `config.toml`, removed when the test ends.
fn config_home(text: &str) -> TempPath {
    let guard = TempPath(temp_path(".config"));
    let dir = guard.0.join("svolder");
    std::fs::create_dir_all(&dir).expect("create the config directory");
    std::fs::write(dir.join("config.toml"), text).expect("write the test config");
    guard
}

/// A temporary `XDG_CONFIG_HOME` holding one user profile under `svolder/profiles`, the
/// directory both front ends read their overrides from.
fn profile_home(name: &str, text: &str) -> TempPath {
    let guard = TempPath(temp_path(".config"));
    let dir = guard.0.join("svolder").join("profiles");
    std::fs::create_dir_all(&dir).expect("create the profiles directory");
    std::fs::write(dir.join(format!("{name}.toml")), text).expect("write the test profile");
    guard
}

/// Run the CLI against a daemon socket, and never against a radio.
///
/// The transport is left at its default, `auto`, so these cases exercise the very choice a
/// user gets. The adapter names a controller that does not exist, so a run that somehow
/// found no daemon fails at adapter lookup — a read-only D-Bus enumeration — rather than
/// scanning the host's radio.
///
/// `-v` is always passed, so the CLI names the path it took on stderr and a case can assert
/// it with [`assert_over_the_daemon`].
fn svolder(socket: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let empty = TempPath(temp_path(".empty-config"));
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_SYSTEMCTL", empty.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", empty.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", empty.0.join("units"))
        .env("XDG_CONFIG_HOME", &empty.0)
        .env("XDG_STATE_HOME", &empty.0)
        .env("XDG_DATA_HOME", &empty.0)
        .env("XDG_RUNTIME_DIR", &empty.0)
        .env_remove("SVOLDER_TRANSPORT")
        .env_remove("SVOLDER_SOCKET")
        .arg("-v")
        .arg("--socket")
        .arg(socket)
        .args(args)
        .output()
        .expect("the binary runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Assert that a run whose stderr is `err` went to the daemon on `socket`, from the path `-v`
/// names there. Every case that means to read a daemon's answer says so, so an answer from a
/// session the CLI opened for itself fails that case instead of passing as the daemon's.
fn assert_over_the_daemon(socket: &Path, err: &str) {
    assert!(
        err.contains(&format!("daemon at {}", socket.display())),
        "the run did not go to the daemon on {}: err={err}",
        socket.display()
    );
}

/// Run the CLI off the runtime, so a stub daemon on this runtime keeps answering while the
/// child process talks to it.
async fn run_svolder(socket: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let socket = socket.to_path_buf();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        svolder(
            &socket,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    })
    .await
    .expect("the CLI run finished")
}

/// A stand-in for the daemon that answers `state` with one reading and `devices` with that
/// device disconnected — the state the in-process fake cannot be put into, since its device
/// is either connected or has never been seen at all.
///
/// It speaks the wire protocol straight: one `Reply` line per `RequestLine` read.
async fn stub_daemon(path: PathBuf) -> tokio::task::JoinHandle<()> {
    stub_daemon_listing(path, true).await
}

/// [`stub_daemon`], with `listed` false for a daemon whose `devices` answer no longer carries
/// the device its `state` answer still names: the moment a device is removed between the two.
async fn stub_daemon_listing(path: PathBuf, listed: bool) -> tokio::task::JoinHandle<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let listener = tokio::net::UnixListener::bind(&path).expect("bind the stub socket");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut lines = BufReader::new(read).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(request) = serde_json::from_str::<RequestLine>(&line) else {
                        continue;
                    };
                    let response = stub_response(&request.request, listed);
                    let reply = Reply::Response {
                        id: request.id,
                        response,
                    };
                    let line = svolder_ipc::to_line(&reply).expect("serialize a reply");
                    if write.write_all(line.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    })
}

/// The stub daemon's answer to one request.
fn stub_response(request: &Request, listed: bool) -> Response {
    match request {
        Request::State { .. } => Response::Values {
            device: "fake".into(),
            values: vec![ValueObject {
                heard_at_ms: None,
                id: "10".into(),
                key: Some("volume".into()),
                label: "Volume".into(),
                value_type: "u8".into(),
                raw: serde_json::json!(109),
                display: "109 (-6.0 dB)".into(),
                value: Some(-6.021),
                unit: Some("dB".into()),
            }],
        },
        Request::Devices if !listed => Response::Devices {
            devices: Vec::new(),
        },
        Request::Devices => Response::Devices {
            devices: vec![DeviceInfo {
                signal: None,
                name: "fake".into(),
                unit: "0102030405060708".into(),
                product_id: 2,
                product: Some("Gungnir 2".into()),
                firmware: 3,
                profile: "gungnir2-v2".into(),
                connected: false,
                stale: true,
                retry_in_secs: None,
                passphrase_note: None,
                refused: false,
                recalled: None,
            }],
        },
        // Answers with the window it was asked for as the one device's unit, so a case can read
        // what reached the daemon.
        Request::Scan { window_ms } => Response::Scan {
            entries: vec![svolder_ipc::ScanEntry {
                unit: window_ms.to_string(),
                product_id: 0,
                product: None,
                firmware: 0,
                passphrase: false,
                rssi: None,
            }],
        },
        // The link is down, as the `devices` answer says.
        Request::Identify { .. } => Response::Error {
            code: 3,
            message: svolder_ipc::NOT_CONNECTED.into(),
        },
        other => Response::Error {
            code: 1,
            message: format!("the stub daemon does not answer {other:?}"),
        },
    }
}

/// A spawned `svolderd` and the temporary directory it was given, holding its configuration
/// file and its socket. Dropping it kills the child and removes the directory, so a failed
/// assertion never leaves a daemon or its files behind.
struct ChildGuard {
    child: Child,
    dir: PathBuf,
    socket: PathBuf,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Start a daemon over the fake transport and wait until its one device is connected, so a
/// state read has something to report.
///
/// Every `SVOLDER_FAKE_*` override is cleared before `envs` is applied, so a developer's own
/// shell environment cannot change the device the daemon under test talks to, and a case that
/// wants one of them still gets it.
///
/// The configuration file gets a directory of its own, because the daemon reads user profile
/// overrides from that file's directory plus `profiles/`: alongside the temporary directory
/// itself, that would be whatever anything else has left in there.
async fn start_daemon(svolderd: &Path, envs: &[(&str, &str)]) -> ChildGuard {
    start_daemon_on(svolderd, CONFIG, envs).await
}

/// [`start_daemon`] on a configuration file of the case's own, with every home the daemon
/// could reach for pointed into its own directory.
async fn start_daemon_on(svolderd: &Path, text: &str, envs: &[(&str, &str)]) -> ChildGuard {
    let dir = temp_path(".daemon");
    // Creating the directory rather than creating it if absent keeps a reused name loud.
    std::fs::create_dir(&dir).expect("create the daemon's directory");
    let socket = dir.join("svolder.sock");
    let config = dir.join("config.toml");
    std::fs::write(&config, text).expect("write the test config");
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
    for (key, value) in envs {
        command.env(key, value);
    }
    let child = command
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_SYSTEMCTL", dir.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", dir.join("no-wireplumber"))
        .env("SVOLDER_HOST", "off")
        .env("XDG_CONFIG_HOME", &dir)
        .env("XDG_STATE_HOME", &dir)
        .env("XDG_DATA_HOME", &dir)
        .env("XDG_RUNTIME_DIR", &dir)
        .env_remove("SVOLDER_MQTT_PASSWORD")
        .env_remove("SVOLDER_SOCKET")
        .arg("--socket")
        .arg(&socket)
        .arg("--config")
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn svolderd");
    let guard = ChildGuard {
        child,
        dir,
        socket: socket.clone(),
    };

    let mut client = None;
    for _ in 0..500 {
        if socket.exists()
            && let Ok(c) = Client::connect(&socket).await
        {
            client = Some(c);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut client = client.expect("the daemon came up within 5s");
    for _ in 0..500 {
        let devices = client
            .request(Request::Devices)
            .await
            .expect("a devices reply");
        if matches!(&devices, Response::Devices { devices } if devices.iter().any(|d| d.connected))
        {
            return guard;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the daemon never connected to the fake device");
}

#[test]
fn the_private_daemon_build_never_shares_the_workspace_binarys_directory() {
    let exe = Path::new("/w/target/debug/svolder");
    let dir = common::daemon_target_dir(exe);
    assert_eq!(dir, Path::new("/w/target/test-daemon"));
    assert!(!dir.starts_with(exe.parent().unwrap()));
}

#[test]
fn a_temp_dir_is_private_unique_and_removed_with_its_contents() {
    use std::os::unix::fs::PermissionsExt;
    let (a, b) = (common::TempDir::new("t"), common::TempDir::new("t"));
    assert_ne!(a.path(), b.path());
    let mode = std::fs::metadata(a.path()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    let inside = a.join("s.sock");
    std::fs::write(&inside, b"").unwrap();
    let gone = a.path().to_path_buf();
    drop(a);
    assert!(!gone.exists());
}

/// A temporary path of this test's own.
///
/// The counter is what makes the name unique: the clock alone can read the same nanosecond
/// twice for two of these tests running side by side, and a shared path would put two daemons
/// on one socket — where the second refuses to start and the first answers for both.
fn temp_path(suffix: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "svolder-backend-test-{}-{n}-{seq}{suffix}",
        std::process::id()
    ))
}

/// The config of a device whose entry is a code the fake device (code `1234`) refuses.
const REFUSED_CONFIG: &str =
    "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\npassphrase = \"9999\"\n";

/// A verb that reads a device whose passphrase is refused reports the refusal and its step with
/// exit 3 instead of printing nothing and exiting 0, whether the link is held by a daemon or by
/// a session of the verb's own.
#[tokio::test]
async fn a_read_verb_on_a_refused_device_reports_the_refusal_on_both_paths() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(TEST_TIMEOUT, async {
        let daemon = start_daemon_on(&svolderd, REFUSED_CONFIG, &[]).await;
        let (code, out, err) = run_svolder(&daemon.socket, &["status"]).await;
        assert_eq!(code, Some(3), "daemon: out={out} err={err}");
        assert!(out.is_empty(), "nothing is printed: {out}");
        assert!(err.contains("Give svolder the device's passcode"), "{err}");

        let home = config_home(REFUSED_CONFIG);
        let own = tokio::task::spawn_blocking(move || {
            let run = Command::new(env!("CARGO_BIN_EXE_svolder"))
                .env("SVOLDER_HOST", "off")
                .env("SVOLDER_TRANSPORT", "fake")
                .env("SVOLDER_SYSTEMCTL", home.0.join("no-systemctl"))
                .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
                .env("SVOLDER_UNITDIR", home.0.join("units"))
                .env("SVOLDER_SOCKET", home.0.join("no-daemon.sock"))
                .env("XDG_CONFIG_HOME", &home.0)
                .env("XDG_STATE_HOME", &home.0)
                .env("XDG_DATA_HOME", &home.0)
                .env_remove("SVOLDER_FAKE_PASSPHRASE")
                .arg("status")
                .output()
                .expect("the binary runs");
            (
                run.status.code(),
                String::from_utf8_lossy(&run.stdout).into_owned(),
                String::from_utf8_lossy(&run.stderr).into_owned(),
            )
        })
        .await
        .expect("the run finished");
        assert_eq!(own.0, Some(3), "direct: out={} err={}", own.1, own.2);
        assert!(own.1.is_empty(), "nothing is printed: {}", own.1);
        assert!(
            own.2.contains("Give svolder the device's passcode"),
            "{}",
            own.2
        );
    })
    .await
    .expect("test timed out");
}
