// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The page's model: the viewer, the fleet and its declared hops, the computer's player, and the
//! shown device, as the first snapshot a page carries.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use svolder_host::eq::HostEqState;
use svolder_host::{RouteFacts, SinkFacts};
use svolder_ipc::DeviceInfo;
use svolder_ipc::config::{HopKind, HopWhat};
use svolder_profiles::Family;
use svolder_profiles::tone::preset::same_name;

use super::computer::{ComputerModel, computer_model};
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
    pub(crate) host: Option<HostModel>,
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

/// The computer's player and route: the Now playing card with the route's facts beside it.
#[derive(Debug, Serialize)]
pub(crate) struct HostModel {
    #[serde(flatten)]
    pub(crate) playing: PlayingModel,
    /// What the last read of the audio graph says of the route to the sink; `None` while it is
    /// not read.
    pub(crate) route: Option<RouteModel>,
    /// The computer's own volume and EQ; `None` while the daemon does not serve them.
    pub(crate) computer: Option<ComputerModel>,
}

/// The route from the player to the computer's sink.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RouteModel {
    /// The default sink; `None` when the graph names none.
    pub(crate) sink: Option<SinkModel>,
    /// The sample rate the player's stream plays at.
    pub(crate) player_rate_hz: Option<u32>,
    /// Whether the computer EQ sits in the player's path.
    pub(crate) host_eq: bool,
    /// The applied preset's name: given only while the computer EQ is in the path, on, and in
    /// front of this sink.
    pub(crate) eq_preset: Option<String>,
    /// Whether that preset came from AutoEq (it names a headphone).
    pub(crate) eq_autoeq: bool,
    /// How many other running streams mix into the sink.
    pub(crate) others_mixing: u32,
}

/// The computer's sink.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SinkModel {
    /// What the sink is called for people: its description, else its node name.
    pub(crate) words: String,
    pub(crate) rate_hz: Option<u32>,
    pub(crate) format: Option<String>,
    /// PipeWire's volume as a percentage.
    pub(crate) volume_pct: Option<u32>,
    pub(crate) muted: bool,
}

impl From<&SinkFacts> for SinkModel {
    fn from(sink: &SinkFacts) -> Self {
        SinkModel {
            words: if sink.description.is_empty() {
                sink.name.clone()
            } else {
                sink.description.clone()
            },
            rate_hz: sink.rate_hz,
            format: sink.format.clone(),
            volume_pct: sink.volume_milli.map(|milli| (milli + 5) / 10),
            muted: sink.muted,
        }
    }
}

/// The route's model, naming the preset `eq` says is applied when it is in the player's path
/// in front of this very sink.
pub(crate) fn route_model(route: &RouteFacts, eq: Option<&HostEqState>) -> RouteModel {
    let applied = eq.filter(|eq| {
        route.host_eq
            && eq.on
            && route
                .sink
                .as_ref()
                .is_some_and(|sink| sink.name == eq.target)
    });
    RouteModel {
        sink: route.sink.as_ref().map(Into::into),
        player_rate_hz: route.player.as_ref().and_then(|player| player.rate_hz),
        host_eq: route.host_eq,
        eq_preset: applied.map(|eq| eq.name.clone()),
        eq_autoeq: applied.is_some_and(|eq| eq.origin.is_some()),
        others_mixing: route.others_mixing,
    }
}

/// The computer EQ's saved state, as this machine's `svolder host eq` keeps it; `None` when
/// there is none or it cannot be read (then no preset is named).
pub(super) fn host_eq_state(path: &Path) -> Option<HostEqState> {
    HostEqState::load(path).ok().flatten()
}

/// The route's model as `web`'s daemon has it now; `None` while the route is not read. The
/// state file is read only when the route says the computer EQ is in the path.
pub(crate) fn host_route_model(web: &Web) -> Option<RouteModel> {
    let view = web.fleet.host_view();
    let route = view.route.as_ref()?;
    let eq = if route.host_eq {
        host_eq_state(&svolder_ipc::config::host_eq_state_path())
    } else {
        None
    };
    Some(route_model(route, eq.as_ref()))
}

/// The computer's player and route as `web`'s daemon has them, for a viewer with `access`, with
/// no device to judge the path against: the card, and the route read once. `None` while the host
/// is not watched.
pub(crate) fn host_parts(web: &Web, access: Access) -> Option<(view::Playing, Option<RouteModel>)> {
    let view = web.fleet.host_view();
    let playing = view::playing_of(&view, access, None)?;
    Some((playing, host_route_model(web)))
}

impl HostModel {
    /// The model of `card`, `route` and `computer`, as [`host_parts`] and [`host_computer`] have
    /// them.
    pub(crate) fn new(
        card: &view::Playing,
        route: Option<RouteModel>,
        computer: Option<ComputerModel>,
    ) -> HostModel {
        HostModel {
            playing: card.into(),
            route,
            computer,
        }
    }
}

/// The computer's controls as `web`'s daemon has them, for a page showing `shown`.
pub(crate) fn host_computer(web: &Web, shown: Option<&DeviceInfo>) -> Option<ComputerModel> {
    computer_model(web, &web.fleet.host_view(), shown)
}

/// The computer's player, route and controls as `web`'s daemon has them, for a viewer with
/// `access` on a page showing `shown`, with no device to judge the path against; `None` while
/// the host is not watched.
pub(crate) fn host_model(
    web: &Web,
    access: Access,
    shown: Option<&DeviceInfo>,
) -> Option<HostModel> {
    let (card, route) = host_parts(web, access)?;
    Some(HostModel::new(&card, route, host_computer(web, shown)))
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
        host: host_model(web, access, shown),
        strip,
    }
}
