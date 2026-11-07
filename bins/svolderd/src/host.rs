// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's host service: the computer's music player, its cover, the transport presses and
//! the route the sound takes to the default sink, kept once for every front end.
//!
//! One task follows the players on the session bus (through [`Players`]) for as long as
//! `[host]` is enabled; what it learns is published as a [`HostView`] on a watch channel that
//! the socket's `Host` request, every `Subscribe` stream that asks for the host and the web UI
//! read. The picture is read for each new cover address and kept as the one current cover,
//! named by the first 16 hex characters of its SHA-256 and served only under that name.
//!
//! The route is read out of `pw-dump` only while someone holds a [`Watcher`] (a `Subscribe`
//! connection that asked for the host, the event stream of a DAC's web page): at once for the
//! first, then every two seconds, and never two reads closer than that, so a player change or a
//! sink switch shows up on the next read. With nobody watching the daemon does no work for the
//! route and keeps none.
//!
//! The capture of what the computer plays is kept here too, once for every front end: while
//! any [`FramesHold`] exists (the frames stream of a web page, the frames connection of a
//! joined TUI) one capture of the default sink runs and its analysed frames go out on a
//! broadcast channel; 2 s after the last hold is let go it stops, and with nobody holding the
//! daemon captures and analyses nothing (see [`capture`]).
//!
//! [`HostMode`] says where the sources come from: the real session bus and PipeWire, a fake
//! player with a fake route and the bundled cover (for demos and checks), or nothing at all. A
//! service that cannot reach a bus, or is switched off, reports `available = false` and nothing
//! else. [`HostService::reconfigure`] swaps what runs behind the same channel, so a stream that
//! is open when the config changes hears the change.

use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use ring::digest;
use svolder_host::cover::{self, Cover, CoverPlaces};
use svolder_host::eq::{EqControl, Keep};
use svolder_host::fake::FakePlayers;
use svolder_host::fake::{FakeChain, FakeParams, FakeSink};
use svolder_host::route::{PlayerMatch, PwDump, SinkFacts, StreamFacts, route};
use svolder_host::{
    Button, Frame, FrameCapture, Mpris, NoBus, NowPlaying, Players, Progress, PwFrames, RouteFacts,
    Sink, SinkState, Status, Wpctl, unix_ms,
};
use svolder_ipc::config::HostConfig;
use svolder_ipc::host::{
    CaptureKind, CaptureView, HostView, HostVolume, PlayingView, SCHIIT_VENDOR_ID,
};
use svolder_ipc::{Event, HostEqOp};
use svolder_profiles::tone::preset::Preset;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_stream::wrappers::WatchStream;

use crate::locks::locked;

mod capture;
mod computer;
mod synthetic;

pub(crate) use computer::{Computer, EqRefusal, is_unavailable};

/// How long after the bus went away it is looked for again, and after each look that found
/// none.
const BUS_RETRY: Duration = Duration::from_secs(10);

/// The least time between two route reads while someone watches.
const ROUTE_EVERY: Duration = Duration::from_secs(2);

/// How many times a cover address is read in all: a player may name a picture a moment before
/// it has written it, so a miss is followed by a second read 1 s after the first and a third
/// 3 s after it.
const COVER_TRIES: u8 = 3;

/// When the reads after the first are due, from the first.
const COVER_RETRIES: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];

/// The cover the fake player names; the fake cover source answers it with the bundled picture.
const FAKE_ART_URL: &str = "file:///svolder-fake/cover.png";

/// The node the fake route's sink and the fake capture are on.
const FAKE_SINK_NODE: &str = "alsa_output.usb-Schiit_Audio_Fake-00.analog-stereo";

/// The picture the fake player's cover is.
const BUNDLED_COVER: &[u8] = include_bytes!("../web/icon-192.png");

/// How many frames a page may fall behind before the oldest are skipped: about a quarter of a
/// second at 30 frames a second.
const FRAMES_BACKLOG: usize = 8;

/// The sample rate the fake route's sink and the fake player's stream run at: the one the fake
/// device reads, so the path on the fake DAC reads untouched.
const FAKE_RATE_HZ: u32 = svolder_fake::SAMPLE_RATE_HZ;

/// Where the host's sources come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostMode {
    /// The session bus and PipeWire's tools.
    Real,
    /// A fixed fake player, a fake route and the bundled cover.
    Fake,
    /// No host at all.
    Off,
}

impl HostMode {
    /// The mode `SVOLDER_HOST` says for this process.
    pub(crate) fn from_env() -> HostMode {
        Self::of(std::env::var_os(svolder_host::HOST_ENV).as_deref())
    }

    /// The mode a value of `SVOLDER_HOST` says: `off` is off, `fake` is fake (both in any case,
    /// blanks around), anything else, or nothing, is the real host.
    pub(crate) fn of(value: Option<&OsStr>) -> HostMode {
        if svolder_host::is_off(value) {
            HostMode::Off
        } else if value
            .and_then(OsStr::to_str)
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("fake"))
        {
            HostMode::Fake
        } else {
            HostMode::Real
        }
    }
}

/// Where the route comes from.
#[async_trait]
pub(crate) trait Routes: Send + Sync {
    /// The route to the default sink for the player being followed, or `None` when it cannot
    /// be read.
    async fn read(&self, player: Option<NowPlaying>) -> Option<RouteFacts>;
}

/// Finds the players on the bus: called when the host starts, and again every [`BUS_RETRY`]
/// once a bus it found has gone away, or one that took the connection and has not answered
/// ([`NoBus::Silent`]). It says why there is no bus ([`NoBus`]) when there is none.
pub(crate) type Connector =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn Players>, NoBus>> + Send + Sync>;

/// A connector that finds `players` every time.
pub(crate) fn found(players: Arc<dyn Players>) -> Connector {
    Arc::new(move || {
        let players = Arc::clone(&players);
        Box::pin(async move { Ok(players) })
    })
}

/// Where a cover comes from: the picture an address names, if it names one that can be read.
pub(crate) type CoverSource = Arc<dyn Fn(&str) -> Option<Cover> + Send + Sync>;

