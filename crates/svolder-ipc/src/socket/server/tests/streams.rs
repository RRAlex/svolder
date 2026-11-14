// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Event streams on a connection: subscriptions, host frames, follow streams and long requests.

use super::*;

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
        let first: Reply =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).expect("a reply line");
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

    async fn stream(
        &self,
        request: Request,
    ) -> Result<futures::stream::BoxStream<'static, Event>, Box<Response>> {
        assert_eq!(request, Request::HostFrames);
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
        let next: Reply = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
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

/// A handler for the stream and long-request tests: the family follow streams are served,
/// one of them is refused, and a `Scan` is held until the test lets it go.
struct Streams {
    release: Arc<tokio::sync::Notify>,
    /// Told when a `Scan` has reached the handler.
    entered: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl Handler for Streams {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Scan { .. } => {
                self.entered.notify_one();
                self.release.notified().await;
                Response::Scan { entries: vec![] }
            }
            _ => Response::Ack,
        }
    }

    async fn subscribe(
        &self,
        _device: Option<String>,
    ) -> futures::stream::BoxStream<'static, Event> {
        futures::stream::pending().boxed()
    }

    async fn stream(
        &self,
        request: Request,
    ) -> Result<futures::stream::BoxStream<'static, Event>, Box<Response>> {
        match request {
            Request::Walk { .. } => Ok(futures::stream::iter(vec![
                Event::Walk(JobEvent {
                    job: 1,
                    state: JobState::Started,
                }),
                Event::Walk(JobEvent {
                    job: 1,
                    state: JobState::Done {
                        payload: serde_json::Value::Null,
                    },
                }),
            ])
            .boxed()),
            _ => Err(Box::new(
                Response::error(CODE_USAGE, "nothing to follow").with_reason(Reason::NoSuchJob),
            )),
        }
    }
}

async fn next_reply(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
) -> Reply {
    serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap()
}

/// A stream the handler gives for a follow request goes out under the request's id,
/// opened with an acknowledgement, and ends with the connection when the stream does.
#[tokio::test]
async fn a_new_stream_opens_with_an_ack_and_then_its_events() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let release = Arc::new(tokio::sync::Notify::new());
        let server = start_server(Arc::new(Streams {
            release,
            entered: Arc::default(),
        }))
        .await;
        let stream = UnixStream::connect(&server.path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let follow = to_line(&RequestLine {
            id: 7,
            request: Request::Walk { op: WalkOp::Follow },
        })
        .unwrap();
        write.write_all(follow.as_bytes()).await.unwrap();
        assert_eq!(
            next_reply(&mut lines).await,
            Reply::Response {
                id: 7,
                response: Response::Ack
            }
        );
        for state in [
            JobState::Started,
            JobState::Done {
                payload: serde_json::Value::Null,
            },
        ] {
            assert_eq!(
                next_reply(&mut lines).await,
                Reply::Event {
                    id: 7,
                    event: Event::Walk(JobEvent { job: 1, state })
                }
            );
        }
        assert!(
            lines.next_line().await.unwrap().is_none(),
            "the stream ended"
        );
    })
    .await
    .expect("test timed out");
}

/// A stream the handler refuses is answered with its refusal, reason kept, and the
/// connection goes on as for any request.
#[tokio::test]
async fn a_refused_stream_is_an_answer_and_the_connection_goes_on() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let release = Arc::new(tokio::sync::Notify::new());
        let server = start_server(Arc::new(Streams {
            release,
            entered: Arc::default(),
        }))
        .await;
        let stream = UnixStream::connect(&server.path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let follow = to_line(&RequestLine {
            id: 1,
            request: Request::AutoEq {
                op: AutoEqOp::Follow,
            },
        })
        .unwrap();
        write.write_all(follow.as_bytes()).await.unwrap();
        assert_eq!(
            next_reply(&mut lines).await,
            Reply::Response {
                id: 1,
                response: Response::error(CODE_USAGE, "nothing to follow")
                    .with_reason(Reason::NoSuchJob)
            }
        );
        let ask = to_line(&RequestLine {
            id: 2,
            request: Request::Devices,
        })
        .unwrap();
        write.write_all(ask.as_bytes()).await.unwrap();
        assert_eq!(
            next_reply(&mut lines).await,
            Reply::Response {
                id: 2,
                response: Response::Ack
            }
        );
    })
    .await
    .expect("test timed out");
}

/// A handler that serves no streams of its own refuses a stream request it was handed, and
/// serves the two that exist through `subscribe` and its own refusal of the frames.
#[tokio::test]
async fn the_default_stream_serves_subscribe_and_refuses_the_rest() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let fake = Fake::new();
        assert!(
            fake.stream(Request::Subscribe {
                device: None,
                host: true
            })
            .await
            .is_ok()
        );
        let Err(frames) = fake.stream(Request::HostFrames).await else {
            panic!("no frames to give");
        };
        assert!(
            matches!(&*frames, Response::Error { code: CODE_USAGE, message, .. }
            if message.contains("no host frames")),
            "{frames:?}"
        );
        let Err(walk) = fake.stream(Request::Walk { op: WalkOp::Follow }).await else {
            panic!("no walk to follow");
        };
        assert!(
            matches!(
                *walk,
                Response::Error {
                    code: CODE_USAGE,
                    ..
                }
            ),
            "{walk:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A request that takes long on one connection holds up nothing on another: the second
/// connection is answered while the first still waits, and the first is answered when its
/// work ends.
#[tokio::test]
async fn a_long_request_on_one_connection_does_not_hold_up_another() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let release = Arc::new(tokio::sync::Notify::new());
        let entered = Arc::new(tokio::sync::Notify::new());
        let server = start_server(Arc::new(Streams {
            release: Arc::clone(&release),
            entered: Arc::clone(&entered),
        }))
        .await;
        let mut slow = crate::socket::client::Client::connect(&server.path)
            .await
            .unwrap();
        let long = tokio::spawn(async move {
            slow.request_within(Request::Scan { window_ms: 1 }, None)
                .await
        });
        entered.notified().await;
        let mut other = crate::socket::client::Client::connect(&server.path)
            .await
            .unwrap();
        let quick = other
            .request_within(Request::Devices, Some(Duration::from_secs(2)))
            .await
            .expect("answered while the long request waits");
        assert_eq!(quick, Response::Ack);
        assert!(!long.is_finished(), "the long request is still waiting");
        release.notify_one();
        let answer = long.await.unwrap().unwrap();
        assert!(matches!(answer, Response::Scan { .. }), "{answer:?}");
    })
    .await
    .expect("test timed out");
}
