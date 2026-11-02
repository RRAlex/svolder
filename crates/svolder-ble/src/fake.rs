// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An in-process transport backed by a [`svolder_fake::FakeDevice`], for tests and for the
//! CLI's default `--transport fake` mode, which needs no radio.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use svolder_fake::{FakeDevice, Signal};
use svolder_proto::{ADVERT_COMPANY_ID, UnitId};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::advert::{Advertisement, ManufacturerRecord};
use crate::error::BleError;
use crate::transport::{Advertisements, Link, Notifications, Transport};

mod fleet;
pub use fleet::FakeFleet;

/// What the fake device's advertisement says about its code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Advertised {
    /// The code the device holds: what a device that has just advertised says.
    #[default]
    Truthfully,
    /// This code, which is not the device's, until the next link to it is released or lost: what
    /// an adapter holding an old advertisement says of a device a central is holding, which
    /// advertises again only once the link is gone.
    UntilReleased(svolder_proto::Passphrase),
    /// This code, which is not the device's, for good.
    Always(svolder_proto::Passphrase),
    /// `stale`, which is not the device's, until the next link to it is released or lost, and
    /// then `next`, which is not its code either, for good: a device whose code was changed
    /// twice, or one that advertises something else than the code it holds.
    ThenAlways {
        /// What the advertisement says until the link ends.
        stale: svolder_proto::Passphrase,
        /// What it says from then on.
        next: svolder_proto::Passphrase,
    },
}

/// What the fake device does on the air, shared with the links it hands out so a link's end
/// can change it.
#[derive(Default)]
struct Air {
    /// What the advertisement says of the device's code.
    advertised: Mutex<Advertised>,
    /// Whether the device advertises nothing from the next release on, while the adapter goes
    /// on holding its last advertisement.
    quiet_after_release: AtomicBool,
    /// Whether the device is advertising nothing now.
    quiet: AtomicBool,
    /// How many links have been opened.
    connects: AtomicUsize,
    /// How many links a caller has released.
    releases: AtomicUsize,
    /// How long after the next link comes up it is cut, once, when a test armed it.
    sever_after: Mutex<Option<Duration>>,
    /// Off the air for good: not heard, not reached.
    gone: AtomicBool,
}

/// Take the device off the air for good: its link, if one is up, is cut.
fn go_away(current: &Mutex<Option<Current>>, air: &Air) {
    air.gone.store(true, Ordering::SeqCst);
    let ended = current.lock().expect("current link mutex").take().is_some();
    if ended {
        air.link_ended();
    }
}

impl Air {
    /// The device is on the air again after a link to it ended.
    fn link_ended(&self) {
        let mut advertised = self.advertised.lock().expect("advertised mutex");
        *advertised = match *advertised {
            Advertised::UntilReleased(_) => Advertised::Truthfully,
            Advertised::ThenAlways { next, .. } => Advertised::Always(next),
            other => other,
        };
    }
}

/// The link that is up: which `connect` opened it, and the sender its notifications go out on.
struct Current {
    /// The link's ordinal, the `connects` count when it was opened.
    token: usize,
    tx: mpsc::UnboundedSender<Vec<u8>>,
}

/// A transport that talks to one in-process fake device.
pub struct FakeTransport {
    device: Arc<Mutex<FakeDevice>>,
    unit: UnitId,
    /// The link that is up, shared with every `FakeLink`. `connect` replaces it, ending the
    /// earlier link's stream; `sever` clears it without a replacement.
    current: Arc<Mutex<Option<Current>>>,
    /// How many times a caller has asked this transport to settle.
    settles: Arc<AtomicUsize>,
    /// The devices a caller asked this transport to settle, in order.
    settled: Arc<Mutex<Vec<UnitId>>>,
    /// The task [`drive_signal`] started, if one was asked for. The transport owns it, so
    /// letting the transport go stops the signal.
    signal: Mutex<Option<JoinHandle<()>>>,
    /// What the device says and does on the air.
    air: Arc<Air>,
}

impl FakeTransport {
    /// Wrap a freshly seeded fake device for one product.
    pub fn from_profile(
        profile: &svolder_profiles::Profile,
        unit: UnitId,
        firmware_build: u16,
        passphrase: svolder_proto::Passphrase,
    ) -> Self {
        Self::from_device(
            FakeDevice::from_profile(profile, unit, firmware_build, passphrase),
            unit,
        )
    }

    /// Wrap a fake device that is already built.
    fn from_device(device: FakeDevice, unit: UnitId) -> Self {
        Self {
            device: Arc::new(Mutex::new(device)),
            unit,
            current: Arc::new(Mutex::new(None)),
            settles: Arc::new(AtomicUsize::new(0)),
            settled: Arc::default(),
            signal: Mutex::new(None),
            air: Arc::new(Air::default()),
        }
    }

    /// The transport the `SVOLDER_FAKE_*` knobs describe, as the environment gives them: the
    /// device [`FakeTransport::from_knobs`] builds, and with `SVOLDER_FAKE_MORE` set, the fleet
    /// of it and the devices that names ([`FakeFleet`]).
    pub fn transport_from_env(
        registry: &svolder_profiles::Registry,
    ) -> Result<Arc<dyn Transport>, String> {
        Self::transport_from_knobs(registry, |name| std::env::var(name).ok())
    }

