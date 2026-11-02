// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The BlueZ (btleplug) central transport and its helpers: the Forkbeard service and
//! characteristic UUIDs, decoding a scanned device's manufacturer data into an
//! `Advertisement`, and mapping btleplug errors to `BleError`.
//!
//! - `releases`: the radio work kept for callers that may stop waiting for it, per device.
//! - [`transport`]: the BlueZ central transport: scan, connect, write TX frames, stream RX.

use std::collections::HashMap;

use btleplug::api::PeripheralProperties;
use uuid::Uuid;

use crate::advert::{Advertisement, ManufacturerRecord};
use crate::error::BleError;
use svolder_proto::{ADVERT_COMPANY_ID, ManufacturerData};

mod releases;
pub mod transport;

pub use transport::BluezTransport;

/// The Forkbeard GATT service UUID.
pub const SERVICE_UUID: Uuid = Uuid::from_u128(svolder_proto::FORKBEARD_SERVICE_UUID);
/// The characteristic the host writes commands to.
pub const TX_UUID: Uuid = Uuid::from_u128(svolder_proto::FORKBEARD_TX_UUID);
/// The characteristic the device notifies status frames on.
pub const RX_UUID: Uuid = Uuid::from_u128(svolder_proto::FORKBEARD_RX_UUID);

/// Whether a scanned device's advertised services make it a Forkbeard: it offers the
/// Forkbeard GATT service.
pub fn is_forkbeard(services: &[Uuid]) -> bool {
    services.contains(&SERVICE_UUID)
}

/// The Forkbeard advertisement a scanned device's properties carry: it offers the Forkbeard
/// service and its manufacturer data parses. `None` for any other device.
pub fn forkbeard_advertisement(props: &PeripheralProperties) -> Option<Advertisement> {
    if !is_forkbeard(&props.services) {
        return None;
    }
    advertisement_from_manufacturer_data(&props.manufacturer_data, props.rssi)
}

/// Build an `Advertisement` from a scanned device's manufacturer data: the 27-byte record
/// under any company id (the protocol gives the id no meaning), the one under
/// `ADVERT_COMPANY_ID` first, else the lowest id whose payload parses. `None` when no entry
/// parses. The service UUID decides what is a Forkbeard (`forkbeard_advertisement`); this
/// only finds the record.
pub fn advertisement_from_manufacturer_data(
    manufacturer_data: &HashMap<u16, Vec<u8>>,
    rssi: Option<i16>,
) -> Option<Advertisement> {
    let mut ids: Vec<u16> = manufacturer_data.keys().copied().collect();
    ids.sort_unstable_by_key(|id| (*id != ADVERT_COMPANY_ID, *id));
    let (company_id, manufacturer) = ids.iter().find_map(|id| {
        ManufacturerData::parse(&manufacturer_data[id])
            .ok()
            .map(|parsed| (*id, parsed))
    })?;
    Some(Advertisement {
        manufacturer,
        rssi,
        record: Some(ManufacturerRecord {
            company_id,
            bytes: manufacturer_data[&company_id].clone(),
        }),
    })
}

