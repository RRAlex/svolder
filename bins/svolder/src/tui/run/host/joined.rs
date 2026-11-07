// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loop joined to a daemon that speaks for the host: the player, the transport and the path
//! come from the daemon's view, a press goes to it, and the loop's own watcher of the players
//! stands aside, or keeps the host when the daemon has none.
//!
//! The daemon is a stub on a real socket, so these run on the real clock: a paused one would
//! jump past the wait on an answer before the answer could arrive.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures::stream;
use ratatui::Terminal;
use svolder_host::fake::{CaptureCall, FakeCapture, FakePlayers, FakeSink};
use svolder_host::{
    Button, NoBus, PlayerMatch, Players, RouteFacts, SinkFacts, Status, StreamFacts,
};
use svolder_ipc::host::{CaptureKind, CaptureView, FrameView, HostView, PlayingView};
use tokio::sync::mpsc;

use crate::test_support::stub_daemon::{StubServer, stub_daemon};
use crate::tui::host::Host;
use crate::wizard::fake::FakeEffects;

use super::super::test_support::*;
use super::super::*;
use super::{Connect, HostLink, connected, next_change};

/// How long a case waits for the loop to show what it expects: far more than it needs.
const SHOWS_WITHIN: Duration = Duration::from_secs(20);

/// What the driver of a case sends the loop as the terminal.
type Keys = mpsc::UnboundedSender<std::io::Result<crossterm::event::Event>>;

/// A terminal tall enough for the whole Host group of a device with many rows.
const TALL: (u16, u16) = (200, 70);

/// A track as the daemon's view says it.
fn playing(title: &str, status: &str) -> PlayingView {
    PlayingView {
        title: title.into(),
        artist: "Miles Davis".into(),
        album: "Kind of Blue".into(),
        player: "mpv".into(),
        status: status.into(),
        ..PlayingView::default()
    }
}

/// A view with a player and no route.
fn view(title: &str, status: &str) -> HostView {
    HostView {
        available: true,
        playing: Some(playing(title, status)),
        route: None,
        now_ms: 1,
        volume: None,
        eq: None,
    }
}

/// A route to a Schiit sink named for the stub daemon's Gungnir 2, with `edit` applied: a
/// player on the same rate, every volume at 100 % and nothing mixing.
fn route(edit: impl FnOnce(&mut RouteFacts)) -> RouteFacts {
    let mut route = RouteFacts {
        sink: Some(SinkFacts {
            name: "alsa_output.usb-Schiit_Audio_Gungnir_2".into(),
            description: "Gungnir 2".into(),
            rate_hz: Some(48_000),
            format: Some("S32LE".into()),
            volume_milli: Some(1000),
            muted: false,
            usb_vendor_id: Some(0x30be),
            usb_product: Some("Gungnir 2".into()),
        }),
        player: Some(StreamFacts {
            rate_hz: Some(48_000),
            format: Some("S16LE".into()),
            volume_milli: Some(1000),
            muted: false,
        }),
        player_match: PlayerMatch::Pid,
        others_mixing: 0,
        host_eq: false,
    };
    edit(&mut route);
    route
}

/// A search for the players that counts how often it was called, and finds none.
fn counting(looked: &Arc<AtomicUsize>) -> Connect {
    let counted = Arc::clone(looked);
    Box::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(NoBus::Absent) })
    })
}

/// A search for the players that counts how often it was called, and finds `players`.
fn counting_finds(looked: &Arc<AtomicUsize>, players: FakePlayers) -> Connect {
    let counted = Arc::clone(looked);
    Box::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        let players = players.clone();
        Box::pin(async move { Ok(Arc::new(players) as Arc<dyn Players>) })
    })
}

/// The loop opened on the stub daemon as the TUI opens a device, `tui` as its configuration,
/// linked to a host whose sink reads and whose players `connect` finds.
async fn joined(daemon: &StubServer, tui: TuiConfig, connect: Connect) -> Watching {
    let opener = Opener::new(
        globals(false),
        tui,
        scratch_config(),
        Stubbed(daemon.socket.clone()),
    );
    joined_via(&opener, connect).await
}

/// [`joined`] over `opener`, which the loop then goes on to use for the opens a daemon that
/// comes back asks for.
async fn joined_via(opener: &Opener<Stubbed>, connect: Connect) -> Watching {
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon opens");
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    host_on(&mut watching, &sink, connect);
    watching
}

