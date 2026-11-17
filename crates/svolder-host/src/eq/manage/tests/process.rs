// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The process manager: the chain as a detached process, found by its command line.

use super::*;

#[tokio::test]
async fn process_apply_starts_pipewire_detached_and_writes_the_pid_and_the_autostart_entry() {
    let f = Fixture::new();
    let state = f
        .host(Manager::Process)
        .apply(request("HD 650"))
        .await
        .expect("applies");
    assert_eq!(state.manager, Manager::Process);
    let (pid, argv) = pid_of(&f);
    assert!(argv.runs(pid));
    assert!(argv.conf().ends_with("host-eq.conf"));
    // A session of its own: the process leads one, so a terminal closing does not reach it.
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let after_name = stat
        .rsplit(')')
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(after_name[3], pid.to_string(), "the session id of {stat}");
    assert_eq!(
        f.written(),
        [
            "config/autostart/svolder-host-eq.desktop",
            "config/svolder/host-eq.conf",
            "run/svolder-host-eq.pid",
            "state/svolder/host-eq.toml"
        ]
    );
    let desktop =
        std::fs::read_to_string(f.dir.join("config/autostart/svolder-host-eq.desktop")).unwrap();
    assert!(desktop.contains("Type=Application\n") && desktop.contains("NoDisplay=true\n"));
    assert!(desktop.contains(&format!("Exec={} -c ", f.dir.join("pipewire").display())));
    // systemd was asked only whether it knows a unit, never to change one.
    assert_eq!(f.systemctl_writes(), Vec::<String>::new());
}

#[tokio::test]
async fn process_remove_stops_the_process_after_checking_its_command_line() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    host.apply(request("HD 650")).await.unwrap();
    let (pid, argv) = pid_of(&f);
    host.remove().await.expect("removes");
    assert!(until(|| !argv.runs(pid)).await, "the process is stopped");
    assert_eq!(f.written(), ["state/svolder/host-eq.toml"]);
    assert!(!host.state().unwrap().on);
}

#[tokio::test]
async fn a_process_that_exits_at_once_is_a_failure_and_leaves_nothing_behind() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("pipewire-dies"), "").unwrap();
    let failure = f
        .host(Manager::Process)
        .apply(request("HD 650"))
        .await
        .unwrap_err();
    assert!(failure.what.contains("exited"), "{}", failure.what);
    assert_eq!(f.written(), Vec::<String>::new());
}

/// A chain held by its pidfd takes the signal sent through the hold, and once it has exited
/// and been reaped a signal through the same hold reaches nothing, whatever runs at its pid.
#[tokio::test]
async fn a_held_chain_takes_its_signal_and_a_reaped_one_takes_none() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    host.apply(request("HD 650")).await.unwrap();
    let (pid, argv) = pid_of(&f);
    let held = Held::open(pid, &argv).expect("the chain is held");
    assert_eq!(held.pid(), pid);
    assert!(held.by_pidfd(), "the chain is not held by a pidfd");
    assert!(
        held.signal(libc::SIGTERM),
        "the chain did not take the signal"
    );
    assert!(until(|| !argv.runs(pid)).await, "the chain did not stop");
    // `spawn_detached` left a thread waiting on the chain, so it is reaped and its pid free.
    assert!(
        until(|| !Path::new(&format!("/proc/{pid}")).exists()).await,
        "the chain was not reaped"
    );
    assert!(
        !held.signal(0),
        "a signal went through the hold of a reaped chain"
    );
}

/// A refused `pidfd_open` leaves no hold only when the process is not there; any other refusal
/// holds the pid itself, so a running chain never looks gone.
#[test]
fn a_refused_pidfd_open_holds_the_pid_unless_the_process_is_not_there() {
    for errno in [libc::ESRCH, libc::EINVAL] {
        assert!(
            Held::after_refusal(7, Some(errno)).is_none(),
            "errno {errno} held"
        );
    }
    for errno in [libc::ENOSYS, libc::EPERM, libc::EMFILE] {
        let held = Held::after_refusal(7, Some(errno)).expect("the pid is held");
        assert_eq!(held.pid(), 7);
        assert!(!held.by_pidfd());
    }
}

