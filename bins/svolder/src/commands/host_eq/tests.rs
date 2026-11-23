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
        flat: false,
        crossfeed: None,
        speakers: None,
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

/// A stub daemon that serves the computer EQ, with the EQ on or off.
async fn serving_daemon(on: bool) -> crate::test_support::stub_daemon::StubServer {
    use crate::test_support::stub_daemon::stub_daemon;
    let daemon = stub_daemon().await;
    daemon.hosts(svolder_ipc::host::HostView {
        available: true,
        eq: Some(EqView {
            on,
            ..EqView::default()
        }),
        ..svolder_ipc::host::HostView::default()
    });
    daemon
}

fn code_is(code: ExitCode, expected: u8) {
    assert_eq!(
        format!("{code:?}"),
        format!("{:?}", ExitCode::from(expected))
    );
}

/// `host eq on` and `host eq off` each send the one switch request, and nothing else.
#[tokio::test]
async fn on_and_off_send_the_switch() {
    let daemon = serving_daemon(false).await;
    let socket = daemon.socket.display().to_string();
    code_is(verbs::host_mode(&globals(&socket), true, true).await, 0);
    code_is(verbs::host_mode(&globals(&socket), false, false).await, 0);
    assert_eq!(
        daemon.host_eq_ops(),
        [HostEqOp::Mode { on: true }, HostEqOp::Mode { on: false }]
    );
}

/// A preset goes as a `preset` request; a preset a device plays too is asked about, and only
/// `--yes` repeats the request with `confirm`.
#[tokio::test]
async fn a_preset_asks_before_it_moves_a_device_s_preset_and_yes_confirms() {
    let daemon = serving_daemon(false).await;
    let socket = daemon.socket.display().to_string();
    let preset = |confirm| HostEqOp::Preset {
        name: "Warm".to_string(),
        device: None,
        confirm,
    };
    code_is(
        verbs::preset_on(&globals(&socket), "Warm", (false, false), false).await,
        0,
    );
    assert_eq!(daemon.host_eq_ops(), [preset(false)]);

    daemon.asks_before_a_preset();
    // No terminal and no --yes: the question is refused with the flag that answers it.
    code_is(
        verbs::preset_on(&globals(&socket), "Warm", (false, false), false).await,
        1,
    );
    assert_eq!(daemon.host_eq_ops().len(), 2, "no confirm was sent");
    code_is(
        verbs::preset_on(&globals(&socket), "Warm", (true, false), false).await,
        0,
    );
    assert_eq!(daemon.host_eq_ops()[2..], [preset(false), preset(true)]);
}

/// With no daemon the verbs change nothing and exit 3; a daemon whose host is off exits 6.
#[tokio::test]
async fn without_a_serving_daemon_the_verbs_refuse() {
    use crate::test_support::stub_daemon::{SocketFile, fresh_socket, stub_daemon};
    let none = SocketFile(fresh_socket());
    let nowhere = none.0.display().to_string();
    code_is(verbs::host_mode(&globals(&nowhere), true, false).await, 3);
    code_is(
        verbs::preset_on(&globals(&nowhere), "Warm", (true, false), false).await,
        3,
    );

    let daemon = stub_daemon().await;
    daemon.hosts(svolder_ipc::host::HostView {
        available: true,
        ..svolder_ipc::host::HostView::default()
    });
    let socket = daemon.socket.display().to_string();
    code_is(verbs::host_mode(&globals(&socket), true, false).await, 6);
    assert!(daemon.host_eq_ops().is_empty());
}

/// A daemon that asks again after a yes ends the verb with an error, not another question.
#[tokio::test]
async fn a_question_asked_again_after_a_yes_is_an_error() {
    let daemon = serving_daemon(false).await;
    let socket = daemon.socket.display().to_string();
    daemon.asks_before_a_preset();
    daemon.asks_after_a_yes_too();
    code_is(
        verbs::preset_on(&globals(&socket), "Warm", (true, false), false).await,
        3,
    );
    assert_eq!(
        daemon.host_eq_ops().len(),
        2,
        "one question, one yes, no more"
    );
}
