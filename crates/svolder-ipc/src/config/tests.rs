// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the configuration reads and writes, by topic. The helpers here are the ones several
//! topics use; the tests are in the topic files listed below.

use super::edit::{Slot, commented_key, comments_out, set_key_at, table_at_mut, write_atomically};
use super::template::template_stops;
use super::*;
use crate::test_support::temp_dir;
use svolder_profiles::EqMode;
use svolder_profiles::tone::preset::{NAME_MAX, PresetBand};
use toml_edit::{DocumentMut, Value};

// parsing the file
mod parse;
// device writers
mod devices;
// declared hops and their writers
mod hops;
// user presets and their writers
mod presets;
// imported presets and the AutoEq table
mod autoeq;
// the `[tui]` block
mod tui;
// saving `[tui]` settings
mod tui_writes;
// the `[mqtt]` block
mod mqtt;
// the `[mcp]` block
mod mcp;
// the `[web]` block
mod web;
// the template
mod template;
// the file's mode, locks and failed saves
mod writers;
// the config home and scratch homes
mod paths;

/// A device with nothing but a name and a udid.
fn device(name: &str, udid: &str) -> DeviceConfig {
    DeviceConfig {
        name: name.into(),
        udid: udid.into(),
        profile: None,
        passphrase: String::new(),
        inputs: BTreeMap::new(),
    }
}

/// A 5-band preset, "HD650 warm".
fn hd650_warm() -> Preset {
    Preset {
        name: "HD650 warm".into(),
        source: None,
        bands: 5,
        mode: EqMode::Eq,
        loudness: 0,
        band: vec![
            PresetBand {
                f: 40.0,
                g: 3.0,
                q: 0.7,
            },
            PresetBand {
                f: 160.0,
                g: 1.5,
                q: 0.7,
            },
            PresetBand {
                f: 632.0,
                g: 0.0,
                q: 4.3,
            },
            PresetBand {
                f: 2515.0,
                g: 0.0,
                q: 4.3,
            },
            PresetBand {
                f: 10000.0,
                g: 2.0,
                q: 0.7,
            },
        ],
    }
}

/// A well-formed `[[preset]]` block with 3 bands, as text, for a validation test to break
/// one field of.
fn three_band_block(name: &str) -> String {
    format!(
        "[[preset]]\nname = {name:?}\nbands = 3\nmode = \"eq\"\nloudness = 0\n\
             band = [\n  {{ f = 63, g = 1.0, q = 0.7 }},\n  {{ f = 630, g = 0.0, q = 4.3 }},\n  \
             {{ f = 6300, g = 0.0, q = 4.3 }},\n]\n"
    )
}

/// Where writers of one file meet: how many have read it, and a bell for the others.
type Meeting = std::sync::Arc<(std::sync::Mutex<usize>, std::sync::Condvar)>;

/// The files whose writers meet after reading, so a test can make two writers overlap.
static MEETINGS: std::sync::Mutex<Vec<(std::path::PathBuf, Meeting)>> =
    std::sync::Mutex::new(Vec::new());

/// Called by a writer once it has read the file under its lock. A writer of a file with a
/// meeting waits there for another to arrive, or a fifth of a second, whichever is first: a lock
/// that works lets only one in, so the wait ends on the clock, and no lock lets both in and the
/// wait ends on the second.
pub(super) fn after_read(path: &Path) {
    let meeting = MEETINGS
        .lock()
        .unwrap()
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, m)| std::sync::Arc::clone(m));
    let Some(meeting) = meeting else { return };
    let (count, bell) = &*meeting;
    let mut arrived = count.lock().unwrap();
    *arrived += 1;
    bell.notify_all();
    let _ = bell
        .wait_timeout_while(arrived, std::time::Duration::from_millis(200), |n| *n < 2)
        .unwrap();
}
