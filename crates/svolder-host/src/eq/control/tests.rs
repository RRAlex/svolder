// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use svolder_profiles::tone::response::{correction_peak_db, correction_played_peak_db};

use super::*;
use crate::eq::live::LiveNode;
use crate::fake::{ChainCall, FakeChain, FakeParams};

/// A scratch directory, taken away with the test.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "svolder-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Dir(path)
    }

    fn state(&self) -> PathBuf {
        self.0.join("host-eq.toml")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pk(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::Peaking,
        f,
        g,
        q,
    }
}

fn setup(keep: Keep) -> (EqControl, FakeChain, FakeParams) {
    let chain = FakeChain::new();
    let params = FakeParams::new();
    let control = EqControl::new(Arc::new(chain.clone()), Box::new(params.clone()), keep);
    (control, chain, params)
}

fn gain(index: usize, g: f64, done: bool) -> Edit {
    Edit::Set {
        index,
        slot: None,
        kind: None,
        f: None,
        g: Some(g),
        q: None,
        done,
    }
}

fn touches(sent: &[(String, f64)], slot: u8) -> bool {
    let prefix = format!("l{slot:02}:");
    sent.iter().any(|(name, _)| name.starts_with(&prefix))
}

fn value(sent: &[(String, f64)], name: &str) -> Option<f64> {
    sent.iter().find(|(key, _)| key == name).map(|(_, v)| *v)
}

fn correction(filters: Vec<SourceFilter>, preamp_db: f64) -> Correction {
    Correction { preamp_db, filters }
}

fn saved(path: &Path, filters: Vec<SourceFilter>, slots: Vec<u8>, on: bool) {
    HostEqState {
        on,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters,
        slots,
    }
    .save(path)
    .unwrap();
}

#[tokio::test]
async fn the_first_eq_applies_the_kept_points() {
    let dir = Dir::new();
    saved(
        &dir.state(),
        vec![pk(100.0, 2.0, 1.0), pk(5000.0, -3.0, 2.0)],
        vec![1, 3],
        false,
    );
    let (mut control, chain, _) = setup(Keep::File(dir.state()));
    assert!(!control.view().on);
    control.set_on(true).await.unwrap();
    let calls = chain.calls();
    let [ChainCall::Apply(request)] = calls.as_slice() else {
        panic!("{:?}", chain.calls());
    };
    assert_eq!(request.correction.filters.len(), 2);
    assert_eq!(request.slots, vec![1, 3]);
    assert!(control.view().on);

    let (mut lone, chain, _) = setup(Keep::Memory);
    lone.set_on(true).await.unwrap();
    let calls = chain.calls();
    let [ChainCall::Apply(request)] = calls.as_slice() else {
        panic!("{:?}", chain.calls());
    };
    assert_eq!(request.correction.filters, vec![pk(1000.0, 0.0, 1.0)]);
}

#[tokio::test]
async fn off_removes_the_chain_and_keeps_the_points() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 4.0, true)).await.unwrap();
    control.set_on(false).await.unwrap();
    assert_eq!(chain.calls().last(), Some(&ChainCall::Remove));
    let view = control.view();
    assert!(!view.on);
    assert_eq!(view.points.len(), 1);
    assert_eq!(view.points[0].g, 4.0);
}

#[tokio::test]
async fn an_edit_while_off_turns_it_on_first() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.edit(gain(0, 3.0, true)).await.unwrap();
    assert!(matches!(chain.calls().first(), Some(ChainCall::Apply(_))));
    assert!(control.view().on);
    assert_eq!(params.sent().len(), 1);
}

#[tokio::test]
async fn a_drag_is_sent_live_at_most_twenty_times_a_second() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    let mut last = 0.0;
    for i in 0..60 {
        last = -6.0 + f64::from(i) * 0.2;
        control.edit(gain(0, last, false)).await.unwrap();
        if i == 1 {
            assert!(params.sent().is_empty(), "nothing is sent before a tick");
        }
        if i % 3 == 2 {
            control.tick().await;
        }
    }
    let sent = params.sent();
    let for_the_slot = sent.iter().filter(|send| touches(send, 1)).count();
    assert!((1..=20).contains(&for_the_slot), "{for_the_slot}");
    let newest = slot_params(1, Some(&pk(1000.0, last, 1.0)), 48_000);
    let b0 = &newest[0];
    assert_eq!(value(sent.last().unwrap(), &b0.0), Some(b0.1));
    assert_eq!(chain.calls().len(), 1, "a drag applies nothing");
}

#[tokio::test]
async fn the_release_sends_the_last_value_and_saves() {
    let dir = Dir::new();
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    control.set_on(true).await.unwrap();
    for g in [1.0, 2.0, 3.0] {
        control.edit(gain(0, g, false)).await.unwrap();
    }
    assert!(params.sent().is_empty());
    control.edit(gain(0, 4.5, true)).await.unwrap();
    let sent = params.sent();
    assert_eq!(sent.len(), 1);
    let want = slot_params(1, Some(&pk(1000.0, 4.5, 1.0)), 48_000);
    assert_eq!(value(&sent[0], &want[0].0), Some(want[0].1));
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(state.filters[0].g, 4.5);
    assert_eq!(state.slots, vec![1]);
    assert!(
        matches!(chain.calls().last(), Some(ChainCall::KeepConf(_))),
        "the config a restart reads is kept"
    );
}

#[tokio::test]
async fn no_update_is_saved_mid_drag() {
    let dir = Dir::new();
    let (mut control, _, _) = setup(Keep::File(dir.state()));
    control.edit(gain(0, 1.0, true)).await.unwrap();
    let before = std::fs::read_to_string(dir.state()).unwrap();
    for i in 0..30 {
        control
            .edit(gain(0, 1.0 + f64::from(i) * 0.1, false))
            .await
            .unwrap();
        control.tick().await;
    }
    assert_eq!(std::fs::read_to_string(dir.state()).unwrap(), before);
    control.edit(gain(0, 5.0, true)).await.unwrap();
    assert_ne!(std::fs::read_to_string(dir.state()).unwrap(), before);
}

