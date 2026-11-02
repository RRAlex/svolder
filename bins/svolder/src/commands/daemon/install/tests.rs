// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon install` and `daemon uninstall` against a scratch unit directory and fake programs, for
//! each kind of machine: a user's, root's, NixOS and a container.

use svolder_ipc::{Client, Request};

use super::*;
use crate::commands::daemon::test_support::{
    Tools, a_daemon_on, installed_units, no_units, shown_units, stopped_units, units_in,
};
use crate::test_support::temp_dir;

/// A target that writes into `dir` and installs a daemon from `bindir`.
fn target(dir: &Path, socket: &Path, runtime: Option<&Path>) -> Target {
    Target {
        dir: dir.join("systemd").join("user"),
        bindir: PathBuf::from("/opt/svolder/bin"),
        socket: socket.to_path_buf(),
        runtime_socket: runtime.map(|dir| dir.join("svolder.sock")),
        nixos: false,
        container: false,
        root: false,
        named_socket: false,
    }
}

/// A target for root: its units go to `dir`, and it has no user session to ask.
fn root_target(dir: &Path, socket: &Path) -> Target {
    Target {
        root: true,
        ..target(dir, socket, None)
    }
}

/// `install` writes both units, with the machine's own paths in them, and enables both.
#[tokio::test]
async fn install_writes_both_units_for_this_machine_and_enables_them() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-install");
    let runtime = temp_dir("daemon-runtime");
    let socket = runtime.0.join("svolder.sock");
    let target = target(&home.0, &socket, Some(&runtime.0));

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert!(
        installed.block.is_none(),
        "a machine that keeps its own units"
    );
    assert!(
        installed.said.iter().any(|line| line.starts_with("wrote "))
            && installed
                .said
                .iter()
                .any(|line| line.starts_with("enabled ")),
        "what it did comes back as lines: {:?}",
        installed.said
    );

    let socket_unit =
        std::fs::read_to_string(target.dir.join(SOCKET_UNIT)).expect("the socket unit is written");
    let service_unit = std::fs::read_to_string(target.dir.join(SERVICE_UNIT))
        .expect("the service unit is written");
    assert!(
        socket_unit.contains(&format!("ListenStream={RUNTIME_LISTEN_STREAM}")),
        "the default socket is the runtime directory's: {socket_unit}"
    );
    assert!(
        !socket_unit.contains("__"),
        "nothing was left unfilled: {socket_unit}"
    );
    assert!(
        service_unit.contains("ExecStart=/opt/svolder/bin/svolderd"),
        "the service starts the daemon beside this binary: {service_unit}"
    );
    assert_eq!(
        tools.calls(),
        vec![
            shown_units(),
            "systemctl --user daemon-reload".to_string(),
            format!("systemctl --user enable --now {SOCKET_UNIT}"),
            format!("systemctl --user enable --now {SERVICE_UNIT}"),
        ],
        "install asked systemd for exactly these, in this order"
    );
}

/// A socket somewhere other than the runtime directory is written as the path it is.
#[tokio::test]
async fn a_socket_outside_the_runtime_directory_is_written_as_itself() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-install-elsewhere");
    let socket = home.0.join("elsewhere").join("svolder.sock");
    let target = target(&home.0, &socket, Some(&home.0));

    assert_eq!(
        install_into(&tools.ctl(), &target, false)
            .await
            .done
            .expect("the units are installed"),
        Installed::Units
    );

    let socket_unit =
        std::fs::read_to_string(target.dir.join(SOCKET_UNIT)).expect("the socket unit is written");
    assert!(
        socket_unit.contains(&format!("ListenStream={}", socket.display())),
        "{socket_unit}"
    );
}

/// `--on-demand` enables the socket alone, so nothing runs until a command needs it.
#[tokio::test]
async fn install_on_demand_enables_the_socket_and_not_the_service() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-install-on-demand");
    let runtime = temp_dir("daemon-on-demand-runtime");
    let target = target(&home.0, &runtime.0.join("svolder.sock"), Some(&runtime.0));

    assert_eq!(
        install_into(&tools.ctl(), &target, true)
            .await
            .done
            .expect("the units are installed"),
        Installed::Units
    );

    let enabled: Vec<String> = tools
        .calls()
        .into_iter()
        .filter(|call| call.contains("enable"))
        .collect();
    assert_eq!(
        enabled,
        vec![format!("systemctl --user enable --now {SOCKET_UNIT}")]
    );
    assert!(
        target.dir.join(SERVICE_UNIT).is_file(),
        "the service unit is written either way: the socket starts it"
    );
}