/// What a running host is made of.
pub(crate) struct Parts {
    /// Reaches the players.
    pub(crate) connect: Connector,
    /// Reads the route; `None` when there is no tool to read it with.
    pub(crate) routes: Option<Arc<dyn Routes>>,
    /// Finds the picture a player names.
    pub(crate) covers: CoverSource,
    /// Tasks that run beside the host and end with it.
    pub(crate) helpers: Vec<JoinHandle<()>>,
    /// What the capture of the sink is made of, or why there is none.
    pub(crate) frames: Result<Frames, String>,
    /// The computer's volume and EQ; `None` serves neither.
    pub(crate) computer: Option<Arc<Computer>>,
}

/// What a capture of the default sink is made of: the capture itself, and the sink it follows.
/// The sink is only ever read.
pub(crate) struct Frames {
    /// The capture, started on the sink's node and stopped when nobody watches.
    pub(crate) capture: Box<dyn FrameCapture>,
    /// The default sink, read every 2 s while capturing to follow it.
    pub(crate) sink: Arc<dyn Sink>,
}

/// Where the daemon's capture stands, for the pages and the TUIs that hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureState {
    /// Nothing is captured: it has not started, or it waits for a viewer.
    Starting,
    /// Frames are coming from a sink.
    Running {
        /// What the sink is called for people.
        sink: String,
        /// The node the capture runs on.
        target: String,
    },
    /// No capture is possible now.
    Unavailable {
        /// Why, in a plain sentence.
        why: String,
    },
}

impl CaptureState {
    /// The state as the socket and the pages say it.
    pub fn view(&self) -> CaptureView {
        match self {
            CaptureState::Starting => CaptureView {
                state: CaptureKind::Starting,
                sink: None,
                why: None,
            },
            CaptureState::Running { sink, .. } => CaptureView {
                state: CaptureKind::Running,
                sink: Some(sink.clone()),
                why: None,
            },
            CaptureState::Unavailable { why } => CaptureView {
                state: CaptureKind::Unavailable,
                sink: None,
                why: Some(why.clone()),
            },
        }
    }

    /// The state of a capture that is off: the host does not run.
    fn off() -> Self {
        CaptureState::Unavailable {
            why: "host audio is off".to_string(),
        }
    }
}

/// What a holder of the capture hears.
#[derive(Debug, Clone)]
pub enum FramesEvent {
    /// The capture's state: the first event of a stream, and each change after it.
    Capture(CaptureState),
    /// One analysed frame, shared by everyone who holds the capture.
    Frame(Arc<Frame>),
}

/// What the fake host does on its own, from the environment in fake mode.
#[derive(Debug, Clone)]
pub(crate) struct FakeKnobs {
    /// How long after it starts the fake player's art is dropped.
    pub(crate) cover_gone_after: Option<Duration>,
    /// How long after it starts the fake player quits.
    pub(crate) player_gone_after: Option<Duration>,
    /// A path: the fake player's art is dropped the moment a file appears there.
    pub(crate) cover_gone_file: Option<std::path::PathBuf>,
    /// A path: the fake player quits the moment a file appears there.
    pub(crate) player_gone_file: Option<std::path::PathBuf>,
    /// The display name of the fake device, which the fake route's sink is named for.
    pub(crate) display: String,
    /// Whether the fake route changes the signal in every way it can: a quieter player, a
    /// resample, a quieter sink, the host EQ and other sounds mixing.
    pub(crate) route_changed: bool,
    /// Whether the fake host has no capture to give: it reports the capture unavailable.
    pub(crate) capture_off: bool,
    /// The fake track's title in place of the fixed one, for a card whose title is cut.
    pub(crate) title: Option<String>,
    /// How the fake computer EQ behaves.
    pub(crate) eq: FakeEq,
    /// The fake computer's volume at the start, in percent.
    pub(crate) computer_volume: u8,
}

/// How the fake computer EQ behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FakeEq {
    /// A chain that takes live changes.
    #[default]
    Live,
    /// A chain whose live changes never read back, so a drag is applied when it ends.
    Release,
    /// No `pw-cli` on this computer.
    Off,
}

impl Default for FakeKnobs {
    fn default() -> Self {
        FakeKnobs {
            cover_gone_after: None,
            player_gone_after: None,
            cover_gone_file: None,
            player_gone_file: None,
            display: "Gungnir 2".to_string(),
            route_changed: false,
            capture_off: false,
            title: None,
            eq: FakeEq::Live,
            computer_volume: 42,
        }
    }
}

impl FakeKnobs {
    /// The knobs the environment sets.
    pub(crate) fn from_env() -> Self {
        Self::from_knobs(|name| std::env::var(name).ok())
    }

    /// The knobs `knob` answers by variable name: `SVOLDER_FAKE_COVER_GONE_AFTER_MS` and
    /// `SVOLDER_FAKE_PLAYER_GONE_AFTER_MS` in milliseconds (anything else is unset),
    /// `SVOLDER_FAKE_COVER_GONE_FILE` and `SVOLDER_FAKE_PLAYER_GONE_FILE` as paths (unset when
    /// empty),
    /// `SVOLDER_FAKE_ROUTE=changed` for the route that alters the signal,
    /// `SVOLDER_FAKE_CAPTURE=off` for a host with no capture, `SVOLDER_FAKE_TITLE` for the
    /// fake track's title (unset when empty), `SVOLDER_FAKE_HOST_EQ` as `release` for an EQ whose
    /// live changes never read back or `off` for a computer with no `pw-cli`, and the display name of the profile
    /// `SVOLDER_FAKE_PROFILE` names for the fake device.
    pub(crate) fn from_knobs(knob: impl Fn(&str) -> Option<String>) -> Self {
        let millis = |name: &str| {
            knob(name)
                .and_then(|ms| ms.trim().parse::<u64>().ok())
                .map(Duration::from_millis)
        };
        let path = |name: &str| {
            knob(name)
                .filter(|path| !path.is_empty())
                .map(std::path::PathBuf::from)
        };
        let mut knobs = FakeKnobs {
            cover_gone_after: millis("SVOLDER_FAKE_COVER_GONE_AFTER_MS"),
            player_gone_after: millis("SVOLDER_FAKE_PLAYER_GONE_AFTER_MS"),
            cover_gone_file: path("SVOLDER_FAKE_COVER_GONE_FILE"),
            player_gone_file: path("SVOLDER_FAKE_PLAYER_GONE_FILE"),
            route_changed: knob("SVOLDER_FAKE_ROUTE")
                .is_some_and(|route| route.trim().eq_ignore_ascii_case("changed")),
            capture_off: knob("SVOLDER_FAKE_CAPTURE")
                .is_some_and(|capture| capture.trim().eq_ignore_ascii_case("off")),
            title: knob("SVOLDER_FAKE_TITLE")
                .map(|title| title.trim().to_string())
                .filter(|title| !title.is_empty()),
            computer_volume: knob("SVOLDER_FAKE_COMPUTER_VOLUME")
                .and_then(|pct| pct.trim().parse::<u8>().ok())
                .map_or(42, |pct| pct.min(100)),
            eq: match knob("SVOLDER_FAKE_HOST_EQ")
                .map(|word| word.trim().to_ascii_lowercase())
                .as_deref()
            {
                Some("release") => FakeEq::Release,
                Some("off") => FakeEq::Off,
                _ => FakeEq::Live,
            },
            ..FakeKnobs::default()
        };
        if let Some(name) = knob("SVOLDER_FAKE_PROFILE")
            && let Ok(registry) = svolder_profiles::Registry::builtin()
            && let Some(profile) = registry.get(&name)
        {
            knobs.display = profile.display.clone();
        }
        knobs
    }
}

