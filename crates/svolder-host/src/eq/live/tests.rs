// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use super::*;
use crate::eq::render_conf;

fn pk(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::Peaking,
        f,
        g,
        q,
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn one_point() -> Points {
    Points {
        filters: vec![pk(1000.0, 6.0, 1.0)],
        slots: vec![1],
    }
}

#[test]
fn the_live_conf_has_sixteen_slots_per_channel_and_the_preamp_first() {
    let conf = render_live_conf(&one_point(), -6.0, "sink", Route::Smart, 48_000);
    let graph = conf
        .split("                nodes = [\n")
        .nth(1)
        .expect("the graph is there");
    let lines: Vec<&str> = graph.lines().collect();
    assert_eq!(
        lines[0],
        "                    { type = builtin name = lpre label = linear control = { Mult = 0.5011872336272722 Add = 0.0 } }"
    );
    assert_eq!(
        lines[1],
        "                    { type = builtin name = l01 label = bq_raw config = { coefficients = [ { rate = 48000 b0 = 1.043953086990335 b1 = -1.8953207239365961 b2 = 0.8677222847598566 a0 = 1 a1 = -1.8953207239365961 a2 = 0.9116753717501915 } ] } }"
    );
    assert_eq!(
        lines[2],
        "                    { type = builtin name = l02 label = bq_raw config = { coefficients = [ { rate = 48000 b0 = 1 b1 = 0 b2 = 0 a0 = 1 a1 = 0 a2 = 0 } ] } }"
    );
    // lpre, 16 slots, rpre, 16 slots, then the end of the list.
    assert!(lines[16].contains("name = l16 "));
    assert!(lines[17].contains("name = rpre "));
    assert!(lines[18].contains("name = r01 "));
    assert!(lines[33].contains("name = r16 "));
    // Then the crossfeed stage: six nodes a channel, after both channels' slots.
    assert!(lines[34].contains("name = ldir "));
    assert!(lines[39].contains("name = lmix "));
    assert!(lines[40].contains("name = rdir "));
    assert!(lines[45].contains("name = rmix "));
    assert_eq!(lines[46], "                ]");
    assert_eq!(lines[47], "                links = [");
    assert_eq!(
        lines[48],
        "                    { output = \"lpre:Out\" input = \"l01:In\" }"
    );
    assert_eq!(
        lines[63],
        "                    { output = \"l15:Out\" input = \"l16:In\" }"
    );
    assert_eq!(
        lines[64],
        "                    { output = \"rpre:Out\" input = \"r01:In\" }"
    );
    // The stage's links follow the slots'; then the end of the list, the inputs and the outputs.
    assert_eq!(lines[96], "                ]");
    assert_eq!(
        lines[97],
        "                inputs  = [ \"lpre:In\" \"rpre:In\" ]"
    );
    assert_eq!(
        lines[98],
        "                outputs = [ \"lmix:Out\" \"rmix:Out\" ]"
    );
    assert_eq!(lines[99], "            }");
    // Round the graph it is the frame every chain has.
    assert!(conf.contains("node.name = \"svolder_eq\""));
    assert!(conf.contains("filter.smart.target = { node.name = \"sink\" }"));
    let frame = render_conf(
        &svolder_profiles::tone::apo::Correction {
            preamp_db: 0.0,
            filters: Vec::new(),
        },
        "sink",
        Route::Smart,
    );
    let after = |text: &str| text.split("audio.channels = 2").nth(1).unwrap().to_string();
    assert_eq!(after(&conf), after(&frame));
}

#[test]
fn a_point_sits_in_the_slot_it_was_given() {
    let points = Points {
        filters: vec![pk(1000.0, 6.0, 1.0)],
        slots: vec![7],
    };
    let conf = render_live_conf(&points, 0.0, "sink", Route::Default, 48_000);
    let slot = |name: &str| {
        conf.lines()
            .find(|line| line.contains(&format!("name = {name} ")))
            .unwrap()
            .to_string()
    };
    assert!(slot("l07").contains("b0 = 1.0439"));
    assert!(slot("r07").contains("b0 = 1.0439"));
    assert!(slot("l01").contains("b0 = 1 b1 = 0"));
    assert!(conf.contains("target.object = \"sink\""));
}

#[test]
fn an_empty_slot_is_transparent() {
    let params = slot_params(5, None, 48_000);
    match SHAPE {
        SlotShape::Raw => {
            let left: Vec<(&str, f64)> =
                params[..6].iter().map(|(n, v)| (n.as_str(), *v)).collect();
            assert_eq!(
                left,
                vec![
                    ("l05:b0", 1.0),
                    ("l05:b1", 0.0),
                    ("l05:b2", 0.0),
                    ("l05:a0", 1.0),
                    ("l05:a1", 0.0),
                    ("l05:a2", 0.0),
                ]
            );
        }
        SlotShape::Typed => assert!(params.contains(&("l05:Gain".to_string(), 0.0))),
    }
    assert_eq!(params.len(), 12);
    assert!(params[6..].iter().all(|(name, _)| name.starts_with("r05:")));
}

#[test]
fn raw_coefficients_match_the_cookbook() {
    let params = slot_params(7, Some(&pk(1000.0, 6.0, 1.0)), 48_000);
    let value = |name: &str| params.iter().find(|(n, _)| n == name).unwrap().1;
    // The cookbook's +6 dB peak at 1 kHz, Q 1 at 48 kHz, worked out by hand:
    // b0 1.092187, b1 -1.982890, b2 0.907813, a0 1.046203, a1 -1.982890, a2 0.953797.
    for channel in ["l07", "r07"] {
        assert!(near(value(&format!("{channel}:b0")), 1.092187 / 1.046203));
        assert!(near(value(&format!("{channel}:b1")), -1.982890 / 1.046203));
        assert!(near(value(&format!("{channel}:b2")), 0.907813 / 1.046203));
        assert_eq!(value(&format!("{channel}:a0")), 1.0);
        assert!(near(value(&format!("{channel}:a1")), -1.982890 / 1.046203));
        assert!(near(value(&format!("{channel}:a2")), 0.953797 / 1.046203));
    }
    // At 44.1 kHz the same filter is another set of numbers.
    let other = slot_params(7, Some(&pk(1000.0, 6.0, 1.0)), 44_100);
    assert!(!near(other[0].1, params[0].1) || !near(other[1].1, params[1].1));
}

/// The five numbers `slot_params` sends for the left channel's `b0 b1 b2 a1 a2`.
fn five(params: &[(String, f64)]) -> Vec<f64> {
    [0, 1, 2, 4, 5].iter().map(|i| params[*i].1).collect()
}

fn all_near(got: &[f64], want: &[f64]) -> bool {
    got.len() == want.len() && got.iter().zip(want).all(|(a, b)| near(*a, *b))
}

#[test]
fn the_shelves_match_the_cookbook() {
    // RBJ cookbook worked out for 48 kHz, divided by a0: low shelf 100 Hz +6 dB Q 0.7 and
    // high shelf 8 kHz -4 dB Q 0.9.
    let low = SourceFilter {
        kind: FilterKind::LowShelf,
        f: 100.0,
        g: 6.0,
        q: 0.7,
    };
    let high = SourceFilter {
        kind: FilterKind::HighShelf,
        f: 8000.0,
        g: -4.0,
        q: 0.9,
    };
    assert!(all_near(
        &five(&slot_params(1, Some(&low), 48_000)),
        &[1.003250, -1.984209, 0.981199, -1.984269, 0.984389]
    ));
    assert!(all_near(
        &five(&slot_params(1, Some(&high), 48_000)),
        &[0.735024, -0.398779, 0.240448, -0.800673, 0.377366]
    ));
}

#[test]
fn a_value_outside_the_hand_plays_as_written() {
    // 14 dB low shelf with Q 15, and 22 kHz at 96 kHz: none of it is held.
    let shelf = SourceFilter {
        kind: FilterKind::LowShelf,
        f: 105.0,
        g: 14.0,
        q: 15.0,
    };
    let params = slot_params(1, Some(&shelf), 48_000);
    let [b0, b1, b2, a1, a2] = svolder_profiles::tone::response::coefficients(
        FilterKind::LowShelf,
        105.0,
        14.0,
        15.0,
        48_000,
    );
    assert_eq!(five(&params), vec![b0, b1, b2, a1, a2]);
    let high = pk(22_000.0, 6.0, 1.0);
    let at_96 = svolder_profiles::tone::response::coefficients(
        FilterKind::Peaking,
        22_000.0,
        6.0,
        1.0,
        96_000,
    );
    assert_eq!(five(&slot_params(1, Some(&high), 96_000)), at_96.to_vec());
}

#[test]
fn an_invalid_value_is_held_and_never_nan() {
    for (f, q) in [
        (0.0, 1.0),
        (-5.0, 1.0),
        (1000.0, 0.0),
        (1000.0, -2.0),
        (1e9, 1e9),
    ] {
        for kind in [
            FilterKind::Peaking,
            FilterKind::LowShelf,
            FilterKind::HighShelf,
        ] {
            let filter = SourceFilter { kind, f, g: 6.0, q };
            let params = slot_params(3, Some(&filter), 48_000);
            assert!(
                params.iter().all(|(_, v)| v.is_finite()),
                "{f} {q}: {params:?}"
            );
        }
    }
    // Held to the nearest valid value, not dropped to flat.
    assert_eq!(
        slot_params(1, Some(&pk(0.0, 6.0, 1.0)), 48_000),
        slot_params(1, Some(&pk(1.0, 6.0, 1.0)), 48_000)
    );
    assert_eq!(
        slot_params(1, Some(&pk(1000.0, 6.0, 0.0)), 48_000),
        slot_params(1, Some(&pk(1000.0, 6.0, 0.01)), 48_000)
    );
    assert_ne!(
        slot_params(1, Some(&pk(1000.0, 6.0, 1.0)), 48_000),
        slot_params(1, Some(&pk(1000.0, 9.0, 1.0)), 48_000),
        "the gain is not held"
    );
}

#[test]
fn the_preamp_is_a_linear_gain() {
    let params = preamp_params(-6.0);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].0, "lpre:Mult");
    assert_eq!(params[1].0, "rpre:Mult");
    assert!(near(params[0].1, 0.501187));
    assert_eq!(preamp_params(0.0)[0].1, 1.0);
}

