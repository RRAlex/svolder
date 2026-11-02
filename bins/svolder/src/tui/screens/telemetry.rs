// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The telemetry screen: every reading the device has sent, its wire type and raw value, a
//! label drawn from the profile where one names the function, and how old the reading is and
//! how often it arrives.
//!
//! A reading no control, no tone-control function and no firmware query explains draws `?`
//! for its label, in [`Role::Unknown`], which is how an undocumented stream shows up. An
//! update rate at or above once a second draws in [`Role::RateFast`] for the same reason: a
//! fast, unlabelled stream is worth a user's attention.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use svolder_profiles::{ControlKind, EqParam};
use svolder_proto::FunctionId;
use svolder_proto::function::{FIRMWARE_FAMILY, FIRMWARE_VERSION_QUERY};

use crate::tui::app::{App, Reading, Screen};
use crate::tui::keys::Scene;
use crate::tui::layout_tier::{self, Tier};
use crate::tui::mouse::{Hit, HitMap, Selected};
use crate::tui::render::FrameId;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::actions::Action;
use crate::tui::widgets::graph::{self, Shape};
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::device::{
    Rule, border_style, contour, draw_header, frame_title, header_built, push_marks,
};

/// Columns the ID field takes.
const ID_WIDTH: usize = 9;
/// Columns the Type field takes.
const TYPE_WIDTH: usize = 6;
/// Columns the Value field takes.
const VALUE_WIDTH: usize = 12;
/// Columns the Age field takes.
const AGE_WIDTH: usize = 5;
/// Columns the Rate field takes.
const RATE_WIDTH: usize = 6;
/// Columns the margin after the Rate field takes: as wide as the cursor column on the other
/// side, so a row stands off both of the pane's borders by the same amount.
const MARGIN_WIDTH: usize = CURSOR_WIDTH;
/// Columns the Label field keeps when the graph column is drawn.
const MIN_LABEL: usize = 16;
/// Columns the graph column needs to be drawn at all.
const MIN_GRAPH: usize = 16;
/// Columns the graph column takes at most; the label has the rest.
const MAX_GRAPH: usize = 40;
/// The update rate, in readings per second, at and above which a stream draws as fast.
const FAST_RATE_PER_S: f64 = 1.0;

/// The tiers `area` falls in, and the rows the header and the Telemetry block take over it.
fn rows_of(area: Rect) -> (Tier, [Rect; 2]) {
    layout_tier::split(area)
}

/// The one bordered block this screen draws over `area`: the readings, below the header.
pub(crate) fn frames_of(area: Rect) -> Vec<(FrameId, Rect)> {
    let (_, [_, body]) = rows_of(area);
    vec![(FrameId::Screen, body)]
}

/// The name in the list's top border.
const TITLE: &str = " Telemetry ";

/// The word in the list's top border.
const ACTIONS: [Action; 1] = [Action {
    code: KeyCode::Char('r'),
    word: "refresh",
}];

/// Draw the telemetry screen into `area`.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, hits: &mut HitMap) {
    let (tier, [header, body]) = rows_of(area);

    let (line, marks) = header_built(app, theme, header);
    draw_header(
        frame,
        header,
        theme,
        line,
        &Rule::Tabs {
            current: Screen::Telemetry,
            has_eq: app.profile.eq.is_some(),
        },
        hits,
    );
    push_marks(hits, header, marks);

    let rows = app.telemetry_rows();
    // The keys are always in the list, the one frame the screen draws.
    let (words, word_hits) = widgets::actions::actions_title(
        &ACTIONS,
        Some(&format!("{} functions", rows.len())),
        widgets::actions::action_room(body.width, widgets::text::cells(TITLE)),
        Screen::Telemetry,
        &Scene::of(app),
        theme,
    );
    let block = tier
        .block()
        .border_style(contour(app, theme, true, border_style(app, theme)))
        .title(Span::styled(TITLE, frame_title(theme)))
        .title(words.clone().right_aligned());
    let inner = block.inner(body);
    let (label_width, graph_width) = columns_of(inner.width, app.tui.graphs);
    let data_height = usize::from(inner.height).saturating_sub(1);
    let start = widgets::scroll::scroll_start(rows.len(), data_height, app.selected);
    let visible: Vec<usize> = (start..rows.len()).take(data_height).collect();

    let mut lines = vec![column_header(label_width, graph_width, theme)];
    lines.extend(rows.iter().enumerate().skip(start).take(data_height).map(
        |(index, (id, reading))| {
            let row = Row {
                focused: index == app.selected,
                label_width,
                graph_width,
            };
            data_line(app, *id, reading, &row, theme)
        },
    ));
    frame.render_widget(Paragraph::new(lines).block(block), body);
    let list = Rect {
        y: inner.y + 1,
        height: inner.height.saturating_sub(1),
        ..inner
    };
    widgets::surfaces::fade_list(
        frame.buffer_mut(),
        app.list_fade,
        list,
        start,
        data_height,
        app.selected,
        theme,
    );
    // The window says on the pane's right border, beside its first and last rows, which way
    // the readings it is not showing are.
    let margin = Rect {
        x: body.right().saturating_sub(1),
        y: inner.y + 1,
        width: 1.min(body.width),
        height: u16::try_from(visible.len()).unwrap_or(u16::MAX),
    };
    widgets::scroll::scroll_marks(frame.buffer_mut(), margin, start, rows.len(), theme);
    for (line, index) in visible.into_iter().enumerate() {
        let row = Rect {
            x: inner.x,
            y: inner.y + u16::try_from(line + 1).unwrap_or(u16::MAX),
            width: inner.width,
            height: 1,
        };
        hits.push(row.intersection(inner), Hit::ListRow(index));
        if index == app.selected {
            hits.select(Selected::Telemetry, index, row.intersection(inner));
        }
    }
    let title = Rect {
        height: 1.min(body.height),
        ..body
    };
    hits.push(title, Hit::Tab(Screen::Device));
    widgets::actions::push_action_hits(hits, body, &words, &word_hits);
}

