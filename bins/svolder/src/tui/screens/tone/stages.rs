// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer side's stages: the strip of four stops under the Response block and the one
//! panel under it that stands in place of the Points pane.
//!
//! The strip is one row: `EQ`, `Loudness`, `Space` and `Bypass`, each with a `●` while it is on
//! and a `○` while it is off and its level where it has one, the open stop in reverse video, and
//! the total at the end. The panel shows the open stage: the Points table, the Loudness rows,
//! the Space rows or the Bypass rows. While the keys are in it its border is heavy and drawn in
//! the role of the frame the keys are in, and its bottom border lists its keys. The window's
//! last row stays blank (the toast's row), so the panel never touches it.
//!
//! Every number and word is the view's: levels come from [`EqView::levels`]
//! (`svolder_host::eq::ChainLevels`), so nothing here works a level out.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use svolder_host::eq::levels::db_word;
use svolder_host::eq::{EqView, SpeakersView};

use crate::tui::app::crossfeed::{self as xf_words, LineRow};
use crate::tui::app::speakers as words;
use crate::tui::app::{App, Ld, Space, Stage, ToneRow, Xf};
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::screens::device::{self, row_style};
use crate::tui::screens::switch;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::crossfeed::{Built, draw_curves};
use super::points;

/// Rows of the panel inside its border below 40 rows of window.
const INNER_SHORT: u16 = 9;
/// Rows from 40.
const INNER_TALL: u16 = 13;
/// Rows from 48: the 16 points and their column titles.
const INNER_TALLEST: u16 = 17;
/// The columns of a row's label, the blank after it included.
const LABEL: usize = 11;
/// Columns of the side box the curves go in, from 120 columns.
const SIDE_WIDTH: u16 = 40;
/// The fewest rows the side box needs: its borders, the key line and a few of the chart.
const SIDE_MIN_HEIGHT: u16 = 7;
/// The strip's separator below the wide tier, and in it.
const SEP: &str = " \u{2502} ";
const SEP_WIDE: &str = "  \u{2502}  ";

/// The rows the panel takes for a window `height` rows tall: its inner rows and the two borders.
pub(super) fn panel_height(height: u16) -> u16 {
    2 + match height {
        h if h >= 48 => INNER_TALLEST,
        h if h >= 40 => INNER_TALL,
        _ => INNER_SHORT,
    }
}

