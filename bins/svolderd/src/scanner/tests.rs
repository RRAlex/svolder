// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use svolder_profiles::Registry;

use super::*;

const AMP: &str = "0102030405060708";
const DAC: &str = "0807060504030201";
/// How often a device on the air advertises.
const EVERY: Duration = Duration::from_millis(100);
/// How long a scan takes to stop once a listen is over, as an adapter's does.
const STOP: Duration = Duration::from_millis(50);

fn unit(text: &str) -> UnitId {
    text.parse().expect("a valid unit id")
}

/// The advertisement a fake gungnir 2 with this unit id sends.
fn advert(text: &str) -> Advertisement {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    svolder_ble::FakeTransport::from_profile(
        &profile,
        unit(text),
        3,
        Passphrase::from_text("1234").expect("four digits"),
    )
    .advertisement()
}

/// Devices on the air from moments of their own, under listens that hear each wanted one at
/// its next advertisement: what an adapter that reports devices one at a time does.
struct OnAir {
    /// Each device's advertisement, and when it comes on the air (`None`: never).
    devices: Vec<(Advertisement, Option<Instant>)>,
    /// When each listen began and, once it has, ended.
    listens: Mutex<Vec<(Instant, Option<Instant>)>>,
    /// Whether a listen is running, its scan not yet stopped.
    listening: AtomicBool,
    /// Whether every listen fails.
    fails: AtomicBool,
}

impl OnAir {
    /// The devices `on`, each on the air from that long after now, or never.
    fn new(on: &[(&str, Option<Duration>)]) -> Arc<OnAir> {
        let now = Instant::now();
        Arc::new(OnAir {
            devices: on
                .iter()
                .map(|(text, after)| (advert(text), after.map(|after| now + after)))
                .collect(),
            listens: Mutex::new(Vec::new()),
            listening: AtomicBool::new(false),
            fails: AtomicBool::new(false),
        })
    }

    fn listens(&self) -> Vec<(Instant, Option<Instant>)> {
        self.listens.lock().unwrap().clone()
    }

    fn is_listening(&self) -> bool {
        self.listening.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Transport for OnAir {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        tokio::time::sleep(window).await;
        let now = Instant::now();
        let on: Vec<Advertisement> = self
            .devices
            .iter()
            .filter(|(_, from)| from.is_some_and(|from| from <= now))
            .map(|(advert, _)| advert.clone())
            .collect();
        Ok(futures::stream::iter(on).boxed())
    }

    async fn listen(&self, window: Duration, listening: &dyn Listening) -> Result<(), BleError> {
        let began = Instant::now();
        self.listens.lock().unwrap().push((began, None));
        if self.fails.load(Ordering::SeqCst) {
            return Err(BleError::Adapter("the adapter is off".into()));
        }
        self.listening.store(true, Ordering::SeqCst);
        let deadline = began + window;
        loop {
            let now = Instant::now();
            for (advert, from) in &self.devices {
                if from.is_some_and(|from| from <= now) && listening.wants(advert.unit_id()) {
                    listening.heard(advert);
                }
            }
            if listening.done() || now >= deadline {
                break;
            }
            tokio::time::sleep_until((now + EVERY).min(deadline)).await;
        }
        tokio::time::sleep(STOP).await;
        self.listening.store(false, Ordering::SeqCst);
        if let Some(last) = self.listens.lock().unwrap().last_mut() {
            last.1 = Some(Instant::now());
        }
        Ok(())
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        Err(BleError::NotFound)
    }
}

fn scanner_on(air: &Arc<OnAir>) -> Scanner {
    Scanner::new(Arc::clone(air) as Arc<dyn Transport>)
}

/// Devices looked for together share one listen, and each hears nothing at its end.
#[tokio::test(start_paused = true)]
async fn devices_looked_for_together_share_one_listen() {
    let air = OnAir::new(&[(AMP, None), (DAC, None)]);
    let scanner = scanner_on(&air);
    let started = Instant::now();
    let (amp, dac) = tokio::join!(scanner.sight(unit(AMP)), scanner.sight(unit(DAC)));
    assert!(
        matches!(amp, Ok(None)) && matches!(dac, Ok(None)),
        "{amp:?} {dac:?}"
    );
    assert_eq!(started.elapsed(), SCAN_WINDOW + STOP);
    assert_eq!(air.listens().len(), 1);
}

/// A device asked for just after a listen began counts as having had it.
#[tokio::test(start_paused = true)]
async fn a_device_asked_for_just_after_a_listen_began_shares_it() {
    let air = OnAir::new(&[(AMP, None), (DAC, None)]);
    let scanner = scanner_on(&air);
    let (amp, dac) = tokio::join!(scanner.sight(unit(AMP)), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        scanner.sight(unit(DAC)).await
    });
    assert!(
        matches!(amp, Ok(None)) && matches!(dac, Ok(None)),
        "{amp:?} {dac:?}"
    );
    assert_eq!(air.listens().len(), 1);
}

