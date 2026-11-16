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
    sweep_finished_runs_in(&std::env::temp_dir());
}

/// [`sweep_finished_runs`] over the scratch directories of `root`.
fn sweep_finished_runs_in(root: &Path) {
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
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
            epoch: tokio::sync::watch::Sender::new(0),
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
  if [ -f "$D/unit-activating" ]; then echo ActiveState=activating
  elif [ -f "$D/units/svolder-host-eq.service" ] && [ ! -f "$D/unit-stopped" ]; then echo ActiveState=active; else echo ActiveState=inactive; fi
  if [ -f "$D/mainpid" ]; then echo MainPID=$(cat "$D/mainpid"); else echo MainPID=0; fi ;;
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
        flat: false,
        crossfeed: None,
        speakers: None,
    }
}

/// Wait up to ten seconds for `done` to hold.
async fn until(done: impl Fn() -> bool) -> bool {
    // Returns as soon as `done` holds; the bound is long because a process may take seconds to
    // start or end on a machine under heavy load.
    for _ in 0..200 {
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
fn pid_of_or_none(fixture: &Fixture) -> Option<(u32, Argv)> {
    parse_pid_file(&std::fs::read_to_string(fixture.dir.join("run/svolder-host-eq.pid")).ok()?)
}

mod child;
mod exit;
mod paths;
mod playable;
mod process;
mod range;
mod refusals;
mod review_rounds;
mod route;
mod stale_state;
mod status;
mod systemd;
mod unity;

#[test]
fn status_words_name_a_pass_through_and_a_chain_for_crossfeed_alone() {
    let state = |crossfeed_on| HostEqState {
        on: true,
        name: "x".into(),
        origin: None,
        target: "sink".into(),
        route: Route::Smart,
        manager: Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on,
        flat: true,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
    };
    let words = |crossfeed_on| {
        Status {
            state: Some(state(crossfeed_on)),
            running: Some(true),
            unreadable: None,
        }
        .lines()
        .join("\n")
    };
    assert!(words(true).contains("running for crossfeed alone"));
    assert!(words(false).contains("kept as a pass-through until idle (running)"));
}
