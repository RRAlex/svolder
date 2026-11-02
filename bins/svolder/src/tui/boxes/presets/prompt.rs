// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The file prompt inside the preset list's box: a path typed after `~/`, completed with `Tab` as
//! a shell completes it, imported with `Enter`.
//!
//! `Tab` completes the one match fully, adding `/` after a folder; with several it completes to
//! their common prefix and lists them under the prompt, six at a time, and a further `Tab` steps
//! through them. Only folders and `.txt` files are offered, and hidden entries only once the
//! part typed starts with a dot. A folder that cannot be read is said under the prompt.

use std::path::{Path, PathBuf};

use crossterm::event::KeyCode;
use svolder_profiles::tone::preset;
use svolder_profiles::tone::preset::Imported;

use crate::tui::app::{App, Intent};

use super::import::{self, Alt};
use super::{PresetList, Step};

/// What the prompt starts with.
const START: &str = "~/";
/// The most candidates listed under the prompt before `… N more`.
pub const LISTED: usize = 6;
/// The most characters a path takes.
const PATH_MAX: usize = 1024;

/// The candidates one `Tab` found, and which of them a further `Tab` has stepped to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// The typed path up to and including its last `/`, which every candidate follows.
    pub dir: String,
    /// The candidates, folders ending in `/`, in name order.
    pub names: Vec<String>,
    /// The candidate the text was last stepped to, if it was.
    pub at: Option<usize>,
    /// What the text was left as, so an edit tells the candidates are out of date.
    pub text: String,
}

/// The file prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePrompt {
    /// What `~` stands for, taken when the prompt opened.
    pub home: Option<PathBuf>,
    /// The path as typed.
    pub text: String,
    /// The candidates the last `Tab` listed, while the text is as it left it.
    pub tab: Option<Completion>,
    /// What to say under the prompt: a folder that cannot be read, or no match.
    pub note: Option<String>,
}

impl FilePrompt {
    /// A prompt at `~/`, with `home` as `~`.
    pub fn new(home: Option<PathBuf>) -> FilePrompt {
        FilePrompt {
            home,
            text: START.to_string(),
            tab: None,
            note: None,
        }
    }

    /// `path` with a leading `~` made the home folder.
    fn expand(&self, path: &str) -> Result<PathBuf, String> {
        let rest = match path {
            "~" => Some(""),
            _ => path.strip_prefix("~/"),
        };
        match (rest, &self.home) {
            (Some(rest), Some(home)) => Ok(home.join(rest)),
            (Some(_), None) => Err("no home folder to put ~ in".to_string()),
            (None, _) => Ok(PathBuf::from(path)),
        }
    }

    /// The candidates still in play, which the box lists: a window of [`LISTED`] holding the one
    /// stepped to, and how many more lie below it.
    pub fn listing(&self) -> Option<(Vec<&str>, usize, Option<usize>)> {
        let tab = self.tab.as_ref().filter(|tab| tab.text == self.text)?;
        let start = tab.at.map_or(0, |at| (at + 1).saturating_sub(LISTED));
        let shown: Vec<&str> = tab
            .names
            .iter()
            .skip(start)
            .take(LISTED)
            .map(String::as_str)
            .collect();
        let more = tab.names.len().saturating_sub(start + shown.len());
        Some((shown, more, tab.at.map(|at| at - start)))
    }

