// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use svolder_host::Frame;

use axum::body::Body;
use tower::ServiceExt;

use super::*;
use crate::host::CaptureState;
use crate::web::router::router;
use crate::web::test_support::{events_until, fake_daemon_playing, get, web_over};

/// The test vectors of RFC 4648 (section 10), and what a byte string of every length mod 3 pads to.
#[test]
fn the_encoder_matches_known_base64() {
    for (plain, encoded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64(plain.as_bytes()), encoded, "{plain:?}");
    }
    // Every byte value, so the alphabet's `+` and `/` are reached.
    let all: Vec<u8> = (0..=255).collect();
    let encoded = base64(&all);
    assert_eq!(encoded.len(), 344);
    assert!(
        encoded.starts_with("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4v")
    );
    assert!(encoded.ends_with("/P3+/w=="), "{encoded}");
    // 240 bytes need no padding: 320 characters.
    assert_eq!(base64(&[0; 240]).len(), 320);
}

/// A frame goes out in the shape the page reads: `t`, `l`, `r`, `lv`, `beat` as 0 or 1, `bpm`
/// a number or null, `phase`.
#[test]
fn a_frame_is_json_in_the_wire_shape() {
    let mut frame = Frame {
        capture_ms: 1_700_000_000_123,
        left: [0; 240],
        right: [255; 240],
        levels: [1, 2, 3, 4],
        beat: true,
        bpm: None,
        phase: 9,
    };
    frame.left[0] = 1;
    let json: serde_json::Value = serde_json::from_str(&frame_json(&frame, false)).unwrap();
    assert_eq!(json["t"], 1_700_000_000_123u64);
    assert!(json["l"].as_str().unwrap().starts_with("AQAA"), "{json}");
    assert!(json["r"].as_str().unwrap().starts_with("////"), "{json}");
    assert_eq!(json["lv"], serde_json::json!([1, 2, 3, 4]));
    assert_eq!(json["beat"], 1);
    assert_eq!(json["bpm"], serde_json::Value::Null);
    assert_eq!(json["phase"], 9);
    frame.beat = false;
    frame.bpm = Some(120);
    let json: serde_json::Value = serde_json::from_str(&frame_json(&frame, false)).unwrap();
    assert_eq!(
        (json["beat"].clone(), json["bpm"].clone()),
        (0.into(), 120.into())
    );
}

/// The capture's three states in the words the page reads, the sink and the reason being null
/// when they do not apply; `hello` is the same with the daemon's clock first.
#[test]
fn the_capture_states_are_json_in_the_wire_shape() {
    let running = CaptureState::Running {
        sink: "Schiit Gungnir 2".to_string(),
        target: "alsa_output.usb".to_string(),
    };
    let json: serde_json::Value = serde_json::from_str(&capture_json(&running, None)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"capture": "running", "sink": "Schiit Gungnir 2", "why": null})
    );
    let json: serde_json::Value =
        serde_json::from_str(&capture_json(&CaptureState::Starting, Some(42))).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"now_ms": 42, "capture": "starting", "sink": null, "why": null})
    );
    let gone = CaptureState::Unavailable {
        why: "pw-record could not be started".to_string(),
    };
    let json: serde_json::Value = serde_json::from_str(&capture_json(&gone, None)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "capture": "unavailable",
            "sink": null,
            "why": "pw-record could not be started"
        })
    );
}

