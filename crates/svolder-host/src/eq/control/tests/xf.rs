// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The crossfeed in the controller: every lifecycle row, every reset, the level, the edges.

use super::*;
use crate::eq::crossfeed::{
    CrossfeedSettings, Model, Stage as CrossfeedStage, stage_worst_db, worst_gain_db,
};
use crate::eq::live::OutputSink;

fn on() -> CrossfeedEdit {
    CrossfeedEdit {
        on: Some(true),
        done: true,
        ..CrossfeedEdit::default()
    }
}

fn off() -> CrossfeedEdit {
    CrossfeedEdit {
        on: Some(false),
        done: true,
        ..CrossfeedEdit::default()
    }
}

fn model(word: &str) -> CrossfeedEdit {
    CrossfeedEdit {
        model: Some(word.to_string()),
        done: true,
        ..CrossfeedEdit::default()
    }
}

fn sink(name: &str) -> OutputSink {
    OutputSink {
        name: name.to_string(),
        description: Some(format!("{name} (description)")),
        rate_hz: Some(48_000),
    }
}

/// A controller over the state file `dir` holds, with a chain that says what the file says (a run
/// that left the chain behind).
fn setup_over(path: &Path) -> (EqControl, FakeChain, FakeParams) {
    let chain = FakeChain::with_state(HostEqState::load(path).unwrap().unwrap());
    let params = FakeParams::new();
    let control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params.clone()),
        Keep::File(path.to_path_buf()),
    );
    (control, chain, params)
}

async fn looks(control: &mut EqControl, seconds: u64) {
    for _ in 0..seconds * WATCH_EVERY {
        control.tick().await;
    }
}

/// A controller with the stage on over a flat chain (the EQ off), the output `a` seen.
async fn flat_on() -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.crossfeed(on()).await.unwrap();
    (control, chain, params)
}

/// A controller with the EQ on and the stage on, the output `a` seen.
async fn both_on() -> (EqControl, FakeChain, FakeParams) {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    control.crossfeed(on()).await.unwrap();
    (control, chain, params)
}

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

fn mix_of(sent: &[(String, f64)]) -> Option<f64> {
    value(sent, "lmix:Gain 2")
}

fn pre_db(sent: &[(String, f64)]) -> Option<f64> {
    value(sent, "lpre:Mult").map(|mult| 20.0 * mult.log10())
}

/// The sends of `params` since `from`, each as the list of its values in order.
fn stage_sends(params: &FakeParams, from: usize) -> Vec<Vec<(String, f64)>> {
    params.sent().into_iter().skip(from).collect()
}

/// Walks every send entry by entry and checks that, after each one, the preamp and the stage
/// that play together never add above unity: `worst_db` is the stage's worst gain while it is on.
fn assert_never_above_unity(sends: &[Vec<(String, f64)>], worst_db: f64) {
    let (mut pre, mut mix) = (0.0f64, 0.0f64);
    for (n, send) in sends.iter().enumerate() {
        for (name, value) in send {
            match name.as_str() {
                "lpre:Mult" => pre = 20.0 * value.log10(),
                "lmix:Gain 2" => mix = *value,
                _ => continue,
            }
            let level = pre + if mix > 0.0 { worst_db } else { 0.0 };
            assert!(
                level <= 1e-9,
                "send {n}: after {name} the level is {level} dB (pre {pre}, mix {mix})"
            );
        }
    }
}

#[tokio::test]
async fn crossfeed_on_with_no_chain_applies_one_flat_and_sends_the_stage() {
    let (control, chain, params) = flat_on().await;
    let calls = applies(&chain);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].flat, "the EQ plays flat");
    assert_eq!(
        calls[0].crossfeed, None,
        "the stage is sent, not in the config"
    );
    let view = control.view();
    assert!(!view.on, "the EQ's own switch stays off");
    let xf = view.crossfeed.unwrap();
    assert!(xf.on);
    assert_eq!(xf.output.as_deref(), Some("a (description)"));
    let sent = params.sent();
    let last = sent.last().expect("the stage was sent");
    assert_eq!(mix_of(last), Some(1.0));
    // The EQ is flat on the chain: every slot the identity.
    assert_eq!(value(last, "l01:b0"), Some(1.0));
    assert_eq!(value(last, "l01:b1"), Some(0.0));
    let state = chain.state().unwrap();
    assert!(state.flat && state.on);
}

#[tokio::test]
async fn the_eq_off_while_crossfeed_is_on_goes_flat_with_no_apply_or_remove() {
    let (mut control, chain, params) = both_on().await;
    control.edit(gain(0, 6.0, true)).await.unwrap();
    let (applied, sends) = (applies(&chain).len(), params.sent().len());
    control.set_on(false).await.unwrap();
    assert_eq!(applies(&chain).len(), applied, "no apply");
    assert_eq!(removes(&chain), 0, "no remove");
    let view = control.view();
    assert!(!view.on && view.crossfeed.as_ref().unwrap().on);
    let sent = stage_sends(&params, sends);
    let flat = sent.last().expect("slots and preamp were sent");
    assert_eq!(value(flat, "l01:b0"), Some(1.0), "slot 1 is the identity");
    assert!(pre_db(flat).is_some(), "the preamp is sent again");
    // The points are kept for when the EQ comes back.
    assert_eq!(view.points[0].g, 6.0);

    // The EQ on again sends the slots live and applies nothing.
    let sends = params.sent().len();
    control.set_on(true).await.unwrap();
    assert_eq!(applies(&chain).len(), applied);
    assert_eq!(removes(&chain), 0);
    let sent = stage_sends(&params, sends);
    let back = sent.last().unwrap();
    assert_ne!(
        value(back, "l01:b0"),
        Some(1.0),
        "the boost is back in slot 1"
    );
    let view = control.view();
    assert!(view.on && view.crossfeed.unwrap().on);
}

#[tokio::test]
async fn both_off_removes_the_chain_and_one_off_keeps_it() {
    // Crossfeed alone: turning it off removes the chain.
    let (mut control, chain, _) = flat_on().await;
    control.crossfeed(off()).await.unwrap();
    assert_eq!(removes(&chain), 1);
    assert!(!control.view().crossfeed.unwrap().on);
    assert!(!chain.state().unwrap().on);

    // The EQ on as well: crossfeed off keeps the chain, then the EQ off removes it.
    let (mut control, chain, params) = both_on().await;
    let sends = params.sent().len();
    control.crossfeed(off()).await.unwrap();
    assert_eq!(removes(&chain), 0);
    assert_eq!(
        mix_of(params.sent().last().unwrap()),
        Some(0.0),
        "the bypass"
    );
    assert!(params.sent().len() > sends);
    control.set_on(false).await.unwrap();
    assert_eq!(removes(&chain), 1);

    // The EQ off first (flat), then crossfeed off: the same.
    let (mut control, chain, _) = both_on().await;
    control.set_on(false).await.unwrap();
    assert_eq!(removes(&chain), 0);
    control.crossfeed(off()).await.unwrap();
    assert_eq!(removes(&chain), 1);
}

