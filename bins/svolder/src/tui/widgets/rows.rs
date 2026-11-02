// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The label-and-value rows of a control pane: select, toggle, level and balance rows, the bar
//! they share, and the columns the rows of one pane keep in common.
//!
//! Every row draws one control's label and the value its last reading carried. The rows of a pane
//! share their columns, which [`Columns::fit`] works out from the pane's width so every value
//! starts in the same place; a level or balance row adds a bar, drawn only for a control whose
//! profile names the range it runs over. The meter rows, which look like level rows, are in
//! [`meters`](super::meters).

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use svolder_profiles::{Control, ControlKind, units};

use super::text::{cells, cut_to_cells};
use super::{MISSING, TICK};
use crate::tui::app::Reading;
use crate::tui::theme::{Role, Theme};

/// The widest label a pane gives its label column; a longer one is cut to fit.
const LABEL_MAX: usize = 12;

/// The fewest, which a pane too narrow for more keeps anyway.
const BAR_MIN: usize = 8;

/// The narrowest label column a row gives way to for its bar and value: two letters, the `…`
/// that cuts the rest, and the space before the value.
const LABEL_FLOOR: usize = 4;

/// Columns between a bar and the value beside it, where the row has room for them.
const VALUE_GAP: usize = 2;

/// What a balance row marks the reading with, on its own cell of the track.
pub const BALANCE_MARK: &str = "┃";

/// The columns the rows of one pane share.
#[derive(Debug, Clone, Copy)]
pub struct Columns {
    /// Columns the label takes, the space before the value included.
    pub(super) field: usize,
    /// Cells a bar, or a balance track, is drawn across.
    pub(super) bar: usize,
    /// Columns a row has in all, after its cursor.
    pub(super) room: usize,
}

impl Columns {
    /// The columns a pane whose rows have `room` columns after their cursor gives them, for
    /// rows whose longest label is `longest` characters and whose widest value beside a bar
    /// is `widest`, with bars no longer than `bar_max`.
    ///
    /// The label column takes the longest label the pane has, so every value starts in the
    /// same place, and the bar takes what is left of the row. A row too narrow for the label,
    /// the bar at [`BAR_MIN`] and the value shrinks the label column first (a label wider than
    /// it is cut with `…` where it is drawn), down to [`LABEL_FLOOR`], and only then the bar,
    /// so the three stay within `room` unless the value alone is wider than the row, which no
    /// built-in profile reaches (the bar is then one cell).
    pub fn fit(room: usize, longest: usize, widest: usize, bar_max: usize) -> Columns {
        let wanted = longest.min(LABEL_MAX) + 1;
        let rest = room.saturating_sub(VALUE_GAP + widest);
        let field = wanted.min(rest.saturating_sub(BAR_MIN).max(LABEL_FLOOR));
        let bar = rest.saturating_sub(field).min(bar_max.max(BAR_MIN)).max(1);
        Columns { field, bar, room }
    }

    /// Columns the label takes, the space before the value included: where a toggle's box, a
    /// select's arrows and a level's bar all start, for the mouse hits a row draws over them.
    pub(crate) fn field(&self) -> usize {
        self.field
    }

    /// Cells a bar, or a balance track, is drawn across, for the mouse hit over it.
    pub(crate) fn bar(&self) -> usize {
        self.bar
    }

    /// Columns a row has in all, after its cursor.
    pub(crate) fn room(&self) -> usize {
        self.room
    }
}

/// One select row: the raw value and the name the profile gives it.
pub fn select_line(
    control: &Control,
    reading: Option<&Reading>,
    theme: &Theme,
    columns: Columns,
) -> Line<'static> {
    let mut spans = vec![label(&control.label, columns.field)];
    match reading.and_then(Reading::raw_i64) {
        None => spans.push(Span::raw(MISSING)),
        Some(raw) => {
            spans.push(Span::raw(format!("‹ {raw}")));
            if let Some(name) = u8::try_from(raw).ok().and_then(|v| control.value_name(v)) {
                spans.push(Span::raw(" · "));
                spans.push(Span::styled(
                    name.to_string(),
                    theme.style(Role::SelectName),
                ));
            }
            spans.push(Span::raw(" ›"));
        }
    }
    Line::from(spans)
}

