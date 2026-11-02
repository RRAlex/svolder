// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone-control screen: the response the bands add up to, and the bands themselves, under a
//! header that names the preset on the device.
//!
//! The curve is the summed magnitude of the bands' peaking filters, worked out in [`response`]
//! and sampled twice per column since Braille draws two dots across a cell. Each band's centre
//! is marked on it, the focused one in its own colour; a band the device has not reported all
//! three values for is left out of the sum and shows `—` in the table below. The curve draws
//! each band where it is on its way to (a step a key or a drag has asked for, before the device
//! confirms it), and, while the edit line is open on a band, at the steps the line's fields
//! hold, though nothing is written before `Enter`; the table shows what the device confirmed.
//!
//! The graph draws what the device applies in EQ mode ([`Graph`]): the curve, coloured by
//! frequency, while a band is off 0 dB, or a muted flat line (it IS the applied curve) with
//! every band at 0 dB. In Flat mode, the device's bypass, nothing is applied, so the graph draws
//! the stored curve instead — what the bands would shape if the bypass were not there — muted
//! the same way (hue kept but desaturated, resting red to violet, or on the palette's own ramp,
//! regardless of the rainbow scroll), with their points on it rather than floating apart from
//! a flat line that isn't theirs. In Loudness mode, whose contour nothing has measured, a line
//! of text stands in for a curve. Outside EQ mode the band table is dimmed and says so under its
//! rows, except the cursor's row, which keeps its selected style; in EQ mode, while the curve
//! boosts, it says there how far down the volume has to stay to keep clear of clipping.
//!
//! - [`fit`]: what the screen adds for an imported preset: the target curve and a side pane.

use std::sync::Arc;

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType, Paragraph};
use svolder_profiles::convert::frequency_text;
use svolder_profiles::tone::response::{self, BandParams, F_MAX, F_MIN};
use svolder_profiles::{EqMode, EqParam, EqSpec};
use svolder_proto::FunctionId;

use crate::tui::app::curve_drag;
use crate::tui::app::{App, Screen, TargetView, ToneRow};
use crate::tui::boxes::edit_line::{self, EditLine};
use crate::tui::boxes::presets::{self, Standing};
use crate::tui::keys::Scene;
use crate::tui::layout_tier::{self, Tier};
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render::FrameId;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::actions::Action;
use crate::tui::widgets::{self, LOUDNESS_WIDTH, MISSING};

use super::device::{
    self, BANDS_APPLY_IN_EQ, ModeHalves, band_dim, band_ids, band_values, bands_apply, mode_line,
    row_style,
};

/// The words in the Response chart's top border.
const RESPONSE_ACTIONS: [Action; 3] = [
    Action {
        code: KeyCode::Char('f'),
        word: "flat",
    },
    Action {
        code: KeyCode::Char('p'),
        word: "presets",
    },
    Action {
        code: KeyCode::Char('r'),
        word: "refresh",
    },
];

/// The word in the Bands block's top border.
const BANDS_ACTIONS: [Action; 1] = [Action {
    code: KeyCode::Char('0'),
    word: "0 flatten",
}];

/// Columns the mode's own label takes in the header.
const MODE_FIELD: usize = 5;
/// Columns the chart keeps for its gain labels, the space after them included.
const GAIN_LABEL_WIDTH: u16 = 4;
/// Rows the Bands block takes beyond one per band: two borders and the column titles. The
/// tall tier's blank top row comes on top of these, added where the block is laid out.
const BANDS_CHROME: u16 = 3;
/// The lowest gain the curve is drawn over, in dB.
pub(crate) const GAIN_FLOOR_DB: f64 = -12.0;
/// The highest.
pub(crate) const GAIN_CEILING_DB: f64 = 6.0;
/// A blank as wide as a gain's label, the space after it included: what the chart's own y
/// labels are, so it keeps their column for [`gain_labels`] to write in.
const GAIN_BLANK: &str = "    ";
const _: () = assert!(GAIN_BLANK.len() == GAIN_LABEL_WIDTH as usize);
/// The gains the y axis names, bottom to top, each held off the axis by a space.
const GAIN_LABELS: [(f64, &str); 4] = [
    (GAIN_FLOOR_DB, "-12 "),
    (-6.0, " -6 "),
    (0.0, "  0 "),
    (GAIN_CEILING_DB, " +6 "),
];
// Every label above has to fill the column GAIN_LABEL_WIDTH reserves for it, the same way
// GAIN_BLANK does, or the chart's left edge would ripple where a label falls short.
const _: () = {
    let mut i = 0;
    while i < GAIN_LABELS.len() {
        assert!(GAIN_LABELS[i].1.len() == GAIN_LABEL_WIDTH as usize);
        i += 1;
    }
};
/// The frequencies the x axis names, and what it calls them.
const FREQUENCY_LABELS: [(f64, &str); 9] = [
    (20.0, "20"),
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1000.0, "1k"),
    (2000.0, "2k"),
    (5000.0, "5k"),
    (20000.0, "20k"),
];
/// The band table's column titles.
const BAND_TITLES: &str = "   #   Freq        Gain       Q        raw (f/g/q)";
/// What sets the preset's name apart from the product before it in the header: the two blanks
/// every header sets its state apart from the device's identity with.
const PRESET_OPEN: &str = "  ";
/// Blank cells the header keeps between the preset's name and the mode, beyond the one the mode
/// line opens with.
const PRESET_GAP: usize = 1;
/// What the graph says in Loudness mode while the DAC plays at its full volume, where the
/// device's loudness contour adds nothing.
const LOUDNESS_AT_FULL: &str = "no effect at full volume";
/// What it says at any other volume, before the strength the contour is set to.
const LOUDNESS_ACTIVE: &str = "loudness active — the boost grows as the volume goes down";
/// The words the note under the band table ends on, which a table too narrow for the whole
/// note leaves off: the rest of it still says what to do.
const LOUD_MASTERS: &str = " on loud masters";
/// Each of the edit line's fields, in the order it draws them: its letter, its unit, and the
/// cells its text is shown in, the cursor's own cell after the text among them.
const LINE_FIELDS: [(&str, &str, usize); 3] = [("f", "Hz", 7), ("g", "dB", 7), ("q", "", 5)];
/// Blank cells between one of the edit line's fields and the next, and before its keys.
const FIELD_GAP: usize = 2;
/// Blank cells the edit line keeps after its keys, before the block's right border.
const LINE_TRAIL: usize = 1;
/// The edit line's keys, on a row with room for what each of them does.
const LINE_KEYS: [(&str, &str); 4] = [
    ("Tab/⇧Tab ←→", "moves"),
    ("↑↓", "steps"),
    ("Enter", "writes"),
    ("Esc", "cancels"),
];
/// The same keys on a row without that room, as an 80-column one is: moving between the fields
/// and stepping one are what the three fields beside them already say, and `Enter` is written
/// `↵`.
const LINE_KEYS_SHORT: [(&str, &str); 3] = [("Tab ←→ ↑↓", ""), ("↵", "writes"), ("Esc", "cancels")];

