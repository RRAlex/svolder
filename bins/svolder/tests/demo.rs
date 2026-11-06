// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder tui --demo` on a pseudo-terminal: the demo daemon starts in a private directory,
//! and every way the TUI can end leaves neither the daemon nor the directory behind.
//!
//! `script(1)` supplies the terminal. Every run gets a home of its own for each XDG directory,
//! a `systemctl` that is not there and the host's audio asked to be on (the demo must turn it
//! off), so no case can reach this machine's daemon, socket, configuration or state. The demo's
//! directory is made under the system's temporary directory, as it always is, and a case finds
//! its processes by what they were started with: the TUI by its executable and the case's own
//! runtime directory in its environment, the daemon as that TUI's child.

mod common;

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use common::TempPath;

/// How long a step of a case may take: starting a daemon, finding a process, one ending.
const STEP: Duration = Duration::from_secs(20);

/// The `svolder` a case runs.
///
/// The demo starts the `svolderd` that stands beside the `svolder` it runs from. With
/// `SVOLDER_TEST_PREBUILT=1` that is the `svolder` under test, whose sibling the workspace build
/// made. Otherwise it is a copy of it placed beside the private daemon build, so the demo finds
/// that daemon and nothing is built into `target/debug`. The copy is made once per process, under
/// a temporary name renamed into place, so a copy another run is executing is never written to.
fn svolder_under_test() -> PathBuf {
    static EXE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    EXE.get_or_init(|| {
        let own = PathBuf::from(env!("CARGO_BIN_EXE_svolder"));
        if std::env::var("SVOLDER_TEST_PREBUILT").as_deref() == Ok("1") {
            return own;
        }
        let beside = common::daemon_binary().with_file_name("svolder");
        let staged = beside.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::copy(&own, &staged).expect("copy svolder beside the private daemon");
        std::fs::rename(&staged, &beside).expect("put the copy in place");
        beside
    })
    .clone()
}

/// A `svolder tui --demo` on a terminal of its own, and the case's home.
struct Demo {
    /// The `script` that holds the terminal; its child is the TUI.
    script: Child,
    /// What the terminal shows, read by a thread of its own.
    screen: Receiver<Vec<u8>>,
    seen: Vec<u8>,
    home: TempPath,
    /// The `svolder` the TUI runs as.
    svolder: PathBuf,
}

impl Demo {
    /// `None`, with the reason printed, on a machine with no `script(1)` to give a terminal.
    fn start() -> Option<Demo> {
        Demo::start_with("--demo")
    }

