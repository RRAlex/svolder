// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Bluetooth checks: the service, the adapters, extended advertising and bluetoothd's plugins.

use super::{
    ADAPTERS, AdapterFacts, BLUETOOTH_SERVICE, BLUETOOTHD_PLUGINS, BluezFacts,
    EXTENDED_ADVERTISING, Fix, Outcome, RfkillFacts, State, SystemdFacts, pass,
};
use crate::config::Config;

/// How a blocked radio is blocked; a hard block outranks a soft one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Soft,
    Hard,
}

/// Whether an rfkill switch name is an adapter's own (`hci0`); any other name is a platform
/// switch (`thinkpad_bluetooth`, `dell-bluetooth`).
fn is_adapter_name(name: &str) -> bool {
    name.strip_prefix("hci")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The block on the radio of adapter `name`, `None` when its switch is not blocked or it has
/// none; a hard block outranks a soft one. A switch not named for an adapter is a platform
/// switch and blocks every adapter.
fn block_of(rfkill: &[RfkillFacts], name: &str) -> Option<Block> {
    let mine = || {
        rfkill
            .iter()
            .filter(|r| r.name == name || !is_adapter_name(&r.name))
    };
    if mine().any(|r| r.hard) {
        Some(Block::Hard)
    } else if mine().any(|r| r.soft) {
        Some(Block::Soft)
    } else {
        None
    }
}

/// The plugins bluetoothd must not load for a BLE peripheral of this project's to be reachable:
/// each one claims services or connections svolder and the simulator need for themselves.
pub(super) const NOPLUGIN: &str = "midi,vcp,micp,bass,csip,mcp,ccp";

/// Whether `org.bluez` answers at all, which every other Bluetooth fact depends on.
///
/// A bus that was never asked is a warning, not a failure: nothing is known to be wrong with
/// the host, only that the question took longer than it was given, and printing the reinstall
/// and group fixes under that would send a reader after a problem that may not exist.
pub(super) fn bluetooth_service(bluez: &BluezFacts) -> Outcome {
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
        Err(why) if not_on_the_bus(why) => Outcome {
            check: BLUETOOTH_SERVICE,
            state: State::Fail,
            reason:
                "BlueZ is not on the system bus: the bluetooth service is not running, or bluez is \
                     not installed (install bluez, then start the service)"
                    .into(),
            fix: Some(Fix {
                command: "sudo systemctl unmask bluetooth && sudo systemctl enable --now bluetooth".into(),
                applies_itself: false,
            }),
        },
        Err(why) => Outcome {
            check: BLUETOOTH_SERVICE,
            state: State::Fail,
            reason: format!(
                "org.bluez does not answer on the system D-Bus ({why}); it is not running, \
                 or this user may not talk to it"
            ),
            fix: Some(Fix {
                command: "sudo systemctl unmask bluetooth && sudo systemctl enable --now bluetooth — or add the user to the \
                          bluetooth group and log in again"
                    .into(),
                applies_itself: false,
            }),
        },
    }
}

/// The D-Bus error names that say nobody owns `org.bluez` and the bus could not start it: no
/// owner, no service file, or systemd has no (or a masked) unit to activate.
const NOT_ON_THE_BUS: [&str; 4] = [
    "org.freedesktop.DBus.Error.ServiceUnknown",
    "org.freedesktop.DBus.Error.NameHasNoOwner",
    "org.freedesktop.systemd1.NoSuchUnit",
    "org.freedesktop.systemd1.UnitMasked",
];

/// Whether the bus error, written `<error name>: <message>` by the probe, is one of
/// [`NOT_ON_THE_BUS`]: the name decides, never the message's words.
fn not_on_the_bus(why: &str) -> bool {
    let name = why.split_once(": ").map_or(why, |(name, _)| name);
    NOT_ON_THE_BUS.contains(&name)
}

/// The outcome for adapters `names` that are off while their radio is blocked.
fn blocked_adapters(block: Block, names: &str, present: &str) -> Outcome {
    let (what, fix) = match block {
        Block::Soft => (
            "blocked by rfkill (soft)",
            Some(Fix {
                command: "sudo rfkill unblock bluetooth".into(),
                applies_itself: false,
            }),
        ),
        Block::Hard => (
            "hard-blocked by rfkill: a hardware switch or the firmware blocks it, and no \
             command lifts it",
            None,
        ),
    };
    Outcome {
        check: ADAPTERS,
        state: State::Warn,
        reason: format!("{names} is {what} ({present})"),
        fix,
    }
}

/// Which adapters BlueZ reports, and whether the one the config asks for is among them and on.
pub(super) fn adapters(
    bluez: &BluezFacts,
    config: Option<&Result<Config, String>>,
    rfkill: &[RfkillFacts],
) -> Outcome {
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
    // What stands between the powered-off adapters in `off` and being on: nothing, so BlueZ is
    // asked (`bluetoothctl power on`), else the software blocks of their own switches, else only
    // hardware blocks. A block on another adapter's switch is none of this one's business.
    let power_fix = |off: &[&AdapterFacts]| -> Outcome {
        let blocks: Vec<Option<Block>> = off.iter().map(|a| block_of(rfkill, &a.name)).collect();
        let names = off
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if blocks.iter().any(Option::is_none) {
            Outcome {
                check: ADAPTERS,
                state: State::Warn,
                reason: if wanted.is_some() {
                    format!("{names} is present but powered off ({present})")
                } else {
                    format!("no adapter is powered on (present: {present})")
                },
                fix: power_on(),
            }
        } else if blocks.contains(&Some(Block::Soft)) {
            blocked_adapters(Block::Soft, &names, &present)
        } else {
            blocked_adapters(Block::Hard, &names, &present)
        }
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
            Some(adapter) if !adapter.powered => power_fix(&[adapter]),
            Some(adapter) => pass(
                ADAPTERS,
                format!(
                    "{name} is powered on ({}); present: {present}",
                    adapter.address
                ),
            ),
        },
        None if !bluez.adapters.iter().any(|a| a.powered) => {
            power_fix(&bluez.adapters.iter().collect::<Vec<_>>())
        }
        None => pass(ADAPTERS, format!("present: {present}")),
    }
}

/// Whether two adapters can advertise beyond the legacy 31 bytes, which the simulator and the
/// end-to-end script both need.
pub(super) fn extended_advertising(bluez: &BluezFacts) -> Outcome {
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
pub(super) fn bluetoothd_plugins(systemd: &SystemdFacts) -> Outcome {
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