/// What the Response graph draws: what the device applies, from the mode its tone control is in
/// and the curve its bands add up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Graph {
    /// EQ mode with a band off 0 dB: the summed curve, coloured as a spectrum by frequency.
    Curve,
    /// EQ mode with every band at 0 dB, a flat curve: its flat line, muted — it IS the applied
    /// curve, so a flat line is what it should draw.
    FlatCurve,
    /// Flat mode, the device's bypass: the stored curve the bands would shape if it applied,
    /// muted, with their points on it. Nothing here is applied — the chart's own 0 dB grid
    /// line, the header's `Mode ‹ Flat ›`, and the muted colour all say so — so this is not a
    /// flat line unless the bands are.
    FlatMode,
    /// Loudness mode: no curve, since the device's contour is not known until it is measured,
    /// and a line of text in its place.
    Loudness,
}

impl Graph {
    /// What the graph draws for `app`'s tone control. A mode the device has not reported yet
    /// is taken as EQ, the curve its bands describe.
    fn of(app: &App, eq: &EqSpec) -> Graph {
        match app.eq_mode() {
            Some(EqMode::Flat) => Graph::FlatMode,
            Some(EqMode::Loudness) => Graph::Loudness,
            Some(EqMode::Eq) | None => {
                match plot(app, eq).filters.iter().any(|band| band.gain_db != 0.0) {
                    true => Graph::Curve,
                    false => Graph::FlatCurve,
                }
            }
        }
    }
}

/// Draw the tone-control screen into `area`, and answer with where each band's row landed.
///
/// A device with no tone control has no screen here and no key that reaches one.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) -> Vec<(FunctionId, Rect)> {
    let Some(eq) = app.profile.eq.as_ref() else {
        return Vec::new();
    };
    let (tier, [header, response, bands]) = rows(area, eq.bands.len());
    let standing = presets::standing(app);
    let target = target_of(app, standing.as_ref());

    let text = device::header_text(header);
    let (line, halves) = header_line(
        app,
        eq,
        theme,
        header,
        (standing.as_ref(), target.as_deref()),
    );
    let mouse_at = line
        .width()
        .saturating_sub(device::mouse_span(app, theme).width());
    device::draw_header(
        frame,
        header,
        theme,
        line,
        &device::Rule::Tabs {
            current: Screen::Tone,
            has_eq: true,
        },
        hits,
    );
    // The mouse indicator closes the line: its cells say what it is to a resting pointer.
    let mouse_width = device::mouse_span(app, theme).width();
    let (mark, hit) = device::mark_rect(text, mouse_at, mouse_width, Hit::MouseMark);
    hits.push(mark, hit);
    // The mode and the loudness are rows the keys change as well, drawn in the header: each
    // half is the row a change to it glows and shimmers over.
    let half = |(at, width): (usize, usize)| {
        Rect {
            x: text.x + u16::try_from(at).unwrap_or(0),
            y: text.y,
            width: u16::try_from(width).unwrap_or(0),
            height: 1,
        }
        .intersection(header)
    };
    let mut header_rows = vec![
        (eq.mode_id(), half(halves.mode)),
        (eq.loudness_id(), half(halves.loudness)),
    ];
    // A click anywhere on the Mode half but its own `‹ ›` toggles the flat mode; a click on
    // either arrow still steps the select, like every select's, so the narrower
    // hits are pushed after the half they sit inside of and win there. A click anywhere on the
    // Loudness half but its bar toggles the loudness mode; the wheel over it steps the
    // strength.
    hits.push(half(halves.mode), Hit::Mode);
    hits.push(half((halves.mode_arrows.0, 1)), Hit::ModePrev);
    hits.push(half((halves.mode_arrows.1, 1)), Hit::ModeNext);
    hits.push(half(halves.loudness), Hit::Loudness);
    // The bar inside the half sets the strength under a press and a drag, over the half's
    // toggle ([`Hit::LoudnessBar`]).
    if app.raw_of(eq.loudness_id()).is_some() {
        let bar = Rect {
            x: text.x + u16::try_from(halves.loudness_bar).unwrap_or(0),
            y: text.y,
            width: u16::try_from(LOUDNESS_WIDTH).unwrap_or(0),
            height: 1,
        };
        hits.push_bar(bar.intersection(header), bar, Hit::LoudnessBar);
    }
    let (chart, side) = split_response(response, tier, target.is_some());
    app.tone.target_cache.set_target(target.is_some());
    draw_response(
        frame,
        chart,
        curve_area_in(area, eq.bands.len(), target.is_some()),
        app,
        eq,
        (theme, tier, target.as_deref()),
        hits,
    );
    if let (Some(side), Some(view)) = (side, target.as_deref()) {
        fit::draw_side(frame, side, view, app, theme);
    }
    header_rows.extend(draw_bands(frame, bands, app, eq, (theme, tier), hits));
    header_rows
}

/// The imported preset the screen draws against the device, from what the header says is on it
/// or loaded with changes: `None` for every other preset, and for `custom`.
fn target_of(app: &App, standing: Option<&Standing>) -> Option<Arc<TargetView>> {
    standing
        .filter(|standing| standing.preset)
        .and_then(|standing| app.target_view(&standing.name))
}

