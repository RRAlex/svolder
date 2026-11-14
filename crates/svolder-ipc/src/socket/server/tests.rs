// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The server's tests: requests and framing here, streams in `streams`.

use super::*;
use crate::socket::protocol::*;
use crate::test_support::{Fake, start_server};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::AsyncBufReadExt;

mod streams;

#[tokio::test]
async fn state_request_gets_a_response() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let server = start_server(Arc::new(Fake::new())).await;
        let mut client = crate::socket::client::Client::connect(&server.path)
            .await
            .unwrap();
        let resp = client
            .request(Request::State { device: None })
            .await
            .unwrap();
        assert!(matches!(resp, Response::Values { values, .. } if values.len() == 1));
    })
    .await
    .expect("test timed out");
}

/// Every kind the `Request` enum declares is in [`REQUEST_CMDS`] (read from the enum's source),
/// and every entry is a kind serde knows (so a rename or alias cannot leave the list naming
/// what the enum no longer decodes): a variant added without its entry would be refused as
/// unknown.
#[test]
fn the_request_kinds_list_is_the_enums() {
    let source = include_str!("../protocol/request.rs");
    let body = source
        .split("pub enum Request {")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .expect("the Request enum");
    let declared: Vec<String> = body
        .lines()
        .filter(|l| l.starts_with("    ") && !l.starts_with("     "))
        .filter_map(|l| l.trim().split(|c: char| !c.is_alphanumeric()).next())
        .filter(|w| w.starts_with(|c: char| c.is_ascii_uppercase()))
        .map(|w| {
            let mut out = String::new();
            for (at, c) in w.chars().enumerate() {
                if c.is_ascii_uppercase() && at > 0 {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            }
            out
        })
        .collect();
    assert_eq!(
        declared, REQUEST_CMDS,
        "REQUEST_CMDS is the enum's kinds in order"
    );
    for cmd in REQUEST_CMDS {
        let line = format!("{{\"id\":1,\"request\":{{\"cmd\":\"{cmd}\"}}}}");
        assert_eq!(unknown_request(&line), None, "{cmd}");
        // Test code may read serde's words: a kind serde knows never fails as unknown.
        let probe = serde_json::from_value::<Request>(serde_json::json!({ "cmd": cmd }));
        let unknown = probe
            .err()
            .is_some_and(|e| e.to_string().starts_with("unknown variant"));
        assert!(!unknown, "{cmd} is not a kind of Request");
    }
    assert_eq!(
        unknown_request("{\"request\":{\"cmd\":\"zzz\"}}"),
        Some("zzz".into())
    );
}

/// A request kind the server does not know is answered with its own code, and a known kind
/// with a field it cannot read keeps the usage code.
#[tokio::test]
async fn an_unknown_request_has_its_own_code_and_a_malformed_one_keeps_usage() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let server = start_server(Arc::new(Fake::new())).await;
        let stream = UnixStream::connect(&server.path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let mut next = async move |write: &mut tokio::net::unix::OwnedWriteHalf, line: &str| {
            write.write_all(line.as_bytes()).await.unwrap();
            let answer = lines.next_line().await.unwrap().expect("an answer");
            serde_json::from_str::<Reply>(&answer).expect("a reply line")
        };
        let unknown = next(&mut write, "{\"id\":7,\"request\":{\"cmd\":\"nope\"}}\n").await;
        let Reply::Response {
            id: 7,
            response: Response::Error { code, message, .. },
        } = unknown
        else {
            panic!("an error answer, got {unknown:?}");
        };
        assert_eq!(code, CODE_UNKNOWN_REQUEST);
        assert!(message.contains("`nope`"), "{message}");
        let malformed = next(
            &mut write,
            "{\"id\":8,\"request\":{\"cmd\":\"state\",\"device\":5}}\n",
        )
        .await;
        assert!(
            matches!(
                &malformed,
                Reply::Response {
                    id: 8,
                    response: Response::Error {
                        code: CODE_USAGE,
                        ..
                    }
                }
            ),
            "got {malformed:?}"
        );
        // No tag at all, or one that is no string: malformed, not unknown.
        for line in [
            "{\"id\":10,\"request\":{}}\n",
            "{\"id\":11,\"request\":{\"cmd\":5}}\n",
            "{\"id\":12}\n",
        ] {
            let answer = next(&mut write, line).await;
            assert!(
                matches!(
                    &answer,
                    Reply::Response {
                        response: Response::Error {
                            code: CODE_USAGE,
                            ..
                        },
                        ..
                    }
                ),
                "{line} got {answer:?}"
            );
        }
        let missing = next(&mut write, "{\"id\":9,\"request\":{\"cmd\":\"set\"}}\n").await;
        assert!(
            matches!(
                &missing,
                Reply::Response {
                    id: 9,
                    response: Response::Error {
                        code: CODE_USAGE,
                        ..
                    }
                }
            ),
            "got {missing:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A line the server cannot parse earns an `Error` response, not a closed connection:
/// the client is told what was wrong and the next request is answered as usual.
#[tokio::test]
async fn a_malformed_line_is_answered_and_the_connection_keeps_serving() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let server = start_server(Arc::new(Fake::new())).await;
        let stream = UnixStream::connect(&server.path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let mut next = async move |write: &mut tokio::net::unix::OwnedWriteHalf, line: &str| {
            write.write_all(line.as_bytes()).await.unwrap();
            let answer = lines
                .next_line()
                .await
                .unwrap()
                .expect("the server answered rather than hanging up");
            serde_json::from_str::<Reply>(&answer).expect("a reply line")
        };

        // Not JSON at all: nothing to correlate the complaint with, so id 0.
        let first = next(&mut write, "not json\n").await;
        assert!(
            matches!(&first, Reply::Response { id: 0, response: Response::Error { code: CODE_USAGE, message, .. } }
                if message.starts_with("bad request: ")),
            "got {first:?}"
        );

        // JSON carrying an id, but not a request this build knows: the id comes back, so
        // a client can tell which of its lines was refused.
        let second = next(&mut write, "{\"id\":7,\"request\":{\"cmd\":\"nope\"}}\n").await;
        assert!(
            matches!(
                &second,
                Reply::Response {
                    id: 7,
                    response: Response::Error { code: CODE_UNKNOWN_REQUEST, .. }
                }
            ),
            "got {second:?}"
        );

        // The connection is still serving: a real request is answered.
        let line = to_line(&RequestLine {
            id: 9,
            request: Request::State { device: None },
        })
        .unwrap();
        let third = next(&mut write, &line).await;
        assert!(
            matches!(
                &third,
                Reply::Response {
                    id: 9,
                    response: Response::Values { .. }
                }
            ),
            "got {third:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A line over the cap is answered with an error naming the cap, and the connection serves
/// the next line: the oversized one is skipped, not parsed and not the end of the session.
#[tokio::test]
async fn an_over_long_line_is_answered_and_the_connection_keeps_serving() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let server = start_server(Arc::new(Fake::new())).await;
        let stream = UnixStream::connect(&server.path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();

        let mut long = vec![b'a'; crate::socket::lines::MAX_LINE + 4096];
        long.push(b'\n');
        let ask = to_line(&RequestLine {
            id: 9,
            request: Request::State { device: None },
        })
        .unwrap();
        long.extend_from_slice(ask.as_bytes());
        write.write_all(&long).await.unwrap();

        let first: Reply =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).expect("a reply line");
        let Reply::Response {
            response:
                Response::Error {
                    code: CODE_USAGE,
                    message,
                    ..
                },
            ..
        } = &first
        else {
            panic!("got {first:?}");
        };
        assert!(
            message.contains(&crate::socket::lines::MAX_LINE.to_string()),
            "the error names the cap: {message}"
        );
        let second: Reply =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).expect("a reply line");
        assert!(
            matches!(
                &second,
                Reply::Response {
                    id: 9,
                    response: Response::Values { .. }
                }
            ),
            "got {second:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A request line that is not UTF-8 is answered with an error, not a closed connection.
#[tokio::test]
async fn a_non_utf8_line_is_answered_and_the_connection_keeps_serving() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let server = start_server(Arc::new(Fake::new())).await;
        let stream = UnixStream::connect(&server.path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();

        write.write_all(b"{\"id\":3,\"\xff\xfe\n").await.unwrap();
        let first: Reply = serde_json::from_str(
            &lines
                .next_line()
                .await
                .unwrap()
                .expect("the server answered rather than hanging up"),
        )
        .expect("a reply line");
        assert!(
            matches!(&first, Reply::Response { response: Response::Error { code: CODE_USAGE, message, .. }, .. }
                if message.contains("UTF-8")),
            "got {first:?}"
        );

        let ask = to_line(&RequestLine {
            id: 4,
            request: Request::State { device: None },
        })
        .unwrap();
        write.write_all(ask.as_bytes()).await.unwrap();
        let second: Reply = serde_json::from_str(&lines.next_line().await.unwrap().unwrap())
            .expect("a reply line");
        assert!(
            matches!(
                &second,
                Reply::Response {
                    id: 4,
                    response: Response::Values { .. }
                }
            ),
            "got {second:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A last request line without its newline, followed by a half-close, is still answered.
#[tokio::test]
async fn an_unterminated_last_line_is_answered_after_a_half_close() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let server = start_server(Arc::new(Fake::new())).await;
        let stream = UnixStream::connect(&server.path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();

        let ask = to_line(&RequestLine {
            id: 5,
            request: Request::State { device: None },
        })
        .unwrap();
        write.write_all(ask.trim_end().as_bytes()).await.unwrap();
        write.shutdown().await.unwrap();
        let reply: Reply = serde_json::from_str(
            &lines
                .next_line()
                .await
                .unwrap()
                .expect("the server answered the unterminated line"),
        )
        .expect("a reply line");
        assert!(
            matches!(
                &reply,
                Reply::Response {
                    id: 5,
                    response: Response::Values { .. }
                }
            ),
            "got {reply:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A handler answering with the largest messages the protocol carries: every device the
/// daemon could list, and every reading a profile could report.
struct Big;

#[async_trait::async_trait]
impl Handler for Big {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Devices => Response::Devices {
                devices: (0..500)
                    .map(|i| DeviceInfo {
                        signal: None,
                        name: format!("a-device-named-at-some-length-{i}"),
                        unit: format!("{i:016x}"),
                        product_id: 0x1234,
                        product: Some("Gungnir Multibit 2".into()),
                        firmware: 300,
                        profile: "gungnir2-v2".into(),
                        connected: true,
                        stale: false,
                        retry_in_secs: Some(60),
                        passphrase_note: None,
                        refused: false,
                        recalled: None,
                        key_readings: Vec::new(),
                    })
                    .collect(),
            },
            Request::State { .. } => Response::Values {
                device: "fake".into(),
                values: (0..300)
                    .map(|i| crate::test_support::vo(&format!("{i}"), "a_long_reading_key"))
                    .collect(),
            },
            _ => Response::Ack,
        }
    }

    async fn subscribe(
        &self,
        _device: Option<String>,
    ) -> futures::stream::BoxStream<'static, Event> {
        futures::stream::pending().boxed()
    }
}

/// The largest legitimate messages stay well inside the cap on both ends: a listing of
/// hundreds of devices, a state of hundreds of readings, and a full `EqSet` request all
/// pass through the real client and server.
#[tokio::test]
async fn the_largest_legitimate_messages_pass() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let server = start_server(Arc::new(Big)).await;
        let mut client = crate::socket::client::Client::connect(&server.path)
            .await
            .unwrap();

        let devices = client.request(Request::Devices).await.unwrap();
        let state = client
            .request(Request::State { device: None })
            .await
            .unwrap();
        let Response::Devices { devices: listed } = &devices else {
            panic!("got {devices:?}");
        };
        assert_eq!(listed.len(), 500);
        let Response::Values { values, .. } = &state else {
            panic!("got {state:?}");
        };
        assert_eq!(values.len(), 300);
        for message in [to_line(&devices).unwrap(), to_line(&state).unwrap()] {
            assert!(
                message.len() < crate::socket::lines::MAX_LINE / 2,
                "{} bytes is not comfortably inside the cap",
                message.len()
            );
        }

        let eq_set = Request::EqSet {
            device: Some("a-device-named-at-some-length".into()),
            band: 5,
            param: "frequency".into(),
            value: 12345.678,
            no_wait: false,
        };
        assert!(to_line(&eq_set).unwrap().len() < 1024);
        let ack = client.request(eq_set).await.unwrap();
        assert_eq!(ack, Response::Ack);
    })
    .await
    .expect("test timed out");
}

/// A line of only whitespace, Unicode included, is skipped rather than answered.
#[tokio::test]
async fn a_blank_line_is_skipped() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let server = start_server(Arc::new(Fake::new())).await;
        let stream = UnixStream::connect(&server.path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let ask = to_line(&RequestLine {
            id: 2,
            request: Request::State { device: None },
        })
        .unwrap();
        write
            .write_all(format!("\n \t\n\u{a0}\u{2003}\n{ask}").as_bytes())
            .await
            .unwrap();
        let first: Reply =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).expect("a reply line");
        assert!(
            matches!(
                &first,
                Reply::Response {
                    id: 2,
                    response: Response::Values { .. }
                }
            ),
            "the first answer is the request's, got {first:?}"
        );
    })
    .await
    .expect("test timed out");
}
