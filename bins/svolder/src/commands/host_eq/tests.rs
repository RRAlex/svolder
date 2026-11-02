// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer EQ's managers, against fake `systemctl`, `pipewire` and `wpctl` programs in a
//! scratch directory. The fakes log their arguments there, so a test reads what was asked of
//! them, and nothing here reaches the real session.

use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU32, Ordering};

use svolder_profiles::EqMode;
use svolder_profiles::tone::preset::{self, PresetBand};

use std::process::{Command, Stdio};

use super::*;

/// The programs directory, for the exit handler that removes it.
static MASTERS_AT_EXIT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

extern "C" fn remove_masters() {
    if let Some(dir) = MASTERS_AT_EXIT.get() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The one directory the fake programs are written into, once for the whole test process: a
/// script written while another test thread forks can be busy when it is first run, so every
/// script is written before any fixture runs one, and the fixtures only link to them.
fn masters() -> &'static Path {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        sweep_finished_runs();
        let dir = std::env::temp_dir().join(format!("svhe-programs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the programs directory");
        // The directory belongs to the process, so it goes when the process exits.
        let _ = MASTERS_AT_EXIT.set(dir.clone());
        // SAFETY: `atexit` takes an `extern "C"` function that touches only a static and never
        // unwinds, and it is registered once, inside the `OnceLock` that makes the directory.
        unsafe {
            libc::atexit(remove_masters);
        }
        for (name, body) in [
            ("systemctl", SYSTEMCTL),
            ("pipewire", PIPEWIRE),
            ("wpctl", WPCTL),
            ("wireplumber", WIREPLUMBER),
        ] {
            let path = dir.join(name);
            crate::test_support::install_script(
                &path,
                &format!("#!/bin/sh\nD=$(dirname \"$0\")\n{body}"),
            );
        }
        dir
    })
}

/// Stop the fakes of earlier runs that are over and take their directories away: a run that was
/// killed or aborted never dropped its fixtures, so their fakes still run from them. A directory
/// something was still running from is kept for the next sweep, so a fake that outlives its
/// signal can still be found by it.
fn sweep_finished_runs() {
    for entry in std::fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .flatten()
    {
        let Some(run) = entry.file_name().to_str().and_then(run_of) else {
            continue;
        };
        if Path::new(&format!("/proc/{run}")).exists() {
            continue;
        }
        let fakes = running_from(&entry.path());
        for fake in &fakes {
            signal(*fake, libc::SIGKILL);
        }
        if fakes.is_empty() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The test run a scratch directory of these tests belongs to, from its name: `svhe-<pid>-<n>`
/// for a fixture, `svhe-programs-<pid>` and `svhe-desktop-<pid>` for the run's own.
fn run_of(name: &str) -> Option<u32> {
    let pid = match name.strip_prefix("svhe-")?.split_once('-')? {
        ("programs" | "desktop", pid) => pid,
        (pid, _) => pid,
    };
    pid.parse().ok()
}

/// Send `signal` to `pid`: how the tests end the fakes they find by their scratch directory or
/// by a pid file.
fn signal(pid: u32, signal: libc::c_int) {
    // SAFETY: `kill` takes a pid and a signal number and touches no memory of this process.
    unsafe {
        libc::kill(pid as libc::pid_t, signal);
    }
}

/// The pids of this user's processes, other than this one, with an argument inside `dir`: a
/// fake run from a fixture carries its path there, as the script its shell runs.
fn running_from(dir: &Path) -> Vec<u32> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::geteuid() };
    let own = std::process::id();
    let mut inside = dir.as_os_str().as_bytes().to_vec();
    inside.push(b'/');
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            if pid == own || entry.metadata().ok()?.uid() != uid {
                return None;
            }
            let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
            cmdline
                .split(|byte| *byte == 0)
                .any(|arg| arg.starts_with(&inside))
                .then_some(pid)
        })
        .collect()
}

