// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Home Assistant MQTT discovery: one retained config per exposed control, so an entity
//! appears with no YAML, and the empty payload that removes one again.

use serde_json::{Map, Value as Json, json};

use svolder_profiles::{Control, ControlKind, EqMode, EqSpec, Profile};
use svolder_proto::{FunctionClass, ValueType};

use crate::json_number;
use crate::topics;

pub(crate) use svolder_profiles::units::{round3, spread as table_bounds};

/// The key the tone-control mode is exposed under. It is not a real profile control, so it is
/// not in `profile.controls`; [`exposed_controls`] adds it as a `select` in its own right. The
/// rest of the tone control (the preset, the loudness strength and every band) is modeled in
/// [`crate::eq`].
pub const EQ_MODE_KEY: &str = "eq_mode";

/// The key the per-device connectivity entity is exposed under.
const LINK_KEY: &str = "link";

/// The key the signal-strength sensor is exposed under.
pub const SIGNAL_KEY: &str = "signal";

/// The key the main firmware build's sensor is exposed under.
pub const FIRMWARE_KEY: &str = "firmware";

/// The key the DSP firmware build's sensor is exposed under.
pub const DSP_FIRMWARE_KEY: &str = "dsp_firmware";

/// The firmware subsystem the main build is read from (`210.2.1`).
const MAIN_FIRMWARE: u8 = 1;

/// The firmware subsystem the DSP build is read from (`210.2.2`).
const DSP_FIRMWARE: u8 = 2;

/// The key the identify button is exposed under; its press arrives on this key's command topic.
pub const IDENTIFY_KEY: &str = "identify";

/// The payload Home Assistant sends when the identify button is pressed.
pub const IDENTIFY_PRESS: &str = "PRESS";

/// The manufacturer every device's `device` block names.
const MANUFACTURER: &str = "Schiit Audio";

/// The payload that clears a retained discovery config: an empty message removes the entity
/// from Home Assistant without touching any other retained topic.
pub const CLEARING_PAYLOAD: &str = "";

/// One entity's discovery config: the topic it is retained on and the payload Home Assistant
/// reads there.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryEntry {
    /// `homeassistant/<component>/svolder_<unit>/<key>/config`.
    pub topic: String,
    /// The JSON body, ready to serialize.
    pub payload: Json,
}

/// A device's identity for the `device` and `origin` blocks every entity's config carries.
#[derive(Debug, Clone, Copy)]
pub struct Device<'a> {
    /// The name configured for it, shown as the Home Assistant device's own name.
    pub name: &'a str,
    /// Its unit id, the sixteen hex characters `svolder_<unit_id>` is built from.
    pub unit_id: &'a str,
    /// Its firmware build, when the daemon has read one; fills `device.sw_version`.
    pub firmware: Option<u16>,
}

/// Every discovery config for one device: the link, then one entry per exposed control (the
/// tone-control mode included), matching the platform `commands` maps a control's kind to, then
/// the tone control's own entities from [`crate::eq`] when the profile has one, or the preset
/// select of [`crate::device_presets`] when it has presets of its own, then the diagnostics: the
/// signal strength, the firmware builds the profile has the session ask for, and, with
/// `commands`, the identify button where the profile offers one. `presets` names the presets
/// that fit the device, in the order its preset select offers them. An empty `discovery_prefix`
/// turns discovery off entirely, returning nothing to publish.
pub fn configs(
    base_topic: &str,
    discovery_prefix: &str,
    profile: &Profile,
    device: &Device<'_>,
    commands: bool,
    presets: &[String],
) -> Vec<DiscoveryEntry> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    let mut out = vec![link_entry(base_topic, discovery_prefix, profile, device)];
    for control in exposed_controls(profile) {
        out.push(control_entry(
            base_topic,
            discovery_prefix,
            profile,
            device,
            &control,
            commands,
        ));
    }
    out.extend(crate::eq::entries(
        base_topic,
        discovery_prefix,
        profile,
        device,
        commands,
        presets,
    ));
    out.extend(crate::device_presets::entries(
        base_topic,
        discovery_prefix,
        profile,
        device,
        commands,
    ));
    out.extend(diagnostic_entries(
        base_topic,
        discovery_prefix,
        profile,
        device,
        commands,
    ));
    out
}

