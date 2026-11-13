// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use svolder_ipc::config::{WebConfig, WebGuest};

use crate::web::test_support::{
    Code, FakeDaemon, call, fake_daemon, get, open, post, web, web_with,
};

const AMP: &str = "1112131415161718";

/// The fake's configuration file as it stands.
fn config(dac: &FakeDaemon) -> svolder_ipc::Config {
    svolder_ipc::Config::load_from(&dac.dir.join("config.toml")).unwrap()
}

fn said(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{body}: {e}"))
}

/// The names in an answer's `devices`, in order.
fn answered(body: &str) -> Vec<String> {
    said(body)["devices"]
        .as_array()
        .unwrap_or_else(|| panic!("no devices in {body}"))
        .iter()
        .map(|device| device["name"].as_str().unwrap().to_string())
        .collect()
}

/// The page lists the configured devices for an editor, adds one, edits it, makes it the
/// default and removes it; each answer is the devices as the file now names them, and no answer
/// carries a passphrase.
#[tokio::test]
async fn devices_are_listed_added_edited_made_default_and_removed_from_the_page() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&web, get("/api/v1/devices")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let first = &said(&reply.body)["devices"][0];
    assert_eq!(first["name"], "dac");
    assert_eq!(first["unit"], "0102030405060708");
    assert_eq!(first["passphrase"], "set");
    assert_eq!(first["connected"], true);
    assert!(!reply.body.contains("1234"), "{}", reply.body);

    let body = format!(r#"{{"unit":"{AMP}","name":"amp","passphrase":"5678"}}"#);
    let reply = call(&web, post("/api/v1/devices", &body)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(answered(&reply.body), ["dac", "amp"]);
    assert!(!reply.body.contains("5678"), "{}", reply.body);
    assert_eq!(config(&dac).devices[1].passphrase, "5678");

    let reply = call(
        &web,
        post(
            "/api/v1/devices/amp/edit",
            r#"{"name":"desk amp","profile":"gungnir2-v2","passphrase":"none"}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(answered(&reply.body), ["dac", "desk amp"]);
    let edited = &said(&reply.body)["devices"][1];
    assert_eq!(edited["profile"], "gungnir2-v2");
    assert_eq!(edited["passphrase"], "none");

    // A name with a blank in it travels in the path, and `profile: null` takes the pin off.
    let reply = call(
        &web,
        post("/api/v1/devices/desk%20amp/edit", r#"{"profile":null}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(said(&reply.body)["devices"][1].get("profile").is_none());

    let reply = call(&web, post(&format!("/api/v1/devices/{AMP}/default"), "")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(said(&reply.body)["devices"][1]["default"], true);
    assert_eq!(config(&dac).default_device.as_deref(), Some("desk amp"));

    let reply = call(&web, post("/api/v1/devices/desk%20amp/remove", "")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(answered(&reply.body), ["dac"]);
    let file = config(&dac);
    assert_eq!(file.devices.len(), 1);
    assert_eq!(file.default_device, None);
    dac.stop().await;
}

/// The profiles in an answer, by name.
fn offered(body: &str) -> Vec<(String, serde_json::Value)> {
    said(body)["profiles"]
        .as_array()
        .unwrap_or_else(|| panic!("no profiles in {body}"))
        .iter()
        .map(|profile| {
            (
                profile["name"].as_str().unwrap().to_string(),
                profile.clone(),
            )
        })
        .collect()
}

/// The list names the profiles an editor offers, each with the product it describes where it
/// describes one, and a write's answer names them too. What the list names is what an edit
/// takes: every name in it pins, a name out of it is refused, and a profile file put in the
/// user's folder is in the next list and pins from then on.
#[tokio::test]
async fn the_devices_answer_names_the_profiles_an_edit_takes() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let body = format!(r#"{{"unit":"{AMP}","name":"amp"}}"#);
    let reply = call(&web, post("/api/v1/devices", &body)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let reply = call(&web, get("/api/v1/devices")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let profiles = offered(&reply.body);
    let names: Vec<&str> = profiles.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.is_sorted(), "{names:?}");
    let of = |name: &str| &profiles.iter().find(|(n, _)| n == name).unwrap().1;
    assert_eq!(of("bifrost3")["display"], "Bifrost 3");
    assert!(of("bifrost3").get("base").is_none());
    assert_eq!(of("dac-common")["base"], true);
    assert!(of("dac-common").get("display").is_none());
    assert!(!names.contains(&"desk-dac"), "{names:?}");

    let edit = |profile: &str| {
        post(
            "/api/v1/devices/amp/edit",
            &format!(r#"{{"profile":"{profile}"}}"#),
        )
    };
    let reply = call(&web, edit("desk-dac")).await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert_eq!(said(&reply.body)["reason"], "invalid");
    assert!(said(&reply.body).get("profiles").is_none());

    for name in ["bifrost3", "dac-common"] {
        let reply = call(&web, edit(name)).await;
        assert_eq!(reply.status, Code::OK, "{name}: {}", reply.body);
        assert_eq!(offered(&reply.body), profiles, "a write's answer");
    }

    let folder = dac.dir.join("profiles");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("desk-dac.toml"),
        "schema = 1\nname = \"desk-dac\"\nextends = \"bifrost3\"\ndisplay = \"Desk DAC\"\n",
    )
    .unwrap();
    let reply = call(&web, get("/api/v1/devices")).await;
    let later = offered(&reply.body);
    assert_eq!(later.len(), profiles.len() + 1);
    let added = &later.iter().find(|(n, _)| n == "desk-dac").unwrap().1;
    assert_eq!(added["display"], "Desk DAC");
    let reply = call(&web, edit("desk-dac")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(said(&reply.body)["devices"][1]["profile"], "desk-dac");
    dac.stop().await;
}

/// A refusal is the daemon's words with its reason beside them, under a status that says the
/// kind: 422 for what the request asked wrongly, 404 for a device nothing configures.
#[tokio::test]
async fn a_refused_device_write_carries_the_reason() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let cases = [
        (
            "/api/v1/devices",
            format!(r#"{{"unit":"{AMP}","name":"DAC"}}"#),
            Code::UNPROCESSABLE_ENTITY,
            "taken",
        ),
        (
            "/api/v1/devices",
            r#"{"unit":"0102030405060708","name":"twin"}"#.to_string(),
            Code::UNPROCESSABLE_ENTITY,
            "invalid",
        ),
        (
            "/api/v1/devices",
            format!(r#"{{"unit":"{AMP}","name":" "}}"#),
            Code::UNPROCESSABLE_ENTITY,
            "invalid",
        ),
        (
            "/api/v1/devices/nobody/edit",
            r#"{"name":"x"}"#.to_string(),
            Code::NOT_FOUND,
            "missing",
        ),
        (
            "/api/v1/devices/nobody/remove",
            String::new(),
            Code::NOT_FOUND,
            "missing",
        ),
        (
            "/api/v1/devices/nobody/default",
            String::new(),
            Code::NOT_FOUND,
            "missing",
        ),
    ];
    for (path, body, status, reason) in cases {
        let reply = call(&web, post(path, &body)).await;
        assert_eq!(reply.status, status, "{path}: {}", reply.body);
        let answer = said(&reply.body);
        assert_eq!(answer["reason"], reason, "{path}: {answer}");
        assert!(answer["error"].is_string(), "{path}: {answer}");
    }
    let reply = call(&web, post("/api/v1/devices", r#"{"name":"no unit"}"#)).await;
    assert_eq!(reply.status, Code::BAD_REQUEST, "{}", reply.body);
    assert_eq!(config(&dac).devices.len(), 1);
    dac.stop().await;
}

/// A body that is not what a route takes is a 400 that names the key at fault and never what
/// the body held: a passphrase typed as a number is not shown back.
#[tokio::test]
async fn a_body_that_does_not_parse_is_refused_without_its_content() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let cases = [
        (
            "/api/v1/devices",
            format!(r#"{{"unit":"{AMP}","name":"amp","passphrase":4321}}"#),
        ),
        (
            "/api/v1/devices",
            format!(r#"{{"unit":"{AMP}","name":4321}}"#),
        ),
        ("/api/v1/devices", "4321".to_string()),
        ("/api/v1/devices", r#"["4321"]"#.to_string()),
        ("/api/v1/devices", r#"{"unit":"4321" "#.to_string()),
        (
            "/api/v1/devices/dac/edit",
            r#"{"passphrase":4321}"#.to_string(),
        ),
        (
            "/api/v1/devices/dac/edit",
            r#"{"profile":{"name":"4321"}}"#.to_string(),
        ),
    ];
    for (path, body) in cases {
        let reply = call(&web, post(path, &body)).await;
        assert_eq!(reply.status, Code::BAD_REQUEST, "{body}: {}", reply.body);
        assert!(!reply.body.contains("4321"), "{body}: {}", reply.body);
        assert!(said(&reply.body)["error"].is_string(), "{}", reply.body);
    }
    let reply = call(
        &web,
        post("/api/v1/devices/dac/edit", r#"{"passphrase":4321}"#),
    )
    .await;
    assert!(reply.body.contains("passphrase"), "{}", reply.body);
    assert_eq!(config(&dac).devices[0].passphrase, "1234");
    dac.stop().await;
}

/// A file that cannot be written is a 409 with `read_only`; a daemon that is stopping a 503
/// with `stopping`.
#[tokio::test]
async fn a_read_only_file_and_a_stopping_daemon_refuse_with_their_reasons() {
    use std::os::unix::fs::PermissionsExt;
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let path = dac.dir.join("config.toml");
    let web = web(open(), Arc::clone(&dac.fleet));
    let body = format!(r#"{{"unit":"{AMP}","name":"amp"}}"#);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let reply = call(&web, post("/api/v1/devices", &body)).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert_eq!(said(&reply.body)["reason"], "read_only");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    dac.daemon.shutdown().await;
    let reply = call(&web, post("/api/v1/devices", &body)).await;
    assert_eq!(reply.status, Code::SERVICE_UNAVAILABLE, "{}", reply.body);
    assert_eq!(said(&reply.body)["reason"], "stopping");
    assert_eq!(config(&dac).devices.len(), 1);
}

/// Only a signed-in writer reads the configured devices or writes them: a guest is told to sign
/// in (401), a page with writes off that it is read-only (403).
#[tokio::test]
async fn the_device_routes_obey_the_write_gate() {
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
    let body = format!(r#"{{"unit":"{AMP}","name":"amp"}}"#);
    for (page, status) in [(&guests, Code::UNAUTHORIZED), (&read_only, Code::FORBIDDEN)] {
        let reply = call(page, get("/api/v1/devices")).await;
        assert_eq!(reply.status, status, "list: {}", reply.body);
        assert!(!reply.body.contains("dac"), "{}", reply.body);
        assert!(!reply.body.contains("profiles"), "{}", reply.body);
        for (path, body) in [
            ("/api/v1/devices", body.as_str()),
            ("/api/v1/devices/dac/edit", r#"{"name":"x"}"#),
            ("/api/v1/devices/dac/remove", ""),
            ("/api/v1/devices/dac/default", ""),
        ] {
            let reply = call(page, post(path, body)).await;
            assert_eq!(reply.status, status, "{path}: {}", reply.body);
        }
    }
    let file = config(&dac);
    assert_eq!(file.devices.len(), 1);
    assert_eq!(file.devices[0].name, "dac");
    assert_eq!(file.default_device, None);
    dac.stop().await;
}

/// A write tells every open page the fleet changed.
#[tokio::test]
async fn a_device_write_tells_the_open_pages() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let before = *web.fleet_changed.borrow();
    let reply = call(&web, post("/api/v1/devices/dac/default", "")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_ne!(*web.fleet_changed.borrow(), before);
    dac.stop().await;
}

/// A body longer than a device route could need is refused before it is read whole.
#[tokio::test]
async fn a_body_past_the_routes_limit_is_refused() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let long = format!(
        r#"{{"unit":"{AMP}","name":"amp","note":"{}"}}"#,
        "x".repeat(5000)
    );
    let reply = call(&web, post("/api/v1/devices", &long)).await;
    assert_eq!(reply.status, Code::PAYLOAD_TOO_LARGE, "{}", reply.body);
    assert_eq!(config(&dac).devices.len(), 1);
    dac.stop().await;
}

/// A passphrase changed on a connected device reaches its actor through the reload, on the link
/// it holds: the device stays connected, and the daemon says the new code is not the device's.
#[tokio::test]
async fn a_changed_passphrase_reaches_the_connected_actor() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let row = || async {
        match dac.fleet.request(svolder_ipc::Request::Devices).await {
            svolder_ipc::Response::Devices { devices } => devices[0].clone(),
            other => panic!("{other:?}"),
        }
    };
    let before = row().await;
    assert!(before.connected && !before.refused);
    assert_eq!(before.passphrase_note, None);
    let reply = call(
        &web,
        post("/api/v1/devices/dac/edit", r#"{"passphrase":"9999"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let mut after = row().await;
    for _ in 0..200 {
        if after.refused || after.passphrase_note.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        after = row().await;
    }
    assert!(
        after.refused || after.passphrase_note.is_some(),
        "the actor took the new code: {after:?}"
    );
    assert!(after.connected, "on the link it held: {after:?}");
    dac.stop().await;
}

/// Set the file's modification time to `secs` after a fixed instant: a hand edit as the daemon
/// sees one, whatever the clock's granularity.
fn touched(dac: &FakeDaemon, secs: u64) {
    std::fs::File::options()
        .write(true)
        .open(dac.dir.join("config.toml"))
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + secs))
        .unwrap();
}

/// A writer's list and every write's answer carry `config_changed`: the daemon's words while the
/// file's time differs from the snapshot's, else null. A hand edit shows in the list, and a
/// rename through the route, which the daemon reloads from, answers null.
#[tokio::test]
async fn the_devices_answer_says_when_the_config_changed_since_the_last_reload() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(&web, get("/api/v1/devices")).await;
    assert!(
        said(&reply.body)["config_changed"].is_null(),
        "{}",
        reply.body
    );

    touched(&dac, 5);
    let reply = call(&web, get("/api/v1/devices")).await;
    let words = format!(
        "config: {} changed since the last reload: svolder daemon reload (or SIGHUP)",
        dac.dir.join("config.toml").display()
    );
    assert_eq!(said(&reply.body)["config_changed"], words.as_str());

    let reply = call(&web, post("/api/v1/devices/dac/edit", r#"{"name":"desk"}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(
        said(&reply.body)["config_changed"].is_null(),
        "a write of the daemon's own is no change: {}",
        reply.body
    );
    dac.stop().await;
}

/// A deleted `config.toml` is told in the daemon's words for a missing file, through the same
/// field.
#[tokio::test]
async fn the_devices_answer_says_when_the_config_is_missing() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let path = dac.dir.join("config.toml");
    std::fs::remove_file(&path).unwrap();
    let reply = call(&web, get("/api/v1/devices")).await;
    let words =
        svolder_ipc::socket::daemon_version::config_missing_line(&path.display().to_string());
    assert_eq!(said(&reply.body)["config_changed"], words.as_str());
    dac.stop().await;
}
