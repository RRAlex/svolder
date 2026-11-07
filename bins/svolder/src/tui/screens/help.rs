// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The help overlay: every key of every screen, over whatever screen is showing.
//!
//! The overlay is drawn from the key table ([`keys`]), so a key is named here exactly as the
//! screen that takes it names it, and a key that is not worth pressing now is dimmed with the
//! reason rather than left out.
//!
//! Each key is named once. The three rules open the box, the keys more than one screen takes
//! follow under one heading, and the rest is a section per screen holding what that screen
//! alone does — so the length of the overlay is the number of keys there are rather than the
//! number of screens that list them.
//!
//! The sections are laid out down two columns or more, each column filled before the next and
//! the columns kept about as long as one another. A section stays whole in one column unless it
//! is longer than a column, and a heading always has the first of its keys under it.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph};

use crate::tui::app::{App, Screen};
use crate::tui::keys::{self, KeyHint, Scene, When};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;
use crate::tui::widgets::scroll::{MORE_DOWN, MORE_UP};

/// The screens the overlay lists, in the order it lists them after the one being shown.
const SCREENS: [Screen; 5] = [
    Screen::Device,
    Screen::Tone,
    Screen::Telemetry,
    Screen::Devices,
    Screen::Settings,
];

/// The three rules every key in the table follows, which the overlay opens with: a reader who
/// knows these can guess a key rather than look it up.
const RULES: [&str; 3] = [
    "Lowercase letters act on the device, or open a screen or the Settings box.",
    "Uppercase letters toggle a TUI mode and save it to the config.",
    "Symbols and navigation keys are the same on every screen.",
];

/// What the overlay calls the section holding the keys that work on every screen.
const EVERYWHERE: &str = "Everywhere";

/// The one key the overlay names that the key table does not: Ctrl-C, which quits at once from
/// every screen, past every question. It is a line the overlay reads out rather than a key a
/// key press names — the table holds keys without a modifier, and a bare `c` is a key of its
/// own on the device list.
const CTRL_C: (&str, &str) = ("Ctrl-C", "quit at once");

/// What the mouse does on the tone-control screen, as the overlay lists it after that screen's
/// own keys: gestures rather than keys, which the key table does not hold.
pub(crate) const TONE_GESTURES: [(&str, &str); 4] = [
    ("L-drag", "point freq/gain"),
    ("R-drag", "Q, right widens"),
    ("wheel", "Q on the curve"),
    ("2×click", "edit band value"),
];

/// Columns between one column of keys and the next.
const COLUMN_GAP: usize = 2;
/// The fewest columns of keys the box is laid out in wherever that many fit across it.
const MIN_COLUMNS: usize = 2;
/// Cells of the screen underneath the box leaves in sight on every side.
const MARGIN: u16 = 1;
/// Blank cells inside the box's border on the left and the right, so no text touches it.
const PADDING: u16 = 1;
/// Columns between a key's label and what it does.
const LABEL_GAP: usize = 2;
/// What the box says it is, and how to leave it.
const TITLE: &str = " Keys · ? or Esc closes ";

/// Draw the overlay over `area` and answer the box it drew in.
///
/// The box holds every key the TUI reads (the ones every screen takes, then each screen's own,
/// with the screen being shown first), laid out in the columns [`view`] works out, and is only
/// as large as those columns need: it keeps [`MARGIN`] cells of the screen underneath in sight
/// on every side, all of it dimmed so the box is what reads, and is drawn in the theme's own
/// colours as a focused pane is (border and title in the pane's, rules and what each key does
/// in the primary text, keys in the accent, a key that does nothing here in the faint grey its
/// reason explains). A box that cannot hold every key at once is scrolled with the arrows, the
/// page keys and `Home`/`End`, and says so on the border it has keys past.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) -> Rect {
    let view = view(area, app);
    let at = app.overlays.help_scroll.min(view.most);
    let mut lines: Vec<Line<'static>> = view
        .rules
        .iter()
        .map(|rule| Line::from(Span::styled(rule.clone(), theme.style(Role::Text))))
        .collect();
    lines.push(Line::default());
    lines.extend((0..view.rows).map(|row| {
        let spans = view
            .columns
            .iter()
            .flat_map(|column| match shown(column, at, row, view.rows) {
                Some(entry) => entry.spans(theme, view.label, view.column),
                None => vec![Span::raw(" ".repeat(view.column))],
            })
            .collect::<Vec<Span<'static>>>();
        Line::from(spans)
    }));
    let mut block = widgets::surfaces::modal_block(boxed(), TITLE, app, theme);
    if at > 0 {
        block = block.title(Line::from(MORE_UP).right_aligned());
    }
    if at < view.most {
        block = block.title_bottom(Line::from(MORE_DOWN).right_aligned());
    }
    widgets::surfaces::modal_ground(frame, view.area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), view.area);
    view.area
}

