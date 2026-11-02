// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Parsing of the manufacturer-specific payload carried in a BLE advertisement.

use crate::error::ProtoError;
use crate::passphrase::Passphrase;
use crate::unit::UnitId;

/// Length of the manufacturer-specific payload after the 2-byte company id.
pub const MANUFACTURER_DATA_LEN: usize = 27;

/// The Forkbeard GATT service, as a 128-bit integer. Central and peripheral code build
/// their own UUID type from it, so this crate needs no UUID dependency.
pub const FORKBEARD_SERVICE_UUID: u128 = 0x2DA00001_9F4C_4A75_BDEC_FCC5C2877BBF;
/// The GATT characteristic a central writes command frames to, as a 128-bit integer.
pub const FORKBEARD_TX_UUID: u128 = 0x2DA00002_9F4C_4A75_BDEC_FCC5C2877BBF;
/// The GATT characteristic a device notifies status frames on, as a 128-bit integer.
pub const FORKBEARD_RX_UUID: u128 = 0x2DA00003_9F4C_4A75_BDEC_FCC5C2877BBF;

/// Company id under which svolder's own simulator and fake advertise the 27-byte payload,
/// and the one read first when several entries parse. The id carries no meaning, so any
/// value round-trips; 0xFFFF is the Bluetooth SIG id reserved for testing.
pub const ADVERT_COMPANY_ID: u16 = 0xFFFF;

/// What a Forkbeard device says about itself in every advertisement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManufacturerData {
    /// Identifies the product model; see [`product_name`].
    pub product_id: u16,
    /// Set while a firmware update is in progress or was interrupted.
    pub incomplete_dfu: bool,
    /// The device's stable identifier.
    pub unit_id: UnitId,
    /// The passphrase field as advertised; recover it with [`ManufacturerData::passphrase`].
    pub encrypted_passphrase: u32,
    /// Zero when the device is in its bootloader waiting for firmware.
    pub firmware_build: u16,
    /// Hardware revision of the Forkbeard module.
    pub module_hardware: u8,
    /// Firmware build of the Forkbeard module.
    pub module_firmware: u8,
}

impl ManufacturerData {
    /// Parse the manufacturer-specific payload. `data` excludes the 2-byte company id.
    pub fn parse(data: &[u8]) -> Result<Self, ProtoError> {
        if data.len() != MANUFACTURER_DATA_LEN {
            return Err(ProtoError::BadAdvertLen(data.len()));
        }
        let u16_at = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
        let u32_at =
            |i: usize| u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        Ok(Self {
            product_id: u16_at(0),
            incomplete_dfu: data[5] & 0x01 != 0,
            unit_id: UnitId::from_slice(&data[6..14])?,
            encrypted_passphrase: u32_at(14),
            firmware_build: u16_at(18),
            module_hardware: data[20],
            module_firmware: data[21],
        })
    }

    /// The 27-byte manufacturer payload (excludes the 2-byte company id). Inverse of `parse`.
    pub fn to_bytes(&self) -> [u8; MANUFACTURER_DATA_LEN] {
        let mut out = [0u8; MANUFACTURER_DATA_LEN];
        out[0..2].copy_from_slice(&self.product_id.to_le_bytes());
        if self.incomplete_dfu {
            out[5] |= 0x01;
        }
        out[6..14].copy_from_slice(&self.unit_id.bytes());
        out[14..18].copy_from_slice(&self.encrypted_passphrase.to_le_bytes());
        out[18..20].copy_from_slice(&self.firmware_build.to_le_bytes());
        out[20] = self.module_hardware;
        out[21] = self.module_firmware;
        out
    }

    /// The device's current passphrase, recovered from the advertisement.
    pub fn passphrase(&self) -> Passphrase {
        Passphrase::from_advert(self.encrypted_passphrase, self.unit_id)
    }

    /// True when the device must not be connected to (firmware update in progress or pending).
    pub fn in_bootloader(&self) -> bool {
        self.firmware_build == 0 || self.incomplete_dfu
    }

    /// Marketing name for this device's product id, if known.
    pub fn product_name(&self) -> Option<&'static str> {
        product_name(self.product_id)
    }
}

