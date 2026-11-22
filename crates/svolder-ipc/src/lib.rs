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
//! | `error` | The crate's error types |
//! | `passphrase_note` | The note for a passphrase that does not match its advertisement |
//! | [`token`] | The bearer tokens the daemon's HTTP servers ask for |
//!
//! The crate's root re-exports the names the front ends use most (`Client`, `Request`, `Config`).
#![warn(missing_docs)]

pub mod checks;
pub mod clock;
pub mod config;
pub(crate) mod error;
pub mod host;
pub(crate) mod passphrase_note;
pub mod socket;
#[cfg(test)]
pub(crate) mod test_support;
pub mod token;

pub use config::scenes;
pub use config::{
    ColorDepth, Config, CoverArt, DaemonConfig, DeviceConfig, EffectsConfig, FamilyAccents,
    GraphSpan, HostConfig, IntroEffect, LinkSpan, McpConfig, TUI_SETTINGS, Transition, TuiConfig,
    TuiSetting, TuiSettingKind, TuiValue, WebAuth, WebConfig, WebGuest, normal_origin,
    passphrase_from, profile_for_advert,
};
pub use error::{ConfigError, IpcError};
pub use host::{
    CaptureKind, CaptureView, DacFacts, DacStage, FrameView, Hop, HostView, HostVolume, PathLine,
    PlayingView, SETUP_LIMIT, SetupCheck, SetupLine, SetupOutcome, SetupSummary, Verdict,
    computer_line, path_line, run_eq_op, setup_check, sink_is_dac,
};
pub use passphrase_note::{PassphraseNote, passphrase_verdict};
pub use socket::client::{Client, is_running, request_on_own_connection};
pub use socket::protocol::{
    ACTOR_STOP_MARGIN, AUTOEQ_GROUPS_MAX, AUTOEQ_QUERY_MAX, Activity, AutoEqGroup,
    AutoEqMeasurement, AutoEqOp, AutoEqPreview, AutoEqSaved, AutoEqSearch, AutoEqTarget,
    AutoEqVariant, BRIDGE_STOP_BOUND, CODE_COMPUTER_UNAVAILABLE, CODE_LINK, CODE_NOT_FOUND,
    CODE_TIMEOUT, CODE_UNKNOWN_REQUEST, CODE_USAGE, COMMAND_DEADLINE, COVER_CHUNK_BYTES,
    ConfiguredDevice, Connect, ConnectFailure, CoverReader, DAY_SECS, DEFAULT_REQUEST_TIMEOUT,
    DeviceConfigOp, DeviceInfo, Event, FEATURE_AUTOEQ, FEATURE_DEVICE_CRUD, FEATURE_GUEST_SWITCH,
    FEATURE_HOST_COVER, FEATURE_MQTT_SET, FEATURE_PRESET_WRITE, FEATURE_SCENES,
    FEATURE_SERVER_SWITCH, FEATURE_SPEAKERS, FEATURE_UNKNOWN_REQUEST_CODE, FEATURE_WALK, FEATURES,
    HostEqOp, JOB_RESULT_MAX_BYTES, JOB_STOP_BOUND, JobEvent, JobId, JobState, LinkChange,
    MAX_SCAN_WINDOW, McpStatus, MqttOp, MqttProblem, MqttStatus, NO_DEVICES_CONFIGURED,
    NOT_CONNECTED, NOT_FOUND_YET, PartOutcome, PartRow, PartStatus, PassphraseKind, PresetOp,
    ProfileChoice, READINGS_SPAN_SECS, RELOAD_TIMEOUT, RUNTIME_STOP_BOUND, Reason, RecallOut,
    Reply, Request, RequestLine, Response, SERIES_SIGNAL, SPEAKERS_OFF_PREFIX, ScanEntry, SceneOp,
    SceneRow, ScenesView, SeriesOut, Server, ServerActivity, SpeakersAngles, SpeakersCatalogue,
    SpeakersOp, SpeakersProgress, SpeakersRoom, SpeakersSaved, SpeakersSet, TokenInfo, TokenKind,
    ValueObject, WALK_IDLE_BOUND, WEB_OFF, WRITE_SETTLE_BOUND, WalkOp, WebStatus, cover_chunk,
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
