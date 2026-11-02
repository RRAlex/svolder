// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The hand-over around a body, through a fake `systemctl`, a daemon on a socket of the case's
//! own and a host that counts the daemons it is asked to start.

use std::cell::Cell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::FutureExt;
use svolder_ipc::checks::{self, SystemdFacts};
use tokio::net::UnixListener;

use super::*;
use crate::commands::daemon::test_support::{Tools, a_daemon_on, no_units, units_in};
use crate::commands::daemon::{SERVICE_UNIT, SOCKET_UNIT};
use crate::test_support::temp_dir;

/// A host that finds no daemon to start and counts the times it was asked for one.
struct CountingHost {
    asked: Cell<usize>,
    dir: PathBuf,
}

impl CountingHost {
    fn new(dir: &Path) -> CountingHost {
        CountingHost {
            asked: Cell::new(0),
            dir: dir.to_path_buf(),
        }
    }
}

impl Host for CountingHost {
    fn units(&self) -> SystemdFacts {
        checks::live::absent()
    }

    fn start_socket(&self) -> Result<(), String> {
        Err("no systemd".to_string())
    }

    fn daemon_binary(&self) -> Option<PathBuf> {
        self.asked.set(self.asked.get() + 1);
        None
    }

    fn log(&self) -> PathBuf {
        self.dir.join("svolderd.log")
    }

    fn daemon_env(&self) -> Vec<(OsString, OsString)> {
        Vec::new()
    }
}

fn globals(socket: &str, transport: &str) -> (String, String) {
    (socket.to_string(), transport.to_string())
}

fn start_calls(tools: &Tools) -> Vec<String> {
    tools
        .calls()
        .into_iter()
        .filter(|call| call.contains(" start "))
        .collect()
}

/// With the socket unit listening and the service idle, a connect to the socket would start the
/// service; the holder is found from the unit facts and nothing connects.
#[tokio::test]
async fn systemd_with_an_idle_service_is_found_without_connecting() {
    let dir = temp_dir("handover-idle");
    let socket = dir.0.join("svolder.sock");
    let listener = UnixListener::bind(&socket).expect("the socket is bound");
    let tools = Tools::new(&units_in(
        &socket.display().to_string(),
        "active",
        "inactive",
    ));
    let (path, transport) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &transport,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };

    let found = holder(&g, &Config::default(), &tools.ctl()).await;

    assert_eq!(
        found,
        Holder::Systemd {
            socket: true,
            service: false
        }
    );
    assert!(
        listener.accept().now_or_never().is_none(),
        "something connected to the socket systemd owns"
    );
    assert!(tools.calls().iter().all(|call| !call.contains(" start ")));
}

/// Units that are installed but both idle hold nothing, and nothing can start behind a connect,
/// so a daemon answering on the path is found, and none is none.
#[tokio::test]
async fn idle_units_leave_the_socket_to_be_asked() {
    let dir = temp_dir("handover-stopped");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&units_in(
        &socket.display().to_string(),
        "inactive",
        "inactive",
    ));
    let (path, transport) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &transport,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };
    assert_eq!(
        holder(&g, &Config::default(), &tools.ctl()).await,
        Holder::Nobody
    );

    let served = a_daemon_on(&socket).await;
    assert_eq!(
        holder(&g, &Config::default(), &tools.ctl()).await,
        Holder::Spawned {
            socket: socket.clone()
        }
    );
    stop(&g, &tools.ctl(), &Holder::Spawned { socket })
        .await
        .expect("stopped");
    let _ = served.await;
}

/// A daemon on a path the units do not listen on is a spawned one, whatever the units say.
#[tokio::test]
async fn a_daemon_on_a_path_the_units_do_not_own_is_spawned() {
    let dir = temp_dir("handover-elsewhere");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&units_in("/run/user/1000/svolder.sock", "active", "active"));
    let served = a_daemon_on(&socket).await;
    let (path, transport) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &transport,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };
    assert_eq!(
        holder(&g, &Config::default(), &tools.ctl()).await,
        Holder::Spawned {
            socket: socket.clone()
        }
    );
    stop(&g, &tools.ctl(), &Holder::Spawned { socket })
        .await
        .expect("stopped");
    assert!(tools.calls().iter().all(|call| !call.contains(" stop ")));
    let _ = served.await;
}

