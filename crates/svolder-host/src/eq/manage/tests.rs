// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer EQ's managers, against fake `systemctl`, `pipewire` and `wpctl` programs in a
//! scratch directory. The fakes log their arguments there, so a test reads what was asked of
//! them, and nothing here reaches the real session.

use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU32, Ordering};

use std::process::{Command, Stdio};

use svolder_profiles::tone::apo::{FilterKind, SourceFilter};

use super::*;

#[path = "../../../tests/common/script.rs"]
mod script;

/// A scratch directory removed when the test that made it ends.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

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
            script::install_script(&path, &format!("#!/bin/sh\nD=$(dirname \"$0\")\n{body}"));
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
            ctl: UserSystemd::with_program(self.dir.join("systemctl")),
            forced: Some(manager),
            bad_manager: None,
            conf: self.dir.join("config/svolder/host-eq.conf"),
            state: self.dir.join("state/svolder/host-eq.toml"),
            unit_dir: self.dir.join("units"),
            autostart_dir: self.dir.join("config/autostart"),
            pid_file: Some(self.dir.join("run/svolder-host-eq.pid")),
            child: Mutex::new(None),
            gate: tokio::sync::Mutex::new(()),
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
        slots: Vec::new(),
        rate_hz: None,
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
    assert!(
        ["systemd", "process", "child"]
            .iter()
            .all(|word| failure.next.contains(word)),
        "{}",
        failure.next
    );
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
    let mult = 10f64.powf(applied / 20.0);
    assert!(conf.contains(&format!("Mult = {mult} ")), "{conf}");
    assert!(!conf.contains("Mult = 1.99"), "{conf}");
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
    let imported = Correction {
        preamp_db: 3.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: 1000.0,
            g: 5.0,
            q: 1.0,
        }],
    };
    let applied = applied_preamp(&f, imported.clone()).await;
    assert!(applied <= -peak_of(&imported) + 1e-9, "{applied}");
    // A band preset's correction: its bands as peaking filters, with minus the peak, rounded
    // up to 0.1 dB, as its preamp.
    let band = Correction {
        preamp_db: -6.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: 100.0,
            g: 6.0,
            q: 1.0,
        }],
    };
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
            slots: Vec::new(),
            rate_hz: None,
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
    let dir = TempDir(dir);
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
            slots: Vec::new(),
            rate_hz: None,
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

// ---- the child manager ----

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

