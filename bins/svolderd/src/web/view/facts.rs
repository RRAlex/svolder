// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Device fold-out: what the daemon knows of a device that the screen does not show (its
//! product, unit id, firmware and DSP builds, profile, link, signal strength and passphrase
//! note) and an All readings table with every reading it holds, named or not. The part is
//! re-sent only when something it says changes ([`Facts::key`]); the ages are worked out on the
//! page from the stamps and the clock the part carries.

use svolder_ipc::{SIGNAL_MINUS, Signal, SignalSource, describe_signal};
use svolder_profiles::{ControlKind, describe};
use svolder_proto::{FunctionClass, FunctionId};

use super::{Snapshot, minus};

/// The main firmware build's reading.
const FIRMWARE: FunctionId = FunctionId::three(210, 2, 1);
/// The DSP firmware build's reading.
const DSP: FunctionId = FunctionId::three(210, 2, 2);
/// The grain a reading's stamp counts at in [`Facts::key`]: a minute, in milliseconds.
const STAMP_GRAIN_MS: u64 = 60_000;
/// What stands where there is nothing to say: an em dash.
pub(crate) const NONE: &str = "\u{2014}";

/// The Device fold-out.
#[derive(Debug)]
pub(crate) struct Facts {
    /// The product's name, when the daemon has one.
    pub(crate) product: Option<String>,
    /// The advertised product id.
    pub(crate) product_id: u16,
    /// The unit id; empty for a guest, who is not shown it.
    pub(crate) unit: String,
    /// The main firmware build as the device read it, else the advertised build and
    /// ` (advertised)`, else a dash.
    pub(crate) firmware: String,
    /// The DSP firmware build, once the device has read it.
    pub(crate) dsp: Option<u16>,
    /// The profile the device is read through.
    pub(crate) profile: String,
    /// The link in the TUI's words: `connected` or `disconnected`, then what is amiss, joined
    /// by ` · `.
    pub(crate) link: String,
    /// The signal strength in the helper's words, with its age as of the clock the part carries;
    /// a dash while there is none.
    pub(crate) signal: String,
    /// What the page needs to age the signal in place; `None` while there is no signal.
    pub(crate) signal_age: Option<SignalAge>,
    /// How the configured passphrase disagrees with the advertisement, for a viewer who may see
    /// it.
    pub(crate) passphrase_note: Option<String>,
    /// Every reading the daemon holds, in function order, but the passphrase's echo: it carries
    /// the code, and the page reaches people who are not in radio range.
    pub(crate) rows: Vec<FactRow>,
    /// The daemon's clock when the part was made, in unix milliseconds: what the ages are
    /// counted from.
    pub(crate) now_ms: u64,
    /// Whether the Identify button works: the viewer may write.
    pub(crate) identify: bool,
    /// Whether the page has an Identify button at all: the profile offers the cue.
    pub(crate) identify_offered: bool,
    /// The least time between two identify cues the daemon sends, in seconds: what the page says
    /// to wait after a press that sent nothing.
    pub(crate) identify_gap_secs: u64,
}

/// The signal's words without its age, and what the age is counted from.
#[derive(Debug)]
pub(crate) struct SignalAge {
    /// `−62 dBm advert` or `−58 dBm live`.
    pub(crate) head: String,
    /// When it was heard, in unix milliseconds.
    pub(crate) heard_at_ms: u64,
    /// Whether it is a live signal, whose age shows only after half a minute.
    pub(crate) live: bool,
}

/// One reading in the All readings table.
#[derive(Debug)]
pub(crate) struct FactRow {
    /// Its function address, dotted.
    pub(crate) id: String,
    /// The profile's label, `?` when the profile names none.
    pub(crate) label: String,
    /// Its value in the page's words.
    pub(crate) value: String,
    /// When the daemon last heard it, in unix milliseconds.
    pub(crate) heard_at_ms: Option<u64>,
    /// Whether the profile calls it a meter: its value is on the meters already, and moves too
    /// often to be what the stream sends the part again for.
    pub(crate) meter: bool,
}

