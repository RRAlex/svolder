// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The climb: a change that raises the level (a stage off, the EQ off into a pass-through, the
//! bypass) starts the preamp at the level heard before, in the same line as the stage's ports,
//! and brings it up at 0.32 dB a tick to the target, never above the ceiling; a lower target ends
//! it at once.

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

fn xf(on: bool) -> CrossfeedEdit {
    CrossfeedEdit {
        on: Some(on),
        done: true,
        ..CrossfeedEdit::default()
    }
}

fn mid_side() -> CrossfeedEdit {
    CrossfeedEdit {
        model: Some("mid-side".into()),
        ..xf(true)
    }
}

fn db(send: &[(String, f64)]) -> Option<f64> {
    value(send, "lpre:Mult").map(|mult| 20.0 * mult.log10())
}

/// The preamps of the lines sent after the first `from` of them, in dB.
fn preamps(params: &FakeParams, from: usize) -> Vec<f64> {
    params.sent()[from..]
        .iter()
        .filter_map(|send| db(send))
        .collect()
}

async fn ticks(control: &mut EqControl, count: u64) {
    for _ in 0..count {
        control.tick().await;
    }
}

/// A chain a stream plays through, crossfeed (Mid/Side bass, about 3.1 dB) on.
async fn playing_mid_side() -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    params.with_loudness_ports();
    control.crossfeed(mid_side()).await.unwrap();
    params.set_playing(true);
    ticks(&mut control, 60).await;
    (control, chain, params)
}

#[test]
fn the_pace_is_the_dac_rules_pace_over_a_64_db_range() {
    assert!((LEVEL_CLIMB_DB_PER_TICK - 0.32).abs() < 1e-12);
}

#[tokio::test]
async fn crossfeed_off_starts_at_the_level_heard_and_climbs_to_zero_at_the_pace() {
    let (mut control, _chain, params) = playing_mid_side().await;
    let cost = control.stage_gain();
    assert!((cost - 3.1).abs() < 0.05, "{cost}");
    let sends = params.sent().len();
    control.crossfeed(xf(false)).await.unwrap();
    // The first line: the preamp where it was (the sound does not rise) and the stage bypassed.
    let first = &params.sent()[sends];
    assert!((db(first).unwrap() + cost).abs() < 1e-6, "{:?}", db(first));
    assert_eq!(value(first, "lmix:Gain 2"), Some(0.0));
    let view = control.view();
    let levels = view.levels.clone().unwrap();
    assert_eq!(levels.climb_to_db, Some(0.0));
    assert_eq!(levels.words, "climbing to 0.0 dB");
    assert!((view.preamp_db + cost).abs() < 1e-6);
    // Every tick that follows raises it one step, until it is exactly 0 dB.
    ticks(&mut control, 30).await;
    let sent = preamps(&params, sends);
    assert!(sent.len() >= 10, "{sent:?}");
    for pair in sent.windows(2) {
        let step = pair[1] - pair[0];
        assert!(
            (-1e-9..=LEVEL_CLIMB_DB_PER_TICK + 1e-6).contains(&step),
            "a step of {step} dB in {sent:?}"
        );
    }
    assert_eq!(sent.last().copied().map(f64::abs), Some(0.0), "{sent:?}");
    let levels = control.view().levels.unwrap();
    assert_eq!(levels.climb_to_db, None);
    assert!(levels.passthrough && !levels.words.starts_with("climbing"));
}

#[tokio::test]
async fn a_lower_target_ends_the_climb_at_once() {
    let (mut control, _chain, params) = playing_mid_side().await;
    control.crossfeed(xf(false)).await.unwrap();
    ticks(&mut control, 4).await;
    let raised = control.view().preamp_db;
    assert!(raised < -0.5 && control.view().levels.unwrap().climb_to_db.is_some());
    let sends = params.sent().len();
    control.crossfeed(mid_side()).await.unwrap();
    let cost = control.stage_gain();
    let first = &params.sent()[sends];
    assert!(
        (db(first).unwrap() + cost).abs() < 1e-6,
        "the preamp falls to the stage's at once, in the line that turns it on: {:?}",
        db(first)
    );
    ticks(&mut control, 20).await;
    assert!(
        preamps(&params, sends)
            .iter()
            .all(|level| *level <= -cost + 1e-6),
        "never above the target"
    );
    assert_eq!(control.view().levels.unwrap().climb_to_db, None);
}

