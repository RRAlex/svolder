// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset list's box, over the tone-control screen or the device screen.
//!
//! It is a modal box like the device editor: everything around it is dimmed, the box is drawn in
//! the theme's own colours, and it names its own keys on its last lines. It shows one of three
//! things — the presets that fit the tone control, the one-line prompt a name is typed into, or
//! the `[y/N]` question an update or a deletion asks — each in a box of its own size.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use svolder_profiles::EqSpec;
use svolder_profiles::tone::preset;

use crate::tui::app::App;
use crate::tui::boxes::presets::search::{self, SearchBox};
use crate::tui::boxes::presets::{self, Entry, FilePrompt, NameLine, PresetList, Question, Step};
use crate::tui::mouse::{HitMap, Selected};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::scroll::{MORE_DOWN, MORE_UP};
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::device::row_style;
use super::devices::boxed;

/// The keys that act on the list as a whole, and on the focused preset whatever it is.
const LIST_KEYS: [(&str, &str); 3] = [("Enter", "loads"), ("n", "saves new"), ("Esc", "closes")];
/// The keys that bring a preset in.
const ADD_KEYS: [(&str, &str); 2] = [("a", "AutoEq"), ("i", "from file")];
/// The keys that change the focused preset, which a built-in refuses.
const EDIT_KEYS: [(&str, &str); 3] = [("u", "updates"), ("c", "renames"), ("x", "deletes")];
/// Why the keys that change a preset are dimmed while a built-in is focused.
const READ_ONLY: &str = "built-ins are read-only";
/// What marks a built-in's row.
const BUILT_IN: &str = "built-in";
/// What marks an imported preset's row.
const IMPORTED: &str = "imported";
/// What marks the row of the preset that is on the computer.
const COMPUTER: &str = "computer";
/// What marks the row of the preset that is on.
const ON: &str = "●";
/// Columns between a built-in's name and its mark.
const GAP: usize = 2;
/// Blank cells the box keeps inside its right border after its longest line, as the one space
/// every line opens with keeps off its left.
const TRAIL: usize = 1;
/// What the name prompt's one row is called.
const NAME: &str = "Name";
/// Columns the name prompt's label takes, the space before the name included.
const NAME_FIELD: usize = 6;
/// Columns the name prompt's box takes, borders and all: the device editor's width.
const PROMPT_WIDTH: u16 = 56;
/// Rows it takes: the name, a blank line and its keys between two borders.
const PROMPT_HEIGHT: u16 = 5;
/// Columns a rename prompt's title keeps for its own word and the blanks round the name.
const TITLE_WORD: usize = " Rename  ".len();
/// The keys that answer the prompt a new preset's name is typed into.
const SAVE_KEYS: [(&str, &str); 2] = [("Enter", "saves"), ("Esc", "cancels")];
/// The keys that answer the prompt a new name for a preset is typed into.
const RENAME_KEYS: [(&str, &str); 2] = [("Enter", "renames"), ("Esc", "cancels")];
/// The keys that answer whether to update a preset; no is the default.
const UPDATE_KEYS: [(&str, &str); 2] = [("y", "updates"), ("Enter/n/Esc", "cancels")];
/// The keys that answer whether to delete one.
const DELETE_KEYS: [(&str, &str); 2] = [("y", "deletes"), ("Enter/n/Esc", "cancels")];
/// The keys that answer whether to move a preset between the DAC and the computer.
const MOVE_KEYS: [(&str, &str); 2] = [("y", "moves"), ("Enter/n/Esc", "cancels")];
/// Columns the search box takes, borders and all, where the terminal has them.
const SEARCH_WIDTH: u16 = 76;
/// Measurement rows the search box shows at most.
const SEARCH_ROWS: usize = 8;
/// What the search box's query line is called, and the columns it takes with its blank.
const SEARCH_LABEL: &str = "Search AutoEq";
/// What the search box says over its rows before anything is typed.
const SEARCH_HINT: &str = "type a headphone name";
/// What the search box says under the rows when the query finds nothing.
const SEARCH_NONE: &str = "no measurement matches";
/// The keys of the search box.
const SEARCH_KEYS: [(&str, &str); 4] = [
    ("↑↓", "move"),
    ("Enter", "imports"),
    ("Ctrl-R", "refreshes"),
    ("Esc", "back"),
];
/// What the file prompt's one row is called.
const FILE: &str = "File";
/// Columns the file prompt's label takes, the space before the path included.
const FILE_FIELD: usize = 6;
/// The keys of the file prompt.
const FILE_KEYS: [(&str, &str); 3] = [("Tab", "completes"), ("Enter", "imports"), ("Esc", "back")];