/// An install that stopped partway still says what it had done: the unit files are written
/// whether or not systemd was ever told about them.
///
/// Nothing is said from inside the install — [`Install::say`] is the only thing that puts
/// these lines anywhere — so a front end drawing its own screen shows them itself.
#[tokio::test]
async fn an_install_that_stopped_partway_still_says_what_it_wrote() {
    let nowhere = temp_dir("daemon-no-systemctl");
    let home = temp_dir("daemon-install-stopped");
    let runtime = temp_dir("daemon-stopped-runtime");
    let target = target(&home.0, &runtime.0.join("svolder.sock"), Some(&runtime.0));

    let installed = install_into(&Ctl::in_dir(&nowhere.0), &target, false).await;
    let why = installed.done.expect_err("there is no systemctl to ask");
    assert!(why.what.contains("daemon-reload"), "{}", why.what);
    assert!(installed.block.is_none());
    for unit in [SOCKET_UNIT, SERVICE_UNIT] {
        assert!(target.dir.join(unit).is_file(), "{unit} was not written");
        assert!(
            installed
                .said
                .iter()
                .any(|line| line.starts_with("wrote ") && line.contains(unit)),
            "{unit} is not among {:?}",
            installed.said
        );
    }
    assert!(
        !installed
            .said
            .iter()
            .any(|line| line.starts_with("enabled")),
        "nothing was enabled: {:?}",
        installed.said
    );
}

/// On NixOS the unit files are generated, so `install` prints the block to generate them
/// from and writes nothing.
#[tokio::test]
async fn install_on_nixos_prints_the_home_manager_block_and_writes_nothing() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-install-nixos");
    let runtime = temp_dir("daemon-nixos-runtime");
    let mut target = target(&home.0, &runtime.0.join("svolder.sock"), Some(&runtime.0));
    target.nixos = true;

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert!(
        matches!(&installed.done, Ok(Installed::Skipped(why)) if why.contains("NixOS")),
        "nothing was installed, and the answer says so: {:?}",
        installed.done
    );

    let snippet = installed
        .block
        .clone()
        .expect("the block to put in the configuration");
    assert_eq!(snippet, home_manager_snippet(&target));
    assert!(
        snippet.contains("systemd.user.sockets.svolderd")
            && snippet.contains("systemd.user.services.svolderd"),
        "{snippet}"
    );
    assert!(
        snippet.contains(&format!("ListenStream = \"{RUNTIME_LISTEN_STREAM}\""))
            && snippet.contains("ExecStart = \"/opt/svolder/bin/svolderd\""),
        "{snippet}"
    );
    assert!(!target.dir.exists(), "nothing was written");
    assert!(tools.calls().is_empty(), "systemd was not asked anything");
}

/// In a container the daemon is the container's own process, and nothing is installed.
#[tokio::test]
async fn install_in_a_container_says_the_daemon_is_the_container() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-install-container");
    let runtime = temp_dir("daemon-container-runtime");
    let mut target = target(&home.0, &runtime.0.join("svolder.sock"), Some(&runtime.0));
    target.container = true;

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert!(
        matches!(&installed.done, Ok(Installed::Skipped(why)) if why.contains("container")),
        "nothing was installed, and the answer says so: {:?}",
        installed.done
    );
    assert!(installed.block.is_none(), "a container generates nothing");

    assert!(!target.dir.exists(), "nothing was written");
    assert!(tools.calls().is_empty(), "systemd was not asked anything");
}

/// The one precedence rule: a machine that generates its units, then a container, then systemd.
#[test]
fn nixos_beats_container_beats_systemd() {
    assert_eq!(Service::of(true, true, true), Service::Snippet);
    assert_eq!(Service::of(false, true, true), Service::Container);
    assert_eq!(Service::of(false, false, true), Service::Install);
    assert_eq!(Service::of(false, false, false), Service::None);
}

