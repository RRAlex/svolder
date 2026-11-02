// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The configuration template: the whole file with every key commented out at the value it
//! has while absent, which a first configuration is made from and `config template` prints.

use std::path::Path;

use toml_edit::Value;

use super::edit::{lock, make_parent, write_atomically};
use super::{Config, ConfigError, TUI_SETTINGS, TuiSettingKind};

impl Config {
    /// The whole configuration file with every key commented out at its default, one line
    /// above each saying what the key does, and an example `[[device]]` block.
    ///
    /// `schema = 1` is the only live key: uncommenting any other line leaves svolder behaving
    /// exactly as it did, because each one shows the value the key has while it is absent —
    /// the empty string for keys whose default is to override nothing, and the default
    /// palette's own stops for the spectrum (what the glow travels while nothing names any).
    /// The values come from the same `Default` implementations the parser falls back on, so
    /// the template cannot drift from them.
    ///
    /// This is what a file svolder creates for the first time starts from, and what
    /// `config template` prints.
    pub fn template() -> String {
        let config = Config::default();
        let tui = &config.tui;
        let effects = tui.effects;
        let mut out = String::from(
            "# The svolder configuration. Every key below is commented out at the value svolder\n\
             # uses while the key is absent, so uncommenting a line on its own changes nothing.\n\
             \n\
             schema = 1\n",
        );
        commented(
            &mut out,
            "The Bluetooth adapter to talk through, named the way BlueZ names it.",
            "adapter",
            UNSET,
        );
        commented(
            &mut out,
            "The device a command acts on when it names none; one of the devices below.",
            "default_device",
            UNSET,
        );
        commented(
            &mut out,
            "The socket the front ends and the daemon meet on, overriding where svolder looks.",
            "socket",
            UNSET,
        );
        out.push_str("\n# How a command that needs a daemon gets one.\n[daemon]\n");
        commented(
            &mut out,
            "Whether a command that finds no daemon listening starts one of its own.",
            "autospawn",
            config.daemon.autospawn,
        );
        out.push_str(
            "\n# svolderd's bridge to an MQTT broker for Home Assistant, off until enabled with a host.\n[mqtt]\n",
        );
        commented(
            &mut out,
            "Whether the bridge runs at all.",
            "enabled",
            config.mqtt.enabled,
        );
        commented(&mut out, "The broker's host.", "host", UNSET);
        commented(
            &mut out,
            "The broker's port.",
            "port",
            i64::from(config.mqtt.port),
        );
        commented(
            &mut out,
            "The broker username, when it wants one.",
            "username",
            UNSET,
        );
        commented(
            &mut out,
            "The broker password, when it wants one; SVOLDER_MQTT_PASSWORD overrides this.",
            "password",
            UNSET,
        );
        commented(
            &mut out,
            "The client id to connect with; the default is svolder-, the host name and a hash of the socket.",
            "client_id",
            UNSET,
        );
        commented(
            &mut out,
            "The topic prefix every topic is built under.",
            "base_topic",
            config.mqtt.base_topic.as_str(),
        );
        commented(
            &mut out,
            "The discovery prefix Home Assistant watches; empty turns discovery off.",
            "discovery_prefix",
            config.mqtt.discovery_prefix.as_str(),
        );
        commented(
            &mut out,
            "Whether the bridge takes commands back, through the same validation `svolder set` uses.",
            "commands",
            config.mqtt.commands,
        );
        // An example, not the value while absent: there is no TOML spelling of "no cap" for a
        // number, unlike the blank string every optional text key above is shown at.
        commented(
            &mut out,
            "An optional cap, in dB, on a volume command's value; no cap while absent.",
            "max_volume_db",
            -10.0,
        );
        out.push_str(
            "\n# svolderd's MCP server for AI assistants, over HTTP, off until enabled.\n[mcp]\n",
        );
        commented(
            &mut out,
            "Whether the server runs at all.",
            "enabled",
            config.mcp.enabled,
        );
        commented(
            &mut out,
            "The address it listens on; the default is loopback only.",
            "listen",
            config.mcp.listen.to_string().as_str(),
        );
        commented(
            &mut out,
            "\"token\" asks for a bearer token (`svolder mcp token` prints it); \"none\" is for a loopback listen only.",
            "auth",
            "token",
        );
        commented(
            &mut out,
            "Whether the tools that change a device are offered.",
            "writes",
            config.mcp.writes,
        );
        commented(
            &mut out,
            "An optional cap, in dB, on a volume the server will set; no cap while absent.",
            "max_volume_db",
            -10.0,
        );
        commented(
            &mut out,
            "Browser origins allowed to call the server, such as \"https://claude.ai:443\" (a port, or :*, is required); none while empty.",
            "origins",
            toml_edit::Array::new(),
        );
        commented(
            &mut out,
            "Host names clients reach the server by (a reverse proxy's, say), such as \"mcp.home.lan\"; an entry without a port matches any port. Added to the listen address and the loopback names; with none set, a wildcard listen address accepts any Host.",
            "hosts",
            toml_edit::Array::new(),
        );
        commented(
            &mut out,
            "A file holding the token, in place of the generated one; SVOLDER_MCP_TOKEN outranks it.",
            "token_file",
            UNSET,
        );
        out.push_str(
            "\n# svolderd's web UI for a browser, off until enabled.\n\
             # To reach it from a phone, listen on \"0.0.0.0:8738\": that is plain HTTP, and anyone \
             who can watch your network can copy a sign-in.\n[web]\n",
        );
        commented(
            &mut out,
            "Whether the daemon serves the web UI.",
            "enabled",
            config.web.enabled,
        );
        commented(
            &mut out,
            "The address it listens on; the default is this computer only.",
            "listen",
            config.web.listen.to_string().as_str(),
        );
        commented(
            &mut out,
            "\"token\" asks a browser to sign in (`svolder web` opens one signed in); \"none\" is for a loopback listen only.",
            "auth",
            "token",
        );
        commented(
            &mut out,
            "Whether the page may change a device; false makes a read-only dashboard.",
            "writes",
            config.web.writes,
        );
        commented(
            &mut out,
            "An optional cap, in dB, on a volume the page sets; no cap while absent.",
            "max_volume_db",
            -10.0,
        );
        commented(
            &mut out,
            "\"read\" lets anyone who reaches the page look, read-only; \"none\" shows them the sign-in page.",
            "guest",
            "none",
        );
        commented(
            &mut out,
            "The address links and QR codes carry, such as \"https://svolder.home.lan/\"; worked out from listen while empty.",
            "url",
            UNSET,
        );
        commented(
            &mut out,
            "Host names browsers reach the page by (a reverse proxy's, say); with none set, a wildcard listen address accepts any Host.",
            "hosts",
            toml_edit::Array::new(),
        );
        commented(
            &mut out,
            "Origins a reverse proxy presents besides the page's own, such as \"https://svolder.home.lan\".",
            "origins",
            toml_edit::Array::new(),
        );
        commented(
            &mut out,
            "A file holding the token, in place of the generated one; SVOLDER_WEB_TOKEN outranks it.",
            "token_file",
            UNSET,
        );
        out.push_str(
            "\n# One block per device. `svolder setup` and `svolder add` write these for you.\n\
             # [[device]]\n\
             # The name the front ends pick this device out by.\n\
             # name = \"bifrost\"\n\
             # The device's 8-byte unit id, as sixteen hex characters.\n\
             # udid = \"1A2B3C4D5E6F7A8B\"\n\
             # The profile to drive it with, when it differs from the one svolder detects.\n\
             # profile = \"bifrost3\"\n\
             # The pairing passphrase: auto, none, or four digits.\n\
             # passphrase = \"auto\"\n",
        );
        out.push_str(
            "\n# One block per preset. `svolder eq save` writes these for you.\n\
             # [[preset]]\n\
             # The name the tone screen and `svolder eq` pick it by, at most 40 characters.\n\
             # name = \"HD650 warm\"\n\
             # How many bands the tone controls it fits have: 3 or 5.\n\
             # bands = 5\n\
             # The mode it puts the tone control in: flat, eq or loudness.\n\
             # mode = \"eq\"\n\
             # The loudness strength, 0-127.\n\
             # loudness = 0\n\
             # One entry per band, in band order: frequency in Hz, gain in dB and Q.\n\
             # band = [\n\
             #   { f = 40, g = 3.0, q = 0.7 },\n\
             #   { f = 160, g = 1.5, q = 0.7 },\n\
             #   { f = 632, g = 0.0, q = 4.3 },\n\
             #   { f = 2515, g = 0.0, q = 4.3 },\n\
             #   { f = 10000, g = 2.0, q = 0.7 },\n\
             # ]\n\
             \n\
             # An imported correction (`svolder eq import`) holds filters instead of bands, and\n\
             # fits every tone control: type is PK, LSC or HSC, f in Hz, g in dB and q.\n\
             # [[preset]]\n\
             # name = \"Sennheiser HD 650\"\n\
             # origin = \"AutoEq\"\n\
             # preamp = -6.1\n\
             # filter = [\n\
             #   { type = \"LSC\", f = 105.0, g = 6.0, q = 0.7 },\n\
             #   { type = \"PK\", f = 180.0, g = -2.1, q = 0.9 },\n\
             # ]\n",
        );
        out.push_str(
            "\n# What the sound goes through after the DAC, in order. `svolder hops` writes these.\n\
             # A hop is either named by hand, with a kind (amp, headphones, speakers or other), or\n\
             # linked to a configured device; `note` and `after` (the device it follows) are optional.\n\
             # [[hop]]\n\
             # name = \"HD 650\"\n\
             # kind = \"headphones\"\n\
             # note = \"balanced\"\n\
             # after = \"bifrost\"\n\
             \n\
             # [[hop]]\n\
             # device = \"vali\"\n",
        );
        out.push_str("\n# The AutoEq index `svolder eq search` reads, kept in the cache directory.\n[autoeq]\n");
        commented(
            &mut out,
            "Weeks (1 to 99) before the cached index is fetched again, or \"never\".",
            "refresh_weeks",
            i64::from(13),
        );
        out.push_str(
            "\n# Whether svolderd watches the computer's music player and the route its sound \
             takes to the DAC.\n[host]\n",
        );
        commented(
            &mut out,
            "Whether the daemon reads the player and the route.",
            "enabled",
            Config::default().host.enabled,
        );
        out.push_str("\n# How `svolder tui` looks and behaves.\n[tui]\n");
        for key in TUI_SETTINGS {
            let explain = (key.template)();
            match key.kind {
                TuiSettingKind::Bool { get, .. } => {
                    commented(&mut out, &explain, key.key, get(tui))
                }
                TuiSettingKind::Word { get, .. } => {
                    commented(&mut out, &explain, key.key, get(tui))
                }
                TuiSettingKind::Accent => commented(&mut out, &explain, key.key, UNSET),
            }
        }
        out.push_str("\n# Which of the animations run.\n[tui.effects]\n");
        for (switch, explain, on) in [
            (
                "enabled",
                "Whether the animation layer runs at all.",
                effects.enabled,
            ),
            (
                "eased",
                "Whether a changed value's bar glides to its new length and its row glows.",
                effects.eased,
            ),
            (
                "glow",
                "Whether the row the cursor left keeps a fading trail.",
                effects.glow,
            ),
            (
                "shimmer",
                "Whether a highlight sweeps the row you change, and the selected row while you rest.",
                effects.shimmer,
            ),
            (
                "breathing",
                "Whether panel contours pulse with the level: the amp's meters, else what the host plays.",
                effects.breathing,
            ),
            (
                "frames",
                "Whether a light runs once round the frame the keys move or arrive to.",
                effects.frames,
            ),
            (
                "fault_presence",
                "Whether a standing fault's row and the header border pulse.",
                effects.fault_presence,
            ),
            (
                "peak_hold",
                "Whether a marker lingers at a meter's recent peak.",
                effects.peak_hold,
            ),
            (
                "hue_drift",
                "Whether the decoration drifts along the palette's spectrum, glowing hotter as the level rises.",
                effects.hue_drift,
            ),
            (
                "spectrum_drift",
                "Whether the tone curve's colours scroll: round the hue circle, or along red's and neon's ramps.",
                effects.spectrum_drift,
            ),
            (
                "list_fade",
                "Whether the rows of a list dim with their distance from the selected row.",
                effects.list_fade,
            ),
            (
                "mascot",
                "mascot: Ratatoskr, the squirrel that comes out when called.",
                effects.mascot,
            ),
        ] {
            commented(&mut out, explain, switch, on);
        }
        out.push_str("\n# An accent for one device family at a time, which `accent` overrides.\n[tui.accents]\n");
        for (family, explain) in [
            ("dac", "The accent a DAC draws with."),
            ("amp", "The accent an amplifier draws with."),
            ("preamp", "The accent a preamplifier draws with."),
        ] {
            commented(&mut out, explain, family, UNSET);
        }
        out.push_str(
            "\n# The stops the glow travels, cool end to hot end, in place of the palette's own.\n\
             [tui.spectrum]\n",
        );
        commented(
            &mut out,
            "Three to five #rrggbb colours; the glow rests on the one nearest the DAC accent.",
            "stops",
            stops_value(&template_stops()),
        );
        out
    }

