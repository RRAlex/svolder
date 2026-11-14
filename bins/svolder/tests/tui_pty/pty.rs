// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A pseudo-terminal pair and a child that has the slave end as its controlling terminal.

use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

/// The master end of a pty, with the size the terminal currently has.
pub struct Master {
    pub file: File,
}

/// Open a pty of `cols` x `rows` cells (`pixels` wide and high in all), returning the master and the slave.
pub fn open(cols: u16, rows: u16, pixels: (u16, u16)) -> io::Result<(Master, File)> {
    // SAFETY: the calls below take the descriptor they are given (one this function just made)
    // and a buffer it owns; every failure is turned into an error before the descriptor is used.
    unsafe {
        let raw = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC);
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let master = OwnedFd::from_raw_fd(raw);
        if libc::grantpt(raw) != 0 || libc::unlockpt(raw) != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut name = [0 as libc::c_char; 128];
        let err = libc::ptsname_r(raw, name.as_mut_ptr(), name.len());
        if err != 0 {
            return Err(io::Error::from_raw_os_error(err));
        }
        let slave = libc::open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        );
        if slave < 0 {
            return Err(io::Error::last_os_error());
        }
        let slave = File::from(OwnedFd::from_raw_fd(slave));
        let master = Master {
            file: File::from(master),
        };
        master.resize(cols, rows, pixels)?;
        Ok((master, slave))
    }
}

impl Master {
    /// Tell the terminal it is now `cols` x `rows` cells of `pixels` in all (`(0, 0)`: unknown,
    /// as most ptys report); the foreground process gets SIGWINCH.
    pub fn resize(&self, cols: u16, rows: u16, pixels: (u16, u16)) -> io::Result<()> {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: pixels.0,
            ws_ypixel: pixels.1,
        };
        // SAFETY: TIOCSWINSZ reads one `winsize` from the pointer, which outlives the call.
        let done = unsafe { libc::ioctl(self.file.as_raw_fd(), libc::TIOCSWINSZ, &size) };
        if done != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Start `command` with `slave` as its standard streams and controlling terminal, in a session of
/// its own (so its process group is its pid).
pub fn spawn(mut command: Command, slave: &File) -> io::Result<Child> {
    command
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave.try_clone()?));
    // SAFETY: the closure runs between fork and exec and calls only `setsid` and `ioctl`, both
    // async-signal-safe, on descriptor 0 (the slave) and touches no memory of the parent.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn()
}