#[test]
fn the_param_line_sets_both_channels_in_one_call() {
    let params = slot_params(7, None, 48_000);
    let line = set_param_line(42, &params);
    assert_eq!(
        line,
        "set-param 42 Props { params = [ \"l07:b0\" 1 \"l07:b1\" 0 \"l07:b2\" 0 \"l07:a0\" 1 \"l07:a1\" 0 \"l07:a2\" 0 \"r07:b0\" 1 \"r07:b1\" 0 \"r07:b2\" 0 \"r07:a0\" 1 \"r07:a1\" 0 \"r07:a2\" 0 ] }"
    );
    assert!(!line.contains('\n'));
    assert_eq!(
        set_param_line(3, &[("lpre:Mult".to_string(), -0.0)]),
        "set-param 3 Props { params = [ \"lpre:Mult\" 0 ] }"
    );
}

#[test]
fn the_node_is_read_from_the_live_chain_fixture() {
    let dump = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pw-dump/live-chain.json"
    ))
    .unwrap();
    let node = props_of(&dump).expect("the chain is in the dump");
    assert_eq!(node.id, 32);
    assert_eq!(node.rate_hz, Some(48_000));
    assert_eq!(node.params["lpre:Mult"], 0.5);
    assert!(near(node.params["l07:b0"], 1.043953));
    assert!(near(node.params["r07:a2"], 0.911675));
    // The slots that were not set read back as the config's flat point.
    assert_eq!(node.params["l01:b0"], 1.0);
    assert!(near(node.params["l01:b1"], -1.861408));
    assert!(!node.params.contains_key("channelmix.min-volume"));
}

