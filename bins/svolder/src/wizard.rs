// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The guided first run: one state machine, and the front ends that drive it.
//!
//! [`Wizard`] walks nine steps — the host's health, the fixes svolder can
//! apply itself, a scan, the device, its name, its passphrase, the adapter that hears it best,
//! the daemon, and what was written. It renders each step as a [`Prompt`] and applies what an
//! answer asks for through [`Effects`], which is the whole of what it can do to the machine.
//!
//! Nothing here prints: a front end may be drawing on the alternate screen, so the core hands
//! out [`Prompt`]s and only [`cli`] and [`summary()`] put words on a terminal.
//!
//! - [`cli`]: the wizard as plain prompts on a terminal.
//! - `effects`: what the wizard can do to the machine it runs on, in three seams.
//! - `fake`: a machine the wizard can be walked over without touching this one.
//! - [`live`]: the wizard against the machine it is running on.
//! - `summary`: the wizard with nobody to ask, printed once.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::process::ExitCode;

use svolder_ipc::checks::{self, Fix, Outcome};
use svolder_ipc::{Config, DeviceConfig, ScanEntry};

use crate::backend::Globals;
use crate::commands::daemon::install::{CONTAINER_DAEMON, Service};
use crate::recovery::{Failure, Kind};
use crate::scanning::{GRADE_WINDOW, GRADE_WINDOWS, Grade};

pub mod cli;
mod effects;
#[cfg(test)]
pub(crate) mod fake;
pub mod live;
mod summary;

pub use effects::{Effects, Files, Host, Radio};
pub use summary::summary;

/// What `svolder setup` does, and what bare `svolder` does with nothing configured.
///
/// On a terminal it walks the wizard; off one it prints the summary, because a run whose output
/// is being read has no one to ask. Both need stdin as well as stdout: a run whose answers
/// would come from a file is one that would answer every question with the end of that file.
pub async fn setup(g: &Globals<'_>, note: Option<String>) -> ExitCode {
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
    walked(g, Wizard::new(effects).noting(note)).await
}

/// The wizard from its scan on, for the callers that have already shown what the host looks
/// like: `svolder add` with no unit id, and the `svolder setup` fix `doctor` offers.
pub async fn adding(g: &Globals<'_>, given: Given) -> ExitCode {
    let effects = match live::LiveEffects::live(&quietly(g), &Config::path()).await {
        Ok(effects) => effects,
        Err(failure) => return failure.report(),
    };
    walked(g, Wizard::starting_at(effects, Step::Scan, given)).await
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

/// What a scan that heard nothing says.
const NOTHING_HEARD: &str = "Nothing answered: power the device on and close the phone app, \
                             which holds its only connection.";

/// What a device whose product no profile covers says on its line, with what picking it leads
/// to.
const UNKNOWN_PRODUCT: &str = "unknown product — choose a profile";

/// The first step's last choice, which leaves the walk with nothing configured.
pub const LATER: &str = "Later: open svolder without a device";

/// The first step's choices, [`LATER`] last.
const HEALTH_CHOICES: [&str; 2] = ["Continue", LATER];

/// What the plain walk ends with when it is left for later.
pub const OPEN_AGAIN: &str = "Run svolder again to open the interface.";

/// The steps the wizard walks, in the order it walks them.
///
/// A step that has nothing to ask is not walked: a healthy host skips [`Step::Fixes`], a
/// device that connects on the advertised passphrase skips [`Step::Passphrase`], and a machine
/// with one adapter skips [`Step::Adapter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// What the host checks say about this machine.
    Health,
    /// The fixes svolder may apply itself, as one confirmation.
    Fixes,
    /// Listening for devices in range.
    Scan,
    /// Which of the devices heard to configure.
    Pick,
    /// What to call it.
    Name,
    /// The four digits the device wants, when it turned the first connection away.
    Passphrase,
    /// Which adapter hears it best.
    Adapter,
    /// Whether to install the user service.
    Daemon,
    /// What was written.
    Done,
}

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

