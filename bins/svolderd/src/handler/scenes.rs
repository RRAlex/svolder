// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scenes as the daemon serves them: the `scene` request, and the view every front end
//! follows. The service itself is [`crate::scenes`]; this is its glue to the daemon.

use std::path::PathBuf;
use std::sync::Arc;

use svolder_ipc::Response;
use svolder_ipc::socket::protocol::{SceneOp, ScenesView};
use tokio::sync::watch;

use super::Daemon;
use crate::scenes::{self, Scenes};
use crate::writer::Origin;

impl Daemon {
    /// The daemon keeping the scene last recalled in the file at `path`, which survives a restart,
    /// and starting with what the file holds.
    #[must_use]
    pub fn with_scenes_file(mut self, path: PathBuf) -> Self {
        let scenes = std::mem::replace(&mut self.scenes, Scenes::new());
        self.scenes = scenes.with_file(path);
        self
    }

    /// The scenes service.
    pub(crate) fn scenes(&self) -> &Scenes {
        &self.scenes
    }

    /// This daemon as the shared handle, while it is behind one.
    pub(crate) fn shared_handle(&self) -> Option<Arc<Daemon>> {
        self.me.get().and_then(std::sync::Weak::upgrade)
    }

    /// What the MQTT bridge follows and asks the scenes through, while the daemon runs behind a
    /// shared handle; none for a daemon that does not (a test's), which has no scene entities.
    pub(crate) fn scenes_link(&self) -> Option<crate::mqtt_bridge::ScenesLink> {
        let me = self.me.get()?.clone();
        Some(crate::mqtt_bridge::ScenesLink {
            view: self.scenes_watch(),
            desk: Arc::new(MqttDesk(me)),
        })
    }

    /// The scenes' view as it is now and every change of it.
    pub(crate) fn scenes_watch(&self) -> watch::Receiver<ScenesView> {
        self.scenes.watch(self)
    }

    /// Put `devices` in the running set in place of the actors, for a test whose devices are
    /// stand-ins (a reload keeps one whose name and unit id the config still lists).
    #[cfg(test)]
    pub(crate) fn with_devices(&self, devices: Vec<Arc<crate::device::DeviceHandle>>) {
        crate::locks::written(&self.devices).devices = devices;
    }

    /// Answer one `scene` request from `origin`.
    pub(super) async fn scene(&self, origin: Origin, op: SceneOp) -> Response {
        match op {
            SceneOp::Save { name, force } => scenes::save(self, name, force).await,
            SceneOp::Recall { name, confirm } => scenes::recall(self, origin, name, confirm).await,
            SceneOp::Rename { from, to } => scenes::rename(self, from, to).await,
            SceneOp::Delete { name } => scenes::delete(self, name).await,
        }
    }
}

/// The daemon as the MQTT bridge's scene desk: held weakly, so the bridge never keeps the daemon
/// alive.
struct MqttDesk(std::sync::Weak<Daemon>);

#[async_trait::async_trait]
impl crate::mqtt_bridge::SceneDesk for MqttDesk {
    async fn request(&self, op: SceneOp) -> Response {
        match self.0.upgrade() {
            Some(daemon) => daemon.scene(Origin::Mqtt, op).await,
            None => Response::Error {
                code: svolder_ipc::CODE_LINK,
                message: "the daemon is stopping".to_string(),
                reason: Some(svolder_ipc::socket::protocol::Reason::Stopping),
            },
        }
    }
}
