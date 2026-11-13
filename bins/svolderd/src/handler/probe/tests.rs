// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::time::Duration;

use svolder_ipc::config::DeviceConfig;
use svolder_ipc::{Config, Connect, ConnectFailure, JOB_STOP_BOUND, WRITE_SETTLE_BOUND};
use tokio::time::Instant;

use super::super::tests::{amp_entry, daemon};
use super::super::{Daemon, SHUTDOWN_DEADLINE};
use crate::device::DeviceHandle;
use crate::locks::{read, written};

/// The unit of the fake device on the air, whose code is 1234.
const ON_AIR: &str = "0102030405060708";
/// A unit nothing on the air answers to.
const ABSENT: &str = "1112131415161718";

fn probe_config(unit: &str) -> DeviceConfig {
    DeviceConfig {
        name: format!("probe {unit}"),
        ..amp_entry(unit)
    }
}

fn probe_of(daemon: &Daemon) -> Option<Arc<DeviceHandle>> {
    read(&daemon.devices).probe.clone()
}

/// The daemon's stop asks a probe that is still running to close with the other actors, within
/// the stop's bound.
#[tokio::test(start_paused = true)]
async fn the_stop_closes_a_running_probe_within_its_bound() {
    let daemon = daemon();
    let (probe, _first) = daemon
        .start_probe(probe_config(ABSENT))
        .expect("the probe starts");
    let started = Instant::now();
    daemon.shutdown().await;
    assert!(
        started.elapsed() <= JOB_STOP_BOUND + SHUTDOWN_DEADLINE + WRITE_SETTLE_BOUND,
        "{:?}",
        started.elapsed()
    );
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert!(probe.commands.is_closed(), "the probe's actor still runs");
}

/// Once the stop's mark is set no probe starts.
#[tokio::test(start_paused = true)]
async fn the_stop_mark_refuses_a_probe() {
    let daemon = daemon();
    written(&daemon.devices).stopping = true;
    let refused = daemon.start_probe(probe_config(ON_AIR)).err();
    assert_eq!(refused, Some(ConnectFailure::Stopping));
    assert!(probe_of(&daemon).is_none());
    let tested = daemon.probe(ON_AIR, "1234", std::future::pending()).await;
    assert_eq!(
        tested,
        Some(Connect::Failed {
            kind: ConnectFailure::Stopping
        })
    );
}

/// A reload starts no actor for the unit the probe holds.
#[tokio::test(start_paused = true)]
async fn a_reload_starts_no_actor_for_the_probe_s_unit() {
    let daemon = daemon();
    let (_probe, _first) = daemon
        .start_probe(probe_config(ABSENT))
        .expect("the probe starts");
    let config = Config {
        devices: vec![amp_entry(ABSENT)],
        ..Config::default()
    };
    let running = daemon.reconcile(&config).await;
    assert!(running.is_empty(), "{running:?}");
    assert!(daemon.devices().is_empty());
    daemon.shutdown().await;
}

/// A reload that meets the unit the probe holds starts no actor for it, and the probe's end
/// starts it: the daemon runs every unit the configuration names once the slot is free.
#[tokio::test(start_paused = true)]
async fn the_probe_s_end_starts_the_actor_a_reload_left_out() {
    let daemon = daemon();
    let config = Config {
        devices: vec![amp_entry(ABSENT)],
        ..Config::default()
    };
    let probing = daemon.probe(ABSENT, "auto", std::future::pending());
    let reloading = async {
        // paused time moves on while everything waits, so a probe that never registers panics
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while probe_of(&daemon).is_none() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "never came: the probe to register"
            );
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        daemon.hold_snapshot(config.clone(), 1, None);
        let running = daemon.reconcile(&config).await;
        assert!(running.is_empty(), "the reload met the probe: {running:?}");
    };
    let (tested, ()) = tokio::join!(probing, reloading);
    assert!(matches!(tested, Some(Connect::Failed { .. })), "{tested:?}");
    assert!(probe_of(&daemon).is_none());
    let names: Vec<_> = daemon.devices().iter().map(|d| d.name.clone()).collect();
    assert_eq!(names, ["amp"], "the probe's end started the unit's actor");
    daemon.shutdown().await;
}

/// A probe's end starts nothing a reload did not leave out: a unit that has its actor is left
/// alone, and so is a unit no configuration names.
#[tokio::test(start_paused = true)]
async fn the_probe_s_end_starts_only_the_units_without_an_actor() {
    let daemon = daemon();
    let config = Config {
        devices: vec![amp_entry(ABSENT)],
        ..Config::default()
    };
    daemon.hold_snapshot(config.clone(), 1, None);
    assert_eq!(daemon.reconcile(&config).await, ["amp"]);
    let before = daemon.devices();
    let _ = daemon.probe(ON_AIR, "1234", std::future::pending()).await;
    let after = daemon.devices();
    assert_eq!(after.len(), 1);
    assert!(Arc::ptr_eq(&before[0], &after[0]), "the actor was kept");
    daemon.shutdown().await;
}

/// A probe is refused a unit an actor holds.
#[tokio::test(start_paused = true)]
async fn a_probe_is_refused_a_unit_an_actor_holds() {
    let daemon = daemon();
    let config = Config {
        devices: vec![amp_entry(ABSENT)],
        ..Config::default()
    };
    assert_eq!(daemon.reconcile(&config).await, ["amp"]);
    let refused = daemon.start_probe(probe_config(ABSENT)).err();
    assert_eq!(refused, Some(ConnectFailure::Held));
    assert!(probe_of(&daemon).is_none());
    daemon.shutdown().await;
}

/// The right code is accepted, a wrong one refused, and either way the probe has closed and
/// left the slot by the time the answer is in.
#[tokio::test(start_paused = true)]
async fn the_right_code_is_accepted_a_wrong_one_refused_and_the_probe_goes() {
    let daemon = daemon();
    for (code, answer) in [("4321", Connect::Refused), ("1234", Connect::Accepted)] {
        let tested = daemon.probe(ON_AIR, code, std::future::pending()).await;
        assert_eq!(tested, Some(answer), "{code}");
        assert!(probe_of(&daemon).is_none(), "{code}: the slot is empty");
    }
}

/// A unit nothing answers to is not in range.
#[tokio::test(start_paused = true)]
async fn a_unit_nobody_hears_is_not_in_range() {
    let daemon = daemon();
    let tested = daemon.probe(ABSENT, "auto", std::future::pending()).await;
    assert_eq!(
        tested,
        Some(Connect::Failed {
            kind: ConnectFailure::NotInRange
        })
    );
    assert!(probe_of(&daemon).is_none());
}

/// A test that is cancelled before its first attempt ends answers nothing, and its probe
/// closes and leaves the slot.
#[tokio::test(start_paused = true)]
async fn a_cancelled_test_closes_its_probe() {
    let daemon = daemon();
    let tested = daemon.probe(ABSENT, "auto", std::future::ready(())).await;
    assert_eq!(tested, None);
    assert!(probe_of(&daemon).is_none());
}
