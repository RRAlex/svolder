// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Discovery goldens: the exact JSON Home Assistant would read for a device with a tone
//! control (`gungnir2-v2`), one with meters but no tone control (`fake-amp`), one with presets of
//! its own (`lokimax`), and the first device read-only. A change here is a change to what an
//! entity looks like in Home Assistant, so it must be a deliberate edit to the checked-in file,
//! not a passing test.

use serde_json::json;
use svolder_mqtt::discovery::{Device, configs};
use svolder_profiles::Registry;

fn profile(name: &str) -> svolder_profiles::Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .expect("profile exists")
        .clone()
}

/// The names the preset select offers a profile: its built-ins, then one preset of the
/// user's own.
fn preset_names(profile: &svolder_profiles::Profile) -> Vec<String> {
    let Some(eq) = profile.eq.as_ref() else {
        return Vec::new();
    };
    svolder_profiles::tone::preset::fitting(eq, &[])
        .into_iter()
        .map(|(preset, _)| preset.name)
        .chain(["Evening".to_string()])
        .collect()
}

fn device() -> Device<'static> {
    Device {
        name: "Living room DAC",
        unit_id: "0102030405060708",
        firmware: Some(3),
    }
}

/// Every entry's topic and payload, sorted by topic so the golden reads in a stable order
/// regardless of a profile's own control order.
fn sorted_json(entries: Vec<svolder_mqtt::DiscoveryEntry>) -> serde_json::Value {
    let mut rows: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| json!({"topic": e.topic, "payload": e.payload}))
        .collect();
    rows.sort_by(|a, b| a["topic"].as_str().cmp(&b["topic"].as_str()));
    serde_json::Value::Array(rows)
}

/// The golden files carry `{version}` in place of `origin.sw_version`, substituted here for
/// this build's own `CARGO_PKG_VERSION`, so a version bump alone never breaks one of them.
fn assert_golden(actual: serde_json::Value, path: &str) {
    let golden_path = format!("{}/tests/goldens/{path}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|e| panic!("reading {golden_path}: {e}"))
        .replace("{version}", env!("CARGO_PKG_VERSION"));
    let want: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {golden_path}: {e}"));
    assert_eq!(
        actual, want,
        "{golden_path} no longer matches (origin.sw_version is filled in from this build's \
         own version at compare time, so a version bump alone cannot be the cause); if this \
         is deliberate, update the golden file"
    );
}

#[test]
fn gungnir2_v2_with_commands() {
    let profile = profile("gungnir2-v2");
    let entries = configs(
        "svolder",
        "homeassistant",
        &profile,
        &device(),
        true,
        &preset_names(&profile),
    );
    assert_golden(sorted_json(entries), "gungnir2-v2-commands.json");
}

#[test]
fn gungnir2_v2_read_only() {
    let profile = profile("gungnir2-v2");
    let entries = configs(
        "svolder",
        "homeassistant",
        &profile,
        &device(),
        false,
        &preset_names(&profile),
    );
    assert_golden(sorted_json(entries), "gungnir2-v2-read-only.json");
}

#[test]
fn fake_amp_with_commands() {
    let entries = configs(
        "svolder",
        "homeassistant",
        &profile("fake-amp"),
        &device(),
        true,
        &[],
    );
    assert_golden(sorted_json(entries), "fake-amp-commands.json");
}

#[test]
fn lokimax_with_commands() {
    let entries = configs(
        "svolder",
        "homeassistant",
        &profile("lokimax"),
        &device(),
        true,
        &[],
    );
    assert_golden(sorted_json(entries), "lokimax-commands.json");
}
