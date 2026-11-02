// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use ratatui::Terminal;
use svolder_host::fake::{CaptureCall, FakeCapture, FakePlayers, FakeSink};
use svolder_host::{NoBus, Status};

use crate::tui::decoration::title::TITLE_PERIOD;
use crate::wizard::fake::FakeEffects;

use super::super::test_support::*;
use super::super::*;
use super::*;

/// A stream that says nothing until `after`, then quits: the `q` that asks and the `q`
/// that answers.
fn quit_after(after: Duration) -> BoxStream<'static, std::io::Result<crossterm::event::Event>> {
    Box::pin(
        stream::once(async move {
            tokio::time::sleep(after).await;
            key('q')
        })
        .chain(stream::iter(vec![key('q')])),
    )
}

/// Run the loop on `watching` with `script` as its input, until the script quits it.
async fn run(
    watching: Watching,
    script: BoxStream<'static, std::io::Result<crossterm::event::Event>>,
) -> LoopEnd {
    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let adds = Adds {
        effects: FakeEffects::healthy(),
    };
    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &adds,
        watching,
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;
    assert_eq!(end.code, 0, "{:?}", end.why);
    end
}

/// While the device screen shows, the sink is read once a second — the first read at once
/// — and the players are followed from the moment the bus answers: nothing is written to
/// the sink and nothing is pressed on a player without a key asking for it.
#[tokio::test(start_paused = true)]
async fn the_sink_is_read_every_second_and_nothing_is_asked_without_a_key() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let players = FakePlayers::new(Some(track("So What", Status::Playing)));
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, finds(players.clone()));

    let end = run(watching, quit_after(Duration::from_millis(3_500))).await;

    assert_eq!(
        sink.reads(),
        4,
        "not one read a second from the first frame on"
    );
    assert_eq!(sink.writes(), [], "the sink was written to with no key");
    assert_eq!(players.presses(), [], "a player was pressed with no key");
    let host = &end.watching.app.host;
    assert_eq!(host.sink, Some(sink_state(0.54, false)));
    assert!(host.bus, "the bus that answered is not followed");
    assert_eq!(host.playing, Some(track("So What", Status::Playing)));
    end.watching.close().await;
}

/// Off the device screen the sink is not read at all, and back on it it is read at once.
#[tokio::test(start_paused = true)]
async fn the_sink_is_read_only_while_the_device_screen_shows() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.show(Screen::Telemetry);
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, no_bus());
    let script = Box::pin(
        stream::once(async {
            tokio::time::sleep(Duration::from_millis(2_500)).await;
            Ok(crossterm::event::Event::Key(
                crossterm::event::KeyCode::Esc.into(),
            ))
        })
        .chain(quit_after(Duration::from_millis(500))),
    );

    let end = run(watching, script).await;

    assert_eq!(
        sink.reads(),
        1,
        "the sink was read off the device screen, or not at once back on it"
    );
    assert_eq!(end.watching.app.host.sink, Some(sink_state(0.54, false)));
    assert!(
        !end.watching.app.host.bus,
        "a bus that never answered is followed"
    );
    end.watching.close().await;
}

/// With `[tui] host_audio` off nothing of the host is read, and the players are not even
/// looked for.
#[tokio::test(start_paused = true)]
async fn with_host_audio_off_nothing_of_the_host_is_read_or_looked_for() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.tui.host_audio = false;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connect = Box::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(NoBus::Absent) })
    });
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, connect);

    let end = run(watching, quit_after(Duration::from_secs(3))).await;

    assert_eq!(sink.reads(), 0, "the sink was read with host audio off");
    assert_eq!(
        looked.load(Ordering::SeqCst),
        0,
        "the players were looked for"
    );
    assert_eq!(end.watching.app.host.sink, None);
    end.watching.close().await;
}

/// The players are looked for once per session, the first time host audio shows: never
/// while it does not, and never again after.
#[tokio::test(start_paused = true)]
async fn the_players_are_looked_for_once_the_first_time_host_audio_shows() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.host.linked = true;
    app.tui.host_audio = false;
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connect = Box::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(NoBus::Absent) })
    });
    let mut link = HostLink::over(None, connect);
    link.wake(&app);
    assert_eq!(
        looked.load(Ordering::SeqCst),
        0,
        "looked for with host audio off"
    );
    app.tui.host_audio = true;
    link.wake(&app);
    link.wake(&app);
    assert_eq!(
        looked.load(Ordering::SeqCst),
        1,
        "not looked for exactly once"
    );
    assert!(link.connecting.is_some());
    drop(backend);
}

/// What the followed player plays is followed as it changes, and once the bus goes the
/// Now playing row has nothing left to say.
#[tokio::test(start_paused = true)]
async fn what_the_player_plays_is_followed_until_the_bus_goes() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let players = FakePlayers::new(Some(track("So What", Status::Playing)));
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, finds(players.clone()));
    let held = players.clone();
    let changes = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        held.set(Some(track("Blue in Green", Status::Paused)));
    });

    let end = run(watching, quit_after(Duration::from_secs(2))).await;
    changes.await.expect("the player changed");
    assert_eq!(
        end.watching.app.host.playing,
        Some(track("Blue in Green", Status::Paused)),
        "the new track was not followed"
    );

    let mut watching = end.watching;
    players.lose();
    let lost = run(watching, quit_after(Duration::from_secs(1))).await;
    watching = lost.watching;
    assert!(!watching.app.host.bus, "a bus that went is still followed");
    assert_eq!(watching.app.host.playing, None);
    assert!(watching.host.changes.is_none() && watching.host.players.is_none());
    watching.close().await;
}

