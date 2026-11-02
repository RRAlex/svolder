// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder capture` as a spawned binary over the fake device, in homes of its own.

mod common;

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use common::TempPath;

/// A scratch home of one run's own, removed with what the run left in it.
fn scratch(what: &str) -> TempPath {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let at = NEXT.fetch_add(1, Ordering::Relaxed);
    TempPath(std::env::temp_dir().join(format!("svolder-{what}-{}-{at}", std::process::id())))
}

/// `svolder capture --seconds 2 -o <out>` with every home under `home`, over the fake device
/// with `envs` set, and the stderr and the file it wrote.
fn capture(home: &Path, envs: &[(&str, &str)]) -> (i32, String, String) {
    std::fs::create_dir_all(home).expect("the home is made");
    let out = home.join("capture.txt");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_svolder"));
    cmd.env("HOME", home)
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", home.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", home.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", home.join("units"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env("XDG_DATA_HOME", home.join("data"))
        .args(["capture", "--seconds", "2", "-o"])
        .arg(&out);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let run = cmd.output().expect("the binary runs");
    let text = std::fs::read_to_string(&out).unwrap_or_default();
    (
        run.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&run.stderr).into_owned(),
        text,
    )
}

/// The seconds of a frame line `+S.mmm s  received  ...`.
fn seconds_of(line: &str) -> Option<f64> {
    line.strip_prefix('+')?.split(" s ").next()?.parse().ok()
}

/// A device that sends notifications on its own during the window has them in the file, after
/// the first second of the window as well as before it.
#[test]
fn notifications_inside_the_window_are_in_the_file() {
    let home = scratch("capture-signal");
    let (code, stderr, text) = capture(
        &home.0,
        &[
            ("SVOLDER_FAKE_PROFILE", "fake-amp"),
            ("SVOLDER_FAKE_SIGNAL", "1"),
        ],
    );
    assert_eq!(code, 0, "{stderr}\n{text}");
    let received: Vec<f64> = text
        .lines()
        .filter(|line| line.contains("  received  "))
        .filter_map(seconds_of)
        .collect();
    assert!(
        received.iter().any(|at| *at > 1.0),
        "frames heard after the first second: {received:?}\n{text}"
    );
    assert!(text.contains("-- frame window from +"), "{text}");
    assert!(stderr.contains("checking the host"), "{stderr}");
}

/// A name the owner gave an input is not in the file; the profile's own names are.
#[test]
fn the_owners_input_names_stay_out_of_the_file() {
    let home = scratch("capture-inputs");
    let config = home.0.join("config").join("svolder");
    std::fs::create_dir_all(&config).expect("the config directory is made");
    std::fs::write(
        config.join("config.toml"),
        "schema = 1\n[[device]]\nname = \"Sam Example office DAC\"\nudid = \"0102030405060708\"\n\
         [device.inputs]\n1 = \"Sam Example laptop\"\n",
    )
    .expect("the config is written");
    let (code, stderr, text) = capture(&home.0, &[]);
    assert_eq!(code, 0, "{stderr}\n{text}");
    assert!(!text.contains("Sam Example laptop"), "{text}");
    assert!(text.contains("== inspect =="), "{text}");
}
