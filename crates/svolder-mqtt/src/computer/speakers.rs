// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's virtual speakers over MQTT: an ON/OFF switch and a select of the fetched sets
//! (a binary sensor and a sensor when commands are off), on the computer's Home Assistant device
//! beside the EQ's and the crossfeed's entities.
//!
//! The select's options are the fetched sets, one per set, each as `<label> · <room name> ·
//! <term>` ("SADIE II, D2 · No room · HRTF"), taken from the view and never from a copy; a label
//! two sets share gets ` (<id>)` after it so the options stay distinct. The select's state is the
//! chosen set's option, kept while the speakers are off. Choosing an option sends it to the
//! command topic, which picks the set (one rebuild of the chain while the speakers play); it does
//! not turn the speakers on. The switch goes `ON` only while they play. Nothing is fetched,
//! imported or forgotten from here: the entities offer what is already on disk.
//!
//! The chosen set's licence, credit line, source page, measurement angles and loudness change
//! are the select's attributes (`json_attributes_topic`), so a dashboard can show the licence
//! wherever the set is chosen.

use serde_json::{Value as Json, json};
use svolder_host::eq::SpeakersView;
use svolder_host::eq::speakers::SetView;

use super::{Computer, OFF, ON, envelope};
use crate::discovery::DiscoveryEntry;
use crate::topics;

/// The speakers switch's key.
pub const SWITCH_KEY: &str = "computer_speakers";

/// The set select's key.
pub const SET_KEY: &str = "computer_speakers_set";

/// Between the parts of an option.
const SEPARATOR: &str = " · ";

/// What the entities say of the speakers: the switch, the chosen set's option (empty with no
/// fetched set chosen) and the chosen set's attributes as JSON text (empty with none). Read the
/// payloads to publish through [`State::set_payload`] and [`State::attributes_payload`]: an empty
/// retained message would not reset Home Assistant's select or attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// `ON` while they play, else `OFF`.
    pub on: &'static str,
    /// The chosen set's option; empty when none is chosen or the chosen one is not fetched.
    pub set: String,
    /// The chosen set's attributes; empty with no set.
    pub attributes: String,
}

impl State {
    /// What the select's state topic carries: the option, or `None` (Home Assistant's reset to
    /// unknown) when no fetched set is chosen.
    pub fn set_payload(&self) -> &str {
        match self.set.is_empty() {
            true => "None",
            false => &self.set,
        }
    }

    /// What the attributes topic carries: the attributes, or `{}` (Home Assistant's reset) when
    /// no fetched set is chosen.
    pub fn attributes_payload(&self) -> &str {
        match self.attributes.is_empty() {
            true => "{}",
            false => &self.attributes,
        }
    }
}

/// The topic of the select's attributes, `<base>/computer_<host>/computer_speakers_set/attributes`.
pub fn attributes_topic(computer: &Computer, base_topic: &str) -> String {
    format!("{base_topic}/{}/{SET_KEY}/attributes", computer.unit())
}

/// The discovery topics of the two entities, for `commands`, whatever the view holds.
pub(super) fn topics_of(
    computer: &Computer,
    discovery_prefix: &str,
    commands: bool,
) -> Vec<String> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    let (switch, set) = match commands {
        true => ("switch", "select"),
        false => ("binary_sensor", "sensor"),
    };
    vec![
        topics::discovery_config(discovery_prefix, switch, computer.unit(), SWITCH_KEY),
        topics::discovery_config(discovery_prefix, set, computer.unit(), SET_KEY),
    ]
}

/// The two entries, for a computer whose controller serves the speakers (`view` is their view).
pub(super) fn entries(
    computer: &Computer,
    base_topic: &str,
    discovery_prefix: &str,
    commands: bool,
    view: &SpeakersView,
) -> Vec<DiscoveryEntry> {
    let mut switch = envelope(computer, base_topic, "Virtual speakers", SWITCH_KEY);
    switch.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), SWITCH_KEY).into(),
    );
    switch.insert("payload_on".into(), ON.into());
    switch.insert("payload_off".into(), OFF.into());
    let mut set = envelope(computer, base_topic, "Virtual speakers set", SET_KEY);
    set.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), SET_KEY).into(),
    );
    set.insert(
        "json_attributes_topic".into(),
        attributes_topic(computer, base_topic).into(),
    );
    if commands {
        switch.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), SWITCH_KEY).into(),
        );
        set.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), SET_KEY).into(),
        );
        set.insert(
            "options".into(),
            Json::Array(options(&view.sets).into_iter().map(Json::from).collect()),
        );
    }
    let topics = topics_of(computer, discovery_prefix, commands);
    vec![
        DiscoveryEntry {
            topic: topics[0].clone(),
            payload: Json::Object(switch),
        },
        DiscoveryEntry {
            topic: topics[1].clone(),
            payload: Json::Object(set),
        },
    ]
}

/// The options for `sets`, in the view's order: a label two sets share is told apart by the id.
pub fn options(sets: &[SetView]) -> Vec<String> {
    sets.iter()
        .map(|set| {
            let base = plain(set);
            match sets.iter().filter(|other| plain(other) == base).count() {
                1 => base,
                _ => format!("{base} ({})", set.id),
            }
        })
        .collect()
}

/// A set's option before any id is added.
fn plain(set: &SetView) -> String {
    format!(
        "{}{SEPARATOR}{}{SEPARATOR}{}",
        set.label, set.room_name, set.term
    )
}

/// What the entities say for `view`.
pub fn state(view: &SpeakersView) -> State {
    let chosen = view
        .set
        .as_deref()
        .and_then(|id| view.sets.iter().position(|set| set.id == id));
    let Some(at) = chosen else {
        return State {
            on: if view.on { ON } else { OFF },
            set: String::new(),
            attributes: String::new(),
        };
    };
    let attributes = json!({
        "set": view.set,
        "room": view.room_name,
        "term": view.term,
        "licence": view.licence,
        "credit": view.credit,
        "source_url": view.source_url,
        "angles": view.angles,
        "loudness_db": view.loudness_db,
        "cut": view.cut,
    });
    State {
        on: if view.on { ON } else { OFF },
        set: options(&view.sets).swap_remove(at),
        attributes: attributes.to_string(),
    }
}

/// The id of the set a command on the select names, against the view's options, in any case,
/// blanks aside; `None` for text that names no fetched set.
pub fn parse_set(view: &SpeakersView, body: &str) -> Option<String> {
    let body = body.trim();
    let options = options(&view.sets);
    // The exact text first: two labels that differ only in case are two sets.
    options
        .iter()
        .position(|option| option == body)
        .or_else(|| {
            options
                .iter()
                .position(|option| option.eq_ignore_ascii_case(body))
        })
        .map(|at| view.sets[at].id.clone())
}