/// A row put together: its line, the places in it a click lands on, and what a click anywhere on
/// it is.
type Row = (Line<'static>, Vec<(usize, usize, Hit)>, Option<Hit>);

/// A `−7.8` or `+7.8` level word with a true minus; no sign for zero.
fn level(db: f64) -> String {
    match db_word(db) {
        word if word == "0.0" => word,
        word if word.starts_with('\u{2212}') => word,
        word => format!("+{word}"),
    }
}

/// What one stop of the strip says: its name, the dot, and the level where it has one.
fn stop_text(app: &App, stage: Stage, wide: bool) -> String {
    let on = app.stage_on(stage);
    let dot = match on {
        true => "\u{25cf}",
        false => "\u{25cb}",
    };
    let levels = app.computer_eq().and_then(|eq| eq.levels.as_ref());
    let unit = match wide {
        true => " dB",
        false => "",
    };
    let more = match (stage, on) {
        (_, false) => String::new(),
        (Stage::Eq, true) => levels
            .map(|levels| format!(" {}{unit}", level(levels.eq_db)))
            .unwrap_or_default(),
        (Stage::Loudness, true) => app
            .loudness_view()
            .map(|view| format!(" {:.0} %", view.strength_pct))
            .unwrap_or_default(),
        (Stage::Space, true) => {
            let word = match (app.space_now(), wide) {
                (Space::Speakers, true) => "virtual speakers",
                (Space::Speakers, false) => "speakers",
                _ => "crossfeed",
            };
            let db = levels
                .map(|levels| format!(" {}{unit}", level(levels.space_db)))
                .unwrap_or_default();
            format!(" {word}{db}")
        }
        (Stage::Bypass, true) => String::new(),
    };
    format!("{} {dot}{more}", stage.label())
}

/// The strip's tail: the total, in the words the view makes at 120 columns and a short form
/// below, except that an untouched or climbing chain keeps the daemon's words at any width.
fn total_text(app: &App, wide: bool) -> String {
    let Some(levels) = app.computer_eq().and_then(|eq| eq.levels.as_ref()) else {
        return String::new();
    };
    if levels.words.is_empty() {
        return String::new();
    }
    // The words are short when the chain is untouched or climbing, so they fit below 120 columns
    // too.
    if wide || levels.untouched || levels.climb_to_db.is_some() {
        return levels.words.clone();
    }
    let all = format!("all {} dB", db_word(levels.total_db));
    match levels.passthrough {
        true => format!("pass-through \u{b7} {all}"),
        false => all,
    }
}

/// Draw the strip into `area` (one row) and push its stops' hits.
pub(super) fn draw_strip(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) {
    let wide = Tier::of(frame.area()).wide;
    let open = app.open_stage();
    let on_strip = matches!(app.tone.row, ToneRow::Strip(_));
    let sep = match wide {
        true => SEP_WIDE,
        false => SEP,
    };
    let dim = theme.style(Role::Dim);
    let plain = Style::default();
    let mut spans = vec![Span::raw(" ")];
    let mut col = 1usize;
    let mut spots: Vec<(usize, usize, Stage)> = Vec::new();
    for (at, stage) in app.stages().into_iter().enumerate() {
        if at > 0 {
            spans.push(Span::styled(sep, dim));
            col += widgets::text::cells(sep);
        }
        let text = stop_text(app, stage, wide);
        let width = widgets::text::cells(&text);
        // The open stop is reversed: bold and boxed in the selected role while the keys are on the
        // strip, plain reverse video while they are in its panel.
        let style = match (stage == open, on_strip) {
            (true, true) => theme.style(Role::Selected),
            (true, false) => plain.add_modifier(Modifier::REVERSED),
            (false, _) => plain,
        };
        spots.push((col, width, stage));
        spans.push(Span::styled(text, style));
        col += width;
    }
    let total = total_text(app, wide);
    if !total.is_empty() {
        spans.push(Span::styled(sep, dim));
        spans.push(Span::styled(total, dim));
    }
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
    for (at, width, stage) in spots {
        let cell = Rect {
            x: area.x.saturating_add(u16::try_from(at).unwrap_or(u16::MAX)),
            width: u16::try_from(width).unwrap_or(0),
            ..area
        }
        .intersection(area);
        hits.push(cell, Hit::Stage(stage));
    }
}

/// The keys a panel lists on its bottom border, `droppable` ones last: they go first where the
/// line is too long.
fn keys_line(core: &[&str], droppable: &[&str], room: usize) -> String {
    let mut all: Vec<&str> = core.to_vec();
    let mut extra: Vec<&str> = droppable.to_vec();
    loop {
        let mut line: Vec<&str> = all.clone();
        // droppable words come before the last two (`␣ on/off` and `Esc strip`), as the mockups do
        let at = line.len().saturating_sub(2);
        for word in extra.iter().rev() {
            line.insert(at, word);
        }
        let text = line.join(" \u{b7} ");
        if widgets::text::cells(&text) + 4 <= room || extra.is_empty() {
            return text;
        }
        extra.pop();
        all = core.to_vec();
    }
}

/// Draw the open stage's panel into `area` and push the hits of its rows.
pub(super) fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    (theme, tier): (&Theme, Tier),
    hits: &mut HitMap,
) {
    let stage = app.open_stage();
    let keyed = !matches!(
        app.tone.row,
        ToneRow::Strip(_) | ToneRow::Mode | ToneRow::Target
    ) && App::row_is_stage(app.tone.row);
    let set = match keyed {
        true => border::THICK,
        false => border::PLAIN,
    };
    let style = device::contour(app, theme, keyed, device::border_style(app, theme));
    let title_style = device::pane_title(theme, keyed);
    let dim = theme.style(Role::Dim);
    let wide = tier.wide;
    let mut block = Block::bordered().border_set(set).border_style(style);
    let inner_width = usize::from(area.width.saturating_sub(2));
    let inner = block.inner(area);
    let (title, right, keys) = match stage {
        Stage::Eq => eq_titles(app, inner, wide),
        Stage::Loudness => loudness_titles(wide),
        Stage::Space => space_titles(app, wide),
        Stage::Bypass => (
            "Bypass".to_string(),
            String::new(),
            keys_line(
                &["\u{2423} or f bypass on/off", "Esc strip"],
                &[],
                inner_width,
            ),
        ),
    };
    block = block.title(Span::styled(format!(" {title} "), title_style));
    if !right.is_empty() {
        block = block.title(Line::from(Span::styled(format!(" {right} "), dim)).right_aligned());
    }
    // The window's last row is never drawn on, so the border's own bottom row is the panel's.
    block = block.title_bottom(Line::from(Span::styled(format!(" {keys} "), dim)).left_aligned());
    frame.render_widget(block, area);
    match stage {
        Stage::Eq => draw_points(frame, inner, app, (theme, tier), hits),
        Stage::Loudness => draw_loudness(frame, inner, app, theme, wide, hits),
        Stage::Space => draw_space(frame, inner, app, (theme, tier), hits),
        Stage::Bypass => draw_bypass(frame, inner, app, theme, wide, hits),
    }
}

/// How many points the EQ panel shows and from which, so the focused one is in view.
fn visible_points(app: &App, inner: Rect) -> (usize, usize) {
    let visible = usize::from(inner.height).saturating_sub(1);
    let start = points::scroll_start(app.computer_points().len(), visible, app.focused_point());
    (start, visible)
}