    /// [`Demo::start`] with `flags` after `tui`. Never `--host`: no case touches the real host.
    fn start_with(flags: &str) -> Option<Demo> {
        assert!(!flags.contains("--host"), "no case runs with the host on");
        if matches!(
            Command::new("script").arg("--version").output(),
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound
        ) {
            eprintln!("skipped: script(1) is not installed, so there is no pseudo-terminal");
            return None;
        }
        let svolder = svolder_under_test();
        let home = TempPath(std::env::temp_dir().join(format!(
            "svolder-demo-test-{}-{}-{:?}",
            std::process::id(),
            {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after 1970")
                .as_nanos()
        )));
        std::fs::create_dir(&home.0).expect("create the case's home");
        let scratch = &home.0;
        let mut script = Command::new("script")
            .args([
                "-qec",
                &format!(
                    "stty cols 80 rows 40; exec {} tui {flags}",
                    svolder.display()
                ),
            ])
            .arg("/dev/null")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("TERM", "xterm-256color")
            .env("HOME", scratch)
            .env("XDG_CONFIG_HOME", scratch.join("user-config"))
            .env("XDG_STATE_HOME", scratch.join("user-state"))
            .env("XDG_DATA_HOME", scratch.join("user-data"))
            .env("XDG_RUNTIME_DIR", scratch)
            .env("SVOLDER_HOST", "on")
            .env("SVOLDER_SOCKET", scratch.join("not-this.sock"))
            .env("SVOLDER_SYSTEMCTL", scratch.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", scratch.join("no-wireplumber"))
            .env("SVOLDER_UNITDIR", scratch.join("units"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("run script");
        let mut out = script
            .stdout
            .take()
            .expect("the terminal's output is piped");
        let (tx, screen) = channel();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = out.read(&mut chunk) {
                if n == 0 || tx.send(chunk[..n].to_vec()).is_err() {
                    return;
                }
            }
        });
        Some(Demo {
            script,
            screen,
            seen: Vec::new(),
            home,
            svolder,
        })
    }

    /// Type `keys` at the TUI.
    fn type_keys(&mut self, keys: &[u8]) {
        let stdin = self
            .script
            .stdin
            .as_mut()
            .expect("the terminal's input is piped");
        let _ = stdin.write_all(keys);
        let _ = stdin.flush();
    }

    /// Read what the terminal shows until `needle` is on it, or [`STEP`] passes.
    fn shows(&mut self, needle: &str) -> bool {
        let until = Instant::now() + STEP;
        loop {
            if without_escapes(&self.seen).contains(needle) {
                return true;
            }
            let left = until.saturating_duration_since(Instant::now());
            match self.screen.recv_timeout(left) {
                Ok(bytes) => self.seen.extend(bytes),
                Err(_) => return false,
            }
        }
    }

    /// The TUI: this case's `svolder`, started with the case's runtime directory.
    fn tui(&self) -> Option<u32> {
        let scratch = format!("XDG_RUNTIME_DIR={}", self.home.0.display());
        processes().into_iter().find_map(|(pid, args)| {
            let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
            let env = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
            (exe == self.svolder
                && args.iter().any(|a| a == "--demo")
                && String::from_utf8_lossy(&env)
                    .split('\0')
                    .any(|e| e == scratch))
            .then_some(pid)
        })
    }

    /// The demo's daemon, which is the TUI's child, and the directory it serves in.
    fn daemon(&self) -> Option<(u32, PathBuf)> {
        let tui = self.tui()?;
        processes().into_iter().find_map(|(pid, args)| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let parent: u32 = stat
                .rsplit_once(')')?
                .1
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()?;
            let at = args.iter().position(|a| a == "--socket")?;
            (parent == tui && args[0].ends_with("svolderd"))
                .then(|| Some((pid, Path::new(&args[at + 1]).parent()?.to_path_buf())))
                .flatten()
        })
    }

    /// Wait for the daemon to be up, and return its pid and directory.
    fn wait_for_daemon(&self) -> (u32, PathBuf) {
        let until = Instant::now() + STEP;
        loop {
            if let Some((pid, dir)) = self.daemon()
                && dir.join("sv.sock").exists()
            {
                return (pid, dir);
            }
            assert!(Instant::now() < until, "no demo daemon came up");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Wait until the daemon `pid` is gone and, when `directory_too`, `dir` with it.
    fn gone(&self, pid: u32, dir: &Path, directory_too: bool) -> bool {
        let until = Instant::now() + STEP;
        loop {
            let alive = std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|c| !c.is_empty());
            if !alive && (!directory_too || !dir.exists()) {
                return true;
            }
            if Instant::now() >= until {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Wait for the TUI to have the terminal and say how a second front end joins, and return
    /// its pid.
    fn wait_for_tui(&mut self, dir: &Path) -> u32 {
        let command = format!("svolder tui --join {}", dir.join("sv.sock").display());
        assert!(
            self.shows(&command),
            "no demo toast with {command:?} on one line; the screen was: {:?}",
            String::from_utf8_lossy(&self.seen)
        );
        self.tui().expect("the TUI is running")
    }
}

impl Drop for Demo {
    fn drop(&mut self) {
        // A failed case must not leave a daemon: it is stopped by hand, then the terminal.
        if let Some((pid, _)) = self.daemon() {
            signal(pid, libc::SIGKILL);
        }
        if let Some(pid) = self.tui() {
            signal(pid, libc::SIGKILL);
        }
        let _ = self.script.kill();
        let _ = self.script.wait();
    }
}

/// Send `signal` to the process `pid`.
fn signal(pid: u32, signal: libc::c_int) {
    // SAFETY: `kill` takes a pid and a signal and reads no memory of ours; the pids here are
    // processes this case found by what it started them with, and at worst one that has just
    // exited, which the call then reports as an error that is ignored.
    unsafe { libc::kill(pid as i32, signal) };
}

/// `bytes` as text with the terminal's escape sequences (CSI and OSC) taken out, so a line the
/// TUI drew in several writes reads as the text it shows.
fn without_escapes(bytes: &[u8]) -> String {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != 0x1b {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        i += 1;
        match bytes.get(i) {
            Some(b'[') => {
                i += 1;
                while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                    i += 1;
                }
                i += 1;
            }
            Some(b']') => {
                while i < bytes.len() && bytes[i] != 0x07 && bytes[i] != 0x1b {
                    i += 1;
                }
                i += if bytes.get(i) == Some(&0x1b) { 2 } else { 1 };
            }
            _ => i += 1,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Every process's pid and argument list, from `/proc`.
fn processes() -> Vec<(u32, Vec<String>)> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let pid: u32 = entry.ok()?.file_name().to_str()?.parse().ok()?;
            Some((pid, args_of(pid)?))
        })
        .collect()
}

fn args_of(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        raw.split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect(),
    )
}

/// The daemon runs over the fake transport in a private 0700 directory with homes of its own,
/// the TUI says once how a second front end reaches it (the whole command, on one line at 80
/// columns), nothing lands in the home the case gave the user, the TUI turns the host's audio off
/// whatever the caller said, and `q` ends the TUI, the daemon and the directory.
#[test]
fn the_demo_runs_in_a_private_directory_and_quitting_removes_it() {
    let Some(mut demo) = Demo::start() else {
        return;
    };
    let (pid, dir) = demo.wait_for_daemon();
    let mode = std::fs::metadata(&dir)
        .expect("the directory exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700, "the directory is private");
    let env = std::fs::read(format!("/proc/{pid}/environ")).expect("the daemon's environment");
    let env = String::from_utf8_lossy(&env).into_owned();
    for want in [
        "SVOLDER_TRANSPORT=fake".to_string(),
        "SVOLDER_HOST=fake".to_string(),
        format!("XDG_CONFIG_HOME={}", dir.join("config").display()),
        format!("XDG_STATE_HOME={}", dir.join("state").display()),
        format!("XDG_DATA_HOME={}", dir.join("data").display()),
        format!("XDG_RUNTIME_DIR={}", dir.display()),
        format!("SVOLDER_SYSTEMCTL={}", dir.join("no-systemctl").display()),
        format!("SVOLDER_UNITDIR={}", dir.join("units").display()),
    ] {
        assert!(env.split('\0').any(|e| e == want), "{want} is in {env:?}");
    }
    assert!(
        !env.contains("MQTT"),
        "no broker password reaches the daemon: {env:?}"
    );
    let config = std::fs::read_to_string(dir.join("config/svolder/config.toml")).expect("a config");
    assert!(!config.contains("mqtt"), "{config}");
    let tui = demo.wait_for_tui(&dir);
    // The TUI's own environment is set after it starts, which /proc does not show; the
    // home it reads from is shown by where nothing was made, below.
    let _ = tui;
    // Nothing of the user's was made, in the homes the case named or in the ones a leak would
    // fall back to.
    for untouched in [
        "user-config",
        "user-state",
        "user-data",
        ".config",
        ".local",
    ] {
        assert!(
            !demo.home.0.join(untouched).exists(),
            "{untouched} was made"
        );
    }
    // The first `q` asks, the second answers.
    demo.type_keys(b"q");
    assert!(demo.shows("Quit svolder?"));
    demo.type_keys(b"q");
    assert!(
        demo.gone(pid, &dir, true),
        "the daemon or the directory outlived q"
    );
}

/// Ctrl-C is a key in the TUI and ends the demo the way `q` does.
#[test]
fn ctrl_c_ends_the_demo_and_removes_the_directory() {
    let Some(mut demo) = Demo::start() else {
        return;
    };
    let (pid, dir) = demo.wait_for_daemon();
    demo.wait_for_tui(&dir);
    demo.type_keys(b"\x03");
    assert!(
        demo.gone(pid, &dir, true),
        "the daemon or the directory outlived Ctrl-C"
    );
}

/// A TUI that is told to stop (SIGTERM, or the hangup a closed window sends) gives the
/// terminal back, stops the daemon and removes the directory.
#[test]
fn a_terminated_tui_removes_the_daemon_and_the_directory() {
    let Some(mut demo) = Demo::start() else {
        return;
    };
    let (pid, dir) = demo.wait_for_daemon();
    let tui = demo.wait_for_tui(&dir);
    signal(tui, libc::SIGTERM);
    assert!(
        demo.gone(pid, &dir, true),
        "the daemon or the directory outlived SIGTERM"
    );
}

/// A signal while the daemon is still starting removes the directory too: it is the guard's from
/// the moment it is made.
#[test]
fn a_signal_during_start_removes_the_directory() {
    let Some(demo) = Demo::start() else {
        return;
    };
    // The TUI is signalled as soon as its daemon is a child of it, which is before the daemon
    // has been waited for or, on a slow machine, has answered.
    let until = Instant::now() + STEP;
    let (pid, dir) = loop {
        if let Some(found) = demo.daemon() {
            break found;
        }
        assert!(Instant::now() < until, "no demo daemon was started");
    };
    let tui = demo.tui().expect("the TUI is running");
    signal(tui, libc::SIGTERM);
    assert!(
        demo.gone(pid, &dir, true),
        "the daemon or the directory outlived the signal"
    );
}

/// A TUI killed outright cannot clean up after itself, and still leaves no daemon: the kernel
/// ends the daemon with its parent.
#[test]
fn a_killed_tui_leaves_no_daemon() {
    let Some(mut demo) = Demo::start() else {
        return;
    };
    let (pid, dir) = demo.wait_for_daemon();
    let tui = demo.wait_for_tui(&dir);
    signal(tui, libc::SIGKILL);
    assert!(demo.gone(pid, &dir, false), "the daemon outlived its TUI");
    // The next demo removes what this one left; clear it here so the case leaves nothing in /tmp.
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--web` has the demo daemon serve the web UI: the configuration names a port that is none of
/// the reserved ones on every interface with token sign-in and writes on, and the page answers
/// there (the daemon's host is the fake one, as in every case here).
#[test]
fn the_web_option_serves_the_page() {
    let Some(mut demo) = Demo::start_with("--demo --web") else {
        return;
    };
    let (pid, dir) = demo.wait_for_daemon();
    let config = std::fs::read_to_string(dir.join("config/svolder/config.toml")).expect("a config");
    let listen = config
        .lines()
        .find_map(|l| l.strip_prefix("listen = \"0.0.0.0:"))
        .and_then(|l| l.strip_suffix('"'))
        .unwrap_or_else(|| panic!("no web listen in {config}"));
    let port: u16 = listen.parse().expect("a port");
    assert!(![8737, 8738, 8741].contains(&port), "{port}");
    for want in [
        "[web]",
        "enabled = true",
        "auth = \"token\"",
        "writes = true",
    ] {
        assert!(config.contains(want), "{want} in {config}");
    }
    let until = Instant::now() + STEP;
    let answer = loop {
        if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream.set_read_timeout(Some(STEP));
            let _ = stream.write_all(
                format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            );
            let mut text = Vec::new();
            let _ = stream.read_to_end(&mut text);
            break String::from_utf8_lossy(&text).into_owned();
        }
        assert!(
            Instant::now() < until,
            "the web UI never listened on {port}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer:.200}");
    assert!(answer.to_lowercase().contains("<html"), "{answer:.200}");
    demo.wait_for_tui(&dir);
    demo.type_keys(b"q");
    assert!(demo.shows("Quit svolder?"));
    demo.type_keys(b"q");
    assert!(
        demo.gone(pid, &dir, true),
        "the daemon or the directory outlived q"
    );
}
