// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The flow docs/usage/mcp.md's "MCP server" section walks a user through, against a fake daemon in
//! temporary XDG homes: enable `[mcp]`, run `svolder mcp token`, then `tools/list` and
//! `get_state` over HTTP with that token. The token is compared and measured, never printed.

mod common;

use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use svolder_ipc::{McpStatus, Request, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use common::{CONFIG, POLL, POLLS, TEST_TIMEOUT, connect, spawn_daemon_isolated};

/// The `svolder` binary built beside `svolderd`, or `None` when this package's tests are run
/// alone and cargo has not built it.
fn svolder_binary() -> Option<std::path::PathBuf> {
    let path = Path::new(env!("CARGO_BIN_EXE_svolderd")).with_file_name("svolder");
    if path.exists() {
        return Some(path);
    }
    // `make test` builds every binary and sets this, so a missing one is a broken build.
    assert!(
        std::env::var_os("SVOLDER_TEST_PREBUILT").is_none(),
        "SVOLDER_TEST_PREBUILT is set but {} is not there",
        path.display()
    );
    None
}

/// `svolder mcp token` with `home` as every XDG home, as the daemon has it.
fn cli_token(binary: &Path, home: &Path) -> String {
    let out = Command::new(binary)
        .args(["mcp", "token"])
        .env("SVOLDER_TRANSPORT", "fake")
        .env("SVOLDER_ADAPTER", "hci-none")
        .env("SVOLDER_HOST", "off")
        .env("SVOLDER_SYSTEMCTL", home.join("no-systemctl"))
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_STATE_HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("XDG_RUNTIME_DIR", home)
        .env_remove("SVOLDER_SOCKET")
        .env_remove("SVOLDER_MCP_TOKEN")
        .output()
        .expect("the binary runs");
    assert!(out.status.success(), "svolder mcp token failed");
    String::from_utf8(out.stdout)
        .unwrap_or_else(|_| panic!("a token is text"))
        .trim_end()
        .to_string()
}

/// One JSON-RPC call, as a 2026-07-28 client sends it, with `token` as the bearer.
async fn rpc(addr: SocketAddr, token: &str, method: &str, params: Value) -> Value {
    let mut params = params;
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "walkthrough", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    let name = params["name"]
        .as_str()
        .map(|n| format!("Mcp-Name: {n}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\n\
         MCP-Protocol-Version: 2026-07-28\r\nMcp-Method: {method}\r\n{name}\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(request.as_bytes()).await.expect("send");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, rest) = text.split_once("\r\n\r\n").expect("a response");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let rest = if head.to_ascii_lowercase().contains("chunked") {
        dechunk(rest)
    } else {
        rest.to_string()
    };
    serde_json::from_str(&rest).expect("a JSON answer")
}

fn dechunk(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some((size, after)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if n == 0 {
            break;
        }
        out.push_str(&after[..n]);
        rest = &after[n + 2..];
    }
    out
}

const READ_TOOLS: [&str; 4] = [
    "list_devices",
    "get_state",
    "get_link_history",
    "list_presets",
];
const WRITE_TOOLS: [&str; 4] = ["set_control", "load_preset", "mute", "identify"];

#[tokio::test]
async fn the_documented_flow_lists_the_tools_and_reads_the_state() {
    flow(true).await;
}

#[tokio::test]
async fn the_default_flow_offers_the_read_tools_only() {
    flow(false).await;
}

async fn flow(writes: bool) {
    let Some(binary) = svolder_binary() else {
        eprintln!(
            "skipped: no svolder beside svolderd; build the workspace (`make test`) to run this case"
        );
        return;
    };
    tokio::time::timeout(TEST_TIMEOUT, async {
        let config = format!(
            "{CONFIG}[mcp]\nenabled = true\nlisten = \"127.0.0.1:0\"\n{}",
            if writes { "writes = true\n" } else { "" }
        );
        let (mut child, socket) = spawn_daemon_isolated(&config, &[]);
        let mut client = connect(&mut child, &socket).await;
        common::wait_until_connected(&mut child, &mut client).await;
        let addr = 'wait: {
            for _ in 0..POLLS {
                if let Ok(Response::Version {
                    mcp: Some(McpStatus::Listening { listen, .. }),
                    ..
                }) = client.request(Request::Version).await
                {
                    break 'wait listen;
                }
                tokio::time::sleep(POLL).await;
            }
            panic!(
                "the server never listened; the daemon said {:?}",
                child.said()
            );
        };

        let home = child
            .config_path()
            .parent()
            .expect("a directory")
            .to_path_buf();
        let token = cli_token(&binary, &home);
        assert_eq!(token.len(), 43);

        let listed = rpc(addr, &token, "tools/list", json!({})).await;
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .expect("a tool list")
            .iter()
            .map(|t| t["name"].as_str().expect("a name"))
            .collect();
        let mut expected = READ_TOOLS.to_vec();
        if writes {
            expected.extend(WRITE_TOOLS);
        }
        assert_eq!(names, expected);

        let state = rpc(
            addr,
            &token,
            "tools/call",
            json!({"name": "get_state", "arguments": {}}),
        )
        .await;
        assert_eq!(state["result"]["isError"], json!(false));
        let state = &state["result"]["structuredContent"];
        assert_eq!(state["device"], json!("fake"));
        assert!(
            state["controls"]
                .as_array()
                .expect("controls")
                .iter()
                .any(|c| c["key"] == json!("volume"))
        );
    })
    .await
    .expect("test timed out");
}
