// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A chain that nothing has played through yet: PipeWire lists every control of a graph that has
//! not run at 0, whatever its config holds or was sent, and the graph starts from its config when
//! it runs (what was sent before is lost). Such a look bounds no preamp and checks no send; the
//! first look at the graph run sends everything again.

use super::*;

/// A fake whose node is a chain that has not run: its slots, preamp and stage list 0, and a send
/// is taken but not seen.
fn idle_params() -> FakeParams {
    let params = FakeParams::new();
    let mut ports: Vec<(String, f64)> = crate::eq::live::preamp_params(0.0);
    for slot in 1..=SLOTS as u8 {
        ports.extend(crate::eq::live::slot_params(slot, None, 48_000));
    }
    ports.extend(crate::eq::live::crossfeed_params(
        &crate::eq::CrossfeedStage::bypass(),
    ));
    let zeros: Vec<(&str, f64)> = ports.iter().map(|(name, _)| (name.as_str(), 0.0)).collect();
    params.set_ports(&zeros);
    params.holds(false);
    params
}

/// What a flat chain's config holds, its preamp at `pre_db`: every slot the identity, the stage off.
fn flat_config(pre_db: f64) -> Vec<(String, f64)> {
    let mut ports = crate::eq::live::preamp_params(pre_db);
    for slot in 1..=SLOTS as u8 {
        ports.extend(crate::eq::live::slot_params(slot, None, 48_000));
    }
    ports.extend(crate::eq::live::crossfeed_params(
        &crate::eq::CrossfeedStage::bypass(),
    ));
    ports
}

/// The graph runs: it holds what its config holds (nothing sent before is on it), and from now on
/// what is sent.
fn runs(params: &FakeParams, config: &[(String, f64)]) {
    let ports: Vec<(&str, f64)> = config.iter().map(|(name, v)| (name.as_str(), *v)).collect();
    params.set_ports(&ports);
    params.holds(true);
}

fn controller(params: &FakeParams) -> EqControl {
    EqControl::new(
        Arc::new(FakeChain::new()),
        Box::new(params.clone()),
        Keep::Memory,
    )
}

async fn looks(control: &mut EqControl, seconds: u64) {
    for _ in 0..seconds * WATCH_EVERY {
        control.tick().await;
    }
}

fn pre_db(sent: &[(String, f64)]) -> Option<f64> {
    value(sent, "lpre:Mult").map(|mult| 20.0 * mult.log10())
}

fn last_pre_db(params: &FakeParams) -> f64 {
    let sent = params.sent();
    let last = sent.iter().rev().find_map(|send| pre_db(send));
    last.expect("a preamp was sent")
}

/// Crossfeed on, on a model whose stage takes a preamp of its own.
fn on() -> CrossfeedEdit {
    CrossfeedEdit {
        on: Some(true),
        model: Some("meier".to_string()),
        done: true,
        ..CrossfeedEdit::default()
    }
}

/// Where `name` is in `send`.
fn at(send: &[(String, f64)], name: &str) -> Option<usize> {
    send.iter().position(|(key, _)| key == name)
}

#[tokio::test]
async fn crossfeed_turned_on_over_a_chain_that_has_not_run_plays_at_the_stages_preamp() {
    let params = idle_params();
    let mut control = controller(&params);
    control.crossfeed(on()).await.unwrap();
    looks(&mut control, 3).await;
    let want = -control.stage_gain();
    assert!(
        want < -0.1,
        "the stage takes a preamp of its own ({want} dB)"
    );
    let view = control.view();
    assert!(
        (view.preamp_db - want).abs() < 1e-6,
        "the view says {} dB, the stage wants {want} dB",
        view.preamp_db
    );
    let db = last_pre_db(&params);
    assert!(
        (db - want).abs() < 1e-6,
        "{db} dB sent, the stage wants {want} dB"
    );
    assert!(
        view.live,
        "the zeros are no failed read back ({:?})",
        view.note
    );
    // A tone starts: the graph runs from its config, flat and without the stage, and is sent the
    // stage and the preamp again.
    let before = params.sent().len();
    runs(&params, &flat_config(0.0));
    looks(&mut control, 3).await;
    let again = &params.sent()[before..];
    assert!(
        again
            .iter()
            .any(|send| value(send, "lmix:Gain 2") == Some(1.0)
                && pre_db(send).is_some_and(|db| (db - want).abs() < 1e-6)
                && at(send, "lpre:Mult") < at(send, "lmix:Gain 2")),
        "the stage and its preamp, which falls first, go again once the graph runs: {again:?}"
    );
    let view = control.view();
    assert!(
        (view.preamp_db - want).abs() < 1e-6,
        "{} dB once it runs",
        view.preamp_db
    );
    assert!(view.live && view.crossfeed.unwrap().on, "{:?}", view.note);
}

