// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Scenes box: the scenes the daemon keeps, in a box over any screen, to recall one, save the
//! sound as it is now, save over one, rename one and delete one.
//!
//! The daemon owns scenes: this side reads the [`ScenesView`] its feed carries and sends
//! [`SceneOp`]s, and judges nothing itself (which scene is on, which parts a scene has, what a name
//! may be). Rows, lamps and parts are the view's own; a refusal and a recall's outcome are said in
//! a toast in the daemon's words. Scenes are offered only while the daemon lists
//! [`FEATURE_SCENES`](svolder_ipc::FEATURE_SCENES) and a view has arrived ([`ScenesBox::offered`]).
//! Nothing here writes: a key answers with the [`Intent`]s the loop sends, and what the daemon
//! says comes back as a [`SceneDone`].

use crossterm::event::KeyCode;
use svolder_ipc::socket::protocol::{PartOutcome, PartStatus, SceneOp, ScenesView};
use svolder_profiles::tone::preset::NAME_MAX;

use crate::tui::app::{App, Intent, stepped};

/// What `z` says where this session holds no daemon, which keeps the scenes.
pub const NEEDS_DAEMON: &str = "scenes are kept by the daemon: svolder daemon install";
/// What `z` says where the daemon lists no scenes.
pub const NOT_KEPT: &str = "this svolderd keeps no scenes";
/// What `z` says while the daemon lists scenes but its view has not come yet.
pub const READING: &str = "scenes: reading…";
/// What the box suggests a first scene is called.
const FIRST_NAME: &str = "Scene 1";

/// The scenes' state: what the daemon last said of them and the box over the screens.
#[derive(Debug, Default)]
pub struct ScenesBox {
    /// Whether the daemon lists the scenes feature, once it has said what it is.
    pub served: Option<bool>,
    /// The scenes and which one is on, as the daemon's feed last carried them.
    pub view: Option<ScenesView>,
    /// The words for the request in flight (`Recalling Movie…`), while one is: the box takes no
    /// key but `Esc` until the daemon answers, so one press is one request.
    pub pending: Option<String>,
    /// The box over the screens, while it is open.
    pub list: Option<SceneList>,
}

impl ScenesBox {
    /// Whether scenes are on offer: the daemon lists them and has sent its view.
    pub fn offered(&self) -> bool {
        self.served == Some(true) && self.view.is_some()
    }

    /// The row the cursor is on, held inside the rows there are.
    pub fn focused(&self) -> Option<&str> {
        let view = self.view.as_ref()?;
        let list = self.list.as_ref()?;
        let row = list.row.min(view.scenes.len().checked_sub(1)?);
        view.scenes.get(row).map(|scene| scene.name.as_str())
    }
}

/// The box, while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneList {
    /// The focused row, as an index into the view's scenes.
    pub row: usize,
    /// What the box is showing.
    pub step: Step,
    /// The scene to put the cursor on once the view lists it: the one just saved or renamed.
    pub follow: Option<String>,
}

/// What the box's last row is showing.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// The scenes themselves, to move through and act on.
    Choosing,
    /// A name being typed, for a new scene or a rename.
    Naming(NameLine),
    /// A `[y/N]` question.
    Asking(Question),
}

/// The one line the name prompt takes.
#[derive(Debug, Clone, PartialEq)]
pub struct NameLine {
    /// The scene being renamed, as the file spells it, or `None` for a new one.
    pub renaming: Option<String>,
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
            caret: text.chars().count(),
            text,
        }
    }
}

/// What the box asks before it acts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Whether to save the sound as it is now over the scene of this name.
    Update(String),
    /// Whether to delete the scene of this name, for every device.
    Delete(String),
    /// The daemon's question about a recall that plays one preset twice: the scene's name and
    /// the daemon's words; yes recalls the scene as it was saved.
    Both {
        /// The scene.
        name: String,
        /// The question in the daemon's words.
        message: String,
    },
}