/// The EQ panel's titles: the count (and the preset where wide), what is scrolled away, the
/// preamp, and the keys.
fn eq_titles(app: &App, inner: Rect, wide: bool) -> (String, String, String) {
    let points = app.computer_points();
    let max = app.computer_eq().map_or(16, |eq| usize::from(eq.max));
    let (start, visible) = visible_points(app, inner);
    let mut title = String::from("EQ");
    if wide && let Some(name) = app.computer_eq().and_then(|eq| eq.preset.clone()) {
        title.push_str(&format!(" \u{b7} {name}"));
    }
    title.push_str(&format!(" \u{b7} {} of {max} points", points.len()));
    if start > 0 {
        title.push_str(&format!(" \u{b7} {} more \u{2191}", start));
    }
    let below = points.len().saturating_sub(start + visible);
    if below > 0 {
        title.push_str(&format!(" \u{b7} {below} more \u{2193}"));
    }
    let right = app
        .computer_eq()
        .map(|eq| {
            // The EQ's own share of the level; the preamp sent also holds crossfeed's headroom.
            points::preamp_text(
                eq.levels
                    .as_ref()
                    .map_or(eq.preamp_db, |levels| levels.eq_db),
            )
        })
        .unwrap_or_default();
    let on_off = match wide {
        true => "\u{2423} EQ on/off",
        false => "\u{2423} on/off",
    };
    let keys = keys_line(
        &[
            "\u{2191}\u{2193} row",
            "\u{2190}\u{2192} value",
            "Enter type",
            "a add",
            "x remove",
            on_off,
            "Esc strip",
        ],
        &["y type", "[ ] Q"],
        usize::from(inner.width) + 2,
    );
    (title, right, keys)
}

/// The Loudness panel's titles.
fn loudness_titles(wide: bool) -> (String, String, String) {
    let right = match wide {
        true => "follows the computer volume".to_string(),
        false => String::new(),
    };
    let enter = match wide {
        true => "Enter type a number",
        false => "Enter type",
    };
    let keys = [
        "\u{2191}\u{2193} row",
        "\u{2190}\u{2192} change",
        enter,
        "\u{2423} on/off",
        "r volume now",
        "Esc strip",
    ]
    .join(" \u{b7} ");
    ("Loudness".to_string(), right, keys)
}

/// The Space panel's titles: what plays, how much quieter it is, and the keys.
fn space_titles(app: &App, wide: bool) -> (String, String, String) {
    let now = app.space_now();
    let title = match now {
        Space::Off => "Space \u{b7} off".to_string(),
        Space::Crossfeed => "Space \u{b7} crossfeed".to_string(),
        Space::Speakers => "Space \u{b7} virtual speakers".to_string(),
    };
    let db = app
        .computer_eq()
        .and_then(|eq| eq.levels.as_ref())
        .map(|levels| levels.space_db)
        .unwrap_or(0.0);
    let right = match (now, db) {
        (Space::Off, _) => String::new(),
        (_, db) if db.abs() < 0.05 => String::new(),
        (Space::Speakers, db) => format!("about {} dB quieter", db_word(-db)),
        (_, db) => format!("plays {} dB quieter", db_word(-db)),
    };
    let enter = match (now, wide) {
        (Space::Speakers, _) => "Enter apply or open",
        _ => "Enter apply or type",
    };
    let keys = [
        "\u{2191}\u{2193} row",
        "\u{2190}\u{2192} change",
        enter,
        "\u{2423} on/off",
        "S sets",
        "Esc strip",
    ]
    .join(" \u{b7} ");
    (title, right, keys)
}

