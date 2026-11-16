// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use svolder_ipc::config::TOOLTIPS;
use svolder_ipc::{
    ColorDepth, GraphSpan, LinkSpan, TUI_SETTINGS, Transition, TuiConfig, TuiSettingKind, TuiValue,
};
use svolder_profiles::Family;

use crate::tui::app::{App, EffectSwitch, Intent, Screen, SettingsBox, TuiChange};
use crate::tui::mouse::{self, HitMap, MouseAction};
use crate::tui::render::{self, FrameAreas};
use crate::tui::test_support::{
    click, frame, frame_with_theme, press, profile, text, theme_for, type_text,
};
use crate::tui::theme::Theme;

use super::*;

/// An app on the Device screen with the Settings box open over it, drawn from the defaults
/// every key case starts from.
fn app() -> App {
    let mut app = plain();
    press(&mut app, KeyCode::Char('s'));
    app
}

/// An app on the Device screen with no box open.
fn plain() -> App {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.config_path = home()
        .expect("the test user has a home directory")
        .join(".config/svolder/config.toml");
    app
}

/// The row of the plain `[tui]` key the file spells `name`.
fn key(name: &str) -> Setting {
    Setting::Key(
        TUI_SETTINGS
            .into_iter()
            .find(|key| key.key == name)
            .unwrap_or_else(|| panic!("no key {name}")),
    )
}

/// Put the box on `setting`'s category and row.
fn select(app: &mut App, setting: Setting) {
    let (category, (_, rows)) = CATEGORIES
        .iter()
        .enumerate()
        .find(|(_, (_, rows))| rows.contains(&setting))
        .expect("every setting is in a category");
    let row = rows
        .iter()
        .position(|row| *row == setting)
        .expect("the setting is in its category");
    app.overlays.settings = Some(SettingsBox { category, row });
}

/// Every setting the box lists, category by category.
fn every_setting() -> Vec<Setting> {
    CATEGORIES
        .iter()
        .flat_map(|(_, rows)| rows.iter().copied())
        .collect()
}

/// One frame of `app` at `width` by `height`, with where it painted the box and the hits it
/// recorded.
fn drawn(app: &App, width: u16, height: u16) -> (Buffer, FrameAreas, HitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, &Theme::new(false), &mut hits))
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), areas, hits)
}

/// The 22 rows of the box at 80×24, from its top border to its bottom one.
fn box_rows(app: &App) -> Vec<String> {
    text(&frame(app, 80, 24))[2..24].to_vec()
}

/// What a left click at `(column, row)` asks for against the frame `app` draws at 80×24.
fn clicked(app: &App, column: u16, row: u16) -> Option<MouseAction> {
    let (_, _, hits) = drawn(app, 80, 24);
    mouse::resolve(&hits, click(column, row), app)
}

/// The Settings box on Look at 80×24, as `docs/usage/tui.md` shows it: the 22 rows from the
/// top border to the bottom one.
const LOOK_MOCKUP: [&str; 22] = [
    "┌ Settings ─ tab→ ─┤ ¹Look ├─ ²Effects ─ ³Behaviour ─ ⁴Host audio ─ ⁵Connect ──┐",
    "│ ▶ Palette             ‹ amber ›           │                                  │",
    "│   Accent · DAC          (preset)          │                                  │",
    "│   Accent · amp          (preset)          │  The colour preset every screen  │",
    "│   Accent · preamp       (preset)          │  draws from: amber, ice, red or  │",
    "│   Colours               auto              │  neon. Each names an accent per  │",
    "│   Monochrome            off               │  device family and the good,     │",
    "│   Theme background      on                │  warn and alert colours.         │",
    "│   Graphs                on                │                                  │",
    "│   Graph span            5m                │  [tui] palette = \"amber\"         │",
    "│   Link span             12h               │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "└ ~/.config/svolder/config.toml ──────────────────────────────────── esc close ┘",
];

// The Connect category. Every token here is a fixed fake one, compared with `assert!` so a
// failing case never prints it.

use std::time::Duration;

use svolder_ipc::config::McpAuth;
use svolder_ipc::token::TokenSource;
use svolder_ipc::{McpStatus, MqttStatus};

use crate::backend::DaemonInfo;
use crate::tui::app::ConnChange;
use crate::tui::boxes::connections::{self, Field, Line, Modal, TokenView};

/// The token every Connect case holds: 43 characters ending `3fQ`, not a real one.
const FAKE_TOKEN: &str = "fake-token-0123456789-abcdefghijklmnop-x3fQ";

/// An app with the Settings box open on the Connect category, as it is once the loop has read
/// the file, the token's tail and the daemon's answer.
fn connect_app() -> App {
    let mut app = app();
    app.overlays.settings = Some(SettingsBox {
        category: CONNECT,
        row: 0,
    });
    app.session.daemon = true;
    app.overlays.conn.token = TokenView::Present {
        masked: connections::mask(FAKE_TOKEN),
        source: TokenSource::Generated("/home/user/.local/state/svolder/mcp-token".into()),
    };
    app.overlays.conn.mcp_status = Some(McpStatus::Listening {
        listen: "127.0.0.1:8737".parse().unwrap(),
        auth: McpAuth::Token,
        writes: false,
    });
    app.overlays.conn.mqtt_status = Some(MqttStatus::Off);
    app
}

/// What the daemon says of itself, about this session's own configuration file.
fn daemon_with_config(config: Option<&str>) -> DaemonInfo {
    DaemonInfo {
        version: "0.0.0".into(),
        mqtt: None,
        mcp: None,
        config: config.map(str::to_string),
        mcp_token: None,
        web: None,
        web_token: None,
        features: Vec::new(),
        config_changed: false,
        config_missing: false,
    }
}

/// The text of the box's rows at 80×24 joined, for a case that asks whether something shows.
fn shown(app: &App) -> String {
    box_rows(app).join("\n")
}

/// The box's right column at 80×24 as one line of words, wrapped lines rejoined with a space.
fn right_words(app: &App) -> String {
    box_rows(app)[1..21]
        .iter()
        .map(|row| row.chars().skip(45).take(33).collect::<String>())
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

mod connect;
mod look;
mod mqtt;
mod rows;
mod web_box;
