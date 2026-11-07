// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device screen's Controls pane: its rows, the Host group's rows under them, and the
//! keys and clicks that step, flip and nudge them.

use crossterm::event::KeyCode;
use svolder_host::Button;
use svolder_ipc::host::{DacFacts, PathLine, ROUTE_NOT_READ, Verdict, path_line};
use svolder_profiles::roles;
use svolder_profiles::{Control, ControlKind, Family, Profile, Registry};

use crate::tui::boxes::{device_presets, presets};
use crate::tui::host::{self, HostRow};

use super::intent::{set, updown};
use super::{App, Intent, Pane, Reading, Screen};

/// What the Path row says with no player to follow.
const NOTHING_PLAYING: &str = "nothing playing";

/// The built-in profile called `name`, parsed once.
fn built_in(name: &str) -> Option<&'static Profile> {
    static BUILT_IN: std::sync::OnceLock<Option<Registry>> = std::sync::OnceLock::new();
    BUILT_IN
        .get_or_init(|| Registry::builtin().ok())
        .as_ref()?
        .get(name)
}

/// One row of the device screen's Controls pane.
#[derive(Debug)]
pub(crate) struct ControlRow<'a> {
    /// What the row is.
    pub control: &'a Control,
    /// Its last confirmed reading, or `None` until one arrives.
    pub reading: Option<&'a Reading>,
    /// Whether the value shown is known to be out of date.
    pub stale: bool,
}

impl App {
    /// The device screen's keys: the selection, and the focused row of whichever pane has it —
    /// a control, one of the Host group's rows under them, or one of the tone control's own rows
    /// — and, from wherever it is, the volume, the balance's centre, the tone control's flat
    /// mode and the preset list.
    pub(super) fn device_key(&mut self, code: KeyCode) -> Vec<Intent> {
        match code {
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Left => self.step_focused(-1),
            KeyCode::Right => self.step_focused(1),
            KeyCode::Char(' ') | KeyCode::Enter => match self.focused_control() {
                Some(control) if self.is_computer_volume(control) => self.computer_mute(),
                Some(control) => self.flip(control),
                None => match self.focused_host_row() {
                    Some(row) => self.act_on_host(row),
                    None => Vec::new(),
                },
            },
            // `=` is the unshifted `+` on most layouts, so it raises the volume too.
            KeyCode::Char('+' | '=') if self.on_computer() => self.step_host_volume(1),
            KeyCode::Char('-') if self.on_computer() => self.step_host_volume(-1),
            KeyCode::Char('+' | '=') => self.nudge("volume", 1),
            KeyCode::Char('-') => self.nudge("volume", -1),
            KeyCode::Char('0') => self.centre_balance(),
            KeyCode::Char('f') => self.flat(),
            KeyCode::Char('C') => self.flip_side(),
            KeyCode::Char('p') => self.open_presets(),
            _ => Vec::new(),
        }
    }

    /// Open the preset list: the tone control's on a device that has one, the device's own on
    /// a device that keeps presets itself, and nothing on any other.
    pub(super) fn open_presets(&mut self) -> Vec<Intent> {
        match self.profile.eq.is_some() || self.on_computer() {
            true => presets::open(self),
            false => device_presets::open(self),
        }
    }

