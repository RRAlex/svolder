// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use svolder_ipc::McpStatus;
use svolder_ipc::config::{McpAuth as AuthKind, McpConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::*;

/// A token that exists only here.
const TOKEN: &str = "unit-test-token-0123456789abcdefghijklmnopq";

/// A `tools/list` as a 2026-07-28 client sends it: no `initialize`, the protocol version and the
/// client's identity in every request's `_meta`.
const TOOLS_LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"test","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}}}"#;

/// What a raw HTTP/1.1 exchange came back with.
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A config for a loopback server on a port the system picks.
fn config() -> McpConfig {
    McpConfig {
        enabled: true,
        listen: "127.0.0.1:0".parse().unwrap(),
        ..McpConfig::default()
    }
}

/// Start a server over [`TOKEN`] and say where it listens.
fn serve(config: &McpConfig) -> (McpServer, SocketAddr) {
    let auth = Arc::new(TokenAuth::new(TOKEN));
    let server = McpServer::start_with(config, auth.clone(), Some(auth)).expect("the server binds");
    let McpStatus::Listening { listen, .. } = server.status() else {
        panic!("the server is not listening");
    };
    (server, listen)
}

/// One request, one connection, read to the end. `headers` is everything beyond `Host` (which
/// is the server's own unless `headers` names one), `Content-Length` and `Connection`.
async fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let mut request = format!("{method} {path} HTTP/1.1\r\n");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        request.push_str(&format!("Host: {addr}\r\n"));
    }
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        // A refusal may answer before the body has been written, and reset the rest.
        let _ = stream.write_all(request.as_bytes()).await;
        let mut raw = Vec::new();
        let _ = stream.read_to_end(&mut raw).await;
        parse(&raw)
    })
    .await
    .expect("the exchange finished")
}

/// Split a raw response into its status, headers and body, undoing chunked framing.
fn parse(raw: &[u8]) -> Reply {
    let text = String::from_utf8_lossy(raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").expect("a complete response");
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .expect("a status line");
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
    let body = if chunked {
        dechunk(body)
    } else {
        body.to_string()
    };
    Reply {
        status,
        headers,
        body,
    }
}

fn dechunk(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some((size, after)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if size == 0 || after.len() < size {
            break;
        }
        out.push_str(&after[..size]);
        rest = after[size..].trim_start_matches("\r\n");
    }
    out
}

/// The headers a 2026-07-28 JSON-RPC POST carries, with the bearer token.
fn rpc_headers(token: Option<&str>) -> Vec<(String, String)> {
    let mut headers = vec![
        ("Content-Type".into(), "application/json".into()),
        (
            "Accept".into(),
            "application/json, text/event-stream".into(),
        ),
        ("MCP-Protocol-Version".into(), "2026-07-28".into()),
        ("Mcp-Method".into(), "tools/list".into()),
    ];
    if let Some(token) = token {
        headers.push(("Authorization".into(), format!("Bearer {token}")));
    }
    headers
}

/// A `tools/list` POST to `/mcp` with the given extra headers.
async fn list(addr: SocketAddr, token: Option<&str>, extra: &[(&str, &str)]) -> Reply {
    let mut headers = rpc_headers(token);
    headers.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    let borrowed: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (&**k, &**v)).collect();
    send(addr, "POST", PATH, &borrowed, TOOLS_LIST).await
}

#[tokio::test]
async fn the_right_token_gets_the_tool_list() {
    let (server, addr) = serve(&config());
    let reply = list(addr, Some(TOKEN), &[]).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let json: serde_json::Value = serde_json::from_str(&reply.body).expect("a JSON answer");
    let names: Vec<&str> = json["result"]["tools"]
        .as_array()
        .expect("a tool list")
        .iter()
        .map(|t| t["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(
        names,
        [
            "list_devices",
            "get_state",
            "get_link_history",
            "list_presets"
        ],
        "{json}"
    );
    server.stop().await;
}

#[tokio::test]
async fn a_request_without_the_token_is_refused_whatever_it_asks() {
    let (server, addr) = serve(&config());
    let mut cases: Vec<(&str, &str, &str)> = vec![
        ("POST", PATH, TOOLS_LIST),
        (
            "POST",
            PATH,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"x"}}"#,
        ),
        (
            "POST",
            PATH,
            r#"{"jsonrpc":"2.0","id":1,"method":"server/discover"}"#,
        ),
        ("POST", PATH, "not json"),
        ("POST", PATH, ""),
        ("GET", PATH, ""),
        ("DELETE", PATH, ""),
        ("PUT", PATH, ""),
        ("POST", "/", TOOLS_LIST),
        ("POST", "/mcp/", TOOLS_LIST),
        ("POST", "/elsewhere", TOOLS_LIST),
    ];
    cases.push(("OPTIONS", "*", ""));
    for (method, path, body) in cases {
        let headers = rpc_headers(None);
        let borrowed: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (&**k, &**v)).collect();
        let reply = send(addr, method, path, &borrowed, body).await;
        assert_eq!(reply.status, 401, "{method} {path}: {}", reply.body);
        assert_eq!(reply.header("www-authenticate"), Some("Bearer"));
        assert_eq!(reply.body, "", "{method} {path} said something");
    }
    server.stop().await;
}

