// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Settings box: every choice the `[tui]` table holds, and what each of them says now.
//!
//! The box is a modal over the screen the user was on, opened by `s`. Its rows are the table's
//! own keys, grouped in four categories under tabs on its top border (a fifth, Connect, is
//! described below); the left column lists a category's rows and the right one explains the
//! selected row and names the key it is in the file. Each row asks for one
//! [`TuiChange`](crate::tui::app::TuiChange): the loop applies it to the running theme and
//! animation layer and writes it back to the file whose path is on the box's bottom border, so
//! what is on the screen and what is in the file never part company.
//!
//! A fifth category, Connect, holds the daemon's `[mcp]` and `[mqtt]` sections instead: each
//! row asks for one [`ConnChange`](crate::tui::app::ConnChange), written to the file key by key
//! and followed by a `reload`, and the status rows say what the daemon answers. Its token row
//! is masked, and only the loop ever reads the token itself.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use svolder_ipc::config::RefreshWeeks;
use svolder_ipc::config::{
    COLORS, CONFIRM_QUIT, GRAPH_SPAN, GRAPHS, HOST_AUDIO, INTRO, INTRO_EFFECT, INTRO_SOUND,
    LINK_SPAN, MONOCHROME, MOUSE, PALETTE, THEME_BACKGROUND, TITLE_METER, TRANSITION, VISUALIZER,
};
use svolder_ipc::token::TokenSource;
use svolder_ipc::{FamilyAccents, McpStatus, TuiConfig, TuiSetting, TuiSettingKind};
use svolder_profiles::Family;

use crate::tui::app::{App, EffectSwitch, SettingsBox};
use crate::tui::boxes::connections::{Connections, Field, Modal, TokenState, TokenView};
use crate::tui::boxes::web_box::{self, WebBox, WebRow};
use crate::tui::keys;
use crate::tui::layout_tier;
use crate::tui::mouse::{Hit, HitMap, Selected};
use crate::tui::theme::{Role, Surface, Theme};
use crate::tui::widgets;

use super::devices::boxed;

/// What an accent row says for a family the config leaves to the palette.
const PRESET: &str = "(preset)";
/// What an accent row carries after the colour when the `[tui]` `accent` key is drawing every
/// family at once rather than this row's own.
const ALL_FAMILIES: &str = "(all)";
/// What the host audio row carries after its value while the environment turns host audio off
/// for this process, whatever the row says.
const HELD_OFF: &str = "(SVOLDER_HOST=off)";
/// What the Effects row carries after `on` while the terminal cannot show the animation layer.
const INERT: &str = "(inactive)";
/// Columns the left column takes: the cursor, the label, the value with its arrows and the key.
const LEFT_WIDTH: u16 = 43;
/// Columns a row's label field takes after the cursor, an animation's indent included.
const LABEL_FIELD: usize = 19;
/// Columns a value takes; a longer one is cut to one fewer and `…`.
const VALUE_FIELD: usize = 13;
/// Columns the arrows and the value take together: `‹ `, the value field and ` ›`.
const VALUE_ZONE: usize = VALUE_FIELD + 4;
/// Columns before the value zone: the cursor's three, the label field and a blank.
const ZONE_AT: usize = 3 + LABEL_FIELD + 1;
/// Columns an animation's own row is indented by, under the switch that turns them all.
const INDENT: usize = 2;
/// Cells between the right column's text and the rule on its left, and between the text and
/// the box's right border.
const TEXT_SIDE: u16 = 2;
/// Rows between the box's top border and the right column's first line.
const TEXT_TOP: u16 = 2;
/// Columns the edit line's box takes, borders and all.
const PROMPT_WIDTH: u16 = 56;
/// Rows it takes: the line, a blank and its keys between two borders.
const PROMPT_HEIGHT: u16 = 5;
/// Columns the edit line's label takes, the space before the text included.
const PROMPT_LABEL: usize = 8;
/// The digits the categories are numbered with on the top border.
const SUPERSCRIPTS: [char; 9] = ['¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

/// One row of the box: one key of the `[tui]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// A plain key of the `[tui]` table, which the table in `svolder-ipc` describes: its label,
    /// its help text and how it is read and changed.
    Key(&'static TuiSetting),
    /// Whether the animation layer runs at all.
    Effects,
    /// Whether one animation runs.
    Effect(EffectSwitch),
    /// Whether the decoration drifts along the palette's spectrum, glowing hotter as the
    /// level rises.
    HueDrift,
    /// Whether the tone curve's colours scroll round the hue circle (along the palette's own ramp
    /// on `red` and `neon`) rather than resting red at the lowest frequency and violet at the
    /// highest (or on that ramp).
    SpectrumDrift,
    /// Which palette preset the screens draw from.
    Palette,
    /// What one device family accents with.
    Accent(Family),
    /// Which colour depth the screens draw at.
    Colors,
    /// Whether mouse capture is on.
    Mouse,
    /// Whether the TUI reaches the host's own audio: what plays, its volume and its transport.
    HostAudio,
    /// How old the cached AutoEq index may get before it is fetched again.
    AutoEqRefresh,
    /// The web UI: its state, and the box that changes it.
    WebUi,
    /// Whether the daemon serves MCP.
    McpEnabled,
    /// The address the MCP server listens on.
    McpListen,
    /// Whether the MCP server offers the tools that change a device.
    McpWrites,
    /// The MCP token: masked, revealed, copied or rotated.
    McpToken,
    /// Where the daemon says its MCP server stands.
    McpStatus,
    /// Whether the daemon bridges to an MQTT broker.
    MqttEnabled,
    /// The broker's host.
    MqttHost,
    /// The broker's port.
    MqttPort,
    /// Whether the bridge takes commands from the broker.
    MqttCommands,
    /// Whether a broker password is set; never the password.
    MqttPassword,
    /// Where the daemon says its MQTT bridge stands.
    MqttStatus,
}

/// The box's categories, in the order their tabs are drawn, each with its rows in the order the
/// left column lists them.
pub const CATEGORIES: [(&str, &[Setting]); 5] = [
    (
        "Look",
        &[
            Setting::Palette,
            Setting::Accent(Family::Dac),
            Setting::Accent(Family::Amp),
            Setting::Accent(Family::Preamp),
            Setting::Colors,
            Setting::Key(&MONOCHROME),
            Setting::Key(&THEME_BACKGROUND),
            Setting::Key(&GRAPHS),
            Setting::Key(&GRAPH_SPAN),
            Setting::Key(&LINK_SPAN),
        ],
    ),
    (
        "Effects",
        &[
            Setting::Effects,
            Setting::Effect(EffectSwitch::Eased),
            Setting::Effect(EffectSwitch::Glow),
            Setting::Effect(EffectSwitch::Shimmer),
            Setting::Effect(EffectSwitch::Breathing),
            Setting::Effect(EffectSwitch::Frames),
            Setting::Effect(EffectSwitch::FaultPresence),
            Setting::Effect(EffectSwitch::PeakHold),
            Setting::Effect(EffectSwitch::ListFade),
            Setting::Effect(EffectSwitch::Mascot),
            Setting::HueDrift,
            Setting::SpectrumDrift,
            Setting::Key(&TRANSITION),
        ],
    ),
    (
        "Behaviour",
        &[
            Setting::Mouse,
            Setting::Key(&CONFIRM_QUIT),
            Setting::Key(&TITLE_METER),
            Setting::Key(&INTRO),
            Setting::Key(&INTRO_EFFECT),
            Setting::Key(&INTRO_SOUND),
            Setting::AutoEqRefresh,
        ],
    ),
    (
        "Host audio",
        &[Setting::HostAudio, Setting::Key(&VISUALIZER)],
    ),
    (
        "Connect",
        &[
            Setting::WebUi,
            Setting::McpEnabled,
            Setting::McpListen,
            Setting::McpWrites,
            Setting::McpToken,
            Setting::McpStatus,
            Setting::MqttEnabled,
            Setting::MqttHost,
            Setting::MqttPort,
            Setting::MqttCommands,
            Setting::MqttPassword,
            Setting::MqttStatus,
        ],
    ),
];

/// The index of the Connect category.
pub const CONNECT: usize = 4;

/// The rows of category `category`, or none for a number past the last.
pub fn rows(category: usize) -> &'static [Setting] {
    CATEGORIES.get(category).map_or(&[], |(_, rows)| rows)
}

