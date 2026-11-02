// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::time::Duration;

use svolder_ipc::config::{WebConfig, WebGuest};

use futures::StreamExt;
use tower::ServiceExt;

use crate::web::auth::{cookie_name, mint, now_secs};
use crate::web::router::router;
use crate::web::test_support::{
    Code, FakeDaemon, call, events_until, events_within, fake_daemon, fake_daemon_playing, get,
    open, post, web, web_over, web_with,
};

/// The hash of the fake player's cover, once the daemon has read it.
async fn cover_hash(daemon: &FakeDaemon) -> String {
    for _ in 0..200 {
        let hash = daemon
            .fleet
            .host_view()
            .playing
            .and_then(|playing| playing.cover);
        if let Some(hash) = hash {
            return hash;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the fake cover was never read");
}

/// The cover is served under its hash: the picture, its type, an ETag that is the hash, and a
/// cache the browser may keep; a second ask with the ETag is a 304.
#[tokio::test]
async fn the_cover_is_served_under_its_hash_with_an_etag() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let hash = cover_hash(&dac).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let path = format!("/api/v1/host/cover?v={hash}");
    let reply = call(&web, get(&path)).await;
    assert_eq!(reply.status, Code::OK);
    assert_eq!(reply.header("content-type"), Some("image/png"));
    assert_eq!(reply.header("etag"), Some(format!("\"{hash}\"").as_str()));
    assert_eq!(
        reply.header("cache-control"),
        Some("private, max-age=31536000, immutable")
    );
    assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
    assert!(reply.body.starts_with("\u{fffd}PNG"), "a PNG's first bytes");
    let mut again = get(&path);
    again
        .headers_mut()
        .insert("if-none-match", format!("\"{hash}\"").parse().unwrap());
    let reply = call(&web, again).await;
    assert_eq!(reply.status, Code::NOT_MODIFIED);
    assert_eq!(reply.header("etag"), Some(format!("\"{hash}\"").as_str()));
    dac.stop().await;
}

/// A browser's or a proxy's `If-None-Match` is answered 304 when it names the cover's tag in any
/// of the forms the header allows: a weak validator, a list, `*`; another tag gets the picture.
#[tokio::test]
async fn the_etag_is_matched_in_every_form_of_if_none_match() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let hash = cover_hash(&dac).await;
    let web = web_over(&dac);
    let path = format!("/api/v1/host/cover?v={hash}");
    for (header, status) in [
        (format!("\"{hash}\""), Code::NOT_MODIFIED),
        (format!("W/\"{hash}\""), Code::NOT_MODIFIED),
        (format!("\"other\", \"{hash}\""), Code::NOT_MODIFIED),
        (format!("W/\"other\" , W/\"{hash}\""), Code::NOT_MODIFIED),
        ("*".to_string(), Code::NOT_MODIFIED),
        ("\"other\"".to_string(), Code::OK),
        (format!("\"{hash}x\""), Code::OK),
        (String::new(), Code::OK),
    ] {
        let mut request = get(&path);
        request
            .headers_mut()
            .insert("if-none-match", header.parse().unwrap());
        assert_eq!(call(&web, request).await.status, status, "{header:?}");
    }
    dac.stop().await;
}

/// Only the current cover is served: another hash, none, or an odd one is a 404, and so is a
/// host with no cover at all.
#[tokio::test]
async fn any_other_hash_is_a_404() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let hash = cover_hash(&dac).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    for path in [
        "/api/v1/host/cover".to_string(),
        "/api/v1/host/cover?v=".to_string(),
        "/api/v1/host/cover?v=0000000000000000".to_string(),
        format!("/api/v1/host/cover?v={}x", hash),
        "/api/v1/host/cover?v=../../etc/passwd".to_string(),
    ] {
        assert_eq!(
            call(&web, get(&path)).await.status,
            Code::NOT_FOUND,
            "{path}"
        );
    }
    dac.stop().await;
    let none = fake_daemon("gungnir2-v2", "dac").await;
    let web = web_over(&none);
    let reply = call(&web, get(&format!("/api/v1/host/cover?v={hash}"))).await;
    assert_eq!(reply.status, Code::NOT_FOUND, "no host, no cover");
    none.stop().await;
}