/// Without systemd's units a daemon answering on the socket is the spawned one, and the fake
/// transport has no holder at all.
#[tokio::test]
async fn a_daemon_without_units_is_spawned_and_the_fake_has_none() {
    let dir = temp_dir("handover-spawned");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&no_units());
    let (path, auto) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &auto,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };
    assert_eq!(
        holder(&g, &Config::default(), &tools.ctl()).await,
        Holder::Nobody
    );

    let served = a_daemon_on(&socket).await;
    assert_eq!(
        holder(&g, &Config::default(), &tools.ctl()).await,
        Holder::Spawned {
            socket: socket.clone()
        }
    );
    let fake = Globals {
        transport: "fake",
        ..g
    };
    assert_eq!(
        holder(&fake, &Config::default(), &tools.ctl()).await,
        Holder::Nobody
    );
    stop(
        &g,
        &tools.ctl(),
        &Holder::Spawned {
            socket: socket.clone(),
        },
    )
    .await
    .expect("the daemon is stopped");
    assert!(!svolder_ipc::is_running(&socket).await);
    let _ = served.await;
}

/// What was running comes back, the socket before the service; a spawned daemon is spawned.
#[tokio::test]
async fn restore_starts_what_was_active() {
    let dir = temp_dir("handover-restore");
    let tools = Tools::new(&no_units());
    let host = CountingHost::new(&dir.0);

    restore(
        &tools.ctl(),
        &host,
        &Holder::Systemd {
            socket: true,
            service: false,
        },
    )
    .await
    .expect("started");
    assert_eq!(
        start_calls(&tools),
        vec![format!("systemctl --user start {SOCKET_UNIT}")]
    );

    restore(
        &tools.ctl(),
        &host,
        &Holder::Systemd {
            socket: true,
            service: true,
        },
    )
    .await
    .expect("started");
    assert_eq!(
        start_calls(&tools)[1..],
        [
            format!("systemctl --user start {SOCKET_UNIT}"),
            format!("systemctl --user start {SERVICE_UNIT}")
        ]
    );

    let before = tools.calls().len();
    restore(&tools.ctl(), &host, &Holder::Nobody)
        .await
        .expect("nothing to do");
    assert_eq!(tools.calls().len(), before);
    assert_eq!(host.asked.get(), 0);

    let socket = dir.0.join("svolder.sock");
    let _ = restore(&tools.ctl(), &host, &Holder::Spawned { socket }).await;
    assert_eq!(host.asked.get(), 1, "the daemon was spawned once");
}

/// A body that fails still leaves the units started again.
#[tokio::test]
async fn away_restores_after_a_body_that_fails() {
    let dir = temp_dir("handover-fails");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&units_in(&socket.display().to_string(), "active", "active"));
    let host = CountingHost::new(&dir.0);
    let (path, transport) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &transport,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };
    let ctl = tools.ctl();
    let held = holder(&g, &Config::default(), &ctl).await;

    let (body, back) = away(&g, &ctl, &host, &held, async {
        Err::<(), &str>("nothing in range")
    })
    .await;

    assert_eq!(body.expect("the body ran"), Err("nothing in range"));
    back.expect("the units came back");
    let calls = tools.calls();
    let stopped = calls
        .iter()
        .position(|c| c == &format!("systemctl --user stop {SERVICE_UNIT} {SOCKET_UNIT}"))
        .expect("stopped");
    let started = calls
        .iter()
        .position(|c| c == &format!("systemctl --user start {SOCKET_UNIT}"))
        .expect("the socket started");
    assert!(stopped < started, "{calls:?}");
    assert!(calls.contains(&format!("systemctl --user start {SERVICE_UNIT}")));
}

