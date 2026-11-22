// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's part of a scene: captured with the stated preamp, recalled as one change (one
//! apply at most, or one live line), refused whole before anything changes, and told apart from
//! a computer that plays something else.

use super::vs::put_set;
use super::*;
use crate::eq::ComputerPart;
use crate::eq::crossfeed::{CrossfeedSettings, Model};
use crate::eq::live::OutputSink;
use crate::eq::loudness::LoudnessSettings;
use svolder_speakers::Store;

fn sink(name: &str) -> OutputSink {
    OutputSink {
        name: name.to_string(),
        description: None,
        rate_hz: Some(48_000),
    }
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

async fn ticks(control: &mut EqControl, count: u64) {
    for _ in 0..count {
        control.tick().await;
    }
}

/// A part that plays `filters` at `preamp_db` with every stage off.
fn eq_part(filters: Vec<SourceFilter>, preamp_db: f64) -> ComputerPart {
    ComputerPart {
        eq_on: true,
        name: "Desk".to_string(),
        origin: None,
        preamp_db,
        filters,
        loudness: None,
        crossfeed_on: false,
        crossfeed: None,
        speakers_on: false,
        speakers: None,
    }
}

/// A part with nothing playing.
fn silent_part() -> ComputerPart {
    ComputerPart {
        eq_on: false,
        ..eq_part(vec![pk(100.0, 2.0, 1.0)], 0.0)
    }
}

fn loud(on: bool) -> Option<LoudnessSettings> {
    Some(LoudnessSettings {
        on,
        strength_pct: 70.0,
        reference_pct: 90.0,
    })
}

/// A node as a current chain shows it, with the output `a` seen.
fn rig() -> (EqControl, FakeChain, FakeParams) {
    let (control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    params.with_loudness_ports();
    (control, chain, params)
}

/// A controller with the speakers served: sets `set-a` and `set-b` fetched.
fn spk_rig() -> (EqControl, FakeChain, FakeParams, Dir) {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    put_set(&store, "set-b", "Dummy head, B");
    let (control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    params.with_speakers_ports(false);
    params.with_loudness_ports();
    (control.with_speakers(store), chain, params, dir)
}

#[tokio::test]
async fn a_recall_of_curve_crossfeed_and_loudness_from_off_with_no_chain_is_one_apply() {
    let (mut control, chain, _params) = rig();
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0)], -2.0);
    part.crossfeed_on = true;
    part.crossfeed = Some(CrossfeedSettings::default());
    part.loudness = loud(true);
    control.recall(part.clone()).await.unwrap();
    assert_eq!(applies(&chain), 1, "one apply for every stage");
    assert_eq!(removes(&chain), 0);
    let view = control.view();
    assert!(view.on);
    assert!(view.crossfeed.as_ref().unwrap().on);
    assert!(view.loudness.as_ref().unwrap().on);
    assert!(part.matches(&view));
    assert_eq!(control.view().loads, 1);
}

#[tokio::test]
async fn a_recall_over_a_running_chain_is_one_live_line_and_no_apply() {
    let (mut control, chain, params) = rig();
    control
        .load(
            "Old".into(),
            None,
            correction(vec![pk(500.0, -1.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    assert_eq!(applies(&chain), 1);
    let sends = params.sent().len();
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0), pk(4000.0, -3.0, 2.0)], -2.0);
    part.crossfeed_on = true;
    // Mid/Side bass at 50 % is the worst case: +3 dB, which the preamp pays before it plays.
    part.crossfeed = Some(CrossfeedSettings::for_model(Model::MidSide));
    part.loudness = loud(true);
    let recalled = control.recall(part.clone()).await.unwrap();
    assert_eq!(applies(&chain), 1, "no apply");
    assert_eq!(removes(&chain), 0);
    assert_eq!(params.sent().len(), sends + 1, "one line");
    assert!(recalled.notes.is_empty(), "{:?}", recalled.notes);
    let view = control.view();
    assert!(part.matches(&view));
    // The crossfeed's cost is in the preamp the line carries.
    assert!(preamp_of(params.sent().last().unwrap()) < -4.9);
    assert!(view.preamp_db < -4.9, "{}", view.preamp_db);
}

#[tokio::test]
async fn a_speakers_set_change_is_one_apply_and_the_same_set_is_none() {
    let (mut control, chain, _params, _dir) = spk_rig();
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0)], -2.0);
    part.speakers_on = true;
    part.speakers = Some("set-a".into());
    control.recall(part.clone()).await.unwrap();
    assert_eq!(applies(&chain), 1, "the chain is built with the set");
    assert!(control.view().speakers.unwrap().on);

    let mut next = part.clone();
    next.speakers = Some("set-b".into());
    let recalled = control.recall(next.clone()).await.unwrap();
    assert_eq!(applies(&chain), 2, "another set is one rebuild");
    assert_eq!(recalled.notes.len(), 1, "{:?}", recalled.notes);
    assert!(next.matches(&control.view()));
    assert!(!part.matches(&control.view()));

    control.recall(next).await.unwrap();
    assert_eq!(applies(&chain), 2, "the same set is live");
}

