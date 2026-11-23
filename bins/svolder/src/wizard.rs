// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The guided first run: one state machine, and the front ends that drive it.
//!
//! [`Wizard`] walks nine steps — the host's health, the fixes svolder can
//! apply itself, a scan, the device, its name, its passphrase, the adapter that hears it best,
//! the daemon, and what was written. The scan, the device, its name and its passphrase are the
//! device walk of the `svolder_walk` crate, which the daemon runs too; the wizard embeds it with
//! effects over this machine. It renders each step as a [`Prompt`] and applies what an answer
//! asks for through [`Effects`], which is the whole of what it can do to the machine.
//!
//! Nothing here prints: a front end may be drawing on the alternate screen, so the core hands
//! out [`Prompt`]s and only [`cli`] and [`summary()`] put words on a terminal.
//!
//! - [`cli`]: the wizard as plain prompts on a terminal.
//! - `effects`: what the wizard can do to the machine it runs on, in three seams.
//! - `fake`: a machine the wizard can be walked over without touching this one.
//! - [`live`]: the wizard against the machine it is running on.
//! - `local`: the device walk's effects over the wizard's own.
//! - `remote`: the device walk as a client of the daemon's, whenever a daemon runs.
//! - `summary`: the wizard with nobody to ask, printed once.

use std::io::IsTerminal;
use std::process::ExitCode;

use svolder_ipc::checks::{self, Fix, Outcome};
use svolder_ipc::{Config, DeviceConfig};
use svolder_walk::{Configured, DeviceWalk, WalkError, Walked};

use crate::backend::Globals;
use crate::commands::daemon::install::{CONTAINER_DAEMON, Service};
use crate::recovery::{Failure, Kind};
use crate::scanning::{GRADE_WINDOW, GRADE_WINDOWS, Grade};

pub mod cli;
mod effects;
#[cfg(test)]
pub(crate) mod fake;
pub mod live;
mod local;
mod remote;
pub(crate) mod signals;
mod summary;

pub use effects::{Effects, Files, Host, Radio};
use local::Local;
use remote::{Answered, RemoteWalk};
pub use summary::summary;
pub use svolder_ipc::Connect;
pub use svolder_walk::{Given, Prompt, Step, Takes};

/// The last line of the wizard: where Home Assistant is set up, for a user who wants it.
const HOME_ASSISTANT_LINE: &str =
    "For Home Assistant: `svolder mqtt` or Settings, Connect in the TUI.";

/// The guided first run starts the daemon and opens BlueZ itself, so `--no-start` does not apply
/// to it, whichever door it is entered by (`setup`, `add` on a terminal, the bare `svolder` with
/// no configuration): it runs as `auto` would.
fn exempt<'a>(g: &Globals<'a>) -> Globals<'a> {
    Globals {
        transport: match g.transport == crate::backend::RUNNING {
            true => "auto",
            false => g.transport,
        },
        ..*g
    }
}

/// What `svolder setup` does, and what bare `svolder` does with nothing configured.
///
/// On a terminal it walks the wizard; off one it prints the summary, because a run whose output
/// is being read has no one to ask. Both need stdin as well as stdout: a run whose answers
/// would come from a file is one that would answer every question with the end of that file.
pub async fn setup(g: &Globals<'_>, note: Option<String>) -> ExitCode {
    let g = &exempt(g);
    if !on_a_terminal() {
        let effects = match live::LiveEffects::live(g, &Config::path()).await {
            Ok(effects) => effects,
            Err(failure) => return failure.report(),
        };
        return summary(&effects).await;
    }
    let effects = match live::LiveEffects::live(&quietly(g), &Config::path()).await {
        Ok(effects) => effects,
        Err(failure) => return failure.report(),
    };
    let socket = effects.socket().to_path_buf();
    let mut wizard = Wizard::new(effects).noting(note);
    if let Err(failure) = wizard.attach(&socket).await {
        return failure.report();
    }
    walked(g, wizard).await
}

/// The wizard from its scan on, for the callers that have already shown what the host looks
/// like: `svolder add` with no unit id, and the `svolder setup` fix `doctor` offers.
pub async fn adding(g: &Globals<'_>, given: Given) -> ExitCode {
    let g = &exempt(g);
    let effects = match live::LiveEffects::live(&quietly(g), &Config::path()).await {
        Ok(effects) => effects,
        Err(failure) => return failure.report(),
    };
    let socket = effects.socket().to_path_buf();
    let mut wizard = Wizard::starting_at(effects, Step::Scan, given);
    if let Err(failure) = wizard.attach(&socket).await {
        return failure.report();
    }
    walked(g, wizard).await
}

