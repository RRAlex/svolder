// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::time::Duration;

use svolder_ipc::config::{WebConfig, WebGuest};
use svolder_ipc::{Request, Response};
use svolder_profiles::{EqMode, EqParam, EqSpec, units};
use svolder_proto::FunctionId;

use crate::web::test_support::{Code, FakeDaemon, call, fake_daemon, open, post, web, web_with};
use crate::writer::Origin;

/// The fake's tone control.
fn eq(dac: &FakeDaemon) -> EqSpec {
    let info = dac.fleet.resolve(Some("dac")).unwrap();
    dac.fleet.profile(&info).unwrap().eq.unwrap()
}

/// Function `id`'s raw reading on the fake now.
fn reading(dac: &FakeDaemon, id: FunctionId) -> Option<i64> {
    let info = dac.fleet.resolve(Some("dac")).unwrap();
    let watched = dac.fleet.watch(&info).unwrap();
    let feed = watched.feed.borrow();
    feed.readings
        .iter()
        .find(|m| m.id == id)
        .and_then(|m| m.value.as_i64())
}

/// Whether `id` comes to read `want` within five seconds.
async fn comes_to(dac: &FakeDaemon, id: FunctionId, want: i64) -> bool {
    for _ in 0..250 {
        if reading(dac, id) == Some(want) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

/// Set the fake's mode as a socket client would.
async fn mode_to(dac: &FakeDaemon, mode: &str) {
    let answer = dac
        .daemon
        .handle_from(
            Origin::Socket,
            Request::EqMode {
                device: None,
                mode: mode.into(),
                no_wait: false,
            },
        )
        .await;
    assert!(matches!(answer, Response::Value(_)), "{answer:?}");
}

/// The mode track writes the mode and answers the tone part; a mode with no name is refused in
/// the core's words.
#[tokio::test]
async fn a_mode_write_answers_the_tone_part() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "eq").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let flat = post("/api/v1/devices/dac/eq/mode", r#"{"mode":"flat"}"#);
    let reply = call(&web, flat).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["mode"], "flat", "{said}");
    assert!(said["parts"]["tone"].is_object(), "{said}");
    assert_eq!(reading(&dac, eq(&dac).mode_id()), Some(EqMode::Flat as i64));
    let reply = call(
        &web,
        post("/api/v1/devices/dac/eq/mode", r#"{"mode":"loud"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert!(reply.body.contains("loud"), "{}", reply.body);
    dac.stop().await;
}

/// A stepper moves a band's parameter one raw step and answers it in the band wording; at the
/// end of its steps it writes nothing and answers where it is.
#[tokio::test]
async fn a_band_steps_one_raw_step_and_answers_in_the_band_wording() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "eq").await;
    let eq = eq(&dac);
    let id = eq.band_id(1, EqParam::Gain).unwrap();
    let before = reading(&dac, id).expect("the fake reports band 2's gain");
    let web = web(open(), Arc::clone(&dac.fleet));
    let step = post(
        "/api/v1/devices/dac/eq/bands/2",
        r#"{"param":"g","step":1}"#,
    );
    let reply = call(&web, step).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["raw"], before + 1, "{said}");
    let words = units::band_text(&eq, 1, EqParam::Gain, before + 1)
        .unwrap()
        .replace('-', "−");
    assert_eq!(said["text"], words.as_str());
    assert_eq!(said["mode_set_to_eq"], false);
    // A step of five is still one step.
    let big = post(
        "/api/v1/devices/dac/eq/bands/2",
        r#"{"param":"g","step":5}"#,
    );
    let said: serde_json::Value = serde_json::from_str(&call(&web, big).await.body).unwrap();
    assert_eq!(said["raw"], before + 2, "{said}");
    // At the top of Q's steps a further step writes nothing.
    let q = eq.band_id(1, EqParam::Q).unwrap();
    let to_top = post(
        "/api/v1/devices/dac/eq/bands/2",
        r#"{"param":"q","value":99}"#,
    );
    let top = call(&web, to_top).await;
    assert_eq!(top.status, Code::OK, "{}", top.body);
    assert!(comes_to(&dac, q, 127).await);
    let more = post(
        "/api/v1/devices/dac/eq/bands/2",
        r#"{"param":"q","step":1}"#,
    );
    let said: serde_json::Value = serde_json::from_str(&call(&web, more).await.body).unwrap();
    assert_eq!(said["raw"], 127);
    for (path, body, code) in [
        (
            "/api/v1/devices/dac/eq/bands/9",
            r#"{"param":"g","step":1}"#,
            Code::NOT_FOUND,
        ),
        (
            "/api/v1/devices/dac/eq/bands/0",
            r#"{"param":"g","step":1}"#,
            Code::NOT_FOUND,
        ),
        (
            "/api/v1/devices/dac/eq/bands/2",
            r#"{"param":"z","step":1}"#,
            Code::UNPROCESSABLE_ENTITY,
        ),
        (
            "/api/v1/devices/dac/eq/bands/2",
            r#"{"param":"g"}"#,
            Code::BAD_REQUEST,
        ),
    ] {
        let reply = call(&web, post(path, body)).await;
        assert_eq!(reply.status, code, "{path} {body}: {}", reply.body);
    }
    dac.stop().await;
}

