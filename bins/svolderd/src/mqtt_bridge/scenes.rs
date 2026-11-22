// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scenes on the bridge: the select (a sensor with commands off) that shows and recalls the
//! scene that is on, and the text that saves the current settings under a typed name, both on the
//! computer's Home Assistant device ([`svolder_mqtt::computer::scenes`]).
//!
//! The bridge follows the daemon's [`ScenesView`] and judges nothing: the names, which scene is
//! on and whether that is known come from it. It announces the pair whether or not the host has
//! an EQ (the computer's device exists whenever the bridge runs, so every DAC's `via_device`
//! points at a device), as the computer's parent rank so a device is never announced before it.
//! While the view is not `known` the select's state stays as it was last published: a read after
//! a reconnect never flips it to `custom`.
//!
//! A command is one `Request::Scene` as the MQTT client's: the select recalls the scene it names
//! (never `custom`, never a retained command), the text saves under its name without `force`, so
//! a taken name is refused in the daemon's words, which the select's `saved` attribute carries.
//! The last recall's answer goes in `outcome`. Rename and delete are not here. A recall that
//! would play one preset on a device and on the computer at once is asked, and a select cannot
//! answer: nothing changes, and the daemon's question is logged and goes in `outcome` as its
//! `error`, as does the daemon's refusal of any recall. Each answer carries a `seq` above every
//! earlier one, so repeating an answer still changes the attributes.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value as Json, json};
use svolder_ipc::Response;
use svolder_ipc::socket::protocol::{PartOutcome, PartStatus, SceneOp, ScenesView};
use svolder_mqtt::computer::scenes;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::{BridgeTask, SceneDesk, ScenesLink, WriteOutcome};
use crate::log;

/// What the bridge's changes channel carries when the scenes' view changed: a unit id never has
/// this form.
pub(super) const CHANGED: &str = "scenes";

/// What the bridge keeps of the scenes.
pub(super) struct Published {
    pub(super) view: watch::Receiver<ScenesView>,
    pub(super) desk: Arc<dyn SceneDesk>,
    /// Tells the task each time the view changes.
    watcher: JoinHandle<()>,
    /// Whether the view or an answer changed since it was last published.
    pub(super) dirty: bool,
    /// Whether the entities of the other commands mode have been cleared.
    pub(super) swept: bool,
    /// The discovery configs announced, by topic.
    pub(super) configs: BTreeMap<String, String>,
    /// The select's state last published.
    pub(super) state: Option<String>,
    /// The attributes last published.
    pub(super) attributes: Option<String>,
    /// The last recall's answer: `{"name", "parts", "seq"}`, or `{"name", "error", "seq"}` when the
    /// daemon refused or asked.
    pub(super) outcome: Option<Json>,
    /// The last save's answer: `{"name", "seq"}` or `{"name", "error", "seq"}`.
    pub(super) saved: Option<Json>,
    /// The `seq` of the last answer kept; the next is above it.
    last_seq: u64,
}

impl Published {
    /// The `seq` of the next answer: above every earlier one, and above any a former run left
    /// retained (the clock in ms), so a repeat of an answer is a different attributes payload.
    fn next_seq(&mut self) -> u64 {
        self.last_seq = (self.last_seq + 1).max(now_ms());
        self.last_seq
    }
}

/// The wall clock in milliseconds since the epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl Drop for Published {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

/// What a scene command came back with.
pub(super) enum SceneAnswer {
    /// A recall of the scene `name`.
    Recall { name: String, answer: Response },
    /// A save under `name`.
    Save { name: String, answer: Response },
}

impl BridgeTask {
    /// Follow the daemon's scenes through `link` from the first publish on.
    pub(super) fn follow_scenes(&mut self, link: ScenesLink) {
        let watcher = tokio::spawn(watch_view(link.view.clone(), self.changes_tx.clone()));
        self.scenes = Some(Published {
            view: link.view,
            desk: link.desk,
            watcher,
            dirty: true,
            swept: false,
            configs: BTreeMap::new(),
            state: None,
            attributes: None,
            outcome: None,
            saved: None,
            last_seq: now_ms(),
        });
        self.changed(CHANGED);
    }

    /// Publish what changed of the scenes: the entities, the select's state and its attributes.
    pub(super) fn publish_scenes(&mut self) {
        let base = self.config.base_topic.clone();
        let prefix = self.config.discovery_prefix.clone();
        let commands = self.config.commands;
        let me = self.me.clone();
        let client = &self.client;
        let Some(published) = self.scenes.as_mut().filter(|s| s.dirty) else {
            return;
        };
        published.dirty = false;
        let view = published.view.borrow().clone();
        if !published.swept {
            published.swept = true;
            // An earlier run with the other commands setting left the other kind of entity.
            for topic in scenes::topics_of(&me, &prefix, !commands) {
                client.clear(topic);
            }
        }
        let names: Vec<&str> = view.scenes.iter().map(|s| s.name.as_str()).collect();
        let next: BTreeMap<String, String> = scenes::entries(&me, &base, &prefix, commands, &names)
            .into_iter()
            .map(|entry| (entry.topic, entry.payload.to_string()))
            .collect();
        for (topic, body) in &next {
            if published.configs.get(topic) != Some(body) {
                client.announce_parent(topic.clone(), body.clone());
            }
        }
        published.configs = next;

        // A scene on is only said once the daemon knows which: until then the state stays.
        if view.known {
            let state = scenes::state(view.on.as_deref());
            if published.state.as_ref() != Some(&state) {
                client.retain(topics_state(&base, &me), state.clone());
                published.state = Some(state);
            }
        }
        let attributes = scenes::attributes(
            serde_json::to_value(&view.scenes).unwrap_or(Json::Null),
            view.known,
            view.recalled.as_deref(),
            published.outcome.as_ref(),
            published.saved.as_ref(),
        );
        if published.attributes.as_ref() != Some(&attributes) {
            client.retain(scenes::attributes_topic(&me, &base), attributes.clone());
            published.attributes = Some(attributes);
        }
    }

