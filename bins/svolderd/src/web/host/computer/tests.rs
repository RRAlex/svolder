// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use serde_json::Value;
use svolder_ipc::config::{WebConfig, WebGuest};
use svolder_ipc::{HostView, Request, Response};

use super::question;
use crate::web::test_support::{
    Code, FakeDaemon, call, fake_daemon, fake_daemon_playing, open, post, web, web_over, web_with,
};

/// Every route of the computer, with a body each would accept.
const ROUTES: [(&str, &str); 7] = [
    ("/api/v1/host/volume", r#"{"pct":40}"#),
    ("/api/v1/host/mute", r#"{"muted":true}"#),
    ("/api/v1/host/eq", r#"{"on":true}"#),
    ("/api/v1/host/eq/preset", r#"{"name":"Warm"}"#),
    (
        "/api/v1/host/eq/points",
        r#"{"type":"PK","f":1000,"g":3,"q":1}"#,
    ),
    ("/api/v1/host/eq/points/1", r#"{"g":2,"done":true}"#),
    ("/api/v1/host/eq/points/1/remove", "{}"),
];

/// The view the daemon answers the socket's `host` request with.
async fn view_of(daemon: &FakeDaemon) -> HostView {
    match daemon.fleet.request(Request::Host).await {
        Response::Host(view) => view,
        other => panic!("no host view: {other:?}"),
    }
}

fn json(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"))
}

/// A guest is refused 401 and a page with writes off 403 on every route, and nothing is sent to
/// the computer.
#[tokio::test]
async fn every_computer_route_needs_a_signed_writer() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let token = "a-test-token-0123456789abcdefghijklmn";
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&daemon.fleet),
        Some(token),
    );
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&daemon.fleet),
    );
    for (path, body) in ROUTES {
        let reply = call(&guests, post(path, body)).await;
        assert_eq!(reply.status, Code::UNAUTHORIZED, "{path}: {}", reply.body);
        let reply = call(&read_only, post(path, body)).await;
        assert_eq!(reply.status, Code::FORBIDDEN, "{path}: {}", reply.body);
    }
    let eq = view_of(&daemon).await.eq.expect("the computer EQ");
    assert!(!eq.on, "nothing was switched on");
    assert_eq!(eq.points.len(), 1, "nor a point added");
    daemon.stop().await;
}

