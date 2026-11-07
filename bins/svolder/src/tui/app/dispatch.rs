// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How a key press becomes intents: the boxes open over the screen first, then `Tab` and the
//! keys every screen shares, then the screen's own; and the moves from one screen to
//! another.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::glide;
use crate::tui::boxes::edit_line;
use crate::tui::boxes::editor;
use crate::tui::boxes::{device_presets, presets};
use crate::tui::keys;
use crate::tui::screens::{help, menu};

use super::{App, FocusMemory, Intent, Screen, stepped};

impl App {
    /// The writes one key press asks for, with the row the cursor left noted for its trail.
    ///
    /// Ctrl-C quits; every other chord with control or alt asks for nothing, so a terminal habit
    /// such as ctrl-L never reaches the device. Everything else is read as a key of
    /// the screen being shown, after the keys that mean the same thing everywhere.
    pub fn key(&mut self, key: KeyEvent) -> Vec<Intent> {
        // A key hides the tooltip.
        self.pointer.tooltip = None;
        let from = self.cursor_id();
        // A plain key stops the Volume's climb where it is, and lets go of a level bar held,
        // before it does what it does. A chord with control or alt (Ctrl-C included) drops both
        // without writing the step held: it never reaches the device.
        let mut intents = match key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            true => {
                self.pointer.climb = None;
                self.pointer.bar_drag = None;
                Vec::new()
            }
            false => glide::stop(self),
        };
        intents.extend(self.pressed(key));
        // A box open over the screen takes the mouse, so a drag it opened over would never see
        // its button let go.
        if self.modal_open() {
            self.pointer.curve_drag = None;
            self.pointer.bar_drag = None;
            // A box over the screen is not where the climb can be watched or stopped.
            self.pointer.climb = None;
        }
        self.cursor_moved(from);
        self.touched(&intents);
        intents
    }

    /// The writes one key press asks for.
    ///
    /// Ctrl-C quits from wherever it is pressed, past every question: raw mode keeps the
    /// terminal from turning it into a signal, so this is the one way it reaches the loop. Every
    /// other chord with control or alt asks for nothing.
    ///
    /// The quit question, the menu and the help overlay take every key while open, except the
    /// keys that close them, move through them, or leave. The Settings box takes every key too, so a key
    /// it has no use for never reaches the screen under it. The device editor, its removal question, the
    /// preset lists and a band's edit line take every key too, for the same reason: a letter
    /// typed into a name, or a digit into a band's value, is that character, not the key it is
    /// elsewhere. While a point is being dragged on the tone curve, `Esc` lets go of it and does
    /// nothing else.
    fn pressed(&mut self, key: KeyEvent) -> Vec<Intent> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return vec![Intent::Quit];
        }
        // `Ctrl-R` fetches the AutoEq index again where the search box is open, whose letters
        // are all taken by the query.
        if key.code == KeyCode::Char('r')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && presets::search::showing(self)
        {
            return presets::search::refresh(self);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Vec::new();
        }
        if self.overlays.quitting {
            return self.quit_answer(key.code);
        }
        if self.overlays.menu.is_some() {
            return self.menu_key(key.code);
        }
        if self.overlays.help_open {
            return self.help_key(key.code);
        }
        if self.overlays.settings.is_some() {
            return self.settings_box_key(key.code);
        }
        if self.devices.editor.is_some() {
            return editor::key(self, key.code);
        }
        if self.devices.confirm.is_some() {
            return editor::answer(self, key.code);
        }
        if self.presets.preset_list.is_some() {
            return presets::key(self, key.code);
        }
        if self.presets.device_list.is_some() {
            return device_presets::key(self, key.code);
        }
        if self.tone.edit_line.is_some() {
            return edit_line::key(self, key.code);
        }
        if self.tone.point_line.is_some() {
            return self.point_line_key(key.code);
        }
        // `Esc` lets go of a point being dragged on the tone curve, and does nothing else then.
        if key.code == KeyCode::Esc && self.pointer.curve_drag.take().is_some() {
            return Vec::new();
        }
        self.on_screen(self.aliased(key))
    }

    /// The key `key` stands for on the screen being shown: on the tone-control screen a
    /// Shift+arrow is the symbol key it doubles for — `[` and `]` for ↑ and ↓, `,` and `.` for
    /// ← and → — which some keyboard layouts reach only through AltGr; everywhere else, and for
    /// every other key, the key itself, a Shift+arrow meaning what the arrow means.
    fn aliased(&self, key: KeyEvent) -> KeyCode {
        if self.screen != Screen::Tone || !key.modifiers.contains(KeyModifiers::SHIFT) {
            return key.code;
        }
        match key.code {
            KeyCode::Up => KeyCode::Char('['),
            KeyCode::Down => KeyCode::Char(']'),
            KeyCode::Left => KeyCode::Char(','),
            KeyCode::Right => KeyCode::Char('.'),
            code => code,
        }
    }

    /// The writes one key asks of the screen being shown, which is what the keys mean with no
    /// overlay over them: `Tab`, then the keys that mean the same thing everywhere, then the
    /// screen's own.
    pub(crate) fn on_screen(&mut self, code: KeyCode) -> Vec<Intent> {
        match code {
            KeyCode::Tab => return self.cycle_pane(1),
            KeyCode::BackTab => return self.cycle_pane(-1),
            _ => {}
        }
        if let Some(intents) = self.global_key(code) {
            return intents;
        }
        match self.screen {
            Screen::Device => self.device_key(code),
            Screen::Tone => self.tone_key(code),
            Screen::Telemetry => self.telemetry_key(code),
            Screen::Devices => self.devices_key(code),
            // The Settings box takes its keys before any screen's, and no app shows its name.
            Screen::Settings => Vec::new(),
            // The wizard reads its own keys against the step it is on, and no app is ever
            // showing it while this loop is turning.
            Screen::Setup => Vec::new(),
        }
    }

    /// What a key does while the help overlay is open: `?` and `Esc` close it, the arrows, the
    /// page keys and `Home`/`End` move through the keys it lists where it holds more of them
    /// than the terminal, `q` leaves as it does from the screen — asking first where the
    /// settings say to, in place of the overlay — and every other key is swallowed: the overlay
    /// is read, never typed into.
    fn help_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let (page, most) = help::scroll_of(self);
        // The content can shrink without a resize, so the offset is held inside it first.
        self.overlays.help_scroll = self.overlays.help_scroll.min(most);
        let mid_code = self.q_is_typed(code);
        if self.feed_code(code) {
            self.overlays.help_open = false;
            return Vec::new();
        }
        match code {
            KeyCode::Char('?') | KeyCode::Esc => {
                self.overlays.help_open = false;
                self.mascot.codes.clear();
            }
            KeyCode::Char('q') if mid_code => {}
            KeyCode::Char('q') => {
                self.overlays.help_open = false;
                self.mascot.codes.clear();
                return self.quit();
            }
            KeyCode::Up => self.overlays.help_scroll = self.overlays.help_scroll.saturating_sub(1),
            KeyCode::Down => self.overlays.help_scroll = (self.overlays.help_scroll + 1).min(most),
            KeyCode::PageUp => {
                self.overlays.help_scroll = self.overlays.help_scroll.saturating_sub(page)
            }
            KeyCode::PageDown => {
                self.overlays.help_scroll = (self.overlays.help_scroll + page).min(most)
            }
            KeyCode::Home => self.overlays.help_scroll = 0,
            KeyCode::End => self.overlays.help_scroll = most,
            _ => {}
        }
        Vec::new()
    }

    /// What a key does while the menu is open: `↑↓` move the cursor, stopping at the ends,
    /// `Enter` takes the entry under it, `s`, `h` or `?`, and `q` take theirs at once, `Esc`
    /// closes the menu, and every other key is swallowed, so none reaches the screen under it.
    fn menu_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let Some(at) = self.overlays.menu else {
            return Vec::new();
        };
        let mid_code = self.q_is_typed(code);
        if self.feed_code(code) {
            self.overlays.menu = None;
            return Vec::new();
        }
        let entry = match code {
            KeyCode::Esc => {
                self.overlays.menu = None;
                self.mascot.codes.clear();
                return Vec::new();
            }
            KeyCode::Char('q') if mid_code => return Vec::new(),
            KeyCode::Up => {
                self.overlays.menu = Some(stepped(at, -1, menu::ENTRIES.len()));
                return Vec::new();
            }
            KeyCode::Down => {
                self.overlays.menu = Some(stepped(at, 1, menu::ENTRIES.len()));
                return Vec::new();
            }
            KeyCode::Enter => at,
            KeyCode::Char('s') => 0,
            KeyCode::Char('h' | '?') => 1,
            KeyCode::Char('q') => 2,
            _ => return Vec::new(),
        };
        self.overlays.menu = None;
        self.mascot.codes.clear();
        match entry {
            0 => {
                self.open_settings();
                Vec::new()
            }
            1 => {
                self.open_help();
                Vec::new()
            }
            _ => self.quit(),
        }
    }

    /// Whether `code` is the `q` that follows `i d d`, which the code takes and the overlay
    /// does not act on. Read before the key is fed.
    fn q_is_typed(&self, code: KeyCode) -> bool {
        code == KeyCode::Char('q') && self.mascot_on() && self.mascot.codes.holds_idd(self.now)
    }

    /// Feed `code` to the code matcher while an overlay is open and the mascot is on. A code
    /// it completes is acted on, and the matcher cleared, before this says so: the caller
    /// closes the overlay.
    fn feed_code(&mut self, code: KeyCode) -> bool {
        if !self.mascot_on() {
            return false;
        }
        let Some(done) = self.mascot.codes.feed(code, self.now) else {
            return false;
        };
        self.mascot.codes.clear();
        self.mascot.code(done, self.now);
        true
    }

    /// What `a` does. With a fault standing it toggles that fault's advice. With none it shows
    /// the step the last failure named, once per failure (a step already on the advice line
    /// counts as shown); otherwise it hides the advice line and says how long all has been
    /// clear, with a tip or a quip.
    fn advise_key(&mut self) {
        if !self.faults().is_empty() {
            self.notices.show_advice = !self.notices.show_advice;
            return;
        }
        if self.notices.advice.is_some() && !self.notices.step_shown && !self.notices.show_advice {
            self.notices.step_shown = true;
            self.notices.show_advice = true;
            return;
        }
        // A step the advice line already shows (the fault that stood has cleared) has been seen.
        if self.notices.show_advice {
            self.notices.step_shown = true;
        }
        self.notices.show_advice = false;
        self.notices.advice_presses += 1;
        let line = super::advice::idle_line(self, self.now, self.notices.advice_presses);
        self.advise(line);
    }

    /// Open the help overlay at its first key. The overlay is a screen of its own as far as the
    /// animation layer is concerned, so it sweeps in the way a screen change does.
    pub(super) fn open_help(&mut self) {
        self.overlays.help_open = true;
        self.overlays.help_scroll = 0;
        self.mascot.codes.clear();
        self.frame_flags.screen_changed = true;
    }

    /// The keys that mean the same thing on every screen, or `None` for a key that does not.
    ///
    /// The lowercase ones act on the device or move between screens; the uppercase ones turn a
    /// mode of the TUI's own the other way, which the loop writes into the `[tui]` table so the
    /// setting and the switch the Settings box shows for it are never two different answers.
    fn global_key(&mut self, code: KeyCode) -> Option<Vec<Intent>> {
        // The uppercase keys each turn one settings row, which the key table names for both.
        if let Some(setting) = keys::setting_turned_by(code) {
            return Some(self.turned(setting).map(Intent::Tui).into_iter().collect());
        }
        let intents = match code {
            KeyCode::Char('q') => self.quit(),
            KeyCode::Char('r') => vec![Intent::Refresh],
            KeyCode::Char('b') => self.identify_key(),
            KeyCode::Char('m') => self.mute(),
            KeyCode::Char('i') => self.flip_key("phase"),
            KeyCode::Char('o') => self.flip_key("nos"),
            KeyCode::Char('a') if self.screen == Screen::Tone && self.on_computer() => return None,
            KeyCode::Char('a') => {
                self.advise_key();
                Vec::new()
            }
            KeyCode::Char('?') => {
                self.open_help();
                Vec::new()
            }
            KeyCode::Char('c') => self.go(Screen::Device),
            KeyCode::Char('e') if self.profile.eq.is_some() => self.go(Screen::Tone),
            KeyCode::Char('t') => self.go(Screen::Telemetry),
            KeyCode::Char('d') => self.go(Screen::Devices),
            KeyCode::Char('s') => {
                self.open_settings();
                Vec::new()
            }
            // `Esc` goes up one level: back to the device screen from another, and from the
            // device screen to the menu.
            KeyCode::Esc if self.screen == Screen::Device => {
                self.overlays.menu = Some(0);
                self.mascot.codes.clear();
                Vec::new()
            }
            KeyCode::Esc => self.go(Screen::Device),
            _ => return None,
        };
        Some(intents)
    }

    /// What `b` asks for: the device on show to show itself, and on the Devices list the device
    /// of the row the cursor is on, which nothing under the cursor leaves unasked.
    fn identify_key(&self) -> Vec<Intent> {
        if self.screen != Screen::Devices {
            return vec![Intent::Identify(None)];
        }
        self.devices
            .rows
            .get(self.selected)
            .map(|row| vec![Intent::Identify(Some(row.name.clone()))])
            .unwrap_or_default()
    }

    /// What `q` asks for: to leave at once where the settings say not to ask, and otherwise
    /// nothing yet but the question, which the next key answers.
    pub(super) fn quit(&mut self) -> Vec<Intent> {
        if !self.tui.confirm_quit {
            return vec![Intent::Quit];
        }
        self.overlays.quitting = true;
        Vec::new()
    }

    /// What a key does while the quit question is open: `Enter`, `y` and a second `q` leave —
    /// the question's default is to, and `qq` stays the one gesture it always was — `Esc` and
    /// `n` stay, and every other key is swallowed.
    fn quit_answer(&mut self, code: KeyCode) -> Vec<Intent> {
        match code {
            KeyCode::Enter | KeyCode::Char('y' | 'Y' | 'q') => {
                self.overlays.quitting = false;
                vec![Intent::Quit]
            }
            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                self.overlays.quitting = false;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Show `screen`, with the row focused that it was left on, and ask for nothing.
    fn go(&mut self, screen: Screen) -> Vec<Intent> {
        // Settings is a box over a screen, never a screen of its own.
        if screen == Screen::Settings {
            self.open_settings();
            return Vec::new();
        }
        self.remember_row();
        self.enter(screen);
        self.recall_row();
        Vec::new()
    }

    /// Show `screen`, the way a click on a tab header does: the same change `e`, `t`, `d` and
    /// `Esc` make from the keyboard.
    pub(crate) fn show(&mut self, screen: Screen) {
        self.go(screen);
    }

    /// Show `screen` after a device switch: the change is marked for the trace as any other
    /// is, and not for the noise or the melt, since what the switch changed is the device
    /// rather than the screen the user asked for.
    pub(crate) fn settle_on(&mut self, screen: Screen) {
        self.enter(screen);
        self.focus_memory = FocusMemory::default();
        self.selected = 0;
        self.frame_flags.navigated = false;
    }

    /// Show `screen`, noting the change for the transition it earns.
    ///
    /// Every path that shows another screen comes through here, so the flags are raised once
    /// per change and never for a key that shows the screen already showing.
    fn enter(&mut self, screen: Screen) {
        if self.screen != screen {
            self.screen = screen;
            self.frame_flags.screen_changed = true;
            self.frame_flags.navigated = true;
            // The curve a drag holds a point of is not on the screen any more, nor the bar.
            self.pointer.curve_drag = None;
            self.pointer.bar_drag = None;
            // The climb is on a bar the new screen does not show.
            self.pointer.climb = None;
        }
    }

    /// Note the row focused on the screen being left, to come back to when it is shown again.
    ///
    /// The device and devices screens are remembered by row; the telemetry screen by
    /// the reading the row shows, since its rows are a live list a new reading can slip into.
    fn remember_row(&mut self) {
        match self.screen {
            Screen::Device => self.focus_memory.device = self.selected,
            Screen::Devices => self.focus_memory.devices = self.selected,
            Screen::Telemetry => self.focus_memory.reading = self.telemetry_focus(),
            Screen::Tone | Screen::Settings | Screen::Setup => {}
        }
    }

    /// Focus the row the screen now showing was left on, held inside its rows as they are now:
    /// the first row where it has none remembered, or the reading it showed has gone.
    fn recall_row(&mut self) {
        let (row, len) = match self.screen {
            Screen::Device => (self.focus_memory.device, self.controls_len()),
            Screen::Devices => (self.focus_memory.devices, self.devices.rows.len()),
            Screen::Telemetry => (
                self.focus_memory
                    .reading
                    .and_then(|id| self.readings.values.keys().position(|had| *had == id))
                    .unwrap_or(0),
                self.readings.values.len(),
            ),
            Screen::Tone | Screen::Settings | Screen::Setup => (0, 1),
        };
        self.selected = row.min(len.saturating_sub(1));
    }

    /// Move the focused row of a list `len` long, and ask for nothing.
    pub(super) fn move_row(&mut self, delta: isize, len: usize) -> Vec<Intent> {
        self.selected = stepped(self.selected, delta, len);
        Vec::new()
    }

    /// Whether a box is open over the screen that takes every key while it is: the help
    /// overlay, the menu, the Settings box, the device editor, the question a removal asks, the preset list, a band's edit
    /// line, or the question `q` asks. Everything around it is drawn dimmed, and nothing the
    /// screen under it earned plays.
    pub(crate) fn modal_open(&self) -> bool {
        self.overlays.help_open
            || self.overlays.menu.is_some()
            || self.overlays.settings.is_some()
            || self.devices.editor.is_some()
            || self.devices.confirm.is_some()
            || self.presets.preset_list.is_some()
            || self.presets.device_list.is_some()
            || self.tone.edit_line.is_some()
            || self.tone.point_line.is_some()
            || self.overlays.quitting
    }
}
