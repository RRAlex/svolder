// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The setup wizard on the terminal: one step per screen, answered with the keys the key table
//! names.
//!
//! The walk itself is [`Wizard`]'s: this reads what it asks, draws it, turns a key press into
//! the choice or the line that answers it, and hands the answer back. Nothing is printed while
//! the loop runs (the alternate screen is up), so a refused answer becomes a notice on the step
//! it was refused on, and what the run leaves with is said by the caller once the terminal is
//! back.
//!
//! Waiting is drawn rather than waited out blind: the seconds a scan, a connect or the adapter
//! grading takes are spent under a frame that names them, with the two give-up keys read
//! throughout, exactly as the wait for a device is. A wait over an effect that changes the
//! machine is seen through before either key is honoured, so a write that has begun is never
//! left half-told.

use std::io;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use futures::{Stream, StreamExt};
use ratatui::Terminal;
use ratatui::backend::Backend;
use svolder_ipc::{Config, DeviceConfig};

use crate::backend::{Failure, Globals};
use crate::recovery::Kind;
use crate::wizard::{Effects, Prompt, Step, Wizard};

use super::app::Screen;
use super::keys::{self, Scene};
use super::mouse::{self, Hit, HitMap};
use super::palette::detect_depth;
use super::run::{self, Look, Opener};
use super::start::{Live, landed, leave_unopened, watch};
use super::terminal::{Claim, Stdout};
use super::theme::{Role, Theme};
use super::waiting::{Interrupted, WaitSignal, connecting, waiting};
use super::widgets::passphrase_row::PassphraseRow;
use super::widgets::spinner;
use super::{TuiOptions, render, screens};

/// The title a frame carries before the wizard has said what the first step is.
const OPENING: &str = "Setting up";

/// The choice that asks for nothing to be done, which is the one answer to a step that spends
/// no time at all: nothing is waited for, so nothing says it is being waited for.
const SKIP: &str = "Skip";

/// What a wait that has been asked to end says while the effect under it is seen through.
const FINISHING: &str = "finishing…";

/// What the last step says about the device it is handing over to.
fn opens_with(name: &str) -> String {
    format!("Enter opens {name}.")
}

/// What one frame of the wizard is drawn with: the terminal, the map of where the last frame
/// put what, and the styles every row is drawn in.
///
/// The three travel together — every frame needs all of them, and every wait draws a frame —
/// so they are handed round as one rather than as three arguments repeated at each call.
pub(super) struct Canvas<'a, B: Backend> {
    /// The terminal the frames go to.
    terminal: &'a mut Terminal<B>,
    /// Where the last frame put the rows a click can reach.
    hits: &'a mut HitMap,
    /// The styles the rows are drawn in.
    theme: &'a Theme,
}

impl<'a, B: Backend> Canvas<'a, B> {
    /// Draw on `terminal` in `theme`, recording what one frame drew into `hits`.
    pub(super) fn new(
        terminal: &'a mut Terminal<B>,
        hits: &'a mut HitMap,
        theme: &'a Theme,
    ) -> Canvas<'a, B> {
        Canvas {
            terminal,
            hits,
            theme,
        }
    }

    /// Where the last frame put the rows a click can reach.
    fn hits(&self) -> &HitMap {
        self.hits
    }
}

/// Whether a walk has a screen to go back to when it is left from its first step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Leaving {
    /// The TUI opened the walk over one of its screens: `Esc` on the first step leaves the
    /// walk, and the TUI draws that screen again.
    ToTui,
    /// The walk is the whole run (`svolder setup`): `Esc` on the first step leaves it for later,
    /// and on a walk that starts further on it does nothing.
    Nowhere,
}

/// How the wizard ended.
pub(super) enum WizardEnd {
    /// The walk was left for later: the interface opens on an empty Devices screen.
    Later,
    /// There is nothing to open: the user left, or the run reached its last step without a
    /// device to show.
    Quit,
    /// The wizard wrote this device, and the device screen opens on it.
    Added(DeviceConfig),
    /// The run ended on its own: the code to leave with, and what to say once the terminal is
    /// back.
    Stopped {
        /// The exit code the run earned.
        code: u8,
        /// What to say about it, without the `svolder: ` the caller prints it behind.
        why: String,
    },
}

