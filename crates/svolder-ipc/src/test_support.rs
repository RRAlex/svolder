// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared fixtures for the crate's tests: a fake handler, a temp-socket helper, a
//! self-cleaning running-server fixture, a self-cleaning temporary path and directory, and the
//! one way a test writes a script it runs.

#[path = "../../svolder-host/tests/common/script.rs"]
mod script;

pub(crate) use script::install_script;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::StreamExt;
use futures::stream::BoxStream;

use crate::error::IpcError;
use crate::socket::protocol::*;
use crate::socket::server::{Handler, serve};

/// A path under the system temp dir that one case owns — a directory, or a socket file —
/// removed when the case lets go of it, whether the case passed or an assertion failed first.
pub(crate) struct TempPath(pub(crate) PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.0).is_err() {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

impl std::ops::Deref for TempPath {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TempPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// A directory of this test's own under the system temp dir, named `svolder-{what}-…` and empty to
/// start with.
///
/// Every writer case works inside one of these, so nothing a test writes lands anywhere near a
/// real configuration and two cases never share a file; it is removed with everything in it when
/// the case ends, however it ends.
pub(crate) fn temp_dir(what: &str) -> TempPath {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let at = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("svolder-{what}-{}-{at}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the temp directory is made");
    TempPath(dir)
}

/// A unique socket path under the system temp directory.
///
/// The counter is what makes two calls in the same nanosecond distinct: tests run on several
/// threads at once, and a clock coarser than the name it labels would otherwise let two of
/// them race for the same path.
pub(crate) fn temp_socket() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "svolder-test-{}-{n}-{seq}.sock",
        std::process::id()
    ))
}

/// A `serve()` instance spawned for a test. Dropping it sends the shutdown signal (if not
/// already sent) and removes the socket file, so a failed assertion or a fired timeout in the
/// test body never leaves a `svolder-test-*.sock` behind.
pub(crate) struct ServerFixture {
    pub(crate) path: PathBuf,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<Result<(), IpcError>>>,
}

impl Drop for ServerFixture {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Spawn `serve` with `handler` on a fresh temp socket and wait for it to come up.
pub(crate) async fn start_server<H: Handler>(handler: Arc<H>) -> ServerFixture {
    start_server_at(temp_socket(), handler).await
}

/// Spawn `serve` with `handler` at a caller-chosen path (for the test that needs a stale
/// socket file already sitting at that exact path before `serve` binds it) and wait for the
/// socket to appear.
///
/// `serve`'s own result is kept, not discarded: a bind failure would otherwise surface only as
/// a baffling connect failure in whichever test hit the missing socket first, rather than as
/// the daemon's own reason here.
pub(crate) async fn start_server_at<H: Handler>(path: PathBuf, handler: Arc<H>) -> ServerFixture {
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let p2 = path.clone();
    let join = tokio::spawn(async move {
        serve(&p2, handler, async {
            let _ = rx.await;
        })
        .await
    });
    for _ in 0..50 {
        if path.exists() || join.is_finished() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    if !path.exists() {
        let outcome = join.await;
        panic!("the server never bound {}: {outcome:?}", path.display());
    }
    ServerFixture {
        path,
        shutdown: Some(tx),
        join: Some(join),
    }
}

/// Build a minimal `ValueObject` for tests.
pub(crate) fn vo(id: &str, key: &str) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: Some(key.into()),
        label: key.into(),
        value_type: "u8".into(),
        raw: serde_json::json!(1),
        display: "1".into(),
        value: None,
        unit: None,
    }
}

/// A handler that answers `State` with one canned value and `Subscribe` with a two-item
/// stream that never ends on its own (so tests can exercise EOF-driven cleanup).
pub(crate) struct Fake {
    /// Incremented each time a stream this handler returned is dropped by the server.
    pub(crate) streams_dropped: Arc<AtomicUsize>,
}

impl Fake {
    /// A `Fake` that does not track stream drops.
    pub(crate) fn new() -> Self {
        Fake {
            streams_dropped: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// A `Fake` sharing the given drop counter, for the subscriber-cleanup test.
    pub(crate) fn with_counter(streams_dropped: Arc<AtomicUsize>) -> Self {
        Fake { streams_dropped }
    }
}

#[async_trait::async_trait]
impl Handler for Fake {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::State { .. } => Response::Values {
                device: "fake".into(),
                values: vec![vo("10", "volume")],
            },
            _ => Response::Error {
                code: 1,
                message: "no".into(),
            },
        }
    }

    async fn subscribe(&self, _device: Option<String>) -> BoxStream<'static, Event> {
        use futures::stream;
        let events = stream::iter(vec![
            Event::Status {
                device: "fake".into(),
                reading: vo("10", "volume"),
            },
            Event::Status {
                device: "fake".into(),
                reading: vo("11", "balance"),
            },
        ])
        .chain(stream::pending());
        DropCounted {
            inner: events.boxed(),
            counter: Arc::clone(&self.streams_dropped),
        }
        .boxed()
    }
}

/// Wraps a stream and increments a shared counter when it is dropped, so tests can observe
/// that the server has released a subscriber's event stream.
pub(crate) struct DropCounted {
    inner: BoxStream<'static, Event>,
    counter: Arc<AtomicUsize>,
}

impl DropCounted {
    /// `inner`, counting its drop on `counter`.
    pub(crate) fn new(inner: BoxStream<'static, Event>, counter: Arc<AtomicUsize>) -> Self {
        DropCounted { inner, counter }
    }
}

impl futures::Stream for DropCounted {
    type Item = Event;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl Drop for DropCounted {
    fn drop(&mut self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }
}
