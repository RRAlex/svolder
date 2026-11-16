// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn header(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
    let align = channels * bits / 8;
    let mut out = b"RIFF".to_vec();
    out.extend((36 + data.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(tag.to_le_bytes());
    out.extend(channels.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * u32::from(align)).to_le_bytes());
    out.extend(align.to_le_bytes());
    out.extend(bits.to_le_bytes());
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    out
}

#[test]
fn what_is_written_reads_back_exactly() {
    let channels = vec![vec![0.5, -0.25, 0.0], vec![1.0, 0.125, -1.0]];
    let bytes = write(48000, &channels);
    assert_eq!(
        read(&bytes),
        Ok(Wav {
            rate: 48000,
            channels
        })
    );
}

#[test]
fn the_written_header_is_the_plain_float_one() {
    let bytes = write(
        44100,
        &[vec![0.5, 0.25], vec![0.0, 0.0], vec![0.0; 2], vec![1.0; 2]],
    );
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 3, "IEEE float");
    assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 4, "channels");
    assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 44100);
    assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 32, "bits");
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(
        u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
        2 * 4 * 4
    );
    assert_eq!(bytes.len(), 44 + 2 * 4 * 4);
    // Frame-interleaved: the first frame is channel 0..3 at sample 0.
    let first: Vec<f32> = (0..4)
        .map(|c| f32::from_le_bytes(bytes[44 + 4 * c..48 + 4 * c].try_into().unwrap()))
        .collect();
    assert_eq!(first, [0.5, 0.0, 0.0, 1.0]);
}

#[test]
fn pcm_16_24_and_32_bits_read_scaled() {
    let pcm16 = header(1, 1, 8000, 16, &[0x00, 0x40, 0x00, 0xC0]);
    assert_eq!(read(&pcm16).unwrap().channels, vec![vec![0.5, -0.5]]);
    let pcm24 = header(1, 1, 8000, 24, &[0x00, 0x00, 0x40, 0x00, 0x00, 0xC0]);
    assert_eq!(read(&pcm24).unwrap().channels, vec![vec![0.5, -0.5]]);
    let pcm32 = header(1, 1, 8000, 32, &[0, 0, 0, 0x40, 0, 0, 0, 0xC0]);
    assert_eq!(read(&pcm32).unwrap().channels, vec![vec![0.5, -0.5]]);
}

#[test]
fn float_64_bits_reads() {
    let mut data = 0.25f64.to_le_bytes().to_vec();
    data.extend((-0.5f64).to_le_bytes());
    let wav = read(&header(3, 2, 8000, 64, &data)).unwrap();
    assert_eq!(wav.channels, vec![vec![0.25], vec![-0.5]]);
}

#[test]
fn an_extensible_header_reads() {
    let data: Vec<u8> = [0.5f32, 0.25]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let mut out = b"RIFF".to_vec();
    out.extend((60 + data.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(40u32.to_le_bytes());
    out.extend(0xFFFEu16.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(8000u32.to_le_bytes());
    out.extend(64000u32.to_le_bytes());
    out.extend(8u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend(22u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend(3u32.to_le_bytes());
    out.extend(3u16.to_le_bytes());
    out.extend([0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(&data);
    assert_eq!(read(&out).unwrap().channels, vec![vec![0.5], vec![0.25]]);
}

#[test]
fn other_chunks_are_skipped_and_odd_ones_padded() {
    let plain = header(3, 1, 8000, 32, &0.5f32.to_le_bytes());
    let mut out = plain[..36].to_vec();
    out.extend(b"LIST");
    out.extend(3u32.to_le_bytes());
    out.extend([1, 2, 3, 0]);
    out.extend(&plain[36..]);
    assert_eq!(read(&out).unwrap().channels, vec![vec![0.5]]);
}

#[test]
fn refusals_say_what_is_wrong() {
    assert!(matches!(read(b"nope"), Err(ConvertError::BadWav(_))));
    let mut truncated = header(3, 1, 8000, 32, &[0; 16]);
    truncated.truncate(truncated.len() - 4);
    assert!(matches!(read(&truncated), Err(ConvertError::BadWav(w)) if w.contains("truncated")));
    let adpcm = header(2, 1, 8000, 4, &[0; 4]);
    assert!(matches!(read(&adpcm), Err(ConvertError::BadWav(w)) if w.contains("format")));
    let no_data = header(3, 1, 8000, 32, &[])[..36].to_vec();
    assert!(matches!(read(&no_data), Err(ConvertError::BadWav(_))));
    let zero_channels = header(3, 0, 8000, 32, &[]);
    assert!(matches!(read(&zero_channels), Err(ConvertError::BadWav(_))));
}

#[test]
fn a_channel_count_not_accepted_is_refused_before_any_sample_is_decoded() {
    // 1000 channels of 8-bit samples: refused from the header, the data never decoded.
    let wav = header(1, 1000, 8000, 8, &vec![0u8; 1000 * 4]);
    assert_eq!(
        read_channels(&wav, &[4, 14]),
        Err(ConvertError::Channels(1000))
    );
    assert!(
        read_channels(&wav, &[]).is_ok(),
        "an empty list accepts any count"
    );
    let four = write(
        48000,
        &[vec![0.0; 3], vec![0.0; 3], vec![0.0; 3], vec![0.0; 3]],
    );
    assert!(read_channels(&four, &[4, 14]).is_ok());
    assert_eq!(read_channels(&four, &[2]), Err(ConvertError::Channels(4)));
}