/// The row `at` points at, held inside its category's rows.
pub fn current(at: SettingsBox) -> Setting {
    let rows = rows(at.category.min(CATEGORIES.len() - 1));
    rows[at.row.min(rows.len() - 1)]
}

impl Setting {
    /// What the row is called.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Setting::Key(key) => key.label,
            Setting::Effects => "Effects",
            Setting::Effect(EffectSwitch::Eased) => "Eased values",
            Setting::Effect(EffectSwitch::Glow) => "Selection glow",
            Setting::Effect(EffectSwitch::Shimmer) => "Shimmer",
            Setting::Effect(EffectSwitch::Breathing) => "Breathing borders",
            Setting::Effect(EffectSwitch::Frames) => "Focus runner",
            Setting::Effect(EffectSwitch::FaultPresence) => "Fault presence",
            Setting::Effect(EffectSwitch::PeakHold) => "Peak hold",
            Setting::Effect(EffectSwitch::ListFade) => "List fade",
            Setting::Effect(EffectSwitch::Mascot) => "Mascot",
            Setting::HueDrift => "Hue drift",
            Setting::SpectrumDrift => "Rainbow scroll",
            Setting::Palette => PALETTE.label,
            Setting::Accent(Family::Dac) => "Accent · DAC",
            Setting::Accent(Family::Amp) => "Accent · amp",
            Setting::Accent(Family::Preamp) => "Accent · preamp",
            Setting::Colors => COLORS.label,
            Setting::Mouse => MOUSE.label,
            Setting::HostAudio => HOST_AUDIO.label,
            Setting::AutoEqRefresh => "AutoEq refresh",
            Setting::WebUi => "Web UI",
            Setting::McpEnabled => "MCP server",
            Setting::McpListen => "MCP listen",
            Setting::McpWrites => "MCP writes",
            Setting::McpToken => "MCP token",
            Setting::McpStatus => "MCP status",
            Setting::MqttEnabled => "MQTT bridge",
            Setting::MqttHost => "MQTT broker host",
            Setting::MqttPort => "MQTT broker port",
            Setting::MqttCommands => "MQTT commands",
            Setting::MqttPassword => "MQTT password",
            Setting::MqttStatus => "MQTT status",
        }
    }

    /// Whether the row belongs to the Connect category, which edits the daemon's `[mcp]` and
    /// `[mqtt]` sections rather than `[tui]`.
    pub(crate) fn is_connection(self) -> bool {
        matches!(
            self,
            Setting::WebUi
                | Setting::McpEnabled
                | Setting::McpListen
                | Setting::McpWrites
                | Setting::McpToken
                | Setting::McpStatus
                | Setting::MqttEnabled
                | Setting::MqttHost
                | Setting::MqttPort
                | Setting::MqttCommands
                | Setting::MqttPassword
                | Setting::MqttStatus
        )
    }

    /// Whether the row is one animation's own, which is drawn indented under the switch that
    /// turns every one of them.
    fn indented(self) -> bool {
        matches!(self, Setting::Effect(_))
    }

    /// What the row says now.
    ///
    /// Mouse capture answers for what is on rather than for what the file asked for: `M` turns
    /// it too, and this row is where a user looks to see which way it is. So does an accent
    /// row: a single `accent` key draws all three families whatever the per-family keys hold,
    /// so every accent row says that colour and marks it [`ALL_FAMILIES`] rather than claiming
    /// the palette's own.
    ///
    /// The Effects row says `on (inactive)` where the switch is on but the terminal cannot show
    /// the animation layer, so nothing animates.
    ///
    /// The host audio row says what the file holds, and also says when `SVOLDER_HOST=off` holds
    /// host audio off for this process whatever the file holds.
    pub(crate) fn value(self, app: &App) -> String {
        let effects = &app.tui.effects;
        match self {
            Setting::Key(key) => key_value(key, &app.tui),
            Setting::Effects => match effects.enabled && !app.effects {
                true => format!("{} {INERT}", word(true)),
                false => word(effects.enabled),
            },
            Setting::Effect(switch) => word(switch.of(effects)),
            Setting::HueDrift => word(effects.hue_drift),
            Setting::SpectrumDrift => word(effects.spectrum_drift),
            Setting::Palette => app.tui.palette.clone(),
            Setting::Accent(family) => match app.tui.accent.as_deref() {
                Some(every) => format!("{every} {ALL_FAMILIES}"),
                None => family_accent(&app.tui.accents, family)
                    .unwrap_or(PRESET)
                    .to_string(),
            },
            Setting::Colors => app.tui.colors.name().to_string(),
            Setting::Mouse => word(app.mouse),
            Setting::HostAudio => match app.host.held_off {
                true => format!("{} {HELD_OFF}", word(app.tui.host_audio)),
                false => word(app.tui.host_audio),
            },
            Setting::AutoEqRefresh => refresh_word(app.presets.autoeq_weeks),
            // The state alone: the column is 13 cells.
            Setting::WebUi => web_box::state_word(
                app.overlays.conn.web_status.as_ref(),
                app.overlays.conn.daemon_up,
                app.session.daemon,
            ),
            Setting::McpEnabled => word(app.overlays.conn.mcp.enabled),
            Setting::McpListen => listen_word(app.overlays.conn.mcp.listen),
            Setting::McpWrites => access(app.overlays.conn.mcp.writes, "read/write"),
            Setting::McpToken => token_word(app),
            Setting::McpStatus => mcp_word(app),
            Setting::MqttEnabled => word(app.overlays.conn.mqtt.enabled),
            Setting::MqttHost => app
                .overlays
                .conn
                .mqtt
                .host
                .clone()
                .unwrap_or_else(|| NOT_SET.to_string()),
            Setting::MqttPort => app.overlays.conn.mqtt.port.to_string(),
            Setting::MqttCommands => match app.overlays.conn.mqtt.commands {
                true => "allowed".to_string(),
                false => "read-only".to_string(),
            },
            Setting::MqttPassword => match app.overlays.conn.mqtt.password_set {
                true => "set".to_string(),
                false => NOT_SET.to_string(),
            },
            Setting::MqttStatus => mqtt_word(app),
        }
    }
}

