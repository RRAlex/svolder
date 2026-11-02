// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::fd::AsRawFd;
use std::os::unix::fs::DirBuilderExt;
use std::time::{Duration, SystemTime};

use super::*;

/// A directory of this case's own to sweep.
fn base(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("svolder-sweep-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the base");
    dir
}

/// A demo directory under `base`, last touched `age` ago, with a lock file when `locked` is
/// given, held while the returned file lives.
fn leftover(
    base: &Path,
    name: &str,
    age: Duration,
    locked: bool,
    marked: bool,
) -> (PathBuf, Option<File>) {
    let dir = base.join(name);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .expect("create the directory");
    if marked {
        std::fs::write(dir.join(MARKER), MARKER_LINE).expect("the marker");
    }
    let held = locked.then(|| {
        let lock = File::create(dir.join(format!("{SOCKET_FILE}.lock"))).expect("a lock file");
        // SAFETY: the descriptor belongs to `lock`, which is returned and so outlives the lock.
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        lock
    });
    File::open(&dir)
        .expect("open the directory")
        .set_modified(SystemTime::now() - age)
        .expect("set the age");
    (dir, held)
}

/// Only a marked, old, unlocked, exactly-named demo directory is removed: a live daemon's lock, a
/// recent directory, another name, a missing marker and a symlink all stay.
#[test]
fn the_sweep_takes_only_dead_old_marked_demo_directories() {
    let base = base("sweep");
    let old = Duration::from_secs(3600);
    let (dead, _) = leftover(&base, "sd-aaaaaa", old, false, true);
    // The lock file a daemon that is gone leaves, with nobody holding it. A lock taken here and
    // let go would stay held by any child another test's spawn forked while it was held.
    let (dead_lock, _) = leftover(&base, "sd-bbbbbb", old, false, true);
    File::create(dead_lock.join(format!("{SOCKET_FILE}.lock"))).expect("a lock file");
    File::open(&dead_lock)
        .expect("open")
        .set_modified(SystemTime::now() - old)
        .expect("age");
    let (alive, _held) = leftover(&base, "sd-cccccc", old, true, true);
    let (fresh, _) = leftover(&base, "sd-dddddd", Duration::ZERO, false, true);
    let (other, _) = leftover(&base, "keep-eeeeee", old, false, true);
    let (card, _) = leftover(&base, "sd-card", old, false, true);
    let (card_backup, _) = leftover(&base, "sd-card-backup", old, false, true);
    let (unmarked, _) = leftover(&base, "sd-abc123", old, false, false);
    let (upper, _) = leftover(&base, "sd-ABC123", old, false, true);
    // A symlink named like a demo directory, to a directory that would otherwise qualify.
    let target = base.join("target-dir");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&target)
        .expect("target");
    std::fs::write(target.join(MARKER), MARKER_LINE).expect("marker");
    File::open(&target)
        .expect("open")
        .set_modified(SystemTime::now() - old)
        .expect("age");
    let link = base.join("sd-fedcba");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    sweep(&base);
    assert!(!dead.exists());
    assert!(!dead_lock.exists());
    assert!(alive.exists(), "a daemon holds its lock");
    assert!(fresh.exists(), "too recent to be a leftover");
    for kept in [&other, &card, &card_backup, &unmarked, &upper] {
        assert!(kept.exists(), "{} is not a demo's", kept.display());
    }
    assert!(target.join(MARKER).exists(), "the symlink was followed");
    assert!(link.exists(), "the symlink was removed");
    let _ = std::fs::remove_dir_all(&base);
}

/// The names the demo makes are the names the sweep takes, and no others.
#[test]
fn only_the_demos_own_names_are_demo_names() {
    let base = base("names");
    let made = private_dir(&base).expect("a directory");
    assert!(demo_name(&made.file_name().unwrap().to_string_lossy()));
    for no in [
        "sd-card",
        "sd-abc12",
        "sd-abc1234",
        "sd-ABC123",
        "x-abc123",
        "sd-abc12g",
        "sd-",
    ] {
        assert!(!demo_name(no), "{no}");
    }
    let _ = std::fs::remove_dir_all(&base);
}

