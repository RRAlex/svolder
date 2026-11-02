// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which devices the bridge publishes and what it announces for them: following the daemon's
//! device list and the user's presets, subscribing to a device's topics, and clearing every
//! retained topic of a device that leaves.
//!
//! The device list and the presets reach the task as operations from [`super::Bridge`]; the
//! loop in [`super::connection`] calls these methods, and what they set up is published by
//! [`super::state`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use svolder_ipc::config::MqttConfig;
use svolder_mqtt::topics;
use svolder_profiles::Profile;
use svolder_profiles::tone::preset::{self, Preset};

use super::{BridgeTask, PublishedDevice, watch_feed};
use crate::device::DeviceHandle;
use crate::log;

impl BridgeTask {
    /// Follow exactly `devices`: forget each device that left the list, and start on each
    /// that joined it. A device that stayed keeps what it has published, under whatever name
    /// its feed now carries.
    pub(super) fn set_devices(&mut self, devices: Vec<Arc<DeviceHandle>>) {
        let wanted: BTreeMap<String, Arc<DeviceHandle>> = devices
            .into_iter()
            .map(|handle| {
                let unit = handle.feed.borrow().unit.clone();
                (unit, handle)
            })
            .collect();
        let gone: Vec<String> = self
            .devices
            .keys()
            .filter(|unit| !wanted.contains_key(*unit))
            .cloned()
            .collect();
        for unit in gone {
            self.forget(&unit);
        }
        for (unit, handle) in wanted {
            match self.devices.get_mut(&unit) {
                Some(published) => {
                    published.writer = handle.writer.clone();
                    // A device renamed in the file has an actor of its own for the same unit
                    // id: follow the new actor's feed, and announce the device again under
                    // whatever it now says.
                    if !published.feed.same_channel(&handle.feed) {
                        published.watcher.abort();
                        published.feed = handle.feed.clone();
                        published.watcher = tokio::spawn(watch_feed(
                            published.feed.clone(),
                            handle.writer.recalled(),
                            unit.clone(),
                            self.changes_tx.clone(),
                        ));
                        published.available = None;
                        published.configs_from = None;
                    }
                }
                None => self.track(unit.clone(), &handle),
            }
            self.changed(&unit);
        }
    }

    /// Take `presets` as the user's from now on, and have every device published again, so a
    /// preset select announces the options they now make.
    pub(super) fn set_presets(&mut self, presets: Vec<Preset>) {
        if self.presets == presets {
            return;
        }
        self.presets = presets;
        let units: Vec<String> = self.devices.keys().cloned().collect();
        for unit in units {
            self.changed(&unit);
        }
    }

    /// Start publishing one device: watch its feed; subscribe to what its discovery prefix
    /// holds under it (finding any config an earlier run left there) and, when commands are
    /// on, to its command topics.
    pub(super) fn track(&mut self, unit: String, handle: &DeviceHandle) {
        let feed = handle.feed.clone();
        let watcher = tokio::spawn(watch_feed(
            feed.clone(),
            handle.writer.recalled(),
            unit.clone(),
            self.changes_tx.clone(),
        ));
        for filter in subscriptions(&self.config, &unit) {
            self.client.subscribe(filter);
        }
        self.devices.insert(
            unit,
            PublishedDevice {
                feed,
                writer: handle.writer.clone(),
                watcher,
                dirty: true,
                available: None,
                configs_from: None,
                configs: BTreeMap::new(),
                leftovers: BTreeSet::new(),
                state: None,
                diagnostics: BTreeMap::new(),
                meters: None,
            },
        );
    }

    /// Stop publishing one device, and clear every retained topic of its own: its discovery
    /// configs — the ones this bridge announced, and the ones an earlier run left under its
    /// node that were waiting for its profile — so its entities leave Home Assistant, its
    /// state, its diagnostics and its availability.
    pub(super) fn forget(&mut self, unit: &str) {
        let Some(published) = self.devices.remove(unit) else {
            return;
        };
        published.watcher.abort();
        for topic in published
            .configs
            .keys()
            .chain(published.leftovers.iter())
            .chain(published.diagnostics.keys())
        {
            self.client.clear(topic.clone());
        }
        let base = &self.config.base_topic;
        self.client.clear(topics::state(base, unit));
        self.client.clear(topics::device_availability(base, unit));
        for filter in subscriptions(&self.config, unit) {
            self.client.unsubscribe(filter);
        }
        log::info(&format!(
            "mqtt: {} is no longer configured; its topics are cleared",
            published.feed.borrow().name
        ));
    }
}

/// Every filter the bridge subscribes to for one device: the one that matches every discovery
/// config under its node id, `<prefix>/+/svolder_<unit>/+/config`, while discovery is on, and
/// the one that matches its command topics, `<base>/<unit>/+/set`, only while commands are on
/// — a bridge that takes no commands subscribes to no command topic at all.
pub(super) fn subscriptions(config: &MqttConfig, unit: &str) -> Vec<String> {
    let prefix = &config.discovery_prefix;
    let discovery =
        (!prefix.is_empty()).then(|| format!("{prefix}/+/{}/+/config", topics::device_id(unit)));
    let commands = config
        .commands
        .then(|| topics::command(&config.base_topic, unit, "+"));
    discovery.into_iter().chain(commands).collect()
}

/// The presets that fit `profile`'s tone control, in the order its preset select offers them:
/// the built-ins, then the user's own that have its band count and every imported one.
pub(super) fn fitting_presets(profile: &Profile, user: &[Preset]) -> Vec<Preset> {
    profile
        .eq
        .as_ref()
        .map(|eq| {
            preset::fitting(eq, user)
                .into_iter()
                .map(|(p, _)| p)
                .collect()
        })
        .unwrap_or_default()
}

/// The unit id a discovery topic, `<prefix>/<component>/svolder_<unit>/<key>/config`, is
/// under, or `None` for any other topic.
pub(super) fn discovery_unit<'t>(prefix: &str, topic: &'t str) -> Option<&'t str> {
    if prefix.is_empty() {
        return None;
    }
    let rest = topic.strip_prefix(prefix)?.strip_prefix('/')?;
    match rest.split('/').collect::<Vec<_>>().as_slice() {
        [_component, node, _key, "config"] => node.strip_prefix("svolder_"),
        _ => None,
    }
}
