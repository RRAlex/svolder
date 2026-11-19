// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a dump says of the PipeWire setup between the player and the sink, for the setup check
//! and the path verdict; filled by the route reader.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::prop;

#[cfg(test)]
mod tests;

/// The setup facts a dump gives for `sink` (the node the route ends in) and `stream` (the
/// player's stream), with the graph's `clock`. The ALSA hop's `hw` and `kernel` stay unread:
/// [`fill_alsa`](super::fill_alsa) reads them from `/proc/asound`.
pub(super) fn read(
    sink: Option<&Value>,
    stream: Option<&Value>,
    clock: Option<ClockFacts>,
) -> SetupFacts {
    let follower = sink.and_then(follower_params);
    let param = |key: &str| follower.and_then(|params| param_of(params, key));
    SetupFacts {
        offer: sink.and_then(offer_of),
        forced_format: param("audio.format")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty() && *name != "UNKNOWN")
            .map(str::to_string),
        node_rates: param("audio.allowed-rates")
            .map(numbers_of)
            .unwrap_or_default(),
        clock,
        sink_layout: sink.and_then(negotiated_layout),
        stream_layout: stream.and_then(|node| {
            negotiated_layout(node).or_else(|| raw_entry(node).and_then(layout_of))
        }),
        dither: sink.and_then(dither_of),
        sink_filter: sink.and_then(has_filter),
        stream_filter: stream.and_then(has_filter),
        alsa: sink.and_then(alsa_of),
    }
}

/// The clock of the `settings` metadata's `entries`: the rate, the allowed rates (printed as a
/// string, `[ 44100 48000 ]`) and the forced rate, 0 meaning none. `None` when it lists none
/// of them.
pub(super) fn clock_of(entries: &[Value]) -> Option<ClockFacts> {
    let value = |key: &str| {
        entries
            .iter()
            .find(|entry| entry["key"].as_str() == Some(key))
            .map(|entry| &entry["value"])
    };
    let number = |key: &str| value(key).and_then(number_of);
    let allowed = value("clock.allowed-rates");
    let clock = ClockFacts {
        rate: number("clock.rate"),
        allowed_rates: allowed.map(numbers_of).unwrap_or_default(),
        force_rate: number("clock.force-rate").filter(|rate| *rate != 0),
    };
    let listed =
        value("clock.rate").is_some() || allowed.is_some() || value("clock.force-rate").is_some();
    listed.then_some(clock)
}

/// A number as PipeWire printed it: a JSON number, or a string holding one.
fn number_of(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// The whole numbers in `value`: an array of them, or a string like `[ 44100 48000 ]`.
fn numbers_of(value: &Value) -> Vec<u32> {
    match value {
        Value::Array(items) => items.iter().filter_map(number_of).collect(),
        Value::String(text) => text
            .split(|c: char| !c.is_ascii_digit())
            .filter(|word| !word.is_empty())
            .filter_map(|word| word.parse().ok())
            .collect(),
        _ => Vec::new(),
    }
}

/// The value after `key` in a flat `[key, value, key, value, ...]` params array.
fn param_of<'v>(params: &'v [Value], key: &str) -> Option<&'v Value> {
    let at = params.iter().step_by(2).position(|item| item == key)?;
    params.get(at * 2 + 1)
}

/// The flat `params` of the node's `Props` entry holding `key`.
fn params_with<'v>(node: &'v Value, key: &str) -> Option<&'v [Value]> {
    node["info"]["params"]["Props"]
        .as_array()?
        .iter()
        .filter_map(|entry| entry["params"].as_array())
        .find(|params| param_of(params, key).is_some())
        .map(Vec::as_slice)
}

/// The ALSA follower's flat params: its `Props` entry that lists `audio.allowed-rates`.
fn follower_params(node: &Value) -> Option<&[Value]> {
    params_with(node, "audio.allowed-rates")
}

/// The `Props` entry of the node that carries a `device` (the ALSA follower's).
fn follower_device(node: &Value) -> Option<&str> {
    node["info"]["params"]["Props"]
        .as_array()?
        .iter()
        .find_map(|entry| entry["device"].as_str())
        .filter(|device| !device.is_empty())
}

/// The converter's dither: its `dither.method` and `dither.noise` params. Read from the
/// params, not the node's props: a stream's prop of the same name does not take. `None` when
/// either param is missing: a dither that was not read is unknown, never none.
pub(super) fn dither_of(node: &Value) -> Option<Dither> {
    let params = params_with(node, "dither.method")?;
    let method = param_of(params, "dither.method")?.as_str()?.to_string();
    let noise = param_of(params, "dither.noise").and_then(number_of)?;
    Some(Dither { method, noise })
}

