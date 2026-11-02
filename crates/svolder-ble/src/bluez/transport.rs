// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The BlueZ central transport: scan for Forkbeard devices, connect, write TX frames,
//! and stream RX notifications, over btleplug.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt::Display;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::log::LogLevel;

use async_trait::async_trait;
use btleplug::api::{
    Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use futures::FutureExt;
use futures::future::{BoxFuture, ready};
use futures::stream::{self, BoxStream, StreamExt};
use svolder_proto::UnitId;

use super::releases::{Hold, Peer, Releases, deliver_or_release};
use super::{
    RX_UUID, SERVICE_UUID, TX_UUID, advertisement_from_manufacturer_data, forkbeard_advertisement,
    map_btleplug_error,
};
use crate::advert::Advertisement;
use crate::error::BleError;
use crate::transport::{Advertisements, Link, Listening, Notifications, Transport};

/// Per-operation timeout for a single discover/write/disconnect on the radio.
const OP_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for opening a link. Longer than the other operations: BlueZ has to catch an
/// advertisement before it can start the connection, and then resolves the peripheral's
/// whole GATT database before it reports the device connected.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Environment variable that turns the per-step connect trace on.
const TRACE_VAR: &str = "SVOLDER_BLE_TRACE";

/// How many events already queued on a new event stream a collection drops before it opens
/// its window: enough to clear the devices the adapter is holding, and a bound in case the
/// stream keeps handing over events that are ready.
const DRAIN_CAP: usize = 4096;

/// How long a device a scan heard stays good for a connect to open its link on without scanning
/// for it again: past the end of a selection's window and the reads that follow it, and short of
/// the time a device takes to be carried out of range.
const SIGHTING_FRESH: Duration = Duration::from_secs(10);

/// How long a connect with no fresh sighting scans for its device before it falls back to the
/// one the adapter holds.
const FIND_WINDOW: Duration = Duration::from_secs(3);
/// How often a listen that hears nothing checks whether anyone is left to listen for.
const DONE_CHECK: Duration = Duration::from_millis(250);

/// When a scan stops collecting.
#[derive(Clone, Copy)]
enum Until<'a> {
    /// Keep listening for the whole window, listing every device that advertises.
    WindowEnds,
    /// Stop as soon as one device matches.
    FirstMatch,
    /// Stop as soon as the listen's [`Listening`] has nobody left to listen for; each device it
    /// wants is recorded as a sighting and handed over the moment it is listed.
    Done(&'a dyn Listening),
}

/// The live BlueZ (btleplug) central transport.
pub struct BluezTransport {
    adapter: Adapter,
    /// The adapter's own name, as BlueZ reports it (`hci0`), read once when it was opened:
    /// what a caller that records which controller reached a device needs, and the only thing
    /// that makes the record mean anything when nothing named an adapter on the way in.
    name: Option<String>,
    /// Whether each connect step reports its outcome and duration on stderr.
    trace: bool,
    /// The connects opening a link and the releases taking one down, per device, for
    /// [`Transport::settle`] and [`Transport::settle_for`] to wait on.
    releases: Releases,
    /// Held by every scan the transport runs, from before it starts until its stop has answered
    /// or [`OP_TIMEOUT`] has passed, so a connect's own scan for its device waits for one already
    /// running instead of being refused by BlueZ as already in progress, and a stop still pending
    /// never lands on the next scan.
    scanning: Arc<tokio::sync::Mutex<()>>,
    /// The devices the latest scans heard, for a connect that follows one to open its link on
    /// without scanning for the same device again.
    sightings: Mutex<Sightings<Peripheral>>,
}

/// The peripherals the latest scans heard, by unit id, each with when it was heard.
///
/// A caller that picks a device out of a scan connects to it next, and the peripheral the scan
/// heard is all that connect needs: looking for it again is a second scan for a device heard a
/// moment ago. A sighting is taken once, by the connect it was for; the next connect — a
/// reconnect after a drop — scans as before, and so does one that comes too late to trust it.
struct Sightings<P> {
    heard: Vec<(UnitId, P, Instant)>,
}

impl<P> Sightings<P> {
    /// No sightings yet.
    const fn new() -> Self {
        Self { heard: Vec::new() }
    }

    /// Record that `unit` was heard through `peripheral` at `at`, in place of any earlier
    /// sighting of it.
    fn record(&mut self, unit: UnitId, peripheral: P, at: Instant) {
        self.heard.retain(|(seen, _, _)| *seen != unit);
        self.heard.push((unit, peripheral, at));
    }

    /// The peripheral `unit` was heard through, when that was within [`SIGHTING_FRESH`] of
    /// `now`. The sighting is used up whether or not it was still fresh, and so is every other
    /// one gone stale by `now`.
    fn take(&mut self, unit: UnitId, now: Instant) -> Option<P> {
        let fresh = |at: &Instant| now.saturating_duration_since(*at) <= SIGHTING_FRESH;
        let taken = self
            .heard
            .iter()
            .position(|(seen, _, _)| *seen == unit)
            .map(|at| self.heard.swap_remove(at));
        self.heard.retain(|(_, _, at)| fresh(at));
        taken
            .filter(|(_, _, at)| fresh(at))
            .map(|(_, peripheral, _)| peripheral)
    }
}

impl BluezTransport {
    /// Open the named adapter (e.g. "hci1"), or the first adapter when `None`.
    pub async fn new(adapter: Option<&str>) -> Result<Self, BleError> {
        let manager = Manager::new().await.map_err(|e| map_btleplug_error(&e))?;
        let adapters = manager
            .adapters()
            .await
            .map_err(|e| map_btleplug_error(&e))?;
        let chosen = match adapter {
            None => adapters
                .into_iter()
                .next()
                .ok_or_else(|| BleError::Adapter("no Bluetooth adapter found".into()))?,
            Some(name) => {
                let mut found = None;
                // The names are kept as they go by, so a name that matches none of them can
                // say which ones there were.
                let mut present: Vec<String> = Vec::new();
                for a in adapters {
                    let Ok(info) = a.adapter_info().await else {
                        continue;
                    };
                    if let Some(seen) = adapter_name(&info) {
                        present.push(seen.to_string());
                    }
                    if adapter_info_matches(&info, name) {
                        found = Some(a);
                        break;
                    }
                }
                found.ok_or_else(|| BleError::Adapter(no_adapter_named(name, &present)))?
            }
        };
        let name = chosen
            .adapter_info()
            .await
            .ok()
            .and_then(|info| adapter_name(&info).map(str::to_string));
        Ok(Self {
            adapter: chosen,
            name,
            trace: trace_enabled(std::env::var_os(TRACE_VAR)),
            releases: Releases::default(),
            scanning: Arc::default(),
            sightings: Mutex::new(Sightings::new()),
        })
    }

    /// Wait, at most `bound`, for the connects and releases still in flight for `unit`, or for
    /// every device's with `None`, and trace how that went.
    async fn settle_on(&self, unit: Option<UnitId>, bound: Duration) {
        let started = Instant::now();
        if let Some(settled) = self.releases.settle(unit, bound).await {
            trace_step(self.trace, "settle", started, &settled);
        }
    }

    /// The devices a scan reports as being on the air, whose advertisement carries a
    /// Forkbeard payload `wanted` accepts, each with that advertisement.
    ///
    /// `window` bounds the listening, not the whole call: the reads that turn the window's
    /// last event into an advertisement run past it, and so does the end-of-window sweep
    /// below, which has its own bound.
    ///
    /// A device counts as reported when an event naming it arrives during the window: the scan
    /// asks BlueZ for duplicate data, so a device on the air sends one per advertising report,
    /// and one the adapter only remembers sends none, whoever else holds a discovery session.
    /// Signal strength proves nothing: BlueZ keeps a departed device's last reading until
    /// discovery stops on the adapter, which waits for every D-Bus client's session to end.
    async fn advertisers(
        &self,
        window: Duration,
        until: Until<'_>,
        wanted: impl Fn(&Advertisement) -> bool,
    ) -> Result<Vec<(Peripheral, Advertisement)>, BleError> {
        let scanning = Arc::clone(&self.scanning).lock_owned().await;
        if let Until::Done(listening) = until
            && listening.done()
        {
            return Ok(Vec::new());
        }
        // Taken before the scan starts, so an advertisement that arrives while BlueZ is
        // setting the scan up still reaches the loop below.
        let mut events = bounded(
            self.adapter.events(),
            "opening the adapter's events timed out",
        )
        .await?;
        // A new event stream opens with one `DeviceDiscovered` for every device the adapter
        // is already holding, whether or not it is still there. Drop what the stream has
        // ready before the scan starts; what arrives afterwards arrives because a device
        // sent something.
        for _ in 0..DRAIN_CAP {
            if events.next().now_or_never().flatten().is_none() {
                break;
            }
        }

        let scan = ScanGuard::start(&self.adapter, scanning, self.trace).await?;
        let deadline = tokio::time::Instant::now() + window;
        // Every device an event named during the window, whether or not it could be listed
        // at that moment.
        let mut touched: HashSet<PeripheralId> = HashSet::new();
        let mut listed: HashSet<PeripheralId> = HashSet::new();
        let mut found: Vec<(Peripheral, Advertisement)> = Vec::new();
        loop {
            let event = tokio::select! {
                () = tokio::time::sleep_until(deadline) => break,
                event = events.next() => event,
                // A quiet room still ends a listen nobody is left in.
                () = tokio::time::sleep(DONE_CHECK) => {
                    if let Until::Done(listening) = until
                        && listening.done()
                    {
                        break;
                    }
                    continue;
                }
            };
            // The adapter stopped reporting: nothing more can arrive on this stream.
            let Some(event) = event else { break };
            if let Until::Done(listening) = until
                && listening.done()
            {
                break;
            }
            let Some(id) = advertising_device(&event) else {
                continue;
            };
            touched.insert(id.clone());
            if listed.contains(id) {
                continue;
            }
            let Ok(peripheral) = self.adapter.peripheral(id).await else {
                continue;
            };
            let Ok(Some(props)) = peripheral.properties().await else {
                continue;
            };
            let Some(advert) = forkbeard_advertisement(&props) else {
                continue;
            };
            if !wanted(&advert) {
                continue;
            }
            listed.insert(id.clone());
            if let Until::Done(listening) = until {
                // Kept before it is handed over, so the connect it lets go of opens its link
                // on the peripheral heard here rather than scanning for it again.
                self.record(&[(peripheral.clone(), advert.clone())]);
                listening.heard(&advert);
            }
            found.push((peripheral, advert));
            match until {
                Until::FirstMatch => break,
                Until::Done(listening) if listening.done() => break,
                _ => {}
            }
        }

        // Devices that sent something during the window but could not be listed the moment
        // their event arrived (a failed properties read, or manufacturer data BlueZ had not
        // filled in yet) — only ones an event touched are considered. Signal strength is
        // no proof of presence (above), so this re-reads properties instead. A first-match scan
        // that already has one needs nothing from this. One properties read per touched device,
        // bounded like every other radio operation; a pass cut short keeps what it already listed.
        let sweep = match until {
            Until::WindowEnds => true,
            Until::FirstMatch => found.is_empty(),
            Until::Done(listening) => !listening.done(),
        };
        if sweep {
            let sweep = async {
                for peripheral in self.adapter.peripherals().await.unwrap_or_default() {
                    let id = peripheral.id();
                    if listed.contains(&id) || !touched.contains(&id) {
                        continue;
                    }
                    let Ok(Some(props)) = peripheral.properties().await else {
                        continue;
                    };
                    let Some(advert) = forkbeard_advertisement(&props) else {
                        continue;
                    };
                    if !wanted(&advert) {
                        continue;
                    }
                    listed.insert(peripheral.id());
                    if let Until::Done(listening) = until {
                        self.record(&[(peripheral.clone(), advert.clone())]);
                        listening.heard(&advert);
                    }
                    found.push((peripheral, advert));
                    if let Until::Done(listening) = until
                        && listening.done()
                    {
                        break;
                    }
                }
            };
            let _ = tokio::time::timeout(OP_TIMEOUT, sweep).await;
        }
        scan.stop().await;
        Ok(found)
    }

    /// The peripheral the adapter is holding whose recorded advertisement carries `unit`, or
    /// `None` when none of them does. Bounded like every other radio operation: the look
    /// costs one properties read per held device.
    async fn known(&self, unit: UnitId) -> Result<Option<Peripheral>, BleError> {
        let look = async {
            let peripherals = self
                .adapter
                .peripherals()
                .await
                .map_err(|e| map_btleplug_error(&e))?;
            for peripheral in peripherals {
                if let Ok(Some(props)) = peripheral.properties().await
                    && let Some(advert) = forkbeard_advertisement(&props)
                    && advert.unit_id() == unit
                {
                    return Ok(Some(peripheral));
                }
            }
            Ok(None)
        };
        match tokio::time::timeout(OP_TIMEOUT, look).await {
            Ok(found) => found,
            Err(_) => Err(BleError::Connection(
                "reading the adapter's devices timed out".into(),
            )),
        }
    }

    /// Scan and return the peripheral whose advertisement carries `unit`, stopping the
    /// scan the moment it shows up, so the link that follows is opened on a controller
    /// that has nothing else to do. Gives up after `window`.
    ///
    /// A window that ends with nothing on the air falls back to the device the adapter is
    /// holding for `unit`: its address is all a connection needs, and the connection is
    /// what settles whether the device is still there, so a peripheral the radio did not
    /// report in this window is offered to `connect` rather than refused here.
    async fn find(&self, unit: UnitId, window: Duration) -> Result<Peripheral, BleError> {
        let started = Instant::now();
        let live = self
            .advertisers(window, Until::FirstMatch, |a| a.unit_id() == unit)
            .await
            .and_then(|seen| {
                seen.into_iter()
                    .next()
                    .map(|(peripheral, _)| peripheral)
                    .ok_or(BleError::NotFound)
            });
        trace_step(self.trace, "find", started, &live);
        match live {
            Err(BleError::NotFound) => {
                let started = Instant::now();
                let held = self
                    .known(unit)
                    .await
                    .and_then(|p| p.ok_or(BleError::NotFound));
                trace_step(self.trace, "known", started, &held);
                held
            }
            other => other,
        }
    }

    /// The sightings, locked whatever a holder that panicked left behind: they are only hints
    /// for the next connect, which finds the device afresh without them.
    fn sightings(&self) -> MutexGuard<'_, Sightings<Peripheral>> {
        self.sightings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Keep what a scan heard for the connect that may follow it.
    fn record(&self, heard: &[(Peripheral, Advertisement)]) {
        let now = Instant::now();
        let mut sightings = self.sightings();
        for (peripheral, advert) in heard {
            sightings.record(advert.unit_id(), peripheral.clone(), now);
        }
    }

    /// A future that resolves when this adapter reports `peripheral` disconnected. Taken
    /// before the connection is opened, so a drop at any point during setup still reaches
    /// the link's notification stream.
    ///
    /// A disconnect the adapter reports before the link is built — a connection from
    /// earlier being torn down, say — is waiting on the stream and ends the notifications
    /// on their first poll, so the session sees a link that closed at once and reconnects.
    async fn disconnect_of(
        &self,
        peripheral: &Peripheral,
    ) -> Result<BoxFuture<'static, ()>, BleError> {
        let started = Instant::now();
        let events = bounded(
            self.adapter.events(),
            "opening the adapter's events timed out",
        )
        .await;
        trace_step(self.trace, "events", started, &events);
        let events = events?;
        let id = peripheral.id();
        Ok(events
            .filter(move |e| {
                ready(matches!(e, CentralEvent::DeviceDisconnected(pid) if *pid == id))
            })
            .into_future()
            .map(|_| ())
            .boxed())
    }
}

