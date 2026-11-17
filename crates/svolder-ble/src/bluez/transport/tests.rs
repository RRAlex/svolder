// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::unit;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use svolder_proto::ADVERT_COMPANY_ID;

/// A device a scan has just heard is the one a connect takes, once: the connect after it
/// looks for the device again.
#[test]
fn a_fresh_sighting_is_taken_once_by_the_connect_after_it() {
    let (amp, dac) = (unit("0102030405060708"), unit("0807060504030201"));
    let heard = Instant::now();
    let mut sightings = Sightings::new();
    sightings.record(amp, "amp", heard);
    sightings.record(dac, "dac", heard);

    let soon = heard + Duration::from_secs(2);
    assert_eq!(sightings.take(amp, soon), Some("amp"));
    assert_eq!(
        sightings.take(amp, soon),
        None,
        "a sighting was taken twice"
    );
    assert_eq!(
        sightings.take(dac, soon),
        Some("dac"),
        "taking one device's sighting spent another's"
    );
}

/// A sighting too old to trust is no sighting: the connect looks for the device instead,
/// and a stale one is dropped whoever it was for.
#[test]
fn a_stale_sighting_is_not_taken() {
    let (amp, dac) = (unit("0102030405060708"), unit("0807060504030201"));
    let heard = Instant::now();
    let mut sightings = Sightings::new();
    sightings.record(amp, "amp", heard);
    sightings.record(dac, "dac", heard);

    let late = heard + SIGHTING_FRESH + Duration::from_millis(1);
    assert_eq!(sightings.take(amp, late), None);
    assert!(sightings.heard.is_empty(), "a stale sighting was kept");

    // A newer sighting replaces an older one of the same device.
    sightings.record(amp, "old", heard);
    sightings.record(amp, "new", late);
    assert_eq!(sightings.take(amp, late), Some("new"));
}

#[test]
fn adapter_name_matches_leading_token() {
    assert!(adapter_info_matches("hci1 (usb:v1D6Bp0246)", "hci1"));
    assert!(!adapter_info_matches("hci10 (usb:...)", "hci1"));
    assert!(!adapter_info_matches("hci0", "hci1"));
    assert!(adapter_info_matches("hci0", "hci0"));
}

/// A name no controller answers to says which ones there were, so the user can name one
/// of those instead; with none at all it says that rather than listing nothing.
#[test]
fn a_missing_adapter_names_the_adapters_there_are() {
    assert_eq!(
        no_adapter_named("hci9", &["hci0".to_string(), "hci1".to_string()]),
        "no adapter named hci9; adapters: hci0, hci1"
    );
    assert_eq!(
        no_adapter_named("hci9", &[]),
        "no adapter named hci9; no adapters present"
    );
}

#[test]
fn tracing_is_off_unless_the_variable_asks_for_it() {
    assert!(!trace_enabled(None));
    assert!(!trace_enabled(Some(OsString::from(""))));
    assert!(!trace_enabled(Some(OsString::from("0"))));
    assert!(trace_enabled(Some(OsString::from("1"))));
    assert!(trace_enabled(Some(OsString::from("yes"))));
}

/// Discovery whose start reaches the adapter the moment it is asked and answers a second later,
/// and whose stop answers a second after it is asked: `on` is whether the adapter is
/// discovering.
#[derive(Clone, Default)]
struct SlowDiscovery {
    on: Arc<AtomicBool>,
}

impl SlowDiscovery {
    fn on(&self) -> bool {
        self.on.load(Ordering::SeqCst)
    }
}

impl Discovery for SlowDiscovery {
    fn start(&self) -> BoxFuture<'static, Result<(), BleError>> {
        let on = Arc::clone(&self.on);
        async move {
            on.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        }
        .boxed()
    }

    fn stop(&self) -> BoxFuture<'static, ()> {
        let on = Arc::clone(&self.on);
        async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            on.store(false, Ordering::SeqCst);
        }
        .boxed()
    }
}

/// A caller that leaves while BlueZ is still answering its start leaves no discovery on: the
/// start that landed is stopped, and the scan lock comes free only once it has been.
#[tokio::test(start_paused = true)]
async fn a_start_its_caller_left_is_stopped_before_the_lock_comes_free() {
    let discovery = SlowDiscovery::default();
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    let scanning = Arc::clone(&lock).lock_owned().await;
    let start = ScanGuard::start(&discovery, scanning, false);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), start)
            .await
            .is_err(),
        "the caller was meant to leave before the start answered"
    );
    assert!(discovery.on(), "the start reached the adapter");
    let _next = Arc::clone(&lock).lock_owned().await;
    assert!(
        !discovery.on(),
        "the next scan found the adapter still discovering"
    );
}

/// A caller that waits gets the scan, and its stop has turned discovery off before the lock
/// comes free.
#[tokio::test(start_paused = true)]
async fn a_scan_its_caller_stops_is_off_before_the_lock_comes_free() {
    let discovery = SlowDiscovery::default();
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    let scanning = Arc::clone(&lock).lock_owned().await;
    let scan = ScanGuard::start(&discovery, scanning, false)
        .await
        .expect("the scan starts");
    assert!(discovery.on());
    scan.stop().await;
    assert!(!discovery.on());
    assert!(
        Arc::clone(&lock).try_lock_owned().is_ok(),
        "the lock outlived the stop"
    );
}