#[tokio::test]
async fn a_wrong_or_malformed_credential_is_refused() {
    let (server, addr) = serve(&config());
    let longer = format!("{TOKEN}x");
    let shorter = &TOKEN[..TOKEN.len() - 1];
    let overlong = "a".repeat(MAX_AUTHORIZATION + 1);
    for credential in [
        "Bearer wrong".to_string(),
        format!("Bearer {longer}"),
        format!("Bearer {shorter}"),
        "Bearer ".to_string(),
        "Bearer".to_string(),
        TOKEN.to_string(),
        format!("Basic {TOKEN}"),
        format!("Bearer {overlong}"),
        format!("Bearer  {TOKEN}"),
    ] {
        let reply = list(addr, None, &[("Authorization", &credential)]).await;
        assert_eq!(
            reply.status,
            401,
            "a credential of {} bytes",
            credential.len()
        );
        assert_eq!(reply.header("www-authenticate"), Some("Bearer"));
    }
    // The scheme's case is not the credential's.
    let reply = list(addr, None, &[("Authorization", &format!("bearer {TOKEN}"))]).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    server.stop().await;
}

#[tokio::test]
async fn more_than_one_credential_is_refused() {
    let (server, addr) = serve(&config());
    let good = format!("Bearer {TOKEN}");
    for pair in [
        [good.as_str(), good.as_str()],
        [good.as_str(), "Bearer other"],
        ["Bearer other", good.as_str()],
    ] {
        let mut headers = rpc_headers(None);
        headers.push(("Authorization".into(), pair[0].into()));
        headers.push(("Authorization".into(), pair[1].into()));
        let borrowed: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (&**k, &**v)).collect();
        let reply = send(addr, "POST", PATH, &borrowed, TOOLS_LIST).await;
        assert_eq!(reply.status, 401);
    }
    server.stop().await;
}

#[tokio::test]
async fn a_token_that_was_replaced_stops_working_at_once() {
    let auth = Arc::new(TokenAuth::new(TOKEN));
    let server = McpServer::start_with(&config(), auth.clone(), Some(auth.clone())).unwrap();
    let McpStatus::Listening { listen: addr, .. } = server.status() else {
        panic!("not listening");
    };
    auth.replace("the-next-token");
    assert_eq!(list(addr, Some(TOKEN), &[]).await.status, 401);
    assert_eq!(list(addr, Some("the-next-token"), &[]).await.status, 200);
    server.stop().await;
}

#[tokio::test]
async fn auth_none_skips_the_check() {
    let config = McpConfig {
        auth: AuthKind::None,
        ..config()
    };
    let server = McpServer::start_with(&config, Arc::new(NoAuth), None).unwrap();
    let McpStatus::Listening { listen, auth, .. } = server.status() else {
        panic!("not listening");
    };
    assert_eq!(auth, AuthKind::None);
    assert_eq!(list(listen, None, &[]).await.status, 200);
    server.stop().await;
}

#[tokio::test]
async fn a_foreign_origin_is_refused_even_with_the_token() {
    let (server, addr) = serve(&config());
    for origin in [
        "https://evil.example",
        "http://127.0.0.1:1",
        "null",
        "garbage",
    ] {
        let reply = list(addr, Some(TOKEN), &[("Origin", origin)]).await;
        assert_eq!(reply.status, 403, "Origin {origin}: {}", reply.body);
    }
    // Without an Origin header (a client that is not a browser) the request goes on.
    assert_eq!(list(addr, Some(TOKEN), &[]).await.status, 200);
    server.stop().await;
}

