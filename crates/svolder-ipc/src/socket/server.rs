// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A newline-delimited-JSON server over a Unix socket.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::error::IpcError;
use crate::socket::client;
use crate::socket::lines::{Line, LineReader, MAX_LINE};
use crate::socket::protocol::{Reply, Request, RequestLine, Response, to_line};

/// Handles requests and subscriptions for one daemon.
#[async_trait::async_trait]
pub trait Handler: Send + Sync + 'static {
    /// Answer one request/response verb.
    async fn handle(&self, request: Request) -> crate::socket::protocol::Response;
    /// A stream of events for a subscription, ending when the client goes away.
    async fn subscribe(
        &self,
        device: Option<String>,
    ) -> futures::stream::BoxStream<'static, crate::socket::protocol::Event>;
    /// The stream of a subscription that asked for the host's view too (`host: true`). A
    /// handler with no host to show gives the plain stream.
    async fn subscribe_with_host(
        &self,
        device: Option<String>,
    ) -> futures::stream::BoxStream<'static, crate::socket::protocol::Event> {
        self.subscribe(device).await
    }
    /// The stream of a `HostFrames` request: the daemon's capture state and its analysed frames,
    /// held for as long as the stream lives. The default is an error, which is what a handler
    /// with no host audio, and a daemon from before it, answer.
    async fn host_frames(
        &self,
    ) -> Result<futures::stream::BoxStream<'static, crate::socket::protocol::Event>, String> {
        Err("this daemon has no host frames".to_string())
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

/// A listener this process bound on a path of its own, and the inode of the socket file it
/// bound.
///
/// A daemon that has to know its socket file is still its own — one whose path another process
/// may unlink and bind a listener of its own on — learns the inode here rather than by looking
/// at the path afterwards, where what it finds may already be somebody else's.
pub struct Bound {
    listener: UnixListener,
    path: PathBuf,
    inode: Option<u64>,
}

impl Bound {
    /// The inode of the socket file at the moment it was bound, or `None` when the filesystem
    /// could not say.
    pub fn inode(&self) -> Option<u64> {
        self.inode
    }
}

/// Create the socket's directory if need be, then bind `path`: refuse if a live daemon already
/// answers there, clear a stale socket file, and leave the file at mode 0600.
pub async fn bind(path: &Path) -> Result<Bound, IpcError> {
    crate::socket::path::ensure_parent(path)?;
    let listener = bind_listener(path).await?;
    let inode = inode(path).ok().flatten();
    Ok(Bound {
        listener,
        path: path.to_path_buf(),
        inode,
    })
}

/// Serve `handler` on a listener [`bind`] returned until `shutdown` resolves, then remove the
/// socket file.
pub async fn serve_bound<H: Handler>(
    bound: Bound,
    handler: Arc<H>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), IpcError> {
    let Bound {
        listener,
        path,
        inode: mine,
    } = bound;
    let outcome = accept_loop(&listener, handler, shutdown).await;
    // Only the file this listener bound is this listener's to remove. A path another process
    // has since bound — a socket unit that unlinked it and put its own there — belongs to that
    // process, and taking it away would leave that daemon listening where nobody can reach it.
    if mine.is_some() && inode(&path).ok().flatten() == mine {
        let _ = std::fs::remove_file(&path);
    }
    outcome
}

