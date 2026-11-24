// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The route a player's sound takes to the default sink, read from `pw-dump`: which sink is
//! the default, which stream is the followed player's, how many other streams are playing
//! into the same sink, and whether the computer EQ's filter sits between them.
//!
//! [`PwDump`] runs `pw-dump` once and gives it [`TIMEOUT`]; a missing tool, one that fails or
//! one that does not answer in time reads as no dump, never as an error. [`route`] is a pure
//! function of that text and of the player being followed, so every case of it is tested
//! against `pw-dump` output saved in `tests/fixtures/pw-dump`. It reads the dump as a plain
//! JSON array of PipeWire objects (nodes, ports, links, clients, devices and the `default`
//! metadata), with no PipeWire library.
//!
//! The default sink comes from the `default` metadata's `default.audio.sink`. When that is the
//! computer EQ's own sink ([`NODE_NAME`]), the sink described is the real one the filter's
//! output feeds, so the facts always describe the device the sound ends up in.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::eq::NODE_NAME;
use crate::sink::output_within;

mod asound;
mod setup;

pub use asound::{ASOUND_DIR_ENV, Asound, fill_alsa, hw_state, kernel_offer};
pub use setup::{
    AlsaFacts, ClockFacts, Dither, HwParams, HwState, KernelOffer, Layout, Rates, SetupFacts,
    SinkOffer,
};

/// The environment variable naming the `pw-dump` to run, for [`PwDump::from_env`]; unset or
/// empty, it is `pw-dump` on `PATH`.
pub const PW_DUMP_ENV: &str = "SVOLDER_PW_DUMP";

/// How long one `pw-dump` call gets before it is killed and read as no answer.
pub const TIMEOUT: Duration = Duration::from_secs(2);

/// The graph's dump, through PipeWire's `pw-dump`.
#[derive(Debug, Clone)]
pub struct PwDump {
    /// The `pw-dump` to run.
    program: OsString,
    /// How long a call gets.
    timeout: Duration,
}

impl PwDump {
    /// The dump through the `pw-dump` at `program` (a path, or a name looked up on `PATH`).
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            timeout: TIMEOUT,
        }
    }

    /// The dump through the `pw-dump` [`PW_DUMP_ENV`] names, else `pw-dump` on `PATH`; `None`
    /// when [`host_unreached`](crate::host_unreached) says host audio is off for this process.
    pub fn from_env() -> Option<Self> {
        Self::resolve(
            crate::host_unreached(),
            std::env::var_os(PW_DUMP_ENV).as_deref(),
        )
    }

    /// The dump `from_env` makes of `named` (the value of [`PW_DUMP_ENV`]), `None` when `off`.
    fn resolve(off: bool, named: Option<&OsStr>) -> Option<Self> {
        if off {
            return None;
        }
        let program = named.filter(|named| !named.is_empty());
        Some(Self::new(program.unwrap_or(OsStr::new("pw-dump"))))
    }

    /// The same dump with each call given `timeout` instead of [`TIMEOUT`].
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// What `pw-dump` printed when it ran, exited 0 and did so in time; the child is killed
    /// when it runs out of time. `None` otherwise.
    pub async fn read(&self) -> Option<String> {
        output_within(&self.program, &[], self.timeout).await
    }

    /// [`PwDump::read`] of the object `id` alone (and what `pw-dump` lists with it), given
    /// `timeout`: a fraction of the whole graph's cost.
    pub async fn read_one(&self, id: u32, timeout: Duration) -> Option<String> {
        output_within(&self.program, &[&id.to_string()], timeout).await
    }
}

/// How the followed player's stream was found among the streams of the graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerMatch {
    /// A stream whose client has the player's process id.
    Pid,
    /// No pid matched, and a stream's application name matches the player's identity.
    Name,
    /// The player has no stream of its own in the graph, and exactly one stream is running
    /// into the sink: taken as the player's.
    Assumed,
    /// The player's stream (found by pid or by name) plays into another sink than the default
    /// one: `sink` is that sink's description, else its node name.
    Elsewhere {
        /// What the other sink is called.
        sink: String,
    },
    /// A player is followed, but none of the streams could be taken as its own (or its own
    /// stream is linked to no sink).
    NotFound,
    /// No player is followed, so there is no stream to look for: nothing plays into the sink,
    /// or several streams do and none can be told from the others.
    #[default]
    NoPlayer,
    /// No MPRIS player is followed, and exactly one stream is running into the sink: taken as
    /// the sound's source, though what plays it cannot be named or controlled.
    NoMpris,
}