/// The Response block's area `response` as the chart takes it, and the side pane's beside it:
/// the pane only in the wide tier, and only for an imported preset (`side`).
fn split_response(response: Rect, tier: Tier, side: bool) -> (Rect, Option<Rect>) {
    if !(tier.wide && side) {
        return (response, None);
    }
    let [chart, pane] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(fit::SIDE_WIDTH)])
            .spacing(tier.gap())
            .areas(response);
    (chart, Some(pane))
}

/// The tiers `area` falls in, and the rows the header, the Response block and the Bands block
/// take — the two blocks a row apart in the tall tier.
///
/// The Response block takes what the others leave, so a taller terminal draws a taller curve
/// and a device with fewer bands a longer one.
fn rows(area: Rect, bands: usize) -> (Tier, [Rect; 3]) {
    let (tier, [header, body]) = layout_tier::split(area);
    let table = u16::try_from(bands)
        .unwrap_or(0)
        .saturating_add(BANDS_CHROME + tier.top());
    let [response, _, bands] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(tier.row_gap()),
        Constraint::Length(table),
    ])
    .areas(body);
    (tier, [header, response, bands])
}

/// The bordered blocks this screen draws over `area`: the Response block with the curve in
/// it, and the Bands block under it that the keys move through. A device with no tone control
/// has no screen here and draws neither.
pub(crate) fn frames_of(area: Rect, app: &App) -> Vec<(FrameId, Rect)> {
    let Some(eq) = app.profile.eq.as_ref() else {
        return Vec::new();
    };
    let (tier, [_, response, bands]) = rows(area, eq.bands.len());
    let (chart, _) = split_response(response, tier, app.tone.target_cache.target());
    vec![(FrameId::Response, chart), (FrameId::Bands, bands)]
}

/// The cells this screen's response curve is drawn in over `area`, while it draws a curve at
/// all: shaped ([`Graph::Curve`]), a flat curve in EQ mode, or Flat mode's own curve, whatever
/// the bands hold ([`curve_muted`] tells the two kinds apart). `None` for a device with no tone
/// control and while the graph is Loudness mode's line of text, which draws no curve.
///
/// This is the area the draw colours as a spectrum by frequency, so it answers for a frame of
/// `app` rather than for a band count a caller has to know.
pub(crate) fn curve_of(area: Rect, app: &App) -> Option<Rect> {
    let eq = app.profile.eq.as_ref()?;
    (Graph::of(app, eq) != Graph::Loudness)
        .then(|| curve_area_in(area, eq.bands.len(), app.tone.target_cache.target()))
}

/// Whether the curve [`curve_of`] answers for is muted: a flat curve in EQ mode or Flat mode,
/// either of which the spectrum desaturates and rests red to violet rather than colouring as
/// the curve the bands are shaping ([`Graph::Curve`]). `false` on a device with no tone control
/// and in Loudness mode, where `curve_of` answers `None` and this is moot.
pub(crate) fn curve_muted(app: &App) -> bool {
    let Some(eq) = app.profile.eq.as_ref() else {
        return false;
    };
    matches!(Graph::of(app, eq), Graph::FlatCurve | Graph::FlatMode)
}

/// The cells the response curve is drawn in, for a screen `area` showing `bands` bands.
///
/// The chart keeps the gain labels and the y axis to the left of these cells, and the x axis
/// and its own labels on the two rows below them. This is the frame with no side pane; a frame
/// with one has fewer columns ([`curve_of`]).
#[cfg(test)]
pub(crate) fn curve_area(area: Rect, bands: usize) -> Rect {
    curve_area_in(area, bands, false)
}

/// The cells the response curve is drawn in, for a frame that does, or does not, have the side
/// pane beside the chart ([`split_response`]).
fn curve_area_in(area: Rect, bands: usize, side: bool) -> Rect {
    let (tier, [_, response, _]) = rows(area, bands);
    let (chart, _) = split_response(response, tier, side);
    let inner = response_block(tier).inner(chart);
    let left = GAIN_LABEL_WIDTH + 1;
    Rect {
        x: inner.x + left,
        y: inner.y,
        width: inner.width.saturating_sub(left),
        height: inner.height.saturating_sub(2),
    }
}

/// The header of a screen whose header area is `area`: what the device is, the preset on it, and
/// what its tone control is set to. This screen's header has no clock, so the mouse-capture
/// indicator sits at the far right instead, past the loudness bar.
///
/// The preset's name takes whatever's left of the line after the product, mode, loudness and
/// mouse indicator, cut short with `…` where that's less than the whole name; a line with no
/// room for any of it leaves it out.
///
/// The wordmark says svolder's own name, so a screen wide enough to draw it opens the line on
/// the device instead, as every other screen's header does.
fn header_line(
    app: &App,
    eq: &EqSpec,
    theme: &Theme,
    area: Rect,
    (standing, target): (Option<&Standing>, Option<&TargetView>),
) -> (Line<'static>, ModeHalves) {
    let width = device::header_text(area).width;
    let lead = match device::marked(area.width) {
        true => "",
        false => " svolder  ",
    };
    let mouse = device::mouse_span(app, theme);
    // The mode and the loudness at their own width, with no pad for a focused half: what the
    // preset's name is measured against, so moving between the rows never cuts it differently.
    let natural = mode_line(app, eq, theme, MODE_FIELD, 0).0.width();
    // The device's name gives way to the mode, the loudness and the mouse indicator, cut short
    // with `…` where it would push them off the line.
    let name_room =
        usize::from(width).saturating_sub(widgets::text::cells(lead) + natural + 2 + mouse.width());
    let mut spans = Vec::new();
    if !lead.is_empty() {
        spans.push(Span::styled(lead, theme.style(Role::Header)));
    }
    spans.push(Span::styled(
        device::cut_cells(&app.session.product, name_room),
        theme.style(Role::Header),
    ));
    let mut line = Line::from(spans);
    let room =
        usize::from(width).saturating_sub(line.width() + natural + 2 + mouse.width() + PRESET_GAP);
    if let Some(preset) = preset_text(standing, room) {
        line.push_span(Span::raw(preset));
        // What follows the name goes in only where it fits whole: the name is cut first.
        let words = [
            target.map(|view| view.header.clone()),
            device::host_eq_words(app, area),
        ];
        for words in words.into_iter().flatten() {
            let words = format!("{PRESET_OPEN}{words}");
            let left = usize::from(width)
                .saturating_sub(line.width() + natural + 2 + mouse.width() + PRESET_GAP);
            if widgets::text::cells(&words) > left {
                break;
            }
            line.push_span(Span::raw(words));
        }
    }
    // What mode_line has left to draw in, once this line's own text and the mouse indicator —
    // "  " and the indicator itself — have taken their share: the budget a focused half's own
    // trailing pad is bound to, so it can never push the mouse indicator's own last column off.
    let mode_budget = usize::from(width).saturating_sub(line.width() + 2 + mouse.width());
    let (mode, halves) = mode_line(app, eq, theme, MODE_FIELD, mode_budget);
    let tail = mode.width() + 2 + mouse.width();
    let gap = usize::from(width).saturating_sub(line.width() + tail);
    line.push_span(Span::raw(" ".repeat(gap)));
    let mode_start = line.width();
    for span in mode.spans {
        line.push_span(span);
    }
    line.push_span(Span::raw("  "));
    line.push_span(mouse);
    let halves = ModeHalves {
        mode: (mode_start + halves.mode.0, halves.mode.1),
        loudness: (mode_start + halves.loudness.0, halves.loudness.1),
        loudness_bar: mode_start + halves.loudness_bar,
        mode_arrows: (
            mode_start + halves.mode_arrows.0,
            mode_start + halves.mode_arrows.1,
        ),
    };
    (line, halves)
}

