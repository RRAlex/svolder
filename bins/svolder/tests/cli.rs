// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs the built `svolder` binary: the command vocabulary, the front door, and the verbs
//! that reach neither a device nor a daemon.
//!
//! Every spawn runs over the in-process fake transport, with a config home of its own and a
//! socket path nothing listens on, so no case here touches a radio or the machine's daemon.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use svolder_ipc::Config;

mod common;

use common::TempPath;

/// What one run of the binary left behind.
struct Run {
    /// The exit code, or -1 for a child a signal ended.
    code: i32,
    /// Everything the run printed on stdout.
    stdout: String,
    /// Everything the run printed on stderr.
    stderr: String,
}

/// A fresh, empty config home for one run.
fn config_home() -> TempPath {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    let k = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    TempPath(std::env::temp_dir().join(format!("svolder-cli-test-{}-{n}-{k}", std::process::id())))
}

/// Run the CLI with `xdg` as its config home.
///
/// The transport is the in-process fake and the adapter names a controller that does not
/// exist, so a case that overrides the transport on the command line still reaches no radio;
/// the socket points where nothing listens, so no daemon of the host's answers either. The
/// `systemctl` named is not a program, and the unit directory is under the test's own home, so
/// a verb that installs the units — the fix `doctor --yes` applies among the rest — writes
/// where the test can see it and asks the machine's own systemd for nothing.
fn svolder_in(args: &[&str], xdg: &Path) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_SOCKET", xdg.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", xdg.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", xdg.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", xdg.join("units"))
        .env("XDG_CONFIG_HOME", xdg)
        .env("XDG_STATE_HOME", xdg)
        .args(args)
        .output()
        .expect("the binary runs");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// How long a run that must end on its own is given before it is killed.
const DEADLINE: Duration = Duration::from_secs(10);

/// How often a run that has not finished is looked at again.
const POLL: Duration = Duration::from_millis(20);

