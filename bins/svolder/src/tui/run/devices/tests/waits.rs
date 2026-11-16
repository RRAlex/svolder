// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wait a switch draws, its spinner, and the keys that give up or are dropped during it.

use super::*;

/// What a frame drawn during a switch showed, and how far the switch had got.
#[derive(Debug, Clone, PartialEq)]
struct Shot {
    notice: Option<String>,
    released: bool,
    opens: usize,
}

/// A device switch draws a frame saying which device it is opening before it awaits the close,
/// the scan or the connect.
#[tokio::test(start_paused = true)]
async fn a_switch_says_what_it_is_opening_before_it_closes_anything() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    app.screen = Screen::Devices;
    let log: Log = Log::default();
    let opener = opener_over(fake_open(Arc::clone(&log), Arc::clone(&gate), &[]));
    let mut watching = watching(backend, app, Some("gungnir"));
    let mut shots: Vec<Shot> = Vec::new();
    let seen = Arc::clone(&log);
    let held = Arc::clone(&gate);

    switch_device(
        &opener,
        &mut watching,
        "bifrost",
        &look(),
        &mut |app: &mut App, _: &Look| {
            shots.push(Shot {
                notice: app.toast().map(|toast| toast.text.clone()),
                released: held.closed(),
                opens: opens(&seen).len(),
            });
        },
    )
    .await;

    let first = shots.first().expect("the switch drew no frame at all");
    assert!(
        first
            .notice
            .as_deref()
            .is_some_and(|n| n.ends_with(" opening bifrost…")),
        "the first frame did not say what is being opened: {first:?}"
    );
    assert!(
        !first.released && first.opens == 0,
        "the frame came after the close or the open began: {first:?}"
    );
    assert_eq!(watching.device.as_deref(), Some("bifrost"));
    watching.close().await;
}

/// The spinner keeps turning, one step at a time, for as long as the open takes.
#[tokio::test(start_paused = true)]
async fn the_spinner_turns_at_its_own_step_while_a_switch_waits() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    app.screen = Screen::Devices;
    let held = Arc::clone(&gate);
    let slow = move |globals: OwnedGlobals| {
        let held = Arc::clone(&held);
        async move {
            let _ = (globals, held);
            tokio::time::sleep(Duration::from_millis(350)).await;
            let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
            Ok((backend, device_rows()))
        }
    };
    let opener = opener_over(slow);
    let mut watching = watching(backend, app, Some("gungnir"));
    let mut glyphs: Vec<char> = Vec::new();
    let start = tokio::time::Instant::now();
    let mut stamps: Vec<Duration> = Vec::new();

    switch_device(
        &opener,
        &mut watching,
        "bifrost",
        &look(),
        &mut |app: &mut App, _: &Look| {
            let notice = app
                .toast()
                .map(|toast| toast.text.clone())
                .expect("a notice while waiting");
            glyphs.push(notice.chars().next().expect("a glyph"));
            stamps.push(start.elapsed());
        },
    )
    .await;

    assert_eq!(&glyphs[..4], &['⠋', '⠙', '⠹', '⠸'], "{glyphs:?}");
    for (at, stamp) in stamps.iter().enumerate().take(4) {
        assert_eq!(
            *stamp,
            spinner::STEP * at as u32,
            "frame {at} was not on the step"
        );
    }
    watching.close().await;
}

/// Drawn on a terminal, the switch's frame carries the notice in the footer.
#[tokio::test(start_paused = true)]
async fn the_switch_frame_on_a_terminal_shows_the_opening_notice() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    app.screen = Screen::Devices;
    let opener = opener_over(fake_open(Log::default(), Arc::clone(&gate), &[]));
    let mut watching = watching(backend, app, Some("gungnir"));
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut events = stream::pending::<io::Result<Event>>();
    let mut screen = OnScreen {
        terminal: &mut terminal,
        hits: &mut hits,
        events: &mut events,
    };

    switch_device(&opener, &mut watching, "bifrost", &look(), &mut screen).await;

    let first = frames
        .lock()
        .expect("frames mutex")
        .first()
        .cloned()
        .expect("the switch drew nothing");
    assert!(
        first.contains("opening bifrost…"),
        "the frame does not say what is opening:\n{first}"
    );
    watching.close().await;
}

/// A screen that draws nothing and reads the events a case scripts, each after its own delay.
struct Scripted {
    events: BoxStream<'static, io::Result<Event>>,
}

impl Shown for Scripted {
    fn frame(&mut self, _app: &mut App, _look: &Look) {}

    fn next_event(&mut self) -> futures::future::LocalBoxFuture<'_, Option<io::Result<Event>>> {
        Box::pin(self.events.next())
    }
}

/// A screen reading `script`: each event arrives its delay after the one before it.
fn scripted(script: Vec<(Duration, io::Result<Event>)>) -> Scripted {
    Scripted {
        events: Box::pin(stream::iter(script).then(|(after, event)| async move {
            tokio::time::sleep(after).await;
            event
        })),
    }
}