    /// [`FakeTransport::transport_from_env`] with the knobs `knob` gives.
    pub fn transport_from_knobs(
        registry: &svolder_profiles::Registry,
        knob: impl Fn(&str) -> Option<String>,
    ) -> Result<Arc<dyn Transport>, String> {
        let first = Self::from_knobs(registry, &knob)?;
        match knob("SVOLDER_FAKE_MORE") {
            Some(more) => {
                let signal = knob("SVOLDER_FAKE_SIGNAL").as_deref() == Some("1");
                FakeFleet::from_more(registry, first, &more, signal)
                    .map(|fleet| Arc::new(fleet) as Arc<dyn Transport>)
            }
            None => Ok(Arc::new(first)),
        }
    }

    /// Build the fake device from the `SVOLDER_FAKE_*` knobs, as the environment gives them.
    pub fn from_env(registry: &svolder_profiles::Registry) -> Result<Self, String> {
        Self::from_knobs(registry, |name| std::env::var(name).ok())
    }

    /// Build the fake device from knobs `knob` answers by variable name, `None` for one that is
    /// not set:
    /// - `SVOLDER_FAKE_PROFILE`, the profile (default `gungnir2-v2`);
    /// - `SVOLDER_FAKE_UNIT`, the unit id (default `0102030405060708`);
    /// - `SVOLDER_FAKE_FIRMWARE`, the firmware build (default `3`);
    /// - `SVOLDER_FAKE_PASSPHRASE`, the code (`1234` by default, `none` for no passphrase, or
    ///   another 4-digit string);
    /// - `SVOLDER_FAKE_FAULTS`, `1` to seed every `Fault` control of the profile as active;
    /// - `SVOLDER_FAKE_SIGNAL`, `1` to play a signal on the meters, which needs a profile whose
    ///   meters have a scale, such as `fake-amp`, and a Tokio runtime;
    /// - `SVOLDER_FAKE_LOKI_REPORTS`, `full` (the default) to report a Loki Max band on its own
    ///   three-id form, or `short` to report it on its two-id form (see
    ///   [`FakeDevice::report_aliases`]); any other value is an error naming both;
    /// - `SVOLDER_FAKE_SILENT_RECALL`, `1` to have a device-slot recall set the bands without
    ///   reporting them (see [`FakeDevice::silent_recall`]);
    /// - `SVOLDER_FAKE_QUIET_SAME`, `1` to have every control stay silent on a Command equal to
    ///   the value it holds (see [`FakeDevice::quiet_same`]);
    /// - `SVOLDER_FAKE_SEVER_AFTER_MS`, the milliseconds after which the first link is cut (see
    ///   [`FakeTransport::sever_next_link_after`]), which needs a Tokio runtime;
    /// - `SVOLDER_FAKE_GONE_AFTER_MS`, the milliseconds after which the device goes off the air
    ///   for good (see [`FakeTransport::go_away_after`]), which needs a Tokio runtime;
    /// - `SVOLDER_FAKE_GONE_FILE`, a path: the device goes off the air for good when a file
    ///   appears there (see [`FakeTransport::go_away_when_file`]), which needs a Tokio runtime.
    ///
    /// A value that does not parse is an error naming its variable. `SVOLDER_FAKE_MORE` is read
    /// by [`FakeTransport::transport_from_knobs`], and the knobs above describe the first device
    /// alone, except `SVOLDER_FAKE_SIGNAL`, which plays on every device of a fleet.
    pub fn from_knobs(
        registry: &svolder_profiles::Registry,
        knob: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let profile_name = knob("SVOLDER_FAKE_PROFILE").unwrap_or_else(|| "gungnir2-v2".into());
        let profile = registry
            .get(&profile_name)
            .ok_or_else(|| format!("no profile {profile_name}"))?
            .clone();
        let unit: UnitId = knob("SVOLDER_FAKE_UNIT")
            .unwrap_or_else(|| "0102030405060708".into())
            .parse()
            .map_err(|_| "bad SVOLDER_FAKE_UNIT".to_string())?;
        let firmware: u16 = knob("SVOLDER_FAKE_FIRMWARE")
            .unwrap_or_else(|| "3".into())
            .parse()
            .map_err(|_| "bad SVOLDER_FAKE_FIRMWARE".to_string())?;
        let passphrase = svolder_proto::Passphrase::from_text(
            knob("SVOLDER_FAKE_PASSPHRASE").as_deref().unwrap_or("1234"),
        )
        .map_err(|e| format!("bad SVOLDER_FAKE_PASSPHRASE: {e}"))?;
        let report_aliases = match knob("SVOLDER_FAKE_LOKI_REPORTS").as_deref() {
            None | Some("full") => false,
            Some("short") => true,
            Some(other) => {
                return Err(format!(
                    "bad SVOLDER_FAKE_LOKI_REPORTS: {other:?} is neither full nor short"
                ));
            }
        };
        let silent_recall = knob("SVOLDER_FAKE_SILENT_RECALL").as_deref() == Some("1");
        let quiet_same = knob("SVOLDER_FAKE_QUIET_SAME").as_deref() == Some("1");
        let device = FakeDevice::from_profile(&profile, unit, firmware, passphrase)
            .report_aliases(report_aliases)
            .silent_recall(silent_recall)
            .quiet_same(quiet_same);
        let transport = Self::from_device(device, unit);
        if knob("SVOLDER_FAKE_FAULTS").as_deref() == Some("1") {
            let mut device = transport.device.lock().expect("device mutex");
            for control in &profile.controls {
                if control.kind == svolder_profiles::ControlKind::Fault {
                    device.set_raw(control.id, svolder_proto::Value::Bool(true));
                }
            }
        }
        let sever_after = knob("SVOLDER_FAKE_SEVER_AFTER_MS")
            .map(|ms| {
                ms.parse::<u64>()
                    .map_err(|_| "bad SVOLDER_FAKE_SEVER_AFTER_MS".to_string())
            })
            .transpose()?;
        let gone_after = knob("SVOLDER_FAKE_GONE_AFTER_MS")
            .map(|ms| {
                ms.parse::<u64>()
                    .map_err(|_| "bad SVOLDER_FAKE_GONE_AFTER_MS".to_string())
            })
            .transpose()?;
        if knob("SVOLDER_FAKE_SIGNAL").as_deref() == Some("1") {
            drive_signal(&transport, svolder_fake::SIGNAL_PERIOD);
        }
        if let Some(ms) = sever_after {
            transport.sever_next_link_after(Duration::from_millis(ms));
        }
        if let Some(ms) = gone_after {
            transport.go_away_after(Duration::from_millis(ms));
        }
        if let Some(path) = knob("SVOLDER_FAKE_GONE_FILE") {
            transport.go_away_when_file(path.into());
        }
        Ok(transport)
    }