/// Run the CLI with `xdg` as its config home, and give up on it after [`DEADLINE`].
///
/// Standard input is a pipe nothing ever writes to, so a verb that stopped to read an answer
/// would still be waiting when the deadline passed; `None` is that verb, and the child is
/// killed rather than left behind. Everything else is [`svolder_in`]'s environment.
fn svolder_until(args: &[&str], xdg: &Path) -> Option<Run> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/")
        .env("SVOLDER_SOCKET", xdg.join("no-daemon.sock"))
        .env("SVOLDER_SYSTEMCTL", xdg.join("no-systemctl"))
        .env("SVOLDER_WIREPLUMBER", xdg.join("no-wireplumber"))
        .env("SVOLDER_UNITDIR", xdg.join("units"))
        .env("XDG_CONFIG_HOME", xdg)
        .env("XDG_STATE_HOME", xdg)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    let until = Instant::now() + DEADLINE;
    loop {
        match child.try_wait().expect("the child can be waited on") {
            Some(status) => {
                // The child has already exited, so its output is whole and no reader of one
                // pipe is keeping it from writing the other.
                let out = child.wait_with_output().expect("the child's output");
                return Some(Run {
                    code: status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                });
            }
            None if Instant::now() < until => std::thread::sleep(POLL),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Run the CLI with a config home of its own and no configuration file in it.
fn svolder(args: &[&str]) -> Run {
    let home = config_home();
    svolder_in(args, &home.0)
}

/// Run the CLI with `config` as the whole of its `config.toml`.
fn svolder_with_config(args: &[&str], config: &str) -> Run {
    let home = config_home();
    let dir = home.0.join("svolder");
    std::fs::create_dir_all(&dir).expect("the config directory is made");
    std::fs::write(dir.join("config.toml"), config).expect("the config file is written");
    svolder_in(args, &home.0)
}

/// The exit code a `raw` write the device confirms leaves with, which is what `raw 7.1 u8 0`
/// earns against the fake.
fn svolder_old_raw_code() -> i32 {
    0
}

#[test]
fn help_groups_the_verbs_by_task_in_the_spec_order() {
    let out = svolder(&["--help"]).stdout;
    let pos = |s: &str| out.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(pos("Getting started") < pos("setup") && pos("setup") < pos("doctor"));
    assert!(pos("doctor") < pos("scan"));
    assert!(pos("Everyday") < pos("tui"));
    assert!(pos("Devices") < pos("devices"));
    assert!(pos("Daemon") < pos("daemon"));
    assert!(pos("Diagnostics") < pos("inspect") && pos("inspect") < pos("raw"));
    assert!(!out.contains("probe"), "{out}");
}

#[test]
fn help_env_prints_the_table_and_top_level_help_points_at_it() {
    let help = svolder(&["--help"]).stdout;
    assert!(
        help.contains("Run `svolder help env` to list the environment variables it reads."),
        "{help}"
    );

    let env = svolder(&["help", "env"]);
    assert_eq!(env.code, 0, "err={}", env.stderr);
    assert!(env.stdout.contains("SVOLDER_TRANSPORT"), "{}", env.stdout);
    assert!(env.stdout.contains("SVOLDER_SOCKET"), "{}", env.stdout);
    assert!(
        env.stdout.contains("For tests and development:"),
        "{}",
        env.stdout
    );
    assert!(env.stdout.contains("SVOLDER_SYSTEMCTL"), "{}", env.stdout);

    let markdown = svolder(&["help", "env", "--markdown"]);
    assert_eq!(markdown.code, 0, "err={}", markdown.stderr);
    assert!(
        markdown
            .stdout
            .starts_with("| Variable | Read by | What it does |"),
        "{}",
        markdown.stdout
    );
}

/// `help env`'s interception ahead of `Cli::try_parse` touches nothing else: a bare `help`, a
/// `help <command>`, and a `<command> --help` all still work exactly as they did before it.
#[test]
fn help_and_a_commands_own_help_still_work_the_way_they_did_before_help_env() {
    let bare = svolder(&["help"]);
    assert_eq!(bare.code, 0, "err={}", bare.stderr);
    assert!(bare.stdout.contains("Getting started"), "{}", bare.stdout);

    // Both runs share one config home so `--socket`'s `[env: SVOLDER_SOCKET=...]` resolves to
    // the same path in each, or the comparison below would fail on that alone.
    let home = config_home();
    let daemon = svolder_in(&["help", "daemon"], &home.0);
    assert_eq!(daemon.code, 0, "err={}", daemon.stderr);
    let daemon_flag = svolder_in(&["daemon", "--help"], &home.0);
    assert_eq!(daemon_flag.code, 0, "err={}", daemon_flag.stderr);
    assert_eq!(daemon.stdout, daemon_flag.stdout);

    // clap's own `help` rejects a topic it has no subcommand for; `env` still is not one once
    // anything follows it, so a `help env` this binary does not intercept reaches clap exactly
    // as before and is refused the same way.
    let unknown = svolder(&["help", "env", "extra"]);
    assert_ne!(unknown.code, 0);
}

#[test]
fn bare_svolder_off_a_terminal_prints_the_summary_and_exits_zero() {
    let run = svolder_with_config(
        &[],
        "schema = 1\n[[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\n",
    );
    assert_eq!(run.code, 0, "out={} err={}", run.stdout, run.stderr);
    assert!(run.stdout.contains("amp"), "{}", run.stdout);
    assert!(run.stdout.contains("svolder tui"), "{}", run.stdout);
    let none = svolder_with_config(&[], "schema = 1\n");
    assert_eq!(none.code, 0, "err={}", none.stderr);
    assert!(none.stdout.contains("svolder setup"), "{}", none.stdout);
}

#[test]
fn scan_prints_a_ready_add_line_per_device() {
    let run = svolder(&["scan", "--timeout", "1s"]);
    assert_eq!(run.code, 0, "err={}", run.stderr);
    assert!(
        run.stdout.contains("svolder add 0102030405060708"),
        "{}",
        run.stdout
    );
}

#[test]
fn scan_json_prints_objects_and_no_add_line() {
    let run = svolder(&["scan", "--timeout", "1s", "--json"]);
    assert_eq!(run.code, 0, "err={}", run.stderr);
    for line in run.stdout.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("one JSON object per line");
    }
}

#[test]
fn renamed_verbs_answer_and_old_names_do_not() {
    assert_eq!(svolder(&["inspect"]).code, 0);
    assert_ne!(svolder(&["probe"]).code, 0);
    assert_eq!(
        svolder(&["raw", "7.1", "u8", "0"]).code,
        svolder_old_raw_code()
    );
    assert_eq!(svolder(&["config", "template"]).stdout, Config::template());
}

/// Off a terminal `setup` prints what a terminal would have been asked, and asks nothing: the
/// host's health, every device in range with the `add` line that configures it, and what to do
/// about the daemon.
///
/// The child's standard input is closed — [`svolder`] runs it through `Command::output`, which
/// gives the child `/dev/null` — so a run that stopped to read an answer would be answered by
/// the end of the file rather than by a user, and the exact `add` line below would never be
/// printed.
///
/// The daemon line is the one the run's own environment earns: the `systemctl` these cases
/// pin is not a program, so the run sees no systemd to install user units into whatever this
/// machine has, and is told what applies to a session without one instead.
#[test]
fn setup_off_a_terminal_prints_the_add_line_and_the_daemon_line() {
    let run = svolder(&["setup"]);
    assert_eq!(run.code, 0, "out={} err={}", run.stdout, run.stderr);
    let marked = run
        .stdout
        .lines()
        .filter(|line| ["✓ ", "! ", "✗ ", "· "].iter().any(|m| line.starts_with(m)))
        .count();
    assert!(marked >= 5, "out={} err={}", run.stdout, run.stderr);
    assert!(
        run.stdout
            .contains("svolder add 0102030405060708 --name \"Gungnir 2\""),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("no systemd to install user units into"),
        "{}",
        run.stdout
    );
    assert!(
        !run.stdout.contains("svolder daemon install"),
        "a session with no systemd was sent to the verb that installs user units: {}",
        run.stdout
    );
}

/// `doctor --json` answers with one array of the checks it ran, each row the five fields the
/// checks serialise, and a host with no configuration file warns on the config row — so the
/// run cannot exit 0.
///
/// The exit code is read back from the rows rather than fixed at 1, because which checks a
/// host passes is the host's business: whatever the rows say, the code is the one they ask
/// for.
#[test]
fn doctor_json_is_one_array_of_checks_and_a_missing_config_warns() {
    let run = svolder(&["doctor", "--json"]);
    let outcomes: Vec<serde_json::Value> =
        serde_json::from_str(&run.stdout).expect("one JSON array of checks");
    for outcome in &outcomes {
        let keys: Vec<&str> = outcome
            .as_object()
            .expect("one object per check")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["applies_itself", "check", "fix", "reason", "state"],
            "{outcome}"
        );
    }
    let at = |check: &str| outcomes.iter().position(|o| o["check"] == check);
    let config = at("Config").expect("a Config check");
    assert_eq!(outcomes[config]["state"], "warn", "{}", run.stdout);

    // With no file there is a row offering to write one, right under the config check, and
    // `--yes` has it in the array it may apply.
    let file = at("Config file").expect("a Config file check");
    assert_eq!(file, config + 1, "{}", run.stdout);
    assert_eq!(outcomes[file]["state"], "warn", "{}", run.stdout);
    assert_eq!(outcomes[file]["applies_itself"], true, "{}", run.stdout);
    assert!(
        outcomes[file]["fix"]
            .as_str()
            .is_some_and(|fix| fix.contains("from the template")),
        "{}",
        run.stdout
    );

    // Nothing is configured, so there is no device to look for and no adapter to grade
    // against one.
    assert_eq!(at("Devices"), None, "{}", run.stdout);
    assert_eq!(at("Adapter grade"), None, "{}", run.stdout);

    let failed = outcomes.iter().any(|o| o["state"] == "fail");
    assert_eq!(
        run.code,
        if failed { 2 } else { 1 },
        "out={} err={}",
        run.stdout,
        run.stderr
    );
}

