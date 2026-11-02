// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::time::Duration;

use svolder_ipc::config::{WebConfig, WebGuest};
use svolder_ipc::{Request, Response};

use crate::fleet::{DaemonFleet, Fleet};
use crate::web::test_support::{Code, FakeDaemon, call, fake_daemon, open, post, web, web_with};
use crate::writer::{Mode, Origin};

/// The fake's volume as the daemon reads it now.
async fn volume(dac: &FakeDaemon) -> i64 {
    let Response::Value(value) = dac
        .daemon
        .handle_from(
            Origin::Socket,
            Request::Get {
                device: None,
                key: "volume".into(),
            },
        )
        .await
    else {
        panic!("no volume");
    };
    value.raw.as_i64().expect("a step")
}

/// Set the fake's volume to `raw` as a socket client would.
async fn set_volume(dac: &FakeDaemon, raw: i64) {
    let answer = dac
        .daemon
        .handle_from(
            Origin::Socket,
            Request::Set {
                device: None,
                key: "volume".into(),
                value: raw.to_string(),
                step: 1,
                no_wait: false,
            },
        )
        .await;
    assert!(matches!(answer, Response::Value(_)), "{answer:?}");
}

/// The knob dragged to the top climbs instead of jumping: three quarters at once, then a tenth of
/// the range a second, through the writer, answered as soon as the climb is on.
#[tokio::test(start_paused = true)]
async fn the_web_knob_at_the_top_climbs_instead_of_jumping() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 63).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&web, post("/api/v1/devices/dac/volume", r#"{"raw":127}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["climbing"]["target"], 127, "{said}");
    assert!(
        volume(&dac).await <= 95,
        "the first write went past three quarters"
    );
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after_one = volume(&dac).await;
    assert!(after_one <= 95 + 14, "{after_one}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(volume(&dac).await, 127);
    dac.stop().await;
}

/// A touch on the dial during another front end's climb stops it where the device is: nothing
/// above that level is written afterwards.
#[tokio::test(start_paused = true)]
async fn a_web_touch_stops_another_front_ends_climb() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 63).await;
    let mqtt = DaemonFleet::new(Arc::downgrade(&dac.daemon), Origin::Mqtt, Mode::Latest);
    let info = dac.fleet.resolve(Some("dac")).unwrap();
    let _ = mqtt.glide(&info, "volume", 127).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&web, post("/api/v1/devices/dac/volume/stop", "{}")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let stopped = volume(&dac).await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(
        volume(&dac).await <= stopped + 1,
        "the climb went on after the touch"
    );
    assert!(volume(&dac).await < 127);
    dac.stop().await;
}