/// The entry `column` shows on row `row` of a window `rows` tall scrolled `at` rows down, or
/// nothing for a row past the column's end.
///
/// A heading on the window's last row is left blank: its first key is below the window, and
/// a heading with nothing under it would read as a section with no keys. It comes back on the
/// row the scroll brings it to.
fn shown(column: &[Entry], at: usize, row: usize, rows: usize) -> Option<&Entry> {
    let entry = column.get(at + row)?;
    match entry {
        Entry::Heading(_) if row + 1 == rows && column.get(at + row + 1).is_some() => None,
        _ => Some(entry),
    }
}

/// The box's border and the [`PADDING`] inside it.
fn boxed() -> Block<'static> {
    Block::bordered().padding(Padding::horizontal(PADDING))
}

/// How far the overlay scrolls on the terminal `app` was last drawn in: one page, and the
/// furthest it goes.
///
/// The keys that move through the overlay ask this, so what a page is and where the end is are
/// the very numbers the frame is laid out from.
pub fn scroll_of(app: &App) -> (usize, usize) {
    let area = Rect {
        x: 0,
        y: 0,
        width: app.size.0,
        height: app.size.1,
    };
    let view = view(area, app);
    (view.rows, view.most)
}

/// How one frame of the overlay is laid out.
struct View {
    /// The box itself, borders and all.
    area: Rect,
    /// The three rules as the box's width wraps them.
    rules: Vec<String>,
    /// The entries each column of keys holds, top to bottom, the ones past the box included.
    columns: Vec<Vec<Entry>>,
    /// Cells the label column takes.
    label: usize,
    /// Cells one column of keys takes, the gap after it included.
    column: usize,
    /// How many rows of each column are on the screen.
    rows: usize,
    /// The furthest the box scrolls: how many rows its longest column holds past the ones it
    /// shows.
    most: usize,
}

/// How the overlay is laid out over `area`: the box, the columns of keys inside it, and how
/// far they run past it.
///
/// The keys take [`MIN_COLUMNS`] columns wherever that many fit across the room the
/// [`MARGIN`] leaves, and more only where the height needs them to hold every key; the
/// columns are as short as [`pack`] can make them. What still does not fit is what the box
/// scrolls through, in as many columns as its width takes.
fn view(area: Rect, app: &App) -> View {
    let sections = sections(app);
    let lead = lead(app, &sections);
    let entries = sections.iter().flatten();
    let label = entries.clone().map(Entry::label_width).max().unwrap_or(0);
    let column = entries.map(|entry| entry.width(label)).max().unwrap_or(0) + COLUMN_GAP;
    let room = Rect {
        x: area.x + MARGIN,
        y: area.y + MARGIN,
        width: area.width.saturating_sub(2 * MARGIN),
        height: area.height.saturating_sub(2 * MARGIN),
    };
    let chrome = usize::from(2 + 2 * PADDING);
    let text_room = usize::from(room.width).saturating_sub(chrome);
    let most_columns = ((text_room + COLUMN_GAP) / column.max(1)).max(1);
    let widest = RULES
        .iter()
        .map(|rule| widgets::text::cells(rule))
        .max()
        .unwrap_or(0);
    // The rules and the blank line under them take the top of the box, so the keys have the
    // rest of it: they are read once and the keys are read every time. How many lines the
    // rules wrap onto depends on how wide the box is, which depends on how many columns of
    // keys it holds, so each count of columns is weighed with its own.
    let layout = |columns: usize| {
        let keys = (columns * column).saturating_sub(COLUMN_GAP);
        let width = (keys.max(widest) + chrome).min(usize::from(room.width));
        let rules: Vec<String> = RULES
            .iter()
            .flat_map(|rule| widgets::text::wrap(rule, width.saturating_sub(chrome).max(1)))
            .collect();
        let room_rows = usize::from(room.height).saturating_sub(2 + rules.len() + 1);
        let height = fit(&sections, columns, lead);
        (width, rules, room_rows, height)
    };
    let fewest = MIN_COLUMNS.min(most_columns);
    let columns = (fewest..=most_columns)
        .find(|columns| {
            let (_, _, room_rows, height) = layout(*columns);
            height <= room_rows
        })
        .unwrap_or(most_columns);
    let (width, rules, _, height) = layout(columns);
    let said = rules.len() + 1;
    let box_area = centred(
        area,
        u16::try_from(width).unwrap_or(room.width),
        u16::try_from(height + 2 + said)
            .unwrap_or(room.height)
            .min(room.height),
    );
    let rows = usize::from(boxed().inner(box_area).height).saturating_sub(said);
    let columns = pack(&sections, height, if columns > 1 { lead } else { 0 });
    View {
        area: box_area,
        rules,
        columns,
        label,
        column,
        rows,
        most: height.saturating_sub(rows),
    }
}

