// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Raw-mode terminal setup and teardown, undone on every exit path including a panic.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use svolder_ipc::Config;

use super::decoration::title::{self, TitleSink};
use crate::backend::Failure;
use crate::recovery::Kind;

/// Turns mouse capture on and off.
///
/// The event loop names what it wants through this rather than through the terminal it holds,
/// so a loop under test can be given something that records the calls instead of writing
/// escape sequences to a real terminal.
pub trait MouseCapture {
    /// Turn mouse capture on or off.
    fn set_mouse(&mut self, on: bool) -> std::io::Result<()>;
}

/// Discards the input the terminal has sent but nobody has read.
///
/// A mouse report that was in flight when capture went off would otherwise be read by the shell
/// as typed text.
pub trait InputFlush {
    /// Drop the pending input, and what arrives while the terminal catches up, best-effort.
    fn flush_input(&mut self);
}

/// How long stdin has to be quiet before the drain is done.
const QUIET: Duration = Duration::from_millis(30);

/// The longest the drain runs in all, whatever keeps arriving.
const DRAIN_CAP: Duration = Duration::from_millis(150);

/// The input queue [`drain_then_flush`] works on, which a test replaces with a clock and a
/// script of its own.
trait InputQueue {
    /// Time since some fixed start.
    fn now(&mut self) -> Duration;
    /// Wait up to `within` for input to read, and say whether there is some.
    fn readable(&mut self, within: Duration) -> bool;
    /// Read what is there and drop it, without ever blocking. `false` once stdin is closed or
    /// failed; `true` otherwise, including a read that found nothing to take (would-block), so
    /// the drain polls again until its quiet window or its cap.
    fn discard(&mut self) -> bool;
    /// Drop whatever is still queued.
    fn flush(&mut self);
}

/// Read and drop stdin until it has been quiet for [`QUIET`], for at most [`DRAIN_CAP`] in
/// all, then flush what is left.
///
/// Turning mouse capture off is read by the terminal emulator some milliseconds after it is
/// written, and until then the emulator goes on sending reports. They arrive while raw mode
/// is still on, where they are read and dropped here, rather than after it is off, where the
/// line discipline would echo them to the screen as `^[[<35;81;59M`.
fn drain_then_flush(queue: &mut dyn InputQueue) {
    let started = queue.now();
    loop {
        let spent = queue.now().saturating_sub(started);
        if spent >= DRAIN_CAP {
            break;
        }
        if !queue.readable(QUIET.min(DRAIN_CAP - spent)) || !queue.discard() {
            break;
        }
    }
    queue.flush();
}

/// The process's stdin, drained and flushed with `poll`, `read` and `tcflush`.
pub struct StdinFlush;

impl InputFlush for StdinFlush {
    /// Drain and flush stdin when it is a terminal; any failure along the way is ignored.
    fn flush_input(&mut self) {
        // SAFETY: isatty only takes a descriptor number.
        if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
            return;
        }
        drain_then_flush(&mut Stdin::new());
    }
}

/// Stdin as an [`InputQueue`], timed from the instant it was made, and non-blocking until it is
/// dropped: crossterm's input thread may have taken the last report already, and a blocking
/// read would then wait for a key.
struct Stdin {
    began: Instant,
    /// The descriptor's status flags before this made it non-blocking, when it did.
    flags: Option<libc::c_int>,
}

impl Stdin {
    fn new() -> Stdin {
        // SAFETY: fcntl on a descriptor number with a command and flags.
        let flags = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
        let flags = (flags >= 0).then_some(flags).filter(|&was| unsafe {
            libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, was | libc::O_NONBLOCK) == 0
        });
        Stdin {
            began: Instant::now(),
            flags,
        }
    }
}

impl Drop for Stdin {
    fn drop(&mut self) {
        if let Some(was) = self.flags {
            // SAFETY: restores the flags read in `new`.
            let _ = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, was) };
        }
    }
}

impl InputQueue for Stdin {
    fn now(&mut self) -> Duration {
        self.began.elapsed()
    }

    fn readable(&mut self, within: Duration) -> bool {
        let mut watched = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = i32::try_from(within.as_millis()).unwrap_or(i32::MAX);
        // SAFETY: one valid pollfd for the one descriptor counted.
        let ready = unsafe { libc::poll(&raw mut watched, 1, millis) };
        ready > 0 && watched.revents & libc::POLLIN != 0
    }

    fn discard(&mut self) -> bool {
        let mut scratch = [0u8; 256];
        // SAFETY: the buffer is valid for the length given.
        let read = unsafe {
            libc::read(
                libc::STDIN_FILENO,
                scratch.as_mut_ptr().cast(),
                scratch.len(),
            )
        };
        read > 0
            || (read < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock)
    }

    fn flush(&mut self) {
        // SAFETY: tcflush only takes a descriptor number and a queue selector.
        let _ = unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH) };
    }
}

/// Turn mouse capture off and then discard the input already queued and still arriving, in that
/// order, so a report sent before the terminal handled the end of capture never reaches the
/// shell. This runs with raw mode still on.
fn mouse_off_then_flush(mouse: &mut dyn MouseCapture, flush: &mut dyn InputFlush) {
    let _ = mouse.set_mouse(false);
    flush.flush_input();
}

/// The terminal's mouse capture, and which way it is set now.
#[derive(Default)]
pub struct Mouse {
    on: bool,
}

