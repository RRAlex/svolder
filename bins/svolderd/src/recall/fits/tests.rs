// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use svolder_profiles::Registry;
use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::fit::Fit;

/// A preset whose name starts with this takes [`SLOW`] to fit, so a test can see whether the
/// thread that asked for the fit kept running meanwhile.
pub(crate) const SLOW_PREFIX: &str = "slow-fit";

/// How long a fit of a slow preset takes.
pub(crate) const SLOW: Duration = Duration::from_millis(300);

/// How many fits of presets named `slow-fit-counted` ran.
static COUNTED: AtomicUsize = AtomicUsize::new(0);

/// When the fit of the preset called `fit-fast-held` began.
static STARTED: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// The injected delay of a fit.
pub(super) fn slow(name: &str) {
    if name.starts_with("fit-short") {
        std::thread::sleep(Duration::from_millis(60));
        return;
    }
    if name == "fit-fast-held" {
        *STARTED.lock().unwrap() = Some(std::time::Instant::now());
        return;
    }
    if name.starts_with(SLOW_PREFIX) {
        if name == "slow-fit-counted" {
            COUNTED.fetch_add(1, Ordering::SeqCst);
        }
        std::thread::sleep(SLOW);
    }
}

/// The fits [`warm`] made, by preset name: what its fit returned.
static RECORDED: std::sync::Mutex<Vec<(String, Option<Arc<Fit>>)>> =
    std::sync::Mutex::new(Vec::new());

/// Note the fit `warm` made for the preset called `name`.
pub(super) fn record(name: &str, fit: Option<Arc<Fit>>) {
    RECORDED.lock().unwrap().push((name.to_string(), fit));
}

/// Whether the fit kept for `preset` on `eq` is the very one [`warm`] made: what the code that
/// reads the kept fit afterwards (`preset::on`, `matches`, a recall) gets.
pub(crate) fn consumer_reads_the_fit_warm_made(eq: &EqSpec, preset: &Preset) -> bool {
    let made = RECORDED
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(name, _)| *name == preset.name)
        .and_then(|(_, fit)| fit.clone());
    let source = preset.source.as_ref().expect("imported");
    made.is_some_and(|made| Arc::ptr_eq(&made, &preset::fit_for(eq, source)))
}

/// The tone control of the Gungnir.
pub(crate) fn eq() -> EqSpec {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("gungnir2-v2")
        .eq
        .clone()
        .expect("a tone control")
}

/// An imported preset named `name`, whose correction `gain` tells apart from the others.
pub(crate) fn imported(name: &str, gain: f64) -> Preset {
    preset::from_computer(
        name,
        Correction {
            preamp_db: -2.0,
            filters: (0..12)
                .map(|n| SourceFilter {
                    kind: FilterKind::Peaking,
                    f: 80.0 * 1.45_f64.powi(n),
                    g: gain,
                    q: 1.3,
                })
                .collect(),
        },
    )
}

/// Whether the fit of `preset` to `eq` has been made by [`warm`].
pub(crate) fn is_warm(eq: &EqSpec, preset: &Preset) -> bool {
    let key = preset::fit_key(eq, &preset.source.as_ref().expect("imported").correction);
    WARMED
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|keys| keys.contains(&key))
}

/// How many times a 20 ms interval ticked while `work` ran, on a runtime of one thread.
pub(crate) async fn ticks_during<F: std::future::Future>(work: F) -> usize {
    let mut interval = tokio::time::interval(Duration::from_millis(20));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut ticks = 0;
    tokio::pin!(work);
    loop {
        tokio::select! {
            _ = &mut work => return ticks,
            _ = interval.tick() => ticks += 1,
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_fit_runs_beside_the_runtime_which_keeps_ticking() {
    let ticks = ticks_during(fitted(|| std::thread::sleep(SLOW))).await;
    assert!(ticks >= 10, "the interval ticked {ticks} times in 300 ms");
}

#[tokio::test(flavor = "current_thread")]
async fn warming_a_slow_fit_keeps_the_runtime_ticking_and_leaves_it_kept() {
    let eq = eq();
    let preset = imported("slow-fit-ticking", 3.0);
    assert!(!is_warm(&eq, &preset));
    let ticks = ticks_during(warm(&eq, std::slice::from_ref(&preset))).await;
    assert!(ticks >= 10, "the interval ticked {ticks} times in 300 ms");
    assert!(is_warm(&eq, &preset));
    assert!(consumer_reads_the_fit_warm_made(&eq, &preset));
}

#[tokio::test(flavor = "current_thread")]
async fn a_fit_already_made_is_not_made_again() {
    let eq = eq();
    let preset = imported("slow-fit-counted", 4.0);
    warm(&eq, std::slice::from_ref(&preset)).await;
    warm(&eq, std::slice::from_ref(&preset)).await;
    assert_eq!(COUNTED.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn a_dropped_caller_does_not_free_the_permit_while_its_fit_runs() {
    let eq = eq();
    let (one, two, three) = (
        imported("slow-fit-held-1", 6.0),
        imported("slow-fit-held-2", 6.5),
        imported("fit-fast-held", 7.0),
    );
    // Two callers take both permits, then are dropped while their fits sleep; a third gets a
    // permit only when a fit has finished.
    let began = std::time::Instant::now();
    for preset in [&one, &two] {
        let (eq, preset) = (eq.clone(), preset.clone());
        let _ = tokio::time::timeout(Duration::from_millis(50), async move {
            warm(&eq, &[preset]).await
        })
        .await;
    }
    warm(&eq, &[three]).await;
    let started = STARTED.lock().unwrap().expect("the third fit ran") - began;
    assert!(
        started >= SLOW - Duration::from_millis(100),
        "the third fit began after {started:?}, while two fits still ran"
    );
}