/// Off a terminal `doctor` prints its marked lines and leaves: a pipe is never offered the
/// fixes, so nothing waits on an answer that will not come.
#[test]
fn doctor_on_a_pipe_prints_marked_lines_and_never_blocks() {
    let home = config_home();
    let run = svolder_until(&["doctor"], &home.0).expect("doctor ends on its own off a terminal");
    let marked = run
        .stdout
        .lines()
        .filter(|line| ["✓ ", "! ", "✗ ", "· "].iter().any(|m| line.starts_with(m)))
        .count();
    assert!(marked >= 5, "out={} err={}", run.stdout, run.stderr);
    assert!(run.stdout.contains("Config —"), "{}", run.stdout);
    assert!(!run.stdout.contains("[y/N]"), "{}", run.stdout);
    assert!(!run.stderr.contains("[y/N]"), "{}", run.stderr);
}

/// `--yes` applies the fixes doctor can apply itself without asking, and on a host with no
/// configuration file one of them is the file itself, written once from the template.
#[test]
fn doctor_yes_creates_the_config_from_the_template() {
    let home = config_home();
    let run = svolder_until(&["doctor", "--yes"], &home.0).expect("doctor ends on its own");
    let written = std::fs::read_to_string(home.0.join("svolder").join("config.toml"))
        .expect("a configuration file was written");
    assert_eq!(written, Config::template(), "err={}", run.stderr);
    assert_eq!(
        run.stderr.matches("from the template").count(),
        1,
        "the file was offered more than once: {}",
        run.stderr
    );
    // The `systemctl` this run is pointed at is not a program, so it sees no systemd at all:
    // the units fix is not among the ones it is offered, nothing was written into the unit
    // directory the test named, and nothing of this machine's was enabled.
    assert!(
        !home.0.join("units").exists(),
        "a unit file was written: {:?}",
        std::fs::read_dir(home.0.join("units"))
            .map(|entries| entries.count())
            .unwrap_or_default()
    );
    assert!(
        !run.stderr.contains("enabled svolderd") && !run.stderr.contains(".socket"),
        "a unit was installed: {}",
        run.stderr
    );
}