/// Walk `wizard` where the user can see it: the TUI wizard on a terminal, and the plain prompts
/// where the terminal cannot be claimed at all.
///
/// Both ways in come through here, so `svolder setup`, bare `svolder` with nothing configured,
/// `svolder add` with no unit id and the `svolder setup` fix `doctor` offers all reach the same
/// front end without any of them knowing which one it is.
async fn walked<E: Effects>(g: &Globals<'_>, wizard: Wizard<E>) -> ExitCode {
    walked_with(
        wizard,
        |wizard| {
            crate::tui::wizard::setup(
                g,
                wizard,
                crate::tui::TuiOptions {
                    // `setup` has no `--no-color` flag of its own; `NO_COLOR` is honoured as it
                    // is everywhere else.
                    no_color: crate::monochrome(false),
                    demo: None,
                    join: None,
                    reloaded: None,
                    checked: false,
                },
            )
        },
        cli::run,
    )
    .await
}

/// [`walked`] over the two front ends it chooses between: `tui` first, and `plain` with the
/// wizard the TUI handed back when it could not take the walk.
async fn walked_with<E: Effects, W, P>(
    wizard: Wizard<E>,
    tui: impl FnOnce(Wizard<E>) -> W,
    plain: impl FnOnce(Wizard<E>) -> P,
) -> ExitCode
where
    W: Future<Output = Result<ExitCode, Wizard<E>>>,
    P: Future<Output = ExitCode>,
{
    match tui(wizard).await {
        Ok(code) => code,
        Err(wizard) => plain(wizard).await,
    }
}

/// The same command line with `-v` dropped.
///
/// Every session the wizard opens happens with the alternate screen up, where a diagnostic
/// would land in the middle of a frame; the TUI drops `-v` from its own opens for the same
/// reason. What `-v` has to say about a run that walks the wizard is said before the screen is
/// claimed. The running TUI builds its own wizard for the `n` key, and builds it through this
/// for the very same reason.
pub(crate) fn quietly<'a>(g: &Globals<'a>) -> Globals<'a> {
    Globals {
        transport: g.transport,
        adapter: g.adapter,
        socket: g.socket,
        device: g.device,
        verbose: false,
    }
}

/// What to run after a walk that has reached its last step, for a front end that hands the
/// terminal back rather than opening what was added itself.
pub fn next_command(added: bool) -> &'static str {
    match added {
        true => "Run `svolder tui` to open it.",
        false => "Run `svolder setup` again once the device is on the air.",
    }
}

/// Whether there is someone to ask: both ends of the conversation are a terminal.
pub fn on_a_terminal() -> bool {
    std::io::stdout().is_terminal() && std::io::stdin().is_terminal()
}

/// The first step's last choice, which leaves the walk with nothing configured.
pub const LATER: &str = "Later: open svolder without a device";

/// The first step's choices, [`LATER`] last.
const HEALTH_CHOICES: [&str; 2] = ["Continue", LATER];

/// What the plain walk ends with when it is left for later.
pub const OPEN_AGAIN: &str = "Run svolder again to open the interface.";

/// What the machine the wizard runs on makes of a background service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Platform {
    /// Whether this session has a systemd to install user units into.
    pub systemd: bool,
    /// Whether this machine generates its unit files rather than keeping them.
    pub nixos: bool,
    /// Whether this process runs in a container, where the daemon is the container's own
    /// process.
    pub container: bool,
}

/// The guided first run, as far as it has got.
///
/// The steps from the scan to the write are the device walk ([`DeviceWalk`]), which this
/// embeds with effects over this machine's own radio and file; the wizard walks the host's
/// health and fixes before it, and the adapter, the daemon and what was written after it.
pub struct Wizard<E: Effects> {
    /// The device walk, which holds what this wizard may do to the machine.
    walk: DeviceWalk<Local<E>>,
    /// Which step it is on: [`Step::Scan`] stands for the whole of the walk, whose own step
    /// [`Wizard::walk`] holds.
    step: Step,
    /// The steps it came through, most recent last, for [`Wizard::back`].
    visited: Vec<Step>,
    /// What the host checks said, once they have been asked.
    outcomes: Option<Vec<Outcome>>,
    /// What applying the fixes did, in the order they were applied.
    applied: Vec<String>,
    /// Whether the daemon has been told to read the file, which happens once a run.
    reloaded: bool,
    /// How well each adapter heard the device, best first.
    grades: Vec<Grade>,
    /// The adapter that was written.
    adapter: Option<String>,
    /// What installing the daemon did.
    installed: Option<String>,
    /// The code an effect that did not happen leaves with, after which there is nothing left to
    /// ask.
    stopped: Option<u8>,
    /// Whether the last step has been answered, after which there is nothing left to ask.
    finished: bool,
    /// Whether the walk was left for later, with nothing configured.
    later: bool,
    /// What to say above the first step's own lines, such as a daemon told to read the file
    /// again.
    opening: Option<String>,
    /// The walk the daemon runs for this front end, when a daemon runs: it stands in for
    /// [`Wizard::walk`], which then asks nothing.
    remote: Option<RemoteWalk>,
    /// What the command line called the device, offered at the name step of a walk the daemon
    /// runs (the daemon's walk takes no given answers).
    given_name: Option<String>,
    /// What stopped the run, in words, beside [`Wizard::stopped`]'s code.
    stopped_why: Option<String>,
    /// The device the daemon's walk wrote, as the block this side shows (no passphrase).
    added_remotely: Option<DeviceConfig>,
    /// What `--profile` said, answered at the daemon's profile question or, for a product no
    /// question is asked about, written after the walk.
    given_profile: Option<String>,
    /// What `--passphrase` said (never the default `auto`), answered at the daemon's passphrase
    /// step or written after the walk.
    given_code: Option<String>,
    /// The signals this run leaves the daemon's walk by, held while a walk runs.
    signals: Option<signals::Signals>,
    /// What the daemon's walk could not do of the given answers, said on the last step.
    after_note: Option<String>,
    /// Whether the given profile was answered at the walk's question.
    profile_given: bool,
    /// Whether the given code was answered at the walk's passphrase step.
    code_given: bool,
}