/// Returns once a file exists at `path`, looking again every 50 ms.
async fn wait_for_file(path: &std::path::Path) {
    while !path.exists() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

impl Parts {
    /// The fake host: one fixed track playing from the moment it starts, which answers
    /// presses; the bundled cover; a route on a Schiit sink named for the fake device, at the
    /// player's own rate; synthetic music for a capture, unless `knobs` has none; and whatever
    /// `knobs` has it lose. Needs a tokio runtime for the knobs' timers.
    pub(crate) fn fake(knobs: FakeKnobs) -> Parts {
        let players = fake_players(knobs.title.as_deref());
        let mut helpers = Vec::new();
        let drop_cover = {
            let players = players.clone();
            move || {
                if let Some(now) = players.now_playing() {
                    players.set(Some(NowPlaying {
                        art_url: None,
                        ..now
                    }));
                }
            }
        };
        if let Some(after) = knobs.cover_gone_after {
            let drop_cover = drop_cover.clone();
            helpers.push(tokio::spawn(async move {
                tokio::time::sleep(after).await;
                drop_cover();
            }));
        }
        if let Some(path) = knobs.cover_gone_file {
            helpers.push(tokio::spawn(async move {
                wait_for_file(&path).await;
                drop_cover();
            }));
        }
        if let Some(after) = knobs.player_gone_after {
            let players = players.clone();
            helpers.push(tokio::spawn(async move {
                tokio::time::sleep(after).await;
                players.set(None);
            }));
        }
        if let Some(path) = knobs.player_gone_file {
            let players = players.clone();
            helpers.push(tokio::spawn(async move {
                wait_for_file(&path).await;
                players.set(None);
            }));
        }
        let frames = match knobs.capture_off {
            true => Err("capture is switched off (SVOLDER_FAKE_CAPTURE=off)".to_string()),
            false => Ok(Frames {
                capture: Box::new(synthetic::SyntheticFrames::new()),
                sink: Arc::new(FakeSink::new(Some(SinkState {
                    volume: 1.0,
                    muted: false,
                    name: format!("Schiit {}", knobs.display),
                    target: Some(FAKE_SINK_NODE.to_string()),
                }))),
            }),
        };
        let computer = Arc::new(fake_computer(
            &knobs.display,
            knobs.eq,
            knobs.computer_volume,
        ));
        Parts {
            connect: found(Arc::new(players)),
            routes: Some(Arc::new(FakeRoute {
                display: knobs.display,
                changed: knobs.route_changed,
                computer: Some(Arc::clone(&computer)),
            })),
            covers: Arc::new(|url| {
                (url == FAKE_ART_URL).then(|| Cover {
                    bytes: BUNDLED_COVER.to_vec(),
                    mime: "image/png",
                })
            }),
            helpers,
            frames,
            computer: Some(computer),
        }
    }

    /// The real host: the session bus, `pw-dump` and the places a cover may be read from.
    fn real() -> Parts {
        let places = CoverPlaces::from_env();
        Parts {
            connect: Arc::new(|| {
                Box::pin(async {
                    Mpris::look_session()
                        .await
                        .map(|mpris| Arc::new(mpris) as Arc<dyn Players>)
                })
            }),
            routes: PwDump::from_env().map(|dump| Arc::new(PwRoutes(dump)) as Arc<dyn Routes>),
            covers: Arc::new(move |url| cover::cover(url, &places)),
            helpers: Vec::new(),
            frames: match (PwFrames::from_env(), Wpctl::from_env()) {
                (Some(capture), Some(sink)) => Ok(Frames {
                    capture: Box::new(capture),
                    sink: Arc::new(sink),
                }),
                _ => Err("this host has no capture tool".to_string()),
            },
            computer: Some(Arc::new(Computer::real())),
        }
    }
}

/// The fake computer: a sink at 42 % named for the fake device, and an EQ over a chain and a
/// `pw-cli` that live in memory and keep their points there.
fn fake_computer(display: &str, eq: FakeEq, volume_pct: u8) -> Computer {
    let sink: Arc<dyn Sink> = Arc::new(FakeSink::new(Some(SinkState {
        volume: f32::from(volume_pct) / 100.0,
        muted: false,
        name: format!("Schiit {display}"),
        target: Some(FAKE_SINK_NODE.to_string()),
    })));
    let controller = match eq {
        FakeEq::Off => Err("no pw-cli on this computer".to_string()),
        _ => {
            let params = FakeParams::new();
            params.holds(eq == FakeEq::Live);
            Ok(EqControl::new(
                Arc::new(FakeChain::new()),
                Box::new(params),
                Keep::Memory,
            ))
        }
    };
    Computer::new(Ok(sink), controller)
}

/// The fake players: one track playing from now, which answers presses. `title` replaces the
/// fixed title.
fn fake_players(title: Option<&str>) -> FakePlayers {
    let started_ms = unix_ms();
    FakePlayers::new(Some(NowPlaying {
        title: title.unwrap_or("Test Pressing").to_string(),
        artist: "The Fakes".to_string(),
        album: "Demo Tape".to_string(),
        genre: vec!["Demo".to_string()],
        identity: "Fake Player".to_string(),
        status: Status::Playing,
        art_url: Some(FAKE_ART_URL.to_string()),
        track_id: Some("/org/svolder/fake/track/1".to_string()),
        length_us: Some(20 * 60 * 1_000_000),
        progress: Some(Progress {
            position_us: 0,
            rate_milli: 1000,
            read_at_ms: started_ms,
        }),
        pid: Some(std::process::id()),
    }))
    .answering()
}

/// The real route: `pw-dump`, read for the player being followed.
struct PwRoutes(PwDump);

#[async_trait]
impl Routes for PwRoutes {
    async fn read(&self, player: Option<NowPlaying>) -> Option<RouteFacts> {
        let dump = self.0.read().await?;
        let pid = player.as_ref().and_then(|player| player.pid);
        let identity = player
            .as_ref()
            .map_or("", |player| player.identity.as_str());
        route(&dump, pid, identity)
    }
}

/// The fake route: a Schiit sink named for the fake device, the player's stream at the same
/// rate and nothing else in the way, so the path reads untouched; or, when asked, a route that
/// alters the signal in every way: the player at 91 % and 44.1 kHz into a sink at 48 kHz and
/// 54 %, the host EQ on and two other sounds mixing. The fake computer's EQ is part of the route:
/// on with a curve that is not flat, it is in the path as the real chain would be.
struct FakeRoute {
    display: String,
    /// Whether the route changes the signal (see [`FakeKnobs::route_changed`]).
    changed: bool,
    /// The fake computer, whose EQ the route follows.
    computer: Option<Arc<Computer>>,
}

impl FakeRoute {
    /// Whether the fake computer's EQ is on with a curve that changes the sound.
    fn eq_changes_the_sound(&self) -> bool {
        self.computer.as_ref().is_some_and(|computer| {
            let eq = computer.eq_view();
            eq.on && eq.points.iter().any(|point| point.g != 0.0)
        })
    }
}

#[async_trait]
impl Routes for FakeRoute {
    async fn read(&self, player: Option<NowPlaying>) -> Option<RouteFacts> {
        let product = format!("Schiit {}", self.display);
        // The fake computer's own volume and mute are the sink's, as the real route reads them.
        let (volume_milli, muted) = match (&self.computer, self.changed) {
            (Some(computer), false) => match computer.read_volume().await {
                Some(volume) => (u32::from(volume.pct) * 10, volume.muted),
                None => (1000, false),
            },
            (_, true) => (540, false),
            _ => (1000, false),
        };
        let sink = SinkFacts {
            name: FAKE_SINK_NODE.to_string(),
            description: product.clone(),
            rate_hz: Some(FAKE_RATE_HZ),
            format: Some("S32LE".to_string()),
            volume_milli: Some(volume_milli),
            muted,
            usb_vendor_id: Some(SCHIIT_VENDOR_ID),
            usb_product: Some(product),
        };
        let stream = StreamFacts {
            rate_hz: Some(if self.changed { 44_100 } else { FAKE_RATE_HZ }),
            format: Some("S16LE".to_string()),
            volume_milli: Some(if self.changed { 910 } else { 1000 }),
            muted: false,
        };
        let playing = player.is_some();
        Some(RouteFacts {
            sink: Some(sink),
            player: playing.then_some(stream),
            player_match: if playing {
                PlayerMatch::Pid
            } else {
                PlayerMatch::NoPlayer
            },
            others_mixing: if self.changed { 2 } else { 0 },
            host_eq: self.changed || self.eq_changes_the_sound(),
        })
    }
}

/// What the service's task and its handles share.
struct Shared {
    /// The view, which the task publishes and everyone reads.
    view: watch::Sender<HostView>,
    /// How many [`Watcher`]s are held.
    watchers: watch::Sender<usize>,
    /// How many [`FramesHold`]s are held: the capture runs while there is one.
    holds: watch::Sender<usize>,
    /// The frames of the running capture, to every hold.
    frames: broadcast::Sender<Arc<Frame>>,
    /// Where the capture stands.
    capture: watch::Sender<CaptureState>,
    /// Whether a run is in place that captures (or says why it cannot): false before the first
    /// run and after a halt, when no hold is given.
    capturing: AtomicBool,
    /// The players presses go to, while there is a bus.
    players: Mutex<Option<Arc<dyn Players>>>,
    /// The computer's volume and EQ, while a run has them.
    computer: Mutex<Option<Arc<Computer>>>,
    /// The current cover and its hash.
    cover: Mutex<Option<(String, Cover)>>,
    /// Which run is the current one. Halting moves it on, and a task of an earlier run, which
    /// may get one more step in before it is ended, changes nothing (see [`Shared::current`]).
    generation: Mutex<u64>,
}

impl Shared {
    /// Run `change` if `generation` is still the current one. The gate is held throughout, so a
    /// halt cannot come between the check and the change: a task that was halted never
    /// publishes an available view, takes the players or leaves a cover behind.
    fn current<R>(&self, generation: u64, change: impl FnOnce() -> R) -> Option<R> {
        let gate = locked(&self.generation);
        (*gate == generation).then(change)
    }
}

/// What the task that follows the players is made of.
#[derive(Clone)]
struct Follows {
    connect: Connector,
    routes: Option<Arc<dyn Routes>>,
    covers: CoverSource,
}

/// What is running behind the service.
#[derive(Default)]
struct Running {
    /// The task following the players.
    task: Option<JoinHandle<()>>,
    /// The tasks that run beside it.
    helpers: Vec<JoinHandle<()>>,
    /// What the following task is made of, kept so one that finished (a bus that never answered)
    /// can be started again without stopping the computer's controls.
    follows: Option<Follows>,
    /// What [`HostService::reconfigure`] last started: whether `[host]` was enabled and the mode.
    wanted: Option<(bool, HostMode)>,
    /// Whether the daemon's stop has begun, after which nothing is started.
    stopped: bool,
}

/// The host as the daemon holds it.
pub(crate) struct HostService {
    shared: Arc<Shared>,
    running: Mutex<Running>,
}

/// What turns the computer EQ on and off for a front end that holds no controller of its own: the
/// MQTT bridge.
#[async_trait::async_trait]
pub(crate) trait EqSwitch: Send + Sync {
    /// Turn the EQ on with the points kept, or off, which keeps them.
    async fn set_on(&self, on: bool) -> Result<(), String>;
}

/// The computer's controller as the service holds it, reached when a switch is turned.
struct SharedSwitch(Arc<Shared>);

#[async_trait::async_trait]
impl EqSwitch for SharedSwitch {
    async fn set_on(&self, on: bool) -> Result<(), String> {
        let computer = locked(&self.0.computer)
            .clone()
            .ok_or_else(|| "computer controls unavailable: host audio is off".to_string())?;
        computer
            .eq_op(
                HostEqOp::Mode { on },
                &[],
                |_| Vec::new(),
                |_| async { Ok(()) },
            )
            .await
            .map_err(|refusal| refusal.to_string())
    }
}

/// A hold on the route reads: while one exists the route is read and kept up to date. Dropping
/// it lets go.
pub(crate) struct Watcher {
    shared: Arc<Shared>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.shared
            .watchers
            .send_modify(|count| *count = count.saturating_sub(1));
    }
}

