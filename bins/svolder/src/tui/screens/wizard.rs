// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One step of the setup wizard: what it asks, what answers it, and the keys that do.
//!
//! Every step is drawn the same way: the mark and the step's title in the header, what the step
//! needs the reader to know under it, then the choices as rows or the one line a step that takes
//! typing is typed into, and the key line last. What is on the screen is whatever
//! [`Prompt`](crate::wizard::Prompt) the wizard handed over, so a step the wizard changes changes
//! here with it, and this module decides nothing about the walk itself.
//!
//! A step with more of either than the screen holds shows part of it and says so: the choices
//! are a window that follows the cursor, and the step's own words scroll under the keys that
//! carry them. Nothing is dropped off the bottom of a frame without a way to reach it.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::app::Screen;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;
use crate::tui::widgets::footer::Footing;
use crate::tui::widgets::spinner;
use crate::tui::wizard::{Said, ShownStep};

use super::device::{Rule, draw_header, header_height, marked};

/// Columns kept clear down the left of the body, which is what every other screen leaves.
const LEFT: usize = 1;

/// What a step that takes a typed line is typed behind.
const MARKER: &str = ">";

/// Rows of its own words a step keeps, however many choices it is offering.
const MIN_SAID: usize = 3;

/// Draw the step `step` is on into `area`, and answer with how much of what it says was
/// shown.
///
/// The body is laid out from the top: what the step says, a blank row, and then what answers
/// it. The answer comes first when the two do not both fit: the choices are a window that
/// follows the cursor, with the number past each end on the row above and below, and what the
/// step says takes the rest and scrolls (the last row saying how many rows are below it). The
/// cursor is always drawn, and nothing is hidden with no key to reach it.
///
/// A notice takes the row directly above the key line, so what was refused is read beside the
/// keys that answer the step again.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    step: &ShownStep,
    theme: &Theme,
    hits: &mut HitMap,
) -> Said {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(header_height(area.width)),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(
        frame,
        header,
        theme,
        title_line(step, theme, header),
        &Rule::Plain,
        hits,
    );

    let notice = step.notice.as_deref().map(|notice| {
        (
            notice,
            Rect {
                y: body.y + body.height.saturating_sub(1),
                height: 1.min(body.height),
                ..body
            },
        )
    });
    let rows = match notice {
        Some(_) => body.height.saturating_sub(1),
        None => body.height,
    };
    let inside = Rect {
        height: rows,
        ..body
    };

    let width = usize::from(body.width).saturating_sub(LEFT);
    let mut said: Vec<Line<'static>> = Vec::new();
    for line in step.read() {
        said.extend(wrap(line, width).into_iter().map(indented));
    }
    let wait: Vec<Line<'static>> = match (step.waiting.as_deref(), step.spinner) {
        (Some(waiting), Some(glyph)) => vec![
            Line::default(),
            Line::from(Span::styled(
                format!(" {}", spinner::before(glyph, waiting)),
                theme.style(Role::Stale),
            )),
        ],
        _ => Vec::new(),
    };

    // The rows are shared out so that both are reachable: what answers the step takes what it
    // needs while the two of them fit, and all but [`MIN_SAID`] rows once they do not; what
    // the step says takes what is left, and scrolls through the rest.
    let room = usize::from(rows);
    let blank = usize::from(!said.is_empty());
    let free = room.saturating_sub(blank);
    let needed = match step.typing {
        true => 1 + wait.len(),
        false => step.prompt.choices.len() + wait.len(),
    };
    let capped = needed.min(free.saturating_sub(MIN_SAID)).max(1).min(free);
    let said_room = said.len().min(free.saturating_sub(capped));
    let answer_room = free - said_room;

    let (shown, more) = window(&said, step.scroll, said_room);
    let mut lines: Vec<Line<'static>> = shown.to_vec();
    if more > 0 && said_room > 0 {
        lines.truncate(said_room - 1);
        lines.push(Line::from(Span::styled(
            format!(" … {} more lines", more + 1),
            theme.style(Role::Faint),
        )));
    }
    let drawn = lines.len().saturating_sub(usize::from(more > 0));
    // A body that scrolls keeps its rows whether or not the screenful it is showing fills
    // them, so what answers the step stays where it is as the words are carried past it.
    if said.len() > said_room {
        lines.resize(said_room, Line::default());
    }
    if blank > 0 {
        lines.push(Line::default());
    }
    // Where the first row that answers the step landed, and which choice it is, for the clicks
    // that answer it.
    let answers_at = lines.len();
    let (answers, offered) = answering(step, theme, answer_room.saturating_sub(wait.len()), width);
    lines.extend(answers);
    lines.extend(wait);
    frame.render_widget(Paragraph::new(lines), inside);

    for at in 0..offered.shown {
        let row = Rect {
            x: body.x,
            y: body.y + u16::try_from(answers_at + offered.at + at).unwrap_or(u16::MAX),
            width: body.width,
            height: 1,
        };
        hits.push(row.intersection(inside), Hit::ListRow(offered.from + at));
    }
    // The terminal's own cursor sits in the line being typed, so a step that takes one looks
    // like the field it is.
    if step.typing && step.code.is_none() {
        let x = body.x + u16::try_from(widgets::CURSOR_WIDTH + step.caret).unwrap_or(u16::MAX);
        let y = body.y + u16::try_from(answers_at).unwrap_or(u16::MAX);
        if x < body.x + body.width && y < body.y + rows {
            frame.set_cursor_position(Position { x, y });
        }
    }

    if let Some((notice, area)) = notice {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {notice}"),
                theme.style(Role::Notice),
            ))),
            area,
        );
    }

    // What this frame made of the step is what its own key line says the keys can do: a hint
    // read off the frame before it would be one page behind the reader.
    let laid_out = Said {
        rows: drawn,
        lines: said.len(),
        window: offered.from,
    };
    widgets::footer::draw_footer(
        frame,
        footer,
        &Footing::resting(step.scene_of(laid_out)),
        theme,
        Screen::Setup,
        hits,
    );
    // The wizard has no panes: the theme's ground goes under all of it.
    widgets::surfaces::paint_surfaces(frame.buffer_mut(), &[], theme);
    laid_out
}