/// A bus that was found and goes away is looked for again every 10 s, as often as it takes, and
/// the Now playing row follows the players on the next bus; one that is back is not looked for.
#[tokio::test(start_paused = true)]
async fn a_bus_that_goes_away_is_looked_for_again_every_10_s() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let first = FakePlayers::new(Some(track("So What", Status::Playing)));
    let second = FakePlayers::new(Some(track("Blue in Green", Status::Playing)));
    let looked = Arc::new(AtomicUsize::new(0));
    let (counted, held, next) = (Arc::clone(&looked), first.clone(), second.clone());
    let connect: Connect = Box::new(move || {
        // the bus is back on the third look, not the second
        let found = match counted.fetch_add(1, Ordering::SeqCst) {
            0 => Ok(Arc::new(held.clone()) as Arc<dyn Players>),
            2 => Ok(Arc::new(next.clone()) as Arc<dyn Players>),
            _ => Err(NoBus::Absent),
        };
        Box::pin(async move { found })
    });
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, connect);

    watching = run(watching, quit_after(Duration::from_secs(25)))
        .await
        .watching;
    assert_eq!(
        looked.load(Ordering::SeqCst),
        1,
        "a bus still there was looked for"
    );
    first.lose();
    watching = run(watching, quit_after(Duration::from_secs(5)))
        .await
        .watching;
    assert!(!watching.app.host.bus && watching.app.host.playing.is_none());
    assert_eq!(looked.load(Ordering::SeqCst), 1, "looked for before 10 s");
    watching = run(watching, quit_after(Duration::from_secs(6)))
        .await
        .watching;
    assert_eq!(looked.load(Ordering::SeqCst), 2);
    assert!(!watching.app.host.bus, "a look that found none found a bus");
    watching = run(watching, quit_after(Duration::from_secs(10)))
        .await
        .watching;
    assert_eq!(looked.load(Ordering::SeqCst), 3);
    assert_eq!(
        watching.app.host.playing,
        Some(track("Blue in Green", Status::Playing)),
        "the next bus's players were not followed"
    );
    for _ in 0..3 {
        watching = run(watching, quit_after(Duration::from_secs(10)))
            .await
            .watching;
    }
    assert_eq!(
        looked.load(Ordering::SeqCst),
        3,
        "a bus that is back was looked for"
    );
    watching.close().await;
}

/// A bus that was never found is not looked for again: only a reload of the session would.
#[tokio::test(start_paused = true)]
async fn a_bus_never_found_is_not_looked_for_again() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connect = Box::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(NoBus::Absent) })
    });
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, connect);
    for _ in 0..3 {
        watching = run(watching, quit_after(Duration::from_secs(10)))
            .await
            .watching;
    }
    assert_eq!(looked.load(Ordering::SeqCst), 1);
    watching.close().await;
}

/// A bus that took the connection and has not answered is looked for again every 10 s even though
/// none was found yet, until it answers: a look refused only because an earlier one still waits
/// on it is not the end of the search.
#[tokio::test(start_paused = true)]
async fn a_bus_that_has_not_answered_is_looked_for_again_until_it_does() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let players = FakePlayers::new(Some(track("So What", Status::Playing)));
    let looked = Arc::new(AtomicUsize::new(0));
    let (counted, held) = (Arc::clone(&looked), players.clone());
    let connect: Connect = Box::new(move || {
        // silent on the first two looks, answering on the third
        let found = match counted.fetch_add(1, Ordering::SeqCst) {
            0 | 1 => Err(NoBus::Silent),
            _ => Ok(Arc::new(held.clone()) as Arc<dyn Players>),
        };
        Box::pin(async move { found })
    });
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, connect);

    watching = run(watching, quit_after(Duration::from_secs(5)))
        .await
        .watching;
    assert_eq!(looked.load(Ordering::SeqCst), 1, "the first look");
    assert_eq!(watching.app.host.playing, None);
    watching = run(watching, quit_after(Duration::from_secs(10)))
        .await
        .watching;
    assert_eq!(looked.load(Ordering::SeqCst), 2, "the second look, 10 s on");
    watching = run(watching, quit_after(Duration::from_secs(10)))
        .await
        .watching;
    assert_eq!(looked.load(Ordering::SeqCst), 3, "the third look, 10 s on");
    assert_eq!(
        watching.app.host.playing.as_ref().map(|n| n.title.as_str()),
        Some("So What"),
        "the bus that answered is followed"
    );
    for _ in 0..2 {
        watching = run(watching, quit_after(Duration::from_secs(15)))
            .await
            .watching;
    }
    assert_eq!(
        looked.load(Ordering::SeqCst),
        3,
        "a bus that answered was looked for"
    );
    watching.close().await;
}

/// A bus that took the connection and then, on a later look, is not there is still looked for
/// every 10 s: a bus once seen is not given up.
#[tokio::test(start_paused = true)]
async fn a_silent_bus_that_then_looks_absent_is_still_looked_for() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connect = Box::new(move || {
        let found = match counted.fetch_add(1, Ordering::SeqCst) {
            0 => Err(NoBus::Silent),
            _ => Err(NoBus::Absent),
        };
        Box::pin(async move { found })
    });
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, connect);
    for _ in 0..3 {
        watching = run(watching, quit_after(Duration::from_secs(10)))
            .await
            .watching;
    }
    assert!(
        looked.load(Ordering::SeqCst) >= 3,
        "gave up after {} looks",
        looked.load(Ordering::SeqCst)
    );
    watching.close().await;
}

