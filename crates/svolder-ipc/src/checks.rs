// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Health checks over a snapshot of the host.
//!
//! [`run`] turns a [`Probes`] snapshot — what BlueZ, systemd, the environment and the config
//! file say — into one [`Outcome`] per check, in the order they are meant to be read. Nothing
//! here reaches the system itself: [`live`](live::live) takes the snapshot, so a front end can
//! print the outcomes and the daemon can log them without either one growing its own probes,
//! and the tests drive every branch from a fixture.
//!
//! The checks that need a radio session — grading the adapters against a device, and scanning
//! for the configured devices — belong to the front end that has one.
//!
//! - [`live`]: taking the snapshot from the real host.
//! - [`processes`]: which svolderd processes this user is running.

pub mod live;
pub mod processes;

use std::path::{Path, PathBuf};

use crate::checks::processes::RunningDaemon;
use crate::config::Config;

/// The service that must answer before anything else can work.
pub const BLUETOOTH_SERVICE: &str = "Bluetooth service";
/// The adapters BlueZ reports.
pub const ADAPTERS: &str = "Adapters";
/// Where the socket lives.
pub const RUNTIME: &str = "Runtime";
/// The units that start the daemon.
pub const DAEMON: &str = "Daemon";
/// A svolderd of this user's that listens somewhere else than this session's socket.
pub const OTHER_DAEMONS: &str = "Other daemons";
/// The config file and what it names.
pub const CONFIG: &str = "Config";
/// What a container needs from its host.
pub const CONTAINER: &str = "Container";
/// Whether two adapters can advertise beyond the legacy 31 bytes.
pub const EXTENDED_ADVERTISING: &str = "Extended advertising";
/// Whether bluetoothd runs without the plugins that fight for the same devices.
pub const BLUETOOTHD_PLUGINS: &str = "bluetoothd plugins";
/// Whether WirePlumber leaves BLE MIDI devices alone.
pub const WIREPLUMBER: &str = "WirePlumber";

/// The command the daemon check's fix names: the verb that writes the units and enables them.
///
/// A front end that applies a fix itself recognises it by the command it carries, so the
/// command is written once, here, rather than in the check and again in the front end.
pub const DAEMON_INSTALL: &str = "svolder daemon install";

/// The command the config check's fix names: the verb that walks a first run.
pub const SETUP: &str = "svolder setup";

/// How a check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// The host has what this check looks for.
    Pass,
    /// Something works but is not what it should be.
    Warn,
    /// Something this check looks for is missing or broken.
    Fail,
    /// A fact that is neither good nor bad, recorded so the reader knows it was looked at.
    Note,
}

/// One check's result.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The check's name, as it is printed.
    pub check: &'static str,
    /// How it came out.
    pub state: State,
    /// One line saying what was found.
    pub reason: String,
    /// What to do about it, when there is something to do.
    pub fix: Option<Fix>,
}

/// What to do about a check that did not pass.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    /// The command to run, exactly as it should be typed.
    pub command: String,
    /// Whether a front end may run this itself, without asking the user to type it. A fix that
    /// needs root, or that changes the state of a radio, never may.
    pub applies_itself: bool,
}

/// An outcome is one JSON object of `check`, `state`, `reason`, `fix` and `applies_itself`:
/// the fix is flattened to its command, or `null` when there is none, and `applies_itself`
/// stands beside it so a reader never has to look inside a nested object.
impl serde::Serialize for Outcome {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("Outcome", 5)?;
        out.serialize_field("check", self.check)?;
        out.serialize_field("state", &self.state)?;
        out.serialize_field("reason", &self.reason)?;
        out.serialize_field("fix", &self.fix.as_ref().map(|f| f.command.as_str()))?;
        out.serialize_field(
            "applies_itself",
            &self.fix.as_ref().is_some_and(|f| f.applies_itself),
        )?;
        out.end()
    }
}

