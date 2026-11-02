// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers several of `run`'s test modules share: a recorder terminal, fake backends and
//! openers, a scratch configuration file, and the small pieces of input a case scripts.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use futures::future::BoxFuture;
use ratatui::backend::{ClearType, TestBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use svolder_ipc::{ColorDepth, Config, TuiConfig};

use svolder_autoeq::{Entry, Index, Refresh};
use svolder_host::eq::{HostEqState, Manager, Route};
use svolder_host::fake::{FakeCapture, FakePlayers, FakeSink};
use svolder_host::{Block, NoBus, NowPlaying, Players, SinkState, Status};

use crate::backend::FakeGate;
use crate::commands::host_eq::ApplyRequest;
use crate::recovery::Kind;
use crate::test_support::stub_daemon::stub_daemon;
use crate::tui::app::{App, Intent};
use crate::tui::decoration::title::TitleMeter;
use crate::tui::decoration::title::TitleSink;
use crate::wizard::Wizard;
use crate::wizard::fake::FakeEffects;

use super::connect::TokenSite;
use super::devices::relisted;
use super::host::Connect;
use super::input::take_intent;
use super::jobs::Hands;
use super::{Adding, Backend, Failure, Flow, HostLink, Look, Open, Opener, OwnedGlobals, Watching};

/// How long a reconnect attempt is given for a case that wants it to give up.
pub(super) const SHORT_DEADLINE: Duration = Duration::from_millis(50);
/// A function address no profile names and the fake device never reports.
pub(super) const ABSENT: svolder_proto::FunctionId = svolder_proto::FunctionId::two(99, 99);
/// The terminal every loop case draws in: the smallest a screen is drawn in.
pub(super) const SIZE: (u16, u16) = (80, 24);
/// How many frames a loop case may draw before its terminal gives up, which turns a loop
/// that spins into a failed assertion rather than a test that never returns.
const FRAME_BUDGET: usize = 200;
/// What an open a case has arranged to fail says.
pub(super) const REFUSED: &str = "the device is not answering";

/// The text of every frame a [`Recorder`] has drawn, in order.
pub(super) type Frames = Arc<Mutex<Vec<String>>>;

/// The size a [`Recorder`]'s terminal is, which a case can change while the loop runs.
pub(super) type TerminalSize = Arc<Mutex<(u16, u16)>>;

/// A terminal that keeps the text of every frame drawn on it.
///
/// Each frame is recorded as ratatui finishes it, so a case can say what the loop showed
/// over time rather than only what was left on the screen at the end.
pub(super) struct Recorder {
    inner: TestBackend,
    frames: Frames,
    /// How big the terminal is, which a case can change while the loop runs, as a window being
    /// resized does: the next frame is drawn at the new size.
    size: TerminalSize,
}

impl Recorder {
    /// A recorder over a terminal `width` by `height`, and the frames it will fill.
    pub(super) fn new(width: u16, height: u16) -> (Recorder, Frames) {
        let (recorder, frames, _) = Recorder::resizable(width, height);
        (recorder, frames)
    }

    /// A recorder over a terminal `width` by `height`, the frames it will fill, and its size,
    /// which the case can change as the loop runs.
    pub(super) fn resizable(width: u16, height: u16) -> (Recorder, Frames, TerminalSize) {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let size = Arc::new(Mutex::new((width, height)));
        let recorder = Recorder {
            inner: TestBackend::new(width, height),
            frames: Arc::clone(&frames),
            size: Arc::clone(&size),
        };
        (recorder, frames, size)
    }

    /// Take the size the case last set, before the terminal clears for a frame at it or draws
    /// one.
    fn follow(&mut self) {
        let (width, height) = *self.size.lock().expect("size mutex");
        let area = self.inner.buffer().area;
        if (area.width, area.height) != (width, height) {
            self.inner.resize(width, height);
        }
    }
}

/// What a `TestBackend` answers, in the failable currency a recorder speaks: it cannot
/// fail, so only the recorder's own frame budget ever produces an error.
fn kept<T>(answer: Result<T, std::convert::Infallible>) -> io::Result<T> {
    match answer {
        Ok(value) => Ok(value),
        Err(never) => match never {},
    }
}

impl ratatui::backend::Backend for Recorder {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.follow();
        kept(self.inner.draw(content))
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        kept(self.inner.hide_cursor())
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        kept(self.inner.show_cursor())
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        kept(self.inner.get_cursor_position())
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        kept(self.inner.set_cursor_position(position))
    }

    fn clear(&mut self) -> io::Result<()> {
        self.follow();
        kept(self.inner.clear())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.follow();
        kept(self.inner.clear_region(clear_type))
    }

    fn size(&self) -> io::Result<Size> {
        let (width, height) = *self.size.lock().expect("size mutex");
        Ok(Size::new(width, height))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        kept(self.inner.window_size())
    }

    /// Record the frame ratatui has just finished, and refuse to draw once a case has had
    /// more than any of them should need.
    fn flush(&mut self) -> io::Result<()> {
        kept(self.inner.flush())?;
        let buffer = self.inner.buffer();
        let text: String = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut frames = self.frames.lock().expect("frames mutex");
        frames.push(text);
        if frames.len() > FRAME_BUDGET {
            return Err(io::Error::other("the frame budget is spent"));
        }
        Ok(())
    }
}