    /// Say what the advertisement carries as the device's code from now on, which a test uses
    /// to hand a session a code that is not the device's.
    pub fn advertise(&self, how: Advertised) {
        *self.air.advertised.lock().expect("advertised mutex") = how;
    }

    /// Have the device advertise nothing once a link to it has been released, while
    /// [`Transport::held`] goes on answering with the advertisement it had: a device that is
    /// not heard again in the window a caller listens for.
    pub fn go_quiet_after_release(&self) {
        self.air.quiet_after_release.store(true, Ordering::SeqCst);
    }

    /// Have the device advertise again after `go_quiet_after_release`: it is heard from now on,
    /// and is not silenced by a later release either.
    pub fn speak_again(&self) {
        self.air.quiet_after_release.store(false, Ordering::SeqCst);
        self.air.quiet.store(false, Ordering::SeqCst);
    }

    /// How many links have been opened on this transport.
    pub fn connects(&self) -> usize {
        self.air.connects.load(Ordering::SeqCst)
    }

    /// How many links a caller has released with [`Link::disconnect`].
    pub fn releases(&self) -> usize {
        self.air.releases.load(Ordering::SeqCst)
    }

    /// Shared handle to the underlying device, for assertions in tests.
    pub fn device(&self) -> Arc<Mutex<FakeDevice>> {
        Arc::clone(&self.device)
    }

    /// How many times a caller has asked this transport to [`Transport::settle`], so a test
    /// can prove that a session waiting on the radio asks before it stops driving it.
    pub fn settles(&self) -> usize {
        self.settles.load(Ordering::SeqCst)
    }

    /// The devices a caller asked this transport to settle for, in order.
    pub fn settled(&self) -> Vec<UnitId> {
        self.settled.lock().expect("settled mutex").clone()
    }

    /// Test seam: cut the current link the way a device dropping the connection would,
    /// without a new `connect` replacing it. The next read of the notification stream the
    /// session holds ends, and any further write on that link fails.
    pub fn sever(&self) {
        *self.current.lock().expect("current link mutex") = None;
        self.air.link_ended();
    }

    /// Test seam: cut the next link `after` it comes up, the way a device dropping the
    /// connection would, once. The link is cut only if it is still the one up by then; the links
    /// after it are left alone. Needs a Tokio runtime at the `connect` it arms.
    pub fn sever_next_link_after(&self, after: Duration) {
        *self.air.sever_after.lock().expect("sever mutex") = Some(after);
    }

    /// Test seam: take the device off the air `after` this long, for good: the link that is up
    /// is cut, and it is neither heard by a scan nor held nor reached afterwards. Needs a Tokio
    /// runtime.
    pub fn go_away_after(&self, after: Duration) {
        let current = Arc::clone(&self.current);
        let air = Arc::clone(&self.air);
        tokio::spawn(async move {
            tokio::time::sleep(after).await;
            go_away(&current, &air);
        });
    }

    /// Test seam: take the device off the air, as [`FakeTransport::go_away_after`] does, the
    /// moment a file appears at `path`, so a test drops the device on its own cue instead of on
    /// a clock. The path is looked at every 50 ms. Needs a Tokio runtime.
    pub fn go_away_when_file(&self, path: std::path::PathBuf) {
        let current = Arc::clone(&self.current);
        let air = Arc::clone(&self.air);
        tokio::spawn(async move {
            while !path.exists() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            go_away(&current, &air);
        });
    }

    /// Test seam: notify the current link of these statuses, as a device reporting on its own
    /// would. Nothing is sent while no link is up.
    pub fn push(&self, messages: &[svolder_proto::Message]) {
        let Ok(frame) = svolder_proto::encode_notification(messages) else {
            return;
        };
        if let Some(tx) = self.current_sender() {
            let _ = tx.send(frame);
        }
    }

    /// The sender of the link that is up, if one is.
    fn current_sender(&self) -> Option<mpsc::UnboundedSender<Vec<u8>>> {
        self.current
            .lock()
            .expect("current link mutex")
            .as_ref()
            .map(|current| current.tx.clone())
    }

