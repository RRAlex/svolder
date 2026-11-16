// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The systemd manager: the unit, its files, and `systemctl` that is slow or refuses.

use super::*;

#[tokio::test]
async fn systemd_apply_writes_the_conf_and_the_unit_then_reloads_enables_and_restarts() {
    let f = Fixture::new();
    let state = f
        .host(Manager::Systemd)
        .apply(request("HD 650"))
        .await
        .expect("applies");
    assert_eq!(
        f.systemctl_writes(),
        [
            "--user daemon-reload",
            "--user enable svolder-host-eq.service",
            "--user restart svolder-host-eq.service",
        ]
    );
    assert_eq!(
        f.written(),
        [
            "config/svolder/host-eq.conf",
            "state/svolder/host-eq.toml",
            "units/svolder-host-eq.service"
        ]
    );
    let conf = std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap();
    assert!(
        conf.contains("filter.smart.target = { node.name = \"dac\" }"),
        "{conf}"
    );
    assert!(
        conf.contains("name = lpre label = linear control = { Mult = 0.5956"),
        "{conf}"
    );
    let unit = std::fs::read_to_string(f.dir.join("units/svolder-host-eq.service")).unwrap();
    assert!(
        unit.contains(&format!(
            "ExecStart={} -c {}\n",
            f.dir.join("pipewire").display(),
            f.dir.join("config/svolder/host-eq.conf").display()
        )),
        "{unit}"
    );
    assert_eq!(state.route, Route::Smart);
    assert_eq!(state.manager, Manager::Systemd);
    assert_eq!(state.target, "dac");
    assert_eq!(state.previous_default, None);
    assert_eq!(
        HostEqState::load(&f.dir.join("state/svolder/host-eq.toml")),
        Ok(Some(state))
    );
    // The smart route leaves the default output alone.
    assert_eq!(f.default_id(), "42");
    assert!(
        !f.log("wpctl.log")
            .iter()
            .any(|line| line.starts_with("set-default"))
    );
}

#[tokio::test]
async fn systemd_remove_disables_the_unit_and_deletes_what_apply_wrote() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    host.apply(request("HD 650")).await.unwrap();
    host.remove().await.expect("removes");
    let writes = f.systemctl_writes();
    assert_eq!(
        writes[3..],
        [
            "--user stop svolder-host-eq.service",
            "--user disable svolder-host-eq.service",
            "--user daemon-reload"
        ]
    );
    // Only the state stays, off, with the correction it had.
    assert_eq!(f.written(), ["state/svolder/host-eq.toml"]);
    let state = host.state().expect("the state stays");
    assert!(!state.on);
    assert_eq!(state.name, "HD 650");
    assert_eq!(state.preamp_db, -4.5);
    assert_eq!(state.filters.len(), 1);
}

#[tokio::test]
async fn a_failed_restart_is_said_and_leaves_nothing_behind() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("restart-fails"), "").unwrap();
    let failure = f
        .host(Manager::Systemd)
        .apply(request("HD 650"))
        .await
        .unwrap_err();
    assert!(failure.what.contains("unit is masked"), "{}", failure.what);
    assert_eq!(f.written(), Vec::<String>::new());
}

#[tokio::test]
async fn a_unit_that_is_not_active_after_the_restart_is_a_failure() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("unit-stopped"), "").unwrap();
    let failure = f
        .host(Manager::Systemd)
        .apply(request("HD 650"))
        .await
        .unwrap_err();
    assert!(failure.what.contains("did not start"), "{}", failure.what);
    assert!(
        failure.next.contains("systemctl --user status"),
        "{}",
        failure.next
    );
    assert_eq!(f.written(), Vec::<String>::new());
}

