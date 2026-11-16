// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone screen's header: the device, the preset on it, the side switch and the mode.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use svolder_profiles::EqSpec;

use crate::tui::app::{App, Side, TargetView};
use crate::tui::boxes::presets::Standing;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;

use super::device::{self, ModeHalves, mode_line};
use super::response::plot;
use super::switch;

/// Columns the mode's own label takes in the header.
const MODE_FIELD: usize = 5;

/// What sets the preset's name apart from the product before it in the header: the two blanks
/// every header sets its state apart from the device's identity with.
const PRESET_OPEN: &str = "  ";
/// Blank cells the header keeps between the preset's name and the mode, beyond the one the mode
/// line opens with.
const PRESET_GAP: usize = 1;

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
pub(super) fn header_line(
    app: &App,
    eq: &EqSpec,
    theme: &Theme,
    area: Rect,
    (standing, target): (Option<&Standing>, Option<&TargetView>),
) -> HeaderLines {
    let width = device::header_text(area).width;
    // With the computer's controls usable, the switch is the header's own line and the mode,
    // the loudness and the preset go on the row under it.
    let drawn = match split_of(app, area) {
        true => switch::draw(
            app,
            theme,
            switch_room(area),
            theme.style(Role::Header),
            app.switch_focused(),
        ),
        false => None,
    };
    let split = drawn.is_some();
    let lead = match device::marked(area.width) {
        true => "",
        false => " svolder  ",
    };
    let mouse = device::mouse_span(app, theme);
    let mouse_w = match split {
        true => 0,
        false => mouse.width(),
    };
    // The mode and the loudness at their own width, with no pad for a focused half: what the
    // preset's name is measured against, so moving between the rows never cuts it differently.
    let natural = mode_line(app, eq, theme, MODE_FIELD, 0).0.width();
    // The device's name gives way to the mode, the loudness and the mouse indicator, cut short
    // with `…` where it would push them off the line.
    let name_room =
        usize::from(width).saturating_sub(widgets::text::cells(lead) + natural + 2 + mouse_w);
    let mut spans = Vec::new();
    if !lead.is_empty() {
        spans.push(Span::styled(lead, theme.style(Role::Header)));
    }
    let mut second = None;
    let mut switch = None;
    let mut first = None;
    match (split, side_spans(app, theme)) {
        (true, _) => {
            if let Some(drawn) = drawn {
                let at = spans.iter().map(Span::width).sum::<usize>() + CURSOR;
                let [(a, wa, sa), (b, wb, sb)] = drawn.names;
                switch = Some([(sa, (at + a, wa)), (sb, (at + b, wb))]);
                let mark = match app.switch_focused() {
                    true => Span::styled("▶ ", theme.style(Role::Cursor)),
                    false => Span::raw("  "),
                };
                let mut line = spans.clone();
                line.push(mark);
                line.extend(drawn.spans);
                first = Some(line);
            }
            // the product and the logo word are on the switch's row; this row opens on blanks
            if !lead.is_empty() {
                spans[0] = Span::raw(" ".repeat(widgets::text::cells(lead)));
            }
            spans.push(Span::raw(" ".repeat(CURSOR)));
        }
        (false, Some((words, cells)))
            if widgets::text::cells(lead) + cells.0 <= usize::from(width) =>
        {
            spans.extend(words);
        }
        _ => spans.push(Span::styled(
            device::cut_cells(&app.session.product, name_room),
            theme.style(Role::Header),
        )),
    }
    let mut line = Line::from(spans);
    let room = usize::from(width).saturating_sub(line.width() + natural + 2 + mouse_w + PRESET_GAP);
    if let Some(preset) = preset_text(standing, room) {
        line.push_span(Span::raw(preset));
        // What follows the name goes in only where it fits whole: the name is cut first.
        // The fit of the bands on the device, which a band edit moves; left out while a band is
        // unread, as the web page leaves out its overlay.
        let bands = plot(app, eq).filters;
        let fit = target
            .filter(|_| bands.len() == eq.bands.len())
            .map(|view| view.header(&bands));
        let words = [fit, device::host_eq_words(app, area)];
        for words in words.into_iter().flatten() {
            let words = format!("{PRESET_OPEN}{words}");
            let left = usize::from(width)
                .saturating_sub(line.width() + natural + 2 + mouse_w + PRESET_GAP);
            if widgets::text::cells(&words) > left {
                break;
            }
            line.push_span(Span::raw(words));
        }
    }
    // What mode_line has left to draw in, once this line's own text and the mouse indicator —
    // "  " and the indicator itself — have taken their share: the budget a focused half's own
    // trailing pad is bound to, so it can never push the mouse indicator's own last column off.
    let mode_budget = usize::from(width).saturating_sub(line.width() + 2 + mouse_w);
    let (mode, halves) = mode_line(app, eq, theme, MODE_FIELD, mode_budget);
    let tail = mode.width() + 2 + mouse_w;
    let gap = usize::from(width).saturating_sub(line.width() + tail);
    line.push_span(Span::raw(" ".repeat(gap)));
    let mode_start = line.width();
    for span in mode.spans {
        line.push_span(span);
    }
    let line = match first {
        Some(first) => {
            // The switch's row carries the mouse indicator at its end.
            let mut top = Line::from(first);
            let gap = usize::from(width).saturating_sub(top.width() + 2 + mouse.width());
            top.push_span(Span::raw(" ".repeat(gap)));
            top.push_span(Span::raw("  "));
            top.push_span(mouse);
            second = Some(line);
            top
        }
        None => {
            line.push_span(Span::raw("  "));
            line.push_span(mouse);
            line
        }
    };
    let halves = ModeHalves {
        mode: (mode_start + halves.mode.0, halves.mode.1),
        loudness: (mode_start + halves.loudness.0, halves.loudness.1),
        loudness_bar: mode_start + halves.loudness_bar,
        mode_arrows: (
            mode_start + halves.mode_arrows.0,
            mode_start + halves.mode_arrows.1,
        ),
    };
    HeaderLines {
        line,
        second,
        halves,
        switch,
    }
}