/// A device asked for while a listen runs is heard by that listen.
#[tokio::test(start_paused = true)]
async fn a_device_asked_for_mid_listen_is_heard_by_it() {
    let air = OnAir::new(&[(AMP, None), (DAC, Some(Duration::ZERO))]);
    let scanner = scanner_on(&air);
    let started = Instant::now();
    let (_, dac) = tokio::join!(scanner.sight(unit(AMP)), async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let heard = scanner.sight(unit(DAC)).await;
        (heard, started.elapsed())
    });
    let (heard, at) = dac;
    assert_eq!(
        heard
            .expect("a listen")
            .expect("the dac was heard")
            .unit_id(),
        unit(DAC)
    );
    assert!(at < Duration::from_millis(1_500), "the dac waited {at:?}");
    assert_eq!(air.listens().len(), 1);
}

/// A device asked for late into a listen and not heard is not answered at its end: the next
/// listen, which it has whole, answers it.
#[tokio::test(start_paused = true)]
async fn a_device_asked_for_late_and_not_heard_gets_a_listen_of_its_own() {
    let air = OnAir::new(&[(AMP, None), (DAC, None)]);
    let scanner = scanner_on(&air);
    let started = Instant::now();
    let (amp, dac) = tokio::join!(
        async {
            let heard = scanner.sight(unit(AMP)).await;
            (heard, started.elapsed())
        },
        async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let heard = scanner.sight(unit(DAC)).await;
            (heard, started.elapsed())
        }
    );
    assert!(matches!(amp.0, Ok(None)));
    assert_eq!(amp.1, SCAN_WINDOW + STOP);
    assert!(matches!(dac.0, Ok(None)));
    assert_eq!(dac.1, (SCAN_WINDOW + STOP) * 2);
    assert_eq!(air.listens().len(), 2);
}

/// A device heard while another is still looked for is answered at once, the listen going on.
#[tokio::test(start_paused = true)]
async fn a_device_heard_early_is_answered_while_the_listen_goes_on() {
    let air = OnAir::new(&[(AMP, None), (DAC, Some(Duration::ZERO))]);
    let scanner = scanner_on(&air);
    let (amp, dac) = tokio::join!(scanner.sight(unit(AMP)), async {
        let heard = scanner.sight(unit(DAC)).await;
        (heard, air.is_listening())
    });
    assert!(matches!(dac.0, Ok(Some(_))), "{:?}", dac.0);
    assert!(dac.1, "the dac's answer waited for the listen to stop");
    assert!(matches!(amp, Ok(None)));
}

/// The answer that leaves nobody waiting goes out once the scan has stopped, so the connect it
/// lets go of runs on an adapter that is no longer scanning.
#[tokio::test(start_paused = true)]
async fn the_last_device_heard_is_answered_once_the_listen_has_stopped() {
    let air = OnAir::new(&[(DAC, Some(Duration::ZERO))]);
    let scanner = scanner_on(&air);
    let heard = scanner.sight(unit(DAC)).await;
    assert!(matches!(heard, Ok(Some(_))), "{heard:?}");
    assert!(
        !air.is_listening(),
        "answered while the scan was still running"
    );
}