#[tokio::test]
async fn the_eq_off_into_a_pass_through_keeps_its_cut_and_climbs() {
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    control.edit(gain(0, 6.0, true)).await.unwrap();
    params.set_playing(true);
    let cut = control.view().preamp_db;
    assert!(cut < -5.0, "{cut}");
    let sends = params.sent().len();
    control.set_on(false).await.unwrap();
    let first = &params.sent()[sends];
    assert!((db(first).unwrap() - cut).abs() < 1e-6, "{:?}", db(first));
    assert_eq!(value(first, "l01:b0"), Some(1.0), "the slots go flat");
    assert_eq!(control.view().levels.unwrap().climb_to_db, Some(0.0));
    ticks(&mut control, 40).await;
    assert_eq!(control.view().preamp_db, 0.0);
    assert!(control.view().levels.unwrap().passthrough);
}

#[tokio::test]
async fn speakers_off_drops_the_preamp_by_the_sets_level_in_the_line_that_turns_them_off() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    let (control, _chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    params.with_speakers_ports(false);
    let mut control = control.with_speakers(store);
    control.edit(gain(0, 6.0, true)).await.unwrap();
    control
        .speakers(SpeakersEdit {
            on: Some(true),
            set: Some("set-a".into()),
            done: true,
        })
        .await
        .unwrap();
    params.set_playing(true);
    ticks(&mut control, 40).await;
    let before = control.view().levels.unwrap();
    assert!((before.space_db + 6.9).abs() < 1e-9, "{}", before.space_db);
    let sends = params.sent().len();
    control
        .speakers(SpeakersEdit {
            on: Some(false),
            set: None,
            done: true,
        })
        .await
        .unwrap();
    let first = &params.sent()[sends];
    // The sound stays where it was: the preamp falls by the set's level, before the mixers.
    let want = before.total_db;
    assert!(
        (db(first).unwrap() - want).abs() < 1e-6,
        "{:?} vs {want}",
        db(first)
    );
    let pre = first
        .iter()
        .position(|(name, _)| name == "lpre:Mult")
        .unwrap();
    let mix = first
        .iter()
        .position(|(name, _)| name == "lvs:Gain 1")
        .unwrap();
    assert!(pre < mix, "the falling preamp goes before the mixers");
    assert_eq!(value(first, "lvs:Gain 1"), Some(1.0), "the dry sound");
    let target = control.view().levels.unwrap().climb_to_db;
    assert!(
        target.is_some_and(|to| (to - (want + 6.9)).abs() < 1e-6),
        "{target:?}"
    );
    ticks(&mut control, 60).await;
    let after = control.view().levels.unwrap();
    assert_eq!(after.climb_to_db, None);
    assert!((after.total_db - (want + 6.9)).abs() < 1e-6);
}

#[tokio::test]
async fn the_bypass_climbs_from_what_was_heard() {
    let (mut control, _chain, params) = playing_mid_side().await;
    control.edit(gain(0, 6.0, true)).await.unwrap();
    ticks(&mut control, 10).await;
    let heard = control.view().levels.unwrap().total_db;
    assert!(heard < -6.0, "{heard}");
    let sends = params.sent().len();
    control.bypass(true).await.unwrap();
    let first = &params.sent()[sends];
    assert!((db(first).unwrap() - heard).abs() < 1e-6, "{:?}", db(first));
    assert_eq!(control.view().levels.unwrap().words, "climbing to 0.0 dB");
    ticks(&mut control, 80).await;
    assert_eq!(control.view().preamp_db, 0.0);
}

#[tokio::test]
async fn a_climb_that_nothing_plays_through_ends_with_the_chain() {
    let (mut control, chain, params) = playing_mid_side().await;
    control.crossfeed(xf(false)).await.unwrap();
    ticks(&mut control, 3).await;
    params.set_playing(false);
    ticks(&mut control, 120).await;
    assert_eq!(
        chain.calls().last(),
        Some(&ChainCall::Remove),
        "the idle pass-through goes"
    );
    let levels = control.view().levels.unwrap();
    assert_eq!((levels.climb_to_db, levels.words.as_str()), (None, ""));
}

#[tokio::test]
async fn a_rate_change_in_the_climb_goes_on_climbing() {
    let (mut control, _chain, params) = playing_mid_side().await;
    control.crossfeed(xf(false)).await.unwrap();
    ticks(&mut control, 3).await;
    params.set_rate(Some(44_100));
    ticks(&mut control, 60).await;
    assert_eq!(control.view().preamp_db, 0.0);
}

