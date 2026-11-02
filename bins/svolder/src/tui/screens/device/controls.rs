// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Controls pane: a row per control a user can set, then the meters the device streams,
//! then the Host group, with where a click on each row, box, arrow or bar lands.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use svolder_host::Button;
use svolder_ipc::host::{PathLine, Verdict};
use svolder_profiles::{Control, ControlKind, Placed};
use svolder_proto::FunctionId;

use crate::tui::app::{App, Reading, row_group};
use crate::tui::audio_level::LevelFrom;
use crate::tui::host::{self, HostRow};
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::Hit;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::graph::{self, Aggregate, Series, Shape};
use crate::tui::widgets::rows::Columns;
use crate::tui::widgets::{self, CURSOR_WIDTH, MISSING};

use super::{cut_cells, row_style};

/// One row of the Controls pane: what it draws, and how it draws it.
struct Row<'a> {
    /// The control the row shows.
    control: &'a Control,
    /// Its last reading, or `None` until one arrives.
    reading: Option<&'a Reading>,
    /// Whether the value shown is known to be out of date.
    stale: bool,
    /// Whether the row cursor points at it.
    focused: bool,
}

/// The Controls pane's lines, with where among them each control row landed and the finer
/// hits inside a row — a toggle's box, a select's arrows, a level's bar — for the rows that
/// have one.
pub(super) struct Controls {
    /// Every line the pane draws, in order.
    pub(super) lines: Vec<Line<'static>>,
    /// For each control row, the function it shows, the interior line it starts on and how
    /// many it takes.
    pub(super) rows: Vec<(FunctionId, u16, u16)>,
    /// A finer hit inside a row: the line it is on, its column offset and width inside the
    /// pane, and what a click or a wheel there means.
    pub(super) extras: Vec<(u16, u16, u16, Hit)>,
    /// The interior line each of the Host group's rows landed on, in the order they are drawn.
    pub(super) host: Vec<u16>,
    /// The interior line the Host group's Spectrum row landed on, on a pane that draws it.
    pub(super) spectrum: Option<u16>,
    /// Cells a level's bar is drawn across, which the Tone pane's loudness bar matches.
    pub(super) bar: usize,
}

/// The Controls pane: one row per control a user can set, then the meters the device
/// streams, then — while host audio shows — the Host group, its Spectrum row last in the wide
/// and tall tiers, drawn for a pane whose rows have `room` columns after their cursor.
///
/// The pane sets its groups a blank line apart — the selects and toggles, the levels, the
/// balance, the meters, the Host group — wherever its `height` lines hold every row and the
/// blanks; with fewer it packs them, so no row goes out of sight. The tall tier draws a meter's
/// bar on two rows, its label and reading on the first; the wide tier lets its bars run longer.
pub(super) fn controls_pane(
    app: &App,
    theme: &Theme,
    room: usize,
    tier: Tier,
    height: usize,
) -> Controls {
    let spaced = controls_lines(app, theme, room, tier, true);
    match spaced.lines.len() <= height {
        true => spaced,
        false => controls_lines(app, theme, room, tier, false),
    }
}

/// The group a row of the pane is spaced apart by: its group among the rows
/// ([`row_group`]), with the balance standing apart from the other levels.
fn spacing_group(control: &Control) -> (u8, bool) {
    (row_group(control), control.center.is_some())
}

