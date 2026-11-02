// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loop over a daemon: one that would not start, one restarted or gone under it, the wait for
//! it and what the status pane says of it.

use super::*;

/// An opener over the fake device that records the `-v` each open ran under.
fn verbose_opener(log: Arc<Mutex<Vec<bool>>>) -> impl Open {
    move |globals: OwnedGlobals| {
        let log = Arc::clone(&log);
        async move {
            log.lock().expect("log mutex").push(globals.verbose);
            let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
            Ok((backend, device_rows()))
        }
    }
}

/// `-v` names where a backend ran on the terminal the user still owns, and every open the
/// TUI makes happens with the alternate screen up — the one it starts on included, since
/// the screen is claimed before the first device is opened. So none of them runs verbose.
#[tokio::test(start_paused = true)]
async fn no_open_the_tui_makes_runs_verbose() {
    let seen: Arc<Mutex<Vec<bool>>> = Arc::default();
    let opener = Opener::new(
        globals(true),
        TuiConfig::default(),
        scratch_config(),
        verbose_opener(Arc::clone(&seen)),
    );

    let started = opener
        .reopen(Some("gungnir"), None)
        .await
        .expect("the fake device opens");
    let switched = opener
        .reopen(Some("bifrost"), Some(&started.app))
        .await
        .expect("the fake device opens");

    assert_eq!(
        *seen.lock().expect("log mutex"),
        vec![false, false],
        "an open under the alternate screen must stay quiet, and they all are"
    );
    started.close().await;
    switched.close().await;
}

/// A daemon the chain could not start is shown as a notice, like everything else this loop
/// has to say, rather than printed: the screen is claimed before the first device is
/// opened, and a line on standard error would land in the middle of a frame.
#[tokio::test(start_paused = true)]
async fn a_daemon_that_would_not_start_is_a_notice_rather_than_a_printed_line() {
    /// What the launcher hands back when a daemon it started never answered, in the words
    /// the front end that asked says it in.
    const AUTOSPAWN: &str =
        "the daemon did not answer in 2s; using a direct session — see /state/svolderd.log";

    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        scratch_config(),
        |_: OwnedGlobals| async {
            let (mut backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
            // What `Backend::open` does with a chain that ended in a direct session after
            // an autospawn it could not finish: the backend keeps the line.
            backend.warn(AUTOSPAWN.to_string());
            Ok((backend, device_rows()))
        },
    );

    let watching = opener
        .reopen(Some("gungnir"), None)
        .await
        .expect("the fake device opens");

    assert_eq!(
        watching.app.toast().map(|toast| toast.text.as_str()),
        Some(AUTOSPAWN)
    );
    watching.close().await;
}

/// The device list asks the daemon again while it shows, so the wait before a device's
/// next attempt that it draws is the daemon's own within a couple of seconds, rather than
/// whatever it was when the list came up.
///
/// Real time rather than a paused clock: the daemon is a stub on a real socket, and a
/// paused clock would jump past the bound on an ask before its answer could arrive.
#[tokio::test]
async fn the_device_list_keeps_the_daemon_s_wait_current_while_it_shows() {
    use crate::test_support::stub_daemon::{CANONICAL, OTHER, stub_daemon};

    let daemon = stub_daemon().await;
    let retry = daemon.other_retry();
    *retry.lock().expect("the wait") = Some(15);
    let mut backend = Backend::stub_remote(&daemon.socket).await;
    // As every opened session has: a daemon session with no feed would read as one whose
    // connection had closed.
    backend
        .open_feed()
        .await
        .expect("the stub daemon is subscribed to");
    let mut app = app_for(&backend);
    // What the list starts from: the file's own rows, which say nothing about the links.
    app.devices.rows = vec![DeviceRow::for_test(CANONICAL), DeviceRow::for_test(OTHER)];
    app.screen = crate::tui::app::Screen::Devices;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    // The daemon's wait moves while the list shows, as a daemon counting down would.
    let moved = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        *retry.lock().expect("the wait") = Some(5);
    });

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: quit_after(RELIST_PERIOD + Duration::from_millis(600)),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;
    moved.await.expect("the wait moved");

    assert_eq!(end.code, 0, "{:?}", end.why);
    let frames = drawn(&frames);
    let first = first_frame(&frames, "disconnected, next attempt in 15 s");
    let later = first_frame(&frames, "disconnected, next attempt in 5 s");
    assert!(
        later > first,
        "the list drew the moved wait before the one it came up with"
    );
    end.watching.close().await;
}