/// Map a btleplug error to the transport's `BleError`.
pub fn map_btleplug_error(err: &btleplug::Error) -> BleError {
    use btleplug::Error::*;
    match err {
        PermissionDenied | NoAdapterAvailable => BleError::Adapter(err.to_string()),
        DeviceNotFound => BleError::NotFound,
        _ => BleError::Connection(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::hex;
    use std::collections::HashMap;

    /// The 27-byte manufacturer payload: product 2, unit 0102030405060708, passphrase 1234,
    /// fw 3.
    fn payload() -> Vec<u8> {
        hex("02000000000001020304050607088a53f037030001070000000000")
    }

    /// The record keeps the bytes the parse drops (2..5 and 22..27) and its company id.
    #[test]
    fn the_heard_record_keeps_the_bytes_svolder_does_not_parse() {
        let mut bytes = payload();
        bytes[2..5].copy_from_slice(&[0xAA, 0xBB, 0xCC]);
        bytes[22..27].copy_from_slice(&[1, 2, 3, 4, 5]);
        let mut m = HashMap::new();
        m.insert(0x1234_u16, bytes.clone());
        let a = advertisement_from_manufacturer_data(&m, None).expect("advert");
        assert_eq!(
            a.record,
            Some(ManufacturerRecord {
                company_id: 0x1234,
                bytes
            })
        );
    }

    #[test]
    fn picks_the_27_byte_entry_and_parses_it() {
        let mut m = HashMap::new();
        m.insert(0x004C_u16, vec![1, 2, 3]); // some other vendor, wrong length
        m.insert(ADVERT_COMPANY_ID, payload()); // the Forkbeard payload
        let a = advertisement_from_manufacturer_data(&m, Some(-50)).expect("advert");
        assert_eq!(a.product_name(), Some("Gungnir 2"));
        assert_eq!(a.unit_id().to_string(), "0102030405060708");
        assert_eq!(a.rssi, Some(-50));
    }

    /// The company id carries no meaning: a 27-byte record under any id is read.
    #[test]
    fn a_27_byte_payload_under_another_company_id_is_read() {
        let mut m = HashMap::new();
        m.insert(0x004C_u16, payload());
        let a = advertisement_from_manufacturer_data(&m, None).expect("read under 0x004C");
        assert_eq!(a.unit_id().to_string(), "0102030405060708");
    }

    /// Two entries that both parse: the one under `ADVERT_COMPANY_ID` wins, whatever the order.
    #[test]
    fn the_simulators_company_id_is_read_first() {
        let mut other = payload();
        other[6] ^= 0xFF; // a different unit id (bytes 6..14 of the record)
        let mut m = HashMap::new();
        m.insert(0x0001_u16, other);
        m.insert(ADVERT_COMPANY_ID, payload());
        let a = advertisement_from_manufacturer_data(&m, None).expect("read");
        assert_eq!(a.unit_id().to_string(), "0102030405060708");
    }

    #[test]
    fn no_forkbeard_entry_yields_none() {
        let mut m = HashMap::new();
        m.insert(0x004C_u16, vec![9, 9, 9]);
        assert!(advertisement_from_manufacturer_data(&m, None).is_none());
        m.insert(ADVERT_COMPANY_ID, vec![9, 9, 9]); // right company id, too short to parse
        assert!(advertisement_from_manufacturer_data(&m, None).is_none());
    }

    #[test]
    fn a_device_without_the_service_uuid_is_not_a_forkbeard() {
        let other = Uuid::from_u128(0x0000_1801_0000_1000_8000_0080_5f9b_34fb);
        assert!(is_forkbeard(&[SERVICE_UUID]));
        assert!(is_forkbeard(&[other, SERVICE_UUID]));
        assert!(!is_forkbeard(&[other]));
        assert!(!is_forkbeard(&[]));
    }

    /// A scanned device's properties, carrying whichever of the two things a Forkbeard
    /// advertises the case under test needs.
    fn props(
        services: Vec<Uuid>,
        manufacturer_data: HashMap<u16, Vec<u8>>,
    ) -> PeripheralProperties {
        PeripheralProperties {
            services,
            manufacturer_data,
            rssi: Some(-60),
            ..Default::default()
        }
    }

    #[test]
    fn a_devices_properties_carry_an_advertisement_only_with_both_halves() {
        let mut m = HashMap::new();
        m.insert(ADVERT_COMPANY_ID, payload());
        let a = forkbeard_advertisement(&props(vec![SERVICE_UUID], m.clone())).expect("advert");
        assert_eq!(a.unit_id().to_string(), "0102030405060708");
        assert_eq!(a.rssi, Some(-60));
        // The service without the payload, and the payload without the service.
        assert!(forkbeard_advertisement(&props(vec![SERVICE_UUID], HashMap::new())).is_none());
        assert!(forkbeard_advertisement(&props(Vec::new(), m)).is_none());
    }

    #[test]
    fn uuids_have_the_expected_text() {
        assert_eq!(
            SERVICE_UUID.to_string(),
            "2da00001-9f4c-4a75-bdec-fcc5c2877bbf"
        );
        assert_eq!(TX_UUID.to_string(), "2da00002-9f4c-4a75-bdec-fcc5c2877bbf");
        assert_eq!(RX_UUID.to_string(), "2da00003-9f4c-4a75-bdec-fcc5c2877bbf");
    }

    #[test]
    fn maps_permission_denied_to_adapter() {
        let e = map_btleplug_error(&btleplug::Error::PermissionDenied);
        assert!(matches!(e, BleError::Adapter(_)));
        let e = map_btleplug_error(&btleplug::Error::DeviceNotFound);
        assert!(matches!(e, BleError::NotFound));
    }

    #[test]
    fn maps_no_adapter_available_to_adapter() {
        let e = map_btleplug_error(&btleplug::Error::NoAdapterAvailable);
        assert!(matches!(e, BleError::Adapter(_)));
    }
}
