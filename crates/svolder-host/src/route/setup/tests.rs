// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The setup facts read from `pw-dump` fixtures: real output, and the files derived from it
//! (see `tests/fixtures/pw-dump/README.md`), with small edits made in the test where a shape
//! has no file of its own.

use serde_json::{Value, json};

use super::*;
use crate::route::{RouteFacts, route};

const SCHIIT: &str = include_str!("../../../tests/fixtures/pw-dump/schiit-sink.json");
const NARROW: &str = include_str!("../../../tests/fixtures/pw-dump/narrow-sink.json");
const PLUG: &str = include_str!("../../../tests/fixtures/pw-dump/plug-sink.json");
const RATE_ENUM: &str = include_str!("../../../tests/fixtures/pw-dump/rate-enum-sink.json");
const MONO: &str = include_str!("../../../tests/fixtures/pw-dump/mono-sink.json");
const ONE_STREAM: &str = include_str!("../../../tests/fixtures/pw-dump/one-stream.json");

/// The `pw-cat` client's pid in the schiit and `one-stream` fixtures.
const PW_CAT_PID: u32 = 1_790_719;
const SCHIIT_SINK: &str = "alsa_output.usb-Schiit_Audio_Gungnir_2-00.analog-stereo";
const HDMI_SINK: &str = "alsa_output.pci-0000_11_00.1.hdmi-stereo";

fn setup_of(dump: &str) -> SetupFacts {
    route(dump, Some(PW_CAT_PID), "")
        .expect("the fixture is a pw-dump")
        .setup
        .expect("this build always reads the setup")
}

fn objects(dump: &str) -> Vec<Value> {
    serde_json::from_str(dump).expect("valid JSON")
}

fn text(objects: &[Value]) -> String {
    serde_json::to_string(objects).expect("it serialises")
}

/// `dump` with the node named `node_name` edited by `edit`.
fn edited(dump: &str, node_name: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut all = objects(dump);
    let node = all
        .iter_mut()
        .find(|object| object["info"]["props"]["node.name"] == node_name)
        .expect("the node is in the fixture");
    edit(node);
    text(&all)
}

/// `dump` with the default sink naming `name`.
fn default_sink(dump: &str, name: &str) -> String {
    let mut all = objects(dump);
    for object in &mut all {
        if object["props"]["metadata.name"] != "default" {
            continue;
        }
        for entry in object["metadata"].as_array_mut().expect("entries") {
            if entry["key"] == "default.audio.sink" {
                entry["value"] = json!({ "name": name });
            }
        }
    }
    text(&all)
}

/// `dump` with the `settings` metadata object added, holding `entries` of `(key, value)`.
fn with_settings(dump: &str, entries: &[(&str, Value)]) -> String {
    let mut all = objects(dump);
    let entries: Vec<Value> = entries
        .iter()
        .map(|(key, value)| json!({"subject": 0, "key": key, "type": "", "value": value}))
        .collect();
    all.push(json!({"id": 9001, "type": "PipeWire:Interface:Metadata",
        "props": {"metadata.name": "settings"}, "metadata": entries}));
    text(&all)
}

fn layout(positions: &[&str]) -> Layout {
    Layout {
        channels: positions.len() as u32,
        positions: positions.iter().map(|p| p.to_string()).collect(),
    }
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// The converter's flat `params` array of the sink node, to edit.
fn converter_params(node: &mut Value) -> &mut Vec<Value> {
    node["info"]["params"]["Props"][0]["params"]
        .as_array_mut()
        .expect("the converter's params")
}

fn set_param(params: &mut [Value], key: &str, value: Value) {
    let at = params
        .iter()
        .position(|item| item == key)
        .expect("the key is in the params");
    params[at + 1] = value;
}

// (1) the clock

#[test]
fn the_clock_is_read_from_the_settings_metadata() {
    let dump = with_settings(
        SCHIIT,
        &[
            ("clock.rate", json!(48_000)),
            ("clock.allowed-rates", json!("[ 44100 48000 96000 ]")),
            ("clock.quantum", json!(1024)),
            ("clock.force-rate", json!(96_000)),
        ],
    );
    assert_eq!(
        setup_of(&dump).clock,
        Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: vec![44_100, 48_000, 96_000],
            force_rate: Some(96_000),
        })
    );
}