/// A scratch directory with the fake programs and the files the managers write, removed when
/// the test is done. Its path stays short: a unix socket path may not pass 108 bytes.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "svhe-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["units", "run", "state", "config"] {
            std::fs::create_dir_all(dir.join(sub)).expect("create a scratch directory");
        }
        let fixture = Fixture { dir };
        fixture.script("systemctl", SYSTEMCTL);
        fixture.script("pipewire", PIPEWIRE);
        fixture.script("wpctl", WPCTL);
        fixture.script("wireplumber", WIREPLUMBER);
        std::fs::write(fixture.dir.join("wpver"), "0.5.13").unwrap();
        std::fs::write(fixture.dir.join("default"), "42").unwrap();
        fixture
    }

    /// Link the fake program `name` into the scratch directory. The fake finds the directory
    /// it logs into from the path it was run by.
    fn script(&self, name: &str, body: &str) {
        let master = masters().join(name);
        let _ = body;
        std::os::unix::fs::symlink(master, self.dir.join(name)).unwrap();
    }

    /// The host these tests run on, with `manager` forced.
    fn host(&self, manager: Manager) -> Host {
        Host {
            off: false,
            container: false,
            wpctl: self.dir.join("wpctl").into(),
            wireplumber: self.dir.join("wireplumber").into(),
            pipewire: Some(self.dir.join("pipewire")),
            ctl: Ctl::with_systemctl(self.dir.join("systemctl")),
            forced: Some(manager),
            bad_manager: None,
            conf: self.dir.join("config/svolder/host-eq.conf"),
            state: self.dir.join("state/svolder/host-eq.toml"),
            unit_dir: self.dir.join("units"),
            autostart_dir: self.dir.join("config/autostart"),
            pid_file: Some(self.dir.join("run/svolder-host-eq.pid")),
        }
    }

    /// The lines a fake logged.
    fn log(&self, name: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.join(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// What `systemctl` was asked to do, leaving out the reads.
    fn systemctl_writes(&self) -> Vec<String> {
        self.log("systemctl.log")
            .into_iter()
            .filter(|line| !line.starts_with("--user show"))
            .collect()
    }

    /// Everything under the directory that a manager writes, as relative paths.
    fn written(&self) -> Vec<String> {
        let mut files = Vec::new();
        for sub in ["units", "run", "state", "config"] {
            collect(&self.dir.join(sub), &self.dir, &mut files);
        }
        files.sort();
        files
    }

    fn default_id(&self) -> String {
        std::fs::read_to_string(self.dir.join("default"))
            .unwrap()
            .trim()
            .to_string()
    }
}

impl Drop for Fixture {
    /// Stop every fake run from the directory, whatever program of it a test started, then take
    /// the directory away.
    fn drop(&mut self) {
        for pid in running_from(&self.dir) {
            signal(pid, libc::SIGKILL);
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn collect(dir: &Path, root: &Path, files: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        match path.is_dir() {
            true => collect(&path, root, files),
            false => files.push(path.strip_prefix(root).unwrap().display().to_string()),
        }
    }
}

/// A `systemctl` that knows the one unit: loaded while its file is there, active unless the
/// test says it is stopped, and failing a `restart` when asked.
const SYSTEMCTL: &str = r#"echo "$*" >> "$D/systemctl.log"
case "$2" in
show)
  if [ -f "$D/units/svolder-host-eq.service" ] || [ -f "$D/loaded-anyway" ]; then echo LoadState=loaded; else echo LoadState=not-found; fi
  if [ -f "$D/units/svolder-host-eq.service" ] && [ ! -f "$D/unit-stopped" ]; then echo ActiveState=active; else echo ActiveState=inactive; fi ;;
stop) if [ -f "$D/stop-notloaded" ]; then echo "Unit svolder-host-eq.service not loaded." >&2; exit 1; fi ;;
disable) if [ -f "$D/disable-missing" ]; then echo "Failed to disable unit: Unit file svolder-host-eq.service does not exist." >&2; exit 1; fi
  if [ -f "$D/disable-fails" ]; then echo "Failed to disable unit: access denied" >&2; exit 1; fi ;;
