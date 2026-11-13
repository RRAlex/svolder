// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq routes over a real daemon whose client reaches a loopback server.

use std::sync::Arc;
use std::time::Duration;

use svolder_autoeq::test_server::{Reply, Server};
use svolder_autoeq::{Client, Limits};
use svolder_ipc::config::{WebConfig, WebGuest};
use svolder_ipc::{AutoEqOp, Event, Request};

use crate::web::test_support::{
    Code, FakeDaemon, call, fake_daemon_reaching, open, post, web, web_with,
};

const INDEX: &str = "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990\n\
- [Sennheiser HD 650](./crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650) by crinacle on GRAS 43AG-7\n";

const PEQ: &str = "Preamp: -6.1 dB\n\
Filter 1: ON LSC Fc 105 Hz Gain 6.0 dB Q 0.70\n\
Filter 2: ON PK Fc 1000 Hz Gain -2.0 dB Q 1.00\n";

const ORATORY: &str = r#"{"name":"Sennheiser HD 650","source":"oratory1990","form":"over-ear"}"#;
const CRINACLE: &str =
    r#"{"name":"Sennheiser HD 650","source":"crinacle","form":"over-ear","rig":"GRAS 43AG-7"}"#;

fn server() -> Server {
    Server::start(|path| match path {
        "/results/INDEX.md" => Reply::Ok(INDEX.as_bytes().to_vec()),
        p if p.ends_with("ParametricEQ.txt") => Reply::Ok(PEQ.as_bytes().to_vec()),
        _ => Reply::Status(404),
    })
}

/// The index cache of a test's daemon, removed when it drops, whether the test ends or fails.
struct Cache(std::path::PathBuf);