/// What the header says about the presets in `room` cells — `Preset NAME`, `Preset NAME*`,
/// `Preset custom` or `loading NAME`, after [`PRESET_OPEN`] — with the name cut short to fit, or
/// nothing where `room` does not hold one character of the name and the `…` after it.
fn preset_text(standing: Option<&Standing>, room: usize) -> Option<String> {
    let standing = standing?;
    let fixed = widgets::text::cells(PRESET_OPEN)
        + widgets::text::cells(standing.lead)
        + widgets::text::cells(standing.mark);
    let name_room = room.checked_sub(fixed)?;
    let length = widgets::text::cells(&standing.name);
    let name = match length <= name_room {
        true => standing.name.clone(),
        false if name_room >= 2 => widgets::text::cut_to_cells(&standing.name, name_room),
        false => return None,
    };
    Some(format!(
        "{PRESET_OPEN}{}{name}{}",
        standing.lead, standing.mark
    ))
}

/// The Response block's border and the padding inside it: its labels and its curve open and
/// close on no blank of their own, so the wide tier keeps two columns clear of them.
fn response_block(tier: Tier) -> Block<'static> {
    Block::bordered().padding(tier.bare_padding())
}

/// Push the hits of the Response block's title row: the title leads back to the device screen,
/// the words in the border do what their keys do. Answers with those words.
fn push_response_title(area: Rect, app: &App, theme: &Theme, hits: &mut HitMap) -> Line<'static> {
    // The keys are never in the chart, so its frame is the one a palette with an emphasis
    // colour draws faint and its title the unfocused one.
    let (words, word_hits) = widgets::actions::actions_title(
        &RESPONSE_ACTIONS,
        None,
        widgets::actions::action_room(area.width, widgets::text::cells(" Response ")),
        Screen::Tone,
        &Scene::of(app),
        theme,
    );
    let title = Rect {
        height: 1.min(area.height),
        ..area
    };
    hits.push(title, Hit::Tab(Screen::Device));
    widgets::actions::push_action_hits(hits, area, &words, &word_hits);
    words
}

/// What the Response block draws its datasets from: the sampled curve, the overlay of an imported
/// preset's target, and the bands' points, with how the graph is to be read.
struct ResponseSeries {
    /// Which of the graphs this is.
    graph: Graph,
    /// Whether the line and the points are drawn muted.
    muted: bool,
    /// The role the summed line is drawn in.
    line: Role,
    /// The summed curve as (log10 Hz, dB) points, none in Loudness mode.
    summed: Vec<(f64, f64)>,
    /// The bands the curve is drawn from, and the point each one's centre is marked at.
    plot: Plot,
    /// The target and the gap under the fit, when a preset's target is on show.
    overlay: Option<fit::Overlay>,
}

/// Sample the curve the Response block draws over `curve`, and gather the overlay and the points.
fn response_series(
    app: &App,
    eq: &EqSpec,
    target: Option<&TargetView>,
    curve: Rect,
) -> ResponseSeries {
    // Braille draws two dots across a cell, so the curve is sampled twice per column.
    let freqs = response::log_freqs(2 * usize::from(curve.width));
    let plot = plot(app, eq);
    let graph = Graph::of(app, eq);
    let summed: Vec<(f64, f64)> = match graph {
        // Flat mode draws the stored curve too, muted: what the bands would shape if the
        // bypass were not standing between them and what plays, so the line the bands' own
        // points sit on is never a flat one they are visibly off — see `Graph::FlatMode`.
        Graph::Curve | Graph::FlatCurve | Graph::FlatMode => freqs
            .iter()
            .zip(response::response_db(&plot.filters, &freqs))
            .map(|(hz, db)| (hz.log10(), db))
            .collect(),
        Graph::Loudness => Vec::new(),
    };
    // A muted graph draws its line and every point in the dim role here, the focused band's
    // too: the base coat a theme with no spectrum to draw in keeps, and the one the spectrum
    // (see `curve_of` and `curve_muted`) recolours desaturated over, at true colour and 256.
    let muted = matches!(graph, Graph::FlatCurve | Graph::FlatMode);
    let line = match muted {
        true => Role::Dim,
        false => Role::CurveSum,
    };

    // An imported preset's target is dotted under the fit, with the gap between them filled under
    // both. A muted curve is not what the device applies, so it has no gap to show against the
    // target. Neither is
    // drawn in Braille or Dot cells, the two the spectrum recolours.
    let overlay = target
        .filter(|_| graph != Graph::Loudness)
        .map(|view| fit::overlay(view, &plot.filters, curve));
    ResponseSeries {
        graph,
        muted,
        line,
        summed,
        plot,
        overlay,
    }
}

