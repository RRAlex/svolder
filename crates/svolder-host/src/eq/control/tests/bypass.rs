// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bypass (every stage off as one change, brought back as it was) and the levels the view
//! carries.

use super::vs::put_set;
use super::*;
use crate::eq::ComputerPart;
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

/// Mid/Side bass on: the model whose stage costs about 3.1 dB.
fn mid_side() -> CrossfeedEdit {
    CrossfeedEdit {
        model: Some("mid-side".into()),
        ..xf(true)
    }
}

fn ld(on: bool) -> LoudnessEdit {
    LoudnessEdit {
        on: Some(on),
        done: true,
        ..LoudnessEdit::default()
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

fn rig(keep: Keep) -> (EqControl, FakeChain, FakeParams) {
    let (control, chain, params) = setup(keep);
    params.set_output(Some(sink()));
    params.with_loudness_ports();
    (control, chain, params)
}

/// The EQ (cutting 6 dB) and loudness on, crossfeed off.
async fn eq_and_loudness(keep: Keep) -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = rig(keep);
    control.edit(gain(0, 6.0, true)).await.unwrap();
    control.loudness(ld(true)).await.unwrap();
    (control, chain, params)
}

fn bypass_on(control: &EqControl) -> bool {
    control.view().bypass.expect("a controller serves it").on
}

fn held(control: &EqControl) -> crate::eq::HeldStages {
    control.view().bypass.unwrap().held
}

#[tokio::test]
async fn levels_of_the_eq_alone_are_its_preamp() {
    let (mut control, _chain, _params) = rig(Keep::Memory);
    control.edit(gain(0, 6.0, true)).await.unwrap();
    let view = control.view();
    let levels = view.levels.expect("a controller serves levels");
    assert!(view.preamp_db < -1.0, "{}", view.preamp_db);
    assert_eq!(levels.eq_db, view.preamp_db);
    assert_eq!(levels.space_db, 0.0);
    assert_eq!(levels.total_db, view.preamp_db);
    assert!(!levels.passthrough && !levels.untouched);
    assert_eq!(levels.climb_to_db, None);
    assert_eq!(
        levels.words,
        format!(
            "plays {} dB in all",
            crate::eq::levels::db_word(view.preamp_db)
        )
    );
}

#[tokio::test]
async fn levels_of_crossfeed_alone_are_its_cost() {
    let (mut control, _chain, _params) = rig(Keep::Memory);
    control.crossfeed(mid_side()).await.unwrap();
    let levels = control.view().levels.unwrap();
    let cost = control.stage_gain();
    assert!((cost - 3.1).abs() < 0.05, "{cost}");
    assert_eq!(levels.space_db, -cost);
    assert_eq!(levels.eq_db, 0.0, "the EQ plays flat");
    assert!((levels.total_db + cost).abs() < 1e-9);
    assert!(levels.words.starts_with("plays \u{2212}"));
}

#[tokio::test]
async fn levels_with_the_eq_and_a_stage_add_up() {
    let (mut control, _chain, _params) = rig(Keep::Memory);
    control.edit(gain(0, 6.0, true)).await.unwrap();
    let eq_only = control.view().levels.unwrap().eq_db;
    control.crossfeed(mid_side()).await.unwrap();
    let levels = control.view().levels.unwrap();
    assert!(
        (levels.eq_db - eq_only).abs() < 1e-9,
        "the EQ's share stays"
    );
    assert!(levels.space_db < -1.0);
    assert!((levels.total_db - (levels.eq_db + levels.space_db)).abs() < 1e-9);
}

#[tokio::test]
async fn levels_of_the_speakers_are_the_sets_level() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    let (control, _chain, params) = setup(Keep::Memory);
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
    let view = control.view();
    let levels = view.levels.unwrap();
    assert_eq!(levels.space_db, -6.9, "the set's level against Off");
    assert!((levels.total_db - (view.preamp_db - 6.9)).abs() < 1e-9);
    assert!(levels.words.contains("plays"));
}

#[tokio::test]
async fn levels_of_a_passthrough_say_so_and_no_chain_says_nothing() {
    let (mut control, _chain, params) = rig(Keep::Memory);
    let none = control.view().levels.unwrap();
    assert_eq!(
        (none.words.as_str(), none.total_db, none.passthrough),
        ("", 0.0, false)
    );
    control.crossfeed(xf(true)).await.unwrap();
    params.set_playing(true);
    control.crossfeed(xf(false)).await.unwrap();
    let levels = control.view().levels.unwrap();
    assert!(levels.passthrough);
    assert_eq!(levels.words, "pass-through 0.0 dB");
}