/// `--transport daemon` is a value the front door takes, and with no daemon to reach it says
/// that rather than that it never heard of the transport.
///
/// Autospawn is off in the configuration, so the run ends at the last step of the chain
/// instead of starting a daemon of its own: what this case is about is the value being known.
#[test]
fn transport_daemon_is_accepted() {
    let run = svolder_with_config(
        &["--transport", "daemon", "scan", "--timeout", "1s"],
        "schema = 1\n[daemon]\nautospawn = false\n",
    );
    assert!(
        !run.stderr.contains("unknown transport"),
        "err={}",
        run.stderr
    );
    assert!(run.stderr.contains("no daemon"), "err={}", run.stderr);
}

/// A `--transport` naming nowhere to run is refused before the front door chooses a path, so a
/// mistyped one is answered rather than quietly given the summary.
#[test]
fn a_mistyped_transport_with_no_verb_exits_one() {
    let run = svolder(&["--transport", "nope"]);
    assert_eq!(run.code, 1, "out={} err={}", run.stdout, run.stderr);
    assert!(run.stderr.contains("unknown transport"), "{}", run.stderr);
    assert!(
        run.stderr.contains("See `svolder --help`"),
        "{}",
        run.stderr
    );
    assert!(run.stdout.is_empty(), "{}", run.stdout);
}

/// `svolder help` is another way of asking for `--help`, and prints the same thing.
///
/// Both runs share one config home, because the help repeats what the environment holds for
/// each flag that reads one — the socket path among them.
#[test]
fn help_as_a_verb_prints_the_help() {
    let home = config_home();
    let run = svolder_in(&["help"], &home.0);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(run.stdout, svolder_in(&["--help"], &home.0).stdout);
}

#[test]
fn a_mistyped_verb_exits_one() {
    let run = svolder(&["nosuchverb"]);
    assert_eq!(run.code, 1);
    assert!(!run.stderr.is_empty(), "{}", run.stderr);
}

#[test]
fn decodes_a_notification() {
    let run = svolder(&["decode", "0300c102030200b1057efe"]);
    assert_eq!(run.code, 0, "{}", run.stdout);
    assert!(run.stdout.contains("Status 2 u8 3"), "{}", run.stdout);
    assert!(run.stdout.contains("Status 5 bool true"), "{}", run.stdout);
}

