// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loudness stage following the computer's volume: the lifecycle (what keeps the chain), the
//! glide and its cadence (one line a tick, nothing once it arrives), the level from the look and
//! from a write's hint, and how the stage rides 0.36.11's lines, reverts, chases and rebuilds.
//! The stage is cut-only, so every state any of these leaves plays at or under unity; these
//! tests prove the stage is where the volume says, and that nothing loops or is left hanging.

use super::*;
use crate::eq::loudness::{self, LoudnessSettings};
use crate::route::SinkLevel;

/// The computer's volume at `pct` percent on the volume scale.
fn level(pct: f64) -> SinkLevel {
    SinkLevel {
        linear: (pct / 100.0).powi(3),
        muted: false,
    }
}

fn ld(on: bool) -> LoudnessEdit {
    LoudnessEdit {
        on: Some(on),
        done: true,
        ..LoudnessEdit::default()
    }
}

async fn ticks(control: &mut EqControl, count: u64) {
    for _ in 0..count {
        control.tick().await;
    }
}

/// The loudness ports of `send`, in order.
fn ld_ports(send: &[(String, f64)]) -> Vec<(String, f64)> {
    send.iter()
        .filter(|(name, _)| name.starts_with("lld") || name.starts_with("rld"))
        .cloned()
        .collect()
}

/// The ports that set the stage for the depth `d_e` at `rate`.
fn stage_ports(d_e: f64, rate: u32) -> Vec<(String, f64)> {
    crate::eq::live::loudness_params(&loudness::stage(d_e, rate).sections)
}

/// Whether `send` puts exactly the stage for `d_e` at `rate` on the node, in whatever order.
fn exactly(send: &[(String, f64)], d_e: f64, rate: u32) -> bool {
    let sent: HashMap<String, f64> = ld_ports(send).into_iter().collect();
    let want: HashMap<String, f64> = stage_ports(d_e, rate).into_iter().collect();
    sent == want
}

/// How much deeper than the stage's own a line's low section cuts, in dB (its gain is the stage's
/// scaled down).
fn extra_cut_db(send: &[(String, f64)], d_e: f64, rate: u32) -> f64 {
    let sent: HashMap<String, f64> = ld_ports(send).into_iter().collect();
    let want: HashMap<String, f64> = stage_ports(d_e, rate).into_iter().collect();
    20.0 * (want["lld1:b0"] / sent["lld1:b0"]).log10()
}

/// Whether `send` puts the stage for `d_e` at `rate` on the node, in whatever order: the high
/// section and the low section's pole exactly, the low section's gain at or under the stage's own
/// (a line toward a shallower stage cuts it for the high section the node may still hold). The
/// identity exactly.
fn carries(send: &[(String, f64)], d_e: f64, rate: u32) -> bool {
    let sent: HashMap<String, f64> = ld_ports(send).into_iter().collect();
    let want = stage_ports(d_e, rate);
    sent.len() == want.len()
        && want.iter().all(|(name, value)| {
            let Some(got) = sent.get(name) else {
                return false;
            };
            let gain = name.ends_with("ld1:b0") || name.ends_with("ld1:b1");
            match gain && d_e > 0.0 {
                true => got.signum() == value.signum() && got.abs() <= value.abs() + 1e-9,
                false => got == value,
            }
        })
}

/// The section `n` (`ld1`, `ld2`) of `channel` as `held` lists it, `[b0, b1, b2, a1, a2]` over
/// `a0`; the identity for ports it does not list.
fn held_section(held: &HashMap<String, f64>, channel: char, n: &str) -> [f64; 5] {
    let at = |port: &str, or: f64| *held.get(&format!("{channel}{n}:{port}")).unwrap_or(&or);
    let a0 = at("a0", 1.0);
    [
        at("b0", 1.0) / a0,
        at("b1", 0.0) / a0,
        at("b2", 0.0) / a0,
        at("a1", 0.0) / a0,
        at("a2", 0.0) / a0,
    ]
}

/// Play `lines` on a node that starts holding `start` (loudness ports; the identity for the ones
/// it does not list), the node taking each line a section at a time: after every section of every
/// line, the loudness stage alone plays at most 0 dB on both channels. `resets` lists the lines
/// before which the node went back to its config's identity unseen (a rebuild no look saw).
fn parts_under_unity(
    start: &[(String, f64)],
    lines: &[Vec<(String, f64)>],
    resets: &[usize],
    rate: u32,
) {
    parts_under_unity_lost(start, lines, resets, &[], rate);
}

/// [`parts_under_unity`], with the lines `lost` never reaching the node (a `pw-cli` that took
/// them and died).
fn parts_under_unity_lost(
    start: &[(String, f64)],
    lines: &[Vec<(String, f64)>],
    resets: &[usize],
    lost: &[usize],
    rate: u32,
) {
    let mut held: HashMap<String, f64> = start.iter().cloned().collect();
    for (at, line) in lines.iter().enumerate() {
        if resets.contains(&at) {
            held = stage_ports(0.0, rate).into_iter().collect();
        }
        if lost.contains(&at) {
            continue;
        }
        for (name, value) in ld_ports(line) {
            let done = name.ends_with(":a2");
            held.insert(name.clone(), value);
            if !done {
                continue;
            }
            for channel in ['l', 'r'] {
                let sets = [
                    held_section(&held, channel, "ld1"),
                    held_section(&held, channel, "ld2"),
                ];
                let peak = svolder_profiles::tone::response::played_peak_db_of(&sets, &[], rate);
                assert!(
                    peak <= 1e-9,
                    "line {at} plays {peak:+.4} dB on {channel} after {name}: {sets:?}"
                );
            }
        }
    }
}

fn depth(control: &EqControl) -> f64 {
    control.view().loudness.unwrap().depth_db
}

/// The depth the volume `pct` gives at the default settings (strength 50 %, reference 100 %).
fn target(pct: f64) -> f64 {
    loudness::quantise_db(loudness::depth_db(
        level(pct).linear,
        &LoudnessSettings::default(),
    ))
}

