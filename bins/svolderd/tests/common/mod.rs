// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scaffolding the daemon's integration tests share: a temporary directory, a spawned
//! `svolderd` over the fake transport, and a client connected to it.
//!
//! Every test binary compiles this module of its own, and each reaches for a different part
//! of it, so what one binary does not call is not unused code.
#![allow(dead_code)]

use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use svolder_ipc::{Client, Request, Response};

/// How long any one test may take before it is a failure rather than a hang.
///
/// Every wait inside a test is a fraction of this, so a test that stalls fails at the step
/// that stalled — with a message naming what never happened — rather than at this outer bound
/// with nothing to say.
///
/// Thirty seconds, because what a case waits on is a daemon process starting, answering and
/// reconnecting under the load of a whole suite running beside it; the fake answers a scan at
/// once (`SCAN_WINDOW` in `bins/svolderd/src/scanner.rs` bounds only a real one), so a passing
/// run spends none of it and the bound only has to tell a hang from a slow machine.
///
/// Three seconds is the floor: [`STEP_BUDGET`] divides this by three in whole seconds, so
/// anything smaller leaves a step no time at all.
pub const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long one step inside a test waits. A third of the whole test's budget, so a test can
/// take two steps in a row and the step that stalled still reports itself, by name, before
/// [`TEST_TIMEOUT`] fires and says only that the test was slow.
pub const STEP_BUDGET: Duration = Duration::from_secs(TEST_TIMEOUT.as_secs() / 3);

/// How often a step that polls looks again.
pub const POLL: Duration = Duration::from_millis(10);

/// How many of those looks fit in one step's budget.
pub const POLLS: u32 = (STEP_BUDGET.as_millis() / POLL.as_millis()) as u32;

/// The config the daemon under test reads: one device, the fake's own unit id.
pub const CONFIG: &str = "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0102030405060708\"\n";

/// The descriptor a service manager passes the first listening socket on.
const LISTEN_FDS_START: i32 = 3;

/// A spawned `svolderd` and the temporary directory it was given, holding its config file, its
/// socket, its stderr, and whatever profile overrides a test writes. Dropping it kills the
/// child and removes the directory, so a failed assertion never leaves a daemon or its files
/// behind.
pub struct ChildGuard {
    child: Child,
    dir: PathBuf,
}

impl ChildGuard {
    /// The daemon's process id, for a signal a test sends by hand.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The configuration file the daemon was started with, which a reload re-reads.
    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.toml")
    }

    /// The directory the daemon reads profile overrides from, beside its config file.
    pub fn profiles_dir(&self) -> PathBuf {
        self.dir.join("profiles")
    }

    /// The daemon's stderr, for a test that spawned it with a pipe.
    pub fn child_stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.stderr.take()
    }

    /// Everything the daemon has written to the stderr file beside its config, for a failure
    /// message. Empty for a daemon whose stderr a test took as a pipe instead.
    pub fn said(&self) -> String {
        std::fs::read_to_string(self.dir.join("stderr.log")).unwrap_or_default()
    }

    /// How the daemon exited, if it already has. A request that fails asks this, so the
    /// failure says the daemon stopped rather than only that the socket would not carry it.
    pub fn exited(&mut self) -> Option<std::process::ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Wait for the daemon to exit of its own accord, or say how long it was given.
    pub async fn wait_for_exit(&mut self) -> std::process::ExitStatus {
        for _ in 0..POLLS {
            if let Some(status) = self.exited() {
                return status;
            }
            tokio::time::sleep(POLL).await;
        }
        panic!(
            "the daemon was still running {STEP_BUDGET:?} after it was asked to stop; it said: \
             {:?}",
            self.said()
        );
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A temporary directory of this test's own.
///
/// The counter is what makes the name unique: the clock alone can read the same nanosecond
/// twice for two of these tests running side by side, and one directory shared between two
/// daemons is one config file and one socket shared between them — the second daemon refuses
/// to start, and the first loses its socket when the other's guard removes the directory.
/// Creating the directory rather than creating it if absent keeps a reused name loud.
pub fn temp_dir() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("svolderd-test-{}-{n}-{seq}", std::process::id()));
    std::fs::create_dir(&dir).expect("create the test directory");
    dir
}

/// Spawn a daemon over the fake transport with `config` as its configuration file, its socket
/// beside it, and its stderr in `stderr.log` beside both, which a failure quotes.
pub fn spawn_daemon(config: &str) -> (ChildGuard, PathBuf) {
    spawn_with_stderr(config, None, &[])
}

