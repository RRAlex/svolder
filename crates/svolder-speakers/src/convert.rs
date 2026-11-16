// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Any source a set can come from, converted to the four prepared impulse responses.

use crate::error::ConvertError;
use crate::ir::{Angles, Irs, Report};
use crate::{sofa, wav};

/// The most bytes a source may be.
pub const MAX_SOURCE_BYTES: usize = wav::MAX_BYTES;

/// The channels of the common 14-channel 7.1 layout that hold the front speakers' paths: the
/// left speaker to the left and right ear, then the right speaker to the left and right ear.
const LAYOUT_14: [usize; 4] = [0, 1, 8, 7];

/// Which measurements of a SOFA file are the two speakers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recipe {
    /// The left speaker's azimuth in degrees (counter-clockwise from the front).
    pub left_az: f64,
    /// The right speaker's azimuth in degrees.
    pub right_az: f64,
    /// How far a measurement may be from either, in degrees.
    pub tolerance: f64,
}

impl Default for Recipe {
    /// The speakers at 30 and 330 degrees, a measurement within 10 degrees of each.
    fn default() -> Self {
        Recipe {
            left_az: 30.0,
            right_az: 330.0,
            tolerance: sofa::DEFAULT_TOLERANCE,
        }
    }
}

/// A source made into a set: the impulse responses that play and what they measure.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    /// The responses as they are written to the WAV.
    pub irs: Irs,
    /// What was done to them and what they measure.
    pub report: Report,
    /// Where the speakers' measurements were, for a SOFA source.
    pub angles: Option<Angles>,
}

impl Converted {
    /// The sample rate in Hz.
    pub fn rate(&self) -> u32 {
        self.irs.rate
    }

    /// The 4-channel float WAV the convolvers play.
    pub fn wav_bytes(&self) -> Vec<u8> {
        wav::write(self.irs.rate, &self.irs.ch)
    }
}

/// Convert the SOFA file or WAV in `bytes`: a SOFA file by `recipe`, a WAV of 4 channels as it
/// is and one of 14 by the 7.1 layout.
pub fn convert(bytes: &[u8], recipe: &Recipe) -> Result<Converted, ConvertError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(ConvertError::TooLarge);
    }
    if bytes.starts_with(b"\x89HDF") {
        let set = sofa::parse(bytes)?;
        let (irs, angles) = sofa::pick(&set, recipe.left_az, recipe.right_az, recipe.tolerance)?;
        return finish(irs, Some(angles));
    }
    if bytes.starts_with(b"RIFF") {
        let wav = wav::read_channels(bytes, &[4, 14])?;
        let channels = wav.channels.len();
        let ch = match channels {
            4 => [0, 1, 2, 3],
            14 => LAYOUT_14,
            other => return Err(ConvertError::Channels(other as u16)),
        }
        .map(|i| wav.channels[i].clone());
        return finish(Irs { rate: wav.rate, ch }, None);
    }
    Err(ConvertError::UnknownFormat)
}

/// Convert two 2-channel WAVs: the left speaker's (its left-ear and right-ear responses) and the
/// right speaker's.
pub fn convert_wav_pair(left: &[u8], right: &[u8]) -> Result<Converted, ConvertError> {
    if left.len() > MAX_SOURCE_BYTES || right.len() > MAX_SOURCE_BYTES {
        return Err(ConvertError::TooLarge);
    }
    let (left, right) = (
        wav::read_channels(left, &[2])?,
        wav::read_channels(right, &[2])?,
    );
    for wav in [&left, &right] {
        if wav.channels.len() != 2 {
            return Err(ConvertError::Channels(wav.channels.len() as u16));
        }
    }
    if left.rate != right.rate {
        return Err(ConvertError::BadWav(format!(
            "the two files differ in rate ({} and {} Hz)",
            left.rate, right.rate
        )));
    }
    let [ll, lr] = <[_; 2]>::try_from(left.channels).unwrap();
    let [rl, rr] = <[_; 2]>::try_from(right.channels).unwrap();
    finish(
        Irs {
            rate: left.rate,
            ch: [ll, lr, rl, rr],
        },
        None,
    )
}

fn finish(irs: Irs, angles: Option<Angles>) -> Result<Converted, ConvertError> {
    let (irs, report) = crate::ir::prepare(irs)?;
    Ok(Converted {
        irs,
        report,
        angles,
    })
}

#[cfg(test)]
mod tests;
