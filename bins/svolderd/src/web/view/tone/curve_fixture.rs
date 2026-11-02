// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The numbers the page's `tone-curve.js` is held to: the core's own curves, peaks, peak words
//! and marker points for a spread of band sets on the Gungnir 2's tone control, and the viewBox
//! maps. The test compares them with `scripts/web-checks/node/fixtures/tone-curve.json`, which
//! the node harness `tone-curve.mjs` checks the script against, and fails when the file is stale.
//! `SVOLDER_WRITE_FIXTURES=1 cargo test -p svolderd curve_fixture` rewrites it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Value, json};
use svolder_profiles::tone::response::{self, PEAK_SHOWN_DB};
use svolder_profiles::{EqMode, EqParam, EqSpec};
use svolder_proto::FunctionId;

use super::{POINTS, marker_db, own, peak_words, y};
use crate::test_support::profile_named;

/// How far apart a number in the file and a number now may be.
const TOLERANCE: f64 = 1e-9;

/// The file's two licence lines, as keys: a JSON file has no comment, and the licence check
/// reads the first three lines.
const LICENCE: [&str; 2] = [
    "SPDX-FileCopyrightText: 2026 the svolder authors",
    "SPDX-License-Identifier: GPL-3.0-or-later",
];

/// x of frequency `hz` in the 1000-wide viewBox the page draws the curve in.
fn x(hz: f64) -> f64 {
    1000.0 * (hz / response::F_MIN).ln() / (response::F_MAX / response::F_MIN).ln()
}

/// `M x y L x y …` through `ys` at the curve's frequencies.
fn path(freqs: &[f64], ys: &[f64]) -> String {
    let mut out = String::new();
    for (i, (f, y)) in freqs.iter().zip(ys).enumerate() {
        out.push_str(&format!(
            "{}{:.1} {:.1}",
            if i == 0 { "M" } else { "L" },
            x(*f),
            y
        ));
    }
    out
}

/// One band set to put the curve maths through: the mode read and each band's raw frequency,
/// gain and Q steps.
struct Case {
    name: String,
    mode: Option<EqMode>,
    steps: Vec<[u8; 3]>,
}

/// Every band at its centre frequency, 0 dB and the middle Q.
fn at_rest(eq: &EqSpec) -> Vec<[u8; 3]> {
    vec![[64, eq.gain_zero, 64]; eq.bands.len()]
}

/// A case in EQ mode that starts at rest and lets `change` move bands.
fn eq_case(eq: &EqSpec, name: &str, change: impl FnOnce(&mut Vec<[u8; 3]>)) -> Case {
    let mut steps = at_rest(eq);
    change(&mut steps);
    Case {
        name: name.into(),
        mode: Some(EqMode::Eq),
        steps,
    }
}

/// The cases: flat, a boost, a cut, overlaps, the edges of every band's window, the gain and Q
/// ranges, the peaks either side of what is worth a warning, and the other modes.
fn cases(eq: &EqSpec) -> Vec<Case> {
    let top = EqSpec::STEP_MAX;
    let boost = top;
    let mut cases = vec![
        eq_case(eq, "flat", |_| {}),
        eq_case(eq, "one boost", |s| s[2] = [64, eq.gain_raw(4.5), 64]),
        eq_case(eq, "one cut", |s| s[2] = [64, eq.gain_raw(-9.0), 64]),
        eq_case(eq, "two overlapping boosts", |s| {
            s[1] = [top, boost, 64];
            s[2] = [0, boost, 64];
        }),
        eq_case(eq, "boost and cut overlapping", |s| {
            s[1] = [top, boost, 64];
            s[2] = [0, 0, 64];
        }),
        eq_case(eq, "gain -20", |s| s[3] = [64, 0, 64]),
        eq_case(eq, "gain +6", |s| s[3] = [64, boost, 64]),
        eq_case(eq, "q 0.5", |s| s[2] = [64, boost, 0]),
        eq_case(eq, "q 8", |s| s[2] = [64, boost, top]),
        // The highest sample lies in the wide band, the narrow one peaks between two samples: only
        // the refinement round each band's centre finds that height.
        eq_case(eq, "wide boost and a narrow one between samples", |s| {
            s[0] = [64, boost, 0];
            s[4] = [3, boost, top];
        }),
        // A low-Q boost and a cut of the band above it, of opposite sign: the sum peaks about
        // half an octave below the boost's centre, outside every band's own half-octave window,
        // so only the refinement either side of the highest sample finds its true height.
        eq_case(eq, "peak away from every band centre", |s| {
            s[1] = [116, boost, 0];
            s[2] = [0, 72, 20];
        }),
        eq_case(eq, "tiny peak", |s| {
            s[1] = [top, 102, 0];
            s[2] = [0, 94, 0];
        }),
        eq_case(eq, "peak just over", |s| {
            s[1] = [top, 101, top];
            s[2] = [0, 95, 64];
        }),
    ];
    for band in 0..eq.bands.len() {
        for (edge, f) in [("low", 0), ("high", top)] {
            cases.push(eq_case(
                eq,
                &format!("band {} {edge} edge", band + 1),
                |s| s[band] = [f, boost, 40],
            ));
        }
    }
    let shaped = |mode: Option<EqMode>| {
        let mut steps = at_rest(eq);
        steps[1] = [100, eq.gain_raw(-6.0), 90];
        steps[2] = [50, boost, 30];
        steps[4] = [64, eq.gain_raw(3.0), 64];
        Case {
            name: format!(
                "{} mode with shaped bands",
                mode.map_or("unread", EqMode::name)
            ),
            mode,
            steps,
        }
    };
    cases.push(shaped(Some(EqMode::Flat)));
    cases.push(shaped(Some(EqMode::Loudness)));
    cases.push(shaped(None));
    cases
}

