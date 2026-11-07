// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder host eq` and the questions that keep one correction from standing on the DAC and
//! the computer at once.
//!
//! Every run is in a scratch home of its own, with fake `systemctl`, `pipewire` and `wpctl`
//! programs that log what they were asked into it: the real session, the real units and the
//! user's configuration are never reached. The two cases that need a device the same across
//! calls spawn a `svolderd` over the fake transport, as `eq_presets.rs` does.

use std::io::Write;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use svolder_ipc::{Client, Request, Response};

mod common;

use common::TempPath;

/// A `systemctl` that logs and knows the one unit by its file.
const SYSTEMCTL: &str = r#"echo "$*" >> "$D/systemctl.log"
case "$2" in
show)
  if [ -f "$D/units/svolder-host-eq.service" ]; then echo LoadState=loaded; echo ActiveState=active; else echo LoadState=not-found; echo ActiveState=inactive; fi ;;
esac
exit 0
"#;

/// A `pipewire` that stays up until it is told to stop, its home is removed, or the test run
/// that started it (`SVHE_TEST_PID`) is gone.
const PIPEWIRE: &str = r#"echo "$$ $*" >> "$D/pipewire.log"
trap 'exit 0' TERM
while [ -d "$D" ] && { [ -z "$SVHE_TEST_PID" ] || kill -0 "$SVHE_TEST_PID" 2>/dev/null; }; do sleep 0.1; done
"#;

/// A `wireplumber` that prints its version banner.
const WIREPLUMBER: &str = r#"printf 'wireplumber\nCompiled with libwireplumber 0.5.13\nLinked with libwireplumber 0.5.13\n'
"#;

/// A `wpctl` with a DAC as the default output and the chain's own sink.
const WPCTL: &str = r#"echo "$*" >> "$D/wpctl.log"
case "$1" in
--version) echo "Error: Unknown option --version"; exit 1;;
status) printf 'Audio\n ├─ Sinks:\n │  *   42. DAC [vol: 1.00]\n │      44. svolder EQ [vol: 1.00]\n';;
inspect)
  case "$2" in
    @DEFAULT_AUDIO_SINK@|42) printf 'id 42, type PipeWire:Interface:Node\n  * media.class = "Audio/Sink"\n  * node.name = "dac"\n';;
    44) printf 'id 44, type PipeWire:Interface:Node\n  * media.class = "Audio/Sink"\n  * node.name = "svolder_eq"\n';;
    *) exit 1;;
  esac;;
esac
exit 0
"#;

/// Where the fake programs live: written once, before any test runs one (a script written
/// while another thread forks can be busy when it is first run), and linked into each home.
fn programs() -> &'static Path {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        sweep_finished_homes(&std::env::temp_dir());
        for entry in std::fs::read_dir(env!("CARGO_TARGET_TMPDIR"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(pid) = name.strip_prefix("svhe-cli-programs-")
                && !Path::new(&format!("/proc/{pid}")).exists()
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("svhe-cli-programs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the programs directory");
        for (name, body) in [
            ("systemctl", SYSTEMCTL),
            ("pipewire", PIPEWIRE),
            ("wpctl", WPCTL),
            ("wireplumber", WIREPLUMBER),
        ] {
            let path = dir.join(name);
            common::install_script(&path, &format!("#!/bin/sh\nD=$(dirname \"$0\")\n{body}"));
        }
        dir
    })
}

