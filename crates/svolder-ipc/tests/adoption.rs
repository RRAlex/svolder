// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The systemd socket handover: what [`listen_fds`] adopts, what it refuses, and that
//! [`serve_on`] answers on the descriptor it was handed without binding a socket of its own.
//!
//! Everything here runs from one `#[test]` in a test binary of its own, so this thread is the
//! only one in the process while it runs. That is what makes the handover safe to simulate:
//! it borrows descriptor 3 and writes the environment, and neither is safe beside another
//! thread reading either of them.

use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_ipc::socket::server::{Handler, listen_fds, serve_on};
use svolder_ipc::{Client, Event, Request, Response};

/// The descriptor a service manager passes the first listening socket on.
const LISTEN_FDS_START: i32 = 3;

/// The version the handler under test answers with, which is all this needs to tell a served
/// socket from a bound one nobody is listening to.
const VERSION: &str = "handed-over";

/// A handler that answers every request with [`VERSION`].
struct Answering;

#[async_trait::async_trait]
impl Handler for Answering {
    async fn handle(&self, _request: Request) -> Response {
        Response::Version {
            version: VERSION.to_string(),
            mqtt: None,
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        }
    }

    async fn subscribe(&self, _device: Option<String>) -> BoxStream<'static, Event> {
        futures::stream::pending().boxed()
    }
}

/// A socket path of this test's own, whose file is removed when the test lets go of it —
/// whether the test passed or an assertion failed on the way.
struct TempSocket(PathBuf);

impl Drop for TempSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A socket path of this test's own.
fn temp_socket() -> TempSocket {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    TempSocket(
        std::env::temp_dir().join(format!("svolder-adoption-{}-{n}.sock", std::process::id())),
    )
}

/// Set `name`, or remove it when `value` is `None`.
///
/// # Safety
///
/// Nothing else in the process may read or write the environment concurrently, which is why
/// this test binary holds exactly one test.
unsafe fn set_env(name: &str, value: Option<&str>) {
    match value {
        Some(value) => unsafe { std::env::set_var(name, value) },
        None => unsafe { std::env::remove_var(name) },
    }
}

/// Hand `listener` to `f` the way systemd hands a socket to a service it started: as
/// descriptor 3, with `LISTEN_FDS=1` and no `LISTEN_PID`.
///
/// Descriptor 3 is borrowed rather than taken. A test binary is started by cargo with
/// descriptors of its own already open, and one of them is number 3, so this saves whatever
/// is there, lends the number to the handover, moves what `f` adopted onto a descriptor of
/// its own and gives number 3 back. The environment goes back too. Nothing else in the
/// process can look at either in between: this binary holds one test and has not started a
/// runtime yet.
fn with_listen_fds(
    listener: std::os::unix::net::UnixListener,
    f: impl FnOnce() -> Option<std::os::unix::net::UnixListener>,
) -> Option<std::os::unix::net::UnixListener> {
    // SAFETY: a copy of whatever descriptor 3 holds, closed again at the end of this call.
    let saved = unsafe { libc::dup(LISTEN_FDS_START) };
    // SAFETY: `listener` owns the descriptor being duplicated and outlives the duplicate;
    // what descriptor 3 held is in `saved` and goes back there below.
    let duplicated = unsafe { libc::dup2(listener.as_raw_fd(), LISTEN_FDS_START) };
    assert_eq!(
        duplicated,
        LISTEN_FDS_START,
        "the listener could not be put on descriptor {LISTEN_FDS_START}: {}",
        std::io::Error::last_os_error()
    );

    // SAFETY: single-threaded, as the module comment explains.
    unsafe {
        set_env("LISTEN_FDS", Some("1"));
        set_env("LISTEN_PID", None);
    }
    let adopted = f();
    // SAFETY: as above.
    unsafe { set_env("LISTEN_FDS", None) };

    // Free descriptor 3 again: what the handover produced moves to a descriptor of its own,
    // and what it did not adopt is closed.
    let adopted = adopted.map(|adopted| {
        let lent = adopted.into_raw_fd();
        // SAFETY: `lent` is the descriptor just adopted, and the listener that owned it has
        // given it up, so nothing else holds it.
        let moved = unsafe { libc::fcntl(lent, libc::F_DUPFD_CLOEXEC, 16) };
        assert!(moved >= 0, "{}", std::io::Error::last_os_error());
        // SAFETY: `lent` is now the spare of the two, and `moved` has no other owner.
        unsafe {
            libc::close(lent);
            std::os::unix::net::UnixListener::from_raw_fd(moved)
        }
    });
    if adopted.is_none() {
        // SAFETY: descriptor 3 is the copy made above, which nothing else owns.
        unsafe { libc::close(LISTEN_FDS_START) };
    }
    if saved >= 0 {
        // SAFETY: `saved` is this thread's copy of what descriptor 3 held; putting it back
        // leaves the process with exactly the descriptors it started with.
        unsafe {
            libc::dup2(saved, LISTEN_FDS_START);
            libc::close(saved);
        }
    }
    adopted
}