/// A caller that leaves after the start has answered, but before taking the scan, leaves
/// no discovery on either.
#[tokio::test(start_paused = true)]
async fn a_start_answered_but_never_taken_is_stopped_before_the_lock_comes_free() {
    let discovery = SlowDiscovery::default();
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    let scanning = Arc::clone(&lock).lock_owned().await;
    let mut start = Box::pin(ScanGuard::start(&discovery, scanning, false));
    assert!(futures::poll!(&mut start).is_pending());
    tokio::time::sleep(Duration::from_secs(2)).await; // answered at 1 s, never read
    drop(start);
    let _next = Arc::clone(&lock).lock_owned().await;
    assert!(
        !discovery.on(),
        "the next scan found the adapter still discovering"
    );
}

/// Discovery whose first `fail` starts fail at once; it counts the starts and stops asked.
#[derive(Clone, Default)]
struct CountingDiscovery {
    fail: usize,
    starts: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
}

impl Discovery for CountingDiscovery {
    fn start(&self) -> BoxFuture<'static, Result<(), BleError>> {
        let n = self.starts.fetch_add(1, Ordering::SeqCst);
        let fail = n < self.fail;
        async move {
            if fail {
                Err(BleError::Connection("refused".into()))
            } else {
                Ok(())
            }
        }
        .boxed()
    }

    fn stop(&self) -> BoxFuture<'static, ()> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        async {}.boxed()
    }
}

/// A start that fails both tries gives its caller the error, asks for no stop after the
/// last try, and leaves the lock free at once.
#[tokio::test(start_paused = true)]
async fn a_start_that_fails_is_an_error_and_frees_the_lock() {
    let discovery = CountingDiscovery {
        fail: usize::MAX,
        ..Default::default()
    };
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    let scanning = Arc::clone(&lock).lock_owned().await;
    let result = ScanGuard::start(&discovery, scanning, false).await;
    assert!(result.is_err());
    assert_eq!(discovery.starts.load(Ordering::SeqCst), 2);
    assert_eq!(
        discovery.stops.load(Ordering::SeqCst),
        1,
        "one stop, between the tries"
    );
    tokio::task::yield_now().await;
    assert!(Arc::clone(&lock).try_lock_owned().is_ok());
}

/// A first start that fails is stopped and tried once more; the caller gets the scan.
#[tokio::test(start_paused = true)]
async fn a_failed_first_start_is_stopped_and_retried_once() {
    let discovery = CountingDiscovery {
        fail: 1,
        ..Default::default()
    };
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    let scanning = Arc::clone(&lock).lock_owned().await;
    let scan = ScanGuard::start(&discovery, scanning, false)
        .await
        .expect("the retry starts the scan");
    assert_eq!(discovery.starts.load(Ordering::SeqCst), 2);
    assert_eq!(discovery.stops.load(Ordering::SeqCst), 1);
    drop(scan);
}

/// A scan starts without a service filter: BlueZ 5.87 crashes when one matches a device.
#[test]
fn a_scan_starts_without_a_service_filter() {
    assert!(scan_filter().services.is_empty());
}

fn data(id: u16, bytes: &[u8]) -> HashMap<u16, Vec<u8>> {
    HashMap::from([(id, bytes.to_vec())])
}

/// A device whose manufacturer data does not parse is set aside at once and costs no read on
/// its later events; data that parses brings it back.
#[test]
fn a_foreign_device_costs_no_read_after_its_first_event() {
    let mut set_aside = HashSet::new();
    let foreign = data(0x004C, &[1, 2, 3]);
    assert!(!worth_a_read(&mut set_aside, &"tv", Some(&foreign)));
    assert!(!worth_a_read(&mut set_aside, &"tv", None), "an rssi event");
    assert!(
        worth_a_read(&mut set_aside, &"amp", None),
        "a device not yet judged is read"
    );
    let record = crate::test_support::hex("02000000000001020304050607088a53f037030001070000000000");
    let parsed = data(ADVERT_COMPANY_ID, &record);
    assert!(worth_a_read(&mut set_aside, &"tv", Some(&parsed)));
    assert!(worth_a_read(&mut set_aside, &"tv", None));
}

/// A read that finds no Forkbeard data sets the device aside; one whose data parses does not (its
/// UUIDs may not be filled in yet).
#[test]
fn a_read_with_no_forkbeard_data_sets_the_device_aside() {
    let mut set_aside = HashSet::new();
    let props = PeripheralProperties::default();
    set_aside_after_read(&mut set_aside, &"bare", &props);
    assert!(!worth_a_read(&mut set_aside, &"bare", None));
}

/// A read of a device whose manufacturer data parses but whose services are not filled in yet
/// leaves it in: the UUIDs arrive later, and its next event is read again.
#[test]
fn a_read_with_forkbeard_data_but_no_services_keeps_the_device_in() {
    let mut set_aside = HashSet::new();
    let props = PeripheralProperties {
        manufacturer_data: data(
            ADVERT_COMPANY_ID,
            &crate::test_support::hex("02000000000001020304050607088a53f037030001070000000000"),
        ),
        ..Default::default()
    };
    set_aside_after_read(&mut set_aside, &"amp", &props);
    assert!(worth_a_read(&mut set_aside, &"amp", None));
}
