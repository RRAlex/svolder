// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The client side of the svolder IPC protocol.

use std::path::Path;
use std::time::Duration;

use futures::StreamExt;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::error::IpcError;
use crate::socket::lines::{Line, LineReader};
use crate::socket::protocol::{
    DEFAULT_REQUEST_TIMEOUT, Event, Reply, Request, RequestLine, Response, to_line,
};

/// A connection to a running daemon.
pub struct Client {
    write: tokio::net::unix::OwnedWriteHalf,
    lines: LineReader<BufReader<tokio::net::unix::OwnedReadHalf>>,
    /// The next request id. A plain counter: every method that reads it takes `self` by value
    /// or by exclusive reference, so nothing ever reads or writes it concurrently.
    next_id: u64,
    /// Set once a reply line ran past the cap: the connection is left mid-line, so no further
    /// request is sent on it.
    broken: bool,
}

impl Client {
    /// Connect to a listening daemon, or `Err` if nothing answers on `path`.
    pub async fn connect(path: &Path) -> Result<Client, IpcError> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|_| IpcError::NotRunning)?;
        let (read, write) = stream.into_split();
        Ok(Client {
            write,
            lines: LineReader::new(BufReader::new(read)),
            next_id: 1,
            broken: false,
        })
    }

    /// Connect to the daemon on `path`, telling no daemon from a socket that cannot be judged:
    /// `Ok(None)` when no socket is there or nothing listens on it (the two answers that mean
    /// no daemon), and the error for anything else (a path this user may not open, one too long
    /// for a socket), behind which a daemon may well run. A front end that writes the
    /// configuration file itself only with no daemon asks here, never [`Client::connect`], whose
    /// every failure reads the same.
    pub async fn connect_if_any(path: &Path) -> std::io::Result<Option<Client>> {
        use std::io::ErrorKind;
        match UnixStream::connect(path).await {
            Ok(stream) => {
                let (read, write) = stream.into_split();
                Ok(Some(Client {
                    write,
                    lines: LineReader::new(BufReader::new(read)),
                    next_id: 1,
                    broken: false,
                }))
            }
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// One request/response round-trip, bounded by [`DEFAULT_REQUEST_TIMEOUT`]; see
    /// [`Client::request_within`] for another bound or none. Events are not expected on this path.
    ///
    /// Refuses a request for which [`Request::is_stream`] holds outright: the server only ever
    /// answers one with events, never with the `Response` this waits for, so sending it through
    /// here would wait forever. Use [`Client::stream`] instead.
    pub async fn request(&mut self, request: Request) -> Result<Response, IpcError> {
        self.request_within(request, Some(DEFAULT_REQUEST_TIMEOUT))
            .await
    }

    /// [`Client::request`] with the wait `within` bounds it (`None` waits as long as the daemon
    /// takes, for a request that runs long on purpose). A request that outlasts the bound fails
    /// with [`IpcError::Timeout`] and the connection stays usable: replies are matched by id, so
    /// the late one is passed over by the next request (a late reply to a line the server could
    /// not read carries id 0 and is taken by the next request as its own). A bound that cuts a
    /// request off while it is still being written leaves the connection mid-line, and it is
    /// then unusable.
    pub async fn request_within(
        &mut self,
        request: Request,
        within: Option<Duration>,
    ) -> Result<Response, IpcError> {
        let Some(within) = within else {
            return self.round_trip(request, &mut true).await;
        };
        let mut sent = false;
        match tokio::time::timeout(within, self.round_trip(request, &mut sent)).await {
            Ok(answer) => answer,
            Err(_) => {
                self.broken |= !sent;
                Err(IpcError::Timeout(within))
            }
        }
    }

    /// Send `request` and read its reply, setting `sent` once the whole line is written.
    async fn round_trip(
        &mut self,
        request: Request,
        sent: &mut bool,
    ) -> Result<Response, IpcError> {
        if request.is_stream() {
            return Err(IpcError::Protocol(format!(
                "a {} request has no Response; use Client::stream",
                request.cmd()
            )));
        }
        if self.broken {
            return Err(IpcError::Protocol(
                "the connection is unusable after an over-long reply; connect again".into(),
            ));
        }
        let id = self.next_id;
        self.next_id += 1;
        let line = to_line(&RequestLine { id, request })?;
        self.write
            .write_all(line.as_bytes())
            .await
            .map_err(IpcError::from_wire)?;
        *sent = true;
        loop {
            let bytes = match self.lines.next_line().await.map_err(IpcError::from_wire)? {
                Line::Bytes(bytes) => bytes,
                Line::TooLong => {
                    self.broken = true;
                    return Err(IpcError::ReplyTooLong);
                }
                Line::Eof => {
                    return Err(IpcError::DaemonGone);
                }
            };
            let reply = match serde_json::from_slice::<Reply>(&bytes) {
                Ok(reply) => reply,
                // A reply the daemon was killed in the middle of writing ends at the stream's
                // end without its newline.
                Err(_) if self.lines.cut_short() => return Err(IpcError::DaemonGone),
                Err(e) => return Err(e.into()),
            };
            match reply {
                Reply::Response { id: rid, response } if rid == id => return Ok(response),
                // The server refuses a line it cannot read under id 0 (it has no id to give),
                // and this sends one request at a time, so that refusal is this request's.
                Reply::Response {
                    id: 0,
                    response: response @ Response::Error { .. },
                } => return Ok(response),
                _ => continue, // ignore anything not this request's response
            }
        }
    }

    /// Turn the connection into an event stream (consumes the client).
    ///
    /// The stream ends when the daemon closes the connection, a read fails, or a line runs past
    /// the cap; a line that cannot be read as an event is passed over.
    pub async fn subscribe(
        self,
        device: Option<String>,
    ) -> Result<futures::stream::BoxStream<'static, Event>, IpcError> {
        self.subscribe_as(device, false).await
    }

    /// [`Client::subscribe`] for a stream that also carries the host's view (`Event::Host`,
    /// once after the connection snapshot and then on each change). The daemon reads the
    /// computer's PipeWire graph for as long as such a stream is open, so only a front end that
    /// shows the host's player or signal path asks for it. A daemon from before it sends none.
    pub async fn subscribe_with_host(
        self,
        device: Option<String>,
    ) -> Result<futures::stream::BoxStream<'static, Event>, IpcError> {
        self.subscribe_as(device, true).await
    }

    async fn subscribe_as(
        self,
        device: Option<String>,
        host: bool,
    ) -> Result<futures::stream::BoxStream<'static, Event>, IpcError> {
        self.stream(Request::Subscribe { device, host }).await
    }

    /// Turn the connection into the event stream `request` asks for (consumes the client):
    /// any request for which [`Request::is_stream`] holds, the way every stream is opened.
    ///
    /// A subscription is a stream from the moment the line is sent: the daemon answers it with
    /// nothing, and it may stay silent until something changes, so the stream is handed back at
    /// once. Every other stream is waited on until the daemon's first line says whether it
    /// opened: its acknowledgement (a stream with no event to send yet is open all the same),
    /// an event (the first of the stream, and kept in it), or an error, which this returns as
    /// [`IpcError::Refused`] with the daemon's whole answer (code, words and reason). A daemon
    /// too old to know the request answers the line it could not read with an error, and so does
    /// this. The wait for that first line has no bound of its own: a caller that must not hang
    /// on a wedged daemon wraps the call in a timeout.
    pub async fn stream(
        mut self,
        request: Request,
    ) -> Result<futures::stream::BoxStream<'static, Event>, IpcError> {
        if !request.is_stream() {
            return Err(IpcError::Protocol(format!(
                "a {} request has an answer, not a stream; use Client::request",
                request.cmd()
            )));
        }
        let waits = request.stream_waits();
        let id = self.next_id;
        self.next_id += 1;
        let line = to_line(&RequestLine { id, request })?;
        self.write
            .write_all(line.as_bytes())
            .await
            .map_err(IpcError::from_wire)?;
        if !waits {
            return Ok(events_of(self.lines, self.write));
        }
        let first = loop {
            let bytes = match self.lines.next_line().await.map_err(IpcError::from_wire)? {
                Line::Bytes(bytes) => bytes,
                Line::TooLong => return Err(IpcError::ReplyTooLong),
                Line::Eof => {
                    return Err(IpcError::DaemonGone);
                }
            };
            match serde_json::from_slice::<Reply>(&bytes) {
                Ok(Reply::Event { event, .. }) if event != Event::Unknown => break Some(event),
                Ok(Reply::Response {
                    id: answered,
                    response: Response::Ack,
                }) if answered == id => break None,
                Ok(Reply::Response {
                    response: refusal @ Response::Error { .. },
                    ..
                }) => return Err(IpcError::Refused(Box::new(refusal))),
                _ => continue,
            }
        };
        let events = events_of(self.lines, self.write);
        Ok(match first {
            Some(first) => futures::stream::once(async move { first })
                .chain(events)
                .boxed(),
            None => events,
        })
    }

    /// Turn the connection into a stream of the daemon's analysed audio frames
    /// (`Event::HostCapture` first, then `Event::HostFrame`s while it captures), which holds the
    /// daemon's capture of the default sink for as long as the stream lives (consumes the
    /// client). A daemon that cannot give them (one from before them, or one with no host audio)
    /// answers with an error instead of an event, and so does this.
    pub async fn host_frames(self) -> Result<futures::stream::BoxStream<'static, Event>, IpcError> {
        self.stream(Request::HostFrames).await
    }
}