fn applies(chain: &FakeChain) -> usize {
    chain
        .calls()
        .iter()
        .filter(|call| matches!(call, ChainCall::Apply(_)))
        .count()
}

fn removes(chain: &FakeChain) -> usize {
    chain
        .calls()
        .iter()
        .filter(|call| **call == ChainCall::Remove)
        .count()
}

/// Tick until the stage plays the depth `want`, at most `limit` ticks; the ticks it took.
async fn glide_to(control: &mut EqControl, want: f64, limit: u64) -> u64 {
    for n in 1..=limit {
        control.tick().await;
        if (depth(control) - want).abs() < 1e-9 {
            return n;
        }
    }
    panic!(
        "the stage plays {} dB, not {want} dB, after {limit} ticks",
        depth(control)
    );
}

/// The EQ on with a curve, over a node with the loudness stage's controls and the volume at
/// `pct`, loudness on and arrived, every port seen by a look.
async fn eq_and_loudness_at(pct: f64) -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    params.set_level(Some(level(pct)));
    control
        .load(
            "Curve".into(),
            None,
            correction(vec![pk(100.0, 3.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    control.loudness(ld(true)).await.unwrap();
    glide_to(&mut control, target(pct), 100).await;
    ticks(&mut control, 2 * WATCH_EVERY).await;
    (control, chain, params)
}

/// A flat chain for loudness alone over a node with its controls, the volume at `pct`, arrived.
async fn loudness_alone_at(pct: f64) -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    params.set_level(Some(level(pct)));
    control.loudness(ld(true)).await.unwrap();
    glide_to(&mut control, target(pct), 100).await;
    ticks(&mut control, 2 * WATCH_EVERY).await;
    (control, chain, params)
}

#[tokio::test]
async fn on_with_no_chain_applies_a_flat_chain_saves_it_and_sends_the_stage() {
    let dir = Dir::new();
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    params.with_loudness_ports();
    params.set_level(Some(level(50.0)));
    control
        .loudness(LoudnessEdit {
            on: Some(true),
            strength_pct: Some(40.0),
            ..ld(true)
        })
        .await
        .unwrap();
    let calls = chain.calls();
    let [ChainCall::Apply(request), ..] = calls.as_slice() else {
        panic!("{calls:?}");
    };
    assert!(request.flat, "the chain plays flat for loudness alone");
    let saved = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(
        saved.loudness,
        Some(LoudnessSettings {
            on: true,
            strength_pct: 40.0,
            reference_pct: 100.0
        })
    );
    assert!(saved.on && saved.flat);
    let want = loudness::quantise_db(loudness::depth_db(
        level(50.0).linear,
        &saved.loudness.unwrap(),
    ));
    glide_to(&mut control, want, 100).await;
    assert!(carries(params.sent().last().unwrap(), want, 48_000));
    let view = control.view();
    assert!(!view.on, "the EQ itself stays off");
    assert!(view.loudness.unwrap().following);

    // A fresh controller over the file reads it back on, the chain kept.
    let again = EqControl::new(
        Arc::new(FakeChain::with_state(saved)),
        Box::new(FakeParams::new()),
        Keep::File(dir.state()),
    );
    assert!(again.view().loudness.unwrap().on);
    assert!(
        again.owed.is_none(),
        "a chain for loudness is not owed away"
    );
}

#[tokio::test]
async fn everything_off_removes_the_chain() {
    let (mut control, chain, _params) = loudness_alone_at(50.0).await;
    control.loudness(ld(false)).await.unwrap();
    assert_eq!(removes(&chain), 1, "loudness was the last reason");
    assert!(!control.runs);
    assert_eq!(depth(&control), 0.0);

    // The EQ and loudness on; the EQ off keeps it, loudness off then removes it.
    let (mut control, chain, _params) = eq_and_loudness_at(50.0).await;
    control.set_on(false).await.unwrap();
    assert_eq!(removes(&chain), 0);
    control.loudness(ld(false)).await.unwrap();
    assert_eq!(removes(&chain), 1);
}

#[tokio::test]
async fn the_eq_off_with_loudness_on_goes_flat_live() {
    let (mut control, chain, params) = eq_and_loudness_at(50.0).await;
    let applied = applies(&chain);
    let lines = params.lines().len();
    control.set_on(false).await.unwrap();
    assert_eq!(applies(&chain), applied, "no apply");
    assert_eq!(removes(&chain), 0, "no remove");
    assert_eq!(params.lines().len(), lines + 1, "one line");
    let last = params.sent().last().unwrap().clone();
    let identity = slot_params(1, None, 48_000);
    assert_eq!(value(&last, "l01:b0"), Some(identity[0].1), "slot 1 flat");
    assert!(carries(&last, target(50.0), 48_000), "the stage rides it");
    let view = control.view();
    assert!(!view.on && view.loudness.unwrap().on);
    assert!(control.runs && control.flat);
    assert_eq!(view.note, None, "no pass-through: loudness keeps the chain");
}

// The speakers' half of this is in tests/vs.rs.
#[tokio::test]
async fn crossfeed_off_with_loudness_on_keeps_the_chain() {
    let (mut control, chain, params) = loudness_alone_at(50.0).await;
    let on = CrossfeedEdit {
        on: Some(true),
        done: true,
        ..CrossfeedEdit::default()
    };
    control.crossfeed(on).await.unwrap();
    assert!(control.view().crossfeed.unwrap().on);
    let lines = params.lines().len();
    control
        .crossfeed(CrossfeedEdit {
            on: Some(false),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    assert_eq!(removes(&chain), 0, "loudness keeps the chain");
    assert!(control.runs && control.flat);
    let last = params.sent().last().unwrap().clone();
    assert!(params.lines().len() > lines);
    assert_eq!(value(&last, "lmix:Gain 2"), Some(0.0), "the stage off live");
    assert!(carries(&last, target(50.0), 48_000));
    ticks(&mut control, IDLE_KEEP + WATCH_EVERY).await;
    assert_eq!(removes(&chain), 0, "not a pass-through that idles away");
}

#[tokio::test]
async fn a_volume_change_glides_a_step_a_tick_arrives_and_then_sends_nothing() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    assert_eq!(depth(&control), target(50.0));
    params.set_level(Some(level(25.0)));
    let want = target(25.0);
    assert!(want - target(50.0) > 5.0, "{want}");
    let mut last = depth(&control);
    let mut moving = 0;
    for _ in 0..200 {
        let lines = params.lines().len();
        control.tick().await;
        let now = depth(&control);
        assert!((now - last).abs() <= 0.5 + 1e-9, "{last} -> {now}");
        if now != last {
            moving += 1;
            assert_eq!(params.lines().len(), lines + 1, "one line a step");
            assert!(carries(params.sent().last().unwrap(), now, 48_000));
        }
        last = now;
        if now == want {
            break;
        }
    }
    assert_eq!(last, want, "it arrives");
    assert!(moving >= 18, "{moving} steps");
    // Once there: the one repeat a second after the last line, then nothing.
    let lines = params.lines().len();
    ticks(&mut control, 2 * WATCH_EVERY).await;
    assert!(params.lines().len() <= lines + 1);
    let lines = params.lines().len();
    ticks(&mut control, 3 * WATCH_EVERY).await;
    assert_eq!(params.lines().len(), lines, "nothing once it has arrived");
}

#[tokio::test]
async fn a_writes_hint_starts_the_glide_before_the_next_look_and_never_waits_on_the_lock() {
    let (control, _chain, params) = eq_and_loudness_at(50.0).await;
    let hint = control.level_hint();
    let control = Arc::new(tokio::sync::Mutex::new(control));
    {
        // The controller is held (a tick inside a pw-dump): the write's hint goes in anyway.
        let _held = control.lock().await;
        let hint = hint.clone();
        std::thread::spawn(move || hint.set(level(25.0)))
            .join()
            .unwrap();
    }
    // The graph says 25 % too from now on, but no look has read it yet.
    params.set_level(Some(level(25.0)));
    let mut control = control.lock().await;
    while control.ticks % WATCH_EVERY >= WATCH_EVERY - 2 {
        control.tick().await;
    }
    let reads = params.reads();
    let before = depth(&control);
    control.tick().await;
    assert_eq!(params.reads(), reads, "no look on this tick");
    assert_eq!(depth(&control), before + 0.5, "the glide started");
    glide_to(&mut control, target(25.0), 100).await;
}

#[tokio::test]
async fn a_tick_with_a_drag_sends_one_line_with_slots_preamp_and_loudness() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    params.set_level(Some(level(25.0)));
    ticks(&mut control, WATCH_EVERY).await;
    assert!(depth(&control) > target(50.0), "gliding");
    while control.ticks % WATCH_EVERY >= WATCH_EVERY - 2 {
        control.tick().await;
    }
    control.edit(gain(0, 5.0, false)).await.unwrap();
    let (lines, reads) = (params.lines().len(), params.reads());
    control.tick().await;
    assert_eq!(params.lines().len(), lines + 1, "one line");
    assert_eq!(params.reads(), reads, "no read but the line's own");
    let last = params.sent().last().unwrap().clone();
    assert!(touches(&last, 1), "the slot");
    assert!(value(&last, "lpre:Mult").is_some(), "the preamp");
    assert!(carries(&last, depth(&control), 48_000), "the stage");
}

#[tokio::test]
async fn a_glide_step_with_nothing_else_due_changes_only_the_loudness_ports() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    let before = params.sent().last().unwrap().clone();
    for name in ["l01:b0", "lpre:Mult"] {
        assert!(control.confirmed.contains(name), "{name} seen at its value");
    }
    let slot_line = control.line_of["l01:b0"];
    let hint = control.level_hint();
    hint.set(level(25.0));
    params.set_level(Some(level(25.0)));
    let lines = params.lines().len();
    control.tick().await;
    assert_eq!(params.lines().len(), lines + 1);
    let line = params.sent().last().unwrap().clone();
    assert_eq!(line.len(), before.len(), "an ordinary full line");
    for ((name, value), (was_name, was)) in line.iter().zip(&before) {
        assert_eq!(name, was_name);
        let loudness = name.starts_with("lld") || name.starts_with("rld");
        if !loudness {
            assert_eq!(value, was, "{name} is sent at the value it holds");
        }
    }
    assert_ne!(ld_ports(&line), ld_ports(&before), "the stage moved");
    for name in ["l01:b0", "lpre:Mult"] {
        assert!(control.confirmed.contains(name), "{name} stays confirmed");
    }
    assert_eq!(
        control.line_of["l01:b0"], slot_line,
        "the slot's line stays"
    );
    assert_eq!(control.line_of["lld1:b0"].0, control.lines);
    assert!(!control.confirmed.contains("lld1:b0"));
}

#[tokio::test]
async fn a_muted_or_unread_volume_holds_the_stage() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    let at = depth(&control);
    let hint = control.level_hint();
    hint.set(SinkLevel {
        muted: true,
        ..level(25.0)
    });
    params.set_level(Some(SinkLevel {
        muted: true,
        ..level(25.0)
    }));
    ticks(&mut control, 3 * WATCH_EVERY).await;
    let view = control.view().loudness.unwrap();
    assert_eq!(view.depth_db, at, "muted: held");
    assert!(!view.following);

    params.set_level(None);
    ticks(&mut control, 3 * WATCH_EVERY).await;
    let view = control.view().loudness.unwrap();
    assert_eq!(view.depth_db, at, "unread: held");
    assert!(!view.following);
    assert_eq!(view.volume_pct, None);

    params.set_level(Some(level(25.0)));
    glide_to(&mut control, target(25.0), 100).await;
    assert!(control.view().loudness.unwrap().following);
}