restart) if [ -f "$D/restart-fails" ]; then echo "Failed to restart: unit is masked" >&2; exit 1; fi ;;
esac
exit 0
"#;

/// A `pipewire` that stays up until it is told to stop, or exits at once when asked.
const PIPEWIRE: &str = r#"echo "$$ $*" >> "$D/pipewire.log"
[ -f "$D/pipewire-dies" ] && exit 1
trap 'exit 0' TERM
while :; do sleep 0.1; done
"#;

/// A `wpctl` with four sinks (a DAC, speakers, the chain's own and a fourth) and a default it
/// can change. Like the real one, it has no `--version`.
const WPCTL: &str = r#"echo "$*" >> "$D/wpctl.log"
name_of() {
  case "$1" in
    42) echo dac;; 43) echo alsa_output.speakers;; 44) echo svolder_eq;; 45) echo hdmi;; *) return 1;;
  esac
}
case "$1" in
--version) echo "Error: Unknown option --version"; exit 1;;
status)
  printf 'PipeWire 1.6.2\n\nAudio\n ├─ Devices:\n │      40. Some card\n │\n ├─ Sinks:\n │  *   42. DAC [vol: 1.00]\n │      43. Speakers [vol: 0.50]\n │      44. svolder EQ [vol: 1.00]\n │      45. HDMI [vol: 0.50]\n │\n ├─ Sources:\n │      50. Mic [vol: 1.00]\n\nVideo\n';;
inspect)
  id="$2"
  [ "$id" = "@DEFAULT_AUDIO_SINK@" ] && id="$(cat "$D/default")"
  name="$(name_of "$id")" || exit 1
  printf 'id %s, type PipeWire:Interface:Node\n  * media.class = "Audio/Sink"\n  * node.name = "%s"\n' "$id" "$name";;
set-default) echo "$2" > "$D/default";;
esac
exit 0
"#;

/// A `wireplumber` that prints the version banner, at the version the test sets.
const WIREPLUMBER: &str = r#"echo "$*" >> "$D/wireplumber.log"
[ -f "$D/wireplumber-gone" ] && exit 127
printf 'wireplumber\nCompiled with libwireplumber %s\nLinked with libwireplumber %s\n' "$(cat "$D/wpver")" "$(cat "$D/wpver")"
"#;

fn request(name: &str) -> ApplyRequest {
    ApplyRequest {
        name: name.to_string(),
        origin: Some("AutoEq · test".to_string()),
        correction: Correction {
            preamp_db: -4.5,
            filters: vec![SourceFilter {
                kind: FilterKind::Peaking,
                f: 1000.0,
                g: 3.0,
                q: 1.0,
            }],
        },
        sink: None,
    }
}

/// Wait up to two seconds for `done` to hold.
async fn until(done: impl Fn() -> bool) -> bool {
    for _ in 0..40 {
        if done() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    done()
}

fn pid_of(fixture: &Fixture) -> (u32, Argv) {
    let text = std::fs::read_to_string(fixture.dir.join("run/svolder-host-eq.pid"))
        .expect("the pid file is there");
    parse_pid_file(&text).expect("the pid file parses")
}

// ---- systemd ----

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
    assert!(conf.contains("gain = -4.5 q = 1.0"), "{conf}");
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

// ---- process ----

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

// ---- the default route ----

#[tokio::test]
async fn the_default_route_sets_the_default_and_remove_restores_it() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("wpver"), "0.4.17").unwrap();
    let host = f.host(Manager::Systemd);
    let state = host.apply(request("HD 650")).await.unwrap();
    assert_eq!(state.route, Route::Default);
    assert_eq!(state.target, "dac");
    assert_eq!(state.previous_default.as_deref(), Some("dac"));
    assert_eq!(f.default_id(), "44", "the chain is the default output");
    let conf = std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap();
    assert!(conf.contains("target.object = \"dac\""), "{conf}");
    assert!(!conf.contains("filter.smart"), "{conf}");

    // Applied again while it is the default: the sink and the previous default are kept.
    let again = host.apply(request("Another")).await.unwrap();
    assert_eq!(again.target, "dac");
    assert_eq!(again.previous_default.as_deref(), Some("dac"));

    host.remove().await.unwrap();
    assert_eq!(f.default_id(), "42", "the default output is back");
}