/// The events a daemon sends on a connection that became a stream, until it closes, a read
/// fails or a line runs past the cap; a line that cannot be read as an event is passed over.
/// The write half rides along in the state so the connection stays fully open for as long as
/// the caller holds the stream.
fn events_of(
    lines: LineReader<BufReader<tokio::net::unix::OwnedReadHalf>>,
    write: tokio::net::unix::OwnedWriteHalf,
) -> futures::stream::BoxStream<'static, Event> {
    futures::stream::unfold((lines, write), move |(mut lines, write)| async move {
        loop {
            match lines.next_line().await {
                Ok(Line::Bytes(line)) => match serde_json::from_slice::<Reply>(&line) {
                    Ok(Reply::Event { event, .. }) if event != Event::Unknown => {
                        return Some((event, (lines, write)));
                    }
                    // A reply, an event or message kind this build does not know, or a
                    // line that is no JSON at all.
                    Ok(_) | Err(_) => continue,
                },
                _ => return None,
            }
        }
    })
    .boxed()
}

/// One request on a connection made for it alone, bounded by `within` as
/// [`Client::request_within`] bounds it (`None` waits as long as the daemon takes). A front end
/// sends a long request this way, so the connection its reads and writes go over is never held
/// behind it.
pub async fn request_on_own_connection(
    path: &Path,
    request: Request,
    within: Option<Duration>,
) -> Result<Response, IpcError> {
    Client::connect(path)
        .await?
        .request_within(request, within)
        .await
}

