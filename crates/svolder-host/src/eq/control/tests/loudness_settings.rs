// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loudness's settings in the controller: changed, saved when final and shown. The stage that
//! follows the volume, and the chain it keeps, are tests/ld.rs's.

use super::*;
use crate::eq::loudness::{LoudnessChange, LoudnessSettings};

fn edit() -> LoudnessEdit {
    LoudnessEdit {
        done: true,
        ..LoudnessEdit::default()
    }
}

fn saved(dir: &Dir) -> Option<LoudnessSettings> {
    HostEqState::load(&dir.state()).unwrap().unwrap().loudness
}

/// A controller over a state file that says the EQ is on, so that there is a state to save to.
async fn over_file(dir: &Dir) -> (EqControl, FakeChain, FakeParams) {
    let state = HostEqState {
        on: true,
        name: "test".into(),
        origin: None,
        target: String::new(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(100.0, 1.0, 1.0)],
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on: false,
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
        bypass: None,
    };
    state.save(&dir.state()).unwrap();
    let chain = FakeChain::with_state(state);
    let params = FakeParams::new();
    let control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params.clone()),
        Keep::File(dir.state()),
    );
    (control, chain, params)
}

#[tokio::test]
async fn the_view_always_carries_the_loudness_with_the_defaults() {
    let (control, _chain, _params) = setup(Keep::Memory);
    let view = control.view().loudness.expect("a controller serves it");
    assert!(!view.on);
    assert_eq!((view.strength_pct, view.reference_pct), (50.0, 100.0));
    assert_eq!(view.volume_pct, None, "the volume is read later");
    assert!(!view.following);
    assert_eq!(view.note, None);
    assert_eq!(view.curve.hz.len(), 61);
}

#[test]
fn a_view_without_loudness_parses_and_one_without_it_leaves_the_key_out() {
    let (control, _chain, _params) = {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _guard = rt.enter();
        setup(Keep::Memory)
    };
    let mut view = control.view();
    view.loudness = None;
    let text = serde_json::to_string(&view).unwrap();
    let keys: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(keys.get("loudness").is_none(), "{text}");
    let back: EqView = serde_json::from_str(&text).unwrap();
    assert_eq!(back.loudness, None);
}

#[tokio::test]
async fn a_change_is_kept_snapped_and_shown_and_touches_no_chain() {
    let (mut control, chain, params) = setup(Keep::Memory);
    // Settings only, loudness off: turning it on applies a chain (tests/ld.rs).
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(37.0),
            reference_pct: Some(62.4),
            ..edit()
        })
        .await
        .unwrap();
    let view = control.view().loudness.unwrap();
    assert!(!view.on);
    assert_eq!(view.strength_pct, 35.0, "snapped to the step of 5");
    assert_eq!(view.reference_pct, 62.0, "snapped to the step of 1");
    assert!(!view.following, "nothing follows the volume while off");
    assert_eq!(view.note, None);
    assert_eq!(
        chain.calls(),
        Vec::new(),
        "no chain was applied, kept or removed"
    );
    assert_eq!(params.sent(), Vec::<Vec<(String, f64)>>::new());
    assert_eq!(params.forgotten(), 0, "the graph was not looked at");
}

#[tokio::test]
async fn only_what_is_named_changes_and_on_is_applied_last() {
    let (mut control, ..) = setup(Keep::Memory);
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(80.0),
            ..edit()
        })
        .await
        .unwrap();
    control
        .loudness(LoudnessEdit {
            on: Some(true),
            ..edit()
        })
        .await
        .unwrap();
    let view = control.view().loudness.unwrap();
    assert!(view.on);
    assert_eq!(view.strength_pct, 80.0, "the strength stayed");
    assert_eq!(view.reference_pct, 100.0);
    control
        .loudness(LoudnessEdit {
            on: Some(false),
            ..edit()
        })
        .await
        .unwrap();
    let view = control.view().loudness.unwrap();
    assert!(!view.on);
    assert_eq!(
        view.strength_pct, 80.0,
        "the numbers are kept while it is off"
    );
}

#[tokio::test]
async fn a_change_that_names_nothing_or_is_not_a_number_is_refused_in_words() {
    let (mut control, ..) = setup(Keep::Memory);
    let failure = control.loudness(edit()).await.unwrap_err();
    assert!(failure.what.contains("names nothing"), "{}", failure.what);
    let before = control.view().loudness.unwrap();
    let failure = control
        .loudness(LoudnessEdit {
            strength_pct: Some(f64::NAN),
            on: Some(true),
            ..edit()
        })
        .await
        .unwrap_err();
    let words = LoudnessSettings::default()
        .change(LoudnessChange {
            strength_pct: Some(f64::NAN),
            ..LoudnessChange::default()
        })
        .unwrap_err();
    assert_eq!(failure, words, "the controller's refusal is change's own");
    assert_eq!(control.view().loudness.unwrap(), before, "nothing changed");
}

#[tokio::test]
async fn reference_now_is_refused_while_the_volume_has_not_been_read() {
    let (mut control, ..) = setup(Keep::Memory);
    let failure = control
        .loudness(LoudnessEdit {
            reference_now: true,
            ..edit()
        })
        .await
        .unwrap_err();
    assert!(
        failure.what.contains("has not been read"),
        "{}",
        failure.what
    );
    assert_eq!(control.view().loudness.unwrap().reference_pct, 100.0);
}