/// A mouse capture that only remembers which way it was last set.
#[derive(Default)]
pub(super) struct SpyMouse {
    pub(super) on: Option<bool>,
}

impl crate::tui::terminal::MouseCapture for SpyMouse {
    fn set_mouse(&mut self, on: bool) -> io::Result<()> {
        self.on = Some(on);
        Ok(())
    }
}

/// A mouse capture the terminal refuses to change.
pub(super) struct RefusingMouse;

impl crate::tui::terminal::MouseCapture for RefusingMouse {
    fn set_mouse(&mut self, _on: bool) -> io::Result<()> {
        Err(io::Error::other("the terminal refused"))
    }
}

/// A terminal title that only remembers what was written to it.
#[derive(Default, Clone)]
struct SpyTitle(Arc<Mutex<Vec<String>>>);

impl TitleSink for SpyTitle {
    fn set_title(&mut self, text: &str) -> io::Result<()> {
        self.0.lock().expect("the titles mutex").push(text.into());
        Ok(())
    }
}

/// A title meter over a spy, and the titles the spy is handed.
pub(super) fn spied_title() -> (TitleMeter, Arc<Mutex<Vec<String>>>) {
    let spy = SpyTitle::default();
    let titles = spy.0.clone();
    (TitleMeter::new(Box::new(spy)), titles)
}

/// A title meter for the cases the terminal's title is beside the point of.
pub(super) fn title() -> TitleMeter {
    spied_title().0
}

/// The scratch directories the cases on this thread have been handed, removed with
/// everything in them when the thread ends.
///
/// The test harness runs every case on a thread of its own and lets that thread end once the
/// case is over, passed or failed (an assertion that panics unwinds the case, not the thread's
/// exit), so the directories go with the case that made them, however it ended. A case hands
/// its scratch path on to a [`Look`] or an [`Opener`], which keep it for the rest of the case,
/// so the path is held here rather than by a guard each case would have to keep alive.
struct Scratch(Vec<PathBuf>);

impl Drop for Scratch {
    fn drop(&mut self) {
        for dir in &self.0 {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = const { RefCell::new(Scratch(Vec::new())) };
}

/// A configuration file under a directory of this case's own, so nothing a case saves goes
/// anywhere near a real configuration. The directory is made by the writer itself, and only
/// a case that changes a setting ever has one; whichever it is, it is removed when the case
/// ends (see [`Scratch`]).
pub(super) fn scratch_config() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let at = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("svolder-tui-run-{}-{at}", std::process::id()));
    SCRATCH.with(|scratch| scratch.borrow_mut().0.push(dir.clone()));
    dir.join("config.toml")
}