/// The entities that are not controls: the signal strength with its attributes, the main and
/// DSP firmware builds (diagnostic sensors, each offered only where the profile's
/// `firmware_subsystems` holds the subsystem it reads, 1 and 2), and, only with `commands` and a
/// profile that offers identify, the identify button.
fn diagnostic_entries(
    base_topic: &str,
    discovery_prefix: &str,
    profile: &Profile,
    device: &Device<'_>,
    commands: bool,
) -> Vec<DiscoveryEntry> {
    let unit = device.unit_id;
    let sensor = |key: &str, name: &str| {
        let mut payload = entity_envelope(base_topic, profile, device, name, key);
        payload.insert(
            "state_topic".into(),
            topics::diagnostic(base_topic, unit, key).into(),
        );
        payload.insert("entity_category".into(), "diagnostic".into());
        payload
    };
    let entry = |component: &str, key: &str, payload: Map<String, Json>| DiscoveryEntry {
        topic: topics::discovery_config(discovery_prefix, component, unit, key),
        payload: Json::Object(payload),
    };
    let mut signal = sensor(SIGNAL_KEY, "Signal");
    signal.insert("device_class".into(), "signal_strength".into());
    signal.insert("unit_of_measurement".into(), "dBm".into());
    signal.insert("state_class".into(), "measurement".into());
    signal.insert(
        "json_attributes_topic".into(),
        topics::signal_attributes(base_topic, unit).into(),
    );
    let mut out = vec![entry("sensor", SIGNAL_KEY, signal)];
    let asked = |subsystem: u8| profile.firmware_subsystems.contains(&subsystem);
    if asked(MAIN_FIRMWARE) {
        out.push(entry(
            "sensor",
            FIRMWARE_KEY,
            sensor(FIRMWARE_KEY, "Firmware"),
        ));
    }
    if asked(DSP_FIRMWARE) {
        out.push(entry(
            "sensor",
            DSP_FIRMWARE_KEY,
            sensor(DSP_FIRMWARE_KEY, "DSP firmware"),
        ));
    }
    if commands && profile.identify {
        let mut button = entity_envelope(base_topic, profile, device, "Identify", IDENTIFY_KEY);
        button.insert(
            "command_topic".into(),
            topics::command(base_topic, unit, IDENTIFY_KEY).into(),
        );
        button.insert("payload_press".into(), IDENTIFY_PRESS.into());
        button.insert("entity_category".into(), "config".into());
        out.push(entry("button", IDENTIFY_KEY, button));
    }
    out
}

/// Every control a profile exposes over MQTT, in profile order, with the tone-control mode
/// appended as a `select` when the profile has a tone control: only ids of the `Normal`
/// function class ever leave this list, so nothing in the firmware-update family or the
/// passphrase function can be published as a control or written by a command (nothing in a real
/// profile names either as a control, but the check stands regardless). The only readings of
/// that family that are published are the two version readings, `210.2.1` and `210.2.2`, as
/// read-only diagnostic sensors ([`crate::payload::firmware_state`]), never through this list.
/// The tone control's preset, loudness strength and bands are not controls and are modeled by
/// [`crate::eq`] instead.
/// `svolderd`'s bridge task uses this to know which controls to publish and coalesce commands
/// for, the same list [`configs`] builds discovery entries from.
pub fn exposed_controls(profile: &Profile) -> Vec<Control> {
    let mut out: Vec<Control> = profile
        .controls
        .iter()
        .filter(|c| c.id.class() == FunctionClass::Normal)
        .cloned()
        .collect();
    if let Some(eq) = &profile.eq {
        out.push(eq_mode_control(eq));
    }
    out
}

