// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the last frame drew what, so a click can ask for the same thing a key does.
//!
//! A screen records the rectangle of every row and widget it draws as it draws it; [`resolve`]
//! turns a mouse position back into one of these, reusing the same [`App`] helpers a key press
//! goes through so a click or the wheel never derives a value the keys did not already know
//! how to derive. A press on the tone-control screen's curve grabs a band's point instead, which
//! [`curve_drag`] holds until the button is let go.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::app::curve_drag::{self, Gesture};
use super::app::glide::Bar;
use super::app::{App, Intent, Pane, Screen, SettingsBox, ToneRow};

/// The longest two left presses on the same cell can be apart and still make a double-click:
/// crossterm reports every press alone, so [`note_press`] tells a double-click by the time
/// between them.
pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// What a click inside one recorded rectangle asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Focus row `1` (the tuple's second field) of pane `0` (its first).
    Row(Pane, usize),
    /// Flip the toggle on this row.
    ToggleOf(usize),
    /// Step this row's select back one value.
    SelectPrev(usize),
    /// Step this row's select on one value.
    SelectNext(usize),
    /// Nudge this row's level.
    Level(usize),
    /// The cells of this row's level bar, drawn over the row's own [`Hit::Level`]: a press sets
    /// the value the cell stands for and holds the bar ([`glide`](super::app::glide)); the wheel
    /// nudges the row as over the rest of it.
    LevelBar(usize),
    /// The tone control's Loudness bar, wherever it is drawn, drawn over the half's
    /// [`Hit::Loudness`]: a press sets the strength the cell stands for and holds the bar; the
    /// wheel steps the strength as over the rest of the half.
    LoudnessBar,
    /// Focus the Mode half and toggle it, off its `‹ ›` arrows.
    Mode,
    /// Step the Mode select back one value, the way `‹` on it does.
    ModePrev,
    /// Step the Mode select on one value, the way `›` on it does.
    ModeNext,
    /// Focus the Loudness half and toggle it, off its bar; the wheel over it steps the strength
    /// instead.
    Loudness,
    /// Focus this pane.
    PaneHeader(Pane),
    /// Focus this band, 0-based.
    Band(usize),
    /// One value of band `0`'s row — field `1` of the edit line, an index into
    /// [`PARAMS`](super::boxes::edit_line::PARAMS) — which a click focuses the band from, as
    /// anywhere on the row, and a double-click opens the edit line on.
    BandField(usize, usize),
    /// Show this screen.
    Tab(Screen),
    /// A word on a pane's top border, which a click presses the key of.
    Key(KeyCode),
    /// The tab strip's `? help`, which opens the help overlay as `?` does.
    HelpMark,
    /// The tab strip's `esc …`, which acts as `Esc` does.
    EscMark,
    /// Focus this row of a screen with no pane of its own — the telemetry and device lists.
    ListRow(usize),
    /// Press this key of the setup wizard's key row, by where it sits in the screen's own
    /// [`hints_for`](super::keys::hints_for).
    Footer(usize),
    /// The tone-control screen's Response curve, its cells alone: a press near a band's point
    /// there grabs the band ([`curve_drag::grab`]).
    Curve,
    /// One of the Transport row's buttons, an index into
    /// [`Button::ALL`](svolder_host::Button::ALL).
    Transport(usize),
    /// Select this row of the Settings box's category.
    SettingRow(usize),
    /// The link the Web UI box shows, on every line it wraps to, which a click copies and opens.
    WebLink,
    /// Step the Settings box's selected row back, the way `←` does: the `‹` before its value.
    SettingPrev,
    /// Step the Settings box's selected row on, the way `→` does: the `›` after its value.
    SettingNext,
    /// Show this category of the Settings box, on its first row.
    SettingCategory(usize),
    /// Take this entry of the menu, an index into
    /// [`menu::ENTRIES`](super::screens::menu::ENTRIES).
    MenuEntry(usize),
    /// The dimmed cells around the menu, which a click closes it from.
    MenuOutside,
    /// The toast, which a click dismisses and which hides what is under it from the mouse.
    Toast,
    /// The header's `● connected` / `○ disconnected` words, which a click does nothing on; a
    /// pointer resting on them says what they are ([`tooltip`](super::boxes::tooltip)).
    Connection,
    /// The header's `mouse:on` / `mouse:off`, which a click does nothing on.
    MouseMark,
    /// The header's fault glyphs, which a click does nothing on.
    FaultMark,
    /// The squirrel, which a click makes hop and the wheel does nothing on.
    Mascot,
}

