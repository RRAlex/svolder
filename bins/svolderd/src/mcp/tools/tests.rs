// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use svolder_ipc::{Config, HostEqOp};
use svolder_profiles::Registry;

use super::describe::{level_range, value_of};
use super::*;
use crate::device::Command;
use crate::fleet::Unavailable;
use crate::fleet::Watched;
use crate::recall::{Applied, Recalled};
use crate::writer::ClimbView;
use crate::writer::{Options, Outcome};
use futures::StreamExt;
use svolder_ipc::{CODE_LINK, CODE_TIMEOUT};
use svolder_ipc::{Event, ValueObject};

mod climb;
mod crossfeed;
mod describe;
mod host;
mod identify;
mod loudness;
mod presets;
mod set_control;
mod speakers;

/// What runs with the key of a read.
type ReadHook = Box<dyn Fn(&str) + Send>;

/// A one-device fleet over a built-in profile whose controls read what `readings` says (by
/// key, as raw steps) and which records every `set` it is sent.
struct Scripted {
    readings: StdMutex<HashMap<String, i64>>,
    sets: StdMutex<Vec<(String, String)>>,
    /// The `(key, raw)` of every glide `glide` was sent.
    glides: StdMutex<Vec<(String, i64)>>,
    /// Whether a climb stops where the reading is, as one another front end cut short.
    stuck: StdMutex<bool>,
    /// Whether a read never answers once a climb has begun, as a daemon too busy to.
    deaf: StdMutex<bool>,
    /// Whether a glide has been sent while `deaf` is set.
    gone_deaf: StdMutex<bool>,
    /// The names of the presets `recall` was asked for.
    recalls: StdMutex<Vec<String>>,
    /// The words every read is answered with as an error, when set.
    failing_reads: StdMutex<Option<String>>,
    /// The names of the built-in settings `apply` was asked for.
    applied: StdMutex<Vec<String>>,
    /// Every command `submit` was sent, as the stand-ins log one.
    submitted: StdMutex<Vec<String>>,
    /// The device's memory, as its row reports it.
    recalled: StdMutex<Option<String>>,
    /// The tone control's readings, which `eq_show` answers.
    tone: StdMutex<Vec<ValueObject>>,
    /// The user's presets, which `config` answers.
    presets: StdMutex<Vec<Preset>>,
    /// Every reading the device holds, which `state` answers.
    state: StdMutex<Vec<ValueObject>>,
    /// The signal the device's row carries.
    signal: StdMutex<Option<svolder_ipc::Signal>>,
    /// What an identify is answered with, and how many were sent.
    identify: StdMutex<Option<Response>>,
    identifies: StdMutex<usize>,
    profile: Profile,
    /// The computer, as `Request::Host` answers, and every request that changed it.
    host: StdMutex<svolder_ipc::host::HostView>,
    host_sent: StdMutex<Vec<Request>>,
    host_error: StdMutex<Option<String>>,
    /// The device a preset recall on the computer is refused for, unless it is confirmed.
    played_twice: StdMutex<Option<String>>,
    /// The events `events` hands out, once.
    events: StdMutex<Option<tokio::sync::mpsc::Receiver<Event>>>,
    /// Run with the key of every read the fleet answers, as another front end acting between two
    /// reads of a tool.
    on_read: StdMutex<Option<ReadHook>>,
    /// How many `Request::Get`s were answered.
    gets: StdMutex<usize>,
    /// Whether a climb is shown on the watch and left to the test to carry out and end.
    shown_climb: StdMutex<bool>,
    /// The climb the writer shows: what [`Fleet::watch`] hands out.
    climb: tokio::sync::watch::Sender<Option<ClimbView>>,
    feed: tokio::sync::watch::Sender<crate::device::Feed>,
    /// A second device named `second`, with its profile and feed, listed after the first.
    second: StdMutex<Option<(Profile, tokio::sync::watch::Sender<crate::device::Feed>)>>,
    recalled_watch: tokio::sync::watch::Sender<Option<String>>,
}