/// The cover is read like the rest of the page: a visitor with no sign-in and no guest access
/// is refused, a guest may read it.
#[tokio::test]
async fn the_cover_is_for_readers() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let hash = cover_hash(&dac).await;
    let token = "a-test-token-0123456789abcdefghijklmn";
    let path = format!("/api/v1/host/cover?v={hash}");
    let closed = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some(token),
    );
    assert_eq!(call(&closed, get(&path)).await.status, Code::UNAUTHORIZED);
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some(token),
    );
    assert_eq!(call(&guests, get(&path)).await.status, Code::OK);
    dac.stop().await;
}

/// A press reaches the player: the fake takes it, and the view says so.
#[tokio::test]
async fn a_press_reaches_the_player() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    cover_hash(&dac).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&web, post("/api/v1/host/press", r#"{"button":"pause"}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(reply.body.contains("\"ok\":true"), "{}", reply.body);
    let status = |daemon: &FakeDaemon| daemon.fleet.host_view().playing.map(|p| p.status);
    for _ in 0..100 {
        if status(&dac).as_deref() == Some("paused") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(status(&dac).as_deref(), Some("paused"));
    dac.stop().await;
}

/// A button that is none of the four is refused 422 before the daemon is asked, a body that is
/// not what the endpoint takes 400, and a daemon with no player says so with a 409.
#[tokio::test]
async fn odd_presses_are_refused_with_words() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&web, post("/api/v1/host/press", r#"{"button":"stop"}"#)).await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert!(reply.body.contains("no button"), "{}", reply.body);
    let reply = call(&web, post("/api/v1/host/press", r#"{"push":"play"}"#)).await;
    assert_eq!(reply.status, Code::BAD_REQUEST, "{}", reply.body);
    dac.stop().await;
    let none = fake_daemon("gungnir2-v2", "dac").await;
    let web = web_over(&none);
    let reply = call(&web, post("/api/v1/host/press", r#"{"button":"play"}"#)).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert!(
        reply.body.contains("host audio is not available"),
        "{}",
        reply.body
    );
    none.stop().await;
}

/// A guest's press is refused 401, a page with writes off 403, and the player is not touched.
#[tokio::test]
async fn a_guest_and_a_read_only_page_cannot_press() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    cover_hash(&dac).await;
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
    let reply = call(&guests, post("/api/v1/host/press", r#"{"button":"pause"}"#)).await;
    assert_eq!(reply.status, Code::UNAUTHORIZED, "{}", reply.body);
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    let reply = call(
        &read_only,
        post("/api/v1/host/press", r#"{"button":"pause"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::FORBIDDEN, "{}", reply.body);
    assert_eq!(
        dac.fleet.host_view().playing.map(|p| p.status).as_deref(),
        Some("playing")
    );
    dac.stop().await;
}

/// The frames stream opens with the reconnect delay and a `hello` that carries the daemon's
/// clock and where the capture stands, then the capture's changes and the frames, each frame
/// in the wire shape: two base64 strings of 240 bands, the levels, the beat, the tempo and the
/// phase.
#[tokio::test]
async fn the_frames_stream_opens_with_hello_and_the_daemon_clock() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&dac);
    let before = svolder_host::unix_ms();
    let response = router(web)
        .oneshot(get("/api/v1/host/frames"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap()),
        Some("text/event-stream")
    );
    let seen = events_until(response.into_body(), |seen| {
        seen.iter().filter(|(name, _)| name == "frame").count() >= 3
    })
    .await;
    let after = svolder_host::unix_ms();
    assert_eq!(
        seen[0],
        ("retry".to_string(), "2000".to_string()),
        "{seen:?}"
    );
    // A comment sits between them and the first event; `hello` is the first one named.
    let named: Vec<&(String, String)> = seen
        .iter()
        .filter(|(name, _)| name != "message" && name != "retry")
        .collect();
    assert_eq!(named[0].0, "hello", "{seen:?}");
    let hello: serde_json::Value = serde_json::from_str(&named[0].1).unwrap();
    let now = hello["now_ms"].as_u64().expect("the daemon's clock");
    assert!(
        (before..=after).contains(&now),
        "{now} not in {before}..={after}"
    );
    assert!(
        ["starting", "running"].contains(&hello["capture"].as_str().unwrap()),
        "{hello}"
    );

    let capture = seen
        .iter()
        .find(|(name, data)| name == "capture" && data.contains("running"))
        .or_else(|| (hello["capture"] == "running").then_some(named[0]))
        .expect("the capture was running");
    let capture: serde_json::Value = serde_json::from_str(&capture.1).unwrap();
    assert_eq!(capture["sink"], "Schiit Gungnir 2");
    assert_eq!(capture["why"], serde_json::Value::Null);

    let frames: Vec<serde_json::Value> = seen
        .iter()
        .filter(|(name, _)| name == "frame")
        .map(|(_, data)| serde_json::from_str(data).unwrap())
        .collect();
    assert!(frames.len() >= 3, "{seen:?}");
    for frame in &frames {
        let t = frame["t"].as_u64().expect("the capture time");
        assert!(t + 1000 >= before && t <= after + 1000, "{t}");
        for side in ["l", "r"] {
            let bytes = frame[side].as_str().expect("a base64 string");
            assert_eq!(bytes.len(), 320, "240 bands in base64: {bytes}");
        }
        assert_eq!(frame["lv"].as_array().map(Vec::len), Some(4));
        assert!(frame["beat"] == 0 || frame["beat"] == 1, "{frame}");
        assert!(frame["bpm"].is_null() || frame["bpm"].is_u64(), "{frame}");
        assert!(frame["phase"].is_u64(), "{frame}");
    }
    dac.stop().await;
}

/// A host that is off has no frames: the route is a 404, as for a device that is not there.
#[tokio::test]
async fn no_host_no_frames_route() {
    let none = fake_daemon("gungnir2-v2", "dac").await;
    let web = web_over(&none);
    assert_eq!(
        call(&web, get("/api/v1/host/frames")).await.status,
        Code::NOT_FOUND
    );
    none.stop().await;
}

/// A guest reads the stream as it does the page; a visitor with neither sign-in nor guest
/// access is refused.
#[tokio::test]
async fn a_guest_may_watch() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let token = "a-test-token-0123456789abcdefghijklmn";
    let closed = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some(token),
    );
    assert_eq!(
        call(&closed, get("/api/v1/host/frames")).await.status,
        Code::UNAUTHORIZED
    );
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some(token),
    );
    let response = router(guests)
        .oneshot(get("/api/v1/host/frames"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let seen = events_until(response.into_body(), |seen| {
        seen.iter().any(|(name, _)| name == "hello")
    })
    .await;
    assert!(seen.iter().any(|(name, _)| name == "hello"), "{seen:?}");
    dac.stop().await;
}

/// A rotated token ends an open frames stream at once, as it does the device's.
#[tokio::test]
async fn a_rotated_token_ends_the_frames_stream() {
    const TOKEN: &str = "a-test-token-0123456789abcdefghijklmn";
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some(TOKEN),
    );
    let mut request = get("/api/v1/host/frames");
    let cookie = format!("{}={}", cookie_name(8738), mint(TOKEN, now_secs()));
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let response = router(web.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
    let mut body = response.into_body().into_data_stream();
    let first = tokio::time::timeout(Duration::from_secs(5), body.next()).await;
    assert!(matches!(first, Ok(Some(Ok(_)))), "no first event");
    web.rekey_value("a-new-token-0123456789abcdefghijklmn", None);
    let ended = tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(Ok(_)) = body.next().await {}
    })
    .await;
    assert!(ended.is_ok(), "the stream outlived the rotation");
    assert_eq!(
        dac.daemon.host().frame_holds(),
        0,
        "and let go of the capture"
    );
    dac.stop().await;
}

/// A page that goes away lets the capture go: its stream held it, and the hold ends with the
/// connection.
#[tokio::test]
async fn closing_the_frames_stream_lets_the_capture_go() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&dac);
    let response = router(web)
        .oneshot(get("/api/v1/host/frames"))
        .await
        .unwrap();
    let _ = events_within(
        response.into_body(),
        |seen| seen.iter().any(|(name, _)| name == "frame"),
        Duration::from_secs(5),
    )
    .await;
    // The body was dropped by the helper: the stream's task ends and its hold with it.
    for _ in 0..100 {
        if dac.daemon.host().frame_holds() == 0 {
            dac.stop().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the hold outlived the stream");
}

/// A real server over a daemon whose capture is unavailable, with a page's frames stream open
/// on a real connection and its `hello` read: the server, the connection and what it said.
async fn unavailable_frames_page(
    dac: &FakeDaemon,
) -> (crate::web::WebServer, tokio::net::TcpStream, String) {
    use crate::http_slot::Server;
    use svolder_ipc::WebStatus;
    use svolder_ipc::config::{WebAuth, WebConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    dac.daemon
        .host()
        .run_with(crate::host::Parts::fake(crate::host::FakeKnobs {
            capture_off: true,
            ..crate::host::FakeKnobs::default()
        }));
    let config = WebConfig {
        enabled: true,
        listen: "127.0.0.1:0".parse().unwrap(),
        auth: WebAuth::None,
        ..WebConfig::default()
    };
    let server =
        crate::web::WebServer::start(&config, Arc::clone(&dac.fleet), Default::default()).await;
    let WebStatus::Listening { listen, .. } = server.status() else {
        panic!("not listening: {:?}", server.status());
    };
    let mut stream = tokio::net::TcpStream::connect(listen).await.unwrap();
    let request = format!(
        "GET /api/v1/host/frames HTTP/1.1\r\nHost: {listen}\r\n\
         Accept: text/event-stream\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut seen = String::new();
    let mut chunk = [0u8; 1024];
    let heard = tokio::time::timeout(Duration::from_secs(5), async {
        while !seen.contains("event: hello") {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0, "the stream closed: {seen}");
            seen.push_str(&String::from_utf8_lossy(&chunk[..n]));
        }
    })
    .await;
    assert!(heard.is_ok(), "no hello: {seen}");
    assert!(seen.contains("unavailable"), "{seen}");
    (server, stream, seen)
}

/// A page that closes its connection while the capture says nothing (it is unavailable, so no
/// frame is written and no write could fail) lets the capture go within a second: the hold ends
/// with the connection, not with the next write.
#[tokio::test]
async fn a_closed_connection_lets_the_capture_go_while_no_frames_flow() {
    use crate::http_slot::Server;
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let (server, stream, _) = unavailable_frames_page(&dac).await;
    assert_eq!(
        dac.daemon.host().frame_holds(),
        1,
        "the page holds the capture"
    );
    drop(stream);
    let mut gone = false;
    for _ in 0..40 {
        if dac.daemon.host().frame_holds() == 0 {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    server.stop().await;
    dac.stop().await;
    assert!(gone, "the hold outlived the closed connection");
}

/// An idle frames stream (no frames flow) writes a keep-alive comment every 3 s, so a connection
/// that went away without a close is found by a write within that.
#[tokio::test]
async fn an_idle_frames_stream_writes_a_keep_alive_within_4_s() {
    use crate::http_slot::Server;
    use tokio::io::AsyncReadExt;
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let (server, mut stream, _) = unavailable_frames_page(&dac).await;
    let mut chunk = [0u8; 256];
    let heard = tokio::time::timeout(Duration::from_secs(4), async {
        let n = stream.read(&mut chunk).await.unwrap();
        String::from_utf8_lossy(&chunk[..n]).into_owned()
    })
    .await;
    server.stop().await;
    dac.stop().await;
    let text = heard.expect("nothing written for 4 s");
    assert!(text.contains(':'), "a comment line: {text:?}");
}