/// The raw readings a case stands for, by function id.
fn readings(eq: &EqSpec, case: &Case) -> BTreeMap<FunctionId, i64> {
    let mut raw = BTreeMap::new();
    if let Some(mode) = case.mode {
        raw.insert(eq.mode_id(), i64::from(mode as u8));
    }
    for (band, steps) in case.steps.iter().enumerate() {
        for (param, step) in [EqParam::Frequency, EqParam::Gain, EqParam::Q]
            .into_iter()
            .zip(steps)
        {
            raw.insert(eq.band_id(band, param).expect("a band"), i64::from(*step));
        }
    }
    raw
}

/// What the core says about `case`, as JSON.
fn case_json(eq: &EqSpec, freqs: &[f64], case: &Case) -> Value {
    let raw = readings(eq, case);
    let read = |id: FunctionId| raw.get(&id).copied();
    let bands = response::bands_of(eq, read).expect("every band is read");
    let ys: Vec<f64> = response::response_db(&bands, freqs)
        .into_iter()
        .map(y)
        .collect();
    let peak = response::clipping_peak(eq, read);
    let path = path(freqs, &ys);
    let markers: Vec<Value> = (0..eq.bands.len())
        .map(|band| {
            let (hz, gain) = own(eq, read, band).expect("a band read in full");
            json!({ "x": x(hz), "y": y(marker_db(case.mode, gain)) })
        })
        .collect();
    json!({
        "name": case.name,
        "mode": case.mode.map(EqMode::name),
        "bands": bands
            .iter()
            .map(|b| json!({ "f0": b.f0, "gain_db": b.gain_db, "q": b.q }))
            .collect::<Vec<_>>(),
        "ys": ys,
        "line": path,
        "peak_db": response::peak_db(&bands),
        "peak": peak,
        "peak_words": peak.map(peak_words),
        "markers": markers,
    })
}

/// The whole fixture: the frequencies, every case and the maps.
fn fixture() -> Value {
    let profile = profile_named("gungnir2-v2");
    let eq = profile.eq.as_ref().expect("a tone control");
    let freqs = response::log_freqs(POINTS);
    let cases: Vec<Value> = cases(eq)
        .iter()
        .map(|case| case_json(eq, &freqs, case))
        .collect();
    let hz = [
        15.0, 20.0, 40.0, 100.0, 440.0, 1000.0, 5000.0, 20000.0, 30000.0,
    ];
    let db = [-20.0, -13.0, -6.5, 0.0, 0.05, 3.3, 13.0, 20.0];
    json!({
        "licence_1": LICENCE[0],
        "licence_2": LICENCE[1],
        "freqs": freqs,
        "cases": cases,
        "maps": {
            "x": hz.map(|hz| json!([hz, x(hz)])),
            "y": db.map(|db| json!([db, y(db)])),
        },
    })
}

/// The file as written: the frequencies and each case on a line of their own.
fn text_of(fixture: &Value) -> String {
    let line = |value: &Value| serde_json::to_string(value).expect("JSON");
    let cases: Vec<String> = fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| format!("    {}", line(case)))
        .collect();
    format!(
        "{{\n  \"licence_1\": {},\n  \"licence_2\": {},\n  \"freqs\": {},\n  \"cases\": [\n{}\n  ],\n  \"maps\": {}\n}}\n",
        line(&fixture["licence_1"]),
        line(&fixture["licence_2"]),
        line(&fixture["freqs"]),
        cases.join(",\n"),
        line(&fixture["maps"]),
    )
}