/// One toggle row: the box, and the word the reading itself carries.
pub fn toggle_line(
    control: &Control,
    reading: Option<&Reading>,
    theme: &Theme,
    columns: Columns,
) -> Line<'static> {
    let on = reading.is_some_and(Reading::is_active);
    let (box_text, style) = match on {
        true => ("[x]", theme.style(Role::ToggleOn)),
        false => ("[ ]", Style::default()),
    };
    Line::from(vec![
        label(&control.label, columns.field),
        Span::styled(box_text, style),
        Span::raw(" "),
        Span::raw(word(reading)),
    ])
}

/// One level row: the bar, with the raw value over its range written inside it, then the
/// converted value after it.
///
/// A control whose profile names no range has no bar and shows its value alone. The raw value
/// (`63/127`) is centred over the bar's own cells in place of the glyphs it covers: reversed on
/// each filled cell's own shade (reads as a hole in the bar), in the text role over an empty
/// cell. A bar too short for the text is drawn without it.
///
/// `eased` is the raw value the bar draws at while a change is still gliding to the value the
/// device confirmed; the numbers written out are always the confirmed value. `selected` is
/// whether the row is focused, where the bar keeps its own colours (see [`bar_spans`]). `stale`
/// is whether the row shows a value unconfirmed since the link went down, where digits over the
/// bar's empty cells take the stale role the rest of the row uses.
///
/// `clip` is the highest raw value a boosting tone curve stays clear of clipping at, for the
/// volume while there is one. The bar's last filled cell at that step carries [`TICK`] (warn
/// role at or below the step, bold alert role once past it — a cell holds several steps, so
/// ones past the threshold sharing the tick's cell must not look safe). Every cell the bar
/// reaches only past the step is the clipping-risk zone: filled in the alert role rather than
/// the bar's own shade (as `▓` beside an accent near the alert red, see [`Theme::zone_fill`]),
/// empty in the alert's dim shade ([`Theme::alert_ground`]). The tick is a
/// glyph so the threshold still reads in monochrome (by weight) and beside an alert-red accent.
/// The raw value over a zone cell keeps its own rule (reversed on red where filled, text role
/// where not) and moves aside for the tick where the bar has room ([`clear_of`]), giving way to
/// it on the one cell they'd otherwise share.
///
/// `target` is the raw value the volume is climbing to, `None` for every other row and while it
/// is not climbing ([`level_bar`] draws its cell dim).
pub fn level_line(
    control: &Control,
    reading: Option<&Reading>,
    theme: &Theme,
    columns: Columns,
    eased: Option<i64>,
    (selected, stale): (bool, bool),
    (clip, target): (Option<i64>, Option<i64>),
) -> Line<'static> {
    let (Some(raw), Some((min, max))) = (reading.and_then(Reading::raw_i64), control.range) else {
        return plain_row(control, reading, columns);
    };
    let mut spans = vec![label(&control.label, columns.field)];
    spans.extend(level_bar(
        theme,
        (eased.unwrap_or(raw), raw),
        (min, max),
        columns.bar,
        &format!("{raw}/{max}"),
        (selected, stale),
        (clip, target),
    ));
    if let Some(text) = bar_value(control, reading) {
        spans.push(Span::raw(format!("{}{text}", gap(&text, columns))));
    }
    Line::from(spans)
}