    /// The advertisement the device broadcasts now, carrying the code [`FakeTransport::advertise`]
    /// says.
    pub fn advertisement(&self) -> Advertisement {
        let mut manufacturer = self
            .device
            .lock()
            .expect("device mutex")
            .manufacturer_data();
        let said = match *self.air.advertised.lock().expect("advertised mutex") {
            Advertised::Truthfully => None,
            Advertised::UntilReleased(code)
            | Advertised::Always(code)
            | Advertised::ThenAlways { stale: code, .. } => Some(code),
        };
        if let Some(code) = said {
            manufacturer.encrypted_passphrase = match code.is_none() {
                true => 0,
                false => svolder_proto::Passphrase::encrypt(
                    code.encoded(),
                    svolder_proto::Passphrase::advert_key(manufacturer.unit_id),
                ),
            };
        }
        let bytes = manufacturer.to_bytes().to_vec();
        Advertisement {
            manufacturer,
            rssi: Some(-50),
            record: Some(ManufacturerRecord {
                company_id: ADVERT_COMPANY_ID,
                bytes,
            }),
        }
    }
}

impl Drop for FakeTransport {
    /// The signal task holds the device and the notification slot itself, so nothing else
    /// would end it: it is stopped here, with the transport that started it.
    fn drop(&mut self) {
        if let Some(task) = self.signal.lock().expect("signal mutex").take() {
            task.abort();
        }
    }
}

/// Play a signal on the fake device's meters, one notification every `period` for as long as
/// the transport lives.
///
/// Nothing is sent while no link is up — a device off the air notifies nobody — and a device
/// whose meters have no scale has nothing to play, so the task ends at once. It must be
/// started inside a Tokio runtime, and the transport owns the one signal it plays: asking for
/// another replaces this one, and dropping the transport stops it.
pub fn drive_signal(transport: &FakeTransport, period: Duration) {
    let device = Arc::clone(&transport.device);
    let current = Arc::clone(&transport.current);
    let task = tokio::spawn(async move {
        let signal = Signal::new(1);
        let started = tokio::time::Instant::now();
        let mut ticks = tokio::time::interval(period);
        loop {
            ticks.tick().await;
            let elapsed = started.elapsed();
            let frame = {
                let mut device = device.lock().expect("device mutex");
                device.signal_frame(&|channel| signal.at(elapsed, channel))
            };
            let Some(frame) = frame else { return };
            // With no link there is nobody to tell, and a send fails only once the link's
            // own receiver has gone, which leaves nobody to tell either.
            let tx = current
                .lock()
                .expect("current link mutex")
                .as_ref()
                .map(|current| current.tx.clone());
            if let Some(tx) = tx {
                let _ = tx.send(frame);
            }
        }
    });
    if let Some(playing) = transport.signal.lock().expect("signal mutex").replace(task) {
        playing.abort();
    }
}

#[async_trait]
impl Transport for FakeTransport {
    async fn scan(&self, _window: Duration) -> Result<Advertisements, BleError> {
        if self.air.quiet.load(Ordering::SeqCst) || self.air.gone.load(Ordering::SeqCst) {
            return Ok(stream::empty().boxed());
        }
        let advert = self.advertisement();
        let s: BoxStream<'static, Advertisement> = stream::once(async move { advert }).boxed();
        Ok(s)
    }

    /// The fake device is the only device there is, and the transport draws no line between
    /// what it can see now and what it holds, so this answers with the same advertisement
    /// `scan` does, and with it when the device is quiet too.
    async fn held(&self) -> Result<Advertisements, BleError> {
        if self.air.gone.load(Ordering::SeqCst) {
            return Ok(stream::empty().boxed());
        }
        let advert = self.advertisement();
        let s: BoxStream<'static, Advertisement> = stream::once(async move { advert }).boxed();
        Ok(s)
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        if unit != self.unit || self.air.gone.load(Ordering::SeqCst) {
            return Err(BleError::NotFound);
        }
        let token = self.air.connects.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::unbounded_channel();
        // Installing the fresh link drops whatever the slot held before, which ends the
        // notification stream of any link this one is replacing.
        *self.current.lock().expect("current link mutex") = Some(Current { token, tx });
        if let Some(after) = self.air.sever_after.lock().expect("sever mutex").take() {
            let current = Arc::clone(&self.current);
            let air = Arc::clone(&self.air);
            tokio::spawn(async move {
                tokio::time::sleep(after).await;
                let mut slot = current.lock().expect("current link mutex");
                if slot.as_ref().is_some_and(|c| c.token == token) {
                    *slot = None;
                    drop(slot);
                    air.link_ended();
                }
            });
        }
        Ok(Box::new(FakeLink {
            token,
            device: Arc::clone(&self.device),
            current: Arc::clone(&self.current),
            air: Arc::clone(&self.air),
            rx: Mutex::new(Some(rx)),
        }))
    }

    /// Nothing here outlives the call that asked for it, so there is nothing to wait for; the
    /// ask itself is counted, and [`FakeTransport::settles`] reports it.
    async fn settle(&self, _bound: Duration) {
        self.settles.fetch_add(1, Ordering::SeqCst);
    }

    /// Records the device, then counts as a settle of everything.
    async fn settle_for(&self, unit: UnitId, bound: Duration) {
        self.settled.lock().expect("settled mutex").push(unit);
        self.settle(bound).await;
    }
}

/// One connection to the fake device. Writes are handled synchronously and the resulting
/// notifications are pushed onto the notification channel.
struct FakeLink {
    /// Which `connect` opened this link: it acts on the device only while it is the one up.
    token: usize,
    device: Arc<Mutex<FakeDevice>>,
    current: Arc<Mutex<Option<Current>>>,
    air: Arc<Air>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<Vec<u8>>>>,
}