/// `install` says the container's daemon with the sentence the wizard shares.
#[tokio::test]
async fn install_in_a_container_says_the_shared_container_sentence() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-install-container-sentence");
    let runtime = temp_dir("daemon-container-sentence-runtime");
    let mut target = target(&home.0, &runtime.0.join("svolder.sock"), Some(&runtime.0));
    target.container = true;

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert!(
        installed.said.iter().any(|l| l.contains(CONTAINER_DAEMON)),
        "{:?}",
        installed.said
    );
}

/// A daemon a front end started is asked to stop before systemd is told to take its
/// socket: one daemon holds one device, and systemd binding the path under a running
/// daemon would leave that daemon holding the device and answering to nobody.
#[tokio::test]
async fn install_stops_the_daemon_svolder_started_before_it_enables_the_units() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-install-over-a-daemon");
    let runtime = temp_dir("daemon-install-over-runtime");
    let socket = runtime.0.join("svolder.sock");
    let target = target(&home.0, &socket, Some(&runtime.0));
    let served = a_daemon_on(&socket).await;
    assert!(svolder_ipc::is_running(&socket).await, "the daemon is up");

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert!(
        installed
            .said
            .iter()
            .any(|line| line.starts_with("stopped the daemon svolder had started")),
        "what it did to the running daemon is said: {:?}",
        installed.said
    );
    assert!(
        !svolder_ipc::is_running(&socket).await,
        "the daemon svolder had started is still holding the socket"
    );
    let _ = served.await;

    // And the stop happened before systemd was told about anything: the daemon let the
    // socket go, and only then was the unit enabled on it.
    let enabled = tools
        .calls()
        .iter()
        .position(|call| call.contains("enable"))
        .expect("the socket unit was enabled");
    let stopped = installed
        .said
        .iter()
        .position(|line| line.starts_with("stopped the daemon"))
        .expect("the daemon was stopped");
    let wrote = installed
        .said
        .iter()
        .filter(|line| line.starts_with("wrote "))
        .count();
    assert_eq!(
        stopped, wrote,
        "the stop came after the files and before the rest"
    );
    assert!(enabled > 0, "systemd was asked for the units' states first");
}

/// A machine with no `systemctl` to run has no unit about to take the socket, so the
/// daemon serving on it is left serving: the install writes its files and fails where it
/// always would, with the device still held by the only thing holding it.
#[tokio::test]
async fn install_without_a_systemctl_leaves_a_running_daemon_alone() {
    let nowhere = temp_dir("daemon-install-no-systemctl");
    let home = temp_dir("daemon-install-bare");
    let runtime = temp_dir("daemon-install-bare-runtime");
    let socket = runtime.0.join("svolder.sock");
    let target = target(&home.0, &socket, Some(&runtime.0));
    let served = a_daemon_on(&socket).await;

    let installed = install_into(&Ctl::in_dir(&nowhere.0), &target, false).await;
    let why = installed.done.expect_err("there is no systemctl to ask");
    assert!(why.what.contains("daemon-reload"), "{}", why.what);
    assert!(
        !installed
            .said
            .iter()
            .any(|line| line.starts_with("stopped the daemon")),
        "the daemon serving this machine was stopped: {:?}",
        installed.said
    );
    assert!(
        svolder_ipc::is_running(&socket).await,
        "the daemon serving this machine was stopped"
    );
    for unit in [SOCKET_UNIT, SERVICE_UNIT] {
        assert!(target.dir.join(unit).is_file(), "{unit} was not written");
    }

    let mut client = Client::connect(&socket).await.expect("the daemon answers");
    let _ = client.request(Request::Stop).await;
    let _ = served.await;
}

