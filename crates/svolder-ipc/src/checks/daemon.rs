// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The checks on where the daemon's socket lives and which daemons run.

use std::path::{Path, PathBuf};

use super::{
    DAEMON, DAEMON_INSTALL, EnvFacts, Fix, OTHER_DAEMONS, Outcome, RUNTIME, State, SystemdFacts,
    UnitFacts, pass,
};
use crate::checks::processes::RunningDaemon;

/// Where the socket lives, and whether this session has somewhere to put it.
///
/// Only the path the caller actually resolved is judged: a socket named by `SVOLDER_SOCKET` or
/// by the config is where the socket is, whether or not `$XDG_RUNTIME_DIR` is set, and the
/// private `/tmp` fallback is the one case worth a warning.
pub(super) fn runtime(env: &EnvFacts, socket: &Path) -> Outcome {
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
pub(super) fn daemon(systemd: &SystemdFacts, socket: &Path) -> Outcome {
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
pub(super) fn other_daemons(
    daemons: &[RunningDaemon],
    socket: &Path,
    listen: Option<&Path>,
) -> Vec<Outcome> {
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