#[test]
fn a_force_rate_of_zero_is_not_forced() {
    let dump = with_settings(
        SCHIIT,
        &[
            ("clock.rate", json!(48_000)),
            ("clock.allowed-rates", json!("[ 48000 ]")),
            ("clock.force-rate", json!(0)),
        ],
    );
    let clock = setup_of(&dump).clock.expect("settings were read");
    assert_eq!(clock.force_rate, None);
    assert_eq!(clock.allowed_rates, vec![48_000]);
}

#[test]
fn a_dump_without_settings_has_no_clock() {
    assert_eq!(setup_of(SCHIIT).clock, None);
}

// (2) what the sink offers

#[test]
fn the_offer_of_a_usb_sink_is_its_formats_a_rate_range_and_stereo() {
    assert_eq!(
        setup_of(SCHIIT).offer,
        Some(SinkOffer {
            formats: names(&["S32LE", "S24LE", "S16LE"]),
            rates: Some(Rates::Range {
                min: 44_100,
                max: 384_000
            }),
            layout: Some(layout(&["FL", "FR"])),
        })
    );
}

#[test]
fn the_iec958_entry_of_a_hdmi_sink_is_skipped() {
    let offer = setup_of(&default_sink(SCHIIT, HDMI_SINK))
        .offer
        .expect("the raw entry");
    assert_eq!(offer.formats, names(&["S32LE", "S16LE"]));
    assert_eq!(
        offer.rates,
        Some(Rates::Range {
            min: 32_000,
            max: 192_000
        })
    );
}

#[test]
fn the_offer_of_a_null_sink_is_a_planar_format_and_a_huge_range() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["params"]["EnumFormat"][0]["format"] =
            json!({"default": "F32P", "alt1": "F32P", "alt2": "F32LE"});
        node["info"]["params"]["EnumFormat"][0]["rate"] =
            json!({"default": 48_000, "min": 1, "max": 2_147_483_647});
    });
    let offer = setup_of(&dump).offer.expect("an offer");
    assert_eq!(offer.formats, names(&["F32P", "F32LE"]));
    assert_eq!(
        offer.rates,
        Some(Rates::Range {
            min: 1,
            max: 2_147_483_647
        })
    );
}

/// A sink that lists its iec958 entry first still offers its raw entry's values: the entry is
/// found by subtype, not by position.
#[test]
fn the_raw_entry_is_found_by_subtype_not_position() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        let entries = node["info"]["params"]["EnumFormat"]
            .as_array_mut()
            .expect("entries");
        entries.insert(
            0,
            json!({"mediaType": "audio", "mediaSubtype": "iec958",
                "iec958Codec": {"default": "PCM", "alt1": "PCM"}}),
        );
    });
    assert_eq!(setup_of(&dump).offer, setup_of(SCHIIT).offer);
    let hdmi = default_sink(SCHIIT, HDMI_SINK);
    let swapped = edited(&hdmi, HDMI_SINK, |node| {
        node["info"]["params"]["EnumFormat"]
            .as_array_mut()
            .expect("entries")
            .reverse();
    });
    assert_eq!(setup_of(&swapped).offer, setup_of(&hdmi).offer);
}

#[test]
fn a_pipe_sinks_plain_values_are_read_as_themselves() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["params"]["EnumFormat"][0]["format"] = json!("S24LE");
        node["info"]["params"]["EnumFormat"][0]["rate"] = json!(48_000);
    });
    let offer = setup_of(&dump).offer.expect("an offer");
    assert_eq!(offer.formats, names(&["S24LE"]));
    assert_eq!(offer.rates, Some(Rates::One(48_000)));
}

#[test]
fn a_rate_enum_is_a_list_with_the_default_first_and_no_repeats() {
    let offer = setup_of(RATE_ENUM).offer.expect("an offer");
    assert_eq!(offer.rates, Some(Rates::List(vec![48_000, 44_100, 96_000])));
}

