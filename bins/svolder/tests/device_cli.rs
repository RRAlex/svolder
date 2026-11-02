// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs the built svolder binary against the fake transport.

mod common;

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use common::{TempPath, WATCH_DEADLINE, Watcher};

/// A scratch home of one run's own under the system temp dir, removed with whatever the run
/// left in it when the case lets go of it.
fn scratch(what: &str) -> TempPath {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let at = NEXT.fetch_add(1, Ordering::Relaxed);
    TempPath(std::env::temp_dir().join(format!("svolder-{what}-{}-{at}", std::process::id())))
}

fn svolder(args: &[&str]) -> (i32, String, String) {
    svolder_env(args, &[])
}

/// Same as `svolder`, with extra environment variables set on the process (for example
/// `SVOLDER_FAKE_FAULTS`).
///
/// The socket points where nothing listens and the config and state directories do not exist,
/// so every case here runs against the in-process fake whatever daemon, configuration or
/// remembered adapter the machine running the tests happens to have. The `systemctl` named is
/// not a program and the unit directory is under the same scratch directory, so nothing a verb
/// does about the daemon reaches the machine's own systemd.
fn svolder_env(args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
    let home = scratch("cli-test");
    run_in(&home.0, args, envs)
}

/// [`svolder_env`] in a home whose `config.toml` is `config`.
fn svolder_configured(config: &str, args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
    let home = scratch("cli-test");
    let dir = home.0.join("svolder");
    std::fs::create_dir_all(&dir).expect("the configuration directory is made");
    std::fs::write(dir.join("config.toml"), config).expect("the configuration is written");
    run_in(&home.0, args, envs)
}

/// The configuration of a home with one Loki Max in it, which says which profile the preset
/// verbs read without a device to ask.
const LOKI_MAX: &str =
    "schema = 1\n[[device]]\nname = \"loki\"\nudid = \"0102030405060708\"\nprofile = \"lokimax\"\n";