#[test]
fn decodes_a_command_frame_and_reports_the_passphrase_field() {
    let run = svolder(&["decode", "39 af fa 35 03 00 41 02 03 b6 ff"]);
    assert_eq!(run.code, 0, "{}", run.stdout);
    assert!(
        run.stdout.contains("passphrase field 0x35faaf39"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("Command 2 u8 3"), "{}", run.stdout);
}

#[test]
fn names_functions_with_a_profile() {
    let run = svolder(&[
        "decode",
        "--profile",
        "gungnir2-v2",
        "0300c102030200b1057efe",
    ]);
    assert_eq!(run.code, 0, "{}", run.stdout);
    assert!(run.stdout.contains("Input = 3 (Optical)"), "{}", run.stdout);
    assert!(run.stdout.contains("Mute = on"), "{}", run.stdout);
}

#[test]
fn names_a_lock_flag_on_a_gungnir_2() {
    // Status bool true on function 8, the S/PDIF-locked flag.
    let run = svolder(&["decode", "--profile", "gungnir2-v2", "0200b10844ff"]);
    assert_eq!(run.code, 0, "{}", run.stdout);
    assert!(
        run.stdout.contains("S/PDIF locked"),
        "the flag was not named: {}",
        run.stdout
    );
    let run = svolder(&[
        "decode",
        "--json",
        "--profile",
        "gungnir2-v2",
        "0200b10844ff",
    ]);
    let v: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(v["messages"][0]["id"], "8");
    assert_eq!(v["messages"][0]["key"], "spdif_locked");
    assert_eq!(v["messages"][0]["raw"], true);
}

#[test]
fn json_output() {
    let run = svolder(&[
        "decode",
        "--json",
        "--profile",
        "yggdrasil",
        "0600c7d2020103005afe",
    ]);
    assert_eq!(run.code, 0, "{}", run.stdout);
    let v: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(v["messages"][0]["id"], "210.2.1");
    assert_eq!(v["messages"][0]["raw"], 3);
    assert_eq!(v["messages"][0]["label"], "?");

    // Status u8 109 on function 10, the Gungnir 2 volume, with its trailing checksum.
    let run = svolder(&[
        "decode",
        "--json",
        "--profile",
        "gungnir2-v2",
        "0300c10a6dc4fe",
    ]);
    assert_eq!(run.code, 0, "{}", run.stdout);
    let v: serde_json::Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(v["messages"][0]["raw"], 109);
    assert_eq!(v["messages"][0]["value"], -6.021);
}

#[test]
fn bad_hex_and_bad_checksum_exit_one() {
    let run = svolder(&["decode", "zz"]);
    assert_eq!(run.code, 1);
    assert!(run.stderr.contains("hex"), "{}", run.stderr);
    let run = svolder(&["decode", "0400c102037dfe"]);
    assert_eq!(run.code, 1);
    assert!(run.stderr.contains("checksum"), "{}", run.stderr);
}

#[test]
fn profiles_list_and_show() {
    let run = svolder(&["profiles", "list"]);
    assert_eq!(run.code, 0);
    assert!(
        run.stdout.lines().any(|l| l.starts_with("bifrost3 ")),
        "{}",
        run.stdout
    );
    let run = svolder(&["profiles", "show", "freya"]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.contains("volume"), "{}", run.stdout);
    assert!(run.stdout.contains("6 "), "{}", run.stdout);
    let run = svolder(&["profiles", "show", "nope"]);
    assert_eq!(run.code, 1);
    assert!(run.stderr.contains("nope"), "{}", run.stderr);
}

/// A meter's scale shows under its row: a fixed zero as a number, a floor-following one by the
/// readout it follows.
#[test]
fn profiles_show_prints_a_meters_scale() {
    let run = svolder(&["profiles", "show", "fake-amp"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .lines()
            .any(|l| l.trim() == "scale zero 160, full 350, class-A 0.25"),
        "{}",
        run.stdout
    );
    let run = svolder(&["profiles", "show", "tyr"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .lines()
            .any(|l| l.trim() == "scale zero from meter_floor, full 1000, class-A 0.1"),
        "{}",
        run.stdout
    );
}

#[test]
fn config_path_prints_a_path() {
    let run = svolder(&["config", "path"]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.trim().ends_with("svolder"), "{}", run.stdout);
}

#[test]
fn help_exits_zero() {
    let run = svolder(&["--help"]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.contains("decode"), "{}", run.stdout);
}

/// `daemon restart --if-running` with no daemon and no units says there is nothing to restart,
/// succeeds, and starts none.
#[test]
fn daemon_restart_if_running_with_no_daemon_starts_none() {
    let home = config_home();
    std::fs::create_dir_all(&home.0).expect("the home exists");
    let run = svolder_in(&["daemon", "restart", "--if-running"], &home.0);
    assert_eq!(run.code, 0, "err={}", run.stderr);
    let socket = home.0.join("no-daemon.sock");
    assert!(
        run.stderr.contains(&format!(
            "svolder: no daemon answers on {}; nothing to restart",
            socket.display()
        )),
        "err={}",
        run.stderr
    );
    assert!(
        !socket.exists(),
        "a daemon was started on {}",
        socket.display()
    );
}

/// `daemon logs` reads the log whatever the configuration says: a file that does not parse is
/// what a daemon that refused a reload sends people here for, so the socket asked is the
/// default one and the autospawn log is read.
#[test]
fn daemon_logs_reads_the_log_when_the_configuration_does_not_parse() {
    let home = config_home();
    let dir = home.0.join("svolder");
    std::fs::create_dir_all(&dir).expect("the directories exist");
    std::fs::write(dir.join("config.toml"), "this is = not [valid toml\n")
        .expect("the broken configuration is written");
    std::fs::write(dir.join("svolderd.log"), "a line the daemon logged\n")
        .expect("the log is written");
    let run = svolder_in(&["daemon", "logs"], &home.0);
    assert_eq!(run.code, 0, "err={}", run.stderr);
    assert!(
        run.stdout.contains("a line the daemon logged"),
        "out={} err={}",
        run.stdout,
        run.stderr
    );
}