/// A hold on the capture: while one exists the daemon captures the default sink and analyses it
/// (see [`HostService::frames`]). Dropping it lets go.
pub struct FramesHold {
    shared: Arc<Shared>,
    frames: broadcast::Receiver<Arc<Frame>>,
    state: watch::Receiver<CaptureState>,
}

impl Drop for FramesHold {
    fn drop(&mut self) {
        self.shared
            .holds
            .send_modify(|count| *count = count.saturating_sub(1));
    }
}

impl FramesHold {
    /// Where the capture stands now.
    #[cfg(test)]
    pub fn state(&self) -> CaptureState {
        self.state.borrow().clone()
    }

    /// The capture's state now and on each change, and the frames while it runs, for as long as
    /// the stream lives: it owns the hold. A listener that falls behind skips to the newest
    /// frames, so the capture never waits for it.
    pub fn events(self) -> BoxStream<'static, FramesEvent> {
        futures::stream::unfold((self, true), |(mut hold, first)| async move {
            if first {
                let state = hold.state.borrow_and_update().clone();
                return Some((FramesEvent::Capture(state), (hold, false)));
            }
            loop {
                tokio::select! {
                    biased;
                    changed = hold.state.changed() => {
                        changed.ok()?;
                        let state = hold.state.borrow_and_update().clone();
                        return Some((FramesEvent::Capture(state), (hold, false)));
                    }
                    frame = hold.frames.recv() => match frame {
                        Ok(frame) => return Some((FramesEvent::Frame(frame), (hold, false))),
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
                }
            }
        })
        .boxed()
    }
}

