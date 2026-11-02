// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MCP server in a running daemon: it starts from `[mcp]`, answers only a request with the
//! token, follows a reload, and never takes the daemon's socket or devices down with it.

mod common;

use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use svolder_ipc::{Client, McpStatus, Request, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use common::{CONFIG, POLL, POLLS, STEP_BUDGET, TEST_TIMEOUT, connect, spawn_daemon_isolated};

const TOOLS_LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"test","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}}}"#;

/// The config with `[mcp]` on, on a port the system picks, plus `extra` keys.
fn with_mcp(extra: &str) -> String {
    format!("{CONFIG}[mcp]\nenabled = true\nlisten = \"127.0.0.1:0\"\n{extra}")
}

/// What the daemon says about its MCP server.
async fn mcp_status(client: &mut Client) -> McpStatus {
    match client
        .request(Request::Version)
        .await
        .expect("a version reply")
    {
        Response::Version { mcp: Some(mcp), .. } => mcp,
        other => panic!("no MCP status in {other:?}"),
    }
}

/// Where the server listens, once it does.
async fn listening(client: &mut Client) -> SocketAddr {
    match mcp_status(client).await {
        McpStatus::Listening { listen, .. } => listen,
        other => panic!("the server is not listening: {}", other.line()),
    }
}

/// The status of a server that is not serving.
async fn failure(client: &mut Client) -> (SocketAddr, String) {
    match mcp_status(client).await {
        McpStatus::Failed { listen, error } => (listen, error),
        other => panic!("expected a failed server: {}", other.line()),
    }
}

/// A `tools/list` over a raw connection, with `token` as the bearer when given; the HTTP
/// status it was answered with.
async fn list_status(addr: SocketAddr, token: Option<&str>) -> u16 {
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\n{auth}Content-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nMCP-Protocol-Version: 2026-07-28\r\n\
         Mcp-Method: tools/list\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{TOOLS_LIST}",
        TOOLS_LIST.len()
    );
    let mut stream = TcpStream::connect(addr)
        .await
        .expect("connect to the server");
    stream.write_all(request.as_bytes()).await.expect("send");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    let text = String::from_utf8_lossy(&raw);
    text.split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("a status line")
}

/// The generated token, once the daemon has written it.
fn generated_token(state_home: &Path) -> String {
    std::fs::read_to_string(state_home.join("svolder").join("mcp-token"))
        .expect("the generated token file")
        .trim()
        .to_string()
}

async fn daemon_with(config: &str) -> (common::ChildGuard, Client) {
    let (mut child, socket) = spawn_daemon_isolated(config, &[]);
    let client = connect(&mut child, &socket).await;
    (child, client)
}

/// Wait for the server to be listening: it starts beside the socket, not before it.
async fn wait_listening(client: &mut Client, child: &common::ChildGuard) -> SocketAddr {
    for _ in 0..POLLS {
        if let McpStatus::Listening { listen, .. } = mcp_status(client).await {
            return listen;
        }
        tokio::time::sleep(POLL).await;
    }
    panic!(
        "the server never listened within {STEP_BUDGET:?}; the daemon said: {:?}",
        child.said()
    );
}

/// The daemon's directory is the state home: `spawn_daemon_isolated` points every XDG home there.
fn state_home(child: &common::ChildGuard) -> std::path::PathBuf {
    child
        .config_path()
        .parent()
        .expect("a directory")
        .to_path_buf()
}

#[tokio::test]
async fn the_daemon_serves_mcp_to_a_request_with_the_token() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client) = daemon_with(&with_mcp("")).await;
        let addr = wait_listening(&mut client, &child).await;
        assert_eq!(
            mcp_status(&mut client).await,
            McpStatus::Listening {
                listen: addr,
                auth: svolder_ipc::config::McpAuth::Token,
                writes: false
            }
        );

        let file = state_home(&child).join("svolder").join("mcp-token");
        let mode = std::fs::metadata(&file)
            .expect("the token file")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let token = generated_token(&state_home(&child));

        assert_eq!(list_status(addr, None).await, 401);
        assert_eq!(list_status(addr, Some("not-the-token")).await, 401);
        assert_eq!(list_status(addr, Some(&token)).await, 200);
        assert_eq!(list_status(addr, Some(&token)).await, 200);
    })
    .await
    .expect("test timed out");
}

