// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A stand-in for `svolderd` on a socket of a case's own, for every test that talks to a
//! daemon: the backend's, the commands' and the TUI's. It answers over the real server, client
//! and wire format, so what a case checks is this side of the socket.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream::BoxStream;
use futures::{StreamExt, stream};
use svolder_ipc::host::HostView;
use svolder_ipc::{
    CODE_LINK, CODE_NOT_FOUND, CODE_USAGE, Client, DeviceInfo, Event, Handler, LinkChange, Request,
    Response, ValueObject, serve,
};
use tokio::sync::broadcast;

/// The device the stub daemon answers every selector with, by the name it lists it under.
pub(crate) const CANONICAL: &str = "fake";
/// The second device the stub daemon holds, which no case here opens.
pub(crate) const OTHER: &str = "other";
/// The version the stub daemon answers a `version` request with.
pub(crate) const STUB_VERSION: &str = "9.9.9";
/// The link code the stub's web UI mints.
pub(crate) const STUB_LINK_CODE: &str = "c3R1Yi1saW5rLWNvZGUtMDE";
/// The link code the stub's web UI mints once the first one is spent.
pub(crate) const STUB_FRESH_LINK_CODE: &str = "ZnJlc2gtbGluay1jb2RlLTAyMw";
/// The pairing code the stub's web UI mints.
pub(crate) const STUB_PAIR_CODE: &str = "482913";
/// The unit id the stub daemon gives the device it answers for.
pub(crate) const CANONICAL_UNIT: &str = "0102030405060708";
/// The unit id it gives the other device.
const OTHER_UNIT: &str = "0807060504030201";
/// The product both of the stub daemon's devices are, which a selector may name instead
/// of a device.
pub(crate) const PRODUCT: &str = "Gungnir 2";

