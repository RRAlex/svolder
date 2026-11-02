// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::*;
use crate::test_support::unit;

const AMP: &str = "0102030405060708";
const DAC: &str = "0807060504030201";

/// Work that takes `secs` seconds and then ends.
fn takes(secs: u64) -> impl Future<Output = ()> + Send + 'static {
    tokio::time::sleep(Duration::from_secs(secs))
}

/// A release that adds to `count`, whatever it is handed.
macro_rules! counting_release {
    ($count:ident) => {{
        let counted = Arc::clone(&$count);
        move |_unclaimed| async move {
            counted.fetch_add(1, Ordering::Release);
        }
    }};
}

/// Work that takes a second to succeed, standing in for the open steps on a radio.
async fn slow_success() -> Result<u32, BleError> {
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(7)
}

/// Work that panics instead of producing a result.
async fn panicking() -> Result<u32, BleError> {
    panic!("the open steps panicked")
}

/// A peer that counts the asks to take its connection down, and fails the first one when told
/// to.
#[derive(Clone, Default)]
struct FakePeer {
    asks: Arc<AtomicUsize>,
    fail_first: Arc<AtomicBool>,
}

impl FakePeer {
    fn failing_first() -> FakePeer {
        let peer = FakePeer::default();
        peer.fail_first.store(true, Ordering::SeqCst);
        peer
    }

    fn asks(&self) -> usize {
        self.asks.load(Ordering::SeqCst)
    }
}

impl Peer for FakePeer {
    fn take_down(&self) -> BoxFuture<'static, Result<(), BleError>> {
        let asks = Arc::clone(&self.asks);
        let fails = self.fail_first.swap(false, Ordering::SeqCst);
        async move {
            asks.fetch_add(1, Ordering::SeqCst);
            match fails {
                true => Err(BleError::Connection("disconnect timed out".into())),
                false => Ok(()),
            }
        }
        .boxed()
    }
}

/// One device's settle waits for that device's work alone: a release still running for
/// another device is none of its business.
#[tokio::test(start_paused = true)]
async fn a_settle_for_one_device_does_not_wait_for_another() {
    let releases = Releases::default();
    releases.spawn(unit(DAC), takes(5));
    releases.spawn(unit(AMP), takes(1));
    let started = tokio::time::Instant::now();
    let settled = releases
        .settle(Some(unit(AMP)), Duration::from_secs(10))
        .await;
    assert!(matches!(settled, Some(Ok(()))), "got {settled:?}");
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(1),
        "the amp's settle waited for the dac's release"
    );
}

/// Two settles at once for the same device both wait for its work: neither takes it from
/// the other.
#[tokio::test(start_paused = true)]
async fn two_settles_for_one_device_both_wait_for_its_work() {
    let releases = Releases::default();
    releases.spawn(unit(AMP), takes(3));
    let started = tokio::time::Instant::now();
    let (first, second) = tokio::join!(
        releases.settle(Some(unit(AMP)), Duration::from_secs(5)),
        releases.settle(Some(unit(AMP)), Duration::from_secs(5)),
    );
    assert!(
        matches!(first, Some(Ok(()))),
        "the first settle got {first:?}"
    );
    assert!(
        matches!(second, Some(Ok(()))),
        "the second settle got {second:?}"
    );
    assert_eq!(started.elapsed(), Duration::from_secs(3));
}

/// A settle for every device waits for all of them.
#[tokio::test(start_paused = true)]
async fn a_settle_for_every_device_waits_for_all_of_them() {
    let releases = Releases::default();
    releases.spawn(unit(DAC), takes(5));
    releases.spawn(unit(AMP), takes(1));
    let started = tokio::time::Instant::now();
    assert!(matches!(
        releases.settle(None, Duration::from_secs(10)).await,
        Some(Ok(()))
    ));
    assert_eq!(started.elapsed(), Duration::from_secs(5));
}

/// Work that ends leaves the record as it ends, not at the next spawn.
#[tokio::test(start_paused = true)]
async fn finished_work_leaves_the_record_at_once() {
    let releases = Releases::default();
    releases.spawn(unit(AMP), async {});
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(releases.len(), 0, "a finished task stayed on the record");
    assert!(
        releases
            .settle(None, Duration::from_secs(1))
            .await
            .is_none()
    );
}