/// A typed value is read with its unit, through the core's reader; text that is no number is
/// refused with the parameter's words.
#[tokio::test]
async fn a_typed_value_is_read_with_its_unit() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "eq").await;
    let eq = eq(&dac);
    let web = web(open(), Arc::clone(&dac.fleet));
    let hz = post(
        "/api/v1/devices/dac/eq/bands/3",
        r#"{"param":"f","value":"1000 Hz"}"#,
    );
    let reply = call(&web, hz).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let want = i64::from(eq.frequency_raw(2, 1000.0).unwrap());
    assert!(comes_to(&dac, eq.band_id(2, EqParam::Frequency).unwrap(), want).await);
    let db = post(
        "/api/v1/devices/dac/eq/bands/3",
        r#"{"param":"g","value":"−2 dB"}"#,
    );
    let reply = call(&web, db).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let want = i64::from(eq.gain_raw(-2.0));
    assert!(comes_to(&dac, eq.band_id(2, EqParam::Gain).unwrap(), want).await);
    for body in [
        r#"{"param":"f","value":"loud"}"#,
        r#"{"param":"f","value":-5}"#,
    ] {
        let reply = call(&web, post("/api/v1/devices/dac/eq/bands/3", body)).await;
        assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{body}");
        assert!(reply.body.contains("Frequency takes"), "{}", reply.body);
    }
    dac.stop().await;
}

/// A band written in Flat switches the mode to EQ through the writer, and the answer says so, as a
/// flag for the band editor and as the words of a toast.
#[tokio::test]
async fn a_band_written_in_flat_switches_the_mode_to_eq() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "flat").await;
    let eq = eq(&dac);
    let web = web(open(), Arc::clone(&dac.fleet));
    let step = post(
        "/api/v1/devices/dac/eq/bands/1",
        r#"{"param":"g","step":1}"#,
    );
    let reply = call(&web, step).await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["mode_set_to_eq"], true, "{said}");
    assert!(
        comes_to(&dac, eq.mode_id(), EqMode::Eq as i64).await,
        "the mode stayed Flat"
    );
    mode_to(&dac, "flat").await;
    let down = post(
        "/api/v1/devices/dac/eq/bands/1",
        r#"{"param":"g","step":-1}"#,
    );
    let reply = call(&web, down).await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["toast"], "Mode set to EQ", "{said}");
    dac.stop().await;
}

/// A band written out of Flat says so in its toast, and the tone part it answers with reads EQ.
#[tokio::test]
async fn a_band_write_out_of_flat_says_so() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "flat").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let step = post(
        "/api/v1/devices/dac/eq/bands/1",
        r#"{"param":"g","step":1}"#,
    );
    let reply = call(&web, step).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["mode_set_to_eq"], true, "{said}");
    assert_eq!(said["toast"], "Mode set to EQ", "{said}");
    assert_eq!(said["parts"]["tone"]["mode"], "eq", "{said}");
    // A band written in EQ has nothing to say.
    let step = post(
        "/api/v1/devices/dac/eq/bands/1",
        r#"{"param":"g","step":1}"#,
    );
    let said: serde_json::Value = serde_json::from_str(&call(&web, step).await.body).unwrap();
    assert!(said.get("toast").is_none(), "{said}");
    dac.stop().await;
}

