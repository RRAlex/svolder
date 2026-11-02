// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use svolder_ipc::config::{WebConfig, WebGuest};

use crate::fleet::Unavailable;
use crate::web::test_support::{Code, call, fake_daemon, get, open, page_model, web, web_with};

/// The routes that served the screen and its parts as HTML, and the old per-device event stream,
/// answer 404: the page draws from the model and the JSON events.
#[tokio::test]
async fn the_old_html_routes_are_gone() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    for path in [
        "/api/v1/devices/dac/screen",
        "/api/v1/devices/dac/parts/volume",
        "/api/v1/devices/dac/parts/link",
        "/api/v1/devices/dac/events",
    ] {
        let reply = call(&web, get(path)).await;
        assert_eq!(reply.status, Code::NOT_FOUND, "{path}");
        assert!(
            !reply.body.contains("id=\"part-"),
            "{path} serves no part: {}",
            reply.body
        );
    }
    // What replaced them is there (the stream is judged by its status: its body does not end).
    let reply = call(&web, get("/api/v1/devices/dac/model")).await;
    assert_eq!(reply.status, Code::OK);
    let stream = tower::ServiceExt::oneshot(
        crate::web::router::router(web.clone()),
        get("/api/v1/events?device=dac"),
    )
    .await
    .unwrap();
    assert_eq!(stream.status(), 200);
    dac.stop().await;
}

/// Every part of a DAC's model and an amp's is there, under its own name.
#[tokio::test]
async fn every_part_is_in_the_model_under_its_name() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let dac_web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&dac_web, get("/api/v1/devices/dac/model")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let device = &said["device"];
    for part in ["link", "volume", "controls", "tone", "presets", "facts"] {
        assert!(!device[part].is_null(), "{part}: {device}");
    }
    assert!(
        device["meters"].is_null() && device["power"].is_null(),
        "{device}"
    );
    dac.stop().await;
    let amp = fake_daemon("aegir2", "amp").await;
    let amp_web = web(open(), Arc::clone(&amp.fleet));
    let reply = call(&amp_web, get("/api/v1/devices/amp/model")).await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    for part in ["link", "meters", "power", "facts"] {
        assert!(!said["device"][part].is_null(), "{part}: {said}");
    }
    amp.stop().await;
}

/// A device named with a space and a slash is reached through its percent-encoded segment,
/// and its model carries the same encoding for the requests the page makes.
#[tokio::test]
async fn a_device_name_with_a_space_and_a_slash_round_trips_through_its_urls() {
    let device = fake_daemon("gungnir2-v2", "Living room/DAC").await;
    let web = web(open(), Arc::clone(&device.fleet));
    let reply = call(&web, get("/api/v1/devices/Living%20room%2FDAC/model")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["device"]["url"], "Living%20room%2FDAC", "{said}");
    assert_eq!(said["device"]["name"], "Living room/DAC");
    let page = call(&web, get("/?device=Living%20room%2FDAC")).await;
    assert_eq!(page_model(&page.body)["device"]["name"], "Living room/DAC");
    device.stop().await;
}

/// No such device is a 404 that says so.
#[tokio::test]
async fn an_unknown_device_is_not_found() {
    let device = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&device.fleet));
    let none = call(&web, get("/api/v1/devices/nope/model")).await;
    assert_eq!(none.status, Code::NOT_FOUND);
    assert!(none.body.contains("\"error\""), "{}", none.body);
    device.stop().await;
}

/// Without a sign-in the API answers 401 with nothing said; a guest reads it.
#[tokio::test]
async fn the_api_needs_a_sign_in_or_a_guest() {
    let device = fake_daemon("gungnir2-v2", "dac").await;
    let closed = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::clone(&device.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let refused = call(&closed, get("/api/v1/devices/dac/model")).await;
    assert_eq!(refused.status, Code::UNAUTHORIZED);
    assert_eq!(refused.body, "");
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&device.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let read = call(&guests, get("/api/v1/devices/dac/model")).await;
    assert_eq!(read.status, Code::OK);
    let said: serde_json::Value = serde_json::from_str(&read.body).unwrap();
    assert_eq!(said["device"]["inert"], true, "{said}");
    device.stop().await;
}

/// The page carries the default device's model; with no device configured, the card that says how
/// to add one.
#[tokio::test]
async fn the_page_carries_the_default_screen_or_says_there_is_none() {
    let device = fake_daemon("gungnir2-v2", "dac").await;
    let page = call(&web(open(), Arc::clone(&device.fleet)), get("/")).await;
    let model = page_model(&page.body);
    assert_eq!(model["shown"], "dac");
    assert!(model["device"]["volume"].is_object(), "{model}");
    assert!(model["device"]["link"].is_object(), "{model}");
    device.stop().await;
    let empty = page_model(
        &call(&web(open(), Arc::new(Unavailable)), get("/"))
            .await
            .body,
    );
    assert_eq!(empty["card"]["title"], "No devices", "{empty}");
    assert!(empty["device"].is_null());
    let spans = empty["card"]["spans"].as_array().expect("the card's words");
    assert!(
        spans
            .iter()
            .any(|span| span["text"] == "svolder setup" && span["code"] == true),
        "{empty}"
    );
}

/// The model carries the device's name for the page's "Reconnected to …" toast.
#[tokio::test]
async fn the_model_carries_the_devices_name() {
    let device = fake_daemon("gungnir2-v2", "dac").await;
    let reply = call(
        &web(open(), Arc::clone(&device.fleet)),
        get("/api/v1/devices/dac/model"),
    )
    .await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["device"]["name"], "dac", "{said}");
    device.stop().await;
}

