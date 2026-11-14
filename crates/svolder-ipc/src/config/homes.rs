// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where svolder keeps its files: the config, state and cache homes, the host EQ paths and the
//! scratch homes unit tests run in.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME`, else `$HOME/.config`, else the account's home directory (from the
/// password database) plus `.config`; the relative `.config` only when the account has no home.
pub fn config_home() -> PathBuf {
    config_home_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
        passwd_home,
    )
}

/// Points every home svolder resolves at a scratch directory, so a unit test never reads or
/// writes the user's own config, state, data, cache or runtime directory, however the tests
/// were started.
///
/// The scratch root is a private directory (mode 0700, owned by this user) named
/// `svolder-unit-<pid>` under the temporary directory, holding `config`, `state`, `data`,
/// `cache` and `run`. A value of `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_DATA_HOME`,
/// `XDG_CACHE_HOME` or `XDG_RUNTIME_DIR` that is already a strict descendant of the temporary
/// directory is kept; any other value, or none, becomes the root's directory of that name.
/// `make test` points `TMPDIR` at a directory of its own and the homes beside it, so each test
/// binary pins its own homes under that `TMPDIR`. When no safe root can be made, the homes are
/// left as they are. `SVOLDER_MCP_TOKEN`, `SVOLDER_WEB_TOKEN` and the web and MCP switch, listen
/// and URL variables are removed, and
/// `SVOLDER_WIREPLUMBER` and `SVOLDER_AUTOEQ_URL` get the values `make test` gives them when
/// unset. The scratch roots of runs whose lock nobody holds are removed first.
///
/// It runs before `main`, where a panic would abort the test binary without a word, so every
/// call in it either cannot panic (the environment setters get valid names and values) or has
/// its error dropped.
///
/// # Safety
///
/// Call it only while the process has one thread: it sets environment variables.
#[doc(hidden)]
pub unsafe fn pin_scratch_homes() {
    let temp = std::env::temp_dir();
    sweep_finished_scratch_homes(&temp);
    if let Some((root, lock)) = scratch_root(&temp, std::process::id()) {
        // The lock is held until the process ends: its descriptor is never closed.
        std::mem::forget(lock);
        for (name, leaf) in [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_RUNTIME_DIR", "run"),
        ] {
            let dir = match std::env::var_os(name) {
                Some(kept) if is_strictly_under(Path::new(&kept), &temp) => PathBuf::from(kept),
                _ => root.join(leaf),
            };
            let _ = create_private_dir(&dir);
            // SAFETY: the caller guarantees one thread.
            unsafe { std::env::set_var(name, &dir) };
        }
        if std::env::var_os("SVOLDER_WIREPLUMBER").is_none() {
            // SAFETY: the caller guarantees one thread.
            unsafe { std::env::set_var("SVOLDER_WIREPLUMBER", root.join("no-wireplumber")) };
        }
    }
    // SAFETY: the caller guarantees one thread.
    unsafe { std::env::remove_var("SVOLDER_MCP_TOKEN") };
    // SAFETY: the caller guarantees one thread.
    unsafe { std::env::remove_var("SVOLDER_WEB_TOKEN") };
    // The container switches and listen defaults would turn a test daemon's servers on or move
    // them onto a developer's own ports.
    for name in [
        "SVOLDER_WEB_ENABLED",
        "SVOLDER_MCP_ENABLED",
        "SVOLDER_WEB_LISTEN",
        "SVOLDER_MCP_LISTEN",
        "SVOLDER_WEB_URL",
        "SVOLDER_MQTT_ENABLED",
        "SVOLDER_MQTT_HOST",
        "SVOLDER_MQTT_PORT",
        "SVOLDER_MQTT_USERNAME",
        "SVOLDER_MQTT_PASSWORD",
    ] {
        // SAFETY: the caller guarantees one thread.
        unsafe { std::env::remove_var(name) };
    }
    if std::env::var_os("SVOLDER_AUTOEQ_URL").is_none() {
        // SAFETY: the caller guarantees one thread.
        unsafe { std::env::set_var("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/") };
    }
}

/// Whether `dir` is absolute, has no `..` component and lies strictly below `temp`.
pub(crate) fn is_strictly_under(dir: &Path, temp: &Path) -> bool {
    dir.is_absolute()
        && dir != temp
        && dir.starts_with(temp)
        && !dir
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}

/// Creates `dir` (and its parents) with mode 0700.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// The file in a scratch root that its run holds a lock on for as long as it lives.
const LIVE_LOCK: &str = "live.lock";

/// Takes `file`'s lock unless another holds it, without waiting: whether this process holds it
/// now.
fn locked_now(file: &std::fs::File) -> bool {
    use std::os::fd::AsRawFd;
    // SAFETY: `flock` only reads the descriptor, which `file` keeps open for the call.
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
}

/// Whether `path` is still the file `open` was opened from: a sweep may have unlinked a lock
/// file between its opening and its locking.
fn is_file_at(open: &std::fs::File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (open.metadata(), std::fs::symlink_metadata(path)) {
        (Ok(held), Ok(there)) => held.dev() == there.dev() && held.ino() == there.ino(),
        _ => false,
    }
}

