// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A recall that needs a higher preamp than the running chain allows (the cap of the curve it
//! was started with), made while a stream plays: it goes live with the preamp held at the cap,
//! so the sink stays and the player plays on, and the chain is applied again with the curve's
//! own preamp once nothing has played through it for the idle time.

use super::*;
use crate::eq::live::OutputSink;

fn applies(chain: &FakeChain) -> Vec<ApplyRequest> {
    chain
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            ChainCall::Apply(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn removes(chain: &FakeChain) -> usize {
    chain
        .calls()
        .iter()
        .filter(|call| **call == ChainCall::Remove)
        .count()
}

async fn ticks(control: &mut EqControl, count: u64) {
    for _ in 0..count {
        control.tick().await;
    }
}

/// A boost that needs about -10 dB of preamp.
fn hot() -> Correction {
    correction(vec![pk(100.0, 10.0, 1.0)], 0.0)
}

/// A boost that needs about -2 dB.
fn gentle() -> Correction {
    correction(vec![pk(100.0, 2.0, 1.0)], 0.0)
}

/// A curve that needs none.
fn flat() -> Correction {
    correction(vec![pk(1000.0, 0.0, 1.0)], 0.0)
}

async fn recall(control: &mut EqControl, name: &str, curve: Correction) {
    control.load(name.into(), None, curve).await.unwrap();
}

/// The chain applied with the hot curve and a stream playing through it; the preamp it plays.
async fn hot_playing(keep: Keep) -> (EqControl, FakeChain, FakeParams, f64) {
    let (mut control, chain, params) = setup(keep);
    chain.restarts_node_of(&params);
    params.set_output(Some(OutputSink {
        name: "a".into(),
        description: None,
        rate_hz: Some(48_000),
    }));
    recall(&mut control, "Hot", hot()).await;
    params.set_playing(true);
    let cap = control.view().preamp_db;
    assert!(cap < -9.0, "{cap}");
    (control, chain, params, cap)
}

/// The preamp every send from `from` on carries is at or under `cap`.
fn sends_under(params: &FakeParams, from: usize, cap: f64) {
    for send in &params.sent()[from..] {
        let db = preamp_of(send);
        assert!(db <= cap + 1e-6, "{db} dB sent over the cap of {cap} dB");
    }
}

/// Silence until the chain is applied again: not before the idle time, and once after it.
async fn silence_lifts(control: &mut EqControl, chain: &FakeChain, params: &FakeParams) {
    let before = applies(chain).len();
    params.set_playing(false);
    ticks(control, IDLE_KEEP - 2 * WATCH_EVERY).await;
    assert_eq!(applies(chain).len(), before, "not before the idle time");
    ticks(control, 3 * WATCH_EVERY).await;
    assert_eq!(applies(chain).len(), before + 1, "applied again once idle");
    ticks(control, IDLE_KEEP + WATCH_EVERY).await;
    assert_eq!(applies(chain).len(), before + 1, "and only once");
}

#[tokio::test]
async fn a_recall_that_needs_more_preamp_while_a_stream_plays_goes_live_held_at_the_cap() {
    let (mut control, chain, params, cap) = hot_playing(Keep::Memory).await;
    let sends = params.sent().len();
    recall(&mut control, "Gentle", gentle()).await;
    assert_eq!(applies(&chain).len(), 1, "no restart under the stream");
    assert_eq!(removes(&chain), 0);
    assert!(params.sent().len() > sends, "the curve went live");
    let last = params.sent().last().unwrap().clone();
    assert!(
        (preamp_of(&last) - cap).abs() < 1e-6,
        "{} dB sent, the cap is {cap} dB",
        preamp_of(&last)
    );
    let want = slot_params(1, Some(&pk(100.0, 2.0, 1.0)), 48_000);
    assert_eq!(value(&last, &want[0].0), Some(want[0].1), "slot 1 plays it");
    let view = control.view();
    assert_eq!(view.preset.as_deref(), Some("Gentle"));
    assert!((view.preamp_db - cap).abs() < 1e-6, "{}", view.preamp_db);
    assert_eq!(view.note.as_deref(), Some(LEVEL_HELD_NOTE));
    assert!(view.on && view.live);

    // Playing on: held, never applied.
    ticks(&mut control, IDLE_KEEP + 100).await;
    assert_eq!(applies(&chain).len(), 1);
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    sends_under(&params, sends, cap);

    // The music pauses: once idle, the chain is applied again with the curve's own preamp.
    silence_lifts(&mut control, &chain, &params).await;
    let request = applies(&chain).pop().unwrap();
    assert_eq!(request.name, "Gentle");
    assert_eq!(request.correction.filters, gentle().filters);
    let view = control.view();
    assert!(view.preamp_db > -3.0, "the curve's own: {}", view.preamp_db);
    assert_eq!(view.note, None);
    assert!(view.on);
}

#[tokio::test]
async fn a_recall_that_needs_a_lower_preamp_while_held_stays_live_and_lifts_nothing() {
    let (mut control, chain, params, cap) = hot_playing(Keep::Memory).await;
    recall(&mut control, "Gentle", gentle()).await;
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    recall(&mut control, "Hot", hot()).await;
    assert_eq!(applies(&chain).len(), 1);
    let view = control.view();
    assert_eq!(view.note, None, "the hot curve plays at its own preamp");
    assert!((view.preamp_db - cap).abs() < 1e-6);
    params.set_playing(false);
    ticks(&mut control, IDLE_KEEP * 2).await;
    assert_eq!(applies(&chain).len(), 1, "nothing to lift");
}

#[tokio::test]
async fn repeated_recalls_while_held_stay_live_and_the_restart_plays_the_last() {
    let (mut control, chain, params, cap) = hot_playing(Keep::Memory).await;
    let sends = params.sent().len();
    for (name, curve) in [
        ("Gentle", gentle()),
        ("Flat", flat()),
        ("Gentle", gentle()),
        ("Flat", flat()),
    ] {
        recall(&mut control, name, curve).await;
        ticks(&mut control, 30).await;
        assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    }
    assert_eq!(applies(&chain).len(), 1);
    assert_eq!(removes(&chain), 0);
    sends_under(&params, sends, cap);
    silence_lifts(&mut control, &chain, &params).await;
    assert_eq!(applies(&chain).pop().unwrap().name, "Flat");
    assert!(control.view().preamp_db.abs() < 1e-6);
}

#[tokio::test]
async fn with_nothing_playing_a_recall_that_needs_more_preamp_applies_at_once() {
    let (mut control, chain, params, _) = hot_playing(Keep::Memory).await;
    params.set_playing(false);
    recall(&mut control, "Gentle", gentle()).await;
    assert_eq!(applies(&chain).len(), 2);
    let view = control.view();
    assert!(view.preamp_db > -3.0, "{}", view.preamp_db);
    assert_eq!(view.note, None);
}

#[tokio::test]
async fn eq_off_while_held_keeps_a_pass_through_and_on_again_is_still_held() {
    let (mut control, chain, params, cap) = hot_playing(Keep::Memory).await;
    recall(&mut control, "Gentle", gentle()).await;
    let sends = params.sent().len();
    control.set_on(false).await.unwrap();
    assert_eq!(removes(&chain), 0, "the sink stays under the stream");
    assert_eq!(control.view().note.as_deref(), Some(PASSTHROUGH_NOTE));
    control.set_on(true).await.unwrap();
    assert_eq!(applies(&chain).len(), 1);
    let view = control.view();
    assert!(view.on);
    assert_eq!(view.note.as_deref(), Some(LEVEL_HELD_NOTE));
    sends_under(&params, sends, cap);
    silence_lifts(&mut control, &chain, &params).await;
    assert!(control.view().preamp_db > -3.0);
}

#[tokio::test]
async fn eq_off_while_held_with_nothing_playing_removes_the_chain_and_on_plays_the_curve_fully() {
    let (mut control, chain, params, _) = hot_playing(Keep::Memory).await;
    recall(&mut control, "Gentle", gentle()).await;
    params.set_playing(false);
    control.set_on(false).await.unwrap();
    assert_eq!(removes(&chain), 1);
    assert_eq!(control.view().note, None);
    control.set_on(true).await.unwrap();
    let request = applies(&chain).pop().unwrap();
    assert_eq!(request.name, "Gentle");
    let view = control.view();
    assert!(view.preamp_db > -3.0, "{}", view.preamp_db);
    assert_eq!(view.note, None);
    ticks(&mut control, IDLE_KEEP * 2).await;
    assert_eq!(applies(&chain).len(), 2, "nothing left to lift");
}

#[tokio::test]
async fn crossfeed_while_held_plays_under_the_cap_and_stays_on_through_the_restart() {
    let (mut control, chain, params, cap) = hot_playing(Keep::Memory).await;
    recall(&mut control, "Gentle", gentle()).await;
    let sends = params.sent().len();
    control
        .crossfeed(CrossfeedEdit {
            on: Some(true),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    ticks(&mut control, 30).await;
    assert!(control.view().crossfeed.unwrap().on);
    assert_eq!(applies(&chain).len(), 1);
    sends_under(&params, sends, cap);
    silence_lifts(&mut control, &chain, &params).await;
    ticks(&mut control, 30).await;
    let view = control.view();
    assert!(
        view.crossfeed.as_ref().unwrap().on,
        "crossfeed stays on: {:?}",
        view.crossfeed
    );
    let want = control.wanted_preamp(48_000) - control.stage_gain();
    assert!(
        (view.preamp_db - want).abs() < 1e-6,
        "{} dB, the curve and the stage want {want} dB",
        view.preamp_db
    );
    assert_eq!(view.note, None);
}

#[tokio::test]
async fn a_rate_change_while_held_never_plays_above_unity() {
    let (mut control, _, params, _) = hot_playing(Keep::Memory).await;
    let from = params.sent().len();
    recall(&mut control, "Gentle", gentle()).await;
    params.set_rate(Some(44_100));
    ticks(&mut control, 3 * WATCH_EVERY).await;
    let sends = params.sent();
    assert!(sends.len() > from + 1, "the new rate was sent");
    // The rate change put the slots back to the hot curve: each send, on top of that, plays at or
    // under unity.
    let conf = [(1, pk(100.0, 10.0, 1.0))];
    for (at, send) in sends.iter().enumerate().skip(from) {
        let over = over_unity(&conf, 48_000, std::slice::from_ref(send), 44_100);
        assert!(over <= 0.01, "after send {at}: {over} dB over unity");
    }
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
}

#[tokio::test]
async fn a_daemon_stopped_while_held_keeps_the_curve_and_the_next_start_plays_it_fully() {
    let dir = Dir::new();
    let (mut control, chain, params, cap) = hot_playing(Keep::File(dir.state())).await;
    recall(&mut control, "Gentle", gentle()).await;
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(state.name, "Gentle");
    assert_eq!(state.filters, gentle().filters);
    assert!(
        state.preamp_db > -3.0,
        "the state keeps the curve's own preamp: {}",
        state.preamp_db
    );
    control.shutdown().await;
    drop(control);

    // The chain played on (a manager that outlives the daemon): the next start finds it holding
    // the held preamp, plays under it while the stream plays, and lifts it once idle.
    let mut control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params.clone()),
        Keep::File(dir.state()),
    );
    let sends = params.sent().len();
    ticks(&mut control, 2 * WATCH_EVERY).await;
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    sends_under(&params, sends, cap);
    let before = applies(&chain).len();
    silence_lifts(&mut control, &chain, &params).await;
    assert_eq!(applies(&chain).len(), before + 1);
    let view = control.view();
    assert!(view.preamp_db > -3.0, "{}", view.preamp_db);
    assert_eq!(view.note, None);
}

/// A stage turned on while the level is held may cover the gap (the preamp it takes is deeper than
/// the cap): the level is not held then, but the lift is kept, so once the stage is off again the
/// level is held and said, and lifts once idle.
#[tokio::test]
async fn a_stage_that_covers_the_gap_for_a_while_keeps_the_lift() {
    let (mut control, chain, params) = setup(Keep::Memory);
    chain.restarts_node_of(&params);
    params.set_output(Some(OutputSink {
        name: "a".into(),
        description: None,
        rate_hz: Some(48_000),
    }));
    recall(&mut control, "Gentle", gentle()).await;
    params.set_playing(true);
    let cap = control.view().preamp_db;
    assert!((cap + 2.0).abs() < 1e-9, "{cap}");
    recall(
        &mut control,
        "Softer",
        correction(vec![pk(100.0, 1.9, 1.0)], 0.0),
    )
    .await;
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    let side = |on: bool| CrossfeedEdit {
        on: Some(on),
        model: Some("mid-side".to_string()),
        done: true,
        ..CrossfeedEdit::default()
    };
    control.crossfeed(side(true)).await.unwrap();
    ticks(&mut control, 30).await;
    let view = control.view();
    assert!(
        view.preamp_db < cap - 1.0,
        "the stage's own: {}",
        view.preamp_db
    );
    assert_eq!(view.note, None, "the stage covers the gap");
    control.crossfeed(side(false)).await.unwrap();
    ticks(&mut control, 30).await;
    let view = control.view();
    assert!((view.preamp_db - cap).abs() < 1e-9, "{}", view.preamp_db);
    assert_eq!(view.note.as_deref(), Some(LEVEL_HELD_NOTE), "held again");
    assert_eq!(applies(&chain).len(), 1);
    silence_lifts(&mut control, &chain, &params).await;
    let view = control.view();
    assert!((view.preamp_db + 1.9).abs() < 1e-9, "{}", view.preamp_db);
    assert_eq!(view.note, None);
}

/// The EQ turned off while a stream plays, where the pass-through's line fails and so does the
/// remove that follows: the EQ stays on with its points, and the next line carries them, since
/// the session repairs a failed line with the next one and would otherwise play the flat line the
/// controller took back (seed 4421 of the preamp lab).
#[tokio::test]
async fn a_pass_through_line_that_failed_is_put_right_by_the_next_line() {
    let (mut control, chain, params) = setup(Keep::Memory);
    recall(&mut control, "Hot", hot()).await;
    params.set_playing(true);
    params.takes(false);
    chain.refuse(Some(EqFailure {
        what: "the remove failed".into(),
        next: "Try again".into(),
    }));
    assert!(control.set_on(false).await.is_err());
    assert!(control.view().on, "the EQ stays on");
    chain.refuse(None);
    params.takes(true);
    let sends = params.sent().len();
    ticks(&mut control, 5).await;
    let sent = &params.sent()[sends..];
    let want = slot_params(1, Some(&pk(100.0, 10.0, 1.0)), 48_000);
    assert!(
        sent.last()
            .is_some_and(|last| value(last, &want[0].0) == Some(want[0].1)),
        "the points go with the next line: {sent:?}"
    );
}

/// The chain applied with `curve` while a stream is on the output, as a real chain comes up: its
/// preamp ports hold the config's preamp as `pw-dump` prints it, and its graph has not run.
async fn applied(curve: Correction) -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = setup(Keep::Memory);
    chain.restarts_node_like_pipewire(&params);
    params.set_output(Some(OutputSink {
        name: "a".into(),
        description: None,
        rate_hz: Some(48_000),
    }));
    recall(&mut control, "First", curve).await;
    params.set_playing(true);
    (control, chain, params)
}

/// [`applied`], and the graph runs with no line sent to it before its first look at it run;
/// `seen_unrun` says whether a look came before it ran.
async fn applied_then_run(
    curve: Correction,
    seen_unrun: bool,
) -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = applied(curve).await;
    if seen_unrun {
        ticks(&mut control, 2 * WATCH_EVERY).await;
    }
    params.run_graph();
    ticks(&mut control, 2 * WATCH_EVERY).await;
    (control, chain, params)
}