#[tokio::test]
async fn the_preamp_keeps_every_live_update_under_unity() {
    let (mut control, _, params) = setup(Keep::Memory);
    control
        .load(
            "Loud".into(),
            None,
            correction(vec![pk(1000.0, 9.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    let mut checked = 0;
    for i in 0..40 {
        let g = -12.0 + f64::from(i) * 0.6;
        control.edit(gain(0, g, false)).await.unwrap();
        control.tick().await;
        let sent = params.sent();
        let last = sent.last().unwrap();
        let mult = value(last, "lpre:Mult").expect("the preamp goes with every update");
        let preamp_db = 20.0 * mult.log10();
        let peak = correction_peak_db(&correction(vec![pk(1000.0, g, 1.0)], 0.0)).max(0.0);
        assert!(
            preamp_db <= -peak + 1e-6,
            "{preamp_db} dB over a {peak} dB peak"
        );
        assert!(preamp_db <= 1e-9);
        checked += 1;
    }
    assert_eq!(checked, 40);
}

#[tokio::test]
async fn a_value_that_does_not_take_falls_back_to_apply_on_release_and_says_so_once() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.holds(false);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 3.0, false)).await.unwrap();
    control.tick().await;
    for _ in 0..2 {
        control.tick().await;
        assert!(
            control.view().live,
            "one miss is not the end of live updates"
        );
    }
    control.tick().await;
    let view = control.view();
    assert!(!view.live);
    assert_eq!(view.note.as_deref(), Some(LIVE_NOTE));
    let sends = params.sent().len();

    control.edit(gain(0, 4.0, false)).await.unwrap();
    control.tick().await;
    assert_eq!(params.sent().len(), sends, "a drag only moves the view");
    assert_eq!(control.view().points[0].g, 4.0);
    assert_eq!(control.view().note.as_deref(), Some(LIVE_NOTE));
    let applies = |chain: &FakeChain| {
        chain
            .calls()
            .iter()
            .filter(|call| matches!(call, ChainCall::Apply(_)))
            .count()
    };
    assert_eq!(applies(&chain), 1);
    control.edit(gain(0, 5.0, true)).await.unwrap();
    assert_eq!(applies(&chain), 2);
    assert_eq!(params.sent().len(), sends);
}

#[tokio::test]
async fn a_pw_cli_that_keeps_failing_is_unavailable_and_applies_on_release() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    params.takes(false);
    control.edit(gain(0, 3.0, false)).await.unwrap();
    for _ in 0..FAIL_LIMIT - 1 {
        control.tick().await;
    }
    assert_eq!(control.view().unavailable, None);
    control.tick().await;
    let view = control.view();
    assert!(!view.live);
    let why = view.unavailable.expect("it says so");
    assert!(why.starts_with("computer controls unavailable: "), "{why}");
    let before = chain.calls().len();
    control.edit(gain(0, 5.0, true)).await.unwrap();
    assert!(matches!(chain.calls()[before], ChainCall::Apply(_)));
    assert_eq!(
        control.view().unavailable,
        None,
        "an apply that works clears it"
    );
    assert!(control.view().live);
}

#[tokio::test]
async fn one_restart_of_pw_cli_is_ridden_out() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    params.takes(false);
    control.edit(gain(0, 3.0, false)).await.unwrap();
    for _ in 0..20 {
        control.tick().await;
    }
    params.takes(true);
    control.tick().await;
    assert_eq!(control.view().unavailable, None);
    assert!(control.view().live);
    assert!(!params.sent().is_empty());
}

#[tokio::test]
async fn a_type_change_is_live_or_rebuilt_per_the_shape() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    let change = Edit::Set {
        index: 0,
        slot: None,
        kind: Some(FilterKind::LowShelf),
        f: None,
        g: Some(3.0),
        q: None,
        done: false,
    };
    control.edit(change).await.unwrap();
    control.tick().await;
    let applies = |chain: &FakeChain| {
        chain
            .calls()
            .iter()
            .filter(|call| matches!(call, ChainCall::Apply(_)))
            .count()
    };
    assert_eq!(control.view().points[0].kind, "LSC");
    match SHAPE {
        SlotShape::Raw => {
            assert_eq!(params.sent().len(), 1, "live, in the slot");
            let shelf = slot_params(
                1,
                Some(&SourceFilter {
                    kind: FilterKind::LowShelf,
                    f: 1000.0,
                    g: 3.0,
                    q: 1.0,
                }),
                48_000,
            );
            assert_eq!(value(&params.sent()[0], "l01:b0"), Some(shelf[0].1));
            assert_eq!(applies(&chain), 1);
        }
        SlotShape::Typed => {
            assert!(params.sent().is_empty());
            assert_eq!(applies(&chain), 1);
            control.edit(gain(0, 3.0, true)).await.unwrap();
            assert_eq!(applies(&chain), 2, "rebuilt when the drag ends");
        }
    }
}

#[tokio::test]
async fn a_hand_edit_makes_the_preset_custom() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control
        .load(
            "Warm".into(),
            Some("test".into()),
            correction(vec![pk(100.0, 3.0, 1.0), pk(3000.0, -2.0, 1.0)], -4.0),
        )
        .await
        .unwrap();
    assert_eq!(control.view().preset.as_deref(), Some("Warm"));
    control.edit(gain(1, -1.0, true)).await.unwrap();
    assert_eq!(control.view().preset.as_deref(), Some(CUSTOM));
    let Some(ChainCall::KeepConf(request)) = chain.calls().last().cloned() else {
        panic!("{:?}", chain.calls());
    };
    assert_eq!(request.name, CUSTOM);
    assert_eq!(request.correction.preamp_db, 0.0);
    assert_eq!(request.origin, None);
}

#[tokio::test]
async fn seventeen_filters_are_refused() {
    let (mut control, chain, params) = setup(Keep::Memory);
    let many = (0..17)
        .map(|i| pk(100.0 + f64::from(i) * 100.0, 1.0, 1.0))
        .collect();
    let refused = control
        .load("Big".into(), None, correction(many, 0.0))
        .await
        .unwrap_err();
    assert!(
        refused
            .what
            .contains("17 filters; the computer EQ holds 16"),
        "{}",
        refused.what
    );
    assert!(chain.calls().is_empty());
    assert!(params.sent().is_empty());
    assert_eq!(control.view().points.len(), 1);
    assert_eq!(control.view().preset, None);

    let sixteen = (0..16)
        .map(|i| pk(100.0 + f64::from(i) * 100.0, 1.0, 1.0))
        .collect();
    control
        .load("Fits".into(), None, correction(sixteen, 0.0))
        .await
        .unwrap();
    assert_eq!(control.view().points.len(), 16);
    let seventeenth = control.edit(Edit::Add(pk(5000.0, 0.0, 1.0))).await;
    assert!(seventeenth.is_err());
    assert_eq!(control.view().points.len(), 16);
}

#[tokio::test]
async fn editing_one_field_leaves_the_others_as_written() {
    let (mut control, _, _) = setup(Keep::Memory);
    let shelf = SourceFilter {
        kind: FilterKind::LowShelf,
        f: 90.0,
        g: 14.0,
        q: 15.0,
    };
    control
        .load("Bassy".into(), None, correction(vec![shelf], -14.0))
        .await
        .unwrap();
    let edit = Edit::Set {
        index: 0,
        slot: None,
        kind: None,
        f: Some(200.0),
        g: None,
        q: None,
        done: true,
    };
    control.edit(edit).await.unwrap();
    let point = &control.view().points[0];
    assert_eq!(
        (point.kind.as_str(), point.f, point.g, point.q),
        ("LSC", 200.0, 14.0, 15.0)
    );
    control.edit(gain(0, 20.0, true)).await.unwrap();
    let point = &control.view().points[0];
    assert_eq!((point.f, point.g, point.q), (200.0, 12.0, 15.0));
}

#[tokio::test]
async fn a_state_file_changed_by_the_cli_is_read_again() {
    let dir = Dir::new();
    let (mut control, _, _) = setup(Keep::File(dir.state()));
    assert!(!control.reload_if_changed());
    control.edit(gain(0, 2.0, true)).await.unwrap();
    assert!(
        !control.reload_if_changed(),
        "what the controller saved is not news"
    );
    saved(
        &dir.state(),
        vec![pk(200.0, 1.0, 1.0), pk(800.0, -2.0, 1.0)],
        vec![2, 5],
        true,
    );
    assert!(control.reload_if_changed());
    let view = control.view();
    assert_eq!(view.points.len(), 2);
    assert_eq!(view.points[1].f, 800.0);
    assert_eq!(view.preset.as_deref(), Some("Saved"));
    assert!(!control.reload_if_changed());

    control.edit(gain(1, -3.0, false)).await.unwrap();
    saved(&dir.state(), vec![pk(900.0, 0.0, 1.0)], vec![1], true);
    assert!(
        !control.reload_if_changed(),
        "a drag with values to send is not overwritten"
    );
    control.tick().await;
    assert!(control.reload_if_changed());
    assert_eq!(control.view().points.len(), 1);
}