/// Draw whatever the preset list is showing over `area`, and answer the box it drew in; nothing
/// while the list is closed, or on a device with no tone control to list presets for.
///
/// The row the keys select in the list or the search results is recorded in `hits`.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) -> Option<Rect> {
    let list = app.presets.preset_list.as_ref()?;
    let eq = app.profile.eq.as_ref()?;
    Some(match &list.step {
        Step::Choosing => draw_list(frame, area, app, (eq, list), (theme, hits)),
        Step::Naming(line) => draw_prompt(frame, area, app, line, theme),
        Step::Asking(question) => draw_question(frame, area, app, question, theme),
        Step::Searching(search) => draw_search(frame, area, app, search, (theme, hits)),
        Step::Prompting(prompt) => draw_file(frame, area, app, prompt, theme),
    })
}

/// What follows a preset's name on its row.
struct Tags {
    /// `built-in` or `imported`, or nothing for a preset of bands.
    mark: &'static str,
    /// An imported preset's fit to this tone control: `fit 0.9 dB`.
    fit: Option<String>,
    /// Whether the preset is the one on the computer.
    computer: bool,
}

impl Tags {
    /// The tags of `entry` on the tone control `eq`. An imported preset's fit is the cached one:
    /// the list worked it out when it was opened or saved, so no frame fits.
    fn of(entry: &Entry, eq: &EqSpec, app: &App) -> Tags {
        let imported = entry.preset.source.as_ref();
        Tags {
            mark: match (entry.builtin, imported) {
                (true, _) => BUILT_IN,
                (false, Some(_)) => IMPORTED,
                (false, None) => "",
            },
            fit: imported
                .filter(|_| !eq.bands.is_empty())
                .map(|source| format!("fit {:.1} dB", preset::fit_for(eq, source).rms_db)),
            computer: app
                .computer_preset()
                .is_some_and(|name| preset::same_name(&name, &entry.preset.name)),
        }
    }
}

/// How the columns after the names are laid out.
struct Columns {
    /// Columns the names take.
    name: usize,
    /// Columns the `built-in` and `imported` marks take.
    mark: usize,
    /// Columns the fits take, none where the box is too narrow for them.
    fit: usize,
    /// Columns the `computer` mark takes, none while no row has it.
    computer: usize,
}

