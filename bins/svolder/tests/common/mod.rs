// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers shared by the integration tests.
//!
//! Every test binary compiles this module of its own, and each reaches for a different part
//! of it, so what one binary does not call is not unused code.
#![allow(dead_code)]

#[path = "../../../../crates/svolder-host/tests/common/script.rs"]
mod script;

#[allow(
    unused_imports,
    reason = "each test binary uses the part of this it needs"
)]
pub use script::install_script;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// How long a watching case waits for the line it is looking for.
///
/// Generous on purpose: the child has to connect, read a snapshot and print it while the rest
/// of the suite — five other daemon cases and a nested `cargo build` — competes for the same
/// cores. The case ends as soon as the line arrives, so a long deadline costs nothing.
pub const WATCH_DEADLINE: Duration = Duration::from_secs(20);

/// A spawned `svolder watch` and the lines it has printed.
///
/// `watch` never ends on its own — over a fake device the link never drops — so an orphan
/// would outlive the whole test run. Dropping this kills the child and reaps it, on a failed
/// assertion as much as on a clean end.
pub struct Watcher {
    child: Child,
    /// Lines from the child's stdout, read by a thread of their own so a deadline can be
    /// applied to waiting for one.
    lines: Receiver<String>,
    /// Everything read so far, for the failure message.
    seen: String,
}

impl Watcher {
    /// Take a spawned child whose stdout is piped, and start reading it.
    pub fn new(mut child: Child) -> Watcher {
        let stdout = child.stdout.take().expect("the child's stdout is piped");
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return; // the test is gone; stop reading
                }
            }
        });
        Watcher {
            child,
            lines,
            seen: String::new(),
        }
    }

    /// Read lines until one contains `needle`, and say whether one did.
    ///
    /// Gives up when `deadline` passes with no such line, or when the child's stdout ends.
    pub fn wait_for_line(&mut self, needle: &str, deadline: Duration) -> bool {
        let until = Instant::now() + deadline;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            // An error is the deadline or the end of the child's output; either way there is
            // no line to find.
            let Ok(line) = self.lines.recv_timeout(left) else {
                return false;
            };
            let found = line.contains(needle);
            self.seen.push_str(&line);
            self.seen.push('\n');
            if found {
                return true;
            }
        }
    }

    /// Send the child SIGINT, as Ctrl-C in its terminal does.
    pub fn interrupt(&self) {
        // SAFETY: `kill` takes a pid and a signal number and touches no memory of this process.
        unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGINT) };
    }

    /// The child's exit status once it has left, or `None` when `deadline` passes first.
    pub fn exit_within(&mut self, deadline: Duration) -> Option<std::process::ExitStatus> {
        let until = Instant::now() + deadline;
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Some(status);
            }
            if Instant::now() >= until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Everything the child has printed that this watcher has read.
    pub fn output(&self) -> &str {
        &self.seen
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A path under the system temp dir that one test owns — a directory, or a socket file —
/// removed when the test lets go of it, whether the test passed or an assertion failed first.
pub struct TempPath(pub PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.0).is_err() {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

/// A directory under the system temp dir that one test owns, created empty with mode 0700 and
/// removed, with whatever is in it, when the test lets go of it.
///
/// The name is never reused: a directory that exists already is left alone and the next name
/// is tried.
pub struct TempDir(PathBuf);

impl TempDir {
    /// Make a directory whose name starts `svolder-<tag>-<pid>-`.
    pub fn new(tag: &str) -> TempDir {
        use std::os::unix::fs::DirBuilderExt;
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        loop {
            let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("svolder-{tag}-{}-{seq}", std::process::id()));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return TempDir(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create {}: {e}", path.display()),
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The `svolderd` a case spawns.
///
/// With `SVOLDER_TEST_PREBUILT=1` (what `make test` sets) every binary was built by the one
/// workspace invocation, so the sibling of the `svolder` under test is used as it stands.
/// Otherwise the daemon is built once into `<target>/test-daemon`, a directory of its own: the
/// build a single package gets is not the workspace build's artifact, and sharing the path would
/// relink a daemon another suite may be running.
///
/// The first bare run compiles the daemon's whole dependency tree here, so a case calls this
/// before its own deadline starts, never inside it.
pub fn daemon_binary() -> PathBuf {
    static DAEMON: OnceLock<PathBuf> = OnceLock::new();
    DAEMON
        .get_or_init(|| {
            let svolder = Path::new(env!("CARGO_BIN_EXE_svolder"));
            if std::env::var("SVOLDER_TEST_PREBUILT").as_deref() == Ok("1") {
                let path = svolder.with_file_name("svolderd");
                assert!(
                    path.exists(),
                    "{} is not there: SVOLDER_TEST_PREBUILT=1 says the caller already built \
                     every binary in the workspace, so build the workspace first or unset the \
                     variable and let this build the daemon itself",
                    path.display()
                );
                return path;
            }
            let dir = daemon_target_dir(svolder);
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
            let built = Command::new(cargo)
                .args(["build", "-q", "-p", "svolderd", "--target-dir"])
                .arg(&dir)
                .status()
                .expect("run cargo build");
            assert!(built.success(), "cargo build -p svolderd failed");
            let path = dir.join("debug").join("svolderd");
            assert!(
                path.exists(),
                "{} is not there, even though cargo build -p svolderd reported success",
                path.display()
            );
            path
        })
        .clone()
}

/// Where the private daemon build goes, given the path of the `svolder` binary under test
/// (`<target>/debug/svolder`): `<target>/test-daemon`.
pub fn daemon_target_dir(svolder_exe: &Path) -> PathBuf {
    svolder_exe
        .parent()
        .and_then(Path::parent)
        .expect("a target directory")
        .join("test-daemon")
}
