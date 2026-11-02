// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wait for a device the daemon has not found yet: the panel it shows, what ends or resumes it
//! and how the device opens in full.

use super::*;

/// A device the daemon holds and has not found yet is waited for, not given up on: the open
/// answers code 3, the daemon lists it as not connected, and the loop starts on the waiting
/// panel (and no Controls). When the daemon finds it, the loop opens it in full by itself and
/// the device screen draws its real profile's rows.
#[tokio::test]
async fn an_unfound_device_waits_and_opens_in_full_when_the_daemon_finds_it() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    daemon.unfound(true);
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        Stubbed(daemon.socket.clone()),
    );
    let failure = opener
        .reopen(None, None)
        .await
        .err()
        .expect("the daemon has not found the device");
    assert_eq!(failure.code, 3);
    let landing = opener.landing_after(None, failure).await;
    let Landing::Waiting { device, .. } = &landing else {
        panic!("the device the daemon lists as not connected is waited for");
    };
    assert_eq!(device, "fake");
    let watching = land(&opener, landing).await;
    assert!(matches!(
        watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));

    let (recorder, frames) = Recorder::new(80, 24);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let finding = async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        daemon.finds_it();
    };
    let adds = nothing_to_add();
    let mut mouse = SpyMouse::default();
    let mut title = title();
    let (end, ()) = tokio::join!(
        event_loop(LoopParts {
            opener: &opener,
            adder: &adds,
            watching,
            terminal: &mut terminal,
            events: quit_after(Duration::from_millis(1500)),
            mouse: &mut mouse,
            title: &mut title,
            look: look()
        }),
        finding
    );
    let frames = drawn(&frames);
    let waited = first_frame(&frames, "fake not found yet");
    let opened = first_frame(&frames, "Mute");
    assert!(waited < opened, "the panel comes before the controls");
    let panel = &frames[waited];
    let at = hhmm_in(panel);
    assert!(
        panel.contains(&format!("svolderd keeps looking (since {at}).")),
        "{panel}"
    );
    assert!(panel.contains("Is it on and in range?"), "{panel}");
    for frame in &frames[..opened] {
        assert!(!frame.contains("Volume"), "{frame}");
        assert!(!frame.contains("Status"), "{frame}");
    }
    assert!(any_frame(&frames[opened..], "Status"));
    assert!(end.watching.app.session.holding.is_none());
    assert_eq!(end.watching.app.session.device_name, "fake");
    assert!(end.watching.app.profile.eq.is_some(), "the real profile");
    end.watching.close().await;
}

/// The `HH:MM` the waiting panel names, read back off a frame as the text after `since `.
fn hhmm_in(frame: &str) -> String {
    let after = frame
        .split("since ")
        .nth(1)
        .expect("the panel says since when");
    after.chars().take(5).collect()
}

/// A stub-daemon opener with one device `fake` configured, and the stub not finding it yet.
async fn unfound_start() -> (
    crate::test_support::stub_daemon::StubServer,
    Opener<Stubbed>,
) {
    let daemon = crate::test_support::stub_daemon::stub_daemon().await;
    daemon.unfound(true);
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        Stubbed(daemon.socket.clone()),
    );
    (daemon, opener)
}

/// Run the loop on `watching` for `ms` milliseconds, then quit, and give back the frames.
async fn run_for<O: Open>(
    opener: &Opener<O>,
    watching: Watching,
    ms: u64,
) -> (LoopEnd, Vec<String>) {
    let (recorder, frames) = Recorder::new(80, 24);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let adds = nothing_to_add();
    let mut mouse = SpyMouse::default();
    let mut title = title();
    let end = event_loop(LoopParts {
        opener,
        adder: &adds,
        watching,
        terminal: &mut terminal,
        events: quit_after(Duration::from_millis(ms)),
        mouse: &mut mouse,
        title: &mut title,
        look: look(),
    })
    .await;
    (end, drawn(&frames))
}

