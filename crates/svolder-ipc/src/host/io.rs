// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The two lines the config box and popup show under the sink: what comes in from the player and
//! what the card is open at. The one judge of these words: every front end shows them as given.

use super::sample::{Sample, khz, sample_of};
use serde::{Deserialize, Serialize};
use svolder_host::{HwParams, HwState, RouteFacts, StreamFacts};

/// What comes in and what goes out, as words.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IoLines {
    /// The player's stream as PipeWire negotiated it: `44.1 kHz · S16LE · stereo`, or `not read`.
    pub from_player: String,
    /// What the card is open at: `44.1 kHz · 32-bit (S32_LE) · stereo · hw:2`, `not open` when
    /// the card is closed, `not read` when the route says nothing of it.
    pub to_dac: String,
    /// The same without the format's name and the card, as the GNOME hover words end:
    /// `44.1 kHz · 32-bit · stereo`, or `not open`; empty when the route says nothing of the card.
    pub to_dac_short: String,
}

/// The two lines of `route`.
pub fn io_lines(route: &RouteFacts) -> IoLines {
    let alsa = route.setup.as_ref().and_then(|setup| setup.alsa.as_ref());
    let (to_dac, to_dac_short) = match alsa.map(|alsa| (&alsa.hw, alsa.card)) {
        Some((HwState::Open(hw), card)) => open_words(hw, card),
        Some((HwState::Closed, _)) => ("not open".to_string(), "not open".to_string()),
        _ => ("not read".to_string(), String::new()),
    };
    IoLines {
        from_player: route
            .player
            .as_ref()
            .and_then(player_words)
            .unwrap_or_else(|| "not read".to_string()),
        to_dac,
        to_dac_short,
    }
}

/// The stream's rate, format name and channels, whichever it has; `None` with none of them.
fn player_words(stream: &StreamFacts) -> Option<String> {
    let parts: Vec<String> = [
        stream.rate_hz.map(rate_words),
        stream.format.clone().filter(|format| !format.is_empty()),
        stream.channels.map(channel_words),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(JOIN))
}

/// What an open card says, with and without the format's own name and the card.
fn open_words(hw: &HwParams, card: Option<u32>) -> (String, String) {
    let depth = depth_words(&hw.format);
    let name = (!hw.format.is_empty()).then(|| hw.format.clone());
    let shown = match (&depth, &name) {
        (Some(depth), Some(name)) => Some(format!("{depth} ({name})")),
        (Some(depth), None) => Some(depth.clone()),
        (None, name) => name.clone(),
    };
    let short = [
        Some(rate_words(hw.rate)),
        depth.or(name),
        Some(channel_words(hw.channels)),
    ];
    let full = [
        Some(rate_words(hw.rate)),
        shown,
        Some(channel_words(hw.channels)),
        card.map(|card| format!("hw:{card}")),
    ];
    (join(full), join(short))
}

fn join(parts: impl IntoIterator<Item = Option<String>>) -> String {
    parts.into_iter().flatten().collect::<Vec<_>>().join(JOIN)
}

/// The words between the parts of a line.
const JOIN: &str = " \u{b7} ";

fn rate_words(rate_hz: u32) -> String {
    format!("{} kHz", khz(rate_hz))
}

/// `mono`, `stereo` or `N ch`.
fn channel_words(channels: u32) -> String {
    match channels {
        1 => "mono".to_string(),
        2 => "stereo".to_string(),
        other => format!("{other} ch"),
    }
}

/// The bit depth of an ALSA format name: `32-bit`, `24-bit`, `32-bit float`; `None` for a name
/// the judges cannot read.
fn depth_words(format: &str) -> Option<String> {
    if let Some(rest) = format.strip_prefix("FLOAT") {
        let bits = if rest.starts_with("64") { 64 } else { 32 };
        return Some(format!("{bits}-bit float"));
    }
    match sample_of(format)? {
        Sample::Int { bits, .. } => Some(format!("{bits}-bit")),
        Sample::Float => Some(format!(
            "{}-bit float",
            if format.contains("64") { 64 } else { 32 }
        )),
    }
}

#[cfg(test)]
mod tests;