/// The coefficients `[b0, b1, b2, a1, a2]` of one held filter from the audio-EQ cookbook,
/// written out here and divided by `a0`, then rounded to f32 as PipeWire runs them.
fn played_coefficients(filter: &SourceFilter, rate_hz: u32) -> [f64; 5] {
    let filter = live::held(filter, rate_hz);
    let rate = f64::from(rate_hz);
    let a = 10f64.powf(filter.g / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * filter.f / rate;
    let (c, s) = (w0.cos(), w0.sin());
    let alpha = s / (2.0 * filter.q);
    let beta = 2.0 * a.sqrt() * alpha;
    let (b, d) = match filter.kind {
        FilterKind::Peaking => (
            [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
        ),
        FilterKind::LowShelf => (
            [
                a * ((a + 1.0) - (a - 1.0) * c + beta),
                2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                a * ((a + 1.0) - (a - 1.0) * c - beta),
            ],
            [
                (a + 1.0) + (a - 1.0) * c + beta,
                -2.0 * ((a - 1.0) + (a + 1.0) * c),
                (a + 1.0) + (a - 1.0) * c - beta,
            ],
        ),
        FilterKind::HighShelf => (
            [
                a * ((a + 1.0) + (a - 1.0) * c + beta),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                a * ((a + 1.0) + (a - 1.0) * c - beta),
            ],
            [
                (a + 1.0) - (a - 1.0) * c + beta,
                2.0 * ((a - 1.0) - (a + 1.0) * c),
                (a + 1.0) - (a - 1.0) * c - beta,
            ],
        ),
    };
    [
        b[0] / d[0],
        b[1] / d[0],
        b[2] / d[0],
        d[1] / d[0],
        d[2] / d[0],
    ]
    .map(|value| f64::from(value as f32))
}

/// Whether the poles of `1 + a1 z^-1 + a2 z^-2` are inside the unit circle.
fn is_stable(c: &[f64; 5]) -> bool {
    c[4].abs() < 1.0 && c[3].abs() < 1.0 + c[4]
}

/// The sum in dB of the filters' responses at `w`, by `|p0 + p1 z + p2 z^2|^2` written in
/// `sin^2(w/2)` below half of Nyquist's w and in `cos^2(w/2)` above.
fn summed_db(sets: &[[f64; 5]], w: f64) -> f64 {
    let (sign, x) = match w <= std::f64::consts::FRAC_PI_2 {
        true => (1.0, (w / 2.0).sin().powi(2)),
        false => (-1.0, (w / 2.0).cos().powi(2)),
    };
    let power = |p: [f64; 3]| {
        let sum = p[0] + p[1] + p[2];
        sum * sum - 4.0 * (p[0] * p[1] + 4.0 * p[0] * p[2] + p[1] * p[2]) * x
            + 16.0 * p[0] * p[2] * x * x
    };
    sets.iter()
        .map(|c| {
            10.0 * (power([c[0], sign * c[1], c[2]]) / power([1.0, sign * c[3], c[4]])).log10()
        })
        .sum()
}

/// The highest the played (f32) filters rise anywhere from 0.01 Hz to Nyquist, DC and Nyquist
/// included, and where: a log grid in frequency, a second log grid in the distance from
/// Nyquist and each filter's centre, with every local maximum zoomed in on by repeated scans.
fn dense_peak(filters: &[SourceFilter], rate_hz: u32) -> (f64, f64) {
    use std::f64::consts::PI;
    let rate = f64::from(rate_hz);
    let sets: Vec<[f64; 5]> = filters
        .iter()
        .map(|f| played_coefficients(f, rate_hz))
        .collect();
    let total = |w: f64| summed_db(&sets, w);
    let mut ws = vec![0.0, PI];
    let (low, high) = (0.01f64, rate / 2.0 * 0.99999);
    let steps = 8192;
    ws.extend(
        (0..=steps)
            .map(|i| 2.0 * PI * low * (high / low).powf(f64::from(i) / f64::from(steps)) / rate),
    );
    let nyquist_steps = 4096;
    let (near, far) = (PI * 1e-8, PI / 2.0);
    ws.extend(
        (0..=nyquist_steps)
            .map(|i| PI - near * (far / near).powf(f64::from(i) / f64::from(nyquist_steps))),
    );
    ws.extend(
        filters
            .iter()
            .map(|f| (2.0 * PI * live::held(f, rate_hz).f / rate).min(PI)),
    );
    ws.sort_by(f64::total_cmp);
    ws.dedup();
    let values: Vec<f64> = ws.iter().map(|&w| total(w)).collect();
    let last = ws.len() - 1;
    let mut best = (f64::NEG_INFINITY, 0.0);
    for i in 0..=last {
        if values[i] > best.0 {
            best = (values[i], ws[i]);
        }
        let top = i == 0 || values[i] > values[i - 1];
        if !(top && (i == last || values[i] >= values[i + 1])) {
            continue;
        }
        let (mut from, mut to) = (ws[i.saturating_sub(1)], ws[(i + 1).min(last)]);
        for _ in 0..4 {
            let scan = 64;
            let mut here = (f64::NEG_INFINITY, from);
            for k in 0..=scan {
                let w = from + (to - from) * f64::from(k) / f64::from(scan);
                let value = total(w);
                if value > here.0 {
                    here = (value, w);
                }
            }
            if here.0 > best.0 {
                best = here;
            }
            let width = (to - from) / f64::from(scan);
            (from, to) = ((here.1 - width).max(0.0), (here.1 + width).min(PI));
        }
    }
    (best.0, best.1 * rate / (2.0 * PI))
}

fn filter(kind: FilterKind, f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter { kind, f, g, q }
}

/// A cookbook low shelf at 20 Hz, +12 dB, Q 10 resonates well above its stated gain below its
/// corner, at about 14 Hz.
#[test]
fn the_dense_evaluator_sees_the_low_shelf_resonance_at_14_hz() {
    let (peak, at) = dense_peak(&[filter(FilterKind::LowShelf, 20.0, 12.0, 10.0)], 48_000);
    assert!((29.0..30.5).contains(&peak), "{peak} dB at {at} Hz");
    assert!((12.0..16.0).contains(&at), "{at} Hz");
}

/// The evaluator reads a peak five hertz wide next to Nyquist at its true height.
#[test]
fn the_dense_evaluator_sees_a_narrow_peak_beside_nyquist() {
    let (peak, at) = dense_peak(
        &[
            filter(FilterKind::Peaking, 23_890.0, 27.0, 20.0),
            filter(FilterKind::HighShelf, 21_810.0, 30.0, 0.06),
        ],
        48_000,
    );
    assert!((50.4..50.8).contains(&peak), "{peak} dB at {at} Hz");
}

/// What a stack must satisfy: refused, or its preamp plus the peak of the played (f32)
/// coefficients is at most +0.01 dB. A stack with an unstable filter must be refused, and
/// none may be accepted below the preamp floor. Returns the line to report when it does not.
fn over_unity(name: &str, rate: u32, filters: &[SourceFilter]) -> Option<String> {
    let correction = Correction {
        preamp_db: 0.0,
        filters: filters.to_vec(),
    };
    let unstable = filters
        .iter()
        .any(|f| !is_stable(&played_coefficients(f, rate)));
    match check_playable(&correction, rate) {
        Err(_) => None,
        Ok(()) if unstable => Some(format!("{rate} {name}: an unstable filter was accepted")),
        Ok(()) => {
            let preamp = played_preamp(&correction, rate);
            let (peak, at) = dense_peak(filters, rate);
            (preamp + peak > 0.01).then(|| {
                format!("{rate} {name}: preamp {preamp} dB, played peak {peak} dB at {at} Hz")
            })
        }
    }
}

/// The preamp plus the curve as played over the whole band the chain plays, DC to Nyquist, is
/// never above unity: for the corners a hand can set and the extremes a preset can hold, alone
/// and sixteen deep, at the rates the chain runs at.
#[test]
fn the_preamp_keeps_the_whole_played_band_under_unity() {
    let kinds = [
        FilterKind::LowShelf,
        FilterKind::HighShelf,
        FilterKind::Peaking,
    ];
    let mut cases: Vec<(String, SourceFilter)> = vec![];
    for kind in kinds {
        for f in [20.0, 20_000.0] {
            for q in [0.1, 0.707, 10.0] {
                cases.push((
                    format!("{kind:?} {f} Hz +12 Q{q}"),
                    filter(kind, f, 12.0, q),
                ));
            }
        }
    }
    for (kind, f, g, q) in [
        (FilterKind::Peaking, 10.0, 30.0, 20.0),
        (FilterKind::HighShelf, 24_000.0, 30.0, 0.707),
        (FilterKind::HighShelf, 24_000.0, 30.0, 20.0),
        (FilterKind::LowShelf, 10.0, 30.0, 0.707),
        (FilterKind::LowShelf, 10.0, 30.0, 20.0),
        (FilterKind::Peaking, 20.0, 12.0, 10.0),
    ] {
        cases.push((
            format!("{kind:?} {f} Hz {g:+} Q{q} (preset)"),
            filter(kind, f, g, q),
        ));
    }
    let mut over: Vec<String> = vec![];
    for rate in [44_100u32, 48_000, 88_200, 96_000, 192_000] {
        for (name, one) in &cases {
            for count in [1usize, 16] {
                let filters = vec![one.clone(); count];
                over.extend(over_unity(&format!("x{count} {name}"), rate, &filters));
            }
        }
    }
    assert!(
        over.is_empty(),
        "{} cases over unity:\n{}",
        over.len(),
        over.join("\n")
    );
}

/// Two narrow filters next to Nyquist that a sparse grid steps over.
#[test]
fn a_narrow_peak_beside_nyquist_is_covered_by_the_preamp() {
    let at_48k = [
        filter(FilterKind::Peaking, 23_890.0, 27.0, 20.0),
        filter(FilterKind::HighShelf, 21_810.0, 12.0, 0.06),
    ];
    let at_44k = [
        filter(FilterKind::LowShelf, 23_746.0, 5.0, 0.82),
        filter(FilterKind::Peaking, 23_928.0, 22.0, 20.0),
    ];
    for (name, rate, filters) in [
        ("48 kHz pair", 48_000, at_48k),
        ("44.1 kHz pair", 44_100, at_44k),
    ] {
        let correction = Correction {
            preamp_db: 0.0,
            filters: filters.to_vec(),
        };
        assert_eq!(
            check_playable(&correction, rate).map_err(|e| e.what),
            Ok(()),
            "{name}"
        );
        assert_eq!(over_unity(name, rate, &filters), None);
        let (peak, _) = dense_peak(&filters, rate);
        let preamp = played_preamp(&correction, rate);
        assert!(
            peak > 20.0 && preamp <= -peak,
            "{name}: {preamp} dB for a {peak} dB peak"
        );
    }
}

/// PipeWire clamps a biquad coefficient to plus or minus 10 (read back from a private
/// instance: 10.5 reads 10, -31.6 reads -10), so a filter that needs more is refused.
#[test]
fn a_filter_with_a_coefficient_past_ten_is_refused() {
    let one = |kind, f, g, q| Correction {
        preamp_db: 0.0,
        filters: vec![filter(kind, f, g, q)],
    };
    let failure =
        check_playable(&one(FilterKind::HighShelf, 1000.0, 30.0, 0.707), 48_000).unwrap_err();
    assert!(
        failure.what.contains("point 1 boosts more than"),
        "{}",
        failure.what
    );
    // What a hand can set stays inside.
    for kind in [
        FilterKind::LowShelf,
        FilterKind::HighShelf,
        FilterKind::Peaking,
    ] {
        for rate in [44_100, 48_000] {
            for f in [20.0, 1000.0, 20_000.0] {
                let correction = one(kind, f, 12.0, 0.707);
                assert_eq!(
                    check_playable(&correction, rate).map_err(|e| e.what),
                    Ok(()),
                    "{kind:?} {f} Hz at {rate}"
                );
            }
        }
    }
}

/// A seeded xorshift stream of numbers in `[0, 1)`.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn log(&mut self, from: f64, to: f64) -> f64 {
        (from.ln() + (to.ln() - from.ln()) * self.next()).exp()
    }

    fn filter(&mut self, rate: f64) -> SourceFilter {
        let kind = match (self.next() * 3.0) as u8 {
            0 => FilterKind::Peaking,
            1 => FilterKind::LowShelf,
            _ => FilterKind::HighShelf,
        };
        let f = match self.next() {
            zone if zone < 0.3 => self.log(10.0, 40.0),
            zone if zone < 0.65 => self.log((0.42 * rate).min(21_000.0), 24_000.0),
            _ => self.log(10.0, 24_000.0),
        };
        let g = match self.next() {
            edge if edge < 0.3 => 30.0 * if self.next() < 0.5 { 1.0 } else { -1.0 },
            _ => (self.next() * 2.0 - 1.0) * 30.0,
        };
        let q = match self.next() {
            edge if edge < 0.3 => 20.0,
            _ => self.log(0.02, 20.0),
        };
        filter(kind, f, g, q)
    }
}