/// One step as the wizard is showing it.
///
/// The prompt is the wizard's own; everything beside it is what this front end adds — where the
/// cursor is, what has been typed into a step that takes a line, what a refused answer said,
/// and what is being waited for.
pub(super) struct ShownStep {
    /// What the step being shown says and offers.
    pub(super) prompt: Prompt,
    /// Which choice an answer would take.
    pub(super) choice: usize,
    /// Whether this step is answered by typing a line rather than by choosing.
    pub(super) typing: bool,
    /// The line as it has been typed.
    pub(super) typed: String,
    /// The Passphrase step's row, which stands in for the typed line there: `none`, `auto` or a
    /// code, stepped between with the arrows and typed in digit by digit.
    pub(super) code: Option<PassphraseRow>,
    /// Where the cursor sits in that line, in characters from its start.
    pub(super) caret: usize,
    /// What a refused answer said, until the step is answered or left.
    pub(super) notice: Option<String>,
    /// What is being waited for, while something is.
    pub(super) waiting: Option<String>,
    /// The spinner's glyph in front of [`ShownStep::waiting`]'s words, kept alongside it: `None`
    /// exactly when `waiting` is, which is what keeps the spinner off a step that is asking
    /// rather than waiting.
    pub(super) spinner: Option<char>,
    /// The device the last step hands over to, once the walk has one.
    pub(super) opens: Option<String>,
    /// Whether the wizard can still go back from this step.
    pub(super) can_go_back: bool,
    /// How far the step's own words have been scrolled, in rows.
    pub(super) scroll: usize,
    /// How the last frame laid those words out, which is what a page of them is.
    pub(super) said: Said,
}

/// How one frame laid the step out.
///
/// A page is what the last frame showed, and how far the words can be carried is what it did
/// not: the keys that scroll work against the frame the reader is looking at rather than
/// against a width and a height they would have to work out again. `window` is the other half
/// of the same idea for a choice list too long to draw whole.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Said {
    /// Rows of those words the frame drew.
    pub(super) rows: usize,
    /// How many rows there are in all.
    pub(super) lines: usize,
    /// Which choice the first row of a windowed list showed, so the next frame keeps the
    /// window where the reader last saw it rather than recentring under their cursor.
    pub(super) window: usize,
}

impl ShownStep {
    /// A wizard with nothing to show yet: what is on the screen while the first step is being
    /// worked out.
    pub(super) fn opening() -> ShownStep {
        ShownStep {
            prompt: Prompt {
                step: Step::Health,
                title: OPENING.to_string(),
                body: Vec::new(),
                choices: Vec::new(),
                default: 0,
            },
            choice: 0,
            typing: false,
            typed: String::new(),
            code: None,
            caret: 0,
            notice: None,
            waiting: None,
            spinner: None,
            opens: None,
            can_go_back: false,
            scroll: 0,
            said: Said::default(),
        }
    }

    /// Show `prompt`.
    ///
    /// A prompt the wizard asks again unchanged — a name another device carries, a passphrase
    /// the device turned away — leaves the cursor and the typed line where they are, so the
    /// answer that was refused is there to be corrected rather than typed out afresh.
    pub(super) fn show(&mut self, prompt: Prompt) {
        if prompt == self.prompt {
            return;
        }
        self.choice = prompt.default.min(prompt.choices.len().saturating_sub(1));
        self.typing = prompt.choices.is_empty();
        // The step that asks for a passphrase is answered on a row of its own: the code, since
        // the connection the wizard tried on the advertised one has just been turned away.
        self.code = (prompt.step == Step::Passphrase).then(PassphraseRow::code);
        self.typed.clear();
        self.caret = 0;
        self.scroll = 0;
        self.said = Said::default();
        self.prompt = prompt;
    }