#[test]
fn a_dump_without_the_chain_has_no_node() {
    let dump = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pw-dump/one-stream.json"
    ))
    .unwrap();
    assert_eq!(props_of(&dump), None);
    assert_eq!(props_of("not json"), None);
}

#[test]
fn a_frequency_just_under_nyquist_plays_as_written() {
    let wanted = svolder_profiles::tone::response::coefficients(
        FilterKind::Peaking,
        22_000.0,
        6.0,
        1.0,
        44_100,
    );
    assert_eq!(
        five(&slot_params(1, Some(&pk(22_000.0, 6.0, 1.0)), 44_100)),
        wanted.to_vec()
    );
}

#[test]
fn a_bogus_rate_cannot_panic_the_hold() {
    for rate in [0, 1, 2] {
        let params = slot_params(1, Some(&pk(1000.0, 6.0, 1.0)), rate);
        assert!(
            params.iter().all(|(_, v)| v.is_finite()),
            "{rate}: {params:?}"
        );
    }
}

#[test]
fn a_gain_that_is_not_a_number_is_zero_db() {
    for g in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let params = slot_params(1, Some(&pk(1000.0, g, 1.0)), 48_000);
        assert!(params.iter().all(|(_, v)| v.is_finite()), "{g}: {params:?}");
        assert_eq!(params, slot_params(1, Some(&pk(1000.0, 0.0, 1.0)), 48_000));
    }
}

