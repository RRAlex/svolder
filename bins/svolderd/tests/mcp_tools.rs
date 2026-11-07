// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MCP tools against a running daemon over the fake transport: what each answers, what
//! `writes = false` hides, and the volume rules for both kinds of client.

mod common;

use std::net::SocketAddr;
use std::path::Path;

use serde_json::{Value, json};
use svolder_ipc::{Client, McpStatus, Request, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use common::{CONFIG, POLL, POLLS, TEST_TIMEOUT, connect, spawn_daemon_isolated};

/// A running daemon with `[mcp]` on, its socket client and its server's address and token.
struct Rig {
    /// Held so the daemon lives as long as the rig.
    _child: common::ChildGuard,
    client: Client,
    addr: SocketAddr,
    token: String,
}

async fn rig(extra: &str) -> Rig {
    rig_on(extra, &[]).await
}

/// [`rig`] over a fake device with the environment `envs` gives it.
async fn rig_on(extra: &str, envs: &[(&str, &str)]) -> Rig {
    let config = format!("{CONFIG}[mcp]\nenabled = true\nlisten = \"127.0.0.1:0\"\n{extra}");
    rig_from(&config, envs).await
}

/// [`rig`] over a whole configuration file of the test's own.
async fn rig_with_config(config: &str) -> Rig {
    rig_from(config, &[]).await
}

async fn rig_from(config: &str, envs: &[(&str, &str)]) -> Rig {
    let (mut child, socket) = spawn_daemon_isolated(config, envs);
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
    let token = token_in(&child);
    Rig {
        _child: child,
        client,
        addr,
        token,
    }
}

fn token_in(child: &common::ChildGuard) -> String {
    let home: &Path = &child.config_path();
    std::fs::read_to_string(home.parent().unwrap().join("svolder").join("mcp-token"))
        .expect("the token file")
        .trim()
        .to_string()
}

/// The two kinds of client the volume rule tells apart.
#[derive(Clone, Copy)]
enum Kind {
    /// 2026-07-28, able to answer a question.
    Newer,
    /// 2025-11-25.
    Older,
    /// 2026-07-28, but declaring no way to answer a question.
    Mute,
    /// 2026-07-28, able to answer only a question that opens a web page.
    UrlOnly,
}

impl Rig {
    /// One JSON-RPC call of a client of `kind`, answered with the JSON body.
    async fn rpc(&self, kind: Kind, method: &str, params: Value) -> Value {
        let (version, meta) = match kind {
            Kind::UrlOnly => (
                "2026-07-28",
                json!({
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
                    "io.modelcontextprotocol/clientCapabilities": {"elicitation": {"url": {}}}
                }),
            ),
            Kind::Mute => (
                "2026-07-28",
                json!({
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
                    "io.modelcontextprotocol/clientCapabilities": {}
                }),
            ),
            Kind::Newer => (
                "2026-07-28",
                json!({
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
                    "io.modelcontextprotocol/clientCapabilities": {"elicitation": {"form": {}}}
                }),
            ),
            Kind::Older => (
                "2025-11-25",
                json!({
                    "io.modelcontextprotocol/protocolVersion": "2025-11-25",
                    "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
                    "io.modelcontextprotocol/clientCapabilities": {"elicitation": {"form": {}}}
                }),
            ),
        };
        let mut params = params;
        params["_meta"] = meta;
        let body =
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
        let name = params["name"]
            .as_str()
            .map(|n| format!("Mcp-Name: {n}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\
             Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\n\
             MCP-Protocol-Version: {version}\r\nMcp-Method: {method}\r\n{name}\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.addr,
            self.token,
            body.len()
        );
        let mut stream = TcpStream::connect(self.addr).await.expect("connect");
        stream.write_all(request.as_bytes()).await.expect("send");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.expect("read");
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, rest) = text.split_once("\r\n\r\n").expect("a response");
        let rest = if head.to_ascii_lowercase().contains("chunked") {
            dechunk(rest)
        } else {
            rest.to_string()
        };
        serde_json::from_str(&rest).unwrap_or_else(|e| panic!("not JSON ({e}): {head}\n{rest}"))
    }
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

impl Rig {
    /// A `tools/call` of `name` with `arguments`.
    async fn call(&self, kind: Kind, name: &str, arguments: Value) -> Value {
        self.rpc(
            kind,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        )
        .await
    }

    /// The volume the device holds, in dB, read over the daemon's own socket.
    async fn volume(&mut self) -> f64 {
        let reply = self
            .client
            .request(Request::Get {
                device: None,
                key: "volume".into(),
            })
            .await
            .expect("a reply");
        match reply {
            Response::Value(vo) => vo.value.expect("a level in dB"),
            other => panic!("{other:?}"),
        }
    }

    /// The raw reading of `key`.
    async fn raw(&mut self, key: &str) -> Value {
        match self
            .client
            .request(Request::Get {
                device: None,
                key: key.into(),
            })
            .await
            .expect("a reply")
        {
            Response::Value(vo) => vo.raw,
            other => panic!("{other:?}"),
        }
    }
}

/// The names of the tools a `tools/list` carries.
fn tool_names(list: &Value) -> Vec<String> {
    list["result"]["tools"]
        .as_array()
        .expect("a tool list")
        .iter()
        .map(|t| t["name"].as_str().expect("a name").to_string())
        .collect()
}

/// The structured answer of a successful call.
fn ok(reply: &Value) -> &Value {
    assert_eq!(reply["result"]["isError"], json!(false), "{reply:#}");
    &reply["result"]["structuredContent"]
}

/// The words of a tool error.
fn error_text(reply: &Value) -> String {
    assert_eq!(reply["result"]["isError"], json!(true), "{reply:#}");
    reply["result"]["content"][0]["text"]
        .as_str()
        .expect("a text")
        .to_string()
}

const READ_TOOLS: [&str; 5] = [
    "list_devices",
    "get_state",
    "get_link_history",
    "list_presets",
    "get_host",
];
const WRITE_TOOLS: [&str; 7] = [
    "set_control",
    "load_preset",
    "mute",
    "identify",
    "set_host_volume",
    "host_mute",
    "set_host_eq",
];

#[tokio::test]
async fn the_read_tools_answer_with_device_data() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let rig = rig("").await;
        let listed = rig.rpc(Kind::Newer, "tools/list", json!({})).await;
        assert_eq!(tool_names(&listed), READ_TOOLS);

        let devices = rig.call(Kind::Newer, "list_devices", json!({})).await;
        // The fake advertises at -50 dBm; the stamp is the daemon's clock, whatever it was.
        let mut devices = ok(&devices).clone();
        let heard = devices["devices"][0]["signal"]["heard_at_ms"].take();
        assert!(heard.as_u64().is_some_and(|ms| ms > 0), "{heard}");
        assert_eq!(
            devices,
            json!({"devices": [{
                "name": "fake",
                "product": "Gungnir 2",
                "connected": true,
                "signal": {"dbm": -50, "source": "advert", "heard_at_ms": null},
            }]})
        );

        let state = rig.call(Kind::Newer, "get_state", json!({})).await;
        let state = ok(&state);
        assert_eq!(state["device"], "fake");
        let controls = state["controls"].as_array().expect("controls");
        let control = |key: &str| {
            controls
                .iter()
                .find(|c| c["key"] == key)
                .unwrap_or_else(|| panic!("no {key} in {state:#}"))
        };
        let volume = control("volume");
        assert_eq!(volume["kind"], "level");
        assert_eq!(volume["unit"], "dB");
        assert_eq!(volume["label"], "Volume");
        assert!(volume["value"].is_number(), "{volume}");
        assert_eq!(volume["range"]["max"], json!(0.0));
        assert_eq!(control("input")["kind"], "select");
        assert_eq!(control("input")["values"][0], "USB 1");
        assert_eq!(control("mute")["kind"], "toggle");
        assert_eq!(control("mute")["value"], json!(false));

        let history = rig
            .call(Kind::Newer, "get_link_history", json!({"hours": 1}))
            .await;
        let history = ok(&history);
        assert_eq!(history["device"], "fake");
        let last = history["transitions"]
            .as_array()
            .expect("transitions")
            .last()
            .cloned();
        assert_eq!(last.expect("the connect")["connected"], json!(true));

        let presets = rig.call(Kind::Newer, "list_presets", json!({})).await;
        let names: Vec<&str> = ok(&presets)["presets"]
            .as_array()
            .expect("presets")
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Flat", "Warm", "Bass", "Presence"]);

        // Device data only: nothing about what the host is playing.
        for reply in [&devices, state, history, &presets] {
            let text = reply.to_string().to_ascii_lowercase();
            for word in ["mpris", "artist", "track", "player", "album"] {
                assert!(!text.contains(word), "{word} in {text}");
            }
        }
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn the_history_span_is_bounded_to_a_day() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let rig = rig("").await;
        for hours in [json!(25), json!(0), json!(-1)] {
            let reply = rig
                .call(Kind::Newer, "get_link_history", json!({"hours": hours}))
                .await;
            assert!(error_text(&reply).contains("at most 24"), "{reply:#}");
        }
        let reply = rig
            .call(Kind::Newer, "get_link_history", json!({"hours": 24}))
            .await;
        ok(&reply);
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn without_writes_the_write_tools_are_hidden_and_refused() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("").await;
        let listed = rig.rpc(Kind::Newer, "tools/list", json!({})).await;
        let names = tool_names(&listed);
        for tool in WRITE_TOOLS {
            assert!(
                !names.iter().any(|n| n == tool),
                "{tool} is listed: {names:?}"
            );
        }
        let before = rig.volume().await;
        let calls = [
            ("set_control", json!({"key": "volume", "value": -50})),
            ("load_preset", json!({"name": "Warm"})),
            ("mute", json!({"on": true})),
            ("identify", json!({})),
            ("set_host_volume", json!({"percent": 10})),
            ("host_mute", json!({"muted": true})),
            ("set_host_eq", json!({"off": true})),
        ];
        for (tool, arguments) in calls {
            for kind in [Kind::Newer, Kind::Older] {
                let reply = rig.call(kind, tool, arguments.clone()).await;
                let text = error_text(&reply);
                assert!(text.contains("[mcp] writes"), "{tool}: {text}");
            }
        }
        assert_eq!(rig.volume().await, before);
        assert_eq!(rig.raw("mute").await, json!(false));
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn with_writes_the_tools_set_a_toggle_a_select_a_level_a_preset_and_the_mute() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let listed = rig.rpc(Kind::Newer, "tools/list", json!({})).await;
        assert_eq!(
            tool_names(&listed),
            [READ_TOOLS.as_slice(), WRITE_TOOLS.as_slice()].concat()
        );

        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "phase", "value": true}),
            )
            .await;
        assert_eq!(ok(&reply)["key"], "phase");
        assert_eq!(rig.raw("phase").await, json!(true));

        rig.call(
            Kind::Newer,
            "set_control",
            json!({"key": "input", "value": "Optical"}),
        )
        .await;
        assert_eq!(rig.raw("input").await, json!(3));

        // A level that is not the volume goes through whatever the jump: a balance of 36 steps
        // right of its centre of 64 is raw step 100.
        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "balance", "value": 36}),
            )
            .await;
        ok(&reply);
        assert_eq!(rig.raw("balance").await, json!(100));

        let reply = rig.call(Kind::Newer, "mute", json!({"on": true})).await;
        assert_eq!(ok(&reply)["key"], "mute");
        assert_eq!(rig.raw("mute").await, json!(true));
        rig.call(Kind::Older, "mute", json!({"on": false, "confirm": true}))
            .await;
        assert_eq!(rig.raw("mute").await, json!(false));

        // The cue goes out once; a second call inside the session's gap sends nothing.
        let reply = rig.call(Kind::Newer, "identify", json!({})).await;
        assert_eq!(ok(&reply), &json!({"device": "fake", "sent": true}));
        let reply = rig.call(Kind::Newer, "identify", json!({})).await;
        assert_eq!(ok(&reply), &json!({"device": "fake", "sent": false}));

        let reply = rig
            .call(Kind::Newer, "load_preset", json!({"name": "bass"}))
            .await;
        assert_eq!(ok(&reply)["preset"], "Bass", "{reply:#}");
        assert!(ok(&reply)["writes"].as_u64().unwrap() > 0);
        let shown = rig
            .client
            .request(Request::EqShow { device: None })
            .await
            .unwrap();
        let Response::Values { values, .. } = shown else {
            panic!("{shown:?}")
        };
        assert!(!values.is_empty());
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_confirm_change_of_six_db_or_less_goes_through() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let start = rig.volume().await;
        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": start + 4.0}),
            )
            .await;
        ok(&reply);
        let now = rig.volume().await;
        assert!((now - (start + 4.0)).abs() < 1.0, "{start} -> {now}");
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_newer_client_is_asked_and_the_rise_goes_through_when_it_answers_yes() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let start = rig.volume().await;
        let arguments = json!({"key": "volume", "value": -10});

        let asked = rig
            .call(Kind::Newer, "set_control", arguments.clone())
            .await;
        let result = &asked["result"];
        assert_eq!(result["resultType"], "input_required", "{asked:#}");
        let state = result["requestState"]
            .as_str()
            .expect("a state")
            .to_string();
        let question = &result["inputRequests"]["confirm_change"];
        let message = question["params"]["message"].as_str().expect("a message");
        assert!(message.contains("fake"), "{message}");
        assert!(message.contains(&format!("{start:.1} dB")), "{message}");
        assert!(
            message.contains("-10.0 dB") || message.contains("-9.9 dB"),
            "{message}"
        );
        assert_eq!(rig.volume().await, start, "nothing was set by asking");

        let retry = |answer: Value, state: &str| {
            json!({
                "name": "set_control",
                "arguments": arguments.clone(),
                "inputResponses": {"confirm_change": answer},
                "requestState": state
            })
        };
        // A no, an answer with the wrong state, and no answer at all set nothing.
        let declined = rig
            .rpc(
                Kind::Newer,
                "tools/call",
                retry(json!({"action": "decline"}), &state),
            )
            .await;
        assert!(error_text(&declined).contains("declined"), "{declined:#}");
        let accept = json!({"action": "accept", "content": {"confirm": true}});
        let stale = rig
            .rpc(
                Kind::Newer,
                "tools/call",
                retry(accept.clone(), "level|fake|volume|1|none"),
            )
            .await;
        assert_eq!(stale["result"]["resultType"], "input_required", "{stale:#}");
        let unchecked = rig
            .rpc(
                Kind::Newer,
                "tools/call",
                retry(
                    json!({"action": "accept", "content": {"confirm": false}}),
                    &state,
                ),
            )
            .await;
        assert!(error_text(&unchecked).contains("declined"), "{unchecked:#}");
        assert_eq!(rig.volume().await, start);

        let done = rig
            .rpc(Kind::Newer, "tools/call", retry(accept, &state))
            .await;
        ok(&done);
        let now = rig.volume().await;
        assert!((now + 10.0).abs() < 1.0, "{now}");
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn an_older_client_gets_a_tool_error_and_succeeds_with_confirm() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let start = rig.volume().await;
        let arguments = json!({"key": "volume", "value": -10});
        let refused = rig.call(Kind::Older, "set_control", arguments).await;
        let text = error_text(&refused);
        assert!(text.contains("\"confirm\": true"), "{text}");
        assert!(text.contains("fake"), "{text}");
        assert_eq!(rig.volume().await, start);

        let confirmed = rig
            .call(
                Kind::Older,
                "set_control",
                json!({"key": "volume", "value": -10, "confirm": true}),
            )
            .await;
        ok(&confirmed);
        assert!((rig.volume().await + 10.0).abs() < 1.0);
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_newer_client_that_cannot_answer_gets_the_confirm_error() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let start = rig.volume().await;
        let arguments = json!({"key": "volume", "value": -10});
        let refused = rig.call(Kind::Mute, "set_control", arguments).await;
        assert!(
            error_text(&refused).contains("\"confirm\": true"),
            "{refused:#}"
        );
        assert_eq!(rig.volume().await, start);
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_lowering_never_asks_and_the_cap_holds_even_when_confirmed() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\nmax_volume_db = -20.0\n").await;
        // Lowering from -26 dB by 30 dB: no question.
        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": -60}),
            )
            .await;
        ok(&reply);
        let low = rig.volume().await;
        assert!((low + 60.0).abs() < 1.0, "{low}");

        for kind in [Kind::Newer, Kind::Older] {
            for confirm in [false, true] {
                let reply = rig
                    .call(
                        kind,
                        "set_control",
                        json!({"key": "volume", "value": -10, "confirm": confirm}),
                    )
                    .await;
                let text = error_text(&reply);
                assert!(text.contains("max_volume_db"), "{text}");
                assert!(text.contains("-20.0 dB"), "{text}");
            }
        }
        // A string with its unit counts the same.
        let reply = rig
            .call(
                Kind::Older,
                "set_control",
                json!({"key": "volume", "value": "-5 dB", "confirm": true}),
            )
            .await;
        assert!(error_text(&reply).contains("max_volume_db"));
        assert_eq!(rig.volume().await, low);

        // A rise that stays under the cap, from a client that cannot ask, with the user's yes.
        let reply = rig
            .call(
                Kind::Older,
                "set_control",
                json!({"key": "volume", "value": -30, "confirm": true}),
            )
            .await;
        ok(&reply);
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn words_and_numbers_outside_the_range_are_refused_for_the_volume() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let start = rig.volume().await;
        for word in ["up", "max", "loud"] {
            let reply = rig
                .call(
                    Kind::Older,
                    "set_control",
                    json!({"key": "volume", "value": word, "confirm": true}),
                )
                .await;
            assert!(
                error_text(&reply).contains("number in dB"),
                "{word}: {reply:#}"
            );
        }
        for value in [json!(3), json!(1e308), json!(-1e308)] {
            let reply = rig
                .call(
                    Kind::Older,
                    "set_control",
                    json!({"key": "volume", "value": value, "confirm": true}),
                )
                .await;
            let text = error_text(&reply);
            assert!(text.contains("between -96.33 and 0"), "{text}");
        }
        assert_eq!(rig.volume().await, start);
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn an_unknown_device_or_key_names_the_valid_ones() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let rig = rig("writes = true\n").await;
        let reply = rig
            .call(Kind::Newer, "get_state", json!({"device": "nowhere"}))
            .await;
        let text = error_text(&reply);
        assert!(text.contains("nowhere") && text.contains("fake"), "{text}");

        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "vollume", "value": -50}),
            )
            .await;
        let text = error_text(&reply);
        assert!(text.contains("vollume"), "{text}");
        for key in ["volume", "mute", "input", "balance"] {
            assert!(text.contains(key), "{key} missing from {text}");
        }
        assert!(
            !text.contains("sample_rate"),
            "a read-only key is offered: {text}"
        );

        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "sample_rate", "value": 1}),
            )
            .await;
        assert!(error_text(&reply).contains("read-only"));

        let reply = rig
            .call(Kind::Newer, "load_preset", json!({"name": "nonesuch"}))
            .await;
        let text = error_text(&reply);
        assert!(text.contains("Warm") && text.contains("Presence"), "{text}");

        let reply = rig.call(Kind::Newer, "no_such_tool", json!({})).await;
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("list_devices"),
            "{reply:#}"
        );

        let reply = rig
            .call(Kind::Newer, "list_devices", json!({"surprise": 1}))
            .await;
        assert!(error_text(&reply).contains("surprise"));
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_model_written_confirm_does_not_skip_the_question_of_a_client_that_can_ask() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let start = rig.volume().await;
        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": 0, "confirm": true}),
            )
            .await;
        assert_eq!(reply["result"]["resultType"], "input_required", "{reply:#}");
        assert_eq!(rig.volume().await, start);
        // A client that can answer only a web page cannot be asked, so it is told to confirm.
        let reply = rig
            .call(
                Kind::UrlOnly,
                "set_control",
                json!({"key": "volume", "value": 0}),
            )
            .await;
        assert!(
            error_text(&reply).contains("\"confirm\": true"),
            "{reply:#}"
        );
        assert_eq!(rig.volume().await, start);
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_lowering_goes_through_above_the_cap_and_small_steps_add_up() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\nmax_volume_db = -40.0\n").await;
        // The device sits near -26 dB, above the cap: a smaller drop is allowed.
        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": -30}),
            )
            .await;
        ok(&reply);
        assert!((rig.volume().await + 30.0).abs() < 1.0);
        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": -28}),
            )
            .await;
        assert!(error_text(&reply).contains("max_volume_db"), "{reply:#}");

        // Without a cap: +5, then +5 again within the minute, asks.
        let mut rig = rig_on("writes = true\n", &[]).await;
        let start = rig.volume().await;
        let first = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": start + 5.0}),
            )
            .await;
        ok(&first);
        let second = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": start + 10.0}),
            )
            .await;
        assert_eq!(
            second["result"]["resultType"], "input_required",
            "{second:#}"
        );
    })
    .await
    .expect("test timed out");
}

