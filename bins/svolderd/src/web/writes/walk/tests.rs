// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use svolder_ipc::config::{WebConfig, WebGuest};
use tower::ServiceExt;

use crate::web::router;
use crate::web::test_support::{
    Code, FakeDaemon, call, events_until, fake_daemon, get, open, post, web, web_with,
};

fn said(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{body}: {e}"))
}

/// Start a walk from `page`; its job's id.
async fn started(page: &crate::web::Web) -> u64 {
    let reply = call(page, post("/api/v1/walk", "")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    said(&reply.body)["job"].as_u64().expect("the job's id")
}

/// The page's events stream, open on `path`.
async fn stream(page: &crate::web::Web, path: &str) -> Body {
    let response = router::router(page.clone())
        .oneshot(get(path))
        .await
        .expect("the router never fails");
    assert_eq!(response.status(), Code::OK);
    response.into_body()
}

/// The `walk` events among `events`, parsed.
fn walks(events: &[(String, String)]) -> Vec<serde_json::Value> {
    events
        .iter()
        .filter(|(name, _)| name == "walk")
        .map(|(_, data)| said(data))
        .collect()
}

/// Whether the walk's last state among `events` waits at the step `step`.
fn waits_at(events: &[(String, String)], step: &str) -> bool {
    walks(events).last().is_some_and(|event| {
        let payload = &event["state"]["payload"];
        payload["phase"] == "waiting" && payload["prompt"]["step"] == step
    })
}

/// A walk is started, answered and cancelled from the page; its states come on the page's
/// stream with `needs=walk`; a second start is refused busy, naming the web.
#[tokio::test]
async fn a_walk_is_started_answered_and_followed_from_the_page() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let page = web(open(), Arc::clone(&dac.fleet));
    let body = stream(&page, "/api/v1/events?needs=walk").await;
    let reply = call(&page, post("/api/v1/walk", "")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let job = said(&reply.body)["job"].as_u64().expect("the job's id");
    let started_run = said(&reply.body)["run"].clone();

    let busy = call(&page, post("/api/v1/walk", "")).await;
    assert_eq!(busy.status, Code::CONFLICT, "{}", busy.body);
    let refused = said(&busy.body);
    assert_eq!(refused["reason"], "busy");
    assert!(
        refused["error"].as_str().unwrap().contains("web"),
        "{refused}"
    );

    let events = events_until(body, |events| waits_at(events, "linked")).await;
    let warned = walks(&events).last().cloned().expect("a walk event");
    assert_eq!(warned["job"], job);
    assert_eq!(warned["state"]["payload"]["origin"], "web");
    assert!(
        warned["run"].as_u64().is_some_and(|run| run > 0),
        "{warned}"
    );
    assert_eq!(
        warned["run"], started_run,
        "the start and the events name one run"
    );
    assert!(
        warned["state"]["payload"]["prompt"]["body"][0]
            .as_str()
            .unwrap()
            .starts_with("dac has a link"),
        "{warned}"
    );

    let body = stream(&page, "/api/v1/events?needs=walk").await;
    let reply = call(
        &page,
        post(&format!("/api/v1/walk/{job}/answer"), r#"{"choice":0}"#),
    )
    .await;
    assert_eq!(reply.status, Code::NO_CONTENT, "{}", reply.body);
    let events = events_until(body, |events| waits_at(events, "scan")).await;
    assert!(waits_at(&events, "scan"), "{events:?}");

    let body = stream(&page, "/api/v1/events?needs=walk").await;
    let reply = call(&page, post(&format!("/api/v1/walk/{job}/answer"), "")).await;
    assert_eq!(reply.status, Code::NO_CONTENT, "{}", reply.body);
    let events = events_until(body, |events| waits_at(events, "pick")).await;
    let pick = walks(&events).last().cloned().expect("a walk event");
    let rows = pick["state"]["payload"]["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "{pick}");
    assert_eq!(rows[0]["configured"], "dac");

    let reply = call(&page, post(&format!("/api/v1/walk/{job}/back"), "")).await;
    assert_eq!(reply.status, Code::NO_CONTENT, "{}", reply.body);
    let body = stream(&page, "/api/v1/events?needs=walk").await;
    let reply = call(&page, post(&format!("/api/v1/walk/{job}/cancel"), "")).await;
    assert_eq!(reply.status, Code::NO_CONTENT, "{}", reply.body);
    let events = events_until(body, |events| {
        walks(events)
            .last()
            .is_some_and(|event| event["state"]["kind"] == "cancelled")
    })
    .await;
    assert!(!walks(&events).is_empty(), "{events:?}");

    let late = call(&page, post(&format!("/api/v1/walk/{job}/answer"), "")).await;
    assert_eq!(late.status, Code::CONFLICT, "{}", late.body);
    assert_eq!(said(&late.body)["reason"], "not_waiting");
    let none = call(&page, post("/api/v1/walk/999/back", "")).await;
    assert_eq!(none.status, Code::NOT_FOUND, "{}", none.body);
    assert_eq!(said(&none.body)["reason"], "no_such_job");
    dac.stop().await;
}

/// A body that is not what an answer takes is a 400 whose words never quote it: a text that is
/// no string (a passphrase typed as a number), a body that is no object, a job that is no number.
#[tokio::test]
async fn a_body_that_does_not_parse_is_refused_without_its_content() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let page = web(open(), Arc::clone(&dac.fleet));
    let job = started(&page).await;
    for (path, body) in [
        (format!("/api/v1/walk/{job}/answer"), r#"{"text":4321}"#),
        (format!("/api/v1/walk/{job}/answer"), r#"["4321"]"#),
        (format!("/api/v1/walk/{job}/answer"), "4321"),
        (
            "/api/v1/walk/4321x/answer".to_string(),
            r#"{"text":"1234"}"#,
        ),
    ] {
        let reply = call(&page, post(&path, body)).await;
        assert_eq!(
            reply.status,
            Code::BAD_REQUEST,
            "{path} {body}: {}",
            reply.body
        );
        assert!(!reply.body.contains("4321"), "{}", reply.body);
        assert!(!reply.body.contains("1234"), "{}", reply.body);
        assert!(said(&reply.body).get("reason").is_none(), "{}", reply.body);
    }
    call(&page, post(&format!("/api/v1/walk/{job}/cancel"), "")).await;
    dac.stop().await;
}

/// Only a signed-in writer walks: a guest is told to sign in (401), a page with writes off that
/// it is read-only (403), and neither page is sent the walk's states.
#[tokio::test]
async fn the_walk_routes_obey_the_write_gate() {
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
    for (page, status) in [(&guests, Code::UNAUTHORIZED), (&read_only, Code::FORBIDDEN)] {
        for path in [
            "/api/v1/walk",
            "/api/v1/walk/1/answer",
            "/api/v1/walk/1/back",
            "/api/v1/walk/1/cancel",
        ] {
            let reply = call(page, post(path, r#"{"text":"1234"}"#)).await;
            assert_eq!(reply.status, status, "{path}: {}", reply.body);
        }
    }
    let writer = web(open(), Arc::clone(&dac.fleet));
    let job = started(&writer).await;
    for page in [&guests, &read_only] {
        let body = stream(page, "/api/v1/events?needs=walk").await;
        let events = crate::web::test_support::events_within(
            body,
            |events| !walks(events).is_empty(),
            Duration::from_millis(500),
        )
        .await;
        assert!(walks(&events).is_empty(), "{events:?}");
    }
    call(&writer, post(&format!("/api/v1/walk/{job}/cancel"), "")).await;
    dac.stop().await;
}

/// The page that does not ask for the walk is not sent it.
#[tokio::test]
async fn a_page_that_does_not_need_the_walk_is_not_sent_it() {
    let dac: FakeDaemon = fake_daemon("gungnir2-v2", "dac").await;
    let page = web(open(), Arc::clone(&dac.fleet));
    let job = started(&page).await;
    let body = stream(&page, "/api/v1/events").await;
    let events = crate::web::test_support::events_within(
        body,
        |events| !walks(events).is_empty(),
        Duration::from_millis(500),
    )
    .await;
    assert!(walks(&events).is_empty(), "{events:?}");
    call(&page, post(&format!("/api/v1/walk/{job}/cancel"), "")).await;
    dac.stop().await;
}

/// An op that names a run that is not the daemon's is refused `no_such_job` and touches nothing; the
/// walk it names runs on. An op naming the current run, or none, is let through.
#[tokio::test]
async fn an_op_naming_another_run_is_refused_and_the_walk_runs_on() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let page = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&page, post("/api/v1/walk", "")).await;
    let job = said(&reply.body)["job"].as_u64().expect("the job's id");
    let run = said(&reply.body)["run"].as_u64().expect("the run");
    let stale = run + 1;
    for tail in ["answer", "back", "cancel"] {
        let body = format!(r#"{{"choice":0,"run":{stale}}}"#);
        let refused = call(&page, post(&format!("/api/v1/walk/{job}/{tail}"), &body)).await;
        assert_eq!(refused.status, Code::NOT_FOUND, "{tail}: {}", refused.body);
        assert_eq!(said(&refused.body)["reason"], "no_such_job", "{tail}");
    }
    let odd = call(
        &page,
        post(&format!("/api/v1/walk/{job}/cancel"), r#"{"run":"x"}"#),
    )
    .await;
    assert_eq!(odd.status, Code::BAD_REQUEST, "{}", odd.body);
    // the walk still runs: a second start is busy
    let busy = call(&page, post("/api/v1/walk", "")).await;
    assert_eq!(busy.status, Code::CONFLICT, "{}", busy.body);
    // the current run is let through, as a body with no run is
    let ok = call(
        &page,
        post(
            &format!("/api/v1/walk/{job}/cancel"),
            &format!(r#"{{"run":{run}}}"#),
        ),
    )
    .await;
    assert_eq!(ok.status, Code::NO_CONTENT, "{}", ok.body);
}
