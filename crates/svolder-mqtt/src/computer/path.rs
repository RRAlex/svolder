// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the computer's path to the DAC does to the sound, over MQTT: a read-only sensor for the
//! path's verdict and a read-only problem sensor for the PipeWire config, on the computer's Home
//! Assistant device. Neither takes a command.
//!
//! The path sensor reads one of [`STATES`]; its attributes are the path's `words` (a list), its
//! `sentence` and the `dac` it was judged against. The config sensor is on while a line of the
//! PipeWire config check fails and off while every line passes; while the config was not fully
//! read it says `None` (Home Assistant's reset to unknown), never off, and never an empty
//! payload, which Home Assistant ignores and so keeps the last state. Its attributes are
//! the check's summary, the sink it judged and each line. The judging itself is the IPC crate's:
//! this module only words what that judgement says.

use serde_json::{Value as Json, json};

use super::{OFF, ON, UNIT, envelope};
use crate::discovery::DiscoveryEntry;
use crate::topics;

/// The path sensor's key.
pub const PATH_KEY: &str = "computer_path";

/// The config sensor's key.
pub const CONFIG_KEY: &str = "computer_config";

/// The path is known to leave the sound untouched.
pub const UNTOUCHED: &str = "untouched";

/// The path changes the sound.
pub const CHANGED: &str = "changed";

/// Nothing is known to change the sound, but something could not be read.
pub const UNKNOWN: &str = "unknown";

/// The sound goes to another sink, not to the DAC.
pub const NOT_THIS_DAC: &str = "not_this_dac";

/// The DAC listens to an input the computer does not feed.
pub const OTHER_INPUT: &str = "other_input";

/// The route has not been read.
pub const NOT_READ: &str = "not_read";

/// Every state the path sensor can read: the keys of the IPC crate's verdicts, and `not_read`
/// while there is no route.
pub const STATES: [&str; 6] = [
    UNTOUCHED,
    CHANGED,
    NOT_THIS_DAC,
    OTHER_INPUT,
    UNKNOWN,
    NOT_READ,
];

/// The two entities' discovery topics, whatever the view holds. They are read-only, so `commands`
/// changes nothing.
pub(super) fn topics_of(discovery_prefix: &str) -> Vec<String> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    vec![
        topics::discovery_config(discovery_prefix, "sensor", UNIT, PATH_KEY),
        topics::discovery_config(discovery_prefix, "binary_sensor", UNIT, CONFIG_KEY),
    ]
}

/// The topic of the path sensor's attributes, `<base>/computer/computer_path/attributes`.
pub fn path_attributes_topic(base_topic: &str) -> String {
    format!("{base_topic}/{UNIT}/{PATH_KEY}/attributes")
}

/// The topic of the config sensor's attributes, `<base>/computer/computer_config/attributes`.
pub fn config_attributes_topic(base_topic: &str) -> String {
    format!("{base_topic}/{UNIT}/{CONFIG_KEY}/attributes")
}

/// The two entries.
pub(super) fn entries(base_topic: &str, discovery_prefix: &str) -> Vec<DiscoveryEntry> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    let mut path = envelope(base_topic, "Path", PATH_KEY);
    path.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, UNIT, PATH_KEY).into(),
    );
    path.insert(
        "json_attributes_topic".into(),
        path_attributes_topic(base_topic).into(),
    );
    path.insert("device_class".into(), "enum".into());
    path.insert("options".into(), json!(STATES));
    let mut config = envelope(base_topic, "PipeWire config", CONFIG_KEY);
    config.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, UNIT, CONFIG_KEY).into(),
    );
    config.insert(
        "json_attributes_topic".into(),
        config_attributes_topic(base_topic).into(),
    );
    config.insert("device_class".into(), "problem".into());
    config.insert("payload_on".into(), ON.into());
    config.insert("payload_off".into(), OFF.into());
    let topics = topics_of(discovery_prefix);
    vec![
        DiscoveryEntry {
            topic: topics[0].clone(),
            payload: Json::Object(path),
        },
        DiscoveryEntry {
            topic: topics[1].clone(),
            payload: Json::Object(config),
        },
    ]
}

/// What the path sensor says: one of [`STATES`] and its attributes as JSON text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathState {
    /// The state topic's payload.
    pub state: &'static str,
    /// The attributes topic's payload.
    pub attributes: String,
}

/// The path sensor's say for `state` (one of [`STATES`]) with the path's short `words` (one per
/// change or thing not read), its whole `sentence` when it has one and the `dac` it was judged
/// against, when there is one.
pub fn path_state(
    state: &'static str,
    words: &[String],
    sentence: Option<&str>,
    dac: Option<&str>,
) -> PathState {
    PathState {
        state,
        attributes: json!({"words": words, "sentence": sentence, "dac": dac}).to_string(),
    }
}

/// One line of the config check as the attributes carry it.
#[derive(Debug, Clone, Copy)]
pub struct ConfigLine<'a> {
    /// The line's stable key (`rates`, `filter`, ...).
    pub key: &'a str,
    /// `pass`, `fail` or `unknown`.
    pub outcome: &'a str,
    /// What was found, in words.
    pub words: &'a str,
}

/// What the config sensor says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigState {
    /// `ON` while a line fails, `OFF` while all pass, `None` while it was not fully read or not
    /// judged. Publish it through [`ConfigState::payload`].
    pub state: Option<&'static str>,
    /// The attributes topic's payload.
    pub attributes: String,
}

impl ConfigState {
    /// What the state topic carries: `ON`, `OFF`, or the text `None` (Home Assistant's reset to
    /// unknown) while there is no state. An empty retained payload would be ignored and Home
    /// Assistant would keep showing the last state.
    pub fn payload(&self) -> &'static str {
        self.state.unwrap_or("None")
    }
}

/// The config sensor's say: `problem` is whether a line fails, `None` while a line could not be
/// read and none fails (or nothing was read); `outcome` is the check's summary key, `summary`,
/// `sink` and `limit` are its own words, `lines` its lines.
pub fn config_state(
    problem: Option<bool>,
    outcome: &str,
    summary: &str,
    sink: &str,
    limit: &str,
    lines: &[ConfigLine<'_>],
) -> ConfigState {
    let lines: Vec<Json> = lines
        .iter()
        .map(|l| json!({"key": l.key, "outcome": l.outcome, "words": l.words}))
        .collect();
    ConfigState {
        state: problem.map(|on| if on { ON } else { OFF }),
        attributes: json!({
            "outcome": outcome,
            "summary": summary,
            "sink": sink,
            "limit": limit,
            "lines": lines,
        })
        .to_string(),
    }
}

/// The config sensor's say while it is not judged: unknown, with empty attributes.
pub fn config_unjudged() -> ConfigState {
    ConfigState {
        state: None,
        attributes: "{}".to_string(),
    }
}