/// How tall the columns are when `sections` are packed into `columns` of them: the shortest
/// height [`pack`] fits them into that many columns at.
fn fit(sections: &[Vec<Entry>], columns: usize, lead: usize) -> usize {
    let lines: usize =
        sections.iter().map(Vec::len).sum::<usize>() + sections.len().saturating_sub(1);
    let fewest = lines.div_ceil(columns.max(1)).max(2);
    (fewest..lines.max(fewest))
        .find(|height| pack(sections, *height, if columns > 1 { lead } else { 0 }).len() <= columns)
        .unwrap_or(lines.max(fewest))
}

/// `sections` laid out down columns `height` entries tall, each filled before the next.
///
/// A section follows the one before it in the same column, a blank line between them, where
/// the column has room for the whole of it, and starts the next column where it has not. Only a
/// section longer than a column is split: it opens where the column still has room for its
/// heading and its first key, and runs on down the columns after it. So no column ends on a
/// heading, a heading always has its first key under it, and no column opens on a blank line.
/// The `lead` sections first (0 for no rule) fill the first column alone: the section after them
/// always opens a new column.
fn pack(sections: &[Vec<Entry>], height: usize, lead: usize) -> Vec<Vec<Entry>> {
    let height = height.max(2);
    let mut columns: Vec<Vec<Entry>> = vec![Vec::new()];
    for (at, section) in sections.iter().enumerate() {
        // The section after the lead ones opens a column of its own, so the screen showing heads
        // the next column rather than trailing the shared keys.
        if lead > 0 && at == lead && !columns[columns.len() - 1].is_empty() {
            columns.push(Vec::new());
        }
        let last = columns.len() - 1;
        let taken = columns[last].len();
        let gap = usize::from(taken > 0);
        if taken + gap + section.len() <= height {
            if gap > 0 {
                columns[last].push(Entry::Gap);
            }
            columns[last].extend(section.iter().cloned());
            continue;
        }
        if section.len() <= height {
            columns.push(section.clone());
            continue;
        }
        match taken {
            0 => {}
            _ if taken + 1 + 2 > height => columns.push(Vec::new()),
            _ => columns[last].push(Entry::Gap),
        }
        for entry in section {
            if columns.last().is_some_and(|column| column.len() >= height) {
                columns.push(Vec::new());
            }
            if let Some(column) = columns.last_mut() {
                column.push(entry.clone());
            }
        }
    }
    columns
}

