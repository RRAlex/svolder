// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq search inside the preset list's box: a query line, and under it the measurements
//! the daemon's AutoEq service finds for it, one to a row, grouped under their headphone with
//! the recommended one first. `Enter` asks the daemon to import the focused measurement
//! ([`super::import`]).
//!
//! The index lives in the daemon, so every change of the query is a question to it, asked off
//! the loop and after a short pause in the typing ([`DEBOUNCE`]); the loop never waits for the
//! answer. Each question is an [`Ask`] with a number, the box keeps the number of the last one,
//! and an answer for any other is dropped: a slow answer that comes after a newer question, or
//! after the box was closed or opened again, shows nothing. A question is also asked when the
//! box opens, with an empty query, so the daemon starts loading its index and the status line
//! can say where it stands.

use std::time::Duration;

use crossterm::event::KeyCode;
use svolder_ipc::{AutoEqSearch, AutoEqVariant};

use crate::tui::app::{App, Intent};

use super::{PresetList, Step};

/// The most characters a query takes.
pub const QUERY_MAX: usize = 60;
/// The most measurements the box keeps for one query.
pub const ROWS_MAX: usize = 500;
/// How long the typing pauses before a query is asked: a burst of keys is one question, so the
/// daemon does not search once per key, and long enough that a typist's next key usually comes
/// first. Short enough that the list follows the typing without a felt delay.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// One question to the daemon: the query, whether the index is fetched again for it, and the
/// number its answer is matched to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// The question's number: the box takes only the answer of the last one it asked.
    pub seq: u64,
    /// What to look for.
    pub query: String,
    /// Whether to fetch the index again whatever its age.
    pub refresh: bool,
}

/// Where the index stands, as the status line says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    /// The daemon has not answered yet: it may be loading the index.
    Fetching,
    /// The daemon answered.
    Ready,
    /// The last question failed, for this reason.
    Failed(String),
}

/// What the daemon says of the index it searched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    /// When the copy was fetched, in unix seconds.
    pub fetched_unix: u64,
    /// How many measurements it lists.
    pub measurements: usize,
}

/// One measurement in the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The headphone's key: its name as lowercase letters and digits. Rows with one key are one
    /// headphone.
    pub key: String,
    /// The measurement, to hand back to the daemon to import.
    pub variant: AutoEqVariant,
    /// `source · form[ · rig]`: what tells it from the headphone's others.
    pub words: String,
}

/// The search box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchBox {
    /// What has been typed.
    pub query: String,
    /// The focused row, an index into `rows`.
    pub row: usize,
    /// The measurements of the last answer taken, grouped by headphone with the recommended one
    /// first.
    pub rows: Vec<Row>,
    /// Where the index stands.
    pub load: Load,
    /// The headphone whose import the daemon is working on, while one is.
    pub importing: Option<String>,
    /// The preset name that import saves under: the daemon's answer clears `importing` only for
    /// the import it is the answer to, so a box opened again is not reset by an older import.
    pub importing_as: Option<String>,
    /// The number of the last question asked: the one whose answer the box waits for.
    pub seq: u64,
    /// Whether the answer to that question is still owed.
    pub waiting: bool,
    /// What the last answer said of the index.
    pub about: Option<About>,
    /// How many headphones the last answer left out past its cap.
    pub more: usize,
}

impl SearchBox {
    /// An empty box, waiting for the answer to question `seq`.
    fn new(seq: u64) -> SearchBox {
        SearchBox {
            query: String::new(),
            row: 0,
            rows: Vec::new(),
            load: Load::Fetching,
            importing: None,
            importing_as: None,
            seq,
            waiting: true,
            about: None,
            more: 0,
        }
    }

    /// The measurement that is focused.
    pub fn focused(&self) -> Option<&Row> {
        self.rows.get(self.row)
    }

    /// Take the daemon's answer for the question the box last asked: its groups as rows,
    /// keeping the focus on the row it was on where there still is one.
    fn take(&mut self, found: AutoEqSearch) {
        self.rows = found
            .groups
            .into_iter()
            .flat_map(|group| {
                group.variants.into_iter().map(move |m| Row {
                    key: group.key.clone(),
                    variant: m.variant,
                    words: m.words,
                })
            })
            .take(ROWS_MAX)
            .collect();
        self.row = self.row.min(self.rows.len().saturating_sub(1));
        self.more = found.more;
        self.about = Some(About {
            fetched_unix: found.fetched_unix,
            measurements: found.measurements,
        });
        self.load = Load::Ready;
    }
}

/// The next question, numbered after every one the session asked before it.
fn ask(app: &mut App, query: &str, refresh: bool) -> Ask {
    app.presets.ask_seq += 1;
    Ask {
        seq: app.presets.ask_seq,
        query: query.to_string(),
        refresh,
    }
}

