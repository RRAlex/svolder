// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's stages on the tone screen: the strip of four stops under the Response block
//! (EQ, Loudness, Space, Bypass), the one panel that stands in place of the Points pane, and the
//! keys that move through both.
//!
//! The keys are in one place at a time ([`ToneRow`]): the header (Mode), a stop of the strip
//! ([`ToneRow::Strip`]), or a row of the open stage's panel ([`ToneRow::Point`] for the EQ,
//! [`ToneRow::Ld`] for Loudness, [`ToneRow::Crossfeed`] for Space, [`ToneRow::Bypass`]). `Tab`
//! and `⇧Tab` step to the next and previous stage and open its panel; in a panel `↑↓` move
//! between rows (`↑` on the first goes to the strip), `←→` change the row's value, `Enter` types
//! a number or applies the highlighted choice, `␣` turns the stage on or off and `Esc` goes up to
//! the strip. On the strip `←→` move between the stops, `↓` and `Enter` go into the panel, `␣`
//! turns the stop on or off and `Esc` leaves the screen. `f` is the bypass from anywhere on the
//! computer side; `L` and `X` jump to the Loudness and Space panels.
//!
//! Every change is the one the stage's own rows already send ([`HostEqOp`]): this module only
//! decides which row a key reaches.

use crossterm::event::KeyCode;
use svolder_ipc::HostEqOp;

use super::{App, Intent, Ld, Screen, Space, ToneRow, Xf};

/// One stop of the strip, in the chain's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    /// The EQ: its Points table.
    #[default]
    Eq,
    /// The loudness.
    Loudness,
    /// The space: crossfeed or the virtual speakers.
    Space,
    /// The bypass: every stage off at once.
    Bypass,
}

impl Stage {
    /// Every stage, for the tests that walk them.
    #[cfg(test)]
    pub const ALL_FOR_TEST: [Stage; 4] = [Stage::Eq, Stage::Loudness, Stage::Space, Stage::Bypass];

    /// The stop's word on the strip and the panel's title.
    pub fn label(self) -> &'static str {
        match self {
            Stage::Eq => "EQ",
            Stage::Loudness => "Loudness",
            Stage::Space => "Space",
            Stage::Bypass => "Bypass",
        }
    }
}

/// Whether `code` is one of the Points table's own keys: add, remove, type, flatten, a point's
/// number, Q and frequency.
fn points_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Char('a' | 'x' | 'y' | '0'..='9' | '[' | ']' | ',' | '.')
    )
}

impl App {
    /// Whether the tone screen is showing the stages: the computer side, with a controller that
    /// serves its view.
    pub(crate) fn stages_shown(&self) -> bool {
        self.screen == Screen::Tone && self.on_computer() && self.computer_eq().is_some()
    }

    /// The stops the strip has: the EQ always, the others where the controller serves them.
    pub(crate) fn stages(&self) -> Vec<Stage> {
        let mut stages = vec![Stage::Eq];
        if self.ld_shown() {
            stages.push(Stage::Loudness);
        }
        if self.crossfeed_shown() {
            stages.push(Stage::Space);
        }
        if self.bypass_view().is_some() {
            stages.push(Stage::Bypass);
        }
        stages
    }

    /// The stage the keys are in or were last in: the one whose panel the screen shows.
    pub(crate) fn open_stage(&self) -> Stage {
        let stage = match self.tone.row {
            ToneRow::Strip(stage) => stage,
            ToneRow::Point(_) => Stage::Eq,
            ToneRow::Ld(_) => Stage::Loudness,
            ToneRow::Crossfeed(_) => Stage::Space,
            ToneRow::Bypass => Stage::Bypass,
            ToneRow::Mode | ToneRow::Loudness | ToneRow::Band | ToneRow::Target => {
                self.tone.stage.get()
            }
        };
        // A stage the controller stopped serving leaves the EQ's panel in its place.
        let stage = match self.stages().contains(&stage) {
            true => stage,
            false => Stage::Eq,
        };
        self.tone.stage.set(stage);
        stage
    }

    /// Whether `stage` is on: what its strip stop's `●` says.
    pub(crate) fn stage_on(&self, stage: Stage) -> bool {
        match stage {
            Stage::Eq => self.computer_eq().is_some_and(|eq| eq.on),
            Stage::Loudness => self.loudness_view().is_some_and(|view| view.on),
            Stage::Space => self.space_now() != Space::Off,
            Stage::Bypass => self.bypass_view().is_some_and(|view| view.on),
        }
    }