/// A changed `[mcp]` restarts the listener, and the last minute of levels survives it: a rise
/// that adds up with one made before the restart still asks.
#[tokio::test]
async fn a_restart_of_the_listener_keeps_the_last_minute_of_levels() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let start = rig.volume().await;
        let first = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": start + 5.0}),
            )
            .await;
        ok(&first);
        // A cap high enough to change nothing about this rise changes the section.
        let path = rig._child.config_path();
        let config = std::fs::read_to_string(&path).expect("the config");
        std::fs::write(&path, format!("{config}max_volume_db = 0.0\n")).expect("write the config");
        assert!(matches!(
            rig.client.request(Request::Reload).await,
            Ok(Response::Reloaded { .. })
        ));
        let Ok(Response::Version {
            mcp: Some(McpStatus::Listening { listen, .. }),
            ..
        }) = rig.client.request(Request::Version).await
        else {
            panic!("the server is not listening");
        };
        rig.addr = listen;
        let second = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "volume", "value": start + 10.0}),
            )
            .await;
        assert_eq!(
            second["result"]["resultType"], "input_required",
            "{second:#}"
        );
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn an_unmute_asks_through_both_tools() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig("writes = true\n").await;
        let reply = rig.call(Kind::Newer, "mute", json!({"on": true})).await;
        ok(&reply);
        assert_eq!(rig.raw("mute").await, json!(true));
        let reply = rig.call(Kind::Newer, "mute", json!({"on": false})).await;
        assert_eq!(reply["result"]["resultType"], "input_required", "{reply:#}");
        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "mute", "value": false}),
            )
            .await;
        assert_eq!(reply["result"]["resultType"], "input_required", "{reply:#}");
        assert_eq!(rig.raw("mute").await, json!(true));
        let reply = rig
            .call(Kind::Older, "mute", json!({"on": false, "confirm": true}))
            .await;
        ok(&reply);
        assert_eq!(rig.raw("mute").await, json!(false));
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_gain_select_asks_when_raised() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let rig = rig_on("writes = true\n", &[("SVOLDER_FAKE_PROFILE", "hati")]).await;
        let down = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "gain", "value": "40 dB"}),
            )
            .await;
        ok(&down);
        let up = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "gain", "value": "70 dB"}),
            )
            .await;
        assert_eq!(up["result"]["resultType"], "input_required", "{up:#}");
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_broken_configuration_is_never_quoted_to_the_client() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let rig = rig("").await;
        std::fs::write(
            rig._child.config_path(),
            "schema = 1\n[mqtt]\nhost = \"b\"\npassword = hunter2-secret\nport = =\n",
        )
        .expect("break the config");
        let reply = rig.call(Kind::Newer, "list_presets", json!({})).await;
        let text = error_text(&reply);
        assert!(text.contains("could not be read"), "{text}");
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("mqtt"), "{text}");
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn a_configured_input_label_works_in_both_directions() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let config = format!(
            "{}[mcp]\nenabled = true\nlisten = \"127.0.0.1:0\"\nwrites = true\n",
            CONFIG.replace(
                "udid = \"0102030405060708\"\n",
                "udid = \"0102030405060708\"\ninputs = { 3 = \"Turntable\" }\n"
            )
        );
        let rig = rig_with_config(&config).await;
        let state = rig.call(Kind::Newer, "get_state", json!({})).await;
        let controls = ok(&state)["controls"].as_array().unwrap().clone();
        let input = controls.iter().find(|c| c["key"] == "input").unwrap();
        assert!(input["values"].to_string().contains("Turntable"), "{input}");
        let reply = rig
            .call(
                Kind::Newer,
                "set_control",
                json!({"key": "input", "value": "Turntable"}),
            )
            .await;
        ok(&reply);
    })
    .await
    .expect("test timed out");
}

