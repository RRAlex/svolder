// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq verbs: `eq search`, and `eq import --autoeq`, which has the daemon download a
//! headphone's correction and save it as an imported preset.
//!
//! Only `svolderd` reaches AutoEq's repository: these verbs ask it (see [`crate::autoeq`]), and
//! with no daemon, or under `--transport ble` or `fake`, which open none, they say so.

use std::path::PathBuf;
use std::process::ExitCode;

use svolder_ipc::socket::daemon_version::{RESTART_COMMAND, is_config_missing_refusal};
use svolder_ipc::{
    AutoEqGroup, AutoEqMeasurement, AutoEqTarget, AutoEqVariant, Config, FEATURE_AUTOEQ, Reason,
    socket,
};

use crate::autoeq::{self, NEEDS_DAEMON, Refused};
use crate::backend::{Failure, Globals};
use crate::recovery::Kind;

use super::config_failure;

/// The most lines a text search prints before it asks for a narrower one.
const MAX_LINES: usize = 30;

/// The socket of the daemon that serves AutoEq, or why there is none: a run that opens no
/// daemon of its own (`--transport ble` or `fake`), nothing listening, or a daemon that does not
/// have the feature. `what` is the verb's work, for a daemon that is too old to do it.
pub(super) async fn daemon(
    g: &Globals<'_>,
    config: &Config,
    what: &str,
) -> Result<PathBuf, Failure> {
    if matches!(g.transport, "ble" | "fake") {
        return Err(Failure {
            next: "Drop `--transport`, so the verb asks the daemon".into(),
            ..Failure::of(
                Kind::Usage,
                format!(
                    "{NEEDS_DAEMON}: `--transport {}` opens no daemon",
                    g.transport
                ),
            )
        });
    }
    let socket = socket::path::resolve(g.socket, config.socket.as_deref());
    match crate::reach::daemon_runs(&socket, FEATURE_AUTOEQ, what).await? {
        true => Ok(socket),
        false => Err(Failure {
            next: format!(
                "Start it with `{RESTART_COMMAND}`, or import a file with `svolder eq import FILE`"
            ),
            ..Failure::of(
                Kind::NoDaemon,
                format!("{NEEDS_DAEMON}: no daemon answers on {}", socket.display()),
            )
        }),
    }
}

/// What a refused call leaves with: an interrupt (Ctrl-C, a closed terminal) cancelled the job and
/// exits 130 with a line saying so, as the walk does; any other refusal is [`refusal`]'s.
pub(super) fn refusal_exit(refused: &Refused, others: bool) -> ExitCode {
    match refused.interrupted {
        true => {
            eprintln!();
            eprintln!("svolder: {}", refused.message);
            ExitCode::from(130)
        }
        false => refusal(refused, others).report(),
    }
}

/// A refusal of the daemon's AutoEq service, with the step that follows it. `others` says the
/// headphone has measurements from another source, which `--measured-by` can pick.
pub(super) fn refusal(refused: &Refused, others: bool) -> Failure {
    if refused.too_old {
        return Failure {
            next: format!("Run `{RESTART_COMMAND}`"),
            ..Failure::of(Kind::DaemonTooOld, refused.message.clone())
        };
    }
    let mut failure = Failure::of(Kind::Usage, refused.message.clone());
    failure.next = match (refused.reason, others) {
        (Some(Reason::NotListed), true) => "Pick another source with `--measured-by`, or import a \
             file with `svolder eq import FILE`"
            .to_string(),
        (Some(Reason::NotListed), false) => {
            "Import a file with `svolder eq import FILE`".to_string()
        }
        // AutoEq is off: waiting changes nothing.
        (Some(Reason::Unreachable), _)
            if refused.message.starts_with(svolder_autoeq::OFF_PREFIX) =>
        {
            "Import a file with `svolder eq import FILE`".to_string()
        }
        (Some(Reason::Unreachable), _) => {
            "Retry later, or import a file with `svolder eq import FILE`".to_string()
        }
        (Some(Reason::Taken), _) => {
            "Pick another with `--name`, or pass `--force` to replace it".to_string()
        }
        (Some(Reason::BadName), _) => "Pick another with `--name`".to_string(),
        _ if is_config_missing_refusal(&refused.message) => String::new(),
        _ => failure.next,
    };
    failure
}

/// A headphone `headphone` can mean: its name and the measurements that count as its own.
struct Candidate<'a> {
    /// The name the daemon's group gives it.
    name: &'a str,
    /// Its measurements, AutoEq's recommended one first.
    variants: Vec<&'a AutoEqMeasurement>,
}

