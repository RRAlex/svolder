// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{self, BoxStream};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::backend::{DeviceRow, FakeGate};
use crate::test_support::stub_daemon::stub_daemon;
use crate::tui::app::Intent;
use crate::tui::mouse::HitMap;
use crate::tui::render;
use crate::tui::theme::Theme;
use crate::wizard::fake;
use crate::wizard::fake::FakeEffects;

use super::super::test_support::*;
use super::super::*;
use super::*;

/// The unit id of the device a walk finds on the air.
const ADDED_UDID: &str = "1112131415161718";

/// An opener that reaches the stub daemon on `socket`, asking it for whatever device the
/// loop named, and records what it was asked for.
///
/// `held` is the gate of the device the loop started on, as [`fake_open`]'s is.
fn remote_opener(log: Log, held: Arc<FakeGate>, socket: PathBuf) -> impl Open {
    move |globals: OwnedGlobals| {
        let log = Arc::clone(&log);
        let held = Arc::clone(&held);
        let socket = socket.clone();
        async move {
            let device = globals.device.clone();
            log.lock()
                .expect("log mutex")
                .push((device.clone(), held.closed()));
            let backend = Backend::stub_remote_named(&socket, device.as_deref()).await?;
            Ok((backend, device_rows()))
        }
    }
}
/// An opener over `open` reading and writing the configuration file at `path`.
fn opener_on<O: Open>(path: PathBuf, open: O) -> Opener<O> {
    Opener::new(globals(false), TuiConfig::default(), path, open)
}
/// A walk over a host with one device on the air, writing what it adds into `path`.
fn adds(path: PathBuf) -> Adds {
    Adds {
        effects: FakeEffects::healthy()
            .with_scan(vec![fake::seen_product(ADDED_UDID, "Bifrost 3", -50)])
            .writing_into(path),
    }
}
/// One device as the configuration file has it, with a unit id of its own.
fn device_config(name: &str, at: u8) -> DeviceConfig {
    DeviceConfig {
        name: name.into(),
        udid: format!("010203040506070{at}"),
        profile: None,
        passphrase: "auto".into(),
        inputs: std::collections::BTreeMap::new(),
    }
}
/// A configuration file of this case's own, naming `names` in the order given, written by
/// the very writer that wrote them on the machine.
fn config_naming(names: &[&str]) -> PathBuf {
    let path = scratch_config();
    for (at, name) in names.iter().enumerate() {
        Config::add_device(&path, &device_config(name, u8::try_from(at).unwrap_or(0)))
            .expect("the scratch config takes a device");
    }
    path
}
/// What the configuration file at `path` names now, in order.
fn named(path: &Path) -> Vec<String> {
    Config::load_from(path)
        .expect("the config loads")
        .devices
        .into_iter()
        .map(|device| device.name)
        .collect()
}
/// Carry out one intent and hand back what the loop is watching, for the cases about the
/// writes the device list makes.
async fn taken<O: Open>(intent: Intent, opener: &Opener<O>, watching: Watching) -> Flow {
    let mut mouse = SpyMouse::default();
    take_intent(intent, opener, watching, &mut mouse, &mut look(), &mut ()).await
}
/// What the loop is watching after an intent that does not end it or ask for a walk.
fn carried_on(flow: Flow) -> Watching {
    match flow {
        Flow::Continue(watching) => watching,
        Flow::Adding(_) => panic!("the intent asked for a walk"),
        Flow::Ended(_) => panic!("the intent ended the loop"),
    }
}
/// One frame of `app`, for a case that asks what the loop would draw next.
fn frame_of(app: &App) -> String {
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, app, &Theme::new(true), &mut hits);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// One `Enter` as a terminal reports it.
fn enter() -> io::Result<Event> {
    Ok(Event::Key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Enter,
    )))
}
/// A stream of the events `script` names, a moment apart, so a wizard step is answered
/// after the frame that asked it has been drawn.
fn pressed(script: Vec<io::Result<Event>>) -> BoxStream<'static, io::Result<Event>> {
    Box::pin(stream::iter(script).then(|event| async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        event
    }))
}
/// Only a daemon's rows are asked for again, and only while the list shows them: a direct
/// session's rows are the file's, which say nothing about how a device is doing.
#[tokio::test]
async fn only_a_daemon_s_list_on_screen_is_asked_again() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.screen = Screen::Devices;
    let direct = watching(backend, app, None);
    assert!(
        !relisting(&direct),
        "a direct session's list was asked again"
    );
    direct.close().await;

    let daemon = stub_daemon().await;
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut remote = watching(backend, app, None);
    assert!(!relisting(&remote), "the list was asked for off its screen");
    remote.app.screen = Screen::Devices;
    assert!(
        relisting(&remote),
        "the daemon's list on screen was not asked again"
    );
    remote.close().await;
}
/// Asking the daemon again takes what it says about every device it lists, the wait before
/// a next attempt included, and leaves a device it does not list as the list had it.
#[tokio::test]
async fn asking_the_daemon_again_takes_its_rows_and_keeps_the_ones_it_does_not_list() {
    use crate::test_support::stub_daemon::{CANONICAL, OTHER};

    let daemon = stub_daemon().await;
    *daemon.other_retry().lock().expect("the wait") = Some(30);
    let backend = Backend::stub_remote(&daemon.socket).await;
    let mut app = app_for(&backend);
    app.devices.rows = vec![
        DeviceRow::for_test(CANONICAL),
        DeviceRow::for_test(OTHER),
        DeviceRow::for_test("unheld"),
    ];
    let mut watching = watching(backend, app, None);

    relist_held(&mut watching).await;

    let rows = &watching.app.devices.rows;
    assert_eq!(rows[0].connected, Some(true));
    assert_eq!(rows[0].product.as_deref(), Some("Gungnir 2"));
    assert_eq!(
        (rows[1].connected, rows[1].retry_in_secs),
        (Some(false), Some(30))
    );
    assert_eq!(
        rows[2],
        DeviceRow::for_test("unheld"),
        "an unlisted row changed"
    );
    let facts = watching
        .app
        .session
        .facts
        .as_ref()
        .expect("the open device's facts");
    assert_eq!(
        (facts.name.as_str(), facts.product.as_deref()),
        (CANONICAL, Some("Gungnir 2")),
        "the Status pane's facts did not follow the same answer"
    );
    watching.close().await;
}
/// An edited device is written through the configuration writer, the daemon holding the
/// devices is told to read the file again, and the list is drawn from what is on disk.
#[tokio::test]
async fn an_edited_device_is_written_to_the_file_and_the_daemon_told() {
    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir", "bifrost"]);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let mut app = app_for(&backend);
    app.session.device_name = "bifrost".into();
    let mut watching = watching(backend, app, Some("bifrost"));
    let _ = relisted(&path, &mut watching.app);

    let edited = DeviceConfig {
        name: "dac".into(),
        passphrase: "1234".into(),
        ..device_config("gungnir", 0)
    };
    let watching = carried_on(
        taken(
            Intent::UpdateDevice("gungnir".into(), edited),
            &opener,
            watching,
        )
        .await,
    );

    assert_eq!(
        named(&path),
        ["dac", "bifrost"],
        "the file was not rewritten"
    );
    assert_eq!(
        watching.app.devices.configured[0].passphrase, "1234",
        "the editor's own view is not what the file says"
    );
    assert_eq!(
        watching
            .app
            .devices
            .rows
            .iter()
            .map(|row| row.name.clone())
            .collect::<Vec<_>>(),
        ["dac", "bifrost"],
        "the list is not what the file says"
    );
    assert_eq!(daemon.reloads(), 1, "the daemon was not told to reload");
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        None,
        "{:?}",
        watching.app.toast().map(|toast| toast.text.clone())
    );
    watching.close().await;
}
/// The preset list's writes take the device editor's path: each is written through its
/// configuration writer into the file this session reads, a daemon that reads that file is told
/// to read it again, and the list is drawn from what is on disk.
#[tokio::test]
async fn the_preset_lists_writes_reach_the_file_the_daemon_and_the_list() {
    use svolder_profiles::EqMode;
    use svolder_profiles::tone::preset::{Preset, PresetBand};

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    daemon.says(None, path.to_str());
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, None);
    let _ = relisted(&path, &mut watching.app);
    assert!(watching.app.presets.saved.is_empty());
    let curve = |name: &str, g: f64| Preset {
        name: name.into(),
        source: None,
        bands: 5,
        mode: EqMode::Eq,
        loudness: 0,
        band: vec![
            PresetBand {
                f: 1000.0,
                g,
                q: 1.0
            };
            5
        ],
    };
    let on_disk = |path: &Path| -> Vec<(String, f64)> {
        Config::load_from(path)
            .expect("the config loads")
            .presets
            .into_iter()
            .map(|preset| (preset.name, preset.band[0].g))
            .collect()
    };

    let steps = [
        (
            Intent::SavePreset(curve("custom 1", 2.0)),
            vec![("custom 1".to_string(), 2.0)],
        ),
        (
            Intent::UpdatePreset("custom 1".into(), curve("custom 1", -1.5)),
            vec![("custom 1".to_string(), -1.5)],
        ),
        (
            Intent::RenamePreset("custom 1".into(), "HD650 warm".into()),
            vec![("HD650 warm".to_string(), -1.5)],
        ),
        (Intent::RemovePreset("HD650 warm".into()), Vec::new()),
    ];
    for (reloads, (intent, wanted)) in (1..).zip(steps) {
        let what = format!("{intent:?}");
        watching = carried_on(taken(intent, &opener, watching).await);
        assert_eq!(on_disk(&path), wanted, "{what}: the file");
        let listed: Vec<(String, f64)> = watching
            .app
            .presets
            .saved
            .iter()
            .map(|preset| (preset.name.clone(), preset.band[0].g))
            .collect();
        assert_eq!(listed, wanted, "{what}: the list is not what the file says");
        assert_eq!(daemon.reloads(), reloads, "{what}: the daemon was not told");
        assert_eq!(
            watching.app.toast().map(|toast| toast.text.clone()),
            None,
            "{what}: {:?}",
            watching.app.toast().map(|toast| toast.text.clone())
        );
    }
    assert_eq!(
        named(&path),
        ["gungnir"],
        "a preset write touched the devices"
    );
    watching.close().await;
}
/// A daemon that reads another file, or names none, is not told to reload when a preset is
/// written: the write reaches the file and the list, and nothing is said.
#[tokio::test]
async fn a_preset_write_does_not_reload_a_daemon_that_reads_another_file() {
    use svolder_profiles::EqMode;
    use svolder_profiles::tone::preset::{Preset, PresetBand};

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    let other = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, None);
    let preset = Preset {
        name: "evening".into(),
        source: None,
        bands: 5,
        mode: EqMode::Eq,
        loudness: 0,
        band: vec![
            PresetBand {
                f: 1000.0,
                g: 1.0,
                q: 1.0
            };
            5
        ],
    };
    for reads in [other.to_str(), None] {
        daemon.says(None, reads);
        watching = carried_on(taken(Intent::SavePreset(preset.clone()), &opener, watching).await);
        assert_eq!(daemon.reloads(), 0, "{reads:?}: the daemon was told");
        watching =
            carried_on(taken(Intent::RemovePreset("evening".into()), &opener, watching).await);
        assert_eq!(daemon.reloads(), 0, "{reads:?}: the daemon was told");
    }
    assert!(
        watching.app.presets.saved.is_empty(),
        "the list is not the file"
    );
    assert_eq!(watching.app.toast().map(|t| t.text.clone()), None);
    watching.close().await;
}
/// Opening the preset list reads the file again, so a preset another front end wrote since
/// is listed — and focused, being the curve the device carries — and a file that no longer
/// reads is a notice, with the list still up on what the session held.
#[tokio::test(start_paused = true)]
async fn opening_the_preset_list_reads_what_another_front_end_wrote() {
    use crossterm::event::{KeyCode, KeyEvent};
    use svolder_profiles::tone::preset;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let _ = relisted(&path, &mut app);
    app.show(Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    let theirs = preset::from_readings(&eq, |id| app.raw_of(id), "theirs")
        .expect("the fake reports its whole curve");
    Config::add_preset(&path, &theirs).expect("another front end writes it");
    assert!(app.presets.saved.is_empty(), "the session read it already");

    let intents = app.key(KeyEvent::from(KeyCode::Char('p')));
    assert_eq!(intents, [Intent::ReadPresets]);
    let mut watching = watching(backend, app, None);
    for intent in intents {
        watching = carried_on(taken(intent, &opener, watching).await);
    }
    let names: Vec<&str> = watching
        .app
        .presets
        .saved
        .iter()
        .map(|preset| preset.name.as_str())
        .collect();
    assert_eq!(names, ["theirs"]);
    assert_eq!(
        watching
            .app
            .presets
            .preset_list
            .as_ref()
            .map(|list| list.row),
        Some(4),
        "the preset the device carries is not focused"
    );

    std::fs::write(&path, "[[preset]\n").expect("the file is broken by hand");
    watching = carried_on(taken(Intent::ReadPresets, &opener, watching).await);
    assert!(
        watching.app.toast().is_some(),
        "an unreadable file said nothing"
    );
    assert!(
        watching.app.presets.preset_list.is_some(),
        "the list closed"
    );
    assert_eq!(
        watching.app.presets.saved.len(),
        1,
        "the session's presets were dropped"
    );
    watching.close().await;
}
/// A preset write the file refuses is a notice on the key line, as a device write is, and
/// the file is left as it was.
#[tokio::test(start_paused = true)]
async fn a_preset_write_the_file_refuses_is_a_notice() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let watching = carried_on(
        taken(
            Intent::RenamePreset("nobody".into(), "somebody".into()),
            &opener,
            watching(backend, app, None),
        )
        .await,
    );
    let notice = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .expect("a notice was set");
    assert!(notice.contains("nobody"), "{notice}");
    assert!(Config::load_from(&path).unwrap().presets.is_empty());
    watching.close().await;
}
/// Reading the file again keeps the device list's cursor on a row the list still has, and
/// leaves any other screen's focused row alone: a preset written from the device screen
/// does not move the Controls pane's cursor.
#[tokio::test(start_paused = true)]
async fn reading_the_file_again_moves_only_the_device_lists_own_cursor() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    backend.close().await;
    let path = config_naming(&["gungnir"]);
    app.show(Screen::Device);
    app.selected = 4;
    relisted(&path, &mut app).expect("the file reads");
    assert_eq!(app.selected, 4, "the Controls pane's cursor moved");
    app.show(Screen::Devices);
    app.selected = 4;
    relisted(&path, &mut app).expect("the file reads");
    assert_eq!(app.selected, 0, "the list's cursor is past its one row");
}
/// A write the file refuses is a notice on the key line: nothing crashes, and nothing is
/// quietly dropped.
#[tokio::test(start_paused = true)]
async fn a_write_the_file_refuses_is_a_notice() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());

    let watching = carried_on(
        taken(
            Intent::UpdateDevice("nobody".into(), device_config("nobody", 9)),
            &opener,
            watching(backend, app, None),
        )
        .await,
    );

    let notice = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .expect("a notice was set");
    assert!(notice.contains("nobody"), "{notice}");
    assert_eq!(named(&path), ["gungnir"], "the file was written anyway");
    watching.close().await;
}
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
    watching.host.send(super::super::host::SinkOp::Read);

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