/// What a selection the keys move through belongs to, for telling a cursor that moved within a
/// list from one that arrived with the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selected {
    /// The Telemetry screen's rows.
    Telemetry,
    /// The Devices screen's list.
    Devices,
    /// The menu's entries.
    Menu,
    /// The Settings box's rows.
    Settings,
    /// The preset list's rows.
    Presets,
    /// The AutoEq search box's results.
    Search,
}

/// The row the keys select in a list, as the last frame drew it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Which list the row is in.
    pub kind: Selected,
    /// Where the row stands in that list: its index, whatever the list is scrolled to. The
    /// Settings box counts its categories in as well, so the first row of another category is
    /// another place.
    pub index: usize,
    /// The cells the row fills.
    pub area: Rect,
}

/// Every rectangle the last frame drew something clickable in.
#[derive(Default)]
pub struct HitMap {
    /// The rectangles, in the order they were drawn.
    hits: Vec<(Rect, Hit)>,
    /// The whole cells of each level bar drawn, whatever shows of it.
    bars: Vec<(Rect, Hit)>,
    /// The rectangle of every modal box drawn, borders included, the last drawn on top.
    boxes: Vec<Rect>,
    /// The row the keys select in the list the frame ended on, the last drawn on top.
    selection: Option<Selection>,
}

impl HitMap {
    /// Record that `hit` was drawn at `area`.
    pub fn push(&mut self, area: Rect, hit: Hit) {
        self.hits.push((area, hit));
    }

    /// Record that `hit`, a level bar, was drawn at `full`, of which only `visible` shows: a pane
    /// can cut the end of a bar off. The hit answers on the visible cells, but a cell is read
    /// against the whole bar ([`HitMap::bar_area`]), so it stands for the value it is drawn at.
    pub fn push_bar(&mut self, visible: Rect, full: Rect, hit: Hit) {
        self.hits.push((visible, hit));
        self.bars.push((full, hit));
    }

    /// The cells the last frame drew the level bar `hit` across, the part a pane cut off
    /// included.
    pub fn bar_area(&self, hit: Hit) -> Option<Rect> {
        self.bars
            .iter()
            .rev()
            .find(|(_, drawn)| *drawn == hit)
            .map(|(area, _)| *area)
    }

    /// Record that a modal box was drawn over `area`, borders included.
    pub fn push_box(&mut self, area: Rect) {
        self.boxes.push(area);
    }

    /// The rectangle of the box on top: the last one the frame drew.
    pub fn top_box(&self) -> Option<Rect> {
        self.boxes.last().copied()
    }

    /// Record that the keys select row `index` of the list `kind`, drawn over `area`, in place of
    /// whatever the frame had selected before.
    pub fn select(&mut self, kind: Selected, index: usize, area: Rect) {
        self.selection = Some(Selection { kind, index, area });
    }

    /// Forget the selection drawn so far: what is drawn next covers it, and its own selection,
    /// if it has one, is what the keys are in.
    pub fn deselect(&mut self) {
        self.selection = None;
    }

    /// The row the keys select in the list on top, if the frame drew one.
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// Forget the last frame, which a new frame is about to replace.
    pub fn clear(&mut self) {
        self.hits.clear();
        self.bars.clear();
        self.boxes.clear();
        self.selection = None;
    }