#[async_trait]
impl Transport for BluezTransport {
    fn adapter(&self) -> Option<&str> {
        self.name.as_deref()
    }

    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        let started = Instant::now();
        let seen = self.advertisers(window, Until::WindowEnds, |_| true).await;
        trace_count(self.trace, "scan", started, &seen);
        let seen = seen?;
        self.record(&seen);
        let s: BoxStream<'static, Advertisement> =
            stream::iter(seen.into_iter().map(|(_, advert)| advert)).boxed();
        Ok(s)
    }

    /// A scan that stops the moment `unit` advertises, keeping the peripheral it heard for the
    /// connect that follows.
    async fn sight(
        &self,
        unit: UnitId,
        window: Duration,
    ) -> Result<Option<Advertisement>, BleError> {
        let started = Instant::now();
        let seen = self
            .advertisers(window, Until::FirstMatch, |a| a.unit_id() == unit)
            .await;
        trace_count(self.trace, "sight", started, &seen);
        let seen = seen?;
        self.record(&seen);
        Ok(seen.into_iter().next().map(|(_, advert)| advert))
    }

    /// A scan that hands each device `listening` wants over the moment it is heard, keeping its
    /// peripheral for the connect that follows, and stops as soon as nobody is left.
    async fn listen(&self, window: Duration, listening: &dyn Listening) -> Result<(), BleError> {
        let started = Instant::now();
        let seen = self
            .advertisers(window, Until::Done(listening), |a| {
                listening.wants(a.unit_id())
            })
            .await;
        trace_count(self.trace, "listen", started, &seen);
        seen.map(|_| ())
    }

    /// Every Forkbeard the adapter is holding, with the last advertisement it took from
    /// each. No scan runs and no window is waited out: whatever BlueZ has on these devices
    /// is what comes back, which once discovery has stopped on the adapter carries no
    /// signal strength. Bounded like every other radio operation: the pass costs one
    /// properties read per held device, and one cut short hands over what it has.
    async fn held(&self) -> Result<Advertisements, BleError> {
        let mut adverts = Vec::new();
        let gather = async {
            let peripherals = self
                .adapter
                .peripherals()
                .await
                .map_err(|e| map_btleplug_error(&e))?;
            for peripheral in peripherals {
                if let Ok(Some(props)) = peripheral.properties().await
                    && let Some(advert) = forkbeard_advertisement(&props)
                {
                    adverts.push(advert);
                }
            }
            Ok::<(), BleError>(())
        };
        if let Ok(failed @ Err(_)) = tokio::time::timeout(OP_TIMEOUT, gather).await {
            failed?;
        }
        let s: BoxStream<'static, Advertisement> = stream::iter(adverts).boxed();
        Ok(s)
    }

    /// Open a link to `unit`, on the peripheral a scan has just heard it through when there is
    /// one, else on the one `find` looks for.
    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        // Never race the previous link's still-pending release of this device.
        self.settle_for(unit, OP_TIMEOUT).await;
        let started = Instant::now();
        let sighted = self.sightings().take(unit, started);
        // Only a sighting taken is a step worth a line: without one, `find` says what it did.
        let peripheral = match sighted {
            Some(peripheral) => {
                trace_step(
                    self.trace,
                    "sighted",
                    started,
                    &Ok::<_, BleError>(&peripheral),
                );
                peripheral
            }
            None => self.find(unit, FIND_WINDOW).await?,
        };
        // A device in bootloader mode advertises firmware build 0; the session also guards
        // this, but refuse here too rather than open a link that must not carry commands.
        // Bounded like every other radio operation; a read that does not arrive leaves the
        // guard to the session.
        if let Ok(Ok(Some(props))) = tokio::time::timeout(OP_TIMEOUT, peripheral.properties()).await
            && let Some(a) =
                advertisement_from_manufacturer_data(&props.manufacturer_data, props.rssi)
            && a.manufacturer.in_bootloader()
        {
            return Err(BleError::Bootloader);
        }
        let disconnected = self.disconnect_of(&peripheral).await?;

        // Release whatever BlueZ has half-built when the link has nowhere to go, whether a
        // step failed or the caller walked away mid-setup. A connection BlueZ is still
        // setting up outlives the call that asked for it, and every later attempt is
        // refused as already in progress until it is cancelled. Bounded like every other
        // radio operation.
        let orphan = peripheral.clone();
        let opened = deliver_or_release(
            unit,
            open(
                peripheral,
                disconnected,
                self.trace,
                unit,
                self.releases.clone(),
            ),
            move |unclaimed: Option<Box<dyn Link>>| async move {
                match unclaimed {
                    // The link's own release, which also spares its drop from asking again.
                    Some(link) => {
                        let _ = link.disconnect().await;
                    }
                    None => {
                        let _ = orphan.take_down().await;
                    }
                }
            },
            &self.releases,
        )
        .await;
        if opened.is_err() {
            // The caller is still here to be told, and is likely to try again straight away,
            // so wait for the release this failure started rather than letting the next
            // attempt meet a connection BlueZ is still taking down.
            self.settle_for(unit, OP_TIMEOUT).await;
        }
        opened
    }

    /// Wait for the connects this transport still has in flight and the disconnects that
    /// follow them, at most `bound`. Each runs on a task of its own so that a caller who
    /// stopped waiting for a link is not made to wait for the radio; a caller about to stop
    /// driving the runtime those tasks live on comes here first. Work still going when the
    /// bound passes is left to run for as long as the runtime lasts, and stays recorded, so
    /// the next call waits for it too.
    async fn settle(&self, bound: Duration) {
        self.settle_on(None, bound).await;
    }

    async fn settle_for(&self, unit: UnitId, bound: Duration) {
        self.settle_on(Some(unit), bound).await;
    }
}

