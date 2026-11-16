// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The virtual speakers' set list: a box over the tone screen or the device screen that lists the
//! catalogue's sets and the ones the person imported, fetches one, chooses a fetched one, forgets
//! one and imports a file.
//!
//! The daemon owns the sets (it is the one process that reaches the sources): this side asks it
//! for the catalogue, starts a fetch or an import as a job and follows the job's progress
//! (`run::speakers`), and chooses a fetched set through the computer EQ's own op
//! ([`HostEqOp::Speakers`]). A box opened with no daemon shows the sets the computer's view lists
//! as fetched and says that fetching needs the daemon.
//!
//! The rows are read from the catalogue the daemon last answered, or, before it has, from the
//! fetched sets of the computer's view. Which set plays comes from the view, so a choice made
//! elsewhere is on its row at once. While the list is open, a change of the view's sets (a fetch
//! or a forget from another front end) has the catalogue read again, and a set the view lists is
//! chosen, never fetched again. A fetch is one at a time ([`Fetch`]): its progress rides on its
//! row, and `x` stops it.

use crossterm::event::KeyCode;
use std::sync::Arc;
use svolder_host::eq::speakers::Room;

use svolder_ipc::{HostEqOp, SpeakersCatalogue, SpeakersRoom, SpeakersSaved};

use crate::commands::interrupts::Presses;

use crate::tui::app::{App, Intent};

/// What a fetch that was stopped says, so the stop is told as one and not as a failure.
pub const CANCELLED: &str = "cancelled";
/// The most characters a path takes.
const PATH_MAX: usize = 1024;

/// The way to stop a fetch or an import: a press cancels its job on the daemon, whenever the job
/// has started.
#[derive(Clone)]
pub struct Stop(pub Arc<Presses>);

impl Stop {
    /// A stop nothing has pressed.
    pub fn new() -> Stop {
        Stop(Arc::new(Presses::new()))
    }

    /// Stop the job.
    pub fn press(&self) {
        self.0.press();
    }
}

impl std::fmt::Debug for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Stop")
    }
}

impl PartialEq for Stop {
    fn eq(&self, other: &Stop) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Stop {}

/// One thing the daemon is asked to do about the sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeakersCall {
    /// List the sets.
    Catalogue,
    /// Fetch a catalogue set (`label` is what the progress calls it).
    Fetch {
        /// The set's catalogue id.
        id: String,
        /// What to call it in words.
        label: String,
        /// The way to stop it.
        stop: Stop,
    },
    /// Import a file the daemon can read as the person's own set.
    Import {
        /// Where the file is.
        path: String,
        /// Which choice it belongs under.
        room: SpeakersRoom,
        /// The way to stop it.
        stop: Stop,
    },
    /// Remove a fetched or imported set.
    Forget {
        /// The set's id.
        id: String,
    },
}

/// What a call the loop ran ended with, or said on the way.
#[derive(Debug)]
pub enum SpeakersDone {
    /// The catalogue, or why there is none.
    Catalogue(Result<Box<SpeakersCatalogue>, String>),
    /// A download's progress.
    Progress {
        /// The set.
        id: String,
        /// The bytes received so far.
        received: u64,
        /// The bytes expected.
        total: u64,
    },
    /// A fetch or import ended: the set it stored, or why not.
    Ended {
        /// What the job was for: the set's id, or the file's path for an import.
        id: String,
        /// The result.
        result: Result<Box<SpeakersSaved>, String>,
    },
    /// A forget ended.
    Forgotten {
        /// The set.
        id: String,
        /// The result.
        result: Result<(), String>,
    },
}

/// The fetch or import in flight: one at a time, as the daemon runs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    /// The set's id, or the file's path for an import.
    pub id: String,
    /// What to call it.
    pub label: String,
    /// Stops the job.
    pub stop: Stop,
    /// The bytes received so far.
    pub received: u64,
    /// The bytes expected; 0 where it is not known (an import).
    pub total: u64,
    /// Whether it is an import of a file, which has no download to count.
    pub import: bool,
    /// Whether it was asked to stop and has not ended yet: the daemon may take a few seconds.
    pub stopping: bool,
}

