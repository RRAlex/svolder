// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon logs`: what the daemon has said, read from wherever it writes.
//!
//! A running daemon says where that is (a file, or the journal); one that says nothing is read
//! from the journal where systemd manages it and from the file a daemon nobody installed writes
//! otherwise. The reader runs on this process's own streams, so a follow ends when the user ends
//! it, and on a terminal each line is coloured on its way through.

use std::ffi::{OsStr, OsString};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use svolder_ipc::socket::daemon_version::{self, Skew};
use svolder_ipc::{Client, socket};

use crate::backend::{ASK_WAIT, Globals};
use crate::recovery::Failure;

use super::status::version;
use super::{Ctl, OWN_VERSION, resolved_socket, systemd_manages, trouble};

/// How many lines of the daemon's own log `logs` prints: a few starts' worth.
const LOG_LINES: &str = "200";

/// What a daemon's `version` reply names its log when stderr is the journal.
pub(super) const JOURNAL: &str = "journal";

impl Ctl {
    /// Run a program and wait for it, however long it takes.
    ///
    /// This is what a log reader is: `journalctl -f` and `tail -f` end when the user ends them,
    /// so nothing here puts a deadline on one. With `colored`, its output is read a line at a
    /// time, coloured and written there as it arrives; without, it runs on this process's own
    /// streams.
    fn stream(
        &self,
        program: &str,
        args: &[&OsStr],
        colored: Option<&mut dyn Write>,
    ) -> Result<(), String> {
        let mut command = self.command(OsStr::new(program));
        command.args(args);
        let status = match colored {
            None => command.status().map_err(|e| format!("{program}: {e}"))?,
            Some(out) => {
                let mut child = command
                    .stdout(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("{program}: {e}"))?;
                let mut reader =
                    std::io::BufReader::new(child.stdout.take().expect("the stdout is piped"));
                let mut raw = Vec::new();
                loop {
                    raw.clear();
                    match reader.read_until(b'\n', &mut raw) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    let text = String::from_utf8_lossy(&raw);
                    let line = colorize(text.trim_end_matches(['\n', '\r']));
                    if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                        // The reader of our output is gone, and so is the reason to follow.
                        let _ = child.kill();
                        break;
                    }
                }
                child.wait().map_err(|e| format!("{program}: {e}"))?
            }
        };
        match status.success() || status.code().is_none() {
            // A follow ended by Ctrl-C carries the signal rather than a code, and that is how
            // a reader is meant to end: it is not a failure of the verb.
            true => Ok(()),
            false => Err(format!("{program}: {status}")),
        }
    }
}

/// What the daemon has said: where a running daemon says it writes, else the journal where
/// systemd keeps it, else the log file a daemon nobody installed writes.
///
/// One line on stderr names the source first. On a terminal the stamps are dimmed and the
/// warnings and errors coloured. The reader runs on this process's own streams and this call
/// ends when it does, so a follow ends when the user ends it.
pub async fn logs(g: &Globals<'_>, follow: bool) -> Result<(), Failure> {
    // A configuration that does not parse is no reason to read nothing: the log is what says
    // why, so the socket is the one `--socket` or the default names.
    let socket = resolved_socket(g).unwrap_or_else(|_| socket::path::resolve(g.socket, None));
    let reported = reported_log(&socket).await;
    let color = wants_color(
        std::io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").as_deref(),
    );
    logs_from(
        &Ctl::live(),
        &crate::state::log_path(),
        &reported,
        follow,
        color
            .then(std::io::stdout)
            .as_mut()
            .map(|o| o as &mut dyn Write),
    )
}

/// What the daemon on `socket` says of where it logs, given [`ASK_WAIT`] to answer: a daemon
/// that takes the connection and never answers is read as one that said nothing.
async fn reported_log(socket: &Path) -> LogSaid {
    let ask = async {
        let mut client = Client::connect(socket).await.ok()?;
        let (version, _, _, _, log) = version(&mut client).await;
        Some(LogSaid::of(&version, log))
    };
    tokio::time::timeout(ASK_WAIT, ask)
        .await
        .ok()
        .flatten()
        .unwrap_or(LogSaid::Unsaid)
}