/// The headphones `headphone` can mean among the `groups` a search for it found, narrowest
/// reading first: the measurements named as the query is written (blanks round it dropped) or,
/// failing that, the same ignoring case; then the headphones whose name reads the same as the
/// query once only letters and digits count; then every headphone the search found. Spellings
/// that differ in case are one headphone, whoever wrote them, and one that differs otherwise
/// (`Melomania 1` and `Melomania 1+`, which the daemon's search lists as one group) is a
/// headphone of its own: a name that is not exact never picks one spelling silently.
/// `measured_by`, when given, drops the headphones that source did not measure, unless that would
/// leave none, so the caller can still say which sources there are.
fn candidates<'a>(
    groups: &'a [AutoEqGroup],
    headphone: &str,
    measured_by: Option<&str>,
) -> Vec<Candidate<'a>> {
    let literal = headphone.trim();
    let folded = literal.to_lowercase();
    // Each group's measurements told apart by the spelling of the name in any case.
    let spellings = |keep: &dyn Fn(&AutoEqGroup) -> bool| -> Vec<Candidate<'a>> {
        let mut found: Vec<Candidate<'a>> = Vec::new();
        for group in groups.iter().filter(|group| keep(group)) {
            let first = found.len();
            for m in &group.variants {
                let spelled = m.variant.name.trim().to_lowercase();
                match found[first..]
                    .iter_mut()
                    .find(|candidate| candidate.name.trim().to_lowercase() == spelled)
                {
                    Some(candidate) => candidate.variants.push(m),
                    None => found.push(Candidate {
                        name: &m.variant.name,
                        variants: vec![m],
                    }),
                }
            }
        }
        found
    };
    let mut found: Vec<Candidate<'a>> = spellings(&|group| {
        group
            .variants
            .iter()
            .any(|m| m.variant.name.trim() == literal)
    });
    if found.is_empty() {
        found = spellings(&|group| {
            group
                .variants
                .iter()
                .any(|m| m.variant.name.trim().to_lowercase() == folded)
        });
    }
    // Only the spelling asked for, with its case variants, is the headphone.
    found.retain(|candidate| {
        let name = candidate.name.trim();
        name == literal || name.to_lowercase() == folded
    });
    if found.is_empty() {
        // A name that is exactly the query wins over longer names that hold it.
        let plain: String = headphone
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        found = spellings(&|group| group.key == plain);
    }
    if found.is_empty() {
        found = spellings(&|_| true);
    }
    let Some(source) = measured_by else {
        return found;
    };
    let measured: Vec<Candidate<'a>> = found
        .iter()
        .filter(|candidate| {
            candidate
                .variants
                .iter()
                .any(|m| m.variant.source.eq_ignore_ascii_case(source.trim()))
        })
        .map(|candidate| Candidate {
            name: candidate.name,
            variants: candidate.variants.clone(),
        })
        .collect();
    match measured.is_empty() {
        true => found,
        false => measured,
    }
}

/// `eq search QUERY… [--all] [--refresh] [--json]`: one line per headphone, with the
/// recommended measurement and how many others there are, or per measurement with `all`.
/// With no query, `refresh` only refreshes the index.
pub async fn eq_search(
    g: &Globals<'_>,
    query: &[String],
    all: bool,
    refresh: bool,
    json: bool,
) -> Result<ExitCode, String> {
    let config = match Config::load() {
        Ok(config) => config,
        Err(e) => return Ok(config_failure(e).report()),
    };
    let query = query.join(" ");
    if query.trim().is_empty() && !refresh {
        let failure = Failure::of(
            Kind::Usage,
            "give a headphone name to search for".to_string(),
        );
        return Ok(Failure {
            next: "Run `svolder eq search HD 650`, or `svolder eq search --refresh` to fetch the \
                   index again"
                .into(),
            ..failure
        }
        .report());
    }
    let socket = match daemon(g, &config, "search AutoEq").await {
        Ok(socket) => socket,
        Err(failure) => return Ok(failure.report()),
    };
    let found = match autoeq::search(&socket, &query, refresh).await {
        Ok(found) => found,
        Err(refused) => return Ok(refusal(&refused, false).report()),
    };
    if let Some(note) = &found.note {
        eprintln!("svolder: {note}");
    }
    let refresh_failed = refresh && found.note.is_some();
    if query.trim().is_empty() {
        if found.note.is_none() {
            eprintln!(
                "svolder: refreshed the AutoEq index ({} measurements)",
                found.measurements
            );
            return Ok(ExitCode::SUCCESS);
        }
        return Ok(outcome(true));
    }
    if found.groups.is_empty() {
        let failure = Failure::of(Kind::Usage, format!("no headphone matches {query:?}"));
        return Ok(Failure {
            next: "Try fewer words; `hd650` finds `HD 650`".into(),
            ..failure
        }
        .report());
    }
    let rows: Vec<(&svolder_ipc::AutoEqMeasurement, usize)> = match all {
        true => found
            .groups
            .iter()
            .flat_map(|group| group.variants.iter().map(|m| (m, 0)))
            .collect(),
        false => found
            .groups
            .iter()
            .filter_map(|group| Some((group.variants.first()?, group.variants.len() - 1)))
            .collect(),
    };
    if json {
        for (m, more) in &rows {
            let row = serde_json::json!({
                "name": m.variant.name,
                "source": m.variant.source,
                "form": m.variant.form,
                "rig": m.variant.rig,
                "more": more,
            });
            println!("{row}");
        }
        return Ok(outcome(refresh_failed));
    }
    let width = rows
        .iter()
        .take(MAX_LINES)
        .map(|(m, _)| m.variant.name.chars().count())
        .max()
        .unwrap_or(0);
    for (m, more) in rows.iter().take(MAX_LINES) {
        let name = &m.variant.name;
        let pad = " ".repeat(width - name.chars().count());
        let words = &m.words;
        match more {
            0 => println!("{name}{pad}  {words}"),
            more => println!("{name}{pad}  {words}  +{more} more"),
        }
    }
    // Rows past the cut are measurements with `--all` and headphones without it; the daemon's own
    // cut (`more`) is always headphones.
    let left = rows.len().saturating_sub(MAX_LINES) + if all { 0 } else { found.more };
    if left > 0 {
        println!("… {left} more: narrow the search");
    }
    if all && found.more > 0 {
        println!("… {} more headphones: narrow the search", found.more);
    }
    Ok(outcome(refresh_failed))
}

