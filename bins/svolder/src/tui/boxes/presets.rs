// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset list: the named tone curves that fit the device's tone control, in a box over the
//! tone-control screen or the device screen, to load one, keep the curve on the device as one,
//! and change or take away the ones the configuration file holds.
//!
//! The built-ins come first, built for this tone control from its own profile, then the user's
//! own in the order the file has them, those with this tone control's band count and every
//! imported one. `a` searches AutoEq's headphones ([`search`]) and `i` imports a file ([`prompt`]);
//! on the computer's side `Enter` puts the focused preset on the computer ([`computer`]). Nothing
//! here writes: a load answers with the writes the recall plan asks for, which the loop sends
//! without waiting as it sends every key's, and a change to the library answers with the
//! [`Intent`]s the loop carries out through the configuration writers. What the list refuses is
//! said in a toast with the box still up, as the device editor says it.
//!
//! - [`computer`]: the list's keys for the computer EQ.
//! - [`import`]: what the search box and the file prompt hand over: a correction to keep.
//! - [`prompt`]: the file prompt inside the box.
//! - [`search`]: the AutoEq search inside the box.

use std::time::Instant;

use crossterm::event::KeyCode;
use svolder_autoeq::{Entry as Measurement, Index};
use svolder_ipc::config::RefreshWeeks;
use svolder_profiles::EqSpec;
use svolder_profiles::tone::preset::{self, BothSides, EqCommand, Plan, Preset};
use svolder_proto::FunctionId;

use crate::tui::app::{ANSWER_LIFETIME, App, Intent, stepped};

pub(crate) mod computer;
mod import;
pub(crate) mod prompt;
pub(crate) mod search;

pub use import::Pending;
pub use prompt::FilePrompt;
pub use search::SearchBox;

/// What the header's name opens with while a preset is on, or was loaded.
const PRESET: &str = "Preset ";
/// What it opens with while a recall is still being confirmed.
const LOADING: &str = "loading ";
/// What follows the name of the preset loaded last once the curve has moved away from it.
const CHANGED: &str = "*";
/// What the header calls a curve no preset matches, and what `n` numbers a new name after.
const CUSTOM: &str = "custom";

/// What saving or updating before the device has reported its whole curve is answered with.
const NOT_READ: &str = "the tone control has not reported its whole curve yet";

/// What `n` or `u` is answered with while a load is still being confirmed: each saves the curve
/// the device reads, which it has not yet brought up to date.
fn still_loading_notice(name: &str) -> String {
    format!("still loading {name}")
}

/// What `u`, `c` or `x` on a built-in is answered with.
fn read_only(name: &str) -> String {
    format!("{name} is built in; n saves the curve as a new preset")
}

/// The presets' state: what the configuration file names, the list over the screens, the AutoEq
/// index and the computer EQ.
pub struct PresetsBox {
    /// The presets the configuration file names, as it names them and in its order, which the
    /// loop re-reads after every write: what the preset list offers after the built-ins, those
    /// of them with this tone control's band count.
    pub saved: Vec<Preset>,
    /// The preset list over the tone-control screen or the device screen, while it is open.
    pub preset_list: Option<PresetList>,
    /// The list of the device's own presets over the device screen, while it is open: a device
    /// with a tone control has [`preset_list`](Self::preset_list) instead.
    pub device_list: Option<super::device_presets::DeviceList>,
    /// The AutoEq index, once the search has had it: kept for the session.
    pub autoeq: Option<std::sync::Arc<svolder_autoeq::Index>>,
    /// How often the AutoEq index is fetched again, as the configuration file holds it.
    pub autoeq_weeks: RefreshWeeks,
    /// The preset loaded, or saved from the device, last this session: what the tone-control
    /// screen's header names with a `*` once the curve has moved away from it. Kept for the
    /// session only and written nowhere.
    pub loaded_preset: Option<String>,
    /// The preset the DAC was flattened for, while the apply that follows has not ended: a refusal
    /// of that apply says the DAC was flattened.
    pub computer_flattened: Option<String>,
}

/// The preset list, while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetList {
    /// The focused row, as an index into [`entries`].
    pub row: usize,
    /// What the box is showing: the list itself, or one of the two things it asks on the way.
    pub step: Step,
}

