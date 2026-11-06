// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use futures::StreamExt;
use http_body_util::BodyStream;
use serde_json::{Value, json};
use tower::ServiceExt;

use svolder_ipc::config::{WebConfig, WebGuest};

use super::Asked;
use crate::web::auth::{cookie_name, mint, now_secs};
use crate::web::router::router;
use crate::web::test_support::stub::{Stub, open_stream};
use crate::web::test_support::{
    events_until, events_within, fake_daemon, fake_daemon_fleet, fake_daemon_playing, get, open,
    web, web_with,
};

/// One event's data as JSON.
fn data(seen: &[(String, String)], name: &str) -> Value {
    let (_, data) = seen
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no {name} event in {seen:?}"));
    serde_json::from_str(data).unwrap_or_else(|e| panic!("{name}: {e}: {data}"))
}

/// The events named `name`, as JSON.
fn all(seen: &[(String, String)], name: &str) -> Vec<Value> {
    seen.iter()
        .filter(|(n, _)| n == name)
        .map(|(_, data)| serde_json::from_str(data).expect("JSON"))
        .collect()
}

/// The `part` events of `device` named `name`, as JSON.
fn parts(seen: &[(String, String)], device: &str, name: &str) -> Vec<Value> {
    all(seen, "part")
        .into_iter()
        .filter(|part| part["device"] == device && part["name"] == name)
        .collect()
}

/// A condition for [`events_until`]: it runs `act` once, as the first model arrives (the page has
/// its snapshot, so what `act` changes is not in it), and then holds when `until` does.
fn after_model<'a>(
    act: impl FnOnce() + 'a,
    until: impl Fn(&[(String, String)]) -> bool + 'a,
) -> impl Fn(&[(String, String)]) -> bool + 'a {
    let act = RefCell::new(Some(act));
    move |seen| {
        if seen.iter().any(|(n, _)| n == "model")
            && let Some(act) = act.borrow_mut().take()
        {
            act();
        }
        until(seen)
    }
}

/// The device `dac`'s stream over `stub`, with `query` after `device=dac`.
async fn stub_stream(stub: &Arc<Stub>, query: &str) -> Body {
    open_stream(stub.clone(), &format!("/api/v1/events?device=dac{query}")).await
}

/// The URL's words: the device and the others it follows, each as the segment the model names,
/// and what the page needs.
#[test]
fn the_query_names_the_device_the_followed_and_the_needs() {
    let asked = Asked::parse(Some(
        "device=Gungnir%202&needs=meters,host,bogus&follow=Amp,Living%2C%20room",
    ));
    assert_eq!(asked.device.as_deref(), Some("Gungnir 2"));
    assert!(asked.needs.meters && asked.needs.host);
    assert_eq!(asked.follow, ["Amp", "Living, room"]);
    assert_eq!(Asked::parse(None), Asked::default());
    assert_eq!(Asked::parse(Some("device=&follow=")), Asked::default());
}

/// The stream opens with the reconnect delay, the model, then the shown device's components and
/// its strip; no event carries markup.
#[tokio::test]
async fn the_stream_opens_with_the_model_then_the_components() {
    let dac = fake_daemon("gungnir2-v2", "Gungnir 2").await;
    let response = router(web(open(), Arc::clone(&dac.fleet)))
        .oneshot(get("/api/v1/events?device=Gungnir%202"))
        .await
        .unwrap();
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap()),
        Some("text/event-stream")
    );
    let seen = events_until(response.into_body(), |seen| {
        seen.iter().any(|(n, _)| n == "strip")
    })
    .await;
    assert_eq!(
        seen.first(),
        Some(&("retry".to_string(), "2000".to_string())),
        "{seen:?}"
    );
    let mut names: Vec<&str> = Vec::new();
    for (name, _) in &seen {
        if ["model", "volume", "strip"].contains(&name.as_str()) && !names.contains(&name.as_str())
        {
            names.push(name);
        }
    }
    assert_eq!(names, ["model", "volume", "strip"], "{seen:?}");
    assert!(seen.iter().any(|(n, _)| n == "level"), "{seen:?}");
    let model = data(&seen, "model");
    assert_eq!(model["device"]["url"], "Gungnir%202");
    assert_eq!(model["strip"], Value::Null);
    assert!(model["fleet"]["devices"].is_array());
    let volume = data(&seen, "volume");
    assert_eq!(volume["device"], "Gungnir%202");
    assert!(
        volume["raw"].is_number() && volume["unread"] == false,
        "{volume}"
    );
    assert_eq!(data(&seen, "strip")["device"], "Gungnir%202");
    assert!(
        seen.iter().all(|(_, data)| !data.contains('<')),
        "no markup: {seen:?}"
    );
    dac.stop().await;
}

/// A write from the daemon's side reaches the page as the part it changes, with the value it
/// has now.
#[tokio::test]
async fn a_reading_sends_the_part_it_changes() {
    let dac = fake_daemon("gungnir2-v2", "Gungnir 2").await;
    let body = open_stream(Arc::clone(&dac.fleet), "/api/v1/events?device=Gungnir%202").await;
    let fleet = Arc::clone(&dac.fleet);
    let write = move || {
        tokio::spawn(async move {
            let _ = fleet
                .request(svolder_ipc::Request::Set {
                    device: Some("Gungnir 2".into()),
                    key: "phase".into(),
                    value: "on".into(),
                    step: 1,
                    no_wait: false,
                })
                .await;
        });
    };
    let phase_on = |seen: &[(String, String)]| {
        parts(seen, "Gungnir%202", "controls").iter().any(|part| {
            part["data"].as_array().is_some_and(|controls| {
                controls
                    .iter()
                    .any(|c| c["key"] == "phase" && c["on"] == true)
            })
        })
    };
    let seen = events_until(body, after_model(write, phase_on)).await;
    assert!(phase_on(&seen), "{seen:?}");
    // The first thing the page is told of it is that part, not a second model.
    assert_eq!(all(&seen, "model").len(), 1, "{seen:?}");
    dac.stop().await;
}

