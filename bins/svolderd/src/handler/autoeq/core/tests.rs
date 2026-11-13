// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use svolder_autoeq::test_server::{Reply, Server};
use svolder_autoeq::{Client, Limits};

use super::super::tests::{INDEX, dir, index_server, variant};
use super::*;

fn core_for(dir: &std::path::Path, server: &Server) -> Core {
    Core::new(Some(Client::new(
        server.url.clone(),
        dir.join("cache"),
        Limits::default(),
    )))
}

/// A failed load is shared by the requests that were waiting on it, a request just after gets the
/// failure without a fetch, and one for the index `Now` tries again.
#[tokio::test]
async fn a_failed_load_is_shared_by_those_waiting_and_retried_on_demand() {
    let dir = dir("core-fail");
    let server = Server::start(|_| Reply::Status(500));
    let core = std::sync::Arc::new(core_for(&dir, &server));
    let asks = (0..4).map(|_| {
        let core = std::sync::Arc::clone(&core);
        async move { core.index(Refresh::Never).await }
    });
    let results = futures::future::join_all(asks).await;
    assert!(results.iter().all(|r| r.is_err()));
    assert!(
        server.hits() < 4,
        "the waiting askers share one failure, not {}",
        server.hits()
    );
    let before = server.hits();
    assert!(core.index(Refresh::Never).await.is_err());
    assert_eq!(server.hits(), before, "a request just after is not fetched");
    assert!(core.index(Refresh::Now).await.is_err());
    assert!(server.hits() > before, "a refresh tries again");
}

#[tokio::test]
async fn only_a_listed_measurement_is_looked_up() {
    let dir = dir("core-entry");
    let server = index_server();
    let core = core_for(&dir, &server);
    let entry = core
        .entry(&variant("crinacle"), Refresh::Never)
        .await
        .expect("listed");
    assert_eq!(entry.rig.as_deref(), Some("GRAS 43AG-7"));
    let mut wrong = variant("crinacle");
    wrong.rig = None;
    let refused = core.entry(&wrong, Refresh::Never).await.unwrap_err();
    assert_eq!(refused.reason, Reason::NotListed);
    assert!(INDEX.contains("crinacle"));
}

#[tokio::test]
async fn the_computer_refuses_more_filters_than_it_holds() {
    use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
    let core = Core::new(None);
    let many = Correction {
        preamp_db: 0.0,
        filters: (0..17)
            .map(|n| SourceFilter {
                kind: FilterKind::Peaking,
                f: 100.0 + f64::from(n) * 100.0,
                g: 1.0,
                q: 1.0,
            })
            .collect(),
    };
    let refused = core.preview(Side::Computer, many).await.unwrap_err();
    assert_eq!(refused.reason, Reason::Invalid);
}

/// A failed refresh with a stale cache serves the stale copy, and the next requests do not fetch
/// again: one attempt per back-off, not one per request.
#[tokio::test]
async fn a_failed_refresh_serves_the_stale_copy_without_fetching_again() {
    let dir = dir("core-stale");
    let cache = dir.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let file = cache.join("INDEX.md");
    std::fs::write(&file, INDEX).unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(30 * 7 * 24 * 3600);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let server = Server::start(|_| Reply::Status(500));
    let core = core_for(&dir, &server);
    for _ in 0..3 {
        let index = core
            .index(Refresh::AfterWeeks(4))
            .await
            .expect("the stale copy");
        assert!(index.note.is_some(), "the copy says it is stale");
    }
    assert_eq!(server.hits(), 1, "one attempt, not one per request");
    // An explicit refresh goes round the back-off.
    core.index(Refresh::Now).await.expect("the stale copy");
    assert_eq!(server.hits(), 2);
}

/// A waiter that goes away leaves the load running, and the next request joins it: one fetch.
#[tokio::test]
async fn a_dropped_waiter_does_not_start_a_second_load() {
    let dir = dir("core-drop");
    let server = Server::start(|_| Reply::Silent);
    let core = std::sync::Arc::new(core_for(&dir, &server));
    let first = tokio::time::timeout(Duration::from_millis(300), core.index(Refresh::Never)).await;
    assert!(first.is_err(), "the load is still going");
    let second = tokio::time::timeout(Duration::from_millis(300), core.index(Refresh::Never)).await;
    assert!(second.is_err());
    assert_eq!(server.hits(), 1, "the second request joined the first load");
}

/// A failed load while a stale copy with no note is held serves that copy with a note and backs
/// off: three requests make one fetch.
#[tokio::test]
async fn a_failed_load_with_a_plain_stale_copy_serves_it_and_backs_off() {
    let dir = dir("core-plain");
    let cache = dir.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let file = cache.join("INDEX.md");
    std::fs::write(&file, INDEX).unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(30 * 7 * 24 * 3600);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let server = Server::start(|_| Reply::Status(500));
    let core = core_for(&dir, &server);
    // Loaded as it is (never refreshed), so the copy held has no note.
    assert!(core.index(Refresh::Never).await.unwrap().note.is_none());
    // The cache file goes; the next refresh fails and finds no disk copy of its own.
    std::fs::remove_file(&file).unwrap();
    for _ in 0..3 {
        let index = core
            .index(Refresh::AfterWeeks(4))
            .await
            .expect("the held copy");
        assert!(index.note.is_some());
    }
    assert_eq!(server.hits(), 1, "one attempt, not one per request");
}
