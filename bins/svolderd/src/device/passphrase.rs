// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The passphrase a device's entry asks for: keying the session with it, judging it against the
//! advertisement, and changing it on the device.
//!
//! A device on `auto` that refuses the code it advertises gets one release of the link to read
//! the code afresh.

use svolder_ble::{Advertisement, Session};
use svolder_ipc::config::DeviceConfig;
use svolder_ipc::{Event, Response, passphrase_verdict};
use svolder_proto::{Message, Passphrase, Value};

use super::Actor;
use super::answers::{error_for, not_connected, usage_error};
use crate::locks::locked;
use crate::log;

/// What the log says when a device on `auto` refuses the code its advertisement carried and the
/// daemon releases the link to read the code afresh: one line for the episode, whatever follows.
pub(super) const REFUSED_LINE: &str = "the device refused the code its advertisement carried; \
     releasing the link to read the code afresh, once";

impl Actor {
    /// Key the session, when there is one, with the passphrase this device's entry asks for
    /// against `advert`, or say why the entry's digits are no passphrase.
    pub(super) fn rekey(&mut self, advert: &Advertisement) -> Result<(), String> {
        let passphrase = resolve_passphrase(&self.config, advert)?;
        if let Some(session) = self.session.as_mut() {
            session.set_passphrase(passphrase);
        }
        Ok(())
    }

    /// Judge the configured passphrase against the advertisement the actor holds, and say so
    /// when the verdict changes.
    ///
    /// Best effort: the advertisement can be older than the device's code, so a verdict is a
    /// note and never a refusal, and nothing the actor sends depends on it. It stands until the
    /// entry, the advertisement or the device settles it: a session the device has answered
    /// without a refusal (a connect's refresh burst, or the echo of a waited `set`) has shown
    /// the code works, whatever the advertisement says. With no advertisement held there is
    /// nothing to judge against and no note.
    ///
    /// It also records whether the link is refused, for `devices`, and ends the refusal episode
    /// when the device has shown another code works than the one it refused.
    ///
    /// Says whether the verdict changed, which is when it is logged: a repeat of the same one,
    /// as every connect attempt makes, is not.
    pub(super) fn judge_passphrase(&mut self) -> bool {
        if let Some(session) = self.session.as_ref() {
            self.episode.observe(session);
        }
        let refused = self.session.as_ref().is_some_and(Session::refused);
        locked(&self.info).refused = refused;
        let proven = self.session.as_ref().is_some_and(Session::code_accepted);
        let note = match (self.advert.as_ref(), proven) {
            (Some(advert), false) => {
                passphrase_verdict(&self.config.passphrase, &advert.manufacturer)
            }
            _ => None,
        };
        if note == self.note {
            return false;
        }
        self.note = note;
        locked(&self.info).passphrase_note = note;
        match note {
            Some(note) => log::warn(&note.line(&self.name)),
            None => log::info(&format!(
                "{}: the passphrase note no longer applies",
                self.name
            )),
        }
        true
    }

    /// Release the link when a device on `auto` has refused its code on it and the episode still
    /// has its one drop, so the attempt that follows listens for the device's next advertisement
    /// and keys the session from that. Says whether the link was released, which the caller
    /// treats as a link that dropped.
    ///
    /// The drop goes through the states a lost link does, and is logged once: the episode has no
    /// second drop to log. Nothing else about the reconnect changes: the attempt follows at once,
    /// and the schedule between attempts is for the ones that fail.
    pub(super) async fn release_after_refusal(&mut self) -> bool {
        let auto = self.config.passphrase == "auto";
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        if !self.episode.due(auto, session) {
            return false;
        }
        self.episode.dropped(session.passphrase());
        log::warn(&format!("{}: {REFUSED_LINE}", self.name));
        session.close().await;
        self.set_connected(false);
        let device = self.name.clone();
        self.publish(Event::Connection {
            device,
            connected: false,
        });
        true
    }

    /// Change the device's passphrase, then encode later frames with it.
    pub(super) async fn passphrase(&mut self, digits: &str) -> Response {
        let passphrase = match Passphrase::from_text(digits).map_err(|e| e.to_string()) {
            Ok(p) => p,
            Err(message) => return usage_error(&message),
        };
        if !self.linked() {
            return not_connected();
        }
        let message = Message::command(
            svolder_proto::function::PASSPHRASE,
            Value::U32(passphrase.encoded()),
        );
        log::message("->", &self.name, &message);
        let session = self.session.as_mut().expect("a link implies a session");
        match session.set_no_wait(message).await {
            Ok(()) => {
                session.set_passphrase(passphrase);
                // The device advertises the new code from now on, and does not while this link
                // holds it, so the held advertisement is made to say so: `auto`, a re-key and
                // the verdict all agree with what the device holds.
                if let Some(advert) = self.advert.as_mut() {
                    advert.set_advertised_passphrase(passphrase);
                }
                Response::Ack
            }
            Err(e) => error_for(&e),
        }
    }
}

/// The passphrase to encode with: the advertised one, none, or the configured digits.
pub(super) fn resolve_passphrase(
    config: &DeviceConfig,
    advert: &Advertisement,
) -> Result<Passphrase, String> {
    match config.passphrase.as_str() {
        "auto" => Ok(advert.manufacturer.passphrase()),
        text => Passphrase::from_text(text).map_err(|e| e.to_string()),
    }
}