/// The EQ panel's rows: the column titles, then a row per point, scrolled to keep the focused
/// one in view.
fn draw_points(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    (theme, _tier): (&Theme, Tier),
    hits: &mut HitMap,
) {
    let points = app.computer_points();
    let (start, visible) = visible_points(app, inner);
    let focused = matches!(app.tone.row, ToneRow::Point(_));
    let mut lines = vec![Line::from(Span::styled(
        points::POINT_TITLES,
        theme.style(Role::Dim),
    ))];
    let on = app.computer_eq().is_some_and(|eq| eq.on);
    for at in start..points.len().min(start + visible) {
        let selected = at == app.focused_point() && focused;
        let text = match app.tone.point_line.as_ref().filter(|line| line.point == at) {
            Some(line) => points::line_text(line),
            None => points::row_text(app, at),
        };
        let line = widgets::rows::with_cursor(Line::raw(text), selected, theme);
        lines.push(row_style(
            line,
            selected,
            !on && !selected,
            theme,
            usize::from(inner.width),
        ));
        let row = Rect {
            y: inner.y + u16::try_from(lines.len() - 1).unwrap_or(0),
            height: 1,
            ..inner
        }
        .intersection(inner);
        hits.push(row, Hit::Point(at));
        for (field, (first, width)) in points::FIELDS.into_iter().enumerate() {
            let cells = Rect {
                x: row.x.saturating_add(
                    u16::try_from(widgets::CURSOR_WIDTH + first).unwrap_or(u16::MAX),
                ),
                width: u16::try_from(width).unwrap_or(0),
                ..row
            }
            .intersection(row);
            hits.push(cells, Hit::PointField(at, field));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Pad `label` to the label column; a label as wide as the column keeps two blanks after it.
fn label_text(label: &str) -> String {
    match widgets::text::cells(label) {
        n if n >= LABEL => format!("{label}  "),
        _ => format!("{label:<LABEL$}"),
    }
}

/// The rows of a panel, drawn one under the other from the top of `inner`, each with the hit of
/// the row as a whole under the hits of its parts.
fn put_rows(frame: &mut Frame, inner: Rect, rows: Vec<Row>, hits: &mut HitMap) {
    for (at, (line, spots, hit)) in rows.into_iter().enumerate() {
        let y = inner.y + u16::try_from(at).unwrap_or(0);
        if y >= inner.bottom() {
            break;
        }
        let cell = Rect {
            y,
            height: 1,
            ..inner
        };
        if let Some(hit) = hit {
            hits.push(cell, hit);
        }
        frame.render_widget(Paragraph::new(line), cell);
        for (at, cells, hit) in spots {
            let spot = Rect {
                x: cell.x.saturating_add(u16::try_from(at).unwrap_or(u16::MAX)),
                width: u16::try_from(cells).unwrap_or(0),
                ..cell
            };
            // The strength bar's cells are pushed as a bar, so a press reads a value off them.
            match hit {
                Hit::LdBar => hits.push_bar(spot.intersection(cell), spot, hit),
                _ => hits.push(spot.intersection(cell), hit),
            }
        }
    }
}

/// A row with the cursor and the label (`head` where focused), whose value the caller adds.
fn start(app: &App, theme: &Theme, row: ToneRow, label: &str) -> (Built, bool, [Style; 3]) {
    let focused = app.tone.row == row;
    let (head, lit, other) = switch::styles(theme, Style::default(), focused);
    let mut built = Built::new();
    // only the word takes the label's style: the blanks up to the value stay plain
    let text = label_text(label);
    let word = widgets::text::cells(label).min(widgets::text::cells(&text));
    built.put(label.to_string(), head, None);
    built.put(
        " ".repeat(widgets::text::cells(&text) - word),
        Style::default(),
        None,
    );
    (built, focused, [head, lit, other])
}

/// `‹ value ›` with a click on each arrow and on the value.
fn stepped(
    built: &mut Built,
    theme: &Theme,
    value: &str,
    style: Style,
    (row, step): (Hit, impl Fn(i64) -> Hit),
) {
    let arrow = theme.style(Role::ModeArrow);
    built.put("\u{2039} ", arrow, Some(step(-1)));
    built.put(value.to_string(), style, Some(row));
    built.put(" \u{203a}", arrow, Some(step(1)));
}

/// The edit line's text: `[37] %  Enter writes · Esc cancels`, or why the last key was refused.
fn edit_text(line: &crate::tui::app::XfLine) -> String {
    let tail = match &line.refusal {
        Some(why) => why.clone(),
        None => "Enter writes \u{b7} Esc cancels".to_string(),
    };
    format!("[{}] {}  {tail}", line.text, line.row.unit())
}

/// The open edit line for `row`, if there is one.
fn line_of(app: &App, row: LineRow) -> Option<&crate::tui::app::XfLine> {
    app.tone.xf_line.as_ref().filter(|line| line.row == row)
}

/// A row that says one thing, dim, as the panels' `Now` and `Level` rows do.
fn fact_row(app: &App, theme: &Theme, label: &str, text: &str, width: usize) -> Row {
    let mut built = Built::new();
    built.put(label_text(label), theme.style(Role::Dim), None);
    built.put(
        widgets::text::cut_to_cells(text, width.saturating_sub(CURSOR_WIDTH + LABEL + 1)),
        Style::default(),
        None,
    );
    let (line, spots) = built.finish(false, !app.session.connected, theme, width);
    (line, spots, None)
}

/// The Loudness panel: on or off, strength with its bar, reference, and what it does now.
fn draw_loudness(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    theme: &Theme,
    wide: bool,
    hits: &mut HitMap,
) {
    let Some(view) = app.loudness_view() else {
        return;
    };
    let width = usize::from(inner.width);
    let stale = !app.session.connected;
    let plain = Style::default();
    let mut rows: Vec<Row> = Vec::new();
    // On or off.
    {
        let (mut built, focused, [_, lit, other]) =
            start(app, theme, ToneRow::Ld(Ld::On), "Loudness");
        let word = if view.on { "on" } else { "off" };
        stepped(
            &mut built,
            theme,
            word,
            if view.on || focused { lit } else { other },
            (Hit::Ld(Ld::On), |step| Hit::LdStep(Ld::On, step)),
        );
        let (line, spots) = built.finish_marked(focused, stale, theme, width);
        rows.push((line, spots, Some(Hit::Ld(Ld::On))));
    }
    // Strength, with its bar.
    {
        let (mut built, focused, [_, lit, _]) =
            start(app, theme, ToneRow::Ld(Ld::Strength), "Strength");
        match line_of(app, LineRow::Ld(Ld::Strength)) {
            Some(line) => built.put(edit_text(line), plain, None),
            None => {
                stepped(
                    &mut built,
                    theme,
                    &format!("{:.0} %", view.strength_pct),
                    if focused { lit } else { plain },
                    (Hit::Ld(Ld::Strength), |step| {
                        Hit::LdStep(Ld::Strength, step)
                    }),
                );
                built.put("  ", plain, None);
                let range = view.ranges.strength_pct;
                let bar = 10;
                let spans = widgets::rows::bar_spans(
                    theme,
                    view.strength_pct.round() as i64,
                    (range.min.round() as i64, range.max.round() as i64),
                    bar,
                    focused,
                    |_| theme.style(Role::Bar),
                );
                built.put_spans(spans, bar, Some(Hit::LdBar));
            }
        }
        let (line, spots) = built.finish_marked(focused, stale, theme, width);
        rows.push((line, spots, Some(Hit::Ld(Ld::Strength))));
    }
    // Reference.
    {
        let (mut built, focused, [_, lit, _]) =
            start(app, theme, ToneRow::Ld(Ld::Reference), "Reference");
        match line_of(app, LineRow::Ld(Ld::Reference)) {
            Some(line) => built.put(edit_text(line), plain, None),
            None => {
                stepped(
                    &mut built,
                    theme,
                    &format!("{:.0} %", view.reference_pct),
                    if focused { lit } else { plain },
                    (Hit::Ld(Ld::Reference), |step| {
                        Hit::LdStep(Ld::Reference, step)
                    }),
                );
                let now = match (view.volume_pct, wide) {
                    (Some(volume), true) => {
                        format!(
                            "the volume you mixed at \u{b7} r: use the volume now ({volume:.0} %)"
                        )
                    }
                    (Some(volume), false) => {
                        format!("the volume you mixed at \u{b7} r: the volume now ({volume:.0} %)")
                    }
                    (None, _) => "the volume you mixed at \u{b7} r: needs the volume".to_string(),
                };
                let room = width.saturating_sub(built.col() + 2);
                built.put(
                    format!("  {}", widgets::text::cut_to_cells(&now, room)),
                    theme.style(Role::Dim),
                    None,
                );
            }
        }
        let (line, spots) = built.finish_marked(focused, stale, theme, width);
        rows.push((line, spots, Some(Hit::Ld(Ld::Reference))));
    }
    // What it does now.
    let room = width.saturating_sub(CURSOR_WIDTH + LABEL + 1);
    let now = crate::tui::app::computer_loudness::readout(view, app.dac_loudness_on(), true, room);
    rows.push(fact_row(app, theme, "Now", &now, width));
    put_rows(frame, inner, rows, hits);
}

/// The Plays row: `Plays  ‹ Crossfeed ›   Off · Crossfeed · Virtual speakers`, the one that plays
/// standing out and the highlighted one (before `Enter` applies it) reversed.
fn plays_row(app: &App, theme: &Theme, width: usize) -> Row {
    let stale = !app.session.connected;
    let row = ToneRow::Crossfeed(Xf::Space);
    let on_off = !app.space_shown();
    let label = match on_off {
        true => "Crossfeed",
        false => Xf::Space.label(),
    };
    let (mut built, focused, [_, lit, other]) = start(app, theme, row, label);
    let plain = Style::default();
    let hit = Hit::Crossfeed(Xf::Space);
    let step = |step: i64| Hit::CrossfeedStep(Xf::Space, step);
    let now = app.space_now();
    if on_off {
        let on = now == Space::Crossfeed;
        stepped(
            &mut built,
            theme,
            if on { "on" } else { "off" },
            if on || focused { lit } else { other },
            (hit, |step| Hit::CrossfeedStep(Xf::Space, step)),
        );
    } else {
        let shown = app.plays_highlight();
        stepped(
            &mut built,
            theme,
            shown.word(),
            if focused { lit } else { plain },
            (hit, step),
        );
        built.put("   ", plain, Some(hit));
        for (at, space) in app.plays_choices().into_iter().enumerate() {
            if at > 0 {
                built.put(" \u{b7} ", other, Some(hit));
            }
            let style = match (space == now, space == shown && shown != now) {
                (_, true) => plain.add_modifier(Modifier::REVERSED),
                (true, false) => lit,
                (false, false) => other,
            };
            built.put(space.word(), style, Some(hit));
        }
        if shown != now {
            built.put("  Enter plays this", theme.style(Role::Dim), Some(hit));
        }
    }
    let (line, spots) = built.finish_marked(focused, stale, theme, width);
    (line, spots, Some(hit))
}

/// The Space panel: what plays, then the crossfeed's rows or the virtual speakers' facts, with
/// the two curves in a side box from 120 columns.
fn draw_space(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    (theme, tier): (&Theme, Tier),
    hits: &mut HitMap,
) {
    let speakers = app.space_shown() && app.space_now() == Space::Speakers;
    // The side box takes its columns first; the rows keep the rest.
    let side = match (tier.wide, inner.height >= SIDE_MIN_HEIGHT) {
        (true, true) => Some(Rect {
            x: inner.right().saturating_sub(SIDE_WIDTH + 1),
            width: SIDE_WIDTH.min(inner.width.saturating_sub(1)),
            height: inner.height.min(SIDE_MIN_HEIGHT.max(inner.height.min(9))),
            ..inner
        }),
        _ => None,
    };
    let rows_area = match side {
        Some(side) => Rect {
            width: inner.width.saturating_sub(side.width + 1),
            ..inner
        },
        None => inner,
    };
    let width = usize::from(rows_area.width);
    let mut rows = vec![plays_row(app, theme, width)];
    if speakers {
        if let Some(view) = app.speakers() {
            rows.extend(speakers_rows(app, view, theme, width, tier.wide));
        }
    } else if let Some(view) = app.crossfeed() {
        rows.extend(crossfeed_rows(app, view, theme, width));
    }
    put_rows(frame, rows_area, rows, hits);
    if speakers
        && !tier.wide
        && let Some(view) = app.speakers()
        && !view.curve.hz.is_empty()
    {
        let y = rows_area.y + 6;
        if y < rows_area.bottom() {
            frame.render_widget(
                Paragraph::new(strips_row(view, theme, width)),
                Rect {
                    y,
                    height: 1,
                    ..rows_area
                },
            );
        }
    }
    if let Some(side) = side {
        draw_side(frame, side, app, speakers, theme);
    }
}

/// The side box's two curves, in a thin border titled `Same ear · other ear`.
fn draw_side(frame: &mut Frame, area: Rect, app: &App, speakers: bool, theme: &Theme) {
    let curves = match speakers {
        true => app.speakers().map(|view| {
            let (same, other) = same_other(view);
            (view.curve.hz.clone(), same, other)
        }),
        false => app.crossfeed().map(|view| {
            (
                view.curve.hz.clone(),
                view.curve.same_db.clone(),
                view.curve.other_db.clone(),
            )
        }),
    };
    let Some((hz, same, other)) = curves.filter(|(hz, ..)| !hz.is_empty()) else {
        return;
    };
    let block = Block::bordered()
        .border_style(theme.style(Role::Dim))
        .title(Span::styled(
            " Same ear \u{b7} other ear ",
            theme.style(Role::Dim),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    draw_curves(frame, inner, (&hz, &same, &other), theme);
}

/// The Plays-at line's words: how much quieter than Off what plays is.
fn plays_at(app: &App) -> String {
    let db = app
        .computer_eq()
        .and_then(|eq| eq.levels.as_ref())
        .map(|levels| levels.space_db)
        .unwrap_or(0.0);
    match db {
        db if db.abs() < 0.05 => "as loud as Off".to_string(),
        db if db < 0.0 => format!("{} dB quieter", db_word(-db)),
        db => format!("{} dB louder", db_word(db)),
    }
}

/// The crossfeed's rows under Plays: model, preset, level, crossover, delay and what it plays at.
fn crossfeed_rows(
    app: &App,
    view: &svolder_host::eq::crossfeed::CrossfeedView,
    theme: &Theme,
    width: usize,
) -> Vec<Row> {
    let plain = Style::default();
    let dim = theme.style(Role::Dim);
    let stale = !app.session.connected;
    let mut rows: Vec<Row> = Vec::new();
    let model = view
        .models
        .iter()
        .find(|m| m.word == view.model)
        .map_or(view.model.as_str(), |m| m.name.as_str());
    // Model.
    {
        let (mut built, focused, [_, lit, _]) =
            start(app, theme, ToneRow::Crossfeed(Xf::Model), Xf::Model.label());
        stepped(
            &mut built,
            theme,
            model,
            if focused { lit } else { plain },
            (Hit::Crossfeed(Xf::Model), |step| {
                Hit::CrossfeedStep(Xf::Model, step)
            }),
        );
        let (line, spots) = built.finish_marked(focused, stale, theme, width);
        rows.push((line, spots, Some(Hit::Crossfeed(Xf::Model))));
    }
    // Preset: a model with the one preset says so rather than offering an arrow that does nothing.
    {
        let (mut built, focused, [_, lit, _]) = start(
            app,
            theme,
            ToneRow::Crossfeed(Xf::Preset),
            Xf::Preset.label(),
        );
        let presets = app.xf_model().map_or(0, |model| model.presets.len());
        let hit = Hit::Crossfeed(Xf::Preset);
        let name = view.preset.as_deref().unwrap_or("your own");
        match (view.preset.is_some(), presets) {
            (true, 1) => built.put(
                format!("{name} (the only one)"),
                if focused { lit } else { plain },
                Some(hit),
            ),
            _ => {
                stepped(
                    &mut built,
                    theme,
                    name,
                    if focused { lit } else { plain },
                    (hit, |step| Hit::CrossfeedStep(Xf::Preset, step)),
                );
            }
        }
        if let Some(from) = view.edited_from.as_ref().filter(|_| view.preset.is_none()) {
            built.put(format!("  edited from {from}"), dim, None);
        }
        let (line, spots) = built.finish_marked(focused, stale, theme, width);
        rows.push((line, spots, Some(hit)));
    }
    // Level, crossover, delay.
    for row in [Xf::Level, Xf::Crossover, Xf::Delay] {
        let (mut built, focused, [_, lit, _]) =
            start(app, theme, ToneRow::Crossfeed(row), row.label());
        let hit = Hit::Crossfeed(row);
        let settable = row != Xf::Delay || view.delay_settable;
        match line_of(app, LineRow::Xf(row)) {
            Some(line) => built.put(edit_text(line), plain, None),
            None => {
                let value = match row {
                    Xf::Level => format!("{:.0} %", view.amount_pct),
                    Xf::Crossover => xf_words::crossover_text(view),
                    _ => xf_words::delay_text(view),
                };
                match settable {
                    true => stepped(
                        &mut built,
                        theme,
                        &value,
                        if focused { lit } else { plain },
                        (hit, move |step| Hit::CrossfeedStep(row, step)),
                    ),
                    false => built.put(value, dim, Some(hit)),
                }
            }
        }
        let (line, spots) =
            built.finish_marked(focused, stale || !settable && !focused, theme, width);
        rows.push((line, spots, Some(hit)));
    }
    rows.push(fact_row(app, theme, "Plays at", &plays_at(app), width));
    rows
}

/// The two curves as strips of blocks: the fallback below 120 columns.
fn strips_row(view: &SpeakersView, theme: &Theme, width: usize) -> Line<'static> {
    let (same, other) = same_other(view);
    let labels = "Same ear  ".len() + "   Other ear  ".len();
    let cols = (width.saturating_sub(CURSOR_WIDTH + labels) / 2).min(24);
    Line::from(vec![
        Span::raw(" ".repeat(CURSOR_WIDTH)),
        Span::styled("Same ear  ", theme.style(Role::Dim)),
        Span::styled(strip(&same, cols), theme.style(Role::CurveSum)),
        Span::styled("   Other ear  ", theme.style(Role::Dim)),
        Span::styled(strip(&other, cols), theme.style(Role::SelectName)),
    ])
}

/// What the same ear and the other ear hear, in dB over the view's grid.
fn same_other(view: &SpeakersView) -> (Vec<f64>, Vec<f64>) {
    let curve = &view.curve;
    let mean = |a: &[f64], b: &[f64]| -> Vec<f64> {
        a.iter().zip(b).map(|(a, b)| (a + b) / 2.0).collect()
    };
    (
        mean(&curve.ll_db, &curve.rr_db),
        mean(&curve.lr_db, &curve.rl_db),
    )
}

/// `db` as a strip of `cols` level blocks.
fn strip(db: &[f64], cols: usize) -> String {
    const BLOCKS: [char; 8] = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];
    const RANGE: [f64; 2] = [-30.0, 3.0];
    if db.is_empty() || cols == 0 {
        return String::new();
    }
    (0..cols)
        .map(|col| {
            let at = match cols {
                1 => 0,
                _ => col * (db.len() - 1) / (cols - 1),
            };
            let level = (db[at].clamp(RANGE[0], RANGE[1]) - RANGE[0]) / (RANGE[1] - RANGE[0]);
            BLOCKS[((level * (BLOCKS.len() - 1) as f64).round() as usize).min(BLOCKS.len() - 1)]
        })
        .collect()
}

/// The virtual speakers' rows under Plays: the set, then the facts the view holds.
fn speakers_rows(
    app: &App,
    view: &SpeakersView,
    theme: &Theme,
    width: usize,
    wide: bool,
) -> Vec<Row> {
    let plain = Style::default();
    let dim = theme.style(Role::Dim);
    let stale = !app.session.connected;
    let mut rows: Vec<Row> = Vec::new();
    {
        let (mut built, focused, [_, lit, _]) =
            start(app, theme, ToneRow::Crossfeed(Xf::Set), Xf::Set.label());
        let hit = Hit::Crossfeed(Xf::Set);
        let name = match (&view.room_name, &view.term) {
            (Some(room), Some(term)) => format!("{room} \u{b7} {term}  {}", view.label),
            _ => "none chosen".to_string(),
        };
        let room = width.saturating_sub(CURSOR_WIDTH + LABEL + 4 + 12);
        stepped(
            &mut built,
            theme,
            &widgets::text::cut_to_cells(&name, room),
            if focused { lit } else { plain },
            (hit, |step| Hit::CrossfeedStep(Xf::Set, step)),
        );
        built.put("   ", plain, Some(hit));
        built.put(
            "S sets\u{2026}",
            dim,
            Some(Hit::Key(crossterm::event::KeyCode::Char('S'))),
        );
        let (line, spots) = built.finish_marked(focused, stale, theme, width);
        rows.push((line, spots, Some(hit)));
    }
    let angles = view
        .angles
        .replace(" deg", "\u{b0}")
        .replace(", ", " \u{b7} ");
    let level = words::level_text(view);
    let source = view
        .source_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string();
    let mut facts: Vec<(&str, String)> = Vec::new();
    match (wide, angles.is_empty()) {
        (_, true) => facts.push(("Length", words::length_text(view))),
        (true, false) => {
            facts.push(("Angles", angles));
            facts.push(("Length", words::length_text(view)));
        }
        (false, false) => facts.push((
            "Angles",
            format!("{angles} \u{b7} {}", words::length_text(view)),
        )),
    }
    facts.push(("Level", level));
    facts.push(("Licence", words::licence_text(view)));
    if wide {
        facts.push(("Source", source));
    }
    for (label, text) in facts {
        rows.push(fact_row(app, theme, label, &text, width));
    }
    rows
}

/// What the bypass holds or would turn off, in words: `EQ Bass · Loudness 60 % · Space virtual
/// speakers`.
fn stages_words(eq: &EqView, held: Option<&svolder_host::eq::HeldStages>) -> String {
    let mut parts: Vec<String> = Vec::new();
    let (has_eq, has_ld, has_xf, has_sp) = match held {
        Some(held) => (held.eq, held.loudness, held.crossfeed, held.speakers),
        None => (
            eq.on,
            eq.loudness.as_ref().is_some_and(|view| view.on),
            eq.crossfeed.as_ref().is_some_and(|view| view.on),
            eq.speakers.as_ref().is_some_and(|view| view.on),
        ),
    };
    if has_eq {
        parts.push(match eq.preset.as_deref() {
            Some(name) => format!("EQ {name}"),
            None => "EQ".to_string(),
        });
    }
    if has_ld {
        parts.push(match eq.loudness.as_ref() {
            Some(view) => format!("Loudness {:.0} %", view.strength_pct),
            None => "Loudness".to_string(),
        });
    }
    if has_sp {
        parts.push("Space virtual speakers".to_string());
    } else if has_xf {
        parts.push("Space crossfeed".to_string());
    }
    match parts.is_empty() {
        true => "nothing".to_string(),
        false => parts.join(" \u{b7} "),
    }
}

/// A click on the Bypass row presses `f`.
fn bypass_key() -> Hit {
    Hit::Key(crossterm::event::KeyCode::Char('f'))
}

/// The Bypass panel: on or off, what it turns off or brings back, and the level.
fn draw_bypass(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    theme: &Theme,
    wide: bool,
    hits: &mut HitMap,
) {
    let (Some(eq), Some(view)) = (app.computer_eq(), app.bypass_view()) else {
        return;
    };
    let width = usize::from(inner.width);
    let stale = !app.session.connected;
    let dim = theme.style(Role::Dim);
    let mut rows: Vec<Row> = Vec::new();
    {
        let (mut built, focused, [_, lit, other]) = start(app, theme, ToneRow::Bypass, "Bypass");
        stepped(
            &mut built,
            theme,
            if view.on { "on" } else { "off" },
            if view.on || focused { lit } else { other },
            (bypass_key(), |_| bypass_key()),
        );
        let key = match wide {
            true => "    f from anywhere on the computer side",
            false => "    f anywhere on the computer side",
        };
        let room = width.saturating_sub(built.col());
        built.put(widgets::text::cut_to_cells(key, room), dim, None);
        let (line, spots) = built.finish_marked(focused, stale, theme, width);
        rows.push((line, spots, Some(bypass_key())));
    }
    let levels = eq.levels.as_ref();
    match view.on {
        true => {
            rows.push(fact_row(
                app,
                theme,
                "Brings back",
                &stages_words(eq, Some(&view.held)),
                width,
            ));
            // The daemon's words as they are: only `levels.untouched` may call a chain untouched.
            let now = match levels {
                Some(levels) if !levels.words.is_empty() => levels.words.clone(),
                _ => "all stages are off".to_string(),
            };
            rows.push(fact_row(app, theme, "Now", &now, width));
        }
        false => {
            let turns = match wide {
                true => format!("{}, and brings each back as it was", stages_words(eq, None)),
                false => stages_words(eq, None),
            };
            rows.push(fact_row(app, theme, "Turns off", &turns, width));
            let words = levels
                .map(|levels| levels.words.clone())
                .unwrap_or_default();
            let level = match (wide, words.is_empty()) {
                (_, true) => String::new(),
                (true, false) => format!(
                    "{words}; bypassed it climbs to 0.0 dB, or to the level the loaded curve allows"
                ),
                (false, false) => words,
            };
            if !level.is_empty() {
                rows.push(fact_row(app, theme, "Level", &level, width));
            }
        }
    }
    put_rows(frame, inner, rows, hits);
}
