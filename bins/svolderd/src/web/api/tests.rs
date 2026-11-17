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

/// A guest and a page with writes off read what the read-only Devices popup draws from: the fleet,
/// a device's model (its facts with the unit empty for the guest only, and no identify) and the
/// key series; the writer's device list stays refused.
#[tokio::test]
async fn a_reader_reads_what_the_read_only_devices_popup_needs() {
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
    let writes_off = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    for (who, web, unit_shown) in [("guest", &guests, false), ("writes off", &writes_off, true)] {
        let fleet = call(web, get("/api/v1/fleet")).await;
        assert_eq!(fleet.status, Code::OK, "{who}: {}", fleet.body);
        assert!(fleet.body.contains("\"dac\""), "{who}: {}", fleet.body);
        let model = call(web, get("/api/v1/devices/dac/model")).await;
        assert_eq!(model.status, Code::OK, "{who}: {}", model.body);
        let said: serde_json::Value = serde_json::from_str(&model.body).unwrap();
        let facts = &said["device"]["facts"];
        assert!(!facts.is_null(), "{who}: {said}");
        let unit = facts["unit"].as_str().unwrap_or_default();
        assert_eq!(!unit.is_empty(), unit_shown, "{who}: unit {unit:?}");
        assert_eq!(facts["identify"], false, "{who}: {facts}");
        let series = call(web, get("/api/v1/devices/dac/series")).await;
        assert_eq!(series.status, Code::OK, "{who}: {}", series.body);
        let list = call(web, get("/api/v1/devices")).await;
        assert!(
            matches!(list.status, Code::UNAUTHORIZED | Code::FORBIDDEN),
            "{who}: {}",
            list.body
        );
    }
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
    assert_eq!(keys, ["old", "state", "word"]);
    device.stop().await;
}

/// The ids of an answer's series, in order.
fn series_ids(said: &serde_json::Value) -> Vec<String> {
    said["series"]
        .as_array()
        .unwrap_or_else(|| panic!("no series in {said}"))
        .iter()
        .map(|series| series["id"].as_str().unwrap().to_string())
        .collect()
}

/// The series route answers the ids asked for from the daemon's ring, in the order asked, by
/// the key the profile gives each; an id that names nothing is listed as refused and the rest
/// are served. With no ids it answers the device's key readings.
#[tokio::test]
async fn the_series_route_serves_the_ids_it_knows_and_names_the_rest() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        get("/api/v1/devices/dac/series?ids=volume,bogus,signal,Sample_Rate,10&secs=600"),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(series_ids(&said), ["volume", "signal", "sample_rate"]);
    assert_eq!(said["refused"], serde_json::json!(["bogus", "10"]));
    assert_eq!(said["secs"], 600);
    assert_eq!(said["bin_ms"], 1000);
    assert!(said["now_ms"].as_u64().is_some_and(|ms| ms > 0), "{said}");
    let volume = &said["series"][0];
    let points = volume["points"].as_array().expect("points");
    assert!(!points.is_empty(), "the connect's state is in the ring");
    assert!(points[0][0].as_u64().is_some() && points[0][1].is_number());
    assert_eq!(volume["aggregate"], "max");
    assert_eq!(said["series"][1]["unit"], "dBm");
    assert!(said["link"].is_array(), "{said}");

    let reply = call(&web, get("/api/v1/devices/dac/series")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(
        series_ids(&said),
        ["signal", "sample_rate", "volume", "input"],
        "the key readings, signal first"
    );
    assert_eq!(said["refused"], serde_json::json!([]));
    assert_eq!(said["secs"], 1800, "the ring's half hour when not given");

    let reply = call(&web, get("/api/v1/devices/dac/series?secs=999999")).await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["secs"], 1800, "at most what the ring keeps");
    let reply = call(&web, get("/api/v1/devices/nobody/series?ids=signal")).await;
    assert_eq!(reply.status, Code::NOT_FOUND, "{}", reply.body);
    dac.stop().await;
}

/// The series are open to whoever may read the page, as the strip is: a guest reads them, and
/// a visitor who is neither signed in nor a guest gets a 401 with nothing said.
#[tokio::test]
async fn the_series_route_is_open_to_a_reader() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
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
    let closed = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some(token),
    );
    for path in [
        "/api/v1/devices/dac/series?ids=signal",
        "/api/v1/devices/dac/strip",
    ] {
        assert_eq!(call(&guests, get(path)).await.status, Code::OK, "{path}");
        let reply = call(&closed, get(path)).await;
        assert_eq!(reply.status, Code::UNAUTHORIZED, "{path}");
        assert!(reply.body.is_empty(), "{path}: {}", reply.body);
    }
    dac.stop().await;
}

