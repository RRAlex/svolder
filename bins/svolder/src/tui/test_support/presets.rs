// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fixtures for the tests of the preset box and every screen that opens it: an app on a tone
//! control resting flat, presets of a known shape, an AutoEq index of two headphones, and the
//! helpers that echo a load back the way the device does. They sit apart from the TUI's other
//! shared helpers because only the preset-related tests use them.

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::json;
use svolder_autoeq::Index;
use svolder_ipc::{TuiConfig, ValueObject};
use svolder_profiles::tone::preset::{self, Preset, PresetBand};
use svolder_profiles::{EqMode, EqParam, EqSpec};
use svolder_proto::FunctionId;

use crate::backend::Update;
use crate::tui::app::{App, Intent, Screen};
use crate::tui::boxes::presets::{Step, entries};
use crate::tui::test_support::{press_from as press, profile_named};

/// One reading of a function no control names, as a backend hands it over.
fn reading(id: FunctionId, raw: i64) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.to_string(),
        key: None,
        label: "?".into(),
        value_type: "u8".into(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    }
}

/// Report one reading.
pub(in crate::tui) fn hear_id(app: &mut App, id: FunctionId, raw: i64) {
    app.apply(Update::Status(reading(id, raw)), Instant::now());
}

/// The tone control of the profile `app` is on.
pub(in crate::tui) fn eq_of(app: &App) -> EqSpec {
    app.profile
        .eq
        .clone()
        .expect("the profile has a tone control")
}

/// Report the device's whole tone control as it rests: every band on its own centre at 0 dB
/// with the centre Q, the loudness strength at 0, and the mode `mode`.
pub(in crate::tui) fn hear_flat(app: &mut App, mode: EqMode) {
    let eq = eq_of(app);
    hear_id(app, eq.mode_id(), i64::from(mode as u8));
    hear_id(app, eq.loudness_id(), 0);
    for band in 0..eq.bands.len() {
        for (param, raw) in [
            (EqParam::Frequency, 64),
            (EqParam::Gain, i64::from(eq.gain_zero)),
            (EqParam::Q, 64),
        ] {
            let id = eq.band_id(band, param).expect("a band of the tone control");
            hear_id(app, id, raw);
        }
    }
}

/// An app on the built-in `name` profile, on its tone-control screen, with the device's
/// tone control resting flat in `mode`.
pub(in crate::tui) fn resting(name: &str, mode: EqMode) -> App {
    let mut app = App::new(
        profile_named(name),
        name.into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.show(Screen::Tone);
    hear_flat(&mut app, mode);
    app
}

/// A preset of the user's own for `bands` bands, every band at 1 kHz and `g` dB.
pub(in crate::tui) fn own(name: &str, bands: u8, g: f64) -> Preset {
    Preset {
        name: name.into(),
        source: None,
        bands,
        mode: EqMode::Eq,
        loudness: 0,
        band: vec![
            PresetBand {
                f: 1000.0,
                g,
                q: 1.0
            };
            usize::from(bands)
        ],
    }
}

/// The names the list offers `app`, in order.
pub(in crate::tui) fn names(app: &App) -> Vec<String> {
    entries(app, &eq_of(app))
        .into_iter()
        .map(|entry| entry.preset.name)
        .collect()
}

/// Open the list and focus the preset called `name`.
pub(in crate::tui) fn focus(app: &mut App, name: &str) {
    press(app, KeyCode::Char('p'));
    let at = names(app)
        .iter()
        .position(|listed| listed == name)
        .unwrap_or_else(|| panic!("the list offers no {name}"));
    let row = app
        .presets
        .preset_list
        .as_ref()
        .expect("the list is open")
        .row;
    for _ in row..at {
        press(app, KeyCode::Down);
    }
    for _ in at..row {
        press(app, KeyCode::Up);
    }
    assert_eq!(
        app.presets.preset_list.as_ref().map(|list| list.row),
        Some(at)
    );
}

/// What the list is showing, while it is open.
pub(in crate::tui) fn step(app: &App) -> Option<&Step> {
    app.presets.preset_list.as_ref().map(|list| &list.step)
}

/// The (band, parameter) of every band write among `intents`, in order.
pub(in crate::tui) fn band_params(intents: &[Intent]) -> Vec<(usize, &str)> {
    intents
        .iter()
        .filter_map(|intent| match intent {
            Intent::EqSet { band, param, .. } => Some((*band, param.as_str())),
            _ => None,
        })
        .collect()
}

/// The device carrying `intents` out in order and echoing each write at the step its value
/// converts to: a band parameter, the mode, and the loudness strength set by key or
/// straight to its function.
pub(in crate::tui) fn carried_out(app: &mut App, intents: &[Intent]) {
    let eq = eq_of(app);
    for intent in intents {
        let (id, raw) = match intent {
            Intent::EqSet { band, param, value } => {
                let param = param.parse::<EqParam>().expect("a band parameter");
                let step = eq
                    .step(band - 1, param, *value)
                    .expect("a step of the band");
                (eq.band_id(band - 1, param).expect("a band"), step)
            }
            Intent::EqMode(mode) => (
                eq.mode_id(),
                mode.parse::<EqMode>().expect("a mode's name") as u8,
            ),
            Intent::Set { value, .. } | Intent::Raw { value, .. } => {
                (eq.loudness_id(), value.parse().expect("a strength"))
            }
            Intent::Recalled(_) => continue,
            other => panic!("the device was asked for {other:?}"),
        };
        hear_id(app, id, i64::from(raw));
    }
}

/// An imported preset named `name`: a bass shelf and a peak.
pub(in crate::tui) fn imported(name: &str) -> Preset {
    use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
    Preset {
        name: name.into(),
        source: Some(preset::Imported {
            origin: None,
            correction: Correction {
                preamp_db: -3.0,
                filters: vec![
                    SourceFilter {
                        kind: FilterKind::LowShelf,
                        f: 105.0,
                        g: 5.0,
                        q: 0.7,
                    },
                    SourceFilter {
                        kind: FilterKind::Peaking,
                        f: 3000.0,
                        g: -3.0,
                        q: 2.0,
                    },
                ],
            },
        }),
        bands: 0,
        mode: EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
    }
}

/// A correction file as AutoEq writes it: a preamp and two filters.
pub(in crate::tui) const PEQ: &str = "Preamp: -3.0 dB\n\
    Filter 1: ON LSC Fc 105 Hz Gain 5.0 dB Q 0.70\n\
    Filter 2: ON PK Fc 3000 Hz Gain -3.0 dB Q 2.00\n";

/// An AutoEq index of these index lines, fetched on 2026-07-01.
pub(in crate::tui) fn index_of(lines: &[&str]) -> Index {
    Index {
        entries: svolder_autoeq::parse_index(&lines.join("\n")),
        fetched: std::time::UNIX_EPOCH + Duration::from_secs(1_782_864_000),
        note: None,
    }
}

/// Two headphones, the first measured by two sources.
pub(in crate::tui) fn two_headphones() -> Index {
    index_of(&[
        "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990",
        "- [Sennheiser HD 650](./crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650) by crinacle on GRAS 43AG-7",
        "- [Sennheiser HD 600](./oratory1990/over-ear/Sennheiser%20HD%20600) by oratory1990",
    ])
}

/// The list, on the tone-control screen, open on the search box with the index already held.
pub(in crate::tui) fn searching(index: Index) -> App {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.autoeq = Some(std::sync::Arc::new(index));
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('a'));
    app
}
