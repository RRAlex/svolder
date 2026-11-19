// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `web on|off` and `mcp on|off` over a stub daemon and a configuration file of each case's
//! own: who writes the file, and what the verb says.

use svolder_ipc::{
    CODE_USAGE, Client, Config, FEATURE_GUEST_SWITCH, FEATURE_SERVER_SWITCH, McpStatus, Reason,
    Response, WebGuest, WebStatus,
};

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon_at};
use crate::test_support::{short_dir, temp_dir};

/// A socket path short enough for any `TMPDIR`: a socket's path has about a hundred bytes, and a
/// case whose stub could not be reached there would be a case about no daemon at all.
fn short_socket() -> std::path::PathBuf {
    crate::test_support::short_socket("svk1")
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
    let dir = short_dir("switch-unreachable");
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

/// With a daemon that has the request, the guest switch is one `guest_switch` request and this
/// side writes nothing; the line is the web UI's, naming the guest mode.
#[tokio::test]
async fn the_guest_verb_with_a_daemon_is_one_request_and_writes_nothing() {
    let dir = temp_dir("guest-daemon");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_GUEST_SWITCH]);
    daemon.says_web(
        Some(WebStatus::Listening {
            listen: ([127, 0, 0, 1], 8738).into(),
            auth: svolder_ipc::config::WebAuth::Token,
            writes: true,
            guest: WebGuest::None,
        }),
        None,
    );

    let done = guest_switch(&daemon.socket, &own, true).await.expect("on");
    assert_eq!(
        done.line,
        "web: listening on 127.0.0.1:8738 (token, writes, guest read)"
    );
    assert_eq!(done.notes, Vec::<String>::new());
    let done = guest_switch(&daemon.socket, &own, false)
        .await
        .expect("off");
    assert_eq!(
        done.line,
        "web: listening on 127.0.0.1:8738 (token, writes)"
    );
    assert_eq!(daemon.guest_switches(), [true, false]);
    assert_eq!(daemon.switches(), [], "not a server switch");
    assert_eq!(daemon.reloads(), 0);
    assert!(!own.exists(), "the daemon is the writer");
}

/// With no daemon the verb writes `guest` in its own file and says that nothing runs yet.
#[tokio::test]
async fn the_guest_verb_with_no_daemon_writes_the_file_itself() {
    let dir = temp_dir("guest-alone");
    let own = dir.0.join("config.toml");
    let none = fresh_socket();

    let done = guest_switch(&none, &own, true).await.expect("on");
    assert_eq!(done.line, format!("web guest: on in {}", own.display()));
    assert_eq!(
        done.notes,
        [format!(
            "nothing answers on {}: the guest mode applies with the next daemon",
            none.display()
        )]
    );
    assert_eq!(
        Config::load_from(&own).expect("parses").web.guest,
        WebGuest::Read
    );
    guest_switch(&none, &own, false).await.expect("off");
    assert_eq!(
        Config::load_from(&own).expect("parses").web.guest,
        WebGuest::None
    );
}

/// A daemon without `guest_switch` (one that has only the server switch) is named, exits 7,
/// and nothing is sent or written.
#[tokio::test]
async fn the_guest_verb_names_a_daemon_without_the_request() {
    let dir = temp_dir("guest-old");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_SERVER_SWITCH]);

    let said = guest_switch(&daemon.socket, &own, true)
        .await
        .expect_err("refused");
    assert_eq!(
        said.to_string(),
        "the daemon (9.9.9) cannot switch the guest view, which this svolder asks it to. Run \
         `svolder daemon restart`"
    );
    assert!(
        matches!(&said, Failed::TooOld(failure) if failure.code == 7),
        "{said}"
    );
    assert_eq!(daemon.guest_switches(), Vec::<bool>::new());
    assert!(!own.exists());
}

/// The daemon's refusal is the failure in its words; an answer that leaves the guest mode as it
/// was fails; a web UI that is off succeeds with a note, since the key is written.
#[tokio::test]
async fn the_guest_verb_judges_where_the_web_ui_stands() {
    let dir = temp_dir("guest-judged");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_GUEST_SWITCH]);

    daemon.says_web(Some(WebStatus::Off), None);
    let done = guest_switch(&daemon.socket, &own, true)
        .await
        .expect("off page");
    assert_eq!(done.line, "web: off");
    assert_eq!(
        done.notes,
        [
            "[web] guest = \"read\" is written; the web UI is off, so no one sees it until `svolder web on`"
        ]
    );

    daemon.switches_late();
    daemon.says_web(
        Some(WebStatus::Listening {
            listen: ([127, 0, 0, 1], 8738).into(),
            auth: svolder_ipc::config::WebAuth::Token,
            writes: true,
            guest: WebGuest::None,
        }),
        None,
    );
    let said = guest_switch(&daemon.socket, &own, true)
        .await
        .expect_err("still none");
    assert_eq!(
        said.to_string(),
        "web: listening on 127.0.0.1:8738 (token, writes) ([web] guest = \"read\" is written \
         and the daemon has not acted on it: svolder daemon status)"
    );

    daemon.answers_switch(
        Response::error(CODE_USAGE, "Saving the server switch is off: read-only")
            .with_reason(Reason::ReadOnly),
    );
    let said = guest_switch(&daemon.socket, &own, true)
        .await
        .expect_err("refused");
    assert_eq!(
        said.to_string(),
        "Saving the server switch is off: read-only"
    );
    assert!(!own.exists());
}

/// A daemon that started just before the no-daemon write, and reads the file written, is told
/// to reload, so `guest off` reaches it; one that reads another file is left alone.
#[tokio::test]
async fn the_no_daemon_guest_write_reloads_a_daemon_that_now_reads_the_file() {
    let dir = temp_dir("guest-late");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.says(None, Some(&own.display().to_string()));
    let done = guest_alone(&daemon.socket, &own, false).await.expect("off");
    assert_eq!(daemon.reloads(), 1, "the late daemon re-read the file");
    assert!(done.line.starts_with("web guest: off in "), "{}", done.line);

    let other = dir.0.join("other.toml");
    guest_alone(&daemon.socket, &other, false)
        .await
        .expect("off");
    assert_eq!(daemon.reloads(), 1, "a daemon reading another file");
}

/// With `auth = "none"` every visitor is signed in, so `guest on` says it changes nothing and
/// still succeeds, through a daemon and with none.
#[tokio::test]
async fn guest_on_without_sign_in_says_it_changes_nothing() {
    let note = "web.auth = \"none\": every visitor is signed in, so the guest view changes nothing";
    let dir = temp_dir("guest-noauth");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_GUEST_SWITCH]);
    daemon.says_web(
        Some(WebStatus::Listening {
            listen: ([127, 0, 0, 1], 8738).into(),
            auth: svolder_ipc::config::WebAuth::None,
            writes: true,
            guest: WebGuest::None,
        }),
        None,
    );
    let done = guest_switch(&daemon.socket, &own, true).await.expect("on");
    assert_eq!(done.notes, [note]);
    let done = guest_switch(&daemon.socket, &own, false)
        .await
        .expect("off");
    assert_eq!(done.notes, Vec::<String>::new());

    std::fs::write(&own, "schema = 1\n\n[web]\nauth = \"none\"\n").unwrap();
    let done = guest_switch(&fresh_socket(), &own, true).await.expect("on");
    assert!(done.notes.contains(&note.to_string()), "{:?}", done.notes);
}
