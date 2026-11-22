// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's crossfeed over MQTT: an ON/OFF switch and a preset select (a read-only sensor
//! when commands are off), on the computer's Home Assistant device beside the EQ's entities.
//!
//! The select's options come from the view's model table, never from a copy: every preset of
//! every model as `<Model> · <preset>` (a model with one preset shows the model alone), then
//! [`CUSTOM`]. `Custom` is a state only. Choosing it in Home Assistant sends it to the command
//! topic, where it is dropped and changes nothing; the select goes on showing what the state
//! topic last said, which is an option the select holds, so it never reads as unknown. A
//! crossfeed that turned itself off (a reset) is no entity of its own: the switch goes `OFF`.

use serde_json::Value as Json;
use svolder_host::eq::CrossfeedView;
use svolder_host::eq::crossfeed::ModelView;

use super::{Computer, OFF, ON, envelope};
use crate::discovery::DiscoveryEntry;
use crate::topics;

/// The crossfeed switch's key.
pub const SWITCH_KEY: &str = "computer_crossfeed";

/// The crossfeed preset entity's key.
pub const PRESET_KEY: &str = "computer_crossfeed_preset";

/// The option for settings no preset stands for; a state, never a command.
pub const CUSTOM: &str = "Custom";

/// Between a model's name and a preset's in an option.
const SEPARATOR: &str = " · ";

/// What a command on the preset select asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// `Custom`: no command, the settings stay as they are.
    Custom,
    /// A preset of a model: the model's wire word and the preset's name.
    Preset {
        /// The model's wire word.
        model: String,
        /// The preset's name within the model.
        preset: String,
    },
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
    vec![
        topics::discovery_config(
            discovery_prefix,
            switch_component(commands),
            computer.unit(),
            SWITCH_KEY,
        ),
        topics::discovery_config(
            discovery_prefix,
            preset_component(commands),
            computer.unit(),
            PRESET_KEY,
        ),
    ]
}

fn switch_component(commands: bool) -> &'static str {
    if commands { "switch" } else { "binary_sensor" }
}

fn preset_component(commands: bool) -> &'static str {
    if commands { "select" } else { "sensor" }
}

/// The two entries, for a computer whose controller serves crossfeed (`view` is its view).
pub(super) fn entries(
    computer: &Computer,
    base_topic: &str,
    discovery_prefix: &str,
    commands: bool,
    view: &CrossfeedView,
) -> Vec<DiscoveryEntry> {
    let mut switch = envelope(computer, base_topic, "Crossfeed", SWITCH_KEY);
    switch.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), SWITCH_KEY).into(),
    );
    switch.insert("payload_on".into(), ON.into());
    switch.insert("payload_off".into(), OFF.into());
    let mut preset = envelope(computer, base_topic, "Crossfeed preset", PRESET_KEY);
    preset.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), PRESET_KEY).into(),
    );
    if commands {
        switch.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), SWITCH_KEY).into(),
        );
        preset.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), PRESET_KEY).into(),
        );
        preset.insert(
            "options".into(),
            Json::Array(options(&view.models).into_iter().map(Json::from).collect()),
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
            payload: Json::Object(preset),
        },
    ]
}

/// The label of `preset` of `model`: the model alone when it has the one preset.
fn label(model: &ModelView, preset: &str) -> String {
    match model.presets.len() {
        1 => model.name.clone(),
        _ => format!("{}{SEPARATOR}{preset}", model.name),
    }
}

/// The select's options: every preset of every model, then [`CUSTOM`].
pub fn options(models: &[ModelView]) -> Vec<String> {
    let mut options: Vec<String> = models
        .iter()
        .flat_map(|model| {
            model
                .presets
                .iter()
                .map(|preset| label(model, &preset.name))
        })
        .collect();
    options.push(CUSTOM.to_string());
    options
}

/// What the two state topics carry: the switch's `ON` or `OFF`, and the preset's option, which
/// is [`CUSTOM`] for settings no preset stands for. The preset is told while the crossfeed is
/// off too, as the settings it will play when turned on.
pub fn state(view: &CrossfeedView) -> (&'static str, String) {
    let on = if view.on { ON } else { OFF };
    let preset = view
        .preset
        .as_deref()
        .map(|name| {
            view.models
                .iter()
                .find(|model| model.word == view.model)
                .map_or_else(|| name.to_string(), |model| label(model, name))
        })
        .unwrap_or_else(|| CUSTOM.to_string());
    (on, preset)
}

/// Parse a command on the preset select against the view's options, in any case, blanks aside.
pub fn parse_preset(view: &CrossfeedView, body: &str) -> Option<Choice> {
    let body = body.trim();
    if body.eq_ignore_ascii_case(CUSTOM) {
        return Some(Choice::Custom);
    }
    view.models.iter().find_map(|model| {
        model.presets.iter().find_map(|preset| {
            label(model, &preset.name)
                .eq_ignore_ascii_case(body)
                .then(|| Choice::Preset {
                    model: model.word.clone(),
                    preset: preset.name.clone(),
                })
        })
    })
}