    /// The bypass as the controller last said it.
    pub(crate) fn bypass_view(&self) -> Option<&svolder_host::eq::BypassView> {
        self.computer_eq().and_then(|eq| eq.bypass.as_ref())
    }

    /// The rows of `stage`'s panel, top to bottom, as the places the keys can be.
    pub(crate) fn panel_rows(&self, stage: Stage) -> Vec<ToneRow> {
        match stage {
            Stage::Eq => (0..self.computer_points().len().max(1))
                .map(ToneRow::Point)
                .collect(),
            Stage::Loudness => Ld::ALL.into_iter().map(ToneRow::Ld).collect(),
            Stage::Space => self
                .xf_stops()
                .into_iter()
                .map(ToneRow::Crossfeed)
                .collect(),
            Stage::Bypass => vec![ToneRow::Bypass],
        }
    }

    /// Put the keys on the strip's stop for `stage`.
    pub(crate) fn focus_strip(&mut self, stage: Stage) {
        let from = self.cursor_id();
        self.tone.xf_line = None;
        self.tone.plays = None;
        self.tone.row = ToneRow::Strip(stage);
        self.tone.stage.set(stage);
        self.frame_flags.pane_changed = true;
        self.cursor_moved(from);
    }

    /// Put the keys in `stage`'s panel, on its first row (the focused point for the EQ).
    pub(crate) fn enter_stage(&mut self, stage: Stage) {
        if !self.stages().contains(&stage) {
            return;
        }
        let from = self.cursor_id();
        self.tone.xf_line = None;
        self.tone.plays = None;
        match stage {
            Stage::Eq => self.focus_point(self.focused_point()),
            Stage::Loudness => self.tone.row = ToneRow::Ld(Ld::On),
            Stage::Space => self.tone.row = ToneRow::Crossfeed(self.xf_first()),
            Stage::Bypass => self.tone.row = ToneRow::Bypass,
        }
        self.tone.stage.set(stage);
        self.frame_flags.pane_changed = true;
        self.cursor_moved(from);
    }

    /// `L`: the Loudness panel, from wherever the keys are; pressed in the panel, back to the
    /// strip.
    pub(crate) fn jump_to_loudness(&mut self) -> Vec<Intent> {
        if !self.ld_shown() {
            return Vec::new();
        }
        self.jump_to_stage(Stage::Loudness);
        Vec::new()
    }

    /// Put the keys in `stage`'s panel, on the tone screen, or back on the strip when they are
    /// in that panel already.
    pub(crate) fn jump_to_stage(&mut self, stage: Stage) {
        if self.screen == Screen::Tone
            && Self::row_is_stage(self.tone.row)
            && !matches!(self.tone.row, ToneRow::Strip(_))
            && self.open_stage() == stage
        {
            self.focus_strip(stage);
            return;
        }
        if self.screen != Screen::Tone {
            self.show(Screen::Tone);
        }
        self.enter_stage(stage);
    }

    /// The bypass turned `on`, or the other way from now when `None`; refused in words where the
    /// controller has none.
    pub(crate) fn bypass_set(&mut self, on: Option<bool>) -> Vec<Intent> {
        let Some(now) = self.bypass_view().map(|view| view.on) else {
            self.warn("this controller has no bypass");
            return Vec::new();
        };
        let on = on.unwrap_or(!now);
        if on == now {
            return Vec::new();
        }
        vec![Intent::eq(HostEqOp::Bypass { on })]
    }

    /// `␣` on `stage`: it goes off or on, the bypass the other way.
    fn stage_toggle(&mut self, stage: Stage) -> Vec<Intent> {
        match stage {
            Stage::Eq => self.computer_mode(None),
            Stage::Loudness => self.ld_toggle(None),
            Stage::Space => self.space_toggle(),
            Stage::Bypass => self.bypass_set(None),
        }
    }

    /// `←→` on a stage's on/off row: off with `←`, on with `→`.
    fn stage_set(&mut self, stage: Stage, dir: i64) -> Vec<Intent> {
        match stage {
            Stage::Eq => self.computer_mode(Some(dir > 0)),
            Stage::Loudness => self.ld_toggle(Some(dir > 0)),
            Stage::Space => self.xf_toggle(Some(dir > 0)),
            Stage::Bypass => self.bypass_set(Some(dir > 0)),
        }
    }

    /// The choice of Plays the arrows have highlighted: what plays until `Enter` applies another.
    pub(crate) fn plays_highlight(&self) -> Space {
        self.tone.plays.unwrap_or_else(|| self.space_now())
    }