    /// The last hit pushed whose rectangle contains `(x, y)`, so a widget drawn inside its own
    /// row's rectangle — a toggle's box inside the row it flips, say — wins over the row
    /// itself.
    pub fn hit_at(&self, x: u16, y: u16) -> Option<&Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(area, _)| area.contains(Position { x, y }))
            .map(|(_, hit)| hit)
    }

    /// Every rectangle the last frame drew `hit` in, in the order they were drawn.
    pub fn areas_of(&self, hit: Hit) -> impl Iterator<Item = Rect> + '_ {
        self.hits
            .iter()
            .filter(move |(_, drawn)| *drawn == hit)
            .map(|(area, _)| *area)
    }

    /// The rectangle the last frame drew `hit` in — the last of them, for a hit drawn twice — or
    /// `None` for one it did not draw.
    pub fn area_of(&self, hit: Hit) -> Option<Rect> {
        self.hits
            .iter()
            .rev()
            .find(|(_, drawn)| *drawn == hit)
            .map(|(area, _)| *area)
    }
}

/// What resolving a mouse event against the last frame's hits asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum MouseAction {
    /// Make the squirrel hop.
    Mascot,
    /// Focus row `1` of pane `0`, the way `Tab` then the arrow keys would.
    Focus(Pane, usize),
    /// Carry out this intent, the way a key that asks for it would.
    Intent(Intent),
    /// Carry out these writes to the bands, finished as a band key's are by
    /// [`App::band_edit`]: on a tone control in Flat mode, the switch to EQ after them.
    BandEdit(Vec<Intent>),
    /// Show this screen, the way `e`, `t`, `d` or `Esc` would.
    Screen(Screen),
    /// Focus this band, 0-based.
    Band(usize),
    /// Focus this half of the tone control's header or Tone pane rows — Mode or Loudness — and
    /// toggle it, finished the way [`App::click_tone_row`] finishes it.
    ToggleTone(ToneRow),
    /// Step the tone control's mode this way, the way a click on the Mode select's `‹` or `›`
    /// does: finished the way [`App::step_tone_mode`] finishes it, with no change of focus.
    StepMode(i64),
    /// Press this key, which is what a click on a word of a border asks for.
    Key(KeyCode),
    /// Hold this band's point for this gesture, focusing the band: a press on the curve that
    /// grabbed it ([`curve_drag::start`]).
    Grab {
        /// The band grabbed, 0-based.
        band: usize,
        /// What dragging it does.
        gesture: Gesture,
        /// The cell the press landed on.
        at: (u16, u16),
    },
    /// Drag the point being held to this cell of the curve drawn in these cells, finished as a
    /// band key's writes are ([`curve_drag::to`]).
    DragTo {
        /// The cell under the pointer, which may be past the curve's edges.
        at: (u16, u16),
        /// The cells the last frame drew the curve in.
        curve: Rect,
    },
    /// Open the edit line on this band with this field focused — an index into
    /// [`PARAMS`](super::boxes::edit_line::PARAMS) — the way a double-click on that value of the
    /// band's row does ([`edit_line::open_at`](super::boxes::edit_line::open_at)).
    EditBand {
        /// The band, 0-based.
        band: usize,
        /// The field to focus.
        field: usize,
    },
    /// Focus the Transport row on this button, an index into
    /// [`Button::ALL`](svolder_host::Button::ALL), and press it, the way `Enter` there would
    /// ([`App::click_transport`]).
    Transport(usize),
    /// Move the Settings box to this category and row.
    Setting(SettingsBox),
    /// Dismiss the toast.
    DismissToast,
    /// Press a level bar drawn across `area` at column `at`: focus its row, set the value the
    /// cell stands for and hold the bar ([`glide::press`](super::app::glide::press)).
    BarPress {
        /// The bar pressed.
        bar: Bar,
        /// The column pressed.
        at: u16,
        /// The cells the last frame drew the bar in.
        area: Rect,
    },
    /// Drag the held bar to column `at` of the bar drawn across `area`.
    BarTo {
        /// The column under the pointer, which may be past the bar's ends.
        at: u16,
        /// The cells the last frame drew the bar in.
        area: Rect,
    },
    /// Let go of the held bar, its button released at column `at` with the bar last drawn in
    /// `area` — `None` where the last frame drew none.
    BarRelease {
        /// The column under the pointer as the button was released.
        at: u16,
        /// The cells the last frame drew the bar in.
        area: Option<Rect>,
    },
    /// Let go of the point being dragged, its button released at this cell with the curve last
    /// drawn in these cells — `None` where the last frame drew none — writing where it let go
    /// ([`curve_drag::release`]).
    Release {
        /// The cell under the pointer as the button was released.
        at: (u16, u16),
        /// The cells the last frame drew the curve in.
        curve: Option<Rect>,
    },
}