/// A release that panics.
async fn release_that_panics() {
    panic!("the release panicked")
}

/// Work that panics leaves the record too.
#[tokio::test(start_paused = true)]
async fn work_that_panics_leaves_the_record() {
    let releases = Releases::default();
    releases.spawn(unit(AMP), release_that_panics());
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(releases.len(), 0);
}

/// A settle waits for work that the work it was waiting for started, such as the drop of a
/// link a release could not take down.
#[tokio::test(start_paused = true)]
async fn a_settle_waits_for_work_started_while_it_waits() {
    let releases = Releases::default();
    let more = releases.clone();
    releases.spawn(unit(AMP), async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        more.spawn(unit(AMP), takes(2));
    });
    let started = tokio::time::Instant::now();
    assert!(matches!(
        releases
            .settle(Some(unit(AMP)), Duration::from_secs(10))
            .await,
        Some(Ok(()))
    ));
    assert_eq!(started.elapsed(), Duration::from_secs(3));
}

/// A settle that runs out of time leaves the work on the record for the next one.
#[tokio::test(start_paused = true)]
async fn a_settle_that_runs_out_of_time_leaves_the_work_for_the_next_one() {
    let releases = Releases::default();
    releases.spawn(unit(AMP), takes(3));
    assert!(matches!(
        releases
            .settle(Some(unit(AMP)), Duration::from_secs(1))
            .await,
        Some(Err(_))
    ));
    assert!(matches!(
        releases
            .settle(Some(unit(AMP)), Duration::from_secs(5))
            .await,
        Some(Ok(()))
    ));
}

/// The same for a settle its caller drops.
#[tokio::test(start_paused = true)]
async fn a_settle_the_caller_drops_leaves_the_work_for_the_next_one() {
    let releases = Releases::default();
    releases.spawn(unit(AMP), takes(3));
    let dropped = tokio::time::timeout(
        Duration::from_millis(100),
        releases.settle(Some(unit(AMP)), Duration::from_secs(5)),
    )
    .await;
    assert!(dropped.is_err(), "the caller was meant to give up first");
    assert!(matches!(
        releases
            .settle(Some(unit(AMP)), Duration::from_secs(5))
            .await,
        Some(Ok(()))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_result_the_caller_takes_is_not_released() {
    let count = Arc::new(AtomicUsize::new(0));
    let releases = Releases::default();
    let got = deliver_or_release(
        unit(AMP),
        slow_success(),
        counting_release!(count),
        &releases,
    )
    .await
    .expect("the work succeeded");
    assert_eq!(got, 7);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(count.load(Ordering::Acquire), 0);
}

#[tokio::test(start_paused = true)]
async fn a_failed_result_is_released_even_though_the_caller_waited() {
    let count = Arc::new(AtomicUsize::new(0));
    let releases = Releases::default();
    let err = deliver_or_release(
        unit(AMP),
        async { Err::<u32, BleError>(BleError::NotFound) },
        counting_release!(count),
        &releases,
    )
    .await
    .expect_err("the work failed");
    assert!(matches!(err, BleError::NotFound));
    let settled = releases
        .settle(Some(unit(AMP)), Duration::from_secs(5))
        .await;
    assert!(matches!(settled, Some(Ok(())) | None), "got {settled:?}");
    assert_eq!(count.load(Ordering::Acquire), 1);
}

/// A caller that leaves mid-open — a session's connect deadline, a quit — has the open stopped
/// where it stands and its half-built connection taken down at once, not when the radio's own
/// connect timeout ends the open.
#[tokio::test(start_paused = true)]
async fn a_caller_that_leaves_mid_work_stops_it_and_releases_at_once() {
    let count = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let releases = Releases::default();
    let work = {
        let finished = Arc::clone(&finished);
        async move {
            tokio::time::sleep(Duration::from_secs(15)).await;
            finished.store(true, Ordering::SeqCst);
            Ok::<u32, BleError>(7)
        }
    };
    let call = deliver_or_release(unit(AMP), work, counting_release!(count), &releases);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), call)
            .await
            .is_err(),
        "the caller was meant to give up before the work finished"
    );
    let settled = releases
        .settle(Some(unit(AMP)), Duration::from_secs(1))
        .await;
    assert!(
        matches!(settled, Some(Ok(()))),
        "the release waited for the work its caller had left: {settled:?}"
    );
    assert_eq!(count.load(Ordering::Acquire), 1);
    tokio::time::sleep(Duration::from_secs(20)).await;
    assert!(
        !finished.load(Ordering::SeqCst),
        "the work went on after its caller left"
    );
}

