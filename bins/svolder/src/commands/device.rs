// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device-facing verbs: they pick a backend, read or command the device through it, and
//! print what came back.
//!
//! `add` and `remove`, which write the `[[device]]` blocks the rest of them read, live here
//! too: `add` opens a backend of its own to find the device it is naming, so it belongs beside
//! the verbs that open one rather than beside the `config` verbs, which never do.
//!
//! - [`autoeq`]: the AutoEq verbs: `eq search`, and `eq import --autoeq`.
//! - [`mod@device_presets`]: the device's own presets: `preset list` and `preset apply`, and the
//!   slots `set recall` and `set save` send.
//! - [`mod@devices`]: which devices there are: `scan`, `devices`, `add` and `remove`.
//! - [`mod@eq`]: the verbs that read or write the curve: `eq show`, `eq set`, `eq mode`,
//!   `eq flatten`.
//! - [`moves`]: moving a correction between the DAC and the computer.
//! - [`mod@passphrase`]: the one sanctioned path to function 250, asked for on a terminal first.
//! - [`presets`]: the preset verbs: `eq list`, `load`, `save`, `import`, `rename` and `delete`.
//! - [`select`]: finding the device a backend opens, and the profile it is read through.
//! - [`values`]: the verbs that read or write values: `status`, `inspect`, `get`, `set` and `raw`.
//! - [`mod@watch`]: the device's values, then every status it sends until Ctrl-C.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use svolder_ble::BleError;
use svolder_ipc::{Client, Config, ConfigError, Request, Response, ValueObject, socket};

use crate::backend::{Backend, Failure, Globals, WriteError};
use crate::cli::{CLEAR, EqCommand, PassphraseCommand, PresetCommand};
use crate::format::print_value;
use crate::recovery::Kind;

mod autoeq;
mod device_presets;
mod devices;
mod eq;
mod moves;
mod passphrase;
mod presets;
mod select;
mod values;
mod watch;

use autoeq::{eq_import_autoeq, eq_search};
use device_presets::{preset_apply, preset_list};
pub(crate) use devices::found_lines;
pub use devices::{add, devices, remove, scan};
use eq::{eq_flatten, eq_mode, eq_set, eq_show};
pub(crate) use moves::{confirm, flatten as flatten_holders, holders, nothing_changed};
use passphrase::passphrase_set;
use presets::{eq_delete, eq_import, eq_list, eq_load, eq_rename, eq_save};
pub(crate) use presets::{no_such_preset, read_correction, saved_line};
pub(crate) use select::collect_unique;
pub use select::{Selected, profile_for, select_device};
pub use values::{get, inspect, raw, set, status};
pub use watch::watch;

/// Carry out one of the `eq` sub-commands.
pub(crate) async fn eq(g: &Globals<'_>, command: EqCommand) -> Result<ExitCode, String> {
    match command {
        EqCommand::Show { json } => eq_show(g, json).await,
        EqCommand::Set {
            band,
            param,
            value,
            json,
        } => eq_set(g, band, &param, value, json).await,
        EqCommand::Mode { mode, json } => eq_mode(g, &mode, json).await,
        EqCommand::List { json, all } => eq_list(g, json, all).await,
        EqCommand::Load { name, yes } => eq_load(g, &name, yes).await,
        EqCommand::Save { name, force } => eq_save(g, &name, force).await,
        EqCommand::Import {
            file,
            autoeq,
            measured_by,
            name,
            force,
        } => match (file, autoeq) {
            (Some(_), _) if measured_by.is_some() => Ok(Failure::of(
                Kind::Usage,
                "--measured-by goes with --autoeq, which picks a source of AutoEq's",
            )
            .report()),
            (Some(file), _) => eq_import(g, &file, name.as_deref(), force).await,
            (None, Some(headphone)) => {
                eq_import_autoeq(
                    g,
                    &headphone,
                    measured_by.as_deref(),
                    name.as_deref(),
                    force,
                )
                .await
            }
            (None, None) => unreachable!("clap requires one of the two"),
        },
        EqCommand::Search {
            query,
            all,
            refresh,
            json,
        } => eq_search(&query, all, refresh, json).await,
        EqCommand::Rename { old, new } => Ok(eq_rename(g, &old, &new).await),
        EqCommand::Delete { name, yes } => Ok(eq_delete(g, &name, yes).await),
        EqCommand::Flatten => eq_flatten(g).await,
    }
}