#[tokio::test]
async fn the_preamp_goes_down_before_the_stage_and_up_after_it() {
    let (mut control, _, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    // Mid/Side bass at 50 % is the worst case: +3 dB.
    control
        .crossfeed(CrossfeedEdit {
            model: Some("mid-side".into()),
            on: Some(true),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let worst = worst_gain_db(&control.xf, 48_000);
    assert!(worst > 2.9, "{worst}");
    let turned_on = params.sent();
    let send = turned_on.last().unwrap();
    let names: Vec<&str> = send.iter().map(|(n, _)| n.as_str()).collect();
    let pre = names.iter().position(|n| *n == "lpre:Mult").unwrap();
    let mix = names.iter().position(|n| *n == "lmix:Gain 2").unwrap();
    assert!(pre < mix, "the preamp falls before the stage turns on");
    assert!(pre_db(send).unwrap() <= -worst + 1e-9);

    let sends = params.sent().len();
    control.crossfeed(off()).await.unwrap();
    let sent = stage_sends(&params, sends);
    let send = sent.last().unwrap();
    let names: Vec<&str> = send.iter().map(|(n, _)| n.as_str()).collect();
    let pre = names.iter().position(|n| *n == "lpre:Mult").unwrap();
    let mix = names.iter().position(|n| *n == "lmix:Gain 2").unwrap();
    assert!(mix < pre, "the preamp rises after the stage turns off");
    assert_eq!(pre_db(send).map(|db| db.round()), Some(0.0));
    assert_never_above_unity(&params.sent(), worst);
}

#[tokio::test]
async fn no_update_plays_above_unity_through_every_change() {
    let (mut control, _, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 5.0, true)).await.unwrap();
    control.crossfeed(model("mid-side")).await.unwrap();
    control.crossfeed(on()).await.unwrap();
    // The worst case of everything below: Mid/Side bass at 50 %.
    let worst = worst_gain_db(&control.xf, 48_000);
    control.edit(gain(0, -2.0, true)).await.unwrap();
    // Mid/Side bass at 50 % is the same worst case whatever the crossover is.
    for hz in [100.0, 200.0, 60.0, 120.0] {
        control
            .crossfeed(CrossfeedEdit {
                crossover_hz: Some(hz),
                done: true,
                ..CrossfeedEdit::default()
            })
            .await
            .unwrap();
    }
    control.set_on(false).await.unwrap();
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 3.0, true)).await.unwrap();
    control.crossfeed(off()).await.unwrap();
    assert_never_above_unity(&params.sent(), worst);
    // And the view's preamp is the played one.
    assert!(control.view().preamp_db <= 0.0);
}

/// The most the stage `settings` makes at `rate` adds when it plays at any of the usual rates:
/// a rate change between two looks plays it at another rate until it is sent again.
fn worst_at_any_rate(settings: &CrossfeedSettings, rate: u32) -> f64 {
    let stage = CrossfeedStage::of(settings, rate).unwrap();
    crate::eq::manage::CONFIG_STAGE_RATES
        .into_iter()
        .chain([rate])
        .map(|play| stage_worst_db(&stage, play))
        .fold(0.0, f64::max)
}

/// A chain that refuses its apply number `fail_at` (counted from 1 over its life) and does as the
/// fake does otherwise.
struct FailsOneApply {
    inner: FakeChain,
    fail_at: AtomicUsize,
    applies: AtomicUsize,
}

#[async_trait::async_trait]
impl Chain for FailsOneApply {
    async fn apply(&self, request: ApplyRequest) -> Result<HostEqState, EqFailure> {
        let count = self.applies.fetch_add(1, Ordering::SeqCst) + 1;
        if count == self.fail_at.load(Ordering::SeqCst) {
            return Err(refusal("the apply failed", "try again"));
        }
        self.inner.apply(request).await
    }

    async fn remove(&self) -> Result<(), EqFailure> {
        self.inner.remove().await
    }

    async fn status(&self) -> crate::eq::manage::Status {
        self.inner.status().await
    }

    async fn keep_conf(&self, request: ApplyRequest) -> Result<(), EqFailure> {
        self.inner.keep_conf(request).await
    }

    fn unavailable(&self) -> Option<EqFailure> {
        self.inner.unavailable()
    }

    fn state(&self) -> Option<HostEqState> {
        self.inner.state()
    }
}

/// Crossfeed turned on with the EQ off applies a flat chain; when the stage's send fails and so
/// does the apply that would hold it in the config, the start is undone whole: the flat chain is
/// removed, and the EQ the user had turned off is not sent to it.
#[tokio::test]
async fn a_failed_crossfeed_start_from_eq_off_leaves_no_chain_and_the_eq_off() {
    let chain = FakeChain::new();
    let params = FakeParams::new();
    params.set_output(Some(sink("a")));
    let failing = Arc::new(FailsOneApply {
        inner: chain.clone(),
        fail_at: AtomicUsize::new(0),
        applies: AtomicUsize::new(0),
    });
    let mut control = EqControl::new(failing.clone(), Box::new(params.clone()), Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 6.0, true)).await.unwrap();
    control.set_on(false).await.unwrap();
    let removed = removes(&chain);
    // The flat apply goes, the send does not, and the apply that holds the stage fails.
    failing
        .fail_at
        .store(failing.applies.load(Ordering::SeqCst) + 2, Ordering::SeqCst);
    params.takes(false);
    assert!(control.crossfeed(on()).await.is_err());
    params.takes(true);
    let sends = params.sent().len();
    looks(&mut control, 3).await;
    let view = control.view();
    assert!(!view.on, "the EQ stays off");
    assert_eq!(view.crossfeed.map(|xf| xf.on), Some(false));
    assert_eq!(removes(&chain), removed + 1, "the flat chain is removed");
    assert_eq!(
        params.sent().len(),
        sends,
        "nothing goes to a chain that is not there"
    );
}