/// How every loop case draws: the 16 ANSI colours with the colour taken out, so the
/// animation layer is inert and no case turns on how long an animation runs. Its browser refuses
/// every link and runs nothing, so a case that clicks one never starts a real browser.
pub(super) fn look() -> Look {
    let mut look = Look::new(
        TuiConfig::default(),
        true,
        ColorDepth::Ansi16,
        scratch_config(),
    );
    look.browser = Arc::new(|_| Err("no browser in a test".to_string()));
    look.link_out = Box::new(|_| {});
    look.env = Arc::new(|_| None);
    look.copier = Arc::new(|_| crate::tui::clipboard::Tried::NoDisplay);
    look
}

/// An app on the profile the backend reads its device through, with no device list.
pub(super) fn app_for(backend: &Backend) -> App {
    App::new(
        backend.profile().clone(),
        "fake".into(),
        &TuiConfig::default(),
        Vec::new(),
    )
}

/// A backend over the fake device, the gate that severs its link, and an app holding
/// everything the device reports now.
pub(super) async fn fake(connect_deadline: Duration) -> (Backend, Arc<FakeGate>, App) {
    let (mut backend, gate) = Backend::fake(connect_deadline).await;
    let mut app = app_for(&backend);
    let values = backend.snapshot().await.expect("the fake device answers");
    app.replace_all(values, std::time::Instant::now());
    (backend, gate, app)
}

/// What a case starts the loop watching: the device it opened, with its feed open and no
/// daemon version request of its own outstanding.
pub(super) fn watching(backend: Backend, app: App, device: Option<&str>) -> Watching {
    Watching {
        backend: Some(backend),
        app: Box::new(app),
        device: device.map(str::to_string),
        feed_open: true,
        daemon_version: None,
        web_code: None,
        web_check: None,
        web_copy: None,
        web_copy_next: None,
        reconnect: None,
        host_refresh: None,
        host: HostLink::none(),
        jobs: super::jobs::Jobs::default(),
    }
}

/// The devices a case switches between, as a daemon holding both links reports them.
pub(super) fn device_rows() -> Vec<crate::backend::DeviceRow> {
    vec![
        crate::backend::DeviceRow {
            name: "gungnir".into(),
            product: Some("Gungnir 2".into()),
            connected: Some(true),
            retry_in_secs: None,
            ..crate::backend::DeviceRow::default()
        },
        crate::backend::DeviceRow {
            name: "bifrost".into(),
            product: Some("Bifrost 3".into()),
            connected: Some(false),
            retry_in_secs: None,
            ..crate::backend::DeviceRow::default()
        },
    ]
}

/// What a test opener records: the device each open was asked for, and whether the device
/// the loop held had already been released by then.
pub(super) type Log = Arc<Mutex<Vec<(Option<String>, bool)>>>;

/// An opener over the fake device that records what it was asked for.
///
/// `held` is the gate of the device the loop started on, so every open says whether that
/// device had been released before another was opened: a Forkbeard device serves one
/// central at a time. `refuse` names the devices this opener will not open.
pub(super) fn fake_open(
    log: Log,
    held: Arc<FakeGate>,
    refuse: &'static [&'static str],
) -> impl Open {
    move |globals: OwnedGlobals| {
        let log = Arc::clone(&log);
        let held = Arc::clone(&held);
        async move {
            let device = globals.device.clone();
            log.lock()
                .expect("log mutex")
                .push((device.clone(), held.closed()));
            if device.as_deref().is_some_and(|name| refuse.contains(&name)) {
                return Err(Failure::of(Kind::Dropped, REFUSED));
            }
            let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
            Ok((backend, device_rows()))
        }
    }
}

/// An opener over the stub daemon on a socket, which opens as the TUI's own does (a state read
/// for the device named) and holds as it does ([`Backend::hold`]), so a case can have the
/// daemon not find its device and then find it.
pub(super) struct Stubbed(pub(super) PathBuf);

