// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The half of a backend that talks to a running daemon: the daemon holds the device, and
//! the CLI asks it for readings and hands it commands.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_ble::BleError;
use svolder_ipc::socket::daemon_version::{self, DaemonInfo};
use svolder_ipc::{
    Client, DeviceInfo, Event, IpcError, LinkChange, NOT_CONNECTED, PassphraseNote, Request,
    Response, ScanEntry, SeriesOut, ValueObject,
};
use svolder_profiles::{Profile, Registry};

use super::{DeviceRow, Failure, Identified, Update, WriteError, refused};
use crate::recovery::{Kind, next_step};

/// How long the daemon is given to answer the question about a device's passphrase note, which a
/// verb asks before every output: a best-effort nicety that never holds a verb up for long.
const NOTE_WAIT: Duration = Duration::from_secs(1);

/// What a front end says of a daemon that does not know the identify request.
const TOO_OLD_TO_IDENTIFY: &str = "this daemon is too old to identify; restart it";

/// The step that follows [`TOO_OLD_TO_IDENTIFY`].
const TOO_OLD_STEP: &str = "Run `svolder daemon restart`";

/// Whether `message` is a daemon's refusal of the `identify` request as one it does not know.
fn is_unknown_request(message: &str) -> bool {
    message.starts_with("bad request: ") && message.contains("`identify`")
}

/// The failure a daemon's reply earns when it is not the answer a write asked for: the code and
/// words it refused with, which become this process's exit code unchanged, a reply of another
/// kind, or a connection that failed.
fn write_failure<E: std::fmt::Display>(reply: Result<Response, E>) -> WriteError {
    match reply {
        Ok(Response::Error { code, message }) => {
            let next = crate::recovery::daemon_step(code, &message);
            refused(code, next, message)
        }
        Ok(other) => refused(
            3,
            next_step(Kind::DaemonError),
            format!("the daemon sent an unexpected reply: {other:?}"),
        ),
        Err(e) => refused(3, next_step(Kind::DaemonError), e.to_string()),
    }
}

/// A conversation with the daemon, and the profile the CLI reads its readings through.
pub struct RemoteBackend {
    /// The request/response connection, which every read and write goes over.
    client: Client,
    /// The event stream a subscription opened, on a second connection of its own.
    events: Option<BoxStream<'static, Event>>,
    /// Whether that subscription asked for the daemon's host view.
    host_feed: bool,
    /// The socket the connection was made on, so the subscription and a fresh snapshot each
    /// have a connection of their own to make.
    socket: PathBuf,
    /// The device every request names and every event is matched against: the name the daemon
    /// answered under, or the selector as it was typed by a backend that has asked it nothing.
    pub(super) device: Option<String>,
    /// The profile the daemon named for the device it answered for, read from the CLI's own
    /// registry. A backend opened without connecting holds a placeholder it never reads.
    pub(super) profile: Profile,
    /// The device the daemon answered for but did not list in `devices`, which is what stops
    /// a fault check from reading "no profile" as "no fault".
    pub(super) unknown_device: Option<String>,
    /// The readings `open` fetched; empty for a backend opened without connecting.
    values: Vec<ValueObject>,
    /// The daemon's device list as `open` read it; `None` for a backend opened without
    /// connecting.
    listed: Option<Vec<DeviceInfo>>,
    /// What opening had to say that was not worth refusing to open over, until something
    /// takes it to report in its own way.
    pub(super) warnings: Vec<String>,
    /// Whether the daemon said, when this backend was opened, that the link it holds to the
    /// device has been refused the passphrase.
    refused: bool,
}

/// What the open says of a device whose link is down, after its name.
pub(super) const LAST_KNOWN: &str = "readings are the last known";