/// Draw the list over `area`: a row per preset that fits the tone control, the built-ins first
/// and marked so, the one that is on marked `●`, an imported one marked `imported` with its fit
/// where the box is wide enough, the one on the computer marked `computer`, then the keys. The
/// keys that change a preset are dimmed with the reason while a built-in is focused. A list
/// longer than the box holds shows the rows round the focused one, and says on its border which
/// way the rest are; a name longer than the box has room for beside the marks is cut short, so
/// the marks stay in view.
fn draw_list(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    (eq, list): (&EqSpec, &PresetList),
    (theme, hits): (&Theme, &mut HitMap),
) -> Rect {
    let entries = presets::entries(app, eq);
    let on = presets::on(app, eq, &entries);
    let row = list.row.min(entries.len().saturating_sub(1));
    let tags: Vec<Tags> = entries
        .iter()
        .map(|entry| Tags::of(entry, eq, app))
        .collect();
    let widest_of = |texts: &mut dyn Iterator<Item = usize>| texts.max().unwrap_or(0);
    let longest = widest_of(
        &mut entries
            .iter()
            .map(|entry| widgets::text::cells(&entry.preset.name)),
    );
    let mark = widest_of(&mut tags.iter().map(|tag| widgets::text::cells(tag.mark)));
    let fit = widest_of(
        &mut tags
            .iter()
            .filter_map(|tag| tag.fit.as_ref())
            .map(|fit| widgets::text::cells(fit)),
    );
    let computer = match tags.iter().any(|tag| tag.computer) {
        true => GAP + widgets::text::cells(COMPUTER),
        false => 0,
    };
    let room = usize::from(area.width).saturating_sub(2 + TRAIL);
    // What a row takes beside its name: the cursor, the `●` column, the gap and the marks.
    let fixed = CURSOR_WIDTH + widgets::text::cells(ON) + 1 + GAP + mark + computer;
    let fit = match longest + fixed + GAP + fit <= room {
        true => GAP + fit,
        false => 0,
    };
    let marks = fixed + fit;
    let columns = Columns {
        name: longest.min(room.saturating_sub(marks)),
        mark,
        fit: fit.saturating_sub(GAP),
        computer: computer.saturating_sub(GAP),
    };
    let read_only = entries.get(row).is_some_and(|entry| entry.builtin);
    let keys = [
        widgets::surfaces::modal_keys(&LIST_KEYS, theme),
        edit_keys(read_only, theme),
        widgets::surfaces::modal_keys(&ADD_KEYS, theme),
    ];
    // The box is as wide as the widest thing it can show, the reason included, so it keeps its
    // size while the focus moves between a built-in and a preset of the user's own.
    let widest = [
        marks + columns.name,
        keys[0].width(),
        edit_keys(true, theme).width(),
        keys[2].width(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let width = u16::try_from(widest + TRAIL + 2).unwrap_or(u16::MAX);
    // Two borders, the blank line and the three lines of keys are what the rows leave.
    let room = usize::from(area.height.saturating_sub(6)).max(1);
    let shown = entries.len().min(room);
    let top = (row + 1).saturating_sub(shown);
    let max_width = usize::from(width.saturating_sub(2));
    let mut lines: Vec<Line<'static>> = entries
        .iter()
        .zip(&tags)
        .enumerate()
        .skip(top)
        .take(shown)
        .map(|(at, (entry, tag))| {
            let is_on = on.as_deref() == Some(entry.preset.name.as_str());
            entry_line(entry, tag, is_on, at == row, &columns, theme, max_width)
        })
        .collect();
    lines.push(Line::default());
    lines.extend(keys);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    let title = format!(" Presets · {} bands ", eq.bands.len());
    let mut block = widgets::surfaces::modal_block(Block::bordered(), &title, app, theme);
    if top > 0 {
        block = block.title(Line::from(MORE_UP).right_aligned());
    }
    if top + shown < entries.len() {
        block = block.title_bottom(Line::from(MORE_DOWN).right_aligned());
    }
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    if !entries.is_empty() {
        let inner = Block::bordered().inner(box_area);
        let at = Rect {
            y: inner.y + u16::try_from(row - top).unwrap_or(0),
            height: 1,
            ..inner
        };
        hits.select(Selected::Presets, row, at.intersection(inner));
    }
    widgets::surfaces::fade_list(
        frame.buffer_mut(),
        app.list_fade,
        Block::bordered().inner(box_area),
        top,
        shown,
        row,
        theme,
    );
    box_area
}

/// One preset's row: the cursor, `●` for the one that is on, its name cut to the name column,
/// and its tags. The tags are held back in the faint grey, but on the focused row, which is drawn
/// whole in the selected role; `computer` takes the accent.
fn entry_line(
    entry: &Entry,
    tag: &Tags,
    on: bool,
    focused: bool,
    columns: &Columns,
    theme: &Theme,
    max_width: usize,
) -> Line<'static> {
    let mut spans = vec![
        match on {
            true => Span::styled(ON, theme.style(Role::Good)),
            false => Span::raw(" "),
        },
        Span::raw(" "),
    ];
    let name = widgets::text::cut_to_cells(&entry.preset.name, columns.name);
    let faint = |text: String| match focused {
        true => Span::raw(text),
        false => Span::styled(text, theme.style(Role::Faint)),
    };
    // The columns this row has, each padded to its width when another follows it.
    let mut cells: Vec<(String, usize, Option<Style>)> = Vec::new();
    cells.push((tag.mark.to_string(), columns.mark, None));
    if columns.fit > 0 {
        cells.push((tag.fit.clone().unwrap_or_default(), columns.fit, None));
    }
    if tag.computer {
        let accent = match focused {
            true => Style::default(),
            false => theme.style(Role::Rule),
        };
        cells.push((COMPUTER.to_string(), columns.computer, Some(accent)));
    }
    while cells.last().is_some_and(|(text, _, _)| text.is_empty()) {
        cells.pop();
    }
    match cells.is_empty() {
        true => spans.push(Span::raw(name)),
        false => {
            spans.push(Span::raw(widgets::text::pad_cells(&name, columns.name)));
            let last = cells.len() - 1;
            for (at, (text, width, style)) in cells.into_iter().enumerate() {
                spans.push(Span::raw(" ".repeat(GAP)));
                let text = match at == last {
                    true => text,
                    false => widgets::text::pad_cells(&text, width),
                };
                spans.push(match style {
                    Some(style) => Span::styled(text, style),
                    None => faint(text),
                });
            }
        }
    }
    row_style(
        widgets::rows::with_cursor(Line::from(spans), focused, theme),
        focused,
        false,
        theme,
        max_width,
    )
}

/// The keys that change the focused preset: as every modal box names its keys, or, on a
/// built-in, all of them in the faint grey with the reason they do nothing after them.
fn edit_keys(read_only: bool, theme: &Theme) -> Line<'static> {
    if !read_only {
        return widgets::surfaces::modal_keys(&EDIT_KEYS, theme);
    }
    let faint = theme.style(Role::Faint);
    let mut spans = vec![Span::raw(" ")];
    for (at, (key, does)) in EDIT_KEYS.iter().enumerate() {
        if at > 0 {
            spans.push(Span::styled(" · ", faint));
        }
        spans.push(Span::styled(*key, faint));
        spans.push(Span::styled(format!(" {does}"), faint));
    }
    spans.push(Span::styled(format!(" · {READ_ONLY}"), faint));
    Line::from(spans)
}

