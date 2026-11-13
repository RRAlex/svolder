// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device walk: listen for devices, pick one (and a profile for a product no profile
//! selects), name it, test its passphrase, and write it into the configuration.
//!
//! [`DeviceWalk`] is one state machine that any front end embeds: `svolder setup`, `svolder
//! add` and the TUI's `n` run it with effects over their own radio and file, and the daemon
//! runs it as a job, with effects over its scanner, a probe actor and its config writer. It
//! renders each step as a [`Prompt`], takes an answer to it, and does what the answer asks for
//! through [`WalkEffects`], which is the whole of what it can do to the machine. Nothing here
//! prints, opens a radio or touches a file.
//!
//! - `prompt`: the [`Prompt`] a step is shown as, and the [`Step`]s of a guided run.
//! - `effects`: what the walk asks of the machine ([`WalkEffects`]).
//! - `error`: why an answer did not move the walk on ([`WalkError`]), and the [`Secret`] a
//!   passphrase is held as.
//! - `wire`: a walk the daemon runs, as its job's progress says it ([`WalkState`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use svolder_ipc::{Connect, ConnectFailure, DeviceConfig, ScanEntry};
use svolder_proto::UnitId;

mod effects;
mod error;
mod prompt;
mod wire;

pub use effects::{Heard, WalkEffects, WriteError};
pub use error::{Secret, WalkError, connect_words};
pub use prompt::{Prompt, Step, Takes};
pub use wire::{Phase, ROWS_MAX, RefusalKind, WalkRefusal, WalkState, Working};

/// What a scan that heard nothing says.
pub const NOTHING_HEARD: &str = "Nothing answered: power the device on and close the phone app, \
                                 which holds its only connection.";

/// What a device whose product no profile covers says on its line, with what picking it leads
/// to.
pub const UNKNOWN_PRODUCT: &str = "unknown product — choose a profile";

/// The passphrase before a device has said otherwise: whatever the advertisement carries.
pub const AUTO: &str = "auto";

/// What a device with no passphrase is configured with.
pub const NONE: &str = "none";

/// How many scans that could not run at all a walk makes in a row before it stops.
pub const SCAN_ATTEMPTS: u8 = 3;

/// The linked step's choices: go on first.
const LINKED_CHOICES: [&str; 2] = ["Go on", "Cancel"];

/// What the command line already answered, so the walk asks for it with that in place.
///
/// `svolder add --name NAME` with no unit id is someone who has already said what to call the
/// device; the walk offers it as the default rather than asking as though nothing was said.
#[derive(Clone, Default)]
pub struct Given {
    /// What to call the device, as `--name` said.
    pub name: Option<String>,
    /// The profile to use, as `--profile` said.
    pub profile: Option<String>,
    /// The passphrase to connect with, as `--passphrase` said.
    pub passphrase: Option<String>,
}

impl std::fmt::Debug for Given {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Given")
            .field("name", &self.name)
            .field("profile", &self.profile)
            .field("passphrase", &self.passphrase.as_ref().map(|_| "<hidden>"))
            .finish()
    }
}

/// What the pick step does with a unit that is configured already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Configured {
    /// Leave it out of the choices, with a line naming what was left out.
    LeaveOut,
    /// Offer it marked with its configured name; picking it is refused, naming that device.
    Mark,
}

/// One device heard, as the walk offers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// what the scan heard
    #[serde(flatten)]
    pub entry: ScanEntry,
    /// the configured device's name, when the unit is configured already
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured: Option<String>,
}

/// Where an answer left the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walked {
    /// On to another step, or the same one asked again; [`DeviceWalk::prompt`] says which.
    On,
    /// The device is written: the walk is over, and [`DeviceWalk::added`] says what it wrote.
    Added,
    /// A quiet scan was given up on: the walk is over with nothing written.
    Skipped,
    /// The walk was cancelled at its linked step: it is over with nothing done.
    Cancelled,
}