/// A machine systemd already runs the daemon on is systemd's to stop: the install asks it
/// nothing of its own and lets `enable --now` restart what it manages.
#[tokio::test]
async fn install_where_systemd_already_owns_the_daemon_stops_nothing_itself() {
    let home = temp_dir("daemon-install-over-systemd");
    let runtime = temp_dir("daemon-install-over-systemd-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&installed_units(&socket.display().to_string()));
    let target = target(&home.0, &socket, Some(&runtime.0));
    let served = a_daemon_on(&socket).await;

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert!(
        !installed
            .said
            .iter()
            .any(|line| line.starts_with("stopped the daemon")),
        "a daemon systemd manages was stopped by hand: {:?}",
        installed.said
    );
    assert!(
        svolder_ipc::is_running(&socket).await,
        "the daemon systemd manages was stopped anyway"
    );

    let mut client = Client::connect(&socket).await.expect("the daemon answers");
    let _ = client.request(Request::Stop).await;
    let _ = served.await;
}

/// Installed units that are both stopped hold nothing on the socket, so a daemon answering
/// there is one a front end started, and it is asked to stop before `enable --now` binds the
/// path under it, as on a machine with no units at all.
#[tokio::test]
async fn install_over_stopped_units_stops_the_daemon_svolder_started() {
    let home = temp_dir("daemon-install-over-stopped-units");
    let runtime = temp_dir("daemon-install-over-stopped-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&stopped_units(&socket.display().to_string()));
    let target = target(&home.0, &socket, Some(&runtime.0));
    let served = a_daemon_on(&socket).await;

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert!(
        installed
            .said
            .iter()
            .any(|line| line.starts_with("stopped the daemon svolder had started")),
        "the daemon a front end started was left on the socket: {:?}",
        installed.said
    );
    assert!(
        !svolder_ipc::is_running(&socket).await,
        "the daemon a front end started still holds the socket"
    );
    let _ = served.await;
}

/// `uninstall` disables both units and takes both files away.
#[tokio::test]
async fn uninstall_disables_both_units_and_removes_the_files() {
    let tools = Tools::new(&installed_units(RUNTIME_LISTEN_STREAM));
    let home = temp_dir("daemon-uninstall");
    let runtime = temp_dir("daemon-uninstall-runtime");
    let target = target(&home.0, &runtime.0.join("svolder.sock"), Some(&runtime.0));
    install_into(&tools.ctl(), &target, false)
        .await
        .done
        .expect("the units are installed");

    uninstall_from(&tools.ctl(), &target).expect("the units are taken out");

    assert!(
        !target.dir.join(SOCKET_UNIT).exists(),
        "the socket unit is gone"
    );
    assert!(
        !target.dir.join(SERVICE_UNIT).exists(),
        "the service unit is gone"
    );
    let calls = tools.calls();
    assert!(
        calls.contains(&format!("systemctl --user disable --now {SERVICE_UNIT}"))
            && calls.contains(&format!("systemctl --user disable --now {SOCKET_UNIT}")),
        "{calls:?}"
    );
    assert_eq!(
        calls.last().map(String::as_str),
        Some("systemctl --user daemon-reload"),
        "systemd is told last: {calls:?}"
    );
}

/// As root, `install` writes both units, makes no `systemctl` call, and says how each user
/// enables them.
#[tokio::test]
async fn install_as_root_writes_the_units_and_leaves_enabling_to_each_user() {
    for (on_demand, command) in [
        (
            false,
            "each user enables them with: systemctl --user enable --now svolderd.socket svolderd.service",
        ),
        (
            true,
            "each user enables them with: systemctl --user enable --now svolderd.socket",
        ),
    ] {
        let tools = Tools::new(&no_units());
        let home = temp_dir("daemon-root-install");
        let socket = home.0.join("svolder.sock");
        let target = root_target(&home.0, &socket);

        let installed = install_into(&tools.ctl(), &target, on_demand).await;
        assert_eq!(installed.done.expect("written"), Installed::Written);
        assert_eq!(
            installed.said,
            vec![
                format!(
                    "wrote svolderd.socket and svolderd.service into {}, for every user",
                    target.dir.display()
                ),
                command.to_string(),
            ]
        );
        assert!(target.dir.join(SOCKET_UNIT).exists());
        assert!(target.dir.join(SERVICE_UNIT).exists());
        assert!(tools.calls().is_empty(), "calls: {:?}", tools.calls());
    }
}

/// As root, `uninstall` removes both files and makes no `systemctl` call.
#[test]
fn uninstall_as_root_removes_the_files_and_asks_systemd_nothing() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-root-uninstall");
    let target = root_target(&home.0, &home.0.join("svolder.sock"));
    std::fs::create_dir_all(&target.dir).expect("the unit directory exists");
    for (name, text) in target.units() {
        std::fs::write(target.dir.join(name), text).expect("a unit is written");
    }

    uninstall_from(&tools.ctl(), &target).expect("the units are taken out");
    assert!(!target.dir.join(SOCKET_UNIT).exists());
    assert!(!target.dir.join(SERVICE_UNIT).exists());
    assert!(tools.calls().is_empty(), "calls: {:?}", tools.calls());
}

/// Units written by root name the runtime-directory specifier, because root's own resolved
/// socket is no one else's.
#[tokio::test]
async fn root_units_listen_on_the_runtime_directory_specifier() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-root-specifier");
    let target = root_target(&home.0, Path::new("/tmp/svolder-0/svolder.sock"));
    assert!(target.runtime_socket.is_none());

    let installed = install_into(&tools.ctl(), &target, false).await;
    installed.done.expect("written");
    let text = std::fs::read_to_string(target.dir.join(SOCKET_UNIT)).expect("the socket unit");
    assert!(text.contains("ListenStream=%t/svolder.sock"), "{text}");
}

