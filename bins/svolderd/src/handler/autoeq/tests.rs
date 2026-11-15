// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq ops through the daemon, against a loopback server: no test reaches a network.

use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use svolder_autoeq::test_server::{Reply, Server};
use svolder_autoeq::{Client, Limits};
use svolder_ipc::{
    AutoEqOp, AutoEqTarget, AutoEqVariant, Event, Handler, JobEvent, JobState, Request,
};

use super::*;
use crate::handler::tests::daemon_over;

pub(super) const INDEX: &str = "# AutoEq\n\n\
- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990\n\
- [Sennheiser HD 600](./oratory1990/over-ear/Sennheiser%20HD%20600) by oratory1990\n\
- [Sennheiser HD 650](./crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650) by crinacle on GRAS 43AG-7\n";

const PEQ: &str = "Preamp: -6.1 dB\n\
Filter 1: ON LSC Fc 105 Hz Gain 6.0 dB Q 0.70\n\
Filter 2: ON PK Fc 1000 Hz Gain -2.0 dB Q 1.00\n\
Filter 3: ON PK Fc 3500 Hz Gain 3.0 dB Q 2.00\n";

pub(super) fn index_server() -> Server {
    Server::start(|path| match path {
        "/results/INDEX.md" => Reply::Ok(INDEX.as_bytes().to_vec()),
        p if p.contains("HD%20600") => Reply::Status(404),
        p if p.ends_with("ParametricEQ.txt") => Reply::Ok(PEQ.as_bytes().to_vec()),
        _ => Reply::Status(404),
    })
}

/// A test's directory in the temp dir, removed when it drops, whether the test ends or fails.
pub(super) struct Dir(PathBuf);