/// What a request to the daemon ended with.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneDone {
    /// The daemon did it: the scene's name as the file now spells it, and for a recall how each
    /// part fared.
    Answered {
        /// The op asked.
        op: SceneOp,
        /// The scene's name as the file spells it.
        name: String,
        /// One entry per part of a recalled scene.
        outcome: Vec<PartOutcome>,
    },
    /// The daemon asked whether a recall that plays a preset twice should go ahead.
    Asked {
        /// The op asked.
        op: SceneOp,
        /// The question in the daemon's words.
        message: String,
    },
    /// The daemon refused, or could not be reached: its words.
    Refused {
        /// The op asked.
        op: SceneOp,
        /// What it said.
        words: String,
    },
}

/// What a recall's outcome says: `NAME recalled.` and then what each part's notes say, in the
/// daemon's words; a part that failed or was set in part with no note says so itself.
pub fn recalled_words(name: &str, outcome: &[PartOutcome]) -> String {
    // A recall in which no part was set, whole or in part, did not happen.
    let set = outcome
        .iter()
        .any(|part| matches!(part.status, PartStatus::Done | PartStatus::Partial));
    let mut words = vec![match set || outcome.is_empty() {
        true => format!("{name} recalled."),
        false => format!("{name} was not recalled."),
    }];
    for part in outcome {
        match (part.status, part.notes.is_empty()) {
            (PartStatus::Failed, true) => words.push(format!("{} failed.", part.who)),
            (PartStatus::Partial, true) => words.push(format!("{} was set in part.", part.who)),
            (PartStatus::Left, true) => words.push(format!("{} was left as it is.", part.who)),
            _ => words.extend(part.notes.iter().cloned()),
        }
    }
    words.join(" ")
}

/// What a save says: `lead`, then the daemon's note for each device it left out of the scene.
fn saved_words(lead: &str, outcome: &[PartOutcome]) -> String {
    let mut words = vec![lead.to_string()];
    for part in outcome {
        words.extend(part.notes.iter().cloned());
    }
    words.join(" ")
}

/// Whether a recall's outcome left anything out, which its toast says in the warning role.
pub fn left_out(outcome: &[PartOutcome]) -> bool {
    outcome
        .iter()
        .any(|part| !matches!(part.status, PartStatus::Done | PartStatus::Unknown))
}

/// The name `n` offers: the focused scene's name numbered past the names the view has, or a
/// first name for no scene at all.
fn suggestion(view: &ScenesView, focused: Option<&str>) -> String {
    let Some(base) = focused else {
        return FIRST_NAME.to_string();
    };
    (2u32..)
        .map(|n| {
            // The base is cut so the numbered name is one a name may be.
            let tail = format!(" {n}");
            let room = NAME_MAX.saturating_sub(tail.chars().count());
            format!("{}{tail}", base.chars().take(room).collect::<String>())
        })
        .find(|name| {
            !view
                .scenes
                .iter()
                .any(|scene| scene.name.eq_ignore_ascii_case(name))
        })
        .unwrap_or_else(|| base.to_string())
}

/// Open the box on the scene that is on, or say why not: the daemon is not there, or keeps no
/// scenes. The box opens with its cursor on the lit row.
pub(in crate::tui) fn open(app: &mut App) -> Vec<Intent> {
    if !app.session.daemon {
        app.warn(NEEDS_DAEMON);
        return Vec::new();
    }
    if !app.scenes.offered() {
        // A daemon that lists scenes, or has not said yet, is still sending its view.
        match app.scenes.served {
            Some(false) => app.warn(NOT_KEPT),
            _ => app.warn(READING),
        }
        return Vec::new();
    }
    let row = app
        .scenes
        .view
        .as_ref()
        .and_then(|view| {
            let on = view.on.as_deref()?;
            view.scenes.iter().position(|scene| scene.name == on)
        })
        .unwrap_or(0);
    app.scenes.list = Some(SceneList {
        row,
        step: Step::Choosing,
        follow: None,
    });
    Vec::new()
}

