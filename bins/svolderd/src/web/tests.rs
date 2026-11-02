// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use svolder_ipc::WebStatus;
use svolder_ipc::config::{WebAuth, WebConfig};
use tokio::net::TcpStream;

use super::*;
use crate::fleet::Unavailable;

/// A loopback config with no sign-in, on a port the system picks.
fn loopback() -> WebConfig {
    WebConfig {
        enabled: true,
        listen: "127.0.0.1:0".parse().unwrap(),
        auth: WebAuth::None,
        ..WebConfig::default()
    }
}

#[tokio::test]
async fn a_server_listens_and_stopping_closes_the_port() {
    let server = WebServer::start(&loopback(), Arc::new(Unavailable), Default::default()).await;
    let WebStatus::Listening {
        listen,
        auth,
        writes,
        guest,
    } = server.status()
    else {
        panic!("not listening: {:?}", server.status());
    };
    assert_eq!((auth, writes), (WebAuth::None, true));
    assert_eq!(guest, svolder_ipc::config::WebGuest::None);
    assert!(TcpStream::connect(listen).await.is_ok());
    server.stop().await;
    assert!(TcpStream::connect(listen).await.is_err());
}

#[tokio::test]
async fn an_address_in_use_leaves_a_failed_server() {
    let first = WebServer::start(&loopback(), Arc::new(Unavailable), Default::default()).await;
    let WebStatus::Listening { listen, .. } = first.status() else {
        panic!("not listening");
    };
    let taken = WebConfig {
        listen,
        ..loopback()
    };
    let second = WebServer::start(&taken, Arc::new(Unavailable), Default::default()).await;
    assert!(!second.serving());
    let WebStatus::Failed {
        listen: asked,
        error,
    } = second.status()
    else {
        panic!("expected a failed server");
    };
    assert_eq!(asked, listen);
    assert!(!error.is_empty());
    second.stop().await;
    first.stop().await;
}