/// The Response block: the summed curve with a point per band — what the device applies in EQ
/// mode, or, in Flat mode, the stored curve it is not applying — or, in Loudness mode, a line
/// of text in the curve's place — and the axes they are read against. Its title is this
/// screen's own tab header, leading back to the device screen.
fn draw_response(
    frame: &mut Frame,
    area: Rect,
    curve: Rect,
    app: &App,
    eq: &EqSpec,
    (theme, tier, target): (&Theme, Tier, Option<&TargetView>),
    hits: &mut HitMap,
) {
    let words = push_response_title(area, app, theme, hits);
    let block = response_block(tier)
        .border_style(device::contour(
            app,
            theme,
            false,
            device::border_style(app, theme),
        ))
        .title(Span::styled(" Response ", device::pane_title(theme, false)))
        .title(words.right_aligned());
    let inner = block.inner(area);
    let ResponseSeries {
        graph,
        muted,
        line,
        summed,
        plot,
        overlay,
    } = response_series(app, eq, target, curve);
    // The mouse reaches the curve wherever one is drawn: a press grabs a band's point there.
    if graph != Graph::Loudness {
        hits.push(curve, Hit::Curve);
    }
    let mut datasets = Vec::new();
    if let Some(overlay) = overlay.as_ref() {
        if !muted {
            datasets.push(
                Dataset::default()
                    .marker(Marker::Block)
                    .graph_type(GraphType::Scatter)
                    .style(theme.style(Role::CurveGap))
                    .data(&overlay.gap),
            );
        }
        datasets.push(
            Dataset::default()
                .marker(Marker::Quadrant)
                .graph_type(GraphType::Scatter)
                .style(theme.style(Role::CurveTarget))
                .data(&overlay.target),
        );
    }
    datasets.push(
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(theme.style(line))
            .data(&summed),
    );
    if graph != Graph::Loudness {
        datasets.extend(plot.centres.iter().map(|(band, point)| {
            let role = match (muted, *band == app.tone.band) {
                (true, _) => Role::Dim,
                (false, true) => Role::CurveSelected,
                (false, false) => Role::CurvePoint,
            };
            Dataset::default()
                .marker(Marker::Dot)
                .graph_type(GraphType::Scatter)
                .style(theme.style(role))
                .data(point)
        }));
    }

    let axis = theme.style(Role::Axis);
    let chart = Chart::new(datasets)
        .block(block)
        // ratatui spreads an axis's labels evenly, which the frequencies of a log axis are
        // not, so one empty label keeps the row it draws them on and [`label_row`] fills it.
        .x_axis(
            Axis::default()
                .bounds([F_MIN.log10(), F_MAX.log10()])
                .labels([Line::raw("")])
                .style(axis),
        )
        // It spreads the gains' labels evenly too, rounding them onto rows its own way rather
        // than where its canvas draws a gain, so blank labels as wide as theirs keep their
        // column and [`gain_labels`] writes each on the row its gain is drawn in.
        .y_axis(
            Axis::default()
                .bounds([GAIN_FLOOR_DB, GAIN_CEILING_DB])
                .labels([GAIN_BLANK; 2])
                .style(axis),
        );
    frame.render_widget(chart, area);
    gain_labels(frame, inner, curve);
    if overlay.is_some() {
        fit::restyle(frame.buffer_mut(), curve, theme);
    }
    // A chart draws its dots in a colour alone, so a muted line and its points take the rest of
    // the dim role — the dim attribute at 16 colours — once they are drawn; the spectrum then
    // recolours the Braille and Dot cells desaturated wherever it has one to draw in. Nothing
    // else is drawn in the cells the curve goes in.
    if muted {
        frame.buffer_mut().set_style(curve, theme.style(Role::Dim));
    }
    frame.render_widget(
        Paragraph::new(Line::raw(label_row(inner, curve))),
        Rect {
            y: inner.bottom().saturating_sub(1),
            height: 1.min(inner.height),
            ..inner
        },
    );
    if graph == Graph::Loudness {
        let text = widgets::text::cut_to_cells(&loudness_text(app, eq), usize::from(curve.width));
        frame.render_widget(
            Paragraph::new(Line::styled(text, theme.style(Role::Text)).centered()),
            Rect {
                y: zero_row(curve),
                height: 1.min(curve.height),
                ..curve
            },
        );
    }
}

/// Where each band's point is drawn in `curve`: the band, 0-based, and the cell its centre lands
/// in — its column along the log frequency axis and its row down the gain axis — found the way
/// the chart places it ([`column_at`], [`row_at`]). A band the chart draws no point for is left
/// out: one the device has not reported in full, one whose gain is off the chart, and every band
/// in Loudness mode, which draws no curve.
pub(crate) fn points(app: &App, curve: Rect) -> Vec<(usize, (u16, u16))> {
    let Some(eq) = app.profile.eq.as_ref() else {
        return Vec::new();
    };
    if Graph::of(app, eq) == Graph::Loudness {
        return Vec::new();
    }
    plot(app, eq)
        .centres
        .iter()
        .filter_map(|&(band, [(log_hz, db)])| {
            Some((band, (column_at(curve, log_hz)?, row_at(curve, db)?)))
        })
        .collect()
}

/// The band whose centre is nearest column `x` of `curve` in log frequency, among the bands the
/// curve is drawn from — the lower band where two are as near — or `None` where none is drawn.
pub(crate) fn nearest_centre(app: &App, curve: Rect, x: u16) -> Option<usize> {
    let eq = app.profile.eq.as_ref()?;
    if Graph::of(app, eq) == Graph::Loudness {
        return None;
    }
    let x = f64::from(x.saturating_sub(curve.x));
    let mut nearest: Option<(usize, f64)> = None;
    for &(band, [(log_hz, _)]) in &plot(app, eq).centres {
        let off = (along(curve, log_hz) - x).abs();
        if nearest.is_none_or(|(_, best)| off < best) {
            nearest = Some((band, off));
        }
    }
    nearest.map(|(band, _)| band)
}

/// How many columns into `curve` the chart puts a frequency whose base-10 logarithm is
/// `log_hz`, before it rounds to a cell: the log axis from [`F_MIN`] to [`F_MAX`] spread over the
/// curve's first to its last column, worked out as ratatui's own canvas works it out so a point
/// is found exactly where the chart drew it.
fn along(curve: Rect, log_hz: f64) -> f64 {
    let (left, right) = (F_MIN.log10(), F_MAX.log10());
    (log_hz - left) * (f64::from(curve.width) - 1.0) / (right - left)
}

