// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Answering a command: reads from the session's last known state, writes built through the
//! profile and sent to the device.
//!
//! Every command answers at once when the link is down, so a front end is never left waiting on
//! a device that is away.

use svolder_ble::DeviceState;
use svolder_ipc::Response;
use svolder_profiles::roles;
use svolder_profiles::{EqMode, EqParam, ProfileError, intent};
use svolder_proto::function::IDENTIFY;
use svolder_proto::{FunctionId, Message, ProtoError, Value, ValueType};

use super::answers::{Refused, error_for, no_eq, not_connected, usage, usage_error};
use super::{Actor, Command};
use crate::log;

impl Actor {
    /// Run one command against the session. Every command answers immediately when the link
    /// is down: a read from the last known state, anything that would send a frame refused.
    pub(super) async fn run_command(&mut self, command: Command) -> Response {
        // A command built from the profile is refused up front while the link is down: until
        // an advertisement has been seen the profile is a placeholder, and its complaints
        // would be about the placeholder rather than about the missing device. `Raw` is not
        // on that list, so its refusal of a protected function id stands whether the device
        // is there or not; `send` still turns it away for want of a link.
        if command.needs_link() && !self.linked() {
            return not_connected();
        }
        match command {
            Command::State => self.state_values(),
            Command::EqShow => self.eq_values(),
            Command::Get { key } => self.get(&key),
            Command::Set {
                key,
                value,
                step,
                no_wait,
            } => match self.build_set(&key, &value, step) {
                Ok(message) => self.send(message, no_wait).await,
                Err(refused) => refused.into(),
            },
            Command::Raw {
                id,
                value_type,
                value,
                no_wait,
            } => match build_raw(&id, &value_type, &value) {
                Ok(message) => self.send(message, no_wait).await,
                Err(refused) => refused.into(),
            },
            Command::EqSet {
                band,
                param,
                value,
                no_wait,
            } => match self.build_eq_set(band, &param, value) {
                Ok(message) => self.send(message, no_wait).await,
                Err(refused) => refused.into(),
            },
            Command::EqMode { mode, no_wait } => match self.build_eq_mode(&mode) {
                Ok(message) => self.send(message, no_wait).await,
                Err(refused) => refused.into(),
            },
            Command::Passphrase { digits } => self.passphrase(&digits).await,
            Command::Identify => self.identify().await,
            Command::Reload(registry) => {
                self.reload(registry);
                Response::Ack
            }
            Command::Configure(config) => self.configure(config),
            Command::Wake => Response::Ack,
            // Every loop that forwards a command here takes `Shutdown` for itself first, so
            // this arm is unreachable from any of them.
            Command::Shutdown => Response::Ack,
        }
    }

    /// Send one command through the session.
    ///
    /// Every frame the daemon produces passes the one refusal check here, before the session
    /// is even reached: the firmware-update family and the passphrase function are turned
    /// away, so the session's own codec check is a second line of defence, not the first.
    pub(super) async fn send(&mut self, message: Message, no_wait: bool) -> Response {
        if let Some(refusal) = refuse(message.id) {
            return refusal;
        }
        if !self.linked() {
            return not_connected();
        }
        log::message("->", &self.name, &message);
        // An action has no echo and no reading: it is sent as a write that waits for nothing, and
        // nothing of it is published or folded as a status, however the command asked.
        let no_wait = no_wait || self.profile.action_of(message.id).is_some();
        let outcome = {
            let session = self.session.as_mut().expect("a link implies a session");
            if no_wait {
                session.set_no_wait(message).await.map(|()| None)
            } else {
                session.set(message).await.map(Some)
            }
        };
        match outcome {
            Ok(None) => Response::Ack,
            Ok(Some(confirmed)) => {
                // An echo the device answered with no refusal shows the code works.
                self.judge_passphrase();
                // Subscribers see confirmed values too: `Session::set` consumes the echo, so
                // the actor's own read never sees it.
                self.publish_status(&confirmed);
                Response::Value(self.value_object(&confirmed))
            }
            Err(e) => {
                self.judge_passphrase();
                error_for(&e)
            }
        }
    }

    /// Ask the device to show itself, through the session's one send per gap, unless its profile
    /// offers no identify (a usage refusal, link or not). The frame is logged like any other the
    /// daemon sends, only when it goes out.
    async fn identify(&mut self) -> Response {
        if !self.profile.identify {
            return usage_error(&format!("{} offers no identify", self.name));
        }
        if !self.linked() {
            return not_connected();
        }
        let session = self.session.as_mut().expect("a link implies a session");
        match session.identify().await {
            Ok(sent) => {
                if sent {
                    log::message(
                        "->",
                        &self.name,
                        &Message::command(IDENTIFY, Value::Data(vec![])),
                    );
                }
                Response::Identify {
                    device: self.name.clone(),
                    sent,
                }
            }
            Err(e) => error_for(&e),
        }
    }

    /// The readings reads answer from: the session's own while it holds one, and otherwise
    /// the copy taken before a connect attempt carried the session off.
    pub(super) fn state_of(&self) -> &DeviceState {
        match self.session.as_ref() {
            Some(session) => session.state(),
            None => &self.last_state,
        }
    }

    /// Whether the device has ever been found; until then there is no profile to look a key
    /// up in.
    pub(super) fn seen(&self) -> bool {
        self.advert.is_some()
    }

