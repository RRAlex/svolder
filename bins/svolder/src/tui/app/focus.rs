// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the cursor is: the focused pane, row and band, what a click focuses, the moves `Tab`
//! and the arrows make, and the rows the trail and the sweep play over.

use svolder_profiles::{EqParam, EqSpec};
use svolder_proto::FunctionId;

use super::{App, Intent, Pane, Screen, ToneRow, cycled};

/// Whether `intent` writes a value the user chose on a row of a list rather than on a control
/// row of the device: a setting, the connection, a device's place in the list, a preset or an
/// AutoEq correction.
fn writes_from_a_list(intent: &Intent) -> bool {
    matches!(
        intent,
        Intent::Tui(_)
            | Intent::AutoEqRefresh(_)
            | Intent::Connection(_)
            | Intent::DefaultDevice(_)
            | Intent::UpdateDevice(..)
            | Intent::SavePreset(_)
            | Intent::UpdatePreset(..)
            | Intent::RenamePreset(..)
            | Intent::RemovePreset(_)
            | Intent::AutoEqDownload(_)
    )
}

impl App {
    /// Note the row `intents` write to, for the sweep the user's own change earns: the first
    /// of them that names a row, and nothing at all for a press that writes no value.
    ///
    /// Every path the user changes a value by comes through here — the keys, a click on a
    /// word of a border, and the mouse — so the sweep answers what they did rather than what the
    /// device echoed back: a reading the device pushes by itself names no row here.
    pub(crate) fn touched(&mut self, intents: &[Intent]) {
        if let Some(id) = intents.iter().find_map(|intent| self.written_row(intent)) {
            self.frame_flags.touched_row = Some(id);
        }
        self.frame_flags.touched_cursor |= intents.iter().any(writes_from_a_list);
    }

    /// The row one intent writes to: the control its key names, the address a raw write
    /// names, the band a parameter belongs to — the band row is found by its gain, as
    /// [`App::cursor_id`] finds it — or the tone control's mode row. Nothing for the intents
    /// that write no value to the device.
    fn written_row(&self, intent: &Intent) -> Option<FunctionId> {
        match intent {
            Intent::Set { key, .. } => self.profile.control_by_key(key).map(|control| control.id),
            Intent::Raw { id, .. } => id.parse().ok(),
            Intent::EqSet { band, param, .. } => {
                let eq = self.profile.eq.as_ref()?;
                eq.band_id(band.checked_sub(1)?, param.parse().ok()?)
            }
            Intent::EqMode(_) => self.profile.eq.as_ref().map(EqSpec::mode_id),
            Intent::Recalled(_)
            | Intent::Identify(_)
            | Intent::Refresh
            | Intent::SwitchDevice(_)
            | Intent::NewDevice
            | Intent::UpdateDevice(..)
            | Intent::RemoveDevice(_)
            | Intent::DefaultDevice(_)
            | Intent::SavePreset(_)
            | Intent::UpdatePreset(..)
            | Intent::RenamePreset(..)
            | Intent::RemovePreset(_)
            | Intent::ReadPresets
            | Intent::AutoEqIndex { .. }
            | Intent::AutoEqDownload(_)
            | Intent::HostEq(_)
            | Intent::MoveToDac(_)
            | Intent::Tui(_)
            | Intent::AutoEqRefresh(_)
            | Intent::Connection(_)
            | Intent::RevealToken
            | Intent::CopyToken
            | Intent::RotateToken
            | Intent::WebLink
            | Intent::WebPair
            | Intent::CopyWebLink
            | Intent::RotateWebToken
            | Intent::OpenWebLink
            | Intent::HostVolume(_)
            | Intent::HostMute
            | Intent::HostPress(_)
            | Intent::Quit => None,
        }
    }

    /// The row or band the cursor points at, which is the one a trail and the shimmer play
    /// over: the row the keys are acting on, so on the device screen it follows the focused
    /// pane as the keys do.
    ///
    /// `None` where there is no cursor to speak of: the Status pane, whose lines no key moves
    /// through, and the screens whose rows no function names.
    pub fn cursor_id(&self) -> Option<FunctionId> {
        match (self.screen, self.pane) {
            (Screen::Device, Pane::Controls) => self.selected_control().map(|control| control.id),
            (Screen::Device, Pane::Tone) | (Screen::Tone, _) => match self.tone.row {
                ToneRow::Mode => self.profile.eq.as_ref().map(EqSpec::mode_id),
                ToneRow::Loudness => self.profile.eq.as_ref().map(EqSpec::loudness_id),
                ToneRow::Band => self.band_cursor(),
                ToneRow::Point(_) => None,
            },
            (Screen::Device, Pane::Status) => None,
            (Screen::Telemetry | Screen::Devices | Screen::Settings | Screen::Setup, _) => None,
        }
    }

    /// The function the focused band's row is found by: its gain, one of the three values the
    /// row shows.
    fn band_cursor(&self) -> Option<FunctionId> {
        self.profile
            .eq
            .as_ref()
            .and_then(|eq| eq.band_id(self.tone.band, EqParam::Gain))
    }

    /// Focus pane `pane`'s row `row`, the way a click on it does, noting the row the cursor
    /// left for the trail it earns and the pane it left for the light the new one earns.
    pub(crate) fn focus(&mut self, pane: Pane, row: usize) {
        let from = self.cursor_id();
        self.frame_flags.pane_changed |= self.pane != pane;
        self.pane = pane;
        self.selected = row;
        self.cursor_moved(from);
    }

