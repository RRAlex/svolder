// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use serde_json::{Value, json};

use super::*;
use crate::web::test_support::{fake_daemon_playing, web_over};
use svolder_profiles::tone::apo::FilterKind;

fn point(kind: &str, f: f64, g: f64, q: f64) -> PointModel {
    PointModel {
        slot: 0,
        kind: kind.to_string(),
        f,
        g,
        q,
    }
}

/// The file `computer-response.json`: three points, one of each type, and the curve the core
/// draws for them. The page's script (`computer-curve`) is held to the same file.
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/web-checks/node/fixtures/computer-response.json")
}

/// The fixture's points, as the page sends them.
fn fixture_points() -> Vec<PointModel> {
    vec![
        point("PK", 1000.0, 6.0, 1.4),
        point("LSC", 100.0, -4.0, 0.7),
        point("HSC", 8000.0, 5.0, 0.7),
    ]
}

/// The Rust response of the fixture's three points equals the file within 0.01 dB.
/// `SVOLDER_WRITE_FIXTURES=1 cargo test -p svolderd computer_response` rewrites it.
#[test]
fn the_response_matches_the_fixture() {
    let points = fixture_points();
    let response = response_of(&points);
    assert_eq!(response.len(), 181);
    // the fixture holds what the curve is made of: a point's slot is not part of it
    let shown: Vec<Value> = points
        .iter()
        .map(|point| {
            let mut value = json!(point);
            value.as_object_mut().expect("an object").remove("slot");
            value
        })
        .collect();
    if std::env::var_os("SVOLDER_WRITE_FIXTURES").is_some() {
        let text = json!({
            "SPDX-FileCopyrightText: 2026 the svolder authors": "",
            "SPDX-License-Identifier: GPL-3.0-or-later": "",
            "points": shown,
            "freqs": svolder_profiles::tone::response::log_freqs(181),
            "response": response,
        });
        std::fs::write(
            fixture_path(),
            serde_json::to_string_pretty(&text).unwrap() + "\n",
        )
        .unwrap();
    }
    let file: Value = serde_json::from_str(&std::fs::read_to_string(fixture_path()).unwrap())
        .expect("the fixture is JSON");
    assert_eq!(file["points"], json!(shown), "the fixture's points");
    let wanted: Vec<f64> = file["response"]
        .as_array()
        .expect("a response")
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert_eq!(wanted.len(), response.len());
    for (i, (a, b)) in wanted.iter().zip(&response).enumerate() {
        assert!((a - b).abs() < 0.01, "point {i}: file {a}, now {b}");
    }
    assert!(response.iter().any(|db| db.abs() > 3.0), "not a flat curve");
}

/// A page showing the DAC gets the computer's volume, EQ and the presets both sides share.
#[tokio::test]
async fn the_model_carries_volume_eq_and_the_shared_presets() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    // The sink is read while someone watches.
    let _held = web.fleet.host_watch(true);
    let info = web.fleet.resolve(Some("dac")).unwrap();
    let mut model = None;
    for _ in 0..200 {
        model = host_model_of(&web, &info);
        if model.as_ref().is_some_and(|m| m["volume"].is_object()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let model = model.expect("a computer model");
    assert_eq!(model["volume"]["pct"], 42, "{model}");
    assert_eq!(model["volume"]["muted"], false);
    assert!(model["volume"]["sink"].is_string());
    assert_eq!(model["unavailable"], Value::Null);
    assert_eq!(model["eq"]["on"], false);
    assert_eq!(model["eq"]["max"], 16);
    assert_eq!(model["eq"]["points"].as_array().unwrap().len(), 1);
    assert_eq!(model["eq"]["response"].as_array().unwrap().len(), 181);
    let names: Vec<&str> = model["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    for builtin in ["Flat", "Warm", "Bass", "Presence"] {
        assert!(names.contains(&builtin), "{names:?}");
    }
    let warm = model["presets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Warm")
        .unwrap();
    assert_eq!(warm["built_in"], true);
    assert_eq!(warm["playable"], true);
    assert_eq!(warm["why"], Value::Null);
    daemon.stop().await;
}

/// The computer's model as JSON, for the host the daemon serves, shown on `info`.
fn host_model_of(web: &crate::web::Web, info: &svolder_ipc::DeviceInfo) -> Option<Value> {
    let model = crate::web::model::host_computer(web, Some(info))?;
    Some(serde_json::to_value(model).unwrap())
}

/// A saved preset of 17 filters is listed, disabled, with the reason.
#[tokio::test]
async fn a_preset_of_seventeen_filters_is_listed_disabled_with_the_reason() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let config = daemon.dir.join("config.toml");
    for (name, count) in [("Wide", 17), ("Fits", 16)] {
        let filters = (0..count)
            .map(|i| SourceFilter {
                kind: FilterKind::Peaking,
                f: 100.0 + f64::from(i) * 100.0,
                g: 1.0,
                q: 1.0,
            })
            .collect();
        let correction = Correction {
            preamp_db: -3.0,
            filters,
        };
        svolder_ipc::Config::add_preset(&config, &preset::from_computer(name, correction)).unwrap();
    }
    let web = web_over(&daemon);
    let info = web.fleet.resolve(Some("dac")).unwrap();
    let model = host_model_of(&web, &info).expect("a model");
    let presets = model["presets"].as_array().unwrap();
    let find = |name: &str| presets.iter().find(|p| p["name"] == name).cloned();
    let wide = find("Wide").unwrap_or_else(|| panic!("no Wide in {presets:?}"));
    assert_eq!(wide["playable"], false, "{wide}");
    assert_eq!(wide["why"], "17 filters; the computer EQ holds 16");
    assert_eq!(wide["built_in"], false);
    let fits = find("Fits").expect("Fits");
    assert_eq!(fits["playable"], true, "{fits}");
    daemon.stop().await;
}

/// The saved presets are kept while the file stands and read again when it changes.
#[tokio::test]
async fn the_saved_presets_follow_the_file() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    let info = web.fleet.resolve(Some("dac")).unwrap();
    let names = |model: &Value| -> Vec<String> {
        model["presets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect()
    };
    let before = host_model_of(&web, &info).expect("a model");
    assert_eq!(names(&before), names(&host_model_of(&web, &info).unwrap()));
    assert!(!names(&before).contains(&"Mine".to_string()));
    let correction = Correction {
        preamp_db: 0.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: 1000.0,
            g: 1.0,
            q: 1.0,
        }],
    };
    svolder_ipc::Config::add_preset(
        &daemon.dir.join("config.toml"),
        &preset::from_computer("Mine", correction),
    )
    .unwrap();
    let after = host_model_of(&web, &info).expect("a model");
    assert!(names(&after).contains(&"Mine".to_string()), "{after}");
    daemon.stop().await;
}
