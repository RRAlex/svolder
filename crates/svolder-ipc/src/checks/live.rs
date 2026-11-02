// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The live snapshot of the host the checks read.
//!
//! Everything here talks to the system: BlueZ over the system bus, systemd through
//! `systemctl`, and the filesystem for the rest. It is gathered once, on a blocking thread,
//! and handed to [`run`](super::run) as a [`Probes`] snapshot, so the checks themselves stay
//! a pure function of what was found and every caller sees one consistent picture of the host.
//!
//! Every probe is given [`TIMEOUT`] and nothing here ever fails the caller: a probe that does
//! not answer leaves its fact absent, and that is what the checks report on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use dbus::arg::{PropMap, RefArg};
use dbus::blocking::Connection;
use dbus::blocking::stdintf::org_freedesktop_dbus::ObjectManager;

use super::processes::{RunningDaemon, running_daemons};
use super::{AdapterFacts, BluezFacts, EnvFacts, Probes, SystemdFacts, UnitFacts};

/// How long any one D-Bus call or subprocess gets before it is given up on.
pub const TIMEOUT: Duration = Duration::from_secs(2);

/// How long the whole snapshot gets: enough for each probe to spend its own [`TIMEOUT`] in
/// turn. It bounds what a single probe's own timeout cannot — opening the system bus, which the
/// `dbus` crate gives no deadline of its own — so a caller always gets an answer.
pub const BOUND: Duration = Duration::from_secs(4 * TIMEOUT.as_secs());

/// How often a subprocess that has not finished is looked at again.
const POLL: Duration = Duration::from_millis(20);

/// What the host said when it was asked, once.
///
/// Take one with [`live`] and read it through its [`Probes`] implementation; it holds no
/// connection and no handle, so it can be kept for as long as its answers are wanted.
#[derive(Debug, Clone)]
pub struct LiveSnapshot {
    /// What BlueZ answered over the system bus.
    bluez: BluezFacts,
    /// What `systemctl` printed about the units.
    systemd: SystemdFacts,
    /// What the environment and the filesystem said.
    env: EnvFacts,
    /// The config file's text, as [`Probes::config_text`] describes it.
    config: Option<Result<String, String>>,
    /// Every profile name the built-in set and the user's profile directory hold.
    profiles: Vec<String>,
    /// Why the user's profile directory could not be loaded, when it could not.
    profiles_error: Option<String>,
    /// Every svolderd this user is running, as `/proc` reports them.
    daemons: Vec<RunningDaemon>,
}

impl Probes for LiveSnapshot {
    fn bluez(&self) -> BluezFacts {
        self.bluez.clone()
    }

    fn systemd(&self) -> SystemdFacts {
        self.systemd.clone()
    }

    fn env(&self) -> EnvFacts {
        self.env.clone()
    }

    fn config_text(&self) -> Option<Result<String, String>> {
        self.config.clone()
    }

    fn profiles(&self) -> Vec<String> {
        self.profiles.clone()
    }

    fn profiles_error(&self) -> Option<String> {
        self.profiles_error.clone()
    }

    fn daemons(&self) -> Vec<RunningDaemon> {
        self.daemons.clone()
    }
}

/// Ask the host everything the checks read, on a blocking thread, within [`BOUND`].
///
/// `socket` is the socket path the caller resolved: its directory is the one the runtime check
/// wants to know it can write to. `config` is the config file the caller reads, which is also
/// where the profile directory beside it is looked for.
pub async fn live(socket: &Path, config: &Path) -> LiveSnapshot {
    let (socket, config) = (socket.to_path_buf(), config.to_path_buf());
    within(
        BOUND,
        tokio::task::spawn_blocking(move || gather(&socket, &config)),
    )
    .await
}

/// [`live`] with its gathering and its bound passed in, so the path where the host never
/// answers can be taken without a host.
async fn within(
    bound: Duration,
    gathering: impl Future<Output = Result<LiveSnapshot, tokio::task::JoinError>>,
) -> LiveSnapshot {
    match tokio::time::timeout(bound, gathering).await {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(e)) => unread(format!("the host could not be asked: {e}")),
        Err(_) => unread(format!("timed out after {} s", bound.as_secs())),
    }
}