/// The rows of `said` from `scroll` on that fit in `room`, and how many are below them.
fn window<'a>(
    said: &'a [Line<'static>],
    scroll: usize,
    room: usize,
) -> (&'a [Line<'static>], usize) {
    let from = scroll.min(said.len());
    let rest = &said[from..];
    match rest.len() > room {
        true => (&rest[..room], rest.len() - room),
        false => (rest, 0),
    }
}

/// Which choices one frame drew, and where.
#[derive(Debug, Clone, Copy, Default)]
struct Offered {
    /// Rows into the answer block the first of them landed on.
    at: usize,
    /// Which choice that row is.
    from: usize,
    /// How many of them were drawn.
    shown: usize,
}

/// The rows that answer the step: the one line a typed step is typed into, or the choices as a
/// window `room` rows deep that follows the cursor, and which of them were drawn where.
///
/// A list that does not fit keeps a row at each end for the number of choices past it, so the
/// list never simply stops: what is drawn says there is more above or below it.
fn answering(
    step: &ShownStep,
    theme: &Theme,
    room: usize,
    max_width: usize,
) -> (Vec<Line<'static>>, Offered) {
    if step.typing {
        return (vec![field(step, theme)], Offered::default());
    }
    let choices = &step.prompt.choices;
    let row = |at: usize| choice_line(&choices[at], at == step.choice, theme, max_width);
    if choices.len() <= room {
        let offered = Offered {
            at: 0,
            from: 0,
            shown: choices.len(),
        };
        return ((0..choices.len()).map(row).collect(), offered);
    }
    let shown = room.saturating_sub(2).max(1);
    let from = following(step.said.window, step.choice, choices.len(), shown);
    let mut lines = vec![past(from, "↑", theme)];
    lines.extend((from..from + shown).map(row));
    lines.push(past(choices.len() - from - shown, "↓", theme));
    (lines, Offered { at: 1, from, shown })
}

/// Where a window `shown` choices deep starts on a list of `len` with the cursor on `choice`,
/// given that the frame before it started at `from`.
///
/// The window stays where it was until the cursor would leave it, and then moves by exactly as
/// much as it takes to draw the cursor again. A window that recentred on every move would slide
/// the whole list under the pointer, so the row a click landed on would carry a different choice
/// by the time the second click of a confirm arrived.
fn following(from: usize, choice: usize, len: usize, shown: usize) -> usize {
    if len <= shown {
        return 0;
    }
    let last = len - shown;
    let from = from.min(last);
    if choice < from {
        return choice;
    }
    if choice >= from + shown {
        return (choice + 1 - shown).min(last);
    }
    from
}

