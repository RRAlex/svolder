// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How a control and its reading are described: units, gated read-outs and `get_state`.

use super::*;

#[test]
fn the_volume_in_db_keeps_its_own_unit_and_range() {
    let registry = Registry::builtin().unwrap();
    let volume = registry
        .get("gungnir2-v2")
        .unwrap()
        .control_by_key("volume")
        .unwrap();
    assert_eq!(level_raw(volume, &json!(0)).unwrap(), 127);
    let range = level_range(volume).expect("a range");
    assert_eq!(range["max"], json!(0.0));
    assert!(range["min"].as_f64().unwrap() < -96.0);
}

/// `get_state` reads a readout an `only_while` gates as its control allows: warming up reads
/// false while the amp is off or has not said, and true only while it is on and warming up.
#[test]
fn a_gated_readout_reads_false_while_its_gate_reads_false() {
    let registry = Registry::builtin().unwrap();
    let tyr = registry.get("tyr").unwrap();
    let warming = tyr.control_by_key("warming_up").unwrap();
    let power = tyr.control_by_key("power").unwrap();
    let at = |control: &Control, raw: Value| {
        let mut value = reading(&control.key, 0);
        value.id = control.id.to_string();
        value.raw = raw;
        value
    };
    let warm = at(warming, json!(true));
    for (gate, shown) in [
        (Some(json!(false)), false),
        (None, false),
        (Some(json!(true)), true),
    ] {
        let mut values = vec![warm.clone()];
        values.extend(gate.map(|raw| at(power, raw)));
        assert_eq!(
            describe_control(tyr, warming, &values)["value"],
            json!(shown),
            "{values:?}"
        );
    }
    let cool = [at(warming, json!(false)), at(power, json!(true))];
    assert_eq!(describe_control(tyr, warming, &cool)["value"], json!(false));
}

/// A device echoes a passphrase change with a status on function 250 that carries the new code in
/// its encoded form. `get_state` answers a remote client, so even a profile that names a control
/// on that function shows no row for it and no code.
#[tokio::test]
async fn the_passphrase_never_reaches_a_remote_view_in_get_state() {
    let registry = Registry::builtin().unwrap();
    let mut profile = registry.get("gungnir2-v2").unwrap().clone();
    let mut code = profile.control_by_key("input").unwrap().clone();
    code.id = svolder_proto::function::PASSPHRASE;
    code.key = "code".to_string();
    code.label = "Code".to_string();
    code.kind = ControlKind::Readout;
    code.values.clear();
    profile.controls.push(code);
    let fleet = scripted_over(profile, &[]);
    let mut echo = reading("code", 12_648_430);
    echo.id = "250".to_string();
    *fleet.state.lock().unwrap() = vec![reading("input", 1), echo];
    let answer = run(&fleet, None, call("get_state", json!({})), false).await;
    assert!(went(&answer), "{answer:?}");
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let state = result.structured_content.as_ref().expect("a state");
    let text = state.to_string();
    assert!(
        !text.contains("12648430") && !text.contains("\"code\""),
        "{text}"
    );
    let controls = state["controls"].as_array().expect("controls");
    assert!(controls.iter().any(|c| c["key"] == "input"), "{text}");
}
