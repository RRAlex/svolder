// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A correction that is out of range, and the preamp that keeps what plays under unity.

use super::*;

#[tokio::test]
async fn a_correction_out_of_range_is_refused_naming_the_filter_before_anything_changes() {
    let f = Fixture::new();
    for (q, g, f0, said) in [
        (0.0, 3.0, 1000.0, "filter 2: Q 0"),
        (-1.0, 3.0, 1000.0, "filter 2: Q -1"),
        (f64::NAN, 3.0, 1000.0, "filter 2: Q NaN"),
        (1.0, f64::INFINITY, 1000.0, "filter 2: Gain inf"),
        (1.0, 3.0, 0.0, "filter 2: Fc 0"),
    ] {
        let mut asked = request("Bad");
        asked.correction.filters.push(SourceFilter {
            kind: FilterKind::Peaking,
            f: f0,
            g,
            q,
        });
        let failure = f.host(Manager::Systemd).apply(asked).await.unwrap_err();
        assert!(failure.what.contains(said), "{}", failure.what);
    }
    let mut nan_preamp = request("Bad");
    nan_preamp.correction.preamp_db = f64::NAN;
    assert!(f.host(Manager::Systemd).apply(nan_preamp).await.is_err());
    let mut none = request("Bad");
    none.correction.filters.clear();
    assert!(f.host(Manager::Systemd).apply(none).await.is_err());
    assert_eq!(f.written(), Vec::<String>::new());
    assert_eq!(f.systemctl_writes(), Vec::<String>::new());
}

#[tokio::test]
async fn a_refused_re_apply_leaves_the_chain_that_is_on_running() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    host.apply(request("First")).await.unwrap();
    let (pid, argv) = pid_of(&f);
    let conf = std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap();
    let mut bad = request("Second");
    bad.correction.filters[0].q = 0.0;
    assert!(host.apply(bad).await.is_err());
    let mut elsewhere = request("Third");
    elsewhere.sink = Some("99".into());
    assert!(host.apply(elsewhere).await.is_err());
    assert!(argv.runs(pid), "the chain still runs");
    assert_eq!(
        std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap(),
        conf
    );
    let state = host.state().unwrap();
    assert!(state.on);
    assert_eq!(state.name, "First");
}

/// A directory made read-only for as long as the guard lives, as a full disk or a directory
/// that cannot be written leaves a write; dropped, it gives the directory its mode back so the
/// fixture can take it away.
struct Locked(PathBuf);

impl Locked {
    fn new(dir: PathBuf) -> Locked {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        Locked(dir)
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
    }
}

/// Every file under the directories a manager writes, with what it holds.
fn contents(f: &Fixture) -> Vec<(String, String)> {
    f.written()
        .into_iter()
        .map(|file| {
            let text = std::fs::read_to_string(f.dir.join(&file)).unwrap_or_default();
            (file, text)
        })
        .collect()
}

/// A re-apply whose files cannot be written, or whose old files cannot be deleted, fails before
/// anything of the computer EQ that is on is touched, under either manager and whichever
/// directory refuses, and before the other manager's chain is stopped on a switch: the chain
/// runs on, on its own files, systemctl is asked nothing, and the state still names it.
#[tokio::test]
async fn a_re_apply_that_cannot_write_its_files_leaves_the_chain_that_is_on_running() {
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        // Root writes into a directory whatever its mode says, so there is no refusal to see.
        return;
    }
    for (first, then, dir) in [
        (Manager::Process, Manager::Process, "config/autostart"),
        (Manager::Process, Manager::Process, "config/svolder"),
        (Manager::Systemd, Manager::Systemd, "units"),
        (Manager::Systemd, Manager::Systemd, "config/svolder"),
        (Manager::Systemd, Manager::Process, "config/svolder"),
        (Manager::Process, Manager::Systemd, "units"),
        // The files a switch deletes of the manager it leaves, and the pid the process
        // manager keeps, are in directories of their own.
        (Manager::Systemd, Manager::Process, "units"),
        (Manager::Process, Manager::Systemd, "run"),
        (Manager::Process, Manager::Process, "run"),
        (Manager::Systemd, Manager::Systemd, "state/svolder"),
    ] {
        let case = format!("{first:?} then {then:?}, {dir} read-only");
        let f = Fixture::new();
        f.host(first).apply(request("First")).await.unwrap();
        let files = contents(&f);
        let asked = f.systemctl_writes();
        let running = pid_of_or_none(&f);
        let host = f.host(then);
        {
            let _locked = Locked::new(f.dir.join(dir));
            assert!(
                host.apply(request("Second")).await.is_err(),
                "{case}: the apply went through"
            );
        }
        if let Some((pid, argv)) = running {
            assert!(argv.runs(pid), "{case}: the chain was stopped");
        }
        assert_eq!(f.systemctl_writes(), asked, "{case}");
        assert_eq!(contents(&f), files, "{case}");
        let state = host.state().unwrap();
        assert!(state.on, "{case}: the state was turned off");
        assert_eq!(state.name, "First", "{case}");
    }
}

/// A switch away from systemd whose unit link cannot be deleted fails before the unit is
/// stopped: the link's own directory is tried as the unit's is.
#[tokio::test]
async fn a_switch_whose_unit_link_cannot_be_deleted_leaves_the_unit_running() {
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        // Root writes into a directory whatever its mode says, so there is no refusal to see.
        return;
    }
    let f = Fixture::new();
    f.host(Manager::Systemd)
        .apply(request("First"))
        .await
        .unwrap();
    // The fake systemctl makes no link for `enable`; this one stands in for it.
    let wants = f.dir.join("units/pipewire.service.wants");
    std::fs::create_dir_all(&wants).unwrap();
    std::os::unix::fs::symlink(
        f.dir.join("units/svolder-host-eq.service"),
        wants.join("svolder-host-eq.service"),
    )
    .unwrap();
    let files = contents(&f);
    let asked = f.systemctl_writes();
    {
        let _locked = Locked::new(wants);
        assert!(
            f.host(Manager::Process)
                .apply(request("Second"))
                .await
                .is_err()
        );
    }
    assert_eq!(f.systemctl_writes(), asked, "the unit was stopped");
    assert_eq!(contents(&f), files);
}

/// A process apply where there is no runtime directory for its pid is refused before anything
/// changes: a computer EQ the unit runs stays on.
#[tokio::test]
async fn a_process_apply_with_no_runtime_directory_leaves_the_unit_running() {
    let f = Fixture::new();
    f.host(Manager::Systemd)
        .apply(request("First"))
        .await
        .unwrap();
    let files = contents(&f);
    let asked = f.systemctl_writes();
    let host = Host {
        pid_file: None,
        ..f.host(Manager::Process)
    };
    assert!(host.apply(request("Second")).await.is_err());
    assert_eq!(f.systemctl_writes(), asked, "the unit was stopped");
    assert_eq!(contents(&f), files);
    let state = host.state().unwrap();
    assert!(state.on);
    assert_eq!(state.name, "First");
}

#[tokio::test]
async fn a_default_route_switch_that_fails_puts_the_default_back() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("wpver"), "0.4.17").unwrap();
    std::fs::write(f.dir.join("restart-fails"), "").unwrap();
    assert!(f.host(Manager::Systemd).apply(request("A")).await.is_err());
    assert_eq!(f.default_id(), "42");
    assert_eq!(f.written(), Vec::<String>::new());
}
