// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the bridge does with a message from the broker and with the end of a write: parsing a
//! command topic against the device's profile, handing the write to the device's writer, and
//! logging how it ended.
//!
//! A command never reaches the device directly: [`BridgeTask::enqueue`] gives it to the device's
//! writer, which sends it in order, and its outcome returns on the task's `writes` set as a
//! [`WriteOutcome`]. The loop in [`super::connection`] calls [`BridgeTask::message`] for each
//! arrival and [`BridgeTask::take_outcome`] for each end.

use svolder_ipc::Response;
use svolder_mqtt::discovery;
use svolder_profiles::tone::preset;
use svolder_profiles::{EqMode, Profile, ProfileError, roles};
use svolder_proto::Value;
use tokio::sync::oneshot;

use super::discovery::{discovery_unit, fitting_presets};
use super::{BridgeTask, Control, WriteOutcome, WriteRequest};
use crate::device::Command;
use crate::log;
use crate::recall::{self, Recalled};
use crate::writer::{Glided, Mode, Options, Origin, Outcome, Planned, Recalling, not_running};

impl BridgeTask {
    /// Act on one message from a subscription: a command for one of the devices, or a
    /// discovery config found under one of them.
    ///
    /// A command the broker hands over as its topic's retained message is never acted on: the
    /// broker gives that copy to every new subscription, so a command someone published
    /// retained would be written to the device again on every connect. It is said once per
    /// topic, and the message itself is left where it is, since it is not this bridge's.
    pub(super) fn message(&mut self, topic: &str, body: &[u8], retained: bool) {
        match command_target(&self.config.base_topic, topic) {
            Some(_) if self.config.commands && retained => {
                if self.retained_commands.insert(topic.to_string()) {
                    log::warn(&format!(
                        "mqtt: ignored a retained command on {topic}; a command is acted on only \
                         as it is published"
                    ));
                }
            }
            Some((unit, key)) if self.config.commands => {
                let (unit, key) = (unit.to_string(), key.to_string());
                self.command(&unit, &key, body);
            }
            _ => self.leftover(topic, body),
        }
    }

    /// One command: parsed against the device's profile the way `svolder set` parses a value,
    /// then handed to the device's actor, or logged and dropped.
    fn command(&mut self, unit: &str, key: &str, body: &[u8]) {
        let Some(published) = self.devices.get(unit) else {
            return;
        };
        let (name, profile) = {
            let feed = published.feed.borrow();
            (feed.name.clone(), feed.profile.clone())
        };
        let drop =
            |why: &str| log::warn(&format!("mqtt: {name}: a command for {key} dropped: {why}"));
        let Ok(text) = std::str::from_utf8(body) else {
            return drop("the payload is not text");
        };
        let text = text.trim();
        // The identify button is not a control of the profile: a press asks the unit to show
        // itself, and needs no profile to be understood.
        if key.eq_ignore_ascii_case(discovery::IDENTIFY_KEY) {
            if text != discovery::IDENTIFY_PRESS {
                return drop(&format!("{text:?} is not {:?}", discovery::IDENTIFY_PRESS));
            }
            return self.enqueue(
                (unit.to_string(), discovery::IDENTIFY_KEY.to_string()),
                WriteRequest::Identify,
            );
        }
        let Some(profile) = profile else {
            return drop("the device has not been seen yet");
        };
        if svolder_mqtt::device_presets::target(&profile, key) {
            return self.device_preset_command(unit, &name, &profile, text);
        }
        if let Some(target) = svolder_mqtt::eq::target(&profile, key) {
            return self.eq_command(unit, &name, &profile, target, text);
        }
        // Only a control discovery announces as writable takes commands, and nothing
        // restricted is on that list.
        let Some(control) = discovery::exposed_controls(&profile)
            .into_iter()
            .find(|c| c.key.eq_ignore_ascii_case(key) && c.kind.writable())
        else {
            return drop("no control of this device takes commands on that topic");
        };
        let parsed = match svolder_mqtt::parse_command(
            &profile,
            &control.key,
            text,
            self.config.max_volume_db,
        ) {
            Ok(parsed) => parsed,
            Err(e) => return drop(&format!("{text:?}: {e}")),
        };
        if parsed.clamped
            && let Some(cap) = self.config.max_volume_db
        {
            log::info(&match parsed.quietest {
                true => format!(
                    "mqtt: {name}: volume {text} dB: max_volume_db, {cap} dB, is below every step; \
                     the quietest step was used"
                ),
                false => {
                    format!("mqtt: {name}: volume {text} dB capped at max_volume_db, {cap} dB")
                }
            });
        }
        let request = match control.key.eq_ignore_ascii_case(discovery::EQ_MODE_KEY) {
            true => WriteRequest::EqMode {
                mode: parsed
                    .message
                    .value
                    .as_u64()
                    .and_then(|raw| u8::try_from(raw).ok())
                    .and_then(EqMode::from_raw)
                    .map_or_else(|| text.to_string(), |mode| mode.name().to_string()),
            },
            false => WriteRequest::Set {
                key: control.key.clone(),
                value: set_value(&parsed.message.value),
            },
        };
        self.enqueue((unit.to_string(), control.key), request);
    }