/// The Controls pane's lines, its groups a blank line apart where `spaced`.
fn controls_lines(app: &App, theme: &Theme, room: usize, tier: Tier, spaced: bool) -> Controls {
    let rows = pane_rows(app);
    let spectrum = app.spectrum_drawn_in(tier);
    let mut host_labels: Vec<&str> = match app.host_shown() {
        true => host::ROWS.iter().map(|row| row.label()).collect(),
        false => Vec::new(),
    };
    let path = app.host_path();
    if path.is_some() {
        host_labels.push(host::PATH);
    }
    if spectrum {
        host_labels.push(host::SPECTRUM);
    }
    let columns = columns_of(&rows, &host_labels, room, tier);
    let mut pane = Controls {
        lines: Vec::new(),
        rows: Vec::new(),
        extras: Vec::new(),
        host: Vec::new(),
        spectrum: None,
        bar: columns.bar(),
    };
    // The row index a `Row`, `ToggleOf`, `Select…` or `Level` hit is keyed by: the position
    // among the writable rows only, the same order `App::rows` lists them in.
    let mut index = 0;
    let mut last_group = None;
    for row in &rows {
        let control = row.control;
        let group = spacing_group(control);
        if spaced && last_group.is_some_and(|last| last != group) {
            pane.lines.push(Line::default());
        }
        last_group = Some(group);
        let top = u16::try_from(pane.lines.len()).unwrap_or(u16::MAX);
        let lines = row_lines(
            app,
            theme,
            tier,
            row,
            columns,
            (index, top),
            &mut pane.extras,
        );
        let height = u16::try_from(lines.len()).unwrap_or(1);
        for (n, line) in lines.into_iter().enumerate() {
            let line = widgets::rows::with_cursor(line, row.focused && n == 0, theme);
            pane.lines
                .push(band(line, row.focused, row.stale, theme, room));
        }
        // Only the rows a user can set are rows a click can focus; the meters are a readout.
        if control.kind.writable() {
            pane.rows.push((control.id, top, height));
            index += 1;
        }
    }
    let source = graph_source(app, tier);
    if source == Some(LevelFrom::Meters) {
        meter_graphs(app, theme, room, &mut pane);
    }
    if app.host_shown() {
        host_group(
            app,
            theme,
            columns,
            (index, spaced),
            path.as_ref(),
            &mut pane,
        );
        if spectrum {
            let (line, dim) = spectrum_line(app, theme, columns);
            let line = widgets::rows::with_cursor(line, false, theme);
            pane.spectrum = u16::try_from(pane.lines.len()).ok();
            pane.lines
                .push(band(line, false, dim, theme, columns.room()));
        }
        if source == Some(LevelFrom::Host) {
            host_graph(app, theme, room, &mut pane);
        }
    }
    pane
}

/// One control's row: its lines, and, in `extras`, the finer hits inside it — a toggle's box, a
/// select's arrows, a level's bar. `index` is the row index a hit is keyed by and `top` the
/// interior line the row starts on.
fn row_lines(
    app: &App,
    theme: &Theme,
    tier: Tier,
    row: &Row,
    columns: Columns,
    (index, top): (usize, u16),
    extras: &mut Vec<(u16, u16, u16, Hit)>,
) -> Vec<Line<'static>> {
    let (control, reading) = (row.control, row.reading);
    // Where a toggle's box, a select's arrows and a level's bar start and end, in columns from
    // a row's own start: the same for every row, since a pane's rows share their columns.
    let field = u16::try_from(columns.field()).unwrap_or(0);
    let bar = u16::try_from(columns.bar()).unwrap_or(0);
    // The length a bar is drawn at, which is on its way to the value the row writes out
    // while that value is still gliding there — or the value a drag holds the bar at, which
    // is drawn at once.
    let eased = app
        .dragged_raw(control.id)
        .or_else(|| app.eased_raw(control.id, app.now));
    match control.kind {
        ControlKind::Select => {
            let line = widgets::rows::select_line(control, reading, theme, columns);
            if reading.and_then(Reading::raw_i64).is_some() {
                let width = u16::try_from(line.width()).unwrap_or(0);
                extras.push((top, CURSOR_OFFSET + field, 1, Hit::SelectPrev(index)));
                extras.push((
                    top,
                    CURSOR_OFFSET + width.saturating_sub(1),
                    1,
                    Hit::SelectNext(index),
                ));
            }
            vec![line]
        }
        ControlKind::Toggle => {
            extras.push((top, CURSOR_OFFSET + field, 3, Hit::ToggleOf(index)));
            vec![widgets::rows::toggle_line(control, reading, theme, columns)]
        }
        ControlKind::Meter => widgets::meters::meter_lines(
            control,
            reading,
            theme,
            columns,
            widgets::meters::MeterState {
                placed: app.placed(control),
                live: app.meters_live(),
                eased,
                peak: app.peak(control.id, app.now),
            },
            if tier.tall { 2 } else { 1 },
        ),
        // A level with a centre is a balance; the pane lists no other kind. Its track
        // starts two columns after the label, past the "L " that opens it.
        _ if control.center.is_some() => {
            if has_bar(control, reading) {
                extras.push((
                    top,
                    CURSOR_OFFSET + field + BALANCE_TRACK_OFFSET,
                    bar,
                    Hit::Level(index),
                ));
                // The track takes a press and a drag of its own, over the row's hit.
                extras.push((
                    top,
                    CURSOR_OFFSET + field + BALANCE_TRACK_OFFSET,
                    bar,
                    Hit::LevelBar(index),
                ));
            }
            vec![widgets::rows::balance_line(
                control,
                reading,
                theme,
                columns,
                eased,
                row.focused,
            )]
        }
        _ => {
            if has_bar(control, reading) {
                extras.push((top, CURSOR_OFFSET + field, bar, Hit::Level(index)));
                extras.push((top, CURSOR_OFFSET + field, bar, Hit::LevelBar(index)));
            }
            // The volume carries the step a boosting curve can clip from, and the target it
            // is climbing to.
            let volume = app.volume().is_some_and(|volume| volume.id == control.id);
            let clip = match volume {
                true => app.clip_step(),
                false => None,
            };
            let target = app
                .pointer
                .climb
                .as_ref()
                .filter(|climb| climb.id() == control.id)
                .map(|climb| climb.target());
            vec![widgets::rows::level_line(
                control,
                reading,
                theme,
                columns,
                eased,
                (row.focused, row.stale),
                (clip, target),
            )]
        }
    }
}

