// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset verbs: `eq list`, `eq load` and `eq save`, which reach the device, and
//! `eq import`, `eq rename` and `eq delete`, which only edit the configuration file.

use std::io::{IsTerminal, Read};
use std::path::Path;
use std::process::ExitCode;

use svolder_ipc::{Config, ValueObject};
use svolder_profiles::tone::apo;
use svolder_profiles::tone::preset::{self, Imported, NameRefusal, Preset};
use svolder_profiles::{EqSpec, Profile, ProfileError};

use crate::backend::{Backend, Failure, Globals};
use crate::recovery::Kind;

use super::device_presets;
use super::eq::{current_raw, partial_failure_line, send_plan, write_count};
use super::{ask, config_failure, open, opened, reload_daemon_for_presets};

/// List every preset the configuration and the built-ins offer, one line or JSON object each,
/// built-ins first. A text listing with a device that is known shows the presets that fit its
/// band count and counts the others, unless `all` asks for them; with none known it groups the
/// presets under a heading for each band count.
///
/// No session of this process's own is ever opened, so browsing the library never waits on a
/// device. Every preset is listed by name and band count; the built-ins are named through
/// [`svolder_profiles::tone::preset::builtin_names`], since their values need a device's profile to
/// compute. A device's profile marks each preset that fits its band count, and a daemon that
/// holds the device marks, among those, the one that is on from the readings it holds.
///
/// Without a daemon (none answering, or `--transport ble` or `fake`, which never ask one) the
/// profile is the one the configured device names under `profile`, nothing is marked `on`, and
/// a text listing ends with the verb that says which preset is on. A daemon that answers but
/// cannot give the device's readings leaves the listing by name alone.
pub async fn eq_list(g: &Globals<'_>, json: bool, all: bool) -> Result<ExitCode, String> {
    if let Err(failure) = crate::backend::known_transport(g.transport) {
        return Ok(failure.report());
    }
    let config = Config::load().map_err(|e| e.to_string())?;
    crate::backend::print_warnings(&config);
    if crate::backend::asks_a_daemon(g.transport)
        && crate::backend::launch::probe(g, &config).await.is_some()
    {
        return match opened(g, true).await {
            Ok(mut backend) => {
                let outcome = eq_list_with_backend(&mut backend, &config, (json, all)).await;
                backend.close().await;
                outcome
            }
            // A daemon that has no device to answer for leaves the library worth showing from
            // the configuration, as with no daemon at all; `eq list` says nothing about why,
            // since browsing presets never needed a device to begin with.
            Err(_) => {
                let eq = configured_eq(g, &config);
                print_preset_list(
                    &config,
                    eq.as_ref().map(|eq| (eq, None)),
                    (json, all),
                    false,
                )
            }
        };
    }
    let eq = configured_eq(g, &config);
    print_preset_list(&config, eq.as_ref().map(|eq| (eq, None)), (json, all), true)
}

/// The profile of the configured device this call names, read without connecting: the one the
/// configuration names, else the one the state file recorded when the device last connected.
///
/// `None` when neither is there, or when the name they give is no profile, which is said on
/// standard error with `consequence`, what the verb leaves undone for it.
pub(super) fn configured_profile(
    g: &Globals<'_>,
    config: &Config,
    consequence: &str,
) -> Option<Profile> {
    let device = crate::backend::choose_configured(config, g.device)?;
    let name = device
        .profile
        .clone()
        .or_else(|| crate::state::DeviceState::load().last_profile(&device.udid))?;
    let registry = crate::backend::registry().ok()?;
    let Some(profile) = registry.get(&name) else {
        eprintln!("svolder: profile {name:?} is unknown; {consequence}");
        return None;
    };
    Some(profile.clone())
}

/// The tone control of the configured device this call names, read without connecting.
///
/// A device with no tone control but presets of its own is pointed to `preset list`, on standard
/// error, since the library listed is not what it can be put on.
fn configured_eq(g: &Globals<'_>, config: &Config) -> Option<EqSpec> {
    let profile = configured_profile(g, config, "fits not marked")?;
    point_to_device_presets(&profile);
    profile.eq
}

