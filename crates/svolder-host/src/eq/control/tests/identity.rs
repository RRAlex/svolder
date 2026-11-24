// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Option D: every chain is applied from an IDENTITY config (every slot flat, no stage, the
//! preamp the first live line carries) and the curve and the stages are sent live as soon as the
//! node's graph runs, so a rebuild of the graph puts back a flat chain under the last preamp sent.
//! A PipeWire that takes no live changes keeps the curve and the stage in the config.

use super::vs::put_set;
use super::*;
use crate::eq::live::OutputSink;
use svolder_speakers::Store;

fn sink() -> OutputSink {
    OutputSink {
        name: "a".to_string(),
        description: None,
        rate_hz: Some(48_000),
    }
}

fn applied(chain: &FakeChain) -> Vec<ApplyRequest> {
    chain
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            ChainCall::Apply(request) => Some(request),
            _ => None,
        })
        .collect()
}

async fn ticks(control: &mut EqControl, count: u64) {
    for _ in 0..count {
        control.tick().await;
    }
}

fn db(send: &[(String, f64)]) -> Option<f64> {
    value(send, "lpre:Mult").map(|mult| 20.0 * mult.log10())
}

fn xf_on() -> CrossfeedEdit {
    CrossfeedEdit {
        on: Some(true),
        model: Some("mid-side".into()),
        done: true,
        ..CrossfeedEdit::default()
    }
}

#[tokio::test]
async fn the_config_written_is_the_identity_with_the_first_lines_preamp() {
    // The EQ alone.
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    control.edit(gain(0, 6.0, true)).await.unwrap();
    let request = applied(&chain).remove(0);
    let want = control.view().preamp_db;
    assert!(want < -5.0, "{want}");
    assert_eq!(request.identity, Some(want), "the cut the curve needs");
    assert_eq!(request.crossfeed, None);
    assert_eq!(
        request.correction.filters.len(),
        1,
        "the state keeps the curve"
    );

    // The EQ and crossfeed: the cut includes the stage's, which goes live too.
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    control.edit(gain(0, 6.0, true)).await.unwrap();
    control.crossfeed(xf_on()).await.unwrap();
    let both = applied(&chain);
    let last = both.last().unwrap();
    assert!(last.crossfeed.is_none() && last.identity.is_some());

    // Crossfeed alone (the EQ plays flat), the speakers alone, loudness alone: identity as well.
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    control.crossfeed(xf_on()).await.unwrap();
    let request = applied(&chain).remove(0);
    let cost = control.stage_gain();
    assert!(cost > 3.0);
    assert_eq!(
        request.identity,
        Some(-cost),
        "the stage's cost, as its first line has it"
    );

    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    let (control, chain, params) = setup(Keep::Memory);
    chain.restarts_node_like_pipewire(&params);
    params.set_output(Some(sink()));
    params.with_speakers_ports(false);
    let mut control = control.with_speakers(store);
    control
        .speakers(SpeakersEdit {
            on: Some(true),
            set: Some("set-a".into()),
            done: true,
        })
        .await
        .unwrap();
    let request = applied(&chain).remove(0);
    // The config plays the dry sound until the first line turns the mixers wet: its preamp is the
    // first line's less the set's level, so it is no louder than that line.
    let level = control.view().speakers.unwrap().loudness_db;
    assert!(level < -5.0, "{level}");
    assert_eq!(request.identity, Some(level));
    assert!(request.speakers.is_some_and(|speakers| !speakers.on));
    // The first line, once the graph runs, gives the level back as the mixers turn wet: no climb.
    params.set_playing(true);
    ticks(&mut control, 20).await;
    params.run_graph();
    ticks(&mut control, 20).await;
    let levels = control.view().levels.unwrap();
    assert_eq!(levels.climb_to_db, None);
    assert!((levels.total_db - level).abs() < 1e-6, "{levels:?}");

    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    params.with_loudness_ports();
    control
        .loudness(LoudnessEdit {
            on: Some(true),
            done: true,
            ..LoudnessEdit::default()
        })
        .await
        .unwrap();
    assert_eq!(applied(&chain).remove(0).identity, Some(0.0));
}