/// One run of the binary with `scratch` as its configuration and state home.
fn run_in(
    scratch: &std::path::Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_svolder"));
    cmd.env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", scratch.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", scratch.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", scratch.join("units"))
        .env("XDG_CONFIG_HOME", scratch)
        .env("XDG_STATE_HOME", scratch)
        .args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("binary runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn scan_lists_the_fake_gungnir() {
    let (code, out, _) = svolder(&["scan"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Gungnir 2"), "{out}");
    assert!(out.contains("0102030405060708"), "{out}");
}

/// A scan row carries how loud the device was and which product it says it is: the signal is
/// on the line a person reads, and both are in the object a program reads.
#[test]
fn scan_reports_the_signal_and_the_product_id() {
    let (code, out, err) = svolder(&["scan", "--json"]);
    assert_eq!(code, 0, "out={out} err={err}");
    let row: serde_json::Value =
        serde_json::from_str(out.lines().next().expect("one row")).expect("one JSON object");
    assert_eq!(row["product_id"], 2, "{out}");
    assert_eq!(row["rssi"], -50, "{out}");

    let (code, out, err) = svolder(&["scan"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("-50 dBm"), "{out}");
}

/// The fake amp advertises a product id the codec's table does not know, so it is named after
/// the profile made for it: `scan` lists it as that product, its `add` line names it so, and a
/// selector naming that product reaches it.
#[test]
fn the_fake_amp_is_named_after_its_profile() {
    let amp = [("SVOLDER_FAKE_PROFILE", "fake-amp")];
    let (code, out, err) = svolder_env(&["scan"], &amp);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("0102030405060708  Fake amp  fw "), "{out}");
    assert!(
        out.contains(r#"svolder add 0102030405060708 --name "Fake amp""#),
        "{out}"
    );

    let (code, out, err) = svolder_env(&["status", "-d", "fake amp"], &amp);
    assert_eq!(code, 0, "out={out} err={err}");
}

#[test]
fn inspect_prints_controls() {
    let (code, out, err) = svolder(&["inspect"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("Volume"), "{out}");
    assert!(out.contains("Input"), "{out}");
}

#[test]
fn inspect_unknown_device_exits_two() {
    let (code, _, err) = svolder(&["-d", "ZZ", "inspect"]);
    assert_eq!(code, 2);
    assert!(
        err.contains("close the phone app") && err.contains("`svolder scan`"),
        "the message names what to do next: {err}"
    );
}

#[test]
fn status_lists_every_control() {
    let (code, out, err) = svolder(&["status"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("Volume"), "{out}");
    assert!(out.contains("Mute"), "{out}");
}

#[test]
fn get_returns_a_bare_value() {
    let (code, out, _) = svolder(&["get", "input"]);
    assert_eq!(code, 0, "{out}");
    // seeded default input is the lowest select value, 1 (USB 1)
    assert_eq!(out.trim(), "1 (USB 1)");
}

#[test]
fn set_confirms_the_new_value() {
    let (code, out, err) = svolder(&["set", "input", "4"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains('4'), "{out}");
}

#[test]
fn freya2_seeds_passive_and_writes_the_wire_mode() {
    let env = [("SVOLDER_FAKE_PROFILE", "freya2")];
    let (code, out, err) = svolder_env(&["status"], &env);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("Status 3.1 u8 0   Mode = 0 (Passive)"),
        "{out}"
    );
    let (code, out, err) = svolder_env(&["set", "mode", "Tube voltage / Tube current"], &env);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Status 3.1 u8 4"), "{out}");
}

#[test]
fn hati_lists_its_controls() {
    let (code, out, err) = svolder_env(&["status"], &[("SVOLDER_FAKE_PROFILE", "hati")]);
    assert_eq!(code, 0, "{err}");
    for line in [
        "Status 2 u8 1   Input = 1 (RCA 1)",
        "Status 10 bool false   Power = off",
        "Status 11.1 u8 1   Load capacitance = 1 (50 pF)",
        "Status 201 bool false   Remote lock = false",
    ] {
        assert!(out.contains(line), "{line}: {out}");
    }
}

#[test]
fn set_up_moves_a_level() {
    // Volume seeded to its midpoint; `up` nudges it by one raw step.
    let (code, out, err) = svolder(&["set", "volume", "up"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(!out.is_empty(), "{out}");
}

#[test]
fn get_unknown_key_exits_one() {
    let (code, _, err) = svolder(&["get", "nope"]);
    assert_eq!(code, 1);
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn set_accepts_a_negative_db_value() {
    let (code, out, err) = svolder(&["set", "volume", "-6dB"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("109"), "{out}");
    assert!(out.contains("-6.0 dB"), "{out}");
}

#[test]
fn get_volume_prints_the_canonical_display() {
    let (code, out, err) = svolder(&["get", "volume"]);
    assert_eq!(code, 0, "out={out} err={err}");
    // Seeded default volume is raw 63, the midpoint of the 0..=127 range.
    assert_eq!(out.trim(), "-26.1 dB", "{out}");
}

/// The balance is counted from its centre both ways, as every other front end counts it: `get`
/// reads 0 at the centre, `set balance 6` is six steps right and `set balance 0` centres it. A
/// number past either end is refused in the same terms, the old raw centre among them.
#[test]
fn the_balance_is_read_and_set_from_its_centre() {
    let (code, out, err) = svolder(&["get", "balance"]);
    assert_eq!((code, out.trim()), (0, "0"), "{err}");
    let (code, out, err) = svolder(&["set", "balance", "6"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.trim(), "Status 11 u8 70   Balance = 6");
    let (code, out, err) = svolder(&["set", "balance", "0"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.trim(), "Status 11 u8 64   Balance = 0");
    let (code, _, err) = svolder(&["set", "balance", "64"]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("balance is from -63 to 63, counted from its centre; 64 is outside that"),
        "{err}"
    );
}

#[test]
fn set_up_on_a_non_level_exits_one() {
    let (code, _, err) = svolder(&["set", "mute", "up"]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("control mute has no range to step through"),
        "{err}"
    );
}

/// The refusals of a relative step on the direct path, which the daemon's own integration test
/// repeats word for word: an unknown key is a usage error, and a step down from the current
/// volume is written.
#[test]
fn a_relative_step_is_refused_with_the_same_words_as_over_the_daemon() {
    let (code, _, err) = svolder(&["set", "nope", "up"]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("no control named nope; this device has: "),
        "{err}"
    );
    let (code, out, err) = svolder(&["set", "volume", "down"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("62"), "{out}");
}

#[test]
fn set_with_an_extreme_step_clamps() {
    let (code, out, err) = svolder(&["set", "volume", "up", "--step", "9223372036854775807"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("127"), "{out}");
}

#[test]
fn set_no_wait_prints_nothing() {
    let (code, out, err) = svolder(&["set", "input", "4", "--no-wait"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.is_empty(), "{out}");
}

#[test]
fn set_json_prints_one_object() {
    let (code, out, err) = svolder(&["set", "input", "4", "--json"]);
    assert_eq!(code, 0, "out={out} err={err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("one JSON object");
    assert_eq!(v["key"], "input");
    assert_eq!(v["raw"], 4);
}

#[test]
fn set_bad_literal_exits_one() {
    let (code, _, err) = svolder(&["set", "input", "nonsense"]);
    assert_eq!(code, 1, "{err}");
}

#[test]
fn status_exits_five_on_an_active_fault() {
    let (code, _, err) = svolder_env(
        &["status"],
        &[
            ("SVOLDER_FAKE_FAULTS", "1"),
            ("SVOLDER_FAKE_PROFILE", "fake-amp"),
        ],
    );
    assert_eq!(code, 5, "{err}");
    assert!(err.contains("active fault"), "{err}");
}

#[test]
fn raw_set_of_firmware_update_is_refused() {
    // The firmware-update family is refused: this must fail, and the device must see nothing.
    let (code, _, err) = svolder(&["raw", "210.1", "u8", "0"]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("firmware-update") || err.contains("cannot be sent"),
        "{err}"
    );
}

/// The firmware-update refusal again, from the other side: it happens before a device is
/// even selected, so a selector that matches nothing still exits 1 (the refusal) and not 2.
#[test]
fn raw_set_of_firmware_update_is_refused_before_connecting() {
    let (code, _, err) = svolder(&["raw", "210.3.1", "-d", "nosuchdevice", "u8", "0"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("firmware-update family"), "{err}");
}

/// An empty `data` value is an empty payload (a Loki Max preset's save or recall carries
/// nothing), so it passes the value check and the run goes on to select the device, which here
/// matches nothing.
#[test]
fn raw_takes_an_empty_data_value() {
    let (code, _, err) = svolder(&["raw", "7.2.1", "data", "", "-d", "nosuchdevice"]);
    assert!(!err.contains("is not hex"), "{err}");
    assert_eq!(code, 2, "the run reached device selection: {err}");
}

/// A value type that does not exist, and a value that does not fit its type, are refused
/// before any device is selected, so a selector that matches nothing still exits 1.
#[test]
fn raw_with_a_bad_value_is_refused_before_connecting() {
    let (code, _, err) = svolder(&["raw", "2", "u8", "999", "-d", "nosuchdevice"]);
    assert_eq!(code, 1, "{err}");
    assert!(!err.contains("nosuchdevice"), "{err}");
    let (code, _, err) = svolder(&["raw", "2", "nosuchtype", "1", "-d", "nosuchdevice"]);
    assert_eq!(code, 1, "{err}");
    assert!(!err.contains("nosuchdevice"), "{err}");
}

/// `210.2.<sub>` is the firmware-version query, the one member of the 210 family that is
/// not restricted, so it must go through.
#[test]
fn raw_set_of_the_firmware_version_query_is_allowed() {
    let (code, out, err) = svolder(&["raw", "210.2.1", "u8", "0"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("210.2.1"), "{out}");
}

/// A fake knob that does not parse is an error, not a silent default.
#[test]
fn a_non_numeric_fake_firmware_is_an_error() {
    let (code, _, err) = svolder_env(&["scan"], &[("SVOLDER_FAKE_FIRMWARE", "abc")]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("SVOLDER_FAKE_FIRMWARE"), "{err}");
}

#[test]
fn raw_set_of_a_normal_id_confirms() {
    let (code, out, err) = svolder(&["raw", "2", "u8", "3"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains('3'), "{out}");
}

#[test]
fn raw_set_json_prints_one_object() {
    let (code, out, err) = svolder(&["raw", "2", "u8", "3", "--json"]);
    assert_eq!(code, 0, "out={out} err={err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("one JSON object");
    assert_eq!(v["id"], "2");
    assert_eq!(v["raw"], 3);
}

#[test]
fn raw_set_of_passphrase_id_is_refused() {
    let (code, _, err) = svolder(&["raw", "250", "u32", "1"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("passphrase set"), "{err}");
}

/// `passphrase clear` reaches the device through the same call `passphrase set none` does, and
/// reports the same thing: the passphrase is gone.
#[test]
fn passphrase_clear_clears_the_passphrase() {
    let (code, out, err) = svolder(&["passphrase", "clear", "--yes"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(err.contains("passphrase cleared"), "out={out} err={err}");
}

/// Without `--yes` and with stdin not a TTY, `passphrase clear` refuses exactly as `set` does:
/// a script cannot clear a passphrase by accident.
#[test]
fn passphrase_clear_without_yes_on_a_non_tty_stdin_refuses() {
    let (code, _out, err) = svolder(&["passphrase", "clear"]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("refusing to change the passphrase without confirmation"),
        "{err}"
    );
}

#[test]
fn eq_mode_sets_on_a_device_with_tone_control() {
    // gungnir2-v2 has a 5-band eq (id 12).
    // The fake rests in flat, and a unit reports a change of the mode only, so a change is the
    // write that is answered at once.
    let (code, out, err) = svolder(&["eq", "mode", "eq"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("Tone control mode = eq"), "{out}");
    // Each run starts a fresh fake in flat, so this writes the mode it is already in: no Status
    // comes back, and the answer is still the mode, not an error.
    let (code, out, err) = svolder(&["eq", "mode", "flat"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("Tone control mode = flat"), "{out}");
}

#[test]
fn eq_show_lists_the_bands_with_the_values_the_device_reports() {
    let (code, out, err) = svolder(&["eq", "show"]);
    assert_eq!(code, 0, "out={out} err={err}");
    // The fake reports its tone control at rest, so every band has a value to show.
    assert!(out.contains("mode flat"), "{out}");
    assert!(out.contains("loudness 0"), "{out}");
    assert!(out.contains("band 1  40.0 Hz  +0.0 dB  Q 4.30"), "{out}");
    assert!(out.contains("band 5  10000 Hz  +0.0 dB  Q 4.30"), "{out}");
    assert!(!out.contains('?'), "no band is left unknown: {out}");
}

/// `fake-amp` has meters but no tone control; both eq verbs name the reason rather than
/// failing some other way.
#[test]
fn eq_mode_on_a_device_with_no_tone_control_names_the_reason() {
    let (code, _, err) = svolder_env(
        &["eq", "mode", "flat"],
        &[("SVOLDER_FAKE_PROFILE", "fake-amp")],
    );
    assert_eq!(code, 1, "{err}");
    let said = svolder_profiles::ProfileError::NoEq.to_string();
    assert!(err.contains(&said), "{err}");
}

#[test]
fn eq_show_on_a_device_with_no_tone_control_names_the_reason() {
    let (code, _, err) = svolder_env(&["eq", "show"], &[("SVOLDER_FAKE_PROFILE", "fake-amp")]);
    assert_eq!(code, 1, "{err}");
    let said = svolder_profiles::ProfileError::NoEq.to_string();
    assert!(err.contains(&said), "{err}");
}

#[test]
fn fake_transport_still_default() {
    // Regression: with no --transport, the fake still answers.
    let (code, out, _) = svolder(&["scan"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Gungnir 2"), "{out}");
}

#[test]
fn an_unknown_transport_exits_one() {
    let (code, _, err) = svolder(&["--transport", "nope", "scan"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("nope"), "{err}");
    assert!(
        err.contains("auto") && err.contains("ble") && err.contains("fake"),
        "the message names the values it does accept: {err}"
    );
    assert!(
        err.contains("See `svolder --help`"),
        "the message names what to do next: {err}"
    );
}

#[test]
fn ble_transport_unknown_adapter_exits_three() {
    // A named adapter that does not exist is an adapter failure -> exit 3.
    let (code, _out, err) = svolder(&["--transport", "ble", "--adapter", "hcinope9", "scan"]);
    assert_eq!(code, 3, "err={err}");
}

/// An adapter failure names the verb that looks at the machine's Bluetooth, and exits with the
/// code it always did.
///
/// The words themselves are the transport's, and what it says depends on what BlueZ answers on
/// the machine running this, so only the step is pinned here; the sentence it is appended to is
/// pinned by the `recovery` unit tests.
#[test]
fn a_missing_adapter_names_the_next_step() {
    let (code, _out, err) = svolder_env(
        &["--transport", "ble", "scan"],
        &[
            ("SVOLDER_ADAPTER", "hci-none"),
            ("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/"),
        ],
    );
    assert_eq!(code, 3, "err={err}");
    assert!(
        err.contains("Run `svolder doctor`"),
        "the message names what to do next: {err}"
    );
}

/// The fake amplifier plays a signal on its meters, so `watch` over it keeps reporting
/// readings instead of falling silent after the snapshot, and a front end that draws to the
/// music has something moving to draw.
///
/// Three lines within 600 ms of the spawn, measured from it — about five times the headroom
/// actually measured, generous against a loaded machine and still far short of `drain_bound`
/// (2 s): the snapshot carries one, so the third is the second the signal pushed, and a
/// refresh that waited out its bound instead of ending on the readings would not get there.
/// The watcher is guarded the same way the case below is: `watch` against a fake device never
/// ends on its own.
#[test]
fn watch_over_the_signalling_fake_amp_keeps_reporting_its_meters() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let home = scratch("cli-signal");
    let scratch = &home.0;
    let child = Command::new(env!("CARGO_BIN_EXE_svolder"))
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
        .env("SVOLDER_FAKE_PROFILE", "fake-amp")
        .env("SVOLDER_FAKE_SIGNAL", "1")
        .args(["watch"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn watch");
    let spawned = Instant::now();
    let deadline = Duration::from_millis(1000);
    let mut watcher = Watcher::new(child);
    for nth in 1..=3 {
        let left = deadline.saturating_sub(spawned.elapsed());
        assert!(
            watcher.wait_for_line("Meter L", left),
            "meter reading {nth} had not arrived {:?} after the spawn: {:?}",
            spawned.elapsed(),
            watcher.output()
        );
    }
}

/// `watch` over the in-process fake prints the refresh burst it drained on connecting.
///
/// The watcher is guarded: `watch` against a fake device never ends on its own — the link
/// never drops — so a panic here without a `Drop` that kills the child would leave one
/// running for as long as the machine is up.
#[test]
fn watch_streams_then_stops_on_kill() {
    use std::process::{Command, Stdio};
    let home = scratch("cli-watch");
    let scratch = &home.0;
    let child = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", scratch.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", scratch.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", scratch.join("units"))
        .env("XDG_CONFIG_HOME", scratch)
        .env("XDG_STATE_HOME", scratch)
        .args(["watch"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn watch");
    let mut watcher = Watcher::new(child);
    let arrived = watcher.wait_for_line("Volume", WATCH_DEADLINE);
    assert!(
        arrived,
        "watch produced no statuses within {WATCH_DEADLINE:?}: {:?}",
        watcher.output()
    );
}

/// A Ctrl-C sent as soon as the first status line is out ends `watch` with exit 0, never by the
/// signal: the handler is in place before the first reading is printed.
///
/// Repeated, so a handler installed later than the first line would show.
#[test]
fn watch_leaves_with_zero_on_ctrl_c_sent_right_after_the_first_status() {
    use std::process::{Command, Stdio};
    for round in 0..5 {
        let home = scratch("cli-watch-ctrl-c");
        let scratch = &home.0;
        let child = Command::new(env!("CARGO_BIN_EXE_svolder"))
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_TRANSPORT", "fake")
            .env("SVOLDER_SOCKET", scratch.join("no-daemon.sock"))
            .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", scratch.join("no-wireplumber"))
            .env("SVOLDER_UNITDIR", scratch.join("units"))
            .env("XDG_CONFIG_HOME", scratch)
            .env("XDG_STATE_HOME", scratch)
            .args(["watch"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn watch");
        let mut watcher = Watcher::new(child);
        assert!(
            watcher.wait_for_line("Volume", WATCH_DEADLINE),
            "round {round}: no status within {WATCH_DEADLINE:?}: {:?}",
            watcher.output()
        );
        watcher.interrupt();
        let status = watcher
            .exit_within(Duration::from_secs(10))
            .unwrap_or_else(|| panic!("round {round}: watch did not leave after Ctrl-C"));
        assert_eq!(status.code(), Some(0), "round {round}: {status:?}");
    }
}

/// `watch --json` prints one `ValueObject` per line, the same shape every other verb's
/// `--json` does, rather than the plain status line.
#[test]
fn watch_json_streams_value_objects() {
    use std::process::{Command, Stdio};
    let home = scratch("cli-watch-json");
    let scratch = &home.0;
    let child = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_SOCKET", scratch.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", scratch.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", scratch.join("units"))
        .env("XDG_CONFIG_HOME", scratch)
        .env("XDG_STATE_HOME", scratch)
        .args(["watch", "--json"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn watch --json");
    let mut watcher = Watcher::new(child);
    let arrived = watcher.wait_for_line("\"label\":\"Volume\"", WATCH_DEADLINE);
    assert!(
        arrived,
        "watch --json produced no volume object within {WATCH_DEADLINE:?}: {:?}",
        watcher.output()
    );
}

/// A run against a user profile with a gated warm-up readout and an `auto_off` readout.
fn gated_profile_run(args: &[&str]) -> (i32, String, String) {
    let home = scratch("gated-profile");
    let profiles = home.0.join("svolder").join("profiles");
    std::fs::create_dir_all(&profiles).expect("create the profile directory");
    std::fs::write(
        profiles.join("gated.toml"),
        "schema = 1\nname = \"gated\"\nproduct_id = 999\n\
         [[control]]\nid = [4]\nkey = \"warming_up\"\nlabel = \"Warming up\"\nkind = \"readout\"\n\
         value_type = \"bool\"\nonly_while = \"power\"\n\
         [[control]]\nid = [10]\nkey = \"power\"\nlabel = \"Power\"\nkind = \"toggle\"\n\
         [[control]]\nid = [21, 1]\nkey = \"temperature_reporting\"\nlabel = \"Temperature reporting\"\n\
         kind = \"readout\"\nvalue_type = \"bool\"\nauto_off = true\n",
    )
    .expect("write the profile");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_svolder"));
    cmd.env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_FAKE_PROFILE", "gated")
        .env("SVOLDER_SOCKET", home.0.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", home.0.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", home.0.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", home.0.join("units"))
        .env("XDG_CONFIG_HOME", &home.0)
        .env("XDG_STATE_HOME", &home.0)
        .env("XDG_DATA_HOME", &home.0)
        .env("XDG_RUNTIME_DIR", &home.0)
        .args(args);
    let out = cmd.output().expect("binary runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A warm-up readout shows false while the power it depends on reads false, and an
/// `auto_off` readout is refused as not writable.
#[test]
fn a_gated_readout_prints_its_derived_value_and_an_auto_off_readout_is_read_only() {
    let (code, out, err) = gated_profile_run(&["status"]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(out.contains("Warming up = false"), "{out}");

    let (code, out, err) = gated_profile_run(&["set", "temperature_reporting", "off"]);
    assert_ne!(code, 0, "out={out} err={err}");
    assert!(
        err.contains("not writable") || err.contains("read-only"),
        "{err}"
    );
}

/// `set standby` names no control on an amp; `set power` does.
#[test]
fn the_old_standby_key_is_unknown_and_power_is_set() {
    let (code, out, err) = svolder_env(
        &["set", "standby", "on"],
        &[("SVOLDER_FAKE_PROFILE", "aegir2")],
    );
    assert_eq!(code, 1, "out={out} err={err}");
    assert!(
        err.contains("no control named standby; this device has: ")
            && err.contains("did you mean power?"),
        "{err}"
    );

    let (code, out, err) = svolder_env(
        &["set", "power", "on"],
        &[("SVOLDER_FAKE_PROFILE", "aegir2")],
    );
    assert_eq!(code, 0, "out={out} err={err}");
}

/// An amp's temperature reporting is a readout the session switches off, so a user cannot
/// write it.
#[test]
fn an_amps_temperature_reporting_is_not_writable() {
    let (code, out, err) = svolder_env(
        &["set", "temperature_reporting", "off"],
        &[("SVOLDER_FAKE_PROFILE", "kraken")],
    );
    assert_ne!(code, 0, "out={out} err={err}");
    assert!(
        err.contains("not writable") || err.contains("read-only"),
        "{err}"
    );
}

/// `watch` keeps the link of a device that refuses its passphrase, where a verb that reads it
/// once reports the refusal: the volunteered meter readings still come through.
#[test]
fn watch_keeps_a_refused_link_and_goes_on_reporting() {
    use std::process::{Command, Stdio};
    use std::time::Duration;
    let home = scratch("cli-refused-watch");
    let scratch = &home.0;
    let config = scratch.join("svolder");
    std::fs::create_dir_all(&config).expect("create the config directory");
    std::fs::write(
        config.join("config.toml"),
        "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\npassphrase = \"9999\"\n",
    )
    .expect("write the config");
    let child = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_SOCKET", scratch.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", scratch.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", scratch.join("units"))
        .env("XDG_CONFIG_HOME", scratch)
        .env("XDG_STATE_HOME", scratch)
        .env("SVOLDER_FAKE_PROFILE", "fake-amp")
        .env("SVOLDER_FAKE_SIGNAL", "1")
        .args(["watch"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn watch");
    let mut watcher = Watcher::new(child);
    for nth in 1..=3 {
        assert!(
            watcher.wait_for_line("Meter L", Duration::from_secs(5)),
            "meter reading {nth} never arrived: {:?}",
            watcher.output()
        );
    }
}

/// The environment that makes the fake device a Loki Max.
const FAKE_LOKI: [(&str, &str); 1] = [("SVOLDER_FAKE_PROFILE", "lokimax")];

/// A recall is sent with no wait and no reading: the CLI says what it sent on stderr, prints
/// nothing on stdout, and with `--json` prints one object saying so.
#[test]
fn set_recall_sends_the_recall_and_says_so() {
    let (code, out, err) = svolder_env(&["set", "recall", "2"], &FAKE_LOKI);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(out, "", "an action has no reading to print");
    assert!(err.contains("svolder: recalled preset 2"), "{err}");

    let (code, out, err) = svolder_env(&["set", "recall", "Preset 3", "--json"], &FAKE_LOKI);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(out.trim(), r#"{"key":"recall","slot":3,"sent":true}"#);
}

/// A save overwrites what the device holds in the slot, so off a terminal it is refused until
/// `--yes` says so, and says how to say it. This proves the words and the exit code; that
/// nothing is sent is proved by `a_refused_save_leaves_the_slot_as_the_last_save_left_it` (in
/// `commands/device/device_presets/tests.rs`) and by
/// `set_save_with_yes_saves_and_a_later_recall_brings_the_bands_back` (in `backend.rs`), which
/// read the slot back after a refused save.
#[test]
fn set_save_off_a_terminal_without_yes_is_refused_and_sends_nothing() {
    let (code, out, err) = svolder_env(&["set", "save", "2"], &FAKE_LOKI);
    assert_eq!(code, 1, "out={out} err={err}");
    assert_eq!(out, "");
    assert!(
        err.contains(
            "svolder: saving to preset 2 overwrites what Loki Max holds there; \
             add --yes to save without asking"
        ),
        "{err}"
    );
    assert!(!err.contains("saved"), "{err}");

    let (code, out, err) = svolder_env(&["set", "save", "2", "--yes"], &FAKE_LOKI);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(
        err.contains("svolder: saved the bands to preset 2"),
        "{err}"
    );
}

/// With `--json` the refusal of a save off a terminal is one object on stdout, so a script reads
/// that nothing was sent and why; the words stay on stderr and the exit code is 1.
#[test]
fn set_save_json_off_a_terminal_without_yes_prints_the_refusal_as_one_object() {
    let (code, out, err) = svolder_env(&["set", "save", "2", "--json"], &FAKE_LOKI);
    assert_eq!(code, 1, "out={out} err={err}");
    assert_eq!(
        out.trim(),
        r#"{"key":"save","slot":2,"sent":false,"error":"saving to preset 2 overwrites what Loki Max holds there; add --yes to save without asking"}"#
    );
    assert!(
        err.contains("saving to preset 2 overwrites what Loki Max holds there"),
        "{err}"
    );
    assert!(!err.contains("saved"), "{err}");
}

/// The device has three slots, and a slot it has not got is the user's mistake, said the way
/// every refused value is.
#[test]
fn set_recall_4_on_a_three_slot_device_is_a_usage_error() {
    let (code, out, err) = svolder_env(&["set", "recall", "4"], &FAKE_LOKI);
    assert_eq!(code, 1, "out={out} err={err}");
    assert!(err.contains("recall takes a slot from 1 to 3"), "{err}");
    let (code, _, err) = svolder_env(&["set", "save", "0", "--yes"], &FAKE_LOKI);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("save takes a slot from 1 to 3"), "{err}");
    let (code, _, err) = svolder_env(&["set", "recall", "up"], &FAKE_LOKI);
    assert_eq!(code, 1, "{err}");
}

/// An action is sent, never read: `get` says so and names the verb that sends it.
#[test]
fn get_recall_is_a_usage_error_naming_set() {
    let (code, out, err) = svolder_env(&["get", "recall"], &FAKE_LOKI);
    assert_eq!(code, 1, "out={out} err={err}");
    assert_eq!(out, "");
    assert!(
        err.contains(
            "recall is an action: it is sent, never read; svolder set recall 1-3 sends it"
        ),
        "{err}"
    );
}

/// `get` on a band prints what it prints for any level: the value in the unit the profile gives.
#[test]
fn get_reads_a_band_of_the_loki_max_as_it_reads_any_level() {
    let (code, out, err) = svolder_env(&["get", "band1"], &FAKE_LOKI);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(out.trim(), "0.0 dB");
}

/// The Loki Max lists its three slots, which only `set` reaches, then the five built-in
/// settings, and ends on the two verbs that use them.
#[test]
fn preset_list_lists_three_slots_then_the_five_builtins() {
    let (code, out, err) = svolder_configured(LOKI_MAX, &["preset", "list"], &[]);
    assert_eq!(code, 0, "out={out} err={err}");
    let rows: Vec<Vec<&str>> = out
        .lines()
        .map(|line| line.split("  ").filter(|w| !w.is_empty()).collect())
        .collect();
    let names: Vec<&str> = rows.iter().map(|row| row[0].trim()).collect();
    assert_eq!(
        &names[..8],
        [
            "Preset 1",
            "Preset 2",
            "Preset 3",
            "Flat",
            "Bass Boost",
            "Treble Boost",
            "Smiley",
            "Loudness"
        ]
    );
    assert!(rows[..3].iter().all(|row| row.contains(&"slot")), "{out}");
    assert!(
        rows[3..8].iter().all(|row| row.contains(&"built-in")),
        "{out}"
    );
    assert!(
        !out.contains(" on"),
        "no daemon holds the device, so nothing is marked on: {out}"
    );
    assert_eq!(
        out.lines().last(),
        Some("svolder set recall N recalls a slot; svolder preset apply NAME applies a built-in")
    );
}

/// `--json` gives one object a row: a slot has its number and no `on`, a built-in has
/// `built_in` and, with no daemon to read the bands from, no `on` either.
#[test]
fn preset_list_json_has_one_object_per_row() {
    let (code, out, err) = svolder_configured(LOKI_MAX, &["preset", "list", "--json"], &[]);
    assert_eq!(code, 0, "out={out} err={err}");
    let rows: Vec<serde_json::Value> = out
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object a line"))
        .collect();
    assert_eq!(rows.len(), 8, "{out}");
    assert_eq!(
        rows[1],
        serde_json::json!({"name": "Preset 2", "slot": 2, "built_in": false, "on": null})
    );
    assert_eq!(
        rows[4],
        serde_json::json!({"name": "Bass Boost", "slot": null, "built_in": true, "on": null})
    );
}

/// Applying a setting writes each band that is not on its step, waits for each echo and counts
/// them; the fake starts every band at 0 dB.
#[test]
fn preset_apply_writes_each_band_off_its_step_and_counts_the_writes() {
    let (code, out, err) = svolder_env(&["preset", "apply", "bass boost"], &FAKE_LOKI);
    assert_eq!(code, 0, "out={out} err={err}");
    assert_eq!(out, "");
    assert!(err.contains("svolder: Bass Boost on (3 writes)"), "{err}");
}

/// A setting every band is already on sends nothing and says so.
#[test]
fn preset_apply_of_a_setting_already_on_sends_nothing() {
    let (code, out, err) = svolder_env(&["preset", "apply", "Flat"], &FAKE_LOKI);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(err.contains("svolder: Flat is already on"), "{err}");
    assert!(!err.contains("writes"), "{err}");
}

/// A name the device does not list is refused with the names it does.
#[test]
fn preset_apply_of_an_unknown_name_lists_the_settings() {
    let (code, out, err) = svolder_env(&["preset", "apply", "Warm"], &FAKE_LOKI);
    assert_eq!(code, 1, "out={out} err={err}");
    assert!(
        err.contains(
            "no built-in setting named Warm; the settings are: \
             Flat, Bass Boost, Treble Boost, Smiley, Loudness"
        ),
        "{err}"
    );
}

/// A device with a tone control has presets of its own, which `eq list` lists; `preset` says so
/// rather than listing nothing.
#[test]
fn preset_list_on_a_device_with_a_tone_control_points_to_eq_list() {
    let config = LOKI_MAX.replace("lokimax", "gungnir2-v2");
    let (code, out, err) = svolder_configured(&config, &["preset", "list"], &[]);
    assert_eq!(code, 1, "out={out} err={err}");
    assert_eq!(out, "");
    assert!(
        err.contains("has no device presets; svolder eq list lists its tone-control presets"),
        "{err}"
    );
    let (code, _, err) = svolder_env(&["preset", "apply", "Flat"], &[]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("has no device presets; svolder eq list lists its tone-control presets"),
        "{err}"
    );
}

/// The tone-control verbs on a device that has none but has presets of its own point to
/// `preset`.
#[test]
fn the_eq_verbs_on_a_loki_max_point_to_preset_list() {
    let (code, _, err) = svolder_env(&["eq", "load", "Flat"], &FAKE_LOKI);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("this device has no tone control; svolder preset list lists its presets"),
        "{err}"
    );
    let (code, out, err) = svolder_configured(LOKI_MAX, &["eq", "list"], &[]);
    assert_eq!(code, 0, "out={out} err={err}");
    assert!(
        err.contains("this device has no tone control; svolder preset list lists its presets"),
        "{err}"
    );
}

/// `profiles show` lists what the Loki Max can be sent, how a band is read and the built-in
/// settings with their raw steps.
#[test]
fn profiles_show_lokimax_lists_its_actions_reads_and_settings() {
    let (code, out, err) = svolder_env(&["profiles", "show", "lokimax"], &[]);
    assert_eq!(code, 0, "out={out} err={err}");
    let line = |needle: &str| {
        out.lines()
            .find(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no line with {needle}: {out}"))
    };
    let recall = line("7.2.<1-3>");
    assert!(
        recall.contains("recall") && recall.contains("action") && recall.contains("Recall preset"),
        "{recall}"
    );
    assert!(line("7.1.<1-3>").contains("save"), "{out}");
    assert!(
        out.lines().any(|l| l.contains("overwrites")),
        "the save says it overwrites: {out}"
    );
    assert!(out.lines().any(|l| l.contains("changes band1")), "{out}");
    assert!(
        out.lines().any(|l| l.trim_end().ends_with("reads 6.1")),
        "{out}"
    );
    assert!(
        line("Bass Boost").contains("28 21 17 15 15 15"),
        "the setting's raw steps: {out}"
    );
    assert!(
        out.contains("band1, band2, band3, band4, band5, band6"),
        "{out}"
    );
}