/// A device found between the check that saw it waiting and the landing is not lost: the
/// landing holds what the check held, so the subscription opens on a daemon that says the link is
/// up, and the loop opens the device in full.
#[tokio::test]
async fn a_device_found_while_the_landing_is_made_still_opens_by_itself() {
    let (daemon, opener) = unfound_start().await;
    let failure = opener.reopen(None, None).await.err().expect("not found");
    let landing = opener.landing_after(None, failure).await;
    assert!(matches!(&landing, Landing::Waiting { held: Some(_), .. }));
    daemon.unfound(false);
    let watching = land(&opener, landing).await;
    assert!(matches!(
        watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));
    let (end, frames) = run_for(&opener, watching, 1000).await;
    assert!(any_frame(&frames, "Mute"), "the device opened");
    assert!(end.watching.app.session.holding.is_none());
    end.watching.close().await;
}

/// A device the daemon finds after the open that said it had not, and before the landing's check
/// (the intro plays between the two), is opened: the check sees it connected, so there is nothing
/// to wait for, and the start does not stay on the Devices screen saying the failure.
#[tokio::test]
async fn a_device_found_after_the_failed_open_is_opened_when_the_start_lands() {
    let (daemon, opener) = unfound_start().await;
    let failure = opener.reopen(None, None).await.err().expect("not found");
    assert_eq!(failure.code, 3);
    daemon.unfound(false);
    let watching = crate::tui::start::landed(&opener, None, Err(failure)).await;
    assert!(
        watching.app.session.holding.is_none(),
        "the device is open, not waited for or missing"
    );
    assert_eq!(watching.app.session.device_name, "fake");
    watching.close().await;
}

/// A device the daemon has still not found when the start lands is waited for, as it is when the
/// landing is made at once: the open said so and the daemon's list agrees.
#[tokio::test]
async fn a_device_still_not_found_when_the_start_lands_is_waited_for() {
    let (_daemon, opener) = unfound_start().await;
    let failure = opener.reopen(None, None).await.err().expect("not found");
    let watching = crate::tui::start::landed(&opener, None, Err(failure)).await;
    assert!(matches!(
        watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));
    watching.close().await;
}

/// An opener over the stub daemon that has the daemon lose its device again as the second open
/// begins, so a case can have the retry of a start meet a device that dropped since the check.
struct DropsAtTheRetry<'a> {
    daemon: &'a crate::test_support::stub_daemon::StubServer,
    over: Stubbed,
    opens: std::sync::atomic::AtomicUsize,
}

impl Open for DropsAtTheRetry<'_> {
    async fn open(&self, globals: OwnedGlobals, config: &Path) -> Result<Opened, Failure> {
        if self.opens.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            self.daemon.unfound(true);
        }
        self.over.open(globals, config).await
    }

    async fn hold(
        &self,
        socket: Option<PathBuf>,
        globals: OwnedGlobals,
        wanted: Option<String>,
        config: &Path,
    ) -> Option<Held> {
        self.over.hold(socket, globals, wanted, config).await
    }

    async fn open_on(&self, socket: PathBuf, globals: OwnedGlobals) -> Result<Opened, Failure> {
        self.over.open_on(socket, globals).await
    }
}

/// A device that is found after the failed open and gone again when the start opens it once more
/// is waited for, as any device the daemon lists as not connected: the retry's own failure is
/// checked as the first was.
#[tokio::test]
async fn a_device_gone_again_at_the_retry_is_waited_for() {
    let daemon = crate::test_support::stub_daemon::stub_daemon().await;
    daemon.unfound(true);
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        DropsAtTheRetry {
            daemon: &daemon,
            over: Stubbed(daemon.socket.clone()),
            opens: std::sync::atomic::AtomicUsize::new(0),
        },
    );
    let failure = opener.reopen(None, None).await.err().expect("not found");
    daemon.unfound(false);
    let watching = crate::tui::start::landed(&opener, None, Err(failure)).await;
    assert_eq!(
        opener.open.opens.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the start opened once more, and once only"
    );
    assert!(matches!(
        watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));
    watching.close().await;
}

/// An opener over the stub daemon that has the daemon find its device as soon as an open has
/// failed, so a case can have the check for a wait meet a device that came up since the open.
struct FindsAfterTheOpen<'a> {
    daemon: &'a crate::test_support::stub_daemon::StubServer,
    over: Stubbed,
    opens: std::sync::atomic::AtomicUsize,
}