/// How a device answered a connection attempt.
///
/// A refusal is not a failure: it is the device asking for its passphrase, which is a question
/// the wizard has a step for. Everything else that can stop a connection is a [`Failure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connect {
    /// The device let the session in.
    Accepted,
    /// The device turned the passphrase away.
    Refused,
}

/// What the command line already answered, so the wizard asks for it with that in place.
///
/// `svolder add --name NAME` with no unit id is someone who has already said what to call the
/// device; the wizard offers it as the default rather than asking as though nothing was said.
#[derive(Debug, Clone, Default)]
pub struct Given {
    /// What to call the device, as `--name` said.
    pub name: Option<String>,
    /// The profile to use, as `--profile` said.
    pub profile: Option<String>,
    /// The passphrase to connect with, as `--passphrase` said.
    pub passphrase: Option<String>,
}

/// One step as something to show and something to answer.
///
/// A prompt with choices takes the number of one of them; a prompt with none takes a line of
/// text, and its last body line is the default an empty line stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Which step this is.
    pub step: Step,
    /// The one line that says what is being asked.
    pub title: String,
    /// What the reader needs to answer it.
    pub body: Vec<String>,
    /// What can be chosen, in the order they are offered.
    pub choices: Vec<String>,
    /// Which choice an empty answer takes.
    pub default: usize,
}

/// The guided first run, as far as it has got.
pub struct Wizard<E: Effects> {
    /// What this wizard may do to the machine.
    effects: E,
    /// Which step it is on.
    step: Step,
    /// The steps it came through, most recent last, for [`Wizard::back`].
    visited: Vec<Step>,
    /// What the command line already answered.
    given: Given,
    /// What the host checks said, once they have been asked.
    outcomes: Option<Vec<Outcome>>,
    /// What applying the fixes did, in the order they were applied.
    applied: Vec<String>,
    /// What the last scan heard.
    seen: Vec<ScanEntry>,
    /// What opening a backend for that scan had to say.
    notices: Vec<String>,
    /// Whether a scan has run, so that hearing nothing is an answer rather than a start.
    scanned: bool,
    /// How many scans in a row have failed outright, which is what a run gives up after.
    scan_failures: u8,
    /// Whether the daemon has been told to read the file, which happens once a run.
    reloaded: bool,
    /// Which of [`Wizard::seen`] is being configured.
    picked: Option<usize>,
    /// Whether the pick step is on its second question, the profile for a product no profile
    /// covers.
    choosing_profile: bool,
    /// The profile to write, when one was chosen or given.
    profile: Option<String>,
    /// What to call the device.
    name: Option<String>,
    /// The passphrase to connect with and to write: `auto`, `none`, or four digits.
    passphrase: String,
    /// The device that was written.
    added: Option<DeviceConfig>,
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
}

impl<E: Effects> Wizard<E> {
    /// The wizard from its first step, with nothing answered yet.
    pub fn new(effects: E) -> Wizard<E> {
        Wizard::starting_at(effects, Step::Health, Given::default())
    }

