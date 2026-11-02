// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone-control verbs that read or write the curve itself: `eq show`, `eq set`, `eq mode`
//! and `eq flatten`, and the sending of a plan of writes that `eq load` shares.

use std::process::ExitCode;

use svolder_ipc::{Config, ValueObject};
use svolder_profiles::tone::preset::{self, Preset};
use svolder_profiles::{EqMode, EqParam, EqSpec, ProfileError};
use svolder_profiles::{tone::response, units};
use svolder_proto::FunctionId;

use crate::backend::{Backend, Globals, WriteError};
use crate::format::print_value;

use super::{open, report};

/// Reach the device and print the tone control's mode, loudness, and every band's
/// frequency, gain, and Q from its confirmed readings. A band value the device has not yet
/// reported prints as `?`. A profile with no `eq` block is an error. A direct session is
/// closed before returning, on every path.
pub async fn eq_show(g: &Globals<'_>, json: bool) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    let outcome = print_eq(&mut backend, json).await;
    backend.close().await;
    outcome
}

/// Print the tone control's mode, loudness, and every band's frequency, gain, and Q, with the
/// preset that is on and, while the curve can clip, how far it peaks.
///
/// As JSON: one object per reading of the tone control, then `{"preset":NAME}` (`custom` where
/// none is on) and `{"peak":DB}`, the peak in dB to the tenth as the text's `peak` line has it,
/// or `null` wherever the text prints no such line.
async fn print_eq(backend: &mut Backend, json: bool) -> Result<ExitCode, String> {
    // As in `print_one_reading`: a backend with nothing to read reports that first.
    let values = match backend.state().await {
        Ok(values) => values,
        Err(failure) => return Ok(failure.report()),
    };
    let eq = backend
        .profile()
        .eq
        .clone()
        .ok_or_else(|| ProfileError::NoEq.to_string())?;
    let config = Config::load().map_err(|e| e.to_string())?;
    let on = on_preset(
        &eq,
        &config,
        &values,
        backend.recalled_as_opened().as_deref(),
    );
    let preset_name = on
        .as_ref()
        .map_or_else(|| "custom".to_string(), |preset| preset.name.clone());
    if json {
        for vo in values.iter().filter(|vo| in_eq_block(vo, eq.id)) {
            print_value(vo, true);
        }
        let peak = response::clipping_peak(&eq, current_raw(&values))
            .map(|peak| (peak * 10.0).round() / 10.0);
        println!("{}", serde_json::json!({ "preset": preset_name }));
        println!("{}", serde_json::json!({ "peak": peak }));
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(vo) = reading_for(&values, eq.mode_id()) {
        println!("mode {}", vo.display);
    }
    if let Some(vo) = reading_for(&values, eq.loudness_id()) {
        println!("loudness {}", vo.display);
    }
    println!("preset {preset_name}");
    if let Some(imported) = on.as_ref().and_then(|preset| preset.source.as_ref()) {
        println!("{}", preset::fit_for(&eq, imported).line());
    }
    for band in 0..eq.bands.len() {
        // A value the device has not reported reads `?`, with its unit.
        let text = |param: EqParam, missing: &str| {
            eq_band_raw(&values, &eq, band, param)
                .and_then(|v| units::band_text(&eq, band, param, i64::from(v)))
                .unwrap_or_else(|| missing.to_string())
        };
        println!(
            "band {}  {}  {}  Q {}",
            band + 1,
            text(EqParam::Frequency, "? Hz"),
            text(EqParam::Gain, "? dB"),
            text(EqParam::Q, "?")
        );
    }
    if let Some(line) = peak_line(&eq, &values) {
        println!("{line}");
    }
    Ok(ExitCode::SUCCESS)
}

/// The preset that is on ([`preset::on`]): the device's mode and every band match it, and, for
/// a Loudness preset, its strength too. The preset recalled last, by any front end, wins while
/// it matches ([`Backend::recalled_as_opened`]); otherwise the built-ins this tone control
/// offers are checked before the user's own. `None` when none matches or the curve is not read
/// in full (`eq show` prints `custom`).
fn on_preset(
    eq: &EqSpec,
    config: &Config,
    values: &[ValueObject],
    recalled: Option<&str>,
) -> Option<Preset> {
    let fitting: Vec<Preset> = preset::fitting(eq, &config.presets)
        .into_iter()
        .map(|(preset, _)| preset)
        .collect();
    match preset::on(eq, current_raw(values), &fitting, recalled) {
        preset::On::Preset(found) => Some(found.clone()),
        preset::On::Unread | preset::On::Custom => None,
    }
}

/// The note `eq show` prints under the bands while the curve boosts: gone in
/// any mode but EQ, while a band has not reported in full, and while the summed peak is at or
/// under 0 dB — [`response::clipping_peak`]'s test, which the TUI's zone and note ask too.
fn peak_line(eq: &EqSpec, values: &[ValueObject]) -> Option<String> {
    let peak = response::clipping_peak(eq, current_raw(values))?;
    Some(format!(
        "peak {peak:+.1} dB — keep the volume at or below {:.1} dB to avoid clipping on loud \
         masters",
        -peak
    ))
}

/// The tone-control mode the device last reported, when it has reported one.
fn current_mode(eq: &EqSpec, values: &[ValueObject]) -> Option<EqMode> {
    reading_for(values, eq.mode_id())?
        .raw
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .and_then(EqMode::from_raw)
}

/// The raw reading for one function id, as [`svolder_profiles::tone::preset`] takes it: `None` for
/// one the device has not reported, or that does not fit in an `i64` — which every wire value
/// this crate reads does.
pub(super) fn current_raw(values: &[ValueObject]) -> impl Fn(FunctionId) -> Option<i64> + '_ {
    move |id: FunctionId| reading_for(values, id).and_then(|vo| vo.raw.as_i64())
}