/// Carry out one of the `preset` sub-commands.
pub(crate) async fn preset(g: &Globals<'_>, command: PresetCommand) -> Result<ExitCode, String> {
    match command {
        PresetCommand::List { json } => preset_list(g, json).await,
        PresetCommand::Apply { name } => preset_apply(g, &name).await,
    }
}

/// Carry out one of the `passphrase` sub-commands.
pub(crate) async fn passphrase(
    g: &Globals<'_>,
    command: PassphraseCommand,
) -> Result<ExitCode, String> {
    match command {
        PassphraseCommand::Set { code, yes } => passphrase_set(g, &code, yes).await,
        PassphraseCommand::Clear { yes } => passphrase_set(g, CLEAR, yes).await,
    }
}

/// Open the backend a verb runs on, saying whatever it has to say on the way.
///
/// The backend itself prints nothing, so this is where a verb's own boundary is: a warning it
/// kept (a daemon answering for a device whose link is down, or a daemon this process tried to
/// start and could not) is printed before the verb prints anything of its own, and so is the
/// note that the device's configured passphrase does not match the code it advertises. `scan`
/// and the search behind `add` call this too, with `connect: false`, so the notice is said
/// exactly once whether or not the verb goes on to connect.
pub(crate) async fn opened(g: &Globals<'_>, connect: bool) -> Result<Backend, Failure> {
    let backend = opened_keeping_refusal(g, connect).await?;
    // A verb that wants the device to answer has nothing to print from a refused link, and
    // exiting 0 over an empty reading would say it worked: the refusal is its answer, and
    // the same whether a daemon or this process holds the link.
    if connect && let Some(refusal) = backend.refusal() {
        backend.close().await;
        return Err(refusal);
    }
    Ok(backend)
}

/// [`opened`] for a session that is watched: a link the device has refused is kept and reported
/// as a refused link always was, since the session may yet be given the right code.
pub(crate) async fn opened_keeping_refusal(
    g: &Globals<'_>,
    connect: bool,
) -> Result<Backend, Failure> {
    opened_saying(g, connect, false).await
}

/// [`opened_keeping_refusal`] for a verb that prints no readings: it leaves out the note that a
/// down device's readings are the last known.
pub(crate) async fn opened_for_a_cue(g: &Globals<'_>) -> Result<Backend, Failure> {
    opened_saying(g, true, true).await
}

/// The open behind both, with `drop_last_known` leaving out the note about stale readings.
async fn opened_saying(
    g: &Globals<'_>,
    connect: bool,
    drop_last_known: bool,
) -> Result<Backend, Failure> {
    let mut backend = Backend::open(g, connect).await?;
    if drop_last_known {
        backend.drop_last_known_warning();
    }
    for warning in backend.take_warnings() {
        eprintln!("svolder: {warning}");
    }
    // Said before the verb prints anything of its own, so it reads ahead of a refusal it may
    // go with. Best effort: nothing the verb does depends on it.
    if connect && let Some(line) = backend.passphrase_note_line().await {
        eprintln!("svolder: {line}");
    }
    Ok(backend)
}

/// [`opened`] with `connect: true`, turning a failure into the exit code it earns: what every
/// verb that talks to the device once it has a backend uses.
async fn open(g: &Globals<'_>) -> Result<Backend, ExitCode> {
    opened(g, true).await.map_err(Failure::report)
}

/// [`open`] for `watch`, which keeps the link of a device that refuses the passphrase.
async fn open_watched(g: &Globals<'_>) -> Result<Backend, ExitCode> {
    opened_keeping_refusal(g, true)
        .await
        .map_err(Failure::report)
}