/// What a scan asks of the adapter: discovery of the Forkbeard service started, and stopped.
/// The BlueZ adapter is the one there is; a test stands in its own to see what is asked when.
trait Discovery: Clone + Send + Sync + 'static {
    /// Start discovering, bounded by [`OP_TIMEOUT`].
    fn start(&self) -> BoxFuture<'static, Result<(), BleError>>;
    /// Stop discovering, bounded by [`OP_TIMEOUT`]; what BlueZ answers is not needed.
    fn stop(&self) -> BoxFuture<'static, ()>;
}

impl Discovery for Adapter {
    fn start(&self) -> BoxFuture<'static, Result<(), BleError>> {
        let adapter = self.clone();
        async move {
            bounded(
                adapter.start_scan(ScanFilter {
                    services: vec![SERVICE_UUID],
                }),
                "starting the scan timed out",
            )
            .await
        }
        .boxed()
    }

    fn stop(&self) -> BoxFuture<'static, ()> {
        let adapter = self.clone();
        async move {
            let _ = tokio::time::timeout(OP_TIMEOUT, adapter.stop_scan()).await;
        }
        .boxed()
    }
}

/// A scan the transport started, stopped on every way out of the collection that started
/// it: the list it returns, an error on the way up, or a caller that drops the future
/// while the window is still open.
struct ScanGuard<D: Discovery> {
    discovery: Option<D>,
    /// The transport's scan lock, let go when the stop has answered or [`OP_TIMEOUT`] has
    /// passed, whichever comes first, by the one task that stops the scan (see
    /// [`spawn_stop`]), so the next scan starts on an adapter that has stopped.
    scanning: Option<tokio::sync::OwnedMutexGuard<()>>,
}