impl MouseCapture for Mouse {
    /// Turn mouse capture on or off, a no-op when it is already there.
    fn set_mouse(&mut self, on: bool) -> std::io::Result<()> {
        use crossterm::{
            event::{DisableMouseCapture, EnableMouseCapture},
            execute,
        };
        if on == self.on {
            return Ok(());
        }
        if on {
            execute!(std::io::stdout(), EnableMouseCapture)?;
        } else {
            execute!(std::io::stdout(), DisableMouseCapture)?;
        }
        self.on = on;
        Ok(())
    }
}

/// Puts the terminal into TUI mode and guarantees it comes back, even on panic.
///
/// The terminal and the mouse capture are separate fields so a caller can lend out both at
/// once: the event loop draws on one and toggles the other.
pub struct Guard {
    /// The ratatui terminal, backed by the alternate screen and raw mode.
    pub terminal: ratatui::DefaultTerminal,
    /// This terminal's mouse capture.
    pub mouse: Mouse,
}

impl Guard {
    /// Enter raw mode and the alternate screen, and set mouse capture to `mouse`.
    ///
    /// The terminal's own title goes onto its title stack on the way in and comes back off it
    /// on the way out, so a tab the meters renamed reads as it did before svolder ran.
    ///
    /// Installs a panic hook ahead of every other one so a panic mid-draw still restores the
    /// terminal before the default hook prints its message. It turns mouse capture off first,
    /// drains and discards input and puts the title back, as [`Guard::drop`] does, so the shell
    /// the panic drops back into isn't left swallowing clicks; a terminal that rejects one of
    /// those sequences is no reason not to restore the rest, so its answer is dropped.
    pub fn enter(mouse: bool) -> std::io::Result<Guard> {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            mouse_off_then_flush(&mut Mouse { on: true }, &mut StdinFlush);
            restore_title();
            ratatui::restore();
            hook(info);
        }));
        let terminal = ratatui::try_init()?;
        let mut guard = Guard {
            terminal,
            mouse: Mouse::default(),
        };
        guard.mouse.set_mouse(mouse)?;
        title::emit(&title::push_title());
        Ok(guard)
    }
}

/// Say svolder's own name in the title and then pop the terminal's own back off its stack.
///
/// The plain title comes first because a terminal with no title stack ignores the pop and
/// would otherwise be left showing whatever level the meters last drew.
fn restore_title() {
    let _ = title::TerminalTitle.set_title(title::PLAIN);
    title::emit(&title::pop_title());
}

impl Drop for Guard {
    /// Turn mouse capture off, drain and discard the input that arrives meanwhile, put the
    /// terminal's own title back, and leave the alternate screen and raw mode, best-effort.
    fn drop(&mut self) {
        mouse_off_then_flush(&mut self.mouse, &mut StdinFlush);
        restore_title();
        ratatui::restore();
    }
}

/// Which side of a frame a synchronized-output mark goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncEdge {
    /// Before the frame's first byte.
    Begin,
    /// After its last.
    End,
}

/// What brackets a frame in synchronized output (DEC 2026), so a terminal that knows the mode
/// shows each frame whole or not at all.
pub type FrameSync = Box<dyn FnMut(SyncEdge)>;

/// The mark that opens a synchronized frame.
const BEGIN_SYNC: &str = "\x1b[?2026h";
/// The mark that closes it.
const END_SYNC: &str = "\x1b[?2026l";

/// The terminal's own marks, written straight to stdout, where the frame itself goes. A terminal
/// that does not know the mode ignores them.
pub fn frame_sync() -> FrameSync {
    Box::new(|edge| {
        super::decoration::title::emit(match edge {
            SyncEdge::Begin => BEGIN_SYNC,
            SyncEdge::End => END_SYNC,
        });
    })
}

/// No marks at all, for a frame drawn where no terminal reads them.
pub fn no_sync() -> FrameSync {
    Box::new(|_| {})
}

/// How a run claims its terminal: whether there is one, and the claiming.
pub(in crate::tui) trait Claim {
    /// Whether the output is a terminal at all.
    fn is_terminal(&self) -> bool;
    /// Put the terminal in the state the TUI draws in, with the mouse reported when `mouse`.
    fn enter(&self, mouse: bool) -> std::io::Result<Guard>;
}

/// The process's own terminal.
pub(in crate::tui) struct Stdout;

impl Claim for Stdout {
    fn is_terminal(&self) -> bool {
        std::io::stdout().is_terminal()
    }

    fn enter(&self, mouse: bool) -> std::io::Result<Guard> {
        Guard::enter(mouse)
    }
}

/// What a run needs before it draws anything: a terminal, and a configuration that could be
/// read. The terminal is checked first, so a run whose output is being read says so whatever
/// the file holds.
pub(super) fn preflight(
    claim: &impl Claim,
    config: Result<Config, svolder_ipc::ConfigError>,
) -> Result<Config, Failure> {
    if !claim.is_terminal() {
        return Err(Failure::of(
            Kind::Usage,
            "the TUI needs a terminal; its output is being read",
        ));
    }
    config.map_err(|e| Failure::of(Kind::ConfigUnreadable, e.to_string()))
}

/// Claim the terminal, or say why it cannot be.
pub(super) fn claimed(claim: &impl Claim, mouse: bool) -> Result<Guard, Failure> {
    claim
        .enter(mouse)
        .map_err(|e| Failure::of(Kind::Usage, format!("the terminal: {e}")))
}

#[cfg(test)]
mod tests;