    /// The answer for a read of a device the daemon has never found: code 3, in words that say
    /// the daemon is still looking, which a front end tells from a link that was lost.
    pub(super) fn not_found(&self) -> Response {
        Response::Error {
            code: 3,
            message: svolder_ipc::not_found_yet(&self.name),
        }
    }

    /// Every reading the device has confirmed.
    pub(super) fn state_values(&self) -> Response {
        // As in `get` and `eq_values`: with no sighting there is nothing to read and no
        // profile to describe it through, so report the missing device rather than answer
        // with an empty list a front end would print as a healthy device with no controls.
        if !self.seen() {
            return self.not_found();
        }
        let values = self
            .state_of()
            .messages()
            .map(|m| self.value_object(m))
            .collect();
        Response::Values {
            device: self.name.clone(),
            values,
        }
    }

    /// Every tone-control reading the device has confirmed.
    pub(super) fn eq_values(&self) -> Response {
        // Before the first sighting the profile is a placeholder that describes no tone
        // control whatever the device has: report the missing link, not a missing eq block.
        if !self.seen() {
            return self.not_found();
        }
        let Some(eq) = self.profile.eq.as_ref() else {
            return no_eq();
        };
        let values = self
            .state_of()
            .messages()
            .filter(|m| m.id.first() == eq.id)
            .map(|m| self.value_object(m))
            .collect();
        Response::Values {
            device: self.name.clone(),
            values,
        }
    }

    /// One control's confirmed reading.
    pub(super) fn get(&self, key: &str) -> Response {
        // As in `eq_values`: with no sighting there is no profile to look a key up in.
        if !self.seen() {
            return self.not_found();
        }
        let Some(control) = self.profile.control_by_key(key) else {
            // An action is sent and never read: the profile crate says how to send it.
            if let Some(action) = self.profile.action_by_key(key) {
                return usage_error(
                    &ProfileError::NoReading {
                        key: action.key.clone(),
                        device_word: action.slots_word(),
                    }
                    .to_string(),
                );
            }
            return usage_error(&self.profile.unknown_key(key).to_string());
        };
        match self.state_of().get(control.id) {
            Some(m) => Response::Value(self.value_object(m)),
            None => Response::Error {
                code: 4,
                message: format!("no value reported yet for {key}"),
            },
        }
    }

    /// The command one `set` asks for: a literal through `intent`, or a step from the
    /// control's current raw value.
    pub(super) fn build_set(&self, key: &str, value: &str, step: i64) -> Result<Message, Refused> {
        match value.to_ascii_lowercase().as_str() {
            "up" => self.build_relative(key, step),
            "down" => self.build_relative(key, step.saturating_neg()),
            _ => intent(&self.profile, key, value).map_err(|e| usage(e.to_string())),
        }
    }

    /// The command that nudges a level control from what the device last reported.
    ///
    /// A control that cannot be stepped is turned away before the current value is looked
    /// up, as it is on the CLI's direct path, so `set mute up` complains about the control
    /// (code 1) rather than about a reading a boolean was never going to supply (code 4).
    pub(super) fn build_relative(&self, key: &str, delta: i64) -> Result<Message, Refused> {
        let current = self
            .profile
            .control_by_key(key)
            .and_then(|control| self.state_of().get(control.id))
            .and_then(|m| m.value.as_i64());
        roles::relative_step(&self.profile, key, current, delta)
            .map_err(|e| usage(e.to_string()))?
            .ok_or_else(|| Refused {
                code: 4,
                message: format!("no current value for {key}"),
            })
    }

    /// The command that sets one band parameter; `band` is 1-based, as on the wire.
    pub(super) fn build_eq_set(
        &self,
        band: usize,
        param: &str,
        value: f64,
    ) -> Result<Message, Refused> {
        let param: EqParam = param
            .parse()
            .map_err(|e: ProfileError| usage(e.to_string()))?;
        let band0 = band
            .checked_sub(1)
            .ok_or_else(|| usage("band numbers start at 1"))?;
        svolder_profiles::eq_set(&self.profile, band0, param, value)
            .map_err(|e| usage(e.to_string()))
    }

    /// The command that selects a tone-control mode.
    pub(super) fn build_eq_mode(&self, mode: &str) -> Result<Message, Refused> {
        let mode: EqMode = mode
            .parse()
            .map_err(|e: ProfileError| usage(e.to_string()))?;
        svolder_profiles::eq_mode(&self.profile, mode).map_err(|e| usage(e.to_string()))
    }
}

/// The refusal a protected function id earns, or `None` when a frame for it may be built.
///
/// The words are [`FunctionId::refusal`]'s, so the daemon turns a protected id away in
/// exactly the same terms the CLI's own two gates use: the firmware-update family, and the
/// passphrase function, which only `Command::Passphrase` may address.
pub(super) fn refuse(id: FunctionId) -> Option<Response> {
    id.refusal().map(|refusal| Response::Error {
        code: 1,
        message: refusal.to_string(),
    })
}

/// The command a `raw` request asks for, parsed exactly as the CLI parses it.
pub(super) fn build_raw(id: &str, value_type: &str, value: &str) -> Result<Message, Refused> {
    let id: FunctionId = id.parse().map_err(|e: ProtoError| usage(e.to_string()))?;
    let value_type: ValueType = value_type
        .parse()
        .map_err(|e: ProtoError| usage(e.to_string()))?;
    let value = Value::from_text(value_type, value).map_err(|e| usage(e.to_string()))?;
    Ok(Message::command(id, value))
}