/// A snapshot of a host that did not answer: every fact absent, and the reason carried where
/// the checks that would have used it can say it.
fn unread(why: String) -> LiveSnapshot {
    LiveSnapshot {
        bluez: BluezFacts {
            reachable: Err(why.clone()),
            adapters: Vec::new(),
            unreadable: Some(why.clone()),
        },
        systemd: SystemdFacts {
            unreadable: Some(why),
            ..absent()
        },
        env: EnvFacts {
            runtime_dir: None,
            socket_dir_writable: None,
            container: false,
            nixos: false,
            uid: uid(),
            wireplumber_version: None,
            wireplumber_midi_disabled: None,
        },
        config: None,
        profiles: Vec::new(),
        profiles_error: None,
        daemons: Vec::new(),
    }
}

/// Every probe, in one blocking pass.
fn gather(socket: &Path, config: &Path) -> LiveSnapshot {
    let (profiles, profiles_error) = profile_names(config);
    LiveSnapshot {
        bluez: bluez_facts(),
        systemd: systemd_facts(),
        env: env_facts(socket),
        config: config_text(config),
        profiles,
        profiles_error,
        daemons: running_daemons(),
    }
}

/// What BlueZ answers to `GetManagedObjects` on `org.bluez`: every object carrying
/// `org.bluez.Adapter1`, with the advertising length its `LEAdvertisingManager1` reports.
fn bluez_facts() -> BluezFacts {
    let unreachable = |why: String| BluezFacts {
        reachable: Err(why),
        adapters: Vec::new(),
        unreadable: None,
    };
    let connection = match Connection::new_system() {
        Ok(connection) => connection,
        Err(e) => return unreachable(e.to_string()),
    };
    let objects = match connection
        .with_proxy("org.bluez", "/", TIMEOUT)
        .get_managed_objects()
    {
        Ok(objects) => objects,
        Err(e) => return unreachable(e.to_string()),
    };
    let mut adapters: Vec<AdapterFacts> = objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let adapter = interfaces.get("org.bluez.Adapter1")?;
            Some(AdapterFacts {
                name: path.rsplit('/').next().unwrap_or_default().to_string(),
                address: string_of(adapter, "Address").unwrap_or_default(),
                powered: adapter
                    .get("Powered")
                    .and_then(|value| value.as_u64())
                    .is_some_and(|value| value != 0),
                max_adv_len: interfaces
                    .get("org.bluez.LEAdvertisingManager1")
                    .and_then(max_adv_len),
            })
        })
        .collect();
    adapters.sort_by(|a, b| a.name.cmp(&b.name));
    BluezFacts {
        reachable: Ok(()),
        adapters,
        unreadable: None,
    }
}

/// One string property of an interface's properties.
fn string_of(props: &PropMap, key: &str) -> Option<String> {
    Some(props.get(key)?.as_str()?.to_string())
}

/// `MaxAdvLen` out of `LEAdvertisingManager1`'s `SupportedCapabilities`, a dictionary inside a
/// variant, which is read by walking its flattened key-value pairs.
fn max_adv_len(props: &PropMap) -> Option<u8> {
    let capabilities = props.get("SupportedCapabilities")?;
    let mut entries = capabilities.0.as_iter()?;
    while let (Some(key), Some(value)) = (entries.next(), entries.next()) {
        if key.as_str() == Some("MaxAdvLen") {
            return value.as_u64().and_then(|len| u8::try_from(len).ok());
        }
    }
    None
}

/// The socket unit this project installs, as `systemctl` names it.
pub const SOCKET_UNIT: &str = "svolderd.socket";

/// The service unit this project installs, as `systemctl` names it.
pub const SERVICE_UNIT: &str = "svolderd.service";

/// The properties [`user_units`] reads out of `systemctl --user show`.
pub const UNIT_PROPERTIES: &str = "Id,LoadState,UnitFileState,ActiveState,Listen";

/// The environment variable that names the `systemctl` to run.
///
/// `make install-service SYSTEMCTL=echo` is a dry run of the install: the Makefile hands its
/// own `SYSTEMCTL` to the verb through this, so what the Makefile is told to run and what runs
/// are one thing.
const SYSTEMCTL_ENV: &str = "SVOLDER_SYSTEMCTL";