/// Random mixed stacks, as many filters as a preset or a file can bring, at every rate: each is
/// refused or is kept under unity as played.
#[test]
fn random_mixed_stacks_stay_under_unity_as_played() {
    let mut stream = Stream(0x9e37_79b9_7f4a_7c15);
    let rates = [44_100u32, 48_000, 88_200, 96_000, 192_000];
    let mut over: Vec<String> = vec![];
    let mut accepted = 0;
    for case in 0..2500 {
        let rate = rates[case % rates.len()];
        let count = 1 + (stream.next() * 16.0) as usize;
        // Filters the chain can play one by one, so a stack is not refused for one of them.
        let mut filters: Vec<SourceFilter> = vec![];
        while filters.len() < count {
            let one = stream.filter(f64::from(rate));
            let alone = Correction {
                preamp_db: 0.0,
                filters: vec![one.clone()],
            };
            if check_playable(&alone, rate).is_ok() {
                filters.push(one);
            }
        }
        let correction = Correction {
            preamp_db: 0.0,
            filters: filters.clone(),
        };
        accepted += usize::from(check_playable(&correction, rate).is_ok());
        assert!(
            svolder_profiles::tone::apo::validate(&correction).is_ok(),
            "{filters:?}"
        );
        over.extend(over_unity(
            &format!("case {case} {filters:?}"),
            rate,
            &filters,
        ));
    }
    assert!(
        accepted >= 1500,
        "only {accepted} of 2500 stacks were accepted"
    );
    assert!(
        over.is_empty(),
        "{} stacks over unity:\n{}",
        over.len(),
        over.join("\n")
    );
}

