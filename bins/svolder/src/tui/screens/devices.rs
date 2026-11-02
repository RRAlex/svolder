// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device list: every device the configuration names, which one the TUI is on, and what
//! each of them is doing.
//!
//! A daemon holding the links knows which are up, so its rows carry a product name and a
//! connection state. A direct session knows neither — a configuration file says which devices
//! exist, not how they are doing — and those rows draw `-` where the answer would be.
//!
//! A signal column sits between the product and the state while any row has a signal strength
//! to show and the list is wide enough for it; it is the first thing a narrow list gives up.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::backend::DeviceRow;
use crate::format::{link_down, link_flags};
use crate::tui::app::{App, Holding, Screen};
use crate::tui::boxes::editor::{DeviceEditor, EDITOR_ROWS, EditorRow};
use crate::tui::keys::Scene;
use crate::tui::layout_tier::{self, Tier};
use crate::tui::mouse::{Hit, HitMap, Selected};
use crate::tui::render::FrameId;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::actions::Action;
use crate::tui::widgets::graph;
use crate::tui::widgets::scroll::{MORE_DOWN, MORE_UP};
use crate::tui::widgets::spinner;
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::device::{
    Rule, border_style, contour, draw_header, frame_title, header_built, pane_title, push_marks,
    row_style,
};

/// What puts a link flag after the state's words: a middle dot between blanks.
const FLAG_GAP: &str = " \u{b7} ";
/// The columns the `(current)` badge and the blank before it take after the state.
const BADGE_ROOM: usize = 1 + CURRENT.len();
/// What a row draws where a product name or a connection state is not known.
const UNKNOWN: &str = "-";
/// The fewest columns the name field takes, so a list of short names still lines up.
const NAME_MIN: usize = 8;
/// Columns between one field and the next.
const GAP: usize = 2;
/// What the list's top border says it is, blanks included.
const TITLE: &str = " Devices ";

/// The words in the list's top border.
const ACTIONS: [Action; 4] = [
    Action {
        code: KeyCode::Char('n'),
        word: "new",
    },
    Action {
        code: KeyCode::Char('u'),
        word: "update",
    },
    Action {
        code: KeyCode::Char('x'),
        word: "x remove",
    },
    Action {
        code: KeyCode::Char('r'),
        word: "refresh",
    },
];
/// What the list says when the configuration names no device.
const NONE_CONFIGURED: &str = "No devices configured - n adds one";
/// What the row of an open device with no entry in the configuration says in place of a state.
const NOT_CONFIGURED: &str = "not configured";
/// The badge that marks the device the TUI is on.
const CURRENT: &str = "(current)";
/// Columns an editor row's label takes after the cursor, so every value starts in the same
/// place.
const FIELD: usize = 12;
/// Columns the editor's box takes, borders and all.
const EDITOR_WIDTH: u16 = 56;
/// Columns an editor row has to itself, the box's own border taken off both sides.
const EDITOR_INNER_WIDTH: usize = (EDITOR_WIDTH - 2) as usize;
/// Columns the line a row takes is shown in: what the cursor, the label and a blank kept before
/// the box's border leave of the row.
const TYPED_ROOM: usize = EDITOR_INNER_WIDTH - CURSOR_WIDTH - FIELD - 1;
/// What the editor says its keys are.
const EDITOR_KEYS: [(&str, &str); 2] = [("Enter", "saves"), ("Esc", "cancels")];
/// What answers the question a removal asks.
const REMOVE_KEYS: [(&str, &str); 2] = [("Enter", "removes"), ("Esc", "cancels")];

/// The tiers `area` falls in, and the rows the header and the Devices block take over it.
fn rows_of(area: Rect) -> (Tier, [Rect; 2]) {
    layout_tier::split(area)
}

/// The one bordered block this screen draws over `area`: the list itself. The editor and the
/// question a removal asks are boxes over the list rather than frames of the screen.
pub(crate) fn frames_of(area: Rect, app: &App) -> Vec<(FrameId, Rect)> {
    let (_, [_, body]) = rows_of(area);
    vec![(FrameId::Screen, split_notice(body, app).1)]
}

/// The failure a start ended in, as the panel above the list says it: what happened, then the
/// step that follows.
fn notice_of(app: &App) -> Option<&(String, String)> {
    match &app.session.holding {
        Some(Holding::Nothing { why: Some(why) }) => Some(why),
        _ => None,
    }
}

/// How wide the panel's words may be on a panel `width` wide: the frame's two columns and the
/// cell of padding either side of the words are taken off.
fn notice_room(width: u16) -> usize {
    usize::from(width.saturating_sub(4))
}