/// Two pages dragging the knob at once are both the web's: one climb, steered, never two jumps
/// to three quarters and never faster than a tenth of the range a second. The second page's raise
/// comes while the climb is above three quarters, so it must steer the climb, not jump again.
#[tokio::test(start_paused = true)]
async fn two_pages_dragging_the_knob_at_once_steer_one_climb() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 40).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let first = call(&web, post("/api/v1/devices/dac/volume", r#"{"raw":110}"#)).await;
    assert!(first.status.is_success(), "{first:?}");
    let mut seen = vec![volume(&dac).await];
    assert_eq!(seen[0], 95, "the first raise is at most three quarters");
    let mut posted = false;
    for tick in 0..60 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if tick == 3 {
            let second = call(&web, post("/api/v1/devices/dac/volume", r#"{"raw":127}"#)).await;
            assert!(second.status.is_success(), "{second:?}");
            posted = true;
        }
        seen.push(volume(&dac).await);
    }
    assert!(posted);
    for pair in seen.windows(2) {
        let (last, now) = (pair[0], pair[1]);
        assert!(now >= last, "a drop from {last} to {now}: {seen:?}");
        assert!(now - last <= 2, "a jump from {last} to {now}: {seen:?}");
    }
    // A tenth of the range is 12.7 steps a second: 14 leaves room for the tick that lands between.
    for window in seen.windows(11) {
        assert!(
            window[10] - window[0] <= 14,
            "faster than the climb: {seen:?}"
        );
    }
    assert_eq!(*seen.last().unwrap(), 127, "{seen:?}");
    dac.stop().await;
}

/// A cap no step is at or below refuses a raise with the cap's words, and writes nothing. The
/// Freya 2's volume runs from −80 dB with no silence, so −200 dB is below every step (a
/// Gungnir 2's bottom step is silence, which is below any cap).
#[tokio::test]
async fn a_cap_below_every_step_refuses_the_raise_with_its_words() {
    let pre = fake_daemon("freya2", "pre").await;
    set_volume(&pre, 40).await;
    let capped = WebConfig {
        max_volume_db: Some(-200.0),
        ..open()
    };
    let web = web(capped, Arc::clone(&pre.fleet));
    let reply = call(&web, post("/api/v1/devices/pre/volume", r#"{"raw":100}"#)).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert!(reply.body.contains("max_volume_db"), "{}", reply.body);
    assert_eq!(volume(&pre).await, 40);
    pre.stop().await;
}

/// Under that cap a lowering from the page is not refused: it reaches the device at the quietest
/// step through the writer, and a raise from there is still refused.
#[tokio::test]
async fn a_cap_below_every_step_lets_the_page_turn_the_volume_down_to_the_quietest_step() {
    let pre = fake_daemon("freya2", "pre").await;
    set_volume(&pre, 40).await;
    let capped = WebConfig {
        max_volume_db: Some(-200.0),
        ..open()
    };
    let web = web(capped, Arc::clone(&pre.fleet));
    let lower = call(&web, post("/api/v1/devices/pre/volume", r#"{"raw":20}"#)).await;
    assert_eq!(lower.status, Code::OK, "{}", lower.body);
    assert_eq!(volume(&pre).await, 0);
    let raise = call(&web, post("/api/v1/devices/pre/volume", r#"{"raw":30}"#)).await;
    assert_eq!(raise.status, Code::CONFLICT, "{}", raise.body);
    assert_eq!(volume(&pre).await, 0);
    pre.stop().await;
}

/// A raw step outside the volume's range is refused with 422 and the volume stays where it was;
/// so is the stop of a device with no volume (404).
#[tokio::test]
async fn a_volume_step_out_of_range_is_refused_and_a_device_without_volume_is_not_found() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 40).await;
    let page = web(open(), Arc::clone(&dac.fleet));
    for body in [r#"{"raw":128}"#, r#"{"raw":-1}"#] {
        let reply = call(&page, post("/api/v1/devices/dac/volume", body)).await;
        assert_eq!(
            reply.status,
            Code::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            reply.body
        );
    }
    let reply = call(
        &page,
        post("/api/v1/devices/dac/controls/volume", r#"{"raw":200}"#),
    )
    .await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert_eq!(volume(&dac).await, 40);
    dac.stop().await;
    let amp = fake_daemon("tyr", "amp").await;
    let page = web(open(), Arc::clone(&amp.fleet));
    let reply = call(&page, post("/api/v1/devices/amp/volume/stop", "{}")).await;
    assert_eq!(reply.status, Code::NOT_FOUND, "{}", reply.body);
    amp.stop().await;
}

/// A cap clamps a raise to it.
#[tokio::test]
async fn the_cap_clamps_the_knob() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 40).await;
    let web = web(
        WebConfig {
            max_volume_db: Some(-30.0),
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    let reply = call(&web, post("/api/v1/devices/dac/volume", r#"{"raw":90}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let raw = volume(&dac).await;
    let profile = dac
        .fleet
        .profile(&dac.fleet.resolve(Some("dac")).unwrap())
        .unwrap();
    let control = svolder_profiles::roles::volume(&profile).unwrap();
    let db = svolder_profiles::roles::step_value(control, raw).unwrap();
    assert!(db <= -30.0, "{db}");
    dac.stop().await;
}

/// A select by name, a toggle as a bool and a centred level from its centre are written, and the
/// answer carries the device's echo and the part that holds them.
#[tokio::test]
async fn controls_are_written_and_answered_with_the_echo() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        post(
            "/api/v1/devices/dac/controls/input",
            r#"{"value":"Optical"}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let input = said["parts"]["controls"]
        .as_array()
        .and_then(|controls| controls.iter().find(|c| c["key"] == "input"))
        .unwrap_or_else(|| panic!("the controls part holds the input: {said}"));
    let optical = input["options"]
        .as_array()
        .and_then(|options| options.iter().find(|o| o["label"] == "Optical"))
        .expect("Optical");
    assert_eq!(optical["on"], true, "{input}");
    let reply = call(
        &web,
        post("/api/v1/devices/dac/controls/phase", r#"{"value":true}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let reply = call(
        &web,
        post("/api/v1/devices/dac/controls/balance", r#"{"value":-5}"#),
    )
    .await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!((&said["raw"], &said["text"]), (&59.into(), &"−5".into()));
    assert!(said["parts"].is_object(), "{said}");
    let reply = call(
        &web,
        post("/api/v1/devices/dac/controls/balance", r#"{"raw":70}"#),
    )
    .await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["raw"], 70);
    for (key, body) in [
        ("input", r#"{"value":"Nowhere"}"#),
        ("phase", r#"{"value":"on"}"#),
        ("sample_rate", r#"{"value":1}"#),
        ("nope", r#"{"value":1}"#),
        ("balance", r#"{"value":99}"#),
    ] {
        let reply = call(
            &web,
            post(&format!("/api/v1/devices/dac/controls/{key}"), body),
        )
        .await;
        assert!(
            reply.status.is_client_error(),
            "{key} {body}: {:?}",
            reply.status
        );
        assert!(reply.body.contains("\"error\""), "{key}: {}", reply.body);
    }
    dac.stop().await;
}

/// A control write answers its echo and the parts it changed: the controls, which hold the toggle
/// as it reads now.
#[tokio::test]
async fn a_control_write_answers_its_echo_and_its_parts() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        post("/api/v1/devices/dac/controls/phase", r#"{"value":true}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["raw"], 1, "{said}");
    let controls = said["parts"]["controls"].as_array().expect("controls part");
    let phase = controls
        .iter()
        .find(|c| c["key"] == "phase")
        .expect("the phase toggle");
    assert_eq!(phase["kind"], "toggle");
    assert_eq!(phase["on"], true, "{phase}");
    assert!(said.get("toast").is_none(), "{said}");
    dac.stop().await;
}

/// A control answers with the part that holds it: an amp's switches are in the power part, a
/// DAC's in the controls, whatever the request carries; the page's press names no target.
#[tokio::test]
async fn a_control_answers_with_the_part_that_holds_it() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    // A press from a page that still sends htmx's headers is answered as any other.
    let mut press = post("/api/v1/devices/dac/controls/phase", r#"{"value":true}"#);
    press
        .headers_mut()
        .insert("hx-target", "part-power".parse().unwrap());
    let reply = call(&web, press).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert!(said["parts"]["controls"].is_array(), "{said}");
    assert!(
        said["parts"].get("power").is_none(),
        "no power part here: {said}"
    );
    dac.stop().await;
    let amp = fake_daemon("aegir2", "amp").await;
    let amp_web = self::web(open(), Arc::clone(&amp.fleet));
    let aegir = crate::test_support::profile_named("aegir2");
    let switch = aegir
        .controls
        .iter()
        .find(|c| {
            c.kind == svolder_profiles::ControlKind::Toggle && !c.powers_output && !c.auto_off
        })
        .expect("a switch of the amp");
    let path = format!("/api/v1/devices/amp/controls/{}", switch.key);
    let reply = call(&amp_web, post(&path, r#"{"value":true}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let toggles = said["parts"]["power"]["toggles"]
        .as_array()
        .expect("the power part");
    let held = toggles.iter().find(|t| t["key"] == switch.key.as_str());
    assert!(
        held.is_some(),
        "the amp's switches are in the power part: {said}"
    );
    amp.stop().await;
}

/// A write answers JSON, whatever the request carries: one that comes with htmx's headers is
/// answered as any other: a write's answer is JSON for the page to draw from.
#[tokio::test]
async fn every_write_answers_json_even_from_htmx() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let from_htmx = |path: &str, body: &str| {
        let mut request = post(path, body);
        let headers = request.headers_mut();
        headers.insert("hx-request", "true".parse().unwrap());
        headers.insert("hx-target", "part-controls".parse().unwrap());
        request
    };
    for (path, body) in [
        ("/api/v1/devices/dac/volume", r#"{"raw":40}"#),
        ("/api/v1/devices/dac/controls/phase", r#"{"value":true}"#),
        ("/api/v1/devices/dac/eq/mode", r#"{"mode":"eq"}"#),
        (
            "/api/v1/devices/dac/eq/bands/1",
            r#"{"param":"g","step":1}"#,
        ),
        ("/api/v1/devices/dac/presets/recall", r#"{"name":"Warm"}"#),
        ("/api/v1/devices/dac/presets", r#"{"name":"Mine"}"#),
    ] {
        let reply = call(&web, from_htmx(path, body)).await;
        assert_eq!(reply.status, Code::OK, "{path}: {}", reply.body);
        assert_eq!(
            reply.header("content-type"),
            Some("application/json"),
            "{path}"
        );
        assert!(reply.header("hx-trigger").is_none(), "{path}");
        let said: serde_json::Value = serde_json::from_str(&reply.body)
            .unwrap_or_else(|e| panic!("{path}: {e}: {}", reply.body));
        assert!(said.is_object(), "{path}: {said}");
    }
    dac.stop().await;
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let loki_web = self::web(open(), Arc::clone(&loki.fleet));
    for (path, body) in [
        (
            "/api/v1/devices/Loki%20Max/device-presets/recall",
            r#"{"slot":2}"#,
        ),
        (
            "/api/v1/devices/Loki%20Max/device-presets/save",
            r#"{"slot":2,"confirm":true}"#,
        ),
        (
            "/api/v1/devices/Loki%20Max/device-presets/apply",
            r#"{"name":"Bass Boost"}"#,
        ),
    ] {
        let reply = call(&loki_web, from_htmx(path, body)).await;
        assert_eq!(reply.status, Code::OK, "{path}: {}", reply.body);
        assert_eq!(
            reply.header("content-type"),
            Some("application/json"),
            "{path}"
        );
        assert!(reply.header("hx-trigger").is_none(), "{path}");
        let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
        assert!(said["toast"].is_string(), "{path}: {said}");
    }
    loki.stop().await;
}

/// A guest's write is refused 401, a page with writes off 403, each saying so; the device is
/// not touched.
#[tokio::test]
async fn a_guest_and_a_read_only_page_cannot_write() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 40).await;
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
    let reply = call(&guests, post("/api/v1/devices/dac/volume", r#"{"raw":50}"#)).await;
    assert_eq!(reply.status, Code::UNAUTHORIZED);
    assert!(
        reply.body.contains("Sign in to change this"),
        "{}",
        reply.body
    );
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    for (path, body) in [
        ("/api/v1/devices/dac/volume", r#"{"raw":50}"#),
        ("/api/v1/devices/dac/controls/mute", r#"{"value":true}"#),
        ("/api/v1/devices/dac/connect", "{}"),
        ("/api/v1/devices/dac/identify", "{}"),
    ] {
        let reply = call(&read_only, post(path, body)).await;
        assert_eq!(reply.status, Code::FORBIDDEN, "{path}");
        assert!(
            reply.body.contains("Read-only: [web] writes is off"),
            "{}",
            reply.body
        );
    }
    assert_eq!(volume(&dac).await, 40);
    dac.stop().await;
}

/// A guest is refused 401 on every write route, whatever it posts, and nothing is written.
#[tokio::test]
async fn a_guest_is_refused_on_every_write_route() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 40).await;
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
    for (path, body) in [
        ("/api/v1/devices/dac/volume", r#"{"raw":50}"#),
        ("/api/v1/devices/dac/volume/stop", "{}"),
        ("/api/v1/devices/dac/connect", "{}"),
        ("/api/v1/devices/dac/identify", "{}"),
        ("/api/v1/devices/dac/controls/mute", r#"{"value":true}"#),
        ("/api/v1/devices/dac/controls/volume", r#"{"raw":50}"#),
        (
            "/api/v1/devices/dac/controls/input",
            r#"{"value":"Optical"}"#,
        ),
        ("/api/v1/hops", r#"{"name":"Amp","kind":"amp"}"#),
        ("/api/v1/hops/edit", r#"{"id":"Amp","note":"x"}"#),
        ("/api/v1/hops/remove", r#"{"id":"Amp"}"#),
        ("/api/v1/hops/move", r#"{"id":"Amp","to":0}"#),
    ] {
        let reply = call(&guests, post(path, body)).await;
        assert_eq!(reply.status, Code::UNAUTHORIZED, "{path}: {}", reply.body);
    }
    assert_eq!(volume(&dac).await, 40);
    dac.stop().await;
}

/// The volume written through `controls/{key}` takes the same road as `volume`: capped, and a
/// raise to the top climbs instead of jumping.
#[tokio::test(start_paused = true)]
async fn the_volume_through_controls_is_capped_and_glides() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 63).await;
    let plain = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &plain,
        post("/api/v1/devices/dac/controls/volume", r#"{"raw":127}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(volume(&dac).await <= 95, "the raise jumped");
    let capped = WebConfig {
        max_volume_db: Some(-30.0),
        ..open()
    };
    let capped_web = web(capped, Arc::clone(&dac.fleet));
    set_volume(&dac, 40).await;
    let reply = call(
        &capped_web,
        post("/api/v1/devices/dac/controls/volume", r#"{"raw":90}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let profile = dac
        .fleet
        .profile(&dac.fleet.resolve(Some("dac")).unwrap())
        .unwrap();
    let control = svolder_profiles::roles::volume(&profile).unwrap();
    let raw = volume(&dac).await;
    assert!(
        svolder_profiles::roles::step_value(control, raw).unwrap() <= -30.0,
        "{raw}"
    );
    dac.stop().await;
}

/// A fleet that is the daemon's own but for Try now, which it counts and answers as told.
struct Counting {
    inner: Arc<dyn Fleet>,
    wakes: std::sync::atomic::AtomicUsize,
    configs: std::sync::atomic::AtomicUsize,
    answer: Response,
}

#[async_trait::async_trait]
impl Fleet for Counting {
    fn resolve(&self, selector: Option<&str>) -> Result<svolder_ipc::DeviceInfo, String> {
        self.inner.resolve(selector)
    }
    async fn request(&self, request: Request) -> Response {
        self.inner.request(request).await
    }
    async fn recall(
        &self,
        device: &svolder_ipc::DeviceInfo,
        name: &str,
        fitting: Vec<svolder_profiles::tone::preset::Preset>,
    ) -> Result<(crate::recall::Recalled, Response), String> {
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
    async fn submit(
        &self,
        device: &svolder_ipc::DeviceInfo,
        command: crate::device::Command,
        options: crate::writer::Options,
    ) -> crate::writer::Outcome {
        self.inner.submit(device, command, options).await
    }
    async fn wake(&self, _device: &svolder_ipc::DeviceInfo) -> Response {
        self.wakes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.answer.clone()
    }
    fn profile(&self, device: &svolder_ipc::DeviceInfo) -> Option<svolder_profiles::Profile> {
        self.inner.profile(device)
    }
    fn watch(&self, device: &svolder_ipc::DeviceInfo) -> Option<crate::fleet::Watched> {
        self.inner.watch(device)
    }
    fn config(&self) -> Result<svolder_ipc::Config, String> {
        self.configs
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.config()
    }
}

/// Try now asks the device's actor to wake once, and says so; a refusal is a 409 in its words.
#[tokio::test]
async fn try_now_wakes_the_device_and_a_refusal_is_a_409() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    for (answer, status) in [
        (Response::Ack, Code::OK),
        (
            Response::Error {
                code: 3,
                message: "the daemon is stopping".into(),
            },
            Code::CONFLICT,
        ),
    ] {
        let fleet = Arc::new(Counting {
            inner: Arc::clone(&dac.fleet),
            wakes: 0.into(),
            configs: 0.into(),
            answer,
        });
        let web = web(open(), Arc::clone(&fleet) as Arc<dyn Fleet>);
        let reply = call(&web, post("/api/v1/devices/dac/connect", "{}")).await;
        assert_eq!(reply.status, status, "{}", reply.body);
        assert_eq!(fleet.wakes.load(std::sync::atomic::Ordering::SeqCst), 1);
        if status == Code::CONFLICT {
            assert!(
                reply.body.contains("the daemon is stopping"),
                "{}",
                reply.body
            );
        }
    }
    dac.stop().await;
}

/// A write does not read the configuration file: a drag streams writes, and the part it answers
/// with has no presets.
#[tokio::test]
async fn a_write_does_not_read_the_configuration() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 40).await;
    let fleet = Arc::new(Counting {
        inner: Arc::clone(&dac.fleet),
        wakes: 0.into(),
        configs: 0.into(),
        answer: Response::Ack,
    });
    let web = web(open(), Arc::clone(&fleet) as Arc<dyn Fleet>);
    let reply = call(&web, post("/api/v1/devices/dac/volume", r#"{"raw":50}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let reply = call(
        &web,
        post("/api/v1/devices/dac/controls/phase", r#"{"value":true}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(fleet.configs.load(std::sync::atomic::Ordering::SeqCst), 0);
    dac.stop().await;
}

/// Mute from the page ends another front end's climb, as the writer's cancel set says.
#[tokio::test(start_paused = true)]
async fn a_web_mute_stops_another_front_ends_climb() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    set_volume(&dac, 63).await;
    let mqtt = DaemonFleet::new(Arc::downgrade(&dac.daemon), Origin::Mqtt, Mode::Latest);
    let info = dac.fleet.resolve(Some("dac")).unwrap();
    let _ = mqtt.glide(&info, "volume", 127).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        post("/api/v1/devices/dac/controls/mute", r#"{"value":true}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let at = volume(&dac).await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(
        volume(&dac).await <= at + 1,
        "the climb went on after the mute"
    );
    dac.stop().await;
}

/// A device named with a space and a slash is written through its percent-encoded name.
#[tokio::test]
async fn a_write_reaches_a_device_named_with_a_space_and_a_slash() {
    let dac = fake_daemon("gungnir2-v2", "Living room/DAC").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let path = "/api/v1/devices/Living%20room%2FDAC/controls/phase";
    let reply = call(&web, post(path, r#"{"value":true}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    dac.stop().await;
}

/// A write a newer one took the place of answers 204 with nothing in it, so the page draws
/// nothing; one that never reached the device answers 409 with why.
#[tokio::test]
async fn a_superseded_write_answers_204_and_draws_nothing() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let answer = super::outcome(&web, "dac", crate::writer::Outcome::Superseded, &[], |_| {
        serde_json::Value::Null
    });
    assert_eq!(answer.status(), Code::NO_CONTENT);
    let answer = super::outcome(
        &web,
        "dac",
        crate::writer::Outcome::Cancelled("the link went down"),
        &[],
        |_| serde_json::Value::Null,
    );
    assert_eq!(answer.status(), Code::CONFLICT);
    dac.stop().await;
}

/// A fleet that is the daemon's own but for what it answers an identify request.
struct Identifying {
    inner: Arc<dyn Fleet>,
    answer: Response,
}

#[async_trait::async_trait]
impl Fleet for Identifying {
    fn resolve(&self, selector: Option<&str>) -> Result<svolder_ipc::DeviceInfo, String> {
        self.inner
            .resolve(selector)
            .map(|info| svolder_ipc::DeviceInfo {
                retry_in_secs: Some(12),
                ..info
            })
    }
    async fn request(&self, request: Request) -> Response {
        match request {
            Request::Identify { .. } => self.answer.clone(),
            other => self.inner.request(other).await,
        }
    }
    async fn recall(
        &self,
        device: &svolder_ipc::DeviceInfo,
        name: &str,
        fitting: Vec<svolder_profiles::tone::preset::Preset>,
    ) -> Result<(crate::recall::Recalled, Response), String> {
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
    fn config(&self) -> Result<svolder_ipc::Config, String> {
        self.inner.config()
    }
}

/// Identify is a write: a page with writes off is refused 403 and a guest 401, as every write
/// is, and the device is not asked; a signed-in page's answer is `{"sent": bool}`, `true` the
/// first time and `false` inside the gap the session keeps.
#[tokio::test(start_paused = true)]
async fn identify_route_obeys_the_write_gate() {
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
    let reply = call(&guests, post("/api/v1/devices/dac/identify", "{}")).await;
    assert_eq!(reply.status, Code::UNAUTHORIZED, "{}", reply.body);
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    let reply = call(&read_only, post("/api/v1/devices/dac/identify", "{}")).await;
    assert_eq!(reply.status, Code::FORBIDDEN, "{}", reply.body);
    // Neither reached the device: the first signed-in press is the first send.
    let page = web(open(), Arc::clone(&dac.fleet));
    for sent in [true, false] {
        let reply = call(&page, post("/api/v1/devices/dac/identify", "{}")).await;
        assert_eq!(reply.status, Code::OK, "{}", reply.body);
        let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
        assert_eq!(said, serde_json::json!({ "sent": sent }));
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    let reply = call(&page, post("/api/v1/devices/dac/identify", "{}")).await;
    assert!(reply.body.contains("\"sent\":true"), "{}", reply.body);
    // The route is behind the guard like every write: JSON only.
    let mut plain = post("/api/v1/devices/dac/identify", "{}");
    plain
        .headers_mut()
        .insert("content-type", "text/plain".parse().unwrap());
    assert_eq!(
        call(&page, plain).await.status,
        Code::UNSUPPORTED_MEDIA_TYPE
    );
    dac.stop().await;
}

/// A device that is down answers identify 409 in the daemon's words with the wait to the next
/// attempt; any other refusal is a 409 in its own words; an unknown device is a 404.
#[tokio::test]
async fn identify_on_a_device_that_is_down_says_so_with_the_wait() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let down = Arc::new(Identifying {
        inner: Arc::clone(&dac.fleet),
        answer: Response::Error {
            code: 3,
            message: svolder_ipc::NOT_CONNECTED.to_string(),
        },
    });
    let page = web(open(), down as Arc<dyn Fleet>);
    let reply = call(&page, post("/api/v1/devices/dac/identify", "{}")).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert!(
        reply
            .body
            .contains("device not connected, next attempt in 12 s"),
        "{}",
        reply.body
    );
    let other = Arc::new(Identifying {
        inner: Arc::clone(&dac.fleet),
        answer: Response::Error {
            code: 3,
            message: "the transport failed".to_string(),
        },
    });
    let page = web(open(), other as Arc<dyn Fleet>);
    let reply = call(&page, post("/api/v1/devices/dac/identify", "{}")).await;
    assert_eq!(reply.status, Code::CONFLICT);
    assert!(
        reply.body.contains("the transport failed"),
        "{}",
        reply.body
    );
    let reply = call(&page, post("/api/v1/devices/nope/identify", "{}")).await;
    assert_eq!(reply.status, Code::NOT_FOUND, "{}", reply.body);
    dac.stop().await;
}

/// A fleet that is the daemon's own and logs what the page sends through it: each write with how
/// it waits, and each built-in setting applied.
struct Recording {
    inner: Arc<dyn Fleet>,
    sent: std::sync::Mutex<Vec<(String, crate::writer::Options)>>,
    applied: std::sync::Mutex<Vec<String>>,
}

impl Recording {
    fn over(dac: &FakeDaemon) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::clone(&dac.fleet),
            sent: Default::default(),
            applied: Default::default(),
        })
    }

    /// The writes sent, as the stand-ins log them.
    fn sent(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|(shown, _)| shown.clone())
            .collect()
    }
}

#[async_trait::async_trait]
impl Fleet for Recording {
    fn resolve(&self, selector: Option<&str>) -> Result<svolder_ipc::DeviceInfo, String> {
        self.inner.resolve(selector)
    }
    async fn request(&self, request: Request) -> Response {
        self.inner.request(request).await
    }
    async fn recall(
        &self,
        device: &svolder_ipc::DeviceInfo,
        name: &str,
        fitting: Vec<svolder_profiles::tone::preset::Preset>,
    ) -> Result<(crate::recall::Recalled, Response), String> {
        self.inner.recall(device, name, fitting).await
    }
    async fn apply(
        &self,
        device: &svolder_ipc::DeviceInfo,
        name: &str,
    ) -> Result<(crate::recall::Applied, Response), String> {
        self.applied.lock().unwrap().push(name.to_string());
        self.inner.apply(device, name).await
    }
    async fn glide(
        &self,
        device: &svolder_ipc::DeviceInfo,
        key: &str,
        raw: i64,
    ) -> crate::writer::Glided {
        self.inner.glide(device, key, raw).await
    }
    async fn submit(
        &self,
        device: &svolder_ipc::DeviceInfo,
        command: crate::device::Command,
        options: crate::writer::Options,
    ) -> crate::writer::Outcome {
        let shown = crate::test_support::shown(&command);
        self.sent.lock().unwrap().push((shown, options));
        self.inner.submit(device, command, options).await
    }
    fn profile(&self, device: &svolder_ipc::DeviceInfo) -> Option<svolder_profiles::Profile> {
        self.inner.profile(device)
    }
    fn watch(&self, device: &svolder_ipc::DeviceInfo) -> Option<crate::fleet::Watched> {
        self.inner.watch(device)
    }
    fn config(&self) -> Result<svolder_ipc::Config, String> {
        self.inner.config()
    }
}

/// The band `key`'s reading on the Loki Max `dac` as the daemon holds it, in raw steps.
async fn band(dac: &FakeDaemon, key: &str) -> Option<i64> {
    let Response::Value(value) = dac
        .daemon
        .handle_from(
            Origin::Socket,
            Request::Get {
                device: None,
                key: key.into(),
            },
        )
        .await
    else {
        return None;
    };
    value.raw.as_i64()
}

/// A slot's recall from the page is the recall command of that slot through the writer, in order,
/// and the page is answered with its presets part and a toast; a slot the device lacks is refused.
#[tokio::test]
async fn a_slot_recall_posts_the_recall_through_the_writer() {
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let fleet = Recording::over(&loki);
    let web = web(open(), Arc::clone(&fleet) as Arc<dyn Fleet>);
    let url = "/api/v1/devices/Loki%20Max/device-presets/recall";
    let reply = call(&web, post(url, r#"{"slot":2}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["slot"], 2, "{said}");
    assert_eq!(fleet.sent(), ["set recall 2"]);
    assert_eq!(
        fleet.sent.lock().unwrap()[0].1,
        crate::writer::Options::from(Mode::Ordered)
    );
    let reply = call(&web, post(url, r#"{"slot":3}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert!(said["parts"]["device_presets"].is_object(), "{said}");
    assert_eq!(said["toast"], "Recalled Preset 3", "{said}");
    for body in [
        r#"{"slot":0}"#,
        r#"{"slot":4}"#,
        r#"{"slot":-1}"#,
        r#"{"slot":300}"#,
    ] {
        let reply = call(&web, post(url, body)).await;
        assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{body}");
    }
    assert_eq!(fleet.sent(), ["set recall 2", "set recall 3"]);
    loki.stop().await;
}

/// A slot's recall answers its toast and the device presets part, with the slots.
#[tokio::test]
async fn a_slot_recall_answers_its_toast_and_the_part() {
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let web = web(open(), Arc::clone(&loki.fleet));
    let url = "/api/v1/devices/Loki%20Max/device-presets/recall";
    let reply = call(&web, post(url, r#"{"slot":2}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["slot"], 2, "{said}");
    assert_eq!(said["toast"], "Recalled Preset 2", "{said}");
    assert_eq!(
        said["parts"]["device_presets"]["slots"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let url = "/api/v1/devices/Loki%20Max/device-presets/save";
    let reply = call(&web, post(url, r#"{"slot":2,"confirm":true}"#)).await;
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["toast"], "Saved to Preset 2", "{said}");
    assert!(said["parts"]["device_presets"].is_object(), "{said}");
    loki.stop().await;
}

/// A setting applied twice says so the second time; the first says it is on.
#[tokio::test]
async fn a_setting_already_on_says_so() {
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let web = web(open(), Arc::clone(&loki.fleet));
    let url = "/api/v1/devices/Loki%20Max/device-presets/apply";
    let first = call(&web, post(url, r#"{"name":"Bass Boost"}"#)).await;
    assert_eq!(first.status, Code::OK, "{}", first.body);
    let said: serde_json::Value = serde_json::from_str(&first.body).unwrap();
    assert_eq!(said["toast"], "Bass Boost on", "{said}");
    assert!(said["parts"]["device_presets"].is_object(), "{said}");
    let again = call(&web, post(url, r#"{"name":"Bass Boost"}"#)).await;
    let said: serde_json::Value = serde_json::from_str(&again.body).unwrap();
    assert_eq!(said["toast"], "Bass Boost is already on", "{said}");
    loki.stop().await;
}

/// A built-in setting from the page is one batch through the writer's apply, not six band writes
/// of the page's own, and the bands end on it.
#[tokio::test]
async fn a_builtin_apply_goes_through_the_writer_as_one_batch() {
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let fleet = Recording::over(&loki);
    let web = web(open(), Arc::clone(&fleet) as Arc<dyn Fleet>);
    let url = "/api/v1/devices/Loki%20Max/device-presets/apply";
    let reply = call(&web, post(url, r#"{"name":"bass boost"}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(*fleet.applied.lock().unwrap(), ["bass boost"]);
    assert!(fleet.sent().is_empty(), "{:?}", fleet.sent());
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["toast"], "Bass Boost on", "{said}");
    let lit: Vec<&serde_json::Value> = said["parts"]["device_presets"]["builtins"]
        .as_array()
        .expect("the builtins")
        .iter()
        .filter(|b| b["on"] == true)
        .map(|b| &b["name"])
        .collect();
    assert_eq!(lit, ["Bass Boost"], "{said}");
    for (key, raw) in [("band1", 28), ("band2", 21), ("band3", 17), ("band6", 15)] {
        assert_eq!(band(&loki, key).await, Some(raw), "{key}");
    }
    let again = call(&web, post(url, r#"{"name":"Bass Boost"}"#)).await;
    let said: serde_json::Value = serde_json::from_str(&again.body).unwrap();
    assert_eq!(said["toast"], "Bass Boost is already on", "{said}");
    let unknown = call(&web, post(url, r#"{"name":"Sparkle"}"#)).await;
    assert_eq!(unknown.status, Code::CONFLICT);
    assert!(unknown.body.contains("Sparkle"), "{}", unknown.body);
    loki.stop().await;
}

/// A save without `"confirm": true` is refused 422 and nothing is sent, whatever else the body
/// says; with it the save is the save command of that slot, in order.
#[tokio::test]
async fn a_slot_save_without_confirm_true_is_refused_and_sends_nothing() {
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let fleet = Recording::over(&loki);
    let web = web(open(), Arc::clone(&fleet) as Arc<dyn Fleet>);
    let url = "/api/v1/devices/Loki%20Max/device-presets/save";
    for body in [
        r#"{"slot":2}"#,
        r#"{"slot":2,"confirm":false}"#,
        r#"{"slot":2,"confirm":"true"}"#,
        r#"{"slot":2,"confirm":1}"#,
        r#"{"slot":2,"confirm":null}"#,
    ] {
        let reply = call(&web, post(url, body)).await;
        assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{body}");
    }
    let reply = call(&web, post(url, r#"{"slot":2}"#)).await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert!(reply.body.contains("confirm"), "{}", reply.body);
    assert!(fleet.sent().is_empty(), "{:?}", fleet.sent());
    let reply = call(&web, post(url, r#"{"slot":9,"confirm":true}"#)).await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert!(fleet.sent().is_empty(), "{:?}", fleet.sent());
    let reply = call(&web, post(url, r#"{"slot":2,"confirm":true}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(fleet.sent(), ["set save 2"]);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["toast"], "Saved to Preset 2", "{said}");
    loki.stop().await;
}

/// A save needs a signed-in writer: a guest is refused 401 and a page with writes off 403, with
/// `confirm: true` in the body, and nothing is sent.
#[tokio::test]
async fn a_slot_save_needs_writes() {
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let fleet = Recording::over(&loki);
    let token = "a-test-token-0123456789abcdefghijklmn";
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&fleet) as Arc<dyn Fleet>,
        Some(token),
    );
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&fleet) as Arc<dyn Fleet>,
    );
    for (web, status) in [(&guests, Code::UNAUTHORIZED), (&read_only, Code::FORBIDDEN)] {
        for (path, body) in [
            ("save", r#"{"slot":2,"confirm":true}"#),
            ("recall", r#"{"slot":2}"#),
            ("apply", r#"{"name":"Flat"}"#),
        ] {
            let url = format!("/api/v1/devices/Loki%20Max/device-presets/{path}");
            let reply = call(web, post(&url, body)).await;
            assert_eq!(reply.status, status, "{path}: {}", reply.body);
        }
    }
    assert!(fleet.sent().is_empty(), "{:?}", fleet.sent());
    assert!(fleet.applied.lock().unwrap().is_empty());
    loki.stop().await;
}

/// The control route does not reach an action: `recall` and `save` are no controls, so a write to
/// either is a 404 and nothing is sent.
#[tokio::test]
async fn the_control_route_refuses_an_action_key() {
    let loki = fake_daemon("lokimax", "Loki Max").await;
    let fleet = Recording::over(&loki);
    let web = web(open(), Arc::clone(&fleet) as Arc<dyn Fleet>);
    for key in ["recall", "save"] {
        for body in [r#"{"value":2}"#, r#"{"raw":2}"#, r#"{"value":"2"}"#] {
            let url = format!("/api/v1/devices/Loki%20Max/controls/{key}");
            let reply = call(&web, post(&url, body)).await;
            assert_eq!(
                reply.status,
                Code::NOT_FOUND,
                "{key} {body}: {}",
                reply.body
            );
        }
    }
    assert!(fleet.sent().is_empty(), "{:?}", fleet.sent());
    loki.stop().await;
}

/// A device with no presets of its own has no such routes to use: 404 saying so.
#[tokio::test]
async fn a_device_without_presets_of_its_own_has_nothing_to_recall_or_save() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let fleet = Recording::over(&dac);
    let web = web(open(), Arc::clone(&fleet) as Arc<dyn Fleet>);
    for (path, body) in [
        ("recall", r#"{"slot":1}"#),
        ("save", r#"{"slot":1,"confirm":true}"#),
        ("apply", r#"{"name":"Flat"}"#),
    ] {
        let url = format!("/api/v1/devices/dac/device-presets/{path}");
        let reply = call(&web, post(&url, body)).await;
        assert_eq!(reply.status, Code::NOT_FOUND, "{path}: {}", reply.body);
        assert!(
            reply.body.contains("no presets of its own"),
            "{}",
            reply.body
        );
    }
    assert!(fleet.sent().is_empty());
    dac.stop().await;
}
