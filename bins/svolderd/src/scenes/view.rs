// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The [`ScenesView`] from what the daemon holds: the scenes of the config, which parts of each
//! can be recalled now, and which scene is on.

use std::sync::Arc;

use svolder_host::eq::EqView;
use svolder_ipc::Config;
use svolder_ipc::scenes::{DeviceNow, SceneState, on};
use svolder_ipc::socket::protocol::{PartRow, SceneRow, ScenesView};
use svolder_profiles::Profile;
use svolder_profiles::tone::preset::same_name;
use svolder_proto::{FunctionId, Message};

use crate::device::DeviceHandle;

/// What the view needs of one device, taken from its feed in one look.
struct Seen {
    udid: String,
    connected: bool,
    profile: Option<Arc<Profile>>,
    readings: Vec<Message>,
}

/// The raw step `readings` hold for function `id`: a toggle's boolean reads as 0 or 1
/// ([`svolder_profiles::scene::raw_of`], the one reader a scene is judged through).
pub(super) fn raw_of(readings: &[Message], id: FunctionId) -> Option<i64> {
    svolder_profiles::scene::raw_of(readings)(id)
}

/// The view of `config`'s scenes against `devices` as they read now and `eq`, the computer's EQ
/// while its controls can be used, with `recalled` the scene last recalled.
///
/// Only a device that is connected and has been seen counts toward which scene is on: a reading
/// kept from before a disconnect never does. `recalled` shows as the scene's name in the config
/// (a name no scene has is left out).
pub(super) fn of(
    config: &Config,
    devices: &[Arc<DeviceHandle>],
    eq: Option<&EqView>,
    recalled: Option<&str>,
) -> ScenesView {
    let seen: Vec<Seen> = devices
        .iter()
        .map(|device| {
            let feed = device.feed.borrow();
            Seen {
                udid: device.udid.clone(),
                connected: feed.connected,
                profile: feed.profile.clone(),
                readings: feed.readings.clone(),
            }
        })
        .collect();
    let reads: Vec<Box<dyn Fn(FunctionId) -> Option<i64> + '_>> = seen
        .iter()
        .map(|device| {
            Box::new(|id| raw_of(&device.readings, id)) as Box<dyn Fn(FunctionId) -> Option<i64>>
        })
        .collect();
    let now: Vec<DeviceNow<'_>> = seen
        .iter()
        .zip(&reads)
        .filter(|(device, _)| device.connected)
        .filter_map(|(device, raw)| {
            Some(DeviceNow {
                udid: &device.udid,
                profile: device.profile.as_deref()?,
                raw: &**raw,
            })
        })
        .collect();
    let state = SceneState {
        devices: &now,
        computer: eq,
    };
    let verdict = on(&config.scenes, &state, recalled);
    let scenes = config
        .scenes
        .iter()
        .map(|scene| {
            let mut names = scene.part_names(&config.devices).into_iter();
            let computer = scene.computer.as_ref().and_then(|_| {
                names.next().map(|who| PartRow {
                    who,
                    present: eq.is_some(),
                })
            });
            let devices = scene.devices.iter().zip(names).map(|(part, who)| PartRow {
                who,
                present: seen
                    .iter()
                    .any(|device| device.connected && device.udid.eq_ignore_ascii_case(&part.udid)),
            });
            SceneRow {
                name: scene.name.clone(),
                parts: computer.into_iter().chain(devices).collect(),
            }
        })
        .collect();
    ScenesView {
        scenes,
        on: verdict.name,
        recalled: recalled.and_then(|name| {
            config
                .scenes
                .iter()
                .find(|scene| same_name(&scene.name, name))
                .map(|scene| scene.name.clone())
        }),
        known: verdict.known,
    }
}
