// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The link to the device: a feed that ends, a refresh that reads again, a reopen, the strip drawn
//! down and up, and a failure ageing out.

use super::*;

/// How long one reconnect attempt keeps trying, for a case that drives the loop across a
/// dropped link: long enough that the loop's ticks are what interrupt it, not its own
/// deadline.
const LONG_DEADLINE: Duration = Duration::from_secs(30);

/// How long the device takes to answer a connect, for a case about a link coming back:
/// several of the loop's ticks, as a real adapter's discovery and pairing are.
const CONNECT_TIME: Duration = Duration::from_secs(1);

/// A refresh with the feed open reads the device again, and takes what it answers in
/// place of what the app held.
#[tokio::test(start_paused = true)]
async fn a_refresh_reads_the_device_again() {
    let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
    let mut app = app_for(&backend);
    app.replace_all(vec![absent_reading()], Instant::now());
    let mut watching = watching(backend, app, None);

    dispatch(Intent::Refresh, &mut watching).await;

    assert!(
        raw_of(&watching.app, INPUT).is_some(),
        "an open feed is refreshed from the device"
    );
    assert!(
        !watching.app.readings.values.contains_key(&ABSENT),
        "a refresh replaces the readings the app held, it does not add to them"
    );
    watching.close().await;
}

/// Only a reading or a link that changed leaves the feed open. A feed that ended and one
/// that failed both close it, show the link down and say so: reading either again would
/// answer instantly and forever.
#[tokio::test(start_paused = true)]
async fn an_ended_or_failed_feed_closes_and_the_app_shows_it_down() {
    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let now = Instant::now();
    let down = Update::Connection { connected: false };

    assert!(
        heard(Ok(Some(down)), &mut backend, &mut app, now).await,
        "a link that went down is not the end of the feed"
    );
    assert!(!app.session.connected);
    assert_eq!(
        app.toast().map(|toast| toast.text.clone()),
        None,
        "a disconnect is drawn, not announced"
    );

    app.session.connected = true;
    assert!(
        !heard(Err(BleError::NotFound), &mut backend, &mut app, now).await,
        "a feed that failed is never read again"
    );
    assert!(!app.session.connected);
    assert_eq!(
        app.toast().map(|toast| toast.text.clone()),
        Some(BleError::NotFound.to_string())
    );

    app.session.connected = true;
    assert!(
        !heard(Ok(None), &mut backend, &mut app, now).await,
        "an ended feed is never read again"
    );
    assert!(!app.session.connected);
    assert_eq!(app.toast().map(|toast| toast.text.as_str()), Some(GONE));
}

/// Reading the whole device again is what a link coming back costs, and a link that never
/// went away is not one coming back.
///
/// A subscription opens by saying the link is up — the daemon's does — which arrives on a
/// feed whose app already has the link up. That is no reconnect, so it costs no round trip
/// and leaves every reading's age and rate as they were.
#[tokio::test(start_paused = true)]
async fn a_link_that_never_went_away_is_not_read_again() {
    let (mut backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let now = Instant::now();
    app.replace_all(vec![absent_reading()], now);
    let up = || Update::Connection { connected: true };

    assert!(heard(Ok(Some(up())), &mut backend, &mut app, now).await);
    assert!(
        app.readings.values.contains_key(&ABSENT),
        "the device was read again over a link that was up all along"
    );

    app.apply(Update::Connection { connected: false }, now);
    assert!(heard(Ok(Some(up())), &mut backend, &mut app, now).await);
    assert!(
        !app.readings.values.contains_key(&ABSENT),
        "a link that really came back left the app on what it held while away"
    );
    assert!(
        raw_of(&app, INPUT).is_some(),
        "a link that really came back was not read again"
    );
}

/// A refresh with the feed closed opens the device again, through the very steps the TUI
/// started on: the link is gone for good, and there is nothing left to ask it.
#[tokio::test(start_paused = true)]
async fn a_refresh_on_a_closed_feed_opens_the_device_again() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.replace_all(vec![absent_reading()], Instant::now());
    let log: Log = Log::default();
    let opener = opener_over(fake_open(Arc::clone(&log), Arc::clone(&gate), &[]));
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.feed_open = false;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![key('r')]).chain(quit_after(Duration::from_secs(5)));

    let end = event_loop(LoopParts {
        opener: &opener,
        adder: &nothing_to_add(),
        watching,
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    assert_eq!(
        opens(&log),
        vec![(Some("gungnir".to_string()), true)],
        "the refresh did not open the device the loop was on again"
    );
    assert!(
        any_frame(&drawn(&frames), "USB 1"),
        "the frame after the refresh does not draw what the device holds"
    );
    end.watching.close().await;
}

/// Quitting while a refresh reopens the device on a closed feed stops the open at once and
/// ends the loop cleanly, holding nothing.
#[tokio::test(start_paused = true)]
async fn quitting_during_the_reopen_of_a_refresh_ends_the_loop() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let log: Log = Log::default();
    let opener = opener_over({
        let log = Arc::clone(&log);
        move |globals: OwnedGlobals| {
            let log = Arc::clone(&log);
            async move {
                log.lock().expect("log mutex").push((globals.device, true));
                tokio::time::sleep(Duration::from_secs(30)).await;
                let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
                Ok((backend, device_rows()))
            }
        }
    });
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.feed_open = false;
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let script = stream::iter(vec![key('r')]).chain(quit_after(Duration::from_secs(1)));
    let start = tokio::time::Instant::now();

    let end = event_loop(LoopParts {
        opener: &opener,
        adder: &nothing_to_add(),
        watching,
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
    assert!(end.watching.backend.is_none(), "a device was still held");
    assert_eq!(opens(&log).len(), 1, "no second open was tried");
}

/// A failure's toast is gone six seconds after the tick that stamped it.
#[tokio::test(start_paused = true)]
async fn a_failure_ages_out_after_six_seconds() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    let start = Instant::now();
    watching.app.tick(start);
    dispatch(refused_write(), &mut watching).await;
    watching.app.tick(start + Duration::from_millis(5_750));
    assert!(watching.app.toast().is_some(), "the toast went early");
    watching.app.tick(start + Duration::from_millis(6_000));
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        None,
        "the toast outstayed its six seconds"
    );
    watching.close().await;
}

