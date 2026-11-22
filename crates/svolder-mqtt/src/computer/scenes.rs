// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scenes over MQTT: a select to recall one and a text to save the current settings under a
//! typed name (a sensor, and no text, when commands are off), on the computer's Home Assistant
//! device beside the EQ's entities.
//!
//! The select's options are the scene names the daemon keeps and [`CUSTOM`]; its state is the
//! scene that is on, else `custom`. Choosing an option recalls that scene; `custom` is the state
//! of no scene and is no choice. While the daemon has not settled which scene is on the state is
//! left as it was last published, so a reconnect read never flips it to `custom`. The select's
//! attributes (`json_attributes_topic`) carry what a dashboard card shows: `scenes` (each with
//! its parts), `known`, `recalled`, and, once there is one, `outcome` (the last recall this
//! bridge made) and `saved` (the last save through the text entity, with the daemon's words when
//! it was refused).
//!
//! The text takes a name: the current settings are saved under it, and a name a scene already has
//! is refused in the daemon's words. The text asks `min` 1 and `max` [`NAME_MAX`] (the daemon's
//! name length) and no `pattern`: the rest of the daemon's name rule (no control character) does
//! not fit one, and the daemon answers a name it refuses in words. Scenes are recalled and saved
//! from here, never renamed or deleted: those are for the front ends of the config.
//!
//! The pair belongs to the computer whether or not the host has an EQ, so its entries are not
//! part of [`super::entries_of`]; their availability is the bridge's alone.

use serde_json::{Map, Value as Json, json};
use svolder_profiles::tone::preset::{NAME_MAX, same_name};

use super::{Computer, envelope};
use crate::discovery::DiscoveryEntry;
use crate::topics;

/// The select's key (a sensor's, with commands off).
pub const SELECT_KEY: &str = "scene";

/// The text's key.
pub const SAVE_KEY: &str = "scene_save";

/// The select's state, and last option, when no scene is on.
pub const CUSTOM: &str = super::CUSTOM;

/// The topic of the select's attributes, `<base>/computer_<host>/scene/attributes`.
pub fn attributes_topic(computer: &Computer, base_topic: &str) -> String {
    format!("{base_topic}/{}/{SELECT_KEY}/attributes", computer.unit())
}

/// The discovery topics of the entities for `commands`: the select and the text, or the sensor.
pub fn topics_of(computer: &Computer, discovery_prefix: &str, commands: bool) -> Vec<String> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    match commands {
        true => vec![
            topics::discovery_config(discovery_prefix, "select", computer.unit(), SELECT_KEY),
            topics::discovery_config(discovery_prefix, "text", computer.unit(), SAVE_KEY),
        ],
        false => vec![topics::discovery_config(
            discovery_prefix,
            "sensor",
            computer.unit(),
            SELECT_KEY,
        )],
    }
}

/// The options for the scene `names`: the names in order, then [`CUSTOM`] (left out when a scene
/// already has that name, so an option is never listed twice).
pub fn options<S: AsRef<str>>(names: &[S]) -> Vec<String> {
    let mut all: Vec<String> = names.iter().map(|n| n.as_ref().to_string()).collect();
    if !all.iter().any(|name| same_name(name, CUSTOM)) {
        all.push(CUSTOM.to_string());
    }
    all
}

/// The entries: the select and the text with commands on, the sensor with them off. `names` are
/// the scene names the daemon keeps now. An empty `discovery_prefix` turns discovery off.
pub fn entries<S: AsRef<str>>(
    computer: &Computer,
    base_topic: &str,
    discovery_prefix: &str,
    commands: bool,
    names: &[S],
) -> Vec<DiscoveryEntry> {
    let topics = topics_of(computer, discovery_prefix, commands);
    if topics.is_empty() {
        return Vec::new();
    }
    let mut select = scene_envelope(computer, base_topic, "Scene", SELECT_KEY);
    select.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), SELECT_KEY).into(),
    );
    select.insert(
        "json_attributes_topic".into(),
        attributes_topic(computer, base_topic).into(),
    );
    let mut all = Vec::new();
    if commands {
        select.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), SELECT_KEY).into(),
        );
        select.insert(
            "options".into(),
            Json::Array(options(names).into_iter().map(Json::from).collect()),
        );
        let mut text = scene_envelope(computer, base_topic, "Scene save", SAVE_KEY);
        text.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), SAVE_KEY).into(),
        );
        text.insert("mode".into(), "text".into());
        text.insert("min".into(), 1.into());
        text.insert("max".into(), NAME_MAX.into());
        all.push(DiscoveryEntry {
            topic: topics[1].clone(),
            payload: Json::Object(text),
        });
    }
    all.insert(
        0,
        DiscoveryEntry {
            topic: topics[0].clone(),
            payload: Json::Object(select),
        },
    );
    all
}

/// The computer's envelope with the availability of the bridge alone: the scenes do not wait for
/// the computer's EQ.
fn scene_envelope(
    computer: &Computer,
    base_topic: &str,
    name: &str,
    key: &str,
) -> Map<String, Json> {
    let mut m = envelope(computer, base_topic, name, key);
    m.insert(
        "availability".into(),
        json!([{"topic": topics::bridge_availability(base_topic, computer.id())}]),
    );
    m
}

/// What the select's state topic carries: the scene that is on, else [`CUSTOM`].
pub fn state(on: Option<&str>) -> String {
    on.unwrap_or(CUSTOM).to_string()
}

/// What a command on the select names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// `custom`: a state, not a choice; nothing is recalled.
    Custom,
    /// The scene, spelled as the daemon keeps it.
    Scene(String),
}

/// The scene a command on the select names, against the scene `names`: the exact text first,
/// then the same name by the daemon's rule (blanks and case aside). `None` for text that names
/// none.
pub fn parse_choice<S: AsRef<str>>(names: &[S], body: &str) -> Option<Choice> {
    let body = body.trim();
    let found = names
        .iter()
        .map(AsRef::as_ref)
        .find(|name| *name == body)
        .or_else(|| names.iter().map(AsRef::as_ref).find(|n| same_name(n, body)));
    match found {
        Some(name) => Some(Choice::Scene(name.to_string())),
        None if same_name(body, CUSTOM) => Some(Choice::Custom),
        None => None,
    }
}

/// The name a command on the text carries: the text without the blanks round it, `None` when
/// nothing is left. Whether a name may be a scene's is the daemon's to say.
pub fn parse_save(body: &str) -> Option<String> {
    let name = body.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The JSON the select's attributes carry. `scenes` is each scene with its parts as the daemon's
/// view lists them, `recalled` the scene last recalled, `outcome` the last recall as
/// `{"name", "parts", "seq"}` (or `{"name", "error", "seq"}` when the daemon refused or asked)
/// and `saved` the last save as `{"name", "seq"}` or `{"name", "error", "seq"}`; the last two are
/// left out until there is one. The caller makes `seq` rise with each answer, so a repeated
/// answer is still a changed payload.
pub fn attributes(
    scenes: Json,
    known: bool,
    recalled: Option<&str>,
    outcome: Option<&Json>,
    saved: Option<&Json>,
) -> String {
    let mut m = Map::new();
    m.insert("scenes".into(), scenes);
    m.insert("known".into(), known.into());
    m.insert("recalled".into(), recalled.into());
    if let Some(outcome) = outcome {
        m.insert("outcome".into(), outcome.clone());
    }
    if let Some(saved) = saved {
        m.insert("saved".into(), saved.clone());
    }
    Json::Object(m).to_string()
}

#[cfg(test)]
mod tests;