/// Columns the label of a level graph takes, after the row cursor: `L/R`, `1` to `5`, `M` or
/// `host`, and a blank.
const GRAPH_LABEL: usize = 5;
/// The fewest cells a level graph is drawn in; a pane with less room leaves it out.
const GRAPH_MIN: usize = 4;

/// Where the level history comes from in a pane of `tier`: [`App::level_from`]'s choice, with a
/// device whose meters carry no scale — which that choice never calls streaming — counting as
/// the meters while it reports them. `None` when no graph is drawn: outside the tall tier, with
/// graphs off, or with no level source.
fn graph_source(app: &App, tier: Tier) -> Option<LevelFrom> {
    if !tier.tall || !app.tui.graphs {
        return None;
    }
    match app.level_from() {
        LevelFrom::Nowhere if !app.unscaled_meters_heard() => None,
        LevelFrom::Host if app.unscaled_meters_heard() => Some(LevelFrom::Meters),
        LevelFrom::Nowhere => Some(LevelFrom::Meters),
        from => Some(from),
    }
}

/// One graph row: the `label` in its column, then the graph's cells, in the pane's row style.
fn graph_line(label: &str, cells: Vec<Span<'static>>, theme: &Theme, room: usize) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{label:<GRAPH_LABEL$}"))];
    spans.extend(cells);
    let line = widgets::rows::with_cursor(Line::from(spans), false, theme);
    band(line, false, false, theme, room)
}