/// Draw the name prompt over `area`: the name as it has been typed, with the terminal's own
/// cursor where the next letter goes, and the two keys that end it. A name longer than the field
/// scrolls through it with the cursor.
fn draw_prompt(frame: &mut Frame, area: Rect, app: &App, line: &NameLine, theme: &Theme) -> Rect {
    let box_area = boxed(area, PROMPT_WIDTH, PROMPT_HEIGHT);
    // The field is what the box's borders, the blank the line opens with and the label leave;
    // the title has the top border between its two corners.
    let across = usize::from(box_area.width.saturating_sub(2));
    let room = across.saturating_sub(1 + NAME_FIELD);
    let (title, keys) = match line.renaming.as_deref() {
        Some(old) => (
            format!(
                " Rename {} ",
                widgets::text::cut_to_cells(old, across.saturating_sub(TITLE_WORD))
            ),
            RENAME_KEYS,
        ),
        None => (" New preset ".to_string(), SAVE_KEYS),
    };
    let (shown, column) = widgets::text::typed_window(&line.text, line.caret, room);
    let lines = vec![
        Line::from(vec![
            Span::raw(" "),
            widgets::rows::label(NAME, NAME_FIELD),
            Span::raw(shown),
        ]),
        Line::default(),
        widgets::surfaces::modal_keys(&keys, theme),
    ];
    let block = widgets::surfaces::modal_block(Block::bordered(), &title, app, theme);
    let inner = block.inner(box_area);
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    let x = inner.x + u16::try_from(1 + NAME_FIELD + column).unwrap_or(u16::MAX);
    if x < inner.right() && inner.height > 0 {
        frame.set_cursor_position(Position { x, y: inner.y });
    }
    box_area
}

