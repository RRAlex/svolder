// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The words the Settings box says: each row's value, its explanation and its key line.

use super::*;

/// What a plain key's row says: `on` or `off` for a switch, the word for a word.
pub(super) fn key_value(key: &TuiSetting, tui: &TuiConfig) -> String {
    match &key.kind {
        TuiSettingKind::Bool { get, .. } => word(get(tui)),
        TuiSettingKind::Word { get, .. } => get(tui).to_string(),
        TuiSettingKind::Accent => {
            debug_assert!(false, "{} has a row per family, not a key row", key.key);
            String::new()
        }
    }
}

/// Where a plain key lives in the configuration file, with the value `tui` holds now.
fn key_line(key: &TuiSetting, tui: &TuiConfig) -> String {
    match &key.kind {
        TuiSettingKind::Bool { get, .. } => format!("[tui] {} = {}", key.key, get(tui)),
        TuiSettingKind::Word { get, .. } => format!("[tui] {} = \"{}\"", key.key, get(tui)),
        TuiSettingKind::Accent => {
            debug_assert!(false, "{} has a row per family, not a key row", key.key);
            format!("[tui] {}", key.key)
        }
    }
}

/// What the AutoEq refresh row says: `never`, `1 week`, `2 weeks` … `99 weeks`.
pub(super) fn refresh_word(weeks: RefreshWeeks) -> String {
    match weeks {
        RefreshWeeks::Never => "never".to_string(),
        RefreshWeeks::Weeks(1) => "1 week".to_string(),
        RefreshWeeks::Weeks(weeks) => format!("{weeks} weeks"),
    }
}

/// What the token row says while the token is on show; the token itself is in the right column.
const REVEALED: &str = "(revealed)";
/// What a row says of a value the file leaves out.
pub(super) const NOT_SET: &str = "not set";

/// `on` as a write row says it: `read-only`, or `writes`.
pub(super) fn access(on: bool, writes: &str) -> String {
    match on {
        true => writes.to_string(),
        false => "read-only".to_string(),
    }
}

/// The token row's value: the daemon's masked tail where its token is not the one this session
/// reads, else this session's own: masked, or `(revealed)`.
pub(super) fn token_word(app: &App) -> String {
    if let TokenState::Differs { masked, .. } = app.overlays.conn.token_state() {
        return masked;
    }
    match (&app.overlays.conn.token, app.overlays.conn.shown(app.now)) {
        (TokenView::Present { .. }, Some(_)) => REVEALED.to_string(),
        (TokenView::Present { masked, .. }, None) => masked.clone(),
        (TokenView::Missing, _) => "not made yet".to_string(),
        (TokenView::Unreadable(_), _) => "unreadable".to_string(),
        (TokenView::Unread, _) => "…".to_string(),
    }
}

/// The listen row's value: `loopback:PORT` for the default address, which is a cell too wide as
/// written, and the address as written for any other (cut to the row if it is too long; the
/// right column has it whole).
pub(super) fn listen_word(listen: std::net::SocketAddr) -> String {
    match listen.ip() == std::net::Ipv4Addr::LOCALHOST {
        true => format!("loopback:{}", listen.port()),
        false => listen.to_string(),
    }
}

/// The password row's word: where the password comes from, never the password. A password the
/// daemon cannot send for want of a username says so.
pub(super) fn password_word(mqtt: &MqttView) -> String {
    let unsent = match mqtt.username {
        None => " — not sent: no username",
        Some(_) => "",
    };
    match (mqtt.password_in_env, mqtt.password_in_file) {
        (true, _) => format!("from SVOLDER_MQTT_PASSWORD{unsent}"),
        (false, true) => format!("set{unsent}"),
        (false, false) => NOT_SET.to_string(),
    }
}

