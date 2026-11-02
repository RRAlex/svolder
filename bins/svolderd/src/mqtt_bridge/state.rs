// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the bridge publishes for a device and when: marking a device changed, the flush that
//! publishes the marked ones, and the publish of one device's availability, discovery configs,
//! state JSON and meters JSON.
//!
//! The state goes out at most every [`STATE_EVERY`] and the meters at most every
//! [`METERS_EVERY`], so a feed that changes often is coalesced rather than forwarded one change
//! at a time. The loop in [`super::connection`] drives these methods from the feed's changes
//! and its flush timer.

use std::collections::BTreeMap;
use std::sync::Arc;

use svolder_mqtt::{Client, discovery, payload, topics};
use tokio::time::Instant;

use super::discovery::fitting_presets;
use super::{BridgeTask, ConfigsFrom, METERS_EVERY, PublishedDevice, STATE_EVERY};
use crate::device::Feed;

impl BridgeTask {
    /// Mark one device changed, and have it published within [`STATE_EVERY`] of the last
    /// publish.
    pub(super) fn changed(&mut self, unit: &str) {
        let Some(published) = self.devices.get_mut(unit) else {
            return;
        };
        published.dirty = true;
        let now = Instant::now();
        let due = self
            .last_flush
            .map_or(now, |last| (last + STATE_EVERY).max(now));
        self.flush_at = Some(self.flush_at.map_or(due, |at| at.min(due)));
    }

    /// Publish every device that changed. A device whose meters moved sooner than
    /// [`METERS_EVERY`] after their last publish stays marked, and the next flush is when
    /// they are due.
    pub(super) fn flush(&mut self) {
        let now = Instant::now();
        self.last_flush = Some(now);
        self.flush_at = None;
        let units: Vec<String> = self
            .devices
            .iter()
            .filter(|(_, published)| published.dirty)
            .map(|(unit, _)| unit.clone())
            .collect();
        for unit in units {
            if let Some(owed) = self.publish(&unit, now) {
                self.flush_at = Some(self.flush_at.map_or(owed, |at| at.min(owed)));
            }
        }
    }

    /// Publish what changed for one device: its availability, its discovery configs, its
    /// state and its meters. Answers with when the meters are owed, when they changed too
    /// soon after the last publish of them.
    pub(super) fn publish(&mut self, unit: &str, now: Instant) -> Option<Instant> {
        let config = &self.config;
        let client = &self.client;
        let published = self.devices.get_mut(unit)?;
        published.dirty = false;
        let feed = published.feed.borrow().clone();
        let base = &config.base_topic;

        if published.available != Some(feed.connected) {
            let word = match feed.connected {
                true => payload::ONLINE,
                false => payload::OFFLINE,
            };
            client.retain(topics::device_availability(base, unit), word);
            published.available = Some(feed.connected);
        }

        // Until the device has been seen there is no profile to say what it has.
        let profile = feed.profile.as_ref()?;
        let fitting = fitting_presets(profile, &self.presets);
        let names: Vec<String> = fitting.iter().map(|p| p.name.clone()).collect();
        let built = published.configs_from.as_ref().is_some_and(|from| {
            Arc::ptr_eq(&from.profile, profile)
                && from.name == feed.name
                && from.firmware == feed.firmware
                && from.presets == names
        });
        if !built {
            let device = discovery::Device {
                name: &feed.name,
                unit_id: unit,
                firmware: feed.firmware,
            };
            let next: BTreeMap<String, String> = svolder_mqtt::discovery_configs(
                base,
                &config.discovery_prefix,
                profile,
                &device,
                config.commands,
                &names,
            )
            .into_iter()
            .map(|entry| (entry.topic, entry.payload.to_string()))
            .collect();
            for (topic, body) in &next {
                if published.configs.get(topic) != Some(body) {
                    client.announce(topic.clone(), body.clone());
                }
            }
            let stale = published
                .configs
                .keys()
                .chain(published.leftovers.iter())
                .filter(|topic| !next.contains_key(*topic));
            for topic in stale {
                client.clear(topic.clone());
            }
            published.leftovers.clear();
            published.configs = next;
            published.configs_from = Some(ConfigsFrom {
                profile: Arc::clone(profile),
                name: feed.name.clone(),
                firmware: feed.firmware,
                presets: names,
            });
        }

        let mut state = payload::state(profile, &feed.readings);
        // The select reports the daemon-wide memory: the preset last recalled by any front end,
        // for as long as the device still matches it.
        let recalled = published.writer.recalled().borrow().clone();
        if let Some(chosen) =
            svolder_mqtt::eq::preset_state(profile, &feed.readings, &fitting, recalled.as_deref())
        {
            state[svolder_mqtt::eq::PRESET_KEY] = chosen.into();
        }
        let state = state.to_string();
        if published.state.as_ref() != Some(&state) {
            client.retain(topics::state(base, unit), state.clone());
            published.state = Some(state);
        }

        publish_diagnostics(client, base, unit, &feed, published);

        let meters = payload::meters(profile, &feed.readings);
        if meters.as_object().is_some_and(|m| !m.is_empty()) {
            let meters = meters.to_string();
            match &published.meters {
                Some((last, _)) if *last == meters => {}
                Some((_, at)) if now < *at + METERS_EVERY => {
                    published.dirty = true;
                    return Some(*at + METERS_EVERY);
                }
                _ => {
                    client.publish(topics::meters(base, unit), meters.clone());
                    published.meters = Some((meters, now));
                }
            }
        }
        None
    }
}

/// Publish what changed among the diagnostics, as [`changed_diagnostics`] says.
fn publish_diagnostics(
    client: &Client,
    base: &str,
    unit: &str,
    feed: &Feed,
    published: &mut PublishedDevice,
) {
    for (topic, body) in changed_diagnostics(base, unit, feed, &mut published.diagnostics) {
        client.retain(topic, body);
    }
}

/// The diagnostic payloads to publish now, as `(topic, payload)` in publish order: the signal's
/// attributes then its strength (when one is known: with none, nothing goes out and Home
/// Assistant shows the sensor unknown), and each firmware build the device has reported. A
/// payload that is the one `known` holds for its topic is left out, so a repeat of the same
/// hearing is not a message; each one returned is recorded in `known`.
pub(super) fn changed_diagnostics(
    base: &str,
    unit: &str,
    feed: &Feed,
    known: &mut BTreeMap<String, String>,
) -> Vec<(String, String)> {
    let mut wanted: Vec<(String, String)> = Vec::new();
    if let Some(signal) = feed.signal {
        wanted.push((
            topics::signal_attributes(base, unit),
            payload::signal_attributes(signal.source.word(), signal.heard_at_ms).to_string(),
        ));
        wanted.push((
            topics::diagnostic(base, unit, discovery::SIGNAL_KEY),
            payload::signal_state(signal.dbm),
        ));
    }
    for (key, id) in [
        (discovery::FIRMWARE_KEY, payload::FIRMWARE),
        (discovery::DSP_FIRMWARE_KEY, payload::DSP_FIRMWARE),
    ] {
        if let Some(build) = payload::firmware_state(&feed.readings, id) {
            wanted.push((topics::diagnostic(base, unit, key), build));
        }
    }
    wanted.retain(|(topic, body)| known.get(topic) != Some(body));
    for (topic, body) in &wanted {
        known.insert(topic.clone(), body.clone());
    }
    wanted
}