impl<E: Effects> Wizard<E> {
    /// The wizard from its first step, with nothing answered yet.
    pub fn new(effects: E) -> Wizard<E> {
        Wizard::starting_at(effects, Step::Health, Given::default())
    }

    /// The wizard from `step`, with whatever the command line already answered. Any step of
    /// the device walk starts the walk at its scan.
    pub fn starting_at(effects: E, step: Step, given: Given) -> Wizard<E> {
        Wizard {
            given_name: given.name.clone(),
            given_profile: given.profile.clone(),
            given_code: given
                .passphrase
                .clone()
                .filter(|code| code != svolder_walk::AUTO),
            after_note: None,
            signals: None,
            profile_given: false,
            code_given: false,
            walk: DeviceWalk::new(Local(effects), given, Configured::LeaveOut),
            step: match is_walk_step(step) {
                true => Step::Scan,
                false => step,
            },
            visited: Vec::new(),
            outcomes: None,
            applied: Vec::new(),
            reloaded: false,
            grades: Vec::new(),
            adapter: None,
            installed: None,
            stopped: None,
            finished: false,
            later: false,
            opening: None,
            remote: None,
            stopped_why: None,
            added_remotely: None,
        }
    }

    /// Hand the walk to the daemon on `socket` when one runs, and keep it local when none does.
    ///
    /// A daemon is the one writer of the devices and the one that scans for them, so with one
    /// running the scan, the passphrase test and the write are its: the front end draws what the
    /// walk asks and sends the answers, and the adapter step is skipped (the daemon opened its
    /// adapter at start and a reload does not reopen it). A daemon that cannot walk is refused
    /// in words, never written around. A wizard standing at the scan starts the daemon's walk
    /// here, so a walk already running is refused before anything is drawn; one that begins at
    /// the host checks starts it on reaching the scan.
    pub async fn attach(&mut self, socket: &std::path::Path) -> Result<(), Failure> {
        let runs =
            crate::reach::daemon_runs(socket, svolder_ipc::FEATURE_WALK, "walk a device").await?;
        if !runs {
            return Ok(());
        }
        let mut remote = RemoteWalk::new(socket.to_path_buf());
        if self.walking() {
            remote.begin().await?;
        }
        // The daemon reloads itself when its walk writes, so there is nothing to tell it.
        self.reloaded = true;
        self.remote = Some(remote);
        // A real run counts the signals that end it; the unit tests install no handler.
        #[cfg(not(test))]
        {
            self.signals = Some(signals::Signals::listen());
        }
        Ok(())
    }

    /// Count `presses` as the signals that leave the walk, in place of the process's own.
    #[cfg(test)]
    pub(crate) fn leaving_by(
        &mut self,
        presses: std::sync::Arc<crate::commands::interrupts::Presses>,
    ) {
        self.signals = Some(signals::Signals::scripted(presses));
    }

    /// The presses of the signals that leave a daemon's walk, when this run holds a walk the
    /// daemon runs: a front end waits on them beside its own work and leaves on one.
    pub fn presses(&self) -> Option<std::sync::Arc<crate::commands::interrupts::Presses>> {
        self.signals.as_ref().map(signals::Signals::presses)
    }

    /// The lines of the devices a daemon's scan has heard so far, when the walk runs there.
    pub(crate) fn heard(&self) -> Option<tokio::sync::watch::Receiver<Vec<String>>> {
        self.remote.as_ref().map(RemoteWalk::heard)
    }

    /// Whether the walk runs on the daemon.
    pub fn is_remote(&self) -> bool {
        self.remote.is_some()
    }

    /// What this wizard does to the machine through.
    fn effects(&self) -> &E {
        &self.walk.effects().0
    }

    /// Whether the wizard is in the device walk.
    fn walking(&self) -> bool {
        self.step == Step::Scan
    }

