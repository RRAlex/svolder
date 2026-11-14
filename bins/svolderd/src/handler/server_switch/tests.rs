// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;

use svolder_ipc::{
    CODE_USAGE, Config, Handler, McpStatus, Reason, Request, Response, Server, WebGuest, WebStatus,
};

use crate::handler::Daemon;
use crate::handler::tests::daemon_over;

/// Both servers off, each on a loopback port the system picks and with no token to make, so a
/// switch binds a real listener and touches nothing outside the scratch folder.
const BOTH_OFF: &str = "schema = 1\n\n[web]\nlisten = \"127.0.0.1:0\"\nauth = \"none\"\n\n\
                        [mcp]\nlisten = \"127.0.0.1:0\"\nauth = \"none\"\n";

/// A scratch folder with a configuration file holding `text`, and a shared daemon reading it.
fn daemon_on(tag: &str, text: &str) -> (PathBuf, PathBuf, Arc<Daemon>) {
    let dir = std::env::temp_dir().join(format!("svolderd-ss-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, text).unwrap();
    let daemon = daemon_over(path.clone(), dir.join("profiles")).shared();
    (dir, path, daemon)
}

async fn switch(daemon: &Daemon, server: Server, enabled: bool) -> Response {
    daemon
        .handle(Request::ServerSwitch { server, enabled })
        .await
}

/// The two states of an answer that is one.
fn stands(answer: Response) -> (WebStatus, McpStatus) {
    match answer {
        Response::ServerSwitch { web, mcp } => (web, mcp),
        other => panic!("not the servers: {other:?}"),
    }
}

/// The reason of an answer that refuses, which is always code 1.
fn refused(answer: Response) -> Reason {
    match answer {
        Response::Error {
            code: CODE_USAGE,
            reason: Some(reason),
            ..
        } => reason,
        other => panic!("not a refusal with a reason: {other:?}"),
    }
}

/// Turning the web UI on writes `enabled = true` under `[web]`, and the reload the write asks
/// for binds the listener that was never bound: the answer says it listens. The MCP server is
/// left as it was, in the file and in the daemon.
#[tokio::test]
async fn web_on_writes_the_key_and_the_reload_binds_the_listener() {
    let (dir, path, daemon) = daemon_on("web-on", BOTH_OFF);
    let (web, mcp) = stands(switch(&daemon, Server::Web, true).await);
    assert!(matches!(web, WebStatus::Listening { .. }), "{web:?}");
    assert_eq!(mcp, McpStatus::Off);
    let config = Config::load_from(&path).unwrap();
    assert!(config.web.enabled && !config.mcp.enabled);
    assert!(daemon.config_snapshot().web.enabled, "the snapshot follows");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("auth = \"none\""),
        "the other keys stay: {text}"
    );
    let Response::Version { web: said, .. } = daemon.handle(Request::Version).await else {
        panic!("a version");
    };
    assert_eq!(said, Some(web), "version says the same");
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Turning it off again closes the listener at the same reload and writes `enabled = false`.
#[tokio::test]
async fn web_off_closes_the_listener() {
    let (dir, path, daemon) = daemon_on("web-off", BOTH_OFF);
    let (web, _) = stands(switch(&daemon, Server::Web, true).await);
    let WebStatus::Listening { listen, .. } = web else {
        panic!("not listening: {web:?}");
    };
    let (web, mcp) = stands(switch(&daemon, Server::Web, false).await);
    assert_eq!((web, mcp), (WebStatus::Off, McpStatus::Off));
    assert!(!Config::load_from(&path).unwrap().web.enabled);
    assert!(
        std::net::TcpStream::connect(listen).is_err(),
        "nothing answers on {listen} any more"
    );
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// The MCP server has the same switch, and each server's leaves the other alone.
#[tokio::test]
async fn mcp_on_and_off_leave_the_web_ui_alone() {
    let (dir, path, daemon) = daemon_on("mcp", BOTH_OFF);
    let (web, mcp) = stands(switch(&daemon, Server::Mcp, true).await);
    assert!(matches!(mcp, McpStatus::Listening { .. }), "{mcp:?}");
    assert_eq!(web, WebStatus::Off);
    let config = Config::load_from(&path).unwrap();
    assert!(config.mcp.enabled && !config.web.enabled);
    let (web, mcp) = stands(switch(&daemon, Server::Mcp, false).await);
    assert_eq!((web, mcp), (WebStatus::Off, McpStatus::Off));
    assert!(!Config::load_from(&path).unwrap().mcp.enabled);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A switch to the state the server is in writes nothing and answers where the servers stand.
#[tokio::test]
async fn a_switch_that_changes_nothing_leaves_the_file_byte_for_byte() {
    let (dir, path, daemon) = daemon_on("same", BOTH_OFF);
    let (web, mcp) = stands(switch(&daemon, Server::Web, false).await);
    assert_eq!((web, mcp), (WebStatus::Off, McpStatus::Off));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOTH_OFF);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A daemon that never had a configuration file answers a switch to off, which is where a server
/// with no file stands, and binds nothing.
#[tokio::test]
async fn a_switch_to_off_with_no_file_is_answered() {
    let (dir, path, daemon) = daemon_on("no-file", BOTH_OFF);
    let never = dir.join("never.toml");
    let daemon_never = daemon_over(never.clone(), dir.join("profiles")).shared();
    let _ = &path;
    for server in [Server::Web, Server::Mcp] {
        let (web, mcp) = stands(switch(&daemon_never, server, false).await);
        assert_eq!((web, mcp), (WebStatus::Off, McpStatus::Off));
    }
    daemon_never.shutdown().await;
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A switch while the file the daemon read is gone is refused in the missing-file words, and the
/// file is not made again.
#[tokio::test]
async fn a_switch_while_the_file_is_gone_is_refused_and_writes_nothing() {
    let (dir, path, daemon) = daemon_on("gone", BOTH_OFF);
    std::fs::remove_file(&path).unwrap();
    for server in [Server::Web, Server::Mcp] {
        let Response::Error {
            message, reason, ..
        } = switch(&daemon, server, false).await
        else {
            panic!("a switch with the file gone is refused");
        };
        assert!(message.contains("is missing"), "{message}");
        assert_eq!(reason, Some(svolder_ipc::Reason::ReadOnly));
    }
    assert!(!path.exists(), "the file was made again");
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A switch on a file that does not parse (here a hand edit made since the last reload) is
/// refused as `invalid`, and the file is left as it was.
#[tokio::test]
async fn a_switch_the_file_would_not_parse_with_is_refused_as_invalid() {
    let text = "schema = 1\n\n[web]\nlisten = \"127.0.0.1:0\"\nauth = \"none\"\n\n\
                [mqtt]\nenabled = false\n";
    let (dir, path, daemon) = daemon_on("invalid", text);
    let broken = format!("{text}host = 5\n");
    std::fs::write(&path, &broken).unwrap();
    assert_eq!(
        refused(switch(&daemon, Server::Web, true).await),
        Reason::Invalid
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file that cannot be written refuses the switch as `read_only`.
#[tokio::test]
async fn a_read_only_file_refuses_the_switch_as_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, path, daemon) = daemon_on("read-only", BOTH_OFF);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(
        refused(switch(&daemon, Server::Web, true).await),
        Reason::ReadOnly
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOTH_OFF);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Once the stop has begun the switch is refused as `stopping` and the file is left alone.
#[tokio::test]
async fn a_switch_after_the_stop_began_is_refused_as_stopping() {
    let (dir, path, daemon) = daemon_on("stopping", BOTH_OFF);
    daemon.shutdown().await;
    for server in [Server::Web, Server::Mcp] {
        assert_eq!(
            refused(switch(&daemon, server, true).await),
            Reason::Stopping
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOTH_OFF);
    let _ = std::fs::remove_dir_all(&dir);
}

/// While another writer holds the file's lock past the wait, the switch is refused as
/// `file_busy` and nothing is written.
#[tokio::test]
async fn a_switch_that_waits_out_the_files_lock_is_refused_as_file_busy() {
    let (dir, path, daemon) = daemon_on("file-busy", BOTH_OFF);
    let mut lock = path.clone().into_os_string();
    lock.push(".lock");
    let held = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lock)
        .unwrap();
    held.lock().expect("the test takes the file's lock");
    assert_eq!(
        refused(switch(&daemon, Server::Mcp, true).await),
        Reason::FileBusy
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOTH_OFF);
    drop(held);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

async fn guest(daemon: &Daemon, read: bool) -> Response {
    daemon.handle(Request::GuestSwitch { read }).await
}

/// A web UI that signs in by token, off, on a loopback port the system picks, with the token in
/// a file in the scratch folder.
fn web_with_token(dir: &std::path::Path) -> String {
    let token = dir.join("web-token");
    std::fs::write(&token, "guest-switch-token-0123456789abcdefghijkl\n").unwrap();
    std::fs::set_permissions(&token, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    format!(
        "schema = 1\n\n[web]\nlisten = \"127.0.0.1:0\"\nauth = \"token\"\n\
         token_file = \"{}\"\n",
        token.display()
    )
}

/// The status of a `GET /api/v1/fleet` with no sign-in, which a guest may read.
async fn guest_get(listen: std::net::SocketAddr) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let request =
        format!("GET /api/v1/fleet HTTP/1.1\r\nHost: {listen}\r\nConnection: close\r\n\r\n");
    let mut stream = tokio::net::TcpStream::connect(listen)
        .await
        .expect("connect");
    stream.write_all(request.as_bytes()).await.expect("send");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    String::from_utf8_lossy(&raw)
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("a status line")
}

/// The guest switch writes `guest = "read"` and the reload makes the running web server answer a
/// visitor without a sign-in; switching off writes `guest = "none"` and the same visitor is
/// refused again. The other keys and the server's enabled state stay.
#[tokio::test]
async fn the_guest_switch_writes_the_key_and_the_running_server_follows() {
    let dir = std::env::temp_dir().join(format!("svolderd-gs-on-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, web_with_token(&dir)).unwrap();
    let daemon = daemon_over(path.clone(), dir.join("profiles")).shared();
    let (web, _) = stands(switch(&daemon, Server::Web, true).await);
    let WebStatus::Listening {
        listen, guest: now, ..
    } = web
    else {
        panic!("not listening: {web:?}");
    };
    assert_eq!(now, WebGuest::None);
    assert_eq!(guest_get(listen).await, 401);

    let (web, mcp) = stands(guest(&daemon, true).await);
    let WebStatus::Listening {
        listen, guest: now, ..
    } = web
    else {
        panic!("not listening: {web:?}");
    };
    assert_eq!((now, mcp), (WebGuest::Read, McpStatus::Off));
    assert_eq!(guest_get(listen).await, 200, "a guest reads after on");
    let config = Config::load_from(&path).unwrap();
    assert_eq!(config.web.guest, WebGuest::Read);
    assert!(config.web.enabled);
    assert_eq!(daemon.config_snapshot().web.guest, WebGuest::Read);

    let (web, _) = stands(guest(&daemon, false).await);
    let WebStatus::Listening {
        listen, guest: now, ..
    } = web
    else {
        panic!("not listening: {web:?}");
    };
    assert_eq!(now, WebGuest::None);
    assert_eq!(guest_get(listen).await, 401, "a guest is refused after off");
    assert_eq!(Config::load_from(&path).unwrap().web.guest, WebGuest::None);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A guest switch to where the key already stands writes nothing.
#[tokio::test]
async fn a_guest_switch_that_changes_nothing_leaves_the_file_byte_for_byte() {
    let (dir, path, daemon) = daemon_on("guest-same", BOTH_OFF);
    let (web, mcp) = stands(guest(&daemon, false).await);
    assert_eq!((web, mcp), (WebStatus::Off, McpStatus::Off));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOTH_OFF);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every refusal of the server switch is the guest switch's too: `invalid`, `read_only`,
/// `file_busy` and `stopping`, with the file left as it was.
#[tokio::test]
async fn the_guest_switch_is_refused_for_each_reason() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, path, daemon) = daemon_on("guest-invalid", BOTH_OFF);
    let broken = format!("{BOTH_OFF}\n[mqtt]\nhost = 5\n");
    std::fs::write(&path, &broken).unwrap();
    assert_eq!(refused(guest(&daemon, true).await), Reason::Invalid);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);

    let (dir, path, daemon) = daemon_on("guest-ro", BOTH_OFF);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(refused(guest(&daemon, true).await), Reason::ReadOnly);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOTH_OFF);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);

    let (dir, path, daemon) = daemon_on("guest-gone", BOTH_OFF);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(refused(guest(&daemon, true).await), Reason::ReadOnly);
    assert!(!path.exists(), "the file was made again");
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);

    let (dir, path, daemon) = daemon_on("guest-busy", BOTH_OFF);
    let mut lock = path.clone().into_os_string();
    lock.push(".lock");
    let held = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lock)
        .unwrap();
    held.lock().expect("the test takes the file's lock");
    assert_eq!(refused(guest(&daemon, true).await), Reason::FileBusy);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOTH_OFF);
    drop(held);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);

    let (dir, path, daemon) = daemon_on("guest-stopping", BOTH_OFF);
    daemon.shutdown().await;
    assert_eq!(refused(guest(&daemon, true).await), Reason::Stopping);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOTH_OFF);
    let _ = std::fs::remove_dir_all(&dir);
}