fn scripted(profile: &str, readings: &[(&str, i64)]) -> Arc<Scripted> {
    let registry = Registry::builtin().expect("the built-in profiles");
    scripted_over(registry.get(profile).expect("a built-in").clone(), readings)
}

/// [`scripted`] over a profile of the test's own.
fn scripted_over(profile: Profile, readings: &[(&str, i64)]) -> Arc<Scripted> {
    Arc::new(Scripted {
        readings: StdMutex::new(readings.iter().map(|(k, v)| (k.to_string(), *v)).collect()),
        sets: StdMutex::new(Vec::new()),
        glides: StdMutex::new(Vec::new()),
        stuck: StdMutex::new(false),
        deaf: StdMutex::new(false),
        gone_deaf: StdMutex::new(false),
        recalls: StdMutex::new(Vec::new()),
        failing_reads: StdMutex::new(None),
        applied: StdMutex::new(Vec::new()),
        submitted: StdMutex::new(Vec::new()),
        recalled: StdMutex::new(None),
        tone: StdMutex::new(Vec::new()),
        presets: StdMutex::new(Vec::new()),
        state: StdMutex::new(Vec::new()),
        signal: StdMutex::new(None),
        identify: StdMutex::new(None),
        identifies: StdMutex::new(0),
        profile,
        host: StdMutex::new(svolder_ipc::host::HostView::default()),
        host_sent: StdMutex::new(Vec::new()),
        host_error: StdMutex::new(None),
        played_twice: StdMutex::new(None),
        events: StdMutex::new(None),
        on_read: StdMutex::new(None),
        gets: StdMutex::new(0),
        shown_climb: StdMutex::new(false),
        climb: tokio::sync::watch::channel(None).0,
        feed: tokio::sync::watch::channel(Default::default()).0,
        second: StdMutex::new(None),
        recalled_watch: tokio::sync::watch::channel(None).0,
    })
}

impl Scripted {
    /// The scripted reading of `key`, after the read hook has run.
    fn read_hooked(&self, key: &str) -> Option<i64> {
        if let Some(hook) = &*self.on_read.lock().unwrap() {
            hook(key);
        }
        self.readings.lock().unwrap().get(key).copied()
    }
}

#[async_trait]
impl Fleet for Scripted {
    fn resolve(&self, _selector: Option<&str>) -> Result<DeviceInfo, String> {
        Ok(DeviceInfo {
            signal: *self.signal.lock().unwrap(),
            name: "amp".to_string(),
            unit: "0102030405060708".to_string(),
            product_id: 2,
            product: Some("Amp".to_string()),
            firmware: 3,
            profile: self.profile.name.clone(),
            connected: true,
            stale: false,
            retry_in_secs: None,
            passphrase_note: None,
            refused: false,
            recalled: self.recalled.lock().unwrap().clone(),
            key_readings: Vec::new(),
        })
    }