impl Drop for Cache {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A fake daemon reaching `server`, and its cache (kept by the caller for the test's length).
async fn daemon_for(server: &Server) -> (FakeDaemon, Cache) {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let cache =
        Cache(std::env::temp_dir().join(format!("svolderd-web-autoeq-{}-{n}", std::process::id())));
    let client = Client::new(server.url.clone(), cache.0.clone(), Limits::default());
    (
        fake_daemon_reaching("gungnir2-v2", "dac", client).await,
        cache,
    )
}

fn json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{body}: {e}"))
}

#[tokio::test]
async fn search_answers_the_groups() {
    let server = server();
    let (dac, _cache) = daemon_for(&server).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&web, post("/api/v1/autoeq/search", r#"{"query":"hd 650"}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said = json(&reply.body);
    let variants = &said["groups"][0]["variants"];
    assert_eq!(variants.as_array().unwrap().len(), 2, "{said}");
    assert_eq!(variants[1]["rig"], "GRAS 43AG-7");
    let long = format!(r#"{{"query":"{}"}}"#, "x".repeat(61));
    let reply = call(&web, post("/api/v1/autoeq/search", &long)).await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert_eq!(json(&reply.body)["reason"], "invalid");
    dac.stop().await;
}

#[tokio::test]
async fn preview_waits_for_its_job_and_answers_the_numbers() {
    let server = server();
    let (dac, _cache) = daemon_for(&server).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let body = format!(
        r#"{{"variant":{ORATORY},"target":{{"side":"device","device":"dac"}},"compare":{CRINACLE}}}"#
    );
    let reply = call(&web, post("/api/v1/autoeq/preview", &body)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said = json(&reply.body);
    assert_eq!(said["preview"]["target"].as_array().unwrap().len(), 181);
    assert!(said["preview"]["fit"]["rms_db"].is_number(), "{said}");
    assert!(said["compare"]["target"].is_array());
    let computer = format!(r#"{{"variant":{ORATORY},"target":{{"side":"computer"}}}}"#);
    let reply = call(&web, post("/api/v1/autoeq/preview", &computer)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(json(&reply.body)["preview"]["fit"].is_null());
    let unlisted =
        r#"{"variant":{"name":"Nope","source":"x","form":"in-ear"},"target":{"side":"computer"}}"#;
    let reply = call(&web, post("/api/v1/autoeq/preview", unlisted)).await;
    assert_eq!(reply.status, Code::NOT_FOUND, "{}", reply.body);
    assert_eq!(json(&reply.body)["reason"], "not_listed");
    dac.stop().await;
}

#[tokio::test]
async fn import_and_upload_save_a_preset_and_refuse_a_taken_name() {
    let server = server();
    let (dac, _cache) = daemon_for(&server).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let import = |name: &str, force: bool| {
        format!(
            r#"{{"variant":{ORATORY},"name":"{name}","force":{force},"target":{{"side":"computer"}}}}"#
        )
    };
    let reply = call(
        &web,
        post("/api/v1/autoeq/import", &import("HD 650", false)),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said = json(&reply.body);
    assert_eq!(
        (said["name"].as_str(), said["filters"].as_u64()),
        (Some("HD 650"), Some(2))
    );
    let reply = call(
        &web,
        post("/api/v1/autoeq/import", &import("hd 650", false)),
    )
    .await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert_eq!(json(&reply.body)["reason"], "taken");
    let reply = call(&web, post("/api/v1/autoeq/import", &import("hd 650", true))).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let upload = serde_json::json!({"text": PEQ, "name": "Mine"}).to_string();
    let reply = call(&web, post("/api/v1/autoeq/upload", &upload)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let text = std::fs::read_to_string(dac.dir.join("config.toml")).unwrap();
    assert!(text.contains("name = \"Mine\""), "{text}");
    dac.stop().await;
}

/// A guest is refused 401 and a read-only page 403 on every route, and nothing reaches the
/// repository.
#[tokio::test]
async fn only_a_signed_in_writer_reaches_the_repository() {
    let server = server();
    let (dac, _cache) = daemon_for(&server).await;
    let token = "a-test-token-0123456789abcdefghijklmn";
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some(token),
    );
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    for (path, body) in [
        ("search", r#"{"query":"hd"}"#),
        ("preview", "{}"),
        ("import", "{}"),
        ("upload", "{}"),
    ] {
        let url = format!("/api/v1/autoeq/{path}");
        assert_eq!(
            call(&guests, post(&url, body)).await.status,
            Code::UNAUTHORIZED,
            "{path}"
        );
        assert_eq!(
            call(&read_only, post(&url, body)).await.status,
            Code::FORBIDDEN,
            "{path}"
        );
    }
    assert_eq!(server.hits(), 0);
    dac.stop().await;
}

#[tokio::test]
async fn a_body_that_is_not_the_ops_is_a_400_and_an_oversize_one_a_413() {
    let server = server();
    let (dac, _cache) = daemon_for(&server).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    for body in ["[]", "nope", r#"{"query":3}"#, r#"{"nothing":true}"#] {
        let reply = call(&web, post("/api/v1/autoeq/search", body)).await;
        assert_eq!(reply.status, Code::BAD_REQUEST, "{body}: {}", reply.body);
    }
    let big = format!(r#"{{"query":"{}"}}"#, "x".repeat(20 * 1024));
    let reply = call(&web, post("/api/v1/autoeq/search", &big)).await;
    assert_eq!(reply.status, Code::PAYLOAD_TOO_LARGE, "{}", reply.body);
    let text = "# c\n".repeat(20_000);
    assert!(text.len() < 128 * 1024 && text.len() > 64 * 1024);
    let upload = serde_json::json!({"text": text, "name": "Big"}).to_string();
    let reply = call(&web, post("/api/v1/autoeq/upload", &upload)).await;
    assert_eq!(
        reply.status,
        Code::UNPROCESSABLE_ENTITY,
        "the daemon refuses: {}",
        reply.body
    );
    let huge = serde_json::json!({"text": "a".repeat(130 * 1024), "name": "Big"}).to_string();
    let reply = call(&web, post("/api/v1/autoeq/upload", &huge)).await;
    assert_eq!(reply.status, Code::PAYLOAD_TOO_LARGE, "{}", reply.body);
    dac.stop().await;
}

/// A client that goes while its job waits for the repository cancels the job: the daemon's own
/// stream sees it end as cancelled, and its slot is free.
#[tokio::test]
async fn the_job_is_cancelled_when_the_client_goes() {
    let silent = Server::start(|path| match path {
        "/results/INDEX.md" => Reply::Ok(INDEX.as_bytes().to_vec()),
        _ => Reply::Silent,
    });
    let (dac, _cache) = daemon_for(&silent).await;
    let mut events = dac
        .fleet
        .stream(Request::AutoEq {
            op: AutoEqOp::Follow,
        })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the stream");
    let web = web(open(), Arc::clone(&dac.fleet));
    let body = format!(r#"{{"variant":{ORATORY},"target":{{"side":"computer"}}}}"#);
    // The client gives up after a moment: the handler's future is dropped.
    let gone = tokio::time::timeout(
        Duration::from_millis(500),
        call(&web, post("/api/v1/autoeq/preview", &body)),
    )
    .await;
    assert!(gone.is_err(), "the route should still be waiting");
    use futures::StreamExt;
    let ended = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match events.next().await {
                Some(Event::AutoEq(event)) if event.state.is_final() => return event.state,
                Some(_) => {}
                None => panic!("the stream closed"),
            }
        }
    })
    .await
    .expect("the job ended");
    assert_eq!(ended, svolder_ipc::JobState::Cancelled);
    dac.stop().await;
}