#[tokio::test]
async fn the_bypass_turns_everything_off_in_one_change_and_off_brings_back_exactly_that() {
    let (mut control, chain, _params) = eq_and_loudness(Keep::Memory).await;
    let before = control.computer_part();
    assert!(before.eq_on && before.loudness.unwrap().on && !before.crossfeed_on);
    control.bypass(true).await.unwrap();
    let view = control.view();
    assert!(!view.on && !view.loudness.as_ref().unwrap().on);
    assert!(bypass_on(&control));
    assert_eq!(
        held(&control),
        crate::eq::HeldStages {
            eq: true,
            loudness: true,
            crossfeed: false,
            speakers: false
        }
    );
    assert_eq!(removes(&chain), 1, "nothing plays: the chain goes once");
    control.bypass(false).await.unwrap();
    assert!(!bypass_on(&control));
    assert!(held(&control).is_empty());
    let after = control.computer_part();
    assert_eq!(after, before, "exactly what played, loudness included");
    assert!(control.view().on);
}

#[tokio::test]
async fn the_bypass_with_nothing_on_is_refused_in_words() {
    let (mut control, chain, _params) = rig(Keep::Memory);
    let calls = chain.calls().len();
    let refused = control.bypass(true).await.unwrap_err();
    assert_eq!(refused.what, "nothing plays on the computer to bypass");
    assert!(!bypass_on(&control));
    assert_eq!(chain.calls().len(), calls, "nothing was changed");
}

#[tokio::test]
async fn a_stream_keeps_the_chain_as_a_passthrough_through_the_bypass() {
    let (mut control, chain, params) = eq_and_loudness(Keep::Memory).await;
    params.set_playing(true);
    let (apps, gone, sends) = (applies(&chain), removes(&chain), params.sent().len());
    control.bypass(true).await.unwrap();
    assert_eq!(
        (applies(&chain), removes(&chain)),
        (apps, gone),
        "no apply, no swap"
    );
    assert_eq!(params.sent().len(), sends + 1, "one live line");
    let view = control.view();
    assert!(!view.on && view.levels.unwrap().passthrough);
    let last = params.sent().last().cloned().unwrap();
    assert_eq!(
        value(&last, "l01:b0"),
        Some(1.0),
        "the slots are the identity"
    );
    control.bypass(false).await.unwrap();
    assert!(control.view().on && !bypass_on(&control));
}

#[tokio::test]
async fn a_restore_that_is_refused_keeps_the_bypass_on() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    let (control, _chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    params.with_speakers_ports(false);
    let mut control = control.with_speakers(store.clone());
    control
        .speakers(SpeakersEdit {
            on: Some(true),
            set: Some("set-a".into()),
            done: true,
        })
        .await
        .unwrap();
    control.bypass(true).await.unwrap();
    assert!(held(&control).speakers);
    store.forget("set-a").unwrap();
    let refused = control.bypass(false).await.unwrap_err();
    assert!(!refused.what.is_empty());
    assert!(bypass_on(&control), "still bypassed");
    assert!(held(&control).speakers, "and the held set is kept");
}

/// A bypassed controller (the EQ and loudness held).
async fn bypassed() -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = eq_and_loudness(Keep::Memory).await;
    control.bypass(true).await.unwrap();
    assert!(bypass_on(&control));
    (control, chain, params)
}

#[tokio::test]
async fn the_eq_switch_ends_the_bypass() {
    let (mut control, _, _) = bypassed().await;
    control.set_on(true).await.unwrap();
    assert!(!bypass_on(&control) && held(&control).is_empty());
}

#[tokio::test]
async fn a_load_ends_the_bypass() {
    let (mut control, _, _) = bypassed().await;
    control
        .load(
            "Desk".into(),
            None,
            correction(vec![pk(100.0, 2.0, 1.0)], -2.0),
        )
        .await
        .unwrap();
    assert!(!bypass_on(&control));
}

#[tokio::test]
async fn an_edit_that_turns_the_eq_on_ends_the_bypass() {
    let (mut control, _, _) = bypassed().await;
    control.edit(gain(0, 3.0, true)).await.unwrap();
    assert!(control.view().on);
    assert!(!bypass_on(&control));
}

#[tokio::test]
async fn loudness_on_ends_the_bypass() {
    let (mut control, _, _) = bypassed().await;
    control.loudness(ld(true)).await.unwrap();
    assert!(!bypass_on(&control));
}

#[tokio::test]
async fn crossfeed_on_ends_the_bypass() {
    let (mut control, _, _) = bypassed().await;
    control.crossfeed(xf(true)).await.unwrap();
    assert!(!bypass_on(&control));
}

#[tokio::test]
async fn the_speakers_on_end_the_bypass() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    let (control, _chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    params.with_speakers_ports(false);
    params.with_loudness_ports();
    let mut control = control.with_speakers(store);
    control.edit(gain(0, 3.0, true)).await.unwrap();
    control.bypass(true).await.unwrap();
    control
        .speakers(SpeakersEdit {
            on: Some(true),
            set: Some("set-a".into()),
            done: true,
        })
        .await
        .unwrap();
    assert!(control.view().speakers.unwrap().on);
    assert!(!bypass_on(&control));
}

