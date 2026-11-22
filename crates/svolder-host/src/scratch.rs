// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one directory of the temporary directory that svolder's own code makes scratch
//! directories in: `<temp>/svolder`, the Rust side of `scripts/tmp-base.sh`.

use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// `<temp dir>/svolder`, made when absent; see [`scratch_base_in`].
pub fn scratch_base() -> io::Result<PathBuf> {
    scratch_base_in(&std::env::temp_dir())
}

/// `<temp>/svolder`, made with mode 0700 when absent and refused unless it is a real directory
/// (not a link) owned by this user. A mode other than 0700 on a directory this user owns is set
/// to 0700. The temporary directory is shared, so another user could plant a link or a directory
/// of their own at that name.
pub fn scratch_base_in(temp: &Path) -> io::Result<PathBuf> {
    claim(temp, std::fs::metadata("/proc/self")?.uid())
}

/// [`scratch_base_in`] for a caller whose uid is `uid`.
fn claim(temp: &Path, uid: u32) -> io::Result<PathBuf> {
    let base = temp.join("svolder");
    match std::fs::DirBuilder::new().mode(0o700).create(&base) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err),
    }
    let refuse = |what: &str| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} {what}; refusing to use it for temporary files",
                base.display()
            ),
        ))
    };
    let meta = std::fs::symlink_metadata(&base)?;
    if meta.file_type().is_symlink() {
        return refuse("is a symbolic link");
    }
    if !meta.is_dir() {
        return refuse("is not a directory");
    }
    if meta.uid() != uid {
        return refuse("is not owned by you");
    }
    if meta.mode() & 0o777 != 0o700 {
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of its own inside a test-owned root in the scratch base, so a failed assert
    /// leaves it there and not loose in the temporary directory.
    fn temp(tag: &str) -> PathBuf {
        let dir = scratch_base()
            .unwrap()
            .join(format!("scratch-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_base_is_made_private() {
        let t = temp("make");
        let base = scratch_base_in(&t).unwrap();
        assert_eq!(base, t.join("svolder"));
        assert_eq!(std::fs::metadata(&base).unwrap().mode() & 0o777, 0o700);
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn a_loose_mode_is_set_to_private() {
        let t = temp("mode");
        std::fs::create_dir(t.join("svolder")).unwrap();
        std::fs::set_permissions(t.join("svolder"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let base = scratch_base_in(&t).unwrap();
        assert_eq!(std::fs::metadata(&base).unwrap().mode() & 0o777, 0o700);
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn a_planted_link_is_refused() {
        let t = temp("link");
        std::fs::create_dir(t.join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(t.join("elsewhere"), t.join("svolder")).unwrap();
        let err = scratch_base_in(&t).unwrap_err();
        assert!(err.to_string().contains("symbolic link"), "{err}");
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn a_planted_file_is_refused() {
        let t = temp("file");
        std::fs::write(t.join("svolder"), b"").unwrap();
        let err = scratch_base_in(&t).unwrap_err();
        assert!(err.to_string().contains("not a directory"), "{err}");
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn a_directory_of_another_owner_is_refused() {
        let t = temp("owner");
        std::fs::create_dir(t.join("svolder")).unwrap();
        let me = std::fs::metadata("/proc/self").unwrap().uid();
        let err = claim(&t, me + 1).unwrap_err();
        assert!(err.to_string().contains("not owned"), "{err}");
        std::fs::remove_dir_all(&t).unwrap();
    }
}
