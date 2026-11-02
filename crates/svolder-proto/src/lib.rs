// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Codec for the Schiit Forkbeard Bluetooth LE protocol.
//!
//! This crate turns bytes into typed messages and back. It performs no I/O.
//! See `docs/protocol.md` in the repository for the wire format.
//!
//! Sending a command: build a [`Message`] and call [`encode_command`].
//! Receiving: pass each RX notification to [`decode_notification`].
//! Discovery: parse the advertisement's manufacturer data with [`ManufacturerData::parse`].
//!
//! Functions in the firmware-update family ([`FunctionClass::Restricted`]) cannot be
//! encoded; every path to command bytes in this crate runs the check; the BLE session
//! must send [`Message`]s through [`encode_command`], never raw bytes.
//!
//! - [`advert`]: parsing the manufacturer data an advertisement carries.
//! - [`mod@checksum`]: the one's-complement checksum that trails every frame.
//! - [`error`]: the one error type every decode, parse and encode check returns.
//! - [`frame`]: turning messages into the bytes of the BLE characteristics, and back.
//! - [`function`]: function addresses and the safety class they belong to.
//! - [`header`]: the header byte every message carries.
//! - [`message`]: the typed form of one command or status.
//! - [`notice`]: the copyright and licence notice svolder's binaries show.
//! - [`passphrase`]: the device passphrase: its digit encoding and its transform.
//! - [`mod@unit`]: the 8-byte device identifier every advertisement carries.
//! - [`value`]: typed message payloads and their conversion to and from wire bytes.
#![warn(missing_docs)]

pub mod advert;
pub mod checksum;
pub mod error;
pub mod frame;
pub mod function;
pub mod header;
pub mod message;
pub mod notice;
pub mod passphrase;
#[cfg(test)]
pub(crate) mod test_support;
pub mod unit;
pub mod value;

pub use advert::{
    ADVERT_COMPANY_ID, FORKBEARD_RX_UUID, FORKBEARD_SERVICE_UUID, FORKBEARD_TX_UUID,
    MANUFACTURER_DATA_LEN, ManufacturerData, product_name,
};
pub use checksum::checksum;
pub use error::ProtoError;
pub use frame::{
    DecodedCommand, Notification, decode_command, decode_notification, encode_command,
    encode_notification,
};
pub use function::{FunctionClass, FunctionId};
pub use header::{Direction, Header, ValueType};
pub use message::Message;
pub use passphrase::Passphrase;
pub use unit::UnitId;
pub use value::Value;