/// Kills and reaps a child on every path, so a failed assert leaks nothing.
struct Reaped(std::process::Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A pid whose command line is anything but the chain's is never held, so nothing is ever
/// signalled through it.
#[test]
fn a_pid_that_runs_anything_else_is_never_held() {
    let mut bystander = Reaped(
        Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .expect("a bystander starts"),
    );
    let argv = Argv::new("/usr/bin/pipewire", "/x/host-eq.conf");
    assert!(
        Held::open(bystander.0.id(), &argv).is_none(),
        "the bystander was held"
    );
    assert!(Held::open(0, &argv).is_none());
    assert!(Held::open(u32::MAX, &argv).is_none());
    assert!(
        bystander.0.try_wait().unwrap().is_none(),
        "the bystander was signalled"
    );
}

/// Start the fake pipewire the way a login does: with the program and the config, and no pid
/// file written.
fn start_stray(f: &Fixture, program: &str) -> u32 {
    let conf = f.dir.join("config/svolder/host-eq.conf");
    std::fs::create_dir_all(conf.parent().unwrap()).unwrap();
    std::fs::write(&conf, "# conf\n").unwrap();
    spawn_detached(&f.dir.join(program), &conf)
        .expect("the stray chain starts")
        .pid()
}

#[tokio::test]
async fn a_chain_started_at_login_with_no_pid_file_is_seen_stopped_and_replaced() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    let mut state = host.apply(request("HD 650")).await.unwrap();
    // A reboot: the runtime directory, and with it the pid file, is gone, and the autostart
    // entry has started the chain on its own.
    let (first, argv) = pid_of(&f);
    signal(first, libc::SIGKILL);
    assert!(until(|| !argv.runs(first)).await);
    std::fs::remove_file(f.dir.join("run/svolder-host-eq.pid")).unwrap();
    let stray = start_stray(&f, "pipewire");
    // Until the exec has set up its command line, the new process does not run the fake.
    assert!(until(|| argv.runs(stray)).await, "the stray did not start");
    assert_eq!(host.status().await.running, Some(true));
    assert_eq!(host.status().await.problem(), None);

    // Applying replaces it: one chain runs afterwards, and it is the new one.
    state.name = "Second".into();
    host.apply(request("Second")).await.unwrap();
    assert!(
        until(|| !argv.runs(stray)).await,
        "the old chain is stopped"
    );
    assert_eq!(host.chains().len(), 1);

    // Removing finds a chain that has no pid file too.
    std::fs::remove_file(f.dir.join("run/svolder-host-eq.pid")).unwrap();
    let (pid, argv) = host.chains().pop().expect("the chain runs");
    host.remove().await.unwrap();
    assert!(until(|| !argv.runs(pid)).await, "remove stopped it");
    assert!(host.chains().is_empty());
    assert_eq!(host.status().await.running, Some(false));
}

#[tokio::test]
async fn a_chain_the_autostart_entry_started_with_another_pipewire_is_found_by_the_entry() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    std::os::unix::fs::symlink(masters().join("pipewire"), f.dir.join("pipewire-old")).unwrap();
    let conf = f.dir.join("config/svolder/host-eq.conf");
    let stray = start_stray(&f, "pipewire-old");
    std::fs::create_dir_all(f.dir.join("config/autostart")).unwrap();
    std::fs::write(
        f.dir.join("config/autostart/svolder-host-eq.desktop"),
        render_desktop(&f.dir.join("pipewire-old"), &conf),
    )
    .unwrap();
    host.apply(request("Now")).await.unwrap();
    let old = Argv::new(
        &f.dir.join("pipewire-old").to_string_lossy(),
        &conf.to_string_lossy(),
    );
    assert!(
        until(|| !old.runs(stray)).await,
        "the chain the entry named is replaced"
    );
    host.remove().await.unwrap();
    assert!(host.chains().is_empty());
}

/// A fixture stops every fake run from its directory when it is dropped, a stray started under
/// another name than `pipewire` included.
#[tokio::test]
async fn a_fixture_stops_every_fake_of_its_directory_when_dropped() {
    let f = Fixture::new();
    std::os::unix::fs::symlink(masters().join("pipewire"), f.dir.join("pipewire-old")).unwrap();
    let stray = start_stray(&f, "pipewire-old");
    let argv = Argv::new(
        &f.dir.join("pipewire-old").to_string_lossy(),
        &f.dir.join("config/svolder/host-eq.conf").to_string_lossy(),
    );
    assert!(until(|| argv.runs(stray)).await, "the stray did not start");
    drop(f);
    assert!(
        until(|| !argv.runs(stray)).await,
        "the stray outlived its fixture"
    );
}