#[tokio::test]
async fn an_origin_the_config_names_is_allowed() {
    let config = McpConfig {
        origins: vec!["https://app.example:443".to_string()],
        ..config()
    };
    let (server, addr) = serve(&config);
    let allowed = list(addr, Some(TOKEN), &[("Origin", "https://app.example")]).await;
    assert_eq!(allowed.status, 200, "{}", allowed.body);
    let other = list(addr, Some(TOKEN), &[("Origin", "https://evil.example")]).await;
    assert_eq!(other.status, 403);
    server.stop().await;
}

#[tokio::test]
async fn a_foreign_host_is_refused_even_with_the_token() {
    let (server, addr) = serve(&config());
    let port = addr.port();
    for host in [
        "evil.example".to_string(),
        format!("evil.example:{port}"),
        "127.0.0.1".to_string(),
        format!("127.0.0.1:{}", port.wrapping_add(1)),
    ] {
        let reply = list(addr, Some(TOKEN), &[("Host", &host)]).await;
        assert!(
            matches!(reply.status, 400 | 403),
            "Host {host}: {} {}",
            reply.status,
            reply.body
        );
    }
    for host in [
        format!("localhost:{port}"),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
    ] {
        let reply = list(addr, Some(TOKEN), &[("Host", &host)]).await;
        assert_eq!(reply.status, 200, "Host {host}: {}", reply.body);
    }
    server.stop().await;
}

#[tokio::test]
async fn a_body_over_the_cap_is_refused() {
    let (server, addr) = serve(&config());
    let big = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{{"pad":"{}"}}}}"#,
        "a".repeat(70 * 1024)
    );
    let headers = rpc_headers(Some(TOKEN));
    let borrowed: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (&**k, &**v)).collect();
    let reply = send(addr, "POST", PATH, &borrowed, &big).await;
    assert_eq!(reply.status, 413, "{}", reply.body);
    server.stop().await;
}

#[tokio::test]
async fn requests_need_no_session() {
    let (server, addr) = serve(&config());
    for _ in 0..2 {
        let reply = list(addr, Some(TOKEN), &[]).await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.header("mcp-session-id"), None);
    }
    server.stop().await;
}

#[tokio::test]
async fn an_address_in_use_leaves_a_failed_server() {
    let (first, addr) = serve(&config());
    // No auth, so nothing here reads a token from the developer's own state directory.
    let taken = McpConfig {
        listen: addr,
        auth: AuthKind::None,
        ..config()
    };
    let second =
        McpServer::start(&taken, Arc::new(crate::fleet::Unavailable), Arc::default()).await;
    assert!(!second.serving());
    let McpStatus::Failed { listen, error } = second.status() else {
        panic!("expected a failed server");
    };
    assert_eq!(listen, addr);
    assert!(!error.is_empty());
    second.stop().await;
    // The first still answers.
    assert_eq!(list(addr, Some(TOKEN), &[]).await.status, 200);
    first.stop().await;
}