#[tokio::test]
async fn a_previous_default_that_is_gone_is_not_restored() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("wpver"), "0.4.17").unwrap();
    let host = f.host(Manager::Systemd);
    let mut state = host.apply(request("HD 650")).await.unwrap();
    state.previous_default = Some("unplugged".to_string());
    state.save(&host.state).unwrap();
    host.remove().await.unwrap();
    assert_eq!(f.default_id(), "44");
}

// ---- the sink ----

#[tokio::test]
async fn the_sink_is_named_by_node_name_or_by_wpctl_id() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    let mut by_name = request("A");
    by_name.sink = Some("alsa_output.speakers".to_string());
    assert_eq!(
        host.apply(by_name).await.unwrap().target,
        "alsa_output.speakers"
    );
    let mut by_id = request("A");
    by_id.sink = Some("45".to_string());
    assert_eq!(host.apply(by_id).await.unwrap().target, "hdmi");
    let conf = std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap();
    assert!(conf.contains("node.name = \"hdmi\" }"), "{conf}");
}

#[tokio::test]
async fn a_sink_that_is_no_sink_or_is_the_chain_itself_is_refused() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    for (sink, said) in [
        ("99", "no sink with id 99"),
        ("svolder_eq", "the computer EQ itself"),
        ("  ", "not a sink name"),
    ] {
        let mut asked = request("A");
        asked.sink = Some(sink.to_string());
        let failure = host.apply(asked).await.unwrap_err();
        assert!(failure.what.contains(said), "{sink}: {}", failure.what);
    }
    assert_eq!(f.written(), Vec::<String>::new());
}

#[tokio::test]
async fn without_a_default_output_a_sink_must_be_named() {
    let f = Fixture::new();
    std::fs::write(f.dir.join("default"), "").unwrap();
    let failure = f
        .host(Manager::Systemd)
        .apply(request("A"))
        .await
        .unwrap_err();
    assert!(
        failure.what.contains("no default output"),
        "{}",
        failure.what
    );
    assert!(failure.next.contains("--sink"), "{}", failure.next);
}

// ---- refusals ----