/// A [`PlayerMatch`] from the wire, with a way this build has no name for read as `NotFound`
/// (the player is not identified), so a newer daemon's view still decodes.
fn lenient_match<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<PlayerMatch, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(PlayerMatch::deserialize(value).unwrap_or(PlayerMatch::NotFound))
}

/// A [`SetupFacts`] from the wire, with one this build cannot read dropped to `None` so the
/// rest of a newer daemon's facts still decode.
fn lenient_setup<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<SetupFacts>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(Option::<SetupFacts>::deserialize(value).ok().flatten())
}

/// What the dump says about the route from the followed player to the default sink.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RouteFacts {
    /// The default sink, or the real sink behind the computer EQ's when that is the default;
    /// `None` when the dump names no default sink or the node is not in it.
    pub sink: Option<SinkFacts>,
    /// The stream into the sink taken as the sound's source: the followed player's, or with no
    /// MPRIS player the lone running one, when one was found (see [`PlayerMatch`]; none for
    /// `Elsewhere`).
    pub player: Option<StreamFacts>,
    /// How the player's stream was found; a way a newer daemon knows and this build does not
    /// reads as [`PlayerMatch::NotFound`].
    #[serde(deserialize_with = "lenient_match")]
    pub player_match: PlayerMatch,
    /// How many running streams reach the sink and are not the stream in `player`: another
    /// stream of the player's own counts too. With no `player` stream (`NotFound`, `NoPlayer`,
    /// `Elsewhere`) it counts every running stream into the sink.
    pub others_mixing: u32,
    /// Whether the computer EQ's filter feeds the sink and the player's stream goes through it
    /// (with no player's stream known: whether the filter feeds the sink at all; never for
    /// `Elsewhere`, whose stream does not play into this sink).
    pub host_eq: bool,
    /// Which stages of that chain play, when the daemon's EQ view told: `None` where only the
    /// route was read (the chain is then just "on"). The route alone cannot tell them apart.
    pub stages: Option<ChainStages>,
    /// What the dump says of the PipeWire setup between the player and the sink (see
    /// [`SetupFacts`]); `None` from an older svolderd, or when what was sent does not decode (a
    /// newer one): its checks are then skipped, never guessed.
    #[serde(
        deserialize_with = "lenient_setup",
        skip_serializing_if = "Option::is_none"
    )]
    pub setup: Option<SetupFacts>,
}

/// Which stages of the computer's filter chain change the sound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChainStages {
    /// The EQ points are in use.
    pub eq: bool,
    /// The loudness stage cuts: it is on and the computer's volume is under its reference.
    pub loudness: bool,
    /// The crossfeed plays.
    pub crossfeed: bool,
    /// The virtual speakers play.
    pub speakers: bool,
    /// The chain is a pass-through the daemon proved untouched (`levels.untouched`): no stage
    /// word is told for it, so the computer hop reads as changing nothing.
    pub untouched: bool,
}

impl ChainStages {
    /// The stages the computer's EQ view says are on; `None` when the view cannot vouch for them
    /// (the controls are unavailable, an older svolder's chain, or no stage on and no word that
    /// the chain is a kept pass-through, by its note or by `levels.passthrough` when another note
    /// stands, or runs for loudness), so the line says only that a chain
    /// is on. A chain that runs for loudness while the volume is at its reference changes nothing:
    /// it is told as a pass-through.
    pub fn of_view(view: &crate::eq::EqView) -> Option<Self> {
        use crate::eq::{LEGACY_NOTE, PASSTHROUGH_NOTE};
        if view.unavailable.is_some() || view.note.as_deref() == Some(LEGACY_NOTE) {
            return None;
        }
        let loudness_on = view
            .loudness
            .as_ref()
            .is_some_and(|ld| ld.on && ld.unavailable.is_none());
        let stages = Self {
            eq: view.on,
            loudness: loudness_on && view.loudness.as_ref().is_some_and(|ld| ld.depth_db > 0.0),
            crossfeed: view.crossfeed.as_ref().is_some_and(|xf| xf.on),
            speakers: view.speakers.as_ref().is_some_and(|sp| sp.on),
            untouched: view.levels.as_ref().is_some_and(|levels| levels.untouched),
        };
        let any = stages.eq || stages.loudness || stages.crossfeed || stages.speakers;
        let passthrough = view.note.as_deref() == Some(PASSTHROUGH_NOTE)
            || view
                .levels
                .as_ref()
                .is_some_and(|levels| levels.passthrough);
        (any || loudness_on || passthrough).then_some(stages)
    }
}

