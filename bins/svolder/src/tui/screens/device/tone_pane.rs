// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Tone pane: the tone control's mode, its loudness strength and a row per band, and
//! the mode line and band readings the tone-control screen draws the same way.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use svolder_profiles::convert::{frequency_text, gain_text};
use svolder_profiles::{EqMode, EqParam, EqSpec};
use svolder_proto::FunctionId;

use crate::tui::app::{App, LOUDNESS_MAX, Space, ToneRow, Xf, crossfeed, speakers};
use crate::tui::boxes::presets;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::{self, CURSOR_WIDTH, LOUDNESS_WIDTH, MISSING};

use super::{raw_of, row_style};
use crate::tui::screens::switch;
use crate::tui::screens::tone::computer_loudness as ld_tone;

/// Columns the Tone pane's own label column takes.
const TONE_FIELD: usize = 10;
/// Cells a sub-row's label is indented inside the label column: the computer side's Preset under
/// Mode, and Model and Preset under the Crossfeed row, so each reads as part of the row above it.
const SUB_INDENT: usize = 2;
/// Columns the word "Loudness" takes where it is written in a header rather than in a label
/// column of its own: the word and the space after it.
const LOUDNESS_FIELD: usize = 9;
/// Columns from the start of the Tone pane's row to its loudness bar: the cursor and the label
/// column.
pub(super) const LOUDNESS_BAR_AT: usize = CURSOR_WIDTH + TONE_FIELD;
/// Columns the loudness's value takes after its bar at the most: a space and three digits.
const LOUDNESS_VALUE: usize = 4;

/// The computer's point table's column titles.
const POINT_TITLES: &str = " #  Type  Freq     Gain     Q";
/// The band table's column titles.
const BAND_TITLES: &str = " Band   Freq      Gain      Q";
/// What a band table says under its rows while the tone control is in a mode its bands do not
/// apply in: Flat, the device's bypass, and Loudness, the device's own contour.
pub(in crate::tui::screens) const BANDS_APPLY_IN_EQ: &str = "bands apply in EQ mode";

/// The Tone pane's lines, and where among them the pieces a click or an effect reaches
/// landed.
pub(super) struct TonePane {
    /// Every line the pane draws, in order.
    pub(super) lines: Vec<Line<'static>>,
    /// Where the Mode row's `‹` and `›` sit in its own line, each one cell, for the click that
    /// steps the select rather than toggling the row.
    pub(super) mode_arrows: (usize, usize),
    /// The interior line each band's row landed on, in band order.
    pub(super) bands: Vec<u16>,
    /// Where each band's frequency, gain and Q sit in its row, each as a first cell and a width
    /// from the row's own start, cursor included, in band order.
    pub(super) fields: Vec<[(usize, usize); 3]>,
    /// Cells the Loudness row's bar is drawn across, from [`LOUDNESS_BAR_AT`].
    pub(super) loudness_bar: usize,
    /// The point the first of `bands` stands for: the table scrolls to keep the focused point
    /// in view where the pane is short.
    pub(super) bands_from: usize,
    /// The Crossfeed row at the pane's bottom, on the computer side with crossfeed.
    pub(super) crossfeed: Option<CrossfeedRow>,
    /// The Loudness row over it, in the chain's order, on the computer side with loudness.
    pub(super) loudness: Option<LoudnessRow>,
}