/// A chain of this svolder lists its preamp control; the typed `param_eq` chain an older
/// svolder built (the `host-eq` fixture's shape: a node with no control ports) does not.
#[test]
fn a_node_without_the_preamp_control_is_a_chain_from_an_older_svolder() {
    let read = |name: &str| {
        let dump = std::fs::read_to_string(format!(
            "{}/tests/fixtures/pw-dump/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        props_of(&dump).expect("the chain is in the dump")
    };
    assert!(read("live-chain.json").has_preamp_control());
    assert!(!read("host-eq.json").has_preamp_control());
}

#[test]
fn a_zero_db_filter_is_the_exact_identity_whatever_the_rate() {
    for (kind, f, q, rate) in [
        (FilterKind::LowShelf, 10.0, 20.0, 192_000),
        (FilterKind::Peaking, 1.0, 100.0, 192_000),
        (FilterKind::Peaking, 10.0, 20.0, 192_000),
        (FilterKind::LowShelf, 10.0, 0.707, 192_000),
        (FilterKind::Peaking, 1000.0, 1.0, 48_000),
    ] {
        let filter = SourceFilter { kind, f, g: 0.0, q };
        let sent: Vec<f64> = slot_params(1, Some(&filter), rate)
            .into_iter()
            .take(6)
            .map(|(_, value)| value)
            .collect();
        assert_eq!(
            sent,
            vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            "{kind:?} {f} Hz at {rate}"
        );
        let (sets, _) = played_sets(
            &Correction {
                preamp_db: 0.0,
                filters: vec![filter],
            },
            rate,
        );
        assert_eq!(sets, vec![[1.0, 0.0, 0.0, 0.0, 0.0]]);
    }
    let boost = slot_params(1, Some(&pk(1000.0, 3.0, 1.0)), 48_000);
    assert_ne!(boost[1].1, 0.0, "a boost keeps its coefficients");
}

#[test]
fn the_default_sinks_rate_is_read_from_the_dump() {
    let dump = |default: &str| {
        format!(
            r#"[
 {{"id":31,"type":"PipeWire:Interface:Metadata","props":{{"metadata.name":"default"}},
   "metadata":[{{"key":"default.audio.sink","value":{{"name":"{default}"}}}}]}},
 {{"id":40,"type":"PipeWire:Interface:Node","info":{{"props":{{"node.name":"card"}},
   "params":{{"Format":[{{"rate":96000}}]}}}}}},
 {{"id":41,"type":"PipeWire:Interface:Node","info":{{"props":{{"node.name":"svolder_eq"}},
   "params":{{"Format":[{{"rate":48000}}]}}}}}}
]"#
        )
    };
    assert_eq!(default_sink_rate(&dump("card")), Some(96_000));
    assert_eq!(
        default_sink_rate(&dump("svolder_eq")),
        None,
        "the chain itself"
    );
    assert_eq!(default_sink_rate(&dump("gone")), None);
    assert_eq!(default_sink_rate("[]"), None);
}

/// The conf's lines between `from` (the first one containing it) and `to` (the first after it).
fn between<'a>(conf: &'a str, from: &str, to: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = conf.lines().collect();
    let start = lines.iter().position(|l| l.contains(from)).unwrap();
    let end = start + lines[start..].iter().position(|l| l.contains(to)).unwrap();
    lines[start..end].to_vec()
}

const OFF_STAGE_LEFT: &str = r#"                    { type = builtin name = ldir label = bq_raw config = { coefficients = [ { rate = 48000 b0 = 1 b1 = 0 b2 = 0 a0 = 1 a1 = 0 a2 = 0 } ] } }
                    { type = builtin name = lx label = bq_raw config = { coefficients = [ { rate = 48000 b0 = 0 b1 = 0 b2 = 0 a0 = 1 a1 = 0 a2 = 0 } ] } }
                    { type = builtin name = ls label = bq_raw config = { coefficients = [ { rate = 48000 b0 = 0 b1 = 0 b2 = 0 a0 = 1 a1 = 0 a2 = 0 } ] } }
                    { type = builtin name = lsum label = mixer control = { "Gain 1" = 1 "Gain 2" = 0 } }
                    { type = builtin name = ldel label = delay config = { "max-delay" = 0.002 } control = { "Delay (s)" = 0 } }
                    { type = builtin name = lmix label = mixer control = { "Gain 1" = 1 "Gain 2" = 0 } }"#;

#[test]
fn the_conf_always_holds_the_crossfeed_stage_off() {
    let conf = render_live_conf(&one_point(), -6.0, "sink", Route::Smart, 48_000);
    let left = between(&conf, "name = ldir ", "name = rdir ").join("\n");
    assert_eq!(left, OFF_STAGE_LEFT);
    let right = between(&conf, "name = rdir ", "                ]").join("\n");
    assert_eq!(right, OFF_STAGE_LEFT.replace("name = l", "name = r"));
    // A rate other than 48 kHz is the rate of the coefficients, the stage still off.
    let at_96 = render_live_conf(&one_point(), -6.0, "sink", Route::Default, 96_000);
    assert!(
        at_96.contains(
            "name = lx label = bq_raw config = { coefficients = [ { rate = 96000 b0 = 0 "
        )
    );
    assert!(
        at_96.contains("name = lmix label = mixer control = { \"Gain 1\" = 1 \"Gain 2\" = 0 }")
    );
}

