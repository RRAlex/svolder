// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon restart` and `daemon stop`, through a fake `systemctl` where systemd owns the daemon
//! and through a daemon on a socket where it does not.

use std::ffi::OsString;
use std::sync::Arc;

use svolder_ipc::checks;

use super::*;
use crate::commands::daemon::POLL;
use crate::commands::daemon::test_support::{
    FakeDaemon, Tools, a_daemon_on, installed_units, no_units, shown_units, stopped_units, units_in,
};
use crate::test_support::temp_dir;

/// A host with no daemon to start, for a case that must not start one.
struct NoDaemon(PathBuf);

impl Host for NoDaemon {
    fn units(&self) -> SystemdFacts {
        checks::live::absent()
    }

    fn start_socket(&self) -> Result<(), String> {
        Err("no systemd".to_string())
    }

    fn daemon_binary(&self) -> Option<PathBuf> {
        None
    }

    fn log(&self) -> PathBuf {
        self.0.join("svolderd.log")
    }

    fn daemon_env(&self) -> Vec<(OsString, OsString)> {
        Vec::new()
    }
}

/// `stop` and `restart` go through systemd where it has the units and its socket unit
/// listens on the socket in hand.
#[tokio::test]
async fn stop_and_restart_ask_systemd_where_it_manages_the_daemon() {
    let dir = temp_dir("daemon-stop");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&installed_units(&socket.display().to_string()));

    stop_on(&tools.ctl(), &socket).await.expect("stopped");
    restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, false)
        .await
        .expect("restarted");

    let calls = tools.calls();
    assert!(
        calls.contains(&format!(
            "systemctl --user stop {SERVICE_UNIT} {SOCKET_UNIT}"
        )),
        "{calls:?}"
    );
    assert!(
        calls.contains(&format!("systemctl --user restart {SERVICE_UNIT}")),
        "{calls:?}"
    );
}

/// A socket systemd's unit does not listen on holds another daemon, and `stop` asks that
/// one rather than asking systemd to stop its own — which is what the `also running` line
/// and the doctor row that print `svolder --socket <path> daemon stop` promise.
#[tokio::test]
async fn stop_asks_the_daemon_on_a_socket_the_unit_does_not_listen_on() {
    let tools = Tools::new(&installed_units("/run/user/1000/svolder.sock"));
    let dir = temp_dir("daemon-stop-elsewhere");
    let socket = dir.0.join("svolder.sock");
    let served = a_daemon_on(&socket).await;

    stop_on(&tools.ctl(), &socket)
        .await
        .expect("the daemon on that socket is stopped");

    assert!(
        !tools.calls().iter().any(|call| call.contains("stop")),
        "systemd was not asked to stop its own: {:?}",
        tools.calls()
    );
    assert!(
        !svolder_ipc::is_running(&socket).await,
        "the daemon on {} let its socket go",
        socket.display()
    );
    let _ = served.await;
}

/// Without systemd, `stop` asks the daemon itself — and says so when there is none.
#[tokio::test]
async fn stop_without_systemd_asks_the_daemon_and_takes_no_for_an_answer() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-stop-bare");
    let socket = dir.0.join("svolder.sock");

    stop_on(&tools.ctl(), &socket)
        .await
        .expect("nothing to stop is not a failure");
    assert!(
        !tools.calls().iter().any(|call| call.contains("stop")),
        "systemd was not asked: {:?}",
        tools.calls()
    );

    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let served = tokio::spawn({
        let socket = socket.clone();
        async move {
            let _ = svolder_ipc::serve(&socket, Arc::new(FakeDaemon), async {
                let _ = stopped.await;
            })
            .await;
        }
    });
    for _ in 0..100 {
        if svolder_ipc::is_running(&socket).await {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    // The fake daemon answers the request but keeps serving, so this is the wait that
    // gives up on a daemon that will not let go of its socket.
    let failure = stop_on(&tools.ctl(), &socket)
        .await
        .expect_err("a daemon that keeps the socket is reported");
    assert!(failure.what.contains("still answers"), "{}", failure.what);
    let _ = stop.send(());
    let _ = served.await;
}

/// Without systemd, a restart with nothing running starts a daemon — and says where to
/// look when there is none to start.
#[tokio::test]
async fn restart_without_systemd_starts_a_daemon_and_names_the_log_when_it_cannot() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-restart-bare");
    let socket = dir.0.join("svolder.sock");

    let failure = restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, false)
        .await
        .expect_err("there is no daemon to start");
    assert_eq!(failure.code, Kind::AutospawnFailed.code());
    assert_eq!(
        failure.next,
        crate::recovery::next_step(Kind::AutospawnFailed)
    );
}

