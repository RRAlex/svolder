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
//! - [`mod@points`]: the computer's side: its curve and the Points table's rows.
//! - [`stages`]: the computer's strip and the panel under it.
//! - [`computer_loudness`]: the computer's Loudness row the device pane shares.

use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;
use svolder_profiles::tone::response::{F_MAX, F_MIN};
use svolder_profiles::{EqMode, EqSpec};
use svolder_proto::FunctionId;

use crate::tui::app::{App, Screen, TargetView};
use crate::tui::boxes::presets::{self, Standing};
use crate::tui::layout_tier::{self, Tier};
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render::FrameId;
use crate::tui::theme::Theme;
use crate::tui::widgets::{self, LOUDNESS_WIDTH};

use super::device;
use super::switch;

use bands::draw_bands;
pub(crate) use bands::draw_edit_line;
#[cfg(test)]
use header::preset_text;
use header::{HeaderLines, header_line, split_of};
use response::{Plot, ResponseSeries, draw_response, plot, response_block, row_at};
#[cfg(test)]
use response::{along, column_at, loudness_text, zero_row};
pub(crate) use response::{frequency_at, gain_at, nearest_centre, on_zero_row, points};

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
/// labels are, so it keeps their column for `response::gain_labels` to write in.
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
        if app.on_computer() {
            return points::graph(app);
        }
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
    let stages = app.stages_shown();
    let (tier, [header, response, bands]) =
        rows(area, table_rows(app, eq), split_of(app, area), stages);
    let standing = match app.on_computer() {
        true => computer_standing(app),
        false => presets::standing(app),
    };
    let target = target_of(app, standing.as_ref());

    let text = device::header_text(header);
    app.drawn.tone_switch.set(split_of(app, area));
    let HeaderLines {
        line,
        second,
        halves,
        switch,
    } = header_line(
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
    let half_on = |y: u16, (at, width): (usize, usize)| {
        Rect {
            x: text.x + u16::try_from(at).unwrap_or(0),
            y,
            width: u16::try_from(width).unwrap_or(0),
            height: 1,
        }
        .intersection(area)
    };
    // Where the switch has the header's own row, the mode's is the one under the rule.
    let mode_y = match second {
        Some(_) => header.y + header.height,
        None => text.y,
    };
    let half = |cells: (usize, usize)| half_on(mode_y, cells);
    if let Some(line) = second {
        let row = Rect { y: mode_y, ..text }.intersection(area);
        frame.render_widget(Paragraph::new(line), row);
    }
    let mut header_rows = vec![(eq.mode_id(), half(halves.mode))];
    if let Some(switch) = switch {
        // from the label to the end of the second name
        let start = switch[0]
            .1
            .0
            .saturating_sub(widgets::text::cells(switch::LABEL) + 3);
        let end = switch[1].1.0 + switch[1].1.1;
        hits.push(half_on(text.y, (start, end - start)), Hit::Target);
        header_rows.push((
            crate::tui::app::SWITCH_ROW,
            half_on(text.y, (start, end - start)),
        ));
        for (side, (at, width)) in switch {
            hits.push(half_on(text.y, (at, width)), Hit::Side(side));
        }
    }
    let computer = app.on_computer();
    if !computer {
        header_rows.push((eq.loudness_id(), half(halves.loudness)));
    }
    // A click anywhere on the Mode half but its own `‹ ›` toggles the flat mode; a click on
    // either arrow still steps the select, like every select's, so the narrower
    // hits are pushed after the half they sit inside of and win there. A click anywhere on the
    // Loudness half but its bar toggles the loudness mode; the wheel over it steps the
    // strength.
    hits.push(half(halves.mode), Hit::Mode);
    hits.push(half((halves.mode_arrows.0, 1)), Hit::ModePrev);
    hits.push(half((halves.mode_arrows.1, 1)), Hit::ModeNext);
    // The bar inside the half sets the strength under a press and a drag, over the half's
    // toggle ([`Hit::LoudnessBar`]).
    if !computer {
        hits.push(half(halves.loudness), Hit::Loudness);
    }
    if !computer && app.raw_of(eq.loudness_id()).is_some() {
        let bar = Rect {
            x: text.x + u16::try_from(halves.loudness_bar).unwrap_or(0),
            y: text.y,
            width: u16::try_from(LOUDNESS_WIDTH).unwrap_or(0),
            height: 1,
        };
        hits.push_bar(bar.intersection(header), bar, Hit::LoudnessBar);
    }
    let has_side = target.is_some() || computer;
    let (chart, side) = split_response(response, tier, has_side);
    app.tone.target_cache.set_target(has_side);
    draw_response(
        frame,
        chart,
        curve_area_in(
            area,
            table_rows(app, eq),
            has_side,
            split_of(app, area),
            stages,
        ),
        app,
        eq,
        (theme, tier, target.as_deref()),
        hits,
    );
    match (side, target.as_deref()) {
        (Some(side), _) if computer => fit::draw_computer_side(frame, side, app, theme),
        (Some(side), Some(view)) => fit::draw_side(frame, side, view, app, theme),
        _ => {}
    }
    if app.stages_shown() {
        // The strip is the row over the panel; the window's last row stays blank under it.
        let strip = Rect {
            y: bands.y.saturating_sub(1),
            height: 1,
            ..bands
        };
        stages::draw_strip(frame, strip, app, theme, hits);
        stages::draw_panel(frame, bands, app, (theme, tier), hits);
        return header_rows;
    }
    if computer {
        // The computer's view is not there: no strip and no panel, and no band table either.
        return header_rows;
    }
    header_rows.extend(draw_bands(frame, bands, app, eq, (theme, tier), hits));
    header_rows
}

