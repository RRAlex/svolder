// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saving the sound settings as a scene: the computer's part and one part for every device that
//! is connected, written as a `[[scene]]` block through the daemon's config writer.

use std::sync::Arc;

use svolder_ipc::scenes::Scene;
use svolder_ipc::socket::protocol::{PartOutcome, PartStatus, Reason};
use svolder_ipc::{CODE_USAGE, Config, ConfigError, Response};
use svolder_profiles::scene::{self, DevicePart};
use svolder_profiles::tone::preset::Preset;

use super::view::raw_of;
use super::words;
use crate::device::DeviceHandle;
use crate::handler::{ConfigEdit, ConfigWriteError, Daemon};

/// A refusal in `words` with `reason`.
pub(super) fn refuse(words: impl Into<String>, reason: Reason) -> Response {
    Response::error(CODE_USAGE, words).with_reason(reason)
}

/// Save the settings as they are now under `name`; with `force`, over the scene of that name.
///
/// Refused `busy`, in words, while a connected device's settings are unread or its writer runs or
/// holds a recall or a setting. A device that is not connected is not in the scene.
pub(crate) async fn save(daemon: &Daemon, name: String, force: bool) -> Response {
    let held = match daemon.config_for_write("scenes").await {
        Ok(held) => held,
        Err(why) => return why.answer(),
    };
    let own = held.scene(&name).map(|existing| existing.name.clone());
    if let (Some(own), false) = (&own, force) {
        return ConfigWriteError::Refused(ConfigError::Taken {
            kind: "scene",
            name: own.clone(),
        })
        .answer();
    }
    let devices = match capture_devices(daemon, &held).await {
        Ok(parts) => parts,
        Err(refused) => return *refused,
    };
    let computer = match daemon.host().scene_eq() {
        Some(_) => daemon.host().scene_part().await.ok(),
        None => None,
    };
    if computer.is_none() && devices.is_empty() {
        return refuse(words::NOTHING_TO_SAVE, Reason::NothingToSave);
    }
    let written = own.clone().unwrap_or_else(|| name.trim().to_string());
    let scene_devices: Vec<String> = devices.iter().map(|part| part.udid.clone()).collect();
    let scene = Scene {
        name: written.clone(),
        computer,
        devices,
    };
    let edit: ConfigEdit = match own {
        Some(own) => Box::new(move |path| Config::update_scene(path, &own, &scene)),
        None => Box::new(move |path| Config::add_scene(path, &scene)),
    };
    if let Err(why) = daemon.edit_then_reload(edit).await {
        return why.answer();
    }
    daemon.scenes().changed(daemon);
    // Each configured device that is not connected is named: it is not in the scene.
    let kept: Vec<&str> = scene_devices.iter().map(String::as_str).collect();
    let outcome = held
        .devices
        .iter()
        .filter(|device| {
            !kept
                .iter()
                .any(|udid| udid.eq_ignore_ascii_case(&device.udid))
        })
        .map(|device| PartOutcome {
            who: device.name.clone(),
            status: PartStatus::Left,
            notes: vec![words::not_in_scene(&device.name)],
        })
        .collect();
    Response::Scene {
        name: written,
        outcome,
    }
}

/// The part of every connected device that has been seen, in the daemon's device order; a
/// refusal when one is unread or its writer is busy.
async fn capture_devices(
    daemon: &Daemon,
    config: &Config,
) -> Result<Vec<DevicePart>, Box<Response>> {
    let mut parts = Vec::new();
    for device in daemon.devices() {
        let feed = device.feed.borrow().clone();
        let (true, Some(profile)) = (feed.connected, feed.profile.clone()) else {
            continue;
        };
        if *device.writer.busy().borrow() {
            let why = words::still_applying(&device.name);
            return Err(Box::new(refuse(why, Reason::Busy)));
        }
        let fitting = fitting_of(config, &profile);
        if let Some(eq) = profile.eq.as_ref() {
            crate::recall::warm(eq, &fitting).await;
        }
        let recalled = device.writer.recalled().borrow().clone();
        let part = scene::capture(
            &profile,
            &device.udid,
            &device.name,
            &fitting,
            recalled.as_deref(),
            |id| raw_of(&feed.readings, id),
        );
        match part {
            Ok(part) => parts.push(part),
            Err(unread) => {
                let why = words::still_reading(&device.name, &profile, &unread.what);
                return Err(Box::new(refuse(why, Reason::Busy)));
            }
        }
    }
    Ok(parts)
}

/// The presets that fit `profile`'s tone control: its built-ins and the saved ones that fit.
pub(super) fn fitting_of(config: &Config, profile: &svolder_profiles::Profile) -> Vec<Preset> {
    config
        .fitting_presets(profile)
        .into_iter()
        .map(|(preset, _)| preset)
        .collect()
}

/// Whether `device` is connected and has been seen: the only kind whose part a scene saves or
/// recalls.
pub(super) fn usable(device: &Arc<DeviceHandle>) -> bool {
    let feed = device.feed.borrow();
    feed.connected && feed.profile.is_some()
}
