// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Controls pane: a row per control a user can set, then the meters the device streams,
//! then the Host group, with where a click on each row, box, arrow or bar lands.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use svolder_host::Button;
use svolder_ipc::host::{PathRow, Tone};
use svolder_profiles::{Control, ControlKind, Placed};
use svolder_proto::FunctionId;

use crate::tui::app::{App, Reading, Side, row_group};
use crate::tui::audio_level::LevelFrom;
use crate::tui::host::{self, HostRow};
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::Hit;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::graph::{self, Aggregate, Series, Shape};
use crate::tui::widgets::rows::Columns;
use crate::tui::widgets::{self, CURSOR_WIDTH, MISSING};

use super::{cut_cells, path_line, row_style};
use crate::tui::screens::switch;

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
    /// The interior line the switch is drawn on, over the Volume row's bar or as the Host
    /// group's Volume row, where there is one.
    pub(super) switch: Option<u16>,
    /// Where the Path row landed, when the pane draws one.
    pub(super) path: Option<PathDrawn>,
    /// How many of the lines are the blanks that set the groups apart.
    pub(super) blanks: usize,
    /// The gaps this layout could still draw in the Host group, in the order spare rows are
    /// given to them.
    pub(super) offered: Vec<Gap>,
}

impl Controls {
    /// How many lines the pane takes with its Path row counted at `path` lines, whatever the
    /// reading gives it now: the pane's lines as they are where it draws none.
    pub(super) fn lines_reserving(&self, path: usize) -> usize {
        let drawn = self.path.map_or(0, |drawn| usize::from(drawn.height));
        match self.path {
            Some(_) => self.lines.len() - drawn + path,
            None => self.lines.len(),
        }
    }
}

/// An empty line the Host group's rows can be aired by, one before the row they name. Each is
/// drawn from rows the pane has to spare only: no row is cut or squeezed for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Gap {
    /// Before the Path row, which no key reaches.
    Path,
    /// Before the Spectrum row.
    Spectrum,
}

/// Where the Path row landed in the Controls pane and how it was laid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PathDrawn {
    /// The interior line the row starts on.
    pub(super) top: u16,
    /// How many lines it takes: more than `base` where it wrapped.
    pub(super) height: u16,
    /// How many lines it takes unwrapped: one, or two with a values line.
    pub(super) base: u16,
    /// Whether it wrapped onto continuation lines.
    pub(super) wrapped: bool,
    /// How many cells it runs past its room, where it is cut and scrolls.
    pub(super) travel: Option<usize>,
}

/// The Controls pane: one row per control a user can set, then the meters the device
/// streams, then — while host audio shows — the Host group, its Spectrum row last in the wide
/// and tall tiers, drawn for a pane whose rows have `room` columns after their cursor.
///
/// The pane sets its groups a blank line apart — the selects and toggles, the levels, the
/// balance, the meters, the Host group — wherever its `height` lines hold every row, the blanks
/// and the Path row at the most lines it can take; with fewer it packs them, so no row goes out
/// of sight. The tall tier draws a meter's
/// bar on two rows, its label and reading on the first; the wide tier lets its bars run longer.
pub(super) fn controls_pane(
    app: &App,
    theme: &Theme,
    room: usize,
    tier: Tier,
    height: usize,
) -> Controls {
    // The pane is built once with the groups spaced and the path wrapped; the other layouts'
    // heights follow from it (the blanks, the path's lines), so only the one chosen is built, if
    // it is another. In the order of what is given up first: the blank lines between the groups,
    // then the wrap (the path is cut and scrolls); a row is never lost. Rows still spare in a
    // layout that keeps the group blanks go to the Host group's gaps, which the first build names.
    // The choice counts the Path row at the most it can take in each of the two ways it is laid
    // (wrapped, cut), never at the lines the reading gives it now: the rows above it keep their
    // screen rows while a reading comes, goes or changes how long the row is, and what the row
    // does not use is blank space at the pane's end.
    let spaced = controls_lines(app, theme, room, tier, true, true, &[]);
    let blanks = spaced.blanks;
    let wrapped_lines = spaced.lines_reserving(path_line::MAX_LINES);
    let cut_lines = spaced.lines_reserving(path_line::MAX_BASE_LINES);
    let (groups, wrap, used) = match (
        wrapped_lines <= height,
        wrapped_lines - blanks <= height,
        cut_lines <= height,
    ) {
        (true, ..) => (true, true, wrapped_lines),
        // Packed still wraps the path.
        (_, true, _) => (false, true, wrapped_lines - blanks),
        // Spaced, with the path cut.
        (_, _, true) => (true, false, cut_lines),
        _ => (false, false, cut_lines - blanks),
    };
    // Only a layout that keeps the blanks between the groups has rows to air the Host group with.
    let spare = if groups {
        height.saturating_sub(used)
    } else {
        0
    };
    let air: Vec<Gap> = spaced.offered.iter().copied().take(spare).collect();
    match (groups && wrap, air.is_empty()) {
        (true, true) => spaced,
        _ => controls_lines(app, theme, room, tier, groups, wrap, &air),
    }
}

