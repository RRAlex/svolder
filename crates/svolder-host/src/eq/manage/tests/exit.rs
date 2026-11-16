// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chain's exit signal.

use super::*;

/// Whether `waiting` ends within a few seconds; a bound for a loaded machine, not a poll.
async fn ends(waiting: &mut tokio::task::JoinHandle<()>) -> bool {
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .is_ok()
}

/// Whether `waiting` is still waiting after a moment.
async fn still_waits(waiting: &mut tokio::task::JoinHandle<()>) -> bool {
    tokio::time::timeout(Duration::from_millis(150), waiting)
        .await
        .is_err()
}

/// The pid the fake `pipewire` logged last.
fn last_logged_pid(f: &Fixture) -> u32 {
    let line = f.log("pipewire.log").pop().expect("it started");
    line.split(' ').next().unwrap().parse().unwrap()
}

#[tokio::test]
async fn nothing_exits_while_no_computer_eq_is_on() {
    let f = Fixture::new();
    let host = std::sync::Arc::new(f.host(Manager::Child));
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    assert!(still_waits(&mut waiting).await);
    waiting.abort();
}

#[tokio::test]
async fn a_child_chain_that_is_killed_is_seen_and_an_apply_or_remove_is_not_an_exit() {
    let f = Fixture::new();
    let host = std::sync::Arc::new(f.host(Manager::Child));
    host.apply(request("First")).await.unwrap();
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    assert!(still_waits(&mut waiting).await);
    // A replacement stops the chain it waits on: not an exit; the next chain is waited on.
    host.apply(request("Second")).await.unwrap();
    assert!(still_waits(&mut waiting).await);
    signal(last_logged_pid(&f), libc::SIGKILL);
    assert!(ends(&mut waiting).await, "the killed chain was not seen");
    // A remove is not an exit either.
    let again = std::sync::Arc::new(f.host(Manager::Child));
    again.apply(request("Third")).await.unwrap();
    let mut waiting = tokio::spawn({
        let again = again.clone();
        async move { again.exited().await }
    });
    assert!(still_waits(&mut waiting).await);
    again.remove().await.unwrap();
    assert!(still_waits(&mut waiting).await);
    waiting.abort();
}

#[tokio::test]
async fn a_chain_that_died_before_the_wait_began_is_an_exit_at_once() {
    let f = Fixture::new();
    let host = f.host(Manager::Child);
    host.apply(request("First")).await.unwrap();
    signal(last_logged_pid(&f), libc::SIGKILL);
    let mut waiting = tokio::spawn(async move { host.exited().await });
    assert!(ends(&mut waiting).await, "the dead chain was not seen");
}

#[tokio::test]
async fn a_process_chain_that_is_killed_is_seen() {
    let f = Fixture::new();
    let host = std::sync::Arc::new(f.host(Manager::Process));
    host.apply(request("First")).await.unwrap();
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    assert!(still_waits(&mut waiting).await);
    host.apply(request("Second")).await.unwrap();
    assert!(still_waits(&mut waiting).await);
    signal(pid_of(&f).0, libc::SIGKILL);
    assert!(ends(&mut waiting).await, "the killed chain was not seen");
}

#[tokio::test]
async fn a_units_main_process_that_dies_is_seen() {
    let f = Fixture::new();
    let host = std::sync::Arc::new(f.host(Manager::Systemd));
    host.apply(request("First")).await.unwrap();
    // The unit's main process, as `systemctl show` names it.
    let mut main = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    std::fs::write(f.dir.join("mainpid"), main.id().to_string()).unwrap();
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    assert!(still_waits(&mut waiting).await);
    main.kill().unwrap();
    let _ = main.wait();
    assert!(
        ends(&mut waiting).await,
        "the dead unit process was not seen"
    );
}

/// A live `sleep` standing in for the unit's main process, named by the fake `systemctl`.
fn main_process(f: &Fixture) -> std::process::Child {
    let main = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    std::fs::write(f.dir.join("mainpid"), main.id().to_string()).unwrap();
    main
}