#[tokio::test]
async fn remove_tolerates_what_systemctl_says_is_not_there_and_deletes_the_link() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    host.apply(request("A")).await.unwrap();
    // The unit file was deleted by hand while systemd still has it loaded, and what it is
    // asked to stop and disable is not there.
    std::fs::remove_file(f.dir.join("units/svolder-host-eq.service")).unwrap();
    std::fs::write(f.dir.join("loaded-anyway"), "").unwrap();
    std::fs::write(f.dir.join("stop-notloaded"), "").unwrap();
    std::fs::write(f.dir.join("disable-missing"), "").unwrap();
    let wants = f.dir.join("units/pipewire.service.wants");
    std::fs::create_dir_all(&wants).unwrap();
    std::os::unix::fs::symlink(
        f.dir.join("units/gone.service"),
        wants.join("svolder-host-eq.service"),
    )
    .unwrap();
    host.remove().await.expect("removes");
    assert!(
        wants
            .join("svolder-host-eq.service")
            .symlink_metadata()
            .is_err(),
        "the dangling link is gone"
    );
    let writes = f.systemctl_writes();
    assert_eq!(
        writes[3..],
        [
            "--user stop svolder-host-eq.service",
            "--user disable svolder-host-eq.service",
            "--user daemon-reload"
        ]
    );
    assert_eq!(f.written(), ["state/svolder/host-eq.toml"]);
}

#[tokio::test]
async fn remove_does_not_delete_files_when_systemctl_refuses_for_another_reason() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    host.apply(request("A")).await.unwrap();
    std::fs::write(f.dir.join("disable-fails"), "").unwrap();
    let failure = host.remove().await.unwrap_err();
    assert!(failure.what.contains("access denied"), "{}", failure.what);
    assert!(f.dir.join("units/svolder-host-eq.service").exists());
    assert!(host.state().unwrap().on);
}

#[tokio::test]
async fn a_systemd_apply_leaves_the_active_units_own_chain_to_systemctl() {
    let f = Fixture::new();
    let process = f.host(Manager::Process);
    // A chain that runs as the unit's (same command line) while the unit is active.
    process.apply(request("First")).await.unwrap();
    let (pid, argv) = pid_of(&f);
    std::fs::write(f.dir.join("units/svolder-host-eq.service"), "[Unit]\n").unwrap();
    f.host(Manager::Systemd)
        .apply(request("Second"))
        .await
        .unwrap();
    assert!(
        argv.runs(pid),
        "the unit's chain was not signalled by svolder"
    );
    // With no unit active, a process-manager chain is stopped before the unit starts.
    let g = Fixture::new();
    g.host(Manager::Process)
        .apply(request("First"))
        .await
        .unwrap();
    let (pid, argv) = pid_of(&g);
    g.host(Manager::Systemd)
        .apply(request("Second"))
        .await
        .unwrap();
    assert!(until(|| !argv.runs(pid)).await, "the leftover was stopped");
}

/// A `systemctl` that takes a second to answer anything.
pub(super) const SLOW_SYSTEMCTL: &str = "#!/bin/sh\nsleep 1\necho ActiveState=active\nexit 0\n";

/// A slow `systemctl` does not hold the runtime: a 20 ms interval on the same single thread
/// keeps ticking while a unit starts.
#[tokio::test(flavor = "current_thread")]
async fn a_slow_systemctl_does_not_stall_the_runtime() {
    let f = Fixture::new();
    let slow = f.dir.join("slow-systemctl");
    script::install_script(&slow, SLOW_SYSTEMCTL);
    let mut host = f.host(Manager::Systemd);
    host.ctl = UserSystemd::with_program(&slow);
    let staged = stage(&host.unit_path(), "[Unit]\n").unwrap();

    let ticks = std::sync::Arc::new(AtomicU32::new(0));
    let counter = ticks.clone();
    let ticker = tokio::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_millis(20));
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            counter.fetch_add(1, Ordering::Relaxed);
        }
    });
    host.start_unit(staged).await.unwrap();
    ticker.abort();
    let seen = ticks.load(Ordering::Relaxed);
    assert!(
        seen >= 10,
        "the runtime ticked {seen} times in four slow calls"
    );
}