/// A line under the Crossfeed row, and which Model or Preset row it is with where its `‹` and `›`
/// sit, where it is one.
type Detail = (Line<'static>, Option<(Xf, (usize, usize))>);

/// Where the Loudness row sits in the Tone pane.
pub(super) struct LoudnessRow {
    /// The interior line it landed on.
    pub(super) line: u16,
    /// The dim lines under it, each a click on the row.
    pub(super) under: u16,
    /// The places in it a click lands on, as a first cell and a width from the line's start,
    /// cursor included, each with its hit.
    pub(super) spots: Vec<(usize, usize, crate::tui::mouse::Hit)>,
}

/// Where the Crossfeed row sits in the Tone pane.
pub(super) struct CrossfeedRow {
    /// The interior line it landed on.
    pub(super) line: u16,
    /// Where its `‹` and `›` sit in the line, cursor included.
    pub(super) arrows: (usize, usize),
    /// The Model and Preset rows under it while crossfeed plays, and where their `‹` and `›`
    /// sit: the lines right under `line`, in order.
    pub(super) picks: Vec<(Xf, (usize, usize))>,
}

/// The Tone pane: the mode the tone control is in, how strong its loudness curve is, and then a
/// row per band — every one of them a row the pane's cursor reaches and the arrows change.
///
/// `max_width` is the pane's own interior width, its border and padding already taken off, so a
/// focused row's highlight never runs past it. The loudness bar is drawn across all the row has
/// room for besides its value.
#[cfg(test)]
pub(super) fn tone_lines(app: &App, eq: &EqSpec, theme: &Theme, max_width: usize) -> TonePane {
    tone_lines_in(app, eq, theme, (max_width, usize::MAX))
}

/// `tone_lines` for a pane whose interior is `(max_width, max_height)` cells: the computer
/// side keeps its Crossfeed row at the bottom of the pane, under a table that scrolls to leave
/// it room.
pub(super) fn tone_lines_in(
    app: &App,
    eq: &EqSpec,
    theme: &Theme,
    (max_width, max_height): (usize, usize),
) -> TonePane {
    #[cfg(test)]
    crate::tui::test_support::work::tick("tone_lines");
    if app.on_computer() {
        return computer_lines(app, theme, max_width, max_height);
    }
    let mode_raw = mode_spans(app, eq, theme, TONE_FIELD);
    let mode_width: usize = mode_raw.iter().map(Span::width).sum();
    let mode = tone_row(
        mode_raw,
        app.tone.row == ToneRow::Mode,
        app,
        theme,
        max_width,
    );
    let focused = app.tone.row == ToneRow::Loudness;
    let room = max_width.saturating_sub(LOUDNESS_BAR_AT + LOUDNESS_VALUE);
    let loudness_bar = room.max(1);
    let spans = loudness_spans(app, eq, theme, TONE_FIELD, focused, loudness_bar);
    let loudness = tone_row(spans, focused, app, theme, max_width);
    // A blank line sets the band table off from the rows above it.
    let mut lines = vec![
        mode,
        loudness,
        preset_line(app, theme, max_width),
        Line::default(),
        Line::from(Span::styled(BAND_TITLES, theme.style(Role::Dim))),
    ];
    let mut bands = Vec::with_capacity(eq.bands.len());
    let mut fields = Vec::with_capacity(eq.bands.len());
    for band in 0..eq.bands.len() {
        bands.push(u16::try_from(lines.len()).unwrap_or(u16::MAX));
        let (line, at) = band_line(app, eq, band, theme, max_width);
        lines.push(line);
        fields.push(at);
    }
    // The note sits right under the bands, so the blank above the table costs no line.
    if !bands_apply(app) {
        lines.push(Line::from(Span::styled(
            format!(" {BANDS_APPLY_IN_EQ}"),
            theme.style(Role::Dim),
        )));
    }
    // The cursor takes three cells in front of every row of this pane, so a click on the Mode
    // row's arrows reaches them where they are actually drawn.
    let mode_arrows = (CURSOR_WIDTH + TONE_FIELD, CURSOR_WIDTH + mode_width - 1);
    TonePane {
        lines,
        mode_arrows,
        bands,
        fields,
        loudness_bar,
        bands_from: 0,
        crossfeed: None,
        loudness: None,
    }
}

/// The Tone pane on the computer side: the mode as `‹ Off | EQ ›`, the preset the points came
/// from, and a row per point.
fn computer_lines(app: &App, theme: &Theme, max_width: usize, max_height: usize) -> TonePane {
    let mode_focused = app.tone.row == ToneRow::Mode;
    let (mode_raw, mode_width) =
        computer_mode_spans(app, theme, TONE_FIELD, Style::default(), mode_focused);
    // The row carries the cursor but no box of its own: the choice draws its focus as Target's
    // does, on the label and the side in force.
    let mode = row_style(
        widgets::rows::with_cursor(Line::from(mode_raw), mode_focused, theme),
        false,
        !app.session.connected,
        theme,
        max_width,
    );
    let points = app.computer_points();
    let folded = computer_folds(app, (max_width, max_height));
    let preset = app
        .computer_eq()
        .and_then(|eq| eq.preset.clone())
        .unwrap_or_default();
    // A folded table says what it holds in the Preset line.
    let preset = match (folded, points.len()) {
        (false, _) => preset,
        (true, 0) => custom_or(preset),
        (true, 1) => format!("{} \u{b7} 1 point", custom_or(preset)),
        (true, n) => format!("{} \u{b7} {n} points", custom_or(preset)),
    };
    let room = max_width.saturating_sub(CURSOR_WIDTH + TONE_FIELD);
    let preset = tone_row(
        vec![
            sub_label("Preset"),
            Span::raw(widgets::text::cut_to_cells(&preset, room)),
        ],
        false,
        app,
        theme,
        max_width,
    );
    let mut lines = vec![mode, preset, Line::default()];
    if !folded {
        lines.push(Line::from(Span::styled(
            POINT_TITLES,
            theme.style(Role::Dim),
        )));
    }
    let mut bands = Vec::new();
    // The Crossfeed row (and its numbers under it, where the pane has the room) sit at the
    // bottom: the table gives up rows to them, scrolled to keep the focused point in view.
    let xf = app.crossfeed();
    let head = lines.len();
    // The numbers under the row: the crossfeed's one line, or the two lines of the space choice
    // (what it is and how it sounds).
    let details: Vec<Detail> = match (xf, app.space_now()) {
        (None, _) | (Some(_), Space::Off) => Vec::new(),
        (Some(view), Space::Crossfeed) => xf_pick_lines(app, view, theme, max_width),
        (Some(_), Space::Speakers) => space_details(app, theme, max_width)
            .into_iter()
            .map(|line| (line, None))
            .collect(),
    };
    // The Loudness row sits over the Crossfeed row (the chain's order: the points, loudness,
    // crossfeed). Its dim readout and the hint under it never take a point's row: they show only
    // where every point is in view (or the table is folded away) and the row fits.
    let ld = app.loudness_view().filter(|_| app.ld_shown());
    let ld_base = usize::from(ld.is_some());
    // The table's own rows: a point and the gap row under it, which a folded table has not.
    let table = usize::from(!folded);
    let (shown, gap) = match (xf, max_height) {
        (None, _) => (0, false),
        (Some(_), usize::MAX) => (details.len(), !folded),
        (Some(_), h) => {
            let room = h >= head + table + table + 1 + details.len() + ld_base;
            (if room { details.len() } else { 0 }, room && !folded)
        }
    };
    let xf_rows = xf.map_or(0, |_| 1 + shown);
    let reserved = match (xf, ld) {
        (None, None) => 0,
        _ => ld_base + xf_rows + usize::from(gap),
    };
    let visible = match max_height {
        _ if folded => 0,
        usize::MAX => points.len(),
        h => h.saturating_sub(head + reserved).min(points.len()),
    };
    let first = match max_height {
        usize::MAX => 0,
        _ => widgets::scroll::scroll_start(points.len(), visible, app.focused_point()),
    };
    let spare = match max_height {
        usize::MAX => usize::MAX / 2,
        h => h.saturating_sub(head + reserved + visible),
    };
    let in_view = folded || visible == points.len();
    let ld_dim = ld.is_some() && in_view && spare >= 1;
    let ld_hint = ld_dim && dac_hint_wanted(app, max_width) && spare >= 2;
    let tail = ld_base + usize::from(ld_dim) + usize::from(ld_hint) + xf_rows;
    for (at, point) in points.iter().enumerate().skip(first).take(visible) {
        bands.push(u16::try_from(lines.len()).unwrap_or(u16::MAX));
        let text = format!(
            "{}  {:<4} {:>5} Hz {:>5} dB {:>4}",
            at + 1,
            point.kind,
            frequency_text(point.f),
            gain_text(point.g),
            format!("{:.1}", point.q)
        );
        let focused = app.tone.row == ToneRow::Point(at);
        let on = app.computer_eq().is_some_and(|eq| eq.on);
        let line = widgets::rows::with_cursor(Line::raw(text), focused, theme);
        lines.push(row_style(
            line,
            focused,
            !app.session.connected || (!on && !focused),
            theme,
            max_width,
        ));
    }
    // Under the table, at the pane's bottom where its height is known.
    if tail > 0 && !folded {
        let target = match max_height {
            usize::MAX => lines.len() + usize::from(gap),
            h => h.saturating_sub(tail),
        };
        while lines.len() < target {
            lines.push(Line::default());
        }
    }
    let loudness = ld.map(|view| {
        let at = u16::try_from(lines.len()).unwrap_or(u16::MAX);
        let row = ld_tone::strength_row(app, view, theme, max_width);
        lines.push(row.line);
        let mut under = 0;
        if ld_dim {
            lines.push(ld_tone::readout_row(app, view, theme, max_width).line);
            under += 1;
        }
        if ld_hint {
            lines.push(ld_tone::hint_row(app, theme, max_width).line);
            under += 1;
        }
        LoudnessRow {
            line: at,
            under,
            spots: row.spots,
        }
    });
    let crossfeed = xf.map(|view| {
        let (line, arrows) = match app.space_shown() {
            true => space_line(app, theme, max_width),
            false => crossfeed_line(app, view, theme, max_width),
        };
        let at = u16::try_from(lines.len()).unwrap_or(u16::MAX);
        lines.push(line);
        let mut picks = Vec::new();
        for (line, pick) in details.into_iter().take(shown) {
            lines.push(line);
            picks.extend(pick);
        }
        CrossfeedRow {
            line: at,
            arrows,
            picks,
        }
    });
    TonePane {
        lines,
        mode_arrows: (CURSOR_WIDTH + TONE_FIELD, CURSOR_WIDTH + mode_width - 1),
        fields: vec![[(0, 0); 3]; bands.len()],
        bands,
        loudness_bar: 0,
        bands_from: first,
        crossfeed,
        loudness,
    }
}

/// A sub-row's label: indented under the row it belongs to, in the pane's label column.
fn sub_label(text: &str) -> Span<'static> {
    widgets::rows::label(&format!("{}{text}", " ".repeat(SUB_INDENT)), TONE_FIELD)
}

