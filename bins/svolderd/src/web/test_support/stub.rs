// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A fleet of one device whose state a test changes, for the event streams' tests.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use tower::ServiceExt;

use super::{get, open, web};
use crate::device::Feed;
use crate::fleet::{Fleet, Watched};
use crate::recall::Recalled;
use crate::web::router::router;
use crate::writer::{ClimbView, Glided};

/// The readings of a device that had a link: the volume at step 40.
pub(crate) fn had_a_link(profile: &svolder_profiles::Profile) -> Vec<svolder_proto::Message> {
    let id = svolder_profiles::roles::volume(profile)
        .expect("a volume")
        .id;
    vec![svolder_proto::Message::status(
        id,
        svolder_proto::Value::U8(40),
    )]
}

/// A fleet of one device whose state a test changes: its row, its feed and the writer's watches.
pub(crate) struct Stub {
    pub(crate) info: Mutex<svolder_ipc::DeviceInfo>,
    pub(crate) feed: tokio::sync::watch::Sender<Feed>,
    pub(crate) climb: tokio::sync::watch::Sender<Option<ClimbView>>,
    pub(crate) recalled: tokio::sync::watch::Sender<Option<String>>,
    pub(crate) watches: std::sync::atomic::AtomicUsize,
}

impl Stub {
    /// A device named `dac` of the Gungnir 2 profile, connected or not, seen or not.
    pub(crate) fn new(connected: bool, seen: bool) -> Arc<Stub> {
        let registry = svolder_profiles::Registry::builtin().unwrap();
        let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
        let info = svolder_ipc::DeviceInfo {
            signal: None,
            name: "dac".into(),
            unit: "0102030405060708".into(),
            product_id: 2,
            product: None,
            firmware: 3,
            profile: String::new(),
            connected,
            stale: false,
            retry_in_secs: (!connected).then_some(12),
            passphrase_note: None,
            refused: false,
            recalled: None,
        };
        let feed = Feed {
            heard_at: Default::default(),
            name: "dac".into(),
            unit: info.unit.clone(),
            profile: seen.then(|| Arc::new(profile.clone())),
            firmware: None,
            signal: None,
            connected,
            // A device seen and not connected had a link once: its last readings stay.
            readings: if seen && !connected {
                had_a_link(&profile)
            } else {
                Vec::new()
            },
        };
        Arc::new(Stub {
            info: Mutex::new(info),
            feed: tokio::sync::watch::channel(feed).0,
            climb: tokio::sync::watch::channel(None).0,
            recalled: tokio::sync::watch::channel(None).0,
            watches: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// The device reports the volume at `raw`.
    pub(crate) fn read_volume(&self, raw: u8) {
        self.feed.send_modify(|feed| {
            let id = svolder_profiles::roles::volume(feed.profile.as_deref().expect("seen"))
                .expect("a volume")
                .id;
            feed.readings = vec![svolder_proto::Message::status(
                id,
                svolder_proto::Value::U8(raw),
            )];
        });
    }

    /// The device reads through the built-in profile `name` instead.
    pub(crate) fn on_profile(self: Arc<Self>, name: &str) -> Arc<Self> {
        let registry = svolder_profiles::Registry::builtin().unwrap();
        let profile = registry.get(name).expect("a built-in").clone();
        self.feed
            .send_modify(|feed| feed.profile = Some(Arc::new(profile)));
        self
    }

    /// The device reports its first meter at `raw`, and nothing else.
    pub(crate) fn read_meter(&self, raw: u16) {
        self.feed.send_modify(|feed| {
            let id = feed
                .profile
                .as_deref()
                .expect("seen")
                .controls
                .iter()
                .find(|c| c.kind == svolder_profiles::ControlKind::Meter)
                .expect("a meter")
                .id;
            feed.readings = vec![svolder_proto::Message::status(
                id,
                svolder_proto::Value::U16(raw),
            )];
        });
    }

    /// Only the row changes: the seconds to the next attempt, or an attempt under way.
    pub(crate) fn set_retry(&self, secs: Option<u64>) {
        self.info.lock().unwrap().retry_in_secs = secs;
    }

    /// The feed names another profile, as a re-resolved one does.
    pub(crate) fn rename_profile(&self, name: &str) {
        self.feed.send_modify(|feed| {
            let mut profile = (**feed.profile.as_ref().expect("a profile")).clone();
            profile.name = name.to_string();
            feed.profile = Some(Arc::new(profile));
        });
    }

    /// The same device with its profile laid out as an amp, which has a tone control too.
    pub(crate) fn amp_layout(self: Arc<Self>) -> Arc<Self> {
        self.rename_profile("gungnir2-v2");
        self.feed.send_modify(|feed| {
            let mut profile = (**feed.profile.as_ref().expect("a profile")).clone();
            profile.family = svolder_profiles::Family::Amp;
            feed.profile = Some(Arc::new(profile));
        });
        self
    }

    /// The device goes away.
    pub(crate) fn go_away(&self) {
        {
            let mut info = self.info.lock().unwrap();
            info.connected = false;
            info.retry_in_secs = Some(12);
        }
        // It had been connected: its last readings stay (a device never connected has none).
        self.feed.send_modify(|feed| {
            feed.connected = false;
            if feed.readings.is_empty() {
                feed.readings = had_a_link(feed.profile.as_deref().expect("seen"));
            }
        });
    }

    /// The device is seen for the first time, and connects.
    pub(crate) fn appear(&self) {
        let registry = svolder_profiles::Registry::builtin().unwrap();
        let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
        {
            let mut info = self.info.lock().unwrap();
            info.connected = true;
            info.retry_in_secs = None;
        }
        self.feed.send_modify(|feed| {
            feed.connected = true;
            feed.profile = Some(Arc::new(profile));
        });
    }
}

#[async_trait::async_trait]
impl Fleet for Stub {
    fn resolve(&self, selector: Option<&str>) -> Result<svolder_ipc::DeviceInfo, String> {
        let info = self.info.lock().unwrap().clone();
        match selector {
            Some(name) if name != info.name => Err(format!("no device named {name}")),
            _ => Ok(info),
        }
    }

    async fn request(&self, _request: svolder_ipc::Request) -> svolder_ipc::Response {
        svolder_ipc::Response::Error {
            code: 3,
            message: "no daemon".to_string(),
        }
    }

    async fn recall(
        &self,
        _device: &svolder_ipc::DeviceInfo,
        _name: &str,
        _fitting: Vec<svolder_profiles::tone::preset::Preset>,
    ) -> Result<(Recalled, svolder_ipc::Response), String> {
        Err("no daemon".to_string())
    }

    async fn glide(&self, _device: &svolder_ipc::DeviceInfo, _key: &str, _raw: i64) -> Glided {
        Glided::Refused(svolder_ipc::Response::Error {
            code: 3,
            message: "no daemon".to_string(),
        })
    }

    fn profile(&self, _device: &svolder_ipc::DeviceInfo) -> Option<svolder_profiles::Profile> {
        None
    }

    fn watch(&self, _device: &svolder_ipc::DeviceInfo) -> Option<Watched> {
        self.watches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(Watched {
            feed: self.feed.subscribe(),
            climb: self.climb.subscribe(),
            recalled: self.recalled.subscribe(),
        })
    }

    fn config(&self) -> Result<svolder_ipc::Config, String> {
        Ok(svolder_ipc::Config::default())
    }
}

/// A stream of `path` over `fleet`, open to anyone.
pub(crate) async fn open_stream(fleet: Arc<dyn Fleet>, path: &str) -> Body {
    let response = router(web(open(), fleet)).oneshot(get(path)).await.unwrap();
    assert_eq!(response.status(), 200);
    response.into_body()
}