    /// Write [`Config::template`] at `path`, making the directories above it.
    ///
    /// This is what makes a host's first configuration file, not what replaces one: a file
    /// already there is left exactly as it is and the write is refused, so a fix run against a
    /// host that gained a config in the meantime cannot throw one away.
    pub fn create_from_template(path: &Path) -> Result<(), ConfigError> {
        make_parent(path)?;
        let _lock = lock(path)?;
        if path.exists() {
            return Err(ConfigError::Io(
                path.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "there is a configuration file here already",
                ),
            ));
        }
        write_atomically(path, &Self::template())
    }
}

/// The value the template shows a key at while it overrides nothing, and the value a writer
/// puts the key back at when it clears one.
pub(super) const UNSET: &str = "";

/// The default palette's own spectrum stops, cool end to hot end: what the TUI's amber preset
/// draws its glow along, and the one array both it and the template's own stops read rather
/// than each keeping a copy of the same four colours.
pub const DEFAULT_SPECTRUM: [(u8, u8, u8); 4] = [
    (0x5a, 0x1a, 0x08),
    (0xc0, 0x4a, 0x10),
    (0xe0, 0xa0, 0x30),
    (0xf6, 0xdc, 0xa0),
];

/// [`DEFAULT_SPECTRUM`] as the `#rrggbb` strings the template shows a spectrum at, and the
/// ones a writer puts the key back at when it clears one: the line says what the glow travels
/// while the key is absent.
pub(super) fn template_stops() -> [String; DEFAULT_SPECTRUM.len()] {
    std::array::from_fn(|i| hex(DEFAULT_SPECTRUM[i]))
}

/// `rgb` as the `#rrggbb` string [`template_stops`] writes it with.
fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `stops` as the array a spectrum is written and commented out at.
pub(super) fn stops_value<S: AsRef<str>>(stops: &[S]) -> Value {
    let mut array = toml_edit::Array::new();
    for stop in stops {
        array.push(stop.as_ref());
    }
    array.fmt();
    Value::Array(array)
}

/// Write one key into [`Config::template`]: what it does on one line, then the key itself
/// commented out at `value`.
fn commented(out: &mut String, explain: &str, key: &str, value: impl Into<Value>) {
    out.push_str("\n# ");
    out.push_str(explain);
    out.push('\n');
    out.push_str(&commented_line(key, value));
}

/// One key commented out at `value`, as [`Config::template`] writes it and as clearing the key
/// puts it back.
pub(super) fn commented_line(key: &str, value: impl Into<Value>) -> String {
    format!("# {key} = {}\n", value.into().to_string().trim())
}
