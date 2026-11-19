// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The stub daemon itself: a stub that cannot serve fails the case that asked for it.

use std::time::Duration;

use super::stub_daemon_at;

/// A socket path one byte longer than a Unix socket can hold puts the stub's bind out of
/// reach; the stub says so at once, well inside its 5 s wait for an answer, instead of
/// handing the case a daemon that never answers.
#[tokio::test]
async fn a_stub_that_cannot_bind_its_socket_fails_at_once_and_says_why() {
    let socket = std::env::temp_dir().join("s".repeat(108));
    let started = tokio::spawn(stub_daemon_at(socket));
    let ended = tokio::time::timeout(Duration::from_secs(2), started)
        .await
        .expect("the stub gave up at once");
    let Err(failed) = ended else {
        panic!("a stub that cannot serve fails");
    };
    let panicked = failed.into_panic();
    let said = panicked
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        said.contains("the stub daemon could not serve on") && said.contains("SUN_LEN"),
        "{said}"
    );
}
