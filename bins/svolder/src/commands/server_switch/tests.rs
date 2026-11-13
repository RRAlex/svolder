// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `web on|off` and `mcp on|off` over a stub daemon and a configuration file of each case's
//! own: who writes the file, and what the verb says.

use svolder_ipc::{
    CODE_USAGE, Client, Config, FEATURE_SERVER_SWITCH, McpStatus, Reason, Response, WebStatus,
};

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon_at};
use crate::test_support::temp_dir;

/// A socket path short enough for any `TMPDIR`: a socket's path has about a hundred bytes, and a
/// case whose stub could not be reached there would be a case about no daemon at all.
fn short_socket() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .subsec_nanos();
    std::path::PathBuf::from(format!(
        "/tmp/svk1-{}-{seq}-{nanos:x}.sock",
        std::process::id()
    ))
}

/// A stub daemon on a short socket that a client reaches: a case that starts a daemon fails
/// here, loudly, when nothing answers where it listens.
async fn stub_daemon() -> StubServer {
    let daemon = stub_daemon_at(short_socket()).await;
    Client::connect(&daemon.socket)
        .await
        .expect("the stub daemon answers on its socket");
    daemon
}

/// A socket path no daemon listens on.
fn fresh_socket() -> std::path::PathBuf {
    short_socket()
}

/// With a daemon that has the request, the daemon makes the switch: one request, nothing
/// written by this side, and the line is where the server now stands.
#[tokio::test]
async fn with_a_daemon_the_switch_is_one_request_and_this_side_writes_nothing() {
    let dir = temp_dir("switch-daemon");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_SERVER_SWITCH]);

    let done = switch(&daemon.socket, &own, Server::Web, true)
        .await
        .expect("web on");
    assert_eq!(
        done.line,
        "web: listening on 127.0.0.1:8738 (token, writes)"
    );
    assert_eq!(done.notes, Vec::<String>::new());
    let done = switch(&daemon.socket, &own, Server::Mcp, true)
        .await
        .expect("mcp on");
    assert_eq!(
        done.line,
        "mcp: listening on 127.0.0.1:8737 (token, read-only)"
    );
    let done = switch(&daemon.socket, &own, Server::Web, false)
        .await
        .expect("web off");
    assert_eq!(done.line, "web: off");
    assert_eq!(done.notes, Vec::<String>::new());

    assert_eq!(
        daemon.switches(),
        [
            (Server::Web, true),
            (Server::Mcp, true),
            (Server::Web, false)
        ]
    );
    assert_eq!(daemon.reloads(), 0, "the daemon reloads itself");
    assert!(!own.exists(), "the daemon is the writer");
}

/// With no daemon the verb writes the file itself, and says that nothing runs yet.
#[tokio::test]
async fn with_no_daemon_the_verb_writes_the_file_itself() {
    let dir = temp_dir("switch-alone");
    let own = dir.0.join("config.toml");
    let none = fresh_socket();

    let done = switch(&none, &own, Server::Web, true)
        .await
        .expect("web on");
    assert_eq!(done.line, format!("web: on in {}", own.display()));
    assert_eq!(
        done.notes,
        [format!(
            "nothing answers on {}: the web UI starts with the next daemon",
            none.display()
        )]
    );
    assert!(
        Config::load_from(&own)
            .expect("the file parses")
            .web
            .enabled
    );

    let done = switch(&none, &own, Server::Mcp, true)
        .await
        .expect("mcp on");
    assert_eq!(done.line, format!("mcp: on in {}", own.display()));
    assert_eq!(
        done.notes,
        [format!(
            "nothing answers on {}: the MCP server starts with the next daemon",
            none.display()
        )]
    );
    let done = switch(&none, &own, Server::Web, false)
        .await
        .expect("web off");
    assert_eq!(done.line, format!("web: off in {}", own.display()));
    assert_eq!(
        done.notes,
        [format!("nothing answers on {}", none.display())]
    );
    let config = Config::load_from(&own).expect("the file parses");
    assert!(!config.web.enabled && config.mcp.enabled);
}

/// A daemon too old to have the request is not written around: the verb says to restart it,
/// sends no switch and leaves the file alone.
#[tokio::test]
async fn a_daemon_without_the_request_is_named_and_nothing_is_written() {
    let dir = temp_dir("switch-old");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;

    let said = switch(&daemon.socket, &own, Server::Web, true)
        .await
        .expect_err("refused");
    assert_eq!(
        said.to_string(),
        "the daemon (9.9.9) cannot switch its servers, which this svolder asks it to. Run \
         `svolder daemon restart`"
    );
    assert!(
        matches!(&said, Failed::TooOld(failure) if failure.code == 7),
        "a daemon too old exits 7, not 1: {said}"
    );
    assert_eq!(daemon.switches(), []);
    assert!(!own.exists());
}