impl Drop for HostService {
    fn drop(&mut self) {
        halt(&mut locked(&self.running), &self.shared);
    }
}

impl HostService {
    /// A service that runs nothing yet: unavailable until [`HostService::reconfigure`] or a
    /// test seam starts something.
    pub(crate) fn idle() -> Self {
        HostService {
            shared: Arc::new(Shared {
                view: watch::channel(HostView::default()).0,
                watchers: watch::channel(0).0,
                holds: watch::channel(0).0,
                frames: broadcast::channel(FRAMES_BACKLOG).0,
                capture: watch::channel(CaptureState::off()).0,
                capturing: AtomicBool::new(false),
                players: Mutex::new(None),
                computer: Mutex::new(None),
                cover: Mutex::new(None),
                generation: Mutex::new(0),
            }),
            running: Mutex::new(Running::default()),
        }
    }

    /// A service running what `config` and `mode` ask for. Needs a tokio runtime.
    #[cfg(test)]
    pub(crate) fn start(config: &HostConfig, mode: HostMode) -> Self {
        let service = Self::idle();
        service.reconfigure(config, mode);
        service
    }

    /// Bring what runs in line with `config` and `mode`. A host that is off (either way) runs
    /// nothing and reports itself unavailable; one that already runs the same is left alone,
    /// except that one whose bus never answered is tried again; any other change stops the old
    /// one and starts the new behind the same channels. Nothing starts once [`HostService::stop`]
    /// has been called. Needs a tokio runtime.
    pub(crate) fn reconfigure(&self, config: &HostConfig, mode: HostMode) {
        let wanted = (config.enabled, mode);
        let off = !config.enabled || mode == HostMode::Off;
        let mut running = locked(&self.running);
        if running.stopped {
            return;
        }
        let alive = running
            .task
            .as_ref()
            .is_some_and(|task| !task.is_finished());
        if running.wanted == Some(wanted) && (off || alive) {
            return;
        }
        // The same host whose following task has finished (no bus answered) is tried again; the
        // computer's controls keep running, so a reload never takes the chain away.
        if running.wanted == Some(wanted)
            && let Some(follows) = running.follows.clone()
        {
            let generation = *locked(&self.shared.generation);
            running.task = Some(spawn_follow(&self.shared, generation, follows));
            return;
        }
        halt(&mut running, &self.shared);
        running.wanted = Some(wanted);
        if off {
            return;
        }
        let parts = match mode {
            HostMode::Fake => Parts::fake(FakeKnobs::from_env()),
            _ => Parts::real(),
        };
        launch(
            &mut running,
            &self.shared,
            parts,
            svolder_ipc::checks::live::in_container(),
        );
    }

    /// Run `parts` in place of whatever runs, for tests. Needs a tokio runtime.
    #[cfg(test)]
    pub(crate) fn run_with(&self, parts: Parts) {
        self.run_with_in(parts, false);
    }