/// The device's fold-out.
pub(crate) fn facts(s: &Snapshot) -> Facts {
    let info = s.info;
    let flags = [
        info.refused.then_some("passphrase refused"),
        info.stale.then_some("readings stale"),
    ];
    let state = if info.connected {
        "connected"
    } else {
        "disconnected"
    };
    let link = std::iter::once(state)
        .chain(flags.into_iter().flatten())
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
    let firmware = match s.raw(FIRMWARE) {
        Some(build) => build.to_string(),
        None if info.firmware != 0 => format!("{} (advertised)", info.firmware),
        None => NONE.to_string(),
    };
    let signal = info.signal.as_ref();
    let guest = s.access.guest;
    Facts {
        product: info.product.clone(),
        product_id: info.product_id,
        unit: if guest {
            String::new()
        } else {
            info.unit.clone()
        },
        firmware,
        dsp: s.raw(DSP).and_then(|build| u16::try_from(build).ok()),
        profile: s
            .profile
            .map_or_else(|| info.profile.clone(), |p| p.name.clone()),
        link,
        signal: describe_signal(signal, s.now_ms, SIGNAL_MINUS),
        signal_age: signal.map(signal_age),
        passphrase_note: info
            .passphrase_note
            .filter(|_| !guest)
            .map(|note| format!("Passphrase {}", note.short())),
        rows: s
            .readings
            .iter()
            .filter(|m| m.id.class() != FunctionClass::Sensitive)
            .map(|m| {
                let meter = s
                    .profile
                    .and_then(|profile| profile.control_by_id(m.id))
                    .is_some_and(|control| control.kind == ControlKind::Meter);
                let (label, value) = match s.profile {
                    Some(profile) => {
                        let reading = describe(profile, m);
                        (reading.label, reading.display)
                    }
                    None => ("?".to_string(), m.value.to_string()),
                };
                FactRow {
                    id: m.id.to_string(),
                    label,
                    value: minus(&value),
                    heard_at_ms: s.heard_at.get(&m.id).copied(),
                    meter,
                }
            })
            .collect(),
        now_ms: s.now_ms,
        identify: s.access.writes && !guest,
        identify_offered: s.profile.is_none_or(|p| p.identify),
        identify_gap_secs: svolder_ble::IDENTIFY_GAP.as_secs(),
    }
}

/// What ages `signal` in place.
fn signal_age(signal: &Signal) -> SignalAge {
    SignalAge {
        head: signal.describe_undated(SIGNAL_MINUS),
        heard_at_ms: signal.heard_at_ms,
        live: signal.source == SignalSource::Live,
    }
}

impl Facts {
    /// What the part says, apart from how long ago: everything but the clock and the signal's age,
    /// with the minute of the newest reading's stamp, and without the meters' readings (their
    /// values are on the meters already, and an amp's change ten times a second). The stream sends
    /// the part again when this changes, so a part whose ages merely grew is not sent, and
    /// readings heard again with the same words send it once a minute at the most, however their
    /// stamps are staggered. The table still lists the meters, as they stood when the part was
    /// last sent for another reason. A row that is not the newest can therefore show an age up
    /// to about two minutes old until the next send.
    pub(crate) fn key(&self) -> String {
        let signal = self.signal_age.as_ref().map(|a| (&a.head, a.heard_at_ms));
        let rows: Vec<(&str, &str, &str)> = self
            .rows
            .iter()
            .filter(|r| !r.meter)
            .map(|r| (r.id.as_str(), r.label.as_str(), r.value.as_str()))
            .collect();
        // Only the newest stamp's minute: readings heard on staggered phases cross a minute
        // boundary at a different second each, and a key holding every row's minute would change
        // at each of them.
        let newest = self
            .rows
            .iter()
            .filter(|r| !r.meter)
            .filter_map(|r| r.heard_at_ms)
            .max()
            .map(|at| at / STAMP_GRAIN_MS);
        format!(
            "{:?}",
            (
                &self.product,
                self.product_id,
                &self.unit,
                &self.firmware,
                self.dsp,
                &self.profile,
                &self.link,
                signal,
                &self.passphrase_note,
                self.identify,
                self.identify_offered,
                (rows, newest),
            )
        )
    }
}