/// The inode a path names, for a caller that has to know a socket file is still the one it
/// bound.
///
/// `Ok(None)` is a path with nothing there. An `Err` is a path that could not be asked about
/// at all — a filesystem that answered something other than "no such file" — which says
/// nothing about whose the socket is: a caller that acts on the answer wants that told apart
/// from an inode that changed.
pub fn inode(path: &Path) -> std::io::Result<Option<u64>> {
    use std::os::unix::fs::MetadataExt;
    match std::fs::metadata(path) {
        Ok(meta) => Ok(Some(meta.ino())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
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

/// The first descriptor a service manager passes a listening socket on.
const LISTEN_FDS_START: std::os::fd::RawFd = 3;

/// The listening socket a service manager handed this process, or `None` when it handed none.
///
/// This is the systemd socket-activation protocol as far as svolder uses it: `LISTEN_FDS`
/// counts the descriptors passed, starting at 3, and `LISTEN_PID` names the process they were
/// meant for. Exactly one socket is expected, since `svolderd` listens on one; a `LISTEN_PID`
/// naming another process is a handover inherited by a child rather than one addressed here,
/// and is refused so the child does not serve its parent's socket. An absent `LISTEN_PID` is
/// accepted: a handover simulated by hand does not always set it.
///
/// Call this once, before anything else in the process can have opened a descriptor on top of
/// the one passed. Every later call answers `None`, because adopting the same descriptor twice
/// would hand out two owners of one socket.
pub fn listen_fds() -> Option<std::os::unix::net::UnixListener> {
    use std::os::fd::FromRawFd;
    use std::sync::atomic::{AtomicBool, Ordering};
    static ADOPTED: AtomicBool = AtomicBool::new(false);

    if !handed_over(
        std::env::var("LISTEN_FDS").ok().as_deref(),
        std::env::var("LISTEN_PID").ok().as_deref(),
        std::process::id(),
    ) {
        return None;
    }
    if ADOPTED.swap(true, Ordering::SeqCst) {
        return None;
    }
    // SAFETY: the environment says descriptor 3 is a listening socket this process was given
    // and has not taken over yet — `ADOPTED` is what makes that true of this call alone — so
    // nothing else owns it and the listener returned is its only owner.
    Some(unsafe { std::os::unix::net::UnixListener::from_raw_fd(LISTEN_FDS_START) })
}

/// Whether the environment describes exactly one socket handed to process `pid`.
fn handed_over(listen_fds: Option<&str>, listen_pid: Option<&str>, pid: u32) -> bool {
    if listen_fds.and_then(|count| count.trim().parse::<u32>().ok()) != Some(1) {
        return false;
    }
    match listen_pid {
        None => true,
        Some(named) => named.trim().parse::<u32>() == Ok(pid),
    }
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

/// Bind the listener, refusing if a live daemon already answers and clearing a stale socket.
async fn bind_listener(path: &Path) -> Result<UnixListener, IpcError> {
    let listener = match UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if client::is_running(path).await {
                return Err(IpcError::Protocol(
                    "another svolderd is already running".into(),
                ));
            }
            let _ = std::fs::remove_file(path);
            UnixListener::bind(path)?
        }
        Err(e) => return Err(e.into()),
    };
    // A socket this call binds itself takes the umask's own mode; systemd socket activation
    // hands the daemon an already-bound, already-0600 listener instead, so this only matters
    // to a daemon started directly (a container's own process, `svolderd` run by hand). Force
    // it here so a directory shared with other users on the machine (a bind-mounted `./run`,
    // unlike `$XDG_RUNTIME_DIR`'s own 0700) still keeps other users off the socket.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Serve one accepted connection until the client disconnects.
///
/// A `Subscribe` or a `HostFrames` request switches the connection to writing the handler's
/// event stream; a `select!` races each event against the next incoming line so that the
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
                refuse(&mut write, id_of(&line), format!("bad request: {e}")).await?;
                continue;
            }
        };
        match req.request {
            Request::Subscribe { device, host } => {
                let events = match host {
                    true => handler.subscribe_with_host(device).await,
                    false => handler.subscribe(device).await,
                };
                return stream_events(&mut lines, &mut write, req.id, events).await;
            }
            Request::HostFrames => match handler.host_frames().await {
                Ok(events) => return stream_events(&mut lines, &mut write, req.id, events).await,
                // A refusal is an answer, and the connection goes on as for any request.
                Err(message) => {
                    let reply = Reply::Response {
                        id: req.id,
                        response: Response::Error { code: 1, message },
                    };
                    write.write_all(to_line(&reply)?.as_bytes()).await?;
                }
            },
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
/// (or an error reading it) ends the loop and drops the event stream.
async fn stream_events<R: tokio::io::AsyncBufRead + Unpin>(
    lines: &mut LineReader<R>,
    write: &mut tokio::net::unix::OwnedWriteHalf,
    id: u64,
    mut events: futures::stream::BoxStream<'static, crate::socket::protocol::Event>,
) -> Result<(), IpcError> {
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
        response: Response::Error { code: 1, message },
    };
    write.write_all(to_line(&reply)?.as_bytes()).await?;
    Ok(())
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
mod tests {
    use super::*;
    use crate::socket::protocol::*;
    use crate::test_support::{Fake, start_server, start_server_at, temp_socket};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::io::AsyncBufReadExt;

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
                matches!(&first, Reply::Response { id: 0, response: Response::Error { code: 1, message } }
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
                        response: Response::Error { code: 1, .. }
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

            let first: Reply = serde_json::from_str(&lines.next_line().await.unwrap().unwrap())
                .expect("a reply line");
            let Reply::Response {
                response: Response::Error { code: 1, message },
                ..
            } = &first
            else {
                panic!("got {first:?}");
            };
            assert!(
                message.contains(&crate::socket::lines::MAX_LINE.to_string()),
                "the error names the cap: {message}"
            );
            let second: Reply = serde_json::from_str(&lines.next_line().await.unwrap().unwrap())
                .expect("a reply line");
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
                matches!(&first, Reply::Response { response: Response::Error { code: 1, message }, .. }
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
            let first: Reply = serde_json::from_str(&lines.next_line().await.unwrap().unwrap())
                .expect("a reply line");
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

    #[tokio::test]
    async fn a_second_bind_is_refused_while_the_first_lives() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server = start_server(Arc::new(Fake::new())).await;
            let second = serve(&server.path, Arc::new(Fake::new()), std::future::pending()).await;
            assert!(
                second.is_err(),
                "second serve must refuse while the first holds the socket"
            );
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn dropped_subscriber_releases_the_event_stream() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let counter = Arc::new(AtomicUsize::new(0));
            let server = start_server(Arc::new(Fake::with_counter(Arc::clone(&counter)))).await;
            let client = crate::socket::client::Client::connect(&server.path)
                .await
                .unwrap();
            let mut events = client.subscribe(None).await.unwrap();
            let first = events.next().await.expect("one event");
            assert!(matches!(first, Event::Status { .. }));
            drop(events);

            // The server's connection task notices the client's departure asynchronously;
            // poll for the drop counter rather than assuming it lands on the next await.
            for _ in 0..200 {
                if counter.load(Ordering::SeqCst) == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert_eq!(counter.load(Ordering::SeqCst), 1);
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn stale_socket_file_is_replaced() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let socket = crate::test_support::TempPath(temp_socket());
            let path = socket.0.clone();
            crate::socket::path::ensure_parent(&path).unwrap();
            // Bind and drop a listener: the socket file stays behind with nothing serving it.
            let listener = UnixListener::bind(&path).unwrap();
            drop(listener);
            assert!(path.exists());

            // The fixture's own existence poll sees the stale file immediately, so retry the
            // connect itself until `serve` has cleared it and is actually listening.
            let server = start_server_at(path, Arc::new(Fake::new())).await;
            for _ in 0..50 {
                if let Ok(mut c) = crate::socket::client::Client::connect(&server.path).await {
                    let resp = c.request(Request::State { device: None }).await.unwrap();
                    assert!(matches!(resp, Response::Values { values, .. } if values.len() == 1));
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("server never came up on the stale socket path");
        })
        .await
        .expect("test timed out");
    }

    /// A socket this call binds itself is forced to 0600 rather than left at whatever the
    /// umask gives it, on both the fresh-bind path and the stale-socket-replaced one: a
    /// directory shared with other users (a bind-mounted `./run`, unlike `$XDG_RUNTIME_DIR`'s
    /// own 0700) must not leave the daemon's socket open to them either way.
    #[tokio::test]
    async fn bind_forces_the_socket_to_0600() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mode = |path: &std::path::Path| {
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777
            };

            let fresh = start_server(Arc::new(Fake::new())).await;
            assert_eq!(mode(&fresh.path), 0o600, "a fresh bind");

            let socket = crate::test_support::TempPath(temp_socket());
            let path = socket.0.clone();
            crate::socket::path::ensure_parent(&path).unwrap();
            drop(UnixListener::bind(&path).unwrap());
            let replaced = start_server_at(path, Arc::new(Fake::new())).await;
            for _ in 0..50 {
                if crate::socket::client::Client::connect(&replaced.path)
                    .await
                    .is_ok()
                {
                    assert_eq!(mode(&replaced.path), 0o600, "a stale socket replaced");
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("server never came up on the stale socket path");
        })
        .await
        .expect("test timed out");
    }

    /// The socket file is this listener's to remove once its accept loop ends, so the path is
    /// free for the next daemon that binds it rather than left claiming an inode nobody serves.
    #[tokio::test]
    async fn serve_removes_its_socket_file_when_it_stops() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let (tx, rx) = tokio::sync::oneshot::channel::<()>();
            let p2 = path.clone();
            let join = tokio::spawn(async move {
                serve(&p2, Arc::new(Fake::new()), async {
                    let _ = rx.await;
                })
                .await
            });
            for _ in 0..50 {
                if path.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(path.exists(), "the server never bound");
            tx.send(()).expect("the server is still waiting on it");
            join.await
                .expect("the server task did not panic")
                .expect("serve ended cleanly");
            assert!(!path.exists(), "the socket file was left behind");
        })
        .await
        .expect("test timed out");
    }

    /// What the socket-activation environment has to say before a descriptor is adopted.
    /// `tests/adoption.rs` drives the descriptor itself; this is the reading of the two
    /// variables, which needs no descriptor and no environment of its own.
    #[test]
    fn a_handover_is_one_socket_addressed_to_this_process() {
        assert!(handed_over(Some("1"), None, 42));
        assert!(handed_over(Some("1"), Some("42"), 42));
        assert!(handed_over(Some(" 1 "), Some(" 42 "), 42));

        assert!(!handed_over(None, None, 42), "no handover at all");
        assert!(!handed_over(Some("0"), None, 42), "no socket passed");
        assert!(!handed_over(Some("2"), None, 42), "more sockets than one");
        assert!(!handed_over(Some(""), None, 42), "a blank count");
        assert!(!handed_over(Some("many"), None, 42), "an unreadable count");
        assert!(
            !handed_over(Some("1"), Some("43"), 42),
            "a handover addressed to another process"
        );
        assert!(
            !handed_over(Some("1"), Some("pid"), 42),
            "an unreadable process id"
        );
    }

    /// A handler whose subscription hands out what the test sends it, when the test sends it.
    struct Paced {
        events: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Event>>>,
    }

    #[async_trait::async_trait]
    impl Handler for Paced {
        async fn handle(&self, _request: Request) -> Response {
            Response::Ack
        }

        async fn subscribe(
            &self,
            _device: Option<String>,
        ) -> futures::stream::BoxStream<'static, Event> {
            let rx = self
                .events
                .lock()
                .unwrap()
                .take()
                .expect("one subscription");
            futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })
            .boxed()
        }
    }

    /// An over-long line a subscriber sends holds no event back: the line is read on in pieces
    /// while the events keep going out, though its newline never comes.
    #[tokio::test]
    async fn an_over_long_line_from_a_subscriber_holds_no_event_back() {
        tokio::time::timeout(Duration::from_secs(10), async {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let server = start_server(Arc::new(Paced {
                events: std::sync::Mutex::new(Some(rx)),
            }))
            .await;
            let stream = UnixStream::connect(&server.path).await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            let subscribe = to_line(&RequestLine {
                id: 4,
                request: Request::Subscribe {
                    device: None,
                    host: false,
                },
            })
            .unwrap();
            write.write_all(subscribe.as_bytes()).await.unwrap();
            let event = |label: &str| Event::Fault {
                device: "fake".into(),
                label: label.into(),
            };

            // The subscription is serving once its first event comes back.
            tx.send(event("first")).unwrap();
            let first: Reply = serde_json::from_str(&lines.next_line().await.unwrap().unwrap())
                .expect("a reply line");
            assert_eq!(
                first,
                Reply::Event {
                    id: 4,
                    event: event("first")
                }
            );

            // Twice the cap and no newline: once `write_all` returns, the server has read past
            // the cap, since a Unix socket buffers far less than that.
            write
                .write_all(&vec![b'a'; 2 * crate::socket::lines::MAX_LINE])
                .await
                .unwrap();
            tx.send(event("second")).unwrap();
            let second = tokio::time::timeout(Duration::from_secs(2), lines.next_line())
                .await
                .expect("the event went out while the long line was still arriving")
                .unwrap()
                .unwrap();
            let second: Reply = serde_json::from_str(&second).expect("a reply line");
            assert_eq!(
                second,
                Reply::Event {
                    id: 4,
                    event: event("second")
                }
            );
        })
        .await
        .expect("test timed out");
    }
    /// A handler that has frames to give: the capture's state, then one frame, then silence; it
    /// counts the streams it gave that were dropped.
    struct Framing {
        dropped: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Handler for Framing {
        async fn handle(&self, _request: Request) -> Response {
            Response::Ack
        }

        async fn subscribe(
            &self,
            _device: Option<String>,
        ) -> futures::stream::BoxStream<'static, Event> {
            futures::stream::pending().boxed()
        }

        async fn host_frames(&self) -> Result<futures::stream::BoxStream<'static, Event>, String> {
            let events = futures::stream::iter(vec![
                Event::HostCapture(crate::host::CaptureView {
                    state: crate::host::CaptureKind::Starting,
                    sink: None,
                    why: None,
                }),
                Event::HostFrame(crate::host::FrameView {
                    t: 5,
                    left: vec![1; 240],
                    right: vec![2; 240],
                    levels: [3; 4],
                    beat: false,
                    bpm: None,
                    phase: 0,
                }),
            ])
            .chain(futures::stream::pending());
            Ok(
                crate::test_support::DropCounted::new(events.boxed(), Arc::clone(&self.dropped))
                    .boxed(),
            )
        }
    }

    /// A handler that does not override `host_frames` refuses the request with an error and the
    /// connection goes on serving, as it does for any request it cannot take.
    #[tokio::test]
    async fn a_handler_with_no_host_frames_answers_an_error_and_keeps_serving() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server = start_server(Arc::new(Fake::new())).await;
            let stream = UnixStream::connect(&server.path).await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            let frames = to_line(&RequestLine {
                id: 3,
                request: Request::HostFrames,
            })
            .unwrap();
            write.write_all(frames.as_bytes()).await.unwrap();
            let refused: Reply =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            let Reply::Response {
                id: 3,
                response: Response::Error { message, .. },
            } = refused
            else {
                panic!("expected an error answer, got {refused:?}");
            };
            assert!(message.contains("no host frames"), "{message}");
            let state = to_line(&RequestLine {
                id: 4,
                request: Request::State { device: None },
            })
            .unwrap();
            write.write_all(state.as_bytes()).await.unwrap();
            let next: Reply =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert!(
                matches!(
                    next,
                    Reply::Response {
                        id: 4,
                        response: Response::Values { .. }
                    }
                ),
                "{next:?}"
            );
        })
        .await
        .expect("test timed out");
    }

    /// A frames request turns the connection into an event stream, which is dropped when the
    /// client goes away.
    #[tokio::test]
    async fn a_host_frames_request_streams_events_until_the_client_goes() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let dropped = Arc::new(AtomicUsize::new(0));
            let server = start_server(Arc::new(Framing {
                dropped: Arc::clone(&dropped),
            }))
            .await;
            let client = crate::socket::client::Client::connect(&server.path)
                .await
                .unwrap();
            let mut events = client.host_frames().await.unwrap();
            assert!(matches!(events.next().await, Some(Event::HostCapture(_))));
            assert!(matches!(events.next().await, Some(Event::HostFrame(frame)) if frame.t == 5));
            assert_eq!(
                dropped.load(Ordering::SeqCst),
                0,
                "held while the client reads"
            );
            drop(events);
            for _ in 0..200 {
                if dropped.load(Ordering::SeqCst) == 1 {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("the server kept the stream after the client went");
        })
        .await
        .expect("test timed out");
    }
}