/// Spawn a daemon whose socket is `socket_in(dir)`, `dir` being its own temporary directory,
/// for a test that wants a socket path other than the default one.
pub fn spawn_daemon_on(
    config: &str,
    socket_in: impl FnOnce(&Path) -> PathBuf,
) -> (ChildGuard, PathBuf) {
    let dir = temp_dir();
    let socket = socket_in(&dir);
    let path = write_config(&dir, config);
    let mut command = daemon_command(&dir, None);
    command
        .arg("--socket")
        .arg(&socket)
        .arg("--config")
        .arg(&path);
    let child = command.spawn().expect("spawn svolderd");
    (ChildGuard { child, dir }, socket)
}

/// Spawn a daemon with environment of its own, for a test that wants a fake device other than
/// the default one. Every `SVOLDER_FAKE_*` override is cleared first, so what a test names
/// here is all the child gets.
pub fn spawn_daemon_with_env(config: &str, envs: &[(&str, &str)]) -> (ChildGuard, PathBuf) {
    spawn_with_stderr(config, None, envs)
}

/// Spawn a daemon with every home it could reach for pointed into its own directory — the
/// XDG config, state, data and runtime directories — an adapter that does not exist, a
/// `systemctl` that is not there and the host's audio left alone, plus `envs`. The broker
/// password and MCP token variables are cleared, so only a case that names one hands it over.
pub fn spawn_daemon_isolated(config: &str, envs: &[(&str, &str)]) -> (ChildGuard, PathBuf) {
    spawn_daemon_isolated_with_profiles(config, &[], envs)
}

/// [`spawn_daemon_isolated`], with each `(name, text)` of `profiles` written as a profile
/// override in the directory the daemon reads them from before it starts.
pub fn spawn_daemon_isolated_with_profiles(
    config: &str,
    profiles: &[(&str, &str)],
    envs: &[(&str, &str)],
) -> (ChildGuard, PathBuf) {
    let dir = temp_dir();
    let socket = dir.join("svolder.sock");
    let path = write_config(&dir, config);
    if !profiles.is_empty() {
        let overrides = dir.join("profiles");
        std::fs::create_dir(&overrides).expect("create the profiles directory");
        for (name, text) in profiles {
            std::fs::write(overrides.join(format!("{name}.toml")), text)
                .expect("write a profile override");
        }
    }
    let mut command = daemon_command(&dir, None);
    command
        .arg("--socket")
        .arg(&socket)
        .arg("--config")
        .arg(&path)
        .env("XDG_CONFIG_HOME", &dir)
        .env("XDG_STATE_HOME", &dir)
        .env("XDG_DATA_HOME", &dir)
        .env("XDG_RUNTIME_DIR", &dir)
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_SYSTEMCTL", dir.join("no-systemctl"))
        .env("SVOLDER_HOST", "off")
        .env_remove("SVOLDER_MQTT_PASSWORD")
        .env_remove("SVOLDER_MCP_TOKEN");
    for (key, value) in envs {
        command.env(key, value);
    }
    let child = command.spawn().expect("spawn svolderd");
    (ChildGuard { child, dir }, socket)
}

/// Spawn a daemon over the fake transport with its stderr piped, for a test that reads the
/// daemon's log lines as they arrive rather than after the fact.
pub fn spawn_daemon_with_piped_stderr(config: &str) -> (ChildGuard, PathBuf) {
    spawn_with_stderr(config, Some(Stdio::piped()), &[])
}

/// Spawn the daemon. With no `stderr`, it goes to `stderr.log` in the daemon's own directory.
fn spawn_with_stderr(
    config: &str,
    stderr: Option<Stdio>,
    envs: &[(&str, &str)],
) -> (ChildGuard, PathBuf) {
    let dir = temp_dir();
    let socket = dir.join("svolder.sock");
    let path = write_config(&dir, config);
    let mut command = daemon_command(&dir, stderr);
    command
        .arg("--socket")
        .arg(&socket)
        .arg("--config")
        .arg(&path);
    for (key, value) in envs {
        command.env(key, value);
    }
    let child = command.spawn().expect("spawn svolderd");
    (ChildGuard { child, dir }, socket)
}