impl Fetch {
    /// Whole percent of the download, `None` before the size is known.
    pub fn percent(&self) -> Option<u64> {
        (self.total > 0).then(|| (self.received.saturating_mul(100) / self.total).min(100))
    }
}

/// Which sets the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    /// Every set.
    #[default]
    All,
    /// Sets with a room.
    With,
    /// Sets in the free field.
    No,
}

impl Filter {
    /// The next filter, round the ends; `dir` is 1 or -1.
    fn cycled(self, dir: i64) -> Filter {
        let all = [Filter::All, Filter::With, Filter::No];
        let at = all.iter().position(|f| *f == self).unwrap_or(0);
        all[(at as i64 + dir).rem_euclid(3) as usize]
    }

    /// Whether `room` passes.
    fn admits(self, room: SpeakersRoom) -> bool {
        match self {
            Filter::All => true,
            Filter::With => room == SpeakersRoom::With,
            Filter::No => room == SpeakersRoom::No,
        }
    }

    /// The word the filter row shows: the choice's name and its technical term.
    pub fn words(self) -> String {
        match self {
            Filter::All => "All".to_string(),
            Filter::With => room_words(SpeakersRoom::With),
            Filter::No => room_words(SpeakersRoom::No),
        }
    }
}

/// The file prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportLine {
    /// The path as typed.
    pub text: String,
    /// The choice the file is imported under.
    pub room: SpeakersRoom,
}

/// What the box is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The rows.
    Choosing,
    /// The file prompt.
    Importing(ImportLine),
    /// The `[y/N]` question of a forget: the set's id and label.
    Forgetting(String, String),
}

/// The set list, while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetsBox {
    /// The focused row of the filtered rows.
    pub row: usize,
    /// Which sets show.
    pub filter: Filter,
    /// What the box is showing.
    pub step: Step,
}

/// What the TUI keeps of the sets: the box, the catalogue and the fetch in flight.
#[derive(Debug, Default)]
pub struct SpeakersBox {
    /// The box, while it is open.
    pub list: Option<SetsBox>,
    /// The catalogue the daemon last answered.
    pub catalogue: Option<SpeakersCatalogue>,
    /// Why there is no catalogue, when the last ask failed.
    pub catalogue_error: Option<String>,
    /// The fetch or import in flight.
    pub fetch: Option<Fetch>,
    /// The ids of the view's sets when the catalogue was last asked for.
    pub seen: Option<Vec<String>>,
    /// Whether the list is owed a read of the catalogue: the view's sets changed while it was
    /// open (another front end fetched or forgot one).
    pub owed: bool,
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The set's id.
    pub id: String,
    /// With room or no room.
    pub room: SpeakersRoom,
    /// The dataset and subject, or the file's name.
    pub label: String,
    /// The licence.
    pub licence: String,
    /// The dataset's credit words; empty where the licence asks for none.
    pub credit: String,
    /// The source page; empty for an imported set.
    pub source_url: String,
    /// The download's size in bytes; 0 for an imported set.
    pub bytes: u64,
    /// Whether it is on this machine.
    pub fetched: bool,
    /// What is wrong with it, for a stored set that cannot play.
    pub damaged: Option<String>,
    /// Whether the responses were cut to the length cap.
    pub cut: bool,
    /// How long the responses are, in milliseconds; 0 where it is not known yet.
    pub taps_ms: f64,
    /// Whether it is the chosen set.
    pub chosen: bool,
    /// Whether the person imported it.
    pub own: bool,
}

/// A choice's name with its technical term: `With room · BRIR`.
pub fn room_words(room: SpeakersRoom) -> String {
    let room = match room {
        SpeakersRoom::With => Room::With,
        SpeakersRoom::No => Room::No,
    };
    format!("{} \u{b7} {}", room.name(), room.term())
}

