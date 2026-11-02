// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeping the link up: the loop that connects, serves while it holds, and waits a growing step
//! between failed attempts.
//!
//! The actor's `run` is these two loops in turn, so a device that goes away is found again
//! without a restart.

use std::time::Duration;

use svolder_ble::Session;
use svolder_ipc::{Event, Response};
use svolder_proto::Message;
use tokio::sync::oneshot;

use super::{Actor, Command, Commands};
use crate::log;

/// How long the actor waits between failed discover-and-connect attempts, one step per
/// consecutive failure, the last one repeating as long as the device stays away.
///
/// Growing the steps avoids scanning continuously for a device that is off — power spent on a
/// device that is not there, and a scan every other user of the radio on the host has to share
/// the air with — while the last step (within a minute) still picks it back up promptly. A
/// command that needs the link, or a changed configuration entry, ends the wait early (see
/// [`Command::ends_the_wait`]).
pub(super) const RETRY_SCHEDULE: [Duration; 5] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
];

/// What the supervision loop woke up for.
pub(super) enum Woke {
    /// A command arrived, or the channel closed.
    Command(Option<(Command, oneshot::Sender<Response>)>),
    /// A status arrived, or the link ended.
    Status(Option<Message>),
}

/// How a wait between two discover-and-connect attempts ended.
pub(super) enum Idled {
    /// The wait ran out, so the next attempt is due.
    Elapsed,
    /// A command that says the device may be reachable now cut the wait short, so the caller
    /// makes its next attempt at once.
    Interrupted,
    /// The actor was asked to stop, or has nobody left to serve.
    Stop,
}

/// How one discover-and-connect attempt ended.
pub(super) enum AttemptEnd {
    /// The link is up.
    Connected,
    /// The attempt failed, with the reason to log.
    Failed(String),
    /// The actor was asked to stop while the attempt ran.
    Stop,
}

impl Actor {
    /// Discover and connect, waiting out a step of [`RETRY_SCHEDULE`] between attempts, until
    /// a link is up.
    ///
    /// Each attempt runs in its own task so the actor keeps answering commands throughout it
    /// (on a real adapter: a five-second scan, then a twenty-second connect deadline, so
    /// twenty-five in all, of which the deadline's own first three seconds are another scan).
    /// No read needs the link. Returns `false` when the actor should stop.
    pub(super) async fn ensure_connected(&mut self, rx: &mut Commands) -> bool {
        loop {
            if self.linked() {
                return true;
            }
            match self.attempt(rx).await {
                AttemptEnd::Stop => return false,
                AttemptEnd::Connected => {
                    self.record_state();
                    self.set_connected(true);
                    self.last_failure = None;
                    self.failures = 0;
                    let device = self.name.clone();
                    self.publish(Event::Connection {
                        device,
                        connected: true,
                    });
                    log::info(&format!(
                        "{}: connected, profile {}",
                        self.name, self.profile.name
                    ));
                    return true;
                }
                AttemptEnd::Failed(message) => {
                    let wait =
                        RETRY_SCHEDULE[(self.failures as usize).min(RETRY_SCHEDULE.len() - 1)];
                    self.log_failure(&message, wait);
                    self.failures = self.failures.saturating_add(1);
                    self.set_next_attempt(Some(tokio::time::Instant::now() + wait));
                    match self.idle(rx, wait).await {
                        Idled::Stop => return false,
                        Idled::Elapsed => {}
                        // Something said the device may be reachable before the schedule
                        // expected it ([`Command::ends_the_wait`]): the attempt this loop
                        // makes next starts the schedule over.
                        Idled::Interrupted => self.failures = 0,
                    }
                }
            }
        }
    }

    /// Report a failed attempt, naming the wait before the next one: at `warn` the first time
    /// and whenever the reason changes, so a device that never appears is visible in a
    /// journal at the default level, and at `debug` for every repeat of the same reason, so a
    /// retry loop does not flood it.
    pub(super) fn log_failure(&mut self, message: &str, wait: Duration) {
        let line = format!(
            "{}: connect failed: {message}; retrying in {} s",
            self.name,
            wait.as_secs()
        );
        if self.last_failure.as_deref() == Some(message) {
            log::debug(&line);
            return;
        }
        log::warn(&line);
        self.last_failure = Some(message.to_string());
    }

    /// Read statuses and run commands while the link is up.
    ///
    /// Both arms of the `select!` are cancel-safe: `Receiver::recv` loses nothing when it is
    /// dropped, and `Session::recv` writes its queued replies and any refresh it owes the device
    /// and then awaits its notification stream; its own cancel-safety paragraph says what a drop
    /// in either step costs.
    /// Reconnection runs outside this loop, so a cancelled branch can never orphan a radio connection.
    /// Returns `false` when the actor should stop, `true` when the link dropped.
    pub(super) async fn serve_connected(&mut self, rx: &mut Commands) -> bool {
        loop {
            let woke = {
                let Some(session) = self.session.as_mut() else {
                    return true;
                };
                tokio::select! {
                    incoming = rx.recv() => Woke::Command(incoming),
                    status = session.recv() => Woke::Status(status),
                }
            };
            match woke {
                Woke::Command(None) => return false,
                Woke::Command(Some((Command::Shutdown, reply))) => {
                    self.farewell = Some(reply);
                    return false;
                }
                Woke::Command(Some((command, reply))) => {
                    let response = self.run_command(command).await;
                    let _ = reply.send(response);
                    if self.release_after_refusal().await {
                        return true;
                    }
                }
                Woke::Status(Some(m)) => {
                    self.publish_status(&m);
                    // A refresh a wake owed may have shown the code works, and a refusal ends
                    // what an earlier answer showed.
                    self.judge_passphrase();
                    if self.release_after_refusal().await {
                        return true;
                    }
                }
                Woke::Status(None) => {
                    self.set_connected(false);
                    let device = self.name.clone();
                    self.publish(Event::Connection {
                        device,
                        connected: false,
                    });
                    log::warn(&format!("{}: link lost", self.name));
                    return true;
                }
            }
        }
    }

    /// Wait out `delay`, answering commands as they arrive.
    ///
    /// Every command is answered here exactly as it is anywhere else with the link down: one
    /// that needs the link refused, a read from the last known state, a changed entry stored.
    /// What the answer is has nothing to do with whether the wait goes on —
    /// [`Command::ends_the_wait`] decides that, and a wait it ends is one whose next attempt
    /// is worth making now rather than at the end of a step that may be a minute long.
    pub(super) async fn idle(&mut self, rx: &mut Commands, delay: Duration) -> Idled {
        let deadline = tokio::time::Instant::now() + delay;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => return Idled::Elapsed,
                incoming = rx.recv() => match incoming {
                    None => return Idled::Stop,
                    Some((Command::Shutdown, reply)) => {
                        self.farewell = Some(reply);
                        return Idled::Stop;
                    }
                    Some((command, reply)) => {
                        // Asked before the command is consumed, and against the entry the
                        // actor is still running: `run_command` is what adopts a new one.
                        let ends_the_wait = command.ends_the_wait(&self.config);
                        let response = self.run_command(command).await;
                        let _ = reply.send(response);
                        if ends_the_wait {
                            return Idled::Interrupted;
                        }
                    }
                },
            }
        }
    }

    /// True while the actor holds a link.
    pub(super) fn linked(&self) -> bool {
        self.session.as_ref().is_some_and(Session::is_connected)
    }

    /// Release the radio, so bluetoothd hands the device back.
    pub(super) async fn close(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.close().await;
        }
        self.set_connected(false);
        self.set_next_attempt(None);
    }
}