/// Columns the Label field takes: what a pane `width` columns wide leaves once the cursor, the
/// margin on the other side and every other column has taken its share.
fn label_width(width: u16) -> usize {
    usize::from(width).saturating_sub(
        CURSOR_WIDTH + ID_WIDTH + TYPE_WIDTH + VALUE_WIDTH + AGE_WIDTH + RATE_WIDTH + MARGIN_WIDTH,
    )
}

/// The Label and Graph columns' widths in a pane `width` columns wide. The graph column takes
/// what is left once the label has its [`MIN_LABEL`] columns, up to [`MAX_GRAPH`], and only when
/// that is [`MIN_GRAPH`] or more and `graphs` is on; otherwise it is `0` and the label takes
/// the whole of the room, as it did before the graph existed.
fn columns_of(width: u16, graphs: bool) -> (usize, usize) {
    let room = label_width(width);
    match room.saturating_sub(MIN_LABEL) {
        graph if graphs && graph >= MIN_GRAPH => {
            let graph = graph.min(MAX_GRAPH);
            (room - graph, graph)
        }
        _ => (room, 0),
    }
}

/// What one row is drawn as: whether the cursor is on it and its columns' widths.
struct Row {
    focused: bool,
    label_width: usize,
    /// Cells the graph takes; `0` draws none.
    graph_width: usize,
}

/// One reading's graph: its history over the graph span, `width` cells wide, in the shape its
/// wire type calls for. A reading with no series (a string) and one not heard yet draw blank.
/// A control with a scale (a meter, or a level with a range) draws through it, any other reading
/// through the window's own smallest and largest value.
fn graph_spans(
    app: &App,
    id: FunctionId,
    reading: &Reading,
    width: usize,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let blank = || vec![Span::raw(" ".repeat(width))];
    let Some(series) = app.readings.histories.reading(id) else {
        return blank();
    };
    let span = app.tui.graph_span.duration();
    let columns = graph::columns(series, app.now, span, width * 2);
    let fault = app
        .profile
        .control_by_id(id)
        .is_some_and(|control| control.kind == ControlKind::Fault);
    let (shape, levels) = match reading.value.value_type.as_str() {
        "bool" => (if fault { Shape::Fault } else { Shape::Dots }, columns),
        _ => (
            Shape::Fill,
            graph::normalise(
                app.profile
                    .control_by_id(id)
                    .and_then(|control| app.placed(control)),
                &columns,
            ),
        ),
    };
    graph::draw(shape, &levels, &[], theme)
        .pop()
        .unwrap_or_else(blank)
}

/// The column titles, aligned under where each row's own field lands and held back from the
/// rows they title.
fn column_header(label_width: usize, graph_width: usize, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            "{}{}{}{}{}{}{}{}{}",
            " ".repeat(CURSOR_WIDTH),
            pad_left("ID", ID_WIDTH),
            pad_left("Type", TYPE_WIDTH),
            pad_left("Value", VALUE_WIDTH),
            cell("Label", label_width),
            cell("History", graph_width),
            pad_right("Age", AGE_WIDTH),
            pad_right("Rate", RATE_WIDTH),
            " ".repeat(MARGIN_WIDTH),
        ),
        theme.style(Role::Dim),
    ))
}