/// One request cannot ask for more than the ring holds: an id asked twice, in any letter case,
/// is answered once, and more distinct ids than the cap are refused with the cap in words. The
/// span is held to the ring's half hour, and the answer says the span it covers.
#[tokio::test]
async fn the_series_route_answers_each_id_once_and_caps_their_number() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        get("/api/v1/devices/dac/series?ids=volume,VOLUME,signal,volume,Signal,bogus,BOGUS"),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(series_ids(&said), ["volume", "signal"]);
    assert_eq!(said["refused"], serde_json::json!(["bogus"]));

    let many = |count: usize| {
        let ids: Vec<String> = (0..count).map(|n| format!("id{n}")).collect();
        format!("/api/v1/devices/dac/series?ids={}", ids.join(","))
    };
    let reply = call(&web, get(&many(super::SERIES_IDS_MAX))).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let reply = call(&web, get(&many(super::SERIES_IDS_MAX + 1))).await;
    assert_eq!(reply.status, Code::BAD_REQUEST, "{}", reply.body);
    assert!(
        reply
            .body
            .contains(&format!("at most {} ids", super::SERIES_IDS_MAX)),
        "{}",
        reply.body
    );
    // The same id eight thousand times is one id, and one series.
    let repeated = format!(
        "/api/v1/devices/dac/series?ids={}",
        vec!["volume"; 8000].join(",")
    );
    let reply = call(&web, get(&repeated)).await;
    assert_eq!(reply.status, Code::OK);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(series_ids(&said), ["volume"]);
    assert!(
        reply.body.len() < 64 * 1024,
        "{} bytes for one series",
        reply.body.len()
    );
    dac.stop().await;
}

/// A fleet over a daemon whose devices are all removed just before it answers a `readings`
/// request: what a route meets when a device goes between its own look-up and the daemon's.
struct Vanishing {
    inner: Arc<dyn crate::fleet::Fleet>,
    daemon: Arc<crate::handler::Daemon>,
}

#[async_trait::async_trait]
impl crate::fleet::Fleet for Vanishing {
    fn resolve(&self, selector: Option<&str>) -> Result<svolder_ipc::DeviceInfo, String> {
        self.inner.resolve(selector)
    }

    async fn request(&self, request: svolder_ipc::Request) -> svolder_ipc::Response {
        if matches!(request, svolder_ipc::Request::Readings { .. }) {
            self.daemon.reconcile(&svolder_ipc::Config::default()).await;
        }
        self.inner.request(request).await
    }

    async fn recall(
        &self,
        device: &svolder_ipc::DeviceInfo,
        name: &str,
        fitting: Vec<svolder_profiles::tone::preset::Preset>,
    ) -> Result<(crate::recall::Recalled, svolder_ipc::Response), String> {
        self.inner.recall(device, name, fitting).await
    }

    async fn glide(
        &self,
        device: &svolder_ipc::DeviceInfo,
        key: &str,
        raw: i64,
    ) -> crate::writer::Glided {
        self.inner.glide(device, key, raw).await
    }

    fn profile(&self, device: &svolder_ipc::DeviceInfo) -> Option<svolder_profiles::Profile> {
        self.inner.profile(device)
    }

    fn watch(&self, device: &svolder_ipc::DeviceInfo) -> Option<crate::fleet::Watched> {
        self.inner.watch(device)
    }

    fn config(&self) -> Result<Arc<svolder_ipc::Config>, String> {
        self.inner.config()
    }
}

/// A device removed between the route's look-up and the daemon's answer is a 404 in the
/// daemon's words, as a device that was never there is: not a failure of the server.
#[tokio::test]
async fn a_device_removed_mid_request_is_a_404_on_the_series_route() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let fleet = Arc::new(Vanishing {
        inner: Arc::clone(&dac.fleet),
        daemon: Arc::clone(&dac.daemon),
    });
    let web = web(open(), fleet);
    let reply = call(&web, get("/api/v1/devices/dac/series?ids=signal")).await;
    assert_eq!(reply.status, Code::NOT_FOUND, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert!(said["error"].is_string(), "{said}");
    dac.stop().await;
}