#[test]
fn the_stage_links_tee_each_channel_to_both_outputs() {
    let conf = render_live_conf(&one_point(), 0.0, "sink", Route::Smart, 48_000);
    let link = |from: &str, to: &str| format!("{{ output = \"{from}\" input = \"{to}\" }}");
    for (from, to) in [
        ("l16:Out", "ldir:In"),
        ("l16:Out", "ls:In"),
        ("r16:Out", "lx:In"),
        ("lx:Out", "lsum:In 1"),
        ("ls:Out", "lsum:In 2"),
        ("lsum:Out", "ldel:In"),
        ("ldir:Out", "lmix:In 1"),
        ("ldel:Out", "lmix:In 2"),
        ("r16:Out", "rdir:In"),
        ("r16:Out", "rs:In"),
        ("l16:Out", "rx:In"),
        ("rx:Out", "rsum:In 1"),
        ("rs:Out", "rsum:In 2"),
        ("rsum:Out", "rdel:In"),
        ("rdir:Out", "rmix:In 1"),
        ("rdel:Out", "rmix:In 2"),
    ] {
        assert_eq!(conf.matches(&link(from, to)).count(), 1, "{from} -> {to}");
    }
    // The graph's outputs are the mixers, not the last slots.
    assert!(!conf.contains("outputs = [ \"l16:Out\""));
}

#[test]
fn a_given_stage_is_written_into_the_conf() {
    let settings = crate::eq::crossfeed::CrossfeedSettings::default();
    let stage = Stage::of(&settings, 48_000).unwrap();
    let conf = render_live_conf_with(&one_point(), 0.0, "sink", Route::Smart, 48_000, &stage);
    let dir = between(&conf, "name = ldir ", "name = lx ")[0];
    assert!(
        dir.contains(&format!("b0 = {}", num(stage.dir[0]))),
        "{dir}"
    );
    assert!(
        !dir.contains("b0 = 1 b1 = 0"),
        "the direct path is the Bauer boost: {dir}"
    );
    assert!(conf.contains("name = lmix label = mixer control = { \"Gain 1\" = 1 \"Gain 2\" = 1 }"));
    // The same text as the off conf apart from the stage's numbers.
    let off = render_live_conf(&one_point(), 0.0, "sink", Route::Smart, 48_000);
    assert_ne!(conf, off);
    assert_eq!(conf.lines().count(), off.lines().count());
}

#[test]
fn crossfeed_params_name_the_conf_ports_for_both_channels() {
    let params = crossfeed_params(&Stage::bypass());
    assert_eq!(params.len(), 2 * (3 * 6 + 4));
    let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        &names[..6],
        [
            "ldir:b0", "ldir:b1", "ldir:b2", "ldir:a0", "ldir:a1", "ldir:a2"
        ]
    );
    assert_eq!(
        &names[18..22],
        [
            "lsum:Gain 1",
            "lsum:Gain 2",
            "ldel:Delay (s)",
            "lmix:Gain 2"
        ]
    );
    assert_eq!(names[22], "rdir:b0");
    assert_eq!(names[43], "rmix:Gain 2");
    // Every port is a control the conf lists: node name, then a port of its builtin.
    let conf = render_live_conf(&one_point(), 0.0, "sink", Route::Smart, 48_000);
    for name in &names {
        let (node, _) = name.split_once(':').unwrap();
        assert!(conf.contains(&format!("name = {node} ")), "{name}");
    }
    // Bypass values: the identity direct path, nothing crossed, the mixer input off.
    let value = |name: &str| params.iter().find(|(n, _)| n == name).unwrap().1;
    assert_eq!(value("ldir:b0"), 1.0);
    assert_eq!(value("ldir:a0"), 1.0);
    assert_eq!(value("lx:b0"), 0.0);
    assert_eq!(value("lmix:Gain 2"), 0.0);
    assert_eq!(value("rdel:Delay (s)"), 0.0);
    // On: the stage's own numbers.
    let on = Stage::of(&crate::eq::crossfeed::CrossfeedSettings::default(), 48_000).unwrap();
    let params = crossfeed_params(&on);
    let value = |name: &str| params.iter().find(|(n, _)| n == name).unwrap().1;
    assert_eq!(value("rx:b0"), on.x[0]);
    assert_eq!(value("lx:a1"), on.x[3]);
    assert_eq!(value("lmix:Gain 2"), 1.0);
}

