// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the configuration lives: the config home and the scratch homes of the unit tests.

use super::*;

/// The config directory with the environment as inputs, so no case changes the process's own.
#[test]
fn the_config_home_falls_back_to_the_passwd_home_when_home_is_unset() {
    use std::ffi::OsString;
    let passwd = || Some(PathBuf::from("/home/pw"));
    let none = || -> Option<PathBuf> { None };
    assert_eq!(
        config_home_from(
            Some(OsString::from("/xdg")),
            Some(OsString::from("/h")),
            passwd
        ),
        PathBuf::from("/xdg")
    );
    assert_eq!(
        config_home_from(None, Some(OsString::from("/h")), passwd),
        PathBuf::from("/h/.config")
    );
    assert_eq!(
        config_home_from(None, None, passwd),
        PathBuf::from("/home/pw/.config")
    );
    // An empty HOME is as good as none.
    assert_eq!(
        config_home_from(None, Some(OsString::new()), passwd),
        PathBuf::from("/home/pw/.config")
    );
    assert_eq!(config_home_from(None, None, none), PathBuf::from(".config"));
}

/// A unit test never resolves the user's own homes, with or without `make test`: every home is
/// under the temporary directory.
#[test]
fn unit_tests_resolve_scratch_homes() {
    let scratch = std::env::temp_dir();
    let homes = [
        crate::config::config_home(),
        crate::config::state_home(),
        std::path::PathBuf::from(std::env::var_os("XDG_DATA_HOME").expect("a data home")),
        crate::config::cache_home(),
        std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").expect("a runtime dir")),
    ];
    for home in homes {
        assert!(
            home.starts_with(&scratch),
            "{home:?} is not under {scratch:?}"
        );
    }
    assert!(std::env::var_os("SVOLDER_MCP_TOKEN").is_none());
}

/// The sweep removes the scratch homes of runs that are over and keeps a live one.
#[test]
fn the_scratch_sweep_removes_only_finished_runs() {
    let root = std::env::temp_dir().join(format!("svolder-sweep-test-{}", std::process::id()));
    let dead = root.join("svolder-unit-4294967295");
    let live = root.join(format!("svolder-unit-{}", std::process::id()));
    std::fs::create_dir_all(dead.join("state")).unwrap();
    std::fs::create_dir_all(&live).unwrap();
    sweep_finished_scratch_homes(&root);
    assert!(!dead.exists());
    assert!(live.exists());
    std::fs::remove_dir_all(&root).unwrap();
}

/// The sweep keeps a scratch root whose run holds its lock, whatever its pid says (a run in
/// another PID namespace has a pid this one cannot see in `/proc`), and removes it once the lock
/// is let go.
#[test]
fn the_scratch_sweep_keeps_a_root_whose_run_holds_its_lock() {
    let temp = temp_dir("config-writer");
    // Pids no process here has, as a run in another PID namespace looks from here.
    let (live, lock) = scratch_root(&temp, 4_294_967_294).expect("a root");
    let (over, ended) = scratch_root(&temp, 4_294_967_293).expect("a root");
    drop(ended);
    sweep_finished_scratch_homes(&temp);
    assert!(live.exists(), "a live run's root was removed");
    assert!(!over.exists(), "a finished run's root was kept");
    drop(lock);
    sweep_finished_scratch_homes(&temp);
    assert!(!live.exists(), "a root outlived its run");
}

/// A root another live run holds (the same pid in another PID namespace) is not shared: the
/// next name is taken.
#[test]
fn a_scratch_root_another_run_holds_is_not_shared() {
    let temp = temp_dir("config-writer");
    let (first, _held) = scratch_root(&temp, 7).expect("a root");
    let (second, _also) = scratch_root(&temp, 7).expect("another root");
    assert_eq!(first, temp.join("svolder-unit-7"));
    assert_eq!(second, temp.join("svolder-unit-7-1"));
}

/// When every name for a pid is held, there is no root: a held one is never shared.
#[test]
fn a_scratch_root_is_none_when_every_name_is_held() {
    let temp = temp_dir("config-writer");
    let held: Vec<_> = (0..9)
        .map(|_| scratch_root(&temp, 7).expect("a root"))
        .collect();
    assert_eq!(held.len(), 9);
    assert!(scratch_root(&temp, 7).is_none(), "a held root was shared");
}

/// Roots whose lock file cannot be opened (a directory stands in its place) are skipped, and the
/// result is `None`, not a panic.
#[test]
fn a_scratch_root_whose_lock_cannot_be_opened_is_none() {
    use std::os::unix::fs::DirBuilderExt;
    let temp = temp_dir("config-writer");
    for name in std::iter::once("svolder-unit-7".to_owned())
        .chain((1..=8).map(|n| format!("svolder-unit-7-{n}")))
    {
        let root = temp.join(name);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        std::fs::create_dir(root.join("live.lock")).unwrap();
    }
    assert!(scratch_root(&temp, 7).is_none());
}

/// Under a directory it cannot write, no root is made and nothing panics.
#[test]
fn a_scratch_root_under_a_directory_it_cannot_write_is_none() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let temp = temp_dir("config-writer");
    std::fs::set_permissions(&*temp, std::fs::Permissions::from_mode(0o500)).unwrap();
    let root = scratch_root(&temp, 7);
    std::fs::set_permissions(&*temp, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(root.is_none());
}

/// The sweep removes only real directories this process's user owns.
#[test]
fn the_scratch_sweep_leaves_another_users_directory_alone() {
    assert!(is_ours_to_sweep(true, 1000, 1000));
    assert!(!is_ours_to_sweep(true, 0, 1000), "another owner");
    assert!(!is_ours_to_sweep(false, 1000, 1000), "not a real directory");
}

/// A scratch root another user planted as a symlink is refused: a different, real, private
/// directory is chosen.
#[test]
fn a_planted_scratch_root_is_not_taken() {
    use std::os::unix::fs::MetadataExt;
    let temp = std::env::temp_dir().join(format!("svolder-root-test-{}", std::process::id()));
    let elsewhere = temp.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, temp.join("svolder-unit-7")).unwrap();
    let (root, _lock) = scratch_root(&temp, 7).expect("a root");
    assert_ne!(root, temp.join("svolder-unit-7"));
    let meta = std::fs::symlink_metadata(&root).unwrap();
    assert!(meta.is_dir());
    assert_eq!(meta.mode() & 0o777, 0o700);
    std::fs::remove_dir_all(&temp).unwrap();
}

/// A home counts as under the temporary directory only when it is a strict descendant by its
/// components, with no `..`.
#[test]
fn only_a_strict_descendant_of_temp_is_kept() {
    let temp = std::path::Path::new("/tmp");
    assert!(is_strictly_under(std::path::Path::new("/tmp/x"), temp));
    assert!(!is_strictly_under(std::path::Path::new("/tmp"), temp));
    assert!(!is_strictly_under(std::path::Path::new("/tmp/../x"), temp));
    assert!(!is_strictly_under(std::path::Path::new("tmp/x"), temp));
    assert!(!is_strictly_under(std::path::Path::new("/tmpx/y"), temp));
}