/// The preset's name, `custom` where the view names none.
fn custom_or(name: String) -> String {
    match name.is_empty() {
        true => "custom".to_string(),
        false => name,
    }
}

/// Whether the line saying the device's own loudness plays too wants a row of its own: the
/// computer's loudness is shown, the device's is on, and the readout does not already say so.
fn dac_hint_wanted(app: &App, max_width: usize) -> bool {
    let Some(view) = app.loudness_view().filter(|_| app.ld_shown()) else {
        return false;
    };
    let said_inline = crate::tui::app::computer_loudness::readout(
        view,
        true,
        false,
        max_width.saturating_sub(CURSOR_WIDTH),
    )
    .contains(crate::tui::app::computer_loudness::DAC_HINT);
    app.dac_loudness_on() && !said_inline
}

/// How many lines sit under the Crossfeed (or Space) row on the computer side.
fn detail_rows(app: &App) -> usize {
    match (app.crossfeed(), app.space_now()) {
        (None, _) | (Some(_), Space::Off) => 0,
        (Some(_), Space::Crossfeed) => 2,
        (Some(_), Space::Speakers) => 2 * usize::from(app.speakers().is_some()),
    }
}

/// Whether the computer side's points table is folded away in a pane whose interior is `(width,
/// height)`: the EQ is off, there is something under the table, and the pane is too short for
/// every point and every row under them (Loudness, its readout and hint, Space with its sub-rows
/// and the blank row over them). With the EQ on the table never folds; it scrolls.
pub(crate) fn computer_folds(app: &App, (width, height): (usize, usize)) -> bool {
    let Some(eq) = app.computer_eq() else {
        return false;
    };
    if eq.on || height == usize::MAX {
        return false;
    }
    let ld = app.loudness_view().filter(|_| app.ld_shown()).is_some();
    let xf = app.crossfeed().is_some();
    if !ld && !xf {
        return false;
    }
    let under = match ld {
        true => 2 + usize::from(dac_hint_wanted(app, width)),
        false => 0,
    } + match xf {
        // the Crossfeed (or Space) row, its sub-rows, and the blank row over what is under, which
        // only the layout with a Crossfeed row draws
        true => 1 + detail_rows(app) + 1,
        false => 0,
    };
    // the mode, the preset, the blank, the titles, the points and what is under them
    height < 4 + app.computer_points().len() + under
}

