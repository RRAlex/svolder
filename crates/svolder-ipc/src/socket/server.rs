// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A newline-delimited-JSON server over a Unix socket.

use std::path::Path;
use std::sync::Arc;

use futures::StreamExt;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::error::IpcError;
use crate::socket::lines::{Line, LineReader, MAX_LINE};
use crate::socket::protocol::{
    CODE_USAGE, REQUEST_CMDS, Reply, Request, RequestLine, Response, to_line,
};

mod listener;

pub use listener::{Bound, bind, inode, listen_fds, serve_bound};

/// Handles requests and streams for one daemon.
#[async_trait::async_trait]
pub trait Handler: Send + Sync + 'static {
    /// Answer one request/response verb.
    async fn handle(&self, request: Request) -> crate::socket::protocol::Response;
    /// A stream of events for a subscription, ending when the client goes away.
    async fn subscribe(
        &self,
        device: Option<String>,
    ) -> futures::stream::BoxStream<'static, crate::socket::protocol::Event>;
    /// The stream a request for which [`Request::is_stream`] holds turns its connection into,
    /// or the answer that refuses it, which the connection then goes on after as for any
    /// request. This is the one way the server serves a stream: it asks here for every such
    /// request and never by the request's kind.
    ///
    /// The default serves a subscription through [`Handler::subscribe`], whether or not it
    /// asked for the host's view (a handler with no host has nothing more to show), and refuses
    /// every other stream: the host's frames as a daemon with no host audio does, any other as
    /// one that does not stream it.
    async fn stream(
        &self,
        request: Request,
    ) -> Result<futures::stream::BoxStream<'static, crate::socket::protocol::Event>, Box<Response>>
    {
        match request {
            Request::Subscribe { device, .. } => Ok(self.subscribe(device).await),
            Request::HostFrames => Err(Box::new(Response::error(
                CODE_USAGE,
                "this daemon has no host frames",
            ))),
            other => Err(Box::new(Response::error(
                CODE_USAGE,
                format!("this daemon does not stream `{}`", other.cmd()),
            ))),
        }
    }
}

/// Bind `path` and serve `handler` until `shutdown` resolves. Refuses to start if another
/// instance already answers on `path`; removes a stale socket file whose owner is gone. The
/// socket file is removed however the accept loop ends.
pub async fn serve<H: Handler>(
    path: &Path,
    handler: Arc<H>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), IpcError> {
    serve_bound(bind(path).await?, handler, shutdown).await
}

/// Serve `handler` on a listener that is already bound — the one a service manager handed
/// over — until `shutdown` resolves.
///
/// The socket file is left where it is however the accept loop ends: the listener was bound
/// by whoever passed it in, and removing the file would take away a path that process is
/// still listening on.
pub async fn serve_on<H: Handler>(
    listener: UnixListener,
    handler: Arc<H>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), IpcError> {
    accept_loop(&listener, handler, shutdown).await
}

/// Accept connections until `shutdown` resolves. A failed `accept` (descriptor exhaustion, a
/// connection aborted before accept) is transient for a long-lived daemon: pause briefly and
/// keep serving rather than tear down the whole listener (and leak the socket file with it).
async fn accept_loop<H: Handler>(
    listener: &UnixListener,
    handler: Arc<H>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), IpcError> {
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                };
                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    // A client that hangs up mid-request is normal, not a daemon fault.
                    let _ = serve_connection(stream, handler).await;
                });
            }
        }
    }
}

