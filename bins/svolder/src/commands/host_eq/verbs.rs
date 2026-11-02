// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The verbs of `svolder host eq`, and the question `eq load` asks, over [`super::Host`]'s
//! machine-wide entry points.

use std::path::Path;
use std::process::ExitCode;

use svolder_host::eq::Route;
use svolder_ipc::Config;
use svolder_profiles::tone::apo::Correction;
use svolder_profiles::tone::preset;

use super::{
    ApplyRequest, Status, apply, available, host_correction, lowered_note, remove, state, status,
};
use crate::backend::Globals;
use crate::commands::device;
use crate::recovery::{Failure, Kind};

/// `svolder host eq apply TARGET`: a file when TARGET names one, else a preset. A preset that
/// is on a device of a running daemon is asked about first, and that device is flattened when
/// the answer is yes: the same correction is never left on both.
pub async fn host_apply(g: &Globals<'_>, target: &str, sink: Option<&str>, yes: bool) -> ExitCode {
    if let Err(failure) = available() {
        return failure.report();
    }
    let config = match Config::load() {
        Ok(config) => config,
        Err(e) => return Failure::of(Kind::ConfigUnreadable, e.to_string()).report(),
    };
    let (name, origin, correction, is_preset) = match source(&config, target) {
        Ok(source) => source,
        Err(failure) => return failure.report(),
    };
    let mut flattened = false;
    if is_preset {
        let on_dac = device::holders(g, &config, &name).await;
        if !on_dac.is_empty() {
            let question = format!("{name} is on the DAC. Move it to the computer? [y/N]");
            match device::confirm(&question, yes) {
                Ok(true) => {}
                Ok(false) => return device::nothing_changed(),
                Err(failure) => return failure.report(),
            }
            if let Err(code) = device::flatten_holders(&on_dac).await {
                return code;
            }
            flattened = !on_dac.is_empty();
        }
    }
    let stated = correction.preamp_db;
    let request = ApplyRequest {
        name: name.clone(),
        origin,
        correction,
        sink: sink.map(str::to_string),
    };
    match apply(request).await {
        Ok(state) => {
            if let Some(note) = lowered_note(stated, state.preamp_db) {
                eprintln!("svolder: {note}");
            }
            let status = Status {
                state: Some(state.clone()),
                running: Some(true),
                unreadable: None,
            };
            eprintln!("svolder: {}", status.lines()[0]);
            if state.route == Route::Default {
                eprintln!(
                    "svolder: this WirePlumber has no smart filters: with the sink unplugged the \
                     sound goes to the next output, still corrected"
                );
            }
            ExitCode::SUCCESS
        }
        Err(mut failure) => {
            if flattened {
                failure.next = format!(
                    "The DAC was flattened; put the curve back with `svolder eq load {name}`. {}",
                    failure.next
                );
            }
            failure.report()
        }
    }
}

/// The name, the origin and the correction `target` stands for, and whether it is a preset
/// (as opposed to a file, which is nothing saved and so is on no device).
fn source(
    config: &Config,
    target: &str,
) -> Result<(String, Option<String>, Correction, bool), Failure> {
    if Path::new(target).is_file() {
        let path = Path::new(target);
        let parsed = device::read_correction(path)?;
        let shown = path.file_name().unwrap_or_default().to_string_lossy();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let name = Some(preset::default_name(&stem))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| shown.into_owned());
        return Ok((name, None, parsed.correction, false));
    }
    if preset::is_builtin(target) {
        return Err(Failure::of(
            Kind::Usage,
            format!(
                "{target:?} is a built-in preset of the DAC's tone control; \
                 save a curve under another name to put it on the computer"
            ),
        ));
    }
    let found = config
        .preset(target)
        .ok_or_else(|| device::no_such_preset(target))?;
    let origin = found.source.as_ref().and_then(|s| s.origin.clone());
    let correction = host_correction(found).map_err(|why| Failure::of(Kind::Usage, why))?;
    Ok((found.name.clone(), origin, correction, true))
}

/// `svolder host eq remove`.
pub async fn host_remove() -> ExitCode {
    let was_on = state().is_some_and(|state| state.on);
    match remove().await {
        Ok(()) => {
            eprintln!(
                "svolder: {}",
                match was_on {
                    true => "host EQ removed",
                    false => "host EQ was not on",
                }
            );
            ExitCode::SUCCESS
        }
        Err(failure) => failure.report(),
    }
}

/// `svolder host eq status [--json]`.
pub async fn host_status(json: bool) -> ExitCode {
    let status = status().await;
    match json {
        true => println!("{}", status.json()),
        false => status.lines().iter().for_each(|line| println!("{line}")),
    }
    ExitCode::SUCCESS
}

/// The question `eq load NAME` asks when NAME is on the computer, and its consequence: on yes
/// the computer EQ is removed so the preset can go to the DAC. `Ok(true)` goes on loading,
/// `Ok(false)` is a no, and `Err` carries the exit code of a refusal or a failed removal.
pub async fn move_to_dac(name: &str, yes: bool) -> Result<bool, ExitCode> {
    let Some(state) = state().filter(|s| s.on && preset::same_name(&s.name, name)) else {
        return Ok(true);
    };
    let question = format!(
        "{} is on the computer. Move it to the DAC? [y/N]",
        state.name
    );
    match device::confirm(&question, yes) {
        Ok(true) => {}
        Ok(false) => return Ok(false),
        Err(failure) => return Err(failure.report()),
    }
    remove().await.map_err(Failure::report)?;
    Ok(true)
}