impl Open for FindsAfterTheOpen<'_> {
    async fn open(&self, globals: OwnedGlobals, config: &Path) -> Result<Opened, Failure> {
        self.opens.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let opened = self.over.open(globals, config).await;
        if opened.is_err() {
            self.daemon.unfound(false);
        }
        opened
    }

    async fn hold(
        &self,
        socket: Option<PathBuf>,
        globals: OwnedGlobals,
        wanted: Option<String>,
        config: &Path,
    ) -> Option<Held> {
        self.over.hold(socket, globals, wanted, config).await
    }

    async fn open_on(&self, socket: PathBuf, globals: OwnedGlobals) -> Result<Opened, Failure> {
        self.over.open_on(socket, globals).await
    }
}

/// A switch whose open meets a device the daemon has not found yet, and whose check for a wait
/// meets that device connected, opens it once more, as a start does: it lands on the device,
/// not on the Devices screen under the words of a failure that has passed.
#[tokio::test]
async fn a_device_found_between_a_switchs_open_and_its_check_is_opened_once_more() {
    let daemon = crate::test_support::stub_daemon::stub_daemon().await;
    daemon.unfound(true);
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        FindsAfterTheOpen {
            daemon: &daemon,
            over: Stubbed(daemon.socket.clone()),
            opens: std::sync::atomic::AtomicUsize::new(0),
        },
    );
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = vec![DeviceRow::for_test("fake")];
    app.screen = Screen::Devices;
    let mut watching = watching(backend, app, None);
    let switched = switch_device(&opener, &mut watching, "fake", &look(), &mut ()).await;
    assert_eq!(switched, Switched::Opened);
    assert_eq!(
        opener.open.opens.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the switch opened once more, and once only"
    );
    assert!(watching.app.session.holding.is_none(), "the device is open");
    assert_eq!(watching.app.screen, Screen::Device);
    assert!(watching.app.toast().is_none(), "no failure is shown");
    watching.close().await;
}

/// With the subscription closed, the wait still ends: the relist tick takes the subscription up
/// again and sees the daemon's list say the device is connected.
#[tokio::test]
async fn a_wait_whose_feed_is_closed_ends_when_the_daemons_list_says_it_is_up() {
    let (daemon, opener) = unfound_start().await;
    let failure = opener.reopen(None, None).await.err().expect("not found");
    let landing = opener.landing_after(None, failure).await;
    let mut watching = land(&opener, landing).await;
    watching.feed_open = false;
    watching.app.session.feed_open = false;
    daemon.unfound(false);
    let (end, frames) = run_for(&opener, watching, 4000).await;
    assert!(any_frame(&frames, "Mute"), "the device opened");
    assert!(end.watching.app.session.holding.is_none());
    end.watching.close().await;
}

/// A product name picks the waited device the way the daemon does: the one device with that
/// product, whatever the case.
#[tokio::test]
async fn a_product_name_selects_the_device_that_is_waited_for() {
    let (_daemon, mut opener) = unfound_start().await;
    opener.globals.device = Some("gungnir 2".into());
    let failure = opener.reopen(None, None).await.err().expect("not found");
    let landing = opener.landing_after(None, failure).await;
    let Landing::Waiting { device, .. } = landing else {
        panic!("the product names the one device that is waited for");
    };
    assert_eq!(device, "fake");
}

/// A switch over a named socket lands holding that daemon, not the one the opener would find
/// by itself.
#[tokio::test]
async fn a_switchs_landing_holds_the_socket_the_switch_used() {
    let (daemon, _) = unfound_start().await;
    let dead = std::env::temp_dir().join("svolder-not-the-daemon.sock");
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        Stubbed(dead),
    );
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    let switched = switch_via(
        &opener,
        &mut watching,
        Some("fake"),
        Some(daemon.socket.clone()),
        &look(),
        &mut (),
    )
    .await;
    assert_eq!(switched, Switched::Opened);
    assert!(matches!(
        watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));
    let held = watching.backend.as_ref().and_then(Backend::daemon_socket);
    assert_eq!(held, Some(daemon.socket.clone()));
    watching.close().await;
}

