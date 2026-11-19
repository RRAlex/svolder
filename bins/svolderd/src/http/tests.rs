// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::convert::Infallible;
use std::net::SocketAddr;

use http_body_util::{Full, StreamBody};
use hyper::body::{Bytes, Frame};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::*;

/// A loop on a free loopback port answering every request with `ok`.
fn serve_ok() -> (SocketAddr, CancellationToken) {
    let listener = bind("127.0.0.1:0".parse().unwrap()).expect("a free port");
    let addr = listener.local_addr().unwrap();
    let stop = CancellationToken::new();
    let serve: Serve<Full<Bytes>> =
        Arc::new(|_| Box::pin(async { Response::new(Full::new(Bytes::from_static(b"ok"))) }));
    tokio::spawn(accept_loop(
        listener,
        serve,
        stop.clone(),
        "test",
        Activity::default(),
    ));
    (addr, stop)
}

/// A connection that sends nothing is closed once the header timeout has passed.
#[tokio::test]
async fn an_idle_connection_is_closed_after_the_header_timeout() {
    let (addr, stop) = serve_ok();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let started = std::time::Instant::now();
    let mut raw = Vec::new();
    tokio::time::timeout(HEADER_TIMEOUT * 2, stream.read_to_end(&mut raw))
        .await
        .expect("the server closed the idle connection")
        .ok();
    assert!(started.elapsed() >= HEADER_TIMEOUT - Duration::from_secs(1));
    stop.cancel();
}

