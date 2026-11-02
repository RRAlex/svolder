// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Request routing: the `Handler` the IPC server calls, one device actor per request.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures::StreamExt;
use futures::stream::{self, BoxStream};
use svolder_ble::Transport;
use svolder_host::Button;
use svolder_ipc::config::{DeviceConfig, HostConfig, McpConfig, MqttConfig, WebConfig};
use svolder_ipc::host::FrameView;
use svolder_ipc::{
    Config, Event, Handler, MAX_SCAN_WINDOW, MqttStatus, NO_DEVICES_CONFIGURED, Request, Response,
    ScanEntry,
};
use svolder_profiles::Registry;
use svolder_profiles::tone::preset::Preset;
use tokio::sync::oneshot;
use tokio_stream::wrappers::BroadcastStream;

use crate::device::{self, Command, DeviceHandle, Refused};
use crate::fleet::DaemonFleet;
use crate::host::{FramesEvent, HostMode, HostService};
use crate::http_slot::Slot;
use crate::locks::{locked, read, written};
use crate::log;
use crate::mcp::McpServer;
use crate::mcp::tools::Floors;
use crate::mqtt_bridge::Bridge;
use crate::scanner::Scanner;
use crate::web::WebServer;
use crate::writer::{Mode, Origin, Outcome};

/// The longest a request waits for an actor, and for a write, the time it spends in the writer's
/// line behind other writes or a recall too. An actor answers a command it cannot carry out
/// rather than holding it, so this only ever fires for one that is genuinely stuck or a line
/// that is long; a write that times out may still reach the device afterwards.
pub(crate) const ACTOR_DEADLINE: Duration = Duration::from_secs(60);
/// What an actor is given beyond [`svolder_ble::FAREWELL_BOUND`] to finish closing: the
/// disconnect itself and the write spacing before it.
const SHUTDOWN_MARGIN: Duration = Duration::from_secs(6);
/// The longest the daemon waits for one actor to close its session as it exits. An actor asked
/// to stop mid-connect spends up to the farewell bound reclaiming the attempt and closing, so
/// the wait is that bound and a margin, which keeps the actor's answer ahead of the settle step
/// `main` runs after this, and stays well inside a service manager's own stop timeout.
const SHUTDOWN_DEADLINE: Duration = svolder_ble::FAREWELL_BOUND.saturating_add(SHUTDOWN_MARGIN);

/// Everything a request needs: the device actors, the default among them, the transport a
/// `scan` runs on, and the files a reload re-reads.
pub struct Daemon {
    /// The running actors and the default among them. A reload rebuilds the list and swaps it
    /// in whole, so a request in flight resolves against one complete set or the other.
    devices: RwLock<DeviceSet>,
    transport: Arc<dyn Transport>,
    /// Every scan on the adapter, shared by the actors and the `scan` verb.
    scanner: Arc<Scanner>,
    /// The registry an actor is started with, replaced by every reload.
    registry: RwLock<Arc<Registry>>,
    /// The configuration file a reload re-reads.
    config_path: PathBuf,
    /// The directory a reload re-reads user profiles from.
    profiles_dir: PathBuf,
    /// Held for the whole of a reload. Two of them — a hangup and a request, or two requests
    /// — would otherwise read the same device set and each write a different one over it:
    /// both would start an actor for a device the other started too, and the set written last
    /// could keep a handle whose actor the other one stopped.
    reloading: tokio::sync::Mutex<()>,
    /// Raised by a `stop` request; what the daemon waits on beside the termination signals.
    stop: tokio::sync::Notify,
    /// The socket the daemon serves on, which makes the MQTT bridge's default client id its
    /// own.
    socket: PathBuf,
    /// The MQTT bridge, while `[mqtt]` turns one on.
    bridge: std::sync::Mutex<BridgeSlot>,
    /// The MCP server, while `[mcp]` turns one on.
    mcp: Slot<McpServer>,
    /// The web UI, while `[web]` turns it on.
    web: Slot<WebServer>,
    /// The computer's player, cover and route, kept once for every front end.
    host: HostService,
    /// Where the host's sources come from; a daemon is made with [`HostMode::Off`] and `main`
    /// says what `SVOLDER_HOST` asks for.
    host_mode: HostMode,
    /// The web UI's one-use codes, which outlive a restart of its server.
    web_codes: crate::web::codes::SharedCodes,
    /// The levels the MCP tools measure a volume rise from, kept across the servers the daemon
    /// starts so a restart of the listener keeps the last minute.
    mcp_floors: Arc<Floors>,
    /// This daemon as a shared handle, which the MCP server's tools reach it through; set by
    /// [`Daemon::shared`], and weak so the server never keeps the daemon alive.
    me: std::sync::OnceLock<std::sync::Weak<Daemon>>,
}

