// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Virtual speakers' sets as a family of requests: `Request::Speakers { op }`, as `auto_eq`
//! carries its op.
//!
//! The daemon is the one process that reaches the sets' sources and keeps the converted sets:
//! `catalogue` and `forget` are plain requests answered at once; `fetch` and `import` convert a
//! file, so they are jobs (see [`super::jobs`]): the request answers with the job's id, the
//! progress of a download rides in the job's progress events ([`SpeakersProgress`]) and the result
//! in its `done` ([`SpeakersSaved`]). What every family of jobs has is here too: the op that
//! cancels a job by its id, and the op that turns the connection into the stream of the family's
//! jobs.
//!
//! Which set plays, and whether the stage is on, is the computer EQ's (`host_eq`); this family
//! only manages the sets.

use serde::{Deserialize, Serialize};

use super::jobs::JobId;

/// How the daemon's words begin when it is set up to fetch nothing (its sets' base address is the
/// discard port): a client that sees them offers no "try again later".
pub const SPEAKERS_OFF_PREFIX: &str = "fetching sets is off";

/// The two choices: the speakers heard in a measured room, or in the open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeakersRoom {
    /// With room (a binaural room impulse response, BRIR).
    With,
    /// No room (a head-related transfer function, HRTF).
    No,
}

/// One step of the sets' management, as a `speakers` request carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SpeakersOp {
    /// List the catalogue's sets and the ones the person imported, each with whether it is
    /// fetched. Answered with [`super::Response::SpeakersCatalogue`]. Reaches nothing.
    Catalogue,
    /// Download a catalogue set, check it, convert it and keep the result (a job, answered with
    /// its id; the result is a [`SpeakersSaved`]).
    Fetch {
        /// the set's catalogue id
        id: String,
    },
    /// Convert a file the daemon can read (a SOFA file, or a WAV of four or fourteen channels)
    /// and keep the result as the person's own set (a job, answered with its id; the result is a
    /// [`SpeakersSaved`]).
    Import {
        /// where the file is, on the daemon's machine
        path: String,
        /// the name to show; the file's own name when left out
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// which choice it belongs under; `no` (free field) when left out
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room: Option<SpeakersRoom>,
    },
    /// Where the daemon wants the files it is asked to import put while they wait to be read: a
    /// private directory under its set store, swept when it starts. Answered with
    /// [`super::Response::SpeakersImportDir`]. For a front end that holds a file's bytes and not
    /// a path the daemon can read (the web page's import).
    ImportDir,
    /// Remove a fetched or imported set. Refused with the reason `busy` while the set plays or a
    /// fetch or import is running.
    Forget {
        /// the set's id
        id: String,
    },
    /// Stop a job; answered with an acknowledgement, or with the reason `no_such_job` when the
    /// daemon never ran or no longer remembers it.
    Cancel {
        /// the job's id
        job: JobId,
    },
    /// Turn the connection into the stream of the sets' jobs (`Event::Speakers`): first the last
    /// state of each job the daemon still remembers, then each change as it happens. Opened with
    /// an acknowledgement, and held until the client goes. A reader keeps the last state of each
    /// job and never counts events.
    Follow,
}

impl SpeakersOp {
    /// Whether the op is answered with a stream of events rather than one response.
    pub fn is_stream(&self) -> bool {
        matches!(self, SpeakersOp::Follow)
    }
}

/// Where a set's two speakers were measured, as the source says.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeakersAngles {
    /// the left speaker's azimuth in degrees, counter-clockwise from the front
    pub left_deg: f64,
    /// the right speaker's azimuth in degrees
    pub right_deg: f64,
    /// the measurement distance in metres
    pub distance_m: f64,
}

/// One set: a catalogue entry, a fetched set, or one the person imported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakersSet {
    /// the id: the file name the set is kept under
    pub id: String,
    /// with room or no room
    pub room: SpeakersRoom,
    /// the dataset and subject, or the file's name for an imported set
    pub label: String,
    /// the licence as an SPDX identifier
    pub licence: String,
    /// the dataset's own credit words; empty where the licence asks for none
    pub credit: String,
    /// the page the dataset is described on; empty for an imported set
    pub source_url: String,
    /// the size of the download in bytes; 0 for an imported set
    pub bytes: u64,
    /// whether the person imported it
    pub own: bool,
    /// whether it is on this machine
    pub fetched: bool,
    /// what is wrong with a stored set that cannot be played, in words
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damaged: Option<String>,
    /// where the speakers were measured, for a fetched set
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angles: Option<SpeakersAngles>,
    /// the length of the responses in milliseconds, for a fetched set
    #[serde(default)]
    pub taps_ms: f64,
    /// the sample rate in Hz, for a fetched set
    #[serde(default)]
    pub rate: u32,
    /// how much quieter a centred signal is than with no virtual speakers, in dB, for a fetched
    /// set
    #[serde(default)]
    pub loudness_db: f64,
    /// whether the responses were cut to the length cap
    #[serde(default)]
    pub cut: bool,
}

/// The answer to `catalogue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakersCatalogue {
    /// the catalogue's sets first (in the catalogue's order), then the others stored
    pub sets: Vec<SpeakersSet>,
    /// whether this daemon can fetch: false when it is set up to reach nothing
    pub fetching: bool,
    /// what to say of the state of the sets, when something needs saying
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// how many sets are kept at most; the oldest unused one goes past that
    pub max_sets: usize,
}

/// What a running `fetch` says of itself, in the job's progress events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakersProgress {
    /// the set being fetched
    pub id: String,
    /// the bytes received so far
    pub received: u64,
    /// the bytes expected
    pub total: u64,
}

/// The result of a `fetch` or `import` job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakersSaved {
    /// the set as it is now stored
    pub set: SpeakersSet,
    /// the ids of the older sets that went to make room
    #[serde(default)]
    pub evicted: Vec<String>,
}
