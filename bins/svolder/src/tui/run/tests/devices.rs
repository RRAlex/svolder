// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Devices screen as the loop lands on it: a device that will not open, a row chosen, a held
//! accent kept across a switch.

use super::*;

/// An opener over the fake device, already on the configured device called `name`.
fn opener_configured_as(name: &'static str) -> impl Open {
    move |_: OwnedGlobals| async move {
        let (mut backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
        backend.configured_as(name);
        Ok((backend, device_rows()))
    }
}

/// With no `-d`, the header takes the name the file configured this device under, not the
/// profile's own display name — the two differ here on purpose (the fake is a Gungnir 2,
/// configured as "living-room-dac") so a fallback to the profile's name would be caught.
#[tokio::test]
async fn opening_with_no_d_takes_the_configured_name_over_the_profiles_own() {
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        scratch_config(),
        opener_configured_as("living-room-dac"),
    );
    let watching = opener
        .reopen(None, None)
        .await
        .expect("the fake device opens");
    assert_eq!(watching.app.session.device_name, "living-room-dac");
    watching.close().await;
}

/// A colour of the user's own that an accent row holds is still a stop after a switch of device.
#[tokio::test]
async fn a_held_accent_survives_a_device_switch() {
    use crate::test_support::stub_daemon::stub_daemon;
    use crate::tui::app::SettingsBox;
    use crate::tui::screens::settings::{CATEGORIES, Setting};
    use crossterm::event::KeyCode;
    use svolder_profiles::Family;

    let daemon = stub_daemon().await;
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    let row = Setting::Accent(Family::Dac);
    let (category, (_, rows)) = CATEGORIES
        .iter()
        .enumerate()
        .find(|(_, (_, rows))| rows.contains(&row))
        .expect("the row is in a category");
    let row = rows
        .iter()
        .position(|r| *r == row)
        .expect("in its category");
    watching.app.overlays.settings = Some(SettingsBox { category, row });
    watching.app.tui.accents.dac = Some("#e0341c".into());
    watching.app.key(KeyEvent::from(KeyCode::Right));

    let next = opener
        .reopen_keeping(None, Some(Kept::of(&watching.app)))
        .await
        .expect("the stub daemon answers");
    assert_eq!(
        next.app.overlays.held_accents.get(Family::Dac),
        Some("#e0341c"),
        "the colour of the user's own was forgotten"
    );
    watching.close().await;
    next.close().await;
}

/// The landing that waits on a device or shows the Devices screen takes the held accents over
/// too.
#[tokio::test]
async fn a_held_accent_survives_a_landing() {
    use crate::test_support::stub_daemon::stub_daemon;
    use svolder_profiles::Family;

    let daemon = stub_daemon().await;
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    watching
        .app
        .overlays
        .held_accents
        .set(Family::Amp, Some("#e0341c".into()));
    let landed = opener
        .land_keeping(
            Landing::Devices { why: None },
            Some(Kept::of(&watching.app)),
        )
        .await;
    assert_eq!(
        landed.app.overlays.held_accents.get(Family::Amp),
        Some("#e0341c"),
        "the landing forgot the colour of the user's own"
    );
    watching.close().await;
    landed.close().await;
}

/// An open that fails any other way lands on the Devices screen: the configuration's rows with
/// the failure's own words and next step in a panel above them. `r` there opens the start
/// again, and the loop is on the device when it answers.
#[tokio::test]
async fn an_open_that_fails_lands_on_the_devices_screen_and_r_retries() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let opener = move |_: OwnedGlobals| {
        let seen = Arc::clone(&seen);
        async move {
            match seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                0 => Err(Failure::of(Kind::NothingInRange, "nothing answered")),
                _ => {
                    let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
                    Ok((backend, device_rows()))
                }
            }
        }
    };
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        opener,
    );
    let failure = opener
        .reopen(None, None)
        .await
        .err()
        .expect("nothing is in range");
    let landing = opener.landing_after(None, failure).await;
    assert!(matches!(landing, Landing::Devices { why: Some(_) }));
    let watching = land(&opener, landing).await;
    assert_eq!(watching.app.screen, Screen::Devices);
    assert!(watching.backend.is_none(), "nothing answers to be held");

    let (recorder, frames) = Recorder::new(80, 24);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let script: BoxStream<'static, io::Result<Event>> = Box::pin(
        stream::once(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            key('r')
        })
        .chain(stream::once(async {
            tokio::time::sleep(Duration::from_millis(400)).await;
            key('q')
        }))
        .chain(stream::iter(vec![key('q')])),
    );
    let end = event_loop(LoopParts {
        opener: &opener,
        adder: &nothing_to_add(),
        watching,
        terminal: &mut terminal,
        events: script,
        mouse: &mut SpyMouse::default(),
        title: &mut title(),
        look: look(),
    })
    .await;
    let frames = drawn(&frames);
    let first = &frames[0];
    assert!(first.contains("nothing answered"), "{first}");
    assert!(first.contains("fake"), "the configuration's row: {first}");
    assert!(
        first.contains("Power the device on"),
        "the failure's next step: {first}"
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert!(end.watching.app.session.holding.is_none());
    assert!(end.watching.backend.is_some());
    assert_eq!(
        end.watching.app.screen,
        Screen::Device,
        "a retried start lands on the device screen, not the list `r` was pressed on"
    );
    end.watching.close().await;
}