/// The MQTT bridge the daemon runs, the section the next one is started from while the one
/// before it stops, and whether the daemon's own stop has begun — after which no bridge is
/// started at all.
#[derive(Default)]
struct BridgeSlot {
    running: Option<Bridge>,
    starting: Option<MqttConfig>,
    stopping: bool,
}

/// The devices a request resolves against: the running actors, and the name of the one a
/// request that selects none acts on.
#[derive(Default)]
struct DeviceSet {
    devices: Vec<Arc<DeviceHandle>>,
    default_device: Option<String>,
}

impl Daemon {
    /// A handler with no devices yet; [`Daemon::reconcile`] starts the ones the config names.
    pub fn new(
        transport: Arc<dyn Transport>,
        registry: Arc<Registry>,
        config_path: PathBuf,
        profiles_dir: PathBuf,
        socket: PathBuf,
    ) -> Self {
        let scanner = Arc::new(Scanner::new(Arc::clone(&transport)));
        Daemon {
            devices: RwLock::new(DeviceSet::default()),
            transport,
            scanner,
            registry: RwLock::new(registry),
            config_path,
            profiles_dir,
            reloading: tokio::sync::Mutex::new(()),
            stop: tokio::sync::Notify::new(),
            bridge: std::sync::Mutex::new(BridgeSlot::default()),
            mcp: Slot::default(),
            web: Slot::default(),
            host: HostService::idle(),
            host_mode: HostMode::Off,
            web_codes: Arc::default(),
            mcp_floors: Arc::default(),
            me: std::sync::OnceLock::new(),
            socket,
        }
    }

    /// The daemon with its host's sources taken from `mode` from the next
    /// [`Daemon::host_to`] on.
    pub fn with_host_mode(mut self, mode: HostMode) -> Self {
        self.host_mode = mode;
        self
    }

    /// Put the daemon behind the shared handle the MCP server's tools need; a daemon not made
    /// shared serves no MCP.
    pub fn shared(self) -> Arc<Daemon> {
        Arc::new_cyclic(|weak| {
            let _ = self.me.set(weak.clone());
            self
        })
    }

    /// Hand every actor a freshly loaded registry to re-resolve its profile against, and start
    /// the actors of later reloads with it.
    pub async fn reload(&self, registry: Arc<Registry>) {
        *written(&self.registry) = Arc::clone(&registry);
        let devices = self.devices();
        // Every actor is asked at once, so a reload takes as long as the slowest answer rather
        // than the sum of them. An actor that fails to answer is left as it was.
        futures::future::join_all(
            devices
                .iter()
                .map(|device| self.ask(device, Command::Reload(Arc::clone(&registry)))),
        )
        .await;
    }

    /// Re-read the configuration file and the user profiles, and bring the running devices in
    /// line with both. Answers with the name of every device now running, or with why the
    /// file could not be read — in which case the devices are left exactly as they were.
    ///
    /// Every reload comes through here, a hangup's as much as a request's, and they are taken
    /// one at a time: a reload reads the running device set and writes the next one, so two
    /// at once would each reconcile against a set the other was replacing.
    pub async fn reload_all(&self) -> Result<Vec<String>, String> {
        let _reloading = self.reloading.lock().await;
        // Both files are read before either is acted on, so a profile that will not parse
        // leaves the devices exactly as they were rather than reporting a failure over a
        // device set it has already changed.
        let config = Config::load_from(&self.config_path)
            .map_err(|e| format!("{}: {e}", self.config_path.display()))?;
        let registry = Registry::with_user_dir(&self.profiles_dir)
            .map_err(|e| format!("{}: {e}", self.profiles_dir.display()))?;
        for warning in config.warnings() {
            log::warn(&warning);
        }
        let devices = self.reconcile(&config).await;
        self.bridge_to(&config.mqtt, &config.presets).await;
        self.servers_to(&config.mcp, &config.web).await;
        self.host_to(&config.host);
        self.reload(Arc::new(registry)).await;
        Ok(devices)
    }

