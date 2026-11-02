// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq verbs: `eq search`, and `eq import --autoeq`, which downloads a headphone's
//! correction and saves it as an imported preset. Config and network only, never a device.

use std::process::ExitCode;

use svolder_autoeq::{AutoEqError, Client, Entry, Index, Refresh, exact, search};
use svolder_ipc::Config;
use svolder_ipc::config::{RefreshWeeks, cache_home};
use svolder_profiles::tone::apo;
use svolder_profiles::tone::preset::{self, Imported};

use crate::backend::{Failure, Globals};
use crate::recovery::Kind;

use super::config_failure;
use super::presets::import_and_report;

/// The most lines a text search prints before it asks for a narrower one.
const MAX_LINES: usize = 30;

/// The client the verbs use: `SVOLDER_AUTOEQ_URL` or AutoEq's repository, cached under the
/// cache home's `svolder/autoeq`.
fn client() -> Client {
    Client::from_env(cache_home().join("svolder").join("autoeq"))
}

/// A failure of the network side, with the step that follows it. `others` says the headphone
/// has measurements from another source, which `--measured-by` can pick.
fn network_failure(error: &AutoEqError, others: bool) -> Failure {
    let mut failure = Failure::of(Kind::Usage, error.to_string());
    failure.next = match (error, others) {
        (AutoEqError::Status { status: 404, .. }, true) => {
            "That measurement is listed but its file is not there: pick another source with \
             `--measured-by`, or import a file with `svolder eq import FILE`"
        }
        (AutoEqError::Status { status: 404, .. }, false) => {
            "That measurement is listed but its file is not there: import a file with \
             `svolder eq import FILE`"
        }
        _ => "Retry later, or import a file with `svolder eq import FILE`",
    }
    .to_string();
    failure
}

/// The index, fetched or read from the cache as the config's `[autoeq] refresh_weeks` says, or
/// at once when `refresh`. A refresh that failed with a cached copy in hand is said on stderr.
async fn load_index(config: &Config, refresh: bool) -> Result<Index, Failure> {
    let policy = match (refresh, config.autoeq.refresh_weeks) {
        (true, _) => Refresh::Now,
        (false, RefreshWeeks::Never) => Refresh::Never,
        (false, RefreshWeeks::Weeks(weeks)) => Refresh::AfterWeeks(weeks),
    };
    let client = client();
    let index = tokio::task::spawn_blocking(move || client.index(policy))
        .await
        .map_err(|e| Failure::of(Kind::Usage, format!("the index fetch stopped: {e}")))?
        .map_err(|e| network_failure(&e, false))?;
    if let Some(note) = &index.note {
        eprintln!("svolder: {note}");
    }
    Ok(index)
}

/// `source · form[ · rig]`: what tells two measurements of one headphone apart.
fn measurement(entry: &Entry) -> String {
    let origin = entry.origin();
    origin
        .strip_prefix("AutoEq · ")
        .unwrap_or(&origin)
        .to_string()
}

/// The matches of `query` grouped by headphone name, each group in AutoEq's order. Names that
/// are equal ignoring case are one headphone, and their measurements share a group.
fn grouped(matches: Vec<&Entry>) -> Vec<Vec<&Entry>> {
    let mut groups: Vec<Vec<&Entry>> = Vec::new();
    let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for entry in matches {
        match at.entry(entry.name.to_lowercase()) {
            std::collections::hash_map::Entry::Occupied(found) => groups[*found.get()].push(entry),
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(groups.len());
                groups.push(vec![entry]);
            }
        }
    }
    groups
}

/// The headphones `headphone` can mean, as groups of measurements, narrowest reading first:
/// the literal name (trimmed, case-sensitive, then trimmed, ignoring case), then the names that
/// read the same as the query once only letters and digits count, then the names that hold it.
/// `measured_by`, when given, drops the groups that source did not measure, unless that would
/// leave none, so the caller can still say which sources there are.
fn candidates<'a>(
    entries: &'a [Entry],
    headphone: &str,
    measured_by: Option<&str>,
) -> Vec<Vec<&'a Entry>> {
    let literal = headphone.trim();
    let mut found: Vec<&'a Entry> = entries
        .iter()
        .filter(|entry| entry.name.trim() == literal)
        .collect();
    if found.is_empty() {
        let folded = literal.to_lowercase();
        found = entries
            .iter()
            .filter(|entry| entry.name.trim().to_lowercase() == folded)
            .collect();
    }
    if !found.is_empty() {
        // The literal step only picks which headphone: every spelling of its name that differs
        // in case is the same headphone, and its measurements all count.
        let folded = found[0].name.trim().to_lowercase();
        found = entries
            .iter()
            .filter(|entry| entry.name.trim().to_lowercase() == folded)
            .collect();
    }
    if found.is_empty() {
        // A name that is exactly the query wins over longer names that hold it.
        found = exact(entries, headphone);
    }
    if found.is_empty() {
        found = search(entries, headphone);
    }
    let groups = grouped(found);
    let Some(source) = measured_by else {
        return groups;
    };
    let measured: Vec<Vec<&Entry>> = groups
        .iter()
        .filter(|group| {
            group
                .iter()
                .any(|entry| entry.source.eq_ignore_ascii_case(source.trim()))
        })
        .cloned()
        .collect();
    match measured.is_empty() {
        true => groups,
        false => measured,
    }
}