/// The sink a route ends in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SinkFacts {
    /// The node's `node.name`.
    pub name: String,
    /// What the node is called for people: its `node.description`, else its `node.nick`;
    /// empty when it has neither.
    pub description: String,
    /// The sample rate the sink runs at, from its negotiated `Format` only: a suspended sink
    /// has none, and reads `None` however its `node.rate` reads.
    pub rate_hz: Option<u32>,
    /// The sample format name, such as `S32LE`, when it has a format.
    pub format: Option<String>,
    /// The computer volume in thousandths, 1000 being 100 %: the node's master `volume` times
    /// its lowest channel volume, on the scale `wpctl` prints (the cube root of the linear
    /// value PipeWire reports), so 0.54 in `wpctl` is 540. When the computer EQ sits in the
    /// path it is the sink's volume times the filter node's, the gain the signal gets from
    /// both. `None` when the node reports no channel volumes.
    pub volume_milli: Option<u32>,
    /// Whether the sink is muted, hard or soft (or the computer EQ's node in the path is).
    pub muted: bool,
    /// The USB vendor id of the sink's device, when it sits on the USB bus.
    pub usb_vendor_id: Option<u16>,
    /// The USB product name of the sink's device (`device.product.name`, else the node's
    /// `api.alsa.card.name`), when it sits on the USB bus.
    pub usb_product: Option<String>,
}

/// One stream playing into the sink, as the stream itself negotiated it, before the sink
/// converts it, with the volume and mute the stream carries of its own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamFacts {
    /// The sample rate the stream plays at, when it has a format.
    pub rate_hz: Option<u32>,
    /// The sample format name, such as `S16LE`, when it has a format.
    pub format: Option<String>,
    /// How many channels the stream plays, when its format says.
    pub channels: Option<u32>,
    /// The stream's own volume in thousandths, on the same scale as [`SinkFacts::volume_milli`]
    /// (master `volume` times the lowest channel volume, cube-rooted); `None` when the node
    /// reports no channel volumes. A player that turned itself down reads below 1000 here.
    pub volume_milli: Option<u32>,
    /// Whether the stream is muted, hard or soft.
    pub muted: bool,
}

/// The route read out of `dump` (`pw-dump`'s output) for the player being followed: its D-Bus
/// connection's process id and its MPRIS identity, either of which may be unknown (`None` and
/// empty). `None` when `dump` is not a JSON array.
pub fn route(dump: &str, player_pid: Option<u32>, player_identity: &str) -> Option<RouteFacts> {
    let objects: Vec<Value> = serde_json::from_str(dump).ok()?;
    Some(Graph::new(&objects).facts(player_pid, player_identity))
}

/// The media class of a sink, and the prefix of a stream's that plays audio out.
const SINK_CLASS: &str = "Audio/Sink";
const STREAM_CLASS: &str = "Stream/Output/Audio";

/// The computer's volume and mute as the graph holds them: what the signal is given by the
/// sink and, while the computer EQ's chain is in the path, by the chain's node too.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SinkLevel {
    /// The linear gain (1.0 is full): the master `volume` times the lowest channel volume of the
    /// sink, times the chain node's. PipeWire reports the cube of the scale `wpctl` prints.
    pub linear: f64,
    /// Whether the sink, or the chain node in the path, is muted, hard or soft.
    pub muted: bool,
}