/// Open the search box and ask the daemon about nothing yet, which has it load its index.
pub(super) fn open(app: &mut App) -> Vec<Intent> {
    let first = ask(app, "", false);
    if let Some(list) = app.presets.preset_list.as_mut() {
        list.step = Step::Searching(SearchBox::new(first.seq));
    }
    vec![Intent::AutoEqSearch {
        ask: first,
        soon: false,
    }]
}

/// What a key does in the search box: printable keys edit the query, `↑↓` move, `Enter` asks for
/// the focused measurement to be imported, `Esc` goes back to the list.
pub(super) fn key(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(list) = app.presets.preset_list.as_mut() else {
        return Vec::new();
    };
    let Step::Searching(search) = &mut list.step else {
        return Vec::new();
    };
    let edited = match code {
        KeyCode::Esc => {
            list.step = Step::Choosing;
            return Vec::new();
        }
        KeyCode::Up => {
            search.row = search.row.saturating_sub(1);
            false
        }
        KeyCode::Down => {
            search.row = (search.row + 1).min(search.rows.len().saturating_sub(1));
            false
        }
        KeyCode::Backspace => search.query.pop().is_some(),
        KeyCode::Char(letter)
            if !letter.is_control() && search.query.chars().count() < QUERY_MAX =>
        {
            search.query.push(letter);
            true
        }
        // The rows are the last answer's: while a newer one is owed, they may be for another
        // query than the one on the line.
        KeyCode::Enter if search.importing.is_none() && !search.waiting => {
            return match search.focused().map(|row| row.variant.clone()) {
                Some(variant) => super::import::autoeq(app, variant),
                None => Vec::new(),
            };
        }
        _ => false,
    };
    if !edited {
        return Vec::new();
    }
    let query = search.query.clone();
    let asked = ask(app, &query, false);
    if let Some(PresetList {
        step: Step::Searching(search),
        ..
    }) = app.presets.preset_list.as_mut()
    {
        search.seq = asked.seq;
        search.waiting = true;
        if query.is_empty() {
            search.rows.clear();
            search.row = 0;
        }
    }
    vec![Intent::AutoEqSearch {
        ask: asked,
        soon: true,
    }]
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

/// `Ctrl-R`: ask again with the index fetched anew, whatever the cached copy's age.
pub(crate) fn refresh(app: &mut App) -> Vec<Intent> {
    let Some(PresetList {
        step: Step::Searching(search),
        ..
    }) = app.presets.preset_list.as_ref()
    else {
        return Vec::new();
    };
    let query = search.query.clone();
    let asked = ask(app, &query, true);
    if let Some(PresetList {
        step: Step::Searching(search),
        ..
    }) = app.presets.preset_list.as_mut()
    {
        search.seq = asked.seq;
        search.waiting = true;
        search.load = Load::Fetching;
    }
    vec![Intent::AutoEqSearch {
        ask: asked,
        soon: false,
    }]
}

/// The daemon answered question `ask`: show its rows, or say why there are none. An answer for
/// a question that is not the box's last, or for a box that is no longer open, is dropped. A
/// refresh that fell back on the cached copy says so in a toast.
pub(crate) fn answered(app: &mut App, ask: &Ask, result: Result<AutoEqSearch, String>) {
    let Some(PresetList {
        step: Step::Searching(search),
        ..
    }) = app.presets.preset_list.as_mut()
    else {
        return;
    };
    if search.seq != ask.seq {
        return;
    }
    search.waiting = false;
    match result {
        Ok(found) => {
            let note = found.note.clone();
            search.take(found);
            if let Some(note) = note {
                app.warn(note);
            }
        }
        Err(why) => {
            search.rows.clear();
            search.row = 0;
            search.load = Load::Failed(why);
        }
    }
}

/// The status line: what the daemon is doing, or where the index is from, or why it failed.
pub fn status(search: &SearchBox) -> String {
    if let Some(name) = &search.importing {
        return format!("importing {name}…");
    }
    match (&search.load, &search.about) {
        (Load::Fetching, _) => "fetching the index…".to_string(),
        (Load::Failed(why), _) => why.clone(),
        (Load::Ready, Some(about)) => {
            let mut line = format!(
                "index from {} · {} measurements",
                crate::clock::date(std::time::UNIX_EPOCH + Duration::from_secs(about.fetched_unix)),
                about.measurements
            );
            if search.more > 0 {
                line.push_str(&format!(" · {} more headphones: narrow it", search.more));
            }
            line
        }
        (Load::Ready, None) => String::new(),
    }
}

/// `source · form[ · rig]`: what tells two measurements of one headphone apart.
pub fn measurement(row: &Row) -> String {
    row.words.clone()
}

#[cfg(test)]
mod tests;
