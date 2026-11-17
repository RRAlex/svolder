// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors the IPC layer and the config loader return.

use std::path::PathBuf;

use svolder_profiles::tone::preset::NameRefusal;

/// A failure in the socket protocol or transport.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// The underlying socket I/O failed for a reason that is not the daemon going away (a
    /// permission, a path too long); the ones that mean it went are [`IpcError::DaemonGone`].
    #[error("socket io: {0}")]
    Io(#[from] std::io::Error),
    /// The daemon went away under a conversation: the pipe broke, the connection was reset or
    /// closed mid-reply, or the socket is missing or refused. The device is not at fault.
    #[error("The svolder daemon stopped.")]
    DaemonGone,
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

impl IpcError {
    /// The error for a read or write that failed on a conversation already under way: a broken
    /// pipe, a reset or aborted connection and a stream cut short mean the daemon went away
    /// ([`IpcError::DaemonGone`]); any other failure stays [`IpcError::Io`]. Only the client's
    /// wire reads and writes call this, so the daemon's own socket errors (a bind that cannot
    /// find its directory) keep their words.
    pub fn from_wire(err: std::io::Error) -> IpcError {
        use std::io::ErrorKind::{BrokenPipe, ConnectionAborted, ConnectionReset, UnexpectedEof};
        match err.kind() {
            BrokenPipe | ConnectionReset | ConnectionAborted | UnexpectedEof => {
                IpcError::DaemonGone
            }
            _ => IpcError::Io(err),
        }
    }

    /// Whether the daemon is not there to talk to: it went away mid-conversation, or nothing
    /// listens on the socket.
    pub fn daemon_gone(&self) -> bool {
        matches!(self, IpcError::DaemonGone | IpcError::NotRunning)
    }
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
    /// The file is gone and the writer was told to leave it so rather than start one from the
    /// template ([`Config::while_file_must_exist`]); the words are the daemon's missing-file
    /// refusal.
    ///
    /// [`Config::while_file_must_exist`]: crate::Config::while_file_must_exist
    #[error("{}", crate::socket::daemon_version::config_missing_refusal(&.0.display().to_string()))]
    Gone(PathBuf),
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
            ConfigError::ReadOnly(_) | ConfigError::Gone(_) => Reason::ReadOnly,
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

/// A serde or TOML parser's `message` without the value it found: `invalid type: integer
/// `4321`, expected a string` reads `invalid type: integer, expected a string`. A value that
/// does not fit may be the passphrase, and an answer never shows one back.
pub(crate) fn without_value(message: &str) -> String {
    let wrong_value =
        message.starts_with("invalid type: ") || message.starts_with("invalid value: ");
    if !wrong_value {
        return message.to_string();
    }
    let (found, rest) = match message.rsplit_once(", expected") {
        Some((found, rest)) => (found, format!(", expected{rest}")),
        None => (message, String::new()),
    };
    let found = match found.find(['`', '"']) {
        Some(at) => found[..at].trim_end(),
        None => found,
    };
    format!("{found}{rest}")
}

/// `line L, column C` of byte `offset` in `text`, both counted from 1.
pub(crate) fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = &text[..end];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count())
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind};

    use super::*;

    /// A wire failure that means the peer went away is the one typed case, in plain words.
    #[test]
    fn wire_failures_that_mean_the_daemon_went_are_one_typed_case() {
        for kind in [
            ErrorKind::BrokenPipe,
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted,
            ErrorKind::UnexpectedEof,
        ] {
            let err = IpcError::from_wire(Error::from(kind));
            assert!(matches!(err, IpcError::DaemonGone), "{kind:?}: {err:?}");
            assert!(err.daemon_gone());
            assert_eq!(err.to_string(), "The svolder daemon stopped.");
        }
    }

    /// An error that is not a wire failure, and every error through `?` (a bind on a missing
    /// directory included), keeps its own words; a missing listener counts as gone.
    #[test]
    fn other_io_failures_stay_io_and_no_listener_counts_as_gone() {
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::NotFound,
            ErrorKind::ConnectionRefused,
        ] {
            let wire = IpcError::from_wire(Error::from(kind));
            assert!(matches!(wire, IpcError::Io(_)), "{wire:?}");
            assert!(!wire.daemon_gone());
        }
        let bind = IpcError::from(Error::from(ErrorKind::NotFound));
        assert!(matches!(bind, IpcError::Io(_)), "{bind:?}");
        assert!(
            IpcError::from(Error::from(ErrorKind::BrokenPipe))
                .to_string()
                .starts_with("socket io")
        );
        assert!(IpcError::NotRunning.daemon_gone());
        assert!(!IpcError::Protocol("x".into()).daemon_gone());
    }
}