/// The meters' history, after the meter group: `L` and `R` mirrored in two rows, every other
/// meter a row of its own labelled by its channel. Meters with a scale draw through their own;
/// when any has none, all share one scale over the window, so the channels compare honestly.
fn meter_graphs(app: &App, theme: &Theme, room: usize, pane: &mut Controls) {
    let cells = room.saturating_sub(GRAPH_LABEL);
    if cells < GRAPH_MIN {
        return;
    }
    pane.lines.push(Line::default());
    let span = app.tui.graph_span.duration();
    let mut meters: Vec<&Control> = app
        .profile
        .controls
        .iter()
        .filter(|control| control.kind == ControlKind::Meter)
        .collect();
    meters.sort_unstable_by_key(|control| control.id);
    let channel = |want: &str| {
        meters
            .iter()
            .position(|control| control.channel.as_deref() == Some(want))
    };
    let pair = channel("L").zip(channel("R"));
    let columns_of = |control: &Control| {
        let empty = Series::new(Aggregate::Max, graph::Window::TUI);
        let series = app.readings.histories.reading(control.id).unwrap_or(&empty);
        graph::columns(series, app.now, span, cells * 2)
    };
    let all: Vec<Vec<Option<f64>>> = meters.iter().map(|control| columns_of(control)).collect();
    let placed: Vec<Option<Placed>> = meters.iter().map(|control| app.placed(control)).collect();
    let levels = if placed.iter().all(Option::is_some) {
        placed
            .iter()
            .zip(&all)
            .map(|(placed, columns)| graph::normalise(*placed, columns))
            .collect()
    } else {
        let windows: Vec<&[Option<f64>]> = all.iter().map(Vec::as_slice).collect();
        graph::normalise_together(None, &windows)
    };
    for (at, control) in meters.iter().enumerate() {
        match pair {
            Some((l, r)) if at == l => {
                let drawn = graph::draw(Shape::Mirrored, &levels[l], &levels[r], theme);
                let labels = ["L/R", ""];
                for (label, row) in labels.into_iter().zip(drawn) {
                    pane.lines.push(graph_line(label, row, theme, room));
                }
            }
            Some((_, r)) if at == r => {}
            _ => {
                let label = control.channel.as_deref().unwrap_or_default();
                let row = graph::draw(Shape::Fill, &levels[at], &[], theme).remove(0);
                pane.lines.push(graph_line(label, row, theme, room));
            }
        }
    }
}

/// The host capture's history after the Host group: its left and right levels, mirrored, on
/// the fixed scale 0 to 1 they already are.
fn host_graph(app: &App, theme: &Theme, room: usize, pane: &mut Controls) {
    let cells = room.saturating_sub(GRAPH_LABEL);
    if cells < GRAPH_MIN {
        return;
    }
    pane.lines.push(Line::default());
    let span = app.tui.graph_span.duration();
    let [left, right] = app.readings.histories.host();
    let left = graph::columns(left, app.now, span, cells * 2);
    let right = graph::columns(right, app.now, span, cells * 2);
    let drawn = graph::draw(Shape::Mirrored, &left, &right, theme);
    for (label, row) in ["host", ""].into_iter().zip(drawn) {
        pane.lines.push(graph_line(label, row, theme, room));
    }
}

/// `line` as one of the pane's rows: [`row_style`]d, and, while it has the cursor, extended in
/// the selection's style across the `room` columns after the cursor and the blank
/// [`CONTROLS_TRAIL`] to the pane's inner right edge, so the selection reads as a band the whole
/// width of the pane, in monochrome as well.
fn band(
    line: Line<'static>,
    focused: bool,
    dim: bool,
    theme: &Theme,
    room: usize,
) -> Line<'static> {
    let edge = CURSOR_WIDTH + room + CONTROLS_TRAIL;
    let mut line = row_style(line, focused, dim, theme, CURSOR_WIDTH + room);
    let width = line.width();
    if focused && width < edge {
        let style = line.style;
        line.spans
            .push(Span::styled(" ".repeat(edge - width), style));
    }
    line
}