    /// Say `note`, when there is one, above the first step's own lines.
    pub fn noting(mut self, note: Option<String>) -> Wizard<E> {
        self.opening = note;
        self
    }

    /// Whether the walk was left for later: the user chose to open svolder without a device.
    pub fn is_later(&self) -> bool {
        self.later
    }

    /// Whether the current step is the first one, which is the one that can be left for later.
    pub fn offers_later(&self) -> bool {
        self.step == Step::Health && !self.finished && self.stopped.is_none()
    }

    /// Leave the walk for later, which ends it with nothing asked and nothing configured.
    pub fn leave_for_later(&mut self) {
        self.later = true;
        self.finished = true;
    }

    /// What the current step asks, or nothing when the wizard has finished.
    ///
    /// What a step needs to show is gathered here rather than when the step before it was
    /// answered: the host checks on the first render of [`Step::Health`], and the adapter
    /// grades on the first render of [`Step::Adapter`]. Both take seconds, and
    /// [`Wizard::busy`] says so before this is called, so a run started at a later step never
    /// waits for answers it has no line to show.
    pub async fn prompt(&mut self) -> Option<Prompt> {
        if self.finished || self.stopped.is_some() {
            return None;
        }
        if self.walking() && self.remote.is_some() {
            return self.remote_prompt().await;
        }
        if self.step == Step::Health && self.outcomes.is_none() {
            self.outcomes = Some(self.effects().checks().await);
        }
        if self.step == Step::Adapter && self.grades.is_empty() {
            self.graded().await;
        }
        Some(match self.step {
            Step::Health => self.health(),
            Step::Fixes => self.fixes(),
            Step::Adapter => self.adapter(),
            Step::Daemon => self.daemon(),
            Step::Done => self.done(),
            _ => self.walk_prompt(),
        })
    }

    /// The prompt the daemon's walk shows. A walk that cannot be had stops the run, and says why
    /// through [`Wizard::stopped_why`].
    async fn remote_prompt(&mut self) -> Option<Prompt> {
        let shown = self.remote.as_mut()?.prompt().await;
        match shown {
            Ok(mut prompt) => {
                if prompt.step == Step::Scan {
                    prompt.body.splice(0..0, self.applied.iter().cloned());
                }
                if let (Step::Name, Some(name)) = (prompt.step, self.given_name.as_deref()) {
                    // What `--name` said is what an empty line answers, as it is on a local walk.
                    if let Some(last) = prompt.body.last_mut() {
                        *last = name.to_string();
                    }
                }
                Some(prompt)
            }
            Err(failure) => {
                self.stop(failure);
                None
            }
        }
    }

    /// Whether the command line gave an answer to `prompt`, which [`Wizard::answer_given`] sends:
    /// a front end does that under its own wait, as it does any answer, and never inside
    /// [`Wizard::prompt`], which stays free of side effects.
    pub fn has_given_answer(&self, prompt: &Prompt) -> bool {
        self.remote.is_some() && self.given_answer(prompt).is_some()
    }

    /// Send the answer the command line gave to `prompt`, once. A refusal (a code the device
    /// turned away) is said in the failure, and the step is then asked of the person.
    pub async fn answer_given(&mut self, prompt: &Prompt) -> Result<(), Failure> {
        let Some((choice, text)) = self.given_answer(prompt) else {
            return Ok(());
        };
        let code = prompt.step == Step::Passphrase;
        match code {
            true => {
                self.given_code = None;
                self.code_given = true;
            }
            false => {
                self.given_profile = None;
                self.profile_given = true;
            }
        }
        self.walked_remotely(choice, text)
            .await
            .map_err(|mut failure| {
                let what = match code {
                    true => "the passphrase given",
                    false => "the profile given",
                };
                failure.what = format!("{what} was not taken: {}", failure.what);
                failure
            })
    }

    /// The answer the command line gave to `prompt`, as a choice and a line: the code at the
    /// passphrase step, the profile (by name) at the profile question.
    fn given_answer(&self, prompt: &Prompt) -> Option<(usize, Option<String>)> {
        match prompt.step {
            Step::Passphrase => self.given_code.clone().map(|code| (0, Some(code))),
            Step::Pick if prompt.title == "Which profile" => {
                let named = self.given_profile.as_deref()?;
                let at = prompt
                    .choices
                    .iter()
                    .position(|choice| choice.eq_ignore_ascii_case(named))?;
                Some((at, None))
            }
            _ => None,
        }
    }

    /// The walk's prompt, with what applying the fixes did above its scan.
    fn walk_prompt(&self) -> Prompt {
        let mut prompt = self.walk.prompt();
        if prompt.step == Step::Scan {
            prompt.body.splice(0..0, self.applied.iter().cloned());
        }
        prompt
    }