    /// Bring the MQTT bridge in line with `mqtt` and the devices as they now stand: a bridge
    /// already running that section keeps its connection and gets the current devices; one on
    /// a changed section is stopped (`offline` for every device and for itself) and the one
    /// `mqtt` describes, if any, started in its place.
    ///
    /// Stopping the old bridge first clears devices the file no longer names, so their
    /// entities leave Home Assistant at once; `version` reports connecting until the next
    /// bridge starts.
    ///
    /// A reload that meets the daemon's stop (a hangup, or a `reload` request racing SIGTERM)
    /// starts nothing: [`Daemon::stop_bridge`] marks the stop under the same lock
    /// [`Daemon::start_bridge`] checks, so a handover the stop overtook ends with the old
    /// bridge stopped and no new one.
    pub async fn bridge_to(&self, mqtt: &MqttConfig, presets: &[Preset]) {
        let devices = self.devices();
        let replaced = {
            let mut slot = locked(&self.bridge);
            if let Some(running) = slot.running.as_ref()
                && running.config() == mqtt
            {
                running.presets(presets.to_vec());
                running.devices(devices);
                return;
            }
            slot.starting = Some(mqtt.clone());
            slot.running.take()
        };
        if let Some(old) = replaced {
            log::info("mqtt: the [mqtt] section changed; stopping the bridge it described");
            old.stop(&devices).await;
        }
        self.start_bridge(mqtt, devices, presets);
    }

    /// Start the bridge `mqtt` describes in the slot, unless the daemon's stop has begun. The
    /// check and the start are one step under the lock, so a stop marked a moment earlier is
    /// always seen.
    fn start_bridge(&self, mqtt: &MqttConfig, devices: Vec<Arc<DeviceHandle>>, presets: &[Preset]) {
        let mut slot = locked(&self.bridge);
        slot.starting = None;
        if slot.stopping {
            return;
        }
        slot.running = Bridge::start(mqtt, devices, presets.to_vec(), &self.socket);
    }

    /// Stop the MQTT bridge, if one runs, with `offline` for every device and for itself.
    ///
    /// Marks the stop first, so a reload running at the same time starts no bridge after it.
    async fn stop_bridge(&self) {
        let running = {
            let mut slot = locked(&self.bridge);
            slot.stopping = true;
            slot.starting = None;
            slot.running.take()
        };
        if let Some(bridge) = running {
            bridge.stop(&self.devices()).await;
        }
    }

    /// Bring both HTTP servers in line with their sections in one go: each slot releases what
    /// it no longer serves before either starts a server, so servers that swapped ports find
    /// them free rather than one failing until the next reload.
    async fn servers_to(&self, mcp: &McpConfig, web: &WebConfig) {
        let start_mcp = self.mcp.release(mcp, mcp.enabled).await;
        let start_web = self.web.release(web, web.enabled).await;
        if start_mcp {
            self.start_mcp(mcp).await;
        }
        if start_web {
            self.start_web(web).await;
        }
    }

    /// Bring the MCP server in line with `mcp`: one already serving that section, whatever its
    /// auth, keeps its port, and one with a token re-reads it (a rotated one takes effect at
    /// once); a failed one is tried again; one on a changed section, or on a section that turns
    /// it off, is stopped — its port closed — and the one `mcp` describes, if any, started in its
    /// place.
    ///
    /// A server that cannot start is logged and reported by `version`, and never stops
    /// anything else. A reload that meets the daemon's stop starts nothing, the way
    /// [`Daemon::bridge_to`] does.
    pub async fn mcp_to(&self, mcp: &McpConfig) {
        if self.mcp.release(mcp, mcp.enabled).await {
            self.start_mcp(mcp).await;
        }
    }

    /// Bring the host service in line with `host`: switched off by `enabled = false` or by the
    /// mode, it runs nothing and reports itself unavailable; otherwise it follows the player
    /// and, while someone watches, reads the route. A host already running what the section
    /// says is left as it is, so a reload that changes nothing here is not seen by anyone.
    pub fn host_to(&self, host: &HostConfig) {
        self.host.reconfigure(host, self.host_mode);
    }

    /// The host service, which the web UI reads and holds a watcher on.
    pub(crate) fn host(&self) -> &HostService {
        &self.host
    }

    /// Start the MCP server `mcp` describes in its slot.
    async fn start_mcp(&self, mcp: &McpConfig) {
        self.mcp
            .start(|| async {
                let Some(me) = self.me.get().cloned() else {
                    log::error("mcp: the daemon was not made shared, so it serves no MCP");
                    return None;
                };
                Some(
                    McpServer::start(
                        mcp,
                        Arc::new(DaemonFleet::new(me, Origin::Mcp, Mode::Ordered)),
                        Arc::clone(&self.mcp_floors),
                    )
                    .await,
                )
            })
            .await;
    }

