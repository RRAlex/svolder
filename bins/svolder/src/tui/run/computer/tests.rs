// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::time::Duration;

use svolder_host::eq::Keep;
use svolder_host::fake::{ChainCall, FakeChain, FakeParams};

use super::super::test_support::on_a_daemon;
use super::*;

/// How long a case waits for what it expects: far more than it needs.
const WITHIN: Duration = Duration::from_secs(10);

/// A point's `Set` of its gain, as a key sends it.
fn gain(index: u8, g: f64) -> HostEqOp {
    HostEqOp::Set {
        index,
        slot: None,
        kind: None,
        f: None,
        g: Some(g),
        q: None,
        done: true,
    }
}

/// With no daemon the loop owns a controller of its own: a mode and an edit reach it in order,
/// its view comes back on its watch channel, and a refusal comes back in words. Fakes only.
#[tokio::test]
async fn a_direct_tui_drives_its_own_controller() {
    let chain = Arc::new(FakeChain::new());
    let control = EqControl::new(chain.clone(), Box::new(FakeParams::new()), Keep::Memory);
    let mut link = ComputerLink::over(control);
    assert_eq!(
        link.view().map(|view| view.on),
        Some(false),
        "making the link turns nothing on"
    );
    assert!(chain.calls().is_empty(), "and writes nothing");

    link.direct(HostEqOp::Mode { on: true })
        .expect("a controller");
    link.direct(gain(1, 3.0)).expect("a controller");
    let seen = tokio::time::timeout(WITHIN, async {
        loop {
            let view = next_view(&mut link.views).await;
            if view.on && view.points.first().is_some_and(|point| point.g == 3.0) {
                return view;
            }
        }
    })
    .await
    .expect("the view says the change");
    assert!(seen.on);
    assert!(
        chain
            .calls()
            .iter()
            .any(|call| matches!(call, ChainCall::Apply(_))),
        "turning it on applied the chain once"
    );

    link.direct(gain(0, 1.0)).expect("a controller");
    let answer = tokio::time::timeout(WITHIN, answered(&mut link.answers))
        .await
        .expect("a refusal comes back");
    assert!(
        matches!(&*answer, Answer::Said(why) if why.contains("count from 1")),
        "{answer:?}"
    );
}

/// With no controller the change says why and goes nowhere.
#[test]
fn a_link_with_no_controller_says_why() {
    let mut link = ComputerLink::none();
    assert_eq!(
        link.direct(HostEqOp::Mode { on: true }),
        Err("computer controls unavailable".to_string())
    );
}