/// The token is in the file and in the requests, and nowhere the daemon writes: not its log,
/// not its status.
#[tokio::test]
async fn the_token_never_reaches_the_log_or_the_status() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client) = daemon_with(&with_mcp("")).await;
        let addr = wait_listening(&mut client, &child).await;
        let token = generated_token(&state_home(&child));
        list_status(addr, Some(&token)).await;
        list_status(addr, Some("not-the-token")).await;
        list_status(addr, None).await;
        let status = format!("{:?}", mcp_status(&mut client).await);
        let version = format!("{:?}", client.request(Request::Version).await);
        assert!(!status.contains(&token), "the MCP status carries the token");
        assert!(
            !version.contains(&token),
            "the version reply carries the token"
        );
        assert!(
            !child.said().contains(&token),
            "the daemon's log carries the token"
        );
        assert!(
            child.said().contains("mcp: listening on"),
            "the log says nothing of the server"
        );
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_port_in_use_is_reported_and_the_daemon_serves_on() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let held = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
        let taken = held.local_addr().unwrap();
        let config = format!("{CONFIG}[mcp]\nenabled = true\nlisten = \"{taken}\"\n");
        let (mut child, mut client) = daemon_with(&config).await;
        common::wait_until_connected(&mut child, &mut client).await;
        let (listen, error) = failure(&mut client).await;
        assert_eq!(listen, taken);
        assert!(!error.is_empty());
        let line = McpStatus::Failed { listen, error }.line();
        assert!(
            line.starts_with(&format!("mcp: cannot listen on {taken}: ")),
            "{line}"
        );
        // The devices and the socket carry on.
        assert_eq!(
            common::device_names(&mut client).await,
            vec!["fake".to_string()]
        );
    })
    .await
    .expect("test timed out");
}

/// A reload tries a failed server again: the port freed, the server starts.
#[tokio::test]
async fn a_reload_retries_a_server_that_failed_to_bind() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let held = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
        let taken = held.local_addr().unwrap();
        let config = format!("{CONFIG}[mcp]\nenabled = true\nlisten = \"{taken}\"\n");
        let (child, mut client) = daemon_with(&config).await;
        failure(&mut client).await;
        drop(held);
        let reloaded = client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert!(
            matches!(reloaded, Response::Reloaded { .. }),
            "{reloaded:?}"
        );
        assert_eq!(listening(&mut client).await, taken);
        let token = generated_token(&state_home(&child));
        assert_eq!(list_status(taken, Some(&token)).await, 200);
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_reload_that_turns_mcp_off_closes_the_port() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client) = daemon_with(&with_mcp("")).await;
        let addr = wait_listening(&mut client, &child).await;
        assert!(TcpStream::connect(addr).await.is_ok());

        std::fs::write(child.config_path(), CONFIG).expect("turn [mcp] off");
        client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert_eq!(mcp_status(&mut client).await, McpStatus::Off);
        assert!(
            TcpStream::connect(addr).await.is_err(),
            "the port is still open"
        );

        // And on again.
        std::fs::write(child.config_path(), with_mcp("")).expect("turn [mcp] on");
        client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        let again = listening(&mut client).await;
        let token = generated_token(&state_home(&child));
        assert_eq!(list_status(again, Some(&token)).await, 200);
    })
    .await
    .expect("test timed out");
}

