// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a one-shot command does under the child manager.
//! What a preset applies is in `svolder-profiles`; the manager's own tests are in `svolder-host`.

use svolder_host::eq::{ApplyRequest, EqPaths, Host};
use svolder_profiles::tone::apo::Correction;

use super::*;

/// A one-shot apply or remove under the child manager is refused before it writes anything: the
/// chain would end with the command.
#[tokio::test]
async fn a_one_shot_command_under_the_child_manager_is_refused_and_writes_nothing() {
    let dir = std::env::temp_dir().join(format!("svhe-child-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = crate::test_support::TempDir(dir);
    let state = dir.join("host-eq.toml");
    std::fs::write(&state, "kept\n").unwrap();
    let paths = EqPaths {
        conf: dir.join("host-eq.conf"),
        state: state.clone(),
        unit_dir: dir.join("units"),
        autostart_dir: dir.join("autostart"),
        pid_file: Some(dir.join("host-eq.pid")),
    };
    // The manager is named outright, so an inherited value cannot change what this tests.
    let host = || {
        // SAFETY: no other test of this crate reads or writes this variable.
        unsafe { std::env::set_var("SVOLDER_HOST_EQ_MANAGER", "child") };
        let host = Host::live(paths.clone(), false);
        // SAFETY: as above.
        unsafe { std::env::remove_var("SVOLDER_HOST_EQ_MANAGER") };
        host
    };
    let request = ApplyRequest {
        name: "x".into(),
        origin: None,
        correction: Correction {
            preamp_db: 0.0,
            filters: Vec::new(),
        },
        sink: None,
        slots: Vec::new(),
        rate_hz: None,
    };
    let failure = apply_once(host(), request).await.unwrap_err();
    assert_eq!(failure.code, 1);
    assert_eq!(
        failure.to_string(),
        "in a container the computer EQ runs with the daemon. start svolderd, then try again"
    );
    assert!(remove_once(host()).await.is_err());
    assert_eq!(std::fs::read_to_string(&state).unwrap(), "kept\n");
    let mut left: Vec<_> = std::fs::read_dir(&*dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    left.sort();
    assert_eq!(left, ["host-eq.toml"]);
}

/// The arguments of a command that reaches the daemon on `socket` and nothing else.
fn globals(socket: &str) -> Globals<'_> {
    Globals {
        transport: "daemon",
        adapter: None,
        socket: Some(socket),
        device: None,
        verbose: false,
    }
}

/// Only a daemon that shows the computer EQ is the daemon the computer EQ commands go through:
/// one that does not (an older one, or a host that is off) and none at all leave the command to
/// do the work itself.
#[tokio::test]
async fn only_a_daemon_that_shows_the_computer_eq_serves_the_commands() {
    use crate::test_support::stub_daemon::{SocketFile, fresh_socket, stub_daemon};
    use svolder_ipc::host::HostView;

    let daemon = stub_daemon().await;
    let socket = daemon.socket.display().to_string();
    let config = Config::default();
    assert!(
        served(&globals(&socket), &config).await.is_none(),
        "no host view at all"
    );

    daemon.hosts(HostView {
        available: true,
        ..HostView::default()
    });
    assert!(
        served(&globals(&socket), &config).await.is_none(),
        "no EQ in the view"
    );

    daemon.hosts(HostView {
        available: true,
        eq: Some(EqView {
            on: true,
            ..EqView::default()
        }),
        ..HostView::default()
    });
    let (path, eq) = served(&globals(&socket), &config)
        .await
        .expect("a serving daemon");
    assert_eq!(path, daemon.socket);
    assert!(eq.on);
    let direct = Globals {
        transport: "fake",
        ..globals(&socket)
    };
    assert!(
        served(&direct, &config).await.is_none(),
        "a direct transport asks no daemon"
    );

    let none = SocketFile(fresh_socket());
    let nowhere = none.0.display().to_string();
    assert!(
        served(&globals(&nowhere), &config).await.is_none(),
        "no daemon"
    );
}

/// An operation goes to the daemon as the request it is, and a refusal comes back in the
/// daemon's own words with the code it gave.
#[tokio::test]
async fn an_operation_reaches_the_daemon_and_its_refusal_comes_back_in_its_words() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    eq_op(&daemon.socket, HostEqOp::Mode { on: false })
        .await
        .expect("taken");
    assert_eq!(daemon.host_eq_ops(), [HostEqOp::Mode { on: false }]);

    daemon.refuses_eq("computer controls unavailable: no pw-cli on this computer");
    let refused = eq_op(&daemon.socket, HostEqOp::Mode { on: true })
        .await
        .unwrap_err();
    assert_eq!(refused.code, 1);
    assert!(
        refused
            .what
            .contains("computer controls unavailable: no pw-cli on this computer"),
        "{refused}"
    );
}
