// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The sequences the ordinary gate runs: a fixed block of seeds (300 sequences, a few seconds), so a failure is the same on
/// every machine.
const SHORT_FIRST: u64 = 1;
const SHORT_RUNS: u64 = 300;

fn from_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
fn the_lab_finds_nothing_in_a_short_run() {
    let failures = run_seeds(SHORT_FIRST, SHORT_RUNS);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The long run: `make preamp-lab`, with `SVOLDER_LAB_SEED` (the first seed) and
/// `SVOLDER_LAB_RUNS` (how many) from the environment.
#[test]
#[ignore = "the long run: make preamp-lab"]
fn the_lab_finds_nothing_in_a_long_run() {
    let first = from_env("SVOLDER_LAB_SEED", 1_000);
    let runs = from_env("SVOLDER_LAB_RUNS", 5_000);
    let started = std::time::Instant::now();
    let failures = run_seeds(first, runs);
    eprintln!(
        "preamp lab: seeds {first}..{} ({runs} runs) in {:.0?}, {} failed",
        first + runs,
        started.elapsed(),
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn a_seed_makes_the_same_sequence_every_time() {
    assert_eq!(sequence_of(7), sequence_of(7));
    assert_ne!(sequence_of(7), sequence_of(8));
}

#[test]
fn the_model_reverts_slots_on_a_rate_change_and_keeps_the_preamp() {
    let conf = Conf {
        pre: 0.5,
        slots: vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; SLOTS],
    };
    let mut node = Node::start(3, &conf, 0);
    let live = vec![("l01:b0".to_string(), 2.0), ("lpre:Mult".to_string(), 0.9)];
    for (name, value) in &live {
        node.ports.insert(name.clone(), held_value(name, *value));
    }
    node.load_slots();
    assert_eq!(node.value("l01:b0"), 1.0, "the slot is the config's again");
    assert!(
        (node.value("lpre:Mult") - 0.9).abs() < 1e-6,
        "the preamp is as last set"
    );
    assert_eq!(
        held_value("l01:b0", 40.0),
        10.0,
        "a coefficient is held to 10"
    );
    assert_eq!(held_value("lpre:Mult", 40.0), 40.0);
}

#[test]
fn a_set_param_to_a_stale_id_does_nothing() {
    let mut lab = Lab::new(48_000);
    let mut world = world(&lab.shared);
    world.conf = Some(Conf {
        pre: 1.0,
        slots: vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; SLOTS],
    });
    world.start_node(false);
    let id = world.node.as_ref().unwrap().id;
    world.cli.alive = true;
    world.set_param(id - 1, &[("lpre:Mult".to_string(), 0.1)]);
    assert_eq!(world.node.as_ref().unwrap().value("lpre:Mult"), 1.0);
    assert!(world.cli.refused, "pw-cli says so");
    drop(world);
    assert!(lab.check().is_ok());
}

#[test]
fn the_evaluator_sees_a_peak_over_unity() {
    // A peaking filter of +6 dB at 1 kHz under no preamp.
    let c = svolder_profiles::tone::response::coefficients(
        FilterKind::Peaking,
        1000.0,
        6.0,
        1.0,
        48_000,
    );
    let peak = dense_peak(&[c], 48_000);
    assert!((peak - 6.0).abs() < 0.01, "{peak}");
    let flat = dense_peak(&[[1.0, 0.0, 0.0, 0.0, 0.0]], 48_000);
    assert!(flat.abs() < 1e-9, "{flat}");
}

#[test]
fn the_rule_is_checked_on_what_plays() {
    let mut lab = Lab::new(48_000);
    {
        let mut world = world(&lab.shared);
        let mut conf = Conf {
            pre: 1.0,
            slots: vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; SLOTS],
        };
        let c = svolder_profiles::tone::response::coefficients(
            FilterKind::Peaking,
            1000.0,
            6.0,
            1.0,
            48_000,
        );
        conf.slots[0] = [c[0], c[1], c[2], 1.0, c[3], c[4]];
        world.conf = Some(conf);
        world.start_node(false);
    }
    let failure = lab
        .check()
        .expect_err("+6 dB under unity gain is over unity");
    assert!(failure.what.contains("over unity"), "{}", failure.what);
}