/// Say, on standard error, where the presets of a device with no tone control but presets of its
/// own are listed. Nothing is said of a device that has a tone control, or has no presets.
fn point_to_device_presets(profile: &Profile) {
    if profile.eq.is_none() && svolder_profiles::device_presets::offered(profile) {
        eprintln!(
            "svolder: {}; {}",
            ProfileError::NoEq,
            device_presets::POINTER
        );
    }
}

/// [`eq_list`]'s daemon half: read the current curve and mark every preset against it.
async fn eq_list_with_backend(
    backend: &mut Backend,
    config: &Config,
    (json, all): (bool, bool),
) -> Result<ExitCode, String> {
    let values = match backend.state().await {
        Ok(values) => values,
        Err(failure) => return Ok(failure.report()),
    };
    point_to_device_presets(backend.profile());
    let Some(eq) = backend.profile().eq.clone() else {
        return print_preset_list(config, None, (json, all), false);
    };
    print_preset_list(
        config,
        Some((&eq, Some(values.as_slice()))),
        (json, all),
        false,
    )
}

/// The device `eq list` marks presets against: its tone control, and the readings that say
/// which preset is on when there are any.
type Marked<'a> = (&'a EqSpec, Option<&'a [ValueObject]>);

/// One row of `eq list`: a preset's name, its band count (0 for an imported preset, which fits
/// any), whether it is a built-in or imported, and, with a device, whether it fits that
/// device's tone control and whether it is the one that is on.
#[derive(serde::Serialize)]
struct PresetRow {
    name: String,
    bands: u8,
    built_in: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    imported: bool,
    fits: Option<bool>,
    on: Option<bool>,
}

impl PresetRow {
    /// The marks that follow a row's name: whichever of `built-in`, `fits` and `on` apply.
    fn marks(&self) -> Vec<&'static str> {
        let mut words = Vec::new();
        if self.built_in {
            words.push("built-in");
        }
        if self.imported {
            words.push("imported");
        }
        if self.fits == Some(true) {
            words.push("fits");
        }
        if self.on == Some(true) {
            words.push("on");
        }
        words
    }

    /// The heading a row is listed under without a device: its band count, or `None` for the
    /// imported presets, which have none.
    fn group(&self) -> Option<u8> {
        (!self.imported).then_some(self.bands)
    }

    /// The text line of a row listed on its own: the name padded to `width`, the band count
    /// (an imported preset has none), then the marks.
    fn line(&self, width: usize) -> String {
        let mut words = vec![format!("{:<width$}", self.name)];
        if !self.imported {
            words.push(format!("{} bands", self.bands));
        }
        words.extend(self.marks().into_iter().map(str::to_string));
        words.join("  ").trim_end().to_string()
    }

    /// The text line of a row listed under its band count's heading, or the `imported` one:
    /// indented, the name padded to `width`, then the marks the heading does not say.
    fn grouped_line(&self, width: usize) -> String {
        let mut words = vec![format!("{:<width$}", self.name)];
        words.extend(
            self.marks()
                .into_iter()
                .filter(|mark| *mark != "imported")
                .map(str::to_string),
        );
        format!("  {}", words.join("  ").trim_end())
    }
}

/// What a text `eq list` made without a daemon ends with: the readings that say which preset is
/// on are the daemon's, or `eq show`'s.
const ON_HINT: &str = "svolder eq show names the preset that is on";

/// Print every row [`preset_rows`] builds, one text line or one JSON object each. JSON lists
/// every preset. Text lists the presets that fit a known device and counts the rest, or, with
/// `all` or no device, every preset under a heading for its band count. A text listing marked
/// without readings (`unread`) ends with the verb that names the preset that is on.
fn print_preset_list(
    config: &Config,
    device: Option<Marked<'_>>,
    (json, all): (bool, bool),
    unread: bool,
) -> Result<ExitCode, String> {
    let rows = preset_rows(config, device);
    if json {
        for row in &rows {
            println!("{}", serde_json::to_string(row).map_err(|e| e.to_string())?);
        }
        return Ok(ExitCode::SUCCESS);
    }
    for line in text_lines(&rows, device.is_some() && !all) {
        println!("{line}");
    }
    if unread {
        println!("{ON_HINT}");
    }
    Ok(ExitCode::SUCCESS)
}

