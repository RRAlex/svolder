// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's EQ on the bridge: following the host's view, announcing its switch and preset
//! sensor, publishing their states, and turning the EQ on or off when the switch is commanded.
//!
//! Nothing is announced while the host reports no EQ at all (host audio off): what an earlier
//! announcement left is cleared, once the host has had [`START_SETTLES`] from the start to report
//! one. While it reports the computer's controls unavailable the
//! entities stay and the computer's availability says `offline`. Home Assistant is only ever
//! told what the view says: an EQ whose chain the daemon could not verify reads as the saved
//! state says.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use svolder_ipc::host::HostView;
use svolder_mqtt::{computer, payload, topics};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::{BridgeTask, ComputerLink, WriteOutcome};
use crate::host::EqSwitch;
use crate::log;

/// How long a start waits for the host to report the computer's EQ before it takes down what an
/// earlier run left: the host's first view comes a moment after the bridge's, and clearing an
/// entity that is about to be announced makes Home Assistant remove and add it again.
pub(super) const START_SETTLES: Duration = Duration::from_secs(3);

/// What the bridge keeps of the computer's EQ.
pub(super) struct Published {
    pub(super) view: watch::Receiver<HostView>,
    pub(super) switch: Arc<dyn EqSwitch>,
    /// Tells the task each time the view changes.
    watcher: JoinHandle<()>,
    /// Whether the view changed since it was last published.
    pub(super) dirty: bool,
    /// The discovery configs announced, by topic.
    pub(super) configs: BTreeMap<String, String>,
    /// The availability last published: `true` for `online`.
    pub(super) available: Option<bool>,
    /// The switch's state and the preset sensor's last published.
    pub(super) state: Option<(&'static str, String)>,
    /// Whether what an earlier run left retained has been cleared.
    pub(super) swept: bool,
    /// When the bridge began following the computer.
    since: Instant,
}

impl Drop for Published {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

impl BridgeTask {
    /// Follow the computer's EQ through `link`, from the first publish on, and take its commands
    /// when commands are on.
    pub(super) fn follow_computer(&mut self, link: ComputerLink) {
        let watcher = tokio::spawn(watch_view(link.view.clone(), self.changes_tx.clone()));
        if self.config.commands {
            self.client.subscribe(topics::command(
                &self.config.base_topic,
                computer::UNIT,
                "+",
            ));
        }
        self.computer = Some(Published {
            view: link.view,
            switch: link.switch,
            watcher,
            dirty: true,
            configs: BTreeMap::new(),
            available: None,
            state: None,
            swept: false,
            since: Instant::now(),
        });
        self.changed(computer::UNIT);
    }

