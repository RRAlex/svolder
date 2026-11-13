// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a walk shows: one [`Prompt`] per step, and the [`Step`]s a guided run is made of.

use serde::{Deserialize, Serialize};

/// The steps of a guided run, in the order it walks them.
///
/// The device walk ([`crate::DeviceWalk`]) walks [`Step::Linked`], [`Step::Scan`],
/// [`Step::Pick`], [`Step::Name`] and [`Step::Passphrase`]; a front end's first run walks the
/// others itself around it (the host's health and fixes before it, the adapter, the daemon and
/// what was written after it), so one vocabulary names every step a person is shown.
///
/// A step that has nothing to ask is not walked: a walk with no device linked skips
/// [`Step::Linked`], a device that connects on the advertised passphrase skips
/// [`Step::Passphrase`], a healthy host skips [`Step::Fixes`], and a machine with one adapter
/// skips [`Step::Adapter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// What the host checks say about this machine.
    Health,
    /// The fixes svolder may apply itself, as one confirmation.
    Fixes,
    /// The devices that have a link, which the scan and the passphrase test may disturb.
    Linked,
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

/// What kind of answer a prompt takes, so a front end draws the right control without guessing
/// from the step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Takes {
    /// The number of one of the prompt's choices.
    Choice,
    /// A line of text; an empty one takes the prompt's last body line.
    Text,
    /// A line of text that is a secret: a front end hides it as it is typed and never shows it
    /// back.
    Secret,
}

/// One step as something to show and something to answer.
///
/// A prompt that takes a choice takes the number of one of its choices; one that takes text
/// offers none, and its last body line is the default an empty line stands for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    /// which step this is
    pub step: Step,
    /// the one line that says what is being asked
    pub title: String,
    /// what the reader needs to answer it
    pub body: Vec<String>,
    /// what can be chosen, in the order they are offered
    pub choices: Vec<String>,
    /// which choice an empty answer takes
    pub default: usize,
    /// what kind of answer it takes
    pub takes: Takes,
}