    /// One command for a tone-control entity: parsed as `svolder eq set` parses a band value,
    /// a loudness strength or a preset's name, then handed to the device's actor, or logged and
    /// dropped. A preset command naming none of the presets that fit the device, `custom`
    /// among them, is dropped.
    fn eq_command(
        &mut self,
        unit: &str,
        name: &str,
        profile: &Profile,
        target: svolder_mqtt::eq::Target,
        text: &str,
    ) {
        let key = svolder_mqtt::eq::key(target);
        let drop =
            |why: &str| log::warn(&format!("mqtt: {name}: a command for {key} dropped: {why}"));
        let parsed = match svolder_mqtt::eq::parse(profile, target, text) {
            Ok(parsed) => parsed,
            Err(e) => return drop(&format!("{text:?}: {e}")),
        };
        let request = match parsed {
            svolder_mqtt::eq::Command::Band { band, param, value } => {
                WriteRequest::EqBand { band, param, value }
            }
            svolder_mqtt::eq::Command::Loudness(raw) => {
                let id = profile.eq.as_ref().map(|eq| eq.loudness_id());
                let Some(id) = id else {
                    return drop(&ProfileError::NoEq.to_string());
                };
                WriteRequest::Loudness {
                    key: profile.control_by_id(id).map(|c| c.key.clone()),
                    id,
                    raw,
                }
            }
            svolder_mqtt::eq::Command::Preset(asked) => {
                let listed = svolder_mqtt::eq::preset_options(
                    &fitting_presets(profile, &self.presets)
                        .into_iter()
                        .map(|p| p.name)
                        .collect::<Vec<_>>(),
                );
                let Some(chosen) = listed
                    .iter()
                    .filter(|option| !preset::same_name(option, svolder_mqtt::eq::CUSTOM))
                    .find(|option| preset::same_name(option, &asked))
                else {
                    return drop(&format!(
                        "{asked:?} is not one of the presets that fit this device ({})",
                        listed.join(", ")
                    ));
                };
                WriteRequest::Preset {
                    name: chosen.clone(),
                }
            }
        };
        self.enqueue((unit.to_string(), key), request);
    }

    /// One command for the preset select of a device with slots and built-in settings of its
    /// own: a slot is recalled, a built-in setting applied as one batch, and `custom`, a save or
    /// anything the device does not list is logged and dropped. Nothing here asks: a recall is
    /// the user's own press, and the bridge asks about no band write.
    fn device_preset_command(&mut self, unit: &str, name: &str, profile: &Profile, text: &str) {
        let key = svolder_mqtt::device_presets::PRESET_KEY;
        let request = match svolder_mqtt::device_presets::parse(profile, text) {
            Ok(svolder_mqtt::device_presets::Command::Slot(slot)) => WriteRequest::Slot { slot },
            Ok(svolder_mqtt::device_presets::Command::Setting(name)) => {
                WriteRequest::Setting { name }
            }
            Err(e) => {
                return log::warn(&format!(
                    "mqtt: {name}: a command for {key} dropped: {text:?}: {e}"
                ));
            }
        };
        self.enqueue((unit.to_string(), key.to_string()), request);
    }