/// The private directory is 0700 and named short, and two are never the same.
#[test]
fn the_private_directory_is_short_and_private() {
    use std::os::unix::fs::PermissionsExt;
    let base = base("private");
    let one = private_dir(&base).expect("a directory");
    let two = private_dir(&base).expect("another");
    assert_ne!(one, two);
    let mode = std::fs::metadata(&one)
        .expect("exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    let socket = one.join(SOCKET_FILE);
    assert!(socket.to_string_lossy().len() < 108);
    let _ = std::fs::remove_dir_all(&base);
}

/// The demo's TUI process turns the host's audio off whatever the caller said, since its
/// controls would change the user's own volume and players, and reads its homes from the
/// directory. (Its daemon runs a fake host instead.)
#[test]
fn the_demo_environment_has_no_host_and_its_own_homes() {
    let dir = Path::new("/tmp/sd-abcdef");
    let env = isolated_env(dir);
    let get = |name: &str| env.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
    assert_eq!(get("SVOLDER_HOST"), Some(PathBuf::from("off")));
    assert_eq!(get("SVOLDER_TRANSPORT"), Some(PathBuf::from("fake")));
    assert_eq!(get("XDG_CONFIG_HOME"), Some(dir.join("config")));
    assert_eq!(get("XDG_STATE_HOME"), Some(dir.join("state")));
    assert_eq!(get("XDG_DATA_HOME"), Some(dir.join("data")));
    assert_eq!(get("XDG_RUNTIME_DIR"), Some(dir.to_path_buf()));
    assert_eq!(get("SVOLDER_SYSTEMCTL"), Some(dir.join("no-systemctl")));
    assert_eq!(get("SVOLDER_UNITDIR"), Some(dir.join("units")));
}

/// The fake device's profile, faults and meter signal reach the demo daemon, so `--demo` can
/// try an amplifier with its meters playing.
#[test]
fn the_fake_devices_profile_faults_and_signal_reach_the_demo_daemon() {
    for knob in [
        "SVOLDER_FAKE_PROFILE",
        "SVOLDER_FAKE_FAULTS",
        "SVOLDER_FAKE_SIGNAL",
    ] {
        assert!(PASSED_ON.contains(&knob), "{knob}");
    }
}

/// The knobs that make the fake a Loki Max with the other report form, or a recall that reports
/// nothing, reach the demo daemon, so `SVOLDER_FAKE_PROFILE=lokimax make demo` can show one.
#[test]
fn the_demo_passes_the_loki_knobs_on() {
    for knob in ["SVOLDER_FAKE_LOKI_REPORTS", "SVOLDER_FAKE_SILENT_RECALL"] {
        assert!(PASSED_ON.contains(&knob), "{knob}");
    }
    let registry = svolder_profiles::Registry::builtin().expect("the built-in profiles");
    assert!(
        registry.get("lokimax").is_some(),
        "the profile the demo is asked for exists"
    );
}

/// The cases that start a demo share the interrupt flag, so they take turns.
static STARTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A demo whose "daemon" is the shell command `script`, in a directory under a base of its own.
fn demo_running(name: &str, script: &str) -> (Demo, PathBuf) {
    let base = base(name);
    let dir = private_dir(&base).expect("a directory");
    let log = File::create(dir.join("daemon.log")).expect("a log");
    let child = Command::new("sh")
        .args(["-c", script])
        .stdout(Stdio::from(log.try_clone().expect("clone")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn sh");
    let demo = Demo {
        child: Some(child),
        dir,
        keep: false,
    };
    (demo, base)
}

/// A daemon that stops while the demo starts leaves the directory, and the message names the
/// daemon's own log rather than `svolder daemon logs`.
#[test]
fn a_demo_that_fails_to_start_keeps_its_directory_and_names_its_log() {
    let _turn = STARTING.lock().unwrap_or_else(|e| e.into_inner());
    let (mut demo, base) = demo_running("fails", "echo boom; exit 3");
    let dir = demo.dir.clone();
    let Err(StartError::Failed(failure)) = demo.wait_until_up() else {
        panic!("the start did not fail");
    };
    assert!(failure.what.contains("boom"), "{failure}");
    assert_eq!(
        failure.next,
        format!(
            "The demo daemon's log is {}",
            dir.join("daemon.log").display()
        )
    );
    assert!(!failure.to_string().contains("svolder daemon logs"));
    drop(demo);
    assert!(dir.join("daemon.log").exists(), "the log was removed");
    let _ = std::fs::remove_dir_all(&base);
}

/// A signal during the start is the quiet end the plain TUI gives an interrupt, not a failure,
/// and the directory goes.
#[test]
fn an_interrupt_during_the_start_is_not_a_failure() {
    let _turn = STARTING.lock().unwrap_or_else(|e| e.into_inner());
    let (mut demo, base) = demo_running("interrupted", "sleep 30");
    let dir = demo.dir.clone();
    INTERRUPTED.store(true, Ordering::SeqCst);
    let ended = demo.wait_until_up();
    INTERRUPTED.store(false, Ordering::SeqCst);
    assert!(matches!(ended, Err(StartError::Interrupted)));
    drop(demo);
    assert!(!dir.exists());
    let _ = std::fs::remove_dir_all(&base);
}

/// Dropping a demo whose daemon has already exited and been reaped removes the directory and
/// signals nothing.
#[test]
fn dropping_a_demo_with_a_reaped_daemon_removes_the_directory() {
    let (mut demo, base) = demo_running("reaped", "exit 0");
    let dir = demo.dir.clone();
    let child = demo.child.as_mut().expect("a child");
    while child.try_wait().expect("wait").is_none() {
        std::thread::sleep(POLL);
    }
    drop(demo);
    assert!(!dir.exists());
    let _ = std::fs::remove_dir_all(&base);
}
