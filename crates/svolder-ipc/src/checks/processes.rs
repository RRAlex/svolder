// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which svolderd processes this user is running.
//!
//! One daemon holds the devices of whoever reaches its socket, and a machine ends up with more
//! than one easily enough: a front end starts one on the socket it resolved, and a session with
//! a home, a runtime directory or a `SVOLDER_SOCKET` of its own resolves another path and
//! starts a second. `/proc` says which are running and where each listens, so
//! [`crate::checks::run`] and `svolder daemon status` can name a daemon nothing else in this
//! session would ever reach, whoever started it and however it was told where to listen.
//!
//! A daemon's listen path is the listening Unix socket it holds: its `fd` links name each open
//! socket by inode, and its own `net/unix` (the socket table of the network namespace it runs
//! in, another one than this process's for a daemon in a container) gives the path of every
//! listening one. What it was told — its `--socket` argument, else `SVOLDER_SOCKET` in its
//! environment — stands in only when its descriptors or socket table cannot be read.
//!
//! Everything here is read out of this user's own processes — `cmdline`, `status`, `fd`,
//! `net/unix` and `environ` — so nothing needs privileges; a process that ends while `/proc` is
//! walked, or whose files this user may not read, is left out rather than guessed at.

use std::path::{Path, PathBuf};

/// One svolderd this user is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningDaemon {
    /// Its process id.
    pub pid: u32,
    /// The socket it listens on: the path of the listening Unix socket it holds, whether it
    /// bound that socket itself or a service manager handed it over, as the socket table of
    /// the network namespace it runs in names it. For a process whose descriptors or socket
    /// table cannot be read, the socket it was told to listen on stands in — its
    /// `--socket` argument, else the `SVOLDER_SOCKET` in its environment. `None` when it holds
    /// no listening socket with a path, or, unreadable, was told neither. A daemon in a mount
    /// namespace of its own has the path named through its root, `/proc/<pid>/root/...`, which
    /// is where this side of that namespace reaches it.
    pub socket: Option<PathBuf>,
}

impl RunningDaemon {
    /// Where this daemon listens when it is not the one this session is about, and `None` when
    /// it is that one.
    ///
    /// `socket` is the path this session resolved and `listen` the path a socket unit binds,
    /// when there is one: a daemon on either is the daemon everything else here reaches. A
    /// daemon with no socket known — none listening yet, or one this user cannot look into
    /// and that was told nothing — has no path to name, so nothing is claimed about it.
    pub fn elsewhere_than<'a>(&'a self, socket: &Path, listen: Option<&Path>) -> Option<&'a Path> {
        let told = self.socket.as_deref()?;
        (told != socket && Some(told) != listen).then_some(told)
    }
}

/// The directory the processes are read from.
const PROC: &str = "/proc";

/// The program name a process must have been started under to be one of these daemons.
const SVOLDERD: &[u8] = b"svolderd";

/// The argument that names the socket, as `--socket PATH`.
const SOCKET_FLAG: &[u8] = b"--socket";

/// The same argument written as one word, `--socket=PATH`.
const SOCKET_FLAG_EQ: &[u8] = b"--socket=";

/// The environment variable that names the socket, with the `=` its entry carries.
const SOCKET_VAR: &[u8] = b"SVOLDER_SOCKET=";

/// Where a process directory keeps the table of every Unix socket in the network namespace
/// that process runs in.
const UNIX_TABLE: &str = "net/unix";

/// Where a process directory keeps the link naming the mount namespace that process runs in.
const MOUNT_NS: &str = "ns/mnt";

/// What an `fd` link to a socket reads as before its inode, and after it.
const SOCKET_LINK: (&[u8], &[u8]) = (b"socket:[", b"]");

/// The `Flags` column of a socket that accepts connections (`__SO_ACCEPTCON`).
const LISTENING_FLAGS: &[u8] = b"00010000";

/// The `St` column of a socket that is not connected to a peer, which a listening one is not.
const UNCONNECTED: &[u8] = b"01";

/// One listening Unix socket with a path: its inode, and the path it is bound to.
type Listening = (u64, PathBuf);

