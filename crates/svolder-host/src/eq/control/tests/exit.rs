// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A chain that dies: seen, started again once, and given up on the second time in a row.

use super::*;
use crate::eq::control::exit::DEATH_WINDOW;

fn sink_a() -> crate::eq::live::OutputSink {
    crate::eq::live::OutputSink {
        name: "a".to_string(),
        description: None,
        rate_hz: Some(48_000),
    }
}

fn stage_on() -> CrossfeedEdit {
    CrossfeedEdit {
        on: Some(true),
        done: true,
        ..CrossfeedEdit::default()
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

/// The chain ends; the waiting task runs and the next tick answers.
async fn dies(control: &mut EqControl, chain: &FakeChain) {
    chain.die();
    tokio::task::yield_now().await;
    control.tick().await;
}

#[tokio::test]
async fn a_chain_that_dies_is_started_again_with_crossfeed_off() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink_a()));
    control.set_on(true).await.unwrap();
    control.crossfeed(stage_on()).await.unwrap();
    assert!(control.view().crossfeed.unwrap().on);
    let before = applies(&chain);

    chain.die();
    tokio::task::yield_now().await;
    // Between the death and the tick that answers it the view does not say live.
    assert!(!control.view().live);
    control.tick().await;

    assert_eq!(
        applies(&chain),
        before + 1,
        "the chain is started again once"
    );
    let view = control.view();
    assert!(view.on && view.live, "the EQ plays again");
    let xf = view.crossfeed.unwrap();
    assert!(!xf.on);
    assert_eq!(xf.off_reason.as_deref(), Some("chain-restarted"));
    assert!(
        xf.note
            .as_deref()
            .is_some_and(|note| note.contains("restarted"))
    );
    assert!(control.view().note.is_some());
    // The next death after the window is the first of a new run.
    for _ in 0..DEATH_WINDOW + 1 {
        control.tick().await;
    }
    dies(&mut control, &chain).await;
    assert_eq!(applies(&chain), before + 2);
    assert!(control.view().on);
}

#[tokio::test]
async fn the_second_quick_death_after_a_restart_leaves_the_eq_off_with_a_note() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    dies(&mut control, &chain).await;
    assert!(control.view().on);
    // The chain that was started again dies soon after (PipeWire restarting): started again.
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    dies(&mut control, &chain).await;
    assert!(
        control.view().on && control.view().live,
        "one quick death is ridden out"
    );
    let applied = applies(&chain);
    // And once more: a crash loop.
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    dies(&mut control, &chain).await;
    let view = control.view();
    assert!(
        !view.on && !view.live,
        "no chain runs, so the view says off"
    );
    assert!(
        view.note
            .as_deref()
            .is_some_and(|note| note.contains("chain stopped")),
        "{:?}",
        view.note
    );
    assert_eq!(applies(&chain), applied, "it is not started a third time");
    assert_eq!(removes(&chain), 1);
    assert_eq!(chain.status().await.running, Some(false));
    // Nothing more happens by itself.
    for _ in 0..WATCH_EVERY * 3 {
        control.tick().await;
    }
    assert_eq!(applies(&chain), applied);
}

#[tokio::test]
async fn a_death_after_one_an_apply_put_right_is_still_seen() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    // A death, and an op applies a healthy chain before the tick takes the flag.
    chain.die();
    tokio::task::yield_now().await;
    control.set_on(true).await.unwrap();
    let applied = applies(&chain);
    ticks_of(&mut control, WATCH_EVERY).await;
    assert_eq!(applies(&chain), applied);
    // The chain dies again later: it is seen.
    chain.die();
    ticks_of(&mut control, 2).await;
    assert_eq!(applies(&chain), applied + 1);
    assert!(control.view().on && control.view().live);
}

#[tokio::test]
async fn a_chain_that_runs_for_crossfeed_alone_is_not_started_again_when_it_dies() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink_a()));
    control.crossfeed(stage_on()).await.unwrap();
    assert!(control.view().crossfeed.unwrap().on);
    let before = applies(&chain);
    dies(&mut control, &chain).await;
    assert_eq!(applies(&chain), before, "no chain is wanted with both off");
    let view = control.view();
    assert!(!view.on && !view.crossfeed.unwrap().on);
    assert_eq!(chain.status().await.running, Some(false));
}

#[tokio::test]
async fn a_death_while_the_eq_is_off_does_nothing() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.set_on(false).await.unwrap();
    let calls = chain.calls().len();
    dies(&mut control, &chain).await;
    assert_eq!(chain.calls().len(), calls);
    assert!(!control.view().on);
}