/// The unit id the opener's devices all report.
const OPENED_UNIT: &str = "0102030405060708";

/// An app watching a device whose unit id is `unit`, holding one history sample, switched to
/// `name`; the app that replaces it, with the id of the sample.
async fn switched_with_history(unit: &str, name: &str) -> (Watching, svolder_proto::FunctionId) {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    let opener = opener_over(fake_open(Log::default(), Arc::clone(&gate), &[]));
    app.session.unit = Some(unit.into());
    let id = "99.9".parse().expect("a function id");
    app.readings
        .histories
        .record(&app.profile, id, std::time::Instant::now(), 3.0);
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.app.devices.rows = device_rows();
    let switched = switch_device(&opener, &mut watching, name, &look(), &mut ()).await;
    assert_eq!(switched, Switched::Opened);
    assert_eq!(watching.app.session.unit.as_deref(), Some(OPENED_UNIT));
    (watching, id)
}

/// A reopen of the same device (the unit id the old app had) carries the histories on.
#[tokio::test(start_paused = true)]
async fn a_reopen_of_the_same_device_keeps_the_histories() {
    let (watching, id) = switched_with_history(OPENED_UNIT, "gungnir").await;
    assert!(watching.app.readings.histories.reading(id).is_some());
    watching.close().await;
}

