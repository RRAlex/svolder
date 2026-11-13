// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why an answer did not move the walk on, and the passphrase that is never shown.

use svolder_ipc::ConnectFailure;

/// A passphrase as the walk holds it: printed with `{:?}` it reads `<hidden>`, it has no
/// `Display` and no serde, and only [`Secret::expose`] gives the digits back.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Hold `text` as a secret.
    pub fn new(text: impl Into<String>) -> Secret {
        Secret(text.into())
    }

    /// The text itself, for the one place that needs it (the device, the file, or the person
    /// who typed it).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for Secret {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<hidden>")
    }
}

/// Why an answer did not move the walk on. The step stays where it is to be asked again,
/// unless [`crate::DeviceWalk::stopped`] says the walk has stopped on it.
#[derive(Debug)]
pub enum WalkError<F> {
    /// An effect did not happen: a scan that could not run, a session that failed, a file that
    /// cannot take the write.
    Effect(F),
    /// The answer is not one the step takes, in words that say what it takes.
    Answer(String),
    /// Another configured device already carries this name.
    NameTaken(String),
    /// The row picked is a unit configured already, under this name.
    Configured {
        /// the configured device's name
        name: String,
        /// its unit id
        unit: String,
    },
    /// Another block configures the unit, in the writer's words.
    UnitConfigured(String),
    /// The device turned this code away.
    Refused {
        /// the code it turned away: `auto` or four digits
        code: Secret,
    },
    /// The passphrase test got no answer from the device.
    Connect(ConnectFailure),
    /// A step that needs a device was reached with none picked.
    NoDevice,
}

impl<F> WalkError<F> {
    /// The words a front end shows for this refusal, given the words it uses for an effect's
    /// failure. A refused code is never quoted: a front end that wants to say which code was
    /// turned away reads [`WalkError::Refused`] itself.
    pub fn words(&self, effect: impl FnOnce(&F) -> String) -> String {
        match self {
            WalkError::Effect(failure) => effect(failure),
            WalkError::Answer(why) => why.clone(),
            WalkError::NameTaken(name) => format!("a device named {name:?} is already configured"),
            WalkError::Configured { name, unit } => {
                format!("{unit} is configured already, as {name}")
            }
            WalkError::UnitConfigured(words) => words.clone(),
            WalkError::Refused { code } if code.expose() == crate::AUTO => {
                "the device did not accept the code it advertises".to_string()
            }
            WalkError::Refused { .. } => "the device did not accept that code".to_string(),
            WalkError::Connect(kind) => connect_words(*kind).to_string(),
            WalkError::NoDevice => "no device has been picked".to_string(),
        }
    }
}

/// What a passphrase test that got no answer says.
pub fn connect_words(kind: ConnectFailure) -> &'static str {
    match kind {
        ConnectFailure::NotInRange => {
            "the device was not heard: power it on and close the phone app, which holds its only \
             connection"
        }
        ConnectFailure::Link => "the device was heard, but the link to it could not be opened",
        ConnectFailure::Held => "a configured device already reaches for that unit",
        ConnectFailure::Stopping => "the daemon is stopping",
        ConnectFailure::Unknown => "the passphrase test got no answer",
    }
}
