// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq search inside the preset list's box: a query line typed over the measurements the
//! AutoEq index names, filtered as the query changes. `Enter` asks for the selected
//! measurement's correction, which arrives as a [`Done`](super::Done) and is imported by
//! [`super::import`].
//!
//! The index is fetched off the UI loop, so the box opens at once and says it is fetching; what
//! is typed meanwhile filters the index when it lands. The index, once had, is kept on the app
//! for the session, so the next `a` opens with it.

use crossterm::event::KeyCode;
use svolder_autoeq::{Entry as Measurement, Index, search};

use crate::tui::app::{App, Intent};

use super::{PresetList, Step};

/// The most characters a query takes.
pub const QUERY_MAX: usize = 60;
/// The most measurements the box keeps for one query.
pub const ROWS_MAX: usize = 500;

/// Where the index stands, as the status line says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    /// The index is being fetched.
    Fetching,
    /// The index is in hand.
    Ready,
    /// The last fetch failed, for this reason.
    Failed(String),
}

/// The search box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchBox {
    /// What has been typed.
    pub query: String,
    /// The focused row, an index into `rows`.
    pub row: usize,
    /// The measurements the query finds, grouped by headphone with the recommended one first.
    pub rows: Vec<Measurement>,
    /// Where the index stands.
    pub load: Load,
    /// The headphone whose correction is being downloaded, while one is.
    pub downloading: Option<String>,
}

impl SearchBox {
    /// A box over `index`, or one waiting for it.
    fn new(index: Option<&Index>) -> SearchBox {
        SearchBox {
            query: String::new(),
            row: 0,
            rows: Vec::new(),
            load: match index {
                Some(_) => Load::Ready,
                None => Load::Fetching,
            },
            downloading: None,
        }
    }

    /// Find the measurements of the query again in `index`, keeping the focus on the row it
    /// was on where there still is one.
    fn refilter(&mut self, index: Option<&Index>) {
        self.rows = match index {
            Some(index) => search(&index.entries, &self.query)
                .into_iter()
                .take(ROWS_MAX)
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        self.row = self.row.min(self.rows.len().saturating_sub(1));
    }

    /// The measurement that is focused.
    pub fn focused(&self) -> Option<&Measurement> {
        self.rows.get(self.row)
    }
}

/// Open the search box, fetching the index unless the session already holds it.
pub(super) fn open(app: &mut App) -> Vec<Intent> {
    let have = app.presets.autoeq.is_some();
    let search = SearchBox::new(app.presets.autoeq.as_deref());
    if let Some(list) = app.presets.preset_list.as_mut() {
        list.step = Step::Searching(search);
    }
    match have {
        true => Vec::new(),
        false => vec![Intent::AutoEqIndex { refresh: false }],
    }
}

/// What a key does in the search box: printable keys edit the query, `↑↓` move, `Enter` asks for
/// the focused measurement's correction, `Esc` goes back to the list.
pub(super) fn key(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let index = app.presets.autoeq.as_deref();
    let Some(list) = app.presets.preset_list.as_mut() else {
        return Vec::new();
    };
    let Step::Searching(search) = &mut list.step else {
        return Vec::new();
    };
    match code {
        KeyCode::Esc => list.step = Step::Choosing,
        KeyCode::Up => search.row = search.row.saturating_sub(1),
        KeyCode::Down => {
            search.row = (search.row + 1).min(search.rows.len().saturating_sub(1));
        }
        KeyCode::Backspace => {
            search.query.pop();
            search.refilter(index);
        }
        KeyCode::Char(letter)
            if !letter.is_control() && search.query.chars().count() < QUERY_MAX =>
        {
            search.query.push(letter);
            search.refilter(index);
        }
        KeyCode::Enter if search.downloading.is_none() => {
            if let Some(entry) = search.focused().cloned() {
                search.downloading = Some(entry.name.clone());
                return vec![Intent::AutoEqDownload(entry)];
            }
        }
        _ => {}
    }
    Vec::new()
}

/// Whether the search box is the one showing.
pub(crate) fn showing(app: &App) -> bool {
    matches!(
        app.presets.preset_list,
        Some(PresetList {
            step: Step::Searching(_),
            ..
        })
    )
}

/// `Ctrl-R`: fetch the index again, whatever the cached copy's age.
pub(crate) fn refresh(app: &mut App) -> Vec<Intent> {
    let Some(PresetList {
        step: Step::Searching(search),
        ..
    }) = app.presets.preset_list.as_mut()
    else {
        return Vec::new();
    };
    search.load = Load::Fetching;
    vec![Intent::AutoEqIndex { refresh: true }]
}

/// The index fetch ended: keep the index for the session and filter the open box through it, or
/// say why there is none. A refresh that fell back on the cached copy says so in a toast.
pub(crate) fn index_ready(app: &mut App, result: Result<Index, String>) {
    match result {
        Ok(index) => {
            if let Some(note) = &index.note {
                app.warn(note.clone());
            }
            app.presets.autoeq = Some(std::sync::Arc::new(index));
            if let Some(PresetList {
                step: Step::Searching(search),
                ..
            }) = app.presets.preset_list.as_mut()
            {
                search.load = Load::Ready;
                search.refilter(app.presets.autoeq.as_deref());
            }
        }
        Err(why) => {
            if let Some(PresetList {
                step: Step::Searching(search),
                ..
            }) = app.presets.preset_list.as_mut()
            {
                search.load = Load::Failed(why);
            }
        }
    }
}

/// The status line: where the index is from, or what it is doing, or why it failed.
pub fn status(search: &SearchBox, index: Option<&Index>) -> String {
    if let Some(name) = &search.downloading {
        return format!("downloading {name}…");
    }
    match (&search.load, index) {
        (Load::Fetching, _) => "fetching the index…".to_string(),
        (Load::Failed(why), _) => why.clone(),
        (Load::Ready, Some(index)) => format!(
            "index from {} · {} measurements",
            crate::clock::date(index.fetched),
            index.entries.len()
        ),
        (Load::Ready, None) => String::new(),
    }
}

/// `source · form[ · rig]`: what tells two measurements of one headphone apart.
pub fn measurement(entry: &Measurement) -> String {
    let origin = entry.origin();
    origin
        .strip_prefix("AutoEq · ")
        .unwrap_or(&origin)
        .to_string()
}

#[cfg(test)]
mod tests;
