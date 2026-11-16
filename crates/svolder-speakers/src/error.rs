// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a source could not become a set, in words a person can act on.

/// A source the converter refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ConvertError {
    /// The bytes are neither a SOFA file nor a WAV.
    #[error("this is neither a SOFA file nor a WAV file")]
    UnknownFormat,
    /// The source is bigger than [`crate::MAX_SOURCE_BYTES`].
    #[error("the file is larger than 64 MiB")]
    TooLarge,
    /// A SOFA file that cannot be read.
    #[error("this SOFA file cannot be read: {0}")]
    NotSofa(String),
    /// A SOFA file that is not a set of impulse responses.
    #[error("this SOFA file holds {0}, not impulse responses (FIR)")]
    NotFir(String),
    /// A SOFA file whose layout the converter does not read.
    #[error("this SOFA file's layout is not supported: {0}")]
    Layout(String),
    /// No measurement near enough to a speaker's angle.
    #[error(
        "no measurement within {tolerance:.0} degrees of {wanted:.0} degrees (the nearest is at \
         {found:.1} degrees)"
    )]
    NoMeasurement {
        /// The azimuth asked for.
        wanted: f64,
        /// The azimuth of the nearest measurement.
        found: f64,
        /// How far a measurement may be from it.
        tolerance: f64,
    },
    /// A WAV that cannot be read.
    #[error("this WAV file cannot be read: {0}")]
    BadWav(String),
    /// A WAV with a channel count the converter has no layout for.
    #[error("a WAV of {0} channels has no known speaker layout (4 channels, or 14 channels)")]
    Channels(u16),
    /// A rate outside what a filter chain runs at.
    #[error("the sample rate {0} Hz is not supported")]
    Rate(u32),
    /// A sample that is not a finite number.
    #[error("the responses hold values that are not numbers")]
    NotNumbers,
    /// Every impulse response is silent.
    #[error("the impulse responses are silent")]
    Silent,
}