/// The Crossfeed row of the pane: `Crossfeed ‹ on › Bauer · Chu Moy`, and where its `‹` and `›`
/// sit in the line.
fn crossfeed_line(
    app: &App,
    view: &svolder_host::eq::crossfeed::CrossfeedView,
    theme: &Theme,
    max_width: usize,
) -> (Line<'static>, (usize, usize)) {
    let focused = app.tone.row == ToneRow::Crossfeed(Xf::Space);
    let (head, lit, other) = switch::styles(theme, Style::default(), focused);
    let arrow = theme.style(Role::ModeArrow);
    let word = if view.on { "on" } else { "off" };
    let spans = vec![
        Span::styled("Crossfeed", head),
        Span::raw(" ".repeat(TONE_FIELD - "Crossfeed".len())),
        Span::styled("\u{2039} ", arrow),
        Span::styled(word, if view.on || focused { lit } else { other }),
        Span::styled(" \u{203a}", arrow),
    ];
    let arrows = (
        CURSOR_WIDTH + TONE_FIELD,
        CURSOR_WIDTH + TONE_FIELD + 2 + word.len() + 1,
    );
    // the cursor marks the row; the focus is the label and the word, as the Target switch has it
    let line = widgets::rows::with_cursor(Line::from(spans), focused, theme);
    let line = row_style(line, false, !app.session.connected, theme, max_width);
    (line, arrows)
}

/// The Space row of the pane: `Space ‹ Virtual speakers ›`, and where its `‹` and `›` sit.
fn space_line(app: &App, theme: &Theme, max_width: usize) -> (Line<'static>, (usize, usize)) {
    let focused = app.tone.row == ToneRow::Crossfeed(Xf::Space);
    let (head, lit, _) = switch::styles(theme, Style::default(), focused);
    let arrow = theme.style(Role::ModeArrow);
    // The arrows move a highlight that `Enter` plays; what plays stays the word until then.
    let shown = match focused {
        true => app.plays_highlight(),
        false => app.space_now(),
    };
    let word = shown.word();
    let mut spans = vec![
        Span::styled("Space", head),
        Span::raw(" ".repeat(TONE_FIELD - "Space".len())),
        Span::styled("\u{2039} ", arrow),
        Span::styled(word, lit),
        Span::styled(" \u{203a}", arrow),
    ];
    if shown != app.space_now() {
        spans.push(Span::styled(" Enter plays", theme.style(Role::Dim)));
    }
    let arrows = (
        CURSOR_WIDTH + TONE_FIELD,
        CURSOR_WIDTH + TONE_FIELD + 2 + widgets::text::cells(word) + 1,
    );
    let line = widgets::rows::with_cursor(Line::from(spans), focused, theme);
    let line = row_style(line, false, !app.session.connected, theme, max_width);
    (line, arrows)
}