#[test]
fn a_sink_with_no_enum_format_has_no_offer() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["params"]["EnumFormat"] = json!([]);
    });
    assert_eq!(setup_of(&dump).offer, None);
}

// layouts

#[test]
fn the_sinks_layout_is_its_negotiated_format() {
    let setup = setup_of(SCHIIT);
    assert_eq!(setup.sink_layout, Some(layout(&["FL", "FR"])));
    let mono = setup_of(MONO);
    assert_eq!(mono.sink_layout, Some(layout(&["MONO"])));
}

#[test]
fn a_suspended_sink_has_no_negotiated_layout() {
    let setup = setup_of(&default_sink(SCHIIT, HDMI_SINK));
    assert_eq!(setup.sink_layout, None);
}

#[test]
fn the_players_stream_layout_is_its_format() {
    assert_eq!(setup_of(SCHIIT).stream_layout, Some(layout(&["FL", "FR"])));
}

#[test]
fn a_stream_with_no_format_reads_its_enum_format() {
    let dump = edited(SCHIIT, "pw-cat", |node| {
        node["info"]["params"]["Format"] = json!([]);
        node["info"]["params"]["EnumFormat"] = json!([{"mediaType": "audio",
            "mediaSubtype": "raw", "format": "S16LE", "rate": 44_100, "channels": 1,
            "position": ["MONO"]}]);
    });
    assert_eq!(setup_of(&dump).stream_layout, Some(layout(&["MONO"])));
}

#[test]
fn no_player_stream_means_no_stream_layout() {
    let found = route(SCHIIT, None, "")
        .expect("a dump")
        .setup
        .expect("setup");
    assert_eq!(found.stream_layout, None);
    assert_eq!(found.stream_filter, None);
}

// (3) dither

#[test]
fn the_sink_converters_dither_is_read_from_its_params() {
    assert_eq!(
        setup_of(SCHIIT).dither,
        Some(Dither {
            method: "none".into(),
            noise: 0
        })
    );
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        set_param(converter_params(node), "dither.method", json!("triangular"));
    });
    assert_eq!(
        setup_of(&dump).dither,
        Some(Dither {
            method: "triangular".into(),
            noise: 0
        })
    );
}

#[test]
fn a_streams_dither_comes_from_its_params_not_its_props() {
    let mut all = objects(ONE_STREAM);
    let node = all
        .iter_mut()
        .find(|object| object["info"]["props"]["node.name"] == "pw-cat")
        .expect("the stream");
    node["info"]["props"]["dither.method"] = json!("triangular");
    set_param(converter_params(node), "dither.noise", json!(2));
    assert_eq!(
        dither_of(node),
        Some(Dither {
            method: "none".into(),
            noise: 2
        })
    );
}

#[test]
fn a_node_with_no_converter_params_has_no_dither() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["params"]["Props"][0]["params"] = json!([]);
    });
    assert_eq!(setup_of(&dump).dither, None);
}

#[test]
fn a_dither_with_no_noise_param_is_unknown() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        let params = converter_params(node);
        let at = params
            .iter()
            .position(|item| item == "dither.noise")
            .expect("key");
        params.drain(at..at + 2);
    });
    assert_eq!(setup_of(&dump).dither, None);
}

// (4) filters

/// PipeWire before 1.4 lists no `audioconvert.filter-graph` params: the converter was read and
/// has no graph.
#[test]
fn a_converter_without_the_graph_params_has_no_filter() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        let params = converter_params(node);
        for key in [
            "audioconvert.filter-graph.disable",
            "audioconvert.filter-graph",
        ] {
            let at = params.iter().position(|item| item == key).expect("key");
            params.drain(at..at + 2);
        }
    });
    assert_eq!(setup_of(&dump).sink_filter, Some(false));
}