#[tokio::test]
async fn a_systemctl_that_cannot_answer_is_not_an_exit() {
    let f = Fixture::new();
    let mut host = f.host(Manager::Systemd);
    host.apply(request("First")).await.unwrap();
    let mut main = main_process(&f);
    let failing = f.dir.join("failing-systemctl");
    script::install_script(&failing, "#!/bin/sh\nexit 1\n");
    host.ctl = UserSystemd::with_program(&failing);
    let host = std::sync::Arc::new(host);
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    assert!(
        still_waits(&mut waiting).await,
        "a systemctl that did not answer was taken for a dead chain"
    );
    waiting.abort();
    let _ = main.kill();
    let _ = main.wait();
}

#[tokio::test]
async fn a_unit_that_is_activating_is_not_an_exit() {
    let f = Fixture::new();
    let host = std::sync::Arc::new(f.host(Manager::Systemd));
    host.apply(request("First")).await.unwrap();
    let mut main = main_process(&f);
    std::fs::write(f.dir.join("unit-activating"), "").unwrap();
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    assert!(
        still_waits(&mut waiting).await,
        "an activating unit was taken for a dead chain"
    );
    waiting.abort();
    let _ = main.kill();
    let _ = main.wait();
}

#[tokio::test]
async fn a_stopped_unit_is_an_exit() {
    let f = Fixture::new();
    let host = std::sync::Arc::new(f.host(Manager::Systemd));
    host.apply(request("First")).await.unwrap();
    std::fs::write(f.dir.join("unit-stopped"), "").unwrap();
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    assert!(ends(&mut waiting).await, "an inactive unit was not seen");
}

#[tokio::test]
async fn a_systemd_remove_is_not_an_exit() {
    let f = Fixture::new();
    let host = std::sync::Arc::new(f.host(Manager::Systemd));
    host.apply(request("First")).await.unwrap();
    let mut main = main_process(&f);
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    assert!(still_waits(&mut waiting).await);
    host.remove().await.unwrap();
    assert!(still_waits(&mut waiting).await);
    waiting.abort();
    let _ = main.kill();
    let _ = main.wait();
}

/// `exited` parked in a `select!` beside the apply and remove of the same task does not hold
/// the gate while systemd is asked.
#[tokio::test]
async fn a_pinned_exited_beside_an_apply_in_one_task_does_not_deadlock() {
    let f = Fixture::new();
    let mut host = f.host(Manager::Systemd);
    host.apply(request("First")).await.unwrap();
    let slow = f.dir.join("slow-systemctl");
    script::install_script(&slow, super::systemd::SLOW_SYSTEMCTL);
    host.ctl = UserSystemd::with_program(&slow);
    let exited = host.exited();
    tokio::pin!(exited);
    tokio::select! {
        _ = &mut exited => panic!("the chain was not gone"),
        _ = tokio::time::sleep(Duration::from_millis(100)) => {}
    }
    let removed = tokio::time::timeout(Duration::from_secs(20), host.remove()).await;
    assert!(removed.is_ok(), "the remove never got the gate");
}

/// A unit that an apply or remove under way has stopped is not an exit, even when systemd
/// answers "inactive" before that apply or remove has ended.
#[tokio::test]
async fn a_unit_stopped_by_a_remove_under_way_is_not_an_exit() {
    let f = Fixture::new();
    let mut host = f.host(Manager::Systemd);
    host.apply(request("First")).await.unwrap();
    // The first `show` takes 2 s to answer; `stop` marks the unit stopped, then takes 3 s.
    let wrapper = f.dir.join("wrapper-systemctl");
    script::install_script(
        &wrapper,
        r#"#!/bin/sh
D=$(dirname "$0")
case "$2" in
show)
  if [ ! -f "$D/asked" ]; then touch "$D/asked"; sleep 2; fi
  if [ -f "$D/stopped" ]; then echo ActiveState=inactive; else echo ActiveState=active; fi
  echo MainPID=0 ;;
stop) touch "$D/stopped"; sleep 3 ;;
esac
exit 0
"#,
    );
    host.ctl = UserSystemd::with_program(&wrapper);
    let host = std::sync::Arc::new(host);
    let mut waiting = tokio::spawn({
        let host = host.clone();
        async move { host.exited().await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let removing = tokio::spawn({
        let host = host.clone();
        async move { host.remove().await }
    });
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        !waiting.is_finished(),
        "the remove's own stop was taken for an exit"
    );
    removing.await.unwrap().unwrap();
    assert!(still_waits(&mut waiting).await);
    waiting.abort();
}