/// The preamp a live stage plays under covers it at every usual rate, not only the one it was
/// worked out for: Meier's delay plays as other whole samples at another rate until the stage is
/// sent again (Meier at 0.5 ms worked out at 48 kHz needs 2.0 dB there and 3.2 dB at another).
#[tokio::test]
async fn a_live_stage_is_paid_for_at_every_rate_it_may_play_at() {
    let (mut control, _, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.crossfeed(model("meier")).await.unwrap();
    control.crossfeed(on()).await.unwrap();
    for ms in [0.25, 0.5, 0.73, 0.9, 1.0] {
        control
            .crossfeed(CrossfeedEdit {
                delay_ms: Some(ms),
                done: true,
                ..CrossfeedEdit::default()
            })
            .await
            .unwrap();
        let anywhere = worst_at_any_rate(&control.xf, 48_000);
        let db = control.view().preamp_db;
        assert!(db <= -anywhere + 1e-9, "{ms} ms: {db} vs {anywhere}");
        assert!(
            db > -anywhere - 0.11,
            "{ms} ms: not lowered more than that needs"
        );
        assert!((pre_db(params.sent().last().unwrap()).unwrap() - db).abs() < 1e-6);
    }
}

/// The usual rates take in the ones a DAC runs at over 192 kHz: Meier at 50 % and 0.5 ms worked
/// out at 176.4 kHz adds +4.75 dB at 384 kHz, against +3.8 dB over the rates up to 192 kHz.
#[test]
fn the_stage_is_paid_for_up_to_384_khz() {
    use crate::eq::crossfeed::CrossfeedChange;
    use crate::eq::manage::{CONFIG_STAGE_RATES, config_stage_cost_db};
    assert!(CONFIG_STAGE_RATES.contains(&352_800) && CONFIG_STAGE_RATES.contains(&384_000));
    for rate in CONFIG_STAGE_RATES {
        let settings = CrossfeedSettings::for_model(Model::Meier)
            .change(
                &CrossfeedChange {
                    amount_pct: Some(50.0),
                    delay_ms: Some(0.5),
                    ..Default::default()
                },
                rate,
            )
            .unwrap();
        let stage = CrossfeedStage::of(&settings, rate).unwrap();
        let cost = config_stage_cost_db(&settings, rate);
        for play in [352_800, 384_000] {
            let worst = stage_worst_db(&stage, play);
            assert!(
                cost >= worst - 1e-9,
                "{rate} -> {play}: cost {cost} under {worst}"
            );
        }
    }
}

/// The models with no added delay play the same stage at every rate: their cost is the played
/// rate's, as before.
#[tokio::test]
async fn a_stage_with_no_delay_costs_what_its_own_rate_needs() {
    for word in ["bauer", "linkwitz", "mid-side", "meier"] {
        let (mut control, _, params) = setup(Keep::Memory);
        params.set_output(Some(sink("a")));
        control.set_on(true).await.unwrap();
        control.crossfeed(model(word)).await.unwrap();
        control.crossfeed(on()).await.unwrap();
        let cost = crate::eq::manage::stage_cost_db(&control.xf, 48_000);
        assert_eq!(control.view().preamp_db, -cost, "{word}");
    }
}

#[tokio::test]
async fn meiers_worst_gain_is_folded_again_whenever_the_delay_or_the_values_move() {
    let (mut control, _, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    control.crossfeed(model("meier")).await.unwrap();
    control.crossfeed(on()).await.unwrap();
    let own = control.view().preamp_db;
    assert!((-0.7..=-0.4).contains(&own), "{own}");
    let preamp = |control: &EqControl| control.view().preamp_db;

    let mut seen = vec![own];
    // The delay: 0.5 ms total then 1 ms total.
    for ms in [0.5, 1.0] {
        control
            .crossfeed(CrossfeedEdit {
                delay_ms: Some(ms),
                done: true,
                ..CrossfeedEdit::default()
            })
            .await
            .unwrap();
        let db = preamp(&control);
        let worst = worst_gain_db(&control.xf, 48_000);
        let anywhere = worst_at_any_rate(&control.xf, 48_000);
        assert!(db <= -worst + 1e-9, "{ms} ms: {db} vs worst {worst}");
        assert!(
            db <= -anywhere + 1e-9,
            "{ms} ms: {db} vs {anywhere} at another rate"
        );
        assert!(
            db > -anywhere - 0.11,
            "{ms} ms: not lowered more than that needs"
        );
        seen.push(db);
    }
    assert!(seen[0] > seen[1] && seen[1] > seen[2], "{seen:?}");
    // The amount moves it too.
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(10.0),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let low = preamp(&control);
    assert!(low > seen[2], "{low} vs {}", seen[2]);
    // The value really was sent.
    assert!((pre_db(params.sent().last().unwrap()).unwrap() - low).abs() < 1e-6);
    // A model switch moves it: Bauer needs none.
    control.crossfeed(model("bauer")).await.unwrap();
    assert_eq!(preamp(&control), 0.0);
}

#[tokio::test]
async fn a_drag_is_sent_on_the_tick_and_saved_only_at_its_end() {
    let dir = Dir::new();
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    control.crossfeed(on()).await.unwrap();
    let sends = params.sent().len();
    for pct in [20.0, 22.0, 24.0] {
        control
            .crossfeed(CrossfeedEdit {
                amount_pct: Some(pct),
                done: false,
                ..CrossfeedEdit::default()
            })
            .await
            .unwrap();
    }
    assert_eq!(params.sent().len(), sends, "nothing goes before the tick");
    control.tick().await;
    assert_eq!(params.sent().len(), sends + 1, "the newest value goes once");
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(
        state.crossfeed.unwrap().amount,
        CrossfeedSettings::default().amount,
        "not saved"
    );
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(24.0),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!((state.crossfeed.unwrap().amount - 0.24).abs() < 1e-9);
    assert!(state.crossfeed_on);
}

// ----- the resets -----

#[tokio::test]
async fn the_owner_start_turns_a_crossfeed_found_on_off() {
    // The EQ and the stage on: the stage is taken out of a chain that holds it, which runs on.
    let dir = Dir::new();
    HostEqState {
        on: true,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(1000.0, 3.0, 1.0)],
        slots: vec![1],
        crossfeed: Some(CrossfeedSettings::for_model(Model::Bauer)),
        crossfeed_on: true,
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
    }
    .save(&dir.state())
    .unwrap();
    let (mut control, chain, _) = setup_over(&dir.state());
    let xf = control.view().crossfeed.unwrap();
    assert!(!xf.on, "never on from a file");
    assert_eq!(xf.off_reason.as_deref(), Some("restart"));
    assert!(xf.note.unwrap().contains("restart"));
    control.sync_running().await;
    let requests = applies(&chain);
    assert_eq!(
        requests.len(),
        1,
        "the chain is applied again without the stage"
    );
    assert_eq!(requests[0].crossfeed, None);
    assert!(!requests[0].flat);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(!state.crossfeed_on && state.on);
    assert!(state.crossfeed.is_some(), "the settings stay");
    assert!(control.view().on, "the EQ plays on");
}

#[tokio::test]
async fn the_owner_start_removes_a_chain_that_ran_for_crossfeed_alone() {
    let dir = Dir::new();
    HostEqState {
        on: true,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(1000.0, 3.0, 1.0)],
        slots: vec![1],
        crossfeed: Some(CrossfeedSettings::for_model(Model::Meier)),
        crossfeed_on: true,
        flat: true,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
    }
    .save(&dir.state())
    .unwrap();
    let (mut control, chain, _) = setup_over(&dir.state());
    assert!(!control.view().crossfeed.unwrap().on);
    // The chain the state names runs (the fake says so while the state says on).
    control.sync_running().await;
    assert_eq!(removes(&chain), 1);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(!state.on && !state.flat && !state.crossfeed_on);
    assert_eq!(
        state.crossfeed.map(|s| s.model),
        Some(Model::Meier),
        "the settings stay"
    );
    assert!(!control.view().on);
}

#[tokio::test]
async fn the_output_changing_turns_the_stage_off_on_either_chain() {
    // The EQ and crossfeed: the stage goes off live.
    let (mut control, chain, params) = both_on().await;
    looks(&mut control, 2).await;
    assert!(
        control.view().crossfeed.unwrap().on,
        "the same output is no change"
    );
    params.set_output(Some(sink("b")));
    looks(&mut control, 1).await;
    let xf = control.view().crossfeed.unwrap();
    assert!(!xf.on);
    assert_eq!(xf.off_reason.as_deref(), Some("output-changed"));
    assert!(xf.note.unwrap().contains("b (description)"));
    assert_eq!(mix_of(params.sent().last().unwrap()), Some(0.0));
    assert_eq!(removes(&chain), 0);
    assert!(control.view().on, "the EQ plays on");

    // Crossfeed alone: the chain is removed.
    let (mut control, chain, params) = flat_on().await;
    params.set_output(Some(sink("b")));
    looks(&mut control, 1).await;
    assert!(!control.view().crossfeed.unwrap().on);
    assert_eq!(removes(&chain), 1);
    assert!(!control.view().on);
}

#[tokio::test]
async fn a_chain_that_restarts_turns_the_stage_off() {
    let (mut control, chain, params) = both_on().await;
    params.set_node(Some(node(9, Some(48_000))));
    looks(&mut control, 1).await;
    let xf = control.view().crossfeed.unwrap();
    assert!(!xf.on);
    assert_eq!(xf.off_reason.as_deref(), Some("chain-restarted"));
    assert_eq!(removes(&chain), 0, "the EQ's chain stays");
    assert!(!control.xf_conf && control.owed.is_none());

    // For crossfeed alone the restarted chain is a transparent one: removed.
    let (mut control, chain, params) = flat_on().await;
    params.set_node(Some(node(9, Some(48_000))));
    looks(&mut control, 1).await;
    assert!(!control.view().crossfeed.unwrap().on);
    assert_eq!(removes(&chain), 1);
}

#[tokio::test]
async fn a_chain_that_restarts_under_the_same_id_turns_the_stage_off() {
    let (mut control, chain, params) = both_on().await;
    looks(&mut control, 1).await;
    // PipeWire lists the new node under the old one's id: only its serial is its own.
    let mut again = node(1, Some(48_000));
    again
        .params
        .extend(crate::eq::live::crossfeed_params(&CrossfeedStage::bypass()));
    again.serial = Some(9);
    params.set_node(Some(again));
    looks(&mut control, 1).await;
    let xf = control.view().crossfeed.unwrap();
    assert!(!xf.on);
    assert_eq!(xf.off_reason.as_deref(), Some("chain-restarted"));
    assert_eq!(removes(&chain), 0, "the EQ's chain stays");
}