/// The text lines of a listing. With `fitting_only`, the rows that fit the device, then one line
/// for each other band count that says how many presets it leaves out; otherwise every row,
/// grouped under a `<n> bands` heading in the order the band counts first appear, the imported
/// presets under `imported`.
fn text_lines(rows: &[PresetRow], fitting_only: bool) -> Vec<String> {
    let width = |rows: &mut dyn Iterator<Item = &PresetRow>| {
        rows.map(|row| row.name.chars().count()).max().unwrap_or(0)
    };
    let mut lines = Vec::new();
    if fitting_only {
        let shown: Vec<&PresetRow> = rows.iter().filter(|r| r.fits != Some(false)).collect();
        let name_width = width(&mut shown.iter().copied());
        lines.extend(shown.iter().map(|row| row.line(name_width)));
        let mut counts: Vec<(u8, usize)> = Vec::new();
        for row in rows.iter().filter(|r| r.fits == Some(false)) {
            match counts.iter_mut().find(|(bands, _)| *bands == row.bands) {
                Some((_, n)) => *n += 1,
                None => counts.push((row.bands, 1)),
            }
        }
        for (bands, n) in counts {
            // An imported row never fits a tone control with no bands, and has none of its own
            // to name.
            if bands == 0 {
                lines.push(format!("{n} more do not fit this device; --all lists them"));
                continue;
            }
            lines.push(format!("{n} more for {bands} bands; --all lists them"));
        }
        return lines;
    }
    let name_width = width(&mut rows.iter());
    let mut seen: Vec<Option<u8>> = Vec::new();
    for row in rows {
        if !seen.contains(&row.group()) {
            seen.push(row.group());
        }
    }
    for group in seen {
        lines.push(match group {
            Some(bands) => format!("{bands} bands"),
            None => "imported".to_string(),
        });
        lines.extend(
            rows.iter()
                .filter(|row| row.group() == group)
                .map(|row| row.grouped_line(name_width)),
        );
    }
    lines
}

/// Every row `eq list` prints: the built-ins for five bands, then for three (the only band
/// counts a tone control has today), then every configured preset in file order.
fn preset_rows(config: &Config, device: Option<Marked<'_>>) -> Vec<PresetRow> {
    let mut rows = Vec::new();
    for bands in [5usize, 3usize] {
        for name in preset::builtin_names(bands) {
            rows.push(builtin_row(name, bands as u8, device));
        }
    }
    for preset in &config.presets {
        rows.push(user_row(preset, device));
    }
    rows
}

/// Whether `preset` fits `device`'s tone control, when there is a device: its band count is the
/// tone control's, or it is imported and the tone control has bands.
fn fits(preset: &Preset, device: Option<Marked<'_>>) -> Option<bool> {
    device.map(|(eq, _)| fits_eq(preset, eq))
}

/// Whether `preset` fits the tone control `eq`.
fn fits_eq(preset: &Preset, eq: &EqSpec) -> bool {
    match preset.is_imported() {
        true => !eq.bands.is_empty(),
        false => eq.bands.len() == usize::from(preset.bands),
    }
}

/// One built-in's row: its values come from the device's own profile when it fits, since a
/// built-in's frequencies sit on the tone control's own centres.
fn builtin_row(name: &str, bands: u8, device: Option<Marked<'_>>) -> PresetRow {
    let on = match device {
        Some((eq, Some(values))) if eq.bands.len() == usize::from(bands) => {
            let raw = current_raw(values);
            preset::builtins(eq)
                .into_iter()
                .find(|preset| preset.name.eq_ignore_ascii_case(name))
                .map(|preset| preset::matches(eq, &raw, &preset))
        }
        _ => None,
    };
    PresetRow {
        name: name.to_string(),
        bands,
        built_in: true,
        imported: false,
        fits: device.map(|(eq, _)| eq.bands.len() == usize::from(bands)),
        on,
    }
}