#[tokio::test]
async fn a_hand_edit_that_softens_the_curve_is_not_a_climb() {
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    control.edit(gain(0, 6.0, true)).await.unwrap();
    params.set_playing(true);
    ticks(&mut control, 40).await;
    let sends = params.sent().len();
    control.edit(gain(0, 0.0, true)).await.unwrap();
    assert_eq!(
        preamps(&params, sends).last().copied().map(f64::abs),
        Some(0.0),
        "an edit puts the preamp at what the curve needs, at once"
    );
    assert_eq!(control.view().levels.unwrap().climb_to_db, None);
}

/// The ports of an untouched chain, as a look reads them.
fn identity_ports() -> Vec<(String, f64)> {
    let mut ports = crate::eq::live::preamp_params(0.0);
    for slot in 1..=SLOTS as u8 {
        ports.extend(crate::eq::live::slot_params(slot, None, 48_000));
    }
    ports
}

#[tokio::test]
async fn a_pass_through_at_zero_over_an_identity_config_is_untouched_once_a_look_reads_it() {
    let (mut control, _chain, params) = playing_mid_side().await;
    control.crossfeed(xf(false)).await.unwrap();
    ticks(&mut control, 30).await;
    // The climb ended; the look after its last line reads every port at the identity.
    ticks(&mut control, 40).await;
    let levels = control.view().levels.unwrap();
    assert!(levels.untouched, "{levels:?}");
    assert_eq!(levels.words, "pass-through, untouched");
    // One port off its identity (another process moved it) and it is not untouched.
    params.set_ports(&[("l05:b0", 0.97)]);
    params.holds(false);
    ticks(&mut control, 40).await;
    let levels = control.view().levels.unwrap();
    assert!(!levels.untouched, "{levels:?}");
    assert!(identity_ports().len() > 32);
}

#[tokio::test]
async fn an_untouched_chain_stays_untouched_through_the_repeat_of_its_last_line() {
    // The looks fall every WATCH_EVERY ticks and the repeat of a line one WATCH_EVERY after it:
    // whatever the tick the climb ends on, once a look has proved the chain untouched, the repeat
    // (which carries nothing new) must not take that away until the next look.
    for offset in 0..WATCH_EVERY {
        let (mut control, _chain, _params) = playing_mid_side().await;
        ticks(&mut control, offset).await;
        control.crossfeed(xf(false)).await.unwrap();
        let mut proven = false;
        for tick in 0..200 {
            ticks(&mut control, 1).await;
            let levels = control.view().levels.unwrap();
            assert!(
                levels.untouched || !proven,
                "offset {offset}, tick {tick}: {levels:?}"
            );
            proven |= levels.untouched;
        }
        assert!(proven, "offset {offset}");
    }
}

#[tokio::test]
async fn a_fresh_identity_resend_revokes_untouched_until_a_look_after_it() {
    let (mut control, _chain, params) = playing_mid_side().await;
    control.crossfeed(xf(false)).await.unwrap();
    ticks(&mut control, 100).await;
    assert!(control.view().levels.unwrap().untouched);
    // A fresh line of identity values (the saved state reloaded, a chain taken over: every line
    // goes again). It is not a repeat: the chain is not proven untouched on the tick it goes,
    // nor before a look at least REVERT_QUIET after.
    let lines = params.sent().len();
    control.resend = true;
    control.watch_due = true;
    let mut revoked = false;
    let mut again_at = None;
    let mut sent_at = None;
    for tick in 0..100 {
        ticks(&mut control, 1).await;
        let untouched = control.view().levels.unwrap().untouched;
        if sent_at.is_none() && params.sent().len() > lines {
            sent_at = Some(tick);
            assert!(!untouched, "untouched on the tick of the fresh line");
        }
        revoked |= sent_at.is_some() && !untouched;
        if revoked && untouched && again_at.is_none() {
            again_at = Some(tick);
        }
    }
    let (sent, again) = (
        sent_at.expect("a fresh line went"),
        again_at.expect("proven again"),
    );
    assert!(
        again >= sent + REVERT_QUIET as usize,
        "sent {sent}, again {again}"
    );
}

#[tokio::test]
async fn a_chain_whose_config_holds_the_stage_is_never_untouched() {
    let (mut control, _chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    // A PipeWire that takes no live changes: the stage is in the config.
    control.live = false;
    control.crossfeed(xf(true)).await.unwrap();
    params.set_playing(true);
    ticks(&mut control, 60).await;
    assert!(!control.view().levels.unwrap().untouched);
}