/// The computer's volume and mute read out of `dump` (`pw-dump`'s output), as the route line
/// reads them: the default sink's (the one behind the chain when the chain is the default),
/// times the chain node's while that node feeds the sink, in either route. `None` when `dump`
/// is not a JSON array, names no sink, or no node in the path reports a channel volume.
pub fn computer_level(dump: &str) -> Option<SinkLevel> {
    let objects: Vec<Value> = serde_json::from_str(dump).ok()?;
    Graph::new(&objects).level()
}

/// The sink a route ends in and the computer EQ's nodes round it.
struct Ends<'a> {
    sink: Option<&'a Value>,
    filter_in: Option<&'a Value>,
    filter_out: Option<&'a Value>,
    /// Whether the chain's output stream is linked into `sink`.
    filter_feeds: bool,
}

/// The objects of a dump that a route reads, indexed by id.
struct Graph<'a> {
    nodes: Vec<&'a Value>,
    clients: HashMap<u64, &'a Value>,
    devices: HashMap<u64, &'a Value>,
    /// The `(output node, input node)` of every link that is not in error.
    links: Vec<(u64, u64)>,
    /// The `name` of the default audio sink.
    default_sink: Option<String>,
    /// The clock the `settings` metadata gives.
    clock: Option<ClockFacts>,
}

impl<'a> Graph<'a> {
    fn new(objects: &'a [Value]) -> Self {
        let mut graph = Graph {
            nodes: Vec::new(),
            clients: HashMap::new(),
            devices: HashMap::new(),
            links: Vec::new(),
            default_sink: None,
            clock: None,
        };
        for object in objects {
            let Some(id) = object.get("id").and_then(Value::as_u64) else {
                continue;
            };
            let kind = object.get("type").and_then(Value::as_str).unwrap_or("");
            match kind.strip_prefix("PipeWire:Interface:") {
                Some("Node") => graph.nodes.push(object),
                Some("Client") => {
                    graph.clients.insert(id, object);
                }
                Some("Device") => {
                    graph.devices.insert(id, object);
                }
                Some("Link") => graph.add_link(object),
                Some("Metadata") => graph.add_metadata(object),
                _ => {}
            }
        }
        graph
    }

    fn add_link(&mut self, link: &Value) {
        let info = &link["info"];
        let state = info.get("state").and_then(Value::as_str).unwrap_or("");
        if matches!(state, "error" | "unlinked") {
            return;
        }
        let end = |key| info.get(key).and_then(Value::as_u64);
        if let (Some(output), Some(input)) = (end("output-node-id"), end("input-node-id")) {
            self.links.push((output, input));
        }
    }

    /// Remembers the default audio sink's name when `metadata` is the `default` one, and the
    /// clock when it is `settings`.
    fn add_metadata(&mut self, metadata: &Value) {
        let entries = metadata["metadata"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        match metadata["props"]["metadata.name"].as_str() {
            Some("default") => {}
            Some("settings") => {
                self.clock = setup::clock_of(entries);
                return;
            }
            _ => return,
        }
        for entry in entries
            .iter()
            .filter(|entry| entry["key"].as_str() == Some("default.audio.sink"))
        {
            // The value is JSON, already parsed by pw-dump, or still a string of it.
            let name = match &entry["value"] {
                Value::String(text) => serde_json::from_str::<Value>(text)
                    .ok()
                    .and_then(|value| value["name"].as_str().map(str::to_string)),
                value => value["name"].as_str().map(str::to_string),
            };
            self.default_sink = name;
        }
    }

    /// The node with `node.name` `name` and media class `class` (a prefix).
    fn node_named(&self, name: &str, class: &str) -> Option<&'a Value> {
        self.nodes
            .iter()
            .copied()
            .find(|node| prop(node, "node.name") == Some(name) && is_class(node, class))
    }

    /// Whether a link runs from node `from` to node `to`.
    fn linked(&self, from: u64, to: u64) -> bool {
        self.links.contains(&(from, to))
    }