    /// A command on the select or the text: one request to the daemon, answered through the
    /// writes set.
    pub(super) fn scene_command(&mut self, key: &str, body: &[u8]) {
        let host = self.me.name().to_string();
        let drop =
            |why: &str| log::warn(&format!("mqtt: {host}: a command for {key} dropped: {why}"));
        let Some(published) = self.scenes.as_ref() else {
            return drop("this daemon has no scenes");
        };
        let Ok(text) = std::str::from_utf8(body) else {
            return drop("the payload is not text");
        };
        let desk = Arc::clone(&published.desk);
        if key.eq_ignore_ascii_case(scenes::SELECT_KEY) {
            let view = published.view.borrow().clone();
            let names: Vec<&str> = view.scenes.iter().map(|s| s.name.as_str()).collect();
            let name = match scenes::parse_choice(&names, text) {
                None => return drop(&format!("{:?} is not a scene", text.trim())),
                Some(scenes::Choice::Custom) => {
                    return log::info(&format!(
                        "mqtt: {host}: custom is the state of no scene, not a choice; the \
                         settings stay as they are"
                    ));
                }
                Some(scenes::Choice::Scene(name)) => name,
            };
            self.writes.spawn(async move {
                let op = SceneOp::Recall {
                    name: name.clone(),
                    confirm: false,
                };
                let answer = desk.request(op).await;
                WriteOutcome::Scene(SceneAnswer::Recall { name, answer })
            });
            return;
        }
        let Some(name) = scenes::parse_save(text) else {
            return drop("a scene needs a name");
        };
        self.writes.spawn(async move {
            let op = SceneOp::Save {
                name: name.clone(),
                force: false,
            };
            let answer = desk.request(op).await;
            WriteOutcome::Scene(SceneAnswer::Save { name, answer })
        });
    }

    /// The end of a scene command: keep the answer for the select's attributes, and log what
    /// was refused or left out.
    pub(super) fn scene_answered(&mut self, answer: SceneAnswer) {
        let host = self.me.name().to_string();
        let Some(published) = self.scenes.as_mut() else {
            return;
        };
        let seq = published.next_seq();
        match answer {
            SceneAnswer::Recall { name, answer } => {
                published.outcome = Some(match answer {
                    Response::Scene { name, outcome } => {
                        for line in outcome_lines(&host, &name, &outcome) {
                            log::info(&line);
                        }
                        json!({
                            "name": name,
                            "parts": serde_json::to_value(&outcome).unwrap_or(Json::Null),
                            "seq": seq,
                        })
                    }
                    Response::Ask { message, .. } => {
                        log::warn(&format!(
                            "mqtt: {host}: recalling {name} was not done: {message}; a select \
                             cannot answer that question, nothing changed"
                        ));
                        json!({ "name": name, "error": message, "seq": seq })
                    }
                    Response::Error { message, .. } => {
                        log::warn(&format!("mqtt: {host}: recalling {name} failed: {message}"));
                        json!({ "name": name, "error": message, "seq": seq })
                    }
                    other => {
                        let words = format!("an unexpected answer: {other:?}");
                        log::warn(&format!("mqtt: {host}: recalling {name} got {words}"));
                        json!({ "name": name, "error": words, "seq": seq })
                    }
                });
            }
            SceneAnswer::Save { name, answer } => {
                published.saved = Some(match answer {
                    Response::Scene { name, .. } => json!({ "name": name, "seq": seq }),
                    Response::Error { message, .. } => {
                        log::warn(&format!("mqtt: {host}: saving {name} failed: {message}"));
                        json!({ "name": name, "error": message, "seq": seq })
                    }
                    other => {
                        let words = format!("an unexpected answer: {other:?}");
                        log::warn(&format!("mqtt: {host}: saving {name} got {words}"));
                        json!({ "name": name, "error": words, "seq": seq })
                    }
                });
            }
        }
        published.dirty = true;
        self.schedule_flush();
    }
}

/// The select's state topic.
fn topics_state(base: &str, me: &svolder_mqtt::computer::Computer) -> String {
    svolder_mqtt::topics::diagnostic(base, me.unit(), scenes::SELECT_KEY)
}

/// The log lines a recall's answer owes: one for each part that was not set whole.
fn outcome_lines(host: &str, name: &str, outcome: &[PartOutcome]) -> Vec<String> {
    outcome
        .iter()
        .filter(|part| part.status != PartStatus::Done)
        .map(|part| {
            format!(
                "mqtt: {host}: scene {name}: {} {:?}: {}",
                part.who,
                part.status,
                part.notes.join("; ")
            )
        })
        .collect()
}

/// Tell the bridge each time the view changes, until the view or the bridge goes.
async fn watch_view(mut view: watch::Receiver<ScenesView>, changes: mpsc::UnboundedSender<String>) {
    while view.changed().await.is_ok() {
        if changes.send(CHANGED.to_string()).is_err() {
            return;
        }
    }
}
