// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's bypass and levels through `get_host` and `set_host_bypass`.

use super::*;

/// A computer view whose chain plays at -14.8 dB, with the bypass `on` holding the EQ and the
/// loudness, or off.
fn bypass_view(on: bool) -> svolder_ipc::host::HostView {
    use svolder_host::eq::{BypassView, ChainLevels, HeldStages};
    let mut view = computer_view();
    let eq = view.eq.as_mut().unwrap();
    eq.levels = Some(ChainLevels::of(true, -7.8, -7.0, -14.8, false));
    eq.bypass = Some(BypassView {
        on,
        held: HeldStages {
            eq: on,
            loudness: on,
            ..HeldStages::default()
        },
    });
    view
}

#[tokio::test]
async fn get_host_carries_the_bypass_and_the_levels() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = bypass_view(true);
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(
        reply["bypass"],
        json!({"on": true, "held": {"eq": true, "loudness": true, "crossfeed": false, "speakers": false}})
    );
    assert_eq!(reply["levels"]["total_db"], -14.8);
    assert_eq!(reply["levels"]["words"], "plays \u{2212}14.8 dB in all");
    assert_eq!(reply["levels"]["untouched"], false);
    // A daemon that has neither shows null.
    *fleet.host.lock().unwrap() = computer_view();
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["bypass"], Value::Null);
    assert_eq!(reply["levels"], Value::Null);
}

#[tokio::test]
async fn set_host_bypass_sends_one_op_never_asks_and_answers_with_the_view() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = bypass_view(true);
    // A caller that cannot be asked goes through: the bypass never asks.
    let reply = run(
        &fleet,
        None,
        call("set_host_bypass", json!({"on": true})),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    let answer = structured(&reply);
    assert_eq!(answer["bypass"]["on"], true);
    assert_eq!(answer["levels"]["words"], "plays \u{2212}14.8 dB in all");
    assert_eq!(
        host_sent(&fleet),
        [Request::HostEq {
            op: HostEqOp::Bypass { on: true }
        }]
    );
    let reply = run(
        &fleet,
        None,
        call("set_host_bypass", json!({"on": false})),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    assert_eq!(
        host_sent(&fleet)[1],
        Request::HostEq {
            op: HostEqOp::Bypass { on: false }
        }
    );
}

#[tokio::test]
async fn set_host_bypass_needs_on_and_nothing_else() {
    let fleet = computer_fleet(20);
    for bad in [
        json!({}),
        json!({"on": "yes"}),
        json!({"on": true, "off": true}),
    ] {
        let reply = run(&fleet, None, call("set_host_bypass", bad), false).await;
        assert!(is_error(&reply));
    }
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn set_host_bypass_passes_the_controllers_refusal_on() {
    let fleet = computer_fleet(20);
    *fleet.host_error.lock().unwrap() = Some("nothing plays on the computer to bypass".into());
    let reply = run(
        &fleet,
        None,
        call("set_host_bypass", json!({"on": true})),
        false,
    )
    .await;
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("nothing plays on the computer to bypass"));
}

#[tokio::test]
async fn the_bypass_tool_is_listed_only_with_writes_and_refused_without() {
    let fleet = scripted("mesh-eq-5", &[]);
    assert!(!listed(&fleet, false).contains(&"set_host_bypass".to_string()));
    assert!(listed(&fleet, true).contains(&"set_host_bypass".to_string()));
    let tools = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, false, None);
    let reply = tools
        .call(
            call("set_host_bypass", json!({"on": true})),
            Caller { can_ask: false },
        )
        .await
        .unwrap();
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("[mcp] writes"));
    assert!(host_sent(&fleet).is_empty());
}