    /// Answer the current step: the number of a choice, or a line of text where the prompt
    /// offers none.
    ///
    /// What an answer asks for happens as it is given — the fixes are applied, the device and
    /// the adapter are written, the daemon is installed — so a wizard that is left half-walked
    /// has done everything it said it had. An answer the step cannot take is refused and the
    /// step stays where it is, to be asked again.
    pub async fn answer(&mut self, choice: usize, text: Option<String>) -> Result<(), Failure> {
        match self.step {
            Step::Health if choice + 1 == HEALTH_CHOICES.len() => {
                self.leave_for_later();
                Ok(())
            }
            Step::Health => {
                let next = match self.offered().is_empty() {
                    true => Step::Scan,
                    false => Step::Fixes,
                };
                self.go(next);
                Ok(())
            }
            Step::Fixes => {
                if choice == 0 {
                    for fix in self.offered() {
                        match self.effects().apply(&fix).await {
                            Ok(done) => self.applied.push(done),
                            Err(why) => self.applied.push(why),
                        }
                    }
                }
                self.go(Step::Scan);
                Ok(())
            }
            Step::Adapter => self.chose_adapter(choice).await,
            Step::Daemon => self.settled_daemon(choice).await,
            Step::Done => {
                self.finished = true;
                Ok(())
            }
            _ => self.walked(choice, text).await,
        }
    }

    /// Answer the device walk's step, and go on from the walk once it is over: to the adapters
    /// when the machine has several to choose from, else to the daemon.
    async fn walked(&mut self, choice: usize, text: Option<String>) -> Result<(), Failure> {
        if self.remote.is_some() {
            return self.walked_remotely(choice, text).await;
        }
        match self.walk.answer(choice, text).await {
            Ok(Walked::On) => Ok(()),
            // A machine with one adapter has no choice to offer, so it skips the step that
            // would grade them and never spends the seconds grading takes.
            Ok(Walked::Added) => {
                match self.effects().adapters().len() >= 2 {
                    true => self.go(Step::Adapter),
                    false => self.on_to_the_daemon().await,
                }
                Ok(())
            }
            Ok(Walked::Skipped | Walked::Cancelled) => {
                self.on_to_the_daemon().await;
                Ok(())
            }
            Err(error) => {
                let failure = failure_of(error);
                if self.walk.stopped() {
                    self.stopped = Some(failure.code);
                }
                Err(failure)
            }
        }
    }

    /// [`Wizard::walked`] for the daemon's walk, which has the same ways to end. The adapter step
    /// is never walked: there is a daemon, and it holds its adapter.
    async fn walked_remotely(
        &mut self,
        choice: usize,
        mut text: Option<String>,
    ) -> Result<(), Failure> {
        let Some(remote) = self.remote.as_mut() else {
            return Ok(());
        };
        if remote.step() == Some(Step::Name)
            && text.as_deref().is_some_and(|line| line.trim().is_empty())
        {
            text = self.given_name.clone().or(text);
        }
        match remote.answer(choice, text).await {
            Ok(Answered::On) => Ok(()),
            Ok(Answered::Refused(refusal)) => Err(remote::refusal_failure(&refusal)),
            Ok(Answered::Added(device)) => {
                self.added_remotely = Some(device);
                self.write_what_was_given().await;
                self.on_to_the_daemon().await;
                Ok(())
            }
            Ok(Answered::Skipped | Answered::Cancelled) => {
                self.on_to_the_daemon().await;
                Ok(())
            }
            Err(failure) if remote.is_over() => Err(self.stop(failure)),
            Err(failure) => Err(failure),
        }
    }

    /// Write, as the daemon's own `update`, what the command line gave that no step of the walk
    /// asked for: the profile of a product a profile covers, the code of a device that took
    /// `auto`. A refusal is said on the last step, never dropped.
    async fn write_what_was_given(&mut self) {
        let profile = self.given_profile.take().filter(|_| !self.profile_given);
        let code = self.given_code.take().filter(|_| !self.code_given);
        let (Some(remote), Some(device)) = (self.remote.as_ref(), self.added_remotely.as_ref())
        else {
            return;
        };
        if profile.is_none() && code.is_none() {
            return;
        }
        let op = svolder_ipc::DeviceConfigOp::Update {
            device: device.name.clone(),
            name: None,
            profile: profile.map(Some),
            passphrase: code,
        };
        let said = match crate::reach::device_request(remote.socket(), op).await {
            Ok(Some(svolder_ipc::Response::DeviceConfig { .. })) => return,
            Ok(Some(svolder_ipc::Response::Error {
                message, reason, ..
            })) => crate::reach::refusal_words(reason, &message),
            Ok(_) => "the daemon did not take the profile or the passphrase".to_string(),
            Err(failure) => failure.to_string(),
        };
        self.after_note = Some(format!(
            "The profile or passphrase given was not written: {said}"
        ));
    }