/// A switch to a device with another unit id starts the histories empty.
#[tokio::test(start_paused = true)]
async fn a_switch_to_another_device_starts_the_histories_empty() {
    let (watching, id) = switched_with_history("0807060504030201", "bifrost").await;
    assert!(watching.app.readings.histories.reading(id).is_none());
    watching.close().await;
}

/// A reopen over a daemon that knows `history` seeds the strip from its changes.
#[tokio::test]
async fn a_reopen_over_a_daemon_seeds_the_link_strip_from_its_history() {
    use svolder_ipc::LinkChange;

    let daemon = stub_daemon().await;
    daemon.history(vec![
        LinkChange {
            at_ms: 1_000,
            connected: true,
        },
        LinkChange {
            at_ms: 9_000,
            connected: false,
        },
    ]);
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener.reopen(None, None).await.expect("the daemon opens");
    let at = |ms| {
        watching
            .app
            .readings
            .link
            .columns(ms, Duration::from_secs(10), 10)
    };
    assert_eq!(at(10_000)[1], Some(1.0));
    assert_eq!(at(10_000)[9], Some(0.0), "the daemon's last word is down");
    assert_eq!(daemon.history_spans(), [24 * 3600]);
    watching.close().await;
}

/// An older daemon's error leaves the strip on what this session saw: the link as it was when
/// the session opened.
#[tokio::test]
async fn a_reopen_over_an_older_daemon_falls_back_to_its_own_observations() {
    crate::tui::widgets::graph::hold_wall_ms(50_000);
    let daemon = stub_daemon().await;
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener.reopen(None, None).await.expect("the daemon opens");
    let columns = watching
        .app
        .readings
        .link
        .columns(55_000, Duration::from_secs(10), 10);
    assert_eq!(columns[..5], [None; 5]);
    assert_eq!(columns[5..], [Some(1.0); 5]);
    watching.close().await;
}