/// Enter on a row of the Devices landing opens that device, from a loop that holds none.
#[tokio::test]
async fn enter_on_a_row_of_the_devices_landing_switches_to_it() {
    let (_held, gate, _) = fake(SHORT_DEADLINE).await;
    let log: Log = Arc::default();
    let opener = opener_over(fake_open(Arc::clone(&log), gate, &[]));
    let watching = land(&opener, Landing::Devices { why: None }).await;
    assert_eq!(watching.app.screen, Screen::Devices);
    let mut watching = watching;
    watching.app.devices.rows = device_rows();
    let mut mouse = SpyMouse::default();
    let flow = take_intent(
        Intent::SwitchDevice("gungnir".into()),
        &opener,
        watching,
        &mut mouse,
        &mut look(),
        &mut (),
    )
    .await;
    let Flow::Continue(watching) = flow else {
        panic!("the switch carried on");
    };
    assert!(watching.app.session.holding.is_none(), "the device is open");
    assert_eq!(watching.app.screen, Screen::Device);
    assert_eq!(opens(&log).len(), 1);
    watching.close().await;
}

/// A switch to a device the daemon has not found yet lands on the waiting panel for that device,
/// on the device screen, with no toast about a failure.
#[tokio::test]
async fn a_switch_to_a_device_not_found_yet_shows_the_waiting_panel() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    daemon.unfound(true);
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        Stubbed(daemon.socket.clone()),
    );
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = vec![DeviceRow::for_test("fake")];
    app.screen = Screen::Devices;
    let mut watching = watching(backend, app, None);
    let switched = switch_device(&opener, &mut watching, "fake", &look(), &mut ()).await;
    assert_eq!(switched, Switched::Opened);
    assert!(matches!(
        watching.app.session.holding,
        Some(Holding::Waiting { .. })
    ));
    assert_eq!(watching.app.session.device_name, "fake");
    assert_eq!(watching.app.screen, Screen::Device);
    assert!(watching.app.toast().is_none(), "a wait is not a failure");
    assert!(watching.feed_open, "the daemon's events for it are read");
    watching.close().await;
}

/// A code 3 from something that is not a daemon holding the device (no daemon at all) is no wait:
/// only the daemon's own list says a device is waited for.
#[tokio::test]
async fn code_3_without_a_daemon_listing_the_device_is_not_a_wait() {
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        never_opens(),
    );
    let landing = opener
        .landing_after(None, Failure::of(Kind::NoDaemon, "no daemon"))
        .await;
    assert!(matches!(landing, Landing::Devices { why: Some(_) }));
}

/// A joined start with no daemon on the socket opens nothing and starts nothing: the failure
/// says so in the words `--join` always used, and the start lands on the Devices screen with them.
#[tokio::test]
async fn a_joined_start_with_no_daemon_lands_on_the_devices_screen_saying_so() {
    let socket =
        std::env::temp_dir().join(format!("svolder-join-none-{}.sock", std::process::id()));
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        config_with_fake(),
        Live {
            only: Some(socket.clone()),
        },
    );
    let failure = opener
        .reopen(None, None)
        .await
        .err()
        .expect("nothing answers");
    assert_eq!(failure.code, 3);
    let said = format!(
        "no daemon answers on {}; nothing was started",
        socket.display()
    );
    assert_eq!(failure.what, said);
    let landing = opener.landing_after(None, failure).await;
    let watching = land(&opener, landing).await;
    assert_eq!(watching.app.screen, Screen::Devices);
    match &watching.app.session.holding {
        Some(Holding::Nothing {
            why: Some((what, _)),
        }) => assert_eq!(what, &said),
        other => panic!("got {other:?}"),
    }
    assert!(!socket.exists(), "nothing was started");
}
