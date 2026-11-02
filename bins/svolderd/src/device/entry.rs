// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Adopting a changed config entry or profile registry without disturbing the link, and
//! resolving which profile a device uses.
//!
//! The handler sends a changed entry or a reloaded registry as a command, so a reload reaches the
//! running actor and its session.

use std::sync::Arc;

use svolder_ble::Advertisement;
use svolder_ipc::Response;
use svolder_ipc::config::DeviceConfig;
use svolder_profiles::{Profile, Registry, apply_input_labels};

use super::Actor;
use super::answers::usage_error;
use crate::log;

impl Actor {
    /// Adopt a changed entry for this device without disturbing the link. An entry that
    /// repeats what the actor is already doing changes nothing.
    ///
    /// The entry is stored regardless of the actor's state. The profile and the session's
    /// passphrase follow it only once an advertisement has been seen; while a connect attempt
    /// is in flight, [`Actor::absorb`] applies the new entry to the session it hands back.
    pub(super) fn configure(&mut self, config: DeviceConfig) -> Response {
        if self.config == config {
            return Response::Ack;
        }
        // What the actor calls the device is what every reply and every event names it: a
        // handle the set now holds under another spelling would otherwise be answered for
        // under the old one, and a front end matching the two would find neither.
        self.name = config.name.clone();
        if self.config.passphrase != config.passphrase {
            self.episode.end();
        }
        self.config = config;
        self.refresh_feed();
        let Some(advert) = self.advert.clone() else {
            return Response::Ack;
        };
        self.profile = resolve_profile(&self.registry, &self.config, &advert);
        self.publish_profile();
        self.sync_session_profile();
        let rekeyed = self.rekey(&advert);
        self.judge_passphrase();
        if let Err(message) = rekeyed {
            return usage_error(&message);
        }
        log::info(&format!(
            "{}: reconfigured, profile {}",
            self.name, self.profile.name
        ));
        Response::Ack
    }

    /// Adopt a freshly loaded registry and re-resolve the profile from the current advert.
    pub(super) fn reload(&mut self, registry: Arc<Registry>) {
        self.registry = registry;
        if let Some(advert) = self.advert.as_ref() {
            self.profile = resolve_profile(&self.registry, &self.config, advert);
            self.publish_profile();
            self.sync_session_profile();
        }
        log::info(&format!(
            "{}: profiles reloaded, profile {}",
            self.name, self.profile.name
        ));
    }

    /// Hand the session, when there is one, the profile the actor names, so what a refresh
    /// waits to hear follows a reload. A session inside a connect attempt has none to hand
    /// to; [`Actor::absorb`] hands it the profile when the attempt returns it.
    pub(super) fn sync_session_profile(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.set_profile(self.profile.clone());
        }
    }
}

/// The profile for an advertisement: the configured name, else the registry's pick for the
/// product and firmware, else a generic profile, with the configured input labels laid over
/// it.
pub(super) fn resolve_profile(
    registry: &Registry,
    config: &DeviceConfig,
    advert: &Advertisement,
) -> Profile {
    let mut profile = config
        .profile
        .as_deref()
        .and_then(|name| registry.get(name).cloned())
        .or_else(|| {
            registry
                .select(advert.manufacturer.product_id, advert.firmware_build())
                .cloned()
        })
        .unwrap_or_else(|| Registry::generic(advert.manufacturer.product_id));
    apply_input_labels(&mut profile, &config.inputs);
    profile
}