impl Open for Stubbed {
    async fn open(&self, globals: OwnedGlobals, _: &Path) -> Result<super::Opened, Failure> {
        let backend = Backend::stub_remote_named(&self.0, globals.device.as_deref()).await?;
        Ok((backend, device_rows()))
    }

    async fn open_on(
        &self,
        socket: PathBuf,
        globals: OwnedGlobals,
    ) -> Result<super::Opened, Failure> {
        let backend = Backend::stub_remote_named(&socket, globals.device.as_deref()).await?;
        Ok((backend, device_rows()))
    }

    async fn hold(
        &self,
        socket: Option<PathBuf>,
        globals: OwnedGlobals,
        wanted: Option<String>,
        config: &Path,
    ) -> Option<crate::backend::Held> {
        let socket = socket.unwrap_or_else(|| self.0.clone());
        Backend::hold(&globals.borrow(), Some(&socket), wanted.as_deref(), config).await
    }
}

/// An opener a case that never switches device would have to have gone wrong to reach.
pub(super) fn never_opens() -> impl Open {
    |_: OwnedGlobals| async { Err(Failure::of(Kind::Dropped, "this case opens no device")) }
}

/// The command line a case opens under, as `-v` was or was not typed.
pub(super) fn globals(verbose: bool) -> OwnedGlobals {
    OwnedGlobals {
        transport: "fake".into(),
        adapter: None,
        socket: None,
        device: None,
        verbose,
    }
}

/// An opener over `open`, with no device named, the default drawing settings and a
/// configuration file of this case's own.
pub(super) fn opener_over<O: Open>(open: O) -> Opener<O> {
    Opener::new(globals(false), TuiConfig::default(), scratch_config(), open)
}

/// How a case walks someone through adding a device: over the same fake machine the wizard's
/// own cases are walked against, so no case here reaches a radio, a daemon or this host.
pub(super) struct Adds {
    /// The machine the walk applies its answers to.
    pub(super) effects: FakeEffects,
}

impl Adding for Adds {
    type Effects = FakeEffects;

    async fn wizard(&self) -> Result<Wizard<FakeEffects>, Failure> {
        Ok(Wizard::starting_at(
            self.effects.clone(),
            crate::wizard::Step::Scan,
            crate::wizard::Given::default(),
        ))
    }
}

/// The devices every open was asked for, and whether the loop's own was released first.
pub(super) fn opens(log: &Log) -> Vec<(Option<String>, bool)> {
    log.lock().expect("log mutex").clone()
}

/// One key press as a terminal reports it.
pub(super) fn key(c: char) -> io::Result<crossterm::event::Event> {
    Ok(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        ),
    ))
}

/// One reading of a function the fake device never reports, so that a case can see
/// whether a refresh replaced what the app held or only added to it.
pub(super) fn absent_reading() -> svolder_ipc::ValueObject {
    svolder_ipc::ValueObject {
        heard_at_ms: None,
        id: ABSENT.to_string(),
        key: None,
        label: "?".into(),
        value_type: "u8".into(),
        raw: serde_json::json!(7),
        display: "7".into(),
        value: None,
        unit: None,
    }
}

/// The frames drawn so far, copied out so a case holds nothing while it goes on.
pub(super) fn drawn(frames: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    frames.lock().expect("frames mutex").clone()
}

/// Whether any frame carries `text`.
pub(super) fn any_frame(frames: &[String], text: &str) -> bool {
    frames.iter().any(|frame| frame.contains(text))
}

/// What a fake default sink reads: `volume` (1.0 = 100 %), muted or not, with a name for people
/// and a node a capture could target.
pub(super) fn sink_state(volume: f32, muted: bool) -> SinkState {
    SinkState {
        volume,
        muted,
        name: "USB DAC".into(),
        target: Some("alsa_output.usb-dac".into()),
    }
}

/// What a fake player plays: `title` by Miles Davis, in `status`.
pub(super) fn track(title: &str, status: Status) -> NowPlaying {
    NowPlaying {
        genre: Vec::new(),
        title: title.into(),
        artist: "Miles Davis".into(),
        identity: "mpv".into(),
        status,
        ..Default::default()
    }
}