    /// Hand `request` to the writer of `control`'s device: newest-wins on its function, a preset
    /// or a built-in setting as one batch; what comes back arrives on [`BridgeTask::writes`].
    pub(super) fn enqueue(&mut self, control: Control, request: WriteRequest) {
        let Some(published) = self.devices.get(&control.0) else {
            return;
        };
        let writer = published.writer.clone();
        let device = published.feed.borrow().name.clone();
        let batch = match &request {
            WriteRequest::Preset { name } => {
                let fitting = published
                    .feed
                    .borrow()
                    .profile
                    .as_deref()
                    .map(|profile| fitting_presets(profile, &self.presets))
                    .unwrap_or_default();
                Some(writer.recall(Origin::Mqtt, name.clone(), fitting))
            }
            WriteRequest::Setting { name } => Some(writer.apply(Origin::Mqtt, name.clone())),
            _ => None,
        };
        if let Some(Recalling { planned, done }) = batch {
            let started = control.clone();
            self.writes.spawn(async move {
                match planned.await {
                    Ok(planned) => WriteOutcome::Planned(started, planned),
                    Err(_) => WriteOutcome::Replaced,
                }
            });
            self.writes
                .spawn(async move { WriteOutcome::Written(control, ended(done.await, &device)) });
            return;
        }
        let volume = published
            .feed
            .borrow()
            .profile
            .as_deref()
            .and_then(roles::volume)
            .map(|volume| volume.key.clone());
        if let WriteRequest::Set { key, value } = &request
            && volume.is_some_and(|volume| volume.eq_ignore_ascii_case(key))
            && let Ok(raw) = value.parse::<i64>()
        {
            let answer = writer.glide(Origin::Mqtt, key.clone(), raw, Mode::Latest);
            self.writes.spawn(async move {
                WriteOutcome::Written(control, glided(answer.await, &device))
            });
            return;
        }
        let options = match request {
            WriteRequest::EqBand { .. } => Options {
                mode: Mode::Latest,
                flat_to_eq: true,
            },
            // A cue is no setting: every press is its own, and the session decides which send.
            // A slot's recall is the same: each pick is a press, and none replaces another.
            WriteRequest::Identify | WriteRequest::Slot { .. } => Options::from(Mode::Ordered),
            _ => Options::from(Mode::Latest),
        };
        let answer = writer.submit(Origin::Mqtt, command(request), options);
        self.writes
            .spawn(async move { WriteOutcome::Written(control, ended(answer.await, &device)) });
    }

    /// The configured name of the device `unit`, or the unit id when it is not tracked.
    fn name_of(&self, unit: &str) -> String {
        self.devices.get(unit).map_or_else(
            || unit.to_string(),
            |published| published.feed.borrow().name.clone(),
        )
    }

    /// Act on what came back from a device's writer.
    pub(super) fn take_outcome(&mut self, outcome: WriteOutcome) {
        match outcome {
            WriteOutcome::Planned(control, Planned::Plan(recalled)) => {
                self.planned(&control.0, &recalled);
            }
            WriteOutcome::Planned(control, Planned::Refused(why)) => {
                let name = self.name_of(&control.0);
                log::debug(&format!(
                    "mqtt: {name}: the preset could not be planned: {why}"
                ));
            }
            WriteOutcome::Planned(control, Planned::Setting(applied)) => {
                let name = self.name_of(&control.0);
                log::debug(&format!(
                    "mqtt: {name}: built-in setting {} planned with {} write(s)",
                    applied.name,
                    applied.writes.len()
                ));
            }
            WriteOutcome::Replaced => {}
            WriteOutcome::Written(control, outcome) => self.written(&control, outcome),
        }
    }

    /// A recall of `unit` started with `recalled`'s plan: log what it moved to fit. The device's
    /// select follows the writer's memory, which the plan's start has set.
    fn planned(&self, unit: &str, recalled: &Recalled) {
        let Some(published) = self.devices.get(unit) else {
            return;
        };
        let device = published.feed.borrow().name.clone();
        for line in clamp_lines(&device, recalled) {
            log::info(&line);
        }
    }

    /// One write ended: say so when the device or the actor refused it.
    fn written(&self, control: &Control, outcome: Outcome) {
        let name = self.name_of(&control.0);
        let response = match outcome {
            Outcome::Done(response) => response,
            Outcome::Superseded => {
                log::debug(&format!(
                    "mqtt: {name}: {} superseded by a newer command",
                    control.1
                ));
                return;
            }
            Outcome::Cancelled(why) => Response::Error {
                code: 3,
                message: why.to_string(),
            },
        };
        match &response {
            Response::Error { message, .. } => log::warn(&format!(
                "mqtt: {name}: {} not written: {message}",
                control.1
            )),
            Response::Identify { sent, .. } => log::info(&identify_line(&name, *sent)),
            _ => log::debug(&format!("mqtt: {name}: {} written", control.1)),
        }
    }

