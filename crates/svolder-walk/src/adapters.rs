// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What every adapter heard while the walk listened, and the question it asks when the adapter
//! in use is not the one that hears the picked device best.

use serde::{Deserialize, Serialize};
use svolder_ipc::ScanEntry;
use svolder_profiles::Registry;

/// One adapter and what it heard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenedOn {
    /// the kernel's name for the adapter, `hci0`
    pub name: String,
    /// its Bluetooth address; empty where BlueZ gave none
    pub address: String,
    /// the units it heard, each with the signal strength in dBm when the adapter said
    pub heard: Vec<(String, Option<i16>)>,
    /// what the advertisements it heard said, where the adapter kept them: the rows for devices
    /// only another adapter hears. Not sent anywhere.
    #[serde(skip)]
    pub adverts: Vec<svolder_proto::ManufacturerData>,
}

impl ListenedOn {
    /// How this adapter heard `unit`: `None` when it did not, `Some(None)` when it heard the
    /// unit with no signal reading.
    pub fn reading(&self, unit: &str) -> Option<Option<i16>> {
        self.heard
            .iter()
            .find(|(heard, _)| heard.eq_ignore_ascii_case(unit))
            .map(|(_, rssi)| *rssi)
    }

    /// What to write as `adapter` in the config for this adapter: its address, which stays with
    /// the dongle, or its name where BlueZ gave no address.
    pub fn take(&self) -> &str {
        match self.address.is_empty() {
            true => &self.name,
            false => &self.address,
        }
    }
}

/// Every adapter listened at once, and which of them svolder uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listened {
    /// the kernel name of the adapter in use
    pub in_use: String,
    /// every adapter that listened, in name order
    pub adapters: Vec<ListenedOn>,
}

impl Listened {
    /// The adapter in use, when it is among those that listened.
    pub fn used(&self) -> Option<&ListenedOn> {
        self.adapters.iter().find(|a| a.name == self.in_use)
    }

    /// The adapter that hears `unit` best, when it is not the one in use and hears the unit
    /// louder than the one in use does: any adapter hears it louder when the one in use does not
    /// hear it at all, and a reading with no signal is no louder than any. `None` when the one
    /// in use is as good as any, so nothing is asked.
    pub fn better_for(&self, unit: &str) -> Option<&ListenedOn> {
        let own = self.used().and_then(|used| used.reading(unit));
        let (best, rssi) = self
            .adapters
            .iter()
            .filter(|a| a.name != self.in_use)
            .filter_map(|a| a.reading(unit).map(|rssi| (a, rssi)))
            .max_by_key(|(_, rssi)| rssi.unwrap_or(i16::MIN))?;
        let louder = match (own, rssi) {
            (None, _) => true,
            (Some(own), Some(other)) => other > own.unwrap_or(i16::MIN),
            (Some(_), None) => false,
        };
        louder.then_some(best)
    }
}

impl Listened {
    /// The rows for devices some adapter heard that the scan of the adapter in use did not: one
    /// per unit not in `known`, built from the advertisement as a scan's rows are, at the
    /// loudest reading any adapter gave it.
    pub fn rows_elsewhere(
        &self,
        known: &[ScanEntry],
        registry: Option<&Registry>,
    ) -> Vec<ScanEntry> {
        let mut rows: Vec<ScanEntry> = Vec::new();
        for adapter in &self.adapters {
            for advert in &adapter.adverts {
                let unit = advert.unit_id.to_string();
                if known.iter().any(|row| row.unit.eq_ignore_ascii_case(&unit))
                    || rows.iter().any(|row| row.unit.eq_ignore_ascii_case(&unit))
                {
                    continue;
                }
                let rssi = self
                    .adapters
                    .iter()
                    .filter_map(|other| other.reading(&unit)?)
                    .max();
                rows.push(match registry {
                    Some(registry) => ScanEntry::from_advert(advert, rssi, registry),
                    None => ScanEntry {
                        unit,
                        product_id: advert.product_id,
                        product: advert.product_name().map(str::to_string),
                        firmware: advert.firmware_build,
                        passphrase: advert.passphrase().is_well_formed(),
                        rssi,
                    },
                });
            }
        }
        rows
    }
}

/// A signal as it is written: `−74 dBm`, with the minus sign.
pub(crate) fn dbm(rssi: i16) -> String {
    format!("{} dBm", rssi.to_string().replacen('-', "\u{2212}", 1))
}

/// A unit id shortened for a narrow column: its first four and last four characters.
pub(crate) fn short_unit(unit: &str) -> String {
    let chars: Vec<char> = unit.chars().collect();
    match chars.len() > 9 {
        true => format!(
            "{}\u{2026}{}",
            chars[..4].iter().collect::<String>(),
            chars[chars.len() - 4..].iter().collect::<String>()
        ),
        false => unit.to_string(),
    }
}
