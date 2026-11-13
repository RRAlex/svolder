// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);
use crate::test_server::{Reply, Server};

const INDEX: &str = "This is a list.\n\n\
- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990\n\
- [Sennheiser HD 650](./crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650) by crinacle on GRAS 43AG-7\n";

const PEQ: &str = "Preamp: -6.1 dB\nFilter 1: ON PK Fc 1000 Hz Gain 2.0 dB Q 1.00\n";

fn short() -> Limits {
    Limits {
        connect: Duration::from_millis(500),
        index_total: Duration::from_millis(600),
        file_total: Duration::from_millis(600),
        index_bytes: 4096,
        file_bytes: 1024,
    }
}

fn dir() -> Dir {
    static N: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "svolder-autoeq-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    Dir(path)
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Dir {
    fn cache(&self) -> PathBuf {
        self.0.join("cache").join("autoeq")
    }

    fn index_file(&self) -> PathBuf {
        self.cache().join(CACHE_FILE)
    }
}

fn index_server() -> Server {
    Server::start(|path| match path {
        "/results/INDEX.md" => Reply::Ok(INDEX.as_bytes().to_vec()),
        p if p.ends_with("ParametricEQ.txt") => Reply::Ok(PEQ.as_bytes().to_vec()),
        _ => Reply::Status(404),
    })
}

fn age(path: &std::path::Path, by: Duration) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - by).unwrap();
}

#[test]
fn the_index_is_fetched_and_cached() {
    let server = index_server();
    let dir = dir();
    let client = Client::new(server.url.clone(), dir.cache(), short());
    let index = client.index(Refresh::AfterWeeks(13)).unwrap();
    assert_eq!(index.entries.len(), 2);
    assert!(index.note.is_none());
    assert_eq!(std::fs::read_to_string(dir.index_file()).unwrap(), INDEX);
    // No temporary file is left beside it.
    let names: Vec<_> = std::fs::read_dir(dir.cache()).unwrap().collect();
    assert_eq!(names.len(), 1);
    let again = client.index(Refresh::AfterWeeks(13)).unwrap();
    assert_eq!(again.entries, index.entries);
    assert_eq!(server.hits(), 1, "a fresh cache is not fetched again");
}

#[test]
fn the_cache_age_decides_when_to_fetch() {
    let server = index_server();
    let dir = dir();
    let client = Client::new(server.url.clone(), dir.cache(), short());
    client.index(Refresh::Never).unwrap();
    assert_eq!(server.hits(), 1);
    age(&dir.index_file(), WEEK * 3);

    client.index(Refresh::Never).unwrap();
    assert_eq!(server.hits(), 1, "Never keeps the old copy");
    client.index(Refresh::AfterWeeks(4)).unwrap();
    assert_eq!(server.hits(), 1, "three weeks is under four");
    client.index(Refresh::AfterWeeks(2)).unwrap();
    assert_eq!(server.hits(), 2, "three weeks is over two");
    client.index(Refresh::Now).unwrap();
    assert_eq!(server.hits(), 3);
    age(&dir.index_file(), Duration::ZERO);
    client.index(Refresh::AfterWeeks(1)).unwrap();
    assert_eq!(server.hits(), 3);
}

#[test]
fn a_failed_refresh_falls_back_to_the_cache() {
    let good = index_server();
    let dir = dir();
    Client::new(good.url.clone(), dir.cache(), short())
        .index(Refresh::Now)
        .unwrap();
    age(&dir.index_file(), WEEK * 20);
    let bad = Server::start(|_| Reply::Status(500));
    let client = Client::new(bad.url.clone(), dir.cache(), short());
    let index = client.index(Refresh::AfterWeeks(13)).unwrap();
    assert_eq!(index.entries.len(), 2);
    let note = index.note.unwrap();
    let want = format!(
        "using the index from {} (refresh failed: ",
        date(index.fetched)
    );
    assert!(note.starts_with(&want), "{note}");
    assert!(note.contains("500"), "{note}");
    assert_eq!(std::fs::read_to_string(dir.index_file()).unwrap(), INDEX);
    // Now on demand fails the same way.
    assert!(client.index(Refresh::Now).unwrap().note.is_some());
}

#[test]
fn no_cache_and_no_network_refuses() {
    let dir = dir();
    let client = Client::new("http://127.0.0.1:9/".into(), dir.cache(), short());
    let error = client.index(Refresh::Never).unwrap_err();
    assert!(matches!(error, AutoEqError::NoIndex { .. }), "{error}");
    assert!(!dir.index_file().exists());
}

#[test]
fn an_index_without_an_entry_is_refused_and_not_cached() {
    let server = Server::start(|_| Reply::Ok(b"<html>not an index</html>".to_vec()));
    let dir = dir();
    let client = Client::new(server.url.clone(), dir.cache(), short());
    assert!(matches!(
        client.index(Refresh::Now).unwrap_err(),
        AutoEqError::NoIndex { .. }
    ));
    assert!(!dir.index_file().exists());
}