/// Every svolderd this user is running, this process aside, by pid.
///
/// This process is left out so a daemon asking what else runs is not told about itself.
pub fn running_daemons() -> Vec<RunningDaemon> {
    let (uid, me) = (uid(), std::process::id());
    let Ok(entries) = std::fs::read_dir(PROC) else {
        return Vec::new();
    };
    let ours = std::fs::read_link(Path::new(PROC).join("self").join(MOUNT_NS)).ok();
    let mut daemons: Vec<RunningDaemon> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            if pid == me {
                return None;
            }
            daemon_at(pid, &entry.path(), uid, ours.as_deref())
        })
        .collect();
    daemons.sort_by_key(|daemon| daemon.pid);
    daemons
}

/// The daemon the process directory `dir` describes, when what it holds is a svolderd of
/// `uid`'s: anything unreadable — a process that has ended, another user's — is not one.
///
/// Its socket is [`listening_path`]'s answer: the listening one it holds, out of the socket
/// table of its own network namespace, and what it was told only where either of those cannot
/// be read. It is named as [`reachable_path`] names it from `ours`, the link to this process's
/// own mount namespace.
fn daemon_at(pid: u32, dir: &Path, uid: u32, ours: Option<&Path>) -> Option<RunningDaemon> {
    let argv = nul_separated(&std::fs::read(dir.join("cmdline")).ok()?);
    if !is_svolderd(&argv) {
        return None;
    }
    if owner(&std::fs::read_to_string(dir.join("status")).ok()?)? != uid {
        return None;
    }
    let held = held_sockets(dir);
    let table = std::fs::read(dir.join(UNIX_TABLE))
        .ok()
        .map(|bytes| listening_sockets(&bytes));
    let socket = listening_path(held.as_deref(), table.as_deref(), || {
        socket_argument(&argv).or_else(|| {
            let environ = std::fs::read(dir.join("environ")).ok()?;
            socket_variable(&nul_separated(&environ))
        })
    });
    let theirs = std::fs::read_link(dir.join(MOUNT_NS)).ok();
    let socket = socket.map(|path| reachable_path(pid, path, ours, theirs.as_deref()));
    Some(RunningDaemon { pid, socket })
}

/// The path this session reaches a socket at that the daemon `pid` listens on at `path`.
///
/// A daemon in a mount namespace of its own — a container — names its socket by a path in that
/// namespace, which need not exist on this side of it; its root, `/proc/<pid>/root`, is where
/// this user reaches that namespace's files, so the socket is named through it. A daemon in
/// this process's own mount namespace, one whose namespace cannot be read, or a path that is
/// not absolute is named as it is.
fn reachable_path(pid: u32, path: PathBuf, ours: Option<&Path>, theirs: Option<&Path>) -> PathBuf {
    let elsewhere = matches!((ours, theirs), (Some(ours), Some(theirs)) if ours != theirs);
    match (elsewhere, path.strip_prefix("/")) {
        (true, Ok(inside)) => Path::new(PROC)
            .join(pid.to_string())
            .join("root")
            .join(inside),
        _ => path,
    }
}

/// Where a daemon listens, out of what could be read about it: `held`, the inodes of the
/// sockets it holds, and `table`, the listening sockets of the network namespace it runs in.
///
/// With both read, the answer is the listening socket it holds, or none when it holds none — a
/// daemon that has not bound its socket yet has said nothing about where it will. Only when
/// either could not be read does `told`, what it was told to listen on, stand in.
fn listening_path(
    held: Option<&[u64]>,
    table: Option<&[Listening]>,
    told: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    match (held, table) {
        (Some(held), Some(table)) => listens_on(held, table),
        _ => told(),
    }
}

/// The inodes of every socket the process directory `dir` holds open, or `None` when its
/// descriptors cannot be read at all. A descriptor closed while they are walked is left out.
fn held_sockets(dir: &Path) -> Option<Vec<u64>> {
    use std::os::unix::ffi::OsStrExt;
    let entries = std::fs::read_dir(dir.join("fd")).ok()?;
    Some(
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| std::fs::read_link(entry.path()).ok())
            .filter_map(|link| socket_inode(link.as_os_str().as_bytes()))
            .collect(),
    )
}

/// The inode an `fd` link to a socket names, as `socket:[INODE]`; any other link names none.
fn socket_inode(link: &[u8]) -> Option<u64> {
    let (before, after) = SOCKET_LINK;
    let inode = link.strip_prefix(before)?.strip_suffix(after)?;
    std::str::from_utf8(inode).ok()?.parse().ok()
}

