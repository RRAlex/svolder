// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! AutoEq as a family of requests: `Request::AutoEq { op }`, as `host_eq` carries its op.
//!
//! The daemon is the one process that reaches AutoEq's repository: `search` and `upload` are
//! plain requests answered at once; `preview` and `import` download a file, so they are jobs (see
//! [`super::jobs`]): the request answers with the job's id and the result rides in the job's
//! `done` ([`AutoEqPreview`], [`AutoEqSaved`]). What every family of jobs has is here too: the op
//! that cancels a job by its id, and the op that turns the connection into the stream of the
//! family's jobs.
//!
//! Every op that depends on a side of the EQ names it ([`AutoEqTarget`]); nothing means "the
//! shown device".

use serde::{Deserialize, Serialize};
use svolder_profiles::tone::fit::Preview;

use super::jobs::JobId;

/// The most characters a search query may have.
pub const AUTOEQ_QUERY_MAX: usize = 60;

/// The most headphones a search answers with; the rest is only counted.
pub const AUTOEQ_GROUPS_MAX: usize = 200;

/// One measurement of one headphone, named as the index lists it. The daemon looks it up in the
/// index it holds, so only a listed measurement is ever fetched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoEqVariant {
    /// the headphone's name, as AutoEq writes it
    pub name: String,
    /// who measured it
    pub source: String,
    /// `over-ear`, `in-ear` or `earbud`
    pub form: String,
    /// the measurement rig, when the folder names one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig: Option<String>,
}

/// The side of the EQ an op is about: a device, or the computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "side", rename_all = "snake_case")]
pub enum AutoEqTarget {
    /// A configured device, by name: its band count comes from its profile when it is offline.
    Device {
        /// the device's name
        device: String,
    },
    /// The computer's EQ.
    Computer,
}

/// One step of AutoEq, as an `auto_eq` request carries it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AutoEqOp {
    /// Search the index by name. Answered with [`super::Response::AutoEqSearch`]: the headphones
    /// that match, each with its measurements, the recommended one first. A query over
    /// [`AUTOEQ_QUERY_MAX`] characters is refused. Not a job: it runs on every key of a search
    /// box.
    Search {
        /// what to look for
        query: String,
        /// fetch the index again now, whatever the age of the cached copy
        #[serde(default)]
        refresh: bool,
    },
    /// Download a measurement's correction and work out its preview for a side (a job, answered
    /// with its id; the result is an [`AutoEqPreview`]). `compare` is a second measurement whose
    /// target and numbers come back as well.
    Preview {
        /// the measurement (boxed: the op rides in every `Request`)
        variant: Box<AutoEqVariant>,
        /// the side it is previewed for
        target: AutoEqTarget,
        /// a second measurement to overlay
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compare: Option<Box<AutoEqVariant>>,
    },
    /// Download a measurement and save it as an imported preset (a job, answered with its id; the
    /// result is an [`AutoEqSaved`]). A computer target refuses a correction of more filters
    /// than the computer EQ holds.
    Import {
        /// the measurement (boxed: the op rides in every `Request`)
        variant: Box<AutoEqVariant>,
        /// the preset's name; the headphone's own, shortened, when left out
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// write over the user's preset of that name
        #[serde(default)]
        force: bool,
        /// the side it is imported for
        target: AutoEqTarget,
    },
    /// Save parametric EQ text (the formats `svolder eq import` reads) as an imported preset.
    /// Plain request, no network; answered with [`super::Response::AutoEqSaved`]. The text is at
    /// most 64 KiB.
    Upload {
        /// the file's text
        text: String,
        /// the preset's name
        name: String,
        /// write over the user's preset of that name
        #[serde(default)]
        force: bool,
    },
    /// Stop a job; answered with an acknowledgement, or with the reason `no_such_job` when the
    /// daemon never ran or no longer remembers it. The acknowledgement says only that nothing
    /// runs under that id now: a job that has ended is acknowledged too.
    Cancel {
        /// the job's id
        job: JobId,
    },
    /// Turn the connection into the stream of the AutoEq jobs (`Event::AutoEq`): first the last
    /// state of each job the daemon still remembers, then each change as it happens. Opened with
    /// an acknowledgement, and held until the client goes. An event is the job's state, which may
    /// be sent again (a reader that fell behind is told where the jobs stand): a reader keeps the
    /// last state of each job and never counts events.
    Follow,
}

impl AutoEqOp {
    /// Whether the op is answered with a stream of events rather than one response.
    pub fn is_stream(&self) -> bool {
        matches!(self, AutoEqOp::Follow)
    }
}

/// One measurement of a headphone, in a search answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoEqMeasurement {
    /// the measurement to pass back to `preview` or `import`
    #[serde(flatten)]
    pub variant: AutoEqVariant,
    /// `source · form[ · rig]`: what tells it from the others of the headphone
    pub words: String,
}

/// One headphone and its measurements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoEqGroup {
    /// the headphone's key: its name as lowercase letters and digits
    pub key: String,
    /// its name, as the first measurement writes it
    pub name: String,
    /// its measurements, AutoEq's recommended one first
    pub variants: Vec<AutoEqMeasurement>,
}

/// The answer to a search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoEqSearch {
    /// the matching headphones, best match first, at most [`AUTOEQ_GROUPS_MAX`]
    pub groups: Vec<AutoEqGroup>,
    /// how many matching headphones are left out past that
    pub more: usize,
    /// how many measurements the index lists
    pub measurements: usize,
    /// when the index copy was fetched, in unix seconds
    pub fetched_unix: u64,
    /// what to say of where the copy came from: set when a refresh failed and the cached copy
    /// was used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The result of a `preview` job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoEqPreview {
    /// the measurement's preview on the grid
    pub preview: Preview,
    /// the second measurement's, on the same grid, when one was asked for
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<Preview>,
}

/// The result of an `import` job and the answer to an `upload`: the preset that was saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoEqSaved {
    /// the preset's name, as the file now spells it
    pub name: String,
    /// how many filters the source had
    pub filters: usize,
    /// how many lines of the source the parser left out
    pub skipped: usize,
}