#[tokio::test]
async fn a_container_the_host_being_off_and_a_missing_pipewire_refuse_before_anything_is_written() {
    let f = Fixture::new();
    let mut container = f.host(Manager::Systemd);
    container.container = true;
    let failure = container.apply(request("A")).await.unwrap_err();
    assert_eq!(failure.what, "host EQ is not available in a container");
    assert_eq!(
        failure.next,
        "run svolder host eq on the machine that plays the sound"
    );

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

// ---- a stale state (Review Focus 4) ----

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

fn pid_of_or_none(fixture: &Fixture) -> Option<(u32, Argv)> {
    parse_pid_file(&std::fs::read_to_string(fixture.dir.join("run/svolder-host-eq.pid")).ok()?)
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

// ---- status ----

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

// ---- what a preset applies ----

fn band_preset(bands: &[(f64, f64, f64)]) -> Preset {
    Preset {
        name: "Mine".into(),
        bands: bands.len() as u8,
        mode: EqMode::Eq,
        loudness: 0,
        band: bands
            .iter()
            .map(|&(f, g, q)| PresetBand { f, g, q })
            .collect(),
        source: None,
    }
}

#[test]
fn a_band_preset_becomes_peaking_filters_with_a_preamp_of_minus_its_peak_rounded_up() {
    let preset = band_preset(&[(100.0, 3.0, 1.0), (4000.0, -2.0, 2.0)]);
    let correction = host_correction(&preset).unwrap();
    assert_eq!(correction.filters.len(), 2);
    assert!(
        correction
            .filters
            .iter()
            .all(|f| f.kind == FilterKind::Peaking)
    );
    assert_eq!(
        correction.filters[1],
        SourceFilter {
            kind: FilterKind::Peaking,
            f: 4000.0,
            g: -2.0,
            q: 2.0
        }
    );
    let peak = response::peak_db(&[
        BandParams {
            f0: 100.0,
            gain_db: 3.0,
            q: 1.0,
        },
        BandParams {
            f0: 4000.0,
            gain_db: -2.0,
            q: 2.0,
        },
    ]);
    assert!(peak > 2.9 && peak < 3.1, "{peak}");
    // One decimal, never below the peak.
    let tenths = correction.preamp_db * 10.0;
    assert!(
        (tenths - tenths.round()).abs() < 1e-9,
        "{}",
        correction.preamp_db
    );
    assert!(-correction.preamp_db >= peak - 1e-9);
    assert!(-correction.preamp_db < peak + 0.1 + 1e-9);
}

#[test]
fn a_cut_only_or_flat_band_preset_needs_no_preamp() {
    assert_eq!(
        host_correction(&band_preset(&[(100.0, -3.0, 1.0)]))
            .unwrap()
            .preamp_db,
        0.0
    );
    assert_eq!(
        host_correction(&band_preset(&[(100.0, 0.0, 1.0)]))
            .unwrap()
            .preamp_db,
        0.0
    );
    assert_eq!(host_correction(&band_preset(&[])).unwrap().preamp_db, 0.0);
}

#[test]
fn an_imported_preset_applies_exactly_what_it_holds() {
    let correction = request("x").correction;
    let preset = Preset {
        name: "HD 650".into(),
        bands: 0,
        mode: EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
        source: Some(preset::Imported {
            origin: Some("AutoEq".into()),
            correction: correction.clone(),
        }),
    };
    assert_eq!(host_correction(&preset).unwrap(), correction);
}

// ---- a chain nothing recorded (the autostart entry started it) ----

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
    assert!(argv.runs(stray));
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
    assert!(argv.runs(stray), "the stray did not start");
    drop(f);
    assert!(
        until(|| !argv.runs(stray)).await,
        "the stray outlived its fixture"
    );
}

/// The fixtures of a test run that ended without dropping them are swept by the next run: their
/// fakes stopped, and their directories taken away once nothing runs from them. A directory of a
/// run still going is left alone.
#[tokio::test]
async fn the_fixtures_of_a_run_that_is_over_are_swept() {
    // No process ever has the largest pid, so this run is always over.
    let left = std::env::temp_dir().join(format!("svhe-{}-0", u32::MAX));
    let _ = std::fs::remove_dir_all(&left);
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
    let live = Fixture::new();

    sweep_finished_runs();
    assert!(
        until(|| !argv.runs(fake)).await,
        "the fake outlived its run"
    );
    assert!(left.exists(), "the directory went before what ran from it");
    sweep_finished_runs();
    assert!(
        !left.exists(),
        "the directory of a run that is over is still there"
    );
    assert!(live.dir.exists(), "a fixture of this run was swept");
}

#[tokio::test]
async fn a_chain_of_another_conf_or_another_program_is_not_ours() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    let stray = spawn_detached(&f.dir.join("pipewire"), &f.dir.join("elsewhere.conf"))
        .unwrap()
        .pid();
    assert!(host.chains().is_empty());
    host.remove().await.unwrap();
    let theirs = Argv::new(
        &f.dir.join("pipewire").to_string_lossy(),
        &f.dir.join("elsewhere.conf").to_string_lossy(),
    );
    assert!(theirs.runs(stray), "it was left running");
    signal(stray, libc::SIGKILL);
}

