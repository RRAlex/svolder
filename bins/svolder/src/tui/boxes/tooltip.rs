// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tooltip: a pointer resting on anything clickable says what it is and how to use it.
//!
//! The pointer's motion arrives as `Moved` events, which [`hover`] reads against the last
//! frame's hits; a target the pointer has rested on for [`DELAY`] shows its [`text`] in a box
//! under the pointer's cell. The tooltip belongs to the pointer alone: a key, a click, the
//! wheel, a drag, a resize, a box opening, mouse capture turning off or the target leaving its
//! cell hides it.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph};
use svolder_host::Button;
use svolder_profiles::{Control, ControlKind};

use crate::tui::app::{App, Pane, Screen, Xf};
use crate::tui::host::HostRow;
use crate::tui::keys;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::screens::telemetry;
use crate::tui::theme::{Role, Surface, Theme};
use crate::tui::widgets;

/// How long the pointer rests on one target before its tooltip shows.
pub const DELAY: Duration = Duration::from_millis(700);

/// The longest the loop waits between ticks while a tooltip is pending, so it shows within a
/// tick of its delay.
pub const PENDING_TICK: Duration = Duration::from_millis(100);

/// The most cells a line of a tooltip takes.
pub const MAX_WIDTH: usize = 40;

/// A tooltip pending or showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tooltip {
    /// The target the pointer rests on.
    pub hit: Hit,
    /// The cell the box goes under: where the pointer was when the target was reached, moved
    /// along while the tooltip is pending and the pointer stays on the target.
    pub at: (u16, u16),
    /// When the pointer reached the target.
    pub since: Instant,
    /// Whether the delay has passed and the box is drawn.
    pub shown: bool,
}

impl Tooltip {
    /// Show the tooltip if the pointer has rested on its target for [`DELAY`] by `now`.
    pub fn age(&mut self, now: Instant) {
        if now.saturating_duration_since(self.since) >= DELAY {
            self.shown = true;
        }
    }
}

/// Read one mouse event at `now` against the hits of the last frame.
///
/// A `Moved` event over a target that has a tooltip starts one pending, or keeps the one that
/// is there while the target is the same, or — while one shows — shows the new target's at once.
/// Over nothing, or with a box open or mouse capture off, it hides it. Every other event (a
/// press, a release, a drag, the wheel) hides it.
pub fn hover(app: &mut App, hits: &HitMap, event: &MouseEvent, now: Instant) {
    let held = app.pointer.tooltip;
    app.pointer.tooltip = hovered(held, app.tips_allowed(), hits, event, now, &|hit| {
        text(hit, app)
    });
}

/// [`hover`] for any front end: the tooltip `held` before `event`, whether tooltips are
/// `allowed`, and `said`, which gives the words for a target (or `None` where it has none).
/// Returns the tooltip held after the event.
pub fn hovered(
    held: Option<Tooltip>,
    allowed: bool,
    hits: &HitMap,
    event: &MouseEvent,
    now: Instant,
    said: &dyn Fn(Hit) -> Option<Vec<String>>,
) -> Option<Tooltip> {
    let mut held = held;
    if let Some(tooltip) = held.as_mut() {
        tooltip.age(now);
    }
    if event.kind != MouseEventKind::Moved || !allowed {
        return None;
    }
    let at = (event.column, event.row);
    let (hit, words) = hits
        .hit_at(at.0, at.1)
        .copied()
        .and_then(|hit| Some((hit, said(hit)?)))?;
    let shown = match held {
        Some(prior) if said(prior.hit).as_ref() == Some(&words) => {
            return Some(match prior.shown {
                true => prior,
                false => Tooltip { hit, at, ..prior },
            });
        }
        Some(prior) => prior.shown,
        None => false,
    };
    Some(Tooltip {
        hit,
        at,
        since: now,
        shown,
    })
}

/// Hide the tooltip unless the frame just drawn found its target still under its cell
/// (`stands`, from [`standing`] on that frame).
pub fn settle(app: &mut App, stands: bool) {
    if !stands {
        app.pointer.tooltip = None;
    }
}