#[test]
fn the_param_line_quotes_the_names_with_blanks() {
    let line = set_param_line(
        7,
        &[
            ("lmix:Gain 2".to_string(), 1.0),
            ("ldel:Delay (s)".to_string(), 0.00025),
        ],
    );
    assert_eq!(
        line,
        "set-param 7 Props { params = [ \"lmix:Gain 2\" 1 \"ldel:Delay (s)\" 0.00025 ] }"
    );
    let all = set_param_line(1, &crossfeed_params(&Stage::bypass()));
    assert!(
        all.contains("\"lsum:Gain 1\" 1 \"lsum:Gain 2\" 0 \"ldel:Delay (s)\" 0 \"lmix:Gain 2\" 0")
    );
    assert!(!all.contains('\n'));
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/pw-dump/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn a_chain_from_0_34_has_the_preamp_and_no_crossfeed_controls() {
    let node = props_of(&fixture("live-chain.json")).unwrap();
    assert!(node.has_preamp_control());
    assert!(!node.has_crossfeed_controls());
}

#[test]
fn a_current_chain_lists_the_crossfeed_controls() {
    let node = props_of(&fixture("live-chain-crossfeed.json")).unwrap();
    assert!(node.has_preamp_control());
    assert!(node.has_crossfeed_controls());
    assert_eq!(node.params["lmix:Gain 2"], 0.0);
    assert_eq!(node.params["ldel:Delay (s)"], 0.0);
    assert_eq!(node.params["lsum:Gain 1"], 1.0);
    // One port missing is not a current chain.
    let mut partial = node.clone();
    partial.params.remove("rmix:Gain 2");
    assert!(!partial.has_crossfeed_controls());
}

#[test]
fn the_node_reads_the_sink_its_output_is_linked_to() {
    let node = props_of(&fixture("live-chain.json")).unwrap();
    assert_eq!(node.sink.as_deref(), Some("svolder_e2e_sink"));
    let node = props_of(&fixture("host-eq.json")).unwrap();
    assert_eq!(
        node.sink.as_deref(),
        Some("alsa_output.pci-0000_11_00.1.hdmi-stereo")
    );
    // A chain whose output is linked to nothing has no sink.
    let mut dump: serde_json::Value = serde_json::from_str(&fixture("host-eq.json")).unwrap();
    dump.as_array_mut().unwrap().retain(|o| {
        !o["type"].as_str().unwrap().ends_with("Link")
            || o["info"]["output-node-id"].as_u64() != Some(125)
    });
    assert_eq!(props_of(&dump.to_string()).unwrap().sink, None);
}

#[test]
fn the_output_sink_is_the_default_one_or_the_one_behind_the_chain() {
    let hdmi = "alsa_output.pci-0000_11_00.1.hdmi-stereo";
    // Smart route: the default is the real sink, the chain sits in front of it.
    let smart = output_sink(&fixture("host-eq.json")).unwrap();
    assert_eq!(smart.name, hdmi);
    assert!(smart.description.as_deref().unwrap().contains("HDMI"));
    // Default route: the default is the chain, the real sink is behind it.
    let behind = output_sink(&fixture("host-eq-default.json")).unwrap();
    assert_eq!(behind.name, hdmi);
    assert_eq!(behind.description, smart.description);
    // No chain: the default sink, with the rate it has.
    let switched = output_sink(&fixture("sink-switched.json")).unwrap();
    assert_eq!(
        switched.name,
        "alsa_output.usb-Generic_USB_Audio-00.HiFi__Speaker__sink"
    );
    assert_eq!(switched.description.as_deref(), Some("USB Audio Speakers"));
    assert_eq!(switched.rate_hz, Some(48_000));
    // The chain is the default and linked to nothing: no output to name.
    let mut dump: serde_json::Value =
        serde_json::from_str(&fixture("host-eq-default.json")).unwrap();
    dump.as_array_mut().unwrap().retain(|o| {
        !o["type"].as_str().unwrap().ends_with("Link")
            || o["info"]["output-node-id"].as_u64() != Some(125)
    });
    assert_eq!(output_sink(&dump.to_string()), None);
    assert_eq!(output_sink("[]"), None);
    assert_eq!(output_sink("not json"), None);
}

#[test]
fn the_output_sink_falls_back_to_the_nick_for_its_name() {
    let dump = r#"[
 {"id":31,"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"default"},
   "metadata":[{"key":"default.audio.sink","value":{"name":"card"}}]},
 {"id":40,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"card","node.nick":"Card nick"},
   "params":{"Format":[{"rate":96000}]}}}
]"#;
    let sink = output_sink(dump).unwrap();
    assert_eq!(sink.name, "card");
    assert_eq!(sink.description.as_deref(), Some("Card nick"));
    assert_eq!(sink.rate_hz, Some(96_000));
}