#[tokio::test]
async fn crossfeed_and_speakers_both_on_are_refused_before_anything_changes() {
    let (mut control, chain, params, _dir) = spk_rig();
    control
        .load(
            "Old".into(),
            None,
            correction(vec![pk(500.0, -1.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    let (view, sends, calls) = (control.view(), params.sent().len(), chain.calls().len());
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0)], -2.0);
    part.crossfeed_on = true;
    part.crossfeed = Some(CrossfeedSettings::default());
    part.speakers_on = true;
    part.speakers = Some("set-a".into());
    let failure = control.recall(part).await.unwrap_err();
    assert!(failure.what.contains("one choice"), "{}", failure.what);
    assert_eq!(control.view(), view);
    assert_eq!((params.sent().len(), chain.calls().len()), (sends, calls));
}

#[tokio::test]
async fn a_set_that_is_not_fetched_refuses_the_whole_part_and_changes_nothing() {
    let (mut control, chain, params, _dir) = spk_rig();
    control
        .load(
            "Old".into(),
            None,
            correction(vec![pk(500.0, -1.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    let (view, sends, calls) = (control.view(), params.sent().len(), chain.calls().len());
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0)], -2.0);
    part.loudness = loud(true);
    part.speakers_on = true;
    part.speakers = Some("not-fetched".into());
    let failure = control.recall(part).await.unwrap_err();
    assert!(failure.what.contains("not-fetched"), "{}", failure.what);
    assert_eq!(control.view(), view, "the curve and the loudness stayed");
    assert_eq!((params.sent().len(), chain.calls().len()), (sends, calls));
    assert_eq!(control.view().loads, view.loads, "a refusal is not a load");
}

#[tokio::test]
async fn a_curve_the_chain_cannot_play_refuses_the_part() {
    let (mut control, chain, _params) = rig();
    let part = eq_part(vec![pk(100.0, 2.0, 1.0); 17], 0.0);
    let failure = control.recall(part).await.unwrap_err();
    assert!(failure.what.contains("16"), "{}", failure.what);
    assert_eq!(applies(&chain), 0);
    assert!(!control.view().on);
}

/// A boost that needs about -10 dB of preamp.
fn hot() -> Correction {
    correction(vec![pk(100.0, 10.0, 1.0)], 0.0)
}

#[tokio::test]
async fn a_part_that_needs_more_preamp_while_a_stream_plays_goes_live_at_the_cap_and_is_lifted() {
    let (mut control, chain, params) = rig();
    chain.restarts_node_of(&params);
    control.load("Hot".into(), None, hot()).await.unwrap();
    params.set_playing(true);
    let cap = control.view().preamp_db;
    assert!(cap < -9.0, "{cap}");
    let sends = params.sent().len();
    let part = eq_part(vec![pk(100.0, 2.0, 1.0)], 0.0);
    let recalled = control.recall(part.clone()).await.unwrap();
    assert_eq!(applies(&chain), 1, "no restart under the stream");
    assert_eq!(recalled.notes, vec![LEVEL_HELD_NOTE.to_string()]);
    for send in &params.sent()[sends..] {
        assert!(preamp_of(send) <= cap + 1e-6, "over the cap");
    }
    let view = control.view();
    assert!((view.preamp_db - cap).abs() < 1e-6);
    assert_eq!(view.note.as_deref(), Some(LEVEL_HELD_NOTE));
    // The part is what it states, not the level it plays at.
    assert_eq!(control.computer_part().preamp_db, 0.0);
    assert!(part.matches(&view));

    params.set_playing(false);
    ticks(&mut control, IDLE_KEEP + 3 * WATCH_EVERY).await;
    assert_eq!(applies(&chain), 2, "applied again once idle");
    assert!(control.view().preamp_db > -3.0);
    assert!(part.matches(&control.view()));
}

#[tokio::test]
async fn computer_part_after_a_load_states_the_curves_own_preamp_and_matches_tells_a_moved_point() {
    let (mut control, _chain, _params) = rig();
    control
        .load(
            "Desk".into(),
            Some("a file".into()),
            correction(vec![pk(100.0, 2.0, 1.0)], -3.0),
        )
        .await
        .unwrap();
    let part = control.computer_part();
    assert!(part.eq_on);
    assert_eq!(part.name, "Desk");
    assert_eq!(part.origin.as_deref(), Some("a file"));
    assert_eq!(part.preamp_db, -3.0);
    assert_eq!(part.filters, vec![pk(100.0, 2.0, 1.0)]);
    assert!(!part.crossfeed_on && !part.speakers_on);
    assert!(part.matches(&control.view()));
    control.edit(gain(0, 3.0, true)).await.unwrap();
    assert!(!part.matches(&control.view()), "a point moved");
}

#[tokio::test]
async fn a_part_equal_in_points_with_another_stated_preamp_is_not_on() {
    let (mut control, _chain, _params) = rig();
    let curve = vec![pk(100.0, 2.0, 1.0)];
    control.recall(eq_part(curve.clone(), -3.0)).await.unwrap();
    let view = control.view();
    assert!(eq_part(curve.clone(), -3.0).matches(&view));
    assert!(!eq_part(curve, 0.0).matches(&view));
}

#[tokio::test]
async fn settings_kept_while_a_stage_is_off_do_not_decide_whether_a_part_is_on() {
    let (mut control, _chain, _params) = rig();
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0)], 0.0);
    part.loudness = loud(false);
    control.recall(part.clone()).await.unwrap();
    let mut other = part.clone();
    other.loudness = Some(LoudnessSettings {
        strength_pct: 10.0,
        ..part.loudness.unwrap()
    });
    assert!(other.matches(&control.view()), "loudness is off in both");
    other.loudness = loud(true);
    assert!(!other.matches(&control.view()));
}

