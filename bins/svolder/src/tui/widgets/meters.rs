// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The meter rows of an amp's Controls pane: a bar over the scale the profile names, with the
//! class-A block, the tick, the recent peak and the percentage beside it.
//!
//! A meter is drawn like a level's bar (the same columns, label and bar cells) but is read the
//! other way round: the fill follows the live reading, its colour goes by how far up the scale it
//! is, and a dead meter draws empty and reads `off`.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use svolder_ipc::ColorDepth;
use svolder_profiles::{Control, Placed};

use super::rows::{Columns, bar_shade_style, bar_spans_of, gap, label, plain_row};
use super::{MISSING, TICK};
use crate::tui::app::Reading;
use crate::tui::theme::{Role, Theme};

/// What a meter's recent peak is marked with, on the cell the loudest reading reached.
const PEAK_MARK: &str = "▎";

/// Where a meter leaves its normal range, as a fraction of full scale.
const METER_HOT: f64 = 0.7;

/// Where a meter reaches the top of its scale.
const METER_CLIP: f64 = 0.9;

/// The cells a meter row writes its percentage, `off` or dash in, right-aligned.
const METER_TEXT: usize = 4;

/// What a meter row draws besides the control and its reading: the scale the reading is placed
/// on, whether the meters are live, and the bar's moving parts.
#[derive(Debug, Clone, Copy)]
pub struct MeterState {
    /// The meter's scale with its zero settled; `None` for a meter that names no scale.
    pub placed: Option<Placed>,
    /// Whether the meters are live. A dead meter draws dim and empty and reads `off`.
    pub live: bool,
    /// The raw value the bar draws at while a change is still gliding to the reading.
    pub eased: Option<i64>,
    /// How far up the live part the meter reached a moment ago, which is on its way back down.
    pub peak: Option<f64>,
}

/// One meter row: how far up its scale the last reading was.
///
/// A meter whose profile names no scale has nothing to draw against, so the row shows the
/// reading alone, no bar. On a power amp's logarithmic scale the bar opens with the class-A
/// block, `▓` cells ending in a `┃`, and the live part follows it: `█` up to the reading,
/// `░` for the rest. A linear scale has no block; a meter with a `tick` marks where it crosses
/// into class-A operation with `┃` on the bar itself. At 16 colours the whole fill takes one
/// role by the reading's own fraction of the live part, clipped at or above [`METER_CLIP`],
/// hot at or above the tick (or [`METER_HOT`] without one) and ok below; at 256 and true colour
/// each filled cell instead blends green through yellow to red along the live part. On a
/// palette with its own emphasis colour, the fill draws in the emphasis at every depth, as a
/// level's bar does ([`Theme::bar_shade`]), and the tick and peak mark say where it stands.
///
/// Beside the bar the row writes the reading's percentage of the live part, `off` while the
/// meters are dead and [`MISSING`] before a reading arrives. A dead meter draws no fill and no
/// peak, and its tick goes faint.
///
/// `meter.eased` is how far up the scale the bar draws while a change is still gliding to the
/// reading behind it; the percentage, and the bar's role at 16 colours, are always the
/// confirmed value's. `meter.peak` is marked with [`PEAK_MARK`] on the live cell it reached
/// (past the bar's own fill, drawn over the empty cells rather than the bar, and never on the
/// class-A block).
///
/// `rows` is how many lines the row takes: a meter with a bar drawn over two draws the bar on
/// both, its label and percentage on the first alone. A meter with no bar takes one line
/// regardless.
pub fn meter_lines(
    control: &Control,
    reading: Option<&Reading>,
    theme: &Theme,
    columns: Columns,
    meter: MeterState,
    rows: usize,
) -> Vec<Line<'static>> {
    let Some(placed) = meter.placed else {
        return vec![plain_row(control, reading, columns)];
    };
    let alive = meter.live;
    let raw = reading.and_then(Reading::raw_i64);
    let confirmed = raw.map(|raw| placed.fraction(raw as f64));
    let drawn = meter
        .eased
        .or(raw)
        .map(|raw| placed.fraction(raw as f64))
        .filter(|_| alive);
    let width = columns.bar;
    let block = block_cells(placed.class_a(), width);
    let live = width - block;
    let filled = drawn.map_or(0, |p| cells_of(p, live));
    let tick_at = match block {
        0 => placed
            .tick()
            .map(|tick| cells_of(tick, width.saturating_sub(1))),
        block => Some(block - 1),
    };
    let peak_at = meter
        .peak
        .filter(|_| alive)
        .map(|peak| block + cells_of(peak, live.saturating_sub(1)))
        .filter(|at| *at >= block + filled && *at < width);
    let style_at: Box<dyn Fn(usize) -> Style> = match theme.depth() {
        _ if theme.emphasised() => Box::new(|i| bar_shade_style(theme, i, live)),
        ColorDepth::Ansi16 | ColorDepth::Auto => {
            let style = theme.style(meter_role(confirmed.unwrap_or(0.0), placed));
            Box::new(move |_| style)
        }
        ColorDepth::Ansi256 | ColorDepth::TrueColor => {
            Box::new(|i| meter_gradient_style(theme, i, live))
        }
    };
    let bar = || {
        if block == 0 && tick_at.is_none() && peak_at.is_none() {
            return bar_spans_of(theme, filled, width, false, &style_at);
        }
        (0..width)
            .map(|i| {
                if tick_at == Some(i) {
                    match alive {
                        true => Span::raw(TICK),
                        false => Span::styled(TICK, theme.style(Role::Faint)),
                    }
                } else if peak_at == Some(i) {
                    Span::styled(PEAK_MARK, theme.style(Role::Warn))
                } else if i < block {
                    Span::styled("▓", theme.style(Role::Faint))
                } else if i - block < filled {
                    Span::styled("█", style_at(i - block))
                } else {
                    Span::styled("░", theme.style(Role::Faint))
                }
            })
            .collect()
    };
    let text = match (alive, confirmed) {
        (false, _) => "off".to_string(),
        (true, Some(p)) => format!("{}%", (p * 100.0).round() as u32),
        (true, None) => MISSING.to_string(),
    };
    let text = format!("{text:>METER_TEXT$}");
    let mut first = vec![label(&control.label, columns.field)];
    first.extend(bar());
    first.push(Span::raw(format!("{}{text}", gap(&text, columns))));
    let mut lines = vec![Line::from(first)];
    for _ in 1..rows.max(1) {
        let mut more = vec![label("", columns.field)];
        more.extend(bar());
        lines.push(Line::from(more));
    }
    lines
}

