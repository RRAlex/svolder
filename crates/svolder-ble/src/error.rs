// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error type every transport and session operation returns.

use std::time::Duration;

/// Everything that can go wrong scanning, connecting, or talking to a device.
#[derive(Debug, thiserror::Error)]
pub enum BleError {
    /// No device in range matched the requested unit or selector.
    #[error("no matching device found")]
    NotFound,
    /// The link could not be established or was lost; the string names the cause.
    #[error("connection failed: {0}")]
    Connection(String),
    /// The device advertises firmware-update mode and must not be connected to.
    #[error("device is in firmware-update mode, use the official app")]
    Bootloader,
    /// The device reported the passphrase as wrong (status `250.255`).
    #[error("device passphrase was rejected")]
    PassphraseRejected,
    /// A set command was not confirmed within the timeout.
    #[error("no confirmation within {0:?}")]
    Timeout(Duration),
    /// The Bluetooth adapter is missing, powered off, or refused access.
    #[error("adapter unavailable: {0}")]
    Adapter(String),
    /// A frame could not be encoded or decoded.
    #[error(transparent)]
    Proto(#[from] svolder_proto::ProtoError),
    /// A profile could not interpret a value.
    #[error(transparent)]
    Profile(#[from] svolder_profiles::ProfileError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        assert_eq!(BleError::NotFound.to_string(), "no matching device found");
        assert_eq!(
            BleError::PassphraseRejected.to_string(),
            "device passphrase was rejected"
        );
        assert_eq!(
            BleError::Timeout(Duration::from_secs(2)).to_string(),
            "no confirmation within 2s"
        );
    }
}