/// The marketing name of a product id.
pub fn product_name(product_id: u16) -> Option<&'static str> {
    Some(match product_id {
        1 => "Kara",
        2 => "Gungnir 2",
        3 => "Jord",
        4 => "Wotan",
        5 => "Vidar 2",
        6 => "Aegir 2",
        7 => "Kraken",
        8 => "Yggdrasil",
        9 => "Urd",
        10 => "Bifrost 3",
        11 => "Mimir",
        12 => "Saga 2",
        13 => "Stjarna",
        14 => "Lyr 5",
        15 => "Skoll",
        16 => "Forkbeard Module",
        17 => "Gjallarhorn",
        18 => "Tyr",
        19 => "Freya",
        20 => "Mesh Card",
        21 => "Eitr 2",
        22 => "Mjolnir X",
        23 => "Loki Max",
        24 => "Freya 2",
        27 => "Hati",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::hex;

    fn sample() -> Vec<u8> {
        hex("02000000000001020304050607088a53f037030001070000000000")
    }

    #[test]
    fn parses_sample_advertisement() {
        let d = ManufacturerData::parse(&sample()).unwrap();
        assert_eq!(
            d,
            ManufacturerData {
                product_id: 2,
                incomplete_dfu: false,
                unit_id: UnitId::new([1, 2, 3, 4, 5, 6, 7, 8]),
                encrypted_passphrase: 0x37F0538A,
                firmware_build: 3,
                module_hardware: 1,
                module_firmware: 7,
            }
        );
        assert_eq!(d.passphrase().digits(), [1, 2, 3, 4]);
        assert_eq!(d.product_name(), Some("Gungnir 2"));
        assert!(!d.in_bootloader());
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            ManufacturerData::parse(&[0; 26]),
            Err(ProtoError::BadAdvertLen(26))
        );
        assert_eq!(
            ManufacturerData::parse(&[0; 28]),
            Err(ProtoError::BadAdvertLen(28))
        );
    }

    #[test]
    fn detects_bootloader_by_zero_firmware_or_flag() {
        let mut bytes = sample();
        bytes[18] = 0;
        bytes[19] = 0;
        assert!(ManufacturerData::parse(&bytes).unwrap().in_bootloader());

        let mut bytes = sample();
        bytes[5] = 0x01;
        let d = ManufacturerData::parse(&bytes).unwrap();
        assert!(d.incomplete_dfu);
        assert!(d.in_bootloader());
    }

    #[test]
    fn zero_passphrase_field_means_none() {
        let mut bytes = sample();
        bytes[14..18].copy_from_slice(&[0, 0, 0, 0]);
        assert!(
            ManufacturerData::parse(&bytes)
                .unwrap()
                .passphrase()
                .is_none()
        );
    }

    #[test]
    fn to_bytes_round_trips_through_parse() {
        let bytes = hex("02000000000001020304050607088a53f037030001070000000000");
        let parsed = ManufacturerData::parse(&bytes).unwrap();
        assert_eq!(parsed.to_bytes().len(), MANUFACTURER_DATA_LEN);
        assert_eq!(ManufacturerData::parse(&parsed.to_bytes()).unwrap(), parsed);
    }

    #[test]
    fn to_bytes_round_trips_the_incomplete_dfu_flag() {
        let mut bytes = hex("02000000000001020304050607088a53f037030001070000000000");
        bytes[5] = 0x01;
        let parsed = ManufacturerData::parse(&bytes).unwrap();
        assert!(parsed.incomplete_dfu);
        let out = parsed.to_bytes();
        assert_eq!(out[5] & 0x01, 0x01);
        assert_eq!(ManufacturerData::parse(&out).unwrap(), parsed);
    }

    #[test]
    fn names_every_known_product() {
        let expected = [
            (1, "Kara"),
            (2, "Gungnir 2"),
            (3, "Jord"),
            (4, "Wotan"),
            (5, "Vidar 2"),
            (6, "Aegir 2"),
            (7, "Kraken"),
            (8, "Yggdrasil"),
            (9, "Urd"),
            (10, "Bifrost 3"),
            (11, "Mimir"),
            (12, "Saga 2"),
            (13, "Stjarna"),
            (14, "Lyr 5"),
            (15, "Skoll"),
            (16, "Forkbeard Module"),
            (17, "Gjallarhorn"),
            (18, "Tyr"),
            (19, "Freya"),
            (20, "Mesh Card"),
            (21, "Eitr 2"),
            (22, "Mjolnir X"),
            (23, "Loki Max"),
            (24, "Freya 2"),
            (27, "Hati"),
        ];
        for (id, name) in expected {
            assert_eq!(product_name(id), Some(name), "{id}");
        }
        assert_eq!(product_name(0), None);
        assert_eq!(product_name(25), None);
        assert_eq!(product_name(26), None);
    }
}