/// What the preset list's box is showing.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// The presets themselves, to move through and act on.
    Choosing,
    /// A name being typed, for the curve saved as a new preset, a rename, or an import.
    Naming(NameLine),
    /// A `[y/N]` question.
    Asking(Question),
    /// The AutoEq search.
    Searching(SearchBox),
    /// The path of a file to import.
    Prompting(FilePrompt),
}

/// The one line the name prompt takes.
#[derive(Debug, Clone, PartialEq)]
pub struct NameLine {
    /// The preset being renamed, as the file spells it, or `None` for a new one.
    pub renaming: Option<String>,
    /// The correction the name is for, when the preset being named is an import whose first
    /// name was taken.
    pub import: Option<Box<Pending>>,
    /// The name as it has been typed.
    pub text: String,
    /// Where the cursor sits, in characters from the start of the line.
    pub caret: usize,
}

impl NameLine {
    /// A line holding `text`, with the cursor at its end.
    fn new(renaming: Option<String>, text: String) -> NameLine {
        NameLine {
            renaming,
            import: None,
            caret: text.chars().count(),
            text,
        }
    }
}

/// What the preset list asks before it changes the configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Whether to write the device's curve over the preset of this name.
    Update(String),
    /// Whether to take the preset of this name out of the configuration file.
    Delete(String),
    /// Whether to flatten the DAC and put the preset of this name, which is on it, on the
    /// computer.
    MoveToComputer(String),
    /// Whether to take the preset of this name, which is on the computer, off it and load it.
    MoveToDac(String),
}

/// What a job the loop ran off the UI thread ended with.
#[derive(Debug)]
pub enum Done {
    /// The AutoEq index, or why there is none.
    Index(Result<Index, String>),
    /// One measurement's correction text, or why it could not be had.
    Download(Measurement, Result<String, String>),
}

/// Fold what a job ended with into the app, and answer with the writes it asks for.
pub(crate) fn done(app: &mut App, done: Done) -> Vec<Intent> {
    match done {
        Done::Index(result) => {
            search::index_ready(app, result);
            Vec::new()
        }
        Done::Download(entry, result) => import::downloaded(app, &entry, result),
    }
}

/// One row of the preset list.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The preset itself.
    pub preset: Preset,
    /// Whether it is a built-in, which nothing saves over, renames or deletes.
    pub builtin: bool,
}

/// A recall whose writes are on their way to the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loading {
    /// The preset being loaded.
    pub name: String,
    /// The function the recall's last write addresses: its echo is the device confirming the
    /// whole of the curve.
    pub last: FunctionId,
    /// When the recall was sent.
    pub since: Instant,
}

impl Loading {
    /// Whether the header has said this long enough without the device confirming it, as of
    /// `now`.
    pub fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.since) >= ANSWER_LIFETIME
    }
}

/// What the tone-control screen's header says about the curve on the device: `Preset NAME` for
/// the preset that is on, `Preset NAME*` once the curve has moved away from the one loaded last,
/// `Preset custom` for a curve no preset matches, and `loading NAME` while a recall is still
/// being confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Standing {
    /// What the line opens with.
    pub lead: &'static str,
    /// The preset's name, which is what a narrow header cuts short.
    pub name: String,
    /// What follows the name: the mark of a curve changed since it was loaded, or nothing.
    pub mark: &'static str,
    /// Whether `name` is a preset of the list, on or loaded with changes, rather than a load in
    /// flight or the word `custom`.
    pub preset: bool,
}

impl Standing {
    /// The whole of what the header says, uncut.
    #[cfg(test)]
    pub fn text(&self) -> String {
        format!("{}{}{}", self.lead, self.name, self.mark)
    }

    /// What follows a label of its own that reads `Preset`: the lead (`loading ` while a recall
    /// is still being confirmed, else nothing), the name, and its mark.
    pub fn after_label(&self) -> (&'static str, &str, &'static str) {
        let lead = match self.lead {
            PRESET => "",
            lead => lead,
        };
        (lead, &self.name, self.mark)
    }
}

/// Every preset the tone control `eq` takes: the built-ins, built on its own centres, then the
/// configuration file's own with its band count, in the order the file has them.
pub fn entries(app: &App, eq: &EqSpec) -> Vec<Entry> {
    preset::fitting(eq, &app.presets.saved)
        .into_iter()
        .map(|(preset, builtin)| Entry { preset, builtin })
        .collect()
}