/// What a plain key's row says: `on` or `off` for a switch, the word for a word.
fn key_value(key: &TuiSetting, tui: &TuiConfig) -> String {
    match &key.kind {
        TuiSettingKind::Bool { get, .. } => word(get(tui)),
        TuiSettingKind::Word { get, .. } => get(tui).to_string(),
        TuiSettingKind::Accent => {
            debug_assert!(false, "{} has a row per family, not a key row", key.key);
            String::new()
        }
    }
}

/// Where a plain key lives in the configuration file, with the value `tui` holds now.
fn key_line(key: &TuiSetting, tui: &TuiConfig) -> String {
    match &key.kind {
        TuiSettingKind::Bool { get, .. } => format!("[tui] {} = {}", key.key, get(tui)),
        TuiSettingKind::Word { get, .. } => format!("[tui] {} = \"{}\"", key.key, get(tui)),
        TuiSettingKind::Accent => {
            debug_assert!(false, "{} has a row per family, not a key row", key.key);
            format!("[tui] {}", key.key)
        }
    }
}

/// What the AutoEq refresh row says: `never`, `1 week`, `2 weeks` … `99 weeks`.
fn refresh_word(weeks: RefreshWeeks) -> String {
    match weeks {
        RefreshWeeks::Never => "never".to_string(),
        RefreshWeeks::Weeks(1) => "1 week".to_string(),
        RefreshWeeks::Weeks(weeks) => format!("{weeks} weeks"),
    }
}

/// What the token row says while the token is on show; the token itself is in the right column.
const REVEALED: &str = "(revealed)";
/// What a row says of a value the file leaves out.
const NOT_SET: &str = "not set";

/// `on` as a write row says it: `read-only`, or `writes`.
fn access(on: bool, writes: &str) -> String {
    match on {
        true => writes.to_string(),
        false => "read-only".to_string(),
    }
}

/// The token row's value: the daemon's masked tail where its token is not the one this session
/// reads, else this session's own: masked, or `(revealed)`.
fn token_word(app: &App) -> String {
    if let TokenState::Differs { masked, .. } = app.overlays.conn.token_state() {
        return masked;
    }
    match (&app.overlays.conn.token, app.overlays.conn.shown(app.now)) {
        (TokenView::Present { .. }, Some(_)) => REVEALED.to_string(),
        (TokenView::Present { masked, .. }, None) => masked.clone(),
        (TokenView::Missing, _) => "not made yet".to_string(),
        (TokenView::Unreadable(_), _) => "unreadable".to_string(),
        (TokenView::Unread, _) => "…".to_string(),
    }
}

/// The listen row's value: `loopback:PORT` for the default address, which is a cell too wide as
/// written, and the address as written for any other (cut to the row if it is too long; the
/// right column has it whole).
fn listen_word(listen: std::net::SocketAddr) -> String {
    match listen.ip() == std::net::Ipv4Addr::LOCALHOST {
        true => format!("loopback:{}", listen.port()),
        false => listen.to_string(),
    }
}

/// The MCP status row's word.
fn mcp_word(app: &App) -> String {
    match (&app.overlays.conn.mcp_status, app.session.daemon) {
        (_, false) => "no daemon".to_string(),
        (None, true) => "…".to_string(),
        (Some(McpStatus::Off), _) => "off".to_string(),
        (Some(McpStatus::Listening { .. }), _) => "listening".to_string(),
        (Some(McpStatus::Failed { .. }), _) => "failed".to_string(),
        (Some(McpStatus::Unknown), _) => "unknown".to_string(),
    }
}

/// The MQTT status row's word: the first word of the bridge's own line.
fn mqtt_word(app: &App) -> String {
    match (&app.overlays.conn.mqtt_status, app.session.daemon) {
        (_, false) => "no daemon".to_string(),
        (None, true) => "…".to_string(),
        (Some(status), _) => crate::commands::daemon::status::mqtt_line(status)
            .trim_start_matches("mqtt: ")
            .split(' ')
            .next()
            .unwrap_or_default()
            .to_string(),
    }
}