/// The whole handover: every refusal first, then the one adoption a process gets.
#[test]
fn the_server_adopts_a_listener_handed_over_by_systemd() {
    nothing_is_adopted_without_a_handover();

    let socket = temp_socket();
    let path = &socket.0;
    let listener = std::os::unix::net::UnixListener::bind(path).expect("bind the test socket");
    let adopted = with_listen_fds(listener, listen_fds).expect("the handover is adopted");
    adopted
        .set_nonblocking(true)
        .expect("the adopted listener takes non-blocking mode");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let listener =
            tokio::net::UnixListener::from_std(adopted).expect("the adopted listener is a socket");
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let served = tokio::spawn(async move {
            serve_on(listener, Arc::new(Answering), async {
                let _ = stopped.await;
            })
            .await
        });

        // The socket was bound before the server started, which is the whole point of the
        // handover: a client reaches the path without waiting for anything to bind it.
        let mut client = Client::connect(path)
            .await
            .expect("the handed-over socket answers");
        let answer = tokio::time::timeout(Duration::from_secs(5), client.request(Request::Version))
            .await
            .expect("the adopted listener was served within five seconds")
            .expect("a version reply");
        assert!(
            matches!(&answer, Response::Version { version, .. } if version == VERSION),
            "got {answer:?}"
        );

        let _ = stop.send(());
        served
            .await
            .expect("the server task finished")
            .expect("the server ended cleanly");
        assert!(
            path.exists(),
            "a handed-over socket file belongs to whoever bound it and must be left behind"
        );
    });
}

/// Nothing is adopted unless the environment names exactly one descriptor, for this process.
///
/// Every case here is turned away before descriptor 3 is looked at, so none of them spends
/// the one adoption a process gets.
fn nothing_is_adopted_without_a_handover() {
    // SAFETY: single-threaded, as the module comment explains.
    unsafe { set_env("LISTEN_PID", None) };
    assert!(
        listen_fds().is_none(),
        "a process nobody handed a socket to adopts nothing"
    );

    for count in ["0", "2", "", "many"] {
        // SAFETY: as above.
        unsafe { set_env("LISTEN_FDS", Some(count)) };
        assert!(
            listen_fds().is_none(),
            "LISTEN_FDS={count:?} is not one socket on descriptor {LISTEN_FDS_START}"
        );
    }

    // A handover meant for another process, which is what a descriptor inherited by a child
    // looks like: refused, so the child does not serve its parent's socket.
    let other = std::process::id() + 1;
    // SAFETY: as above.
    unsafe {
        set_env("LISTEN_FDS", Some("1"));
        set_env("LISTEN_PID", Some(&other.to_string()));
    }
    assert!(
        listen_fds().is_none(),
        "a handover addressed to process {other} is not this process's"
    );

    // SAFETY: as above.
    unsafe {
        set_env("LISTEN_FDS", None);
        set_env("LISTEN_PID", None);
    }
}