/// Draw the question an update or a deletion asks over `area`: which preset it is about, and the
/// keys that answer it. The box is as wide as the question, and the question stays whole: a name
/// too long for the room is cut short inside it.
fn draw_question(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    question: &Question,
    theme: &Theme,
) -> Rect {
    let (title, before, name, after, keys) = match question {
        Question::Update(name) => (
            " Update ",
            " Update ",
            name,
            " with the curve on the device? [y/N]",
            UPDATE_KEYS,
        ),
        Question::Delete(name) => (
            " Delete ",
            " Delete ",
            name,
            " from the configuration? [y/N]",
            DELETE_KEYS,
        ),
        Question::MoveToComputer(name) => (
            " Move ",
            " ",
            name,
            " is on the DAC. Move it to the computer? [y/N]",
            MOVE_KEYS,
        ),
        Question::MoveToDac(name) => (
            " Move ",
            " ",
            name,
            " is on the computer. Move it to the DAC? [y/N]",
            MOVE_KEYS,
        ),
    };
    let room = usize::from(area.width)
        .saturating_sub(2 + TRAIL + widgets::text::cells(before) + widgets::text::cells(after));
    let text = format!("{before}{}{after}", widgets::text::cut_to_cells(name, room));
    let lines = vec![
        Line::from(Span::raw(text)),
        Line::default(),
        widgets::surfaces::modal_keys(&keys, theme),
    ];
    let widest = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(widest + TRAIL + 2).unwrap_or(u16::MAX);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(
        Paragraph::new(lines).block(widgets::surfaces::modal_block(
            Block::bordered(),
            title,
            app,
            theme,
        )),
        box_area,
    );
    box_area
}

/// Draw the search box over `area`: the query line, the measurements it finds one to a row with
/// the recommended one of each headphone first, the status line, and the keys. A list longer than
/// the box holds shows the rows round the focused one and says on its border which way the rest
/// are. The terminal's own cursor sits at the end of the query.
fn draw_search(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    search: &SearchBox,
    (theme, hits): (&Theme, &mut HitMap),
) -> Rect {
    // The query, a blank, the rows, the status and the keys between two borders.
    let rows_room = usize::from(area.height.saturating_sub(6)).clamp(1, SEARCH_ROWS);
    let height = u16::try_from(rows_room + 6).unwrap_or(u16::MAX);
    let box_area = boxed(area, SEARCH_WIDTH, height);
    let inner = Block::bordered().inner(box_area);
    let across = usize::from(inner.width);
    let row = search.row.min(search.rows.len().saturating_sub(1));
    let shown = search.rows.len().min(rows_room);
    let top = match shown {
        0 => 0,
        _ => (row + 1).saturating_sub(shown),
    };
    let visible = &search.rows[top..top + shown];
    let sources: Vec<String> = visible.iter().map(search::measurement).collect();
    let source_width = sources
        .iter()
        .map(|text| widgets::text::cells(text))
        .max()
        .unwrap_or(0);
    let name_width = visible
        .iter()
        .map(|entry| widgets::text::cells(&entry.name))
        .max()
        .unwrap_or(0)
        .min(across.saturating_sub(CURSOR_WIDTH + TRAIL + GAP + source_width));
    let field = across.saturating_sub(1 + widgets::text::cells(SEARCH_LABEL) + GAP);
    let (typed, column) =
        widgets::text::typed_window(&search.query, search.query.chars().count(), field);
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled(SEARCH_LABEL, theme.style(Role::Dim)),
            Span::raw(" ".repeat(GAP)),
            Span::raw(typed),
        ]),
        Line::default(),
    ];
    for (at, (entry, source)) in visible.iter().zip(&sources).enumerate() {
        let at = top + at;
        let focused = at == row;
        let first = at == 0 || search.rows[at - 1].name != entry.name;
        let name = widgets::text::cut_to_cells(&entry.name, name_width);
        let name = widgets::text::pad_cells(&name, name_width);
        let line = Line::from(vec![
            match first || focused {
                true => Span::raw(name),
                false => Span::styled(name, theme.style(Role::Faint)),
            },
            Span::raw(" ".repeat(GAP)),
            match focused {
                true => Span::raw(source.clone()),
                false => Span::styled(source.clone(), theme.style(Role::Dim)),
            },
        ]);
        lines.push(row_style(
            widgets::rows::with_cursor(line, focused, theme),
            focused,
            false,
            theme,
            across,
        ));
    }
    let hint = match (search.query.trim().is_empty(), search.rows.is_empty()) {
        (true, _) => Some(SEARCH_HINT),
        (false, true) if matches!(search.load, search::Load::Ready) => Some(SEARCH_NONE),
        _ => None,
    };
    if let Some(hint) = hint {
        lines.push(Line::from(Span::styled(
            format!("   {hint}"),
            theme.style(Role::Faint),
        )));
    }
    // The status line sits where the rows end, at a fixed place: the rows' room is padded out.
    while lines.len() < 2 + rows_room {
        lines.push(Line::default());
    }
    let status = search::status(search, app.presets.autoeq.as_deref());
    let role = match search.load {
        search::Load::Failed(_) => Role::Warn,
        _ => Role::Dim,
    };
    lines.push(Line::from(Span::styled(
        format!(
            " {}",
            widgets::text::cut_to_cells(&status, across.saturating_sub(1 + TRAIL))
        ),
        theme.style(role),
    )));
    lines.push(widgets::surfaces::modal_keys(&SEARCH_KEYS, theme));
    let mut block = widgets::surfaces::modal_block(Block::bordered(), " AutoEq ", app, theme);
    if top > 0 {
        block = block.title(Line::from(MORE_UP).right_aligned());
    }
    if top + shown < search.rows.len() {
        block = block.title_bottom(Line::from(MORE_DOWN).right_aligned());
    }
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    if shown > 0 {
        // The query and a blank line come before the first result.
        let at = Rect {
            y: inner.y + 2 + u16::try_from(row - top).unwrap_or(0),
            height: 1,
            ..inner
        };
        hits.select(Selected::Search, row, at.intersection(inner));
    }
    let x = inner.x
        + u16::try_from(1 + widgets::text::cells(SEARCH_LABEL) + GAP + column).unwrap_or(u16::MAX);
    if x < inner.right() && inner.height > 0 {
        frame.set_cursor_position(Position { x, y: inner.y });
    }
    box_area
}