/// A download's size in words: `35 MB`, `1.1 MB`, `640 kB`.
pub fn size_text(bytes: u64) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    match bytes {
        0 => String::new(),
        b if b < 1_000_000 => format!("{} kB", (b as f64 / 1000.0).round()),
        _ if mb < 10.0 => format!("{mb:.1} MB"),
        _ => format!("{mb:.0} MB"),
    }
}

impl App {
    /// The rows the list shows now under its filter: the catalogue's sets, or, before the daemon
    /// has answered, the fetched sets of the computer's view.
    pub(crate) fn sets_rows(&self) -> Vec<Row> {
        let chosen = self.speakers().and_then(|view| view.set.clone());
        let filter = self
            .speakers_box
            .list
            .as_ref()
            .map_or(Filter::All, |list| list.filter);
        let rows: Vec<Row> = match &self.speakers_box.catalogue {
            Some(catalogue) => catalogue
                .sets
                .iter()
                .map(|set| Row {
                    id: set.id.clone(),
                    room: set.room,
                    label: set.label.clone(),
                    licence: set.licence.clone(),
                    credit: set.credit.clone(),
                    source_url: set.source_url.clone(),
                    bytes: set.bytes,
                    fetched: set.fetched && set.damaged.is_none(),
                    damaged: set.damaged.clone(),
                    cut: set.cut,
                    taps_ms: set.taps_ms,
                    chosen: chosen.as_deref() == Some(set.id.as_str()),
                    own: set.own,
                })
                .collect(),
            None => self
                .speakers()
                .map(|view| {
                    view.sets
                        .iter()
                        .map(|set| Row {
                            id: set.id.clone(),
                            room: match set.room.as_str() {
                                "with" => SpeakersRoom::With,
                                _ => SpeakersRoom::No,
                            },
                            label: set.label.clone(),
                            licence: set.licence.clone(),
                            credit: set.credit.clone().unwrap_or_default(),
                            source_url: set.source_url.clone(),
                            bytes: 0,
                            fetched: true,
                            damaged: None,
                            cut: false,
                            taps_ms: 0.0,
                            chosen: chosen.as_deref() == Some(set.id.as_str()),
                            own: false,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        rows.into_iter()
            .filter(|row| filter.admits(row.room))
            .collect()
    }

    /// Open the set list: the catalogue is asked for again so the rows are as the daemon has them.
    pub(crate) fn open_sets(&mut self) -> Vec<Intent> {
        if self.speakers().is_none() {
            return Vec::new();
        }
        self.speakers_box.list = Some(SetsBox {
            row: 0,
            filter: Filter::All,
            step: Step::Choosing,
        });
        // The cursor opens on the chosen set, where there is one.
        let rows = self.sets_rows();
        if let (Some(at), Some(list)) = (
            rows.iter().position(|row| row.chosen),
            self.speakers_box.list.as_mut(),
        ) {
            list.row = at;
        }
        match self.session.daemon {
            true => self.ask_catalogue(),
            false => Vec::new(),
        }
    }

    /// The catalogue call the loop owes the list, once.
    pub(crate) fn owed_sets_call(&mut self) -> Option<SpeakersCall> {
        std::mem::take(&mut self.speakers_box.owed).then_some(SpeakersCall::Catalogue)
    }

    /// The ids of the sets the computer's view lists as fetched.
    fn view_set_ids(&self) -> Vec<String> {
        self.speakers()
            .map(|view| view.sets.iter().map(|set| set.id.clone()).collect())
            .unwrap_or_default()
    }

    /// Ask the daemon for the catalogue, noting the view's sets as of the ask.
    fn ask_catalogue(&mut self) -> Vec<Intent> {
        self.speakers_box.seen = Some(self.view_set_ids());
        vec![Intent::speakers(SpeakersCall::Catalogue)]
    }

    /// The computer's view changed: while the list is open with a daemon, sets that differ from
    /// the ones of the last ask owe it a read of the catalogue.
    pub(crate) fn follow_sets(&mut self) {
        if self.speakers_box.list.is_none() || !self.session.daemon {
            return;
        }
        let ids = self.view_set_ids();
        if self
            .speakers_box
            .seen
            .as_ref()
            .is_some_and(|seen| *seen != ids)
        {
            self.speakers_box.seen = Some(ids);
            self.speakers_box.owed = true;
        }
    }

    /// The rows' focused one.
    fn sets_focus(&self) -> Option<Row> {
        let at = self.speakers_box.list.as_ref()?.row;
        self.sets_rows().into_iter().nth(at)
    }

    /// What a key does in the set list.
    pub(crate) fn sets_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let Some(list) = self.speakers_box.list.as_mut() else {
            return Vec::new();
        };
        match &mut list.step {
            Step::Choosing => self.sets_choosing_key(code),
            Step::Importing(line) => {
                match code {
                    KeyCode::Esc => list.step = Step::Choosing,
                    KeyCode::Enter => return self.import_from_line(),
                    KeyCode::Tab | KeyCode::BackTab => {
                        line.room = match line.room {
                            SpeakersRoom::With => SpeakersRoom::No,
                            SpeakersRoom::No => SpeakersRoom::With,
                        };
                    }
                    KeyCode::Backspace => {
                        line.text.pop();
                    }
                    KeyCode::Char(c) if !c.is_control() && line.text.len() < PATH_MAX => {
                        line.text.push(c);
                    }
                    _ => {}
                }
                Vec::new()
            }
            Step::Forgetting(id, _) => {
                let id = id.clone();
                list.step = Step::Choosing;
                match code {
                    KeyCode::Char('y') => vec![Intent::speakers(SpeakersCall::Forget { id })],
                    _ => Vec::new(),
                }
            }
        }
    }

    /// The keys of the rows.
    fn sets_choosing_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let count = self.sets_rows().len();
        let Some(list) = self.speakers_box.list.as_mut() else {
            return Vec::new();
        };
        match code {
            KeyCode::Esc | KeyCode::Char('S') => self.speakers_box.list = None,
            KeyCode::Up => list.row = list.row.saturating_sub(1),
            KeyCode::Down => list.row = (list.row + 1).min(count.saturating_sub(1)),
            KeyCode::Home => list.row = 0,
            KeyCode::End => list.row = count.saturating_sub(1),
            KeyCode::Tab | KeyCode::BackTab => {
                list.filter = list
                    .filter
                    .cycled(if code == KeyCode::Tab { 1 } else { -1 });
                list.row = 0;
            }
            KeyCode::Enter => return self.sets_enter(),
            KeyCode::Char('x') => return self.sets_cancel(),
            KeyCode::Char('f') => self.sets_ask_forget(),
            KeyCode::Char('i') => {
                list.step = Step::Importing(ImportLine {
                    text: "~/".to_string(),
                    room: SpeakersRoom::No,
                });
            }
            _ => {}
        }
        Vec::new()
    }

    /// `Enter`: use a fetched set, or fetch one that is not.
    fn sets_enter(&mut self) -> Vec<Intent> {
        let Some(row) = self.sets_focus() else {
            return Vec::new();
        };
        // A set the view lists is fetched, whatever an older catalogue says.
        if row.fetched || self.view_set_ids().contains(&row.id) {
            self.speakers_box.list = None;
            return self.choose_set(&row.id);
        }
        if row.own && row.damaged.is_some() {
            self.warn(format!("{} is damaged: import the file again", row.label));
            return Vec::new();
        }
        if let Some(why) = self.speakers().and_then(|view| view.unavailable.clone()) {
            self.warn(why);
            return Vec::new();
        }
        if !self.session.daemon {
            self.warn(crate::speakers::NEEDS_DAEMON);
            return Vec::new();
        }
        if let Some(catalogue) = self.speakers_box.catalogue.as_ref().filter(|c| !c.fetching) {
            // The catalogue's own words, the ones every front end says.
            let why = catalogue.note.clone().unwrap_or_else(|| {
                "this daemon is set up to reach nothing, so it cannot fetch".to_string()
            });
            self.warn(why);
            return Vec::new();
        }
        if let Some(busy) = &self.speakers_box.fetch {
            let label = busy.label.clone();
            self.warn(format!("still fetching {label}"));
            return Vec::new();
        }
        let stop = Stop::new();
        self.speakers_box.fetch = Some(Fetch {
            id: row.id.clone(),
            label: row.label.clone(),
            stop: stop.clone(),
            received: 0,
            total: row.bytes,
            import: false,
            stopping: false,
        });
        vec![Intent::speakers(SpeakersCall::Fetch {
            id: row.id,
            label: row.label,
            stop,
        })]
    }

    /// `x`: stop the fetch in flight.
    fn sets_cancel(&mut self) -> Vec<Intent> {
        match self.speakers_box.fetch.as_mut() {
            None => self.notify("nothing is being fetched"),
            // The press cancels the job on the daemon, once it has started; its end says so.
            Some(fetch) => {
                fetch.stop.press();
                fetch.stopping = true;
            }
        }
        Vec::new()
    }

    /// `f`: ask before forgetting the focused set.
    fn sets_ask_forget(&mut self) {
        let Some(row) = self.sets_focus() else {
            return;
        };
        if !row.fetched && row.damaged.is_none() {
            self.notify(format!("{} is not fetched", row.label));
            return;
        }
        // The daemon's rule and words: the set that plays cannot be forgotten.
        let playing = self.speakers().is_some_and(|view| view.on) && row.chosen;
        if playing {
            self.warn(svolder_host::eq::speakers::plays_now_words(&row.label));
            return;
        }
        if let Some(list) = self.speakers_box.list.as_mut() {
            list.step = Step::Forgetting(row.id, row.label);
        }
    }

    /// `Enter` on the file prompt: the daemon reads the file named.
    fn import_from_line(&mut self) -> Vec<Intent> {
        let Some(SetsBox {
            step: Step::Importing(line),
            ..
        }) = self.speakers_box.list.as_ref()
        else {
            return Vec::new();
        };
        let text = line.text.trim().to_string();
        let room = line.room;
        if text.is_empty() || text == "~/" {
            self.warn("type the file's path");
            return Vec::new();
        }
        if !self.session.daemon {
            self.warn(crate::speakers::NEEDS_DAEMON);
            return Vec::new();
        }
        if let Some(busy) = &self.speakers_box.fetch {
            let label = busy.label.clone();
            self.warn(format!("still fetching {label}"));
            return Vec::new();
        }
        let path = match resolve_path(&text) {
            Ok(path) => path,
            Err(why) => {
                self.warn(why);
                return Vec::new();
            }
        };
        if let Some(list) = self.speakers_box.list.as_mut() {
            list.step = Step::Choosing;
        }
        let label = std::path::Path::new(&path)
            .file_name()
            .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
        let stop = Stop::new();
        self.speakers_box.fetch = Some(Fetch {
            id: path.clone(),
            label,
            stop: stop.clone(),
            received: 0,
            total: 0,
            import: true,
            stopping: false,
        });
        vec![Intent::speakers(SpeakersCall::Import { path, room, stop })]
    }

    /// Choose the fetched set `id` and have it play.
    pub(crate) fn choose_set(&mut self, id: &str) -> Vec<Intent> {
        if let Some(why) = self.speakers().and_then(|view| view.unavailable.clone()) {
            self.warn(why);
            return Vec::new();
        }
        vec![Intent::eq(HostEqOp::Speakers {
            on: Some(true),
            set: Some(id.to_string()),
            done: true,
        })]
    }

    /// A click on row `row` of the list: it is focused, and a double-click on the focused row
    /// does what `Enter` does.
    pub(crate) fn click_set_row(&mut self, row: usize) -> Vec<Intent> {
        let was = self.speakers_box.list.as_ref().map(|list| list.row);
        if let Some(list) = self.speakers_box.list.as_mut() {
            list.row = row;
        }
        match was == Some(row) && self.pointer.double_click {
            true => self.sets_key(KeyCode::Enter),
            false => Vec::new(),
        }
    }

    /// Fold what the loop ran for the sets into the app.
    pub(crate) fn speakers_done(&mut self, done: SpeakersDone) -> Vec<Intent> {
        match done {
            SpeakersDone::Catalogue(Ok(catalogue)) => {
                // The focus stays on the set it was on (else the chosen one) whatever order the
                // new rows come in.
                let keep = self
                    .sets_focus()
                    .or_else(|| self.sets_rows().into_iter().find(|row| row.chosen))
                    .map(|row| row.id);
                self.speakers_box.catalogue = Some(*catalogue);
                self.speakers_box.catalogue_error = None;
                let rows = self.sets_rows();
                let at = keep.and_then(|id| rows.iter().position(|row| row.id == id));
                if let Some(list) = self.speakers_box.list.as_mut() {
                    list.row = at.unwrap_or(list.row).min(rows.len().saturating_sub(1));
                }
                Vec::new()
            }
            SpeakersDone::Catalogue(Err(why)) => {
                self.speakers_box.catalogue_error = Some(why);
                Vec::new()
            }
            SpeakersDone::Progress {
                id,
                received,
                total,
            } => {
                if let Some(fetch) = self.speakers_box.fetch.as_mut().filter(|f| f.id == id) {
                    fetch.received = received;
                    fetch.total = total;
                }
                Vec::new()
            }
            SpeakersDone::Ended { id, result } => {
                let label = match self.speakers_box.fetch.take_if(|fetch| fetch.id == id) {
                    Some(fetch) => fetch.label,
                    None => id,
                };
                match result {
                    Ok(saved) => match saved.evicted.is_empty() {
                        true => self.notify(format!("{} is ready", saved.set.label)),
                        false => self.notify(format!(
                            "{} is ready; the oldest unused set went to make room",
                            saved.set.label
                        )),
                    },
                    Err(why) if why == CANCELLED => self.notify(format!("{label}: stopped")),
                    Err(why) => self.warn(why),
                }
                // The sets changed, or the job did not: either way the rows are asked for again.
                self.ask_catalogue()
            }
            SpeakersDone::Forgotten { id, result } => {
                match result {
                    Ok(()) => {
                        let label = self
                            .speakers_box
                            .catalogue
                            .as_ref()
                            .and_then(|c| c.sets.iter().find(|set| set.id == id))
                            .map_or(id, |set| set.label.clone());
                        self.notify(format!("{label} forgotten"));
                    }
                    Err(why) => self.warn(why),
                }
                self.ask_catalogue()
            }
        }
    }
}

/// The file `text` names as an absolute path: `~` and `~/` stand for the home folder and a
/// relative path is taken from this process's folder (the daemon's own may be another). Says so
/// when it is not a file, before any job starts.
pub(crate) fn resolve_path(text: &str) -> Result<String, String> {
    let home =
        || svolder_ipc::config::home_dir().ok_or_else(|| "no home folder to put ~ in".to_string());
    let path = match text {
        "~" => home()?,
        _ => match text.strip_prefix("~/") {
            Some(rest) => home()?.join(rest),
            None => std::path::PathBuf::from(text),
        },
    };
    let path = match path.is_absolute() {
        true => path,
        false => std::env::current_dir()
            .map_err(|e| format!("cannot tell the current folder: {e}"))?
            .join(path),
    };
    match path.is_file() {
        true => Ok(path.to_string_lossy().into_owned()),
        false => Err(format!("no such file: {}", path.display())),
    }
}

#[cfg(test)]
mod tests;