/// The name of the preset that is on, as `entries` spell it ([`preset::on`]): the one loaded
/// last this session while it still matches the device's readings, else the first of the list
/// that does; `None` (the header's `custom`) while none does or the curve is not read in full.
pub fn on(app: &App, eq: &EqSpec, entries: &[Entry]) -> Option<String> {
    match preset::on(
        eq,
        |id| app.raw_of(id),
        entries.iter().map(|entry| &entry.preset),
        app.presets.loaded_preset.as_deref(),
    ) {
        preset::On::Preset(found) => Some(found.name.clone()),
        preset::On::Unread | preset::On::Custom => None,
    }
}

/// What the tone-control screen's header says about the presets, or nothing on a device with no
/// tone control.
pub fn standing(app: &App) -> Option<Standing> {
    let eq = app.profile.eq.as_ref()?;
    if let Some(loading) = app.readings.loading.as_ref() {
        return Some(Standing {
            lead: LOADING,
            name: loading.name.clone(),
            mark: "",
            preset: false,
        });
    }
    let entries = entries(app, eq);
    if let Some(name) = on(app, eq, &entries) {
        return Some(Standing {
            lead: PRESET,
            name,
            mark: "",
            preset: true,
        });
    }
    let loaded = app.presets.loaded_preset.as_deref().and_then(|loaded| {
        entries
            .iter()
            .find(|entry| entry.preset.name.eq_ignore_ascii_case(loaded))
    });
    Some(match loaded {
        Some(entry) => Standing {
            lead: PRESET,
            name: entry.preset.name.clone(),
            mark: CHANGED,
            preset: true,
        },
        None => Standing {
            lead: PRESET,
            name: CUSTOM.to_string(),
            mark: "",
            preset: false,
        },
    })
}

/// Open the list on the device's tone control, with the preset that is on focused, and ask for
/// the presets to be read from the configuration file again, so the list shows what another
/// front end or `svolder eq` has written since; nothing at all on a device with no tone
/// control.
pub(in crate::tui) fn open(app: &mut App) -> Vec<Intent> {
    if app.preset_spec().is_none() {
        return Vec::new();
    }
    app.presets.preset_list = Some(PresetList {
        row: on_row(app),
        step: Step::Choosing,
    });
    vec![Intent::ReadPresets]
}

/// Take `presets`, the configuration file's as it was just read, in place of the ones the app
/// held, work out each imported one's fit, and focus the preset that is on again if the list is
/// still showing them.
pub(crate) fn reread(app: &mut App, presets: Vec<Preset>) {
    app.presets.saved = presets;
    app.tone.target_cache.clear();
    // Each imported preset's fit is worked out here, once per opening, so no frame fits.
    if let Some(eq) = app.preset_spec().as_ref() {
        for source in app
            .presets
            .saved
            .iter()
            .filter_map(|preset| preset.source.as_ref())
        {
            preset::fit_for(eq, source);
        }
    }
    let row = on_row(app);
    if let Some(PresetList {
        row: focused,
        step: Step::Choosing,
    }) = app.presets.preset_list.as_mut()
    {
        *focused = row;
    }
}

/// The row of the preset that is on, or the first row where none is.
fn on_row(app: &App) -> usize {
    let Some(eq) = app.preset_spec() else {
        return 0;
    };
    let entries = entries(app, &eq);
    on(app, &eq, &entries)
        .and_then(|name| entries.iter().position(|entry| entry.preset.name == name))
        .unwrap_or(0)
}

/// Whether a load is still being confirmed, said in a toast where it is — `still loading
/// NAME` — for a key that saves the curve the device reads: read back now, it would be the one
/// from before the load, or part of the way to it. Nothing that plans writes is held, since
/// every plan is made from the steps the device is on its way to ([`App::planned_raw`]).
fn still_loading(app: &mut App) -> bool {
    let Some(loading) = app.readings.loading.as_ref() else {
        return false;
    };
    let words = still_loading_notice(&loading.name);
    app.warn(words);
    true
}