/// A device that goes away sends the whole model again, with the lost-link card in it: the card
/// and the dimmed view are part of the shape, not of a part.
#[tokio::test]
async fn a_changed_shape_sends_the_whole_model() {
    let stub = Stub::new(true, true);
    let body = stub_stream(&stub, "").await;
    let later = stub.clone();
    let warned = |seen: &[(String, String)]| {
        all(seen, "model")
            .iter()
            .any(|model| model["device"]["card"]["kind"] == "warn")
    };
    let seen = events_until(body, after_model(move || later.go_away(), warned)).await;
    assert!(warned(&seen), "{seen:?}");
    let models = all(&seen, "model");
    assert_eq!(
        models[0]["device"]["card"],
        Value::Null,
        "the first model had no card"
    );
    let warned = models.last().unwrap();
    assert_eq!(warned["device"]["locked"], true);
    assert_eq!(warned["device"]["away"], true);
    assert_eq!(warned["device"]["link"]["state"], "off");
    assert_eq!(models.len(), 2, "{seen:?}");
}

/// A device seen for the first time, on a page opened while it was not, sends the whole model
/// with its knob.
#[tokio::test]
async fn a_device_seen_for_the_first_time_sends_the_model_with_the_knob() {
    let stub = Stub::new(false, false);
    let body = stub_stream(&stub, "").await;
    let later = stub.clone();
    let knob = |seen: &[(String, String)]| {
        all(seen, "model")
            .iter()
            .any(|m| m["device"]["volume"].is_object())
            && seen.iter().any(|(n, _)| n == "volume")
    };
    let seen = events_until(body, after_model(move || later.appear(), knob)).await;
    let models = all(&seen, "model");
    assert_eq!(models[0]["device"]["volume"], Value::Null, "{seen:?}");
    assert_eq!(models[0]["device"]["card"]["title"], "Looking for dac…");
    let last = models.last().unwrap();
    assert!(last["device"]["volume"].is_object(), "{seen:?}");
    assert_eq!(last["device"]["card"], Value::Null);
    assert!(seen.iter().any(|(n, _)| n == "volume"), "{seen:?}");
}

/// A device that is followed and goes away is sent whole as a `device` event, tagged with its
/// name, and the page's model stays as it was.
#[tokio::test]
async fn a_followed_device_that_goes_away_sends_its_whole_device() {
    let stub = Stub::new(true, true);
    let body = open_stream(stub.clone(), "/api/v1/events?follow=dac").await;
    let later = stub.clone();
    let whole = |seen: &[(String, String)]| seen.iter().any(|(n, _)| n == "device");
    let seen = events_until(body, after_model(move || later.go_away(), whole)).await;
    let model = data(&seen, "model");
    assert_eq!(model["device"], Value::Null);
    assert_eq!(model["followed"]["dac"]["card"], Value::Null, "{model}");
    let whole = data(&seen, "device");
    assert_eq!(whole["device"], "dac");
    assert_eq!(whole["data"]["card"]["kind"], "warn", "{whole}");
    assert_eq!(all(&seen, "model").len(), 1, "{seen:?}");
}

/// A change of the device's row alone (the seconds to the next attempt, an attempt under way)
/// reaches the page as the card's new part, with no feed change to wake the stream.
#[tokio::test]
async fn a_change_of_the_retry_seconds_sends_the_card_part_again() {
    let stub = Stub::new(false, true);
    let body = stub_stream(&stub, "").await;
    let later = stub.clone();
    let retry_in_7 = |seen: &[(String, String)]| {
        parts(seen, "dac", "card")
            .iter()
            .any(|part| part["data"]["retry_in"] == 7)
    };
    let seen = events_until(
        body,
        after_model(move || later.set_retry(Some(7)), retry_in_7),
    )
    .await;
    assert!(retry_in_7(&seen), "{seen:?}");
    assert_eq!(all(&seen, "model").len(), 1, "{seen:?}");
}

/// Meters go to a page that asks for them, and to no other.
#[tokio::test]
async fn meters_go_only_to_a_page_that_needs_them() {
    let amp = fake_daemon("fake-amp", "Amp").await;
    let asking = open_stream(
        Arc::clone(&amp.fleet),
        "/api/v1/events?device=Amp&needs=meters",
    )
    .await;
    let seen = events_within(asking, |_| false, Duration::from_secs(2)).await;
    let meters = all(&seen, "meters");
    assert!(!meters.is_empty(), "{seen:?}");
    assert_eq!(meters[0]["device"], "Amp");
    assert!(meters[0]["meters"].is_array(), "{meters:?}");
    let quiet = open_stream(Arc::clone(&amp.fleet), "/api/v1/events?device=Amp").await;
    let seen = events_within(quiet, |_| false, Duration::from_secs(2)).await;
    assert!(seen.iter().any(|(n, _)| n == "model"), "{seen:?}");
    assert!(seen.iter().all(|(n, _)| n != "meters"), "{seen:?}");
    amp.stop().await;
}

/// A device the page follows sends its parts tagged with its name, and the model carries it
/// under its own.
#[tokio::test]
async fn a_followed_device_sends_its_parts_tagged_with_its_name() {
    let fleet = fake_daemon_fleet(&[
        ("gungnir2-v2", "Gungnir 2", "0102030405060708"),
        ("fake-amp", "Amp", "1112131415161718"),
    ])
    .await;
    let body = open_stream(
        Arc::clone(&fleet.fleet),
        "/api/v1/events?device=Gungnir%202&follow=Amp&needs=meters",
    )
    .await;
    let seen = events_within(
        body,
        |seen| all(seen, "meters").iter().any(|m| m["device"] == "Amp"),
        Duration::from_secs(5),
    )
    .await;
    let model = data(&seen, "model");
    assert_eq!(model["device"]["url"], "Gungnir%202");
    assert_eq!(model["followed"]["Amp"]["layout"], "amp", "{model}");
    assert!(model["followed"].get("Gungnir%202").is_none());
    assert_eq!(model["fleet"]["devices"].as_array().map(Vec::len), Some(2));
    let meters = all(&seen, "meters");
    assert!(meters.iter().any(|m| m["device"] == "Amp"), "{seen:?}");
    assert!(
        meters.iter().all(|m| m["device"] == "Amp"),
        "the DAC has no meters: {meters:?}"
    );
    // The shown device's strip is the only one.
    assert_eq!(
        all(&seen, "strip")
            .iter()
            .filter(|s| s["device"] != "Gungnir%202")
            .count(),
        0
    );
    fleet.stop().await;
}