/// Whether the box is open on its scenes, rather than on a prompt or a question.
pub fn choosing(app: &App) -> bool {
    app.scenes
        .list
        .as_ref()
        .is_some_and(|list| matches!(list.step, Step::Choosing))
}

/// Focus row `row` of the box, or its last row for one past the end.
pub fn focus(app: &mut App, row: usize) {
    let last = app
        .scenes
        .view
        .as_ref()
        .map_or(0, |view| view.scenes.len().saturating_sub(1));
    if let Some(list) = app.scenes.list.as_mut() {
        list.row = row.min(last);
    }
}

/// Take the view the daemon sent: the scenes the box lists, with the cursor following a scene
/// just saved or renamed once the view lists it.
pub(crate) fn viewed(app: &mut App, view: ScenesView) {
    app.scenes.view = Some(view);
    follow(app);
    let Some(view) = app.scenes.view.as_ref() else {
        return;
    };
    let last = view.scenes.len().saturating_sub(1);
    if let Some(list) = app.scenes.list.as_mut() {
        list.row = list.row.min(last);
    }
}

/// Put the cursor on the scene the box is waiting to see listed, when the view has it.
fn follow(app: &mut App) {
    let at = app.scenes.list.as_ref().and_then(|list| {
        let name = list.follow.as_deref()?;
        app.scenes
            .view
            .as_ref()?
            .scenes
            .iter()
            .position(|scene| scene.name == name)
    });
    if let (Some(at), Some(list)) = (at, app.scenes.list.as_mut()) {
        list.row = at;
        list.follow = None;
    }
}

/// What a key does while the box is open: the scenes' own keys, the name prompt's typing, or the
/// answer to a `[y/N]` question, and every other key swallowed.
pub(in crate::tui) fn key(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(list) = app.scenes.list.as_ref() else {
        return Vec::new();
    };
    // A request is out: one press is one request, so only `Esc` is taken until it is answered.
    if app.scenes.pending.is_some() && code != KeyCode::Esc {
        return Vec::new();
    }
    match &list.step {
        Step::Choosing => choose(app, code),
        Step::Naming(_) => type_name(app, code),
        Step::Asking(question) => {
            let question = question.clone();
            answer(app, code, question)
        }
    }
}

/// The scenes' own keys: `↑↓` move, `Enter` recalls the focused scene, `n` asks for a name to
/// save the sound under, `u` asks before it saves over the focused scene, `c` asks for its new
/// name, `x` asks before it deletes it, and `Esc` closes the box.
fn choose(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(view) = app.scenes.view.as_ref() else {
        app.scenes.list = None;
        return Vec::new();
    };
    let count = view.scenes.len();
    let focused = app.scenes.focused().map(str::to_string);
    let Some(list) = app.scenes.list.as_mut() else {
        return Vec::new();
    };
    let row = list.row.min(count.saturating_sub(1));
    match code {
        KeyCode::Esc => app.scenes.list = None,
        KeyCode::Up => list.row = stepped(row, -1, count),
        KeyCode::Down => list.row = stepped(row, 1, count),
        KeyCode::Enter => {
            return focused
                .map(|name| {
                    let op = SceneOp::Recall {
                        name,
                        confirm: false,
                    };
                    app.scenes.pending = Some(pending_words(&op));
                    vec![Intent::Scene(op)]
                })
                .unwrap_or_default();
        }
        KeyCode::Char('n') => {
            let text = suggestion(view, focused.as_deref());
            list.step = Step::Naming(NameLine::new(None, text));
        }
        KeyCode::Char('u') => {
            if let Some(name) = focused {
                list.step = Step::Asking(Question::Update(name));
            }
        }
        KeyCode::Char('c') => {
            if let Some(name) = focused {
                list.step = Step::Naming(NameLine::new(Some(name.clone()), name));
            }
        }
        KeyCode::Char('x') => {
            if let Some(name) = focused {
                list.step = Step::Asking(Question::Delete(name));
            }
        }
        _ => {}
    }
    Vec::new()
}

