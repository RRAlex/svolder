// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's host service end to end: a real `svolderd` over the fake device and the fake
//! host (`SVOLDER_HOST=fake`), a real socket and a real `svolder_ipc::Client`. No session bus,
//! player or PipeWire is reached.

mod common;

use futures::StreamExt;
use svolder_ipc::{Client, Event, Request, Response};

use common::{CONFIG, TEST_TIMEOUT, connect, spawn_daemon_with_env};

/// The first host view on `events`, from here on, that `wanted` accepts.
async fn next_host(
    events: &mut (impl futures::Stream<Item = Event> + Unpin),
    wanted: impl Fn(&svolder_ipc::host::HostView) -> bool,
) -> svolder_ipc::host::HostView {
    loop {
        if let Event::Host(view) = events.next().await.expect("the stream is open")
            && wanted(&view)
        {
            return view;
        }
    }
}

/// The host's view, a press, and the view after it, over the socket; the same view arrives on
/// a subscription, and a press with a word that is no button is refused.
#[tokio::test]
async fn the_fake_host_answers_over_the_socket() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) = spawn_daemon_with_env(CONFIG, &[("SVOLDER_HOST", "fake")]);
        let mut client = connect(&mut guard, &socket).await;

        let Response::Host(view) = client.request(Request::Host).await.expect("an answer") else {
            panic!("not a host answer");
        };
        assert!(view.available);
        assert_eq!(view.playing.expect("the fake player").status, "playing");

        let mut events = Client::connect(&socket)
            .await
            .expect("a second connection")
            .subscribe_with_host(None)
            .await
            .expect("a subscription");

        // Someone watches now, so the route is read and arrives as a later view.
        let with_route = next_host(&mut events, |view| view.route.is_some()).await;
        assert!(with_route.available);

        let pause = Request::HostPress {
            button: "pause".into(),
        };
        assert_eq!(
            client.request(pause).await.expect("an answer"),
            Response::Ack
        );
        let paused = next_host(&mut events, |view| {
            view.playing.as_ref().is_some_and(|p| p.status == "paused")
        })
        .await;
        assert!(
            paused.route.is_some(),
            "the route stays while someone watches"
        );

        let rewind = Request::HostPress {
            button: "rewind".into(),
        };
        let refused = client.request(rewind).await.expect("an answer");
        assert!(matches!(refused, Response::Error { .. }), "{refused:?}");
    })
    .await
    .expect("test timed out");
}

/// With the host off (the tests' default) the daemon says so and takes no press.
#[tokio::test]
async fn an_off_host_is_unavailable() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) = spawn_daemon_with_env(CONFIG, &[]);
        let mut client = connect(&mut guard, &socket).await;
        let Response::Host(view) = client.request(Request::Host).await.expect("an answer") else {
            panic!("not a host answer");
        };
        assert!(!view.available);
        let press = Request::HostPress {
            button: "play".into(),
        };
        let refused = client.request(press).await.expect("an answer");
        assert!(matches!(refused, Response::Error { .. }), "{refused:?}");
    })
    .await
    .expect("test timed out");
}