#[tokio::test]
async fn a_part_with_everything_off_removes_the_chain_and_never_leaves_two() {
    let (mut control, chain, _params) = rig();
    let mut on = eq_part(vec![pk(100.0, 2.0, 1.0)], 0.0);
    on.crossfeed_on = true;
    on.crossfeed = Some(CrossfeedSettings::default());
    control.recall(on).await.unwrap();
    assert_eq!(applies(&chain), 1);
    let off = silent_part();
    control.recall(off.clone()).await.unwrap();
    assert_eq!(applies(&chain), 1, "nothing was applied to remove");
    assert_eq!(removes(&chain), 1, "the chain is gone");
    let view = control.view();
    assert!(!view.on);
    assert!(!view.crossfeed.as_ref().unwrap().on);
    assert!(off.matches(&view));
    // Off again with nothing running changes nothing on the chain.
    control.recall(off).await.unwrap();
    assert_eq!((applies(&chain), removes(&chain)), (1, 1));
}

#[tokio::test]
async fn a_part_that_turns_the_eq_off_keeps_the_chain_flat_for_a_stage() {
    let (mut control, chain, params) = rig();
    control
        .load(
            "Old".into(),
            None,
            correction(vec![pk(500.0, -1.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    let mut part = silent_part();
    part.crossfeed_on = true;
    part.crossfeed = Some(CrossfeedSettings::default());
    let sends = params.sent().len();
    control.recall(part.clone()).await.unwrap();
    assert_eq!(
        (applies(&chain), removes(&chain)),
        (1, 0),
        "one chain, live"
    );
    assert_eq!(params.sent().len(), sends + 1);
    let view = control.view();
    assert!(!view.on && view.crossfeed.as_ref().unwrap().on);
    assert!(part.matches(&view));
}

#[tokio::test]
async fn a_chain_that_refuses_the_apply_leaves_every_stage_as_it_was() {
    let (mut control, chain, _params) = rig();
    let before = control.computer_part();
    chain.refuse(Some(EqFailure {
        what: "the chain refused".into(),
        next: "Try again".into(),
    }));
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0)], -2.0);
    part.crossfeed_on = true;
    part.crossfeed = Some(CrossfeedSettings::default());
    part.loudness = loud(true);
    let failure = control.recall(part).await.unwrap_err();
    assert_eq!(failure.what, "the chain refused");
    assert_eq!(control.computer_part(), before);
    assert!(!control.view().on);
    assert_eq!(control.view().loads, 0);
}

/// The place of the first port with `prefix` in `send`.
fn first_of(send: &[(String, f64)], prefix: &str) -> usize {
    send.iter()
        .position(|(name, _)| name.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix} port in the line"))
}

#[tokio::test]
async fn a_line_that_lowers_the_stage_puts_it_before_the_slots_and_one_that_raises_it_after() {
    let (mut control, _chain, params) = rig();
    let mut on = eq_part(vec![pk(100.0, 0.0, 1.0)], 0.0);
    on.crossfeed_on = true;
    on.crossfeed = Some(CrossfeedSettings::for_model(Model::MidSide));
    control.recall(on).await.unwrap();
    // Crossfeed goes off while a boost comes in: the stage's cost falls, so its ports go first.
    control
        .recall(eq_part(vec![pk(100.0, 3.0, 1.0)], 0.0))
        .await
        .unwrap();
    let send = params.sent().last().unwrap().clone();
    assert!(
        first_of(&send, "ldir:") < first_of(&send, "l01:"),
        "stage before slots"
    );
    // And it comes back on: the slots go first, under the preamp that fell before them.
    let mut again = eq_part(vec![pk(100.0, 0.0, 1.0)], 0.0);
    again.crossfeed_on = true;
    again.crossfeed = Some(CrossfeedSettings::for_model(Model::MidSide));
    control.recall(again).await.unwrap();
    let send = params.sent().last().unwrap().clone();
    assert!(
        first_of(&send, "l01:") < first_of(&send, "ldir:"),
        "slots before stage"
    );
    assert!(
        first_of(&send, "lpre:") < first_of(&send, "l01:"),
        "the preamp fell first"
    );
}

#[tokio::test]
async fn a_part_whose_filters_are_not_in_frequency_order_is_on_once_recalled() {
    let (mut control, _chain, _params) = rig();
    // A scene written by hand: the controller keeps its points in frequency order.
    let part = eq_part(vec![pk(4000.0, -3.0, 2.0), pk(100.0, 2.0, 1.0)], -2.0);
    control.recall(part.clone()).await.unwrap();
    assert_eq!(control.view().points[0].f, 100.0);
    assert!(part.matches(&control.view()));
}

#[tokio::test]
async fn a_crossfeed_delay_just_above_the_models_own_is_read_at_the_outputs_rate() {
    let (mut control, _chain, params) = rig();
    params.set_output(Some(OutputSink {
        name: "a".into(),
        description: None,
        rate_hz: Some(192_000),
    }));
    let mut settings = CrossfeedSettings::for_model(Model::Bauer);
    settings.delay_ms = Some(0.30);
    let mut part = eq_part(vec![pk(100.0, 0.0, 1.0)], 0.0);
    part.crossfeed_on = true;
    part.crossfeed = Some(settings);
    control.recall(part.clone()).await.unwrap();
    assert!(part.matches(&control.view()));
}

fn xf_part(model: Model) -> ComputerPart {
    let mut part = eq_part(vec![pk(100.0, 0.0, 1.0)], 0.0);
    part.crossfeed_on = true;
    part.crossfeed = Some(CrossfeedSettings::for_model(model));
    part
}

#[tokio::test]
async fn a_stage_above_384_khz_refuses_the_part_and_changes_nothing() {
    let (mut control, chain, params) = rig();
    control
        .load(
            "Old".into(),
            None,
            correction(vec![pk(500.0, -1.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    params.set_rate(Some(768_000));
    ticks(&mut control, 60).await;
    let (before, sends, calls) = (
        control.computer_part(),
        params.sent().len(),
        chain.calls().len(),
    );
    let failure = control.recall(xf_part(Model::Bauer)).await.unwrap_err();
    assert!(failure.what.contains("384 kHz"), "{}", failure.what);
    assert_eq!(control.computer_part(), before);
    assert_eq!((params.sent().len(), chain.calls().len()), (sends, calls));
}

#[tokio::test]
async fn a_failed_live_send_falls_back_to_an_apply_with_the_stage_in_the_config() {
    let (mut control, chain, params) = rig();
    control
        .load(
            "Old".into(),
            None,
            correction(vec![pk(500.0, -1.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    assert_eq!(applies(&chain), 1);
    params.takes(false);
    control.recall(xf_part(Model::Bauer)).await.unwrap();
    // One apply for the send that did not take (no second one for the stage), the stage in its config.
    assert_eq!(applies(&chain), 2);
    assert!(control.view().crossfeed.unwrap().on);
    params.takes(true);
    // Crossfeed off while the config holds the stage: a rebuild, not a live line.
    let sends = params.sent().len();
    control
        .recall(eq_part(vec![pk(100.0, 0.0, 1.0)], 0.0))
        .await
        .unwrap();
    assert_eq!(
        applies(&chain),
        3,
        "the config is written without the stage"
    );
    assert_eq!(params.sent().len(), sends, "no live line");
    assert!(!control.view().crossfeed.unwrap().on);
}

#[tokio::test]
async fn a_failed_live_send_and_a_refused_fallback_leave_every_stage_as_it_was() {
    let (mut control, chain, params) = rig();
    control
        .load(
            "Old".into(),
            None,
            correction(vec![pk(500.0, -1.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    let before = control.computer_part();
    params.takes(false);
    chain.refuse(Some(EqFailure {
        what: "the chain refused".into(),
        next: "Try again".into(),
    }));
    let failure = control.recall(xf_part(Model::Bauer)).await.unwrap_err();
    assert_eq!(failure.what, "the chain refused");
    assert_eq!(control.computer_part(), before);
    assert!(control.view().on, "the chain it had still plays");
    chain.refuse(None);
    params.takes(true);
    control.recall(xf_part(Model::Bauer)).await.unwrap();
    assert!(control.view().crossfeed.unwrap().on);
}

#[tokio::test]
async fn an_all_off_part_while_a_stream_plays_keeps_the_chain_as_a_pass_through() {
    let (mut control, chain, params) = rig();
    chain.restarts_node_of(&params);
    control
        .load(
            "Old".into(),
            None,
            correction(vec![pk(500.0, -1.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    params.set_playing(true);
    control.recall(silent_part()).await.unwrap();
    assert_eq!(removes(&chain), 0, "the sink stays under the stream");
    assert_eq!(control.view().note.as_deref(), Some(PASSTHROUGH_NOTE));
    assert!(!control.view().on);
}

#[tokio::test]
async fn crossfeed_to_the_speakers_and_back_is_one_live_line_each_with_no_apply() {
    let (mut control, chain, params, _dir) = spk_rig();
    let mut speakers = eq_part(vec![pk(100.0, 0.0, 1.0)], 0.0);
    speakers.speakers_on = true;
    speakers.speakers = Some("set-a".into());
    control.recall(speakers.clone()).await.unwrap();
    let built = applies(&chain);
    let sends = params.sent().len();
    let crossfeed = xf_part(Model::Bauer);
    control.recall(crossfeed.clone()).await.unwrap();
    assert_eq!(applies(&chain), built);
    assert_eq!(params.sent().len(), sends + 1, "one line");
    assert!(crossfeed.matches(&control.view()));
    let mut back = speakers.clone();
    back.crossfeed = crossfeed.crossfeed.clone();
    control.recall(back.clone()).await.unwrap();
    assert_eq!(applies(&chain), built);
    assert_eq!(params.sent().len(), sends + 2, "one line");
    assert!(back.matches(&control.view()));
}

#[tokio::test]
async fn a_kept_set_that_is_not_fetched_is_left_as_it_was_and_said() {
    let (mut control, _chain, _params, _dir) = spk_rig();
    let mut part = eq_part(vec![pk(100.0, 0.0, 1.0)], 0.0);
    part.speakers = Some("gone".into());
    let recalled = control.recall(part).await.unwrap();
    assert_eq!(recalled.notes.len(), 1, "{:?}", recalled.notes);
    assert!(recalled.notes[0].contains("gone"), "{:?}", recalled.notes);
    assert_eq!(control.computer_part().speakers, None);
}

#[tokio::test]
async fn a_held_level_is_promised_a_lift_only_while_the_eq_plays() {
    let (mut control, chain, params) = rig();
    chain.restarts_node_of(&params);
    control.load("Hot".into(), None, hot()).await.unwrap();
    params.set_playing(true);
    // The EQ off with a stage on: the chain plays flat and is never lifted, so nothing is promised.
    let mut part = xf_part(Model::Bauer);
    part.eq_on = false;
    let recalled = control.recall(part).await.unwrap();
    assert!(recalled.notes.is_empty(), "{:?}", recalled.notes);
    assert_eq!(control.view().note, None);
}

#[tokio::test]
async fn a_cheaper_stage_over_a_bigger_boost_is_held_with_the_held_words_and_lifted_when_idle() {
    let (mut control, chain, params) = rig();
    chain.restarts_node_of(&params);
    control
        .recall(eq_part(vec![pk(100.0, 3.0, 1.0)], 0.0))
        .await
        .unwrap();
    let mut flat = xf_part(Model::MidSide);
    flat.filters = vec![pk(1000.0, 0.0, 1.0)];
    control.recall(flat).await.unwrap();
    assert_eq!(applies(&chain), 1);
    params.set_playing(true);
    let mut cheaper = CrossfeedSettings::for_model(Model::MidSide);
    cheaper.amount = 0.25;
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0)], 0.0);
    part.crossfeed_on = true;
    part.crossfeed = Some(cheaper);
    let sends = params.sent().len();
    let recalled = control.recall(part.clone()).await.unwrap();
    assert_eq!(applies(&chain), 1, "no apply under the stream");
    assert_eq!(recalled.notes, vec![LEVEL_HELD_NOTE.to_string()]);
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    assert_eq!(params.sent().len(), sends + 1, "one line");
    let held = control.view().preamp_db;
    // The line goes stage first and the preamp stays under what both orders need.
    let send = params.sent().last().unwrap().clone();
    assert!(first_of(&send, "ldir:") < first_of(&send, "l01:"));
    params.set_playing(false);
    ticks(&mut control, IDLE_KEEP + 3 * WATCH_EVERY).await;
    assert_eq!(
        applies(&chain),
        2,
        "applied again once the music has paused"
    );
    assert!(
        control.view().preamp_db > held,
        "{} over {held}",
        control.view().preamp_db
    );
    assert_eq!(control.view().note, None);
    assert!(part.matches(&control.view()));
}

/// A controller over a chain whose applies can be refused, on the node of `params`.
fn hooked() -> (EqControl, super::vs::HookChain, FakeParams) {
    let chain = FakeChain::new();
    let hook = super::vs::HookChain::over(chain.clone());
    let params = FakeParams::new();
    params.set_output(Some(sink("a")));
    params.with_loudness_ports();
    let control = EqControl::new(
        Arc::new(hook.clone()),
        Box::new(params.clone()),
        Keep::Memory,
    );
    (control, hook, params)
}

/// The three recalls of a cheaper stage over a bigger boost: +3 dB with no stage (the chain's
/// config), flat under Mid/Side, then +2 dB under Mid/Side at a quarter.
async fn up_to_the_held_recall(control: &mut EqControl) -> ComputerPart {
    control
        .recall(eq_part(vec![pk(100.0, 3.0, 1.0)], 0.0))
        .await
        .unwrap();
    let mut flat = xf_part(Model::MidSide);
    flat.filters = vec![pk(1000.0, 0.0, 1.0)];
    control.recall(flat).await.unwrap();
    let mut cheaper = CrossfeedSettings::for_model(Model::MidSide);
    cheaper.amount = 0.25;
    let mut part = eq_part(vec![pk(100.0, 2.0, 1.0)], 0.0);
    part.crossfeed_on = true;
    part.crossfeed = Some(cheaper);
    part
}

#[tokio::test]
async fn a_recall_whose_apply_is_refused_leaves_no_hold_on_the_old_state() {
    let (mut control, hook, _params) = hooked();
    let part = up_to_the_held_recall(&mut control).await;
    let before = control.view().preamp_db;
    hook.set(|call| {
        matches!(call, ChainCall::Apply(_)).then(|| EqFailure {
            what: "no".into(),
            next: "later".into(),
        })
    });
    control.recall(part).await.unwrap_err();
    assert_eq!(
        control.view().preamp_db,
        before,
        "the old state plays at its own level"
    );
    assert_eq!(control.view().note, None);
    hook.set(|_| None);
    ticks(&mut control, IDLE_KEEP + 3 * WATCH_EVERY).await;
    assert_eq!(control.view().preamp_db, before, "and nothing is lifted");
}

#[tokio::test]
async fn a_later_recall_that_needs_no_hold_drops_the_one_before_and_is_lifted_the_normal_way() {
    let (mut control, _hook, params) = hooked();
    let _chain = _hook.inner.clone();
    let part = up_to_the_held_recall(&mut control).await;
    params.set_playing(true);
    control.recall(part.clone()).await.unwrap();
    assert_eq!(control.view().note.as_deref(), Some(LEVEL_HELD_NOTE));
    let held = control.view().preamp_db;
    // A recall back to the dearer stage needs no hold: its line is slots first at its own level.
    let mut dearer = xf_part(Model::MidSide);
    dearer.filters = vec![pk(1000.0, 0.0, 1.0)];
    control.recall(dearer.clone()).await.unwrap();
    assert_eq!(control.view().note, None, "no hold, no words");
    assert!(dearer.matches(&control.view()));
    assert!(
        control.view().preamp_db > held,
        "it plays at its own level, not the held one"
    );
    // The pause lifts nothing: there is no hold left to lift.
    params.set_playing(false);
    let applied = applies(&_chain);
    ticks(&mut control, IDLE_KEEP + 3 * WATCH_EVERY).await;
    assert_eq!(applies(&_chain), applied);
}

#[tokio::test]
async fn a_lone_crossfeed_change_to_a_cheaper_setting_is_not_held_and_goes_slots_first() {
    let (mut control, chain, params) = rig();
    chain.restarts_node_of(&params);
    control
        .recall(eq_part(vec![pk(100.0, 3.0, 1.0)], 0.0))
        .await
        .unwrap();
    let mut flat = xf_part(Model::MidSide);
    flat.filters = vec![pk(1000.0, 0.0, 1.0)];
    control.recall(flat).await.unwrap();
    let applied = applies(&chain);
    let at_cap = control.view().preamp_db;
    // The same chain, no slot moving: a drag of the amount down.
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(25.0),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let send = params.sent().last().unwrap().clone();
    assert!(
        first_of(&send, "l01:") < first_of(&send, "ldir:"),
        "slots first"
    );
    assert_eq!(control.view().note, None, "nothing is held");
    assert!(
        control.view().preamp_db >= at_cap - 1e-9,
        "{} under the cap {at_cap}",
        control.view().preamp_db
    );
    ticks(&mut control, IDLE_KEEP + 3 * WATCH_EVERY).await;
    assert_eq!(applies(&chain), applied, "and nothing is lifted");
}