/// A token file group can read fails the server, and neither the status nor its line carries
/// the file's content.
#[tokio::test]
async fn a_token_that_cannot_be_had_fails_the_server_without_naming_it() {
    let dir = std::env::temp_dir().join(format!("svolderd-web-unit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("loose");
    std::fs::write(&file, "known-secret-value-0123456789").unwrap();
    std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
    let config = WebConfig {
        auth: WebAuth::Token,
        token_file: Some(file.display().to_string()),
        ..loopback()
    };
    let server = WebServer::start(&config, Arc::new(Unavailable), Default::default()).await;
    let status = server.status();
    let WebStatus::Failed { error, .. } = &status else {
        panic!("expected a failed server");
    };
    assert!(error.contains("0644"), "{error}");
    for said in [format!("{status:?}"), status.line()] {
        assert!(!said.contains("known-secret-value"), "{said}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// A token from the web store is described, never shown, and the MCP store is left alone.
#[tokio::test]
async fn a_token_server_says_where_its_token_comes_from() {
    let dir = std::env::temp_dir().join(format!("svolderd-web-token-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("web");
    std::fs::write(&file, "web-token-for-the-test-0123456789").unwrap();
    std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    let config = WebConfig {
        auth: WebAuth::Token,
        token_file: Some(file.display().to_string()),
        ..loopback()
    };
    let server = WebServer::start(&config, Arc::new(Unavailable), Default::default()).await;
    let info = server.token_info().expect("a token's description");
    assert_eq!(info.kind, svolder_ipc::TokenKind::File);
    assert_eq!(info.tail, "789");
    server.stop().await;
    let _ = std::fs::remove_dir_all(dir);
}

/// A token-auth config on a port the system picks, reading its token from `file`.
fn with_token_file(file: &std::path::Path, writes: bool) -> WebConfig {
    WebConfig {
        auth: WebAuth::Token,
        token_file: Some(file.display().to_string()),
        writes,
        ..loopback()
    }
}

/// A token file of mode 0600 holding `value`.
fn token_file(dir: &std::path::Path, name: &str, value: &str) -> std::path::PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, value).unwrap();
    std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    file
}

/// Where the daemon's web server listens now, as `version` reports it.
async fn listening(daemon: &crate::handler::Daemon) -> std::net::SocketAddr {
    let origin = crate::writer::Origin::Socket;
    match daemon
        .handle_from(origin, svolder_ipc::Request::Version)
        .await
    {
        svolder_ipc::Response::Version {
            web: Some(WebStatus::Listening { listen, .. }),
            ..
        } => listen,
        other => panic!("not listening: {other:?}"),
    }
}

/// The status of a `POST` of `body` to `path` on `addr`.
async fn post_status(addr: std::net::SocketAddr, path: &str, body: &str) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://{addr}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    reply
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("no status in {reply:?}"))
}

/// The link and pairing codes the daemon's web server hands out now.
async fn mint(daemon: &crate::handler::Daemon) -> (String, String) {
    use svolder_ipc::{Request, Response};
    let origin = crate::writer::Origin::Socket;
    let Response::WebLink { code: link, .. } = daemon.handle_from(origin, Request::WebLink).await
    else {
        panic!("no link code");
    };
    let Response::WebPair { code: pair, .. } = daemon.handle_from(origin, Request::WebPair).await
    else {
        panic!("no pairing code");
    };
    (link, pair)
}

/// A change of `[web]` that restarts the server keeps the codes open: a link and a pairing code
/// made before it sign in against the new server.
#[tokio::test]
async fn the_codes_outlive_a_restart_of_the_server() {
    let fake = crate::web::test_support::fake_daemon("gungnir2-v2", "dac").await;
    let dir = fake.dir.clone();
    let file = token_file(&dir, "token", "web-token-for-the-restart-0123456789");
    let daemon = &fake.daemon;
    daemon.web_to(&with_token_file(&file, true)).await;
    let first = listening(daemon).await;
    let (link, pair) = mint(daemon).await;
    daemon.web_to(&with_token_file(&file, false)).await;
    let second = listening(daemon).await;
    assert_ne!(
        first, second,
        "the section changed, so the server restarted"
    );
    let link_body = format!(r#"{{"link":"{link}"}}"#);
    assert_eq!(post_status(second, "/login", &link_body).await, 200);
    let pair_body = format!(r#"{{"code":"{pair}"}}"#);
    assert_eq!(post_status(second, "/pair", &pair_body).await, 200);
    // Each is spent once.
    assert_eq!(post_status(second, "/login", &link_body).await, 401);
    daemon.web_to(&WebConfig::default()).await;
    fake.stop().await;
}

/// A link code is open until a browser signs in with it, and asking about it spends nothing.
#[tokio::test]
async fn a_link_code_is_open_until_a_browser_uses_it() {
    use svolder_ipc::{Request, Response};
    let fake = crate::web::test_support::fake_daemon("gungnir2-v2", "dac").await;
    let dir = fake.dir.clone();
    let file = token_file(&dir, "token", "web-token-for-the-ask-0123456789abc");
    let daemon = &fake.daemon;
    daemon.web_to(&with_token_file(&file, true)).await;
    let addr = listening(daemon).await;
    let (link, _) = mint(daemon).await;
    let origin = crate::writer::Origin::Socket;
    let open = |code: String| daemon.handle_from(origin, Request::WebLinkOpen { code });
    assert_eq!(
        open(link.clone()).await,
        Response::WebLinkOpen { open: true }
    );
    let link_body = format!(r#"{{"link":"{link}"}}"#);
    assert_eq!(post_status(addr, "/login", &link_body).await, 200);
    assert_eq!(open(link).await, Response::WebLinkOpen { open: false });
    daemon.web_to(&WebConfig::default()).await;
    fake.stop().await;
}

/// A code made under one token never signs a browser in under another, also when the new server
/// reads its token from elsewhere.
#[tokio::test]
async fn a_restart_under_another_token_ends_the_codes() {
    let fake = crate::web::test_support::fake_daemon("gungnir2-v2", "dac").await;
    let dir = fake.dir.clone();
    let a = token_file(&dir, "a", "web-token-a-for-the-restart-0123456789");
    let b = token_file(&dir, "b", "web-token-b-for-the-restart-0123456789");
    let daemon = &fake.daemon;
    daemon.web_to(&with_token_file(&a, true)).await;
    let (link, pair) = mint(daemon).await;
    daemon.web_to(&with_token_file(&b, true)).await;
    let now = listening(daemon).await;
    let link_body = format!(r#"{{"link":"{link}"}}"#);
    assert_eq!(post_status(now, "/login", &link_body).await, 401);
    let pair_body = format!(r#"{{"code":"{pair}"}}"#);
    assert_eq!(post_status(now, "/pair", &pair_body).await, 401);
    daemon.web_to(&WebConfig::default()).await;
    fake.stop().await;
}
