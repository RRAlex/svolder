// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's answers about the host: its view, the presses and the events on a subscription,
//! over the fake host so no session bus or player is ever reached.

use svolder_ipc::host::{CaptureKind, HostView};

use super::tests::{amp_entry, daemon, daemon_over};
use super::*;

/// Let every task that can run, run, and give the blocking pool (where covers are read) real
/// time to answer; the paused clock does not move meanwhile.
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The view the daemon answers with once `done` holds of it: settle, look, and look again after
/// a few real milliseconds, for up to a second, because a cover is read on the blocking pool
/// whose pace the paused clock and the yields cannot see.
async fn view_when(daemon: &Daemon, what: &str, done: impl Fn(&HostView) -> bool) -> HostView {
    let started = std::time::Instant::now();
    loop {
        settle().await;
        let view = view_of(daemon.handle(Request::Host).await);
        if done(&view) {
            return view;
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "no view with {what} within a second: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A daemon over the fake host, started.
fn fake_host_daemon() -> Daemon {
    let daemon = daemon().with_host_mode(HostMode::Fake);
    daemon.host_to(&HostConfig::default());
    daemon
}

/// The view an answer carries.
fn view_of(response: Response) -> HostView {
    match response {
        Response::Host(view) => view,
        other => panic!("not a host answer: {other:?}"),
    }
}

/// The next host view on `events`, whatever else comes before it.
async fn next_host(events: &mut BoxStream<'static, Event>) -> HostView {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("an event came")
            .expect("the stream is open");
        if let Event::Host(view) = event {
            return view;
        }
    }
}

/// The next host view on `events` for which `done` holds: the views before it are the cover and
/// the route arriving, which come on the blocking pool's own time.
async fn next_host_where(
    events: &mut BoxStream<'static, Event>,
    done: impl Fn(&HostView) -> bool,
) -> HostView {
    loop {
        let view = next_host(events).await;
        if done(&view) {
            return view;
        }
    }
}

/// Whether `view` has a player with its cover read.
fn has_cover(view: &HostView) -> bool {
    view.playing.as_ref().is_some_and(|p| p.cover.is_some())
}

fn press(button: &str) -> Request {
    Request::HostPress {
        button: button.to_string(),
    }
}

#[tokio::test(start_paused = true)]
async fn host_request_answers_the_view() {
    let daemon = fake_host_daemon();
    let view = view_when(&daemon, "the bundled cover", |view| {
        view.playing.as_ref().is_some_and(|p| p.cover.is_some())
    })
    .await;
    assert!(view.available);
    let playing = view.playing.expect("the fake player");
    assert_eq!(playing.status, "playing");
    assert_eq!(view.route, None, "nobody watches, so no route is read");
}

#[tokio::test(start_paused = true)]
async fn host_off_answers_unavailable() {
    let daemon = daemon();
    daemon.host_to(&HostConfig::default());
    settle().await;
    assert!(!view_of(daemon.handle(Request::Host).await).available);
    let Response::Error { message, .. } = daemon.handle(press("pause")).await else {
        panic!("a press with no host is refused");
    };
    assert!(message.contains("not available"), "{message}");
}

#[tokio::test(start_paused = true)]
async fn host_press_answers_ack_or_error() {
    let daemon = fake_host_daemon();
    settle().await;

    assert_eq!(daemon.handle(press("pause")).await, Response::Ack);
    settle().await;
    let view = view_of(daemon.handle(Request::Host).await);
    assert_eq!(view.playing.expect("a player").status, "paused");
    assert_eq!(daemon.handle(press("play")).await, Response::Ack);
    assert_eq!(daemon.handle(press("Next")).await, Response::Ack);

    let Response::Error { code, message } = daemon.handle(press("rewind")).await else {
        panic!("an unknown button is refused");
    };
    assert_eq!(code, 1);
    assert!(
        message.contains("rewind") && message.contains("previous"),
        "{message}"
    );

    // No player: nothing to press.
    let quitting = daemon_over_nothing();
    settle().await;
    let Response::Error { message, .. } = quitting.handle(press("play")).await else {
        panic!("no player, no press");
    };
    assert!(message.contains("no player"), "{message}");
}

/// A daemon over a host with no player on its bus.
fn daemon_over_nothing() -> Daemon {
    let daemon = daemon();
    daemon.host().run_with(crate::host::Parts {
        connect: crate::host::found(Arc::new(svolder_host::fake::FakePlayers::new(None))),
        routes: None,
        covers: Arc::new(|_| None),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    daemon
}

/// A subscription carries the host's view after its snapshot and on each change, whatever
/// device it filters on or none; one that names no device gets its error and nothing else.
#[tokio::test(start_paused = true)]
async fn every_subscription_gets_host_events_whatever_its_device() {
    let daemon = fake_host_daemon();
    let config = Config {
        devices: vec![amp_entry("0102030405060708")],
        ..Config::default()
    };
    daemon.reconcile(&config).await;
    settle().await;

    for selector in [None, Some("amp".to_string())] {
        let mut events = daemon.subscribe_with_host(selector.clone()).await;
        let first = tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("an event came")
            .expect("the stream is open");
        assert!(
            matches!(first, Event::Connection { .. }),
            "the snapshot comes first for {selector:?}: {first:?}"
        );
        let view = next_host(&mut events).await;
        assert_eq!(
            view.playing.expect("a player").status,
            "playing",
            "{selector:?}"
        );

        assert_eq!(daemon.handle(press("pause")).await, Response::Ack);
        let view = next_host(&mut events).await;
        assert_eq!(
            view.playing.expect("a player").status,
            "paused",
            "{selector:?}"
        );
        assert_eq!(daemon.handle(press("play")).await, Response::Ack);
        assert_eq!(
            next_host(&mut events)
                .await
                .playing
                .expect("a player")
                .status,
            "playing"
        );
    }

    let mut events = daemon.subscribe_with_host(Some("nobody".to_string())).await;
    assert!(matches!(events.next().await, Some(Event::Error { .. })));
    assert!(
        events.next().await.is_none(),
        "an unresolved selector ends the stream"
    );
    daemon.shutdown().await;
}

/// A subscription holds the route reads while it lives; the in-process readers of the
/// device events never do.
#[tokio::test(start_paused = true)]
async fn only_a_subscription_holds_the_route_reads() {
    let daemon = fake_host_daemon();
    settle().await;
    let in_process = daemon.device_events(None);
    settle().await;
    assert_eq!(view_of(daemon.handle(Request::Host).await).route, None);

    let mut events = daemon.subscribe_with_host(None).await;
    // The route arrives on the stream as a later view.
    let mut view = next_host(&mut events).await;
    while view.route.is_none() {
        view = next_host(&mut events).await;
    }
    assert!(view_of(daemon.handle(Request::Host).await).route.is_some());

    drop(events);
    settle().await;
    assert_eq!(view_of(daemon.handle(Request::Host).await).route, None);
    drop(in_process);
}

/// A plain subscription gets no host events and holds no route reads, whatever else it hears.
#[tokio::test(start_paused = true)]
async fn a_plain_subscription_gets_no_host_events_and_holds_no_watcher() {
    let daemon = fake_host_daemon();
    let config = Config {
        devices: vec![amp_entry("0102030405060708")],
        ..Config::default()
    };
    daemon.reconcile(&config).await;
    settle().await;
    let mut events = daemon.subscribe(None).await;
    let first = events.next().await.expect("the snapshot");
    assert!(matches!(first, Event::Connection { .. }), "{first:?}");

    // Nothing but device events follow: no host view now, none when the player changes.
    assert_eq!(daemon.handle(press("pause")).await, Response::Ack);
    settle().await;
    let mut heard = Vec::new();
    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_secs(5), events.next()).await {
        heard.push(event);
    }
    assert!(
        !heard.iter().any(|event| matches!(event, Event::Host(_))),
        "a plain stream heard {heard:?}"
    );
    assert_eq!(
        view_of(daemon.handle(Request::Host).await).route,
        None,
        "no route is read for a plain subscription"
    );
    daemon.shutdown().await;
}

/// A reload that turns `[host]` off empties the view for the streams already open, and one
/// that turns it on again fills it again.
#[tokio::test(start_paused = true)]
async fn reload_turns_the_host_off_and_on() {
    let dir = std::env::temp_dir().join(format!("svolderd-host-reload-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("profiles")).expect("a scratch directory");
    let config = dir.join("config.toml");
    let write = |enabled: bool| {
        std::fs::write(&config, format!("[host]\nenabled = {enabled}\n")).expect("the config");
    };
    let daemon = daemon_over(config.clone(), dir.join("profiles")).with_host_mode(HostMode::Fake);

    write(true);
    daemon.reload_all().await.expect("the first reload");
    settle().await;
    let mut events = daemon.subscribe_with_host(None).await;
    assert!(next_host_where(&mut events, has_cover).await.available);

    write(false);
    daemon
        .reload_all()
        .await
        .expect("the reload that turns it off");
    let view = next_host_where(&mut events, |view| !view.available).await;
    assert_eq!(view.playing, None);
    assert!(!view_of(daemon.handle(Request::Host).await).available);
    assert!(matches!(
        daemon.handle(press("pause")).await,
        Response::Error { .. }
    ));

    write(true);
    daemon
        .reload_all()
        .await
        .expect("the reload that turns it on");
    let view = next_host_where(&mut events, has_cover).await;
    assert!(view.available);

    // A reload that changes nothing here is not heard.
    daemon.reload_all().await.expect("a quiet reload");
    settle().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), next_host(&mut events))
            .await
            .is_err(),
        "an unchanged host sends nothing"
    );
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A host-frames connection holds the capture for as long as its stream lives: the fake host
/// captures, the connection hears the capture's state and then frames, and dropping the stream
/// lets the hold go.
#[tokio::test(start_paused = true)]
async fn a_host_frames_connection_holds_the_capture_and_lets_go_when_dropped() {
    let daemon = fake_host_daemon();
    settle().await;
    assert_eq!(
        daemon.host().frame_holds(),
        0,
        "nobody holds, nothing captures"
    );
    let mut events = daemon.host_frames().await.expect("the fake host captures");
    assert_eq!(daemon.host().frame_holds(), 1);
    assert!(matches!(
        events.next().await,
        Some(Event::HostCapture(capture)) if capture.state == CaptureKind::Starting
    ));
    let (mut running, mut frames) = (false, 0);
    while frames < 3 {
        match tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("an event within a second")
            .expect("the stream goes on")
        {
            Event::HostCapture(capture) => {
                running = capture.state == CaptureKind::Running
                    && capture.sink.as_deref() == Some("Schiit Gungnir 2");
            }
            Event::HostFrame(frame) => {
                assert!(running, "frames come only while the capture runs");
                assert_eq!((frame.left.len(), frame.right.len()), (240, 240));
                frames += 1;
            }
            other => panic!("not a capture event: {other:?}"),
        }
    }
    drop(events);
    assert_eq!(
        daemon.host().frame_holds(),
        0,
        "dropping the stream lets go"
    );
}

/// A daemon whose host is off has no frames to give and says so, which a TUI reads as a daemon
/// that cannot capture and captures on its own.
#[tokio::test(start_paused = true)]
async fn host_frames_with_no_host_is_an_error() {
    let daemon = daemon();
    settle().await;
    let Err(message) = daemon.host_frames().await else {
        panic!("a host that is off gave a stream");
    };
    assert!(message.contains("not available"), "{message}");
    let Response::Error { message, .. } = daemon.handle(Request::HostFrames).await else {
        panic!("host_frames is not a request/response verb");
    };
    assert!(message.contains("host_frames"), "{message}");
}
