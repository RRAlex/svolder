// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon reload` against the stub daemon, a refusing one, and none.

use super::*;
use crate::test_support::stub_daemon::{fresh_socket, stub_daemon};

/// A daemon that reloads is told so once, and the devices it now holds are what is printed.
#[tokio::test]
async fn a_reload_lists_the_devices_now_running() {
    let daemon = stub_daemon().await;
    let said = reload_on(&daemon.socket).await.expect("the reload answers");
    assert_eq!(said, "reloaded: the daemon now holds fake, other");
    assert_eq!(daemon.reloads(), 1);
}

/// A refusal (a file that does not parse) is the daemon's words, a failure, and not the code of
/// a missing daemon.
#[tokio::test]
async fn a_refused_reload_is_a_failure_in_the_daemons_words() {
    let daemon = stub_daemon().await;
    daemon.refuses_reloads("/x/config.toml: line 3: expected `=`");
    let failure = reload_on(&daemon.socket).await.expect_err("refused");
    assert!(
        failure.what.contains("line 3: expected `=`") && failure.what.contains("/x/config.toml"),
        "{}",
        failure.what
    );
    assert_eq!(failure.code, 1, "the daemon's own code");
    assert_ne!(failure.code, Failure::of(Kind::NoDaemon, "").code);
}

/// Nothing answering is the no-daemon failure, naming the socket.
#[tokio::test]
async fn no_daemon_is_the_no_daemon_failure() {
    let socket = fresh_socket();
    let failure = reload_on(&socket).await.expect_err("nothing answers");
    assert_eq!(failure.code, Failure::of(Kind::NoDaemon, "").code);
    assert!(
        failure.what.contains(&socket.display().to_string()),
        "{}",
        failure.what
    );
}

/// A daemon that closes the connection before it answers is the stopped daemon, in the same words
/// every command uses, not a raw socket error.
#[tokio::test]
async fn a_daemon_that_closes_before_it_answers_is_said_to_have_stopped() {
    let socket = fresh_socket();
    let listener = tokio::net::UnixListener::bind(&socket).expect("a listener binds");
    let hang_up = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("a client connects");
        let mut line = String::new();
        let mut reader = tokio::io::BufReader::new(stream);
        tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line)
            .await
            .expect("the request arrives");
    });
    let failure = reload_on(&socket).await.expect_err("no answer");
    hang_up.await.expect("the listener ran");
    let _ = std::fs::remove_file(&socket);
    assert_eq!(failure.what, "The svolder daemon stopped.");
    assert!(
        failure.next.contains("svolder daemon restart"),
        "{}",
        failure.next
    );
}

/// A daemon that is stopping refuses the reload with its reason, and the next step is not to fix
/// a file.
#[tokio::test]
async fn a_stopping_daemons_refusal_does_not_send_the_person_to_the_file() {
    use svolder_ipc::{Handler, Reason, Request, Response};

    struct Stopping;
    #[async_trait::async_trait]
    impl Handler for Stopping {
        async fn handle(&self, _request: Request) -> Response {
            Response::error(1, "the daemon is stopping").with_reason(Reason::Stopping)
        }

        async fn subscribe(
            &self,
            _device: Option<String>,
        ) -> futures::stream::BoxStream<'static, svolder_ipc::Event> {
            futures::stream::pending().boxed()
        }
    }
    use futures::StreamExt;
    let socket = fresh_socket();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let served = tokio::spawn({
        let socket = socket.clone();
        async move {
            let _ = svolder_ipc::serve(&socket, std::sync::Arc::new(Stopping), async {
                let _ = stopped.await;
            })
            .await;
        }
    });
    for _ in 0..500 {
        if svolder_ipc::is_running(&socket).await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let failure = reload_on(&socket).await.expect_err("refused");
    assert!(failure.next.contains("daemon restart"), "{}", failure.next);
    assert!(!failure.next.contains("Fix the file"), "{}", failure.next);
    assert_eq!(failure.reason, Some(Reason::Stopping));
    let _ = stop.send(());
    let _ = served.await;
    let _ = std::fs::remove_file(&socket);
}

/// A reload the daemon refuses for a missing file exits with the daemon's own code (1) and sends
/// the person to restore the file.
#[tokio::test]
async fn a_reload_of_a_missing_file_is_refused_with_code_1_and_says_to_restore_it() {
    let daemon = stub_daemon().await;
    daemon.refuses_reloads(
        &svolder_ipc::socket::daemon_version::config_missing_refusal("/x/config.toml"),
    );
    let failure = reload_on(&daemon.socket).await.expect_err("refused");
    assert_eq!(failure.code, 1);
    assert!(
        failure.what.contains("config.toml is missing"),
        "{}",
        failure.what
    );
    assert!(failure.what.contains("then try again"), "{}", failure.what);
    assert_eq!(failure.next, "", "the words give the step");
}