/// An identity apply with the speakers on, then the speakers turned off before the node is
/// listed: the first line, sent from a tick, plays the dry sound, so the preamp the config holds
/// (cut by the set's level) is not given back at once; it climbs from what plays.
#[tokio::test]
async fn a_first_line_that_cannot_give_the_sets_level_back_climbs_from_what_plays() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    let (control, chain, params) = setup(Keep::Memory);
    chain.restarts_node_like_pipewire(&params);
    params.set_output(Some(sink()));
    params.with_speakers_ports(false);
    let mut control = control.with_speakers(store);
    control
        .speakers(SpeakersEdit {
            on: Some(true),
            set: Some("set-a".into()),
            done: true,
        })
        .await
        .unwrap();
    let level = control.view().speakers.unwrap().loudness_db;
    assert!(level < -5.0, "{level}");
    assert_eq!(applied(&chain).remove(0).identity, Some(level));
    // Off while the graph lists no convolver: nothing to send, the config is all that is left to
    // write; the first line then goes from a tick, once the graph runs.
    params.without_speakers_ports();
    control
        .speakers(SpeakersEdit {
            on: Some(false),
            set: None,
            done: true,
        })
        .await
        .unwrap();
    params.set_playing(true);
    ticks(&mut control, 20).await;
    params.run_graph();
    ticks(&mut control, 20).await;
    let sent = params.sent();
    let first = sent
        .iter()
        .find_map(|line| db(line))
        .expect("a first line reached the node");
    assert!(
        first <= level + 0.4,
        "the first line plays {first:+.2} dB over a node that plays {level:+.2} dB"
    );
    ticks(&mut control, 60).await;
    let levels = control.view().levels.unwrap();
    assert_eq!(levels.climb_to_db, None);
    assert!(levels.total_db.abs() < 1e-6, "{levels:?}");
}

#[tokio::test]
async fn a_pipewire_that_takes_no_live_changes_keeps_the_curve_in_the_config() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    control.live = false;
    control.edit(gain(0, 6.0, true)).await.unwrap();
    assert_eq!(applied(&chain).remove(0).identity, None);
}

#[tokio::test]
async fn the_first_line_follows_the_node_and_carries_the_curve_under_the_configs_preamp() {
    let (mut control, chain, params) = setup(Keep::Memory);
    chain.restarts_node_like_pipewire(&params);
    params.set_output(Some(sink()));
    control.edit(gain(0, 6.0, true)).await.unwrap();
    let cut = applied(&chain)[0].identity.unwrap();
    // The graph has not run: what is sent to it is lost with it (the edit's own line, its repeat).
    ticks(&mut control, 60).await;
    let lost = params.sent().len();
    // It runs, flat under the config's preamp: the curve arrives under that very preamp.
    params.run_graph();
    ticks(&mut control, 40).await;
    let first = params
        .sent()
        .into_iter()
        .nth(lost)
        .expect("the curve went live");
    assert!(
        (db(&first).unwrap() - cut).abs() < 1e-4,
        "{:?} vs {cut}",
        db(&first)
    );
    let want = slot_params(1, Some(&pk(1000.0, 6.0, 1.0)), 48_000);
    assert_eq!(
        value(&first, &want[0].0),
        Some(want[0].1),
        "slot 1 plays the curve"
    );
    assert_eq!(
        control.view().levels.unwrap().climb_to_db,
        None,
        "an edit is no climb"
    );
}

#[tokio::test]
async fn a_rebuild_puts_back_the_flat_chain_and_the_curve_is_sent_again() {
    let (mut control, chain, params) = setup(Keep::Memory);
    chain.restarts_node_like_pipewire(&params);
    params.set_output(Some(sink()));
    control.edit(gain(0, 6.0, true)).await.unwrap();
    params.run_graph();
    ticks(&mut control, 60).await;
    let sends = params.sent().len();
    // The sink changes rate: the graph is rebuilt, every slot back to the config's identity.
    params.set_rate(Some(44_100));
    let flat: Vec<(String, f64)> = (1..=SLOTS as u8)
        .flat_map(|slot| slot_params(slot, None, 44_100))
        .collect();
    let ports: Vec<(&str, f64)> = flat.iter().map(|(name, v)| (name.as_str(), *v)).collect();
    params.set_ports(&ports);
    ticks(&mut control, 60).await;
    let again = &params.sent()[sends..];
    assert!(!again.is_empty(), "every slot is sent again");
    let want = slot_params(1, Some(&pk(1000.0, 6.0, 1.0)), 44_100);
    assert_eq!(
        value(again.last().unwrap(), &want[0].0),
        Some(want[0].1),
        "at the new rate"
    );
}

#[tokio::test]
async fn live_changes_that_do_not_take_apply_the_chain_again_with_the_curve_in_its_config() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.holds(false);
    control.edit(gain(0, 6.0, true)).await.unwrap();
    assert_eq!(applied(&chain).len(), 1);
    assert!(applied(&chain)[0].identity.is_some());
    ticks(&mut control, 80).await;
    assert!(!control.view().live);
    let all = applied(&chain);
    assert_eq!(all.len(), 2, "applied again once live changes are given up");
    assert_eq!(all[1].identity, None, "with the curve in the config");
    assert_eq!(control.view().note.as_deref(), Some(LIVE_NOTE));
}

