// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device walk as a family of requests: `Request::Walk { op }`, as `host_eq` carries its op.
//!
//! The walk is a daemon job (see [`super::jobs`]), one at a time daemon-wide. `start` answers with
//! the job's id ([`Response::Job`](super::Response::Job)); the walk then says where it stands on
//! the family's stream (`follow`), each state whole: the prompt it shows, the devices heard so
//! far, why the last answer did not move it on. `answer` and `back` act on the prompt the job
//! shows while it waits for one, and `cancel` stops it. A daemon that has these ops lists the
//! feature [`FEATURE_WALK`](super::FEATURE_WALK).
//!
//! An answer may carry a passphrase, so nothing of an op is ever shown back: an op that does not
//! parse is refused in words that name the key at fault and never its value (the op is read by
//! hand here, since serde's own words for a value of the wrong type quote it), and an op printed
//! with `{:?}` hides an answer's text. The hand reader takes an op as an object only and, of a key
//! given twice, the last.

use std::time::Duration;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use super::jobs::JobId;

/// How long a walk waits for an answer before it ends as cancelled: a front end that went away
/// leaves the one walk free again after this.
pub const WALK_IDLE_BOUND: Duration = Duration::from_secs(600);

/// One step of the device walk, as a `walk` request carries it.
#[derive(Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WalkOp {
    /// Start a walk; answered with its job's id, or refused `busy` while another walk runs (its
    /// id and its origin are in the follow stream's states).
    Start,
    /// Answer the prompt the walk shows: the number of a choice (from 0), or a line of text where
    /// the prompt takes text. Acknowledged once the walk has it; refused `not_waiting` while the
    /// walk works on an answer or once it has ended, and `no_such_job` for an id it never had.
    Answer {
        /// the job's id
        job: JobId,
        /// the choice, from 0; ignored where the prompt takes text
        #[serde(default)]
        choice: usize,
        /// the line typed where the prompt takes text (a name, a passphrase)
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Go back to the step before, refused as an answer is.
    Back {
        /// the job's id
        job: JobId,
    },
    /// Stop a job; answered with an acknowledgement, or with the reason `no_such_job` when the
    /// daemon never ran or no longer remembers it. The acknowledgement says only that nothing
    /// runs under that id now: a job that has ended is acknowledged too.
    Cancel {
        /// the job's id
        job: JobId,
    },
    /// Turn the connection into the stream of the walk's jobs (`Event::Walk`): first the last
    /// state of each job the daemon still remembers, then each change as it happens. Opened with
    /// an acknowledgement, and held until the client goes. An event is the job's state, which may
    /// be sent again (a reader that fell behind is told where the jobs stand): a reader keeps the
    /// last state of each job and never counts events.
    Follow,
}

impl WalkOp {
    /// Whether the op is answered with a stream of events rather than one response.
    pub fn is_stream(&self) -> bool {
        matches!(self, WalkOp::Follow)
    }
}

impl std::fmt::Debug for WalkOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalkOp::Start => f.write_str("Start"),
            WalkOp::Answer { job, choice, text } => f
                .debug_struct("Answer")
                .field("job", job)
                .field("choice", choice)
                .field("text", &text.as_ref().map(|_| "<hidden>"))
                .finish(),
            WalkOp::Back { job } => f.debug_struct("Back").field("job", job).finish(),
            WalkOp::Cancel { job } => f.debug_struct("Cancel").field("job", job).finish(),
            WalkOp::Follow => f.write_str("Follow"),
        }
    }
}

/// The ops' names on the wire, as a refusal lists them.
const OPS: &str = "start, answer, back, cancel, follow";

/// The whole number under `key`, which the op cannot do without.
fn number(keys: &mut Map<String, Value>, key: &str) -> Result<u64, String> {
    match keys.remove(key) {
        Some(Value::Number(n)) if n.as_u64().is_some() => Ok(n.as_u64().unwrap_or_default()),
        _ => Err(format!("`{key}` is needed, as a whole number")),
    }
}

/// The op `keys` spell: its name under `op`, then its own keys; a key the op does not have is
/// passed over, as every request's is.
fn op_of(mut keys: Map<String, Value>) -> Result<WalkOp, String> {
    let op = match keys.remove("op") {
        Some(Value::String(op)) => op,
        _ => return Err(format!("`op` must be one of {OPS}")),
    };
    Ok(match op.as_str() {
        "start" => WalkOp::Start,
        "answer" => WalkOp::Answer {
            job: number(&mut keys, "job")?,
            choice: match keys.contains_key("choice") {
                true => usize::try_from(number(&mut keys, "choice")?)
                    .map_err(|_| "`choice` is too large".to_string())?,
                false => 0,
            },
            text: match keys.remove("text") {
                None | Some(Value::Null) => None,
                Some(Value::String(text)) => Some(text),
                Some(_) => return Err("`text` must be a string".to_string()),
            },
        },
        "back" => WalkOp::Back {
            job: number(&mut keys, "job")?,
        },
        "cancel" => WalkOp::Cancel {
            job: number(&mut keys, "job")?,
        },
        "follow" => WalkOp::Follow,
        _ => return Err(format!("`op` must be one of {OPS}")),
    })
}

impl<'de> Deserialize<'de> for WalkOp {
    fn deserialize<D: Deserializer<'de>>(from: D) -> Result<Self, D::Error> {
        match Value::deserialize(from)? {
            Value::Object(keys) => op_of(keys).map_err(D::Error::custom),
            _ => Err(D::Error::custom(format!(
                "a walk op is an object whose `op` is one of {OPS}"
            ))),
        }
    }
}

/// How a device answered the walk's passphrase test: the session it let in, the code it turned
/// away, or why no answer came.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Connect {
    /// The device let the session in.
    Accepted,
    /// The device turned the passphrase away.
    Refused,
    /// No answer came from the device.
    Failed {
        /// why not
        kind: ConnectFailure,
    },
    /// An outcome a later daemon gives that this build does not know.
    #[serde(other)]
    Unknown,
}

/// Why a passphrase test got no answer from the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectFailure {
    /// No listen heard the device.
    NotInRange,
    /// The device was heard, but the link to it could not be opened.
    Link,
    /// A configured device's actor reaches for that unit, and the test does not fight it.
    Held,
    /// The daemon is stopping and starts no test.
    Stopping,
    /// A kind a later daemon gives that this build does not know.
    #[serde(other)]
    Unknown,
}