/// Everything the checks read, taken once so every check sees the same host.
pub trait Probes {
    /// What BlueZ says over the system bus.
    fn bluez(&self) -> BluezFacts;
    /// What systemd says about the units this project installs and about bluetoothd.
    fn systemd(&self) -> SystemdFacts;
    /// What the process environment and the filesystem say about where svolder runs.
    fn env(&self) -> EnvFacts;
    /// The config file's text, `None` when there is no file, and `Err` when there is one that
    /// could not be read.
    fn config_text(&self) -> Option<Result<String, String>>;
    /// Every profile name a device may name.
    ///
    /// The default is the profiles compiled into the binary; a snapshot that can see the
    /// user's profile directory adds what that holds. An empty list means the profiles could
    /// not be listed at all, and the config check then does not judge a device's profile.
    fn profiles(&self) -> Vec<String> {
        svolder_profiles::Registry::builtin()
            .map(|registry| registry.names().into_iter().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Why the user's profile directory could not be loaded, naming the directory, `None` when
    /// it loaded or when nobody looked.
    fn profiles_error(&self) -> Option<String> {
        None
    }

    /// Every svolderd this user is running, which a snapshot that can read `/proc` fills in.
    ///
    /// The default is none: a snapshot that cannot say which processes are running says
    /// nothing about them rather than claiming there are none to see.
    fn daemons(&self) -> Vec<RunningDaemon> {
        Vec::new()
    }
}

/// What BlueZ says over the system bus.
#[derive(Debug, Clone, PartialEq)]
pub struct BluezFacts {
    /// `Ok` when `org.bluez` answered, and why it did not when it did not.
    pub reachable: Result<(), String>,
    /// One entry per adapter BlueZ reports, in name order.
    pub adapters: Vec<AdapterFacts>,
    /// Why BlueZ could not be asked at all — a probe that ran out of time, or a runtime that
    /// went away under it. `None` when the bus was asked, whatever it answered, so a service
    /// that refused and a service that was never reached are told apart.
    pub unreadable: Option<String>,
}

/// One adapter, as BlueZ describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterFacts {
    /// The adapter's kernel name, e.g. `hci0`.
    pub name: String,
    /// Its Bluetooth address.
    pub address: String,
    /// Whether it is powered on.
    pub powered: bool,
    /// The longest advertisement it accepts, `None` when it does not say. A controller with
    /// extended advertising reports far more than the legacy 31 bytes.
    pub max_adv_len: Option<u8>,
}

/// What systemd says about the units this project installs and about bluetoothd.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemdFacts {
    /// Whether a systemd user session answered at all.
    pub present: bool,
    /// `svolderd.socket`, `None` when systemd does not know it.
    pub socket_unit: Option<UnitFacts>,
    /// `svolderd.service`, `None` when systemd does not know it.
    pub service_unit: Option<UnitFacts>,
    /// The path `svolderd.socket` listens on, when it names one.
    pub listen_stream: Option<PathBuf>,
    /// The command line `bluetooth.service` starts bluetoothd with, when systemd reports one.
    pub bluetoothd_exec_start: Option<String>,
    /// Why systemd could not be asked at all — a probe that ran out of time, or a runtime that
    /// went away under it. `None` when `systemctl` was run, whatever it answered, so a host
    /// with no systemd at all and a host that could not be read are told apart.
    pub unreadable: Option<String>,
}

/// One unit's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitFacts {
    /// Whether the unit file is there to be loaded.
    pub installed: bool,
    /// Whether it starts on its own: enabled, or pulled in by another unit.
    pub enabled: bool,
    /// Whether it is running now — listening, for a socket unit.
    pub active: bool,
    /// Whether systemd has it running or on its way there: active, starting (`activating`) or
    /// reloading. What a verb that restarts or replaces a running daemon goes by.
    pub running: bool,
}

/// What the process environment and the filesystem say about where svolder runs.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvFacts {
    /// `$XDG_RUNTIME_DIR`, when it is set.
    pub runtime_dir: Option<PathBuf>,
    /// Whether the socket's own directory can be written to, `None` when it does not exist
    /// yet and so cannot be judged.
    pub socket_dir_writable: Option<bool>,
    /// Whether this process runs in a container.
    pub container: bool,
    /// Whether this host is NixOS, where unit files are generated rather than written.
    pub nixos: bool,
    /// This process's real user id.
    pub uid: u32,
    /// The installed WirePlumber's version, `None` when it is not installed.
    pub wireplumber_version: Option<String>,
    /// Whether WirePlumber's BLE MIDI monitor is switched off, `None` when there is no
    /// WirePlumber to ask about.
    pub wireplumber_midi_disabled: Option<bool>,
}

