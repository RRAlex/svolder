// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The task that follows one device for a page, the shown one or a followed one: it keeps what
//! the page was last sent (`Sent`), and sends a part, a component or the whole device when what
//! it says changes.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::response::sse::Event;
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::{
    COMPONENTS_AFTER, Context, FACTS_AFTER, PARTS_AFTER, SNAPSHOT_STRIP, Tracked, followed_models,
    host_changed, send_json, sleep_until, unsent_strip,
};
use crate::fleet::{HostWatch, Watched};
use crate::web::api::{self, Loaded};
use crate::web::model::{self, DeviceModel, Part};
use crate::web::router::devices;
use crate::web::view::{self, Playing, Screen};

/// The parts the stream sends as parts, in the order they go: what no component reads, and not
/// the facts and the Now playing card, which have rules of their own.
const PARTS: [Part; 7] = [
    Part::Link,
    Part::Card,
    Part::Controls,
    Part::Tone,
    Part::Presets,
    Part::DevicePresets,
    Part::Power,
];

/// A device as one read of it has it: what the view models and the model are made of.
pub(super) struct Read {
    pub(super) loaded: Loaded,
    screen: Screen,
}

impl Read {
    /// The device `name` with its presets, read now; `None` when it cannot be read.
    pub(super) fn of(cx: &Context, name: &str) -> Option<Read> {
        Some(Read::from(cx, api::load(&cx.web, name).ok()?))
    }

    /// The device `name` as the components' turns read it: no presets, the configuration file
    /// not read.
    fn live(cx: &Context, name: &str) -> Option<Read> {
        Some(Read::from(cx, api::load_live(&cx.web, name).ok()?))
    }

    fn from(cx: &Context, loaded: Loaded) -> Read {
        let screen = view::screen(&loaded.snapshot(&cx.web, cx.access));
        Read { loaded, screen }
    }

    /// Its model.
    pub(super) fn model(&self) -> DeviceModel {
        model::device_model(&self.screen)
    }
}

/// What the page has been sent of a device, so only what changed goes out.
#[derive(Default)]
pub(super) struct Sent {
    /// The shape of the device the page has, once known.
    shape: Option<String>,
    /// Each part's JSON, by name.
    parts: HashMap<&'static str, String>,
    /// What the facts part last sent said (`Facts::key`), and when it went out.
    facts: Option<String>,
    facts_at: Option<Instant>,
    /// What the Now playing card last sent said (`Playing::key`); `None` before one, or with no
    /// card.
    playing: Option<String>,
    volume: Option<String>,
    /// Each slider's reading, by key.
    levels: HashMap<String, String>,
    meters: Option<String>,
}

impl Sent {
    /// What a page has that was sent `model`, read as `screen`: its shape and its parts, and no
    /// component.
    fn of(screen: &Screen, model: &DeviceModel) -> Sent {
        Sent {
            shape: Some(model.shape.clone()),
            parts: PARTS
                .into_iter()
                .map(|part| (part.name(), model::part_value(model, part).to_string()))
                .collect(),
            facts: Some(screen.facts.key()),
            facts_at: Some(Instant::now()),
            playing: screen.playing.as_ref().map(Playing::key),
            ..Sent::default()
        }
    }
}

