// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tool names, their definitions and the checked arguments of a call.

use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, JsonObject, Tool, ToolAnnotations};
use serde_json::{Value, json};

use super::host;

/// The tool names, each with whether it changes a device.
pub(super) const TOOLS: [(&str, bool); 13] = [
    ("list_devices", false),
    ("get_state", false),
    ("get_link_history", false),
    ("list_presets", false),
    ("get_host", false),
    ("set_control", true),
    ("load_preset", true),
    ("mute", true),
    ("identify", true),
    ("set_host_volume", true),
    ("host_mute", true),
    ("set_host_eq", true),
    ("set_host_crossfeed", true),
];

/// A call's arguments, checked: a name the tool does not take is refused, and each is read
/// as the type the tool wants.
pub(super) struct Args<'a> {
    map: &'a JsonObject,
}

impl<'a> Args<'a> {
    pub(super) fn new(
        request: &'a CallToolRequestParams,
        allowed: &[&str],
    ) -> Result<Args<'a>, String> {
        static EMPTY: std::sync::OnceLock<JsonObject> = std::sync::OnceLock::new();
        let map = request
            .arguments
            .as_ref()
            .unwrap_or_else(|| EMPTY.get_or_init(JsonObject::new));
        if let Some(extra) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!(
                "{} takes no argument {extra:?}; its arguments are: {}",
                request.name,
                if allowed.is_empty() {
                    "none".to_string()
                } else {
                    allowed.join(", ")
                }
            ));
        }
        Ok(Args { map })
    }

    pub(super) fn value(&self, key: &str) -> Option<&Value> {
        self.map.get(key).filter(|v| !v.is_null())
    }

    pub(super) fn text(&self, key: &str) -> Result<Option<&str>, String> {
        match self.value(key) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(format!("{key} must be a string")),
        }
    }

    pub(super) fn required_text(&self, key: &str) -> Result<&str, String> {
        self.text(key)?
            .ok_or_else(|| format!("the argument {key:?} is required"))
    }

    pub(super) fn number(&self, key: &str) -> Result<Option<f64>, String> {
        match self.value(key) {
            None => Ok(None),
            Some(v) => v
                .as_f64()
                .map(Some)
                .ok_or_else(|| format!("{key} must be a number")),
        }
    }

    pub(super) fn flag(&self, key: &str) -> Result<Option<bool>, String> {
        match self.value(key) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(format!("{key} must be true or false")),
        }
    }
}

