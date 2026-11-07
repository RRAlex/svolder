// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value as Json, json};
use svolder_ipc::config::{WebConfig, WebGuest};
use svolder_ipc::{Config, DeviceInfo, Request, Response};
use svolder_profiles::tone::preset::Preset;
use svolder_profiles::{Profile, roles};
use svolder_proto::{Message, Value};

use super::page::PageModel;
use super::*;
use crate::device::Feed;
use crate::fleet::{Fleet, Unavailable};
use crate::recall::Recalled;
use crate::test_support::profile_named;
use crate::web::Viewer;
use crate::web::api::{Loaded, embed};
use crate::web::test_support::{Code, call, fake_daemon, fake_daemon_playing, get, open, web};
use crate::web::test_support::{web_over, web_with};
use crate::web::view::test_support::{OPEN, info, read, shot, tone_on};
use crate::web::view::{self, Access, screen};
use crate::writer::Glided;

/// A guest: reads, writes nothing.
const GUEST: Access = Access {
    writes: false,
    guest: true,
};

/// A device's model over the readings of `profile`, for a viewer with `access`.
fn model_of(profile: &Profile, readings: &[Message], access: Access) -> DeviceModel {
    let row = info("Gungnir 2");
    let s = view::Snapshot {
        access,
        ..shot(&row, profile, readings)
    };
    device_model(&screen(&s))
}

/// `value` as JSON.
fn json_of<T: serde::Serialize>(value: &T) -> Json {
    serde_json::to_value(value).expect("the model serialises")
}

/// The page model for `device` read as `viewer` over a server of `config`, with no daemon: the
/// device is a row, a profile and its readings.
async fn page_of(
    config: WebConfig,
    viewer: Viewer,
    profile: &Profile,
    readings: Vec<Message>,
) -> PageModel {
    let web = web(config, Arc::new(Unavailable));
    let row = info("Gungnir 2");
    let loaded = Loaded {
        info: row.clone(),
        feed: Feed {
            profile: Some(Arc::new(profile.clone())),
            readings,
            connected: true,
            ..Feed::default()
        },
        climb: None,
        presets: Vec::new(),
        writable: Ok(()),
        host: Default::default(),
    };
    page_model(
        &web,
        viewer,
        std::slice::from_ref(&row),
        Some(&row),
        Some(&loaded),
        None,
    )
    .await
}

/// A fleet that lists `devices` and reads `config`, and reaches no device.
pub(super) struct Listed {
    pub(super) devices: Vec<DeviceInfo>,
    pub(super) config: Config,
}

#[async_trait]
impl Fleet for Listed {
    fn resolve(&self, selector: Option<&str>) -> Result<DeviceInfo, String> {
        let found = match selector {
            Some(name) => self.devices.iter().find(|d| d.name == name),
            None => self.devices.first(),
        };
        found.cloned().ok_or_else(|| "no such device".to_string())
    }

    async fn request(&self, request: Request) -> Response {
        match request {
            Request::Devices => Response::Devices {
                devices: self.devices.clone(),
            },
            _ => Response::Error {
                code: 3,
                message: "no daemon".to_string(),
            },
        }
    }

    async fn recall(
        &self,
        _device: &DeviceInfo,
        _name: &str,
        _fitting: Vec<Preset>,
    ) -> Result<(Recalled, Response), String> {
        Err("no daemon".to_string())
    }

    async fn glide(&self, _device: &DeviceInfo, _key: &str, _raw: i64) -> Glided {
        Glided::Refused(Response::Error {
            code: 3,
            message: "no daemon".to_string(),
        })
    }

    fn profile(&self, device: &DeviceInfo) -> Option<Profile> {
        Some(profile_named(&device.profile))
    }

    fn config(&self) -> Result<Config, String> {
        Ok(self.config.clone())
    }
}