/// Stops the fake pipewire in every `svhe-cli-<pid>-<n>` home of `tmp` whose run is over (no
/// `/proc/<pid>`) and removes the home. Only a real directory owned by this user is touched; a
/// symlink is never followed.
fn sweep_finished_homes(tmp: &Path) {
    use std::os::unix::fs::MetadataExt;
    let Some(uid) = std::fs::metadata("/proc/self").ok().map(|m| m.uid()) else {
        return;
    };
    for entry in std::fs::read_dir(tmp).into_iter().flatten().flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_prefix("svhe-cli-"))
            .and_then(|rest| rest.split_once('-'))
            .and_then(|(pid, _)| pid.parse::<u32>().ok())
            .filter(|pid| *pid > 0)
        else {
            continue;
        };
        let ours =
            std::fs::symlink_metadata(entry.path()).is_ok_and(|m| m.is_dir() && m.uid() == uid);
        if ours && !Path::new("/proc").join(pid.to_string()).exists() {
            stop_recorded_pipewire(&entry.path());
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Stops the pipewire the pid file of the home `dir` names, only when it is a process of this
/// user whose command line shows both the recorded configuration and the fake `pipewire` of
/// this very home.
fn stop_recorded_pipewire(dir: &Path) {
    use std::os::unix::fs::MetadataExt;
    let Ok(text) = std::fs::read_to_string(dir.join("svolder-host-eq.pid")) else {
        return;
    };
    let mut lines = text.lines();
    let (Some(pid), Some(conf)) = (
        lines.next().and_then(|l| l.parse::<i32>().ok()),
        lines.nth(1),
    ) else {
        return;
    };
    if pid <= 1 || conf.is_empty() {
        return;
    }
    let ours = std::fs::metadata("/proc/self")
        .and_then(|me| std::fs::metadata(format!("/proc/{pid}")).map(|p| p.uid() == me.uid()))
        .unwrap_or(false);
    let fake = dir.join("pipewire");
    let shows = |raw: &[u8]| {
        let mut args = raw.split(|b| *b == 0);
        args.clone().any(|arg| arg == conf.as_bytes())
            && args.any(|arg| arg == fake.as_os_str().as_encoded_bytes())
    };
    if ours && std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|raw| shows(&raw)) {
        // SAFETY: `kill` takes a pid and a signal; the command line says the pid is the fake
        // pipewire this home started.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }
}

/// A scratch home, its fake programs, and optionally a daemon on the fake transport.
struct Home {
    dir: TempPath,
    daemon: Option<Child>,
}

impl Home {
    /// A home with nothing in it. Its path is short: the runtime directory is the home itself
    /// and a socket path may not pass 108 bytes.
    fn new() -> Home {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "svhe-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // The runtime directory is private to its user, as the real one is.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["systemctl", "pipewire", "wpctl", "wireplumber"] {
            std::os::unix::fs::symlink(programs().join(name), dir.join(name)).unwrap();
        }
        Home {
            dir: TempPath(dir),
            daemon: None,
        }
    }

    fn path(&self) -> &Path {
        &self.dir.0
    }

    fn command(&self, args: &[&str]) -> Command {
        let home = self.path();
        let mut command = Command::new(env!("CARGO_BIN_EXE_svolder"));
        command
            .env_remove("SVOLDER_HOST")
            .env("SVHE_TEST_PID", std::process::id().to_string())
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
            .env("SVOLDER_SYSTEMCTL", home.join("systemctl"))
            .env("SVOLDER_PIPEWIRE", home.join("pipewire"))
            .env("SVOLDER_WPCTL", home.join("wpctl"))
            .env("SVOLDER_WIREPLUMBER", home.join("wireplumber"))
            .env("SVOLDER_HOST_EQ_MANAGER", "systemd")
            .env("SVOLDER_UNITDIR", home.join("units"))
            .env("XDG_CACHE_HOME", home)
            .env("XDG_CONFIG_HOME", home)
            .env("XDG_RUNTIME_DIR", home)
            .env("XDG_STATE_HOME", home);
        match self.daemon {
            Some(_) => command
                .env("SVOLDER_TRANSPORT", "daemon")
                .arg("--socket")
                .arg(home.join("svolder.sock"))
                .env_remove("SVOLDER_SOCKET"),
            None => command
                .env("SVOLDER_TRANSPORT", "fake")
                .env("SVOLDER_SOCKET", home.join("no-daemon.sock")),
        };
        command.args(args);
        command
    }

    /// Run the CLI with no terminal on its input.
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        self.run_with(args, &[])
    }

    /// [`Home::run`] with more of the environment set.
    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
        let mut command = self.command(args);
        command.envs(env.iter().copied());
        let out = command.output().expect("the binary runs");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// Run the CLI with a terminal on its input, and `answer` typed on it.
    fn run_on_terminal(&self, args: &[&str], answer: &str) -> (i32, String, String) {
        let (mut master, slave) = {
            let (mut master, mut slave) = (0, 0);
            // SAFETY: `openpty` fills the two descriptors it is given and nothing else.
            let made = unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            assert_eq!(made, 0, "a pseudo-terminal opens");
            // SAFETY: both descriptors are new and owned by nobody else.
            unsafe {
                (
                    std::fs::File::from_raw_fd(master),
                    OwnedFd::from_raw_fd(slave),
                )
            }
        };
        let mut command = self.command(args);
        command
            .stdin(Stdio::from(slave))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn().expect("the binary runs");
        drop(command);
        master
            .write_all(answer.as_bytes())
            .expect("the answer is typed");
        let out = child.wait_with_output().expect("the binary ends");
        drop(master);
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn exists(&self, relative: &str) -> bool {
        self.path().join(relative).exists()
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.path().join(relative)).unwrap_or_default()
    }

    fn write_config(&self, text: &str) {
        let dir = self.path().join("svolder");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), text).unwrap();
    }

    /// A `svolderd` over the fake transport on this home's configuration, with one device.
    async fn start_daemon(&mut self, svolderd: &Path) {
        self.start_daemon_on(svolderd, "off").await;
    }

    /// [`Home::start_daemon`] with the host `SVOLDER_HOST` names: `fake` serves a fake computer.
    async fn start_daemon_on(&mut self, svolderd: &Path, host: &str) {
        self.start_daemon_env(svolderd, host, &[]).await;
    }

    /// [`Home::start_daemon_on`] with `env` set on the daemon.
    async fn start_daemon_env(&mut self, svolderd: &Path, host: &str, env: &[(&str, &str)]) {
        self.write_config("schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n");
        let socket = self.path().join("svolder.sock");
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
            .env("SVOLDER_HOST", host)
            .envs(env.iter().copied())
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_TRANSPORT", "fake")
            .env("XDG_CACHE_HOME", self.path())
            .env("XDG_CONFIG_HOME", self.path())
            .env("XDG_DATA_HOME", self.path())
            .env("XDG_RUNTIME_DIR", self.path())
            .env("XDG_STATE_HOME", self.path())
            .env_remove("SVOLDER_SOCKET")
            .arg("--socket")
            .arg(&socket)
            .arg("--config")
            .arg(self.path().join("svolder/config.toml"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn svolderd");
        self.daemon = Some(child);
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
            if let Ok(Response::Devices { devices }) = client.request(Request::Devices).await
                && devices.iter().any(|d| d.connected)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the daemon never connected to the fake device");
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        if let Some(daemon) = &mut self.daemon {
            let _ = daemon.kill();
            let _ = daemon.wait();
        }
        stop_recorded_pipewire(&self.dir.0);
    }
}

const PEQ: &str = "Preamp: -6.1 dB\r\n\
    Filter 1: ON LSC Fc 105 Hz Gain 6.4 dB Q 0.70\r\n\
    Filter 2: ON PK Fc 8800 Hz Gain 5.1 dB Q 1.42\r\n";

/// Import `PEQ` as the preset `name` into `home`.
fn import(home: &Home, name: &str) {
    let file = home.path().join("import.txt");
    std::fs::write(&file, PEQ).unwrap();
    let (code, _out, err) = home.run(&["eq", "import", file.to_str().unwrap(), "--name", name]);
    assert_eq!(code, 0, "{err}");
}

/// What the systemd fake was asked to change.
fn systemctl_writes(home: &Home) -> Vec<String> {
    home.read("systemctl.log")
        .lines()
        .filter(|line| !line.starts_with("--user show"))
        .map(str::to_string)
        .collect()
}

#[test]
fn applying_an_imported_preset_writes_the_conf_and_the_unit_and_says_what_is_on() {
    let home = Home::new();
    import(&home, "My Phones");
    let (code, out, err) = home.run(&["host", "eq", "apply", "My Phones"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(
        err.contains("host EQ: My Phones · on · in front of dac · systemd"),
        "{err}"
    );
    // The file's own preamp (-6.1 dB) is below the low shelf's +6.4 dB, so it is lowered.
    assert!(
        err.contains("preamp lowered from -6.1 dB to -6.4 dB so the correction never clips"),
        "{err}"
    );
    let conf = home.read("svolder/host-eq.conf");
    // The live chain: a preamp at the lowered gain (10^(-6.4/20)) and sixteen raw slots a side.
    assert!(
        conf.contains("name = lpre label = linear control = { Mult = 0.4786"),
        "{conf}"
    );
    assert_eq!(conf.matches("label = bq_raw").count(), 32, "{conf}");
    let state = home.read("svolder/host-eq.toml");
    let slots = state
        .split("slots = [")
        .nth(1)
        .and_then(|rest| rest.split(']').next());
    let slots: Vec<&str> = slots
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(slots, ["1", "2"], "{state}");
    assert!(
        conf.contains("filter.smart.target = { node.name = \"dac\" }"),
        "{conf}"
    );
    let unit = home.read("units/svolder-host-eq.service");
    assert!(unit.contains("ExecStart="), "{unit}");
    assert_eq!(
        systemctl_writes(&home),
        [
            "--user daemon-reload",
            "--user enable svolder-host-eq.service",
            "--user restart svolder-host-eq.service"
        ]
    );
    let state = home.read("svolder/host-eq.toml");
    assert!(
        state.contains("on = true") && state.contains("name = \"My Phones\""),
        "{state}"
    );

    let (code, out, _err) = home.run(&["host", "eq", "status"]);
    assert_eq!(code, 0);
    assert_eq!(out, "host EQ: My Phones · on · in front of dac · systemd\n");
    let (code, out, _err) = home.run(&["host", "eq", "status", "--json"]);
    assert_eq!(code, 0);
    let json: serde_json::Value = serde_json::from_str(&out).expect("one JSON object");
    assert_eq!(json["on"], true);
    assert_eq!(json["name"], "My Phones");
    assert_eq!(json["target"], "dac");
    assert_eq!(json["route"], "smart");
    assert_eq!(json["manager"], "systemd");
    assert_eq!(json["preamp_db"], -6.4);
    assert_eq!(json["filters"].as_array().unwrap().len(), 2);
}

#[test]
fn a_band_preset_applies_its_bands_as_peaking_filters_with_the_preamp_its_peak_needs() {
    let home = Home::new();
    home.write_config(
        "schema = 1\n[[preset]]\nname = \"Mine\"\nbands = 3\nmode = \"eq\"\nloudness = 0\nband = [\n  \
         { f = 63.0, g = 4.0, q = 0.7 },\n  { f = 1000.0, g = 0.0, q = 1.0 },\n  \
         { f = 8000.0, g = -2.0, q = 0.7 },\n]\n",
    );
    let (code, _out, err) = home.run(&["host", "eq", "apply", "mine"]);
    assert_eq!(code, 0, "{err}");
    let conf = home.read("svolder/host-eq.conf");
    assert_eq!(conf.matches("label = bq_raw").count(), 32, "{conf}");
    let mult: f64 = conf
        .lines()
        .find(|line| line.contains("name = lpre "))
        .and_then(|line| line.split("Mult = ").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|number| number.parse().ok())
        .expect("a preamp line");
    let preamp = 20.0 * mult.log10();
    assert!((-4.6..=-3.9).contains(&preamp), "{preamp}");
    assert!(
        home.read("svolder/host-eq.toml")
            .contains("name = \"Mine\"")
    );
}

#[test]
fn a_band_preset_the_dac_keeps_flat_is_refused_for_the_computer_with_the_reason() {
    for mode in ["flat", "loudness"] {
        let home = Home::new();
        home.write_config(&format!(
            "schema = 1\n[[preset]]\nname = \"Mine\"\nbands = 3\nmode = \"{mode}\"\nloudness = 0\nband = [\n  \
             {{ f = 63.0, g = 4.0, q = 0.7 }},\n  {{ f = 1000.0, g = 0.0, q = 1.0 }},\n  \
             {{ f = 8000.0, g = -2.0, q = 0.7 }},\n]\n"
        ));
        let (code, _out, err) = home.run(&["host", "eq", "apply", "mine"]);
        assert_eq!(code, 1, "{err}");
        assert!(
            err.contains(&format!("Mine is a {mode} setting")) && err.contains("no curve"),
            "{err}"
        );
        assert!(!home.exists("svolder/host-eq.conf"));
    }
}

#[test]
fn a_file_with_a_boosting_preamp_is_applied_at_unity_and_says_so() {
    let home = Home::new();
    let file = home.path().join("Boost ParametricEQ.txt");
    std::fs::write(
        &file,
        "Preamp: +6 dB\nFilter 1: ON PK Fc 1000 Hz Gain -3 dB Q 1\n",
    )
    .unwrap();
    let (code, _out, err) = home.run(&["host", "eq", "apply", file.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains("preamp lowered from 6.0 dB to 0.0 dB so the correction never clips"),
        "{err}"
    );
    assert!(
        home.read("svolder/host-eq.toml")
            .contains("preamp_db = 0.0")
    );
}

#[test]
fn a_file_is_applied_under_its_name_and_saves_nothing() {
    let home = Home::new();
    let file = home.path().join("My Cans ParametricEQ.txt");
    std::fs::write(&file, PEQ).unwrap();
    let (code, _out, err) = home.run(&["host", "eq", "apply", file.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("host EQ: My Cans · on"), "{err}");
    assert!(!home.exists("svolder/config.toml"), "no preset was saved");
}

#[test]
fn remove_deletes_what_apply_wrote_and_status_remembers_the_last() {
    let home = Home::new();
    import(&home, "My Phones");
    home.run(&["host", "eq", "apply", "My Phones"]);
    let (code, _out, err) = home.run(&["host", "eq", "remove"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("host EQ removed"), "{err}");
    assert!(!home.exists("svolder/host-eq.conf"));
    assert!(!home.exists("units/svolder-host-eq.service"));
    assert!(
        systemctl_writes(&home)
            .iter()
            .any(|l| l.contains("disable svolder-host-eq"))
    );
    let (_code, out, _err) = home.run(&["host", "eq", "status"]);
    assert_eq!(out, "host EQ: off (last: My Phones)\n");

    let (code, _out, err) = home.run(&["host", "eq", "remove"]);
    assert_eq!(code, 0);
    assert!(err.contains("host EQ was not on"), "{err}");
}

#[test]
fn status_with_nothing_applied_says_off() {
    let home = Home::new();
    let (code, out, _err) = home.run(&["host", "eq", "status"]);
    assert_eq!((code, out.as_str()), (0, "host EQ: off\n"));
    let (_code, out, _err) = home.run(&["host", "eq", "status", "--json"]);
    assert_eq!(out, "{\"on\":false,\"running\":null}\n");
}

/// A built-in goes to the computer as its bands, like a saved curve of bands.
#[test]
fn a_built_in_goes_to_the_computer_as_its_bands() {
    let home = Home::new();
    let (code, _out, err) = home.run(&["host", "eq", "apply", "Warm"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("host EQ: Warm"), "{err}");
    assert!(home.exists("svolder/host-eq.conf"));
}

/// An Equalizer APO file of `count` peaking filters.
fn peq_of(count: usize) -> String {
    (1..=count).fold("Preamp: -3.0 dB\r\n".to_string(), |text, n| {
        format!(
            "{text}Filter {n}: ON PK Fc {} Hz Gain 1.0 dB Q 1.00\r\n",
            100 * n
        )
    })
}

/// More than 16 filters are refused by name, from a file and from a saved preset, and
/// nothing is written; 16 are applied.
#[test]
fn seventeen_filters_are_refused_by_name_and_sixteen_are_applied() {
    let home = Home::new();
    let long = home.path().join("Long.txt");
    std::fs::write(&long, peq_of(17)).unwrap();
    let (code, _out, err) = home.run(&["host", "eq", "apply", long.to_str().unwrap()]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("17 filters; the computer EQ holds 16"),
        "{err}"
    );
    let (code, _out, err) = home.run(&[
        "eq",
        "import",
        long.to_str().unwrap(),
        "--name",
        "Long preset",
    ]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = home.run(&["host", "eq", "apply", "Long preset"]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("17 filters; the computer EQ holds 16"),
        "{err}"
    );
    assert!(!home.exists("svolder/host-eq.conf"));
    assert!(!home.exists("svolder/host-eq.toml"));

    let sixteen = home.path().join("Sixteen.txt");
    std::fs::write(&sixteen, peq_of(16)).unwrap();
    let (code, _out, err) = home.run(&["host", "eq", "apply", sixteen.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
}

/// A built-in on the DAC is asked about like a saved preset: refused without an answer, flattened
/// on a yes.
#[tokio::test]
async fn a_built_in_on_the_dac_is_moved_to_the_computer_only_when_the_user_says_yes() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut home = Home::new();
        home.start_daemon(&svolderd).await;
        let (code, _out, err) = home.run(&["eq", "load", "Warm"]);
        assert_eq!(code, 0, "{err}");

        let (code, _out, err) = home.run(&["host", "eq", "apply", "Warm"]);
        assert_eq!(code, 1, "{err}");
        assert!(err.contains("--yes"), "{err}");
        assert!(!home.exists("svolder/host-eq.conf"));

        let (code, _out, err) = home.run_on_terminal(&["host", "eq", "apply", "Warm"], "y\n");
        assert_eq!(code, 0, "{err}");
        assert!(
            err.contains("Warm is on the DAC. Move it to the computer? [y/N]"),
            "{err}"
        );
        assert!(home.exists("svolder/host-eq.conf"));
        let (_code, out, _err) = home.run(&["eq", "show"]);
        assert!(out.contains("preset Flat"), "the DAC is flat: {out}");
    })
    .await
    .expect("the case finishes in time");
}

/// A curve that changes nothing cannot play twice: with the DAC on Flat, `Flat` goes to the
/// computer with no question.
#[tokio::test]
async fn flat_goes_to_the_computer_with_no_question_while_the_dac_is_flat() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut home = Home::new();
        home.start_daemon(&svolderd).await;
        let (code, _out, err) = home.run(&["eq", "load", "Flat"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = home.run(&["host", "eq", "apply", "Flat"]);
        assert_eq!(code, 0, "{err}");
        assert!(!err.contains("[y/N]"), "{err}");
        assert!(home.exists("svolder/host-eq.conf"));
    })
    .await
    .expect("the case finishes in time");
}

/// The other direction: `eq load Flat` while the computer is on Flat asks nothing and loads.
#[tokio::test]
async fn flat_goes_to_the_dac_with_no_question_while_the_computer_is_flat() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut home = Home::new();
        home.start_daemon(&svolderd).await;
        let (code, _out, err) = home.run(&["host", "eq", "apply", "Flat"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = home.run(&["eq", "load", "Flat"]);
        assert_eq!(code, 0, "{err}");
        assert!(!err.contains("[y/N]"), "{err}");
        assert!(err.contains("loaded Flat"), "{err}");
    })
    .await
    .expect("the case finishes in time");
}

#[test]
fn refusals_name_what_is_wrong_and_write_nothing() {
    let home = Home::new();
    import(&home, "My Phones");
    let (code, _out, err) = home.run(&["host", "eq", "apply", "nosuch"]);
    assert_eq!(code, 1);
    assert!(err.contains("no preset named"), "{err}");
    let (code, _out, err) = home.run_with(
        &["host", "eq", "apply", "My Phones"],
        &[("SVOLDER_HOST", "off")],
    );
    assert_eq!(code, 1);
    assert!(err.contains("SVOLDER_HOST=off"), "{err}");
    let (code, _out, err) = home.run_with(
        &["host", "eq", "apply", "My Phones"],
        &[("SVOLDER_PIPEWIRE", "/nonexistent/pipewire")],
    );
    assert_eq!(code, 1);
    assert!(err.contains("PipeWire"), "{err}");
    assert!(!home.exists("svolder/host-eq.conf"));
    assert!(!home.exists("svolder/host-eq.toml"));
    assert!(!home.exists("units"));
    assert_eq!(home.read("systemctl.log"), "");
}

#[test]
fn a_process_is_started_detached_with_an_autostart_entry_and_stopped_by_remove() {
    let home = Home::new();
    import(&home, "My Phones");
    let process = [("SVOLDER_HOST_EQ_MANAGER", "process")];
    let (code, _out, err) = home.run_with(&["host", "eq", "apply", "My Phones"], &process);
    assert_eq!(code, 0, "{err}");
    assert!(home.exists("autostart/svolder-host-eq.desktop"));
    assert!(!home.exists("units"));
    let pid: i32 = home
        .read("svolder-host-eq.pid")
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(Path::new(&format!("/proc/{pid}")).exists());
    let (_code, out, _err) = home.run_with(&["host", "eq", "status"], &process);
    assert_eq!(out, "host EQ: My Phones · on · in front of dac · process\n");

    let (code, _out, err) = home.run_with(&["host", "eq", "remove"], &process);
    assert_eq!(code, 0, "{err}");
    for _ in 0..60 {
        if !std::fs::read_to_string(format!("/proc/{pid}/cmdline")).is_ok_and(|c| c.contains("-c"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !std::fs::read_to_string(format!("/proc/{pid}/cmdline")).is_ok_and(|c| c.contains("-c")),
        "the process stopped"
    );
    assert!(!home.exists("autostart/svolder-host-eq.desktop"));
    assert!(!home.exists("svolder-host-eq.pid"));
    assert!(!home.exists("svolder/host-eq.conf"));
}

// ---- the questions ----

/// The computer EQ the daemon on this home shows.
async fn daemon_eq(home: &Home) -> svolder_host::eq::EqView {
    let mut client = Client::connect(&home.path().join("svolder.sock"))
        .await
        .expect("the daemon answers");
    match client.request(Request::Host).await.expect("an answer") {
        Response::Host(view) => view.eq.expect("the daemon serves the computer EQ"),
        other => panic!("not a host answer: {other:?}"),
    }
}

/// With a daemon that serves the computer, apply and remove go to it: the command writes no
/// config, unit or state of its own, and runs nothing.
#[tokio::test]
async fn host_eq_apply_goes_through_a_running_daemon() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut home = Home::new();
        home.start_daemon_on(&svolderd, "fake").await;
        import(&home, "My Phones");

        let (code, _out, err) = home.run(&["host", "eq", "apply", "My Phones"]);
        assert_eq!(code, 0, "{err}");
        assert!(err.contains("My Phones"), "{err}");
        let eq = daemon_eq(&home).await;
        assert!(eq.on, "the daemon applied it");
        assert_eq!(eq.preset.as_deref(), Some("My Phones"));
        assert_eq!(eq.points.len(), 2);
        assert!(
            !home.exists("svolder/host-eq.conf") && !home.exists("units"),
            "the command wrote nothing of its own"
        );
        assert!(systemctl_writes(&home).is_empty());
        assert!(home.read("pipewire.log").is_empty(), "and ran no chain");

        let (code, _out, err) = home.run(&["host", "eq", "remove"]);
        assert_eq!(code, 0, "{err}");
        assert!(err.contains("host EQ removed"), "{err}");
        let eq = daemon_eq(&home).await;
        assert!(!eq.on);
        assert_eq!(eq.points.len(), 2, "off keeps the points");
        assert!(!home.exists("svolder/host-eq.conf"));

        // A refusal is the daemon's, in its words, with the exit code of a failure.
        let (code, _out, err) = home.run(&["host", "eq", "apply", "No Such Preset"]);
        assert_eq!(code, 1, "{err}");
    })
    .await
    .expect("the case finishes in time");
}

/// A daemon that shows the computer but cannot control it (no `pw-cli`) answers with the code of
/// an unavailable computer, and the command leaves with it.
#[tokio::test]
async fn an_unavailable_computer_exits_6() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut home = Home::new();
        home.start_daemon_env(&svolderd, "fake", &[("SVOLDER_FAKE_HOST_EQ", "off")])
            .await;
        let (code, _out, err) = home.run(&["host", "eq", "remove"]);
        assert_eq!(code, 6, "{err}");
        assert!(
            err.contains("computer controls unavailable: no pw-cli"),
            "{err}"
        );
    })
    .await
    .expect("the case finishes in time");
}

/// A built-in is shaped to the shown device's own tone control when a daemon answers, and goes
/// to the daemon like any other curve.
#[tokio::test]
async fn a_built_in_goes_to_the_daemon_shaped_to_the_shown_device() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut home = Home::new();
        home.start_daemon_on(&svolderd, "fake").await;
        let (code, _out, err) = home.run(&["host", "eq", "apply", "Warm"]);
        assert_eq!(code, 0, "{err}");
        let eq = daemon_eq(&home).await;
        assert!(eq.on);
        assert_eq!(eq.preset.as_deref(), Some("Warm"));
        assert!(!home.exists("svolder/host-eq.conf"));
    })
    .await
    .expect("the case finishes in time");
}