/// What the box's right column says of `setting`: what it does and the values it takes.
pub fn explain(setting: Setting) -> Cow<'static, str> {
    Cow::Borrowed(match setting {
        Setting::Key(key) => return (key.help)(),
        Setting::Palette => return (PALETTE.help)(),
        Setting::Accent(Family::Dac) => {
            "What a DAC draws its accent in: the cursor, the rule under the header and the key \
             hints. (preset) keeps the palette's own colour; the arrows step through the \
             colours the palette gives the other two families, and back to a colour of your own."
        }
        Setting::Accent(Family::Amp) => {
            "What an amplifier draws its accent in: the cursor, the rule under the header and \
             the key hints. (preset) keeps the palette's own colour; the arrows step through \
             the colours the palette gives the other two families, and back to a colour of your \
             own."
        }
        Setting::Accent(Family::Preamp) => {
            "What a preamplifier draws its accent in: the cursor, the rule under the header and \
             the key hints. (preset) keeps the palette's own colour; the arrows step through \
             the colours the palette gives the other two families, and back to a colour of your \
             own."
        }
        Setting::Colors => return (COLORS.help)(),
        Setting::Effects => {
            "The animation layer as a whole: off stops every animation, the transitions \
             included. It reads on (inactive) where the terminal cannot show it, below true \
             colour or in monochrome."
        }
        Setting::Effect(EffectSwitch::Eased) => {
            "Whether a changed value's bar glides to its new length instead of jumping there. A \
             meter's bar never glides."
        }
        Setting::Effect(EffectSwitch::Glow) => {
            "Whether the row the cursor just left keeps a fading trail for about 300 ms."
        }
        Setting::Effect(EffectSwitch::Shimmer) => {
            "Whether a bright band crosses the row you just changed or moved onto, as feedback. \
             While you rest it crosses the selected row again after 2 s, 10 s, 30 s and 2 \
             minutes."
        }
        Setting::Effect(EffectSwitch::Breathing) => {
            "Whether the pane borders pulse with the level: the meters' while they stream, else \
             what the host plays. They stay still with neither."
        }
        Setting::Effect(EffectSwitch::Frames) => {
            "Whether a band of light runs once round the frame of the pane that takes the focus. \
             Its brightness and pace follow the level where there is one."
        }
        Setting::Effect(EffectSwitch::FaultPresence) => {
            "Whether a standing fault's row and the header border pulse slowly while the fault \
             stays active."
        }
        Setting::Effect(EffectSwitch::PeakHold) => {
            "Whether a marker lingers at a meter's recent peak and decays over about a second."
        }
        Setting::Effect(EffectSwitch::ListFade) => {
            "Rows of a list dim a little with their distance from the selected row, as btop does."
        }
        Setting::Effect(EffectSwitch::Mascot) => {
            "Whether Ratatoskr, the squirrel, may come out. It stays hidden until it is called."
        }
        Setting::HueDrift => {
            "Whether the pane borders, titles, the header rule and the key hints drift along the \
             palette's spectrum, cool to hot, glowing hotter as the level rises. The selection \
             and every value keep the resting accent."
        }
        Setting::SpectrumDrift => {
            "Whether the tone curve's colours scroll slowly round the hue circle, one round trip \
             every 20 s, rather than resting red at 20 Hz and violet at 20 kHz. On red and neon \
             the curve's own ramp slides along its colours and back instead."
        }
        Setting::Mouse => return (MOUSE.help)(),
        Setting::HostAudio => return (HOST_AUDIO.help)(),
        Setting::WebUi => {
            "The web UI serves this screen's controls to a browser on this computer or your \
             network. Enter opens its box: on and off, how far it reaches, a link or a pairing \
             code to sign a browser in, and its token."
        }
        Setting::McpEnabled => {
            "Whether the daemon serves the Model Context Protocol over HTTP, for AI assistants. \
             Off, it opens no port. The change is written to the file and the daemon reloads."
        }
        Setting::McpListen => {
            "The address the MCP server listens on. Enter types one: loopback keeps the server \
             on this machine."
        }
        Setting::McpWrites => {
            "Whether the server offers the tools that change a device. Read-only offers only \
             the tools that look."
        }
        Setting::McpToken => {
            "The bearer token a client sends. v shows it for 10 s, c copies it to the \
             clipboard (to the terminal, and to the display's tool where there is one), R \
             replaces it after a question."
        }
        Setting::McpStatus => "Where the daemon says its MCP server stands. The daemon answers.",
        Setting::MqttEnabled => {
            "Whether the daemon bridges its devices to an MQTT broker, for Home Assistant. It \
             needs a broker host."
        }
        Setting::MqttHost => "The broker's host name or address. Enter types it.",
        Setting::MqttPort => "The broker's port. Enter types it.",
        Setting::MqttCommands => {
            "Whether the bridge takes commands back from the broker and writes them to the \
             devices. Read-only only reports."
        }
        Setting::MqttPassword => {
            "Whether a broker password is set, as this session's environment and config file \
             say; the daemon's own environment may differ. It is never shown and never edited \
             here: put it in SVOLDER_MQTT_PASSWORD, or in the config file."
        }
        Setting::MqttStatus => "Where the daemon says its MQTT bridge stands. The daemon answers.",
        Setting::AutoEqRefresh => {
            "How old the cached AutoEq index may get before the search fetches it again: never, \
             or 1 to 99 weeks. Ctrl-R in the search fetches it at once whatever this says."
        }
    })
}

/// Where `setting` lives in the configuration file, with the value `tui` holds now: the table
/// and key, and `= value`. An accent a family leaves to the palette has no value to write, and
/// says so.
pub fn config_line(setting: Setting, tui: &TuiConfig) -> String {
    let effects = &tui.effects;
    let flag = |table: &str, key: &str, on: bool| format!("[{table}] {key} = {on}");
    let text = |table: &str, key: &str, value: &str| format!("[{table}] {key} = \"{value}\"");
    match setting {
        Setting::Key(key) => key_line(key, tui),
        Setting::Effects => flag("tui.effects", "enabled", effects.enabled),
        Setting::Effect(switch) => {
            let key = match switch {
                EffectSwitch::Eased => "eased",
                EffectSwitch::Glow => "glow",
                EffectSwitch::Shimmer => "shimmer",
                EffectSwitch::Breathing => "breathing",
                EffectSwitch::Frames => "frames",
                EffectSwitch::FaultPresence => "fault_presence",
                EffectSwitch::PeakHold => "peak_hold",
                EffectSwitch::ListFade => "list_fade",
                EffectSwitch::Mascot => "mascot",
            };
            flag("tui.effects", key, switch.of(effects))
        }
        Setting::HueDrift => flag("tui.effects", "hue_drift", effects.hue_drift),
        Setting::SpectrumDrift => flag("tui.effects", "spectrum_drift", effects.spectrum_drift),
        Setting::Palette => key_line(&PALETTE, tui),
        Setting::Accent(family) => {
            let (key, own) = (accent_key(family), tui.accents.get(family));
            match (tui.accent.as_deref(), own) {
                (Some(every), _) => text("tui", "accent", every),
                (None, Some(colour)) => text("tui.accents", key, colour),
                (None, None) => format!("[tui.accents] {key}  (not set)"),
            }
        }
        Setting::Colors => key_line(&COLORS, tui),
        Setting::Mouse => key_line(&MOUSE, tui),
        Setting::HostAudio => key_line(&HOST_AUDIO, tui),
        Setting::AutoEqRefresh => "[autoeq] refresh_weeks".to_string(),
        Setting::WebUi => "[web] enabled, listen".to_string(),
        Setting::McpEnabled => "[mcp] enabled".to_string(),
        Setting::McpListen => "[mcp] listen".to_string(),
        Setting::McpWrites => "[mcp] writes".to_string(),
        Setting::McpToken => "[mcp] token_file, or SVOLDER_MCP_TOKEN".to_string(),
        Setting::McpStatus => "from the daemon".to_string(),
        Setting::MqttEnabled => "[mqtt] enabled".to_string(),
        Setting::MqttHost => "[mqtt] host".to_string(),
        Setting::MqttPort => "[mqtt] port".to_string(),
        Setting::MqttCommands => "[mqtt] commands".to_string(),
        Setting::MqttPassword => "[mqtt] password, or SVOLDER_MQTT_PASSWORD".to_string(),
        Setting::MqttStatus => "from the daemon".to_string(),
    }
}

/// Where the AutoEq refresh lives in the configuration file, with the value it holds now.
fn autoeq_line(weeks: RefreshWeeks) -> String {
    match weeks {
        RefreshWeeks::Never => "[autoeq] refresh_weeks = \"never\"".to_string(),
        RefreshWeeks::Weeks(weeks) => format!("[autoeq] refresh_weeks = {weeks}"),
    }
}

