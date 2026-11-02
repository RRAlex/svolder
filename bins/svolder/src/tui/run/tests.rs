// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loop around a device, by topic: what keys and the mouse write, the link going down and
//! coming back, the daemon's side and the waits. The helpers here are the ones several topics use;
//! the tests are in the topic files listed below.

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream::{self, BoxStream};
use serde_json::json;

use svolder_ipc::ValueObject;
use svolder_profiles::Registry;
use svolder_proto::{FunctionId, Value};

use crate::tui::theme::Theme;
use crate::wizard::fake::FakeEffects;

use super::test_support::*;
use super::*;

// the link: feed, refresh, reopen, strip
mod link;
// the loop over a daemon
mod daemon;
// the Devices screen and landing
mod devices;
// keys that write to the device
mod writes;
// streamed levels in the title
mod meters;
// keys that end the loop
mod keys;
// the exit and its frames
mod exit;
// clicks and the wheel
mod mouse;
// dragging the tone curve
mod curve;
// the pointer at rest and the web link
mod pointer;
// the daemon version toasts
mod toasts;
// waiting for a device
mod waiting;
// the daemon dying under the loop
mod daemon_loss;

/// How long a case waits for the echo of a write before it gives up.
const ECHO_DEADLINE: Duration = Duration::from_secs(10);

/// The function address the Input control sits on.
const INPUT: FunctionId = FunctionId::one(2);

/// A walk no case takes: the key that asks for one is not among the ones they press, so
/// being asked for a wizard at all is the case having gone wrong.
fn nothing_to_add() -> Adds {
    Adds {
        effects: FakeEffects::healthy(),
    }
}

/// One left click at `(column, row)`, as a terminal reports it.
fn left_click(column: u16, row: u16) -> io::Result<Event> {
    Ok(Event::Mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }))
}

/// A stream that reports `qq` `after` a delay and nothing before it: the `q` that asks
/// and the second `q` that answers the question it asks.
///
/// The delay is what lets a case say when the loop is interrupted: with the clock paused
/// it costs nothing, and until it passes the loop has only its own tick and the device to
/// wake on.
fn quit_after(after: Duration) -> BoxStream<'static, io::Result<Event>> {
    Box::pin(
        stream::once(async move {
            tokio::time::sleep(after).await;
            key('q')
        })
        .chain(stream::iter(vec![key('q')])),
    )
}

/// A write no backend will take: function 210.1 is the firmware-update family.
fn refused_write() -> Intent {
    Intent::Raw {
        id: "210.1".into(),
        value_type: "u8".into(),
        value: "0".into(),
    }
}

/// The raw value the app has for one function, once it has heard one.
fn raw_of(app: &App, id: FunctionId) -> Option<i64> {
    app.readings.values.get(&id)?.raw_i64()
}

/// The index of the first frame carrying `text`.
fn first_frame(frames: &[String], text: &str) -> usize {
    frames
        .iter()
        .position(|frame| frame.contains(text))
        .unwrap_or_else(|| panic!("no frame showed {text:?}"))
}

/// Whether `frame` shows the spinner's glyph, whichever step of its cycle it is on: every
/// glyph it turns through is a braille pattern, and nothing else this binary draws is.
fn has_spinner(frame: &str) -> bool {
    frame
        .chars()
        .any(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
}

/// Where `wanted` first appears in one frame of `app` drawn at [`SIZE`], top to bottom:
/// the screen coordinate a click there reaches, the way the mouse itself reports one.
fn coord_of(app: &App, wanted: &str) -> (u16, u16) {
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(SIZE.0, SIZE.1))
        .expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer();
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).expect("a cell").symbol())
            .collect();
        if let Some(byte_at) = row.find(wanted) {
            let x = row[..byte_at].chars().count();
            return (u16::try_from(x).unwrap_or(0), y);
        }
    }
    panic!("{wanted:?} is on no row of the frame");
}

/// One mouse event of `kind` at `(column, row)`, as a terminal reports it.
fn mouse_at(kind: crossterm::event::MouseEventKind, column: u16, row: u16) -> io::Result<Event> {
    Ok(Event::Mouse(crossterm::event::MouseEvent {
        kind,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }))
}

/// An opener over the stub daemon on `socket`, which waits `wait` for a daemon that went away.
fn daemon_opener(socket: &std::path::Path, wait: Duration) -> Opener<impl Open + use<>> {
    let socket = socket.to_path_buf();
    Opener::new(
        globals(false),
        TuiConfig::default(),
        scratch_config(),
        move |_: OwnedGlobals| {
            let socket = socket.clone();
            async move {
                let backend = Backend::stub_remote(&socket).await;
                Ok((backend, Vec::new()))
            }
        },
    )
    .waiting_for_the_daemon(wait)
}

/// A daemon's answer that says its version and nothing else.
fn said(version: &str) -> DaemonInfo {
    DaemonInfo {
        version: version.to_string(),
        mqtt: None,
        mcp: None,
        config: None,
        mcp_token: None,
        web: None,
        web_token: None,
    }
}

/// A configuration file naming the one device `fake`, the unit id the stub daemon knows it by.
fn config_with_fake() -> PathBuf {
    let path = scratch_config();
    Config::add_device(
        &path,
        &svolder_ipc::DeviceConfig {
            name: "fake".into(),
            udid: "0102030405060708".into(),
            profile: None,
            passphrase: "auto".into(),
            inputs: std::collections::BTreeMap::new(),
        },
    )
    .expect("the scratch config takes a device");
    path
}