/// With no daemon the command applies it itself, as it always did.
#[test]
fn with_no_daemon_it_applies_itself() {
    let home = Home::new();
    import(&home, "My Phones");
    let (code, _out, err) = home.run(&["host", "eq", "apply", "My Phones"]);
    assert_eq!(code, 0, "{err}");
    assert!(home.exists("svolder/host-eq.conf"));
    assert!(home.exists("svolder/host-eq.toml"));
    let (code, _out, err) = home.run(&["host", "eq", "remove"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("host EQ removed"), "{err}");
}

#[test]
fn with_no_daemon_applying_asks_nothing() {
    let home = Home::new();
    import(&home, "My Phones");
    // The fake transport reaches no daemon, so there is no device to ask about and no terminal
    // is needed.
    let (code, _out, err) = home.run(&["host", "eq", "apply", "My Phones"]);
    assert_eq!(code, 0, "{err}");
    assert!(!err.contains("[y/N]"), "{err}");
}

#[test]
fn with_no_daemon_on_the_auto_transport_applying_asks_nothing_and_starts_nothing() {
    let home = Home::new();
    import(&home, "My Phones");
    // `auto` looks for a daemon on the socket and finds none: no question, no scan and no
    // daemon is started to find out.
    let (code, _out, err) = home.run_with(
        &["host", "eq", "apply", "My Phones"],
        &[("SVOLDER_TRANSPORT", "auto")],
    );
    assert_eq!(code, 0, "{err}");
    assert!(!err.contains("[y/N]"), "{err}");
    assert!(!home.exists("no-daemon.sock"));
}

/// Shape the daemon's device with a curve of no built-in's and save it as `Mine`, so `Mine`
/// is on the DAC.
fn put_mine_on_the_dac(home: &Home) {
    let steps: [&[&str]; 3] = [
        &["eq", "mode", "eq"],
        &["eq", "set", "1", "gain", "4.0"],
        &["eq", "set", "3", "gain", "-2.0"],
    ];
    for args in steps {
        let (code, _out, err) = home.run(args);
        assert_eq!(code, 0, "{err}");
    }
    let (code, _out, err) = home.run(&["eq", "save", "Mine"]);
    assert_eq!(code, 0, "{err}");
    let (_code, out, _err) = home.run(&["eq", "show"]);
    assert!(out.contains("preset Mine"), "{out}");
}

#[tokio::test]
async fn a_preset_on_the_dac_is_moved_to_the_computer_only_when_the_user_says_yes() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut home = Home::new();
        home.start_daemon(&svolderd).await;
        put_mine_on_the_dac(&home);

        // No terminal and no --yes: refused, naming the flag, nothing changed.
        let (code, _out, err) = home.run(&["host", "eq", "apply", "Mine"]);
        assert_eq!(code, 1, "{err}");
        assert!(err.contains("--yes"), "{err}");
        assert!(!home.exists("svolder/host-eq.conf"));

        // A no, and an Enter, change nothing and exit 0.
        for answer in ["n\n", "\n"] {
            let (code, _out, err) = home.run_on_terminal(&["host", "eq", "apply", "Mine"], answer);
            assert_eq!(code, 0, "{answer:?}: {err}");
            assert!(
                err.contains("Mine is on the DAC. Move it to the computer? [y/N]"),
                "{err}"
            );
            assert!(err.contains("nothing changed"), "{err}");
            assert!(!home.exists("svolder/host-eq.conf"));
            let (_code, out, _err) = home.run(&["eq", "show"]);
            assert!(out.contains("preset Mine"), "the DAC kept it: {out}");
        }

        // A yes flattens the DAC, then applies on the computer.
        let (code, _out, err) = home.run_on_terminal(&["host", "eq", "apply", "Mine"], "y\n");
        assert_eq!(code, 0, "{err}");
        assert!(err.contains("flattened"), "{err}");
        assert!(err.contains("host EQ: Mine · on"), "{err}");
        assert!(home.exists("svolder/host-eq.conf"));
        let (_code, out, _err) = home.run(&["eq", "show"]);
        assert!(out.contains("preset Flat"), "the DAC is flat: {out}");

        // --yes answers without a terminal, and with nothing on the DAC no question is asked.
        let (code, _out, err) = home.run(&["host", "eq", "apply", "Mine"]);
        assert_eq!(code, 0, "{err}");
        assert!(!err.contains("[y/N]"), "{err}");
    })
    .await
    .expect("the case finishes in time");
}

