// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use svolder_ipc::config::{WebConfig, WebGuest};
use svolder_ipc::socket::protocol::SceneOp;
use svolder_ipc::{Request, Response};
use tower::ServiceExt;

use super::*;
use crate::web::router::router;
use crate::web::test_support::{
    Code, FakeDaemon, call, events_until, events_within, fake_daemon, get, open, page_model, post,
    web, web_with,
};
use crate::writer::Origin;

/// The body of a reply as JSON.
fn json(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {body}"))
}

/// The one event named `name` among `seen`, as JSON.
fn event(seen: &[(String, String)], name: &str) -> Option<Value> {
    seen.iter()
        .find(|(n, _)| n == name)
        .map(|(_, data)| json(data))
}

/// A web page over a guest-readable config (a token is asked for) and one that is read-only.
fn guest_and_read_only(dac: &FakeDaemon) -> (Web, Web) {
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
    (guests, read_only)
}

/// Save the device's settings as `name` through the daemon, once it has read them all: waited for
/// on the device's feed, not on the clock.
async fn saved_through_the_daemon(dac: &FakeDaemon, name: &str) {
    let mut feed = dac.daemon.devices()[0].feed.clone();
    let read = tokio::time::timeout(
        Duration::from_secs(10),
        feed.wait_for(|feed| {
            feed.profile.as_ref().is_some_and(|profile| {
                svolder_profiles::scene::capture(
                    profile,
                    "0102030405060708",
                    "dac",
                    &[],
                    None,
                    svolder_profiles::scene::raw_of(&feed.readings),
                )
                .is_ok()
            })
        }),
    )
    .await
    // The borrow `wait_for` hands back holds the feed's lock: it is let go before anything writes.
    .map(|waited| waited.is_ok());
    assert_eq!(read, Ok(true), "the device never read its settings");
    let answer = dac
        .daemon
        .handle_from(
            Origin::Socket,
            Request::Scene {
                op: SceneOp::Save {
                    name: name.into(),
                    force: false,
                },
            },
        )
        .await;
    assert!(matches!(answer, Response::Scene { .. }), "{answer:?}");
}

