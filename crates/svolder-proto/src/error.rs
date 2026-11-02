// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The single error type returned by every decode, parse and encode check in this crate.

/// Every way a frame or field can fail to decode or be refused for sending.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtoError {
    /// A frame or body has fewer bytes than the format requires.
    #[error("frame too short: {0} bytes")]
    TooShort(usize),
    /// A message body is longer than the u16 length prefix can carry.
    #[error("message body too long for the u16 length prefix: {0} bytes")]
    TooLong(usize),
    /// The trailing checksum did not match the bytes it covers.
    #[error("checksum mismatch: computed {computed:#06x}, received {received:#06x}")]
    ChecksumMismatch {
        /// The checksum this crate computed over the received bytes.
        computed: u16,
        /// The checksum found in the frame.
        received: u16,
    },
    /// A message's declared length runs past the bytes remaining before the checksum.
    #[error("declared message length {declared} exceeds remaining {available} bytes")]
    BadLength {
        /// The length the message claimed.
        declared: usize,
        /// The bytes actually available after the length prefix.
        available: usize,
    },
    /// A function-id field is empty or longer than three bytes.
    #[error("function id needs 1 to 3 bytes, got {0}")]
    BadFunctionIdLen(usize),
    /// Text does not parse as a dotted function id such as `2` or `210.2.1`.
    #[error("cannot parse function id {0:?}")]
    BadFunctionIdText(String),
    /// A value's payload does not have the byte count its type requires.
    #[error("value needs {expected} bytes, got {got}")]
    BadValueSize {
        /// The byte count the value type requires.
        expected: usize,
        /// The byte count actually present.
        got: usize,
    },
    /// The function belongs to the firmware-update family and cannot be sent.
    #[error("function {0} belongs to the firmware-update family and cannot be sent")]
    Restricted(String),
    /// The function changes the passphrase, which only the dedicated verb may do.
    #[error("function {0} changes the passphrase; use `svolder passphrase set`")]
    Sensitive(String),
    /// A message passed to [`crate::encode_notification`] was a Command, not a Status.
    #[error("message {0} is a command, not a status")]
    NotStatus(String),
    /// Manufacturer data was not exactly [`crate::MANUFACTURER_DATA_LEN`] bytes.
    #[error("manufacturer data must be 27 bytes, got {0}")]
    BadAdvertLen(usize),
    /// A passphrase digit was outside 0-9.
    #[error("passphrase digits must be 0-9")]
    BadPassphraseDigit,
    /// Text does not parse as 16 hex characters.
    #[error("unit id must be 16 hex characters")]
    BadUnitId,
    /// Text does not match one of the ten value-type names.
    #[error("unknown value type {0:?}")]
    BadValueTypeText(String),
    /// Text does not parse as a value of the type it was offered for; the message names
    /// both the text and the type it had to fit.
    #[error("{0}")]
    BadValueText(String),
}