    /// What the next [`Wizard::prompt`] will spend time on, when it will spend any.
    ///
    /// Grading opens each adapter in turn for several short windows, which is seconds of a
    /// screen with nothing happening on it; a front end says this before it asks for the
    /// prompt, so the wait is explained while it is being waited out.
    pub fn busy(&self) -> Option<String> {
        if self.finished || self.stopped.is_some() || self.step != Step::Adapter {
            return None;
        }
        if !self.grades.is_empty() {
            return None;
        }
        let adapters = self.effects().adapters().len();
        let seconds = adapters as u64 * GRADE_WINDOWS as u64 * GRADE_WINDOW.as_secs();
        Some(format!("grading {adapters} adapters, up to {seconds} s"))
    }

    /// The code an effect that did not happen left the run with, when one did.
    ///
    /// A wizard that is stopped has nothing left to ask: [`Wizard::prompt`] answers with nothing,
    /// and a front end leaves with this.
    pub fn stopped(&self) -> Option<u8> {
        self.stopped
    }

    /// What stopped the run, in words, when something did and the run has more to say than the
    /// failure it answered an answer with: a daemon's walk that could not be had or went away.
    pub fn stopped_why(&self) -> Option<&str> {
        self.stopped_why.as_deref()
    }

    /// Whether [`Wizard::back`] would change the step: a step inside the device walk that has
    /// one behind it goes back to that, and otherwise there must be a step behind this one that
    /// has not been applied — not on the first step of a walk, and not once the device has been
    /// written.
    pub fn can_go_back(&self) -> bool {
        if self.walking()
            && match &self.remote {
                Some(remote) => remote.can_go_back(),
                None => self.walk.can_go_back(),
            }
        {
            return true;
        }
        self.walk.added().is_none() && !self.visited.is_empty()
    }

    /// Go back to the step before this one.
    ///
    /// Inside the device walk that is the walk's own step before, then the step the walk was
    /// entered from. Once the device has been written there is nothing to go back to: the steps
    /// behind it have all been applied, and walking them again would write a second block for
    /// the same device.
    pub async fn back(&mut self) {
        if self.walking()
            && let Some(remote) = self.remote.as_mut()
        {
            if remote.can_go_back() {
                if let Err(failure) = remote.back().await {
                    self.stop(failure);
                }
                return;
            }
        } else if self.walking() && self.walk.back() {
            return;
        }
        if !self.can_go_back() {
            return;
        }
        if let Some(step) = self.visited.pop() {
            self.step = step;
            self.finished = false;
        }
    }

    /// The device this run added, once it has been written.
    pub fn added(&self) -> Option<&DeviceConfig> {
        self.walk.added().or(self.added_remotely.as_ref())
    }

    /// What the device is called when nothing is typed at the name step.
    #[cfg(test)]
    fn default_name(&self) -> String {
        self.walk.default_name()
    }

    /// Move to `step`, keeping where this came from so [`Wizard::back`] can return to it.
    fn go(&mut self, step: Step) {
        self.visited.push(self.step);
        self.step = step;
    }

    /// The fixes svolder may apply itself, out of the rows the checks gave.
    ///
    /// They are `doctor`'s own: a fix is offered here exactly where that verb would offer it,
    /// so the two never disagree about what a front end may run. The two the wizard is itself
    /// are left out — this run is the guided first run, and the daemon has a step of its own —
    /// so nothing is offered twice and nothing offers what is already happening.
    fn offered(&self) -> Vec<Fix> {
        crate::commands::doctor::fixes(self.outcomes.as_deref().unwrap_or_default())
            .into_iter()
            .filter(|fix| !a_step_of_its_own(&fix.command))
            .collect()
    }

    /// The host's health, as the lines `doctor` prints, and one way on.
    ///
    /// Every fix is under the row that asked for it, the ones that need root included: those
    /// are shown to be typed and are never run from here. The two the wizard is itself are the
    /// exception, as [`health_lines`] says.
    fn health(&self) -> Prompt {
        Prompt {
            step: Step::Health,
            title: "How this host looks".into(),
            body: self
                .opening
                .iter()
                .cloned()
                .chain(health_lines(self.outcomes.as_deref().unwrap_or_default()))
                .collect(),
            choices: HEALTH_CHOICES.iter().map(|c| (*c).to_string()).collect(),
            default: 0,
            takes: Takes::Choice,
        }
    }

    /// The fixes svolder can apply itself, as one confirmation.
    fn fixes(&self) -> Prompt {
        let fixes = self.offered();
        let title = match fixes.len() {
            1 => "Apply 1 fix".to_string(),
            many => format!("Apply {many} fixes"),
        };
        Prompt {
            step: Step::Fixes,
            title: "What svolder can put right itself".into(),
            body: fixes
                .iter()
                .map(|fix| format!("  {}", fix.command))
                .collect(),
            choices: vec![title, "Skip".into()],
            // Changing the machine is never what an answer nobody typed asks for, which is
            // what the `[y/N]` the `doctor` verb offers the very same fixes behind says too.
            default: 1,
            takes: Takes::Choice,
        }
    }

