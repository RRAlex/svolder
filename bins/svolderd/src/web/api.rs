// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The private API the page reads and writes, under `api/v1/`: the page's JSON models (a
//! device's, the fleet's and the computer's player) and a device's strip.
//! Scripts use the CLI, the socket, MQTT or MCP; nothing here is promised to stay.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use hyper::{HeaderMap, StatusCode};
use svolder_ipc::host::HostView;
use svolder_ipc::{Config, DeviceInfo, Request, Response as Answer};

use super::model::{self, PlayingModel};
use super::router::{bare, devices, error};
use super::view::{self, Access, Snapshot};
use super::{Viewer, Web, viewer};
use crate::device::Feed;
use crate::writer::ClimbView;

/// The API's routes.
pub(crate) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/devices/{device}/strip", get(strip))
        .route("/api/v1/devices/{device}/model", get(device_model))
        .route("/api/v1/fleet", get(fleet))
        .route("/api/v1/host", get(host))
}

/// The viewer a read may be answered for: signed in, or a guest; 401 with nothing said
/// otherwise.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(crate) fn reader(web: &Web, headers: &HeaderMap) -> Result<Viewer, Response> {
    viewer(web, headers).ok_or_else(|| bare(StatusCode::UNAUTHORIZED))
}

/// What `viewer` may do on `web`'s page.
pub(crate) fn access(web: &Web, viewer: Viewer) -> Access {
    Access {
        writes: viewer == Viewer::Signed && web.config.writes,
        guest: viewer == Viewer::Guest,
    }
}

/// A device as one request reads it.
pub(crate) struct Loaded {
    pub(crate) info: DeviceInfo,
    pub(crate) feed: Feed,
    pub(crate) climb: Option<ClimbView>,
    /// The presets that fit its tone control, each with whether it is a built-in.
    pub(crate) presets: Vec<(svolder_profiles::tone::preset::Preset, bool)>,
    /// Whether the daemon may write its configuration, or why not.
    pub(crate) writable: Result<(), String>,
    /// The computer's player and route, as the daemon says now.
    pub(crate) host: HostView,
}

impl Loaded {
    /// The view models' input, for a viewer with `access`.
    pub(crate) fn snapshot<'a>(&'a self, web: &Web, access: Access) -> Snapshot<'a> {
        Snapshot {
            info: &self.info,
            profile: self.feed.profile.as_deref(),
            readings: &self.feed.readings,
            heard_at: &self.feed.heard_at,
            now_ms: crate::device::wall_ms(),
            climb: self.climb.as_ref(),
            cap_db: web.config.max_volume_db,
            access,
            presets: &self.presets,
            writable: &self.writable,
            host: &self.host,
        }
    }
}

/// The device `name` names for the components' turn of the stream: its row, feed and climb,
/// with no presets and the configuration not read (only the parts show those); 404 saying so
/// when there is none.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(crate) fn load_live(web: &Web, name: &str) -> Result<Loaded, Response> {
    let info = web
        .fleet
        .resolve(Some(name))
        .map_err(|why| error(StatusCode::NOT_FOUND, why))?;
    let watched = web
        .fleet
        .watch(&info)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no device named {name}")))?;
    let feed = watched.feed.borrow().clone();
    let climb = watched.climb.borrow().clone();
    Ok(Loaded {
        info,
        feed,
        climb,
        presets: Vec::new(),
        writable: Ok(()),
        host: web.fleet.host_view(),
    })
}

/// The device `name` names, as it stands now, with the presets that fit its tone control and
/// whether the configuration can be written: one read of the configuration file.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(crate) fn load(web: &Web, name: &str) -> Result<Loaded, Response> {
    let mut device = load_live(web, name)?;
    if let Some(profile) = device.feed.profile.clone() {
        // A configuration that cannot be read leaves the built-ins, which still fit.
        let config = web.fleet.config().unwrap_or_default();
        device.presets = config.fitting_presets(&profile);
    }
    device.writable = web.fleet.config_path().map_or_else(
        || Err("svolder's configuration file is unknown here".to_string()),
        |path| Config::writable(&path),
    );
    Ok(device)
}