#[tokio::test]
async fn memory_keeping_writes_no_file() {
    let dir = Dir::new();
    let other = Dir::new();
    let (mut memory, _, _) = setup(Keep::Memory);
    let (mut file, _, _) = setup(Keep::File(other.state()));
    for control in [&mut memory, &mut file] {
        control.edit(gain(0, 2.0, true)).await.unwrap();
        control.edit(Edit::Add(pk(4000.0, 1.0, 1.0))).await.unwrap();
        control.set_on(false).await.unwrap();
    }
    assert!(other.state().exists());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&other.0).unwrap().count(), 1);
}

#[tokio::test]
async fn a_new_point_and_a_removed_one_are_live_in_their_slots() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(Edit::Add(pk(4000.0, 3.0, 1.0))).await.unwrap();
    let sent = params.sent();
    let new_point = slot_params(2, Some(&pk(4000.0, 3.0, 1.0)), 48_000);
    assert_eq!(value(&sent[0], "l02:b0"), Some(new_point[0].1));
    assert_eq!(
        value(&sent[0], "l01:b0"),
        Some(slot_params(1, Some(&pk(1000.0, 0.0, 1.0)), 48_000)[0].1),
        "the slot that did not move goes with it, as it is"
    );
    control
        .edit(Edit::Remove {
            index: 0,
            slot: None,
        })
        .await
        .unwrap();
    let sent = params.sent();
    assert!(touches(&sent[1], 1), "the freed slot goes transparent");
    assert_eq!(value(&sent[1], "l01:b0"), Some(1.0));
    assert_eq!(control.view().points.len(), 1);
    control
        .edit(gain(0, 3.0, true))
        .await
        .expect("the lone point is set");
    control
        .edit(Edit::Remove {
            index: 0,
            slot: None,
        })
        .await
        .expect("removing the lone point flattens it");
    let view = control.view();
    assert_eq!(view.points.len(), 1, "point 1 stays while alone");
    assert_eq!(view.points[0].g, 0.0, "and goes flat");
    assert_eq!(
        chain
            .calls()
            .iter()
            .filter(|call| matches!(call, ChainCall::Apply(_)))
            .count(),
        1
    );
}

#[tokio::test]
async fn removing_the_lone_point_keeps_its_frequency_type_and_q() {
    let (mut control, _, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control
        .edit(Edit::Set {
            index: 0,
            slot: None,
            kind: Some(FilterKind::LowShelf),
            f: Some(250.0),
            g: Some(4.0),
            q: Some(0.7),
            done: true,
        })
        .await
        .unwrap();
    control
        .edit(Edit::Remove {
            index: 0,
            slot: None,
        })
        .await
        .unwrap();
    let point = control.view().points[0].clone();
    assert_eq!(
        (point.kind.as_str(), point.f, point.g, point.q),
        ("LSC", 250.0, 0.0, 0.7)
    );
    control
        .edit(Edit::Remove {
            index: 0,
            slot: None,
        })
        .await
        .expect("a flat lone point takes the remove too, and stays");
    assert_eq!(control.view().points[0].g, 0.0);
}

fn frequencies(control: &EqControl) -> Vec<f64> {
    control.view().points.iter().map(|point| point.f).collect()
}

#[tokio::test]
async fn the_points_sort_by_frequency_when_an_edit_ends_and_never_during_a_drag() {
    let (mut control, _, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control
        .edit(Edit::Set {
            index: 0,
            slot: None,
            kind: None,
            f: Some(8000.0),
            g: None,
            q: None,
            done: true,
        })
        .await
        .unwrap();
    control.edit(Edit::Add(pk(226.0, 1.0, 1.0))).await.unwrap();
    control.edit(Edit::Add(pk(2000.0, 1.0, 1.0))).await.unwrap();
    assert_eq!(frequencies(&control), vec![226.0, 2000.0, 8000.0]);
    // A drag of point 1 past point 2 leaves the order alone until it ends.
    let drag = |f, done| Edit::Set {
        index: 0,
        slot: None,
        kind: None,
        f: Some(f),
        g: None,
        q: None,
        done,
    };
    control.edit(drag(5000.0, false)).await.unwrap();
    assert_eq!(frequencies(&control), vec![5000.0, 2000.0, 8000.0]);
    control.edit(drag(5000.0, true)).await.unwrap();
    assert_eq!(frequencies(&control), vec![2000.0, 5000.0, 8000.0]);
}

#[tokio::test]
async fn sorting_moves_no_slot_and_a_tie_keeps_its_order() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(Edit::Add(pk(100.0, 1.0, 1.0))).await.unwrap();
    // Point 1 (slot 1, 1 kHz) is now second; slot 2 holds the 100 Hz point.
    let sent = params.sent();
    let first = slot_params(1, Some(&pk(1000.0, 0.0, 1.0)), 48_000);
    let second = slot_params(2, Some(&pk(100.0, 1.0, 1.0)), 48_000);
    assert_eq!(value(sent.last().unwrap(), "l02:b0"), Some(second[0].1));
    assert_eq!(value(sent.last().unwrap(), "l01:b0"), Some(first[0].1));
    control.edit(Edit::Add(pk(1000.0, 2.0, 1.0))).await.unwrap();
    let gains: Vec<f64> = control.view().points.iter().map(|p| p.g).collect();
    assert_eq!(
        gains,
        vec![1.0, 0.0, 2.0],
        "the tie at 1 kHz keeps its order"
    );
}

fn node(id: u32, rate_hz: Option<u32>) -> LiveNode {
    LiveNode {
        id,
        rate_hz,
        params: crate::eq::live::preamp_params(0.0)
            .into_iter()
            .map(|(name, _)| (name, 0.0))
            .collect(),
    }
}

#[tokio::test]
async fn a_new_rate_or_a_restarted_chain_sends_every_slot_again() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.tick().await;
    assert!(params.sent().is_empty(), "looking sends nothing");

    params.set_node(Some(node(1, Some(44_100))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let sent = params.sent();
    assert_eq!(sent.len(), 1);
    assert!((1..=16).all(|slot| touches(&sent[0], slot)));
    let at_44k = slot_params(1, Some(&pk(1000.0, 0.0, 1.0)), 44_100);
    assert_eq!(value(&sent[0], "l01:b0"), Some(at_44k[0].1));

    params.set_node(Some(node(1, None)));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert_eq!(params.sent().len(), 1, "a suspended sink is not a change");

    params.set_node(Some(node(7, None)));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert_eq!(params.sent().len(), 2, "a new node is");
    assert!(params.forgotten() > 2);
}

#[tokio::test]
async fn one_read_back_miss_does_not_turn_live_off() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    params.holds(false);
    control.edit(gain(0, 3.0, false)).await.unwrap();
    control.tick().await;
    control.tick().await;
    params.holds(true);
    control.edit(gain(0, 4.0, false)).await.unwrap();
    for _ in 0..6 {
        control.tick().await;
    }
    assert!(control.view().live);
    assert_eq!(control.view().note, None);
}

#[tokio::test]
async fn a_state_that_says_on_with_no_chain_applies_when_asked_to_turn_on() {
    let dir = Dir::new();
    saved(&dir.state(), vec![pk(100.0, 2.0, 1.0)], vec![4], true);
    let (mut control, chain, _) = setup(Keep::File(dir.state()));
    assert!(control.view().on);
    assert!(chain.calls().is_empty(), "starting writes nothing");
    control.tick().await;
    assert!(chain.calls().is_empty(), "a tick writes nothing");
    control.set_on(true).await.unwrap();
    let calls = chain.calls();
    let [ChainCall::Apply(request)] = calls.as_slice() else {
        panic!("{calls:?}");
    };
    assert_eq!(request.slots, vec![4]);
    control.set_on(true).await.unwrap();
    assert_eq!(chain.calls().len(), 1, "a chain that runs is left alone");
}

#[tokio::test]
async fn reapply_if_on_applies_only_when_the_file_says_on() {
    let on = Dir::new();
    saved(&on.state(), vec![pk(100.0, 2.0, 1.0)], vec![1], true);
    let (mut control, chain, _) = setup(Keep::File(on.state()));
    assert_eq!(control.reapply_if_on().await, Ok(true));
    assert_eq!(chain.calls().len(), 1);
    assert_eq!(control.reapply_if_on().await, Ok(false), "it runs now");

    let off = Dir::new();
    saved(&off.state(), vec![pk(100.0, 2.0, 1.0)], vec![1], false);
    let (mut control, chain, _) = setup(Keep::File(off.state()));
    assert_eq!(control.reapply_if_on().await, Ok(false));
    let (mut fresh, chain_fresh, _) = setup(Keep::Memory);
    assert_eq!(fresh.reapply_if_on().await, Ok(false));
    assert!(chain.calls().is_empty() && chain_fresh.calls().is_empty());
}

#[tokio::test]
async fn off_saves_the_points_as_they_stand() {
    let dir = Dir::new();
    let (mut control, _, _) = setup(Keep::File(dir.state()));
    control.edit(gain(0, 2.0, true)).await.unwrap();
    control.edit(gain(0, 5.0, false)).await.unwrap();
    control.set_on(false).await.unwrap();
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(state.filters[0].g, 5.0);
    assert!(!state.on);
    assert!(!control.reload_if_changed(), "its own save is not news");
}

#[tokio::test]
async fn a_presets_stated_preamp_is_sent_live_until_the_first_hand_edit() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control
        .load(
            "Quiet".into(),
            None,
            correction(vec![pk(1000.0, 1.0, 1.0)], -4.0),
        )
        .await
        .unwrap();
    let sent = params.sent();
    let mult = value(sent.last().unwrap(), "lpre:Mult").unwrap();
    assert!((20.0 * mult.log10() + 4.0).abs() < 1e-9);
    assert_eq!(control.view().preamp_db, -4.0);
    control.edit(gain(0, 1.0, true)).await.unwrap();
    let mult = value(params.sent().last().unwrap(), "lpre:Mult").unwrap();
    assert!(
        20.0 * mult.log10() > -1.2,
        "the hand edit states 0: only the peak lowers it"
    );
}

