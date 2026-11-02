// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Tone pane: the tone control's mode, its loudness strength and a row per band, and
//! the mode line and band readings the tone-control screen draws the same way.

use ratatui::text::{Line, Span};
use svolder_profiles::convert::{frequency_text, gain_text};
use svolder_profiles::{EqMode, EqParam, EqSpec};
use svolder_proto::FunctionId;

use crate::tui::app::{App, LOUDNESS_MAX, ToneRow};
use crate::tui::boxes::presets;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::{self, CURSOR_WIDTH, LOUDNESS_WIDTH, MISSING};

use super::{raw_of, row_style};

/// Columns the Tone pane's own label column takes.
const TONE_FIELD: usize = 10;
/// Columns the word "Loudness" takes where it is written in a header rather than in a label
/// column of its own: the word and the space after it.
const LOUDNESS_FIELD: usize = 9;
/// Columns from the start of the Tone pane's row to its loudness bar: the cursor and the label
/// column.
pub(super) const LOUDNESS_BAR_AT: usize = CURSOR_WIDTH + TONE_FIELD;
/// Columns the loudness's value takes after its bar at the most: a space and three digits.
const LOUDNESS_VALUE: usize = 4;

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
}

/// The Tone pane: the mode the tone control is in, how strong its loudness curve is, and then a
/// row per band — every one of them a row the pane's cursor reaches and the arrows change.
///
/// `max_width` is the pane's own interior width, its border and padding already taken off, so a
/// focused row's highlight never runs past it. The loudness bar is drawn `bar` cells across, the
/// width of the Controls pane's bars, as far as the row has room for it and its value.
pub(super) fn tone_lines(
    app: &App,
    eq: &EqSpec,
    theme: &Theme,
    max_width: usize,
    bar: usize,
) -> TonePane {
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
    let loudness_bar = bar.min(room).max(1);
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
    }
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