    /// `Tab`: complete the path, or step to the next candidate.
    fn complete(&mut self) {
        if let Some(tab) = self.tab.as_mut()
            && tab.text == self.text
            && tab.names.len() > 1
        {
            let next = tab.at.map_or(0, |at| (at + 1) % tab.names.len());
            tab.at = Some(next);
            self.text = format!("{}{}", tab.dir, tab.names[next]);
            tab.text.clone_from(&self.text);
            return;
        }
        self.tab = None;
        self.note = None;
        if self.text == "~" {
            self.text = START.to_string();
            return;
        }
        let at = self.text.rfind('/').map_or(0, |slash| slash + 1);
        let (dir, stem) = self.text.split_at(at);
        let (dir, stem) = (dir.to_string(), stem.to_string());
        let looked_in = match dir.as_str() {
            "" => Ok(PathBuf::from(".")),
            _ => self.expand(&dir),
        };
        let names = looked_in
            .and_then(|real| offered(&real, &stem).map_err(|e| format!("cannot read {dir}: {e}")));
        let names = match names {
            Ok(names) => names,
            Err(why) => {
                self.note = Some(why);
                return;
            }
        };
        match names.as_slice() {
            [] => self.note = Some("no match".to_string()),
            [only] => self.text = format!("{dir}{only}"),
            _ => {
                let common = common_prefix(&names);
                if common.chars().count() > stem.chars().count() {
                    self.text = format!("{dir}{common}");
                }
                self.tab = Some(Completion {
                    dir,
                    names,
                    at: None,
                    text: self.text.clone(),
                });
            }
        }
    }
}

/// The folders and `.txt` files of `dir` whose names start with `stem`, folders ending in `/`,
/// in name order; hidden ones only where `stem` starts with a dot, and none whose name is not
/// UTF-8, which the prompt's line cannot hold.
fn offered(dir: &Path, stem: &str) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !name.starts_with(stem) || (name.starts_with('.') && !stem.starts_with('.')) {
            continue;
        }
        let Ok(kind) = std::fs::metadata(entry.path()) else {
            continue;
        };
        let text = Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"));
        if kind.is_dir() {
            names.push(format!("{name}/"));
        } else if kind.is_file() && text {
            names.push(name);
        }
    }
    names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
    Ok(names)
}

/// The longest prefix every one of `names` starts with.
fn common_prefix(names: &[String]) -> String {
    let Some((first, rest)) = names.split_first() else {
        return String::new();
    };
    let mut prefix = first.clone();
    for name in rest {
        let same = prefix
            .chars()
            .zip(name.chars())
            .take_while(|(a, b)| a == b)
            .count();
        prefix = prefix.chars().take(same).collect();
    }
    prefix
}

/// Open the prompt at `~/`.
pub(super) fn open(app: &mut App) {
    let prompt = FilePrompt::new(svolder_ipc::config::home_dir());
    if let Some(list) = app.presets.preset_list.as_mut() {
        list.step = Step::Prompting(prompt);
    }
}

/// What a key does in the prompt: characters and `Backspace` edit the path, `Tab` completes it,
/// `Enter` imports the file, `Esc` goes back to the list.
pub(super) fn key(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(list) = app.presets.preset_list.as_mut() else {
        return Vec::new();
    };
    let Step::Prompting(prompt) = &mut list.step else {
        return Vec::new();
    };
    match code {
        KeyCode::Esc => list.step = Step::Choosing,
        KeyCode::Enter => return read(app),
        KeyCode::Tab => prompt.complete(),
        KeyCode::Backspace => {
            prompt.text.pop();
            prompt.tab = None;
            prompt.note = None;
        }
        KeyCode::Char(letter) if !letter.is_control() && prompt.text.len() < PATH_MAX => {
            prompt.text.push(letter);
            prompt.tab = None;
            prompt.note = None;
        }
        _ => {}
    }
    Vec::new()
}

/// Read the file the prompt names and import it, or say in a toast what is wrong with it: the
/// file's own refusal names the line.
fn read(app: &mut App) -> Vec<Intent> {
    let Some(PresetList {
        step: Step::Prompting(prompt),
        ..
    }) = app.presets.preset_list.as_ref()
    else {
        return Vec::new();
    };
    let path = match prompt.expand(&prompt.text) {
        Ok(path) => path,
        Err(why) => {
            app.warn(why);
            return Vec::new();
        }
    };
    let parsed = match crate::commands::device::read_correction(&path) {
        Ok(parsed) => parsed,
        Err(failure) => {
            app.warn(failure.what);
            return Vec::new();
        }
    };
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let name = Some(preset::default_name(&stem))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into()
        });
    let skipped = parsed.notes.len();
    let imported = Imported {
        origin: None,
        correction: parsed.correction,
    };
    import::ready(app, name, imported, skipped, Alt::Number)
}

#[cfg(test)]
mod tests;
