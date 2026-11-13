// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `server_switch` request on a daemon with no configuration file, as a container's first
//! start is: the listen addresses come from the environment (here loopback ports the system
//! picks), and turning the web UI on makes the file from the template and binds the listener.

mod common;

use svolder_ipc::{Config, McpStatus, Request, Response, Server, WebStatus};

#[tokio::test]
async fn web_on_with_no_configuration_file_makes_one_and_binds_the_listener() {
    let env = [
        ("SVOLDER_WEB_LISTEN", "127.0.0.1:0"),
        ("SVOLDER_MCP_LISTEN", "127.0.0.1:0"),
    ];
    let (mut guard, socket) = common::spawn_daemon_isolated_without_file(&env);
    let mut client = common::connect(&mut guard, &socket).await;
    let path = guard.config_path();
    assert!(!path.exists(), "the daemon starts with no file");

    let answer = client
        .request(Request::ServerSwitch {
            server: Server::Web,
            enabled: true,
        })
        .await
        .expect("an answer");
    let Response::ServerSwitch { web, mcp } = answer else {
        panic!(
            "not the servers: {answer:?}; the daemon said {:?}",
            guard.said()
        );
    };
    let WebStatus::Listening { listen, .. } = web else {
        panic!("not listening: {web:?}; the daemon said {:?}", guard.said());
    };
    assert!(listen.ip().is_loopback() && listen.port() != 0, "{listen}");
    assert_eq!(mcp, McpStatus::Off);
    std::net::TcpStream::connect(listen).expect("the listener answers");

    let text = std::fs::read_to_string(&path).expect("the file is made");
    assert!(
        text.starts_with("# The svolder configuration."),
        "made from the template: {text}"
    );
    let enabled: Vec<&str> = text.lines().filter(|l| l.starts_with("enabled")).collect();
    assert_eq!(
        enabled,
        ["enabled = true"],
        "the one live key besides schema"
    );
    let web_at = text.find("\n[web]\n").expect("[web]");
    assert!(text.find("\nenabled = true\n").expect("the key") > web_at);
    assert!(Config::parse(&text).expect("it parses").web.enabled);
}