    /// Bring the web UI in line with `web`, as [`Daemon::mcp_to`] does the MCP server: one
    /// already serving that section keeps its port and re-reads its token; a failed one is tried
    /// again; one on a changed section, or on a section that turns it off, is stopped and the
    /// one `web` describes, if any, started in its place.
    ///
    /// A server that cannot start is logged and reported by `version`, and never stops
    /// anything else.
    pub async fn web_to(&self, web: &WebConfig) {
        if self.web.release(web, web.enabled).await {
            self.start_web(web).await;
        }
    }

    /// Start the web UI `web` describes in its slot.
    async fn start_web(&self, web: &WebConfig) {
        self.web
            .start(|| async {
                let Some(me) = self.me.get().cloned() else {
                    log::error("web: the daemon was not made shared, so it serves no web UI");
                    return None;
                };
                Some(
                    WebServer::start(
                        web,
                        Arc::new(DaemonFleet::new(me, Origin::Web, Mode::Latest)),
                        Arc::clone(&self.web_codes),
                    )
                    .await,
                )
            })
            .await;
    }

    /// A code from the web UI the daemon runs, answered as `answer` builds it; the words of
    /// [`svolder_ipc::WEB_OFF`] while it runs none.
    fn web_code(
        &self,
        make: impl Fn(&WebServer) -> Result<(String, u64), String>,
        answer: impl Fn(String, u64) -> Response,
    ) -> Response {
        match self.web.with(|server| server.map(make)) {
            Some(Ok((code, secs))) => answer(code, secs),
            Some(Err(message)) => Response::Error { code: 1, message },
            None => Response::Error {
                code: 1,
                message: svolder_ipc::WEB_OFF.to_string(),
            },
        }
    }

    /// Bring the running actors in line with `config`, and take the default device from it.
    ///
    /// Devices are matched by configured name, case-insensitively like every selector and
    /// writer: a name the file gained gets an actor, a name it lost has its actor stopped, and
    /// a name whose unit id now points elsewhere has its actor stopped and a new one started.
    /// Everything else in an entry — passphrase, profile, input labels — goes to the actor
    /// already running, so a device with only its settings changed keeps its link.
    ///
    /// Answers with the name of every device now running, in the order the file names them.
    pub async fn reconcile(&self, config: &Config) -> Vec<String> {
        let running = self.devices();
        let registry = Arc::clone(&read(&self.registry));
        let mut next: Vec<Arc<DeviceHandle>> = Vec::new();
        // The handles that were running and are running still, as the very `Arc`s the set
        // held: a device whose entry changed only in spelling goes into `next` under its own
        // handle, so it does not read as an actor nobody wants.
        let mut kept: Vec<Arc<DeviceHandle>> = Vec::new();
        let mut stopped: Vec<Arc<DeviceHandle>> = Vec::new();
        for wanted in &config.devices {
            match running
                .iter()
                .find(|handle| handle.name.eq_ignore_ascii_case(&wanted.name))
            {
                Some(handle) if handle.udid == wanted.udid => {
                    let _ = self.ask(handle, Command::Configure(wanted.clone())).await;
                    kept.push(Arc::clone(handle));
                    next.push(match handle.name == wanted.name {
                        true => Arc::clone(handle),
                        // The actor keeps its link; what the daemon calls it is what the file
                        // now spells, so `devices` answers with the name the file carries.
                        false => Arc::new(handle.renamed(&wanted.name)),
                    });
                }
                found => {
                    // The actor a changed unit id leaves behind is stopped before the new one
                    // starts, so the radio is free by the time the new one reaches for it.
                    if let Some(handle) = found {
                        self.stop(handle).await;
                        stopped.push(Arc::clone(handle));
                    }
                    if let Some(started) = self.start(wanted, &registry) {
                        next.push(started);
                    }
                }
            }
        }
        for handle in &running {
            let still_here = kept.iter().any(|k| Arc::ptr_eq(k, handle));
            let already = stopped.iter().any(|s| Arc::ptr_eq(s, handle));
            if !still_here && !already {
                self.stop(handle).await;
            }
        }
        if next.is_empty() {
            log::warn(&format!(
                "{NO_DEVICES_CONFIGURED} in {}; serving scan and devices only",
                self.config_path.display()
            ));
        }
        let names = next.iter().map(|handle| handle.name.clone()).collect();
        let mut set = written(&self.devices);
        set.devices = next;
        set.default_device = config.default_device.clone();
        names
    }

    /// Where the MQTT bridge stands: off while no section turns it on, and connecting while a
    /// reload hands over from one section's bridge to the next one's.
    fn mqtt_status(&self) -> MqttStatus {
        let slot = locked(&self.bridge);
        match (&slot.running, &slot.starting) {
            (Some(bridge), _) => bridge.status(),
            (None, Some(next)) if next.enabled => match &next.host {
                Some(host) => MqttStatus::Connecting {
                    host: host.clone(),
                    port: next.port,
                    retry_in_secs: None,
                },
                None => MqttStatus::Off,
            },
            (None, _) => MqttStatus::Off,
        }
    }

