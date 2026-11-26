// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the loop does for the Bluetooth adapter list: the short scan that fills it, and what
//! picking an adapter writes and reopens.

use super::devices::off_loop;
use super::*;
use crate::commands::adapters::{self as survey, Survey};

/// Wait for the survey on `rx`, while one is in flight; the `select!` arm reading this is
/// guarded to never poll it with none.
pub(super) async fn recv(
    rx: &mut Option<tokio::sync::oneshot::Receiver<Result<Survey, String>>>,
) -> Option<Result<Survey, String>> {
    match rx {
        Some(receiver) => receiver.await.ok(),
        None => std::future::pending().await,
    }
}

/// Start the scan the open list waits for: every adapter listens at once for
/// [`survey::WINDOW`], in a task of its own, so the loop goes on drawing and reading keys. The
/// answer comes back through [`recv`].
pub(super) fn list<O: Open>(opener: &Opener<O>, watching: &mut Watching) {
    // A list reopened while its scan still runs waits for that scan rather than starting another.
    if watching.adapters_scan.is_some() {
        return;
    }
    let globals = opener.globals.clone();
    let path = opener.path.clone();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    // The devices connected now, which stop advertising and so cannot be heard: the list shows
    // them connected, with the last signal known.
    let mut held = survey::held_from(&watching.app.devices.rows, now_ms);
    let session = &watching.app.session;
    if held.is_empty() && session.connected && !session.device_name.is_empty() {
        held.push(survey::Held {
            device: session.device_name.clone(),
            rssi: None,
            age_s: None,
        });
    }
    let daemon = watching.backend.as_ref().and_then(Backend::daemon_socket);
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        // The daemon's own answer, since it opens the adapter its command line or the config
        // names and keeps it; what this process was started with decides nothing under one.
        let holds = match &daemon {
            Some(socket) => svolder_ipc::socket::daemon_version::version_within(
                socket,
                svolder_ipc::socket::daemon_version::VERSION_WAIT,
            )
            .await
            .and_then(|info| info.adapter),
            None => None,
        };
        let via = match daemon.is_some() {
            true => survey::Via::Daemon {
                holds: holds.as_deref(),
            },
            false => survey::Via::Direct,
        };
        let found = match Config::load_from(&path) {
            Ok(config) => {
                survey::survey_rows(&globals.borrow(), &config, survey::WINDOW, &held, via)
                    .await
                    .map_err(|failure| failure.to_string())
            }
            Err(e) => Err(e.to_string()),
        };
        let _ = tx.send(found);
    });
    watching.adapters_scan = Some(rx);
}

/// What the toast says of the pick `name` (a kernel name) once the daemon answered with `held`, what
/// it says of itself after the reload: only a daemon that holds the picked adapter switched to it.
fn daemon_took(
    name: &str,
    held: Option<&svolder_ipc::socket::daemon_version::DaemonInfo>,
) -> String {
    let saved = format!("adapter saved: {name}");
    let Some(held) = held else {
        return format!("{saved}; the daemon did not say which adapter it holds");
    };
    match (held.adapter.as_deref(), held.adapter_pinned) {
        (Some(now), true) => {
            format!("{saved}; the daemon is pinned to {now} by --adapter or SVOLDER_ADAPTER")
        }
        (None, true) => format!("{saved}; the daemon is pinned by --adapter or SVOLDER_ADAPTER"),
        (Some(now), false) if now == name => format!("{saved}; the daemon switched to it"),
        (Some(now), false) => {
            format!("{saved}; the daemon stays on {now}: it could not use it (see its log)")
        }
        (None, false) => format!("{saved}; the daemon did not say which adapter it holds"),
    }
}

/// Save `address` as the adapter svolder connects through, through the writer `add` uses (the
/// file, then a daemon told to reload), and make it the adapter the devices' state remembers, so
/// the remembered one does not outrank the pick.
///
/// A direct session reconnects through it at once. A daemon follows the saved adapter on its
/// reload unless its own command line (`--adapter` or `SVOLDER_ADAPTER`) pins it, and the toast
/// says which of those happened.
pub(super) async fn pick<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    address: String,
    name: String,
    look: &Look,
    screen: &mut dyn Shown,
) -> Switched {
    let path = opener.path.clone();
    let saved = address.clone();
    let wrote = off_loop(
        "saving…",
        move || Config::set_adapter(&path, Some(&saved)),
        watching,
        look,
        screen,
    )
    .await;
    if !written(opener, watching, wrote).await {
        return Switched::Stayed;
    }
    watching.app.overlays.conn.adapter = Some(address.clone());
    let units: Vec<String> = Config::load_from(&opener.path)
        .map(|config| {
            config
                .devices
                .into_iter()
                .map(|device| device.udid)
                .collect()
        })
        .unwrap_or_default();
    // The address, never the kernel number: the number moves, and a remembered one would outrank
    // the config and send the next session to whichever dongle has it then.
    let kept = address.clone();
    let home = look.token_site.state_home.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let mut state = crate::state::DeviceState::load_from(&home);
        let mut changed = false;
        for unit in &units {
            let profile = state.last_profile(unit);
            changed |= state.remember(unit, &kept, profile.as_deref(), Some(&kept));
        }
        if changed {
            let _ = state.save_to(&home);
        }
    })
    .await;
    let overridden = match opener.globals.adapter.as_deref() {
        Some(named) => format!("; --adapter {named} still overrides it in this session"),
        None => String::new(),
    };
    if let Some(socket) = watching.backend.as_ref().and_then(Backend::daemon_socket) {
        // The daemon follows a changed `adapter` on its reload, which `written` has just asked
        // for; one that does not is told to restart, since it reads the key only when it starts.
        // What it says it holds afterwards is what the toast reports: a daemon pinned by its
        // command line, or one that could not open the adapter, stays where it was. Its own
        // command line is the only one that counts, so this session's `--adapter` adds nothing.
        let said = match crate::reach::daemon_runs(
            &socket,
            svolder_ipc::FEATURE_ADAPTER_SWITCH,
            "switch its adapter",
        )
        .await
        {
            Ok(true) => {
                let held = svolder_ipc::socket::daemon_version::version_within(
                    &socket,
                    svolder_ipc::socket::daemon_version::VERSION_WAIT,
                )
                .await;
                daemon_took(&name, held.as_ref())
            }
            Ok(false) => {
                format!("adapter saved: {name}; no daemon answered, it takes it at its start")
            }
            Err(_) => format!(
                "adapter saved: {name}; this daemon is too old to switch: svolder daemon restart"
            ),
        };
        watching.app.notify(said);
        return Switched::Stayed;
    }
    watching
        .app
        .notify(format!("adapter saved: {name}{overridden}"));
    let device = watching.app.session.device_name.clone();
    let device = (!device.is_empty()).then_some(device);
    switch(opener, watching, device.as_deref(), look, screen).await
}