/// The Host group, under the device's own rows: the group's heading — a blank line above it
/// where the pane is `spaced`, as between its other groups — and a row each for the host's
/// default sink, its media player's transport and the computer EQ, the rows the keys reach,
/// drawn in `columns`. `first` is the row index the keys and a click reach the group's first row
/// by, after the device's own.
fn host_group(
    app: &App,
    theme: &Theme,
    columns: Columns,
    (first, spaced): (usize, bool),
    path: Option<&PathLine>,
    pane: &mut Controls,
) {
    if spaced {
        pane.lines.push(Line::default());
    }
    pane.lines.push(Line::from(vec![
        Span::raw(" ".repeat(CURSOR_WIDTH)),
        Span::styled(host::HEADING, theme.style(Role::Dim)),
    ]));
    let field = u16::try_from(columns.field()).unwrap_or(0);
    let bar = u16::try_from(columns.bar()).unwrap_or(0);
    for (at, row) in host::ROWS.iter().enumerate() {
        let top = u16::try_from(pane.lines.len()).unwrap_or(u16::MAX);
        let focused = app.host_row() == Some(*row);
        let (line, dim) = match row {
            HostRow::Volume => {
                if app.host.sink.is_some() {
                    pane.extras
                        .push((top, CURSOR_OFFSET + field, bar, Hit::Level(first + at)));
                }
                host_volume_line(app, theme, columns, focused)
            }
            HostRow::Transport => {
                let (line, buttons) = transport_line(app, theme, columns, focused);
                for (button, (x, width)) in buttons.into_iter().enumerate() {
                    pane.extras
                        .push((top, CURSOR_OFFSET + x, width, Hit::Transport(button)));
                }
                (line, app.host.playing.is_none())
            }
            HostRow::HostEq => host_eq_line(app, theme, columns),
        };
        let line = widgets::rows::with_cursor(line, focused, theme);
        pane.lines
            .push(band(line, focused, dim, theme, columns.room()));
        pane.host.push(top);
        // The path is a reading under the transport it follows, which no key reaches.
        if let (HostRow::Transport, Some(path)) = (row, path) {
            let (line, dim) = path_row(path, theme, columns);
            let line = widgets::rows::with_cursor(line, false, theme);
            pane.lines
                .push(band(line, false, dim, theme, columns.room()));
        }
    }
}

/// Columns between the path's verdict and its hops.
const PATH_GAP: &str = " · ";
/// What joins one hop of the path to the next.
const HOP_JOIN: &str = " → ";

/// The Path row, and whether it is drawn dim: the verdict first, in the colour of what it says,
/// then each hop from the player to the DAC as its label and what is known of the signal there,
/// the hop that changes the signal in the warning colour. The row is cut at its right edge, its
/// tail replaced by `…`, wherever the words run out of room.
pub(super) fn path_row(path: &PathLine, theme: &Theme, columns: Columns) -> (Line<'static>, bool) {
    let verdict_role = match path.verdict {
        Verdict::Untouched => Role::Good,
        Verdict::Changed => Role::Warn,
        Verdict::NotThisDac { .. } | Verdict::OtherInput { .. } | Verdict::Unknown => Role::Dim,
    };
    let mut parts = vec![(path.words_line(), theme.style(verdict_role))];
    for (at, hop) in path.hops.iter().enumerate() {
        let join = match at {
            0 => PATH_GAP,
            _ => HOP_JOIN,
        };
        let style = match hop.changes {
            true => theme.style(Role::Warn),
            false => theme.style(Role::Dim),
        };
        let words = match hop.detail.is_empty() {
            true => hop.label.clone(),
            false => format!("{} {}", hop.label, hop.detail),
        };
        parts.push((format!("{join}{words}"), style));
    }
    let mut room = columns.room().saturating_sub(columns.field());
    let mut spans = vec![widgets::rows::label(host::PATH, columns.field())];
    for (text, style) in parts {
        let shown = cut_cells(&text, room);
        room = room.saturating_sub(widgets::text::cells(&shown));
        let cut = shown != text;
        spans.push(Span::styled(shown, style));
        if cut {
            break;
        }
    }
    (Line::from(spans), false)
}

/// The Host EQ row, and whether it is drawn dim: the preset on the computer and `on` while the
/// computer EQ is, cut to the row, and `off`, dim, while it is not.
fn host_eq_line(app: &App, theme: &Theme, columns: Columns) -> (Line<'static>, bool) {
    let mut spans = vec![widgets::rows::label(host::HOST_EQ, columns.field())];
    let Some(state) = app.presets.host_eq.as_ref().filter(|state| state.on) else {
        spans.push(Span::raw(host::HOST_EQ_OFF));
        return (Line::from(spans), true);
    };
    let room = columns.room().saturating_sub(columns.field());
    let on = " · on";
    let name = cut_cells(&state.name, room.saturating_sub(on.chars().count()));
    spans.push(Span::raw(name));
    spans.push(Span::styled(on, theme.style(Role::Good)));
    (Line::from(spans), false)
}

/// What the Host volume row says `muted` with, after its bar.
const MUTED: &str = "muted";