/// A token file group can read fails the server, and neither the status nor its line carries
/// the file's content.
#[tokio::test]
async fn a_token_that_cannot_be_had_fails_the_server_without_naming_it() {
    let dir = std::env::temp_dir().join(format!("svolderd-mcp-unit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("loose");
    std::fs::write(&file, "known-secret-value-0123456789").unwrap();
    std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
    let config = McpConfig {
        token_file: Some(file.display().to_string()),
        ..config()
    };
    let server =
        McpServer::start(&config, Arc::new(crate::fleet::Unavailable), Arc::default()).await;
    let status = server.status();
    let McpStatus::Failed { error, .. } = &status else {
        panic!("expected a failed server");
    };
    assert!(error.contains("0644"), "{error}");
    for said in [format!("{status:?}"), status.line()] {
        assert!(!said.contains("known-secret-value"), "{said}");
    }
    server.stop().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_token_file_that_is_a_pipe_fails_the_server_at_once() {
    let dir = std::env::temp_dir().join(format!("svolderd-mcp-fifo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("pipe");
    assert!(
        std::process::Command::new("mkfifo")
            .args(["-m", "600"])
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let config = McpConfig {
        token_file: Some(fifo.display().to_string()),
        ..config()
    };
    let started = std::time::Instant::now();
    let server =
        McpServer::start(&config, Arc::new(crate::fleet::Unavailable), Arc::default()).await;
    assert!(started.elapsed() < Duration::from_secs(2));
    let McpStatus::Failed { error, .. } = server.status() else {
        panic!("expected a failed server");
    };
    assert!(error.contains("not a regular file"), "{error}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A token the header cap cannot carry is refused where it is read, not met as a 401 on every
/// request; and the cap itself refuses a header over it.
#[tokio::test]
async fn the_header_cap_refuses_even_the_right_token_over_it() {
    let long = "t".repeat(MAX_AUTHORIZATION + 88);
    let auth = Arc::new(TokenAuth::new(&long));
    let server = McpServer::start_with(&config(), auth.clone(), Some(auth)).unwrap();
    let McpStatus::Listening { listen: addr, .. } = server.status() else {
        panic!("not listening");
    };
    assert_eq!(list(addr, Some(&long), &[]).await.status, 401);
    server.stop().await;
}

/// Paths other than `/mcp` are 404 for a caller with the token, with nothing in the body.
#[tokio::test]
async fn other_paths_are_not_found_even_with_the_token() {
    let (server, addr) = serve(&config());
    for path in ["/", "/mcp/", "/elsewhere", "/mcpx"] {
        let headers = rpc_headers(Some(TOKEN));
        let borrowed: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (&**k, &**v)).collect();
        let reply = send(addr, "POST", path, &borrowed, TOOLS_LIST).await;
        assert_eq!(reply.status, 404, "{path}");
        assert_eq!(reply.body, "", "{path}");
    }
    server.stop().await;
}

/// With no auth the browser defences are the only ones: a foreign Origin or Host is refused.
#[tokio::test]
async fn with_no_auth_a_browser_page_is_still_refused() {
    let config = McpConfig {
        auth: AuthKind::None,
        ..config()
    };
    let server = McpServer::start_with(&config, Arc::new(NoAuth), None).unwrap();
    let McpStatus::Listening { listen: addr, .. } = server.status() else {
        panic!("not listening");
    };
    for origin in [
        "https://evil.example",
        "null",
        &format!("http://localhost:{}", addr.port()),
    ] {
        let reply = list(addr, None, &[("Origin", origin)]).await;
        assert_eq!(reply.status, 403, "Origin {origin}: {}", reply.body);
    }
    let rebound = list(
        addr,
        None,
        &[("Host", &format!("evil.example:{}", addr.port()))],
    )
    .await;
    assert_eq!(rebound.status, 403, "{}", rebound.body);
    assert_eq!(list(addr, None, &[]).await.status, 200);
    server.stop().await;
}

/// Whatever answers after the auth sees no `Authorization` header.
#[tokio::test]
async fn the_credential_is_removed_before_the_request_goes_on() {
    let echo: Inner = Arc::new(|request| {
        let seen = request.headers().contains_key(AUTHORIZATION);
        Box::pin(async move {
            let body = http_body_util::Full::new(Bytes::from(format!("authorization={seen}")));
            Response::new(body.boxed())
        })
    });
    let auth = Arc::new(TokenAuth::new(TOKEN));
    let listener = bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let server = McpServer::launch(
        &config(),
        listener,
        auth.clone(),
        Some(auth),
        Arc::new(crate::fleet::Unavailable),
        Arc::default(),
        Some(echo),
    )
    .unwrap();
    let McpStatus::Listening { listen: addr, .. } = server.status() else {
        panic!("not listening");
    };
    let reply = list(addr, Some(TOKEN), &[]).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, "authorization=false");
    server.stop().await;
}

/// A name `[mcp] hosts` lists is a Host the server accepts, with any port when it names none.
#[tokio::test]
async fn a_configured_host_name_is_accepted() {
    let config = McpConfig {
        hosts: vec!["mcp.home.lan".to_string()],
        ..config()
    };
    let (server, addr) = serve(&config);
    for host in ["mcp.home.lan", "mcp.home.lan:443"] {
        let reply = list(addr, Some(TOKEN), &[("Host", host)]).await;
        assert_eq!(reply.status, 200, "Host {host}: {}", reply.body);
    }
    let other = list(addr, Some(TOKEN), &[("Host", "evil.example")]).await;
    assert_eq!(other.status, 403);
    server.stop().await;
}

#[tokio::test]
async fn stopping_closes_the_port() {
    let (server, addr) = serve(&config());
    server.stop().await;
    assert!(TcpStream::connect(addr).await.is_err());
}
