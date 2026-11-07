// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's volume, mute and EQ as requests: the answers of `host_volume`, `host_mute` and
//! `host_eq`, and the two things a recall on the computer needs from the devices the daemon
//! holds: which of them play the same preset, and flattening them when the user said yes.

use svolder_ipc::{Config, HostEqOp, Response};
use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{self, Preset};
use svolder_proto::FunctionId;

use super::Daemon;
use crate::host::EqRefusal;
use crate::recall;
use crate::writer::Origin;

/// The built-ins a computer recall is looked up in when no device is shown, and no device is
/// there to say: the Gungnir 2's five-band tone control.
const DEFAULT_TONE_PROFILE: &str = "mesh-eq-5";

fn refuse(message: impl Into<String>) -> Response {
    let message = message.into();
    let code = match crate::host::is_unavailable(&message) {
        true => svolder_ipc::CODE_COMPUTER_UNAVAILABLE,
        false => 1,
    };
    Response::Error { code, message }
}

fn answer(result: Result<(), impl Into<EqRefusal>>) -> Response {
    match result.map_err(Into::into) {
        Ok(()) => Response::Ack,
        Err(EqRefusal::Words(message)) => refuse(message),
        Err(both @ EqRefusal::Both { .. }) => {
            let message = both.to_string();
            let EqRefusal::Both { name, devices } = both else {
                unreachable!("matched above")
            };
            Response::Ask {
                message,
                name,
                devices,
            }
        }
    }
}

impl Daemon {
    /// `host_volume`: an acknowledgement once the sink took the level and read it back.
    pub(super) async fn host_volume(&self, pct: u8) -> Response {
        answer(self.host.host_volume(pct).await.map(drop))
    }

    /// `host_mute`: an acknowledgement once the sink took it.
    pub(super) async fn host_mute(&self, muted: bool) -> Response {
        answer(self.host.host_mute(muted).await.map(drop))
    }

    /// `host_eq`: an acknowledgement, or the reason in words.
    pub(super) async fn host_eq(&self, origin: Origin, op: HostEqOp) -> Response {
        match op {
            HostEqOp::SaveAs { name } => answer(self.save_computer_curve(&name).await),
            HostEqOp::Preset { ref device, .. } => {
                let presets = match self.recallable(device.as_deref()) {
                    Ok(presets) => presets,
                    Err(why) => return refuse(why),
                };
                let holders = |name: &str| self.devices_playing(&presets, name);
                let flatten = |names: Vec<String>| self.flatten_devices(origin, names);
                answer(self.host.host_eq(op, &presets, holders, flatten).await)
            }
            op => answer(
                self.host
                    .host_eq(op, &[], |_| Vec::new(), |_| async { Ok(()) })
                    .await,
            ),
        }
    }

    /// The presets a recall on the computer may name: the saved ones, and the built-ins of the
    /// shown device's own tone control.
    fn recallable(&self, device: Option<&str>) -> Result<Vec<Preset>, String> {
        let config = Config::load_from(&self.config_path)
            .map_err(|_| "the configuration could not be read; see the daemon's log".to_string())?;
        let mut presets = config.presets.clone();
        // A device that is named must be there; with none named the default stands in.
        let shown = match device {
            Some(_) => Some(self.resolve(device).map_err(|refused| refused.message)?),
            None => self.resolve(None).ok(),
        };
        let eq = shown
            .and_then(|device| device.feed.borrow().profile.clone())
            .and_then(|profile| profile.eq.clone())
            .or_else(|| {
                crate::locks::read(&self.registry)
                    .get(DEFAULT_TONE_PROFILE)
                    .and_then(|profile| profile.eq.clone())
            });
        if let Some(eq) = eq {
            presets.extend(preset::builtins(&eq));
        }
        Ok(presets)
    }

    /// The configured names of the connected devices whose tone control plays the preset called
    /// `name` among `presets`.
    fn devices_playing(&self, presets: &[Preset], name: &str) -> Vec<String> {
        let Some(preset) = presets
            .iter()
            .find(|preset| preset::same_name(&preset.name, name))
        else {
            return Vec::new();
        };
        self.devices()
            .iter()
            .filter(|device| {
                let feed = device.feed.borrow();
                let Some(eq) = feed
                    .profile
                    .as_ref()
                    .and_then(|profile| profile.eq.as_ref())
                else {
                    return false;
                };
                feed.connected && preset::matches(eq, |id| raw_of(&feed.readings, id), preset)
            })
            .map(|device| device.name.clone())
            .collect()
    }

    /// Put every band of each of the devices called `names` on Flat's values, as the user asked
    /// when they said yes to the question; the first write that fails ends it.
    async fn flatten_devices(&self, origin: Origin, names: Vec<String>) -> Result<(), String> {
        for name in names {
            let device = self.resolve(Some(&name)).map_err(|e| e.message)?;
            let commands = {
                let feed = device.feed.borrow();
                let (Some(profile), true) = (feed.profile.as_ref(), feed.connected) else {
                    continue;
                };
                let Some(eq) = profile.eq.as_ref() else {
                    continue;
                };
                preset::flatten(eq, |id| raw_of(&feed.readings, id)).commands(profile)
            };
            let total = commands.len();
            for (sent, command) in commands.iter().enumerate() {
                if let Response::Error { message, .. } = self
                    .write(origin, Some(&name), recall::command(command))
                    .await
                {
                    return Err(format!(
                        "could not flatten {name}: {}",
                        recall::stopped_after(sent, total, &message)
                    ));
                }
            }
        }
        Ok(())
    }

    /// Save the computer EQ's points as a preset of the user's under `name`, then have the
    /// daemon read its configuration again.
    async fn save_computer_curve(&self, name: &str) -> Result<(), String> {
        let view = self.host.view_now().await.eq.unwrap_or_default();
        let filters = view
            .points
            .iter()
            .map(|point| {
                let kind = point
                    .kind
                    .parse::<FilterKind>()
                    .map_err(|e| e.to_string())?;
                Ok(SourceFilter {
                    kind,
                    f: point.f,
                    g: point.g,
                    q: point.q,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let correction = Correction {
            preamp_db: view.preamp_db,
            filters,
        };
        Config::writable(&self.config_path)
            .map_err(|why| format!("Saving presets is off: {why}"))?;
        let config = Config::load_from(&self.config_path)
            .map_err(|_| "the configuration could not be read; see the daemon's log".to_string())?;
        let names = config.presets.iter().map(|p| p.name.as_str());
        let wanted = preset::new_name(name, names, None)
            .map_err(|why| why.to_string())?
            .to_string();
        let saved = preset::from_computer(&wanted, correction);
        Config::add_preset(&self.config_path, &saved).map_err(|e| e.to_string())?;
        self.reload_all().await.map(drop)
    }
}

/// Function `id`'s raw reading among `readings`.
fn raw_of(readings: &[svolder_proto::Message], id: FunctionId) -> Option<i64> {
    readings
        .iter()
        .find(|reading| reading.id == id)
        .and_then(|reading| reading.value.as_i64())
}
