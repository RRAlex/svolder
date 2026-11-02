// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use svolder_ipc::{Request, Response};

use crate::web::test_support::{Code, FakeDaemon, call, fake_daemon, open, post, web};
use crate::writer::Origin;

/// The fake's configuration file as it stands.
fn config(dac: &FakeDaemon) -> String {
    std::fs::read_to_string(dac.dir.join("config.toml")).unwrap()
}

/// The names of the devices the daemon serves now.
async fn served(dac: &FakeDaemon) -> Vec<String> {
    let Response::Devices { devices } = dac
        .daemon
        .handle_from(Origin::Socket, Request::Devices)
        .await
    else {
        panic!("no devices");
    };
    devices.into_iter().map(|d| d.name).collect()
}

/// Save writes the bands as a new `[[preset]]` block, answers with the presets part holding it,
/// and has the daemon reload the file: a device added to it meanwhile is served afterwards.
#[tokio::test]
async fn a_save_writes_the_bands_as_a_new_preset_and_reloads() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let mut text = config(&dac);
    text.push_str(
        "\n[[device]]\nname = \"other\"\nudid = \"0807060504030201\"\npassphrase = \"1234\"\n",
    );
    std::fs::write(dac.dir.join("config.toml"), text).unwrap();
    assert_eq!(served(&dac).await, ["dac"]);
    let web = web(open(), Arc::clone(&dac.fleet));
    let save = post("/api/v1/devices/dac/presets", r#"{"name":"  Mine "}"#);
    let reply = call(&web, save).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(said["name"], "Mine", "{said}");
    let rows = said["parts"]["presets"]["rows"]
        .as_array()
        .expect("the presets part");
    assert!(rows.iter().any(|row| row["name"] == "Mine"), "{said}");
    assert!(config(&dac).contains("name = \"Mine\""), "{}", config(&dac));
    assert!(
        served(&dac).await.contains(&"other".to_string()),
        "the daemon did not reload"
    );
    dac.stop().await;
}

/// A save, a rename and a delete answer the name and the presets part as it stands after them.
#[tokio::test]
async fn a_preset_write_answers_its_name_and_the_presets_part() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let mut names = Vec::new();
    for (path, body) in [
        ("", r#"{"name":"Mine"}"#),
        ("/rename", r#"{"from":"Mine","to":"Yours"}"#),
        ("/delete", r#"{"name":"Yours"}"#),
    ] {
        let reply = call(
            &web,
            post(&format!("/api/v1/devices/dac/presets{path}"), body),
        )
        .await;
        assert_eq!(reply.status, Code::OK, "{path}: {}", reply.body);
        let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
        let listed = said["parts"]["presets"].to_string();
        names.push((
            said["name"].clone(),
            listed.contains("Mine"),
            listed.contains("Yours"),
        ));
    }
    assert_eq!(
        names,
        [
            ("Mine".into(), true, false),
            ("Yours".into(), false, true),
            ("Yours".into(), false, false),
        ]
    );
    dac.stop().await;
}

/// A built-in's name, a name taken in another case, `custom` and a blank name are refused in the
/// core's words, and the file is left as it was.
#[tokio::test]
async fn a_taken_or_built_in_name_is_refused_with_the_core_words() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let mine = post("/api/v1/devices/dac/presets", r#"{"name":"Mine"}"#);
    assert_eq!(call(&web, mine).await.status, Code::OK);
    let before = config(&dac);
    for (name, words) in [
        ("Warm", "\\\"Warm\\\" is a built-in preset's name"),
        ("MINE", "a preset named \\\"Mine\\\" already exists"),
        ("custom", "no preset can take it"),
        ("   ", "a preset needs a name"),
    ] {
        let body = format!(r#"{{"name":"{name}"}}"#);
        let reply = call(&web, post("/api/v1/devices/dac/presets", &body)).await;
        assert_eq!(
            reply.status,
            Code::UNPROCESSABLE_ENTITY,
            "{name}: {}",
            reply.body
        );
        assert!(reply.body.contains(words), "{name}: {}", reply.body);
    }
    assert_eq!(config(&dac), before);
    dac.stop().await;
}

/// A rename to a name with markup is answered with the name as text: a string of the JSON answer,
/// which is never markup, wherever the part holds it.
#[tokio::test]
async fn a_rename_to_a_name_with_markup_is_answered_as_text() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let mine = post("/api/v1/devices/dac/presets", r#"{"name":"Mine"}"#);
    assert_eq!(call(&web, mine).await.status, Code::OK);
    let body = r#"{"from":"mine","to":"<img src=x onerror=alert(1)>"}"#;
    let reply = call(&web, post("/api/v1/devices/dac/presets/rename", body)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let rows = said["parts"]["presets"]["rows"]
        .as_array()
        .expect("the presets part");
    assert!(
        rows.iter()
            .any(|row| row["name"] == "<img src=x onerror=alert(1)>"),
        "{said}"
    );
    assert!(!config(&dac).contains("name = \"Mine\""));
    dac.stop().await;
}

/// Delete takes the user's preset out; a built-in and a name no block carries are refused.
#[tokio::test]
async fn a_delete_takes_the_preset_out_and_spares_the_built_ins() {
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let mine = post("/api/v1/devices/dac/presets", r#"{"name":"Mine"}"#);
    assert_eq!(call(&web, mine).await.status, Code::OK);
    let gone = post("/api/v1/devices/dac/presets/delete", r#"{"name":"Mine"}"#);
    let reply = call(&web, gone).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(!config(&dac).contains("Mine"));
    for (path, body, code) in [
        ("delete", r#"{"name":"Warm"}"#, Code::CONFLICT),
        ("delete", r#"{"name":"Mine"}"#, Code::NOT_FOUND),
        ("rename", r#"{"from":"Warm","to":"Hot"}"#, Code::CONFLICT),
        ("rename", r#"{"from":"Nope","to":"Hot"}"#, Code::NOT_FOUND),
    ] {
        let path = format!("/api/v1/devices/dac/presets/{path}");
        let reply = call(&web, post(&path, body)).await;
        assert_eq!(reply.status, code, "{path} {body}: {}", reply.body);
    }
    dac.stop().await;
}

/// A configuration that cannot be written refuses a save with why, and is not touched.
#[tokio::test]
async fn a_read_only_config_refuses_a_save_with_why() {
    use std::os::unix::fs::PermissionsExt;
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let path = dac.dir.join("config.toml");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let web = web(open(), Arc::clone(&dac.fleet));
    let reply = call(
        &web,
        post("/api/v1/devices/dac/presets", r#"{"name":"Mine"}"#),
    )
    .await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert!(reply.body.contains("read-only (0444)"), "{}", reply.body);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    dac.stop().await;
}