/// A chain applied with a boost of `boost` dB at 100 Hz and read back as `pw-dump` prints it:
/// the curve is never held by its own preamp, with or without a look at the graph before it ran.
async fn own_read_back_bounds_nothing(boost: f64, db: f64) {
    for seen_unrun in [true, false] {
        let own = correction(vec![pk(100.0, boost, 1.0)], 0.0);
        let (mut control, chain, params) = applied_then_run(own.clone(), seen_unrun).await;
        assert!((control.view().preamp_db - db).abs() < 0.11, "{db}");
        recall(&mut control, "Flat", flat()).await;
        let held = control.view().note;
        assert_eq!(
            held.as_deref(),
            Some(LEVEL_HELD_NOTE),
            "{db}: flat needs more"
        );
        recall(&mut control, "Again", own).await;
        assert_eq!(
            control.view().note,
            None,
            "{db} seen_unrun={seen_unrun}: not held"
        );
        ticks(&mut control, IDLE_KEEP * 2).await;
        assert_eq!(applies(&chain).len(), 1, "{db}: no apply while it plays");
        params.set_playing(false);
        ticks(&mut control, IDLE_KEEP * 2).await;
        assert_eq!(applies(&chain).len(), 1, "{db}: nor once the music pauses");
    }
}

#[tokio::test]
async fn an_applied_minus_6_db_curve_is_not_held_by_its_own_read_back_preamp() {
    own_read_back_bounds_nothing(6.0, -6.0).await;
}