/// A tool's definition: its name, its one-sentence description and its arguments.
pub(super) fn definition(name: &'static str, write: bool) -> Tool {
    if name == "set_host_crossfeed" {
        return host::crossfeed_definition();
    }
    let device = json!({
        "type": "string",
        "description": "The device's name from list_devices; may be left out when there is one device"
    });
    let (description, properties, required): (&str, Value, &[&str]) = match name {
        "list_devices" => (
            "List the configured devices with their product name and whether each is connected.",
            json!({}),
            &[],
        ),
        "get_state" => (
            "Read every control of a device with its key, label, kind, current value, unit (the volume is in dB; a balance counts from its centre: 0 centred, below 0 to the left, above to the right) and the values or range it accepts; the keys are the ones set_control takes.",
            json!({ "device": device }),
            &[],
        ),
        "get_link_history" => (
            "Show when a device's Bluetooth link came up and dropped over the last hours (at most 24).",
            json!({
                "device": device,
                "hours": { "type": "number", "description": "How far back to look, in hours; above 0, at most 24; 24 when left out" }
            }),
            &[],
        ),
        "list_presets" => (
            "List a device's presets, each with whether it is the one on (`on`): the tone-control presets, built in and saved, that fit its equalizer; or, on a Loki Max, its three device slots (`slot`, never marked on, since the device does not say which is current) and its five built-in settings.",
            json!({ "device": device }),
            &[],
        ),
        "set_control" => (
            "Set one control of a device by its key from get_state (a level in its unit, dB for the volume, a balance in steps from its centre: 0 centred, below 0 to the left, above to the right; a toggle as true or false; a select by name); a rise of more than 6 dB, a higher gain or mode, an unmute, or powering a device on (the power toggle set to true) asks the user first, and a volume above the owner's limit is refused; a volume above three quarters of its range climbs there, and the reply gives the level the device had reached when the tool answered, which is short of the target if the climb is still going; powering off never asks.",
            json!({
                "device": device,
                "key": { "type": "string", "description": "The control's key from get_state" },
                "value": { "type": ["number", "string", "boolean"], "description": "The new value; for a level the number in its unit (dB for the volume; a balance counts from its centre)" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to the change, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["key", "value"],
        ),
        "load_preset" => (
            "Apply a preset from list_presets: a tone-control preset to a device's equalizer, which does not touch the volume control, but each band boosts at most 6 dB and bands that overlap add up, so it can make some frequencies louder; or, on a Loki Max, a device slot (its recall always asks the user first, because the bands it sets are not known and any of them can be set as high as +12 dB) or a built-in setting (it asks the user first when a band would rise more than 6 dB above its lowest level of the last minute, naming each such band). Saving to a slot is not offered.",
            json!({
                "device": device,
                "name": { "type": "string", "description": "The preset's name from list_presets" },
                "confirm": { "type": "boolean", "description": "Only for a Loki Max, only after the user agreed to the question the call raised, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["name"],
        ),
        "get_host" => (
            "Read the computer's volume (percent, muted, the sink), its EQ (on or off, the preset it came from or custom, the preamp in dB and each point's type, frequency, gain and Q), its crossfeed (on or off, model, preset or custom, amount in percent, feed level, crossover, delay and its floor, the output, a note when it turned itself off) and the player (title, artist, album, status); each is null while it is not available, and `unavailable` says why.",
            json!({}),
            &[],
        ),
        "set_host_volume" => (
            "Set the computer's volume in whole percent, 0 to 100 (never above 100); a rise of more than 6 dB over its lowest level in the last minute, on the scale the computer's mixer uses (the cube of the percent), asks the user first; lowering never asks.",
            json!({
                "percent": { "type": "integer", "minimum": 0, "maximum": 100, "description": "The new volume in percent, 0 to 100" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to the rise, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["percent"],
        ),
        "host_mute" => (
            "Mute or unmute the computer's output; unmuting asks the user first.",
            json!({
                "muted": { "type": "boolean", "description": "true to mute, false to unmute; unmuting asks the user first" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to unmute, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["muted"],
        ),
        "set_host_eq" => (
            "Recall a preset on the computer's EQ (`preset`, a saved preset or a built-in one, which is shaped to a device's tone control: name that device in `device`, or leave it out when there is one) or turn the EQ off (`off`: true); give exactly one of them. A preset a connected device plays too asks the user first, since the correction would play twice.",
            json!({
                "preset": { "type": "string", "description": "The preset's name from list_presets" },
                "off": { "type": "boolean", "description": "true to turn the computer's EQ off" },
                "device": device,
                "confirm": { "type": "boolean", "description": "Only after the user agreed to the question the call raised, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &[],
        ),
        "identify" => (
            "Ask a device to show itself, so it can be told apart from the others; it changes nothing on the device, and a second call within two seconds of one that went out sends nothing and answers sent false.",
            json!({ "device": device }),
            &[],
        ),
        _ => (
            "Mute or unmute a device's output; unmuting asks the user first.",
            json!({
                "device": device,
                "on": { "type": "boolean", "description": "true to mute, false to unmute; unmuting asks the user first" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to unmute, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["on"],
        ),
    };
    let schema = json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    });
    let Value::Object(schema) = schema else {
        unreachable!("an object literal");
    };
    let mut tool = Tool::new(name, description, Arc::new(schema));
    tool.annotations = Some(ToolAnnotations::new().read_only(!write));
    tool
}