/// Only a signed-in viewer on a page with writes on sees live controls; a guest and a page with
/// writes off see them inert.
#[tokio::test]
async fn only_a_signed_in_viewer_with_writes_on_sees_live_controls() {
    let device = fake_daemon("gungnir2-v2", "dac").await;
    let inert = |reply: crate::web::test_support::Reply| -> serde_json::Value {
        let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
        said["device"]["inert"].clone()
    };
    let live = call(
        &web(open(), Arc::clone(&device.fleet)),
        get("/api/v1/devices/dac/model"),
    )
    .await;
    assert_eq!(inert(live), false);
    let read_only = WebConfig {
        writes: false,
        ..open()
    };
    let shut = call(
        &web(read_only, Arc::clone(&device.fleet)),
        get("/api/v1/devices/dac/model"),
    )
    .await;
    assert_eq!(inert(shut), true);
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&device.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let page = call(&guests, get("/")).await;
    let model = page_model(&page.body);
    assert_eq!(model["device"]["inert"], true, "{model}");
    assert_eq!(model["viewer"]["guest"], true);
    device.stop().await;
}

/// A signed-in viewer writes when `[web] writes` is on; a guest never does, nor a page with
/// writes off.
#[test]
fn only_a_signed_in_viewer_may_write() {
    use crate::web::Viewer;
    let make = |writes| web(WebConfig { writes, ..open() }, Arc::new(Unavailable));
    let on = super::access(&make(true), Viewer::Signed);
    assert_eq!((on.writes, on.guest), (true, false));
    let guest = super::access(&make(true), Viewer::Guest);
    assert_eq!((guest.writes, guest.guest), (false, true));
    let off = super::access(&make(false), Viewer::Signed);
    assert_eq!((off.writes, off.guest), (false, false));
}

/// The right column's parts are in the model, with the curve's values for the tween, and the
/// strip answers by the second.
#[tokio::test]
async fn the_tone_parts_and_the_strip_are_in_the_model() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&web, get("/api/v1/devices/dac/model?strip=1")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let device = &said["device"];
    assert!(device["tone"].is_object(), "{device}");
    assert!(device["presets"].is_object(), "{device}");
    assert!(device["tone"]["bands"]["rows"].is_array(), "{device}");
    assert_eq!(
        device["tone"]["response"].as_array().map(Vec::len),
        Some(181),
        "the curve's values for the tween"
    );
    assert!(
        said["strip"].is_object(),
        "the strip for the first paint: {said}"
    );
    let strip = call(&web, get("/api/v1/devices/dac/strip?secs=60")).await;
    assert_eq!(strip.status, Code::OK);
    assert_eq!(strip.header("content-type"), Some("application/json"));
    let said: serde_json::Value = serde_json::from_str(&strip.body).unwrap();
    assert_eq!(said["kind"], "dac");
    assert!(said["bars"].is_array() && said["marks"].is_array());
    let wide = call(&web, get("/api/v1/devices/dac/strip?secs=99999")).await;
    assert_eq!(
        wide.status,
        Code::OK,
        "a span over the ring's is held to it"
    );
    dac.stop().await;
}