    /// Say which device the last step hands over to, which is the line that step ends on.
    ///
    /// Every other step ends on its own words: there is nothing to open until the walk has
    /// written something, and what the walk wrote is the wizard's to say.
    pub(super) fn hands_over_to(&mut self, device: Option<&DeviceConfig>) {
        self.opens = match self.prompt.step == Step::Done {
            true => device.map(|device| opens_with(&device.name)),
            false => None,
        };
    }

    /// A frame that says only what is being waited for, under `title`.
    pub(super) fn waiting_on(title: &str, waiting: &str) -> ShownStep {
        let mut step = ShownStep::opening();
        step.prompt.title = title.to_string();
        step.wait_for(Some(waiting.to_string()));
        step
    }

    /// Show `waiting` as what is being waited for now, the spinner in front of it starting at
    /// the glyph a wait opens on: `None` says nothing is being waited for, which is what keeps
    /// the spinner off the screen too.
    pub(super) fn wait_for(&mut self, waiting: Option<String>) {
        self.spinner = waiting.is_some().then(|| spinner::glyph(Duration::ZERO));
        self.waiting = waiting;
    }

    /// Turn the spinner to its glyph `elapsed` after the wait now showing began, while
    /// something still is: a tick that lands after the wait has already ended turns nothing.
    pub(super) fn spin(&mut self, elapsed: Duration) {
        if self.waiting.is_some() {
            self.spinner = Some(spinner::glyph(elapsed));
        }
    }

    /// The lines the step needs read.
    ///
    /// On a step that takes a typed line the last of them is what an empty line answers with,
    /// which the field itself shows, so it is not also written out above the field. The last
    /// step ends on what this front end does next, which is open the device it added.
    pub(super) fn read(&self) -> Vec<&str> {
        let body = match self.typing {
            true => self
                .prompt
                .body
                .split_last()
                .map_or(&[][..], |(_, rest)| rest),
            false => &self.prompt.body,
        };
        let mut lines: Vec<&str> = body.iter().map(String::as_str).collect();
        lines.extend(self.opens.as_deref());
        lines
    }

    /// What an empty line answers this step with, for the field to show while nothing has been
    /// typed.
    pub(super) fn placeholder(&self) -> Option<&str> {
        match self.typing {
            true => self.prompt.body.last().map(String::as_str),
            false => None,
        }
    }

    /// What the keys that are not always worth pressing are answered against here: a step that
    /// takes a typed line is one where `q` is a letter rather than a way out, and a step whose
    /// words are all on the screen is one with nothing to scroll.
    pub(super) fn scene(&self) -> Scene {
        self.scene_of(self.said)
    }

    /// The same, against the frame `said` describes: the frame being drawn right now, whose
    /// own key line has to say what that frame can do rather than what the one before it could.
    pub(super) fn scene_of(&self, said: Said) -> Scene {
        Scene {
            typing: self.typing,
            stepping: self.code.is_some(),
            scrolls: said.lines > said.rows,
            can_go_back: self.can_go_back,
            ..Scene::default()
        }
    }

    /// Carry the step's own words `pages` screenfuls on, stopping at each end.
    ///
    /// A page is the rows the last frame gave them, so what a press moves is what the reader
    /// just read; a step whose words all fit has nowhere to carry them.
    fn scroll_by(&mut self, pages: isize) {
        let page = isize::try_from(self.said.rows.max(1)).unwrap_or(1);
        let most = self.said.lines.saturating_sub(self.said.rows);
        let at = isize::try_from(self.scroll).unwrap_or(0) + pages * page;
        self.scroll = usize::try_from(at).unwrap_or(0).min(most);
    }