    /// [`HostService::run_with`] as a daemon that runs in a container (or not) starts it.
    #[cfg(test)]
    pub(crate) fn run_with_in(&self, parts: Parts, container: bool) {
        let mut running = locked(&self.running);
        halt(&mut running, &self.shared);
        launch(&mut running, &self.shared, parts, container);
    }

    /// End the host for good: what runs stops, the view goes unavailable, and no later
    /// reconfiguration starts anything.
    pub(crate) fn stop(&self) {
        let mut running = locked(&self.running);
        running.stopped = true;
        halt(&mut running, &self.shared);
    }

    /// The host as it stands, stamped with the daemon's clock.
    pub(crate) fn view(&self) -> HostView {
        stamped(self.shared.view.borrow().clone())
    }

    /// The host as it stands, with the computer's EQ as it is this moment and its volume read now
    /// when nobody watches and so none is kept, for a one-shot `host` request.
    pub(crate) async fn view_now(&self) -> HostView {
        let mut view = self.view();
        let Some(computer) = locked(&self.shared.computer).clone() else {
            return view;
        };
        // The controller's own newest view, not the one the throttle last let through.
        view.eq = Some(computer.eq_view());
        if view.volume.is_none() {
            view.volume = computer.read_volume().await;
        }
        view
    }

    /// The computer's controls, or the sentence that says why there are none.
    pub(crate) fn computer(&self) -> Result<Arc<Computer>, String> {
        locked(&self.shared.computer)
            .clone()
            .ok_or_else(|| "computer controls unavailable: host audio is off".to_string())
    }

    /// Keep what a write read back in the view, while someone watches (nothing is kept for
    /// nobody).
    fn keep_volume(&self, volume: &HostVolume) {
        if *self.shared.watchers.borrow() > 0 {
            computer::put_volume(&self.shared, Some(volume.clone()));
        }
    }

    /// Set the computer's volume to `pct` percent, 0 to 100, and read it back.
    pub(crate) async fn host_volume(&self, pct: u8) -> Result<HostVolume, String> {
        let computer = self.computer()?;
        // The write runs on a task of its own: a requester that goes away does not take the
        // newest value with it.
        let volume = tokio::spawn(async move { computer.set_volume(pct).await })
            .await
            .map_err(|_| "the computer's volume write was cut short".to_string())??;
        self.keep_volume(&volume);
        Ok(volume)
    }

    /// Mute or unmute the computer's sink and read it back.
    pub(crate) async fn host_mute(&self, muted: bool) -> Result<HostVolume, String> {
        let volume = self.computer()?.set_mute(muted).await?;
        self.keep_volume(&volume);
        Ok(volume)
    }

    /// Change the computer's EQ (see [`Computer::eq_op`]).
    pub(crate) async fn host_eq<F, Fut>(
        &self,
        op: HostEqOp,
        presets: &[Preset],
        holders: impl Fn(&str) -> Vec<String>,
        flatten: F,
    ) -> Result<(), EqRefusal>
    where
        F: FnOnce(Vec<String>) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        self.computer()?.eq_op(op, presets, holders, flatten).await
    }

    /// The switch of the computer EQ, which follows the controller the service holds when it is
    /// turned, whatever reloads happened since it was handed out.
    pub(crate) fn eq_switch(&self) -> Arc<dyn EqSwitch> {
        Arc::new(SharedSwitch(Arc::clone(&self.shared)))
    }

    /// A receiver that hears each change of the view. Its stamp is the time of the change.
    pub(crate) fn changes(&self) -> watch::Receiver<HostView> {
        self.shared.view.subscribe()
    }

    /// The view now and each change after it, as socket events, for as long as the stream
    /// lives: it holds a [`Watcher`] all that time, so the route is read for it.
    pub(crate) fn events(&self) -> BoxStream<'static, Event> {
        let watcher = self.watching();
        WatchStream::new(self.shared.view.subscribe())
            .map(move |view| {
                let _held = &watcher;
                Event::Host(stamped(view))
            })
            .boxed()
    }

    /// Press `button` on the followed player. Whether a player took it.
    pub(crate) async fn press(&self, button: Button) -> bool {
        let players = locked(&self.shared.players).clone();
        match players {
            Some(players) => players.press(button).await,
            None => false,
        }
    }

    /// The current cover, when `hash` is its hash; never an earlier one.
    pub(crate) fn cover(&self, hash: &str) -> Option<Cover> {
        locked(&self.shared.cover)
            .as_ref()
            .filter(|(current, _)| current == hash)
            .map(|(_, cover)| cover.clone())
    }

    /// How many [`Watcher`]s are held now.
    #[cfg(test)]
    pub(crate) fn watcher_count(&self) -> usize {
        *self.shared.watchers.borrow()
    }

    /// How many [`FramesHold`]s are held now.
    #[cfg(test)]
    pub(crate) fn frame_holds(&self) -> usize {
        *self.shared.holds.borrow()
    }

    /// Hold the capture of what the computer plays for as long as the returned value lives:
    /// the first hold starts it, and it stops 2 s after the last is let go. `None` when the
    /// host is off, which has nothing to capture and says nothing of why.
    pub(crate) fn frames(&self) -> Option<FramesHold> {
        if !self.shared.capturing.load(Ordering::SeqCst) {
            return None;
        }
        // Receivers first, so no frame or change after the hold is missed.
        let frames = self.shared.frames.subscribe();
        let state = self.shared.capture.subscribe();
        self.shared.holds.send_modify(|count| *count += 1);
        Some(FramesHold {
            shared: Arc::clone(&self.shared),
            frames,
            state,
        })
    }

    /// Hold the route reads for as long as the returned value lives.
    pub(crate) fn watching(&self) -> Watcher {
        self.shared.watchers.send_modify(|count| *count += 1);
        Watcher {
            shared: Arc::clone(&self.shared),
        }
    }
}

/// End what runs and leave the view unavailable and empty, and every task of this run unable
/// to change that.
fn halt(running: &mut Running, shared: &Shared) {
    running.follows = None;
    if let Some(task) = running.task.take() {
        task.abort();
    }
    for helper in running.helpers.drain(..) {
        helper.abort();
    }
    let mut gate = locked(&shared.generation);
    *gate += 1;
    *locked(&shared.players) = None;
    *locked(&shared.computer) = None;
    *locked(&shared.cover) = None;
    shared.capturing.store(false, Ordering::SeqCst);
    shared.capture.send_replace(CaptureState::off());
    shared.view.send_if_modified(|current| {
        let cleared = *current == HostView::default();
        *current = HostView::default();
        !cleared
    });
}