/// Bass Boost on a Loki Max raises its first band by more than 6 dB: a client that can answer is
/// asked, nothing is written by asking or by a no, and a yes applies the three bands the setting
/// changes, which the device then reads.
#[tokio::test]
async fn load_preset_bass_boost_over_http_asks_then_applies() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mut rig = rig_on("writes = true\n", &[("SVOLDER_FAKE_PROFILE", "lokimax")]).await;
        let listed = rig.call(Kind::Newer, "list_presets", json!({})).await;
        let rows = ok(&listed)["presets"].as_array().expect("a list").clone();
        assert_eq!(rows.len(), 8, "{listed:#}");
        assert_eq!(rows[0]["name"], "Preset 1");
        assert_eq!(rows[3]["name"], "Flat");

        let arguments = json!({"name": "Bass Boost"});
        let asked = rig
            .call(Kind::Newer, "load_preset", arguments.clone())
            .await;
        let result = &asked["result"];
        assert_eq!(result["resultType"], "input_required", "{asked:#}");
        let state = result["requestState"]
            .as_str()
            .expect("a state")
            .to_string();
        assert_eq!(state, "setting|fake|Bass Boost|band1=10.4@0.0");
        let message = result["inputRequests"]["confirm_change"]["params"]["message"]
            .as_str()
            .expect("a message");
        assert!(
            message.contains("20 Hz") && message.contains("10.4 dB"),
            "{message}"
        );
        assert_eq!(
            rig.raw("band1").await,
            json!(15),
            "nothing was set by asking"
        );

        let retry = |answer: Value| {
            json!({
                "name": "load_preset",
                "arguments": arguments.clone(),
                "inputResponses": {"confirm_change": answer},
                "requestState": state
            })
        };
        let declined = rig
            .rpc(
                Kind::Newer,
                "tools/call",
                retry(json!({"action": "decline"})),
            )
            .await;
        assert!(error_text(&declined).contains("declined"), "{declined:#}");
        assert_eq!(rig.raw("band1").await, json!(15));

        let accept = json!({"action": "accept", "content": {"confirm": true}});
        let done = rig.rpc(Kind::Newer, "tools/call", retry(accept)).await;
        assert_eq!(ok(&done)["preset"], "Bass Boost", "{done:#}");
        assert_eq!(ok(&done)["writes"], json!(3), "{done:#}");
        for (key, raw) in [("band1", 28), ("band2", 21), ("band3", 17)] {
            assert_eq!(rig.raw(key).await, json!(raw), "{key}");
        }
        // A slot recall asks too, and goes through for a client that cannot ask on `confirm`.
        let refused = rig
            .call(Kind::Mute, "load_preset", json!({"name": "Preset 1"}))
            .await;
        assert!(error_text(&refused).contains("12.0 dB"), "{refused:#}");
        let recalled = rig
            .call(
                Kind::Mute,
                "load_preset",
                json!({"name": "Preset 1", "confirm": true}),
            )
            .await;
        assert_eq!(ok(&recalled)["slot"], json!(1), "{recalled:#}");
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn the_computer_tools_read_and_change_the_fake_host() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let rig = rig_on("writes = true\n", &[("SVOLDER_HOST", "fake")]).await;
        let host = rig.call(Kind::Newer, "get_host", json!({})).await;
        assert_eq!(ok(&host)["volume"]["percent"], json!(42), "{host:#}");
        assert_eq!(ok(&host)["eq"]["on"], json!(false), "{host:#}");

        let lowered = rig
            .call(Kind::Newer, "set_host_volume", json!({"percent": 30}))
            .await;
        assert_eq!(ok(&lowered)["volume"]["percent"], json!(30), "{lowered:#}");
        let raised = rig
            .call(Kind::Older, "set_host_volume", json!({"percent": 90}))
            .await;
        assert!(error_text(&raised).contains("confirm"), "{raised:#}");
        let above = rig
            .call(Kind::Newer, "set_host_volume", json!({"percent": 101}))
            .await;
        assert!(error_text(&above).contains("100 %"), "{above:#}");
        let muted = rig
            .call(Kind::Newer, "host_mute", json!({"muted": true}))
            .await;
        assert_eq!(ok(&muted)["volume"]["muted"], json!(true), "{muted:#}");
        let after = rig.call(Kind::Newer, "get_host", json!({})).await;
        assert_eq!(ok(&after)["volume"]["percent"], json!(30), "{after:#}");
        assert_eq!(ok(&after)["volume"]["muted"], json!(true), "{after:#}");
        let off = rig
            .call(Kind::Newer, "set_host_eq", json!({"off": true}))
            .await;
        assert_eq!(ok(&off)["eq"]["on"], json!(false), "{off:#}");
    })
    .await
    .expect("test timed out");
}