    /// What answering this step asks the wizard for: the choice the cursor is on, and the line
    /// where the step takes one.
    ///
    /// A code of fewer than four digits is not an answer: it is refused here, where the answer is
    /// built, with the words that say so, rather than handed to the wizard as an empty line.
    fn answer(&self) -> Result<(usize, Option<String>), &'static str> {
        if let Some(row) = self.code.as_ref() {
            return row.text().map(|code| (0, Some(code)));
        }
        Ok(match self.typing {
            true => (0, Some(self.typed.clone())),
            false => (self.choice, None),
        })
    }

    /// Move the cursor `delta` choices along, stopping at each end: there is no wrapping here,
    /// where the list is the answer rather than a ring to spin through.
    fn step_choice(&mut self, delta: isize) {
        let last = self.prompt.choices.len().saturating_sub(1);
        let at = isize::try_from(self.choice).unwrap_or(0) + delta;
        self.choice = usize::try_from(at).unwrap_or(0).min(last);
    }

    /// Type `letter` where the cursor is.
    fn insert(&mut self, letter: char) {
        let at = self.byte_at(self.caret);
        self.typed.insert(at, letter);
        self.caret += 1;
    }

    /// Delete the character before the cursor.
    fn erase(&mut self) {
        if self.caret == 0 {
            return;
        }
        let at = self.byte_at(self.caret - 1);
        self.typed.remove(at);
        self.caret -= 1;
    }

    /// Where character `at` of the typed line starts, in bytes.
    fn byte_at(&self, at: usize) -> usize {
        self.typed
            .char_indices()
            .nth(at)
            .map_or(self.typed.len(), |(at, _)| at)
    }

    /// How many characters have been typed.
    fn typed_len(&self) -> usize {
        self.typed.chars().count()
    }
}

/// Walk `wizard` as screens until it has nothing left to ask, or until the user leaves.
///
/// One step is shown at a time and answered where it stands: `Enter` takes the choice the
/// cursor is on or the line as it was typed, the arrows move between the choices or along the
/// line, `PgUp` and `PgDn` carry a step with more to say than the screen shows, `Esc` goes back a
/// step (on the first step it leaves the walk when `leaving` says there is a screen to return
/// to, until the device has been written, and otherwise leaves it for later), and `q` or Ctrl-C
/// leaves (on a step that takes a typed line, `q` is just a letter, and Ctrl-C is the way out).
/// An answer the wizard refuses is said on the step it was refused on and asked again; an effect
/// the machine would not carry out ends the run with the code it earned, since asking the same
/// unwritable file the same thing again would not help.
///
/// `events` is the terminal's input, read but never created here, so a test can script it;
/// `terminal` is drawn to once per event and once per wait.
pub(super) async fn event_loop<E, B, S>(
    terminal: &mut Terminal<B>,
    events: &mut S,
    wizard: &mut Wizard<E>,
    theme: &Theme,
    leaving: Leaving,
) -> WizardEnd
where
    E: Effects,
    B: Backend,
    S: Stream<Item = io::Result<Event>> + Unpin,
{
    let end = walk(terminal, events, wizard, theme, leaving).await;
    // However the walk ended, a device it wrote is the daemon's to know of.
    wizard.tell_the_daemon().await;
    end
}