#[tokio::test]
async fn at_or_above_the_reference_the_stage_is_the_identity() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    params.set_level(Some(level(100.0)));
    glide_to(&mut control, 0.0, 100).await;
    // The step onto the identity cuts flat for the high shelf the node may still hold; the
    // repeat a second later puts the exact identity on.
    ticks(&mut control, 2 * WATCH_EVERY).await;
    assert!(carries(params.sent().last().unwrap(), 0.0, 48_000));
    assert!(loudness::stage(0.0, 48_000).is_identity());
    params.set_level(Some(level(120.0)));
    ticks(&mut control, 2 * WATCH_EVERY).await;
    assert_eq!(depth(&control), 0.0, "a boosted sink is flat too");
    control
        .loudness(LoudnessEdit {
            reference_pct: Some(40.0),
            ..LoudnessEdit::default()
        })
        .await
        .unwrap();
    params.set_level(Some(level(60.0)));
    ticks(&mut control, 2 * WATCH_EVERY).await;
    assert_eq!(depth(&control), 0.0, "over the reference");
}

#[tokio::test]
async fn a_rate_change_a_revert_and_a_same_id_restart_send_the_stage_again() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    let at = depth(&control);
    let identity = stage_ports(0.0, 48_000);
    let put_back = |params: &FakeParams| {
        let ports: Vec<(&str, f64)> = identity.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        params.set_ports(&ports);
    };

    // The rate: the graph is rebuilt (the stage back to the config's identity).
    params.set_rate(Some(96_000));
    put_back(&params);
    let lines = params.lines().len();
    ticks(&mut control, WATCH_EVERY + 1).await;
    let again = &params.sent()[lines..];
    assert!(
        again.iter().any(|send| carries(send, at, 96_000)),
        "sent again at the new rate"
    );

    // A revert at the same node and rate.
    ticks(&mut control, 2 * WATCH_EVERY).await;
    put_back(&params);
    let lines = params.lines().len();
    ticks(&mut control, WATCH_EVERY + 1).await;
    assert!(
        params.sent()[lines..]
            .iter()
            .any(|send| carries(send, at, 96_000)),
        "the revert sends it again"
    );

    // The same id, another serial: a chain started again from its config.
    ticks(&mut control, 2 * WATCH_EVERY).await;
    let mut restarted = node(1, Some(96_000));
    restarted.serial = Some(99);
    restarted.params.extend(crate::eq::live::crossfeed_params(
        &crate::eq::CrossfeedStage::bypass(),
    ));
    restarted.params.extend(identity.clone());
    params.set_node(Some(restarted));
    let lines = params.lines().len();
    ticks(&mut control, WATCH_EVERY + 1).await;
    assert!(
        params.sent()[lines..]
            .iter()
            .any(|send| carries(send, at, 96_000)),
        "the new node is sent it"
    );
    assert_eq!(depth(&control), at, "nothing reset the depth");
}

