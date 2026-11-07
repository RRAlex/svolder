// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The page's live updates as JSON: one stream of server-sent events per page, `GET
//! api/v1/events?device=<url>&needs=<meters,host>&follow=<url,url>`. `device` is the URL segment
//! of the device the page shows (`model.device.url`), `follow` the segments of the others it
//! keeps live, separated by commas, `needs` what else it draws from. A segment goes into the
//! query as it is, already percent-encoded.
//!
//! It opens with `retry: 2000` and the page's model (`model`), then the shown device's `volume`,
//! each slider's `level`, its `meters` (with `needs=meters`) and its `strip` of the last five
//! minutes, and the same components of each followed device. Then it sends what changes:
//!
//! - `model`: the shown device's shape changed, so the whole model again;
//! - `device`: a followed device's shape changed: `{device, data}`;
//! - `part`: a part's JSON changed: `{device, name, data}`. The parts gather over 100 ms; the
//!   facts go out when what they say changes, at most once a second, the Now playing card when
//!   what it says changes (not when the clock does: the page counts the track's time itself).
//!   The link and the card go out on the second's turn too, since a device's row changes
//!   without its feed;
//! - `volume`, `level`, `meters`: what a component reads, with the `device` it is of, at most 20
//!   a second;
//! - `strip`: the shown device's bars of each second not sent yet, with the `device`;
//! - `fleet`: the devices' links or families, the list, or the hops changed;
//! - `host` (with `needs=host`): the computer's player changed, with or without a device.
//!
//! A keep-alive comment goes out every 15 s. While the stream lives, with `needs=host` and a device
//! shown, it holds the host's reads on (the route, and the computer's volume). A rotated token ends
//! the stream.
//!
//! - `device`: the task that follows one device.
//! - `page`: the task that follows the fleet and the host.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{RawQuery, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::StreamExt;
use hyper::{HeaderMap, StatusCode};
use serde::Serialize;
use svolder_ipc::DeviceInfo;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_stream::wrappers::ReceiverStream;

use super::api::{self, access, reader};
use super::model::{self, DeviceModel};
use super::router::{devices, error};
use super::view::{self, Access};
use super::{Viewer, Web};
use crate::fleet::{HostWatch, Watched};
use crate::log;
use device::Task;

mod device;
mod page;

/// How long changes gather before the parts they touch are sent.
pub(super) const PARTS_AFTER: Duration = Duration::from_millis(100);
/// The least time between two sends of what the components read: 20 a second. A change after a
/// longer quiet is sent at once.
pub(super) const COMPONENTS_AFTER: Duration = Duration::from_millis(50);
/// The least time between two sends of the facts part: it holds every reading, so a meter's
/// ten changes a second would otherwise redraw a table the page can age by itself.
pub(super) const FACTS_AFTER: Duration = Duration::from_secs(1);
/// How often a comment keeps an idle stream open.
pub(super) const KEEP_ALIVE: Duration = Duration::from_secs(15);
/// The strip the snapshot carries, in seconds.
pub(super) const SNAPSHOT_STRIP: u64 = 300;

/// The stream's route.
pub(crate) fn routes() -> Router<Web> {
    Router::new().route("/api/v1/events", get(events))
}

/// What a page needs besides its devices' own parts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Needs {
    /// The amps' meters, which change many times a second.
    pub(super) meters: bool,
    /// The computer's player.
    pub(super) host: bool,
}

/// What the stream's URL asks for.
#[derive(Debug, Default, PartialEq, Eq)]
struct Asked {
    /// The shown device's URL segment, decoded.
    device: Option<String>,
    needs: Needs,
    /// The followed devices' URL segments, decoded.
    follow: Vec<String>,
}

impl Asked {
    /// The request `query` makes. An unknown `needs` word and a repeated key are ignored; the
    /// commas of `follow` separate segments (a comma in a name is `%2C`).
    fn parse(query: Option<&str>) -> Asked {
        let mut asked = Asked::default();
        for pair in query.unwrap_or_default().split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "device" if !value.is_empty() && asked.device.is_none() => {
                    asked.device = Some(unescaped(value));
                }
                "needs" => {
                    for word in value.split(',') {
                        match word {
                            "meters" => asked.needs.meters = true,
                            "host" => asked.needs.host = true,
                            _ => {}
                        }
                    }
                }
                "follow" if asked.follow.is_empty() => {
                    asked.follow = value
                        .split(',')
                        .filter(|segment| !segment.is_empty())
                        .map(unescaped)
                        .collect();
                }
                _ => {}
            }
        }
        asked
    }
}

