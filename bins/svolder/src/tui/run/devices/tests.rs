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

/// The unit id the opener's devices all report.
const OPENED_UNIT: &str = "0102030405060708";

/// Take `intent`, then the end of the preset write it queued, as the loop's own arm folds it.
async fn written_through<O: Open>(
    intent: Intent,
    opener: &Opener<O>,
    mut watching: Watching,
) -> Watching {
    watching = carried_on(taken(intent, opener, watching).await);
    let done = crate::tui::run::jobs::next(&mut watching.jobs.ended).await;
    let intents = folded(done, opener, &mut watching);
    assert!(intents.is_empty(), "a preset write asked for more writes");
    watching
}

/// The toast the loop is showing, if any.
fn toast_of(watching: &Watching) -> Option<String> {
    watching.app.toast().map(|toast| toast.text.clone())
}

mod reopen;
mod saves;
mod saving_wait;
mod switching;
mod waits;
mod writes;