#[tokio::test]
async fn a_pw_cli_lagging_through_a_glide_is_chased_once_after_it() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    params.holds(false);
    params.set_level(Some(level(25.0)));
    // The look sees the new volume, then a line a tick until it arrives, none of them seen land.
    while depth(&control) == target(50.0) {
        control.tick().await;
    }
    let start = params.lines().len();
    let chased = control.chased;
    let mut ticked = 1;
    while depth(&control) != target(25.0) {
        control.tick().await;
        ticked += 1;
    }
    assert_eq!(params.lines().len() - start, ticked - 1, "one line a tick");
    assert_eq!(control.chased, chased, "no chase during the glide");
    // Three looks with the node still behind: one chase (and the one repeat), no loop.
    let lines = params.lines().len();
    ticks(&mut control, 3 * WATCH_EVERY).await;
    let after = params.lines().len() - lines;
    assert!((1..=2).contains(&after), "{after} lines after the glide");
    assert!(control.chased > chased, "the chase came after it");
    params.holds(true);
    let lines = params.lines().len();
    ticks(&mut control, 3 * WATCH_EVERY).await;
    assert!(params.lines().len() - lines <= 1, "settled");
}

#[tokio::test]
async fn an_owner_start_with_loudness_on_keeps_it_on_and_sends_it_at_the_first_look() {
    let state = HostEqState {
        on: true,
        name: "test".into(),
        origin: None,
        target: String::new(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on: false,
        flat: true,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: Some(LoudnessSettings {
            on: true,
            ..LoudnessSettings::default()
        }),
    };
    let chain = FakeChain::with_state(state);
    let params = FakeParams::new();
    params.with_loudness_ports();
    params.set_level(Some(level(50.0)));
    let mut control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params.clone()),
        Keep::Memory,
    );
    assert!(!control.sync_running().await);
    assert!(control.view().loudness.unwrap().on);
    control.tick().await;
    assert!(
        params
            .sent()
            .first()
            .is_some_and(|send| ld_ports(send) != stage_ports(0.0, 48_000)),
        "the first look's line carries the stage"
    );
    glide_to(&mut control, target(50.0), 100).await;
    assert_eq!(chain.calls(), Vec::new(), "nothing applied or removed");
}

#[tokio::test]
async fn a_graceful_stop_sends_the_stage_flat_and_keeps_a_chain_for_loudness() {
    let (mut control, chain, params) = loudness_alone_at(50.0).await;
    let lines = params.lines().len();
    control.shutdown().await;
    assert_eq!(params.lines().len(), lines + 1, "a line of its own");
    assert!(carries(params.sent().last().unwrap(), 0.0, 48_000));
    assert_eq!(removes(&chain), 0, "the chain stays for the next owner");
    let view = control.view().loudness.unwrap();
    assert!(view.on, "the setting stays on");
    assert_eq!(view.depth_db, 0.0);

    let (mut control, chain, params) = eq_and_loudness_at(50.0).await;
    control.shutdown().await;
    assert!(carries(params.sent().last().unwrap(), 0.0, 48_000));
    assert_eq!(removes(&chain), 0);
}