/// Note a left press at `event`'s cell at `now`, before it is resolved: whether it is the second
/// of a double-click — on the same cell as the press before it and within [`DOUBLE_CLICK`] of it
/// — which is what [`resolve`] reads the press as. A press elsewhere or later starts over, as does
/// the press after a double-click, so a third press is a click again, and a press a box over the
/// screen swallows or a drag held by the other button ignores. Nothing but a left press is
/// noted.
pub fn note_press(app: &mut App, event: &MouseEvent, now: Instant) {
    if event.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    // A press a box swallows, or one a drag of the other button ignores, is no click: it makes
    // no double-click with the press after it.
    if app.modal_open() || app.pointer.curve_drag.is_some() || app.pointer.bar_drag.is_some() {
        app.pointer.last_press = None;
        app.pointer.double_click = false;
        return;
    }
    let at = (event.column, event.row);
    app.pointer.double_click = app.pointer.last_press.is_some_and(|(cell, when)| {
        cell == at && now.saturating_duration_since(when) <= DOUBLE_CLICK
    });
    app.pointer.last_press = (!app.pointer.double_click).then_some((at, now));
}

/// Turn one mouse event into what it asks for, against `hits` (the rectangles the last frame
/// drew) and `app` (the current values a toggle or a select needs to know which way to step).
///
/// A left click (`Down`) and the wheel (`ScrollUp`/`ScrollDown`) resolve against whatever they
/// land on; the second press of a double-click ([`note_press`]) on one of a band row's values
/// opens the edit line on that value's field, where a single click focuses the band as anywhere
/// on its row. On the tone-control screen's curve a left or a right press grabs a band's point
/// instead ([`curve_drag::grab`]) — a right press anywhere else asks for nothing. Dragging with the
/// button a drag is held by drags the point wherever the pointer goes while the curve is drawn,
/// and releasing that button lets go of it. While a drag is held, a press of the other button
/// asks for nothing; a press of its own is read as a fresh press, the loop having let the drag
/// go first ([`curve_drag::lost`]). A move (which [`tooltip::hover`](super::boxes::tooltip::hover)
/// reads instead), a middle click, and a drag or release with no drag held by its button ask
/// for nothing.
///
/// A left press on a level's bar ([`Hit::LevelBar`], [`Hit::LoudnessBar`]) holds the bar: the
/// drag then follows the pointer wherever it goes, and the release lets go of it
/// ([`glide`](super::app::glide)). While one is held, a press of any other button asks for nothing.
///
/// While the help overlay is open it is the only thing the mouse reaches: a click anywhere
/// closes it (as `Esc` does), and the wheel scrolls it a line at a time (as `↑`/`↓` do). The
/// screen under it is read through the overlay, never clicked through it.
///
/// The menu is the mouse's own while it is open: a left click on an entry takes it as its key
/// does, a left click outside the box closes it, and everything else asks for nothing.
///
/// The Settings box is the mouse's own while it is open: a left click on a row selects it, on
/// the `‹` or `›` of the selected row steps it as `←` or `→` do, and on a category's name shows
/// that category, and on the `esc close` of its bottom border closes it; a click inside the box
/// anywhere else, the right button and the wheel ask for nothing. A left click outside the box
/// closes it as `Esc` does, or, with an edit line or a question open over its rows, outside that
/// box, cancels it.
///
/// The device editor, the removal question, the preset list, a band's edit line and the `q`
/// question are answered from the keyboard: a click inside one asks for nothing, and a left
/// click outside it presses `Esc`, which closes it or cancels what it asks, and reaches nothing
/// under it.
///
/// The toast is the one exception to all of that: it is drawn over every box, so a left click
/// on it dismisses it whatever box is open and reaches nothing under it; a right press, a
/// middle press and the wheel over it ask for nothing, and a drag or release passes through to
/// what it would reach without the toast.
pub fn resolve(hits: &HitMap, event: MouseEvent, app: &App) -> Option<MouseAction> {
    // The toast is drawn over everything, boxes included: a click on it dismisses it, and
    // nothing under it is reached.
    // A drag held across it keeps going, and its release is still seen.
    if hits.hit_at(event.column, event.row) == Some(&Hit::Toast) {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => return Some(MouseAction::DismissToast),
            MouseEventKind::Down(_) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                return None;
            }
            _ => {}
        }
    }
    if app.overlays.help_open {
        return match event.kind {
            MouseEventKind::Down(MouseButton::Left) => Some(MouseAction::Key(KeyCode::Esc)),
            MouseEventKind::ScrollUp => Some(MouseAction::Key(KeyCode::Up)),
            MouseEventKind::ScrollDown => Some(MouseAction::Key(KeyCode::Down)),
            _ => None,
        };
    }
    // The menu is clicked: an entry takes it as its key does, and anywhere outside the box
    // closes it as `Esc` does.
    if app.overlays.menu.is_some() {
        return match (event.kind, hits.hit_at(event.column, event.row)) {
            (MouseEventKind::Down(MouseButton::Left), Some(Hit::MenuEntry(entry))) => {
                Some(MouseAction::Key(match entry {
                    0 => KeyCode::Char('s'),
                    1 => KeyCode::Char('h'),
                    _ => KeyCode::Char('q'),
                }))
            }
            (MouseEventKind::Down(MouseButton::Left), Some(Hit::MenuOutside)) => {
                Some(MouseAction::Key(KeyCode::Esc))
            }
            _ => None,
        };
    }
    // A left click outside the box on top closes it as `Esc` does, whatever is drawn there: the
    // greyed header's `esc` mark and logo included. It never reaches what is under the box.
    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
        && (app.overlays.settings.is_some() || app.modal_open())
        && hits.top_box().is_some_and(|top| {
            !top.contains(Position {
                x: event.column,
                y: event.row,
            })
        })
    {
        return Some(MouseAction::Key(KeyCode::Esc));
    }
    // The Settings box is clicked: its rows, the arrows on the selected one and its categories.
    // Under the quit question it is a box like any other, answered from the keyboard.
    if app.overlays.settings.is_some() && !app.overlays.quitting {
        return match event.kind {
            MouseEventKind::Down(MouseButton::Left) => resolve_box_click(hits, event, app),
            _ => None,
        };
    }
    // Every other modal box is answered from the keyboard: the rows under it are not to be
    // clicked out from underneath what is being answered.
    if app.modal_open() {
        return None;
    }
    let at = (event.column, event.row);
    // The other button pressed while a drag is held does nothing: the drag's own is still down.
    if let (Some(drag), MouseEventKind::Down(button)) =
        (app.pointer.curve_drag.as_ref(), event.kind)
        && drag.gesture.button() != button
    {
        return None;
    }
    // A bar held is held by the left button: a press of any other does nothing meanwhile.
    if app.pointer.bar_drag.is_some()
        && matches!(event.kind, MouseEventKind::Down(button) if button != MouseButton::Left)
    {
        return None;
    }
    if let Some(bar) = app.pointer.bar_drag.as_ref().map(|drag| drag.bar) {
        match event.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                return hits
                    .bar_area(bar.hit())
                    .map(|area| MouseAction::BarTo { at: at.0, area });
            }
            MouseEventKind::Up(MouseButton::Left) => {
                return Some(MouseAction::BarRelease {
                    at: at.0,
                    area: hits.bar_area(bar.hit()),
                });
            }
            _ => {}
        }
    }
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            resolve_click(hits, event.column, event.row, app)
        }
        MouseEventKind::Down(MouseButton::Right) => resolve_press(hits, at, app, Gesture::Width),
        MouseEventKind::Drag(button) => {
            app.pointer
                .curve_drag
                .as_ref()
                .filter(|drag| drag.gesture.button() == button)?;
            let curve = hits.area_of(Hit::Curve)?;
            Some(MouseAction::DragTo { at, curve })
        }
        MouseEventKind::Up(button) => app
            .pointer
            .curve_drag
            .as_ref()
            .filter(|drag| drag.gesture.button() == button)
            .map(|_| MouseAction::Release {
                at,
                curve: hits.area_of(Hit::Curve),
            }),
        MouseEventKind::ScrollUp => resolve_wheel(hits, event.column, event.row, app, 1),
        MouseEventKind::ScrollDown => resolve_wheel(hits, event.column, event.row, app, -1),
        _ => None,
    }
}

