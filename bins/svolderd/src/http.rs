// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The hardened HTTP/1 accept loop svolderd's two HTTP servers share, the MCP server and the web
//! UI, with its limits: [`MAX_CONNECTIONS`] connections at once, [`HEADER_TIMEOUT`] to send a
//! request head (and, on a connection kept alive, the next one), a [`MAX_HEAD_BYTES`] head, and
//! [`SETUP_TIMEOUT`] for the token and the bind; and the `Host` allow-list both check against.

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::log;

/// How many connections are served at once; one more is closed as soon as it is accepted.
/// Idle connections count until the header timeout closes them, so a client that opens this many
/// and sends nothing keeps the server from answering for up to [`HEADER_TIMEOUT`] at a time;
/// that is accepted, since the daemon's socket and devices are not affected.
pub const MAX_CONNECTIONS: usize = 64;

/// The most of a request's head hyper buffers per connection, before the request is looked at.
pub const MAX_HEAD_BYTES: usize = 16 * 1024;

/// The longest the token's source and the bind may take. A source that blocks (a file on a
/// hung mount) fails the server that asked and is reported. The daemon's startup awaits each
/// HTTP server in turn before it accepts on its socket, so a client of the socket waits at most
/// this long for each of them.
pub const SETUP_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a connection has to send its request line and headers; an idle one is closed then.
/// A cap per peer address is not needed on top: a connection that sends nothing costs a slot for
/// this long and nothing else, and the daemon's socket and devices do not share it.
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(5);

/// Run `work`, which may block, off the runtime's workers, giving up after [`SETUP_TIMEOUT`].
pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    match tokio::time::timeout(SETUP_TIMEOUT, tokio::task::spawn_blocking(work)).await {
        Ok(Ok(done)) => done,
        Ok(Err(_)) => Err("the step failed unexpectedly".to_string()),
        Err(_) => Err(format!(
            "the step took longer than {} s",
            SETUP_TIMEOUT.as_secs()
        )),
    }
}

/// What a server needs before it can serve on `listen`: the token `token` reads, if the server
/// has one, and the address bound, all off the runtime's workers and under [`SETUP_TIMEOUT`].
/// `open` is a server with no auth at all, which is refused off a loopback address (the config
/// refuses it too; the gate holds it as well).
pub async fn prepare<T: Send + 'static>(
    listen: SocketAddr,
    open: bool,
    token: impl FnOnce() -> Result<Option<T>, String> + Send + 'static,
) -> Result<(Option<T>, TcpListener), String> {
    blocking(move || {
        if open && !listen.ip().is_loopback() {
            return Err("auth is \"none\" on an address that is not loopback".to_string());
        }
        let token = token()?;
        let listener = bind(listen).map_err(|e| e.to_string())?;
        Ok((token, listener))
    })
    .await
}

/// Bind `addr` without waiting, so a start never blocks the daemon.
pub fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    let listener = std::net::TcpListener::bind(addr)?;
    listener.set_nonblocking(true)?;
    TcpListener::from_std(listener)
}

/// The `Host` values a request may carry: the address the server is bound to, with its port,
/// on a loopback address also `localhost` and both loopback literals, and the `hosts`
/// names of `[mcp]` or `[web]` (an entry without a port matches any port, as rmcp reads it).
///
/// An unspecified address (`0.0.0.0`, `::`) names no host a client would use, so with no
/// `hosts` every `Host` is let through there; the token, which the config requires off
/// loopback, is the check. With `hosts` set, a wildcard bind allows those names and the
/// loopback ones only.
pub fn allowed_hosts(bound: SocketAddr, hosts: &[String]) -> Vec<String> {
    let port = bound.port();
    let ip = bound.ip();
    if ip.is_unspecified() && hosts.is_empty() {
        return Vec::new();
    }
    let mut allowed = Vec::new();
    let mut add = |host: String| {
        if !allowed.contains(&host) {
            allowed.push(host);
        }
    };
    if !ip.is_unspecified() {
        add(authority(ip, port));
    }
    if ip.is_loopback() || ip.is_unspecified() {
        for name in ["localhost", "127.0.0.1", "[::1]"] {
            add(format!("{name}:{port}"));
        }
    }
    for host in hosts {
        add(host.clone());
    }
    allowed
}

/// `ip:port`, with an IPv6 address in brackets, as a `Host` header carries it.
fn authority(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(ip) => format!("{ip}:{port}"),
        IpAddr::V6(ip) => format!("[{ip}]:{port}"),
    }
}

/// What answers one request.
pub type Answer<B> = Pin<Box<dyn Future<Output = Response<B>> + Send>>;

/// The server behind the loop: one answer per request.
pub type Serve<B> = Arc<dyn Fn(Request<Incoming>) -> Answer<B> + Send + Sync>;

/// Accept connections until `stop`, each served on a task of its own by `serve`. One past
/// [`MAX_CONNECTIONS`] is closed as soon as it is accepted; dropping the set on the way out aborts
/// every connection still open. `name` heads the log line of a failed accept.
pub async fn accept_loop<B>(
    listener: TcpListener,
    serve: Serve<B>,
    stop: CancellationToken,
    name: &'static str,
) where
    B: hyper::body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let mut connections = JoinSet::new();
    loop {
        let accepted = tokio::select! {
            () = stop.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        let stream = match accepted {
            Ok((stream, _)) => stream,
            Err(e) => {
                // Out of descriptors, most likely: waiting is kinder than spinning.
                log::warn(&format!("{name}: accept failed: {e}"));
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        while connections.try_join_next().is_some() {}
        if connections.len() >= MAX_CONNECTIONS {
            continue;
        }
        let serve = Arc::clone(&serve);
        connections.spawn(async move {
            let served = hyper::server::conn::http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(HEADER_TIMEOUT)
                .max_buf_size(MAX_HEAD_BYTES)
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(move |request| {
                        let answer = serve(request);
                        async move { Ok::<_, Infallible>(answer.await) }
                    }),
                )
                .await;
            // A client that hung up mid-request is not worth a line.
            let _ = served;
        });
    }
}

#[cfg(test)]
mod tests;