/// What a daemon said of where it logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LogSaid {
    /// Nothing: no daemon answered, or one older than this svolder, which has no such word.
    Unsaid,
    /// The file it writes, or [`JOURNAL`].
    Path(String),
    /// A daemon of this version or newer that named nothing: its stderr is a terminal or a
    /// pipe, so it writes no file.
    NoFile,
}

impl LogSaid {
    /// What a daemon of `version` that answered `log` has said.
    pub(super) fn of(version: &str, log: Option<String>) -> LogSaid {
        match log {
            Some(path) => LogSaid::Path(path),
            None if daemon_version::is_version(version)
                && daemon_version::skew(version, OWN_VERSION) != Some(Skew::Older) =>
            {
                LogSaid::NoFile
            }
            None => LogSaid::Unsaid,
        }
    }
}

/// What `status` prints after `log:` for a daemon that writes no file.
pub(super) const NOT_A_FILE: &str = "not a file (stderr is a terminal or a pipe)";

/// Whether `daemon logs` colours: only on a terminal, and never with `NO_COLOR` set.
fn wants_color(stdout_is_terminal: bool, no_color: Option<&OsStr>) -> bool {
    stdout_is_terminal && no_color.is_none()
}

/// The program that reads a log, its arguments, and the line that names what it reads.
struct ReadPlan {
    /// What goes on stderr before the banner, when the daemon's own word calls for it.
    note: Option<String>,
    /// What goes on stderr before the first log line.
    banner: String,
    /// The reader.
    program: &'static str,
    /// The reader's arguments.
    args: Vec<OsString>,
}

/// What reads the log a running daemon `reported`, else the journal when systemd manages the
/// daemon, else the file `log`. A daemon that said it writes no file earns a note ahead of the
/// fallback's banner. An error is the line saying there is nothing to read yet.
fn read_plan(ctl: &Ctl, log: &Path, reported: &LogSaid, follow: bool) -> Result<ReadPlan, String> {
    let fallback = || match systemd_manages(&ctl.units()) {
        true => LogSource::Journal,
        false => LogSource::File(log.to_path_buf()),
    };
    let source = match reported {
        LogSaid::Path(path) if path == JOURNAL => LogSource::Journal,
        LogSaid::Path(path) => LogSource::File(PathBuf::from(path)),
        LogSaid::Unsaid | LogSaid::NoFile => fallback(),
    };
    let note = (*reported == LogSaid::NoFile).then(|| {
        let reading = match &source {
            LogSource::Journal => "the journal".to_string(),
            LogSource::File(path) => path.display().to_string(),
        };
        format!(
            "svolder: the daemon does not log to a file (stderr is a terminal or a pipe); \
             reading {reading} instead"
        )
    });
    let follow_flag = follow.then(|| OsString::from("-f"));
    match source {
        LogSource::Journal => Ok(ReadPlan {
            note,
            banner: "svolder: reading the journal".to_string(),
            program: "journalctl",
            args: ["--user", "-u", "svolderd", "--no-pager", "-o", "short-iso"]
                .map(OsString::from)
                .into_iter()
                .chain(follow_flag)
                .collect(),
        }),
        LogSource::File(path) => {
            let Ok(written) = std::fs::metadata(&path).and_then(|m| m.modified()) else {
                let nothing = format!(
                    "svolder: no daemon has logged anything to {} yet",
                    path.display()
                );
                return Err(match note {
                    Some(note) => format!("{note}\n{nothing}"),
                    None => nothing,
                });
            };
            let stamp = svolder_ipc::clock::local_stamp(written);
            Ok(ReadPlan {
                note,
                banner: format!(
                    "svolder: reading {} (last written {})",
                    path.display(),
                    &stamp[..16]
                ),
                program: "tail",
                args: [OsString::from("-n"), OsString::from(LOG_LINES)]
                    .into_iter()
                    .chain(follow_flag)
                    .chain([path.into_os_string()])
                    .collect(),
            })
        }
    }
}