/// The group a row of the pane is spaced apart by: its group among the rows
/// ([`row_group`]), with the balance standing apart from the other levels.
fn spacing_group(control: &Control) -> (u8, bool) {
    (row_group(control), control.center.is_some())
}

/// The Controls pane's lines, its groups a blank line apart where `spaced`, a Path row too
/// long for its room wrapped onto continuation lines where `wrap` (else cut, and scrolled), and
/// a blank line before each Host group row `air` names.
pub(super) fn controls_lines(
    app: &App,
    theme: &Theme,
    room: usize,
    tier: Tier,
    spaced: bool,
    wrap: bool,
    air: &[Gap],
) -> Controls {
    #[cfg(test)]
    crate::tui::test_support::work::tick("controls_lines");
    app.drawn.controls_pending.set(false);
    let rows = pane_rows(app);
    let spectrum = app.spectrum_drawn_in(tier);
    let mut host_labels: Vec<&str> = match app.host_shown() {
        true => app.host_rows().iter().map(|row| row.label()).collect(),
        false => Vec::new(),
    };
    let path = app.path_row();
    if path.is_some() {
        host_labels.push(host::PATH);
    }
    if spectrum {
        host_labels.push(host::SPECTRUM);
    }
    let columns = columns_of(&rows, &host_labels, room);
    let mut pane = Controls {
        lines: Vec::new(),
        rows: Vec::new(),
        extras: Vec::new(),
        host: Vec::new(),
        spectrum: None,
        switch: None,
        path: None,
        blanks: 0,
        offered: Vec::new(),
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
            pane.blanks += 1;
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
        // A Volume row that carries the switch has it on its first line, a row of its own.
        let switch = lines.len() == 2
            && app.computer_offered()
            && app.volume().is_some_and(|volume| volume.id == control.id);
        for (n, line) in lines.into_iter().enumerate() {
            let focused = match switch {
                true if n == 0 => app.switch_focused(),
                _ => row.focused,
            };
            let line = widgets::rows::with_cursor(line, focused && (n == 0 || switch), theme);
            // The switch line is not banded: the side shown is the box that stands out on it.
            let banded = focused && !(switch && n == 0) && control.kind != ControlKind::Select;
            pane.lines.push(band(line, banded, row.stale, theme, room));
        }
        // Only the rows a user can set are rows a click can focus; the meters are a readout.
        if switch {
            pane.switch = Some(top);
        }
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
            (spaced, wrap),
            path.as_ref(),
            air,
            &mut pane,
        );
        if spectrum {
            if air.contains(&Gap::Spectrum) {
                pane.lines.push(Line::default());
            }
            let (line, dim) = spectrum_line(app, theme, columns);
            let line = widgets::rows::with_cursor(line, false, theme);
            pane.spectrum = u16::try_from(pane.lines.len()).ok();
            pane.lines
                .push(band(line, false, dim, theme, columns.room()));
        }
        if source == Some(LevelFrom::Host) {
            host_graph(app, theme, room, &mut pane);
        }
        pane.offered = [(pane.path.is_some(), Gap::Path), (spectrum, Gap::Spectrum)]
            .into_iter()
            .filter_map(|(offered, gap)| offered.then_some(gap))
            .collect();
    }
    app.drawn
        .controls_switch
        .set(app.drawn.controls_pending.get());
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
    let track = u16::try_from(columns.track()).unwrap_or(0);
    // The length a bar is drawn at, which is on its way to the value the row writes out
    // while that value is still gliding there — or the value a drag holds the bar at, which
    // is drawn at once.
    let eased = app
        .dragged_raw(control.id)
        .or_else(|| app.eased_raw(control.id, app.now));
    match control.kind {
        ControlKind::Select => {
            let line = widgets::rows::select_line(
                control,
                reading,
                theme,
                columns,
                row.focused && !app.switch_focused(),
            );
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
                    track,
                    Hit::Level(index),
                ));
                // The track takes a press and a drag of its own, over the row's hit.
                extras.push((
                    top,
                    CURSOR_OFFSET + field + BALANCE_TRACK_OFFSET,
                    track,
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
            let volume = app.volume().is_some_and(|volume| volume.id == control.id);
            let on_computer = volume && app.on_computer();
            if has_bar(control, reading) && !on_computer {
                extras.push((top, CURSOR_OFFSET + field, bar, Hit::Level(index)));
                extras.push((top, CURSOR_OFFSET + field, bar, Hit::LevelBar(index)));
            }
            if on_computer {
                extras.push((top, CURSOR_OFFSET + field, bar, Hit::Level(index)));
            }
            // The volume carries the step a boosting curve can clip from, and the target it
            // is climbing to.
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
            let line = match on_computer {
                true => computer_volume_line(app, theme, columns, (&control.label, row.focused)),
                false => widgets::rows::level_line(
                    control,
                    reading,
                    theme,
                    columns,
                    eased,
                    (row.focused, row.stale),
                    (clip, target),
                ),
            };
            // The switch between the device's volume and the computer's, on a line of its own over
            // the bar's.
            if volume
                && app.computer_offered()
                && let Some((switch, names)) = switch_line(app, theme, columns)
            {
                if let Some([_, (at, width, _)]) = names {
                    extras.push((top, CURSOR_OFFSET, at + width, Hit::Target));
                }
                for (at, width, side) in names.into_iter().flatten() {
                    extras.push((top, CURSOR_OFFSET + at, width, Hit::Side(side)));
                }
                // The bar's hits are on the second line.
                for extra in extras.iter_mut().filter(|extra| {
                    extra.0 == top && !matches!(extra.3, Hit::Side(_) | Hit::Target)
                }) {
                    extra.0 += 1;
                }
                return vec![switch, line];
            }
            vec![line]
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
/// where the pane is `spaced`, as between its other groups — and the row for its media player's
/// transport, the row the keys reach, drawn in `columns`.
fn host_group(
    app: &App,
    theme: &Theme,
    columns: Columns,
    (spaced, wrap): (bool, bool),
    path: Option<&PathRow>,
    air: &[Gap],
    pane: &mut Controls,
) {
    if spaced {
        pane.lines.push(Line::default());
        pane.blanks += 1;
    }
    pane.lines.push(Line::from(vec![
        Span::raw(" ".repeat(CURSOR_WIDTH)),
        Span::styled(host::HEADING, theme.style(Role::Dim)),
    ]));
    for row in &app.host_rows() {
        let top = u16::try_from(pane.lines.len()).unwrap_or(u16::MAX);
        let on_switch = *row == HostRow::Volume && app.switch_focused();
        let focused = app.host_row() == Some(*row) && !on_switch;
        let (line, dim) = match row {
            HostRow::Volume => {
                // The device has no volume of its own: the row is the switch, and on the computer's
                // side the bar under it.
                let (switch, names) = switch_line(app, theme, columns).unwrap_or_else(|| {
                    (
                        Line::from(widgets::rows::label(row.label(), columns.field())),
                        None,
                    )
                });
                if names.is_some() {
                    pane.switch = Some(top);
                }
                if let Some([_, (at, width, _)]) = names {
                    pane.extras
                        .push((top, CURSOR_OFFSET, at + width, Hit::Target));
                }
                for (at, width, side) in names.into_iter().flatten() {
                    pane.extras
                        .push((top, CURSOR_OFFSET + at, width, Hit::Side(side)));
                }
                if app.on_computer() {
                    let bar = computer_volume_line(app, theme, columns, (row.label(), focused));
                    let line = widgets::rows::with_cursor(switch, on_switch, theme);
                    pane.lines
                        .push(band(line, false, false, theme, columns.room()));
                    pane.extras.push((
                        top + 1,
                        CURSOR_OFFSET + u16::try_from(columns.field()).unwrap_or(0),
                        u16::try_from(columns.bar()).unwrap_or(0),
                        Hit::Level(app.rows().len()),
                    ));
                    (bar, false)
                } else {
                    (switch, false)
                }
            }
            HostRow::Transport => {
                let (line, buttons) = transport_line(app, theme, columns, focused);
                for (button, (x, width)) in buttons.into_iter().enumerate() {
                    pane.extras
                        .push((top, CURSOR_OFFSET + x, width, Hit::Transport(button)));
                }
                (line, app.host.playing.is_none())
            }
        };
        // On the device's side the Volume row is the switch line itself: it wears the cursor.
        let marked = focused || (on_switch && !app.on_computer());
        let line = widgets::rows::with_cursor(line, marked, theme);
        // The Transport row is a choice among buttons: it takes no band, only its label and the
        // button in force stand out.
        let banded = focused && *row != HostRow::Transport;
        pane.lines
            .push(band(line, banded, dim, theme, columns.room()));
        pane.host.push(top);
        // The path is a reading under the transport it follows, which no key reaches.
        if let (HostRow::Transport, Some(path)) = (row, path) {
            if air.contains(&Gap::Path) {
                pane.lines.push(Line::default());
            }
            let top = u16::try_from(pane.lines.len()).unwrap_or(u16::MAX);
            let scroll = app
                .tui
                .scroll_long_lines
                .then_some(usize::from(app.host.path_scroll.offset));
            let rows = path_rows(path, theme, columns, (wrap, scroll));
            pane.path = Some(PathDrawn {
                top,
                height: u16::try_from(rows.lines.len()).unwrap_or(1),
                base: u16::try_from(rows.base).unwrap_or(1),
                wrapped: rows.wrapped,
                travel: rows.travel,
            });
            for line in rows.lines {
                let line = widgets::rows::with_cursor(line, false, theme);
                pane.lines
                    .push(band(line, false, false, theme, columns.room()));
            }
        }
    }
}

/// The Path row as the pane lays it out.
struct PathRows {
    /// The row's lines: one per line of the model, or the continuation lines it wrapped onto.
    lines: Vec<Line<'static>>,
    /// How many lines the row takes when it does not wrap: one, or two with a values line.
    base: usize,
    /// Whether the row wrapped onto more lines than that.
    wrapped: bool,
    /// How many cells the row's words run past its room, where it is cut and scrolls.
    travel: Option<usize>,
}

/// The label of the Path row: its word in the label's tone, padded to the label column.
fn path_label(row: &PathRow, theme: &Theme, columns: Columns) -> Vec<Span<'static>> {
    let plain = widgets::rows::label(host::PATH, columns.field());
    let style = match row.label.tone {
        Tone::Plain => return vec![plain],
        tone => theme.style(path_line::role(tone)),
    };
    let word = host::PATH.len().min(columns.field().saturating_sub(1));
    let (word, pad) = plain.content.split_at(word);
    vec![
        Span::styled(word.to_string(), style),
        Span::raw(pad.to_string()),
    ]
}

/// The Path row in `columns`, scrolled from the offset `scroll` holds (`None` with scrolling
/// off): its two lines (the hops, then the values) each on one line where the words fit; else,
/// where `wrap` allows, on the fewest lines the separators break them into (five at most, a hop
/// broken only between its words), each continuation indented under the first line's words.
/// Where both lines do not fit five lines the values line wraps alone (it holds the DAC's changes
/// and the config tag) and the hops line is cut or scrolled on one line; where even the values
/// do not fit, both are cut, the config tag kept whole while scrolling is off. A cut line
/// scrolls slowly past its room while the `scroll_long_lines` setting is on, from the offset the
/// app holds, and ends in `…` while it is off.
fn path_rows(
    row: &PathRow,
    theme: &Theme,
    columns: Columns,
    (wrap, scroll): (bool, Option<usize>),
) -> PathRows {
    let stages = path_line::row_lines(row);
    let room = columns.room().saturating_sub(columns.field());
    let lines: Vec<&Vec<path_line::Stage>> = stages.lines().collect();
    let base = lines.len().max(1);
    let lead = |at: usize| match at {
        0 => path_label(row, theme, columns),
        _ => vec![Span::raw(" ".repeat(columns.field()))],
    };
    let drawn = |spans: Vec<Vec<Span<'static>>>| -> Vec<Line<'static>> {
        let mut spans = spans;
        if spans.is_empty() {
            spans.push(Vec::new());
        }
        spans
            .into_iter()
            .enumerate()
            .map(|(at, words)| Line::from(lead(at).into_iter().chain(words).collect::<Vec<_>>()))
            .collect()
    };
    let width_of = |line: &Vec<path_line::Stage>| path_line::full_width(line);
    let width = lines.iter().map(|line| width_of(line)).max().unwrap_or(0);
    let colour = |parts: Vec<(String, Role)>| -> Vec<Span<'static>> {
        parts
            .into_iter()
            .map(|(text, role)| Span::styled(text, theme.style(role)))
            .collect()
    };
    // Every line cut at the right edge, the config tag of the values kept whole.
    let cut_all = || -> Vec<Vec<Span<'static>>> {
        lines
            .iter()
            .map(|line| path_line::cut_keeping_config(line, theme, room))
            .collect()
    };
    let one = |travel, spans: Vec<Vec<Span<'static>>>| PathRows {
        lines: drawn(spans),
        base,
        wrapped: false,
        travel,
    };
    if width <= room {
        return one(None, cut_all());
    }
    // The line a cut row shows on one line: from the held offset, or cut with `…`.
    let held = scroll.filter(|_| room > 0);
    let show_cut = |line: &Vec<path_line::Stage>, offset: usize| match held {
        Some(_) => path_line::window(
            line,
            theme,
            offset.min(width_of(line).saturating_sub(room)),
            room,
        ),
        None => path_line::cut_keeping_config(line, theme, room),
    };
    if wrap {
        let each: Option<Vec<_>> = lines
            .iter()
            .map(|line| path_line::wrap(line, room, path_line::MAX_LINES))
            .collect();
        let both = each
            .map(|all| all.into_iter().flatten().collect::<Vec<_>>())
            .filter(|all| all.len() <= path_line::MAX_LINES);
        if let Some(both) = both {
            return PathRows {
                lines: drawn(both.into_iter().map(colour).collect()),
                base,
                wrapped: true,
                travel: None,
            };
        }
        // The values wrap alone, the hops line on one line cut or scrolled.
        if let [hops, values] = lines[..]
            && let Some(wrapped) = path_line::wrap(values, room, path_line::MAX_LINES - 1)
        {
            let own = width_of(hops).saturating_sub(room);
            let offset = held.unwrap_or(0).min(own);
            let mut spans = vec![match own > 0 {
                true => show_cut(hops, offset),
                false => path_line::cut(hops, theme, room),
            }];
            spans.extend(wrapped.into_iter().map(colour));
            return PathRows {
                lines: drawn(spans),
                base,
                wrapped: true,
                travel: held.and(Some(own)).filter(|own| *own > 0),
            };
        }
    }
    let Some(held) = held else {
        return one(None, cut_all());
    };
    let travel = width - room;
    let offset = held.min(travel);
    one(
        Some(travel),
        lines.iter().map(|line| show_cut(line, offset)).collect(),
    )
}