/// `body` split into the panel above the list, when a failure is to be told, and the list's own
/// block under it. The panel is as tall as its words need, its frame included, and never more
/// than half of `body`.
fn split_notice(body: Rect, app: &App) -> (Option<Rect>, Rect) {
    let Some((what, next)) = notice_of(app) else {
        return (None, body);
    };
    let room = notice_room(body.width);
    let words = widgets::text::wrap(what, room).len() + widgets::text::wrap(next, room).len();
    let height = u16::try_from(words + 2)
        .unwrap_or(u16::MAX)
        .min(body.height / 2);
    let panel = Rect { height, ..body };
    let list = Rect {
        y: body.y + height,
        height: body.height - height,
        ..body
    };
    (Some(panel), list)
}

/// Draw the panel that tells why no device opened: the failure's words, then its step, in a frame
/// of their own over the list. Words that do not fit end in `…`.
fn draw_notice(
    frame: &mut Frame,
    panel: Rect,
    (what, next): &(String, String),
    (app, theme, tier): (&App, &Theme, Tier),
) {
    let block = tier
        .block()
        .border_style(contour(app, theme, false, border_style(app, theme)))
        .title(Span::styled(" Not opened ", pane_title(theme, false)));
    let inner = block.inner(panel);
    let room = notice_room(panel.width);
    let mut lines: Vec<Line<'static>> = widgets::text::wrap(what, room)
        .into_iter()
        .chain(widgets::text::wrap(next, room))
        .map(|words| Line::from(format!(" {words}")))
        .collect();
    let rows = usize::from(inner.height);
    if lines.len() > rows && rows > 0 {
        lines.truncate(rows);
        if let Some(last) = lines.last_mut() {
            *last = Line::from(format!("{}…", last));
        }
    }
    frame.render_widget(Paragraph::new(lines).block(block), panel);
}

/// Draw the device list into `area`, and answer with the box drawn over it while the editor or
/// the question a removal asks is open.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) -> Option<Rect> {
    let (tier, [header, whole]) = rows_of(area);
    let (notice, body) = split_notice(whole, app);

    let (line, marks) = header_built(app, theme, header);
    draw_header(
        frame,
        header,
        theme,
        line,
        &Rule::Tabs {
            current: Screen::Devices,
            has_eq: app.profile.eq.is_some(),
        },
        hits,
    );
    push_marks(hits, header, marks);
    if let (Some(panel), Some(why)) = (notice, notice_of(app)) {
        draw_notice(frame, panel, why, (app, theme, tier));
    }

    // The keys are always in the list. With the notice over it, the list's title says so.
    let title = match notice.is_some() {
        true => pane_title(theme, true),
        false => frame_title(theme),
    };
    let mut block = tier
        .block()
        .border_style(contour(app, theme, true, border_style(app, theme)))
        .title(Span::styled(TITLE, title));
    let inner = block.inner(body);
    // A list longer than the block shows the rows round the focused one, and says on the
    // border which way the rest lie.
    let shown = usize::from(inner.height);
    let top = widgets::scroll::scroll_start(app.devices.rows.len(), shown, app.selected);
    let (words, word_hits) = widgets::actions::actions_title(
        &ACTIONS,
        (top > 0).then_some(MORE_UP.trim()),
        widgets::actions::action_room(body.width, widgets::text::cells(TITLE)),
        Screen::Devices,
        &Scene::of(app),
        theme,
    );
    block = block.title(words.clone().right_aligned());
    if top + shown < app.devices.rows.len() {
        block = block.title_bottom(Line::from(MORE_DOWN).right_aligned());
    }
    let lines = list_lines(app, theme, inner.width, top, shown);
    frame.render_widget(Paragraph::new(lines).block(block), body);
    widgets::surfaces::fade_list(
        frame.buffer_mut(),
        app.list_fade,
        inner,
        top,
        shown,
        app.selected,
        theme,
    );
    push_list_hits(hits, app, body, inner, top, &words, &word_hits);

    // The editor and the question a removal asks are drawn over the list they are about, so
    // the row being changed stays in sight behind them, dimmed.
    if let Some(editor) = app.devices.editor.as_ref() {
        Some(draw_editor(frame, body, editor, app, theme))
    } else {
        app.devices
            .confirm
            .as_deref()
            .map(|name| draw_removal(frame, body, name, app, theme))
    }
}