/// A daemon with no record of the device does not erase what this session saw: the carried
/// link history stays and the link is recorded up as of the reopen.
#[tokio::test]
async fn an_empty_daemon_history_keeps_the_carried_link() {
    crate::tui::widgets::graph::hold_wall_ms(50_000);
    let daemon = stub_daemon().await;
    daemon.history(Vec::new());
    let (spare, gate, mut old) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    old.session.unit = Some(OPENED_UNIT.into());
    old.readings.link.record(10_000, true);
    old.readings.link.record(20_000, false);
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener
        .reopen(None, Some(&old))
        .await
        .expect("the daemon opens");
    let columns = watching
        .app
        .readings
        .link
        .columns(60_000, Duration::from_secs(60), 6);
    assert_eq!(
        columns,
        vec![None, Some(1.0), Some(0.0), Some(0.0), Some(0.0), Some(1.0)]
    );
    watching.close().await;
}

/// A same-device reopen over a restarted daemon keeps what the TUI held: the daemon's one change
/// ("up since the restart") joins it, and the gap between, which the old feed's close recorded,
/// draws down.
#[tokio::test]
async fn a_reopen_over_a_restarted_daemon_keeps_the_carried_strip_and_shows_the_gap() {
    use svolder_ipc::LinkChange;

    crate::tui::widgets::graph::hold_wall_ms(50_000);
    let daemon = stub_daemon().await;
    daemon.history(vec![LinkChange {
        at_ms: 40_000,
        connected: true,
    }]);
    let (spare, gate, mut old) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    old.session.unit = Some(OPENED_UNIT.into());
    old.readings.link.record(1_000, true);
    old.readings.link.record(10_000, false);
    old.readings.link.record(20_000, true);
    old.readings.link.record(30_000, false);
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener
        .reopen(None, Some(&old))
        .await
        .expect("the daemon opens");
    let columns = watching
        .app
        .readings
        .link
        .columns(50_000, Duration::from_secs(50), 5);
    assert_eq!(
        columns,
        vec![Some(1.0), Some(0.0), Some(1.0), Some(0.0), Some(1.0)]
    );
    watching.close().await;
}
/// Carry out `intents` one after the other, as the loop does for one key.
async fn carried_all<O: Open>(
    intents: Vec<Intent>,
    opener: &Opener<O>,
    mut watching: Watching,
) -> Watching {
    for intent in intents {
        watching = carried_on(taken(intent, opener, watching).await);
    }
    watching
}

