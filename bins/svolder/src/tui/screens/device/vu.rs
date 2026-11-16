// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The VU rows of the Status pane: `VU L` and `VU R`, a needle on a track, its reading in VU and
//! the over lamp, under the `Now playing` row on a terminal of [`art::GROW_COLS`] by
//! [`art::GROW_ROWS`] or more.
//!
//! The needle is the engine's ([`svolder_host::vu`]) quantised to a cell of a 40-cell track: `─`
//! below 0 VU, `┼` at 0 VU, `┄` above it, `┃` where the needle is. The glyphs carry the meaning
//! in every theme, so a monochrome terminal reads the meter the same way.

use ratatui::text::{Line, Span};
use svolder_host::vu::{Reference, Scale};

use crate::tui::app::App;
use crate::tui::host::vu::Channel;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::MISSING;

use super::art;

/// The rows the VU meter takes in the Status pane: a blank row, then the two channels.
pub(super) const ROWS: u16 = 3;

/// The cells of a needle's track.
pub(super) const TRACK: usize = 40;

/// What the dark and the lit lamp are drawn as, in that order.
const LAMP: [&str; 2] = ["○", "●"];

/// Where each channel's row opens.
const LABELS: [&str; 2] = [" VU L ", " VU R "];

/// What the reading says for a needle below the scale, and how wide a reading is.
const BELOW: &str = "< -20";
const READING: usize = 5;

/// Whether a frame of `size` (columns, rows) is large enough for the VU rows.
pub(crate) fn vu_drawn_in(size: (u16, u16)) -> bool {
    size.0 >= art::GROW_COLS && size.1 >= art::GROW_ROWS
}

/// Whether the Status pane of a frame `size` large draws the VU rows: host audio shows and the
/// frame is large enough.
pub(super) fn vu_drawn(app: &App, size: (u16, u16)) -> bool {
    app.host_shown() && vu_drawn_in(size)
}

/// The cell of the track the needle is on, for a deflection (`0.0` at the left stop).
pub(super) fn needle_cell(deflection: f64) -> usize {
    let last = (TRACK - 1) as f64;
    (deflection.clamp(0.0, Scale::STOP) * last)
        .round()
        .min(last) as usize
}

/// The cell of the track that marks 0 VU.
pub(super) fn zero_cell() -> usize {
    needle_cell(Scale::deflection(1.0))
}

/// What the reading column says of a needle reading `db` VU: one decimal and its sign, `< -20`
/// below the scale.
pub(super) fn reading_text(db: f64) -> String {
    let rounded = (db * 10.0).round() / 10.0;
    if rounded.is_nan() || rounded < -20.0 {
        return BELOW.to_string();
    }
    // A reading of -0.0 says +0.0.
    format!("{:+.1}", rounded + 0.0)
}

/// The track as spans: the glyphs of the scale with the needle's cell in its own.
fn track(deflection: f64, theme: &Theme) -> Vec<Span<'static>> {
    let (needle, zero) = (needle_cell(deflection), zero_cell());
    let hot = needle >= zero;
    (0..TRACK)
        .map(|cell| match cell {
            _ if cell == needle => Span::styled(
                "┃",
                theme.style(match hot {
                    true => Role::MeterHot,
                    false => Role::MeterOk,
                }),
            ),
            _ if cell < zero => Span::styled("─", theme.style(Role::Faint)),
            _ if cell == zero => Span::styled("┼", theme.style(Role::Text)),
            _ => Span::styled("┄", theme.style(Role::Warn)),
        })
        .collect()
}

/// One channel's row: its label, the track, the reading and the lamp. `channel` is `None` while
/// nothing is captured, which reads `—` and a dark lamp, the needle at rest.
fn channel_line(label: &str, channel: Option<Channel>, theme: &Theme) -> Line<'static> {
    let at_rest = Channel {
        ratio: 0.0,
        deflection: 0.0,
        lit: false,
    };
    let mut spans = vec![Span::raw(label.to_string())];
    spans.extend(track(channel.unwrap_or(at_rest).deflection, theme));
    spans.push(Span::raw("  "));
    spans.push(match channel {
        Some(channel) => Span::raw(format!(
            "{:>width$}",
            reading_text(channel.reading()),
            width = READING
        )),
        None => Span::styled(format!("{MISSING:>READING$}"), theme.style(Role::Dim)),
    });
    spans.push(Span::raw("  "));
    spans.push(match channel.is_some_and(|channel| channel.lit) {
        true => Span::styled(LAMP[1], theme.style(Role::Alert)),
        false => Span::styled(LAMP[0], theme.style(Role::Dim)),
    });
    Line::from(spans)
}

/// The VU rows: a blank line, then the left channel and the right, as the app's meter reads at
/// the app's own clock.
pub(super) fn vu_lines(app: &App, theme: &Theme) -> Vec<Line<'static>> {
    let reading = app.host.capturing.then(|| app.host.vu.at(app.now));
    let mut lines = vec![Line::default()];
    for (at, label) in LABELS.into_iter().enumerate() {
        lines.push(channel_line(label, reading.map(|both| both[at]), theme));
    }
    lines
}

/// What the `Level from` row adds while the VU rows are drawn: where 0 VU is.
pub(super) fn reference_words(app: &App) -> String {
    format!(
        "   (0 VU = {} dBFS)",
        Reference::new(i32::from(app.tui.vu_reference)).db()
    )
}