/// How many rows the table under the curve has: a row per band. The computer's panel has a
/// height of its own ([`stages::panel_height`]), so a point added never moves the curve.
fn table_rows(_app: &App, eq: &EqSpec) -> usize {
    eq.bands.len()
}

/// What the header says of the preset on the computer: the one its points came from, `custom`
/// once a hand changed them.
fn computer_standing(app: &App) -> Option<Standing> {
    let name = app.computer_eq()?.preset.clone()?;
    Some(Standing {
        lead: "Preset ",
        name,
        mark: "",
        preset: false,
    })
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
/// take — the two blocks a row apart in the tall tier. With the computer's `stages` the third is
/// the panel and the strip is the row above it: the panel keeps the height
/// [`stages::panel_height`] gives it, the window's last row stays blank, and the Response block
/// takes what is left, so the graph never loses a row to the panel.
///
/// The Response block takes what the others leave, so a taller terminal draws a taller curve
/// and a device with fewer bands a longer one.
fn rows(area: Rect, bands: usize, split: bool, stages: bool) -> (Tier, [Rect; 3]) {
    let (tier, [header, body]) = layout_tier::split(area);
    // The mode's row under the header's rule, where the switch has the header's own.
    let body = match split {
        true => Rect {
            y: body.y + 1,
            height: body.height.saturating_sub(1),
            ..body
        },
        false => body,
    };
    if stages {
        let panel = stages::panel_height(area.height);
        let [response, _strip, panel, _blank] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(panel),
            Constraint::Length(1),
        ])
        .areas(body);
        return (tier, [header, response, panel]);
    }
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
    let (tier, [_, response, bands]) = rows(
        area,
        table_rows(app, eq),
        split_of(app, area),
        app.stages_shown(),
    );
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
    (Graph::of(app, eq) != Graph::Loudness).then(|| {
        curve_area_in(
            area,
            table_rows(app, eq),
            app.tone.target_cache.target(),
            split_of(app, area),
            app.stages_shown(),
        )
    })
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
    curve_area_in(area, bands, false, false, false)
}

/// The cells the computer side's curve is drawn in over `area`.
#[cfg(test)]
pub(crate) fn computer_curve_area(area: Rect) -> Rect {
    curve_area_in(area, 0, true, true, true)
}

/// The cells the response curve is drawn in, for a frame that does, or does not, have the side
/// pane beside the chart ([`split_response`]).
fn curve_area_in(area: Rect, bands: usize, side: bool, split: bool, stages: bool) -> Rect {
    let (tier, [_, response, _]) = rows(area, bands, split, stages);
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

mod bands;
pub(in crate::tui::screens) mod computer_loudness;
mod crossfeed;
mod fit;
mod header;
mod points;
mod response;
mod stages;

#[cfg(test)]
mod tests;
