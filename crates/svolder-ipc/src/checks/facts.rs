// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the checks read: the probes trait and the facts a snapshot of the host holds.

use std::path::PathBuf;

use crate::checks::processes::RunningDaemon;

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

    /// The host's Bluetooth rfkill switches, empty when none exist or none could be read.
    fn rfkill(&self) -> Vec<RfkillFacts> {
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

/// One kernel rfkill switch of type `bluetooth`, from `/sys/class/rfkill`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfkillFacts {
    /// The switch's `name` file: an adapter's (`hci0`), or a platform switch's
    /// (`thinkpad_bluetooth`).
    pub name: String,
    /// Blocked by software: `rfkill block`, or a desktop's airplane mode. `rfkill unblock` lifts it.
    pub soft: bool,
    /// Blocked by a hardware switch or the firmware, which no command lifts.
    pub hard: bool,
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