/// Where a Connect row lives in the configuration file, with the value the file holds now. The
/// token and the password name where they come from and never a value.
fn connection_line(setting: Setting, conn: &Connections) -> String {
    let mcp = &conn.mcp;
    let mqtt = &conn.mqtt;
    match setting {
        Setting::WebUi => format!(
            "[web] enabled = {}, listen = \"{}\"",
            conn.web.enabled, conn.web.listen
        ),
        Setting::McpEnabled => format!("[mcp] enabled = {}", mcp.enabled),
        Setting::McpListen => format!("[mcp] listen = \"{}\"", mcp.listen),
        Setting::McpWrites => format!("[mcp] writes = {}", mcp.writes),
        Setting::MqttEnabled => format!("[mqtt] enabled = {}", mqtt.enabled),
        Setting::MqttHost => match &mqtt.host {
            Some(host) => format!("[mqtt] host = \"{host}\""),
            None => "[mqtt] host  (not set)".to_string(),
        },
        Setting::MqttPort => format!("[mqtt] port = {}", mqtt.port),
        Setting::MqttCommands => format!("[mqtt] commands = {}", mqtt.commands),
        other => config_line(other, &TuiConfig::default()),
    }
}

/// What `family` overrides its accent with, or `None` where the palette's own colour stands.
pub fn family_accent(accents: &FamilyAccents, family: Family) -> Option<&str> {
    accents.get(family)
}

/// The key `family`'s accent is written under in `[tui.accents]`.
fn accent_key(family: Family) -> &'static str {
    match family {
        Family::Dac => "dac",
        Family::Amp => "amp",
        Family::Preamp => "preamp",
    }
}

/// How many rows a category's index may reach, so that a row of one category and a row of
/// another are never the same place in a [`Selection`](crate::tui::mouse::Selection).
const SELECTION_STRIDE: usize = 1_000;

/// The words ending the box's bottom border, which a click presses `Esc` from.
const CLOSE_WORDS: &str = "esc close";

/// Draw the box over the body of `area`, and answer the cells it filled.
///
/// The box is a modal: everything around it is dimmed, and it records the rectangle it fills
/// and the mouse targets of its rows, of the `‹` and `›` on the selected one, of the
/// categories' names on its top border, and of the `esc close` that ends its bottom border.
/// A box over its rows (a Connect edit line, the token question) records its own rectangle too,
/// and the rows under it take no click.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    live_hits: &mut HitMap,
) -> Rect {
    let (_, [_, body]) = layout_tier::split(area);
    // A box open over the rows takes every key, and the rows under it take no click.
    live_hits.push_box(body);
    let mut ignored = HitMap::default();
    let hits = if app.overlays.conn.modal.is_none() {
        &mut *live_hits
    } else {
        &mut ignored
    };
    let at = app.overlays.settings.unwrap_or_default();
    let category = at.category.min(CATEGORIES.len() - 1);
    let rows = rows(category);
    let row = at.row.min(rows.len() - 1);

    widgets::surfaces::modal_ground_on(frame, body, theme, Surface::Pane);
    let inner_room = usize::from(body.width.saturating_sub(2));
    // `esc close` always shows: the path gives way to it, from the left.
    let close = format!(" {CLOSE_WORDS} ");
    let close_width = widgets::text::cells(&close);
    let path = fitted(
        &shorten(&app.config_path, home().as_deref()),
        inner_room.saturating_sub(close_width + 3),
    );
    let block = Block::bordered()
        .border_style(crate::tui::screens::device::border_style(app, theme))
        .title_bottom(Line::from(Span::styled(
            format!(" {path} "),
            theme.style(Role::Dim),
        )))
        .title_bottom(Line::from(Span::styled(close, theme.style(Role::Text))).right_aligned());
    let inner = block.inner(body);
    frame.render_widget(block, body);
    draw_tabs(frame, body, category, app, theme, hits);
    let close_x = body
        .right()
        .saturating_sub(1 + u16::try_from(close_width).unwrap_or(0));
    hits.push(
        Rect {
            x: close_x.max(body.x + 1),
            y: body.bottom().saturating_sub(1),
            width: u16::try_from(close_width)
                .unwrap_or(0)
                .min(body.width.saturating_sub(2)),
            height: 1,
        },
        Hit::EscMark,
    );

    // The left column, a rule, and the right column, its text set in from the rule, the border
    // and the top.
    let left_width = LEFT_WIDTH.min(inner.width);
    let divider = inner.x + left_width;
    let right = Rect {
        x: divider + 1,
        width: inner.right().saturating_sub(divider + 1),
        ..inner
    };
    let height = usize::from(inner.height);
    let start = widgets::scroll::scroll_start(rows.len(), height, row);
    let border = crate::tui::screens::device::border_style(app, theme);
    for y in inner.top()..inner.bottom() {
        frame.buffer_mut().set_string(divider, y, "│", border);
    }
    for (offset, (index, setting)) in rows.iter().enumerate().skip(start).take(height).enumerate() {
        let line_area = Rect {
            x: inner.x,
            y: inner.y + u16::try_from(offset).unwrap_or(u16::MAX),
            width: left_width,
            height: 1,
        };
        let focused = index == row;
        frame.render_widget(row_line(*setting, focused, app, theme), line_area);
        hits.push(line_area, Hit::SettingRow(index));
        if focused {
            hits.select(
                Selected::Settings,
                category * SELECTION_STRIDE + index,
                line_area,
            );
            let value = widgets::text::cut_to_cells(&setting.value(app), VALUE_FIELD);
            let zone = inner.x + u16::try_from(ZONE_AT).unwrap_or(0);
            let end = zone + 2 + u16::try_from(widgets::text::cells(&value)).unwrap_or(0);
            hits.push(
                Rect {
                    x: zone,
                    width: 2,
                    ..line_area
                },
                Hit::SettingPrev,
            );
            hits.push(
                Rect {
                    x: end,
                    width: 2,
                    ..line_area
                },
                Hit::SettingNext,
            );
        }
    }
    let column = Rect {
        width: left_width,
        height: u16::try_from(rows.len().min(height)).unwrap_or(0),
        ..inner
    };
    widgets::surfaces::fade_list(
        frame.buffer_mut(),
        app.list_fade,
        column,
        start,
        height,
        row,
        theme,
    );
    let margin = Rect {
        x: divider,
        width: 1,
        height: u16::try_from(rows.len().min(height)).unwrap_or(0),
        ..inner
    };
    widgets::scroll::scroll_marks(frame.buffer_mut(), margin, start, rows.len(), theme);

    let text = Rect {
        x: right.x + TEXT_SIDE,
        y: right.y + TEXT_TOP,
        width: right.width.saturating_sub(2 * TEXT_SIDE),
        height: right.height.saturating_sub(TEXT_TOP),
    };
    frame.render_widget(
        Paragraph::new(explanation(rows[row], app, theme, text.width)),
        text,
    );
    if let Some(modal) = &app.overlays.conn.modal {
        let box_area = draw_modal(frame, body, app, modal, theme, live_hits);
        live_hits.push_box(box_area);
    }
    body
}

