// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The sequences the ordinary gate runs: a fixed block of seeds (300 sequences under each model of
/// node ids, a few seconds), so a failure is the same on every machine.
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
    let mut failures = run_seeds(SHORT_FIRST, SHORT_RUNS, Ids::Fresh);
    failures.extend(run_seeds(SHORT_FIRST, SHORT_RUNS, Ids::Reused));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The short run of the speakers profile ([`generate_speakers`]): the ordinary mix with one event
/// in eight a change of the virtual speakers.
#[test]
fn the_lab_finds_nothing_in_a_short_speakers_run() {
    let mut failures = run_speakers_seeds(SHORT_FIRST, SHORT_RUNS, Ids::Fresh);
    failures.extend(run_speakers_seeds(SHORT_FIRST, SHORT_RUNS, Ids::Reused));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The short run of the targeted profile ([`generate_targeted`]): nearly half of its events are a
/// command-line apply, a node that starts again, a refusal read late, a write that fails once or
/// a drag, and it catches a send to a node other than the one looked at about twice as often as
/// the ordinary mix.
const TARGETED_FIRST: u64 = 1;
const TARGETED_RUNS: u64 = 300;

#[test]
fn the_lab_finds_nothing_in_a_short_targeted_run() {
    let mut failures = run_targeted_seeds(TARGETED_FIRST, TARGETED_RUNS, Ids::Fresh);
    failures.extend(run_targeted_seeds(
        TARGETED_FIRST,
        TARGETED_RUNS,
        Ids::Reused,
    ));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The short run of the high-rate profile ([`generate_highrate`]): rate changes up to 768 kHz,
/// above the 384 kHz the stages run at, among crossfeed changes and drags in flight.
const HIGHRATE_FIRST: u64 = 1;
const HIGHRATE_RUNS: u64 = 300;

#[test]
fn the_lab_finds_nothing_in_a_short_highrate_run() {
    let mut failures = run_highrate_seeds(HIGHRATE_FIRST, HIGHRATE_RUNS, Ids::Fresh);
    failures.extend(run_highrate_seeds(
        HIGHRATE_FIRST,
        HIGHRATE_RUNS,
        Ids::Reused,
    ));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The short run of the loudness profile ([`generate_loudness`]): volume moves through the
/// daemon and from outside, mutes, loudness changes and rates up to 768 kHz among the ordinary
/// mix.
const LOUDNESS_FIRST: u64 = 1;
const LOUDNESS_RUNS: u64 = 300;

#[test]
fn the_lab_finds_nothing_in_a_short_loudness_run() {
    let mut failures = run_loudness_seeds(LOUDNESS_FIRST, LOUDNESS_RUNS, Ids::Fresh);
    failures.extend(run_loudness_seeds(
        LOUDNESS_FIRST,
        LOUDNESS_RUNS,
        Ids::Reused,
    ));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// A block of seeds of one profile: the first seed, how many, the model of ids.
type RunSeeds = fn(u64, u64, Ids) -> Vec<String>;

/// The long run: `make preamp-lab`, with `SVOLDER_LAB_SEED` (the first seed) and
/// `SVOLDER_LAB_RUNS` (how many) from the environment.
#[test]
#[ignore = "the long run: make preamp-lab"]
fn the_lab_finds_nothing_in_a_long_run() {
    let first = from_env("SVOLDER_LAB_SEED", 1_000);
    let runs = from_env("SVOLDER_LAB_RUNS", 5_000);
    let started = std::time::Instant::now();
    let mut failures = Vec::new();
    let profiles: [(&str, RunSeeds); 5] = [
        ("ordinary", run_seeds),
        ("targeted", run_targeted_seeds),
        ("speakers", run_speakers_seeds),
        ("high-rate", run_highrate_seeds),
        ("loudness", run_loudness_seeds),
    ];
    let mut took = [std::time::Duration::ZERO; 5];
    for ids in [Ids::Fresh, Ids::Reused] {
        for ((_, run), took) in profiles.iter().zip(&mut took) {
            let at = std::time::Instant::now();
            failures.extend(run(first, runs, ids));
            *took += at.elapsed();
        }
    }
    for ((name, _), took) in profiles.iter().zip(took) {
        eprintln!("preamp lab: the {name} profile took {took:.0?}");
    }
    eprintln!(
        "preamp lab: seeds {first}..{} ({runs} runs of each of the five profiles, fresh and reused ids) in {:.0?}, {} failed",
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
    assert_eq!(sequence_of_loudness(7), sequence_of_loudness(7));
    assert_ne!(sequence_of_loudness(7).1, sequence_of(7).1);
}

/// The model reads the loudness stage from a config, a rate change puts it back to the config's
/// (the identity), and an apply keeps the loudness's settings in the state it writes, as the real
/// host does.
#[test]
fn the_model_keeps_the_loudness_stage_and_its_settings() {
    let request = ApplyRequest {
        name: "Flat".to_string(),
        origin: None,
        correction: Correction {
            preamp_db: 0.0,
            filters: vec![point(FilterKind::Peaking, 1000.0, 0.0, 1.0)],
        },
        sink: None,
        slots: Vec::new(),
        rate_hz: None,
        flat: true,
        crossfeed: None,
        speakers: None,
    };
    let (_, conf) = World::stage_apply(request.clone()).unwrap();
    let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    for name in ["ld1", "ld2"] {
        let values: Vec<f64> = PORTS
            .iter()
            .map(|port| {
                conf.ld
                    .iter()
                    .find(|(key, _)| *key == format!("{name}:{port}"))
                    .map(|(_, value)| *value)
                    .expect("the config's loudness port")
            })
            .collect();
        assert_eq!(values, identity, "{name} is the identity in the config");
    }
    let mut node = Node::start(3, 3, &conf, 0);
    let cut = crate::eq::loudness::stage(12.0, 48_000);
    for (name, value) in loudness_params(&cut.sections) {
        node.ports.insert(name.clone(), held_value(&name, value));
    }
    assert_ne!(
        loudness_sets(&node, 'l'),
        crate::eq::loudness::Stage::identity().sections
    );
    node.load_slots();
    assert_eq!(
        loudness_sets(&node, 'r'),
        crate::eq::loudness::Stage::identity().sections,
        "a rate change puts the stage back"
    );

    // An apply keeps the settings the state held.
    let lab = Lab::new(48_000);
    let settings = crate::eq::loudness::LoudnessSettings {
        on: true,
        strength_pct: 40.0,
        reference_pct: 80.0,
    };
    {
        let world = world(&lab.shared);
        let mut state = World::stage_apply(request.clone()).unwrap().0;
        state.loudness = Some(settings);
        state.save(&world.state_path()).unwrap();
    }
    let chain = ModelChain {
        world: lab.shared.clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let state = runtime.block_on(chain.apply(request)).unwrap();
    assert_eq!(state.loudness, Some(settings));
    assert_eq!(world(&lab.shared).state().unwrap().loudness, Some(settings));
}

#[test]
fn the_model_reverts_slots_on_a_rate_change_and_keeps_the_preamp() {
    let conf = Conf {
        stage: Vec::new(),
        ld: Vec::new(),
        pre: 0.5,
        slots: vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; SLOTS],
    };
    let mut node = Node::start(3, 3, &conf, 0);
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
        stage: Vec::new(),
        ld: Vec::new(),
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

/// The evaluator's `|H|^2`, written for accuracy at the ends of the band, against `H(e^jw)` worked
/// out directly with complex numbers.
#[test]
fn the_evaluator_agrees_with_the_transfer_function_at_complex_values() {
    use svolder_profiles::tone::response::coefficients;
    let sets = [
        coefficients(FilterKind::Peaking, 1000.0, 6.0, 1.0, 48_000),
        coefficients(FilterKind::LowShelf, 120.0, -7.5, 0.7, 48_000),
        coefficients(FilterKind::HighShelf, 9000.0, 4.0, 2.5, 48_000),
    ];
    let direct = |w: f64| -> f64 {
        sets.iter()
            .map(|c| {
                let z1 = (w.cos(), -w.sin());
                let z2 = ((2.0 * w).cos(), -(2.0 * w).sin());
                let num = (c[0] + c[1] * z1.0 + c[2] * z2.0, c[1] * z1.1 + c[2] * z2.1);
                let den = (1.0 + c[3] * z1.0 + c[4] * z2.0, c[3] * z1.1 + c[4] * z2.1);
                10.0 * ((num.0 * num.0 + num.1 * num.1) / (den.0 * den.0 + den.1 * den.1)).log10()
            })
            .sum()
    };
    for step in 1..200 {
        let w = std::f64::consts::PI * f64::from(step) / 200.0;
        let (ours, complex) = (summed_db(&sets, w), direct(w));
        assert!(
            (ours - complex).abs() < 1e-9,
            "w {w}: {ours} against {complex}"
        );
    }
}

#[test]
fn the_rule_is_checked_on_what_plays() {
    let mut lab = Lab::new(48_000);
    {
        let mut world = world(&lab.shared);
        let mut conf = Conf {
            stage: Vec::new(),
            ld: Vec::new(),
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

/// The liveness rule's second half: a curve the sink's new rate cannot play turns the EQ off, and
/// once the sink is back at a rate that plays it, the EQ turns on when asked.
#[test]
fn an_eq_turned_off_by_a_rate_it_cannot_play_turns_on_at_a_rate_it_can() {
    holds(
        48_000,
        &[
            Event::Load {
                filters: vec![point(FilterKind::HighShelf, 1000.0, 14.5, 0.707)],
                stated: 0.0,
            },
            Event::Ticks(45),
            Event::Rate(192_000),
            Event::Ticks(45),
            Event::Rate(48_000),
            Event::Ticks(45),
            Event::Ticks(45),
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

/// A slot reverted to the config's value while a drag is under way, at the same node and rate:
/// the next looks put the dragged point's values back on the chain.
#[test]
fn a_slot_revert_in_the_middle_of_a_drag_is_put_right() {
    holds(
        48_000,
        &[
            Event::Toggle(true),
            Event::Ticks(60),
            Event::Drag {
                pick: 2,
                kind: None,
                f: None,
                g: Some(6.0),
                q: None,
                steps: 4,
                gap_ticks: 3,
            },
            Event::SlotRevert,
            Event::Drag {
                pick: 2,
                kind: None,
                f: None,
                g: Some(-4.0),
                q: None,
                steps: 4,
                gap_ticks: 3,
            },
            Event::Ticks(80),
            Event::SlotRevert,
            Event::Ticks(80),
        ],
    );
}

/// The daemon restarts in the middle of a drag: the chain it finds plays values the state file
/// does not hold, and the saved state goes to it with the preamp never above unity.
#[test]
fn a_daemon_that_restarts_in_the_middle_of_a_drag_puts_the_saved_state_on_the_chain() {
    holds(
        48_000,
        &[
            Event::Toggle(true),
            Event::Ticks(60),
            Event::Drag {
                pick: 2,
                kind: None,
                f: None,
                g: Some(9.0),
                q: None,
                steps: 6,
                gap_ticks: 1,
            },
            Event::DaemonRestart,
            Event::Ticks(120),
        ],
    );
}

/// The sequence of `seed` from the profile it fails in: the ordinary mix, else the targeted one,
/// else the speakers one, else the high-rate one, else (it passes in all) the ordinary mix.
fn chosen_sequence(seed: u64) -> (u32, Vec<Event>) {
    let ordinary = sequence_of(seed);
    if run_sequence(ordinary.0, &ordinary.1).is_err() {
        return ordinary;
    }
    let targeted = sequence_of_targeted(seed);
    if run_sequence(targeted.0, &targeted.1).is_err() {
        return targeted;
    }
    let speakers = sequence_of_speakers(seed);
    if run_sequence(speakers.0, &speakers.1).is_err() {
        return speakers;
    }
    let highrate = sequence_of_highrate(seed);
    if run_sequence(highrate.0, &highrate.1).is_err() {
        return highrate;
    }
    let loudness = sequence_of_loudness(seed);
    match run_sequence(loudness.0, &loudness.1).is_err() {
        true => loudness,
        false => ordinary,
    }
}

/// The trace of one seed's shrunk failure, or of its whole sequence when it passes:
/// `SVOLDER_LAB_SEED=17 cargo test -p svolder-host --lib lab_trace -- --ignored --nocapture`.
#[test]
#[ignore = "a debugging aid"]
fn lab_trace() {
    let seed = from_env("SVOLDER_LAB_SEED", 1);
    let (rate, events) = chosen_sequence(seed);
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

/// Seed 4965, shrunk: a drag, then a command-line apply the controller does not read before its
/// own turn-off saves the state file over it, and the remove fails. Without the resend a save
/// over a foreign file makes (the chain's own points sent again at the next look), the chain
/// plays the command line's curve while the view says live with other points.
#[test]
fn a_turn_off_that_fails_over_a_command_line_apply_still_sends_the_points() {
    holds(
        192_000,
        &[
            Event::RefuseSends { ms: 1500 },
            Event::Drag {
                pick: 11,
                kind: None,
                f: None,
                g: Some(4.381),
                q: None,
                steps: 4,
                gap_ticks: 2,
            },
            Event::Fail { kind: 1 },
            Event::Cli {
                filters: vec![
                    point(FilterKind::HighShelf, 3992.71, -2.809, 0.565),
                    point(FilterKind::Peaking, 1200.53, 13.158, 0.135),
                    point(FilterKind::HighShelf, 3207.55, 1.416, 0.58),
                    point(FilterKind::LowShelf, 6497.35, -1.972, 0.087),
                    point(FilterKind::LowShelf, 50.229, 6.15, 0.367),
                    point(FilterKind::HighShelf, 22.238, -5.395, 0.258),
                ],
                stated: 0.0,
                gap_ticks: 8,
            },
            Event::Toggle(false),
        ],
    );
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

fn crossfeed(on: Option<bool>, model: Option<usize>, delay_ms: Option<f64>, done: bool) -> Event {
    Event::Crossfeed {
        on,
        model,
        amount_pct: None,
        delay_ms,
        done,
    }
}

/// Seed 1515, shrunk: Meier with a delay is turned on at 88.2 kHz, the sink moves to 176.4 kHz,
/// and the next line fails once; the full line that repairs it carries the stage as it was
/// worked out at 88.2 kHz, whose delay plays as other whole samples at 176.4 kHz.
#[test]
fn a_repair_after_a_rate_change_plays_the_old_stage_under_its_preamp() {
    holds(
        192_000,
        &[
            Event::Rate(88_200),
            Event::Crossfeed {
                on: Some(false),
                model: Some(2),
                amount_pct: Some(42.998),
                delay_ms: Some(0.7302),
                done: false,
            },
            Event::Drag {
                pick: 3,
                kind: None,
                f: None,
                g: Some(-9.51),
                q: None,
                steps: 4,
                gap_ticks: 1,
            },
            Event::Ticks(25),
            crossfeed(Some(true), None, None, false),
            Event::Rate(176_400),
            Event::FailNextWrite,
            Event::Drag {
                pick: 0,
                kind: None,
                f: None,
                g: Some(1.043),
                q: None,
                steps: 4,
                gap_ticks: 2,
            },
        ],
    );
}

/// Seed 5457, shrunk: the same through a `pw-cli` that dies after a line, at 96 then 192 kHz (the
/// seed asked 1.75 ms, which Meier holds to its 1 ms).
#[test]
fn a_restarted_pw_cli_after_a_rate_change_plays_the_old_stage_under_its_preamp() {
    holds(
        96_000,
        &[
            crossfeed(Some(true), Some(2), Some(1.0), true),
            Event::Ticks(39),
            Event::CliDeath {
                after_line: Some(true),
            },
            Event::Ticks(2),
            Event::Rate(192_000),
            Event::Drag {
                pick: 13,
                kind: None,
                f: None,
                g: Some(8.831),
                q: None,
                steps: 3,
                gap_ticks: 2,
            },
        ],
    );
}

/// No failure at all: a crossfeed drag between a rate change and the next look sends the stage
/// worked out at the rate the controller last saw, at every moment of its second.
#[test]
fn a_crossfeed_drag_right_after_a_rate_change_plays_under_its_preamp() {
    for ticks in 20..45 {
        holds(
            96_000,
            &[
                crossfeed(Some(true), Some(2), Some(1.0), true),
                Event::Ticks(ticks),
                Event::Rate(192_000),
                crossfeed(None, None, Some(1.0), false),
                Event::Ticks(3),
            ],
        );
    }
}

/// No failure either: the repeat a second after the stage went out, sent between a rate change
/// and the next look, carries the stage worked out at the old rate.
#[test]
fn the_stage_repeat_right_after_a_rate_change_plays_under_its_preamp() {
    for (before, after) in [(17, 3), (18, 4), (19, 5)] {
        holds(
            96_000,
            &[
                Event::Ticks(before),
                crossfeed(Some(true), Some(2), Some(1.0), true),
                Event::Ticks(after),
                Event::Rate(192_000),
                Event::Ticks(40),
            ],
        );
    }
}

/// A DAC that runs at 384 kHz: a crossfeed drag right after the sink moves there from 176.4 kHz
/// sends Meier at 50 % and 0.5 ms as it was worked out for 176.4 kHz.
#[test]
fn a_crossfeed_drag_right_after_a_change_to_384_khz_plays_under_its_preamp() {
    let on = Event::Crossfeed {
        on: Some(true),
        model: Some(2),
        amount_pct: Some(50.0),
        delay_ms: Some(0.5),
        done: true,
    };
    holds(
        176_400,
        &[
            on,
            Event::Ticks(25),
            Event::Rate(384_000),
            crossfeed(None, None, Some(0.5), false),
            Event::Ticks(3),
        ],
    );
}

/// Seed 1291, shrunk: Meier with a 1 ms delay goes into the config (its live send is refused),
/// the amount is turned down live at 44.1 kHz, the sink moves to 48 kHz, which puts the config's
/// stage back on the node, and the next drag line carries the slots without the stage. Its preamp
/// pays for the config's stage, not only for the one sent live.
#[test]
fn a_drag_line_after_a_rate_change_pays_for_the_stage_the_config_holds() {
    holds_both(
        384_000,
        &[
            crossfeed(Some(true), Some(2), Some(0.998), true),
            Event::Ticks(25),
            Event::RefuseSends { ms: 6000 },
            Event::Add(point(FilterKind::LowShelf, 65.797, -8.8908, 0.9557)),
            Event::Ticks(30),
            Event::Rate(44_100),
            load(vec![
                point(FilterKind::LowShelf, 3680.3, -0.4797, 4.1582),
                point(FilterKind::HighShelf, 2189.9, -6.7883, 0.2225),
                point(FilterKind::Peaking, 2317.2, -11.840, 9.3404),
            ]),
            Event::Ticks(58),
            Event::Drag {
                pick: 14,
                kind: Some(FilterKind::HighShelf),
                f: Some(484.21),
                g: Some(0.4758),
                q: None,
                steps: 4,
                gap_ticks: 2,
            },
            Event::Crossfeed {
                on: Some(true),
                model: None,
                amount_pct: Some(7.4613),
                delay_ms: None,
                done: true,
            },
            Event::Rate(48_000),
            Event::Drag {
                pick: 0,
                kind: None,
                f: None,
                g: Some(1.146),
                q: None,
                steps: 1,
                gap_ticks: 2,
            },
        ],
    );
}

/// M/S bass at 50 % goes into the config (its live send is refused), the chain
/// restarts and the apply that would take the stage out fails once, a drag's release then keeps
/// the config on disk without the stage, and the daemon restarts. The running chain's config
/// still holds the stage, which the next rate change puts back under a hotter drag's line: the
/// new daemon knows it from the state file, pays for it and takes it out.
#[test]
fn a_stage_the_running_config_holds_is_paid_for_across_a_daemon_restart() {
    holds_both(
        48_000,
        &[
            load(vec![point(FilterKind::Peaking, 1_000.0, 2.0, 1.0)]),
            Event::Ticks(25),
            Event::RefuseSends { ms: 2000 },
            Event::Crossfeed {
                on: Some(true),
                model: Some(3),
                amount_pct: Some(50.0),
                delay_ms: None,
                done: true,
            },
            Event::Ticks(60),
            Event::Fail { kind: 0 },
            Event::NodeRestart,
            Event::Ticks(21),
            drag(0, 2.5),
            Event::Ticks(45),
            Event::DaemonRestart,
            Event::Ticks(45),
            Event::Rate(44_100),
            drag(0, 8.0),
            Event::Ticks(10),
        ],
    );
}

/// Meier at 50 % and 1 ms goes into the config at 352.8 kHz (its live send is
/// refused) and the sink moves to 705.6 kHz. The graph is built again from its config with that
/// stage before any look turns crossfeed off there, so the stage's cost takes in the rates a
/// graph can be rebuilt at, up to 768 kHz, though no stage is run above 384 kHz.
#[test]
fn a_config_stage_rebuilt_above_384_khz_plays_under_its_preamp() {
    holds_both(
        352_800,
        &[
            load(vec![point(FilterKind::Peaking, 1_000.0, 2.0, 1.0)]),
            Event::Ticks(25),
            Event::RefuseSends { ms: 2000 },
            Event::Crossfeed {
                on: Some(true),
                model: Some(2),
                amount_pct: Some(50.0),
                delay_ms: Some(1.0),
                done: true,
            },
            Event::Ticks(60),
            Event::Rate(705_600),
            Event::Ticks(30),
        ],
    );
}

/// A crossfeed drag in flight while the sink moves above 384 kHz: its lines carry the stage worked
/// out at the rate the kept node still says, the release's look sees the new rate and turns
/// crossfeed off, and the line that goes with it must not raise the preamp while that stage still
/// plays. Every tick offset of the move after a look, for a delay of 0.5 and 1 ms, at
/// 384 to 768 kHz and 352.8 to 705.6 kHz.
#[test]
fn a_crossfeed_drag_across_a_move_above_384_khz_plays_under_its_preamp() {
    let pct = |amount: f64, done: bool| Event::Crossfeed {
        on: None,
        model: None,
        amount_pct: Some(amount),
        delay_ms: None,
        done,
    };
    for (from, to) in [(384_000, 768_000), (352_800, 705_600)] {
        for delay in [0.5, 1.0] {
            for ticks in 20..41 {
                holds_both(
                    from,
                    &[
                        load(vec![point(FilterKind::Peaking, 1_000.0, 2.0, 1.0)]),
                        Event::Crossfeed {
                            on: Some(true),
                            model: Some(2),
                            amount_pct: Some(50.0),
                            delay_ms: Some(delay),
                            done: true,
                        },
                        Event::Ticks(ticks),
                        pct(49.0, false),
                        Event::Rate(to),
                        pct(50.0, false),
                        Event::Ticks(1),
                        pct(48.0, false),
                        Event::Ticks(1),
                        pct(50.0, true),
                        Event::Ticks(30),
                    ],
                );
            }
        }
    }
}

/// One row of [`stage_cost_table`]: the stage's settings, and over every rate it may be worked out
/// at (up to 384 kHz) and every rate it may then play at (up to 768 kHz), the most it adds (by the lab's own evaluator), the most
/// that is over what the controller takes off for it, and the most over the cost taken at 48 kHz.
struct CostRow {
    label: String,
    worst_db: f64,
    over_cost_db: f64,
    over_48k_cost_db: f64,
}

fn cost_row(label: String, settings: &crate::eq::crossfeed::CrossfeedSettings) -> CostRow {
    use crate::eq::crossfeed::Stage;
    use crate::eq::manage::config_stage_cost_db;
    let at_48k = config_stage_cost_db(settings, 48_000);
    let mut row = CostRow {
        label,
        worst_db: 0.0,
        over_cost_db: f64::NEG_INFINITY,
        over_48k_cost_db: f64::NEG_INFINITY,
    };
    for worked in RATES {
        let Ok(stage) = Stage::of(settings, worked) else {
            continue;
        };
        let ports = stage_ports(&stage);
        let cost = config_stage_cost_db(settings, worked);
        // No stage runs above 384 kHz, but one a config holds is rebuilt at any rate.
        for play in RATES.into_iter().chain([705_600, 768_000]) {
            let gain = stage_gain_db(&ports, play);
            row.worst_db = row.worst_db.max(gain);
            row.over_cost_db = row.over_cost_db.max(gain - cost);
            row.over_48k_cost_db = row.over_48k_cost_db.max(gain - at_48k);
        }
    }
    row
}

/// Every model's presets, a low Custom amount (seed 1291's 7.5 %), and for a model that plays a
/// delay, each preset with 0.25, 0.5, 0.75 and 1 ms.
fn cost_cases() -> Vec<(String, crate::eq::crossfeed::CrossfeedSettings)> {
    use crate::eq::crossfeed::{CrossfeedChange, CrossfeedSettings};
    let mut cases = Vec::new();
    for model in CrossfeedModel::ALL {
        let spec = model.spec();
        let mut bases: Vec<(String, CrossfeedSettings)> = spec
            .presets
            .iter()
            .filter_map(|preset| {
                let settings = CrossfeedSettings::from_preset(model, preset.name)?;
                Some((format!("{} {}", model.name(), preset.name), settings))
            })
            .collect();
        let low = CrossfeedSettings::for_model(model).change(
            &CrossfeedChange {
                amount_pct: Some(7.5),
                ..CrossfeedChange::default()
            },
            48_000,
        );
        if let Ok(low) = low {
            bases.push((format!("{} 7.5 %", model.name()), low));
        }
        for (label, settings) in bases {
            if spec.delay_settable {
                for ms in [0.25, 0.5, 0.75, 1.0] {
                    let delayed = settings.change(
                        &CrossfeedChange {
                            delay_ms: Some(ms),
                            ..CrossfeedChange::default()
                        },
                        48_000,
                    );
                    if let Some(delayed) = delayed.ok().filter(|held| held.delay_ms.is_some()) {
                        cases.push((format!("{label}, {ms} ms"), delayed));
                    }
                }
            }
            cases.push((format!("{label}, own lag"), settings));
        }
    }
    cases
}

/// The worst case of a line of slots under a stage, the one the config holds after a rate change
/// or the one sent live: for each case of [`cost_cases`], worked out at any of the lab's rates
/// and played at any of them, the stage's gain by the lab's evaluator less the cost the
/// controller takes off the preamp for it. The slots' own part is at most 0.008 dB (the EQ's
/// preamp rounding), so a line plays at most that plus the table's margin over unity:
/// `cargo test --release -p svolder-host --lib stage_cost_table -- --ignored --nocapture`.
#[test]
#[ignore = "a table to read: a few seconds in release"]
fn stage_cost_table() {
    let cases = cost_cases();
    let rows: Vec<CostRow> = std::thread::scope(|scope| {
        let chunks: Vec<_> = cases
            .chunks(cases.len().div_ceil(6))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|(label, settings)| cost_row(label.clone(), settings))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        chunks
            .into_iter()
            .flat_map(|chunk| chunk.join().expect("a row"))
            .collect()
    });
    eprintln!(
        "| stage | worst gain dB | over its cost dB | over the 48 kHz cost dB | line bound dB |"
    );
    eprintln!("|---|---|---|---|---|");
    for row in &rows {
        eprintln!(
            "| {} | {:+.3} | {:+.3} | {:+.3} | {:+.3} |",
            row.label,
            row.worst_db,
            row.over_cost_db,
            row.over_48k_cost_db,
            row.over_cost_db + 0.008
        );
    }
    let worst = rows
        .iter()
        .map(|row| row.over_cost_db)
        .fold(f64::NEG_INFINITY, f64::max);
    eprintln!(
        "{} cases, the most over the cost: {worst:+.4} dB",
        rows.len()
    );
    assert!(worst + 0.008 <= TOLERANCE_DB, "{worst}");
}

/// The ports of `stage` on the left channel as a node holds them (f32), in the order
/// [`stage_gain_db`] reads them.
fn stage_ports(stage: &crate::eq::crossfeed::Stage) -> Vec<f64> {
    let ports: HashMap<String, f64> = crossfeed_params(stage)
        .into_iter()
        .map(|(name, value)| (name, f64::from(value as f32)))
        .collect();
    let mut values = Vec::new();
    for path in ["dir", "x", "s"] {
        for port in PORTS {
            values.push(ports[&format!("l{path}:{port}")]);
        }
    }
    for port in ["sum:Gain 1", "sum:Gain 2", "del:Delay (s)", "mix:Gain 2"] {
        values.push(ports[&format!("l{port}")]);
    }
    values
}

/// The evaluator does not share the controller's guess of which whole sample a delay on the
/// boundary plays: Meier at 35 %, 2 kHz and 0.15 ms worked out at 96 kHz plays 15 samples at
/// 192 kHz by PipeWire's f32 sum and 14 by an f64 one; the worse of the two counts.
#[test]
fn the_evaluator_takes_the_worse_sample_of_a_delay_on_the_boundary() {
    use crate::eq::crossfeed::{CrossfeedChange, CrossfeedSettings, Stage};
    let settings = CrossfeedSettings::for_model(CrossfeedModel::Meier)
        .change(
            &CrossfeedChange {
                amount_pct: Some(35.0),
                crossover_hz: Some(2000.0),
                delay_ms: Some(0.15),
                ..Default::default()
            },
            96_000,
        )
        .unwrap();
    let stage = Stage::of(&settings, 96_000).unwrap();
    let mut on_15 = stage.clone();
    on_15.delay_s = 15.5 / 192_000.0;
    let worse = stage_gain_db(&stage_ports(&on_15), 192_000);
    let got = stage_gain_db(&stage_ports(&stage), 192_000);
    assert!(got >= worse - 1e-9, "{got} under {worse}");
}

/// Seed 15327 (0.35.1's first fix, 20000 runs), shrunk: crossfeed is turned on, its line fails
/// and so does the apply that would hold the stage in the config, so the start is undone; the
/// session's repair then plays the line it was handed (the stage under its own preamp), and the
/// next line must take the stage off again before the preamp rises.
#[test]
fn a_crossfeed_start_that_fails_takes_back_the_line_it_handed_over() {
    holds(
        44_100,
        &[
            Event::Drag {
                pick: 15,
                kind: Some(FilterKind::LowShelf),
                f: None,
                g: Some(-11.55),
                q: None,
                steps: 2,
                gap_ticks: 0,
            },
            Event::FailNextWrite,
            Event::Fail { kind: 0 },
            crossfeed(Some(true), Some(2), None, true),
        ],
    );
}

/// Seed 12272 (fix 2's 20000 runs, the 8-rate list), shrunk: a curve the command line applied
/// and two loads, then the sink moves to 352.8 kHz. The EQ must not say live while the chain
/// plays silence.
#[test]
fn a_move_to_352_8_khz_never_plays_silence_while_live() {
    holds(
        44_100,
        &[
            Event::Drag {
                pick: 15,
                kind: Some(FilterKind::LowShelf),
                f: Some(542.657),
                g: None,
                q: Some(3.232),
                steps: 3,
                gap_ticks: 0,
            },
            Event::RefuseSends { ms: 200 },
            Event::Drag {
                pick: 11,
                kind: Some(FilterKind::HighShelf),
                f: None,
                g: Some(9.936),
                q: None,
                steps: 0,
                gap_ticks: 2,
            },
            Event::Crossfeed {
                on: Some(true),
                model: None,
                amount_pct: Some(46.007),
                delay_ms: None,
                done: true,
            },
            Event::Cli {
                filters: vec![
                    point(FilterKind::LowShelf, 27.4, 8.31, 4.84),
                    point(FilterKind::Peaking, 24.9, -11.48, 0.62),
                    point(FilterKind::HighShelf, 17_640.5, -8.39, 0.27),
                    point(FilterKind::Peaking, 119.0, -3.57, 3.44),
                ],
                stated: 0.0,
                gap_ticks: 3,
            },
            Event::Load {
                filters: vec![
                    point(FilterKind::HighShelf, 49.7, -1.65, 0.32),
                    point(FilterKind::HighShelf, 14.6, -8.59, 2.35),
                ],
                stated: -1.5,
            },
            Event::Load {
                filters: vec![point(FilterKind::HighShelf, 10_697.3, 6.72, 4.01)],
                stated: 0.0,
            },
            Event::Rate(352_800),
        ],
    );
}

/// Seed 20568, shrunk: the same at 384 kHz, from a chain loaded at 176.4 kHz.
#[test]
fn a_move_to_384_khz_never_plays_silence_while_live() {
    holds(
        384_000,
        &[
            Event::Fail { kind: 0 },
            Event::Rate(176_400),
            Event::Crossfeed {
                on: Some(true),
                model: None,
                amount_pct: Some(41.183),
                delay_ms: Some(1.0),
                done: true,
            },
            Event::Load {
                filters: vec![
                    point(FilterKind::HighShelf, 25.2, -23.23, 8.68),
                    point(FilterKind::HighShelf, 5_584.0, 7.07, 3.35),
                    point(FilterKind::LowShelf, 4_886.4, -6.88, 6.67),
                    point(FilterKind::Peaking, 14.6, -4.78, 0.15),
                ],
                stated: 0.0,
            },
            Event::Load {
                filters: vec![
                    point(FilterKind::LowShelf, 5_999.3, 2.29, 0.52),
                    point(FilterKind::LowShelf, 21.5, 9.63, 1.91),
                    point(FilterKind::Peaking, 52.0, -10.03, 7.64),
                    point(FilterKind::LowShelf, 809.0, 9.15, 0.63),
                ],
                stated: 0.0,
            },
            Event::Remove { pick: 8 },
            Event::Rate(384_000),
        ],
    );
}

/// The low shelf of seed 20568 (21.5 Hz, +9.63 dB, Q 1.91) is stable in f32 at 384 kHz but its
/// coefficients play +3.5 dB at DC for +9.6 (b1 rounds to a1): its preamp is measured on what
/// they play, so the chain never goes above unity with it, and it is never silence. A curve with
/// no bound to its peak there (seed 12272's load at 352.8 kHz) is refused, and its preamp is the
/// floor, never below it.
#[test]
fn a_curve_f32_cannot_hold_at_a_high_rate_is_measured_as_it_plays_or_refused() {
    use crate::eq::manage::{check_playable, played_preamp};
    let shelf = Correction {
        preamp_db: 0.0,
        filters: vec![point(FilterKind::LowShelf, 21.5, 9.63, 1.91)],
    };
    let (sets, centres) = crate::eq::live::played_sets(&shelf, 384_000);
    let plays = svolder_profiles::tone::response::played_peak_db_of(&sets, &centres, 384_000);
    let preamp = played_preamp(&shelf, 384_000);
    assert!(plays.is_finite() && preamp.is_finite(), "{plays} {preamp}");
    assert!(plays + preamp <= 0.008, "{plays} under {preamp}");
    let unbounded = Correction {
        preamp_db: -1.5,
        filters: vec![
            point(FilterKind::HighShelf, 14.6, -8.59, 2.35),
            point(FilterKind::HighShelf, 49.7, -1.65, 0.32),
        ],
    };
    let refused = check_playable(&unbounded, 352_800).expect_err("refused at 352.8 kHz");
    assert!(refused.what.contains("352800 Hz"), "{}", refused.what);
    assert_eq!(played_preamp(&unbounded, 352_800), -120.0);
    assert!(check_playable(&unbounded, 48_000).is_ok());
}

/// The shrunk sequence of one seed as Rust, to paste into a regression test:
/// `SVOLDER_LAB_SEED=17 cargo test -p svolder-host --lib lab_code -- --ignored --nocapture`.
#[test]
#[ignore = "a debugging aid"]
fn lab_code() {
    let seed = from_env("SVOLDER_LAB_SEED", 1);
    let (rate, events) = chosen_sequence(seed);
    let events = shrink(rate, &events);
    eprintln!(
        "// sink starts at {rate} Hz: {:?}",
        run_sequence(rate, &events).err()
    );
    for event in &events {
        eprintln!("{event:?},");
    }
}

/// `events` on a sink that starts at `rate`, under both models of node ids, must come to nothing
/// over unity.
fn holds_both(rate: u32, events: &[Event]) {
    let was = IDS.get();
    for ids in [Ids::Fresh, Ids::Reused] {
        IDS.set(ids);
        if let Err(failure) = run_sequence(rate, events) {
            IDS.set(was);
            panic!(
                "[ids {ids:?}] {} (event {})",
                failure.what,
                failure.event + 1
            );
        }
    }
    IDS.set(was);
}

fn load(filters: Vec<SourceFilter>) -> Event {
    Event::Load {
        filters,
        stated: 0.0,
    }
}

/// A boost that needs about -10 dB of preamp, one that needs about -2 dB, and a flat curve.
fn hot() -> Vec<SourceFilter> {
    vec![point(FilterKind::Peaking, 100.0, 10.0, 1.0)]
}

fn gentle() -> Vec<SourceFilter> {
    vec![point(FilterKind::Peaking, 100.0, 2.0, 1.0)]
}

fn flat() -> Vec<SourceFilter> {
    vec![point(FilterKind::Peaking, 1_000.0, 0.0, 1.0)]
}

/// The serial of the node the world runs.
fn serial(lab: &Lab) -> Option<u64> {
    world(&lab.shared).node.as_ref().map(|node| node.serial)
}

/// A recall that needs more preamp while a stream plays leaves the node as it is (no restart, the
/// level held), and the node is started again with the curve's own preamp once nothing has
/// played for the idle time; every step at or under unity, under both models of ids.
#[test]
fn a_recall_while_playing_keeps_the_node_and_the_idle_restart_lifts_the_level() {
    for ids in [Ids::Fresh, Ids::Reused] {
        IDS.set(ids);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let mut lab = Lab::new(48_000);
            for event in [load(hot()), Event::Ticks(30), Event::Playing(true)] {
                lab.run(&event).await.unwrap();
            }
            let started = serial(&lab);
            let cap = lab.control.view().preamp_db;
            for event in [
                load(gentle()),
                Event::Ticks(30),
                load(flat()),
                Event::Ticks(30),
                Event::Rate(44_100),
                Event::Ticks(30),
                load(gentle()),
                Event::Ticks(90),
            ] {
                lab.run(&event).await.unwrap();
            }
            assert_eq!(serial(&lab), started, "[{ids:?}] no restart while playing");
            let view = lab.control.view();
            assert_eq!(view.note.as_deref(), Some(super::super::LEVEL_HELD_NOTE));
            assert!(
                view.preamp_db <= cap + 1e-9,
                "{} over {cap}",
                view.preamp_db
            );
            for event in [Event::Playing(false), Event::Ticks(45), Event::Ticks(45)] {
                lab.run(&event).await.unwrap();
            }
            assert_ne!(serial(&lab), started, "[{ids:?}] started again once idle");
            let view = lab.control.view();
            assert_eq!(view.note, None);
            assert!(view.preamp_db > -3.0, "the curve's own: {}", view.preamp_db);
            lab.quiet_tail(0).await.unwrap();
        });
    }
    IDS.set(Ids::Fresh);
}

/// Held recalls among everything else that can happen while a stream plays: a node that starts
/// again, the daemon restarting, the EQ off and on, the crossfeed, rate changes, a command-line
/// apply, then the music pausing.
#[test]
fn held_recalls_among_restarts_toggles_crossfeed_and_rates_stay_under_unity() {
    let crossfeed_on = Event::Crossfeed {
        on: Some(true),
        model: Some(2),
        amount_pct: None,
        delay_ms: None,
        done: true,
    };
    for rate in [44_100, 96_000, 384_000] {
        holds_both(
            rate,
            &[
                load(hot()),
                Event::Ticks(25),
                Event::Playing(true),
                load(gentle()),
                Event::Ticks(25),
                Event::Rate(48_000),
                Event::Ticks(25),
                load(flat()),
                Event::NodeRestart,
                Event::Ticks(25),
                load(hot()),
                Event::Ticks(5),
                load(flat()),
                Event::DaemonRestart,
                Event::Ticks(45),
                Event::Toggle(false),
                Event::Ticks(10),
                Event::Toggle(true),
                Event::Ticks(10),
                crossfeed_on.clone(),
                load(gentle()),
                Event::Ticks(25),
                Event::Rate(rate),
                Event::Ticks(25),
                Event::Cli {
                    filters: hot(),
                    stated: 0.0,
                    gap_ticks: 4,
                },
                Event::Ticks(25),
                load(flat()),
                Event::Ticks(25),
                Event::Playing(false),
                Event::Ticks(45),
                Event::Ticks(45),
                Event::Playing(true),
                load(hot()),
                Event::Ticks(10),
                load(flat()),
                Event::DaemonRestart,
                Event::Playing(false),
                Event::Ticks(45),
                Event::Ticks(45),
            ],
        );
    }
}

/// Seed 4421, shrunk: the EQ turned on by an Add while a stream plays and sends are refused, then
/// turned off: the pass-through's line fails and so does the remove (armed to fail), so the EQ
/// stays on with its points, and the session's repair must not play the flat line the controller
/// took back.
#[test]
fn a_failed_pass_through_line_is_not_played_by_the_repair() {
    holds_both(
        384_000,
        &[
            Event::Playing(true),
            Event::Fail { kind: 1 },
            Event::RefuseSends { ms: 6000 },
            Event::Add(point(FilterKind::HighShelf, 22.5, 27.93, 2.60)),
            Event::Toggle(false),
        ],
    );
}

/// A speakers start whose line is refused and whose apply fails is taken back, and the session's
/// repair plays that line, then a held recall goes live: nothing over unity, and once quiet the
/// speakers play as the view says, under both models of ids. (Without the stage
/// sent again the model's node holds the convolved sound for about a second, until a look sees
/// the mixers gone from their sent values and the controller sends the stage: the liveness rule
/// looks after three quiet seconds, so the controller test is the one that fails without it.)
#[test]
fn a_failed_speakers_start_is_not_played_by_the_repair() {
    let speakers = |on: Option<bool>, set: Option<usize>| Event::Speakers { on, set };
    let started = [
        load(hot()),
        Event::Ticks(25),
        speakers(Some(true), Some(0)),
        Event::Ticks(25),
        speakers(Some(false), None),
        Event::Ticks(25),
        Event::Playing(true),
        Event::RefuseSends { ms: 300 },
        Event::Fail { kind: 0 },
        speakers(Some(true), None),
        Event::Ticks(25),
        load(flat()),
    ];
    for rate in [48_000, 96_000] {
        let mut quiet = started.to_vec();
        quiet.extend([Event::Ticks(45), Event::Ticks(45)]);
        holds_both(rate, &quiet);
        let mut more = started.to_vec();
        more.extend([
            Event::Ticks(45),
            speakers(Some(true), Some(1)),
            Event::Ticks(25),
            Event::Rate(44_100),
            Event::Ticks(25),
            speakers(None, Some(0)),
            Event::Ticks(25),
        ]);
        holds_both(rate, &more);
    }
}

/// Seed 1204784 of the speakers profile, shrunk: the speakers on, the daemon restarts (and turns
/// them off), the sink moves to 88.2 kHz where the curve cannot play and the Off it takes fails,
/// so the EQ is held. The config holding the set must not be rewritten with that curve at that
/// rate (a coefficient past what the chain holds is clamped to another filter): a chain started
/// from it played +36 dB over unity.
#[test]
fn a_held_eq_does_not_write_a_config_its_rate_cannot_play() {
    holds_both(
        48_000,
        &[
            Event::Fail { kind: 1 },
            Event::Load {
                filters: vec![
                    point(FilterKind::HighShelf, 134.630_844, -7.100_016, 3.203_622),
                    point(FilterKind::LowShelf, 163.976_589, -5.388_895, 0.900_216),
                    point(FilterKind::HighShelf, 3_418.606_45, -12.192_113, 0.547_856),
                    point(FilterKind::HighShelf, 568.560_955, 14.792_883, 0.461_263),
                    point(FilterKind::HighShelf, 8_555.592_717, -1.189_629, 5.180_911),
                    point(FilterKind::Peaking, 3_733.921_516, 4.197_634, 5.433_579),
                ],
                stated: 0.0,
            },
            Event::Speakers {
                on: None,
                set: Some(0),
            },
            Event::Speakers {
                on: Some(true),
                set: None,
            },
            Event::DaemonRestart,
            Event::Rate(88_200),
            Event::Drag {
                pick: 13,
                kind: None,
                f: None,
                g: Some(-0.121_819),
                q: None,
                steps: 1,
                gap_ticks: 2,
            },
            Event::NodeRestart,
        ],
    );
}

/// Seed 5625 of the speakers profile, shrunk: the sink moves to 384 kHz, the command line applies
/// twice (at 48 kHz) and the daemon reads its state, then a speakers change that only saves
/// writes the config at 384 kHz before the first look has checked the read curve there, where a
/// shelf at 14.9 Hz is an unstable biquad; a restart started from it.
#[test]
fn a_save_writes_no_config_whose_points_do_not_play_at_its_rate() {
    holds_both(
        96_000,
        &[
            load(vec![point(
                FilterKind::Peaking,
                3_731.596_494,
                -6.156_831,
                2.731_471,
            )]),
            Event::Rate(384_000),
            load(vec![
                point(FilterKind::LowShelf, 3_284.618_986, -5.701_621, 19.232_823),
                point(FilterKind::HighShelf, 787.556_187, -0.008_255, 2.921_328),
                point(FilterKind::Peaking, 19_678.571_198, -8.733_948, 1.361_537),
                point(FilterKind::HighShelf, 50.070_908, -4.598_966, 0.201_006),
            ]),
            Event::FailNextWrite,
            Event::Cli {
                filters: vec![
                    point(FilterKind::HighShelf, 94.714_457, 3.632_067, 0.810_456),
                    point(FilterKind::HighShelf, 99.522_498, -9.171_464, 8.038_651),
                    point(FilterKind::HighShelf, 702.153_719, 7.633_886, 0.956_540),
                    point(FilterKind::Peaking, 13_141.386_921, -9.094_565, 3.703_136),
                ],
                stated: 0.0,
                gap_ticks: 6,
            },
            Event::Cli {
                filters: vec![
                    point(FilterKind::Peaking, 11_129.414_841, 22.216_178, 0.230_634),
                    point(FilterKind::LowShelf, 15_427.440_669, 15.446_583, 0.160_869),
                    point(FilterKind::LowShelf, 818.875_212, 9.540_379, 1.337_532),
                    point(FilterKind::HighShelf, 14.896_412, -9.502_625, 2.682_378),
                ],
                stated: 0.0,
                gap_ticks: 2,
            },
            Event::Ticks(33),
            Event::Speakers {
                on: Some(false),
                set: None,
            },
            Event::NodeRestart,
        ],
    );
}

/// Seed 174734 (found by 11.2's fresh range), shrunk: crossfeed on at 192 kHz, a drag, a command
/// line apply whose state is read a moment later, an Add and a crossfeed change that keep the
/// config, then the chain starts again from that config: it must play at or under unity, under
/// both models of ids.
#[test]
fn a_config_kept_after_a_command_line_apply_plays_under_unity_after_a_restart() {
    use FilterKind::*;
    holds_both(
        192_000,
        &[
            Event::Crossfeed {
                on: Some(true),
                model: None,
                amount_pct: None,
                delay_ms: None,
                done: true,
            },
            Event::Drag {
                pick: 1,
                kind: Some(HighShelf),
                f: Some(423.4845873055976),
                g: Some(-1.408202989873633),
                q: None,
                steps: 1,
                gap_ticks: 0,
            },
            Event::Cli {
                filters: vec![
                    point(
                        HighShelf,
                        12509.412502107398,
                        23.192142127880878,
                        5.04698586197951,
                    ),
                    point(
                        Peaking,
                        10095.335014492344,
                        -8.020316542853898,
                        17.475255013543975,
                    ),
                    point(
                        LowShelf,
                        812.1583379884456,
                        11.460435518602246,
                        1.1794288065958174,
                    ),
                ],
                stated: -7.1830887584568455,
                gap_ticks: 7,
            },
            Event::Ticks(15),
            Event::Add(point(
                HighShelf,
                27.868242917855955,
                -1.5755263490693103,
                1.9326761521036802,
            )),
            Event::Crossfeed {
                on: None,
                model: None,
                amount_pct: Some(47.25390967587871),
                delay_ms: None,
                done: true,
            },
            Event::NodeRestart,
        ],
    );
}

/// Seed 172526, shrunk: a command-line apply at 384 kHz, a drag, an Add, a rebuild at the rate the
/// sink already ran at, then the slots go back to the config's at the same node and rate. The
/// controller must see the slots went back and send them again: the EQ says it is live, and the
/// node must play the view's points under its preamp, under both models of ids.
#[test]
fn seed_172526_slots_that_go_back_after_a_rebuild_at_the_same_rate_are_sent_again() {
    use FilterKind::*;
    holds_both(
        192_000,
        &[
            Event::Rate(384_000),
            Event::Cli {
                filters: vec![
                    point(
                        HighShelf,
                        111.065_731_339_815,
                        1.873_768_256_835,
                        0.121_911_631_354,
                    ),
                    point(
                        Peaking,
                        171.560_958_187_594,
                        -8.295_154_317_861,
                        6.822_640_042_894,
                    ),
                ],
                stated: 0.0,
                gap_ticks: 5,
            },
            Event::Drag {
                pick: 5,
                kind: None,
                f: Some(22.585_386_910_881),
                g: None,
                q: None,
                steps: 1,
                gap_ticks: 2,
            },
            Event::Add(point(
                HighShelf,
                448.515_192_736_512,
                11.409_322_352_350,
                1.301_883_023_162,
            )),
            Event::Rate(384_000),
            Event::Ticks(25),
            Event::SlotRevert,
        ],
    );
}

/// Seed 1203992, shrunk: sends refused for six seconds and new nodes listed 1.2 s after they
/// start, so every edit is an apply; the sink moves to 384 kHz and a preset is loaded there. Once
/// sends are taken again the EQ says it is live, and the node must play the view's points under its
/// preamp, under both models of ids.
#[test]
fn seed_1203992_edits_applied_while_sends_are_refused_play_as_the_view_says_at_384_khz() {
    use FilterKind::*;
    holds_both(
        44_100,
        &[
            Event::ListDelay { ms: 1200 },
            Event::Ticks(19),
            Event::Drag {
                pick: 9,
                kind: None,
                f: None,
                g: Some(1.227919148580213),
                q: None,
                steps: 4,
                gap_ticks: 1,
            },
            Event::Drag {
                pick: 6,
                kind: None,
                f: None,
                g: Some(-0.6184816599785563),
                q: None,
                steps: 2,
                gap_ticks: 2,
            },
            Event::Ticks(43),
            Event::Ticks(10),
            Event::Ticks(29),
            Event::Cli {
                filters: vec![
                    point(
                        Peaking,
                        67.94447787084933,
                        -3.181307992559713,
                        1.495510122448422,
                    ),
                    point(
                        LowShelf,
                        2157.2926616248596,
                        -11.098614052094783,
                        9.676562496584866,
                    ),
                    point(
                        LowShelf,
                        991.9567892083454,
                        8.494795339212637,
                        2.159994452856449,
                    ),
                    point(
                        LowShelf,
                        6480.945200744217,
                        -10.58763329642574,
                        1.0166814488876295,
                    ),
                    point(
                        HighShelf,
                        177.36925087042857,
                        7.998982252309883,
                        4.309717876161612,
                    ),
                    point(
                        Peaking,
                        3426.4302047422516,
                        -6.210298070400014,
                        0.7164822053776725,
                    ),
                    point(
                        LowShelf,
                        4031.5677995807164,
                        -7.847783936460899,
                        0.934139040809395,
                    ),
                    point(
                        LowShelf,
                        35.663777335388694,
                        0.9292634439869936,
                        0.11484884954781013,
                    ),
                    point(
                        Peaking,
                        28.45733942706801,
                        -1.3077650564946381,
                        0.9169913220396249,
                    ),
                    point(
                        HighShelf,
                        4847.967809691437,
                        7.394210280784854,
                        1.6540732100860263,
                    ),
                    point(
                        LowShelf,
                        8122.009023743965,
                        -8.56772318755124,
                        0.24069472904506795,
                    ),
                    point(
                        HighShelf,
                        2319.4595674361476,
                        4.506803682074853,
                        0.13858051881096226,
                    ),
                    point(
                        Peaking,
                        4427.019577405421,
                        -5.895021583772305,
                        3.290616503059488,
                    ),
                ],
                stated: 0.0,
                gap_ticks: 8,
            },
            Event::RefuseSends { ms: 6000 },
            Event::Drag {
                pick: 12,
                kind: None,
                f: None,
                g: Some(-3.750420217831742),
                q: None,
                steps: 0,
                gap_ticks: 0,
            },
            Event::Load {
                filters: vec![
                    point(
                        HighShelf,
                        690.3436969489012,
                        0.36512590617011753,
                        14.89345422248917,
                    ),
                    point(
                        Peaking,
                        40.97056303858511,
                        7.673220852402842,
                        11.401325942739703,
                    ),
                    point(
                        LowShelf,
                        14.503545544312384,
                        -18.406952626785632,
                        0.4110282775149611,
                    ),
                ],
                stated: -14.283884784939326,
            },
            Event::Load {
                filters: vec![
                    point(
                        HighShelf,
                        886.6306790011173,
                        10.149993517234744,
                        0.44826486604333754,
                    ),
                    point(
                        LowShelf,
                        10285.908995008622,
                        20.031329276345772,
                        0.6425648893211405,
                    ),
                    point(
                        Peaking,
                        150.92823780810187,
                        7.525643516709508,
                        0.3065162477190401,
                    ),
                    point(
                        LowShelf,
                        24.685789750424785,
                        2.50968503575114,
                        0.8103298969145504,
                    ),
                ],
                stated: 0.0,
            },
            Event::Drag {
                pick: 7,
                kind: None,
                f: None,
                g: Some(4.199783444718847),
                q: Some(1.030480330774445),
                steps: 1,
                gap_ticks: 0,
            },
            Event::Drag {
                pick: 10,
                kind: Some(LowShelf),
                f: None,
                g: Some(7.130925153854925),
                q: Some(2.061619247491816),
                steps: 0,
                gap_ticks: 1,
            },
            Event::Toggle(false),
            Event::Drag {
                pick: 13,
                kind: Some(HighShelf),
                f: Some(95.56496542990641),
                g: Some(1.8193659879848667),
                q: None,
                steps: 4,
                gap_ticks: 2,
            },
            Event::Remove { pick: 4 },
            Event::Rate(384000),
            Event::Load {
                filters: vec![
                    point(
                        HighShelf,
                        349.566057660295,
                        -11.475427749702115,
                        9.420934184045729,
                    ),
                    point(
                        Peaking,
                        25.568077171549344,
                        8.375414903925613,
                        0.20179409901524142,
                    ),
                    point(
                        HighShelf,
                        181.6443600426616,
                        1.925924496177343,
                        3.363349427605736,
                    ),
                ],
                stated: -3.573417205191296,
            },
            Event::Ticks(32),
            Event::Ticks(19),
            Event::Ticks(12),
        ],
    );
}

/// Seed 172526's sequence with a cut in place of the boost: the Add leaves the preamp where it
/// was, so no port of its line survives the rebuild at the same rate, and the second rebuild
/// after the repeat must still be put right: the EQ says it is live, and the node must play the
/// view's points under its preamp, under both models of ids.
#[test]
fn seed_172526_with_a_cut_slots_that_go_back_twice_are_sent_again() {
    use FilterKind::*;
    holds_both(
        192_000,
        &[
            Event::Rate(384_000),
            Event::Cli {
                filters: vec![
                    point(
                        HighShelf,
                        111.065_731_339_815,
                        1.873_768_256_835,
                        0.121_911_631_354,
                    ),
                    point(
                        Peaking,
                        171.560_958_187_594,
                        -8.295_154_317_861,
                        6.822_640_042_894,
                    ),
                ],
                stated: 0.0,
                gap_ticks: 5,
            },
            Event::Drag {
                pick: 5,
                kind: None,
                f: Some(22.585_386_910_881),
                g: None,
                q: None,
                steps: 1,
                gap_ticks: 2,
            },
            Event::Add(point(Peaking, 448.515_192_736_512, -6.0, 1.301_883_023_162)),
            Event::Rate(384_000),
            Event::Ticks(25),
            Event::SlotRevert,
        ],
    );
}

/// A command-line apply keeps the loudness's settings in the state it writes, as the real host
/// does: loudness on stays on across the rebuild and the reload that follows, and plays again on
/// the new node.
#[test]
fn loudness_on_survives_a_command_line_rebuild() {
    let events = [
        Event::Toggle(true),
        Event::Loudness {
            on: Some(true),
            strength_pct: None,
            reference_pct: None,
            reference_now: false,
            done: true,
        },
        Event::Volume {
            pct: 30.0,
            daemon: true,
        },
        Event::Ticks(45),
        Event::Cli {
            filters: vec![point(FilterKind::Peaking, 500.0, 2.0, 1.0)],
            stated: 0.0,
            gap_ticks: 3,
        },
        Event::Ticks(45),
        Event::Reload,
        Event::Ticks(45),
        Event::Ticks(45),
        Event::Ticks(45),
        Event::Ticks(45),
    ];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut lab = Lab::new(48_000);
        for (at, event) in events.iter().enumerate() {
            lab.at = at;
            lab.run(event).await.unwrap();
        }
        let saved = world(&lab.shared).state().unwrap().loudness;
        assert!(saved.is_some_and(|ld| ld.on), "{saved:?}");
        let view = lab.control.view().loudness.unwrap();
        assert!(view.on && view.depth_db > 0.0, "{view:?}");
        assert!(lab.ld_checked, "the loudness's liveness rule was asked");
    });
    run_sequence(48_000, &events).unwrap();
}

/// Seeds 1990 and 3932 of the loudness profile, shrunk: a line toward the identity stage fails
/// (a `pw-cli` that takes it and dies, a write that fails once), and the session's repair line
/// sets the ports by name, the low section first. Taken a section at a time, it must not play the
/// identity low section under the high shelf the node still holds.
#[test]
fn a_repair_of_a_line_toward_the_identity_plays_no_part_over_unity() {
    holds_both(
        44_100,
        &[
            Event::Volume {
                pct: 79.38238214187305,
                daemon: true,
            },
            Event::Loudness {
                on: Some(true),
                strength_pct: None,
                reference_pct: None,
                reference_now: false,
                done: false,
            },
            Event::Drag {
                pick: 13,
                kind: None,
                f: None,
                g: Some(-9.236034009223719),
                q: Some(0.21422889671437106),
                steps: 0,
                gap_ticks: 1,
            },
            Event::CliDeath {
                after_line: Some(false),
            },
            Event::Loudness {
                on: None,
                strength_pct: None,
                reference_pct: Some(58.69031068386063),
                reference_now: false,
                done: true,
            },
            Event::Ticks(5),
        ],
    );
    holds_both(
        88_200,
        &[
            Event::VolumeStep {
                delta: -1.0,
                daemon: false,
            },
            Event::Loudness {
                on: Some(true),
                strength_pct: None,
                reference_pct: None,
                reference_now: false,
                done: false,
            },
            Event::Remove { pick: 4 },
            Event::VolumeStep {
                delta: 1.0,
                daemon: false,
            },
            Event::FailNextWrite,
            Event::Ticks(2),
        ],
    );
}

/// A crash takeover over a node left deep (strength 100 %, volume 3 %: about 40 dB of depth),
/// whose first line is not written (the read of the node before it does not answer) or is taken
/// and lost: the line after it, taken a section at a time, must still be cut for the deep shelf
/// the node holds.
#[test]
fn a_takeover_whose_first_line_does_not_land_plays_no_part_over_unity() {
    for lost in [false, true] {
        holds_both(
            48_000,
            &[
                Event::Loudness {
                    on: Some(true),
                    strength_pct: Some(100.0),
                    reference_pct: None,
                    reference_now: false,
                    done: true,
                },
                Event::Volume {
                    pct: 3.0,
                    daemon: true,
                },
                Event::Ticks(45),
                Event::Ticks(45),
                Event::Takeover { lost },
                Event::Ticks(5),
            ],
        );
    }
}

/// The outcome of every seed of a block, one line each (profile, model of ids, seed, and `ok` or
/// the failing event and what broke), written to the file `SVOLDER_LAB_OUT`: two runs of it
/// compare a change to the lab that must not change what it finds. `SVOLDER_LAB_SEED` and
/// `SVOLDER_LAB_RUNS` choose the block, `SVOLDER_LAB_THREADS` the threads (4 by default). With
/// `SVOLDER_LAB_TRACE` set, each line is followed by the run's trace (every event, node start and
/// `set-param` with its preamp): a stricter comparison, for a change to the controller.
#[test]
#[ignore = "an equivalence check: make preamp-lab-outcomes"]
fn lab_outcomes() {
    let first = from_env("SVOLDER_LAB_SEED", 1_000);
    let runs = from_env("SVOLDER_LAB_RUNS", 400);
    let threads = from_env("SVOLDER_LAB_THREADS", 4).max(1);
    let out = std::env::var_os("SVOLDER_LAB_OUT").expect("SVOLDER_LAB_OUT names the file");
    let traced = std::env::var_os("SVOLDER_LAB_TRACE").is_some();
    type Sequence = fn(u64) -> (u32, Vec<Event>);
    let profiles: [(&str, Sequence); 5] = [
        ("plain", sequence_of),
        ("targeted", sequence_of_targeted),
        ("speakers", sequence_of_speakers),
        ("highrate", sequence_of_highrate),
        ("loudness", sequence_of_loudness),
    ];
    let jobs: Vec<(usize, Ids, u64)> = (0..profiles.len())
        .flat_map(|profile| {
            [Ids::Fresh, Ids::Reused]
                .into_iter()
                .flat_map(move |ids| (first..first + runs).map(move |seed| (profile, ids, seed)))
        })
        .collect();
    let next = AtomicUsize::new(0);
    let lines = Mutex::new(Vec::new());
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let Some(&(profile, ids, seed)) = jobs.get(next.fetch_add(1, Ordering::SeqCst))
                    else {
                        return;
                    };
                    IDS.set(ids);
                    let (name, sequence) = profiles[profile];
                    let (rate, events) = sequence(seed);
                    let trace = Mutex::new(Vec::new());
                    let result = run_traced(rate, &events, traced.then_some(&trace));
                    let outcome = match result {
                        Ok(()) => "ok".to_string(),
                        Err(failure) => format!("event {}: {}", failure.event, failure.what),
                    };
                    let mut line = format!("{name} {ids:?} {seed} {outcome}");
                    for step in trace.into_inner().unwrap_or_default() {
                        line.push_str("\n    ");
                        line.push_str(&step);
                    }
                    crate::locked(&lines).push((profile, ids == Ids::Reused, seed, line));
                }
            });
        }
    });
    let mut lines = lines.into_inner().unwrap_or_default();
    lines.sort_by_key(|(profile, reused, seed, _)| (*profile, *reused, *seed));
    let text: String = lines.into_iter().map(|(.., line)| line + "\n").collect();
    std::fs::write(&out, text).expect("the outcomes are written");
    eprintln!(
        "lab outcomes: {} sequences on {threads} threads in {:.1?}",
        jobs.len(),
        started.elapsed()
    );
}

/// [`dense_peak`] leaves out identity slots and nothing else, and gives what [`dense_scan`] of
/// every slot gives, bit for bit: curves of up to five filters (cuts and boosts to 30 dB, some at
/// the cut-off near Nyquist, some poles over equal zeros) among identity slots of either zero
/// sign, at every rate up to 768 kHz, and slots that are all the identity.
#[test]
fn the_dense_peak_without_identity_slots_is_the_full_scan_bit_for_bit() {
    let mut rng = Rng::new(0x1D_E471);
    let identity = |rng: &mut Rng| {
        let mut c = [1.0, 0.0, 0.0, 0.0, 0.0];
        for value in &mut c[1..] {
            if rng.chance(50) {
                *value = -0.0;
            }
        }
        c
    };
    let mut cases: Vec<(Vec<[f64; 5]>, u32)> = Vec::new();
    for at in 0..120 {
        let rate = HIGH_RATES[at % HIGH_RATES.len()];
        let mut sets: Vec<[f64; 5]> = (0..SLOTS).map(|_| identity(&mut rng)).collect();
        for _ in 0..rng.below(6) {
            let mut f = filter(&mut rng, false);
            if rng.chance(25) {
                f.f = f64::from(rate) * 0.49;
            }
            let c = svolder_profiles::tone::response::coefficients(f.kind, f.f, f.g, f.q, rate);
            sets[rng.below(SLOTS as u64) as usize] = c.map(|v| f64::from(v as f32));
        }
        if rng.chance(20) {
            // Poles over equal zeros: plays as the identity, but is not the exact one.
            let (a1, a2) = (rng.range(-1.5, 1.5), rng.range(-0.5, 0.9));
            sets[rng.below(SLOTS as u64) as usize] = [1.0, a1, a2, a1, a2];
        }
        cases.push((sets, rate));
    }
    for rate in [44_100, 384_000, 768_000] {
        cases.push(((0..SLOTS).map(|_| identity(&mut rng)).collect(), rate));
        // A sharp peak with poles over equal zeros a hair beside it: their angle is a point of
        // the grid, which moves the zoom on the peak, so leaving that slot out shows.
        let mut sets: Vec<[f64; 5]> = (0..SLOTS).map(|_| identity(&mut rng)).collect();
        sets[3] = svolder_profiles::tone::response::coefficients(
            FilterKind::Peaking,
            1_000.0,
            12.0,
            10.0,
            rate,
        );
        let w = 2.0 * std::f64::consts::PI * 1_001.0 / f64::from(rate);
        let (a1, a2) = (-2.0 * 0.9 * w.cos(), 0.81);
        sets[9] = [1.0, a1, a2, a1, a2];
        cases.push((sets, rate));
    }
    for (sets, rate) in &cases {
        assert_eq!(
            dense_peak(sets, *rate).to_bits(),
            dense_scan(sets, *rate).to_bits(),
            "at {rate} Hz: {sets:?}"
        );
    }
}
