// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The configured devices as requests: the `device_config` family ([`DeviceConfigOp`]).
//!
//! Placement rule: everything of this family lives here. [`Daemon`] routes
//! `Request::DeviceConfig` to it in one line of `handle_from`; the web and MCP reach it through
//! `Fleet::request` only.
//!
//! While the daemon runs it is the one writer of the `[[device]]` blocks and of
//! `default_device`. Each write goes through [`Daemon::edit_then_reload`]: the core's writer
//! edits the file the daemon reads, under the file's lock, and the daemon reloads it, so the
//! actors follow the file. A changed passphrase or profile reaches the running actor on its
//! link; a removed device has its actor stopped; a renamed one is stopped and started under its
//! new name. The answer is the configured devices as the daemon holds them after the write. A
//! reload that outlasts the write's wait goes on after the answer, so a device just added may
//! not be among `devices` yet.
//!
//! Every answer also lists the profiles a device can be pinned to ([`offered_profiles`]). A
//! write is judged against that same list, read the same way, so an editor never offers a
//! profile a write refuses, and no write takes one the list leaves out.
//!
//! An op names its device by configured name or by whole unit id, neither case-sensitive. The
//! core's writer finds the device and writes it under the file's one lock
//! ([`Config::patch_device`] and its siblings), against the file as it is then and not against
//! the daemon's snapshot: a hand edit made since the last reload is neither lost nor needed to
//! be reloaded first, and two writers at once, the CLI's among them, take turns. Every refusal
//! reaches the socket through [`ConfigWriteError::answer`].

use std::path::Path;

use svolder_ipc::config::DeviceConfig;
use svolder_ipc::{
    Config, ConfigError, ConfiguredDevice, DeviceConfigOp, PassphraseKind, ProfileChoice, Response,
};
use svolder_profiles::Registry;

use super::{ConfigEdit, ConfigWriteError, Daemon};

/// What a refused write calls itself: `Saving devices is off: …`.
const WHAT: &str = "devices";

impl Daemon {
    /// Answer one `device_config` op.
    pub(super) async fn device_config(&self, op: DeviceConfigOp) -> Response {
        let profiles = self.profiles_dir.clone();
        let edit: ConfigEdit = match op {
            DeviceConfigOp::List => return self.configured_devices().await,
            DeviceConfigOp::Add {
                unit,
                name,
                profile,
                passphrase,
            } => Box::new(move |path| {
                known_profile(&profiles, profile.as_deref())?;
                let device = DeviceConfig {
                    name,
                    udid: unit,
                    profile,
                    // Left empty, the writer leaves the key out, which reads `auto`.
                    passphrase: passphrase.unwrap_or_default(),
                    inputs: Default::default(),
                };
                Config::add_device(path, &device)
            }),
            DeviceConfigOp::Update {
                device,
                name,
                profile,
                passphrase,
            } => Box::new(move |path| {
                if let Some(profile) = &profile {
                    known_profile(&profiles, profile.as_deref())?;
                }
                Config::patch_device(path, &device, |current| {
                    let mut next = current.clone();
                    if let Some(name) = name {
                        next.name = name;
                    }
                    if let Some(profile) = profile {
                        next.profile = profile;
                    }
                    if let Some(passphrase) = passphrase {
                        next.passphrase = passphrase;
                    }
                    Ok(next)
                })
            }),
            DeviceConfigOp::Remove { device } => {
                Box::new(move |path| Config::remove_selected_device(path, &device))
            }
            DeviceConfigOp::SetDefault { device } => {
                Box::new(move |path| Config::default_to_device(path, &device))
            }
            DeviceConfigOp::ClearDefault => {
                Box::new(move |path| Config::set_default_device(path, None))
            }
        };
        match self.write_devices(edit).await {
            Ok(()) => self.configured_devices().await,
            Err(why) => why.answer(),
        }
    }

    /// Make `edit` and reload, once the file is known to be writable; a daemon that is stopping
    /// refuses in `edit_then_reload`, as it does every configuration write.
    async fn write_devices(&self, edit: ConfigEdit) -> Result<(), ConfigWriteError> {
        self.config_for_write(WHAT).await?;
        self.edit_then_reload(edit).await
    }

    /// The configured devices as the snapshot names them, each with where its link stands, and
    /// the profiles a write takes now. Profiles that cannot be read are an empty list, as every
    /// pin is refused then.
    async fn configured_devices(&self) -> Response {
        let dir = self.profiles_dir.clone();
        let profiles = tokio::task::spawn_blocking(move || offered_profiles(&dir))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        let config = self.config_snapshot();
        let running = self.devices();
        let devices = config
            .devices
            .iter()
            .map(|device| {
                let actor = running
                    .iter()
                    .find(|handle| handle.name.eq_ignore_ascii_case(&device.name));
                let connected =
                    actor.is_some_and(|handle| crate::locks::locked(&handle.info).connected);
                ConfiguredDevice {
                    unit: device.udid.clone(),
                    name: device.name.clone(),
                    profile: device.profile.clone(),
                    passphrase: PassphraseKind::of(&device.passphrase),
                    default: config
                        .default_device
                        .as_deref()
                        .is_some_and(|name| name.eq_ignore_ascii_case(&device.name)),
                    connected,
                    not_connected_since_ms: actor
                        .filter(|_| !connected)
                        .and_then(|handle| handle.lost_at_ms()),
                }
            })
            .collect();
        Response::DeviceConfig { devices, profiles }
    }
}

/// The profiles a device can be pinned to, by ascending name: those the registry would know
/// after a reload now, the built-in ones and the user's in `dir`, read now, so a profile file
/// added since the last reload counts. It is the one list both an answer's `profiles` and
/// [`known_profile`] go by. A profile's `display` is given where it says more than the name.
fn offered_profiles(dir: &Path) -> Result<Vec<ProfileChoice>, ConfigError> {
    let registry = Registry::with_user_dir(dir)
        .map_err(|why| ConfigError::Parse(format!("{}: {why}", dir.display())))?;
    Ok(registry
        .names()
        .into_iter()
        .filter_map(|name| Some((name, registry.get(name)?)))
        .map(|(name, profile)| ProfileChoice {
            name: name.to_string(),
            display: Some(profile.display.clone()).filter(|display| display != name),
            base: profile.product_id.is_none(),
        })
        .collect())
}

/// Refuse a profile [`offered_profiles`] does not list.
fn known_profile(dir: &Path, profile: Option<&str>) -> Result<(), ConfigError> {
    let Some(name) = profile else {
        return Ok(());
    };
    match offered_profiles(dir)?
        .iter()
        .any(|choice| choice.name == name)
    {
        true => Ok(()),
        false => Err(ConfigError::Parse(format!("no profile is named {name:?}"))),
    }
}

#[cfg(test)]
mod tests;