/// While host audio is off the bus is not looked for, and no timer runs for it: the look that came
/// due is made the moment host audio shows again.
#[tokio::test(start_paused = true)]
async fn the_look_that_comes_due_with_host_audio_off_waits_for_it_to_show() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connect = Box::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(NoBus::Silent) })
    });
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, connect);

    watching = run(watching, quit_after(Duration::from_secs(5)))
        .await
        .watching;
    assert_eq!(looked.load(Ordering::SeqCst), 1, "the first look");
    assert!(
        watching.host.redial.is_some(),
        "the next look is waited for"
    );

    // Host audio goes off before the look is due.
    watching.app.tui.host_audio = false;
    for _ in 0..3 {
        watching = run(watching, quit_after(Duration::from_secs(20)))
            .await
            .watching;
    }
    assert_eq!(
        looked.load(Ordering::SeqCst),
        1,
        "looked with host audio off"
    );
    assert!(
        watching.host.redial.is_none(),
        "a timer keeps running for nothing while host audio is off"
    );

    // And on again: the look that was due is made at once, and the retries go on.
    watching.app.tui.host_audio = true;
    watching = run(watching, quit_after(Duration::from_secs(1)))
        .await
        .watching;
    assert_eq!(
        looked.load(Ordering::SeqCst),
        2,
        "the look owed was not made"
    );
    watching = run(watching, quit_after(Duration::from_secs(10)))
        .await
        .watching;
    assert_eq!(
        looked.load(Ordering::SeqCst),
        3,
        "the retries did not go on"
    );
    watching.close().await;
}

/// The loop's own futures, measured as the test thread's 2 MB stack feels them: what a poll of
/// the whole loop, or of the step that keeps the daemon's feed in line, holds on it. The sizes
/// are of this debug build; a future that outgrows its bound wants what it holds across an
/// await boxed or built in a function of its own (`ask_again`), not the bound raised. The canary
/// is `a_key_on_host_volume_reaches_the_sink_and_a_press_on_next_the_player`, which overflowed
/// first when they grew.
#[tokio::test]
async fn the_loops_futures_stay_small() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    let step = super::super::events::match_host_feed(&mut watching);
    let step_size = std::mem::size_of_val(&step);
    drop(step);

    let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let adds = Adds {
        effects: FakeEffects::healthy(),
    };
    let opener = opener_over(never_opens());
    let mut meter = title();
    let looping = event_loop(LoopParts {
        opener: &opener,
        adder: &adds,
        watching,
        terminal: &mut terminal,
        events: quit_after(Duration::from_secs(1)),
        mouse: &mut mouse,
        title: &mut meter,
        look: look(),
    });
    let loop_size = std::mem::size_of_val(&looping);
    drop(looping);
    for (name, size, bound) in [
        ("the loop", loop_size, 48 * 1024),
        ("the feed step", step_size, 1024),
    ] {
        assert!(
            size <= bound,
            "{name}'s future is {size} bytes, over {bound}: box what it holds across an await"
        );
    }
}

/// A sink that takes ten seconds to answer each read.
struct SlowSink {
    /// How many reads were started.
    reads: Arc<AtomicUsize>,
}

#[async_trait]
impl Sink for SlowSink {
    async fn read(&self) -> Option<SinkState> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_secs(10)).await;
        Some(sink_state(0.54, false))
    }

    async fn set_volume(&self, _volume: f32) -> bool {
        true
    }

    async fn toggle_mute(&self) -> bool {
        true
    }
}

/// A read that takes its time holds up no frame and no key, and no read is asked for
/// while one is still on its way: the loop quits when asked, with the one read started.
#[tokio::test(start_paused = true)]
async fn a_slow_read_holds_up_nothing_and_is_never_piled_on() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.host.linked = true;
    let reads = Arc::new(AtomicUsize::new(0));
    let mut watching = watching(backend, app, None);
    let sink = SlowSink {
        reads: Arc::clone(&reads),
    };
    watching.host = HostLink::over(Some(Arc::new(sink)), no_bus());
    let started = tokio::time::Instant::now();

    let end = run(watching, quit_after(Duration::from_secs(3))).await;

    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the loop waited on the read: {:?}",
        started.elapsed()
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        1,
        "reads were piled on a slow one"
    );
    assert_eq!(
        end.watching.host.outstanding, 1,
        "reads were queued behind a slow one"
    );
    assert_eq!(end.watching.app.host.sink, None, "no read has answered yet");
    end.watching.close().await;
}

/// One key press as a terminal reports it.
fn press(code: crossterm::event::KeyCode) -> std::io::Result<crossterm::event::Event> {
    Ok(crossterm::event::Event::Key(code.into()))
}

/// `presses`, once the loop has had `after` to read the sink, then a quit a second later.
fn pressed_after(
    after: Duration,
    presses: Vec<std::io::Result<crossterm::event::Event>>,
) -> BoxStream<'static, std::io::Result<crossterm::event::Event>> {
    Box::pin(
        stream::once(async move {
            tokio::time::sleep(after).await;
            stream::iter(presses)
        })
        .flatten()
        .chain(quit_after(Duration::from_secs(1))),
    )
}