/// The dim lines under the Space row: for the virtual speakers the set with its choice and term,
/// then how it sounds; for the crossfeed its model and preset, then its numbers; none for off.
fn space_details(app: &App, theme: &Theme, max_width: usize) -> Vec<Line<'static>> {
    let dim = |text: String| {
        let room = max_width.saturating_sub(CURSOR_WIDTH + SUB_INDENT + 1);
        Line::from(Span::styled(
            format!(
                "{}{}",
                " ".repeat(CURSOR_WIDTH + SUB_INDENT),
                widgets::text::cut_to_cells(&text, room)
            ),
            theme.style(Role::Dim),
        ))
    };
    match (app.space_now(), app.speakers(), app.crossfeed()) {
        (Space::Speakers, Some(view), _) => vec![
            dim(speakers::set_words(view)),
            dim(format!(
                "{} \u{b7} {}",
                speakers::level_short(view),
                speakers::length_text(view)
            )),
        ],
        (Space::Crossfeed, _, Some(view)) => vec![
            dim(crossfeed::words_of(view)),
            dim(crossfeed::summary_text(view)),
        ],
        _ => Vec::new(),
    }
}

/// The Model and Preset rows under the Crossfeed row while crossfeed plays, each indented two
/// cells inside the label field (`  Model     ‹ Bauer ›`, `  Preset    ‹ Chu Moy ›`), each with where its `‹` and `›` sit.
fn xf_pick_lines(
    app: &App,
    view: &svolder_host::eq::crossfeed::CrossfeedView,
    theme: &Theme,
    max_width: usize,
) -> Vec<Detail> {
    let model = view
        .models
        .iter()
        .find(|m| m.word == view.model)
        .map_or(view.model.as_str(), |m| m.name.as_str());
    let preset = view.preset.as_deref().unwrap_or("your own");
    // A model with the one preset says so, as the panel does.
    let preset = match (
        view.preset.is_some(),
        app.xf_model().map_or(0, |m| m.presets.len()),
    ) {
        (true, 1) => format!("{preset} (the only one)"),
        _ => preset.to_string(),
    };
    [(Xf::Model, model.to_string()), (Xf::Preset, preset)]
        .into_iter()
        .map(|(row, value)| {
            let focused = app.tone.row == ToneRow::Crossfeed(row);
            let (head, lit, _) = switch::styles(theme, Style::default(), focused);
            let arrow = theme.style(Role::ModeArrow);
            let room = max_width.saturating_sub(CURSOR_WIDTH + TONE_FIELD + 4);
            let value = widgets::text::cut_to_cells(&value, room);
            let width = widgets::text::cells(&value);
            let spans = vec![
                Span::styled(format!("{}{}", " ".repeat(SUB_INDENT), row.label()), head),
                Span::raw(" ".repeat(TONE_FIELD.saturating_sub(SUB_INDENT + row.label().len()))),
                Span::styled("\u{2039} ", arrow),
                Span::styled(value, if focused { lit } else { Style::default() }),
                Span::styled(" \u{203a}", arrow),
            ];
            let arrows = (
                CURSOR_WIDTH + TONE_FIELD,
                CURSOR_WIDTH + TONE_FIELD + 2 + width + 1,
            );
            let line = widgets::rows::with_cursor(Line::from(spans), focused, theme);
            let line = row_style(line, false, !app.session.connected, theme, max_width);
            (line, Some((row, arrows)))
        })
        .collect()
}

/// The mode's label.
const MODE_LABEL: &str = "Mode";