    /// Publish what changed for the computer: the entities, their availability and their states.
    pub(super) fn publish_computer(&mut self) {
        let base = self.config.base_topic.clone();
        let prefix = self.config.discovery_prefix.clone();
        let commands = self.config.commands;
        let client = &self.client;
        let Some(published) = self.computer.as_mut().filter(|c| c.dirty) else {
            return;
        };
        published.dirty = false;
        let view = published.view.borrow().clone();
        let eq = view.eq.as_ref().filter(|_| view.available);
        let availability = topics::device_availability(&base, computer::UNIT);
        let state_topic = topics::diagnostic(&base, computer::UNIT, computer::EQ_KEY);
        let preset_topic = topics::diagnostic(&base, computer::UNIT, computer::PRESET_KEY);

        // The first publish takes down what an earlier run left retained, whichever entities it
        // announced and whether or not this run serves the computer. While the host has not yet
        // reported an EQ it waits, so a computer about to be announced is never cleared first;
        // a host that reports none within [`START_SETTLES`] is swept then.
        if !published.swept && eq.is_none() {
            let due = published.since + START_SETTLES;
            if Instant::now() < due {
                published.dirty = true;
                self.flush_at = Some(self.flush_at.map_or(due, |at| at.min(due)));
                return;
            }
        }
        if !published.swept {
            published.swept = true;
            let wanted: Vec<String> = match eq {
                Some(_) => computer::entries(&base, &prefix, commands)
                    .into_iter()
                    .map(|entry| entry.topic)
                    .collect(),
                None => Vec::new(),
            };
            for with_commands in [true, false] {
                for entry in computer::entries(&base, &prefix, with_commands) {
                    if !wanted.contains(&entry.topic) {
                        client.clear(entry.topic);
                    }
                }
            }
            if eq.is_none() {
                client.clear(availability.clone());
                client.clear(state_topic.clone());
                client.clear(preset_topic.clone());
            }
        }

        let Some(eq) = eq else {
            // No computer EQ: take down what was announced.
            for topic in published.configs.keys() {
                client.clear(topic.clone());
            }
            if published.available.is_some() || !published.configs.is_empty() {
                client.clear(availability);
                client.clear(state_topic);
                client.clear(preset_topic);
            }
            published.configs.clear();
            published.available = None;
            published.state = None;
            return;
        };

        let next: BTreeMap<String, String> = computer::entries(&base, &prefix, commands)
            .into_iter()
            .map(|entry| (entry.topic, entry.payload.to_string()))
            .collect();
        for (topic, body) in &next {
            if published.configs.get(topic) != Some(body) {
                client.announce(topic.clone(), body.clone());
            }
        }
        for topic in published.configs.keys().filter(|t| !next.contains_key(*t)) {
            client.clear(topic.clone());
        }
        published.configs = next;

        let usable = eq.unavailable.is_none();
        if published.available != Some(usable) {
            let word = match usable {
                true => payload::ONLINE,
                false => payload::OFFLINE,
            };
            client.retain(availability, word);
            published.available = Some(usable);
        }
        if !usable {
            return;
        }
        let (on, preset) = computer::state(Some(eq));
        if published.state.as_ref() != Some(&(on, preset.clone())) {
            let kept_preset = published.state.as_ref().map(|(_, p)| p.clone());
            client.retain(state_topic, on);
            if preset.is_empty() {
                client.clear(preset_topic);
            } else if kept_preset.as_deref() != Some(preset.as_str()) {
                client.retain(preset_topic, preset.clone());
            }
            published.state = Some((on, preset));
        }
    }

    /// A command on one of the computer's topics: only the switch takes one, `ON` or `OFF`. ON
    /// with nothing kept to apply leaves the EQ off.
    pub(super) fn computer_command(&mut self, key: &str, body: &[u8]) {
        let drop = |why: &str| {
            log::warn(&format!(
                "mqtt: This computer: a command for {key} dropped: {why}"
            ))
        };
        let Some(published) = self.computer.as_ref() else {
            return;
        };
        if !key.eq_ignore_ascii_case(computer::EQ_KEY) {
            return drop("nothing of the computer but its EQ switch takes commands");
        }
        let Ok(text) = std::str::from_utf8(body) else {
            return drop("the payload is not text");
        };
        let Some(on) = computer::parse(text) else {
            return drop(&format!("{:?} is not ON or OFF", text.trim()));
        };
        let view = published.view.borrow().clone();
        let Some(eq) = view.eq.as_ref().filter(|_| view.available) else {
            return drop("computer controls unavailable: host audio is off");
        };
        if let Some(why) = &eq.unavailable {
            return drop(why);
        }
        if on && !eq.on && computer::nothing_kept(eq) {
            log::info(
                "mqtt: This computer: ON left the EQ off: no EQ points are saved yet; set the \
                 EQ up first",
            );
            return;
        }
        let switch = Arc::clone(&published.switch);
        self.writes
            .spawn(async move { WriteOutcome::Computer(on, switch.set_on(on).await) });
    }

    /// The end of a switch: say so when it was refused.
    pub(super) fn computer_written(&mut self, on: bool, result: Result<(), String>) {
        if let Err(why) = result {
            log::warn(&format!(
                "mqtt: This computer: turning the EQ {} failed: {why}",
                if on { "on" } else { "off" }
            ));
        }
        // The state is whatever the view says now.
        self.changed(computer::UNIT);
    }

    /// Say `offline` for the computer on the way out, when it was announced.
    pub(super) fn computer_farewell(&self) {
        if self
            .computer
            .as_ref()
            .is_some_and(|c| c.available.is_some())
        {
            self.client.retain(
                topics::device_availability(&self.config.base_topic, computer::UNIT),
                payload::OFFLINE,
            );
        }
    }
}

/// Tell the bridge each time the view changes, until the view or the bridge goes.
async fn watch_view(mut view: watch::Receiver<HostView>, changes: mpsc::UnboundedSender<String>) {
    while view.changed().await.is_ok() {
        if changes.send(computer::UNIT.to_string()).is_err() {
            return;
        }
    }
}