impl<D: Discovery> ScanGuard<D> {
    /// Start discovery on a task of its own, which holds the scan lock until BlueZ has answered.
    /// A caller still waiting then gets the scan. A caller that left meanwhile (its future
    /// dropped while BlueZ was answering) leaves a start that has landed, so the task stops it
    /// and lets the lock go only once that stop has answered or [`OP_TIMEOUT`] has passed: the
    /// next scan starts on an adapter that is not discovering. The task is never aborted, for a
    /// D-Bus call dropped mid-flight is what leaves discovery on. `trace` reports a start that
    /// had to be retried.
    async fn start(
        discovery: &D,
        scanning: tokio::sync::OwnedMutexGuard<()>,
        trace: bool,
    ) -> Result<Self, BleError> {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return Err(BleError::Connection(
                "the scan's start ended without an answer".into(),
            ));
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let starting = discovery.clone();
        runtime.spawn(async move {
            match started(&starting, trace).await {
                Ok(()) => {
                    let scan = Self {
                        discovery: Some(starting),
                        scanning: Some(scanning),
                    };
                    // A guard nobody reads stops through its own `Drop`; one handed back
                    // here is stopped in place, so the lock waits for the stop.
                    if let Err(Ok(scan)) = tx.send(Ok(scan)) {
                        scan.stop().await;
                    }
                }
                // The start failed, so nothing is stopped; one that lands late is the next
                // scan's retry to clear.
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        });
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(BleError::Connection(
                "the scan's start ended without an answer".into(),
            )),
        }
    }