/// One user preset's row.
fn user_row(preset: &Preset, device: Option<Marked<'_>>) -> PresetRow {
    let on = match device {
        Some((eq, Some(values))) if fits_eq(preset, eq) => {
            let raw = current_raw(values);
            Some(preset::matches(eq, &raw, preset))
        }
        _ => None,
    };
    PresetRow {
        name: preset.name.clone(),
        bands: preset.bands,
        built_in: false,
        imported: preset.is_imported(),
        fits: fits(preset, device),
        on,
    }
}

/// Reach the device and recall `name` onto it: the writes
/// [`svolder_profiles::tone::preset::plan_recall`] plans, sent in order, each clamp reported on
/// the way. A direct session is closed before returning, on every path.
///
/// A preset that is on the computer is asked about first, and moved: the computer EQ is
/// removed, then the preset is loaded. A no changes nothing. `yes` answers the question without
/// a terminal.
pub async fn eq_load(g: &Globals<'_>, name: &str, yes: bool) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    let outcome = do_load(g, &mut backend, name, yes).await;
    backend.close().await;
    outcome
}

/// [`eq_load`]'s body, once a backend is open. The device is reached and the preset found
/// before the computer EQ is asked about or removed, so a missing DAC or an unknown name never
/// leaves the preset on neither.
async fn do_load(
    g: &Globals<'_>,
    backend: &mut Backend,
    name: &str,
    yes: bool,
) -> Result<ExitCode, String> {
    let values = match backend.state().await {
        Ok(values) => values,
        Err(failure) => return Ok(failure.report()),
    };
    if backend.profile().eq.is_none() {
        return Err(
            match svolder_profiles::device_presets::offered(backend.profile()) {
                true => format!("{}; {}", ProfileError::NoEq, device_presets::POINTER),
                false => ProfileError::NoEq.to_string(),
            },
        );
    }
    let config = Config::load().map_err(|e| e.to_string())?;
    let preset = match resolve_preset(backend.profile(), &config, name) {
        Ok(preset) => preset,
        Err(failure) => return Ok(failure.report()),
    };
    let raw = current_raw(&values);
    if let (Some(imported), Some(eq)) = (&preset.source, backend.profile().eq.as_ref()) {
        eprintln!("svolder: {}", preset::fit_for(eq, imported).line());
    }
    let plan = match preset.eq_writes(backend.profile(), &raw) {
        Ok(recall) => recall.plan,
        Err(e) => return Ok(Failure::of(Kind::Usage, e.to_string()).report()),
    };
    // Only a recall that can be sent takes the preset off the computer.
    match crate::commands::host_eq::move_to_dac(g, &preset, yes).await {
        Ok(true) => {}
        Ok(false) => return Ok(super::nothing_changed()),
        Err(code) => return Ok(code),
    }
    for clamp in &plan.clamped {
        eprintln!("svolder: {clamp}");
    }
    backend.note_recalled(&preset.name).await;
    let total = plan.writes.len();
    match send_plan(backend, &plan).await {
        Ok(n) => {
            eprintln!("svolder: loaded {} ({})", preset.name, write_count(n));
            Ok(ExitCode::SUCCESS)
        }
        Err(failed) => {
            eprintln!("svolder: {}", partial_failure_line(failed.sent, total));
            Ok(failed.err.failure().report())
        }
    }
}

/// The preset `name` names, matched case-insensitively and without the blanks round it
/// ([`preset::same_name`]) against the built-ins this tone control offers before the
/// configuration's own — a built-in is looked up first because its values come from this
/// device's own profile, which a name in the configuration cannot give it.
fn resolve_preset(profile: &Profile, config: &Config, name: &str) -> Result<Preset, Failure> {
    if let Some((preset, _)) = config
        .fitting_presets(profile)
        .into_iter()
        .find(|(preset, _)| preset::same_name(&preset.name, name))
    {
        return Ok(preset);
    }
    // A preset of another band count is found too, so the recall can say it does not fit.
    config
        .preset(name)
        .cloned()
        .ok_or_else(|| no_such_preset(name))
}

