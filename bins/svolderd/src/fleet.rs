// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the daemon's HTTP servers reach it through: the [`Fleet`] trait, and [`DaemonFleet`],
//! the daemon itself as one server's front end, every request answered as the socket answers
//! it, every write made as that server's origin.

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::{self, BoxStream};
use svolder_host::cover::Cover;
use svolder_ipc::host::HostView;
use svolder_ipc::{Config, DeviceInfo, Event, Request, Response};
use svolder_profiles::Profile;
use svolder_profiles::tone::preset::Preset;

use crate::device::Command;
use crate::handler::{ACTOR_DEADLINE, Daemon, actor_gone, actor_silent, silence};
use crate::log;
use crate::recall::{Applied, Recalled};
use crate::writer::{Glided, Mode, Options, Origin, Outcome, Planned, Recalling};

/// One device's live state as the web UI follows it: its feed (profile, readings, link) and the
/// writer's climb.
pub struct Watched {
    /// The device's feed.
    pub feed: tokio::sync::watch::Receiver<crate::device::Feed>,
    /// The writer's climb.
    pub climb: tokio::sync::watch::Receiver<Option<crate::writer::ClimbView>>,
    /// The daemon's recall memory for the device.
    pub recalled: tokio::sync::watch::Receiver<Option<String>>,
}

/// The computer's player as a web page follows it: the host's view and each change of it, and,
/// for a page that shows the path, the route reads held on for as long as the value lives.
pub struct HostWatch {
    /// The view, which the receiver says has changed each time the host publishes.
    pub changes: tokio::sync::watch::Receiver<HostView>,
    /// The hold on the route reads, let go when the page's stream ends.
    _held: Option<crate::host::Watcher>,
}

/// The computer's audio as a page's frames stream follows it: the capture's state and each
/// frame, for as long as the stream lives, which holds the daemon's capture on.
pub type FramesWatch = BoxStream<'static, crate::host::FramesEvent>;

/// What the tools reach the daemon through.
#[async_trait]
pub trait Fleet: Send + Sync + 'static {
    /// The device `selector` names, resolved as every request resolves one; with no selector,
    /// the default or only device. The error names the configured devices.
    fn resolve(&self, selector: Option<&str>) -> Result<DeviceInfo, String>;

    /// Answer one request the way the socket's server would.
    async fn request(&self, request: Request) -> Response;

    /// Recall the preset `name` among `fitting` on `device` through the device's writer, so the
    /// tone-control writes other front ends send meanwhile wait for it: the preset and its plan,
    /// and how the recall ended; or why it could not be planned.
    async fn recall(
        &self,
        device: &DeviceInfo,
        name: &str,
        fitting: Vec<Preset>,
    ) -> Result<(Recalled, Response), String>;

    /// Apply the built-in setting `name` of `device` through the device's writer, as one batch
    /// that holds the writes to the bands it sets: the setting and its writes, and how the batch
    /// ended; or why it could not be planned.
    async fn apply(
        &self,
        _device: &DeviceInfo,
        _name: &str,
    ) -> Result<(Applied, Response), String> {
        Err("no daemon".to_string())
    }

    /// Set the level `key` of `device` to raw step `raw` through the device's writer, which
    /// climbs to it where the shared rule says it may not jump
    /// ([`Writer::glide`](crate::writer::Writer::glide)).
    async fn glide(&self, device: &DeviceInfo, key: &str, raw: i64) -> Glided;

    /// Write `command` to `device` through its writer, as this fleet's origin, waiting as
    /// `options` says: how the write ended.
    async fn submit(&self, _device: &DeviceInfo, _command: Command, _options: Options) -> Outcome {
        Outcome::Cancelled("no daemon")
    }

    /// End `device`'s wait between connect attempts: try it now.
    async fn wake(&self, _device: &DeviceInfo) -> Response {
        Response::Error {
            code: 3,
            message: "no daemon".to_string(),
        }
    }

    /// The profile of that name, as the daemon's registry holds it.
    fn profile(&self, device: &DeviceInfo) -> Option<Profile>;

    /// The devices' events, readings among them; none for a fleet that has no stream.
    async fn events(&self) -> BoxStream<'static, Event> {
        futures::stream::empty().boxed()
    }

    /// The live state of `device`, for a page that follows it; none for a fleet that has no
    /// devices of its own.
    fn watch(&self, _device: &DeviceInfo) -> Option<Watched> {
        None
    }

    /// The computer's player and route as the host service has them now, stamped with the
    /// daemon's clock; an unavailable host for a fleet that has none.
    fn host_view(&self) -> HostView {
        HostView::default()
    }

    /// Follow the host for a page's stream: its changes, and, when `route` is set, the route read
    /// for as long as the returned value is held; none for a fleet that has no host.
    fn host_watch(&self, _route: bool) -> Option<HostWatch> {
        None
    }

    /// The host's current cover when `hash` is its hash; never an earlier one.
    fn host_cover(&self, _hash: &str) -> Option<Cover> {
        None
    }

    /// Hold the daemon's capture of what the computer plays for a page's frames stream, and
    /// follow it: the capture's state first, then its changes and the frames. The capture
    /// stops 2 s after the last hold is let go. None for a fleet that has no host, or whose
    /// host is off.
    fn host_frames(&self) -> Option<FramesWatch> {
        None
    }

    /// The configuration file as it reads now. An error says only that it could not be read:
    /// the detail may quote a secret, so it stays in the daemon's log.
    fn config(&self) -> Result<Config, String>;

    /// The configuration file the daemon reads, for a front end that writes presets into it;
    /// none for a fleet with no file of its own.
    fn config_path(&self) -> Option<std::path::PathBuf> {
        None
    }
}