/// Ctrl-C as a terminal in raw mode reports it.
fn ctrl_c() -> io::Result<Event> {
    Ok(Event::Key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('c'),
        crossterm::event::KeyModifiers::CONTROL,
    )))
}

/// An opener whose device takes `how_long` to answer, and which records each open.
fn slow_opener(log: Log, how_long: Duration) -> impl Open {
    move |globals: OwnedGlobals| {
        let log = Arc::clone(&log);
        async move {
            log.lock().expect("log mutex").push((globals.device, false));
            tokio::time::sleep(how_long).await;
            let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
            Ok((backend, device_rows()))
        }
    }
}

/// Switch to bifrost through the loop's own intent, reading `screen`, and say what came of it.
async fn switched_reading<O: Open>(
    opener: &Opener<O>,
    watching: Watching,
    screen: &mut Scripted,
) -> Flow {
    let mut mouse = SpyMouse::default();
    take_intent(
        Intent::SwitchDevice("bifrost".into()),
        opener,
        watching,
        &mut mouse,
        &mut look(),
        screen,
    )
    .await
}

/// What a give-up leaves: the loop ended cleanly, holding no device.
fn gave_up(flow: Flow) -> Watching {
    match flow {
        Flow::Ended(end) => {
            assert_eq!(end.code, 0, "{:?}", end.why);
            assert!(end.why.is_none(), "{:?}", end.why);
            end.watching
        }
        Flow::Continue(_) | Flow::Adding(_) => panic!("the switch went on after the give-up key"),
    }
}

/// Ctrl-C or `q` while the new device is being opened stops the open at once and ends the TUI.
#[tokio::test(start_paused = true)]
async fn a_give_up_key_during_the_open_of_a_switch_ends_the_tui_at_once() {
    for pressed in [key('q'), ctrl_c()] {
        let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
        app.devices.rows = device_rows();
        let log: Log = Log::default();
        let opener = opener_over(slow_opener(Arc::clone(&log), Duration::from_secs(30)));
        let watching = watching(backend, app, Some("gungnir"));
        let mut screen = scripted(vec![(Duration::from_secs(1), pressed)]);
        let start = tokio::time::Instant::now();

        let flow = switched_reading(&opener, watching, &mut screen).await;

        assert!(
            start.elapsed() < Duration::from_secs(2),
            "the switch went on for {:?} after the key",
            start.elapsed()
        );
        let watching = gave_up(flow);
        assert!(watching.backend.is_none(), "a device was still held");
        assert_eq!(opens(&log).len(), 1, "no second open was tried");
    }
}

/// A slow close, as a reconnect attempt still deep in its connect makes one.
async fn watching_with_a_slow_close() -> (Watching, Arc<FakeGate>) {
    let (mut backend, gate) = Backend::fake(Duration::from_secs(30)).await;
    let mut app = app_for(&backend);
    let values = backend.snapshot().await.expect("the fake device answers");
    app.replace_all(values, std::time::Instant::now());
    app.devices.rows = device_rows();
    gate.slow_connects(Duration::from_secs(10));
    gate.refuse_connects();
    gate.sever();
    backend.next_status().await.expect("an update");
    (watching(backend, app, Some("gungnir")), gate)
}

/// Ctrl-C or `q` while the old device is being closed is remembered: the close runs to its
/// end, the open is skipped, and the TUI then ends.
#[tokio::test(start_paused = true)]
async fn a_give_up_key_during_the_close_of_a_switch_lets_the_close_finish_and_skips_the_open() {
    for pressed in [key('q'), ctrl_c()] {
        let (watching, gate) = watching_with_a_slow_close().await;
        let log: Log = Log::default();
        let opener = opener_over(slow_opener(Arc::clone(&log), Duration::from_secs(1)));
        let mut screen = scripted(vec![(Duration::from_secs(1), pressed)]);
        let start = tokio::time::Instant::now();

        let flow = switched_reading(&opener, watching, &mut screen).await;

        assert!(
            start.elapsed() >= Duration::from_secs(8),
            "the close was cut short after {:?}",
            start.elapsed()
        );
        assert_eq!(
            gate.settles(),
            1,
            "the close did not run through its stopping of the attempt"
        );
        assert!(opens(&log).is_empty(), "a device was opened after the key");
        let watching = gave_up(flow);
        assert!(watching.backend.is_none(), "a device was still held");
    }
}

/// Every other key, and the events that are not keys, are read and dropped in both halves.
#[tokio::test(start_paused = true)]
async fn other_events_during_a_switch_are_dropped() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    let log: Log = Log::default();
    let opener = opener_over(slow_opener(Arc::clone(&log), Duration::from_secs(2)));
    let watching = watching(backend, app, Some("gungnir"));
    let mut screen = scripted(vec![
        (Duration::from_millis(100), key('x')),
        (Duration::from_millis(100), key('c')),
        (Duration::from_millis(100), enter()),
        (Duration::from_millis(100), Ok(Event::Resize(80, 24))),
    ]);

    let flow = switched_reading(&opener, watching, &mut screen).await;

    let watching = carried_on(flow);
    assert_eq!(watching.device.as_deref(), Some("bifrost"));
    assert!(gate.closed(), "the old device was not released");
    watching.close().await;
}