impl FakeLink {
    /// This link's sender while it is the link that is up; `None` once it was severed,
    /// released or replaced.
    fn sender(&self) -> Option<mpsc::UnboundedSender<Vec<u8>>> {
        self.current
            .lock()
            .expect("current link mutex")
            .as_ref()
            .filter(|current| current.token == self.token)
            .map(|current| current.tx.clone())
    }
}

#[async_trait]
impl Link for FakeLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        let Some(sender) = self.sender() else {
            return Err(BleError::Connection("link severed".into()));
        };
        let notifications = {
            let mut device = self.device.lock().expect("device mutex");
            device.handle(frame)
        };
        for note in notifications {
            // A send fails only once the notification receiver has been dropped, so there
            // is nobody left to tell and the result is ignored.
            let _ = sender.send(note);
        }
        Ok(())
    }

    fn notifications(&self) -> Notifications {
        match self.rx.lock().expect("rx mutex").take() {
            Some(rx) => UnboundedReceiverStream::new(rx).boxed(),
            None => stream::empty().boxed(),
        }
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.air.releases.fetch_add(1, Ordering::SeqCst);
        let mut current = self.current.lock().expect("current link mutex");
        // Only the link that is up holds the device: a replaced link's release leaves it be.
        if current.as_ref().is_some_and(|c| c.token == self.token) {
            *current = None;
            drop(current);
            self.air.link_ended();
            if self.air.quiet_after_release.load(Ordering::SeqCst) {
                self.air.quiet.store(true, Ordering::SeqCst);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Listening;
    use svolder_profiles::Registry;
    use svolder_proto::{
        FunctionId, Message, Passphrase, UnitId, Value, decode_notification, encode_command,
    };

    /// A listening that wants one unit, or none, and records what it hears.
    struct Wanting {
        unit: Option<UnitId>,
        heard: Mutex<Vec<UnitId>>,
        done: bool,
    }

    impl Listening for Wanting {
        fn wants(&self, unit: UnitId) -> bool {
            self.unit == Some(unit)
        }

        fn heard(&self, advert: &Advertisement) {
            self.heard.lock().unwrap().push(advert.unit_id());
        }

        fn done(&self) -> bool {
            self.done
        }
    }

    /// The advert carries the record a radio would deliver: company id `0xFFFF`, the device's
    /// 27 manufacturer bytes.
    #[test]
    fn the_fake_advert_carries_its_record() {
        let t = transport();
        let want = t.device().lock().unwrap().manufacturer_data().to_bytes();
        assert_eq!(
            t.advertisement().record,
            Some(ManufacturerRecord {
                company_id: 0xFFFF,
                bytes: want.to_vec()
            })
        );
    }

    /// The record stays the manufacturer data's bytes when the advert says another code.
    #[test]
    fn the_fake_record_follows_an_advertised_code() {
        let t = transport();
        t.advertise(Advertised::Always(
            Passphrase::from_digits([9, 8, 7, 6]).unwrap(),
        ));
        let advert = t.advertisement();
        assert_ne!(advert.manufacturer.encrypted_passphrase, 0);
        let record = advert.record.expect("a record");
        assert_eq!(record.bytes, advert.manufacturer.to_bytes().to_vec());
    }

    /// A listen over the fake hands the device over when it is wanted, and only then.
    #[tokio::test]
    async fn a_listen_hands_over_the_device_only_when_it_is_wanted() {
        let t = transport();
        let wanting = Wanting {
            unit: Some(unit()),
            heard: Mutex::new(Vec::new()),
            done: false,
        };
        t.listen(Duration::from_millis(10), &wanting).await.unwrap();
        assert_eq!(*wanting.heard.lock().unwrap(), vec![unit()]);

        let stranger = Wanting {
            unit: Some("AABBCCDDEEFF0011".parse().unwrap()),
            heard: Mutex::new(Vec::new()),
            done: false,
        };
        t.listen(Duration::from_millis(10), &stranger)
            .await
            .unwrap();
        assert!(stranger.heard.lock().unwrap().is_empty());
    }

    /// A listen that has nobody left to listen for hands nothing over.
    #[tokio::test]
    async fn a_listen_that_is_done_hands_nothing_over() {
        let t = transport();
        let done = Wanting {
            unit: Some(unit()),
            heard: Mutex::new(Vec::new()),
            done: true,
        };
        t.listen(Duration::from_millis(10), &done).await.unwrap();
        assert!(done.heard.lock().unwrap().is_empty());
    }

    fn transport() -> FakeTransport {
        let reg = Registry::builtin().unwrap();
        let profile = reg.get("gungnir2-v2").unwrap().clone();
        let unit = "0102030405060708".parse::<UnitId>().unwrap();
        FakeTransport::from_profile(
            &profile,
            unit,
            3,
            Passphrase::from_digits([1, 2, 3, 4]).unwrap(),
        )
    }

    /// A transport over the fake amplifier, whose meters have the scale a signal needs.
    fn amp_transport() -> FakeTransport {
        let reg = Registry::builtin().unwrap();
        let profile = reg.get("fake-amp").unwrap().clone();
        let unit = "0102030405060708".parse::<UnitId>().unwrap();
        FakeTransport::from_profile(&profile, unit, 3, Passphrase::NONE)
    }

    fn unit() -> UnitId {
        "0102030405060708".parse().unwrap()
    }

    /// A transport that counts its scans and hears nothing.
    struct CountingScans(AtomicUsize);

    #[async_trait::async_trait]
    impl Transport for CountingScans {
        async fn scan(&self, _window: Duration) -> Result<Advertisements, BleError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(stream::empty().boxed())
        }

        async fn connect(&self, _unit: UnitId) -> Result<Box<dyn Link>, BleError> {
            Err(BleError::NotFound)
        }
    }

    /// A transport whose scan lists the amplifier twice.
    struct Repeating(FakeTransport);

    #[async_trait::async_trait]
    impl Transport for Repeating {
        async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
            let once: Vec<Advertisement> = self.0.scan(window).await?.collect().await;
            Ok(stream::iter(once.iter().chain(once.iter()).cloned().collect::<Vec<_>>()).boxed())
        }

        async fn connect(&self, _unit: UnitId) -> Result<Box<dyn Link>, BleError> {
            Err(BleError::NotFound)
        }
    }

    /// A device a scan lists twice is handed over once.
    #[tokio::test]
    async fn a_listen_hands_a_device_over_once() {
        let t = Repeating(transport());
        let wanting = Wanting {
            unit: Some(unit()),
            heard: Mutex::new(Vec::new()),
            done: false,
        };
        t.listen(Duration::from_millis(10), &wanting).await.unwrap();
        assert_eq!(*wanting.heard.lock().unwrap(), vec![unit()]);
    }

    /// A listen with nobody to listen for does not scan at all.
    #[tokio::test]
    async fn a_listen_that_is_done_before_it_starts_does_not_scan() {
        let t = CountingScans(AtomicUsize::new(0));
        let done = Wanting {
            unit: None,
            heard: Mutex::new(Vec::new()),
            done: true,
        };
        t.listen(Duration::from_secs(5), &done).await.unwrap();
        assert_eq!(t.0.load(Ordering::SeqCst), 0);
    }

    /// The next notification on a link, or a failure naming how long it waited.
    async fn next_frame(notes: &mut Notifications) -> Vec<u8> {
        tokio::time::timeout(Duration::from_secs(5), notes.next())
            .await
            .expect("a notification within five seconds")
            .expect("the notification stream is still open")
    }

    /// The signal plays on the link: every reading is a meter status, and the pushed frames
    /// carry a reading for each of the two meters.
    #[tokio::test]
    async fn the_signal_pushes_meter_readings_onto_the_link() {
        let t = amp_transport();
        let link = t.connect(unit()).await.unwrap();
        let mut notes = link.notifications();
        drive_signal(&t, Duration::from_millis(10));
        for _ in 0..3 {
            let n = decode_notification(&next_frame(&mut notes).await).unwrap();
            let ids: Vec<FunctionId> = n.messages.iter().map(|m| m.id).collect();
            assert_eq!(ids, vec![FunctionId::two(20, 1), FunctionId::two(20, 2)]);
            for m in &n.messages {
                let Value::U16(raw) = m.value else {
                    panic!("a meter reads a u16, got {:?}", m.value)
                };
                assert!((160..=350).contains(&raw), "{raw} is on the meter's scale");
            }
        }
    }

    /// A device whose meters have no scale has nothing to play, so a signal asked for on one
    /// sends nothing rather than pushing empty frames at whatever is listening.
    #[tokio::test]
    async fn a_dac_plays_no_signal() {
        let t = transport();
        let link = t.connect(unit()).await.unwrap();
        let mut notes = link.notifications();
        drive_signal(&t, Duration::from_millis(10));
        let quiet = tokio::time::timeout(Duration::from_millis(300), notes.next()).await;
        assert!(quiet.is_err(), "nothing should have arrived, got {quiet:?}");
    }

    /// The transport owns the signal: once it is gone the pushing stops, so a fake a session
    /// has let go of does not go on notifying the link it left behind.
    #[tokio::test]
    async fn dropping_the_transport_stops_the_signal() {
        let t = amp_transport();
        let link = t.connect(unit()).await.unwrap();
        let mut notes = link.notifications();
        drive_signal(&t, Duration::from_millis(5));
        // One frame first, so the signal is proven to be playing before it is taken away.
        let _ = next_frame(&mut notes).await;
        drop(t);
        // What the channel already held drains at once; a signal still playing would keep
        // refilling it and the drain would never come up empty.
        for drained in 0..200 {
            match tokio::time::timeout(Duration::from_millis(300), notes.next()).await {
                Err(_) | Ok(None) => return,
                Ok(Some(_)) => assert!(
                    drained < 199,
                    "the signal was still playing after the transport was dropped"
                ),
            }
        }
    }

    /// The transport owns one signal at a time: asking for a second while the first is still
    /// running stops the first task rather than leaking it, so only the second goes on
    /// notifying the link.
    #[tokio::test]
    async fn starting_a_second_signal_stops_the_first() {
        let t = amp_transport();
        let _link = t.connect(unit()).await.unwrap();
        drive_signal(&t, Duration::from_millis(10));
        let first = t
            .signal
            .lock()
            .expect("signal mutex")
            .as_ref()
            .expect("the first signal is running")
            .abort_handle();
        assert!(!first.is_finished(), "the first signal had already ended");

        drive_signal(&t, Duration::from_millis(10));
        tokio::task::yield_now().await;
        assert!(
            first.is_finished(),
            "starting a second signal left the first one running"
        );
    }

    #[tokio::test]
    async fn scan_yields_the_device() {
        let t = transport();
        let mut adverts = t.scan(Duration::from_millis(10)).await.unwrap();
        let a = adverts.next().await.expect("one advertisement");
        assert_eq!(a.product_name(), Some("Gungnir 2"));
        assert_eq!(a.unit_id().to_string(), "0102030405060708");
    }

    #[tokio::test]
    async fn held_yields_the_device() {
        let t = transport();
        let mut adverts = t.held().await.unwrap();
        let a = adverts.next().await.expect("one advertisement");
        assert_eq!(a.unit_id().to_string(), "0102030405060708");
    }

    #[tokio::test]
    async fn connect_write_refresh_yields_statuses() {
        let t = transport();
        let unit = "0102030405060708".parse::<UnitId>().unwrap();
        let link = t.connect(unit).await.unwrap();
        let mut notes = link.notifications();
        let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        let refresh = encode_command(
            &Message::command(FunctionId::one(1), Value::Data(vec![])),
            pass,
        )
        .unwrap();
        link.write(&refresh).await.unwrap();
        let bytes = notes.next().await.expect("a notification");
        let n = decode_notification(&bytes).unwrap();
        assert!(
            n.messages.iter().any(|m| m.id == FunctionId::one(10)),
            "volume status present"
        );
    }

    #[tokio::test]
    async fn connect_to_a_stranger_is_not_found() {
        let t = transport();
        let other = "AABBCCDDEEFF0011".parse::<UnitId>().unwrap();
        assert!(matches!(t.connect(other).await, Err(BleError::NotFound)));
    }

    /// An armed sever cuts the next link once its delay has passed, and that link only.
    #[tokio::test(start_paused = true)]
    async fn an_armed_sever_cuts_the_next_link_once_after_its_delay() {
        let t = transport();
        t.sever_next_link_after(Duration::from_millis(500));
        let first = t.connect(unit()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(499)).await;
        first
            .write(&refresh())
            .await
            .expect("the link is up until its delay has passed");
        tokio::time::sleep(Duration::from_millis(2)).await;
        assert!(
            first.write(&refresh()).await.is_err(),
            "the link outlived its delay"
        );
        let second = t.connect(unit()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
        second
            .write(&refresh())
            .await
            .expect("the sever was armed for one link only");
    }

    /// A device gone after its time has its link cut, and is neither heard nor reached again.
    #[tokio::test(start_paused = true)]
    async fn a_device_gone_after_its_time_is_out_of_reach() {
        let t = transport();
        let unit: UnitId = "0102030405060708".parse().unwrap();
        let _link = t.connect(unit).await.unwrap();
        t.go_away_after(Duration::from_millis(100));
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(t.current.lock().unwrap().is_none(), "the link was not cut");
        assert!(
            t.scan(Duration::from_millis(10))
                .await
                .unwrap()
                .next()
                .await
                .is_none()
        );
        assert!(t.held().await.unwrap().next().await.is_none());
        assert!(t.connect(unit).await.is_err());
    }

    /// A device told to go when a file appears stays on the air until the file is there, then is
    /// cut and out of reach.
    #[tokio::test(start_paused = true)]
    async fn a_device_gone_when_a_file_appears_stays_until_then() {
        let dir = std::env::temp_dir().join(format!("svolder-fake-gone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("drop");
        let _ = std::fs::remove_file(&file);
        let t = transport();
        let unit: UnitId = "0102030405060708".parse().unwrap();
        let _link = t.connect(unit).await.unwrap();
        t.go_away_when_file(file.clone());
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            t.current.lock().unwrap().is_some(),
            "the link was cut early"
        );
        std::fs::write(&file, b"").unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(t.current.lock().unwrap().is_none(), "the link was not cut");
        assert!(t.connect(unit).await.is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A knob that does not parse is refused in words that name its variable, and an unset one
    /// takes its default.
    #[test]
    fn a_bad_knob_names_its_variable() {
        let reg = Registry::builtin().unwrap();
        for (name, value) in [
            ("SVOLDER_FAKE_UNIT", "xyz"),
            ("SVOLDER_FAKE_FIRMWARE", "many"),
            ("SVOLDER_FAKE_PASSPHRASE", "12"),
            ("SVOLDER_FAKE_SEVER_AFTER_MS", "soon"),
            ("SVOLDER_FAKE_GONE_AFTER_MS", "soon"),
        ] {
            let err = FakeTransport::from_knobs(&reg, |n| (n == name).then(|| value.to_string()))
                .err()
                .unwrap_or_else(|| panic!("{name}={value} was accepted"));
            assert!(err.contains(name), "got {err}");
        }
        let err = FakeTransport::from_knobs(&reg, |n| {
            (n == "SVOLDER_FAKE_PROFILE").then(|| "no-such".to_string())
        })
        .err()
        .expect("an unknown profile is refused");
        assert!(err.contains("no-such"), "got {err}");
        let defaults = FakeTransport::from_knobs(&reg, |_| None).expect("the defaults build");
        assert_eq!(defaults.unit, unit());
    }

    /// A fake Loki Max from the knobs `set` names, with no passphrase.
    fn loki_with(set: &[(&str, &str)]) -> Result<FakeTransport, String> {
        let reg = Registry::builtin().unwrap();
        FakeTransport::from_knobs(&reg, |n| {
            match n {
                "SVOLDER_FAKE_PROFILE" => Some("lokimax"),
                "SVOLDER_FAKE_PASSPHRASE" => Some("none"),
                _ => set.iter().find(|(name, _)| *name == n).map(|(_, v)| *v),
            }
            .map(str::to_string)
        })
    }

    /// What the fake Loki Max in `t` answers to `command`, decoded.
    fn answer(t: &FakeTransport, command: Message) -> Vec<svolder_proto::Notification> {
        let frame = encode_command(&command, Passphrase::NONE).unwrap();
        t.device()
            .lock()
            .unwrap()
            .handle(&frame)
            .iter()
            .map(|n| decode_notification(n).unwrap())
            .collect()
    }

    /// `SVOLDER_FAKE_LOKI_REPORTS` is `full` (the default) for a band reported on its own three-id
    /// form or `short` for its two-id form, and anything else is refused in words that name the
    /// variable and both values.
    #[test]
    fn the_loki_reports_knob_takes_full_or_short_and_refuses_the_rest() {
        let band_write = || Message::command(FunctionId::three(6, 1, 3), Value::U8(20));
        for set in [&[][..], &[("SVOLDER_FAKE_LOKI_REPORTS", "full")][..]] {
            let t = loki_with(set).unwrap();
            assert_eq!(
                answer(&t, band_write())[0].messages[0].id,
                FunctionId::three(6, 1, 3)
            );
        }
        let t = loki_with(&[("SVOLDER_FAKE_LOKI_REPORTS", "short")]).unwrap();
        assert_eq!(
            answer(&t, band_write())[0].messages[0].id,
            FunctionId::two(6, 1)
        );
        let err = loki_with(&[("SVOLDER_FAKE_LOKI_REPORTS", "long")])
            .err()
            .expect("another value is refused");
        for word in ["SVOLDER_FAKE_LOKI_REPORTS", "full", "short"] {
            assert!(err.contains(word), "{word} missing from {err}");
        }
    }

    /// `SVOLDER_FAKE_SILENT_RECALL=1` makes the fake set the bands on a recall without reporting
    /// them; unset, a recall reports what it changed.
    #[test]
    fn the_silent_recall_knob_reaches_the_fake() {
        let recall = || Message::command(FunctionId::three(7, 2, 1), Value::Data(Vec::new()));
        for (set, reports) in [
            (&[][..], true),
            (&[("SVOLDER_FAKE_SILENT_RECALL", "1")][..], false),
        ] {
            let t = loki_with(set).unwrap();
            t.device()
                .lock()
                .unwrap()
                .set_raw(FunctionId::three(6, 1, 3), Value::U8(30));
            assert_eq!(!answer(&t, recall()).is_empty(), reports, "{set:?}");
        }
    }

    /// `SVOLDER_FAKE_QUIET_SAME=1` makes a repeated Command of any control go unreported; unset,
    /// only the tone-control mode is silent on a repeat.
    #[test]
    fn the_quiet_same_knob_reaches_the_fake() {
        for (set, reports) in [
            (&[][..], true),
            (&[("SVOLDER_FAKE_QUIET_SAME", "1")][..], false),
        ] {
            let t = loki_with(set).unwrap();
            let band = || Message::command(FunctionId::three(6, 1, 3), Value::U8(30));
            assert!(!answer(&t, band()).is_empty(), "a change is reported");
            assert_eq!(!answer(&t, band()).is_empty(), reports, "{set:?}");
        }
    }

    /// A refresh frame under the fake's code.
    fn refresh() -> Vec<u8> {
        encode_command(
            &Message::command(FunctionId::one(1), Value::Data(vec![])),
            Passphrase::from_digits([1, 2, 3, 4]).unwrap(),
        )
        .unwrap()
    }

    /// A link `connect` has replaced is gone: it writes nothing onto the link that replaced it,
    /// and its release leaves that link up.
    #[tokio::test]
    async fn a_replaced_link_neither_writes_on_nor_releases_its_successor() {
        let t = transport();
        let old = t.connect(unit()).await.unwrap();
        let new = t.connect(unit()).await.unwrap();
        let mut notes = new.notifications();
        assert!(
            old.write(&refresh()).await.is_err(),
            "the replaced link wrote on the one that replaced it"
        );
        old.disconnect().await.unwrap();
        new.write(&refresh())
            .await
            .expect("the live link still writes after the old one's release");
        let n = decode_notification(&next_frame(&mut notes).await).unwrap();
        assert!(n.messages.iter().any(|m| m.id == FunctionId::one(10)));
        assert_eq!(
            t.releases(),
            1,
            "a release is counted whichever link it was"
        );
    }

    /// Disconnecting clears the shared sender slot, so the notification stream the link
    /// handed out ends rather than waiting for something that will never arrive.
    #[tokio::test]
    async fn disconnecting_ends_the_notification_stream() {
        let t = transport();
        let link = t.connect(unit()).await.unwrap();
        let mut notes = link.notifications();
        link.disconnect().await.unwrap();
        let ended = tokio::time::timeout(Duration::from_millis(300), notes.next()).await;
        assert!(matches!(ended, Ok(None)), "got {ended:?}");
    }

    /// The receiver is handed out once; a second call finds nothing left to take and comes
    /// back with a stream that is already over.
    #[tokio::test]
    async fn a_second_call_to_notifications_is_already_ended() {
        let t = transport();
        let link = t.connect(unit()).await.unwrap();
        let _first = link.notifications();
        let mut second = link.notifications();
        assert!(second.next().await.is_none());
    }

    /// Nobody is left to read a write's notifications once the receiver has been dropped, but
    /// the write itself still succeeds: a send failing is not a write failing.
    #[tokio::test]
    async fn a_write_after_the_receiver_is_dropped_still_succeeds() {
        let t = transport();
        let link = t.connect(unit()).await.unwrap();
        drop(link.notifications());
        let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        let refresh = encode_command(
            &Message::command(FunctionId::one(1), Value::Data(vec![])),
            pass,
        )
        .unwrap();
        assert!(link.write(&refresh).await.is_ok());
    }
}