#[test]
fn a_line_that_carries_one_slot_leaves_the_rest_as_a_rate_change_put_them() {
    holds(
        96_000,
        &[
            Event::Drag {
                pick: 0,
                kind: None,
                f: None,
                g: Some(-3.690741177599108),
                q: None,
                steps: 1,
                gap_ticks: 0,
            },
            Event::Cli {
                filters: vec![
                    SourceFilter {
                        kind: FilterKind::LowShelf,
                        f: 14091.560988224139,
                        g: -3.2595516652729533,
                        q: 0.28753235715507225,
                    },
                    SourceFilter {
                        kind: FilterKind::Peaking,
                        f: 555.8380120604496,
                        g: -11.803605479373832,
                        q: 5.942709029642545,
                    },
                    SourceFilter {
                        kind: FilterKind::HighShelf,
                        f: 741.1536127098026,
                        g: 4.696729786700583,
                        q: 3.1760294457346063,
                    },
                ],
                stated: 0.0,
                gap_ticks: 4,
            },
            Event::Add(SourceFilter {
                kind: FilterKind::LowShelf,
                f: 9398.915921685,
                g: -9.648461044332429,
                q: 2.9348133575038218,
            }),
            Event::Rate(44_100),
            Event::Drag {
                pick: 0,
                kind: None,
                f: Some(29.223071718968697),
                g: None,
                q: None,
                steps: 4,
                gap_ticks: 0,
            },
        ],
    );
}

#[test]
fn nothing_is_sent_to_a_chain_before_it_is_looked_at() {
    holds(
        192_000,
        &[
            Event::Load {
                filters: vec![
                    SourceFilter {
                        kind: FilterKind::Peaking,
                        f: 18.218463695931717,
                        g: 13.340298614464409,
                        q: 0.056365904782746425,
                    },
                    SourceFilter {
                        kind: FilterKind::LowShelf,
                        f: 6947.1037742640065,
                        g: -2.876872260283424,
                        q: 10.67127959057017,
                    },
                    SourceFilter {
                        kind: FilterKind::Peaking,
                        f: 37.98956667391673,
                        g: -10.431608383339968,
                        q: 0.1532830658331228,
                    },
                    SourceFilter {
                        kind: FilterKind::Peaking,
                        f: 254.7795680314753,
                        g: 10.256287959991099,
                        q: 0.08643286227561446,
                    },
                ],
                stated: -3.342646374061758,
            },
            Event::Ticks(38),
            Event::Drag {
                pick: 9,
                kind: None,
                f: None,
                g: Some(8.1876155488542),
                q: Some(0.7213530035517434),
                steps: 3,
                gap_ticks: 0,
            },
            Event::Ticks(35),
            Event::NodeRestart,
            Event::Drag {
                pick: 10,
                kind: None,
                f: None,
                g: Some(11.85942572030067),
                q: Some(0.5804856620406237),
                steps: 1,
                gap_ticks: 1,
            },
            Event::Rate(96_000),
        ],
    );
}

#[test]
fn a_new_chain_does_not_inherit_what_was_sent_to_the_old_one_after_a_pw_cli_death() {
    holds(
        48_000,
        &[
            Event::Drag {
                pick: 5,
                kind: None,
                f: None,
                g: Some(1.9468026524346378),
                q: None,
                steps: 3,
                gap_ticks: 1,
            },
            Event::ListDelay { ms: 1200 },
            Event::NodeRestart,
            Event::Drag {
                pick: 7,
                kind: None,
                f: Some(1559.919513621875),
                g: Some(-5.230721067484156),
                q: None,
                steps: 0,
                gap_ticks: 2,
            },
            Event::Drag {
                pick: 3,
                kind: Some(FilterKind::Peaking),
                f: Some(9133.273117291705),
                g: Some(10.473825831860879),
                q: None,
                steps: 4,
                gap_ticks: 0,
            },
            Event::Ticks(40),
            Event::Rate(48_000),
        ],
    );
}

/// The trace of one seed's shrunk failure, or of its whole sequence when it passes:
/// `SVOLDER_LAB_SEED=17 cargo test -p svolder-host --lib lab_trace -- --ignored --nocapture`.
#[test]
#[ignore = "a debugging aid"]
fn lab_trace() {
    let seed = from_env("SVOLDER_LAB_SEED", 1);
    let (rate, events) = sequence_of(seed);
    let events = shrink(rate, &events);
    let trace = Mutex::new(Vec::new());
    let result = run_traced(rate, &events, Some(&trace));
    eprintln!("sink starts at {rate} Hz, result {result:?}");
    for line in trace.into_inner().unwrap() {
        eprintln!("{line}");
    }
}

fn point(kind: FilterKind, f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter { kind, f, g, q }
}

fn drag(pick: usize, g: f64) -> Event {
    Event::Drag {
        pick,
        kind: None,
        f: None,
        g: Some(g),
        q: None,
        steps: 0,
        gap_ticks: 0,
    }
}

/// `events` run on a sink that starts at `rate`, which must come to nothing over unity.
fn holds(rate: u32, events: &[Event]) {
    if let Err(failure) = run_sequence(rate, events) {
        panic!("{} (event {})", failure.what, failure.event + 1);
    }
}

#[test]
fn an_edit_right_after_a_command_line_apply_goes_to_a_chain_that_was_looked_at() {
    let hot = vec![
        point(FilterKind::Peaking, 216.0, 12.0, 4.4),
        point(FilterKind::HighShelf, 10_700.0, -9.0, 4.0),
    ];
    holds(
        44_100,
        &[
            Event::Toggle(true),
            Event::Cli {
                filters: hot,
                stated: 0.0,
                gap_ticks: 2,
            },
            drag(0, 9.0),
            Event::Rate(48_000),
        ],
    );
}

