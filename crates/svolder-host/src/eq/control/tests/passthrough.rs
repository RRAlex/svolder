// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The last reason for the chain going away while a stream plays: the chain stays as a
//! pass-through and goes once nothing has played through it for a while.

use super::*;
use crate::eq::live::OutputSink;
use svolder_profiles::tone::apo::Correction;

fn xf(on: bool) -> CrossfeedEdit {
    CrossfeedEdit {
        on: Some(on),
        done: true,
        ..CrossfeedEdit::default()
    }
}

fn applied(chain: &FakeChain) -> usize {
    chain
        .calls()
        .iter()
        .filter(|call| matches!(call, ChainCall::Apply(_)))
        .count()
}

fn removed(chain: &FakeChain) -> usize {
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

/// Crossfeed alone on a chain a stream plays through.
async fn flat_playing() -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(OutputSink {
        name: "a".into(),
        description: None,
        rate_hz: Some(48_000),
    }));
    control.crossfeed(xf(true)).await.unwrap();
    params.set_playing(true);
    (control, chain, params)
}

#[tokio::test]
async fn crossfeed_off_with_a_stream_playing_keeps_the_chain_flat_and_removes_it_when_idle() {
    let (mut control, chain, params) = flat_playing().await;
    let (apps, sends) = (applied(&chain), params.sent().len());
    control.crossfeed(xf(false)).await.unwrap();
    assert_eq!(removed(&chain), 0, "the sink stays");
    assert_eq!(applied(&chain), apps, "no apply, no swap");
    let sent = &params.sent()[sends..];
    let last = sent.last().expect("the stage and the slots went live");
    assert_eq!(
        value(last, "lmix:Gain 2"),
        Some(0.0),
        "the stage is bypassed"
    );
    assert_eq!(
        value(last, "l01:b0"),
        Some(1.0),
        "the slots are the identity"
    );
    assert!(
        value(last, "lpre:Mult").is_some_and(|mult| (mult - 1.0).abs() < 1e-9),
        "the preamp is 0 dB"
    );
    let view = control.view();
    assert!(!view.on && !view.crossfeed.as_ref().unwrap().on);
    assert_eq!(view.note.as_deref(), Some(PASSTHROUGH_NOTE));
    assert_eq!(view.preamp_db, 0.0);

    // Playing: the chain stays past the idle time.
    ticks(&mut control, 700).await;
    assert_eq!(removed(&chain), 0);
    assert!(control.view().note.is_some());

    // Idle: it goes once, after the idle time and not before.
    params.set_playing(false);
    ticks(&mut control, 500).await;
    assert_eq!(removed(&chain), 0, "not before about 30 s");
    ticks(&mut control, 200).await;
    assert_eq!(removed(&chain), 1);
    assert_eq!(control.view().note, None);
    assert!(!chain.state().unwrap().on);
}

#[tokio::test]
async fn crossfeed_off_with_nothing_playing_removes_the_chain_at_once() {
    let (mut control, chain, params) = flat_playing().await;
    params.set_playing(false);
    control.crossfeed(xf(false)).await.unwrap();
    assert_eq!(removed(&chain), 1);
    assert_eq!(control.view().note, None);
}

#[tokio::test]
async fn turning_back_on_in_the_grace_reuses_the_chain() {
    let (mut control, chain, params) = flat_playing().await;
    let apps = applied(&chain);
    control.crossfeed(xf(false)).await.unwrap();
    ticks(&mut control, 100).await;
    let sends = params.sent().len();
    control.crossfeed(xf(true)).await.unwrap();
    assert_eq!(applied(&chain), apps, "no apply");
    assert_eq!(removed(&chain), 0);
    let sent = &params.sent()[sends..];
    assert_eq!(
        sent.last().and_then(|last| value(last, "lmix:Gain 2")),
        Some(1.0)
    );
    assert!(control.view().crossfeed.unwrap().on);
    assert_eq!(control.view().note, None);
    // Still on well past the idle time, with nothing playing.
    params.set_playing(false);
    ticks(&mut control, 800).await;
    assert_eq!(removed(&chain), 0, "a chain the crossfeed holds stays");

    // The EQ coming on in the grace is the same.
    control.crossfeed(xf(false)).await.unwrap();
    assert_eq!(removed(&chain), 1, "nothing plays: removed at once");
}

#[tokio::test]
async fn the_eq_on_in_the_grace_reuses_the_chain() {
    let (mut control, chain, params) = flat_playing().await;
    let apps = applied(&chain);
    control.crossfeed(xf(false)).await.unwrap();
    control.set_on(true).await.unwrap();
    assert_eq!(applied(&chain), apps);
    assert_eq!(removed(&chain), 0);
    assert!(control.view().on);
    params.set_playing(false);
    ticks(&mut control, 800).await;
    assert_eq!(removed(&chain), 0, "the EQ holds the chain");
}