/// `→` twice and `Space` on Host volume reach the sink as two steps of 2 % and a mute, in
/// the order they were pressed: the second step follows on from the first even before the
/// sink has answered it.
#[tokio::test(start_paused = true)]
async fn the_keys_on_host_volume_reach_the_sink_in_order() {
    use crossterm::event::KeyCode;
    use svolder_host::fake::SinkWrite;

    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let own = app.rows().len();
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, no_bus());
    let mut keys: Vec<_> = (0..own).map(|_| press(KeyCode::Down)).collect();
    keys.extend([
        press(KeyCode::Right),
        press(KeyCode::Right),
        press(KeyCode::Char(' ')),
    ]);

    let end = run(watching, pressed_after(Duration::from_millis(500), keys)).await;

    assert_eq!(
        sink.writes(),
        [
            SinkWrite::Volume(0.56),
            SinkWrite::Volume(0.58),
            SinkWrite::ToggleMute
        ]
    );
    let host = &end.watching.app.host;
    assert_eq!(
        host.sink,
        Some(sink_state(0.58, true)),
        "the answers were not read"
    );
    assert_eq!(host.asked, None, "the ask outlived its answer");
    end.watching.close().await;
}

/// `→` on Host volume reaches the sink and `Enter` on `▶▶` reaches the followed player,
/// and a click on `▶` presses it too; nothing else reaches either — every write and press
/// is one a key or a click asked for.
#[tokio::test(start_paused = true)]
async fn a_key_on_host_volume_reaches_the_sink_and_a_press_on_next_the_player() {
    use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
    use svolder_host::Button;
    use svolder_host::fake::SinkWrite;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    let own = app.rows().len();
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let players = FakePlayers::new(Some(track("So What", Status::Playing)));
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, finds(players.clone()));
    let mut keys: Vec<_> = (0..own).map(|_| press(KeyCode::Down)).collect();
    keys.push(press(KeyCode::Right));
    keys.push(press(KeyCode::Down));
    keys.extend((0..3).map(|_| press(KeyCode::Right)));
    keys.push(press(KeyCode::Enter));
    // `▶`, the Transport row's second button on the 80×24 frame, at column 21.
    keys.push(Ok(crossterm::event::Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 21,
        row: 14,
        modifiers: crossterm::event::KeyModifiers::NONE,
    })));

    let end = run(watching, pressed_after(Duration::from_millis(500), keys)).await;

    assert_eq!(sink.writes(), [SinkWrite::Volume(0.56)]);
    assert_eq!(players.presses(), [Button::Next, Button::Play]);
    assert_eq!(
        end.watching.app.toast().map(|toast| toast.text.clone()),
        None,
        "{:?}",
        end.watching.app.toast().map(|toast| toast.text.clone())
    );
    end.watching.close().await;
}

/// A press with no player to take it reaches no player, and the key line says so.
#[tokio::test(start_paused = true)]
async fn a_press_with_no_player_reaches_nothing_and_says_so() {
    use crossterm::event::KeyCode;

    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let own = app.rows().len();
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let players = FakePlayers::new(None);
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, finds(players.clone()));
    let mut keys: Vec<_> = (0..own + 1).map(|_| press(KeyCode::Down)).collect();
    keys.push(press(KeyCode::Enter));
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let adds = Adds {
        effects: FakeEffects::healthy(),
    };

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &adds,
        watching,
        terminal: &mut terminal,
        events: pressed_after(Duration::from_millis(500), keys),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;

    assert_eq!(
        players.presses(),
        [],
        "a press went to a bus with no player"
    );
    assert_eq!(sink.writes(), []);
    assert!(
        any_frame(&drawn(&frames), " no player"),
        "the key line never said there is no player"
    );
    end.watching.close().await;
}

/// A host intent that arrives while host audio is off asks nothing of the sink and
/// presses nothing on a player.
#[tokio::test(start_paused = true)]
async fn nothing_reaches_the_sink_while_host_audio_is_off() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.tui.host_audio = false;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let players = FakePlayers::new(Some(track("So What", Status::Playing)));
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, no_bus());
    // Found as they would be had host audio shown earlier in the session.
    let found = players.clone();
    watching
        .host
        .found(Ok(Arc::new(found) as Arc<dyn Players>), &mut watching.app);
    let mut mouse = SpyMouse::default();
    for intent in [
        Intent::HostVolume(56),
        Intent::HostMute,
        Intent::HostPress(svolder_host::Button::Next),
    ] {
        let flow = take_intent(
            intent,
            &opener_over(never_opens()),
            watching,
            &mut mouse,
            &mut look(),
            &mut (),
        )
        .await;
        let Flow::Continue(next) = flow else {
            panic!("a host intent ended the loop");
        };
        watching = next;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        sink.writes(),
        [],
        "the sink was written with host audio off"
    );
    assert_eq!(
        players.presses(),
        [],
        "a player was pressed with host audio off"
    );
    assert!(!watching.host.waiting());
    watching.close().await;
}

/// Under `SVOLDER_HOST=off` the session reaches nothing: no sink, no players to look for,
/// and nothing to capture.
#[test]
fn under_svolder_host_off_the_session_reaches_nothing() {
    let link = HostLink::session(true);
    assert!(link.ops.is_none() && link.states.is_none() && link.poll.is_none());
    assert!(link.connect.is_none(), "the players would be looked for");
    assert!(
        link.capture.is_none(),
        "the sink's monitor would be captured"
    );
}

/// An app on the Gungnir 2, a DAC, linked to a host whose default sink has read, naming
/// the node a capture of its monitor targets.
fn dac() -> App {
    let profile = svolder_profiles::Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("gungnir2-v2 is built in")
        .clone();
    let mut app = App::new(
        profile,
        "gungnir".into(),
        &svolder_ipc::TuiConfig::default(),
        Vec::new(),
    );
    app.host.linked = true;
    app.host.fold_read(Some(sink_state(0.54, false)), true);
    app
}