/// Start the task that runs `parts`.
fn launch(running: &mut Running, shared: &Arc<Shared>, parts: Parts, container: bool) {
    let Parts {
        connect,
        routes,
        covers,
        helpers,
        frames,
        computer,
    } = parts;
    running.helpers = helpers;
    let generation = *locked(&shared.generation);
    if let Some(computer) = computer {
        *locked(&shared.computer) = Some(Arc::clone(&computer));
        running.helpers.push(tokio::spawn(computer::run(
            Arc::clone(shared),
            generation,
            computer,
            container,
        )));
    }
    shared.capturing.store(true, Ordering::SeqCst);
    match frames {
        Ok(frames) => {
            shared.capture.send_replace(CaptureState::Starting);
            running.helpers.push(tokio::spawn(capture::run(
                Arc::clone(shared),
                generation,
                frames,
            )));
        }
        Err(why) => {
            shared
                .capture
                .send_replace(CaptureState::Unavailable { why });
        }
    }
    let follows = Follows {
        connect,
        routes,
        covers,
    };
    running.follows = Some(follows.clone());
    running.task = Some(spawn_follow(shared, generation, follows));
}

/// Start the task that follows the players.
fn spawn_follow(shared: &Arc<Shared>, generation: u64, follows: Follows) -> JoinHandle<()> {
    let Follows {
        connect,
        routes,
        covers,
    } = follows;
    tokio::spawn(follow(
        Arc::clone(shared),
        generation,
        connect,
        routes,
        covers,
    ))
}

/// What the task knows.
#[derive(Default)]
struct State {
    /// What the followed player plays.
    now: Option<NowPlaying>,
    /// The route as the last read gave it.
    route: Option<RouteFacts>,
    /// The cover address being followed and what came of it.
    art: Option<Art>,
    /// The read of that cover under way.
    covering: Option<Covering>,
}

/// One cover address and the picture it gave.
struct Art {
    url: String,
    /// The picture's hash, when one was read.
    hash: Option<String>,
    /// How many reads were made.
    tries: u8,
    /// When the first read was due, which the later ones are spaced from.
    first: Instant,
    /// When the next read is due.
    next: Instant,
}

/// A cover read under way on the blocking pool; ended when dropped, if it has not started.
struct Covering {
    url: String,
    read: JoinHandle<Option<Cover>>,
}

impl Drop for Covering {
    fn drop(&mut self) {
        self.read.abort();
    }
}

impl State {
    /// Follow the cover address the player now names. A new address clears the cover at once
    /// (the old picture is never shown with the new track) and has its first read due now; the
    /// same address changes nothing, and none ends the cover.
    fn follow_cover(&mut self, shared: &Shared, generation: u64) {
        let url = self.now.as_ref().and_then(|now| now.art_url.clone());
        if url.as_deref() == self.art.as_ref().map(|art| art.url.as_str()) {
            return;
        }
        self.covering = None;
        let now = Instant::now();
        self.art = url.map(|url| Art {
            url,
            hash: None,
            tries: 0,
            first: now,
            next: now,
        });
        shared.current(generation, || *locked(&shared.cover) = None);
    }

    /// When the cover is next to be read: not while a read is under way, once a picture is
    /// held, or after the third read.
    fn cover_due(&self) -> Option<Instant> {
        let art = self.art.as_ref()?;
        (art.hash.is_none() && art.tries < COVER_TRIES && self.covering.is_none())
            .then_some(art.next)
    }

    /// Start the read of the cover on the blocking pool, so a slow disk never holds up the
    /// task.
    fn read_cover(&mut self, covers: &CoverSource) {
        let Some(art) = self.art.as_mut() else {
            return;
        };
        art.tries += 1;
        let (url, covers) = (art.url.clone(), Arc::clone(covers));
        let asked = url.clone();
        self.covering = Some(Covering {
            url,
            read: tokio::task::spawn_blocking(move || covers(&asked)),
        });
    }

    /// Take what the finished cover read gave: the picture and its hash, or a miss, after which
    /// the next read is due 1 s and then 3 s after the first one, and no more.
    fn took_cover(&mut self, found: Option<Cover>) -> Option<(String, Cover)> {
        let covering = self.covering.take()?;
        let art = self.art.as_mut().filter(|art| art.url == covering.url)?;
        match found {
            Some(cover) => {
                let hash = hash_of(&cover.bytes);
                art.hash = Some(hash.clone());
                Some((hash, cover))
            }
            None => {
                if let Some(after) = COVER_RETRIES.get(usize::from(art.tries) - 1) {
                    art.next = art.first + *after;
                }
                None
            }
        }
    }

    /// The player is gone, so the route has no stream of its own to show: the stream and the
    /// match go at once rather than at the next read.
    fn lose_player_in_route(&mut self) {
        if self.now.is_none()
            && let Some(route) = self.route.as_mut()
        {
            route.player = None;
            route.player_match = PlayerMatch::NoPlayer;
        }
    }

    /// The view of this state.
    fn view(&self) -> HostView {
        let cover = self.art.as_ref().and_then(|art| art.hash.clone());
        HostView {
            available: true,
            playing: self.now.as_ref().map(|now| playing_of(now, cover)),
            route: self.route.clone(),
            now_ms: unix_ms(),
            volume: None,
            eq: None,
        }
    }
}