/// The MCP status row's word.
pub(super) fn mcp_word(app: &App) -> String {
    match (&app.overlays.conn.mcp_status, app.session.daemon) {
        (_, false) => "no daemon".to_string(),
        (None, true) => "…".to_string(),
        (Some(McpStatus::Off), _) => "off".to_string(),
        (Some(McpStatus::Listening { .. }), _) => "listening".to_string(),
        (Some(McpStatus::Failed { .. }), _) => "failed".to_string(),
        (Some(McpStatus::Unknown), _) => "unknown".to_string(),
    }
}

/// The MQTT status row's word: the state in one or two words, the trouble named where there is
/// one (the right column has the whole line).
pub(super) fn mqtt_word(app: &App) -> String {
    match (&app.overlays.conn.mqtt_status, app.session.daemon) {
        (_, false) => "no daemon".to_string(),
        (None, true) => "…".to_string(),
        (Some(MqttStatus::Off), _) => "off".to_string(),
        (Some(MqttStatus::Connecting { .. }), _) => "connecting".to_string(),
        (Some(MqttStatus::Connected { .. }), _) => "connected".to_string(),
        (Some(MqttStatus::Trouble { problem, .. }), _) => match problem {
            MqttProblem::Refused => "login refused",
            MqttProblem::Unreachable => "not reachable",
            MqttProblem::Tls => "TLS port",
            MqttProblem::Unknown => "trouble",
        }
        .to_string(),
        (Some(MqttStatus::Unknown), _) => "unknown".to_string(),
    }
}