/// The plugins bluetoothd must not load for a BLE peripheral of this project's to be reachable:
/// each one claims services or connections svolder and the simulator need for themselves.
const NOPLUGIN: &str = "midi,vcp,micp,bass,csip,mcp,ccp";

/// The drop-in that switches WirePlumber 0.5's BLE MIDI monitor off.
const WIREPLUMBER_DROP_IN: &str = "/etc/wireplumber/wireplumber.conf.d/90-no-ble-midi.conf";

/// Every check but the ones that need a radio session, in the order they are read.
///
/// `socket` is the socket path the caller resolved, which the daemon check holds the socket
/// unit to. `dev` adds the three checks only someone running the simulator or the end-to-end
/// script cares about.
pub fn run(probes: &dyn Probes, socket: &Path, dev: bool) -> Vec<Outcome> {
    let bluez = probes.bluez();
    let systemd = probes.systemd();
    let env = probes.env();
    let config = probes.config_text().map(|text| match text {
        Ok(text) => Config::parse(&text).map_err(|e| first_line(&e.to_string())),
        Err(message) => Err(first_line(&message)),
    });

    let mut outcomes = vec![
        bluetooth_service(&bluez),
        adapters(&bluez, config.as_ref()),
        runtime(&env, socket),
        daemon(&systemd, socket),
    ];
    outcomes.extend(other_daemons(
        &probes.daemons(),
        socket,
        systemd.listen_stream.as_deref(),
    ));
    outcomes.push(self::config(
        config.as_ref(),
        &probes.profiles(),
        probes.profiles_error().as_deref(),
    ));
    if env.container {
        outcomes.push(container(&bluez, &systemd));
    }
    if dev {
        outcomes.push(extended_advertising(&bluez));
        outcomes.push(bluetoothd_plugins(&systemd));
        outcomes.push(wireplumber(&env));
    }
    outcomes
}

/// Whether `org.bluez` answers at all, which every other Bluetooth fact depends on.
///
/// A bus that was never asked is a warning, not a failure: nothing is known to be wrong with
/// the host, only that the question took longer than it was given, and printing the reinstall
/// and group fixes under that would send a reader after a problem that may not exist.
fn bluetooth_service(bluez: &BluezFacts) -> Outcome {
    if let Some(why) = bluez.unreadable.as_deref() {
        return Outcome {
            check: BLUETOOTH_SERVICE,
            state: State::Warn,
            reason: format!("the Bluetooth service could not be read: {why}"),
            fix: None,
        };
    }
    match &bluez.reachable {
        Ok(()) => pass(BLUETOOTH_SERVICE, "org.bluez answers on the system bus"),
        Err(why) => Outcome {
            check: BLUETOOTH_SERVICE,
            state: State::Fail,
            reason: format!(
                "org.bluez does not answer on the system D-Bus ({why}); it is not running, \
                 or this user may not talk to it"
            ),
            fix: Some(Fix {
                command: "sudo systemctl enable --now bluetooth — or add the user to the \
                          bluetooth group and log in again"
                    .into(),
                applies_itself: false,
            }),
        },
    }
}