/// The computer's mode as a two-way choice, `‹ Off | EQ ›`, drawn as the `Target` switch is
/// ([`switch::styles`], [`switch::choice`]): the one in force and the label stand out, in the
/// selected role's box while the keys are on the row (`focused`), the other side dim. The label
/// is in the style `head` and a column `field` cells wide.
fn computer_mode_spans(
    app: &App,
    theme: &Theme,
    field: usize,
    head: Style,
    focused: bool,
) -> (Vec<Span<'static>>, usize) {
    let on = app.computer_eq().is_some_and(|eq| eq.on);
    let (head, lit, off) = switch::styles(theme, head, focused);
    // only the word takes the label's style: the blanks up to the arrows stay plain
    let pad = " ".repeat(field.saturating_sub(widgets::text::cells(MODE_LABEL)));
    let mut spans = vec![Span::styled(MODE_LABEL, head), Span::raw(pad)];
    spans.extend(switch::choice(
        theme,
        ("Off".to_string(), "EQ".to_string()),
        on,
        (lit, off),
    ));
    // where the choice ends, before anything said after it
    let choice_width = spans.iter().map(Span::width).sum();
    // What the state file says, where the chain could not be looked at: never drawn as verified.
    if app.computer_eq().is_some_and(|eq| eq.running_unknown) {
        spans.push(Span::styled(" not verified", off));
    }
    (spans, choice_width)
}

/// The mode line of the computer side: no loudness.
fn computer_mode_line(app: &App, theme: &Theme, field: usize) -> (Line<'static>, ModeHalves) {
    let focused = app.tone.row == ToneRow::Mode;
    let (raw, width) = computer_mode_spans(app, theme, field, theme.style(Role::Header), focused);
    let mut spans = vec![Span::raw(" ")];
    spans.extend(raw);
    let end: usize = spans.iter().map(Span::width).sum();
    let line = Line::from(spans);
    let halves = ModeHalves {
        mode: (1, end - 1),
        loudness: (end, 0),
        loudness_bar: end,
        mode_arrows: (1 + field, width),
    };
    (line, halves)
}

/// The Tone pane's Preset line: the preset the tone control is on, as the tone-control screen's
/// header names it — `NAME`, `NAME*` once the curve has moved away from the one loaded last,
/// `custom` for a curve no preset matches, `loading NAME` while a recall is still being confirmed
/// — in the label column the rows above it use, the name cut short with `…` to the pane's width.
/// No cursor reaches it; it dims with the rows while the link is down.
fn preset_line(app: &App, theme: &Theme, max_width: usize) -> Line<'static> {
    let Some(standing) = presets::standing(app) else {
        return Line::default();
    };
    let (lead, name, mark) = standing.after_label();
    let room = max_width.saturating_sub(CURSOR_WIDTH + TONE_FIELD);
    let fixed = widgets::text::cells(lead) + widgets::text::cells(mark);
    let name_room = room.saturating_sub(fixed);
    // With no room for even a cut name, the lead and the mark go too: `loading ` or a bare `*`
    // would say nothing.
    let text = match widgets::text::cells(name) <= name_room {
        true => format!("{lead}{name}{mark}"),
        false if name_room >= 2 => {
            format!(
                "{lead}{}{mark}",
                widgets::text::cut_to_cells(name, name_room)
            )
        }
        false => String::new(),
    };
    let spans = vec![widgets::rows::label("Preset", TONE_FIELD), Span::raw(text)];
    tone_row(spans, false, app, theme, max_width)
}

/// One row of the Tone pane: the cursor in front of it, and the pane's own focus and staleness
/// over it, exactly as a band's row is drawn.
fn tone_row(
    spans: Vec<Span<'static>>,
    focused: bool,
    app: &App,
    theme: &Theme,
    max_width: usize,
) -> Line<'static> {
    let line = widgets::rows::with_cursor(Line::from(spans), focused, theme);
    row_style(line, focused, !app.session.connected, theme, max_width)
}

/// The functions one band's row shows: its frequency, its gain and its Q.
///
/// A row is reported under all three, so whichever of them the device confirms as changed,
/// the animation layer finds the one row that says so.
pub(in crate::tui::screens) fn band_ids(
    eq: &EqSpec,
    band: usize,
) -> impl Iterator<Item = FunctionId> {
    [EqParam::Frequency, EqParam::Gain, EqParam::Q]
        .into_iter()
        .filter_map(move |param| eq.band_id(band, param))
}