    /// The first sink node `from` feeds.
    fn sink_after(&self, from: u64) -> Option<&'a Value> {
        let mut sinks = self.nodes.iter().copied().filter(|node| {
            is_class(node, SINK_CLASS) && node_id(node).is_some_and(|id| self.linked(from, id))
        });
        sinks.next()
    }

    /// The sink a stream that plays into `sink` reaches in the end: `sink` itself, or, when it
    /// is the computer EQ's filter (`filter_in`, its output being `filter_out`) and that feeds
    /// another sink, the one it feeds.
    fn behind_filter(
        &self,
        sink: &'a Value,
        filter_in: Option<&'a Value>,
        filter_out: Option<&'a Value>,
    ) -> &'a Value {
        match (filter_in, filter_out.and_then(node_id)) {
            (Some(filter), Some(output)) if same(sink, filter) => {
                self.sink_after(output).unwrap_or(sink)
            }
            _ => sink,
        }
    }

    /// Where the sound ends: the sink the default leads to (the one behind the chain when the
    /// chain is the default), the computer EQ's nodes and whether its output feeds that sink.
    fn ends(&self) -> Ends<'a> {
        let filter_in = self.node_named(NODE_NAME, SINK_CLASS);
        let filter_out = self.node_named(&format!("{NODE_NAME}.output"), STREAM_CLASS);
        let default = self
            .default_sink
            .as_deref()
            .and_then(|name| self.node_named(name, SINK_CLASS));
        // The default may be the filter's own sink: the device is what its output feeds.
        let sink = match (default, filter_in, filter_out) {
            (Some(default), Some(filter), Some(output)) if same(default, filter) => {
                node_id(output).and_then(|id| self.sink_after(id))
            }
            (Some(default), Some(filter), None) if same(default, filter) => None,
            (default, ..) => default,
        };
        let filter_feeds = match (filter_out.and_then(node_id), sink.and_then(node_id)) {
            (Some(output), Some(sink)) => self.linked(output, sink),
            _ => false,
        };
        Ends {
            sink,
            filter_in,
            filter_out,
            filter_feeds,
        }
    }

    /// The computer's volume and mute: the sink's, times the computer EQ's node's while that
    /// node feeds the sink (the same reading the route's `volume_milli` and `muted` make).
    fn level(&self) -> Option<SinkLevel> {
        let ends = self.ends();
        let filter = ends.filter_in.filter(|_| ends.filter_feeds);
        level_of(ends.sink?, filter)
    }

    fn facts(&self, player_pid: Option<u32>, identity: &str) -> RouteFacts {
        let Ends {
            sink,
            filter_in,
            filter_out,
            filter_feeds,
        } = self.ends();
        let sink_id = sink.and_then(node_id);
        let filter_id = filter_in.and_then(node_id);
        let filter_id = filter_id.filter(|_| filter_feeds);

        let streams: Vec<Stream> = self
            .nodes
            .iter()
            .copied()
            .filter(|node| is_class(node, STREAM_CLASS) && !is_filter_output(node))
            .filter_map(|node| {
                let id = node_id(node)?;
                let path = if filter_id.is_some_and(|filter| self.linked(id, filter)) {
                    Path::ThroughFilter
                } else if sink_id.is_some_and(|sink| self.linked(id, sink)) {
                    Path::Direct
                } else {
                    self.sink_after(id)
                        .map(|sink| self.behind_filter(sink, filter_in, filter_out))
                        .map_or(Path::Unlinked, Path::Elsewhere)
                };
                Some(Stream {
                    node,
                    id,
                    running: state(node) == Some("running"),
                    path,
                })
            })
            .collect();

        let (player_match, chosen) = self.find_player(&streams, player_pid, identity);
        let player = chosen.map(|stream| stream_facts(stream.node));
        let others_mixing = streams
            .iter()
            .filter(|stream| stream.reaches() && stream.running)
            .filter(|stream| chosen.is_none_or(|chosen| chosen.id != stream.id))
            .count();
        let host_eq = filter_feeds
            && match (&player_match, chosen) {
                (_, Some(stream)) => matches!(stream.path, Path::ThroughFilter),
                (PlayerMatch::Elsewhere { .. }, None) => false,
                _ => true,
            };
        let mut sink_facts = sink.map(|node| self.sink_facts(node));
        if let (true, Some(facts), Some(filter)) = (host_eq, sink_facts.as_mut(), filter_in) {
            apply_filter(facts, sink, filter);
        }
        RouteFacts {
            sink: sink_facts,
            player,
            player_match,
            others_mixing: u32::try_from(others_mixing).unwrap_or(u32::MAX),
            host_eq,
            stages: None,
            setup: Some(setup::read(
                sink,
                chosen.map(|stream| stream.node),
                self.clock.clone(),
            )),
        }
    }

    /// The stream that is the player's, and how it was found. Every stream of the graph is
    /// looked at, by pid first and then by name; a stream of the player's own that plays into
    /// another sink makes the match `Elsewhere`, and a lone running stream into the sink is
    /// assumed to be the player's only when the player has none of its own anywhere.
    fn find_player<'s>(
        &self,
        streams: &'s [Stream<'a>],
        pid: Option<u32>,
        identity: &str,
    ) -> (PlayerMatch, Option<&'s Stream<'a>>) {
        let identity = identity.trim();
        if pid.is_none() && identity.is_empty() {
            let mut running = streams
                .iter()
                .filter(|stream| stream.reaches() && stream.running);
            return match (running.next(), running.next()) {
                (Some(lone), None) => (PlayerMatch::NoMpris, Some(lone)),
                _ => (PlayerMatch::NoPlayer, None),
            };
        }
        let by_pid = streams
            .iter()
            .filter(|stream| {
                pid.is_some_and(|pid| self.pid_of(stream.node) == Some(u64::from(pid)))
            })
            .collect();
        if let Some(found) = settle(by_pid, PlayerMatch::Pid) {
            return found;
        }
        let by_name = streams
            .iter()
            .filter(|stream| {
                !identity.is_empty()
                    && self
                        .names_of(stream.node)
                        .iter()
                        .any(|name| names_match(identity, name))
            })
            .collect();
        if let Some(found) = settle(by_name, PlayerMatch::Name) {
            return found;
        }
        let mut running = streams
            .iter()
            .filter(|stream| stream.reaches() && stream.running);
        match (running.next(), running.next()) {
            (Some(lone), None) => (PlayerMatch::Assumed, Some(lone)),
            _ => (PlayerMatch::NotFound, None),
        }
    }

    /// The process id of the client behind a stream: its client's `application.process.id`,
    /// else the one on the node itself.
    fn pid_of(&self, node: &Value) -> Option<u64> {
        let client = prop_u64(node, "client.id").and_then(|id| self.clients.get(&id));
        client
            .and_then(|client| prop_u64(client, "application.process.id"))
            .or_else(|| prop_u64(node, "application.process.id"))
    }

    /// The names a stream goes by: its application name and binary, its desktop entry and its
    /// node name, on the node and on its client.
    fn names_of(&self, node: &Value) -> Vec<String> {
        const KEYS: [&str; 5] = [
            "application.name",
            "application.process.binary",
            "application.id",
            "pipewire.access.portal.app_id",
            "node.name",
        ];
        let client = prop_u64(node, "client.id").and_then(|id| self.clients.get(&id));
        let mut names = Vec::new();
        for holder in std::iter::once(node).chain(client.copied()) {
            for key in KEYS {
                if let Some(name) = prop(holder, key).filter(|name| !name.is_empty()) {
                    names.push(name.to_string());
                }
            }
        }
        names
    }

    fn sink_facts(&self, node: &Value) -> SinkFacts {
        let format = first_format(node, false);
        let device = prop_u64(node, "device.id").and_then(|id| self.devices.get(&id));
        let bus = device
            .and_then(|device| prop(device, "device.bus"))
            .or_else(|| prop(node, "device.bus"));
        let usb = bus == Some("usb");
        let vendor = device
            .and_then(|device| prop(device, "device.vendor.id"))
            .or_else(|| prop(node, "device.vendor.id"));
        let product = device
            .and_then(|device| prop(device, "device.product.name"))
            .or_else(|| prop(node, "device.product.name"))
            .or_else(|| prop(node, "api.alsa.card.name"))
            .filter(|name| !name.is_empty());
        let description = prop(node, "node.description")
            .or_else(|| prop(node, "node.nick"))
            .unwrap_or("");
        SinkFacts {
            name: prop(node, "node.name").unwrap_or("").to_string(),
            description: description.to_string(),
            rate_hz: format.as_ref().and_then(|format| format.0),
            format: format.and_then(|format| format.1),
            volume_milli: linear_volume(node).map(milli_of),
            muted: muted(node),
            usb_vendor_id: vendor.filter(|_| usb).and_then(usb_id),
            usb_product: product.filter(|_| usb).map(str::to_string),
        }
    }
}