/// A box `width` by `height` in the middle of `area`.
fn centred(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

/// One line of the overlay: a screen's name, or one of its keys.
#[derive(Debug, Clone)]
enum Entry {
    /// The screen whose keys follow.
    Heading(&'static str),
    /// One key: what it reads as, what it does, and why it is not worth pressing now.
    Key {
        /// The key's own label, as the table writes it.
        label: &'static str,
        /// What the key does.
        does: String,
        /// Why the key is not worth pressing now, for the keys that are conditional.
        reason: Option<&'static str>,
    },
    /// The gap between one screen's keys and the next screen's name.
    Gap,
}

impl Entry {
    /// How many cells the entry's own label takes, which is what the label column is as wide
    /// as.
    fn label_width(&self) -> usize {
        match self {
            Entry::Key { label, .. } => widgets::text::cells(label),
            Entry::Heading(_) | Entry::Gap => 0,
        }
    }

    /// How many cells the entry takes with its label drawn in a column `label` cells wide.
    fn width(&self, label: usize) -> usize {
        match self {
            Entry::Heading(name) => widgets::text::cells(name),
            Entry::Key { does, reason, .. } => {
                label
                    + LABEL_GAP
                    + widgets::text::cells(does)
                    + reason.map_or(0, |why| {
                        widgets::text::cells(why) + widgets::text::cells(REASON_GAP)
                    })
            }
            Entry::Gap => 0,
        }
    }

    /// The entry as it is drawn: its label in a column `label` cells wide so that every
    /// description starts in the same place, padded to `column` cells so that the next column
    /// of keys does too.
    fn spans(&self, theme: &Theme, label: usize, column: usize) -> Vec<Span<'static>> {
        let pad = column.saturating_sub(self.width(label));
        let mut spans = match self {
            Entry::Heading(name) => {
                vec![Span::styled(name.to_string(), theme.style(Role::Header))]
            }
            Entry::Key {
                label: key_label,
                does,
                reason,
            } => {
                let (key, text) = match reason {
                    None => (Role::KeyHint, Role::Text),
                    Some(_) => (Role::Faint, Role::Faint),
                };
                let mut spans = vec![
                    Span::styled(format!("{key_label:<label$}"), theme.style(key)),
                    Span::raw(" ".repeat(LABEL_GAP)),
                    Span::styled(does.clone(), theme.style(text)),
                ];
                if let Some(why) = reason {
                    spans.push(Span::styled(
                        format!("{REASON_GAP}{why}"),
                        theme.style(Role::Faint),
                    ));
                }
                spans
            }
            Entry::Gap => Vec::new(),
        };
        spans.push(Span::raw(" ".repeat(pad)));
        spans
    }
}

/// What separates a key's description from the reason it is not worth pressing.
const REASON_GAP: &str = " · ";

/// Every key the TUI reads, each named once, as the sections the overlay lays out: the keys
/// more than one screen takes under [`EVERYWHERE`], Ctrl-C last among them, then a section per
/// screen — Telemetry right after Everywhere, then the screen being shown (the Settings box,
/// while it is open), the rest in their fixed order — holding that screen's
/// own keys and nothing it has already said. Every section opens on its heading.
///
/// A screen with no key of its own gets no section, and a key two sections would name the same
/// way is left to the first of them: the arrows move and change on the tone-control screen as
/// they do everywhere else, so "Everywhere" is where they are read. The tone-control screen's
/// section ends on what the mouse does there ([`TONE_GESTURES`]).
fn sections(app: &App) -> Vec<Vec<Entry>> {
    let mut listed: Vec<(&'static str, &'static str)> = Vec::new();
    let mut everywhere = vec![Entry::Heading(EVERYWHERE)];
    for hint in keys::shared() {
        if keys::hidden_in(hint, Scene::of(app)) {
            continue;
        }
        listed.push((hint.label, hint.does));
        everywhere.push(entry(hint, app));
    }
    everywhere.push(Entry::Key {
        label: CTRL_C.0,
        does: CTRL_C.1.to_string(),
        reason: None,
    });
    let mut sections = vec![everywhere];
    // The Settings box heads the sections while it is open, over whichever screen is under it.
    let first = if app.overlays.settings.is_some() {
        Screen::Settings
    } else {
        app.screen
    };
    // Telemetry follows Everywhere unless it is the section heading the sections already.
    let telemetry_early = first != Screen::Telemetry;
    let order = telemetry_early
        .then_some(Screen::Telemetry)
        .into_iter()
        .chain(std::iter::once(first))
        .chain(
            SCREENS
                .into_iter()
                .filter(|s| *s != first && !(telemetry_early && *s == Screen::Telemetry)),
        );
    for screen in order {
        let mut own = vec![Entry::Heading(name(screen))];
        for hint in keys::only_on(screen) {
            if listed.contains(&(hint.label, hint.does)) || keys::hidden_in(hint, Scene::of(app)) {
                continue;
            }
            // The token row's keys are the Connect category's: named only while the box is open.
            if hint.when == keys::When::TokenRow && app.overlays.settings.is_none() {
                continue;
            }
            listed.push((hint.label, hint.does));
            own.push(entry(hint, app));
        }
        if screen == Screen::Tone {
            own.extend(gestures(app));
        }
        if own.len() > 1 {
            sections.push(own);
        }
    }
    sections
}

/// How many leading sections fill the first column alone: Everywhere and, when it is not the
/// screen being shown, Telemetry after it; the shown screen then heads the next column.
fn lead(app: &App, sections: &[Vec<Entry>]) -> usize {
    let telemetry_second = matches!(
        sections.get(1).and_then(|section| section.first()),
        Some(Entry::Heading(heading)) if *heading == name(Screen::Telemetry)
    );
    if telemetry_second && (app.overlays.settings.is_some() || app.screen != Screen::Telemetry) {
        2
    } else {
        1
    }
}

/// What the mouse does on the tone-control screen ([`TONE_GESTURES`]), each gesture with the
/// reason it does nothing on `app` now where there is one: a device with no tone control has no
/// such screen, and with mouse capture off the terminal keeps the mouse to itself.
fn gestures(app: &App) -> Vec<Entry> {
    let reason = match (app.profile.eq.is_some(), app.mouse) {
        (false, _) => Some("needs an EQ"),
        (true, false) => Some("mouse capture off"),
        (true, true) => None,
    };
    TONE_GESTURES
        .iter()
        .map(|&(label, does)| Entry::Key {
            label,
            does: does.to_string(),
            reason,
        })
        .collect()
}

/// One key of the table as the overlay lists it, with the reason it is not worth pressing on
/// `app` now where there is one.
fn entry(hint: &KeyHint, app: &App) -> Entry {
    Entry::Key {
        label: hint.label_in(Scene::of(app)),
        does: does(hint, app),
        reason: reason(hint, app),
    }
}

/// What the overlay says a key does on `app`: the key table's own words as the screen says them,
/// but for the uppercase keys, which name the mode they turn as the settings row that shows it
/// does — the rules above say once that an uppercase key toggles.
fn does(hint: &KeyHint, app: &App) -> String {
    match hint.toggles {
        Some(setting) => {
            let name = setting.label();
            let mut chars = name.chars();
            match chars.next() {
                Some(first) => first.to_lowercase().chain(chars).collect(),
                None => String::new(),
            }
        }
        None => hint.long_in(Scene::of(app)).to_string(),
    }
}

/// The name the overlay gives one screen's section: what the screen is for rather than what it
/// is called, so the device screen and the device list read as two different things.
fn name(screen: Screen) -> &'static str {
    match screen {
        Screen::Device => "Controls",
        Screen::Tone => "EQ",
        Screen::Telemetry => "Telemetry",
        Screen::Devices => "Devices",
        Screen::Settings => "Settings",
        Screen::Setup => "Setup",
    }
}

/// Why `hint` is not worth pressing on `app` now, and nothing at all for a key that is.
fn reason(hint: &KeyHint, app: &App) -> Option<&'static str> {
    if keys::available(hint, app) {
        return None;
    }
    Some(match hint.when {
        // `available` is true for `Always`, so the early return above has already taken it.
        When::Always => return None,
        When::HasEq => "needs an EQ",
        When::HasPresets => "needs presets",
        When::HasMute => "needs a mute",
        When::HasPhase => "needs a phase switch",
        When::HasNos => "needs a NOS switch",
        When::HasBalance => "needs a balance",
        When::HasVolume => "needs a volume",
        When::HasToggle => "needs a toggle",
        When::CanIdentify => "this device offers no identify",
        When::HasComputer => "needs the computer's controls",
        When::OnComputer => "on the computer side only",
        When::Choosing => "not while typing",
        When::Typing => "only while typing",
        When::Backable => "nothing to go back to",
        When::Scrolling => "nothing to scroll",
        When::TokenRow => "token row only",
    })
}

#[cfg(test)]
mod tests;