/// A fleet with nothing behind it, for tests of the server that never reach a device.
#[cfg(test)]
pub struct Unavailable;

#[cfg(test)]
#[async_trait]
impl Fleet for Unavailable {
    fn resolve(&self, _selector: Option<&str>) -> Result<DeviceInfo, String> {
        Err("no devices".to_string())
    }

    async fn request(&self, _request: Request) -> Response {
        Response::Error {
            code: 3,
            message: "no daemon".to_string(),
        }
    }

    async fn recall(
        &self,
        _device: &DeviceInfo,
        _name: &str,
        _fitting: Vec<Preset>,
    ) -> Result<(Recalled, Response), String> {
        Err("no daemon".to_string())
    }

    async fn glide(&self, _device: &DeviceInfo, _key: &str, _raw: i64) -> Glided {
        Glided::Refused(Response::Error {
            code: 3,
            message: "no daemon".to_string(),
        })
    }

    fn profile(&self, _device: &DeviceInfo) -> Option<Profile> {
        None
    }

    fn config(&self) -> Result<Config, String> {
        Ok(Config::default())
    }
}

/// The daemon as one HTTP server reaches it: every request goes through
/// [`Daemon::handle_from`] as `origin`'s, the way a socket client's does, and every glide and
/// recall through the device's writer as `origin`'s, waiting as `mode` says.
pub struct DaemonFleet {
    daemon: std::sync::Weak<Daemon>,
    origin: Origin,
    mode: Mode,
}

impl DaemonFleet {
    /// The daemon behind `daemon`, written to as `origin`, waiting as `mode`.
    pub fn new(daemon: std::sync::Weak<Daemon>, origin: Origin, mode: Mode) -> DaemonFleet {
        DaemonFleet {
            daemon,
            origin,
            mode,
        }
    }

    fn daemon(&self) -> Result<Arc<Daemon>, String> {
        self.daemon
            .upgrade()
            .ok_or_else(|| "the daemon is stopping".to_string())
    }

    /// The handle of the device `device` names, or why there is none.
    fn handle(&self, device: &DeviceInfo) -> Result<Arc<crate::device::DeviceHandle>, String> {
        self.daemon()?
            .resolve(Some(&device.name))
            .map_err(|refused| refused.message)
    }

