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
    Client, DeviceInfo, Event, Handler, LinkChange, Request, Response, ValueObject, serve,
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
    /// How many times it has been told to read its configuration again.
    reloads: Mutex<usize>,
    /// What a `reload` is answered with instead of success, when a case sets it.
    reload_error: Mutex<Option<String>>,
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
    /// What else a `version` answer says: where its servers stand and the file it reads.
    says: Mutex<Says>,
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
    /// The button of each `host_press` request, in the order they arrived.
    host_presses: Mutex<Vec<String>>,
    /// What a `host_press` request is refused with, when a case sets it.
    host_press_refusal: Mutex<Option<String>>,
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
            Request::State { .. } if self.empty.load(std::sync::atomic::Ordering::SeqCst) => {
                Response::Error {
                    code: 2,
                    message: svolder_ipc::NO_DEVICES_CONFIGURED.into(),
                }
            }
            Request::State { .. } if self.unfound.load(std::sync::atomic::Ordering::SeqCst) => {
                Response::Error {
                    code: 3,
                    message: svolder_ipc::not_found_yet(CANONICAL),
                }
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
                    None => Response::Error {
                        code: 1,
                        message: "bad request: unknown variant `history`".into(),
                    },
                }
            }
            Request::Readings { .. } => match self.readings.lock().expect("the readings").clone() {
                Some(reply) => reply,
                None => Response::Error {
                    code: 1,
                    message: "bad request: unknown variant `readings`".into(),
                },
            },
            Request::Identify { device } => {
                self.asked_for.lock().expect("the record").push(device);
                match self.identify.lock().expect("the identify answer").clone() {
                    Some(reply) => reply,
                    None => Response::Error {
                        code: 1,
                        message: "bad request: unknown variant `identify`".into(),
                    },
                }
            }
            Request::Host => {
                self.host_snapshots
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match self.host.lock().expect("the host view").clone() {
                    Some(view) => Response::Host(view),
                    None => Response::Error {
                        code: 1,
                        message: "bad request: unknown variant `host`".into(),
                    },
                }
            }
            Request::HostPress { button } => {
                self.host_presses.lock().expect("the record").push(button);
                match self.host_press_refusal.lock().expect("the refusal").clone() {
                    Some(message) => Response::Error { code: 1, message },
                    None => Response::Ack,
                }
            }
            Request::Reload => {
                *self.reloads.lock().expect("reloads mutex") += 1;
                if let Some(message) = self.reload_error.lock().expect("the refusal").clone() {
                    return Response::Error { code: 1, message };
                }
                Response::Reloaded {
                    devices: vec![CANONICAL.into(), OTHER.into()],
                }
            }
            Request::Version => {
                self.version_asks
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Says {
                    mcp,
                    config,
                    token,
                    web,
                    web_token,
                } = self.says.lock().expect("the status").clone();
                Response::Version {
                    version: self.version.lock().expect("the version").clone(),
                    mqtt: None,
                    mcp,
                    config,
                    log: None,
                    mcp_token: token,
                    web,
                    web_token,
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
                    false => Response::Error {
                        code: 1,
                        message: svolder_ipc::WEB_OFF.to_string(),
                    },
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
                    (false, _) => Response::Error {
                        code: 1,
                        message: svolder_ipc::WEB_OFF.to_string(),
                    },
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
            other => Response::Error {
                code: 1,
                message: format!("{other:?} is not answered here"),
            },
        }
    }

    /// Every subscriber opens with a connection snapshot, as the daemon's own does: a
    /// reader that has had it knows the server is pumping this stream, so nothing
    /// published after that is missed.
    async fn subscribe(&self, device: Option<String>) -> BoxStream<'static, Event> {
        self.stream(device, false)
    }

    /// A host subscription also opens with the host view, when the daemon has one.
    async fn subscribe_with_host(&self, device: Option<String>) -> BoxStream<'static, Event> {
        self.stream(device, true)
    }

    /// The frames of the capture, once a case turned them on: the capture's state, then whatever
    /// the case sends, until the connection is dropped. Counted while it lives.
    async fn host_frames(&self) -> Result<BoxStream<'static, Event>, String> {
        self.frames_asked
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.frames_mute.load(std::sync::atomic::Ordering::SeqCst) {
            futures::future::pending::<()>().await;
        }
        let Some(sender) = self.frames.lock().expect("the frames").clone() else {
            return Err("bad request: unknown variant `host_frames`".to_string());
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
    fn stream(&self, device: Option<String>, host: bool) -> BoxStream<'static, Event> {
        self.subscribed
            .lock()
            .expect("subscribed mutex")
            .push(device);
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
    /// How many times the daemon has been told to read its configuration again.
    pub(crate) fn reloads(&self) -> usize {
        *self.handler.reloads.lock().expect("reloads mutex")
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

/// A socket path no other case has.
pub(crate) fn fresh_socket() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "svolder-remote-{}-{n}-{seq}.sock",
        std::process::id()
    ))
}

/// Serve a stub daemon on `socket` and wait for it to answer: a second daemon on the socket
/// of one that has stopped is a restart.
pub(crate) async fn stub_daemon_at(socket: PathBuf) -> StubServer {
    let handler = Arc::new(StubDaemon {
        events: broadcast::channel(16).0,
        subscribed: Mutex::default(),
        reloads: Mutex::default(),
        reload_error: Mutex::new(None),
        asked_for: Mutex::default(),
        other_retry: Arc::default(),
        empty: std::sync::atomic::AtomicBool::new(false),
        unfound: std::sync::atomic::AtomicBool::new(false),
        version: Mutex::new(STUB_VERSION.into()),
        says: Mutex::default(),
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
        host_presses: Mutex::default(),
        host_press_refusal: Mutex::new(None),
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
    tokio::spawn(async move {
        let _ = serve(&path, served, async {
            let _ = rx.await;
        })
        .await;
    });
    for _ in 0..500 {
        if Client::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    StubServer {
        socket,
        handler,
        shutdown: Some(tx),
    }
}