/// An audio stream of the graph and where it plays.
struct Stream<'a> {
    node: &'a Value,
    id: u64,
    running: bool,
    path: Path<'a>,
}

/// Where a stream plays.
enum Path<'a> {
    /// Into the sink.
    Direct,
    /// Into the computer EQ's filter, which feeds the sink.
    ThroughFilter,
    /// Into another sink.
    Elsewhere(&'a Value),
    /// Into no sink.
    Unlinked,
}

impl Stream<'_> {
    /// Whether the stream plays into the sink, directly or through the filter.
    fn reaches(&self) -> bool {
        matches!(self.path, Path::Direct | Path::ThroughFilter)
    }
}

/// The player's stream out of `own`, the streams that carry its pid or its name, and how it
/// was found as `how`: one that reaches the sink (running ones first, then the lowest id),
/// else `Elsewhere` the sink the same choice among the others plays into, a stream that plays
/// into a sink before one linked to none. `None` when `own`
/// is empty, so the next way of finding it is tried; a player whose streams are all linked to
/// no sink is `NotFound`, with no assumption made about another stream.
fn settle<'s, 'a>(
    own: Vec<&'s Stream<'a>>,
    how: PlayerMatch,
) -> Option<(PlayerMatch, Option<&'s Stream<'a>>)> {
    let first = |reaching: bool| {
        own.iter()
            .copied()
            .filter(|stream| stream.reaches() == reaching)
            .min_by_key(|stream| {
                (
                    matches!(stream.path, Path::Unlinked),
                    !stream.running,
                    stream.id,
                )
            })
    };
    if own.is_empty() {
        return None;
    }
    if let Some(stream) = first(true) {
        return Some((how, Some(stream)));
    }
    let elsewhere = first(false).and_then(|stream| match stream.path {
        Path::Elsewhere(sink) => Some(PlayerMatch::Elsewhere {
            sink: sink_name(sink),
        }),
        _ => None,
    });
    Some((elsewhere.unwrap_or(PlayerMatch::NotFound), None))
}