#[test]
fn the_dac_model_lists_every_control_by_kind() {
    let p = profile_named("gungnir2-v2");
    let mut readings = tone_on(&p, "Warm");
    readings.extend([read(&p, "input", "3"), read(&p, "phase", "true")]);
    let model = json_of(&model_of(&p, &readings, OPEN));
    assert_eq!(model["layout"], "dac");
    assert_eq!(model["volume"]["key"], "volume");
    let controls = model["controls"].as_array().expect("controls");
    let kind_of = |key: &str| {
        controls
            .iter()
            .find(|c| c["key"] == key)
            .map(|c| c["kind"].clone())
    };
    assert_eq!(kind_of("input"), Some(json!("choice")));
    assert_eq!(kind_of("phase"), Some(json!("toggle")));
    assert_eq!(kind_of("balance"), Some(json!("slider")));
    assert_eq!(kind_of("usb1_connected"), Some(json!("readout")));
    let eq = p.eq.as_ref().expect("a tone control");
    assert_eq!(model["tone"]["response"].as_array().unwrap().len(), 181);
    assert_eq!(
        model["tone_steps"]["bands"].as_array().unwrap().len(),
        eq.bands.len()
    );
    assert_eq!(model["tone"]["mode"], "eq");
}

#[tokio::test]
async fn a_guests_model_is_read_only_and_names_no_unit() {
    let p = profile_named("gungnir2-v2");
    let guests = WebConfig {
        enabled: true,
        guest: WebGuest::Read,
        ..WebConfig::default()
    };
    let page = json_of(&page_of(guests, Viewer::Guest, &p, vec![read(&p, "input", "3")]).await);
    assert_eq!(page["viewer"]["guest"], true);
    assert_eq!(page["viewer"]["writes"], false);
    assert_eq!(page["viewer"]["version"], Json::Null);
    assert_eq!(page["device"]["inert"], true);
    assert!(page["device"]["facts"].get("unit").is_none());
    assert_eq!(page["device"]["presets"]["recall"], false);
    // A lost link offers no Try now to a guest.
    let lost = DeviceInfo {
        connected: false,
        retry_in_secs: Some(5),
        ..info("Gungnir 2")
    };
    let readings = [read(&p, "input", "3")];
    let s = view::Snapshot {
        access: GUEST,
        ..shot(&lost, &p, &readings)
    };
    let card = json_of(&device_model(&screen(&s)).card);
    assert_eq!(card["kind"], "warn");
    assert_eq!(card["try_now"], false);
    assert_eq!(card["retry_in"], 5);
    // The same for a signed-in viewer does.
    let s = view::Snapshot {
        access: OPEN,
        ..shot(&lost, &p, &readings)
    };
    assert_eq!(json_of(&device_model(&screen(&s)).card)["try_now"], true);
}

/// A device echoes a passphrase change with a status on function 250 that carries the new code
/// in its encoded form. The model never lists that reading, for the owner or for a guest.
#[tokio::test]
async fn the_passphrase_never_reaches_the_model() {
    let p = profile_named("gungnir2-v2");
    let encoded: u32 = 0x00c0_ffee;
    let readings = vec![
        read(&p, "input", "3"),
        Message::status(svolder_proto::function::PASSPHRASE, Value::U32(encoded)),
    ];
    let guests = WebConfig {
        enabled: true,
        guest: WebGuest::Read,
        ..WebConfig::default()
    };
    for (config, viewer) in [(open(), Viewer::Signed), (guests, Viewer::Guest)] {
        let page = page_of(config, viewer, &p, readings.clone()).await;
        let text = embed(&page);
        let id = svolder_proto::function::PASSPHRASE.to_string();
        assert!(!text.contains(&encoded.to_string()), "{text}");
        assert!(!text.contains(&format!("\"id\":\"{id}\"")), "{text}");
        let rows = json_of(&page)["device"]["facts"]["rows"].clone();
        assert!(
            !rows.as_array().unwrap().is_empty(),
            "the readings are listed"
        );
        assert!(
            rows.as_array()
                .unwrap()
                .iter()
                .all(|r| r["id"] != id.as_str()),
            "{rows}"
        );
    }
}