#[tokio::test]
async fn a_node_that_started_again_before_its_first_look_has_the_curve_the_config_on_disk_holds() {
    let (mut control, chain, params) = setup(Keep::Memory);
    chain.restarts_node_like_pipewire(&params);
    params.set_output(Some(sink()));
    // The apply, and no look at its node yet (a drag's value waits for a tick).
    control.edit(gain(0, 6.0, false)).await.unwrap();
    let cut = applied(&chain)[0].identity.unwrap();
    // A restart nobody saw: the node holds the config on disk, the curve under its cut.
    params.run_graph();
    let curve = slot_params(1, Some(&pk(1000.0, 6.0, 1.0)), 48_000);
    let ports: Vec<(&str, f64)> = curve.iter().map(|(name, v)| (name.as_str(), *v)).collect();
    params.set_ports(&ports);
    ticks(&mut control, 2).await;
    // The loaded curve is the kept one: no preamp above its cut is ever sent after it softens.
    let sends = params.sent().len();
    control.edit(gain(0, 0.0, true)).await.unwrap();
    ticks(&mut control, 20).await;
    let after = &params.sent()[sends..];
    assert!(!after.is_empty());
    for send in after {
        if let Some(sent) = db(send) {
            assert!(sent <= cut + 1e-4, "{sent} dB sent over the {cut} dB cut");
        }
    }
}

#[tokio::test]
async fn the_ports_of_an_untouched_chain_are_every_one_listed_at_its_identity() {
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    params.with_loudness_ports();
    control.crossfeed(xf_on()).await.unwrap();
    params.set_playing(true);
    control
        .crossfeed(CrossfeedEdit {
            on: Some(false),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    ticks(&mut control, 60).await;
    let mut ports: std::collections::HashMap<String, f64> =
        params.sent().last().unwrap().iter().cloned().collect();
    assert!(control.ports_are_identity(&ports));
    // One port off its identity, or one not listed, is not.
    let mut moved = ports.clone();
    moved.insert("l05:b0".to_string(), 0.97);
    assert!(!control.ports_are_identity(&moved));
    let name = "rpre:Mult".to_string();
    let held = ports.remove(&name);
    assert!(held.is_some() && !control.ports_are_identity(&ports));
}

#[tokio::test]
async fn a_pw_cli_that_keeps_failing_on_an_identity_chain_applies_the_curve_and_stays_unavailable()
{
    let (mut control, chain, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    assert!(applied(&chain)[0].identity.is_some());
    params.takes(false);
    control.edit(gain(0, 3.0, false)).await.unwrap();
    ticks(&mut control, FAIL_LIMIT as u64 + 1).await;
    let view = control.view();
    assert!(!view.live);
    let why = view.unavailable.expect("it still says so");
    assert!(why.starts_with("computer controls unavailable: "), "{why}");
    let all = applied(&chain);
    assert_eq!(all.len(), 2, "applied again with the curve");
    assert_eq!(all[1].identity, None);
    ticks(&mut control, 10).await;
    assert!(
        control.view().unavailable.is_some(),
        "and live drags are not tried again"
    );
}

/// A rate change on a pass-through whose config holds the curve (the command line's chain) reads
/// that chain's ceiling again: the pass-through holds the loaded curve's cut at the rate it runs
/// at, never above it (no climb runs there: the cut is the ceiling).
#[tokio::test]
async fn a_rate_change_rereads_a_curve_in_config_ceiling() {
    let (mut control, _chain, params) = setup_curve(Keep::Memory);
    params.set_output(Some(sink()));
    control.edit(gain(0, 6.0, true)).await.unwrap();
    params.set_playing(true);
    ticks(&mut control, 40).await;
    let cut_48 = control.conf_preamp(48_000);
    let cut_96 = control.conf_preamp(96_000);
    assert!(cut_48 < -5.0, "{cut_48}");
    let sends = params.sent().len();
    control.set_on(false).await.unwrap();
    // The pass-through holds the loaded curve's cut: that is the ceiling at 48 kHz.
    assert!(
        (control.view().preamp_db - cut_48).abs() < 1e-6,
        "{}",
        control.view().preamp_db
    );
    ticks(&mut control, 2).await;
    let at_48 = params.sent().len();
    params.set_rate(Some(96_000));
    ticks(&mut control, 100).await;
    assert!(
        (control.view().preamp_db - cut_96).abs() < 1e-6,
        "{} dB, the ceiling at 96 kHz is {cut_96} dB",
        control.view().preamp_db
    );
    // Each send is under the ceiling of the rate it went at: the lines before the rate changed
    // under the 48 kHz one, every line after it under the 96 kHz one.
    for (lines, ceiling, rate) in [
        (&params.sent()[sends..at_48], cut_48, 48),
        (&params.sent()[at_48..], cut_96, 96),
    ] {
        for send in lines {
            if let Some(sent) = db(send) {
                assert!(
                    sent <= ceiling + 1e-4,
                    "{sent} dB sent over the {ceiling} dB ceiling at {rate} kHz"
                );
            }
        }
    }
}
