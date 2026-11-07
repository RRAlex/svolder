// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bridge task's loop and its life with the broker: the `select!` that waits on the client,
//! the operations the handle sends and the timers, and the farewell that says `offline` before
//! the connection closes.
//!
//! The loop hands each wake-up to a method of the sibling modules ([`super::discovery`],
//! [`super::state`], [`super::commands`]), each an `impl BridgeTask` block of its own.

use svolder_mqtt::{ClientEvent, payload, topics};
use tokio::sync::mpsc;

use super::{BridgeTask, Op, STOP_WITHIN, address, sleep_until};
use crate::log;

impl BridgeTask {
    pub(super) async fn run(
        mut self,
        mut events: mpsc::UnboundedReceiver<ClientEvent>,
        mut ops: mpsc::UnboundedReceiver<Op>,
        mut changes: mpsc::UnboundedReceiver<String>,
    ) {
        let done = loop {
            let flush_at = self.flush_at;
            tokio::select! {
                op = ops.recv() => match op {
                    Some(Op::Devices(devices)) => self.set_devices(devices),
                    Some(Op::Presets(presets)) => self.set_presets(presets),
                    Some(Op::Stop { keep, done }) => {
                        let gone: Vec<String> = self
                            .devices
                            .keys()
                            .filter(|unit| !keep.contains(*unit))
                            .cloned()
                            .collect();
                        for unit in gone {
                            self.forget(&unit);
                        }
                        break done;
                    }
                    None => return,
                },
                event = events.recv() => match event {
                    Some(event) => self.client_event(event),
                    None => return,
                },
                Some(unit) = changes.recv() => self.changed(&unit),
                () = sleep_until(flush_at), if flush_at.is_some() => self.flush(),
                Some(done) = self.writes.join_next(), if !self.writes.is_empty() => {
                    if let Ok(outcome) = done {
                        self.take_outcome(outcome);
                    }
                }
            }
        };
        self.farewell().await;
        let _ = done.send(());
    }

    /// Say `offline` for every device, then have the client say it for the bridge and
    /// disconnect. A write in flight is the device's writer's, which finishes it, a recall
    /// included.
    async fn farewell(mut self) {
        self.writes.abort_all();
        let base = &self.config.base_topic;
        for (unit, published) in &self.devices {
            published.watcher.abort();
            self.client
                .retain(topics::device_availability(base, unit), payload::OFFLINE);
        }
        self.computer_farewell();
        let address = address(self.host(), self.config.port);
        self.client.stop(STOP_WITHIN).await;
        log::info(&format!("mqtt: disconnected from {address}"));
    }

    /// Log what the client reports, and act on a message that arrived.
    fn client_event(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::Connected => {
                self.last_lost = None;
                log::info(&format!(
                    "mqtt: connected to {}",
                    address(self.host(), self.config.port)
                ));
            }
            ClientEvent::Lost { reason, retry_in } => {
                let line = format!(
                    "mqtt: {}: {reason}; retrying in {} s",
                    address(self.host(), self.config.port),
                    retry_in.as_secs_f64().round() as u64
                );
                // A broker that stays away is said once at `warn`, and at `debug` for every
                // attempt after that fails the same way.
                if self.last_lost.as_deref() == Some(reason.as_str()) {
                    log::debug(&line);
                } else {
                    log::warn(&line);
                    self.last_lost = Some(reason);
                }
            }
            ClientEvent::Refused { filter } => log::warn(&format!(
                "mqtt: the broker refused the subscription to {}; nothing published there \
                 will arrive",
                filter.as_deref().unwrap_or("a topic")
            )),
            ClientEvent::Message {
                topic,
                payload,
                retained,
            } => self.message(&topic, &payload, retained),
        }
    }

    /// The broker's host, which a started bridge always has.
    fn host(&self) -> &str {
        self.config.host.as_deref().unwrap_or_default()
    }
}