/// `segment` with its percent-escapes undone.
fn unescaped(segment: &str) -> String {
    percent_encoding::percent_decode_str(segment)
        .decode_utf8_lossy()
        .into_owned()
}

/// One device the stream follows: its row and its live state.
pub(super) struct Tracked {
    pub(super) info: DeviceInfo,
    pub(super) watched: Watched,
}

/// Who the stream is for and what its page draws: shared by the tasks that follow it.
pub(super) struct Context {
    pub(super) web: Web,
    pub(super) viewer: Viewer,
    pub(super) access: Access,
    pub(super) needs: Needs,
    /// The shown device, whose tone control gives the computer its built-in presets.
    pub(super) shown: Option<DeviceInfo>,
    /// The followed devices' names.
    pub(super) followed: Vec<String>,
}

/// The stream of a page for a viewer who may read it.
async fn events(State(web): State<Web>, headers: HeaderMap, RawQuery(query): RawQuery) -> Response {
    // Subscribed before the cookie is looked at: a token rotated between the two ends the
    // stream, instead of slipping past a check that read the old one.
    let signed_out = web.signed_out.subscribe();
    let viewer = match reader(&web, &headers) {
        Ok(viewer) => viewer,
        Err(refused) => return refused,
    };
    let asked = Asked::parse(query.as_deref());
    let shown = match asked.device.as_deref() {
        Some(name) => match track(&web, name) {
            Ok(tracked) => Some(tracked),
            Err(refused) => return refused,
        },
        None => None,
    };
    let mut followed: Vec<Tracked> = Vec::new();
    for name in &asked.follow {
        let Ok(tracked) = track(&web, name) else {
            continue;
        };
        let again = shown
            .iter()
            .chain(&followed)
            .any(|t| t.info.name == tracked.info.name);
        if !again {
            followed.push(tracked);
        }
    }
    // The computer's controls sit on every shown device's page, a DAC's and an amp's, and the
    // daemon reads the computer's volume only while someone holds it.
    let route = asked.needs.host && shown.is_some();
    let cx = Arc::new(Context {
        access: access(&web, viewer),
        viewer,
        needs: asked.needs,
        shown: shown.as_ref().map(|t| t.info.clone()),
        followed: followed.iter().map(|t| t.info.name.clone()).collect(),
        web,
    });
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(run(cx, shown, followed, route, signed_out, tx));
    let stream = ReceiverStream::new(rx).map(Ok::<_, Infallible>);
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE).text(""))
        .into_response()
}

/// The device `name` and its live state; a 404 saying so when there is none.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn track(web: &Web, name: &str) -> Result<Tracked, Response> {
    let info = web
        .fleet
        .resolve(Some(name))
        .map_err(|why| error(StatusCode::NOT_FOUND, why))?;
    let watched = web
        .fleet
        .watch(&info)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no device named {name}")))?;
    Ok(Tracked { info, watched })
}

