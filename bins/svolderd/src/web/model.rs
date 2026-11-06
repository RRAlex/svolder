// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The page's JSON model: what a theme draws from. Built from the same view models as the
//! templates (so a guest gets no unit id and no passphrase, and inert controls), as values,
//! words, ranges and states and no geometry. The page carries the first snapshot; the model
//! routes and the event stream send fresh ones.
//!
//! - `device`: a device's parts: the volume, the controls, the meters, the tone control, the
//!   presets, the facts and the Now playing card.
//! - `page`: the page: the viewer, the fleet with its hops, the host, and the first snapshot.

mod device;
mod page;

use serde::Serialize;
use serde_json::Value;

use super::view::{REST_GLOW, Screen};
pub(crate) use device::{CardModel, LinkModel, PlayingModel};
use device::{
    ControlModel, DevicePresetsModel, FactsModel, MetersModel, PowerModel, PresetsModel, ToneModel,
    VolumeModel,
};
#[cfg(test)]
pub(crate) use page::route_model;
pub(crate) use page::{
    HostModel, PageModel, RouteModel, fleet_model, host_model, host_parts, page_model,
};

/// One of the parts a device's model is made of, as the stream sends them: each part is sent
/// again when it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    Link,
    Card,
    Volume,
    Controls,
    Meters,
    Power,
    Tone,
    Presets,
    DevicePresets,
    Facts,
    Playing,
}

impl Part {
    /// Every part, in the order the model lists them.
    #[cfg_attr(not(test), allow(dead_code, reason = "only the tests read it"))]
    pub(crate) const ALL: [Part; 11] = [
        Part::Link,
        Part::Card,
        Part::Volume,
        Part::Controls,
        Part::Meters,
        Part::Power,
        Part::Tone,
        Part::Presets,
        Part::DevicePresets,
        Part::Facts,
        Part::Playing,
    ];

    /// The part's key in the model.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Part::Link => "link",
            Part::Card => "card",
            Part::Volume => "volume",
            Part::Controls => "controls",
            Part::Meters => "meters",
            Part::Power => "power",
            Part::Tone => "tone",
            Part::Presets => "presets",
            Part::DevicePresets => "device_presets",
            Part::Facts => "facts",
            Part::Playing => "playing",
        }
    }
}

/// A device as a theme draws it.
#[derive(Debug, Serialize)]
pub(crate) struct DeviceModel {
    pub(crate) name: String,
    /// Its name as one URL path segment.
    pub(crate) url: String,
    /// Its name in hex, for element ids.
    pub(crate) dom: String,
    /// `dac`, `amp` or `preamp`.
    pub(crate) family: &'static str,
    /// `DAC`, `amp` or `preamp`.
    pub(crate) family_word: &'static str,
    /// How it lays out: `amp` (meters and power first) or `dac`.
    pub(crate) layout: &'static str,
    /// Whether it is an amp with only its meters to show.
    pub(crate) lone: bool,
    /// What the screen is made of: a token that changes when its parts or its card change.
    pub(crate) shape: String,
    /// Whether every control is inert: a guest, or writes off.
    pub(crate) inert: bool,
    /// Whether the view is drawn dimmed.
    pub(crate) away: bool,
    /// Whether the controls are locked: the link is lost.
    pub(crate) locked: bool,
    /// The glow's level, 0 to 1.
    pub(crate) glow: f64,
    pub(crate) link: LinkModel,
    pub(crate) card: Option<CardModel>,
    pub(crate) volume: Option<VolumeModel>,
    pub(crate) controls: Vec<ControlModel>,
    pub(crate) meters: Option<MetersModel>,
    pub(crate) power: Option<PowerModel>,
    pub(crate) tone: Option<ToneModel>,
    /// Every band's steps with the core's words, for a drag.
    pub(crate) tone_steps: Option<Value>,
    pub(crate) presets: Option<PresetsModel>,
    pub(crate) device_presets: Option<DevicePresetsModel>,
    pub(crate) facts: FactsModel,
    pub(crate) playing: Option<PlayingModel>,
}

/// `screen` as a theme draws it.
pub(crate) fn device_model(screen: &Screen) -> DeviceModel {
    DeviceModel {
        name: screen.name.clone(),
        url: screen.url.clone(),
        dom: screen.dom.clone(),
        family: screen.family,
        family_word: screen.family_word,
        layout: if screen.amp { "amp" } else { "dac" },
        lone: screen.lone,
        shape: screen.shape.clone(),
        inert: screen.inert,
        away: screen.away(),
        locked: screen.locked(),
        glow: screen.glow.parse().unwrap_or(REST_GLOW),
        link: (&screen.link).into(),
        card: screen.card.as_ref().map(Into::into),
        volume: screen.knob.as_ref().map(Into::into),
        controls: screen.controls.iter().map(Into::into).collect(),
        meters: screen.meters.as_ref().map(Into::into),
        power: screen.power.as_ref().map(Into::into),
        // An amp's layout draws no tone control or presets, so its model carries none.
        tone: screen
            .tone
            .as_ref()
            .filter(|_| screen.has("tone"))
            .map(|tone| device::tone(tone, screen.bands.as_ref())),
        tone_steps: screen
            .tone_steps
            .as_deref()
            .filter(|_| screen.has("tone"))
            .and_then(|steps| serde_json::from_str(steps).ok()),
        presets: screen
            .presets
            .as_ref()
            .filter(|_| screen.has("presets"))
            .map(Into::into),
        device_presets: screen
            .device_presets
            .as_ref()
            .filter(|_| screen.has("device-presets"))
            .map(Into::into),
        facts: (&screen.facts).into(),
        playing: screen.playing.as_ref().map(Into::into),
    }
}

/// `part` of `model` as JSON: what the stream sends when it changes.
pub(crate) fn part_value(model: &DeviceModel, part: Part) -> Value {
    fn value<T: Serialize>(part: &T) -> Value {
        serde_json::to_value(part).unwrap_or(Value::Null)
    }
    match part {
        Part::Link => value(&model.link),
        Part::Card => value(&model.card),
        Part::Volume => value(&model.volume),
        Part::Controls => value(&model.controls),
        Part::Meters => value(&model.meters),
        Part::Power => value(&model.power),
        Part::Tone => value(&model.tone),
        Part::Presets => value(&model.presets),
        Part::DevicePresets => value(&model.device_presets),
        Part::Facts => value(&model.facts),
        Part::Playing => value(&model.playing),
    }
}

#[cfg(test)]
mod tests;
