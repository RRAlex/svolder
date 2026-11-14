// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The checks on the config file, the container and WirePlumber.

use super::{
    BluezFacts, CONFIG, CONTAINER, EnvFacts, Fix, Outcome, SETUP, State, SystemdFacts, WIREPLUMBER,
    first_line, pass,
};
use crate::config::Config;

/// The drop-in that switches WirePlumber 0.5's BLE MIDI monitor off.
pub(super) const WIREPLUMBER_DROP_IN: &str =
    "/etc/wireplumber/wireplumber.conf.d/90-no-ble-midi.conf";

/// Whether the config file reads, and whether what it names exists.
///
/// `profiles_error` is why the user's profile directory could not be loaded: a profile a
/// device names that the built-in set lacks may be in it, so it is not called missing.
pub(super) fn config(
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
    if let Some(duplicate) = config.dropped_duplicates.first() {
        return Outcome {
            check: CONFIG,
            state: State::Warn,
            reason: duplicate.clone(),
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
pub(super) fn container(bluez: &BluezFacts, systemd: &SystemdFacts) -> Outcome {
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

/// Whether WirePlumber's BLE MIDI monitor is switched off, in the form the installed version
/// is configured in.
pub(super) fn wireplumber(env: &EnvFacts) -> Outcome {
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
