// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a key press or a click asks for: a write to the device, to the configuration file or
//! to the host, or a change of the `[tui]` table.

use svolder_autoeq::Entry as Measurement;
use svolder_host::Button;
use svolder_ipc::config::{McpConfig, MqttConfig, RefreshWeeks, WebConfig};
use svolder_ipc::{ColorDepth, DeviceConfig, EffectsConfig, TuiConfig, TuiSetting, TuiValue};
use svolder_profiles::Family;
use svolder_profiles::tone::apo::Correction;
use svolder_profiles::tone::preset::Preset;

/// One write a key press asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Set one control by key, from a literal value or `up`/`down` by `step`.
    Set {
        /// The profile key of the control to set.
        key: String,
        /// The value to set: a number, a value name, or `up`/`down`.
        value: String,
        /// How far `up`/`down` moves.
        step: i64,
    },
    /// Set one function address directly, for a reading no control names.
    Raw {
        /// The function address, as it is written.
        id: String,
        /// The wire type to send.
        value_type: String,
        /// The value to send, as text.
        value: String,
    },
    /// Set one band parameter, in human units.
    EqSet {
        /// The band, 1-based as it is typed and sent.
        band: usize,
        /// Which parameter: `f`, `g` or `q`.
        param: String,
        /// The frequency in Hz, gain in dB, or Q.
        value: f64,
    },
    /// Select the tone-control mode by name.
    EqMode(String),
    /// Tell the daemon this session recalled the preset of this name, so every other front end
    /// shows it; nothing on a direct session.
    Recalled(String),
    /// Ask a device to show itself: the one named, or the one on show for `None`.
    Identify(Option<String>),
    /// Read every value again.
    Refresh,
    /// Watch another device instead.
    SwitchDevice(String),
    /// Walk the user through adding a device, from the scan on, and watch what it adds.
    NewDevice,
    /// Write the device `1` into the configuration file over the block named `0`, which is the
    /// name it had before the editor was opened on it.
    UpdateDevice(String, DeviceConfig),
    /// Take the device named this out of the configuration file.
    RemoveDevice(String),
    /// Name this device as the one a command acts on when it names none, or take that key out
    /// of the file for `None`.
    DefaultDevice(Option<String>),
    /// Write this preset into the configuration file as a new `[[preset]]` block.
    SavePreset(Preset),
    /// Write the preset `1` into the configuration file over the block named `0`.
    UpdatePreset(String, Preset),
    /// Rename the preset named `0` to `1` in the configuration file.
    RenamePreset(String, String),
    /// Take the preset named this out of the configuration file.
    RemovePreset(String),
    /// Read the presets from the configuration file again, and the state of the computer EQ,
    /// for the preset list that has just opened.
    ReadPresets,
    /// Fetch the AutoEq index: from the cache unless it is due again by `[autoeq]
    /// refresh_weeks`, or at once when `refresh`.
    AutoEqIndex {
        /// Whether to fetch whatever the cached copy's age.
        refresh: bool,
    },
    /// Download this measurement's parametric EQ file.
    AutoEqDownload(Measurement),
    /// Put this correction on the computer, in front of its default output.
    HostApply {
        /// The preset's name.
        name: String,
        /// Where the correction came from, if it says.
        origin: Option<String>,
        /// What to apply.
        correction: Correction,
    },
    /// Take the computer EQ off, and then load the preset of this name onto the DAC.
    HostRemove {
        /// The preset to load once it is off.
        then_load: Option<String>,
    },
    /// Change one `[tui]` setting, which the loop applies and writes to the config file.
    Tui(TuiChange),
    /// Change how often the AutoEq index is fetched again, which the loop writes to `[autoeq]`
    /// of the config file.
    AutoEqRefresh(RefreshWeeks),
    /// Change one key of `[mcp]` or `[mqtt]`, which the loop writes to the config file and
    /// follows with a `reload`.
    Connection(ConnChange),
    /// Show the MCP token for a while.
    RevealToken,
    /// Copy the MCP token: to the terminal as OSC 52 and to the display's clipboard tool.
    CopyToken,
    /// Replace the MCP token with a fresh one, after the question has been answered yes.
    RotateToken,
    /// Ask the daemon for a link that signs a browser in, for the Web UI box.
    WebLink,
    /// Ask the daemon for a pairing code, for the Web UI box.
    WebPair,
    /// Copy the web link on show: to the terminal as OSC 52 and to the display's clipboard tool.
    CopyWebLink,
    /// Replace the web token with a fresh one, after the question has been answered yes.
    RotateWebToken,
    /// Copy the web link on show as [`Intent::CopyWebLink`] does and open it in a browser, for a
    /// click on it.
    OpenWebLink,
    /// Set the host's default sink to this volume, in percent.
    HostVolume(u8),
    /// Mute the host's default sink when it plays, and unmute it when it is muted.
    HostMute,
    /// Press this transport button on the host's followed media player.
    HostPress(Button),
    /// Leave.
    Quit,
}