/// A body that returns early, as a Ctrl-C does, still leaves a spawned daemon spawned again.
#[tokio::test]
async fn away_restores_after_a_body_that_was_interrupted() {
    let dir = temp_dir("handover-interrupted");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&no_units());
    let host = CountingHost::new(&dir.0);
    let served = a_daemon_on(&socket).await;
    let (path, transport) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &transport,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };
    let ctl = tools.ctl();
    let held = holder(&g, &Config::default(), &ctl).await;
    assert_eq!(
        held,
        Holder::Spawned {
            socket: socket.clone()
        }
    );

    let presses = crate::commands::interrupts::Presses::new();
    presses.press();
    let waiting = async {
        presses.next().await;
        "interrupted"
    };
    let (body, back) = away(&g, &ctl, &host, &held, waiting).await;

    assert_eq!(body.expect("the body ran"), "interrupted");
    assert_eq!(host.asked.get(), 1, "the daemon was started again");
    assert!(back.is_err(), "this host has no daemon to start");
    let _ = served.await;
}

/// A stop that fails part way still has the restore run, and the body is not run.
#[tokio::test]
async fn away_restores_after_a_stop_that_failed() {
    let dir = temp_dir("handover-stop-fails");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&units_in(&socket.display().to_string(), "active", "active"));
    tools.fail_stop();
    let host = CountingHost::new(&dir.0);
    let (path, transport) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &transport,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };
    let held = Holder::Systemd {
        socket: true,
        service: true,
    };
    let ran = Cell::new(false);

    let (body, back) = away(&g, &tools.ctl(), &host, &held, async { ran.set(true) }).await;

    assert!(body.is_err(), "the body does not run behind a failed stop");
    assert!(!ran.get());
    back.expect("what was there is put back");
    assert_eq!(start_calls(&tools).len(), 2);
}

/// A restore that fails says the command that brings the daemon back.
#[tokio::test]
async fn a_failed_restore_names_the_command() {
    let dir = temp_dir("handover-names");
    let host = CountingHost::new(&dir.0);
    let broken = Ctl::with_systemctl("/bin/false");

    let failed = restore(
        &broken,
        &host,
        &Holder::Systemd {
            socket: true,
            service: true,
        },
    )
    .await
    .expect_err("systemctl refuses");
    let said = failed.to_string();
    assert!(
        said.contains(&format!("systemctl --user start {SOCKET_UNIT}"))
            && said.contains(&format!("systemctl --user start {SERVICE_UNIT}")),
        "{said}"
    );

    let socket = dir.0.join("svolder.sock");
    let failed = restore(&broken, &host, &Holder::Spawned { socket })
        .await
        .expect_err("no svolderd to start");
    assert!(failed.to_string().contains("daemon restart"), "{failed}");
    let default = svolder_ipc::socket::path::resolve(None, None);
    let failed = restore(&broken, &host, &Holder::Spawned { socket: default })
        .await
        .expect_err("no svolderd to start");
    assert!(
        failed.to_string().contains("`svolder daemon restart`"),
        "{failed}"
    );
}

/// A body that panics still has the restore run, and the panic goes on.
#[tokio::test]
async fn away_restores_after_a_body_that_panics() {
    let dir = temp_dir("handover-panics");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&units_in(&socket.display().to_string(), "active", "active"));
    let host = CountingHost::new(&dir.0);
    let (path, transport) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &transport,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };
    let held = Holder::Systemd {
        socket: true,
        service: true,
    };
    let ctl = tools.ctl();

    let outcome = std::panic::AssertUnwindSafe(away(&g, &ctl, &host, &held, async {
        panic!("the radio stack fell over");
    }))
    .catch_unwind()
    .await;

    assert!(outcome.is_err(), "the panic goes on");
    assert_eq!(start_calls(&tools).len(), 2, "{:?}", tools.calls());
}