#[tokio::test]
async fn a_read_only_page_says_why() {
    let p = profile_named("gungnir2-v2");
    let off = WebConfig {
        writes: false,
        ..open()
    };
    let page = json_of(&page_of(off, Viewer::Signed, &p, vec![]).await);
    assert_eq!(page["viewer"]["signed_in"], true);
    assert_eq!(
        page["viewer"]["read_only"],
        "Read-only: [web] writes is off"
    );
    assert_eq!(page["device"]["inert"], true);
    let on = json_of(&page_of(open(), Viewer::Signed, &p, vec![]).await);
    assert_eq!(on["viewer"]["read_only"], Json::Null);
    assert_eq!(on["device"]["inert"], false);
}

#[test]
fn the_amp_model_has_meters_and_power() {
    let p = profile_named("aegir2");
    let model = json_of(&model_of(&p, &[], OPEN));
    assert_eq!(model["layout"], "amp");
    assert_eq!(model["meters"]["meters"].as_array().unwrap().len(), 2);
    assert!(model["power"]["key"].is_string(), "{}", model["power"]);
    assert_eq!(model["volume"], Json::Null, "the Aegir 2 has no knob");
    assert_eq!(model["meters"]["meters"][0]["channel"], "L");
    assert_eq!(model["meters"]["live"], true);
    assert!(model["power"]["toggles"].is_array());
    let tyr = json_of(&model_of(&profile_named("tyr"), &[], OPEN));
    assert_eq!(tyr["volume"], Json::Null);
}