impl RemoteBackend {
    /// Connect to the daemon and, when `connect`, read the device's state through it.
    ///
    /// The daemon resolves the selector by its own rules and names the device it answered for,
    /// so the CLI never re-implements that ladder: it keeps the readings, looks that name up in
    /// `devices`, and takes the profile the daemon resolved from its own registry. That profile
    /// only turns a key into a function id and recognises a fault; the daemon stays authoritative
    /// for the readings themselves. A device the daemon has lost the link to is reported as a
    /// warning, since its readings are the last known ones.
    ///
    /// The resolved name replaces whatever the selector said, for every use from here on: a
    /// subscription asks for the one device these readings came from, not every device the
    /// daemon holds, and its events are named with the daemon's own name for that device (a
    /// selector may be a configured name, a product name or a unit-id prefix, in any case, so
    /// only the resolved name can be compared against what the daemon publishes).
    pub(super) async fn open(
        path: &Path,
        selector: Option<&str>,
        connect: bool,
    ) -> Result<RemoteBackend, Failure> {
        let mut client = connect_to(path).await?;
        let mut device = selector.map(str::to_string);
        let mut profile = Registry::generic(0);
        let mut unknown_device = None;
        let mut values = Vec::new();
        let mut listed = None;
        let mut warnings = Vec::new();
        let mut refused = false;
        if connect {
            let ask = Request::State {
                device: device.clone(),
            };
            let (name, read) = match request(&mut client, path, ask).await? {
                Response::Values { device, values } => (device, values),
                other => return Err(unexpected(&other)),
            };
            values = read;
            device = Some(name.clone());
            let devices = match request(&mut client, path, Request::Devices).await? {
                Response::Devices { devices } => devices,
                other => return Err(unexpected(&other)),
            };
            let found = devices.iter().find(|d| d.name == name).cloned();
            listed = Some(devices);
            match found {
                Some(info) => {
                    // The daemon answers a disconnected device from the readings it held
                    // when the link went down, which is worth having and worth saying: the
                    // verb prints them and leaves with the code it would have anyway.
                    if !info.connected {
                        warnings.push(format!("{name} is disconnected; {LAST_KNOWN}"));
                    }
                    refused = info.refused;
                    let registry = super::registry()?;
                    profile = remote_profile(&registry, &info);
                }
                None => unknown_device = Some(name),
            }
        }
        Ok(RemoteBackend {
            client,
            events: None,
            host_feed: false,
            socket: path.to_path_buf(),
            device,
            profile,
            unknown_device,
            values,
            listed,
            warnings,
            refused,
        })
    }

    /// Whether the device refused the passphrase on the link the daemon holds, as it stood
    /// when this backend was opened.
    pub(super) fn refused(&self) -> bool {
        self.refused
    }

    /// The socket this backend's connection was made on.
    pub(super) fn socket(&self) -> &Path {
        &self.socket
    }

    /// The daemon's device list as `open` read it, as rows; `None` for a backend opened without
    /// connecting.
    pub(super) fn rows(&self) -> Option<Vec<DeviceRow>> {
        self.listed
            .as_ref()
            .map(|l| l.iter().map(device_row).collect())
    }

    /// The entry of the device list `open` read that this backend's device is listed under, or
    /// the only entry for a backend that named no device at all.
    fn listed_device(&self) -> Option<&DeviceInfo> {
        let listed = self.listed.as_deref()?;
        match self.device.as_deref() {
            Some(name) => listed.iter().find(|d| d.name == name),
            None => match listed {
                [only] => Some(only),
                _ => None,
            },
        }
    }

    /// The unit id of the device this backend selected, from the device list `open` read.
    ///
    /// The entry named by the device this backend holds (the name the daemon resolved, which is
    /// the name it lists that device under), or the only entry listed for a backend that named
    /// no device at all. A backend opened without connecting, or one whose daemon listed no
    /// matching device, leaves the front end with no unit id to show rather than a failure to
    /// report. Nothing is sent.
    pub(super) async fn unit(&mut self) -> Option<String> {
        self.listed_device().map(|d| d.unit.clone())
    }

