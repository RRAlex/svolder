// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What `status` says of the state and of what runs.

use super::*;

#[tokio::test]
async fn status_reads_the_state_and_asks_the_manager() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    assert_eq!(
        host.status().await,
        Status {
            state: None,
            running: None,
            unreadable: None
        }
    );
    assert_eq!(host.status().await.lines(), ["host EQ: off"]);
    assert_eq!(
        host.status().await.json(),
        serde_json::json!({"on": false, "running": null})
    );

    host.apply(request("HD 650")).await.unwrap();
    let on = host.status().await;
    assert_eq!(on.running, Some(true));
    assert_eq!(
        on.lines(),
        ["host EQ: HD 650 · on · in front of dac · systemd"]
    );
    let json = on.json();
    assert_eq!(json["on"], true);
    assert_eq!(json["route"], "smart");
    assert_eq!(json["manager"], "systemd");
    assert_eq!(json["target"], "dac");
    assert_eq!(json["preamp_db"], -4.5);
    assert_eq!(
        json["filters"][0],
        serde_json::json!({"type": "PK", "f": 1000.0, "g": 3.0, "q": 1.0})
    );
    assert_eq!(json["problem"], serde_json::Value::Null);

    std::fs::write(f.dir.join("unit-stopped"), "").unwrap();
    let stopped = host.status().await;
    assert_eq!(
        stopped.lines(),
        [
            "host EQ: HD 650 · stopped · in front of dac · systemd".to_string(),
            "the unit is not running: systemctl --user status svolder-host-eq".to_string()
        ]
    );
    std::fs::remove_file(f.dir.join("unit-stopped")).unwrap();

    host.remove().await.unwrap();
    assert_eq!(host.status().await.lines(), ["host EQ: off (last: HD 650)"]);
}

#[tokio::test]
async fn status_of_a_process_names_the_apply_that_brings_it_back() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    host.apply(request("HD 650")).await.unwrap();
    assert_eq!(host.status().await.running, Some(true));
    let (pid, argv) = pid_of(&f);
    signal(pid, libc::SIGKILL);
    assert!(until(|| !argv.runs(pid)).await);
    let status = host.status().await;
    assert_eq!(status.running, Some(false));
    assert_eq!(
        status.problem().as_deref(),
        Some("the process is not running: apply it again with svolder host eq apply")
    );
}

#[tokio::test]
async fn a_chain_that_is_off_in_the_state_but_runs_is_said() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    let mut state = host.apply(request("HD 650")).await.unwrap();
    state.on = false;
    state.save(&host.state).unwrap();
    let status = host.status().await;
    assert_eq!(
        status.problem().as_deref(),
        Some("the unit is still running: svolder host eq remove")
    );
}

#[tokio::test]
async fn with_the_host_off_status_asks_nobody() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    host.apply(request("HD 650")).await.unwrap();
    let mut off = f.host(Manager::Systemd);
    off.off = true;
    let before = f.log("systemctl.log").len();
    assert_eq!(off.status().await.running, None);
    assert_eq!(f.log("systemctl.log").len(), before);
}

#[tokio::test]
async fn an_unreadable_state_is_said_and_not_shown_as_off() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    std::fs::create_dir_all(f.dir.join("state/svolder")).unwrap();
    std::fs::write(f.dir.join("state/svolder/host-eq.toml"), "on = \"maybe\"").unwrap();
    let status = host.status().await;
    assert!(status.unreadable.is_some());
    assert!(
        status.lines()[0].starts_with("host EQ: unknown: "),
        "{:?}",
        status.lines()
    );
    assert_eq!(status.json()["on"], serde_json::Value::Null);
    assert!(status.json()["problem"].is_string());
}