/// The daemon's refusal is the verb's failure, in the daemon's words.
#[tokio::test]
async fn a_refusal_of_the_daemon_fails_the_verb_in_its_words() {
    let dir = temp_dir("switch-refused");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_SERVER_SWITCH]);
    daemon.answers_switch(
        Response::error(
            CODE_USAGE,
            "Saving the server switch is off: /config/svolder/config.toml is read-only".to_string(),
        )
        .with_reason(Reason::ReadOnly),
    );

    let said = switch(&daemon.socket, &own, Server::Mcp, true)
        .await
        .expect_err("refused");
    assert_eq!(
        said.to_string(),
        "Saving the server switch is off: /config/svolder/config.toml is read-only"
    );
    assert!(!own.exists());
}

/// A server turned on that cannot listen fails the verb with the daemon's line and what is now
/// in the file; the other server's state is not this verb's business.
#[tokio::test]
async fn a_server_that_cannot_listen_fails_the_verb() {
    let dir = temp_dir("switch-failed");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_SERVER_SWITCH]);
    daemon.answers_switch(Response::ServerSwitch {
        web: WebStatus::Failed {
            listen: ([0, 0, 0, 0], 8738).into(),
            error: "address in use".to_string(),
        },
        mcp: McpStatus::Off,
    });

    let said = switch(&daemon.socket, &own, Server::Web, true)
        .await
        .expect_err("cannot listen");
    assert_eq!(
        said.to_string(),
        "web: cannot listen on 0.0.0.0:8738: address in use ([web] enabled = true is written; \
         `svolder web off` takes it back)"
    );
    let done = switch(&daemon.socket, &own, Server::Mcp, false)
        .await
        .expect("mcp off");
    assert_eq!(done.line, "mcp: off");
}

/// An answer that still shows the server as it was (the reload failed, or outlasted the
/// write's wait) fails the verb as a server that cannot listen does: one rule, the verb succeeds
/// only when the server stands as asked.
#[tokio::test]
async fn an_answer_that_leaves_the_server_as_it_was_fails_the_verb() {
    let dir = temp_dir("switch-late");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_SERVER_SWITCH]);
    daemon.switches_late();

    let said = switch(&daemon.socket, &own, Server::Web, true)
        .await
        .expect_err("still off");
    assert_eq!(
        said.to_string(),
        "web: off ([web] enabled = true is written and the daemon has not acted on it: svolder \
         daemon status)"
    );
    daemon.says_web(
        Some(WebStatus::Listening {
            listen: ([127, 0, 0, 1], 8738).into(),
            auth: svolder_ipc::config::WebAuth::Token,
            writes: true,
            guest: svolder_ipc::config::WebGuest::None,
        }),
        None,
    );
    let said = switch(&daemon.socket, &own, Server::Web, false)
        .await
        .expect_err("still on");
    assert_eq!(
        said.to_string(),
        "web: listening on 127.0.0.1:8738 (token, writes) ([web] enabled = false is written and \
         the daemon has not acted on it: svolder daemon status)"
    );
}

/// A socket that cannot be reached for another reason than "nothing is there" is not read as
/// no daemon: a daemon may run behind it, so the verb fails naming the socket and writes
/// nothing. Here the path is longer than a socket's path may be.
#[tokio::test]
async fn a_socket_that_cannot_be_tried_fails_the_verb_and_writes_nothing() {
    let dir = temp_dir("switch-unreachable");
    let own = dir.0.join("config.toml");
    let long = dir.0.join(format!("{}.sock", "s".repeat(200)));

    let said = switch(&long, &own, Server::Web, true)
        .await
        .expect_err("not read as no daemon");
    assert!(
        said.to_string().starts_with(&format!(
            "cannot reach the daemon's socket {}: ",
            long.display()
        )),
        "{said}"
    );
    assert!(!own.exists(), "nothing is written");
}

/// A socket this user may not open is the same: a daemon of another user may hold it.
#[tokio::test]
async fn a_socket_this_user_may_not_open_fails_the_verb_and_writes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("switch-denied");
    let own = dir.0.join("config.toml");
    let locked = dir.0.join("locked");
    std::fs::create_dir(&locked).expect("a directory");
    let socket = locked.join("s.sock");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    // Root passes every permission check, so there is nothing to see as root.
    let denied =
        std::fs::metadata(&socket).is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied);
    let said = switch(&socket, &own, Server::Mcp, true).await;
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    if !denied {
        println!("SKIP: this user passes the permission check (root)");
        return;
    }
    let said = said.expect_err("not read as no daemon");
    assert!(
        said.to_string()
            .contains("cannot reach the daemon's socket"),
        "{said}"
    );
    assert!(!own.exists(), "nothing is written");
}
