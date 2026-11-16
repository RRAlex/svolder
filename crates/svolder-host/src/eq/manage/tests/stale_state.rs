// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A state file that no longer matches what runs.

use super::*;

#[tokio::test]
async fn a_re_apply_while_on_replaces_the_unit_cleanly() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    host.apply(request("First")).await.unwrap();
    let second = host.apply(request("Second")).await.unwrap();
    assert_eq!(second.name, "Second");
    assert_eq!(host.state().unwrap().name, "Second");
    assert_eq!(f.written().len(), 3);
    let restarts = f
        .systemctl_writes()
        .iter()
        .filter(|l| l.contains("restart"))
        .count();
    assert_eq!(restarts, 2);
}

#[tokio::test]
async fn an_apply_keeps_the_crossfeed_settings_and_never_starts_with_it_on() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    host.apply(request("First")).await.unwrap();
    let mut state = host.state().unwrap();
    let settings =
        crate::eq::crossfeed::CrossfeedSettings::for_model(crate::eq::crossfeed::Model::MidSide);
    state.crossfeed = Some(settings.clone());
    state.crossfeed_on = true;
    state.flat = true;
    state
        .save(&f.dir.join("state/svolder/host-eq.toml"))
        .unwrap();
    let second = host.apply(request("Second")).await.unwrap();
    assert_eq!(second.crossfeed, Some(settings));
    assert!(
        !second.crossfeed_on,
        "the chain it starts has the stage off"
    );
    assert!(!second.flat, "an applied curve is the EQ's own");
    assert_eq!(host.state().unwrap(), second);
}

#[tokio::test]
async fn a_re_apply_while_on_replaces_the_process_and_stops_the_old_one() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    host.apply(request("First")).await.unwrap();
    let (old, old_argv) = pid_of(&f);
    host.apply(request("Second")).await.unwrap();
    let (new, new_argv) = pid_of(&f);
    assert_ne!(old, new);
    assert!(
        until(|| !old_argv.runs(old)).await,
        "the old process is gone"
    );
    assert!(new_argv.runs(new));
    assert_eq!(host.state().unwrap().name, "Second");
}

#[tokio::test]
async fn changing_manager_stops_the_old_one_and_removes_its_files() {
    let f = Fixture::new();
    f.host(Manager::Systemd).apply(request("A")).await.unwrap();
    f.host(Manager::Process).apply(request("A")).await.unwrap();
    assert!(!f.dir.join("units/svolder-host-eq.service").exists());
    assert!(
        f.systemctl_writes()
            .iter()
            .any(|l| l.contains("disable svolder-host-eq"))
    );
    let (pid, argv) = pid_of(&f);
    assert!(argv.runs(pid));
    f.host(Manager::Process).remove().await.unwrap();
    assert!(until(|| !argv.runs(pid)).await);
    assert_eq!(f.written(), ["state/svolder/host-eq.toml"]);

    f.host(Manager::Process).apply(request("A")).await.unwrap();
    f.host(Manager::Systemd).apply(request("A")).await.unwrap();
    let (pid, argv) = pid_of_or_none(&f).unwrap_or_else(|| (0, Argv::new("none", "none")));
    assert!(
        pid == 0 || !argv.runs(pid),
        "the process is stopped for the unit"
    );
    assert!(f.dir.join("units/svolder-host-eq.service").exists());
    assert!(
        !f.dir
            .join("config/autostart/svolder-host-eq.desktop")
            .exists()
    );
}

#[tokio::test]
async fn a_remove_after_the_process_is_already_gone_succeeds_and_deletes_what_apply_wrote() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    host.apply(request("HD 650")).await.unwrap();
    let (pid, argv) = pid_of(&f);
    signal(pid, libc::SIGKILL);
    assert!(until(|| !argv.runs(pid)).await);
    host.remove().await.expect("removes what is left");
    assert_eq!(f.written(), ["state/svolder/host-eq.toml"]);
    assert!(!host.state().unwrap().on);
}

#[tokio::test]
async fn a_remove_after_the_unit_is_already_gone_succeeds_and_deletes_what_apply_wrote() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    host.apply(request("HD 650")).await.unwrap();
    // The unit file is gone (a reinstall, a hand edit): systemd no longer knows the unit.
    std::fs::remove_file(f.dir.join("units/svolder-host-eq.service")).unwrap();
    host.remove().await.expect("removes what is left");
    assert!(!f.systemctl_writes().iter().any(|l| l.contains("disable")));
    assert_eq!(f.written(), ["state/svolder/host-eq.toml"]);

    // The unit exists but is stopped (after a reboot, before the user manager started it).
    host.apply(request("HD 650")).await.unwrap();
    std::fs::write(f.dir.join("unit-stopped"), "").unwrap();
    host.remove().await.expect("removes a stopped unit");
    assert_eq!(f.written(), ["state/svolder/host-eq.toml"]);
}

#[tokio::test]
async fn a_remove_with_nothing_applied_does_nothing_and_writes_nothing() {
    let f = Fixture::new();
    f.host(Manager::Systemd)
        .remove()
        .await
        .expect("nothing to remove is not an error");
    f.host(Manager::Process)
        .remove()
        .await
        .expect("nor is it for a process");
    assert_eq!(f.written(), Vec::<String>::new());
    assert_eq!(f.systemctl_writes(), Vec::<String>::new());
}

#[tokio::test]
async fn a_pid_another_program_has_been_given_is_never_signalled() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    host.apply(request("HD 650")).await.unwrap();
    let (pid, argv) = pid_of(&f);
    signal(pid, libc::SIGKILL);
    assert!(until(|| !argv.runs(pid)).await);

    // Another program, with the very pid the file names now.
    let mut bystander = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .spawn()
        .expect("a bystander starts");
    std::fs::write(
        f.dir.join("run/svolder-host-eq.pid"),
        format!("{}\n{}\n{}\n", bystander.id(), argv.program(), argv.conf()),
    )
    .unwrap();
    assert!(!host.process_running(), "the bystander is not the chain");
    assert_eq!(host.status().await.running, Some(false));
    host.remove().await.expect("removes");
    assert!(
        bystander.try_wait().unwrap().is_none(),
        "the bystander was left running"
    );
    assert!(!f.dir.join("run/svolder-host-eq.pid").exists());
    let _ = bystander.kill();
    let _ = bystander.wait();
}

#[test]
fn a_command_line_must_be_the_recorded_one_exactly() {
    let argv = Argv::new("/usr/bin/pipewire", "/x/host-eq.conf");
    assert!(!argv.runs(std::process::id()));
    assert!(!argv.runs(0));
    assert!(!argv.runs(u32::MAX));
    assert!(parse_pid_file("1\na\nb\n").is_none(), "pid 1 is never ours");
    assert!(parse_pid_file("x\na\nb\n").is_none());
    assert!(parse_pid_file("5\na\n").is_none());
    let (pid, parsed) = parse_pid_file("5\n/p\n/c\n").unwrap();
    assert_eq!(pid, 5);
    assert_eq!((parsed.program(), parsed.conf()), ("/p", "/c"));
}