/// A `Control` standing for the tone-control mode, built from the EQ spec's own function id
/// and `EqMode`'s names, so the option list can never drift from what `EqMode` actually parses.
fn eq_mode_control(eq: &EqSpec) -> Control {
    let values = (1u8..=3)
        .filter_map(|raw| EqMode::from_raw(raw).map(|mode| (raw, mode.name().to_string())))
        .collect();
    Control {
        id: eq.mode_id(),
        key: EQ_MODE_KEY.to_string(),
        label: "Tone control mode".to_string(),
        kind: ControlKind::Select,
        value_type: ValueType::U8,
        values,
        range: None,
        center: None,
        unit: None,
        convert: None,
        channel: None,
        floor: None,
        tick: None,
        log_scale: None,
        advice: None,
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    }
}

/// The Home Assistant platform one control's kind is discovered as, `commands` deciding
/// between a live control and a read-only one for the three kinds that are ever writable. A
/// readout of boolean type is a binary sensor, any other a sensor.
fn component_for(kind: ControlKind, value_type: ValueType, commands: bool) -> &'static str {
    match kind {
        ControlKind::Readout if value_type == ValueType::Bool => "binary_sensor",
        ControlKind::Level if commands => "number",
        ControlKind::Level => "sensor",
        ControlKind::Toggle if commands => "switch",
        ControlKind::Toggle => "binary_sensor",
        ControlKind::Select if commands => "select",
        ControlKind::Select => "sensor",
        ControlKind::Meter | ControlKind::Readout => "sensor",
        ControlKind::Fault => "binary_sensor",
    }
}

/// One control's discovery entry.
fn control_entry(
    base_topic: &str,
    discovery_prefix: &str,
    profile: &Profile,
    device: &Device<'_>,
    control: &Control,
    commands: bool,
) -> DiscoveryEntry {
    let component = component_for(control.kind, control.value_type, commands);
    let topic = topics::discovery_config(discovery_prefix, component, device.unit_id, &control.key);
    let value_topic = match control.kind {
        ControlKind::Meter => topics::meters(base_topic, device.unit_id),
        _ => topics::state(base_topic, device.unit_id),
    };
    let mut payload = entity_envelope(base_topic, profile, device, &control.label, &control.key);
    payload.insert("state_topic".into(), value_topic.into());
    payload.insert(
        "value_template".into(),
        format!("{{{{ value_json.{} }}}}", control.key).into(),
    );
    let writable = commands && control.kind.writable();
    if writable {
        payload.insert(
            "command_topic".into(),
            topics::command(base_topic, device.unit_id, &control.key).into(),
        );
    }
    match control.kind {
        ControlKind::Level => {
            // `min`/`max`/`step`/`mode` belong to the `number` schema alone: a read-only
            // `sensor` has no slider to bound, so it carries only the unit.
            if writable {
                if let Some(bounds) = svolder_profiles::units::bounds(control) {
                    payload.insert("min".into(), json_number(bounds.min));
                    payload.insert("max".into(), json_number(bounds.max));
                    payload.insert("step".into(), json_number(bounds.step));
                }
                payload.insert("mode".into(), "slider".into());
            }
            if let Some(unit) = &control.unit {
                payload.insert("unit_of_measurement".into(), unit.clone().into());
            }
        }
        ControlKind::Toggle => {
            payload.insert("payload_on".into(), "ON".into());
            payload.insert("payload_off".into(), "OFF".into());
        }
        ControlKind::Readout if control.value_type == ValueType::Bool => {
            payload.insert("payload_on".into(), "ON".into());
            payload.insert("payload_off".into(), "OFF".into());
        }
        ControlKind::Fault => {
            payload.insert("payload_on".into(), "ON".into());
            payload.insert("payload_off".into(), "OFF".into());
            payload.insert("device_class".into(), "problem".into());
        }
        ControlKind::Select if writable => {
            let options: Vec<Json> = control
                .values
                .iter()
                .map(|(_, name)| Json::from(name.clone()))
                .collect();
            payload.insert("options".into(), Json::Array(options));
        }
        ControlKind::Meter => {
            payload.insert("state_class".into(), "measurement".into());
            // A meter with a scale publishes the percentage of its live part.
            if control.scale().is_some() {
                payload.insert("unit_of_measurement".into(), "%".into());
            } else if let Some(unit) = &control.unit {
                payload.insert("unit_of_measurement".into(), unit.clone().into());
            }
        }
        ControlKind::Select | ControlKind::Readout => {}
    }
    DiscoveryEntry {
        topic,
        payload: Json::Object(payload),
    }
}

