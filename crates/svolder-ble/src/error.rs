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
    /// A set command was sent and the device's report of it did not arrive within the timeout.
    #[error("sent, not confirmed: no report from the device within {0:?}")]
    Timeout(Duration),
    /// A set command was sent, and the only report of it was of another value, which may be a
    /// reading from before the write.
    #[error("sent, not confirmed: the device last reported {said}")]
    Unconfirmed {
        /// How long the wait ran.
        within: Duration,
        /// The device's last report of the function, in words.
        said: String,
    },
    /// A set command was never written: the wait for the write spacing outlasted the timeout.
    #[error("not sent: the link was busy for {0:?}")]
    NotSent(Duration),
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

/// What a BlueZ refusal with "Operation already in progress" is said as: another scan or
/// connection holds the adapter.
pub const ADAPTER_BUSY: &str = "the adapter is busy with another scan or connection";

impl BleError {
    /// The error for the text BlueZ gave when a scan or connection failed: a refusal because the
    /// adapter is already busy says so in words ([`ADAPTER_BUSY`]), any other text is a failed
    /// connection as it stands.
    pub fn from_bluez_text(text: impl Into<String>) -> BleError {
        let text = text.into();
        match text
            .to_ascii_lowercase()
            .contains("operation already in progress")
        {
            true => BleError::Adapter(ADAPTER_BUSY.to_string()),
            false => BleError::Connection(text),
        }
    }
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
            "sent, not confirmed: no report from the device within 2s"
        );
    }

    #[test]
    fn bluez_operation_already_in_progress_is_said_as_a_busy_adapter() {
        let e = BleError::from_bluez_text("Operation already in progress");
        assert!(matches!(e, BleError::Adapter(_)), "{e:?}");
        let words = e.to_string();
        assert!(
            words.contains("busy with another scan or connection"),
            "{words}"
        );
        assert!(
            !words.contains("svolder"),
            "the words are neutral for the daemon log: {words}"
        );
        assert!(!words.contains("Operation already in progress"), "{words}");
        let other = BleError::from_bluez_text("le-connection-abort-by-local");
        assert!(matches!(other, BleError::Connection(_)), "{other:?}");
    }
}