    /// How well each adapter heard the device, best first.
    ///
    /// A machine whose adapters went away between being counted and being graded has nothing to
    /// choose between, and is offered the one thing there is to do about that.
    fn adapter(&self) -> Prompt {
        let graded: Vec<String> = self
            .grades
            .iter()
            .map(|grade| match &grade.note {
                Some(note) => {
                    format!("{}  {}/{} ({note})", grade.adapter, grade.seen, grade.of)
                }
                None => format!("{}  {}/{}", grade.adapter, grade.seen, grade.of),
            })
            .collect();
        Prompt {
            step: Step::Adapter,
            title: "Which adapter to use".into(),
            body: vec!["The one picked here is written into the config as `adapter`.".into()],
            choices: match graded.is_empty() {
                true => vec!["Continue".to_string()],
                false => graded,
            },
            default: 0,
            takes: Takes::Choice,
        }
    }

    /// What to do about the daemon on this machine.
    fn daemon(&self) -> Prompt {
        let p = self.effects().platform();
        let choices = match Service::of(p.nixos, p.container, p.systemd) {
            Service::Install => vec!["Install the user service".into(), "Skip".into()],
            _ => vec!["Continue".to_string()],
        };
        Prompt {
            step: Step::Daemon,
            title: "The background service".into(),
            body: daemon_lines(self.effects()),
            choices,
            default: 0,
            takes: Takes::Choice,
        }
    }

    /// What this run wrote.
    ///
    /// What to do next is the front end's own to add: one that is about to open the device
    /// itself and one that is handing the terminal back do not send the reader to the same
    /// place. [`next_command`] is what a front end with nowhere of its own to go says.
    fn done(&self) -> Prompt {
        let mut body = Vec::new();
        match self.added() {
            Some(device) => body.push(format!("Added {} ({}).", device.name, device.udid)),
            None => body.push("No device was added.".into()),
        }
        if let Some(note) = self.after_note.as_deref() {
            body.push(note.to_string());
        }
        if let Some(adapter) = self.adapter.as_deref() {
            body.push(format!("Set adapter = {adapter:?}."));
        }
        if let Some(installed) = self.installed.as_deref() {
            // What an install did is several lines, and a body line is one line: a front end
            // that draws them into a frame has nothing to break up.
            body.push("The daemon:".into());
            body.extend(installed.lines().map(|line| format!("  {line}")));
        }
        body.push(HOME_ASSISTANT_LINE.into());
        Prompt {
            step: Step::Done,
            title: "Done".into(),
            body,
            choices: vec!["Finish".into()],
            default: 0,
            takes: Takes::Choice,
        }
    }

    /// Move on to the daemon step, telling a daemon that is running to read the file once.
    ///
    /// Everything this run writes into the file is in it by now — the device and, where there
    /// was a choice to make, the adapter — so one reload carries the lot. It is not told any
    /// earlier because a daemon told at the Name step connects the device while the adapters
    /// are being graded against it, and a device a central holds is a device the grades cannot
    /// hear.
    async fn on_to_the_daemon(&mut self) {
        self.tell_the_daemon().await;
        self.go(Step::Daemon);
    }

    /// Tell a running daemon to read the file, once a run, when this run has written a device.
    ///
    /// The daemon step does it on the way in, with everything the run writes already in the
    /// file; a walk that ends before that step (left on the Adapter step, or stopped by an adapter
    /// it could not write) has written its device all the same, and its front end calls this as
    /// it ends. A run that wrote nothing tells the daemon nothing.
    pub async fn tell_the_daemon(&mut self) {
        if self.added().is_some() && !self.reloaded {
            self.reloaded = true;
            self.effects().reload_daemon().await;
        }
    }

    /// Leave the run: a walk the daemon still runs for this front end is cancelled (this
    /// client's own, never another's), and a device written locally is the daemon's to know of.
    /// Every front end calls this as its walk ends, however it ends.
    pub async fn leave(&mut self) {
        if let Some(remote) = self.remote.as_mut() {
            remote.leave().await;
        }
        // The default action of each signal is back once the walk is over.
        self.signals = None;
        self.tell_the_daemon().await;
    }

    /// Grade the adapters against the device that was written, best first.
    ///
    /// A tie goes to the adapter the list starts with, which is the one `doctor`'s own grade
    /// row names, so the two never send the reader to different adapters.
    async fn graded(&mut self) {
        let Some(udid) = self.walk.picked().map(|entry| entry.unit.clone()) else {
            return;
        };
        self.grades = self.effects().grade(&udid).await;
        self.grades
            .sort_by_key(|grade| std::cmp::Reverse(grade.seen));
    }

