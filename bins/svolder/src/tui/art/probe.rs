// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Asking the terminal which graphics it draws, once, as the TUI starts.
//!
//! The query is the one `ratatui-image` makes (kitty graphics, primary device attributes for
//! sixel, the cell size in pixels, and a device status report that every terminal answers), but
//! it is written and read here: the crate's own probe gives up on a silent terminal after its
//! timeout and leaves a thread behind that goes on reading input, which would take the keys the
//! TUI is waiting for. This one waits at most [`WAIT`] on the input with `poll`, owns nothing
//! after it, and a reply that comes later is taken out of the keys by [`super::filter`].
//!
//! The terminal has to be in raw mode, and no event reader may be running yet.

use std::io::Write;
use std::time::Duration;

use ratatui_image::picker::cap_parser::{Parser, QueryStdioOptions, Response};
use ratatui_image::picker::{Picker, ProtocolType};

/// The longest the start-up waits for the terminal's answers, in all. A terminal that answers
/// does so within a few milliseconds; one that does not (some tmux and SSH set-ups) is given
/// half-blocks after this.
pub const WAIT: Duration = Duration::from_millis(100);

/// The input and output of the terminal the probe talks to, which a test replaces.
pub(super) trait Line {
    /// Write `bytes` to the terminal and flush them.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    /// Wait up to `within` for input and read what is there into `buf`: `Ok(0)` when none came.
    fn read(&mut self, buf: &mut [u8], within: Duration) -> std::io::Result<usize>;
    /// Time since the probe began.
    fn elapsed(&mut self) -> Duration;
}

/// What the terminal said it draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Drawn {
    /// The best protocol it answered for: kitty, else sixel, else half-blocks.
    pub protocol: ProtocolType,
    /// The size of a cell in pixels `(width, height)`, when it said, or the window's size says.
    pub cell: Option<(u16, u16)>,
}

/// The query's options: a terminal known to draw kitty or sixel badly is not asked for either.
fn options() -> QueryStdioOptions {
    let set = |name: &str| std::env::var(name).is_ok_and(|value| !value.is_empty());
    let mut options = QueryStdioOptions::default();
    if set("WEZTERM_EXECUTABLE") || set("KONSOLE_VERSION") {
        options.blacklist_protocols = vec![ProtocolType::Kitty, ProtocolType::Sixel];
    }
    options
}

/// Write `query` and read answers from `line` until the status report (the last answer a
/// terminal gives) or `wait` has passed.
pub(super) fn ask(line: &mut impl Line, query: &str, wait: Duration) -> Vec<Response> {
    if line.write(query.as_bytes()).is_err() {
        return Vec::new();
    }
    let began = line.elapsed();
    let mut parser = Parser::new();
    let mut answers = Vec::new();
    let mut buf = [0u8; 64];
    loop {
        let left = wait.saturating_sub(line.elapsed().saturating_sub(began));
        if left.is_zero() {
            return answers;
        }
        let Ok(read) = line.read(&mut buf, left) else {
            return answers;
        };
        for byte in &buf[..read] {
            let found = parser.push(char::from(*byte));
            if found == [Response::Status] {
                return answers;
            }
            answers.extend(found);
        }
    }
}

/// What `answers` say the terminal draws, with `window` as the cell size when the terminal gave
/// none (the window's size in pixels over its size in cells, from the tty).
///
/// Anything but half-blocks needs a cell size to map pixels to cells, so a terminal that gave
/// none is left with half-blocks whatever else it said.
pub(super) fn interpret(answers: &[Response], window: Option<(u16, u16)>) -> Drawn {
    let mut protocol = ProtocolType::Halfblocks;
    let mut cell = None;
    for answer in answers {
        match answer {
            Response::Kitty => protocol = ProtocolType::Kitty,
            Response::Sixel if protocol != ProtocolType::Kitty => protocol = ProtocolType::Sixel,
            Response::CellSize(Some(size)) => cell = Some(*size),
            _ => {}
        }
    }
    let cell = cell.or(window).filter(|(w, h)| *w > 0 && *h > 0);
    if cell.is_none() {
        protocol = ProtocolType::Halfblocks;
    }
    Drawn { protocol, cell }
}

