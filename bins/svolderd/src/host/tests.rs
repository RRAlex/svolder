// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use svolder_host::fake::FakePlayers;
use svolder_host::route::{PlayerMatch, SinkFacts, StreamFacts};
use svolder_host::{Mpris, NoBus, Progress, Status};

use super::*;
use crate::locks::locked;

/// What a test's players play: a track with a cover address.
fn track(title: &str, art: Option<&str>) -> NowPlaying {
    NowPlaying {
        title: title.to_string(),
        artist: "Miles Davis".to_string(),
        album: "Kind of Blue".to_string(),
        identity: "Fake Player".to_string(),
        status: Status::Playing,
        art_url: art.map(str::to_string),
        length_us: Some(562_000_000),
        progress: Some(Progress {
            position_us: 12_000_000,
            rate_milli: 1000,
            read_at_ms: 1_000,
        }),
        pid: Some(4242),
        ..NowPlaying::default()
    }
}

/// A cover source that has a picture, made of the address itself, for every `file://` address
/// and none for the others.
fn covers() -> CoverSource {
    Arc::new(|url| {
        url.starts_with("file://").then(|| Cover {
            bytes: url.as_bytes().to_vec(),
            mime: "image/png",
        })
    })
}

/// A route reader that counts its reads and answers what the test last set.
#[derive(Clone, Default)]
struct Counting {
    reads: Arc<AtomicUsize>,
    sink: Arc<Mutex<String>>,
}

impl Counting {
    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    fn sink_is(&self, name: &str) {
        *self.sink.lock().unwrap() = name.to_string();
    }
}

#[async_trait]
impl Routes for Counting {
    async fn read(&self, player: Option<NowPlaying>) -> Option<RouteFacts> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Some(RouteFacts {
            sink: Some(SinkFacts {
                name: self.sink.lock().unwrap().clone(),
                ..SinkFacts::default()
            }),
            player: player.map(|_| StreamFacts::default()),
            player_match: PlayerMatch::Pid,
            ..RouteFacts::default()
        })
    }
}

