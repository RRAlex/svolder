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
    COLORS, CONFIRM_QUIT, COVER_ART, GRAPH_SPAN, GRAPHS, HOST_AUDIO, INTRO, INTRO_EFFECT,
    INTRO_SOUND, LINK_SPAN, MONOCHROME, MOUSE, PALETTE, THEME_BACKGROUND, TITLE_METER, TOOLTIPS,
    TRANSITION, VISUALIZER,
};
use svolder_ipc::token::TokenSource;
use svolder_ipc::{
    FamilyAccents, McpStatus, MqttProblem, MqttStatus, TuiConfig, TuiSetting, TuiSettingKind,
};
use svolder_profiles::Family;

use crate::tui::app::{App, EffectSwitch, SettingsBox};
use crate::tui::boxes::connections::{
    Connections, Field, GuestView, Modal, MqttView, TokenState, TokenView,
};
// The Web UI box's state, under another name because the `web_box` module here draws it.
use crate::tui::boxes::web_box::{self as web_state, WebBox, WebRow};
use crate::tui::keys;
use crate::tui::layout_tier;
use crate::tui::mouse::{Hit, HitMap, Selected};
use crate::tui::theme::{Role, Surface, Theme};
use crate::tui::widgets;

use super::devices::boxed;

mod web_box;
mod words;

use self::web_box::draw_web_box;
use self::words::{
    NOT_SET, access, autoeq_line, connection_line, key_value, listen_word, mcp_word, mqtt_word,
    password_word, refresh_word, token_word,
};
pub use self::words::{config_line, explain};

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
    /// The broker's username.
    MqttUsername,
    /// Whether a broker password is set, and typed here; never shown.
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
            Setting::Key(&TOOLTIPS),
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
        &[
            Setting::HostAudio,
            Setting::Key(&VISUALIZER),
            Setting::Key(&COVER_ART),
        ],
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
            Setting::MqttUsername,
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
            Setting::MqttUsername => "MQTT username",
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
                | Setting::MqttUsername
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
            Setting::WebUi => web_state::state_word(
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
            Setting::MqttUsername => app
                .overlays
                .conn
                .mqtt
                .username
                .clone()
                .unwrap_or_else(|| NOT_SET.to_string()),
            Setting::MqttPassword => password_word(&app.overlays.conn.mqtt),
            Setting::MqttStatus => mqtt_word(app),
        }
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
            // A password is a bullet a character until Tab shows it.
            let typed = match (line.field, app.overlays.conn.password_shown) {
                (Field::Password, false) => "•".repeat(line.text.chars().count()),
                _ => line.text.clone(),
            };
            let (shown, column) = widgets::text::typed_window(&typed, line.caret, room);
            let label = match line.field {
                Field::Listen => "Listen",
                Field::Host => "Host",
                Field::Port => "Port",
                Field::Username => "User",
                Field::Password => "Pass",
            };
            let keys: &[(&str, &str)] = match line.field {
                Field::Password => &[
                    ("Enter", "saves"),
                    ("Tab", "shows"),
                    ("Del Del", "clears"),
                    ("Esc", "cancels"),
                ],
                _ => &[("Enter", "saves"), ("Esc", "cancels")],
            };
            let lines = vec![
                Line::from(vec![
                    Span::raw(" "),
                    widgets::rows::label(label, PROMPT_LABEL),
                    Span::raw(shown),
                ]),
                Line::default(),
                widgets::surfaces::modal_keys(keys, theme),
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
    if let Some(words) = &app.overlays.conn.config_changed {
        say("", dim);
        say(words, theme.style(Role::Warn));
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
            let mqtt = &app.overlays.conn.mqtt;
            if mqtt.username.is_none() && mqtt.password_set {
                say(
                    "Not sent: MQTT 3.1.1 carries a password only with a username.",
                    theme.style(Role::Warn),
                );
            }
            if mqtt.password_in_env {
                say(
                    "SVOLDER_MQTT_PASSWORD is set here, and wins over the file when the daemon \
                     has it too.",
                    dim,
                );
            }
            say("Never shown.", dim);
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