/// A stand-in daemon: it answers the requests opening a backend makes, acknowledges a
/// `Set`, and publishes that set to whoever is subscribed.
///
/// The daemon proper is a separate binary that a unit test cannot reach. What this covers
/// is the connection bookkeeping on this side of the socket — over the very server, client
/// and wire format the daemon speaks — where a subscription takes a second connection and
/// leaves the request one to the reads and writes.
///
/// It holds two devices and fans every published event out to every subscriber whatever
/// selector was asked for, so what a backend does with an event about the other device is
/// the backend's own doing. It answers every selector with [`CANONICAL`], the name it
/// lists that device under, as the daemon answers a product name or a unit-id prefix with
/// the device's configured name.
pub(crate) struct StubDaemon {
    events: broadcast::Sender<Event>,
    /// The selector each subscription asked for, in the order they were made.
    subscribed: Mutex<Vec<Option<String>>>,
    /// Woken whenever a subscription is made.
    subscribed_wake: tokio::sync::Notify,
    /// The scenes it keeps, as a case scripted them.
    scenes: super::scenes_script::ScenesScript,
    /// The device walk it runs, as a case scripted it.
    walk: super::walk_script::WalkScript,
    /// The AutoEq service it runs, as a case scripted it.
    autoeq: super::autoeq_script::AutoEqScript,
    /// The virtual-speakers sets service it runs, as a case scripted it.
    speakers: super::speakers_script::SpeakersScript,
    /// The `device_config` ops it has been sent, in order.
    device_ops: Mutex<Vec<svolder_ipc::DeviceConfigOp>>,
    /// The configuration file its `device_config` ops are applied to, as the daemon's writers
    /// do, when a case names one.
    device_file: Mutex<Option<PathBuf>>,
    /// What a `device_config` op is refused with instead, when a case sets it.
    device_refusal: Mutex<Option<Response>>,
    /// The `preset_write` ops it has been sent, in order.
    preset_ops: Mutex<Vec<svolder_ipc::PresetOp>>,
    /// The configuration file its `preset_write` ops are applied to, as the daemon's writers do,
    /// when a case names one.
    preset_file: Mutex<Option<PathBuf>>,
    /// What a `preset_write` op is refused with instead, when a case sets it.
    preset_refusal: Mutex<Option<Response>>,
    /// How many times it has been told to read its configuration again.
    reloads: Mutex<usize>,
    /// What a `reload` is answered with instead of success, when a case sets it.
    reload_error: Mutex<Option<String>>,
    /// How long a `reload` takes to answer, when a case sets it.
    reload_delay: Mutex<Option<Duration>>,
    /// The selector each state read and each write named, in the order they arrived: what
    /// says which device a front end is really talking to it about.
    asked_for: Mutex<Vec<Option<String>>>,
    /// The wait before its next attempt at [`OTHER`], while it has lost that device's link:
    /// `None` is a link that is up. Shared, so a case can move it while a front end is
    /// asking.
    other_retry: Arc<Mutex<Option<u64>>>,
    /// Whether it answers as a daemon whose configuration names no device, refusing every
    /// state read the way the daemon proper does.
    empty: std::sync::atomic::AtomicBool,
    /// Whether it has not found [`CANONICAL`] yet: it lists it as not connected, refuses a state
    /// read for it with the words the daemon proper uses (code 3), and says so in the connection
    /// snapshot a subscriber opens with.
    unfound: std::sync::atomic::AtomicBool,
    /// The version it answers a `version` request with.
    version: Mutex<String>,
    /// The features its `version` answer lists.
    features: Mutex<Vec<String>>,
    /// What else a `version` answer says: where its servers stand and the file it reads.
    says: Mutex<Says>,
    /// Whether its `version` answer says the configuration file changed since the last reload.
    config_changed: std::sync::atomic::AtomicBool,
    config_missing: std::sync::atomic::AtomicBool,
    /// The adapter its `version` answer says it holds, and whether it says that is pinned.
    adapter: Mutex<(Option<String>, bool)>,
    /// How many `version` requests it has answered.
    version_asks: std::sync::atomic::AtomicUsize,
    /// The link code a browser has used, which `web_link_open` answers as no longer open and
    /// which `web_link` replaces with [`STUB_FRESH_LINK_CODE`].
    spent_link: Mutex<Option<String>>,
    /// How many `web_link_open` requests it has answered.
    link_open_asks: std::sync::atomic::AtomicUsize,
    /// How many `devices` requests it has answered.
    device_asks: std::sync::atomic::AtomicUsize,
    /// Raised to hang up on every subscriber, as a daemon that exits does.
    hangup: tokio::sync::watch::Sender<bool>,
    /// The link changes it answers a `history` request with; `None` answers as a daemon too
    /// old to know the request, with the unknown-request error.
    history: Mutex<Option<Vec<LinkChange>>>,
    /// The note it lists [`CANONICAL`] with.
    note: Mutex<Option<svolder_ipc::PassphraseNote>>,
    /// The span each `history` request asked for, in seconds, in the order they arrived.
    history_spans: Mutex<Vec<u64>>,
    /// What it answers a `readings` request with; `None` answers as a daemon too old to know it.
    readings: Mutex<Option<Response>>,
    /// The preset its memory holds for [`CANONICAL`], which its device row reports and a `recalled`
    /// request sets.
    recalled: Mutex<Option<String>>,
    /// The wait before its next attempt at [`CANONICAL`], while it has lost that device's link;
    /// `None` is a link that is up.
    canonical_retry: Mutex<Option<u64>>,
    /// What it answers an `identify` request with; `None` answers as a daemon too old to know it.
    identify: Mutex<Option<Response>>,
    /// The host view it answers a `host` request with and opens a host subscription with; `None`
    /// answers as a daemon too old to know the request, and sends no host event.
    host: Mutex<Option<HostView>>,
    /// The cover it serves `host_cover` chunks of, by its hash; `None` answers as a daemon too old
    /// to know the request.
    host_cover: Mutex<Option<(String, svolder_host::cover::Cover)>>,
    /// After this many `host_cover` requests the cover it holds is replaced by this one, as a
    /// player changing track in the middle of a read does.
    host_cover_swap: Mutex<Option<(usize, String, svolder_host::cover::Cover)>>,
    /// The offset of each `host_cover` request, in the order they arrived.
    host_cover_asks: Mutex<Vec<u64>>,
    /// The button of each `host_press` request, in the order they arrived.
    host_presses: Mutex<Vec<String>>,
    /// What a `host_press` request is refused with, when a case sets it.
    host_press_refusal: Mutex<Option<String>>,
    /// The operation of each `host_eq` request, in the order they arrived.
    host_eq_ops: Mutex<Vec<svolder_ipc::HostEqOp>>,
    /// The volume of each `host_volume` request and the state of each `host_mute`, in order.
    host_volume_ops: Mutex<Vec<String>>,
    /// What a `host_eq` request is refused with, when a case sets it.
    host_eq_refusal: Mutex<Option<String>>,
    /// Whether a preset request that does not carry `confirm` is answered with the question a
    /// preset a device plays too asks.
    host_eq_asks: Mutex<bool>,
    /// Whether it asks even a preset request that carries `confirm`.
    host_eq_asks_always: Mutex<bool>,
    /// The server and state of each `server_switch` request, in the order they arrived.
    switches: Mutex<Vec<(svolder_ipc::Server, bool)>>,
    /// The state of each `guest_switch` request, in the order they arrived.
    guest_switches: Mutex<Vec<bool>>,
    /// What a `server_switch` or `guest_switch` request is answered with instead, when a case
    /// sets it.
    switch_refusal: Mutex<Option<Response>>,
    /// Whether an `mqtt_set` of a password answers that it narrowed the file's mode.
    mqtt_tightens: Mutex<bool>,
    /// The op of each `mqtt_set` request, in the order they arrived.
    mqtt_ops: Mutex<Vec<svolder_ipc::MqttOp>>,
    /// Whether a `server_switch` leaves the servers as `version` says them, as a daemon whose
    /// reload has not run yet does.
    switch_stuck: Mutex<bool>,
    /// The configuration file a `server_switch` writes its `enabled` key into, as a daemon does.
    switch_file: Mutex<Option<std::path::PathBuf>>,
    /// Whether each subscription asked for the host's view, in the order they were made.
    host_asked: Mutex<Vec<bool>>,
    /// How many `host` requests it has answered.
    host_snapshots: std::sync::atomic::AtomicUsize,
    /// What the frames of the capture are sent through, once a case turns them on: `None`
    /// answers a `host_frames` request as a daemon from before them, with an error.
    frames: Mutex<Option<broadcast::Sender<Event>>>,
    /// Whether the `state`, `devices` and `host` requests are accepted and never answered, as a
    /// daemon that hangs.
    snapshots_mute: std::sync::atomic::AtomicBool,
    /// Whether a `host_frames` request is accepted and never answered.
    frames_mute: std::sync::atomic::AtomicBool,
    /// The capture state a frames connection opens with.
    frames_capture: Mutex<svolder_ipc::host::CaptureView>,
    /// How many frames connections are open now, and how many `host_frames` requests it has
    /// been asked, answered or refused.
    frames_open: Arc<std::sync::atomic::AtomicUsize>,
    frames_asked: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl Handler for StubDaemon {
    async fn handle(&self, request: Request) -> Response {
        if matches!(request, Request::State { .. } | Request::Host)
            && self
                .snapshots_mute
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            futures::future::pending::<()>().await;
        }
        match request {
            Request::Devices if self.empty.load(std::sync::atomic::Ordering::SeqCst) => {
                Response::Devices {
                    devices: Vec::new(),
                }
            }
            Request::State { .. } if self.empty.load(std::sync::atomic::Ordering::SeqCst) => {
                Response::error(CODE_NOT_FOUND, svolder_ipc::NO_DEVICES_CONFIGURED)
            }
            Request::State { .. } if self.unfound.load(std::sync::atomic::Ordering::SeqCst) => {
                Response::error(CODE_LINK, svolder_ipc::not_found_yet(CANONICAL))
            }
            Request::State { device } => {
                self.asked_for.lock().expect("the record").push(device);
                Response::Values {
                    device: CANONICAL.into(),
                    values: vec![mute("off")],
                }
            }
            Request::Devices => {
                self.device_asks
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let retry = *self.other_retry.lock().expect("the wait");
                let found = !self.unfound.load(std::sync::atomic::Ordering::SeqCst);
                // While the first device is unfound the other has a product of its own, so a
                // product name picks out the one that is waited for.
                let other = DeviceInfo {
                    connected: retry.is_none(),
                    retry_in_secs: retry,
                    passphrase_note: None,
                    refused: false,
                    product: (!found).then(|| "Other Amp".to_string()),
                    ..device_info(OTHER, OTHER_UNIT)
                };
                let waiting = *self.canonical_retry.lock().expect("the wait");
                let canonical = DeviceInfo {
                    connected: found && waiting.is_none(),
                    retry_in_secs: waiting,
                    passphrase_note: *self.note.lock().expect("the note"),
                    refused: false,
                    recalled: self.recalled.lock().expect("the memory").clone(),
                    ..device_info(CANONICAL, CANONICAL_UNIT)
                };
                Response::Devices {
                    devices: vec![canonical, other],
                }
            }
            Request::Set { device, value, .. } => {
                self.asked_for.lock().expect("the record").push(device);
                let _ = self.events.send(Event::Status {
                    device: CANONICAL.into(),
                    reading: mute(&value),
                });
                Response::Ack
            }
            Request::Recalled { name, .. } => {
                *self.recalled.lock().expect("the memory") = Some(name);
                Response::Ack
            }
            Request::History { span_secs, .. } => {
                self.history_spans
                    .lock()
                    .expect("the record")
                    .push(span_secs);
                match self.history.lock().expect("the history").clone() {
                    Some(transitions) => Response::History {
                        device: CANONICAL.into(),
                        now_ms: 0,
                        transitions,
                    },
                    None => Response::unknown_request("history"),
                }
            }
            Request::Readings { .. } => match self.readings.lock().expect("the readings").clone() {
                Some(reply) => reply,
                None => Response::unknown_request("readings"),
            },
            Request::Identify { device } => {
                self.asked_for.lock().expect("the record").push(device);
                match self.identify.lock().expect("the identify answer").clone() {
                    Some(reply) => reply,
                    None => Response::unknown_request("identify"),
                }
            }
            Request::Host => {
                self.host_snapshots
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match self.host.lock().expect("the host view").clone() {
                    Some(view) => Response::Host(view),
                    None => Response::unknown_request("host"),
                }
            }
            Request::HostCover { hash, offset } => {
                let asked = {
                    let mut asks = self.host_cover_asks.lock().expect("the record");
                    asks.push(offset);
                    asks.len()
                };
                {
                    let mut swap = self.host_cover_swap.lock().expect("the swap");
                    if swap.as_ref().is_some_and(|(after, ..)| asked > *after) {
                        let (_, hash, cover) = swap.take().expect("checked above");
                        *self.host_cover.lock().expect("the cover") = Some((hash, cover));
                    }
                }
                match self.host_cover.lock().expect("the cover").as_ref() {
                    None => Response::unknown_request("host_cover"),
                    Some((held, cover)) if *held == hash => {
                        svolder_ipc::cover_chunk(&hash, cover, offset)
                    }
                    Some(_) => {
                        Response::error(svolder_ipc::CODE_NOT_FOUND, "no cover with that hash")
                    }
                }
            }
            Request::HostPress { button } => {
                self.host_presses.lock().expect("the record").push(button);
                match self.host_press_refusal.lock().expect("the refusal").clone() {
                    Some(message) => Response::error(CODE_USAGE, message),
                    None => Response::Ack,
                }
            }
            Request::Walk { op } => self.walk.handle(op),
            Request::AutoEq { op } => self.autoeq.handle(op).await,
            Request::Speakers { op } => self.speakers.handle(op).await,
            Request::DeviceConfig { op } => self.device_config(op),
            Request::PresetWrite { op } => self.preset_write(op),
            Request::Scene { op } => self.scenes.handle(op),
            Request::Reload => {
                *self.reloads.lock().expect("reloads mutex") += 1;
                let delay = *self.reload_delay.lock().expect("the delay");
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
                if let Some(message) = self.reload_error.lock().expect("the refusal").clone() {
                    return Response::error(CODE_USAGE, message);
                }
                Response::Reloaded {
                    devices: vec![CANONICAL.into(), OTHER.into()],
                }
            }
            Request::Version => {
                self.version_asks
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Says {
                    mqtt,
                    mcp,
                    config,
                    token,
                    web,
                    web_token,
                } = self.says.lock().expect("the status").clone();
                let (adapter, adapter_pinned) = self.adapter.lock().expect("the adapter").clone();
                Response::Version {
                    version: self.version.lock().expect("the version").clone(),
                    mqtt,
                    mcp,
                    config,
                    log: None,
                    mcp_token: token,
                    web,
                    web_token,
                    features: self.features.lock().expect("the features").clone(),
                    config_changed: self
                        .config_changed
                        .load(std::sync::atomic::Ordering::SeqCst),
                    config_missing: self
                        .config_missing
                        .load(std::sync::atomic::Ordering::SeqCst),
                    adapter,
                    adapter_pinned,
                    activity: None,
                }
            }
            Request::WebLinkOpen { code } => {
                self.link_open_asks
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let listening = matches!(
                    self.says.lock().expect("the status").web,
                    Some(svolder_ipc::WebStatus::Listening { .. })
                );
                match listening {
                    false => Response::error(CODE_USAGE, svolder_ipc::WEB_OFF.to_string()),
                    true => Response::WebLinkOpen {
                        open: self.spent_link.lock().expect("the spent link").as_deref()
                            != Some(code.as_str()),
                    },
                }
            }
            Request::WebLink | Request::WebPair => {
                let listening = matches!(
                    self.says.lock().expect("the status").web,
                    Some(svolder_ipc::WebStatus::Listening { .. })
                );
                match (listening, matches!(request, Request::WebLink)) {
                    (false, _) => Response::error(CODE_USAGE, svolder_ipc::WEB_OFF.to_string()),
                    (true, true) => Response::WebLink {
                        code: match self.spent_link.lock().expect("the spent link").as_deref() {
                            Some(STUB_LINK_CODE) => STUB_FRESH_LINK_CODE.to_string(),
                            _ => STUB_LINK_CODE.to_string(),
                        },
                        expires_in_secs: 120,
                    },
                    (true, false) => Response::WebPair {
                        code: STUB_PAIR_CODE.to_string(),
                        expires_in_secs: 120,
                    },
                }
            }
            Request::HostVolume { pct } => {
                self.host_volume_ops
                    .lock()
                    .expect("the record")
                    .push(format!("volume {pct}"));
                Response::Ack
            }
            Request::HostMute { muted } => {
                self.host_volume_ops
                    .lock()
                    .expect("the record")
                    .push(format!("muted {muted}"));
                Response::Ack
            }
            Request::HostEq { op } => {
                let asked = match &op {
                    svolder_ipc::HostEqOp::Preset { confirm, .. } => {
                        *self.host_eq_asks.lock().expect("the question")
                            && (!confirm || *self.host_eq_asks_always.lock().expect("the question"))
                    }
                    _ => false,
                };
                self.host_eq_ops.lock().expect("the record").push(op);
                if asked {
                    return Response::Ask {
                        message: "Warm is on the Bifrost 3 too: the correction would play twice"
                            .to_string(),
                        name: "Warm".to_string(),
                        devices: vec!["Bifrost 3".to_string()],
                    };
                }
                match self.host_eq_refusal.lock().expect("the refusal").clone() {
                    Some(message) => Response::error(CODE_USAGE, message),
                    None => Response::Ack,
                }
            }
            Request::ServerSwitch { server, enabled } => {
                self.switches
                    .lock()
                    .expect("the record")
                    .push((server, enabled));
                if let Some(refusal) = self.switch_refusal.lock().expect("the refusal").clone() {
                    return refusal;
                }
                if let Some(path) = self.switch_file.lock().expect("the file").clone() {
                    let _ = match server {
                        svolder_ipc::Server::Web => {
                            svolder_ipc::Config::update_web(&path, |web| web.enabled = enabled)
                        }
                        svolder_ipc::Server::Mcp => {
                            svolder_ipc::Config::update_mcp(&path, |mcp| mcp.enabled = enabled)
                        }
                    };
                }
                let mut says = self.says.lock().expect("the status");
                if !*self.switch_stuck.lock().expect("the flag") {
                    match server {
                        svolder_ipc::Server::Web => {
                            says.web = Some(match enabled {
                                true => svolder_ipc::WebStatus::Listening {
                                    listen: ([127, 0, 0, 1], 8738).into(),
                                    auth: svolder_ipc::config::WebAuth::Token,
                                    writes: true,
                                    guest: svolder_ipc::config::WebGuest::None,
                                },
                                false => svolder_ipc::WebStatus::Off,
                            });
                        }
                        svolder_ipc::Server::Mcp => {
                            says.mcp = Some(match enabled {
                                true => svolder_ipc::McpStatus::Listening {
                                    listen: ([127, 0, 0, 1], 8737).into(),
                                    auth: svolder_ipc::config::McpAuth::Token,
                                    writes: false,
                                },
                                false => svolder_ipc::McpStatus::Off,
                            });
                        }
                    }
                }
                Response::ServerSwitch {
                    web: says.web.clone().unwrap_or(svolder_ipc::WebStatus::Off),
                    mcp: says.mcp.clone().unwrap_or(svolder_ipc::McpStatus::Off),
                }
            }
            Request::MqttSet { op } => {
                self.mqtt_ops.lock().expect("the record").push(op.clone());
                if let Some(refusal) = self.switch_refusal.lock().expect("the refusal").clone() {
                    return refusal;
                }
                let mut says = self.says.lock().expect("the status");
                if let svolder_ipc::MqttOp::Enabled { enabled } = op {
                    says.mqtt = Some(match enabled {
                        true => svolder_ipc::MqttStatus::Connected {
                            host: "stub".to_string(),
                            port: 1883,
                            commands: false,
                        },
                        false => svolder_ipc::MqttStatus::Off,
                    });
                }
                Response::MqttSet {
                    status: says.mqtt.clone().unwrap_or(svolder_ipc::MqttStatus::Off),
                    tightened: *self.mqtt_tightens.lock().expect("the flag")
                        && matches!(op, svolder_ipc::MqttOp::Password { value: Some(_) }),
                }
            }
            Request::GuestSwitch { read } => {
                self.guest_switches.lock().expect("the record").push(read);
                if let Some(refusal) = self.switch_refusal.lock().expect("the refusal").clone() {
                    return refusal;
                }
                let mut says = self.says.lock().expect("the status");
                if !*self.switch_stuck.lock().expect("the flag")
                    && let Some(svolder_ipc::WebStatus::Listening { guest, .. }) = says.web.as_mut()
                {
                    *guest = match read {
                        true => svolder_ipc::config::WebGuest::Read,
                        false => svolder_ipc::config::WebGuest::None,
                    };
                }
                Response::ServerSwitch {
                    web: says.web.clone().unwrap_or(svolder_ipc::WebStatus::Off),
                    mcp: says.mcp.clone().unwrap_or(svolder_ipc::McpStatus::Off),
                }
            }
            other => Response::error(CODE_USAGE, format!("{other:?} is not answered here")),
        }
    }

