// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device editor: what one `[[device]]` block says, as rows over the device list.
//!
//! The block itself is the configuration file's own record, taken as it stands when `u` opens
//! the editor and handed back whole when `Enter` saves, so the keys no row here shows — the
//! device's unit id, its input names — go back into the file untouched. Nothing here writes:
//! saving answers with the [`Intent`]s the loop carries out through the configuration writers,
//! and everything the editor itself refuses is said in a toast with the rows still up.

use crossterm::event::KeyCode;
use svolder_ipc::DeviceConfig;

use crate::tui::app::{App, Intent, cycled, stepped};
use crate::tui::widgets::passphrase_row::PassphraseRow;

/// One row of the editor, in the order it draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorRow {
    /// What the device is called, which every front end picks it out by.
    Name,
    /// What the device asks for: no passphrase, the code its advertisement carries, or a code of
    /// four digits, stepped between with the arrows.
    Passphrase,
    /// The profile to drive it with, or the one svolder detects.
    Profile,
    /// Whether this is the device a command acts on when it names none.
    Default,
}

/// The editor's rows, in the order it draws them and the arrows move through them.
pub const EDITOR_ROWS: [EditorRow; 4] = [
    EditorRow::Name,
    EditorRow::Passphrase,
    EditorRow::Profile,
    EditorRow::Default,
];

/// What the profile row means by "whichever profile svolder detects", which is the file saying
/// nothing about a profile at all.
pub const AUTOMATIC: &str = "automatic";

/// What a save with the name blanked out is answered with.
const NEEDS_A_NAME: &str = "a device needs a name";

/// What editing a device the configuration file says nothing about is answered with.
fn nothing_configured(name: &str) -> String {
    format!("{name} is not in this configuration file")
}

/// What saving onto a name another device carries is answered with.
fn name_taken(name: &str) -> String {
    format!("another device is called {name}")
}

/// The device being edited over the device list, as the editor holds it.
pub struct DeviceEditor {
    /// The name the device had when the editor opened, which is the block a save writes over.
    pub original: String,
    /// The device as the rows have it now.
    pub device: DeviceConfig,
    /// Whether it is to be the device a command acts on when it names none.
    pub default: bool,
    /// The focused row, as an index into [`EDITOR_ROWS`].
    pub row: usize,
    /// The profiles the profile row steps through, [`AUTOMATIC`] first.
    pub profiles: Vec<String>,
    /// The passphrase row: which of `none`, `auto` and a code it is on, and the digits typed.
    pub passphrase: PassphraseRow,
    /// Where the cursor sits in the name being typed, in characters from its start.
    pub caret: usize,
}

impl DeviceEditor {
    /// The row the keys are on.
    pub fn focused(&self) -> EditorRow {
        EDITOR_ROWS[self.row.min(EDITOR_ROWS.len() - 1)]
    }

    /// What the profile row reads: the profile the device names, or [`AUTOMATIC`] for the one
    /// svolder detects.
    pub fn profile(&self) -> &str {
        self.device.profile.as_deref().unwrap_or(AUTOMATIC)
    }

    /// Move the focus `delta` rows, stopping at each end, with the cursor left at the end of
    /// whatever line the new row is typed into.
    fn move_row(&mut self, delta: isize) {
        self.row = stepped(self.row, delta, EDITOR_ROWS.len());
        self.caret = self.typed_len();
    }

    /// Change the focused row one step: the cursor along the name, the passphrase round `none`,
    /// `auto` and the code, the next profile round the list, and the default-device switch the
    /// other way.
    fn step(&mut self, dir: isize) {
        match self.focused() {
            EditorRow::Name => {
                self.caret = match dir < 0 {
                    true => self.caret.saturating_sub(1),
                    false => (self.caret + 1).min(self.typed_len()),
                };
            }
            EditorRow::Passphrase => self.passphrase.step(dir),
            EditorRow::Profile => {
                let at = self
                    .profiles
                    .iter()
                    .position(|name| name == self.profile())
                    .unwrap_or(0);
                let next = self.profiles[cycled(at, dir, self.profiles.len())].clone();
                self.device.profile = (next != AUTOMATIC).then_some(next);
            }
            EditorRow::Default => self.default = !self.default,
        }
    }