/// What a name no preset carries earns.
pub(crate) fn no_such_preset(name: &str) -> Failure {
    Failure::of(
        Kind::Usage,
        format!("no preset named {name:?}; run `svolder eq list` to see them"),
    )
}

/// The failure a refused name earns: the rule's one wording, then this verb's `hint` after a
/// built-in's name or `custom`, and `--force` after another preset's name.
fn name_failure(refusal: NameRefusal, hint: &str) -> Failure {
    let words = match refusal {
        NameRefusal::BuiltIn(_) | NameRefusal::Reserved(_) => format!("{refusal}; {hint}"),
        NameRefusal::Taken(_) => format!("{refusal}; use --force to overwrite it"),
        NameRefusal::Name(_) => refusal.to_string(),
    };
    Failure::of(Kind::Usage, words)
}

/// Reach the device and save its current curve as a new preset named `name`, without the blanks
/// round it.
///
/// A name [`preset::checked_name`] refuses, a built-in's name and `custom` are refused outright,
/// before the device is reached. `--force` overwrites a user preset already carrying `name`;
/// without it, a name already taken is refused rather than silently replaced. A direct session
/// is closed before returning, on every path.
pub async fn eq_save(g: &Globals<'_>, name: &str, force: bool) -> Result<ExitCode, String> {
    let name = match preset::new_name(name, std::iter::empty::<&str>(), None) {
        Ok(name) => name,
        Err(refusal) => {
            return Ok(name_failure(refusal, "save this curve under another name").report());
        }
    };
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    let outcome = do_save(g, &mut backend, name, force).await;
    backend.close().await;
    outcome
}

/// [`eq_save`]'s body, once a backend is open and `name` is known not to be a built-in's or
/// `custom`.
async fn do_save(
    g: &Globals<'_>,
    backend: &mut Backend,
    name: &str,
    force: bool,
) -> Result<ExitCode, String> {
    let values = match backend.state().await {
        Ok(values) => values,
        Err(failure) => return Ok(failure.report()),
    };
    let eq = backend
        .profile()
        .eq
        .clone()
        .ok_or_else(|| ProfileError::NoEq.to_string())?;
    let raw = current_raw(&values);
    let Some(preset) = preset::from_readings(&eq, &raw, name) else {
        return Ok(Failure {
            code: 4,
            what: "not every tone-control reading has arrived yet".to_string(),
            next: crate::recovery::INSPECT.to_string(),
        }
        .report());
    };
    let path = Config::path();
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(e) => return Ok(config_failure(e).report()),
    };
    if let Some(imported) = config.preset(name).filter(|found| found.is_imported()) {
        return Ok(
            Failure::of(Kind::Usage, preset::imported_update_refusal(&imported.name)).report(),
        );
    }
    if let Some(found) = config.preset(name).filter(|_| !force) {
        return Ok(name_failure(NameRefusal::Taken(found.name.clone()), "").report());
    }
    let exists = config.preset(name).is_some();
    let result = match exists {
        true => Config::update_preset(&path, name, &preset),
        false => Config::add_preset(&path, &preset),
    };
    match result {
        Ok(()) => {
            eprintln!(
                "svolder: saved {} ({} bands) to {}",
                preset.name,
                preset.bands,
                path.display()
            );
            reload_daemon_for_presets(g, &config, &path).await;
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => Ok(config_failure(e).report()),
    }
}