/// Joined to a daemon, every change of the computer is a request to it, in the order it was asked
/// for, and its refusal comes back in its own words.
#[tokio::test]
async fn a_joined_tui_sends_every_computer_write_to_the_daemon() {
    let (daemon, _path, mut watching) = on_a_daemon().await;
    let ops = [
        HostEqOp::Mode { on: true },
        gain(1, 2.0),
        HostEqOp::Add {
            kind: "PK".into(),
            f: 1000.0,
            g: 0.0,
            q: 1.0,
        },
        HostEqOp::Remove {
            index: 2,
            slot: None,
        },
    ];
    for op in ops.clone() {
        eq(&mut watching, op);
    }
    tokio::time::timeout(WITHIN, async {
        while daemon.host_eq_ops().len() < ops.len() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the daemon got every request");
    assert_eq!(daemon.host_eq_ops(), ops, "in order");
    assert!(
        watching.computer.control.is_none(),
        "a joined loop owns no controller"
    );

    daemon.refuses_eq("computer controls unavailable: no pw-cli on this computer");
    eq(&mut watching, HostEqOp::Mode { on: false });
    let answer = tokio::time::timeout(WITHIN, answered(&mut watching.computer.answers))
        .await
        .expect("the refusal comes back");
    assert_eq!(
        *answer,
        Answer::Said("computer controls unavailable: no pw-cli on this computer".into())
    );
    watching.close().await;
}

/// A queued change that records its number when it runs, and answers nothing.
fn recording(log: &Arc<std::sync::Mutex<Vec<u32>>>, n: u32) -> Work {
    let log = Arc::clone(log);
    Box::pin(async move {
        log.lock().expect("the log").push(n);
        None
    })
}

/// Live values of a point wait only as the newest, the last value of a drag is always kept and
/// goes last for its point, and a change that does not answer holds up the one in flight, never a
/// backlog in front of the last value.
#[tokio::test]
async fn a_hung_daemon_never_holds_a_backlog_in_front_of_the_last_value() {
    let mut link = ComputerLink::none();
    let log = Arc::new(std::sync::Mutex::new(Vec::new()));
    let gate = Arc::new(tokio::sync::Notify::new());
    // The first change hangs until the gate opens: the daemon that does not answer.
    let held = Arc::clone(&gate);
    let hung: Work = Box::pin(async move {
        held.notified().await;
        None
    });
    link.queue(hung, Tag::Other);
    for n in 1..=200 {
        link.queue(recording(&log, n), Tag::Live(Named::Index(1)));
    }
    link.queue(recording(&log, 1000), Tag::Live(Named::Index(2)));
    link.queue(recording(&log, 2000), Tag::Last(Named::Index(1)));
    link.queue(recording(&log, 3000), Tag::Other);
    assert!(
        link.queue
            .as_ref()
            .expect("a queue")
            .items
            .lock()
            .unwrap()
            .len()
            <= 4,
        "superseded values were kept"
    );
    gate.notify_one();
    tokio::time::timeout(WITHIN, async {
        while log.lock().unwrap().len() < 3 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the queue drained");
    assert_eq!(*log.lock().unwrap(), [1000, 2000, 3000]);
}

/// The DAC's load of a moved preset waits for the computer EQ to be off: a removal that is refused
/// answers in words and sends no load, one that succeeds says `Moved`.
#[tokio::test]
async fn a_failed_removal_loads_nothing_on_the_dac() {
    let chain = Arc::new(FakeChain::new());
    let control = EqControl::new(chain.clone(), Box::new(FakeParams::new()), Keep::Memory);
    let mut link = ComputerLink::over(control);
    link.direct(HostEqOp::Mode { on: true })
        .expect("a controller");
    tokio::time::timeout(WITHIN, async {
        loop {
            if next_view(&mut link.views).await.on {
                return;
            }
        }
    })
    .await
    .expect("it turned on");

    chain.refuse(Some(svolder_host::eq::EqFailure {
        what: "the unit would not stop".into(),
        next: "Try again".into(),
    }));
    link.move_off("Phones".into(), None).expect("a controller");
    let answer = tokio::time::timeout(WITHIN, answered(&mut link.answers))
        .await
        .expect("an answer");
    assert!(
        matches!(&*answer, Answer::Said(why) if why.contains("would not stop")),
        "{answer:?}"
    );

    chain.refuse(None);
    link.move_off("Phones".into(), None).expect("a controller");
    let answer = tokio::time::timeout(WITHIN, answered(&mut link.answers))
        .await
        .expect("an answer");
    assert_eq!(*answer, Answer::Moved("Phones".into()));
}

/// Joined, the computer's volume and mute are requests to the daemon, in order, and a refusal comes
/// back in its words.
#[tokio::test]
async fn a_joined_tui_sends_the_volume_and_the_mute_to_the_daemon() {
    let (daemon, _path, mut watching) = on_a_daemon().await;
    watching.app.tui.host_audio = true;
    watching.app.host.linked = true;
    watching.app.host.daemon = Some(crate::tui::host::FromDaemon { route: None });
    watching.app.host.computer.volume = Some(svolder_ipc::host::HostVolume {
        pct: 40,
        muted: false,
        sink: "Schiit".into(),
    });
    volume(&mut watching, 41);
    volume(&mut watching, 42);
    mute(&mut watching);
    tokio::time::timeout(WITHIN, async {
        while daemon.host_volume_ops().len() < 3 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the daemon got every request");
    assert_eq!(
        daemon.host_volume_ops(),
        ["volume 41", "volume 42", "muted true"]
    );
    assert_eq!(
        watching.app.host.asked,
        Some(42),
        "the ask stands until read back"
    );
    watching.close().await;
}

/// A saved state that says on while no chain runs (a reboot) is not shown as on by a direct TUI:
/// the link looks once, writing nothing, and the view says off.
#[tokio::test]
async fn a_direct_tui_never_shows_the_eq_on_with_no_chain() {
    use svolder_host::eq::{HostEqState, Manager, Route};
    use svolder_profiles::tone::apo::{FilterKind, SourceFilter};
    let dir = std::env::temp_dir().join(format!("svolder-link-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let path = dir.join("host-eq.toml");
    HostEqState {
        on: true,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: Route::Smart,
        manager: Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: 100.0,
            g: 2.0,
            q: 1.0,
        }],
        slots: vec![1],
    }
    .save(&path)
    .unwrap();
    let chain = Arc::new(FakeChain::new());
    let control = EqControl::new(chain.clone(), Box::new(FakeParams::new()), Keep::File(path));
    let mut link = ComputerLink::over(control);
    assert_eq!(
        link.view().map(|view| view.on),
        Some(true),
        "as the file says"
    );
    let seen = tokio::time::timeout(WITHIN, async {
        loop {
            let view = next_view(&mut link.views).await;
            if !view.on {
                return view;
            }
        }
    })
    .await
    .expect("the view stops saying on");
    assert!(!seen.on);
    assert!(chain.calls().is_empty(), "looking writes nothing");
}
