// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The curated list of sets that can be fetched.
//!
//! Each entry pins one URL by its size and SHA-256, so a source that changes upstream is a
//! refused checksum and never a silent other file. Only sets whose licence allows any use are
//! listed; anything else can still be imported from a file the person has.

use serde::{Deserialize, Serialize};

use crate::convert::Recipe;

/// The two choices: the speakers heard in a measured room, or in the open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Room {
    /// The speakers in a measured room (a binaural room impulse response, BRIR).
    With,
    /// The same speakers in the open (a head-related transfer function, HRTF).
    No,
}

impl Room {
    /// The wire word, `with` or `no`.
    pub fn wire(self) -> &'static str {
        match self {
            Room::With => "with",
            Room::No => "no",
        }
    }

    /// The choice as the person reads it.
    pub fn label(self) -> &'static str {
        match self {
            Room::With => "With room",
            Room::No => "No room",
        }
    }

    /// The technical term, shown beside the label for people who know it.
    pub fn term(self) -> &'static str {
        match self {
            Room::With => "BRIR",
            Room::No => "HRTF",
        }
    }
}

/// How a download holds the SOFA file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// The download is the SOFA file.
    Sofa,
    /// The download is a zip archive holding it as this member.
    Zip {
        /// The member's path in the archive.
        member: &'static str,
    },
}

/// One set the catalogue lists.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// The id: lower-case letters, digits and dashes.
    pub id: &'static str,
    /// With room or no room.
    pub room: Room,
    /// The dataset and its subject, as shown.
    pub label: &'static str,
    /// The one URL it is fetched from.
    pub url: &'static str,
    /// The download's size in bytes.
    pub bytes: u64,
    /// The download's SHA-256.
    pub sha256: &'static str,
    /// What the download is.
    pub container: Container,
    /// The left speaker's azimuth in degrees.
    pub left_az: f64,
    /// The right speaker's azimuth in degrees.
    pub right_az: f64,
    /// The dataset's licence as an SPDX identifier, or a `LicenseRef-` for words without one.
    pub licence: &'static str,
    /// The dataset's own credit words.
    pub credit: &'static str,
    /// The page the dataset is described on.
    pub source_page: &'static str,
}

impl Entry {
    /// The recipe that picks this set's two speakers from its SOFA file.
    pub fn recipe(&self) -> Recipe {
        Recipe {
            left_az: self.left_az,
            right_az: self.right_az,
            tolerance: 1.0,
        }
    }
}

/// The sets that can be fetched.
pub fn entries() -> &'static [Entry] {
    &ENTRIES
}

/// The entry with `id`.
pub fn find(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.id == id)
}

const ENTRIES: [Entry; 6] = [
    Entry {
        id: "mit-normal",
        room: Room::No,
        label: "MIT, normal pinna",
        url: "https://sofacoustics.org/data/database/mit/mit_kemar_normal_pinna.sofa",
        bytes: 1_171_764,
        sha256: "e7035994f5fd754058424c061380ee92b1d5ed58fccef2887a4266916616acdf",
        container: Container::Sofa,
        left_az: 30.0,
        right_az: 330.0,
        licence: "LicenseRef-MIT-Media-Lab-free",
        credit: "Gardner and Martin (1995)",
        source_page: "https://sofacoustics.org/data/database/mit/",
    },
    Entry {
        id: "sadie-d2",
        room: Room::No,
        label: "SADIE II, D2",
        url: "https://sofacoustics.org/data/database/sadie/D2_48K_24bit_256tap_FIR_SOFA.sofa",
        bytes: 36_591_729,
        sha256: "bb5980288fc5c990c821e02c5ddfa274759079b723973cd6ca47ac577243aa0c",
        container: Container::Sofa,
        left_az: 30.0,
        right_az: 330.0,
        licence: "Apache-2.0",
        credit: "SADIE II, University of York",
        source_page: "https://sofacoustics.org/data/database/sadie/",
    },
    Entry {
        id: "hutubs-pp1",
        room: Room::No,
        label: "HUTUBS, subject pp1",
        url: "https://sofacoustics.org/data/database/hutubs/pp1_HRIRs_measured.sofa",
        bytes: 1_659_720,
        sha256: "f626d335c590db687608fff5992972725230d594067b2e6906893bddc85e5173",
        container: Container::Sofa,
        left_az: 30.0,
        right_az: 330.0,
        licence: "CC-BY-4.0",
        credit: "HUTUBS, Audio Communication Group, TU Berlin",
        source_page: "https://ecosystem.sonicom.eu/databases/76",
    },
    Entry {
        id: "ari-nh2",
        room: Room::No,
        label: "ARI, subject NH2",
        url: "https://sofacoustics.org/data/database/ari/hrtf_nh2.sofa",
        bytes: 2_962_116,
        sha256: "809446dbf3258290b4246a4f77b25a90a0493267b92608625d16666b986e5512",
        container: Container::Sofa,
        left_az: 30.0,
        right_az: 330.0,
        licence: "CC-BY-SA-3.0",
        credit: "Acoustics Research Institute, Austrian Academy of Sciences",
        source_page: "https://sofacoustics.org/data/database/ari/",
    },
    Entry {
        id: "cipic-003",
        room: Room::No,
        label: "CIPIC, subject 003",
        url: "https://sofacoustics.org/data/database/cipic/subject_003.sofa",
        bytes: 1_837_391,
        sha256: "a9db5f938ed1113b118dcbeb43f5fe1b27b9e191d1bbdb63fbc4803b6959b220",
        container: Container::Sofa,
        left_az: 30.0,
        right_az: 330.0,
        licence: "LicenseRef-CIPIC-any-purpose",
        credit: "The Regents of the University of California, CIPIC Interface Laboratory",
        source_page: "https://sofacoustics.org/data/database/cipic/",
    },
    Entry {
        id: "sadie-d2-brir",
        room: Room::With,
        label: "SADIE II, D2 room",
        url: "https://zenodo.org/api/records/12092466/files/D2_BRIR_SOFA.zip/content",
        bytes: 8_359_449,
        sha256: "ae17a404812ddffef0144c86dbf10dbeb92a466b367b36717d0798e5af73bd6a",
        container: Container::Zip {
            member: "D2_BRIR_SOFA/D2_48K_24bit_0.3s_FIR_SOFA.sofa",
        },
        // The room set's grid has no measurement at 30 degrees: the speakers are at 45.
        left_az: 45.0,
        right_az: 315.0,
        licence: "Apache-2.0",
        credit: "SADIE II, University of York",
        source_page: "https://zenodo.org/records/12092466",
    },
];

#[cfg(test)]
mod tests;
