// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The paths and the manager word.

use super::*;

#[tokio::test]
async fn a_path_the_files_cannot_carry_is_refused() {
    let f = Fixture::new();
    for bad in ["config/new\nline", "config/tab\tdir"] {
        let mut host = f.host(Manager::Systemd);
        host.conf = f.dir.join(bad).join("host-eq.conf");
        let failure = host.apply(request("A")).await.unwrap_err();
        assert!(failure.what.contains("character"), "{}", failure.what);
        let mut pipewire = f.host(Manager::Process);
        pipewire.pipewire = Some(f.dir.join(bad));
        assert!(pipewire.apply(request("A")).await.is_err());
    }
    assert_eq!(f.written(), Vec::<String>::new());
}

#[tokio::test]
async fn a_manager_word_that_names_none_is_refused_with_the_accepted_ones() {
    let f = Fixture::new();
    let mut host = f.host(Manager::Systemd);
    host.forced = None;
    host.bad_manager = Some("launchd".into());
    let failure = host.apply(request("A")).await.unwrap_err();
    assert!(failure.what.contains("launchd"), "{}", failure.what);
    assert!(
        ["systemd", "process", "child"]
            .iter()
            .all(|word| failure.next.contains(word)),
        "{}",
        failure.next
    );
    assert_eq!(f.written(), Vec::<String>::new());
}