/// The column of `curve` the chart draws a point at a frequency whose base-10 logarithm is
/// `log_hz` in, or `None` off the log axis, where it draws none.
fn column_at(curve: Rect, log_hz: f64) -> Option<u16> {
    let (left, right) = (F_MIN.log10(), F_MAX.log10());
    if curve.width == 0 || !(left..=right).contains(&log_hz) {
        return None;
    }
    Some(curve.x + along(curve, log_hz).round() as u16)
}

/// The frequency, in Hz, column `x` of `curve` stands for on the log axis: the inverse of where
/// the chart draws one ([`column_at`]), so the frequency found for a cell is drawn back in that
/// very cell. A column off either end of the curve is taken as that end.
pub(crate) fn frequency_at(curve: Rect, x: u16) -> f64 {
    let (left, right) = (F_MIN.log10(), F_MAX.log10());
    let last = f64::from(curve.width.saturating_sub(1));
    if last == 0.0 {
        return F_MIN;
    }
    let along = f64::from(x.clamp(curve.x, curve.right() - 1) - curve.x);
    10f64.powf(left + along * (right - left) / last)
}

/// The gain, in dB, row `y` of `curve` stands for: the inverse of where the chart draws one
/// ([`row_at`]), so the gain found for a cell is drawn back in that very cell. A row above or
/// below the curve is taken as its top or its bottom, the chart's own highest and lowest gains.
pub(crate) fn gain_at(curve: Rect, y: u16) -> f64 {
    let last = f64::from(curve.height.saturating_sub(1));
    if last == 0.0 {
        return GAIN_CEILING_DB;
    }
    let down = f64::from(y.clamp(curve.y, curve.bottom() - 1) - curve.y);
    GAIN_CEILING_DB - down * (GAIN_CEILING_DB - GAIN_FLOOR_DB) / last
}

/// The row of `curve` the chart draws a point at `db` in, from the top of its cells, the way
/// ratatui's canvas rounds it, or `None` off the gains the chart spans, where it draws none.
fn row_at(curve: Rect, db: f64) -> Option<u16> {
    if curve.height == 0 || !(GAIN_FLOOR_DB..=GAIN_CEILING_DB).contains(&db) {
        return None;
    }
    let down = (GAIN_CEILING_DB - db) * (f64::from(curve.height) - 1.0)
        / (GAIN_CEILING_DB - GAIN_FLOOR_DB);
    Some(curve.y + down.round() as u16)
}

/// Whether row `y` of `curve` is the one a flat curve is drawn on ([`zero_row`]), a row above or
/// below the curve taken as its top or its bottom as [`gain_at`] takes it.
pub(crate) fn on_zero_row(curve: Rect, y: u16) -> bool {
    curve.height > 0 && y.clamp(curve.y, curve.bottom() - 1) == zero_row(curve)
}

/// The row of `curve` a flat curve is drawn on: where 0 dB falls between the gains the chart
/// spans, from the top of its cells.
fn zero_row(curve: Rect) -> u16 {
    row_at(curve, 0.0).unwrap_or(curve.y)
}

/// Write the y axis's labels into the column the chart keeps for them inside `inner`, left of
/// `curve`: each on the row its gain is drawn in ([`row_at`]), so the `0` sits on the row a flat
/// curve is drawn on at every size, as the frequencies sit on the columns theirs are.
fn gain_labels(frame: &mut Frame, inner: Rect, curve: Rect) {
    for (db, text) in GAIN_LABELS {
        let Some(y) = row_at(curve, db) else {
            continue;
        };
        let cells = Rect {
            x: inner.x,
            y,
            width: GAIN_LABEL_WIDTH,
            height: 1,
        }
        .intersection(inner);
        frame.render_widget(Paragraph::new(Line::raw(text)), cells);
    }
}

/// What the graph says in Loudness mode, from the DAC's own volume, since the loudness contour
/// grows as that goes down: [`LOUDNESS_AT_FULL`] at its top step, and anywhere else — and
/// while the volume is unread — [`LOUDNESS_ACTIVE`] with the strength the contour is set to.
fn loudness_text(app: &App, eq: &EqSpec) -> String {
    let full = app.volume().is_some_and(|volume| {
        matches!(
            (app.raw_of(volume.id), volume.range),
            (Some(raw), Some((_, top))) if raw >= top
        )
    });
    if full {
        return LOUDNESS_AT_FULL.to_string();
    }
    let strength = app
        .raw_of(eq.loudness_id())
        .map_or_else(|| MISSING.to_string(), |raw| raw.to_string());
    format!("{LOUDNESS_ACTIVE} · strength {strength}")
}

/// What the chart is drawn from.
struct Plot {
    /// The filter every band with a full set of readings describes.
    filters: Vec<BandParams>,
    /// Those bands again, each with the one point its centre is marked at.
    centres: Vec<(usize, [(f64, f64); 1])>,
}

/// The bands the curve is drawn from: the peaking filter each one describes, and the point
/// its centre sits at with the band it belongs to.
///
/// Each band is drawn at the steps it is on its way to ([`curve_drag::drawn_raw`]): a step a band
/// key, a drag or a recall has asked for, before the device has confirmed it, so the point
/// follows the keys and the pointer as they move it. A band that has neither a reading nor a
/// write asked for of its frequency, its gain and its Q is in neither: nothing here guesses at a
/// value the device has not sent.
fn plot(app: &App, eq: &EqSpec) -> Plot {
    let mut plot = Plot {
        filters: Vec::new(),
        centres: Vec::new(),
    };
    for band in 0..eq.bands.len() {
        let raw = |param| {
            eq.band_id(band, param)
                .and_then(|id| curve_drag::drawn_raw(app, id))
                .and_then(|raw| u8::try_from(raw).ok())
        };
        let f0 = raw(EqParam::Frequency).and_then(|raw| eq.frequency(band, raw));
        let gain_db = raw(EqParam::Gain).map(|raw| eq.gain_db(raw));
        let q = raw(EqParam::Q).map(|raw| eq.q(raw));
        let (Some(f0), Some(gain_db), Some(q)) = (f0, gain_db, q) else {
            continue;
        };
        plot.filters.push(BandParams { f0, gain_db, q });
        plot.centres.push((band, [(f0.log10(), gain_db)]));
    }
    plot
}