/// The tone control's mode, and how strong its loudness curve is set, with the mode's own
/// label drawn in a column `field` wide, and where in the returned line the loudness half
/// starts and the mode half ends. The returned line never runs past `max_width`, cell for
/// cell — the budget the header keeps for it once its own text and the mouse indicator have
/// taken theirs.
///
/// The tone-control screen puts the same line in its header, in a column of its own.
pub(in crate::tui::screens) fn mode_line(
    app: &App,
    eq: &EqSpec,
    theme: &Theme,
    field: usize,
    max_width: usize,
) -> (Line<'static>, ModeHalves) {
    if app.on_computer() {
        return computer_mode_line(app, theme, field);
    }
    let mode_raw = mode_spans(app, eq, theme, field);
    let mode_focused = app.tone.row == ToneRow::Mode;
    let loudness_focused = app.tone.row == ToneRow::Loudness;
    let loudness_raw = loudness_spans(
        app,
        eq,
        theme,
        LOUDNESS_FIELD,
        loudness_focused,
        LOUDNESS_WIDTH,
    );
    let mode_width: usize = mode_raw.iter().map(Span::width).sum();
    let loudness_width: usize = loudness_raw.iter().map(Span::width).sum();
    // Whether the whole line, unpadded, leaves one spare cell short of its own budget — for
    // whichever half is focused, never both at once, so the one pad this line ever draws is
    // measured against the budget as a whole rather than against each half's own share of it,
    // which neither half can know in isolation before the other's natural width is found.
    let spare = usize::from(1 + mode_width + 3 + loudness_width < max_width);
    let mut spans = vec![Span::raw(" ")];
    spans.extend(standing_out(
        mode_raw,
        mode_focused,
        theme,
        mode_width + spare,
    ));
    let mode_end: usize = spans.iter().map(Span::width).sum();
    spans.push(Span::raw("   "));
    let before: usize = spans.iter().map(Span::width).sum();
    spans.extend(standing_out(
        loudness_raw,
        loudness_focused,
        theme,
        loudness_width + spare,
    ));
    let line = Line::from(spans);
    let halves = ModeHalves {
        mode: (1, mode_end - 1),
        loudness: (before, line.width() - before),
        loudness_bar: before + LOUDNESS_FIELD,
        mode_arrows: (1 + field, 1 + mode_width - 1),
    };
    (line, halves)
}

/// Where the two halves of a [`mode_line`] lie in it, each as a first cell and a width: the
/// mode's, after the one blank the line opens with, and the loudness's.
pub(in crate::tui::screens) struct ModeHalves {
    /// The mode's half.
    pub(in crate::tui::screens) mode: (usize, usize),
    /// The loudness's half.
    pub(in crate::tui::screens) loudness: (usize, usize),
    /// The column of the line the loudness bar starts at, past the word.
    pub(in crate::tui::screens) loudness_bar: usize,
    /// Where the Mode select's `‹` and `›` sit in the line, each one cell, for the click that
    /// steps it like every select's arrows rather than toggling the half the way
    /// clicking anywhere else on it does.
    pub(in crate::tui::screens) mode_arrows: (usize, usize),
}

/// `spans` drawn as the focused row where `focused`: how the tone-control screen's header says
/// which of the two the arrows are on, since it has no cursor column to put a `▶` in, so the
/// half being acted on is the half that stands out.
///
/// The selected role goes under each span's own style, as a row's does under its spans, so a
/// bar that keeps its own colours on the focused row keeps them here too. A focused half is
/// extended by one blank cell past its last character, in the same style, so the highlight
/// doesn't stop flush against it, short of its own `max_width` columns ([`mode_line`] gives each
/// half whatever is left of the header's budget once its own text and the mouse indicator have
/// taken theirs, so a long product name or mode label never pushes the header's last column off
/// under the pad's own weight).
fn standing_out(
    spans: Vec<Span<'static>>,
    focused: bool,
    theme: &Theme,
    max_width: usize,
) -> Vec<Span<'static>> {
    if !focused {
        return spans;
    }
    let selected = theme.style(Role::Selected);
    let mut spans: Vec<Span<'static>> = spans
        .into_iter()
        .map(|span| {
            let style = selected.patch(span.style);
            span.style(style)
        })
        .collect();
    widgets::text::pad_selected(&mut spans, selected, max_width);
    spans
}

/// The tone control's mode as a select reads: its name between the arrows that step it, in a
/// label column `field` cells wide. The arrows take their own role, the accent the cursor mark
/// is drawn in, whether or not the row is focused, and no colour at all in monochrome.
fn mode_spans(app: &App, eq: &EqSpec, theme: &Theme, field: usize) -> Vec<Span<'static>> {
    let mode = raw_of(app, eq.mode_id())
        .and_then(|raw| u8::try_from(raw).ok())
        .and_then(EqMode::from_raw);
    let name = mode.map_or_else(|| MISSING.to_string(), |mode| mode.display().to_string());
    let arrow = theme.style(Role::ModeArrow);
    vec![
        widgets::rows::label("Mode", field),
        Span::styled("‹ ", arrow),
        Span::styled(name, theme.style(Role::SelectName)),
        Span::styled(" ›", arrow),
    ]
}