/// What it prints for two installed units, the socket on `listen` and the service stopped.
fn inactive_service_units(listen: &str) -> String {
    format!(
        "Id={SOCKET_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState=active\n\
             Listen={listen} (Stream)\n\n\
             Id={SERVICE_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState=inactive\n"
    )
}

/// `restart --if-running` with no daemon and no units does nothing: no `systemctl` restart,
/// nothing started.
#[tokio::test]
async fn restart_if_running_with_nothing_running_does_nothing() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-restart-if-none");
    let socket = dir.0.join("svolder.sock");

    restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, true)
        .await
        .expect("nothing to restart is not a failure");

    assert!(
        !tools.calls().iter().any(|call| call.contains("restart")),
        "{:?}",
        tools.calls()
    );
    assert!(!svolder_ipc::is_running(&socket).await);
}

/// With the units owning the socket and the service stopped, `--if-running` asks systemd
/// for nothing.
#[tokio::test]
async fn restart_if_running_leaves_a_stopped_service_alone() {
    let dir = temp_dir("daemon-restart-if-inactive");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&inactive_service_units(&socket.display().to_string()));

    restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, true)
        .await
        .expect("a stopped service is not a failure");

    assert_eq!(tools.calls(), vec![shown_units()]);
}

/// With both units installed and stopped, a daemon that answers on the socket is stopped and
/// started again, as without units.
#[tokio::test]
async fn restart_if_running_with_stopped_units_restarts_a_daemon_that_answers() {
    let dir = temp_dir("daemon-restart-if-stopped-units");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&stopped_units(&socket.display().to_string()));
    let served = a_daemon_on(&socket).await;

    let failure = restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, true)
        .await
        .expect_err("the host has no daemon to start again");

    assert_eq!(failure.code, Kind::AutospawnFailed.code());
    assert!(
        !svolder_ipc::is_running(&socket).await,
        "the old daemon was stopped"
    );
    assert!(!tools.calls().iter().any(|call| call.contains("restart")));
    let _ = served.await;
}

/// With both units installed and stopped and no daemon answering, nothing is restarted.
#[tokio::test]
async fn restart_if_running_with_stopped_units_and_no_daemon_does_nothing() {
    let dir = temp_dir("daemon-restart-if-stopped-none");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&stopped_units(&socket.display().to_string()));

    restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, true)
        .await
        .expect("nothing to restart is not a failure");

    assert_eq!(tools.calls(), vec![shown_units()]);
    assert!(!svolder_ipc::is_running(&socket).await);
}

/// With the service active, `--if-running` restarts it through systemd.
#[tokio::test]
async fn restart_if_running_restarts_an_active_service() {
    let dir = temp_dir("daemon-restart-if-active");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&installed_units(&socket.display().to_string()));

    restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, true)
        .await
        .expect("restarted");

    let restarts: Vec<_> = tools
        .calls()
        .into_iter()
        .filter(|call| call.contains("restart"))
        .collect();
    assert_eq!(
        restarts,
        vec![format!("systemctl --user restart {SERVICE_UNIT}")]
    );
}