    /// Type `letter` on the rows that take typing: a name takes whatever is typed where the
    /// cursor is, and the passphrase takes digits, in order, after the ones already there.
    fn typed(&mut self, letter: char) {
        match self.focused() {
            EditorRow::Name => {
                let at = byte_at(&self.device.name, self.caret);
                self.device.name.insert(at, letter);
                self.caret += 1;
            }
            EditorRow::Passphrase => self.passphrase.type_digit(letter),
            EditorRow::Profile | EditorRow::Default => {}
        }
    }

    /// Delete the character before the cursor in the name, or take back the passphrase's last
    /// digit.
    fn erase(&mut self) {
        match self.focused() {
            EditorRow::Name => {
                if self.caret == 0 {
                    return;
                }
                let at = byte_at(&self.device.name, self.caret - 1);
                self.device.name.remove(at);
                self.caret -= 1;
            }
            EditorRow::Passphrase => self.passphrase.erase(),
            EditorRow::Profile | EditorRow::Default => {}
        }
    }

    /// How many characters the focused row's line holds, and none at all on a row that takes no
    /// line: the passphrase is typed in order, with no cursor to place.
    fn typed_len(&self) -> usize {
        match self.focused() {
            EditorRow::Name => self.device.name.chars().count(),
            EditorRow::Passphrase | EditorRow::Profile | EditorRow::Default => 0,
        }
    }
}

/// Where character `at` of `text` starts, in bytes, and its end for a cursor past the last one.
fn byte_at(text: &str, at: usize) -> usize {
    text.char_indices()
        .nth(at)
        .map_or(text.len(), |(index, _)| index)
}

/// Open the editor on the device the list has focused, or say why there is nothing to edit.
///
/// A device the file says nothing about is one something else is holding from some other
/// configuration than this session read, which is a thing to say rather than a thing to invent a
/// record for.
pub(in crate::tui) fn open(app: &mut App) {
    let Some(name) = selected(app) else {
        return;
    };
    let Some(device) = configured(app, &name).cloned() else {
        app.warn(nothing_configured(&name));
        return;
    };
    let passphrase = PassphraseRow::from_file(&device.passphrase);
    app.devices.editor = Some(DeviceEditor {
        original: device.name.clone(),
        default: app
            .devices
            .default_device
            .as_deref()
            .is_some_and(|default| default.eq_ignore_ascii_case(&device.name)),
        profiles: profiles_for(app, &name, device.profile.as_deref()),
        caret: device.name.chars().count(),
        device,
        row: 0,
        passphrase,
    });
}

/// Ask whether the focused device is to be taken out of the configuration file.
pub(in crate::tui) fn ask(app: &mut App) {
    app.devices.confirm = selected(app);
}

/// The name of the device the list has focused, if it has one.
fn selected(app: &App) -> Option<String> {
    app.devices
        .rows
        .get(app.selected)
        .map(|device| device.name.clone())
}

/// The configured device called `name`, whichever way it is spelled: the file's names are what a
/// selector matches, and a selector matches them without regard to case.
fn configured<'a>(app: &'a App, name: &str) -> Option<&'a DeviceConfig> {
    app.devices
        .configured
        .iter()
        .find(|device| device.name.eq_ignore_ascii_case(name))
}

