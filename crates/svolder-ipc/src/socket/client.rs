// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The client side of the svolder IPC protocol.

use std::path::Path;

use futures::StreamExt;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::error::IpcError;
use crate::socket::lines::{Line, LineReader};
use crate::socket::protocol::{Event, Reply, Request, RequestLine, Response, to_line};

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

    /// One request/response round-trip. Events are not expected on this path.
    ///
    /// Refuses a `Subscribe` request outright: the server only ever answers one with events,
    /// never with the `Response` this waits for, so sending it through here would wait
    /// forever. Use [`Client::subscribe`] instead.
    pub async fn request(&mut self, request: Request) -> Result<Response, IpcError> {
        if matches!(request, Request::Subscribe { .. }) {
            return Err(IpcError::Protocol(
                "a Subscribe request has no Response; use Client::subscribe".into(),
            ));
        }
        if matches!(request, Request::HostFrames) {
            return Err(IpcError::Protocol(
                "a HostFrames request has no Response; use Client::host_frames".into(),
            ));
        }
        if self.broken {
            return Err(IpcError::Protocol(
                "the connection is unusable after an over-long reply; connect again".into(),
            ));
        }
        let id = self.next_id;
        self.next_id += 1;
        let line = to_line(&RequestLine { id, request })?;
        self.write.write_all(line.as_bytes()).await?;
        loop {
            let bytes = match self.lines.next_line().await? {
                Line::Bytes(bytes) => bytes,
                Line::TooLong => {
                    self.broken = true;
                    return Err(IpcError::ReplyTooLong);
                }
                Line::Eof => {
                    return Err(IpcError::Protocol("daemon closed the connection".into()));
                }
            };
            match serde_json::from_slice::<Reply>(&bytes)? {
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
        mut self,
        device: Option<String>,
        host: bool,
    ) -> Result<futures::stream::BoxStream<'static, Event>, IpcError> {
        let id = self.next_id;
        self.next_id += 1;
        let line = to_line(&RequestLine {
            id,
            request: Request::Subscribe { device, host },
        })?;
        self.write.write_all(line.as_bytes()).await?;
        Ok(events_of(self.lines, self.write))
    }

    /// Turn the connection into a stream of the daemon's analysed audio frames
    /// (`Event::HostCapture` first, then `Event::HostFrame`s while it captures), which holds the
    /// daemon's capture of the default sink for as long as the stream lives (consumes the
    /// client). A daemon that cannot give them (one from before them, or one with no host audio)
    /// answers with an error instead of an event, and so does this.
    pub async fn host_frames(
        mut self,
    ) -> Result<futures::stream::BoxStream<'static, Event>, IpcError> {
        let id = self.next_id;
        self.next_id += 1;
        let line = to_line(&RequestLine {
            id,
            request: Request::HostFrames,
        })?;
        self.write.write_all(line.as_bytes()).await?;
        // The daemon answers at once, with the capture's state or with an error: the first
        // line says which, and an event is the first of the stream.
        let first = loop {
            let bytes = match self.lines.next_line().await? {
                Line::Bytes(bytes) => bytes,
                Line::TooLong => return Err(IpcError::ReplyTooLong),
                Line::Eof => {
                    return Err(IpcError::Protocol("daemon closed the connection".into()));
                }
            };
            match serde_json::from_slice::<Reply>(&bytes) {
                Ok(Reply::Event { event, .. }) if event != Event::Unknown => break event,
                Ok(Reply::Response {
                    response: Response::Error { message, .. },
                    ..
                }) => return Err(IpcError::Protocol(message)),
                _ => continue,
            }
        };
        Ok(futures::stream::once(async move { first })
            .chain(events_of(self.lines, self.write))
            .boxed())
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
                response: Response::Error {
                    code: 2,
                    message: "its own".into(),
                },
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
            assert_eq!(
                answer.unwrap(),
                Response::Error {
                    code: 2,
                    message: "its own".into()
                }
            );
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
                "{\"msg\":\"response\",\"id\":0,\"response\":{\"result\":\"error\",\"code\":1,\"message\":\"bad request: x\"}}\n".into(),
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
                Response::Error {
                    code: 1,
                    message: "bad request: x".into()
                }
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
            let brand_new = "{\"msg\":\"event\",\"id\":1,\"event\":{\"event\":\"brand_new\",\"device\":\"fake\"}}\n";
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
                response: Response::Error {
                    code: 1,
                    message: "bad request: unknown variant `host_frames`".into(),
                },
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
}