/// What a key does while the list is open: the list's own keys, the name prompt's typing, or the
/// answer to a `[y/N]` question, the search's or the file prompt's own keys, and every other key
/// swallowed — the box is answered, and what it
/// covers is not reached through it.
pub(in crate::tui) fn key(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(list) = app.presets.preset_list.as_ref() else {
        return Vec::new();
    };
    match &list.step {
        Step::Choosing => choose(app, code),
        Step::Naming(_) => type_name(app, code),
        Step::Asking(question) => {
            let question = question.clone();
            answer(app, code, question)
        }
        Step::Searching(_) => search::key(app, code),
        Step::Prompting(_) => prompt::key(app, code),
    }
}

/// The list's own keys: `↑↓` move, `Enter` loads the focused preset and closes the list (after
/// asking, for the one on the computer), `n` asks for a name to save the device's curve under,
/// `u` and `x` ask before they update or delete the focused preset, `c` asks for its new name,
/// `a` opens the AutoEq search, `i` the file prompt, and `Esc` closes the
/// list. `u`, `c` and `x` refuse a built-in, saying why, and `n` and `u` refuse while a load is
/// still being confirmed, since each would save the curve from before it.
fn choose(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(eq) = app.preset_spec() else {
        app.presets.preset_list = None;
        return Vec::new();
    };
    let entries = entries(app, &eq);
    let Some(list) = app.presets.preset_list.as_ref() else {
        return Vec::new();
    };
    let row = list.row.min(entries.len().saturating_sub(1));
    let focused = entries.get(row).cloned();
    let (row, step) = match code {
        KeyCode::Esc => {
            app.presets.preset_list = None;
            return Vec::new();
        }
        KeyCode::Up => (stepped(row, -1, entries.len()), Step::Choosing),
        KeyCode::Down => (stepped(row, 1, entries.len()), Step::Choosing),
        KeyCode::Enter => {
            let Some(entry) = focused else {
                return Vec::new();
            };
            if app.on_computer() {
                return computer::recall(app, &entry);
            }
            let there = app.computer_preset();
            if preset::both_sides(&entry.preset, there.as_deref()) != BothSides::Free {
                computer::ask(app, Question::MoveToDac(entry.preset.name));
                return Vec::new();
            }
            return load(app, &eq, &entry.preset);
        }
        KeyCode::Char('a') => return search::open(app),
        KeyCode::Char('i') => {
            prompt::open(app);
            return Vec::new();
        }
        KeyCode::Char('n') => {
            if still_loading(app) {
                return Vec::new();
            }
            let name = next_custom(&app.presets.saved);
            (row, Step::Naming(NameLine::new(None, name)))
        }
        KeyCode::Char(letter @ ('u' | 'c' | 'x')) => {
            let Some(entry) = focused else {
                return Vec::new();
            };
            let imported = entry.preset.is_imported();
            let name = entry.preset.name;
            if entry.builtin {
                app.warn(read_only(&name));
                return Vec::new();
            }
            if letter == 'u' && app.on_computer() {
                app.warn("on the computer n saves its points as a new preset");
                return Vec::new();
            }
            if letter == 'u' && imported {
                app.warn(preset::imported_update_refusal(&name));
                return Vec::new();
            }
            if letter == 'u' && still_loading(app) {
                return Vec::new();
            }
            let step = match letter {
                'u' => Step::Asking(Question::Update(name)),
                'c' => Step::Naming(NameLine::new(Some(name.clone()), name)),
                _ => Step::Asking(Question::Delete(name)),
            };
            (row, step)
        }
        _ => return Vec::new(),
    };
    app.presets.preset_list = Some(PresetList { row, step });
    Vec::new()
}

