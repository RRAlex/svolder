// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The command line's definition: every flag, verb and sub-verb clap parses, and the help text
//! that goes with them.
//!
//! The tree is plain data for clap to derive from, so `main` parses it once and hands each verb
//! to its module under [`crate::commands`]. The command list `svolder --help` prints is built
//! here from one table of verbs and descriptions, the same one each verb's own `--help` reads.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};

/// What each verb gives, in the one line both the command list and the verb's own `--help`
/// show.
///
/// The `GROUPS` table renders these as the command list and the tree below hands the same
/// string to clap as the verb's `about`, so the two cannot come to say different things.
mod about {
    pub(super) const SETUP: &str = "The guided first run, safe to run again";
    pub(super) const DOCTOR: &str = "Check the host and each configured device, and offer fixes";
    pub(super) const SCAN: &str = "List every device in range with its ready-to-run `add` line";
    pub(super) const TUI: &str = "Open the interactive terminal UI";
    pub(super) const STATUS: &str = "Connect to a device and print every control's current value";
    pub(super) const GET: &str = "Read one control's current value";
    pub(super) const SET: &str = "Set one control's value, or recall or save a preset";
    pub(super) const WATCH: &str =
        "Connect to a device and print every status it reports until Ctrl-C";
    pub(super) const EQ: &str = "Inspect or change the tone control";
    pub(super) const PRESET: &str = "List or apply the device's own presets";
    pub(super) const HOST: &str = "Correct what the computer plays, before it reaches the DAC";
    pub(super) const DEVICES: &str = "List the configured devices and their state";
    pub(super) const ADD: &str = "Add a device to the configuration";
    pub(super) const REMOVE: &str = "Take a device out of the configuration";
    pub(super) const IDENTIFY: &str = "Ask the device to show itself, so you can tell which it is";
    pub(super) const HOPS: &str = "List or change what the sound goes through after the DAC";
    pub(super) const PASSPHRASE: &str = "Set or clear the device passphrase";
    pub(super) const PASSPHRASE_SET: &str =
        "Set the device passphrase to four digits (sends function 250)";
    pub(super) const PASSPHRASE_CLEAR: &str = "Clear the device passphrase (sends function 250)";
    pub(super) const DAEMON: &str = "Check, install or control the background service";
    pub(super) const INSPECT: &str = "Connect, refresh, and print everything the device sends";
    pub(super) const CAPTURE: &str = "Record everything about a device into one file to send";
    pub(super) const RAW: &str = "Send a command straight to a function id, bypassing the profile";
    pub(super) const DECODE: &str =
        "Decode a captured frame (command or notification) given as hex";
    pub(super) const PROFILES: &str = "Inspect device profiles";
    pub(super) const MCP: &str = "The MCP server's bearer token";
    pub(super) const WEB: &str = "Open the web UI, or print a link, a code or its token";
    pub(super) const CONFIG: &str = "Configuration paths, settings and the template";
}

/// Every verb, under the group it belongs to, in the order `--help` shows them.
///
/// The groups are tasks rather than subsystems: what a new user does first, what everyone does
/// daily, what changes the configured devices, what looks after the daemon, and what is there
/// when something needs taking apart.
const GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Getting started",
        &[
            ("setup", about::SETUP),
            ("doctor", about::DOCTOR),
            ("scan", about::SCAN),
        ],
    ),
    (
        "Everyday",
        &[
            ("tui", about::TUI),
            ("status", about::STATUS),
            ("get", about::GET),
            ("set", about::SET),
            ("watch", about::WATCH),
            ("eq", about::EQ),
            ("preset", about::PRESET),
            ("host", about::HOST),
        ],
    ),
    (
        "Devices",
        &[
            ("devices", about::DEVICES),
            ("add", about::ADD),
            ("remove", about::REMOVE),
            ("hops", about::HOPS),
            ("identify", about::IDENTIFY),
            ("passphrase", about::PASSPHRASE),
        ],
    ),
    (
        "Daemon",
        &[
            ("daemon", about::DAEMON),
            ("mcp", about::MCP),
            ("web", about::WEB),
        ],
    ),
    (
        "Diagnostics",
        &[
            ("inspect", about::INSPECT),
            ("capture", about::CAPTURE),
            ("raw", about::RAW),
            ("decode", about::DECODE),
            ("profiles", about::PROFILES),
            ("config", about::CONFIG),
        ],
    ),
];