/// A page with no device is sent the model with no device, the fleet, and the computer's player
/// when the fake one changes.
#[tokio::test]
async fn a_page_with_no_device_gets_the_model_fleet_and_host() {
    let dac = fake_daemon_playing("gungnir2-v2", "Gungnir 2").await;
    let body = open_stream(Arc::clone(&dac.fleet), "/api/v1/events?needs=host").await;
    let fleet = Arc::clone(&dac.fleet);
    let press = move || {
        tokio::spawn(async move {
            let _ = fleet
                .request(svolder_ipc::Request::HostPress {
                    button: "pause".into(),
                })
                .await;
        });
    };
    let host_sent = |seen: &[(String, String)]| seen.iter().any(|(n, _)| n == "host");
    let seen = events_until(body, after_model(press, host_sent)).await;
    let model = data(&seen, "model");
    assert_eq!(model["device"], Value::Null, "{model}");
    assert_eq!(model["shown"], Value::Null);
    assert_eq!(model["fleet"]["devices"][0]["name"], "Gungnir 2");
    assert_eq!(model["host"]["track"]["status"], "playing");
    let host = data(&seen, "host");
    assert_eq!(host["track"]["status"], "paused", "{host}");
    assert!(host["path"].is_null(), "no device, no path: {host}");
    assert!(
        seen.iter()
            .all(|(n, _)| !["volume", "level", "meters", "strip"].contains(&n.as_str()))
    );
    dac.stop().await;
}

/// A hop declared while the stream is open reaches it when the fleet is said to have changed,
/// at once and not on the second's turn.
#[tokio::test]
async fn a_fleet_change_reaches_the_open_streams() {
    let dac = fake_daemon("gungnir2-v2", "Gungnir 2").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let response = router(web.clone())
        .oneshot(get("/api/v1/events?device=Gungnir%202"))
        .await
        .unwrap();
    let path = dac.dir.join("config.toml");
    let hop = svolder_ipc::config::Hop {
        what: svolder_ipc::config::HopWhat::Declared {
            kind: svolder_ipc::config::HopKind::Speakers,
            name: "Studio monitors".to_string(),
        },
        note: None,
        after: None,
    };
    let body = response.into_body();
    let declare = move || {
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        svolder_ipc::Config::add_hop(&path, &hop).expect("the hop is added");
        // The file looks as it did: only the word that the fleet changed finds the hop.
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        web.fleet_changed.send_modify(|turn| *turn += 1);
    };
    let hops = |seen: &[(String, String)]| {
        all(seen, "fleet")
            .iter()
            .any(|fleet| fleet["hops"][0]["name"] == "Studio monitors")
    };
    let seen = events_within(body, after_model(declare, hops), Duration::from_millis(900)).await;
    assert!(hops(&seen), "{seen:?}");
    assert_eq!(data(&seen, "model")["fleet"]["hops"], json!([]));
    dac.stop().await;
}

/// A hop written into the file with no one saying so is found on the second's turn.
#[tokio::test]
async fn a_hop_written_into_the_file_is_found_on_the_seconds_turn() {
    let dac = fake_daemon("gungnir2-v2", "Gungnir 2").await;
    let body = open_stream(Arc::clone(&dac.fleet), "/api/v1/events?device=Gungnir%202").await;
    let path = dac.dir.join("config.toml");
    let hop = svolder_ipc::config::Hop {
        what: svolder_ipc::config::HopWhat::Declared {
            kind: svolder_ipc::config::HopKind::Amp,
            name: "Loki".to_string(),
        },
        note: None,
        after: None,
    };
    let declare = move || svolder_ipc::Config::add_hop(&path, &hop).expect("the hop is added");
    let found = |seen: &[(String, String)]| {
        all(seen, "fleet")
            .iter()
            .any(|f| f["hops"][0]["name"] == "Loki")
    };
    let seen = events_until(body, after_model(declare, found)).await;
    assert!(found(&seen), "{seen:?}");
    dac.stop().await;
}

/// The route reads are held for a DAC page that needs the host and for no other: not a page
/// that does not ask, not an amp's. A stream on a daemon without a host carries no card.
#[tokio::test]
async fn host_reads_are_held_only_for_a_dac_page_that_needs_host() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    assert_eq!(dac.daemon.host().watcher_count(), 0);
    let plain = open_stream(Arc::clone(&dac.fleet), "/api/v1/events?device=dac").await;
    let seen = events_until(plain, |seen| seen.iter().any(|(n, _)| n == "strip")).await;
    assert_eq!(
        data(&seen, "model")["device"]["playing"]["track"]["headline"],
        "Test Pressing",
        "{seen:?}"
    );
    assert_eq!(
        dac.daemon.host().watcher_count(),
        0,
        "no host need, no hold"
    );
    // The body is dropped; a page that needs the host holds the reads while it is open.
    let body = open_stream(
        Arc::clone(&dac.fleet),
        "/api/v1/events?device=dac&needs=host",
    )
    .await;
    let mut frames = BodyStream::new(body);
    let mut text = String::new();
    let mut read_until = async |until: &dyn Fn(&str) -> bool| {
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(Ok(frame)) = frames.next().await {
                if let Ok(data) = frame.into_data() {
                    text.push_str(&String::from_utf8_lossy(&data));
                }
                if until(&text) {
                    return;
                }
            }
        })
        .await;
        text.clone()
    };
    let first = read_until(&|text| text.contains("event: strip")).await;
    assert!(first.contains("Test Pressing"), "{first}");
    assert_eq!(dac.daemon.host().watcher_count(), 1, "held while open");
    // A press reaches the fake player: the card's part and the host's view both carry it.
    let before = first.len();
    let answer = dac
        .fleet
        .request(svolder_ipc::Request::HostPress {
            button: "pause".into(),
        })
        .await;
    assert!(matches!(answer, svolder_ipc::Response::Ack), "{answer:?}");
    let later = read_until(&|text| {
        let sent = &text[before..];
        sent.contains("event: host") && sent.contains("event: part")
    })
    .await;
    let sent = &later[before..];
    assert!(sent.contains("\"status\":\"paused\""), "{sent}");
    assert!(sent.contains("\"name\":\"playing\""), "{sent}");
    drop(frames);
    for _ in 0..100 {
        if dac.daemon.host().watcher_count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        dac.daemon.host().watcher_count(),
        0,
        "let go with the stream"
    );
    dac.stop().await;

    let amp = fake_daemon_playing("fake-amp", "amp").await;
    let response = open_stream(
        Arc::clone(&amp.fleet),
        "/api/v1/events?device=amp&needs=host",
    )
    .await;
    let seen = events_until(response, |seen| seen.iter().any(|(n, _)| n == "strip")).await;
    assert!(
        data(&seen, "model")["device"]["playing"].is_object(),
        "the amp has the card: {seen:?}"
    );
    assert_eq!(amp.daemon.host().watcher_count(), 0, "no path, no hold");
    amp.stop().await;

    let quiet = fake_daemon("gungnir2-v2", "dac").await;
    let response = open_stream(
        Arc::clone(&quiet.fleet),
        "/api/v1/events?device=dac&needs=host",
    )
    .await;
    let seen = events_until(response, |seen| seen.iter().any(|(n, _)| n == "strip")).await;
    assert_eq!(
        data(&seen, "model")["device"]["playing"],
        Value::Null,
        "{seen:?}"
    );
    assert_eq!(data(&seen, "model")["host"], Value::Null);
    quiet.stop().await;
}