    async fn request(&self, request: Request) -> Response {
        if *self.gone_deaf.lock().unwrap() {
            std::future::pending::<()>().await;
        }
        if matches!(request, Request::Get { .. }) {
            *self.gets.lock().unwrap() += 1;
        }
        match request {
            Request::Get { .. } if self.failing_reads.lock().unwrap().is_some() => Response::error(
                CODE_LINK,
                self.failing_reads.lock().unwrap().clone().unwrap(),
            ),
            Request::Get { key, .. } => match self.read_hooked(&key) {
                Some(raw) => Response::Value(reading(&key, raw)),
                None => Response::error(CODE_TIMEOUT, format!("no value reported yet for {key}")),
            },
            Request::Set { key, value, .. } => {
                self.sets.lock().unwrap().push((key.clone(), value.clone()));
                Response::Value(reading(&key, value.parse().unwrap_or(0)))
            }
            Request::State { .. } => Response::Values {
                device: "amp".into(),
                values: self.state.lock().unwrap().clone(),
            },
            Request::EqShow { .. } => Response::Values {
                device: "amp".into(),
                values: self.tone.lock().unwrap().clone(),
            },
            Request::Devices => {
                let mut devices = vec![self.resolve(None).expect("the one device")];
                if let Some((profile, _)) = &*self.second.lock().unwrap() {
                    devices.push(DeviceInfo {
                        name: "second".to_string(),
                        profile: profile.name.clone(),
                        ..devices[0].clone()
                    });
                }
                Response::Devices { devices }
            }
            Request::Identify { .. } => {
                *self.identifies.lock().unwrap() += 1;
                self.identify
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or(Response::Identify {
                        device: "amp".into(),
                        sent: true,
                    })
            }
            Request::Host => Response::Host(self.host.lock().unwrap().clone()),
            Request::HostEq {
                op: HostEqOp::Preset { confirm: false, .. },
            } if self.played_twice.lock().unwrap().is_some() => {
                let device = self.played_twice.lock().unwrap().clone().unwrap();
                Response::Ask {
                    message: format!(
                        "Warm is on the {device} too: the correction would play twice"
                    ),
                    name: "Warm".to_string(),
                    devices: vec![device],
                }
            }
            request @ (Request::HostVolume { .. }
            | Request::HostMute { .. }
            | Request::HostEq { .. }) => {
                self.host_sent.lock().unwrap().push(request.clone());
                if let Some(message) = self.host_error.lock().unwrap().clone() {
                    return Response::error(CODE_LINK, message);
                }
                let mut view = self.host.lock().unwrap();
                let volume = view.volume.get_or_insert_with(Default::default);
                match request {
                    Request::HostVolume { pct } => volume.pct = u16::from(pct),
                    Request::HostMute { muted } => volume.muted = muted,
                    Request::HostEq {
                        op: HostEqOp::Mode { on },
                    } => view.eq.get_or_insert_with(Default::default).on = on,
                    Request::HostEq {
                        op: HostEqOp::Preset { name, .. },
                    } => {
                        let eq = view.eq.get_or_insert_with(Default::default);
                        eq.on = true;
                        eq.preset = Some(name);
                    }
                    Request::HostEq {
                        op: HostEqOp::Crossfeed { on, model, .. },
                    } => {
                        let eq = view.eq.get_or_insert_with(Default::default);
                        let xf = eq.crossfeed.get_or_insert_with(Default::default);
                        xf.on = on.unwrap_or(xf.on);
                        if let Some(model) = model {
                            xf.model = model;
                        }
                        if on == Some(true) {
                            // The controller's rule: crossfeed and virtual speakers never both play.
                            if let Some(sp) = eq.speakers.as_mut().filter(|sp| sp.on) {
                                sp.on = false;
                                sp.note =
                                    Some("virtual speakers turned off: crossfeed is on".into());
                            }
                        }
                    }
                    Request::HostEq {
                        op: HostEqOp::Speakers { on, set, .. },
                    } => {
                        let eq = view.eq.get_or_insert_with(Default::default);
                        let sp = eq.speakers.get_or_insert_with(Default::default);
                        sp.on = on.unwrap_or(sp.on);
                        if set.is_some() {
                            sp.set = set;
                        }
                        if on == Some(true)
                            && let Some(xf) = eq.crossfeed.as_mut().filter(|xf| xf.on)
                        {
                            xf.on = false;
                            xf.note = Some("crossfeed turned off: virtual speakers are on".into());
                        }
                    }
                    _ => {}
                }
                Response::Ack
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    /// A glide, judged as the device's writer judges it ([`safety::judge`] by
    /// [`safety::Rules::writer`]) on the scripted reading: one that may land at once answers with
    /// its echo, as a set does, and one that climbs moves the reading to its target, unless
    /// `stuck` holds it where it is, as a climb another front end cut short. Every glide is
    /// recorded among the sets too, so a test counting writes counts it.
    async fn glide(&self, _device: &DeviceInfo, key: &str, raw: i64) -> Glided {
        self.glides.lock().unwrap().push((key.to_string(), raw));
        *self.gone_deaf.lock().unwrap() = *self.deaf.lock().unwrap();
        self.sets
            .lock()
            .unwrap()
            .push((key.to_string(), raw.to_string()));
        let now = self.readings.lock().unwrap().get(key).copied();
        let control = self.profile.control_by_key(key).expect("a control");
        let situation = safety::Situation {
            reading: now,
            base: now,
            floor_db: None,
            asked_db: None,
        };
        match safety::judge(control, &situation, raw, &safety::Rules::writer()).motion {
            safety::Motion::Glide { target, .. } => {
                if *self.shown_climb.lock().unwrap() {
                    self.climb.send_replace(Some(ClimbView {
                        key: key.to_string(),
                        target,
                    }));
                } else if !*self.stuck.lock().unwrap() {
                    self.readings
                        .lock()
                        .unwrap()
                        .insert(key.to_string(), target);
                }
                Glided::Climbing {
                    first: None,
                    target,
                }
            }
            _ => Glided::Landed(Response::Value(reading(key, raw))),
        }
    }

    fn watch(&self, device: &DeviceInfo) -> Option<Watched> {
        let feed = match &*self.second.lock().unwrap() {
            Some((_, feed)) if device.name == "second" => feed.subscribe(),
            _ => self.feed.subscribe(),
        };
        Some(Watched {
            feed,
            climb: self.climb.subscribe(),
            recalled: self.recalled_watch.subscribe(),
        })
    }

    async fn recall(
        &self,
        device: &DeviceInfo,
        name: &str,
        fitting: Vec<Preset>,
    ) -> Result<(Recalled, Response), String> {
        self.recalls.lock().unwrap().push(name.to_string());
        crate::recall::plan(&device.name, &self.profile, &fitting, name, |_| None)
            .map(|recalled| (recalled, Response::Ack))
    }

    /// A built-in setting, planned from the scripted readings as the writer plans one; the
    /// readings take its values.
    async fn apply(&self, device: &DeviceInfo, name: &str) -> Result<(Applied, Response), String> {
        self.applied.lock().unwrap().push(name.to_string());
        let readings = self.readings.lock().unwrap().clone();
        let applied = crate::recall::plan_setting(&device.name, &self.profile, name, |id| {
            let control = self.profile.control_by_id(id)?;
            readings.get(&control.key).copied()
        })?;
        for write in &applied.writes {
            self.readings
                .lock()
                .unwrap()
                .insert(write.key.clone(), write.raw);
        }
        Ok((applied, Response::Ack))
    }

    async fn submit(&self, _device: &DeviceInfo, command: Command, _options: Options) -> Outcome {
        self.submitted
            .lock()
            .unwrap()
            .push(crate::test_support::shown(&command));
        Outcome::Done(Response::Ack)
    }

    fn profile(&self, device: &DeviceInfo) -> Option<Profile> {
        match &*self.second.lock().unwrap() {
            Some((profile, _)) if device.name == "second" => Some(profile.clone()),
            _ => Some(self.profile.clone()),
        }
    }

    async fn events(&self) -> BoxStream<'static, Event> {
        match self.events.lock().unwrap().take() {
            Some(rx) => tokio_stream::wrappers::ReceiverStream::new(rx).boxed(),
            None => futures::stream::empty().boxed(),
        }
    }

    fn config(&self) -> Result<std::sync::Arc<Config>, String> {
        Ok(std::sync::Arc::new(Config {
            presets: self.presets.lock().unwrap().clone(),
            ..Config::default()
        }))
    }
}

fn reading(key: &str, raw: i64) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: "10".to_string(),
        key: Some(key.to_string()),
        label: key.to_string(),
        value_type: "u8".to_string(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    }
}

fn call(tool: &str, arguments: Value) -> CallToolRequestParams {
    let mut call = CallToolRequestParams::new(tool.to_string());
    call.arguments = arguments.as_object().cloned();
    call
}

fn set(key: &str, value: Value, confirm: bool) -> CallToolRequestParams {
    call(
        "set_control",
        json!({"key": key, "value": value, "confirm": confirm}),
    )
}

async fn run(
    fleet: &Arc<Scripted>,
    cap: Option<f64>,
    request: CallToolRequestParams,
    can_ask: bool,
) -> CallToolResponse {
    Tools::new(Arc::clone(fleet) as Arc<dyn Fleet>, true, cap)
        .call(request, Caller { can_ask })
        .await
        .expect("an answer")
}

/// A fresh server over `fleet`, to run several calls through one memory of levels.
fn server(fleet: &Arc<Scripted>, cap: Option<f64>) -> Tools {
    Tools::new(Arc::clone(fleet) as Arc<dyn Fleet>, true, cap)
}

fn is_error(response: &CallToolResponse) -> bool {
    matches!(response, CallToolResponse::Complete(r) if r.is_error == Some(true))
}

fn asks(response: &CallToolResponse) -> bool {
    matches!(response, CallToolResponse::InputRequired(_))
}

fn went(response: &CallToolResponse) -> bool {
    matches!(response, CallToolResponse::Complete(r) if r.is_error == Some(false))
}

fn error_words(response: &CallToolResponse) -> String {
    match response {
        CallToolResponse::Complete(r) => format!("{:?}", r.content),
        other => format!("{other:?}"),
    }
}

fn db(profile: &Profile, raw: i64) -> f64 {
    convert::raw_to_display(profile.control_by_key("volume").unwrap(), raw).unwrap()
}

/// The raw step whose level is the highest one at most `rise` dB above step `from`, or, with
/// `more`, the lowest one above that.
fn step_above(profile: &Profile, from: i64, rise: f64, more: bool) -> i64 {
    let base = db(profile, from);
    let candidates = (0..=127).filter(|r| {
        let d = db(profile, *r) - base;
        if more { d > rise } else { d <= rise && d > 0.0 }
    });
    if more {
        candidates.min_by(|a, b| db(profile, *a).total_cmp(&db(profile, *b)))
    } else {
        candidates.max_by(|a, b| db(profile, *a).total_cmp(&db(profile, *b)))
    }
    .expect("a step")
}

/// The tool names a server lists.
fn listed(fleet: &Arc<Scripted>, writes: bool) -> Vec<String> {
    Tools::new(Arc::clone(fleet) as Arc<dyn Fleet>, writes, None)
        .list()
        .iter()
        .map(|t| t.name.to_string())
        .collect()
}

/// The structured answer of a call that went through.
fn structured(response: &CallToolResponse) -> Value {
    match response {
        CallToolResponse::Complete(r) => r.structured_content.clone().expect("a structured answer"),
        other => panic!("not a complete answer: {other:?}"),
    }
}

/// The computer as the host service reports it: 20 % volume, the EQ off with one point.
fn computer_view() -> svolder_ipc::host::HostView {
    use svolder_host::eq::{EqView, PointView};
    svolder_ipc::host::HostView {
        available: true,
        playing: Some(svolder_ipc::host::PlayingView {
            title: "Song".into(),
            artist: "Band".into(),
            album: "Record".into(),
            player: "mpv".into(),
            status: "playing".into(),
            ..Default::default()
        }),
        volume: Some(svolder_ipc::host::HostVolume {
            pct: 20,
            muted: false,
            sink: "Speakers".into(),
        }),
        eq: Some(EqView {
            on: true,
            points: vec![PointView {
                slot: 0,
                kind: "PK".into(),
                f: 1000.0,
                g: -3.0,
                q: 1.0,
            }],
            preamp_db: -3.0,
            preset: Some("Warm".into()),
            max: 16,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn computer_fleet(pct: u16) -> Arc<Scripted> {
    let fleet = scripted("mesh-eq-5", &[]);
    let mut view = computer_view();
    view.volume.as_mut().unwrap().pct = pct;
    *fleet.host.lock().unwrap() = view;
    fleet
}

fn host_sent(fleet: &Scripted) -> Vec<Request> {
    fleet.host_sent.lock().unwrap().clone()
}
