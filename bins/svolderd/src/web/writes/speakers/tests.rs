// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The speakers routes over a real daemon that keeps its sets in a temporary directory and
//! fetches from a loopback server.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use futures::StreamExt;
use svolder_autoeq::test_server::{Reply, Server};
use svolder_ipc::config::{WebConfig, WebGuest};
use svolder_ipc::{Event, JobState, Request, SpeakersOp};
use svolder_speakers::{Client, Limits, Store, wav};

use super::*;
use crate::fleet::DaemonFleet;
use crate::handler::Daemon;
use crate::web::test_support::{Code, call, open, post, web, web_with};

/// A daemon with no device whose speakers live in `dir`, and the fleet that reaches it.
struct Rig {
    dir: PathBuf,
    daemon: Arc<Daemon>,
    fleet: Arc<dyn Fleet>,
}

impl Rig {
    fn new(base: Option<String>) -> Rig {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "svolderd-web-speakers-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.toml");
        std::fs::write(&config, "schema = 1\n").unwrap();
        let registry = Arc::new(svolder_profiles::Registry::builtin().unwrap());
        let built = registry.get("gungnir2-v2").expect("a built-in").clone();
        let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
            &built,
            "0102030405060708".parse().unwrap(),
            3,
            svolder_proto::Passphrase::from_text("1234").unwrap(),
        ));
        let daemon = Daemon::new(
            transport,
            registry,
            config.clone(),
            dir.join("profiles"),
            dir.join("s.sock"),
            svolder_ipc::Config::load_from(&config).unwrap_or_default(),
        )
        .with_speakers(
            Client::new(base, Limits::default()),
            Store::new(&dir.join("speakers")),
        )
        .shared();
        let fleet: Arc<dyn Fleet> = Arc::new(DaemonFleet::new(
            Arc::downgrade(&daemon),
            crate::writer::Origin::Web,
            crate::writer::Mode::Latest,
        ));
        Rig { dir, daemon, fleet }
    }

    fn stored(&self) -> Vec<String> {
        std::fs::read_dir(self.dir.join("speakers"))
            .map(|dir| {
                dir.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|name| !name.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The import slots are the process's, so the tests that post an import take turns: one of them
/// holds every slot.
static IMPORTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The process's own state home, which no import may write into: the daemon's import files go
/// under its store, and a test's store is in the test's own directory.
struct Outside(Vec<std::ffi::OsString>);

impl Outside {
    fn names() -> Vec<std::ffi::OsString> {
        let home = svolder_ipc::config::state_home()
            .join("svolder")
            .join("speakers");
        let mut names: Vec<_> = walk(&home);
        names.sort();
        names
    }

    fn look() -> Outside {
        Outside(Outside::names())
    }

    fn unchanged(self) {
        assert_eq!(
            self.0,
            Outside::names(),
            "an import wrote into the state home"
        );
    }
}

fn walk(dir: &std::path::Path) -> Vec<std::ffi::OsString> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            found.push(entry.path().into_os_string());
            found.extend(walk(&entry.path()));
        }
    }
    found
}

fn json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - chunk.len()));
        for at in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * at) & 63) as usize] as char);
        }
        out.extend(std::iter::repeat_n('=', 3 - chunk.len()));
    }
    out
}

fn dirac(at: usize, gain: f32) -> Vec<f32> {
    let mut v = vec![0.0; 400];
    v[at] = gain;
    v
}

fn set_wav() -> Vec<u8> {
    wav::write(
        48000,
        &[
            dirac(10, 0.5),
            dirac(14, 0.25),
            dirac(20, 0.125),
            dirac(30, 0.75),
        ],
    )
}

#[test]
fn base64_decodes_every_length_and_refuses_what_is_not_base64() {
    for len in 0..40usize {
        let bytes: Vec<u8> = (0..len).map(|n| (n * 37 + 11) as u8).collect();
        let text = base64(&bytes);
        assert_eq!(decode_base64(&text), Some(bytes.clone()), "{text}");
        assert_eq!(
            decode_base64(text.trim_end_matches('=')),
            Some(bytes),
            "{text}"
        );
    }
    for bad in ["a", "ab!d", "ab d", "-_-_", "QUJD\nQUJD"] {
        assert_eq!(decode_base64(bad), None, "{bad:?}");
    }
}

