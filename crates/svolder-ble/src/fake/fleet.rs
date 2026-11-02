// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Several fake devices on one transport, each answering its own unit id: what
//! `SVOLDER_FAKE_MORE` builds beside the device the other `SVOLDER_FAKE_*` knobs describe.

use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use svolder_proto::UnitId;

use super::FakeTransport;
use crate::advert::Advertisement;
use crate::error::BleError;
use crate::transport::{Advertisements, Link, Transport};

/// A transport over a handful of fake devices, told apart by their unit ids: a scan hears every
/// one that is on the air, and a connect or a settle goes to the one it names.
pub struct FakeFleet {
    members: Vec<(UnitId, FakeTransport)>,
}

impl FakeFleet {
    /// The fleet of `first`, whose unit id is `unit`, and the devices `more` names:
    /// `<profile>@<UNIT>` entries separated by commas, each a device seeded for its profile
    /// with the code `1234` and firmware build 3, and, with `signal`, a signal playing on its
    /// meters (which needs a Tokio runtime; a device without meters plays none). An entry that
    /// does not parse, names no profile, or repeats a unit id is an error that says which.
    pub fn from_more(
        registry: &svolder_profiles::Registry,
        first: FakeTransport,
        more: &str,
        signal: bool,
    ) -> Result<Self, String> {
        let mut members = vec![(first.unit, first)];
        for entry in more.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (profile_name, unit) = entry.split_once('@').ok_or_else(|| {
                format!("bad SVOLDER_FAKE_MORE: {entry:?} is not <profile>@<UNIT>")
            })?;
            let profile = registry
                .get(profile_name)
                .ok_or_else(|| format!("bad SVOLDER_FAKE_MORE: no profile {profile_name}"))?;
            let unit: UnitId = unit
                .parse()
                .map_err(|_| format!("bad SVOLDER_FAKE_MORE: {unit:?} is not a unit id"))?;
            if members.iter().any(|(taken, _)| *taken == unit) {
                return Err(format!("bad SVOLDER_FAKE_MORE: unit {unit} is used twice"));
            }
            let code = svolder_proto::Passphrase::from_text("1234")
                .map_err(|e| format!("bad SVOLDER_FAKE_MORE: {e}"))?;
            let member = FakeTransport::from_profile(profile, unit, 3, code);
            if signal {
                super::drive_signal(&member, svolder_fake::SIGNAL_PERIOD);
            }
            members.push((unit, member));
        }
        Ok(Self { members })
    }

    /// The transport of the device with `unit`.
    fn member(&self, unit: UnitId) -> Option<&FakeTransport> {
        self.members
            .iter()
            .find(|(own, _)| *own == unit)
            .map(|(_, transport)| transport)
    }

    /// The unit ids of the devices, the first device's first.
    pub fn units(&self) -> Vec<UnitId> {
        self.members.iter().map(|(unit, _)| *unit).collect()
    }
}