/// A reload that leaves `[mcp]` alone re-reads the token: the old one stops working and the
/// new one works, on the same port.
#[tokio::test]
async fn a_reload_takes_up_a_rotated_token() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let secret = |dir: &Path, token: &str| {
            let file = dir.join("outside-token");
            std::fs::write(&file, token).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
            file
        };
        let dir = common::temp_dir();
        let file = secret(&dir, "first-token-0123456789");
        let config = with_mcp(&format!("token_file = \"{}\"\n", file.display()));
        let (child, mut client) = daemon_with(&config).await;
        let addr = wait_listening(&mut client, &child).await;
        assert_eq!(list_status(addr, Some("first-token-0123456789")).await, 200);

        secret(&dir, "second-token-0123456789");
        client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert_eq!(listening(&mut client).await, addr, "the port moved");
        assert_eq!(list_status(addr, Some("first-token-0123456789")).await, 401);
        assert_eq!(
            list_status(addr, Some("second-token-0123456789")).await,
            200
        );

        // A file group can read is refused on the next reload: the server stops rather than
        // serve on a secret the operator has replaced.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        let (_, error) = failure(&mut client).await;
        assert!(error.contains("0644"), "{error}");
        assert!(TcpStream::connect(addr).await.is_err());
        drop(child);
        let _ = std::fs::remove_dir_all(dir);
    })
    .await
    .expect("test timed out");
}

/// A reload that leaves `[mcp]` alone keeps a server with no token on its port, too.
#[tokio::test]
async fn a_reload_keeps_a_server_with_no_auth_on_its_port() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client) = daemon_with(&with_mcp("auth = \"none\"\n")).await;
        let addr = wait_listening(&mut client, &child).await;
        for _ in 0..3 {
            client
                .request(Request::Reload)
                .await
                .expect("a reload reply");
            assert_eq!(listening(&mut client).await, addr, "the port moved");
            assert_eq!(list_status(addr, None).await, 200);
        }
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_token_file_with_a_bad_mode_fails_the_server_and_nothing_else() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = common::temp_dir();
        let file = dir.join("loose");
        std::fs::write(&file, "loose-token-0123456789").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let config = with_mcp(&format!("token_file = \"{}\"\n", file.display()));
        let (child, mut client) = daemon_with(&config).await;
        let (_, error) = failure(&mut client).await;
        assert!(error.contains("0644"), "{error}");
        assert!(!error.contains("loose-token"), "{error}");
        assert!(!child.said().contains("loose-token"));
        assert_eq!(
            common::device_names(&mut client).await,
            vec!["fake".to_string()]
        );
        drop(child);
        let _ = std::fs::remove_dir_all(dir);
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn with_no_section_there_is_no_server() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client) = daemon_with(CONFIG).await;
        assert_eq!(mcp_status(&mut client).await, McpStatus::Off);
    })
    .await
    .expect("test timed out");
}

/// A token file that is a named pipe with no writer — the shape a secret manager hands over —
/// fails the MCP server and nothing else: the daemon's socket answers within a second, at
/// startup and across a reload.
#[tokio::test]
async fn a_token_file_that_is_a_pipe_never_holds_up_the_daemon() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let dir = common::temp_dir();
        let fifo = dir.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .args(["-m", "600"])
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(made.success());
        let config = with_mcp(&format!("token_file = \"{}\"\n", fifo.display()));
        let (child, mut client) = daemon_with(&config).await;
        let quickly = Duration::from_secs(1);
        for _ in 0..2 {
            let devices = tokio::time::timeout(quickly, common::device_names(&mut client))
                .await
                .expect("devices answered within a second");
            assert_eq!(devices, vec!["fake".to_string()]);
            let (_, error) = tokio::time::timeout(quickly, failure(&mut client))
                .await
                .expect("version answered within a second");
            assert!(error.contains("not a regular file"), "{error}");
            client
                .request(Request::Reload)
                .await
                .expect("a reload reply");
        }
        drop(child);
        let _ = std::fs::remove_dir_all(dir);
    })
    .await
    .expect("test timed out");
}