impl std::ops::Deref for Dir {
    type Target = std::path::Path;

    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        // A thread still writing into the directory (a download on its way out) can make one
        // removal fail or put a file back, so it is retried until the directory is gone.
        for _ in 0..100 {
            let _ = std::fs::remove_dir_all(&self.0);
            if !self.0.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

pub(super) fn dir(tag: &str) -> Dir {
    let dir = std::env::temp_dir().join(format!("svolderd-autoeq-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    std::fs::write(
        dir.join("config.toml"),
        "[[device]]\nname = \"dac\"\nudid = \"0102030405060708\"\nprofile = \"gungnir2-v2\"\n\n\
         [[device]]\nname = \"ghost\"\nudid = \"1111111111111111\"\n",
    )
    .unwrap();
    Dir(dir)
}

/// A shared daemon with one configured device, `dac`, reaching `server` and caching in `dir`.
pub(super) async fn daemon_for(dir: &std::path::Path, server: &Server) -> Arc<Daemon> {
    let config = dir.join("config.toml");
    let loaded = svolder_ipc::Config::load_from(&config).unwrap();
    let client = Client::new(server.url.clone(), dir.join("cache"), Limits::default());
    let daemon = daemon_over(config, dir.join("profiles"))
        .with_autoeq(client)
        .shared();
    daemon.reconcile(&loaded).await;
    daemon
}

pub(super) fn variant(source: &str) -> AutoEqVariant {
    AutoEqVariant {
        name: "Sennheiser HD 650".into(),
        source: source.into(),
        form: "over-ear".into(),
        rig: (source == "crinacle").then(|| "GRAS 43AG-7".to_string()),
    }
}

fn search_op(query: &str) -> Request {
    Request::AutoEq {
        op: AutoEqOp::Search {
            query: query.into(),
            refresh: false,
        },
    }
}

/// The job id a start answered with.
fn job_of(answer: Response) -> u64 {
    match answer {
        Response::Job { job } => job,
        other => panic!("not a job: {other:?}"),
    }
}

/// The last state of job `job` on `events`.
async fn finish(events: &mut BoxStream<'static, Event>, job: u64) -> JobState {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(20), events.next())
            .await
            .expect("an event in time")
            .expect("the stream is open");
        if let Event::AutoEq(JobEvent { job: id, state }) = event
            && id == job
            && state.is_final()
        {
            return state;
        }
    }
}

async fn follow(daemon: &Daemon) -> BoxStream<'static, Event> {
    daemon
        .stream(Request::AutoEq {
            op: AutoEqOp::Follow,
        })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the stream")
}

#[tokio::test]
async fn a_search_answers_groups_and_loads_the_index_once_for_all_askers() {
    let server = index_server();
    let dir = dir("search");
    let daemon = daemon_for(&dir, &server).await;
    let asks = (0..6).map(|_| daemon.handle(search_op("hd 650")));
    let answers = futures::future::join_all(asks).await;
    for answer in answers {
        let Response::AutoEqSearch(found) = answer else {
            panic!("{answer:?}");
        };
        assert_eq!(found.groups.len(), 1);
        assert_eq!(found.groups[0].variants.len(), 2);
        assert_eq!(found.groups[0].variants[0].variant.source, "oratory1990");
        assert_eq!(found.measurements, 3);
    }
    assert_eq!(server.hits(), 1, "one load served every asker");
    // The next search reads the copy kept: no second fetch.
    daemon.handle(search_op("hd")).await;
    assert_eq!(server.hits(), 1);
    // A refresh fetches again.
    daemon
        .handle(Request::AutoEq {
            op: AutoEqOp::Search {
                query: "hd".into(),
                refresh: true,
            },
        })
        .await;
    assert_eq!(server.hits(), 2);
}

#[tokio::test]
async fn a_long_query_is_refused_and_a_dead_repository_is_unreachable() {
    let server = index_server();
    let dir = dir("refuse");
    let daemon = daemon_for(&dir, &server).await;
    let long = "x".repeat(AUTOEQ_QUERY_MAX + 1);
    let Response::Error { reason, .. } = daemon.handle(search_op(&long)).await else {
        panic!("not refused");
    };
    assert_eq!(reason, Some(Reason::Invalid));
    assert_eq!(server.hits(), 0, "a refused query reaches nothing");
    let down = Server::start(|_| Reply::Status(500));
    let other = self::dir("down");
    let daemon = daemon_for(&other, &down).await;
    let Response::Error { reason, .. } = daemon.handle(search_op("hd")).await else {
        panic!("not refused");
    };
    assert_eq!(reason, Some(Reason::Unreachable));
}

#[tokio::test]
async fn a_search_with_autoeq_off_says_so_in_words() {
    let dir = dir("off");
    let config = dir.join("config.toml");
    let loaded = svolder_ipc::Config::load_from(&config).unwrap();
    let client = Client::for_transport(
        "fake",
        Some("http://127.0.0.1:9/results/".into()),
        dir.join("cache"),
    );
    let daemon = daemon_over(config, dir.join("profiles"))
        .with_autoeq(client)
        .shared();
    daemon.reconcile(&loaded).await;
    let Response::Error {
        reason, message, ..
    } = daemon.handle(search_op("hd")).await
    else {
        panic!("not refused");
    };
    assert_eq!(reason, Some(Reason::Unreachable));
    assert!(message.starts_with(svolder_autoeq::OFF_PREFIX), "{message}");
    assert!(message.contains("fake device"), "{message}");
}

/// The web page recognises the daemon's words for AutoEq being off by their start; a reword of
/// the start must change the page with it.
#[test]
fn the_web_page_recognises_the_off_words() {
    let page = include_str!("../../../web/core/popups/eq-library/draw.js");
    let quoted = format!("startsWith('{}')", svolder_autoeq::OFF_PREFIX);
    assert!(page.contains(&quoted), "draw.js does not match {quoted}");
}

#[tokio::test]
async fn a_daemon_given_no_client_reaches_nothing() {
    let daemon = crate::handler::tests::daemon();
    let Response::Error { reason, .. } = daemon.handle(search_op("hd")).await else {
        panic!("not refused");
    };
    assert_eq!(reason, Some(Reason::Internal));
}

#[tokio::test]
async fn a_preview_job_for_a_device_carries_the_fit_and_a_compare() {
    let server = index_server();
    let dir = dir("preview");
    let daemon = daemon_for(&dir, &server).await;
    let mut events = follow(&daemon).await;
    let job = job_of(
        daemon
            .handle(Request::AutoEq {
                op: AutoEqOp::Preview {
                    variant: Box::new(variant("oratory1990")),
                    target: AutoEqTarget::Device {
                        device: "dac".into(),
                    },
                    compare: Some(Box::new(variant("crinacle"))),
                },
            })
            .await,
    );
    let JobState::Done { payload } = finish(&mut events, job).await else {
        panic!("not done");
    };
    let done: AutoEqPreview = serde_json::from_value(payload).expect("a preview");
    assert_eq!(done.preview.freqs.len(), 181);
    assert!(done.preview.fit.is_some(), "a device is fitted");
    let compare = done.compare.expect("the compare");
    assert_eq!(compare.target.len(), 181);
    // The two files are one URL each; a second preview of the first reads the copy kept.
    let hits = server.hits();
    let job = job_of(
        daemon
            .handle(Request::AutoEq {
                op: AutoEqOp::Preview {
                    variant: Box::new(variant("oratory1990")),
                    target: AutoEqTarget::Computer,
                    compare: None,
                },
            })
            .await,
    );
    let JobState::Done { payload } = finish(&mut events, job).await else {
        panic!("not done");
    };
    let done: AutoEqPreview = serde_json::from_value(payload).unwrap();
    assert!(done.preview.fit.is_none(), "the computer has no fit");
    assert_eq!(server.hits(), hits, "the download was kept");
}

#[tokio::test]
async fn a_preview_names_its_side_and_its_measurement() {
    let server = index_server();
    let dir = dir("sides");
    let daemon = daemon_for(&dir, &server).await;
    let preview = |variant, device: &str| Request::AutoEq {
        op: AutoEqOp::Preview {
            variant: Box::new(variant),
            target: AutoEqTarget::Device {
                device: device.into(),
            },
            compare: None,
        },
    };
    // A device never found, with no profile named for it, has no band count to preview for.
    let Response::Error { reason, .. } = daemon
        .handle(preview(variant("oratory1990"), "ghost"))
        .await
    else {
        panic!("not refused");
    };
    assert_eq!(reason, Some(Reason::NotFoundYet));
    // A device nothing configures is refused before any job starts.
    let Response::Error { reason, .. } = daemon
        .handle(preview(variant("oratory1990"), "no such"))
        .await
    else {
        panic!("not refused");
    };
    assert_eq!(reason, Some(Reason::Missing));
    // A measurement the index does not list ends the job as not_listed, and one whose file
    // is a 404 the same.
    let mut events = follow(&daemon).await;
    for (name, source) in [
        ("Nope", "oratory1990"),
        ("Sennheiser HD 600", "oratory1990"),
    ] {
        let mut asked = variant(source);
        asked.name = name.into();
        let job = job_of(daemon.handle(preview(asked, "dac")).await);
        let JobState::Failed { reason, .. } = finish(&mut events, job).await else {
            panic!("did not fail");
        };
        assert_eq!(reason, Reason::NotListed, "{name}");
    }
}

#[tokio::test]
async fn an_import_job_saves_an_imported_preset_and_refuses_a_taken_name() {
    let server = index_server();
    let dir = dir("import");
    let daemon = daemon_for(&dir, &server).await;
    let mut events = follow(&daemon).await;
    let import = |name: Option<&str>, force: bool, target: AutoEqTarget| Request::AutoEq {
        op: AutoEqOp::Import {
            variant: Box::new(variant("oratory1990")),
            name: name.map(str::to_string),
            force,
            target,
        },
    };
    let on_dac = || AutoEqTarget::Device {
        device: "dac".into(),
    };
    let job = job_of(daemon.handle(import(None, false, on_dac())).await);
    let JobState::Done { payload } = finish(&mut events, job).await else {
        panic!("not done");
    };
    let saved: AutoEqSaved = serde_json::from_value(payload).unwrap();
    assert_eq!(saved.name, "Sennheiser HD 650");
    assert_eq!(saved.filters, 3);
    let presets = daemon.config_snapshot().presets.clone();
    assert_eq!(presets.len(), 1);
    assert_eq!(
        presets[0].source.as_ref().unwrap().origin.as_deref(),
        Some("AutoEq · oratory1990 · over-ear")
    );
    let job = job_of(daemon.handle(import(None, false, on_dac())).await);
    let JobState::Failed { reason, .. } = finish(&mut events, job).await else {
        panic!("did not fail");
    };
    assert_eq!(reason, Reason::Taken);
    let job = job_of(daemon.handle(import(None, true, on_dac())).await);
    assert!(matches!(
        finish(&mut events, job).await,
        JobState::Done { .. }
    ));
    let job = job_of(
        daemon
            .handle(import(Some("   "), false, AutoEqTarget::Computer))
            .await,
    );
    let JobState::Failed { reason, .. } = finish(&mut events, job).await else {
        panic!("did not fail");
    };
    assert_eq!(
        reason,
        Reason::Taken,
        "a blank name takes the headphone's own"
    );
}

#[tokio::test]
async fn an_upload_saves_text_and_refuses_what_is_not_a_correction() {
    let server = index_server();
    let dir = dir("upload");
    let daemon = daemon_for(&dir, &server).await;
    let upload = |text: &str, name: &str| Request::AutoEq {
        op: AutoEqOp::Upload {
            text: text.into(),
            name: name.into(),
            force: false,
        },
    };
    let Response::AutoEqSaved(saved) = daemon.handle(upload(PEQ, " mine ")).await else {
        panic!("not saved");
    };
    assert_eq!((saved.name.as_str(), saved.filters), ("mine", 3));
    for (text, name, reason) in [
        ("not eq", "x", Reason::Invalid),
        (&"a".repeat(apo::MAX_BYTES + 1), "x", Reason::Invalid),
        (PEQ, "mine", Reason::Taken),
        (PEQ, "  ", Reason::BadName),
    ] {
        let Response::Error { reason: got, .. } = daemon.handle(upload(text, name)).await else {
            panic!("not refused: {name}");
        };
        assert_eq!(got, Some(reason), "{name}");
    }
    assert_eq!(server.hits(), 0, "an upload reaches no network");
}

#[tokio::test]
async fn a_read_only_file_refuses_an_import_with_its_reason() {
    let server = index_server();
    let dir = dir("readonly");
    let daemon = daemon_for(&dir, &server).await;
    let locked_dir = dir.join("locked");
    std::fs::create_dir_all(&locked_dir).unwrap();
    let file = locked_dir.join("config.toml");
    std::fs::write(&file, "schema = 1\n").unwrap();
    let mut mode = std::fs::metadata(&file).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o444);
    std::fs::set_permissions(&file, mode).unwrap();
    let readonly = daemon_over(file, dir.join("profiles"))
        .with_autoeq(Client::new(
            server.url.clone(),
            dir.join("cache2"),
            Limits::default(),
        ))
        .shared();
    drop(daemon);
    let Response::Error { reason, .. } = readonly
        .handle(Request::AutoEq {
            op: AutoEqOp::Upload {
                text: PEQ.into(),
                name: "mine".into(),
                force: false,
            },
        })
        .await
    else {
        panic!("not refused");
    };
    assert_eq!(reason, Some(Reason::ReadOnly));
}