// ---- a correction that is out of range ----

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

// ---- paths and the manager word ----

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
    assert!(failure.next.contains("systemd") && failure.next.contains("process"));
    assert_eq!(f.written(), Vec::<String>::new());
}

// ---- remove with a unit systemd has loaded but whose file is gone ----

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

// ---- status of a state that cannot be read ----

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

/// The state file's modification time is what the TUI polls: none without a file, and one that
/// moves on once an apply or a remove has written it.
#[tokio::test]
async fn the_state_stamp_follows_the_state_file() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    assert_eq!(stamp_of(&host.state), None);
    host.apply(request("HD 650")).await.unwrap();
    let applied = stamp_of(&host.state).expect("an applied state has a time");
    std::thread::sleep(std::time::Duration::from_millis(20));
    host.remove().await.unwrap();
    let removed = stamp_of(&host.state).expect("a removed state stays");
    assert!(removed > applied, "{removed:?} after {applied:?}");
}

// ---- the computer EQ never goes above unity ----

/// The preamp `host.apply` ends with for `correction`.
async fn applied_preamp(f: &Fixture, correction: Correction) -> f64 {
    let mut asked = request("Bound");
    asked.correction = correction;
    let state = f.host(Manager::Process).apply(asked).await.unwrap();
    // The conf written carries the preamp that was saved.
    assert!(state.preamp_db <= 0.0, "{}", state.preamp_db);
    state.preamp_db
}

/// The peak of `correction` over the audio band, in dB.
fn peak_of(correction: &Correction) -> f64 {
    response::correction_db(correction, &response::log_freqs(1024))
        .into_iter()
        .fold(f64::NEG_INFINITY, f64::max)
}

fn parsed(text: &str) -> Correction {
    apo::parse(text).unwrap().correction
}

#[tokio::test]
async fn a_file_that_states_a_boost_is_applied_at_minus_its_peak() {
    let f = Fixture::new();
    let boosted = parsed("Preamp: +6 dB\nFilter 1: ON PK Fc 1000 Hz Gain 4 dB Q 1\n");
    let applied = applied_preamp(&f, boosted.clone()).await;
    let peak = peak_of(&boosted);
    assert!(
        applied <= -peak + 1e-9 && applied > -peak - 0.1 - 1e-9,
        "{applied} {peak}"
    );
    // The text the chain is given carries it too.
    let conf = std::fs::read_to_string(f.dir.join("config/svolder/host-eq.conf")).unwrap();
    assert!(conf.contains(&format!("{applied}")), "{conf}");
    assert!(!conf.contains("6.0"), "{conf}");
}

#[tokio::test]
async fn a_file_with_no_preamp_and_boosting_shelves_gets_one() {
    let f = Fixture::new();
    let shelves = parsed(
        "Filter 1: ON LSC Fc 100 Hz Gain 12 dB Q 0.7\nFilter 2: ON HSC Fc 8000 Hz Gain 12 dB Q 0.7\n",
    );
    let applied = applied_preamp(&f, shelves.clone()).await;
    let peak = peak_of(&shelves);
    assert!(peak > 11.0, "{peak}");
    assert!(applied <= -peak + 1e-9, "{applied} {peak}");
}

#[tokio::test]
async fn an_all_cut_file_with_a_boosting_preamp_is_applied_at_unity() {
    let f = Fixture::new();
    let cut = parsed("Preamp: +6 dB\nFilter 1: ON PK Fc 1000 Hz Gain -4 dB Q 1\n");
    assert_eq!(applied_preamp(&f, cut).await, 0.0);
}

#[tokio::test]
async fn an_autoeq_like_file_keeps_its_preamp() {
    let f = Fixture::new();
    let normal = parsed(
        "Preamp: -6.2 dB\nFilter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.7\n\
         Filter 2: ON PK Fc 2500 Hz Gain -3 dB Q 2\nFilter 3: ON PK Fc 6000 Hz Gain 2 dB Q 3\n",
    );
    let peak = peak_of(&normal);
    assert!(
        peak < 6.2,
        "the fixture's own preamp covers its peak: {peak}"
    );
    let applied = applied_preamp(&f, normal).await;
    assert!((applied + 6.2).abs() < 0.1, "{applied}");
}