/// Wait until a frame drawn carries `needle`.
async fn shows(frames: &Frames, needle: &str) {
    let started = std::time::Instant::now();
    while !any_frame(&drawn(frames), needle) {
        assert!(
            started.elapsed() < SHOWS_WITHIN,
            "the loop never drew {needle:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Run the loop on `watching` over a terminal of `size` while `driver` plays the user and the
/// daemon: it is handed the frames and the way to press keys, and the loop quits once it is
/// done. Returns what the loop ended with and the frames it drew.
async fn drive<F, Fut>(watching: Watching, size: (u16, u16), driver: F) -> (LoopEnd, Vec<String>)
where
    F: FnOnce(Frames, Keys) -> Fut,
    Fut: Future<Output = ()>,
{
    drive_with(watching, size, &opener_over(never_opens()), driver).await
}

/// [`drive`] with `opener` for the opens the loop makes.
async fn drive_with<O, F, Fut>(
    watching: Watching,
    size: (u16, u16),
    opener: &Opener<O>,
    driver: F,
) -> (LoopEnd, Vec<String>)
where
    O: Open,
    F: FnOnce(Frames, Keys) -> Fut,
    Fut: Future<Output = ()>,
{
    let (recorder, frames) = Recorder::new(size.0, size.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let adds = Adds {
        effects: FakeEffects::healthy(),
    };
    let (keys, pressed) = mpsc::unbounded_channel();
    let script = stream::unfold(pressed, |mut pressed| async move {
        pressed.recv().await.map(|key| (key, pressed))
    });
    let mut meter = title();
    let looping = event_loop(LoopParts {
        opener,
        adder: &adds,
        watching,
        terminal: &mut terminal,
        events: Box::pin(script),
        mouse: &mut mouse,
        title: &mut meter,
        look: look(),
    });
    let driving = async {
        driver(Arc::clone(&frames), keys.clone()).await;
        // The first asks whether to quit and the second answers.
        let _ = keys.send(key('q'));
        let _ = keys.send(key('q'));
    };
    let (end, ()) = tokio::join!(looping, driving);
    assert_eq!(end.code, 0, "{:?}", end.why);
    (end, drawn(&frames))
}

/// One key press as a terminal reports it.
fn press(code: crossterm::event::KeyCode) -> std::io::Result<crossterm::event::Event> {
    Ok(crossterm::event::Event::Key(code.into()))
}

/// The keys that put the cursor on the Transport button `button` of an app with `own` rows of
/// its own, and press it.
fn press_transport(own: usize, button: Button) -> Vec<std::io::Result<crossterm::event::Event>> {
    let at = Button::ALL
        .iter()
        .position(|candidate| *candidate == button)
        .expect("a transport button");
    let mut keys: Vec<_> = (0..own + 1)
        .map(|_| press(crossterm::event::KeyCode::Down))
        .collect();
    keys.extend((0..at).map(|_| press(crossterm::event::KeyCode::Right)));
    keys.push(press(crossterm::event::KeyCode::Enter));
    keys
}

/// A joined TUI draws the track the daemon's view says is playing, follows it as the daemon
/// sends another, and never looks for players of its own.
#[tokio::test]
async fn a_joined_tui_shows_the_daemons_track() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let looked = Arc::new(AtomicUsize::new(0));
    let watching = joined(&daemon, TuiConfig::default(), counting(&looked)).await;

    let said = &daemon;
    let (end, frames) = drive(watching, TALL, |frames, _| async move {
        shows(&frames, "Now playing: So What — Miles Davis").await;
        said.publish(svolder_ipc::Event::Host(view("Blue in Green", "paused")));
        shows(&frames, "Now playing: Blue in Green — Miles Davis").await;
    })
    .await;

    assert!(any_frame(&frames, "So What — Miles Davis   mpv"));
    assert_eq!(
        looked.load(Ordering::SeqCst),
        0,
        "the TUI looked for players"
    );
    assert_eq!(
        daemon.host_asked(),
        [true],
        "the feed did not ask for the host"
    );
    let host = &end.watching.app.host;
    assert!(host.daemon_owns());
    assert_eq!(host.lit(), Some(Button::Pause));
    end.watching.close().await;
}

/// A press on the Transport row goes to the daemon as a `host_press` request, and no player
/// of the TUI's own is created or pressed.
#[tokio::test]
async fn a_joined_tui_presses_through_the_daemon() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let looked = Arc::new(AtomicUsize::new(0));
    let watching = joined(&daemon, TuiConfig::default(), counting(&looked)).await;
    let own = watching.app.rows().len();

    let asked = &daemon;
    let (end, _) = drive(watching, TALL, |frames, keys| async move {
        shows(&frames, "Now playing: So What").await;
        for pressed in press_transport(own, Button::Pause) {
            keys.send(pressed).expect("the loop reads keys");
        }
        let started = std::time::Instant::now();
        while asked.host_presses().is_empty() {
            assert!(
                started.elapsed() < SHOWS_WITHIN,
                "no press reached the daemon"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;

    assert_eq!(daemon.host_presses(), ["pause"]);
    assert_eq!(looked.load(Ordering::SeqCst), 0, "a local watcher was made");
    end.watching.close().await;
}

/// A press the daemon refuses says why, in the daemon's words.
#[tokio::test]
async fn a_press_the_daemon_refuses_says_why() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    daemon.refuses_presses("no player to press it on");
    let watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    let own = watching.app.rows().len();

    let (_, frames) = drive(watching, TALL, |frames, keys| async move {
        shows(&frames, "Now playing: So What").await;
        for pressed in press_transport(own, Button::Next) {
            keys.send(pressed).expect("the loop reads keys");
        }
        shows(&frames, "no player to press it on").await;
    })
    .await;

    assert!(any_frame(&frames, "no player to press it on"));
}

/// A daemon that has no host side, one too old to know the request and one that says it is
/// off, leaves the TUI on its own watcher: it looks for the players itself, shows what they
/// play and presses on them.
#[tokio::test]
async fn a_daemon_without_host_leaves_the_tui_on_its_own_watcher() {
    let unavailable = HostView::default();
    for (name, said) in [
        ("an old daemon", None),
        ("a daemon with no host", Some(unavailable)),
    ] {
        let daemon = stub_daemon().await;
        if let Some(said) = said {
            daemon.hosts(said);
        }
        let looked = Arc::new(AtomicUsize::new(0));
        let players = FakePlayers::new(Some(track("Local Song", Status::Playing)));
        let watching = joined(
            &daemon,
            TuiConfig::default(),
            counting_finds(&looked, players.clone()),
        )
        .await;
        let own = watching.app.rows().len();

        let (end, _) = drive(watching, TALL, |frames, keys| async move {
            shows(&frames, "Now playing: Local Song — Miles Davis").await;
            for pressed in press_transport(own, Button::Next) {
                keys.send(pressed).expect("the loop reads keys");
            }
            let started = std::time::Instant::now();
            while players.presses().is_empty() {
                assert!(started.elapsed() < SHOWS_WITHIN, "{name}: no press landed");
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await;

        assert_eq!(looked.load(Ordering::SeqCst), 1, "{name}");
        assert!(!end.watching.app.host.daemon_owns(), "{name}");
        assert_eq!(daemon.host_presses(), [] as [String; 0], "{name}");
        end.watching.close().await;
    }
}

/// `[tui] host_audio = false` wins over what the daemon says: the feed does not ask for the
/// host, nothing of the daemon's view is taken, and one that arrives anyway is dropped.
#[tokio::test]
async fn host_audio_off_in_tui_config_wins() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let tui = TuiConfig {
        host_audio: false,
        ..TuiConfig::default()
    };
    let looked = Arc::new(AtomicUsize::new(0));
    let mut watching = joined(&daemon, tui, counting(&looked)).await;
    // `joined` links the host as a session without `SVOLDER_HOST=off` is.
    assert_eq!(daemon.host_asked(), [false], "the feed asked for the host");
    assert!(!watching.app.host.daemon_owns());
    assert_eq!(watching.app.host.playing, None);

    watching.app.apply(
        Update::Host(Box::new(view("So What", "playing"))),
        Instant::now(),
    );
    assert!(!watching.app.host.daemon_owns(), "a view was taken");
    assert_eq!(watching.app.host.playing, None);

    let (end, frames) = drive(watching, TALL, |_, _| async {
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;
    assert!(!any_frame(&frames, "So What"));
    assert_eq!(looked.load(Ordering::SeqCst), 0, "players were looked for");
    end.watching.close().await;
}

/// The Path row follows the transport on a DAC: the verdict first, in the daemon's words, then
/// each hop from the player to the DAC.
#[tokio::test]
async fn the_path_row_shows_the_verdict() {
    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        route: Some(route(|route| route.host_eq = true)),
        ..view("So What", "playing")
    });
    let watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;

    let (end, frames) = drive(watching, TALL, |frames, _| async move {
        shows(&frames, "Path").await;
    })
    .await;

    let row = frames
        .iter()
        .flat_map(|frame| frame.lines())
        .find(|line| line.contains("Path "))
        .expect("a Path row was drawn");
    assert!(row.contains("host EQ on · player"), "{row}");
    assert!(!row.to_lowercase().contains("bit-perfect"));
    end.watching.close().await;
}

/// A sound that goes to another sink says so in the one sentence, with no hops after it.
#[tokio::test]
async fn a_path_to_another_sink_says_where_the_sound_goes() {
    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        route: Some(route(|route| {
            let sink = route.sink.as_mut().expect("a sink");
            sink.usb_vendor_id = None;
            sink.usb_product = None;
            sink.description = "Built-in Audio".into();
        })),
        ..view("So What", "playing")
    });
    let watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;

    let (end, frames) = drive(watching, TALL, |frames, _| async move {
        shows(&frames, "Path").await;
    })
    .await;

    let row = frames
        .iter()
        .flat_map(|frame| frame.lines())
        .find(|line| line.contains("Path "))
        .expect("a Path row was drawn");
    assert!(row.contains("the computer plays to"), "{row}");
    assert!(!row.contains('→'), "{row}");
    end.watching.close().await;
}

/// A DAC has a Path row for as long as the daemon speaks for the host, so the Host group does not
/// grow and shrink as playback starts and stops: it says `nothing playing` with no player, and
/// that the route is not read with a player and no route yet. An amp or a preamp has none, nor
/// has a session with no daemon speaking, or with host audio off.
#[tokio::test]
async fn a_dac_has_a_steady_path_row_while_the_daemon_speaks_for_the_host() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    assert!(app.host_path().is_none(), "a path with no daemon speaking");
    app.host.linked = true;
    let words = |app: &crate::tui::app::App| app.host_path().map(|line| line.words);
    let full = HostView {
        route: Some(route(|_| {})),
        ..view("So What", "playing")
    };
    app.host.fold_daemon(full.clone(), Instant::now());
    assert!(
        app.host_path().is_some_and(|line| !line.hops.is_empty()),
        "a DAC with all of it has no path"
    );

    let no_player = HostView {
        playing: None,
        ..full.clone()
    };
    app.host.fold_daemon(no_player, Instant::now());
    assert_eq!(words(&app), Some(vec!["nothing playing".to_string()]));

    let no_route = HostView {
        route: None,
        ..full.clone()
    };
    app.host.fold_daemon(no_route, Instant::now());
    assert_eq!(words(&app), Some(vec!["route not read".to_string()]));

    app.host.fold_daemon(full.clone(), Instant::now());
    app.tui.host_audio = false;
    assert!(app.host_path().is_none(), "a path with host audio off");
    app.tui.host_audio = true;
    app.host.daemon = None;
    assert!(app.host_path().is_none(), "a path once the daemon is gone");
    drop(backend);

    let (backend, _gate) = Backend::fake_amp(SHORT_DEADLINE).await;
    let mut amp = app_for(&backend);
    amp.host.linked = true;
    amp.host.fold_daemon(full, Instant::now());
    assert!(amp.host.playing.is_some(), "the amp has no track");
    assert!(amp.host_path().is_none(), "an amp has a path row");
    drop(backend);
}

/// Direct mode is as it was: no daemon speaks for the host, the TUI's own watcher is looked
/// for and followed, and there is no path.
#[tokio::test(start_paused = true)]
async fn direct_mode_is_unchanged() {
    let opener = opener_over(|_: OwnedGlobals| async {
        let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
        Ok((backend, device_rows()))
    });
    let mut watching = opener.reopen(None, None).await.expect("the fake opens");
    assert!(!watching.app.host.daemon_owns());
    let looked = Arc::new(AtomicUsize::new(0));
    let players = FakePlayers::new(Some(track("So What", Status::Playing)));
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    host_on(
        &mut watching,
        &sink,
        counting_finds(&looked, players.clone()),
    );

    let (end, frames) = drive(watching, TALL, |_, _| async {
        tokio::time::sleep(Duration::from_millis(500)).await;
    })
    .await;

    assert_eq!(looked.load(Ordering::SeqCst), 1);
    assert_eq!(
        end.watching.app.host.playing,
        Some(track("So What", Status::Playing))
    );
    assert!(any_frame(&frames, "Now playing: So What — Miles Davis"));
    assert!(!any_frame(&frames, "Path "));
    assert!(end.watching.app.host_path().is_none());
    end.watching.close().await;
}

/// The loop's own watcher is held while the daemon speaks and takes the host back, with the
/// players it kept, once the daemon stops speaking for it.
#[tokio::test(start_paused = true)]
async fn the_own_watcher_takes_the_host_back_when_the_daemon_stops_speaking() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.host.linked = true;
    let players = FakePlayers::new(Some(track("Local Song", Status::Playing)));
    let mut link = HostLink::over(None, finds(players.clone()));
    link.wake(&app);
    let found = connected(&mut link.connecting).await;
    link.found(found, &mut app);
    assert_eq!(
        app.host.now_playing().as_deref(),
        Some("Local Song — Miles Davis")
    );

    app.host
        .fold_daemon(view("So What", "playing"), Instant::now());
    link.follow(&mut app);
    players.set(Some(track("Another", Status::Playing)));
    let change = next_change(&mut link.changes).await;
    link.changed(change, &mut app);
    assert_eq!(
        app.host.now_playing().as_deref(),
        Some("So What — Miles Davis"),
        "the own watcher spoke over the daemon"
    );

    app.host.fold_daemon(HostView::default(), Instant::now());
    link.follow(&mut app);
    assert_eq!(
        app.host.now_playing().as_deref(),
        Some("Another — Miles Davis"),
        "the own watcher did not take the host back"
    );
    assert!(!app.host.daemon_owns());
    drop(backend);
}

/// A view that says the host is unavailable changes nothing before any available one, and
/// clears what the daemon said after one.
#[test]
fn an_unavailable_view_gives_the_host_back() {
    let now = Instant::now();
    let mut host = Host::new(now);
    host.found_bus(Some(track("Local Song", Status::Playing)), now);
    host.fold_daemon(HostView::default(), now);
    assert_eq!(
        host.now_playing().as_deref(),
        Some("Local Song — Miles Davis")
    );
    assert!(!host.daemon_owns());

    host.fold_daemon(view("So What", "paused"), now);
    assert!(host.daemon_owns());
    assert_eq!(host.lit(), Some(Button::Pause));
    assert_eq!(
        host.playing.as_ref().map(|p| p.identity.as_str()),
        Some("mpv")
    );

    host.fold_daemon(HostView::default(), now);
    assert!(!host.daemon_owns());
    assert!(!host.bus && host.playing.is_none(), "a stale track stayed");
}

/// An opener over the stub daemon's socket that waits `wait` for a daemon that went away.
fn stubbed(daemon: &StubServer, wait: Duration) -> Opener<Stubbed> {
    Opener::new(
        globals(false),
        TuiConfig::default(),
        scratch_config(),
        Stubbed(daemon.socket.clone()),
    )
    .waiting_for_the_daemon(wait)
}

/// Wait until `done` says so, for at most [`SHOWS_WITHIN`].
async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    while !done() {
        assert!(started.elapsed() < SHOWS_WITHIN, "never: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// A press the daemon takes (`Ack`) says nothing: no toast follows it, however long.
#[tokio::test]
async fn a_press_the_daemon_takes_raises_no_toast() {
    let daemon = stub_daemon().await;
    // A daemon of another version has a toast of its own to say.
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(view("So What", "playing"));
    let watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    let own = watching.app.rows().len();

    let asked = &daemon;
    let (end, _) = drive(watching, TALL, |frames, keys| async move {
        shows(&frames, "Now playing: So What").await;
        for pressed in press_transport(own, Button::Play) {
            keys.send(pressed).expect("the loop reads keys");
        }
        until("the press reached the daemon", || {
            !asked.host_presses().is_empty()
        })
        .await;
        // Long enough for a refusal's toast to have come back, were there one.
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;

    assert_eq!(
        end.watching.app.toast().map(|toast| toast.text.clone()),
        None
    );
    end.watching.close().await;
}

/// A press on a joined TUI whose daemon follows no player says `no player` at once and asks the
/// daemon nothing; the Path row says `nothing playing`.
#[tokio::test]
async fn a_joined_press_with_no_player_says_so_and_asks_nothing() {
    let daemon = stub_daemon().await;
    // A daemon of another version has a toast of its own to say, and its answer, asked on a
    // connection of its own at the open, could land after the press and take the toast's place.
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(HostView {
        playing: None,
        route: Some(route(|_| {})),
        ..view("", "")
    });
    let watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    let own = watching.app.rows().len();

    let (end, frames) = drive(watching, TALL, |frames, keys| async move {
        shows(&frames, "nothing playing").await;
        for pressed in press_transport(own, Button::Next) {
            keys.send(pressed).expect("the loop reads keys");
        }
        // Long enough for a request, were one sent, to have reached the daemon.
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;

    assert!(any_frame(&frames, "Path "));
    assert_eq!(daemon.host_presses(), [] as [String; 0]);
    assert_eq!(
        end.watching.app.toast().map(|toast| toast.text.clone()),
        Some("no player".to_string())
    );
    end.watching.close().await;
}

/// A feed that ends because the daemon went hands the host to the TUI's own watcher, which
/// shows its player and takes the presses, until a daemon speaks for the host again.
#[tokio::test]
async fn a_feed_that_ends_hands_the_host_to_the_own_watcher() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let looked = Arc::new(AtomicUsize::new(0));
    let players = FakePlayers::new(Some(track("Local Song", Status::Playing)));
    let opener = stubbed(&daemon, Duration::from_millis(300));
    let watching = joined_via(&opener, counting_finds(&looked, players.clone())).await;
    let own = watching.app.rows().len();

    let pressed = players.clone();
    let (end, _) = drive_with(watching, TALL, &opener, |frames, keys| async move {
        shows(&frames, "Now playing: So What").await;
        daemon.stop().await;
        shows(&frames, "Now playing: Local Song").await;
        for key in press_transport(own, Button::Next) {
            keys.send(key).expect("the loop reads keys");
        }
        until("the press landed on the own player", || {
            !pressed.presses().is_empty()
        })
        .await;
    })
    .await;

    assert_eq!(looked.load(Ordering::SeqCst), 1);
    assert!(!end.watching.app.host.daemon_owns());
    end.watching.close().await;
}

/// A feed that cannot be opened again (the daemon stopped listening, its open subscription still
/// running) when `host_audio` is switched in Settings reads as an ended feed: the TUI's own
/// watcher holds the host, shows its player and takes the presses.
#[tokio::test]
async fn a_resubscribe_that_fails_hands_the_host_to_the_own_watcher() {
    let mut daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let looked = Arc::new(AtomicUsize::new(0));
    let players = FakePlayers::new(Some(track("Local Song", Status::Playing)));
    let opener = stubbed(&daemon, Duration::from_millis(300));
    let watching = joined_via(&opener, counting_finds(&looked, players.clone())).await;
    let own = watching.app.rows().len();

    let pressed = players.clone();
    let (end, _) = drive_with(watching, TALL, &opener, |frames, keys| async move {
        shows(&frames, "Now playing: So What").await;
        daemon.stop_listening().await;
        // Host audio off in Settings: the feed is opened again without the host, which fails.
        for opened in [key('s'), key('4'), key(' ')] {
            keys.send(opened).expect("the loop reads keys");
        }
        // Host audio on again; the daemon is still gone, so the own watcher is the host.
        keys.send(key(' ')).expect("the loop reads keys");
        until("the own watcher looked for players", || {
            looked.load(Ordering::SeqCst) == 1
        })
        .await;
        // Closes the Settings box (`Esc`), so the rows and the keys that follow reach the loop.
        let esc = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        keys.send(Ok(crossterm::event::Event::Key(esc)))
            .expect("the loop reads keys");
        shows(&frames, "Now playing: Local Song").await;
        for key in press_transport(own, Button::Next) {
            keys.send(key).expect("the loop reads keys");
        }
        until("the press landed on the own player", || {
            !pressed.presses().is_empty()
        })
        .await;
    })
    .await;

    assert!(!end.watching.app.host.daemon_owns());
    end.watching.close().await;
}

/// A daemon that comes back with its host side available, on the socket the old one left, takes
/// the host over again from the TUI's own watcher.
#[tokio::test]
async fn a_feed_that_comes_back_available_takes_over_again() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let socket = daemon.socket.clone();
    let looked = Arc::new(AtomicUsize::new(0));
    let players = FakePlayers::new(Some(track("Local Song", Status::Playing)));
    let opener = stubbed(&daemon, Duration::from_secs(10));
    let watching = joined_via(&opener, counting_finds(&looked, players)).await;

    let (end, _) = drive_with(watching, TALL, &opener, |frames, _| async move {
        shows(&frames, "Now playing: So What").await;
        daemon.stop().await;
        shows(&frames, "Now playing: Local Song").await;
        let back = crate::test_support::stub_daemon::stub_daemon_at(socket).await;
        back.hosts(view("Blue in Green", "playing"));
        shows(&frames, "Now playing: Blue in Green").await;
        back.stop().await;
    })
    .await;

    assert_eq!(looked.load(Ordering::SeqCst), 1);
    end.watching.close().await;
}

/// On a live feed a view that says the host is unavailable gives the host to the TUI's own
/// watcher, and a later available one takes it back.
#[tokio::test]
async fn unavailable_then_available_on_a_live_feed_switches_the_host_back_and_forth() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let looked = Arc::new(AtomicUsize::new(0));
    let players = FakePlayers::new(Some(track("Local Song", Status::Playing)));
    let watching = joined(
        &daemon,
        TuiConfig::default(),
        counting_finds(&looked, players),
    )
    .await;

    let said = &daemon;
    let (end, _) = drive(watching, TALL, |frames, _| async move {
        shows(&frames, "Now playing: So What").await;
        said.publish(svolder_ipc::Event::Host(HostView::default()));
        shows(&frames, "Now playing: Local Song").await;
        said.publish(svolder_ipc::Event::Host(view("Blue in Green", "playing")));
        shows(&frames, "Now playing: Blue in Green").await;
    })
    .await;

    assert_eq!(looked.load(Ordering::SeqCst), 1);
    assert!(end.watching.app.host.daemon_owns());
    end.watching.close().await;
}

/// `[tui] host_audio` turned off in the Settings box opens the daemon's feed again without the
/// host, so the daemon stops reading the computer's audio graph for this TUI; turned on again it
/// opens it with the host and asks the daemon for its view afresh.
#[tokio::test]
async fn host_audio_switched_in_settings_resubscribes_the_feed() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    assert_eq!(daemon.host_snapshots(), 1);

    let asked = &daemon;
    let (end, _) = drive(watching, TALL, |frames, keys| async move {
        shows(&frames, "Now playing: So What").await;
        // The Settings box on its Host audio category, whose first row is Host audio.
        for opened in [key('s'), key('4'), key(' ')] {
            keys.send(opened).expect("the loop reads keys");
        }
        until("the feed was opened without the host", || {
            asked.host_asked() == [true, false]
        })
        .await;
        asked.hosts(view("Blue in Green", "playing"));
        keys.send(key(' ')).expect("the loop reads keys");
        until("the feed was opened with the host again", || {
            asked.host_asked() == [true, false, true]
        })
        .await;
        until("the view was asked for afresh", || {
            asked.host_snapshots() == 2
        })
        .await;
        // The daemon's answer is folded on a turn of its own, after the request reached it, and a
        // frame without the Settings box that carries the new view is what says it was.
        keys.send(press(crossterm::event::KeyCode::Esc))
            .expect("the loop reads keys");
        shows(&frames, "Now playing: Blue in Green").await;
    })
    .await;

    assert!(end.watching.app.host.daemon_owns());
    assert_eq!(
        end.watching.app.host.now_playing().as_deref(),
        Some("Blue in Green — Miles Davis"),
        "the view from before was shown"
    );
    end.watching.close().await;
}

/// A daemon that does not answer the snapshot and the view asked for when `[tui] host_audio` is
/// switched in Settings holds up nothing: the loop goes on drawing and reading keys while the
/// requests are out, and the view from before is not shown meanwhile.
#[tokio::test]
async fn a_daemon_mute_to_the_host_refresh_stalls_nothing() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;

    let asked = &daemon;
    let (end, _) = drive(watching, TALL, |frames, keys| async move {
        shows(&frames, "Now playing: So What").await;
        asked.mutes_snapshots();
        for opened in [key('s'), key('4'), key(' ')] {
            keys.send(opened).expect("the loop reads keys");
        }
        until("the feed was opened without the host", || {
            asked.host_asked() == [true, false]
        })
        .await;
        // The box closes, and a frame without it is drawn while the daemon is still mute: the
        // requests the refresh made take a second or more to give up.
        let before = drawn(&frames).len();
        keys.send(press(crossterm::event::KeyCode::Esc))
            .expect("the loop reads keys");
        let started = std::time::Instant::now();
        while drawn(&frames).len() == before
            || drawn(&frames).last().is_some_and(|frame| boxed(frame))
        {
            assert!(
                started.elapsed() < Duration::from_millis(500),
                "the loop stalled on the daemon"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    end.watching.close().await;
}

/// Whether a frame has the Settings box on it.
fn boxed(frame: &str) -> bool {
    frame.contains("┌ Settings")
}

/// The waiting landing asks the daemon for its host view too, so no search for players of the
/// TUI's own starts there while the daemon has a host.
#[tokio::test]
async fn the_waiting_landing_takes_the_daemons_view_and_looks_for_no_players() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    daemon.unfound(true);
    let opener = stubbed(&daemon, Duration::from_secs(1));
    let landing = Landing::Waiting {
        device: crate::test_support::stub_daemon::CANONICAL.to_string(),
        since: SystemTime::now(),
        held: None,
    };
    let mut watching = opener.land_keeping(landing, None).await;
    assert!(watching.app.session.holding.is_some(), "not waiting");
    let looked = Arc::new(AtomicUsize::new(0));
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    host_on(&mut watching, &sink, counting(&looked));

    assert!(watching.app.host.daemon_owns(), "no view was asked for");
    watching.host.follow(&mut watching.app);
    watching.host.wake(&watching.app);
    assert_eq!(looked.load(Ordering::SeqCst), 0, "players were looked for");
    watching.close().await;
}

/// The loop's futures carry `Update`, `Host`, `Kept` and `Watching` by value, and the tests run
/// the whole loop on a 2 MB thread stack: the daemon's host view (the route above all) is far too
/// large to sit in them, which once overflowed that stack on an unrelated test. A type that grows
/// past its bound here wants its large part boxed, not the bound raised.
#[test]
fn what_the_loop_carries_by_value_stays_small() {
    use std::mem::size_of;
    for (name, size, bound) in [
        ("Update", size_of::<Update>(), 256),
        ("Host", size_of::<Host>(), 512),
        ("FromDaemon", size_of::<crate::tui::host::FromDaemon>(), 16),
        ("Kept", size_of::<Kept>(), 1400),
        ("Watching", size_of::<Watching>(), 600),
    ] {
        assert!(
            size <= bound,
            "{name} is {size} bytes, over {bound}: box its large part, or the loop's futures \
             overflow the test thread's stack"
        );
    }
}

/// The TUI's own watcher, already following a player when a daemon first speaks for the host, is
/// held while the daemon speaks and takes the host back, with the player it kept, when the daemon
/// says the host is unavailable.
#[tokio::test]
async fn a_daemon_that_speaks_late_and_stops_gives_the_held_players_back() {
    let daemon = stub_daemon().await;
    let looked = Arc::new(AtomicUsize::new(0));
    let players = FakePlayers::new(Some(track("Local Song", Status::Playing)));
    let watching = joined(
        &daemon,
        TuiConfig::default(),
        counting_finds(&looked, players),
    )
    .await;

    let said = &daemon;
    let (end, _) = drive(watching, TALL, |frames, _| async move {
        shows(&frames, "Now playing: Local Song").await;
        said.publish(svolder_ipc::Event::Host(view("So What", "playing")));
        until("the daemon's track is the last drawn", || {
            drawn(&frames)
                .last()
                .is_some_and(|frame| frame.contains("Now playing: So What"))
        })
        .await;
        said.publish(svolder_ipc::Event::Host(HostView::default()));
        until("the held player is the last drawn", || {
            drawn(&frames)
                .last()
                .is_some_and(|frame| frame.contains("Now playing: Local Song"))
        })
        .await;
    })
    .await;

    assert_eq!(looked.load(Ordering::SeqCst), 1, "searched more than once");
    end.watching.close().await;
}

/// A device opened in place of another takes `[tui] host_audio` as the session holds it now, not
/// as the opener's table read when the TUI started: one switched off in Settings is not asked
/// for again by the next open, and none of the daemon's view is taken.
#[tokio::test]
async fn a_reopen_follows_the_sessions_host_audio_not_the_openers() {
    let daemon = stub_daemon().await;
    daemon.hosts(view("So What", "playing"));
    let opener = stubbed(&daemon, Duration::from_secs(1));
    let mut first = joined_via(&opener, counting(&Arc::default())).await;
    assert_eq!(daemon.host_asked(), [true]);
    first.app.tui.host_audio = false;

    let next = opener
        .reopen(None, Some(&first.app))
        .await
        .expect("the stub daemon opens");

    assert_eq!(daemon.host_asked(), [true, false]);
    assert!(!next.app.host.daemon_owns());
    first.close().await;
    next.close().await;
}

/// What the daemon's capture says when it runs on the sink the stub reads.
fn running() -> CaptureView {
    CaptureView {
        state: CaptureKind::Running,
        sink: Some("USB DAC".into()),
        why: None,
    }
}

/// A frame of the daemon with every band at the top of the scale, flagged a beat or not.
fn loud(beat: bool) -> FrameView {
    FrameView {
        t: 1,
        left: vec![255; svolder_host::analysis::BANDS],
        right: vec![255; svolder_host::analysis::BANDS],
        levels: [255; 4],
        beat,
        bpm: None,
        phase: 0,
    }
}

/// The Spectrum row of a full block of the loop's own capture.
const FULL_ROW: &str = "Spectrum     ████████████████";

/// The capture of the sink `sink_state` names.
fn usb() -> CaptureCall {
    CaptureCall::Start("alsa_output.usb-dac".into())
}

/// A joined TUI with a daemon that gives frames draws its Spectrum row from them, a beat in
/// one pulses it, and the loop's own capture stops at the first: the host is recorded once.
#[tokio::test]
async fn a_joined_tui_draws_the_daemons_frames() {
    let daemon = stub_daemon().await;
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(view("So What", "playing"));
    daemon.frames(running());
    let capture = FakeCapture::new();
    let mut watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    capture_on(&mut watching, &capture);

    let sent = &daemon;
    let (end, frames) = drive(watching, TALL, |frames, _keys| async move {
        until("the frames connection opened", || sent.frames_open() == 1).await;
        sent.sends_frame(loud(true));
        shows(&frames, FULL_ROW).await;
    })
    .await;

    assert!(any_frame(&frames, FULL_ROW));
    assert_eq!(
        capture.calls().last(),
        Some(&CaptureCall::Stop),
        "the loop kept recording the host itself"
    );
    assert_eq!(daemon.frames_asked(), 1);
    end.watching.close().await;
    until("the connection was let go with the loop", || {
        daemon.frames_open() == 0
    })
    .await;
}

/// A daemon from before the frames (or with no host audio) refuses the request: the TUI is
/// left on its own capture, asks once and does not keep asking.
#[tokio::test]
async fn an_old_daemon_leaves_the_tui_on_its_own_capture() {
    let daemon = stub_daemon().await;
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(view("So What", "playing"));
    let capture = FakeCapture::new();
    let mut watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    capture_on(&mut watching, &capture);

    let held = capture.clone();
    let (end, _) = drive(watching, TALL, |frames, _keys| async move {
        until("the own capture started", || held.calls() == [usb()]).await;
        held.push(block(1.0, 1.0));
        shows(&frames, FULL_ROW).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;

    assert_eq!(daemon.frames_asked(), 1, "the refusal was asked again");
    assert_eq!(daemon.frames_open(), 0);
    end.watching.close().await;
}

/// A daemon whose capture cannot run says so: the connection is let go and the TUI's own
/// capture takes over.
#[tokio::test]
async fn a_daemon_that_cannot_capture_leaves_the_tui_on_its_own() {
    let daemon = stub_daemon().await;
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(view("So What", "playing"));
    daemon.frames(CaptureView {
        state: CaptureKind::Unavailable,
        sink: None,
        why: Some("this host has no capture tool".into()),
    });
    let capture = FakeCapture::new();
    let mut watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    capture_on(&mut watching, &capture);

    let held = capture.clone();
    let sent = &daemon;
    let (end, _) = drive(watching, TALL, |frames, _keys| async move {
        until("the own capture started", || held.calls() == [usb()]).await;
        held.push(block(1.0, 1.0));
        shows(&frames, FULL_ROW).await;
        assert_eq!(sent.frames_open(), 0, "the connection was kept");
    })
    .await;

    end.watching.close().await;
}

/// The frames connection is open only while a row wants a capture: with nothing but the
/// Spectrum row asking, leaving the device screen lets it go and coming back opens it again.
#[tokio::test]
async fn the_frames_connection_closes_when_no_row_wants_it() {
    let daemon = stub_daemon().await;
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(view("So What", "playing"));
    daemon.frames(running());
    let tui = TuiConfig {
        title_meter: false,
        ..TuiConfig::default()
    };
    let mut watching = joined(&daemon, tui, counting(&Arc::default())).await;
    let capture = FakeCapture::new();
    capture_on(&mut watching, &capture);

    let sent = &daemon;
    let (end, _) = drive(watching, TALL, |_frames, keys| async move {
        until("the frames connection opened", || sent.frames_open() == 1).await;
        keys.send(key('t')).expect("the loop reads keys");
        until("the connection was let go off the device screen", || {
            sent.frames_open() == 0
        })
        .await;
        keys.send(press(crossterm::event::KeyCode::Esc))
            .expect("the loop reads keys");
        until("the connection came back with the row", || {
            sent.frames_open() == 1
        })
        .await;
    })
    .await;

    assert_eq!(daemon.frames_asked(), 2);
    end.watching.close().await;
}

/// A daemon whose capture stops while it streams (its capture tool went) tells the TUI so: the
/// connection is let go and the TUI's own capture takes the row over.
#[tokio::test]
async fn a_daemon_capture_that_stops_hands_the_row_to_the_own() {
    let daemon = stub_daemon().await;
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(view("So What", "playing"));
    daemon.frames(running());
    let capture = FakeCapture::new();
    let mut watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    capture_on(&mut watching, &capture);

    let held = capture.clone();
    let sent = &daemon;
    let (end, _) = drive(watching, TALL, |frames, _keys| async move {
        until("the frames connection opened", || sent.frames_open() == 1).await;
        sent.sends_frame(loud(false));
        shows(&frames, FULL_ROW).await;
        until("the own capture let go", || {
            held.calls().last() == Some(&CaptureCall::Stop)
        })
        .await;
        sent.sends_capture(CaptureView {
            state: CaptureKind::Unavailable,
            sink: None,
            why: Some("the capture ended".into()),
        });
        until("the own capture took the row over", || {
            held.calls() == [usb(), CaptureCall::Stop, usb()]
        })
        .await;
        until("the connection was let go", || sent.frames_open() == 0).await;
    })
    .await;

    end.watching.close().await;
}

/// A daemon whose connection is open and has sent no frame yet leaves the Spectrum row on the
/// loop's own capture: the row has data, the own capture is not stopped, and only the daemon's
/// first frame stops it, so the two never both feed the row.
#[tokio::test]
async fn the_own_capture_feeds_the_row_until_the_daemons_first_frame() {
    let daemon = stub_daemon().await;
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(view("So What", "playing"));
    daemon.frames(running());
    let capture = FakeCapture::new();
    let mut watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    capture_on(&mut watching, &capture);

    let held = capture.clone();
    let sent = &daemon;
    let (end, _) = drive(watching, TALL, |frames, _keys| async move {
        until("the frames connection opened", || sent.frames_open() == 1).await;
        until("the own capture started", || held.calls() == [usb()]).await;
        held.push(block(1.0, 1.0));
        shows(&frames, FULL_ROW).await;
        assert_eq!(
            held.calls(),
            [usb()],
            "the own capture stopped before a frame"
        );
        sent.sends_frame(loud(false));
        until("the own capture let go at the first frame", || {
            held.calls() == [usb(), CaptureCall::Stop]
        })
        .await;
    })
    .await;

    end.watching.close().await;
}

/// A daemon that accepts the request and never answers costs the own capture only the short
/// wait for the answer, once: it is not asked again for half a minute, so the own capture does
/// not flap in and out.
#[tokio::test]
async fn a_daemon_that_never_answers_leaves_the_own_capture_running() {
    let daemon = stub_daemon().await;
    daemon.version(env!("CARGO_PKG_VERSION"));
    daemon.hosts(view("So What", "playing"));
    daemon.frames(running());
    daemon.mutes_frames();
    let capture = FakeCapture::new();
    let mut watching = joined(&daemon, TuiConfig::default(), counting(&Arc::default())).await;
    capture_on(&mut watching, &capture);

    let held = capture.clone();
    let asked = &daemon;
    let (end, _) = drive(watching, TALL, |frames, _keys| async move {
        until("the own capture started", || held.calls() == [usb()]).await;
        held.push(block(1.0, 1.0));
        shows(&frames, FULL_ROW).await;
        // The own capture keeps running through the open's wait (3 s, past this window; the step
        // that ends it is `a_connection_still_opening_leaves_the_own_capture_alone`), and a retry
        // would have stopped and started it again.
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        assert_eq!(held.calls(), [usb()], "the own capture flapped");
        assert_eq!(asked.frames_asked(), 1, "the silent daemon was asked again");
    })
    .await;

    end.watching.close().await;
}

/// `[tui] host_audio = false` and `SVOLDER_HOST=off` keep the frames connection shut, whatever
/// the daemon would give: the same session that opens it otherwise never asks.
#[tokio::test]
async fn the_off_switches_never_open_the_frames_connection() {
    for off in ["host_audio", "SVOLDER_HOST"] {
        let daemon = stub_daemon().await;
        daemon.version(env!("CARGO_PKG_VERSION"));
        daemon.hosts(view("So What", "playing"));
        daemon.frames(running());
        let tui = TuiConfig {
            host_audio: off != "host_audio",
            ..TuiConfig::default()
        };
        let mut watching = joined(&daemon, tui, counting(&Arc::default())).await;
        if off == "SVOLDER_HOST" {
            // what `reach_host` leaves under the variable
            watching.app.host.held_off = true;
            watching.app.host.linked = false;
        }

        let (end, _) = drive(watching, TALL, |_, _| async {
            tokio::time::sleep(Duration::from_millis(600)).await;
        })
        .await;

        assert_eq!(
            daemon.frames_asked(),
            0,
            "{off} did not hold the frames off"
        );
        end.watching.close().await;
    }
}
