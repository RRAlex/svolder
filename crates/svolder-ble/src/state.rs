// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The last confirmed value of every control, folded from the device's status messages.

use std::collections::{BTreeMap, BTreeSet};

use svolder_proto::function::PASSPHRASE_REJECTED;
use svolder_proto::{FunctionId, Message, Notification};

/// Rolling counts of things that went wrong on the link.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counters {
    /// Frames the codec could not decode at all.
    pub decode_errors: u64,
    /// Messages dropped inside otherwise-valid notifications.
    pub dropped: u64,
    /// Times the device rejected the passphrase.
    pub passphrase_rejections: u64,
}

/// Every control's last confirmed value, plus link health.
#[derive(Debug, Clone, Default)]
pub struct DeviceState {
    values: BTreeMap<FunctionId, Message>,
    /// The functions folded in since the values were last marked stale: the ones the device
    /// has reported on the link the state now serves.
    heard: BTreeSet<FunctionId>,
    counters: Counters,
    stale: bool,
    passphrase_rejected: bool,
}

impl DeviceState {
    /// Fold one notification's messages into the map and count its drops.
    pub fn apply_notification(&mut self, n: &Notification) {
        self.counters.dropped += n.dropped.len() as u64;
        for m in &n.messages {
            self.apply(m);
        }
    }

    /// Fold one message into the map, keyed by its function id.
    ///
    /// Nothing here checks the direction bit: only decoded RX bytes are fed through, so every
    /// message is a status.
    pub fn apply(&mut self, message: &Message) {
        if message.id == PASSPHRASE_REJECTED {
            self.passphrase_rejected = true;
            self.counters.passphrase_rejections += 1;
            return;
        }
        self.values.insert(message.id, message.clone());
        self.heard.insert(message.id);
    }

    /// Record a notification the codec could not decode at all.
    pub fn note_decode_error(&mut self) {
        self.counters.decode_errors += 1;
    }

    /// The last message seen for a function id.
    pub fn get(&self, id: FunctionId) -> Option<&Message> {
        self.values.get(&id)
    }

    /// Every confirmed message, in function-id order.
    pub fn messages(&self) -> impl Iterator<Item = &Message> {
        self.values.values()
    }

    /// Whether the device has reported function `id` since the values were last marked stale.
    pub fn heard(&self, id: FunctionId) -> bool {
        self.heard.contains(&id)
    }

    /// The confirmed messages the device has reported since the values were last marked stale,
    /// in function-id order: a reading kept from before a disconnect is not among them until the
    /// device reports it again.
    pub fn heard_since_stale(&self) -> impl Iterator<Item = &Message> {
        self.values.values().filter(|m| self.heard.contains(&m.id))
    }

    /// Link-health counters.
    pub fn counters(&self) -> Counters {
        self.counters
    }

    /// True when the values are from before a disconnect and may be out of date.
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// Mark the values stale (on disconnect) or fresh (after a refresh). Going stale forgets which
    /// functions the device has reported.
    pub fn set_stale(&mut self, stale: bool) {
        self.stale = stale;
        if stale {
            self.heard.clear();
        }
    }

    /// True once the device has rejected the passphrase.
    pub fn passphrase_rejected(&self) -> bool {
        self.passphrase_rejected
    }

    /// Forget that the device rejected the passphrase, so the next frame is sent rather than
    /// refused before it is built.
    ///
    /// The rolling count in [`Counters::passphrase_rejections`] is left alone: it is the
    /// record of what the link has been through, which nothing here undoes.
    pub fn clear_passphrase_rejected(&mut self) {
        self.passphrase_rejected = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svolder_proto::{FunctionId, Message, Notification, ProtoError, Value};

    #[test]
    fn applies_and_overwrites() {
        let mut s = DeviceState::default();
        s.apply(&Message::status(FunctionId::one(10), Value::U8(100)));
        s.apply(&Message::status(FunctionId::one(10), Value::U8(109)));
        assert_eq!(s.get(FunctionId::one(10)).unwrap().value, Value::U8(109));
    }

    #[test]
    fn a_stale_reading_is_not_heard_until_the_device_reports_it_again() {
        let (a, b) = (FunctionId::one(10), FunctionId::one(11));
        let mut s = DeviceState::default();
        s.apply(&Message::status(a, Value::U8(1)));
        s.apply(&Message::status(b, Value::U8(2)));
        assert_eq!(s.heard_since_stale().count(), 2);
        s.set_stale(true);
        assert_eq!(s.heard_since_stale().count(), 0);
        assert_eq!(s.messages().count(), 2, "the values stay");
        s.apply(&Message::status(a, Value::U8(1)));
        let heard: Vec<_> = s.heard_since_stale().map(|m| m.id).collect();
        assert_eq!(heard, vec![a]);
    }

    #[test]
    fn counts_dropped_frames() {
        let mut s = DeviceState::default();
        let n = Notification {
            messages: vec![Message::status(FunctionId::one(2), Value::U8(3))],
            dropped: vec![ProtoError::TooShort(1)],
        };
        s.apply_notification(&n);
        assert_eq!(s.counters().dropped, 1);
        assert_eq!(s.counters().decode_errors, 0);
        s.note_decode_error();
        assert_eq!(s.counters().decode_errors, 1);
        assert_eq!(s.get(FunctionId::one(2)).unwrap().value, Value::U8(3));
    }

    #[test]
    fn passphrase_rejection_is_flagged_not_stored() {
        let mut s = DeviceState::default();
        s.apply(&Message::status(
            FunctionId::two(250, 255),
            Value::Bool(true),
        ));
        assert!(s.passphrase_rejected());
        assert_eq!(s.counters().passphrase_rejections, 1);
        assert!(s.get(FunctionId::two(250, 255)).is_none());

        // A new passphrase is what the refusal was about, so it is forgotten on request —
        // but the count of what the link has been through is not.
        s.clear_passphrase_rejected();
        assert!(!s.passphrase_rejected());
        assert_eq!(s.counters().passphrase_rejections, 1);
    }

    #[test]
    fn stale_flag_round_trips() {
        let mut s = DeviceState::default();
        assert!(!s.is_stale());
        s.set_stale(true);
        assert!(s.is_stale());
    }
}