#[test]
fn a_filter_whose_node_was_not_read_is_unknown_not_absent() {
    let no_props = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["props"] = Value::Null;
    });
    assert_eq!(setup_of(&no_props).sink_filter, None);
    let no_params = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["params"]["Props"] = json!([]);
    });
    assert_eq!(setup_of(&no_params).sink_filter, None);
}

#[test]
fn a_sink_without_a_filter_graph_reads_false() {
    let setup = setup_of(SCHIIT);
    assert_eq!(setup.sink_filter, Some(false));
    assert_eq!(setup.stream_filter, Some(false));
}

#[test]
fn a_filter_graph_prop_on_the_node_is_a_filter() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["props"]["audioconvert.filter-graph.0"] =
            json!("{ nodes = [ { type = builtin name = g label = mixer } ] }");
    });
    assert_eq!(setup_of(&dump).sink_filter, Some(true));
}

#[test]
fn a_filter_graph_param_is_a_filter_when_not_empty() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        set_param(
            converter_params(node),
            "audioconvert.filter-graph",
            json!("{ nodes = [ ] }"),
        );
    });
    assert_eq!(setup_of(&dump).sink_filter, Some(true));
}

#[test]
fn a_disabled_filter_graph_is_no_filter() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["props"]["audioconvert.filter-graph.0"] = json!("{ nodes = [ ] }");
        set_param(
            converter_params(node),
            "audioconvert.filter-graph.disable",
            json!(true),
        );
    });
    assert_eq!(setup_of(&dump).sink_filter, Some(false));
    let by_prop = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["props"]["audioconvert.filter-graph.0"] = json!("{ nodes = [ ] }");
        node["info"]["props"]["audioconvert.filter-graph.disable"] = json!(true);
    });
    assert_eq!(setup_of(&by_prop).sink_filter, Some(false));
}

#[test]
fn the_disable_switch_alone_is_not_a_filter() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["props"]["audioconvert.filter-graph.disable"] = json!(false);
    });
    assert_eq!(setup_of(&dump).sink_filter, Some(false));
}

#[test]
fn a_filter_in_the_players_stream_is_its_own_fact() {
    let dump = edited(SCHIIT, "pw-cat", |node| {
        node["info"]["props"]["audioconvert.filter-graph.0"] = json!("{ nodes = [ ] }");
    });
    let setup = setup_of(&dump);
    assert_eq!(setup.stream_filter, Some(true));
    assert_eq!(setup.sink_filter, Some(false));
}

// (5) the ALSA hop

#[test]
fn a_usb_sinks_alsa_hop_names_its_path_card_and_device() {
    let hop = setup_of(SCHIIT).alsa.expect("an ALSA sink");
    assert_eq!(hop.path.as_deref(), Some("hw:Audio"));
    assert_eq!(hop.card, Some(1));
    assert_eq!(hop.device, Some(0));
    assert_eq!(hop.subdevice, Some(0));
    assert_eq!(hop.hw, HwState::NotRead);
    assert_eq!(hop.kernel, None);
}

#[test]
fn a_plug_path_is_read_from_the_node_and_the_follower() {
    let hop = setup_of(PLUG).alsa.expect("an ALSA sink");
    assert_eq!(hop.path.as_deref(), Some("plughw:1"));
}

#[test]
fn the_followers_device_is_the_path_when_the_node_has_none() {
    let dump = edited(PLUG, SCHIIT_SINK, |node| {
        node["info"]["props"]
            .as_object_mut()
            .expect("props")
            .remove("api.alsa.path");
    });
    assert_eq!(
        setup_of(&dump).alsa.and_then(|hop| hop.path).as_deref(),
        Some("plughw:1")
    );
}

#[test]
fn the_pcm_device_number_is_read_when_the_node_has_one() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        node["info"]["props"]["api.alsa.pcm.device"] = json!(2);
    });
    assert_eq!(setup_of(&dump).alsa.and_then(|hop| hop.device), Some(2));
}

#[test]
fn a_sink_that_is_not_alsa_has_no_alsa_hop() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        let props = node["info"]["props"].as_object_mut().expect("props");
        props.remove("device.api");
        props.remove("api.alsa.path");
        props.insert("pipe.filename".into(), json!("/run/pipe"));
    });
    assert_eq!(setup_of(&dump).alsa, None);
}