/// A dump with the chain's sink (id 10), its output stream (11) linked to a real sink (12), and
/// `extra` objects.
fn playing_dump(extra: Vec<serde_json::Value>) -> String {
    use serde_json::json;
    let node = |id: u64, name: &str, class: &str, state: &str| {
        json!({"id": id, "type": "PipeWire:Interface:Node",
               "info": {"state": state, "props": {"node.name": name, "media.class": class}}})
    };
    let mut objects = vec![
        node(10, NODE_NAME, "Audio/Sink", "running"),
        node(
            11,
            &format!("{NODE_NAME}.output"),
            "Stream/Output/Audio",
            "running",
        ),
        node(12, "real", "Audio/Sink", "running"),
        link_of(20, 11, 12),
    ];
    objects.extend(extra);
    serde_json::to_string(&objects).unwrap()
}

fn stream(id: u64, state: &str) -> serde_json::Value {
    serde_json::json!({"id": id, "type": "PipeWire:Interface:Node",
        "info": {"state": state, "props": {"node.name": format!("s{id}")}}})
}

fn link_of(id: u64, from: u64, to: u64) -> serde_json::Value {
    serde_json::json!({"id": id, "type": "PipeWire:Interface:Link",
        "info": {"output-node-id": from, "input-node-id": to, "state": "active"}})
}

#[test]
fn a_running_stream_linked_into_the_chain_is_playing() {
    let dump = playing_dump(vec![stream(30, "running"), link_of(40, 30, 10)]);
    assert!(props_of(&dump).unwrap().playing);
}

#[test]
fn a_paused_or_corked_stream_is_not_playing() {
    let dump = playing_dump(vec![stream(31, "idle"), link_of(41, 31, 10)]);
    assert!(!props_of(&dump).unwrap().playing);
    let dump = playing_dump(vec![stream(31, "suspended"), link_of(41, 31, 10)]);
    assert!(!props_of(&dump).unwrap().playing);
}

#[test]
fn a_capture_of_the_chains_monitor_is_not_playing() {
    // The capture's link runs from the chain's sink to the capture, whatever the states.
    let dump = playing_dump(vec![stream(32, "running"), link_of(42, 10, 32)]);
    assert!(!props_of(&dump).unwrap().playing);
}

#[test]
fn a_running_stream_linked_to_another_sink_is_not_playing_through_the_chain() {
    let dump = playing_dump(vec![stream(30, "running"), link_of(40, 30, 12)]);
    assert!(!props_of(&dump).unwrap().playing);
}

fn speakers_conf(on: bool) -> String {
    let graph = SpeakersGraph::new(Path::new("/state/svolder/speakers/sadie-d2.wav"), on).unwrap();
    render_live_conf_full(
        &one_point(),
        0.0,
        "sink",
        Route::Smart,
        48_000,
        &Stage::bypass(),
        Some(&graph),
    )
}

#[test]
fn the_speakers_graph_has_four_convolvers_and_two_mixers_after_the_crossfeed_stage() {
    let conf = speakers_conf(false);
    for (name, channel) in [("vLL", 0), ("vLR", 1), ("vRL", 2), ("vRR", 3)] {
        let node = format!(
            "{{ type = builtin name = {name} label = convolver config = {{ filename = \"/state/svolder/speakers/sadie-d2.wav\" channel = {channel} gain = 1 }} }}"
        );
        assert_eq!(conf.matches(&node).count(), 1, "{node}");
    }
    for name in ["lvs", "rvs"] {
        let node = format!(
            "{{ type = builtin name = {name} label = mixer control = {{ \"Gain 1\" = 1 \"Gain 2\" = 0 \"Gain 3\" = 0 }} }}"
        );
        assert_eq!(conf.matches(&node).count(), 1, "{node}");
    }
    let link = |from: &str, to: &str| format!("{{ output = \"{from}\" input = \"{to}\" }}");
    for (from, to) in [
        ("lmix:Out", "lvs:In 1"),
        ("rmix:Out", "rvs:In 1"),
        ("lmix:Out", "vLL:In"),
        ("rmix:Out", "vRL:In"),
        ("lmix:Out", "vLR:In"),
        ("rmix:Out", "vRR:In"),
        ("vLL:Out", "lvs:In 2"),
        ("vRL:Out", "lvs:In 3"),
        ("vLR:Out", "rvs:In 2"),
        ("vRR:Out", "rvs:In 3"),
    ] {
        assert_eq!(conf.matches(&link(from, to)).count(), 1, "{from} -> {to}");
    }
    assert!(conf.contains("outputs = [ \"lvs:Out\" \"rvs:Out\" ]"));
    assert!(!conf.contains("outputs = [ \"lmix:Out\""));
}