/// The list's rows from `top` on, `shown` at most, each filling `inner_width` cells; with none
/// configured, the line that says so. The open device that the configuration does not name
/// follows the rows.
fn list_lines(
    app: &App,
    theme: &Theme,
    inner_width: u16,
    top: usize,
    shown: usize,
) -> Vec<Line<'static>> {
    let width = usize::from(inner_width);
    let name_width =
        field_width(app.devices.rows.iter().map(|row| row.name.as_str())).max(NAME_MIN);
    let product_width = field_width(app.devices.rows.iter().map(product));
    let glyph = app.spinner();
    let signals = signal_column(app, width, (name_width, product_width), glyph);
    let mut lines: Vec<Line<'static>> = if app.devices.rows.is_empty() {
        vec![row_style(
            Line::from(format!("   {NONE_CONFIGURED}")),
            false,
            false,
            theme,
            width,
        )]
    } else {
        app.devices
            .rows
            .iter()
            .enumerate()
            .skip(top)
            .take(shown)
            .map(|(index, row)| {
                device_line(
                    row,
                    index == app.selected,
                    row.name == app.session.device_name,
                    Fields {
                        name: name_width,
                        product: product_width,
                        signal: signals
                            .as_ref()
                            .map(|(texts, wide)| (texts[index].as_str(), *wide)),
                    },
                    theme,
                    width,
                    glyph,
                )
            })
            .collect()
    };
    // The device the TUI is on, when the configuration does not name it (the fake transport's
    // one device), follows the rows once the last of them is in view.
    let open_unlisted = !app.session.device_name.is_empty()
        && !app
            .devices
            .rows
            .iter()
            .any(|row| row.name == app.session.device_name);
    if open_unlisted && top + shown >= app.devices.rows.len() && lines.len() < shown {
        lines.push(unconfigured_line(app, theme, width));
    }
    lines
}

/// Push the hits of the list: a row each for the rows in view, the top border and the title, which
/// lead back to the device screen, and the words in the border.
fn push_list_hits(
    hits: &mut HitMap,
    app: &App,
    body: Rect,
    inner: Rect,
    top: usize,
    words: &Line<'static>,
    word_hits: &[(u16, u16, KeyCode)],
) {
    let shown = usize::from(inner.height);
    for (at, index) in (top..app.devices.rows.len()).take(shown).enumerate() {
        let row = Rect {
            x: inner.x,
            y: inner.y + u16::try_from(at).unwrap_or(u16::MAX),
            width: inner.width,
            height: 1,
        };
        hits.push(row.intersection(inner), Hit::ListRow(index));
        if index == app.selected {
            hits.select(Selected::Devices, index, row.intersection(inner));
        }
    }
    // The whole top border leads back to the device screen except the cells of the `more ↑`
    // mark, which do nothing when clicked; the title, pushed last, leads there too.
    let border = Rect {
        height: 1.min(body.height),
        ..body
    };
    if top > 0 {
        let mark = u16::try_from(widgets::text::cells(MORE_UP)).unwrap_or(u16::MAX);
        let left = Rect {
            width: body.width.saturating_sub(mark.saturating_add(1)),
            ..border
        };
        hits.push(left, Hit::Tab(Screen::Device));
    } else {
        hits.push(border, Hit::Tab(Screen::Device));
    }
    let title = Rect {
        x: body.x + 1,
        width: u16::try_from(widgets::text::cells(TITLE))
            .unwrap_or(u16::MAX)
            .min(body.width.saturating_sub(1)),
        height: 1.min(body.height),
        ..body
    };
    hits.push(title, Hit::Tab(Screen::Device));
    widgets::actions::push_action_hits(hits, body, words, word_hits);
}

/// Draw the editor over `area`: a row for each of the four things a `[[device]]` block says
/// that a user changes, and the two keys that end it.
///
/// The terminal's own cursor sits in the row being typed into, so a field looks like one.
/// Everything around the box is dimmed, the box itself is drawn in the theme's own colours, and
/// the box is what this answers with.
fn draw_editor(
    frame: &mut Frame,
    area: Rect,
    editor: &DeviceEditor,
    app: &App,
    theme: &Theme,
) -> Rect {
    let lines: Vec<Line<'static>> = EDITOR_ROWS
        .iter()
        .enumerate()
        .map(|(index, row)| {
            editor_line(*row, editor, index == editor.row, theme, EDITOR_INNER_WIDTH)
        })
        .chain([
            Line::default(),
            widgets::surfaces::modal_keys(&EDITOR_KEYS, theme),
        ])
        .collect();
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, EDITOR_WIDTH, height);
    let block = widgets::surfaces::modal_block(
        Block::bordered(),
        &format!(" Edit {} ", editor.original),
        app,
        theme,
    );
    let inner = block.inner(box_area);
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    if editor.focused() == EditorRow::Name {
        let (_, column) = typed_field(editor, editor.focused(), true);
        let x = inner.x + u16::try_from(CURSOR_WIDTH + FIELD + column).unwrap_or(u16::MAX);
        let y = inner.y + u16::try_from(editor.row).unwrap_or(0);
        if x < inner.x + inner.width && y < inner.y + inner.height {
            frame.set_cursor_position(Position { x, y });
        }
    }
    box_area
}