/// What a sink is called for people: its description, else its node name.
fn sink_name(node: &Value) -> String {
    let named = |key| prop(node, key).filter(|name| !name.is_empty());
    named("node.description")
        .or_else(|| named("node.name"))
        .unwrap_or("")
        .to_string()
}

/// `facts` with the computer EQ's node in the path counted: its volume multiplies the sink's
/// and its mute adds to it. `sink` is the sink node `facts` came from.
fn apply_filter(facts: &mut SinkFacts, sink: Option<&Value>, filter: &Value) {
    if let (Some(sink), Some(filter_volume)) = (sink, linear_volume(filter)) {
        let sink_volume = linear_volume(sink).unwrap_or(1.0);
        facts.volume_milli = Some(milli_of(sink_volume * filter_volume));
    }
    facts.muted |= muted(filter);
}

/// The level of `sink`, with the chain node `filter` counted when it is in the path: a node
/// without channel volumes counts as full.
fn level_of(sink: &Value, filter: Option<&Value>) -> Option<SinkLevel> {
    let volumes = (linear_volume(sink), filter.and_then(linear_volume));
    let linear = match volumes {
        (None, None) => return None,
        (sink, filter) => sink.unwrap_or(1.0) * filter.unwrap_or(1.0),
    };
    Some(SinkLevel {
        linear,
        muted: muted(sink) || filter.is_some_and(muted),
    })
}

fn same(a: &Value, b: &Value) -> bool {
    node_id(a).is_some() && node_id(a) == node_id(b)
}

fn node_id(node: &Value) -> Option<u64> {
    node.get("id").and_then(Value::as_u64)
}

fn state(node: &Value) -> Option<&str> {
    node["info"]["state"].as_str()
}