/// The box the Connect category opens over its rows: the line a host, a port or an address is
/// typed into, or the question before the token is replaced. Answers the cells it fills.
fn draw_modal(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    modal: &Modal,
    theme: &Theme,
    live_hits: &mut HitMap,
) -> Rect {
    match modal {
        Modal::Editing(line) => {
            let box_area = boxed(area, PROMPT_WIDTH, PROMPT_HEIGHT);
            let across = usize::from(box_area.width.saturating_sub(2));
            let room = across.saturating_sub(1 + PROMPT_LABEL);
            let (shown, column) = widgets::text::typed_window(&line.text, line.caret, room);
            let label = match line.field {
                Field::Listen => "Listen",
                Field::Host => "Host",
                Field::Port => "Port",
            };
            let lines = vec![
                Line::from(vec![
                    Span::raw(" "),
                    widgets::rows::label(label, PROMPT_LABEL),
                    Span::raw(shown),
                ]),
                Line::default(),
                widgets::surfaces::modal_keys(&[("Enter", "saves"), ("Esc", "cancels")], theme),
            ];
            let title = format!(" {} ", line.field.title());
            let block = widgets::surfaces::modal_block(Block::bordered(), &title, app, theme);
            let inner = block.inner(box_area);
            widgets::surfaces::modal_ground(frame, box_area, theme);
            frame.render_widget(Paragraph::new(lines).block(block), box_area);
            let x = inner.x + u16::try_from(1 + PROMPT_LABEL + column).unwrap_or(u16::MAX);
            if x < inner.right() && inner.height > 0 {
                frame.set_cursor_position(Position { x, y: inner.y });
            }
            box_area
        }
        Modal::Web(web) => draw_web_box(frame, area, app, web, theme, live_hits),
        Modal::Rotating => {
            let lines = vec![
                Line::from(Span::raw(" Replace the MCP token? [y/N]")),
                Line::from(Span::raw(" every client needs the new one")),
                Line::default(),
                widgets::surfaces::modal_keys(
                    &[("y", "replaces"), ("Enter/n/Esc/q", "cancels")],
                    theme,
                ),
            ];
            let widest = lines.iter().map(Line::width).max().unwrap_or(0);
            let width = u16::try_from(widest + 2).unwrap_or(u16::MAX);
            let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
            let box_area = boxed(area, width, height);
            widgets::surfaces::modal_ground(frame, box_area, theme);
            frame.render_widget(
                Paragraph::new(lines).block(widgets::surfaces::modal_block(
                    Block::bordered(),
                    " MCP token ",
                    app,
                    theme,
                )),
                box_area,
            );
            box_area
        }
    }
}

/// The terminal cells `text` fills.
fn cell_count(text: &str) -> u16 {
    u16::try_from(widgets::text::cells(text)).unwrap_or(u16::MAX)
}

/// Columns the Web UI box takes, borders and all.
const WEB_WIDTH: u16 = 64;
/// Columns the Web UI box's labels take, the space after them included.
const WEB_LABEL: usize = 14;
/// What the box says under a link on show: the terminal's own link handling, where it has any.
const WEB_LINK_HINT: &str = "Ctrl+click opens it; Shift+right-click copies it";
/// What the box says under a link that replaced one a browser used.
const WEB_LINK_REPLACED: &str = "the last link no longer works; a fresh one is shown";

/// The Web UI box: six `label · value` lines, the link's QR code below them where the box has
/// room, the question before the token is replaced while it is up, and its keys. Answers the
/// cells it fills.
fn draw_web_box(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    web: &WebBox,
    theme: &Theme,
    live_hits: &mut HitMap,
) -> Rect {
    let (lines, link_lines) = web_box_lines(app, web, area, theme);
    let width = WEB_WIDTH.min(area.width);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    let block = widgets::surfaces::modal_block(Block::bordered(), " Web UI ", app, theme);
    let inner = block.inner(box_area);
    for (at, cells) in link_lines {
        let text = Rect {
            x: inner.x + 1,
            y: inner
                .y
                .saturating_add(u16::try_from(at).unwrap_or(u16::MAX)),
            width: cells,
            height: 1,
        };
        let seen = text.intersection(inner);
        if !seen.is_empty() {
            live_hits.push(seen, Hit::WebLink);
        }
    }
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    box_area
}

/// The Web UI box's lines, and the lines the link's text fills with each one's width in cells:
/// what a click opens.
fn web_box_lines(
    app: &App,
    web: &WebBox,
    area: Rect,
    theme: &Theme,
) -> (Vec<Line<'static>>, Vec<(usize, u16)>) {
    let conn = &app.overlays.conn;
    let across = usize::from(WEB_WIDTH.min(area.width).saturating_sub(2));
    let room = across.saturating_sub(WEB_LABEL + 1);
    let direct = conn.daemon_up != Some(true);
    let ask = |code: &Option<web_box::Code>, pairing: bool, makes: &str| match (code, direct) {
        (Some(code), _) if pairing => web_box::code_line(code, app.now),
        (Some(code), _) => web_box::timer(code, app.now),
        (None, true) if app.session.daemon && conn.daemon_up.is_none() => "…".to_string(),
        (None, true) => "the daemon serves the web UI".to_string(),
        (None, false) => makes.to_string(),
    };
    let token = match &conn.web_token {
        TokenView::Present { masked, .. } => masked.clone(),
        TokenView::Missing => "not made yet".to_string(),
        TokenView::Unreadable(_) => "unreadable".to_string(),
        TokenView::Unread => "…".to_string(),
    };
    let values = [
        (
            WebRow::State,
            web_box::state_word(conn.web_status.as_ref(), conn.daemon_up, app.session.daemon),
        ),
        (
            WebRow::WebUi,
            match conn.web.enabled {
                true => "on".to_string(),
                false => "off".to_string(),
            },
        ),
        (WebRow::Reach, web_box::reach(conn.web.listen).word()),
        (WebRow::Link, ask(&web.link, false, "l makes one")),
        (WebRow::Pairing, ask(&web.pair, true, "p makes one")),
        (WebRow::Token, token),
    ];
    let mut lines: Vec<Line<'static>> = Vec::new();
    // The lines the link's text fills, with each one's width in cells: what a click opens.
    let mut link_lines: Vec<(usize, u16)> = Vec::new();
    // The link's lines take the box's width less a cell of margin each side, so the text of a
    // wrapped link is whole on its lines and a selection of them is the link.
    let link_room = across.saturating_sub(2).max(1);
    let underline = match app.pointer.over_link {
        true => Modifier::UNDERLINED,
        false => Modifier::empty(),
    };
    for (row, value) in values {
        let style = match web.row == row {
            true => theme.style(Role::Selected),
            false => theme.style(Role::Text),
        };
        let live_link = web
            .link
            .as_ref()
            .filter(|c| c.until > app.now)
            .filter(|_| row == WebRow::Link);
        let mut chunks = value.chars().collect::<Vec<_>>();
        let first: String = chunks.drain(..room.min(chunks.len())).collect();
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                widgets::rows::label(row.label(), WEB_LABEL)
                    .content
                    .to_string(),
                style,
            ),
            Span::styled(first, style),
        ]));
        // A value longer than the box is wide goes on under its label.
        while !chunks.is_empty() {
            let part: String = chunks.drain(..room.min(chunks.len())).collect();
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(WEB_LABEL + 1)),
                Span::styled(part, style),
            ]));
        }
        // The link on lines of its own, as wide as they go, with nothing after its last piece.
        if let Some(link) = live_link {
            let text: Vec<char> = link.text.chars().collect();
            for part in text.chunks(link_room) {
                let part: String = part.iter().collect();
                link_lines.push((lines.len(), cell_count(&part)));
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(part, style.add_modifier(underline)),
                ]));
            }
            for part in widgets::text::wrap(WEB_LINK_HINT, across.saturating_sub(2)) {
                lines.push(Line::from(Span::styled(
                    format!(" {part}"),
                    theme.style(Role::Dim),
                )));
            }
            if web.link_replaced {
                for part in widgets::text::wrap(WEB_LINK_REPLACED, across.saturating_sub(2)) {
                    lines.push(Line::from(Span::styled(
                        format!(" {part}"),
                        theme.style(Role::Dim),
                    )));
                }
            }
        }
        if row == WebRow::Pairing && web.pair.is_some() {
            for part in [
                "guesses from the network can use a code up:",
                "the link is the fallback",
            ] {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(WEB_LABEL + 1)),
                    Span::styled(part, theme.style(Role::Dim)),
                ]));
            }
        }
    }
    if web.rotating {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            " Replace the web token? [Y/n]",
            theme.style(Role::Warn),
        )));
        lines.push(Line::from(Span::styled(
            " every signed-in browser is signed out",
            theme.style(Role::Warn),
        )));
        lines.push(widgets::surfaces::modal_keys(
            &[("Enter/y", "replaces"), ("Esc/n/q", "cancels")],
            theme,
        ));
    }
    lines.extend(web_qr_lines(app, web, area, across, lines.len(), theme));
    lines.push(Line::default());
    lines.push(widgets::surfaces::modal_keys(
        &[
            ("↑↓", "rows"),
            ("←→", "change"),
            ("Enter", "link/code"),
            ("l", "link"),
            ("c", "copy"),
        ],
        theme,
    ));
    lines.push(widgets::surfaces::modal_keys(
        &[
            ("p", "code"),
            ("r", "token"),
            ("click", "link opens"),
            ("Esc/q", "closes"),
        ],
        theme,
    ));
    (lines, link_lines)
}