#[tokio::test]
async fn yes_moves_a_preset_from_the_dac_to_the_computer_and_back() {
    let svolderd = common::daemon_binary();
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut home = Home::new();
        home.start_daemon(&svolderd).await;
        put_mine_on_the_dac(&home);
        let (code, _out, err) = home.run(&["host", "eq", "apply", "Mine", "--yes"]);
        assert_eq!(code, 0, "{err}");
        let (_code, out, _err) = home.run(&["eq", "show"]);
        assert!(out.contains("preset Flat"), "{out}");
        assert!(home.read("svolder/host-eq.toml").contains("on = true"));

        // A name no preset has, or a device that cannot be reached, leaves the computer EQ on.
        let (code, _out, err) = home.run(&["eq", "load", "No Such Preset", "--yes"]);
        assert_eq!(code, 1, "{err}");
        assert!(home.read("svolder/host-eq.toml").contains("on = true"));
        assert!(home.exists("svolder/host-eq.conf"));

        // Back: without a terminal and without --yes it refuses, naming the flag.
        let (code, _out, err) = home.run(&["eq", "load", "Mine"]);
        assert_eq!(code, 1, "{err}");
        assert!(err.contains("--yes"), "{err}");
        assert!(home.read("svolder/host-eq.toml").contains("on = true"));

        let (code, _out, err) = home.run(&["eq", "load", "Mine", "--yes"]);
        assert_eq!(code, 0, "{err}");
        assert!(err.contains("loaded Mine"), "{err}");
        assert!(home.read("svolder/host-eq.toml").contains("on = false"));
        assert!(!home.exists("svolder/host-eq.conf"));
        let (_code, out, _err) = home.run(&["eq", "show"]);
        assert!(out.contains("preset Mine"), "{out}");

        // A preset of another band count cannot be recalled, and so is not taken off the
        // computer for it.
        let config = home.path().join("svolder/config.toml");
        let mut text = std::fs::read_to_string(&config).unwrap();
        text.push_str(
            "[[preset]]\nname = \"Three\"\nbands = 3\nmode = \"eq\"\nloudness = 0\nband = [\n  \
             { f = 63.0, g = 1.0, q = 0.7 },\n  { f = 1000.0, g = 0.0, q = 1.0 },\n  \
             { f = 8000.0, g = -2.0, q = 0.7 },\n]\n",
        );
        std::fs::write(&config, text).unwrap();
        let (code, _out, err) = home.run(&["host", "eq", "apply", "Three", "--yes"]);
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = home.run(&["eq", "load", "Three", "--yes"]);
        assert_eq!(code, 1, "{err}");
        assert!(home.read("svolder/host-eq.toml").contains("on = true"));
        assert!(home.exists("svolder/host-eq.conf"));
    })
    .await
    .expect("the case finishes in time");
}