/// What the box's right column says of `setting`: what it does and the values it takes.
pub fn explain(setting: Setting) -> Cow<'static, str> {
    Cow::Borrowed(match setting {
        Setting::Key(key) => return (key.help)(),
        Setting::Palette => return (PALETTE.help)(),
        Setting::Accent(Family::Dac) => {
            "What a DAC draws its accent in: the cursor, the rule under the header and the key \
             hints. (preset) keeps the palette's own colour; the arrows step through the \
             colours the palette gives the other two families, and back to a colour of your own."
        }
        Setting::Accent(Family::Amp) => {
            "What an amplifier draws its accent in: the cursor, the rule under the header and \
             the key hints. (preset) keeps the palette's own colour; the arrows step through \
             the colours the palette gives the other two families, and back to a colour of your \
             own."
        }
        Setting::Accent(Family::Preamp) => {
            "What a preamplifier draws its accent in: the cursor, the rule under the header and \
             the key hints. (preset) keeps the palette's own colour; the arrows step through \
             the colours the palette gives the other two families, and back to a colour of your \
             own."
        }
        Setting::Colors => return (COLORS.help)(),
        Setting::Effects => {
            "The animation layer as a whole: off stops every animation, the transitions \
             included. It reads on (inactive) where the terminal cannot show it, below true \
             colour or in monochrome."
        }
        Setting::Effect(EffectSwitch::Eased) => {
            "Whether a changed value's bar glides to its new length instead of jumping there. A \
             meter's bar never glides."
        }
        Setting::Effect(EffectSwitch::Glow) => {
            "Whether the row the cursor just left keeps a fading trail for about 300 ms."
        }
        Setting::Effect(EffectSwitch::Shimmer) => {
            "Whether a bright band crosses the row you just changed or moved onto, as feedback. \
             While you rest it crosses the selected row again after 2 s, 10 s, 30 s and 2 \
             minutes."
        }
        Setting::Effect(EffectSwitch::Breathing) => {
            "Whether the pane borders pulse with the level: the meters' while they stream, else \
             what the host plays. They stay still with neither."
        }
        Setting::Effect(EffectSwitch::Frames) => {
            "Whether a band of light runs once round the frame of the pane that takes the focus. \
             Its brightness and pace follow the level where there is one."
        }
        Setting::Effect(EffectSwitch::FaultPresence) => {
            "Whether a standing fault's row and the header border pulse slowly while the fault \
             stays active."
        }
        Setting::Effect(EffectSwitch::PeakHold) => {
            "Whether a marker lingers at a meter's recent peak and decays over about a second."
        }
        Setting::Effect(EffectSwitch::ListFade) => {
            "Rows of a list dim a little with their distance from the selected row, as btop does."
        }
        Setting::Effect(EffectSwitch::Mascot) => {
            "Whether Ratatoskr, the squirrel, may come out. It stays hidden until it is called."
        }
        Setting::HueDrift => {
            "Whether the pane borders, titles, the header rule and the key hints drift along the \
             palette's spectrum, cool to hot, glowing hotter as the level rises. The selection \
             and every value keep the resting accent."
        }
        Setting::SpectrumDrift => {
            "Whether the tone curve's colours scroll slowly round the hue circle, one round trip \
             every 20 s, rather than resting red at 20 Hz and violet at 20 kHz. On red and neon \
             the curve's own ramp slides along its colours and back instead."
        }
        Setting::Mouse => return (MOUSE.help)(),
        Setting::HostAudio => return (HOST_AUDIO.help)(),
        Setting::WebUi => {
            "The web UI serves this screen's controls to a browser on this computer or your \
             network. Enter opens its box: on and off, how far it reaches, a link or a pairing \
             code to sign a browser in, and its token."
        }
        Setting::McpEnabled => {
            "Whether the daemon serves the Model Context Protocol over HTTP, for AI assistants. \
             Off, it opens no port. The change is written to the file and the daemon reloads."
        }
        Setting::McpListen => {
            "The address the MCP server listens on. Enter types one: loopback keeps the server \
             on this machine."
        }
        Setting::McpWrites => {
            "Whether the server offers the tools that change a device. Read-only offers only \
             the tools that look."
        }
        Setting::McpToken => {
            "The bearer token a client sends. v shows it for 10 s, c copies it to the \
             clipboard (to the terminal, and to the display's tool where there is one), R \
             replaces it after a question."
        }
        Setting::McpStatus => "Where the daemon says its MCP server stands. The daemon answers.",
        Setting::MqttEnabled => {
            "Whether the daemon bridges its devices to an MQTT broker, for Home Assistant. It \
             needs a broker host."
        }
        Setting::MqttHost => "The broker's host name or address. Enter types it.",
        Setting::MqttPort => "The broker's port. Enter types it.",
        Setting::MqttCommands => {
            "Whether the bridge takes commands back from the broker and writes them to the \
             devices. Read-only only reports."
        }
        Setting::MqttUsername => {
            "The username the broker wants. Enter types it; an empty line takes it away. With no \
             username the daemon sends no password either, and a broker that wants a login \
             refuses it."
        }
        Setting::MqttPassword => {
            "The broker's password. Enter types it, masked (Tab shows it); Delete twice on \
             the line clears it. Never read back. SVOLDER_MQTT_PASSWORD in the daemon's environment wins."
        }
        Setting::MqttStatus => "Where the daemon says its MQTT bridge stands. The daemon answers.",
        Setting::AutoEqRefresh => {
            "How old the cached AutoEq index may get before the search fetches it again: never, \
             or 1 to 99 weeks. Ctrl-R in the search fetches it at once whatever this says."
        }
    })
}