#[tokio::test]
async fn a_graceful_stop_leaves_no_stage_and_no_flat_chain() {
    let dir = Dir::new();
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    control.crossfeed(on()).await.unwrap();
    assert!(
        HostEqState::load(&dir.state())
            .unwrap()
            .unwrap()
            .crossfeed_on
    );
    control.shutdown().await;
    assert_eq!(mix_of(params.sent().last().unwrap()), Some(0.0));
    let xf = control.view().crossfeed.unwrap();
    assert!(!xf.on);
    assert_eq!(xf.off_reason.as_deref(), Some("stopped"));
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(!state.crossfeed_on && state.on && state.crossfeed.is_some());
    assert_eq!(removes(&chain), 0);

    let dir = Dir::new();
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    params.set_output(Some(sink("a")));
    control.crossfeed(on()).await.unwrap();
    control.shutdown().await;
    assert_eq!(removes(&chain), 1);
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(!state.on && !state.flat && !state.crossfeed_on);
}

#[tokio::test]
async fn a_new_rate_sends_the_stage_again_and_is_no_reset() {
    for flat in [false, true] {
        let (mut control, _, params) = match flat {
            true => flat_on().await,
            false => both_on().await,
        };
        let sends = params.sent().len();
        params.set_node(Some({
            let mut n = LiveNode {
                id: 1,
                serial: Some(1),
                rate_hz: Some(44_100),
                sink: None,
                playing: false,
                params: crate::eq::live::preamp_params(60.0).into_iter().collect(),
            };
            n.params
                .extend(crate::eq::live::crossfeed_params(&CrossfeedStage::bypass()));
            n
        }));
        looks(&mut control, 1).await;
        let xf = control.view().crossfeed.unwrap();
        assert!(xf.on, "a rate change is not a reset (flat: {flat})");
        let sent = stage_sends(&params, sends);
        let last = sent.last().expect("re-sent");
        assert_eq!(mix_of(last), Some(1.0), "the stage plays at the new rate");
        let at_44k =
            crate::eq::live::crossfeed_params(&CrossfeedStage::of(&control.xf, 44_100).unwrap());
        assert_eq!(value(last, "ldir:b0"), value(&at_44k, "ldir:b0"));
    }
}

/// The stages run up to 384 kHz: a sink that moves above it turns crossfeed off with a note
/// that says why, the chain plays the points under their preamp with the stage out, and turning
/// crossfeed on there is refused in words.
#[tokio::test]
async fn above_384_khz_crossfeed_turns_off_and_is_refused() {
    let (mut control, _, params) = both_on().await;
    control.crossfeed(model("meier")).await.unwrap();
    control.edit(gain(0, 6.0, true)).await.unwrap();
    let sends = params.sent().len();
    let mut node = node(1, Some(705_600));
    node.params
        .extend(crate::eq::live::crossfeed_params(&CrossfeedStage::bypass()));
    params.set_node(Some(node));
    looks(&mut control, 2).await;
    let xf = control.view().crossfeed.unwrap();
    assert!(!xf.on, "off above 384 kHz");
    let note = xf.note.unwrap_or_default();
    assert!(note.contains("384 kHz"), "{note}");
    let sent = stage_sends(&params, sends);
    let last_mix = sent.iter().rev().find_map(|send| mix_of(send));
    assert_eq!(last_mix, Some(0.0), "the stage is taken out: {sent:?}");
    let last = sent.last().expect("sent");
    assert!(played_peak(&control) + pre_db(last).unwrap() <= 0.02);
    let refused = control.crossfeed(on()).await.unwrap_err();
    assert!(refused.what.contains("384 kHz"), "{refused:?}");
    assert!(!control.view().crossfeed.unwrap().on);
}

/// A crossfeed drag in flight while the sink moves above 384 kHz: the look that sees the new rate
/// turns crossfeed off, and no line raises the preamp while the stage worked out at 384 kHz still
/// plays (Meier at 0.5 ms worked out at 384 kHz adds +4.49 dB at 768 kHz, against the 3.7 dB of the
/// same settings worked out at 768 kHz).
#[tokio::test]
async fn a_crossfeed_drag_across_a_move_above_384_khz_never_plays_above_unity() {
    use crate::eq::crossfeed::CrossfeedChange;
    let (mut control, _, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    let mut at_384k = node(1, Some(384_000));
    at_384k
        .params
        .extend(crate::eq::live::crossfeed_params(&CrossfeedStage::bypass()));
    params.set_node(Some(at_384k));
    control.set_on(true).await.unwrap();
    control.crossfeed(model("meier")).await.unwrap();
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(50.0),
            delay_ms: Some(0.5),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    control.crossfeed(on()).await.unwrap();
    looks(&mut control, 1).await;
    // Each stage a drag line may carry (the amount, worked out at either rate), told by its
    // direct path's b0, with the most it adds at any rate.
    let mut stages = Vec::new();
    for pct in [48.0, 49.0, 50.0] {
        for rate in [384_000, 768_000] {
            let settings = control
                .xf
                .change(
                    &CrossfeedChange {
                        amount_pct: Some(pct),
                        ..CrossfeedChange::default()
                    },
                    rate,
                )
                .unwrap();
            let ports =
                crate::eq::live::crossfeed_params(&CrossfeedStage::of(&settings, rate).unwrap());
            let b0 = value(&ports, "ldir:b0").unwrap();
            stages.push((b0, worst_at_any_rate(&settings, rate)));
        }
    }
    let worst_of = |b0: f64| {
        stages
            .iter()
            .find(|(held, _)| (held - b0).abs() < 1e-12)
            .map(|(_, worst)| *worst)
            .expect("a stage the drag sends")
    };
    let from = params.sent().len();
    let drag = |pct: f64, done: bool| CrossfeedEdit {
        amount_pct: Some(pct),
        done,
        ..CrossfeedEdit::default()
    };
    control.crossfeed(drag(49.0, false)).await.unwrap();
    control.tick().await;
    params.set_rate(Some(768_000));
    for pct in [50.0, 48.0] {
        let _ = control.crossfeed(drag(pct, false)).await;
        control.tick().await;
    }
    let _ = control.crossfeed(drag(50.0, true)).await;
    looks(&mut control, 1).await;
    assert!(!control.view().crossfeed.unwrap().on, "off above 384 kHz");
    assert!(params.sent().len() > from, "the drag went live");
    // Walked entry by entry from the drag on: the stage that plays is the last one sent.
    let sends = params.sent();
    let held = |name: &str| {
        sends[..from]
            .iter()
            .rev()
            .find_map(|send| value(send, name))
    };
    let mut pre = 20.0 * held("lpre:Mult").unwrap().log10();
    let mut mix = held("lmix:Gain 2").unwrap();
    let mut worst = worst_of(held("ldir:b0").unwrap());
    for (n, send) in sends[from..].iter().enumerate() {
        for (name, value) in send {
            match name.as_str() {
                "lpre:Mult" => pre = 20.0 * value.log10(),
                "lmix:Gain 2" => mix = *value,
                // The bypass's direct path is the identity: the stage before it stays priced
                // until the mixer takes it off.
                "ldir:b0" if *value != 1.0 => worst = worst_of(*value),
                _ => continue,
            }
            let level = pre + if mix > 0.0 { worst } else { 0.0 };
            assert!(
                level <= 0.02,
                "send {n}: after {name} {level:+.3} dB (pre {pre:+.2}, mix {mix})"
            );
        }
    }
}

/// With no chain running the rate is the sink's own: crossfeed turned on there above 384 kHz is
/// refused, and no chain is applied for it.
#[tokio::test]
async fn crossfeed_on_with_no_chain_above_384_khz_is_refused() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    params.set_node(Some(node(1, Some(768_000))));
    let refused = control.crossfeed(on()).await.unwrap_err();
    assert!(refused.what.contains("384 kHz"), "{refused:?}");
    assert!(applies(&chain).is_empty(), "no chain for it");
    assert!(!control.view().crossfeed.unwrap().on);
}