/// Whether a reading addresses the tone control's function block.
fn in_eq_block(vo: &ValueObject, eq_id: u8) -> bool {
    vo.id
        .parse::<FunctionId>()
        .is_ok_and(|id| id.first() == eq_id)
}

/// The reading for one function id, when the device has reported one.
fn reading_for(values: &[ValueObject], id: FunctionId) -> Option<&ValueObject> {
    let want = id.to_string();
    values.iter().find(|vo| vo.id == want)
}

/// The raw value the device last reported for one band parameter, when it has reported one.
fn eq_band_raw(values: &[ValueObject], eq: &EqSpec, band: usize, param: EqParam) -> Option<u8> {
    let vo = reading_for(values, eq.band_id(band, param)?)?;
    vo.raw.as_u64().and_then(|v| u8::try_from(v).ok())
}

/// Reach the device, set one band's frequency (Hz), gain (dB), or Q, and print the confirmed
/// value. `band` is 1-based on the command line. A direct session is closed before
/// returning, on every path.
pub async fn eq_set(
    g: &Globals<'_>,
    band: usize,
    param: &str,
    value: f64,
    json: bool,
) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    // The verb prints the reading the device confirmed, so it waits for it.
    let code = report(backend.eq_set(band, param, value, false).await, json);
    backend.close().await;
    Ok(code)
}

/// Reach the device, select the tone-control mode, and print the confirmed value. A direct
/// session is closed before returning, on every path.
pub async fn eq_mode(g: &Globals<'_>, mode: &str, json: bool) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    // As in `eq_set`: the confirmed reading is what this prints.
    let code = report(backend.eq_mode(mode, false).await, json);
    backend.close().await;
    Ok(code)
}

/// Reach the device and put every band on 0 dB at its centre frequency and centre Q, as the
/// built-in Flat has them, leaving the mode and the loudness strength alone — except that, in
/// Flat mode, the mode is written to EQ after the bands so the change is heard; in Loudness
/// mode it is left alone. A curve already on Flat's values writes nothing, the mode included,
/// and says [`ALREADY_FLAT`], as the TUI's `0` writes nothing there. A direct session is closed
/// before returning, on every path.
pub async fn eq_flatten(g: &Globals<'_>) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    let outcome = do_flatten(&mut backend).await;
    backend.close().await;
    outcome
}