    /// Wait for what `recalling` plans and for how it ends, no longer than [`ACTOR_DEADLINE`] for
    /// each; a plan that never comes, or a batch a later one took the place of, is an error that
    /// calls the batch `what`, a `recall` or a `setting`.
    async fn finish(
        &self,
        device: &DeviceInfo,
        recalling: Recalling,
        what: &str,
    ) -> Result<(Planned, Response), String> {
        let Recalling { planned, done } = recalling;
        let planned = match tokio::time::timeout(ACTOR_DEADLINE, planned).await {
            Ok(Ok(planned)) => planned,
            Ok(Err(_)) => {
                return Err(format!(
                    "a later {what} on {} took this one's place before it started",
                    device.name
                ));
            }
            Err(_) => return Err(silence(&device.name)),
        };
        let response = match tokio::time::timeout(ACTOR_DEADLINE, done).await {
            Ok(Ok(Outcome::Done(response))) => response,
            Ok(Ok(Outcome::Superseded)) => Response::Error {
                code: 3,
                message: format!("a later {what} on {} took this one's place", device.name),
            },
            Ok(Ok(Outcome::Cancelled(why))) => Response::Error {
                code: 3,
                message: why.to_string(),
            },
            Ok(Err(_)) => actor_gone(&device.name),
            Err(_) => actor_silent(&device.name, ACTOR_DEADLINE),
        };
        Ok((planned, response))
    }
}

#[async_trait::async_trait]
impl Fleet for DaemonFleet {
    fn resolve(&self, selector: Option<&str>) -> Result<svolder_ipc::DeviceInfo, String> {
        let daemon = self.daemon()?;
        daemon
            .resolve(selector)
            .map(|device| device.describe())
            .map_err(|mut refused| {
                refused.message = refused
                    .message
                    .replace("pick one with -d", "name one with the device argument");
                let names: Vec<String> = daemon.devices().iter().map(|d| d.name.clone()).collect();
                match names.is_empty() {
                    true => refused.message,
                    false => format!("{}; the devices are: {}", refused.message, names.join(", ")),
                }
            })
    }