/// A left click on the Settings box: a row selects it, the arrows on the selected row step it
/// as `←` and `→` do, a category's name shows that category, and anywhere else does nothing.
fn resolve_box_click(hits: &HitMap, event: MouseEvent, app: &App) -> Option<MouseAction> {
    let at = app.overlays.settings?;
    match *hits.hit_at(event.column, event.row)? {
        Hit::SettingRow(row) => Some(MouseAction::Setting(SettingsBox { row, ..at })),
        Hit::SettingPrev => Some(MouseAction::Key(KeyCode::Left)),
        Hit::SettingNext => Some(MouseAction::Key(KeyCode::Right)),
        Hit::EscMark => Some(MouseAction::Key(KeyCode::Esc)),
        Hit::WebLink => Some(MouseAction::Intent(Intent::OpenWebLink)),
        Hit::SettingCategory(category) => {
            Some(MouseAction::Setting(SettingsBox { category, row: 0 }))
        }
        _ => None,
    }
}

/// A press at `at` for `gesture`: the band it grabs where it lands on the curve, and nothing
/// anywhere else or where it grabs nothing.
fn resolve_press(
    hits: &HitMap,
    at: (u16, u16),
    app: &App,
    gesture: Gesture,
) -> Option<MouseAction> {
    if hits.hit_at(at.0, at.1) != Some(&Hit::Curve) {
        return None;
    }
    let curve = hits.area_of(Hit::Curve)?;
    curve_drag::grab(app, curve, at, gesture).map(|band| MouseAction::Grab { band, gesture, at })
}