/// The text of the tooltip held, while its target is still under its cell in `hits` and nothing
/// forbids it: a box open, mouse capture off. The frame asks this once and both draws the lines
/// and keeps the tooltip on the answer.
pub(crate) fn standing(app: &App, hits: &HitMap) -> Option<Vec<String>> {
    standing_with(app.pointer.tooltip, app.tips_allowed(), hits, &|hit| {
        text(hit, app)
    })
}

/// [`standing`] for any front end: `held` is the tooltip held, `allowed` whether tooltips may
/// show, and `said` gives the words for a target.
pub(crate) fn standing_with(
    held: Option<Tooltip>,
    allowed: bool,
    hits: &HitMap,
    said: &dyn Fn(Hit) -> Option<Vec<String>>,
) -> Option<Vec<String>> {
    let tooltip = held?;
    if !allowed {
        return None;
    }
    let (x, y) = tooltip.at;
    let under = said(*hits.hit_at(x, y)?)?;
    (Some(&under) == said(tooltip.hit).as_ref()).then_some(under)
}

/// Where a tooltip of `lines` goes on a screen of `screen`'s size for a pointer at `at`: under
/// the pointer's cell, above it where it would pass the last row, and shifted left where it
/// would pass the right edge.
fn area(screen: Rect, at: (u16, u16), lines: &[String]) -> Rect {
    let (width, height) = size(screen, lines);
    let below = at.1.saturating_add(1);
    let y = match below.saturating_add(height) <= screen.bottom() {
        true => below,
        false => at.1.saturating_sub(height).max(screen.y),
    };
    boxed(screen, at.0, y, width, height)
}

/// Where the tooltip goes as [`area`] puts it, unless that meets `avoid` (the toast's box): then
/// on the pointer's other side, and nowhere when that side meets it too or has no room.
#[cfg(test)]
fn place(screen: Rect, at: (u16, u16), lines: &[String], avoid: Option<Rect>) -> Option<Rect> {
    place_clear(screen, at, lines, avoid, &|_| 0)
}

/// [`area`] that also keeps clear of what is read: `covered` counts the cells of a rectangle
/// that show a value. The box goes where [`area`] would unless that covers some; then on the
/// pointer's other side if that covers none, then at the same two heights pushed to the right
/// edge, and where every place covers some, on the one that covers the fewest (the first of them
/// on a tie). None of them may meet `avoid`.
fn place_clear(
    screen: Rect,
    at: (u16, u16),
    lines: &[String],
    avoid: Option<Rect>,
    covered: &dyn Fn(Rect) -> usize,
) -> Option<Rect> {
    let (width, height) = size(screen, lines);
    let preferred = area(screen, at, lines);
    let missing = |rect: &Rect| avoid.is_none_or(|avoid| !rect.intersects(avoid));
    let mut best: Option<(usize, Rect)> = None;
    let consider = |rect: Rect, best: &mut Option<(usize, Rect)>| {
        if missing(&rect) {
            let cost = covered(rect);
            if best.is_none_or(|(least, _)| cost < least) {
                *best = Some((cost, rect));
            }
        }
    };
    consider(preferred, &mut best);
    let other = other_side(screen, at, preferred, width, height);
    if let Some(other) = other {
        consider(other, &mut best);
    }
    if best.is_none() || best.is_some_and(|(cost, _)| cost == 0) {
        return best.map(|(_, rect)| rect);
    }
    // Every base place covers a value: try the right edge at both heights.
    let right = screen.right().saturating_sub(width);
    for rect in [Some(preferred), other].into_iter().flatten() {
        consider(boxed(screen, right, rect.y, width, height), &mut best);
    }
    best.map(|(_, rect)| rect)
}

