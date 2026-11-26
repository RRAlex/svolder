// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Bluetooth adapters a machine has: how one is named, how loudly each hears a device, and
//! what to tell a user whose adapter cannot hold a link another one could.
//!
//! An adapter is named by its kernel name (`hci0`) or by its Bluetooth address
//! (`AA:BB:CC:DD:EE:01`). The kernel numbers move when dongles are plugged in a different order
//! or the machine reboots; the address stays with the hardware, so a configuration that names
//! the address keeps meaning the same dongle. [`resolve`] turns either into the kernel name the
//! rest of the program opens, from a list of what is present.

use std::path::Path;
use std::time::Duration;

use btleplug::api::{Central, Manager as _};
use btleplug::platform::Manager;
use futures::StreamExt;
use futures::future::join_all;

use crate::bluez::transport::{BluezTransport, adapter_name};
use crate::error::BleError;
use crate::transport::Transport;

/// One adapter the machine has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInfo {
    /// The kernel's name for it, `hci0`.
    pub name: String,
    /// Its Bluetooth address, upper case with colons; empty when BlueZ does not say.
    pub address: String,
    /// What the USB device behind it calls itself, `UB500 (2357:0604)`; `None` for an adapter
    /// that is not a USB device.
    pub usb: Option<String>,
}

/// One device an adapter heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    /// The device's unit id, as hex.
    pub unit: String,
    /// The signal strength in dBm, when the adapter said.
    pub rssi: Option<i16>,
    /// What its advertisement said of the device, when it was heard rather than named: a row the
    /// device walk can offer for a device only this adapter hears.
    pub manufacturer: Option<svolder_proto::ManufacturerData>,
}

/// What one adapter heard in a listening window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hearing {
    /// The adapter that listened.
    pub adapter: AdapterInfo,
    /// Every device it heard, or why it could not listen.
    pub heard: Result<Vec<Heard>, String>,
}

impl Hearing {
    /// How the adapter heard `unit` (a unit id or a prefix of one): `None` when it did not hear
    /// it, `Some(None)` when it heard it with no signal reading.
    pub fn signal_of(&self, unit: &str) -> Option<Option<i16>> {
        let want = unit.to_ascii_lowercase();
        self.heard.as_ref().ok()?.iter().find_map(|heard| {
            heard
                .unit
                .to_ascii_lowercase()
                .starts_with(&want)
                .then_some(heard.rssi)
        })
    }
}

