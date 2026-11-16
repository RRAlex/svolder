// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an apply refuses, and what a refusal leaves as it was.

use super::*;

#[tokio::test]
async fn the_host_being_off_and_a_missing_pipewire_refuse_before_anything_is_written() {
    let f = Fixture::new();
    let mut off = f.host(Manager::Systemd);
    off.off = true;
    let failure = off.apply(request("A")).await.unwrap_err();
    assert!(
        failure.what.contains("SVOLDER_HOST=off"),
        "{}",
        failure.what
    );
    assert!(off.remove().await.is_err());

    let mut none = f.host(Manager::Systemd);
    none.pipewire = None;
    let failure = none.apply(request("A")).await.unwrap_err();
    assert!(failure.what.contains("PipeWire"), "{}", failure.what);
    assert!(none.unavailable().is_some());

    assert_eq!(f.written(), Vec::<String>::new());
    assert_eq!(f.log("systemctl.log"), Vec::<String>::new());
}

#[tokio::test]
async fn without_wireplumber_or_a_version_it_says_so() {
    let f = Fixture::new();
    let mut host = f.host(Manager::Systemd);
    host.wireplumber = f.dir.join("no-such-wireplumber").into();
    let failure = host.apply(request("A")).await.unwrap_err();
    assert!(
        failure.what.contains("WirePlumber is needed"),
        "{}",
        failure.what
    );
    assert!(
        failure.next.contains("SVOLDER_WIREPLUMBER"),
        "{}",
        failure.next
    );
    std::fs::write(f.dir.join("wpver"), "unknown").unwrap();
    let failure = f
        .host(Manager::Systemd)
        .apply(request("A"))
        .await
        .unwrap_err();
    assert!(failure.what.contains("does not say"), "{}", failure.what);
    assert_eq!(f.written(), Vec::<String>::new());
}

/// The version comes from `wireplumber --version`; `wpctl`, which has no such option, is never
/// asked for one.
#[tokio::test]
async fn the_version_comes_from_wireplumber_and_never_from_wpctl() {
    let f = Fixture::new();
    f.host(Manager::Systemd).apply(request("A")).await.unwrap();
    let asked = std::fs::read_to_string(f.dir.join("wireplumber.log")).unwrap_or_default();
    assert_eq!(asked.lines().collect::<Vec<_>>(), ["--version"]);
    let wpctl = std::fs::read_to_string(f.dir.join("wpctl.log")).unwrap_or_default();
    assert!(!wpctl.contains("--version"), "{wpctl}");
}