/// The Now playing card is sent when what it says changes, not when the host publishes: a page
/// that holds no route reads is sent nothing while the player plays on, and one card when the
/// player is paused.
#[tokio::test]
async fn a_card_that_says_the_same_is_not_sent_again() {
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    let body = open_stream(Arc::clone(&dac.fleet), "/api/v1/events?device=dac").await;
    let fleet = Arc::clone(&dac.fleet);
    let press_later = move || {
        tokio::spawn(async move {
            // The player plays on for a while first, with nothing sent for it.
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let _ = fleet
                .request(svolder_ipc::Request::HostPress {
                    button: "pause".into(),
                })
                .await;
        });
    };
    let seen = events_within(
        body,
        after_model(press_later, |_| false),
        Duration::from_millis(3500),
    )
    .await;
    let cards = parts(&seen, "dac", "playing");
    assert_eq!(cards.len(), 1, "{seen:?}");
    assert_eq!(cards[0]["data"]["track"]["status"], "paused", "{cards:?}");
    assert!(
        seen.iter().all(|(n, _)| n != "host"),
        "no host need: {seen:?}"
    );
    dac.stop().await;
}

/// A guest's stream names no unit and has every control inert.
#[tokio::test]
async fn a_guest_stream_names_no_unit_and_is_inert() {
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
    let response = router(guests)
        .oneshot(get("/api/v1/events?device=dac"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let seen = events_until(response.into_body(), |seen| {
        seen.iter().any(|(n, _)| n == "strip")
    })
    .await;
    let model = data(&seen, "model");
    assert_eq!(model["viewer"]["guest"], true);
    assert_eq!(model["device"]["inert"], true);
    assert!(model["device"]["facts"].get("unit").is_none(), "{model}");
    assert!(
        seen.iter()
            .all(|(_, data)| !data.contains("0102030405060708")),
        "the unit id went out: {seen:?}"
    );
    dac.stop().await;
}

/// A rotated token ends an open stream at once: the browser it signed out reads no more, and its
/// reconnect meets the 401 that loads the sign-in page.
#[tokio::test]
async fn a_rotated_token_ends_the_stream() {
    const TOKEN: &str = "a-test-token-0123456789abcdefghijklmn";
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some(TOKEN),
    );
    let mut request = get("/api/v1/events?device=dac&follow=dac");
    let cookie = format!("{}={}", cookie_name(8738), mint(TOKEN, now_secs()));
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let response = router(web.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
    let mut body = response.into_body().into_data_stream();
    let first = tokio::time::timeout(Duration::from_secs(5), body.next()).await;
    assert!(matches!(first, Ok(Some(Ok(_)))), "no snapshot");
    web.rekey_value("a-new-token-0123456789abcdefghijklmn", None);
    let ended = tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(Ok(_)) = body.next().await {}
    })
    .await;
    assert!(ended.is_ok(), "the stream outlived the rotation");
    let mut again = get("/api/v1/events?device=dac");
    again
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let refused = router(web.clone()).oneshot(again).await.unwrap();
    assert_eq!(refused.status(), 401, "the old cookie reconnects");
    dac.stop().await;
}

/// The stream is refused to a browser that is not signed in, and for a device that is not
/// there; a followed device that is not there is left out.
#[tokio::test]
async fn the_stream_refuses_a_stranger_and_an_unknown_device() {
    let stub = Stub::new(true, true);
    let token_web = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        stub.clone(),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let stranger = router(token_web)
        .oneshot(get("/api/v1/events?device=dac"))
        .await
        .unwrap();
    assert_eq!(stranger.status(), 401);
    let unknown = router(web(open(), stub.clone()))
        .oneshot(get("/api/v1/events?device=nowhere"))
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);
    let body = open_stream(stub.clone(), "/api/v1/events?device=dac&follow=nowhere,dac").await;
    let seen = events_until(body, |seen| seen.iter().any(|(n, _)| n == "strip")).await;
    let model = data(&seen, "model");
    assert_eq!(model["device"]["url"], "dac");
    assert_eq!(
        model["followed"],
        json!({}),
        "the shown device is not followed twice"
    );
}

/// A device whose readings change ten times a second is sent the facts part at most once a
/// second.
#[tokio::test(start_paused = true)]
async fn facts_part_is_sent_at_most_once_a_second() {
    let stub = Stub::new(true, true);
    let body = stub_stream(&stub, "").await;
    let meters = stub.clone();
    tokio::spawn(async move {
        for step in 0..30u8 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            meters.read_volume(40 + step);
        }
    });
    let seen = events_within(body, |_| false, Duration::from_millis(3050)).await;
    let sent = parts(&seen, "dac", "facts");
    // The snapshot's is in the model; then at most one a second.
    assert!(!sent.is_empty(), "{seen:?}");
    assert!(
        sent.len() <= 3,
        "{} sends in three seconds: {seen:?}",
        sent.len()
    );
    assert!(
        sent.iter().all(|part| part["data"]["rows"].is_array()),
        "{sent:?}"
    );
}

/// A climb reported a step every 84 ms (the writer's pace on a 127-step range) reaches the page
/// at that pace, each step as it comes: the coalescing neither holds a step for the parts' turn
/// nor lets two arrive close behind each other.
#[tokio::test(start_paused = true)]
async fn each_reported_volume_step_reaches_the_page_at_the_pace_it_came() {
    let stub = Stub::new(true, true);
    stub.read_volume(95);
    let mut body = BodyStream::new(stub_stream(&stub, "").await);
    let mut text = String::new();
    let mut arrivals: Vec<(tokio::time::Instant, u64)> = Vec::new();
    let mut sent = None;
    let mut next = 96u8;
    let mut at = tokio::time::Instant::now() + Duration::from_millis(500);
    while arrivals.len() < 12 {
        tokio::select! {
            () = tokio::time::sleep_until(at), if next <= 112 => {
                stub.read_volume(next);
                sent = Some(tokio::time::Instant::now());
                next += 1;
                at += Duration::from_millis(84);
            }
            Some(Ok(frame)) = body.next() => {
                if let Ok(data) = frame.into_data() {
                    text.push_str(&String::from_utf8_lossy(&data));
                }
                while let Some(end) = text.find("\n\n") {
                    let block: String = text.drain(..end + 2).collect();
                    if block.contains("event: volume") && sent.is_some() {
                        let raw = block.split("\"raw\":").nth(1).and_then(|r| {
                            r.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
                        });
                        arrivals.push((tokio::time::Instant::now(), raw.unwrap_or(0)));
                    }
                }
            }
        }
    }
    let raws: Vec<u64> = arrivals.iter().map(|(_, raw)| *raw).collect();
    assert_eq!(
        raws,
        (96..108).collect::<Vec<u64>>(),
        "every step, in order"
    );
    let strides: Vec<u128> = arrivals
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).as_millis())
        .collect();
    assert!(
        strides.iter().all(|s| (74..=94).contains(s)),
        "the strides between arrivals, in ms: {strides:?}"
    );
}