    /// Send `recalled` for this device; the answer is not read for anything.
    pub(super) async fn note_recalled(&mut self, name: &str) {
        let ask = Request::Recalled {
            device: self.device.clone(),
            name: name.to_string(),
        };
        let _ = self.client.request(ask).await;
    }

    /// The preset the daemon's memory named for this device when `open` read the device list;
    /// `None` for none, and for a backend opened without connecting.
    pub(super) fn recalled_as_opened(&self) -> Option<String> {
        self.listed_device()?.recalled.clone()
    }

    /// How the daemon judged this device's passphrase when `open` read the device list; `None`
    /// for no note and for a backend opened without connecting.
    pub(super) fn passphrase_note_as_opened(&self) -> Option<PassphraseNote> {
        self.listed_device()?.passphrase_note
    }

    /// The device as the daemon's device list `open` read has it, as a row.
    pub(super) fn own_row_as_opened(&self) -> Option<DeviceRow> {
        self.listed_device().map(device_row)
    }

    /// The device as the daemon lists it now, over a connection of this call's own, as a future
    /// that owns what it needs: `None` for a daemon that cannot be asked in time or does not list
    /// the device.
    pub(super) fn own_info_request(
        &self,
    ) -> impl Future<Output = Option<DeviceInfo>> + Send + 'static {
        let socket = self.socket.clone();
        let device = self.device.clone();
        async move {
            own_of(
                tokio::time::timeout(NOTE_WAIT, device_infos(&socket)).await,
                device,
            )
        }
    }

    /// [`RemoteBackend::own_info_request`], borrowing this backend while it runs.
    pub(super) async fn own_info(&self) -> Option<DeviceInfo> {
        own_of(
            tokio::time::timeout(NOTE_WAIT, device_infos(&self.socket)).await,
            self.device.clone(),
        )
    }

    /// How the daemon judges this device's configured passphrase against its advertisement, from
    /// the daemon's device list over a connection of this call's own. `None` for no note, and
    /// for a daemon that cannot be asked in time or does not judge: the note is a nicety a front
    /// end shows when it can.
    pub(super) async fn passphrase_note(&self) -> Option<PassphraseNote> {
        self.note_of(self.device.as_deref()).await
    }

    /// The same for the device the daemon lists as `name`, which need not be the one this
    /// backend watches; `None` names the only device listed.
    pub(super) async fn note_of(&self, name: Option<&str>) -> Option<PassphraseNote> {
        let devices = tokio::time::timeout(NOTE_WAIT, device_infos(&self.socket))
            .await
            .ok()?
            .ok()?;
        match name {
            Some(name) => resolve(&devices, name),
            None => match devices.as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            },
        }?
        .passphrase_note
    }

    /// The device's link changes over the last `span_secs` seconds, plus the one before them,
    /// from the daemon. `None` for anything but a history answer: a daemon too old to know the
    /// request answers it with its unknown-request error, and a front end then has only its own
    /// observations to go on.
    pub(super) async fn link_history(&mut self, span_secs: u64) -> Option<Vec<LinkChange>> {
        let ask = Request::History {
            device: self.device.clone(),
            span_secs,
        };
        match self.client.request(ask).await.ok()? {
            Response::History { transitions, .. } => Some(transitions),
            _ => None,
        }
    }

    /// The device's readings over the last `span_secs` seconds from the daemon, with the
    /// daemon's wall clock when it answered: `None` for anything but a readings answer, as a
    /// daemon too old to know the request answers it.
    pub(super) async fn readings(&mut self, span_secs: u64) -> Option<(u64, Vec<SeriesOut>)> {
        let ask = Request::Readings {
            device: self.device.clone(),
            span_secs,
        };
        match self.client.request(ask).await.ok()? {
            Response::Readings { now_ms, series, .. } => Some((now_ms, series)),
            _ => None,
        }
    }

    /// Every device the daemon sees within `window`; the daemon holds a longer one to its own
    /// maximum.
    pub(super) async fn scan(&mut self, window: Duration) -> Result<Vec<ScanEntry>, Failure> {
        let window_ms = u64::try_from(window.as_millis()).unwrap_or(u64::MAX);
        match request(&mut self.client, &self.socket, Request::Scan { window_ms }).await? {
            Response::Scan { entries } => Ok(entries),
            other => Err(unexpected(&other)),
        }
    }

    /// Every reading the daemon's device had confirmed when `open` read them; none for a
    /// backend opened without connecting.
    pub(super) fn state(&self) -> Vec<ValueObject> {
        self.values.clone()
    }

    /// Ask the daemon for the readings again, over a connection of its own.
    ///
    /// A second connection rather than this backend's: asking over a connection of its own
    /// costs one socket and leaves the request client free for whatever the caller sends next.
    pub(super) async fn snapshot(&mut self) -> Result<Vec<ValueObject>, Failure> {
        self.snapshot_request().await
    }

    /// [`RemoteBackend::snapshot`] as a future that owns what it needs, so it does not borrow
    /// this backend while it runs.
    pub(super) fn snapshot_request(
        &self,
    ) -> impl Future<Output = Result<Vec<ValueObject>, Failure>> + Send + 'static {
        let socket = self.socket.clone();
        let device = self.device.clone();
        async move {
            let mut client = connect_to(&socket).await?;
            let ask = Request::State { device };
            match request(&mut client, &socket, ask).await? {
                Response::Values { values, .. } => Ok(values),
                other => Err(unexpected(&other)),
            }
        }
    }

    /// One write over the daemon: the reading it confirmed, nothing for a no-wait, or the
    /// code and words it refused with, which become this process's exit code unchanged.
    pub(super) async fn write(
        &mut self,
        request: Request,
    ) -> Result<Option<ValueObject>, WriteError> {
        match self.client.request(request).await {
            Ok(Response::Value(vo)) => Ok(Some(vo)),
            Ok(Response::Ack) => Ok(None),
            failed => Err(write_failure(failed)),
        }
    }

    /// Ask the daemon to have the device show itself.
    ///
    /// A device the daemon has no link to is refused as any write is, with the wait until its
    /// next attempt added in the words every listing uses. A daemon too old to know the
    /// request answers it with its unknown-request error, which is turned into the advice to
    /// restart it.
    pub(super) async fn identify(
        &mut self,
        device: Option<&str>,
    ) -> Result<Identified, WriteError> {
        let device = device.map(str::to_string).or_else(|| self.device.clone());
        let ask = Request::Identify {
            device: device.clone(),
        };
        match self.client.request(ask).await {
            Ok(Response::Identify { device, sent }) => Ok(Identified { device, sent }),
            Ok(Response::Error { code: 1, message }) if is_unknown_request(&message) => {
                Err(refused(1, TOO_OLD_STEP, TOO_OLD_TO_IDENTIFY))
            }
            Ok(Response::Error { code: 3, message }) if message == NOT_CONNECTED => {
                let wait = self.retry_in_secs(device.as_deref()).await;
                Err(write_failure(Ok::<_, IpcError>(Response::Error {
                    code: 3,
                    message: crate::format::link_down(NOT_CONNECTED, wait),
                })))
            }
            failed => Err(write_failure(failed)),
        }
    }

    /// Whole seconds until the daemon's next attempt at `device` (the one it lists alone, for
    /// none), from its device list over a connection of this call's own; `None` when it cannot
    /// be asked in time or is not waiting.
    async fn retry_in_secs(&self, device: Option<&str>) -> Option<u64> {
        let devices = tokio::time::timeout(NOTE_WAIT, device_infos(&self.socket))
            .await
            .ok()?
            .ok()?;
        match device {
            Some(name) => resolve(&devices, name),
            // The daemon's not-connected answer to an identify names no device, so with several
            // listed and no `-d` there is no row to take the wait from.
            None => match devices.as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            },
        }?
        .retry_in_secs
    }

    /// Tell the daemon to read the configuration file again, and answer with the devices it
    /// holds after that.
    ///
    /// This is what a front end that has just written the file sends, so a device it added is
    /// held from now on and one it took away is let go. It goes over a connection of its own,
    /// leaving the request client to whatever the caller sends next — a reload is asked for in
    /// the middle of a session, not while opening one.
    pub(super) async fn reload(&self) -> Result<Vec<String>, Failure> {
        let mut client = connect_to(&self.socket).await?;
        match request(&mut client, &self.socket, Request::Reload).await? {
            Response::Reloaded { devices } => Ok(devices),
            other => Err(unexpected(&other)),
        }
    }

    /// The daemon's device list as it stands now, over a connection of this call's own, so a
    /// listing never waits behind a read or a write on the request connection.
    pub(super) async fn device_rows(&self) -> Result<Vec<DeviceRow>, Failure> {
        device_rows(&self.socket).await
    }

    /// A future asking the daemon for its own version and what it says of itself, over a
    /// connection of its own — owned, so it can be spawned rather than borrow this backend for
    /// as long as it runs. `None` for anything but a plain version answer within
    /// [`daemon_version::VERSION_WAIT`]: what the daemon says of itself is a nicety a front end
    /// shows when it can, never a reason to fail the session over.
    pub(super) fn version_request(
        &self,
    ) -> impl Future<Output = Option<DaemonInfo>> + Send + 'static {
        let socket = self.socket.clone();
        async move { daemon_version::version_within(&socket, daemon_version::VERSION_WAIT).await }
    }

    /// A future that sends `request` to the daemon over a connection of its own and answers its
    /// reply, or `None` when none comes within [`daemon_version::VERSION_WAIT`].
    pub(super) fn request_once(
        &self,
        request: Request,
    ) -> impl Future<Output = Option<Response>> + Send + 'static {
        let socket = self.socket.clone();
        async move {
            let ask = async {
                let mut client = Client::connect(&socket).await.ok()?;
                client.request(request).await.ok()
            };
            tokio::time::timeout(daemon_version::VERSION_WAIT, ask)
                .await
                .ok()
                .flatten()
        }
    }

    /// A future that opens the daemon's frames of the host's capture on a connection of their
    /// own (owned, so it can be spawned): a stream of `HostCapture` and `HostFrame` events that
    /// holds the daemon's capture for as long as it lives, or why it could not be had.
    pub(super) fn host_frames(
        &self,
    ) -> impl Future<Output = Result<BoxStream<'static, Event>, IpcError>> + Send + 'static {
        let socket = self.socket.clone();
        async move { Client::connect(&socket).await?.host_frames().await }
    }

    /// Subscribe to this device's events over a connection of the feed's own.
    ///
    /// [`Client::subscribe`] consumes the client it is called on, so the subscription gets a
    /// second connection and this backend keeps the one every read and write goes over. A
    /// caller makes this call once, before it starts reading, and never from inside a
    /// `select!` where a dropped future would leave the subscription half-made.
    pub(super) async fn open_feed(&mut self) -> Result<(), BleError> {
        self.subscribe(false).await
    }

    /// [`RemoteBackend::open_feed`] with the subscription also asking for the daemon's host
    /// view, which the daemon then keeps reading the computer's audio graph for.
    pub(super) async fn open_host_feed(&mut self) -> Result<(), BleError> {
        self.subscribe(true).await
    }

    /// Whether the open subscription asks for the daemon's host view.
    pub(super) fn host_feed(&self) -> bool {
        self.events.is_some() && self.host_feed
    }

    /// Close the subscription and open another that asks for the host's view when `host`, which
    /// is how a front end starts and stops being a reason for the daemon to read the computer's
    /// audio graph. What the old one had not delivered is lost, so the caller reads the device
    /// again.
    pub(super) async fn resubscribe(&mut self, host: bool) -> Result<(), BleError> {
        self.events = None;
        self.subscribe(host).await
    }

    /// Subscribe unless a subscription is open already, asking for the host's view when `host`.
    async fn subscribe(&mut self, host: bool) -> Result<(), BleError> {
        if self.events.is_some() {
            return Ok(());
        }
        let client = Client::connect(&self.socket)
            .await
            .map_err(|e| BleError::Connection(e.to_string()))?;
        let device = self.device.clone();
        let events = match host {
            true => client.subscribe_with_host(device).await,
            false => client.subscribe(device).await,
        }
        .map_err(|e| BleError::Connection(e.to_string()))?;
        self.events = Some(events);
        self.host_feed = host;
        Ok(())
    }

    /// The next event the daemon has for this device, or `Ok(None)` once the stream has ended
    /// or with no subscription to read.
    ///
    /// This only reads: [`RemoteBackend::open_feed`] is what opens the subscription, so a
    /// dropped future here consumes nothing. An event for another device is skipped, so a
    /// daemon that fans one out cannot put a second device's readings on these rows. A fault
    /// is skipped too: it has already arrived as the status that set it, and an error on the
    /// stream is the daemon's own note rather than a reading to print.
    pub(super) async fn next_status(&mut self) -> Result<Option<Update>, BleError> {
        let Some(events) = self.events.as_mut() else {
            return Ok(None);
        };
        while let Some(event) = events.next().await {
            match event {
                Event::Status { device, reading } if mine(self.device.as_deref(), &device) => {
                    return Ok(Some(Update::Status(reading)));
                }
                Event::Connection { device, connected }
                    if mine(self.device.as_deref(), &device) =>
                {
                    return Ok(Some(Update::Connection { connected }));
                }
                Event::Recalled { device, name } if mine(self.device.as_deref(), &device) => {
                    return Ok(Some(Update::Recalled(name)));
                }
                // The host's view is the daemon's, whatever device this feed is on.
                Event::Host(view) => return Ok(Some(Update::Host(Box::new(view)))),
                // What is not this device's recall, status or connection: another device's
                // event, this device's climb, or a note about the daemon.
                Event::Status { .. }
                | Event::Connection { .. }
                | Event::Fault { .. }
                | Event::Error { .. }
                | Event::Recalled { .. }
                | Event::Climb { .. }
                // The frames of the capture come only on a frames connection.
                | Event::HostCapture(_)
                | Event::HostFrame(_)
                | Event::Unknown => continue,
            }
        }
        // An ended stream is dropped: polling it again is a panic, and a later `open_feed`
        // subscribes afresh instead of taking the dead stream for a live one.
        self.events = None;
        Ok(None)
    }
}