/// How strong the loudness curve is, as a level reads: its bar, `width` cells across, and the
/// raw value behind it in a label column `field` cells wide, and nothing at all until there is
/// a reading to draw it from. On the focused row (`selected`) the bar keeps its own colours, as every bar does. A
/// click on the row this draws toggles the loudness mode ([`Hit::Loudness`]) except on the bar,
/// where a press sets the strength the cell stands for and a drag follows it
/// ([`Hit::LoudnessBar`]); the wheel over the bar steps the strength.
///
/// [`Hit::Loudness`]: crate::tui::mouse::Hit::Loudness
/// [`Hit::LoudnessBar`]: crate::tui::mouse::Hit::LoudnessBar
fn loudness_spans(
    app: &App,
    eq: &EqSpec,
    theme: &Theme,
    field: usize,
    selected: bool,
    width: usize,
) -> Vec<Span<'static>> {
    let mut spans = vec![widgets::rows::label("Loudness", field)];
    match raw_of(app, eq.loudness_id()) {
        Some(raw) => {
            let eased = app
                .dragged_raw(eq.loudness_id())
                .or_else(|| app.eased_raw(eq.loudness_id(), app.now))
                .unwrap_or(raw);
            spans.extend(widgets::rows::bar_spans(
                theme,
                eased,
                (0, LOUDNESS_MAX),
                width,
                selected,
                |_| theme.style(Role::Bar),
            ));
            spans.push(Span::raw(format!(" {raw}")));
        }
        None => spans.push(Span::raw(MISSING)),
    }
    spans
}

/// What one band of the tone control reads now.
pub(in crate::tui::screens) struct BandValues {
    /// Its centre frequency in Hz.
    pub hz: Option<f64>,
    /// Its gain in dB.
    pub gain: Option<f64>,
    /// How narrow it is.
    pub q: Option<f64>,
    /// The raw values those were converted from, in frequency, gain and Q order.
    pub raws: [Option<u8>; 3],
}

/// One band's raw frequency, gain and Q, and the units the profile converts them to.
///
/// The tone-control screen reads its bands the same way, and its curve is drawn from these.
pub(in crate::tui::screens) fn band_values(app: &App, eq: &EqSpec, band: usize) -> BandValues {
    let raw = |param: EqParam| -> Option<u8> {
        let id = eq.band_id(band, param)?;
        u8::try_from(raw_of(app, id)?).ok()
    };
    let raws = [raw(EqParam::Frequency), raw(EqParam::Gain), raw(EqParam::Q)];
    BandValues {
        hz: raws[0].and_then(|raw| eq.frequency(band, raw)),
        gain: raws[1].map(|raw| eq.gain_db(raw)),
        q: raws[2].map(|raw| eq.q(raw)),
        raws,
    }
}

/// One band's frequency, gain and Q, in the units the profile converts them to, with where each
/// of the three sits in the row, its unit with it, as a first cell and a width from the row's
/// own start — the cursor's cells in front of the text counted — for the clicks on them.
fn band_line(
    app: &App,
    eq: &EqSpec,
    band: usize,
    theme: &Theme,
    max_width: usize,
) -> (Line<'static>, [(usize, usize); 3]) {
    let values = band_values(app, eq, band);
    let number = format!("{}  ", band + 1);
    let frequency = format!(
        "{:>5} Hz",
        values
            .hz
            .map_or_else(|| MISSING.to_string(), frequency_text)
    );
    let gain = format!(
        "{:>5} dB",
        values.gain.map_or_else(|| MISSING.to_string(), gain_text)
    );
    let q = values
        .q
        .map_or_else(|| MISSING.to_string(), |q| format!("{q:.1}"));
    let gap = "   ";
    let cells = |text: &str| text.chars().count();
    let at_frequency = CURSOR_WIDTH + cells(&number);
    let at_gain = at_frequency + cells(&frequency) + cells(gap);
    let at_q = at_gain + cells(&gain) + cells(gap);
    let fields = [
        (at_frequency, cells(&frequency)),
        (at_gain, cells(&gain)),
        (at_q, cells(&q)),
    ];
    let text = format!("{number}{frequency}{gap}{gain}{gap}{q}");
    // The pane's cursor is on a band only while it is off the mode and loudness rows above
    // them, so exactly one row of the pane carries it.
    let focused = app.tone.row == ToneRow::Band && app.tone.band == band;
    let line = widgets::rows::with_cursor(Line::raw(text), focused, theme);
    (
        row_style(line, focused, band_dim(app, focused), theme, max_width),
        fields,
    )
}

/// Whether a band's row is drawn dim: always while the link is down, since its value then is
/// last-known rather than confirmed, and — unless the row is focused, which keeps the selected
/// style rather than fading it to grey — while the mode the tone control is in does not
/// apply the bands at all ([`bands_apply`]).
pub(in crate::tui::screens) fn band_dim(app: &App, focused: bool) -> bool {
    !app.session.connected || (!bands_apply(app) && !focused)
}

/// Whether the tone control's bands shape what the device plays: in EQ mode, and until the
/// mode has been read. Flat mode is the device's bypass and Loudness its own contour, and in
/// either a band table is drawn dimmed, saying [`BANDS_APPLY_IN_EQ`].
pub(in crate::tui::screens) fn bands_apply(app: &App) -> bool {
    !matches!(app.eq_mode(), Some(EqMode::Flat | EqMode::Loudness))
}