/// The box on the pointer's other side than `preferred`, where there is room for the whole box.
fn other_side(
    screen: Rect,
    at: (u16, u16),
    preferred: Rect,
    width: u16,
    height: u16,
) -> Option<Rect> {
    match preferred.y > at.1 {
        // Drawn below: try above, where there is room for the whole box.
        true => {
            at.1.checked_sub(height)
                .filter(|y| *y >= screen.y)
                .map(|y| boxed(screen, at.0, y, width, height))
        }
        // Drawn above: try below.
        false => {
            let below = at.1.saturating_add(1);
            (below.saturating_add(height) <= screen.bottom())
                .then(|| boxed(screen, at.0, below, width, height))
        }
    }
}

/// The targets that show a value somebody reads: rows, levels, toggles, selects, list rows,
/// band and point fields, the Mode and Loudness halves and the transport buttons.
fn shows_value(hit: Hit) -> bool {
    matches!(
        hit,
        Hit::Row(..)
            | Hit::Level(_)
            | Hit::LevelBar(_)
            | Hit::ToggleOf(_)
            | Hit::SelectPrev(_)
            | Hit::SelectNext(_)
            | Hit::ListRow(_)
            | Hit::BandField(..)
            | Hit::PointField(..)
            | Hit::Mode
            | Hit::ModePrev
            | Hit::ModeNext
            | Hit::Loudness
            | Hit::LoudnessBar
            | Hit::Transport(_)
    )
}

/// How many cells of `rect` in `buffer` hold text inside a target that shows a value, leaving
/// out the targets the pointer at `at` is on.
fn cells_read(buffer: &Buffer, hits: &HitMap, at: (u16, u16), rect: Rect) -> usize {
    let rect = rect.intersection(buffer.area);
    let mut seen = std::collections::HashSet::new();
    for (area, hit) in hits.entries() {
        if !shows_value(hit) || area.contains(Position { x: at.0, y: at.1 }) {
            continue;
        }
        let part = area.intersection(rect);
        for y in part.top()..part.bottom() {
            for x in part.left()..part.right() {
                if !buffer[(x, y)].symbol().trim().is_empty() {
                    seen.insert((x, y));
                }
            }
        }
    }
    seen.len()
}

/// The cells a box of `lines` takes, at most the screen's.
fn size(screen: Rect, lines: &[String]) -> (u16, u16) {
    let widest = lines
        .iter()
        .map(|line| widgets::text::cells(line))
        .max()
        .unwrap_or(0);
    let width = u16::try_from(widest + 2)
        .unwrap_or(u16::MAX)
        .min(screen.width);
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .min(screen.height);
    (width, height)
}

