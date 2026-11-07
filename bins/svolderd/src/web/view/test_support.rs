// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the view models' tests and the JSON model's tests share: a device's row, a reading, and a
//! snapshot of a profile with its readings and everything else at rest.

use std::collections::BTreeMap;

use svolder_ipc::DeviceInfo;
use svolder_ipc::host::HostView;
use svolder_profiles::tone::preset;
use svolder_profiles::{Profile, intent};
use svolder_proto::{FunctionId, Message, Value, ValueType};

use super::{Access, Snapshot};

/// The row of a connected device called `name`.
pub(crate) fn info(name: &str) -> DeviceInfo {
    DeviceInfo {
        signal: None,
        name: name.into(),
        unit: "0102030405060708".into(),
        product_id: 2,
        product: Some(name.into()),
        firmware: 3,
        profile: String::new(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
    }
}

/// `profile`'s status for `key` at `raw` (a raw step, or a value's name).
pub(crate) fn read(profile: &Profile, key: &str, raw: &str) -> Message {
    let control = profile.control_by_key(key).expect("a control");
    if control.kind.writable() {
        let m = intent(profile, key, raw).expect("a reading");
        return Message::status(m.id, m.value);
    }
    // A readout, a fault or a meter has no command: its reading is built from its type.
    let value = match control.value_type {
        ValueType::Bool => Value::Bool(matches!(raw, "true" | "on" | "1")),
        ValueType::U8 => Value::U8(raw.parse().expect("a u8")),
        ValueType::U16 => Value::U16(raw.parse().expect("a u16")),
        other => panic!("{key} is a {other:?}"),
    };
    Message::status(control.id, value)
}

/// The tone control's readings once `name`, a built-in, is on.
pub(crate) fn tone_on(profile: &Profile, name: &str) -> Vec<Message> {
    let eq = profile.eq.as_ref().expect("a tone control");
    let found = preset::builtins(eq)
        .into_iter()
        .find(|p| p.name == name)
        .expect("a built-in");
    found
        .eq_writes(profile, |_| None)
        .expect("a plan")
        .plan
        .writes
        .iter()
        .filter_map(|w| Some(Message::status(w.id(eq)?, Value::U8(w.raw()))))
        .collect()
}

/// A configuration the daemon can write.
static WRITABLE: Result<(), String> = Ok(());

/// A viewer who may change a device.
pub(crate) const OPEN: Access = Access {
    writes: true,
    guest: false,
};

/// A host the daemon does not watch.
static NO_HOST: HostView = HostView {
    available: false,
    playing: None,
    route: None,
    now_ms: 0,
    volume: None,
    eq: None,
};

/// No reading has a stamp.
static UNSTAMPED: BTreeMap<FunctionId, u64> = BTreeMap::new();

/// The daemon's clock in the snapshots the tests take, in unix milliseconds.
pub(crate) const NOW_MS: u64 = 1_700_000_100_000;

/// A snapshot of `profile` with `readings`, everything else at rest.
pub(crate) fn shot<'a>(
    info: &'a DeviceInfo,
    profile: &'a Profile,
    readings: &'a [Message],
) -> Snapshot<'a> {
    Snapshot {
        info,
        profile: Some(profile),
        readings,
        heard_at: &UNSTAMPED,
        now_ms: NOW_MS,
        climb: None,
        cap_db: None,
        access: OPEN,
        presets: &[],
        writable: &WRITABLE,
        host: &NO_HOST,
    }
}
