// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the crate's own diagnostic lines go: stderr, unless the host program installs a
//! logger.

use std::sync::OnceLock;

/// How serious a diagnostic line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// A step that went as expected.
    Info,
    /// A failure or something that looks wrong.
    Warn,
}

static LOGGER: OnceLock<fn(LogLevel, &str)> = OnceLock::new();

/// Send every diagnostic line the crate prints to `logger` instead of stderr. The text is the
/// line without its `svolder-ble: ` prefix. Only the first call takes effect.
pub fn set_logger(logger: fn(LogLevel, &str)) {
    let _ = LOGGER.set(logger);
}

/// Print one diagnostic line: through the installed logger, else on stderr.
pub(crate) fn line(level: LogLevel, text: &str) {
    match LOGGER.get() {
        Some(logger) => logger(level, text),
        None => eprintln!("svolder-ble: {text}"),
    }
}