#[tokio::test]
async fn a_chain_turned_on_is_written_at_the_rate_the_sink_runs_at() {
    for rate in [44_100, 96_000] {
        let (mut control, chain, params) = setup(Keep::Memory);
        params.set_node(Some(node(1, Some(rate))));
        control.set_on(true).await.unwrap();
        control.set_on(false).await.unwrap();
        control.set_on(true).await.unwrap();
        let applies: Vec<_> = chain
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                ChainCall::Apply(request) => Some(request.rate_hz),
                _ => None,
            })
            .collect();
        assert_eq!(applies, [Some(rate), Some(rate)]);
        for _ in 0..WATCH_EVERY {
            control.tick().await;
        }
        assert!(
            params.sent().is_empty(),
            "no wrong-rate window to put right"
        );
    }
}

#[tokio::test]
async fn a_rate_that_cannot_be_read_is_written_at_the_default_and_sent_again() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_node(Some(node(1, None)));
    control.set_on(true).await.unwrap();
    assert!(matches!(
        chain.calls().last(),
        Some(ChainCall::Apply(request)) if request.rate_hz.is_none()
    ));
    params.set_node(Some(node(1, Some(44_100))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let sent = params.sent();
    assert_eq!(
        sent.len(),
        1,
        "the first look sends every slot at the real rate"
    );
    assert!((1..=16).all(|slot| touches(&sent[0], slot)));
}

#[tokio::test]
async fn readers_get_the_newest_view_without_the_controller() {
    let (mut control, _, _) = setup(Keep::Memory);
    let watcher = control.watcher();
    assert!(!watcher.borrow().on);
    control.edit(gain(0, 3.0, true)).await.unwrap();
    let seen = watcher.borrow().clone();
    assert!(seen.on);
    assert_eq!(seen.points[0].g, 3.0);
    assert_eq!(seen, control.view());
}

/// A controller whose saved state says on over a chain that holds that state and cannot tell
/// whether it runs.
async fn unknown_status() -> (EqControl, FakeChain, Dir) {
    let dir = Dir::new();
    saved(&dir.state(), vec![pk(100.0, 2.0, 1.0)], vec![1], true);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    let chain = FakeChain::with_state(state);
    chain.cannot_tell(true);
    let mut control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(FakeParams::new()),
        Keep::File(dir.state()),
    );
    assert!(!control.sync_running().await, "unknown is not off");
    (control, chain, dir)
}

#[tokio::test]
async fn an_unknown_status_is_not_off_and_the_view_says_so() {
    let (mut control, chain, dir) = unknown_status().await;
    let view = control.view();
    assert!(view.on && view.running_unknown);
    control.edit(gain(0, 3.0, true)).await.unwrap();
    assert!(
        HostEqState::load(&dir.state()).unwrap().unwrap().on,
        "an edit never saves off for a chain that may run"
    );
    assert!(chain.calls().iter().all(|call| *call != ChainCall::Remove));
}

#[tokio::test]
async fn an_unknown_status_followed_by_off_removes_the_chain() {
    let (mut control, chain, dir) = unknown_status().await;
    control.set_on(false).await.unwrap();
    assert!(chain.calls().contains(&ChainCall::Remove));
    assert!(!control.view().on && !control.view().running_unknown);
    assert!(!HostEqState::load(&dir.state()).unwrap().unwrap().on);
}

#[tokio::test]
async fn an_unknown_status_followed_by_on_ends_with_exactly_one_chain() {
    let (mut control, chain, _dir) = unknown_status().await;
    control.set_on(true).await.unwrap();
    let calls: Vec<&'static str> = chain
        .calls()
        .iter()
        .map(|call| match call {
            ChainCall::Remove => "remove",
            ChainCall::Apply(_) => "apply",
            ChainCall::KeepConf(_) => "keep",
        })
        .collect();
    assert_eq!(
        calls,
        ["remove", "apply"],
        "the old one goes before the new one comes"
    );
    assert!(!control.view().running_unknown);
    assert_eq!(control.reapply_if_on().await, Ok(false));
}

/// The preamp a send carries, in dB.
fn preamp_of(sent: &[(String, f64)]) -> f64 {
    20.0 * value(sent, "lpre:Mult")
        .expect("every send carries the preamp")
        .log10()
}