/// A daemon restarted under the TUI is reopened without a key: the connection closing is
/// waited out, and once a daemon answers on the socket again the device it had open is opened
/// on it and a toast says so.
#[tokio::test]
async fn a_daemon_restarted_under_the_tui_is_reopened_without_a_key() {
    use crate::test_support::stub_daemon::{stub_daemon, stub_daemon_at};

    let first = stub_daemon().await;
    first.version(env!("CARGO_PKG_VERSION"));
    let socket = first.socket.clone();
    let opener = daemon_opener(&socket, Duration::from_secs(5));
    let watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let restart = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        first.stop().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let second = stub_daemon_at(socket.clone()).await;
        second.version(env!("CARGO_PKG_VERSION"));
        second
    };
    let (adder, mut meter) = (nothing_to_add(), title());
    let (end, second) = tokio::join!(
        event_loop(LoopParts {
            opener: &opener,
            adder: &adder,
            watching,
            terminal: &mut terminal,
            events: quit_after(Duration::from_secs(3)),
            mouse: &mut mouse,
            title: &mut meter,
            look: look()
        }),
        restart
    );

    assert_eq!(end.code, 0, "{:?}", end.why);
    let frames = drawn(&frames);
    assert!(
        any_frame(&frames, "reconnected to the daemon"),
        "no toast said the device was reopened"
    );
    assert!(
        !any_frame(&frames, "device did not come back"),
        "the restart was reported as a loss"
    );
    assert!(
        !second.asked_for().is_empty(),
        "the new daemon was never asked for the device"
    );
    end.watching.close().await;
}

/// With nothing answering for the whole wait the TUI says what it always said, and `r` still
/// reopens the device once a daemon is back.
#[tokio::test]
async fn nothing_answering_for_the_wait_is_a_lost_device_and_r_still_reopens() {
    use crate::test_support::stub_daemon::{stub_daemon, stub_daemon_at};

    let first = stub_daemon().await;
    let socket = first.socket.clone();
    let opener = daemon_opener(&socket, Duration::from_millis(400));
    let watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script: BoxStream<'static, io::Result<Event>> = Box::pin(
        stream::once(async {
            tokio::time::sleep(Duration::from_millis(2500)).await;
            key('r')
        })
        .chain(quit_after(Duration::from_millis(800))),
    );
    let (adder, mut meter) = (nothing_to_add(), title());

    let restart = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        first.stop().await;
        tokio::time::sleep(Duration::from_millis(1800)).await;
        stub_daemon_at(socket.clone()).await
    };
    let (end, second) = tokio::join!(
        event_loop(LoopParts {
            opener: &opener,
            adder: &adder,
            watching,
            terminal: &mut terminal,
            events: script,
            mouse: &mut mouse,
            title: &mut meter,
            look: look()
        }),
        restart
    );

    assert_eq!(end.code, 0, "{:?}", end.why);
    let frames = drawn(&frames);
    assert!(
        any_frame(&frames, "device did not come back"),
        "the window ran out without a word"
    );
    assert!(
        !second.asked_for().is_empty(),
        "`r` did not reopen the device on the daemon that came back"
    );
    end.watching.close().await;
}

/// An opener that counts the times the launcher chain is walked and connects to a socket for
/// the other, as the TUI's own does.
struct ChainCounting(Arc<std::sync::atomic::AtomicUsize>);

impl Open for ChainCounting {
    async fn open(&self, _: OwnedGlobals, _: &Path) -> Result<Opened, Failure> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(Failure::of(Kind::Dropped, "the chain was walked"))
    }

    async fn hold(
        &self,
        _: Option<PathBuf>,
        _: OwnedGlobals,
        _: Option<String>,
        _: &Path,
    ) -> Option<Held> {
        None
    }

    async fn open_on(&self, socket: PathBuf, globals: OwnedGlobals) -> Result<Opened, Failure> {
        Backend::open_on(&socket, globals.device.as_deref()).await
    }
}