/// The walk itself, ending however it ends.
async fn walk<E, B, S>(
    terminal: &mut Terminal<B>,
    events: &mut S,
    wizard: &mut Wizard<E>,
    theme: &Theme,
    leaving: Leaving,
) -> WizardEnd
where
    E: Effects,
    B: Backend,
    S: Stream<Item = io::Result<Event>> + Unpin,
{
    let mut view = ShownStep::opening();
    let mut hits = HitMap::default();
    let mut canvas = Canvas::new(terminal, &mut hits, theme);
    // Whether the terminal has anything left to say. A stream that has ended answers the
    // moment it is asked — and is not to be asked again — so once it has, the run is over.
    let mut reading = true;
    loop {
        // What the next step spends before it can be shown is named while it is spent. Nothing
        // has been asked for yet, so giving up here drops the work and leaves.
        view.wait_for(wizard.busy());
        let asked = wait_on(
            &mut canvas,
            events,
            &mut reading,
            &mut view,
            CANCELS,
            wizard.prompt(),
        )
        .await;
        view.wait_for(None);
        // Nothing came back at all: the user left while the step was being worked out.
        let Waited::Done(asked) = asked else {
            return WizardEnd::Quit;
        };
        // The wizard has nothing left to ask, which is what the last step answered leaves.
        let Some(prompt) = asked else {
            break;
        };
        view.show(prompt);
        // The last step ends on what this front end does next: open the device it added.
        view.hands_over_to(wizard.added());
        let returns = leaving == Leaving::ToTui && wizard.added().is_none();
        let for_later = leaving == Leaving::Nowhere && wizard.offers_later();
        view.can_go_back = wizard.can_go_back() || returns || for_later;
        loop {
            // What the frame made of the step's own words is what the keys that scroll them
            // move by, and how far they may move.
            match draw(&mut canvas, &view) {
                Ok(said) => view.said = said,
                Err(e) => {
                    return WizardEnd::Stopped {
                        code: 1,
                        why: e.to_string(),
                    };
                }
            }
            let event = match reading {
                true => events.next().await,
                false => None,
            };
            let Some(Ok(event)) = event else {
                return WizardEnd::Stopped {
                    code: 1,
                    why: super::run::INPUT_ENDED.to_string(),
                };
            };
            let wanted = match event {
                // A release is the other half of a press this loop has already acted on.
                Event::Key(key) if key.kind != KeyEventKind::Release => press(&mut view, key),
                Event::Mouse(click) => clicked(&mut view, canvas.hits(), click),
                _ => Asked::Nothing,
            };
            match wanted {
                Asked::Nothing => {}
                Asked::Quit => return WizardEnd::Quit,
                Asked::Back if wizard.can_go_back() => {
                    wizard.back();
                    view.notice = None;
                    break;
                }
                Asked::Back if returns => return WizardEnd::Quit,
                Asked::Back if for_later => {
                    wizard.leave_for_later();
                    return WizardEnd::Later;
                }
                Asked::Back => {}
                Asked::Answer => {
                    let (choice, text) = match view.answer() {
                        Ok(answer) => answer,
                        Err(why) => {
                            view.notice = Some(why.to_string());
                            continue;
                        }
                    };
                    view.wait_for(waiting_for(&view.prompt, choice));
                    let cancels = !writes(&view.prompt, choice);
                    let answered = wait_on(
                        &mut canvas,
                        events,
                        &mut reading,
                        &mut view,
                        cancels,
                        wizard.answer(choice, text),
                    )
                    .await;
                    view.wait_for(None);
                    let (answered, left) = match answered {
                        Waited::Done(answered) => (answered, false),
                        Waited::Left(answered) => (answered, true),
                        Waited::GaveUp => return WizardEnd::Quit,
                    };
                    if let Err(failure) = answered {
                        if wizard.stopped().is_some() {
                            return WizardEnd::Stopped {
                                code: failure.code,
                                why: failure.to_string(),
                            };
                        }
                        view.notice = Some(failure.to_string());
                    } else {
                        view.notice = None;
                    }
                    // The key that arrived mid-write is honoured now that the write is over:
                    // the user asked to leave, so the walk ends here rather than going on to
                    // the step after the one they left.
                    if left {
                        return WizardEnd::Quit;
                    }
                    break;
                }
            }
        }
    }
    if wizard.is_later() {
        return WizardEnd::Later;
    }
    match wizard.added() {
        Some(device) => WizardEnd::Added(device.clone()),
        None => WizardEnd::Quit,
    }
}

/// What one event asks the wizard for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    /// Nothing the wizard needs to hear about: the cursor moved, or a letter was typed.
    Nothing,
    /// Leave, with whatever the walk has already written left where it is.
    Quit,
    /// The step before this one.
    Back,
    /// Answer this step.
    Answer,
}