/// What the sent filters of a point play: its held values at the rate the chain runs at.
fn played_peak(control: &EqControl) -> f64 {
    let correction = control.points.correction(0.0);
    let played = crate::eq::live::played(&correction, 48_000);
    correction_played_peak_db(&played, 48_000).max(0.0)
}

#[tokio::test]
async fn an_added_point_with_a_zero_q_or_frequency_never_plays_above_unity() {
    for filter in [
        pk(1000.0, 12.0, 0.0),
        pk(0.0, 12.0, 0.01),
        pk(-5.0, 40.0, -3.0),
        pk(f64::MAX, 12.0, 1.0),
        pk(1000.0, 12.0, 1.0e-12),
    ] {
        let (mut control, _, params) = setup(Keep::Memory);
        control.set_on(true).await.unwrap();
        control.edit(Edit::Add(filter.clone())).await.unwrap();
        control.tick().await;
        let sent = params.sent();
        assert!(!sent.is_empty(), "{filter:?}");
        for send in &sent {
            assert!(
                preamp_of(send) <= -played_peak(&control) + 1e-6,
                "{filter:?}: {} dB over a {} dB peak",
                preamp_of(send),
                played_peak(&control)
            );
        }
        let view = control.view();
        assert!(view.preamp_db <= -played_peak(&control) + 1e-6);
    }
}

#[tokio::test]
async fn a_huge_gain_is_held_and_nothing_panics() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control
        .edit(Edit::Add(pk(1000.0, 1.0e5, 1.0)))
        .await
        .unwrap();
    control.edit(gain(0, f64::MAX, false)).await.unwrap();
    control.tick().await;
    for send in params.sent() {
        assert!(send.iter().all(|(_, value)| value.is_finite()));
    }
    for g in control.view().points.iter().map(|point| point.g) {
        assert!(g.abs() <= 12.0, "{g}");
    }
}

#[tokio::test]
async fn points_kept_outside_every_range_still_send_finite_values_under_unity() {
    let dir = Dir::new();
    saved(
        &dir.state(),
        vec![pk(15.0, 1.0e5, 0.0), pk(f64::INFINITY, -1.0e5, f64::MAX)],
        vec![1, 2],
        true,
    );
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    control.edit(gain(1, 3.0, true)).await.unwrap();
    control.tick().await;
    let sent = params.sent();
    assert!(!sent.is_empty());
    for send in &sent {
        assert!(send.iter().all(|(_, value)| value.is_finite()));
        assert!(preamp_of(send) <= -played_peak(&control) + 1e-6);
    }
}

#[tokio::test]
async fn a_correction_outside_the_ranges_is_refused_and_changes_nothing() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    let before = control.view();
    for bad in [
        correction(vec![pk(1000.0, 12.0, 0.0)], 0.0),
        correction(vec![pk(0.0, 12.0, 1.0)], 0.0),
        correction(vec![pk(1000.0, 1.0e5, 1.0)], 0.0),
        correction(vec![pk(1000.0, f64::NAN, 1.0)], 0.0),
        correction(vec![pk(1000.0, 1.0, 1.0)], f64::INFINITY),
    ] {
        let refused = control.load("Bad".into(), None, bad.clone()).await;
        assert!(refused.is_err(), "{bad:?}");
    }
    assert_eq!(control.view(), before);
    assert_eq!(chain.calls().len(), 1, "nothing was applied");
    assert!(params.sent().is_empty());
}

#[tokio::test]
async fn the_first_send_after_an_apply_names_the_new_node() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 2.0, true)).await.unwrap();
    assert!(params.lines()[0].starts_with("set-param 1 "));
    control.set_on(false).await.unwrap();
    params.set_node(Some(node(41, Some(48_000))));
    control.edit(Edit::Add(pk(3000.0, 2.0, 1.0))).await.unwrap();
    let lines = params.lines();
    assert!(lines.len() >= 2);
    assert!(
        lines[1..]
            .iter()
            .all(|line| line.starts_with("set-param 41 ")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn a_falling_preamp_goes_first_and_a_rising_one_last() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 3.0, true)).await.unwrap();
    control.edit(gain(0, 9.0, true)).await.unwrap();
    let falling = params.sent().last().unwrap().clone();
    assert_eq!(falling[0].0, "lpre:Mult", "the cut comes before the boost");
    control.edit(gain(0, 1.0, true)).await.unwrap();
    let rising = params.sent().last().unwrap().clone();
    let names: Vec<&str> = rising.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(&names[names.len() - 2..], ["lpre:Mult", "rpre:Mult"]);
}

/// Three points, in slots 1, 2 and 3 by frequency order 200, 1000, 5000.
async fn three_points() -> (EqControl, FakeParams) {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(Edit::Add(pk(200.0, 1.0, 1.0))).await.unwrap();
    control.edit(Edit::Add(pk(5000.0, 2.0, 1.0))).await.unwrap();
    (control, params)
}

fn slot_gain(index: usize, slot: u8, g: f64) -> Edit {
    Edit::Set {
        index,
        slot: Some(slot),
        kind: None,
        f: None,
        g: Some(g),
        q: None,
        done: true,
    }
}

#[tokio::test]
async fn a_slot_names_the_point_whatever_place_the_table_lists_it_in() {
    let (mut control, _) = three_points().await;
    let view = control.view();
    let slots: Vec<u8> = view.points.iter().map(|point| point.slot).collect();
    assert_eq!(slots, vec![2, 1, 3], "sorted by frequency: 200, 1000, 5000");
    // A sender that read the table before the sort holds slot 1 at index 0: the slot wins.
    control.edit(slot_gain(0, 1, 7.0)).await.unwrap();
    let after = control.view();
    assert_eq!(after.points[1].slot, 1);
    assert_eq!(after.points[1].g, 7.0);
    assert_eq!(
        after.points[0].g, 1.0,
        "the point at the stale index is unchanged"
    );
    assert_eq!(after.points[2].g, 2.0);
}

#[tokio::test]
async fn a_slot_nobody_holds_is_refused_and_nothing_changes() {
    let (mut control, params) = three_points().await;
    let before = control.view();
    let sent = params.sent().len();
    for slot in [4, 0, 17] {
        let refused = control.edit(slot_gain(0, slot, 5.0)).await.unwrap_err();
        // The web page (`core/actions.js`, `movedPoint`) reads this sentence to tell a point
        // that moved from any other 409: change it there too.
        assert_eq!(
            refused.what,
            format!("the point in slot {slot} has moved or is gone"),
        );
    }
    let refused = control
        .edit(Edit::Remove {
            index: 0,
            slot: Some(9),
        })
        .await;
    assert!(refused.is_err());
    assert_eq!(control.view(), before);
    assert_eq!(params.sent().len(), sent, "nothing was sent");
}

#[tokio::test]
async fn a_remove_by_slot_removes_that_point_and_an_index_only_request_still_works() {
    let (mut control, _) = three_points().await;
    control
        .edit(Edit::Remove {
            index: 0,
            slot: Some(1),
        })
        .await
        .unwrap();
    let slots: Vec<u8> = control.view().points.iter().map(|p| p.slot).collect();
    assert_eq!(slots, vec![2, 3]);
    control.edit(gain(1, -3.0, true)).await.unwrap();
    let view = control.view();
    assert_eq!(view.points[1].g, -3.0);
    assert_eq!(view.points[0].g, 1.0);
}