    /// Resolve once a `stop` request has been answered; this ends the accept loop.
    pub async fn stop_requested(&self) {
        self.stop.notified().await;
    }

    /// Stop the MQTT bridge, then ask every actor to close its session, so the radio is
    /// released before the daemon exits.
    ///
    /// The bridge goes first, while every device is still its own, and says `offline` for each
    /// of them and for itself before disconnecting. Every actor is asked at once, each under
    /// its own deadline — asking them one at a time could let an unreachable device delay the
    /// exit past a service manager's SIGKILL, leaving its session open.
    pub async fn shutdown(&self) {
        self.host.stop();
        self.mcp.stop().await;
        self.web.stop().await;
        self.stop_bridge().await;
        let devices = self.devices();
        let asks = devices
            .iter()
            .map(|device| self.ask_within(device, Command::Shutdown, SHUTDOWN_DEADLINE));
        futures::future::join_all(asks).await;
    }

    /// Start the actor for one configured device, or report why it could not start.
    fn start(&self, config: &DeviceConfig, registry: &Arc<Registry>) -> Option<Arc<DeviceHandle>> {
        match device::spawn(
            Arc::clone(&self.transport),
            Arc::clone(registry),
            config.clone(),
            Arc::clone(&self.scanner),
        ) {
            Ok(handle) => Some(Arc::new(handle)),
            Err(message) => {
                log::error(&message);
                None
            }
        }
    }

    /// Ask one actor to close its session and stop, so the radio is released before the last
    /// handle to it goes.
    async fn stop(&self, device: &DeviceHandle) {
        self.ask_within(device, Command::Shutdown, SHUTDOWN_DEADLINE)
            .await;
    }

    /// The registry an actor is started with now.
    pub(crate) fn registry(&self) -> Arc<Registry> {
        Arc::clone(&read(&self.registry))
    }

    /// The configuration file a reload re-reads.
    pub(crate) fn config_path(&self) -> &std::path::Path {
        &self.config_path
    }

    /// The actors as they stand, taken out of the lock so a caller may await on them.
    pub(crate) fn devices(&self) -> Vec<Arc<DeviceHandle>> {
        read(&self.devices).devices.clone()
    }

    /// The device a selector names.
    ///
    /// With no selector: the configured default, else the only configured device; a default
    /// naming no configured device is refused, even with one device. With one:
    /// a configured name, a product name, or a unit-id prefix, none case-sensitive; no match
    /// or more than one is an error rather than a guess.
    pub(crate) fn resolve(&self, selector: Option<&str>) -> Result<Arc<DeviceHandle>, Refused> {
        let unresolved = |message: String| Refused { code: 2, message };
        let set = read(&self.devices);
        let Some(selector) = selector else {
            if let Some(name) = set.default_device.as_deref()
                && !set
                    .devices
                    .iter()
                    .any(|d| d.name.eq_ignore_ascii_case(name))
            {
                return Err(Refused {
                    code: 1,
                    message: svolder_ipc::config::dangling_default_message(name),
                });
            }
            if let [only] = set.devices.as_slice() {
                return Ok(Arc::clone(only));
            }
            if let Some(name) = set.default_device.as_deref()
                && let Some(device) = set
                    .devices
                    .iter()
                    .find(|d| d.name.eq_ignore_ascii_case(name))
            {
                return Ok(Arc::clone(device));
            }
            return Err(unresolved(if set.devices.is_empty() {
                NO_DEVICES_CONFIGURED.to_string()
            } else {
                "several devices are configured; pick one with -d".to_string()
            }));
        };
        let matches: Vec<&Arc<DeviceHandle>> =
            set.devices.iter().filter(|d| d.matches(selector)).collect();
        match matches.as_slice() {
            [one] => Ok(Arc::clone(one)),
            [] => Err(unresolved(format!(
                "no configured device matches {selector:?}"
            ))),
            _ => Err(unresolved(format!(
                "{selector:?} matches several configured devices"
            ))),
        }
    }

    /// Forward one command to an actor and wait for its answer.
    ///
    /// An actor answers a command it cannot carry out rather than holding it, so the wait is
    /// short in practice: the bound below is the backstop that keeps a client from hanging
    /// on an actor that has somehow wedged.
    async fn ask(&self, device: &DeviceHandle, command: Command) -> Response {
        self.ask_within(device, command, ACTOR_DEADLINE).await
    }