/// Where `a` and `b` first differ, as a path, with numbers equal within [`TOLERANCE`].
fn differs(path: &str, a: &Value, b: &Value) -> Option<String> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64()?, b.as_f64()?);
            ((a - b).abs() > TOLERANCE).then(|| format!("{path}: {a} against {b}"))
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!("{path}: {} entries against {}", a.len(), b.len()));
            }
            a.iter()
                .zip(b)
                .enumerate()
                .find_map(|(i, (a, b))| differs(&format!("{path}[{i}]"), a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() {
                return Some(format!("{path}: {} keys against {}", a.len(), b.len()));
            }
            a.iter().find_map(|(key, a)| match b.get(key) {
                Some(b) => differs(&format!("{path}.{key}"), a, b),
                None => Some(format!("{path}: no {key}")),
            })
        }
        _ => (a != b).then(|| format!("{path}: {a} against {b}")),
    }
}

/// The fixture file the node harness reads.
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/web-checks/node/fixtures/tone-curve.json")
}

/// The file holds what the core computes now: a stale one fails with the variable that rewrites
/// it, so the page's script is never held to numbers the core no longer gives.
#[test]
fn the_curve_fixture_is_current() {
    let now = fixture();
    let path = fixture_path();
    if std::env::var_os("SVOLDER_WRITE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("a directory");
        std::fs::write(&path, text_of(&now)).expect("the fixture is written");
        return;
    }
    let held: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
        .unwrap_or(Value::Null);
    if let Some(at) = differs("fixture", &held, &now) {
        panic!(
            "{} is stale ({at}): rewrite it with \
             SVOLDER_WRITE_FIXTURES=1 cargo test -p svolderd curve_fixture",
            path.display()
        );
    }
}

/// The cases are what they say: a peak under what is worth a warning and one just over it, a
/// boost the warning holds for in EQ mode only, and a curve that has a peak to draw.
#[test]
fn the_fixture_cases_have_the_character_they_are_named_for() {
    let now = fixture();
    let case = |name: &str| {
        now["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .find(|case| case["name"] == name)
            .unwrap_or_else(|| panic!("no case {name}"))
            .clone()
    };
    // the case whose peak lies away from every band: the refinement round the highest sample
    // finds a height the samples alone miss, and the peak is one worth showing
    let profile = profile_named("gungnir2-v2");
    let eq = profile.eq.as_ref().expect("a tone control");
    let away = case("peak away from every band centre");
    let all = cases(eq);
    let found = all
        .iter()
        .find(|c| c.name == "peak away from every band centre");
    let raw = readings(eq, found.expect("a case"));
    let bands = response::bands_of(eq, |id| raw.get(&id).copied()).expect("read");
    let sampled = response::response_db(&bands, &response::log_freqs(1024))
        .into_iter()
        .fold(f64::NEG_INFINITY, f64::max);
    let refined = away["peak_db"].as_f64().expect("a peak");
    assert!(
        refined - sampled > 5e-5,
        "the refinement adds {} dB to the best sample",
        refined - sampled
    );
    assert!(away["peak"].as_f64().expect("shown") > 0.5);
    let signs: Vec<f64> = bands.iter().map(|b| b.gain_db).collect();
    assert!(signs.iter().any(|g| *g > 1.0) && signs.iter().any(|g| *g < -1.0));
    assert_eq!(case("flat")["peak"], Value::Null);
    assert_eq!(case("one cut")["peak"], Value::Null);
    assert!(case("one boost")["peak"].as_f64().expect("a peak") > 4.0);
    for (name, shown) in [("tiny peak", false), ("peak just over", true)] {
        let all = cases(eq);
        let found = all.iter().find(|c| c.name == name).expect("a case");
        let raw = readings(eq, found);
        let bands = response::bands_of(eq, |id| raw.get(&id).copied()).expect("read");
        let peak = response::peak_db(&bands);
        assert!(peak > 0.0, "{name}: {peak}");
        assert_eq!(peak >= PEAK_SHOWN_DB, shown, "{name}: {peak}");
        assert_eq!(case(name)["peak"].is_null(), !shown, "{name}");
    }
    for name in [
        "flat mode with shaped bands",
        "loudness mode with shaped bands",
        "unread mode with shaped bands",
    ] {
        assert_eq!(case(name)["peak"], Value::Null, "{name}");
    }
    assert_eq!(
        case("one boost")["peak_words"]
            .as_str()
            .map(|w| w.starts_with("peak +")),
        Some(true)
    );
}
