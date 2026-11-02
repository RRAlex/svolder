// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bounded line reader both ends of the socket read through.
//!
//! A line is bytes up to a newline. The reader yields them as bytes, holds at most
//! [`MAX_LINE`] of them, and leaves UTF-8 and JSON to the caller, so a line that is not text
//! is the caller's to answer rather than an I/O error that ends the connection.

use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// The longest line, in bytes and without its newline, either end of the socket accepts.
pub const MAX_LINE: usize = 1024 * 1024;

/// What one read of a [`LineReader`] found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// A whole line, without its newline (and without a carriage return before it).
    Bytes(Vec<u8>),
    /// A line already past [`MAX_LINE`] bytes with no newline yet. What was read of it is
    /// dropped; [`LineReader::skip_line`] discards the rest.
    TooLong,
    /// The peer closed the connection, with no bytes after the last newline.
    Eof,
}

/// Reads newline-terminated lines from `R`, never holding more than [`MAX_LINE`] bytes of one.
///
/// A read is cancel-safe: the bytes of a partial line wait in the reader, so a read that loses
/// a `select!` race resumes where it stopped.
pub(crate) struct LineReader<R> {
    inner: R,
    buf: Vec<u8>,
    /// The most bytes `buf` has held at once.
    #[cfg(test)]
    high_water: usize,
}

impl<R: AsyncBufRead + Unpin> LineReader<R> {
    pub(crate) fn new(inner: R) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            #[cfg(test)]
            high_water: 0,
        }
    }

    /// The next line, or [`Line::TooLong`] once one runs past the cap.
    pub(crate) async fn next_line(&mut self) -> std::io::Result<Line> {
        loop {
            let available = self.inner.fill_buf().await?;
            if available.is_empty() {
                // A last line the peer closed without a newline is still a line.
                if self.buf.is_empty() {
                    return Ok(Line::Eof);
                }
                if self.buf.last() == Some(&b'\r') {
                    self.buf.pop();
                }
                return Ok(Line::Bytes(std::mem::take(&mut self.buf)));
            }
            let newline = available.iter().position(|&b| b == b'\n');
            let take = newline.unwrap_or(available.len());
            if self.buf.len() + take > MAX_LINE {
                self.buf.clear();
                return Ok(Line::TooLong);
            }
            self.buf.extend_from_slice(&available[..take]);
            #[cfg(test)]
            {
                self.high_water = self.high_water.max(self.buf.len());
            }
            self.inner.consume(take + usize::from(newline.is_some()));
            if newline.is_some() {
                if self.buf.last() == Some(&b'\r') {
                    self.buf.pop();
                }
                return Ok(Line::Bytes(std::mem::take(&mut self.buf)));
            }
        }
    }

    /// Discard the rest of the line a [`Line::TooLong`] stopped in, through its newline,
    /// holding none of it. `false` when the peer closed first.
    pub(crate) async fn skip_line(&mut self) -> std::io::Result<bool> {
        loop {
            let available = self.inner.fill_buf().await?;
            if available.is_empty() {
                return Ok(false);
            }
            match available.iter().position(|&b| b == b'\n') {
                Some(at) => {
                    self.inner.consume(at + 1);
                    return Ok(true);
                }
                None => {
                    let all = available.len();
                    self.inner.consume(all);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