#[test]
fn the_request_names_svolder_and_its_version() {
    let server = index_server();
    let dir = dir();
    let client = Client::new(server.url.clone(), dir.cache(), short());
    client.index(Refresh::Now).unwrap();
    let agent = server
        .requests()
        .into_iter()
        .find(|line| line.to_lowercase().starts_with("user-agent:"))
        .unwrap();
    assert_eq!(
        agent.to_lowercase(),
        format!("user-agent: svolder/{}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn a_file_is_downloaded_from_its_encoded_url() {
    let server = index_server();
    let dir = dir();
    let client = Client::new(server.url.clone(), dir.cache(), short());
    let index = client.index(Refresh::Now).unwrap();
    let text = client.download(&index.entries[1]).unwrap();
    assert_eq!(text, PEQ);
    let last = server
        .requests()
        .into_iter()
        .rev()
        .find(|l| l.starts_with("GET"))
        .unwrap();
    assert!(
        last.contains("/results/crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650/Sennheiser%20HD%20650%20ParametricEQ.txt"),
        "{last}"
    );
}

/// Review Focus 2: each misbehaving server ends in a named error within the limits and leaves
/// the cache as it was.
#[test]
fn a_misbehaving_server_never_hangs_or_floods() {
    let good = index_server();
    let dir = dir();
    let seed = Client::new(good.url.clone(), dir.cache(), short());
    let entry = seed.index(Refresh::Now).unwrap().entries[0].clone();
    let before = std::fs::read(dir.index_file()).unwrap();

    let silent = Server::start(|_| Reply::Silent);
    let looping = Server::start(|path| Reply::Redirect(path.to_string()));
    let flood = Server::start(|_| Reply::Flood(10 * 1024 * 1024));
    let missing = Server::start(|_| Reply::Status(404));

    let started = std::time::Instant::now();
    let client = Client::new(silent.url.clone(), dir.cache(), short());
    let error = client.download(&entry).unwrap_err();
    assert!(matches!(error, AutoEqError::Fetch { .. }), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(client.index(Refresh::Now).unwrap().note.is_some());

    let client = Client::new(looping.url.clone(), dir.cache(), short());
    let error = client.download(&entry).unwrap_err();
    assert!(matches!(error, AutoEqError::Fetch { .. }), "{error}");
    assert!(looping.hits() <= 7, "{} hits", looping.hits());

    let client = Client::new(flood.url.clone(), dir.cache(), short());
    let error = client.download(&entry).unwrap_err();
    assert!(
        matches!(error, AutoEqError::TooLarge { limit: 1024, .. }),
        "{error}"
    );
    let indexed = client.index(Refresh::Now).unwrap();
    assert!(indexed.note.unwrap().contains("larger than 4096 bytes"));

    let client = Client::new(missing.url.clone(), dir.cache(), short());
    let error = client.download(&entry).unwrap_err();
    assert!(
        matches!(error, AutoEqError::Status { status: 404, .. }),
        "{error}"
    );

    assert_eq!(std::fs::read(dir.index_file()).unwrap(), before);
    let files: Vec<_> = std::fs::read_dir(dir.cache()).unwrap().collect();
    assert_eq!(files.len(), 1, "no temporary file is left");
}

#[test]
fn a_binary_body_is_not_text() {
    let server = Server::start(|_| Reply::Ok(vec![0xff, 0xfe, 0x00]));
    let dir = dir();
    let client = Client::new(server.url.clone(), dir.cache(), short());
    let entry = parse_index(INDEX).remove(0);
    assert!(matches!(
        client.download(&entry).unwrap_err(),
        AutoEqError::NotText { .. }
    ));
}

#[test]
fn dates_are_utc_calendar_days() {
    let at = |secs| UNIX_EPOCH + Duration::from_secs(secs);
    assert_eq!(date(at(0)), "1970-01-01");
    assert_eq!(date(at(951_782_400)), "2000-02-29");
    assert_eq!(date(at(1_782_864_000 + 86_399)), "2026-07-01");
}

#[test]
fn the_default_limits_are_the_documented_ones() {
    let limits = Limits::default();
    assert_eq!(limits.connect, Duration::from_secs(10));
    assert_eq!(limits.index_total, Duration::from_secs(60));
    assert_eq!(limits.file_total, Duration::from_secs(20));
    assert_eq!(limits.index_bytes, 8 * 1024 * 1024);
    assert_eq!(limits.file_bytes, 64 * 1024);
}

#[test]
fn only_loopback_urls_skip_the_proxy() {
    assert!(is_loopback("http://127.0.0.1:8080/"));
    assert!(is_loopback("http://localhost/x"));
    assert!(!is_loopback("https://raw.githubusercontent.com/"));
    assert!(!is_loopback("http://example.com/"));
}

#[test]
fn an_https_base_never_follows_a_redirect_to_http() {
    let dir = dir();
    let secure = Client::new("https://example.com/results/".into(), dir.cache(), short());
    assert!(secure.agent(Duration::from_secs(1)).config().https_only());
    let plain = Client::new("http://127.0.0.1:9/".into(), dir.cache(), short());
    assert!(!plain.agent(Duration::from_secs(1)).config().https_only());
}

/// A cache directory that cannot be written does not lose the index just fetched: it comes back
/// with a note.
#[test]
fn an_unwritable_cache_still_returns_the_fetched_index() {
    let server = index_server();
    let dir = dir();
    std::fs::create_dir_all(&dir.0).unwrap();
    // The cache directory's place is a file, so it cannot be created.
    let blocked = dir.0.join("cache");
    std::fs::write(&blocked, "x").unwrap();
    let client = Client::new(server.url.clone(), blocked.join("autoeq"), short());
    let index = client.index(Refresh::Never).expect("the fetched index");
    assert_eq!(index.entries.len(), 2);
    assert!(
        index
            .note
            .as_deref()
            .is_some_and(|n| n.starts_with("not cached")),
        "{:?}",
        index.note
    );
}

/// Under the fake transport the default base is this machine's own, where nothing listens; the
/// real one is AutoEq's repository over HTTPS.
#[test]
fn the_fake_transport_never_defaults_to_the_internet() {
    assert!(is_loopback(default_base(true)));
    assert!(default_base(false).starts_with("https://"));
    let client = Client::new(default_base(true).to_string(), dir().cache(), short());
    assert!(client.index(Refresh::Never).is_err());
}
