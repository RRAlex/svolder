// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the fake host's computer plays virtual speakers from: two synthetic sets, one for each
//! choice, in a store of the fake daemon's own under `$XDG_RUNTIME_DIR/svolder/fake-speakers`
//! (made when the daemon starts, removed when it stops; never the person's own store), kept as
//! the person's own sets under ids no catalogue has. The impulse responses are made here from a
//! few arrivals and a decaying ripple, never read from a dataset; they play nowhere (the fake
//! chain only records).

use svolder_speakers::ir::{Angles, Direction};
use svolder_speakers::{Room, SetMeta, Store};

/// The rate of the synthetic sets.
const RATE: u32 = 48_000;

/// The ids kept: the free-field set and the room set. No catalogue has them.
const IDS: [(&str, Room, &str); 2] = [
    ("fake-no-room", Room::No, "Synthetic, no room"),
    ("fake-with-room", Room::With, "Synthetic, with room"),
];

/// Where the fake daemon keeps its store: `fake-speakers` in the socket's own private directory
/// (`$XDG_RUNTIME_DIR`, else `/run/user/<uid>`, else `/tmp/svolder-<uid>`, which is refused when
/// somebody else owns it or others may write to it), never a shared path.
pub(crate) fn dir() -> std::io::Result<std::path::PathBuf> {
    Ok(svolder_ipc::socket::path::private_dir()?.join("fake-speakers"))
}

/// A fresh store of the fake daemon's own, with the two synthetic sets in it. Without a private
/// directory the store is one that cannot be made (nothing is written anywhere) and the fake
/// host serves no sets; the daemon says so in its log.
pub(crate) fn open() -> Store {
    match dir() {
        Ok(dir) => open_in(&dir),
        Err(why) => {
            crate::log::warn(&format!(
                "the fake host has no private directory for its synthetic virtual speakers: {why}"
            ));
            Store::new(std::path::Path::new("/dev/null/svolder-fake-speakers"))
        }
    }
}

/// A fresh store in `dir`, with the two synthetic sets in it.
pub(crate) fn open_in(dir: &std::path::Path) -> Store {
    let _ = std::fs::remove_dir_all(dir);
    let store = Store::new(dir);
    seed(&store);
    store
}

/// Remove the fake daemon's store.
pub(crate) fn remove() {
    if let Ok(dir) = dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// One impulse response: the direct arrival at `at` samples with `gain`, and with a room a tail
/// of a deterministic ripple that decays by 60 dB over `tail` samples.
fn response(at: usize, gain: f32, room: bool, seed: u32, len: usize) -> Vec<f32> {
    let mut out = vec![0.0; len];
    out[at] = gain;
    if room {
        let mut x = seed;
        for (i, slot) in out.iter_mut().enumerate().skip(at + 24) {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (x >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            let fade = 10f32.powf(-3.0 * (i - at) as f32 / (len - at) as f32);
            *slot += 0.06 * noise * fade;
        }
    }
    out
}

/// Save the two synthetic sets that the store does not hold yet.
pub(crate) fn seed(store: &Store) {
    for (id, room, label) in IDS {
        if store.check(id).is_ok() {
            continue;
        }
        let with = room == Room::With;
        let len = if with { 24_000 } else { 256 };
        let channels = [
            response(8, 0.7, with, 1, len),
            response(20, 0.35, with, 2, len),
            response(20, 0.35, with, 3, len),
            response(8, 0.7, with, 4, len),
        ];
        let wav = svolder_speakers::wav::write(RATE, &channels);
        let direction = |az_deg| Direction {
            az_deg,
            el_deg: 0.0,
            distance_m: 1.5,
        };
        let meta = SetMeta {
            id: id.to_string(),
            room,
            label: label.to_string(),
            licence: "LicenseRef-synthetic".to_string(),
            credit: String::new(),
            source_url: String::new(),
            source_sha256: "0".repeat(64),
            wav_sha256: String::new(),
            rate: RATE,
            taps: len,
            taps_ms: len as f64 / f64::from(RATE) * 1000.0,
            angles: Some(Angles {
                left: direction(30.0),
                right: direction(330.0),
            }),
            loudness_db: -7.0,
            scale_db: -3.0,
            worst_gain_db: -0.1,
            cut: with,
            own: true,
            added: 1,
            used: 1,
        };
        if let Err(why) = store.save(meta, &wav, None) {
            crate::log::warn(&format!(
                "the fake host's synthetic set {id} was not saved: {why}"
            ));
        }
    }
}
