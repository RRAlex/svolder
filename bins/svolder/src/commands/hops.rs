// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `hops` verbs: list, declare, link, change, remove and move what the sound goes through
//! after the DAC.
//!
//! They work on the `[[hop]]` blocks of the configuration file and open neither a Bluetooth
//! transport nor a daemon connection. A daemon that is running and reads the file is told to
//! reload after each write.

use std::path::Path;
use std::process::ExitCode;

use svolder_ipc::Config;
use svolder_ipc::config::{Hop, HopKind, HopWhat};
use svolder_profiles::Family;
use svolder_profiles::tone::preset::same_name;

use crate::backend::{Failure, Globals};
use crate::cli::HopsCommand;
use crate::commands::device::{config_failure, reload_daemon_for_presets};
use crate::recovery::Kind;

/// Carry out one of the `hops` sub-commands; a bare `hops` lists.
pub(crate) async fn run(g: &Globals<'_>, json: bool, command: Option<HopsCommand>) -> ExitCode {
    let outcome = match command {
        None => list(json),
        Some(HopsCommand::List { json }) => list(json),
        Some(HopsCommand::Add {
            name,
            kind,
            note,
            after,
        }) => add(g, name, &kind, note, after).await,
        Some(HopsCommand::Link {
            device,
            note,
            after,
        }) => link(g, &device, note, after).await,
        Some(HopsCommand::Edit {
            id,
            name,
            kind,
            note,
            no_note,
            after,
        }) => {
            let change = Change {
                name,
                kind,
                note: match (note, no_note) {
                    (Some(note), _) => Some(Some(note)),
                    (None, true) => Some(None),
                    (None, false) => None,
                },
                after,
            };
            edit(g, &id, change).await
        }
        Some(HopsCommand::Remove { id }) => remove(g, &id).await,
        Some(HopsCommand::Move { id, position }) => move_to(g, &id, position).await,
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => failure.report(),
    }
}

/// What `edit` was asked to change; every field left `None` stays as it is, and a note of
/// `Some(None)` is taken away.
struct Change {
    name: Option<String>,
    kind: Option<String>,
    note: Option<Option<String>>,
    after: Option<String>,
}

/// One hop as `list --json` prints it, one object a line.
#[derive(serde::Serialize)]
struct Row<'a> {
    id: &'a str,
    kind: &'static str,
    name: Option<&'a str>,
    note: Option<&'a str>,
    after: Option<&'a str>,
    device: Option<&'a str>,
    live: bool,
}

/// The configuration at its path, and the path.
fn loaded() -> Result<(Config, std::path::PathBuf), Failure> {
    let path = Config::path();
    let config = Config::load_from(&path).map_err(config_failure)?;
    Ok((config, path))
}

/// The failure for a hop no block carries.
fn no_such_hop(id: &str) -> Failure {
    Failure::of(
        Kind::Usage,
        format!("no hop named {id:?} is configured; `svolder hops` lists them"),
    )
}

/// The kind of `word`, or the failure that lists the words.
fn kind_of(word: &str) -> Result<HopKind, Failure> {
    HopKind::named(word).ok_or_else(|| {
        Failure::of(
            Kind::Usage,
            format!("kind {word:?} is not one of {}", HopKind::words()),
        )
    })
}

