// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The process manager's plumbing: recognising the chain's `pipewire` among the processes of
//! this user, starting it detached, and stopping it.
//!
//! The chain is recognised by its command line alone: exactly `pipewire -c <svolder's own
//! config path>`, as the kernel shows it in `/proc/<pid>/cmdline`. A pid file is a shortcut to
//! the process, never proof of it, and after a login or a reboot there is no pid file at all:
//! the autostart entry starts the chain on its own. A pid is signalled only through a pidfd
//! opened before its command line is checked, so a pid another program has been given is never
//! touched.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The command line of the chain: the program, `-c`, and the config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Argv {
    /// The program and its arguments, as `exec` is given them.
    args: [String; 3],
    /// What the kernel puts in front of them when the program is a script: its interpreter
    /// and the one argument its `#!` line may carry. Empty for a program that is a binary.
    interpreter: Vec<String>,
}

impl Argv {
    /// The command line of `program -c conf`.
    pub(super) fn new(program: &str, conf: &str) -> Argv {
        Argv {
            args: [program.to_string(), "-c".to_string(), conf.to_string()],
            interpreter: interpreter(Path::new(program)),
        }
    }

    /// The program.
    pub(super) fn program(&self) -> &str {
        &self.args[0]
    }

    /// The config.
    pub(super) fn conf(&self) -> &str {
        &self.args[2]
    }

    /// Whether process `pid` runs exactly this command line.
    pub(super) fn runs(&self, pid: u32) -> bool {
        let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            return false;
        };
        let mut seen: Vec<&[u8]> = raw.split(|byte| *byte == 0).collect();
        // The command line ends with a NUL, which leaves one empty piece at the end.
        if seen.last().is_some_and(|last| last.is_empty()) {
            seen.pop();
        }
        let wanted = self.interpreter.iter().chain(self.args.iter());
        seen.len() == self.interpreter.len() + self.args.len()
            && seen
                .iter()
                .zip(wanted)
                .all(|(got, want)| *got == want.as_bytes())
    }

    /// The pids of this user's processes that run this command line.
    pub(super) fn find(&self) -> Vec<u32> {
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::geteuid() };
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
                (entry.metadata().ok()?.uid() == uid && self.runs(pid)).then_some(pid)
            })
            .collect()
    }
}

/// The interpreter a script's `#!` line names, and its argument if it has one; nothing for a
/// file that is not a script.
fn interpreter(program: &Path) -> Vec<String> {
    use std::io::Read;
    let mut head = [0u8; 256];
    let read = std::fs::File::open(program)
        .and_then(|mut file| file.read(&mut head))
        .unwrap_or(0);
    let Some(line) = head[..read].strip_prefix(b"#!") else {
        return Vec::new();
    };
    let line = String::from_utf8_lossy(line.split(|b| *b == b'\n').next().unwrap_or_default());
    let line = line.trim();
    match line.split_once(char::is_whitespace) {
        Some((interpreter, argument)) => vec![interpreter.to_string(), argument.trim().to_string()],
        None => vec![line.to_string()],
    }
}

/// The pid and the command line a pid file records: three lines, the pid, the program and the
/// config.
pub(super) fn parse_pid_file(text: &str) -> Option<(u32, Argv)> {
    let mut lines = text.lines();
    let pid = lines.next()?.trim().parse().ok().filter(|pid| *pid > 1)?;
    let program = lines.next()?;
    let conf = lines.next()?;
    Some((pid, Argv::new(program, conf)))
}

/// The program an autostart entry's `Exec=` line starts: its first word, with both layers of
/// escaping the writer applied undone, the Desktop Entry string rule first (`\\` is `\`),
/// then the Exec quoting rule (a backslash takes the next character inside quotes) and `%%`.
pub(super) fn desktop_program(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value = text.lines().find_map(|line| line.strip_prefix("Exec="))?;
    let value = unescape_string(value);
    let Some(quoted) = value.strip_prefix('"') else {
        let word = value.split_whitespace().next()?;
        return Some(word.replace("%%", "%"));
    };
    let mut word = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(word.replace("%%", "%")),
            '\\' => word.extend(chars.next()),
            c => word.push(c),
        }
    }
    None
}