/// Every route answers as the contract says: `{"name"}`, a recall's `outcome`, 422 for a name
/// that is bad or taken, 409 for any other refusal, each with its `reason`.
#[tokio::test]
async fn the_routes_answer_names_outcomes_and_reasons() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    saved_through_the_daemon(&dac, "First").await;
    let web = web(open(), Arc::clone(&dac.fleet));

    let listed = call(&web, get("/api/v1/scenes")).await;
    assert_eq!(listed.status, Code::OK, "{}", listed.body);
    let view = json(&listed.body);
    assert_eq!(view["scenes"][0]["name"], "First", "{view}");
    assert_eq!(view["scenes"][0]["parts"][0]["who"], "dac", "{view}");
    assert_eq!(view["known"], true);

    let saved = call(&web, post("/api/v1/scenes/save", r#"{"name":"  Second "}"#)).await;
    assert_eq!(saved.status, Code::OK, "{}", saved.body);
    assert_eq!(
        json(&saved.body),
        serde_json::json!({ "name": "Second", "outcome": [] })
    );

    let taken = call(&web, post("/api/v1/scenes/save", r#"{"name":"second"}"#)).await;
    assert_eq!(taken.status, Code::UNPROCESSABLE_ENTITY, "{}", taken.body);
    assert_eq!(json(&taken.body)["reason"], "taken");
    let forced = call(
        &web,
        post("/api/v1/scenes/save", r#"{"name":"second","force":true}"#),
    )
    .await;
    assert_eq!(forced.status, Code::OK, "{}", forced.body);
    let bad = call(&web, post("/api/v1/scenes/save", r#"{"name":"custom"}"#)).await;
    assert_eq!(bad.status, Code::UNPROCESSABLE_ENTITY, "{}", bad.body);
    assert_eq!(json(&bad.body)["reason"], "bad_name");

    let recalled = call(&web, post("/api/v1/scenes/recall", r#"{"name":"first"}"#)).await;
    assert_eq!(recalled.status, Code::OK, "{}", recalled.body);
    let said = json(&recalled.body);
    assert_eq!(said["name"], "First");
    assert_eq!(said["outcome"][0]["who"], "dac", "{said}");
    assert_eq!(said["outcome"][0]["status"], "done", "{said}");
    let missing = call(&web, post("/api/v1/scenes/recall", r#"{"name":"Nope"}"#)).await;
    assert_eq!(missing.status, Code::CONFLICT, "{}", missing.body);
    assert_eq!(json(&missing.body)["reason"], "missing");

    let renamed = call(
        &web,
        post("/api/v1/scenes/rename", r#"{"from":"Second","to":"Third"}"#),
    )
    .await;
    assert_eq!(renamed.status, Code::OK, "{}", renamed.body);
    assert_eq!(json(&renamed.body), serde_json::json!({ "name": "Third" }));
    let gone = call(&web, post("/api/v1/scenes/delete", r#"{"name":"third"}"#)).await;
    assert_eq!(gone.status, Code::OK, "{}", gone.body);
    assert_eq!(json(&gone.body), serde_json::json!({ "name": "Third" }));
    let again = call(&web, post("/api/v1/scenes/delete", r#"{"name":"third"}"#)).await;
    assert_eq!(again.status, Code::CONFLICT, "{}", again.body);

    let junk = call(&web, post("/api/v1/scenes/save", r#"{"nom":"x"}"#)).await;
    assert_eq!(junk.status, Code::BAD_REQUEST, "{}", junk.body);
    dac.stop().await;
}

/// The both-sides question is the computer route's shape in scene words: a 409 with `ask`.
#[test]
fn the_both_sides_question_is_a_409_with_an_ask() {
    let question = "Late night leaves Warm on the Desk DAC and on the computer: it plays twice. \
                    Recall it anyway?";
    let reply = outcome(Answer::Ask {
        message: question.into(),
        name: "Warm".into(),
        devices: vec!["Desk DAC".into()],
    });
    assert_eq!(reply.status(), Code::CONFLICT);
    let body = axum::body::to_bytes(reply.into_body(), usize::MAX);
    let body = futures::executor::block_on(body).expect("a body");
    let said = json(std::str::from_utf8(&body).unwrap());
    assert_eq!(said["error"], question);
    assert_eq!(
        said["ask"],
        serde_json::json!({
            "kind": "both_sides",
            "name": "Warm",
            "devices": ["Desk DAC"],
            "question": question,
        })
    );
}

/// A guest is refused 401 and a read-only page 403, on every scenes route, the list included.
#[tokio::test]
async fn a_guest_and_a_read_only_page_get_none_of_it() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let (guests, read_only) = guest_and_read_only(&dac);
    for (path, body) in [
        ("/api/v1/scenes/save", r#"{"name":"A"}"#),
        ("/api/v1/scenes/recall", r#"{"name":"A"}"#),
        ("/api/v1/scenes/rename", r#"{"from":"A","to":"B"}"#),
        ("/api/v1/scenes/delete", r#"{"name":"A"}"#),
    ] {
        let guest = call(&guests, post(path, body)).await;
        assert_eq!(guest.status, Code::UNAUTHORIZED, "{path}");
        let off = call(&read_only, post(path, body)).await;
        assert_eq!(off.status, Code::FORBIDDEN, "{path}");
    }
    let guest = call(&guests, get("/api/v1/scenes")).await;
    assert_eq!(guest.status, Code::UNAUTHORIZED);
    let off = call(&read_only, get("/api/v1/scenes")).await;
    assert_eq!(off.status, Code::FORBIDDEN);
    dac.stop().await;
}

/// A writer's page model carries `scenes`; a guest's and a read-only page's does not.
#[tokio::test]
async fn only_a_writers_page_model_has_the_scenes() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    saved_through_the_daemon(&dac, "First").await;
    let writer = call(&web(open(), Arc::clone(&dac.fleet)), get("/")).await;
    let model = page_model(&writer.body);
    assert_eq!(model["scenes"]["scenes"][0]["name"], "First", "{model}");
    let (guests, read_only) = guest_and_read_only(&dac);
    for page in [guests, read_only] {
        let model = page_model(&call(&page, get("/")).await.body);
        assert!(model.get("scenes").is_none(), "{model}");
    }
    dac.stop().await;
}

/// A writer's stream sends `scenes` when the view changes, and a guest's and a read-only page's
/// stream never does.
#[tokio::test]
async fn the_scenes_event_goes_to_a_writer_and_to_nobody_else() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let (guests, read_only) = guest_and_read_only(&dac);
    let open_web = web(open(), Arc::clone(&dac.fleet));
    let stream = |page: &Web| {
        let page = page.clone();
        async move {
            router(page)
                .oneshot(get("/api/v1/events?device=dac"))
                .await
                .unwrap()
                .into_body()
        }
    };
    let writer_body = stream(&open_web).await;
    let guest_body = stream(&guests).await;
    let off_body = stream(&read_only).await;
    // Each stream is open (its model is the first thing it sent) before the change.
    saved_through_the_daemon(&dac, "First").await;

    let seen = events_until(writer_body, |seen| {
        seen.iter().any(|(name, _)| name == "scenes")
    })
    .await;
    let sent = event(&seen, "scenes").expect("a scenes event");
    assert_eq!(sent["scenes"][0]["name"], "First", "{sent}");
    for body in [guest_body, off_body] {
        let seen = events_within(body, |_| false, Duration::from_millis(1500)).await;
        assert!(seen.iter().any(|(name, _)| name == "model"), "{seen:?}");
        assert!(
            seen.iter()
                .all(|(name, data)| name != "scenes" && !data.contains("\"First\"")),
            "{seen:?}"
        );
    }
    dac.stop().await;
}

/// A daemon that has gone answers the list with a reason, never a bare 409.
#[tokio::test]
async fn the_list_of_a_fleet_with_no_scenes_says_why() {
    let web = web(open(), Arc::new(crate::fleet::Unavailable));
    let reply = call(&web, get("/api/v1/scenes")).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert_eq!(json(&reply.body)["reason"], "stopping");
}

/// Nothing to save is a 409 with its own reason.
#[test]
fn nothing_to_save_is_a_409_with_its_reason() {
    let reply = refused(Answer::Error {
        code: svolder_ipc::CODE_USAGE,
        message: "nothing to save".into(),
        reason: Some(svolder_ipc::Reason::NothingToSave),
    });
    assert_eq!(reply.status(), Code::CONFLICT);
    let body = futures::executor::block_on(axum::body::to_bytes(reply.into_body(), usize::MAX))
        .expect("a body");
    assert_eq!(
        json(std::str::from_utf8(&body).unwrap())["reason"],
        "nothing_to_save"
    );
}