/// The capture of the sink `sink_state` names.
fn usb() -> CaptureCall {
    CaptureCall::Start("alsa_output.usb-dac".into())
}

/// The capture runs while something wants a level no meter streams — the title meter, the
/// breathing borders — and host audio shows, on the node the default sink named: it moves
/// with the sink, and stops when the switches go off, when the sink names no node, and
/// when a meter of the device's starts to stream.
#[test]
fn the_capture_follows_the_switches_the_sink_and_the_meters() {
    let capture = FakeCapture::new();
    let mut link = HostLink::none().with_capture(Box::new(capture.clone()));
    let mut app = dac();
    link.steer(&mut app);
    link.steer(&mut app);
    assert_eq!(
        capture.calls(),
        [usb()],
        "not started once, for the title meter"
    );
    assert!(app.host.capturing);

    app.tui.title_meter = false;
    link.steer(&mut app);
    assert!(!app.host.capturing, "running with nothing wanting a level");
    app.breathing = true;
    link.steer(&mut app);
    app.tui.host_audio = false;
    link.steer(&mut app);
    app.tui.host_audio = true;
    link.steer(&mut app);
    let mut other = sink_state(0.54, false);
    other.target = Some("hdmi-out".into());
    app.host.fold_read(Some(other), true);
    link.steer(&mut app);
    let mut unnamed = sink_state(0.54, false);
    unnamed.target = None;
    app.host.fold_read(Some(unnamed), true);
    link.steer(&mut app);
    app.host.fold_read(Some(sink_state(0.54, false)), true);
    link.steer(&mut app);
    app.profile = svolder_profiles::Registry::builtin()
        .expect("the built-in profiles load")
        .get("fake-amp")
        .expect("fake-amp is built in")
        .clone();
    let meter = svolder_ipc::ValueObject {
        heard_at_ms: None,
        id: "20.1".into(),
        key: None,
        label: "Meter".into(),
        value_type: "u16".into(),
        raw: serde_json::json!(900),
        display: "900".into(),
        value: None,
        unit: None,
    };
    app.apply(
        crate::backend::Update::Status(meter),
        std::time::Instant::now(),
    );
    link.steer(&mut app);

    assert_eq!(
        capture.calls(),
        [
            usb(),
            CaptureCall::Stop,
            usb(),
            CaptureCall::Stop,
            usb(),
            CaptureCall::Start("hdmi-out".into()),
            CaptureCall::Stop,
            usb(),
            CaptureCall::Stop,
        ]
    );
    assert!(!app.host.capturing, "running while the meters stream");
}

/// With nothing wanting a level, the capture runs for the Spectrum row alone, while the
/// frame last drawn put it on the screen — and on an amp whose meters stream, which keep
/// the level, the row still has it run.
#[test]
fn the_capture_runs_for_the_spectrum_row_while_it_is_on_the_screen() {
    let capture = FakeCapture::new();
    let mut link = HostLink::none().with_capture(Box::new(capture.clone()));
    let mut app = dac();
    app.tui.title_meter = false;
    link.steer(&mut app);
    assert_eq!(
        capture.calls(),
        [],
        "started with nothing on the screen for it"
    );
    app.host.spectrum_on_screen = true;
    link.steer(&mut app);
    app.host.spectrum_on_screen = false;
    link.steer(&mut app);
    app.host.spectrum_on_screen = true;
    link.steer(&mut app);
    app.profile = svolder_profiles::Registry::builtin()
        .expect("the built-in profiles load")
        .get("fake-amp")
        .expect("fake-amp is built in")
        .clone();
    let meter = svolder_ipc::ValueObject {
        heard_at_ms: None,
        id: "20.1".into(),
        key: None,
        label: "Meter".into(),
        value_type: "u16".into(),
        raw: serde_json::json!(900),
        display: "900".into(),
        value: None,
        unit: None,
    };
    app.apply(
        crate::backend::Update::Status(meter),
        std::time::Instant::now(),
    );
    app.tui.title_meter = true;
    link.steer(&mut app);
    assert_eq!(capture.calls(), [usb(), CaptureCall::Stop, usb()]);
    assert!(app.host.capturing, "the row lost its capture to the meters");
    assert_eq!(
        app.level_from(),
        crate::tui::audio_level::LevelFrom::Meters,
        "the row's capture took the level from the meters"
    );
}

/// A capture moved to another sink keeps nothing the old sink played: the level, the
/// channels and the Spectrum row's bands start again from the new sink's first block.
#[test]
fn a_capture_moved_to_another_sink_starts_its_readings_again() {
    let capture = FakeCapture::new();
    let mut link = HostLink::none().with_capture(Box::new(capture.clone()));
    let mut app = dac();
    link.steer(&mut app);
    app.heard_block(&block(0.9, 0.9), std::time::Instant::now());
    assert!(app.host.level.is_some() && app.host.channels.is_some());
    let mut hdmi = sink_state(0.54, false);
    hdmi.target = Some("hdmi-out".into());
    app.host.fold_read(Some(hdmi), true);
    link.steer(&mut app);

    assert_eq!(
        capture.calls(),
        [usb(), CaptureCall::Start("hdmi-out".into())]
    );
    assert!(app.host.capturing, "the moved capture reads as stopped");
    assert_eq!(app.host.level, None, "the old sink's level stayed");
    assert_eq!(app.host.channels, None, "the old sink's channels stayed");
    assert!(
        !app.host.bands.moving(std::time::Instant::now()),
        "the old sink's bands stayed"
    );
    assert_eq!(app.level_from(), crate::tui::audio_level::LevelFrom::Host);
}