#[tokio::test]
async fn the_eq_off_with_crossfeed_off_and_a_stream_playing_keeps_a_flat_chain() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 6.0, true)).await.unwrap();
    params.set_playing(true);
    let (apps, sends) = (applied(&chain), params.sent().len());
    control.set_on(false).await.unwrap();
    assert_eq!((applied(&chain), removed(&chain)), (apps, 0));
    let last = params.sent()[sends..].last().cloned().expect("sent live");
    assert_eq!(value(&last, "l01:b0"), Some(1.0));
    let view = control.view();
    assert!(!view.on);
    assert_eq!(view.note.as_deref(), Some(PASSTHROUGH_NOTE));
    assert_eq!(view.points[0].g, 6.0, "the points are kept");
    params.set_playing(false);
    ticks(&mut control, 700).await;
    assert_eq!(removed(&chain), 1);
}

#[tokio::test]
async fn a_graceful_stop_removes_a_pass_through_chain() {
    let (mut control, chain, _) = flat_playing().await;
    control.crossfeed(xf(false)).await.unwrap();
    control.shutdown().await;
    assert_eq!(removed(&chain), 1);
    assert!(!chain.state().unwrap().on);
}

#[tokio::test]
async fn a_second_off_in_the_grace_keeps_the_chain_and_the_timer() {
    let (mut control, chain, params) = flat_playing().await;
    control.crossfeed(xf(false)).await.unwrap();
    // The stream ends at once, so the idle time counts from the first off.
    params.set_playing(false);
    ticks(&mut control, 300).await;
    // An off sent again (a switch that repeats, a double toggle) changes nothing: the chain
    // stays, and the timer is not restarted.
    control.set_on(false).await.unwrap();
    control.crossfeed(xf(false)).await.unwrap();
    assert_eq!(removed(&chain), 0);
    ticks(&mut control, 350).await;
    assert_eq!(
        removed(&chain),
        1,
        "gone 30 s after the first off, not after the second"
    );
}

#[tokio::test]
async fn a_pass_through_chain_that_dies_is_not_started_again() {
    let (mut control, chain, _) = flat_playing().await;
    control.crossfeed(xf(false)).await.unwrap();
    let apps = applied(&chain);
    chain.die();
    tokio::task::yield_now().await;
    control.tick().await;
    assert_eq!(applied(&chain), apps, "no apply");
    let view = control.view();
    assert!(!view.on && view.note.is_none() && !view.live);
    ticks(&mut control, 800).await;
    assert_eq!(applied(&chain), apps);
}

/// The state file a pass-through left behind is read by a new owner while the chain still runs.
#[tokio::test]
async fn an_owner_that_starts_over_a_pass_through_removes_it_without_a_crossfeed_note() {
    let dir = Dir::new();
    let chain = FakeChain::new();
    let params = FakeParams::new();
    let mut first = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params.clone()),
        Keep::File(dir.state()),
    );
    first.crossfeed(xf(true)).await.unwrap();
    params.set_playing(true);
    first.crossfeed(xf(false)).await.unwrap();
    assert_eq!(removed(&chain), 0);
    drop(first);

    let mut second = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params),
        Keep::File(dir.state()),
    );
    assert!(second.sync_running().await || removed(&chain) == 1);
    assert_eq!(removed(&chain), 1, "removed at the first call");
    let xf = second.view().crossfeed.unwrap();
    assert_eq!((xf.note, xf.off_reason), (None, None));
    assert!(!HostEqState::load(&dir.state()).unwrap().unwrap().on);
}

#[tokio::test]
async fn an_owner_that_starts_with_the_pass_through_gone_applies_nothing() {
    let dir = Dir::new();
    let chain = FakeChain::new();
    let params = FakeParams::new();
    let mut first = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params.clone()),
        Keep::File(dir.state()),
    );
    first.crossfeed(xf(true)).await.unwrap();
    params.set_playing(true);
    first.crossfeed(xf(false)).await.unwrap();
    drop(first);
    chain.remove().await.unwrap();
    let apps = applied(&chain);

    let mut second = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params),
        Keep::File(dir.state()),
    );
    assert!(!second.reapply_if_on().await.unwrap());
    assert_eq!(applied(&chain), apps);
    assert!(!HostEqState::load(&dir.state()).unwrap().unwrap().on);
}

