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

/// The computer's volume and EQ over a real socket: in the view, in the subscription (a drag
/// reaches a watcher as its last value), and the fake knob that takes `pw-cli` away.
#[tokio::test]
async fn the_fake_computer_answers_over_the_socket() {
    use svolder_ipc::HostEqOp;
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) = spawn_daemon_with_env(CONFIG, &[("SVOLDER_HOST", "fake")]);
        let mut client = connect(&mut guard, &socket).await;
        let Response::Host(view) = client.request(Request::Host).await.expect("an answer") else {
            panic!("not a host answer");
        };
        assert_eq!(view.volume.as_ref().map(|v| v.pct), Some(42));
        assert!(view.eq.is_some_and(|eq| !eq.on && eq.points.len() == 1));

        let mut events = Client::connect(&socket)
            .await
            .expect("a second connection")
            .subscribe_with_host(None)
            .await
            .expect("a subscription");
        assert_eq!(
            client
                .request(Request::HostVolume { pct: 70 })
                .await
                .expect("an answer"),
            Response::Ack
        );
        next_host(&mut events, |view| {
            view.volume.as_ref().is_some_and(|v| v.pct == 70)
        })
        .await;

        let on = Request::HostEq {
            op: HostEqOp::Mode { on: true },
        };
        assert_eq!(client.request(on).await.expect("an answer"), Response::Ack);
        let drag = |g, done| Request::HostEq {
            op: HostEqOp::Set {
                index: 1,
                slot: None,
                kind: None,
                f: None,
                g: Some(g),
                q: None,
                done,
            },
        };
        for g in [1.0, 2.0, 3.0] {
            client.request(drag(g, false)).await.expect("an answer");
        }
        client.request(drag(4.0, true)).await.expect("an answer");
        let ended = next_host(&mut events, |view| {
            view.eq
                .as_ref()
                .is_some_and(|eq| eq.on && eq.points.first().is_some_and(|p| p.g == 4.0))
        })
        .await;
        assert!(ended.eq.is_some());
    })
    .await
    .expect("test timed out");
}

/// A fake computer with no `pw-cli` says what is missing, and takes no EQ change.
#[tokio::test]
async fn a_fake_computer_without_pw_cli_is_unavailable() {
    use svolder_ipc::HostEqOp;
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) = spawn_daemon_with_env(
            CONFIG,
            &[("SVOLDER_HOST", "fake"), ("SVOLDER_FAKE_HOST_EQ", "off")],
        );
        let mut client = connect(&mut guard, &socket).await;
        let Response::Host(view) = client.request(Request::Host).await.expect("an answer") else {
            panic!("not a host answer");
        };
        assert_eq!(
            view.eq.and_then(|eq| eq.unavailable).as_deref(),
            Some("computer controls unavailable: no pw-cli on this computer")
        );
        let on = Request::HostEq {
            op: HostEqOp::Mode { on: true },
        };
        assert!(matches!(
            client.request(on).await.expect("an answer"),
            Response::Error { .. }
        ));
    })
    .await
    .expect("test timed out");
}