#[tokio::test]
async fn a_rebuild_at_the_same_rate_keeps_the_stage_while_the_view_says_on() {
    let (mut control, chain, params) = both_on().await;
    control.apply().await.unwrap();
    assert!(control.view().crossfeed.unwrap().on);
    assert!(control.stage_dirty, "the new chain's stage is owed");
    let sends = params.sent().len();
    looks(&mut control, 1).await;
    let sent = stage_sends(&params, sends);
    assert_eq!(mix_of(sent.last().expect("sent")), Some(1.0));
    assert_eq!(applies(&chain).len(), 2);
    assert!(control.view().crossfeed.unwrap().on);
}

// ----- the edges -----

#[tokio::test]
async fn without_live_updates_the_stage_is_in_the_config_and_the_start_finds_it() {
    let dir = Dir::new();
    let (mut control, chain, params) = setup(Keep::File(dir.state()));
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    control.live = false;
    control.crossfeed(on()).await.unwrap();
    let requests = applies(&chain);
    let last = requests.last().unwrap();
    assert_eq!(
        last.crossfeed,
        Some(control.xf.clone()),
        "the config holds the stage"
    );
    assert!(
        HostEqState::load(&dir.state())
            .unwrap()
            .unwrap()
            .crossfeed_on
    );
    assert!(control.view().crossfeed.unwrap().on);

    // A new owner finds crossfeed_on and applies the chain without the stage.
    let (mut again, chain, _) = setup_over(&dir.state());
    again.sync_running().await;
    assert_eq!(applies(&chain).last().unwrap().crossfeed, None);
    assert!(
        !HostEqState::load(&dir.state())
            .unwrap()
            .unwrap()
            .crossfeed_on
    );
}

#[tokio::test]
async fn a_chain_from_before_the_stage_is_rebuilt_once_and_the_view_says_so() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    params.without_crossfeed_ports();
    control.crossfeed(on()).await.unwrap();
    assert_eq!(applies(&chain).len(), 2, "applied once more");
    let xf = control.view().crossfeed.unwrap();
    assert!(xf.on);
    assert!(xf.note.unwrap().contains("rebuilt"));
    assert!(!applies(&chain)[1].flat, "the EQ plays on");
}

#[tokio::test]
async fn crossfeed_is_refused_in_words_where_the_chain_cannot_take_it() {
    // A chain from an older svolder.
    let dir = Dir::new();
    saved(&dir.state(), vec![pk(1000.0, 6.0, 1.0)], vec![1], true);
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    params.set_node(Some(legacy_node()));
    looks(&mut control, 1).await;
    let failure = control.crossfeed(on()).await.unwrap_err();
    assert!(failure.what.contains("older svolder"), "{failure}");
    assert!(control.view().crossfeed.unwrap().unavailable.is_some());
    assert!(!control.view().crossfeed.unwrap().on);

    // The computer's controls unavailable.
    let (mut control, chain, _) = setup(Keep::Memory);
    chain.refuse(Some(EqFailure {
        what: "PipeWire is not installed".into(),
        next: "Install it".into(),
    }));
    let failure = control.crossfeed(on()).await.unwrap_err();
    assert!(failure.what.contains("PipeWire"), "{failure}");
    assert!(
        control
            .view()
            .crossfeed
            .unwrap()
            .unavailable
            .unwrap()
            .starts_with("computer controls unavailable:")
    );

    // A change that names nothing, an unknown model, a delay for a model with none.
    let (mut control, chain, _) = setup(Keep::Memory);
    assert!(control.crossfeed(CrossfeedEdit::default()).await.is_err());
    assert!(control.crossfeed(model("nonsense")).await.is_err());
    control.crossfeed(model("mid-side")).await.unwrap();
    let failure = control
        .crossfeed(CrossfeedEdit {
            delay_ms: Some(0.5),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap_err();
    assert!(failure.what.contains("no delay"), "{failure}");
    assert_eq!(control.view().crossfeed.unwrap().model, "mid-side");
    assert!(chain.calls().is_empty(), "a refusal changes nothing");
}

#[tokio::test]
async fn settings_are_kept_while_off_and_a_model_switch_loads_its_default() {
    let dir = Dir::new();
    let (mut control, _, params) = setup(Keep::File(dir.state()));
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(30.0),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let xf = control.view().crossfeed.unwrap();
    assert_eq!(xf.preset, None, "a value edit is Custom");
    assert_eq!(xf.edited_from.as_deref(), Some("bs2b default"));
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(
        (state.crossfeed.unwrap().amount - 0.30).abs() < 1e-9,
        "saved while off"
    );
    control.crossfeed(model("linkwitz")).await.unwrap();
    let xf = control.view().crossfeed.unwrap();
    assert_eq!(xf.model, "linkwitz");
    assert!(xf.preset.is_some(), "its default preset");
}

// ----- review fixes (2.1 fix 1) -----

#[tokio::test]
async fn a_container_restart_with_crossfeed_found_on_applies_the_eq_again_with_it_off() {
    let dir = Dir::new();
    let mut state = HostEqState {
        on: true,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(1000.0, 3.0, 1.0)],
        slots: vec![1],
        crossfeed: Some(CrossfeedSettings::for_model(Model::Bauer)),
        crossfeed_on: true,
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
    };
    state.save(&dir.state()).unwrap();
    // The container's chain died with the crash: the chain says it does not run.
    state.on = false;
    let chain = FakeChain::with_state(state);
    let params = FakeParams::new();
    let mut control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(params),
        Keep::File(dir.state()),
    );
    assert!(
        control.reapply_if_on().await.unwrap(),
        "the EQ is applied again"
    );
    let requests = applies(&chain);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].crossfeed, None);
    assert!(!requests[0].flat);
    let saved = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert!(saved.on && !saved.crossfeed_on && !saved.flat);
    assert!(control.view().on && !control.view().crossfeed.unwrap().on);
}

#[tokio::test]
async fn the_output_changing_turns_the_stage_off_without_live_updates_too() {
    let (mut control, chain, params) = both_on().await;
    control.live = false;
    params.set_output(Some(sink("b")));
    looks(&mut control, 1).await;
    let xf = control.view().crossfeed.unwrap();
    assert!(!xf.on);
    assert_eq!(xf.off_reason.as_deref(), Some("output-changed"));
    assert!(
        applies(&chain).last().unwrap().crossfeed.is_none(),
        "the chain is applied again without the stage"
    );
}

#[tokio::test]
async fn a_revert_of_the_graph_at_the_same_node_and_rate_is_seen_and_the_stage_sent_again() {
    let (mut control, _, params) = both_on().await;
    // The first look reads the live values back.
    looks(&mut control, 1).await;
    assert!(control.verified, "live updates are verified");
    let sends = params.sent().len();
    // PipeWire rebuilds the graph at the same rate: the stage goes back to the config's bypass.
    params.set_ports(&[("lmix:Gain 2", 0.0), ("rmix:Gain 2", 0.0)]);
    looks(&mut control, 1).await;
    assert!(control.view().crossfeed.unwrap().on, "the view says on");
    let sent = stage_sends(&params, sends);
    assert_eq!(
        mix_of(sent.last().expect("the stage was sent again")),
        Some(1.0)
    );
}

#[tokio::test]
async fn a_chain_that_keeps_reverting_is_not_sent_to_forever() {
    let (mut control, _, params) = both_on().await;
    looks(&mut control, 1).await;
    for _ in 0..8 {
        params.set_ports(&[("lmix:Gain 2", 0.0), ("rmix:Gain 2", 0.0)]);
        looks(&mut control, 1).await;
    }
    let xf = control.view().crossfeed.unwrap();
    assert!(!xf.on, "after a few reverts the crossfeed gives up");
    assert!(xf.note.unwrap().contains("going back"));
}