/// The preset list's AutoEq search and the computer's recall through the loop with a fake
/// network and a fake host: the intents start jobs, the jobs' ends come back the way the loop's own arm takes them,
/// and the import lands in the configuration file as an imported preset, unloaded.
#[tokio::test]
async fn the_preset_list_imports_from_autoeq_and_puts_it_on_the_computer_through_the_loop() {
    use crossterm::event::{KeyCode, KeyEvent};

    use crate::tui::run::jobs::{Jobs, next};
    use crate::tui::run::test_support::FakeHands;

    let hands = Arc::new(FakeHands {
        index_lines: Some(vec![
            "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990"
                .into(),
        ]),
        text: Some("Preamp: -3.0 dB\nFilter 1: ON PK Fc 1000 Hz Gain 2.0 dB Q 1.0\n".into()),
        ..FakeHands::default()
    });
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let _ = relisted(&path, &mut app);
    app.show(Screen::Tone);
    let mut watching = watching(backend, app, None);
    watching.jobs = Jobs::over(hands.clone());

    let press = async |mut watching: Watching, code: KeyCode| {
        let intents = watching.app.key(KeyEvent::from(code));
        carried_all(intents, &opener, watching).await
    };
    for code in [KeyCode::Char('p'), KeyCode::Char('a')] {
        watching = press(watching, code).await;
    }
    // The index job ends; its answer is folded in where the loop's arm folds it.
    let done = next(&mut watching.jobs.ended).await;
    let intents = presets::done(&mut watching.app, done);
    watching = carried_all(intents, &opener, watching).await;
    for letter in "hd650".chars() {
        watching = press(watching, KeyCode::Char(letter)).await;
    }
    watching = press(watching, KeyCode::Enter).await;
    let done = next(&mut watching.jobs.ended).await;
    let intents = presets::done(&mut watching.app, done);
    watching = carried_all(intents, &opener, watching).await;

    let saved = Config::load_from(&path).expect("the config loads").presets;
    let [imported] = saved.as_slice() else {
        panic!("expected one preset in the file, got {saved:?}");
    };
    assert_eq!(imported.name, "Sennheiser HD 650");
    assert_eq!(
        imported
            .source
            .as_ref()
            .and_then(|source| source.origin.as_deref()),
        Some("AutoEq · oratory1990 · over-ear")
    );
    assert_eq!(
        watching.app.presets.loaded_preset, None,
        "the import was loaded"
    );

    // On the computer's side `Enter` on it sends it to the loop's own controller, whose view
    // names it as the one on the computer.
    let chain = Arc::new(svolder_host::fake::FakeChain::new());
    watching.computer = Box::new(crate::tui::run::computer::ComputerLink::over(
        svolder_host::eq::EqControl::new(
            chain.clone(),
            Box::new(svolder_host::fake::FakeParams::new()),
            svolder_host::eq::Keep::Memory,
        ),
    ));
    watching.app.host.computer.eq = watching.computer.view().map(Box::new);
    watching = press(watching, KeyCode::Esc).await;
    watching = press(watching, KeyCode::Char('C')).await;
    assert!(watching.app.on_computer());
    watching = press(watching, KeyCode::Char('p')).await;
    for _ in 0..20 {
        watching = press(watching, KeyCode::Down).await;
    }
    watching = press(watching, KeyCode::Enter).await;
    let seen = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let view = crate::tui::run::computer::next_view(&mut watching.computer.views).await;
            if view.on {
                return view;
            }
        }
    })
    .await
    .expect("the controller applied it");
    assert_eq!(seen.preset.as_deref(), Some("Sennheiser HD 650"));
    assert!(
        chain
            .calls()
            .iter()
            .any(|call| matches!(call, svolder_host::fake::ChainCall::Apply(_)))
    );
    watching.close().await;
}