/// The per-device connectivity entity: `online` while the link the availability topic itself
/// carries, depending only on the bridge's own availability rather than on itself.
fn link_entry(
    base_topic: &str,
    discovery_prefix: &str,
    profile: &Profile,
    device: &Device<'_>,
) -> DiscoveryEntry {
    let topic =
        topics::discovery_config(discovery_prefix, "binary_sensor", device.unit_id, LINK_KEY);
    let mut payload = entity_envelope(base_topic, profile, device, "Link", LINK_KEY);
    payload.insert(
        "state_topic".into(),
        topics::device_availability(base_topic, device.unit_id).into(),
    );
    payload.insert("payload_on".into(), "online".into());
    payload.insert("payload_off".into(), "offline".into());
    payload.insert("device_class".into(), "connectivity".into());
    // The link's own state topic already is the availability signal, so unlike every other
    // entity it depends on the bridge alone, not on itself as well.
    payload.insert(
        "availability".into(),
        Json::Array(vec![
            json!({"topic": topics::bridge_availability(base_topic)}),
        ]),
    );
    DiscoveryEntry {
        topic,
        payload: Json::Object(payload),
    }
}

/// The fields every entity's config carries: its name, its unique id, both availability
/// topics with `availability_mode: all`, and the shared `device` and `origin` blocks. A
/// caller overwrites `availability` afterwards for the one entity (the link) that depends on
/// only one of them.
pub(crate) fn entity_envelope(
    base_topic: &str,
    profile: &Profile,
    device: &Device<'_>,
    name: &str,
    key: &str,
) -> Map<String, Json> {
    let mut m = Map::new();
    m.insert("name".into(), name.into());
    m.insert(
        "unique_id".into(),
        topics::unique_id(device.unit_id, key).into(),
    );
    m.insert(
        "availability".into(),
        Json::Array(vec![
            json!({"topic": topics::bridge_availability(base_topic)}),
            json!({"topic": topics::device_availability(base_topic, device.unit_id)}),
        ]),
    );
    m.insert("availability_mode".into(), "all".into());
    m.insert("device".into(), device_json(profile, device));
    m.insert("origin".into(), origin_json());
    m
}

/// The `device` block every entity of one device shares.
fn device_json(profile: &Profile, device: &Device<'_>) -> Json {
    let mut m = Map::new();
    m.insert(
        "identifiers".into(),
        Json::Array(vec![topics::device_id(device.unit_id).into()]),
    );
    m.insert("name".into(), device.name.into());
    m.insert("manufacturer".into(), MANUFACTURER.into());
    m.insert("model".into(), profile.display.clone().into());
    if let Some(firmware) = device.firmware {
        m.insert("sw_version".into(), firmware.to_string().into());
    }
    Json::Object(m)
}

/// The `origin` block naming this software, every entity carries the same one.
fn origin_json() -> Json {
    json!({"name": "svolder", "sw_version": env!("CARGO_PKG_VERSION")})
}

#[cfg(test)]
mod tests;
