// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's answers to `host_cover`: the current hash, a stale one, no art and a picture past
//! the bound, over fake players so no session bus or file is ever read.

use svolder_host::NowPlaying;
use svolder_host::cover::{Cover, MAX_COVER_BYTES};
use svolder_host::fake::FakePlayers;
use svolder_ipc::host::HostView;
use svolder_ipc::{COVER_CHUNK_BYTES, CoverReader, FEATURE_HOST_COVER, FEATURES};

use super::tests::daemon;
use super::*;

/// A daemon whose player names one cover address and whose cover reader answers `cover` for it,
/// and how many times the reader has been asked.
fn daemon_counting(cover: Option<Cover>) -> (Daemon, Arc<std::sync::atomic::AtomicUsize>) {
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = Arc::clone(&reads);
    let daemon = daemon();
    let players = FakePlayers::new(Some(NowPlaying {
        title: "Song".to_string(),
        identity: "Fake Player".to_string(),
        art_url: Some("file:///cover".to_string()),
        ..NowPlaying::default()
    }));
    daemon.host().run_with(crate::host::Parts {
        connect: crate::host::found(Arc::new(players)),
        routes: None,
        covers: Arc::new(move |_| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            cover.clone()
        }),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
        computer: None,
    });
    (daemon, reads)
}

/// [`daemon_counting`] without the count.
fn daemon_with(cover: Option<Cover>) -> Daemon {
    daemon_counting(cover).0
}

/// The hash the host view names the cover with, once the cover is read.
async fn cover_hash(daemon: &Daemon) -> String {
    let started = std::time::Instant::now();
    loop {
        for _ in 0..20 {
            tokio::task::yield_now().await;
            std::thread::sleep(Duration::from_millis(1));
        }
        let Response::Host(HostView { playing, .. }) = daemon.handle(Request::Host).await else {
            panic!("a host answer");
        };
        if let Some(hash) = playing.and_then(|p| p.cover) {
            return hash;
        }
        assert!(started.elapsed() < Duration::from_secs(1), "no cover read");
    }
}

fn ask(hash: &str, offset: u64) -> Request {
    Request::HostCover {
        hash: hash.to_string(),
        offset,
    }
}

fn picture(len: usize) -> Cover {
    Cover {
        bytes: (0..len).map(|i| (i * 13 + i / 7) as u8).collect(),
        mime: "image/png",
    }
}

/// The daemon lists the request.
#[test]
fn the_daemon_lists_host_cover() {
    assert!(FEATURES.contains(&FEATURE_HOST_COVER));
}

/// The current hash gets the picture, in as many chunks as it takes, and they put together are
/// the picture the daemon read.
#[tokio::test(start_paused = true)]
async fn the_current_hash_gets_the_cover_in_chunks() {
    let cover = picture(COVER_CHUNK_BYTES * 2 + 100);
    let daemon = daemon_with(Some(cover.clone()));
    let hash = cover_hash(&daemon).await;
    let mut reader = CoverReader::new(&hash);
    let mut asks = 0;
    let got = loop {
        let answer = daemon.handle(ask(&hash, reader.next_offset())).await;
        asks += 1;
        if let Some(done) = reader.push(answer).expect("a chunk") {
            break done;
        }
    };
    assert_eq!((asks, got), (3, cover));
}

/// A hash that is not the current one, whether an earlier cover's or made up, gets not found.
#[tokio::test(start_paused = true)]
async fn another_hash_is_not_found() {
    let daemon = daemon_with(Some(picture(1000)));
    let hash = cover_hash(&daemon).await;
    for other in ["0000000000000000", "", &hash[..8], &format!("{hash}0")] {
        let Response::Error { code, .. } = daemon.handle(ask(other, 0)).await else {
            panic!("{other:?} is not the current hash");
        };
        assert_eq!(code, svolder_ipc::CODE_NOT_FOUND, "{other:?}");
    }
}

/// With no art (no player, or a player whose cover cannot be read) no hash is served.
#[tokio::test(start_paused = true)]
async fn no_art_serves_no_hash() {
    let (daemon, reads) = daemon_counting(None);
    let started = std::time::Instant::now();
    while reads.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(1));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the cover was never looked for"
        );
    }
    let Response::Host(view) = daemon.handle(Request::Host).await else {
        panic!("a host answer");
    };
    assert_eq!(view.playing.and_then(|p| p.cover), None);
    let Response::Error { code, .. } = daemon.handle(ask("0123456789abcdef", 0)).await else {
        panic!("no cover to serve");
    };
    assert_eq!(code, svolder_ipc::CODE_NOT_FOUND);

    // A daemon whose host is off answers the same.
    let off = super::tests::daemon();
    let Response::Error { code, .. } = off.handle(ask("0123456789abcdef", 0)).await else {
        panic!("no host, no cover");
    };
    assert_eq!(code, svolder_ipc::CODE_NOT_FOUND);
}

/// A picture past the bound is never sent, even though its hash is the current one; an offset
/// past the end is a usage error.
#[tokio::test(start_paused = true)]
async fn an_oversize_cover_is_refused_and_so_is_an_offset_past_the_end() {
    let daemon = daemon_with(Some(picture(MAX_COVER_BYTES as usize + 1)));
    let hash = cover_hash(&daemon).await;
    let Response::Error { code, message, .. } = daemon.handle(ask(&hash, 0)).await else {
        panic!("too large to send");
    };
    assert_eq!(code, svolder_ipc::CODE_NOT_FOUND, "{message}");

    let daemon = daemon_with(Some(picture(10)));
    let hash = cover_hash(&daemon).await;
    let Response::Error { code, .. } = daemon.handle(ask(&hash, 10)).await else {
        panic!("past the end");
    };
    assert_eq!(code, svolder_ipc::CODE_USAGE);
}
