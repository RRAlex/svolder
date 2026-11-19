// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where a test binds a Unix socket. The path of a socket must be shorter than `SUN_LEN` (108
//! bytes, the last for the terminating NUL), and a `TMPDIR` under a worktree
//! (`~/src/<worktree>/target/...`) is already most of that. So the base is `TMPDIR` itself when
//! it is short, which keeps `make test`'s check of what a case left in `TMPDIR` covering sockets
//! and daemon directories, and `/tmp/svolder` (or `SVOLDER_TMP_BASE`) when it is not. Every other
//! temporary file stays under `TMPDIR`. Every test crate that binds a socket includes this file.

#![allow(dead_code)]

use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// The longest `TMPDIR` that is used as the base. A socket address holds 107 bytes. What a case
/// makes under the base is at most `MAX_AFTER_BASE` = 73 bytes (separator, a directory name of at
/// most `MAX_NAME` = 59, `/svolder.sock`), which `short_join` enforces, so a base of 26 leaves 8
/// to spare (73 + 26 = 99). `make test`'s 24-byte scratch is under it.
const SHORT_TMPDIR: usize = 26;

/// The longest directory name a case makes under the base: the longest the cases make is the
/// backend cases' `<svbe 4>-<pid 7>-<seq 3>-<time 16>` plus a suffix of up to 20, 53 bytes.
const MAX_NAME: usize = 59;

/// The longest label a name carries; the pid, counter and time make the name unique.
const LABEL: usize = 20;

/// What a socket path may be at most (`sockaddr_un.sun_path` is 108 bytes with the NUL).
const SUN_PATH: usize = 107;

/// The shared base the scripts use too (`scripts/tmp-base.sh`).
const SHARED_BASE: &str = "/tmp/svolder";

/// The shared base: made 0700 when absent, and refused unless it is a real directory (not a
/// link) owned by this user, because another local user could pre-create it or plant a link in
/// the sticky `/tmp`. A mode other than 0700 on a directory of ours is set to 0700.
fn shared_base() -> PathBuf {
    let base =
        PathBuf::from(std::env::var_os("SVOLDER_TMP_BASE").unwrap_or_else(|| SHARED_BASE.into()));
    let _ = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&base);
    let shown = base.display();
    let meta = std::fs::symlink_metadata(&base).unwrap_or_else(|e| {
        panic!("svolder: {shown}: {e}; refusing to use it for temporary files")
    });
    assert!(
        !meta.file_type().is_symlink(),
        "svolder: {shown} is a symbolic link; refusing to use it for temporary files"
    );
    assert!(
        meta.is_dir(),
        "svolder: {shown} is not a directory; refusing to use it for temporary files"
    );
    let ours = std::fs::metadata("/proc/self")
        .expect("/proc/self is readable")
        .uid();
    assert_eq!(
        meta.uid(),
        ours,
        "svolder: {shown} is not owned by you; refusing to use it for temporary files"
    );
    if meta.permissions().mode() & 0o777 != 0o700 {
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|e| panic!("svolder: {shown} cannot be set to 0700: {e}"));
    }
    base
}

/// A name for a case's own directory or socket, unique to the case: the tag (cut to 20 bytes), the
/// pid, a counter and the time.
pub fn short_unique(tag: &str) -> String {
    unique(tag)
}

fn unique(tag: &str) -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let label: String = tag.chars().take(LABEL).collect();
    format!("{label}-{}-{seq}-{nanos:x}", std::process::id())
}

/// The directory a case makes its own short-named directories in: a case that runs a daemon in
/// a directory of its own joins its name to this instead of to `TMPDIR`.
pub fn short_base() -> PathBuf {
    let temp = std::env::temp_dir();
    if temp.as_os_str().len() <= SHORT_TMPDIR {
        temp
    } else {
        shared_base()
    }
}

/// `<base>/<name>` for a directory in which a socket `svolder.sock` is bound (a daemon's runtime
/// directory, or a stub's). Fails here, with the path, when `name` could make that socket path
/// pass 107 bytes under a `TMPDIR` of the longest length that is used as the base (or under a
/// longer actual base), so no case depends on the length of the `TMPDIR` it ran under.
pub fn short_join(name: &str) -> PathBuf {
    let base = short_base();
    let path = base.join(name);
    let base_len = base.as_os_str().len().max(SHORT_TMPDIR);
    let worst = base_len + 1 + name.len() + "/svolder.sock".len();
    assert!(
        name.len() <= MAX_NAME && worst <= SUN_PATH,
        "the socket directory {} would hold a socket path of {worst} bytes, over the {SUN_PATH} a \
         socket address holds",
        path.display()
    );
    path
}

/// A path for a socket no other case has, under a directory that exists. The case removes the
/// file it binds there.
pub fn short_socket(tag: &str) -> PathBuf {
    let path = short_base().join(format!("{}.sock", unique(tag)));
    assert_fits(&path);
    path
}

/// A directory of the case's own for sockets (and the files that go with them), made now. The
/// case removes it.
pub fn short_dir(tag: &str) -> PathBuf {
    let dir = short_base().join(unique(tag));
    // A socket in the directory is named `svolder.sock` or `bus` by the cases.
    assert_fits(&dir.join("svolder.sock"));
    std::fs::create_dir(&dir).expect("the short socket directory is made");
    dir
}

/// Fails here, loudly, when a socket at `path` could not be bound.
fn assert_fits(path: &std::path::Path) {
    assert!(
        path.as_os_str().len() <= SUN_PATH,
        "the socket path {} is {} bytes, over the {SUN_PATH} a socket address holds",
        path.display(),
        path.as_os_str().len()
    );
}