    /// Every subscriber opens with a connection snapshot, as the daemon's own does: a
    /// reader that has had it knows the server is pumping this stream, so nothing
    /// published after that is missed.
    async fn subscribe(&self, device: Option<String>) -> BoxStream<'static, Event> {
        self.device_stream(device, false)
    }

    /// A host subscription also opens with the host view, when the daemon has one; the frames
    /// of the capture are given once a case turned them on.
    async fn stream(&self, request: Request) -> Result<BoxStream<'static, Event>, Box<Response>> {
        match request {
            Request::Subscribe { device, host } => Ok(self.device_stream(device, host)),
            Request::Walk {
                op: svolder_ipc::WalkOp::Follow,
            } => Ok(self.walk.follow()),
            // A daemon streams AutoEq's jobs only once a case scripted the service, which lists
            // the feature; before that it is as a daemon from before the family.
            Request::AutoEq {
                op: svolder_ipc::AutoEqOp::Follow,
            } if self
                .features
                .lock()
                .expect("the features")
                .iter()
                .any(|feature| feature == "autoeq") =>
            {
                match self.autoeq.follow_refused() {
                    true => Err(Box::new(Response::error(CODE_USAGE, "no stream today"))),
                    false => Ok(self.autoeq.follow()),
                }
            }
            Request::Speakers {
                op: svolder_ipc::SpeakersOp::Follow,
            } => Ok(self.speakers.follow()),
            Request::HostFrames => self
                .open_frames()
                .await
                .map_err(|message| Box::new(Response::error(CODE_USAGE, message))),
            other => Err(Box::new(Response::error(
                CODE_USAGE,
                format!("this daemon does not stream `{}`", other.cmd()),
            ))),
        }
    }
}