/// A followed device's later events carry its name like the opening ones: the volume it reports,
/// the facts that change with it, and every level.
#[tokio::test]
async fn a_followed_devices_later_events_carry_its_tag() {
    let stub = Stub::new(true, true);
    let body = open_stream(stub.clone(), "/api/v1/events?follow=dac").await;
    let later = stub.clone();
    let reported = |seen: &[(String, String)]| {
        all(seen, "volume").iter().any(|v| v["raw"] == 50)
            && !parts(seen, "dac", "facts").is_empty()
    };
    let seen = events_until(body, after_model(move || later.read_volume(50), reported)).await;
    assert!(reported(&seen), "{seen:?}");
    for event in ["volume", "level"] {
        let sent = all(&seen, event);
        assert!(!sent.is_empty(), "{event}: {seen:?}");
        assert!(
            sent.iter().all(|e| e["device"] == "dac"),
            "{event}: {sent:?}"
        );
    }
    assert!(all(&seen, "part").iter().all(|p| p["device"] == "dac"));
    assert!(
        all(&seen, "strip").is_empty(),
        "only the shown device has a strip"
    );
}

/// The tasks of a page end when it goes: nothing keeps following the device.
#[tokio::test]
async fn the_per_device_tasks_end_when_the_page_goes() {
    for query in ["device=dac", "follow=dac", "device=dac&needs=meters,host"] {
        let stub = Stub::new(true, true);
        let before = stub.feed.receiver_count();
        let body = open_stream(stub.clone(), &format!("/api/v1/events?{query}")).await;
        let open = std::sync::atomic::AtomicUsize::new(before);
        let seen = events_until(body, |seen| {
            open.store(
                stub.feed.receiver_count(),
                std::sync::atomic::Ordering::Relaxed,
            );
            seen.iter().any(|(n, _)| n == "volume")
        })
        .await;
        assert!(seen.iter().any(|(n, _)| n == "volume"), "{query}: {seen:?}");
        let open = open.load(std::sync::atomic::Ordering::Relaxed);
        assert!(open > before, "{query}: no task followed the feed ({open})");
        // The body went with `events_until`.
        let ended = tokio::time::timeout(Duration::from_secs(1), async {
            while stub.feed.receiver_count() > before {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(ended.is_ok(), "{query}: a task outlived its page");
    }
}

/// What changed between the snapshot the page was sent and the moment a device's task starts (the
/// route's first read is one such change) reaches the stream at once: the task compares what the
/// device says then with what the page was sent, and does not wait for another change, nor for the
/// second's turn, to send it.
#[tokio::test]
async fn a_change_between_the_snapshot_and_the_task_start_still_reaches_the_stream() {
    use super::device::{Read, Task};
    use crate::web::{Viewer, api};
    let stub = Stub::new(false, true);
    let web = web(open(), stub.clone());
    let cx = Arc::new(super::Context {
        access: api::access(&web, Viewer::Signed),
        viewer: Viewer::Signed,
        needs: super::Needs::default(),
        followed: Vec::new(),
        web: web.clone(),
    });
    // The snapshot the page was sent, then a change it does not have.
    let snapshot = Read::of(&cx, "dac").expect("the device reads");
    stub.set_retry(Some(7));
    // The task starts from what the device holds now, as `run` marks it, and nothing more happens.
    let mut tracked = super::track(&web, "dac").expect("tracked");
    super::mark_seen(&mut tracked.watched);
    let task = Task::new(Arc::clone(&cx), tracked, Some(&snapshot), true);
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let handle = tokio::spawn(task.run(web.signed_out.subscribe(), tx));
    let body = axum::response::IntoResponse::into_response(axum::response::sse::Sse::new(
        tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok::<_, std::convert::Infallible>),
    ))
    .into_body();
    let retry_in_7 = |seen: &[(String, String)]| {
        parts(seen, "dac", "card")
            .iter()
            .any(|part| part["data"]["retry_in"] == 7)
    };
    // Well inside the second before the task's own tick.
    let seen = events_within(body, retry_in_7, Duration::from_millis(600)).await;
    assert!(retry_in_7(&seen), "{seen:?}");
    handle.abort();
}

/// A reading pushes the volume; a recall pushes the presets part with the preset that is on.
#[tokio::test]
async fn a_reading_pushes_the_volume_and_a_recall_the_presets() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let body = open_stream(Arc::clone(&dac.fleet), "/api/v1/events?device=dac").await;
    let fleet = Arc::clone(&dac.fleet);
    let act = move || {
        tokio::spawn(async move {
            let info = fleet.resolve(Some("dac")).expect("the device");
            let _ = fleet
                .request(svolder_ipc::Request::Set {
                    device: Some("dac".into()),
                    key: "volume".into(),
                    value: "50".into(),
                    step: 1,
                    no_wait: false,
                })
                .await;
            let profile = fleet.profile(&info).expect("a profile");
            let fitting =
                svolder_profiles::tone::preset::fitting(profile.eq.as_ref().unwrap(), &[])
                    .into_iter()
                    .map(|(p, _)| p)
                    .collect();
            let _ = fleet.recall(&info, "Bass", fitting).await;
        });
    };
    let pushed = |seen: &[(String, String)]| {
        all(seen, "volume").iter().any(|v| v["raw"] == 50)
            && parts(seen, "dac", "presets")
                .iter()
                .any(|part| part["data"]["status"]["name"] == "Bass")
    };
    let seen = events_until(body, after_model(act, pushed)).await;
    assert!(pushed(&seen), "{seen:?}");
    dac.stop().await;
}

/// A device with no power control is sent no power part: it has none in its model.
#[tokio::test]
async fn a_device_without_power_is_sent_no_power_part() {
    let stub = Stub::new(true, true);
    let body = stub_stream(&stub, "").await;
    let seen = events_until(body, |seen| seen.iter().any(|(n, _)| n == "volume")).await;
    assert!(
        data(&seen, "model")["device"]["power"].is_null(),
        "{seen:?}"
    );
    assert!(parts(&seen, "dac", "power").is_empty(), "{seen:?}");
}

/// The route looks the device up once before it spawns the stream (the runtime is single
/// threaded, so the stream's own turns have not run yet).
#[tokio::test]
async fn the_route_watches_its_device_once() {
    let stub = Stub::new(true, true);
    let _body = stub_stream(&stub, "").await;
    assert_eq!(stub.watches.load(std::sync::atomic::Ordering::Relaxed), 1);
}

/// The amp layout draws no tone control or presets, so its model has none and none is sent.
#[tokio::test]
async fn an_amp_layout_is_sent_no_tone_or_presets() {
    let stub = Stub::new(true, true).amp_layout();
    let body = stub_stream(&stub, "").await;
    let seen = events_until(body, |seen| seen.iter().any(|(n, _)| n == "volume")).await;
    let device = &data(&seen, "model")["device"];
    assert!(device["controls"].is_array(), "{device}");
    assert!(
        device["tone"].is_null() && device["presets"].is_null(),
        "{device}"
    );
    for part in ["tone", "presets"] {
        assert!(parts(&seen, "dac", part).is_empty(), "{part}: {seen:?}");
    }
}

/// A profile re-resolved under an open page sends the whole model again: the kept knob and meters
/// carry the first paint's range and words.
#[tokio::test]
async fn a_profile_picked_again_sends_the_model_again() {
    let stub = Stub::new(true, true);
    let body = stub_stream(&stub, "").await;
    let later = stub.clone();
    let again = |seen: &[(String, String)]| all(seen, "model").len() >= 2;
    let seen = events_until(
        body,
        after_model(move || later.rename_profile("another-profile"), again),
    )
    .await;
    assert!(again(&seen), "{seen:?}");
}

/// A band's reading redraws the device presets part: applying a built-in setting lights its
/// button in a pushed part, and the lamp it leaves behind is the one the bands match.
#[tokio::test]
async fn a_band_reading_sends_the_device_presets_part() {
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let body = open_stream(Arc::clone(&loki.fleet), "/api/v1/events?device=Loki%20Max").await;
    let fleet = Arc::clone(&loki.fleet);
    let apply = move || {
        tokio::spawn(async move {
            let info = fleet.resolve(Some("Loki Max")).expect("the device");
            let _ = fleet.apply(&info, "Bass Boost").await;
        });
    };
    let lit = |seen: &[(String, String)]| {
        parts(seen, "Loki%20Max", "device_presets")
            .iter()
            .any(|part| {
                part["data"]["builtins"].as_array().is_some_and(|b| {
                    b.iter()
                        .any(|s| s["name"] == "Bass Boost" && s["on"] == true)
                })
            })
    };
    let seen = events_until(body, after_model(apply, lit)).await;
    assert!(lit(&seen), "{seen:?}");
    loki.stop().await;
}

/// The `part` events that carry the facts.
fn facts_sends(seen: &[(String, String)]) -> usize {
    parts(seen, "dac", "facts").len()
}

/// Stamp the feed's readings `ms`.
fn heard(stub: &Stub, ms: u64) {
    stub.feed.send_modify(|feed| {
        for m in feed.readings.clone() {
            feed.heard_at.insert(m.id, ms);
        }
    });
}

/// An amp's meters change ten times a second for three seconds and nothing else does: no facts
/// part is sent after the model (which carries the snapshot's). The meters show those values
/// already, so they are not what the part says changed.
#[tokio::test(start_paused = true)]
async fn facts_part_is_not_sent_for_meters_alone() {
    let stub = Stub::new(true, true).on_profile("fake-five");
    stub.read_meter(100);
    let body = stub_stream(&stub, "&needs=meters").await;
    let meters = stub.clone();
    tokio::spawn(async move {
        for step in 0..30u16 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            meters.read_meter(101 + step);
        }
    });
    let seen = events_within(body, |_| false, Duration::from_millis(3050)).await;
    assert!(
        !all(&seen, "meters").is_empty(),
        "the meters were sent: {seen:?}"
    );
    assert_eq!(facts_sends(&seen), 0, "only the model's: {seen:?}");
}