/// A left click against the hit under it.
fn resolve_click(hits: &HitMap, x: u16, y: u16, app: &App) -> Option<MouseAction> {
    match *hits.hit_at(x, y)? {
        Hit::Row(pane, i) => Some(MouseAction::Focus(pane, i)),
        Hit::Level(i) => Some(MouseAction::Focus(Pane::Controls, i)),
        Hit::LevelBar(i) => bar_press(hits, Bar::Row(i), x),
        Hit::LoudnessBar => bar_press(hits, Bar::Loudness, x),
        Hit::ListRow(i) => Some(MouseAction::Focus(app.pane, i)),
        Hit::ToggleOf(i) => intent(app.flip_row(i)),
        Hit::SelectPrev(i) => intent(app.step_row(i, -1)),
        Hit::SelectNext(i) => intent(app.step_row(i, 1)),
        Hit::PaneHeader(pane) => Some(MouseAction::Focus(pane, app.selected)),
        Hit::Band(i) => Some(MouseAction::Band(i)),
        // The edit line is drawn over the tone-control screen only, so on the device screen's
        // Tone pane a double-click selects the band and goes no further.
        Hit::BandField(band, field) => Some(
            match app.pointer.double_click && app.screen == Screen::Tone {
                true => MouseAction::EditBand { band, field },
                false => MouseAction::Band(band),
            },
        ),
        // The tab of the screen showing asks for nothing.
        Hit::Tab(screen) => (screen != app.screen).then_some(MouseAction::Screen(screen)),
        Hit::HelpMark => Some(MouseAction::Key(KeyCode::Char('?'))),
        Hit::EscMark => Some(MouseAction::Key(KeyCode::Esc)),
        // A word of a border is the key, whether or not it is worth pressing here: the key
        // does what it does.
        Hit::Key(code) => Some(MouseAction::Key(code)),
        // The setup wizard answers the clicks on its own key row.
        Hit::Footer(_) => None,
        Hit::Mascot => Some(MouseAction::Mascot),
        Hit::Mode => Some(MouseAction::ToggleTone(ToneRow::Mode)),
        Hit::ModePrev => Some(MouseAction::StepMode(-1)),
        Hit::ModeNext => Some(MouseAction::StepMode(1)),
        Hit::Loudness => Some(MouseAction::ToggleTone(ToneRow::Loudness)),
        Hit::Curve => resolve_press(hits, (x, y), app, Gesture::Move),
        Hit::Transport(button) => Some(MouseAction::Transport(button)),
        Hit::Toast => Some(MouseAction::DismissToast),
        // These only say what they are to a resting pointer.
        Hit::Connection | Hit::MouseMark | Hit::FaultMark => None,
        // The menu answers its own clicks.
        Hit::MenuEntry(_) | Hit::MenuOutside => None,
        // These are drawn by the Settings box alone, which answers its own clicks.
        Hit::SettingRow(_)
        | Hit::SettingPrev
        | Hit::SettingNext
        | Hit::SettingCategory(_)
        | Hit::WebLink => None,
    }
}