/// A private scratch root for process `pid` under `temp` and the lock on it this run holds:
/// `svolder-unit-<pid>`, else `svolder-unit-<pid>-<n>`. A new one is created with mode 0700; an
/// existing one is taken only when it is a real directory (not a symlink) owned by this user with
/// mode 0700 whose lock nobody holds, so a run with the same pid in another PID namespace keeps
/// its own. `None` when no candidate is safe.
pub(crate) fn scratch_root(temp: &Path, pid: u32) -> Option<(PathBuf, std::fs::File)> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};

    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    let candidates = std::iter::once(format!("svolder-unit-{pid}"))
        .chain((1..=8).map(|n| format!("svolder-unit-{pid}-{n}")));
    for name in candidates {
        let root = temp.join(name);
        let made = std::fs::DirBuilder::new().mode(0o700).create(&root).is_ok();
        let safe = made
            || std::fs::symlink_metadata(&root).is_ok_and(|meta| {
                meta.is_dir() && meta.uid() == uid && meta.mode() & 0o777 == 0o700
            });
        if !safe {
            continue;
        }
        // Never follow a symlink, never wait on a FIFO: this runs before `main`.
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join(LIVE_LOCK));
        if let Ok(lock) = lock
            && locked_now(&lock)
            && is_file_at(&lock, &root.join(LIVE_LOCK))
        {
            return Some((root, lock));
        }
    }
    None
}

/// Removes the `svolder-unit-<pid>` and `svolder-unit-<pid>-<n>` directories of `root` whose run
/// no longer holds its lock. A symlink is never followed, and a directory another user owns is
/// left alone.
pub(crate) fn sweep_finished_scratch_homes(root: &Path) {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let Some(uid) = std::fs::metadata("/proc/self").ok().map(|m| m.uid()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix("svolder-unit-"))
            .and_then(|rest| rest.split('-').next())
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        let ours = std::fs::symlink_metadata(entry.path())
            .is_ok_and(|m| is_ours_to_sweep(m.is_dir(), m.uid(), uid));
        if !ours {
            continue;
        }
        // A live run holds its root's lock for as long as it runs, in whatever PID namespace; the
        // lock taken here is held while the root goes, and a run that locks a file this removal
        // unlinks finds it gone and takes another name. A root with no lock file (made by a run
        // that takes no lock, or in the instant between its creation and its lock) is judged by
        // its pid in `/proc`.
        let held = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(entry.path().join(LIVE_LOCK))
        {
            Ok(file) if locked_now(&file) => Some(file),
            Ok(_) => continue,
            Err(_) if Path::new("/proc").join(pid.to_string()).exists() => continue,
            Err(_) => None,
        };
        let _ = std::fs::remove_dir_all(entry.path());
        drop(held);
    }
}

/// Whether a scratch entry is this process's to remove: a real directory (never a symlink,
/// which `symlink_metadata` reports as not a directory) owned by `uid`.
fn is_ours_to_sweep(is_real_dir: bool, owner: u32, uid: u32) -> bool {
    is_real_dir && owner == uid
}

/// `$XDG_STATE_HOME`, or `~/.local/state`: where svolder keeps what this machine learned and
/// the secrets it generates, in a `svolder` directory of its own.
pub fn state_home() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().unwrap_or_default().join(".local").join("state"))
}

/// Where this machine's computer EQ keeps its saved state: `host-eq.toml` in svolder's state
/// directory.
pub fn host_eq_state_path() -> PathBuf {
    state_home().join("svolder").join("host-eq.toml")
}

/// The environment variable that names where systemd unit files go.
pub const UNITDIR_ENV: &str = "SVOLDER_UNITDIR";

/// Where this machine's computer EQ keeps its files: the filter chain's config and the state file
/// beside the configuration, the user's unit directory (`$SVOLDER_UNITDIR`, else where systemd
/// reads a user's own units from), the autostart directory and the process manager's pid file in
/// the private runtime directory.
pub fn host_eq_paths() -> svolder_host::eq::EqPaths {
    let unit_dir = std::env::var_os(UNITDIR_ENV)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| config_home().join("systemd").join("user"));
    svolder_host::eq::EqPaths {
        conf: config_home().join("svolder").join("host-eq.conf"),
        state: host_eq_state_path(),
        unit_dir,
        autostart_dir: config_home().join("autostart"),
        pid_file: crate::socket::path::private_dir()
            .ok()
            .map(|dir| dir.join("svolder-host-eq.pid")),
    }
}

/// `$XDG_CACHE_HOME`, or `~/.cache`: where svolder keeps what it can fetch again, in a
/// `svolder` directory of its own.
pub fn cache_home() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().unwrap_or_default().join(".cache"))
}

/// The user's home directory: `$HOME`, else the password database's entry for this uid.
pub fn home_dir() -> Option<PathBuf> {
    home_from(std::env::var_os("HOME"), passwd_home)
}

/// [`config_home`] with the environment and the password lookup as inputs.
fn config_home_from(
    xdg: Option<OsString>,
    home: Option<OsString>,
    passwd: impl FnOnce() -> Option<PathBuf>,
) -> PathBuf {
    match xdg.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home_from(home, passwd).unwrap_or_default().join(".config"),
    }
}

/// A non-empty `home`, else what `passwd` finds.
fn home_from(home: Option<OsString>, passwd: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    home.filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(passwd)
}

/// The home directory the password database lists for the current uid, when it lists one.
fn passwd_home() -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    let mut buf = vec![0u8; 1024];
    loop {
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: `entry` and `buf` outlive the call, `buf.len()` is its real size, and
        // `found` is read only after the call has set it.
        let rc = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                entry.as_mut_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut found,
            )
        };
        if rc == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        // SAFETY: `found` is non-null when it is read, so it points at the initialized `entry`.
        if rc != 0 || found.is_null() || unsafe { (*found).pw_dir.is_null() } {
            return None;
        }
        // SAFETY: `getpwuid_r` succeeded with a non-null result, so `entry` is initialized and
        // `pw_dir` points at a NUL-terminated string inside `buf`.
        let dir = unsafe { std::ffi::CStr::from_ptr((*found).pw_dir) };
        let bytes = dir.to_bytes();
        return (!bytes.is_empty()).then(|| PathBuf::from(OsString::from_vec(bytes.to_vec())));
    }
}

#[cfg(test)]
mod tests;