/// What PipeWire cannot hold: a filter that is not stable in its f32 coefficients at the
/// running rate, or a curve that needs a preamp below the floor.
#[tokio::test]
async fn a_curve_the_chain_cannot_play_is_refused_before_anything_changes() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    let mut unstable = request("Unstable");
    unstable.rate_hz = Some(96_000);
    unstable.correction.filters = vec![filter(FilterKind::LowShelf, 10.0, 30.0, 20.0)];
    let failure = host.apply(unstable).await.unwrap_err();
    assert!(
        failure.what.contains("point 1 would not be stable"),
        "{}",
        failure.what
    );
    let mut too_deep = request("Deep");
    too_deep.correction.filters = vec![filter(FilterKind::LowShelf, 20.0, 12.0, 10.0); 16];
    let failure = host.apply(too_deep).await.unwrap_err();
    assert!(
        failure
            .what
            .contains("the computer EQ plays curves up to 120 dB"),
        "{}",
        failure.what
    );
    assert_eq!(f.written(), Vec::<String>::new());
    assert_eq!(f.systemctl_writes(), Vec::<String>::new());
}

/// The same refusals, asked of the check itself, and what it lets through.
#[test]
fn check_playable_refuses_unstable_f32_filters_and_curves_below_the_floor() {
    let one = |kind, f, g, q| Correction {
        preamp_db: 0.0,
        filters: vec![filter(kind, f, g, q)],
    };
    assert!(check_playable(&one(FilterKind::LowShelf, 10.0, 30.0, 20.0), 96_000).is_err());
    assert!(check_playable(&one(FilterKind::LowShelf, 10.0, 30.0, 0.707), 192_000).is_err());
    assert!(check_playable(&one(FilterKind::LowShelf, 10.0, 30.0, 0.707), 88_200).is_err());
    assert!(check_playable(&one(FilterKind::Peaking, 10.0, 30.0, 20.0), 192_000).is_err());
    assert!(check_playable(&one(FilterKind::HighShelf, 10.0, -30.0, 0.707), 192_000).is_err());
    for (rate, kind, f, g, q) in [
        (88_200, FilterKind::LowShelf, 20.0, 12.0, 0.707),
        (96_000, FilterKind::Peaking, 20.0, 1.5, 10.0),
        (192_000, FilterKind::Peaking, 100.0, 12.0, 10.0),
        (48_000, FilterKind::Peaking, 1000.0, 3.0, 1.0),
    ] {
        assert_eq!(
            check_playable(&one(kind, f, g, q), rate).map_err(|e| e.what),
            Ok(()),
            "{kind:?} {f} Hz {g} dB Q{q} at {rate}"
        );
    }
    let deep = Correction {
        preamp_db: 0.0,
        filters: vec![filter(FilterKind::LowShelf, 20.0, 12.0, 10.0); 16],
    };
    assert!(check_playable(&deep, 48_000).is_err());
}