/// A level's bar `width` cells long, filled to where `drawn` sits in `range` and shaded along
/// its length as [`bar_shade_style`] shades it, with `text` centred over its cells where it fits
/// (reversed on a filled cell's shade, text role on an empty one, stale role if `stale`). The
/// cells from [`zone_start`] for `clip` on are the clipping-risk zone (filled in the alert role,
/// empty in [`Theme::alert_ground`]); the cell before them carries [`TICK`] over whatever else
/// it would draw, in the style [`tick_style`] gives the confirmed `reading` — the text moves
/// aside for it first where the bar has room. `target` is the raw value the volume is climbing
/// to, if it is: its cell, when the bar has not filled it yet, is drawn in the dim role.
pub(crate) fn level_bar(
    theme: &Theme,
    (drawn, reading): (i64, i64),
    range: (i64, i64),
    width: usize,
    text: &str,
    (selected, stale): (bool, bool),
    (clip, target): (Option<i64>, Option<i64>),
) -> Vec<Span<'static>> {
    let letters: Vec<char> = text.chars().collect();
    let filled = fill(drawn, range, width);
    let zone = zone_start(clip, range, width);
    let aim = target.map(|target| fill(target, range, width).min(width.saturating_sub(1)));
    // The threshold step's own cell: the last the bar fills at it, the one before the zone.
    let tick = clip.and_then(|_| zone.checked_sub(1));
    // A bar too short for the text is drawn without it.
    let start = width
        .checked_sub(letters.len())
        .map(|room| clear_of(room / 2, letters.len(), tick, width));
    let ground = bar_ground(theme, selected);
    let digits = own_colours(theme.style(match stale {
        true => Role::Stale,
        false => Role::Text,
    }));
    (0..width)
        .map(|i| {
            if tick == Some(i) {
                return Span::styled(TICK, tick_style(theme, reading, clip));
            }
            let letter = start
                .and_then(|start| i.checked_sub(start))
                .and_then(|at| letters.get(at));
            // Where the volume is climbing to, past the cells it has filled.
            if aim == Some(i) && i >= filled {
                let glyph = letter.map_or_else(|| "█".to_string(), char::to_string);
                return Span::styled(glyph, own_colours(theme.style(Role::Dim)));
            }
            let (shade, ground, fill) = match i >= zone {
                true => (
                    own_colours(theme.style(Role::Alert)),
                    own_colours(theme.alert_ground()),
                    theme.zone_fill(),
                ),
                false => (own_colours(bar_shade_style(theme, i, width)), ground, "█"),
            };
            match (letter, i < filled) {
                (Some(letter), true) => {
                    Span::styled(letter.to_string(), shade.add_modifier(Modifier::REVERSED))
                }
                (Some(letter), false) => Span::styled(letter.to_string(), digits),
                (None, true) => Span::styled(fill, shade),
                (None, false) => Span::styled("░", ground),
            }
        })
        .collect()
}

/// The style the volume bar's tick is drawn in for a `reading` against the threshold step
/// `clip`: the warn role while the reading is at or below the step, and the alert role, bold,
/// once it is past it — the weight is what still tells the two apart in monochrome. Never
/// reversed, and the focused row's own weight left off the warn one, as every bar cell's is.
fn tick_style(theme: &Theme, reading: i64, clip: Option<i64>) -> Style {
    match clip.is_some_and(|clip| reading > clip) {
        true => own_colours(theme.style(Role::Alert)).add_modifier(Modifier::BOLD),
        false => own_colours(theme.style(Role::Warn)),
    }
}

/// Where text `len` cells long starts on a bar `width` cells long, from the start `centred`
/// that centres it, once it has moved aside for the tick on cell `tick`: the nearest start that
/// leaves the tick's cell clear, so the value reads whole, or `centred` itself where the text
/// does not reach the tick or the bar has no room to move it — and there the tick takes its
/// one cell of the text.
fn clear_of(centred: usize, len: usize, tick: Option<usize>, width: usize) -> usize {
    let Some(tick) = tick.filter(|tick| (centred..centred + len).contains(tick)) else {
        return centred;
    };
    let before = tick.checked_sub(len);
    let after = (tick + 1 + len <= width).then_some(tick + 1);
    [before, after]
        .into_iter()
        .flatten()
        .min_by_key(|start| start.abs_diff(centred))
        .unwrap_or(centred)
}

/// The first of a bar's `width` cells over `range` that the clipping-risk zone takes, for a
/// curve that clips past raw value `clip`: the cells the bar fills only once it reads more than
/// `clip`, which are every cell from the one it stops short of at `clip` itself. `width` — no
/// cell — without a step to warn from.
fn zone_start(clip: Option<i64>, range: (i64, i64), width: usize) -> usize {
    clip.map_or(width, |clip| fill(clip, range, width))
}