/// Where `setting` lives in the configuration file, with the value `tui` holds now: the table
/// and key, and `= value`. An accent a family leaves to the palette has no value to write, and
/// says so.
pub fn config_line(setting: Setting, tui: &TuiConfig) -> String {
    let effects = &tui.effects;
    let flag = |table: &str, key: &str, on: bool| format!("[{table}] {key} = {on}");
    let text = |table: &str, key: &str, value: &str| format!("[{table}] {key} = \"{value}\"");
    match setting {
        Setting::Key(key) => key_line(key, tui),
        Setting::Effects => flag("tui.effects", "enabled", effects.enabled),
        Setting::Effect(switch) => {
            let key = match switch {
                EffectSwitch::Eased => "eased",
                EffectSwitch::Glow => "glow",
                EffectSwitch::Shimmer => "shimmer",
                EffectSwitch::Breathing => "breathing",
                EffectSwitch::Frames => "frames",
                EffectSwitch::FaultPresence => "fault_presence",
                EffectSwitch::PeakHold => "peak_hold",
                EffectSwitch::ListFade => "list_fade",
                EffectSwitch::Mascot => "mascot",
            };
            flag("tui.effects", key, switch.of(effects))
        }
        Setting::HueDrift => flag("tui.effects", "hue_drift", effects.hue_drift),
        Setting::SpectrumDrift => flag("tui.effects", "spectrum_drift", effects.spectrum_drift),
        Setting::Palette => key_line(&PALETTE, tui),
        Setting::Accent(family) => {
            let (key, own) = (accent_key(family), tui.accents.get(family));
            match (tui.accent.as_deref(), own) {
                (Some(every), _) => text("tui", "accent", every),
                (None, Some(colour)) => text("tui.accents", key, colour),
                (None, None) => format!("[tui.accents] {key}  (not set)"),
            }
        }
        Setting::Colors => key_line(&COLORS, tui),
        Setting::Mouse => key_line(&MOUSE, tui),
        Setting::HostAudio => key_line(&HOST_AUDIO, tui),
        Setting::AutoEqRefresh => "[autoeq] refresh_weeks".to_string(),
        Setting::WebUi => "[web] enabled, listen".to_string(),
        Setting::McpEnabled => "[mcp] enabled".to_string(),
        Setting::McpListen => "[mcp] listen".to_string(),
        Setting::McpWrites => "[mcp] writes".to_string(),
        Setting::McpToken => "[mcp] token_file, or SVOLDER_MCP_TOKEN".to_string(),
        Setting::McpStatus => "from the daemon".to_string(),
        Setting::MqttEnabled => "[mqtt] enabled".to_string(),
        Setting::MqttHost => "[mqtt] host".to_string(),
        Setting::MqttPort => "[mqtt] port".to_string(),
        Setting::MqttCommands => "[mqtt] commands".to_string(),
        Setting::MqttUsername => "[mqtt] username".to_string(),
        Setting::MqttPassword => "[mqtt] password, or SVOLDER_MQTT_PASSWORD".to_string(),
        Setting::MqttStatus => "from the daemon".to_string(),
    }
}

/// Where the AutoEq refresh lives in the configuration file, with the value it holds now.
pub(super) fn autoeq_line(weeks: RefreshWeeks) -> String {
    match weeks {
        RefreshWeeks::Never => "[autoeq] refresh_weeks = \"never\"".to_string(),
        RefreshWeeks::Weeks(weeks) => format!("[autoeq] refresh_weeks = {weeks}"),
    }
}

/// Where a Connect row lives in the configuration file, with the value the file holds now. The
/// token and the password name where they come from and never a value.
pub(super) fn connection_line(setting: Setting, conn: &Connections) -> String {
    let mcp = &conn.mcp;
    let mqtt = &conn.mqtt;
    match setting {
        Setting::WebUi => format!(
            "[web] enabled = {}, listen = \"{}\"",
            conn.web.enabled, conn.web.listen
        ),
        Setting::McpEnabled => format!("[mcp] enabled = {}", mcp.enabled),
        Setting::McpListen => format!("[mcp] listen = \"{}\"", mcp.listen),
        Setting::McpWrites => format!("[mcp] writes = {}", mcp.writes),
        Setting::MqttEnabled => format!("[mqtt] enabled = {}", mqtt.enabled),
        Setting::MqttHost => match &mqtt.host {
            Some(host) => format!("[mqtt] host = \"{host}\""),
            None => "[mqtt] host  (not set)".to_string(),
        },
        Setting::MqttPort => format!("[mqtt] port = {}", mqtt.port),
        Setting::MqttCommands => format!("[mqtt] commands = {}", mqtt.commands),
        other => config_line(other, &TuiConfig::default()),
    }
}