/// A capture that could not start — no `pw-record`, say — is not started again on every
/// frame: the sink is left alone for [`CAPTURE_RETRY`], then tried once more.
#[tokio::test(start_paused = true)]
async fn a_capture_that_could_not_start_is_tried_again_only_after_its_wait() {
    let capture = FakeCapture::new();
    capture.refuse(true);
    let mut link = HostLink::none().with_capture(Box::new(capture.clone()));
    let mut app = dac();
    for _ in 0..5 {
        link.steer(&mut app);
    }
    assert_eq!(capture.calls(), [usb()], "started again on every frame");
    assert!(!app.host.capturing);
    tokio::time::advance(CAPTURE_RETRY - Duration::from_millis(1)).await;
    link.steer(&mut app);
    assert_eq!(
        capture.calls().len(),
        1,
        "tried again before its wait was out"
    );
    tokio::time::advance(Duration::from_millis(1)).await;
    capture.refuse(false);
    link.steer(&mut app);
    assert_eq!(capture.calls(), [usb(), usb()]);
    assert!(app.host.capturing, "the second go did not take");
}

/// A capture that stopped by itself — the tool quit, the sink went — leaves nothing to
/// draw and is left alone for [`CAPTURE_RETRY`] before it is started again.
#[tokio::test(start_paused = true)]
async fn a_capture_that_stopped_by_itself_is_left_for_its_wait() {
    let held = FakeCapture::new();
    let mut link = HostLink::none().with_capture(Box::new(held.clone()));
    let mut app = dac();
    link.steer(&mut app);
    assert!(held.push(block(0.5, 0.5)));
    held.end();
    let first = next_block(&mut link.capture).await;
    link.took(first, &mut app);
    assert!(app.host.level.is_some(), "the block was not folded in");
    let ended = next_block(&mut link.capture).await;
    assert_eq!(ended, None, "the fake did not end");
    link.took(ended, &mut app);
    assert!(!app.host.capturing && !link.capturing());
    assert_eq!(
        app.host.level, None,
        "a stopped capture left its level drawn"
    );
    assert!(
        !app.host.bands.moving(std::time::Instant::now()),
        "a stopped capture left its bands falling"
    );

    link.steer(&mut app);
    assert_eq!(held.calls(), [usb()], "started again at once");
    tokio::time::advance(CAPTURE_RETRY).await;
    link.steer(&mut app);
    assert_eq!(held.calls(), [usb(), usb()]);
    assert!(app.host.capturing);
}

/// On a DAC the capture starts once the sink has read and named its node, and stops the
/// moment the loop ends.
#[tokio::test(start_paused = true)]
async fn the_capture_starts_once_the_sink_names_its_node_and_stops_when_the_loop_ends() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let capture = FakeCapture::new();
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, no_bus());
    capture_on(&mut watching, &capture);

    let end = run(watching, quit_after(Duration::from_secs(2))).await;

    assert_eq!(capture.calls(), [usb(), CaptureCall::Stop]);
    assert_eq!(capture.target(), None, "the capture outlived the loop");
    assert!(!end.watching.app.host.capturing);
    end.watching.close().await;
}

/// With no sink the host named — none read, or one that named no node — nothing is ever
/// captured, whatever wants a level.
#[tokio::test(start_paused = true)]
async fn nothing_is_captured_without_a_sink_the_host_named() {
    let mut unnamed = sink_state(0.54, false);
    unnamed.target = None;
    for state in [None, Some(unnamed)] {
        let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
        let sink = FakeSink::new(state);
        let capture = FakeCapture::new();
        let mut watching = watching(backend, app, None);
        host_on(&mut watching, &sink, no_bus());
        capture_on(&mut watching, &capture);

        let end = run(watching, quit_after(Duration::from_secs(2))).await;

        assert_eq!(capture.calls(), [], "a capture started with no sink named");
        end.watching.close().await;
    }
}

/// Through the loop, with nothing wanting a level: the capture runs while the device screen
/// draws the Spectrum row, in the tall tier, and stops as another screen shows, as the
/// terminal shrinks to the 80×24 layout and as a box opens over the screen; the row draws
/// the blocks at the loop's fast tick, never a frame a block.
#[tokio::test(start_paused = true)]
async fn the_capture_follows_the_spectrum_row_across_the_screens_and_the_tiers() {
    use crossterm::event::KeyCode;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.tui.title_meter = false;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let capture = FakeCapture::new();
    let held = capture.clone();
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, no_bus());
    capture_on(&mut watching, &capture);
    let pushed = tokio::spawn(async move {
        let mut taken = 0;
        for _ in 0..600 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let full = svolder_host::Block {
                bands: [1.0; svolder_host::fft::BANDS],
                left: 0.5,
                right: 0.5,
            };
            taken += usize::from(held.push(full));
        }
        taken
    });
    let (recorder, frames, size) = Recorder::resizable(80, 45);
    let (smaller, larger) = (Arc::clone(&size), Arc::clone(&size));
    let half = Duration::from_millis(500);
    let script = Box::pin(
        stream::once(async move {
            tokio::time::sleep(half).await;
            key('t')
        })
        .chain(stream::once(async move {
            tokio::time::sleep(half).await;
            press(KeyCode::Esc)
        }))
        .chain(stream::once(async move {
            tokio::time::sleep(half).await;
            *smaller.lock().expect("size mutex") = (80, 24);
            Ok(crossterm::event::Event::Resize(80, 24))
        }))
        .chain(stream::once(async move {
            tokio::time::sleep(half).await;
            *larger.lock().expect("size mutex") = (80, 45);
            Ok(crossterm::event::Event::Resize(80, 45))
        }))
        .chain(quit_after(half)),
    );
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let adds = Adds {
        effects: FakeEffects::healthy(),
    };
    let mut look = look();
    look.tui.title_meter = false;

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &adds,
        watching,
        terminal: &mut terminal,
        events: script,
        mouse: &mut mouse,
        title: &mut title(),
        look,
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    let taken = pushed.await.expect("the blocks were pushed");
    // Started on the device screen, stopped on the telemetry screen, started back on the
    // device screen, stopped at 80×24, started at 80×45 again, and stopped under the box
    // `q` opens.
    assert_eq!(
        capture.calls(),
        [
            usb(),
            CaptureCall::Stop,
            usb(),
            CaptureCall::Stop,
            usb(),
            CaptureCall::Stop
        ]
    );
    let frames = drawn(&frames);
    assert!(
        any_frame(&frames, "Spectrum     ████████████████"),
        "the row never drew a block"
    );
    assert!(
        taken > 200 && frames.len() < taken / 2,
        "{} frames for {taken} blocks",
        frames.len()
    );
    end.watching.close().await;
}

