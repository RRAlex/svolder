// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The svolder inter-process layer, in three groups: the socket both front ends speak through,
//! the configuration file, and the doctor's host checks. Four shared modules sit at the root
//! because several groups use them: `clock`, `error`, `passphrase_note` and `token`.
//!
//! | Module | What it holds |
//! | --- | --- |
//! | [`socket`] | The wire protocol, framing, server, client, socket paths, version check |
//! | [`config`] | The `config.toml` model, its parser, template and writers |
//! | [`checks`] | The environment checks behind `svolder doctor` |
//! | [`host`] | The host's player and route as the daemon reports them, and the path verdict |
//! | [`clock`] | Local wall-clock time |
//! | [`error`] | The crate's error types |
//! | [`passphrase_note`] | The note for a passphrase that does not match its advertisement |
//! | [`token`] | The bearer tokens the daemon's HTTP servers ask for |
//!
//! The crate's root re-exports the names the front ends use most (`Client`, `Request`, `Config`).
#![warn(missing_docs)]

pub mod checks;
pub mod clock;
pub mod config;
pub mod error;
pub mod host;
pub mod passphrase_note;
pub mod socket;
#[cfg(test)]
pub(crate) mod test_support;
pub mod token;

pub use config::{
    ColorDepth, Config, DaemonConfig, DeviceConfig, EffectsConfig, FamilyAccents, GraphSpan,
    HostConfig, IntroEffect, LinkSpan, McpConfig, TUI_SETTINGS, Transition, TuiConfig, TuiSetting,
    TuiSettingKind, TuiValue, WebAuth, WebConfig, WebGuest, normal_origin,
};
pub use error::{ConfigError, IpcError};
pub use host::{
    CaptureKind, CaptureView, DacFacts, FrameView, Hop, HostView, PathLine, PlayingView, Verdict,
    path_line, sink_is_dac,
};
pub use passphrase_note::{PassphraseNote, passphrase_verdict};
pub use socket::client::{Client, is_running};
pub use socket::protocol::{
    DeviceInfo, Event, LinkChange, MAX_SCAN_WINDOW, McpStatus, MqttStatus, NO_DEVICES_CONFIGURED,
    NOT_CONNECTED, NOT_FOUND_YET, READINGS_SPAN_SECS, RecallOut, Reply, Request, RequestLine,
    Response, ScanEntry, SeriesOut, TokenInfo, TokenKind, ValueObject, WEB_OFF, WebStatus,
    not_found_yet, raw_json, to_line,
};
pub use socket::server::{Bound, Handler, bind, listen_fds, serve, serve_bound, serve_on};
pub use socket::signal::{
    SIGNAL_MINUS, Signal, SignalSource, describe_signal, describe_signal_short,
};

/// Unit tests run with scratch homes, as `make test` gives them, so none reads or writes the
/// user's own config, state, data, cache or runtime directory, however the tests were started.
#[cfg(test)]
#[used]
#[unsafe(link_section = ".init_array")]
static SCRATCH_HOMES: extern "C" fn() = {
    extern "C" fn pin() {
        // SAFETY: the ELF initialisers run before `main`, while the process has one thread.
        unsafe { crate::config::pin_scratch_homes() }
    }
    pin
};