/// What `eq flatten` says when every band is on Flat's values already.
const ALREADY_FLAT: &str = "the curve is already flat (0 writes)";

/// [`eq_flatten`]'s body, once a backend is open.
pub(super) async fn do_flatten(backend: &mut Backend) -> Result<ExitCode, String> {
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
    let mut plan = preset::flatten(&eq, &raw);
    // A curve already flat has no band to write, and so nothing to switch the mode for: the
    // TUI's `0` does the same.
    if plan.writes.is_empty() {
        eprintln!("svolder: {ALREADY_FLAT}");
        return Ok(ExitCode::SUCCESS);
    }
    let switching = current_mode(&eq, &values) == Some(EqMode::Flat);
    if switching {
        plan.writes.push(preset::Write::Mode(EqMode::Eq));
    }
    let total = plan.writes.len();
    match send_plan(backend, &plan).await {
        Ok(n) => {
            if switching {
                eprintln!("svolder: {}", preset::SWITCHED_TO_EQ);
            }
            eprintln!("svolder: flattened ({})", write_count(n));
            Ok(ExitCode::SUCCESS)
        }
        Err(failed) => {
            eprintln!("svolder: {}", partial_failure_line(failed.sent, total));
            Ok(failed.err.failure().report())
        }
    }
}

/// Where a plan's send stopped: how many writes had already gone through, and why the next one
/// did not.
pub(super) struct PlanFailure {
    pub(super) sent: usize,
    pub(super) err: WriteError,
}

/// `n` as `eq load`/`eq flatten` name it: `1 write`, or `N writes` for any other count.
pub(super) fn write_count(n: usize) -> String {
    match n {
        1 => "1 write".to_string(),
        n => format!("{n} writes"),
    }
}

/// What `eq load`/`eq flatten` say when [`send_plan`] stops partway: `sent` writes went
/// through before the one at index `sent` failed, out of `total` the plan held.
///
/// A plan of exactly one write — where `sent` is always `0`, the only index there is to fail
/// at — gets its own sentence rather than the awkward "0 of 1 write went through".
pub(super) fn partial_failure_line(sent: usize, total: usize) -> String {
    match total {
        1 => "the write failed; nothing went through".to_string(),
        total => format!("only {sent} of {} went through", write_count(total)),
    }
}

/// Send every command of `plan`, in order, waiting for each confirmation, as `eq set` does.
/// Stops at the first failure.
pub(super) async fn send_plan(
    backend: &mut Backend,
    plan: &preset::Plan,
) -> Result<usize, PlanFailure> {
    let profile = backend.profile().clone();
    let commands = plan.commands(&profile);
    for (sent, command) in commands.iter().enumerate() {
        if let Err(err) = send_command(backend, command).await {
            return Err(PlanFailure { sent, err });
        }
    }
    Ok(commands.len())
}

/// Send one command through the backend call [`Backend::eq_set`], [`Backend::eq_mode`] or
/// the `set`/`raw` path the loudness strength goes through.
async fn send_command(
    backend: &mut Backend,
    command: &preset::EqCommand,
) -> Result<Option<ValueObject>, WriteError> {
    match command {
        preset::EqCommand::Band { band, param, value } => {
            backend.eq_set(*band, param, *value, false).await
        }
        preset::EqCommand::Mode(mode) => backend.eq_mode(mode, false).await,
        preset::EqCommand::Loudness {
            key: Some(key),
            raw,
            ..
        } => backend.set(key, &raw.to_string(), 0, false).await,
        preset::EqCommand::Loudness { id, raw, .. } => {
            backend
                .raw(&id.to_string(), "u8", &raw.to_string(), false)
                .await
        }
    }
}