    /// Write the adapter this choice names.
    ///
    /// A machine whose adapters have all gone away between being counted and being graded has
    /// nothing to choose from, and the one thing it was offered only moves on.
    async fn chose_adapter(&mut self, choice: usize) -> Result<(), Failure> {
        if !self.grades.is_empty() {
            let adapter = self
                .grades
                .get(choice)
                .ok_or_else(|| usage(format!("there is no choice {}", choice + 1)))?
                .adapter
                .clone();
            if let Err(why) = self.effects().write_adapter(&adapter) {
                return Err(self.stop(Failure::of(Kind::ConfigUnreadable, why)));
            }
            self.adapter = Some(adapter);
        }
        self.on_to_the_daemon().await;
        Ok(())
    }

    /// Install the user units where this machine keeps them and the answer asked for it.
    async fn settled_daemon(&mut self, choice: usize) -> Result<(), Failure> {
        let p = self.effects().platform();
        if choice == 0 && Service::of(p.nixos, p.container, p.systemd) == Service::Install {
            match self.effects().install_daemon().await {
                Ok(done) => self.installed = Some(done),
                Err(why) => return Err(self.stop(Failure::of(Kind::DaemonError, why))),
            }
        }
        self.go(Step::Done);
        Ok(())
    }

    /// Keep the code an effect that did not happen leaves with, and hand the failure back.
    ///
    /// A refused answer is asked again — another name, another code — but an effect the wizard
    /// asked the machine for and did not get is not something answering again would change, so
    /// the run ends on it rather than asking the same step of the same unwritable file forever.
    fn stop(&mut self, failure: Failure) -> Failure {
        self.stopped = Some(failure.code);
        self.stopped_why = Some(failure.to_string());
        failure
    }
}

/// Whether `step` is one the device walk walks.
fn is_walk_step(step: Step) -> bool {
    matches!(
        step,
        Step::Linked | Step::Scan | Step::Pick | Step::Name | Step::Passphrase
    )
}

/// The failure a refusal of the device walk is on this front end, with the exit code its kind
/// has; a refused code is named back to the person who typed it.
fn failure_of(error: WalkError<Failure>) -> Failure {
    match error {
        WalkError::Effect(failure) => failure,
        WalkError::Refused { code } => {
            let mut refused = Failure::of(
                Kind::Refused,
                format!("the device did not accept {}", code.expose()),
            );
            // `auto` was the step's own way out of a lost code, so what follows its refusal is
            // what comes after it.
            if code.expose() == svolder_walk::AUTO {
                refused.next = crate::recovery::REFUSED_UNDER_AUTO.to_string();
            }
            refused
        }
        WalkError::NoDevice => Failure::of(Kind::NothingInRange, "no device has been picked"),
        other => usage(other.words(|failure| failure.to_string())),
    }
}

/// A failure the user can fix by answering differently: exit 1.
fn usage(message: impl Into<String>) -> Failure {
    Failure::of(Kind::Usage, message)
}

/// The health rows as the lines that show them, as `doctor` prints them.
///
/// The fix lines for the two steps the wizard is itself are left out: a run of the wizard is the
/// `svolder setup` fix, and the daemon has a step of its own three steps down, so printing
/// either would be telling the reader to run what is running. Every other fix stands under its
/// row, the ones that need root included, which are shown to be typed and are never run.
pub fn health_lines(outcomes: &[Outcome]) -> Vec<String> {
    outcomes
        .iter()
        .flat_map(|outcome| {
            let mut lines = vec![crate::commands::doctor::line(outcome)];
            if !outcome
                .fix
                .as_ref()
                .is_some_and(|fix| a_step_of_its_own(&fix.command))
            {
                lines.extend(crate::commands::doctor::fix_line(outcome));
            }
            lines
        })
        .collect()
}

/// Whether this fix is one the wizard walks as a step of its own rather than applying as a fix.
fn a_step_of_its_own(command: &str) -> bool {
    command == checks::SETUP || command == checks::DAEMON_INSTALL
}

/// What to do about the daemon on this machine, as the lines that say it.
///
/// The guided run shows these under its question and the non-interactive summary prints them
/// on their own, so a host is told the same thing either way.
pub fn daemon_lines<H: Host>(effects: &H) -> Vec<String> {
    let p = effects.platform();
    match Service::of(p.nixos, p.container, p.systemd) {
        Service::Install => vec![
            "The daemon holds each device's link, so a command need not connect first.".to_string(),
            "  svolder daemon install".to_string(),
        ],
        Service::Snippet => {
            let mut lines = vec![
                "NixOS generates its unit files; put this block in your home-manager \
                 configuration:"
                    .to_string(),
            ];
            lines.extend(effects.nixos_snippet());
            lines
        }
        Service::Container => vec![format!("In a container {CONTAINER_DAEMON}.")],
        Service::None => vec![
            "This session has no systemd to install user units into; svolder starts a daemon of \
             its own the first time a command needs one."
                .to_string(),
            "  [daemon] autospawn = false in config.toml turns that off.".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests;
