// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

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
    assert_eq!(lines[34], "                ]");
    assert_eq!(lines[35], "                links = [");
    assert_eq!(
        lines[36],
        "                    { output = \"lpre:Out\" input = \"l01:In\" }"
    );
    assert_eq!(
        lines[51],
        "                    { output = \"l15:Out\" input = \"l16:In\" }"
    );
    assert_eq!(
        lines[52],
        "                    { output = \"rpre:Out\" input = \"r01:In\" }"
    );
    assert_eq!(lines[68], "                ]");
    assert_eq!(
        lines[69],
        "                inputs  = [ \"lpre:In\" \"rpre:In\" ]"
    );
    assert_eq!(
        lines[70],
        "                outputs = [ \"l16:Out\" \"r16:Out\" ]"
    );
    assert_eq!(lines[71], "            }");
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
