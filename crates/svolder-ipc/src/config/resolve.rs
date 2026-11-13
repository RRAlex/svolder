// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a `[[device]]` entry resolves to once a device advertises: its profile and the code its
//! frames are encoded with. The daemon, the direct sessions and the commands all ask here, so a
//! configured name and `auto` mean one thing everywhere.

use svolder_profiles::{Profile, Registry, apply_input_labels};
use svolder_proto::{ManufacturerData, Passphrase, ProtoError};

use super::DeviceConfig;

/// The profile for a device advertising `product_id` at `firmware`, with nothing configured: the
/// registry's pick, else a generic profile for the product.
pub fn profile_for_advert(registry: &Registry, product_id: u16, firmware: u16) -> Profile {
    registry
        .select(product_id, firmware)
        .cloned()
        .unwrap_or_else(|| Registry::generic(product_id))
}

/// The code a `passphrase` setting stands for against what the device advertises: `auto` is the
/// advertised code, anything else is `none` or four digits, or the error saying it is neither.
/// A device with no entry has the setting `auto`.
pub fn passphrase_from(setting: &str, advert: &ManufacturerData) -> Result<Passphrase, ProtoError> {
    match setting {
        "auto" => Ok(advert.passphrase()),
        text => Passphrase::from_text(text),
    }
}

impl DeviceConfig {
    /// The profile for this device when it advertises `product_id` at `firmware`: the profile
    /// the entry names (a name the registry does not know is passed over), else
    /// [`profile_for_advert`], with the entry's input names laid over it.
    pub fn profile_for(&self, registry: &Registry, product_id: u16, firmware: u16) -> Profile {
        let mut profile = self
            .profile
            .as_deref()
            .and_then(|name| registry.get(name).cloned())
            .unwrap_or_else(|| profile_for_advert(registry, product_id, firmware));
        apply_input_labels(&mut profile, &self.inputs);
        profile
    }

    /// The code to encode this device's frames with: the advertised one for `auto`, else the
    /// configured `none` or four digits, or why the entry's text is no code.
    pub fn passphrase_for(&self, advert: &ManufacturerData) -> Result<Passphrase, ProtoError> {
        passphrase_from(&self.passphrase, advert)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn entry(profile: Option<&str>, passphrase: &str) -> DeviceConfig {
        DeviceConfig {
            name: "dac".into(),
            udid: "0102030405060708".into(),
            profile: profile.map(str::to_string),
            passphrase: passphrase.into(),
            inputs: BTreeMap::new(),
        }
    }

    fn advert(product_id: u16) -> ManufacturerData {
        ManufacturerData {
            product_id,
            incomplete_dfu: false,
            unit_id: svolder_proto::UnitId::new([1, 2, 3, 4, 5, 6, 7, 8]),
            encrypted_passphrase: 0x37F0_538A,
            firmware_build: 3,
            module_hardware: 1,
            module_firmware: 7,
        }
    }

    #[test]
    fn a_named_profile_wins_over_the_registry_pick() {
        let registry = Registry::builtin().unwrap();
        let picked = profile_for_advert(&registry, 2, 3);
        let other = registry
            .names()
            .into_iter()
            .find(|name| *name != picked.name)
            .unwrap()
            .to_string();
        let named = entry(Some(&other), "auto").profile_for(&registry, 2, 3);
        assert_eq!(named.name, other);
        assert_eq!(entry(None, "auto").profile_for(&registry, 2, 3), picked);
    }

    #[test]
    fn an_unknown_name_falls_back_to_the_pick_and_an_unknown_product_to_generic() {
        let registry = Registry::builtin().unwrap();
        let picked = profile_for_advert(&registry, 2, 3);
        assert_eq!(
            entry(Some("no-such"), "auto").profile_for(&registry, 2, 3),
            picked
        );
        assert_eq!(
            entry(None, "auto").profile_for(&registry, 60000, 1).name,
            "generic-60000"
        );
    }

    #[test]
    fn the_entrys_input_names_are_laid_over_the_profile() {
        let registry = Registry::builtin().unwrap();
        let name = registry
            .names()
            .into_iter()
            .find(|name| {
                let profile = registry.get(name).unwrap();
                profile.product_id.is_some() && profile.controls.iter().any(|c| c.key == "input")
            })
            .expect("a built-in profile with an input control");
        let profile = registry.get(name).unwrap();
        let mut device = entry(Some(name), "auto");
        device.inputs.insert(0, "Turntable".into());
        let laid = device.profile_for(&registry, profile.product_id.unwrap(), 3);
        let control = laid.controls.iter().find(|c| c.key == "input").unwrap();
        assert!(control.values.iter().any(|(_, name)| name == "Turntable"));
    }

    #[test]
    fn auto_follows_the_advertisement_and_the_rest_is_the_text() {
        let advert = advert(2);
        assert_eq!(
            entry(None, "auto").passphrase_for(&advert).unwrap(),
            advert.passphrase()
        );
        assert_eq!(
            entry(None, "1234").passphrase_for(&advert).unwrap(),
            Passphrase::from_text("1234").unwrap()
        );
        assert!(
            entry(None, "none")
                .passphrase_for(&advert)
                .unwrap()
                .is_none()
        );
        assert!(entry(None, "12x4").passphrase_for(&advert).is_err());
    }
}
