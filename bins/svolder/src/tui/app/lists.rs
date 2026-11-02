// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The keys of the list screens: Telemetry's selection and raw nudge, the device list's
//! switch and edits, and the Settings box with each row turned or stepped through its choices.

use crossterm::event::KeyCode;
use svolder_ipc::config::{HOST_AUDIO, RefreshWeeks};
use svolder_ipc::{ColorDepth, TuiSettingKind, TuiValue};
use svolder_profiles::{ControlKind, Family};
use svolder_proto::{FunctionId, ValueType};

use crate::tui::boxes::editor;
use crate::tui::keys;
use crate::tui::palette;
use crate::tui::screens::settings::{self, Setting};
use crate::tui::theme;

use super::{App, Intent, SettingsBox, TuiChange, cycled, stepped};

impl App {
    /// The telemetry screen's keys: the selection, and a raw nudge of the entries no
    /// control of its own can set.
    pub(super) fn telemetry_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let len = self.readings.values.len();
        match code {
            KeyCode::Up => self.move_row(-1, len),
            KeyCode::Down => self.move_row(1, len),
            KeyCode::Left => self.step_raw(-1),
            KeyCode::Right => self.step_raw(1),
            _ => Vec::new(),
        }
    }

    /// The reading the telemetry screen has focused, if it has any.
    pub(super) fn telemetry_focus(&self) -> Option<FunctionId> {
        self.readings.values.keys().nth(self.selected).copied()
    }

    /// Nudge the focused telemetry entry by one raw step, held inside what its wire type
    /// can hold. An entry a writable control names is left to that control's own keys, and a
    /// meter is only read.
    fn step_raw(&self, delta: i64) -> Vec<Intent> {
        let Some((id, reading)) = self.readings.values.iter().nth(self.selected) else {
            return Vec::new();
        };
        if self
            .profile
            .control_by_id(*id)
            .is_some_and(|control| control.kind.writable() || control.kind == ControlKind::Meter)
        {
            return Vec::new();
        }
        let Some(current) = reading.raw_i64() else {
            return Vec::new();
        };
        let Some((lo, hi)) = type_range(&reading.value.value_type) else {
            return Vec::new();
        };
        let value = current.saturating_add(delta).clamp(lo, hi);
        if value == current {
            return Vec::new();
        }
        vec![Intent::Raw {
            id: id.to_string(),
            value_type: reading.value.value_type.clone(),
            value: value.to_string(),
        }]
    }

    /// The device list's keys: the selection, the device to watch instead, and the three that
    /// change what the configuration file says — a new device, the selected one's settings, and
    /// taking it away.
    pub(super) fn devices_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let len = self.devices.rows.len();
        match code {
            KeyCode::Up => self.move_row(-1, len),
            KeyCode::Down => self.move_row(1, len),
            KeyCode::Enter => self
                .devices
                .rows
                .get(self.selected)
                .map(|device| vec![Intent::SwitchDevice(device.name.clone())])
                .unwrap_or_default(),
            KeyCode::Char('n') => vec![Intent::NewDevice],
            KeyCode::Char('u') => {
                editor::open(self);
                Vec::new()
            }
            KeyCode::Char('x') => {
                editor::ask(self);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Open the Settings box over the screen being shown, on the category and row it was last
    /// closed on.
    pub fn open_settings(&mut self) {
        self.overlays.settings = Some(self.overlays.settings_memory);
    }

    /// Close the Settings box onto the screen under it, remembering where it was.
    fn close_settings(&mut self) {
        if let Some(at) = self.overlays.settings.take() {
            self.overlays.settings_memory = at;
        }
    }

    /// The Settings box's keys, which it takes whole: `↑↓` move within the category, `←→` step
    /// a row through its choices or flip a switch the same as `Space` or `Enter` does, `Tab`
    /// and `1`-`5` choose the category, an uppercase toggle turns its row, `Esc` closes the box,
    /// `?` opens the help overlay over it and `q` asks to quit. Every other key does nothing,
    /// and none reaches the screen under it.
    ///
    /// Every change is one [`Intent::Tui`], which the loop applies to the running theme and
    /// animation layer and writes back to the config file.
    pub(super) fn settings_box_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let Some(at) = self.overlays.settings else {
            return Vec::new();
        };
        if self.overlays.conn.modal.is_some() {
            return self.connection_modal_key(code);
        }
        let categories = settings::CATEGORIES.len();
        let change = |category: usize| SettingsBox { category, row: 0 };
        match code {
            KeyCode::Esc => self.close_settings(),
            KeyCode::Char('q') => return self.quit(),
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Up | KeyCode::Down => {
                let delta = if code == KeyCode::Up { -1 } else { 1 };
                let len = settings::rows(at.category).len();
                self.overlays.settings = Some(SettingsBox {
                    row: stepped(at.row, delta, len),
                    ..at
                });
            }
            KeyCode::Tab => {
                self.overlays.settings = Some(change(cycled(at.category, 1, categories)))
            }
            KeyCode::BackTab => {
                self.overlays.settings = Some(change(cycled(at.category, -1, categories)))
            }
            KeyCode::Char(digit @ '1'..='9') => {
                let category = usize::from(digit as u8 - b'1');
                if category < categories {
                    self.overlays.settings = Some(change(category));
                }
            }
            KeyCode::Char(key @ ('v' | 'c' | 'R'))
                if self.selected_setting() == Some(Setting::McpToken) =>
            {
                return self.token_key(key);
            }
            KeyCode::Char(' ') | KeyCode::Enter => return self.toggle_setting(),
            KeyCode::Left => return self.step_setting(-1),
            KeyCode::Right => return self.step_setting(1),
            code => {
                if let Some(setting) = keys::setting_turned_by(code) {
                    return self.turned(setting).map(Intent::Tui).into_iter().collect();
                }
            }
        }
        Vec::new()
    }

    /// The row the Settings box has selected.
    fn selected_setting(&self) -> Option<Setting> {
        self.overlays.settings.map(settings::current)
    }

    /// Turn the selected setting the other way, or ask for nothing on a row that steps through
    /// choices rather than turning.
    fn toggle_setting(&mut self) -> Vec<Intent> {
        if let Some(setting) = self.selected_setting().filter(|s| s.is_connection()) {
            return self.connection_step(setting);
        }
        self.selected_setting()
            .and_then(|setting| self.turned(setting))
            .map(Intent::Tui)
            .into_iter()
            .collect()
    }

    /// `setting` turned the other way from what it says now, or `None` for a row that steps
    /// through choices rather than turning. Mouse capture turns from what is on rather than
    /// from what the file asked for, which is what its row says.
    pub(super) fn turned(&self, setting: Setting) -> Option<TuiChange> {
        let effects = &self.tui.effects;
        Some(match setting {
            Setting::Key(key) => TuiChange::Set(key, TuiValue::Bool(!key.flag(&self.tui)?)),
            Setting::Effects => TuiChange::Effects(!effects.enabled),
            Setting::Effect(switch) => TuiChange::Effect(switch, !switch.of(effects)),
            Setting::HueDrift => TuiChange::HueDrift(!effects.hue_drift),
            Setting::SpectrumDrift => TuiChange::SpectrumDrift(!effects.spectrum_drift),
            Setting::Mouse => TuiChange::Mouse(!self.mouse),
            Setting::HostAudio => TuiChange::Set(&HOST_AUDIO, TuiValue::Bool(!self.tui.host_audio)),
            _ => return None,
        })
    }

    /// Step the focused setting one choice round its own cycle, or flip it — either arrow the
    /// same as the other — on a row that turns rather than stepping, as the Controls pane's own
    /// toggles flip on `←→` too ([`App::step_selected`], `ControlKind::Toggle => flip`).
    fn step_setting(&mut self, dir: isize) -> Vec<Intent> {
        let Some(setting) = self.selected_setting() else {
            return Vec::new();
        };
        if setting.is_connection() {
            return self.connection_step(setting);
        }
        let change = match setting {
            Setting::Key(key) => {
                let TuiSettingKind::Word { words, .. } = &key.kind else {
                    return self.turned(setting).map(Intent::Tui).into_iter().collect();
                };
                let words = words();
                let now = key.word(&self.tui);
                let at = words.iter().position(|word| Some(*word) == now);
                TuiChange::Set(
                    key,
                    TuiValue::Word(words[cycled(at.unwrap_or(0), dir, words.len())]),
                )
            }
            Setting::AutoEqRefresh => {
                return refresh_stepped(self.presets.autoeq_weeks, dir)
                    .map(Intent::AutoEqRefresh)
                    .into_iter()
                    .collect();
            }
            Setting::Palette => {
                let names = theme::palettes();
                let at = names.iter().position(|name| *name == self.tui.palette);
                TuiChange::Palette(names[cycled(at.unwrap_or(0), dir, names.len())].to_string())
            }
            Setting::Accent(family) => {
                let now = self.tui.accent.clone().or_else(|| {
                    settings::family_accent(&self.tui.accents, family).map(str::to_string)
                });
                let stops = self.accent_stops(family, now.as_deref());
                let at = stops
                    .iter()
                    .position(|stop| same_colour(stop.as_deref(), now.as_deref()));
                TuiChange::Accent(
                    family,
                    stops[cycled(at.unwrap_or(0), dir, stops.len())].clone(),
                )
            }
            Setting::Colors => {
                let at = DEPTHS.iter().position(|depth| *depth == self.tui.colors);
                TuiChange::Colors(DEPTHS[cycled(at.unwrap_or(0), dir, DEPTHS.len())])
            }
            _ => return self.turned(setting).map(Intent::Tui).into_iter().collect(),
        };
        vec![Intent::Tui(change)]
    }

    /// The stops one family's accent row steps through: a colour of the user's own first, when
    /// the row's colour is one the palette does not offer (written by hand, another palette's, or
    /// a bare `accent`'s) — held from the first step away so the arrows step back to it — then the
    /// preset's own colour and the two the palette gives the other two families.
    fn accent_stops(&mut self, family: Family, now: Option<&str>) -> Vec<Option<String>> {
        let offered = |colour: &str, stops: &[Option<String>]| {
            stops
                .iter()
                .flatten()
                .any(|stop| stop.eq_ignore_ascii_case(colour))
        };
        // A bare `accent` colours every family and a step clears it, so all three hold it.
        let holders = match self.tui.accent {
            Some(_) => vec![Family::Dac, Family::Amp, Family::Preamp],
            None => vec![family],
        };
        if let Some(own) = now {
            for holder in holders {
                if !offered(own, &self.accent_choices(holder)) {
                    self.overlays
                        .held_accents
                        .set(holder, Some(own.to_string()));
                }
            }
        }
        let mut stops = self.accent_choices(family);
        if let Some(held) = self.overlays.held_accents.get(family)
            && !offered(held, &stops)
        {
            stops.insert(0, Some(held.to_string()));
        }
        stops
    }

    /// The accents the palette itself offers one family's row: the preset's own colour, and then
    /// the two colours this palette gives the other two families.
    fn accent_choices(&self, family: Family) -> Vec<Option<String>> {
        let mut choices = vec![None];
        let Some(preset) = theme::preset(&self.tui.palette) else {
            return choices;
        };
        for other in [Family::Dac, Family::Amp, Family::Preamp] {
            if other != family {
                choices.push(Some(palette::hex_text(preset.family(other))));
            }
        }
        choices
    }
}