impl StubDaemon {
    /// Answer a `preset_write` op: record it, apply it to the file a case named (a reload,
    /// counted), or refuse it as the case said.
    fn preset_write(&self, op: svolder_ipc::PresetOp) -> Response {
        use svolder_ipc::{Config, PresetOp};
        self.preset_ops.lock().expect("the record").push(op.clone());
        if let Some(refusal) = self.preset_refusal.lock().expect("the refusal").clone() {
            return refusal;
        }
        let Some(path) = self.preset_file.lock().expect("the file").clone() else {
            return Response::Ack;
        };
        let (name, wrote) = match op {
            PresetOp::Save { preset, force } => (
                preset.name.clone(),
                match force {
                    true => Config::update_preset(&path, &preset.name, &preset),
                    false => Config::add_preset(&path, &preset),
                },
            ),
            PresetOp::Update { preset } => (
                preset.name.clone(),
                Config::update_preset(&path, &preset.name, &preset),
            ),
            PresetOp::Rename { from, to } => {
                let wrote = Config::rename_preset(&path, &from, &to);
                (to, wrote)
            }
            PresetOp::Delete { name } => {
                let wrote = Config::remove_preset(&path, &name);
                (name, wrote)
            }
        };
        match wrote {
            Ok(()) => {
                *self.reloads.lock().expect("reloads mutex") += 1;
                Response::PresetWritten { name }
            }
            Err(e) => Response::error(CODE_USAGE, e.to_string()).with_reason(e.reason()),
        }
    }