/// The blocks the capture reads reach the app's level and the terminal's title, as the
/// facing pair of the capture's two channels, and draw no frame of their own: three hundred
/// of them over three seconds leave the loop well inside its frame budget.
#[tokio::test(start_paused = true)]
async fn the_captures_blocks_reach_the_title_and_draw_no_frame_of_their_own() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let capture = FakeCapture::new();
    let held = capture.clone();
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, no_bus());
    capture_on(&mut watching, &capture);
    let pushed = tokio::spawn(async move {
        let mut taken = 0;
        for n in 0..300 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            taken += usize::from(held.push(block(0.4, 0.6)));
            if n == 150 {
                // The title is written at most once a period of the wall clock, which the
                // paused clock does not move: that much is let pass for real, once.
                std::thread::sleep(TITLE_PERIOD + Duration::from_millis(50));
            }
        }
        taken
    });
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let (mut title, titles) = spied_title();
    let adds = Adds {
        effects: FakeEffects::healthy(),
    };

    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &adds,
        watching,
        terminal: &mut terminal,
        events: quit_after(Duration::from_millis(3_500)),
        mouse: &mut mouse,
        title: &mut title,
        look: look(),
    })
    .await;

    assert_eq!(end.code, 0, "{:?}", end.why);
    let taken = pushed.await.expect("the blocks were pushed");
    assert!(taken > 250, "the capture took {taken} blocks");
    let drawn = drawn(&frames).len();
    assert!(drawn < taken / 4, "{drawn} frames for {taken} blocks");
    let titles = titles.lock().expect("the titles mutex").clone();
    assert!(
        titles
            .iter()
            .any(|title| title == "(██░░░░░███) svolder · fake"),
        "{titles:?}"
    );
    end.watching.close().await;
}

/// A frame of the daemon with every band at `level`, flagged a beat or not.
fn analysed(level: u8, beat: bool) -> svolder_ipc::host::FrameView {
    svolder_ipc::host::FrameView {
        t: 1,
        left: vec![level; svolder_host::analysis::BANDS],
        right: vec![level; svolder_host::analysis::BANDS],
        levels: [level; 4],
        beat,
        bpm: None,
        phase: 0,
    }
}

/// A frame the daemon sent lands as a block of the loop's own capture does — the bands on the
/// row, the level, the capture running — and asks for no frame of its own to draw; one with a
/// beat pulses what is drawn; one that is not a whole frame lands nothing.
#[test]
fn a_frame_lands_like_a_block_and_a_beat_pulses() {
    let mut link = HostLink::none();
    let mut app = dac();
    let event = |view| Step::Event(Some(Box::new(svolder_ipc::Event::HostFrame(view))));
    assert!(link.framed(event(analysed(255, false)), &mut app));
    assert!(app.host.capturing);
    assert!(app.host.level.is_some_and(|level| level > 0.9));
    assert!(app.host.bands.moving(app.now));
    assert_eq!(app.host.beat, None, "a frame with no beat pulsed");
    assert_eq!(app.pulse(), 0.0);

    assert!(link.framed(event(analysed(255, true)), &mut app));
    assert!(app.host.beat.is_some(), "the beat did not land");
    assert!(app.pulse() > 0.9, "the pulse starts at its full");

    let mut short = analysed(255, true);
    short.left.truncate(10);
    let mut fresh = dac();
    assert!(!link.framed(event(short), &mut fresh));
    assert!(!fresh.host.capturing, "a frame that is not whole landed");
}

/// The capture's states the daemon reports: running marks the capture running, starting
/// changes nothing, unavailable and the end of the connection let it go (nothing it read is
/// left to draw) and bar it, the first for longer than the second.
#[tokio::test]
async fn the_daemons_capture_states_run_and_let_go_of_the_row() {
    let capture = |state| {
        Step::Event(Some(Box::new(svolder_ipc::Event::HostCapture(
            svolder_ipc::host::CaptureView {
                state,
                sink: None,
                why: None,
            },
        ))))
    };
    use svolder_ipc::host::CaptureKind::{Running, Starting, Unavailable};
    let mut link = HostLink::none();
    link.frames = frames::FramesLink::Open(Box::pin(stream::pending()));
    let mut app = dac();
    assert!(!link.framed(capture(Starting), &mut app));
    assert!(!app.host.capturing);
    assert!(!link.framed(capture(Running), &mut app));
    assert!(app.host.capturing);
    assert!(link.frames.active());

    assert!(!link.framed(capture(Unavailable), &mut app));
    assert!(!link.frames.active(), "the connection was kept");
    assert!(!app.host.capturing);
    assert!(matches!(link.barred, frames::Barred::Until(_)));

    link.frames = frames::FramesLink::Open(Box::pin(stream::pending()));
    app.host.capturing = true;
    assert!(!link.framed(Step::Event(None), &mut app));
    assert!(!link.frames.active() && !app.host.capturing);
    assert!(matches!(link.barred, frames::Barred::Until(_)));
}