/// One balance row: a track with the marker where the reading sits, and how far that is from
/// the centre the profile names.
///
/// The marker is [`BALANCE_MARK`], a heavy bar on a light track filling its own cell's width so
/// it reads as sitting on the column it names, not beside it (the same glyph a meter uses for
/// its class-A tick).
///
/// `eased` is where the marker draws while a change is still gliding to the reading behind it;
/// the distance written out is always the confirmed one. `selected` is whether the row is
/// focused, where the track keeps its own colours as a bar does (see [`bar_spans`]).
pub fn balance_line(
    control: &Control,
    reading: Option<&Reading>,
    theme: &Theme,
    columns: Columns,
    eased: Option<i64>,
    selected: bool,
) -> Line<'static> {
    let (Some(raw), Some((min, max)), Some(text)) = (
        reading.and_then(Reading::raw_i64),
        control.range,
        bar_value(control, reading),
    ) else {
        return plain_row(control, reading, columns);
    };
    let at = position(eased.unwrap_or(raw), (min, max), columns.bar);
    let ground = bar_ground(theme, selected);
    Line::from(vec![
        label(&control.label, columns.field),
        Span::raw("L "),
        Span::styled("─".repeat(at), ground),
        Span::styled(BALANCE_MARK, own_colours(theme.style(Role::Bar))),
        Span::styled("─".repeat(columns.bar - 1 - at), ground),
        Span::raw(format!(" R {text}")),
    ])
}

/// `style` held to its own colours on whatever row it is drawn on: the reverse video and the
/// weight the focused row takes are dropped from it, so a bar's cells keep their shades on the
/// row the cursor is on rather than turning into the row's own inverse.
pub fn own_colours(style: Style) -> Style {
    style.remove_modifier(Modifier::REVERSED | Modifier::BOLD)
}

/// What a bar's empty cells, and a track's ground, draw in: faint on any row, or on the focused
/// row [`Theme::bar_dim`] instead (the dim shade of the accent, or the emphasis where the
/// palette names one), so the bar reads as part of the row it is on.
fn bar_ground(theme: &Theme, selected: bool) -> Style {
    match selected {
        true => own_colours(theme.bar_dim()),
        false => own_colours(theme.style(Role::Faint)),
    }
}

/// The text beside a row's bar, for the rows that draw one: a level's value as every front end
/// writes it, built from its raw step (`-6.0 dB`, `silence`), a balance's distance from its
/// centre right-aligned in two cells, a meter's percentage. `None` for a row with no bar, for a
/// level with neither a centre nor a conversion, and for a raw step outside its table.
pub fn bar_value(control: &Control, reading: Option<&Reading>) -> Option<String> {
    if control.kind == ControlKind::Meter {
        // The widest text a meter row writes beside its bar.
        return control.scale().map(|_| "100%".to_string());
    }
    let raw = reading.and_then(Reading::raw_i64)?;
    control.range?;
    match (control.kind, control.center) {
        (ControlKind::Level, Some(_)) => units::text(control, raw).map(|text| format!("{text:>2}")),
        (ControlKind::Level, None) => {
            control.convert.as_ref()?;
            units::text(control, raw)
        }
        _ => None,
    }
}

/// `line` with the row cursor in front: `▶` on the focused row, blank on every other.
pub fn with_cursor(line: Line<'static>, focused: bool, theme: &Theme) -> Line<'static> {
    let mark = match focused {
        true => Span::styled("▶", theme.style(Role::Cursor)),
        false => Span::raw(" "),
    };
    let mut spans = vec![Span::raw(" "), mark, Span::raw(" ")];
    spans.extend(line.spans);
    Line::from(spans)
}

/// `width` cells of bar, filled in proportion to where `raw` sits in `range`: each filled cell
/// styled by `style_at` at its own index, so a bar can shade or blend along its length rather
/// than draw every cell alike; unreached cells are dotted and faint.
///
/// Every cell keeps its own colours on the focused row (`selected`) rather than taking the
/// row's reverse video (which would turn a filled cell into a block of the row's colour); there
/// the unreached cells take the dim shade of the row's selected colour instead of faint grey.
pub fn bar_spans(
    theme: &Theme,
    raw: i64,
    range: (i64, i64),
    width: usize,
    selected: bool,
    style_at: impl Fn(usize) -> Style,
) -> Vec<Span<'static>> {
    bar_spans_of(theme, fill(raw, range, width), width, selected, &style_at)
}