/// A drag writes the raw step it shows: for every band and parameter the first, a middle and the
/// last step land on that exact step, which the answer echoes.
#[tokio::test]
async fn a_raw_band_write_lands_on_that_exact_step() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "eq").await;
    let eq = eq(&dac);
    let web = web(open(), Arc::clone(&dac.fleet));
    for band in 0..eq.bands.len() {
        for param in [EqParam::Frequency, EqParam::Gain, EqParam::Q] {
            for raw in [0_i64, 64, 127] {
                let body = format!(r#"{{"param":"{}","raw":{raw}}}"#, param.word());
                let path = format!("/api/v1/devices/dac/eq/bands/{}", band + 1);
                let reply = call(&web, post(&path, &body)).await;
                assert_eq!(reply.status, Code::OK, "{path} {body}: {}", reply.body);
                let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
                assert_eq!(said["raw"], raw, "{path} {body}: {said}");
                let id = eq.band_id(band, param).unwrap();
                assert!(comes_to(&dac, id, raw).await, "{path} {body}");
            }
        }
    }
    dac.stop().await;
}

/// A raw step is 0 to 127; one outside is refused in those words and writes nothing.
#[tokio::test]
async fn a_raw_step_out_of_range_is_refused() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "eq").await;
    let id = eq(&dac).band_id(0, EqParam::Gain).unwrap();
    let before = reading(&dac, id);
    let web = web(open(), Arc::clone(&dac.fleet));
    for raw in [-1, 128] {
        let body = format!(r#"{{"param":"g","raw":{raw}}}"#);
        let reply = call(&web, post("/api/v1/devices/dac/eq/bands/1", &body)).await;
        assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{body}");
        assert!(reply.body.contains("a step is 0 to 127"), "{}", reply.body);
    }
    assert_eq!(reading(&dac, id), before);
    dac.stop().await;
}

/// A band write takes exactly one of `step`, `value` and `raw`: two of them, or none, is refused.
#[tokio::test]
async fn a_band_write_takes_one_of_step_value_or_raw() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "eq").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    for body in [
        r#"{"param":"g","step":1,"raw":5}"#,
        r#"{"param":"g","value":1,"raw":5}"#,
        r#"{"param":"g","step":1,"value":1}"#,
        r#"{"param":"g","step":1,"value":1,"raw":5}"#,
        r#"{"param":"g"}"#,
    ] {
        let reply = call(&web, post("/api/v1/devices/dac/eq/bands/1", body)).await;
        assert_eq!(reply.status, Code::BAD_REQUEST, "{body}");
        assert!(
            reply
                .body
                .contains("a band write takes a step, a value or a raw step"),
            "{}",
            reply.body
        );
    }
    dac.stop().await;
}

/// A raw band write while the mode reads Flat switches it to EQ through the writer, like every
/// band write, and the answer says so.
#[tokio::test]
async fn a_raw_write_in_flat_switches_to_eq() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "flat").await;
    let eq = eq(&dac);
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        post(
            "/api/v1/devices/dac/eq/bands/1",
            r#"{"param":"g","raw":90}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["mode_set_to_eq"], true, "{said}");
    assert!(
        comes_to(&dac, eq.mode_id(), EqMode::Eq as i64).await,
        "the mode stayed Flat"
    );
    dac.stop().await;
}