/// Rename the preset `old` to `new` in the configuration file. Config only, no device: a name
/// carries between devices, so there is nothing on any one of them to reach. `old` is looked
/// up as every name is ([`preset::same_name`]), `new` is written without the blanks round it
/// and refused where [`preset::new_name`] refuses it (`custom` included), and a built-in's name
/// is refused on either side. A running daemon that reads this file is told to reload, so its
/// MQTT bridge offers the new list.
pub async fn eq_rename(g: &Globals<'_>, old: &str, new: &str) -> ExitCode {
    if preset::is_builtin(old) {
        return Failure::of(
            Kind::Usage,
            format!("{old:?} is a built-in preset; it cannot be renamed"),
        )
        .report();
    }
    let new = match preset::new_name(new, std::iter::empty::<&str>(), None) {
        Ok(new) => new,
        Err(refusal) => return name_failure(refusal, "pick another").report(),
    };
    let path = Config::path();
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(e) => return config_failure(e).report(),
    };
    let Some(old) = config.preset(old).map(|found| found.name.as_str()) else {
        return no_such_preset(old).report();
    };
    match Config::rename_preset(&path, old, new) {
        Ok(()) => {
            eprintln!("svolder: renamed {old:?} to {new:?} in {}", path.display());
            reload_daemon_for_presets(g, &config, &path).await;
            ExitCode::SUCCESS
        }
        Err(e) => config_failure(e).report(),
    }
}

/// Delete the preset `name` from the configuration file. Config only, no device.
///
/// `name` is looked up as every name is ([`preset::same_name`]), and the question and the
/// report name the preset as the configuration loads it. A built-in's name is refused. Without
/// `--yes`, a terminal is asked to confirm, as `passphrase` asks; a non-terminal stdin without
/// `--yes` refuses outright, so a script cannot delete a preset by accident. A running daemon
/// that reads this file is told to reload — see [`eq_rename`].
pub async fn eq_delete(g: &Globals<'_>, name: &str, yes: bool) -> ExitCode {
    if preset::is_builtin(name) {
        return Failure::of(
            Kind::Usage,
            format!("{name:?} is a built-in preset; it cannot be deleted"),
        )
        .report();
    }
    let path = Config::path();
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(e) => return config_failure(e).report(),
    };
    let Some(name) = config.preset(name).map(|found| found.name.as_str()) else {
        return no_such_preset(name).report();
    };
    if !yes {
        if std::io::stdin().is_terminal() {
            match ask(&format!("Delete preset {name:?}? [y/N]")) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!("svolder: cancelled");
                    return ExitCode::from(1);
                }
                Err(e) => {
                    eprintln!("svolder: {e}");
                    return ExitCode::from(1);
                }
            }
        } else {
            eprintln!("svolder: refusing to delete a preset without confirmation; pass --yes");
            return ExitCode::from(1);
        }
    }
    match Config::remove_preset(&path, name) {
        Ok(()) => {
            eprintln!("svolder: removed preset {name:?} from {}", path.display());
            reload_daemon_for_presets(g, &config, &path).await;
            ExitCode::SUCCESS
        }
        Err(e) => config_failure(e).report(),
    }
}

/// Read the Equalizer APO file `file` and save it as a preset: the file's name without its
/// extension and its ` ParametricEQ` unless `name` says another. Config only, no device. Prints
/// what was saved, then one fit line per distinct tone control among the configured devices,
/// worked out from their profiles with no connection. A running daemon that reads this file is
/// told to reload, as for [`eq_rename`].
pub async fn eq_import(
    g: &Globals<'_>,
    file: &Path,
    name: Option<&str>,
    force: bool,
) -> Result<ExitCode, String> {
    let parsed = match read_correction(file) {
        Ok(parsed) => parsed,
        Err(failure) => return Ok(failure.report()),
    };
    let named = match name {
        Some(name) => name.to_string(),
        None => preset::default_name(&file.file_stem().unwrap_or_default().to_string_lossy()),
    };
    let path = Config::path();
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(e) => return Ok(config_failure(e).report()),
    };
    let skipped = parsed.notes.len();
    let imported = Imported {
        origin: None,
        correction: parsed.correction,
    };
    Ok(import_and_report(g, &config, &path, &named, imported, skipped, force).await)
}