/// Readings heard again with the same words within the minute, and a clock that moves on, send
/// nothing: the page ages them itself.
#[tokio::test(start_paused = true)]
async fn facts_part_is_not_sent_when_no_value_changed() {
    let stub = Stub::new(true, true);
    stub.read_volume(40);
    heard(&stub, 1_700_000_040_000 + 1_000);
    let body = stub_stream(&stub, "").await;
    let again = stub.clone();
    tokio::spawn(async move {
        for stamp in 0..30u64 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            heard(&again, 1_700_000_040_000 + 2_000 + stamp * 1_000);
        }
    });
    let seen = events_within(body, |_| false, Duration::from_millis(3050)).await;
    assert_eq!(facts_sends(&seen), 0, "only the model's: {seen:?}");
}

/// A reading heard again with the same words in a later minute sends the part once, so "Heard"
/// is never more than about a minute out.
#[tokio::test(start_paused = true)]
async fn facts_part_is_sent_once_when_a_reading_is_heard_again_in_a_new_minute() {
    let stub = Stub::new(true, true);
    stub.read_volume(40);
    heard(&stub, 1_700_000_040_000 + 30_000);
    let body = stub_stream(&stub, "").await;
    let again = stub.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        heard(&again, 1_700_000_040_000 + 70_000);
        tokio::time::sleep(Duration::from_millis(300)).await;
        heard(&again, 1_700_000_040_000 + 71_000);
    });
    let seen = events_within(body, |_| false, Duration::from_millis(3050)).await;
    assert_eq!(facts_sends(&seen), 1, "one after the model's: {seen:?}");
}