/// Draw the file prompt over `area`: the path as typed with the cursor at its end, the candidates
/// the last `Tab` found (six, then `… N more`) or what is wrong under it, and the keys.
fn draw_file(frame: &mut Frame, area: Rect, app: &App, prompt: &FilePrompt, theme: &Theme) -> Rect {
    let listing = prompt.listing();
    let listed = listing
        .as_ref()
        .map_or(0, |(shown, more, _)| shown.len() + usize::from(*more > 0));
    let height =
        u16::try_from(PROMPT_HEIGHT as usize + listed.max(usize::from(prompt.note.is_some())))
            .unwrap_or(u16::MAX);
    let box_area = boxed(area, PROMPT_WIDTH, height);
    let inner = Block::bordered().inner(box_area);
    let across = usize::from(inner.width);
    let room = across.saturating_sub(1 + FILE_FIELD);
    let end = prompt.text.chars().count();
    let (typed, column) = widgets::text::typed_window(&prompt.text, end, room);
    let mut lines = vec![Line::from(vec![
        Span::raw(" "),
        widgets::rows::label(FILE, FILE_FIELD),
        Span::raw(typed),
    ])];
    if let Some((shown, more, at)) = &listing {
        for (index, name) in shown.iter().enumerate() {
            let focused = *at == Some(index);
            let line = Line::from(format!(
                "{}{}",
                " ".repeat(1 + FILE_FIELD),
                widgets::text::cut_to_cells(name, across.saturating_sub(1 + FILE_FIELD))
            ));
            lines.push(row_style(line, focused, false, theme, across));
        }
        if *more > 0 {
            lines.push(Line::from(Span::styled(
                format!("{}… {more} more", " ".repeat(1 + FILE_FIELD)),
                theme.style(Role::Faint),
            )));
        }
    } else if let Some(note) = &prompt.note {
        lines.push(Line::from(Span::styled(
            format!(
                " {}",
                widgets::text::cut_to_cells(note, across.saturating_sub(1 + TRAIL))
            ),
            theme.style(Role::Warn),
        )));
    }
    lines.push(Line::default());
    lines.push(widgets::surfaces::modal_keys(&FILE_KEYS, theme));
    let block = widgets::surfaces::modal_block(Block::bordered(), " Import a file ", app, theme);
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    let x = inner.x + u16::try_from(1 + FILE_FIELD + column).unwrap_or(u16::MAX);
    if x < inner.right() && inner.height > 0 {
        frame.set_cursor_position(Position { x, y: inner.y });
    }
    box_area
}

#[cfg(test)]
mod tests;