/// The `systemctl` to run: `$SVOLDER_SYSTEMCTL`, else `systemctl` itself.
///
/// Every program in this workspace that runs one looks it up here — the `daemon` verbs, these
/// probes, and the daemon's own startup checks — so a host that names another one, or a test
/// that names a program that is not there, is read the same way by all of them rather than by
/// whichever of them happened to ask.
pub fn systemctl_program() -> std::ffi::OsString {
    std::env::var_os(SYSTEMCTL_ENV)
        .filter(|named| !named.is_empty())
        .unwrap_or_else(|| "systemctl".into())
}

/// What `systemctl` says about the two units this project installs and about bluetoothd.
///
/// `Id` is asked for alongside the rest so each block of properties names the unit it belongs
/// to, whatever order `systemctl` prints them in and whether or not it knows the unit at all.
/// Nothing here changes a unit: both calls are `show`, which reads.
///
/// It is gathered once per snapshot, by [`gather`], and read through [`Probes::systemd`]; a
/// caller that wants one of these facts on its own takes a snapshot rather than paying these
/// two subprocesses again.
fn systemd_facts() -> SystemdFacts {
    let systemctl = systemctl_program();
    let user = output_within(Command::new(&systemctl).args([
        "--user",
        "show",
        SOCKET_UNIT,
        SERVICE_UNIT,
        "-p",
        UNIT_PROPERTIES,
    ]));
    let bluetoothd_exec_start =
        output_within(Command::new(&systemctl).args(["show", "bluetooth", "-p", "ExecStart"]))
            .as_deref()
            .and_then(exec_start);
    let mut facts = match user.as_deref() {
        Some(text) => user_units(text),
        None => absent(),
    };
    facts.bluetoothd_exec_start = bluetoothd_exec_start;
    facts
}

/// What the two units look like in the output of
/// `systemctl --user show <SOCKET_UNIT> <SERVICE_UNIT> -p <UNIT_PROPERTIES>`.
///
/// Anything that ran that command reads its answer through this, so the CLI's `daemon` verbs
/// and the health checks cannot disagree about what systemd said. Nothing but the two units is
/// read here: `bluetoothd_exec_start` comes from a command of its own.
pub fn user_units(text: &str) -> SystemdFacts {
    let blocks = blocks(text);
    let block = |id: &str| blocks.iter().find(|b| b.get("Id") == Some(&id));
    let socket = block(SOCKET_UNIT);
    SystemdFacts {
        present: true,
        socket_unit: socket.and_then(|b| unit_facts(b)),
        service_unit: block(SERVICE_UNIT).and_then(|b| unit_facts(b)),
        listen_stream: socket
            .and_then(|b| b.get("Listen").copied())
            .and_then(listen_path),
        bluetoothd_exec_start: None,
        unreadable: None,
    }
}

/// What a host with no systemd user session says: no units, and nothing wrong with the host.
pub fn absent() -> SystemdFacts {
    SystemdFacts {
        present: false,
        socket_unit: None,
        service_unit: None,
        listen_stream: None,
        bluetoothd_exec_start: None,
        unreadable: None,
    }
}

/// One `key=value` map per unit: `systemctl show` separates the units it was asked about with
/// a blank line.
fn blocks(text: &str) -> Vec<HashMap<&str, &str>> {
    let mut blocks = Vec::new();
    let mut current: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else if let Some((key, value)) = line.split_once('=') {
            current.insert(key, value);
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

/// One unit's state, or `None` when systemd has no unit file for it.
///
/// `enabled` is read as "starts on its own": everything but a unit that was never enabled, is
/// disabled, or is masked, so a unit another one pulls in — a `static` service behind a socket
/// — counts as enabled.
fn unit_facts(block: &HashMap<&str, &str>) -> Option<UnitFacts> {
    let load_state = block.get("LoadState").copied().unwrap_or_default();
    if load_state == "not-found" {
        return None;
    }
    let file_state = block.get("UnitFileState").copied().unwrap_or_default();
    let state = block.get("ActiveState").copied();
    Some(UnitFacts {
        installed: load_state == "loaded",
        enabled: !matches!(file_state, "" | "disabled" | "masked" | "masked-runtime"),
        active: state == Some("active"),
        running: matches!(state, Some("active" | "activating" | "reloading")),
    })
}

/// The path out of a socket unit's `Listen` property, which systemd prints as
/// `/run/…/svolder.sock (Stream)`: the text before the last ` (`, when it is an absolute path.
/// A value with no `(…)` suffix that is an absolute path is kept as it is.
fn listen_path(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    let path = match value.rfind(" (") {
        Some(at) => value[..at].trim(),
        None => value,
    };
    let path = PathBuf::from(path);
    path.is_absolute().then_some(path)
}

/// The command line out of `systemctl show -p ExecStart`, which reads
/// `ExecStart={ path=… ; argv[]=… ; … }`.
fn exec_start(text: &str) -> Option<String> {
    let value = text.lines().find_map(|l| l.strip_prefix("ExecStart="))?;
    let argv = value.split("argv[]=").nth(1)?.split(" ; ").next()?.trim();
    (!argv.is_empty()).then(|| argv.to_string())
}

/// A command's standard output when it ran, exited 0 and did so within [`TIMEOUT`]; `None`
/// when the binary is absent, the command failed, or it had to be killed.
fn output_within(command: &mut Command) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // The output is a few hundred bytes of properties, far inside the pipe's own
                // buffer, so the child is never waiting on a reader that has not started.
                let output = child.wait_with_output().ok()?;
                return status
                    .success()
                    .then(|| String::from_utf8_lossy(&output.stdout).into_owned());
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => return None,
        }
    }
}