/// Spawn a daemon the way a service manager with a socket unit does: the socket is bound
/// here and handed over on descriptor 3 with `LISTEN_FDS=1`, so the daemon adopts it rather
/// than binding one of its own and never learns the path from `--socket`.
pub fn spawn_daemon_with_handed_over_socket(config: &str) -> (ChildGuard, PathBuf) {
    let dir = temp_dir();
    let socket = dir.join("svolder.sock");
    let path = write_config(&dir, config);
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind the test socket");
    let handed_over = listener.as_raw_fd();
    let mut command = daemon_command(&dir, None);
    command
        .env("LISTEN_FDS", "1")
        .env_remove("LISTEN_PID")
        .arg("--config")
        .arg(&path);
    // SAFETY: the closure runs between fork and exec and calls nothing but `dup2` and
    // `fcntl`, both async-signal-safe. It puts the listener on descriptor 3 without the
    // close-on-exec flag, which is where a service manager leaves a socket it hands over.
    unsafe {
        command.pre_exec(move || {
            if handed_over == LISTEN_FDS_START {
                if libc::fcntl(LISTEN_FDS_START, libc::F_SETFD, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            } else if libc::dup2(handed_over, LISTEN_FDS_START) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().expect("spawn svolderd");
    drop(listener);
    (ChildGuard { child, dir }, socket)
}

/// Write the daemon's configuration file into `dir` and hand back its path.
fn write_config(dir: &Path, config: &str) -> PathBuf {
    let path = dir.join("config.toml");
    std::fs::write(&path, config).expect("write the test config");
    path
}

/// The daemon's command line over the fake transport, with its stderr in `stderr.log` beside
/// its config unless the caller asked for something else.
///
/// Every `SVOLDER_FAKE_*` override is cleared, so a developer's own shell environment cannot
/// change the device the daemon under test talks to, and so are the socket and the handover,
/// so only what a spawner sets reaches the child. The host is off, so the daemon never reaches
/// the developer's session bus or its players; a case that wants the fake host says `fake`.
fn daemon_command(dir: &Path, stderr: Option<Stdio>) -> Command {
    let stderr = stderr.unwrap_or_else(|| {
        Stdio::from(
            std::fs::File::create(dir.join("stderr.log")).expect("create the daemon's log file"),
        )
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_svolderd"));
    command
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_HOST", "off")
        .env_remove("SVOLDER_SOCKET")
        .env_remove("SVOLDER_FAKE_PROFILE")
        .env_remove("SVOLDER_FAKE_UNIT")
        .env_remove("SVOLDER_FAKE_FIRMWARE")
        .env_remove("SVOLDER_FAKE_PASSPHRASE")
        .env_remove("SVOLDER_FAKE_FAULTS")
        .env_remove("SVOLDER_FAKE_SIGNAL")
        .env_remove("SVOLDER_FAKE_LOKI_REPORTS")
        .env_remove("SVOLDER_FAKE_SILENT_RECALL")
        .env_remove("SVOLDER_FAKE_QUIET_SAME")
        .env_remove("SVOLDER_FAKE_SEVER_AFTER_MS")
        .env_remove("SVOLDER_FAKE_COVER_GONE_AFTER_MS")
        .env_remove("SVOLDER_FAKE_PLAYER_GONE_AFTER_MS")
        .env_remove("SVOLDER_FAKE_ROUTE")
        .env_remove("SVOLDER_FAKE_CAPTURE")
        .env_remove("SVOLDER_FAKE_HOST_EQ")
        .env_remove("SVOLDER_FAKE_TITLE")
        .env_remove("LISTEN_FDS")
        .env_remove("LISTEN_PID")
        .stdout(Stdio::null())
        .stderr(stderr);
    command
}

/// Connect to a daemon's socket, waiting for it to appear.
pub async fn connect(guard: &mut ChildGuard, socket: &Path) -> Client {
    for _ in 0..POLLS {
        if socket.exists()
            && let Ok(client) = Client::connect(socket).await
        {
            return client;
        }
        tokio::time::sleep(POLL).await;
    }
    // Whether the socket is there says which half failed — no socket is a daemon that never
    // listened, a socket with nothing behind it is one that stopped after — and the daemon's
    // own exit and log say why.
    panic!(
        "the daemon never came up within {STEP_BUDGET:?}; its socket {} is {}, it exited with \
         {:?}, and it said: {:?}",
        socket.display(),
        if socket.exists() { "present" } else { "absent" },
        guard.exited(),
        guard.said()
    );
}

/// Start a daemon over the fake transport and connect to it, waiting until its one device
/// is connected so a state read has something to report.
pub async fn daemon() -> (ChildGuard, Client, PathBuf) {
    let (mut guard, socket) = spawn_daemon(CONFIG);
    let mut client = connect(&mut guard, &socket).await;
    wait_until_connected(&mut guard, &mut client).await;
    (guard, client, socket)
}

/// Wait until the daemon reports at least one connected device.
pub async fn wait_until_connected(guard: &mut ChildGuard, client: &mut Client) {
    for _ in 0..POLLS {
        let devices = client
            .request(Request::Devices)
            .await
            .expect("a devices reply");
        if matches!(&devices, Response::Devices { devices } if devices.iter().any(|d| d.connected))
        {
            return;
        }
        tokio::time::sleep(POLL).await;
    }
    panic!(
        "the daemon never connected to the fake device within {STEP_BUDGET:?}; it said: {:?}",
        guard.said()
    );
}

/// The names the daemon currently reports for its configured devices.
pub async fn device_names(client: &mut Client) -> Vec<String> {
    let response = client
        .request(Request::Devices)
        .await
        .expect("a devices reply");
    let Response::Devices { devices } = response else {
        panic!("expected a device list, got {response:?}");
    };
    devices.into_iter().map(|d| d.name).collect()
}