#[tokio::test]
async fn an_edit_over_a_chain_that_has_not_run_plays_at_its_own_preamp() {
    let params = idle_params();
    let mut control = controller(&params);
    control.set_on(true).await.unwrap();
    looks(&mut control, 2).await;
    control.edit(gain(0, -3.0, true)).await.unwrap();
    looks(&mut control, 3).await;
    let db = last_pre_db(&params);
    assert!(db > -1.0, "{db} dB sent for a -3 dB cut");
    let view = control.view();
    assert!((view.preamp_db - db).abs() < 1e-6);
    assert!(
        view.live,
        "the zeros are no failed read back ({:?})",
        view.note
    );
    // A tone starts: the graph runs from its config (flat) and is sent the points again.
    let before = params.sent().len();
    runs(&params, &flat_config(0.0));
    looks(&mut control, 3).await;
    let again = &params.sent()[before..];
    assert!(
        again
            .iter()
            .any(|send| (1..=16).all(|slot| touches(send, slot))
                && send
                    .iter()
                    .any(|(name, v)| name.ends_with(":b0") && name.starts_with('l') && *v != 1.0)
                && pre_db(send).is_some_and(|pre| (pre - db).abs() < 1e-6)),
        "the cut and its preamp go again once the graph runs: {again:?}"
    );
    assert!(control.view().live, "{:?}", control.view().note);
}

/// A chain that had not run when it was first looked at, and runs holding a preamp nobody sent
/// (another process started it): that preamp bounds what is sent, as for any chain found.
#[tokio::test]
async fn the_preamp_a_chain_runs_with_bounds_the_sends_once_it_has_run() {
    let params = idle_params();
    let mut control = controller(&params);
    control.set_on(true).await.unwrap();
    looks(&mut control, 2).await;
    runs(&params, &flat_config(-12.0));
    looks(&mut control, 2).await;
    control.edit(gain(0, 3.0, true)).await.unwrap();
    looks(&mut control, 1).await;
    let db = last_pre_db(&params);
    assert!(db <= -11.9, "{db} dB sent to a node that runs at -12 dB");
}

/// A boost sent before the graph ran is not on it: the graph starts from its config (0 dB), so the
/// preamp of a smaller boost sent once it runs falls, and goes before the slots.
#[tokio::test]
async fn the_first_sends_to_a_graph_that_runs_order_the_preamp_against_its_config() {
    let params = idle_params();
    let mut control = controller(&params);
    control.set_on(true).await.unwrap();
    looks(&mut control, 2).await;
    control.edit(gain(0, 6.0, true)).await.unwrap();
    control.tick().await;
    let before = params.sent().len();
    runs(&params, &flat_config(0.0));
    control.edit(gain(0, 3.0, true)).await.unwrap();
    looks(&mut control, 2).await;
    let after = &params.sent()[before..];
    assert!(!after.is_empty());
    for send in after {
        let pre = at(send, "lpre:Mult").expect("the preamp goes");
        let first_slot = send.iter().position(|(name, _)| name.starts_with("l01:"));
        assert!(
            pre_db(send).unwrap() < -2.9 && first_slot.is_some_and(|slot| pre < slot),
            "the falling preamp goes first: {send:?}"
        );
    }
}

/// A boost sent in the moment between the graph's first run and the look that finds it run is
/// this controller's own preamp, not the bound of the config: softened, the preamp comes back.
#[tokio::test]
async fn a_send_that_lands_before_the_run_look_is_no_bound() {
    let params = idle_params();
    let mut control = controller(&params);
    control.set_on(true).await.unwrap();
    looks(&mut control, 2).await;
    runs(&params, &flat_config(0.0));
    control.edit(gain(0, 6.0, true)).await.unwrap();
    control.tick().await;
    assert!(last_pre_db(&params) < -5.9, "the boost is under its preamp");
    looks(&mut control, 2).await;
    control.edit(gain(0, 0.0, true)).await.unwrap();
    looks(&mut control, 1).await;
    let db = last_pre_db(&params);
    assert!(db.abs() < 1e-6, "{db} dB sent for a flat curve");
    assert!(control.view().preamp_db.abs() < 1e-6);
}