/// The Host volume row, and whether it is drawn dim: a level's bar from 0 to 100 % with the
/// volume written inside it, `muted` while the sink is, and the sink's own name dim after them,
/// cut to what the row has left. Before the sink has read, or on a host with no sink to read,
/// the row reads `—`, dim.
fn host_volume_line(
    app: &App,
    theme: &Theme,
    columns: Columns,
    focused: bool,
) -> (Line<'static>, bool) {
    let mut spans = vec![widgets::rows::label(host::VOLUME, columns.field())];
    let Some(sink) = app.host.sink.as_ref() else {
        spans.push(Span::raw(MISSING));
        return (Line::from(spans), true);
    };
    let percent = host::percent(sink.volume);
    let drawn = i64::from(percent.min(u16::from(host::VOLUME_MAX)));
    spans.extend(widgets::rows::level_bar(
        theme,
        (drawn, drawn),
        (0, i64::from(host::VOLUME_MAX)),
        columns.bar(),
        &format!("{percent}%"),
        (focused, false),
        (None, None),
    ));
    let mut left = columns
        .room()
        .saturating_sub(columns.field() + columns.bar());
    if sink.muted && left >= VALUE_GAP + MUTED.len() {
        spans.push(Span::raw(" ".repeat(VALUE_GAP)));
        spans.push(Span::styled(MUTED, theme.style(Role::Warn)));
        left -= VALUE_GAP + MUTED.len();
    }
    if !sink.name.is_empty() && left > VALUE_GAP {
        spans.push(Span::raw(" ".repeat(VALUE_GAP)));
        spans.push(Span::styled(
            cut_cells(&sink.name, left - VALUE_GAP),
            theme.style(Role::Dim),
        ));
    }
    (Line::from(spans), false)
}

/// Columns between a row's bar and what follows it.
const VALUE_GAP: usize = 2;

/// The Spectrum row, the Host group's last, and whether it is drawn dim: what the default sink
/// plays, in the capture's bands from 40 Hz to 16 kHz, each a block glyph as tall as the band is
/// loud — every block's bands as it lands, falling back between blocks — while the capture of
/// the sink's monitor runs; `—`, dim, while none does. A row with room for fewer cells than
/// there are bands draws as many as it has room for, from the lowest. The keys do not reach it:
/// it is a reading, as a meter's row is.
fn spectrum_line(app: &App, theme: &Theme, columns: Columns) -> (Line<'static>, bool) {
    let mut spans = vec![widgets::rows::label(host::SPECTRUM, columns.field())];
    if !app.host.capturing {
        spans.push(Span::raw(MISSING));
        return (Line::from(spans), true);
    }
    let room = columns.room().saturating_sub(columns.field());
    let cells: String = app
        .host
        .bands
        .at(app.now)
        .into_iter()
        .take(room)
        .map(host::glyph)
        .collect();
    // a beat of the daemon's frames makes the bands bold for a moment
    let style = match app.row_pulse() > 0.0 {
        true => theme.style(Role::Bar).add_modifier(Modifier::BOLD),
        false => theme.style(Role::Bar),
    };
    spans.push(Span::styled(cells, style));
    (Line::from(spans), false)
}

/// The Transport row — previous, play, pause and next, in the plain geometric glyphs — and
/// where each button's click lands, as a column from the row's start and a width: its glyphs
/// and the cell on either side of them it shares no button with. The button matching what the
/// followed player is doing is drawn in the accent, bold; on the focused row the button under
/// the row's own cursor is drawn out of the row's highlight, so it reads as the one `Enter`
/// presses.
fn transport_line(
    app: &App,
    theme: &Theme,
    columns: Columns,
    focused: bool,
) -> (Line<'static>, Vec<(u16, u16)>) {
    let mut spans = vec![widgets::rows::label(host::TRANSPORT, columns.field())];
    let mut at = columns.field();
    let mut buttons = Vec::with_capacity(host::GLYPHS.len());
    let last = host::GLYPHS.len() - 1;
    for (index, (glyph, button)) in host::GLYPHS.iter().zip(Button::ALL).enumerate() {
        if index > 0 {
            spans.push(Span::raw(" ".repeat(host::BUTTON_GAP)));
            at += host::BUTTON_GAP;
        }
        let mut style = match app.host.lit() == Some(button) {
            true => theme.style(Role::Cursor).add_modifier(Modifier::BOLD),
            false => Style::default(),
        };
        if focused && app.host.button() == button {
            style = style.remove_modifier(Modifier::REVERSED);
        }
        let span = Span::styled(*glyph, style);
        let width = span.width();
        spans.push(span);
        let before = usize::from(index > 0);
        let after = usize::from(index < last);
        buttons.push((
            u16::try_from(at - before).unwrap_or(0),
            u16::try_from(width + before + after).unwrap_or(0),
        ));
        at += width;
    }
    (Line::from(spans), buttons)
}

