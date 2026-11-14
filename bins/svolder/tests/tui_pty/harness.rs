// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder tui --demo` in a pty of a given size: keys, mouse and resizes go in, the parsed screen
//! and the raw bytes come out.
//!
//! Everything runs in temporary homes (one directory holds `HOME`, every XDG directory, the
//! runtime directory and `TMPDIR`, so the demo's private directory is made under it as well),
//! over the fake transport, with no host audio and no network: nothing of the machine's is read
//! or written. No wait is a fixed sleep: a wait polls the screen and is woken by every chunk the
//! program writes, up to a deadline.

#![allow(
    dead_code,
    reason = "the harness API is wider than the cases of one task use; later cases take the rest"
)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::common::{self, TempDir};
use super::pty;
use super::tap::{Query, Responder, Shared};

/// How long a wait for something on the screen may take. Generous on purpose: a run beside
/// a cargo build is slow, and a deadline is only ever reached by a failure.
pub const WAIT: Duration = Duration::from_secs(60);

/// What a program must never leave drawn: the text of a terminal reply (echoed or misread), not a
/// screen.
pub const STRAY: [&str; 7] = ["[?", "62;", "^[", "rgb:", "_Gi=", "OK\\", "]11;"];

/// What a mouse gesture is. Columns and rows are 0-based, as [`Tui::cell`]'s are.
#[derive(Clone, Copy, Debug)]
pub enum Mouse {
    /// Two left clicks written together, so a loaded machine does not put them further apart
    /// than the double-click window when it reads them.
    DoubleClick,
    /// A left press and release.
    Click,
    /// A left press, motion to the cell, and the release there.
    DragTo(u16, u16),
    WheelUp,
    WheelDown,
    /// The pointer moves to the cell with no button down.
    Move,
}

/// A cell of the screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    pub fg: vt100::Color,
    pub bg: vt100::Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

/// A running `svolder tui`.
pub struct Tui {
    shared: Arc<Shared>,
    master: Mutex<pty::Master>,
    pid: u32,
    home: TempDir,
    exited: Arc<AtomicBool>,
    exit: Receiver<std::process::ExitStatus>,
    status: Option<std::process::ExitStatus>,
}

impl Tui {
    /// `svolder tui --demo` of `cols` x `rows`, once it has the alternate screen. The terminal
    /// answers queries as `responder` says.
    pub fn start(cols: u16, rows: u16, responder: Responder) -> Tui {
        Tui::start_with(cols, rows, responder, &["--demo"], &[])
    }