/// A caller that stops waiting is listened for no longer, and a listen left with nobody stops.
#[tokio::test(start_paused = true)]
async fn a_caller_that_stops_waiting_ends_a_listen_for_nobody() {
    let air = OnAir::new(&[(AMP, None)]);
    let scanner = scanner_on(&air);
    let gave_up = tokio::time::timeout(Duration::from_secs(1), scanner.sight(unit(AMP))).await;
    assert!(gave_up.is_err());
    tokio::time::sleep(Duration::from_secs(1)).await;
    let listens = air.listens();
    assert_eq!(listens.len(), 1);
    let (began, ended) = listens[0];
    let ended = ended.expect("the listen ended");
    assert!(
        ended - began < Duration::from_millis(1_500),
        "the listen ran {:?} for nobody",
        ended - began
    );
}

/// A failed listen answers everyone it was for with the failure, and is not run again for
/// nobody.
#[tokio::test(start_paused = true)]
async fn a_failed_listen_answers_who_it_was_for_with_the_failure() {
    let air = OnAir::new(&[(AMP, None), (DAC, None)]);
    air.fails.store(true, Ordering::SeqCst);
    let scanner = scanner_on(&air);
    let (amp, dac) = tokio::join!(scanner.sight(unit(AMP)), scanner.sight(unit(DAC)));
    for answer in [amp, dac] {
        assert!(
            matches!(&answer, Err(m) if m.contains("the adapter is off")),
            "{answer:?}"
        );
    }
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(
        air.listens().len(),
        1,
        "a listen ran again with nobody waiting"
    );
}

/// The `scan` verb and a shared listen take turns on the radio.
#[tokio::test(start_paused = true)]
async fn the_scan_verb_and_a_listen_take_turns() {
    let air = OnAir::new(&[(AMP, None)]);
    let scanner = scanner_on(&air);
    let started = Instant::now();
    let (_scan, _amp) = tokio::join!(scanner.scan(Duration::from_secs(2)), async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        scanner.sight(unit(AMP)).await
    });
    let (began, _) = air.listens()[0];
    assert!(
        began - started >= Duration::from_secs(2),
        "the listen began during the verb's scan"
    );
}

/// A device asked for after a listen has said it is done, while the scan is stopping, has not
/// had that listen: it is not answered "not heard" by it, and gets the next.
#[tokio::test(start_paused = true)]
async fn a_device_asked_for_as_a_listen_stops_gets_the_next_one() {
    let air = OnAir::new(&[(AMP, Some(Duration::ZERO)), (DAC, None)]);
    let scanner = scanner_on(&air);
    let started = Instant::now();
    let (amp, dac) = tokio::join!(scanner.sight(unit(AMP)), async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let heard = scanner.sight(unit(DAC)).await;
        (heard, started.elapsed())
    });
    assert!(matches!(amp, Ok(Some(_))), "{amp:?}");
    assert!(matches!(dac.0, Ok(None)), "{:?}", dac.0);
    assert!(
        dac.1 >= SCAN_WINDOW,
        "the dac was answered by a listen that had stopped: {:?}",
        dac.1
    );
    assert_eq!(air.listens().len(), 2);
}

/// A transport that hands every wanted device on the air over once at the start of a listen and
/// checks `done()` only at the end of the window, as one that does not look between events.
struct LateDone(Arc<OnAir>);

#[async_trait::async_trait]
impl Transport for LateDone {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.0.scan(window).await
    }

    async fn listen(&self, window: Duration, listening: &dyn Listening) -> Result<(), BleError> {
        let air = &self.0;
        let began = Instant::now();
        air.listens.lock().unwrap().push((began, None));
        air.listening.store(true, Ordering::SeqCst);
        for (advert, from) in &air.devices {
            if from.is_some_and(|from| from <= began) && listening.wants(advert.unit_id()) {
                listening.heard(advert);
            }
        }
        tokio::time::sleep_until(began + window).await;
        listening.done();
        tokio::time::sleep(STOP).await;
        air.listening.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        Err(BleError::NotFound)
    }
}