#[test]
fn the_loki_max_model_has_its_slots_and_settings() {
    let p = profile_named("lokimax");
    let model = json_of(&model_of(&p, &[], OPEN));
    let presets = &model["device_presets"];
    let slots: Vec<&str> = presets["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(slots, ["Preset 1", "Preset 2", "Preset 3"]);
    assert_eq!(presets["slots"][1]["slot"], 2);
    assert_eq!(
        presets["slots"][1]["save_json"],
        r#"{"confirm":true,"slot":2}"#
    );
    assert_eq!(presets["builtins"].as_array().unwrap().len(), 5);
    assert_eq!(presets["recall"], true);
}

#[test]
fn the_volume_carries_its_marks_risk_and_cap() {
    let p = profile_named("gungnir2-v2");
    let row = info("Gungnir 2");
    let readings = tone_on(&p, "Warm");
    let s = view::Snapshot {
        cap_db: Some(-40.0),
        ..shot(&row, &p, &readings)
    };
    let volume = json_of(&device_model(&screen(&s)).volume);
    let control = roles::volume(&p).expect("a volume");
    let cap = roles::capped_step(control, -40.0).expect("a cap");
    assert_eq!(volume["cap"], cap);
    assert_eq!(volume["label"], "Volume");
    let marks = volume["marks"].as_array().expect("marks");
    assert_eq!(marks[0]["text"], "−∞");
    let (lo, hi) = (
        volume["lo"].as_i64().unwrap(),
        volume["hi"].as_i64().unwrap(),
    );
    assert_eq!(marks[0]["at"], lo as f64);
    assert_eq!(marks[1]["at"], hi as f64);
    let sixty = marks
        .iter()
        .find(|m| m["text"] == "−60")
        .expect("a −60 mark");
    let at = sixty["at"].as_f64().unwrap();
    assert!(
        at > lo as f64 && at < hi as f64 && at.fract() != 0.0,
        "{at}"
    );
    assert_eq!(
        volume["texts"].as_array().unwrap().len() as i64,
        hi - lo + 1
    );
    assert_eq!(volume["db"].as_array().unwrap().len() as i64, hi - lo + 1);
    assert_eq!(volume["db"][0], Json::Null, "silence has no dB");
    assert!(volume["risk"].is_i64(), "a warm EQ peaks above 0 dB");
}

#[test]
fn the_model_names_every_part_once() {
    let p = profile_named("gungnir2-v2");
    let model = model_of(&p, &tone_on(&p, "Warm"), OPEN);
    let names: Vec<&str> = Part::ALL.iter().map(|part| part.name()).collect();
    assert_eq!(
        names,
        [
            "link",
            "card",
            "volume",
            "controls",
            "meters",
            "power",
            "tone",
            "presets",
            "device_presets",
            "facts",
            "playing"
        ]
    );
    let whole = json_of(&model);
    let keys: Vec<&String> = whole.as_object().unwrap().keys().collect();
    for name in &names {
        assert_eq!(
            keys.iter().filter(|k| k.as_str() == *name).count(),
            1,
            "{name}"
        );
    }
    for part in Part::ALL {
        assert_eq!(
            part_value(&model, part),
            whole[part.name()],
            "{}",
            part.name()
        );
    }
}

#[test]
fn the_embedded_model_cannot_close_its_script_element() {
    let p = profile_named("gungnir2-v2");
    let mut value = json_of(&model_of(&p, &[], OPEN));
    value["facts"]["profile"] = json!("</script><b>");
    let text = embed(&value);
    assert!(!text.contains("</") && !text.contains('<'), "{text}");
    let back: Json = serde_json::from_str(&text).unwrap();
    assert_eq!(back["facts"]["profile"], "</script><b>");
}

#[tokio::test]
async fn the_page_carries_its_model_for_the_first_paint() {
    let device = fake_daemon("gungnir2-v2", "Gungnir 2").await;
    let web = web_over(&device);
    let page = call(&web, get("/")).await;
    assert_eq!(page.status, Code::OK);
    let open = "<script type=\"application/json\" id=\"svolder-page\">";
    let start = page.body.find(open).expect("the data block") + open.len();
    let end = start + page.body[start..].find("</script>").expect("its end");
    let model: Json = serde_json::from_str(&page.body[start..end]).expect("JSON");
    assert_eq!(model["v"], 1);
    assert_eq!(model["device"]["url"], "Gungnir%202");
    assert_eq!(model["shown"], "Gungnir%202");
    assert_eq!(model["fleet"]["devices"][0]["name"], "Gungnir 2");
    assert_eq!(model["followed"], json!({}));
    assert_eq!(model["strip"]["kind"], "dac");
    device.stop().await;
}

#[tokio::test]
async fn the_model_route_answers_a_reader_and_refuses_a_stranger() {
    let device = fake_daemon("gungnir2-v2", "dac").await;
    let closed = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::clone(&device.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    for path in ["/api/v1/devices/dac/model", "/api/v1/fleet", "/api/v1/host"] {
        let refused = call(&closed, get(path)).await;
        assert_eq!((path, refused.status), (path, Code::UNAUTHORIZED));
        assert_eq!(refused.body, "");
    }
    let web = web_over(&device);
    let unknown = call(&web, get("/api/v1/devices/nope/model")).await;
    assert_eq!(unknown.status, Code::NOT_FOUND);
    let plain = call(&web, get("/api/v1/devices/dac/model")).await;
    assert_eq!(plain.status, Code::OK, "{}", plain.body);
    let answer: Json = serde_json::from_str(&plain.body).unwrap();
    assert_eq!(answer["device"]["layout"], "dac");
    assert_eq!(answer["strip"], Json::Null);
    let with = call(&web, get("/api/v1/devices/dac/model?strip=1")).await;
    let answer: Json = serde_json::from_str(&with.body).unwrap();
    assert!(answer["strip"]["bars"].is_array(), "{}", with.body);
    device.stop().await;
}

#[tokio::test]
async fn the_fleet_lists_every_device_with_its_link_and_the_hops() {
    let config = Config::parse(
        "schema = 1\n\
         [[device]]\nname = \"Gungnir 2\"\nudid = \"0102030405060708\"\n\
         [[device]]\nname = \"Aegir 2\"\nudid = \"1112131415161718\"\n\
         [[hop]]\nname = \"HD 650\"\nkind = \"headphones\"\nnote = \"on the desk\"\n\
         [[hop]]\ndevice = \"Aegir 2\"\nafter = \"Gungnir 2\"\n\
         [[hop]]\ndevice = \"Ghost\"\n",
    )
    .expect("a configuration");
    let dac = DeviceInfo {
        profile: "gungnir2-v2".into(),
        ..info("Gungnir 2")
    };
    let amp = DeviceInfo {
        profile: "aegir2".into(),
        connected: false,
        retry_in_secs: Some(4),
        ..info("Aegir 2")
    };
    let fleet = Listed {
        devices: vec![dac, amp],
        config,
    };
    let web = web(open(), Arc::new(fleet));
    let reply = call(&web, get("/api/v1/fleet")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let fleet: Json = serde_json::from_str(&reply.body).unwrap();
    let devices = fleet["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0]["family"], "dac");
    assert_eq!(
        devices[0]["link"],
        json!({"word": "connected", "state": "on"})
    );
    assert_eq!(devices[1]["family"], "amp");
    assert_eq!(devices[1]["url"], "Aegir%202");
    assert_eq!(devices[1]["link"]["state"], "off");
    let hops = fleet["hops"].as_array().unwrap();
    assert_eq!(hops.len(), 3);
    assert_eq!(hops[0]["id"], "HD 650");
    assert_eq!(hops[0]["kind"], "headphones");
    assert_eq!(hops[0]["note"], "on the desk");
    assert_eq!(hops[0]["after"], "Gungnir%202", "the default device");
    assert_eq!(hops[0]["device"], Json::Null);
    assert_eq!(hops[0]["live"], false);
    assert_eq!(hops[1]["kind"], "amp");
    assert_eq!(hops[1]["name"], "Aegir 2");
    assert_eq!(hops[1]["device"], "Aegir%202");
    assert_eq!(hops[1]["live"], true);
    assert_eq!(hops[2]["kind"], "other");
    assert_eq!(hops[2]["device"], Json::Null);
    assert_eq!(hops[2]["live"], false);
}

#[tokio::test]
async fn the_playing_model_carries_the_album() {
    let device = fake_daemon_playing("gungnir2-v2", "dac").await;
    let reply = call(&web_over(&device), get("/api/v1/host")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let host: Json = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(host["track"]["album"], "Demo Tape", "{host}");
    device.stop().await;
}

#[tokio::test]
async fn the_host_model_is_readable_without_a_device() {
    let device = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&device);
    let reply = call(&web, get("/api/v1/host")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let host: Json = serde_json::from_str(&reply.body).unwrap();
    assert!(host["track"]["headline"].is_string(), "{host}");
    assert_eq!(host["path"], Json::Null, "no device, no path");
    assert_eq!(host["sink_label"], true);
    device.stop().await;
    // A host that is not watched says null.
    let quiet = fake_daemon("gungnir2-v2", "dac").await;
    let reply = call(&web_over(&quiet), get("/api/v1/host")).await;
    assert_eq!(reply.body, "null");
    quiet.stop().await;
}

/// A route to the Bifrost's sink, with the computer EQ in the path or not.
fn bifrost_route(host_eq: bool) -> svolder_host::RouteFacts {
    svolder_host::RouteFacts {
        sink: Some(svolder_host::SinkFacts {
            name: "alsa_output.usb-Schiit".to_string(),
            description: "Bifrost 3".to_string(),
            rate_hz: Some(96000),
            format: Some("S32LE".to_string()),
            volume_milli: Some(540),
            muted: false,
            ..Default::default()
        }),
        player: Some(svolder_host::StreamFacts {
            rate_hz: Some(96000),
            ..Default::default()
        }),
        host_eq,
        others_mixing: 1,
        ..Default::default()
    }
}

/// A computer EQ state for `target`, on, from AutoEq when `origin` is given.
fn eq_state(target: &str, on: bool, origin: Option<&str>) -> svolder_host::eq::HostEqState {
    use svolder_host::eq::{HostEqState, Manager, Route};
    HostEqState {
        on,
        name: "HD 650".to_string(),
        origin: origin.map(str::to_string),
        target: target.to_string(),
        route: Route::Smart,
        manager: Manager::Systemd,
        previous_default: None,
        preamp_db: -3.0,
        filters: Vec::new(),
        slots: Vec::new(),
    }
}

#[test]
fn the_page_model_carries_the_routes_facts() {
    let route = json_of(&page::route_model(&bifrost_route(true), None));
    assert_eq!(route["sink"]["words"], "Bifrost 3");
    assert_eq!(route["sink"]["rate_hz"], 96000);
    assert_eq!(route["sink"]["format"], "S32LE");
    assert_eq!(route["sink"]["volume_pct"], 54);
    assert_eq!(route["sink"]["muted"], false);
    assert_eq!(route["host_eq"], true);
    assert_eq!(route["others_mixing"], 1);
    assert_eq!(route["player_rate_hz"], 96000);
}

#[tokio::test]
async fn a_route_not_read_is_null_in_the_model() {
    let device = fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web_over(&device);
    let host = json_of(&page::host_model(&web, GUEST, None).expect("a host"));
    // The test daemon reads no route (and the card keeps its fields beside it).
    assert_eq!(host["route"], Json::Null, "{host}");
    assert!(host["track"].is_object(), "{host}");
    device.stop().await;
}

#[test]
fn a_sink_without_a_description_is_named_by_its_node_name() {
    let mut route = bifrost_route(false);
    route.sink.as_mut().unwrap().description.clear();
    route.sink.as_mut().unwrap().volume_milli = None;
    let route = json_of(&page::route_model(&route, None));
    assert_eq!(route["sink"]["words"], "alsa_output.usb-Schiit");
    assert_eq!(route["sink"]["volume_pct"], Json::Null);
    let mut none = bifrost_route(false);
    none.sink = None;
    assert_eq!(json_of(&page::route_model(&none, None))["sink"], Json::Null);
}

#[test]
fn the_applied_presets_name_shows_when_the_route_has_the_host_eq() {
    let eq = eq_state("alsa_output.usb-Schiit", true, Some("oratory1990"));
    let route = json_of(&page::route_model(&bifrost_route(true), Some(&eq)));
    assert_eq!(route["eq_preset"], "HD 650");
    assert_eq!(route["eq_autoeq"], true);
}

#[test]
fn a_host_eq_for_another_sink_gives_no_name() {
    let other = eq_state("alsa_output.pci-other", true, Some("oratory1990"));
    let route = json_of(&page::route_model(&bifrost_route(true), Some(&other)));
    assert_eq!(route["eq_preset"], Json::Null);
    assert_eq!(route["eq_autoeq"], false);
    // Off, or not in the route's path, gives none either.
    let off = eq_state("alsa_output.usb-Schiit", false, Some("x"));
    let route = json_of(&page::route_model(&bifrost_route(true), Some(&off)));
    assert_eq!(route["eq_preset"], Json::Null);
    let on = eq_state("alsa_output.usb-Schiit", true, Some("x"));
    let route = json_of(&page::route_model(&bifrost_route(false), Some(&on)));
    assert_eq!(route["eq_preset"], Json::Null);
}

#[test]
fn a_host_eq_put_in_by_hand_is_on_without_a_name() {
    let route = json_of(&page::route_model(&bifrost_route(true), None));
    assert_eq!(route["host_eq"], true);
    assert_eq!(route["eq_preset"], Json::Null);
    assert_eq!(route["eq_autoeq"], false);
}

#[test]
fn a_band_preset_names_the_eq_but_is_not_autoeq() {
    let eq = eq_state("alsa_output.usb-Schiit", true, None);
    let route = json_of(&page::route_model(&bifrost_route(true), Some(&eq)));
    assert_eq!(route["eq_preset"], "HD 650");
    assert_eq!(route["eq_autoeq"], false);
}

/// A scratch directory that goes when the test ends, however it ends.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn an_unreadable_host_eq_file_gives_no_name() {
    let dir = std::env::temp_dir().join(format!("svolderd-hosteq-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _scratch = Scratch(dir.clone());
    let broken = dir.join("host-eq.toml");
    std::fs::write(&broken, "on = \"not a state").unwrap();
    assert!(page::host_eq_state(&broken).is_none());
    assert!(page::host_eq_state(&dir.join("absent.toml")).is_none());
    // A directory where the file should be is unreadable too.
    assert!(page::host_eq_state(&dir).is_none());
    let eq = eq_state("alsa_output.usb-Schiit", true, Some("x"));
    eq.save(&broken).unwrap();
    assert_eq!(page::host_eq_state(&broken), Some(eq));
}