#[tokio::test]
async fn a_final_change_is_saved_and_a_drag_is_not() {
    let dir = Dir::new();
    let (mut control, chain, params) = over_file(&dir).await;
    // A current chain: turning loudness on rebuilds nothing.
    params.with_loudness_ports();
    assert_eq!(saved(&dir), None, "nothing set yet: no [loudness]");
    let calls = chain.calls().len();
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(30.0),
            done: false,
            ..edit()
        })
        .await
        .unwrap();
    assert_eq!(saved(&dir), None, "a drag is heard, not saved");
    assert_eq!(control.view().loudness.unwrap().strength_pct, 30.0);
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(35.0),
            on: Some(true),
            ..edit()
        })
        .await
        .unwrap();
    assert_eq!(
        saved(&dir),
        Some(LoudnessSettings {
            on: true,
            strength_pct: 35.0,
            reference_pct: 100.0
        })
    );
    assert_eq!(
        chain.calls().len(),
        calls,
        "saving the file keeps no config"
    );
}

#[tokio::test]
async fn a_saved_loudness_is_adopted_with_the_state_and_survives_the_next_save() {
    let dir = Dir::new();
    {
        let (mut control, ..) = over_file(&dir).await;
        control
            .loudness(LoudnessEdit {
                on: Some(true),
                reference_pct: Some(70.0),
                ..edit()
            })
            .await
            .unwrap();
    }
    let (mut control, _chain, _params) = {
        let chain = FakeChain::with_state(HostEqState::load(&dir.state()).unwrap().unwrap());
        let params = FakeParams::new();
        let control = EqControl::new(
            Arc::new(chain.clone()),
            Box::new(params.clone()),
            Keep::File(dir.state()),
        );
        (control, chain, params)
    };
    let view = control.view().loudness.unwrap();
    assert!(view.on, "it stays on across a restart");
    assert_eq!(view.reference_pct, 70.0);
    // An edit of the points saves the state again: the loudness is written with it.
    control.edit(Edit::Add(pk(1000.0, 2.0, 1.0))).await.unwrap();
    assert_eq!(saved(&dir).map(|s| s.reference_pct), Some(70.0));
}

#[tokio::test]
async fn a_state_that_cannot_be_written_refuses_and_changes_nothing() {
    let dir = Dir::new();
    let (mut control, ..) = over_file(&dir).await;
    // The state's path becomes a directory: the write is refused.
    std::fs::remove_file(dir.state()).unwrap();
    std::fs::create_dir(dir.state()).unwrap();
    let before = control.view().loudness.unwrap();
    control
        .loudness(LoudnessEdit {
            on: Some(true),
            ..edit()
        })
        .await
        .unwrap_err();
    assert_eq!(control.view().loudness.unwrap(), before);
}

#[tokio::test]
async fn an_apply_keeps_the_loudness_the_state_holds() {
    let dir = Dir::new();
    let (mut control, ..) = over_file(&dir).await;
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(20.0),
            ..edit()
        })
        .await
        .unwrap();
    control.set_on(false).await.unwrap();
    control.set_on(true).await.unwrap();
    assert_eq!(saved(&dir).map(|s| s.strength_pct), Some(20.0));
    assert_eq!(control.view().loudness.unwrap().strength_pct, 20.0);
}

/// A `FakeParams` that has also read a volume at its last look.
struct WithLevel(FakeParams, crate::route::SinkLevel);

#[async_trait::async_trait]
impl crate::eq::Params for WithLevel {
    async fn node(&mut self) -> Option<LiveNode> {
        self.0.node().await
    }

    async fn send(&mut self, node: crate::eq::NodeKey, params: &[(String, f64)]) -> bool {
        self.0.send(node, params).await
    }

    fn level(&self) -> Option<crate::route::SinkLevel> {
        Some(self.1)
    }

    fn forget_node(&mut self) {
        self.0.forget_node();
    }
}

#[tokio::test]
async fn reference_now_uses_the_last_look_only_while_a_chain_runs() {
    let level = crate::route::SinkLevel {
        linear: 0.125,
        muted: false,
    };
    let now = LoudnessEdit {
        reference_now: true,
        ..edit()
    };
    // No chain runs: the level kept from an old look counts as unread.
    let mut idle = EqControl::new(
        Arc::new(FakeChain::new()),
        Box::new(WithLevel(FakeParams::new(), level)),
        Keep::Memory,
    );
    let failure = idle.loudness(now.clone()).await.unwrap_err();
    assert!(
        failure.what.contains("has not been read"),
        "{}",
        failure.what
    );
    // A chain runs: 0.125 on the cubic scale is 50 %.
    let dir = Dir::new();
    drop(over_file(&dir).await);
    let chain = FakeChain::with_state(HostEqState::load(&dir.state()).unwrap().unwrap());
    let mut control = EqControl::new(
        Arc::new(chain),
        Box::new(WithLevel(FakeParams::new(), level)),
        Keep::File(dir.state()),
    );
    control.loudness(now).await.unwrap();
    assert_eq!(control.view().loudness.unwrap().reference_pct, 50.0);
}
