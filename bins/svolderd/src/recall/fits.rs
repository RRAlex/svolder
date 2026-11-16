// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fits of imported presets, kept off the runtime's threads.
//!
//! A fit is a second or so of one core. [`svolder_profiles::tone::preset::fit_for`] keeps every
//! result for the process, so a page model, a recall or a tool that asks for one again is
//! cheap; only the first ask of a correction on a tone-control layout is slow. [`warm`] makes
//! that first ask here, on a blocking thread behind the same permits as AutoEq's previews, so
//! the synchronous code that comes next only reads the kept result.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use svolder_profiles::EqSpec;
use svolder_profiles::tone::fit::Fit;
use svolder_profiles::tone::preset::{self, Preset};
use tokio::sync::Semaphore;

/// How many fits run at once: a fit is a second or so of one core, and a page asks for one per
/// preview.
const FITS_AT_ONCE: usize = 2;

/// The most keys [`warm`] remembers, as many as the fit cache keeps.
const WARMED_MAX: usize = 4096;

/// The permits every fit runs behind: AutoEq's previews, a page model, a recall, a tool.
static FITS: Semaphore = Semaphore::const_new(FITS_AT_ONCE);

/// The fits [`warm`] has made, by [`preset::fit_key`].
static WARMED: Mutex<Option<HashSet<Vec<u64>>>> = Mutex::new(None);

/// Run `work`, a fit, on a blocking thread once one of the permits is free. The permit goes
/// with the work, so a caller that goes away does not free it while the fit still runs. `Err`
/// when the work panicked.
pub async fn fitted<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, tokio::task::JoinError> {
    let permit = FITS
        .acquire()
        .await
        .expect("the fit permits are never closed");
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
}

/// Make the fit of each imported preset of `presets` to the tone control `eq` that has not been
/// made yet, one permit per fit so that AutoEq's previews interleave with a long list, so that
/// [`preset::on`], [`preset::matches`] and a recall find it kept. A fit that did not finish is
/// made again where it is asked for.
pub async fn warm(eq: &EqSpec, presets: &[Preset]) {
    for preset in presets {
        let Some(source) = preset.source.as_ref() else {
            continue;
        };
        let key = preset::fit_key(eq, &source.correction);
        if known(&key) {
            continue;
        }
        let (eq, preset) = (eq.clone(), preset.clone());
        let made = fitted(move || {
            #[cfg(test)]
            tests::slow(&preset.name);
            let fit = fit_one(&eq, &preset);
            #[cfg(test)]
            tests::record(&preset.name, fit);
            #[cfg(not(test))]
            drop(fit);
        })
        .await;
        if made.is_err() {
            continue;
        }
        let mut warmed = WARMED.lock().unwrap_or_else(PoisonError::into_inner);
        let warmed = warmed.get_or_insert_with(HashSet::new);
        if warmed.len() >= WARMED_MAX {
            warmed.clear();
        }
        warmed.insert(key);
    }
}

/// Whether [`warm`] has made the fit with this [`preset::fit_key`].
fn known(key: &[u64]) -> bool {
    let warmed = WARMED.lock().unwrap_or_else(PoisonError::into_inner);
    warmed.as_ref().is_some_and(|keys| keys.contains(key))
}

/// The fit of `preset` to `eq`, kept by [`preset::fit_for`]; `None` for a preset that is not
/// imported or whose filters stand on the bands as they are, which [`Preset::for_device`]
/// needs no fit for.
fn fit_one(eq: &EqSpec, preset: &Preset) -> Option<Arc<Fit>> {
    let source = preset.source.as_ref()?;
    if preset::as_bands(eq, &source.correction).is_some() || eq.bands.is_empty() {
        return None;
    }
    Some(preset::fit_for(eq, source))
}

#[cfg(test)]
pub(crate) mod tests;