    /// Stop the scan and wait for the task that does it, so what the caller does next runs on
    /// a controller that is no longer scanning. The stop and the lock are the task's from the
    /// moment it is spawned, so a caller dropped while it waits neither stops the scan twice
    /// nor lets the lock go early.
    async fn stop(mut self) {
        if let Some(discovery) = self.discovery.take()
            && let Some(stopping) = spawn_stop(discovery, self.scanning.take())
        {
            let _ = stopping.await;
        }
    }
}

impl<D: Discovery> Drop for ScanGuard<D> {
    /// Ask for the scan to stop on the way out. A destructor cannot wait for an answer, so
    /// the stop goes to a task of its own, which holds the scan lock until it has answered or
    /// [`OP_TIMEOUT`] has passed.
    fn drop(&mut self) {
        if let Some(discovery) = self.discovery.take() {
            spawn_stop(discovery, self.scanning.take());
        }
    }
}

/// Start discovery, and once more after a stop when the first try fails: a start cut short
/// earlier (past its bound) may still have landed in BlueZ, which then refuses every later
/// start as already in progress, and btleplug reports that refusal as plain text.
async fn started<D: Discovery>(discovery: &D, trace: bool) -> Result<(), BleError> {
    let at = Instant::now();
    if let Err(first) = discovery.start().await {
        trace_step(trace, "start scan", at, &Err::<(), _>(first));
        discovery.stop().await;
        discovery.start().await?;
    }
    Ok(())
}