/// The device walk, as far as it has got.
pub struct DeviceWalk<E: WalkEffects> {
    /// What this walk may do to the machine.
    effects: E,
    /// Which step it is on.
    step: Step,
    /// The steps it came through, most recent last, for [`DeviceWalk::back`].
    visited: Vec<Step>,
    /// What the command line already answered.
    given: Given,
    /// What the pick step does with a configured unit.
    configured: Configured,
    /// The configured devices that held a link when the walk started.
    linked: Vec<String>,
    /// What the last scan heard, as the pick step offers it.
    rows: Vec<Row>,
    /// What opening the radio for that scan had to say.
    notices: Vec<String>,
    /// Whether a scan has run, so that hearing nothing is an answer rather than a start.
    scanned: bool,
    /// How many scans in a row have failed outright, which is what a walk stops after.
    scan_failures: u8,
    /// Which of [`DeviceWalk::rows`] is being configured.
    picked: Option<usize>,
    /// Whether the pick step is on its second question, the profile for a product no profile
    /// covers.
    choosing_profile: bool,
    /// The profile to write, when one was chosen or given.
    profile: Option<String>,
    /// What to call the device.
    name: Option<String>,
    /// The passphrase to connect with and to write: `auto`, `none`, or four digits.
    passphrase: Secret,
    /// How the last passphrase test went.
    connect: Option<Connect>,
    /// The device that was written.
    added: Option<DeviceConfig>,
    /// Whether an effect that did not happen stopped the walk, after which nothing is asked.
    stopped: bool,
}

impl<E: WalkEffects> DeviceWalk<E> {
    /// A walk with nothing answered yet but what `given` holds: at its linked step when a
    /// configured device holds a link, at its scan otherwise.
    pub fn new(effects: E, given: Given, configured: Configured) -> DeviceWalk<E> {
        let linked = effects.linked();
        let step = match linked.is_empty() {
            true => Step::Scan,
            false => Step::Linked,
        };
        DeviceWalk {
            step,
            visited: Vec::new(),
            configured,
            linked,
            rows: Vec::new(),
            notices: Vec::new(),
            scanned: false,
            scan_failures: 0,
            picked: None,
            choosing_profile: false,
            profile: given.profile.clone(),
            name: None,
            passphrase: Secret::new(given.passphrase.clone().unwrap_or_else(|| AUTO.into())),
            connect: None,
            given,
            added: None,
            stopped: false,
            effects,
        }
    }

    /// What this walk does to the machine through.
    pub fn effects(&self) -> &E {
        &self.effects
    }

    /// The step the walk is on.
    pub fn step(&self) -> Step {
        self.step
    }

    /// The devices the last scan heard, as the pick step offers them.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// How the last passphrase test went, once one has run.
    pub fn connect(&self) -> Option<Connect> {
        self.connect
    }

    /// The device this walk wrote, once it is written.
    pub fn added(&self) -> Option<&DeviceConfig> {
        self.added.as_ref()
    }

    /// The unit picked to configure, once one is.
    pub fn picked(&self) -> Option<&ScanEntry> {
        self.picked
            .and_then(|at| self.rows.get(at))
            .map(|row| &row.entry)
    }

    /// Whether an effect that did not happen stopped the walk: a scan that could never run, or
    /// a file that cannot take the write. A stopped walk asks nothing more.
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// What the current step asks.
    pub fn prompt(&self) -> Prompt {
        match self.step {
            Step::Linked => self.linked_prompt(),
            Step::Pick if self.choosing_profile => self.profiles(),
            Step::Pick => self.pick(),
            Step::Name => self.name_prompt(),
            Step::Passphrase => self.passphrase_prompt(),
            _ => self.scan_prompt(),
        }
    }

    /// Answer the current step: the number of a choice, or a line of text where the prompt
    /// takes text.
    ///
    /// What an answer asks for happens as it is given: the scan runs, the passphrase is tested,
    /// the device is written. An answer the step cannot take is refused and the step stays where
    /// it is, to be asked again, unless the walk stopped on it ([`DeviceWalk::stopped`]).
    pub async fn answer(
        &mut self,
        choice: usize,
        text: Option<String>,
    ) -> Result<Walked, WalkError<E::Failure>> {
        match self.step {
            Step::Linked if choice == 1 => Ok(Walked::Cancelled),
            Step::Linked if choice == 0 => {
                self.go(Step::Scan);
                Ok(Walked::On)
            }
            Step::Linked => Err(out_of_range(choice)),
            Step::Pick if self.choosing_profile => self.chose_profile(choice),
            Step::Pick => self.chose_device(choice),
            Step::Name => self.named(text).await,
            Step::Passphrase => self.coded(text).await,
            _ => self.searched(choice).await,
        }
    }

    /// Whether [`DeviceWalk::back`] would change the step: the second question of
    /// [`Step::Pick`] goes back to its first, and otherwise there must be a step behind this
    /// one, and no device written.
    pub fn can_go_back(&self) -> bool {
        self.choosing_profile || (self.added.is_none() && !self.visited.is_empty())
    }