/// The Path row of `row` on its lines as the tests read them: wrapped where `wrap`, and where it
/// is cut scrolled from `scroll` (`None`: cut with `…`).
#[cfg(test)]
pub(super) fn path_row(
    row: &PathRow,
    theme: &Theme,
    columns: Columns,
    (wrap, scroll): (bool, Option<usize>),
) -> Vec<Line<'static>> {
    path_rows(row, theme, columns, (wrap, scroll)).lines
}

/// What the computer's volume row says `muted` with, after its bar.
const MUTED: &str = "muted";

/// The Volume row on the computer's side: a level's bar from 0 to 100 % with the volume written
/// inside it, `muted` while the sink is, and the sink's own name dim after them, cut to what the
/// row has left. Before the volume has been read the row reads `—`, dim, and never a live 0 %.
fn computer_volume_line(
    app: &App,
    theme: &Theme,
    columns: Columns,
    (label, focused): (&str, bool),
) -> Line<'static> {
    let mut spans = vec![widgets::rows::label(label, columns.field())];
    let Some(volume) = app.host.volume() else {
        spans.push(Span::styled(MISSING, theme.style(Role::Dim)));
        return Line::from(spans);
    };
    let percent = volume.pct;
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
    if volume.muted && left >= VALUE_GAP + MUTED.len() {
        spans.push(Span::raw(" ".repeat(VALUE_GAP)));
        spans.push(Span::styled(MUTED, theme.style(Role::Warn)));
        left -= VALUE_GAP + MUTED.len();
    }
    if !volume.sink.is_empty() && left > VALUE_GAP {
        spans.push(Span::raw(" ".repeat(VALUE_GAP)));
        spans.push(Span::styled(
            cut_cells(&volume.sink, left - VALUE_GAP),
            theme.style(Role::Dim),
        ));
    }
    Line::from(spans)
}

