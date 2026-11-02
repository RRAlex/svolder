// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The verbs that read the device's values or write one of them: `status`, `inspect`, `get`,
//! `set` and `raw`.

use std::process::ExitCode;

use svolder_ipc::ValueObject;
use svolder_profiles::units::{self, Outside};
use svolder_profiles::{ControlKind, Profile, ProfileError};
use svolder_proto::{FunctionId, ProtoError};

use crate::backend::{Backend, Failure, Globals};
use crate::format::{bare_value, print_value};
use crate::recovery::Kind;

use super::device_presets::send_action;
use super::{open, print_values, report};

/// Reach the device, refreshing it on the way, and print every status it holds.
///
/// A direct session is closed before returning, so the BLE connection is released rather
/// than left open in bluetoothd — a Forkbeard device accepts one central at a time.
pub async fn inspect(g: &Globals<'_>, json: bool) -> Result<ExitCode, String> {
    read_and_print(g, json, Faults::Ignore).await
}

/// Reach the device and print every control's confirmed value.
///
/// Exits 5, after printing every reading, if any `Fault` control's state is boolean true.
/// A direct session is closed before returning, on either exit path.
pub async fn status(g: &Globals<'_>, json: bool) -> Result<ExitCode, String> {
    read_and_print(g, json, Faults::Check).await
}

/// Whether a read verb exits non-zero for an active fault.
#[derive(Clone, Copy)]
enum Faults {
    Ignore,
    Check,
}

/// Reach the device, print every status it holds, close the session, and answer with the code.
async fn read_and_print(g: &Globals<'_>, json: bool, faults: Faults) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    let code = match backend.state().await {
        Ok(values) => {
            print_values(&values, json);
            match faults {
                Faults::Ignore => ExitCode::SUCCESS,
                Faults::Check => fault_code(&backend, &values),
            }
        }
        Err(failure) => failure.report(),
    };
    backend.close().await;
    Ok(code)
}

/// 5 for an active fault, 3 when the faults cannot be read, 0 otherwise.
fn fault_code(backend: &Backend, values: &[ValueObject]) -> ExitCode {
    match backend.faulted(values) {
        Ok(Some(label)) => {
            eprintln!("svolder: active fault: {label}");
            ExitCode::from(5)
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("svolder: {message}");
            ExitCode::from(3)
        }
    }
}

/// Reach the device and print one control's current value.
///
/// An unknown key is a usage error (exit 1); a known key the device has not yet reported
/// exits 4. A direct session is closed before returning, on every path.
pub async fn get(g: &Globals<'_>, key: &str, json: bool) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    let outcome = print_one_reading(&mut backend, key, json).await;
    backend.close().await;
    outcome
}

/// Print the value the device last reported for `key`, or say why there is none.
async fn print_one_reading(
    backend: &mut Backend,
    key: &str,
    json: bool,
) -> Result<ExitCode, String> {
    // The readings come first so that a backend with nothing to read says so in its own
    // words rather than complaining about the key.
    let values = match backend.state().await {
        Ok(values) => values,
        Err(failure) => return Ok(failure.report()),
    };
    // A device the daemon answered for but no longer lists has no profile, so the key would
    // read as unknown whatever it is.
    if let Some(name) = backend.unlisted_device() {
        eprintln!("svolder: the daemon answered for an unknown device {name}; {key} not checked");
        return Ok(ExitCode::from(3));
    }
    // An action is sent and never read, so it has no value to wait for, and the words say which
    // verb sends it.
    if let Some(action) = backend.profile().action_by_key(key)
        && backend.profile().control_by_key(key).is_none()
    {
        return Err(ProfileError::NoReading {
            key: action.key.clone(),
            device_word: action.slots_word(),
        }
        .to_string());
    }
    let control = backend
        .profile()
        .control_by_key(key)
        .cloned()
        .ok_or_else(|| backend.profile().unknown_key(key).to_string())?;
    let id = control.id.to_string();
    let Some(vo) = values.iter().find(|vo| vo.id == id) else {
        eprintln!("svolder: no value reported yet for {key}");
        return Ok(ExitCode::from(4));
    };
    if json {
        print_value(vo, true);
    } else {
        println!("{}", bare_value(&control, vo));
    }
    Ok(ExitCode::SUCCESS)
}