    /// [`Tui::start`] with these arguments after `tui` and these extra environment variables.
    /// Never `--host`.
    pub fn start_with(
        cols: u16,
        rows: u16,
        responder: Responder,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Tui {
        assert!(!args.contains(&"--host"), "no case runs with the host on");
        let mut all = vec!["tui"];
        all.extend_from_slice(args);
        let tui = Tui::launch(
            cols,
            rows,
            responder,
            &common::svolder_with_daemon(),
            &all,
            env,
            None,
        );
        tui.wait_for("the alternate screen", WAIT, |s| s.alternate_screen());
        tui
    }

    /// `svolder tui` (no demo, no daemon: the fake device in a direct session) of `cols` x `rows`,
    /// once it has the alternate screen, reading a configuration with the device and these lines
    /// under `[tui]`. The terminal answers queries as `responder` says.
    pub fn start_plain(cols: u16, rows: u16, responder: Responder, tui_lines: &str) -> Tui {
        let config = format!(
            "schema = 1\n[daemon]\nautospawn = false\n[[device]]\nname = \"demo\"\n\
             udid = \"0102030405060708\"\n[tui]\n{tui_lines}\n"
        );
        let tui = Tui::launch(
            cols,
            rows,
            responder,
            &common::svolder_with_daemon(),
            &["tui"],
            &[],
            Some(&config),
        );
        tui.wait_for("the alternate screen", WAIT, |s| s.alternate_screen());
        tui
    }

    /// [`Tui::start`] without the demo, over the fake transport, with `config` as the
    /// configuration file the run starts from: a run that reads what an earlier one saved.
    pub fn start_with_config(cols: u16, rows: u16, config: &str) -> Tui {
        let tui = Tui::launch_in(
            cols,
            rows,
            Responder::None,
            &common::svolder_with_daemon(),
            &["tui"],
            &[],
            Some(config),
        );
        tui.wait_for("the alternate screen", WAIT, |s| s.alternate_screen());
        tui
    }

    /// Run `program` with `args` in the same pty and homes, with no wait for a screen: the
    /// harness's own tests run small shell scripts this way.
    pub fn run_program(cols: u16, rows: u16, program: &Path, args: &[&str]) -> Tui {
        Tui::launch(cols, rows, Responder::None, program, args, &[], None)
    }

    fn launch(
        cols: u16,
        rows: u16,
        responder: Responder,
        program: &Path,
        args: &[&str],
        extra_env: &[(&str, &str)],
        config: Option<&str>,
    ) -> Tui {
        Tui::launch_in(cols, rows, responder, program, args, extra_env, config)
    }

    fn launch_in(
        cols: u16,
        rows: u16,
        responder: Responder,
        program: &Path,
        args: &[&str],
        extra_env: &[(&str, &str)],
        config: Option<&str>,
    ) -> Tui {
        let home = TempDir::new("pty");
        let dir = home.path();
        if let Some(config) = config {
            let folder = dir.join("config").join("svolder");
            std::fs::create_dir_all(&folder).expect("make the config folder");
            std::fs::write(folder.join("config.toml"), config).expect("write the config");
        }
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(dir)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("TERM", "xterm-256color")
            .env("HOME", dir)
            .env("TMPDIR", dir)
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .env("XDG_STATE_HOME", dir.join("state"))
            .env("XDG_DATA_HOME", dir.join("data"))
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .env("XDG_RUNTIME_DIR", dir)
            .env("SVOLDER_TRANSPORT", "fake")
            .env("SVOLDER_ADAPTER", "hci-none")
            .env("SVOLDER_HOST", "off")
            .env("SVOLDER_SOCKET", dir.join("not-this.sock"))
            .env("SVOLDER_SYSTEMCTL", dir.join("no-systemctl"))
            .env("SVOLDER_WIREPLUMBER", dir.join("no-wireplumber"))
            .env("SVOLDER_UNITDIR", dir.join("units"))
            .env("SVOLDER_AUTOEQ_URL", "http://127.0.0.1:9/");
        for (key, value) in extra_env {
            command.env(key, value);
        }
        let (master, slave) =
            pty::open(cols, rows, responder.pixels(cols, rows)).expect("open a pty");
        let mut child = pty::spawn(command, &slave).expect("start svolder tui");
        // Only the child (and what it passes the slave to) holds the slave now, so the master
        // reads end when they are gone.
        drop(slave);
        let pid = child.id();

        let shared = Arc::new(Shared::new(cols, rows, responder));
        let mut reading = master.file.try_clone().expect("clone the master");
        let mut answering = master.file.try_clone().expect("clone the master");
        let tap = Arc::clone(&shared);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 16384];
            loop {
                match reading.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => tap.take(&chunk[..n], &mut answering),
                }
            }
            tap.close();
        });

        let exited = Arc::new(AtomicBool::new(false));
        let (tx, exit) = channel();
        let flag = Arc::clone(&exited);
        std::thread::spawn(move || {
            if let Ok(status) = child.wait() {
                flag.store(true, Ordering::SeqCst);
                let _ = tx.send(status);
            }
        });

        Tui {
            shared,
            master: Mutex::new(master),
            pid,
            home,
            exited,
            exit,
            status: None,
        }
    }

    /// The program's process id.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// The directory that is every home of the run.
    pub fn home(&self) -> &Path {
        self.home.path()
    }

    /// Wait until `ready` holds of the screen, panicking with the screen if `timeout` passes
    /// first. `what` says what the wait is for.
    pub fn wait_for(&self, what: &str, timeout: Duration, ready: impl Fn(&vt100::Screen) -> bool) {
        if let Err(why) = self.try_wait_for(timeout, ready) {
            panic!("timed out waiting for {what}; {why}");
        }
    }

    /// Wait until `ready` holds of the screen: `Ok`, or the screen and the last bytes when
    /// `timeout` passes first or the program has closed the terminal.
    pub fn try_wait_for(
        &self,
        timeout: Duration,
        ready: impl Fn(&vt100::Screen) -> bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut tap = self.shared.lock();
        loop {
            if ready(tap.parser.screen()) {
                return Ok(());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || tap.closed {
                let screen = tap.parser.screen().contents();
                let tail = String::from_utf8_lossy(&tap.raw[tap.raw.len().saturating_sub(300)..])
                    .into_owned();
                return Err(format!(
                    "{}; the screen was:\n{screen}\nthe last bytes: {tail:?}",
                    if tap.closed {
                        "the program had closed the terminal"
                    } else {
                        "the deadline passed"
                    }
                ));
            }
            tap = self
                .shared
                .changed
                .wait_timeout(tap, left)
                .expect("the tap's lock")
                .0;
        }
    }

    /// Wait for `text` to be on the screen.
    pub fn wait_for_text(&self, text: &str) {
        self.wait_for(&format!("{text:?} on the screen"), WAIT, |s| {
            s.contents().contains(text)
        });
    }

    /// Wait for the device screen: the intro is over and the fake device is connected. A key
    /// sent earlier would end the intro's wait instead of reaching the screen.
    pub fn wait_ready(&self) {
        self.wait_for("the connected device screen", WAIT, |s| {
            let text = s.contents();
            text.contains("● connected") && text.contains("Controls")
        });
    }

    /// Type `text` as the terminal sends it: letters as themselves, `\r` for Enter, `\x1b` for Esc.
    pub fn send_keys(&self, text: &str) {
        self.send_bytes(text.as_bytes());
    }

    /// Write `bytes` to the program's input as they are.
    pub fn send_bytes(&self, bytes: &[u8]) {
        let master = self.master.lock().expect("the master's lock");
        (&master.file)
            .write_all(bytes)
            .and_then(|()| (&master.file).flush())
            .expect("write to the pty");
    }

    /// A mouse gesture at 0-based `col`, `row`, in the SGR encoding the TUI turns on.
    pub fn send_mouse(&self, gesture: Mouse, col: u16, row: u16) {
        {
            let tap = self.shared.lock();
            let screen = tap.parser.screen();
            assert_ne!(
                screen.mouse_protocol_mode(),
                vt100::MouseProtocolMode::None,
                "the program has not turned mouse reporting on"
            );
            assert_eq!(
                screen.mouse_protocol_encoding(),
                vt100::MouseProtocolEncoding::Sgr,
                "the program has not asked for SGR mouse reports"
            );
        }
        let at = |button: u16, c: u16, r: u16, press: bool| {
            format!(
                "\x1b[<{button};{};{}{}",
                c + 1,
                r + 1,
                if press { 'M' } else { 'm' }
            )
        };
        let mut out = String::new();
        match gesture {
            Mouse::Click => {
                out += &at(0, col, row, true);
                out += &at(0, col, row, false);
            }
            Mouse::DoubleClick => {
                for _ in 0..2 {
                    out += &at(0, col, row, true);
                    out += &at(0, col, row, false);
                }
            }
            Mouse::Move => out += &at(35, col, row, true),
            Mouse::WheelUp => out += &at(64, col, row, true),
            Mouse::WheelDown => out += &at(65, col, row, true),
            Mouse::DragTo(to_col, to_row) => {
                out += &at(0, col, row, true);
                // A few motion reports on the way, then the one at the end.
                for step in 1..=4u16 {
                    let c = col as i32 + (to_col as i32 - col as i32) * step as i32 / 4;
                    let r = row as i32 + (to_row as i32 - row as i32) * step as i32 / 4;
                    out += &at(32, c as u16, r as u16, true);
                }
                out += &at(0, to_col, to_row, false);
            }
        }
        self.send_bytes(out.as_bytes());
    }

    /// Make the terminal `cols` x `rows`: the program gets SIGWINCH.
    pub fn resize(&self, cols: u16, rows: u16) {
        let master = self.master.lock().expect("the master's lock");
        let mut tap = self.shared.lock();
        tap.parser.screen_mut().set_size(
            rows.try_into().expect("rows are not zero"),
            cols.try_into().expect("columns are not zero"),
        );
        tap.size = (cols, rows);
        let pixels = tap.responder.pixels(cols, rows);
        master.resize(cols, rows, pixels).expect("resize the pty");
    }

    /// The screen as text, one line per row.
    pub fn screen_text(&self) -> String {
        self.shared.lock().parser.screen().contents()
    }

    /// Where `needle` first is on the screen: 0-based column and row of its first character.
    pub fn find(&self, needle: &str) -> (u16, u16) {
        let text = self.screen_text();
        for (row, line) in text.lines().enumerate() {
            if let Some(byte) = line.find(needle) {
                return (line[..byte].chars().count() as u16, row as u16);
            }
        }
        panic!("{needle:?} is not on the screen:\n{text}");
    }

    /// The cell at 0-based `col`, `row`.
    pub fn cell(&self, col: u16, row: u16) -> Cell {
        let tap = self.shared.lock();
        let cell = tap
            .parser
            .screen()
            .cell(row, col)
            .unwrap_or_else(|| panic!("no cell at column {col}, row {row}"));
        Cell {
            text: cell.contents().to_string(),
            fg: cell.fgcolor(),
            bg: cell.bgcolor(),
            bold: cell.bold(),
            dim: cell.dim(),
            italic: cell.italic(),
            underline: cell.underline(),
            inverse: cell.inverse(),
        }
    }

    /// Run `look` on the screen, under the tap's lock.
    pub fn with_screen<T>(&self, look: impl FnOnce(&vt100::Screen) -> T) -> T {
        look(self.shared.lock().parser.screen())
    }

    /// Every byte the program has written so far.
    pub fn raw(&self) -> Vec<u8> {
        self.shared.lock().raw.clone()
    }

    /// How many times `needle` is in the bytes the program has written.
    pub fn count_in_raw(&self, needle: &[u8]) -> usize {
        count_bytes(&self.shared.lock().raw, needle)
    }

    /// How many sixel pictures the program has written (a DCS string whose final byte is `q`).
    pub fn sixel_count(&self) -> usize {
        sixels_in(&self.shared.lock().raw)
    }

    /// Wait until `ready` holds of the tap (the bytes written so far, and the screen they made),
    /// panicking with the screen if `timeout` passes first. `ready` runs under the tap's lock, so
    /// it must not call a method of [`Tui`] that reads the tap.
    pub fn wait_tap(
        &self,
        what: &str,
        timeout: Duration,
        ready: impl Fn(&super::tap::Tap) -> bool,
    ) {
        let deadline = Instant::now() + timeout;
        let mut tap = self.shared.lock();
        loop {
            if ready(&tap) {
                return;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || tap.closed {
                let screen = tap.parser.screen().contents();
                panic!("timed out waiting for {what}; the screen was:\n{screen}");
            }
            tap = self
                .shared
                .changed
                .wait_timeout(tap, left)
                .expect("the tap's lock")
                .0;
        }
    }

    /// The queries the program has written, in order.
    pub fn queries(&self) -> Vec<Query> {
        self.shared.lock().asked.clone()
    }

    /// The answers the terminal wrote back, in order.
    pub fn answers(&self) -> Vec<Vec<u8>> {
        self.shared.lock().answered.clone()
    }

    /// Wait for the program to exit, returning its status.
    pub fn wait_exit(&mut self) -> std::process::ExitStatus {
        self.wait_exit_for(WAIT)
            .expect("svolder tui did not exit in time")
    }

    /// [`Tui::wait_exit`] with a deadline of its own; `None` when it passes.
    pub fn wait_exit_for(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        if self.status.is_none() {
            self.status = self.exit.recv_timeout(timeout).ok();
        }
        self.status
    }

    /// Quit the way a person does (`q`, then Enter on the question) and prove the terminal was
    /// given back, as [`Tui::finish`] does. A `q` that is not read fails the case.
    pub fn quit(self) {
        self.send_keys("q");
        self.wait_for_text("Quit svolder?");
        self.send_keys("\r");
        self.finish();
    }

    /// [`Tui::quit`] for a case that has crossed the too-small size: the TUI sometimes does not
    /// read a `q` sent just after (a known bug of the TUI, the one `quit` must not hide in any
    /// other case), so a `q` that brings no question is sent again; on an open question a
    /// second `q` quits, as Enter does.
    pub fn quit_after_crossing_too_small(mut self) {
        for _ in 0..4 {
            self.send_keys("q");
            let asked = self.try_wait_for(Duration::from_secs(5), |s| {
                s.contents().contains("Quit svolder?")
            });
            if asked.is_ok() {
                self.send_keys("\r");
                return self.finish();
            }
            if self.wait_exit_for(Duration::from_millis(1)).is_some() {
                return self.finish();
            }
        }
        panic!("no `q` brought the question:\n{}", self.screen_text());
    }

    /// Wait for the program to exit, and prove the terminal was given back and nothing was
    /// left behind: it exited 0, the alternate screen is left, mouse reporting is off, the cursor
    /// is shown, the main screen holds none of [`STRAY`] (an answer echoed before raw mode, or a
    /// report after it ended, lands there), the demo's private directory is gone and no process
    /// of the run (its daemon) is left.
    pub fn finish(mut self) {
        let status = self.wait_exit();
        assert_eq!(status.code(), Some(0), "svolder tui exited with {status:?}");
        // The master reads to the end once the program has exited and let go of the slave.
        self.wait_closed();
        let tap = self.shared.lock();
        let screen = tap.parser.screen();
        assert!(
            !screen.alternate_screen(),
            "the alternate screen was not left"
        );
        assert_eq!(
            screen.mouse_protocol_mode(),
            vt100::MouseProtocolMode::None,
            "mouse reporting was left on"
        );
        assert!(!screen.hide_cursor(), "the cursor was left hidden");
        let main = screen.contents();
        for stray in STRAY {
            assert!(
                !main.contains(stray),
                "{stray:?} is on the main screen after the program left:\n{main}"
            );
        }
        drop(tap);
        let left: Vec<PathBuf> = std::fs::read_dir(self.home())
            .expect("read the run's home")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("sd-"))
            })
            .collect();
        assert!(left.is_empty(), "the demo left its directory: {left:?}");
        // The demo stops its daemon before it leaves; give a slow machine a deadline for it.
        let deadline = Instant::now() + WAIT;
        loop {
            let alive = processes_naming(self.home());
            if alive.is_empty() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the run left processes behind: {alive:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Wait until the reader has seen the end of the program's output.
    fn wait_closed(&self) {
        let deadline = Instant::now() + WAIT;
        let mut tap = self.shared.lock();
        while !tap.closed {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(
                !left.is_zero(),
                "the terminal's output never ended after the program exited"
            );
            tap = self
                .shared
                .changed
                .wait_timeout(tap, left)
                .expect("the tap's lock")
                .0;
        }
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        if !self.exited.load(Ordering::SeqCst) {
            // A failed case: stop what this run started, by pid. The TUI leads its own
            // session; the demo's daemon is found by the run's home in its command line.
            stop_run(self.pid, self.home.path());
        }
    }
}

/// The pids of the processes whose command line names `home`: the run's daemon, whose socket
/// and configuration are under it.
fn processes_naming(home: &Path) -> Vec<u32> {
    let home = home.to_string_lossy();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            std::fs::read(format!("/proc/{pid}/cmdline"))
                .is_ok_and(|c| String::from_utf8_lossy(&c).contains(home.as_ref()))
        })
        .collect()
}

/// Kill the TUI `pid` and every process whose command line names `home`.
fn stop_run(pid: u32, home: &Path) {
    for other in processes_naming(home) {
        if other != pid {
            kill(other);
        }
    }
    kill(pid);
}

fn kill(pid: u32) {
    // SAFETY: `kill` takes a pid and a signal and reads no memory of ours; the pid is a process
    // this run started (or one that has just exited, which the call reports as an ignored error).
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
}

/// How many times `needle` is in `raw`.
pub fn count_bytes(raw: &[u8], needle: &[u8]) -> usize {
    raw.windows(needle.len()).filter(|w| *w == needle).count()
}

/// How many sixel pictures `raw` has: DCS strings (`ESC P`, digits and semicolons) ending in `q`.
pub fn sixels_in(raw: &[u8]) -> usize {
    let mut count = 0;
    let mut at = 0;
    while let Some(rel) = raw[at..].windows(2).position(|w| w == b"\x1bP") {
        let start = at + rel + 2;
        let params = raw[start..]
            .iter()
            .take_while(|b| b.is_ascii_digit() || **b == b';')
            .count();
        if raw.get(start + params) == Some(&b'q') {
            count += 1;
        }
        at = start;
    }
    count
}