/// The command list `--help` prints: each group under its own heading, each verb under its
/// group with what it gives.
///
/// clap heads one list of subcommands and cannot head several, so the list is built here and
/// [`help_layout`] puts it where clap's own would have gone. The verb column is as wide as the
/// longest verb, across every group, so the descriptions line up down the whole block.
fn grouped_help() -> String {
    let column = GROUPS
        .iter()
        .flat_map(|(_, verbs)| verbs.iter())
        .map(|(verb, _)| verb.len())
        .max()
        .unwrap_or_default();
    let mut out = String::new();
    for (group, verbs) in GROUPS {
        out.push_str(group);
        out.push_str(":\n");
        for (verb, about) in *verbs {
            out.push_str(&format!("  {verb:column$}  {about}\n"));
        }
        out.push('\n');
    }
    out
}

/// How `svolder --help` is laid out: what the program is, how it is called, the grouped
/// command list, the global flags, and a closing line pointing at `svolder help env`.
///
/// The command list and that closing line are this binary's own text; every other section is
/// clap's, through the placeholders it fills in.
fn help_layout() -> String {
    format!(
        "{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n{}Options:\n{{options}}\n\n\
         Run `svolder help env` to list the environment variables it reads.",
        grouped_help()
    )
}

#[derive(Parser)]
#[command(
    name = "svolder",
    version,
    long_version = svolder_proto::long_version!(),
    about = "Control a Schiit Forkbeard device from a Linux terminal",
    help_template = help_layout()
)]
pub(crate) struct Cli {
    /// Device selector: a unit-id prefix or a product name. Optional with one device.
    #[arg(short = 'd', long, global = true)]
    pub(crate) device: Option<String>,
    /// Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ
    /// session), `daemon` (the daemon, failing when none can be reached), `ble` (direct
    /// BlueZ), or `fake` (in-process).
    #[arg(long, global = true, env = "SVOLDER_TRANSPORT", default_value = "auto")]
    pub(crate) transport: String,
    /// Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured
    /// `adapter`, else the first one.
    #[arg(long, global = true, env = "SVOLDER_ADAPTER")]
    pub(crate) adapter: Option<String>,
    /// Daemon socket to use. Defaults to the runtime directory's `svolder.sock`.
    #[arg(long, global = true, env = "SVOLDER_SOCKET")]
    pub(crate) socket: Option<String>,
    /// Print the backend choice — a daemon, or a direct transport — to stderr
    #[arg(short = 'v', long, global = true)]
    pub(crate) verbose: bool,
    /// The verb to run. With none, `svolder` opens its front door: see
    /// [`crate::front_door::front_door`].
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    #[command(about = about::SETUP)]
    Setup,
    #[command(about = about::DOCTOR)]
    Doctor {
        /// Print one JSON object per check instead of text
        #[arg(long)]
        json: bool,
        /// Apply the fixes this verb can apply itself without asking
        #[arg(long)]
        yes: bool,
        /// Also check what a second adapter and a patched bluetoothd give
        #[arg(long)]
        dev: bool,
    },
    #[command(about = about::SCAN)]
    Scan {
        /// How long to listen for advertisements
        #[arg(long, value_parser = parse_duration, default_value = "5s")]
        timeout: Duration,
        /// Print one JSON object per device instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::TUI)]
    Tui {
        /// Force monochrome rendering; also set by the `NO_COLOR` environment variable
        #[arg(long)]
        no_color: bool,
        /// Try the TUI against a fake device on a daemon of its own, in a private directory
        /// that is removed on exit; nothing of yours is read or written
        #[arg(long)]
        demo: bool,
        /// With --demo, leave the computer's audio on: the real default sink's volume and
        /// meters, the real media player and the computer EQ, on the fake device. A volume or
        /// player change stays after the demo; the computer EQ the demo applied is removed
        #[arg(long, requires = "demo")]
        host: bool,
        /// With --demo, serve the web UI on a port of its own on every interface, signed in by
        /// token; the TUI's Web box and standard error show the link
        #[arg(long, requires = "demo")]
        web: bool,
        /// Open the TUI on the daemon listening on SOCKET and nothing else: no daemon is
        /// started and no direct session stands in, so a second front end can join a demo or
        /// any private daemon
        #[arg(long, value_name = "SOCKET", conflicts_with = "demo")]
        join: Option<PathBuf>,
    },
    #[command(about = about::STATUS)]
    Status {
        /// Print one JSON object per control instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::GET)]
    Get {
        /// The control to read, by its profile key
        key: String,
        /// Print one JSON object instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::SET)]
    Set {
        /// The control to set, by its profile key, or an action: `recall` or `save`
        key: String,
        /// A literal value (a number, name, unit-suffixed number, `min`/`max`), or `up`/`down`;
        /// for an action, the slot: `2` or `Preset 2`
        #[arg(allow_hyphen_values = true)]
        value: String,
        /// Raw-value step for `up`/`down`
        #[arg(long, allow_hyphen_values = true, default_value_t = 1)]
        step: i64,
        /// Send without waiting for the device to confirm
        #[arg(long)]
        no_wait: bool,
        /// Answer yes to the question a save asks
        #[arg(long)]
        yes: bool,
        /// Print one JSON object instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::WATCH)]
    Watch {
        /// Print one JSON object per reading instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::EQ)]
    Eq {
        #[command(subcommand)]
        command: EqCommand,
    },
    #[command(about = about::PRESET)]
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
    #[command(about = about::HOST)]
    Host {
        #[command(subcommand)]
        command: HostCommand,
    },
    #[command(about = about::DEVICES)]
    Devices {
        /// Print one JSON object per device instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::ADD)]
    Add {
        /// The device's 8-byte unit id, as sixteen hex characters
        udid: Option<String>,
        /// The name to select this device by
        #[arg(long)]
        name: Option<String>,
        /// The profile to use for this device, when it differs from the detected one
        #[arg(long)]
        profile: Option<String>,
        /// `auto`, `none`, or four digits
        #[arg(long, default_value = "auto")]
        passphrase: String,
    },
    #[command(about = about::REMOVE)]
    Remove {
        /// The configured name of the device to take out
        name: String,
    },
    #[command(about = about::HOPS, args_conflicts_with_subcommands = true)]
    Hops {
        /// Print one JSON object per hop instead of text
        #[arg(long)]
        json: bool,
        #[command(subcommand)]
        command: Option<HopsCommand>,
    },
    #[command(about = about::PASSPHRASE)]
    Passphrase {
        #[command(subcommand)]
        command: PassphraseCommand,
    },
    #[command(subcommand, about = about::DAEMON)]
    Daemon(DaemonCommand),
    #[command(about = about::INSPECT)]
    Inspect {
        /// Print one JSON object per reading instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::IDENTIFY)]
    Identify,
    #[command(about = about::CAPTURE)]
    Capture {
        /// How long to record every frame while you press the device's buttons, in seconds
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(0..=3600))]
        seconds: u32,
        /// Keep unit ids, passphrase bytes, the home directory and Bluetooth addresses in the file
        #[arg(long)]
        keep_ids: bool,
        /// Stop the daemon that holds the device, and start it again after, without asking
        #[arg(long)]
        yes: bool,
        /// Write the file here instead of a name made from the product and the time
        #[arg(short, long, value_name = "FILE")]
        out: Option<std::path::PathBuf>,
    },
    #[command(about = about::RAW)]
    Raw {
        /// The function address, e.g. `2` or `210.2.1`
        id: String,
        /// The wire type: u8, u16, u32, u64, i8, i16, i32, i64, bool, or data
        value_type: String,
        /// The value: a number, `on`/`off`/`true`/`false`/`1`/`0`, or hex bytes for `data` (empty for none)
        #[arg(allow_hyphen_values = true)]
        value: String,
        /// Send without waiting for the device to confirm
        #[arg(long)]
        no_wait: bool,
        /// Print one JSON object instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::DECODE)]
    Decode {
        /// Hex bytes, spaces allowed
        hex: String,
        /// Name functions using this profile
        #[arg(long)]
        profile: Option<String>,
        /// Print one JSON object instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(about = about::PROFILES)]
    Profiles {
        #[command(subcommand)]
        command: ProfilesCommand,
    },
    #[command(about = about::MCP)]
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    #[command(about = about::WEB, args_conflicts_with_subcommands = true)]
    Web {
        /// Print the link as a QR code for a phone, in place of opening it here
        #[arg(long)]
        qr: bool,
        #[command(subcommand)]
        command: Option<WebCommand>,
    },
    #[command(about = about::CONFIG)]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum EqCommand {
    /// Print the tone control's current mode, loudness and band settings
    Show {
        /// Print one JSON object per reading, then the preset and the peak, instead of text
        #[arg(long)]
        json: bool,
    },
    /// Set one band's frequency, gain, or Q
    Set {
        /// Band number, starting at 1
        band: usize,
        /// Which parameter: `f`/`frequency`, `g`/`gain`, or `q`
        param: String,
        /// The new value, in Hz, dB, or Q
        #[arg(allow_hyphen_values = true)]
        value: f64,
        /// Print one JSON object instead of text
        #[arg(long)]
        json: bool,
    },
    /// Select the tone-control mode
    Mode {
        /// `flat`, `eq`, or `loudness`
        mode: String,
        /// Print one JSON object instead of text
        #[arg(long)]
        json: bool,
    },
    /// List the presets, built-ins first
    List {
        /// Print one JSON object per preset instead of text
        #[arg(long)]
        json: bool,
        /// List the presets that do not fit the device's band count too
        #[arg(long)]
        all: bool,
    },
    /// Recall a preset onto the device
    Load {
        /// The preset's name
        name: String,
        /// Answer yes to the question asked when the preset is on the computer
        #[arg(long)]
        yes: bool,
    },
    /// Save the device's current curve as a preset
    Save {
        /// The name to save it under
        name: String,
        /// Overwrite a user preset of that name
        #[arg(long)]
        force: bool,
    },
    /// Import an Equalizer APO parametric EQ file, or an AutoEq headphone, as a preset
    #[command(group(clap::ArgGroup::new("source").required(true).args(["file", "autoeq"])))]
    Import {
        /// An Equalizer APO parametric EQ text file, as AutoEq writes them
        file: Option<PathBuf>,
        /// Download this headphone's correction from AutoEq instead of reading a file
        #[arg(long, value_name = "HEADPHONE")]
        autoeq: Option<String>,
        /// With --autoeq, the source to take the measurement from (the recommended one by
        /// default)
        #[arg(long, value_name = "SOURCE")]
        measured_by: Option<String>,
        /// The name to save it under; the file's name, or the headphone's, by default
        #[arg(long)]
        name: Option<String>,
        /// Overwrite a user preset of that name
        #[arg(long)]
        force: bool,
    },
    /// Search AutoEq's headphone index
    Search {
        /// The words of the headphone's name, in any case and with or without spaces
        query: Vec<String>,
        /// One line per measurement instead of one per headphone
        #[arg(long)]
        all: bool,
        /// Fetch the index again before searching; with no query, only that
        #[arg(long)]
        refresh: bool,
        /// Print one JSON object per line instead of text
        #[arg(long)]
        json: bool,
    },
    /// Rename a preset
    Rename {
        /// Its current name
        old: String,
        /// Its new name
        new: String,
    },
    /// Delete a preset
    Delete {
        /// The preset's name
        name: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Put every band back on its centre frequency and centre Q at 0 dB
    Flatten,
}

#[derive(Subcommand)]
pub(crate) enum PresetCommand {
    /// List the device's presets: its slots, then its built-in settings
    List {
        /// Print one JSON object per preset instead of text
        #[arg(long)]
        json: bool,
    },
    /// Put the device on one of its built-in settings
    Apply {
        /// The setting's name, as `preset list` shows it
        name: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum HopsCommand {
    /// List the hops in the order the sound takes through them
    List {
        /// Print one JSON object per hop instead of text
        #[arg(long)]
        json: bool,
    },
    /// Declare a piece of equipment the sound goes through
    Add {
        /// The name to pick it by
        name: String,
        /// What it is: amp, headphones, speakers or other
        #[arg(long)]
        kind: String,
        /// A line about it
        #[arg(long)]
        note: Option<String>,
        /// The configured device whose output it follows
        #[arg(long)]
        after: Option<String>,
    },
    /// Link a configured device as a hop
    Link {
        /// The configured name of the device
        device: String,
        /// A line about it
        #[arg(long)]
        note: Option<String>,
        /// The configured device whose output it follows
        #[arg(long)]
        after: Option<String>,
    },
    /// Change a hop; what the flags leave out stays as it is
    Edit {
        /// The hop's id, as `hops list` shows it
        id: String,
        /// Rename it (a declared hop only)
        #[arg(long)]
        name: Option<String>,
        /// What it is: amp, headphones, speakers or other (a declared hop only)
        #[arg(long)]
        kind: Option<String>,
        /// Replace its note
        #[arg(long, conflicts_with = "no_note")]
        note: Option<String>,
        /// Take its note away
        #[arg(long)]
        no_note: bool,
        /// The configured device whose output it follows
        #[arg(long)]
        after: Option<String>,
    },
    /// Take a hop out of the configuration
    Remove {
        /// The hop's id, as `hops list` shows it
        id: String,
    },
    /// Put a hop at another position in the list
    Move {
        /// The hop's id, as `hops list` shows it
        id: String,
        /// Where it goes, counted from 1; past the end puts it last
        position: usize,
    },
}

#[derive(Subcommand)]
pub(crate) enum HostCommand {
    /// An equalizer on what the computer plays
    Eq {
        #[command(subcommand)]
        command: HostEqCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum HostEqCommand {
    /// Put a correction in front of the sink that feeds the DAC and keep it there
    Apply {
        /// A preset's name, or an Equalizer APO parametric EQ file
        target: String,
        /// The sink to put it in front of, by node name or wpctl id; the default output when
        /// left out
        #[arg(long, value_name = "NODE")]
        sink: Option<String>,
        /// Answer yes to the question asked when the preset is on the DAC
        #[arg(long)]
        yes: bool,
    },
    /// Stop the computer EQ and delete what was written for it
    Remove,
    /// Print what is applied and whether it runs
    Status {
        /// Print one JSON object instead of text
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum DaemonCommand {
    /// Print the daemon's unit states, version and connected devices
    Status,
    /// Print the daemon's log
    Logs {
        /// Follow the log instead of printing what is there and exiting
        #[arg(short = 'f', long)]
        follow: bool,
    },
    /// Restart the daemon
    Restart {
        /// Restart only a daemon that is running, and start none
        #[arg(long)]
        if_running: bool,
    },
    /// Stop the daemon
    Stop,
    /// Write the systemd user units and enable them
    Install {
        /// Enable only the socket, so the daemon starts on the first command
        #[arg(long)]
        on_demand: bool,
    },
    /// Take the systemd user units out again
    Uninstall,
}

#[derive(Subcommand)]
pub(crate) enum ProfilesCommand {
    /// List every profile with its product id and firmware floor
    List,
    /// Show one profile's controls
    Show { name: String },
}

#[derive(Subcommand)]
pub(crate) enum McpCommand {
    /// Print the bearer token a client of the MCP server sends, and nothing else
    Token {
        /// Replace the generated token with a fresh one before printing it
        #[arg(long)]
        rotate: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum WebCommand {
    /// Print a six-digit code that signs in one browser within five minutes
    Pair,
    /// Print the token the web UI's sign-in page takes, and nothing else
    Token {
        /// Replace the generated token with a fresh one before printing it
        #[arg(long)]
        rotate: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ConfigCommand {
    /// Print the configuration directory
    Path,
    /// Print the loaded configuration
    Show {
        /// Print the loaded config as one JSON object instead of text
        #[arg(long)]
        json: bool,
    },
    /// Print a configuration file with every key commented out at its default
    Template,
}

#[derive(Subcommand)]
pub(crate) enum PassphraseCommand {
    #[command(about = about::PASSPHRASE_SET)]
    Set {
        /// Four digits, or `none` to clear the passphrase
        code: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    #[command(about = about::PASSPHRASE_CLEAR)]
    Clear {
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

/// The code `passphrase clear` sends, which is the one `passphrase set` clears with: both
/// verbs reach the device through the same call, so what a cleared passphrase does to it
/// cannot depend on which of the two asked.
pub(crate) const CLEAR: &str = "none";

/// Parse a duration written as `5s`, `500ms`, or a bare number of seconds.
fn parse_duration(text: &str) -> Result<Duration, String> {
    let bad =
        || format!("{text:?} is not a duration; try `5s`, `500ms`, or a plain number of seconds");
    let s = text.trim();
    let (number, seconds_per_unit) = if let Some(rest) = s.strip_suffix("ms") {
        (rest, 0.001)
    } else if let Some(rest) = s.strip_suffix('s') {
        (rest, 1.0)
    } else {
        (s, 1.0)
    };
    let value: f64 = number.parse().map_err(|_| bad())?;
    // Reject anything that would make `Duration::from_secs_f64` panic, or that is
    // absurdly large for a CLI timeout.
    const MAX_SECS: f64 = 1e9;
    if !value.is_finite() || value < 0.0 || value * seconds_per_unit > MAX_SECS {
        return Err(bad());
    }
    Ok(Duration::from_secs_f64(value * seconds_per_unit))
}

#[cfg(test)]
mod tests;