/// Sets a flag when it is dropped.
struct Guard(Arc<AtomicBool>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// A page that stopped reading, with a capture event waiting to be sent, is let go of at once by
/// a rotated token: the stream (and the capture hold it owns) ends, though the queue is full and
/// no client is there to drain it.
#[tokio::test]
async fn a_rotated_token_ends_a_stream_stuck_on_a_state_event() {
    use futures::StreamExt;
    let dac = crate::web::test_support::fake_daemon("gungnir2-v2", "dac").await;
    let web = crate::web::test_support::web_with(
        svolder_ipc::config::WebConfig {
            enabled: true,
            ..Default::default()
        },
        Arc::clone(&dac.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let released = Arc::new(AtomicBool::new(false));
    let guard = Guard(Arc::clone(&released));
    let events = futures::stream::iter((0..20).map(|n| {
        FramesEvent::Capture(CaptureState::Unavailable {
            why: format!("state {n}"),
        })
    }))
    .chain(futures::stream::pending())
    .map(move |event| {
        let _held = &guard;
        event
    })
    .boxed();
    // A queue of one that nobody reads: the hello fills it and the next state waits for room.
    let (tx, _rx) = mpsc::channel(1);
    let signed_out = web.signed_out.subscribe();
    let following = tokio::spawn(follow(web.clone(), events, signed_out, tx, false));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!following.is_finished(), "it waits on the full queue");
    assert!(!released.load(Ordering::SeqCst));

    web.rekey_value("a-new-token-0123456789abcdefghijklmn", None);
    tokio::time::timeout(Duration::from_secs(1), following)
        .await
        .expect("the stream outlived the rotation")
        .unwrap();
    assert!(released.load(Ordering::SeqCst), "the hold was not let go");
    dac.stop().await;
}

/// The query's `only` selects the levels-only form for the value `levels` alone; a missing
/// query, another value or another key leaves the full frames.
#[test]
fn only_levels_is_read_from_the_query() {
    assert!(levels_only(Some("only=levels")));
    assert!(levels_only(Some("x=1&only=levels")));
    for query in [
        None,
        Some(""),
        Some("only="),
        Some("only=bands"),
        Some("only=Levels"),
        Some("only=levels2"),
        Some("other=levels"),
        Some("only"),
    ] {
        assert!(!levels_only(query), "{query:?}");
    }
}

/// The levels-only frame carries exactly `t` and `lv`; the full frame is unchanged.
#[test]
fn a_levels_only_frame_has_exactly_t_and_lv() {
    let frame = Frame {
        capture_ms: 1_700_000_000_123,
        left: [7; 240],
        right: [8; 240],
        levels: [10, 20, 255, 254],
        beat: true,
        bpm: Some(120),
        phase: 9,
    };
    let json: serde_json::Value = serde_json::from_str(&frame_json(&frame, true)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"t": 1_700_000_000_123u64, "lv": [10, 20, 255, 254]})
    );
    let full: serde_json::Value = serde_json::from_str(&frame_json(&frame, false)).unwrap();
    let mut keys: Vec<&String> = full.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["beat", "bpm", "l", "lv", "phase", "r", "t"]);
}

/// The route's events up to the third frame, for the query `query`.
async fn route_events(query: &str) -> Vec<(String, String)> {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let response = router(web_over(&dac))
        .oneshot(get(&format!("/api/v1/host/frames{query}")))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Body = response.into_body();
    let seen = events_until(body, |seen| {
        seen.iter().filter(|(name, _)| name == "frame").count() >= 3
    })
    .await;
    dac.stop().await;
    seen
}

/// `?only=levels` sends the same `hello` and `capture` events, then frames with exactly `t` and
/// `lv`.
#[tokio::test]
async fn only_levels_streams_hello_capture_and_light_frames() {
    let seen = route_events("?only=levels").await;
    let named: Vec<&(String, String)> = seen
        .iter()
        .filter(|(name, _)| name != "message" && name != "retry")
        .collect();
    assert_eq!(named[0].0, "hello", "{seen:?}");
    let hello: serde_json::Value = serde_json::from_str(&named[0].1).unwrap();
    assert!(hello["now_ms"].is_u64(), "{hello}");
    assert!(named.iter().any(|(name, _)| name == "capture"), "{seen:?}");
    let frames: Vec<serde_json::Value> = seen
        .iter()
        .filter(|(name, _)| name == "frame")
        .map(|(_, data)| serde_json::from_str(data).unwrap())
        .collect();
    assert!(frames.len() >= 3, "{seen:?}");
    for frame in &frames {
        let mut keys: Vec<&String> = frame.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["lv", "t"], "{frame}");
        assert_eq!(frame["lv"].as_array().map(Vec::len), Some(4));
    }
}

/// With no query, or a value that is not `levels`, the events are today's full frames.
#[tokio::test]
async fn no_query_or_a_bad_value_streams_full_frames() {
    for query in ["", "?only=bands", "?only="] {
        let seen = route_events(query).await;
        let frame = seen
            .iter()
            .find(|(name, _)| name == "frame")
            .unwrap_or_else(|| panic!("{query}: {seen:?}"));
        let json: serde_json::Value = serde_json::from_str(&frame.1).unwrap();
        let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["beat", "bpm", "l", "lv", "phase", "r", "t"],
            "{query}"
        );
    }
}
