// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Getting a listener: binding the socket path, adopting a socket a service manager handed over,
//! and serving on a bound listener until it is told to stop.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::net::UnixListener;

use super::{Handler, accept_loop};
use crate::error::IpcError;
use crate::socket::client;

/// A listener this process bound on a path of its own, and the inode of the socket file it
/// bound.
///
/// A daemon that has to know its socket file is still its own — one whose path another process
/// may unlink and bind a listener of its own on — learns the inode here rather than by looking
/// at the path afterwards, where what it finds may already be somebody else's.
pub struct Bound {
    listener: UnixListener,
    path: PathBuf,
    inode: Option<u64>,
}

impl Bound {
    /// The inode of the socket file at the moment it was bound, or `None` when the filesystem
    /// could not say.
    pub fn inode(&self) -> Option<u64> {
        self.inode
    }
}

/// Create the socket's directory if need be, then bind `path`: refuse if a live daemon already
/// answers there, clear a stale socket file, and leave the file at mode 0600.
pub async fn bind(path: &Path) -> Result<Bound, IpcError> {
    crate::socket::path::ensure_parent(path)?;
    let listener = bind_listener(path).await?;
    let inode = inode(path).ok().flatten();
    Ok(Bound {
        listener,
        path: path.to_path_buf(),
        inode,
    })
}

/// Serve `handler` on a listener [`bind`] returned until `shutdown` resolves, then remove the
/// socket file.
pub async fn serve_bound<H: Handler>(
    bound: Bound,
    handler: Arc<H>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), IpcError> {
    let Bound {
        listener,
        path,
        inode: mine,
    } = bound;
    let outcome = accept_loop(&listener, handler, shutdown).await;
    // Only the file this listener bound is this listener's to remove. A path another process
    // has since bound — a socket unit that unlinked it and put its own there — belongs to that
    // process, and taking it away would leave that daemon listening where nobody can reach it.
    if mine.is_some() && inode(&path).ok().flatten() == mine {
        let _ = std::fs::remove_file(&path);
    }
    outcome
}

/// The inode a path names, for a caller that has to know a socket file is still the one it
/// bound.
///
/// `Ok(None)` is a path with nothing there. An `Err` is a path that could not be asked about
/// at all — a filesystem that answered something other than "no such file" — which says
/// nothing about whose the socket is: a caller that acts on the answer wants that told apart
/// from an inode that changed.
pub fn inode(path: &Path) -> std::io::Result<Option<u64>> {
    use std::os::unix::fs::MetadataExt;
    match std::fs::metadata(path) {
        Ok(meta) => Ok(Some(meta.ino())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// The first descriptor a service manager passes a listening socket on.
const LISTEN_FDS_START: std::os::fd::RawFd = 3;

/// The listening socket a service manager handed this process, or `None` when it handed none.
///
/// This is the systemd socket-activation protocol as far as svolder uses it: `LISTEN_FDS`
/// counts the descriptors passed, starting at 3, and `LISTEN_PID` names the process they were
/// meant for. Exactly one socket is expected, since `svolderd` listens on one; a `LISTEN_PID`
/// naming another process is a handover inherited by a child rather than one addressed here,
/// and is refused so the child does not serve its parent's socket. An absent `LISTEN_PID` is
/// accepted: a handover simulated by hand does not always set it.
///
/// Call this once, before anything else in the process can have opened a descriptor on top of
/// the one passed. Every later call answers `None`, because adopting the same descriptor twice
/// would hand out two owners of one socket.
pub fn listen_fds() -> Option<std::os::unix::net::UnixListener> {
    use std::os::fd::FromRawFd;
    use std::sync::atomic::{AtomicBool, Ordering};
    static ADOPTED: AtomicBool = AtomicBool::new(false);

    if !handed_over(
        std::env::var("LISTEN_FDS").ok().as_deref(),
        std::env::var("LISTEN_PID").ok().as_deref(),
        std::process::id(),
    ) {
        return None;
    }
    if ADOPTED.swap(true, Ordering::SeqCst) {
        return None;
    }
    // SAFETY: the environment says descriptor 3 is a listening socket this process was given
    // and has not taken over yet — `ADOPTED` is what makes that true of this call alone — so
    // nothing else owns it and the listener returned is its only owner.
    Some(unsafe { std::os::unix::net::UnixListener::from_raw_fd(LISTEN_FDS_START) })
}

/// Whether the environment describes exactly one socket handed to process `pid`.
fn handed_over(listen_fds: Option<&str>, listen_pid: Option<&str>, pid: u32) -> bool {
    if listen_fds.and_then(|count| count.trim().parse::<u32>().ok()) != Some(1) {
        return false;
    }
    match listen_pid {
        None => true,
        Some(named) => named.trim().parse::<u32>() == Ok(pid),
    }
}

/// Bind the listener, refusing if a live daemon already answers and clearing a stale socket.
async fn bind_listener(path: &Path) -> Result<UnixListener, IpcError> {
    let listener = match UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if client::is_running(path).await {
                return Err(IpcError::Protocol(
                    "another svolderd is already running".into(),
                ));
            }
            let _ = std::fs::remove_file(path);
            UnixListener::bind(path)?
        }
        Err(e) => return Err(e.into()),
    };
    // A socket this call binds itself takes the umask's own mode; systemd socket activation
    // hands the daemon an already-bound, already-0600 listener instead, so this only matters
    // to a daemon started directly (a container's own process, `svolderd` run by hand). Force
    // it here so a directory shared with other users on the machine (a bind-mounted `./run`,
    // unlike `$XDG_RUNTIME_DIR`'s own 0700) still keeps other users off the socket.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

#[cfg(test)]
mod tests;