/// A service following `players`, reading routes through `routes` and covers through `covers`.
fn service(players: &FakePlayers, routes: Option<&Counting>, covers: CoverSource) -> HostService {
    let service = HostService::idle();
    let players: Arc<dyn Players> = Arc::new(players.clone());
    service.run_with(Parts {
        connect: found(players),
        routes: routes.map(|routes| Arc::new(routes.clone()) as Arc<dyn Routes>),
        covers,
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    service
}

/// Let every task that can run, run, and give the blocking pool (where covers are read) real
/// time to answer; the paused clock does not move meanwhile.
pub(super) async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Move the paused clock on by `by`, and settle.
pub(super) async fn pass(by: Duration) {
    tokio::time::advance(by).await;
    settle().await;
}

#[tokio::test(start_paused = true)]
async fn the_view_carries_the_playing_track() {
    let players = FakePlayers::new(Some(track("So What", Some("file:///cover/a.png"))));
    let service = service(&players, None, covers());
    settle().await;

    let view = service.view();
    assert!(view.available);
    assert!(view.now_ms > 0, "the view carries the daemon's clock");
    let playing = view.playing.expect("a player");
    assert_eq!(playing.title, "So What");
    assert_eq!(playing.artist, "Miles Davis");
    assert_eq!(playing.album, "Kind of Blue");
    assert_eq!(playing.player, "Fake Player");
    assert_eq!(playing.status, "playing");
    assert_eq!(playing.length_us, Some(562_000_000));
    assert_eq!(playing.progress.map(|p| p.position_us), Some(12_000_000));
    let hash = playing.cover.expect("a cover was read");
    assert_eq!(hash.len(), 16);
    assert!(
        hash.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    let cover = service.cover(&hash).expect("the current cover");
    assert_eq!(cover.bytes, b"file:///cover/a.png");
    assert_eq!(service.cover("0000000000000000"), None);
}

/// The hash is the first 16 hex characters of the cover's SHA-256.
#[tokio::test(start_paused = true)]
async fn the_cover_hash_is_the_sha_256_of_the_picture() {
    let players = FakePlayers::new(Some(track("So What", Some("file:///cover/a.png"))));
    let service = service(&players, None, covers());
    settle().await;
    let hash = service
        .view()
        .playing
        .and_then(|p| p.cover)
        .expect("a cover");
    let digest = ring::digest::digest(&ring::digest::SHA256, b"file:///cover/a.png");
    let expected: String = digest.as_ref()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hash, expected);
}

#[tokio::test(start_paused = true)]
async fn the_cover_hash_changes_with_the_art() {
    let players = FakePlayers::new(Some(track("So What", Some("file:///cover/a.png"))));
    let service = service(&players, None, covers());
    settle().await;
    let first = service
        .view()
        .playing
        .and_then(|p| p.cover)
        .expect("a cover");

    players.set(Some(track(
        "Freddie Freeloader",
        Some("file:///cover/b.png"),
    )));
    settle().await;
    let second = service
        .view()
        .playing
        .and_then(|p| p.cover)
        .expect("a cover");
    assert_ne!(first, second);
    assert_eq!(
        service.cover(&second).expect("served").bytes,
        b"file:///cover/b.png"
    );
    assert_eq!(
        service.cover(&first),
        None,
        "only the current cover is served"
    );

    // An address that is no cover leaves the view with none, and nothing to serve.
    players.set(Some(track(
        "Blue in Green",
        Some("http://example.org/a.png"),
    )));
    settle().await;
    assert_eq!(service.view().playing.expect("a player").cover, None);
    assert_eq!(service.cover(&second), None);
}

/// A cover source that has a picture from its `n`th ask on, counting its asks.
fn cover_from_ask(n: usize) -> (CoverSource, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&asked);
    let source: CoverSource = Arc::new(move |_| {
        (counted.fetch_add(1, Ordering::SeqCst) + 1 >= n).then(|| Cover {
            bytes: vec![1, 2, 3],
            mime: "image/png",
        })
    });
    (source, asked)
}

/// A cover that was not there at first is read again 1 s after the first read, so a picture a
/// player writes a moment after it names it is picked up; a player change in between does not
/// read it sooner.
#[tokio::test(start_paused = true)]
async fn a_cover_that_was_missing_is_read_again_a_second_later() {
    let (late, asked) = cover_from_ask(2);
    let players = FakePlayers::new(Some(track("So What", Some("file:///cover/a.png"))));
    let service = service(&players, None, late);
    settle().await;
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert_eq!(service.view().playing.expect("a player").cover, None);

    let mut paused = track("So What", Some("file:///cover/a.png"));
    paused.status = Status::Paused;
    players.set(Some(paused));
    pass(Duration::from_millis(500)).await;
    assert_eq!(
        asked.load(Ordering::SeqCst),
        1,
        "a player change reads nothing sooner"
    );

    pass(Duration::from_millis(600)).await;
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    assert!(service.view().playing.expect("a player").cover.is_some());
}

/// A picture that is still missing 1 s later is read once more 3 s after the first read, and
/// then never again.
#[tokio::test(start_paused = true)]
async fn a_cover_is_read_three_times_at_most() {
    let (never, asked) = cover_from_ask(usize::MAX);
    let players = FakePlayers::new(Some(track("So What", Some("file:///cover/a.png"))));
    let service = service(&players, None, never);
    settle().await;
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    pass(Duration::from_millis(1_100)).await;
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    pass(Duration::from_millis(1_800)).await;
    assert_eq!(
        asked.load(Ordering::SeqCst),
        2,
        "2.9 s after the first read"
    );
    pass(Duration::from_millis(200)).await;
    assert_eq!(asked.load(Ordering::SeqCst), 3, "3.1 s after it");
    pass(Duration::from_secs(60)).await;
    assert_eq!(asked.load(Ordering::SeqCst), 3);
    assert_eq!(service.view().playing.expect("a player").cover, None);
}

/// The cover is read off the task that follows the player: a source that blocks leaves the
/// player's changes published meanwhile.
#[tokio::test(start_paused = true)]
async fn a_slow_cover_read_does_not_hold_up_the_player() {
    let (release, hold) = std::sync::mpsc::channel::<()>();
    let hold = Mutex::new(hold);
    let slow: CoverSource = Arc::new(move |_| {
        let _ = hold.lock().unwrap().recv();
        None
    });
    let players = FakePlayers::new(Some(track("So What", Some("file:///cover/a.png"))));
    let service = service(&players, None, slow);
    settle().await;

    players.set(Some(track("Freddie Freeloader", None)));
    settle().await;
    assert_eq!(
        service.view().playing.expect("a player").title,
        "Freddie Freeloader",
        "the new track is published while the old cover read is stuck"
    );
    release.send(()).unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_press_reaches_the_player() {
    let players = FakePlayers::new(Some(track("So What", None))).answering();
    let service = service(&players, None, covers());
    settle().await;

    assert!(service.press(Button::Pause).await);
    settle().await;
    assert_eq!(players.presses(), [Button::Pause]);
    assert_eq!(service.view().playing.expect("a player").status, "paused");

    assert!(service.press(Button::Play).await);
    settle().await;
    assert_eq!(service.view().playing.expect("a player").status, "playing");
}

/// A press with no player to take it is refused.
#[tokio::test(start_paused = true)]
async fn a_press_with_no_player_is_refused() {
    let players = FakePlayers::new(None);
    let service = service(&players, None, covers());
    settle().await;
    assert!(!service.press(Button::Next).await);
}

/// The player quits mid-track, and the view goes quietly to no player, with
/// no title, no cover to serve and the host still there.
#[tokio::test(start_paused = true)]
async fn a_lost_player_empties_the_view() {
    let players = FakePlayers::new(Some(track("So What", Some("file:///cover/a.png"))));
    let service = service(&players, None, covers());
    settle().await;
    let hash = service
        .view()
        .playing
        .and_then(|p| p.cover)
        .expect("a cover");

    players.set(None);
    settle().await;
    let view = service.view();
    assert!(view.available, "the host is still watched");
    assert_eq!(view.playing, None);
    assert_eq!(service.cover(&hash), None, "no stale picture is served");
}

/// The bus going away (the stream of changes ends) makes the host unavailable.
#[tokio::test(start_paused = true)]
async fn a_lost_bus_reports_unavailable() {
    let players = FakePlayers::new(Some(track("So What", None)));
    let service = service(&players, None, covers());
    settle().await;
    assert!(service.view().available);

    players.lose();
    settle().await;
    let view = service.view();
    assert!(!view.available);
    assert_eq!(view.playing, None);
    assert!(!service.press(Button::Pause).await);
}

/// `SVOLDER_HOST=off` (the mode it makes) leaves no host at all.
#[tokio::test(start_paused = true)]
async fn host_off_reports_unavailable() {
    let service = HostService::start(&HostConfig::default(), HostMode::Off);
    settle().await;
    let view = service.view();
    assert!(!view.available);
    assert_eq!(view.playing, None);
    assert_eq!(view.route, None);
    assert!(!service.press(Button::Play).await);
}

/// `[host] enabled = false` is as off as the environment, even in fake mode.
#[tokio::test(start_paused = true)]
async fn a_disabled_section_reports_unavailable() {
    let service = HostService::start(&HostConfig { enabled: false }, HostMode::Fake);
    settle().await;
    assert!(!service.view().available);
}

/// A bus that cannot be reached is no host, reached the way the real one is.
#[tokio::test(start_paused = true)]
async fn an_unreachable_bus_reports_unavailable() {
    let service = HostService::idle();
    service.run_with(Parts {
        connect: Arc::new(|| {
            Box::pin(async {
                Mpris::look("unix:path=/nonexistent/svolderd-test-bus")
                    .await
                    .map(|mpris| Arc::new(mpris) as Arc<dyn Players>)
            })
        }),
        routes: None,
        covers: covers(),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(!service.view().available);
}

/// No read while nobody watches; while someone does, one at once and then at most one
/// every two seconds, whatever the player does in between.
#[tokio::test(start_paused = true)]
async fn routes_are_read_only_while_watched_and_at_most_every_2_s() {
    let players = FakePlayers::new(Some(track("So What", None)));
    let routes = Counting::default();
    routes.sink_is("dac");
    let service = service(&players, Some(&routes), covers());
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(routes.reads(), 0, "nobody watches");
    assert_eq!(service.view().route, None);

    let watcher = service.watching();
    settle().await;
    assert_eq!(routes.reads(), 1, "the first look comes at once");
    let route = service.view().route.expect("a route");
    assert_eq!(route.sink.expect("a sink").name, "dac");

    tokio::time::sleep(Duration::from_millis(500)).await;
    players.set(Some(track("Freddie Freeloader", None)));
    settle().await;
    assert_eq!(
        routes.reads(),
        1,
        "a player change inside two seconds waits"
    );

    tokio::time::sleep(Duration::from_millis(1_400)).await;
    assert_eq!(routes.reads(), 1, "1.9 s after the first read");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(routes.reads(), 2, "2.1 s after it");

    tokio::time::sleep(Duration::from_secs(10)).await;
    let reads = routes.reads();
    assert!((6..=8).contains(&reads), "{reads} reads in about 12 s");

    drop(watcher);
    settle().await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(routes.reads(), reads, "nobody watches again");
    assert_eq!(service.view().route, None, "no stale route is kept");
}

/// Two watchers read once, and the last to go stops the reads.
#[tokio::test(start_paused = true)]
async fn watchers_share_the_reads() {
    let players = FakePlayers::new(Some(track("So What", None)));
    let routes = Counting::default();
    let service = service(&players, Some(&routes), covers());
    let (first, second) = (service.watching(), service.watching());
    settle().await;
    assert_eq!(routes.reads(), 1);
    drop(first);
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    assert_eq!(routes.reads(), 2, "one watcher is enough");
    drop(second);
    settle().await;
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(routes.reads(), 2);
}

/// The default sink changes mid-play, and the next read says so, within two
/// seconds, to whoever listens for changes.
#[tokio::test(start_paused = true)]
async fn a_sink_change_rereads_the_route() {
    let players = FakePlayers::new(Some(track("So What", None)));
    let routes = Counting::default();
    routes.sink_is("alsa_output.usb-dac");
    let service = service(&players, Some(&routes), covers());
    let mut changes = service.changes();
    let _watcher = service.watching();
    settle().await;
    changes.borrow_and_update();
    let sink = |view: &HostView| view.route.as_ref()?.sink.as_ref().map(|s| s.name.clone());
    assert_eq!(
        sink(&service.view()).as_deref(),
        Some("alsa_output.usb-dac")
    );

    routes.sink_is("alsa_output.hdmi");
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    changes.changed().await.expect("the service is up");
    assert_eq!(sink(&changes.borrow()).as_deref(), Some("alsa_output.hdmi"));
}

/// A read that finds nothing (no `pw-dump`) leaves no route.
#[tokio::test(start_paused = true)]
async fn a_route_that_cannot_be_read_is_no_route() {
    struct Nothing;
    #[async_trait]
    impl Routes for Nothing {
        async fn read(&self, _player: Option<NowPlaying>) -> Option<RouteFacts> {
            None
        }
    }
    let players = FakePlayers::new(Some(track("So What", None)));
    let service = HostService::idle();
    let shared: Arc<dyn Players> = Arc::new(players);
    service.run_with(Parts {
        connect: found(shared),
        routes: Some(Arc::new(Nothing)),
        covers: covers(),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    let _watcher = service.watching();
    tokio::time::sleep(Duration::from_secs(5)).await;
    let view = service.view();
    assert!(view.available);
    assert_eq!(view.route, None);
}

/// The fake mode: a fixed track with the bundled cover, a route on a Schiit sink that reads
/// untouched, and presses the player answers.
#[tokio::test(start_paused = true)]
async fn the_fake_host_plays_a_fixed_track_with_its_cover_and_route() {
    let service = HostService::start(&HostConfig::default(), HostMode::Fake);
    let _watcher = service.watching();
    settle().await;
    let view = service.view();
    assert!(view.available);
    let playing = view.playing.expect("the fake player");
    assert_eq!(playing.status, "playing");
    assert!(!playing.title.is_empty() && !playing.player.is_empty());
    assert!(playing.length_us.is_some() && playing.progress.is_some());
    let cover = service
        .cover(&playing.cover.expect("the bundled cover"))
        .expect("served");
    assert_eq!(cover.bytes, BUNDLED_COVER);
    assert_eq!(cover.mime, "image/png");

    let route = view.route.expect("the fake route");
    let sink = route.sink.expect("a sink");
    assert_eq!(
        sink.usb_vendor_id,
        Some(svolder_ipc::host::SCHIIT_VENDOR_ID)
    );
    assert!(svolder_ipc::host::sink_is_dac(
        &sink,
        &svolder_ipc::host::DacFacts {
            display: "Gungnir 2".to_string(),
            input: None,
            rate_hz: sink.rate_hz,
            usb_ports: None,
        }
    ));
    assert_eq!(
        sink.rate_hz,
        Some(svolder_fake::SAMPLE_RATE_HZ),
        "the rate the fake device reads"
    );
    assert_eq!(route.player_match, PlayerMatch::Pid);
    assert_eq!(route.player.expect("a stream").rate_hz, sink.rate_hz);
    assert_eq!((route.others_mixing, route.host_eq), (0, false));

    assert!(service.press(Button::Pause).await);
    settle().await;
    assert_eq!(service.view().playing.expect("the player").status, "paused");
}

/// The fake player quits, or loses its art, this long after the host starts.
#[tokio::test(start_paused = true)]
async fn the_fake_host_can_lose_its_cover_and_then_its_player() {
    let knobs = FakeKnobs {
        cover_gone_after: Some(Duration::from_secs(3)),
        player_gone_after: Some(Duration::from_secs(6)),
        ..FakeKnobs::default()
    };
    let service = HostService::idle();
    service.run_with(Parts::fake(knobs));
    settle().await;
    assert!(service.view().playing.expect("a player").cover.is_some());

    tokio::time::sleep(Duration::from_secs(4)).await;
    let playing = service.view().playing.expect("still playing");
    assert_eq!(playing.cover, None, "the art was dropped");
    assert!(!playing.title.is_empty(), "the track stays");

    tokio::time::sleep(Duration::from_secs(3)).await;
    let view = service.view();
    assert!(view.available);
    assert_eq!(view.playing, None, "the player quit");
}

/// The knobs are plain milliseconds; anything else is unset.
#[test]
fn the_fake_knobs_read_milliseconds() {
    let knobs = FakeKnobs::from_knobs(|name| match name {
        "SVOLDER_FAKE_COVER_GONE_AFTER_MS" => Some("1500".to_string()),
        "SVOLDER_FAKE_PLAYER_GONE_AFTER_MS" => Some("soon".to_string()),
        _ => None,
    });
    assert_eq!(knobs.cover_gone_after, Some(Duration::from_millis(1500)));
    assert_eq!(knobs.player_gone_after, None);
    assert!(!knobs.route_changed);
    for (value, changed) in [
        ("changed", true),
        (" Changed ", true),
        ("", false),
        ("x", false),
    ] {
        let knobs =
            FakeKnobs::from_knobs(|name| (name == "SVOLDER_FAKE_ROUTE").then(|| value.to_string()));
        assert_eq!(knobs.route_changed, changed, "{value:?}");
    }
}

/// `SVOLDER_FAKE_TITLE` names the fake track; empty or unset leaves the fixed title.
#[test]
fn the_fake_title_knob_replaces_the_fixed_title() {
    let title = |value: Option<&str>| {
        FakeKnobs::from_knobs(|name| {
            (name == "SVOLDER_FAKE_TITLE")
                .then(|| value.map(str::to_string))
                .flatten()
        })
        .title
    };
    assert_eq!(title(None), None);
    assert_eq!(title(Some("  ")), None);
    assert_eq!(
        title(Some(" A Title Too Long For A Phone ")).as_deref(),
        Some("A Title Too Long For A Phone")
    );
    let players = fake_players(Some("A Title Too Long For A Phone"));
    let now = players.now_playing().expect("a track");
    assert_eq!(now.title, "A Title Too Long For A Phone");
    assert_eq!(
        fake_players(None).now_playing().expect("a track").title,
        "Test Pressing"
    );
}

/// The changed fake route alters the signal in every way a route can, and names each in the
/// verdict the page shows for the fake DAC.
#[tokio::test]
async fn the_changed_fake_route_changes_everything() {
    let route = FakeRoute {
        display: "Gungnir 2".to_string(),
        changed: true,
    };
    let facts = route.read(Some(track("x", None))).await.expect("a route");
    let dac = svolder_ipc::host::DacFacts {
        display: "Gungnir 2".to_string(),
        input: None,
        rate_hz: Some(FAKE_RATE_HZ),
        usb_ports: None,
    };
    let line = svolder_ipc::host::path_line(&facts, &dac);
    assert_eq!(line.verdict, svolder_ipc::host::Verdict::Changed);
    assert_eq!(
        line.words,
        [
            "player volume 91 %",
            "resampled 44.1 → 48 kHz",
            "computer volume 54 %",
            "host EQ on",
            "2 other sounds mixing"
        ]
    );
}

#[test]
fn the_mode_follows_svolder_host() {
    for (value, mode) in [
        (None, HostMode::Real),
        (Some(""), HostMode::Real),
        (Some("off"), HostMode::Off),
        (Some(" OFF "), HostMode::Off),
        (Some("fake"), HostMode::Fake),
        (Some("Fake"), HostMode::Fake),
        (Some("on"), HostMode::Real),
    ] {
        let value = value.map(std::ffi::OsStr::new);
        assert_eq!(HostMode::of(value), mode, "{value:?}");
    }
}

/// A reconfiguration to the same thing leaves a running host alone; to another, restarts it
/// on the same channel its listeners already hold.
#[tokio::test(start_paused = true)]
async fn reconfiguring_keeps_the_listeners_channel() {
    let service = HostService::start(&HostConfig::default(), HostMode::Fake);
    let mut changes = service.changes();
    settle().await;
    assert!(changes.borrow_and_update().available);

    service.reconfigure(&HostConfig { enabled: false }, HostMode::Fake);
    settle().await;
    assert!(changes.has_changed().unwrap());
    assert!(!changes.borrow_and_update().available);

    service.reconfigure(&HostConfig::default(), HostMode::Fake);
    settle().await;
    assert!(changes.has_changed().unwrap());
    assert!(changes.borrow_and_update().available);

    // Nothing changed: no restart, so no flicker.
    service.reconfigure(&HostConfig::default(), HostMode::Fake);
    settle().await;
    assert!(!changes.has_changed().unwrap());

    service.stop();
    settle().await;
    assert!(!service.view().available);
    service.reconfigure(&HostConfig::default(), HostMode::Fake);
    settle().await;
    assert!(!service.view().available, "a stopped host stays stopped");
}

/// The player quits while someone watches and a route is held: the route stops naming its
/// stream at once, without waiting for the next read.
#[tokio::test(start_paused = true)]
async fn a_lost_player_leaves_no_stream_in_the_route() {
    let players = FakePlayers::new(Some(track("So What", None)));
    let routes = Counting::default();
    routes.sink_is("dac");
    let service = service(&players, Some(&routes), covers());
    let _watcher = service.watching();
    settle().await;
    let route = service.view().route.expect("a route");
    assert!(route.player.is_some());
    assert_eq!(route.player_match, PlayerMatch::Pid);

    players.set(None);
    settle().await;
    assert_eq!(routes.reads(), 1, "no read was waited for");
    let view = service.view();
    assert_eq!(view.playing, None);
    let route = view.route.expect("the route stays");
    assert_eq!(route.sink.expect("the sink stays").name, "dac");
    assert_eq!(route.player, None);
    assert_eq!(route.player_match, PlayerMatch::NoPlayer);
}

/// A route reader that never answers and says when it is dropped.
struct Hanging(Arc<std::sync::atomic::AtomicBool>);

struct SetOnDrop(Arc<std::sync::atomic::AtomicBool>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl Routes for Hanging {
    async fn read(&self, _player: Option<NowPlaying>) -> Option<RouteFacts> {
        let _dropped = SetOnDrop(Arc::clone(&self.0));
        std::future::pending().await
    }
}

/// Stopping the host ends a route read that is under way and the tasks that run beside it.
#[tokio::test(start_paused = true)]
async fn stop_ends_a_route_read_and_the_helper_tasks() {
    let read_dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let helper_dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let guard = SetOnDrop(Arc::clone(&helper_dropped));
    let players: Arc<dyn Players> = Arc::new(FakePlayers::new(Some(track("So What", None))));
    let service = HostService::idle();
    service.run_with(Parts {
        connect: found(players),
        routes: Some(Arc::new(Hanging(Arc::clone(&read_dropped)))),
        covers: covers(),
        helpers: vec![tokio::spawn(async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        })],
        frames: Err("no capture in this test".to_string()),
    });
    let _watcher = service.watching();
    settle().await;
    assert!(
        !read_dropped.load(Ordering::SeqCst),
        "the read is under way"
    );
    assert!(!helper_dropped.load(Ordering::SeqCst));

    service.stop();
    settle().await;
    assert!(read_dropped.load(Ordering::SeqCst), "the read was ended");
    assert!(
        helper_dropped.load(Ordering::SeqCst),
        "the helper was ended"
    );
    assert!(!service.view().available);
}

/// A task of a run that was halted changes nothing, even when it is not ended: it neither takes
/// the players nor publishes an available view afterwards.
#[tokio::test(start_paused = true)]
async fn a_halted_run_can_no_longer_publish() {
    let service = HostService::idle();
    let shared = Arc::clone(&service.shared);
    let generation = *locked(&shared.generation);
    let players = FakePlayers::new(Some(track("So What", Some("file:///cover/a.png"))));
    let following: Arc<dyn Players> = Arc::new(players.clone());

    // One that connects after the halt never takes the players.
    let late = tokio::spawn(follow(
        Arc::clone(&shared),
        generation,
        Arc::new(move || {
            let following = Arc::clone(&following);
            Box::pin(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok(following)
            })
        }),
        None,
        covers(),
    ));
    // One already following, which the halt does not end.
    let running: Arc<dyn Players> = Arc::new(players.clone());
    let early = tokio::spawn(follow(
        Arc::clone(&shared),
        generation,
        found(running),
        None,
        covers(),
    ));
    settle().await;
    assert!(service.view().available, "the early run published");

    service.stop();
    assert!(!service.view().available);
    players.set(Some(track(
        "Freddie Freeloader",
        Some("file:///cover/b.png"),
    )));
    pass(Duration::from_secs(2)).await;
    let view = service.view();
    assert!(!view.available, "a halted run published {view:?}");
    assert!(
        locked(&shared.players).is_none(),
        "a halted run took the players"
    );
    assert!(locked(&shared.cover).is_none(), "a halted run left a cover");
    early.abort();
    late.abort();
}

/// Players on a bus that goes away when told to: the stream of changes ends then.
struct OnABus {
    now: NowPlaying,
    gone: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

#[async_trait]
impl Players for OnABus {
    fn now_playing(&self) -> Option<NowPlaying> {
        Some(self.now.clone())
    }

    fn changes(&self) -> svolder_host::players::Changes {
        let gone = locked(&self.gone).take().expect("followed once");
        futures::stream::pending::<Option<NowPlaying>>()
            .take_until(gone)
            .boxed()
    }

    async fn press(&self, _button: Button) -> bool {
        true
    }
}

/// A session bus that goes away is looked for again every 10 s, as often as it takes, and the
/// host is available again with what the players on the new bus play; one that is never lost
/// is not looked for again.
#[tokio::test(start_paused = true)]
async fn a_bus_that_goes_away_is_looked_for_again_every_10_s() {
    let (lose, gone) = tokio::sync::oneshot::channel();
    let first: Arc<dyn Players> = Arc::new(OnABus {
        now: track("So What", None),
        gone: Mutex::new(Some(gone)),
    });
    let (_stays, never) = tokio::sync::oneshot::channel();
    let second: Arc<dyn Players> = Arc::new(OnABus {
        now: track("Blue in Green", None),
        gone: Mutex::new(Some(never)),
    });
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connector = Arc::new(move || {
        let attempt = counted.fetch_add(1, Ordering::SeqCst);
        // The bus is back on the third look, not the second.
        let found = match attempt {
            0 => Ok(Arc::clone(&first)),
            2 => Ok(Arc::clone(&second)),
            _ => Err(NoBus::Absent),
        };
        Box::pin(async move { found })
    });
    let service = HostService::idle();
    service.run_with(Parts {
        connect,
        routes: None,
        covers: covers(),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    settle().await;
    assert_eq!(title_of(&service), Some("So What".to_string()));
    pass(Duration::from_secs(30)).await;
    assert_eq!(
        looked.load(Ordering::SeqCst),
        1,
        "a bus still there was looked for"
    );

    lose.send(()).expect("the bus is followed");
    settle().await;
    assert!(
        !service.view().available,
        "the lost bus left the host available"
    );
    assert!(
        !service.press(Button::Play).await,
        "a press went to the lost bus"
    );

    pass(Duration::from_secs(9)).await;
    assert_eq!(looked.load(Ordering::SeqCst), 1, "looked for before 10 s");
    pass(Duration::from_secs(2)).await;
    assert_eq!(looked.load(Ordering::SeqCst), 2);
    assert!(
        !service.view().available,
        "a look that found none made it available"
    );

    pass(Duration::from_secs(10)).await;
    assert_eq!(looked.load(Ordering::SeqCst), 3);
    assert_eq!(title_of(&service), Some("Blue in Green".to_string()));
    assert!(
        service.press(Button::Play).await,
        "the new bus took no press"
    );
    pass(Duration::from_secs(60)).await;
    assert_eq!(
        looked.load(Ordering::SeqCst),
        3,
        "a bus that is back was looked for"
    );
}

/// The title the host's view carries, if it has a track.
fn title_of(service: &HostService) -> Option<String> {
    service.view().playing.map(|playing| playing.title)
}

/// The look for a lost bus ends with the host: stopped while it waits for the next look, nothing
/// is looked for again.
#[tokio::test(start_paused = true)]
async fn a_stopped_host_does_not_look_for_the_lost_bus() {
    let (lose, gone) = tokio::sync::oneshot::channel();
    let first: Arc<dyn Players> = Arc::new(OnABus {
        now: track("So What", None),
        gone: Mutex::new(Some(gone)),
    });
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connector = Arc::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        let first = Arc::clone(&first);
        Box::pin(async move { Ok(first) })
    });
    let service = HostService::idle();
    service.run_with(Parts {
        connect,
        routes: None,
        covers: covers(),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    settle().await;
    lose.send(()).expect("the bus is followed");
    pass(Duration::from_secs(5)).await;
    service.stop();
    pass(Duration::from_secs(60)).await;
    assert_eq!(
        looked.load(Ordering::SeqCst),
        1,
        "a stopped host looked for the bus"
    );
    assert!(!service.view().available);
}

/// A first look that finds a bus that took the connection and has not answered (or one that an
/// earlier, abandoned look still waits on) is looked for again every ten seconds, like a lost
/// bus, until it answers: the host comes up by itself.
#[tokio::test(start_paused = true)]
async fn a_bus_that_has_not_answered_yet_is_looked_for_again() {
    let players: Arc<dyn Players> = Arc::new(FakePlayers::new(Some(track("So What", None))));
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connector = Arc::new(move || {
        let attempt = counted.fetch_add(1, Ordering::SeqCst);
        let players = Arc::clone(&players);
        // Silent on the first two looks, answering on the third.
        Box::pin(async move {
            match attempt {
                0 | 1 => Err(NoBus::Silent),
                _ => Ok(players),
            }
        })
    });
    let service = HostService::idle();
    service.run_with(Parts {
        connect,
        routes: None,
        covers: covers(),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    settle().await;
    assert!(!service.view().available, "a silent bus made it available");
    pass(Duration::from_secs(9)).await;
    assert_eq!(looked.load(Ordering::SeqCst), 1, "looked for before 10 s");
    pass(Duration::from_secs(2)).await;
    assert_eq!(looked.load(Ordering::SeqCst), 2);
    pass(Duration::from_secs(10)).await;
    assert_eq!(looked.load(Ordering::SeqCst), 3);
    assert_eq!(title_of(&service), Some("So What".to_string()));
    pass(Duration::from_secs(60)).await;
    assert_eq!(
        looked.load(Ordering::SeqCst),
        3,
        "a bus that answered was looked for"
    );
}

/// A first look that finds no bus at all is not repeated: a bus that starts later is left to the
/// next reconfiguration.
#[tokio::test(start_paused = true)]
async fn a_bus_that_is_not_there_is_not_looked_for_again() {
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connector = Arc::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(NoBus::Absent) })
    });
    let service = HostService::idle();
    service.run_with(Parts {
        connect,
        routes: None,
        covers: covers(),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    pass(Duration::from_secs(60)).await;
    assert_eq!(looked.load(Ordering::SeqCst), 1);
    assert!(!service.view().available);
}

/// A bus that took the connection and then, on a later look, is not there (its socket went while
/// the host waited) is still looked for: a bus once seen is not given up.
#[tokio::test(start_paused = true)]
async fn a_silent_bus_that_then_looks_absent_is_still_looked_for() {
    let players: Arc<dyn Players> = Arc::new(FakePlayers::new(Some(track("So What", None))));
    let looked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&looked);
    let connect: Connector = Arc::new(move || {
        let attempt = counted.fetch_add(1, Ordering::SeqCst);
        let players = Arc::clone(&players);
        Box::pin(async move {
            match attempt {
                0 => Err(NoBus::Silent),
                1 | 2 => Err(NoBus::Absent),
                _ => Ok(players),
            }
        })
    });
    let service = HostService::idle();
    service.run_with(Parts {
        connect,
        routes: None,
        covers: covers(),
        helpers: Vec::new(),
        frames: Err("no capture in this test".to_string()),
    });
    for _ in 0..4 {
        pass(Duration::from_secs(11)).await;
    }
    assert_eq!(looked.load(Ordering::SeqCst), 4, "the looks went on");
    assert_eq!(title_of(&service), Some("So What".to_string()));
}