/// The profiles the profile row steps through for the device called `name`: [`AUTOMATIC`] first,
/// then the profiles that name the product the device advertises, and every profile there is
/// where nothing has said what the product is.
///
/// A `pinned` profile the list does not hold — one written by hand, or one from a profile
/// directory this build cannot read — is offered as the entry it is, so the row reads what the
/// file says and stepping starts from there rather than jumping to the first choice.
fn profiles_for(app: &App, name: &str, pinned: Option<&str>) -> Vec<String> {
    let mut choices = vec![AUTOMATIC.to_string()];
    if let Ok(registry) = crate::backend::registry() {
        let product = app
            .devices
            .rows
            .iter()
            .find(|row| row.name == name)
            .and_then(|row| row.product.as_deref());
        let names = registry.names();
        let matching: Vec<String> = names
            .iter()
            .filter(|profile| match (product, registry.get(profile)) {
                (Some(product), Some(profile)) => profile.display.eq_ignore_ascii_case(product),
                _ => true,
            })
            .map(|profile| (*profile).to_string())
            .collect();
        // A product no profile covers is one the user picks a profile for by hand, so the whole
        // list is offered rather than none of it.
        choices.extend(match matching.is_empty() {
            true => names.into_iter().map(str::to_string).collect(),
            false => matching,
        });
    }
    if let Some(pinned) = pinned
        && !choices.iter().any(|choice| choice == pinned)
    {
        choices.push(pinned.to_string());
    }
    choices
}

/// What a key does while the editor is open: the arrows move between the rows and change the one
/// they are on, the letters are typed into the name and the digits into the passphrase's code,
/// `Enter` saves and `Esc` leaves the device as the file has it.
///
/// Everything the editor refuses is said in a toast with the editor still open, so a name
/// that is taken or half a code is answered where it was typed.
pub(in crate::tui) fn key(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(editor) = app.devices.editor.as_mut() else {
        return Vec::new();
    };
    match code {
        KeyCode::Esc => app.devices.editor = None,
        KeyCode::Up => editor.move_row(-1),
        KeyCode::Down => editor.move_row(1),
        KeyCode::Left => editor.step(-1),
        KeyCode::Right => editor.step(1),
        KeyCode::Char(' ') if editor.focused() == EditorRow::Default => editor.step(1),
        KeyCode::Char(letter) => editor.typed(letter),
        KeyCode::Backspace => editor.erase(),
        KeyCode::Home => editor.caret = 0,
        KeyCode::End => editor.caret = editor.typed_len(),
        KeyCode::Enter => return save(app),
        _ => {}
    }
    Vec::new()
}

/// Write what the editor holds, or say what stopped it.
///
/// The whole device goes back to the file, so the keys no row here shows survive. The
/// default-device key is asked for beside it only where the row changed what the file says: the
/// writer carries a default naming this device through a rename itself, so a renamed default
/// needs nothing more.
fn save(app: &mut App) -> Vec<Intent> {
    let Some(editor) = app.devices.editor.as_ref() else {
        return Vec::new();
    };
    let name = editor.device.name.trim().to_string();
    if name.is_empty() {
        app.warn(NEEDS_A_NAME);
        return Vec::new();
    }
    if app.devices.configured.iter().any(|other| {
        other.name.eq_ignore_ascii_case(&name) && !other.name.eq_ignore_ascii_case(&editor.original)
    }) {
        app.warn(name_taken(&name));
        return Vec::new();
    }
    let passphrase = match editor.passphrase.text() {
        Ok(passphrase) => passphrase,
        Err(why) => {
            app.warn(why);
            return Vec::new();
        }
    };
    let device = DeviceConfig {
        name,
        passphrase,
        ..editor.device.clone()
    };
    let was_default = app
        .devices
        .default_device
        .as_deref()
        .is_some_and(|default| default.eq_ignore_ascii_case(&editor.original));
    let default = match (editor.default, was_default) {
        (true, false) => Some(Some(device.name.clone())),
        (false, true) => Some(None),
        _ => None,
    };
    let original = editor.original.clone();
    app.devices.editor = None;
    let mut intents = vec![Intent::UpdateDevice(original, device)];
    intents.extend(default.map(Intent::DefaultDevice));
    intents
}

/// What a key does while a removal is waiting to be confirmed: `Enter` takes the device out of
/// the file, `Esc` leaves it there, and every other key is swallowed — the question is answered,
/// not typed into.
pub(in crate::tui) fn answer(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(name) = app.devices.confirm.clone() else {
        return Vec::new();
    };
    match code {
        KeyCode::Enter => {
            app.devices.confirm = None;
            vec![Intent::RemoveDevice(name)]
        }
        KeyCode::Esc => {
            app.devices.confirm = None;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