/// A value the work built for a caller that had gone is taken down through itself, once: the
/// release does not ask a second time beside the value's own drop.
#[tokio::test(start_paused = true)]
async fn a_value_nobody_took_is_released_once_through_itself() {
    let peer = FakePeer::default();
    let releases = Releases::default();
    let held = Hold::new(peer.clone(), unit(AMP), releases.clone());
    let orphan = peer.clone();
    let call = deliver_or_release(
        unit(AMP),
        async move { Ok::<_, BleError>(held) },
        move |unclaimed: Option<Hold<FakePeer>>| async move {
            match unclaimed {
                Some(held) => {
                    let _ = held.release().await;
                }
                None => {
                    let _ = orphan.take_down().await;
                }
            }
        },
        &releases,
    );
    // The caller asks and leaves before the work's task has run, so the value is ready with
    // nobody to take it.
    {
        tokio::pin!(call);
        assert!(futures::poll!(call.as_mut()).is_pending());
    }
    let settled = releases
        .settle(Some(unit(AMP)), Duration::from_secs(1))
        .await;
    assert!(matches!(settled, Some(Ok(())) | None), "got {settled:?}");
    assert_eq!(
        peer.asks(),
        1,
        "the connection was asked to go more than once"
    );
}

/// A panic in the work is reported as a failed connection, and what the work may have half
/// built is still taken down.
#[tokio::test(start_paused = true)]
async fn work_that_panics_is_reported_and_still_released() {
    let count = Arc::new(AtomicUsize::new(0));
    let releases = Releases::default();
    let err = deliver_or_release(unit(AMP), panicking(), counting_release!(count), &releases)
        .await
        .expect_err("a panic is not a result");
    assert!(matches!(err, BleError::Connection(_)), "got {err:?}");
    let settled = releases
        .settle(Some(unit(AMP)), Duration::from_secs(1))
        .await;
    assert!(matches!(settled, Some(Ok(())) | None), "got {settled:?}");
    assert_eq!(
        count.load(Ordering::Acquire),
        1,
        "the panic skipped the release"
    );
}

/// A link dropped without a release asks once, on a task a settle for its device waits for.
#[tokio::test(start_paused = true)]
async fn a_dropped_hold_asks_once_on_a_recorded_task() {
    let peer = FakePeer::default();
    let releases = Releases::default();
    drop(Hold::new(peer.clone(), unit(AMP), releases.clone()));
    assert_eq!(releases.len(), 1, "the drop's ask is not on the record");
    assert!(matches!(
        releases
            .settle(Some(unit(AMP)), Duration::from_secs(1))
            .await,
        Some(Ok(()))
    ));
    assert_eq!(peer.asks(), 1);
}

/// A release that succeeded spares the drop its own ask.
#[tokio::test(start_paused = true)]
async fn a_released_hold_is_not_asked_again_when_dropped() {
    let peer = FakePeer::default();
    let releases = Releases::default();
    let held = Hold::new(peer.clone(), unit(AMP), releases.clone());
    held.release()
        .await
        .expect("the peer takes the connection down");
    drop(held);
    assert!(
        releases
            .settle(None, Duration::from_secs(1))
            .await
            .is_none()
    );
    assert_eq!(peer.asks(), 1);
}

/// A release that failed or timed out is asked once more by the drop.
#[tokio::test(start_paused = true)]
async fn a_hold_whose_release_failed_is_asked_again_when_dropped() {
    let peer = FakePeer::failing_first();
    let releases = Releases::default();
    let held = Hold::new(peer.clone(), unit(AMP), releases.clone());
    assert!(held.release().await.is_err());
    drop(held);
    assert!(matches!(
        releases
            .settle(Some(unit(AMP)), Duration::from_secs(1))
            .await,
        Some(Ok(()))
    ));
    assert_eq!(peer.asks(), 2);
}