/// Serve one accepted connection until the client disconnects.
///
/// A request for which [`Request::is_stream`] holds switches the connection to writing the
/// handler's event stream; a `select!` races each event against the next incoming line so that the
/// client's EOF (or an error reading it) ends the loop and drops the event stream, rather than
/// leaving a task parked on it forever. Any further line sent while subscribed is ignored,
/// however long, since the connection has become an event stream from the client's point of
/// view.
///
/// A line that does not parse as a request is answered with an `Error` response and the
/// connection keeps serving, so a version-skewed or buggy client reads what was wrong with
/// its line instead of watching the connection close on it. The same holds for a line that
/// is not UTF-8 and for one longer than [`MAX_LINE`] bytes, which is read no further than the
/// cap and skipped through its newline. The reply is the whole diagnostic: this crate has no
/// logger of its own.
async fn serve_connection<H: Handler>(stream: UnixStream, handler: Arc<H>) -> Result<(), IpcError> {
    let (read, mut write) = stream.into_split();
    let mut lines = LineReader::new(BufReader::new(read));
    loop {
        let bytes = match lines.next_line().await? {
            Line::Eof => return Ok(()),
            Line::Bytes(bytes) => bytes,
            Line::TooLong => {
                // The line is not read far enough to find its id, so the reply carries 0.
                refuse(
                    &mut write,
                    0,
                    format!("bad request: the line is longer than {MAX_LINE} bytes"),
                )
                .await?;
                if !lines.skip_line().await? {
                    return Ok(());
                }
                continue;
            }
        };
        let line = match String::from_utf8(bytes) {
            Ok(line) => line,
            Err(e) => {
                let id = id_of(&String::from_utf8_lossy(e.as_bytes()));
                refuse(&mut write, id, "bad request: the line is not UTF-8".into()).await?;
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: RequestLine = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let id = id_of(&line);
                match unknown_request(&line) {
                    Some(cmd) => {
                        let reply = Reply::Response {
                            id,
                            response: Response::unknown_request(&cmd),
                        };
                        write.write_all(to_line(&reply)?.as_bytes()).await?;
                    }
                    None => refuse(&mut write, id, format!("bad request: {e}")).await?,
                }
                continue;
            }
        };
        match req.request {
            request if request.is_stream() => {
                let acks = request.stream_acks();
                match handler.stream(request).await {
                    Ok(events) => {
                        return stream_events(&mut lines, &mut write, req.id, acks, events).await;
                    }
                    // A refusal is an answer, and the connection goes on as for any request.
                    Err(response) => {
                        let reply = Reply::Response {
                            id: req.id,
                            response: *response,
                        };
                        write.write_all(to_line(&reply)?.as_bytes()).await?;
                    }
                }
            }
            other => {
                let response = handler.handle(other).await;
                let line = to_line(&Reply::Response {
                    id: req.id,
                    response,
                })?;
                write.write_all(line.as_bytes()).await?;
            }
        }
    }
}

/// Write `events` to the client under request `id` until the stream ends or the client goes
/// away: a `select!` races each event against the next incoming line so that the client's EOF
/// (or an error reading it) ends the loop and drops the event stream. With `acks` the stream
/// opens with an acknowledgement under the same id.
async fn stream_events<R: tokio::io::AsyncBufRead + Unpin>(
    lines: &mut LineReader<R>,
    write: &mut tokio::net::unix::OwnedWriteHalf,
    id: u64,
    acks: bool,
    mut events: futures::stream::BoxStream<'static, crate::socket::protocol::Event>,
) -> Result<(), IpcError> {
    if acks {
        let reply = Reply::Response {
            id,
            response: Response::Ack,
        };
        if write.write_all(to_line(&reply)?.as_bytes()).await.is_err() {
            return Ok(()); // client gone
        }
    }
    loop {
        tokio::select! {
            event = events.next() => {
                match event {
                    Some(event) => {
                        let line = to_line(&Reply::Event { id, event })?;
                        if write.write_all(line.as_bytes()).await.is_err() {
                            return Ok(()); // client gone
                        }
                    }
                    None => return Ok(()), // handler's stream ended
                }
            }
            next = lines.next_line() => {
                match next {
                    // Ignored while subscribed, however long: a line over the cap is read on
                    // in pieces of at most the cap, each ignored the same way, so the events
                    // keep going out while it arrives.
                    Ok(Line::Bytes(_) | Line::TooLong) => continue,
                    _ => return Ok(()), // EOF or read error: drop the event stream
                }
            }
        }
    }
}

/// Answer a line the server cannot act on with an `Error` response, code 1.
async fn refuse(
    write: &mut tokio::net::unix::OwnedWriteHalf,
    id: u64,
    message: String,
) -> Result<(), IpcError> {
    let reply = Reply::Response {
        id,
        response: Response::error(CODE_USAGE, message),
    };
    write.write_all(to_line(&reply)?.as_bytes()).await?;
    Ok(())
}

/// The request kind `line` names when it is one this build does not have: a JSON line whose
/// `request` has a string `cmd` that is not in [`REQUEST_CMDS`]. A line that is no JSON, names no
/// `cmd`, or names a kind this build has with a field missing or unreadable is a malformed
/// request, not an unknown one.
fn unknown_request(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let cmd = value.get("request")?.get("cmd")?.as_str()?;
    (!REQUEST_CMDS.contains(&cmd)).then(|| cmd.to_string())
}

/// The correlation id a line carries: its own `id` when the line is JSON with a numeric one,
/// and 0 when there is nothing to correlate the reply with.
fn id_of(line: &str) -> u64 {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