    /// Answer a `device_config` op: record it, apply it to the file a case named (a reload,
    /// counted), or refuse it as the case said.
    fn device_config(&self, op: svolder_ipc::DeviceConfigOp) -> Response {
        use svolder_ipc::{Config, DeviceConfigOp};
        self.device_ops.lock().expect("the record").push(op.clone());
        if let Some(refusal) = self.device_refusal.lock().expect("the refusal").clone() {
            return refusal;
        }
        if let Some(path) = self.device_file.lock().expect("the file").clone() {
            let wrote = match op {
                DeviceConfigOp::Update {
                    device,
                    name,
                    profile,
                    passphrase,
                } => Config::patch_device(&path, &device, |current| {
                    let mut next = current.clone();
                    if let Some(name) = name {
                        next.name = name;
                    }
                    if let Some(profile) = profile {
                        next.profile = profile;
                    }
                    if let Some(passphrase) = passphrase {
                        next.passphrase = passphrase;
                    }
                    Ok(next)
                }),
                DeviceConfigOp::Remove { device } => Config::remove_selected_device(&path, &device),
                DeviceConfigOp::SetDefault { device } => Config::default_to_device(&path, &device),
                DeviceConfigOp::ClearDefault => Config::set_default_device(&path, None),
                DeviceConfigOp::List | DeviceConfigOp::Add { .. } => Ok(()),
            };
            if let Err(e) = wrote {
                return Response::error(CODE_USAGE, e.to_string());
            }
            *self.reloads.lock().expect("reloads mutex") += 1;
        }
        // The devices as the file holds them (none when no file was named), as the daemon lists.
        let devices = match self.device_file.lock().expect("the file").clone() {
            Some(path) => Config::load_from(&path)
                .map(|config| {
                    config
                        .devices
                        .iter()
                        .map(|device| svolder_ipc::ConfiguredDevice {
                            unit: device.udid.clone(),
                            name: device.name.clone(),
                            profile: device.profile.clone(),
                            passphrase: svolder_ipc::PassphraseKind::of(&device.passphrase),
                            default: config
                                .default_device
                                .as_deref()
                                .is_some_and(|name| name.eq_ignore_ascii_case(&device.name)),
                            connected: false,
                            not_connected_since_ms: None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            None => Vec::new(),
        };
        Response::DeviceConfig {
            devices,
            profiles: Vec::new(),
        }
    }

    /// The frames of the capture, once a case turned them on: the capture's state, then whatever
    /// the case sends, until the connection is dropped. Counted while it lives.
    async fn open_frames(&self) -> Result<BoxStream<'static, Event>, String> {
        self.frames_asked
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.frames_mute.load(std::sync::atomic::Ordering::SeqCst) {
            futures::future::pending::<()>().await;
        }
        let Some(sender) = self.frames.lock().expect("the frames").clone() else {
            return Err("unknown request `host_frames`".to_string());
        };
        let first = Event::HostCapture(self.frames_capture.lock().expect("the capture").clone());
        let rx = sender.subscribe();
        self.frames_open
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let live = LiveConnection(Arc::clone(&self.frames_open));
        Ok(Box::pin(stream::once(async move { first }).chain(
            stream::unfold((rx, live), |(mut rx, live)| async move {
                rx.recv().await.ok().map(|event| (event, (rx, live)))
            }),
        )))
    }
}

/// One open frames connection, counted until it is dropped.
struct LiveConnection(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for LiveConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl StubDaemon {
    /// The stream of a subscription, with the host's view after the snapshot when it asked.
    fn device_stream(&self, device: Option<String>, host: bool) -> BoxStream<'static, Event> {
        self.subscribed
            .lock()
            .expect("subscribed mutex")
            .push(device);
        self.subscribed_wake.notify_waiters();
        self.host_asked.lock().expect("the record").push(host);
        let rx = self.events.subscribe();
        let mut hung_up = self.hangup.subscribe();
        let hello = Event::Connection {
            device: CANONICAL.into(),
            connected: !self.unfound.load(std::sync::atomic::Ordering::SeqCst),
        };
        let view = match host {
            true => self.host.lock().expect("the host view").clone(),
            false => None,
        };
        Box::pin(
            stream::once(async move { hello })
                .chain(stream::iter(view.map(Event::Host)))
                .chain(stream::iter(self.scenes.opening()))
                .chain(stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.ok().map(|event| (event, rx))
                }))
                .take_until(async move {
                    let _ = hung_up.wait_for(|hung_up| *hung_up).await;
                }),
        )
    }
}

/// One device the stub daemon holds, as it describes it.
fn device_info(name: &str, unit: &str) -> DeviceInfo {
    DeviceInfo {
        signal: None,
        name: name.into(),
        unit: unit.into(),
        product_id: 2,
        product: Some(PRODUCT.into()),
        firmware: 3,
        profile: "gungnir2-v2".into(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
        key_readings: Vec::new(),
        adapter_hint: None,
    }
}

/// One reading of the Mute control, as the daemon describes it.
pub(crate) fn mute(display: &str) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: "5".into(),
        key: Some("mute".into()),
        label: "Mute".into(),
        value_type: "bool".into(),
        raw: serde_json::Value::Bool(display == "on"),
        display: display.into(),
        value: None,
        unit: None,
    }
}

/// What the stub's `version` answer says besides the version.
#[derive(Clone, Default)]
struct Says {
    mqtt: Option<svolder_ipc::MqttStatus>,
    mcp: Option<svolder_ipc::McpStatus>,
    config: Option<String>,
    token: Option<svolder_ipc::TokenInfo>,
    web: Option<svolder_ipc::WebStatus>,
    web_token: Option<svolder_ipc::TokenInfo>,
}