/// What the Web UI box shows under its rows for a link on show: its QR code, black on white and
/// centred, where `area` has room for it beside the `above` lines the box already holds, the
/// words that say to widen the terminal where it has not, and on a loopback listen the words
/// that say a phone cannot reach it. Nothing without a link on show.
fn web_qr_lines(
    app: &App,
    web: &WebBox,
    area: Rect,
    across: usize,
    above: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let conn = &app.overlays.conn;
    let Some(link) = web.link.as_ref().filter(|link| link.until > app.now) else {
        return Vec::new();
    };
    let words = |text: &str| -> Vec<Line<'static>> {
        let mut lines = vec![Line::default()];
        lines.extend(
            widgets::text::wrap(text, across.saturating_sub(2))
                .into_iter()
                .map(|part| Line::from(Span::styled(format!(" {part}"), theme.style(Role::Dim)))),
        );
        lines
    };
    if web_box::reach(conn.web.listen) == web_box::Reach::ThisComputer && conn.web.url.is_none() {
        return words("a phone cannot reach this computer's loopback: choose your network");
    }
    if web.link_local {
        return words(
            "a phone cannot open this link: no address a phone can reach was found, so set \
             [web] url",
        );
    }
    // The rows above, a blank, the code, a blank and the two key lines, between two borders.
    let wanted = |code: &[String]| above + 1 + code.len() + 3 + 2;
    match crate::qr::half_blocks(&link.text) {
        Ok(code)
            if wanted(&code) <= usize::from(area.height)
                && code.iter().all(|line| line.chars().count() <= across) =>
        {
            let paint = Style::new().fg(Color::Black).bg(Color::White);
            let mut lines = vec![Line::default()];
            lines.extend(
                code.into_iter()
                    .map(|line| Line::from(Span::styled(line, paint)).centered()),
            );
            lines
        }
        _ => words("widen the terminal to see the QR code"),
    }
}

/// The box's top border: its name, the hint at how the categories are reached, and the
/// categories' names, the current one between `┤ ├` in the selected role. Each name is a mouse
/// target.
fn draw_tabs(
    frame: &mut Frame,
    body: Rect,
    current: usize,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) {
    let border = crate::tui::screens::device::border_style(app, theme);
    let mut spans = vec![
        Span::styled(" Settings ", theme.style(Role::Selected)),
        Span::styled("─", border),
        Span::styled(" tab→ ", theme.style(Role::Dim)),
    ];
    let mut x = body.x + 1 + spans.iter().map(|span| span.width() as u16).sum::<u16>();
    for (index, (name, _)) in CATEGORIES.iter().enumerate() {
        spans.push(Span::styled("─", border));
        x += 1;
        let digit = SUPERSCRIPTS.get(index).copied().unwrap_or(' ');
        let label = format!(" {digit}{name} ");
        let width = u16::try_from(widgets::text::cells(&label)).unwrap_or(0);
        let on = index == current;
        if on {
            spans.push(Span::styled("┤", border));
            x += 1;
        }
        let style = match on {
            true => theme.style(Role::Selected),
            false => theme.style(Role::Text),
        };
        spans.push(Span::styled(label, style));
        let room = body.right().saturating_sub(1).saturating_sub(x);
        hits.push(
            Rect {
                x,
                y: body.y,
                width: width.min(room),
                height: 1,
            },
            Hit::SettingCategory(index),
        );
        x += width;
        if on {
            spans.push(Span::styled("├", border));
            x += 1;
        }
    }
    let top = Rect {
        x: body.x + 1,
        y: body.y,
        width: body.width.saturating_sub(2),
        height: 1,
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), top);
}

/// The right column's lines for `setting`: what it does, a blank line, where it is in the
/// file, and, for a row an uppercase key also turns, that key.
fn explanation(setting: Setting, app: &App, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let mut lines: Vec<Line<'static>> = widgets::text::wrap(&explain(setting), width)
        .into_iter()
        .map(Line::from)
        .collect();
    lines.push(Line::default());
    let dim = theme.style(Role::Dim);
    let placed = match (setting.is_connection(), setting) {
        (true, _) => connection_line(setting, &app.overlays.conn),
        (false, Setting::AutoEqRefresh) => autoeq_line(app.presets.autoeq_weeks),
        (false, _) => config_line(setting, &app.tui),
    };
    for line in widgets::text::wrap(&placed, width) {
        lines.push(Line::from(Span::styled(line, dim)));
    }
    if let Some(hint) = keys::toggled_by(setting) {
        lines.push(Line::from(Span::styled(
            format!("{} from any screen", hint.label),
            dim,
        )));
    }
    if setting.is_connection() {
        lines.extend(connection_lines(setting, app, theme, width));
    }
    lines
}