/// Success, or the failure of a `--refresh` that fell back on the cached index (the note about
/// it is already on stderr).
fn outcome(refresh_failed: bool) -> ExitCode {
    match refresh_failed {
        true => Failure {
            next: "Retry later; the cached index stays in use".into(),
            ..Failure::of(
                Kind::Usage,
                "the AutoEq index could not be refreshed".to_string(),
            )
        }
        .report(),
        false => ExitCode::SUCCESS,
    }
}

/// `eq import --autoeq HEADPHONE [--measured-by SOURCE] [--name NAME] [--force]`: find the one
/// headphone `headphone` names, have the daemon download the correction of the measurement
/// `measured_by` picks (the recommended one by default) and save it like a file import, with the
/// origin AutoEq gives it. Several headphones, or a source that did not measure it, are listed
/// with the usage code.
///
/// The preset goes to the library, which no side of the EQ owns; the daemon is told the computer
/// as the side, which only bounds a correction to the filters the computer takes.
pub async fn eq_import_autoeq(
    g: &Globals<'_>,
    headphone: &str,
    measured_by: Option<&str>,
    name: Option<&str>,
    force: bool,
) -> Result<ExitCode, String> {
    let path = Config::path();
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(e) => return Ok(config_failure(e).report()),
    };
    let socket = match daemon(g, &config, "import from AutoEq").await {
        Ok(socket) => socket,
        Err(failure) => return Ok(failure.report()),
    };
    let found = match autoeq::search(&socket, headphone, false).await {
        Ok(found) => found,
        Err(refused) => return Ok(refusal(&refused, false).report()),
    };
    if let Some(note) = &found.note {
        eprintln!("svolder: {note}");
    }
    let groups = candidates(&found.groups, headphone, measured_by);
    let group = match groups.as_slice() {
        [] => {
            let failure = Failure::of(Kind::Usage, format!("no headphone matches {headphone:?}"));
            return Ok(Failure {
                next: "Look for it with `svolder eq search`".into(),
                ..failure
            }
            .report());
        }
        [only] => only,
        many => {
            let mut what = format!("{headphone:?} matches {} headphones:", many.len());
            for group in many.iter().take(MAX_LINES) {
                what.push_str(&format!("\n  {}", group.name));
            }
            if many.len() > MAX_LINES {
                what.push_str(&format!("\n  … {} more", many.len() - MAX_LINES));
            }
            let failure = Failure::of(Kind::Usage, what);
            return Ok(Failure {
                next: "Give the full name of one".into(),
                ..failure
            }
            .report());
        }
    };
    let measurement = match measured_by {
        None => group.variants.first().copied(),
        Some(source) => group
            .variants
            .iter()
            .copied()
            .find(|m| m.variant.source.eq_ignore_ascii_case(source.trim())),
    };
    let Some(measurement) = measurement else {
        let mut sources: Vec<&str> = Vec::new();
        for m in &group.variants {
            if !sources.contains(&m.variant.source.as_str()) {
                sources.push(&m.variant.source);
            }
        }
        let failure = Failure::of(
            Kind::Usage,
            format!(
                "{:?} has no measurement of {}; the sources are {}",
                measured_by.unwrap_or_default(),
                group.name,
                sources.join(", ")
            ),
        );
        return Ok(Failure {
            next: "Pick one with `--measured-by`".into(),
            ..failure
        }
        .report());
    };
    let others = group
        .variants
        .iter()
        .any(|other| other.variant.source != measurement.variant.source);
    let variant: AutoEqVariant = measurement.variant.clone();
    // Ctrl-C (and a closed terminal) cancels this job on the daemon while it is waited for.
    let signals = crate::wizard::signals::Signals::listen();
    let saved = match autoeq::import(
        &socket,
        variant,
        name.map(str::to_string),
        force,
        AutoEqTarget::Computer,
        Some(signals.presses()),
    )
    .await
    {
        Ok(saved) => saved,
        Err(refused) => return Ok(refusal_exit(&refused, others)),
    };
    super::presets::report_saved(&path, &saved);
    Ok(ExitCode::SUCCESS)
}