/// The daemon's device list, over a connection of this call's own.
///
/// A listing needs no session, so it does not open one: the daemon already holds the links
/// and knows which of them are up.
pub(super) async fn device_rows(path: &Path) -> Result<Vec<DeviceRow>, Failure> {
    let mut client = connect_to(path).await?;
    match request(&mut client, path, Request::Devices).await? {
        Response::Devices { devices } => Ok(devices.iter().map(device_row).collect()),
        other => Err(unexpected(&other)),
    }
}

/// The daemon's devices as it describes them, over a connection of this call's own.
pub(super) async fn device_infos(path: &Path) -> Result<Vec<DeviceInfo>, Failure> {
    let mut client = connect_to(path).await?;
    match request(&mut client, path, Request::Devices).await? {
        Response::Devices { devices } => Ok(devices),
        other => Err(unexpected(&other)),
    }
}

/// The device the daemon's list `listed` has for `device`, the one device it names or, with no
/// name, the only one it lists.
fn own_of(
    listed: Result<Result<Vec<DeviceInfo>, Failure>, tokio::time::error::Elapsed>,
    device: Option<String>,
) -> Option<DeviceInfo> {
    let devices = listed.ok()?.ok()?;
    match device.as_deref() {
        Some(name) => resolve(&devices, name),
        None => match devices.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        },
    }
}