/// The volume goes from 0 to 100 and no further; a value outside it is a 422 and reaches nothing.
#[tokio::test]
async fn a_volume_above_100_is_422() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    for body in [
        r#"{"pct":101}"#,
        r#"{"pct":-1}"#,
        r#"{"pct":256}"#,
        r#"{"pct":100000}"#,
    ] {
        let reply = call(&web, post("/api/v1/host/volume", body)).await;
        assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{body}");
    }
    let reply = call(&web, post("/api/v1/host/volume", r#"{"pct":100}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(json(&reply.body), serde_json::json!({ "ok": true }));
    let volume = view_of(&daemon).await.volume.expect("a volume");
    assert_eq!(volume.pct, 100);
    let reply = call(&web, post("/api/v1/host/volume", r#"{"pct":"loud"}"#)).await;
    assert_eq!(reply.status, Code::BAD_REQUEST);
    daemon.stop().await;
}

/// Mute, the switch, a point added, changed and removed reach the service, which counts from 1.
#[tokio::test]
async fn the_routes_reach_the_service_with_one_based_points() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    let send = |path: &'static str, body: &'static str| {
        let web = web.clone();
        async move {
            let reply = call(&web, post(path, body)).await;
            assert_eq!(reply.status, Code::OK, "{path}: {}", reply.body);
        }
    };
    send("/api/v1/host/mute", r#"{"muted":true}"#).await;
    assert!(view_of(&daemon).await.volume.expect("volume").muted);
    send("/api/v1/host/mute", r#"{"muted":false}"#).await;
    assert!(!view_of(&daemon).await.volume.expect("volume").muted);
    send("/api/v1/host/eq", r#"{"on":true}"#).await;
    assert!(view_of(&daemon).await.eq.expect("eq").on);
    send(
        "/api/v1/host/eq/points",
        r#"{"type":"lsc","f":120,"g":4,"q":0.7}"#,
    )
    .await;
    let points = view_of(&daemon).await.eq.expect("eq").points;
    assert_eq!(points.len(), 2);
    // The points are listed by frequency: the new 120 Hz point is point 1.
    assert_eq!(
        (points[0].kind.as_str(), points[0].f, points[0].g),
        ("LSC", 120.0, 4.0)
    );
    send(
        "/api/v1/host/eq/points/1",
        r#"{"type":"HSC","g":-3,"done":false}"#,
    )
    .await;
    let points = view_of(&daemon).await.eq.expect("eq").points;
    assert_eq!(points[0].f, 120.0, "a drag never re-sorts");
    send("/api/v1/host/eq/points/1", r#"{"f":8000}"#).await;
    let points = view_of(&daemon).await.eq.expect("eq").points;
    assert_eq!(
        (points[1].kind.as_str(), points[1].f, points[1].g),
        ("HSC", 8000.0, -3.0)
    );
    assert_eq!(
        points[0].g, 0.0,
        "the 1 kHz point is untouched and now first"
    );
    for path in ["/api/v1/host/eq/points/0", "/api/v1/host/eq/points/x"] {
        let reply = call(&web, post(path, r#"{"g":1}"#)).await;
        assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{path}");
    }
    let reply = call(
        &web,
        post(
            "/api/v1/host/eq/points",
            r#"{"type":"BP","f":1000,"g":0,"q":1}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    send("/api/v1/host/eq/points/2/remove", "{}").await;
    assert_eq!(view_of(&daemon).await.eq.expect("eq").points.len(), 1);
    let reply = call(&web, post("/api/v1/host/eq/points/5/remove", "{}")).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert!(json(&reply.body)["error"].is_string());
    daemon.stop().await;
}

/// A request that names the point's slot lands on that point even when the table has been
/// re-sorted since the sender read it; a slot nobody holds is a 409 that changes nothing, and a
/// slot outside 1 to 16 is a 422.
#[tokio::test]
async fn a_slot_names_the_point_and_a_vanished_one_is_a_409() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    let reply = call(&web, post("/api/v1/host/eq", r#"{"on":true}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let reply = call(
        &web,
        post(
            "/api/v1/host/eq/points",
            r#"{"type":"PK","f":120,"g":4,"q":1}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let points = view_of(&daemon).await.eq.expect("eq").points;
    assert_eq!((points[0].f, points[0].slot), (120.0, 2));
    assert_eq!((points[1].f, points[1].slot), (1000.0, 1));
    // The sender holds the 1 kHz point as point 1: the slot wins over the stale index.
    let reply = call(
        &web,
        post(
            "/api/v1/host/eq/points/1",
            r#"{"g":6,"slot":1,"done":true}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let before = view_of(&daemon).await.eq.expect("eq").points;
    assert_eq!((before[0].f, before[0].g), (120.0, 4.0));
    assert_eq!((before[1].f, before[1].g), (1000.0, 6.0));
    for (path, body) in [
        ("/api/v1/host/eq/points/1", r#"{"g":-5,"slot":9}"#),
        ("/api/v1/host/eq/points/2/remove", r#"{"slot":9}"#),
    ] {
        let reply = call(&web, post(path, body)).await;
        assert_eq!(reply.status, Code::CONFLICT, "{path}: {}", reply.body);
        assert!(
            json(&reply.body)["error"]
                .as_str()
                .is_some_and(|words| words.contains("moved or is gone")),
            "{}",
            reply.body
        );
    }
    assert_eq!(view_of(&daemon).await.eq.expect("eq").points, before);
    for body in [r#"{"g":1,"slot":0}"#, r#"{"g":1,"slot":17}"#] {
        let reply = call(&web, post("/api/v1/host/eq/points/1", body)).await;
        assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{body}");
    }
    let reply = call(
        &web,
        post("/api/v1/host/eq/points/1/remove", r#"{"slot":17}"#),
    )
    .await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY);
    assert_eq!(view_of(&daemon).await.eq.expect("eq").points, before);
    // A remove by slot takes that point, and a bare remove still takes the one at the index.
    let reply = call(
        &web,
        post("/api/v1/host/eq/points/1/remove", r#"{"slot":1}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let points = view_of(&daemon).await.eq.expect("eq").points;
    assert_eq!(points.len(), 1);
    assert_eq!((points[0].f, points[0].slot), (120.0, 2));
    daemon.stop().await;
}

/// A point added outside the hand ranges is a 422 and adds nothing.
#[tokio::test]
async fn a_point_outside_the_hand_ranges_is_422() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    for body in [
        r#"{"type":"PK","f":1000,"g":12,"q":0}"#,
        r#"{"type":"PK","f":0,"g":0,"q":1}"#,
        r#"{"type":"PK","f":1000,"g":100000,"q":1}"#,
        r#"{"type":"PK","f":30000,"g":0,"q":1}"#,
    ] {
        let reply = call(&web, post("/api/v1/host/eq/points", body)).await;
        assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{body}");
    }
    assert_eq!(view_of(&daemon).await.eq.expect("eq").points.len(), 1);
    daemon.stop().await;
}

/// With the host off every route answers 503 with the reason.
#[tokio::test]
async fn unavailable_is_503_with_the_reason() {
    let daemon = fake_daemon("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    for (path, body) in ROUTES {
        let reply = call(&web, post(path, body)).await;
        assert_eq!(reply.status, Code::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(
            json(&reply.body)["error"],
            "computer controls unavailable: host audio is off",
            "{path}"
        );
    }
    daemon.stop().await;
}

/// A preset a connected device plays too is a question, not a plain refusal; yes repeats the
/// request with `confirm` and the device is flattened before the computer takes it.
#[tokio::test]
async fn the_same_preset_on_both_asks_and_confirm_flattens_the_dac() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    let reply = call(
        &web,
        post("/api/v1/devices/dac/presets/recall", r#"{"name":"Warm"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let body = r#"{"name":"Warm","device":"dac"}"#;
    let reply = call(&web, post("/api/v1/host/eq/preset", body)).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    let said = json(&reply.body);
    assert_eq!(
        said["error"],
        "Warm is on the dac too: the correction would play twice"
    );
    assert_eq!(said["ask"]["kind"], "both_sides");
    assert_eq!(said["ask"]["name"], "Warm");
    assert_eq!(said["ask"]["devices"], serde_json::json!(["dac"]));
    assert_eq!(
        said["ask"]["question"],
        "Warm is on the dac too: flatten the dac and apply on the computer?"
    );
    assert!(
        !view_of(&daemon).await.eq.expect("eq").on,
        "asking changes nothing"
    );
    let again = r#"{"name":"Warm","device":"dac","confirm":true}"#;
    let reply = call(&web, post("/api/v1/host/eq/preset", again)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let eq = view_of(&daemon).await.eq.expect("eq");
    assert_eq!(eq.preset.as_deref(), Some("Warm"));
    daemon.stop().await;
}

/// The question is built from the daemon's fields: a device whose name has " and the " in it is
/// one device, and the several are joined for the sentence only.
#[tokio::test]
async fn the_question_keeps_every_device_whole() {
    let said = question(
        "Warm is on the cat and the hat and the dac too: the correction would play twice".into(),
        "Warm",
        &["cat and the hat".to_string(), "dac".to_string()],
    );
    assert_eq!(said.status(), Code::CONFLICT);
    let body = axum::body::to_bytes(said.into_body(), usize::MAX)
        .await
        .unwrap();
    let said = json(std::str::from_utf8(&body).unwrap());
    assert_eq!(
        said["ask"]["devices"],
        serde_json::json!(["cat and the hat", "dac"])
    );
    assert_eq!(
        said["ask"]["question"],
        "Warm is on the cat and the hat and the dac too: flatten the cat and the hat and the dac \
         and apply on the computer?"
    );
}

/// A curve that changes nothing is never asked about, though a device plays it: Flat on the
/// computer while the DAC is flat is a plain 200.
#[tokio::test]
async fn a_neutral_curve_never_asks() {
    let daemon = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&daemon);
    let reply = call(
        &web,
        post("/api/v1/devices/dac/presets/recall", r#"{"name":"Flat"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let reply = call(
        &web,
        post(
            "/api/v1/host/eq/preset",
            r#"{"name":"Flat","device":"dac"}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(json(&reply.body).get("ask").is_none());
    daemon.stop().await;
}