/// A preset named with markup is text wherever its name goes: a string of the model, never markup
/// in the page that embeds it (`ember-parts.mjs` holds the same name for the part that draws it).
#[tokio::test]
async fn a_preset_named_with_markup_is_shown_as_text() {
    let named = "\n[[preset]]\nname = \"<img src=x onerror=alert(1)>\"\nbands = 5\nmode = \"eq\"\n\
                 loudness = 0\n\
                 band = [{ f = 40, g = 3.0, q = 0.7 }, { f = 160, g = 0.0, q = 4.3 }, \
                 { f = 632, g = 0.0, q = 4.3 }, { f = 2515, g = 0.0, q = 4.3 }, \
                 { f = 10000, g = 0.0, q = 4.3 }]\n";
    let dac = crate::web::test_support::fake_daemon_with("gungnir2-v2", "dac", named).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let markup = "<img src=x onerror=alert(1)>";
    // The daemon remembers it as the last preset recalled, so the status line names it too.
    let said = dac
        .fleet
        .request(svolder_ipc::Request::Recalled {
            device: None,
            name: markup.to_string(),
        })
        .await;
    assert!(
        !matches!(said, svolder_ipc::Response::Error { .. }),
        "{said:?}"
    );
    // The writer's actor takes the note on its own turn.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while dac.fleet.resolve(None).unwrap().recalled.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the daemon notes the recall");
    let page = call(&web, get("/")).await;
    assert_eq!(page.status, Code::OK);
    assert!(!page.body.contains("<img src=x"), "{}", page.body);
    let presets = &page_model(&page.body)["device"]["presets"];
    let rows = presets["rows"].as_array().expect("the presets");
    assert!(rows.iter().any(|row| row["name"] == markup), "{presets}");
    assert_eq!(presets["status"]["name"], markup, "{presets}");
    let reply = call(&web, get("/api/v1/devices/dac/model")).await;
    assert_eq!(reply.header("content-type"), Some("application/json"));
    dac.stop().await;
}

/// The mode track, the preset buttons and the band rows are inert for a viewer who may not write
/// (writes off, or a guest), and live for one who may: the model says so once for the device
/// (`ember-parts.mjs` holds the same name for the controls it marks).
#[tokio::test]
async fn the_tone_controls_are_inert_for_a_viewer_who_may_not_write() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    // How many of each control the model holds, and whether the device is inert.
    let tone = |body: &str| {
        let said: serde_json::Value = serde_json::from_str(body).unwrap();
        let device = &said["device"];
        let count = |value: &serde_json::Value| value.as_array().map_or(0, Vec::len);
        (
            [
                count(&device["tone"]["modes"]),
                count(&device["presets"]["rows"]),
                count(&device["tone"]["bands"]["rows"]),
            ],
            device["inert"].clone(),
        )
    };
    let live = call(
        &web(open(), Arc::clone(&dac.fleet)),
        get("/api/v1/devices/dac/model"),
    )
    .await;
    assert_eq!(tone(&live.body), ([3, 4, 5], false.into()));
    let read_only = WebConfig {
        writes: false,
        ..open()
    };
    let inert = call(
        &web(read_only, Arc::clone(&dac.fleet)),
        get("/api/v1/devices/dac/model"),
    )
    .await;
    assert_eq!(tone(&inert.body), ([3, 4, 5], true.into()));
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let guest = call(&guests, get("/api/v1/devices/dac/model")).await;
    assert_eq!(tone(&guest.body), ([3, 4, 5], true.into()));
    dac.stop().await;
}

/// The strip's JSON in the page cannot close its `<script>` element, whatever a mark says, and
/// reads back as the same text.
#[test]
fn the_strips_json_cannot_close_its_script_element() {
    let strip = crate::web::view::Strip::for_test_with_mark("</script><b>");
    let json = super::embed(&strip);
    assert!(!json.contains('<'), "{json}");
    let back: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(back["marks"][0]["text"], "</script><b>");
}

/// A device's model holds the glow as a level and nothing else of it: no layer, no markup.
#[tokio::test]
async fn the_model_holds_the_glow_level_and_no_markup() {
    let device = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&device.fleet));
    let reply = call(&web, get("/api/v1/devices/dac/model")).await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert!(
        said["device"]["glow"].is_number(),
        "the level still travels: {said}"
    );
    let glow = said["device"]["glow"].as_f64().expect("a number");
    assert!((0.0..=1.0).contains(&glow), "{glow}");
    let fields = said["device"].as_object().expect("the device");
    assert!(
        fields
            .keys()
            .all(|key| !key.contains("fx") && !key.contains("layer")),
        "{:?}",
        fields.keys().collect::<Vec<_>>()
    );
    device.stop().await;
}

/// The pill's own part carries no menu: the model holds its word and its state and nothing else,
/// so the effects button outlives every redraw of it (`ember-parts.mjs` holds the same name).
#[tokio::test]
async fn the_link_part_holds_no_menu_button() {
    let device = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&device.fleet));
    let reply = call(&web, get("/api/v1/devices/dac/model")).await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let link = said["device"]["link"].as_object().expect("the link part");
    let mut keys: Vec<&str> = link.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["state", "word"]);
    device.stop().await;
}