/// A daemon that answered the wait and is gone again by the reopen is a lost device: the open
/// connects to the socket and nothing else, so the launcher chain is never walked (it would
/// start a daemon or fall back to a direct session) and no backend replaces the daemon's.
#[tokio::test]
async fn a_daemon_gone_again_after_the_wait_is_a_lost_device_and_the_chain_stays_unwalked() {
    use crate::test_support::stub_daemon::stub_daemon;

    let walked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let daemon = stub_daemon().await;
    let socket = daemon.socket.clone();
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        scratch_config(),
        ChainCounting(Arc::clone(&walked)),
    );
    let watching = opener
        .reopen_via(None, None, Some(socket.clone()))
        .await
        .expect("the stub daemon answers");
    assert_eq!(walked.load(std::sync::atomic::Ordering::SeqCst), 0);
    daemon.stop().await;

    let flow = daemon_back(true, socket, &opener, watching, &look(), &mut ()).await;
    let Flow::Continue(watching) = flow else {
        panic!("the loop ended");
    };
    assert_eq!(
        walked.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "the launcher chain was walked"
    );
    assert!(
        watching.backend.is_none(),
        "a backend replaced the daemon's"
    );
    assert!(!watching.app.session.connected);
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.as_str()),
        Some("device did not come back")
    );
}

/// A wait that something supersedes stops looking at the socket.
#[tokio::test]
async fn a_superseded_wait_stops_looking() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    watching.wait_for_daemon(
        std::env::temp_dir().join("svolder-no-such.sock"),
        Duration::from_secs(30),
    );
    let probe = watching
        .reconnect
        .as_ref()
        .expect("the wait is held")
        .probe
        .abort_handle();
    watching.reconnect = None;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(probe.is_finished());
    watching.close().await;
}

/// The demo's opener connects to its socket and does nothing else: with no daemon there, an open
/// fails where the launcher chain (over the fake transport these cases run under) would have
/// opened a session of its own, and nothing is created at the socket's path.
#[tokio::test]
async fn the_demo_opener_never_leaves_its_socket() {
    let socket = scratch_config().with_file_name("demo.sock");
    let demo = Live {
        only: Some(socket.clone()),
    };
    assert!(demo.open(globals(false), &scratch_config()).await.is_err());
    assert!(demo.open_on(socket.clone(), globals(false)).await.is_err());
    assert!(!socket.exists());
}

/// The strip's last column as of the wall clock `ms`.
fn strip_end(app: &App, ms: u64) -> Option<f64> {
    crate::tui::widgets::graph::hold_wall_ms(ms);
    app.readings
        .link
        .columns(ms, Duration::from_secs(10), 10)
        .last()
        .copied()
        .flatten()
}

/// An app whose link has been up since 1 s, with the wall clock at 5 s.
async fn up_since_one_second() -> (Backend, Arc<crate::backend::FakeGate>, App) {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    crate::tui::widgets::graph::hold_wall_ms(1_000);
    app.link_changed(true);
    assert_eq!(strip_end(&app, 2_000), Some(1.0));
    crate::tui::widgets::graph::hold_wall_ms(5_000);
    (backend, gate, app)
}

/// Every path that takes the link down draws it down in the strip as well as in the header:
/// the feed's own news of it, a feed that failed or ended, a daemon that went away, one that
/// did not come back, and a device that is lost.
#[tokio::test(start_paused = true)]
async fn every_path_that_takes_the_link_down_draws_the_strip_down() {
    let (mut backend, _gate, mut app) = up_since_one_second().await;
    let now = std::time::Instant::now();
    assert!(
        heard(
            Ok(Some(Update::Connection { connected: false })),
            &mut backend,
            &mut app,
            now
        )
        .await
    );
    assert_eq!(strip_end(&app, 6_000), Some(0.0), "a connection event");
    backend.close().await;

    for (name, end) in [
        ("an ended feed", Ok(None)),
        ("a failed feed", Err(BleError::NotFound)),
    ] {
        let (mut backend, _gate, mut app) = up_since_one_second().await;
        assert!(!heard(end, &mut backend, &mut app, now).await);
        assert_eq!(strip_end(&app, 6_000), Some(0.0), "{name}");
        backend.close().await;
    }

    let (backend, _gate, app) = up_since_one_second().await;
    let mut watching = super::test_support::watching(backend, app, None);
    watching.wait_for_daemon(
        std::env::temp_dir().join("svolder-no-such.sock"),
        Duration::from_secs(30),
    );
    assert_eq!(strip_end(&watching.app, 6_000), Some(0.0), "a daemon wait");
    watching.reconnect = None;
    watching.close().await;

    let (backend, _gate, app) = up_since_one_second().await;
    let mut watching = super::test_support::watching(backend, app, None);
    watching.gone();
    assert_eq!(strip_end(&watching.app, 6_000), Some(0.0), "gone");
    watching.close().await;

    let (backend, _gate, app) = up_since_one_second().await;
    let mut watching = super::test_support::watching(backend, app, None);
    watching.lost();
    assert_eq!(strip_end(&watching.app, 6_000), Some(0.0), "lost");
    watching.close().await;
}

