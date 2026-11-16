// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The private API the page reads and writes, under `api/v1/`: the page's JSON models (a
//! device's, the fleet's and the computer's player), a device's strip and its series.
//! Scripts use the CLI, the socket, MQTT or MCP; nothing here is promised to stay.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use hyper::{HeaderMap, StatusCode};
use svolder_ipc::host::HostView;
use svolder_ipc::{Config, DeviceInfo, Request, Response as Answer};
use svolder_profiles::tone::preset::Preset;

use super::model::{self, HostModel};
use super::router::{bare, devices, error, refusal};
use super::view::{self, Access, Snapshot};
use super::{Viewer, Web, viewer};
use crate::device::Feed;
use crate::writer::ClimbView;

/// The API's routes.
pub(crate) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/devices/{device}/strip", get(strip))
        .route("/api/v1/devices/{device}/series", get(series))
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

/// [`load`] on a blocking thread: it reads the configuration file, which a slow file system
/// must not make an async worker wait for.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(crate) async fn load_off_thread(web: &Web, name: &str) -> Result<Loaded, Response> {
    let web = web.clone();
    let name = name.to_string();
    let loaded = match tokio::task::spawn_blocking(move || load(&web, &name)).await {
        Ok(loaded) => loaded,
        Err(_) => {
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the device could not be read",
            ));
        }
    }?;
    // The page model reads which preset is on, which fits the imported ones: made here, off the
    // runtime's threads, so that reading only finds them kept.
    if let Some(eq) = loaded
        .feed
        .profile
        .as_ref()
        .and_then(|profile| profile.eq.as_ref())
    {
        let listed: Vec<Preset> = loaded.presets.iter().map(|(p, _)| p.clone()).collect();
        crate::recall::warm(eq, &listed).await;
    }
    Ok(loaded)
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
    let device = match load_off_thread(&web, &name).await {
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

/// What `host` takes: the device whose tone control gives the computer's built-in presets.
#[derive(serde::Deserialize)]
struct Shown {
    device: Option<String>,
}

/// The computer's player, route and controls as a model, or `null` while the daemon does not
/// watch it. `?device=<url>` names the shown device, whose own built-in presets the computer
/// lists.
async fn host(State(web): State<Web>, headers: HeaderMap, Query(shown): Query<Shown>) -> Response {
    let viewer = match reader(&web, &headers) {
        Ok(viewer) => viewer,
        Err(refused) => return refused,
    };
    let shown = shown
        .device
        .and_then(|name| web.fleet.resolve(Some(&name)).ok());
    let host: Option<HostModel> =
        model::host_model(&web, access(&web, viewer), shown.as_ref()).await;
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

/// The most ids one `series` request may name, each once: more than any profile has readings to
/// graph, so one answer holds at most this many series of the ring's 1800 points.
pub(super) const SERIES_IDS_MAX: usize = 32;

/// What `series` takes.
#[derive(serde::Deserialize)]
struct SeriesAsked {
    /// The ids, separated by commas; the device's key readings when not given.
    ids: Option<String>,
    secs: Option<u64>,
}

/// One series of a `series` answer.
#[derive(serde::Serialize)]
struct SeriesModel {
    /// The id as the daemon names it: `signal`, or a control's key.
    id: String,
    /// The unit of the points, where the route knows one: `dBm` for the signal.
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<&'static str>,
    /// How the samples of one bin were joined.
    aggregate: svolder_profiles::history::Aggregate,
    /// `[unix milliseconds, value]`, oldest first. A control's series starts with its newest
    /// point from before the span, which held when the span began, and its values are raw
    /// readings, a bool 0 or 1; the signal's has only what was heard inside the span.
    points: Vec<(u64, f64)>,
}

/// The series of a device's readings over the last `secs` seconds (the ring's half hour when
/// not given, and at most that), from the daemon's ring, which keeps one point a second:
/// `?ids=signal,volume` names them, each `signal` (the signal strengths heard, in dBm) or the
/// key of a control of the device's profile, in any letter case; without `ids` the device's key
/// readings are answered. An id that names neither is listed in `refused` and the rest are
/// served; a series the ring has no point of is served empty. An id asked more than once is
/// answered once, and more than [`SERIES_IDS_MAX`] distinct ids are a 400 that says so, so one
/// request never costs more than that many series of the ring. Open to whoever may read the
/// page, as the strip is.
async fn series(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(asked): Query<SeriesAsked>,
) -> Response {
    if let Err(refused) = reader(&web, &headers) {
        return refused;
    }
    let device = match load_live(&web, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let secs = asked
        .secs
        .unwrap_or(svolder_ipc::READINGS_SPAN_SECS)
        .clamp(1, svolder_ipc::READINGS_SPAN_SECS);
    // Each id once, the first of its spellings, in the order asked: a repeat adds nothing to
    // the answer, and the distinct ids are few enough to compare with each other.
    let mut ids: Vec<String> = Vec::new();
    let asked_ids = match asked.ids.as_deref() {
        Some(ids) => ids.split(',').map(str::trim).map(str::to_string).collect(),
        None => device.info.key_readings.clone(),
    };
    for id in asked_ids {
        if id.is_empty() || ids.iter().any(|had| had.eq_ignore_ascii_case(&id)) {
            continue;
        }
        if ids.len() == SERIES_IDS_MAX {
            return error(
                StatusCode::BAD_REQUEST,
                format!("one request names at most {SERIES_IDS_MAX} ids"),
            );
        }
        ids.push(id);
    }
    let (now_ms, bin_ms, kept, link, signal) = match web
        .fleet
        .request(Request::Readings {
            device: Some(device.info.name.clone()),
            span_secs: secs,
        })
        .await
    {
        Answer::Readings {
            now_ms,
            bin_ms,
            series,
            link,
            signal,
            ..
        } => (now_ms, bin_ms, series, link, signal),
        // The device went since it was loaded, or the daemon is stopping: its own words.
        other => return refusal(other),
    };
    let profile = device.feed.profile.as_deref();
    let (mut series, mut refused) = (Vec::new(), Vec::new());
    for id in ids {
        if id.eq_ignore_ascii_case(svolder_ipc::SERIES_SIGNAL) {
            series.push(SeriesModel {
                id: svolder_ipc::SERIES_SIGNAL.to_string(),
                unit: Some("dBm"),
                aggregate: svolder_profiles::history::Aggregate::Last,
                points: signal.clone(),
            });
            continue;
        }
        let Some((profile, control)) =
            profile.and_then(|profile| Some((profile, profile.control_by_key(&id)?)))
        else {
            refused.push(id);
            continue;
        };
        let found = kept
            .iter()
            .find(|series| series.key.as_deref() == Some(control.key.as_str()));
        series.push(SeriesModel {
            id: control.key.clone(),
            unit: None,
            aggregate: found.map_or_else(
                || svolder_profiles::history::aggregate_of(profile, control.id),
                |series| series.aggregate,
            ),
            points: found
                .map(|series| series.points.clone())
                .unwrap_or_default(),
        });
    }
    axum::Json(serde_json::json!({
        "now_ms": now_ms,
        "bin_ms": bin_ms,
        "secs": secs,
        "series": series,
        "refused": refused,
        "link": link,
    }))
    .into_response()
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