/// Print the hops, as text or one JSON object a line.
fn list(json: bool) -> Result<(), Failure> {
    let (config, _) = loaded()?;
    crate::backend::print_warnings(&config);
    let registry = match config
        .hops
        .iter()
        .any(|hop| matches!(hop.what, HopWhat::Linked { .. }))
    {
        true => Some(crate::registry().map_err(|why| Failure::of(Kind::ConfigUnreadable, why))?),
        false => None,
    };
    let mut lines = Vec::with_capacity(config.hops.len());
    for (at, hop) in config.hops.iter().enumerate() {
        let device = match &hop.what {
            HopWhat::Linked { device } => config
                .devices
                .iter()
                .find(|configured| same_name(&configured.name, device)),
            HopWhat::Declared { .. } => None,
        };
        let kind = match (&hop.what, device) {
            (HopWhat::Declared { kind, .. }, _) => *kind,
            (HopWhat::Linked { .. }, Some(device)) => device
                .profile
                .as_deref()
                .and_then(|profile| registry.as_ref()?.get(profile))
                .filter(|profile| profile.family == Family::Amp)
                .map_or(HopKind::Other, |_| HopKind::Amp),
            (HopWhat::Linked { .. }, None) => HopKind::Other,
        };
        let row = Row {
            id: hop.id(),
            kind: kind.name(),
            name: Some(hop.id()),
            note: hop.note.as_deref(),
            after: hop.after.as_deref(),
            device: device
                .map(|device| device.name.as_str())
                .or(match &hop.what {
                    HopWhat::Linked { device } => Some(device.as_str()),
                    HopWhat::Declared { .. } => None,
                }),
            live: device.is_some(),
        };
        lines.push(match json {
            true => serde_json::to_string(&row).map_err(|e| {
                Failure::of(Kind::ConfigUnreadable, format!("the hops as JSON: {e}"))
            })?,
            false => text_line(at + 1, &row, matches!(hop.what, HopWhat::Linked { .. })),
        });
    }
    if lines.is_empty() && !json {
        eprintln!("svolder: no hops; `svolder hops add --help` declares one");
    }
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

/// The line `list` prints for `row`, the hop at `position` (from 1).
fn text_line(position: usize, row: &Row<'_>, linked: bool) -> String {
    let what = match (linked, row.live) {
        (false, _) => row.kind.to_string(),
        (true, true) => format!("linked {}", row.kind),
        (true, false) => "linked, no such device".to_string(),
    };
    let mut line = format!("{position}. {} ({what})", row.id);
    if let Some(after) = row.after {
        line.push_str(&format!(" after {after}"));
    }
    if let Some(note) = row.note {
        line.push_str(&format!(": {note}"));
    }
    line
}

/// Declare a piece of equipment.
async fn add(
    g: &Globals<'_>,
    name: String,
    kind: &str,
    note: Option<String>,
    after: Option<String>,
) -> Result<(), Failure> {
    let kind = kind_of(kind)?;
    let (config, path) = loaded()?;
    let hop = Hop {
        what: HopWhat::Declared { kind, name },
        note,
        after,
    };
    Config::add_hop(&path, &hop).map_err(config_failure)?;
    written(
        g,
        &config,
        &path,
        &format!("added hop {:?}", hop.id().trim()),
    )
    .await;
    Ok(())
}

/// Link a configured device.
async fn link(
    g: &Globals<'_>,
    device: &str,
    note: Option<String>,
    after: Option<String>,
) -> Result<(), Failure> {
    let (config, path) = loaded()?;
    let Some(configured) = config
        .devices
        .iter()
        .find(|configured| same_name(&configured.name, device))
    else {
        let names: Vec<&str> = config.devices.iter().map(|d| d.name.as_str()).collect();
        let known = match names.is_empty() {
            true => "none is configured".to_string(),
            false => format!("the configured devices are {}", names.join(", ")),
        };
        return Err(Failure::of(
            Kind::Usage,
            format!("no configured device is named {device:?}; {known}"),
        ));
    };
    let hop = Hop {
        what: HopWhat::Linked {
            device: configured.name.clone(),
        },
        note,
        after,
    };
    Config::add_hop(&path, &hop).map_err(config_failure)?;
    written(
        g,
        &config,
        &path,
        &format!("linked {:?} as a hop", configured.name),
    )
    .await;
    Ok(())
}

/// Change the hop `id`: what `change` names, and nothing else.
async fn edit(g: &Globals<'_>, id: &str, change: Change) -> Result<(), Failure> {
    let (config, path) = loaded()?;
    let old = config.hop(id).ok_or_else(|| no_such_hop(id))?;
    let what = match (&old.what, change.name, change.kind) {
        (HopWhat::Declared { kind, name }, new_name, new_kind) => HopWhat::Declared {
            kind: match new_kind {
                Some(word) => kind_of(&word)?,
                None => *kind,
            },
            name: new_name.unwrap_or_else(|| name.clone()),
        },
        (HopWhat::Linked { device }, None, None) => HopWhat::Linked {
            device: device.clone(),
        },
        (HopWhat::Linked { device }, _, _) => {
            return Err(Failure::of(
                Kind::Usage,
                format!(
                    "{device:?} links a device, which gives it its name and kind; \
                     remove the hop and add it declared to name it"
                ),
            ));
        }
    };
    let hop = Hop {
        what,
        note: match change.note {
            Some(note) => note,
            None => old.note.clone(),
        },
        after: change.after.or_else(|| old.after.clone()),
    };
    Config::update_hop(&path, old.id(), &hop).map_err(config_failure)?;
    written(
        g,
        &config,
        &path,
        &format!("changed hop {:?}", hop.id().trim()),
    )
    .await;
    Ok(())
}

/// Take the hop `id` out.
async fn remove(g: &Globals<'_>, id: &str) -> Result<(), Failure> {
    let (config, path) = loaded()?;
    let id = config.hop(id).map(Hop::id).ok_or_else(|| no_such_hop(id))?;
    Config::remove_hop(&path, id).map_err(config_failure)?;
    written(g, &config, &path, &format!("removed hop {id:?}")).await;
    Ok(())
}

/// Put the hop `id` at `position`, counted from 1.
async fn move_to(g: &Globals<'_>, id: &str, position: usize) -> Result<(), Failure> {
    let Some(to) = position.checked_sub(1) else {
        return Err(Failure::of(
            Kind::Usage,
            "positions count from 1; `svolder hops` numbers the hops",
        ));
    };
    let (config, path) = loaded()?;
    let id = config.hop(id).map(Hop::id).ok_or_else(|| no_such_hop(id))?;
    Config::move_hop(&path, id, to).map_err(config_failure)?;
    written(
        g,
        &config,
        &path,
        &format!("moved hop {id:?} to {position}"),
    )
    .await;
    Ok(())
}

/// Say what was written to `path`, and tell a running daemon that reads that file to reload.
async fn written(g: &Globals<'_>, config: &Config, path: &Path, what: &str) {
    eprintln!("svolder: {what} in {}", path.display());
    reload_daemon_for_presets(g, config, path).await;
}