/// The one role a whole meter fill draws in at 16 colours, for a confirmed reading `p` of the
/// way up the live part: clipped at or above [`METER_CLIP`], whatever the tick; otherwise hot
/// at or above a linear scale's tick when it has one, or at or above [`METER_HOT`] when it does
/// not; ok the rest of the way. The role goes by `p` itself, not by the cells it fills, so a
/// narrow bar changes colour at the same readings as a wide one.
fn meter_role(p: f64, placed: Placed) -> Role {
    if p >= METER_CLIP {
        return Role::MeterClip;
    }
    match placed.tick() {
        Some(tick) if p >= tick => Role::MeterHot,
        Some(_) => Role::MeterOk,
        None if p >= METER_HOT => Role::MeterHot,
        None => Role::MeterOk,
    }
}

/// How many of `n` cells a fill of `p` covers: whole cells only, an exact multiple landing on
/// its own cell despite floating-point rounding.
fn cells_of(p: f64, n: usize) -> usize {
    ((p * n as f64 + 1e-9).floor().max(0.0) as usize).min(n)
}

/// How many cells of a `width`-cell bar a class-A block of `class_a` of it covers: none for no
/// block, else at least one and never the whole bar.
fn block_cells(class_a: f64, width: usize) -> usize {
    if class_a <= 0.0 || width < 2 {
        return 0;
    }
    ((class_a * width as f64).round() as usize).clamp(1, width - 1)
}

/// A meter's filled cell `i` (of `width`) at 256 or true colour: green through yellow to red
/// along the bar's length, or no colour at all when the theme is monochrome.
fn meter_gradient_style(theme: &Theme, i: usize, width: usize) -> Style {
    if theme.monochrome() {
        return Style::default();
    }
    let stops = [
        theme.style(Role::MeterOk).fg.unwrap_or(Color::Green),
        theme.style(Role::MeterHot).fg.unwrap_or(Color::Yellow),
        theme.style(Role::MeterClip).fg.unwrap_or(Color::Red),
    ];
    let t = i as f64 / width as f64;
    Style::default().fg(theme.gradient(t, &stops))
}

#[cfg(test)]
mod tests;