/// Whether the node's converter runs a filter graph: a prop `audioconvert.filter-graph.<n>`, or
/// a non-empty `audioconvert.filter-graph` param, unless `audioconvert.filter-graph.disable`
/// is true. A converter that lists no graph param (PipeWire before 1.4 has none) has no graph.
/// `None` when the node's props or its converter params were not read at all and no prop
/// settles it: a graph that was not looked for is unknown, never absent.
fn has_filter(node: &Value) -> Option<bool> {
    const DISABLE: &str = "audioconvert.filter-graph.disable";
    const GRAPH: &str = "audioconvert.filter-graph";
    let props = node["info"]["props"].as_object()?;
    let params =
        params_with(node, "dither.method").or_else(|| params_with(node, "channelmix.disable"));
    let is_true = |value: &Value| {
        value.as_bool() == Some(true) || value.as_str().is_some_and(|text| text == "true")
    };
    let disabled = props.get(DISABLE).is_some_and(is_true)
        || params
            .and_then(|params| param_of(params, DISABLE))
            .is_some_and(is_true);
    if disabled {
        return Some(false);
    }
    let by_prop = props
        .keys()
        .any(|key| key.starts_with("audioconvert.filter-graph.") && key != DISABLE);
    if by_prop {
        return Some(true);
    }
    let graph = param_of(params?, GRAPH).and_then(Value::as_str);
    Some(graph.is_some_and(|graph| !graph.trim().is_empty()))
}

/// The ALSA hop of a node whose `device.api` is `alsa`: its path (`api.alsa.path`, else the
/// follower's `device`), card, PCM device number and substream.
fn alsa_of(node: &Value) -> Option<AlsaFacts> {
    if prop(node, "device.api") != Some("alsa") {
        return None;
    }
    let path = prop(node, "api.alsa.path")
        .filter(|path| !path.is_empty())
        .or_else(|| follower_device(node));
    let number = |key| super::prop_u64(node, key).and_then(|n| u32::try_from(n).ok());
    Some(AlsaFacts {
        path: path.map(str::to_string),
        card: number("api.alsa.pcm.card"),
        device: number("api.alsa.pcm.device").or_else(|| number("alsa.device")),
        subdevice: number("alsa.subdevice"),
        ..AlsaFacts::default()
    })
}

/// The node's `EnumFormat` entry of subtype `raw` (the iec958 one is not a sample format).
fn raw_entry(node: &Value) -> Option<&Value> {
    node["info"]["params"]["EnumFormat"]
        .as_array()?
        .iter()
        .find(|entry| entry["mediaSubtype"].as_str() == Some("raw"))
}

/// The sink's offer: the formats, rates and layout of its `raw` entry.
fn offer_of(node: &Value) -> Option<SinkOffer> {
    let entry = raw_entry(node)?;
    let formats = choice_values(&entry["format"])
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    Some(SinkOffer {
        formats,
        rates: rates_of(&entry["rate"]),
        layout: layout_of(entry),
    })
}

/// The values of a choice object, `default` first and then `alt1..altN` in order, with no
/// repeats; a plain value is the only one.
fn choice_values(value: &Value) -> Vec<&Value> {
    let Some(map) = value.as_object() else {
        return if value.is_null() {
            Vec::new()
        } else {
            vec![value]
        };
    };
    let mut alts: Vec<(u32, &Value)> = map
        .iter()
        .filter_map(|(key, value)| Some((key.strip_prefix("alt")?.parse().ok()?, value)))
        .collect();
    alts.sort_by_key(|(index, _)| *index);
    let mut seen: Vec<&Value> = Vec::new();
    for value in map
        .get("default")
        .into_iter()
        .chain(alts.into_iter().map(|(_, v)| v))
    {
        if !seen.contains(&value) {
            seen.push(value);
        }
    }
    seen
}

/// The rates of an `EnumFormat` entry: a plain rate, a `min`/`max` range, or a choice list.
fn rates_of(value: &Value) -> Option<Rates> {
    if let (Some(min), Some(max)) = (
        value.get("min").and_then(number_of),
        value.get("max").and_then(number_of),
    ) {
        return Some(Rates::Range { min, max });
    }
    let mut rates: Vec<u32> = choice_values(value)
        .into_iter()
        .filter_map(number_of)
        .collect();
    match rates.len() {
        0 => None,
        1 => Some(Rates::One(rates.remove(0))),
        _ => Some(Rates::List(rates)),
    }
}