/// A daemon that never comes back leaves the strip down once the wait gives up.
#[tokio::test]
async fn a_daemon_that_does_not_come_back_leaves_the_strip_down() {
    let (backend, _gate, app) = up_since_one_second().await;
    let watching = super::test_support::watching(backend, app, None);
    let socket = std::env::temp_dir().join("svolder-no-such.sock");
    let opener = daemon_opener(&socket, Duration::from_millis(10));
    let flow = daemon_back(false, socket, &opener, watching, &look(), &mut ()).await;
    let Flow::Continue(watching) = flow else {
        panic!("the loop ended");
    };
    assert_eq!(strip_end(&watching.app, 9_000), Some(0.0));
    watching.close().await;
}

/// A TUI on a daemon opens knowing what the daemon says of the passphrase, takes a fresh answer
/// whenever the link comes or goes, on a refresh and after the configuration is written (the
/// daemon judges again as it takes the entry in), and carries the word, never a digit.
#[tokio::test]
async fn the_status_pane_follows_the_daemon_s_passphrase_note() {
    use crate::test_support::stub_daemon::stub_daemon;
    use svolder_ipc::PassphraseNote;

    let daemon = stub_daemon().await;
    daemon.notes(Some(PassphraseNote::Differs));
    let socket = daemon.socket.clone();
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        scratch_config(),
        move |_: OwnedGlobals| {
            let socket = socket.clone();
            async move { Ok((Backend::stub_remote(&socket).await, Vec::new())) }
        },
    );
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    assert_eq!(
        watching.app.session.passphrase_note,
        Some(PassphraseNote::Differs),
        "the open did not ask"
    );

    // The daemon judged again at the connect that followed: a link coming or going is when
    // the TUI asks.
    daemon.notes(None);
    let backend = watching.backend.as_mut().expect("a backend");
    let kept = heard(
        Ok(Some(Update::Connection { connected: false })),
        backend,
        &mut watching.app,
        Instant::now(),
    )
    .await;
    assert!(kept);
    assert_eq!(watching.app.session.passphrase_note, None);

    // A refresh asks as well.
    daemon.notes(Some(PassphraseNote::NoneConfigured));
    dispatch(Intent::Refresh, &mut watching).await;
    assert_eq!(
        watching.app.session.passphrase_note,
        Some(PassphraseNote::NoneConfigured)
    );

    // The configuration written, the daemon reloaded and judged the new entry.
    daemon.notes(None);
    assert!(written(&opener, &mut watching, Ok(())).await);
    assert_eq!(watching.app.session.passphrase_note, None);
    watching.close().await;
}

/// A recall's intent tells the daemon which preset it is: its memory holds the name afterwards.
/// Over a direct session the same intent is a no-op that never fails.
#[tokio::test]
async fn a_recalled_intent_reaches_the_daemons_memory() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let daemon = crate::test_support::stub_daemon::stub_daemon().await;
        let mut backend = Backend::stub_remote_named(&daemon.socket, None)
            .await
            .expect("the stub daemon opens");
        let (_, _gate, mut app) = fake(SHORT_DEADLINE).await;
        assert_eq!(daemon.memory(), None);
        execute(Intent::Recalled("Warm".into()), &mut backend, &mut app)
            .await
            .expect("a note never fails");
        assert_eq!(daemon.memory().as_deref(), Some("Warm"));
        let (mut direct, _gate, mut app) = fake(SHORT_DEADLINE).await;
        execute(Intent::Recalled("Warm".into()), &mut direct, &mut app)
            .await
            .expect("a direct session ignores it");
    })
    .await
    .expect("test timed out");
}