#[tokio::test]
async fn a_preamp_beyond_thirty_decibels_is_refused() {
    assert!(apo::parse("Preamp: +31 dB\nFilter 1: ON PK Fc 1000 Hz Gain 1 dB Q 1\n").is_err());
    assert!(apo::parse("Preamp: 1000 dB\nFilter 1: ON PK Fc 1000 Hz Gain 1 dB Q 1\n").is_err());
    let f = Fixture::new();
    let mut asked = request("Bad");
    asked.correction.preamp_db = 31.0;
    assert!(f.host(Manager::Process).apply(asked).await.is_err());
    assert_eq!(f.written(), Vec::<String>::new());
}

#[tokio::test]
async fn an_imported_preset_and_a_band_preset_are_bounded_the_same_way() {
    let f = Fixture::new();
    let imported = Preset {
        name: "Phones".into(),
        bands: 0,
        mode: EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
        source: Some(preset::Imported {
            origin: None,
            correction: Correction {
                preamp_db: 3.0,
                filters: vec![SourceFilter {
                    kind: FilterKind::Peaking,
                    f: 1000.0,
                    g: 5.0,
                    q: 1.0,
                }],
            },
        }),
    };
    let correction = host_correction(&imported).unwrap();
    let applied = applied_preamp(&f, correction.clone()).await;
    assert!(applied <= -peak_of(&correction) + 1e-9, "{applied}");
    let band = host_correction(&band_preset(&[(100.0, 6.0, 1.0)])).unwrap();
    let applied = applied_preamp(&f, band.clone()).await;
    assert!(applied <= -peak_of(&band) + 1e-9, "{applied}");
    assert!(
        (applied - band.preamp_db).abs() < 0.11,
        "{applied} {}",
        band.preamp_db
    );
}

#[tokio::test]
async fn a_re_apply_from_the_state_file_is_bounded_too() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    // A state file written by hand, or by another version, that states a boost.
    let mut state = host.apply(request("Old")).await.unwrap();
    host.remove().await.unwrap();
    state.on = false;
    state.preamp_db = 12.0;
    state.save(&host.state).unwrap();
    let kept = host.state().unwrap();
    let again = host
        .apply(ApplyRequest {
            name: kept.name.clone(),
            origin: kept.origin.clone(),
            correction: kept.correction(),
            sink: None,
        })
        .await
        .unwrap();
    assert!(again.preamp_db <= -3.0 + 1e-9, "{}", again.preamp_db);
    assert_eq!(host.state().unwrap().preamp_db, again.preamp_db);
}

#[test]
fn the_note_names_both_preamps_only_when_one_was_lowered() {
    assert_eq!(
        lowered_note(-2.0, -6.4).as_deref(),
        Some("preamp lowered from -2.0 dB to -6.4 dB so the correction never clips")
    );
    assert_eq!(lowered_note(-6.4, -6.4), None);
    assert_eq!(lowered_note(-6.4, -6.4 + 0.0), None);
}

// ---- review fixes ----

#[test]
fn a_band_preset_the_dac_keeps_flat_or_in_loudness_is_refused_for_the_computer() {
    for mode in [EqMode::Flat, EqMode::Loudness] {
        let mut preset = band_preset(&[(100.0, 3.0, 1.0)]);
        preset.mode = mode;
        let why = host_correction(&preset).unwrap_err();
        assert!(why.contains("Mine") && why.contains("no curve"), "{why}");
    }
}