/// Which adapters BlueZ reports, and whether the one the config asks for is among them and on.
fn adapters(bluez: &BluezFacts, config: Option<&Result<Config, String>>) -> Outcome {
    if let Some(why) = bluez.unreadable.as_deref() {
        return Outcome {
            check: ADAPTERS,
            state: State::Warn,
            reason: format!("the adapters could not be read: {why}"),
            fix: None,
        };
    }
    if bluez.reachable.is_err() {
        return Outcome {
            check: ADAPTERS,
            state: State::Fail,
            reason: "no adapter can be listed until org.bluez answers".into(),
            fix: None,
        };
    }
    if bluez.adapters.is_empty() {
        return Outcome {
            check: ADAPTERS,
            state: State::Fail,
            reason: "no Bluetooth adapter is present".into(),
            fix: None,
        };
    }
    let present = bluez
        .adapters
        .iter()
        .map(|a| format!("{} ({})", a.name, if a.powered { "on" } else { "off" }))
        .collect::<Vec<_>>()
        .join(", ");
    let wanted = match config {
        Some(Ok(config)) => config.adapter.as_deref(),
        _ => None,
    };
    let power_on = || {
        Some(Fix {
            command: "bluetoothctl power on".into(),
            applies_itself: false,
        })
    };
    match wanted {
        Some(name) => match bluez.adapters.iter().find(|a| a.name == name) {
            None => Outcome {
                check: ADAPTERS,
                state: State::Warn,
                reason: format!(
                    "the config names adapter {name:?}, which is not present ({present})"
                ),
                fix: Some(Fix {
                    command: format!(
                        "set adapter = {:?} in config.toml, or clear the key to use the first \
                         adapter",
                        bluez.adapters[0].name
                    ),
                    applies_itself: false,
                }),
            },
            Some(adapter) if !adapter.powered => Outcome {
                check: ADAPTERS,
                state: State::Warn,
                reason: format!("{name} is present but powered off ({present})"),
                fix: power_on(),
            },
            Some(adapter) => pass(
                ADAPTERS,
                format!(
                    "{name} is powered on ({}); present: {present}",
                    adapter.address
                ),
            ),
        },
        None if !bluez.adapters.iter().any(|a| a.powered) => Outcome {
            check: ADAPTERS,
            state: State::Warn,
            reason: format!("no adapter is powered on (present: {present})"),
            fix: power_on(),
        },
        None => pass(ADAPTERS, format!("present: {present}")),
    }
}

/// Where the socket lives, and whether this session has somewhere to put it.
///
/// Only the path the caller actually resolved is judged: a socket named by `SVOLDER_SOCKET` or
/// by the config is where the socket is, whether or not `$XDG_RUNTIME_DIR` is set, and the
/// private `/tmp` fallback is the one case worth a warning.
fn runtime(env: &EnvFacts, socket: &Path) -> Outcome {
    let fallback = PathBuf::from(format!("/tmp/svolder-{}", env.uid));
    let socket_dir = socket.parent().unwrap_or(Path::new("/"));
    if env.socket_dir_writable == Some(false) {
        let elsewhere = if socket_dir == fallback {
            "a path this user can write to".to_string()
        } else {
            format!(
                "a writable path such as {}/svolder.sock",
                fallback.display()
            )
        };
        return Outcome {
            check: RUNTIME,
            state: State::Warn,
            reason: format!(
                "{} is not writable; point SVOLDER_SOCKET at {elsewhere}",
                socket_dir.display()
            ),
            fix: None,
        };
    }
    if env.runtime_dir.is_none() && socket_dir == fallback {
        return Outcome {
            check: RUNTIME,
            state: State::Warn,
            reason: format!(
                "$XDG_RUNTIME_DIR is unset; the socket falls back to {}",
                socket.display()
            ),
            fix: None,
        };
    }
    let runtime_dir = match env.runtime_dir.as_deref() {
        Some(dir) if socket_dir == dir => format!("$XDG_RUNTIME_DIR is {}", dir.display()),
        Some(dir) => format!(
            "$XDG_RUNTIME_DIR is {}, which the socket does not use",
            dir.display()
        ),
        None => "$XDG_RUNTIME_DIR is unset".to_string(),
    };
    pass(
        RUNTIME,
        format!("{runtime_dir}; the socket is {}", socket.display()),
    )
}

