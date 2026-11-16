// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's crossfeed through `get_host` and `set_host_crossfeed`.

use super::*;

/// A computer view whose crossfeed is the default settings, off.
fn crossfeed_view(on: bool) -> svolder_ipc::host::HostView {
    use svolder_host::eq::crossfeed::{CrossfeedSettings, CrossfeedView};
    let mut view = computer_view();
    view.eq.as_mut().unwrap().crossfeed = Some(CrossfeedView::of(
        &CrossfeedSettings::default(),
        on,
        48_000,
        Some("Speakers".into()),
        None,
    ));
    view
}

#[tokio::test]
async fn get_host_reads_the_crossfeed_beside_the_eq() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = crossfeed_view(true);
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    let xf = &reply["crossfeed"];
    let view = fleet
        .host
        .lock()
        .unwrap()
        .eq
        .clone()
        .unwrap()
        .crossfeed
        .unwrap();
    assert_eq!(
        *xf,
        json!({
            "on": true,
            "model": view.model,
            "preset": view.preset,
            "edited_from": view.edited_from,
            "level_pct": view.amount_pct,
            "level_db": view.amount_db,
            "feed_db": view.feed_db,
            "crossover_hz": view.crossover_hz,
            "delay_ms": view.delay_ms,
            "delay_floor_ms": view.delay_floor_ms,
            "delay_own": view.delay_own,
            "delay_settable": view.delay_settable,
            "output": "Speakers",
            "note": null,
            "off_reason": null,
        })
    );
    assert_eq!(
        (xf["model"].clone(), xf["preset"].clone()),
        (json!("bauer"), json!("bs2b default"))
    );
    assert_ne!(view.feed_db, view.amount_db);
    // A reason it turned itself off and a refusal to change come through.
    let mut off = crossfeed_view(false);
    let held = off.eq.as_mut().unwrap().crossfeed.as_mut().unwrap();
    held.note = Some("crossfeed turned off: the output changed".into());
    held.off_reason = Some("output-changed".into());
    held.unavailable = Some("a curve the chain cannot hold".into());
    *fleet.host.lock().unwrap() = off;
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["crossfeed"]["off_reason"], "output-changed");
    assert_eq!(
        reply["crossfeed"]["note"],
        "crossfeed turned off: the output changed"
    );
    assert_eq!(
        reply["crossfeed"]["unavailable"],
        "a curve the chain cannot hold"
    );
    *fleet.host.lock().unwrap() = crossfeed_view(true);
    assert_eq!(reply["eq"]["on"], true);
    // The EQ unavailable takes the crossfeed with it, as it takes the EQ.
    let mut off = crossfeed_view(true);
    off.eq.as_mut().unwrap().unavailable = Some("computer controls unavailable".into());
    *fleet.host.lock().unwrap() = off;
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["crossfeed"], Value::Null);
}