    /// Forward one command and wait no longer than `deadline`, measured from this call, for
    /// the send and the answer together.
    async fn ask_within(
        &self,
        device: &DeviceHandle,
        command: Command,
        deadline: Duration,
    ) -> Response {
        let timeout_at = tokio::time::Instant::now() + deadline;
        let (tx, rx) = oneshot::channel();
        match tokio::time::timeout_at(timeout_at, device.commands.send((command, tx))).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return actor_gone(&device.name),
            Err(_) => return actor_silent(&device.name, deadline),
        }
        match tokio::time::timeout_at(timeout_at, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => actor_gone(&device.name),
            Err(_) => actor_silent(&device.name, deadline),
        }
    }

    /// Route one device command: resolve the device, then hand the command to its actor.
    pub(crate) async fn dispatch(&self, selector: Option<&str>, command: Command) -> Response {
        match self.resolve(selector) {
            Ok(device) => self.ask(&device, command).await,
            Err(refused) => refused.into(),
        }
    }

    /// Every device seen within `window`, one row each, first sighting per unit. A window
    /// longer than [`MAX_SCAN_WINDOW`] is listened for that long.
    async fn scan(&self, window: Duration) -> Response {
        let window = window.min(MAX_SCAN_WINDOW);
        let mut adverts = match self.scanner.scan(window).await {
            Ok(adverts) => adverts,
            Err(e) => {
                return Response::Error {
                    code: crate::device::code_for(&e),
                    message: e.to_string(),
                };
            }
        };
        let registry = Arc::clone(&read(&self.registry));
        let mut entries: Vec<ScanEntry> = Vec::new();
        while let Some(a) = adverts.next().await {
            let unit = a.unit_id().to_string();
            if entries.iter().any(|e| e.unit == unit) {
                continue;
            }
            entries.push(ScanEntry {
                unit,
                product_id: a.manufacturer.product_id,
                product: registry
                    .product_name(a.manufacturer.product_id)
                    .map(str::to_string),
                firmware: a.firmware_build(),
                passphrase: a.manufacturer.passphrase().is_well_formed(),
                rssi: a.rssi,
            });
        }
        Response::Scan { entries }
    }

    /// One device's events, with the lag notices a slow subscriber earns dropped.
    fn events_of(&self, device: &DeviceHandle) -> BoxStream<'static, Event> {
        BroadcastStream::new(device.events.subscribe())
            .filter_map(|event| async move { event.ok() })
            .boxed()
    }

    /// The connection snapshot a fresh subscriber is given, so it knows the current state
    /// without waiting for the next change (and knows its subscription is live).
    fn snapshot(&self, devices: &[Arc<DeviceHandle>]) -> Vec<Event> {
        devices
            .iter()
            .map(|d| Event::Connection {
                device: d.name.clone(),
                connected: d.describe().connected,
            })
            .collect()
    }
}