    /// Go back to the step before this one; says whether the step changed.
    ///
    /// The second question of [`Step::Pick`] goes back to its first. Once the device has been
    /// written there is nothing to go back to: walking the steps again would write a second
    /// block for the same device.
    pub fn back(&mut self) -> bool {
        if self.choosing_profile {
            self.choosing_profile = false;
            return true;
        }
        if !self.can_go_back() {
            return false;
        }
        match self.visited.pop() {
            Some(step) => {
                self.step = step;
                true
            }
            None => false,
        }
    }

    /// Move to `step`, keeping where this came from so [`DeviceWalk::back`] can return to it.
    fn go(&mut self, step: Step) {
        self.visited.push(self.step);
        self.step = step;
    }

    /// The configured devices that hold a link, which the scan and the passphrase test may
    /// disturb, with go on or cancel.
    fn linked_prompt(&self) -> Prompt {
        let names = self.linked.join(", ");
        let (has, them) = match self.linked.len() {
            1 => ("has", "it"),
            _ => ("have", "them"),
        };
        Prompt {
            step: Step::Linked,
            title: "A device is linked".into(),
            body: vec![format!(
                "{names} {has} a link: the scan and the passphrase test may briefly disturb {them}."
            )],
            choices: LINKED_CHOICES.iter().map(|c| (*c).to_string()).collect(),
            default: 0,
            takes: Takes::Choice,
        }
    }

    /// The scan, and after a quiet one the reason a device usually stays quiet.
    fn scan_prompt(&self) -> Prompt {
        let mut body = self.notices.clone();
        let mut choices = vec!["Scan".to_string()];
        if self.scanned && self.rows.is_empty() {
            body.push(NOTHING_HEARD.into());
            choices = vec!["Scan again".into(), "Skip".into()];
        } else if !self.scanned {
            body.push(format!(
                "Listening takes up to {} s.",
                self.effects.scan_window().as_secs()
            ));
        }
        Prompt {
            step: Step::Scan,
            title: "Find the device".into(),
            body,
            choices,
            default: 0,
            takes: Takes::Choice,
        }
    }

    /// One line per device heard: what it is, its unit id, how loud it was, and the profile
    /// its advertisement matches, with a configured unit's name when it is marked.
    fn pick(&self) -> Prompt {
        Prompt {
            step: Step::Pick,
            title: "Which device".into(),
            body: self.notices.clone(),
            choices: self.rows.iter().map(|row| self.row(row)).collect(),
            default: 0,
            takes: Takes::Choice,
        }
    }

    /// One device as the line that offers it.
    fn row(&self, row: &Row) -> String {
        let entry = &row.entry;
        let product = self.product(entry).unwrap_or("unknown");
        let signal = entry
            .rssi
            .map(|rssi| format!("  {rssi} dBm"))
            .unwrap_or_default();
        let profile = self
            .matched(entry)
            .map(|profile| profile.name.clone())
            .or_else(|| self.given.profile.clone())
            .unwrap_or_else(|| UNKNOWN_PRODUCT.to_string());
        let configured = row
            .configured
            .as_ref()
            .map(|name| format!("  configured as {name}"))
            .unwrap_or_default();
        format!("{product}  {}{signal}  {profile}{configured}", entry.unit)
    }

    /// The profiles this build knows, for a device whose product none of them covers.
    fn profiles(&self) -> Prompt {
        let names = self.profile_names();
        let body = match self.picked() {
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
            takes: Takes::Choice,
        }
    }

    /// What to call the device, with the name it advertises as the default.
    fn name_prompt(&self) -> Prompt {
        Prompt {
            step: Step::Name,
            title: "What to call it".into(),
            body: vec![
                "Enter takes the name below; type another to use that instead.".into(),
                self.default_name(),
            ],
            choices: Vec::new(),
            default: 0,
            takes: Takes::Text,
        }
    }