/// What one key press asks for, and whatever it does to the step on the way.
///
/// Ctrl-C leaves from wherever it is pressed, the terminal being in raw mode by now; every
/// other key is read against the step being shown, so the letters of a name are letters on the
/// step that asks for one and keys everywhere else.
fn press(view: &mut ShownStep, key: KeyEvent) -> Asked {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Asked::Quit;
    }
    // Ctrl and Alt make a key a combination rather than the key it is, wherever it lands: the
    // only one this screen answers is the Ctrl-C above.
    if held(key) {
        return Asked::Nothing;
    }
    match key.code {
        KeyCode::Enter => return Asked::Answer,
        KeyCode::Esc => return Asked::Back,
        KeyCode::PageUp => {
            view.scroll_by(-1);
            return Asked::Nothing;
        }
        KeyCode::PageDown => {
            view.scroll_by(1);
            return Asked::Nothing;
        }
        _ => {}
    }
    if let Some(row) = view.code.as_mut() {
        if row.key(key.code) {
            view.notice = None;
        }
        return Asked::Nothing;
    }
    match view.typing {
        false => match key.code {
            KeyCode::Up => view.step_choice(-1),
            KeyCode::Down => view.step_choice(1),
            KeyCode::Char('q') => return Asked::Quit,
            _ => {}
        },
        true => match key.code {
            KeyCode::Char(letter) => view.insert(letter),
            KeyCode::Backspace => view.erase(),
            KeyCode::Left => view.caret = view.caret.saturating_sub(1),
            KeyCode::Right => view.caret = (view.caret + 1).min(view.typed_len()),
            KeyCode::Home => view.caret = 0,
            KeyCode::End => view.caret = view.typed_len(),
            _ => {}
        },
    }
    Asked::Nothing
}