/// Three readings heard again 20 s apart, so that each crosses a minute boundary at a different
/// second, send the part once: the minute of the newest stamp is what the key holds, not every
/// reading's own.
#[tokio::test(start_paused = true)]
async fn facts_part_is_sent_once_for_three_readings_heard_again_across_a_minute() {
    let stub = Stub::new(true, true);
    let ids = [
        svolder_proto::FunctionId::three(210, 2, 1),
        svolder_proto::FunctionId::three(210, 2, 2),
        svolder_proto::FunctionId::three(210, 2, 3),
    ];
    stub.feed.send_modify(|feed| {
        feed.readings = ids
            .iter()
            .map(|id| svolder_proto::Message::status(*id, svolder_proto::Value::U16(7)))
            .collect();
        for id in ids {
            feed.heard_at.insert(id, 1_700_000_040_000 + 10_000);
        }
    });
    let body = stub_stream(&stub, "").await;
    let again = stub.clone();
    tokio::spawn(async move {
        for (id, at) in ids.into_iter().zip([50_000, 70_000, 90_000]) {
            tokio::time::sleep(Duration::from_millis(1_200)).await;
            again.feed.send_modify(|feed| {
                feed.heard_at.insert(id, 1_700_000_040_000 + at);
            });
        }
    });
    let seen = events_within(body, |_| false, Duration::from_millis(4_000)).await;
    assert_eq!(
        facts_sends(&seen),
        1,
        "one for the new minute, after the model's: {seen:?}"
    );
}

/// A change the throttle holds back (the snapshot's send was half a second ago) is not lost: it
/// goes out by the second's tick, about a second after the last send, with nothing else changing.
#[tokio::test(start_paused = true)]
async fn a_throttled_facts_change_is_delivered_by_the_tick() {
    let stub = Stub::new(true, true);
    stub.read_volume(40);
    let body = stub_stream(&stub, "").await;
    let later = stub.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        later.read_volume(41);
    });
    let seen = events_within(body, |_| false, Duration::from_millis(1200)).await;
    let first = &data(&seen, "model")["device"]["facts"]["rows"];
    assert!(
        first.to_string().contains("40 ("),
        "the snapshot's: {first}"
    );
    let held = parts(&seen, "dac", "facts");
    assert_eq!(held.len(), 1, "{seen:?}");
    assert!(
        held[0]["data"]["rows"].to_string().contains("41 ("),
        "the held change: {held:?}"
    );
}

/// Make the ring of the fake device of `profile` hold what it reported once, five seconds ago
/// (the fake reports none on its own).
fn seed_the_ring(daemon: &crate::web::test_support::FakeDaemon, profile: &str) {
    let handle = daemon.daemon.devices().remove(0);
    let ring = Arc::clone(&crate::locks::locked(&handle.info).ring);
    let registry = svolder_profiles::Registry::builtin().unwrap();
    let built = registry.get(profile).expect("a built-in");
    let then = std::time::Instant::now() - Duration::from_secs(5);
    for (key, raw) in [
        ("volume", 40),
        ("meter_floor_l", 20),
        ("meter_floor_r", 20),
        ("meter_1", 120),
    ] {
        let Some(control) = built.control_by_key(key) else {
            continue;
        };
        let value = match control.value_type {
            svolder_proto::ValueType::U16 => svolder_proto::Value::U16(raw),
            _ => svolder_proto::Value::U8(raw as u8),
        };
        crate::locks::locked(&ring).record(
            built,
            &svolder_proto::Message::status(control.id, value),
            then,
        );
    }
}

/// The strip's bars the stream sends in the seconds after its snapshot, for the fake device of
/// `profile` that reports nothing new: the seconds of the bars each later `strip` event carried.
async fn strip_seconds_after_the_snapshot(profile: &str) -> (u64, Vec<Vec<u64>>) {
    let daemon = fake_daemon(profile, "dev").await;
    seed_the_ring(&daemon, profile);
    // Whole seconds of link for the snapshot to carry bars from.
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let web = web(open(), Arc::clone(&daemon.fleet));
    let response = router(web)
        .oneshot(get("/api/v1/events?device=dev"))
        .await
        .unwrap();
    let seen = events_until(response.into_body(), |seen| {
        seen.iter().filter(|(n, _)| n == "strip").count() >= 4
    })
    .await;
    daemon.stop().await;
    let mut strips = all(&seen, "strip").into_iter().map(|strip| {
        strip["bars"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["t"].as_u64().unwrap())
            .collect::<Vec<u64>>()
    });
    let snapshot = strips.next().expect("the snapshot strip");
    (
        snapshot.into_iter().max().expect("a snapshot bar"),
        strips.collect(),
    )
}

/// With nothing changing on the device, the stream sends a bar for each second after the
/// snapshot, once each, on a DAC and on an amp.
#[tokio::test]
async fn a_still_device_gets_a_bar_every_second_after_the_snapshot() {
    for profile in ["gungnir2-v2", "wotan"] {
        let (newest, later) = strip_seconds_after_the_snapshot(profile).await;
        assert!(later.len() >= 3, "{profile}: {later:?}");
        let flat: Vec<u64> = later.into_iter().flatten().collect();
        let want: Vec<u64> = (newest + 1..=newest + flat.len() as u64).collect();
        assert_eq!(flat, want, "{profile}: one bar a second, none twice");
    }
}

/// The tick asks for the seconds it has not sent: a second already sent is not sent again, a late
/// tick sends each second it missed, and a cursor that is current sends nothing.
#[tokio::test]
async fn the_strip_turn_sends_each_unsent_second_once() {
    let daemon = fake_daemon("gungnir2-v2", "dev").await;
    seed_the_ring(&daemon, "gungnir2-v2");
    tokio::time::sleep(Duration::from_millis(4100)).await;
    let web = web(open(), Arc::clone(&daemon.fleet));
    // The daemon's clock held half way into a second, so no read of it crosses a boundary.
    let wall = crate::device::wall_ms() / 1000;
    crate::device::TEST_CLOCK.with(|clock| clock.set(Some(wall * 1000 + 500)));
    // Three seconds behind: the missed seconds all go, in one strip.
    let mut stripped = Some(wall - 4);
    let strip = super::unsent_strip(&web, "dev", &mut stripped)
        .await
        .expect("the missed seconds");
    let seconds: Vec<u64> = strip.bars.iter().map(|bar| bar.t).collect();
    assert_eq!(seconds, [wall - 3, wall - 2, wall - 1]);
    // Asked again in the same wall second: nothing new.
    assert!(
        super::unsent_strip(&web, "dev", &mut stripped)
            .await
            .is_none()
    );
    // A cursor that is current: nothing.
    let mut stripped = Some(wall - 1);
    assert!(
        super::unsent_strip(&web, "dev", &mut stripped)
            .await
            .is_none()
    );
    crate::device::TEST_CLOCK.with(|clock| clock.set(None));
    daemon.stop().await;
}