#[test]
fn loading_a_preset_that_is_on_the_computer_asks_to_move_it() {
    let home = Home::new();
    import(&home, "My Phones");
    home.run(&["host", "eq", "apply", "My Phones"]);

    let (code, _out, err) = home.run(&["eq", "load", "My Phones"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("--yes"), "{err}");
    assert!(home.exists("svolder/host-eq.conf"));

    for answer in ["n\n", "\n"] {
        let (code, _out, err) = home.run_on_terminal(&["eq", "load", "my phones"], answer);
        assert_eq!(code, 0, "{answer:?}: {err}");
        assert!(
            err.contains("My Phones is on the computer. Move it to the DAC? [y/N]"),
            "{err}"
        );
        assert!(
            err.contains("nothing changed") && !err.contains("loaded"),
            "{err}"
        );
        assert!(home.exists("svolder/host-eq.conf"));
    }

    let (code, _out, err) = home.run_on_terminal(&["eq", "load", "My Phones"], "y\n");
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("loaded My Phones"), "{err}");
    assert!(!home.exists("svolder/host-eq.conf"));
    assert!(!home.exists("units/svolder-host-eq.service"));
    let (_code, out, _err) = home.run(&["host", "eq", "status"]);
    assert_eq!(out, "host EQ: off (last: My Phones)\n");
}

