// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The live computer EQ's filter chain: a fixed graph of sixteen slots per channel behind a
//! preamp, and the parameters that set it while it plays.
//!
//! The graph never changes shape. A point is a slot's parameters, so adding, removing, moving
//! or retyping one is a `set-param` on the running node and no rebuild: nothing is heard but
//! the change. Every slot is a `bq_raw` biquad whose coefficients svolder works out for the
//! rate the sink is running at (see [`slot_params`]); the preamp is a `linear` node's `Mult`.
//! A rate change rebuilds the graph and puts the config's coefficients back, so whoever sends
//! reads the rate with [`props_of`] and sends every slot again.
//!
//! After slot 16 the graph holds one crossfeed stage per channel, the same six nodes whatever
//! the model (see [`crossfeed`](super::crossfeed)), so crossfeed changes are `set-param`s too
//! ([`crossfeed_params`]). The config on disk always has the stage off, a bypass that is
//! bit-exact: [`render_live_conf`] writes it so, and [`render_live_conf_with`] writes a given
//! stage for a chain that cannot be set live.
//!
//! [`props_of`] reads the node, the rate, the values the chain holds and the sink it plays into
//! out of `pw-dump`; [`output_sink`] reads the real output sink.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde_json::Value;
use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::response::coefficients;

use super::crossfeed::{DELAY_BUFFER_S, Stage};
use super::points::Points;
use super::{NODE_NAME, Route, builtin_type, conf_frame, num};

/// How many slots each channel has.
pub const SLOTS: usize = 16;

/// How the slots are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotShape {
    /// One builtin biquad per slot of a type fixed in the config, set by `Freq`, `Q` and
    /// `Gain`: a change of type rebuilds the graph.
    Typed,
    /// Every slot a `bq_raw` set by its coefficients, which svolder works out for the running
    /// rate: any type in any slot, live.
    Raw,
}

/// What carries the preamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreampCarrier {
    /// A `linear` node, set by its `Mult`.
    Linear,
    /// A high shelf at 0 Hz, set by its `Gain`.
    Shelf,
}

/// The slot shape the live chain is built with.
pub const SHAPE: SlotShape = SlotShape::Raw;

/// The preamp's carrier.
pub const PREAMP: PreampCarrier = PreampCarrier::Linear;

/// The rate the config's own coefficients are worked out for.
pub const CONFIG_RATE_HZ: u32 = 48_000;

/// The channels' prefixes in a node name.
const CHANNELS: [char; 2] = ['l', 'r'];

/// The coefficient ports of a `bq_raw`, in the order they are sent.
const RAW_PORTS: [&str; 6] = ["b0", "b1", "b2", "a0", "a1", "a2"];