/// The toast the loop is showing, if any.
fn toast_of(watching: &Watching) -> Option<String> {
    watching.app.toast().map(|toast| toast.text.clone())
}

/// Saving the editor says, right then, that the passphrase it wrote does not match the code the
/// device advertises, when the daemon holding the device says so of the entry it took in, and
/// says nothing when it does not: the editor has no advertisement to judge a code by as it is
/// typed, so the note comes with the save.
#[tokio::test]
async fn saving_a_passphrase_the_daemon_judges_a_mismatch_says_so_at_once() {
    use crate::test_support::stub_daemon::{CANONICAL, OTHER};
    use svolder_ipc::PassphraseNote;

    let daemon = stub_daemon().await;
    let path = config_naming(&[CANONICAL, OTHER]);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, Some(CANONICAL));
    let _ = relisted(&path, &mut watching.app);

    let edited = DeviceConfig {
        passphrase: "9999".into(),
        ..device_config(CANONICAL, 0)
    };
    daemon.notes(Some(PassphraseNote::Differs));
    let mut watching = carried_on(
        taken(
            Intent::UpdateDevice(CANONICAL.into(), edited.clone()),
            &opener,
            watching,
        )
        .await,
    );
    assert_eq!(
        toast_of(&watching).as_deref(),
        Some("fake: the passphrase does not match the code the device advertises")
    );

    // A save the daemon has no note for says nothing.
    daemon.notes(None);
    watching.app.dismiss_toast();
    let watching = carried_on(
        taken(
            Intent::UpdateDevice(CANONICAL.into(), edited),
            &opener,
            watching,
        )
        .await,
    );
    assert_eq!(toast_of(&watching), None);

    // The note is the one of the device the editor saved, not of the one being watched: the
    // daemon has one for the watched device and none for the other, which is the one saved.
    daemon.notes(Some(PassphraseNote::Differs));
    let other = DeviceConfig {
        passphrase: "9999".into(),
        ..device_config(OTHER, 1)
    };
    let watching =
        carried_on(taken(Intent::UpdateDevice(OTHER.into(), other), &opener, watching).await);
    assert_eq!(toast_of(&watching), None);
    watching.close().await;
}