    /// `←→` on the Plays row: the highlight moves, nothing is sent.
    pub(crate) fn plays_move(&mut self, dir: i64) {
        let choices = self.plays_choices();
        let at = choices
            .iter()
            .position(|space| *space == self.plays_highlight())
            .unwrap_or(0);
        let next = (at as i64 + dir.signum()).clamp(0, choices.len() as i64 - 1) as usize;
        self.tone.plays = Some(choices[next]);
    }

    /// The choices Plays offers: Off and Crossfeed, and the virtual speakers where served.
    pub(crate) fn plays_choices(&self) -> Vec<Space> {
        match self.space_shown() {
            true => Space::ALL.to_vec(),
            false => vec![Space::Off, Space::Crossfeed],
        }
    }

    /// `Enter` on the Plays row: the highlighted choice plays.
    pub(crate) fn plays_apply(&mut self) -> Vec<Intent> {
        let to = self.plays_highlight();
        self.tone.plays = None;
        self.space_go(to)
    }

    /// The keys of the tone screen's computer side that the stages take, before the screen's
    /// own: `f`, `L` and `X` from anywhere, the strip's keys, and a panel's. `None` for a key
    /// they leave to the rest of the screen.
    pub(crate) fn stage_key(&mut self, code: KeyCode) -> Option<Vec<Intent>> {
        if !self.stages_shown() {
            return None;
        }
        match code {
            KeyCode::Char('f') => return Some(self.bypass_set(None)),
            KeyCode::Char('L') => return Some(self.jump_to_loudness()),
            KeyCode::Char('X') => return Some(self.jump_to_crossfeed()),
            _ => {}
        }
        match self.tone.row {
            ToneRow::Strip(stage) => self.strip_key(stage, code),
            ToneRow::Mode if code == KeyCode::Down => {
                self.focus_strip(self.open_stage());
                Some(Vec::new())
            }
            ToneRow::Point(_) | ToneRow::Ld(_) | ToneRow::Crossfeed(_) | ToneRow::Bypass => {
                self.panel_key(code)
            }
            ToneRow::Mode | ToneRow::Loudness | ToneRow::Band | ToneRow::Target => None,
        }
    }

    /// The keys while they are on the strip's stop for `stage`.
    fn strip_key(&mut self, stage: Stage, code: KeyCode) -> Option<Vec<Intent>> {
        let stages = self.stages();
        let at = stages.iter().position(|s| *s == stage).unwrap_or(0);
        Some(match code {
            KeyCode::Left | KeyCode::Right => {
                let to = match code == KeyCode::Right {
                    true => (at + 1).min(stages.len() - 1),
                    false => at.saturating_sub(1),
                };
                self.focus_strip(stages[to]);
                Vec::new()
            }
            KeyCode::Down | KeyCode::Enter => {
                self.enter_stage(stage);
                Vec::new()
            }
            KeyCode::Up => {
                let from = self.cursor_id();
                self.tone.row = ToneRow::Mode;
                self.cursor_moved(from);
                Vec::new()
            }
            KeyCode::Char(' ') => self.stage_toggle(stage),
            _ if points_key(code) => Vec::new(),
            _ => return None,
        })
    }

    /// The keys while they are in a row of the open stage's panel.
    fn panel_key(&mut self, code: KeyCode) -> Option<Vec<Intent>> {
        let stage = self.open_stage();
        let rows = self.panel_rows(stage);
        let at = rows
            .iter()
            .position(|row| *row == self.tone.row)
            .unwrap_or(0);
        match code {
            KeyCode::Esc => {
                self.focus_strip(stage);
                return Some(Vec::new());
            }
            KeyCode::Up | KeyCode::Down => {
                self.tone.xf_line = None;
                self.tone.plays = None;
                match (code, at) {
                    (KeyCode::Up, 0) => self.focus_strip(stage),
                    (KeyCode::Up, at) => self.go_to_row(rows[at - 1]),
                    (_, at) => self.go_to_row(rows[(at + 1).min(rows.len() - 1)]),
                }
                return Some(Vec::new());
            }
            KeyCode::Char(' ') => return Some(self.stage_toggle(stage)),
            _ => {}
        }
        let handled = match stage {
            Stage::Eq => return None,
            Stage::Loudness => self.loudness_panel_key(code),
            Stage::Space => self.space_panel_key(code),
            Stage::Bypass => match code {
                KeyCode::Left => Some(self.bypass_set(Some(false))),
                KeyCode::Right => Some(self.bypass_set(Some(true))),
                KeyCode::Enter => Some(self.bypass_set(None)),
                _ => None,
            },
        };
        // The Points table's own keys do nothing in another stage's panel: a point is never added
        // or moved from there.
        handled.or_else(|| points_key(code).then(Vec::new))
    }

