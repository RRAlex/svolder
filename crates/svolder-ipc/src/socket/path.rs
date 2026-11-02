// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Unix socket path resolution and management.

use std::path::{Path, PathBuf};

/// Resolve the socket path: `--socket`, then `SVOLDER_SOCKET`, then the configuration's
/// `socket`, then `svolder.sock` in `$XDG_RUNTIME_DIR`, in `/run/user/<uid>` when it exists,
/// else in the private `/tmp/svolder-<uid>`.
pub fn resolve(flag: Option<&str>, config: Option<&str>) -> PathBuf {
    let env = std::env::var("SVOLDER_SOCKET").ok();
    let xdg = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    resolve_from(flag, env.as_deref(), config, xdg.as_deref(), uid(), |p| {
        p.is_dir()
    })
}

/// The pure core of `resolve`, with every environment input passed in so it is testable.
///
/// A blank override is no override: an empty `--socket`, `SVOLDER_SOCKET` or `socket` field
/// falls through to the next source rather than naming the empty path, which `bind` would
/// turn into an anonymous abstract socket no client can reach.
fn resolve_from(
    flag: Option<&str>,
    env: Option<&str>,
    config: Option<&str>,
    xdg_runtime_dir: Option<&Path>,
    uid: u32,
    dir_exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(f) = given(flag) {
        return PathBuf::from(f);
    }
    if let Some(e) = given(env) {
        return PathBuf::from(e);
    }
    if let Some(c) = given(config) {
        return PathBuf::from(c);
    }
    if let Some(dir) = xdg_runtime_dir {
        return dir.join("svolder.sock");
    }
    let run_user = PathBuf::from(format!("/run/user/{uid}"));
    if dir_exists(&run_user) {
        return run_user.join("svolder.sock");
    }
    tmp_fallback_dir(uid).join("svolder.sock")
}

/// The directory the socket would live in with no override at all, created when it is this
/// uid's private `/tmp/svolder-<uid>` fallback: a place for a short-lived private file.
///
/// A directory that somebody else owns, that others may write to, or that is a symlink is
/// refused, so nothing is placed in a directory another user prepared.
pub fn private_dir() -> std::io::Result<PathBuf> {
    let xdg = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let socket = resolve_from(None, None, None, xdg.as_deref(), uid(), |p| p.is_dir());
    ensure_parent(&socket)?;
    let dir = socket
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let meta = std::fs::symlink_metadata(&dir).map_err(|e| named(&dir, e))?;
    if !is_private_to(&meta, uid()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{}: not a private directory of this user", dir.display()),
        ));
    }
    Ok(dir)
}

/// Whether `meta` describes a directory owned by `uid` that neither group nor others can write.
fn is_private_to(meta: &std::fs::Metadata, uid: u32) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    meta.is_dir() && meta.uid() == uid && meta.permissions().mode() & 0o022 == 0
}

/// One source's value when it names something, and `None` when it is absent or blank.
fn given(source: Option<&str>) -> Option<&str> {
    source.filter(|s| !s.trim().is_empty())
}

/// The private fallback directory for this uid, `/tmp/svolder-<uid>`.
fn tmp_fallback_dir(uid: u32) -> PathBuf {
    PathBuf::from(format!("/tmp/svolder-{uid}"))
}

/// Create the socket's parent directory with mode 0700 when, and only when, it is this uid's
/// private `/tmp/svolder-<uid>` fallback. Any other parent is left alone: the runtime
/// directories already exist, and a custom path's directory is the user's to provide.
pub fn ensure_parent(path: &Path) -> std::io::Result<()> {
    ensure_parent_of(path, &tmp_fallback_dir(uid()))
}

/// The pure core of `ensure_parent`, with the fallback directory passed in so tests can point
/// it at a scratch directory.
fn ensure_parent_of(path: &Path, fallback_dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if path.parent() == Some(fallback_dir) && !fallback_dir.exists() {
        std::fs::create_dir_all(fallback_dir).map_err(|e| named(fallback_dir, e))?;
        std::fs::set_permissions(fallback_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| named(fallback_dir, e))?;
    }
    Ok(())
}

/// `e`, with `path` said in its message: neither `create_dir_all` nor `set_permissions` names
/// the path in the error they return on their own.
fn named(path: &Path, e: std::io::Error) -> std::io::Error {
    std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}

