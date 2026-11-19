// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the kernel says of a sound card, read from `/proc/asound` as plain files: the formats
//! and rates the card's `stream0` offers for playback, and the `hw_params` of the playback
//! substream (what reaches the hardware while it plays, `closed` otherwise). No process runs;
//! a file that is missing or cannot be understood reads as not read, never as clean.

use std::ffi::OsStr;
use std::path::PathBuf;

use super::{HwParams, HwState, KernelOffer, RouteFacts};

/// The environment variable naming the directory read in place of `/proc/asound`, for tests.
pub const ASOUND_DIR_ENV: &str = "SVOLDER_ASOUND_DIR";

/// The `/proc/asound` tree, or a copy of its files for a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asound {
    /// The directory holding the `card` folders.
    pub(super) root: PathBuf,
}

impl Asound {
    /// The tree rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `/proc/asound`, or the directory [`ASOUND_DIR_ENV`] names.
    pub fn from_env() -> Self {
        Self::resolve(std::env::var_os(ASOUND_DIR_ENV).as_deref())
    }

    /// The tree `from_env` makes of `named` (the variable's value); unset or empty is
    /// `/proc/asound`.
    pub(super) fn resolve(named: Option<&OsStr>) -> Self {
        let named = named.filter(|named| !named.is_empty());
        Self::new(named.unwrap_or(OsStr::new("/proc/asound")))
    }

    fn read(&self, relative: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(relative)).ok()
    }
}

/// The playback formats and rates in a card's `stream0` file: every `Format:` and `Rates:` line
/// of the `Playback:` section, each name once, the rates ascending. A rate range printed as
/// `a - b (continuous)` adds no rate. `None` when the section lists no format.
pub fn kernel_offer(stream0: &str) -> Option<KernelOffer> {
    let mut playback = false;
    let mut offer = KernelOffer::default();
    for line in stream0.lines() {
        if !line.starts_with(char::is_whitespace) && line.trim_end().ends_with(':') {
            playback = line.trim() == "Playback:";
            continue;
        }
        if !playback {
            continue;
        }
        let line = line.trim();
        if let Some(format) = line.strip_prefix("Format:") {
            let format = format.trim().to_string();
            if !format.is_empty() && !offer.formats.contains(&format) {
                offer.formats.push(format);
            }
        } else if let Some(rates) = line.strip_prefix("Rates:") {
            for rate in rates.split(',').filter_map(|rate| rate.trim().parse().ok()) {
                if !offer.rates.contains(&rate) {
                    offer.rates.push(rate);
                }
            }
        }
    }
    offer.rates.sort_unstable();
    (!offer.formats.is_empty()).then_some(offer)
}

/// The state a playback substream's `hw_params` file shows: `closed`, or the `format:`, `rate:`
/// and `channels:` of an open device. A file that lacks one of the three is not read.
pub fn hw_state(text: &str) -> HwState {
    if text.trim() == "closed" {
        return HwState::Closed;
    }
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
            .map(str::trim)
    };
    let number = |key: &str| value(key)?.split_whitespace().next()?.parse::<u32>().ok();
    match (value("format"), number("rate"), number("channels")) {
        (Some(format), Some(rate), Some(channels)) if !format.is_empty() => {
            HwState::Open(HwParams {
                format: format.to_string(),
                rate,
                channels,
            })
        }
        _ => HwState::NotRead,
    }
}

/// Fills the ALSA hop of `facts.setup` with what `asound` says of its card: `hw` from
/// `cardN/pcm{D}p/sub{S}/hw_params` (needs the device number; the substream is the node's, else
/// 0) and `kernel` from `cardN/stream{D}` (`stream0` when no device is named). A sink without a
/// card number reads nothing; a missing file leaves `NotRead` and `None`.
pub fn fill_alsa(facts: &mut RouteFacts, asound: &Asound) {
    let Some(hop) = facts.setup.as_mut().and_then(|setup| setup.alsa.as_mut()) else {
        return;
    };
    let Some(card) = hop.card else {
        return;
    };
    let sub = hop.subdevice.unwrap_or(0);
    hop.hw = hop
        .device
        .and_then(|device| asound.read(&format!("card{card}/pcm{device}p/sub{sub}/hw_params")))
        .map_or(HwState::NotRead, |text| hw_state(&text));
    let stream = hop.device.unwrap_or(0);
    hop.kernel = asound
        .read(&format!("card{card}/stream{stream}"))
        .and_then(|text| kernel_offer(&text));
}

#[cfg(test)]
mod tests;
