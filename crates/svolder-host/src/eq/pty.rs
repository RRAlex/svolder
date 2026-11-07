// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A pseudo-terminal for `pw-cli` to talk through.
//!
//! On a pipe `pw-cli` buffers its output in blocks that are not flushed per line, so its
//! refusals would arrive long after the line they answer. On a terminal it writes line by line.
//! Nothing here is interactive: svolder writes lines to one end and reads the answers from it.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use tokio::net::unix::pipe::{Receiver, Sender};

/// A terminal: the two ends svolder holds and the one the child is given.
pub(super) struct Pty {
    /// What svolder writes to.
    pub(super) to_child: Sender,
    /// What svolder reads from; ends with an error once the child has gone.
    pub(super) from_child: Receiver,
    /// The child's end, to be given to it as its standard streams.
    pub(super) slave: OwnedFd,
}

fn check(result: libc::c_int) -> io::Result<libc::c_int> {
    match result {
        -1 => Err(io::Error::last_os_error()),
        ok => Ok(ok),
    }
}

/// Open a terminal whose end for the child does not echo, is very wide (so nothing the line
/// editor draws wraps) and is not inherited by any other program svolder starts.
pub(super) fn open() -> io::Result<Pty> {
    let flags = libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC;
    // SAFETY: each call is given a descriptor this function owns and a buffer it sized; the
    // descriptors are wrapped in `OwnedFd` as soon as they exist, so none leaks on an error.
    unsafe {
        let master = OwnedFd::from_raw_fd(check(libc::posix_openpt(flags))?);
        check(libc::grantpt(master.as_raw_fd()))?;
        check(libc::unlockpt(master.as_raw_fd()))?;
        let mut name = [0 as libc::c_char; 128];
        let code = libc::ptsname_r(master.as_raw_fd(), name.as_mut_ptr(), name.len());
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code));
        }
        let slave = OwnedFd::from_raw_fd(check(libc::open(name.as_ptr(), flags))?);
        let mut attributes: libc::termios = std::mem::zeroed();
        check(libc::tcgetattr(slave.as_raw_fd(), &mut attributes))?;
        attributes.c_lflag &= !(libc::ECHO | libc::ECHONL);
        check(libc::tcsetattr(
            slave.as_raw_fd(),
            libc::TCSANOW,
            &attributes,
        ))?;
        let size = libc::winsize {
            ws_row: 24,
            ws_col: u16::MAX,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        check(libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ, &size))?;
        let reading = OwnedFd::from_raw_fd(check(libc::fcntl(
            master.as_raw_fd(),
            libc::F_DUPFD_CLOEXEC,
            3,
        ))?);
        // The two ends of the master share one open file, so this makes both non-blocking.
        let state = check(libc::fcntl(master.as_raw_fd(), libc::F_GETFL))?;
        check(libc::fcntl(
            master.as_raw_fd(),
            libc::F_SETFL,
            state | libc::O_NONBLOCK,
        ))?;
        Ok(Pty {
            to_child: Sender::from_owned_fd_unchecked(master)?,
            from_child: Receiver::from_owned_fd_unchecked(reading)?,
            slave,
        })
    }
}