#[tokio::test]
async fn the_user_turning_crossfeed_off_on_a_flat_chain_gets_no_reset_note() {
    let (mut control, chain, _) = flat_on().await;
    control.crossfeed(off()).await.unwrap();
    assert_eq!(removes(&chain), 1);
    let xf = control.view().crossfeed.unwrap();
    assert_eq!(xf.note, None);
    assert_eq!(xf.off_reason, None);
}

#[tokio::test]
async fn a_chain_replaced_from_outside_after_the_stage_was_sent_to_it_gets_the_stage_taken_off() {
    let (mut control, chain, params) = both_on().await;
    looks(&mut control, 1).await;
    assert!(
        control.stage_on_node,
        "the stage went to the node that is there"
    );
    let sends = params.sent().len();
    // The command line applied a curve of its own over the chain (the state file says so).
    control.adopt(HostEqState {
        on: true,
        name: "Cli".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(1000.0, 0.0, 1.0)],
        slots: vec![1],
        crossfeed: None,
        crossfeed_on: false,
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
    });
    looks(&mut control, 1).await;
    assert!(!control.view().crossfeed.unwrap().on);
    let sent = stage_sends(&params, sends);
    let last = sent.last().expect("the stage was taken off");
    assert_eq!(
        mix_of(last),
        Some(0.0),
        "the stage that was sent to the node is off again"
    );
    assert_eq!(removes(&chain), 0);
}

#[tokio::test]
async fn a_new_controller_takes_a_stage_it_did_not_start_off_the_node() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    // The node plays a stage that an earlier owner sent and a state file never recorded.
    params.set_ports(&[("lmix:Gain 2", 1.0), ("rmix:Gain 2", 1.0)]);
    control.edit(gain(0, 2.0, true)).await.unwrap();
    looks(&mut control, 1).await;
    let sent = params.sent();
    assert!(
        sent.iter().any(|send| mix_of(send) == Some(0.0)),
        "the bypass was sent with the first update"
    );
}

#[tokio::test]
async fn a_config_that_holds_the_stage_keeps_holding_it_when_the_points_are_kept() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    // The live send of the stage is refused: the chain holds it in its config instead.
    params.takes(false);
    control.crossfeed(on()).await.unwrap();
    params.takes(true);
    assert!(control.xf_conf);
    control.edit(gain(0, 1.0, true)).await.unwrap();
    let kept = chain
        .calls()
        .into_iter()
        .rev()
        .find_map(|call| match call {
            ChainCall::KeepConf(request) => Some(request),
            _ => None,
        })
        .expect("the points were kept");
    assert_eq!(kept.crossfeed, Some(control.xf.clone()));
}

/// The running chain's config holds M/S bass at 50 % while crossfeed is off (the
/// apply that would take it out failed, and a save kept the config on disk without it), and the
/// daemon restarts. The state file says so: the new owner pays for that stage, then applies the
/// chain again without it, and the state says no stage is loaded any more.
#[tokio::test]
async fn a_stage_the_running_config_holds_with_crossfeed_off_is_paid_for_and_taken_out() {
    let settings = CrossfeedSettings::for_model(Model::MidSide)
        .change(
            &crate::eq::crossfeed::CrossfeedChange {
                amount_pct: Some(50.0),
                ..crate::eq::crossfeed::CrossfeedChange::default()
            },
            48_000,
        )
        .unwrap();
    let dir = Dir::new();
    HostEqState {
        on: true,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: -2.0,
        filters: vec![pk(1000.0, 2.0, 1.0)],
        slots: vec![1],
        crossfeed: Some(settings.clone()),
        crossfeed_on: false,
        crossfeed_loaded: Some(settings.clone()),
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        loudness: None,
    }
    .save(&dir.state())
    .unwrap();
    let (mut control, chain, params) = setup_over(&dir.state());
    let worst = crate::eq::manage::CONFIG_STAGE_RATES
        .into_iter()
        .map(|worked| worst_at_any_rate(&settings, worked))
        .fold(0.0, f64::max);
    let db = control.view().preamp_db;
    let peak = played_peak(&control);
    // The EQ's preamp and the stage's cost each forgive 0.008 dB.
    assert!(
        peak + db + worst <= 0.02,
        "points {peak:+.3} dB, preamp {db:+.3} dB, the config's stage {worst:+.3} dB"
    );
    params.set_output(Some(sink("a")));
    looks(&mut control, 2).await;
    let last = applies(&chain).pop().expect("applied again");
    assert_eq!(last.crossfeed, None, "without the stage");
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(state.crossfeed_loaded, None);
    assert!(!control.view().crossfeed.unwrap().on);
}

/// A state that says its config holds the stage does not say the rate the stage was worked out
/// at, and Meier at 0.25 ms worked out at 44.1 kHz adds 0.37 dB more than the cost taken at
/// 48 kHz: until the owed reset takes the stage out, the preamp pays for the stage worked out
/// at any usual rate.
#[tokio::test]
async fn an_adopted_config_stage_is_paid_for_at_any_rate_it_was_worked_out_at() {
    use crate::eq::crossfeed::CrossfeedChange;
    use crate::eq::manage::CONFIG_STAGE_RATES;
    let settings = CrossfeedSettings::for_model(Model::Meier)
        .change(
            &CrossfeedChange {
                delay_ms: Some(0.25),
                ..CrossfeedChange::default()
            },
            48_000,
        )
        .unwrap();
    assert!(settings.delay_ms.is_some());
    let dir = Dir::new();
    HostEqState {
        on: true,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(1000.0, 0.0, 1.0)],
        slots: vec![1],
        crossfeed: Some(settings.clone()),
        crossfeed_on: true,
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
    }
    .save(&dir.state())
    .unwrap();
    let (control, _, _) = setup_over(&dir.state());
    let worst = CONFIG_STAGE_RATES
        .into_iter()
        .map(|worked| worst_at_any_rate(&settings, worked))
        .fold(0.0, f64::max);
    let db = control.view().preamp_db;
    // The stage's cost forgives 0.008 dB of rounding.
    assert!(
        db + worst <= 0.008 + 1e-9,
        "preamp {db:+.3} dB, the stage {worst:+.3} dB"
    );
}