#[tokio::test]
async fn a_legacy_or_no_live_chain_makes_loudness_unavailable_with_a_note() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    params.set_node(Some(LiveNode {
        params: HashMap::new(),
        ..node(1, Some(48_000))
    }));
    ticks(&mut control, 2).await;
    assert!(control.legacy);
    let calls = chain.calls().len();
    control.loudness(ld(true)).await.unwrap();
    assert_eq!(chain.calls().len(), calls, "nothing applied");
    let view = control.view().loudness.unwrap();
    assert!(view.unavailable.is_some(), "{view:?}");
    assert!(!view.following);

    let (mut control, chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    params.set_level(Some(level(50.0)));
    control.set_on(true).await.unwrap();
    control.live = false;
    control.note = Some(LIVE_NOTE.to_string());
    let calls = chain.calls().len();
    let lines = params.lines().len();
    control.loudness(ld(true)).await.unwrap();
    ticks(&mut control, 3 * WATCH_EVERY).await;
    assert_eq!(chain.calls().len(), calls, "nothing applied");
    assert_eq!(params.lines().len(), lines, "nothing sent");
    let view = control.view().loudness.unwrap();
    assert!(view.unavailable.is_some(), "{view:?}");
    assert!(!view.following);
}

#[tokio::test]
async fn a_chain_without_the_stage_is_rebuilt_once_at_turn_on() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_level(Some(level(50.0)));
    control.set_on(true).await.unwrap();
    ticks(&mut control, 2).await;
    control.loudness(ld(true)).await.unwrap();
    assert_eq!(applies(&chain), 2, "rebuilt once");
    let note = control.view().loudness.unwrap().note.unwrap_or_default();
    assert!(note.contains("rebuilt once to add loudness"), "{note}");
    // The rebuilt chain has the stage's controls.
    params.with_loudness_ports();
    glide_to(&mut control, target(50.0), 100).await;
    control.loudness(ld(false)).await.unwrap();
    control.loudness(ld(true)).await.unwrap();
    assert_eq!(applies(&chain), 2, "once");
}

