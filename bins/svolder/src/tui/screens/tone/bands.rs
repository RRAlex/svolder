// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone screen's Bands block: the table, its edit line and the notes under it.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use svolder_profiles::EqSpec;
use svolder_profiles::convert::frequency_text;
use svolder_proto::FunctionId;

use crate::tui::app::{App, Screen, ToneRow};
use crate::tui::boxes::edit_line::{self, EditLine};
use crate::tui::keys::KeyContext;
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::actions::Action;
use crate::tui::widgets::{self, MISSING};

use super::device::{
    self, BANDS_APPLY_IN_EQ, band_dim, band_ids, band_values, bands_apply, row_style,
};
use super::header::split_of;
use super::{crossfeed, rows, table_rows};

/// The word in the Bands block's top border.
const BANDS_ACTIONS: [Action; 1] = [Action {
    code: KeyCode::Char('0'),
    word: "0 flatten",
}];

/// The band table's column titles.
const BAND_TITLES: &str = "   #   Freq        Gain       Q        raw (f/g/q)";

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

/// The Bands block: what every band is set to, and which one the keys act on, with what the
/// bands come to written on its bottom border — [`BANDS_APPLY_IN_EQ`] in a mode they do not
/// apply in, where the rows are dimmed, and the clipping note while the curve boosts.
///
/// A band's row is reported under each of the three functions it shows — its frequency, its
/// gain and its Q — so whichever of them changes, the animation layer finds the one row that
/// says so.
pub(super) fn draw_bands(
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
        &KeyContext::of(app),
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
    let xf = crossfeed::layout(app, area);
    let (tier, [_, _, bands, _]) = rows(area, table_rows(app, eq), split_of(app, area), xf);
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
