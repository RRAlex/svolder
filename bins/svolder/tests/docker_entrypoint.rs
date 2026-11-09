// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The decisions of `packaging/docker-entrypoint.sh`, with `id`, `stat`, `chown`, `setpriv` and
//! `svolderd` replaced by fakes that log their arguments, and the uid map and the two folders
//! given through the script's overridable inputs: who it leaves alone, which folder it re-owns,
//! and how it drops.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

mod common;

use common::TempPath;

struct Case<'a> {
    /// What the fake `id -u` prints.
    uid: &'a str,
    /// The first line of the fake `/proc/self/uid_map`.
    uid_map: &'a str,
    /// The fake owner of the folders `a` and `b`.
    owners: (&'a str, &'a str),
    env: &'a [(&'a str, &'a str)],
    /// What the fake `id -G` prints.
    groups: &'a str,
    /// The name the script is run as (`svolder` is the CLI wrapper).
    name: &'a str,
}

const IDENTITY: &str = "         0          0 4294967295";

fn fake(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write a fake");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod a fake");
}

/// Run the script and return what the fakes logged, one line per call.
fn run(case: &Case) -> String {
    run_full(case).0
}

/// [`run`], and what the script wrote to stderr.
fn run_full(case: &Case) -> (String, String) {
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = TempPath(std::env::temp_dir().join(format!(
        "svolder-docker-entry-test-{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
    )));
    let bin = dir.0.join("bin");
    fs::create_dir_all(&bin).expect("create the fake bin");
    let log = dir.0.join("log");
    fs::write(&log, "").expect("create the log");
    fake(
        &bin,
        "id",
        &format!(
            "if [ \"$1\" = -G ]; then echo \"{}\"; else echo {}; fi",
            case.groups, case.uid
        ),
    );
    // `stat -c %u <dir>`: the owner is the file `owner.<last component>` beside the log.
    fake(
        &bin,
        "stat",
        &format!("cat \"{}/owner.$(basename \"$3\")\"", dir.0.display()),
    );
    for tool in ["chown", "setpriv", "svolderd", "svolder-real"] {
        fake(
            &bin,
            tool,
            &format!("echo \"{tool} $*\" >> \"{}\"", log.display()),
        );
    }
    fs::create_dir_all(dir.0.join("a")).expect("folder a");
    fs::create_dir_all(dir.0.join("b")).expect("folder b");
    fs::write(dir.0.join("owner.a"), case.owners.0).expect("owner a");
    fs::write(dir.0.join("owner.b"), case.owners.1).expect("owner b");
    fs::write(dir.0.join("uid_map"), format!("{}\n", case.uid_map)).expect("uid map");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/docker-entrypoint.sh");
    // The script is run through a symlink of the given name, as the image installs it twice.
    let link = dir.0.join(case.name);
    std::os::unix::fs::symlink(&script, &link).expect("link the script");
    let out = Command::new("sh")
        .arg(&link)
        .args(["--flag", "x y"])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("ENTRY_TEST_UID_MAP", dir.0.join("uid_map"))
        .env("ENTRY_TEST_DIRS", format!("{0}/a {0}/b", dir.0.display()))
        .env("ENTRY_TEST_CLI", bin.join("svolder-real"))
        .envs(case.env.iter().copied())
        .output()
        .expect("run the entrypoint");
    assert!(out.status.success(), "{out:?}");
    let shown = |text: String| text.replace(&format!("{}", dir.0.display()), "D");
    (
        shown(fs::read_to_string(&log).expect("read the log")),
        shown(String::from_utf8_lossy(&out.stderr).into_owned()),
    )
}

fn case<'a>(uid: &'a str, uid_map: &'a str, owners: (&'a str, &'a str)) -> Case<'a> {
    Case {
        uid,
        uid_map,
        owners,
        env: &[],
        groups: "0 5",
        name: "entrypoint",
    }
}

#[test]
fn a_non_root_start_runs_the_daemon_unchanged() {
    let log = run(&case("1000", IDENTITY, ("0", "0")));
    assert_eq!(log, "svolderd --flag x y\n");
}

#[test]
fn a_rootless_container_neither_chowns_nor_drops() {
    let log = run(&case("0", "         0       1000          1", ("0", "0")));
    assert_eq!(log, "svolderd --flag x y\n");
}

#[test]
fn root_owned_folders_are_repaired_and_the_daemon_drops_to_1000() {
    let log = run(&case("0", IDENTITY, ("0", "0")));
    assert_eq!(
        log,
        "chown -R 1000:1000 D/a\nchown -R 1000:1000 D/b\n\
         setpriv --reuid=1000 --regid=1000 --groups=5 svolderd --flag x y\n"
    );
}

#[test]
fn puid_and_pgid_name_the_owner_and_the_user() {
    let mut c = case("0", IDENTITY, ("0", "0"));
    c.env = &[("PUID", "1234"), ("PGID", "5678")];
    let log = run(&c);
    assert!(log.contains("chown -R 1234:5678 D/a\n"), "{log}");
    assert!(
        log.ends_with("setpriv --reuid=1234 --regid=5678 --groups=5 svolderd --flag x y\n"),
        "{log}"
    );
}

#[test]
fn a_folder_owned_by_anyone_else_is_never_re_owned() {
    let log = run(&case("0", IDENTITY, ("1000", "0")));
    assert_eq!(
        log,
        "chown -R 1000:1000 D/b\n\
         setpriv --reuid=1000 --regid=1000 --groups=5 svolderd --flag x y\n"
    );
    let log = run(&case("0", IDENTITY, ("4242", "4242")));
    assert!(!log.contains("chown"), "{log}");
}

#[test]
fn the_cli_wrapper_drops_without_chowning() {
    let mut c = case("0", IDENTITY, ("0", "0"));
    c.name = "svolder";
    let log = run(&c);
    assert!(!log.contains("chown"), "{log}");
    assert!(
        log.contains("setpriv --reuid=1000 --regid=1000 --groups=5 D/bin/svolder-real --flag x y"),
        "{log}"
    );
}

#[test]
fn the_cli_wrapper_as_a_non_root_user_runs_the_binary() {
    let mut c = case("1000", IDENTITY, ("0", "0"));
    c.name = "svolder";
    assert_eq!(run(&c), "svolder-real --flag x y\n");
}

#[test]
fn root_s_group_zero_is_not_carried_and_no_groups_means_cleared() {
    let mut c = case("0", IDENTITY, ("0", "0"));
    c.groups = "0";
    let log = run(&c);
    assert!(
        log.ends_with("setpriv --reuid=1000 --regid=1000 --clear-groups svolderd --flag x y\n"),
        "{log}"
    );
    c.groups = "0 12 34";
    let log = run(&c);
    assert!(log.contains("--groups=12,34 "), "{log}");
}

#[test]
fn a_folder_owned_by_another_uid_is_named_on_stderr() {
    let (log, err) = run_full(&case("0", IDENTITY, ("4242", "1000")));
    assert!(!log.contains("chown"), "{log}");
    assert_eq!(
        err,
        "svolder: D/a is owned by uid 4242 but the daemon runs as 1000: set HOST_UID=4242 \
         (and HOST_GID) in .env, or chown the folder\n"
    );
    let (_, err) = run_full(&case("0", IDENTITY, ("0", "1000")));
    assert_eq!(err, "");
}