#[async_trait]
impl Transport for FakeFleet {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        let mut heard: Vec<Advertisement> = Vec::new();
        for (_, member) in &self.members {
            heard.extend(member.scan(window).await?.collect::<Vec<_>>().await);
        }
        Ok(stream::iter(heard).boxed())
    }

    async fn held(&self) -> Result<Advertisements, BleError> {
        let mut held: Vec<Advertisement> = Vec::new();
        for (_, member) in &self.members {
            held.extend(member.held().await?.collect::<Vec<_>>().await);
        }
        Ok(stream::iter(held).boxed())
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        match self.member(unit) {
            Some(member) => member.connect(unit).await,
            None => Err(BleError::NotFound),
        }
    }

    async fn settle(&self, bound: Duration) {
        for (_, member) in &self.members {
            member.settle(bound).await;
        }
    }

    async fn settle_for(&self, unit: UnitId, bound: Duration) {
        if let Some(member) = self.member(unit) {
            member.settle_for(unit, bound).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svolder_profiles::Registry;

    const FIRST: &str = "0102030405060708";
    const SECOND: &str = "1112131415161718";

    fn first() -> FakeTransport {
        let registry = Registry::builtin().unwrap();
        FakeTransport::from_knobs(&registry, |_| None).unwrap()
    }

    fn fleet() -> FakeFleet {
        let registry = Registry::builtin().unwrap();
        FakeFleet::from_more(&registry, first(), &format!("fake-amp@{SECOND}"), false).unwrap()
    }

    /// Each unit gets a link of its own, and a unit nobody holds is refused.
    #[tokio::test]
    async fn two_fakes_answer_their_own_units() {
        let fleet = fleet();
        for unit in [FIRST, SECOND] {
            let link = fleet.connect(unit.parse().unwrap()).await;
            assert!(link.is_ok(), "{unit} has no link");
        }
        let stranger = fleet.connect("2122232425262728".parse().unwrap()).await;
        assert!(matches!(stranger, Err(BleError::NotFound)));
        let counts: Vec<usize> = fleet.members.iter().map(|(_, m)| m.connects()).collect();
        assert_eq!(counts, [1, 1], "each device opened its own link once");
    }

    /// A scan hears every member, and so does a look at what the adapter holds.
    #[tokio::test]
    async fn the_fleet_scan_hears_every_member() {
        let fleet = fleet();
        let units = |adverts: Vec<Advertisement>| -> Vec<String> {
            adverts.iter().map(|a| a.unit_id().to_string()).collect()
        };
        let scanned = fleet.scan(Duration::from_millis(10)).await.unwrap();
        assert_eq!(units(scanned.collect().await), [FIRST, SECOND]);
        let held = fleet.held().await.unwrap();
        assert_eq!(units(held.collect().await), [FIRST, SECOND]);
    }

    /// A member that has gone off the air is not heard, and the others still are.
    #[tokio::test]
    async fn a_member_that_goes_away_is_not_heard() {
        let fleet = fleet();
        fleet.members[1].1.go_away_after(Duration::ZERO);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let heard: Vec<Advertisement> = fleet
            .scan(Duration::from_millis(10))
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].unit_id().to_string(), FIRST);
        assert!(fleet.connect(SECOND.parse().unwrap()).await.is_err());
    }

    /// A settle for one unit reaches that device alone.
    #[tokio::test]
    async fn a_settle_goes_to_the_device_it_names() {
        let fleet = fleet();
        fleet
            .settle_for(SECOND.parse().unwrap(), Duration::from_millis(10))
            .await;
        let settles: Vec<usize> = fleet.members.iter().map(|(_, m)| m.settles()).collect();
        assert_eq!(settles, [0, 1]);
    }

    /// A bad entry is an error that names the variable, and a unit used twice is refused.
    #[test]
    fn a_bad_more_entry_names_its_variable() {
        let registry = Registry::builtin().unwrap();
        for more in [
            "fake-amp",
            "no-such@1112131415161718",
            "fake-amp@xyz",
            &format!("fake-amp@{FIRST}"),
        ] {
            let err = FakeFleet::from_more(&registry, first(), more, false)
                .err()
                .unwrap_or_else(|| panic!("{more:?} was accepted"));
            assert!(err.contains("SVOLDER_FAKE_MORE"), "{err}");
        }
    }

    /// `SVOLDER_FAKE_MORE` makes the knobs' transport a fleet, and without it the one device
    /// answers alone.
    #[tokio::test]
    async fn the_more_knob_makes_the_transport_a_fleet() {
        let registry = Registry::builtin().unwrap();
        let more = format!("fake-amp@{SECOND}");
        let fleet = FakeTransport::transport_from_knobs(&registry, |n| {
            (n == "SVOLDER_FAKE_MORE").then(|| more.clone())
        })
        .unwrap();
        assert!(fleet.connect(SECOND.parse().unwrap()).await.is_ok());
        let alone = FakeTransport::transport_from_knobs(&registry, |_| None).unwrap();
        assert!(alone.connect(SECOND.parse().unwrap()).await.is_err());
        assert!(alone.connect(FIRST.parse().unwrap()).await.is_ok());
    }

    /// `SVOLDER_FAKE_SIGNAL` plays on an added amp's meters too, and without it the amp is
    /// still.
    #[tokio::test]
    async fn an_added_amps_meters_move_with_the_signal_on() {
        let registry = Registry::builtin().unwrap();
        let more = format!("fake-amp@{SECOND}");
        let heard = |signal: bool| {
            let registry = &registry;
            let more = more.clone();
            async move {
                let fleet = FakeTransport::transport_from_knobs(registry, |n| match n {
                    "SVOLDER_FAKE_MORE" => Some(more.clone()),
                    "SVOLDER_FAKE_SIGNAL" => signal.then(|| "1".to_string()),
                    _ => None,
                })
                .unwrap();
                let link = fleet.connect(SECOND.parse().unwrap()).await.unwrap();
                let mut notes = link.notifications();
                tokio::time::timeout(Duration::from_secs(2), notes.next())
                    .await
                    .ok()
                    .flatten()
            }
        };
        let frame = heard(true).await.expect("the signal pushes a frame");
        let note = svolder_proto::decode_notification(&frame).unwrap();
        assert!(!note.messages.is_empty());
        assert!(heard(false).await.is_none(), "no signal, no frames");
    }
}
