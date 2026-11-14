// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors the IPC layer and the config loader return.

use std::path::PathBuf;

use svolder_profiles::tone::preset::NameRefusal;

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
    /// The peer's line ran past `MAX_LINE` bytes without ending.
    #[error(
        "the daemon's reply line is longer than {} bytes",
        crate::socket::lines::MAX_LINE
    )]
    ReplyTooLong,
    /// The daemon gave no answer within the bound the request was given.
    #[error("the daemon did not answer within {} s", .0.as_secs_f32())]
    Timeout(std::time::Duration),
    /// The daemon refused a stream: its whole answer, with the code and the reason a front end
    /// matches on. The error reads as the answer's message.
    #[error("{}", refusal_words(.0))]
    Refused(Box<crate::socket::protocol::Response>),
    /// No daemon is listening on the socket.
    #[error("no daemon is listening")]
    NotRunning,
}

/// The words of a refused answer: its message, or a note when the answer is of another kind.
fn refusal_words(answer: &crate::socket::protocol::Response) -> String {
    match answer {
        crate::socket::protocol::Response::Error { message, .. } => message.clone(),
        other => format!("refused with {other:?}"),
    }
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
    /// Another writer held the file's lock until the wait ran out.
    #[error("config io {0}: {BUSY}")]
    Busy(PathBuf),
    /// A writer refused a preset's name: taken, or not a name the core accepts. The words are the
    /// core's own ([`NameRefusal`]'s), so a front end tells the two apart without reading them.
    #[error("config: {0}")]
    Name(NameRefusal),
    /// A writer found a block of this `kind` already carrying `name`.
    #[error("config: a {kind} named {name:?} is already configured")]
    Taken {
        /// What the block configures: `device`, `hop` or `preset`.
        kind: &'static str,
        /// The name as the file spells it.
        name: String,
    },
    /// A writer found no block of this `kind` carrying `name`.
    #[error("config: no {kind} named {name:?} is configured")]
    Missing {
        /// What the block configures: `device`, `hop` or `preset`.
        kind: &'static str,
        /// The name that was asked for.
        name: String,
    },
    /// A block a writer had just found was no longer in the document it was editing.
    #[error("config: {kind} {name:?} went missing")]
    Vanished {
        /// What the block configures: `device`, `hop` or `preset`.
        kind: &'static str,
        /// The name that was being edited.
        name: String,
    },
    /// Two `[[device]]` blocks name one unit; `first` holds it and `second` asks for it.
    #[error(
        "config: devices {first:?} and {second:?} are both configured for unit {udid}; \
         a unit is configured once, so remove one of the two blocks"
    )]
    UnitTwice {
        /// The device that holds the unit.
        first: String,
        /// The device that asks for it too.
        second: String,
        /// The unit id as the second block spells it.
        udid: String,
    },
}

impl ConfigError {
    /// The one mapping from a refusal of a config writer to the [`Reason`] a socket answer
    /// carries, so a front end tells a taken name from a missing block, a read-only file or a
    /// busy one without reading the words.
    ///
    /// [`Reason`]: crate::socket::protocol::Reason
    pub fn reason(&self) -> crate::socket::protocol::Reason {
        use crate::socket::protocol::Reason;
        match self {
            ConfigError::Taken { .. } | ConfigError::Name(NameRefusal::Taken(_)) => Reason::Taken,
            ConfigError::Name(_) => Reason::BadName,
            ConfigError::Missing { .. } => Reason::Missing,
            ConfigError::ReadOnly(_) => Reason::ReadOnly,
            ConfigError::Busy(_) => Reason::FileBusy,
            ConfigError::Parse(_) | ConfigError::Schema(_) | ConfigError::UnitTwice { .. } => {
                Reason::Invalid
            }
            ConfigError::Io(..) | ConfigError::Vanished { .. } => Reason::Internal,
        }
    }
}

/// The words a lock still held when the wait runs out fails with.
pub(crate) const BUSY: &str = "another svolder is writing this file";
