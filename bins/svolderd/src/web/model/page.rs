// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The page's model: the viewer, the fleet and its declared hops, the computer's player, and the
//! shown device, as the first snapshot a page carries.

use std::collections::BTreeMap;

use serde::Serialize;
use svolder_ipc::DeviceInfo;
use svolder_ipc::config::{HopKind, HopWhat};
use svolder_profiles::Family;
use svolder_profiles::tone::preset::same_name;

use super::{CardModel, DeviceModel, LinkModel, PlayingModel, device_model};
use crate::web::api::{self, Loaded};
use crate::web::view::{self, Access};
use crate::web::writes::READ_ONLY;
use crate::web::{Viewer, Web};

/// The whole page's model, as its first paint reads it.
#[derive(Debug, Serialize)]
pub(crate) struct PageModel {
    /// The model's version.
    pub(crate) v: u8,
    pub(crate) viewer: ViewerModel,
    pub(crate) fleet: FleetModel,
    /// The shown device's URL segment.
    pub(crate) shown: Option<String>,
    pub(crate) device: Option<DeviceModel>,
    /// The other devices the page follows, by URL segment.
    pub(crate) followed: BTreeMap<String, DeviceModel>,
    /// The card for a page with no device to show: none listed, or one that cannot be read.
    pub(crate) card: Option<CardModel>,
    /// The computer's player, when the daemon watches it.
    pub(crate) host: Option<PlayingModel>,
    pub(crate) strip: Option<view::Strip>,
}

/// Who is looking and what they may do.
#[derive(Debug, Serialize)]
pub(crate) struct ViewerModel {
    pub(crate) signed_in: bool,
    pub(crate) guest: bool,
    pub(crate) writes: bool,
    /// Whether the footer offers Sign out: the server asks for a token.
    pub(crate) sign_out: bool,
    /// The daemon's version; absent for a guest.
    pub(crate) version: Option<&'static str>,
    /// Why nothing can be changed, for a signed-in viewer whose page is read-only.
    pub(crate) read_only: Option<&'static str>,
}

/// The viewer `viewer` is on `web`'s page.
fn viewer_model(web: &Web, viewer: Viewer) -> ViewerModel {
    let access = api::access(web, viewer);
    let guest = viewer == Viewer::Guest;
    ViewerModel {
        signed_in: viewer == Viewer::Signed,
        guest,
        writes: access.writes,
        sign_out: !guest && web.config.auth == svolder_ipc::config::WebAuth::Token,
        version: (!guest).then_some(env!("CARGO_PKG_VERSION")),
        read_only: (!guest && !access.writes).then_some(READ_ONLY),
    }
}

/// The devices and the hops after them.
#[derive(Debug, Serialize)]
pub(crate) struct FleetModel {
    pub(crate) devices: Vec<EntryModel>,
    pub(crate) hops: Vec<HopRow>,
}

/// One device in the fleet.
#[derive(Debug, Serialize)]
pub(crate) struct EntryModel {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) dom: String,
    pub(crate) family: &'static str,
    pub(crate) family_word: &'static str,
    pub(crate) link: LinkModel,
}

/// One declared hop after a DAC.
#[derive(Debug, Serialize)]
pub(crate) struct HopRow {
    pub(crate) id: String,
    /// `amp`, `headphones`, `speakers` or `other`.
    pub(crate) kind: &'static str,
    pub(crate) name: String,
    pub(crate) note: Option<String>,
    /// The URL segment of the device whose output it follows: the one it names, else the
    /// default or only device.
    pub(crate) after: Option<String>,
    /// The URL segment of the configured device it is linked to.
    pub(crate) device: Option<String>,
    /// Whether it is linked to a device the daemon lists.
    pub(crate) live: bool,
}

/// The fleet of `devices` and the hops `web`'s configuration declares, in file order.
pub(crate) async fn fleet_model(web: &Web, devices: &[DeviceInfo]) -> FleetModel {
    let listed = |name: &str| devices.iter().find(|d| same_name(&d.name, name));
    let default = web.fleet.resolve(None).ok();
    // A configuration that cannot be read declares no hops.
    let config = web.fleet.config().unwrap_or_default();
    let hops = config
        .hops
        .iter()
        .map(|hop| {
            let linked = match &hop.what {
                HopWhat::Linked { device } => listed(device),
                HopWhat::Declared { .. } => None,
            };
            let kind = match (&hop.what, linked) {
                (HopWhat::Declared { kind, .. }, _) => *kind,
                (HopWhat::Linked { .. }, Some(info))
                    if view::family_of(web, info) == Family::Amp =>
                {
                    HopKind::Amp
                }
                (HopWhat::Linked { .. }, _) => HopKind::Other,
            };
            let after = match &hop.after {
                Some(name) => listed(name),
                None => default.as_ref(),
            };
            HopRow {
                id: hop.id().to_string(),
                kind: kind.name(),
                name: linked.map_or_else(|| hop.id().to_string(), |info| info.name.clone()),
                note: hop.note.clone(),
                after: after.map(|info| view::segment(&info.name)),
                device: linked.map(|info| view::segment(&info.name)),
                live: linked.is_some(),
            }
        })
        .collect();
    FleetModel {
        devices: devices
            .iter()
            .map(|info| {
                let (family, family_word) = view::family_words(view::family_of(web, info));
                EntryModel {
                    name: info.name.clone(),
                    url: view::segment(&info.name),
                    dom: view::dom(&info.name),
                    family,
                    family_word,
                    link: (&view::link_of(info)).into(),
                }
            })
            .collect(),
        hops,
    }
}

/// The computer's player and route as `web`'s daemon has them, for a viewer with `access`, with
/// no device to judge the path against; `None` while the host is not watched.
pub(crate) fn host_model(web: &Web, access: Access) -> Option<PlayingModel> {
    view::playing_of(&web.fleet.host_view(), access, None).map(|card| (&card).into())
}

/// The page for `viewer` showing `shown`, read as `loaded`: the first snapshot, with the strip
/// the page's first paint carries.
pub(crate) async fn page_model(
    web: &Web,
    viewer: Viewer,
    devices: &[DeviceInfo],
    shown: Option<&DeviceInfo>,
    loaded: Option<&Loaded>,
    strip: Option<view::Strip>,
) -> PageModel {
    let access = api::access(web, viewer);
    let device = loaded.map(|loaded| device_model(&view::screen(&loaded.snapshot(web, access))));
    let card = match (&device, shown) {
        (Some(_), _) => None,
        (None, Some(info)) => Some(view::unavailable(&info.name)),
        (None, None) => Some(view::no_devices()),
    };
    PageModel {
        v: 1,
        viewer: viewer_model(web, viewer),
        fleet: fleet_model(web, devices).await,
        shown: shown.map(|info| view::segment(&info.name)),
        device,
        followed: BTreeMap::new(),
        card: card.as_ref().map(Into::into),
        host: host_model(web, access),
        strip,
    }
}