/// A device that connected and has reported nothing since still has a bar for each second from
/// its connect on: the connect's own refresh is in the ring.
#[tokio::test]
async fn a_connect_alone_gives_the_strip_its_bars() {
    let daemon = fake_daemon("gungnir2-v2", "dev").await;
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let web = web(open(), Arc::clone(&daemon.fleet));
    let wall = crate::device::wall_ms() / 1000;
    let mut stripped = Some(wall - 3);
    let strip = super::unsent_strip(&web, "dev", &mut stripped)
        .await
        .expect("bars from the connect");
    assert!(!strip.bars.is_empty(), "{:?}", strip.bars);
    daemon.stop().await;
}

/// The stream's `volume`, `level` and `meters` events are the model's volume, slider and meters
/// exactly, plus the `device` they are tagged with: nothing is dropped or added, each event's key
/// set is the literal list below (a superset of what `core/store.js`, `knob.js`, `slider.js` and
/// `meters.js` read), and no model part has a `device` key of its own for the tag to shadow.
#[test]
fn the_models_parts_hold_every_key_of_the_streams_events() {
    use super::device::{Sent, components_out};
    use crate::test_support::profile_named;
    use crate::web::model::device_model;
    use crate::web::view::screen;
    use crate::web::view::test_support::{info, read, shot};

    /// The sorted keys of an object.
    fn keys(value: &Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }
    let sorted = |words: &[&str]| -> Vec<String> {
        let mut words: Vec<String> = words.iter().map(ToString::to_string).collect();
        words.sort();
        words
    };
    // An event is its model part plus the device tag, and has exactly the keys listed.
    let same_but_tagged = |part: &Value, event: &Value, listed: &[&str], what: &str| {
        assert!(
            part.get("device").is_none(),
            "{what}: the part has a `device` of its own"
        );
        let mut want = part.clone();
        want["device"] = "dev".into();
        assert_eq!(event, &want, "{what}");
        assert_eq!(keys(event), sorted(listed), "{what}");
    };
    // What the page's scripts read of an event: each must be in the event.
    let read_by_scripts = |event: &Value, read: &[&str], what: &str| {
        for key in read {
            assert!(event.get(key).is_some(), "{what}: the scripts read `{key}`");
        }
    };
    // The events a device's components send, as the stream builds them from `model`.
    let events =
        |model: &crate::web::model::DeviceModel, meters: bool| -> Vec<(&'static str, Value)> {
            components_out("dev", model, meters, &mut Sent::default())
                .into_iter()
                .map(|(name, data)| (name, serde_json::from_str(&data).expect("JSON")))
                .collect()
        };

    let p = profile_named("gungnir2-v2");
    let row = info("Gungnir 2");
    let readings = [read(&p, "volume", "40"), read(&p, "balance", "70")];
    let s = screen(&shot(&row, &p, &readings));
    let model = device_model(&s);
    let json = serde_json::to_value(&model).unwrap();
    let sent = events(&model, false);
    let volume = &sent
        .iter()
        .find(|(n, _)| *n == "volume")
        .expect("a volume event")
        .1;
    same_but_tagged(
        &json["volume"],
        volume,
        &[
            "key",
            "label",
            "lo",
            "hi",
            "raw",
            "text",
            "target",
            "target_text",
            "muted",
            "mute",
            "silence",
            "risk",
            "cap",
            "unread",
            "texts",
            "db",
            "marks",
            "readout",
            "unit",
            "sub",
            "notes",
            "room",
            "aria",
            "device",
        ],
        "volume",
    );
    read_by_scripts(
        volume,
        &[
            "raw",
            "text",
            "target",
            "target_text",
            "muted",
            "risk",
            "cap",
            "unread",
            "notes",
            "aria",
        ],
        "volume",
    );
    let levels: Vec<&Value> = sent
        .iter()
        .filter(|(n, _)| *n == "level")
        .map(|(_, e)| e)
        .collect();
    let level = levels
        .iter()
        .find(|e| e["key"] == "balance")
        .expect("a level event");
    let slider = json["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == "balance")
        .expect("a slider");
    same_but_tagged(
        slider,
        level,
        &[
            "kind", "key", "label", "lo", "hi", "raw", "centre", "text", "texts", "at", "from",
            "ends", "device",
        ],
        "level",
    );
    read_by_scripts(level, &["kind", "key", "raw", "text"], "level");
    assert_eq!(
        levels.len(),
        json["controls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["kind"] == "slider")
            .count(),
        "one level event for each slider"
    );

    let aegir = profile_named("aegir2");
    let readings = [
        read(&aegir, "meter_1", "200"),
        read(&aegir, "meter_2", "160"),
    ];
    let s = screen(&shot(&info("Aegir 2"), &aegir, &readings));
    let model = device_model(&s);
    let json = serde_json::to_value(&model).unwrap();
    let sent = events(&model, true);
    let meters = &sent
        .iter()
        .find(|(n, _)| *n == "meters")
        .expect("a meters event")
        .1;
    same_but_tagged(
        &json["meters"],
        meters,
        &["live", "state", "meters", "device"],
        "meters",
    );
    read_by_scripts(meters, &["live", "state", "meters"], "meters");
    for meter in meters["meters"].as_array().unwrap() {
        assert!(
            meter.get("device").is_none(),
            "a meter has no tag of its own"
        );
        assert_eq!(
            keys(meter),
            sorted(&["key", "channel", "fraction", "text", "class_a"])
        );
        read_by_scripts(meter, &["fraction", "text"], "a meter");
    }
}

#[test]
fn the_host_key_changes_with_the_route_and_not_without() {
    use super::page::HostKey;
    use crate::web::model::route_model;
    use crate::web::view::{Access, playing_of};
    use svolder_ipc::host::HostView;

    let host = HostView {
        available: true,
        ..HostView::default()
    };
    let card = playing_of(
        &host,
        Access {
            writes: true,
            guest: false,
        },
        None,
    )
    .expect("a card");
    let route = |milli| {
        let sink = svolder_host::SinkFacts {
            name: "sink".to_string(),
            volume_milli: Some(milli),
            ..Default::default()
        };
        route_model(
            &svolder_host::RouteFacts {
                sink: Some(sink),
                ..Default::default()
            },
            None,
        )
    };
    let key = HostKey::new(&card, Some(route(540)));
    assert_eq!(
        key,
        HostKey::new(&card, Some(route(540))),
        "same route, same key"
    );
    assert_ne!(
        key,
        HostKey::new(&card, Some(route(550))),
        "a changed route sends"
    );
    assert_ne!(key, HostKey::new(&card, None), "a lost route sends");
}