/// The path of the first listening socket in `table` whose inode is among `held`.
fn listens_on(held: &[u64], table: &[Listening]) -> Option<PathBuf> {
    table
        .iter()
        .find(|(inode, _)| held.contains(inode))
        .map(|(_, path)| path.clone())
}

/// Every listening Unix socket with a path, out of the text of a `net/unix` socket table.
///
/// The first line is the column heads. A socket in the abstract namespace — its path written
/// with a leading `@` — has no file to name, so it is not among these.
fn listening_sockets(table: &[u8]) -> Vec<Listening> {
    table
        .split(|byte| *byte == b'\n')
        .skip(1)
        .filter_map(listening_entry)
        .collect()
}

/// One row of a `net/unix` socket table as a listening socket, when it is one.
///
/// A row reads `Num: RefCount Protocol Flags Type St Inode Path`: seven columns, the inode
/// padded to a width with spaces, then — for a socket bound to a path — one space and the path,
/// which is everything to the end of the line, spaces and all.
fn listening_entry(row: &[u8]) -> Option<Listening> {
    let mut rest = row;
    let mut columns: [&[u8]; 7] = [&[]; 7];
    for column in &mut columns {
        let start = rest.iter().position(|byte| *byte != b' ')?;
        rest = &rest[start..];
        let end = rest
            .iter()
            .position(|byte| *byte == b' ')
            .unwrap_or(rest.len());
        *column = &rest[..end];
        rest = &rest[end..];
    }
    let [_, _, _, flags, _, state, inode] = columns;
    if flags != LISTENING_FLAGS || state != UNCONNECTED {
        return None;
    }
    let path = rest.strip_prefix(b" ")?;
    if path.starts_with(b"@") {
        return None;
    }
    let inode = std::str::from_utf8(inode).ok()?.parse().ok()?;
    Some((inode, path_of(path)?))
}

/// The entries of one of `/proc`'s NUL-separated files — a process's arguments, or its
/// environment.
///
/// The file ends with a separator of its own, which leaves one empty entry past the last real
/// one; every other empty entry is kept, because an argument that is the empty string is an
/// argument the program was given.
fn nul_separated(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut entries: Vec<Vec<u8>> = bytes.split(|byte| *byte == 0).map(<[u8]>::to_vec).collect();
    if entries.last().is_some_and(Vec::is_empty) {
        entries.pop();
    }
    entries
}

/// Whether these arguments are a svolderd's: the program that was started, out of whichever
/// directory it was started from, is named `svolderd`.
fn is_svolderd(argv: &[Vec<u8>]) -> bool {
    argv.first()
        .and_then(|program| program.rsplit(|byte| *byte == b'/').next())
        .is_some_and(|name| name == SVOLDERD)
}

/// The path a `--socket PATH` or `--socket=PATH` argument names.
///
/// The last one is the one that counts, which is the one the daemon's own parser read, and a
/// blank one names nothing: an empty socket is no socket wherever it is resolved from.
fn socket_argument(argv: &[Vec<u8>]) -> Option<PathBuf> {
    let mut told = None;
    let mut args = argv.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg.as_slice() == SOCKET_FLAG {
            told = Some(args.next().map(Vec::as_slice).unwrap_or_default());
        } else if let Some(value) = arg.strip_prefix(SOCKET_FLAG_EQ) {
            told = Some(value);
        }
    }
    path_of(told?)
}

/// The path `SVOLDER_SOCKET` names in a process's environment, which is where a front end that
/// starts a daemon puts the socket it resolved.
///
/// The first entry is the one that counts, which is the one the daemon's own lookup read.
fn socket_variable(environ: &[Vec<u8>]) -> Option<PathBuf> {
    environ
        .iter()
        .find_map(|entry| entry.strip_prefix(SOCKET_VAR))
        .and_then(path_of)
}

/// The path some bytes name, and `None` when they name none: a blank path is no path, as every
/// other source of one is read.
fn path_of(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let named = !bytes.is_empty() && !bytes.iter().all(u8::is_ascii_whitespace);
    named.then(|| PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
}

/// The real user id out of a process's `status`, whose `Uid:` line reads
/// `Uid:\t<real>\t<effective>\t<saved>\t<filesystem>`.
fn owner(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// The process's real user id.
fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail; it only reads the process's real uid.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests;