#[test]
fn loading_a_preset_that_is_not_on_the_computer_asks_nothing() {
    let home = Home::new();
    import(&home, "My Phones");
    import(&home, "Other");
    home.run(&["host", "eq", "apply", "My Phones"]);
    let (code, _out, err) = home.run(&["eq", "load", "Other"]);
    assert_eq!(code, 0, "{err}");
    assert!(!err.contains("[y/N]"), "{err}");
    assert!(home.exists("svolder/host-eq.conf"));
}

/// Wait up to five seconds for `child` to exit.
fn exits_soon(child: &mut Child) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}

#[test]
fn a_fake_pipewire_exits_when_the_test_that_started_it_is_gone() {
    let mut gone = Command::new("true").spawn().unwrap();
    gone.wait().unwrap();
    let home = Home::new();
    let mut fake = Command::new(home.path().join("pipewire"))
        .args(["-c", "x"])
        .env("SVHE_TEST_PID", gone.id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(
        exits_soon(&mut fake),
        "the fake pipewire outlived its test run"
    );
}

#[test]
fn the_sweep_stops_the_fakes_and_removes_the_homes_of_runs_that_are_over() {
    let root =
        TempPath(std::env::temp_dir().join(format!("svhe-cli-sweep-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&root.0);
    std::fs::create_dir_all(&root.0).unwrap();
    let dead = root.0.join("svhe-cli-4294967295-0");
    let live = root.0.join(format!("svhe-cli-{}-7", std::process::id()));
    std::fs::create_dir_all(&dead).unwrap();
    std::fs::create_dir_all(&live).unwrap();
    std::os::unix::fs::symlink(programs().join("pipewire"), dead.join("pipewire")).unwrap();
    let mut fake = Command::new(dead.join("pipewire"))
        .args(["-c", "sweep-conf"])
        .env("SVHE_TEST_PID", std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::fs::write(
        dead.join("svolder-host-eq.pid"),
        format!("{}\nignored\nsweep-conf\n", fake.id()),
    )
    .unwrap();
    // The fake has to be running its script, not still the shell that execs it.
    let shown = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < shown
        && !std::fs::read(format!("/proc/{}/cmdline", fake.id()))
            .is_ok_and(|raw| raw.windows(10).any(|w| w == b"sweep-conf"))
    {
        std::thread::sleep(Duration::from_millis(20));
    }

    sweep_finished_homes(&root.0);

    assert!(exits_soon(&mut fake), "the sweep left the fake pipewire up");
    assert!(!dead.exists(), "the finished run's home is still there");
    assert!(live.exists(), "a live run's home was removed");
}

/// Kills and reaps its child when dropped, so a panicking test leaves nothing running.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn the_sweep_leaves_a_process_that_is_not_the_homes_fake_alone() {
    let root =
        TempPath(std::env::temp_dir().join(format!("svhe-cli-other-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&root.0);
    std::fs::create_dir_all(&root.0).unwrap();
    let dead = root.0.join("svhe-cli-4294967295-0");
    std::fs::create_dir_all(&dead).unwrap();
    let mut other = KillOnDrop(Command::new("sleep").arg("30").spawn().unwrap());
    std::fs::write(
        dead.join("svolder-host-eq.pid"),
        format!("{}\nignored\n30\n", other.0.id()),
    )
    .unwrap();

    sweep_finished_homes(&root.0);

    std::thread::sleep(Duration::from_millis(200));
    let alive = other.0.try_wait().unwrap().is_none();
    assert!(!dead.exists(), "the finished run's home is still there");
    assert!(
        alive,
        "the sweep signalled a process that is not the home's fake"
    );
}
