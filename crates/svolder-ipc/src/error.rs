// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors the IPC layer and the config loader return.

use std::path::PathBuf;

/// A failure in the socket protocol or transport.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// The underlying socket I/O failed.
    #[error("socket io: {0}")]
    Io(#[from] std::io::Error),
    /// A line could not be encoded or decoded as JSON.
    #[error("protocol json: {0}")]
    Json(#[from] serde_json::Error),
    /// The peer sent something the protocol does not allow.
    #[error("protocol: {0}")]
    Protocol(String),
    /// The peer's line ran past [`crate::socket::lines::MAX_LINE`] bytes without ending.
    #[error(
        "the daemon's reply line is longer than {} bytes",
        crate::socket::lines::MAX_LINE
    )]
    ReplyTooLong,
    /// No daemon is listening on the socket.
    #[error("no daemon is listening")]
    NotRunning,
}

/// A failure reading or validating the config file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The TOML did not parse or a field was invalid.
    #[error("config: {0}")]
    Parse(String),
    /// The file could not be read.
    #[error("config io {0}: {1}")]
    Io(PathBuf, std::io::Error),
    /// The `schema` field is a version this build does not understand.
    #[error("config schema {0} is not supported (expected 1)")]
    Schema(u32),
    /// The file is read-only, which a writer takes to be on purpose.
    #[error("config {0} is read-only")]
    ReadOnly(PathBuf),
}