/// A search for the players that finds `players` at once.
pub(super) fn finds(players: FakePlayers) -> Connect {
    Box::new(move || {
        let players = players.clone();
        Box::pin(async move { Ok(Arc::new(players) as Arc<dyn Players>) })
    })
}

/// A search for the players that finds no session bus.
pub(super) fn no_bus() -> Connect {
    Box::new(|| Box::pin(async { Err(NoBus::Absent) }))
}

/// Link `watching` to a host whose sink is `sink` and whose players `connect` finds, as the
/// loop links a session to the host it runs on.
pub(super) fn host_on(watching: &mut Watching, sink: &FakeSink, connect: Connect) {
    watching.app.host.linked = true;
    watching.host = HostLink::over(Some(Arc::new(sink.clone())), connect);
}

/// Have the host `watching` is linked to capture its sink's monitor through `capture`.
pub(super) fn capture_on(watching: &mut Watching, capture: &FakeCapture) {
    watching.host = std::mem::take(&mut watching.host).with_capture(Box::new(capture.clone()));
}

/// A block of the sink's output reading `left` and `right` on its two channels, with every band
/// at the louder of the two.
pub(super) fn block(left: f32, right: f32) -> Block {
    Block {
        bands: [left.max(right); svolder_host::fft::BANDS],
        left,
        right,
    }
}

/// One drawn turn of the loop over `app`, as `draw_frame` in `run.rs` draws one, with the clock
/// handed in: the theme pointed at the device's family and level, the app told the terminal's
/// size, a screen change the user asked for starting its passage, the frame drawn and painted
/// over (the intro's exit included) and kept as what the terminal shows, what it earned armed,
/// and whether the Spectrum row is on the screen. The header clock, the mascot, the title and
/// the tooltip are the caller's.
///
/// The README screenshots draw every frame through this, so it takes the same steps in the same
/// order as the loop's own turn.
pub(in crate::tui) fn drawn_turn<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    app: &mut App,
    look: &mut Look,
    now: std::time::Instant,
    last_tick: Duration,
) -> (ratatui::buffer::Buffer, crate::tui::render::FrameAreas) {
    super::look::colour(look, app);
    if let Ok(size) = terminal.size() {
        app.resize(size.width, size.height);
    }
    if std::mem::take(&mut app.frame_flags.navigated) {
        look.effects.on_navigation();
    }
    let mut hits = crate::tui::mouse::HitMap::default();
    let mut areas = crate::tui::render::FrameAreas::default();
    let shown = terminal
        .draw(|frame| {
            areas = crate::tui::render::draw(frame, app, &look.theme, &mut hits);
            super::look::paint_over(frame, &areas, &look.theme, &mut look.effects, last_tick);
        })
        .expect("a test terminal draws")
        .buffer
        .clone();
    look.effects.keep_shown(&shown);
    super::look::arm_effects(app, &mut look.effects, &areas, &look.theme, now);
    app.host.spectrum_on_screen = areas.spectrum.is_some();
    (shown, areas)
}

/// Hands that answer from what a case sets, and count what they were asked.
#[derive(Default)]
pub(super) struct FakeHands {
    /// The index text lines `index` answers with, `None` for a failed fetch.
    pub index_lines: Option<Vec<String>>,
    /// The text `download` answers with, `None` for a failure.
    pub text: Option<String>,
    /// Why the computer EQ cannot run, `None` where it can.
    pub off: Option<String>,
    /// The computer EQ's state file.
    pub state: Mutex<Option<HostEqState>>,
    /// When the state file last changed.
    pub stamp: Mutex<Option<SystemTime>>,
    /// The policies `index` was asked for.
    pub fetched: Mutex<Vec<Refresh>>,
    /// The names `apply` was asked for.
    pub applied: Mutex<Vec<String>>,
    /// How many times `remove` was asked.
    pub removed: AtomicUsize,
}