/// The first 16 lowercase hex characters of the SHA-256 of `bytes`.
fn hash_of(bytes: &[u8]) -> String {
    let digest = digest::digest(&digest::SHA256, bytes);
    digest.as_ref()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A player's track as a front end is told of it.
fn playing_of(now: &NowPlaying, cover: Option<String>) -> PlayingView {
    PlayingView {
        title: now.title.clone(),
        artist: now.artist.clone(),
        album: now.album.clone(),
        player: now.identity.clone(),
        status: match now.status {
            Status::Playing => "playing",
            Status::Paused => "paused",
            Status::Stopped => "stopped",
        }
        .to_string(),
        length_us: now.length_us,
        progress: now.progress,
        cover,
    }
}

/// Make `view` the one everyone reads, unless it differs from it by nothing but the clock. The
/// computer's volume and EQ are not the player's to change: they stay as they are.
fn publish(shared: &Shared, view: HostView) {
    shared.view.send_if_modified(|current| {
        let view = HostView {
            volume: current.volume.clone(),
            eq: current.eq.clone(),
            ..view
        };
        let same = HostView {
            now_ms: current.now_ms,
            ..view.clone()
        } == *current;
        if !same {
            *current = view;
        }
        !same
    });
}

/// `view` with the daemon's clock now.
fn stamped(view: HostView) -> HostView {
    HostView {
        now_ms: unix_ms(),
        ..view
    }
}

/// A route read under way, ended with it.
struct Reading(JoinHandle<Option<RouteFacts>>);

impl Drop for Reading {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The route a read under way gives, once it is done; a read that failed or was ended gives
/// none. Waits for ever when there is no read.
async fn finished_route(reading: &mut Option<Reading>) -> Option<RouteFacts> {
    match reading {
        Some(Reading(read)) => read.await.ok().flatten(),
        None => std::future::pending().await,
    }
}

/// The cover a read under way gives, once it is done; a read that failed gives none. Waits for
/// ever when there is no read.
async fn finished_cover(covering: &mut Option<Covering>) -> Option<Cover> {
    match covering {
        Some(Covering { read, .. }) => read.await.ok().flatten(),
        None => std::future::pending().await,
    }
}

/// Follow the players until the task is ended: connect, then publish every change of the
/// player, and read the route while someone watches. A bus that was there and goes away leaves
/// the host unavailable until it is found again, which is looked for every [`BUS_RETRY`], as is
/// a bus that took the connection and has not answered yet ([`NoBus::Silent`], also a look
/// refused because an earlier one still waits on that bus); a bus that was never there
/// ([`NoBus::Absent`]) is left to the next reconfiguration. A look at a bus that took a
/// connection and never answered is refused while that connect still blocks ([`Mpris::at`]), so
/// retrying a hung bus adds no thread. Everything it changes is changed only while `generation`
/// is the current one.
async fn follow(
    shared: Arc<Shared>,
    generation: u64,
    connect: Connector,
    routes: Option<Arc<dyn Routes>>,
    covers: CoverSource,
) {
    let mut players = match connect().await {
        Ok(found) => found,
        Err(NoBus::Absent) => return,
        // A bus that took the connection counts as one seen: it is looked for until it answers,
        // whatever the looks in between find.
        Err(NoBus::Silent) => find_again(&connect).await,
    };
    loop {
        if !serve(&shared, generation, players, routes.clone(), &covers).await {
            return;
        }
        players = find_again(&connect).await;
    }
}

/// Look for the bus every [`BUS_RETRY`] until it is found, whatever the looks in between say.
async fn find_again(connect: &Connector) -> Arc<dyn Players> {
    loop {
        tokio::time::sleep(BUS_RETRY).await;
        if let Ok(found) = connect().await {
            return found;
        }
    }
}

/// Follow the players on one connection to the bus, until it goes away; whether it was this
/// run that was following them, which a halt ends.
async fn serve(
    shared: &Arc<Shared>,
    generation: u64,
    players: Arc<dyn Players>,
    routes: Option<Arc<dyn Routes>>,
    covers: &CoverSource,
) -> bool {
    let held = shared.current(generation, || {
        *locked(&shared.players) = Some(Arc::clone(&players));
    });
    if held.is_none() {
        return false;
    }
    let mut changes = players.changes();
    let mut watchers = shared.watchers.subscribe();
    let mut state = State {
        now: players.now_playing(),
        ..State::default()
    };
    state.follow_cover(shared, generation);
    let emit = |state: &State| {
        shared.current(generation, || publish(shared, state.view()));
    };
    emit(&state);

    let mut last_read: Option<Instant> = None;
    let mut reading: Option<Reading> = None;
    loop {
        let watched = *watchers.borrow() > 0;
        let route_due = (watched && routes.is_some() && reading.is_none())
            .then(|| last_read.map_or_else(Instant::now, |at| at + ROUTE_EVERY));
        let cover_due = state.cover_due();
        tokio::select! {
            change = changes.next() => {
                let Some(now) = change else { break };
                state.now = now;
                state.follow_cover(shared, generation);
                if state.now.is_none() {
                    reading = None;
                    state.lose_player_in_route();
                }
                emit(&state);
            }
            changed = watchers.changed() => {
                if changed.is_err() {
                    break;
                }
                if *watchers.borrow_and_update() == 0 {
                    reading = None;
                    if state.route.take().is_some() {
                        emit(&state);
                    }
                }
            }
            () = tokio::time::sleep_until(route_due.unwrap_or_else(Instant::now)),
                if route_due.is_some() =>
            {
                last_read = Some(Instant::now());
                if let Some(routes) = routes.clone() {
                    let player = state.now.clone();
                    reading = Some(Reading(tokio::spawn(async move {
                        routes.read(player).await
                    })));
                }
            }
            route = finished_route(&mut reading), if reading.is_some() => {
                reading = None;
                state.route = route;
                emit(&state);
            }
            () = tokio::time::sleep_until(cover_due.unwrap_or_else(Instant::now)),
                if cover_due.is_some() =>
            {
                state.read_cover(covers);
            }
            found = finished_cover(&mut state.covering), if state.covering.is_some() => {
                if let Some((hash, cover)) = state.took_cover(found) {
                    shared.current(generation, || {
                        *locked(&shared.cover) = Some((hash, cover));
                        publish(shared, state.view());
                    });
                } else {
                    emit(&state);
                }
            }
        }
    }
    // The players can no longer be followed: the bus went away.
    drop(reading);
    shared.current(generation, || {
        *locked(&shared.players) = None;
        *locked(&shared.cover) = None;
        publish(shared, HostView::default());
    });
    true
}

#[cfg(test)]
mod tests;