/// The x axis's labels, each centred on the column its frequency is drawn in, written into a
/// row as wide as the block's inside. A label that would run into the one before it is left
/// out rather than drawn somewhere it does not mean.
fn label_row(inner: Rect, curve: Rect) -> String {
    let mut row = vec![' '; usize::from(inner.width)];
    let decades = (F_MAX / F_MIN).log10();
    let span = f64::from(curve.width.saturating_sub(1));
    let left = usize::from(curve.x.saturating_sub(inner.x));
    let mut free = 0;
    for (hz, text) in FREQUENCY_LABELS {
        let width = widgets::text::cells(text);
        let Some(last) = row.len().checked_sub(width) else {
            continue;
        };
        let centre = left + ((hz / F_MIN).log10() / decades * span).round() as usize;
        let start = centre.saturating_sub(width / 2).min(last);
        if start < free {
            continue;
        }
        for (n, letter) in text.chars().enumerate() {
            row[start + n] = letter;
        }
        free = start + width + 1;
    }
    row.into_iter().collect()
}

/// The Bands block: what every band is set to, and which one the keys act on, with what the
/// bands come to written on its bottom border — [`BANDS_APPLY_IN_EQ`] in a mode they do not
/// apply in, where the rows are dimmed, and the clipping note while the curve boosts.
///
/// A band's row is reported under each of the three functions it shows — its frequency, its
/// gain and its Q — so whichever of them changes, the animation layer finds the one row that
/// says so.
fn draw_bands(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    eq: &EqSpec,
    (theme, tier): (&Theme, Tier),
    hits: &mut HitMap,
) -> Vec<(FunctionId, Rect)> {
    let focused = app.tone.row == ToneRow::Band;
    let (words, word_hits) = widgets::actions::actions_title(
        &BANDS_ACTIONS,
        None,
        widgets::actions::action_room(area.width, widgets::text::cells(" Bands ")),
        Screen::Tone,
        &Scene::of(app),
        theme,
    );
    let mut block = tier
        .block()
        .border_style(device::contour(
            app,
            theme,
            focused,
            device::border_style(app, theme),
        ))
        .title(Span::styled(" Bands ", device::pane_title(theme, focused)))
        .title(words.clone().right_aligned());
    let room = usize::from(area.width.saturating_sub(2));
    if let Some(note) = band_note(app, room) {
        // Dimmed text rather than the dim role alone, which at 16 colours has no colour of its
        // own and would take the contour's.
        let dim = theme.style(Role::Text).patch(theme.style(Role::Dim));
        block = block.title_bottom(Span::styled(note, dim));
    }
    let inner = block.inner(area);
    let mut lines = vec![Line::from(Span::styled(
        BAND_TITLES,
        theme.style(Role::Dim),
    ))];
    let mut rows = Vec::new();
    let lead = widgets::rows::with_cursor(Line::default(), false, theme).width();
    for band in 0..eq.bands.len() {
        let (text, fields) = band_text(app, eq, band);
        lines.push(band_row(text, app, band, theme, usize::from(inner.width)));
        let row = band_area(inner, band);
        hits.push(row, Hit::Band(band));
        // Each value, its unit with it, is a target of its own inside the row: a double-click
        // there opens the edit line on that value's field.
        for (field, (start, width)) in fields.into_iter().enumerate() {
            let x = u16::try_from(lead + start).unwrap_or(u16::MAX);
            let cells = Rect {
                x: row.x.saturating_add(x),
                width: u16::try_from(width).unwrap_or(0),
                ..row
            }
            .intersection(row);
            hits.push(cells, Hit::BandField(band, field));
        }
        rows.extend(band_ids(eq, band).map(|id| (id, row)));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
    widgets::actions::push_action_hits(hits, area, &words, &word_hits);
    rows
}

/// The row band `band` is drawn on inside `inner`, the Bands block's area within its border:
/// one row per band, under the column titles.
fn band_area(inner: Rect, band: usize) -> Rect {
    Rect {
        y: inner
            .y
            .saturating_add(u16::try_from(band + 1).unwrap_or(u16::MAX)),
        height: 1,
        ..inner
    }
    .intersection(inner)
}

/// Draw a band's edit line over `area` in place of the band's row, everything else dimmed round
/// it, and answer the row it took; nothing while no line is open, or on a device with no tone
/// control.
///
/// The line holds the band's number, its three fields — `f 160 Hz  g +0.0 dB  q 4.30` — and
/// its own keys, drawn in the theme's own colours as a modal box is: the fields in the primary
/// text and the keys in the accent. The focused field's text is drawn selected while the next
/// character typed replaces it and in the cursor's accent once it is being typed into, with the
/// terminal's own cursor after it. A refusal says why in the keys' place, in the alert role,
/// until the next key.
pub(crate) fn draw_edit_line(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
) -> Option<Rect> {
    let line = app.tone.edit_line.as_ref()?;
    let eq = app.profile.eq.as_ref()?;
    let (tier, [_, _, bands]) = rows(area, eq.bands.len());
    let row = band_area(tier.block().inner(bands), line.band);
    let (text, caret) = edit_line_text(line, usize::from(row.width), theme);
    widgets::surfaces::modal_ground(frame, row, theme);
    frame.render_widget(Paragraph::new(text), row);
    let x = row
        .x
        .saturating_add(u16::try_from(caret).unwrap_or(u16::MAX));
    if x < row.right() && row.height > 0 {
        frame.set_cursor_position(Position { x, y: row.y });
    }
    Some(row)
}

/// The edit line as a row `width` cells wide draws it, and the column the terminal's cursor goes
/// in: after the focused field's text.
///
/// Each field is its letter, its text in [`LINE_FIELDS`]'s cells (longer text scrolled to show
/// its end, where the cursor is), and its unit — the blank before the unit is the cursor's cell,
/// so a field reads `f 160 Hz` whether focused or not. Fields keep their places whatever is
/// typed into the one before. The keys end the row, dropping what `Tab` does where there's no
/// room for it. What the line last said stands in the keys' place while it carries it, cut short
/// with `…` where there's no room for all of it: a refusal in the alert role, a number that
/// found the band already on its step in the notice role.
fn edit_line_text(line: &EditLine, width: usize, theme: &Theme) -> (Line<'static>, usize) {
    let mut spans = vec![
        Span::raw(" "),
        Span::styled("▶", theme.style(Role::Cursor)),
        Span::raw(" "),
        Span::styled(format!("{} ", line.band + 1), theme.style(Role::Text)),
    ];
    let mut caret = 0;
    for (at, (&(letter, unit, room), field)) in LINE_FIELDS.iter().zip(&line.fields).enumerate() {
        if at > 0 {
            spans.push(Span::raw(" ".repeat(FIELD_GAP)));
        }
        spans.push(Span::styled(format!("{letter} "), theme.style(Role::Text)));
        let focused = at == line.focus;
        let (shown, column) =
            widgets::text::typed_window(&field.text, field.text.chars().count(), room);
        if focused {
            caret = spans.iter().map(Span::width).sum::<usize>() + column;
        }
        let role = match (focused, field.fresh) {
            (true, true) => Role::Selected,
            (true, false) => Role::Cursor,
            (false, _) => Role::Text,
        };
        let pad = room.saturating_sub(1 + widgets::text::cells(&shown));
        spans.push(Span::styled(shown, theme.style(role)));
        spans.push(Span::styled(format!(" {unit}"), theme.style(Role::Text)));
        spans.push(Span::raw(" ".repeat(pad)));
    }
    let used: usize = spans.iter().map(Span::width).sum();
    let said = match (&line.refusal, &line.already) {
        (Some(why), _) => Some((why, Role::Alert)),
        (None, Some(already)) => Some((already, Role::Notice)),
        (None, None) => None,
    };
    let keys = match said {
        Some((words, role)) => {
            let room = width.saturating_sub(used + FIELD_GAP + LINE_TRAIL);
            Line::from(vec![
                Span::raw(" "),
                Span::styled(widgets::text::cut_to_cells(words, room), theme.style(role)),
            ])
        }
        None => [&LINE_KEYS[..], &LINE_KEYS_SHORT[..]]
            .iter()
            .map(|keys| widgets::surfaces::modal_keys(keys, theme))
            .find(|keys| used + FIELD_GAP - 1 + keys.width() + LINE_TRAIL <= width)
            .unwrap_or_default(),
    };
    let gap = width.saturating_sub(used + keys.width() + LINE_TRAIL);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(keys.spans);
    (Line::from(spans), caret)
}

/// What the Bands block's bottom border says, in a border `room` cells long, each held off its ends
/// by a blank: [`BANDS_APPLY_IN_EQ`] in Flat and Loudness modes, and while the curve boosts in EQ
/// mode ([`App::clip_peak`]) how far it peaks and how
/// far down that keeps the volume — `peaks at +4.2 dB — keep the volume at or below -4.2 dB to
/// avoid clipping on loud masters`, without its last words in a border too short for them. Nothing
/// otherwise.
fn band_note(app: &App, room: usize) -> Option<String> {
    if !bands_apply(app) {
        return Some(format!(" {BANDS_APPLY_IN_EQ} "));
    }
    let peak = app.clip_peak()?;
    let advice = format!(
        "peaks at {peak:+.1} dB — keep the volume at or below {:.1} dB to avoid clipping",
        -peak
    );
    let whole = format!(" {advice}{LOUD_MASTERS} ");
    match widgets::text::cells(&whole) <= room {
        true => Some(whole),
        false => Some(format!(
            " {} ",
            widgets::text::cut_to_cells(&advice, room.saturating_sub(2))
        )),
    }
}

/// One band's row as the table draws it: `text` ([`band_text`]) behind the cursor, in the
/// focused or the dimmed style the row takes.
fn band_row(
    text: String,
    app: &App,
    band: usize,
    theme: &Theme,
    max_width: usize,
) -> Line<'static> {
    let focused = app.tone.band == band;
    let line = widgets::rows::with_cursor(Line::raw(text), focused, theme);
    row_style(line, focused, band_dim(app, focused), theme, max_width)
}