#[tokio::test]
async fn the_catalogue_route_lists_the_sets() {
    let rig = Rig::new(Some("http://127.0.0.1:9/".into()));
    let web = web(open(), Arc::clone(&rig.fleet));
    let reply = call(&web, post("/api/v1/speakers/catalogue", "")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said = json(&reply.body);
    assert_eq!(
        said["sets"].as_array().unwrap().len(),
        svolder_speakers::entries().len()
    );
    assert_eq!(said["fetching"], false);
    assert_eq!(said["sets"][0]["fetched"], false);
    // every row carries the choice's words and its technical term (svolder-speakers' own), so no
    // front end writes them
    for set in said["sets"].as_array().unwrap() {
        let (name, term) = match set["room"].as_str().unwrap() {
            "with" => ("With room", "BRIR"),
            _ => ("No room", "HRTF"),
        };
        assert_eq!(set["room_name"], name, "{set}");
        assert_eq!(set["term"], term, "{set}");
    }
}

#[tokio::test]
async fn import_forget_and_fetch_refusals_have_their_statuses() {
    let _turn = IMPORTS.lock().await;
    let outside = Outside::look();
    let server = Server::start(|_| Reply::Status(404));
    let rig = Rig::new(Some(server.url.clone()));
    let web = web(open(), Arc::clone(&rig.fleet));
    let body = serde_json::json!({
        "data": base64(&set_wav()), "label": "Mine", "room": "with"
    })
    .to_string();
    let reply = call(&web, post("/api/v1/speakers/import", &body)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said = json(&reply.body);
    assert_eq!(said["set"]["label"], "Mine");
    assert_eq!(said["set"]["room"], "with");
    assert_eq!(said["set"]["own"], true);
    // the saved set has the choice's words too, as a catalogue row does
    assert_eq!(said["set"]["room_name"], "With room", "{said}");
    assert_eq!(said["set"]["term"], "BRIR");
    let id = said["set"]["id"].as_str().unwrap().to_string();
    assert_eq!(rig.stored().len(), 2);
    let leftovers = std::fs::read_dir(rig.dir.join("speakers").join(".import"))
        .map(|dir| dir.flatten().count())
        .expect("the import directory is the daemon's own");
    assert_eq!(leftovers, 0, "the temporary file is removed");

    let reply = call(
        &web,
        post("/api/v1/speakers/forget", &format!(r#"{{"id":"{id}"}}"#)),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert_eq!(json(&reply.body)["id"], id.as_str());
    assert!(rig.stored().is_empty());
    let reply = call(
        &web,
        post("/api/v1/speakers/forget", &format!(r#"{{"id":"{id}"}}"#)),
    )
    .await;
    assert_eq!(reply.status, Code::NOT_FOUND, "{}", reply.body);
    assert_eq!(json(&reply.body)["reason"], "missing");

    let plain = serde_json::json!({"data": base64(&set_wav())}).to_string();
    let reply = call(&web, post("/api/v1/speakers/import", &plain)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let said = json(&reply.body);
    assert_eq!(
        (said["set"]["label"].as_str(), said["set"]["room"].as_str()),
        (Some("Own file"), Some("no"))
    );
    let id = said["set"]["id"].as_str().unwrap();
    let reply = call(
        &web,
        post("/api/v1/speakers/forget", &format!(r#"{{"id":"{id}"}}"#)),
    )
    .await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);

    let junk = serde_json::json!({"data": base64(b"not a set")}).to_string();
    let reply = call(&web, post("/api/v1/speakers/import", &junk)).await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
    assert_eq!(json(&reply.body)["reason"], "invalid");
    assert!(rig.stored().is_empty());

    let reply = call(&web, post("/api/v1/speakers/fetch", r#"{"id":"nowhere"}"#)).await;
    assert_eq!(reply.status, Code::NOT_FOUND, "{}", reply.body);
    assert_eq!(json(&reply.body)["reason"], "not_listed");
    let reply = call(
        &web,
        post("/api/v1/speakers/fetch", r#"{"id":"mit-normal"}"#),
    )
    .await;
    assert_eq!(
        reply.status,
        Code::NOT_FOUND,
        "the source's 404: {}",
        reply.body
    );
    assert_eq!(server.hits(), 1);
    outside.unchanged();
}

#[tokio::test]
async fn a_path_in_the_import_body_is_refused_and_the_daemon_reads_no_file_of_the_pages_choosing() {
    let _turn = IMPORTS.lock().await;
    let rig = Rig::new(None);
    let web = web(open(), Arc::clone(&rig.fleet));
    let file = rig.dir.join("secret.wav");
    std::fs::write(&file, set_wav()).unwrap();
    let body = serde_json::json!({"path": file.display().to_string()}).to_string();
    let reply = call(&web, post("/api/v1/speakers/import", &body)).await;
    assert_eq!(reply.status, Code::BAD_REQUEST, "{}", reply.body);
    assert!(!rig.dir.join("speakers").exists());
}

#[tokio::test]
async fn only_a_signed_in_writer_reaches_the_sets() {
    let server = Server::start(|_| Reply::Status(404));
    let rig = Rig::new(Some(server.url.clone()));
    let token = "a-test-token-0123456789abcdefghijklmn";
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&rig.fleet),
        Some(token),
    );
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&rig.fleet),
    );
    for path in ["catalogue", "fetch", "import", "forget"] {
        let url = format!("/api/v1/speakers/{path}");
        assert_eq!(
            call(&guests, post(&url, "{}")).await.status,
            Code::UNAUTHORIZED,
            "{path}"
        );
        assert_eq!(
            call(&read_only, post(&url, "{}")).await.status,
            Code::FORBIDDEN,
            "{path}"
        );
    }
    assert_eq!(server.hits(), 0);
    assert!(rig.stored().is_empty());
}

#[tokio::test]
async fn a_body_that_is_not_the_ops_is_a_400_and_an_oversize_one_a_413() {
    let _turn = IMPORTS.lock().await;
    let rig = Rig::new(None);
    let web = web(open(), Arc::clone(&rig.fleet));
    for (path, body) in [
        ("fetch", "[]"),
        ("fetch", "nope"),
        ("fetch", r#"{"id":3}"#),
        ("fetch", r#"{"nothing":true}"#),
        ("forget", r#"{"id":"a","more":1}"#),
        ("import", r#"{"data":"!!!"}"#),
        ("import", r#"{"data":"QUJD","room":"sideways"}"#),
    ] {
        let reply = call(&web, post(&format!("/api/v1/speakers/{path}"), body)).await;
        assert_eq!(
            reply.status,
            Code::BAD_REQUEST,
            "{path} {body}: {}",
            reply.body
        );
    }
    let big = format!(r#"{{"id":"{}"}}"#, "x".repeat(20 * 1024));
    let reply = call(&web, post("/api/v1/speakers/fetch", &big)).await;
    assert_eq!(reply.status, Code::PAYLOAD_TOO_LARGE, "{}", reply.body);
}

/// Every route but the import takes a few keys: a body over [`BODY_MAX`] is a 413 on each, the
/// catalogue's included, while the import still reads a large one.
#[tokio::test]
async fn only_the_import_takes_a_body_over_a_few_kib() {
    let _turn = IMPORTS.lock().await;
    let rig = Rig::new(None);
    let web = web(open(), Arc::clone(&rig.fleet));
    let over = format!(r#"{{"id":"{}"}}"#, "x".repeat(BODY_MAX));
    assert!(over.len() < crate::web::auth::MAX_BODY);
    for path in ["catalogue", "fetch", "forget"] {
        let reply = call(&web, post(&format!("/api/v1/speakers/{path}"), &over)).await;
        assert_eq!(
            reply.status,
            Code::PAYLOAD_TOO_LARGE,
            "{path}: {}",
            reply.body
        );
    }
    let reply = call(&web, post("/api/v1/speakers/catalogue", "{}")).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let data = format!(r#"{{"data":"{}"}}"#, "QUJD".repeat(2 * 1024));
    let reply = call(&web, post("/api/v1/speakers/import", &data)).await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
}

/// A client that goes while its job waits for the source cancels the job: the daemon's own stream
/// sees it end as cancelled.
#[tokio::test]
async fn the_job_is_cancelled_when_the_client_goes() {
    let silent = Server::start(|_| Reply::Silent);
    let rig = Rig::new(Some(silent.url.clone()));
    let mut events = rig
        .fleet
        .stream(Request::Speakers {
            op: SpeakersOp::Follow,
        })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the stream");
    let web = web(open(), Arc::clone(&rig.fleet));
    let gone = tokio::time::timeout(
        Duration::from_millis(500),
        call(
            &web,
            post("/api/v1/speakers/fetch", r#"{"id":"mit-normal"}"#),
        ),
    )
    .await;
    assert!(gone.is_err(), "the route should still be waiting");
    let ended = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match events.next().await {
                Some(Event::Speakers(event)) if event.state.is_final() => return event.state,
                Some(_) => {}
                None => panic!("the stream closed"),
            }
        }
    })
    .await
    .expect("the job ended");
    assert_eq!(ended, JobState::Cancelled);
    drop(silent);
    rig.daemon.shutdown().await;
}

/// The larger body of an import is read for a signed-in writer only: anyone else gets the
/// ordinary cap, so an anonymous client cannot make the daemon hold megabytes.
#[tokio::test]
async fn only_a_writer_may_send_a_large_import_body() {
    let _turn = IMPORTS.lock().await;
    let rig = Rig::new(None);
    let token = "a-test-token-0123456789abcdefghijklmn";
    let signed_out = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&rig.fleet),
        Some(token),
    );
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&rig.fleet),
    );
    let large = format!(r#"{{"data":"{}"}}"#, "QUJD".repeat(8 * 1024));
    assert!(large.len() > crate::web::auth::MAX_BODY);
    for page in [&signed_out, &read_only] {
        let reply = call(page, post("/api/v1/speakers/import", &large)).await;
        assert_eq!(reply.status, Code::PAYLOAD_TOO_LARGE, "{}", reply.body);
    }
    // A writer's body of that size is read (and refused as what it is: not a set).
    let writer = web(open(), Arc::clone(&rig.fleet));
    let reply = call(&writer, post("/api/v1/speakers/import", &large)).await;
    assert_eq!(reply.status, Code::UNPROCESSABLE_ENTITY, "{}", reply.body);
}

/// At most two large imports are read at once; a third is refused at once, before its body.
#[tokio::test]
async fn a_third_large_import_at_once_is_refused_busy() {
    let _turn = IMPORTS.lock().await;
    let rig = Rig::new(None);
    let web = web(open(), Arc::clone(&rig.fleet));
    let held: Vec<_> = (0..IMPORTS_AT_ONCE)
        .map(|_| import_permit().expect("a permit is free"))
        .collect();
    assert!(import_permit().is_none());
    let body = serde_json::json!({"data": base64(&set_wav())}).to_string();
    let reply = call(&web, post("/api/v1/speakers/import", &body)).await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    assert_eq!(json(&reply.body)["reason"], "busy");
    drop(held);
    let reply = call(&web, post("/api/v1/speakers/import", &body)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
}

/// A file takes longer to arrive than a few keys: a writer's import body may take most of a
/// minute, any other body is still read for five seconds only.
#[tokio::test(start_paused = true)]
async fn an_import_body_has_a_minute_to_arrive_and_other_bodies_five_seconds() {
    let _turn = IMPORTS.lock().await;
    let rig = Rig::new(None);
    let web = web(open(), Arc::clone(&rig.fleet));
    let slow = |path: &str, text: String| {
        let (parts, _) = post(path, "").into_parts();
        let half = text.len() / 2;
        let (first, second) = (text[..half].to_string(), text[half..].to_string());
        let stream = futures::stream::unfold(0, move |step| {
            let (first, second) = (first.clone(), second.clone());
            async move {
                match step {
                    0 => Some((Ok::<_, std::io::Error>(axum::body::Bytes::from(first)), 1)),
                    1 => {
                        tokio::time::sleep(Duration::from_secs(20)).await;
                        Some((Ok(axum::body::Bytes::from(second)), 2))
                    }
                    _ => None,
                }
            }
        });
        hyper::Request::from_parts(parts, axum::body::Body::from_stream(stream))
    };
    let body = serde_json::json!({"data": base64(&set_wav())}).to_string();
    let reply = call(&web, slow("/api/v1/speakers/import", body)).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    let reply = call(
        &web,
        slow(
            "/api/v1/speakers/forget",
            r#"{"id":"some-set"}"#.to_string(),
        ),
    )
    .await;
    assert_eq!(reply.status, Code::REQUEST_TIMEOUT, "{}", reply.body);
}

/// The slot is let go once the body is read: after an import (done or refused) every slot is free.
#[tokio::test]
async fn the_import_slot_is_free_again_after_each_answer() {
    let _turn = IMPORTS.lock().await;
    let rig = Rig::new(None);
    let web = web(open(), Arc::clone(&rig.fleet));
    for body in [
        serde_json::json!({"data": base64(&set_wav())}).to_string(),
        serde_json::json!({"data": base64(b"not a set")}).to_string(),
        r#"{"data":"!!!"}"#.to_string(),
    ] {
        call(&web, post("/api/v1/speakers/import", &body)).await;
        let held: Vec<_> = (0..IMPORTS_AT_ONCE).map(|_| import_permit()).collect();
        assert!(held.iter().all(Option::is_some), "a slot stayed taken");
    }
}

/// A refused choice of a set the store lacks reaches the page with its reason, so the page reads
/// the sets again on `missing` rather than on words.
#[tokio::test]
async fn a_refused_choice_of_a_set_the_store_lacks_carries_missing_to_the_page() {
    let rig = Rig::new(None);
    let params = svolder_host::fake::FakeParams::new();
    params.with_speakers_ports(false);
    rig.daemon
        .host()
        .run_with_eq(svolder_host::fake::FakeChain::new(), params);
    rig.daemon.link_speakers_to_host();
    let web = web(open(), Arc::clone(&rig.fleet));
    let reply = call(
        &web,
        post(
            "/api/v1/host/speakers",
            r#"{"on":true,"set":"never-fetched"}"#,
        ),
    )
    .await;
    assert_eq!(reply.status, Code::CONFLICT, "{}", reply.body);
    let body: serde_json::Value = serde_json::from_str(&reply.body).expect("JSON");
    assert_eq!(body["reason"], "missing", "{}", reply.body);
    rig.daemon.shutdown().await;
}
