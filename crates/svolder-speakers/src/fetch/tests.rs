// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::Write;
use std::sync::atomic::{AtomicU32, Ordering};

use svolder_autoeq::test_server::{Reply, Server};

use super::*;
use crate::catalogue::Room;
use crate::sha::sha256_hex;
use crate::wav;

struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "svolder-speakers-fetch-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        Dir(path)
    }

    /// The files left in the store's directory, sorted: a directory lists in no set order.
    fn left(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.0)
            .map(|dir| {
                dir.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dirac(at: usize, gain: f32) -> Vec<f32> {
    let mut v = vec![0.0; 400];
    v[at] = gain;
    v
}

/// A 4-channel WAV: the synthetic stand-in for a download.
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

fn zip_of(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(name, options).unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
    out.into_inner()
}

/// An entry for `body`, served at `path` of whatever host.
fn entry(id: &str, path: &str, body: &[u8], container: Container) -> Entry {
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    Entry {
        id: leak(id.to_string()),
        room: Room::No,
        label: "Test set",
        url: leak(format!("https://data.example.invalid{path}")),
        bytes: body.len() as u64,
        sha256: leak(sha256_hex(body)),
        container,
        left_az: 30.0,
        right_az: 330.0,
        licence: "CC-BY-4.0",
        credit: "A test credit",
        source_page: "https://example.invalid/page",
    }
}

fn short() -> Limits {
    Limits {
        connect: Duration::from_millis(500),
        total: Duration::from_millis(800),
        download_bytes: 8 << 20,
        member_bytes: 1 << 20,
    }
}

fn client(server: &Server) -> Client {
    Client::new(
        Some(server.url.trim_end_matches("/results/").to_string()),
        short(),
    )
}

fn run(client: &Client, dir: &Dir, entry: &Entry, cancel: &Cancel) -> Result<Saved, FetchError> {
    client.fetch_entry(
        &Store::new(&dir.0),
        entry,
        100,
        None,
        cancel,
        &mut |_, _| {},
    )
}

#[test]
fn a_listed_set_is_fetched_checked_converted_and_stored_and_the_download_is_gone() {
    let body = set_wav();
    let server = Server::start({
        let body = body.clone();
        move |path| {
            if path == "/data/set.sofa" {
                Reply::Ok(body.clone())
            } else {
                Reply::Status(404)
            }
        }
    });
    let dir = Dir::new();
    let e = entry("test-set", "/data/set.sofa", &body, Container::Sofa);
    let mut seen = Vec::new();
    let saved = client(&server)
        .fetch_entry(
            &Store::new(&dir.0),
            &e,
            100,
            None,
            &Cancel::new(),
            &mut |got, of| seen.push((got, of)),
        )
        .unwrap();
    assert_eq!(server.hits(), 1);
    assert!(
        server.requests()[0].starts_with("GET /data/set.sofa "),
        "{:?}",
        server.requests()
    );
    let meta = &saved.meta;
    assert_eq!(
        (
            meta.id.as_str(),
            meta.licence.as_str(),
            meta.credit.as_str()
        ),
        ("test-set", "CC-BY-4.0", "A test credit")
    );
    assert_eq!(meta.source_sha256, e.sha256);
    assert_eq!(meta.source_url, "https://example.invalid/page");
    assert!(!meta.own && meta.added == 100);
    assert_eq!(seen.last(), Some(&(body.len() as u64, body.len() as u64)));
    assert_eq!(
        dir.left(),
        ["test-set.toml", "test-set.wav"],
        "no .part is left"
    );
    assert!(Store::new(&dir.0).check("test-set").is_ok());
}

#[test]
fn a_zip_member_is_read_and_converted() {
    let body = zip_of("folder/set.wav", &set_wav());
    let server = Server::start({
        let body = body.clone();
        move |_| Reply::Ok(body.clone())
    });
    let dir = Dir::new();
    let e = entry(
        "zipped",
        "/z/set.zip",
        &body,
        Container::Zip {
            member: "folder/set.wav",
        },
    );
    assert!(run(&client(&server), &dir, &e, &Cancel::new()).is_ok());
    assert_eq!(dir.left(), ["zipped.toml", "zipped.wav"]);
}

#[test]
fn a_missing_zip_member_and_an_oversized_one_keep_nothing() {
    let body = zip_of("a.wav", &set_wav());
    let server = Server::start({
        let body = body.clone();
        move |_| Reply::Ok(body.clone())
    });
    let dir = Dir::new();
    let missing = entry(
        "zipped",
        "/z.zip",
        &body,
        Container::Zip { member: "b.wav" },
    );
    let error = run(&client(&server), &dir, &missing, &Cancel::new()).unwrap_err();
    assert_eq!(error.kind(), "archive");
    let tight = Client::new(
        Some(server.url.trim_end_matches("/results/").to_string()),
        Limits {
            member_bytes: 100,
            ..short()
        },
    );
    let big = entry(
        "zipped",
        "/z.zip",
        &body,
        Container::Zip { member: "a.wav" },
    );
    assert_eq!(
        run(&tight, &dir, &big, &Cancel::new()).unwrap_err().kind(),
        "too_large"
    );
    assert!(dir.left().is_empty(), "{:?}", dir.left());
}

#[test]
fn a_changed_source_is_a_refused_checksum_and_keeps_nothing() {
    let body = set_wav();
    let mut other = body.clone();
    *other.last_mut().unwrap() ^= 1;
    let server = Server::start(move |_| Reply::Ok(other.clone()));
    let dir = Dir::new();
    let e = entry("test-set", "/set", &body, Container::Sofa);
    let error = run(&client(&server), &dir, &e, &Cancel::new()).unwrap_err();
    assert_eq!(error.kind(), "checksum");
    assert!(
        error
            .to_string()
            .contains("the source changed; svolder needs an update")
    );
    assert!(dir.left().is_empty());
}

#[test]
fn cancelling_mid_download_keeps_nothing() {
    let size = 4 * 1024 * 1024;
    let server = Server::start(move |_| Reply::Flood(size));
    let dir = Dir::new();
    let mut e = entry("test-set", "/set", b"x", Container::Sofa);
    e.bytes = size as u64;
    let cancel = Cancel::new();
    let flag = cancel.clone();
    let mut chunks = 0;
    let error = client(&server)
        .fetch_entry(&Store::new(&dir.0), &e, 1, None, &cancel, &mut |_, _| {
            chunks += 1;
            if chunks == 2 {
                flag.cancel();
            }
        })
        .unwrap_err();
    assert_eq!(error, FetchError::Cancelled);
    assert_eq!(error.kind(), "cancelled");
    assert!(chunks >= 2);
    assert!(dir.left().is_empty());
}

#[test]
fn more_bytes_than_the_entry_pins_is_too_large() {
    let body = set_wav();
    let longer = [body.clone(), vec![0; 4096]].concat();
    let server = Server::start(move |_| Reply::Ok(longer.clone()));
    let dir = Dir::new();
    let e = entry("test-set", "/set", &body, Container::Sofa);
    assert_eq!(
        run(&client(&server), &dir, &e, &Cancel::new())
            .unwrap_err()
            .kind(),
        "too_large"
    );
    assert!(dir.left().is_empty());
}

#[test]
fn an_entry_over_the_download_cap_is_refused_without_a_request() {
    let server = Server::start(|_| Reply::Status(500));
    let dir = Dir::new();
    let mut e = entry("test-set", "/set", b"x", Container::Sofa);
    e.bytes = (8 << 20) + 1;
    assert_eq!(
        run(&client(&server), &dir, &e, &Cancel::new())
            .unwrap_err()
            .kind(),
        "too_large"
    );
    assert_eq!(server.hits(), 0);
    assert!(dir.left().is_empty());
}

#[test]
fn a_404_is_not_listed_and_another_error_or_silence_is_unreachable() {
    let body = set_wav();
    let dir = Dir::new();
    let e = entry("test-set", "/set", &body, Container::Sofa);
    let gone = Server::start(|_| Reply::Status(404));
    assert_eq!(
        run(&client(&gone), &dir, &e, &Cancel::new())
            .unwrap_err()
            .kind(),
        "not_listed"
    );
    let broken = Server::start(|_| Reply::Status(503));
    let error = run(&client(&broken), &dir, &e, &Cancel::new()).unwrap_err();
    assert_eq!(error.kind(), "unreachable");
    assert!(error.to_string().contains("503"));
    let silent = Server::start(|_| Reply::Silent);
    assert_eq!(
        run(&client(&silent), &dir, &e, &Cancel::new())
            .unwrap_err()
            .kind(),
        "unreachable"
    );
    assert!(dir.left().is_empty());
}

/// A fetch that fails says "the set's source", never the address of the file (whose name may
/// carry what a person is not to read), whether the source answers an error, stays silent or
/// redirects to a host the catalogue does not list.
#[test]
fn no_fetch_error_names_the_file_it_asked_for() {
    let body = set_wav();
    let dir = Dir::new();
    let e = entry("test-set", "/file-name-7/set.sofa", &body, Container::Sofa);
    for server in [
        Server::start(|_| Reply::Status(503)),
        Server::start(|_| Reply::Silent),
        Server::start(|_| Reply::Redirect("http://127.0.0.2:1/file-name-8/x.sofa".into())),
    ] {
        let words = run(&client(&server), &dir, &e, &Cancel::new())
            .unwrap_err()
            .to_string();
        assert!(words.contains("the set's source"), "{words}");
        assert!(!words.contains("file-name-"), "{words}");
    }
}

#[test]
fn a_download_the_converter_refuses_keeps_nothing() {
    let body = wav::write(48000, &[dirac(1, 1.0), dirac(1, 1.0)]);
    let server = Server::start({
        let body = body.clone();
        move |_| Reply::Ok(body.clone())
    });
    let dir = Dir::new();
    let e = entry("test-set", "/set", &body, Container::Sofa);
    let error = run(&client(&server), &dir, &e, &Cancel::new()).unwrap_err();
    assert_eq!(error.kind(), "unreadable");
    assert!(dir.left().is_empty());
}

#[test]
fn only_listed_ids_are_fetched_and_only_at_their_listed_path() {
    let server = Server::start(|_| Reply::Status(404));
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    let error = client(&server)
        .fetch(
            &store,
            "not-in-the-catalogue",
            1,
            None,
            &Cancel::new(),
            &mut |_, _| {},
        )
        .unwrap_err();
    assert_eq!(error, FetchError::NotListed("not-in-the-catalogue".into()));
    assert_eq!(server.hits(), 0, "an id that is not listed asks nothing");
    // A listed id goes to its own URL's path under the base, nothing the caller names.
    let error = client(&server)
        .fetch(
            &store,
            "mit-normal",
            1,
            None,
            &Cancel::new(),
            &mut |_, _| {},
        )
        .unwrap_err();
    assert_eq!(error.kind(), "not_listed");
    assert_eq!(
        server.requests()[0].split(' ').nth(1),
        Some("/data/database/mit/mit_kemar_normal_pinna.sofa")
    );
}

#[test]
fn the_discard_port_turns_fetching_off_without_a_request() {
    let dir = Dir::new();
    let e = entry("test-set", "/set", b"x", Container::Sofa);
    for client in [
        Client::new(Some("http://127.0.0.1:9/".into()), short()),
        Client::for_transport("fake", None),
        Client::for_transport("fake", Some(" ".into())),
    ] {
        assert!(client.is_off());
        assert_eq!(
            run(&client, &dir, &e, &Cancel::new()).unwrap_err(),
            FetchError::Off
        );
    }
    assert!(!Client::for_transport("bluez", None).is_off());
    assert_eq!(Client::for_transport("bluez", None).url_for(&e), e.url);
    assert!(!Client::for_transport("fake", Some("http://127.0.0.1:4000".into())).is_off());
    assert!(dir.left().is_empty());
}

#[test]
fn the_base_replaces_only_the_scheme_and_host() {
    let e = entry("x", "/a/b.sofa?c=1", b"x", Container::Sofa);
    let client = Client::new(Some("http://127.0.0.1:4000/".into()), short());
    assert_eq!(client.url_for(&e), "http://127.0.0.1:4000/a/b.sofa?c=1");
}

#[test]
fn a_redirect_to_the_same_host_is_followed() {
    let body = set_wav();
    let server = Server::start({
        let body = body.clone();
        move |path| match path {
            "/set" => Reply::Redirect("/moved/set".into()),
            "/moved/set" => Reply::Ok(body.clone()),
            _ => Reply::Status(404),
        }
    });
    let dir = Dir::new();
    let e = entry("test-set", "/set", &body, Container::Sofa);
    assert!(run(&client(&server), &dir, &e, &Cancel::new()).is_ok());
    assert_eq!(server.hits(), 2);
}

#[test]
fn a_redirect_to_a_host_the_catalogue_does_not_list_is_refused_unfollowed() {
    let body = set_wav();
    let server = Server::start(|_| Reply::Redirect("http://127.0.0.2:1/elsewhere".into()));
    let dir = Dir::new();
    let e = entry("test-set", "/set", &body, Container::Sofa);
    let error = run(&client(&server), &dir, &e, &Cancel::new()).unwrap_err();
    assert_eq!(error.kind(), "unreachable");
    assert!(error.to_string().contains("does not list"), "{error}");
    assert_eq!(server.hits(), 1);
    assert!(dir.left().is_empty());
}

#[test]
fn a_redirect_to_another_host_the_catalogue_lists_is_allowed_and_a_loop_is_not() {
    let body = set_wav();
    // The listed host is asked for the file, not the test server: only the policy is checked.
    let client = Client::new(None, short());
    let e = entry("test-set", "/set", &body, Container::Sofa);
    assert!(client.host_allowed(&e, "https://zenodo.org/records/1/files/x"));
    assert!(client.host_allowed(&e, "https://sofacoustics.org/a"));
    assert!(client.host_allowed(&e, "https://data.example.invalid/b"));
    assert!(!client.host_allowed(&e, "https://evil.example/a"));
    assert!(!client.host_allowed(&e, "https://sofacoustics.org.evil.example/a"));
    let server = Server::start(|_| Reply::Redirect("/set".into()));
    let dir = Dir::new();
    let error = run(&self::client(&server), &dir, &e, &Cancel::new()).unwrap_err();
    assert!(error.to_string().contains("too many redirects"), "{error}");
}

#[test]
fn the_catalogue_entries_are_not_fetched_by_the_tests() {
    // Every entry fetched through the test server's base only ever reaches loopback.
    for e in crate::entries() {
        assert!(
            Client::new(Some("http://127.0.0.1:9".into()), short())
                .url_for(e)
                .starts_with("http://127.0.0.1:9/")
        );
    }
}