/// `width` cells of bar with the first `filled` of them filled, styled as [`bar_spans`] styles
/// them.
pub(super) fn bar_spans_of(
    theme: &Theme,
    filled: usize,
    width: usize,
    selected: bool,
    style_at: &dyn Fn(usize) -> Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = (0..filled)
        .map(|i| Span::styled("█", own_colours(style_at(i))))
        .collect();
    spans.push(Span::styled(
        "░".repeat(width - filled),
        bar_ground(theme, selected),
    ));
    spans
}

/// `text` as a badge: the state it names in inverse video, which reads as a state at every
/// colour depth and in monochrome alike. On a `selected` row, which is already in inverse video,
/// the badge drops the reverse and stays bold, so it reads as a hole in the row's band.
pub fn badge(text: &str, theme: &Theme, selected: bool) -> Span<'static> {
    let style = theme.style(Role::Badge);
    let style = match selected {
        true => style.remove_modifier(Modifier::REVERSED),
        false => style,
    };
    Span::styled(text.to_string(), style)
}

/// A bar's filled cell `i` (of `width`): [`Theme::bar_shade`] — the accent shading from its
/// dark end to the full accent, or the emphasis end to end where the palette names one — or no
/// colour at all when the theme is monochrome.
pub(super) fn bar_shade_style(theme: &Theme, i: usize, width: usize) -> Style {
    match theme.monochrome() {
        true => Style::default(),
        false => Style::default().fg(theme.bar_shade(i as f64 / width as f64)),
    }
}

/// The label column a row's value starts after, cut to the column with `…` when the label is
/// wider than it.
pub fn label(text: &str, field: usize) -> Span<'static> {
    let room = field.saturating_sub(1);
    let text = cut_to_cells(text, room);
    let pad = " ".repeat(room.saturating_sub(cells(&text)));
    Span::raw(format!("{text}{pad} "))
}

/// One row with no bar: the label and whatever word the reading carries.
pub(super) fn plain_row(
    control: &Control,
    reading: Option<&Reading>,
    columns: Columns,
) -> Line<'static> {
    Line::from(vec![
        label(&control.label, columns.field),
        Span::raw(word(reading)),
    ])
}

/// The spaces between a bar and the `value` beside it: [`VALUE_GAP`] of them, and fewer on a
/// row too narrow for a bar of [`BAR_MIN`] cells and its value both.
pub(super) fn gap(value: &str, columns: Columns) -> String {
    let used = columns.field + columns.bar + cells(value);
    " ".repeat(VALUE_GAP.min(columns.room.saturating_sub(used)))
}

/// The word a reading shows, or [`MISSING`] until one arrives.
fn word(reading: Option<&Reading>) -> String {
    reading.map_or_else(|| MISSING.to_string(), |r| r.value.display.clone())
}

/// How many of `width` cells a bar drawn for `raw` over `range` fills.
fn fill(raw: i64, (min, max): (i64, i64), width: usize) -> usize {
    let span = max.saturating_sub(min);
    if span <= 0 {
        return 0;
    }
    let reached = raw.clamp(min, max).saturating_sub(min);
    let cells = reached.saturating_mul(i64::try_from(width).unwrap_or(0)) / span;
    usize::try_from(cells).unwrap_or(0).min(width)
}

/// Which of `width` cells the marker for `raw` over `range` sits on.
fn position(raw: i64, range: (i64, i64), width: usize) -> usize {
    fill(raw, range, width.saturating_sub(1))
}

#[cfg(test)]
mod tests;