/// A connection kept alive after its answer, and then idle, gives its slot back after the header
/// timeout: a browser's spare connections cannot hold the 64 slots.
#[tokio::test]
async fn an_idle_keep_alive_connection_is_closed_after_the_header_timeout() {
    let (addr, stop) = serve_ok();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(format!("GET / HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut head = vec![0u8; 512];
    let read = stream.read(&mut head).await.unwrap();
    let answer = String::from_utf8_lossy(&head[..read]).into_owned();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.ends_with("ok"), "{answer}");
    let idle = std::time::Instant::now();
    let mut rest = Vec::new();
    tokio::time::timeout(HEADER_TIMEOUT * 2, stream.read_to_end(&mut rest))
        .await
        .expect("the server closed the kept-alive connection")
        .ok();
    assert!(rest.is_empty(), "{rest:?}");
    assert!(idle.elapsed() >= HEADER_TIMEOUT - Duration::from_secs(1));
    stop.cancel();
}

/// An answer that streams for longer than the header timeout (the web UI's events) is an
/// answer, not an idle connection: every part of it arrives.
#[tokio::test]
async fn a_long_answer_is_not_an_idle_connection() {
    let listener = bind("127.0.0.1:0".parse().unwrap()).expect("a free port");
    let addr = listener.local_addr().unwrap();
    let stop = CancellationToken::new();
    type Streamed = StreamBody<
        std::pin::Pin<Box<dyn futures::Stream<Item = Result<Frame<Bytes>, Infallible>> + Send>>,
    >;
    let serve: Serve<Streamed> = Arc::new(|_| {
        Box::pin(async {
            let ticks = futures::stream::unfold(0u32, |n| async move {
                if n == 7 {
                    return None;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                Some((Ok(Frame::data(Bytes::from(format!("tick{n};")))), n + 1))
            });
            Response::new(StreamBody::new(Box::pin(ticks) as _))
        })
    });
    tokio::spawn(accept_loop(
        listener,
        serve,
        stop.clone(),
        "test",
        Activity::default(),
    ));
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            format!("GET / HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), stream.read_to_end(&mut raw))
        .await
        .expect("the answer ended")
        .unwrap();
    let text = String::from_utf8_lossy(&raw);
    assert!(text.contains("tick0;") && text.contains("tick6;"), "{text}");
    stop.cancel();
}

#[test]
fn the_host_list_follows_the_address() {
    let hosts = |addr: &str, extra: &[&str]| {
        let extra: Vec<String> = extra.iter().map(|h| h.to_string()).collect();
        allowed_hosts(addr.parse().unwrap(), &extra)
    };
    assert_eq!(
        hosts("127.0.0.1:8737", &[]),
        ["127.0.0.1:8737", "localhost:8737", "[::1]:8737"]
    );
    assert_eq!(
        hosts("[::1]:9", &[]),
        ["[::1]:9", "localhost:9", "127.0.0.1:9"]
    );
    assert_eq!(hosts("192.168.1.5:8737", &[]), ["192.168.1.5:8737"]);
    assert_eq!(
        hosts("192.168.1.5:8737", &["mcp.home.lan"]),
        ["192.168.1.5:8737", "mcp.home.lan"]
    );
    assert!(hosts("0.0.0.0:8737", &[]).is_empty());
    assert!(hosts("[::]:8737", &[]).is_empty());
    assert_eq!(
        hosts("0.0.0.0:8737", &["mcp.home.lan"]),
        [
            "localhost:8737",
            "127.0.0.1:8737",
            "[::1]:8737",
            "mcp.home.lan"
        ]
    );
    // The container's setup: a wildcard bind with only the loopback names listed is a list, not
    // the any-Host rule, so a request for another name (a rebinding page's) is refused.
    let container = hosts("0.0.0.0:8737", &["localhost", "127.0.0.1"]);
    assert!(!container.is_empty());
    assert!(container.iter().all(|h| !h.contains("evil")));
    assert!(container.contains(&"localhost".to_string()));
    assert!(container.contains(&"127.0.0.1".to_string()));
}

#[tokio::test]
async fn a_server_with_no_auth_is_refused_off_loopback_before_its_token_is_read() {
    let read = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = read.clone();
    let wildcard: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let err = prepare(wildcard, true, move || {
        seen.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(Some(()))
    })
    .await
    .expect_err("an open server off loopback is refused");
    assert_eq!(err, "auth is \"none\" on an address that is not loopback");
    assert!(
        !read.load(std::sync::atomic::Ordering::SeqCst),
        "the token was read"
    );
}

/// A loop on a free loopback port whose answer counts the connection, as a server with no token
/// does, or does not, as `pass` says.
fn serve_counting(pass: bool) -> (SocketAddr, CancellationToken, Activity) {
    let listener = bind("127.0.0.1:0".parse().unwrap()).expect("a free port");
    let addr = listener.local_addr().unwrap();
    let stop = CancellationToken::new();
    let activity = Activity::default();
    let serve: Serve<Full<Bytes>> = Arc::new(move |mut request| {
        let counted = request
            .extensions_mut()
            .remove::<crate::activity::Counted>()
            .expect("the loop hands each request its connection");
        if pass && request.headers().contains_key("x-pass") {
            counted.pass();
        }
        Box::pin(async { Response::new(Full::new(Bytes::from_static(b"ok"))) })
    });
    tokio::spawn(accept_loop(
        listener,
        serve,
        stop.clone(),
        "test",
        activity.clone(),
    ));
    (addr, stop, activity)
}

/// One request on a connection kept open, its answer read.
async fn held(addr: SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    ask(&mut stream, addr, true).await;
    stream
}

/// One request on `stream`, carrying `x-pass` when `pass`, answered 200.
async fn ask(stream: &mut TcpStream, addr: SocketAddr, pass: bool) {
    let mark = if pass { "x-pass: 1\r\n" } else { "" };
    stream
        .write_all(format!("GET / HTTP/1.1\r\nHost: {addr}\r\n{mark}\r\n").as_bytes())
        .await
        .unwrap();
    let mut head = vec![0u8; 512];
    let read = stream.read(&mut head).await.unwrap();
    assert!(String::from_utf8_lossy(&head[..read]).starts_with("HTTP/1.1 200"));
}

/// Two held connections read open 2, and 0 once they close; the one client is seen.
#[tokio::test]
async fn held_connections_are_open_until_they_close() {
    let (addr, stop, activity) = serve_counting(true);
    let first = held(addr).await;
    let second = held(addr).await;
    tokio::time::timeout(Duration::from_secs(10), activity.until(|now| now.open == 2))
        .await
        .expect("both connections count");
    let now = activity.snapshot();
    assert_eq!((now.hour, now.day), (1, 1));
    drop(first);
    drop(second);
    tokio::time::timeout(Duration::from_secs(10), activity.until(|now| now.open == 0))
        .await
        .expect("the closed connections stop counting");
    assert_eq!(activity.snapshot().day, 1);
    stop.cancel();
}

/// A server that stops reads 0 open: its connections are aborted and their guards drop.
#[tokio::test]
async fn a_stopped_server_has_nothing_open() {
    let (addr, stop, activity) = serve_counting(true);
    let _held = held(addr).await;
    tokio::time::timeout(Duration::from_secs(10), activity.until(|now| now.open == 1))
        .await
        .expect("the connection counts");
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(10), activity.until(|now| now.open == 0))
        .await
        .expect("a stop closes the count");
}

/// On one kept-alive connection a first request that fails and a second that passes count the
/// connection once, from the passing one.
#[tokio::test]
async fn a_keep_alive_connection_counts_from_the_request_that_passes() {
    let (addr, stop, activity) = serve_counting(true);
    let mut stream = TcpStream::connect(addr).await.unwrap();
    ask(&mut stream, addr, false).await;
    let before = activity.snapshot();
    assert_eq!((before.open, before.day), (0, 0));
    ask(&mut stream, addr, true).await;
    ask(&mut stream, addr, true).await;
    let after = activity.snapshot();
    assert_eq!((after.open, after.day), (1, 1));
    stop.cancel();
}

/// A request the server never passes leaves the connection uncounted.
#[tokio::test]
async fn a_connection_the_server_does_not_pass_is_not_counted() {
    let (addr, stop, activity) = serve_counting(false);
    let _held = held(addr).await;
    let now = activity.snapshot();
    assert_eq!((now.open, now.hour, now.day), (0, 0, 0));
    stop.cancel();
}