/// The switch line over the Volume row's bar: `Target ‹ NAME | Computer ›`, the side shown
/// standing out, with where its two names lie from the row's start (a column, a width and the
/// side each shows); where the computer's controls cannot be used, the reason, dim, in its place.
/// Nothing where the row has no room for the switch.
#[allow(clippy::type_complexity, reason = "one private pair of answers")]
fn switch_line(
    app: &App,
    theme: &Theme,
    columns: Columns,
) -> Option<(Line<'static>, Option<[(u16, u16, Side); 2]>)> {
    let head = Style::default();
    if let Some(why) = app.host.computer.unavailable() {
        let spans = switch::unavailable(theme, columns.room(), head, why);
        return Some((Line::from(spans), None));
    }
    let drawn = switch::draw(app, theme, columns.room(), head, app.switch_focused())?;
    app.drawn.controls_pending.set(true);
    let cells = |at: usize, width: usize, side| {
        Some((u16::try_from(at).ok()?, u16::try_from(width).ok()?, side))
    };
    let [(a, wa, sa), (b, wb, sb)] = drawn.names;
    let names = cells(a, wa, sa).zip(cells(b, wb, sb));
    Some((Line::from(drawn.spans), names.map(|(a, b)| [a, b])))
}

/// How many columns the Spectrum row needs before it spares a word for its source.
const SOURCE_MIN_ROOM: usize = 24;