/// A Desktop Entry string value with its escapes read: `\s` a space, `\n`, `\t`, `\r` and
/// `\\` themselves; a backslash before anything else stays.
fn unescape_string(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            _ => {
                out.push('\\');
                continue;
            }
        }
        chars.next();
    }
    out
}

/// Wait until `pid` no longer runs `argv`, for at most `within`.
pub(super) async fn gone(pid: u32, argv: &Argv, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while argv.runs(pid) {
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    true
}

/// A process held by a pidfd, so a signal sent through it reaches that process and no other,
/// whatever its pid is given to later.
#[derive(Debug)]
pub(super) struct Held {
    pid: u32,
    /// The pidfd. `None` when `pidfd_open` was refused for a reason that says nothing about the
    /// process (a kernel before Linux 5.3, a seccomp filter, no descriptors left), and for this
    /// process's own child whose pidfd could not be opened; the pid itself is signalled then.
    fd: Option<OwnedFd>,
}

impl Held {
    /// Hold `pid` when it runs `argv`. The pidfd is opened before the command line is read, so
    /// the process the check reads is the process held: one that exits in between leaves the
    /// hold on a process that is gone, and a signal through it reaches nothing.
    pub(super) fn open(pid: u32, argv: &Argv) -> Option<Held> {
        let held = Held::of(pid)?;
        argv.runs(pid).then_some(held)
    }

    /// Hold `pid` with no check: a pid that is gone or is no process's gives nothing.
    fn of(pid: u32) -> Option<Held> {
        // SAFETY: `pidfd_open` takes a pid and flags and touches no memory of this process.
        let fd =
            unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0 as libc::c_uint) };
        if fd >= 0 {
            // SAFETY: the kernel has just returned this descriptor, and nothing else owns it.
            let fd = unsafe { OwnedFd::from_raw_fd(fd as RawFd) };
            return Some(Held { pid, fd: Some(fd) });
        }
        Held::after_refusal(pid, std::io::Error::last_os_error().raw_os_error())
    }

    /// What a refused `pidfd_open` gives: nothing when `errno` says there is no such process
    /// (ESRCH) or no process's pid (EINVAL), else a hold on the pid itself, so a refusal that
    /// says nothing about the process never makes a running chain look gone.
    pub(super) fn after_refusal(pid: u32, errno: Option<i32>) -> Option<Held> {
        match errno {
            Some(libc::ESRCH | libc::EINVAL) => None,
            _ => Some(Held { pid, fd: None }),
        }
    }

    /// Whether signals go through a pidfd.
    #[cfg(test)]
    pub(super) fn by_pidfd(&self) -> bool {
        self.fd.is_some()
    }

    /// The pid of the process held.
    pub(super) fn pid(&self) -> u32 {
        self.pid
    }

    /// Send `signal` to the process held; false when the signal was not sent, as when the
    /// process has gone.
    pub(super) fn signal(&self, signal: libc::c_int) -> bool {
        let sent = match &self.fd {
            // SAFETY: the descriptor is this hold's and open; a null `siginfo` sends the signal
            // as `kill` would, and flags must be 0.
            Some(fd) => unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    signal,
                    std::ptr::null::<libc::siginfo_t>(),
                    0 as libc::c_uint,
                )
            },
            // SAFETY: `kill` takes a pid and a signal number and touches no memory of this
            // process; the pid is this process's own child, or its command line was checked
            // when it was held.
            None => libc::c_long::from(unsafe { libc::kill(self.pid as libc::pid_t, signal) }),
        };
        sent == 0
    }
}

/// Start `program -c conf` in a session of its own, in `/`, with no standard streams, and keep
/// reaping it on a thread so it never lingers as a zombie while this process lives. Returns
/// the child, held.
pub(super) fn spawn_detached(program: &Path, conf: &Path) -> std::io::Result<Held> {
    let mut command = Command::new(program);
    command
        .arg("-c")
        .arg(conf)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: `setsid` is async-signal-safe and the closure does nothing else between fork
    // and exec.
    unsafe {
        command.pre_exec(|| match libc::setsid() {
            -1 => Err(std::io::Error::last_os_error()),
            _ => Ok(()),
        });
    }
    let mut child = command.spawn()?;
    // Held before the reaper starts: an unreaped child's pid cannot be reused.
    let held = Held::of(child.id()).unwrap_or(Held {
        pid: child.id(),
        fd: None,
    });
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(held)
}
