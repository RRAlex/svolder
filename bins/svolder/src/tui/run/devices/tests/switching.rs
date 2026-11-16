// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Switching, removing, renaming and adding devices through the loop, and where each leaves it.

use super::*;

/// The host is the session's rather than the device's: a switch hands the link it is
/// reached through on to the device watched next, a read still on its way included, and
/// the app that replaces the old one keeps what the host said.
#[tokio::test(start_paused = true)]
async fn a_device_switch_keeps_the_host_and_what_it_said() {
    use svolder_host::fake::FakeSink;

    let (backend, gate, app) = fake(SHORT_DEADLINE).await;
    let opener = opener_over(fake_open(Log::default(), Arc::clone(&gate), &[]));
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.app.devices.rows = device_rows();
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    host_on(&mut watching, &sink, no_bus());
    watching.app.host.sink = Some(sink_state(0.4, true));
    watching.host.send(super::super::super::host::SinkOp::Read);

    switch_device(&opener, &mut watching, "bifrost", &look(), &mut ()).await;

    assert_eq!(
        watching.app.session.device_name, "bifrost",
        "the switch did not happen"
    );
    assert!(
        watching.app.host.linked,
        "the new app is not linked to the host"
    );
    assert_eq!(watching.app.host.sink, Some(sink_state(0.4, true)));
    assert!(
        watching.host.waiting(),
        "the read on its way did not come along with the link"
    );
    watching.close().await;
}
/// The graphs' histories belong to the device watched: a switch opens an app with none of the
/// old device's readings, or of the capture's levels heard under it.
#[tokio::test(start_paused = true)]
async fn a_device_switch_clears_the_histories() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    let id = "99.9".parse().expect("a function id");
    app.readings
        .histories
        .record(&app.profile, id, std::time::Instant::now(), 3.0);
    app.readings
        .histories
        .record_host(std::time::Instant::now(), 0.5, 0.5);
    let opener = opener_over(fake_open(Log::default(), Arc::clone(&gate), &[]));
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.app.devices.rows = device_rows();
    assert!(watching.app.readings.histories.reading(id).is_some());

    switch_device(&opener, &mut watching, "bifrost", &look(), &mut ()).await;

    assert_eq!(watching.app.session.device_name, "bifrost");
    assert!(watching.app.readings.histories.reading(id).is_none());
    assert!(watching.app.readings.histories.host()[0].is_empty());
    watching.close().await;
}