/// Stop discovery on a task of its own, and let go of the scan lock when the stop has answered
/// or [`OP_TIMEOUT`] has passed. Dropping the handle leaves the task running. `None` when no
/// runtime is there to run it.
fn spawn_stop<D: Discovery>(
    discovery: D,
    scanning: Option<tokio::sync::OwnedMutexGuard<()>>,
) -> Option<tokio::task::JoinHandle<()>> {
    let runtime = tokio::runtime::Handle::try_current().ok()?;
    Some(runtime.spawn(async move {
        discovery.stop().await;
        drop(scanning);
    }))
}

/// The device an event names, for the events that say a device has just been on the air:
/// one the adapter has newly discovered, one whose advertised properties changed, one whose
/// manufacturer data was received, and one whose signal strength was reported.
fn advertising_device(event: &CentralEvent) -> Option<&PeripheralId> {
    match event {
        CentralEvent::DeviceDiscovered(id)
        | CentralEvent::DeviceUpdated(id)
        | CentralEvent::ManufacturerDataAdvertisement { id, .. }
        | CentralEvent::RssiUpdate { id, .. } => Some(id),
        _ => None,
    }
}

/// A link to one device: the connection BlueZ holds, the characteristic frames go out on, and
/// the notification stream frames come back on.
///
/// Dropping the link releases the connection, so a link that reaches nobody — a result the
/// caller stopped waiting for, a session dropped with the link still in it — does not leave
/// BlueZ holding a connection nobody owns. That drop is [`Hold`]'s.
struct BluezLink {
    /// The connection, taken down by `disconnect`, or by the drop when no `disconnect`
    /// succeeded.
    hold: Hold<Peripheral>,
    tx: Characteristic,
    /// The RX notification stream, built during `connect` and handed out once.
    stream: Mutex<Option<Notifications>>,
}