/// A device heard while another is still looked for is answered at once even when the transport
/// does not call `done()` after handing it over.
#[tokio::test(start_paused = true)]
async fn an_early_answer_does_not_wait_for_the_transport_to_look_again() {
    let air = OnAir::new(&[(AMP, None), (DAC, Some(Duration::ZERO))]);
    let scanner = Scanner::new(Arc::new(LateDone(Arc::clone(&air))) as Arc<dyn Transport>);
    let started = Instant::now();
    let (amp, dac) = tokio::join!(scanner.sight(unit(AMP)), async {
        let heard = scanner.sight(unit(DAC)).await;
        (heard, started.elapsed(), air.is_listening())
    });
    assert!(matches!(dac.0, Ok(Some(_))), "{:?}", dac.0);
    assert_eq!(dac.1, Duration::ZERO);
    assert!(dac.2, "the dac's answer waited for the listen to stop");
    assert!(matches!(amp, Ok(None)), "{amp:?}");
}

/// A listen that panics answers who it was for with a failure, and the next one runs.
#[tokio::test(start_paused = true)]
async fn a_panicking_listen_costs_one_round() {
    struct PanicsOnce(Arc<OnAir>, AtomicBool);

    #[async_trait::async_trait]
    impl Transport for PanicsOnce {
        async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
            self.0.scan(window).await
        }

        async fn listen(&self, w: Duration, l: &dyn Listening) -> Result<(), BleError> {
            if !self.1.swap(true, Ordering::SeqCst) {
                self.0.listens.lock().unwrap().push((Instant::now(), None));
                panic!("the adapter broke");
            }
            self.0.listen(w, l).await
        }

        async fn connect(&self, _unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
            Err(BleError::NotFound)
        }
    }

    let air = OnAir::new(&[(AMP, None), (DAC, None)]);
    let scanner = Scanner::new(
        Arc::new(PanicsOnce(Arc::clone(&air), AtomicBool::new(false))) as Arc<dyn Transport>,
    );
    let (amp, dac) = tokio::join!(scanner.sight(unit(AMP)), scanner.sight(unit(DAC)));
    assert!(amp.is_err() && dac.is_err(), "{amp:?} {dac:?}");
    // An internal failure of the listen, with no word that sends the user to the adapter.
    for failed in [amp.unwrap_err(), dac.unwrap_err()] {
        assert!(failed.contains("internal error"), "{failed}");
        assert!(!failed.contains("adapter"), "{failed}");
    }
    let started = Instant::now();
    let later = scanner.sight(unit(AMP)).await;
    assert!(matches!(later, Ok(None)), "{later:?}");
    assert_eq!(started.elapsed(), SCAN_WINDOW + STOP);
    assert_eq!(air.listens().len(), 2);
}

/// The edge of `JOIN_GRACE`: a device asked for 200 ms into a listen shares it; one asked for at
/// 300 ms and not heard is answered by the next.
#[tokio::test(start_paused = true)]
async fn the_join_grace_edge() {
    let air = OnAir::new(&[(AMP, None), (DAC, None)]);
    let scanner = scanner_on(&air);
    let started = Instant::now();
    let (amp, dac) = tokio::join!(scanner.sight(unit(AMP)), async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let heard = scanner.sight(unit(DAC)).await;
        (heard, started.elapsed())
    });
    assert!(matches!(amp, Ok(None)) && matches!(dac.0, Ok(None)));
    assert_eq!(dac.1, SCAN_WINDOW + STOP);
    assert_eq!(air.listens().len(), 1);

    let air = OnAir::new(&[(AMP, None), (DAC, None)]);
    let scanner = scanner_on(&air);
    let started = Instant::now();
    let (_, dac) = tokio::join!(scanner.sight(unit(AMP)), async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let heard = scanner.sight(unit(DAC)).await;
        (heard, started.elapsed())
    });
    assert!(matches!(dac.0, Ok(None)));
    assert_eq!(dac.1, (SCAN_WINDOW + STOP) * 2);
    assert_eq!(air.listens().len(), 2);
}