#[tokio::test]
async fn a_pass_through_over_a_boosted_preset_plays_under_the_preamp_that_preset_needs() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control
        .load(
            "Boost".into(),
            None,
            Correction {
                preamp_db: -6.0,
                filters: vec![pk(1000.0, 6.0, 1.0)],
            },
        )
        .await
        .unwrap();
    params.set_playing(true);
    let (apps, sends) = (applied(&chain), params.sent().len());
    control.set_on(false).await.unwrap();
    assert_eq!((applied(&chain), removed(&chain)), (apps, 0));
    let view = control.view();
    assert!(
        view.preamp_db < -5.0 && view.preamp_db > -7.0,
        "the loaded config would play its boost again after a rate change: {}",
        view.preamp_db
    );
    let last = params.sent()[sends..].last().cloned().unwrap();
    let mult = value(&last, "lpre:Mult").unwrap();
    assert!((20.0 * mult.log10() - view.preamp_db).abs() < 1e-6);
}

/// A high shelf playable at 48 kHz that cannot play at 192 kHz.
pub(super) fn shelf() -> Correction {
    Correction {
        preamp_db: 0.0,
        filters: vec![SourceFilter {
            kind: FilterKind::HighShelf,
            f: 1000.0,
            g: 14.5,
            q: 0.707,
        }],
    }
}

/// Neither the chain nor a note may say the EQ turned off: it was off.
fn no_eq_turned_off(control: &EqControl) {
    let note = control.view().note.unwrap_or_default();
    assert!(!note.contains("computer EQ turned off"), "{note}");
}

#[tokio::test]
async fn crossfeed_alone_stays_on_at_a_rate_its_kept_points_cannot_play_at() {
    let dir = Dir::new();
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    control.load("Shelf".into(), None, shelf()).await.unwrap();
    control.set_on(false).await.unwrap();
    control.crossfeed(xf(true)).await.unwrap();
    let removes = removed(&chain);
    params.set_node(Some(node(1, Some(192_000))));
    ticks(&mut control, WATCH_EVERY).await;
    assert_eq!(removed(&chain), removes, "the crossfeed's chain stays");
    no_eq_turned_off(&control);
    let view = control.view();
    assert!(view.crossfeed.expect("served").on);
    assert_eq!(view.points[0].g, 14.5, "the points are kept");
    // Turning the EQ on there is refused, and nothing it cannot play is sent.
    let refused = control.set_on(true).await.unwrap_err();
    assert!(refused.what.contains("point 1"), "{}", refused.what);
    nothing_past_ten(&params);
    assert_eq!(removed(&chain), removes);
    assert!(control.view().crossfeed.expect("served").on);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(
        state.preamp_db > -20.0,
        "the saved preamp is a curve's, not the floor: {}",
        state.preamp_db
    );
}

#[tokio::test]
async fn a_pass_through_is_not_removed_for_kept_points_its_rate_cannot_play() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.load("Shelf".into(), None, shelf()).await.unwrap();
    params.set_playing(true);
    control.set_on(false).await.unwrap();
    let removes = removed(&chain);
    params.set_node(Some(LiveNode {
        playing: true,
        ..node(1, Some(192_000))
    }));
    ticks(&mut control, WATCH_EVERY).await;
    assert_eq!(removed(&chain), removes, "the stream keeps its sink");
    no_eq_turned_off(&control);
    nothing_past_ten(&params);
}

#[tokio::test]
async fn crossfeed_alone_saves_kept_points_with_the_preamp_they_played_at() {
    let dir = Dir::new();
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    params.set_node(Some(node(1, Some(48_000))));
    let low = vec![
        SourceFilter {
            kind: FilterKind::HighShelf,
            f: 14.6,
            g: -8.59,
            q: 2.35,
        },
        SourceFilter {
            kind: FilterKind::HighShelf,
            f: 49.7,
            g: -1.65,
            q: 0.32,
        },
    ];
    let low = Correction {
        preamp_db: -1.5,
        filters: low,
    };
    control.load("Low".into(), None, low).await.unwrap();
    control.set_on(false).await.unwrap();
    control.crossfeed(xf(true)).await.unwrap();
    params.set_node(Some(node(1, Some(352_800))));
    ticks(&mut control, WATCH_EVERY).await;
    no_eq_turned_off(&control);
    let level = CrossfeedEdit {
        amount_pct: Some(30.0),
        done: true,
        ..CrossfeedEdit::default()
    };
    control.crossfeed(level).await.unwrap();
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(state.crossfeed_on && state.flat);
    assert!(
        state.preamp_db > -20.0,
        "the saved preamp is a curve's, not the floor: {}",
        state.preamp_db
    );
}