/// Whether `key` is held down with Ctrl or Alt, which is what makes it a combination rather
/// than the character it would otherwise type.
fn held(key: KeyEvent) -> bool {
    key.modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

/// What one click asks for, against the rows the last frame drew.
///
/// A click on a choice takes the cursor there, and a second on the row it is already on
/// answers with it: the two presses a keyboard makes, in the order it makes them. A click on a
/// key of the key line is that key, exactly as it is on every other screen.
fn clicked(view: &mut ShownStep, hits: &HitMap, event: MouseEvent) -> Asked {
    if event.kind != MouseEventKind::Down(MouseButton::Left) {
        return Asked::Nothing;
    }
    match hits.hit_at(event.column, event.row) {
        Some(Hit::ListRow(at)) if *at < view.prompt.choices.len() => {
            if view.choice == *at {
                return Asked::Answer;
            }
            view.choice = *at;
            Asked::Nothing
        }
        Some(Hit::Footer(at)) => match keys::hints_for(Screen::Setup)
            .get(*at)
            .filter(|hint| keys::available_in(hint, view.scene()))
        {
            Some(hint) => press(view, KeyEvent::from(hint.code)),
            None => Asked::Nothing,
        },
        _ => Asked::Nothing,
    }
}

/// Whether a wait may be given up on, which is what the waits that write nothing are.
const CANCELS: bool = true;

/// Whether answering `prompt` with `choice` changes the machine.
///
/// A write that has begun is seen through rather than dropped part-way: the fixes are applied,
/// the device goes into the file and a daemon that is running is told about it, the adapter is
/// written, the units are installed. A scan and a connect change nothing, so a wait over one of
/// those is given up on where it stands.
fn writes(prompt: &Prompt, choice: usize) -> bool {
    if prompt.choices.get(choice).is_some_and(|name| name == SKIP) {
        return false;
    }
    match prompt.step {
        Step::Fixes | Step::Name | Step::Passphrase | Step::Adapter | Step::Daemon => true,
        Step::Health | Step::Scan | Step::Pick | Step::Done => false,
    }
}

/// What answering `prompt` with `choice` spends its seconds on, in the words the wait is drawn
/// under, or nothing for an answer that is over the moment it is given.
fn waiting_for(prompt: &Prompt, choice: usize) -> Option<String> {
    if prompt.choices.get(choice).is_some_and(|name| name == SKIP) {
        return None;
    }
    let waiting = match prompt.step {
        Step::Fixes => "applying the fixes…",
        Step::Scan => "listening…",
        Step::Name | Step::Passphrase => "connecting…",
        Step::Daemon => "installing the user service…",
        Step::Health | Step::Pick | Step::Adapter | Step::Done => return None,
    };
    Some(waiting.to_string())
}

/// Wait for `work`, with `view` on the screen and the give-up keys read throughout, its spinner
/// turning every hundred milliseconds while a wait is named under it.
///
/// The terminal is in raw mode, so Ctrl-C arrives as a key rather than a signal: without this
/// the user would be held on a frozen frame for as long as a scan or a connect takes, with
/// nothing to end it. What those two keys do depends on what is being waited for: a wait that
/// `cancels` ends on them, dropping the work where it stands; one that does not is seen through
/// (the frame says so), with the key honoured the moment the work answers, so a write that has
/// begun is never left half-told. Every other event is read and dropped, so a key pressed at a
/// screen that wasn't asking for one doesn't carry over to the next step.
///
/// A frame that will not draw is not what ends a wait: the work is still worth waiting for, and
/// the loop reports the terminal the moment it tries to draw the answer.
pub(super) async fn wait_on<T, B, S>(
    canvas: &mut Canvas<'_, B>,
    events: &mut S,
    reading: &mut bool,
    view: &mut ShownStep,
    cancels: bool,
    work: impl Future<Output = T>,
) -> Waited<T>
where
    B: Backend,
    S: Stream<Item = io::Result<Event>> + Unpin,
{
    if let Ok(said) = draw(canvas, view) {
        view.said = said;
    }
    let since = Instant::now();
    let mut left = false;
    let done = waiting(work, events, reading, |signal| match signal {
        WaitSignal::GaveUp => {
            if cancels {
                return Interrupted::Stop;
            }
            if !left {
                left = true;
                view.wait_for(Some(FINISHING.to_string()));
                if let Ok(said) = draw(canvas, view) {
                    view.said = said;
                }
            }
            Interrupted::Carry
        }
        WaitSignal::Tick(now) => {
            view.spin(now.saturating_duration_since(since));
            if view.waiting.is_some()
                && let Ok(said) = draw(canvas, view)
            {
                view.said = said;
            }
            Interrupted::Carry
        }
    })
    .await;
    match (done, left) {
        (None, _) => Waited::GaveUp,
        (Some(done), true) => Waited::Left(done),
        (Some(done), false) => Waited::Done(done),
    }
}

/// What a wait came to.
pub(super) enum Waited<T> {
    /// The work answered.
    Done(T),
    /// The work answered, and the user asked to leave while it ran.
    Left(T),
    /// The user gave up before it answered, and the work was dropped.
    GaveUp,
}

/// Draw one frame of `view`, recording where a click can answer it.
///
/// The hit map describes one frame only, so it is emptied here and filled by the screen, which
/// is what [`render::draw`] does for every other screen. A terminal too small for a screen gets
/// the same notice they get.
fn draw<B: Backend>(canvas: &mut Canvas<B>, view: &ShownStep) -> Result<Said, B::Error> {
    let mut said = Said::default();
    let (hits, theme) = (&mut *canvas.hits, canvas.theme);
    canvas.terminal.draw(|frame| {
        hits.clear();
        let area = frame.area();
        frame.buffer_mut().set_style(area, theme.style(Role::Text));
        if render::fits(area) {
            said = screens::wizard::draw(frame, area, view, theme, hits);
        } else {
            render::too_small(frame, area, theme);
        }
    })?;
    Ok(said)
}

/// Walk `wizard` as the setup wizard, and open what it added.
///
/// This is the guided first run's TUI front end: one step per screen under the wordmark, and,
/// when the walk ends with a device written, the device screen on that device without leaving
/// the alternate screen (the mark has been up throughout, so there is no intro to play).
///
/// A terminal that cannot be claimed is no reason to give up on the walk: the wizard comes back
/// in the `Err` for the plain front end to run, with the reason already printed. A walk that
/// wrote a device opens it, or waits for it; one the user left for later opens the empty
/// Devices screen. A quit leaves with success, and an effect the machine refused with its own
/// code. Nothing is printed while the screen is claimed; what a run has to say is said here,
/// once the terminal is back.
pub(crate) async fn setup<E: Effects>(
    g: &Globals<'_>,
    wizard: Wizard<E>,
    opts: TuiOptions,
) -> Result<ExitCode, Wizard<E>> {
    setup_on(
        &Stdout,
        &mut std::io::stderr(),
        Config::load().unwrap_or_default(),
        Config::path(),
        g,
        wizard,
        opts,
    )
    .await
}

/// [`setup`](fn@setup) over a given terminal claim, the writer its fallback line goes to, the
/// loaded configuration and the path it was loaded from.
async fn setup_on<E: Effects>(
    claim: &impl Claim,
    err: &mut impl std::io::Write,
    config: Config,
    config_path: std::path::PathBuf,
    g: &Globals<'_>,
    mut wizard: Wizard<E>,
    opts: TuiOptions,
) -> Result<ExitCode, Wizard<E>> {
    // A run whose output is being read is one the plain front end belongs to, and it says so
    // for itself rather than being told: there is nothing here worth a message.
    if !claim.is_terminal() {
        return Err(wizard);
    }
    // The `[tui]` table says only how to draw. A file that cannot be read is not a reason to
    // refuse to draw — the wizard is here to write into that very file — so the defaults stand
    // (the caller hands them over in that case).
    let look = Look::new(
        config.tui.clone(),
        opts.no_color,
        detect_depth(),
        config_path.clone(),
    );
    let mut guard = match claim.enter(config.tui.mouse) {
        Ok(guard) => guard,
        // The walk itself is not over: the plain front end asks the very same steps, and this
        // says why it is the one asking them.
        Err(e) => {
            let _ = writeln!(
                err,
                "svolder: {}",
                Failure::of(
                    Kind::Usage,
                    format!("the terminal: {e}; asking in plain prompts")
                )
            );
            return Err(wizard);
        }
    };
    let mut events = EventStream::new();
    let end = event_loop(
        &mut guard.terminal,
        &mut events,
        &mut wizard,
        &look.theme,
        Leaving::Nowhere,
    )
    .await;
    let device = match end {
        WizardEnd::Added(device) => device,
        WizardEnd::Quit => {
            drop(guard);
            return Ok(ExitCode::SUCCESS);
        }
        // Nothing is configured and the user chose to look at the interface anyway: it opens
        // on the empty Devices screen, where `n` walks again.
        WizardEnd::Later => {
            let opener = Opener::new(
                g.owned(),
                config.tui.clone(),
                config_path.clone(),
                run::Live { only: None },
            );
            let landing = run::Landing::Devices { why: None };
            let opened = run::land(&opener, landing).await;
            return Ok(watch(
                &opener,
                &Live {
                    g,
                    path: config_path,
                    refuse: None,
                },
                opened,
                guard,
                events,
                look,
            )
            .await);
        }
        WizardEnd::Stopped { code, why } => {
            drop(guard);
            eprintln!("svolder: {why}");
            return Ok(ExitCode::from(code));
        }
    };
    // The device the walk just wrote is opened the way the TUI opens any device, under a frame
    // that says which one is being reached for, and handed to the same loop `run` hands its
    // own to.
    let opener = Opener::new(
        g.owned(),
        config.tui.clone(),
        config_path.clone(),
        run::Live { only: None },
    );
    let opening = opener.reopen(Some(&device.name), None);
    let mut waiting = ShownStep::waiting_on(&device.name, &connecting(&device.name));
    let mut hits = mouse::HitMap::default();
    let mut reading = true;
    let mut canvas = Canvas::new(&mut guard.terminal, &mut hits, &look.theme);
    let Waited::Done(opened) = wait_on(
        &mut canvas,
        &mut events,
        &mut reading,
        &mut waiting,
        // The device screen is what this is on its way to: giving up on it gives up on that
        // and leaves, the way giving up on the device the TUI opens on does.
        true,
        opening,
    )
    .await
    else {
        return Ok(leave_unopened(guard, err));
    };
    let opened = landed(&opener, Some(&device.name), opened).await;
    Ok(watch(
        &opener,
        &Live {
            g,
            path: config_path,
            refuse: None,
        },
        opened,
        guard,
        events,
        look,
    )
    .await)
}

#[cfg(test)]
mod tests;
