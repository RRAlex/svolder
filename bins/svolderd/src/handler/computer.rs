// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's volume, mute and EQ as requests: the answers of `host_volume`, `host_mute` and
//! `host_eq`, and the two things a recall on the computer needs from the devices the daemon
//! holds: which of them play the same preset, and flattening them when the user said yes.

use svolder_ipc::{HostEqOp, Response};
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
        false => svolder_ipc::CODE_USAGE,
    };
    Response::error(code, message)
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
            HostEqOp::SaveAs { name } => match self.save_computer_curve(&name).await {
                Ok(()) => Response::Ack,
                Err(refused) => refused,
            },
            HostEqOp::Preset { ref device, .. } => {
                let presets = match self.recallable(device.as_deref()).await {
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
    async fn recallable(&self, device: Option<&str>) -> Result<Vec<Preset>, String> {
        let mut presets = self.config_snapshot().presets.clone();
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
    #[allow(
        clippy::result_large_err,
        reason = "the refusal is the answer the socket sends as it is"
    )]
    async fn save_computer_curve(&self, name: &str) -> Result<(), Response> {
        let view = self.host.view_now().await.eq.unwrap_or_default();
        let filters = view
            .points
            .iter()
            .map(|point| {
                let kind = point
                    .kind
                    .parse::<FilterKind>()
                    .map_err(|e| refuse(e.to_string()))?;
                Ok(SourceFilter {
                    kind,
                    f: point.f,
                    g: point.g,
                    q: point.q,
                })
            })
            .collect::<Result<Vec<_>, Response>>()?;
        let correction = Correction {
            preamp_db: view.preamp_db,
            filters,
        };
        self.save_preset(name, preset::from_computer(name, correction), false)
            .await
            .map(drop)
            .map_err(|why| why.answer())
    }
}

/// Function `id`'s raw reading among `readings`.
fn raw_of(readings: &[svolder_proto::Message], id: FunctionId) -> Option<i64> {
    readings
        .iter()
        .find(|reading| reading.id == id)
        .and_then(|reading| reading.value.as_i64())
}

#[cfg(test)]
mod tests {
    use svolder_ipc::{Reason, Response};

    use crate::handler::tests::daemon_over;

    fn dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("svolderd-computer-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mine() -> String {
        "schema = 1\n[[preset]]\nname = \"Mine\"\nmode = \"eq\"\nbands = [0, 0, 0, 0, 0]\n"
            .to_string()
    }

    /// The words and reason of a refused answer.
    fn refused(answer: Response) -> (String, Option<Reason>) {
        match answer {
            Response::Error {
                message, reason, ..
            } => (message, reason),
            other => panic!("not a refusal: {other:?}"),
        }
    }

    /// The socket's `SaveAs` refuses a taken name and a read-only file in the words it always
    /// had, each with its reason, and judges the file before the name.
    #[tokio::test]
    async fn the_save_as_refusals_keep_their_words_and_order() {
        use std::os::unix::fs::PermissionsExt;
        let dir = dir("refusals");
        let path = dir.join("config.toml");
        std::fs::write(&path, mine()).unwrap();
        let daemon = daemon_over(path.clone(), dir.join("profiles")).shared();
        let (words, reason) = refused(daemon.save_computer_curve("mine").await.unwrap_err());
        assert_eq!(words, "config: a preset named \"Mine\" already exists");
        assert_eq!(reason, Some(Reason::Taken));
        let (words, reason) = refused(daemon.save_computer_curve("  ").await.unwrap_err());
        assert!(!words.is_empty());
        assert_eq!(reason, Some(Reason::BadName));

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        let off = format!(
            "Saving presets is off: {} is read-only (0444)",
            path.display()
        );
        let (words, reason) = refused(daemon.save_computer_curve("Other").await.unwrap_err());
        assert_eq!((words, reason), (off.clone(), Some(Reason::ReadOnly)));
        let (words, reason) = refused(daemon.save_computer_curve("mine").await.unwrap_err());
        assert_eq!(
            (words, reason),
            (off, Some(Reason::ReadOnly)),
            "the file is judged before the name"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
