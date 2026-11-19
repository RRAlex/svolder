// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's loudness through `get_host` and `set_host_loudness`.

use super::*;

/// A computer view whose loudness is the default settings at volume 35 %, `on` or off.
fn loudness_view(on: bool) -> svolder_ipc::host::HostView {
    use svolder_host::eq::loudness::{LoudnessSettings, LoudnessView};
    let mut view = computer_view();
    let settings = LoudnessSettings {
        on,
        ..LoudnessSettings::default()
    };
    let mut loudness = LoudnessView::of(&settings, None, 0.0, 48_000, None, None);
    loudness.volume_pct = Some(35.0);
    view.eq.as_mut().unwrap().loudness = Some(loudness);
    view
}

#[tokio::test]
async fn get_host_reads_the_loudness_without_its_curve() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = loudness_view(true);
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    let view = fleet
        .host
        .lock()
        .unwrap()
        .eq
        .clone()
        .unwrap()
        .loudness
        .unwrap();
    let ld = &reply["loudness"];
    assert_eq!(
        *ld,
        json!({
            "on": true,
            "strength_pct": view.strength_pct,
            "reference_pct": view.reference_pct,
            "volume_pct": 35.0,
            "depth_db": view.depth_db,
            "lift_db": view.lift_db,
            "mids_db": view.mids_db,
            "treble_db": view.treble_db,
            "following": view.following,
            "note": null,
            "ranges": {
                "strength_pct": {"min": view.ranges.strength_pct.min, "max": view.ranges.strength_pct.max, "step": view.ranges.strength_pct.step},
                "reference_pct": {"min": view.ranges.reference_pct.min, "max": view.ranges.reference_pct.max, "step": view.ranges.reference_pct.step},
            },
        })
    );
    assert!(ld.get("curve").is_none(), "plot data stays out");
    // A refusal to change comes through; the EQ unavailable takes loudness with it.
    let mut off = loudness_view(false);
    off.eq
        .as_mut()
        .unwrap()
        .loudness
        .as_mut()
        .unwrap()
        .unavailable = Some("no sink".into());
    *fleet.host.lock().unwrap() = off;
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["loudness"]["unavailable"], "no sink");
    let mut gone = loudness_view(true);
    gone.eq.as_mut().unwrap().unavailable = Some("computer controls unavailable".into());
    *fleet.host.lock().unwrap() = gone;
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["loudness"], Value::Null);
    // A daemon with no loudness shows null.
    *fleet.host.lock().unwrap() = computer_view();
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["loudness"], Value::Null);
}

#[tokio::test]
async fn set_host_loudness_sends_one_op_and_answers_with_the_view() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = loudness_view(false);
    let reply = run(
        &fleet,
        None,
        call(
            "set_host_loudness",
            json!({"on": true, "strength_pct": 37, "reference_pct": 90}),
        ),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    assert!(structured(&reply)["loudness"].is_object());
    assert_eq!(
        host_sent(&fleet),
        [Request::HostEq {
            op: HostEqOp::Loudness {
                on: Some(true),
                strength_pct: Some(37.0),
                reference_pct: Some(90.0),
                reference_now: None,
                done: true,
            }
        }]
    );
    // "now" maps to reference_now.
    let reply = run(
        &fleet,
        None,
        call("set_host_loudness", json!({"reference": "now"})),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    assert_eq!(
        host_sent(&fleet)[1],
        Request::HostEq {
            op: HostEqOp::Loudness {
                on: None,
                strength_pct: None,
                reference_pct: None,
                reference_now: Some(true),
                done: true,
            }
        }
    );
}

#[tokio::test]
async fn set_host_loudness_refuses_an_empty_call_and_bad_words_before_sending() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = loudness_view(false);
    for bad in [
        json!({}),
        json!({"on": null}),
        json!({"on": "yes"}),
        json!({"strength_pct": "lots"}),
        json!({"reference_pct": true}),
        json!({"reference": "later"}),
        json!({"reference": 3}),
        json!({"reference": "now", "reference_pct": 80}),
        json!({"on": true, "confirm": true}),
        json!({"done": false}),
    ] {
        let reply = run(&fleet, None, call("set_host_loudness", bad.clone()), false).await;
        assert!(is_error(&reply), "{bad}");
    }
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn set_host_loudness_passes_the_controllers_refusal_on() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = loudness_view(false);
    *fleet.host_error.lock().unwrap() = Some("the computer's volume has not been read yet".into());
    let reply = run(
        &fleet,
        None,
        call("set_host_loudness", json!({"reference": "now"})),
        false,
    )
    .await;
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("the computer's volume has not been read yet"));
}

#[tokio::test]
async fn the_loudness_tool_is_listed_only_with_writes_and_refused_without() {
    let fleet = scripted("mesh-eq-5", &[]);
    assert!(!listed(&fleet, false).contains(&"set_host_loudness".to_string()));
    assert!(listed(&fleet, true).contains(&"set_host_loudness".to_string()));
    let tools = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, false, None);
    let reply = tools
        .call(
            call("set_host_loudness", json!({"on": true})),
            Caller { can_ask: false },
        )
        .await
        .unwrap();
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("[mcp] writes"));
    assert!(host_sent(&fleet).is_empty());
}

#[test]
fn the_loudness_tool_schema_reads_the_views_ranges() {
    use svolder_host::eq::loudness::LoudnessView;
    let fleet: Arc<dyn Fleet> = Arc::new(Unavailable);
    let tool = Tools::new(fleet, true, None)
        .list()
        .into_iter()
        .find(|t| t.name == "set_host_loudness")
        .expect("listed");
    let schema = Value::Object((*tool.input_schema).clone());
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], json!([]));
    let props = schema["properties"].as_object().unwrap();
    let mut keys: Vec<&str> = props.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["on", "reference", "reference_pct", "strength_pct"]);
    let ranges = LoudnessView::default().ranges;
    // The default view carries the ranges a controller's view carries.
    let view = svolder_host::eq::loudness::LoudnessView::of(
        &Default::default(),
        None,
        0.0,
        48_000,
        None,
        None,
    );
    assert_eq!(view.ranges, ranges);
    for (key, range) in [
        ("strength_pct", view.ranges.strength_pct),
        ("reference_pct", view.ranges.reference_pct),
    ] {
        assert_eq!(props[key]["minimum"], range.min, "{key}");
        assert_eq!(props[key]["maximum"], range.max, "{key}");
        let text = props[key]["description"].as_str().unwrap();
        assert!(
            text.contains(&format!("in steps of {}", range.step)),
            "{key}: {text}"
        );
    }
    assert_eq!(props["reference"]["enum"], json!(["now"]));
    let description = tool.description.expect("a description");
    assert!(description.ends_with('.'), "{description}");
}