#[tokio::test]
async fn a_held_recall_lifted_once_idle_carries_the_stage_under_the_curves_own_preamp() {
    let (mut control, chain, params) = setup(Keep::Memory);
    chain.restarts_node_of(&params);
    params.with_loudness_ports();
    params.set_level(Some(level(50.0)));
    control
        .load(
            "Hot".into(),
            None,
            correction(vec![pk(100.0, 10.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    control.loudness(ld(true)).await.unwrap();
    glide_to(&mut control, target(50.0), 100).await;
    params.set_playing(true);
    ticks(&mut control, WATCH_EVERY).await;
    // A gentler curve while a stream plays: held at the hot curve's cap.
    control
        .load(
            "Gentle".into(),
            None,
            correction(vec![pk(100.0, 2.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    params.set_playing(false);
    let applied = applies(&chain);
    ticks(&mut control, IDLE_KEEP + 2 * WATCH_EVERY).await;
    assert_eq!(applies(&chain), applied + 1, "applied again once idle");
    // The new node starts from its config's identity stage; the line after the apply (marked by
    // the apply and, on the same tick, by the first look at the unvouched node) puts it back.
    let last = params.sent().last().unwrap().clone();
    assert!(carries(&last, target(50.0), 48_000), "the stage again");
    let own = played_preamp(&correction(vec![pk(100.0, 2.0, 1.0)], 0.0), 48_000);
    assert!(
        (preamp_of(&last) - own).abs() < 1e-6,
        "{}",
        preamp_of(&last)
    );
}

#[tokio::test]
async fn a_glide_line_no_look_saw_land_is_chased_once_with_the_loudness_ports() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    // One step, which the node does not take.
    params.holds(false);
    params.set_level(Some(level(55.0)));
    let want = target(55.0);
    glide_to(&mut control, want, 100).await;
    let lines = params.lines().len();
    ticks(&mut control, 3 * WATCH_EVERY).await;
    let chased: Vec<_> = params.sent()[lines..]
        .iter()
        .filter(|send| carries(send, want, 48_000))
        .cloned()
        .collect();
    assert!(!chased.is_empty(), "chased with the stage");
    assert!(control.chased > 0, "a chase, not only the repeat");
    assert!(chased.len() <= 2, "once (and the repeat)");
}

#[tokio::test]
async fn a_rebuild_that_puts_a_confirmed_stage_back_is_a_revert_and_sends_it_again() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    assert!(control.confirmed.contains("lld1:b0"));
    let identity = stage_ports(0.0, 48_000);
    let ports: Vec<(&str, f64)> = identity.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    params.set_ports(&ports);
    let lines = params.lines().len();
    ticks(&mut control, WATCH_EVERY + 1).await;
    assert_eq!(control.reverts.len(), 1, "a revert");
    assert!(
        params.sent()[lines..]
            .iter()
            .any(|send| carries(send, target(50.0), 48_000))
    );
}

#[tokio::test]
async fn a_failed_send_keeps_the_played_depth_and_the_next_line_steps_from_it() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    let at = depth(&control);
    params.takes(false);
    let hint = control.level_hint();
    hint.set(level(25.0));
    params.set_level(Some(level(25.0)));
    control.tick().await;
    assert_eq!(control.ld.played, at, "nothing went");
    assert_eq!(depth(&control), at);
    params.takes(true);
    control.tick().await;
    assert_eq!(depth(&control), at + 0.5, "one step from it");
    assert!(carries(params.sent().last().unwrap(), at + 0.5, 48_000));
}

#[tokio::test]
async fn a_failed_go_flat_line_is_taken_back_and_the_next_line_carries_the_stage() {
    let (mut control, chain, params) = eq_and_loudness_at(50.0).await;
    params.takes(false);
    chain.refuse(Some(EqFailure {
        what: "no".into(),
        next: "later".into(),
    }));
    control.set_on(false).await.unwrap_err();
    params.takes(true);
    chain.refuse(None);
    let lines = params.lines().len();
    ticks(&mut control, 2).await;
    assert!(params.lines().len() > lines, "the take-back goes");
    assert!(carries(params.sent().last().unwrap(), target(50.0), 48_000));
}

#[tokio::test]
async fn a_move_above_384_khz_turns_crossfeed_off_and_loudness_plays_on() {
    let (mut control, chain, params) = loudness_alone_at(50.0).await;
    control
        .crossfeed(CrossfeedEdit {
            on: Some(true),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    params.set_rate(Some(705_600));
    ticks(&mut control, 2 * WATCH_EVERY).await;
    let view = control.view();
    let xf = view.crossfeed.unwrap();
    assert!(!xf.on);
    assert!(xf.note.unwrap_or_default().contains("384"));
    assert_eq!(removes(&chain), 0, "the chain kept");
    assert!(control.runs);
    assert!(view.loudness.unwrap().on);
    assert!(
        params
            .sent()
            .iter()
            .any(|send| carries(send, target(50.0), 705_600)),
        "the stage at the new rate"
    );
    // And turning it on up there is not refused for the rate.
    control.loudness(ld(false)).await.unwrap();
    control.loudness(ld(true)).await.unwrap();
}

#[tokio::test]
async fn adopting_a_flat_state_with_loudness_on_owes_no_removal() {
    let state = HostEqState {
        on: true,
        name: "test".into(),
        origin: None,
        target: String::new(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on: false,
        flat: true,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: Some(LoudnessSettings {
            on: true,
            ..LoudnessSettings::default()
        }),
    };
    let chain = FakeChain::with_state(state);
    let (mut control, params) = {
        let params = FakeParams::new();
        params.with_loudness_ports();
        let control = EqControl::new(
            Arc::new(chain.clone()),
            Box::new(params.clone()),
            Keep::Memory,
        );
        (control, params)
    };
    assert!(control.owed.is_none());
    params.set_level(Some(level(50.0)));
    ticks(&mut control, IDLE_KEEP + WATCH_EVERY).await;
    assert_eq!(removes(&chain), 0, "the chain kept");
    assert!(control.runs && control.flat);
}

#[tokio::test]
async fn giving_up_on_reverts_leaves_loudness_on_and_the_next_revert_sends_it() {
    let (mut control, _chain, params) = eq_and_loudness_at(50.0).await;
    let identity = stage_ports(0.0, 48_000);
    let ports: Vec<(&str, f64)> = identity.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let mut gave_up = false;
    for _ in 0..REVERT_LIMIT + 1 {
        params.set_ports(&ports);
        ticks(&mut control, WATCH_EVERY).await;
        gave_up |= control.reverts.is_empty();
    }
    assert!(gave_up, "the reverts were given up on");
    assert!(control.view().loudness.unwrap().on, "loudness stays on");
    params.set_ports(&ports);
    let lines = params.lines().len();
    ticks(&mut control, WATCH_EVERY + 1).await;
    assert!(
        params.sent()[lines..]
            .iter()
            .any(|send| carries(send, target(50.0), 48_000)),
        "sent again"
    );
}

#[tokio::test]
async fn a_node_left_holding_a_stage_while_loudness_is_off_is_sent_the_identity() {
    let dir = Dir::new();
    saved(&dir.state(), vec![pk(100.0, 2.0, 1.0)], vec![1], true);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    let chain = FakeChain::with_state(state);
    let params = FakeParams::new();
    let left = stage_ports(12.0, 48_000);
    params.with_loudness_ports();
    let ports: Vec<(&str, f64)> = left.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    params.set_ports(&ports);
    let mut control = EqControl::new(
        Arc::new(chain),
        Box::new(params.clone()),
        Keep::File(dir.state()),
    );
    // The node plays the saved curve under its preamp: only its loudness stage is not the one
    // this controller vouches for.
    let mine: Vec<(String, f64)> = control
        .slot_values(48_000)
        .into_iter()
        .chain(crate::eq::live::preamp_params(control.preamp()))
        .collect();
    let ports: Vec<(&str, f64)> = mine.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    params.set_ports(&ports);
    control.sync_running().await;
    ticks(&mut control, 2).await;
    let first = params.sent().first().cloned().expect("a line");
    let high = |send: &[(String, f64)]| {
        ld_ports(send)
            .into_iter()
            .filter(|(name, _)| name.contains("ld2:"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        high(&first),
        high(&stage_ports(0.0, 48_000)),
        "the first line takes the stage off (the low section a flat cut for the shelf it held)"
    );
    ticks(&mut control, 2 * WATCH_EVERY).await;
    assert!(
        carries(params.sent().last().unwrap(), 0.0, 48_000),
        "then the identity"
    );
}

/// The view is the stage as played: depth, readout and curve.
#[tokio::test]
async fn the_view_reads_out_the_stage_as_played() {
    let (control, _chain, _params) = eq_and_loudness_at(50.0).await;
    let view = control.view().loudness.unwrap();
    assert_eq!(view.depth_db, target(50.0));
    assert_eq!(view.volume_pct, Some(50.0));
    assert!(view.following);
    assert!(view.mids_db < 0.0 && view.lift_db > 0.0);
    assert!(view.curve.db.iter().all(|db| *db <= 0.0));
}

/// A reference taken now is the newest reading: the hint of a write a moment ago.
#[tokio::test]
async fn reference_now_takes_the_newest_level() {
    let (mut control, _chain, _params) = eq_and_loudness_at(50.0).await;
    control.level_hint().set(level(30.0));
    control
        .loudness(LoudnessEdit {
            reference_now: true,
            ..ld(true)
        })
        .await
        .unwrap();
    assert_eq!(control.view().loudness.unwrap().reference_pct, 30.0);
}

/// A node may take a line in part, a section at a time: every part of every line plays the
/// stage alone at or under unity, deeper and shallower, on a snap to the identity (a graceful
/// stop, loudness off as the chain's last reason), for a new owner over a node left deep, and
/// over a rebuild no look has seen.
#[tokio::test]
async fn every_part_of_a_line_plays_the_stage_under_unity_both_ways() {
    let strong = |on: bool| LoudnessEdit {
        on: Some(on),
        strength_pct: Some(100.0),
        ..ld(true)
    };
    // Deeper, shallower, a rebuild while shallower, deeper again, then a graceful stop.
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    params.set_level(Some(level(100.0)));
    control.loudness(strong(true)).await.unwrap();
    ticks(&mut control, 3).await;
    params.set_level(Some(level(3.0)));
    ticks(&mut control, 140).await;
    assert!(depth(&control) > 35.0, "{}", depth(&control));
    params.set_level(Some(level(60.0)));
    ticks(&mut control, 30).await;
    while control.ticks % WATCH_EVERY >= WATCH_EVERY - 3 {
        control.tick().await;
    }
    let reset = params.sent().len();
    let identity = stage_ports(0.0, 48_000);
    let ports: Vec<(&str, f64)> = identity.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    params.set_ports(&ports);
    ticks(&mut control, 60).await;
    params.set_level(Some(level(3.0)));
    ticks(&mut control, 140).await;
    control.shutdown().await;
    assert!(carries(params.sent().last().unwrap(), 0.0, 48_000));
    parts_under_unity(&identity, &params.sent(), &[reset], 48_000);

    // Loudness off as the chain's last reason, a stream playing: the pass-through line snaps it.
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    params.set_level(Some(level(3.0)));
    control.loudness(strong(true)).await.unwrap();
    ticks(&mut control, 140).await;
    params.set_playing(true);
    control.loudness(strong(false)).await.unwrap();
    assert!(control.runs, "kept as a pass-through");
    // Flat at once (the low section a flat cut for the high shelf the node held), then the exact
    // identity with the repeat.
    let snap = params.sent().last().unwrap().clone();
    assert_eq!(
        ld_ports(&snap)
            .into_iter()
            .filter(|(name, _)| name.contains("ld2:"))
            .collect::<Vec<_>>(),
        stage_ports(0.0, 48_000)
            .into_iter()
            .filter(|(name, _)| name.contains("ld2:"))
            .collect::<Vec<_>>(),
        "the high section goes flat at once"
    );
    ticks(&mut control, 2 * WATCH_EVERY).await;
    assert!(carries(params.sent().last().unwrap(), 0.0, 48_000));
    parts_under_unity(&identity, &params.sent(), &[], 48_000);

    // A new owner over a node a crash left deep.
    let state = HostEqState {
        on: true,
        name: "test".into(),
        origin: None,
        target: String::new(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on: false,
        flat: true,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: Some(LoudnessSettings {
            on: true,
            strength_pct: 100.0,
            reference_pct: 100.0,
        }),
    };
    let params = FakeParams::new();
    params.with_loudness_ports();
    let left = stage_ports(40.0, 48_000);
    let ports: Vec<(&str, f64)> = left.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    params.set_ports(&ports);
    params.set_level(Some(level(3.0)));
    let mut control = EqControl::new(
        Arc::new(FakeChain::with_state(state)),
        Box::new(params.clone()),
        Keep::Memory,
    );
    control.sync_running().await;
    ticks(&mut control, 100).await;
    assert!(!params.sent().is_empty());
    parts_under_unity(&left, &params.sent(), &[], 48_000);
}

/// Test 13 upward: `pw-cli` lags through a glide toward a shallower depth, so the lines sent
/// after the look that found the node behind are cut for the deep shelf it held. Once the node
/// catches up and a look confirms the cut, the exact stage goes once: the chain does not stay
/// quieter than its depth says.
#[tokio::test]
async fn a_cut_left_by_a_lagging_glide_up_is_replaced_by_the_exact_stage() {
    let (mut control, _chain, params) = eq_and_loudness_at(25.0).await;
    params.holds(false);
    params.set_level(Some(level(100.0)));
    glide_to(&mut control, 0.0, 200).await;
    // The chase and its repeat go while the node is still behind: both cut for its deep shelf.
    let chased = control.chased;
    ticks(&mut control, 3 * WATCH_EVERY).await;
    assert!(control.chased > chased, "the chase went");
    let cut = params.sent().last().unwrap().clone();
    assert!(
        extra_cut_db(&cut, 0.0, 48_000) > 0.5,
        "cut for the deep shelf"
    );
    // The node catches up with them.
    let ports: Vec<(&str, f64)> = cut.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    params.set_ports(&ports);
    params.holds(true);
    ticks(&mut control, 4 * WATCH_EVERY).await;
    let last = params.sent().last().unwrap().clone();
    assert!(
        exactly(&last, 0.0, 48_000),
        "the node is left {:.3} dB under the stage",
        extra_cut_db(&last, 0.0, 48_000)
    );
    let lines = params.lines().len();
    ticks(&mut control, 4 * WATCH_EVERY).await;
    assert!(params.lines().len() - lines <= 1, "no loop");
}

/// A glide toward a shallower depth (the volume raised toward the reference) cuts each line's low
/// section only for the high shelves of the last lines, about a tenth of a dB, and ends exact.
#[tokio::test]
async fn a_glide_up_dips_by_a_tenth_of_a_db_at_most_and_ends_exact() {
    let (mut control, _chain, params) = eq_and_loudness_at(25.0).await;
    let from = params.sent().len();
    params.set_level(Some(level(100.0)));
    glide_to(&mut control, 0.0, 200).await;
    ticks(&mut control, 3 * WATCH_EVERY).await;
    let mut held = target(25.0);
    for send in &params.sent()[from..] {
        let sent: HashMap<String, f64> = ld_ports(send).into_iter().collect();
        // The depth of the line: the one whose high section it carries.
        let depth = (0..=400)
            .map(|tenth| f64::from(tenth) / 10.0)
            .find(|d| {
                let want: HashMap<String, f64> = stage_ports(*d, 48_000).into_iter().collect();
                want.iter()
                    .filter(|(name, _)| name.contains("ld2:"))
                    .all(|(name, value)| sent.get(name) == Some(value))
            })
            .expect("a stage's high section");
        let cut = extra_cut_db(send, depth, 48_000);
        assert!(
            cut <= 0.16,
            "{cut:.3} dB under the stage at {depth} dB (from {held})"
        );
        held = depth;
    }
    assert!(
        exactly(params.sent().last().unwrap(), 0.0, 48_000),
        "it ends exact"
    );
}

/// A glide that reverses inside one look window, the newest line lost with a dead `pw-cli`: the
/// node holds a high shelf deeper than the last one sent, and the next line, taken a section at a
/// time, still plays under unity.
#[tokio::test]
async fn a_lost_line_in_a_reversal_leaves_no_part_over_unity() {
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    params.set_level(Some(level(100.0)));
    control.loudness(ld(true)).await.unwrap();
    ticks(&mut control, 3 * WATCH_EVERY).await;
    while control.ticks % WATCH_EVERY > 8 {
        control.tick().await;
    }
    let hint = control.level_hint();
    // About 1 dB of depth: 0.5, then 1.0.
    let one_db = 100.0 * 10f64.powf(-2.0 / 60.0);
    params.set_level(Some(level(one_db)));
    hint.set(level(one_db));
    let from = params.sent().len();
    control.tick().await;
    control.tick().await;
    assert_eq!(depth(&control), 1.0);
    // Back to the reference: the step to 0.5 is taken and lost.
    params.set_level(Some(level(100.0)));
    hint.set(level(100.0));
    params.holds(false);
    control.tick().await;
    let lost = params.sent().len() - 1;
    params.holds(true);
    control.tick().await;
    assert!(
        params.sent().len() > lost + 1,
        "the line after the lost one"
    );
    parts_under_unity_lost(
        &stage_ports(0.0, 48_000),
        &params.sent()[from..],
        &[],
        &[lost - from],
        48_000,
    );
}

/// A new owner over a node a crash left at 40 dB of depth (strength 100 %, volume 3 %): the
/// controller, its fake and the stage the node holds.
fn over_a_node_left_deep() -> (EqControl, FakeParams, Vec<(String, f64)>) {
    let state = HostEqState {
        on: true,
        name: "test".into(),
        origin: None,
        target: String::new(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on: false,
        flat: true,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: Some(LoudnessSettings {
            on: true,
            strength_pct: 100.0,
            reference_pct: 100.0,
        }),
    };
    let params = FakeParams::new();
    params.with_loudness_ports();
    let left = stage_ports(40.0, 48_000);
    let ports: Vec<(&str, f64)> = left.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    params.set_ports(&ports);
    params.set_level(Some(level(3.0)));
    let control = EqControl::new(
        Arc::new(FakeChain::with_state(state)),
        Box::new(params.clone()),
        Keep::Memory,
    );
    (control, params, left)
}

/// A new owner's first line over a node left deep fails (no look comes before the next line), or
/// is written and lost: the next line, taken a section at a time, is still cut for the deep shelf
/// the node holds.
#[tokio::test]
async fn a_failed_first_line_over_a_node_left_deep_plays_no_part_over_unity() {
    let (mut control, params, left) = over_a_node_left_deep();
    control.sync_running().await;
    params.takes(false);
    for _ in 0..60 {
        control.tick().await;
        if !control.ld.failed.is_empty() {
            break;
        }
    }
    assert!(!control.ld.failed.is_empty(), "the first line failed");
    assert!(
        (control.ticks + 1) % WATCH_EVERY != 0 && !control.watch_due,
        "no look before the next line"
    );
    params.takes(true);
    control.tick().await;
    assert!(!params.sent().is_empty());
    parts_under_unity(&left, &params.sent(), &[], 48_000);

    // Written and lost.
    let (mut control, params, left) = over_a_node_left_deep();
    control.sync_running().await;
    params.holds(false);
    control.tick().await;
    assert_eq!(params.sent().len(), 1, "the first line, lost");
    params.holds(true);
    ticks(&mut control, 3).await;
    parts_under_unity_lost(&left, &params.sent(), &[], &[0], 48_000);
}

/// A line reported failed may land yet: a deeper line that landed though its send failed, then a
/// line onto the identity taken a section at a time, is still cut for the deeper shelf.
#[tokio::test]
async fn a_line_that_landed_though_its_send_failed_is_counted_by_the_next_cut() {
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    params.set_level(Some(level(100.0)));
    control.loudness(ld(true)).await.unwrap();
    ticks(&mut control, 3 * WATCH_EVERY).await;
    while control.ticks % WATCH_EVERY > 8 {
        control.tick().await;
    }
    let hint = control.level_hint();
    let depth_of = |db: f64| 100.0 * 10f64.powf(-db / 30.0);
    let from = params.sent().len();
    hint.set(level(depth_of(0.5)));
    params.set_level(Some(level(depth_of(0.5))));
    control.tick().await;
    assert_eq!(depth(&control), 0.5);
    // A step deeper lands, and its send reports failure.
    hint.set(level(depth_of(1.0)));
    params.set_level(Some(level(depth_of(1.0))));
    params.lands_but_fails(true);
    control.tick().await;
    params.lands_but_fails(false);
    assert_eq!(depth(&control), 0.5, "taken as not sent");
    // Back to the reference: the next line goes to the identity.
    hint.set(level(100.0));
    params.set_level(Some(level(100.0)));
    control.tick().await;
    assert_eq!(depth(&control), 0.0);
    parts_under_unity(
        &stage_ports(0.0, 48_000),
        &params.sent()[from..],
        &[],
        48_000,
    );
}

#[tokio::test]
async fn a_node_a_line_behind_still_takes_live_changes_while_loudness_glides() {
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    // The node already holds the preamp the chain sends, so the glide's own port is the one the
    // read-back checks.
    params.set_ports(&[("lpre:Mult", 1.0), ("rpre:Mult", 1.0)]);
    params.lands_one_line_late(true);
    params.set_level(Some(level(35.0)));
    control.loudness(ld(true)).await.unwrap();
    ticks(&mut control, 10).await;
    assert!(
        control.live,
        "a node a line behind proves live changes land"
    );
    let view = control.view().loudness.unwrap();
    assert!(view.following, "the stage still follows the volume");
    assert!(
        view.depth_db > 1.5,
        "the depth moves past 1.5 dB: {}",
        view.depth_db
    );
}

#[tokio::test]
async fn a_node_that_never_applies_a_live_line_is_still_given_up() {
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.with_loudness_ports();
    params.set_ports(&[("lpre:Mult", 1.0), ("rpre:Mult", 1.0)]);
    params.holds(false);
    params.set_level(Some(level(35.0)));
    control.loudness(ld(true)).await.unwrap();
    ticks(&mut control, 10).await;
    assert!(
        !control.live,
        "values that never show on the node give live up"
    );
    assert!(!control.view().loudness.unwrap().following);
}
