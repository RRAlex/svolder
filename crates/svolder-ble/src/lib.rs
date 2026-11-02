// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bluetooth LE transport and per-device session for Schiit Forkbeard products.
//!
//! `Transport` and `Link` abstract the radio. `Session` runs the protocol's connect, refresh,
//! set-and-confirm and reconnect behaviour on one link, folding every incoming status into
//! a `DeviceState`. Two transports implement the abstraction: `BluezTransport` drives a
//! real BlueZ adapter over btleplug, and `FakeTransport` an in-process
//! `svolder_fake::FakeDevice`, so the whole stack is testable without hardware.
//!
//! - [`advert`]: a parsed Forkbeard advertisement.
//! - [`bluez`]: the BlueZ central transport and its helpers.
//! - [`episode`]: what a device on `auto` that refuses its code is given.
//! - [`error`]: the error type every transport and session operation returns.
//! - [`fake`]: an in-process transport backed by a fake device.
//! - `log`: where the crate's own diagnostic lines go.
//! - `power`: whether a device that has a power state is on, followed from its statuses.
//! - [`session`]: one device's live session: connect, keep state fresh, set values, reconnect.
//! - [`state`]: the last confirmed value of every control.
//! - [`tap`]: a transport decorator that records every frame sent and received.
//! - [`transport`]: the transport abstraction: scan and connect, then read and write on a link.
#![warn(missing_docs)]

pub mod advert;
pub mod bluez;
pub mod episode;
pub mod error;
pub mod fake;
mod log;
mod power;
pub mod session;
pub mod state;
pub mod tap;
#[cfg(test)]
pub(crate) mod test_support;
pub mod transport;

pub use advert::{Advertisement, ManufacturerRecord};
pub use bluez::transport::BluezTransport;
pub use bluez::{RX_UUID, SERVICE_UUID, TX_UUID};
pub use episode::{Episode, listen_afresh};
pub use error::BleError;
pub use fake::{Advertised, FakeFleet, FakeTransport, drive_signal};
pub use log::{LogLevel, set_logger};
pub use session::{CLOSE_GRACE, FAREWELL_BOUND, IDENTIFY_GAP, Session, SessionConfig};
pub use state::{Counters, DeviceState};
pub use svolder_fake::SIGNAL_PERIOD;
pub use tap::{FrameDirection, FrameTap, TappedFrame, TappedTransport};
pub use transport::{Advertisements, Link, Listening, Notifications, Transport};