/// One change the Connect category asks for, which is one key of `[mcp]`, `[mqtt]` or
/// `[web]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnChange {
    /// Whether the MCP server runs.
    McpEnabled(bool),
    /// The address the MCP server listens on.
    McpListen(std::net::SocketAddr),
    /// Whether the MCP server offers the tools that change a device.
    McpWrites(bool),
    /// Whether the MQTT bridge runs.
    MqttEnabled(bool),
    /// The broker's host.
    MqttHost(String),
    /// The broker's port.
    MqttPort(u16),
    /// Whether the bridge takes commands.
    MqttCommands(bool),
    /// Whether the web UI runs.
    WebEnabled(bool),
    /// The address the web UI listens on.
    WebListen(std::net::SocketAddr),
}

impl ConnChange {
    /// Take this change into `mcp` if it is one of the `[mcp]` keys; every other leaves it.
    pub(crate) fn apply_mcp(&self, mcp: &mut McpConfig) {
        match self {
            ConnChange::McpEnabled(on) => mcp.enabled = *on,
            ConnChange::McpListen(listen) => mcp.listen = *listen,
            ConnChange::McpWrites(on) => mcp.writes = *on,
            _ => {}
        }
    }

    /// Take this change into `mqtt` if it is one of the `[mqtt]` keys; every other leaves it.
    pub(crate) fn apply_mqtt(&self, mqtt: &mut MqttConfig) {
        match self {
            ConnChange::MqttEnabled(on) => mqtt.enabled = *on,
            ConnChange::MqttHost(host) => mqtt.host = Some(host.clone()),
            ConnChange::MqttPort(port) => mqtt.port = *port,
            ConnChange::MqttCommands(on) => mqtt.commands = *on,
            _ => {}
        }
    }

    /// Take this change into `web` if it is one of the `[web]` keys; every other leaves it.
    pub(crate) fn apply_web(&self, web: &mut WebConfig) {
        match self {
            ConnChange::WebEnabled(on) => web.enabled = *on,
            ConnChange::WebListen(listen) => web.listen = *listen,
            _ => {}
        }
    }

    /// Whether the change is to `[web]`.
    pub(crate) fn is_web(&self) -> bool {
        matches!(self, ConnChange::WebEnabled(_) | ConnChange::WebListen(_))
    }

    /// Whether the change is to `[mcp]`.
    pub(crate) fn is_mcp(&self) -> bool {
        matches!(
            self,
            ConnChange::McpEnabled(_) | ConnChange::McpListen(_) | ConnChange::McpWrites(_)
        )
    }
}