/// What the environment and the filesystem say about where this process runs.
///
/// `socket` is the socket path the caller resolved: its directory is the one the runtime check
/// wants to know it can write to.
pub fn env_facts(socket: &Path) -> EnvFacts {
    let wireplumber_version = wireplumber_version();
    EnvFacts {
        runtime_dir: std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        socket_dir_writable: socket.parent().and_then(writable),
        container: in_container(),
        nixos: Path::new("/etc/NIXOS").exists(),
        uid: uid(),
        wireplumber_midi_disabled: wireplumber_version.as_ref().map(|_| midi_disabled()),
        wireplumber_version,
    }
}

/// Whether a directory that exists can be written to and entered; `None` when it is not there
/// to be judged.
fn writable(dir: &Path) -> Option<bool> {
    use std::os::unix::ffi::OsStrExt;
    if !dir.is_dir() {
        return None;
    }
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: `access` reads the permissions of the path it is handed and nothing else; the
    // pointer is a valid NUL-terminated string that outlives the call.
    Some(unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) } == 0)
}

/// Whether this process runs in a container, which the runtime leaves a mark of either in a
/// file of its own — `/.dockerenv` for Docker, `/run/.containerenv` for Podman — or in pid 1's
/// cgroup.
pub fn in_container() -> bool {
    if Path::new("/.dockerenv").exists() || Path::new("/run/.containerenv").exists() {
        return true;
    }
    std::fs::read_to_string("/proc/1/cgroup").is_ok_and(|cgroup| {
        ["docker", "containerd", "podman"]
            .iter()
            .any(|runtime| cgroup.contains(runtime))
    })
}

/// The installed WirePlumber's version, from the banner of the program
/// [`WIREPLUMBER_ENV`](svolder_host::eq::WIREPLUMBER_ENV) names, else `wireplumber` on `PATH`.
fn wireplumber_version() -> Option<String> {
    let named = std::env::var_os(svolder_host::eq::WIREPLUMBER_ENV);
    wireplumber_version_of(&wireplumber_program(named))
}

/// The program that reports the WirePlumber version: the one `named`, else `wireplumber`.
fn wireplumber_program(named: Option<std::ffi::OsString>) -> std::ffi::OsString {
    named
        .filter(|program| !program.is_empty())
        .unwrap_or_else(|| "wireplumber".into())
}

/// The WirePlumber version `program --version` reports, or `None` when it does not run or its
/// banner names no linked version.
fn wireplumber_version_of(program: &std::ffi::OsStr) -> Option<String> {
    let banner = output_within(Command::new(program).arg("--version"))?;
    version_from_banner(&banner)
}

/// The version a `wireplumber --version` banner reports as linked, as `major.minor.patch`:
/// `host eq` reads the same line the same way, so both give one verdict.
fn version_from_banner(banner: &str) -> Option<String> {
    let (major, minor, patch) = svolder_host::eq::wireplumber_version(banner)?;
    Some(format!("{major}.{minor}.{patch}"))
}