/// The channels and positions of a `Format` or `EnumFormat` entry; `None` without a plain
/// channel count (a range of counts is no layout).
fn layout_of(entry: &Value) -> Option<Layout> {
    let channels = number_of(&entry["channels"])?;
    let positions = entry["position"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(Layout {
        channels,
        positions,
    })
}

/// The layout of the node's negotiated `Format` (none while it is suspended).
fn negotiated_layout(node: &Value) -> Option<Layout> {
    layout_of(node["info"]["params"]["Format"].get(0)?)
}

/// The setup facts one dump (and, for the ALSA hop, the card's `/proc/asound` files) gives.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetupFacts {
    /// What the sink offers: the `raw` entry of its `EnumFormat`.
    pub offer: Option<SinkOffer>,
    /// The format forced on the ALSA follower (its Props `audio.format`), when not `UNKNOWN`.
    pub forced_format: Option<String>,
    /// The follower's Props `audio.allowed-rates`; empty means the clock's setting applies.
    pub node_rates: Vec<u32>,
    /// The clock, from the `settings` metadata.
    pub clock: Option<ClockFacts>,
    /// The sink's negotiated Format; `None` while the sink is suspended.
    pub sink_layout: Option<Layout>,
    /// The player's stream's Format, else its `EnumFormat`.
    pub stream_layout: Option<Layout>,
    /// The sink converter's dither parameters.
    pub dither: Option<Dither>,
    /// Whether a filter graph sits inside the sink's converter; `None` when not read.
    pub sink_filter: Option<bool>,
    /// Whether a filter graph sits inside the player's stream's converter; `None` when not read.
    pub stream_filter: Option<bool>,
    /// The ALSA hop's facts; `None` for a sink whose `device.api` is not alsa.
    pub alsa: Option<AlsaFacts>,
}

/// The `raw` entry of a sink's `EnumFormat`: what it offers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SinkOffer {
    /// The offered sample formats, the default first, with no repeats.
    pub formats: Vec<String>,
    /// The offered rates, when the entry names any.
    pub rates: Option<Rates>,
    /// The offered channel layout, when the entry names one.
    pub layout: Option<Layout>,
}

/// The rates an `EnumFormat` entry offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rates {
    /// A single rate.
    One(u32),
    /// Every rate from `min` to `max`.
    Range {
        /// The lowest rate.
        min: u32,
        /// The highest rate.
        max: u32,
    },
    /// An explicit list of rates.
    List(Vec<u32>),
}

/// A channel layout.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// How many channels.
    pub channels: u32,
    /// The channel positions (`FL`, `FR`, ...), in order.
    pub positions: Vec<String>,
}

/// The graph clock's settings, from the `settings` metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockFacts {
    /// The clock's rate.
    pub rate: Option<u32>,
    /// The rates the clock may switch to.
    pub allowed_rates: Vec<u32>,
    /// The rate forced on the clock; a forced rate of 0 reads as `None`.
    pub force_rate: Option<u32>,
}

/// The sink converter's dither setting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Dither {
    /// The dither method's name.
    pub method: String,
    /// The dither noise amount, in bits.
    pub noise: u32,
}

/// The ALSA hop: the device the sink opens and what the kernel says of it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlsaFacts {
    /// The ALSA path: `api.alsa.path`, else the follower's Props `device`.
    pub path: Option<String>,
    /// The card number, from `api.alsa.pcm.card`.
    pub card: Option<u32>,
    /// The PCM device number: `api.alsa.pcm.device`, else `alsa.device`.
    pub device: Option<u32>,
    /// The substream the node plays on, from `alsa.subdevice`; `hw_params` is read from it
    /// (substream 0 when the node does not say).
    pub subdevice: Option<u32>,
    /// The playback substream's `hw_params`, as read from `/proc/asound`.
    pub hw: HwState,
    /// What the card's `stream{device}` file offers for playback.
    pub kernel: Option<KernelOffer>,
}

/// What the playback substream's `hw_params` file said.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HwState {
    /// The file was not read: unknown, never taken as clean.
    #[default]
    NotRead,
    /// The file reads `closed`: nothing is playing through the device.
    Closed,
    /// The device is open with these parameters.
    Open(HwParams),
}

/// The parameters an open ALSA device runs with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HwParams {
    /// The sample format in ALSA's name for it (`S32_LE`).
    pub format: String,
    /// The sample rate in hertz.
    pub rate: u32,
    /// How many channels.
    pub channels: u32,
}

/// What the card's `stream0` file lists for playback.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KernelOffer {
    /// The playback formats in ALSA's names.
    pub formats: Vec<String>,
    /// The playback rates in hertz.
    pub rates: Vec<u32>,
}
