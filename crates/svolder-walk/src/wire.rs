// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A walk the daemon runs, as its job's progress says it: the whole of where it stands in each
//! state, so a front end that follows late, or reconnects, reads everything from the last one.

use serde::{Deserialize, Serialize};
use svolder_ipc::Connect;

use crate::{Prompt, Row, WalkError};

/// The most devices heard a state lists; a scan that hears more offers the first ones heard.
pub const ROWS_MAX: usize = 64;

/// Whether the walk waits for an answer or works on one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// It shows its prompt and takes an answer to it.
    Waiting,
    /// It works on the last answer; an answer now is refused `not_waiting`.
    Working,
}

/// What the walk works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Working {
    /// Listening for devices; the rows grow as they are heard.
    Scanning,
    /// Testing the passphrase on the device picked.
    Testing,
    /// Writing the device into the configuration.
    Writing,
    /// Anything else an answer does.
    Answering,
}

/// What kind of refusal the last answer met, for a front end that draws them differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    /// The answer is not one the step takes.
    Answer,
    /// Another configured device carries that name.
    Taken,
    /// The unit is configured already.
    Configured,
    /// The device turned the code away.
    Passphrase,
    /// The passphrase test got no answer from the device.
    Connect,
    /// An effect did not happen: a scan that could not run, a write that failed.
    Failed,
}

/// Why the last answer did not move the walk on: its prompt is asked again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalkRefusal {
    /// what to say, never quoting a passphrase
    pub message: String,
    /// what kind of refusal it is
    pub kind: RefusalKind,
}

impl WalkRefusal {
    /// The refusal `error` is, with an effect's failure in the words `effect` gives it.
    pub fn of<F>(error: &WalkError<F>, effect: impl FnOnce(&F) -> String) -> WalkRefusal {
        let kind = match error {
            WalkError::Effect(_) => RefusalKind::Failed,
            WalkError::Answer(_) | WalkError::NoDevice => RefusalKind::Answer,
            WalkError::NameTaken(_) => RefusalKind::Taken,
            WalkError::Configured { .. } | WalkError::UnitConfigured(_) => RefusalKind::Configured,
            WalkError::Refused { .. } => RefusalKind::Passphrase,
            WalkError::Connect(_) => RefusalKind::Connect,
        };
        WalkRefusal {
            message: error.words(effect),
            kind,
        }
    }
}

/// Where a walk the daemon runs stands: what each progress state of its job carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalkState {
    /// who started it: `socket`, `web`, `mcp` or `mqtt`
    pub origin: String,
    /// whether it waits for an answer or works on one
    pub phase: Phase,
    /// what it works on, while it works
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working: Option<Working>,
    /// the prompt it shows (while it works, the one being answered)
    pub prompt: Prompt,
    /// the devices heard, at most [`ROWS_MAX`]: while it scans, the ones heard so far
    pub rows: Vec<Row>,
    /// whether `back` would change the step
    pub can_go_back: bool,
    /// why the last answer did not move it on
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<WalkRefusal>,
    /// how the last passphrase test went
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect: Option<Connect>,
}