/// A restore that cannot be named by the default command says which socket.
#[tokio::test]
async fn a_failed_spawned_restore_names_a_socket_that_is_not_the_default() {
    let dir = temp_dir("handover-socket");
    let host = CountingHost::new(&dir.0);
    let socket = dir.0.join("elsewhere.sock");
    let failed = restore(
        &Ctl::with_systemctl("/bin/false"),
        &host,
        &Holder::Spawned {
            socket: socket.clone(),
        },
    )
    .await
    .expect_err("no svolderd");
    assert!(
        failed.to_string().contains(&format!(
            "svolder --socket {} daemon restart",
            socket.display()
        )),
        "{failed}"
    );
}

/// The wait for units to settle ends at its bound and sooner when they already have.
#[tokio::test]
async fn units_that_settle_end_the_wait() {
    let tools = Tools::new(&units_in("/run/user/1000/svolder.sock", "active", "active"));
    let started = Instant::now();
    assert!(
        settled(&tools.ctl(), Duration::from_secs(30), |u| u
            .socket_unit
            .is_some_and(|unit| unit.running))
        .await
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        !settled(&tools.ctl(), Duration::from_millis(150), |u| u
            .service_unit
            .is_some_and(|unit| !unit.running))
        .await
    );
}

/// A stop that leaves the units inactive shows so to the next look.
#[tokio::test]
async fn a_systemd_stop_leaves_the_units_inactive() {
    let dir = temp_dir("handover-stop-shows");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&units_in(&socket.display().to_string(), "active", "active"));
    let (path, transport) = globals(&socket.display().to_string(), "auto");
    let g = Globals {
        transport: &transport,
        adapter: None,
        socket: Some(&path),
        device: None,
        verbose: false,
    };
    let held = holder(&g, &Config::default(), &tools.ctl()).await;
    stop(&g, &tools.ctl(), &held).await.expect("stopped");
    assert_eq!(
        holder(&g, &Config::default(), &tools.ctl()).await,
        Holder::Nobody
    );
}

/// A stop that panics part way is still followed by the restore, and the panic is resumed.
#[tokio::test]
async fn a_panic_in_the_stop_still_brings_the_units_back() {
    let dir = temp_dir("handover-stop-panic");
    let socket = dir.0.join("svolder.sock");
    let tools = Tools::new(&units_in(&socket.display().to_string(), "active", "active"));
    let host = CountingHost::new(&dir.0);
    let holder = Holder::Systemd {
        socket: true,
        service: true,
    };
    let stopping = async { panic!("the stop panicked") };
    let ended = AssertUnwindSafe(away_after(stopping, &tools.ctl(), &host, &holder, async {}))
        .catch_unwind()
        .await;
    assert!(ended.is_err(), "the panic is resumed");
    let starts = start_calls(&tools);
    assert_eq!(starts.len(), 2, "{starts:?}");
}

/// The programs of a `Ctl` in its own group run in a process group that is not this one's.
#[test]
fn programs_in_an_own_group_do_not_share_ours() {
    let dir = temp_dir("handover-group");
    let seen = dir.0.join("group.txt");
    let script = dir.0.join("systemctl");
    crate::test_support::install_script(
        &script,
        &format!(
            "#!/bin/sh\ncut -d' ' -f5 /proc/self/stat > '{}'\nexit 0\n",
            seen.display()
        ),
    );
    let ours = std::fs::read_to_string("/proc/self/stat")
        .expect("our stat reads")
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().nth(2).map(str::to_string))
        .expect("our group");

    Ctl::with_systemctl(&script)
        .in_own_group()
        .systemctl(&["start", "x"])
        .expect("the script runs");
    let theirs = std::fs::read_to_string(&seen).expect("the script wrote its group");
    assert_ne!(theirs.trim(), ours);

    Ctl::with_systemctl(&script)
        .systemctl(&["start", "x"])
        .expect("the script runs");
    let shared = std::fs::read_to_string(&seen).expect("the script wrote its group");
    assert_eq!(shared.trim(), ours, "without the flag the group is ours");
}
