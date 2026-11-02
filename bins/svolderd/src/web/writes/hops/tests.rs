// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use crate::web::test_support::{Code, FakeDaemon, call, fake_daemon, open, post, web};

/// The fake's configuration file as it stands.
fn config(dac: &FakeDaemon) -> String {
    std::fs::read_to_string(dac.dir.join("config.toml")).unwrap()
}

/// The ids of the `[[hop]]` blocks the file holds, in order.
fn file_hops(dac: &FakeDaemon) -> Vec<String> {
    let config = svolder_ipc::Config::load_from(&dac.dir.join("config.toml")).unwrap();
    config.hops.iter().map(|hop| hop.id().to_string()).collect()
}

/// The ids in an answer's `hops`, in order.
fn answered(body: &str) -> Vec<String> {
    let said: serde_json::Value = serde_json::from_str(body).unwrap();
    said["hops"]
        .as_array()
        .unwrap_or_else(|| panic!("no hops in {said}"))
        .iter()
        .map(|hop| hop["id"].as_str().unwrap().to_string())
        .collect()
}

/// Hops added, moved and removed from the page: each answer lists the hops in the file's order,
/// and the file holds the same.
#[tokio::test]
async fn hops_are_added_listed_moved_and_removed_from_the_page() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        post(
            "/api/v1/hops",
            r#"{"name":"  Amp one ","kind":"amp","note":"warm"}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(answered(&reply.body), ["Amp one"]);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["hops"][0]["kind"], "amp");
    assert_eq!(said["hops"][0]["note"], "warm");
    let reply = call(
        &web,
        post("/api/v1/hops", r#"{"name":"Cans","kind":"headphones"}"#),
    )
    .await;
    assert_eq!(answered(&reply.body), ["Amp one", "Cans"]);
    let reply = call(&web, post("/api/v1/hops/move", r#"{"id":"cans","to":0}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(answered(&reply.body), ["Cans", "Amp one"]);
    assert_eq!(file_hops(&dac), ["Cans", "Amp one"]);
    let reply = call(&web, post("/api/v1/hops/remove", r#"{"id":"Cans"}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(answered(&reply.body), ["Amp one"]);
    assert_eq!(file_hops(&dac), ["Amp one"]);
    assert!(!config(&dac).contains("Cans"), "{}", config(&dac));
    dac.stop().await;
}

/// A device the configuration holds is linked as a hop, after the device it names.
#[tokio::test]
async fn a_configured_device_is_linked_as_a_hop() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        post(
            "/api/v1/hops",
            r#"{"device":"DAC","after":"dac","note":"n"}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["hops"][0]["id"], "dac", "{said}");
    assert_eq!(said["hops"][0]["device"], "dac", "{said}");
    assert_eq!(said["hops"][0]["live"], true, "{said}");
    for (body, words) in [
        (r#"{"device":"nowhere"}"#, "no configured device"),
        (
            r#"{"device":"dac","name":"x"}"#,
            "a name and a kind, or a device",
        ),
        (r#"{"name":"x"}"#, "a name and a kind, or a device"),
        (
            r#"{"name":"x","kind":"toaster"}"#,
            r#"kind \"toaster\" is not one of"#,
        ),
        (r#"{"name":"  ","kind":"amp"}"#, "a hop needs a name"),
        (r#"{"name":"DAC","kind":"amp"}"#, "already configured"),
    ] {
        let reply = call(&web, post("/api/v1/hops", body)).await;
        assert_eq!(
            reply.status,
            Code::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            reply.body
        );
        assert!(reply.body.contains(words), "{body}: {}", reply.body);
    }
    assert_eq!(file_hops(&dac), ["dac"]);
    dac.stop().await;
}

/// An edit changes what it names: an absent key keeps, `note: null` clears; a linked hop cannot
/// take a name or kind of its own.
#[tokio::test]
async fn a_hop_edit_merges_onto_the_hop_it_names() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let added = r#"{"name":"Amp","kind":"amp","note":"warm","after":"dac"}"#;
    assert_eq!(
        call(&web, post("/api/v1/hops", added)).await.status,
        Code::OK
    );
    let reply = call(
        &web,
        post("/api/v1/hops/edit", r#"{"id":"amp","kind":"speakers"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["hops"][0]["kind"], "speakers", "{said}");
    assert_eq!(said["hops"][0]["note"], "warm", "{said}");
    assert_eq!(said["hops"][0]["after"], "dac", "{said}");
    let reply = call(
        &web,
        post(
            "/api/v1/hops/edit",
            r#"{"id":"Amp","name":"Mains","note":null}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["hops"][0]["id"], "Mains", "{said}");
    assert!(said["hops"][0]["note"].is_null(), "{said}");
    assert_eq!(said["hops"][0]["kind"], "speakers", "{said}");
    let linked = call(&web, post("/api/v1/hops", r#"{"device":"dac"}"#)).await;
    assert_eq!(linked.status, Code::OK, "{}", linked.body);
    for body in [r#"{"id":"dac","name":"x"}"#, r#"{"id":"dac","kind":"amp"}"#] {
        let reply = call(&web, post("/api/v1/hops/edit", body)).await;
        assert_eq!(
            reply.status,
            Code::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            reply.body
        );
        assert!(reply.body.contains("links a device"), "{}", reply.body);
    }
    let reply = call(
        &web,
        post("/api/v1/hops/edit", r#"{"id":"dac","note":"kept"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let taken = call(
        &web,
        post("/api/v1/hops/edit", r#"{"id":"Mains","name":"dac"}"#),
    )
    .await;
    assert_eq!(taken.status, Code::UNPROCESSABLE_ENTITY, "{}", taken.body);
    assert!(taken.body.contains("already configured"), "{}", taken.body);
    dac.stop().await;
}

/// A hop that is not there is a 404 with its name, whichever write asks.
#[tokio::test]
async fn a_hop_nobody_declared_is_not_found() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let before = config(&dac);
    for (path, body) in [
        ("edit", r#"{"id":"Ghost","note":"x"}"#),
        ("remove", r#"{"id":"Ghost"}"#),
        ("move", r#"{"id":"Ghost","to":0}"#),
    ] {
        let reply = call(&web, post(&format!("/api/v1/hops/{path}"), body)).await;
        assert_eq!(reply.status, Code::NOT_FOUND, "{path}: {}", reply.body);
        assert!(
            reply.body.contains(r#"no hop named \"Ghost\""#),
            "{}",
            reply.body
        );
    }
    assert_eq!(config(&dac), before);
    dac.stop().await;
}

/// Each hop write turns the fleet's change counter, so every open page hears of it.
#[tokio::test]
async fn a_hop_write_turns_the_fleet_changed_counter() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let mut feed = web.fleet_changed.subscribe();
    feed.borrow_and_update();
    for (path, body) in [
        ("", r#"{"name":"Amp","kind":"amp"}"#),
        ("/edit", r#"{"id":"Amp","note":"x"}"#),
        ("/move", r#"{"id":"Amp","to":0}"#),
        ("/remove", r#"{"id":"Amp"}"#),
    ] {
        let reply = call(&web, post(&format!("/api/v1/hops{path}"), body)).await;
        assert_eq!(reply.status, Code::OK, "{path}: {}", reply.body);
        assert!(feed.has_changed().unwrap(), "{path} turned nothing");
        feed.borrow_and_update();
    }
    // A refused write turns nothing.
    let reply = call(&web, post("/api/v1/hops/remove", r#"{"id":"Amp"}"#)).await;
    assert_eq!(reply.status, Code::NOT_FOUND);
    assert!(!feed.has_changed().unwrap());
    dac.stop().await;
}

/// A configuration that cannot be written refuses a hop write with why, and is not touched.
#[tokio::test]
async fn a_hop_write_on_a_read_only_config_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let path = dac.dir.join("config.toml");
    let before = config(&dac);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let web = web(open(), Arc::clone(&dac.fleet));
    for (route, body) in [
        ("", r#"{"name":"Amp","kind":"amp"}"#),
        ("/edit", r#"{"id":"Amp","note":"x"}"#),
        ("/remove", r#"{"id":"Amp"}"#),
        ("/move", r#"{"id":"Amp","to":0}"#),
    ] {
        let reply = call(&web, post(&format!("/api/v1/hops{route}"), body)).await;
        assert_eq!(reply.status, Code::CONFLICT, "{route}: {}", reply.body);
        assert!(
            reply.body.contains("Saving hops is off: "),
            "{route}: {}",
            reply.body
        );
        assert!(reply.body.contains("read-only (0444)"), "{}", reply.body);
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(config(&dac), before);
    dac.stop().await;
}

/// A page with writes off refuses every hop write, and the file is left as it was.
#[tokio::test]
async fn a_read_only_page_cannot_write_hops() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let read_only = web(
        svolder_ipc::config::WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    let before = config(&dac);
    for route in ["", "/edit", "/remove", "/move"] {
        let reply = call(&read_only, post(&format!("/api/v1/hops{route}"), "{}")).await;
        assert_eq!(reply.status, Code::FORBIDDEN, "{route}: {}", reply.body);
    }
    assert_eq!(config(&dac), before);
    dac.stop().await;
}

/// A move past the end puts the hop last.
#[tokio::test]
async fn a_move_past_the_end_puts_the_hop_last() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    for name in ["One", "Two", "Three"] {
        let body = format!(r#"{{"name":"{name}","kind":"other"}}"#);
        assert_eq!(
            call(&web, post("/api/v1/hops", &body)).await.status,
            Code::OK
        );
    }
    let reply = call(&web, post("/api/v1/hops/move", r#"{"id":"One","to":99}"#)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(answered(&reply.body), ["Two", "Three", "One"]);
    assert_eq!(file_hops(&dac), ["Two", "Three", "One"]);
    dac.stop().await;
}

/// An edit sets `after`; `null` or a missing key keeps it and `""` clears it.
#[tokio::test]
async fn a_hop_edit_sets_keeps_and_clears_after() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let added = call(&web, post("/api/v1/hops", r#"{"name":"Amp","kind":"amp"}"#)).await;
    assert_eq!(added.status, Code::OK, "{}", added.body);
    let after = |reply: &str| -> Option<String> {
        let said: serde_json::Value = serde_json::from_str(reply).unwrap();
        said["hops"][0]["after"].as_str().map(str::to_string)
    };
    let hop_after = |dac: &FakeDaemon| {
        let config = svolder_ipc::Config::load_from(&dac.dir.join("config.toml")).unwrap();
        config.hops[0].after.clone()
    };
    assert_eq!(hop_after(&dac), None);
    let reply = call(
        &web,
        post("/api/v1/hops/edit", r#"{"id":"Amp","after":"dac"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(after(&reply.body).as_deref(), Some("dac"));
    assert_eq!(hop_after(&dac).as_deref(), Some("dac"));
    for body in [r#"{"id":"Amp","after":null}"#, r#"{"id":"Amp","note":"n"}"#] {
        let reply = call(&web, post("/api/v1/hops/edit", body)).await;
        assert_eq!(reply.status, Code::OK, "{body}: {}", reply.body);
        assert_eq!(hop_after(&dac).as_deref(), Some("dac"), "{body}");
    }
    let reply = call(
        &web,
        post("/api/v1/hops/edit", r#"{"id":"Amp","after":""}"#),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(hop_after(&dac), None);
    assert!(!config(&dac).contains("after"), "{}", config(&dac));
    dac.stop().await;
}