    /// Focus band `band`, the way a click on its row does, noting the row the cursor left.
    ///
    /// A band is a row of the Tone pane, so focusing one takes the pane's cursor off the mode
    /// and loudness rows above it: the arrows then step the band that was clicked. On the
    /// device screen it takes the keys to the Tone pane as well ([`App::focus_tone_pane`]).
    pub(crate) fn focus_band(&mut self, band: usize) {
        if self.on_computer() {
            self.focus_point(band);
            return;
        }
        let from = self.cursor_id();
        self.focus_tone_pane();
        self.tone.band = band;
        self.tone.row = ToneRow::Band;
        self.cursor_moved(from);
    }

    /// On the device screen, take the keys to the Tone pane, the way a click on one of its rows
    /// does, earning the light that runs round the pane landed in as `Tab` does; the
    /// tone-control screen is the Tone pane written large, with no other pane to leave.
    fn focus_tone_pane(&mut self) {
        if self.screen == Screen::Device {
            self.frame_flags.pane_changed |= self.pane != Pane::Tone;
            self.pane = Pane::Tone;
        }
    }

    /// Focus the tone control's Loudness half, as a click on its bar does: no toggle.
    pub(crate) fn focus_loudness(&mut self) {
        let from = self.cursor_id();
        self.focus_tone_pane();
        self.tone.row = ToneRow::Loudness;
        self.note_tone_header(ToneRow::Loudness);
        self.cursor_moved(from);
    }

    /// Focus half `row` of the tone control's header, or of the Tone pane's own rows on the
    /// device screen — taking the keys to that pane there ([`App::focus_tone_pane`]) — and carry
    /// out what a click there does: toggle the flat mode for Mode, off its `‹ ›` arrows, or the
    /// loudness mode for Loudness, anywhere on it but the bar — the word, the value; a press on
    /// the bar sets the strength ([`glide`](super::glide)) and the wheel over it steps it
    /// ([`App::step_loudness`]), neither reaching here.
    /// Never called with `Band`, which a click on a band row reaches through
    /// [`App::focus_band`] instead.
    pub(crate) fn click_tone_row(&mut self, row: ToneRow) -> Vec<Intent> {
        let from = self.cursor_id();
        self.focus_tone_pane();
        self.tone.row = row;
        self.note_tone_header(row);
        self.cursor_moved(from);
        match row {
            ToneRow::Mode => self.flat(),
            ToneRow::Loudness => self.loudness(),
            ToneRow::Band | ToneRow::Point(_) => Vec::new(),
        }
    }

    /// Note that the cursor has left `from`, for the trail that row earns, unless it is still
    /// sitting where it was.
    pub(super) fn cursor_moved(&mut self, from: Option<FunctionId>) {
        if self.cursor_id() != from {
            self.frame_flags.left_row = from;
        }
    }

    /// Move the focus `dir` panes along the screen being shown, round the ends: forward for
    /// `Tab` and back for Shift-Tab. A screen with only the one pane has nothing to cycle
    /// through, so the keys stay put and say nothing.
    ///
    /// The tone screen is its own case: `Tab` there has two stops, the band table and the
    /// header, and with only the two either direction goes to the other one. Landing on the
    /// header puts the cursor on the half it was last on (Mode the first time); landing on the
    /// band table puts it back on the band it was on.
    ///
    /// The pane landed in earns the light that runs round its contour, unless the keys have
    /// gone nowhere.
    pub(super) fn cycle_pane(&mut self, dir: isize) -> Vec<Intent> {
        if self.screen == Screen::Tone {
            self.tone.row = match self.tone.row {
                ToneRow::Band => match self.tone.header_loudness {
                    true => ToneRow::Loudness,
                    false => ToneRow::Mode,
                },
                ToneRow::Mode | ToneRow::Loudness => match self.on_computer() {
                    true => ToneRow::Point(self.tone.point),
                    false => ToneRow::Band,
                },
                ToneRow::Point(_) => ToneRow::Mode,
            };
            self.frame_flags.pane_changed = true;
            return Vec::new();
        }
        let panes = self.panes();
        let at = panes.iter().position(|p| *p == self.pane).unwrap_or(0);
        let next = panes[cycled(at, dir, panes.len())];
        self.frame_flags.pane_changed |= self.pane != next;
        self.pane = next;
        Vec::new()
    }

    /// The panes `Tab` moves through on the screen being shown, in order, the first of them
    /// being the body a screen opens focused on.
    ///
    /// The device screen has the Controls pane and, on a device with tone control, the Tone
    /// pane; the Status pane has no rows for a key to move through, so it is never focused.
    /// Every other screen has the one body its rows are in.
    pub(crate) fn panes(&self) -> Vec<Pane> {
        match self.screen {
            Screen::Device => {
                let mut panes = vec![Pane::Controls];
                if self.profile.eq.is_some() {
                    panes.push(Pane::Tone);
                }
                panes
            }
            Screen::Tone => vec![Pane::Tone],
            Screen::Telemetry | Screen::Devices | Screen::Settings | Screen::Setup => {
                vec![Pane::Controls]
            }
        }
    }

    /// Move the focused pane's own selection — the control rows, or the bands — and ask for
    /// nothing. The status lines have no rows to move through.
    pub(super) fn move_selection(&mut self, delta: isize) -> Vec<Intent> {
        // The tone-control screen is the Tone pane written large: its rows are the tone
        // control's own wherever the focus happens to be.
        if self.screen == Screen::Tone {
            self.move_tone_row(delta);
            return Vec::new();
        }
        match self.pane {
            Pane::Controls => return self.move_row(delta, self.controls_len()),
            Pane::Tone => self.move_tone_row(delta),
            Pane::Status => {}
        }
        Vec::new()
    }
}