/// One editor row: the cursor, the label, and what the row holds — the name being typed, the
/// passphrase or the profile between the arrows that step it, or the box a switch is drawn as.
fn editor_line(
    row: EditorRow,
    editor: &DeviceEditor,
    focused: bool,
    theme: &Theme,
    max_width: usize,
) -> Line<'static> {
    let mut spans = vec![widgets::rows::label(label(row), FIELD)];
    match row {
        EditorRow::Name => spans.push(Span::raw(typed_field(editor, row, focused).0)),
        // The passphrase steps between its three answers like the profile does, and its code is
        // typed in order, so it has no cursor of its own.
        EditorRow::Passphrase => {
            spans.push(Span::raw("‹ "));
            spans.push(Span::styled(
                editor.passphrase.shown(),
                theme.style(Role::SelectName),
            ));
            spans.push(Span::raw(" ›"));
        }
        EditorRow::Profile => {
            spans.push(Span::raw("‹ "));
            spans.push(Span::styled(
                editor.profile().to_string(),
                theme.style(Role::SelectName),
            ));
            spans.push(Span::raw(" ›"));
        }
        EditorRow::Default => {
            let (box_text, style) = match editor.default {
                true => ("[x]", theme.style(Role::ToggleOn)),
                false => ("[ ]", ratatui::style::Style::default()),
            };
            spans.push(Span::styled(box_text, style));
            spans.push(Span::raw(match editor.default {
                true => " yes",
                false => " no",
            }));
        }
    }
    row_style(
        widgets::rows::with_cursor(Line::from(spans), focused, theme),
        focused,
        false,
        theme,
        max_width,
    )
}

/// What a row that takes a line shows of it, and the column the cursor is at in that: the part
/// round the cursor on the focused row, and the start of the line on any other, so a line longer
/// than the row scrolls through it rather than running off the box.
fn typed_field(editor: &DeviceEditor, row: EditorRow, focused: bool) -> (String, usize) {
    let text = match row {
        EditorRow::Name => editor.device.name.as_str(),
        EditorRow::Passphrase | EditorRow::Profile | EditorRow::Default => "",
    };
    let caret = if focused { editor.caret } else { 0 };
    widgets::text::typed_window(text, caret, TYPED_ROOM)
}

/// What each editor row is called.
fn label(row: EditorRow) -> &'static str {
    match row {
        EditorRow::Name => "Name",
        EditorRow::Passphrase => "Passphrase",
        EditorRow::Profile => "Profile",
        EditorRow::Default => "Default",
    }
}

/// Draw the question a removal asks over `area`: which device it is about, and the two keys
/// that answer it. Everything around the box is dimmed, the box itself is drawn in the theme's
/// own colours, and the box is what this answers with.
fn draw_removal(frame: &mut Frame, area: Rect, name: &str, app: &App, theme: &Theme) -> Rect {
    let lines = vec![
        Line::from(Span::raw(format!(" Remove {name} from the configuration?"))),
        Line::default(),
        widgets::surfaces::modal_keys(&REMOVE_KEYS, theme),
    ];
    let box_area = boxed(
        area,
        EDITOR_WIDTH,
        u16::try_from(lines.len() + 2).unwrap_or(5),
    );
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(
        Paragraph::new(lines).block(widgets::surfaces::modal_block(
            Block::bordered(),
            " Remove ",
            app,
            theme,
        )),
        box_area,
    );
    box_area
}

/// A box `width` by `height` in the middle of `area`, never larger than `area` holds: where the
/// device editor, the question a removal asks and the preset list's boxes go.
pub(super) fn boxed(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// How wide a field holding `texts` is drawn: its longest entry, and one column past it so
/// that two fields never touch even before the gap between them.
fn field_width<'a>(texts: impl Iterator<Item = &'a str>) -> usize {
    texts.map(widgets::text::cells).max().unwrap_or(0) + 1
}

