// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The transport abstraction: scan and connect (`Transport`), then read and write on a
//! `Link`. `BluezTransport` implements it against a real adapter, `FakeTransport`
//! in-process.

use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use svolder_proto::UnitId;

use crate::advert::Advertisement;
use crate::error::BleError;

/// A stream of RX notifications from a connected device.
pub type Notifications = BoxStream<'static, Vec<u8>>;
/// A stream of advertisements seen during a scan.
pub type Advertisements = BoxStream<'static, Advertisement>;

/// What one listen is for, asked while it runs: which devices it still listens for, what
/// becomes of an advertisement heard from one of them, and whether anyone is left.
///
/// Every method is called from inside the listen, between the radio's reports, so each answers
/// at once and never waits. A transport may call them while it holds its own scan, so a
/// listener never blocks on, or drives, a scan or a connect on the same transport. The wanted
/// set can change between calls: `heard` can arrive for a unit `wants` has just dropped. A
/// transport hands over at most one advertisement per peripheral the adapter reports in a
/// listen; one unit heard under two peripheral ids can arrive twice, so a listener stops
/// wanting a unit it has heard.
pub trait Listening: Send + Sync {
    /// Whether the listen is still for `unit`.
    fn wants(&self, unit: UnitId) -> bool;
    /// One advertisement from a unit [`Listening::wants`] named, handed over as it is heard.
    fn heard(&self, advert: &Advertisement);
    /// Whether nobody is left to listen for, which ends the listen before its window does.
    fn done(&self) -> bool;
}

/// Discovers devices and opens links to them.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Advertisements seen within `window`.
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError>;

    /// The Forkbeards the adapter holds from earlier sightings, each with the last
    /// advertisement it took from that device, and with no claim that any of them is still
    /// there. A caller that needs to know a device is on the air asks [`Transport::scan`];
    /// this is for one that needs an address and what the device said about itself. A
    /// transport that keeps nothing between scans holds nothing.
    async fn held(&self) -> Result<Advertisements, BleError> {
        Ok(stream::empty().boxed())
    }

    /// The first advertisement `unit` sends within `window`, or `None` when it sends none.
    ///
    /// A listen for one device named by its full unit id has nothing to wait the rest of the
    /// window out for once that device is heard, so a transport that can stop a scan early
    /// ends it right there; this default answers from a whole [`Transport::scan`].
    async fn sight(
        &self,
        unit: UnitId,
        window: Duration,
    ) -> Result<Option<Advertisement>, BleError> {
        let mut heard = self.scan(window).await?;
        while let Some(advert) = heard.next().await {
            if advert.unit_id() == unit {
                return Ok(Some(advert));
            }
        }
        Ok(None)
    }

    /// Listen for at most `window`, handing `listening` each advertisement from a unit it
    /// wants, until it is done. What it wants can change while the listen runs.
    ///
    /// A transport that hears devices one report at a time hands each over the moment it is
    /// heard and stops scanning the moment `listening` is done; this default answers from a
    /// whole [`Transport::scan`], once that scan is over.
    async fn listen(&self, window: Duration, listening: &dyn Listening) -> Result<(), BleError> {
        if listening.done() {
            return Ok(());
        }
        let mut handed = HashSet::new();
        let mut heard = self.scan(window).await?;
        while let Some(advert) = heard.next().await {
            if listening.done() {
                break;
            }
            if listening.wants(advert.unit_id()) && handed.insert(advert.unit_id()) {
                listening.heard(&advert);
            }
        }
        Ok(())
    }

    /// Open a link to the device with this unit id.
    ///
    /// A transport that remembers what its scans heard may open the link on the device a scan
    /// has just reported rather than looking for it again, since the connection is what settles
    /// whether it is still there.
    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError>;

    /// The adapter this transport is open on, as BlueZ names it (`hci0`), for a caller that
    /// records which controller reached a device. A transport that took no adapter of its own
    /// — the in-process fake — has none to name.
    fn adapter(&self) -> Option<&str> {
        None
    }

    /// Wait, for at most `bound`, for the work the transport still has in flight after a
    /// caller stopped waiting for it: a connection it is taking back down because the link
    /// it was opening has nowhere to go. That work runs on tasks of the transport's own, so
    /// a caller that is about to stop driving its runtime — a front end quitting, a session
    /// closing — asks here for those tasks to reach the radio first. A transport with
    /// nothing in flight returns at once, and one that keeps no work of its own has nothing
    /// to wait for. This waits for every device's work.
    async fn settle(&self, _bound: Duration) {}

    /// [`Transport::settle`] for one device: wait, for at most `bound`, for the work the
    /// transport still has in flight for `unit` alone, so a caller closing one device's
    /// session never waits for another device's release. A transport that does not keep its
    /// work per device waits for all of it, which is this default.
    async fn settle_for(&self, _unit: UnitId, bound: Duration) {
        self.settle(bound).await;
    }
}

/// A live connection to one device: write frames out, read notifications in.
#[async_trait]
pub trait Link: Send + Sync {
    /// Write one TX frame without waiting for a response.
    async fn write(&self, frame: &[u8]) -> Result<(), BleError>;
    /// The notification stream. Call once; a second call may yield an empty stream.
    fn notifications(&self) -> Notifications;
    /// Close the link.
    async fn disconnect(&self) -> Result<(), BleError>;
}
