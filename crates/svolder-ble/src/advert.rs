// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A parsed Forkbeard advertisement: who the device is and how strong its signal was.

use svolder_proto::{ManufacturerData, UnitId};

/// The manufacturer record as the radio heard it: what `ManufacturerData` parses, with the
/// bytes it does not parse kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManufacturerRecord {
    /// The company id the record was sent under.
    pub company_id: u16,
    /// The record's payload, after the company id.
    pub bytes: Vec<u8>,
}

/// One advertisement seen during a scan.
#[derive(Clone, Debug)]
pub struct Advertisement {
    /// The manufacturer-specific payload, already parsed.
    pub manufacturer: ManufacturerData,
    /// Received signal strength in dBm, when the transport reports it.
    pub rssi: Option<i16>,
    /// The record the radio delivered, unparsed; `None` for an advertisement built in code
    /// rather than heard.
    pub record: Option<ManufacturerRecord>,
}

impl Advertisement {
    /// The advertising device's stable identifier.
    pub fn unit_id(&self) -> UnitId {
        self.manufacturer.unit_id
    }

    /// The marketing name for the product id, when known.
    pub fn product_name(&self) -> Option<&'static str> {
        self.manufacturer.product_name()
    }

    /// Say that the device now advertises `passphrase`, as it will from its next advertisement
    /// on once a `250` command has changed its code: the held copy follows what the device holds.
    pub fn set_advertised_passphrase(&mut self, passphrase: svolder_proto::Passphrase) {
        self.manufacturer.encrypted_passphrase = match passphrase.is_none() {
            true => 0,
            false => svolder_proto::Passphrase::encrypt(
                passphrase.encoded(),
                svolder_proto::Passphrase::advert_key(self.manufacturer.unit_id),
            ),
        };
    }

    /// The advertised firmware build; `0` means the device is in its bootloader.
    pub fn firmware_build(&self) -> u16 {
        self.manufacturer.firmware_build
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::hex;

    /// 27-byte manufacturer payload: product 2, unit 0102030405060708, passphrase 1234, fw 3.
    fn gungnir_advert() -> Advertisement {
        let bytes = hex("02000000000001020304050607088a53f037030001070000000000");
        Advertisement {
            manufacturer: ManufacturerData::parse(&bytes).unwrap(),
            rssi: Some(-52),
            record: None,
        }
    }

    #[test]
    fn reads_identity_from_manufacturer_data() {
        let a = gungnir_advert();
        assert_eq!(a.product_name(), Some("Gungnir 2"));
        assert_eq!(a.firmware_build(), 3);
        assert_eq!(a.unit_id().to_string(), "0102030405060708");
        assert_eq!(a.rssi, Some(-52));
    }
}