#[tokio::test]
async fn set_host_crossfeed_sends_what_is_given_and_answers_with_the_view() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = crossfeed_view(false);
    let reply = run(
        &fleet,
        None,
        call(
            "set_host_crossfeed",
            json!({"on": true, "model": "meier", "level_pct": 30, "delay_ms": "own"}),
        ),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    let xf = structured(&reply)["crossfeed"].clone();
    assert_eq!(
        (xf["on"].clone(), xf["model"].clone()),
        (json!(true), json!("meier"))
    );
    assert_eq!(
        host_sent(&fleet),
        [Request::HostEq {
            op: HostEqOp::Crossfeed {
                on: Some(true),
                model: Some("meier".into()),
                preset: None,
                amount_pct: Some(30.0),
                crossover_hz: None,
                delay_ms: Some(0.0),
                done: true,
            }
        }]
    );
    let reply = run(
        &fleet,
        None,
        call(
            "set_host_crossfeed",
            json!({"preset": "Chu Moy", "crossover_hz": 700, "delay_ms": 0.3}),
        ),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    assert_eq!(
        host_sent(&fleet)[1],
        Request::HostEq {
            op: HostEqOp::Crossfeed {
                on: None,
                model: None,
                preset: Some("Chu Moy".into()),
                amount_pct: None,
                crossover_hz: Some(700.0),
                delay_ms: Some(0.3),
                done: true,
            }
        }
    );
}

#[tokio::test]
async fn set_host_crossfeed_refuses_an_empty_call_and_bad_words_before_sending() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = crossfeed_view(false);
    for bad in [
        json!({}),
        json!({"on": null}),
        json!({"model": "tube"}),
        json!({"model": 3}),
        json!({"on": "yes"}),
        json!({"level_pct": "lots"}),
        json!({"delay_ms": "short"}),
        json!({"delay_ms": true}),
        json!({"on": true, "confirm": true}),
        json!({"done": false}),
    ] {
        let reply = run(&fleet, None, call("set_host_crossfeed", bad.clone()), false).await;
        assert!(is_error(&reply), "{bad}");
    }
    let words = error_words(
        &run(
            &fleet,
            None,
            call("set_host_crossfeed", json!({"model": "tube"})),
            false,
        )
        .await,
    );
    assert!(
        words.contains("bauer") && words.contains("linkwitz"),
        "{words}"
    );
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn set_host_crossfeed_passes_the_daemons_refusal_on() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = crossfeed_view(false);
    *fleet.host_error.lock().unwrap() = Some("Bauer has no preset \"Nope\"".into());
    let reply = run(
        &fleet,
        None,
        call("set_host_crossfeed", json!({"preset": "Nope"})),
        false,
    )
    .await;
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("no preset"));
}

#[test]
fn the_crossfeed_tool_schema_reads_the_models_table() {
    use svolder_host::eq::crossfeed::{Model, ModelView};
    let fleet: Arc<dyn Fleet> = Arc::new(Unavailable);
    let tool = Tools::new(fleet, true, None)
        .list()
        .into_iter()
        .find(|t| t.name == "set_host_crossfeed")
        .expect("listed");
    let schema = Value::Object((*tool.input_schema).clone());
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], json!([]));
    let props = schema["properties"].as_object().unwrap();
    let mut keys: Vec<&str> = props.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "crossover_hz",
            "delay_ms",
            "level_pct",
            "model",
            "on",
            "preset"
        ]
    );
    let words: Vec<&str> = Model::ALL.iter().map(|m| m.word()).collect();
    assert_eq!(props["model"]["enum"], json!(words));
    let views: Vec<ModelView> = Model::ALL.into_iter().map(ModelView::of).collect();
    let least = |f: fn(&ModelView) -> f64| views.iter().map(f).fold(f64::INFINITY, f64::min);
    let most = |f: fn(&ModelView) -> f64| views.iter().map(f).fold(0.0, f64::max);
    assert_eq!(props["level_pct"]["minimum"], least(|m| m.amount_pct.min));
    assert_eq!(props["level_pct"]["maximum"], most(|m| m.amount_pct.max));
    assert_eq!(
        props["crossover_hz"]["minimum"],
        least(|m| m.crossover_hz.min)
    );
    assert_eq!(
        props["crossover_hz"]["maximum"],
        most(|m| m.crossover_hz.max)
    );
    let delay = &props["delay_ms"]["anyOf"];
    assert_eq!(delay[0]["maximum"], most(|m| m.delay_max_ms));
    assert_eq!(delay[1]["const"], "own");
    for (key, step) in [("level_pct", 1.0), ("crossover_hz", 10.0)] {
        let text = props[key]["description"].as_str().unwrap();
        assert!(
            text.contains(&format!("in steps of {step}")),
            "{key}: {text}"
        );
    }
    let text = props["delay_ms"]["description"].as_str().unwrap();
    assert!(
        text.contains("in steps of 0.01") && text.contains("mid-side: none"),
        "{text}"
    );
    let preset_words = props["preset"]["description"].as_str().unwrap();
    for view in &views {
        for preset in &view.presets {
            assert!(preset_words.contains(&preset.name), "{}", preset.name);
        }
    }
    let description = tool.description.expect("a description");
    assert!(description.ends_with('.'), "{description}");
}