#[tokio::test]
async fn a_fifth_job_is_busy_and_cancel_ends_a_waiting_download() {
    let silent = Server::start(|path| match path {
        "/results/INDEX.md" => Reply::Ok(INDEX.as_bytes().to_vec()),
        _ => Reply::Silent,
    });
    let dir = dir("busy");
    let daemon = daemon_for(&dir, &silent).await;
    let mut events = follow(&daemon).await;
    let preview = |compare: bool| Request::AutoEq {
        op: AutoEqOp::Preview {
            variant: Box::new(variant("oratory1990")),
            target: AutoEqTarget::Computer,
            compare: compare.then(|| Box::new(variant("crinacle"))),
        },
    };
    let first = job_of(daemon.handle(preview(false)).await);
    let mut jobs = vec![first];
    for _ in 1..JOBS {
        jobs.push(job_of(daemon.handle(preview(true)).await));
    }
    let Response::Error { reason, .. } = daemon.handle(preview(false)).await else {
        panic!("a fifth start was accepted");
    };
    assert_eq!(reason, Some(Reason::Busy));
    assert_eq!(
        daemon
            .handle(Request::AutoEq {
                op: AutoEqOp::Cancel { job: first }
            })
            .await,
        Response::Ack
    );
    // Every job is ended before the directory goes, so none is still writing into it.
    for job in &jobs[1..] {
        assert_eq!(
            daemon
                .handle(Request::AutoEq {
                    op: AutoEqOp::Cancel { job: *job }
                })
                .await,
            Response::Ack
        );
    }
    let mut ended = std::collections::HashMap::new();
    while ended.len() < jobs.len() {
        let event = tokio::time::timeout(Duration::from_secs(20), events.next())
            .await
            .expect("an event in time")
            .expect("the stream is open");
        if let Event::AutoEq(JobEvent { job, state }) = event
            && state.is_final()
        {
            ended.insert(job, state);
        }
    }
    assert_eq!(ended[&first], JobState::Cancelled);
    daemon.shutdown().await;
}
