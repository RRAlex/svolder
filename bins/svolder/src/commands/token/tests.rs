// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder mcp token` over a stub daemon and a temporary state home of each case's own. A
//! token is compared with `assert!(a == b)` and never formatted.

use svolder_ipc::{TokenInfo, TokenKind};

use super::*;
use crate::test_support::stub_daemon::{SocketFile, StubServer, fresh_socket, stub_daemon};

/// A state home of this case's own, with no token in it, removed on drop.
struct Home(PathBuf);

impl Home {
    fn new() -> (Home, Site) {
        let dir = std::env::temp_dir().join(format!(
            "svolder-token-cmd-{}-{}",
            std::process::id(),
            fresh_socket()
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("x")
        ));
        std::fs::create_dir_all(&dir).expect("a scratch home");
        let site = Site {
            env: None,
            state_home: dir.clone(),
        };
        (Home(dir), site)
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What a daemon that uses `token` as `source` says.
fn uses(token: &Token) -> TokenInfo {
    token.info()
}

fn texts(done: &Done) -> String {
    done.notes.join("\n")
}

#[tokio::test]
async fn a_rotation_has_the_daemon_reload_and_says_it_accepts_only_the_new_token() {
    let (_home, site) = Home::new();
    let daemon = stub_daemon().await;
    let first =
        token::resolve_with(token::Store::MCP, None, None, &site.state_home).expect("a token");
    daemon.says_token(Some(uses(&first)));

    let done = run(&site, Store::MCP, None, &daemon.socket, true)
        .await
        .expect("a rotation");

    assert!(done.token != first.value());
    assert_eq!(daemon.reloads(), 1);
    assert_eq!(done.notes, ["the daemon now accepts only the new token"]);
}

#[tokio::test]
async fn a_rotation_with_no_daemon_says_none_uses_the_token_yet() {
    let (_home, site) = Home::new();
    let none = fresh_socket();

    let done = run(&site, Store::MCP, None, &none, true)
        .await
        .expect("a rotation");

    assert_eq!(done.token.len(), 43);
    assert_eq!(
        done.notes,
        ["no daemon is running, so none uses the new token yet"]
    );
}

#[tokio::test]
async fn a_rotation_the_daemon_refuses_to_reload_names_the_restart() {
    let (_home, site) = Home::new();
    let daemon = stub_daemon().await;
    daemon.refuses_reloads("the config is not readable");

    let done = run(&site, Store::MCP, None, &daemon.socket, true)
        .await
        .expect("a rotation");

    let words = texts(&done);
    assert!(words.contains("did not reload"), "{words}");
    assert!(words.contains("the config is not readable"), "{words}");
    assert!(words.contains("svolder daemon restart"), "{words}");
}

#[tokio::test]
async fn printing_warns_when_the_daemon_uses_another_token() {
    let (_home, site) = Home::new();
    let daemon = stub_daemon().await;
    daemon.says_token(Some(TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: "9xZ".into(),
    }));

    let done = run(&site, Store::MCP, None, &daemon.socket, false)
        .await
        .expect("a token");

    let words = texts(&done);
    assert!(
        words.contains("warning: the daemon uses another token"),
        "{words}"
    );
    assert!(
        words.contains("in the daemon's environment (SVOLDER_MCP_TOKEN)"),
        "{words}"
    );
    assert!(!words.contains(&done.token));
}

#[tokio::test]
async fn printing_says_nothing_when_the_daemon_uses_this_token_or_none() {
    let (_home, site) = Home::new();
    let daemon = stub_daemon().await;
    let own =
        token::resolve_with(token::Store::MCP, None, None, &site.state_home).expect("a token");
    for says in [Some(uses(&own)), None] {
        daemon.says_token(says);
        let done = run(&site, Store::MCP, None, &daemon.socket, false)
            .await
            .expect("a token");
        assert!(done.token == own.value());
        assert!(done.notes.is_empty(), "{:?}", done.notes);
    }
    // No daemon at all says nothing either.
    let done = run(&site, Store::MCP, None, &fresh_socket(), false)
        .await
        .expect("a token");
    assert!(done.notes.is_empty());
}

#[tokio::test]
async fn a_rotation_refuses_when_the_daemon_uses_another_token() {
    let (_home, site) = Home::new();
    let daemon: StubServer = stub_daemon().await;
    let own =
        token::resolve_with(token::Store::MCP, None, None, &site.state_home).expect("a token");
    daemon.says_token(Some(TokenInfo {
        kind: TokenKind::State,
        path: Some("/var/lib/svolder/mcp-token".into()),
        tail: token::tail(own.value()),
    }));

    let refused = run(&site, Store::MCP, None, &daemon.socket, true)
        .await
        .err();

    let words = refused.expect("a refusal");
    assert!(words.contains("/var/lib/svolder/mcp-token"), "{words}");
    assert_eq!(daemon.reloads(), 0);
    let kept =
        token::resolve_with(token::Store::MCP, None, None, &site.state_home).expect("a token");
    assert!(kept.value() == own.value(), "the token was replaced");
}

/// The daemon names the file this command reads but has another tail: it has not read the file
/// again. Printing says so, and a rotation goes ahead and has the daemon reload.
#[tokio::test]
async fn a_daemon_that_has_not_read_the_file_again_is_told_so_and_a_rotation_goes_ahead() {
    let (_home, site) = Home::new();
    let daemon = stub_daemon().await;
    let own =
        token::resolve_with(token::Store::MCP, None, None, &site.state_home).expect("a token");
    let mut stale = uses(&own);
    stale.tail = "aaa".into();
    daemon.says_token(Some(stale));

    let done = run(&site, Store::MCP, None, &daemon.socket, false)
        .await
        .expect("a token");
    let words = texts(&done);
    assert!(words.contains("has not read"), "{words}");
    assert!(
        words.contains("svolder daemon restart (or SIGHUP)"),
        "{words}"
    );
    assert!(!words.contains("another token"), "{words}");

    let done = run(&site, Store::MCP, None, &daemon.socket, true)
        .await
        .expect("a rotation");
    assert!(done.token != own.value());
    assert_eq!(daemon.reloads(), 1);
}

/// A daemon that takes the connection and never answers is read as one too old to say, and the
/// ask ends.
#[tokio::test(start_paused = true)]
async fn a_daemon_that_never_answers_is_given_up_on() {
    let socket = SocketFile(fresh_socket());
    // A listener that never accepts: the kernel takes the connection, nothing ever answers it.
    // The clock is paused, so the wait costs no real time; the outer bound is past the ask's own.
    // While the connect is still pending the idle clock jumps to the nearest timer, which would
    // be that outer bound alone, so a nearer one is kept running beside it.
    let listener = std::os::unix::net::UnixListener::bind(&*socket).expect("the socket binds");
    let nearer = tokio::spawn(tokio::time::sleep(std::time::Duration::from_secs(1)));
    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        ask_daemon(&socket, Store::MCP),
    )
    .await
    .expect("the ask ends");
    assert!(matches!(answer, Daemon::Answered(None)));
    nearer.abort();
    drop(listener);
}

/// `web token` resolves the web store's own file and compares it with the daemon's web token,
/// never the MCP one.
#[tokio::test]
async fn the_web_token_is_its_own_store_and_compared_with_the_daemons_web_token() {
    let (_home, site) = Home::new();
    let daemon = stub_daemon().await;
    let mcp = token::resolve_with(Store::MCP, None, None, &site.state_home).expect("an MCP token");
    daemon.says_token(Some(uses(&mcp)));
    let done = run(&site, Store::WEB, None, &daemon.socket, false)
        .await
        .expect("a web token");
    assert!(
        done.token != mcp.value(),
        "the web command printed the MCP token"
    );
    assert!(site.state_home.join("svolder").join("web-token").exists());
    assert!(done.notes.is_empty(), "{}", texts(&done));
    let web = token::resolve_with(Store::WEB, None, None, &site.state_home).unwrap();
    daemon.says_web(None, Some(uses(&web)));
    let again = run(&site, Store::WEB, None, &daemon.socket, false)
        .await
        .unwrap();
    assert!(again.token == web.value());
    assert!(again.notes.is_empty(), "{}", texts(&again));
}
