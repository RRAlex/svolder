// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The child manager: a chain that ends with the host.

use super::*;

/// The pid the fake `pipewire` logged, and the command line it runs as.
fn logged_pid(f: &Fixture) -> (u32, Argv) {
    let line = f
        .log("pipewire.log")
        .into_iter()
        .next()
        .expect("it started");
    let pid = line.split(' ').next().unwrap().parse().unwrap();
    let argv = Argv::new(
        &f.dir.join("pipewire").to_string_lossy(),
        &f.dir.join("config/svolder/host-eq.conf").to_string_lossy(),
    );
    (pid, argv)
}

/// A host in a container, with no manager forced.
fn container_host(f: &Fixture) -> Host {
    let mut host = f.host(Manager::Systemd);
    host.forced = None;
    host.container = true;
    host
}

#[tokio::test]
async fn a_container_uses_the_child_manager() {
    let f = Fixture::new();
    let host = container_host(&f);
    assert!(host.unavailable().is_none());
    let state = host.apply(request("HD 650")).await.expect("applies");
    assert_eq!(state.manager, Manager::Child);
    assert_eq!(
        f.written(),
        ["config/svolder/host-eq.conf", "state/svolder/host-eq.toml"],
        "a child writes no unit, entry or pid file"
    );
    assert_eq!(f.systemctl_writes(), Vec::<String>::new());
    assert_eq!(host.status().await.running, Some(true));
    let (pid, argv) = logged_pid(&f);
    assert!(argv.runs(pid), "the chain does not run as a child");
    host.remove().await.expect("removes");
    assert!(until(|| !argv.runs(pid)).await, "the child did not stop");
    assert_eq!(host.status().await.running, Some(false));
    assert_eq!(f.written(), ["state/svolder/host-eq.toml"]);
}

#[tokio::test]
async fn the_child_ends_with_the_host() {
    let f = Fixture::new();
    let host = container_host(&f);
    host.apply(request("HD 650")).await.expect("applies");
    let (pid, argv) = logged_pid(&f);
    assert!(argv.runs(pid), "the chain does not run");
    drop(host);
    assert!(
        until(|| !argv.runs(pid)).await,
        "the child outlived its host"
    );
}

#[tokio::test]
async fn the_child_manager_can_be_forced() {
    let f = Fixture::new();
    let paths = EqPaths {
        conf: f.dir.join("config/svolder/host-eq.conf"),
        state: f.dir.join("state/svolder/host-eq.toml"),
        unit_dir: f.dir.join("units"),
        autostart_dir: f.dir.join("config/autostart"),
        pid_file: Some(f.dir.join("run/svolder-host-eq.pid")),
    };
    // Nothing else of this crate's tests reads these variables: they build their hosts by hand.
    let set = [
        ("SVOLDER_HOST_EQ_MANAGER", "child".into()),
        ("SVOLDER_PIPEWIRE", f.dir.join("pipewire")),
        ("SVOLDER_WPCTL", f.dir.join("wpctl")),
        ("SVOLDER_WIREPLUMBER", f.dir.join("wireplumber")),
        ("SVOLDER_SYSTEMCTL", "/bin/false".into()),
        ("SVOLDER_HOST", "on".into()),
    ];
    for (name, value) in &set {
        // SAFETY: no other test of this crate reads or writes these variables.
        unsafe { std::env::set_var(name, value) };
    }
    let host = Host::live(paths, false);
    for (name, _) in &set {
        // SAFETY: as above.
        unsafe { std::env::remove_var(name) };
    }
    let state = host.apply(request("HD 650")).await.expect("applies");
    assert_eq!(state.manager, Manager::Child);
    assert_eq!(
        f.written(),
        ["config/svolder/host-eq.conf", "state/svolder/host-eq.toml"]
    );
    assert_eq!(host.status().await.running, Some(true));
    assert_eq!(Manager::parse(" Child "), Some(Manager::Child));
    assert_eq!(Manager::Child.word(), "child");
}

#[tokio::test]
async fn a_held_host_keeps_its_child_across_calls_and_dropping_it_ends_the_child() {
    let f = Fixture::new();
    let host = container_host(&f);
    host.apply(request("A")).await.expect("applies");
    let (first, argv) = logged_pid(&f);
    assert_eq!(host.status().await.running, Some(true));
    assert!(argv.runs(first), "the child ended between two calls");
    // A second apply replaces the child; the host still holds one.
    host.apply(request("B")).await.expect("applies again");
    assert!(until(|| !argv.runs(first)).await, "the old child stayed");
    let second: u32 = f
        .log("pipewire.log")
        .last()
        .and_then(|line| line.split(' ').next()?.parse().ok())
        .expect("the second child logged");
    assert_ne!(first, second);
    assert!(argv.runs(second));
    assert_eq!(host.status().await.running, Some(true));
    drop(host);
    assert!(
        until(|| !argv.runs(second)).await,
        "the child outlived the host"
    );
}