    async fn events(&self) -> BoxStream<'static, Event> {
        match self.daemon() {
            Ok(daemon) => daemon.device_events(None),
            Err(_) => stream::empty().boxed(),
        }
    }

    async fn request(&self, request: Request) -> Response {
        match self.daemon() {
            Ok(daemon) => daemon.handle_from(self.origin, request).await,
            Err(message) => Response::Error { code: 3, message },
        }
    }

    /// Glide the volume of `device` to `raw`, waiting no longer than [`ACTOR_DEADLINE`]. A glide
    /// that outlives it is answered with an error but keeps running in the writer, bounded by the
    /// write deadline and the climb's own end.
    async fn glide(&self, device: &svolder_ipc::DeviceInfo, key: &str, raw: i64) -> Glided {
        let handle = match self.daemon() {
            Ok(daemon) => daemon.resolve(Some(&device.name)),
            Err(message) => return Glided::Refused(Response::Error { code: 3, message }),
        };
        let handle = match handle {
            Ok(handle) => handle,
            Err(refused) => return Glided::Refused(refused.into()),
        };
        let glided = handle
            .writer
            .glide(self.origin, key.to_string(), raw, self.mode);
        match tokio::time::timeout(ACTOR_DEADLINE, glided).await {
            Ok(Ok(glided)) => glided,
            Ok(Err(_)) => Glided::Refused(actor_gone(&device.name)),
            Err(_) => Glided::Refused(actor_silent(&device.name, ACTOR_DEADLINE)),
        }
    }

    async fn submit(&self, device: &DeviceInfo, command: Command, options: Options) -> Outcome {
        let handle = match self.daemon() {
            Ok(daemon) => daemon.resolve(Some(&device.name)),
            Err(message) => return Outcome::Done(Response::Error { code: 3, message }),
        };
        let handle = match handle {
            Ok(handle) => handle,
            Err(refused) => return Outcome::Done(refused.into()),
        };
        let answer = handle.writer.submit(self.origin, command, options);
        match tokio::time::timeout(ACTOR_DEADLINE, answer).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => Outcome::Done(actor_gone(&device.name)),
            Err(_) => Outcome::Done(actor_silent(&device.name, ACTOR_DEADLINE)),
        }
    }

    async fn wake(&self, device: &DeviceInfo) -> Response {
        match self.daemon() {
            Ok(daemon) => daemon.dispatch(Some(&device.name), Command::Wake).await,
            Err(message) => Response::Error { code: 3, message },
        }
    }

    /// Recall the preset `name` on `device`, waiting no longer than [`ACTOR_DEADLINE`] for each
    /// of its plan and its end. A recall that outlives it is answered with an error but keeps
    /// running in the writer, bounded by the write deadline.
    async fn recall(
        &self,
        device: &svolder_ipc::DeviceInfo,
        name: &str,
        fitting: Vec<Preset>,
    ) -> Result<(Recalled, Response), String> {
        let handle = self.handle(device)?;
        let recalling = handle.writer.recall(self.origin, name.to_string(), fitting);
        match self.finish(device, recalling, "recall").await? {
            (Planned::Plan(recalled), response) => Ok((recalled, response)),
            (Planned::Setting(_), _) => Err(format!(
                "internal mismatch: the writer of {} planned a built-in setting for a recall",
                device.name
            )),
            (Planned::Refused(why), _) => Err(why),
        }
    }

    /// Apply the built-in setting `name` on `device`, waiting as [`DaemonFleet::recall`] does. A
    /// batch that outlives the wait is answered with an error but keeps running in the writer.
    async fn apply(
        &self,
        device: &svolder_ipc::DeviceInfo,
        name: &str,
    ) -> Result<(Applied, Response), String> {
        let handle = self.handle(device)?;
        let recalling = handle.writer.apply(self.origin, name.to_string());
        match self.finish(device, recalling, "setting").await? {
            (Planned::Setting(applied), response) => Ok((applied, response)),
            (Planned::Plan(_), _) => Err(format!(
                "internal mismatch: the writer of {} planned a recall for a built-in setting",
                device.name
            )),
            (Planned::Refused(why), _) => Err(why),
        }
    }

    fn profile(&self, device: &svolder_ipc::DeviceInfo) -> Option<svolder_profiles::Profile> {
        let daemon = self.daemon().ok()?;
        // The actor's own profile carries the user's input labels; the registry's does not.
        let live = daemon
            .devices()
            .iter()
            .find(|d| d.name == device.name)
            .and_then(|d| d.feed.borrow().profile.clone());
        match live {
            Some(profile) => Some((*profile).clone()),
            None => daemon.registry().get(&device.profile).cloned(),
        }
    }

    fn watch(&self, device: &DeviceInfo) -> Option<Watched> {
        let handle = self.daemon().ok()?.resolve(Some(&device.name)).ok()?;
        Some(Watched {
            feed: handle.feed.clone(),
            climb: handle.writer.climb(),
            recalled: handle.writer.recalled(),
        })
    }

    fn host_view(&self) -> HostView {
        self.daemon()
            .map(|daemon| daemon.host().view())
            .unwrap_or_default()
    }

    fn host_watch(&self, route: bool) -> Option<HostWatch> {
        let daemon = self.daemon().ok()?;
        let host = daemon.host();
        // Held before the receiver is made, so the first read starts as the page's stream does.
        let held = route.then(|| host.watching());
        Some(HostWatch {
            changes: host.changes(),
            _held: held,
        })
    }

    fn host_cover(&self, hash: &str) -> Option<Cover> {
        self.daemon().ok()?.host().cover(hash)
    }

    fn host_frames(&self) -> Option<FramesWatch> {
        Some(self.daemon().ok()?.host().frames()?.events())
    }

    fn config(&self) -> Result<Config, String> {
        let daemon = self.daemon()?;
        Config::load_from(daemon.config_path()).map_err(|e| {
            // The detail can quote a line of the file, a secret among them.
            log::error(&format!(
                "{}: {}: {e}",
                self.origin.word(),
                daemon.config_path().display()
            ));
            "the daemon's configuration could not be read; see its log".to_string()
        })
    }

    fn config_path(&self) -> Option<std::path::PathBuf> {
        Some(self.daemon().ok()?.config_path().to_path_buf())
    }
}