/// The node's string property `key`.
fn prop<'v>(object: &'v Value, key: &str) -> Option<&'v str> {
    object["info"]["props"][key].as_str()
}

/// The object's property `key` as a number, whether PipeWire printed it as one or as a string.
fn prop_u64(object: &Value, key: &str) -> Option<u64> {
    match &object["info"]["props"][key] {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn is_class(node: &Value, class: &str) -> bool {
    prop(node, "media.class").is_some_and(|found| found == class)
}

fn is_filter_output(node: &Value) -> bool {
    prop(node, "node.name").is_some_and(|name| name == format!("{NODE_NAME}.output"))
}

/// Whether a stream's name and the player's identity name the same program: equal, or one
/// holds the other (`Mozilla Firefox` and `Firefox`), in any case; a name shorter than three
/// characters only matches in full.
fn names_match(identity: &str, name: &str) -> bool {
    let (identity, name) = (identity.to_lowercase(), name.trim().to_lowercase());
    if identity == name {
        return true;
    }
    let (short, long) = if identity.len() <= name.len() {
        (&identity, &name)
    } else {
        (&name, &identity)
    };
    short.chars().count() >= 3 && long.contains(short.as_str())
}

/// The rate and format name of the node's negotiated `Format`; for a stream (`rate_fallback`)
/// else the rate of its `node.rate` (`1/44100`) with no format name. `None` when it has neither.
fn first_format(node: &Value, rate_fallback: bool) -> Option<(Option<u32>, Option<String>)> {
    if let Some(format) = node["info"]["params"]["Format"].get(0) {
        let rate = format["rate"]
            .as_u64()
            .and_then(|rate| u32::try_from(rate).ok());
        let name = format["format"].as_str().map(str::to_string);
        if rate.is_some() || name.is_some() {
            return Some((rate, name));
        }
    }
    if !rate_fallback {
        return None;
    }
    let rate = prop(node, "node.rate")?
        .split_once('/')?
        .1
        .trim()
        .parse()
        .ok()?;
    Some((Some(rate), None))
}

fn stream_facts(node: &Value) -> StreamFacts {
    let format = first_format(node, true);
    StreamFacts {
        rate_hz: format.as_ref().and_then(|format| format.0),
        format: format.and_then(|format| format.1),
        channels: node["info"]["params"]["Format"]
            .get(0)
            .and_then(|format| format["channels"].as_u64())
            .and_then(|channels| u32::try_from(channels).ok()),
        volume_milli: linear_volume(node).map(milli_of),
        muted: muted(node),
    }
}

/// The node's `Props` parameter, the entry that holds its volumes.
fn props_param(node: &Value) -> Option<&Value> {
    node["info"]["params"]["Props"]
        .as_array()?
        .iter()
        .find(|entry| entry.get("channelVolumes").is_some())
}

/// The node's master `volume` (1.0 when it has none) times the lowest of its channel volumes,
/// linear (1.0 = full); `None` without any channel volume.
fn linear_volume(node: &Value) -> Option<f64> {
    let props = props_param(node)?;
    let channel = props["channelVolumes"]
        .as_array()?
        .iter()
        .filter_map(Value::as_f64)
        .reduce(f64::min)?;
    Some(props["volume"].as_f64().unwrap_or(1.0) * channel)
}

/// Whether the node is muted, by its hard `mute` or its `softMute`.
fn muted(node: &Value) -> bool {
    props_param(node).is_some_and(|props| {
        ["mute", "softMute"]
            .iter()
            .any(|key| props[key].as_bool() == Some(true))
    })
}

/// A linear volume on the cube-root scale `wpctl` prints, in thousandths.
fn milli_of(linear: f64) -> u32 {
    let cubic = linear.max(0.0).cbrt() * 1000.0;
    // Rounded, and capped far above anything a sink reports (PipeWire's own limit is 10.0).
    cubic.round().min(f64::from(u32::MAX)) as u32
}

/// A USB id as PipeWire prints it: `0x30be`, or decimal.
fn usb_id(text: &str) -> Option<u16> {
    let text = text.trim();
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u16::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

#[cfg(test)]
mod tests;