/// Reach the device, command one control, and print the value it confirmed.
///
/// `value` is a literal (a number, name, unit-suffixed number, or `min`/`max`) passed to
/// [`svolder_profiles::intent`], or `up`/`down` to nudge a `Level` control's current raw
/// value by `step`, clamped to its range. A whole number for a centred level, the balance, counts
/// from its centre. `--no-wait` sends without confirming and prints
/// nothing on success. A device the daemon answered for but no longer lists is refused, as `get`
/// refuses it. A direct session is closed before returning, on every path.
///
/// A key that names an action of the profile, a recall or a save, takes a slot for its value and
/// has no reading to confirm: see [`send_action`]. `yes` answers the question a save asks, and
/// `no_wait` means nothing to it. They arrive as `(no_wait, yes)`.
pub async fn set(
    g: &Globals<'_>,
    key: &str,
    value: &str,
    step: i64,
    (no_wait, yes): (bool, bool),
    json: bool,
) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    // Without the device's profile a centred level would go to the daemon as a raw step.
    if let Some(name) = backend.unlisted_device() {
        eprintln!("svolder: the daemon answered for an unknown device {name}; {key} not checked");
        backend.close().await;
        return Ok(ExitCode::from(3));
    }
    let action = match backend.profile().control_by_key(key) {
        Some(_) => None,
        None => backend.profile().action_by_key(key).cloned(),
    };
    if let Some(action) = action {
        let outcome = send_action(&mut backend, &action, value, yes, json).await;
        backend.close().await;
        return outcome;
    }
    let value = match centred(backend.profile(), key, value) {
        Ok(value) => value,
        Err(failure) => {
            backend.close().await;
            return Ok(failure.report());
        }
    };
    let code = report(backend.set(key, &value, step, no_wait).await, json);
    backend.close().await;
    Ok(code)
}

/// `value` as the backend reads it. A whole number for a centred level, the balance, counts from
/// its centre, as every front end counts it, so `0` centres it, and is refused past either end in
/// those terms. Anything else goes as typed: `up`, `min`, a unit-suffixed number, and any number
/// for a level that is not centred.
fn centred(profile: &Profile, key: &str, value: &str) -> Result<String, Failure> {
    let Some(control) = profile
        .control_by_key(key)
        .filter(|control| control.kind == ControlKind::Level)
    else {
        return Ok(value.to_string());
    };
    let Some(bounds) = units::bounds(control).filter(|bounds| bounds.centred) else {
        return Ok(value.to_string());
    };
    let Ok(steps) = value.trim().parse::<i64>() else {
        return Ok(value.to_string());
    };
    units::raw_for(control, steps as f64, Outside::Refuse)
        .map(|raw| raw.to_string())
        .map_err(|_| {
            Failure::of(
                Kind::Usage,
                format!(
                    "{} is from {} to {}, counted from its centre; {steps} is outside that",
                    control.key, bounds.min, bounds.max
                ),
            )
        })
}

/// Reach the device, send one command straight to a function id, and print the confirmed
/// value.
///
/// A value type that does not exist, and a value that does not fit it, are refused here too,
/// before any backend is opened, and exit 1: the same words the daemon uses, without a connect
/// first. Both protected families are refused likewise: function
/// 250 changes the passphrase, and a `Restricted` id belongs to the firmware-update family.
/// [`FunctionId::refusal`] is the gate, so a direct session, the daemon and this early check all
/// say the same thing; `encode_command` refuses the firmware family a fourth time inside a
/// session, so no frame can reach a device by any other route. A direct session is closed
/// before returning, on every path.
pub async fn raw(
    g: &Globals<'_>,
    id: &str,
    value_type: &str,
    value: &str,
    no_wait: bool,
    json: bool,
) -> Result<ExitCode, String> {
    let parsed: FunctionId = id.parse().map_err(|e: ProtoError| e.to_string())?;
    if let Some(refusal) = parsed.refusal() {
        eprintln!("svolder: {refusal}");
        return Ok(ExitCode::from(1));
    }
    if let Err(failure) = Backend::check_raw(id, value_type, value) {
        return Ok(failure.report());
    }
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    let code = report(backend.raw(id, value_type, value, no_wait).await, json);
    backend.close().await;
    Ok(code)
}