/// A box of `width` by `height` with its top on row `y`, shifted left so it ends on the screen.
fn boxed(screen: Rect, left: u16, y: u16, width: u16, height: u16) -> Rect {
    let x = left.min(screen.right().saturating_sub(width)).max(screen.x);
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Draw `lines` as a tooltip for a pointer at `at`, clear of the values `hits` shows where a
/// place can be found (see [`place_clear`]) and over everything but `avoid` (the toast's box),
/// and return the box, or `None` where neither side of the pointer misses `avoid`.
///
/// The box takes the raised shade with the theme's primary text and no border; where no raised
/// shade is painted (16 colours, monochrome, the theme background off) it is reverse video.
pub fn draw(
    frame: &mut Frame,
    screen: Rect,
    at: (u16, u16),
    lines: &[String],
    theme: &Theme,
    avoid: Option<Rect>,
    hits: &HitMap,
) -> Option<Rect> {
    let rect = {
        let buffer = frame.buffer_mut();
        place_clear(screen, at, lines, avoid, &|rect| {
            cells_read(buffer, hits, at, rect)
        })?
    };
    frame.render_widget(Clear, rect);
    let mut style = theme.style(Role::Text);
    style = match theme.surface(Surface::Raised) {
        Some(raised) => style.bg(raised),
        None => style.add_modifier(Modifier::REVERSED),
    };
    frame.buffer_mut().set_style(rect, style);
    let text: Vec<Line> = lines
        .iter()
        .map(|line| Line::from(format!(" {line}")))
        .collect();
    frame.render_widget(Paragraph::new(text).style(style), rect);
    Some(rect)
}

/// One or two lines, each cut to [`MAX_WIDTH`] cells.
fn lines(parts: &[&str]) -> Option<Vec<String>> {
    Some(
        parts
            .iter()
            .map(|part| widgets::text::cut_to_cells(part, MAX_WIDTH))
            .collect(),
    )
}

/// What a tooltip on `hit` says, or `None` for a hit with no tooltip: menu entries, the Settings
/// box's rows, the toast, the pane headers and the rest that nothing is worth saying about.
/// Each line is at most [`MAX_WIDTH`] cells.
fn text(hit: Hit, app: &App) -> Option<Vec<String>> {
    match hit {
        Hit::Tab(screen) => tab(screen),
        Hit::HelpMark => lines(&["Every key, by screen · ?"]),
        Hit::EscMark => lines(&["Up one level · Esc"]),
        Hit::Key(code) => key_word(code, app),
        Hit::Row(Pane::Controls, row)
        | Hit::Level(row)
        | Hit::LevelBar(row)
        | Hit::ToggleOf(row)
        | Hit::SelectPrev(row)
        | Hit::SelectNext(row) => control_row(app, row),
        Hit::Row(..) | Hit::PaneHeader(_) => None,
        Hit::Mode | Hit::ModePrev | Hit::ModeNext => {
            lines(&["Mode", "a click toggles · ‹ › steps it"])
        }
        Hit::Loudness => lines(&["Loudness", "a click toggles · the wheel sets it"]),
        Hit::LoudnessBar => lines(&["Loudness strength", "click or drag the bar · the wheel too"]),
        Hit::Band(band) | Hit::BandField(band, _) => lines(&[
            &format!("Band {}", band + 1),
            "double-click a value to edit it",
        ]),
        Hit::Side(_) | Hit::Target => lines(&[
            "Side",
            "a click or C switches between the device and the computer",
        ]),
        Hit::PointField(point, 0) => {
            lines(&[&format!("Point {}", point + 1), "a click cycles its type"])
        }
        Hit::PointField(point, _) | Hit::Point(point) => {
            lines(&[&format!("Point {}", point + 1), "the wheel steps the gain"])
        }
        Hit::CrossfeedOn | Hit::CrossfeedModel(_) => lines(&[
            "Crossfeed",
            "a click on the word turns it on or off · ‹ › picks the model",
        ]),
        Hit::Crossfeed(Xf::Preset) | Hit::CrossfeedStep(Xf::Preset, _) => match app.screen {
            Screen::Tone => lines(&["Crossfeed", "← → preset · , . model · ␣ on/off · X here"]),
            _ => lines(&[
                "Crossfeed",
                "← → ␣ on/off · , . preset · Enter or X: all of it",
            ]),
        },
        Hit::Crossfeed(row) | Hit::CrossfeedStep(row, _) => {
            lines(&[row.label(), "← → steps · wheel too · Enter types a number"])
        }
        Hit::Curve => lines(&["Response curve", "drag · right-drag widens · wheel sets Q"]),
        Hit::Divider => lines(&["Pane divider", "drag · double-click resets · < > nudge"]),
        Hit::Transport(button) => {
            let name = Button::ALL.get(button)?.method();
            lines(&[name, "the player on this computer"])
        }
        Hit::ListRow(row) => list_row(app, row),
        Hit::Connection => match app.session.connected {
            true => lines(&["connected"]),
            false => lines(&["link lost · r reopens"]),
        },
        Hit::MouseMark => lines(&[
            "Mouse capture · M toggles it",
            "off, the terminal selects text",
        ]),
        Hit::FaultMark => {
            let faults = app.faults();
            if faults.is_empty() {
                return None;
            }
            let labels: Vec<&str> = faults.iter().map(|fault| fault.label.as_str()).collect();
            lines(&[&labels.join(", "), "a shows the advice"])
        }
        Hit::Footer(_)
        | Hit::SettingRow(_)
        | Hit::WebLink
        | Hit::SettingPrev
        | Hit::SettingNext
        | Hit::SettingCategory(_)
        | Hit::MenuEntry(_)
        | Hit::MenuOutside
        | Hit::PresetRow(_)
        | Hit::PresetEnd(_)
        | Hit::Mascot
        | Hit::Toast => None,
    }
}

/// A tab of the strip: its name, what it shows and the key that leads there.
fn tab(screen: Screen) -> Option<Vec<String>> {
    match screen {
        Screen::Device => lines(&["Controls · the device screen · Esc"]),
        Screen::Tone => lines(&["EQ · the tone control · e"]),
        Screen::Telemetry => lines(&["Telemetry · every reading · t"]),
        Screen::Devices => lines(&["Devices · add, change, switch · d"]),
        Screen::Settings | Screen::Setup => None,
    }
}

/// A word on a pane's border: what its key does and the key, from the key table.
fn key_word(code: KeyCode, app: &App) -> Option<Vec<String>> {
    let hint = keys::hints_for(app.screen)
        .into_iter()
        .find(|hint| hint.code == code)
        .or_else(|| keys::all().iter().find(|hint| hint.code == code))?;
    hint_tip(hint)
}

/// What a tooltip on one key says: what it does and the key, from the key table.
fn hint_tip(hint: &keys::KeyHint) -> Option<Vec<String>> {
    lines(&[&format!("{} · {}", hint.does, hint.label)])
}

/// What a tooltip on `hit` says in the setup wizard, whose only targets with one are the keys
/// of its footer: the words the key line itself draws for `scene` (a key's words can change
/// with the step), in the form the same key says on a pane's border.
pub(crate) fn wizard_text(hit: Hit, scene: keys::Scene) -> Option<Vec<String>> {
    match hit {
        Hit::Footer(at) => {
            let hint = *keys::hints_for(Screen::Setup).get(at)?;
            lines(&[&format!(
                "{} · {}",
                hint.does_in(scene),
                hint.label_in(scene)
            )])
        }
        _ => None,
    }
}

/// A row of the Controls pane, by the control it shows or the Host group's row it is: the
/// label and what to do with that kind of control. Row `row` counts the device's own writable
/// rows first and the Host group's after them, as the hits do.
fn control_row(app: &App, row: usize) -> Option<Vec<String>> {
    let rows = app.rows();
    if let Some(row) = rows.get(row) {
        if app.is_computer_volume(row.control) {
            return lines(&[
                &row.control.label,
                "←→ or the wheel steps it 1 %",
                "Enter mutes it",
            ]);
        }
        return control(row.control);
    }
    if !app.host_shown() {
        return None;
    }
    match app.host_rows().get(row - rows.len())? {
        HostRow::Volume => lines(&["Volume", "←→ or the wheel steps it 1 % · Enter mutes"]),
        HostRow::Transport => None,
    }
}

/// How a level is used.
const LEVEL: &str = "click or drag the bar · ←→ or the wheel";

/// A control's label and how its kind is used.
fn control(control: &Control) -> Option<Vec<String>> {
    let how = match control.kind {
        ControlKind::Level if control.key == "volume" => "click or drag · glides up above 75%",
        ControlKind::Level => LEVEL,
        ControlKind::Toggle => "Space or a click flips it",
        ControlKind::Select => "←→ or ‹ › steps it",
        ControlKind::Readout | ControlKind::Meter => "read from the device",
        ControlKind::Fault => "a shows the advice",
    };
    lines(&[&control.label, how])
}

/// A row of the Telemetry or Devices list.
fn list_row(app: &App, row: usize) -> Option<Vec<String>> {
    match app.screen {
        Screen::Telemetry => {
            let (id, reading) = app.telemetry_rows().into_iter().nth(row)?;
            let name = telemetry::label_for(app, id, &reading.value.raw);
            lines(&[&name, "as the device last reported it"])
        }
        Screen::Devices => {
            let device = app.devices.rows.get(row)?;
            lines(&[&device.name, "Enter switches · c changes · x removes"])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