    /// Set the balance to its centre, from wherever the keys are on the device screen, or ask
    /// for nothing on a device with no balance.
    fn centre_balance(&self) -> Vec<Intent> {
        match self.balance() {
            Some(control) => control
                .center
                .map(|center| vec![set(&control.key, center.to_string())])
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// The balance the device screen's rows carry: the first level the profile gives a centre,
    /// which is what the row draws as a track with a marker rather than as a bar.
    pub(crate) fn balance(&self) -> Option<&Control> {
        self.row_controls()
            .into_iter()
            .find(|control| control.kind == ControlKind::Level && control.center.is_some())
    }

    /// Turn the device's mute the other way, from wherever the keys are: the one control a key
    /// reaches on every screen, since a room that has to go quiet cannot wait for the screen
    /// that draws the row.
    pub(super) fn mute(&self) -> Vec<Intent> {
        match self.on_computer() {
            true => self.computer_mute(),
            false => self.flip_key("mute"),
        }
    }

    /// Whether `control` is the volume row, which shows the computer's volume while the computer's
    /// side is shown.
    pub(crate) fn is_computer_volume(&self, control: &Control) -> bool {
        self.on_computer() && self.volume().is_some_and(|volume| volume.id == control.id)
    }

    /// Mute the computer's default sink, or unmute it, once its volume has been read.
    fn computer_mute(&self) -> Vec<Intent> {
        match self.host.volume() {
            Some(_) => vec![Intent::HostMute],
            None => Vec::new(),
        }
    }

    /// Turn the toggle the profile names `key` the other way, or nothing where it has none.
    pub(super) fn flip_key(&self, key: &str) -> Vec<Intent> {
        self.profile
            .control_by_key(key)
            .map_or_else(Vec::new, |control| self.flip(control))
    }

    /// Whether this device has a toggle named `key` for a key to turn: a profile that names its
    /// own outputs one at a time has no single mute to turn, and a toast says so.
    pub(crate) fn flips(&self, key: &str) -> bool {
        self.profile
            .control_by_key(key)
            .is_some_and(|control| control.kind == ControlKind::Toggle)
    }

    /// Whether the device `b` would identify offers the cue: the one on show, and on the Devices
    /// list the one under the cursor, by the profile its row names (the profile on show where
    /// that is the one, else a built-in's; a name neither knows offers it, and the daemon says if
    /// it does not).
    pub(crate) fn identify_offered(&self) -> bool {
        let row = (self.screen == Screen::Devices)
            .then(|| self.devices.rows.get(self.selected))
            .flatten()
            .and_then(|row| row.profile.as_deref());
        match row {
            Some(name) if name != self.profile.name => {
                built_in(name).is_none_or(|profile| profile.identify)
            }
            _ => self.profile.identify,
        }
    }

    /// Whether `Space` and `Enter` have something to press on the device screen: a toggle among
    /// the control rows, or the Host group's rows while they show.
    pub(crate) fn has_toggle_or_host_rows(&self) -> bool {
        self.host_shown()
            || self
                .row_controls()
                .iter()
                .any(|control| control.kind == ControlKind::Toggle)
    }

    /// Whether this device has a mute for the key that turns it to reach.
    pub(crate) fn mutes(&self) -> bool {
        self.flips("mute")
    }

    /// The device's volume control, the level the `+` and `-` keys move.
    pub fn volume(&self) -> Option<&Control> {
        roles::volume(&self.profile)
    }

    /// How many rows the Controls pane's cursor moves through: the device's own, and under
    /// them the Host group's while host audio shows.
    pub(crate) fn controls_len(&self) -> usize {
        let host = match self.host_shown() {
            true => self.host_rows().len(),
            false => 0,
        };
        self.row_count() + host
    }

    /// The path the sound takes from the player to this device, judged: only for a DAC, which
    /// is what takes sound over the computer's audio (an amp or a preamp has no path), and while
    /// the daemon speaks for the host, so the row does not come and go as playback does. With no
    /// player it says so, and with a player and no route read yet it says that, as a line that
    /// names what is not known.
    pub(crate) fn host_path(&self) -> Option<PathLine> {
        if !self.host_shown() || self.profile.family != Family::Dac {
            return None;
        }
        let daemon = self.host.daemon.as_ref()?;
        let unknown = |words: &str| PathLine {
            hops: Vec::new(),
            verdict: Verdict::Unknown,
            words: vec![words.to_string()],
        };
        if self.host.playing.is_none() {
            return Some(unknown(NOTHING_PLAYING));
        }
        match daemon.route.as_deref() {
            Some(route) => Some(path_line(route, &self.dac_facts())),
            None => Some(unknown(ROUTE_NOT_READ)),
        }
    }

    /// What the device reports of its end of the path: its name, the input it reads, the sample
    /// rate it reads and which USB ports are connected, as far as the profile names them and the
    /// device has said.
    fn dac_facts(&self) -> DacFacts {
        DacFacts::read(&self.profile, |id| {
            self.readings.values.get(&id)?.raw_or_flag()
        })
    }

    /// The Host group's rows, in the order the Controls pane draws them under the device's own:
    /// the computer's volume where the device has no volume control to carry the switch, then the
    /// transport.
    pub(crate) fn host_rows(&self) -> Vec<HostRow> {
        let mut rows = Vec::new();
        if self.computer_offered() && self.volume().is_none() {
            rows.push(HostRow::Volume);
        }
        rows.push(HostRow::Transport);
        rows
    }

    /// The Host group's row the Controls pane has selected, whichever pane the keys are going
    /// to, or `None` while it has one of the device's own selected or host audio is not shown.
    pub(crate) fn host_row(&self) -> Option<HostRow> {
        if !self.host_shown() {
            return None;
        }
        let at = self.selected.checked_sub(self.row_count())?;
        self.host_rows().get(at).copied()
    }

    /// The Host group's row the keys act on: the one the Controls pane has selected, while the
    /// keys are in the Controls pane.
    fn focused_host_row(&self) -> Option<HostRow> {
        (self.pane == Pane::Controls)
            .then(|| self.host_row())
            .flatten()
    }

    /// Step the Host group's row `row` one value: the Transport row's cursor a button along.
    fn step_host(&mut self, row: HostRow, dir: i64) -> Vec<Intent> {
        match row {
            HostRow::Volume if self.on_computer() => self.step_host_volume(dir),
            HostRow::Volume => Vec::new(),
            HostRow::Transport => {
                self.host.move_button(dir);
                Vec::new()
            }
        }
    }

    /// The computer's volume [`host::VOLUME_STEP`] up or down, once it has been read.
    pub(crate) fn step_host_volume(&self, dir: i64) -> Vec<Intent> {
        self.host
            .stepped(dir)
            .map(Intent::HostVolume)
            .into_iter()
            .collect()
    }

    /// What `Space` and `Enter` do on the Host group's row `row`: the button under the Transport
    /// row's cursor pressed — which, with no player to take it, a toast says rather than sends.
    fn act_on_host(&mut self, row: HostRow) -> Vec<Intent> {
        match row {
            HostRow::Volume if self.on_computer() => self.computer_mute(),
            HostRow::Volume => Vec::new(),
            HostRow::Transport if self.host.playing.is_some() => {
                vec![Intent::HostPress(self.host.button())]
            }
            HostRow::Transport => {
                self.notify(host::NO_PLAYER);
                Vec::new()
            }
        }
    }

    /// Press transport button `button` (an index into [`Button::ALL`]) the way a click on it
    /// does: the Transport row focused with its cursor on the button, and the button pressed as
    /// `Enter` presses it. Asks for nothing while host audio is not shown.
    pub(crate) fn click_transport(&mut self, button: usize) -> Vec<Intent> {
        let Some(at) = self
            .host_rows()
            .iter()
            .position(|row| *row == HostRow::Transport)
        else {
            return Vec::new();
        };
        if !self.host_shown() || button >= Button::ALL.len() {
            return Vec::new();
        }
        let row = self.row_count() + at;
        self.focus(Pane::Controls, row);
        self.host.button = button;
        self.act_on_host(HostRow::Transport)
    }

    /// Step whatever the device screen has focused one value: a control of the Controls pane,
    /// or one of the Tone pane's own rows.
    fn step_focused(&mut self, dir: i64) -> Vec<Intent> {
        if let Some(row) = self.focused_host_row() {
            return self.step_host(row, dir);
        }
        match self.pane {
            Pane::Tone => self.step_tone(dir),
            _ => self.step_selected(dir),
        }
    }

    /// Step the focused control one value: a select through the values the profile names, a
    /// level by one step, a toggle the other way. The volume steps the computer's while the
    /// computer's side is shown.
    fn step_selected(&self, dir: i64) -> Vec<Intent> {
        let Some(control) = self.focused_control() else {
            return Vec::new();
        };
        if self.is_computer_volume(control) {
            return self.step_host_volume(dir);
        }
        match control.kind {
            ControlKind::Select => self.step_select(control, dir),
            ControlKind::Level => self.step_level(control, dir),
            ControlKind::Toggle => self.flip(control),
            _ => Vec::new(),
        }
    }

    /// The value next to the one a select reads now, or nothing at either end.
    fn step_select(&self, control: &Control, dir: i64) -> Vec<Intent> {
        if control.kind != ControlKind::Select {
            return Vec::new();
        }
        let Some(current) = self.raw_of(control.id) else {
            return Vec::new();
        };
        let mut values = control.values.iter();
        let next = if dir > 0 {
            values.find(|(raw, _)| i64::from(*raw) > current)
        } else {
            values.rev().find(|(raw, _)| i64::from(*raw) < current)
        };
        next.map(|(raw, _)| vec![set(&control.key, raw.to_string())])
            .unwrap_or_default()
    }

    /// Turn a toggle the other way from the value it reads now.
    fn flip(&self, control: &Control) -> Vec<Intent> {
        if control.kind != ControlKind::Toggle {
            return Vec::new();
        }
        let on = self
            .readings
            .values
            .get(&control.id)
            .is_some_and(Reading::is_active);
        vec![set(&control.key, if on { "off" } else { "on" })]
    }

    /// Nudge a level control one step; nothing for any other kind.
    fn step_level(&self, control: &Control, dir: i64) -> Vec<Intent> {
        match control.kind {
            ControlKind::Level => vec![set(&control.key, updown(dir))],
            _ => Vec::new(),
        }
    }

    /// Nudge a level control by one step, wherever the selection is.
    fn nudge(&self, key: &str, dir: i64) -> Vec<Intent> {
        self.profile
            .control_by_key(key)
            .map_or_else(Vec::new, |control| self.step_level(control, dir))
    }

    /// Flip the toggle on control row `i`, the way clicking its box does. Asks for nothing for
    /// a row that is not a toggle, or that does not exist.
    pub(crate) fn flip_row(&self, i: usize) -> Vec<Intent> {
        self.rows()
            .get(i)
            .map_or_else(Vec::new, |row| self.flip(row.control))
    }

    /// Step control row `i`'s select one value, the way clicking `‹` (`dir < 0`) or `›`
    /// (`dir > 0`) does. Asks for nothing for a row that is not a select.
    pub(crate) fn step_row(&self, i: usize, dir: i64) -> Vec<Intent> {
        self.rows()
            .get(i)
            .map_or_else(Vec::new, |row| self.step_select(row.control, dir))
    }

    /// Nudge control row `i`'s level one step, the way the wheel over its bar does, or the
    /// computer's volume for the volume row while the computer's side is shown. Asks for nothing for a row that is not
    /// a level — which a balance control is too.
    pub(crate) fn nudge_row(&self, i: usize, dir: i64) -> Vec<Intent> {
        let rows = self.rows();
        match rows.get(i) {
            Some(row) if self.is_computer_volume(row.control) => self.step_host_volume(dir),
            Some(row) => self.step_level(row.control, dir),
            None => match i
                .checked_sub(rows.len())
                .and_then(|at| self.host_rows().get(at).copied())
            {
                Some(HostRow::Volume) if self.on_computer() => self.step_host_volume(dir),
                _ => Vec::new(),
            },
        }
    }

    /// The device screen's control rows: every control a user can change, with the last value
    /// each confirmed — the selects and toggles first, then the levels, each group in
    /// function-address order (see [`row_group`]).
    pub fn rows(&self) -> Vec<ControlRow<'_>> {
        self.row_controls()
            .into_iter()
            .map(|control| ControlRow {
                reading: self.readings.values.get(&control.id),
                control,
                stale: !self.session.connected,
            })
            .collect()
    }

    /// The controls the device screen lists, in profile order: every one a user can change.
    fn listed_controls(&self) -> impl Iterator<Item = &Control> {
        self.profile
            .controls
            .iter()
            .filter(|control| control.kind.writable())
    }

    /// How many control rows the device screen lists, counted without building them.
    fn row_count(&self) -> usize {
        self.listed_controls().count()
    }

    /// The controls the device screen lists, in the order it lists them.
    fn row_controls(&self) -> Vec<&Control> {
        let mut controls: Vec<&Control> = self.listed_controls().collect();
        controls.sort_unstable_by_key(|control| (row_group(control), control.id));
        controls
    }

    /// The control the Controls pane has focused, whichever pane the keys are going to.
    pub fn selected_control(&self) -> Option<&Control> {
        self.row_controls().get(self.selected).copied()
    }

    /// The control a key acts on: the Controls pane's own selection, and nothing at all
    /// while another pane of the screen's own has the focus, whose rows are not the control
    /// rows.
    fn focused_control(&self) -> Option<&Control> {
        (self.pane == Pane::Controls)
            .then(|| self.selected_control())
            .flatten()
    }
}

/// Which of the device screen's groups of rows `control` belongs to, in the order the groups
/// are listed: the selects and toggles, the levels — the balance among them — and the meters
/// the device streams. The Controls pane sets the groups a line apart where it has room, and the
/// balance apart from the other levels.
pub(crate) fn row_group(control: &Control) -> u8 {
    match control.kind {
        ControlKind::Level => 1,
        ControlKind::Meter => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests;