/// A stub daemon on a socket of its own, shut down and cleaned up when the case drops it.
pub(crate) struct StubServer {
    pub(crate) socket: PathBuf,
    handler: Arc<StubDaemon>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl StubServer {
    /// Apply every `device_config` op to the file at `path`, as the daemon's writers do (each a
    /// reload), and list the `device_crud` feature.
    pub(crate) fn applies_device_ops_to(&self, path: &Path) {
        *self.handler.device_file.lock().expect("the file") = Some(path.to_path_buf());
        self.lists_features(&["device_crud"]);
    }

    /// Refuse every `device_config` op with `answer`, and list the `device_crud` feature.
    pub(crate) fn refuses_device_ops(&self, answer: Response) {
        *self.handler.device_refusal.lock().expect("the refusal") = Some(answer);
        self.lists_features(&["device_crud"]);
    }

    /// Apply every `preset_write` op to the file at `path`, as the daemon's writers do (each a
    /// reload), and list the `preset_write` feature.
    pub(crate) fn applies_preset_ops_to(&self, path: &Path) {
        *self.handler.preset_file.lock().expect("the file") = Some(path.to_path_buf());
        self.lists_features(&["preset_write"]);
    }

    /// Refuse every `preset_write` op with `answer`, and list the `preset_write` feature.
    pub(crate) fn refuses_preset_ops(&self, answer: Response) {
        *self.handler.preset_refusal.lock().expect("the refusal") = Some(answer);
        self.lists_features(&["preset_write"]);
    }

    /// The `preset_write` ops the daemon has been sent, in order.
    pub(crate) fn preset_ops(&self) -> Vec<svolder_ipc::PresetOp> {
        self.handler.preset_ops.lock().expect("the record").clone()
    }

    /// The `device_config` ops the daemon has been sent, in order.
    pub(crate) fn device_ops(&self) -> Vec<svolder_ipc::DeviceConfigOp> {
        self.handler.device_ops.lock().expect("the record").clone()
    }

    /// The scenes the daemon keeps, as a case scripts them: its view, which every subscription
    /// opens with, and the answers to the `scene` requests. The daemon lists the `scenes`
    /// feature from now on.
    pub(crate) fn scenes(&self) -> &super::scenes_script::ScenesScript {
        let mut features = self.handler.features.lock().expect("the features");
        if !features
            .iter()
            .any(|feature| feature == svolder_ipc::FEATURE_SCENES)
        {
            features.push(svolder_ipc::FEATURE_SCENES.to_string());
        }
        &self.handler.scenes
    }

    /// The device walk the daemon runs, as a case scripts it.
    pub(crate) fn walk(&self) -> &super::walk_script::WalkScript {
        &self.handler.walk
    }

    /// The AutoEq service the daemon runs, as a case scripts it; the daemon lists the `autoeq`
    /// feature from now on.
    pub(crate) fn autoeq(&self) -> &super::autoeq_script::AutoEqScript {
        let mut features = self.handler.features.lock().expect("the features");
        if !features.iter().any(|feature| feature == "autoeq") {
            features.push("autoeq".to_string());
        }
        &self.handler.autoeq
    }

    /// The virtual-speakers sets service the daemon runs, as a case scripts it.
    pub(crate) fn speakers(&self) -> &super::speakers_script::SpeakersScript {
        &self.handler.speakers
    }

    /// How many times the daemon has been told to read its configuration again.
    /// How many times it has been told to read its configuration again.
    pub(crate) fn reloads(&self) -> usize {
        *self.handler.reloads.lock().expect("reloads mutex")
    }

    /// Take `delay` to answer every `reload` from now on.
    pub(crate) fn delays_reloads(&self, delay: Duration) {
        *self.handler.reload_delay.lock().expect("the delay") = Some(delay);
    }

    /// Answer every `reload` from now on with this error.
    pub(crate) fn refuses_reloads(&self, message: &str) {
        *self.handler.reload_error.lock().expect("the refusal") = Some(message.to_string());
    }

    /// The selector every state read and every write named, in order.
    pub(crate) fn asked_for(&self) -> Vec<Option<String>> {
        self.handler.asked_for.lock().expect("the record").clone()
    }

    /// Where the daemon keeps the wait before its next attempt at its other device: set it
    /// to a number of seconds and that device's link is down and being waited on, to `None`
    /// and the link is up.
    pub(crate) fn other_retry(&self) -> Arc<Mutex<Option<u64>>> {
        Arc::clone(&self.handler.other_retry)
    }

    /// Hold `name` as the preset last recalled on [`CANONICAL`], as the daemon's memory does.
    pub(crate) fn recalled(&self, name: &str) {
        *self.handler.recalled.lock().expect("the memory") = Some(name.to_string());
    }

    /// The preset its memory holds for [`CANONICAL`].
    pub(crate) fn memory(&self) -> Option<String> {
        self.handler.recalled.lock().expect("the memory").clone()
    }

    /// Push a `recalled` event for [`CANONICAL`] to every subscriber.
    pub(crate) fn push_recalled(&self, name: &str) {
        let _ = self.handler.events.send(Event::Recalled {
            device: CANONICAL.into(),
            name: name.into(),
        });
    }

    /// List the daemon's device with this passphrase note from now on.
    pub(crate) fn notes(&self, note: Option<svolder_ipc::PassphraseNote>) {
        *self.handler.note.lock().expect("the note") = note;
    }

    /// Answer a `history` request with `changes` from now on, as a daemon that knows the
    /// request does.
    pub(crate) fn history(&self, changes: Vec<LinkChange>) {
        *self.handler.history.lock().expect("the history") = Some(changes);
    }

    /// Answer a `readings` request with `reply` from now on.
    pub(crate) fn readings(&self, reply: Response) {
        *self.handler.readings.lock().expect("the readings") = Some(reply);
    }

    /// List [`CANONICAL`] as not connected, with `secs` to the daemon's next attempt at it.
    pub(crate) fn canonical_waits(&self, secs: u64) {
        *self.handler.canonical_retry.lock().expect("the wait") = Some(secs);
    }

    /// Answer an `identify` request with `reply` from now on, as a daemon that knows the request
    /// does.
    pub(crate) fn identifies(&self, reply: Response) {
        *self.handler.identify.lock().expect("the identify answer") = Some(reply);
    }

    /// Answer a `host` request with `view` from now on and open every host subscription with it,
    /// as a daemon with a host side does.
    pub(crate) fn hosts(&self, view: HostView) {
        *self.handler.host.lock().expect("the host view") = Some(view);
    }

    /// Serve `cover` under `hash` to `host_cover` requests from now on, as a daemon with a host
    /// side that holds a cover does.
    pub(crate) fn serves_cover(&self, hash: &str, cover: svolder_host::cover::Cover) {
        {
            let mut features = self.handler.features.lock().expect("the features");
            if !features.iter().any(|feature| feature == "host_cover") {
                features.push("host_cover".to_string());
            }
        }
        *self.handler.host_cover.lock().expect("the cover") = Some((hash.to_string(), cover));
    }

    /// Replace the served cover by `cover` under `hash` once `after` `host_cover` requests have
    /// been answered.
    pub(crate) fn swaps_cover_after(
        &self,
        after: usize,
        hash: &str,
        cover: svolder_host::cover::Cover,
    ) {
        *self.handler.host_cover_swap.lock().expect("the swap") =
            Some((after, hash.to_string(), cover));
    }

    /// The offset of every `host_cover` request, in order.
    pub(crate) fn host_cover_asks(&self) -> Vec<u64> {
        self.handler
            .host_cover_asks
            .lock()
            .expect("the record")
            .clone()
    }

    /// Answer a `host_frames` request with a stream that opens with `capture` and carries what
    /// [`StubServer::sends_frame`] and [`StubServer::sends_capture`] send, as a daemon with host
    /// audio does.
    pub(crate) fn frames(&self, capture: svolder_ipc::host::CaptureView) {
        *self.handler.frames_capture.lock().expect("the capture") = capture;
        *self.handler.frames.lock().expect("the frames") = Some(broadcast::channel(64).0);
    }

    /// Accept `state`, `devices` and `host` requests from now on and never answer them, as a
    /// daemon that hangs.
    pub(crate) fn mutes_snapshots(&self) {
        self.handler
            .snapshots_mute
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Accept `host_frames` requests from now on and never answer them, as a daemon that hangs.
    pub(crate) fn mutes_frames(&self) {
        self.handler
            .frames_mute
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Send `frame` down every open frames connection.
    pub(crate) fn sends_frame(&self, frame: svolder_ipc::host::FrameView) {
        self.sends(Event::HostFrame(frame));
    }

    /// Send a change of the capture's state down every open frames connection, and open the
    /// next ones with it.
    pub(crate) fn sends_capture(&self, capture: svolder_ipc::host::CaptureView) {
        *self.handler.frames_capture.lock().expect("the capture") = capture.clone();
        self.sends(Event::HostCapture(capture));
    }

    fn sends(&self, event: Event) {
        if let Some(sender) = self.handler.frames.lock().expect("the frames").as_ref() {
            let _ = sender.send(event);
        }
    }

    /// How many frames connections are open now.
    pub(crate) fn frames_open(&self) -> usize {
        self.handler
            .frames_open
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many `host_frames` requests the daemon has been asked, answered or refused.
    pub(crate) fn frames_asked(&self) -> usize {
        self.handler
            .frames_asked
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Refuse every `host_press` request from now on with `message`.
    pub(crate) fn refuses_presses(&self, message: &str) {
        *self.handler.host_press_refusal.lock().expect("the refusal") = Some(message.to_string());
    }

    /// The button of every `host_press` request, in order.
    pub(crate) fn host_presses(&self) -> Vec<String> {
        self.handler
            .host_presses
            .lock()
            .expect("the record")
            .clone()
    }

    /// Answer a `host_eq` preset request that has no `confirm` with the question a preset that a
    /// device plays too asks, from now on.
    pub(crate) fn asks_before_a_preset(&self) {
        *self.handler.host_eq_asks.lock().expect("the question") = true;
    }

    /// Ask even after a yes: a daemon that answers the same question again.
    pub(crate) fn asks_after_a_yes_too(&self) {
        *self
            .handler
            .host_eq_asks_always
            .lock()
            .expect("the question") = true;
    }

    /// Refuse every `host_eq` request from now on with `message`.
    pub(crate) fn refuses_eq(&self, message: &str) {
        *self.handler.host_eq_refusal.lock().expect("the refusal") = Some(message.to_string());
    }

    /// Every `server_switch` request, in order: the server and the state asked for.
    pub(crate) fn switches(&self) -> Vec<(svolder_ipc::Server, bool)> {
        self.handler.switches.lock().expect("the record").clone()
    }

    /// Answer every `mqtt_set` of a password as one that narrowed the config file's mode.
    pub(crate) fn tightens_on_password(&self) {
        *self.handler.mqtt_tightens.lock().expect("the flag") = true;
    }

    /// Every `mqtt_set` request, in order.
    pub(crate) fn mqtt_ops(&self) -> Vec<svolder_ipc::MqttOp> {
        self.handler.mqtt_ops.lock().expect("the record").clone()
    }

    /// Say `status` for the MQTT client in `version` and in every `mqtt_set` answer from now on.
    pub(crate) fn says_mqtt(&self, status: svolder_ipc::MqttStatus) {
        self.handler.says.lock().expect("the status").mqtt = Some(status);
    }

    /// Every `guest_switch` request, in order: whether guests were asked to read.
    pub(crate) fn guest_switches(&self) -> Vec<bool> {
        self.handler
            .guest_switches
            .lock()
            .expect("the record")
            .clone()
    }

    /// Answer every `server_switch` and `guest_switch` request with `answer` from now on.
    pub(crate) fn answers_switch(&self, answer: Response) {
        *self.handler.switch_refusal.lock().expect("the refusal") = Some(answer);
    }

    /// Have every `server_switch` write its `enabled` key into the file at `path`, as a daemon
    /// does before it answers.
    pub(crate) fn switches_write(&self, path: &std::path::Path) {
        *self.handler.switch_file.lock().expect("the file") = Some(path.to_path_buf());
    }

    /// Answer every `server_switch` request with the servers as they stood before it.
    pub(crate) fn switches_late(&self) {
        *self.handler.switch_stuck.lock().expect("the flag") = true;
    }

    /// Every `host_volume` and `host_mute` request, in order, as `volume 40` and `muted true`.
    pub(crate) fn host_volume_ops(&self) -> Vec<String> {
        self.handler
            .host_volume_ops
            .lock()
            .expect("the record")
            .clone()
    }

    /// The operation of every `host_eq` request, in order.
    pub(crate) fn host_eq_ops(&self) -> Vec<svolder_ipc::HostEqOp> {
        self.handler.host_eq_ops.lock().expect("the record").clone()
    }

    /// How many `host` requests the daemon has answered.
    pub(crate) fn host_snapshots(&self) -> usize {
        self.handler
            .host_snapshots
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether each subscription made so far asked for the host's view.
    pub(crate) fn host_asked(&self) -> Vec<bool> {
        self.handler.host_asked.lock().expect("the record").clone()
    }

    /// The span each `history` request asked for, in seconds.
    pub(crate) fn history_spans(&self) -> Vec<u64> {
        self.handler
            .history_spans
            .lock()
            .expect("the record")
            .clone()
    }

    /// Answer a `version` request with this MCP status and configuration path from now on.
    pub(crate) fn says(&self, mcp: Option<svolder_ipc::McpStatus>, config: Option<&str>) {
        let mut says = self.handler.says.lock().expect("the status");
        says.mcp = mcp;
        says.config = config.map(str::to_string);
    }

    /// Have the daemon's `version` answer say, or stop saying, that its file changed.
    pub(crate) fn config_changed(&self, changed: bool) {
        self.handler
            .config_changed
            .store(changed, std::sync::atomic::Ordering::SeqCst);
    }

    /// Say in the `version` answer that the daemon holds `adapter`, `pinned` by its command line
    /// or not.
    pub(crate) fn holds_adapter(&self, adapter: &str, pinned: bool) {
        *self.handler.adapter.lock().expect("the adapter") = (Some(adapter.to_string()), pinned);
    }

    /// Whether the daemon's `version` reply says its configuration file is gone.
    pub(crate) fn config_missing(&self, missing: bool) {
        self.handler
            .config_missing
            .store(missing, std::sync::atomic::Ordering::SeqCst);
    }

    /// How many `version` requests the daemon has answered.
    pub(crate) fn version_asks(&self) -> usize {
        self.handler
            .version_asks
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many `devices` requests the daemon has answered.
    pub(crate) fn device_asks(&self) -> usize {
        self.handler
            .device_asks
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Answer a `version` request with this token description from now on.
    pub(crate) fn says_token(&self, token: Option<svolder_ipc::TokenInfo>) {
        self.handler.says.lock().expect("the status").token = token;
    }

    /// Answer a `version` request with this web UI status and web token from now on; a web UI
    /// that listens answers the two code requests too.
    pub(crate) fn says_web(
        &self,
        web: Option<svolder_ipc::WebStatus>,
        token: Option<svolder_ipc::TokenInfo>,
    ) {
        let mut says = self.handler.says.lock().expect("the status");
        says.web = web;
        says.web_token = token;
    }

    /// Answer `web_link_open` about `code` as used from now on, and mint [`STUB_FRESH_LINK_CODE`]
    /// where it minted [`STUB_LINK_CODE`].
    pub(crate) fn spends_link(&self, code: &str) {
        *self.handler.spent_link.lock().expect("the spent link") = Some(code.to_string());
    }

    /// How many `web_link_open` requests the daemon has answered.
    pub(crate) fn link_open_asks(&self) -> usize {
        self.handler
            .link_open_asks
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Answer a `version` request with `version` from now on.
    /// List `features` in the `version` answer from now on, as a daemon that has them does.
    pub(crate) fn lists_features(&self, features: &[&str]) {
        *self.handler.features.lock().expect("the features") =
            features.iter().map(|f| (*f).to_string()).collect();
    }

    pub(crate) fn version(&self, version: &str) {
        *self.handler.version.lock().expect("the version") = version.to_string();
    }

    /// Stop as a daemon does when it exits: hang up on every subscriber, let the socket go,
    /// and answer once nothing is bound there any more.
    pub(crate) async fn stop(mut self) {
        let _ = self.handler.hangup.send(true);
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        for _ in 0..500 {
            if !self.socket.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Stop listening, as a daemon does that is going away: the socket is let go, so a new
    /// connection is refused, while every subscription already open goes on.
    pub(crate) async fn stop_listening(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        for _ in 0..500 {
            if !self.socket.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Answer as a daemon that has not found its device yet while `on`, and as one that has
    /// once it is off.
    pub(crate) fn unfound(&self, on: bool) {
        self.handler
            .unfound
            .store(on, std::sync::atomic::Ordering::SeqCst);
    }

    /// Publish that the link to the daemon's device is up, as the daemon does when it finds it.
    pub(crate) fn finds_it(&self) {
        self.unfound(false);
        self.publish(Event::Connection {
            device: CANONICAL.into(),
            connected: true,
        });
    }

    /// Answer from now on as a daemon whose configuration names no device.
    pub(crate) fn empty(&self) {
        self.handler
            .empty
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Wait until a subscription has been made.
    pub(crate) async fn wait_subscribed(&self) {
        loop {
            let woken = self.handler.subscribed_wake.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            if !self.subscribed().is_empty() {
                return;
            }
            woken.await;
        }
    }

    /// The selector every subscription made so far asked for.
    pub(crate) fn subscribed(&self) -> Vec<Option<String>> {
        self.handler
            .subscribed
            .lock()
            .expect("subscribed mutex")
            .clone()
    }

    /// Publish one event to every subscriber, whichever device it is about.
    pub(crate) fn publish(&self, event: Event) {
        self.handler.events.send(event).expect("a subscriber reads");
    }
}

impl Drop for StubServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Serve a stub daemon on a fresh socket and wait for it to answer.
///
/// The counter is what makes the name unique: the clock alone can read the same instant
/// twice for two of these cases starting side by side, and one socket shared between two
/// stubs is one stub answering both cases — each then sees the other's subscriptions.
pub(crate) async fn stub_daemon() -> StubServer {
    stub_daemon_at(fresh_socket()).await
}

/// A socket path a case owns, removed when the case lets go of it.
pub(crate) struct SocketFile(pub(crate) PathBuf);

impl Drop for SocketFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl std::ops::Deref for SocketFile {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

/// A socket path no other case has, short whatever `TMPDIR` is.
pub(crate) fn fresh_socket() -> PathBuf {
    super::short_socket("svolder-remote")
}

/// Serve a stub daemon on `socket` and wait for it to answer: a second daemon on the socket
/// of one that has stopped is a restart.
pub(crate) async fn stub_daemon_at(socket: PathBuf) -> StubServer {
    let handler = Arc::new(StubDaemon {
        events: broadcast::channel(16).0,
        subscribed: Mutex::default(),
        subscribed_wake: tokio::sync::Notify::new(),
        scenes: super::scenes_script::ScenesScript::default(),
        walk: super::walk_script::WalkScript::default(),
        autoeq: super::autoeq_script::AutoEqScript::default(),
        speakers: super::speakers_script::SpeakersScript::default(),
        device_ops: Mutex::default(),
        device_file: Mutex::new(None),
        device_refusal: Mutex::new(None),
        preset_ops: Mutex::default(),
        preset_file: Mutex::new(None),
        preset_refusal: Mutex::new(None),
        reloads: Mutex::default(),
        reload_error: Mutex::new(None),
        reload_delay: Mutex::new(None),
        asked_for: Mutex::default(),
        other_retry: Arc::default(),
        empty: std::sync::atomic::AtomicBool::new(false),
        unfound: std::sync::atomic::AtomicBool::new(false),
        version: Mutex::new(STUB_VERSION.into()),
        features: Mutex::new(Vec::new()),
        says: Mutex::default(),
        config_changed: std::sync::atomic::AtomicBool::new(false),
        config_missing: std::sync::atomic::AtomicBool::new(false),
        adapter: Mutex::new((None, false)),
        version_asks: std::sync::atomic::AtomicUsize::new(0),
        spent_link: Mutex::new(None),
        link_open_asks: std::sync::atomic::AtomicUsize::new(0),
        device_asks: std::sync::atomic::AtomicUsize::new(0),
        hangup: tokio::sync::watch::channel(false).0,
        history: Mutex::new(None),
        note: Mutex::new(None),
        history_spans: Mutex::default(),
        readings: Mutex::new(None),
        recalled: Mutex::new(None),
        canonical_retry: Mutex::new(None),
        identify: Mutex::new(None),
        host: Mutex::new(None),
        host_cover: Mutex::new(None),
        host_cover_swap: Mutex::new(None),
        host_cover_asks: Mutex::new(Vec::new()),
        host_presses: Mutex::default(),
        host_press_refusal: Mutex::new(None),
        host_eq_ops: Mutex::default(),
        host_volume_ops: Mutex::default(),
        host_eq_refusal: Mutex::new(None),
        host_eq_asks: Mutex::new(false),
        host_eq_asks_always: Mutex::new(false),
        switches: Mutex::default(),
        guest_switches: Mutex::default(),
        switch_refusal: Mutex::new(None),
        mqtt_ops: Mutex::default(),
        mqtt_tightens: Mutex::new(false),
        switch_stuck: Mutex::new(false),
        switch_file: Mutex::new(None),
        host_asked: Mutex::default(),
        host_snapshots: std::sync::atomic::AtomicUsize::new(0),
        frames: Mutex::new(None),
        frames_capture: Mutex::new(svolder_ipc::host::CaptureView {
            state: svolder_ipc::host::CaptureKind::Running,
            sink: Some("USB DAC".into()),
            why: None,
        }),
        snapshots_mute: std::sync::atomic::AtomicBool::new(false),
        frames_mute: std::sync::atomic::AtomicBool::new(false),
        frames_open: Arc::default(),
        frames_asked: std::sync::atomic::AtomicUsize::new(0),
    });
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let path = socket.clone();
    let served = Arc::clone(&handler);
    let mut serving = tokio::spawn(async move {
        serve(&path, served, async {
            let _ = rx.await;
        })
        .await
    });
    // A stub that is not up fails the case here: handed on, it is a daemon a case waits on for
    // ever (a job that never starts, a cancel that never arrives).
    for _ in 0..500 {
        if Client::connect(&socket).await.is_ok() {
            return StubServer {
                socket,
                handler,
                shutdown: Some(tx),
            };
        }
        if serving.is_finished() {
            let why = match (&mut serving).await {
                Ok(Err(e)) => e.to_string(),
                Ok(Ok(())) => "it stopped".into(),
                Err(e) => e.to_string(),
            };
            panic!(
                "the stub daemon could not serve on {} ({} bytes): {why}",
                socket.display(),
                socket.as_os_str().len()
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the stub daemon never answered on {}", socket.display());
}

mod tests;
