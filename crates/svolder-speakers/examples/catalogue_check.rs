// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fetch every catalogue entry from its real URL into a scratch store, and check its size, its
//! SHA-256 and that the converter accepts it (`make speakers-catalogue-check`; network).
//!
//! Usage: catalogue_check SCRATCH_DIR [ID...]
//!
//! A base address other than the catalogue's own (a mirror, a test server) comes from the
//! `CATALOGUE_BASE` environment variable.

use svolder_speakers::{Cancel, Client, Store, entries};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: catalogue_check SCRATCH_DIR [ID...]");
        std::process::exit(2);
    };
    let only: Vec<String> = args.collect();
    let store = Store::new(std::path::Path::new(&dir));
    let client = Client::for_transport("", std::env::var("CATALOGUE_BASE").ok());
    let mut failed = 0;
    for entry in entries()
        .iter()
        .filter(|e| only.is_empty() || only.iter().any(|id| id == e.id))
    {
        match client.fetch(&store, entry.id, 0, None, &Cancel::new(), &mut |_, _| {}) {
            Ok(saved) => {
                let meta = saved.meta;
                println!(
                    "ok   {:<14} {} taps at {} Hz, {:.1} ms, {:.1} dB, {}",
                    entry.id,
                    meta.taps,
                    meta.rate,
                    meta.taps_ms,
                    meta.loudness_db,
                    meta.angles.map_or(String::new(), |a| format!(
                        "left {:.1} right {:.1} deg at {:.2} m",
                        a.left.az_deg, a.right.az_deg, a.left.distance_m
                    )),
                );
                let _ = store.forget(entry.id);
            }
            Err(error) => {
                failed += 1;
                println!("FAIL {:<14} {} ({})", entry.id, error, error.kind());
            }
        }
    }
    std::process::exit(i32::from(failed > 0));
}