/// Whether any WirePlumber configuration drop-in switches the BLE MIDI monitor off, in the
/// system's directory or this user's. A commented-out line switches nothing off, so the lines
/// the file's own syntax ignores are ignored here too.
fn midi_disabled() -> bool {
    let mut dirs = vec![PathBuf::from("/etc/wireplumber/wireplumber.conf.d")];
    if let Some(user) = config_home() {
        dirs.push(user.join("wireplumber").join("wireplumber.conf.d"));
    }
    dirs.iter().any(|dir| {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|x| x == "conf"))
                .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
                .any(|text| {
                    text.lines()
                        .map(str::trim_start)
                        .filter(|line| !line.starts_with('#'))
                        .any(|line| {
                            line.contains("monitor.bluez-midi") && line.contains("disabled")
                        })
                })
        })
    })
}

/// `$XDG_CONFIG_HOME`, or `.config` under the user's home directory when it is unset; `None`
/// when neither is known.
fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| crate::config::home_dir().map(|home| home.join(".config")))
}

/// The config file's text, `None` when there is no file and `Err` when there is one that could
/// not be read.
fn config_text(path: &Path) -> Option<Result<String, String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(Ok(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => Some(Err(format!("{}: {e}", path.display()))),
    }
}

/// Every profile name a device may name: the built-in set, plus the user's profile directory
/// beside the config file — the same directory the daemon and the CLI build their registry from.
///
/// When that directory fails to load the names are the built-in ones, and the second part says
/// which directory and why.
fn profile_names(config: &Path) -> (Vec<String>, Option<String>) {
    let dir = config
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("profiles");
    let (registry, error) = match svolder_profiles::Registry::with_user_dir(&dir) {
        Ok(registry) => (Ok(registry), None),
        Err(e) => (
            svolder_profiles::Registry::builtin(),
            Some(format!("{}: {e}", dir.display())),
        ),
    };
    let names = registry
        .map(|registry| registry.names().into_iter().map(str::to_string).collect())
        .unwrap_or_default();
    (names, error)
}

