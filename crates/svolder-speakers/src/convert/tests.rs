// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn dirac(n: usize, at: usize, gain: f32) -> Vec<f32> {
    let mut v = vec![0.0; n];
    v[at] = gain;
    v
}

fn position(taps: &[f32]) -> usize {
    taps.iter().position(|v| v.abs() > 0.01).unwrap()
}

#[test]
fn a_four_channel_wav_is_the_four_paths_as_they_are() {
    let channels = vec![
        dirac(2000, 200, 0.5),
        dirac(2000, 204, 0.25),
        dirac(2000, 210, 0.125),
        dirac(2000, 220, 0.75),
    ];
    let converted = convert(&wav::write(48000, &channels), &Recipe::default()).unwrap();
    // The earliest onset is 200: 152 samples are dropped from all four, a millisecond stays.
    assert_eq!(converted.report.trimmed, 152);
    let at: Vec<usize> = converted.irs.ch.iter().map(|c| position(c)).collect();
    assert_eq!(at, [48, 52, 58, 68]);
    // The gains keep their ratios (0.5 : 0.25 : 0.125 : 0.75).
    let gains: Vec<f32> = converted.irs.ch.iter().map(|c| c[position(c)]).collect();
    assert!((gains[0] / gains[3] - 0.5 / 0.75).abs() < 1e-5);
    assert!((gains[1] / gains[2] - 2.0).abs() < 1e-5);
    assert_eq!(converted.angles, None);
    assert_eq!(converted.rate(), 48000);
}

#[test]
fn a_fourteen_channel_wav_takes_channels_0_1_8_and_7() {
    let mut channels: Vec<Vec<f32>> = (0..14).map(|_| dirac(400, 3, 9.0)).collect();
    channels[0] = dirac(400, 10, 0.5);
    channels[1] = dirac(400, 14, 0.25);
    channels[8] = dirac(400, 20, 0.125);
    channels[7] = dirac(400, 30, 0.75);
    let converted = convert(&wav::write(44100, &channels), &Recipe::default()).unwrap();
    let at: Vec<usize> = converted.irs.ch.iter().map(|c| position(c)).collect();
    assert_eq!(at, [44, 48, 54, 64], "1 ms is 44 samples at 44.1 kHz");
    assert_eq!(converted.rate(), 44100);
}

#[test]
fn a_pair_of_stereo_wavs_is_the_left_speaker_then_the_right() {
    let left = wav::write(48000, &[dirac(300, 10, 0.5), dirac(300, 14, 0.25)]);
    let right = wav::write(48000, &[dirac(300, 20, 0.125), dirac(300, 30, 0.75)]);
    let converted = convert_wav_pair(&left, &right).unwrap();
    let at: Vec<usize> = converted.irs.ch.iter().map(|c| position(c)).collect();
    assert_eq!(at, [48, 52, 58, 68]);
    let other_rate = wav::write(44100, &[dirac(300, 20, 0.125), dirac(300, 30, 0.75)]);
    assert!(
        matches!(convert_wav_pair(&left, &other_rate), Err(ConvertError::BadWav(w)) if w.contains("rate"))
    );
    let mono = wav::write(48000, &[dirac(300, 1, 1.0)]);
    assert_eq!(
        convert_wav_pair(&left, &mono),
        Err(ConvertError::Channels(1))
    );
}

#[test]
fn the_written_wav_reads_back_as_the_four_prepared_responses() {
    let channels = vec![
        dirac(500, 5, 0.5),
        dirac(500, 6, 0.25),
        dirac(500, 7, 0.1),
        dirac(500, 8, 0.9),
    ];
    let converted = convert(&wav::write(48000, &channels), &Recipe::default()).unwrap();
    let back = wav::read(&converted.wav_bytes()).unwrap();
    assert_eq!(back.rate, 48000);
    assert_eq!(back.channels, converted.irs.ch.to_vec());
}

#[test]
fn a_pcm_wav_converts_too() {
    // 4 channels of 16-bit PCM: one frame with 0.5 in each channel, then silence.
    let mut data = Vec::new();
    for frame in 0..10 {
        for _ in 0..4 {
            data.extend((if frame == 2 { 16384i16 } else { 0 }).to_le_bytes());
        }
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data.len() as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    for field in [
        16u32.to_le_bytes().to_vec(),
        1u16.to_le_bytes().to_vec(),
        4u16.to_le_bytes().to_vec(),
    ] {
        bytes.extend(field);
    }
    bytes.extend(48000u32.to_le_bytes());
    bytes.extend((48000u32 * 8).to_le_bytes());
    bytes.extend(8u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((data.len() as u32).to_le_bytes());
    bytes.extend(&data);
    let converted = convert(&bytes, &Recipe::default()).unwrap();
    assert_eq!(
        converted
            .irs
            .ch
            .iter()
            .map(|c| position(c))
            .collect::<Vec<_>>(),
        [48, 48, 48, 48]
    );
}

#[test]
fn refusals_are_in_words() {
    let two = wav::write(48000, &[dirac(100, 1, 1.0), dirac(100, 1, 1.0)]);
    let error = convert(&two, &Recipe::default()).unwrap_err();
    assert_eq!(error, ConvertError::Channels(2));
    assert!(error.to_string().contains("4 channels, or 14 channels"));
    let six: Vec<Vec<f32>> = (0..6).map(|_| dirac(100, 1, 1.0)).collect();
    assert_eq!(
        convert(&wav::write(48000, &six), &Recipe::default()),
        Err(ConvertError::Channels(6))
    );
    assert_eq!(
        convert(b"GIF89a....", &Recipe::default()),
        Err(ConvertError::UnknownFormat)
    );
    assert_eq!(
        convert(b"", &Recipe::default()),
        Err(ConvertError::UnknownFormat)
    );
    let silent: Vec<Vec<f32>> = (0..4).map(|_| vec![0.0; 100]).collect();
    assert_eq!(
        convert(&wav::write(48000, &silent), &Recipe::default()),
        Err(ConvertError::Silent)
    );
    let slow: Vec<Vec<f32>> = (0..4).map(|_| dirac(100, 1, 1.0)).collect();
    assert_eq!(
        convert(&wav::write(4000, &slow), &Recipe::default()),
        Err(ConvertError::Rate(4000))
    );
}

#[test]
fn a_source_over_64_mib_is_refused_before_it_is_read() {
    let big = vec![0u8; MAX_SOURCE_BYTES + 1];
    assert_eq!(
        convert(&big, &Recipe::default()),
        Err(ConvertError::TooLarge)
    );
    assert_eq!(convert_wav_pair(&big, &big), Err(ConvertError::TooLarge));
}

#[test]
fn an_hdf5_file_that_is_not_sofa_is_refused_as_one() {
    let mut bytes = b"\x89HDF\r\n\x1a\n".to_vec();
    bytes.extend([0u8; 64]);
    assert!(matches!(
        convert(&bytes, &Recipe::default()),
        Err(ConvertError::NotSofa(_))
    ));
}