/// One reading's row: its address, wire type and raw value, its label, and its age and rate.
fn data_line(
    app: &App,
    id: FunctionId,
    reading: &Reading,
    row: &Row,
    theme: &Theme,
) -> Line<'static> {
    let Row {
        focused,
        label_width,
        graph_width,
    } = *row;
    let label = label_for(app, id, &reading.value.raw);
    let unknown = label == "?";
    let rate = app.rate_per_s(id);
    let fast = rate.is_some_and(|r| r >= FAST_RATE_PER_S);
    let age = age_text(app.now.saturating_duration_since(reading.received));

    let mut spans = vec![
        Span::raw(pad_left(&id.to_string(), ID_WIDTH)),
        Span::raw(pad_left(&reading.value.value_type, TYPE_WIDTH)),
        Span::raw(cell(&raw_text(&reading.value.raw), VALUE_WIDTH)),
        Span::raw(cell(&label, label_width - usize::from(graph_width > 0))),
    ];
    if graph_width > 0 {
        spans.push(Span::raw(" "));
        let mut graph = graph_spans(app, id, reading, graph_width, theme);
        if focused {
            // The selected row's band runs unbroken across the graph: its dots take the row's
            // own style rather than their colours.
            for span in &mut graph {
                span.style = Style::default();
            }
        }
        spans.extend(graph);
    }
    spans.extend([
        Span::raw(pad_right(&age, AGE_WIDTH)),
        styled(
            pad_right(&rate_text(rate), RATE_WIDTH),
            fast.then_some(Role::RateFast),
            theme,
        ),
        Span::raw(" ".repeat(MARGIN_WIDTH)),
    ]);
    let line = widgets::rows::with_cursor(Line::from(spans), focused, theme);
    // A rate cell keeps its own green regardless, since a span's style overrides the line's;
    // focus otherwise wins over an unlabelled row's yellow, the row itself taking the role
    // that best describes it.
    match (focused, unknown) {
        (true, _) => line.style(theme.style(Role::Selected)),
        (false, true) => line.style(theme.style(Role::Unknown)),
        (false, false) => line,
    }
}

/// A raw value as the Value column shows it: a string (a Data reading's `aa bb`) bare, any other
/// JSON value as it serialises.
fn raw_text(raw: &serde_json::Value) -> String {
    match raw {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `text` in `role`'s style, or with no style of its own for `None`.
fn styled(text: String, role: Option<Role>, theme: &Theme) -> Span<'static> {
    match role {
        Some(role) => Span::styled(text, theme.style(role)),
        None => Span::raw(text),
    }
}

/// `text` left-justified in `width` columns.
fn pad_left(text: &str, width: usize) -> String {
    widgets::text::pad_cells(text, width)
}

/// `text` right-justified in `width` columns.
fn pad_right(text: &str, width: usize) -> String {
    let blank = " ".repeat(width.saturating_sub(widgets::text::cells(text)));
    format!("{blank}{text}")
}

/// One column's text, left-justified in `width` columns, its tail replaced by `…` when it is
/// wider than the column: a control's own label plus a value name, or a raw value of a wide
/// wire type, can run long, and neither may shift the columns beside it.
fn cell(text: &str, width: usize) -> String {
    pad_left(&widgets::text::cut_to_cells(text, width), width)
}

/// The label an id draws: the control's own label plus the value name it gives the raw value
/// when it names one, what the tone control or the firmware family calls an id no control
/// owns, or `?` for a reading nothing explains.
pub(crate) fn label_for(app: &App, id: FunctionId, raw: &serde_json::Value) -> String {
    if let Some(control) = app.profile.control_by_id(id) {
        let mut label = control.label.clone();
        if let Some(name) = raw
            .as_i64()
            .and_then(|v| u8::try_from(v).ok())
            .and_then(|v| control.value_name(v))
        {
            label.push_str(&format!(" ({name})"));
        }
        return label;
    }
    if let Some(eq) = app.profile.eq.as_ref()
        && id.first() == eq.id
    {
        if id == eq.mode_id() {
            return "EQ mode".to_string();
        }
        if id == eq.loudness_id() {
            return "EQ loudness".to_string();
        }
        for band in 0..eq.bands.len() {
            for (param, word) in [
                (EqParam::Frequency, "freq"),
                (EqParam::Gain, "gain"),
                (EqParam::Q, "Q"),
            ] {
                if eq.band_id(band, param) == Some(id) {
                    return format!("EQ band {} {word}", band + 1);
                }
            }
        }
    }
    if id.first() == FIRMWARE_FAMILY && id.second() == Some(FIRMWARE_VERSION_QUERY) {
        return "Firmware".to_string();
    }
    "?".to_string()
}

/// A reading's age as a user reads it: one decimal under a second, whole seconds under a
/// minute, then whole minutes, then whole hours.
fn age_text(age: Duration) -> String {
    let secs = age.as_secs_f64();
    if secs < 1.0 {
        format!("{secs:.1}s")
    } else if age.as_secs() < 60 {
        format!("{}s", age.as_secs())
    } else if age.as_secs() < 3600 {
        format!("{}m", age.as_secs() / 60)
    } else {
        format!("{}h", age.as_secs() / 3600)
    }
}

/// An update rate as a user reads it: whole readings a second at or above one a second, one
/// decimal below that, or `-` for a reading heard only once.
fn rate_text(rate: Option<f64>) -> String {
    match rate {
        None => "-".to_string(),
        Some(r) if r >= FAST_RATE_PER_S => format!("{r:.0}/s"),
        Some(r) => format!("{r:.1}/s"),
    }
}

#[cfg(test)]
mod tests;