/// Without units, a daemon that answers is stopped and started again, as plain `restart`
/// does; here the host has none to start, which is reported after the old one let go.
#[tokio::test]
async fn restart_if_running_restarts_a_daemon_that_answers() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-restart-if-answers");
    let socket = dir.0.join("svolder.sock");
    let served = a_daemon_on(&socket).await;

    let failure = restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, true)
        .await
        .expect_err("the host has no daemon to start again");

    assert_eq!(failure.code, Kind::AutospawnFailed.code());
    assert!(
        !svolder_ipc::is_running(&socket).await,
        "the old daemon was stopped"
    );
    let _ = served.await;
}

/// Take the lock a daemon holds beside `socket` and keep it for `hold`, as an old daemon that
/// is still closing its devices does after its socket is gone.
fn hold_the_lock(socket: &Path, hold: Duration) -> std::thread::JoinHandle<()> {
    let mut name = socket.as_os_str().to_owned();
    name.push(".lock");
    let lock = std::fs::File::create(PathBuf::from(name)).expect("a lock file");
    // SAFETY: the descriptor belongs to `lock`, which outlives the call.
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    std::thread::spawn(move || {
        std::thread::sleep(hold);
        drop(lock);
    })
}

/// A restart that stopped a daemon without systemd does not start the new one before the old
/// one has let go of its lock.
#[tokio::test]
async fn restart_waits_for_the_old_daemon_to_release_its_lock() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-restart-lock");
    let socket = dir.0.join("svolder.sock");
    let served = a_daemon_on(&socket).await;
    let hold = Duration::from_millis(600);
    let started = Instant::now();
    let holder = hold_the_lock(&socket, hold);

    let _ = restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, false).await;

    assert!(
        started.elapsed() >= hold,
        "the restart went on after {:?}, with the lock held for {hold:?}",
        started.elapsed()
    );
    holder.join().unwrap();
    let _ = served.await;
}

/// With the lock held past the bound, the wait ends and says so rather than hanging.
#[tokio::test]
async fn waiting_for_the_old_daemon_gives_up_at_its_bound() {
    let dir = temp_dir("daemon-lock-bound");
    let socket = dir.0.join("svolder.sock");
    let holder = hold_the_lock(&socket, Duration::from_millis(500));
    assert!(!wait_for_the_old_daemon_to_exit(&socket, Duration::from_millis(100)).await);
    holder.join().unwrap();
    assert!(wait_for_the_old_daemon_to_exit(&socket, Duration::from_millis(100)).await);
    // No lock file at all: no daemon was there.
    let none = dir.0.join("none.sock");
    assert!(wait_for_the_old_daemon_to_exit(&none, Duration::from_millis(100)).await);
}

/// A stop of a spawned daemon whose lock stays held fails, with the failure naming the wait.
#[tokio::test]
async fn stop_spawned_fails_with_one_error_when_the_lock_stays_held() {
    let dir = temp_dir("daemon-stop-spawned-lock");
    let socket = dir.0.join("svolder.sock");
    let served = a_daemon_on(&socket).await;
    let holder = hold_the_lock(&socket, Duration::from_millis(600));
    let bound = Duration::from_millis(100);
    let failure = stop_spawned_within(&socket, bound).await.unwrap_err();
    assert!(
        failure.to_string().contains("had not exited"),
        "the failure was {failure}"
    );
    holder.join().unwrap();
    let _ = served.await;
}

/// A service systemd is still starting, or reloading, is running to `--if-running`: it is
/// restarted through systemd rather than taken for stopped.
#[tokio::test]
async fn restart_if_running_restarts_a_service_that_is_starting_or_reloading() {
    for state in ["activating", "reloading"] {
        let dir = temp_dir(&format!("daemon-restart-if-{state}"));
        let socket = dir.0.join("svolder.sock");
        let tools = Tools::new(&units_in(&socket.display().to_string(), "active", state));
        restart_on(&tools.ctl(), &NoDaemon(dir.0.clone()), &socket, true)
            .await
            .expect("restarted");
        assert!(
            tools
                .calls()
                .contains(&format!("systemctl --user restart {SERVICE_UNIT}")),
            "{state}: {:?}",
            tools.calls()
        );
    }
}