    /// The four digits the device wants, `auto` to read the code from the device, or `none`
    /// for a device with no passphrase.
    ///
    /// The last line is what an empty one answers with, as it is on every step that takes a
    /// typed line: a device with no passphrase is configured with [`NONE`].
    fn passphrase_prompt(&self) -> Prompt {
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
            takes: Takes::Secret,
        }
    }

    /// Listen again, or give up on finding a device.
    ///
    /// A scan that cannot run at all (no adapter, a bus that will not answer) is not a quiet
    /// room, and answering the same step again asks the same adapter the same thing: after
    /// [`SCAN_ATTEMPTS`] of them in a row the walk stops on the last reason given.
    async fn searched(&mut self, choice: usize) -> Result<Walked, WalkError<E::Failure>> {
        if self.scanned && self.rows.is_empty() && choice == 1 {
            return Ok(Walked::Skipped);
        }
        let heard = match self.effects.scan().await {
            Ok(heard) => heard,
            Err(failure) => {
                self.scan_failures += 1;
                if self.scan_failures >= SCAN_ATTEMPTS {
                    self.stopped = true;
                }
                return Err(WalkError::Effect(failure));
            }
        };
        self.scan_failures = 0;
        let taken = self.taken_units();
        let mut left_out = Vec::new();
        let mut rows = Vec::new();
        for entry in heard.entries {
            let configured = configured_as(&taken, &entry.unit);
            match (configured, self.configured) {
                // A unit that is configured is not offered again: a second block for it would
                // be a second actor on one device, and the file refuses it.
                (Some(name), Configured::LeaveOut) => {
                    left_out.push(format!("{name} ({})", entry.unit))
                }
                (configured, _) => rows.push(Row { entry, configured }),
            }
        }
        self.rows = rows;
        self.picked = None;
        self.notices = heard.notices;
        if !left_out.is_empty() {
            self.notices.push(format!(
                "Left out, already configured: {}.",
                left_out.join(", ")
            ));
        }
        self.scanned = true;
        if !self.rows.is_empty() {
            self.go(Step::Pick);
        }
        Ok(Walked::On)
    }

    /// Take the device this choice names; a product no profile covers asks which profile to
    /// read it through before going on, and a configured unit is refused, naming its device.
    fn chose_device(&mut self, choice: usize) -> Result<Walked, WalkError<E::Failure>> {
        let Some(row) = self.rows.get(choice) else {
            return Err(out_of_range(choice));
        };
        if let Some(name) = &row.configured {
            return Err(WalkError::Configured {
                name: name.clone(),
                unit: row.entry.unit.clone(),
            });
        }
        let unmatched = self.matched(&row.entry).is_none();
        // A profile chosen at an earlier pick was for that device: each pick starts from what the
        // command line gave.
        self.profile = self.given.profile.clone();
        self.picked = Some(choice);
        if unmatched && self.profile.is_none() {
            self.choosing_profile = true;
            return Ok(Walked::On);
        }
        self.go(Step::Name);
        Ok(Walked::On)
    }

    /// Read the device through the profile this choice names, or through none when the build
    /// has no profiles to offer.
    fn chose_profile(&mut self, choice: usize) -> Result<Walked, WalkError<E::Failure>> {
        let names = self.profile_names();
        if !names.is_empty() {
            let name = names.get(choice).ok_or_else(|| out_of_range(choice))?;
            self.profile = Some(name.clone());
        }
        self.choosing_profile = false;
        self.go(Step::Name);
        Ok(Walked::On)
    }

    /// Name the device and try to reach it; a device that turns the connection away is asked
    /// for its passphrase instead.
    async fn named(&mut self, text: Option<String>) -> Result<Walked, WalkError<E::Failure>> {
        let typed = text.unwrap_or_default();
        let name = match typed.trim() {
            "" => self.default_name(),
            name => name.to_string(),
        };
        if let Some(why) = svolder_ipc::config::device_name_refusal(&name) {
            return Err(WalkError::Answer(why));
        }
        if self
            .effects
            .taken_names()
            .iter()
            .any(|taken| taken.eq_ignore_ascii_case(&name))
        {
            return Err(WalkError::NameTaken(name));
        }
        self.name = Some(name);
        let entry = self.picked().cloned().ok_or(WalkError::NoDevice)?;
        let passphrase = self.passphrase.clone();
        match self.tested(&entry, &passphrase).await? {
            Connect::Accepted => self.wrote().await,
            _ => {
                self.go(Step::Passphrase);
                Ok(Walked::On)
            }
        }
    }

    /// Try the code that was typed; a device that turns this one away too is said to have,
    /// and the step is asked again.
    async fn coded(&mut self, text: Option<String>) -> Result<Walked, WalkError<E::Failure>> {
        let code =
            passphrase_text(text.as_deref().unwrap_or_default()).map_err(WalkError::Answer)?;
        let entry = self.picked().cloned().ok_or(WalkError::NoDevice)?;
        match self.tested(&entry, &code).await? {
            Connect::Accepted => {
                self.passphrase = code;
                self.wrote().await
            }
            _ => Err(WalkError::Refused { code }),
        }
    }

    /// The passphrase test: accepted or refused, and a test that got no answer as the error it
    /// is.
    async fn tested(
        &mut self,
        entry: &ScanEntry,
        code: &Secret,
    ) -> Result<Connect, WalkError<E::Failure>> {
        let answered = self
            .effects
            .try_connect(entry, code.expose())
            .await
            .map_err(WalkError::Effect)?;
        self.connect = Some(answered);
        match answered {
            Connect::Accepted | Connect::Refused => Ok(answered),
            Connect::Failed { kind } => Err(WalkError::Connect(kind)),
            Connect::Unknown => Err(WalkError::Connect(ConnectFailure::Unknown)),
        }
    }

    /// Write the device. A name another device took meanwhile sends the walk back to its name
    /// step, a unit another block took to its pick step; a file that cannot take the write stops
    /// it.
    async fn wrote(&mut self) -> Result<Walked, WalkError<E::Failure>> {
        let entry = self.picked().cloned().ok_or(WalkError::NoDevice)?;
        let device = DeviceConfig {
            name: self.name.clone().unwrap_or_default(),
            udid: entry.unit.clone(),
            profile: self.profile.clone(),
            passphrase: self.passphrase.expose().to_string(),
            inputs: BTreeMap::new(),
        };
        match self.effects.write_device(&device).await {
            Ok(()) => {
                self.added = Some(device);
                Ok(Walked::Added)
            }
            Err(WriteError::NameTaken(name)) => {
                self.back_to(Step::Name);
                Err(WalkError::NameTaken(name))
            }
            Err(WriteError::BadName(words)) => {
                self.back_to(Step::Name);
                Err(WalkError::Answer(words))
            }
            Err(WriteError::UnitConfigured(words)) => {
                self.back_to(Step::Pick);
                let taken = self.taken_units();
                for row in &mut self.rows {
                    row.configured = configured_as(&taken, &row.entry.unit);
                }
                Err(WalkError::UnitConfigured(words))
            }
            Err(WriteError::Stop(failure)) => {
                self.stopped = true;
                Err(WalkError::Effect(failure))
            }
        }
    }

    /// Step back to `step`, a step this walk came through, as [`DeviceWalk::back`] would.
    fn back_to(&mut self, step: Step) {
        while self.step != step {
            match self.visited.pop() {
                Some(earlier) => self.step = earlier,
                None => break,
            }
        }
    }

    /// The configured units, as unit ids with the names of their devices.
    fn taken_units(&self) -> Vec<(String, UnitId)> {
        self.effects
            .taken_units()
            .into_iter()
            .filter_map(|(name, unit)| Some((name, unit.parse().ok()?)))
            .collect()
    }

    /// What the device behind a scan row is called: the name the scan gave it, else the name
    /// the registry gives its product id, which is how a product the codec's table does not
    /// know is named after the profile made for it.
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
    /// else its unit id. The name step offers it as its last body line.
    pub fn default_name(&self) -> String {
        if let Some(name) = self.given.name.as_deref() {
            return name.to_string();
        }
        let Some(entry) = self.picked() else {
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

/// The name a configured unit carries, when `unit` is one of `taken`.
fn configured_as(taken: &[(String, UnitId)], unit: &str) -> Option<String> {
    let unit = unit.parse::<UnitId>().ok()?;
    taken
        .iter()
        .find(|(_, held)| *held == unit)
        .map(|(name, _)| name.clone())
}

/// The passphrase a typed answer means: four digits are themselves, `auto` is the code the
/// advertisement carries, and an empty line, or the word the step offers as what an empty line
/// stands for, is a device with no passphrase.
pub fn passphrase_text(typed: &str) -> Result<Secret, String> {
    let typed = typed.trim();
    if typed.is_empty() || typed.eq_ignore_ascii_case(NONE) {
        return Ok(Secret::new(NONE));
    }
    if typed.eq_ignore_ascii_case(AUTO) {
        return Ok(Secret::new(AUTO));
    }
    if typed.len() == 4 && typed.chars().all(|c| c.is_ascii_digit()) {
        return Ok(Secret::new(typed));
    }
    Err("a code is four digits, or `auto`, or `none`".into())
}

/// A choice that is not one of the ones offered.
pub fn out_of_range<F>(choice: usize) -> WalkError<F> {
    WalkError::Answer(format!("there is no choice {}", choice + 1))
}

#[cfg(test)]
mod tests;