/// The loop's own capture keeps feeding the row while the daemon's frames connection is opening
/// or open and no frame has landed; the first frame stops it (what it read stays drawn, now
/// replaced by the daemon's), and it starts again as the connection goes.
#[tokio::test]
async fn the_own_capture_runs_until_the_daemons_first_frame() {
    let capture = FakeCapture::new();
    let mut link = HostLink::none().with_capture(Box::new(capture.clone()));
    let mut app = dac();
    link.frames = frames::FramesLink::Open(Box::pin(stream::pending()));
    link.steer(&mut app);
    assert_eq!(
        capture.calls(),
        [usb()],
        "a daemon yet to speak left the row bare"
    );
    assert!(link.capturing());

    let frame = Step::Event(Some(Box::new(svolder_ipc::Event::HostFrame(analysed(
        255, false,
    )))));
    assert!(link.framed(frame, &mut app));
    assert_eq!(
        capture.calls(),
        [usb(), CaptureCall::Stop],
        "both captures fed the row"
    );
    assert!(!link.capturing());
    assert!(
        app.host.capturing,
        "the hand-over cleared what the frame landed"
    );
    assert!(app.host.level.is_some_and(|level| level > 0.9));
    link.steer(&mut app);
    assert_eq!(
        capture.calls(),
        [usb(), CaptureCall::Stop],
        "the own capture came back"
    );

    link.drop_frames(&mut app);
    link.steer(&mut app);
    assert_eq!(capture.calls(), [usb(), CaptureCall::Stop, usb()]);
}

/// A connection still opening (the daemon has not answered) holds nothing of the own capture,
/// and one that fails or is refused leaves it running as it was.
#[tokio::test]
async fn a_connection_still_opening_leaves_the_own_capture_alone() {
    let capture = FakeCapture::new();
    let mut link = HostLink::none().with_capture(Box::new(capture.clone()));
    let mut app = dac();
    link.frames = frames::FramesLink::Opening(Aborting(tokio::spawn(std::future::pending())));
    link.steer(&mut app);
    assert_eq!(capture.calls(), [usb()]);
    assert!(!link.daemon_feeds());

    assert!(!link.framed(Step::Opened(frames::Opened::Mute), &mut app));
    link.steer(&mut app);
    assert_eq!(capture.calls(), [usb()], "the own capture flapped");
    assert!(app.host.capturing);
}

/// A session with no daemon behind it keeps its own capture, and never asks for frames.
#[tokio::test(start_paused = true)]
async fn direct_mode_keeps_its_own_capture() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    assert!(!backend.through_daemon());
    let sink = FakeSink::new(Some(sink_state(0.54, false)));
    let capture = FakeCapture::new();
    let mut watching = watching(backend, app, None);
    host_on(&mut watching, &sink, no_bus());
    capture_on(&mut watching, &capture);

    let held = capture.clone();
    let pushed = tokio::spawn(async move {
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            held.push(block(0.5, 0.5));
        }
    });
    let end = run(watching, quit_after(Duration::from_secs(2))).await;
    pushed.await.expect("the blocks were pushed");

    assert_eq!(capture.calls(), [usb(), CaptureCall::Stop]);
    assert!(!end.watching.host.listening(), "frames were asked for");
    end.watching.close().await;
}

/// The daemon that gave the frames stops speaking for the host (its feed ended, or it says its
/// host side is off): the connection is let go with it, the row's last frame is forgotten, and
/// the loop's own capture is free to run again; another daemon may be asked afresh, whatever the
/// last one refused.
#[tokio::test]
async fn frames_end_with_the_daemon_that_gave_them() {
    let capture = FakeCapture::new();
    let mut link = HostLink::none().with_capture(Box::new(capture.clone()));
    let mut app = dac();
    app.host.daemon = Some(crate::tui::host::FromDaemon { route: None });
    link.follow(&mut app);
    link.frames = frames::FramesLink::Open(Box::pin(stream::pending()));
    link.barred = frames::Barred::Daemon;
    app.host.capturing = true;

    app.host.daemon = None;
    link.follow(&mut app);
    assert!(!link.frames.active(), "the connection outlived the daemon");
    assert!(!app.host.capturing);
    assert!(matches!(link.barred, frames::Barred::No));
    link.steer(&mut app);
    assert_eq!(
        capture.calls(),
        [usb()],
        "the own capture did not take over"
    );
}

/// A bar set for a time keeps until then, however briefly the frames are not wanted: a help
/// overlay or another screen hiding the Spectrum row does not make a daemon whose capture cannot
/// run be asked again at once.
#[tokio::test]
async fn a_bar_for_a_time_survives_the_frames_not_being_wanted() {
    let mut link = HostLink::none();
    let mut app = dac();
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    link.barred = frames::Barred::Until(until);
    link.steer_frames(&mut app, None);
    assert_eq!(link.barred, frames::Barred::Until(until));
}