/// One change the Settings box asks for, which is one key of the `[tui]` table.
#[derive(Debug, Clone, PartialEq)]
pub enum TuiChange {
    /// One plain key of the `[tui]` table, set to a value of the kind it holds.
    Set(&'static TuiSetting, TuiValue),
    /// Whether the animation layer runs at all.
    Effects(bool),
    /// Whether one animation runs.
    Effect(EffectSwitch, bool),
    /// Whether the decoration drifts along the palette's spectrum, glowing hotter as the
    /// level rises.
    HueDrift(bool),
    /// Whether the tone curve's colours scroll round the hue circle (along the palette's own ramp
    /// on `red` and `neon`) rather than resting red at the lowest frequency and violet at the
    /// highest (or on that ramp).
    SpectrumDrift(bool),
    /// The palette preset to draw from.
    Palette(String),
    /// What one device family accents with, or `None` for the preset's own colour.
    Accent(Family, Option<String>),
    /// The colour depth to draw at.
    Colors(ColorDepth),
    /// Whether mouse capture is on.
    Mouse(bool),
}

impl TuiChange {
    /// Take this change into `tui`, which is the `[tui]` table as this session holds it.
    ///
    /// Setting one family's accent drops the single `accent` key with it: that key gives all
    /// three families the same colour and overrides them one and all, so a file carrying one
    /// would go on drawing its own colour while the row said otherwise. A user who asks a
    /// family for a colour of its own is asking for the three to differ.
    pub(crate) fn apply(&self, tui: &mut TuiConfig) {
        match self {
            TuiChange::Set(key, value) => key.apply(tui, *value),
            TuiChange::Effects(on) => tui.effects.enabled = *on,
            TuiChange::Effect(switch, on) => switch.set(&mut tui.effects, *on),
            TuiChange::HueDrift(on) => tui.effects.hue_drift = *on,
            TuiChange::SpectrumDrift(on) => tui.effects.spectrum_drift = *on,
            TuiChange::Palette(name) => tui.palette.clone_from(name),
            TuiChange::Accent(family, colour) => {
                tui.accent = None;
                tui.accents.set(*family, colour.clone());
            }
            TuiChange::Colors(depth) => tui.colors = *depth,
            TuiChange::Mouse(on) => tui.mouse = *on,
        }
    }

    /// Whether this change earns the sweep a screen change earns: the palette, the accent and the
    /// colour depth redraw every colour on the screen, and the sweep is what says so.
    pub(crate) fn sweeps(&self) -> bool {
        matches!(
            self,
            TuiChange::Palette(_) | TuiChange::Colors(_) | TuiChange::Accent(..)
        )
    }
}

/// One animation's own switch under `effects`, which the Settings box turns one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectSwitch {
    /// A changed value's bar glides to its new length.
    Eased,
    /// The row the cursor left keeps a fading trail.
    Glow,
    /// A highlight crosses the row the user just changed, and the selected row again while
    /// they rest, further and further apart.
    Shimmer,
    /// The pane contours pulse with the level: the meters', else the host capture's.
    Breathing,
    /// A light runs once round the frame of the pane the focus moves to. How a pane arrives
    /// on the screen is the `[tui]` transition's rather than this switch's.
    Frames,
    /// A standing fault's row and the header border pulse.
    FaultPresence,
    /// A marker lingers at a meter's recent peak.
    PeakHold,
    /// The rows of a list dim with their distance from the selected row.
    ListFade,
    /// The squirrel may come out when it is called.
    Mascot,
}

impl EffectSwitch {
    /// Whether `effects` has this switch on.
    pub fn of(self, effects: &EffectsConfig) -> bool {
        match self {
            EffectSwitch::Eased => effects.eased,
            EffectSwitch::Glow => effects.glow,
            EffectSwitch::Shimmer => effects.shimmer,
            EffectSwitch::Breathing => effects.breathing,
            EffectSwitch::Frames => effects.frames,
            EffectSwitch::FaultPresence => effects.fault_presence,
            EffectSwitch::PeakHold => effects.peak_hold,
            EffectSwitch::ListFade => effects.list_fade,
            EffectSwitch::Mascot => effects.mascot,
        }
    }

    /// Turn this switch on or off in `effects`.
    fn set(self, effects: &mut EffectsConfig, on: bool) {
        match self {
            EffectSwitch::Eased => effects.eased = on,
            EffectSwitch::Glow => effects.glow = on,
            EffectSwitch::Shimmer => effects.shimmer = on,
            EffectSwitch::Breathing => effects.breathing = on,
            EffectSwitch::Frames => effects.frames = on,
            EffectSwitch::FaultPresence => effects.fault_presence = on,
            EffectSwitch::PeakHold => effects.peak_hold = on,
            EffectSwitch::ListFade => effects.list_fade = on,
            EffectSwitch::Mascot => effects.mascot = on,
        }
    }
}

/// One `Set` of `key` to `value`, one step at a time as a key press asks for.
pub(super) fn set(key: &str, value: impl Into<String>) -> Intent {
    Intent::Set {
        key: key.to_string(),
        value: value.into(),
        step: 1,
    }
}

/// The relative move `dir` names, as the backend reads it.
pub(super) fn updown(dir: i64) -> &'static str {
    if dir > 0 { "up" } else { "down" }
}