/// A direct session judges the entry just saved against the advertisement it holds, for the
/// device it is on and no other: `auto` earns nothing, and a code the device's own does not
/// match is said.
#[tokio::test]
async fn saving_a_passphrase_a_direct_session_judges_a_mismatch_says_so_at_once() {
    let (mut backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
    backend.configured_as("fake");
    let app = app_for(&backend);
    let path = config_naming(&["fake", "other"]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, Some("fake"));
    let _ = relisted(&path, &mut watching.app);

    let says = |name: &str, passphrase: &str| {
        let entry = DeviceConfig {
            passphrase: passphrase.into(),
            ..device_config(name, 0)
        };
        Intent::UpdateDevice(name.into(), entry)
    };
    // The fake device's own code is 1234.
    for (name, passphrase, said) in [
        ("fake", "9999", true),
        ("fake", "1234", false),
        ("fake", "auto", false),
        ("other", "9999", false),
    ] {
        watching.app.dismiss_toast();
        watching = carried_on(taken(says(name, passphrase), &opener, watching).await);
        assert_eq!(
            toast_of(&watching).is_some(),
            said,
            "{name} {passphrase}: {:?}",
            toast_of(&watching)
        );
        // The Status pane's copy follows the session that took the entry in, for the device the
        // session is on.
        if name == "fake" {
            assert_eq!(
                watching.app.session.passphrase_note.is_some(),
                said,
                "{name} {passphrase}: the Status pane's note is stale"
            );
        }
    }
    watching.close().await;
}

/// An app the opener builds names the file the opener reads and the look writes, not
/// whatever the environment resolves to.
#[tokio::test(start_paused = true)]
async fn a_reopened_app_shows_the_file_its_opener_reads() {
    let (_backend, gate, _app) = fake(SHORT_DEADLINE).await;
    let path = scratch_config();
    let opener = opener_on(
        path.clone(),
        fake_open(Log::default(), Arc::clone(&gate), &[]),
    );
    let watching = opener
        .reopen(Some("gungnir"), None)
        .await
        .expect("the device opens");
    assert_eq!(watching.app.config_path, path);
    watching.close().await;
}

/// The landing app names the opener's file as well.
#[tokio::test(start_paused = true)]
async fn a_landing_app_shows_the_file_its_opener_reads() {
    let (_backend, gate, _app) = fake(SHORT_DEADLINE).await;
    let path = scratch_config();
    let opener = opener_on(path.clone(), fake_open(Log::default(), gate, &[]));
    let watching = land(&opener, Landing::Devices { why: None }).await;
    assert_eq!(watching.app.config_path, path);
    watching.close().await;
}

/// A reopen over a daemon that keeps readings starts the graphs with them: the volume's series
/// holds the daemon's samples before what this session reads itself.
#[tokio::test]
async fn a_reopen_over_a_daemon_seeds_the_graphs_from_its_readings() {
    let daemon = stub_daemon().await;
    let volume: svolder_proto::FunctionId = "10".parse().expect("the volume's id");
    daemon.readings(svolder_ipc::Response::Readings {
        device: "fake".into(),
        now_ms: 100_000,
        bin_ms: 1000,
        series: vec![svolder_ipc::SeriesOut {
            id: "10".into(),
            key: Some("volume".into()),
            aggregate: svolder_profiles::history::Aggregate::Max,
            points: vec![(40_000, 40.0), (90_000, 50.0)],
        }],
        recalls: Vec::new(),
        link: Vec::new(),
    });
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener.reopen(None, None).await.expect("the daemon opens");
    let series = watching
        .app
        .readings
        .histories
        .reading(volume)
        .expect("the volume's series");
    let values: Vec<f64> = series.samples().map(|(_, value)| value).collect();
    assert_eq!(values[..2], [40.0, 50.0], "{values:?}");
    assert_eq!(
        series.aggregate(),
        svolder_profiles::history::Aggregate::Max
    );
    let (first, _) = series.samples().next().expect("a sample");
    assert!(
        first.elapsed() >= Duration::from_secs(59),
        "the first sample is a minute old: {:?}",
        first.elapsed()
    );
    watching.close().await;
}

/// A same-device reopen keeps the histories it carries as they are: the daemon's readings, which
/// the carried ones already cover, are not laid before them.
#[tokio::test]
async fn a_reopen_that_carries_the_histories_does_not_seed_them_again() {
    let daemon = stub_daemon().await;
    let volume: svolder_proto::FunctionId = "10".parse().expect("the volume's id");
    daemon.readings(svolder_ipc::Response::Readings {
        device: "fake".into(),
        now_ms: 100_000,
        bin_ms: 1000,
        series: vec![svolder_ipc::SeriesOut {
            id: "10".into(),
            key: Some("volume".into()),
            aggregate: svolder_profiles::history::Aggregate::Max,
            points: vec![(40_000, 40.0)],
        }],
        recalls: Vec::new(),
        link: Vec::new(),
    });
    let (spare, gate, mut old) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    old.session.unit = Some(OPENED_UNIT.into());
    let profile = old.profile.clone();
    old.readings
        .histories
        .record(&profile, volume, Instant::now(), 7.0);
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener
        .reopen(None, Some(&old))
        .await
        .expect("the daemon opens");
    let series = watching
        .app
        .readings
        .histories
        .reading(volume)
        .expect("the volume's series");
    assert!(
        series.samples().all(|(_, value)| value != 40.0),
        "the carried series was seeded again"
    );
    watching.close().await;
}

/// A daemon's memory names the preset at connect, and a recall another front end makes while the
/// session runs replaces it in the header.
#[tokio::test]
async fn the_header_follows_the_daemons_recall_memory() {
    let daemon = stub_daemon().await;
    daemon.recalled("Warm");
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let mut watching = opener.reopen(None, None).await.expect("the daemon opens");
    assert_eq!(watching.app.presets.loaded_preset.as_deref(), Some("Warm"));
    daemon.push_recalled("Bass");
    let backend = watching.backend.as_mut().expect("a backend");
    let update = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match backend.next_status().await {
                Ok(Some(Update::Recalled(name))) => return name,
                Ok(Some(_)) => continue,
                other => panic!("the feed gave {other:?}"),
            }
        }
    })
    .await
    .expect("a recalled update");
    watching.app.apply(Update::Recalled(update), Instant::now());
    assert_eq!(watching.app.presets.loaded_preset.as_deref(), Some("Bass"));
    watching.close().await;
}