/// Another process replaces a chain whose graph had not run with one that runs a boost under the
/// very preamp this controller sent the old one in a drag (and lost): that preamp is the new chain's own,
/// and bounds what is sent to it.
#[tokio::test]
async fn a_restart_onto_a_chain_that_holds_a_preamp_we_sent_keeps_its_bound() {
    let params = idle_params();
    let mut control = controller(&params);
    control.set_on(true).await.unwrap();
    looks(&mut control, 2).await;
    control.edit(gain(0, 6.0, false)).await.unwrap();
    looks(&mut control, 1).await;
    let sent = params.sent();
    let lost = value(sent.last().unwrap(), "lpre:Mult").unwrap();
    assert!(20.0 * lost.log10() < -5.9, "the boost is under its preamp");
    let mut other = node(9999, Some(48_000));
    other.params = flat_config(20.0 * lost.log10()).into_iter().collect();
    other.params.extend(crate::eq::live::slot_params(
        1,
        Some(&pk(1000.0, 12.0, 1.0)),
        48_000,
    ));
    params.set_node(Some(other));
    params.holds(true);
    looks(&mut control, 2).await;
    control.edit(gain(0, 0.0, true)).await.unwrap();
    looks(&mut control, 1).await;
    let db = last_pre_db(&params);
    assert!(
        db <= -5.9,
        "{db} dB sent to a chain that runs a +12 dB boost under -6 dB"
    );
}

#[test]
fn a_graph_that_lists_every_slot_at_0_has_not_run() {
    let slots = |b0: f64| -> HashMap<String, f64> {
        (1..=SLOTS as u8)
            .flat_map(|slot| crate::eq::live::slot_params(slot, None, 48_000))
            .map(|(name, value)| (name.clone(), if name.ends_with(":b0") { b0 } else { value }))
            .collect()
    };
    assert!(!graph_ran(&slots(0.0)));
    assert!(graph_ran(&slots(1.0)));
    let mut one = slots(0.0);
    one.insert("r16:b0".to_string(), 0.5);
    assert!(graph_ran(&one));
    assert!(graph_ran(&HashMap::new()), "no slot listed tells nothing");
}

/// Loudness turned on over a chain that has not run: nothing is sent to the graph until the look
/// that finds it run, and the resend that look makes carries the stage.
#[tokio::test]
async fn loudness_over_a_chain_that_has_not_run_goes_with_the_run_looks_resend() {
    let params = idle_params();
    let zeros: Vec<(String, f64)> =
        crate::eq::live::loudness_params(&crate::eq::loudness::Stage::identity().sections)
            .into_iter()
            .map(|(name, _)| (name, 0.0))
            .collect();
    let ports: Vec<(&str, f64)> = zeros.iter().map(|(name, v)| (name.as_str(), *v)).collect();
    params.set_ports(&ports);
    params.set_level(Some(crate::route::SinkLevel {
        linear: 0.125,
        muted: false,
    }));
    let mut control = controller(&params);
    control
        .loudness(LoudnessEdit {
            on: Some(true),
            done: true,
            ..LoudnessEdit::default()
        })
        .await
        .unwrap();
    looks(&mut control, 3).await;
    assert!(
        params.sent().is_empty(),
        "nothing goes to a graph that has not run"
    );
    let mut config = flat_config(0.0);
    config.extend(crate::eq::live::loudness_params(
        &crate::eq::loudness::Stage::identity().sections,
    ));
    runs(&params, &config);
    looks(&mut control, 1).await;
    let first = params
        .sent()
        .first()
        .cloned()
        .expect("the run look's resend");
    let identity =
        crate::eq::live::loudness_params(&crate::eq::loudness::Stage::identity().sections);
    let carried: Vec<(String, f64)> = first
        .iter()
        .filter(|(name, _)| name.contains("ld"))
        .cloned()
        .collect();
    assert_eq!(carried.len(), identity.len(), "the stage rides the resend");
    assert_ne!(carried, identity, "a stage, not the identity");
}

/// A node that takes lines but never applies them, over a graph that has not run: a drag away
/// from the starting gain and back through it sends the config's own values, which the node
/// holds once it runs. That is no proof that a line landed, and live updates are given up.
#[tokio::test]
async fn values_sent_before_the_graph_ran_do_not_prove_a_dead_node_takes_live_lines() {
    let params = idle_params();
    let mut control = controller(&params);
    control.set_on(true).await.unwrap();
    looks(&mut control, 2).await;
    for g in [3.0, 6.0, 3.0, 0.0, -3.0] {
        control.edit(gain(0, g, g == -3.0)).await.unwrap();
        control.tick().await;
    }
    looks(&mut control, 1).await;
    runs(&params, &flat_config(0.0));
    params.holds(false);
    looks(&mut control, 4).await;
    assert!(
        !control.view().live,
        "a node that applies nothing is given up (live: {:?})",
        control.view().note
    );
}