/// A left press at column `x` on `bar`, against the cells the last frame drew it in.
fn bar_press(hits: &HitMap, bar: Bar, x: u16) -> Option<MouseAction> {
    let area = hits.bar_area(bar.hit())?;
    Some(MouseAction::BarPress { bar, at: x, area })
}

/// A `dir` step of the wheel (`1` for `ScrollUp`, `-1` for `ScrollDown`) against the hit under
/// it: a level, a band's gain, the loudness bar, or on the tone curve the Q of the band under
/// the pointer — the one a right press there grabs ([`curve_drag::grab`]), or the band the cursor
/// is on where the curve is drawn from none — up narrowing the bell and down widening it. Anything
/// else asks for nothing.
fn resolve_wheel(hits: &HitMap, x: u16, y: u16, app: &App, dir: i64) -> Option<MouseAction> {
    let edit = |writes: Vec<Intent>| (!writes.is_empty()).then_some(MouseAction::BandEdit(writes));
    match *hits.hit_at(x, y)? {
        Hit::Level(i) | Hit::LevelBar(i) => intent(app.nudge_row(i, dir)),
        Hit::Band(i) | Hit::BandField(i, _) => edit(app.nudge_band(i, dir)),
        Hit::Loudness | Hit::LoudnessBar => intent(app.step_loudness(dir)),
        Hit::Curve => {
            let curve = hits.area_of(Hit::Curve)?;
            let band =
                curve_drag::grab(app, curve, (x, y), Gesture::Width).unwrap_or(app.tone.band);
            edit(app.nudge_q(band, dir))
        }
        _ => None,
    }
}

/// The last (and, for every helper this calls, only) intent a key path answered with, as a
/// [`MouseAction`], or nothing at all from a path that asked for nothing.
fn intent(mut intents: Vec<Intent>) -> Option<MouseAction> {
    intents.pop().map(MouseAction::Intent)
}

#[cfg(test)]
mod tests;