    /// Put the keys on `row` of the open panel.
    fn go_to_row(&mut self, row: ToneRow) {
        let from = self.cursor_id();
        match row {
            ToneRow::Point(at) => self.focus_point(at),
            row => self.tone.row = row,
        }
        self.cursor_moved(from);
    }

    /// The Loudness panel's own keys: `←→` change the row, `Enter` types or turns it on or off,
    /// `r` takes the volume now as the reference.
    fn loudness_panel_key(&mut self, code: KeyCode) -> Option<Vec<Intent>> {
        let ToneRow::Ld(row) = self.tone.row else {
            return None;
        };
        let change = |app: &mut App, dir: i64| match row {
            Ld::On => app.stage_set(Stage::Loudness, dir),
            row => app.ld_value_step(row, dir),
        };
        Some(match code {
            KeyCode::Left | KeyCode::Char(',') => change(self, -1),
            KeyCode::Right | KeyCode::Char('.') => change(self, 1),
            KeyCode::Enter if row == Ld::On => self.ld_toggle(None),
            KeyCode::Enter => {
                self.open_ld_line(row);
                Vec::new()
            }
            KeyCode::Char('r') => self.ld_reference_now(),
            _ => return None,
        })
    }

    /// The Space panel's own keys.
    fn space_panel_key(&mut self, code: KeyCode) -> Option<Vec<Intent>> {
        let ToneRow::Crossfeed(row) = self.tone.row else {
            return None;
        };
        let on_off = !self.space_shown();
        let change = |app: &mut App, dir: i64| match row {
            // With no speakers the first row is crossfeed's own on/off; with them it only
            // highlights a choice, which `Enter` applies.
            Xf::Space if on_off => app.stage_set(Stage::Space, dir),
            Xf::Space => {
                app.plays_move(dir);
                Vec::new()
            }
            Xf::Set => app.set_step(dir),
            Xf::Model => app.xf_model_step(dir),
            Xf::Preset => app.xf_preset(dir),
            row => app.xf_value_step(row, dir),
        };
        Some(match code {
            // `,` and `.` never change the crossfeed model: only `←→` do.
            KeyCode::Char(',' | '.') if row == Xf::Model => Vec::new(),
            KeyCode::Left | KeyCode::Char(',') => change(self, -1),
            KeyCode::Right | KeyCode::Char('.') => change(self, 1),
            KeyCode::Enter => match row {
                Xf::Space if on_off => self.xf_toggle(None),
                Xf::Space => self.plays_apply(),
                Xf::Set => self.open_sets(),
                Xf::Model | Xf::Preset => Vec::new(),
                row => {
                    self.open_xf_line(row);
                    Vec::new()
                }
            },
            _ => return None,
        })
    }

    /// A key pressed on the strip or in a panel is never read by the older rows of the device
    /// screen: this says whether `row` is one of the stages' own.
    pub(crate) fn row_is_stage(row: ToneRow) -> bool {
        matches!(
            row,
            ToneRow::Strip(_)
                | ToneRow::Point(_)
                | ToneRow::Ld(_)
                | ToneRow::Crossfeed(_)
                | ToneRow::Bypass
        )
    }

    /// `Tab` and `⇧Tab` on the computer side's tone screen: the stops are the header and then
    /// each stage's panel, so one key press goes from the header into the EQ and on down the
    /// chain to the bypass.
    pub(crate) fn cycle_stage(&mut self, dir: isize) {
        let stages = self.stages();
        let slot = match self.tone.row {
            ToneRow::Strip(stage) => stages.iter().position(|s| *s == stage).map(|at| at + 1),
            row if Self::row_is_stage(row) => stages
                .iter()
                .position(|s| *s == self.open_stage())
                .map(|at| at + 1),
            _ => Some(0),
        }
        .unwrap_or(0);
        let to = super::cycled(slot, dir, stages.len() + 1);
        self.tone.xf_line = None;
        self.frame_flags.pane_changed = true;
        match to {
            0 => {
                let from = self.cursor_id();
                self.tone.plays = None;
                self.tone.row = ToneRow::Mode;
                self.cursor_moved(from);
            }
            to => self.enter_stage(stages[to - 1]),
        }
    }
}

#[cfg(test)]
mod tests;