/// The sweep test's own root: whatever still runs from it is stopped before it is taken away,
/// so a failed assertion leaves no fake behind that no later sweep could find.
struct SweepRoot(PathBuf);

impl Drop for SweepRoot {
    fn drop(&mut self) {
        for fake in running_from(&self.0) {
            signal(fake, libc::SIGKILL);
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for SweepRoot {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

/// The fixtures of a test run that ended without dropping them are swept by the next run: their
/// fakes stopped, and their directories taken away once nothing runs from them. A directory of a
/// run still going is left alone.
#[tokio::test]
async fn the_fixtures_of_a_run_that_is_over_are_swept() {
    // The sweep runs over a root of this test's own: another test process sweeping the shared
    // temp dir at the same moment would otherwise take these directories as its own finished
    // runs (its names are not a run's, so that sweep leaves the root alone).
    let root =
        SweepRoot(std::env::temp_dir().join(format!("svhe-sweep-test-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&*root);
    // No process ever has the largest pid, so this run is always over.
    let left = root.join(format!("svhe-{}-0", u32::MAX));
    std::fs::create_dir_all(&left).unwrap();
    std::os::unix::fs::symlink(masters().join("pipewire"), left.join("pipewire")).unwrap();
    let conf = left.join("host-eq.conf");
    std::fs::write(&conf, "# conf\n").unwrap();
    let fake = spawn_detached(&left.join("pipewire"), &conf)
        .expect("the fake starts")
        .pid();
    let argv = Argv::new(
        &left.join("pipewire").to_string_lossy(),
        &conf.to_string_lossy(),
    );
    assert!(until(|| argv.runs(fake)).await, "the fake did not start");
    // A directory of this run, which is still going.
    let live = root.join(format!("svhe-{}-0", std::process::id()));
    std::fs::create_dir_all(&live).unwrap();

    sweep_finished_runs_in(&root);
    assert!(
        until(|| !argv.runs(fake)).await,
        "the fake outlived its run"
    );
    assert!(left.exists(), "the directory went before what ran from it");
    assert!(
        until(|| running_from(&left).is_empty()).await,
        "something still runs from the directory"
    );
    sweep_finished_runs_in(&root);
    assert!(
        !left.exists(),
        "the directory of a run that is over is still there"
    );
    assert!(live.exists(), "a directory of this run was swept");
}

#[tokio::test]
async fn a_chain_of_another_conf_or_another_program_is_not_ours() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    let stray = spawn_detached(&f.dir.join("pipewire"), &f.dir.join("elsewhere.conf"))
        .unwrap()
        .pid();
    let theirs = Argv::new(
        &f.dir.join("pipewire").to_string_lossy(),
        &f.dir.join("elsewhere.conf").to_string_lossy(),
    );
    // Until the exec has set up its command line, the new process does not run the fake.
    assert!(
        until(|| theirs.runs(stray)).await,
        "the stray did not start"
    );
    assert!(host.chains().is_empty());
    host.remove().await.unwrap();
    assert!(theirs.runs(stray), "it was left running");
    signal(stray, libc::SIGKILL);
}

/// The thread that reaps a detached chain is named, so `top -H` shows what it is.
#[tokio::test]
async fn the_reaper_thread_is_named() {
    let f = Fixture::new();
    let conf = f.dir.join("reap.conf");
    let held = spawn_detached(&f.dir.join("pipewire"), &conf).expect("it starts");
    // The new thread names itself once it runs, which a loaded machine may delay.
    let named = until(|| {
        std::fs::read_dir("/proc/self/task")
            .unwrap()
            .flatten()
            .filter_map(|task| std::fs::read_to_string(task.path().join("comm")).ok())
            .any(|comm| comm.trim() == "svolder-eq-reap")
    })
    .await;
    held.signal(libc::SIGTERM);
    assert!(named, "no thread is named svolder-eq-reap");
}