/// Send the snapshot, then run a task for each device and one for the page until the page goes
/// or a rotated token signs every browser out (`signed_out` turns).
async fn run(
    cx: Arc<Context>,
    mut shown: Option<Tracked>,
    mut followed: Vec<Tracked>,
    route: bool,
    signed_out: watch::Receiver<u64>,
    tx: mpsc::Sender<Event>,
) {
    let web = &cx.web;
    // A receiver cloned from a writer's channel starts unseen: what it holds now is the
    // snapshot's, not a change.
    for tracked in shown.iter_mut().chain(&mut followed) {
        mark_seen(&mut tracked.watched);
    }
    // Held for as long as the stream lives. What the host holds now is the snapshot's.
    let mut page = page::Task::new(Arc::clone(&cx), route);
    let hello = Event::default()
        .retry(Duration::from_secs(2))
        .comment("svolder");
    if tx.send(hello).await.is_err() {
        return;
    }
    let devices = devices(web).await;
    page.watch_fleet(&devices);
    let shown_view = shown
        .as_ref()
        .and_then(|tracked| device::Read::of(&cx, &tracked.info.name));
    let mut page_model = model::page_model(
        web,
        cx.viewer,
        &devices,
        shown.as_ref().map(|tracked| &tracked.info),
        shown_view.as_ref().map(|read| &read.loaded),
        None,
    )
    .await;
    let followed_views: Vec<Option<device::Read>> = followed
        .iter()
        .map(|tracked| device::Read::of(&cx, &tracked.info.name))
        .collect();
    page_model.followed = followed
        .iter()
        .zip(&followed_views)
        .filter_map(|(tracked, read)| {
            Some((view::segment(&tracked.info.name), read.as_ref()?.model()))
        })
        .collect();
    if send_json(&tx, "model", &page_model).await.is_err() {
        return;
    }
    page.seed(&page_model);
    let mut tasks = JoinSet::new();
    let mut tracked = Vec::new();
    if let Some(shown) = shown {
        tracked.push((shown, shown_view, true));
    }
    tracked.extend(
        followed
            .into_iter()
            .zip(followed_views)
            .map(|(tracked, read)| (tracked, read, false)),
    );
    for (tracked, read, is_shown) in tracked {
        let mut task = Task::new(Arc::clone(&cx), tracked, read.as_ref(), is_shown);
        // The model carries the device's state; its components follow it, then its strip.
        if !task.open(read.as_ref(), &tx).await {
            return;
        }
        tasks.spawn(task.run(signed_out.clone(), tx.clone()));
    }
    // A device's task that panics is logged and the stream goes on; the tasks end with it.
    let panics = async {
        while let Some(ended) = tasks.join_next().await {
            if let Err(e) = ended
                && e.is_panic()
            {
                log::error(&format!("web: a device's event task panicked: {e}"));
            }
        }
        std::future::pending::<()>().await
    };
    tokio::select! {
        () = page.run(signed_out, tx) => {}
        () = panics => {}
    }
}

/// Mark what `watched` holds as seen.
fn mark_seen(watched: &mut Watched) {
    watched.feed.borrow_and_update();
    watched.climb.borrow_and_update();
    watched.recalled.borrow_and_update();
}

/// The followed devices' models, by URL segment, as they read now.
pub(super) fn followed_models(cx: &Context) -> BTreeMap<String, DeviceModel> {
    cx.followed
        .iter()
        .filter_map(|name| {
            let read = device::Read::of(cx, name)?;
            Some((view::segment(name), read.model()))
        })
        .collect()
}

/// `value` as one event named `name`.
pub(super) async fn send_json<T: Serialize>(
    tx: &mpsc::Sender<Event>,
    name: &str,
    value: &T,
) -> Result<(), ()> {
    let json = serde_json::to_string(value).map_err(|_| ())?;
    tx.send(Event::default().event(name).data(json))
        .await
        .map_err(|_| ())
}

/// Wait for the host to change; for ever when there is no host, or once it has gone (its
/// service dropped, which only a daemon that stops does).
pub(super) async fn host_changed(host: &mut Option<HostWatch>) {
    if let Some(watch) = host.as_mut()
        && watch.changes.changed().await.is_ok()
    {
        return;
    }
    *host = None;
    std::future::pending().await
}

/// Wait until `at`, or for ever without one.
pub(super) async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// The strip of the device `name` whose seconds have not gone out yet: those after `stripped`,
/// the newest second sent, up to the last whole second, at most [`SNAPSHOT_STRIP`] of them;
/// `stripped` moves to that last second. `None` when there is nothing new, so a tick that has
/// none sends nothing and two ticks in one wall second send that second once.
pub(super) async fn unsent_strip(
    web: &Web,
    name: &str,
    stripped: &mut Option<u64>,
) -> Option<view::Strip> {
    let device = api::load_live(web, name).ok()?;
    let wall = crate::device::wall_ms() / 1000;
    // From the second the newest sent is in, so the span's start has the points before the first
    // unsent second to pair with (a mark needs two); the seconds already sent are filtered out
    // below. The daemon's clock may be a second ahead of this read of it, which the extra
    // second covers too.
    let asked = stripped
        .map_or(1, |sent| wall.saturating_sub(sent) + 1)
        .min(SNAPSHOT_STRIP);
    let (mut strip, last) = api::strip_ending(web, &device, asked).await?;
    if let Some(sent) = *stripped {
        strip.bars.retain(|bar| bar.t > sent);
        strip.marks.retain(|mark| mark.t > sent);
    }
    *stripped = Some(stripped.map_or(last, |sent| sent.max(last)));
    (!strip.bars.is_empty() || !strip.marks.is_empty()).then_some(strip)
}

#[cfg(test)]
mod tests;