    /// A discovery config the broker holds under one of this bridge's devices: cleared when the
    /// device's own configs are known and none of them is on that topic, else kept until they
    /// are. An empty payload is a config already cleared, and one this bridge published is
    /// its own.
    fn leftover(&mut self, topic: &str, body: &[u8]) {
        if body.is_empty() {
            return;
        }
        let Some(unit) = discovery_unit(&self.config.discovery_prefix, topic) else {
            return;
        };
        let Some(published) = self.devices.get_mut(unit) else {
            return;
        };
        if published.configs.contains_key(topic) {
            return;
        }
        if published.configs_from.is_none() {
            published.leftovers.insert(topic.to_string());
            return;
        }
        log::info(&format!(
            "mqtt: clearing {topic}, which no control of {} announces now",
            published.feed.borrow().name
        ));
        self.client.clear(topic.to_string());
    }
}

/// The unit id and the key a command topic, `<base>/<unit>/<key>/set`, is for, or `None` for
/// any other topic.
pub(super) fn command_target<'t>(base: &str, topic: &'t str) -> Option<(&'t str, &'t str)> {
    let rest = topic.strip_prefix(base)?.strip_prefix('/')?;
    match rest.split('/').collect::<Vec<_>>().as_slice() {
        [unit, key, "set"] => Some((unit, key)),
        _ => None,
    }
}

/// The value a `set` request carries for a parsed command: `on`/`off` for a toggle, the raw
/// number — the step a level snapped to, the value a select's label names — for the rest.
pub(super) fn set_value(value: &Value) -> String {
    match value {
        Value::Bool(on) => match on {
            true => "on".to_string(),
            false => "off".to_string(),
        },
        other => other
            .as_i64()
            .map_or_else(|| other.to_string(), |raw| raw.to_string()),
    }
}

/// The log lines for what a recall's plan moved to a step the device has.
pub(super) fn clamp_lines(device: &str, recalled: &recall::Recalled) -> Vec<String> {
    recalled
        .recall
        .plan
        .clamped
        .iter()
        .map(|clamp| format!("mqtt: {device}: preset {}: {clamp}", recalled.preset.name))
        .collect()
}

/// The log line for the end of an identify press: a button has no state to carry the answer
/// back to Home Assistant, so the log says it.
pub(super) fn identify_line(device: &str, sent: bool) -> String {
    match sent {
        true => format!("mqtt: {device}: identify sent"),
        false => {
            format!("mqtt: {device}: identify not sent: one went out within the last two seconds")
        }
    }
}

/// The actor command a write is, as the device's writer sends it.
fn command(request: WriteRequest) -> Command {
    match request {
        WriteRequest::Set { key, value } => Command::Set {
            key,
            value,
            step: 1,
            no_wait: false,
        },
        WriteRequest::EqMode { mode } => Command::EqMode {
            mode,
            no_wait: false,
        },
        WriteRequest::EqBand { band, param, value } => Command::EqSet {
            band,
            param: param.to_string(),
            value,
            no_wait: false,
        },
        WriteRequest::Loudness { key, id, raw } => {
            recall::command(&preset::EqCommand::Loudness { key, id, raw })
        }
        WriteRequest::Identify => Command::Identify,
        WriteRequest::Slot { slot } => Command::Set {
            key: roles::RECALL_KEY.to_string(),
            value: slot.to_string(),
            step: 1,
            no_wait: false,
        },
        WriteRequest::Preset { .. } | WriteRequest::Setting { .. } => {
            unreachable!("a preset or a setting is sent as a batch, not a command")
        }
    }
}

/// How a glide began, as the end of a write: a climb that started is a write that went through.
fn glided(glided: Result<Glided, oneshot::error::RecvError>, device: &str) -> Outcome {
    match glided {
        Ok(Glided::Landed(response) | Glided::Refused(response)) => Outcome::Done(response),
        Ok(Glided::Climbing { first, target }) => {
            log::debug(&format!(
                "mqtt: {device}: the volume climbs to step {target}"
            ));
            Outcome::Done(first.unwrap_or(Response::Ack))
        }
        Ok(Glided::Superseded) => Outcome::Superseded,
        Err(_) => Outcome::Done(not_running(device)),
    }
}

/// How a write ended, or that its writer is gone.
fn ended(outcome: Result<Outcome, oneshot::error::RecvError>, device: &str) -> Outcome {
    outcome.unwrap_or_else(|_| Outcome::Done(not_running(device)))
}