impl Daemon {
    /// Answer one request from `origin`, the way in for the socket's clients and the MCP server:
    /// every write goes through the device's writer
    /// ([`Writer::submit`](crate::writer::Writer::submit)), in order and never dropped; every
    /// other request goes to the actor or is answered here.
    pub async fn handle_from(&self, origin: Origin, request: Request) -> Response {
        match request {
            Request::Devices => Response::Devices {
                devices: self
                    .devices()
                    .iter()
                    .map(|device| device.describe())
                    .collect(),
            },
            Request::Scan { window_ms } => self.scan(Duration::from_millis(window_ms)).await,
            Request::State { device } => self.dispatch(device.as_deref(), Command::State).await,
            Request::History { device, span_secs } => match self.resolve(device.as_deref()) {
                Ok(device) => device.history(span_secs),
                Err(refused) => refused.into(),
            },
            Request::Readings { device, span_secs } => match self.resolve(device.as_deref()) {
                Ok(device) => device.readings(span_secs),
                Err(refused) => refused.into(),
            },
            Request::Recalled { name, .. }
                if name.chars().count() > svolder_profiles::tone::preset::NAME_MAX =>
            {
                Response::Error {
                    code: 1,
                    message: format!(
                        "a preset's name is at most {} characters",
                        svolder_profiles::tone::preset::NAME_MAX
                    ),
                }
            }
            Request::Recalled { device, name } => match self.resolve(device.as_deref()) {
                Ok(device) => {
                    device.writer.note_recalled(name);
                    Response::Ack
                }
                Err(refused) => refused.into(),
            },
            Request::WebPair => self.web_code(WebServer::pair_code, |code, expires_in_secs| {
                Response::WebPair {
                    code,
                    expires_in_secs,
                }
            }),
            Request::WebLink => self.web_code(WebServer::link_code, |code, expires_in_secs| {
                Response::WebLink {
                    code,
                    expires_in_secs,
                }
            }),
            Request::WebLinkOpen { code } => {
                match self.web.with(|server| server.map(|s| s.link_open(&code))) {
                    Some(Ok(open)) => Response::WebLinkOpen { open },
                    Some(Err(message)) => Response::Error { code: 1, message },
                    None => Response::Error {
                        code: 1,
                        message: svolder_ipc::WEB_OFF.to_string(),
                    },
                }
            }
            Request::EqShow { device } => self.dispatch(device.as_deref(), Command::EqShow).await,
            Request::Get { device, key } => {
                self.dispatch(device.as_deref(), Command::Get { key }).await
            }
            Request::Set {
                device,
                key,
                value,
                step,
                no_wait,
            } => {
                let command = Command::Set {
                    key,
                    value,
                    step,
                    no_wait,
                };
                self.write(origin, device.as_deref(), command).await
            }
            Request::Raw {
                device,
                id,
                value_type,
                value,
                no_wait,
            } => {
                let command = Command::Raw {
                    id,
                    value_type,
                    value,
                    no_wait,
                };
                self.write(origin, device.as_deref(), command).await
            }
            Request::EqSet {
                device,
                band,
                param,
                value,
                no_wait,
            } => {
                let command = Command::EqSet {
                    band,
                    param,
                    value,
                    no_wait,
                };
                self.write(origin, device.as_deref(), command).await
            }
            Request::EqMode {
                device,
                mode,
                no_wait,
            } => {
                self.write(origin, device.as_deref(), Command::EqMode { mode, no_wait })
                    .await
            }
            Request::Passphrase { device, digits } => {
                self.dispatch(device.as_deref(), Command::Passphrase { digits })
                    .await
            }
            Request::Identify { device } => {
                self.dispatch(device.as_deref(), Command::Identify).await
            }
            Request::Reload => match self.reload_all().await {
                Ok(devices) => Response::Reloaded { devices },
                Err(message) => Response::Error { code: 1, message },
            },
            Request::Version => Response::Version {
                version: env!("CARGO_PKG_VERSION").to_string(),
                mqtt: Some(self.mqtt_status()),
                mcp: Some(self.mcp.status()),
                config: Some(self.config_path.display().to_string()),
                log: crate::log::target(),
                mcp_token: self.mcp.token_info(),
                web: Some(self.web.status()),
                web_token: self.web.token_info(),
            },
            Request::Stop => {
                // The accept loop is woken here and the reply is written by the connection
                // task as this returns. What the wake starts is the ordinary exit — every
                // actor asked to close its session, then the transport's in-flight
                // connects and releases waited for — and those awaits are what leave the write
                // its chance to run; a line already in the socket is then readable whether or
                // not the daemon is still there.
                self.stop.notify_one();
                Response::Stopping
            }
            Request::Host => Response::Host(self.host.view()),
            Request::HostPress { button } => self.host_press(&button).await,
            // The server answers `Subscribe` through `subscribe`, never here.
            Request::Subscribe { .. } => Response::Error {
                code: 1,
                message: "subscribe is not a request/response verb".to_string(),
            },
            // And `HostFrames` through `host_frames`.
            Request::HostFrames => Response::Error {
                code: 1,
                message: "host_frames is not a request/response verb".to_string(),
            },
        }
    }

    /// Press `button`, one of `previous`, `play`, `pause` and `next`, on the followed player:
    /// an acknowledgement when the player took it, an error saying why not otherwise.
    async fn host_press(&self, button: &str) -> Response {
        let refuse = |message: &str| Response::Error {
            code: 1,
            message: message.to_string(),
        };
        let Some(button) = Button::ALL
            .into_iter()
            .find(|candidate| candidate.method().eq_ignore_ascii_case(button))
        else {
            return refuse(&format!(
                "no button {button:?}; use previous, play, pause or next"
            ));
        };
        let view = self.host.view();
        if !view.available {
            return refuse("host audio is not available");
        }
        if view.playing.is_none() {
            return refuse("no player to press it on");
        }
        match self.host.press(button).await {
            true => Response::Ack,
            false => refuse("the player did not take the press"),
        }
    }

