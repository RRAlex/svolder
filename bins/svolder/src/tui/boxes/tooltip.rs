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
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph};
use svolder_host::Button;
use svolder_profiles::{Control, ControlKind};

use crate::tui::app::{App, Pane, Screen};
use crate::tui::host::{self, HostRow};
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
    if let Some(tooltip) = app.pointer.tooltip.as_mut() {
        tooltip.age(now);
    }
    if event.kind != MouseEventKind::Moved || !app.mouse || app.modal_open() {
        app.pointer.tooltip = None;
        return;
    }
    let at = (event.column, event.row);
    let Some((hit, said)) = hits
        .hit_at(at.0, at.1)
        .copied()
        .and_then(|hit| Some((hit, text(hit, app)?)))
    else {
        app.pointer.tooltip = None;
        return;
    };
    let shown = match app.pointer.tooltip {
        Some(held) if text(held.hit, app).as_ref() == Some(&said) => {
            if !held.shown {
                app.pointer.tooltip = Some(Tooltip { hit, at, ..held });
            }
            return;
        }
        Some(held) => held.shown,
        None => false,
    };
    app.pointer.tooltip = Some(Tooltip {
        hit,
        at,
        since: now,
        shown,
    });
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
    let tooltip = app.pointer.tooltip?;
    if !app.mouse || app.modal_open() {
        return None;
    }
    let (x, y) = tooltip.at;
    let under = text(*hits.hit_at(x, y)?, app)?;
    (Some(&under) == text(tooltip.hit, app).as_ref()).then_some(under)
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
fn place(screen: Rect, at: (u16, u16), lines: &[String], avoid: Option<Rect>) -> Option<Rect> {
    let preferred = area(screen, at, lines);
    let Some(avoid) = avoid.filter(|avoid| preferred.intersects(*avoid)) else {
        return Some(preferred);
    };
    let (width, height) = size(screen, lines);
    let other = match preferred.y > at.1 {
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
    };
    other.filter(|rect| !rect.intersects(avoid))
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

/// Draw `lines` as a tooltip for a pointer at `at`, over everything but `avoid` (the toast's
/// box), and return the box, or `None` where neither side of the pointer misses `avoid`.
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
) -> Option<Rect> {
    let rect = place(screen, at, lines, avoid)?;
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
        Hit::Curve => lines(&["Response curve", "drag · right-drag widens · wheel sets Q"]),
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
    lines(&[&format!("{} · {}", hint.does, hint.label)])
}

/// A row of the Controls pane, by the control it shows or the Host group's row it is: the
/// label and what to do with that kind of control. Row `row` counts the device's own writable
/// rows first and the Host group's after them, as the hits do.
fn control_row(app: &App, row: usize) -> Option<Vec<String>> {
    let rows = app.rows();
    if let Some(row) = rows.get(row) {
        return control(row.control);
    }
    if !app.host_shown() {
        return None;
    }
    match host::ROWS.get(row - rows.len())? {
        HostRow::Volume => lines(&[host::VOLUME, "←→ or the wheel steps it 2 %"]),
        HostRow::Transport => None,
        HostRow::HostEq => lines(&[
            host::HOST_EQ,
            "Enter or Space puts the computer EQ on or off",
        ]),
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