/// Whether systemd starts the daemon, and on the socket everything else resolves to.
fn daemon(systemd: &SystemdFacts, socket: &Path) -> Outcome {
    if let Some(why) = systemd.unreadable.as_deref() {
        return Outcome {
            check: DAEMON,
            state: State::Note,
            reason: format!("the units could not be read: {why}"),
            fix: None,
        };
    }
    if !systemd.present {
        return Outcome {
            check: DAEMON,
            state: State::Note,
            reason: "no systemd user session; autospawn or a direct session will serve".into(),
            fix: None,
        };
    }
    let install = || {
        Some(Fix {
            command: DAEMON_INSTALL.into(),
            applies_itself: true,
        })
    };
    let if_installed = |unit: &Option<UnitFacts>| unit.filter(|u| u.installed);
    let (socket_unit, service_unit) = (
        if_installed(&systemd.socket_unit),
        if_installed(&systemd.service_unit),
    );
    if socket_unit.is_none() && service_unit.is_none() {
        return Outcome {
            check: DAEMON,
            state: State::Fail,
            reason: "systemd is here and no svolderd units are installed".into(),
            fix: install(),
        };
    }

    let mut problems = Vec::new();
    match (socket_unit, service_unit) {
        (Some(unit), _) if !unit.enabled => {
            problems.push("svolderd.socket is installed but not enabled".to_string());
        }
        (None, Some(unit)) if !unit.enabled => {
            problems.push("svolderd.service is installed but not enabled".to_string());
        }
        _ => {}
    }
    if let (Some(_), Some(listen)) = (socket_unit, systemd.listen_stream.as_deref())
        && listen != socket
    {
        problems.push(format!(
            "svolderd.socket listens on {}, not the resolved {}",
            listen.display(),
            socket.display()
        ));
    }
    if !problems.is_empty() {
        return Outcome {
            check: DAEMON,
            state: State::Warn,
            reason: problems.join("; "),
            fix: install(),
        };
    }

    let mut installed = Vec::new();
    if let Some(unit) = socket_unit {
        installed.push(format!(
            "svolderd.socket is enabled and {}",
            if unit.active {
                "listening"
            } else {
                "idle until the session starts it"
            }
        ));
    }
    if let Some(unit) = service_unit {
        installed.push(format!(
            "svolderd.service is installed and {}",
            if unit.active { "running" } else { "stopped" }
        ));
    }
    pass(DAEMON, installed.join("; "))
}

/// One row per svolderd of this user's that listens somewhere else than this session reaches.
///
/// A daemon holds the devices of whoever reaches its socket, so one listening on a path
/// nothing here resolves to is holding devices nobody here can see or stop — the row names its
/// pid, its path, and the one command that stops it, whoever started it. The daemon on the
/// resolved socket, and the one the unit listens on, are what the rest of the output is about
/// and so excluded here, as is one whose socket cannot be told (it has no path to name).
///
/// The fix is never run for the reader: stopping a daemon drops the links it holds, and which
/// of them to stop is the reader's to say.
fn other_daemons(daemons: &[RunningDaemon], socket: &Path, listen: Option<&Path>) -> Vec<Outcome> {
    daemons
        .iter()
        .filter_map(|daemon| {
            let told = daemon.elsewhere_than(socket, listen)?;
            Some(Outcome {
                check: OTHER_DAEMONS,
                state: State::Warn,
                reason: format!(
                    "another svolderd (pid {}) is listening on {}",
                    daemon.pid,
                    told.display()
                ),
                fix: Some(Fix {
                    command: format!("svolder --socket {} daemon stop", told.display()),
                    applies_itself: false,
                }),
            })
        })
        .collect()
}