/// One band's row text: its frequency, gain and Q in human units, and the raw values behind
/// them — with where each of the three values sits in it, its unit with it, as `(start, width)`
/// in cells from the text's own start, in [`edit_line::PARAMS`] order.
///
/// The gain is read to the hundredth of a dB a device step lands on, `+1.45`, and set right in
/// a column as wide as the longest of them, `-10.41`, so its unit stays in line down the table;
/// a whole number of dB keeps the one decimal it always had, `+0.0`. The edit line opens on these
/// same texts.
fn band_text(app: &App, eq: &EqSpec, band: usize) -> (String, [(usize, usize); 3]) {
    let values = band_values(app, eq, band);
    let raw = |param: usize| {
        values.raws[param].map_or_else(|| MISSING.to_string(), |raw| raw.to_string())
    };
    let number = format!("{}  ", band + 1);
    let frequency = format!(
        "{:>5} Hz",
        values
            .hz
            .map_or_else(|| MISSING.to_string(), frequency_text)
    );
    let gain = format!(
        "{:>6} dB",
        values
            .gain
            .map_or_else(|| MISSING.to_string(), edit_line::gain_text)
    );
    let q = format!(
        "{:>4}",
        values
            .q
            .map_or_else(|| MISSING.to_string(), edit_line::q_text)
    );
    let (after_frequency, after_gain) = ("  ", "    ");
    let at_frequency = widgets::text::cells(&number);
    let at_gain =
        at_frequency + widgets::text::cells(&frequency) + widgets::text::cells(after_frequency);
    let at_q = at_gain + widgets::text::cells(&gain) + widgets::text::cells(after_gain);
    let fields = [
        (at_frequency, widgets::text::cells(&frequency)),
        (at_gain, widgets::text::cells(&gain)),
        (at_q, widgets::text::cells(&q)),
    ];
    let text = format!(
        "{number}{frequency}{after_frequency}{gain}{after_gain}{q}      {} / {} / {}",
        raw(0),
        raw(1),
        raw(2),
    );
    (text, fields)
}

mod fit;

#[cfg(test)]
mod tests;
