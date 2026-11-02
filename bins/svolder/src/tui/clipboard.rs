// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Copying text out of the TUI by every way that can reach the user's clipboard.
//!
//! Two ways are tried together. The terminal is sent an OSC 52 sequence, wrapped for tmux and
//! GNU screen so that it reaches the terminal outside them; a terminal cannot say whether it
//! took it, and some (VTE's) never do. And where the TUI has a display, the text is piped to
//! the display's clipboard tool: `wl-copy` under Wayland, `xclip` or `xsel` under X (which also
//! covers `ssh -X`). Only the tool can say how it came out, so [`note`] says "copied" for a tool
//! that ran and never for the sequence alone.
//!
//! The environment, the lookup of a tool and the running of one are arguments, so a test drives
//! them without touching the process environment or starting a real tool.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// How the environment is read: the value of a variable, if set.
pub type Var<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// A clipboard tool and the arguments that make it read the text on its standard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    /// The program, found on `PATH`.
    pub program: &'static str,
    /// Its arguments.
    pub args: &'static [&'static str],
}

/// How a try with a clipboard tool came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tried {
    /// The named tool took the text and exited well.
    Copied(&'static str),
    /// The named tool was still running after the wait: it may be holding the selection, as
    /// `xclip` does, or be stuck; it has the text but has not said so.
    Handed(&'static str),
    /// There is no display, so no tool was tried.
    NoDisplay,
    /// A tool could not be found or run, with the reason in words.
    Failed(String),
}

/// How a tool that started came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ran {
    /// It exited with success.
    Exited,
    /// It was still running when the wait ended.
    Running,
}

/// How long a tool is given to fail before it is taken to be holding the selection.
const TOOL_MOMENT: Duration = Duration::from_secs(2);

/// `sequence` made to reach the outer terminal through a terminal multiplexer. Under tmux
/// (`TMUX` set) both forms are sent: the plain sequence, which tmux acts on with `set-clipboard
/// on`, then tmux's DCS passthrough with every escape doubled, which tmux 3.3 and later let
/// through only with `allow-passthrough on`; the outer terminal that gets both holds the same
/// text. Under GNU screen (`STY` set) the DCS alone, as screen has no OSC 52 handling of its
/// own and passes a DCS on. Unchanged with neither.
pub fn wrap(sequence: &str, var: Var<'_>) -> String {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    if set("TMUX") {
        format!(
            "{sequence}\x1bPtmux;{}\x1b\\",
            sequence.replace('\x1b', "\x1b\x1b")
        )
    } else if set("STY") {
        format!("\x1bP{sequence}\x1b\\")
    } else {
        sequence.to_string()
    }
}

/// The tools to try, in order, for the display `var` names: `wl-copy` under Wayland, then
/// `xclip` and `xsel` under X; none without a display.
pub fn candidates(var: Var<'_>) -> Vec<Tool> {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    let mut tools = Vec::new();
    if set("WAYLAND_DISPLAY") {
        tools.push(Tool {
            program: "wl-copy",
            args: &[],
        });
    }
    if set("DISPLAY") {
        tools.push(Tool {
            program: "xclip",
            args: &["-selection", "clipboard"],
        });
        tools.push(Tool {
            program: "xsel",
            args: &["--clipboard", "--input"],
        });
    }
    tools
}

/// Pipe `text` to the first of the display's tools that `found` says is installed, through
/// `run`; with no display nothing is tried.
pub fn copy_with_tool(
    var: Var<'_>,
    found: &dyn Fn(&str) -> bool,
    run: &dyn Fn(&Tool, &str) -> Result<Ran, String>,
    text: &str,
) -> Tried {
    let tools = candidates(var);
    if tools.is_empty() {
        return Tried::NoDisplay;
    }
    let Some(tool) = tools.iter().find(|tool| found(tool.program)) else {
        let names: Vec<_> = tools.iter().map(|tool| tool.program).collect();
        return Tried::Failed(format!(
            "no clipboard tool is installed ({})",
            names.join(", ")
        ));
    };
    match run(tool, text) {
        Ok(Ran::Exited) => Tried::Copied(tool.program),
        Ok(Ran::Running) => Tried::Handed(tool.program),
        Err(why) => Tried::Failed(format!("{} did not run: {why}", tool.program)),
    }
}

/// What the box says of a copy of `what` (`link`, `code` or `token`) that sent the terminal an
/// OSC 52 sequence and then tried a tool: `terminal` is why the sequence could not be written, if
/// it could not, and `hand` is what the user can do where the terminal ignores the sequence.
pub fn note(what: &str, tried: &Tried, terminal: Option<&str>, hand: &str) -> String {
    let sent = match terminal {
        Some(why) => format!("{what} could not be sent to the terminal ({why})"),
        None => format!("{what} sent to the terminal's clipboard (OSC 52)"),
    };
    match tried {
        Tried::Copied(tool) => format!("{what} copied with {tool}"),
        Tried::Handed(tool) => format!("{what} handed to {tool}, which has not said it took it"),
        Tried::NoDisplay => format!("{sent} — if your terminal ignores it, {hand}"),
        Tried::Failed(why) => format!("{sent}; {why}"),
    }
}

/// Whether `program` is an executable file in a directory of `PATH`, as `var` reads it.
pub fn on_path(var: Var<'_>, program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Some(path) = var("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        std::fs::metadata(dir.join(program))
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    })
}

/// Run `tool` with `text` on its standard input, its other streams nowhere and in a process
/// group of its own (so a Ctrl-C here does not end it, and it cannot draw on the terminal),
/// waiting up to [`TOOL_MOMENT`] for it. A tool that has failed by then is reported; one still
/// running is reported as [`Ran::Running`] (as `xclip` holding the selection would be) and is
/// left, a thread waiting for it so that it does not stay a defunct process.
pub fn run_tool(tool: &Tool, text: &str) -> Result<Ran, String> {
    run_program(Path::new(tool.program), tool.args, text, TOOL_MOMENT)
}

/// [`run_tool`] for any program, waiting `moment`.
fn run_program(program: &Path, args: &[&str], text: &str, moment: Duration) -> Result<Ran, String> {
    use std::os::unix::process::CommandExt;
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("could not be run: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        // A tool that has quit before reading is reported by its status below.
        let _ = stdin.write_all(text.as_bytes());
    }
    let until = Instant::now() + moment;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(Ran::Exited),
            Ok(Some(status)) => return Err(format!("exited with {status}")),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(Ran::Running);
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests;
