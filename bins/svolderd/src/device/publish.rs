// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the actor reports: statuses and faults as events, and the link, identity, signal strength
//! and profile to the live info and the feed, with the time each reading was heard.
//!
//! The handle, the event subscribers and the MQTT bridge read the device through these, never
//! through the actor itself.

use std::sync::Arc;

use svolder_ble::Advertisement;
use svolder_ipc::{Event, Signal, SignalSource, ValueObject};
use svolder_profiles::{ControlKind, describe, units};
use svolder_proto::{FunctionId, Message};

use super::Actor;
use super::history::wall_ms;
use crate::locks::locked;
use crate::log;

impl Actor {
    /// Publish one status, and a fault alongside it when a fault control has gone active.
    pub(super) fn publish_status(&self, m: &Message) {
        log::message("<-", &self.name, m);
        let reading = describe(&self.profile, m);
        let faulted = reading.kind == Some(ControlKind::Fault) && m.value.bool_value();
        let label = reading.label.clone();
        let ring = Arc::clone(&locked(&self.info).ring);
        locked(&ring).record(&self.profile, m, std::time::Instant::now());
        self.hear(m.id);
        let mut shown = ValueObject::from_reading(&reading, m);
        shown.heard_at_ms = self.heard_at(m.id);
        self.publish(Event::Status {
            device: self.name.clone(),
            reading: shown,
        });
        if faulted {
            self.publish(Event::Fault {
                device: self.name.clone(),
                label,
            });
        }
        self.refresh_feed();
    }

    /// Record in the ring every number the session's state holds, as of now, and stamp what the
    /// device reported on this link. A connect's refresh burst fills the state without reaching
    /// [`Actor::publish_status`], and what the readings answer and the web strip draw from starts
    /// here. Nothing is published.
    pub(super) fn record_state(&self) {
        let ring = Arc::clone(&locked(&self.info).ring);
        locked(&ring).record_all(
            &self.profile,
            self.state_of().messages(),
            std::time::Instant::now(),
        );
        // Only what the device reported on this link is stamped: a reading kept from before a
        // disconnect that the connect's burst did not repeat keeps the stamp it had.
        let now = wall_ms();
        let heard: Vec<FunctionId> = self.state_of().heard_since_stale().map(|m| m.id).collect();
        self.feed.send_if_modified(|feed| {
            for id in heard {
                feed.heard_at.insert(id, now);
            }
            false
        });
    }

    /// Note that function `id` was heard from the device just now. The feed's watchers are not
    /// woken: the caller refreshes the feed with the reading itself.
    fn hear(&self, id: FunctionId) {
        let now = wall_ms();
        self.feed.send_if_modified(|feed| {
            feed.heard_at.insert(id, now);
            false
        });
    }

    /// When function `id` was last heard from the device, in unix milliseconds.
    fn heard_at(&self, id: FunctionId) -> Option<u64> {
        self.feed.borrow().heard_at.get(&id).copied()
    }

    /// One message described through this device's profile. A readout an `only_while` gates
    /// reads `true` only while its gate reads true in the device's state too
    /// ([`units::shows_true`]); its raw value stays as the device sent it.
    pub(super) fn value_object(&self, m: &Message) -> ValueObject {
        let mut vo = ValueObject::from_reading(&describe(&self.profile, m), m);
        if let Some(control) = self.profile.control_by_id(m.id)
            && self.profile.gate_of(control).is_some()
        {
            let state = self.state_of();
            let shown = units::shows_true(
                &self.profile,
                control,
                control.reads_true(&m.value),
                |gate| {
                    state
                        .get(gate.id)
                        .map(|reading| gate.reads_true(&reading.value))
                },
            );
            vo.display = shown.to_string();
        }
        vo.heard_at_ms = self.heard_at(m.id);
        vo
    }

    /// Offer an event to whoever is subscribed; nobody listening is not a failure.
    pub(super) fn publish(&self, event: Event) {
        let _ = self.events.send(event);
    }

    /// Record the link state, and the staleness the session reports with it.
    pub(super) fn set_connected(&self, connected: bool) {
        let stale = self.state_of().is_stale();
        {
            let mut info = locked(&self.info);
            if info.connected != connected {
                locked(&info.history).record(wall_ms(), connected);
            }
            info.connected = connected;
            info.stale = stale;
            if !connected {
                info.refused = false;
            }
        }
        self.refresh_feed();
    }

    /// Bring the bridge's feed up to date with the name, the link and every confirmed
    /// reading. The profile and the firmware change only with a sighting, and
    /// [`Actor::publish_profile`] and [`Actor::update_identity`] carry them.
    pub(super) fn refresh_feed(&self) {
        let connected = locked(&self.info).connected;
        let readings: Vec<Message> = self.state_of().messages().cloned().collect();
        self.feed.send_modify(|feed| {
            feed.name.clone_from(&self.name);
            feed.connected = connected;
            feed.readings = readings;
        });
    }

    /// Record when the next discover-and-connect attempt is due, so `devices` can say how
    /// long a device that is away is being left alone for. `None` means nothing is waiting:
    /// an attempt is running, or the link is up.
    pub(super) fn set_next_attempt(&self, at: Option<tokio::time::Instant>) {
        locked(&self.info).next_attempt = at;
    }

    /// Record what the advertisement says about the device, with its product named as the
    /// registry the actor holds names it, and its signal strength when it carries one and a
    /// listen of the daemon's heard it at `heard_at_ms`. An advertisement taken from the held
    /// list has no such moment and leaves the signal as it is.
    pub(super) fn update_identity(&self, advert: &Advertisement, heard_at_ms: Option<u64>) {
        {
            let mut info = locked(&self.info);
            info.unit = advert.unit_id().to_string();
            info.product_id = advert.manufacturer.product_id;
            info.product = self
                .registry
                .product_name(advert.manufacturer.product_id)
                .map(str::to_string);
            info.firmware = advert.firmware_build();
            // An advertisement that carries no strength, or that no listen heard, leaves the
            // one held as it is.
            if let (Some(dbm), Some(heard_at_ms)) = (advert.rssi, heard_at_ms) {
                info.signal = Some(Signal {
                    dbm,
                    source: SignalSource::Advert,
                    heard_at_ms,
                });
            }
        }
        let firmware = advert.firmware_build();
        let signal = locked(&self.info).signal;
        self.feed.send_modify(|feed| {
            feed.firmware = Some(firmware);
            feed.signal = signal;
        });
    }

    /// Record the profile the actor now names and converts values with, so `devices` reports
    /// what a front end has to read this device's values through.
    pub(super) fn publish_profile(&self) {
        locked(&self.info).profile.clone_from(&self.profile.name);
        let profile = Arc::new(self.profile.clone());
        self.feed.send_modify(|feed| feed.profile = Some(profile));
    }
}