/// The cells the mouse indicator may take at the end of the switch's row.
const MOUSE_CELLS: usize = 9;

/// The cells the switch has on the header's first row of `area`: the line less the wordmark's
/// word where there is one, the row cursor and the mouse indicator.
fn switch_room(area: Rect) -> usize {
    let lead = match device::marked(area.width) {
        true => 0,
        false => widgets::text::cells(" svolder  "),
    };
    usize::from(device::header_text(area).width).saturating_sub(lead + CURSOR + 2 + MOUSE_CELLS)
}

/// Whether the header is two rows: the switch's, and the mode's under it. It is where the
/// computer's controls can be used and the row has room for the switch.
pub(super) fn split_of(app: &App, area: Rect) -> bool {
    app.switch_offered() && switch::fits(switch_room(area))
}

/// The cells the row cursor takes in front of the switch's names.
const CURSOR: usize = 2;

/// What the header draws: its first line, the row under it where the switch has the first,
/// where the mode's halves lie (on the second row where there is one) and the switch's names.
pub(super) struct HeaderLines {
    pub(super) line: Line<'static>,
    pub(super) second: Option<Line<'static>>,
    pub(super) halves: ModeHalves,
    pub(super) switch: Option<Switch>,
}

/// Where the two halves of the side switch lie in the header, each as a first cell and a width.
type Switch = [(Side, (usize, usize)); 2];

/// Where the computer's controls cannot be used, the header's words in the switch's place: the
/// label, the device's name and the reason. `None` where they can, or the computer is not offered.
#[allow(clippy::type_complexity, reason = "one private pair of answers")]
fn side_spans(
    app: &App,
    theme: &Theme,
) -> Option<(Vec<Span<'static>>, (usize, Option<[(usize, usize); 2]>))> {
    if !app.computer_offered() {
        return None;
    }
    let head = theme.style(Role::Header);
    if !app.computer_offered() {
        return None;
    }
    let why = app.host.computer.unavailable()?;
    let mut spans = vec![
        Span::styled(format!("{} ", switch::LABEL), head),
        Span::styled(app.session.product.clone(), head),
        Span::styled(format!("  {why}"), theme.style(Role::Dim)),
    ];
    let width = spans.iter().map(Span::width).sum();
    spans.shrink_to_fit();
    Some((spans, (width, None)))
}

/// What the header says about the presets in `room` cells — `Preset NAME`, `Preset NAME*`,
/// `Preset custom` or `loading NAME`, after [`PRESET_OPEN`] — with the name cut short to fit, or
/// nothing where `room` does not hold one character of the name and the `…` after it.
pub(super) fn preset_text(standing: Option<&Standing>, room: usize) -> Option<String> {
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
