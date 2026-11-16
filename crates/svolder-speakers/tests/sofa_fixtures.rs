// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The converter against the synthetic SOFA files in `tests/fixtures`, written by h5py
//! (`scripts/speakers/make-sofa-fixtures.py`; `make speakers-fixtures` regenerates them). The
//! committed files are what the tests read, so nothing here needs h5py.

use std::path::PathBuf;

use svolder_speakers::{ConvertError, Recipe, convert};

fn read(name: &str) -> Vec<u8> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::read(dir.join(name))
        .unwrap_or_else(|error| panic!("{name}: {error}; regenerate with `make speakers-fixtures`"))
}

fn at(taps: &[f32]) -> usize {
    taps.iter().position(|v| v.abs() > 0.01).unwrap()
}

/// The positions of the four paths' Diracs.
fn positions(name: &str, recipe: &Recipe) -> Vec<usize> {
    let converted = convert(&read(name), recipe).unwrap();
    converted.irs.ch.iter().map(|c| at(c)).collect()
}

#[test]
fn a_free_field_file_gives_the_four_diracs_at_their_samples_and_gains() {
    let converted = convert(&read("free-field.sofa"), &Recipe::default()).unwrap();
    assert_eq!(
        converted.irs.ch.iter().map(|c| at(c)).collect::<Vec<_>>(),
        [48, 52, 58, 68]
    );
    let gains: Vec<f32> = converted.irs.ch.iter().map(|c| c[at(c)]).collect();
    // 0.5 : 0.25 : 0.125 : 0.75, all scaled by one factor.
    assert!((gains[0] / gains[3] - 0.5 / 0.75).abs() < 1e-5);
    assert!((gains[1] / gains[2] - 2.0).abs() < 1e-5);
    assert!((converted.report.worst_gain_db + 0.1).abs() < 0.01);
    let angles = converted.angles.unwrap();
    assert_eq!((angles.left.az_deg, angles.right.az_deg), (30.0, 330.0));
    assert_eq!((angles.left.el_deg, angles.left.distance_m), (0.0, 1.5));
    assert_eq!(converted.rate(), 48000);
}

#[test]
fn the_files_own_rate_is_kept_and_cartesian_positions_are_read() {
    assert_eq!(
        convert(&read("free-field-44100.sofa"), &Recipe::default())
            .unwrap()
            .rate(),
        44100
    );
    assert_eq!(
        positions("cartesian.sofa", &Recipe::default()),
        [48, 52, 58, 68]
    );
}

#[test]
fn data_delay_shifts_by_whole_samples_per_receiver() {
    // Receiver 0 (left ear) is delayed 3 samples, receiver 1 7.
    assert_eq!(
        positions("delay.sofa", &Recipe::default()),
        [48, 56, 58, 72]
    );
}

#[test]
fn receivers_listed_right_ear_first_are_swapped() {
    assert_eq!(
        positions("swapped-receivers.sofa", &Recipe::default()),
        [52, 48, 68, 58]
    );
}

#[test]
fn a_room_file_is_read_at_the_angles_its_grid_has() {
    let wide = Recipe {
        left_az: 45.0,
        right_az: 315.0,
        tolerance: 1.0,
    };
    assert_eq!(positions("room.sofa", &wide), [48, 52, 58, 68]);
    assert!(matches!(
        convert(&read("room.sofa"), &Recipe::default()),
        Err(ConvertError::NoMeasurement { .. })
    ));
}

#[test]
fn files_that_are_not_impulse_responses_or_have_several_emitters_are_refused() {
    assert!(matches!(
        convert(&read("not-fir.sofa"), &Recipe::default()),
        Err(ConvertError::NotFir(_))
    ));
    assert!(matches!(
        convert(&read("mimo.sofa"), &Recipe::default()),
        Err(ConvertError::Layout(_))
    ));
}