/// The process's real user id.
fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail; it only reads the process's real uid.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_private_directory_is_owned_by_the_uid_and_closed_to_others() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("svolder-private-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = crate::test_support::TempPath(dir);
        let set =
            |mode| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
        set(0o700);
        let meta = std::fs::metadata(&dir).unwrap();
        assert!(is_private_to(&meta, uid()));
        assert!(!is_private_to(&meta, uid() + 1), "another owner");
        set(0o770);
        assert!(
            !is_private_to(&std::fs::metadata(&dir).unwrap(), uid()),
            "group writable"
        );
        set(0o702);
        assert!(
            !is_private_to(&std::fs::metadata(&dir).unwrap(), uid()),
            "world writable"
        );
        set(0o700);
        let file = dir.join("f");
        std::fs::write(&file, b"").unwrap();
        assert!(
            !is_private_to(&std::fs::metadata(&file).unwrap(), uid()),
            "a file"
        );
    }

    #[test]
    fn flag_wins_over_everything() {
        assert_eq!(
            resolve(Some("/x/y.sock"), Some("/c.sock")),
            PathBuf::from("/x/y.sock")
        );
    }

    #[test]
    fn env_beats_config_and_config_beats_runtime_dir() {
        // Through the pure core, so the test never depends on the process environment.
        let rt = Some(Path::new("/run/user/1001"));
        assert_eq!(
            resolve_from(None, Some("/e.sock"), Some("/c.sock"), rt, 1001, |_| false),
            PathBuf::from("/e.sock")
        );
        assert_eq!(
            resolve_from(None, None, Some("/c.sock"), rt, 1001, |_| false),
            PathBuf::from("/c.sock")
        );
    }

    #[test]
    fn a_blank_flag_or_environment_socket_is_not_an_override() {
        let rt = Some(Path::new("/run/user/1001"));
        // An empty `--socket` falls through to the environment, and an empty
        // `SVOLDER_SOCKET` to the config, rather than naming the empty path.
        assert_eq!(
            resolve_from(Some(""), Some("/e.sock"), None, rt, 1001, |_| false),
            PathBuf::from("/e.sock")
        );
        assert_eq!(
            resolve_from(Some("  "), None, Some("/c.sock"), rt, 1001, |_| false),
            PathBuf::from("/c.sock")
        );
        assert_eq!(
            resolve_from(Some(""), Some(""), Some(""), rt, 1001, |_| false),
            PathBuf::from("/run/user/1001/svolder.sock")
        );
    }

    #[test]
    fn ensure_parent_creates_only_the_private_fallback_with_mode_0700() {
        use std::os::unix::fs::PermissionsExt;
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = crate::test_support::TempPath(
            std::env::temp_dir().join(format!("svolder-socket-test-{}-{n}", std::process::id())),
        );
        let fallback = root.join("svolder-1001");
        // A socket inside the fallback: the directory is created, private to the user.
        ensure_parent_of(&fallback.join("svolder.sock"), &fallback).unwrap();
        let mode = std::fs::metadata(&fallback).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        // A socket anywhere else: nothing is created.
        let elsewhere = root.join("custom");
        ensure_parent_of(&elsewhere.join("x.sock"), &fallback).unwrap();
        assert!(!elsewhere.exists());
    }

    #[test]
    fn ensure_parent_names_the_path_it_could_not_create() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = crate::test_support::TempPath(
            std::env::temp_dir().join(format!("svolder-socket-test-{}-{n}", std::process::id())),
        );
        // `root` is a plain file, so a fallback directory under it can never be created: every
        // `create_dir_all` on a path through it fails with the file in the way.
        std::fs::write(&root, b"not a directory").unwrap();
        let fallback = root.join("svolder-1001");
        let err = ensure_parent_of(&fallback.join("svolder.sock"), &fallback).unwrap_err();
        assert!(
            err.to_string().contains(&fallback.display().to_string()),
            "{err}"
        );
    }

    #[test]
    fn runtime_dir_default_is_svolder_sock() {
        // The `$XDG_RUNTIME_DIR/svolder.sock` fallback, exercised through the pure helper.
        let got = resolve_from(
            None,
            None,
            None,
            Some(Path::new("/run/user/1001")),
            1001,
            |_| false,
        );
        assert_eq!(got, PathBuf::from("/run/user/1001/svolder.sock"));
    }

    #[test]
    fn tmp_fallback_uses_uid_when_no_runtime_dir() {
        let got = resolve_from(None, None, None, None, 1001, |p| {
            p == Path::new("/run/user/1001")
        });
        // /run/user/1001 exists here, so it is preferred over /tmp.
        assert_eq!(got, PathBuf::from("/run/user/1001/svolder.sock"));
        let got2 = resolve_from(None, None, None, None, 1001, |_| false);
        assert_eq!(got2, PathBuf::from("/tmp/svolder-1001/svolder.sock"));
    }
}