#[test]
fn a_rate_change_between_two_looks_does_not_leave_old_slots_under_a_new_preamp() {
    // The chain's slots go back to the config at the rate change; the edit that follows before
    // the daemon looks must carry every slot.
    let curve = vec![
        point(FilterKind::Peaking, 300.0, 9.0, 1.0),
        point(FilterKind::Peaking, 3_000.0, 9.0, 1.0),
    ];
    holds(
        48_000,
        &[
            Event::Load {
                filters: curve,
                stated: 0.0,
            },
            Event::Ticks(40),
            drag(0, 0.0),
            drag(1, 0.0),
            Event::Rate(96_000),
            drag(0, 2.0),
        ],
    );
}

#[test]
fn a_chain_that_started_again_is_capped_by_its_config_whatever_the_rate_does() {
    let treble = vec![
        point(FilterKind::Peaking, 8_000.0, 12.0, 0.5),
        point(FilterKind::Peaking, 20_000.0, 12.0, 0.5),
    ];
    holds(
        192_000,
        &[
            Event::Load {
                filters: treble,
                stated: 0.0,
            },
            Event::Ticks(25),
            Event::NodeRestart,
            Event::Ticks(25),
            Event::Rate(44_100),
            Event::Ticks(25),
            Event::Rate(192_000),
            Event::Ticks(25),
        ],
    );
}

#[test]
fn a_refusal_read_late_does_not_send_the_daemons_line_to_a_chain_it_did_not_look_at() {
    // The node is replaced and a drag lands on the old id; the refusal is read only after the
    // next edit has begun, which then finds the new node. Nothing goes to it before a look.
    holds(
        96_000,
        &[
            Event::RefusalLag(1),
            Event::Drag {
                pick: 6,
                kind: None,
                f: None,
                g: None,
                q: None,
                steps: 2,
                gap_ticks: 2,
            },
            Event::Cli {
                filters: vec![point(FilterKind::HighShelf, 210.1, 8.18, 0.35)],
                stated: 0.0,
                gap_ticks: 2,
            },
            Event::Remove { pick: 15 },
            Event::Load {
                filters: vec![
                    point(FilterKind::LowShelf, 11_068.4, -6.12, 0.09),
                    point(FilterKind::HighShelf, 38.1, 3.6, 5.57),
                ],
                stated: 0.0,
            },
            Event::Rate(96_000),
        ],
    );
}

#[test]
fn a_write_that_fails_once_does_not_land_the_daemons_line_on_a_chain_it_did_not_look_at() {
    // The write fails and the command line replaces the chain before the daemon's next line:
    // that line goes to the id it kept, fails, and its second attempt finds the new node,
    // which it must not write to before a look.
    let cli = vec![
        point(FilterKind::Peaking, 23_765.0, -5.75, 0.81),
        point(FilterKind::LowShelf, 183.8, -8.41, 2.77),
        point(FilterKind::Peaking, 1_718.7, 0.74, 0.24),
        point(FilterKind::Peaking, 29.7, 4.9, 0.21),
        point(FilterKind::Peaking, 2_600.9, 10.58, 0.2),
        point(FilterKind::LowShelf, 442.2, 20.73, 1.2),
    ];
    holds(
        192_000,
        &[
            Event::Toggle(true),
            Event::Load {
                filters: vec![
                    point(FilterKind::LowShelf, 863.0, -5.04, 1.71),
                    point(FilterKind::HighShelf, 86.1, 9.97, 0.25),
                    point(FilterKind::HighShelf, 13_106.8, -0.48, 0.22),
                ],
                stated: 0.0,
            },
            Event::Ticks(24),
            Event::FailNextWrite,
            Event::Cli {
                filters: cli,
                stated: 0.51,
                gap_ticks: 7,
            },
            Event::Drag {
                pick: 15,
                kind: None,
                f: None,
                g: Some(-2.6),
                q: None,
                steps: 3,
                gap_ticks: 2,
            },
            Event::Rate(44_100),
        ],
    );
}

/// The shrunk sequence of one seed as Rust, to paste into a regression test:
/// `SVOLDER_LAB_SEED=17 cargo test -p svolder-host --lib lab_code -- --ignored --nocapture`.
#[test]
#[ignore = "a debugging aid"]
fn lab_code() {
    let seed = from_env("SVOLDER_LAB_SEED", 1);
    let (rate, events) = sequence_of(seed);
    let events = shrink(rate, &events);
    eprintln!(
        "// sink starts at {rate} Hz: {:?}",
        run_sequence(rate, &events).err()
    );
    for event in &events {
        eprintln!("{event:?},");
    }
}