/// What a key does on the name prompt: the letters are typed where the cursor is, up to the most
/// a name takes, the arrows, `Home` and `End` move the cursor, `Backspace` takes the character
/// before it, `Enter` sends the name, and `Esc` goes back to the scenes with nothing sent.
fn type_name(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(list) = app.scenes.list.as_mut() else {
        return Vec::new();
    };
    let Step::Naming(line) = &mut list.step else {
        return Vec::new();
    };
    let len = line.text.chars().count();
    match code {
        KeyCode::Enter => {
            let op = match line.renaming.clone() {
                Some(from) => SceneOp::Rename {
                    from,
                    to: line.text.clone(),
                },
                None => SceneOp::Save {
                    name: line.text.clone(),
                    force: false,
                },
            };
            list.follow = Some(line.text.clone());
            list.step = Step::Choosing;
            app.scenes.pending = Some(pending_words(&op));
            return vec![Intent::Scene(op)];
        }
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
        KeyCode::Char(letter) if len < NAME_MAX => {
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

/// What a key does while a `[y/N]` question is open: `y` goes ahead, `n`, `Esc` and `Enter` (the
/// default is no) go back to the scenes with nothing sent, and every other key is swallowed.
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
    let op = match question {
        Question::Update(name) => SceneOp::Save { name, force: true },
        Question::Delete(name) => SceneOp::Delete { name },
        Question::Both { name, .. } => SceneOp::Recall {
            name,
            confirm: true,
        },
    };
    app.scenes.pending = Some(pending_words(&op));
    vec![Intent::Scene(op)]
}

/// What the box's last row says while `op` is out.
fn pending_words(op: &SceneOp) -> String {
    match op {
        SceneOp::Recall { name, .. } => format!("Recalling {name}…"),
        SceneOp::Save { name, .. } => format!("Saving {name}…"),
        SceneOp::Rename { from, .. } => format!("Renaming {from}…"),
        SceneOp::Delete { name } => format!("Deleting {name}…"),
    }
}

/// Go back from a question or the name prompt to the scenes themselves.
fn back_to_list(app: &mut App) {
    if let Some(list) = app.scenes.list.as_mut() {
        list.step = Step::Choosing;
    }
}

/// Fold what the daemon said to a request into the app: a recall's outcome or a save's name in a
/// toast, the daemon's question in the box's last row, and its refusal in its own words.
pub(crate) fn done(app: &mut App, done: SceneDone) -> Vec<Intent> {
    app.scenes.pending = None;
    match done {
        SceneDone::Answered { op, name, outcome } => match op {
            SceneOp::Recall { .. } => {
                let words = recalled_words(&name, &outcome);
                match left_out(&outcome) {
                    true => app.warn(words),
                    false => app.notify(words),
                }
                app.scenes.list = None;
            }
            SceneOp::Save { force: false, .. } => {
                app.notify(saved_words(&format!("Saved {name}."), &outcome));
                followed(app, &name);
            }
            SceneOp::Save { force: true, .. } => {
                app.notify(saved_words(&format!("Updated {name}."), &outcome));
            }
            SceneOp::Rename { .. } => {
                app.notify(format!("Renamed to {name}."));
                followed(app, &name);
            }
            SceneOp::Delete { .. } => app.notify(format!("Deleted {name}.")),
        },
        SceneDone::Asked { op, message } => {
            if let SceneOp::Recall { name, .. } = op {
                let list = app.scenes.list.get_or_insert(SceneList {
                    row: 0,
                    step: Step::Choosing,
                    follow: None,
                });
                list.step = Step::Asking(Question::Both { name, message });
            }
        }
        SceneDone::Refused { words, .. } => {
            if let Some(list) = app.scenes.list.as_mut() {
                list.follow = None;
            }
            app.warn(words);
        }
    }
    Vec::new()
}

/// Put the cursor on `name` now if the view lists it, and as soon as it does if not.
fn followed(app: &mut App, name: &str) {
    if let Some(list) = app.scenes.list.as_mut() {
        list.follow = Some(name.to_string());
    }
    follow(app);
}

#[cfg(test)]
mod tests;