/// A config whose stage costs more than the one sent live since holds the
/// preamp down by the difference; once nothing has played for the idle time the chain is applied
/// again with the stage sent live, and the level comes back up.
#[tokio::test]
async fn a_dearer_stage_in_the_config_is_shed_once_idle() {
    let (mut control, chain, params) = setup(Keep::Memory);
    // An apply starts the node again from its config, as a real chain is.
    chain.restarts_node_of(&params);
    params.set_output(Some(sink("a")));
    control.set_idle_ticks(40);
    control.set_on(true).await.unwrap();
    control.crossfeed(model("meier")).await.unwrap();
    control
        .crossfeed(CrossfeedEdit {
            delay_ms: Some(1.0),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    params.takes(false);
    control.crossfeed(on()).await.unwrap();
    params.takes(true);
    assert!(control.xf_conf, "the config holds the stage");
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(7.5),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let held = control.view().preamp_db;
    let before = applies(&chain).len();
    looks(&mut control, 4).await;
    let requests = applies(&chain);
    assert!(requests.len() > before, "applied again once idle");
    assert_eq!(
        requests.last().unwrap().crossfeed,
        None,
        "the stage goes live"
    );
    let db = control.view().preamp_db;
    assert!(
        db > held + 1.0,
        "the level comes back up: {held:+.2} -> {db:+.2}"
    );
    assert!(control.view().crossfeed.unwrap().on);
}

/// The peak of the slots `send` carries, in dB (never below 0): a line carries all sixteen.
fn slots_peak_db(send: &[(String, f64)], rate: u32) -> f64 {
    let sets: Vec<[f64; 5]> = (1..=SLOTS as u8)
        .map(|slot| {
            ["b0", "b1", "b2", "a1", "a2"].map(|port| {
                let held = value(send, &format!("l{slot:02}:{port}")).expect("every slot is sent");
                f64::from(held as f32)
            })
        })
        .collect();
    svolder_profiles::tone::response::played_peak_db_of(&sets, &[], rate).max(0.0)
}

/// The preamp lab's seed 1291: the config holds Meier with a delay (its live send was refused),
/// the amount is turned down live, and a drag makes the curve hotter. A rate change puts the
/// node's stage back to the config's without a word, and a line of the drag before the next look
/// carries the slots and not the stage: its preamp pays for the config's stage too, at every
/// usual rate.
#[tokio::test]
async fn a_line_of_slots_pays_for_the_stage_the_config_holds() {
    let (mut control, chain, params) = setup(Keep::Memory);
    params.set_output(Some(sink("a")));
    control.set_on(true).await.unwrap();
    control.crossfeed(model("meier")).await.unwrap();
    control
        .crossfeed(CrossfeedEdit {
            delay_ms: Some(1.0),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    params.takes(false);
    control.crossfeed(on()).await.unwrap();
    params.takes(true);
    assert!(control.xf_conf, "the config holds the stage");
    let conf = applies(&chain).pop().expect("applied with the stage");
    let conf_rate = conf.rate_hz.unwrap_or(CONFIG_RATE_HZ);
    let conf_stage = worst_at_any_rate(&conf.crossfeed.expect("the stage"), conf_rate);
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(7.5),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let from = params.sent().len();
    for g in [1.0, 3.0, 6.0] {
        control.edit(gain(0, g, false)).await.unwrap();
        control.tick().await;
    }
    control.edit(gain(0, 6.0, true)).await.unwrap();
    let lines: Vec<_> = stage_sends(&params, from)
        .into_iter()
        .filter(|send| mix_of(send).is_none())
        .collect();
    assert!(!lines.is_empty(), "the drag went live");
    for (n, send) in lines.iter().enumerate() {
        let peak = slots_peak_db(send, conf_rate);
        let pre = pre_db(send).expect("every line carries the preamp");
        // The EQ's preamp and the stage's cost each forgive 0.008 dB of rounding.
        assert!(
            peak + pre + conf_stage <= 0.02,
            "line {n}: slots {peak:+.3} dB, preamp {pre:+.3} dB, the config's stage \
             {conf_stage:+.3} dB"
        );
    }
}

#[tokio::test]
async fn a_state_adopted_over_a_stage_sent_to_the_node_says_the_stage_may_still_play() {
    let (mut control, _, _) = both_on().await;
    looks(&mut control, 1).await;
    assert!(control.stage_on_node);
    control.adopt(HostEqState {
        on: true,
        name: "Cli".into(),
        origin: None,
        target: "sink".into(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(1000.0, 0.0, 1.0)],
        slots: vec![1],
        crossfeed: None,
        crossfeed_on: false,
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
    });
    assert!(
        control.owed.as_ref().is_some_and(|owed| owed.plays),
        "the stage that was sent plays until it is taken off"
    );
}

/// The stage's cost never forgives more than float noise: nothing plays above unity by the
/// stage's rounding (the EQ's preamp beside it spends its own 0.008 dB).
#[test]
fn the_stage_cost_is_never_below_its_worst_gain() {
    use crate::eq::crossfeed::Model;
    use crate::eq::manage::stage_cost_db;
    let mut cases = vec![("meier", 14.0, 500.0, 192_000)];
    for model in ["bauer", "mid-side", "meier", "linkwitz"] {
        for amount in [10.0, 14.0, 25.0, 37.0, 50.0] {
            for hz in [60.0, 120.0, 500.0, 700.0, 1500.0] {
                for rate in [44_100, 48_000, 96_000, 192_000] {
                    cases.push((model, amount, hz, rate));
                }
            }
        }
    }
    for (model, amount, hz, rate) in cases {
        let Ok(settings) = CrossfeedSettings::for_model(Model::from_word(model).unwrap()).change(
            &crate::eq::crossfeed::CrossfeedChange {
                amount_pct: Some(amount),
                crossover_hz: Some(hz),
                ..Default::default()
            },
            rate,
        ) else {
            continue;
        };
        let worst = worst_gain_db(&settings, rate);
        let cost = stage_cost_db(&settings, rate);
        assert!(
            cost >= worst - 1e-5,
            "{model} {amount} % {hz} Hz {rate}: cost {cost} < worst {worst}"
        );
    }
}

#[tokio::test]
async fn a_stage_send_is_repeated_once_a_second_later_and_no_more() {
    let (mut control, _, params) = both_on().await;
    let first = params.sent().len();
    let turned_on = params.sent().last().cloned().expect("the stage went out");
    assert_eq!(mix_of(&turned_on), Some(1.0));
    // Nothing goes before the second is up.
    for _ in 0..WATCH_EVERY - 2 {
        control.tick().await;
    }
    assert_eq!(params.sent().len(), first, "no repeat before a second");
    // Then the same values go once more.
    looks(&mut control, 1).await;
    let sent = params.sent();
    assert_eq!(sent.len(), first + 1, "one repeat");
    let again = sent.last().unwrap();
    for (name, value) in turned_on.iter().filter(|(name, _)| !name.contains("pre")) {
        assert_eq!(value_of(again, name), Some(*value), "{name} is the same");
    }
    // And that repeat is not repeated.
    looks(&mut control, 5).await;
    assert_eq!(params.sent().len(), first + 1, "no endless repeats");
}

fn value_of(sent: &[(String, f64)], name: &str) -> Option<f64> {
    value(sent, name)
}

#[tokio::test]
async fn a_failed_repeat_does_not_swallow_the_repeat_of_the_next_change() {
    let (mut control, _, params) = both_on().await;
    // The repeat is due and the send is refused.
    params.takes(false);
    looks(&mut control, 1).await;
    params.takes(true);
    // The user's next change goes out at once; it earns its own repeat a second later.
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(20.0),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let after_change = params.sent().len();
    let changed = params.sent().last().cloned().unwrap();
    looks(&mut control, 1).await;
    let sent = params.sent();
    assert!(sent.len() > after_change, "the change was repeated");
    let again = sent.last().unwrap();
    assert_eq!(value_of(again, "lx:b0"), value_of(&changed, "lx:b0"));
}

#[tokio::test]
async fn an_eq_edit_is_repeated_once_a_second_later_with_the_latest_values() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 4.0, true)).await.unwrap();
    let first = params.sent().len();
    let sent = params.sent().last().cloned().unwrap();
    // A later drag value within the second is what the repeat carries.
    control.edit(gain(0, 2.0, false)).await.unwrap();
    control.tick().await;
    let moved = params.sent().len();
    assert_eq!(moved, first + 1, "the drag value went out on the tick");
    looks(&mut control, 1).await;
    let all = params.sent();
    assert_eq!(all.len(), moved + 1, "one repeat");
    let again = all.last().unwrap();
    let latest = &all[moved - 1];
    for (name, value) in latest {
        assert_eq!(value_of(again, name), Some(*value), "{name} is the latest");
    }
    assert!(
        pre_db(again).unwrap() <= pre_db(&sent).unwrap() + 1e-9 || pre_db(again) == pre_db(latest)
    );
    looks(&mut control, 5).await;
    assert_eq!(params.sent().len(), moved + 1, "no endless repeats");
}

#[tokio::test]
async fn a_value_waiting_when_the_repeat_falls_due_gets_a_repeat_of_its_own() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 4.0, true)).await.unwrap();
    // The repeat of that send falls due on tick 20; a drag value waits for that very tick.
    for _ in 0..WATCH_EVERY - 1 {
        control.tick().await;
    }
    control.edit(gain(0, 2.0, false)).await.unwrap();
    control.tick().await;
    let after = params.sent().len();
    looks(&mut control, 1).await;
    assert!(
        params.sent().len() > after,
        "the value that went out on the due tick is repeated a second later"
    );
}