impl Hands for FakeHands {
    fn index(&self, refresh: Refresh) -> Result<Index, String> {
        self.fetched.lock().expect("the fetch log").push(refresh);
        match &self.index_lines {
            Some(lines) => Ok(Index {
                entries: svolder_autoeq::parse_index(&lines.join("\n")),
                fetched: std::time::UNIX_EPOCH,
                note: None,
            }),
            None => Err("no network".into()),
        }
    }

    fn download(&self, _: &Entry) -> Result<String, String> {
        self.text.clone().ok_or_else(|| "no network".to_string())
    }

    fn apply(&self, request: ApplyRequest) -> BoxFuture<'static, Result<HostEqState, Failure>> {
        self.applied
            .lock()
            .expect("the apply log")
            .push(request.name.clone());
        let state = HostEqState {
            on: true,
            name: request.name,
            origin: request.origin,
            target: "alsa_output.test".into(),
            route: Route::Default,
            manager: Manager::Process,
            previous_default: None,
            preamp_db: request.correction.preamp_db,
            filters: request.correction.filters,
        };
        *self.state.lock().expect("the state") = Some(state.clone());
        Box::pin(async move { Ok(state) })
    }

    fn remove(&self) -> BoxFuture<'static, Result<(), Failure>> {
        self.removed.fetch_add(1, Ordering::SeqCst);
        if let Some(state) = self.state.lock().expect("the state").as_mut() {
            state.on = false;
        }
        Box::pin(async { Ok(()) })
    }

    fn state(&self) -> Option<HostEqState> {
        self.state.lock().expect("the state").clone()
    }

    fn stamp(&self) -> Option<SystemTime> {
        *self.stamp.lock().expect("the stamp")
    }

    fn available(&self) -> Result<(), Failure> {
        match &self.off {
            Some(why) => Err(Failure::of(Kind::Usage, why.clone())),
            None => Ok(()),
        }
    }
}

/// An opener over `path`, which opens nothing.
pub(super) fn opener_on_nothing(path: PathBuf) -> Opener<impl Open> {
    Opener::new(globals(false), TuiConfig::default(), path, never_opens())
}

/// Carry out one intent against `look` and hand back what the loop watches.
pub(super) async fn taken<O: Open>(
    intent: Intent,
    opener: &Opener<O>,
    watching: Watching,
    look: &mut Look,
) -> Watching {
    let mut mouse = SpyMouse::default();
    match take_intent(intent, opener, watching, &mut mouse, look, &mut ()).await {
        Flow::Continue(watching) => watching,
        _ => panic!("the intent ended the loop or asked for a walk"),
    }
}

/// A scratch state home, removed when the case ends, with no token in it.
pub(super) fn state_home() -> PathBuf {
    let home = scratch_config()
        .parent()
        .expect("the scratch config has a directory")
        .join("state");
    std::fs::create_dir_all(&home).expect("the scratch state home is made");
    home
}

/// A look whose token is searched for under `home` with no environment token, and whose
/// clipboard writes into the returned record.
pub(super) fn look_over(home: &Path) -> (Look, Arc<Mutex<Vec<String>>>) {
    let mut look = look();
    look.token_site = TokenSite {
        env: None,
        env_error: None,
        web_env: None,
        web_env_error: None,
        state_home: home.to_path_buf(),
    };
    let sent = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&sent);
    look.clipboard = Box::new(move |sequence| {
        record
            .lock()
            .expect("the record")
            .push(sequence.to_string());
        Ok(())
    });
    (look, sent)
}

/// A watching over a stub daemon, on a configuration file the template starts.
pub(super) async fn on_a_daemon() -> (
    crate::test_support::stub_daemon::StubServer,
    PathBuf,
    Watching,
) {
    let daemon = stub_daemon().await;
    let path = scratch_config();
    Config::update_mcp(&path, |_| {}).expect("the scratch config is made");
    std::fs::write(&path, Config::template()).expect("the template is written");
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, None);
    let _ = relisted(&path, &mut watching.app);
    (daemon, path, watching)
}