/// One event to send: its name and its data.
type Out = (&'static str, String);

/// `value` with the `device` it is of, as an event's data.
fn tagged(device: &str, mut value: Value) -> String {
    if let Value::Object(map) = &mut value {
        map.insert("device".to_string(), device.into());
    }
    value.to_string()
}

/// The `part` event for `name` of the device `device`.
fn part_event(device: &str, name: &str, data: &Value) -> Out {
    (
        "part",
        json!({ "device": device, "name": name, "data": data }).to_string(),
    )
}

/// The parts of `parts` whose JSON is not what `sent` has, as `part` events.
fn parts_out(device: &str, model: &DeviceModel, parts: &[Part], sent: &mut Sent) -> Vec<Out> {
    let mut out = Vec::new();
    for part in parts {
        let value = model::part_value(model, *part);
        let json = value.to_string();
        if sent.parts.get(part.name()) != Some(&json) {
            out.push(part_event(device, part.name(), &value));
            sent.parts.insert(part.name(), json);
        }
    }
    out
}

/// The facts part, when what it says has changed since it was last sent and the last send is
/// more than [`FACTS_AFTER`] old. A change that has to wait goes out on the next turn after
/// that, the second's tick at the latest.
fn facts_out(device: &str, screen: &Screen, model: &DeviceModel, sent: &mut Sent) -> Option<Out> {
    let key = screen.facts.key();
    if sent.facts.as_ref() == Some(&key) {
        return None;
    }
    if sent.facts_at.is_some_and(|at| at.elapsed() < FACTS_AFTER) {
        return None;
    }
    sent.facts = Some(key);
    sent.facts_at = Some(Instant::now());
    Some(part_event(
        device,
        Part::Facts.name(),
        &model::part_value(model, Part::Facts),
    ))
}

/// The Now playing card, when what it says has changed since it was last sent: the card carries
/// the daemon's clock, so a card that says the same is not sent again.
fn playing_out(device: &str, screen: &Screen, model: &DeviceModel, sent: &mut Sent) -> Option<Out> {
    let key = screen.playing.as_ref().map(Playing::key);
    if sent.playing == key {
        return None;
    }
    sent.playing = key;
    Some(part_event(
        device,
        Part::Playing.name(),
        &model::part_value(model, Part::Playing),
    ))
}

/// What the components read, where it is not what `sent` has: the volume, each slider's level
/// and, with `meters`, the meters.
pub(super) fn components_out(
    device: &str,
    model: &DeviceModel,
    meters: bool,
    sent: &mut Sent,
) -> Vec<Out> {
    let mut out = Vec::new();
    let volume = model::part_value(model, Part::Volume);
    if !volume.is_null() {
        let data = tagged(device, volume);
        if sent.volume.as_ref() != Some(&data) {
            sent.volume = Some(data.clone());
            out.push(("volume", data));
        }
    }
    if let Value::Array(controls) = model::part_value(model, Part::Controls) {
        for control in controls.into_iter().filter(|c| c["kind"] == "slider") {
            let key = control["key"].as_str().unwrap_or_default().to_string();
            let data = tagged(device, control);
            if sent.levels.get(&key) != Some(&data) {
                sent.levels.insert(key, data.clone());
                out.push(("level", data));
            }
        }
    }
    let readings = model::part_value(model, Part::Meters);
    if meters && !readings.is_null() {
        let data = tagged(device, readings);
        if sent.meters.as_ref() != Some(&data) {
            sent.meters = Some(data.clone());
            out.push(("meters", data));
        }
    }
    out
}

/// Send `out`. `false` when the page has gone.
async fn send_all(tx: &mpsc::Sender<Event>, out: Vec<Out>) -> bool {
    for (name, data) in out {
        if tx
            .send(Event::default().event(name).data(data))
            .await
            .is_err()
        {
            return false;
        }
    }
    true
}

/// The task of one device.
pub(super) struct Task {
    cx: Arc<Context>,
    /// The device's name, and as a URL segment: what its events carry as `device`.
    name: String,
    url: String,
    /// Whether the page shows it (its shape is the page's `model`), or follows it.
    shown: bool,
    watched: Watched,
    host: Option<HostWatch>,
    sent: Sent,
    /// The newest second the strip has gone out for, the snapshot's included.
    stripped: Option<u64>,
}

impl Task {
    /// The task of `tracked`, which the page has been sent as `read` shows it, or as nothing
    /// when it could not be read.
    pub(super) fn new(
        cx: Arc<Context>,
        tracked: Tracked,
        read: Option<&Read>,
        shown: bool,
    ) -> Task {
        let mut host = cx.web.fleet.host_watch(false);
        if let Some(host) = host.as_mut() {
            host.changes.borrow_and_update();
        }
        Task {
            sent: read.map_or_else(Sent::default, |read| Sent::of(&read.screen, &read.model())),
            url: view::segment(&tracked.info.name),
            name: tracked.info.name,
            watched: tracked.watched,
            cx,
            shown,
            host,
            stripped: None,
        }
    }

    /// Send what the components read, and for the shown device the strip of the last five
    /// minutes. `false` when the page has gone.
    pub(super) async fn open(&mut self, read: Option<&Read>, tx: &mpsc::Sender<Event>) -> bool {
        let Some(read) = read else {
            return true;
        };
        let out = components_out(
            &self.url,
            &read.model(),
            self.cx.needs.meters,
            &mut self.sent,
        );
        if !send_all(tx, out).await {
            return false;
        }
        if self.shown
            && let Some((strip, last)) =
                api::strip_ending(&self.cx.web, &read.loaded, SNAPSHOT_STRIP).await
        {
            self.stripped = Some(last);
            return self.send_strip(&strip, tx).await;
        }
        true
    }

    /// `strip` as an event of this device.
    async fn send_strip(&self, strip: &view::Strip, tx: &mpsc::Sender<Event>) -> bool {
        let data = tagged(
            &self.url,
            serde_json::to_value(strip).unwrap_or(Value::Null),
        );
        send_all(tx, vec![("strip", data)]).await
    }

    /// Send what changes until the page goes, the device does, or a rotated token signs every
    /// browser out (`signed_out` turns).
    pub(super) async fn run(
        mut self,
        mut signed_out: watch::Receiver<u64>,
        tx: mpsc::Sender<Event>,
    ) {
        let second = Duration::from_secs(1);
        let mut second = tokio::time::interval_at(Instant::now() + second, second);
        second.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut parts_due: Option<Instant> = None;
        let mut components_due: Option<Instant> = None;
        let mut components_sent: Option<Instant> = None;
        // What changed between the snapshot the page was sent and now (the route's first read, a
        // reading) was seen by no receiver: it is sent at once.
        if !self.changes(&tx, true).await {
            return;
        }
        loop {
            let wake = parts_due.into_iter().chain(components_due).min();
            tokio::select! {
                biased;
                _ = signed_out.changed() => return,
                () = tx.closed() => return,
                changed = self.watched.feed.changed() => if changed.is_err() { return },
                changed = self.watched.climb.changed() => if changed.is_err() { return },
                changed = self.watched.recalled.changed() => if changed.is_err() { return },
                () = host_changed(&mut self.host) => {},
                () = sleep_until(wake) => {
                    let parts = parts_due.is_some_and(|due| due <= Instant::now());
                    if !self.changes(&tx, parts).await {
                        return;
                    }
                    if parts {
                        parts_due = None;
                    }
                    components_due = None;
                    components_sent = Some(Instant::now());
                    continue;
                }
                _ = second.tick() => {
                    if !self.row_turn(&tx).await || !self.strip_turn(&tx).await {
                        return;
                    }
                    continue;
                }
            }
            // Something changed: the parts gather a while; the components go at once, unless
            // they went less than 50 ms ago, and then when that is up. A step of a climb (one
            // every 80 ms or so) reaches the page as it is reported, instead of waiting out a
            // delay that the parts' turn would cut short.
            let now = Instant::now();
            parts_due.get_or_insert(now + PARTS_AFTER);
            let earliest = components_sent.map_or(now, |sent| (sent + COMPONENTS_AFTER).max(now));
            components_due.get_or_insert(earliest);
        }
    }

    /// Whether the page's shape of the device is not the device's.
    fn stale(&self, read: &Read) -> bool {
        self.sent.shape.as_deref() != Some(read.screen.shape.as_str())
    }

    /// Send the strip's unsent seconds. `false` when the page has gone.
    async fn strip_turn(&mut self, tx: &mpsc::Sender<Event>) -> bool {
        if !self.shown {
            return true;
        }
        match unsent_strip(&self.cx.web, &self.name, &mut self.stripped).await {
            Some(strip) => self.send_strip(&strip, tx).await,
            None => true,
        }
    }

    /// The device's row changes without the feed changing (the seconds to the next attempt, an
    /// attempt under way): once a second, send the pill, the card and the facts as they are now,
    /// or the whole device when its shape has changed. `false` when the page has gone.
    async fn row_turn(&mut self, tx: &mpsc::Sender<Event>) -> bool {
        let Some(read) = Read::live(&self.cx, &self.name) else {
            return true;
        };
        if self.stale(&read) {
            return self.changes(tx, true).await;
        }
        let model = read.model();
        let mut out = parts_out(&self.url, &model, &[Part::Link, Part::Card], &mut self.sent);
        out.extend(facts_out(&self.url, &read.screen, &model, &mut self.sent));
        send_all(tx, out).await
    }

    /// Send what changed since `sent`: what the components read, and with `parts` the parts, or
    /// the whole device when its shape is not the page's. `false` when the page has gone.
    async fn changes(&mut self, tx: &mpsc::Sender<Event>, mut parts: bool) -> bool {
        let cx = Arc::clone(&self.cx);
        // The configuration file is read only on a turn whose parts are due (they show the
        // presets); the components' turns, up to 20 a second, skip it. A turn that finds the
        // shape changed is a parts turn.
        let read = if parts {
            Read::of(&cx, &self.name)
        } else {
            Read::live(&cx, &self.name)
        };
        let Some(mut read) = read else {
            return true;
        };
        if !parts && self.stale(&read) {
            parts = true;
            let Some(loaded) = Read::of(&cx, &self.name) else {
                return true;
            };
            read = loaded;
        }
        let model = read.model();
        let mut out = Vec::new();
        if self.stale(&read) {
            if !self.send_whole(&read, &model, tx).await {
                return false;
            }
            // The components read their state afresh from the new model's events.
            self.sent = Sent::of(&read.screen, &model);
        } else if parts {
            out.extend(parts_out(&self.url, &model, &PARTS, &mut self.sent));
            out.extend(facts_out(&self.url, &read.screen, &model, &mut self.sent));
            out.extend(playing_out(&self.url, &read.screen, &model, &mut self.sent));
        }
        out.extend(components_out(
            &self.url,
            &model,
            cx.needs.meters,
            &mut self.sent,
        ));
        send_all(tx, out).await
    }

    /// Send the device whole: for the shown one the page's model, for a followed one itself.
    async fn send_whole(&self, read: &Read, model: &DeviceModel, tx: &mpsc::Sender<Event>) -> bool {
        let cx = &self.cx;
        if !self.shown {
            let data = serde_json::to_value(model).unwrap_or(Value::Null);
            let whole = json!({ "device": self.url, "data": data });
            return send_all(tx, vec![("device", whole.to_string())]).await;
        }
        let devices = devices(&cx.web).await;
        let mut page = model::page_model(
            &cx.web,
            cx.viewer,
            &devices,
            Some(&read.loaded.info),
            Some(&read.loaded),
            None,
        )
        .await;
        page.followed = followed_models(cx);
        send_json(tx, "model", &page).await.is_ok()
    }
}