/// Columns before a row's label, that its cursor (`▶` on the focused row) takes.
const CURSOR_OFFSET: u16 = CURSOR_WIDTH as u16;
/// Columns a balance track starts after its label, past the `"L "` that opens it.
const BALANCE_TRACK_OFFSET: u16 = 2;

/// Whether a level or a balance control has a bar drawn for it: a reading and a range both
/// present, the same guard [`widgets::rows::level_line`] and [`widgets::rows::balance_line`]
/// draw one under.
fn has_bar(control: &Control, reading: Option<&Reading>) -> bool {
    reading.and_then(Reading::raw_i64).is_some() && control.range.is_some()
}

/// Every row the Controls pane draws: the controls a user can set in the order
/// [`App::rows`] lists them, then the meters the device streams.
fn pane_rows(app: &App) -> Vec<Row<'_>> {
    let focused = app.selected_control().map(|control| control.id);
    let mut rows: Vec<Row<'_>> = app
        .rows()
        .into_iter()
        .map(|row| Row {
            control: row.control,
            reading: row.reading,
            stale: row.stale,
            focused: focused == Some(row.control.id),
        })
        .collect();
    let mut meters: Vec<&Control> = app
        .profile
        .controls
        .iter()
        .filter(|control| control.kind == ControlKind::Meter)
        .collect();
    meters.sort_unstable_by_key(|control| control.id);
    rows.extend(meters.into_iter().map(|control| Row {
        control,
        reading: app.readings.values.get(&control.id),
        stale: !app.session.connected,
        focused: false,
    }));
    rows
}

/// The columns `rows` share with the rows labelled `more` under them: wide enough for the
/// longest label the pane has and the widest value any of its bars carries, with bars as long
/// as `tier` lets them run.
fn columns_of(rows: &[Row<'_>], more: &[&str], room: usize, tier: Tier) -> Columns {
    let longest = rows
        .iter()
        .map(|row| row.control.label.chars().count())
        .chain(more.iter().map(|label| label.chars().count()))
        .max()
        .unwrap_or(0);
    let widest = rows
        .iter()
        .filter_map(|row| widgets::rows::bar_value(row.control, row.reading))
        .map(|value| value.chars().count())
        .max()
        .unwrap_or(0);
    let bar_max = match tier.wide {
        true => widgets::BAR_MAX_WIDE,
        false => widgets::BAR_MAX,
    };
    Columns::fit(room, longest, widest, bar_max)
}

/// The columns a row has for its label and value inside a pane drawn over `area`: what the
/// borders, the tier's padding, the row cursor and the [`CONTROLS_TRAIL`] blank before the
/// right border leave.
pub(super) fn room(area: Rect, tier: Tier) -> usize {
    usize::from(tier.block().inner(area).width)
        .saturating_sub(widgets::CURSOR_WIDTH + CONTROLS_TRAIL)
}

/// The lines a pane drawn over `area` has for its rows: what the borders and the tier's padding
/// leave.
pub(super) fn height(area: Rect, tier: Tier) -> usize {
    usize::from(tier.block().inner(area).height)
}

/// Columns a Controls row keeps blank before the pane's right border, so a value never
/// touches it: with the wide tier's padding, two, the two its rows keep on the left.
const CONTROLS_TRAIL: usize = 1;