    /// The wizard from `step`, with whatever the command line already answered.
    pub fn starting_at(effects: E, step: Step, given: Given) -> Wizard<E> {
        Wizard {
            effects,
            step,
            visited: Vec::new(),
            outcomes: None,
            applied: Vec::new(),
            seen: Vec::new(),
            notices: Vec::new(),
            scanned: false,
            scan_failures: 0,
            reloaded: false,
            picked: None,
            choosing_profile: false,
            profile: given.profile.clone(),
            name: None,
            passphrase: given.passphrase.clone().unwrap_or_else(|| AUTO.to_string()),
            given,
            added: None,
            grades: Vec::new(),
            adapter: None,
            installed: None,
            stopped: None,
            finished: false,
            later: false,
            opening: None,
        }
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
        if self.step == Step::Health && self.outcomes.is_none() {
            self.outcomes = Some(self.effects.checks().await);
        }
        if self.step == Step::Adapter && self.grades.is_empty() {
            self.graded().await;
        }
        Some(match self.step {
            Step::Health => self.health(),
            Step::Fixes => self.fixes(),
            Step::Scan => self.scan(),
            Step::Pick if self.choosing_profile => self.profiles(),
            Step::Pick => self.pick(),
            Step::Name => self.name(),
            Step::Passphrase => self.passphrase(),
            Step::Adapter => self.adapter(),
            Step::Daemon => self.daemon(),
            Step::Done => self.done(),
        })
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
                        match self.effects.apply(&fix).await {
                            Ok(done) => self.applied.push(done),
                            Err(why) => self.applied.push(why),
                        }
                    }
                }
                self.go(Step::Scan);
                Ok(())
            }
            Step::Scan => self.searched(choice).await,
            Step::Pick if self.choosing_profile => self.chose_profile(choice),
            Step::Pick => self.chose_device(choice),
            Step::Name => self.named(text).await,
            Step::Passphrase => self.coded(text).await,
            Step::Adapter => self.chose_adapter(choice).await,
            Step::Daemon => self.settled_daemon(choice).await,
            Step::Done => {
                self.finished = true;
                Ok(())
            }
        }
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
        let adapters = self.effects.adapters().len();
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

    /// Whether [`Wizard::back`] would change the step: the second question of [`Step::Pick`]
    /// goes back to its first, and otherwise there must be a step behind this one that has
    /// not been applied — not on the first step of a walk, and not once the device has been
    /// written.
    pub fn can_go_back(&self) -> bool {
        self.choosing_profile || (self.added.is_none() && !self.visited.is_empty())
    }

    /// Go back to the step before this one.
    ///
    /// The second question of [`Step::Pick`] goes back to its first. Once the device has been
    /// written there is nothing to go back to: the steps behind it have all been applied, and
    /// walking them again would write a second block for the same device.
    pub fn back(&mut self) {
        if self.choosing_profile {
            self.choosing_profile = false;
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
        self.added.as_ref()
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
        }
    }

    /// The scan, and after a quiet one the reason a device usually stays quiet.
    fn scan(&self) -> Prompt {
        let mut body = self.applied.clone();
        body.extend(self.notices.iter().cloned());
        let mut choices = vec!["Scan".to_string()];
        if self.scanned && self.seen.is_empty() {
            body.push(NOTHING_HEARD.into());
            choices = vec!["Scan again".into(), "Skip".into()];
        } else if !self.scanned {
            body.push(format!(
                "Listening takes up to {} s.",
                crate::scanning::WINDOWS as u64 * crate::scanning::WINDOW.as_secs()
            ));
        }
        Prompt {
            step: Step::Scan,
            title: "Find the device".into(),
            body,
            choices,
            default: 0,
        }
    }

    /// One line per device heard: what it is, its unit id, how loud it was, and the profile
    /// its advertisement matches.
    fn pick(&self) -> Prompt {
        Prompt {
            step: Step::Pick,
            title: "Which device".into(),
            body: self.notices.clone(),
            choices: self.seen.iter().map(|entry| self.row(entry)).collect(),
            default: 0,
        }
    }

    /// One device as the line that offers it.
    fn row(&self, entry: &ScanEntry) -> String {
        let product = self.product(entry).unwrap_or("unknown");
        let signal = entry
            .rssi
            .map(|rssi| format!("  {rssi} dBm"))
            .unwrap_or_default();
        let profile = self
            .matched(entry)
            .map(|profile| profile.name.clone())
            .or_else(|| self.profile.clone())
            .unwrap_or_else(|| UNKNOWN_PRODUCT.to_string());
        format!("{product}  {}{signal}  {profile}", entry.unit)
    }

    /// The profiles this build knows, for a device whose product none of them covers.
    fn profiles(&self) -> Prompt {
        let names = self.profile_names();
        let body = match self.picked_entry() {
            Some(entry) => vec![format!(
                "{} advertises product {}, which no profile covers.",
                entry.unit, entry.product_id
            )],
            None => Vec::new(),
        };
        let choices = match names.is_empty() {
            true => vec!["Continue".to_string()],
            false => names,
        };
        Prompt {
            step: Step::Pick,
            title: "Which profile".into(),
            body,
            choices,
            default: 0,
        }
    }

    /// What to call the device, with the name it advertises as the default.
    fn name(&self) -> Prompt {
        Prompt {
            step: Step::Name,
            title: "What to call it".into(),
            body: vec![
                "Enter takes the name below; type another to use that instead.".into(),
                self.default_name(),
            ],
            choices: Vec::new(),
            default: 0,
        }
    }

    /// The four digits the device wants, `auto` to read the code from the device, or `none`
    /// for a device with no passphrase.
    ///
    /// The last line is what an empty one answers with, as it is on every step that takes a
    /// typed line: a device with no passphrase is configured with [`NONE`].
    fn passphrase(&self) -> Prompt {
        Prompt {
            step: Step::Passphrase,
            title: "The device wants its passphrase".into(),
            body: vec![
                "It turned the connection away, which is what a device with a passphrase does."
                    .into(),
                "The four digits the phone app shows, `auto` to have svolder read the code from \
                 the device, or `none` for a device with no passphrase."
                    .into(),
                NONE.into(),
            ],
            choices: Vec::new(),
            default: 0,
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
        }
    }

    /// What to do about the daemon on this machine.
    fn daemon(&self) -> Prompt {
        let p = self.effects.platform();
        let choices = match Service::of(p.nixos, p.container, p.systemd) {
            Service::Install => vec!["Install the user service".into(), "Skip".into()],
            _ => vec!["Continue".to_string()],
        };
        Prompt {
            step: Step::Daemon,
            title: "The background service".into(),
            body: daemon_lines(&self.effects),
            choices,
            default: 0,
        }
    }

    /// What this run wrote.
    ///
    /// What to do next is the front end's own to add: one that is about to open the device
    /// itself and one that is handing the terminal back do not send the reader to the same
    /// place. [`next_command`] is what a front end with nowhere of its own to go says.
    fn done(&self) -> Prompt {
        let mut body = Vec::new();
        match self.added.as_ref() {
            Some(device) => body.push(format!("Added {} ({}).", device.name, device.udid)),
            None => body.push("No device was added.".into()),
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
        Prompt {
            step: Step::Done,
            title: "Done".into(),
            body,
            choices: vec!["Finish".into()],
            default: 0,
        }
    }

    /// Listen again, or give up on finding a device and go on to the daemon.
    ///
    /// A scan that cannot run at all — no adapter, a bus that will not answer — is not a quiet
    /// room, and answering the same step again asks the same adapter the same thing: after
    /// [`SCAN_ATTEMPTS`] of them in a row the run ends on the last reason given.
    async fn searched(&mut self, choice: usize) -> Result<(), Failure> {
        if self.scanned && self.seen.is_empty() && choice == 1 {
            self.on_to_the_daemon().await;
            return Ok(());
        }
        let heard = match self.effects.scan().await {
            Ok(heard) => heard,
            Err(failure) => {
                self.scan_failures += 1;
                return Err(match self.scan_failures >= SCAN_ATTEMPTS {
                    true => self.stop(failure),
                    false => failure,
                });
            }
        };
        self.scan_failures = 0;
        self.seen = heard.entries;
        self.notices = heard.notices;
        self.scanned = true;
        if !self.seen.is_empty() {
            self.go(Step::Pick);
        }
        Ok(())
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
        if self.added.is_some() && !self.reloaded {
            self.reloaded = true;
            self.effects.reload_daemon().await;
        }
    }

    /// Take the device this choice names; a product no profile covers asks which profile to
    /// read it through before going on.
    fn chose_device(&mut self, choice: usize) -> Result<(), Failure> {
        let Some(entry) = self.seen.get(choice) else {
            return Err(out_of_range(choice));
        };
        let unmatched = self.matched(entry).is_none();
        self.picked = Some(choice);
        if unmatched && self.profile.is_none() {
            self.choosing_profile = true;
            return Ok(());
        }
        self.go(Step::Name);
        Ok(())
    }

    /// Read the device through the profile this choice names, or through none when the build
    /// has no profiles to offer.
    fn chose_profile(&mut self, choice: usize) -> Result<(), Failure> {
        let names = self.profile_names();
        if !names.is_empty() {
            let name = names.get(choice).ok_or_else(|| out_of_range(choice))?;
            self.profile = Some(name.clone());
        }
        self.choosing_profile = false;
        self.go(Step::Name);
        Ok(())
    }

    /// Name the device and try to reach it; a device that turns the connection away is asked
    /// for its passphrase instead.
    async fn named(&mut self, text: Option<String>) -> Result<(), Failure> {
        let typed = text.unwrap_or_default();
        let name = match typed.trim() {
            "" => self.default_name(),
            name => name.to_string(),
        };
        if name.is_empty() {
            return Err(Failure::of(Kind::Usage, "a device needs a name"));
        }
        if self
            .effects
            .taken_names()
            .iter()
            .any(|taken| taken.eq_ignore_ascii_case(&name))
        {
            return Err(Failure::of(
                Kind::Usage,
                format!("a device named {name:?} is already configured"),
            ));
        }
        self.name = Some(name);
        let entry = self.picked_entry().cloned().ok_or_else(no_device)?;
        let passphrase = self.passphrase.clone();
        match self.effects.try_connect(&entry, &passphrase).await? {
            Connect::Accepted => self.wrote().await,
            Connect::Refused => {
                self.go(Step::Passphrase);
                Ok(())
            }
        }
    }

    /// Try the code that was typed; a device that turns this one away too is said to have,
    /// and the step is asked again.
    async fn coded(&mut self, text: Option<String>) -> Result<(), Failure> {
        let code = passphrase_text(text.as_deref().unwrap_or_default())?;
        let entry = self.picked_entry().cloned().ok_or_else(no_device)?;
        match self.effects.try_connect(&entry, &code).await? {
            Connect::Accepted => {
                self.passphrase = code;
                self.wrote().await
            }
            Connect::Refused => {
                let mut refused =
                    Failure::of(Kind::Refused, format!("the device did not accept {code}"));
                // `auto` was the step's own way out of a lost code, so what follows its refusal
                // is what comes after it.
                if code == AUTO {
                    refused.next = crate::recovery::REFUSED_UNDER_AUTO.to_string();
                }
                Err(refused)
            }
        }
    }

    /// Write the device and go on: a machine with one adapter has no choice to offer, so it
    /// skips the step that would grade them and never spends the seconds grading takes.
    async fn wrote(&mut self) -> Result<(), Failure> {
        let entry = self.picked_entry().cloned().ok_or_else(no_device)?;
        let device = DeviceConfig {
            name: self.name.clone().unwrap_or_default(),
            udid: entry.unit.clone(),
            profile: self.profile.clone(),
            passphrase: self.passphrase.clone(),
            inputs: BTreeMap::new(),
        };
        if let Err(why) = self.effects.write_device(&device).await {
            return Err(self.stop(Failure::of(Kind::ConfigUnreadable, why)));
        }
        self.added = Some(device);
        match self.effects.adapters().len() >= 2 {
            true => self.go(Step::Adapter),
            false => self.on_to_the_daemon().await,
        }
        Ok(())
    }

    /// Grade the adapters against the device that was written, best first.
    ///
    /// A tie goes to the adapter the list starts with, which is the one `doctor`'s own grade
    /// row names, so the two never send the reader to different adapters.
    async fn graded(&mut self) {
        let Some(udid) = self.picked_entry().map(|entry| entry.unit.clone()) else {
            return;
        };
        self.grades = self.effects.grade(&udid).await;
        self.grades.sort_by(|a, b| b.seen.cmp(&a.seen));
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
                .ok_or_else(|| out_of_range(choice))?
                .adapter
                .clone();
            if let Err(why) = self.effects.write_adapter(&adapter) {
                return Err(self.stop(Failure::of(Kind::ConfigUnreadable, why)));
            }
            self.adapter = Some(adapter);
        }
        self.on_to_the_daemon().await;
        Ok(())
    }

    /// Install the user units where this machine keeps them and the answer asked for it.
    async fn settled_daemon(&mut self, choice: usize) -> Result<(), Failure> {
        let p = self.effects.platform();
        if choice == 0 && Service::of(p.nixos, p.container, p.systemd) == Service::Install {
            match self.effects.install_daemon().await {
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
        failure
    }

    /// The device being configured, as the scan reported it.
    fn picked_entry(&self) -> Option<&ScanEntry> {
        self.picked.and_then(|at| self.seen.get(at))
    }

    /// What the device behind a scan row is called: the name the scan gave it, else the name
    /// [`svolder_profiles::Registry::product_name`] gives its product id — which is how a product
    /// the codec's table does not know is named after the profile made for it.
    fn product<'e>(&'e self, entry: &'e ScanEntry) -> Option<&'e str> {
        entry
            .product
            .as_deref()
            .or_else(|| self.effects.registry()?.product_name(entry.product_id))
    }

    /// The profile this build reads a device advertising this product through.
    fn matched(&self, entry: &ScanEntry) -> Option<&svolder_profiles::Profile> {
        self.effects
            .registry()
            .and_then(|registry| registry.select(entry.product_id, entry.firmware))
    }

    /// Every profile this build knows, in the order the registry lists them.
    fn profile_names(&self) -> Vec<String> {
        self.effects
            .registry()
            .map(|registry| registry.names().iter().map(|n| (*n).to_string()).collect())
            .unwrap_or_default()
    }

    /// The profile that was chosen or given for a product the registry does not match.
    fn chosen(&self) -> Option<&svolder_profiles::Profile> {
        self.effects.registry()?.get(self.profile.as_deref()?)
    }

    /// What the device is called when nothing is typed: the name the command line gave, else
    /// the product it advertises, else the product the profile it is read through describes,
    /// else its unit id.
    fn default_name(&self) -> String {
        if let Some(name) = self.given.name.as_deref() {
            return name.to_string();
        }
        let Some(entry) = self.picked_entry() else {
            return String::new();
        };
        if let Some(product) = self.product(entry) {
            return product.to_string();
        }
        self.matched(entry)
            .or_else(|| self.chosen())
            .map(|profile| profile.display.clone())
            .unwrap_or_else(|| entry.unit.clone())
    }
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

/// How many scans that could not run at all a guided run makes before it gives up.
const SCAN_ATTEMPTS: u8 = 3;

/// The passphrase key a typed answer means: four digits are themselves, `auto` is the code the
/// advertisement carries, and an empty line — or the word the step offers as what an empty line
/// stands for — is a device with no passphrase.
fn passphrase_text(typed: &str) -> Result<String, Failure> {
    let typed = typed.trim();
    if typed.is_empty() || typed.eq_ignore_ascii_case(NONE) {
        return Ok(NONE.to_string());
    }
    if typed.eq_ignore_ascii_case(AUTO) {
        return Ok(AUTO.to_string());
    }
    if typed.len() == 4 && typed.chars().all(|c| c.is_ascii_digit()) {
        return Ok(typed.to_string());
    }
    Err(Failure::of(
        Kind::Usage,
        "a code is four digits, or `auto`, or `none`",
    ))
}

/// What the passphrase is before a device has said otherwise: whatever the advertisement
/// carries.
const AUTO: &str = "auto";

/// What a device with no passphrase is configured with.
const NONE: &str = "none";

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

/// A choice that is not one of the ones offered.
fn out_of_range(choice: usize) -> Failure {
    Failure::of(Kind::Usage, format!("there is no choice {}", choice + 1))
}

/// A step that needs a device reached one without: nothing was picked to configure.
fn no_device() -> Failure {
    Failure::of(Kind::NothingInRange, "no device has been picked")
}

#[cfg(test)]
mod tests;