#[tokio::test]
async fn a_scene_recall_that_turns_a_stage_on_ends_the_bypass() {
    let (mut control, _, _) = bypassed().await;
    let part = ComputerPart {
        eq_on: true,
        ..control.computer_part()
    };
    control.recall(part).await.unwrap();
    assert!(!bypass_on(&control));
}

#[tokio::test]
async fn a_scene_recall_with_everything_off_leaves_the_bypass() {
    let (mut control, _, _) = bypassed().await;
    let part = control.computer_part();
    assert!(!part.eq_on);
    control.recall(part).await.unwrap();
    assert!(bypass_on(&control));
}

#[tokio::test]
async fn the_held_set_is_saved_and_adopted_at_start() {
    let dir = Dir::new();
    let (mut control, _, _) = eq_and_loudness(Keep::File(dir.state())).await;
    control.bypass(true).await.unwrap();
    let text = std::fs::read_to_string(dir.state()).unwrap();
    assert!(text.contains("[bypass]"), "{text}");
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(
        state.bypass,
        Some(crate::eq::HeldStages {
            eq: true,
            loudness: true,
            crossfeed: false,
            speakers: false
        })
    );
    let (again, _, _) = rig(Keep::File(dir.state()));
    let view = again.view().bypass.unwrap();
    assert!(view.on && view.held.eq && view.held.loudness);
    let mut again = again;
    again.bypass(false).await.unwrap();
    let text = std::fs::read_to_string(dir.state()).unwrap();
    assert!(!text.contains("[bypass]"), "absent while off: {text}");
    assert!(again.view().on);
}

/// The state file with the EQ on and the bypass held, as the controller of another process
/// would have saved it.
fn state_with(on: bool, loud: bool, held: crate::eq::HeldStages) -> HostEqState {
    HostEqState {
        on,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: -2.0,
        filters: vec![pk(100.0, 2.0, 1.0)],
        slots: vec![1],
        crossfeed: None,
        crossfeed_on: false,
        crossfeed_loaded: None,
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        loudness: Some(crate::eq::LoudnessSettings {
            on: loud,
            strength_pct: 70.0,
            reference_pct: 90.0,
        }),
        bypass: Some(held),
    }
}

#[tokio::test]
async fn a_saved_bypass_with_a_stage_on_starts_not_bypassed() {
    let dir = Dir::new();
    let eq_held = crate::eq::HeldStages {
        eq: true,
        ..Default::default()
    };
    state_with(false, false, eq_held)
        .save(&dir.state())
        .unwrap();
    let (control, _, _) = rig(Keep::File(dir.state()));
    assert!(bypass_on(&control), "nothing plays: the bypass holds");
    state_with(false, true, eq_held).save(&dir.state()).unwrap();
    let (control, _, _) = rig(Keep::File(dir.state()));
    assert!(!bypass_on(&control), "loudness on ends it at start");
    assert!(held(&control).is_empty());
}

#[tokio::test]
async fn a_state_changed_from_outside_that_turns_a_stage_on_ends_the_bypass() {
    let dir = Dir::new();
    let (mut control, _, _) = eq_and_loudness(Keep::File(dir.state())).await;
    control.bypass(true).await.unwrap();
    assert!(bypass_on(&control));
    // another process turns the EQ on
    let held = crate::eq::HeldStages {
        loudness: true,
        ..Default::default()
    };
    state_with(true, false, held).save(&dir.state()).unwrap();
    assert!(control.reload_if_changed());
    assert!(!bypass_on(&control), "a stage plays: no bypass");
    assert!(!std::fs::read_to_string(dir.state()).unwrap().is_empty());
    control.edit(gain(0, 1.0, true)).await.unwrap();
    let text = std::fs::read_to_string(dir.state()).unwrap();
    assert!(
        !text.contains("[bypass]"),
        "the next save writes none: {text}"
    );
}

#[tokio::test]
async fn a_restore_or_an_all_off_says_what_the_recall_said() {
    let dir = Dir::new();
    let store = Arc::new(Store::new(&dir.0.join("speakers")));
    put_set(&store, "set-a", "Dummy head, A");
    let (control, _chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink()));
    params.with_speakers_ports(false);
    let mut control = control.with_speakers(store.clone());
    control
        .speakers(SpeakersEdit {
            on: None,
            set: Some("set-a".into()),
            done: true,
        })
        .await
        .unwrap();
    control.edit(gain(0, 3.0, true)).await.unwrap();
    control.bypass(true).await.unwrap();
    store.forget("set-a").unwrap();
    control.note = Some(crate::eq::LIVE_NOTE.to_string());
    control.bypass(false).await.unwrap();
    assert!(!bypass_on(&control));
    let note = control.view().note.expect("the recall's words are shown");
    assert!(
        note.contains(crate::eq::LIVE_NOTE),
        "the standing note stays: {note}"
    );
    assert!(
        note.contains("set-a") && note.contains("not fetched"),
        "{note}"
    );
}