/// The picker for what the terminal drew: its protocol and cell size, or half-blocks.
#[expect(
    deprecated,
    reason = "the crate's own probe cannot be bounded, and this is the one constructor that takes \
              a cell size"
)]
pub(super) fn picker_of(drawn: Drawn) -> Picker {
    match drawn.cell {
        Some(cell) => {
            let mut picker = Picker::from_fontsize(cell.into());
            picker.set_protocol_type(drawn.protocol);
            picker
        }
        None => Picker::halfblocks(),
    }
}

/// Ask the terminal behind `line` what it draws, giving up after `wait`.
pub(super) fn probe_on(line: &mut impl Line, window: Option<(u16, u16)>, wait: Duration) -> Picker {
    // The tmux a terminal runs inside is learnt from the environment, and its queries are
    // wrapped for it to pass them on.
    let tmux = Picker::halfblocks().tmux_detected();
    let query = Parser::query(tmux, options());
    let answers = ask(line, &query, wait);
    picker_of(interpret(&answers, window))
}

/// Ask the process's own terminal what it draws: its input and output are the ones the probe
/// reads and writes, and the input is read non-blocking for as long as the probe lasts.
pub fn probe() -> Picker {
    let mut line = Tty::new();
    probe_on(&mut line, window_cell(), WAIT)
}

/// The cell size the tty's window size gives, when it holds pixel sizes.
fn window_cell() -> Option<(u16, u16)> {
    let mut size = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCGWINSZ writes one `winsize` through the pointer given, which is valid.
    let got = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &raw mut size) };
    (got == 0 && size.ws_col > 0 && size.ws_row > 0)
        .then(|| (size.ws_xpixel / size.ws_col, size.ws_ypixel / size.ws_row))
}

/// The process's stdin and stdout, stdin non-blocking until this is dropped.
struct Tty {
    began: std::time::Instant,
    /// The descriptor's status flags before this made it non-blocking, when it did.
    flags: Option<libc::c_int>,
}

impl Tty {
    fn new() -> Tty {
        // SAFETY: fcntl on a descriptor number with a command and flags.
        let flags = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
        let flags = (flags >= 0).then_some(flags).filter(|&was| unsafe {
            libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, was | libc::O_NONBLOCK) == 0
        });
        Tty {
            began: std::time::Instant::now(),
            flags,
        }
    }
}

impl Drop for Tty {
    fn drop(&mut self) {
        if let Some(was) = self.flags {
            // SAFETY: restores the flags read in `new`.
            let _ = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, was) };
        }
    }
}

impl Line for Tty {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let mut out = std::io::stdout().lock();
        out.write_all(bytes)?;
        out.flush()
    }

    fn read(&mut self, buf: &mut [u8], within: Duration) -> std::io::Result<usize> {
        let mut watched = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = i32::try_from(within.as_millis().max(1)).unwrap_or(i32::MAX);
        // SAFETY: one valid pollfd for the one descriptor counted.
        let ready = unsafe { libc::poll(&raw mut watched, 1, millis) };
        if ready <= 0 {
            return Ok(0);
        }
        if watched.revents & libc::POLLIN == 0 {
            // Hung up or failed with nothing left to read.
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        // SAFETY: the buffer is valid for the length given.
        let read = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr().cast(), buf.len()) };
        match usize::try_from(read) {
            Ok(0) => Err(std::io::ErrorKind::UnexpectedEof.into()),
            Ok(read) => Ok(read),
            Err(_) => match std::io::Error::last_os_error() {
                e if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
                e => Err(e),
            },
        }
    }

    fn elapsed(&mut self) -> Duration {
        self.began.elapsed()
    }
}

#[cfg(test)]
mod tests;
