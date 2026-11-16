// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! WAV files: the writer and reader of the 4-channel float file the convolvers play, and the
//! reader of the PCM and float WAVs a person imports.

use crate::error::ConvertError;

/// The most bytes a WAV (or any source) may be.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// A WAV's samples, one `Vec` per channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Wav {
    /// The sample rate in Hz.
    pub rate: u32,
    /// The samples of each channel, all of one length.
    pub channels: Vec<Vec<f32>>,
}

/// `channels` as a 32-bit float WAV at `rate` Hz.
pub fn write(rate: u32, channels: &[Vec<f32>]) -> Vec<u8> {
    let count = channels.len();
    let frames = channels.first().map_or(0, Vec::len);
    let align = (count * 4) as u32;
    let data_len = frames as u32 * align;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend(b"RIFF");
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(3u16.to_le_bytes());
    out.extend((count as u16).to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * align).to_le_bytes());
    out.extend((align as u16).to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    for frame in 0..frames {
        for channel in channels {
            out.extend(channel[frame].to_le_bytes());
        }
    }
    out
}

/// The samples of a WAV: PCM of 8, 16, 24 or 32 bits or float of 32 or 64 bits, with the plain
/// or the extensible header.
pub fn read(bytes: &[u8]) -> Result<Wav, ConvertError> {
    read_channels(bytes, &[])
}

/// [`read`], refusing a channel count not in `accept` (any count when it is empty) before a
/// sample is decoded.
pub fn read_channels(bytes: &[u8], accept: &[u16]) -> Result<Wav, ConvertError> {
    let bad = |why: &str| ConvertError::BadWav(why.to_string());
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("it has no RIFF/WAVE header"));
    }
    let mut format: Option<Format> = None;
    let mut data: Option<&[u8]> = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = at + 8;
        if id == b"data" {
            let end = body.checked_add(len).filter(|end| *end <= bytes.len());
            let Some(end) = end else {
                return Err(bad("the sample data is truncated"));
            };
            data = Some(&bytes[body..end]);
            break;
        }
        let end = body.saturating_add(len).min(bytes.len());
        if id == b"fmt " {
            format = Some(
                Format::parse(&bytes[body..end]).ok_or_else(|| bad("the fmt chunk is short"))?,
            );
        }
        at = body.saturating_add(len).saturating_add(len & 1);
    }
    let format = format.ok_or_else(|| bad("it has no fmt chunk"))?;
    let data = data.ok_or_else(|| bad("it has no data chunk"))?;
    if format.channels == 0 {
        return Err(bad("it has no channels"));
    }
    if !accept.is_empty() && !accept.contains(&format.channels) {
        return Err(ConvertError::Channels(format.channels));
    }
    let width = match (format.float, format.bits) {
        (false, 8 | 16 | 24 | 32) | (true, 32 | 64) => usize::from(format.bits / 8),
        _ => {
            return Err(bad(&format!(
                "the sample format (tag {}, {} bits) is not supported",
                format.tag, format.bits
            )));
        }
    };
    let count = usize::from(format.channels);
    let frames = data.len() / (width * count);
    let mut channels = vec![Vec::with_capacity(frames); count];
    for (i, sample) in data.chunks_exact(width).take(frames * count).enumerate() {
        channels[i % count].push(format.sample(sample));
    }
    Ok(Wav {
        rate: format.rate,
        channels,
    })
}

/// The fmt chunk's fields that matter.
struct Format {
    tag: u16,
    float: bool,
    channels: u16,
    rate: u32,
    bits: u16,
}

impl Format {
    fn parse(body: &[u8]) -> Option<Format> {
        if body.len() < 16 {
            return None;
        }
        let u16_at = |i: usize| u16::from_le_bytes([body[i], body[i + 1]]);
        let mut tag = u16_at(0);
        if tag == 0xFFFE && body.len() >= 26 {
            tag = u16_at(24);
        }
        Some(Format {
            tag,
            // Anything but PCM (1) and float (3) reads as a format the sample reader refuses.
            float: tag == 3,
            channels: u16_at(2),
            rate: u32::from_le_bytes(body[4..8].try_into().ok()?),
            bits: match tag {
                1 | 3 => u16_at(14),
                _ => 0,
            },
        })
    }

    fn sample(&self, raw: &[u8]) -> f32 {
        match (self.float, self.bits) {
            (true, 32) => f32::from_le_bytes(raw.try_into().unwrap()),
            (true, _) => f64::from_le_bytes(raw.try_into().unwrap()) as f32,
            (false, 8) => (f32::from(raw[0]) - 128.0) / 128.0,
            (false, 16) => f32::from(i16::from_le_bytes([raw[0], raw[1]])) / 32768.0,
            (false, 24) => {
                let value = i32::from_le_bytes([0, raw[0], raw[1], raw[2]]) >> 8;
                value as f32 / 8_388_608.0
            }
            (false, _) => {
                (f64::from(i32::from_le_bytes(raw.try_into().unwrap())) / 2_147_483_648.0) as f32
            }
        }
    }
}

#[cfg(test)]
mod tests;