/// What a Connect row adds below the common lines: where the daemon reads its configuration
/// when that is another file than this session's, the daemon's own words for a status row, and
/// for the token row where it comes from and, while it is on show, the token itself.
fn connection_lines(
    setting: Setting,
    app: &App,
    theme: &Theme,
    width: usize,
) -> Vec<Line<'static>> {
    let dim = theme.style(Role::Dim);
    let mut lines = Vec::new();
    let mut say = |text: &str, style| {
        for line in widgets::text::wrap(text, width) {
            lines.push(Line::from(Span::styled(line, style)));
        }
    };
    if let Some(theirs) = &app.overlays.conn.elsewhere {
        say("", dim);
        say(
            &format!("Read-only: the daemon reads {theirs}, not this file."),
            theme.style(Role::Warn),
        );
    }
    match setting {
        Setting::McpStatus => {
            say("", dim);
            let text = match (&app.overlays.conn.mcp_status, app.session.daemon) {
                (_, false) => "no daemon is answering".to_string(),
                (None, true) => "asking the daemon".to_string(),
                (Some(status), _) => status.line(),
            };
            say(&text, theme.style(Role::Text));
        }
        Setting::MqttStatus => {
            say("", dim);
            let text = match (&app.overlays.conn.mqtt_status, app.session.daemon) {
                (_, false) => "no daemon is answering".to_string(),
                (None, true) => "asking the daemon".to_string(),
                (Some(status), _) => crate::commands::daemon::status::mqtt_line(status),
            };
            say(&text, theme.style(Role::Text));
        }
        Setting::McpToken => {
            say("", dim);
            let own = match app.overlays.conn.token_state() {
                TokenState::Differs { reason, .. } => {
                    say(
                        &format!("The daemon's token lives {reason}."),
                        theme.style(Role::Warn),
                    );
                    false
                }
                TokenState::NotReadYet { path } => {
                    say(
                        &format!(
                            "The daemon has not read {path} again yet: svolder daemon restart \
                             (or SIGHUP)."
                        ),
                        theme.style(Role::Warn),
                    );
                    true
                }
                TokenState::Own => true,
            };
            match &app.overlays.conn.token {
                _ if !own => {}
                TokenView::Present { source, .. } => {
                    let note = match app.overlays.conn.daemon_up {
                        Some(false) => " (daemon not running)",
                        _ => "",
                    };
                    say(&format!("from {}{note}", source_words(source)), dim);
                }
                TokenView::Missing => {
                    say("The daemon makes one when the server first starts.", dim)
                }
                TokenView::Unreadable(why) => say(why, theme.style(Role::Warn)),
                TokenView::Unread => {}
            }
            if let Some(token) = app.overlays.conn.shown(app.now) {
                say("", dim);
                let chars: Vec<char> = token.chars().collect();
                for piece in chars.chunks(width.max(1)) {
                    lines.push(Line::from(Span::styled(
                        piece.iter().collect::<String>(),
                        theme.style(Role::Text),
                    )));
                }
            }
        }
        Setting::MqttPassword => {
            say("", dim);
            say("Never shown, never edited here.", dim);
            say(
                "This session's environment and config file; the daemon's may differ.",
                dim,
            );
        }
        _ => {}
    }
    lines
}

/// Where a token comes from, in a few words and with the home directory as `~`.
fn source_words(source: &TokenSource) -> String {
    match source {
        TokenSource::Env(var) => var.to_string(),
        TokenSource::File(path) => format!("the token file {}", shorten(path, home().as_deref())),
        TokenSource::Generated(path) => {
            format!("svolder's own file {}", shorten(path, home().as_deref()))
        }
    }
}

/// How many lines the right column takes for `setting` at `width` cells.
#[cfg(test)]
fn explanation_height(setting: Setting, tui: &TuiConfig, width: usize) -> usize {
    let keyed = usize::from(keys::toggled_by(setting).is_some());
    widgets::text::wrap(&explain(setting), width).len()
        + 1
        + widgets::text::wrap(&config_line(setting, tui), width).len()
        + keyed
}

/// One row of the left column, [`LEFT_WIDTH`] cells wide: the cursor, the label — indented
/// where it belongs to the switch above it — the value, cut to fit, and, dimmed in a column of
/// its own, the key that turns the row from any screen where the key table names one. The
/// selected row alone puts `‹` before its value and ` ›` after it.
fn row_line(setting: Setting, focused: bool, app: &App, theme: &Theme) -> Line<'static> {
    let indent = match setting.indented() {
        true => INDENT,
        false => 0,
    };
    let label = format!(
        "{:indent$}{:<width$}",
        "",
        setting.label(),
        width = LABEL_FIELD - indent
    );
    let value = widgets::text::cut_to_cells(&setting.value(app), VALUE_FIELD);
    let zone = match focused {
        true => format!("‹ {value} ›"),
        false => format!("  {value}"),
    };
    let key = keys::toggled_by(setting).map_or(" ", |hint| hint.label);
    let spans = vec![
        Span::raw(label),
        Span::raw(" "),
        Span::raw(widgets::text::pad_cells(&zone, VALUE_ZONE)),
        Span::styled(key, theme.style(Role::Dim)),
    ];
    let mut line = widgets::rows::with_cursor(Line::from(spans), focused, theme);
    let blank = usize::from(LEFT_WIDTH).saturating_sub(line.width());
    line.push_span(Span::raw(" ".repeat(blank)));
    match focused {
        true => line.style(theme.style(Role::Selected)),
        false => line,
    }
}

/// `on` as the row writes it.
fn word(on: bool) -> String {
    match on {
        true => "on".into(),
        false => "off".into(),
    }
}

/// The user's home directory, which the path row is written against.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// `path` as a user writes it: the home directory as `~`, and everything else in full.
fn shorten(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `path` fitted into `room` cells from the left, so the file name is always the part
/// shown: whole directories give way to `…` from the front until the rest fits —
/// `…/svolder/config.toml` — and a file name longer than the room keeps its own tail after `…`.
/// A path that fits is left whole.
fn fitted(path: &str, room: usize) -> String {
    if widgets::text::cells(path) <= room {
        return path.to_string();
    }
    if room == 0 {
        return String::new();
    }
    let rest = path
        .match_indices('/')
        .map(|(at, _)| &path[at..])
        .find(|rest| widgets::text::cells(rest) < room);
    let rest = rest
        .map(str::to_string)
        .unwrap_or_else(|| tail_cells(path, room - 1));
    format!("…{rest}")
}

/// The end of `text` that is at most `room` cells wide.
fn tail_cells(text: &str, room: usize) -> String {
    let mut taken = 0;
    let mut start = text.len();
    for (at, c) in text.char_indices().rev() {
        taken += widgets::text::cells(c.encode_utf8(&mut [0; 4]));
        if taken > room {
            break;
        }
        start = at;
    }
    text[start..].to_string()
}

#[cfg(test)]
mod tests;