impl Peer for Peripheral {
    fn take_down(&self) -> BoxFuture<'static, Result<(), BleError>> {
        let peripheral = self.clone();
        async move {
            match tokio::time::timeout(OP_TIMEOUT, peripheral.disconnect()).await {
                Ok(done) => done.map_err(|e| map_btleplug_error(&e)),
                Err(_) => Err(BleError::Connection("disconnect timed out".into())),
            }
        }
        .boxed()
    }
}

#[async_trait]
impl Link for BluezLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        match tokio::time::timeout(
            OP_TIMEOUT,
            self.hold
                .peer()
                .write(&self.tx, frame, WriteType::WithoutResponse),
        )
        .await
        {
            Ok(r) => r.map_err(|e| map_btleplug_error(&e)),
            Err(_) => Err(BleError::Connection("write timed out".into())),
        }
    }

    fn notifications(&self) -> Notifications {
        match self.stream.lock().expect("stream mutex").take() {
            Some(stream) => stream,
            None => stream::empty().boxed(),
        }
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.hold.release().await
    }
}

/// Connect, discover, and subscribe on a peripheral `find` has already picked out. The
/// link's notification stream ends when `disconnected` resolves, which is the `None` that
/// `Session` uses to notice a dropped link and start reconnecting; btleplug's own
/// notification stream never ends by itself.
async fn open(
    peripheral: Peripheral,
    disconnected: BoxFuture<'static, ()>,
    trace: bool,
    unit: UnitId,
    releases: Releases,
) -> Result<Box<dyn Link>, BleError> {
    let started = Instant::now();
    let connected = peripheral.connect_with_timeout(CONNECT_TIMEOUT).await;
    trace_step(trace, "connect", started, &connected);
    connected.map_err(|e| map_btleplug_error(&e))?;

    let started = Instant::now();
    let discovered = peripheral.discover_services_with_timeout(OP_TIMEOUT).await;
    trace_step(trace, "discover", started, &discovered);
    discovered.map_err(|e| map_btleplug_error(&e))?;

    let started = Instant::now();
    let found = characteristics(&peripheral);
    trace_step(trace, "characteristics", started, &found);
    let (tx, rx) = found?;

    let started = Instant::now();
    let subscribed = bounded(peripheral.subscribe(&rx), "subscribe timed out").await;
    trace_step(trace, "subscribe", started, &subscribed);
    subscribed?;

    // Awaiting the stream here installs the D-Bus signal match before `connect` returns,
    // so a reply to the first frame the session writes has somewhere to land.
    let started = Instant::now();
    let notes = bounded(
        peripheral.notifications(),
        "taking the notification stream timed out",
    )
    .await;
    trace_step(trace, "notifications", started, &notes);
    let notes = notes?;

    let stream = notes
        .filter_map(|vn| ready((vn.uuid == RX_UUID).then_some(vn.value)))
        .take_until(disconnected)
        .boxed();
    Ok(Box::new(BluezLink {
        hold: Hold::new(peripheral, unit, releases),
        tx,
        stream: Mutex::new(Some(stream)),
    }))
}