/// The process's real user id.
fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail; it only reads the process's real uid.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{
        ADAPTERS, BLUETOOTH_SERVICE, CONFIG, DAEMON, Outcome, State, exit_code, run,
    };
    use crate::test_support::TempPath;

    /// A unit systemd is starting or reloading is running, though only an `active` one is active.
    #[test]
    fn a_unit_starting_or_reloading_is_running_but_not_active() {
        for (state, running) in [
            ("active", true),
            ("activating", true),
            ("reloading", true),
            ("deactivating", false),
            ("inactive", false),
            ("failed", false),
        ] {
            let text = format!(
                "Id={SERVICE_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState={state}\n"
            );
            let unit = user_units(&text)
                .service_unit
                .expect("the service is known");
            assert_eq!(unit.running, running, "{state}");
            assert_eq!(unit.active, state == "active", "{state}");
        }
    }

    /// The text `systemctl --user show` prints for the two units: several properties each, the
    /// socket's `Listen` as `<path> (Stream)`.
    #[test]
    fn the_listen_path_is_read_as_systemd_prints_it() {
        let text = format!(
            "Id={SOCKET_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState=active\n\
             Listen=/run/user/1000/svolder.sock (Stream)\n\n\
             Id={SERVICE_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState=active\n"
        );
        assert_eq!(
            user_units(&text).listen_stream,
            Some(PathBuf::from("/run/user/1000/svolder.sock"))
        );
    }

    /// A value that holds no absolute path is no path; a bare absolute path is one.
    #[test]
    fn a_listen_value_without_an_absolute_path_is_none() {
        assert_eq!(
            listen_path("/run/a b/s.sock (Stream)"),
            Some("/run/a b/s.sock".into())
        );
        assert_eq!(listen_path("/run/s.sock"), Some("/run/s.sock".into()));
        assert_eq!(listen_path("[::]:80 (Stream)"), None);
        assert_eq!(listen_path("(Stream)"), None);
        assert_eq!(listen_path(""), None);
    }

    /// The banner of a WirePlumber built against one library and run against another.
    const BANNER: &str =
        "wireplumber\nCompiled with libwireplumber 0.5.10\nLinked with libwireplumber 0.5.13\n";

    /// The version is the library that runs, not the one the binary was built with.
    #[test]
    fn the_version_is_the_one_linked_not_the_one_compiled_with() {
        assert_eq!(version_from_banner(BANNER).as_deref(), Some("0.5.13"));
        assert_eq!(
            version_from_banner("Linked with libwireplumber 1.0").as_deref(),
            Some("1.0.0")
        );
    }

    /// A banner with no linked line is WirePlumber not installed, as it is for `host eq`.
    #[test]
    fn a_banner_with_no_linked_line_reads_as_not_installed() {
        assert_eq!(
            version_from_banner("wireplumber\nCompiled with libwireplumber 0.5.10\n"),
            None
        );
        assert_eq!(version_from_banner("wpctl 0.5.13"), None);
        assert_eq!(version_from_banner(""), None);
    }

    /// A 0.4 library still reads as 0.4, which is what picks the Lua advice.
    #[test]
    fn a_0_4_library_still_reads_as_0_4() {
        let version =
            version_from_banner("Linked with libwireplumber 0.4.17").expect("a linked version");
        assert!(version.starts_with("0.4"), "{version}");
        assert_eq!(version, "0.4.17");
    }

    /// The program asked is the one named, else `wireplumber`.
    #[test]
    fn the_program_asked_is_the_one_named_else_wireplumber() {
        assert_eq!(wireplumber_program(None), "wireplumber");
        assert_eq!(wireplumber_program(Some("".into())), "wireplumber");
        assert_eq!(wireplumber_program(Some("/opt/wp".into())), "/opt/wp");
    }

    /// The probe runs the program it is given, never the real WirePlumber: a script that prints
    /// a banner reads as that version, and a program that is not there reads as none.
    #[test]
    fn the_probe_reads_the_banner_of_the_program_it_is_given() {
        let dir = std::env::temp_dir().join(format!("svolder-wp-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let script = dir.join("wireplumber");
        crate::test_support::install_script(
            &script,
            "#!/bin/sh\nprintf 'wireplumber\\nCompiled with libwireplumber 0.5.10\\nLinked with libwireplumber 0.5.13\\n'\n",
        );
        assert_eq!(
            wireplumber_version_of(script.as_os_str()).as_deref(),
            Some("0.5.13")
        );
        assert_eq!(wireplumber_version_of(dir.join("absent").as_os_str()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The config check's row for a config file in a temp directory that names one device with
    /// `profile = "mine"`, beside a `profiles/mine.toml` holding `profile_text`.
    fn config_row_beside(tag: &str, profile_text: &str) -> Outcome {
        config_row_naming(tag, "mine", Some(profile_text))
    }

    /// The config check's row for a config file naming one device with `profile`, beside a
    /// `profiles/mine.toml` holding `profile_text`, or beside no `profiles` directory for `None`.
    fn config_row_naming(tag: &str, profile: &str, profile_text: Option<&str>) -> Outcome {
        let dir = TempPath(
            std::env::temp_dir().join(format!("svolder-profiles-{tag}-{}", std::process::id())),
        );
        let _ = std::fs::remove_dir_all(&dir.0);
        std::fs::create_dir_all(&dir.0).expect("make the directory");
        if let Some(text) = profile_text {
            std::fs::create_dir_all(dir.join("profiles")).expect("make the profiles directory");
            std::fs::write(dir.join("profiles/mine.toml"), text).expect("write the profile");
        }
        let config = dir.join("config.toml");
        std::fs::write(
            &config,
            format!(
                "schema = 1\n\n[[device]]\nname = \"mine\"\nudid = \"0011223344556677\"\n\
                 profile = \"{profile}\"\n"
            ),
        )
        .expect("write the config");
        let mut snapshot = unread("not asked".into());
        snapshot.config = config_text(&config);
        (snapshot.profiles, snapshot.profiles_error) = profile_names(&config);
        let rows = run(&snapshot, Path::new("/run/user/1000/svolder.sock"), false);
        rows.into_iter()
            .find(|o| o.check == CONFIG)
            .expect("a config row")
    }

    /// A device naming a built-in profile is not called missing, and the unreadable directory
    /// is not said to hold it, while the user's profile directory is broken: the row fails on the
    /// directory alone.
    #[test]
    fn a_built_in_profile_beside_a_broken_directory_is_not_called_missing() {
        let builtin = svolder_profiles::Registry::builtin()
            .expect("the built-in profiles load")
            .names()
            .first()
            .map(|name| (*name).to_string())
            .expect("a built-in profile");
        let row = config_row_naming("builtin-broken", &builtin, Some("[profile\n"));
        assert!(matches!(row.state, State::Fail), "{row:?}");
        assert!(row.reason.contains("TOML parse error"), "{row:?}");
        assert!(!row.reason.contains("may hold"), "{row:?}");
        assert!(!row.reason.contains("does not exist"), "{row:?}");
    }

    /// A config with no `profiles` directory beside it adds no row about profiles.
    #[test]
    fn a_config_with_no_profiles_directory_adds_no_profile_row() {
        let builtin = svolder_profiles::Registry::builtin()
            .expect("the built-in profiles load")
            .names()
            .first()
            .map(|name| (*name).to_string())
            .expect("a built-in profile");
        let row = config_row_naming("no-profiles-dir", &builtin, None);
        assert!(!matches!(row.state, State::Fail), "{row:?}");
        assert!(!row.reason.contains("profiles"), "{row:?}");
        assert!(!row.reason.contains("does not exist"), "{row:?}");
    }

    /// A profile directory that fails to load is named, with the loader's reason, and a device
    /// naming a profile is not called missing.
    #[test]
    fn a_profile_directory_that_fails_to_load_is_named_and_hides_no_profile() {
        let row = config_row_beside("broken", "[profile\n");
        assert!(matches!(row.state, State::Fail), "{row:?}");
        assert!(row.reason.contains("profiles"), "{row:?}");
        assert!(row.reason.contains("TOML parse error"), "{row:?}");
        assert!(!row.reason.contains("does not exist"), "{row:?}");
        assert!(!row.reason.contains('\n'), "{row:?}");
    }

    /// A profile directory that loads leaves the config row as it is.
    #[test]
    fn a_profile_directory_that_loads_adds_nothing() {
        let valid = "schema = 1\nname = \"mine\"\ndisplay = \"Mine\"\nextends = \"amp-common\"\n\
                     product_id = 65000\n";
        let row = config_row_beside("valid", valid);
        assert!(!row.reason.contains("profiles"), "{row:?}");
        assert!(!row.reason.contains("does not exist"), "{row:?}");
    }

    /// A host that never answers still leaves a snapshot every check can be read from, and it
    /// leaves it within the bound rather than whenever the probes give up.
    ///
    /// The gathering here never finishes, and time is paused, so the eight seconds pass the
    /// moment nothing else is runnable.
    #[tokio::test(start_paused = true)]
    async fn a_host_that_never_answers_times_out_into_a_snapshot_that_says_so() {
        let started = tokio::time::Instant::now();
        let snapshot = within(BOUND, std::future::pending()).await;
        assert!(started.elapsed() <= BOUND, "{:?}", started.elapsed());

        let why = "timed out after 8 s".to_string();
        assert_eq!(snapshot.bluez().reachable, Err(why.clone()));
        assert_eq!(snapshot.bluez().unreadable, Some(why.clone()));
        assert!(!snapshot.systemd().present);
        assert_eq!(snapshot.systemd().unreadable, Some(why));
        assert!(snapshot.env().wireplumber_version.is_none());
        assert!(snapshot.config_text().is_none());

        // Every check still produces a line, the daemon's among them.
        let outcomes = run(&snapshot, Path::new("/run/user/1000/svolder.sock"), true);
        assert_eq!(outcomes.len(), 8, "{outcomes:?}");
        let daemon = outcomes.iter().find(|o| o.check == DAEMON).unwrap();
        assert!(daemon.reason.contains("could not be read"), "{daemon:?}");

        // A host that was too slow to answer is not a host with a broken Bluetooth service:
        // neither row fails, and neither sends the reader after a fix for a problem nobody has
        // shown to exist.
        for check in [BLUETOOTH_SERVICE, ADAPTERS] {
            let row = outcomes.iter().find(|o| o.check == check).unwrap();
            assert!(!matches!(row.state, State::Fail), "{row:?}");
            assert!(row.fix.is_none(), "{row:?}");
            assert!(row.reason.contains("could not be read"), "{row:?}");
            assert!(row.reason.contains("timed out after 8 s"), "{row:?}");
        }
        assert_eq!(exit_code(&outcomes), 1, "{outcomes:?}");
    }
}