#[tokio::test]
async fn an_applied_minus_2_db_curve_is_not_held_by_its_own_read_back_preamp() {
    own_read_back_bounds_nothing(2.0, -2.0).await;
}

#[tokio::test]
async fn an_applied_minus_10_db_curve_which_reads_back_rounded_up_is_not_held_either() {
    own_read_back_bounds_nothing(10.0, -10.0).await;
}

#[tokio::test]
async fn with_nothing_playing_the_applied_curve_goes_live_not_applied() {
    let own = correction(vec![pk(100.0, 6.0, 1.0)], 0.0);
    let (mut control, chain, params) = applied_then_run(own.clone(), true).await;
    params.set_playing(false);
    recall(&mut control, "Again", own).await;
    assert_eq!(applies(&chain).len(), 1, "live edit, no restart");
}

#[tokio::test]
async fn an_own_graph_that_reads_back_a_lower_preamp_than_its_config_still_bounds() {
    let (mut control, chain, params) = applied(correction(vec![pk(100.0, 6.0, 1.0)], 0.0)).await;
    // The node holds -10 dB, under the config's -6 dB: something else set it.
    params.start_from_config(-10.0);
    params.run_graph();
    ticks(&mut control, 2 * WATCH_EVERY).await;
    recall(
        &mut control,
        "Again",
        correction(vec![pk(100.0, 6.0, 1.0)], 0.0),
    )
    .await;
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    assert_eq!(applies(&chain).len(), 1);
}

#[tokio::test]
async fn a_graph_this_controller_did_not_apply_bounds_even_at_the_configs_own_preamp() {
    let curve = || correction(vec![pk(100.0, 6.0, 1.0)], 0.0);
    let (mut control, chain, params) = applied_then_run(curve(), true).await;
    // Another chain comes up under the output (a new serial), holding exactly -6 dB.
    params.restart_node();
    params.start_from_config(-6.0);
    params.set_playing(true);
    params.run_graph();
    ticks(&mut control, 2 * WATCH_EVERY).await;
    recall(&mut control, "Again", curve()).await;
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    assert_eq!(applies(&chain).len(), 1);
}
