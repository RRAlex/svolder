// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the daemon's jobs say on the wire, the same for every family.
//!
//! A request that starts long work answers at once with a job id, and the work reports as it
//! goes on the family's stream: one [`JobEvent`] per change of the job's [`JobState`]. The answer
//! is [`Response::Job`](super::Response::Job). A family puts its own meaning into the `payload`
//! of a progress and into the result a job ends with ([`JobState::Done`]); the mechanism never
//! reads either. The result is part of the job's last state, which a follower that joins late, or
//! reconnects, is given first: it never has to have caught a progress to know it.

use serde::{Deserialize, Serialize};

use super::Reason;

/// A job's number. The daemon gives each job one that no other job of its run has had, so an
/// event naming it is never about an earlier job.
pub type JobId = u64;

/// The most a job's result may weigh as JSON, in bytes. A job whose result is larger ends as
/// failed (reason `internal`) instead, so a result always fits the line a stream sends it in.
pub const JOB_RESULT_MAX_BYTES: usize = 512 * 1024;

/// Where a job stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobState {
    /// The job exists and its work has been handed on.
    Started,
    /// The job has something to say of its work; the family says what.
    Progress {
        /// the family's own words, opaque to the mechanism; `null` when there are none
        #[serde(default)]
        payload: serde_json::Value,
    },
    /// The work finished.
    Done {
        /// what the job produced, in the family's own words, opaque to the mechanism and at most
        /// [`JOB_RESULT_MAX_BYTES`] as JSON; `null` (and left out of the line) when there is none
        #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
        payload: serde_json::Value,
    },
    /// The work could not finish.
    Failed {
        /// what went wrong, in the daemon's words
        message: String,
        /// why, in a word a front end can match on; unknown from a daemon that gave none
        #[serde(default = "unknown_reason")]
        reason: Reason,
    },
    /// The job was stopped before it finished, by its id or by the daemon's own stop.
    Cancelled,
    /// A state this build does not know, from a daemon newer than it.
    #[serde(other)]
    Unknown,
}

fn unknown_reason() -> Reason {
    Reason::Unknown
}

impl JobState {
    /// Whether this state ends the job: nothing follows it.
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            JobState::Done { .. } | JobState::Failed { .. } | JobState::Cancelled
        )
    }
}

/// One job's state, as a family's stream carries it. The stream tells states, not changes: a
/// state may be sent again (a reader that fell behind is told where each job stands), so a
/// reader keeps the last one it has of each job and never counts events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobEvent {
    /// the job it is about
    pub job: JobId,
    /// where the job stands now
    pub state: JobState,
}
