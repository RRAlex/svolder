// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::test_support::TempPath;

/// A directory of one case's own, removed with what is in it when the case ends.
fn scratch(what: &str) -> TempPath {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let at = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("svolder-{what}-{}-{at}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the temp directory is made");
    TempPath(dir)
}

/// A second lock on the same file waits for the first, and gives up after its wait saying
/// another svolder is writing; once the first goes, the next is taken at once.
#[test]
fn a_second_lock_waits_its_turn_and_gives_up_after_the_wait() {
    let dir = scratch("flock-wait");
    let path = dir.join("config.toml");
    let first = lock_beside(&path, Wait::Not).expect("the first lock");
    let started = Instant::now();
    let err = lock_beside(&path, Wait::Until(Duration::from_millis(200))).unwrap_err();
    assert!(started.elapsed() >= Duration::from_millis(200));
    assert_eq!(err.kind(), ErrorKind::WouldBlock);
    assert_eq!(err.to_string(), BUSY);
    assert_eq!(
        lock_beside(&path, Wait::Not).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    drop(first);
    lock_beside(&path, Wait::Until(Duration::from_millis(200))).expect("free once dropped");
}

/// The lock file is private to its user and stays where it is when the lock goes.
#[test]
fn the_lock_file_is_private_and_stays_after_the_lock_goes() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("flock-mode");
    let path = dir.join("config.toml");
    drop(lock_beside(&path, Wait::Not).expect("a lock"));
    let lock = dir.join("config.toml.lock");
    let mode = std::fs::metadata(&lock).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode & 0o077, 0, "{mode:o}");
}

/// A symbolic link where the lock file goes is refused rather than followed.
#[test]
fn a_symbolic_link_where_the_lock_file_goes_is_refused() {
    let dir = scratch("flock-link");
    let path = dir.join("config.toml");
    let elsewhere = dir.join("elsewhere");
    std::fs::write(&elsewhere, "").unwrap();
    std::os::unix::fs::symlink(&elsewhere, dir.join("config.toml.lock")).unwrap();
    let err = lock_beside(&path, Wait::Not).unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::ELOOP));
}