/// Whether two accent values name the same colour, a `#rrggbb` in either case.
fn same_colour(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (None, None) => true,
        _ => false,
    }
}

/// The most weeks the AutoEq refresh row steps to.
const REFRESH_MAX: isize = 99;

/// The AutoEq refresh one step `dir` from `now`: `never` below `1 week`, `99 weeks` the top. It
/// stops at both ends rather than wrapping, and answers `None` at one, where nothing changes.
pub(crate) fn refresh_stepped(now: RefreshWeeks, dir: isize) -> Option<RefreshWeeks> {
    let at = match now {
        RefreshWeeks::Never => 0,
        RefreshWeeks::Weeks(weeks) => isize::from(weeks),
    };
    let to = (at + dir.signum()).clamp(0, REFRESH_MAX);
    if to == at {
        return None;
    }
    Some(match u8::try_from(to) {
        Ok(weeks @ 1..) => RefreshWeeks::Weeks(weeks),
        _ => RefreshWeeks::Never,
    })
}

/// The colour depths the Settings box steps through, in the order it steps them.
const DEPTHS: [ColorDepth; 4] = [
    ColorDepth::Auto,
    ColorDepth::Ansi16,
    ColorDepth::Ansi256,
    ColorDepth::TrueColor,
];

/// What a wire type can hold, as far as an `i64` reaches; `None` for the types a raw nudge
/// has no numbers to move through.
fn type_range(value_type: &str) -> Option<(i64, i64)> {
    match value_type.parse::<ValueType>().ok()? {
        ValueType::U8 => Some((0, i64::from(u8::MAX))),
        ValueType::U16 => Some((0, i64::from(u16::MAX))),
        ValueType::U32 => Some((0, i64::from(u32::MAX))),
        ValueType::U64 => Some((0, i64::MAX)),
        ValueType::I8 => Some((i64::from(i8::MIN), i64::from(i8::MAX))),
        ValueType::I16 => Some((i64::from(i16::MIN), i64::from(i16::MAX))),
        ValueType::I32 => Some((i64::from(i32::MIN), i64::from(i32::MAX))),
        ValueType::I64 => Some((i64::MIN, i64::MAX)),
        ValueType::Bool | ValueType::Data => None,
    }
}