/// Print a confirmed reading — or nothing, for a no-wait — and hand back the exit code the
/// outcome earns.
fn report(outcome: Result<Option<ValueObject>, WriteError>, json: bool) -> ExitCode {
    match outcome {
        Ok(Some(vo)) => {
            print_value(&vo, json);
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(e) => e.failure().report(),
    }
}

/// Map a BleError to the CLI's exit code.
pub fn exit_for(err: &BleError) -> ExitCode {
    ExitCode::from(crate::recovery::code_for(err))
}

/// Print every reading, one line or JSON object each.
fn print_values(values: &[ValueObject], json: bool) {
    if json {
        for vo in values {
            print_value(vo, true);
        }
        return;
    }
    for line in value_lines(values) {
        println!("{line}");
    }
}

/// The text lines `inspect` prints for `values`, one status line each.
pub(crate) fn value_lines(values: &[ValueObject]) -> Vec<String> {
    values.iter().map(crate::format::line).collect()
}

/// Tell a daemon that is running to re-read the configuration file, so a device that was just
/// added is held from now on and one that was just removed is let go.
///
/// Nothing is asked of a daemon that is not there and none is started to ask: the file is the
/// record, and a daemon reads it when it starts. `--transport ble` and `fake` name a session
/// this process would open itself, and a run under one of those has no daemon to tell.
pub(crate) async fn reload_daemon(g: &Globals<'_>, config: &Config) {
    if let Some(said) = reload_daemon_quietly(g, config).await {
        eprintln!("svolder: {said}");
    }
}

/// The same, answering with what would have been printed instead of printing it.
///
/// A front end drawing on the alternate screen has nowhere to print: the setup wizard writes
/// devices from under one, so it tells the daemon through here and shows what came back in its
/// own way. Nothing at all comes back where there was no daemon to tell.
pub(crate) async fn reload_daemon_quietly(g: &Globals<'_>, config: &Config) -> Option<String> {
    if matches!(g.transport, "ble" | "fake") {
        return None;
    }
    let path = socket::path::resolve(g.socket, config.socket.as_deref());
    if !svolder_ipc::is_running(&path).await {
        return None;
    }
    let mut client = Client::connect(&path).await.ok()?;
    Some(match client.request(Request::Reload).await {
        Ok(Response::Reloaded { devices }) => format!(
            "the daemon now holds {}",
            match devices.is_empty() {
                true => "no devices".to_string(),
                false => devices.join(", "),
            }
        ),
        Ok(Response::Error { message, .. }) => format!("the daemon did not reload: {message}"),
        Ok(other) => format!("the daemon answered {other:?} rather than reloading"),
        Err(e) => format!("the daemon did not reload: {e}"),
    })
}

/// Tell a running daemon that a preset was written to `path`, when it reads that very file, so
/// its MQTT bridge offers the new list. Nothing is said, and nothing else is done, where there
/// is no daemon, where it reads another file, or where the reload fails; a direct session
/// (`--transport ble` or `fake`) has none to tell.
pub(crate) async fn reload_daemon_for_presets(g: &Globals<'_>, config: &Config, path: &Path) {
    if matches!(g.transport, "ble" | "fake") {
        return;
    }
    let socket = socket::path::resolve(g.socket, config.socket.as_deref());
    if svolder_ipc::is_running(&socket).await {
        crate::backend::reload_if_reads(socket, path).await;
    }
}

/// What a configuration file that could not be read or written earns: its own words, and the
/// step that points at the file they are about.
pub(crate) fn config_failure(e: ConfigError) -> Failure {
    Failure::of(Kind::ConfigUnreadable, e.to_string())
}

/// Put `question` on stderr and read one line from stdin; true only for `y`/`yes`,
/// case-insensitive.
///
/// The question goes to stderr so that a verb whose stdout is being read still asks where the
/// user can see it, and every prompt in this binary is this one: what a yes and a no are
/// cannot depend on which verb asked.
pub fn ask(question: &str) -> std::io::Result<bool> {
    eprint!("{question} ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

#[cfg(test)]
mod tests;
