// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Makefile's choices (unit directory, which tests `make check` runs), read off a dry run.
//!
//! `make -n` prints the recipes without running them, so nothing is built or installed and no
//! `systemctl` runs; `SYSTEMCTL=echo` would keep even a real run to a print.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root, two levels above this crate.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root exists")
}

/// What `make -n <target>` prints for this `PREFIX` and `HOME` and these extra variables.
fn dry_run_of(target: &str, prefix: &str, home: &str, extra: &[&str]) -> String {
    let out = Command::new("make")
        .args(["-n", "-C"])
        .arg(repo_root())
        .arg(target)
        .arg(format!("PREFIX={prefix}"))
        .arg(format!("HOME={home}"))
        .arg("SYSTEMCTL=echo")
        .args(extra)
        .env_remove("MAKEFLAGS")
        .env_remove("MAKELEVEL")
        .env_remove("MFLAGS")
        .output()
        .expect("make runs");
    assert!(out.status.success(), "make -n failed: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// What `make -n install-service` prints for this `PREFIX` and `HOME`.
fn dry_run(prefix: &str, home: &str) -> String {
    dry_run_of("install-service", prefix, home, &[])
}

fn temp_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("svolder-make-test-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp home");
    dir
}

/// A `PREFIX` that spells the user's `~/.local` differently from `$(HOME)/.local` — here
/// because `HOME` ends in a slash — still gets the user unit directory.
#[test]
fn a_home_with_a_trailing_slash_still_gets_the_user_unit_directory() {
    let base = temp_home("slash");
    let home = base.join("home");
    let prefix = home.join(".local");
    let out = dry_run(
        prefix.to_str().expect("utf-8 path"),
        &format!("{}/", home.display()),
    );
    let want = format!("SVOLDER_UNITDIR={}/.config/systemd/user", home.display());
    assert!(out.contains(&want), "wanted {want} in:\n{out}");
    let _ = std::fs::remove_dir_all(base);
}

/// A `PREFIX` spelled through a symlink to the home directory still gets the user unit
/// directory, once the home's `.local` exists.
#[test]
fn a_prefix_through_a_symlinked_home_still_gets_the_user_unit_directory() {
    let base = temp_home("symlink");
    let home = base.join("home");
    std::fs::create_dir_all(home.join(".local")).expect("create the home's .local");
    let link = base.join("link");
    std::os::unix::fs::symlink(&home, &link).expect("link to the home");
    let prefix = link.join(".local");
    let out = dry_run(
        prefix.to_str().expect("utf-8 path"),
        home.to_str().expect("utf-8 path"),
    );
    let want = format!("SVOLDER_UNITDIR={}/.config/systemd/user", home.display());
    assert!(out.contains(&want), "wanted {want} in:\n{out}");
    let _ = std::fs::remove_dir_all(base);
}

/// A system prefix still routes the units to the system directory.
#[test]
fn a_system_prefix_gets_the_system_unit_directory() {
    let base = temp_home("system");
    let home = base.join("home");
    let out = dry_run("/usr/local", &format!("{}/", home.display()));
    assert!(
        out.contains("SVOLDER_UNITDIR=/etc/systemd/user"),
        "out:\n{out}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// `make install` restarts a daemon that is running, through the binary it just installed.
#[test]
fn install_restarts_a_running_daemon_unless_told_not_to() {
    let base = temp_home("restart");
    let home = base.join("home");
    let prefix = home.join(".local");
    let prefix = prefix.to_str().expect("utf-8 path");
    let home = home.to_str().expect("utf-8 path");

    let out = dry_run_of("install", prefix, home, &[]);
    let line = out
        .lines()
        .find(|l| l.contains("svolder daemon restart --if-running"))
        .unwrap_or_else(|| panic!("no restart line in:\n{out}"));
    assert!(line.contains("SVOLDER_SYSTEMCTL=echo"), "line: {line}");
    // A restart that fails is reported and does not fail the install.
    assert!(
        out.contains("make: the daemon was not restarted; run 'svolder daemon restart'"),
        "no fallback message in:\n{out}"
    );
    // As root the recipe restarts nothing and tells each user to do it for their own daemon.
    assert!(out.contains("id -u"), "no root guard in:\n{out}");
    assert!(
        out.contains("Each user restarts their own daemon: svolder daemon restart"),
        "no root message in:\n{out}"
    );

    let out = dry_run_of("install", prefix, home, &["RESTART=0"]);
    assert!(!out.contains("daemon restart"), "out:\n{out}");
    let _ = std::fs::remove_dir_all(base);
}

/// `make screenshots` checks what the build needs, then runs its script and nothing else.
#[test]
fn screenshots_runs_its_script() {
    let base = temp_home("screenshots");
    let home = base.join("home");
    let out = dry_run_of(
        "screenshots",
        "/usr/local",
        home.to_str().expect("utf-8 path"),
        &[],
    );
    let lines: Vec<&str> = out
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("make"))
        .collect();
    assert_eq!(lines.len(), 2, "out:\n{out}");
    assert!(
        lines[0].starts_with("CARGO=")
            && lines[0].ends_with(" sh scripts/build-deps.sh --quiet build"),
        "out:\n{out}"
    );
    assert_eq!(lines[1], "bash scripts/screenshots.sh", "out:\n{out}");
    let _ = std::fs::remove_dir_all(base);
}

/// `CHECK_TESTS` as `make -pn` reads it with only `bin` on the `PATH`.
fn check_tests_with_path(bin: &Path) -> String {
    // The absolute path, since `PATH` is replaced below.
    let out = Command::new("/usr/bin/make")
        .args(["-pn", "-C"])
        .arg(repo_root())
        .env("PATH", bin)
        .env_remove("MAKEFLAGS")
        .env_remove("MAKELEVEL")
        .env_remove("MFLAGS")
        .output()
        .expect("make runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("CHECK_TESTS := "))
        .unwrap_or_else(|| panic!("no CHECK_TESTS in the make database: {out:?}"))
        .to_string()
}

/// `make check` runs the tests once, on the private bus, when dbus-daemon is there, and keeps
/// the plain run followed by `check-no-bluez` when it is not.
#[test]
fn check_runs_the_tests_once_when_dbus_daemon_is_there() {
    use std::os::unix::fs::PermissionsExt;
    let base = temp_home("check-tests");
    let with = base.join("with");
    let without = base.join("without");
    std::fs::create_dir_all(&with).expect("create a bin dir");
    std::fs::create_dir_all(&without).expect("create an empty bin dir");
    let fake = with.join("dbus-daemon");
    std::fs::write(&fake, "#!/bin/sh\n").expect("write a fake dbus-daemon");
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    assert_eq!(check_tests_with_path(&with), "check-tests-private");
    assert_eq!(check_tests_with_path(&without), "test check-no-bluez");
    let _ = std::fs::remove_dir_all(base);
}

/// `make test T=...` hands cargo that selection in place of the workspace, and then does not
/// claim every binary was prebuilt.
#[test]
fn test_takes_a_cargo_selection() {
    let base = temp_home("test-t");
    let home = base.join("home");
    let home = home.to_str().expect("utf-8 path");
    let plain = dry_run_of("test", "/usr/local", home, &[]);
    assert!(
        plain.contains("SVOLDER_TEST_PREBUILT=1")
            && plain.contains("test --workspace --all-features"),
        "out:\n{plain}"
    );
    let one = dry_run_of("test", "/usr/local", home, &["T=-p svolder-host"]);
    assert!(one.contains("test -p svolder-host"), "out:\n{one}");
    assert!(
        !one.contains("--workspace") && !one.contains("PREBUILT"),
        "out:\n{one}"
    );
    let _ = std::fs::remove_dir_all(base);
}