/// What a `linear` preamp's `Mult` is for `db`.
fn gain_of(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// The lowest frequency and Q, and the highest Q, a biquad is worked out for: only what keeps
/// the maths valid, far outside what a hand sets (a preset's values play as written).
const F_FLOOR_HZ: f64 = 1.0;
const Q_FLOOR: f64 = 0.01;
const Q_CEILING: f64 = 100.0;

/// The largest gain, either way, a biquad is worked out for: a value past it would overflow the
/// maths into coefficients that are not numbers.
const GAIN_BOUND_DB: f64 = 60.0;

/// `filter` held to what keeps the biquad's maths valid at `rate_hz`: a frequency from 1 Hz to
/// just under Nyquist (never below the floor, whatever the rate), a Q from 0.01 to 100 and a gain
/// within plus or minus 60 dB, so a zero, negative, absurd or not-a-number value cannot make
/// NaN coefficients. Everything else in range is as written. This is what plays, so it is what
/// the preamp is measured on ([`played_sets`]).
pub(super) fn held(filter: &SourceFilter, rate_hz: u32) -> SourceFilter {
    let top = (f64::from(rate_hz) * 0.499).max(F_FLOOR_HZ);
    SourceFilter {
        kind: filter.kind,
        f: match filter.f.is_nan() {
            true => 1000.0,
            false => filter.f.clamp(F_FLOOR_HZ, top),
        },
        g: match filter.g.is_finite() {
            true => filter.g.clamp(-GAIN_BOUND_DB, GAIN_BOUND_DB),
            false => 0.0,
        },
        q: match filter.q.is_nan() {
            true => 1.0,
            false => filter.q.clamp(Q_FLOOR, Q_CEILING),
        },
    }
}

/// `correction` with every filter as it plays at `rate_hz` (see [`held`]).
#[cfg(test)]
pub(super) fn played(correction: &Correction, rate_hz: u32) -> Correction {
    Correction {
        preamp_db: correction.preamp_db,
        filters: correction
            .filters
            .iter()
            .map(|filter| held(filter, rate_hz))
            .collect(),
    }
}

/// The coefficients `[b0, b1, b2, a1, a2]` the chain runs for each of `correction`'s filters at
/// `rate_hz`: PipeWire takes its control values as f32, so these are the f64 cookbook values
/// rounded to f32, and the centres of the held filters in Hz.
pub(super) fn played_sets(correction: &Correction, rate_hz: u32) -> (Vec<[f64; 5]>, Vec<f64>) {
    correction
        .filters
        .iter()
        .map(|filter| {
            let f = held(filter, rate_hz);
            let set = coefficients(f.kind, f.f, f.g, f.q, rate_hz).map(|c| f64::from(c as f32));
            match is_identity(&set) {
                true => ([1.0, 0.0, 0.0, 0.0, 0.0], f.f),
                false => (set, f.f),
            }
        })
        .unzip()
}

/// Whether `[b0, b1, b2, a1, a2]` has its numerator equal to its denominator once rounded to
/// f32 (a 0 dB filter): the identity whatever its poles. It is sent as the exact identity, so
/// it is bit-exact however the chain's maths is compiled (a fused multiply-add leaves rounding
/// error in poles near the unit circle).
fn is_identity(set: &[f64; 5]) -> bool {
    let [b0, b1, b2, a1, a2] = set.map(|c| c as f32);
    b0 == 1.0 && b1 == a1 && b2 == a2
}

/// The six numbers of a `bq_raw` for one filter at `rate_hz`; an empty slot is the identity.
/// `a0` is 1: the others are divided by it.
fn raw_values(filter: Option<&SourceFilter>, rate_hz: u32) -> [f64; 6] {
    match filter.map(|f| held(f, rate_hz)) {
        Some(f) => {
            let set = coefficients(f.kind, f.f, f.g, f.q, rate_hz);
            match is_identity(&set) {
                true => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                false => [set[0], set[1], set[2], 1.0, set[3], set[4]],
            }
        }
        None => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

/// A `bq_raw` node named `name` holding `[b0, b1, b2, a0, a1, a2]` for `rate_hz`.
fn raw_node(name: &str, [b0, b1, b2, a0, a1, a2]: [f64; 6], rate_hz: u32) -> String {
    format!(
        "{{ type = builtin name = {name} label = bq_raw config = {{ coefficients = [ {{ rate = {rate_hz} b0 = {} b1 = {} b2 = {} a0 = {} a1 = {} a2 = {} }} ] }} }}",
        num(b0),
        num(b1),
        num(b2),
        num(a0),
        num(a1),
        num(a2)
    )
}

/// The six `bq_raw` numbers of a stage section `[b0, b1, b2, a1, a2]`: `a0` is 1.
fn stage_raw([b0, b1, b2, a1, a2]: [f64; 5]) -> [f64; 6] {
    [b0, b1, b2, 1.0, a1, a2]
}

/// The nodes of the crossfeed stage of `channel`, as the config writes them, holding `stage`.
fn stage_nodes(channel: char, stage: &Stage, rate_hz: u32) -> String {
    let mut out = String::new();
    for (name, section) in [("dir", stage.dir), ("x", stage.x), ("s", stage.s)] {
        let _ = writeln!(
            out,
            "                    {}",
            raw_node(&format!("{channel}{name}"), stage_raw(section), rate_hz)
        );
    }
    let [sum1, sum2] = stage.sum;
    let _ = writeln!(
        out,
        "                    {{ type = builtin name = {channel}sum label = mixer control = {{ \"Gain 1\" = {} \"Gain 2\" = {} }} }}",
        num(sum1),
        num(sum2)
    );
    let _ = writeln!(
        out,
        "                    {{ type = builtin name = {channel}del label = delay config = {{ \"max-delay\" = {} }} control = {{ \"Delay (s)\" = {} }} }}",
        num(DELAY_BUFFER_S),
        num(stage.delay_s)
    );
    let _ = writeln!(
        out,
        "                    {{ type = builtin name = {channel}mix label = mixer control = {{ \"Gain 1\" = 1 \"Gain 2\" = {} }} }}",
        num(stage.mix)
    );
    out
}

/// The links of the crossfeed stage of `channel`: its own channel's last slot feeds the direct
/// and side paths, the other channel's feeds the cross path (the other channel's slot 16 so
/// tees to this channel's cross node).
fn stage_links(channel: char, other: char) -> String {
    let last = format!("{SLOTS:02}");
    let mut out = String::new();
    for (from, to) in [
        (format!("{channel}{last}:Out"), format!("{channel}dir:In")),
        (format!("{channel}{last}:Out"), format!("{channel}s:In")),
        (format!("{other}{last}:Out"), format!("{channel}x:In")),
        (format!("{channel}x:Out"), format!("{channel}sum:In 1")),
        (format!("{channel}s:Out"), format!("{channel}sum:In 2")),
        (format!("{channel}sum:Out"), format!("{channel}del:In")),
        (format!("{channel}dir:Out"), format!("{channel}mix:In 1")),
        (format!("{channel}del:Out"), format!("{channel}mix:In 2")),
    ] {
        let _ = writeln!(
            out,
            "                    {{ output = \"{from}\" input = \"{to}\" }}"
        );
    }
    out
}

/// The graph's node of one slot, as the config writes it.
fn slot_node(channel: char, slot: u8, filter: Option<&SourceFilter>, rate_hz: u32) -> String {
    let name = format!("{channel}{slot:02}");
    match SHAPE {
        SlotShape::Raw => raw_node(&name, raw_values(filter, rate_hz), rate_hz),
        SlotShape::Typed => {
            let (kind, f, g, q) = match filter.map(|f| held(f, rate_hz)) {
                Some(f) => (f.kind, f.f, f.g, f.q),
                None => (FilterKind::Peaking, 1000.0, 0.0, 1.0),
            };
            format!(
                "{{ type = builtin name = {name} label = {} control = {{ Freq = {} Q = {} Gain = {} }} }}",
                builtin_type(kind),
                num(f),
                num(q),
                num(g)
            )
        }
    }
}

/// The graph's preamp node of one channel.
fn preamp_node(channel: char, db: f64) -> String {
    match PREAMP {
        PreampCarrier::Linear => format!(
            "{{ type = builtin name = {channel}pre label = linear control = {{ Mult = {} Add = 0.0 }} }}",
            num(gain_of(db))
        ),
        PreampCarrier::Shelf => format!(
            "{{ type = builtin name = {channel}pre label = bq_highshelf control = {{ Freq = 0 Q = 1.0 Gain = {} }} }}",
            num(db)
        ),
    }
}

/// The complete PipeWire config of the live chain in front of the sink `target`, for `route`:
/// the frame of [`render_conf`](super::render_conf) round a graph of `lpre → l01 … l16` and
/// `rpre → r01 … r16`, the slots holding `points` (their coefficients worked out for a sink
/// running at `rate_hz`), the preamp `preamp_db` and the crossfeed stage OFF (a bypass). The
/// node is called [`NODE_NAME`].
pub fn render_live_conf(
    points: &Points,
    preamp_db: f64,
    target: &str,
    route: Route,
    rate_hz: u32,
) -> String {
    render_live_conf_with(points, preamp_db, target, route, rate_hz, &Stage::bypass())
}

/// [`render_live_conf`] with the crossfeed stage holding `stage`: for a chain that cannot be set
/// live, so the stage has to be in the config it starts with.
pub fn render_live_conf_with(
    points: &Points,
    preamp_db: f64,
    target: &str,
    route: Route,
    rate_hz: u32,
    stage: &Stage,
) -> String {
    let in_slot = |slot: u8| {
        points
            .slots
            .iter()
            .position(|held| *held == slot)
            .map(|index| &points.filters[index])
    };
    let mut nodes = String::new();
    let mut links = String::new();
    for channel in CHANNELS {
        let _ = writeln!(
            nodes,
            "                    {}",
            preamp_node(channel, preamp_db)
        );
        let mut previous = format!("{channel}pre");
        for slot in 1..=SLOTS as u8 {
            let _ = writeln!(
                nodes,
                "                    {}",
                slot_node(channel, slot, in_slot(slot), rate_hz)
            );
            let name = format!("{channel}{slot:02}");
            let _ = writeln!(
                links,
                "                    {{ output = \"{previous}:Out\" input = \"{name}:In\" }}"
            );
            previous = name;
        }
    }
    for channel in CHANNELS {
        nodes.push_str(&stage_nodes(channel, stage, rate_hz));
    }
    for (channel, other) in [('l', 'r'), ('r', 'l')] {
        links.push_str(&stage_links(channel, other));
    }
    let graph = format!(
        "                nodes = [\n{nodes}                ]\n\
         \x20               links = [\n{links}                ]\n\
         \x20               inputs  = [ \"lpre:In\" \"rpre:In\" ]\n\
         \x20               outputs = [ \"lmix:Out\" \"rmix:Out\" ]\n"
    );
    conf_frame(&graph, target, route)
}

/// The parameters that put `filter` (or, for `None`, a transparent filter) in `slot` (1 to 16)
/// on both channels, for a sink running at `rate_hz`; a frequency or Q that would break the
/// maths is held (see `held`), anything else plays as written. Typed slots are `Freq`, `Q` and `Gain`;
/// raw ones `b0 b1 b2 a0 a1 a2`, divided by `a0` (which is sent as 1).
pub fn slot_params(slot: u8, filter: Option<&SourceFilter>, rate_hz: u32) -> Vec<(String, f64)> {
    let mut params = Vec::new();
    for channel in CHANNELS {
        let name = format!("{channel}{slot:02}");
        match SHAPE {
            SlotShape::Raw => {
                for (port, value) in RAW_PORTS.iter().zip(raw_values(filter, rate_hz)) {
                    params.push((format!("{name}:{port}"), value));
                }
            }
            SlotShape::Typed => {
                let (f, g, q) = match filter.map(|f| held(f, rate_hz)) {
                    Some(f) => (f.f, f.g, f.q),
                    None => (1000.0, 0.0, 1.0),
                };
                params.push((format!("{name}:Freq"), f));
                params.push((format!("{name}:Q"), q));
                params.push((format!("{name}:Gain"), g));
            }
        }
    }
    params
}

/// The parameters that set the preamp to `db` on both channels.
pub fn preamp_params(db: f64) -> Vec<(String, f64)> {
    CHANNELS
        .iter()
        .map(|channel| match PREAMP {
            PreampCarrier::Linear => (format!("{channel}pre:Mult"), gain_of(db)),
            PreampCarrier::Shelf => (format!("{channel}pre:Gain"), db),
        })
        .collect()
}

/// The parameters that set the crossfeed stage to `stage` on both channels, in one
/// `set-param`: the three paths' coefficients (`ldir:b0 … ls:a2`, `a0` sent as 1), the sum's
/// gains (`lsum:Gain 1`, `lsum:Gain 2`), the added delay in seconds (`ldel:Delay (s)`) and the
/// delayed path's gain into the mixer (`lmix:Gain 2`, 0 for the bypass).
pub fn crossfeed_params(stage: &Stage) -> Vec<(String, f64)> {
    let mut params = Vec::new();
    for channel in CHANNELS {
        for (name, section) in [("dir", stage.dir), ("x", stage.x), ("s", stage.s)] {
            for (port, value) in RAW_PORTS.iter().zip(stage_raw(section)) {
                params.push((format!("{channel}{name}:{port}"), value));
            }
        }
        params.push((format!("{channel}sum:Gain 1"), stage.sum[0]));
        params.push((format!("{channel}sum:Gain 2"), stage.sum[1]));
        params.push((format!("{channel}del:Delay (s)"), stage.delay_s));
        params.push((format!("{channel}mix:Gain 2"), stage.mix));
    }
    params
}

/// The `pw-cli` line that sets `params` on node `node`, in one call:
/// `set-param 42 Props { params = [ "l07:Gain" 6 … ] }`.
pub fn set_param_line(node: u32, params: &[(String, f64)]) -> String {
    let mut line = format!("set-param {node} Props {{ params = [");
    for (name, value) in params {
        let _ = write!(line, " \"{name}\" {}", num(*value));
    }
    line.push_str(" ] }");
    line
}

impl LiveNode {
    /// Whether the node has the preamp control a current chain lists (even before any set). A
    /// node without it is a chain an older svolder built, which none of the sends here fit.
    pub fn has_preamp_control(&self) -> bool {
        preamp_params(0.0)
            .iter()
            .all(|(name, _)| self.params.contains_key(name))
    }

    /// Whether the node has the crossfeed stage's controls (every port [`crossfeed_params`]
    /// lists). A chain built before the stage existed has the preamp's and not these: it plays,
    /// and is rebuilt once when crossfeed is first turned on.
    pub fn has_crossfeed_controls(&self) -> bool {
        crossfeed_params(&Stage::bypass())
            .iter()
            .all(|(name, _)| self.params.contains_key(name))
    }
}

/// The live chain's node as `pw-dump` shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveNode {
    /// The node's object id, which a restart of the chain changes.
    pub id: u32,
    /// The rate the sink the chain plays into is running at; `None` when that sink is not
    /// found or has no format yet. The chain's own node does not show it.
    pub rate_hz: Option<u32>,
    /// The `node.name` of the sink the chain's output stream is linked to; `None` when it is
    /// not linked to one.
    pub sink: Option<String>,
    /// Whether a playback stream plays through the chain's sink: a node linked into the sink's
    /// input that is itself `running`. The sink's own state is not read: a capture of its
    /// monitor keeps it running with nothing playing, and a paused or corked stream leaves its
    /// node `idle`.
    pub playing: bool,
    /// The values the chain's slots, preamp and crossfeed stage hold, by port name (`l07:b0`,
    /// `lmix:Gain 2`).
    pub params: HashMap<String, f64>,
}

/// The live chain's node in `dump` (`pw-dump`'s output): the `Audio/Sink` node named
/// [`NODE_NAME`], with the rate of the sink its output stream is linked to and the values it
/// holds. `None` when the dump is not one or the chain is not in it.
pub fn props_of(dump: &str) -> Option<LiveNode> {
    let objects: Vec<Value> = serde_json::from_str(dump).ok()?;
    let node_named = |name: &str, class: Option<&str>| {
        objects.iter().find(|object| {
            let props = &object["info"]["props"];
            object["type"].as_str().is_some_and(|t| t.ends_with("Node"))
                && props["node.name"].as_str() == Some(name)
                && class.is_none_or(|class| props["media.class"].as_str() == Some(class))
        })
    };
    let node = node_named(NODE_NAME, Some("Audio/Sink"))?;
    let id = u32::try_from(node["id"].as_u64()?).ok()?;
    let mut params = HashMap::new();
    for props in node["info"]["params"]["Props"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let pairs = props["params"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        for pair in pairs.as_chunks::<2>().0 {
            if let (Some(name), Some(value)) = (pair[0].as_str(), pair[1].as_f64())
                && name.contains(':')
            {
                params.insert(name.to_string(), value);
            }
        }
    }
    let output =
        node_named(&format!("{NODE_NAME}.output"), None).and_then(|output| output["id"].as_u64());
    let sink = output.and_then(|output| {
        objects.iter().find_map(|object| {
            let info = &object["info"];
            (object["type"].as_str().is_some_and(|t| t.ends_with("Link"))
                && info["output-node-id"].as_u64() == Some(output))
            .then(|| info["input-node-id"].as_u64())
            .flatten()
        })
    });
    let sink_object = sink.and_then(|sink| {
        objects
            .iter()
            .find(|object| object["id"].as_u64() == Some(sink))
    });
    let sink_name = sink_object
        .and_then(|sink| sink["info"]["props"]["node.name"].as_str())
        .map(str::to_string);
    let rate_hz = sink_object.and_then(|sink| {
        let rate = sink["info"]["params"]["Format"].as_array()?.first()?["rate"].as_u64()?;
        u32::try_from(rate).ok().filter(|rate| *rate > 0)
    });
    Some(LiveNode {
        id,
        rate_hz,
        sink: sink_name,
        playing: objects.iter().any(|link| {
            link["type"].as_str().is_some_and(|t| t.ends_with("Link"))
                && link["info"]["input-node-id"].as_u64() == Some(u64::from(id))
                && link["info"]["output-node-id"].as_u64().is_some_and(|from| {
                    objects.iter().any(|stream| {
                        stream["id"].as_u64() == Some(from)
                            && stream["info"]["state"].as_str() == Some("running")
                    })
                })
        }),
        params,
    })
}

/// The `node.name` of the default audio sink in the parsed `objects` of a dump.
fn default_sink_name(objects: &[Value]) -> Option<String> {
    objects.iter().find_map(|object| {
        if object["props"]["metadata.name"].as_str() != Some("default") {
            return None;
        }
        object["metadata"]
            .as_array()?
            .iter()
            .filter(|entry| entry["key"].as_str() == Some("default.audio.sink"))
            .find_map(|entry| match &entry["value"] {
                Value::String(text) => serde_json::from_str::<Value>(text)
                    .ok()
                    .and_then(|value| value["name"].as_str().map(str::to_string)),
                value => value["name"].as_str().map(str::to_string),
            })
    })
}

/// Whether `name` is the computer EQ's own sink or its output stream.
fn is_chain_name(name: &str) -> bool {
    name == NODE_NAME || name.starts_with(&format!("{NODE_NAME}."))
}

/// The node named `name` in `objects`.
fn node_by_name<'a>(objects: &'a [Value], name: &str) -> Option<&'a Value> {
    objects.iter().find(|object| {
        object["type"].as_str().is_some_and(|t| t.ends_with("Node"))
            && object["info"]["props"]["node.name"].as_str() == Some(name)
    })
}

/// The first format rate of `sink`, `None` when it has none yet.
fn rate_of(sink: &Value) -> Option<u32> {
    let rate = sink["info"]["params"]["Format"].as_array()?.first()?["rate"].as_u64()?;
    u32::try_from(rate).ok().filter(|rate| *rate > 0)
}

/// The rate the default audio sink runs at in `dump`, `None` when there is no default sink,
/// it is the computer EQ itself, or it has no format yet.
pub fn default_sink_rate(dump: &str) -> Option<u32> {
    let objects: Vec<Value> = serde_json::from_str(dump).ok()?;
    let name = default_sink_name(&objects)?;
    if is_chain_name(&name) {
        return None;
    }
    rate_of(node_by_name(&objects, &name)?)
}

/// The sink the computer plays into, as a dump shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSink {
    /// The sink's `node.name`.
    pub name: String,
    /// What a person calls it: its `node.description`, else its `node.nick`.
    pub description: Option<String>,
    /// The rate it runs at, `None` before it has a format.
    pub rate_hz: Option<u32>,
}

/// The real output sink in `dump`: the default sink, or, when the default is the computer EQ
/// itself (the default route), the sink the chain's output stream is linked to. `None` when
/// there is no default sink, or the default is the chain and it is linked to none. It is what
/// "the output changed" is read from: the same answer whether or not the chain is in front.
pub fn output_sink(dump: &str) -> Option<OutputSink> {
    let objects: Vec<Value> = serde_json::from_str(dump).ok()?;
    let default = default_sink_name(&objects)?;
    let name = match is_chain_name(&default) {
        true => props_of(dump)?.sink?,
        false => default,
    };
    let sink = node_by_name(&objects, &name)?;
    let props = &sink["info"]["props"];
    let text = |key: &str| props[key].as_str().map(str::to_string);
    Some(OutputSink {
        name,
        description: text("node.description").or_else(|| text("node.nick")),
        rate_hz: rate_of(sink),
    })
}

#[cfg(test)]
mod tests;