/// What `device_model` takes.
#[derive(serde::Deserialize)]
struct Asked {
    /// `1` to carry the strip over the last five minutes.
    strip: Option<String>,
}

/// A device's model, and with `?strip=1` its strip.
async fn device_model(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(asked): Query<Asked>,
) -> Response {
    let viewer = match reader(&web, &headers) {
        Ok(viewer) => viewer,
        Err(refused) => return refused,
    };
    let device = match load(&web, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let model = model::device_model(&view::screen(&device.snapshot(&web, access(&web, viewer))));
    let strip = match asked.strip.as_deref() {
        Some("1") => strip_of(&web, &device, 300).await,
        _ => None,
    };
    axum::Json(serde_json::json!({ "device": model, "strip": strip })).into_response()
}

/// The fleet's model: the devices and the hops after them.
async fn fleet(State(web): State<Web>, headers: HeaderMap) -> Response {
    if let Err(refused) = reader(&web, &headers) {
        return refused;
    }
    axum::Json(model::fleet_model(&web, &devices(&web).await).await).into_response()
}

/// The computer's player and route as a model, or `null` while the daemon does not watch it.
async fn host(State(web): State<Web>, headers: HeaderMap) -> Response {
    let viewer = match reader(&web, &headers) {
        Ok(viewer) => viewer,
        Err(refused) => return refused,
    };
    let host: Option<PlayingModel> = model::host_model(&web, access(&web, viewer));
    axum::Json(host).into_response()
}

/// What `strip` takes.
#[derive(serde::Deserialize)]
struct Span {
    secs: Option<u64>,
}

/// The strip's bars and marks over the last `secs` seconds (300 when not given, at most the
/// ring's half hour), from the daemon's ring.
async fn strip(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(span): Query<Span>,
) -> Response {
    if let Err(refused) = reader(&web, &headers) {
        return refused;
    }
    let device = match load_live(&web, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let secs = span
        .secs
        .unwrap_or(300)
        .clamp(1, svolder_ipc::READINGS_SPAN_SECS);
    match strip_of(&web, &device, secs).await {
        Some(strip) => axum::Json(strip).into_response(),
        None => error(
            StatusCode::CONFLICT,
            "the daemon kept no readings for this device",
        ),
    }
}

/// The strip of `device` over `secs` seconds, from the daemon's ring; `None` while it has no
/// profile or the ring cannot be read.
pub(crate) async fn strip_of(web: &Web, device: &Loaded, secs: u64) -> Option<view::Strip> {
    strip_ending(web, device, secs)
        .await
        .map(|(strip, _)| strip)
}

/// [`strip_of`] and the last whole second it covers, in unix seconds, by the daemon's clock.
pub(crate) async fn strip_ending(
    web: &Web,
    device: &Loaded,
    secs: u64,
) -> Option<(view::Strip, u64)> {
    let profile = device.feed.profile.as_deref()?;
    let Answer::Readings {
        now_ms,
        series,
        recalls,
        link,
        ..
    } = web
        .fleet
        .request(Request::Readings {
            device: Some(device.info.name.clone()),
            span_secs: secs,
        })
        .await
    else {
        return None;
    };
    let strip = view::strip(
        profile,
        &view::Past {
            now_ms,
            series: &series,
            recalls: &recalls,
            link: &link,
        },
        secs,
    );
    Some((strip, view::last_second(now_ms)))
}

/// `value` as JSON that cannot close the `<script>` element it is embedded in: every `<` is
/// written `\u003c`. `null` when it cannot be written.
pub(crate) fn embed<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .map_or_else(|_| "null".to_string(), |json| json.replace('<', "\\u003c"))
}

#[cfg(test)]
mod tests;