/// Every corner of what a hand sets, at every rate the sink can run at, is playable.
#[test]
fn every_hand_corner_is_playable_at_every_rate() {
    use crate::eq::HAND;
    for rate in [44_100, 48_000, 88_200, 96_000, 176_400, 192_000] {
        for kind in [
            FilterKind::Peaking,
            FilterKind::LowShelf,
            FilterKind::HighShelf,
        ] {
            for f in [HAND.f.0, 31.5, 1000.0, HAND.f.1] {
                for g in [HAND.g.0, HAND.g.1] {
                    for q in [HAND.q.0, 0.707, HAND.q.1] {
                        let correction = Correction {
                            preamp_db: 0.0,
                            filters: vec![filter(kind, f, g, q)],
                        };
                        assert_eq!(
                            check_playable(&correction, rate).map_err(|e| e.what),
                            Ok(()),
                            "{kind:?} {f} Hz {g} dB Q{q} at {rate}"
                        );
                    }
                }
            }
        }
    }
}

/// A filter of 0 dB is the identity: never refused, however low its frequency and sharp its Q,
/// so a lone point can always be removed.
#[test]
fn a_filter_of_zero_db_is_playable_at_any_rate() {
    for rate in [44_100, 48_000, 96_000, 176_400, 192_000] {
        for kind in [
            FilterKind::Peaking,
            FilterKind::LowShelf,
            FilterKind::HighShelf,
        ] {
            for (f, q) in [(1.0, 100.0), (10.0, 20.0), (20.0, 0.02), (20.0, 20.0)] {
                let correction = Correction {
                    preamp_db: 0.0,
                    filters: vec![filter(kind, f, 0.0, q)],
                };
                assert_eq!(
                    check_playable(&correction, rate).map_err(|e| e.what),
                    Ok(()),
                    "{kind:?} {f} Hz 0 dB Q{q} at {rate}"
                );
            }
        }
    }
}

/// A wide peak past the coefficient range is a peak, not a shelf, in the refusal.
#[test]
fn the_coefficient_refusal_names_the_point_and_does_not_call_a_peak_a_shelf() {
    let correction = Correction {
        preamp_db: 0.0,
        filters: vec![
            filter(FilterKind::Peaking, 1000.0, 0.0, 1.0),
            filter(FilterKind::Peaking, 1000.0, 30.0, 0.02),
        ],
    };
    let failure = check_playable(&correction, 48_000).unwrap_err();
    assert!(
        failure.what.starts_with("point 2 boosts more than"),
        "{failure}"
    );
    assert!(!failure.what.contains("shelf") && !failure.what.contains("filter"));
}