/// Recall `preset` onto the device and close the list: the plan's writes, in order, as the
/// intents the loop sends, with every value the recall moved to fit said in a toast.
///
/// The plan is made from the steps the tone control is on its way to ([`App::planned_raw`]), and
/// its writes are noted as pending ([`App::note_writes`]), so a band key, `0`, the edit line or
/// another recall used before the device answers follows on from this one. The preset is the
/// one loaded from here on, and the header says it is loading until the device answers the
/// plan's last write. The daemon is told which preset it is first, ahead of the writes. A
/// preset the device already carries, or is already on its way to, writes nothing: it waits
/// for nothing, or for the recall still being answered, under its own name.
fn load(app: &mut App, eq: &EqSpec, preset: &Preset) -> Vec<Intent> {
    app.presets.preset_list = None;
    let plan = match preset.eq_writes(&app.profile, |id| app.planned_raw(id)) {
        Ok(recall) => recall.plan,
        Err(e) => {
            app.warn(e.to_string());
            return Vec::new();
        }
    };
    if !plan.clamped.is_empty() {
        let clamps: Vec<String> = plan.clamped.iter().map(ToString::to_string).collect();
        app.warn(clamps.join("; "));
    }
    app.presets.loaded_preset = Some(preset.name.clone());
    let last = plan.writes.last().and_then(|write| write.id(eq));
    app.readings.loading = match (last, app.readings.loading.take()) {
        (Some(last), _) => Some(Loading {
            name: preset.name.clone(),
            last,
            since: app.now,
        }),
        (None, Some(earlier)) => Some(Loading {
            name: preset.name.clone(),
            ..earlier
        }),
        (None, None) => None,
    };
    app.note_writes(eq, &plan.writes);
    let mut intents = vec![Intent::Recalled(preset.name.clone())];
    intents.extend(plan_intents(app, eq, &plan));
    intents
}

/// The intents that carry `plan`'s writes out on a device whose tone control is `eq`, in the
/// plan's own order: a band parameter as the value its raw step stands for, the mode by name,
/// and the loudness strength as the loudness row's own arrows set it.
pub(crate) fn plan_intents(app: &App, eq: &EqSpec, plan: &Plan) -> Vec<Intent> {
    plan.commands(&app.profile)
        .into_iter()
        .map(|command| match command {
            EqCommand::Band { band, param, value } => Intent::EqSet {
                band,
                param: param.to_string(),
                value,
            },
            EqCommand::Mode(mode) => Intent::EqMode(mode.to_string()),
            EqCommand::Loudness { raw, .. } => app.loudness_intent(eq, i64::from(raw)),
        })
        .collect()
}

/// The first `custom N` no preset in the file is called yet, whatever its band count: names are
/// one library-wide space.
fn next_custom(presets: &[Preset]) -> String {
    (1u32..)
        .map(|n| format!("{CUSTOM} {n}"))
        .find(|name| {
            !presets
                .iter()
                .any(|preset| preset.name.eq_ignore_ascii_case(name))
        })
        .unwrap_or_else(|| CUSTOM.to_string())
}

/// What a key does on the name prompt: the letters are typed where the cursor is, up to the most
/// a preset's name takes, the arrows, `Home` and `End` move it, `Backspace` takes the character
/// before it, `Enter` saves or renames, and `Esc` goes back to the list with nothing written.
fn type_name(app: &mut App, code: KeyCode) -> Vec<Intent> {
    if code == KeyCode::Enter {
        return named(app);
    }
    let Some(list) = app.presets.preset_list.as_mut() else {
        return Vec::new();
    };
    let Step::Naming(line) = &mut list.step else {
        return Vec::new();
    };
    let len = line.text.chars().count();
    match code {
        KeyCode::Esc => list.step = Step::Choosing,
        KeyCode::Left => line.caret = line.caret.saturating_sub(1),
        KeyCode::Right => line.caret = (line.caret + 1).min(len),
        KeyCode::Home => line.caret = 0,
        KeyCode::End => line.caret = len,
        KeyCode::Backspace if line.caret > 0 => {
            let at = byte_at(&line.text, line.caret - 1);
            line.text.remove(at);
            line.caret -= 1;
        }
        KeyCode::Char(letter) if len < preset::NAME_MAX => {
            let at = byte_at(&line.text, line.caret);
            line.text.insert(at, letter);
            line.caret += 1;
        }
        _ => {}
    }
    Vec::new()
}

/// Where character `at` of `text` starts, in bytes, and its end for a cursor past the last one.
fn byte_at(text: &str, at: usize) -> usize {
    text.char_indices()
        .nth(at)
        .map_or(text.len(), |(index, _)| index)
}