/// The loudness strength steps within 0 to 127; a raw value outside is refused.
#[tokio::test]
async fn the_loudness_steps_within_its_range() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    mode_to(&dac, "loudness").await;
    let id = eq(&dac).loudness_id();
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        post("/api/v1/devices/dac/eq/loudness", r#"{"raw":20}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(comes_to(&dac, id, 20).await);
    let reply = call(
        &web,
        post("/api/v1/devices/dac/eq/loudness", r#"{"step":1}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(comes_to(&dac, id, 21).await);
    let reply = call(
        &web,
        post("/api/v1/devices/dac/eq/loudness", r#"{"raw":200}"#),
    )
    .await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    dac.stop().await;
}

/// A recall from the page goes through the writer, which remembers it for every front end; a
/// name that fits nothing is refused with the recall's words.
#[tokio::test]
async fn a_recall_from_the_page_lands_and_is_remembered() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let warm = post("/api/v1/devices/dac/presets/recall", r#"{"name":"Warm"}"#);
    let reply = call(&web, warm).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["name"], "Warm", "{said}");
    assert!(said["parts"]["tone"].is_object(), "{said}");
    assert!(said["parts"]["presets"].is_object(), "{said}");
    let Response::Devices { devices } = dac
        .daemon
        .handle_from(Origin::Socket, Request::Devices)
        .await
    else {
        panic!("no devices");
    };
    assert_eq!(devices[0].recalled.as_deref(), Some("Warm"));
    let cool = post(
        "/api/v1/devices/dac/presets/recall",
        r#"{"name":"Presence"}"#,
    );
    let said: serde_json::Value = serde_json::from_str(&call(&web, cool).await.body).unwrap();
    assert_eq!(said["name"], "Presence", "{said}");
    assert!(said["parts"]["tone"].is_object(), "{said}");
    assert!(said["parts"]["presets"].is_object(), "{said}");
    let nope = post("/api/v1/devices/dac/presets/recall", r#"{"name":"Nope"}"#);
    let reply = call(&web, nope).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert!(reply.body.contains("Nope"), "{}", reply.body);
    dac.stop().await;
}

/// A guest's tone and preset writes are refused 401, a read-only page's 403.
#[tokio::test]
async fn a_guest_and_a_read_only_page_cannot_touch_the_tone_control() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    for (path, body) in [
        ("eq/mode", r#"{"mode":"flat"}"#),
        ("eq/bands/1", r#"{"param":"g","step":1}"#),
        ("eq/loudness", r#"{"step":1}"#),
        ("presets/recall", r#"{"name":"Warm"}"#),
        ("presets", r#"{"name":"Mine"}"#),
        ("presets/rename", r#"{"from":"Mine","to":"Ours"}"#),
        ("presets/delete", r#"{"name":"Mine"}"#),
    ] {
        let path = format!("/api/v1/devices/dac/{path}");
        let guest = call(&guests, post(&path, body)).await;
        assert_eq!(guest.status, Code::UNAUTHORIZED, "{path}");
        let off = call(&read_only, post(&path, body)).await;
        assert_eq!(off.status, Code::FORBIDDEN, "{path}");
    }
    dac.stop().await;
}

/// A loudness step from a reading never taken is refused with 409 (as a band's is), not written
/// as 1 or 0; a raw value needs no reading.
#[test]
fn a_loudness_step_needs_a_reading() {
    let step = |read, step, raw| super::loudness_target(read, &super::LoudnessWrite { step, raw });
    assert_eq!(step(Some(10), Some(1), None).ok(), Some(11));
    assert_eq!(step(Some(0), Some(-1), None).ok(), Some(0));
    assert_eq!(step(None, None, Some(20)).ok(), Some(20));
    let refused = step(None, Some(1), None).unwrap_err();
    assert_eq!(refused.status(), Code::CONFLICT);
}

/// A raw step in range that its band has no value for is a 409 (`that step has no value`, as a
/// step from a reading says), not the out-of-range 422; one out of range stays a 422.
#[tokio::test]
async fn a_raw_step_without_a_value_is_a_conflict_not_out_of_range() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let eq = eq(&dac);
    let answer = |index, raw| super::raw_value(&eq, index, EqParam::Frequency, raw);
    assert!(answer(0, 64).is_ok());
    // a band the tone control does not have has no frequency at any step
    let none = answer(eq.bands.len(), 64).unwrap_err();
    assert_eq!(none.status(), Code::CONFLICT);
    let body = axum::body::to_bytes(none.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&body).contains("that step has no value"),
        "{body:?}"
    );
    for raw in [-1, 128] {
        let out = answer(eq.bands.len(), raw).unwrap_err();
        assert_eq!(out.status(), Code::UNPROCESSABLE_ENTITY, "{raw}");
        let out = answer(0, raw).unwrap_err();
        assert_eq!(out.status(), Code::UNPROCESSABLE_ENTITY, "{raw}");
    }
    dac.stop().await;
}
