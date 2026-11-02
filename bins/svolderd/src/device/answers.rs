// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The response codes and words the device actor answers with: the code each link failure maps
//! to, and the answers for a refused request, a device with no tone control and a missing link.
//!
//! The actor builds its answers here so every command speaks with the same codes, which the
//! CLI turns into its exit codes.

use svolder_ble::BleError;
use svolder_ipc::Response;
use svolder_profiles::ProfileError;

/// The exit code the CLI maps each link failure to, as a response code.
pub fn code_for(err: &BleError) -> u8 {
    match err {
        BleError::NotFound => 2,
        BleError::Connection(_)
        | BleError::Bootloader
        | BleError::PassphraseRejected
        | BleError::Adapter(_) => 3,
        BleError::Timeout(_) => 4,
        BleError::Proto(_) | BleError::Profile(_) => 1,
    }
}

/// One link failure as a response.
pub(crate) fn error_for(err: &BleError) -> Response {
    Response::Error {
        code: code_for(err),
        message: err.to_string(),
    }
}

/// A request turned away before any frame was built, carrying the code and words the
/// response reports. Kept small so a `Result` that fails is not dominated by its error.
#[derive(Debug)]
pub struct Refused {
    /// The response code this refusal reports.
    pub code: u8,
    /// What to tell the front end.
    pub message: String,
}

impl From<Refused> for Response {
    fn from(refused: Refused) -> Response {
        Response::Error {
            code: refused.code,
            message: refused.message,
        }
    }
}

/// A request the daemon could not make sense of.
pub fn usage(message: impl Into<String>) -> Refused {
    Refused {
        code: 1,
        message: message.into(),
    }
}

/// A request the daemon could not make sense of, as a response.
pub(super) fn usage_error(message: &str) -> Response {
    usage(message).into()
}

/// The answer for a device whose profile describes no tone control, in the profile crate's words.
pub(super) fn no_eq() -> Response {
    usage(ProfileError::NoEq.to_string()).into()
}

/// The answer for anything that would need a link the actor does not have.
pub(super) fn not_connected() -> Response {
    Response::Error {
        code: 3,
        message: svolder_ipc::NOT_CONNECTED.to_string(),
    }
}