/// Save the device's curve under the name typed, keep the import the prompt is for, or rename the
/// preset the prompt is about, and go back to the list — or say what stops it, with the prompt
/// still up.
///
/// A new preset is the one loaded from here on, and the list's focus goes to the row it lands
/// on, which is the last: the file keeps its presets in the order they were added. A rename
/// carries the loaded name along with it.
fn named(app: &mut App) -> Vec<Intent> {
    let Some(PresetList {
        step: Step::Naming(line),
        ..
    }) = app.presets.preset_list.as_ref()
    else {
        return Vec::new();
    };
    let renaming = line.renaming.clone();
    let importing = line.import.clone();
    let name = match refused_name(&app.presets.saved, &line.text, renaming.as_deref()) {
        Ok(name) => name.to_string(),
        Err(why) => {
            app.warn(why);
            return Vec::new();
        }
    };
    if let Some(pending) = importing {
        return import::save(app, &name, pending.imported, pending.skipped);
    }
    let intents = match renaming {
        Some(old) => {
            if app
                .presets
                .loaded_preset
                .as_deref()
                .is_some_and(|loaded| loaded.eq_ignore_ascii_case(&old))
            {
                app.presets.loaded_preset = Some(name.clone());
            }
            vec![Intent::RenamePreset(old, name)]
        }
        None if app.on_computer() => {
            let intent = match app.host.daemon_owns() {
                true => Intent::eq(svolder_ipc::HostEqOp::SaveAs { name: name.clone() }),
                false => {
                    let Some(correction) = app.computer_correction() else {
                        app.warn(NOT_READ);
                        return Vec::new();
                    };
                    Intent::SavePreset(preset::from_computer(&name, correction))
                }
            };
            vec![intent]
        }
        None => {
            let Some(eq) = app.profile.eq.clone() else {
                return Vec::new();
            };
            let Some(preset) = preset::from_readings(&eq, |id| app.raw_of(id), name.clone()) else {
                app.warn(NOT_READ);
                return Vec::new();
            };
            let row = entries(app, &eq).len();
            if let Some(list) = app.presets.preset_list.as_mut() {
                list.row = row;
            }
            app.presets.loaded_preset = Some(name);
            vec![Intent::SavePreset(preset)]
        }
    };
    if let Some(list) = app.presets.preset_list.as_mut() {
        list.step = Step::Choosing;
    }
    intents
}

/// `text` as what a preset is called, without the blanks round it, or why it cannot be
/// ([`preset::new_name`]): blank, too long, a control character, a built-in's name, `custom`,
/// or another preset's name, in any case — the one being renamed apart, which may take its own
/// name in another spelling.
fn refused_name<'a>(
    presets: &[Preset],
    text: &'a str,
    renaming: Option<&str>,
) -> Result<&'a str, String> {
    preset::new_name(
        text,
        presets.iter().map(|preset| preset.name.as_str()),
        renaming,
    )
    .map_err(|why| why.to_string())
}

/// What a key does while a `[y/N]` question is open: `y` updates, deletes or moves, `n`, `Esc` and
/// `Enter` — the question's default is no — go back to the list with nothing written, and every
/// other key is swallowed.
fn answer(app: &mut App, code: KeyCode, question: Question) -> Vec<Intent> {
    match code {
        KeyCode::Char('y' | 'Y') => {}
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('n' | 'N') => {
            back_to_list(app);
            return Vec::new();
        }
        _ => return Vec::new(),
    }
    back_to_list(app);
    match question {
        Question::Update(name) => {
            let Some(eq) = app.profile.eq.clone() else {
                return Vec::new();
            };
            match preset::from_readings(&eq, |id| app.raw_of(id), name.clone()) {
                Some(preset) => {
                    app.presets.loaded_preset = Some(name.clone());
                    vec![Intent::UpdatePreset(name, preset)]
                }
                None => {
                    app.warn(NOT_READ);
                    Vec::new()
                }
            }
        }
        Question::Delete(name) => {
            if app
                .presets
                .loaded_preset
                .as_deref()
                .is_some_and(|loaded| loaded.eq_ignore_ascii_case(&name))
            {
                app.presets.loaded_preset = None;
            }
            vec![Intent::RemovePreset(name)]
        }
        Question::MoveToComputer(name) => computer::move_to_computer(app, &name),
        Question::MoveToDac(name) => computer::move_to_dac(app, &name),
    }
}

/// Go back from a question or the name prompt to the list itself.
fn back_to_list(app: &mut App) {
    if let Some(list) = app.presets.preset_list.as_mut() {
        list.step = Step::Choosing;
    }
}

#[cfg(test)]
mod tests;