/// The row that says how many choices are past one end of the window, and an empty one where
/// there are none.
fn past(choices: usize, arrow: &str, theme: &Theme) -> Line<'static> {
    match choices {
        0 => Line::default(),
        more => Line::from(Span::styled(
            format!("   {arrow} {more} more"),
            theme.style(Role::Faint),
        )),
    }
}

/// The header's text: the step's own title, which is what the wizard has to say about where it
/// is.
///
/// A screen wide enough for the wordmark has svolder's name beside the title already; a
/// narrower one opens on the title itself, indented to where every other row of the screen
/// starts.
fn title_line(step: &ShownStep, theme: &Theme, area: Rect) -> Line<'static> {
    let mut spans = Vec::new();
    if !marked(area.width) {
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        step.prompt.title.clone(),
        theme.style(Role::Header),
    ));
    Line::from(spans)
}

/// One choice as a row, with the cursor on the one an answer would take.
fn choice_line(choice: &str, focused: bool, theme: &Theme, max_width: usize) -> Line<'static> {
    let line = widgets::rows::with_cursor(Line::from(choice.to_string()), focused, theme);
    if !focused {
        return line;
    }
    let style = theme.style(Role::Selected);
    let mut line = line.style(style);
    widgets::text::pad_selected(&mut line.spans, style, max_width);
    line
}

/// The one line a step that takes typing is typed into: what has been typed, or what an empty
/// line would answer with, held back so it reads as the default rather than as an answer.
///
/// The marker is the one the plain front end puts a typed answer behind, so a field is a field
/// either way round, and it is not the `▶` a choice carries: the two are answered differently
/// and never look alike.
fn field(step: &ShownStep, theme: &Theme) -> Line<'static> {
    // The passphrase is stepped between its three answers, so it reads between the arrows the
    // devices editor's rows do.
    if let Some(row) = step.code.as_ref() {
        return Line::from(vec![
            Span::raw(" "),
            Span::styled(MARKER, theme.style(Role::Cursor)),
            Span::raw(" ‹ "),
            Span::styled(row.shown(), theme.style(Role::SelectName)),
            Span::raw(" ›"),
        ]);
    }
    let typed = match step.typed.is_empty() {
        true => Span::styled(
            step.placeholder().unwrap_or_default().to_string(),
            theme.style(Role::Faint),
        ),
        false => Span::styled(step.typed.clone(), theme.style(Role::Text)),
    };
    Line::from(vec![
        Span::raw(" "),
        Span::styled(MARKER, theme.style(Role::Cursor)),
        Span::raw(" "),
        typed,
    ])
}

/// One body row, in the column the rows of every screen start in.
fn indented(row: String) -> Line<'static> {
    Line::from(format!(" {row}"))
}

/// `text` broken into rows of at most `width` cells.
///
/// Rows break on spaces where there is one to break on, and mid-word where a word is longer
/// than the whole row; a line that opens with spaces — the commands the health rows list under
/// themselves do — keeps that indent on every row it takes.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let indent = text.chars().take_while(|c| *c == ' ').count();
    let room = width.saturating_sub(indent).max(1);
    let pad = " ".repeat(indent.min(width));
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for word in text.split_whitespace() {
        if !row.is_empty() && widgets::text::cells(&row) + 1 + widgets::text::cells(word) > room {
            rows.push(format!("{pad}{row}"));
            row = String::new();
        }
        if widgets::text::cells(word) > room {
            let mut pieces = widgets::text::wrap(word, room);
            row = pieces.pop().unwrap_or_default();
            rows.extend(pieces.into_iter().map(|piece| format!("{pad}{piece}")));
            continue;
        }
        match row.is_empty() {
            true => row = word.to_string(),
            false => {
                row.push(' ');
                row.push_str(word);
            }
        }
    }
    if !row.is_empty() || rows.is_empty() {
        rows.push(format!("{pad}{row}"));
    }
    rows
}

#[cfg(test)]
mod tests;