/// Whether the config file reads, and whether what it names exists.
///
/// `profiles_error` is why the user's profile directory could not be loaded: a profile a
/// device names that the built-in set lacks may be in it, so it is not called missing.
fn config(
    config: Option<&Result<Config, String>>,
    profiles: &[String],
    profiles_error: Option<&str>,
) -> Outcome {
    let setup = || {
        Some(Fix {
            command: SETUP.into(),
            applies_itself: true,
        })
    };
    let config = match config {
        None => {
            return Outcome {
                check: CONFIG,
                state: State::Warn,
                reason: "there is no config file yet, so no device is configured".into(),
                fix: setup(),
            };
        }
        Some(Err(message)) => {
            return Outcome {
                check: CONFIG,
                state: State::Fail,
                reason: message.clone(),
                fix: None,
            };
        }
        Some(Ok(config)) => config,
    };
    let named_profile = config
        .devices
        .iter()
        .filter_map(|d| d.profile.as_deref().map(|p| (d.name.as_str(), p)));
    for (device, profile) in named_profile {
        if let Some(why) = profiles_error {
            if !profiles.iter().any(|known| known == profile) {
                return Outcome {
                    check: CONFIG,
                    state: State::Fail,
                    reason: format!(
                        "the profiles could not be loaded from {}; device {device:?} names profile \
                         {profile:?}, which the unreadable directory may hold",
                        first_line(why)
                    ),
                    fix: None,
                };
            }
        } else if !profiles.is_empty() && !profiles.iter().any(|known| known == profile) {
            return Outcome {
                check: CONFIG,
                state: State::Fail,
                reason: format!(
                    "device {device:?} names profile {profile:?}, which does not exist"
                ),
                fix: None,
            };
        }
    }
    if let Some(why) = profiles_error {
        return Outcome {
            check: CONFIG,
            state: State::Fail,
            reason: format!("the profiles could not be loaded from {}", first_line(why)),
            fix: None,
        };
    }
    if config.devices.is_empty() {
        return Outcome {
            check: CONFIG,
            state: State::Warn,
            reason: "no device is configured".into(),
            fix: setup(),
        };
    }
    let names = config
        .devices
        .iter()
        .map(|d| d.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    pass(
        CONFIG,
        format!(
            "{} {} configured: {names}",
            config.devices.len(),
            if config.devices.len() == 1 {
                "device"
            } else {
                "devices"
            }
        ),
    )
}

/// What a container needs from its host: the system bus, an adapter, and no systemd of its own.
fn container(bluez: &BluezFacts, systemd: &SystemdFacts) -> Outcome {
    if let Some(why) = bluez.unreadable.as_deref() {
        return Outcome {
            check: CONTAINER,
            state: State::Warn,
            reason: format!("in a container, and the system bus could not be read: {why}"),
            fix: None,
        };
    }
    // A bus that does not answer is a missing mount; a bus that answers with no adapter behind
    // it is a user the host's bluetoothd will not talk to. The fix names the one that is
    // missing, never both.
    let missing = if bluez.reachable.is_err() {
        Some((
            "org.bluez does not answer, so /run/dbus/system_bus_socket is not mounted",
            "run the container with \
             -v /run/dbus/system_bus_socket:/run/dbus/system_bus_socket:ro",
        ))
    } else if bluez.adapters.is_empty() {
        Some((
            "the system bus answers but no adapter is visible, so this user is not in the \
             host's bluetooth group",
            "run the container with --group-add \"$(getent group bluetooth | cut -d: -f3)\"",
        ))
    } else {
        None
    };
    let systemd_line = if systemd.present {
        "systemd answers here, which a container does not need"
    } else {
        "no systemd here, which is what a container expects"
    };
    match missing {
        None => pass(
            CONTAINER,
            format!(
                "in a container: /run/dbus/system_bus_socket is mounted and an adapter is \
                 visible; {systemd_line}"
            ),
        ),
        Some((reason, command)) => Outcome {
            check: CONTAINER,
            state: State::Warn,
            reason: format!("in a container: {reason}; {systemd_line}"),
            fix: Some(Fix {
                command: command.to_string(),
                applies_itself: false,
            }),
        },
    }
}

/// Whether two adapters can advertise beyond the legacy 31 bytes, which the simulator and the
/// end-to-end script both need.
fn extended_advertising(bluez: &BluezFacts) -> Outcome {
    if let Some(why) = bluez.unreadable.as_deref() {
        return Outcome {
            check: EXTENDED_ADVERTISING,
            state: State::Note,
            reason: format!("the adapters could not be read: {why}"),
            fix: None,
        };
    }
    if bluez.reachable.is_err() {
        return Outcome {
            check: EXTENDED_ADVERTISING,
            state: State::Note,
            reason: "no adapter can be graded until org.bluez answers".into(),
            fix: None,
        };
    }
    let extended = bluez
        .adapters
        .iter()
        .filter(|a| a.max_adv_len.is_some_and(|len| len > 31))
        .count();
    if extended >= 2 {
        return pass(
            EXTENDED_ADVERTISING,
            format!("{extended} adapters advertise beyond 31 bytes"),
        );
    }
    Outcome {
        check: EXTENDED_ADVERTISING,
        state: State::Warn,
        reason: format!(
            "{extended} of {} adapters advertise beyond 31 bytes; the simulator and the \
             end-to-end run need two",
            bluez.adapters.len()
        ),
        fix: None,
    }
}

/// Whether bluetoothd leaves the plugins out that claim the same devices svolder wants.
fn bluetoothd_plugins(systemd: &SystemdFacts) -> Outcome {
    let Some(exec) = systemd.bluetoothd_exec_start.as_deref() else {
        return Outcome {
            check: BLUETOOTHD_PLUGINS,
            state: State::Note,
            reason: "systemd does not report bluetooth.service's ExecStart".into(),
            fix: None,
        };
    };
    let excluded: Vec<&str> = exec
        .split_whitespace()
        .filter_map(|word| word.strip_prefix("--noplugin="))
        .flat_map(|list| list.split(','))
        .collect();
    let wanted: Vec<&str> = NOPLUGIN.split(',').collect();
    let missing: Vec<&str> = wanted
        .iter()
        .copied()
        .filter(|plugin| !excluded.contains(plugin))
        .collect();
    if missing.is_empty() {
        return pass(
            BLUETOOTHD_PLUGINS,
            format!("bluetoothd runs with --noplugin={NOPLUGIN}"),
        );
    }
    // The drop-in keeps whatever else the unit starts bluetoothd with, so the exclusion list is
    // rebuilt from the command systemd reports rather than from a path this crate guesses.
    let kept: Vec<&str> = exec
        .split_whitespace()
        .filter(|word| !word.starts_with("--noplugin="))
        .collect();
    Outcome {
        check: BLUETOOTHD_PLUGINS,
        state: State::Warn,
        reason: format!("bluetoothd loads {}", missing.join(", ")),
        fix: Some(Fix {
            command: format!(
                "sudo install -d /etc/systemd/system/bluetooth.service.d && \
                 printf '[Service]\\nExecStart=\\nExecStart={} --noplugin={NOPLUGIN}\\n' | \
                 sudo tee /etc/systemd/system/bluetooth.service.d/noplugin.conf >/dev/null && \
                 sudo systemctl daemon-reload && sudo systemctl restart bluetooth",
                kept.join(" ")
            ),
            applies_itself: false,
        }),
    }
}

/// Whether WirePlumber's BLE MIDI monitor is switched off, in the form the installed version
/// is configured in.
fn wireplumber(env: &EnvFacts) -> Outcome {
    let Some(version) = env.wireplumber_version.as_deref() else {
        return Outcome {
            check: WIREPLUMBER,
            state: State::Note,
            reason: "WirePlumber is not installed".into(),
            fix: None,
        };
    };
    match env.wireplumber_midi_disabled {
        Some(true) => pass(
            WIREPLUMBER,
            format!("WirePlumber {version} has the BLE MIDI monitor disabled"),
        ),
        None => Outcome {
            check: WIREPLUMBER,
            state: State::Note,
            reason: format!(
                "WirePlumber {version} is installed; its BLE MIDI monitor's state could not be read"
            ),
            fix: None,
        },
        Some(false) if version.starts_with("0.4") => Outcome {
            check: WIREPLUMBER,
            state: State::Warn,
            reason: format!(
                "WirePlumber {version} has the BLE MIDI monitor enabled, and 0.4 is configured \
                 in Lua rather than by the 0.5 drop-in: disable the monitor in \
                 /etc/wireplumber/bluetooth.lua.d/"
            ),
            fix: None,
        },
        Some(false) => Outcome {
            check: WIREPLUMBER,
            state: State::Warn,
            reason: format!("WirePlumber {version} has the BLE MIDI monitor enabled"),
            fix: Some(Fix {
                command: format!(
                    "sudo install -d /etc/wireplumber/wireplumber.conf.d && \
                     printf 'wireplumber.profiles = {{\\n  main = {{\\n    \
                     monitor.bluez-midi = disabled\\n  }}\\n}}\\n' | \
                     sudo tee {WIREPLUMBER_DROP_IN} >/dev/null && \
                     systemctl --user restart wireplumber"
                ),
                applies_itself: false,
            }),
        },
    }
}

/// The first line of a message, for a reason that is read as one line. A TOML parse error
/// carries a whole snippet of the file under its first line, and the first line is the one that
/// names where in the file the error is.
fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or_default().trim_end().into()
}

/// A check that found what it looks for.
fn pass(check: &'static str, reason: impl Into<String>) -> Outcome {
    Outcome {
        check,
        state: State::Pass,
        reason: reason.into(),
        fix: None,
    }
}

/// The exit code a set of outcomes asks for: 2 when anything failed, 1 when anything warned,
/// 0 when every check passed or only recorded a fact.
pub fn exit_code(outcomes: &[Outcome]) -> u8 {
    if outcomes.iter().any(|o| o.state == State::Fail) {
        2
    } else if outcomes.iter().any(|o| o.state == State::Warn) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests;