fn refusing(what: &str) -> EqFailure {
    EqFailure {
        what: what.to_string(),
        next: "Try again".to_string(),
    }
}

/// `count` ticks, the waiting task given its turn before each.
async fn ticks_of(control: &mut EqControl, count: u64) {
    for _ in 0..count {
        tokio::task::yield_now().await;
        control.tick().await;
    }
}

#[tokio::test]
async fn a_chain_another_svolder_removed_is_not_started_again() {
    let dir = Dir::new();
    saved(&dir.state(), vec![pk(1000.0, 1.0, 1.0)], vec![1], true);
    let (mut control, chain, _) = setup(Keep::File(dir.state()));
    control.set_on(true).await.unwrap();
    let applied = applies(&chain);
    // The command line (no daemon behind it) took the chain away and wrote the state.
    chain.die();
    std::thread::sleep(std::time::Duration::from_millis(20));
    saved(&dir.state(), vec![pk(1000.0, 1.0, 1.0)], vec![1], false);
    tokio::task::yield_now().await;
    control.tick().await;
    assert_eq!(applies(&chain), applied, "its change is not undone");
    assert!(!control.view().on);
    assert!(!HostEqState::load(&dir.state()).unwrap().unwrap().on);
}

#[tokio::test]
async fn a_chain_another_svolder_replaced_is_not_started_again() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    let applied = applies(&chain);
    chain.die();
    tokio::task::yield_now().await;
    // Another process applied a chain before this one's tick.
    let state = chain.state().unwrap();
    chain.set_running_again(state);
    control.tick().await;
    assert_eq!(applies(&chain), applied);
    assert!(control.view().on && control.view().live);
}

#[tokio::test]
async fn a_death_an_apply_already_put_right_is_dropped() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    dies(&mut control, &chain).await;
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    chain.die();
    tokio::task::yield_now().await;
    control.set_on(true).await.unwrap();
    control.tick().await;
    assert!(control.view().on && control.view().live);
    assert_eq!(removes(&chain), 0);
}

#[tokio::test]
async fn a_restart_that_fails_is_tried_again_with_a_back_off_and_says_so() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    let before = applies(&chain);
    chain.refuse(Some(refusing("PipeWire is not back")));
    chain.die();
    tokio::task::yield_now().await;
    control.tick().await;
    let view = control.view();
    assert!(!view.live, "no chain, so not live");
    assert!(
        view.note
            .as_deref()
            .is_some_and(|note| note.contains("trying again")),
        "{:?}",
        view.note
    );
    // Seconds go by: tries are 1 s, then 2 s apart, not one a tick.
    ticks_of(&mut control, WATCH_EVERY * 4).await;
    let tries = applies(&chain) - before;
    assert!((2..=4).contains(&tries), "{tries} tries in 4 s");
    assert!(removes(&chain) == 0, "the EQ is not turned off");
    // PipeWire is back.
    chain.refuse(None);
    ticks_of(&mut control, WATCH_EVERY * 5).await;
    let view = control.view();
    assert!(view.on && view.live, "{:?}", view.note);
}

#[tokio::test]
async fn a_turn_off_that_fails_after_a_crash_loop_is_not_tried_every_tick() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    dies(&mut control, &chain).await;
    ticks_of(&mut control, WATCH_EVERY).await;
    // A real second quick death after a restart that went through.
    dies(&mut control, &chain).await;
    assert!(control.view().on, "the first quick death is ridden out");
    ticks_of(&mut control, WATCH_EVERY).await;
    chain.refuse(Some(refusing("cannot remove")));
    chain.die();
    ticks_of(&mut control, WATCH_EVERY * 4).await;
    let tries = removes(&chain);
    assert!((1..=4).contains(&tries), "{tries} removes in 4 s");
}

#[tokio::test]
async fn one_quick_death_whose_restart_is_retried_leaves_the_eq_on() {
    let (mut control, chain, _) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    dies(&mut control, &chain).await;
    ticks_of(&mut control, WATCH_EVERY).await;
    // PipeWire restarts: the chain dies once more and the restart finds it not back yet.
    chain.refuse(Some(refusing("PipeWire is not back")));
    chain.die();
    ticks_of(&mut control, 2).await;
    chain.refuse(None);
    ticks_of(&mut control, WATCH_EVERY * 4).await;
    let view = control.view();
    assert!(view.on && view.live, "{:?}", view.note);
    assert_eq!(removes(&chain), 0);
}