/// The device `selector` names, by the daemon's own order of resolving one: the configured name,
/// then the one device whose product name it is, then the one whose unit id it starts, in any
/// case.
fn resolve(infos: &[DeviceInfo], selector: &str) -> Option<DeviceInfo> {
    if selector.is_empty() {
        return None;
    }
    let only = |matches: Vec<&DeviceInfo>| match matches.as_slice() {
        [one] => Some((*one).clone()),
        _ => None,
    };
    let lower = selector.to_ascii_lowercase();
    infos
        .iter()
        .find(|info| info.name.eq_ignore_ascii_case(selector))
        .cloned()
        .or_else(|| {
            only(
                infos
                    .iter()
                    .filter(|info| {
                        info.product
                            .as_deref()
                            .is_some_and(|p| p.eq_ignore_ascii_case(selector))
                    })
                    .collect(),
            )
        })
        .or_else(|| {
            only(
                infos
                    .iter()
                    .filter(|info| info.unit.to_ascii_lowercase().starts_with(&lower))
                    .collect(),
            )
        })
}

/// The name the daemon lists the device `selector` names under, when it lists that device as
/// not connected.
pub(super) fn waited_name(infos: &[DeviceInfo], selector: &str) -> Option<String> {
    let named = resolve(infos, selector)?;
    (!named.connected).then_some(named.name)
}