/// Run one radio operation under [`OP_TIMEOUT`], reporting `timed_out` when the bound passes.
/// Without a bound of its own an operation waits out bluez-async's D-Bus method timeout, 30
/// seconds, which outlasts every bound a caller closing down waits on.
async fn bounded<T, F>(op: F, timed_out: &str) -> Result<T, BleError>
where
    F: Future<Output = btleplug::Result<T>>,
{
    match tokio::time::timeout(OP_TIMEOUT, op).await {
        Ok(done) => done.map_err(|e| map_btleplug_error(&e)),
        Err(_) => Err(BleError::Connection(timed_out.into())),
    }
}

/// Report one connect step's outcome and duration on stderr, when `on`.
fn trace_step<T, E: Display>(on: bool, step: &str, started: Instant, result: &Result<T, E>) {
    if !on {
        return;
    }
    let ms = started.elapsed().as_millis();
    match result {
        Ok(_) => crate::log::line(LogLevel::Info, &format!("{step} ok after {ms} ms")),
        Err(e) => crate::log::line(LogLevel::Warn, &format!("{step} failed after {ms} ms: {e}")),
    }
}

/// Report how many devices a step listed, and how long it listened, on stderr, when `on`.
fn trace_count<T, E: Display>(on: bool, step: &str, started: Instant, result: &Result<Vec<T>, E>) {
    if !on {
        return;
    }
    let ms = started.elapsed().as_millis();
    match result {
        Ok(listed) => crate::log::line(
            LogLevel::Info,
            &format!("{step} listed {} after {ms} ms", listed.len()),
        ),
        Err(e) => crate::log::line(LogLevel::Warn, &format!("{step} failed after {ms} ms: {e}")),
    }
}

/// True when a btleplug `adapter_info()` string names this adapter (matches the leading
/// token before the first space, e.g. "hci1" in "hci1 (usb:...)").
fn adapter_info_matches(info: &str, name: &str) -> bool {
    adapter_name(info) == Some(name)
}

/// The adapter's name, as the first word of what BlueZ says about it.
fn adapter_name(info: &str) -> Option<&str> {
    info.split_whitespace().next()
}

/// What a named adapter no controller answers to earns: the name that was asked for, then the
/// adapters there are to ask for instead.
fn no_adapter_named(name: &str, present: &[String]) -> String {
    if present.is_empty() {
        return format!("no adapter named {name}; no adapters present");
    }
    format!("no adapter named {name}; adapters: {}", present.join(", "))
}

/// True when `SVOLDER_BLE_TRACE`'s value asks for the step trace: set to anything other
/// than nothing at all, the empty string, or `0`.
fn trace_enabled(value: Option<OsString>) -> bool {
    match value {
        None => false,
        Some(v) => !v.is_empty() && v.as_os_str() != "0",
    }
}

/// The peripheral's TX and RX characteristics, or the reason one of them is missing.
fn characteristics(peripheral: &Peripheral) -> Result<(Characteristic, Characteristic), BleError> {
    let chars = peripheral.characteristics();
    let find = |uuid, what: &str| {
        chars
            .iter()
            .find(|c| c.uuid == uuid)
            .cloned()
            .ok_or_else(|| BleError::Connection(format!("{what} characteristic not found")))
    };
    Ok((find(TX_UUID, "TX")?, find(RX_UUID, "RX")?))
}

#[cfg(test)]
mod tests;