/// Whether `spec` is written as a Bluetooth address: six hex pairs with colons.
pub fn is_address(spec: &str) -> bool {
    let pairs: Vec<&str> = spec.split(':').collect();
    pairs.len() == 6
        && pairs
            .iter()
            .all(|pair| pair.len() == 2 && pair.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The adapters, in the words an error uses to say which there are.
fn present_words(present: &[AdapterInfo]) -> String {
    match present.is_empty() {
        true => "no adapters present".to_string(),
        false => format!(
            "adapters: {}",
            present
                .iter()
                .map(|a| match a.address.is_empty() {
                    true => a.name.clone(),
                    false => format!("{} {}", a.name, a.address),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The kernel name of the adapter `spec` names among `present`: a name (`hci0`) as before, or an
/// address in either case.
///
/// A spec no adapter answers to is an error naming the adapters there are, with their addresses;
/// it never falls back to another adapter, since a config that names one adapter means that one.
pub fn resolve(spec: &str, present: &[AdapterInfo]) -> Result<String, String> {
    let spec = spec.trim();
    if is_address(spec) {
        return present
            .iter()
            .find(|a| a.address.eq_ignore_ascii_case(spec))
            .map(|a| a.name.clone())
            .ok_or_else(|| {
                format!(
                    "no adapter has the address {}; {}",
                    spec.to_ascii_uppercase(),
                    present_words(present)
                )
            });
    }
    present
        .iter()
        .find(|a| a.name == spec)
        .map(|a| a.name.clone())
        .ok_or_else(|| format!("no adapter named {spec}; {}", present_words(present)))
}

/// `spec` in the form that survives a reboot: the address of the adapter it names when it names
/// one by kernel name and the adapter is among `present` with an address, else `spec` as it was.
/// What is remembered for a device is kept in this form, since the `hciN` numbers move.
pub fn stable_name(spec: &str, present: &[AdapterInfo]) -> String {
    present
        .iter()
        .find(|a| a.name == spec && !a.address.is_empty())
        .map_or_else(|| spec.to_string(), |a| a.address.clone())
}

/// What the USB device behind adapter `name` says of itself, read from the sysfs tree under
/// `sys`: the product string when it has one, with the vendor and product ids, or the ids alone.
pub fn usb_label(sys: &Path, name: &str) -> Option<String> {
    let device =
        std::fs::canonicalize(sys.join("class/bluetooth").join(name).join("device")).ok()?;
    // The adapter's own node is the USB interface; the USB device is its parent.
    let usb = device.parent()?;
    let read = |file: &str| {
        std::fs::read_to_string(usb.join(file))
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    };
    let ids = format!("{}:{}", read("idVendor")?, read("idProduct")?);
    Some(match read("product") {
        Some(product) => format!("{product} ({ids})"),
        None => ids,
    })
}

/// The adapters BlueZ reports, with their addresses and USB products, in name order.
pub async fn present() -> Result<Vec<AdapterInfo>, BleError> {
    let manager = Manager::new()
        .await
        .map_err(|e| crate::bluez::map_btleplug_error(&e))?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|e| crate::bluez::map_btleplug_error(&e))?;
    let mut found = Vec::new();
    for adapter in adapters {
        let Some(name) = adapter
            .adapter_info()
            .await
            .ok()
            .and_then(|info| adapter_name(&info).map(str::to_string))
        else {
            continue;
        };
        let address = match adapter.adapter_address().await {
            Ok(Some(address)) => address.to_string().to_ascii_uppercase(),
            _ => String::new(),
        };
        let usb = usb_label(Path::new("/sys"), &name);
        found.push(AdapterInfo { name, address, usb });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// Every adapter listens for `window` at the same time, and each says what it heard.
///
/// An adapter that cannot be opened or listened on says why instead of what it heard; one
/// failing never stops the others.
pub async fn hear_all(adapters: &[AdapterInfo], window: Duration) -> Vec<Hearing> {
    join_all(adapters.iter().map(|adapter| async move {
        Hearing {
            adapter: adapter.clone(),
            heard: hear_on(&adapter.name, window).await,
        }
    }))
    .await
}

/// What the adapter `name` hears in `window`.
async fn hear_on(name: &str, window: Duration) -> Result<Vec<Heard>, String> {
    let transport = BluezTransport::new(Some(name))
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = transport.scan(window).await.map_err(|e| e.to_string())?;
    let mut heard: Vec<Heard> = Vec::new();
    while let Some(advert) = stream.next().await {
        let unit = advert.unit_id().to_string();
        match heard.iter_mut().find(|h| h.unit == unit) {
            // The first reading stands unless a later one is the first to carry a signal.
            Some(known) => {
                if known.rssi.is_none() {
                    known.rssi = advert.rssi;
                }
            }
            None => heard.push(Heard {
                unit,
                rssi: advert.rssi,
                manufacturer: Some(advert.manufacturer),
            }),
        }
    }
    Ok(heard)
}

/// A signal as it is written: `−74 dBm`, with the minus sign.
pub fn dbm(rssi: i16) -> String {
    format!("{} dBm", rssi.to_string().replacen('-', "−", 1))
}

/// The fake adapters of `SVOLDER_FAKE_ADAPTERS` (`hci0=AA:BB:CC:DD:EE:01@-62,hci2=...@none`),
/// each hearing every device in `units` at its signal (`none`: hearing nothing); one adapter
/// hearing everything at -60 when the knob is unset.
pub fn fake_hearings(knob: Option<&str>, units: &[String]) -> Result<Vec<Hearing>, String> {
    let text = knob.unwrap_or("hci0=00:11:22:33:44:55@-60");
    let mut out = Vec::new();
    for entry in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let bad =
            || format!("bad SVOLDER_FAKE_ADAPTERS: {entry:?} is not <name>=<ADDRESS>@<dBm|none>");
        let (name, rest) = entry.split_once('=').ok_or_else(bad)?;
        let (address, signal) = rest.split_once('@').ok_or_else(bad)?;
        if !is_address(address) {
            return Err(bad());
        }
        let rssi = match signal {
            "none" => None,
            dbm => Some(dbm.parse::<i16>().map_err(|_| bad())?),
        };
        let heard = units
            .iter()
            .filter(|_| rssi.is_some())
            .map(|unit| Heard {
                unit: unit.clone(),
                rssi,
                manufacturer: None,
            })
            .collect();
        out.push(Hearing {
            adapter: AdapterInfo {
                name: name.to_string(),
                address: address.to_ascii_uppercase(),
                usb: Some("Fake dongle (1d6b:0246)".to_string()),
            },
            heard: Ok(heard),
        });
    }
    Ok(out)
}

/// The adapter that hears a device best of those that are not `failing`, and how loudly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Better {
    /// What to name in the config: the adapter's address, or its name where BlueZ gave none.
    pub take: String,
    /// The adapter's kernel name.
    pub name: String,
    /// How loudly it hears the device; `None` for a hearing with no signal reading.
    pub rssi: Option<i16>,
    /// How loudly the failing adapter hears it: `None` when it does not hear it at all.
    pub own: Option<Option<i16>>,
}

impl Better {
    /// Whether this adapter hears the device louder than the failing one does: any adapter does
    /// when the failing one hears nothing, and a reading with no signal is no louder than any.
    pub fn is_louder(&self) -> bool {
        match (self.own, self.rssi) {
            (None, _) => true,
            (Some(own), Some(other)) => other > own.unwrap_or(i16::MIN),
            (Some(_), None) => false,
        }
    }
}

/// The best other adapter for the device `unit`, when another adapter hears it at all.
pub fn better_than(failing: &str, unit: &str, hearings: &[Hearing]) -> Option<Better> {
    let own = hearings
        .iter()
        .find(|h| h.adapter.name == failing)
        .and_then(|h| h.signal_of(unit));
    let (other, signal) = hearings
        .iter()
        .filter(|h| h.adapter.name != failing)
        .filter_map(|h| h.signal_of(unit).map(|signal| (h, signal)))
        .max_by_key(|(_, signal)| signal.unwrap_or(i16::MIN))?;
    let take = match other.adapter.address.is_empty() {
        true => other.adapter.name.clone(),
        false => other.adapter.address.clone(),
    };
    Some(Better {
        take,
        name: other.adapter.name.clone(),
        rssi: signal,
        own,
    })
}

/// What to tell a user whose connects keep failing on the adapter `failing`, when another adapter
/// hears the device: which adapters hear it and the address to name in the config. `None` when
/// no other adapter hears it, so the message that was going to be shown is left as it is.
///
/// `device` is what the config calls it and `unit` its unit id.
pub fn pointer(failing: &str, device: &str, unit: &str, hearings: &[Hearing]) -> Option<String> {
    let better = better_than(failing, unit, hearings)?;
    Some(pointer_words(failing, device, &better))
}

/// The words of [`pointer()`] for the adapter `better` names.
pub fn pointer_words(failing: &str, device: &str, better: &Better) -> String {
    let other_hears = match better.rssi {
        Some(rssi) => format!("{} hears it at {}", better.name, dbm(rssi)),
        None => format!("{} hears it", better.name),
    };
    let own_hears = match better.own {
        Some(Some(rssi)) => format!(
            "{failing} hears {device} at {} and cannot hold a link",
            dbm(rssi)
        ),
        Some(None) => format!("{failing} hears {device} and cannot hold a link"),
        None => format!("{failing} does not hear {device}"),
    };
    format!(
        "{own_hears}; {other_hears}: try adapter = \"{}\"",
        better.take
    )
}

/// `what` without the pointer [`pointer_words`] added after it (`; <own>; <other>: try adapter =
/// "…"`), or as it was when it carries none.
pub fn without_pointer(what: &str) -> &str {
    let Some(at) = what.find(": try adapter = \"") else {
        return what;
    };
    let before = &what[..at];
    match before.rmatch_indices("; ").nth(1) {
        Some((start, _)) => &what[..start],
        None => what,
    }
}

/// Whether the words of a failed connect say it never came up within its deadline, which is what
/// a dongle that hears a device and cannot hold a link ends in. A refused passphrase, a device
/// that is not there and the like are no adapter's trouble.
pub fn is_connect_deadline(what: &str) -> bool {
    what.contains("no connection within")
}

/// How long every adapter listens when a failed connect looks for a better one: short, and once
/// per streak of failures.
pub const POINTER_WINDOW: Duration = Duration::from_secs(3);

/// The adapter that hears the device `unit` for a failing connect on `failing`, for a real
/// machine: the adapters it has listen for [`POINTER_WINDOW`], and the best of the others is
/// answered. `None` with fewer than two adapters (there is nothing to point at) and when no other
/// adapter hears the device.
///
/// The failing adapter listens too, on a transport of its own: BlueZ lets several clients
/// discover on one adapter, so the session that failed is not disturbed.
pub async fn better_for(failing: &str, unit: &str) -> Option<Better> {
    let present = present().await.ok()?;
    if present.len() < 2 {
        return None;
    }
    let hearings = hear_all(&present, POINTER_WINDOW).await;
    better_than(failing, unit, &hearings)
}

/// [`pointer()`] for a real machine, made of what [`better_for`] heard.
pub async fn pointer_for(failing: &str, device: &str, unit: &str) -> Option<String> {
    let better = better_for(failing, unit).await?;
    Some(pointer_words(failing, device, &better))
}

#[cfg(test)]
mod tests;