/// One device the daemon reported, as a row.
pub(super) fn device_row(info: &DeviceInfo) -> DeviceRow {
    DeviceRow {
        name: info.name.clone(),
        product: info.product.clone(),
        connected: Some(info.connected),
        retry_in_secs: info.retry_in_secs,
        signal: info.signal,
        profile: Some(info.profile.clone()),
        product_id: Some(info.product_id),
        stale: info.stale,
        refused: info.refused,
    }
}

async fn connect_to(path: &Path) -> Result<Client, Failure> {
    Client::connect(path)
        .await
        .map_err(|e| Failure::of(Kind::NoDaemon, e.to_string()))
}

/// One request/response round trip with the daemon on `socket`: an error the daemon reports
/// travels back with the exit code it named, in its own words or, for one the front end has
/// words of its own for, in those (see [`Failure::from_daemon_on`]), and a lost connection is
/// a 3.
async fn request(
    client: &mut Client,
    socket: &Path,
    request: Request,
) -> Result<Response, Failure> {
    match client.request(request).await {
        Ok(Response::Error { code, message }) => {
            Err(Failure::from_daemon_on(socket, code, message))
        }
        Ok(response) => Ok(response),
        Err(e) => Err(Failure::of(Kind::DaemonError, e.to_string())),
    }
}

/// Whether an event about `device` is this backend's to report: every event for a backend
/// that names no device of its own, and only that device's for one that does.
fn mine(selected: Option<&str>, device: &str) -> bool {
    selected.is_none_or(|name| name == device)
}

/// A reply that does not answer the request that was sent.
fn unexpected(response: &Response) -> Failure {
    Failure::of(
        Kind::DaemonError,
        format!("the daemon sent an unexpected reply: {response:?}"),
    )
}

/// The profile the daemon resolved for a device, read from the CLI's own registry: by the
/// name the daemon reported, else the registry's own pick for that product and firmware,
/// else a profile with no controls.
fn remote_profile(registry: &Registry, info: &DeviceInfo) -> Profile {
    registry
        .get(&info.profile)
        .or_else(|| registry.select(info.product_id, info.firmware))
        .cloned()
        .unwrap_or_else(|| Registry::generic(info.product_id))
}

#[cfg(test)]
mod tests;