/// The signal column's text for every row, and the column's width, when the list shows one:
/// some row has a signal strength, and the list is wide enough for the column beside the widest
/// state and the badge.
fn signal_column(
    app: &App,
    width: usize,
    (name_width, product_width): (usize, usize),
    glyph: char,
) -> Option<(Vec<String>, usize)> {
    if app.devices.rows.iter().all(|row| row.signal.is_none()) {
        return None;
    }
    let now = graph::wall_ms();
    let texts: Vec<String> = app
        .devices
        .rows
        .iter()
        .map(|row| {
            svolder_ipc::describe_signal_short(row.signal.as_ref(), now, svolder_ipc::SIGNAL_MINUS)
        })
        .collect();
    let wide = field_width(texts.iter().map(String::as_str));
    let state = app
        .devices
        .rows
        .iter()
        .map(|row| {
            let flags = link_flags(row.stale, row.refused).map_or(0, |flags| {
                widgets::text::cells(FLAG_GAP) + widgets::text::cells(&flags)
            });
            widgets::text::cells(&state_of(row, glyph).0) + flags
        })
        .max()
        .unwrap_or(0);
    let base = CURSOR_WIDTH + name_width + GAP + product_width + GAP + state + BADGE_ROOM;
    (base + wide + GAP <= width).then_some((texts, wide))
}

/// A row's link state: its words and the role they take, with a link that is down saying when
/// the daemon tries it again; `-` and no role where nothing holds a link.
fn state_of(row: &DeviceRow, glyph: char) -> (String, Option<Role>) {
    match row.connected {
        Some(true) => ("connected".to_string(), Some(Role::Connected)),
        Some(false) => {
            let state = link_down("disconnected", row.retry_in_secs);
            let state = match row.retry_in_secs {
                Some(_) => spinner::before(glyph, &state),
                None => state,
            };
            (state, Some(Role::Disconnected))
        }
        None => (UNKNOWN.to_string(), None),
    }
}

/// How wide the fields before a row's state are, and the row's signal text with its column's
/// width when the list has a signal column.
struct Fields<'a> {
    /// the name field's width
    name: usize,
    /// the product field's width
    product: usize,
    /// the signal text of this row and the signal field's width
    signal: Option<(&'a str, usize)>,
}

/// One device's row: its name, what it is, how strong its signal is when the list has a signal
/// column (`signal`: the row's text and the column's width), and what it is doing, with the
/// device the TUI is on marked as the current one. A device the daemon is waiting to try again
/// for says when it will, as `svolder daemon status` does; one whose passphrase was refused, or
/// whose readings are stale, says so after its state.
fn device_line(
    row: &DeviceRow,
    focused: bool,
    current: bool,
    fields: Fields,
    theme: &Theme,
    max_width: usize,
    glyph: char,
) -> Line<'static> {
    let (state, role) = state_of(row, glyph);
    let mut spans = vec![
        Span::raw(field(&row.name, fields.name)),
        Span::raw(field(product(row), fields.product)),
    ];
    if let Some((text, wide)) = fields.signal {
        spans.push(Span::raw(field(text, wide)));
    }
    spans.push(match role {
        Some(role) => Span::styled(state, theme.style(role)),
        None => Span::raw(state),
    });
    if let Some(flags) = link_flags(row.stale, row.refused) {
        spans.push(Span::styled(
            format!("{FLAG_GAP}{flags}"),
            theme.style(Role::Warn),
        ));
    }
    if current {
        spans.push(Span::raw(" "));
        spans.push(widgets::rows::badge(CURRENT, theme, focused));
    }
    row_style(
        widgets::rows::with_cursor(Line::from(spans), focused, theme),
        focused,
        false,
        theme,
        max_width,
    )
}

/// The row of the device the TUI is on when the configuration has no entry for it: its name
/// and that it is not configured.
fn unconfigured_line(app: &App, theme: &Theme, max_width: usize) -> Line<'static> {
    let name = field(
        &app.session.device_name,
        field_width([app.session.device_name.as_str()].into_iter()).max(NAME_MIN),
    );
    row_style(
        Line::from(vec![
            Span::raw("   "),
            Span::raw(name),
            Span::raw(NOT_CONFIGURED),
            Span::raw(" "),
            widgets::rows::badge(CURRENT, theme, false),
        ]),
        false,
        false,
        theme,
        max_width,
    )
}

/// The product name a row carries, or [`UNKNOWN`] for a device nothing has seen.
fn product(row: &DeviceRow) -> &str {
    row.product.as_deref().unwrap_or(UNKNOWN)
}

/// `text` left-justified in `width` columns, and the gap before the next field.
fn field(text: &str, width: usize) -> String {
    format!(
        "{}{}",
        widgets::text::pad_cells(text, width),
        " ".repeat(GAP)
    )
}

#[cfg(test)]
mod tests;
