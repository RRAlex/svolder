// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Binding, replacing a stale socket file and reading the socket-activation environment.

use super::*;
use crate::socket::protocol::*;
use crate::socket::server::serve;
use crate::test_support::{Fake, start_server, start_server_at, temp_socket};
use std::time::Duration;

#[tokio::test]
async fn a_second_bind_is_refused_while_the_first_lives() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let server = start_server(Arc::new(Fake::new())).await;
        let second = serve(&server.path, Arc::new(Fake::new()), std::future::pending()).await;
        assert!(
            second.is_err(),
            "second serve must refuse while the first holds the socket"
        );
    })
    .await
    .expect("test timed out");
}

/// A bind in a directory that is not there says so in the socket's own words: the daemon that
/// cannot start is not "stopped".
#[tokio::test]
async fn a_bind_in_a_missing_directory_keeps_its_own_words() {
    let path = temp_socket().join("missing").join("dir").join("s.sock");
    let err = bind_listener(&path)
        .await
        .expect_err("no directory to bind in");
    assert!(matches!(err, IpcError::Io(_)), "{err:?}");
    assert!(!err.daemon_gone());
    assert!(!err.to_string().contains("stopped"), "{err}");
}

#[tokio::test]
async fn stale_socket_file_is_replaced() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let socket = crate::test_support::TempPath(temp_socket());
        let path = socket.0.clone();
        crate::socket::path::ensure_parent(&path).unwrap();
        // Bind and drop a listener: the socket file stays behind with nothing serving it.
        let listener = UnixListener::bind(&path).unwrap();
        drop(listener);
        assert!(path.exists());

        // The fixture's own existence poll sees the stale file immediately, so retry the
        // connect itself until `serve` has cleared it and is actually listening.
        let server = start_server_at(path, Arc::new(Fake::new())).await;
        for _ in 0..50 {
            if let Ok(mut c) = crate::socket::client::Client::connect(&server.path).await {
                let resp = c.request(Request::State { device: None }).await.unwrap();
                assert!(matches!(resp, Response::Values { values, .. } if values.len() == 1));
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("server never came up on the stale socket path");
    })
    .await
    .expect("test timed out");
}

/// A socket this call binds itself is forced to 0600 rather than left at whatever the
/// umask gives it, on both the fresh-bind path and the stale-socket-replaced one: a
/// directory shared with other users (a bind-mounted `./run`, unlike `$XDG_RUNTIME_DIR`'s
/// own 0700) must not leave the daemon's socket open to them either way.
#[tokio::test]
async fn bind_forces_the_socket_to_0600() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;

        let fresh = start_server(Arc::new(Fake::new())).await;
        assert_eq!(mode(&fresh.path), 0o600, "a fresh bind");

        let socket = crate::test_support::TempPath(temp_socket());
        let path = socket.0.clone();
        crate::socket::path::ensure_parent(&path).unwrap();
        drop(UnixListener::bind(&path).unwrap());
        let replaced = start_server_at(path, Arc::new(Fake::new())).await;
        for _ in 0..50 {
            if crate::socket::client::Client::connect(&replaced.path)
                .await
                .is_ok()
            {
                assert_eq!(mode(&replaced.path), 0o600, "a stale socket replaced");
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("server never came up on the stale socket path");
    })
    .await
    .expect("test timed out");
}

/// The socket file is this listener's to remove once its accept loop ends, so the path is
/// free for the next daemon that binds it rather than left claiming an inode nobody serves.
#[tokio::test]
async fn serve_removes_its_socket_file_when_it_stops() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let path = temp_socket();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let p2 = path.clone();
        let join = tokio::spawn(async move {
            serve(&p2, Arc::new(Fake::new()), async {
                let _ = rx.await;
            })
            .await
        });
        for _ in 0..50 {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(path.exists(), "the server never bound");
        tx.send(()).expect("the server is still waiting on it");
        join.await
            .expect("the server task did not panic")
            .expect("serve ended cleanly");
        assert!(!path.exists(), "the socket file was left behind");
    })
    .await
    .expect("test timed out");
}

/// What the socket-activation environment has to say before a descriptor is adopted.
/// `tests/adoption.rs` drives the descriptor itself; this is the reading of the two
/// variables, which needs no descriptor and no environment of its own.
#[test]
fn a_handover_is_one_socket_addressed_to_this_process() {
    assert!(handed_over(Some("1"), None, 42));
    assert!(handed_over(Some("1"), Some("42"), 42));
    assert!(handed_over(Some(" 1 "), Some(" 42 "), 42));

    assert!(!handed_over(None, None, 42), "no handover at all");
    assert!(!handed_over(Some("0"), None, 42), "no socket passed");
    assert!(!handed_over(Some("2"), None, 42), "more sockets than one");
    assert!(!handed_over(Some(""), None, 42), "a blank count");
    assert!(!handed_over(Some("many"), None, 42), "an unreadable count");
    assert!(
        !handed_over(Some("1"), Some("43"), 42),
        "a handover addressed to another process"
    );
    assert!(
        !handed_over(Some("1"), Some("pid"), 42),
        "an unreadable process id"
    );
}