/// A stream that fails is counted the way the first open's wait counts it: four errors in a
/// row still leave Ctrl-C working, the fifth stops the reading.
#[tokio::test(start_paused = true)]
async fn input_errors_during_a_switch_stop_the_reading_after_the_same_count_as_the_first_open() {
    let failed = || Err(io::Error::other("the terminal failed"));
    let stop = crate::tui::waiting::STOP_AFTER_ERRORS;
    for (errors, gives_up) in [(stop - 1, true), (stop, false)] {
        let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
        app.devices.rows = device_rows();
        let opener = opener_over(slow_opener(Log::default(), Duration::from_secs(2)));
        let watching = watching(backend, app, Some("gungnir"));
        let mut script: Vec<_> = (0..errors)
            .map(|_| (Duration::from_millis(50), failed()))
            .collect();
        script.push((Duration::from_millis(50), key('q')));
        let mut screen = scripted(script);

        let flow = switched_reading(&opener, watching, &mut screen).await;

        if gives_up {
            gave_up(flow);
        } else {
            carried_on(flow).close().await;
        }
    }
}

/// A give-up key during the close of a switch changes the notice to name the device still
/// being closed, which is what the loop is waiting on from then until it ends.
#[tokio::test(start_paused = true)]
async fn a_give_up_during_the_close_says_which_device_is_closing() {
    let (watching, _gate) = watching_with_a_slow_close().await;
    let mut watching = watching;
    watching.app.devices.rows = device_rows();
    let opener = opener_over(slow_opener(Log::default(), Duration::from_secs(1)));
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut events = scripted(vec![(Duration::from_secs(1), key('q'))]).events;
    let mut screen = OnScreen {
        terminal: &mut terminal,
        hits: &mut hits,
        events: &mut events,
    };

    let switched = switch_device(&opener, &mut watching, "bifrost", &look(), &mut screen).await;

    assert_eq!(switched, Switched::GaveUp);
    let frames = frames.lock().expect("frames mutex");
    let opening = frames
        .iter()
        .position(|frame| frame.contains("opening bifrost…"))
        .expect("the switch never said it was opening");
    assert!(
        frames[opening..]
            .iter()
            .any(|frame| frame.contains("closing gungnir…")),
        "no frame after the key names the close still running"
    );
}

/// Giving up on a switch, during the open or the close, takes the Wait toast away with it.
#[tokio::test(start_paused = true)]
async fn giving_up_on_a_switch_clears_the_wait_toast() {
    let (mut watching, _gate) = watching_with_a_slow_close().await;
    watching.app.devices.rows = device_rows();
    let opener = opener_over(slow_opener(Log::default(), Duration::from_secs(5)));
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut events = scripted(vec![(Duration::from_secs(1), key('q'))]).events;
    let mut screen = OnScreen {
        terminal: &mut terminal,
        hits: &mut hits,
        events: &mut events,
    };

    let switched = switch_device(&opener, &mut watching, "bifrost", &look(), &mut screen).await;

    assert_eq!(switched, Switched::GaveUp);
    let frames = frames.lock().expect("frames mutex");
    assert!(
        frames
            .iter()
            .any(|frame| frame.contains("opening bifrost…")),
        "the wait never showed its toast"
    );
    assert!(watching.app.toast().is_none(), "{:?}", watching.app.toast());
}

/// Removing the watched device and giving up while its replacement opens ends the loop with
/// nothing held.
#[tokio::test(start_paused = true)]
async fn a_give_up_while_the_replacement_opens_after_a_removal_ends_the_tui() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.devices.rows = device_rows();
    let path = config_naming(&["gungnir", "bifrost"]);
    let log: Log = Log::default();
    let opener = opener_on(
        path.clone(),
        slow_opener(Arc::clone(&log), Duration::from_secs(30)),
    );
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.app.session.device_name = "gungnir".into();
    let _ = relisted(&path, &mut watching.app);
    let mut screen = scripted(vec![(Duration::from_secs(1), key('q'))]);
    let mut mouse = SpyMouse::default();
    let start = tokio::time::Instant::now();

    let flow = take_intent(
        Intent::RemoveDevice("gungnir".into()),
        &opener,
        watching,
        &mut mouse,
        &mut look(),
        &mut screen,
    )
    .await;

    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
    assert!(gave_up(flow).backend.is_none(), "a device was still held");
    assert_eq!(named(&path), ["bifrost"], "the block is still removed");
    assert_eq!(opens(&log).len(), 1, "no second open was tried");
}
