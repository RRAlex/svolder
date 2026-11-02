// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the correction comes from.

use svolder_autoeq::{Client, Limits};

use crate::tui::screenshots::correction_from;

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("svolder-shots-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

#[test]
fn a_cached_correction_is_read_without_the_network() {
    let dir = scratch("cached");
    std::fs::write(
        dir.join("Sennheiser HD 800 S ParametricEQ.txt"),
        "Preamp: -3.0 dB\nFilter 1: ON LSC Fc 105 Hz Gain 4.0 dB Q 0.70\n",
    )
    .expect("written");
    let correction =
        correction_from(&dir, || panic!("no client is made for a cached file")).expect("read");
    assert_eq!(correction.filters.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_unreachable_download_says_where_to_put_the_file() {
    let dir = scratch("offline");
    let why = correction_from(&dir, || {
        Client::new(
            "http://127.0.0.1:9/".into(),
            dir.join("cache"),
            Limits::default(),
        )
    })
    .expect_err("nothing listens on port 9");
    assert!(
        why.contains("Sennheiser HD 800 S ParametricEQ.txt") && why.contains("127.0.0.1:9"),
        "{why}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Serves `body` once over HTTP on a local port, whatever is asked; the base URL to ask it at.
fn serve_once(body: &'static str) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let base = format!("http://{}/", listener.local_addr().expect("an address"));
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    base
}

#[test]
fn a_downloaded_correction_is_cached_once_it_parses() {
    let dir = scratch("good");
    let base = serve_once("Preamp: -3.0 dB\nFilter 1: ON LSC Fc 105 Hz Gain 4.0 dB Q 0.70\n");
    let correction = correction_from(&dir, || {
        Client::new(base, dir.join("cache"), Limits::default())
    })
    .expect("downloaded");
    assert_eq!(correction.filters.len(), 1);
    assert!(dir.join("Sennheiser HD 800 S ParametricEQ.txt").is_file());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_downloaded_body_that_does_not_parse_is_never_cached() {
    let dir = scratch("bad");
    let base = serve_once("Filter 1: ON LSC Fc loud Gain a lot\n<html>not a correction</html>\n");
    let why = correction_from(&dir, || {
        Client::new(base, dir.join("cache"), Limits::default())
    })
    .expect_err("a body that is no correction");
    assert!(
        why.contains("Sennheiser HD 800 S ParametricEQ.txt"),
        "{why}"
    );
    assert!(!dir.join("Sennheiser HD 800 S ParametricEQ.txt").exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_cache_path_that_cannot_be_read_is_an_error_not_a_miss() {
    let dir = scratch("unreadable");
    // A directory where the file belongs: reading it fails, but not with NotFound.
    std::fs::create_dir_all(dir.join("Sennheiser HD 800 S ParametricEQ.txt")).expect("made");
    let why =
        correction_from(&dir, || panic!("a read error is no cache miss")).expect_err("unreadable");
    assert!(
        why.contains("Sennheiser HD 800 S ParametricEQ.txt"),
        "{why}"
    );
    let _ = std::fs::remove_dir_all(dir);
}