#[test]
fn a_chain_without_a_set_has_no_convolvers_and_the_old_outputs() {
    let conf = render_live_conf(&one_point(), 0.0, "sink", Route::Smart, 48_000);
    assert!(!conf.contains("convolver"));
    assert!(!conf.contains("lvs"));
    assert!(conf.contains("outputs = [ \"lmix:Out\" \"rmix:Out\" ]"));
}

#[test]
fn the_conf_text_apart_from_the_speakers_stage_is_the_plain_conf() {
    let plain = render_live_conf(&one_point(), 0.0, "sink", Route::Smart, 48_000);
    let with = speakers_conf(false);
    let kept: Vec<&str> = with
        .lines()
        .filter(|line| !line.contains("vLL") && !line.contains("vLR"))
        .filter(|line| !line.contains("vRL") && !line.contains("vRR"))
        .filter(|line| !line.contains("lvs") && !line.contains("rvs"))
        .filter(|line| !line.contains("outputs"))
        .collect();
    let plain: Vec<&str> = plain.lines().filter(|l| !l.contains("outputs")).collect();
    assert_eq!(kept, plain);
}

#[test]
fn a_conf_written_with_the_stage_on_plays_wet_only() {
    let conf = speakers_conf(true);
    assert!(conf.contains(
        "name = lvs label = mixer control = { \"Gain 1\" = 0 \"Gain 2\" = 1 \"Gain 3\" = 1 }"
    ));
    assert!(conf.contains(
        "name = rvs label = mixer control = { \"Gain 1\" = 0 \"Gain 2\" = 1 \"Gain 3\" = 1 }"
    ));
}

#[test]
fn a_wav_path_that_would_break_the_conf_is_refused() {
    for bad in ["relative/x.wav", "/a\"b.wav", "/a\\b.wav", "/a\nb.wav", ""] {
        assert!(
            SpeakersGraph::new(Path::new(bad), false).is_err(),
            "{bad:?}"
        );
    }
    assert!(SpeakersGraph::new(Path::new("/a b/c d.wav"), false).is_ok());
}

#[test]
fn speakers_params_name_the_mixer_ports_for_both_ears() {
    let off = speakers_params(false);
    let on = speakers_params(true);
    let names: Vec<&str> = off.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "lvs:Gain 1",
            "lvs:Gain 2",
            "lvs:Gain 3",
            "rvs:Gain 1",
            "rvs:Gain 2",
            "rvs:Gain 3"
        ]
    );
    let values = |p: &[(String, f64)]| p.iter().map(|(_, v)| *v).collect::<Vec<_>>();
    assert_eq!(values(&off), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(values(&on), [0.0, 1.0, 1.0, 0.0, 1.0, 1.0]);
}

#[test]
fn a_node_has_the_speakers_controls_only_when_the_conf_had_the_stage() {
    let mut node = LiveNode {
        id: 1,
        rate_hz: Some(48_000),
        sink: None,
        playing: false,
        params: HashMap::new(),
    };
    assert!(!node.has_speakers_controls());
    node.params.extend(speakers_params(false));
    assert!(node.has_speakers_controls());
    node.params.remove("rvs:Gain 3");
    assert!(!node.has_speakers_controls());
}

#[test]
fn the_speakers_mixers_gains_are_read_back_from_a_dump() {
    let mut params = Vec::new();
    for (name, value) in speakers_params(true) {
        params.push(serde_json::json!(name));
        params.push(serde_json::json!(value));
    }
    let dump = serde_json::to_string(&[serde_json::json!({
        "id": 10, "type": "PipeWire:Interface:Node",
        "info": {"state": "running",
                 "props": {"node.name": NODE_NAME, "media.class": "Audio/Sink"},
                 "params": {"Props": [{"params": params}]}}
    })])
    .unwrap();
    let node = props_of(&dump).expect("the chain is in the dump");
    assert!(node.has_speakers_controls());
    assert_eq!(node.params["lvs:Gain 1"], 0.0);
    assert_eq!(node.params["lvs:Gain 2"], 1.0);
    assert_eq!(node.params["rvs:Gain 3"], 1.0);
}
