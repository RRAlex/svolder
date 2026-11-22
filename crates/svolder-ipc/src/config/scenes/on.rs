// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which scene is on: one pure function over the scenes and what the sound settings read now.
//! It judges a device's part with [`svolder_profiles::scene::matches`] and the computer's with
//! [`ComputerPart::matches`](svolder_host::eq::ComputerPart::matches), so nothing a caller passes
//! can change what "on" means.

use svolder_host::eq::EqView;
use svolder_profiles::Profile;
use svolder_profiles::scene;
use svolder_profiles::tone::preset::same_name;
use svolder_proto::FunctionId;

use super::Scene;

/// One device that is connected now, as far as a scene's part is judged by it.
///
/// A device that is not connected is left out of the list [`on`] takes: it has no readings, and
/// its parts do not count for or against a scene.
pub struct DeviceNow<'a> {
    /// The unit id the device is configured under, as the file spells it; compared without
    /// regard to case.
    pub udid: &'a str,
    /// The device's profile: which controls a scene holds, and how its tone control reads.
    pub profile: &'a Profile,
    /// The raw step the daemon last read for a function of the device, `None` while it has
    /// read none.
    pub raw: &'a dyn Fn(FunctionId) -> Option<i64>,
}

/// What the sound settings read now, as far as [`on`] needs them.
pub struct SceneState<'a> {
    /// The connected devices.
    pub devices: &'a [DeviceNow<'a>],
    /// The computer EQ as it plays now; `None` while the computer's controls are not there,
    /// which leaves a scene's computer part out of the count.
    pub computer: Option<&'a EqView>,
}

/// Which scene is on, and whether that is known yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneOn {
    /// The scene that is on, or `None` when none is, or when it is not known yet.
    pub name: Option<String>,
    /// Whether the answer is settled. False only while an unread scene could still change which
    /// scene is on (it ranks above the first match, or nothing matches yet); `name` is then `None`.
    pub known: bool,
}

/// One scene's verdict.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Every part that counts agrees.
    Yes,
    /// A part that counts does not agree.
    No,
    /// No part disagrees, and a reading is missing.
    Unread,
}

/// Which of `scenes` is on: the one last recalled while it still matches, else the first that
/// matches, else none.
///
/// A scene matches when every part that counts agrees with `state`: the computer's part while
/// the computer's controls are there, and each device's part while that device is connected.
/// The parts of a device that is not connected do not count, so leaving a headphone amp off
/// does not turn every scene off; a scene with no part that counts matches nothing, so a
/// scene is never on by having nobody to disagree. A scene with a part that disagrees is
/// settled without reading the rest.
///
/// The scenes are ranked: the recalled one first, then the file's order. The first that
/// matches is on, and the answer is known unless an unread scene ranks above it, one that
/// could still match and take its place. When no scene matches and any is unread, the answer
/// is not known yet (`known` false, no name). An unread scene ranked below the winner changes
/// nothing, so a device reading its settings after a reconnect does not darken the scenes it
/// has no say in.
///
/// `recalled` is the last scene recalled by name, if any; a name no scene has is ignored.
pub fn on(scenes: &[Scene], state: &SceneState<'_>, recalled: Option<&str>) -> SceneOn {
    let first =
        recalled.and_then(|name| scenes.iter().position(|scene| same_name(&scene.name, name)));
    let ranked = first
        .into_iter()
        .chain((0..scenes.len()).filter(|&at| Some(at) != first));
    for at in ranked {
        match verdict(&scenes[at], state) {
            Verdict::Yes => {
                return SceneOn {
                    name: Some(scenes[at].name.clone()),
                    known: true,
                };
            }
            Verdict::Unread => {
                return SceneOn {
                    name: None,
                    known: false,
                };
            }
            Verdict::No => {}
        }
    }
    SceneOn {
        name: None,
        known: true,
    }
}

/// `scene` against `state`.
fn verdict(scene: &Scene, state: &SceneState<'_>) -> Verdict {
    let mut counted = false;
    let mut unread = false;
    if let (Some(part), Some(view)) = (&scene.computer, state.computer) {
        counted = true;
        if !part.matches(view) {
            return Verdict::No;
        }
    }
    for part in &scene.devices {
        let Some(device) = state
            .devices
            .iter()
            .find(|device| device.udid.eq_ignore_ascii_case(&part.udid))
        else {
            continue;
        };
        counted = true;
        match scene::matches(device.profile, part, device.raw) {
            Some(true) => {}
            Some(false) => return Verdict::No,
            None => unread = true,
        }
    }
    match (counted, unread) {
        (false, _) => Verdict::No,
        (true, true) => Verdict::Unread,
        (true, false) => Verdict::Yes,
    }
}
