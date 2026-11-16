// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::wav;

fn dirac(at: usize, gain: f32) -> Vec<f32> {
    let mut v = vec![0.0; 400];
    v[at] = gain;
    v
}

fn four() -> Vec<u8> {
    wav::write(
        48000,
        &[
            dirac(10, 0.5),
            dirac(14, 0.25),
            dirac(20, 0.125),
            dirac(30, 0.75),
        ],
    )
}

fn store() -> (Store, std::path::PathBuf) {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "svolder-speakers-import-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    (Store::new(&dir), dir)
}

#[test]
fn an_own_wav_is_stored_with_its_numbers_and_an_own_id() {
    let (store, dir) = store();
    let bytes = four();
    let saved = import(&store, &bytes, "My room\n.wav", Room::With, 77, None).unwrap();
    let meta = &saved.meta;
    assert_eq!(meta.id, format!("own-{}", &sha256_hex(&bytes)[..8]));
    assert_eq!(meta.label, "My room.wav");
    assert!(meta.own && meta.room == Room::With);
    assert_eq!(meta.licence, OWN_LICENCE);
    assert_eq!(
        (meta.added, meta.used, meta.rate, meta.taps),
        (77, 77, 48000, 438)
    );
    assert!((meta.worst_gain_db + 0.1).abs() < 0.01);
    assert_eq!(meta.source_sha256, sha256_hex(&bytes));
    assert!(store.check(&meta.id).is_ok());
    // The same file again is the same set.
    let again = import(&store, &bytes, "x", Room::With, 90, None).unwrap();
    assert_eq!(again.meta.id, meta.id);
    assert_eq!(store.list().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_pair_of_wavs_is_stored_too() {
    let (store, dir) = store();
    let left = wav::write(48000, &[dirac(10, 0.5), dirac(14, 0.25)]);
    let right = wav::write(48000, &[dirac(20, 0.125), dirac(30, 0.75)]);
    let saved = import_pair(&store, &left, &right, "pair", Room::No, 1, None).unwrap();
    assert!(store.check(&saved.meta.id).is_ok());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refused_file_stores_nothing_and_says_why() {
    let (store, dir) = store();
    let two = wav::write(48000, &[dirac(1, 1.0), dirac(1, 1.0)]);
    let error = import(&store, &two, "x", Room::No, 1, None).unwrap_err();
    assert_eq!(error, ImportError::Convert(ConvertError::Channels(2)));
    assert!(!dir.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_label_is_printable_and_never_empty() {
    assert_eq!(clean_label("\u{1b}[31m  "), "[31m");
    assert_eq!(clean_label(""), "Own file");
    assert_eq!(clean_label(&"é".repeat(100)).chars().count(), 60);
}