/// What `script` does to a stub daemon while the loop waits for its device and `ms` runs out:
/// the frames and what the loop was left watching.
async fn waiting_through<F, Fut>(
    opener: &Opener<Stubbed>,
    input: BoxStream<'static, io::Result<Event>>,
    prepare: impl FnOnce(&mut Watching),
    script: F,
) -> (LoopEnd, Vec<String>)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let failure = opener.reopen(None, None).await.err().expect("not found");
    let landing = opener.landing_after(None, failure).await;
    let mut watching = land(opener, landing).await;
    assert!(matches!(
        watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));
    prepare(&mut watching);
    let (recorder, frames) = Recorder::new(80, 24);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let adds = nothing_to_add();
    let mut mouse = SpyMouse::default();
    let mut title = title();
    let (end, ()) = tokio::join!(
        event_loop(LoopParts {
            opener,
            adder: &adds,
            watching,
            terminal: &mut terminal,
            events: input,
            mouse: &mut mouse,
            title: &mut title,
            look: look()
        }),
        script()
    );
    (end, drawn(&frames))
}

/// The daemon stopping while the waiting panel shows does not end the loop: the subscription's
/// end starts the wait for a daemon, and the poll that runs past one `RELIST_PERIOD` leaves
/// the ended stream alone rather than reading it again.
#[tokio::test]
async fn the_daemon_stopping_while_the_waiting_panel_shows_keeps_the_loop_running() {
    let (daemon, opener) = unfound_start().await;
    let opener = opener.waiting_for_the_daemon(Duration::from_secs(60));
    let stopping = || async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        daemon.stop().await;
    };
    let ms = 300 + 2 * RELIST_PERIOD.as_millis() as u64 + 500;
    let (end, frames) = waiting_through(
        &opener,
        quit_after(Duration::from_millis(ms)),
        |_| {},
        stopping,
    )
    .await;
    assert_eq!(end.code, 0, "the loop ran to the user's quit");
    assert!(
        end.watching.reconnect.is_some(),
        "a wait for the daemon is pending"
    );
    assert!(matches!(
        end.watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));
    assert!(any_frame(&frames, "fake not found yet"));
    end.watching.close().await;
}

/// The daemon coming back resumes the wait on it: the new daemon has not found the device
/// either, so the panel stays and no daemon wait is left pending.
#[tokio::test]
async fn a_daemon_that_comes_back_resumes_the_wait() {
    use crate::test_support::stub_daemon::stub_daemon_at;

    let (daemon, opener) = unfound_start().await;
    let socket = daemon.socket.clone();
    let opener = opener.waiting_for_the_daemon(Duration::from_secs(60));
    let restarting = || async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        daemon.stop().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let again = stub_daemon_at(socket).await;
        again.unfound(true);
        tokio::time::sleep(Duration::from_millis(4000)).await;
        drop(again);
    };
    let (end, frames) = waiting_through(
        &opener,
        quit_after(Duration::from_millis(4000)),
        |_| {},
        restarting,
    )
    .await;
    assert_eq!(end.code, 0);
    assert!(end.watching.reconnect.is_none(), "the daemon answered");
    assert!(matches!(
        end.watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));
    assert!(end.watching.backend.is_some(), "the new daemon is held");
    assert!(any_frame(&frames, "reconnected to the daemon"));
    end.watching.close().await;
}

/// After the daemon came back, finding the device opens it in full.
#[tokio::test]
async fn the_device_found_after_a_daemon_restart_opens_in_full() {
    use crate::test_support::stub_daemon::stub_daemon_at;

    let (daemon, opener) = unfound_start().await;
    let socket = daemon.socket.clone();
    let opener = opener.waiting_for_the_daemon(Duration::from_secs(60));
    let restarting = || async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        daemon.stop().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let again = stub_daemon_at(socket).await;
        again.unfound(true);
        tokio::time::sleep(Duration::from_millis(2500)).await;
        again.finds_it();
        tokio::time::sleep(Duration::from_millis(4000)).await;
        drop(again);
    };
    let (end, frames) = waiting_through(
        &opener,
        quit_after(Duration::from_millis(6000)),
        |_| {},
        restarting,
    )
    .await;
    assert!(any_frame(&frames, "Mute"), "the device opened in full");
    assert!(end.watching.app.session.holding.is_none());
    assert!(end.watching.app.profile.eq.is_some(), "the real profile");
    end.watching.close().await;
}

/// Keys that arrive one after another, each `pause` after the one before.
fn keys_paced(pause: u64, keys: Vec<io::Result<Event>>) -> BoxStream<'static, io::Result<Event>> {
    Box::pin(stream::iter(keys).then(move |event| async move {
        tokio::time::sleep(Duration::from_millis(pause)).await;
        event
    }))
}

/// Esc as a terminal reports it.
fn escape() -> io::Result<Event> {
    Ok(Event::Key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    )))
}