    /// Hand one write to the writer of the device `selector` names, as `origin`'s, and wait for
    /// the actor's answer no longer than [`ACTOR_DEADLINE`].
    async fn write(&self, origin: Origin, selector: Option<&str>, command: Command) -> Response {
        let device = match self.resolve(selector) {
            Ok(device) => device,
            Err(refused) => return refused.into(),
        };
        let answer = device.writer.submit(origin, command, Mode::Ordered);
        match tokio::time::timeout(ACTOR_DEADLINE, answer).await {
            Ok(Ok(Outcome::Done(response))) => response,
            Ok(Ok(Outcome::Superseded)) => Response::Error {
                code: 3,
                message: format!("a newer write to {} took this one's place", device.name),
            },
            Ok(Ok(Outcome::Cancelled(why))) => Response::Error {
                code: 3,
                message: why.to_string(),
            },
            Ok(Err(_)) => actor_gone(&device.name),
            Err(_) => actor_silent(&device.name, ACTOR_DEADLINE),
        }
    }
}

#[async_trait::async_trait]
impl Handler for Daemon {
    async fn handle(&self, request: Request) -> Response {
        self.handle_from(Origin::Socket, request).await
    }

    async fn subscribe(&self, device: Option<String>) -> BoxStream<'static, Event> {
        self.events_of_devices(device, false)
    }

    async fn subscribe_with_host(&self, device: Option<String>) -> BoxStream<'static, Event> {
        self.events_of_devices(device, true)
    }

    async fn host_frames(&self) -> Result<BoxStream<'static, Event>, String> {
        let hold = self
            .host
            .frames()
            .ok_or_else(|| "host audio is not available".to_string())?;
        Ok(hold
            .events()
            .map(|event| match event {
                FramesEvent::Capture(state) => Event::HostCapture(state.view()),
                FramesEvent::Frame(frame) => Event::HostFrame(FrameView::from(&*frame)),
            })
            .boxed())
    }
}

impl Daemon {
    /// The events of the devices `device` selects (all of them for `None`), after the
    /// connection snapshot, without the host's: what the in-process consumers (the MCP server,
    /// the web UI's device streams) and a plain `Subscribe` read, none of which hold the route
    /// reads open.
    pub(crate) fn device_events(&self, device: Option<String>) -> BoxStream<'static, Event> {
        self.events_of_devices(device, false)
    }

    /// The snapshot and the live events of the devices `device` selects. With `with_host` (a
    /// `Subscribe` that asked for it) the host's view follows the snapshot, once and then on
    /// each change, whatever the device filter: the view belongs to the daemon and not to a
    /// device. The stream then holds a watcher, so the route is read for as long as the
    /// connection is open. A selector that
    /// names no device is the one error and the end of the stream, with no host.
    fn events_of_devices(
        &self,
        device: Option<String>,
        with_host: bool,
    ) -> BoxStream<'static, Event> {
        let chosen: Vec<Arc<DeviceHandle>> = match device.as_deref() {
            None => self.devices(),
            Some(selector) => match self.resolve(Some(selector)) {
                Ok(device) => vec![device],
                Err(refused) => {
                    log::debug(&format!("subscribe: {}", refused.message));
                    return stream::iter(vec![Event::Error {
                        message: refused.message,
                    }])
                    .boxed();
                }
            },
        };
        let snapshot = stream::iter(self.snapshot(&chosen));
        let live = stream::select_all(chosen.iter().map(|d| self.events_of(d)));
        // A merge of no device streams ends as soon as it is polled, and an empty subscription
        // is not a failed one: chaining `pending` keeps it open and silent, so a front end
        // shows an empty device list rather than a report that the daemon stopped sending
        // events. This relies on `reload` never replacing one of `chosen`'s actors while a
        // subscriber holds it: `reload` sends a surviving actor `Command::Reload` in place
        // rather than respawning it, so `events_of`'s broadcast channel stays the one this
        // subscription already reads from.
        let live = live.chain(stream::pending());
        match with_host {
            true => snapshot
                .chain(stream::select(live, self.host.events()))
                .boxed(),
            false => snapshot.chain(live).boxed(),
        }
    }
}

/// The answer for a device whose actor has stopped, which only happens as the daemon exits.
pub(crate) fn actor_gone(name: &str) -> Response {
    Response::Error {
        code: 3,
        message: format!("device {name} is not running"),
    }
}

/// The answer for an actor that did not respond within the deadline it was given.
pub(crate) fn actor_silent(name: &str, deadline: Duration) -> Response {
    Response::Error {
        code: 4,
        message: format!("device {name} did not answer within {deadline:?}"),
    }
}

/// The words of [`actor_silent`], for a caller that wants them as text.
pub(crate) fn silence(name: &str) -> String {
    match actor_silent(name, ACTOR_DEADLINE) {
        Response::Error { message, .. } => message,
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod tests;
