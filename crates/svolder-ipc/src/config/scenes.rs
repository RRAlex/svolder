// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[[scene]]` blocks: a named set of sound settings, the computer's and each device's, as
//! the file keeps it; the writers that add, update, rename and remove one; and [`on`], which
//! says which scene the sound settings of the moment are.
//!
//! A scene is a snapshot of values: a preset renamed, edited or deleted later never changes it.
//! A block reads:
//!
//! ```toml
//! [[scene]]
//! name = "Late night"
//!
//! [scene.computer]
//! eq_on = true
//! name = "Warm cans"
//! preamp = -4.5
//! filter = [
//!   { type = "PK", f = 100.0, g = 3.0, q = 1.0 },
//! ]
//! loudness = { on = true, strength_pct = 50.0, reference_pct = 100.0 }
//! crossfeed_on = false
//! speakers_on = false
//!
//! [[scene.device]]
//! udid = "0102030405060708"
//! name = "Bifrost 3"
//! controls = [
//!   { key = "input", step = 1 },
//! ]
//!
//! [scene.device.tone]
//! name = "Warm"
//! mode = "eq"
//! loudness = 0
//! band = [
//!   { f = 63.0, g = 1.0, q = 0.7 },
//! ]
//! ```
//!
//! - `read`: the checked reading of the blocks, every error naming the scene.
//! - `write`: the writers.
//! - `on`: which scene is on.

use svolder_host::eq::ComputerPart;
use svolder_profiles::scene::DevicePart;

mod on;
mod read;
mod write;

pub use on::{DeviceNow, SceneOn, SceneState, on};
pub(super) use read::read_scenes;

/// One scene: a name, the computer's part and one part for each device that was connected when
/// it was saved.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Scene {
    /// The name a user picks it by, matched without regard to case and the blanks round it.
    pub name: String,
    /// What the computer plays; absent for a scene saved with no computer controls.
    pub computer: Option<ComputerPart>,
    /// One part per device, in the file's order.
    pub devices: Vec<DevicePart>,
}

/// What the computer's part of a scene is called wherever the parts are listed.
pub const COMPUTER_PART: &str = "computer";

impl Scene {
    /// The name of each part, in the order they are listed: `computer` first when the scene has
    /// a computer part, then each device's part by the name the configuration gives the device
    /// whose unit id it is (case aside), else by the name the part was saved under.
    pub fn part_names(&self, devices: &[super::DeviceConfig]) -> Vec<String> {
        let computer = self.computer.as_ref().map(|_| COMPUTER_PART.to_string());
        let named = self.devices.iter().map(|part| {
            devices
                .iter()
                .find(|device| device.udid.eq_ignore_ascii_case(&part.udid))
                .map_or_else(|| part.name.clone(), |device| device.name.clone())
        });
        computer.into_iter().chain(named).collect()
    }
}