// (6) the follower's own settings

#[test]
fn an_unforced_follower_has_no_format_and_no_rates_of_its_own() {
    let setup = setup_of(SCHIIT);
    assert_eq!(setup.forced_format, None);
    assert!(setup.node_rates.is_empty());
}

#[test]
fn a_forced_format_and_allowed_rates_on_the_follower_are_read() {
    let dump = edited(SCHIIT, SCHIIT_SINK, |node| {
        let params = node["info"]["params"]["Props"][1]["params"]
            .as_array_mut()
            .expect("the follower's params");
        set_param(params, "audio.format", json!("S16LE"));
        set_param(params, "audio.allowed-rates", json!("[ 44100 48000 ]"));
    });
    let setup = setup_of(&dump);
    assert_eq!(setup.forced_format.as_deref(), Some("S16LE"));
    assert_eq!(setup.node_rates, vec![44_100, 48_000]);
}

// (7) the whole

#[test]
fn every_dump_gets_setup_facts_even_with_no_sink() {
    let found: RouteFacts = route("[]", None, "").expect("an empty graph is a graph");
    assert_eq!(found.setup, Some(SetupFacts::default()));
}

#[test]
fn a_narrow_sinks_format_is_in_the_sink_facts() {
    let found = route(NARROW, Some(PW_CAT_PID), "").expect("a dump");
    assert_eq!(
        found.sink.and_then(|sink| sink.format).as_deref(),
        Some("S16LE")
    );
}

// the real capture from a private PipeWire: pipe sinks, a player stream, `settings`

const PIPE: &str = include_str!("../../../tests/fixtures/pw-dump/setup-pipe.json");
/// The `pw-cat` client's pid in `setup-pipe.json`.
const PIPE_PID: u32 = 3_825_110;

fn pipe_setup(sink: &str) -> SetupFacts {
    route(&default_sink(PIPE, sink), Some(PIPE_PID), "")
        .expect("a dump")
        .setup
        .expect("setup")
}

#[test]
fn the_real_settings_give_the_clock() {
    assert_eq!(
        pipe_setup("bits_s16_tri").clock,
        Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: vec![44_100, 48_000, 96_000],
            force_rate: Some(96_000),
        })
    );
}

#[test]
fn a_real_pipe_sink_with_triangular_dither_and_a_stream_with_noise() {
    let setup = pipe_setup("bits_s16_tri");
    assert_eq!(
        setup.dither,
        Some(Dither {
            method: "triangular".into(),
            noise: 0
        })
    );
    assert_eq!(setup.sink_layout, Some(layout(&["FL", "FR"])));
    assert_eq!(setup.stream_layout, Some(layout(&["FL", "FR"])));
    let offer = setup.offer.expect("an offer");
    assert_eq!(offer.formats, names(&["S16LE"]));
    assert_eq!(offer.rates, Some(Rates::One(48_000)));
    assert_eq!(setup.alsa, None);
    assert_eq!(setup.sink_filter, Some(false));
    assert_eq!(setup.stream_filter, Some(false));
    let all = objects(PIPE);
    let stream = all
        .iter()
        .find(|object| object["info"]["props"]["node.name"] == "bits_player")
        .expect("the player's stream");
    assert_eq!(
        dither_of(stream),
        Some(Dither {
            method: "none".into(),
            noise: 2
        })
    );
}

#[test]
fn a_real_pipe_sink_with_a_filter_graph_reads_a_filter() {
    assert_eq!(pipe_setup("bits_s16_graph").sink_filter, Some(true));
}

#[test]
fn the_real_null_sink_offers_a_planar_format_and_a_huge_range() {
    let offer = pipe_setup("svolder_e2e_sink").offer.expect("an offer");
    assert_eq!(offer.formats, names(&["F32P", "F32LE"]));
    assert_eq!(
        offer.rates,
        Some(Rates::Range {
            min: 1,
            max: 2_147_483_647
        })
    );
}