/// Save `imported` as the preset `name` ([`save_imported`]), print what was saved and one fit
/// line per tone control ([`saved_line`], [`fit_lines`]), and tell a running daemon to reload.
/// `skipped` is how many lines of the source the parser left out.
pub(super) async fn import_and_report(
    g: &Globals<'_>,
    config: &Config,
    path: &Path,
    name: &str,
    imported: Imported,
    skipped: usize,
    force: bool,
) -> ExitCode {
    let (filters, preamp) = (
        imported.correction.filters.len(),
        imported.correction.preamp_db,
    );
    let saved = match save_imported(config, path, name, imported, force) {
        Ok(saved) => saved,
        Err(failure) => return failure.report(),
    };
    eprintln!(
        "svolder: {}",
        saved_line(&saved.name, filters, preamp, skipped)
    );
    if let Some(imported) = &saved.source {
        for line in fit_lines(config, imported) {
            eprintln!("svolder: {line}");
        }
    }
    reload_daemon_for_presets(g, config, path).await;
    ExitCode::SUCCESS
}

/// Save `imported` as the preset `name`, trimmed, in the configuration file at `path`, and
/// return it. `config` is the file as loaded, which says what names are taken.
///
/// A name [`preset::checked_name`] refuses, a built-in's name and `custom` are refused outright. A
/// name another preset carries is refused unless `force`, which replaces a user preset, a preset of
/// bands included, and never a built-in. The caller tells a running daemon to reload.
pub(crate) fn save_imported(
    config: &Config,
    path: &Path,
    name: &str,
    imported: Imported,
    force: bool,
) -> Result<Preset, Failure> {
    let name = match preset::new_name(name, config.presets.iter().map(|p| p.name.as_str()), None) {
        Ok(name) => name,
        Err(NameRefusal::Taken(_)) if force => name.trim(),
        Err(refusal) => return Err(name_failure(refusal, "pick another with --name")),
    };
    let exists = config.preset(name).is_some();
    let preset = Preset::imported(name, imported);
    match exists {
        true => Config::update_preset(path, name, &preset),
        false => Config::add_preset(path, &preset),
    }
    .map_err(config_failure)?;
    Ok(preset)
}

/// The correction in the file `file`, or why it cannot be read: not there, not text, more than
/// [`apo::MAX_BYTES`] long, or refused by [`apo::parse`] with its line.
pub(crate) fn read_correction(file: &Path) -> Result<apo::Parsed, Failure> {
    let shown = file.display();
    let unreadable = |why: String| Failure::of(Kind::Usage, format!("cannot read {shown}: {why}"));
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .and_then(|f| f.take(apo::MAX_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| unreadable(e.to_string()))?;
    let text = String::from_utf8(bytes).map_err(|_| unreadable("it is not UTF-8 text".into()))?;
    apo::parse(&text).map_err(|e| Failure::of(Kind::Usage, format!("{shown}: {e}")))
}

/// What a saved import says: `imported "Sennheiser HD 650" (10 filters, preamp -6.1 dB, 1
/// skipped)`.
pub(crate) fn saved_line(name: &str, filters: usize, preamp: f64, skipped: usize) -> String {
    let plural = if filters == 1 { "" } else { "s" };
    let mut line = format!("imported {name:?} ({filters} filter{plural}, preamp {preamp:.1} dB");
    if skipped > 0 {
        line.push_str(&format!(", {skipped} skipped"));
    }
    line.push(')');
    line
}

/// One fit line per distinct tone control among the configured devices, in the order the file
/// lists them: `Gungnir 2 · 5 bands: fit 0.9 dB RMS · worst 2.4 dB at 8.0 kHz`, the device's
/// profile named by its product. A device whose profile cannot be told, or that has no tone
/// control, adds none.
fn fit_lines(config: &Config, imported: &Imported) -> Vec<String> {
    let Ok(registry) = crate::backend::registry() else {
        return Vec::new();
    };
    let state = crate::state::DeviceState::load();
    let mut seen: Vec<EqSpec> = Vec::new();
    let mut lines = Vec::new();
    for device in &config.devices {
        let Some(profile) = device
            .profile
            .clone()
            .or_else(|| state.last_profile(&device.udid))
            .and_then(|name| registry.get(&name))
        else {
            continue;
        };
        let Some(eq) = profile.eq.as_ref().filter(|eq| !seen.contains(eq)) else {
            continue;
        };
        seen.push(eq.clone());
        lines.push(format!(
            "{} · {} bands: {}",
            profile.display,
            eq.bands.len(),
            preset::fit_for(eq, imported).line()
        ));
    }
    lines
}