/// Settings open on Connect with a half-typed broker host over a waiting panel.
fn typing_a_host(watching: &mut Watching) {
    use crate::tui::app::SettingsBox;
    use crate::tui::boxes::connections::{Field, Line, Modal};

    watching.app.overlays.settings = Some(SettingsBox {
        category: crate::tui::screens::settings::CONNECT,
        row: 0,
    });
    watching.app.overlays.conn.modal = Some(Modal::Editing(Line {
        field: Field::Host,
        text: "broker.exa".into(),
        caret: 10,
    }));
}

/// The device being found while a box is open does not close it: the half-typed host stays
/// and takes the keys typed after, `q` among them, which never reach the quit path. The
/// device opens in full on the first turn after the box has closed.
#[tokio::test]
async fn finding_the_device_waits_for_an_open_box_to_close() {
    let (daemon, opener) = unfound_start().await;
    let finding = || async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        daemon.finds_it();
        tokio::time::sleep(Duration::from_millis(3500)).await;
    };
    let input = keys_paced(
        900,
        vec![
            key('m'),
            key('q'),
            escape(),
            escape(),
            // The device screen, once the box has closed, takes the quit question.
            key('q'),
            key('q'),
        ],
    );
    let (end, frames) = waiting_through(&opener, input, typing_a_host, finding).await;
    assert_eq!(end.code, 0);
    let typed = first_frame(&frames, "broker.examq");
    assert!(
        frames[typed].contains("MQTT broker host"),
        "the edit line is still over the waiting panel"
    );
    let opened = first_frame(&frames, "Mute");
    assert!(typed < opened, "the device opens after the box closed");
    for frame in &frames[typed..opened] {
        assert!(!frame.contains("Mute"), "{frame}");
    }
    assert!(end.watching.app.session.holding.is_none());
    end.watching.close().await;
}

/// A wait that a failed switch interrupts resumes on the daemon the switch used, not on whatever
/// the opener would find by itself.
#[tokio::test]
async fn a_resumed_wait_holds_the_daemon_the_failed_switch_used() {
    let (daemon, on_daemon) = unfound_start().await;
    let failure = on_daemon.reopen(None, None).await.err().expect("not found");
    let landing = on_daemon.landing_after(None, failure).await;
    let mut watching = land(&on_daemon, landing).await;
    let since = match watching.app.session.holding {
        Some(Holding::Waiting { since }) => since,
        _ => panic!("the device is waited for"),
    };
    // The opener finds no daemon of its own: only the named socket has one.
    let dead = std::env::temp_dir().join("svolder-resumed-wait-no-daemon.sock");
    let elsewhere = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        Stubbed(dead),
    );
    let switched = switch_via(
        &elsewhere,
        &mut watching,
        Some("other"),
        Some(daemon.socket.clone()),
        &look(),
        &mut (),
    )
    .await;
    assert_eq!(switched, Switched::Stayed);
    assert!(
        matches!(watching.app.session.holding, Some(Holding::Waiting { since: again }) if again == since),
        "the wait carries on from when it began"
    );
    let held = watching.backend.as_ref().and_then(Backend::daemon_socket);
    assert_eq!(held, Some(daemon.socket.clone()));
    watching.close().await;
}

/// With no daemon left to hold, the resumed wait lands on the Devices screen saying why the
/// switch failed.
#[tokio::test]
async fn a_resumed_wait_with_no_daemon_lands_on_devices_with_the_failure() {
    let (_daemon, on_daemon) = unfound_start().await;
    let failure = on_daemon.reopen(None, None).await.err().expect("not found");
    let landing = on_daemon.landing_after(None, failure).await;
    let mut watching = land(&on_daemon, landing).await;
    let gone = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        |_: OwnedGlobals| async { Err(Failure::of(Kind::NothingInRange, "nothing answered")) },
    );
    let switched = switch_via(&gone, &mut watching, Some("other"), None, &look(), &mut ()).await;
    assert_eq!(switched, Switched::Stayed);
    assert_eq!(watching.app.screen, Screen::Devices);
    assert!(
        matches!(
            &watching.app.session.holding,
            Some(Holding::Nothing { why: Some((what, _)) }) if what.contains("nothing answered")
        ),
        "{:?}",
        watching.app.session.holding
    );
    watching.close().await;
}