/// [`logs`] over given programs, a given fallback log file and the log a daemon reported.
///
/// `colored` is where the coloured lines go, or `None` for the plain lines on this process's own
/// stdout.
fn logs_from(
    ctl: &Ctl,
    log: &Path,
    reported: &LogSaid,
    follow: bool,
    colored: Option<&mut dyn Write>,
) -> Result<(), Failure> {
    let plan = match read_plan(ctl, log, reported, follow) {
        Ok(plan) => plan,
        Err(nothing) => {
            eprintln!("{nothing}");
            return Ok(());
        }
    };
    if let Some(note) = &plan.note {
        eprintln!("{note}");
    }
    eprintln!("{}", plan.banner);
    let args: Vec<&OsStr> = plan.args.iter().map(OsString::as_os_str).collect();
    ctl.stream(plan.program, &args, colored)
        .map_err(|why| trouble(why, "Run `svolder daemon status`"))
}

/// Where `daemon logs` reads from.
enum LogSource {
    /// The user journal, through `journalctl`.
    Journal,
    /// A log file, through `tail`.
    File(PathBuf),
}

/// The dim ANSI code, for a line's stamp.
const DIM: &str = "\x1b[2m";
/// The yellow ANSI code, for a warning line.
const YELLOW: &str = "\x1b[33m";
/// The red ANSI code, for an error line.
const RED: &str = "\x1b[31m";
/// The ANSI code that ends a coloured span.
const RESET: &str = "\x1b[0m";

/// `line` with its leading stamp dimmed and the rest coloured by the level the daemon gave it.
///
/// A stamp is the leading `YYYY-MM-DD` word and, when the next word is a `HH:MM:SS` time, that
/// too. A level is the `warning:` or `error:` right after `svolderd:` or `svolderd[pid]:`.
fn colorize(line: &str) -> String {
    let (stamp, rest) = split_stamp(line);
    let level = match level_code(rest) {
        Some(code) => format!("{code}{rest}{RESET}"),
        None => rest.to_string(),
    };
    match stamp.is_empty() {
        true => level,
        false => format!("{DIM}{stamp}{RESET}{level}"),
    }
}

/// The stamp a line starts with, and the rest after it (its separating space included).
fn split_stamp(line: &str) -> (&str, &str) {
    let bytes = line.as_bytes();
    let dated = bytes.len() >= 10
        && bytes[..10].iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        });
    if !dated {
        return ("", line);
    }
    let mut end = line[10..].find(' ').map_or(line.len(), |at| 10 + at);
    // `2026-10-04 18:04:26`: the time is a word of its own, `T`-joined in `short-iso`.
    let time = line.get(end + 1..end + 9).filter(|t| {
        t.len() == 8
            && t.bytes().enumerate().all(|(i, b)| match i {
                2 | 5 => b == b':',
                _ => b.is_ascii_digit(),
            })
    });
    if time.is_some() && bytes.get(end + 9).is_none_or(|b| *b == b' ') {
        end += 9;
    }
    line.split_at(end)
}

/// The colour for the level a daemon line names, when it names one.
///
/// The level follows the daemon's name (`svolderd:` in the file, `svolderd[pid]:` from the
/// journal) or the transport's (`svolder-ble:`), and a line from before the journal dropped the
/// daemon's own prefix may repeat the name once more.
fn level_code(text: &str) -> Option<&'static str> {
    let (at, len) = ["svolderd", "svolder-ble"]
        .into_iter()
        .filter_map(|name| Some((text.find(name)?, name.len())))
        .min()?;
    let after = &text[at + len..];
    let after = match after.strip_prefix('[') {
        Some(pid) => &pid[pid.find(']')? + 1..],
        None => after,
    };
    let mut message = after.strip_prefix(": ")?;
    while let Some(rest) = message
        .strip_prefix("svolderd: ")
        .or_else(|| message.strip_prefix("svolder-ble: "))
    {
        message = rest;
    }
    match () {
        _ if message.starts_with("warning: ") => Some(YELLOW),
        _ if message.starts_with("error: ") => Some(RED),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