#[tokio::test]
async fn a_re_apply_that_changes_manager_stops_the_other_ones_leftovers_with_no_state_file() {
    let f = Fixture::new();
    // A unit file and an autostart entry no state file names.
    std::fs::write(f.dir.join("units/svolder-host-eq.service"), "[Unit]\n").unwrap();
    std::fs::create_dir_all(f.dir.join("config/autostart")).unwrap();
    let desktop = f.dir.join("config/autostart/svolder-host-eq.desktop");
    std::fs::write(&desktop, "[Desktop Entry]\n").unwrap();
    assert!(!f.host(Manager::Process).state.exists());
    f.host(Manager::Process)
        .apply(request("Now"))
        .await
        .unwrap();
    assert!(!f.dir.join("units/svolder-host-eq.service").exists());
    assert!(
        f.systemctl_writes()
            .iter()
            .any(|line| line.contains("stop")),
        "{:?}",
        f.systemctl_writes()
    );
    f.host(Manager::Process).remove().await.unwrap();
    // And the other way: an autostart entry is removed when systemd takes over.
    std::fs::create_dir_all(f.dir.join("config/autostart")).unwrap();
    std::fs::write(&desktop, "[Desktop Entry]\n").unwrap();
    std::fs::remove_file(&f.host(Manager::Systemd).state).unwrap();
    f.host(Manager::Systemd)
        .apply(request("Next"))
        .await
        .unwrap();
    assert!(!desktop.exists());
}

#[test]
fn a_pipewire_path_with_escape_characters_is_found_again_through_the_entry() {
    let dir = std::env::temp_dir().join(format!("svhe-desktop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = crate::test_support::TempDir(dir);
    let entry = dir.join("e.desktop");
    for program in [
        "/opt/a b/pipewire",
        "/opt/back\\slash/pipewire",
        "/opt/q\"uote/pipewire",
        "/opt/dol$lar/pipewire",
        "/opt/tick`/pipewire",
        "/opt/100%/pipewire",
        "/opt/plain/pipewire",
    ] {
        std::fs::write(
            &entry,
            render_desktop(Path::new(program), Path::new("/etc/c.conf")),
        )
        .unwrap();
        assert_eq!(
            desktop_program(&entry).as_deref(),
            Some(program),
            "{program}"
        );
    }
}

// ---- residual round ----

#[test]
fn a_narrow_boost_between_two_grid_points_is_measured_at_its_true_height() {
    let grid = response::log_freqs(1024);
    let k = grid.iter().position(|f| *f > 13_500.0).unwrap();
    let centre = (grid[k] * grid[k + 1]).sqrt();
    let narrow = Correction {
        preamp_db: 0.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: centre,
            g: 12.0,
            q: 20.0,
        }],
    };
    let truth = response::correction_db(&narrow, &[centre])[0];
    let sampled = response::correction_db(&narrow, &grid)
        .into_iter()
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        truth - sampled > 0.5,
        "the grid misses it: {truth} {sampled}"
    );
    let preamp = safe_preamp(&narrow);
    assert!(preamp <= -truth + 1e-9, "{preamp} {truth}");
    assert!(preamp > -truth - 0.1 - 1e-9, "{preamp} {truth}");
}

#[tokio::test]
async fn a_curve_peaking_above_thirty_decibels_applies_and_applies_again() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    let mut asked = request("Loud");
    asked.correction.preamp_db = 0.0;
    asked.correction.filters = vec![
        SourceFilter {
            kind: FilterKind::Peaking,
            f: 1000.0,
            g: 20.0,
            q: 1.0,
        },
        SourceFilter {
            kind: FilterKind::Peaking,
            f: 1000.0,
            g: 20.0,
            q: 1.0,
        },
    ];
    let first = host.apply(asked).await.unwrap();
    assert!(first.preamp_db < -30.0, "{}", first.preamp_db);
    let again = host
        .apply(ApplyRequest {
            name: first.name.clone(),
            origin: None,
            correction: first.correction(),
            sink: None,
        })
        .await
        .unwrap();
    assert_eq!(again.preamp_db, first.preamp_db);
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