/// Columns between a row's bar and what follows it.
const VALUE_GAP: usize = 2;

/// The Spectrum row, the Host group's last, and whether it is drawn dim: what the default sink
/// plays, in the capture's bands from 40 Hz to 16 kHz, each a block glyph as tall as the band is
/// loud — every block's bands as it lands, falling back between blocks — while the capture of
/// the sink's monitor runs; `—`, dim, while none does. A row with room for fewer cells than
/// there are bands draws as many as it has room for, from the lowest; a wider one lays the bands
/// on the same log axis across all its room, between neighbours ([`host::spread`]), each cell
/// coloured along the spectrum, red at the bass to violet at the treble. The keys do not reach it:
/// it is a reading, as a meter's row is.
fn spectrum_line(app: &App, theme: &Theme, columns: Columns) -> (Line<'static>, bool) {
    let mut spans = vec![widgets::rows::label(host::SPECTRUM, columns.field())];
    if !app.host.capturing {
        spans.push(Span::raw(MISSING));
        return (Line::from(spans), true);
    }
    // the row runs from the label to the pane's right edge, as the Volume row's value does
    let room = columns.room().saturating_sub(columns.field());
    // the row's last word says where the bands come from, when the row is wide enough to spare it
    let source = match app.host.from_daemon {
        true => "daemon",
        false => "own",
    };
    let word = match room >= SOURCE_MIN_ROOM {
        true => source.len() + 1,
        false => 0,
    };
    let room = room - word;
    let bands = app.host.bands.at(app.now);
    let levels = host::spread(&bands, room);
    // a beat of the daemon's frames makes the bands bold for a moment
    let pulse = match app.row_pulse() > 0.0 {
        true => Modifier::BOLD,
        false => Modifier::empty(),
    };
    let own = theme.style(Role::Bar).add_modifier(pulse);
    let last = levels.len().saturating_sub(1).max(1) as f32;
    // red at the bass end to violet at the treble, where the theme has a spectrum to draw
    spans.extend(levels.into_iter().enumerate().map(|(at, level)| {
        let style = match theme.spectrum_row_colour(at as f32 / last) {
            Some(colour) => own.fg(colour),
            None => own,
        };
        Span::styled(host::glyph(level).to_string(), style)
    }));
    if word > 0 {
        spans.push(Span::styled(format!(" {source}"), theme.style(Role::Dim)));
    }
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
    // With the keys on the row only its label and the button under the cursor take the selected
    // style, as the Target switch has it; the other buttons are dim.
    let (head, lit, other) = switch::styles(theme, Style::default(), focused);
    let mut label = widgets::rows::label(host::TRANSPORT, columns.field());
    label.style = head;
    let mut spans = vec![label];
    let mut at = columns.field();
    let mut buttons = Vec::with_capacity(host::GLYPHS.len());
    let last = host::GLYPHS.len() - 1;
    for (index, (glyph, button)) in host::GLYPHS.iter().zip(Button::ALL).enumerate() {
        if index > 0 {
            spans.push(Span::raw(" ".repeat(host::BUTTON_GAP)));
            at += host::BUTTON_GAP;
        }
        let state = theme.style(Role::Cursor).add_modifier(Modifier::BOLD);
        let style = match (app.host.lit() == Some(button), focused) {
            // the cursor's button shows the selected box, even over the state's accent
            (true, true) if app.host.button() == button => lit.add_modifier(Modifier::BOLD),
            (true, _) => state,
            (false, true) if app.host.button() == button => lit,
            (false, true) => other,
            (false, false) => Style::default(),
        };
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
    spans.extend(track_clock(app, theme, columns.room().saturating_sub(at)));
    (Line::from(spans), buttons)
}

/// Columns between the buttons and the time, and between the time and the bar.
const CLOCK_GAP: usize = 2;
/// The fewest cells the track's bar is drawn in.
const CLOCK_BAR_MIN: usize = 4;

/// What follows the buttons on the Transport row in `room` cells: the time into the track and
/// its length (`2:41 / 9:22`), then a thin bar filling what is left, its elapsed part in the bar
/// colour over a dim track. A row too narrow for the bar goes without it, then without `/ total`,
/// the time elapsed last; no length gives the time elapsed alone, and no position nothing.
fn track_clock(app: &App, theme: &Theme, room: usize) -> Vec<Span<'static>> {
    let Some(time) = app.host.track_time(app.now) else {
        return Vec::new();
    };
    let elapsed = host::clock_words(time.elapsed_us);
    let total = time.length_us.map(host::clock_words);
    let both = total.as_ref().map(|total| format!("{elapsed} / {total}"));
    let words = |text: &str| {
        vec![
            Span::raw(" ".repeat(CLOCK_GAP)),
            Span::styled(text.to_string(), theme.style(Role::Dim)),
        ]
    };
    let width = |text: &str| CLOCK_GAP + widgets::text::cells(text);
    let Some(both) = both else {
        return match room >= width(&elapsed) {
            true => words(&elapsed),
            false => Vec::new(),
        };
    };
    let bar = room.saturating_sub(width(&both) + CLOCK_GAP);
    if bar >= CLOCK_BAR_MIN {
        let length = time.length_us.unwrap_or(0).max(1);
        let ratio = time.elapsed_us as f64 / length as f64;
        let filled = ((bar as f64 * ratio).round() as usize).min(bar);
        let mut spans = words(&both);
        spans.push(Span::raw(" ".repeat(CLOCK_GAP)));
        spans.push(Span::styled("━".repeat(filled), theme.style(Role::Bar)));
        spans.push(Span::styled(
            "─".repeat(bar - filled),
            theme.style(Role::Dim),
        ));
        return spans;
    }
    match room {
        r if r >= width(&both) => words(&both),
        r if r >= width(&elapsed) => words(&elapsed),
        _ => Vec::new(),
    }
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
    // With the keys on the switch row over it, the Volume row is not the focused one.
    let focused = app
        .selected_control()
        .map(|control| control.id)
        .filter(|_| !app.switch_focused());
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
/// longest label the pane has and the widest value any of its bars can carry, with bars taking
/// the rest of the row.
fn columns_of(rows: &[Row<'_>], more: &[&str], room: usize) -> Columns {
    let longest = rows
        .iter()
        .map(|row| row.control.label.chars().count())
        .chain(more.iter().map(|label| label.chars().count()))
        .max()
        .unwrap_or(0);
    let widest = rows
        .iter()
        .filter_map(|row| widgets::rows::widest_bar_value(row.control))
        .max()
        .unwrap_or(0);
    Columns::fit(room, longest, widest)
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