#[tokio::test]
async fn a_refused_edit_while_off_changes_nothing_and_turns_nothing_on() {
    let (mut control, chain, params) = setup(Keep::Memory);
    let before = control.view();
    for edit in [
        slot_gain(0, 9, 5.0),
        Edit::Remove {
            index: 0,
            slot: Some(9),
        },
        gain(7, 5.0, true),
        gain(0, f64::NAN, true),
    ] {
        assert!(control.edit(edit).await.is_err());
    }
    assert!(!control.view().on);
    assert_eq!(control.view(), before);
    assert!(chain.calls().is_empty(), "{:?}", chain.calls());
    assert!(params.sent().is_empty());
}

#[tokio::test]
async fn a_reload_forgets_the_node_so_the_next_send_names_the_new_one() {
    let dir = Dir::new();
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    control.edit(gain(0, 2.0, true)).await.unwrap();
    assert!(params.lines()[0].starts_with("set-param 1 "));
    // Another process re-applied the chain: a new node, and a new state file.
    params.set_node(Some(node(41, Some(48_000))));
    saved(&dir.state(), vec![pk(300.0, 1.0, 1.0)], vec![1], true);
    assert!(control.reload_if_changed());
    control.edit(gain(0, 3.0, true)).await.unwrap();
    let lines = params.lines();
    assert!(
        lines[1..]
            .iter()
            .all(|line| line.starts_with("set-param 41 ")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn a_softer_edit_right_after_an_apply_keeps_the_applied_preamp() {
    let dir = Dir::new();
    saved(&dir.state(), vec![pk(1000.0, 9.0, 1.0)], vec![1], false);
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    control.set_on(true).await.unwrap();
    let applied = control.view().preamp_db;
    control.edit(gain(0, 1.0, true)).await.unwrap();
    let sent = params.sent().last().unwrap().clone();
    assert!(
        (preamp_of(&sent) - applied).abs() < 1e-6,
        "{} dB sent, {applied} dB applied",
        preamp_of(&sent)
    );
}

fn deep_shelf() -> SourceFilter {
    SourceFilter {
        kind: FilterKind::LowShelf,
        f: 20.0,
        g: 12.0,
        q: 10.0,
    }
}

/// An Add that is going to be refused is refused before the EQ turns on, and so is a curve the
/// chain cannot play (a preamp below the floor).
#[tokio::test]
async fn an_add_that_will_be_refused_is_refused_before_the_eq_turns_on() {
    let dir = Dir::new();
    let full: Vec<SourceFilter> = (1..=16)
        .map(|i| pk(100.0 * f64::from(i), 0.0, 1.0))
        .collect();
    saved(&dir.state(), full, (1..=16).collect(), false);
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    let before = control.view();
    assert!(control.edit(Edit::Add(pk(50.0, 1.0, 1.0))).await.is_err());
    assert!(
        control
            .edit(Edit::Add(pk(f64::NAN, 1.0, 1.0)))
            .await
            .is_err()
    );
    assert!(!control.view().on);
    assert_eq!(control.view(), before);
    assert!(chain.calls().is_empty(), "{:?}", chain.calls());
    assert!(params.sent().is_empty());
}

#[tokio::test]
async fn an_edit_that_needs_a_preamp_below_the_floor_is_refused_and_changes_nothing() {
    let dir = Dir::new();
    let four = vec![deep_shelf(); 4];
    saved(&dir.state(), four, vec![1, 2, 3, 4], false);
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    let before = control.view();
    let failure = control.edit(Edit::Add(deep_shelf())).await.unwrap_err();
    assert!(
        failure
            .what
            .contains("the computer EQ plays curves up to 120 dB"),
        "{}",
        failure.what
    );
    assert!(!control.view().on);
    assert_eq!(control.view(), before);
    assert!(chain.calls().is_empty(), "{:?}", chain.calls());
    // The same while on: nothing is sent and the points stay.
    control.set_on(true).await.unwrap();
    let applied = chain.calls().len();
    let sent = params.sent().len();
    let on = control.view();
    assert!(control.edit(Edit::Add(deep_shelf())).await.is_err());
    assert_eq!(control.view(), on);
    assert_eq!(chain.calls().len(), applied);
    assert_eq!(params.sent().len(), sent);
    // A preset that needs it is refused too.
    let failure = control
        .load("Deep".into(), None, correction(vec![deep_shelf(); 5], 0.0))
        .await
        .unwrap_err();
    assert!(failure.what.contains("120 dB"), "{}", failure.what);
    assert_eq!(control.view(), on);
}

fn hsc(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::HighShelf,
        f,
        g,
        q,
    }
}

/// No send in `params` may carry a coefficient past what the chain holds.
fn nothing_past_ten(params: &FakeParams) {
    for line in params.sent() {
        for (name, value) in line {
            if name.contains(":b") || name.contains(":a") {
                assert!(value.abs() <= 10.0, "{name} = {value}");
            }
        }
    }
}

/// The high shelf at 1 kHz of +14.5 dB is playable at 48 kHz and its b1 passes -10 at 192 kHz,
/// where the chain would clamp it to another filter.
#[tokio::test]
async fn a_rate_change_that_makes_the_curve_unplayable_turns_the_eq_off() {
    let dir = Dir::new();
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    control
        .load(
            "Shelf".into(),
            None,
            correction(vec![hsc(1000.0, 14.5, 0.707)], 0.0),
        )
        .await
        .unwrap();
    assert!(control.view().on);
    let sends = params.sent().len();

    params.set_node(Some(node(1, Some(192_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert_eq!(
        params.sent().len(),
        sends,
        "nothing is sent at the new rate"
    );
    nothing_past_ten(&params);
    assert_eq!(chain.calls().last(), Some(&ChainCall::Remove));
    let view = control.view();
    assert!(!view.on);
    assert_eq!(view.points.len(), 1, "the points are kept");
    assert_eq!(view.points[0].g, 14.5);
    let note = view.note.expect("a note says why");
    assert!(
        note.contains("point 1") && note.contains("192000 Hz"),
        "{note}"
    );
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(!state.on, "the state says off");
    assert_eq!(state.filters.len(), 1);
    assert!(
        state.preamp_db > -20.0,
        "the saved preamp is a curve's, not the floor: {}",
        state.preamp_db
    );

    // A curve that plays at this rate turns it on again and the note goes.
    control.edit(gain(0, 6.0, true)).await.unwrap();
    let view = control.view();
    assert!(view.on);
    assert_eq!(view.note, None);
}

#[tokio::test]
async fn a_chain_that_restarts_is_checked_before_it_is_sent_to() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control
        .load(
            "Shelf".into(),
            None,
            correction(vec![hsc(1000.0, 14.5, 0.707)], 0.0),
        )
        .await
        .unwrap();
    let sends = params.sent().len();
    // A new node at another rate: both at once.
    params.set_node(Some(node(9, Some(192_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert_eq!(params.sent().len(), sends);
    assert_eq!(chain.calls().last(), Some(&ChainCall::Remove));
    assert!(!control.view().on);
}

#[tokio::test]
async fn a_start_at_another_rate_checks_the_saved_curve_there() {
    let dir = Dir::new();
    saved(&dir.state(), vec![hsc(1000.0, 14.5, 0.707)], vec![1], true);
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    params.set_node(Some(node(1, Some(192_000))));
    assert!(control.view().on);
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert!(params.sent().is_empty());
    assert_eq!(chain.calls().last(), Some(&ChainCall::Remove));
    let view = control.view();
    assert!(!view.on && view.note.is_some());
    assert!(!HostEqState::load(&dir.state()).unwrap().unwrap().on);
}

#[tokio::test]
async fn a_state_written_by_the_command_line_is_checked_at_the_running_rate() {
    let dir = Dir::new();
    saved(&dir.state(), vec![hsc(1000.0, 1.0, 0.707)], vec![1], false);
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    params.set_node(Some(node(1, Some(192_000))));
    // Written at 48 kHz without a daemon, then read by the daemon.
    std::thread::sleep(std::time::Duration::from_millis(20));
    saved(&dir.state(), vec![hsc(1000.0, 14.5, 0.707)], vec![1], true);
    assert!(control.reload_if_changed());
    assert!(control.view().on);
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert!(params.sent().is_empty());
    assert_eq!(chain.calls().last(), Some(&ChainCall::Remove));
    assert!(!control.view().on);
}

#[tokio::test]
async fn the_fake_chain_refuses_what_the_real_host_refuses() {
    let dir = Dir::new();
    saved(&dir.state(), vec![hsc(1000.0, 30.0, 0.707)], vec![1], false);
    let (mut control, _, _) = setup(Keep::File(dir.state()));
    let failure = control.set_on(true).await.unwrap_err();
    assert!(
        failure.what.contains("point 1 boosts more than"),
        "{failure}"
    );
    assert!(!control.view().on);
}

/// What a chain of 0.31 (`param_eq`, no preamp control) looks like to the controller.
fn legacy_node() -> LiveNode {
    LiveNode {
        id: 1,
        rate_hz: Some(48_000),
        params: std::collections::HashMap::new(),
    }
}

#[tokio::test]
async fn a_chain_from_an_older_svolder_is_said_and_not_sent_to() {
    let dir = Dir::new();
    saved(&dir.state(), vec![pk(1000.0, 6.0, 1.0)], vec![1], true);
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    params.set_node(Some(legacy_node()));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let view = control.view();
    assert_eq!(view.note.as_deref(), Some(LEGACY_NOTE));
    assert!(view.note.unwrap().contains("older svolder"));
    assert!(!view.live);
    assert!(view.on);
    assert_eq!(
        view.preamp_db, 0.0,
        "the preamp it was applied with, not a measured one"
    );
    assert!(chain.calls().is_empty(), "nothing is written");
    assert!(params.sent().is_empty(), "nothing is sent");

    // A drag is not sent to it either.
    control.edit(gain(0, 7.0, false)).await.unwrap();
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert!(params.sent().is_empty());

    // Off and on replaces it, and the note goes.
    control.set_on(false).await.unwrap();
    control.set_on(true).await.unwrap();
    assert!(matches!(chain.calls().last(), Some(ChainCall::Apply(_))));
    let view = control.view();
    assert_eq!(view.note, None);
    assert!(view.live);
}

#[tokio::test]
async fn a_reapply_that_is_refused_says_so_in_the_note() {
    let dir = Dir::new();
    saved(&dir.state(), vec![pk(1000.0, 6.0, 1.0)], vec![1], true);
    let (mut control, chain, _) = setup(Keep::File(dir.state()));
    chain.refuse(Some(EqFailure {
        what: "point 3 would not be stable".into(),
        next: "Raise it".into(),
    }));
    assert!(control.reapply_if_on().await.is_err());
    let note = control.view().note.expect("the refusal is in the note");
    assert!(note.contains("point 3 would not be stable"), "{note}");
}

/// The highest the chain plays above unity once `sends` have gone to it, as the sum of the
/// preamp and the peak of the slots: every slot holds what `conf` put there at `conf_rate`
/// (the config the chain started from) until a send replaces it, and the stack is measured at
/// `rate`, the rate the sink runs at.
fn over_unity(
    conf: &[(u8, SourceFilter)],
    conf_rate: u32,
    sends: &[Vec<(String, f64)>],
    rate: u32,
) -> f64 {
    let mut held: HashMap<String, f64> = HashMap::new();
    for slot in 1..=SLOTS as u8 {
        let filter = conf
            .iter()
            .find(|(held, _)| *held == slot)
            .map(|(_, filter)| filter);
        held.extend(slot_params(slot, filter, conf_rate));
    }
    for send in sends {
        held.extend(send.iter().cloned());
    }
    let sets: Vec<[f64; 5]> = (1..=SLOTS as u8)
        .map(|slot| {
            ["b0", "b1", "b2", "a1", "a2"]
                .map(|port| f64::from(held[&format!("l{slot:02}:{port}")] as f32))
        })
        .collect();
    let peak = svolder_profiles::tone::response::played_peak_db_of(&sets, &[], rate).max(0.0);
    peak + 20.0 * held["lpre:Mult"].log10()
}

#[tokio::test]
async fn a_curve_softened_live_never_sends_a_preamp_above_the_applied_ones() {
    let (mut control, _, params) = setup(Keep::Memory);
    control
        .load(
            "Hot".into(),
            None,
            correction(vec![pk(1000.0, 6.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    let applied = control.view().preamp_db;
    assert!(applied < -5.9, "{applied}");
    for g in [4.0, 2.0, 1.0] {
        control.edit(gain(0, g, false)).await.unwrap();
        control.tick().await;
    }
    control.edit(gain(0, 0.0, true)).await.unwrap();
    assert!(
        control.view().preamp_db <= applied + 1e-9,
        "the level stays at the applied curve's preamp: {}",
        control.view().preamp_db
    );
    // A rate change puts the slots back to the config's while the preamp keeps what it was
    // sent: after every send the reverted chain must still be at or under unity.
    params.set_node(Some(node(1, Some(44_100))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let sends = params.sent();
    assert!(sends.len() >= 4, "{}", sends.len());
    let conf = [(1, pk(1000.0, 6.0, 1.0))];
    for upto in 1..=sends.len() {
        let over = over_unity(&conf, 48_000, &sends[upto - 1..upto], 44_100);
        assert!(over <= 0.01, "after send {upto}: {over} dB over unity");
    }
    // Turning it off and on applies the softened curve, and its own preamp comes back.
    control.set_on(false).await.unwrap();
    control.set_on(true).await.unwrap();
    assert!(
        control.view().preamp_db > -0.5,
        "{}",
        control.view().preamp_db
    );
}

#[tokio::test]
async fn a_preset_softer_than_the_applied_curve_is_applied() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control
        .load(
            "Hot".into(),
            None,
            correction(vec![pk(100.0, 10.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    control
        .load(
            "Gentle".into(),
            None,
            correction(vec![pk(100.0, 2.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    let applies = chain
        .calls()
        .iter()
        .filter(|call| matches!(call, ChainCall::Apply(_)))
        .count();
    assert_eq!(applies, 2, "the gentle preset applied too");
    let preamp = control.view().preamp_db;
    assert!(preamp > -3.0, "the gentle curve's own preamp: {preamp}");
}

#[tokio::test]
async fn a_state_the_command_line_wrote_is_sent_again_at_the_running_rate() {
    let dir = Dir::new();
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    control
        .load("A".into(), None, correction(vec![pk(500.0, 2.0, 1.0)], 0.0))
        .await
        .unwrap();
    params.set_node(Some(node(1, Some(192_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    // `--sink` writes the config at 48 kHz and the state, on a new node, with a daemon running.
    std::thread::sleep(std::time::Duration::from_millis(20));
    saved(
        &dir.state(),
        vec![pk(1000.0, 6.0, 1.0), pk(4000.0, 0.0, 1.0)],
        vec![1, 2],
        true,
    );
    params.set_node(Some(node(7, Some(192_000))));
    assert!(control.reload_if_changed());
    let before = params.sent().len();
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let sends = params.sent();
    assert_eq!(sends.len(), before + 1, "every slot is sent again");
    assert!((1..=16).all(|slot| touches(&sends[before], slot)));
    control.edit(gain(1, 6.0, true)).await.unwrap();
    control.tick().await;
    let conf = [(1, pk(1000.0, 6.0, 1.0)), (2, pk(4000.0, 0.0, 1.0))];
    let over = over_unity(&conf, 48_000, &params.sent()[before..], 192_000);
    assert!(over <= 0.01, "{over} dB over unity at 192 kHz");
}

#[tokio::test]
async fn the_eq_turns_on_again_at_the_sinks_rate_after_a_rate_change_turned_it_off() {
    let (mut control, _, params) = setup(Keep::Memory);
    control
        .load(
            "Shelf".into(),
            None,
            correction(vec![hsc(1000.0, 14.5, 0.707)], 0.0),
        )
        .await
        .unwrap();
    params.set_node(Some(node(1, Some(192_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert!(!control.view().on);
    // The sink is back at 48 kHz.
    params.set_node(Some(node(2, Some(48_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    control.set_on(true).await.unwrap();
    assert!(control.view().on);
    assert_eq!(control.view().note, None);
    control.edit(gain(0, 14.4, true)).await.unwrap();
}

#[tokio::test]
async fn a_chain_taken_for_an_older_one_is_watched_until_its_control_appears() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    // The node is listed before its Props are.
    params.set_node(Some(legacy_node()));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    assert_eq!(control.view().note.as_deref(), Some(LEGACY_NOTE));
    assert!(!control.view().live);
    params.set_node(Some(node(1, Some(48_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let view = control.view();
    assert!(view.live, "live drags are back");
    assert_eq!(view.note, None);
    let before = params.sent().len();
    control.edit(gain(0, 4.0, false)).await.unwrap();
    control.tick().await;
    assert!(params.sent().len() > before, "a drag is sent live again");
}

#[tokio::test]
async fn a_failed_off_recovers_once_the_sink_plays_the_curve_again() {
    let (mut control, chain, params) = setup(Keep::Memory);
    control
        .load(
            "Shelf".into(),
            None,
            correction(vec![hsc(1000.0, 14.5, 0.707)], 0.0),
        )
        .await
        .unwrap();
    chain.refuse(Some(EqFailure {
        what: "no remove".into(),
        next: "x".into(),
    }));
    params.set_node(Some(node(1, Some(192_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let view = control.view();
    assert!(view.on && !view.live);
    assert!(view.note.unwrap().contains("could not be turned off"));
    // Still unplayable and the chain can be removed now: it is.
    chain.refuse(None);
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let view = control.view();
    assert!(!view.on);
    assert!(view.note.unwrap().contains("turned off at 192000"));
    // Another run: the sink returns to a rate that plays before the remove works.
    let (mut control, chain, params) = setup(Keep::Memory);
    control
        .load(
            "Shelf".into(),
            None,
            correction(vec![hsc(1000.0, 14.5, 0.707)], 0.0),
        )
        .await
        .unwrap();
    chain.refuse(Some(EqFailure {
        what: "no remove".into(),
        next: "x".into(),
    }));
    params.set_node(Some(node(1, Some(192_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    chain.refuse(None);
    params.set_node(Some(node(1, Some(48_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let view = control.view();
    assert!(view.on && view.live, "{view:?}");
    assert_eq!(view.note, None);
    let sends = params.sent();
    assert!((1..=16).all(|slot| touches(sends.last().unwrap(), slot)));
}

/// The preamp ports of `send`, as a send of their own.
fn preamp_only(send: &[(String, f64)]) -> Vec<(String, f64)> {
    send.iter()
        .filter(|(name, _)| name.contains("pre:"))
        .cloned()
        .collect()
}

#[tokio::test]
async fn a_restarted_chain_is_capped_by_the_preamp_its_config_was_written_with() {
    let (mut control, _, params) = setup(Keep::Memory);
    params.set_node(Some(node(1, Some(192_000))));
    let treble = vec![pk(8_000.0, 12.0, 0.5), pk(20_000.0, 12.0, 0.5)];
    control
        .load("Treble".into(), None, correction(treble.clone(), 0.0))
        .await
        .unwrap();
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    // The chain's node starts again from the config on disk, at the same rate.
    params.set_node(Some(node(2, Some(192_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    params.set_node(Some(node(2, Some(44_100))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    // Back at the config's rate the slots are the config's again, under the preamp last sent.
    let last = params.sent().last().cloned().expect("a send");
    let conf: Vec<(u8, SourceFilter)> = treble.into_iter().zip(1..).map(|(f, s)| (s, f)).collect();
    let over = over_unity(&conf, 192_000, &[preamp_only(&last)], 192_000);
    assert!(over <= 0.01, "{over} dB over unity back at 192 kHz");
}

#[tokio::test]
async fn a_chain_started_by_the_command_line_is_checked_and_sent_again_at_any_rate() {
    let dir = Dir::new();
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    control
        .load(
            "A".into(),
            None,
            correction(vec![pk(1000.0, 6.0, 1.0)], 0.0),
        )
        .await
        .unwrap();
    // The command line's new node shows before its state file is written: the daemon's own
    // points go to it.
    params.set_node(Some(node(7, Some(48_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    let first = params.sent().len();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let mut state = HostEqState::load(&dir.state()).unwrap().unwrap();
    state.filters = vec![pk(1000.0, -6.0, 1.0), pk(4000.0, 2.0, 1.0)];
    state.slots = vec![1, 2];
    state.preamp_db = -1.7;
    state.save(&dir.state()).unwrap();
    assert!(control.reload_if_changed());
    for _ in 0..WATCH_EVERY * 2 {
        control.tick().await;
    }
    let sends = params.sent();
    assert_eq!(
        sends.len(),
        first + 1,
        "every slot is sent after the reload"
    );
    assert!((1..=16).all(|slot| touches(&sends[first], slot)));
    control.edit(gain(1, 3.0, true)).await.unwrap();
    control.tick().await;
    let conf = [(1, pk(1000.0, -6.0, 1.0)), (2, pk(4000.0, 2.0, 1.0))];
    let over = over_unity(&conf, 48_000, &params.sent()[first..], 48_000);
    assert!(over <= 0.01, "{over} dB over unity");
}

#[tokio::test]
async fn the_preamp_of_a_node_another_process_started_bounds_what_is_sent_to_it() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    // The command line replaced the chain before it was looked at: it holds -12 dB.
    let mut other = node(9, Some(48_000));
    for name in ["lpre:Mult", "rpre:Mult"] {
        other.params.insert(name.to_string(), 0.251);
    }
    params.set_node(Some(other));
    control.edit(gain(0, 3.0, true)).await.unwrap();
    let sends = params.sent();
    let sent = sends.last().expect("the edit is sent");
    let db = 20.0 * value(sent, "lpre:Mult").expect("the preamp goes").log10();
    assert!(db <= -11.9, "{db} dB sent to a node that holds -12 dB");
    assert!(
        (1..=16).all(|slot| touches(sent, slot)),
        "the update carries every slot"
    );
}