/// A socket root named is written as the path it named.
#[tokio::test]
async fn root_units_keep_a_socket_the_root_named() {
    let tools = Tools::new(&no_units());
    let home = temp_dir("daemon-root-named");
    let socket = Path::new("/srv/svolder/svolder.sock");
    let target = Target {
        named_socket: true,
        ..root_target(&home.0, socket)
    };

    let installed = install_into(&tools.ctl(), &target, false).await;
    installed.done.expect("written");
    let text = std::fs::read_to_string(target.dir.join(SOCKET_UNIT)).expect("the socket unit");
    assert!(
        text.contains("ListenStream=/srv/svolder/svolder.sock"),
        "{text}"
    );
}

/// Root's units go where every user's manager reads them, unless `SVOLDER_UNITDIR` says
/// otherwise.
#[test]
fn root_units_default_to_the_system_wide_directory() {
    assert_eq!(
        unit_dir_named(true, None),
        PathBuf::from("/etc/systemd/user")
    );
    assert_eq!(
        unit_dir_named(true, Some("/x/units".into())),
        PathBuf::from("/x/units")
    );
}

/// A socket unit running on another path than the one the install writes keeps that path until
/// it is stopped, so the install stops it and its service before `enable --now` starts them on
/// the new one, and says so.
#[tokio::test]
async fn install_moves_a_socket_unit_running_on_another_path() {
    let home = temp_dir("daemon-install-moves-the-socket");
    let runtime = temp_dir("daemon-install-moves-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&installed_units("/run/user/1000/elsewhere.sock"));
    let target = target(&home.0, &socket, Some(&runtime.0));

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert_eq!(
        tools.calls(),
        vec![
            shown_units(),
            "systemctl --user daemon-reload".to_string(),
            format!("systemctl --user stop {SERVICE_UNIT} {SOCKET_UNIT}"),
            format!("systemctl --user enable --now {SOCKET_UNIT}"),
            format!("systemctl --user enable --now {SERVICE_UNIT}"),
        ],
    );
    assert!(
        installed
            .said
            .iter()
            .any(|line| line.contains("/run/user/1000/elsewhere.sock")),
        "{:?}",
        installed.said
    );
}

/// A move with `--on-demand` stops both units, then enables the socket alone.
#[tokio::test]
async fn install_moving_the_socket_on_demand_enables_the_socket_alone() {
    let home = temp_dir("daemon-install-moves-on-demand");
    let runtime = temp_dir("daemon-install-moves-on-demand-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&installed_units("/run/user/1000/elsewhere.sock"));
    let target = target(&home.0, &socket, Some(&runtime.0));

    let installed = install_into(&tools.ctl(), &target, true).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert_eq!(
        tools.calls(),
        vec![
            shown_units(),
            "systemctl --user daemon-reload".to_string(),
            format!("systemctl --user stop {SERVICE_UNIT} {SOCKET_UNIT}"),
            format!("systemctl --user enable --now {SOCKET_UNIT}"),
        ],
    );
    assert!(
        installed
            .said
            .iter()
            .any(|line| line.contains("/run/user/1000/elsewhere.sock")),
        "{:?}",
        installed.said
    );
}

/// The stop of a moved unit is reported even when the enable after it fails.
#[tokio::test]
async fn install_reports_the_stop_of_a_moved_unit_when_the_enable_fails() {
    let home = temp_dir("daemon-install-moves-then-fails");
    let runtime = temp_dir("daemon-install-moves-then-fails-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&installed_units("/run/user/1000/elsewhere.sock"));
    tools.fail_enable();
    let target = target(&home.0, &socket, Some(&runtime.0));

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert!(installed.done.is_err(), "the enable failed");
    assert!(
        installed
            .said
            .iter()
            .any(|line| line.contains("which listened on /run/user/1000/elsewhere.sock")),
        "{:?}",
        installed.said
    );
}

/// A socket unit already listening on the path the install writes is left running.
#[tokio::test]
async fn install_leaves_a_socket_unit_on_its_own_path_running() {
    let home = temp_dir("daemon-install-same-path");
    let runtime = temp_dir("daemon-install-same-path-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&installed_units(&socket.display().to_string()));
    let target = target(&home.0, &socket, Some(&runtime.0));

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert!(
        !tools.calls().iter().any(|call| call.contains(" stop ")),
        "{:?}",
        tools.calls()
    );
}

/// A daemon a front end started on the new path is asked to stop before the units move onto it,
/// as on a machine whose units are stopped.
#[tokio::test]
async fn install_moving_the_socket_stops_the_daemon_on_the_new_path() {
    let home = temp_dir("daemon-install-move-over-a-daemon");
    let runtime = temp_dir("daemon-install-move-over-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&installed_units("/run/user/1000/elsewhere.sock"));
    let target = target(&home.0, &socket, Some(&runtime.0));
    let served = a_daemon_on(&socket).await;

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert!(
        installed
            .said
            .iter()
            .any(|line| line.starts_with("stopped the daemon svolder had started")),
        "{:?}",
        installed.said
    );
    assert!(!svolder_ipc::is_running(&socket).await);
    let _ = served.await;
}

/// Stopped units whose socket unit names another path are started on the new file by
/// `enable --now` as they are: nothing is stopped, nothing is said about moving.
#[tokio::test]
async fn install_over_stopped_units_on_another_path_stops_no_unit() {
    let home = temp_dir("daemon-install-stopped-elsewhere");
    let runtime = temp_dir("daemon-install-stopped-elsewhere-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&units_in(
        "/run/user/1000/elsewhere.sock",
        "inactive",
        "inactive",
    ));
    let target = target(&home.0, &socket, Some(&runtime.0));

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert!(
        !tools.calls().iter().any(|call| call.contains(" stop ")),
        "{:?}",
        tools.calls()
    );
    assert!(
        !installed
            .said
            .iter()
            .any(|line| line.contains("elsewhere.sock")),
        "{:?}",
        installed.said
    );
}

/// Units systemd is still starting hold the socket: a daemon answering there is theirs, and the
/// install asks it nothing of its own.
#[tokio::test]
async fn install_over_units_that_are_starting_stops_nothing_itself() {
    let home = temp_dir("daemon-install-over-starting-units");
    let runtime = temp_dir("daemon-install-over-starting-runtime");
    let socket = runtime.0.join("svolder.sock");
    let tools = Tools::new(&units_in(
        &socket.display().to_string(),
        "activating",
        "activating",
    ));
    let target = target(&home.0, &socket, Some(&runtime.0));
    let served = a_daemon_on(&socket).await;

    let installed = install_into(&tools.ctl(), &target, false).await;
    assert_eq!(
        installed.done.expect("the units are installed"),
        Installed::Units
    );
    assert!(
        !installed
            .said
            .iter()
            .any(|line| line.starts_with("stopped the daemon")),
        "a daemon systemd is starting was stopped by hand: {:?}",
        installed.said
    );
    assert!(svolder_ipc::is_running(&socket).await);

    let mut client = Client::connect(&socket).await.expect("the daemon answers");
    let _ = client.request(Request::Stop).await;
    let _ = served.await;
}