/// A slot's port the controller has sent a value for.
fn slot_port(control: &EqControl) -> (String, f64) {
    let mut ports: Vec<_> = control
        .last_values
        .iter()
        .filter(|(name, _)| name.starts_with("l0") && name.contains(":b0"))
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    ports.sort_by(|a, b| a.0.cmp(&b.0));
    ports.into_iter().next().expect("a slot port was sent")
}

#[tokio::test]
async fn a_revert_of_one_slot_at_the_same_node_and_rate_is_seen_while_crossfeed_plays() {
    let (mut control, _, params) = both_on().await;
    looks(&mut control, 2).await;
    assert!(control.verified, "live updates are verified");
    let (port, wanted) = slot_port(&control);
    let sends = params.sent().len();
    // PipeWire rebuilt the EQ's slots alone: this one went back to its config's value.
    params.set_ports(&[(port.as_str(), wanted + 1.0)]);
    looks(&mut control, 1).await;
    let again = stage_sends(&params, sends);
    assert!(
        again.iter().any(|send| send
            .iter()
            .any(|(name, value)| *name == port && *value == wanted)),
        "the slot was sent again: {again:?}"
    );
}

#[tokio::test]
async fn a_send_that_pw_cli_applies_three_looks_late_is_not_sent_again_as_a_revert() {
    let (mut control, _, params) = both_on().await;
    looks(&mut control, 2).await;
    assert!(control.verified);
    // The node keeps what it held while the edit's line is on its way.
    params.holds(false);
    control.edit(gain(0, 2.0, true)).await.unwrap();
    let after_edit = params.sent().len();
    looks(&mut control, 3).await;
    // The edit's own repeat, a second after it, and one line more once a look a second after
    // the line still finds it off (a rebuild before any look looks the same) are the only other
    // sends.
    assert!(
        params.sent().len() <= after_edit + 2,
        "{} sends after the edit's {after_edit}",
        params.sent().len() - after_edit
    );
    assert_eq!(control.reverts.len(), 0, "no revert was counted");
    let settled = params.sent().len();
    looks(&mut control, 3).await;
    assert_eq!(
        params.sent().len(),
        settled,
        "the late line is not sent again and again"
    );
    assert_eq!(control.reverts.len(), 0, "no revert was counted");
}

#[tokio::test]
async fn a_revert_that_a_gated_look_sees_first_is_still_counted_at_the_next_look() {
    let (mut control, _, params) = both_on().await;
    looks(&mut control, 3).await;
    while control.ticks % 20 != 17 {
        control.tick().await;
    }
    // The edit's repeat falls 20 ticks later, in the 4 ticks before a look.
    control.edit(gain(0, -2.0, false)).await.unwrap();
    for _ in 0..21 {
        control.tick().await;
    }
    let (port, wanted) = slot_port(&control);
    let sends = params.sent().len();
    params.set_ports(&[(port.as_str(), wanted + 1.0), ("lpre:Mult", 0.5)]);
    looks(&mut control, 4).await;
    let again = stage_sends(&params, sends);
    assert!(
        again.iter().any(|send| send
            .iter()
            .any(|(name, value)| *name == port && *value == wanted)),
        "the slot was sent again: {again:?}"
    );
}

/// The preamp port of the left channel, in dB, as the latest send left it.
fn sent_preamp_db(control: &EqControl) -> f64 {
    20.0 * control.last_values["lpre:Mult"].log10()
}

/// Every port of slot 1 on both channels, at the value last sent to it.
fn slot_one(control: &EqControl) -> Vec<(String, f64)> {
    let mut ports: Vec<_> = control
        .last_values
        .iter()
        .filter(|(name, _)| name.starts_with("l01:") || name.starts_with("r01:"))
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    ports.sort_by(|a, b| a.0.cmp(&b.0));
    ports
}

/// PipeWire rebuilt the graph at the same node and rate: every port of slot 1 holds `ports`.
fn put_back(params: &FakeParams, ports: &[(String, f64)]) {
    let ports: Vec<(&str, f64)> = ports
        .iter()
        .map(|(name, value)| (name.as_str(), *value))
        .collect();
    params.set_ports(&ports);
}

/// Whether a send since `from` carried slot 1's `b0` at `wanted`.
fn sent_again(params: &FakeParams, from: usize, wanted: f64) -> bool {
    stage_sends(params, from).iter().any(|send| {
        send.iter()
            .any(|(name, value)| name == "l01:b0" && *value == wanted)
    })
}

#[tokio::test]
async fn a_slot_that_goes_back_before_a_look_saw_it_is_sent_again_once_its_line_is_seen_to_land() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, 2.0, true)).await.unwrap();
    looks(&mut control, 2).await;
    assert!(control.verified, "live updates are verified");
    let before = slot_one(&control);
    let preamp_before = sent_preamp_db(&control);
    // One line moves the slot and the preamp; it lands, and the graph is rebuilt at the same
    // node and rate before any look: every port of the slot goes back, the preamp keeps what
    // was sent.
    control.edit(gain(0, 8.0, true)).await.unwrap();
    let wanted = control.last_values["l01:b0"];
    assert!(slot_one(&control) != before && sent_preamp_db(&control) < preamp_before - 1.0);
    put_back(&params, &before);
    looks(&mut control, 1).await;
    // It goes back again right after the line that put it right, before the next look.
    put_back(&params, &before);
    let sends = params.sent().len();
    looks(&mut control, 2).await;
    assert!(
        sent_again(&params, sends, wanted),
        "the slot was sent again"
    );
    assert!(
        !control.reverts.is_empty(),
        "the moved preamp shows a revert"
    );
}

#[tokio::test]
async fn a_cut_that_goes_back_twice_before_a_look_saw_it_is_sent_again() {
    let (mut control, _, params) = setup(Keep::Memory);
    control.set_on(true).await.unwrap();
    control.edit(gain(0, -2.0, true)).await.unwrap();
    looks(&mut control, 2).await;
    assert!(control.verified, "live updates are verified");
    while control.ticks % WATCH_EVERY != 0 {
        control.tick().await;
    }
    let before = slot_one(&control);
    let preamp_before = sent_preamp_db(&control);
    // A deeper cut leaves the preamp where it was: no port of its line survives a rebuild.
    control.edit(gain(0, -6.0, true)).await.unwrap();
    let wanted = control.last_values["l01:b0"];
    assert!(slot_one(&control) != before);
    assert!((sent_preamp_db(&control) - preamp_before).abs() < 1e-9);
    // The line lands and the graph is rebuilt before the next look; the repeat puts it right
    // after that look, and a second rebuild comes before the one after.
    put_back(&params, &before);
    for _ in 0..WATCH_EVERY {
        control.tick().await;
    }
    put_back(&params, &before);
    let sends = params.sent().len();
    looks(&mut control, 5).await;
    assert!(
        sent_again(&params, sends, wanted),
        "the slot was sent again"
    );
    assert!(control.reverts.is_empty(), "nothing showed a revert");
}

#[tokio::test]
async fn a_restart_or_a_rate_change_forgets_what_the_old_graph_held() {
    let (mut control, _, params) = both_on().await;
    looks(&mut control, 2).await;
    assert!(!control.confirmed.is_empty());
    params.set_node(Some(node(77, Some(48_000))));
    for _ in 0..WATCH_EVERY {
        control.tick().await;
        if control.sent.node.is_some_and(|node| node.id == 77) {
            assert!(control.confirmed.is_empty(), "{:?}", control.confirmed);
            return;
        }
    }
    panic!("the new node was not looked at");
}