/// True if a daemon answers on `path` (a connect that succeeds and is dropped).
pub async fn is_running(path: &Path) -> bool {
    UnixStream::connect(path).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socket::protocol::*;
    use crate::test_support::{Fake, start_server, temp_socket};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn request_and_subscribe_round_trip() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server = start_server(Arc::new(Fake::new())).await;

            let mut client = Client::connect(&server.path).await.unwrap();
            let resp = client
                .request(Request::State { device: None })
                .await
                .unwrap();
            assert!(matches!(resp, Response::Values { .. }));

            let sub = Client::connect(&server.path).await.unwrap();
            let mut events = sub.subscribe(None).await.unwrap();
            let first = events.next().await.expect("one event");
            assert!(matches!(first, Event::Status { .. }));
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn is_running_is_false_without_a_daemon() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            assert!(!is_running(&path).await);
        })
        .await
        .expect("test timed out");
    }

    /// A daemon that closes the connection before it answers is the typed "daemon gone" case,
    /// not a protocol fault.
    #[tokio::test]
    async fn a_connection_closed_before_the_reply_is_the_daemon_gone() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let listener = tokio::net::UnixListener::bind(&path).unwrap();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                // Read the request, then close without a reply: a clean end of the stream.
                let mut line = String::new();
                let mut reader = BufReader::new(stream);
                tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line)
                    .await
                    .unwrap();
                drop(reader);
            });
            let mut client = Client::connect(&path).await.unwrap();
            let err = client
                .request(Request::State { device: None })
                .await
                .unwrap_err();
            let _ = std::fs::remove_file(&path);
            assert!(matches!(err, IpcError::DaemonGone), "{err:?}");
        })
        .await
        .expect("test timed out");
    }

    /// A reply cut off mid-line when the daemon died is the daemon gone, not a JSON error.
    #[tokio::test]
    async fn a_reply_cut_off_mid_line_is_the_daemon_gone() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let listener = tokio::net::UnixListener::bind(&path).unwrap();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line)
                    .await
                    .unwrap();
                let mut stream = reader.into_inner();
                tokio::io::AsyncWriteExt::write_all(&mut stream, b"{\"id\":1,\"respo")
                    .await
                    .unwrap();
            });
            let mut client = Client::connect(&path).await.unwrap();
            let err = client
                .request(Request::State { device: None })
                .await
                .unwrap_err();
            let _ = std::fs::remove_file(&path);
            assert!(matches!(err, IpcError::DaemonGone), "{err:?}");
        })
        .await
        .expect("test timed out");
    }

    /// `request` refuses a `Subscribe` request at once rather than waiting forever for a
    /// `Response` the server will never send one of.
    #[tokio::test]
    async fn request_refuses_a_subscribe_request() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server = start_server(Arc::new(Fake::new())).await;
            let mut client = Client::connect(&server.path).await.unwrap();
            let err = client
                .request(Request::Subscribe {
                    device: None,
                    host: false,
                })
                .await
                .unwrap_err();
            assert!(matches!(err, IpcError::Protocol(_)), "{err}");
        })
        .await
        .expect("test timed out");
    }

    /// A reply line over the cap is refused with an error that says so, rather than buffered
    /// without bound.
    #[tokio::test]
    async fn request_refuses_an_over_long_reply_line() {
        tokio::time::timeout(Duration::from_secs(10), async {
            let path = temp_socket();
            let listener = tokio::net::UnixListener::bind(&path).unwrap();
            let serve = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut sink = [0u8; 256];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut sink).await;
                let junk = vec![b'a'; crate::socket::lines::MAX_LINE + 4096];
                let _ = stream.write_all(&junk).await;
                let _ = stream.write_all(b"\n").await;
                stream
            });
            let mut client = Client::connect(&path).await.unwrap();
            let err = client
                .request(Request::State { device: None })
                .await
                .unwrap_err();
            assert!(matches!(err, IpcError::ReplyTooLong), "{err}");
            assert!(err.to_string().contains("longer than"), "{err}");
            let again = client
                .request(Request::State { device: None })
                .await
                .unwrap_err();
            assert!(matches!(again, IpcError::Protocol(_)), "{again}");
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// A raw server that answers the first request with `lines`, and hands back the stream so
    /// it stays open.
    async fn answer_with(
        path: &std::path::Path,
        lines: String,
    ) -> tokio::task::JoinHandle<tokio::net::UnixStream> {
        let listener = tokio::net::UnixListener::bind(path).unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut sink = [0u8; 256];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut sink).await;
            stream.write_all(lines.as_bytes()).await.unwrap();
            stream
        })
    }

    /// An event kind this build does not know is passed over, and the events after it still
    /// arrive.
    #[tokio::test]
    async fn a_stream_skips_an_event_it_does_not_know() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let known = Event::Connection {
                device: "fake".into(),
                connected: true,
            };
            let known_line = to_line(&Reply::Event {
                id: 1,
                event: known.clone(),
            })
            .unwrap();
            let unknown =
                "{\"msg\":\"event\",\"id\":1,\"event\":{\"event\":\"hologram\",\"x\":1}}\n";
            let serve = answer_with(&path, format!("{unknown}{known_line}")).await;
            let client = Client::connect(&path).await.unwrap();
            let mut events = client.subscribe(None).await.unwrap();
            assert_eq!(events.next().await, Some(known));
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// A response kind this build does not know is an answer, not a failure of the request.
    #[tokio::test]
    async fn a_response_it_does_not_know_is_an_answer_not_a_failure() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let serve = answer_with(
                &path,
                "{\"msg\":\"response\",\"id\":1,\"response\":{\"result\":\"hologram\"}}\n".into(),
            )
            .await;
            let mut client = Client::connect(&path).await.unwrap();
            let answer = client.request(Request::State { device: None }).await;
            assert_eq!(answer.unwrap(), Response::Unknown);
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// A request given a bound that passes with no reply fails with a timeout, and the late reply
    /// is skipped by the request after it, which is answered.
    #[tokio::test]
    async fn a_request_past_its_bound_times_out_and_the_connection_goes_on() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let listener = tokio::net::UnixListener::bind(&path).unwrap();
            let serve = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let (read, mut write) = stream.into_split();
                let mut lines = tokio::io::AsyncBufReadExt::lines(BufReader::new(read));
                let _first = lines.next_line().await.unwrap().unwrap();
                tokio::time::sleep(Duration::from_millis(300)).await;
                for id in [1, 2] {
                    let line = to_line(&Reply::Response {
                        id,
                        response: Response::Ack,
                    })
                    .unwrap();
                    if id == 2 {
                        let _second = lines.next_line().await.unwrap().unwrap();
                    }
                    write.write_all(line.as_bytes()).await.unwrap();
                }
            });
            let mut client = Client::connect(&path).await.unwrap();
            let late = client
                .request_within(Request::Devices, Some(Duration::from_millis(50)))
                .await;
            assert!(matches!(late, Err(IpcError::Timeout(_))), "got {late:?}");
            let answer = client
                .request_within(Request::Devices, Some(Duration::from_secs(3)))
                .await;
            assert_eq!(answer.unwrap(), Response::Ack);
            serve.await.unwrap();
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// A reload's bound covers one deadline per stuck device and one more, for nine devices.
    #[test]
    fn the_reload_bound_covers_nine_stuck_devices() {
        assert!(crate::RELOAD_TIMEOUT >= crate::COMMAND_DEADLINE * (9 + 1));
    }

    /// The default bound outlasts the daemon's worst case for one request, and by a margin.
    #[test]
    fn the_default_bound_outlasts_the_daemons_worst_case() {
        assert!(DEFAULT_REQUEST_TIMEOUT > crate::socket::protocol::COMMAND_WORST_CASE);
        assert_eq!(
            crate::socket::protocol::COMMAND_WORST_CASE,
            crate::COMMAND_DEADLINE * 2,
            "hand-over and answer"
        );
    }

    /// A message kind this build does not know is skipped on the way to the real response.
    #[tokio::test]
    async fn a_message_kind_it_does_not_know_is_skipped() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let ack = to_line(&Reply::Response {
                id: 1,
                response: Response::Ack,
            })
            .unwrap();
            let serve = answer_with(&path, format!("{{\"msg\":\"future\",\"id\":1}}\n{ack}")).await;
            let mut client = Client::connect(&path).await.unwrap();
            let answer = client.request(Request::State { device: None }).await;
            assert_eq!(answer.unwrap(), Response::Ack);
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// An id-0 reply that is not an `Error` is not an answer: the request skips it and waits for
    /// the reply to its own id.
    #[tokio::test]
    async fn an_id_zero_reply_that_is_not_an_error_is_skipped() {
        tokio::time::timeout(Duration::from_secs(2), async {
            let path = temp_socket();
            let own = to_line(&Reply::Response {
                id: 1,
                response: Response::error(CODE_NOT_FOUND, "its own"),
            })
            .unwrap();
            let serve = answer_with(
                &path,
                format!(
                    "{{\"msg\":\"response\",\"id\":0,\"response\":{{\"result\":\"ack\"}}}}\n{own}"
                ),
            )
            .await;
            let mut client = Client::connect(&path).await.unwrap();
            let answer = client.request(Request::State { device: None }).await;
            assert_eq!(answer.unwrap(), Response::error(CODE_NOT_FOUND, "its own"));
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// The server refuses a line it cannot read with an error carrying id 0; the request that
    /// is waiting takes it as its answer rather than waiting for a reply that never comes.
    #[tokio::test]
    async fn an_id_zero_error_answers_the_request_that_is_waiting() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let serve = answer_with(
                &path,
                concat!(
                    "{\"msg\":\"response\",\"id\":0,\"response\":{\"result\":\"error\",",
                    "\"code\":1,\"message\":\"bad request: x\"}}\n"
                )
                .into(),
            )
            .await;
            let mut client = Client::connect(&path).await.unwrap();
            let answer = tokio::time::timeout(
                Duration::from_secs(2),
                client.request(Request::State { device: None }),
            )
            .await
            .expect("the request was answered, not left waiting");
            assert_eq!(
                answer.unwrap(),
                Response::error(CODE_USAGE, "bad request: x")
            );
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// A line on the event stream that cannot be read as an event (a kind this build does not
    /// know, or no JSON at all) is passed over, and the events after it still arrive.
    #[tokio::test]
    async fn subscribe_passes_over_a_line_it_cannot_read() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let known = Event::Connection {
                device: "fake".into(),
                connected: true,
            };
            let known_line = to_line(&Reply::Event {
                id: 1,
                event: known.clone(),
            })
            .unwrap();
            let brand_new = concat!(
                "{\"msg\":\"event\",\"id\":1,",
                "\"event\":{\"event\":\"brand_new\",\"device\":\"fake\"}}\n"
            );
            let serve = answer_with(&path, format!("{brand_new}not json\n{known_line}")).await;
            let client = Client::connect(&path).await.unwrap();
            let mut events = client.subscribe(None).await.unwrap();
            assert_eq!(events.next().await, Some(known));
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }
    /// `host_frames` against a daemon that does not know the request (one from before it
    /// answers the line it cannot read with an error) is an error, which a front end reads as
    /// "capture on my own".
    #[tokio::test]
    async fn host_frames_on_a_daemon_that_does_not_know_it_is_an_error() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let refusal = to_line(&Reply::Response {
                id: 0,
                response: Response::error(CODE_USAGE, "bad request: unknown variant `host_frames`"),
            })
            .unwrap();
            let serve = answer_with(&path, refusal).await;
            let client = Client::connect(&path).await.unwrap();
            let Err(err) = client.host_frames().await else {
                panic!("an old daemon gave a stream");
            };
            assert!(err.to_string().contains("host_frames"), "{err}");
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// The first event a frames request gets is part of the stream it returns, not read away
    /// to see whether the daemon knew the request.
    #[tokio::test]
    async fn host_frames_keeps_the_first_event() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let capture = Event::HostCapture(crate::host::CaptureView {
                state: crate::host::CaptureKind::Running,
                sink: Some("Schiit Gungnir 2".into()),
                why: None,
            });
            let lines = to_line(&Reply::Event {
                id: 1,
                event: capture.clone(),
            })
            .unwrap();
            let serve = answer_with(&path, lines).await;
            let client = Client::connect(&path).await.unwrap();
            let mut events = client.host_frames().await.unwrap();
            assert_eq!(events.next().await, Some(capture));
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// `request` refuses a frames request as it does a subscription: no answer would come.
    #[tokio::test]
    async fn request_refuses_a_host_frames_request() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server = start_server(Arc::new(Fake::new())).await;
            let mut client = Client::connect(&server.path).await.unwrap();
            let err = client.request(Request::HostFrames).await.unwrap_err();
            assert!(matches!(err, IpcError::Protocol(_)), "{err}");
        })
        .await
        .expect("test timed out");
    }

    /// A follow stream is opened by its acknowledgement and then reads as events; the ack is
    /// not an event, and the first event is not lost to it.
    #[tokio::test]
    async fn stream_opens_on_the_ack_and_reads_the_events() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let ack = to_line(&Reply::Response {
                id: 1,
                response: Response::Ack,
            })
            .unwrap();
            let event = Event::Walk(JobEvent {
                job: 2,
                state: JobState::Started,
            });
            let line = to_line(&Reply::Event {
                id: 1,
                event: event.clone(),
            })
            .unwrap();
            let serve = answer_with(&path, format!("{ack}{line}")).await;
            let client = Client::connect(&path).await.unwrap();
            let mut events = client
                .stream(Request::Walk { op: WalkOp::Follow })
                .await
                .unwrap();
            assert_eq!(events.next().await, Some(event));
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// A follow stream that opens with nothing but the ack is a stream, not an error: it may
    /// stay silent for as long as no job changes.
    #[tokio::test]
    async fn stream_opened_by_an_ack_alone_is_open() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let ack = to_line(&Reply::Response {
                id: 1,
                response: Response::Ack,
            })
            .unwrap();
            let serve = answer_with(&path, ack).await;
            let client = Client::connect(&path).await.unwrap();
            let events = client
                .stream(Request::AutoEq {
                    op: AutoEqOp::Follow,
                })
                .await;
            assert!(events.is_ok());
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// A stream the daemon refuses (or a daemon too old to know it, answering the line it could
    /// not read) is an error that carries the daemon's whole answer: code, words and reason.
    #[tokio::test]
    async fn stream_refused_is_an_error_with_the_daemons_words() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let path = temp_socket();
            let refusal = to_line(&Reply::Response {
                id: 1,
                response: Response::unknown_request("walk"),
            })
            .unwrap();
            let serve = answer_with(&path, refusal).await;
            let client = Client::connect(&path).await.unwrap();
            let Err(err) = client.stream(Request::Walk { op: WalkOp::Follow }).await else {
                panic!("a refused stream gave a stream");
            };
            assert!(err.to_string().contains("walk"), "{err}");
            let IpcError::Refused(answer) = err else {
                panic!("the refusal is typed: {err:?}");
            };
            assert!(matches!(
                *answer,
                Response::Error {
                    code: CODE_UNKNOWN_REQUEST,
                    ..
                }
            ));
            let _keep = serve.await;
            let _ = std::fs::remove_file(&path);
        })
        .await
        .expect("test timed out");
    }

    /// A refusal keeps its reason, so `busy` and `no_such_job` are told apart by it.
    #[tokio::test]
    async fn a_refused_stream_keeps_its_code_and_reason() {
        tokio::time::timeout(Duration::from_secs(5), async {
            for reason in [Reason::Busy, Reason::NoSuchJob] {
                let path = temp_socket();
                let refusal = to_line(&Reply::Response {
                    id: 1,
                    response: Response::error(CODE_USAGE, "no").with_reason(reason),
                })
                .unwrap();
                let serve = answer_with(&path, refusal).await;
                let client = Client::connect(&path).await.unwrap();
                let Err(IpcError::Refused(answer)) =
                    client.stream(Request::Walk { op: WalkOp::Follow }).await
                else {
                    panic!("a typed refusal");
                };
                assert_eq!(
                    *answer,
                    Response::error(CODE_USAGE, "no").with_reason(reason)
                );
                let _keep = serve.await;
                let _ = std::fs::remove_file(&path);
            }
        })
        .await
        .expect("test timed out");
    }

    /// `stream` refuses a request that is not a stream, which would answer with a response.
    #[tokio::test]
    async fn stream_refuses_a_request_that_is_not_a_stream() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server = start_server(Arc::new(Fake::new())).await;
            let client = Client::connect(&server.path).await.unwrap();
            let Err(err) = client.stream(Request::Devices).await else {
                panic!("a plain request gave a stream");
            };
            assert!(matches!(err, IpcError::Protocol(_)), "{err}");
        })
        .await
        .expect("test timed out");
    }

    /// `request` refuses the follow ops as it does a subscription: no response would come.
    #[tokio::test]
    async fn request_refuses_a_follow_request() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server = start_server(Arc::new(Fake::new())).await;
            let mut client = Client::connect(&server.path).await.unwrap();
            let err = client
                .request(Request::Walk { op: WalkOp::Follow })
                .await
                .unwrap_err();
            assert!(matches!(err, IpcError::Protocol(_)), "{err}");
            assert!(err.to_string().contains("walk"), "{err}");
        })
        .await
        .expect("test timed out");
    }

    /// A request on a connection of its own is answered while another connection is held by a
    /// stream, and the helper bounds the wait as `request_within` does.
    #[tokio::test]
    async fn a_request_on_its_own_connection_is_answered_beside_a_held_one() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let server = start_server(Arc::new(Fake::new())).await;
            let held = Client::connect(&server.path).await.unwrap();
            let _stream = held.subscribe(None).await.unwrap();
            let answer = request_on_own_connection(
                &server.path,
                Request::State { device: None },
                Some(Duration::from_secs(2)),
            )
            .await
            .unwrap();
            assert!(matches!(answer, Response::Values { .. }), "{answer:?}");
            let none =
                request_on_own_connection(&temp_socket(), Request::State { device: None }, None)
                    .await;
            assert!(matches!(none, Err(IpcError::NotRunning)));
            // No socket, and a file nothing listens on, are no daemon; a path that cannot be
            // tried is an error, not an absence.
            let absent = temp_socket();
            assert!(matches!(Client::connect_if_any(&absent).await, Ok(None)));
            std::fs::write(&absent, "").unwrap();
            assert!(matches!(Client::connect_if_any(&absent).await, Ok(None)));
            std::fs::remove_file(&absent).unwrap();
            let long = absent.with_file_name(format!("{}.sock", "s".repeat(200)));
            assert!(Client::connect_if_any(&long).await.is_err());
        })
        .await
        .expect("test timed out");
    }
}