/// A device switch holds the loop while it closes one device and opens another, with
/// nothing taking the capture's blocks, so the capture is stopped before it: the opener
/// finds none running. The frame after the switch starts it again.
#[tokio::test(start_paused = true)]
async fn a_device_switch_stops_the_capture_while_it_opens_the_next_device() {
    use std::sync::Mutex;
    use svolder_host::Capture;
    use svolder_host::fake::{CaptureCall, FakeCapture, FakeSink};

    let capture = FakeCapture::new();
    let seen: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
    let (noted, held) = (Arc::clone(&seen), capture.clone());
    let opener = opener_over(move |_: OwnedGlobals| {
        let (noted, held) = (Arc::clone(&noted), held.clone());
        async move {
            noted.lock().expect("seen mutex").push(held.target());
            let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
            Ok((backend, device_rows()))
        }
    });
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.app.devices.rows = device_rows();
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    host_on(&mut watching, &sink, no_bus());
    capture_on(&mut watching, &capture);
    watching
        .app
        .host
        .fold_read(Some(sink_state(0.54, false)), true);
    watching.host.steer(&mut watching.app);
    let usb = || CaptureCall::Start("alsa_output.usb-dac".into());
    assert_eq!(capture.calls(), [usb()], "no capture ran to begin with");

    switch_device(&opener, &mut watching, "bifrost", &look(), &mut ()).await;

    assert_eq!(
        watching.app.session.device_name, "bifrost",
        "the switch did not happen"
    );
    assert_eq!(
        *seen.lock().expect("seen mutex"),
        [None],
        "the capture ran on while the next device was opened"
    );
    watching.host.steer(&mut watching.app);
    assert_eq!(capture.calls(), [usb(), CaptureCall::Stop, usb()]);
    watching.close().await;
}
/// The add-device walk draws its own frames and reads the keys itself, with nothing taking
/// the capture's blocks, so the capture is stopped before it starts: the walk finds none
/// running. The loop starts it again once it is back.
#[tokio::test(start_paused = true)]
async fn the_add_device_walk_stops_the_capture_while_it_runs() {
    use std::sync::Mutex;
    use svolder_host::Capture;
    use svolder_host::fake::{CaptureCall, FakeCapture, FakeSink};

    /// A walk that notes, as it is asked for, which sink the capture runs on.
    struct Noting {
        adds: Adds,
        capture: FakeCapture,
        seen: Arc<Mutex<Vec<Option<String>>>>,
    }
    impl Adding for Noting {
        type Effects = FakeEffects;

        async fn wizard(&self) -> Result<Wizard<FakeEffects>, Failure> {
            self.seen
                .lock()
                .expect("seen mutex")
                .push(self.capture.target());
            self.adds.wizard().await
        }
    }

    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, Some("gungnir"));
    let _ = relisted(&path, &mut watching.app);
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let capture = FakeCapture::new();
    host_on(&mut watching, &sink, no_bus());
    capture_on(&mut watching, &capture);
    let noting = Noting {
        adds: adds(path.clone()),
        capture: capture.clone(),
        seen: Arc::default(),
    };
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener,
        adder: &noting,
        watching,
        terminal: &mut terminal,
        events: pressed(vec![key('d'), key('n'), key('q'), key('q'), key('q')]),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    assert_eq!(
        *noting.seen.lock().expect("seen mutex"),
        [None],
        "the capture ran on while the walk was asked for"
    );
    let usb = || CaptureCall::Start("alsa_output.usb-dac".into());
    assert_eq!(
        capture.calls(),
        [usb(), CaptureCall::Stop, usb(), CaptureCall::Stop],
        "not stopped for the walk and started again after it"
    );
    end.watching.close().await;
}
/// Removing the device being watched takes it out of the file and opens the first one
/// still configured; a file that names it as its default has that key cleared in the same
/// write, since the writers refuse to leave a default naming nothing.
#[tokio::test(start_paused = true)]
async fn removing_the_watched_device_switches_to_what_is_left() {
    let (backend, gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir", "bifrost"]);
    Config::set_default_device(&path, Some("gungnir")).expect("the default is written");
    let log: Log = Log::default();
    let opener = opener_on(
        path.clone(),
        fake_open(Arc::clone(&log), Arc::clone(&gate), &[]),
    );
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.app.session.device_name = "gungnir".into();
    let _ = relisted(&path, &mut watching.app);

    let watching =
        carried_on(taken(Intent::RemoveDevice("gungnir".into()), &opener, watching).await);

    assert_eq!(named(&path), ["bifrost"], "the block is still in the file");
    assert_eq!(
        Config::load_from(&path).unwrap().default_device,
        None,
        "the default still names the device that went"
    );
    assert_eq!(
        opens(&log),
        vec![(Some("bifrost".to_string()), true)],
        "the loop did not open the device that is left"
    );
    assert_eq!(watching.app.screen, Screen::Device);
    watching.close().await;
}
/// A reload a Settings write still owes is carried over a switch to another device, not dropped
/// with the watching it was owed on.
#[tokio::test(start_paused = true)]
async fn a_reload_owed_by_a_settings_write_survives_a_switch_of_device() {
    let (backend, gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir", "bifrost"]);
    let log: Log = Log::default();
    let opener = opener_on(
        path.clone(),
        fake_open(Arc::clone(&log), Arc::clone(&gate), &[]),
    );
    let mut watching = watching(backend, app, Some("gungnir"));
    let _ = relisted(&path, &mut watching.app);
    super::super::super::look::reload_soon(&mut watching, &path);

    let mut mouse = SpyMouse::default();
    let flow = take_intent(
        Intent::SwitchDevice("bifrost".into()),
        &opener,
        watching,
        &mut mouse,
        &mut look(),
        &mut (),
    )
    .await;
    let watching = carried_on(flow);

    assert_eq!(opens(&log), vec![(Some("bifrost".to_string()), true)]);
    let owed = watching
        .reload_owed
        .as_ref()
        .expect("the reload is still owed");
    assert_eq!(owed.1, path);
    watching.close().await;
}
/// A run started without `-d` is on whichever device the configuration picked, and the
/// backend is what says which one that is: removing it switches to the device still
/// configured, exactly as removing a device named on the command line does.
#[tokio::test(start_paused = true)]
async fn removing_the_watched_device_switches_with_no_selector_given() {
    let (mut backend, gate, app) = fake(SHORT_DEADLINE).await;
    backend.configured_as("gungnir");
    let path = config_naming(&["gungnir", "bifrost"]);
    let log: Log = Log::default();
    let opener = opener_on(
        path.clone(),
        fake_open(Arc::clone(&log), Arc::clone(&gate), &[]),
    );
    let mut watching = watching(backend, app, None);
    let _ = relisted(&path, &mut watching.app);

    let watching =
        carried_on(taken(Intent::RemoveDevice("gungnir".into()), &opener, watching).await);

    assert_eq!(named(&path), ["bifrost"]);
    assert_eq!(
        opens(&log),
        vec![(Some("bifrost".to_string()), true)],
        "the loop did not open the device that is left"
    );
    watching.close().await;
}
/// The same run with one device configured: taking it away leaves nothing to watch, so the
/// loop walks someone through adding one.
#[tokio::test(start_paused = true)]
async fn removing_the_last_device_with_no_selector_sends_the_loop_to_the_wizard() {
    let (mut backend, _gate, app) = fake(SHORT_DEADLINE).await;
    backend.configured_as("gungnir");
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, None);
    let _ = relisted(&path, &mut watching.app);

    let flow = taken(Intent::RemoveDevice("gungnir".into()), &opener, watching).await;

    let Flow::Adding(watching) = flow else {
        panic!("the last device going did not send the loop to the wizard");
    };
    assert!(named(&path).is_empty());
    watching.close().await;
}
/// Renaming the device being watched renames what a daemon holds it under, so the session is
/// opened again by the new name and every request that follows names it. The writer carries
/// a `default_device` naming it along in the same edit, so one write and one reload take the
/// whole rename.
///
/// The header the loop starts with says `amp`, which is not the configured name and not the
/// name the daemon answers under: what makes the rename reach this session is the backend
/// being asked which configured device it is on, and nothing else here would match.
#[tokio::test]
async fn renaming_the_watched_device_reopens_it_under_its_new_name() {
    let daemon = stub_daemon().await;
    let path = config_naming(&["fake"]);
    Config::set_default_device(&path, Some("fake")).expect("the default is written");
    daemon.applies_device_ops_to(&path);
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let log: Log = Log::default();
    let opener = opener_on(
        path.clone(),
        remote_opener(Arc::clone(&log), Arc::clone(&gate), daemon.socket.clone()),
    );
    let backend = Backend::stub_remote(&daemon.socket).await;
    assert_eq!(
        backend.configured_name().as_deref(),
        Some("fake"),
        "the daemon names the device it answered for"
    );
    let app = App::new(
        backend.profile().clone(),
        "amp".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let mut watching = watching(backend, app, None);
    let _ = relisted(&path, &mut watching.app);

    let renamed = DeviceConfig {
        name: "dac".into(),
        ..device_config("fake", 0)
    };
    let mut watching = carried_on(
        taken(
            Intent::UpdateDevice("fake".into(), renamed),
            &opener,
            watching,
        )
        .await,
    );

    assert_eq!(named(&path), ["dac"], "the file was not rewritten");
    assert_eq!(
        Config::load_from(&path).unwrap().default_device.as_deref(),
        Some("dac"),
        "the default still names the device by its old name"
    );
    assert_eq!(daemon.reloads(), 1, "the rename took more than one write");
    assert_eq!(
        opens(&log),
        vec![(Some("dac".to_string()), true)],
        "the session was not opened again by the new name"
    );
    assert_eq!(
        watching.app.session.device_name, "dac",
        "the header kept the old name"
    );

    // And the session that came back takes a write, which reaches the daemon naming the
    // device the file now carries rather than the one it used to.
    dispatch(
        Intent::Set {
            key: "mute".into(),
            value: "on".into(),
            step: 1,
        },
        &mut watching,
    )
    .await;
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        None,
        "{:?}",
        watching.app.toast().map(|toast| toast.text.clone())
    );
    // The reopen asked the daemon for the device by the name the file now carries — the
    // name it answers under from then on is the daemon's own to say, and this stub keeps
    // one for the device throughout — and the write that followed reached it.
    let asked = daemon.asked_for();
    assert_eq!(
        asked.get(1).cloned(),
        Some(Some("dac".to_string())),
        "the reopen did not name the device the file now carries: {asked:?}"
    );
    assert!(asked.len() > 2, "the write never reached the daemon");
    watching.close().await;
}
/// Taking the last device away leaves nothing to watch, so the loop walks someone through
/// adding one.
#[tokio::test(start_paused = true)]
async fn removing_the_last_device_sends_the_loop_to_the_wizard() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.app.session.device_name = "gungnir".into();
    let _ = relisted(&path, &mut watching.app);

    let flow = taken(Intent::RemoveDevice("gungnir".into()), &opener, watching).await;

    let Flow::Adding(watching) = flow else {
        panic!("the last device going did not send the loop to the wizard");
    };
    assert!(named(&path).is_empty(), "the file still names a device");
    assert!(watching.app.devices.rows.is_empty());
    watching.close().await;
}
/// A walk that added a device switches to it: the list is read from the file the walk
/// wrote, and the device it named is the one the loop opens.
#[tokio::test(start_paused = true)]
async fn a_walk_that_adds_a_device_switches_to_it() {
    let (backend, gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&[]);
    let log: Log = Log::default();
    let opener = opener_on(
        path.clone(),
        fake_open(Arc::clone(&log), Arc::clone(&gate), &[]),
    );
    let mut watching = watching(backend, app, None);
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    // Enter takes the default answer to every step the walk asks, up to and including the
    // last one; the walk names the device after the product it heard.
    let mut events = pressed((0..8).map(|_| enter()).collect());

    let added = add_device(
        &adds(path.clone()),
        &opener,
        &mut watching,
        &mut terminal,
        &mut events,
        &mut look(),
    )
    .await;

    assert!(matches!(added, Added::Carried), "the walk ended the run");
    assert_eq!(named(&path), ["Bifrost 3"], "the walk wrote no device");
    assert_eq!(
        opens(&log),
        vec![(Some("Bifrost 3".to_string()), true)],
        "the loop did not open the device the walk added"
    );
    assert!(
        drawn(&frames).iter().any(|frame| frame.contains("Scan")),
        "the wizard's own steps were not drawn over the session"
    );
    watching.close().await;
}
/// `n` on the device list walks the wizard over the running session, and leaving it comes
/// back to the screen it was drawn over.
#[tokio::test(start_paused = true)]
async fn n_on_the_device_list_walks_the_wizard_and_comes_back() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, Some("gungnir"));
    let _ = relisted(&path, &mut watching.app);
    watching.app.show(Screen::Devices);
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener,
        adder: &adds(path.clone()),
        watching,
        terminal: &mut terminal,
        events: pressed(vec![key('n'), key('q'), key('q'), key('q')]),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    let frames = drawn(&frames);
    assert!(
        any_frame(&frames, "Scan"),
        "the wizard was not drawn over the session"
    );
    assert!(
        frames
            .last()
            .expect("a frame was drawn")
            .contains("Devices"),
        "the last frame is not the list the wizard was left from"
    );
    assert_eq!(named(&path), ["gungnir"], "the walk that was left wrote");
    end.watching.close().await;
}
/// A walk left with nothing configured, over a live backend, comes back to the device list:
/// there is still a device on screen. `q` and `Esc` on the scan both leave.
#[tokio::test(start_paused = true)]
async fn a_walk_left_with_nothing_configured_comes_back_while_a_device_is_on_screen() {
    for leave in [
        key('q'),
        Ok(Event::Key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Esc,
        ))),
    ] {
        let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
        let path = config_naming(&[]);
        let opener = opener_on(path.clone(), never_opens());
        let mut watching = watching(backend, app, None);
        watching.app.devices.rows.clear();
        watching.app.show(Screen::Devices);
        let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
        let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
        let mut events = pressed(vec![leave]);

        let added = add_device(
            &adds(path.clone()),
            &opener,
            &mut watching,
            &mut terminal,
            &mut events,
            &mut look(),
        )
        .await;

        assert!(matches!(added, Added::Carried), "the walk ended the run");
        assert!(named(&path).is_empty(), "the walk that was left wrote");
        assert_eq!(watching.app.screen, Screen::Devices);
        watching.close().await;
    }
}
/// A run with no backend and no device configured stays on its empty Devices screen when the
/// walk is left.
#[tokio::test(start_paused = true)]
async fn a_walk_left_with_no_backend_and_nothing_configured_stays_on_the_empty_devices_screen() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&[]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, None);
    watching.app.devices.rows.clear();
    watching.app.show(Screen::Devices);
    if let Some(backend) = watching.backend.take() {
        backend.close().await;
    }
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut events = pressed(vec![key('q')]);

    let added = add_device(
        &adds(path.clone()),
        &opener,
        &mut watching,
        &mut terminal,
        &mut events,
        &mut look(),
    )
    .await;

    assert!(
        matches!(added, Added::Carried),
        "a walk left with nothing at all ended the run"
    );
    assert_eq!(watching.app.screen, Screen::Devices);
    watching.close().await;
}
/// Through the whole loop: `n` on the device list and `Esc` on the scan draw the list again.
#[tokio::test(start_paused = true)]
async fn esc_on_the_scan_comes_back_to_the_device_list() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&[]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, None);
    watching.app.devices.rows.clear();
    watching.app.show(Screen::Devices);
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    let end = event_loop(LoopParts {
        opener: &opener,
        adder: &adds(path.clone()),
        watching,
        terminal: &mut terminal,
        events: pressed(vec![
            key('n'),
            Ok(Event::Key(crossterm::event::KeyEvent::from(
                crossterm::event::KeyCode::Esc,
            ))),
            key('q'),
            key('q'),
        ]),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    let frames = drawn(&frames);
    assert!(any_frame(&frames, "Scan"), "the walk was not drawn");
    assert!(named(&path).is_empty(), "the walk that was left wrote");
    assert!(
        frames
            .last()
            .expect("a frame was drawn")
            .contains("Devices"),
        "the last frame is not the list"
    );
    end.watching.close().await;
}
/// A name no configured device answers to is refused where it is asked for: nothing is
/// released and nothing is opened, so the device the loop is on keeps running.
#[tokio::test(start_paused = true)]
async fn switching_to_a_name_the_list_does_not_carry_is_refused() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    app.screen = Screen::Devices;
    let log: Log = Log::default();
    let opener = opener_over(fake_open(Arc::clone(&log), Arc::clone(&gate), &[]));
    let mut watching = watching(backend, app, Some("gungnir"));

    switch_device(&opener, &mut watching, "yggdrasil", &look(), &mut ()).await;

    assert_eq!(
        watching.app.toast().map(|toast| toast.text.as_str()),
        Some(no_such_device("yggdrasil").as_str())
    );
    assert!(
        opens(&log).is_empty(),
        "a name nothing answers to was opened"
    );
    assert!(!gate.closed(), "the device the loop was on was released");
    assert!(watching.backend.is_some(), "the loop kept its backend");
    assert!(watching.feed_open, "and kept reading it");
    assert_eq!(
        watching.app.screen,
        Screen::Devices,
        "the list is still there to pick from"
    );
    watching.close().await;
}
/// A switch releases the device the loop was on before it opens the next — a Forkbeard
/// device serves one central at a time — and the next frame draws what it opened.
#[tokio::test(start_paused = true)]
async fn a_switch_releases_the_old_device_first_and_draws_the_new_one() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    app.screen = Screen::Devices;
    app.replace_all(vec![absent_reading()], Instant::now());
    let log: Log = Log::default();
    let opener = opener_over(fake_open(Arc::clone(&log), Arc::clone(&gate), &[]));
    let mut watching = watching(backend, app, Some("gungnir"));

    switch_device(&opener, &mut watching, "bifrost", &look(), &mut ()).await;

    assert_eq!(
        opens(&log),
        vec![(Some("bifrost".to_string()), true)],
        "the new device was opened once, with the old one already released"
    );
    assert_eq!(watching.device.as_deref(), Some("bifrost"));
    assert_eq!(watching.app.session.device_name, "bifrost");
    assert!(watching.feed_open, "the new device's feed is open");
    assert!(
        !watching.app.readings.values.contains_key(&ABSENT),
        "the app was filled from the device that was opened"
    );
    assert_eq!(
        watching.app.screen,
        Screen::Device,
        "the list has done its job"
    );
    let frame = frame_of(&watching.app);
    assert!(
        frame.contains("Gungnir 2") && frame.contains("Controls"),
        "the frame does not draw the device that was opened:\n{frame}"
    );
    watching.close().await;
}
/// A device switch that lands on the device screen is no screen change the user asked for:
/// the trace still draws the new device's panes round, and neither the noise nor the melt
/// is started for it.
#[tokio::test(start_paused = true)]
async fn a_device_switch_plays_no_noise_or_melt() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    app.screen = Screen::Devices;
    let log: Log = Log::default();
    let opener = opener_over(fake_open(Arc::clone(&log), Arc::clone(&gate), &[]));
    let mut watching = watching(backend, app, Some("gungnir"));

    switch_device(&opener, &mut watching, "bifrost", &look(), &mut ()).await;

    assert_eq!(
        watching.app.screen,
        Screen::Device,
        "the switch went through"
    );
    assert!(
        watching.app.frame_flags.screen_changed,
        "the trace was not told the screen changed"
    );
    assert!(
        !watching.app.frame_flags.navigated,
        "the switch is taken for a screen the user moved to"
    );
    watching.close().await;
}
/// A device that will not open leaves the loop on the one it was already watching, with
/// the failure to show for it.
#[tokio::test(start_paused = true)]
async fn a_switch_that_cannot_open_falls_back_to_the_device_it_was_on() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    app.screen = Screen::Devices;
    let log: Log = Log::default();
    let opener = opener_over(fake_open(Arc::clone(&log), Arc::clone(&gate), &["bifrost"]));
    let mut watching = watching(backend, app, Some("gungnir"));

    switch_device(&opener, &mut watching, "bifrost", &look(), &mut ()).await;

    assert_eq!(
        opens(&log),
        vec![
            (Some("bifrost".to_string()), true),
            (Some("gungnir".to_string()), true),
        ],
        "the device the loop was on was not opened again"
    );
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.as_str()),
        Some(REFUSED)
    );
    assert_eq!(
        watching.app.notices.advice.as_deref(),
        Some(crate::recovery::next_step(Kind::Dropped)),
        "the step the failure named is not on the advice line"
    );
    assert_eq!(watching.device.as_deref(), Some("gungnir"));
    assert_eq!(watching.app.session.device_name, "gungnir");
    assert!(
        watching.feed_open,
        "the device it fell back to is being read"
    );
    assert_eq!(
        watching.app.screen,
        Screen::Devices,
        "the list the switch was asked for is still showing"
    );
    watching.close().await;
}
/// With neither device open to be had, the loop keeps drawing and says the link is lost —
/// and draws it lost too, so no frame after the notice has aged out claims a link this
/// process no longer holds.
#[tokio::test(start_paused = true)]
async fn a_switch_that_can_open_nothing_draws_the_link_down_and_says_so() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    app.screen = Screen::Devices;
    let log: Log = Log::default();
    let refusing = fake_open(Arc::clone(&log), Arc::clone(&gate), &["bifrost", "gungnir"]);
    let opener = opener_over(refusing);
    let mut watching = watching(backend, app, Some("gungnir"));

    switch_device(&opener, &mut watching, "bifrost", &look(), &mut ()).await;

    assert_eq!(
        opens(&log),
        vec![
            (Some("bifrost".to_string()), true),
            (Some("gungnir".to_string()), true),
        ],
        "both the device asked for and the one to fall back to were tried"
    );
    assert!(
        watching.backend.is_none(),
        "there is no device left to read"
    );
    assert!(!watching.feed_open);
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.as_str()),
        Some(LINK_LOST)
    );
    assert!(
        !watching.app.session.connected,
        "the link is drawn down, not just said"
    );
    let frame = frame_of(&watching.app);
    assert!(
        frame.contains("○ disconnected"),
        "the frame still claims the link is up:\n{frame}"
    );
    assert!(
        !watching.app.session.feed_open,
        "nothing is left to try this device again with"
    );
    assert!(
        !frame
            .chars()
            .any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)),
        "a switch with nothing left to fall back to still turned a spinner:\n{frame}"
    );

    // The toast has its six seconds and then goes; the header does not.
    watching.app.tick(Instant::now() + Duration::from_secs(7));
    assert_eq!(watching.app.toast().map(|toast| toast.text.clone()), None);
    assert!(
        frame_of(&watching.app).contains("○ disconnected"),
        "a frame after the toast aged out claims the link is up again"
    );
    watching.close().await;
}