/// A link that drops is drawn down, drawn up again once it is back, and drawn with what
/// the device holds now rather than what it held before it went away.
///
/// The device takes [`CONNECT_TIME`] to answer a connect, as a real adapter does, which
/// is several of the loop's ticks: the wait for it is dropped over and over while it
/// runs. So the link only ever comes back if the attempt survives being dropped — a loop
/// that starts a fresh connect on each turn never gets one far enough to finish. The
/// input below moves while the device is away, and a reconnect folds its refresh burst
/// into the session rather than reporting it as updates, so the frame that draws the link
/// back shows the new value only if the loop read the device again.
///
/// This attempt is still under way when the link drops — nothing has given up on it — so
/// the header's spinner turns while it is down, exactly as it would for a daemon session
/// whose own backoff is still trying; it stops the moment the link answers.
#[tokio::test(start_paused = true)]
async fn a_dropped_link_is_drawn_down_then_up_again_with_what_the_device_holds_now() {
    let (backend, gate, app) = fake(LONG_DEADLINE).await;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    gate.slow_connects(CONNECT_TIME);
    gate.sever();
    gate.set_raw(INPUT, Value::U8(4));

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: quit_after(Duration::from_secs(5)),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    let frames = drawn(&frames);
    let down = first_frame(&frames, "○ disconnected");
    assert!(
        frames[down].contains("USB 1"),
        "the device was on input 1 when the link went down"
    );
    assert!(
        has_spinner(&frames[down]),
        "the header shows no spinner while the reconnect is still under way:\n{}",
        frames[down]
    );
    let up = first_frame(&frames[down..], "● connected") + down;
    assert!(up > down, "the link came back before it went down");
    assert!(
        !has_spinner(&frames[up]),
        "the spinner outlived the link it was turning for:\n{}",
        frames[up]
    );
    assert!(
        frames[up].contains("Coaxial"),
        "the frame drawing the link back shows what the device read before it went away:\n{}",
        frames[up]
    );
    end.watching.close().await;
}

/// A feed that has ended stops being read: the loop says so, keeps drawing and keeps
/// taking keys, and turns on its own tick rather than on a feed that answers instantly.
#[tokio::test(start_paused = true)]
async fn an_ended_feed_leaves_the_loop_ticking_rather_than_spinning() {
    let (backend, gate, app) = fake(SHORT_DEADLINE).await;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();

    // Out of reach for good: the attempt spends its deadline and the feed ends.
    gate.refuse_connects();
    gate.sever();

    let seconds = 5;
    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: quit_after(Duration::from_secs(seconds)),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "the loop kept taking keys: {:?}", end.why);
    let frames = drawn(&frames);
    let gone = first_frame(&frames, GONE);
    // The one connect attempt this session gets is still under way for a moment before its
    // deadline passes, which is a genuine wait and turns the spinner like any other; once
    // the feed has said so — the frame that draws GONE, and everything after it — nothing
    // is trying any more, and the spinner has no business turning.
    assert!(
        !frames[gone..].iter().any(|f| has_spinner(f)),
        "a session with nothing left to read from still turned a spinner"
    );
    // One frame per tick over the five seconds, plus the few the updates themselves drew:
    // nothing is retrying this any more, so the loop is back on its own quarter-second tick
    // rather than the header's hundred-millisecond one.
    let ticks = usize::try_from(seconds).expect("a small number of seconds") * 1_000 / 250;
    assert!(
        frames.len() < ticks + 20,
        "the loop drew {} frames in {seconds}s, which is more than its own tick",
        frames.len()
    );
    end.watching.close().await;
}