/// `eq search QUERY… [--all] [--refresh] [--json]`: one line per headphone, with the
/// recommended measurement and how many others there are, or per measurement with `all`.
/// With no query, `refresh` only refreshes the index.
pub async fn eq_search(
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
    let index = match load_index(&config, refresh).await {
        Ok(index) => index,
        Err(failure) => return Ok(failure.report()),
    };
    if query.trim().is_empty() {
        let headphones = grouped(index.entries.iter().collect()).len();
        if index.note.is_none() {
            eprintln!("svolder: refreshed the AutoEq index ({headphones} headphones)");
            return Ok(ExitCode::SUCCESS);
        }
        return Ok(outcome(true));
    }
    let found = search(&index.entries, &query);
    if found.is_empty() {
        let failure = Failure::of(Kind::Usage, format!("no headphone matches {query:?}"));
        return Ok(Failure {
            next: "Try fewer words; `hd650` finds `HD 650`".into(),
            ..failure
        }
        .report());
    }
    let groups = grouped(found);
    let rows: Vec<(&Entry, usize)> = match all {
        true => groups
            .iter()
            .flat_map(|group| group.iter().map(|entry| (*entry, 0)))
            .collect(),
        false => groups
            .iter()
            .map(|group| (group[0], group.len() - 1))
            .collect(),
    };
    if json {
        for (entry, more) in &rows {
            let row = serde_json::json!({
                "name": entry.name,
                "source": entry.source,
                "form": entry.form,
                "rig": entry.rig,
                "more": more,
            });
            println!("{row}");
        }
        return Ok(outcome(refresh && index.note.is_some()));
    }
    let width = rows
        .iter()
        .take(MAX_LINES)
        .map(|(entry, _)| entry.name.chars().count())
        .max()
        .unwrap_or(0);
    for (entry, more) in rows.iter().take(MAX_LINES) {
        let name = &entry.name;
        let pad = " ".repeat(width - name.chars().count());
        match more {
            0 => println!("{name}{pad}  {}", measurement(entry)),
            more => println!("{name}{pad}  {}  +{more} more", measurement(entry)),
        }
    }
    if rows.len() > MAX_LINES {
        println!("… {} more: narrow the search", rows.len() - MAX_LINES);
    }
    Ok(outcome(refresh && index.note.is_some()))
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
/// headphone `headphone` names, download the correction of the measurement `measured_by` picks
/// (the recommended one by default) and save it like a file import, with the origin AutoEq
/// gives it. Several headphones, or a source that did not measure it, are listed with the usage
/// code.
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
    let index = match load_index(&config, false).await {
        Ok(index) => index,
        Err(failure) => return Ok(failure.report()),
    };
    let groups = candidates(&index.entries, headphone, measured_by);
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
                what.push_str(&format!("\n  {}", group[0].name));
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
    let entry = match measured_by {
        None => group[0],
        Some(source) => match group
            .iter()
            .find(|entry| entry.source.eq_ignore_ascii_case(source.trim()))
        {
            Some(entry) => *entry,
            None => {
                let mut sources: Vec<&str> = Vec::new();
                for entry in group.iter() {
                    if !sources.contains(&entry.source.as_str()) {
                        sources.push(&entry.source);
                    }
                }
                let failure = Failure::of(
                    Kind::Usage,
                    format!(
                        "{source:?} has no measurement of {}; the sources are {}",
                        group[0].name,
                        sources.join(", ")
                    ),
                );
                return Ok(Failure {
                    next: "Pick one with `--measured-by`".into(),
                    ..failure
                }
                .report());
            }
        },
    };
    let others = group.iter().any(|other| other.source != entry.source);
    let client = client();
    let wanted = entry.clone();
    let text = tokio::task::spawn_blocking(move || client.download(&wanted))
        .await
        .map_err(|e| format!("the download stopped: {e}"))?;
    let text = match text {
        Ok(text) => text,
        Err(error) => return Ok(network_failure(&error, others).report()),
    };
    let parsed = match apo::parse(&text) {
        Ok(parsed) => parsed,
        Err(e) => {
            let what = format!("the file for {} is not a parametric EQ: {e}", entry.name);
            return Ok(Failure::of(Kind::Usage, what).report());
        }
    };
    let named = match name {
        Some(name) => name.to_string(),
        None => preset::default_name(&entry.name),
    };
    let skipped = parsed.notes.len();
    let imported = Imported {
        origin: Some(entry.origin()),
        correction: parsed.correction,
    };
    Ok(import_and_report(g, &config, &path, &named, imported, skipped, force).await)
}
