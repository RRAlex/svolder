// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The verbs of `svolder host eq`, and the question `eq load` asks, over [`super::Host`]'s
//! machine-wide entry points.

use std::path::Path;
use std::process::ExitCode;

use svolder_host::eq::{ApplyRequest, HostEqState, Route, Status, lowered_note};
use svolder_ipc::{Config, HostEqOp};
use svolder_profiles::tone::apo::Correction;
use svolder_profiles::tone::preset::{self, BothSides, Preset};
use svolder_profiles::{EqSpec, Registry};

use super::{apply, available, eq_op, remove, served, state, status};
use crate::backend::{Backend, Globals};
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
    // A daemon that serves the computer does the work, in front of the default sink; a sink
    // named here is the command's own to put the chain in front of.
    let daemon = match sink {
        None => served(g, &config).await,
        Some(_) => None,
    };
    let shown = match &daemon {
        Some((socket, _)) => shown_eq(socket, g.device).await,
        None => None,
    };
    let (name, origin, correction, held) = match source(&config, target, shown.as_ref()) {
        Ok(source) => source,
        Err(failure) => return failure.report(),
    };
    let mut flattened = false;
    if let Some(held) = held {
        let on_dac = device::holders(g, &config, &held).await;
        let both = preset::both_sides(&held, on_dac.first().map(|_| held.name.as_str()));
        if let BothSides::Ask { .. } = both {
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
    let applied = match &daemon {
        Some((socket, _)) => {
            let op = HostEqOp::Apply {
                name: name.clone(),
                origin,
                correction,
                sink: None,
            };
            eq_op(socket, op).await.map(|()| state())
        }
        None => {
            let request = ApplyRequest {
                name: name.clone(),
                origin,
                correction,
                sink: sink.map(str::to_string),
                slots: Vec::new(),
                rate_hz: None,
            };
            apply(request).await.map(Some)
        }
    };
    match applied {
        Ok(state) => {
            say_applied(&name, stated, state);
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

/// What a successful apply says: the preamp that was lowered, what is on and in front of what.
/// With no saved state to read (the daemon keeps none) it says only that the EQ is on.
fn say_applied(name: &str, stated: f64, state: Option<HostEqState>) {
    let Some(state) = state else {
        eprintln!("svolder: host EQ: {name} · on");
        return;
    };
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
}

/// The tone control of the device the daemon on `socket` shows for `device` (its only or its
/// default one when none is named), which a built-in is shaped to.
async fn shown_eq(socket: &Path, device: Option<&str>) -> Option<EqSpec> {
    let (backend, _) = Backend::open_on(socket, device).await.ok()?;
    let eq = backend.profile().eq.clone();
    backend.close().await;
    eq
}

/// The name, the origin and the correction `target` stands for, and the preset it is when it is
/// one (as opposed to a file, which is nothing saved and so is on no device).
fn source(
    config: &Config,
    target: &str,
    shown: Option<&EqSpec>,
) -> Result<(String, Option<String>, Correction, Option<Preset>), Failure> {
    if Path::new(target).is_file() {
        let path = Path::new(target);
        let parsed = device::read_correction(path)?;
        let shown = path.file_name().unwrap_or_default().to_string_lossy();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let name = Some(preset::default_name(&stem))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| shown.into_owned());
        let as_preset = preset::from_computer(&name, parsed.correction);
        let correction =
            preset::host_correction(&as_preset).map_err(|why| Failure::of(Kind::Usage, why))?;
        return Ok((name, None, correction, None));
    }
    if preset::is_builtin(target) {
        let built_in = built_in(target, shown).ok_or_else(|| device::no_such_preset(target))?;
        let correction =
            preset::host_correction(&built_in).map_err(|why| Failure::of(Kind::Usage, why))?;
        return Ok((built_in.name.clone(), None, correction, Some(built_in)));
    }
    let found = config
        .preset(target)
        .ok_or_else(|| device::no_such_preset(target))?;
    let origin = found.source.as_ref().and_then(|s| s.origin.clone());
    let correction = preset::host_correction(found).map_err(|why| Failure::of(Kind::Usage, why))?;
    Ok((found.name.clone(), origin, correction, Some(found.clone())))
}

/// The built-in preset called `name`, as the tone control `shown` shapes it (the shown device's
/// own), or, with no device to ask, as the five-band tone control of the Gungnir 2 does: a
/// built-in's frequencies are fixed, so the computer plays the same curve.
fn built_in(name: &str, shown: Option<&EqSpec>) -> Option<Preset> {
    let default;
    let eq = match shown {
        Some(eq) => eq,
        None => {
            default = Registry::builtin().ok()?.get("mesh-eq-5")?.eq.clone()?;
            &default
        }
    };
    preset::builtins(eq)
        .into_iter()
        .find(|built_in| preset::same_name(&built_in.name, name))
}

/// `svolder host eq remove`.
pub async fn host_remove(g: &Globals<'_>) -> ExitCode {
    let was_on = state().is_some_and(|state| state.on);
    let done = match Config::load().ok() {
        Some(config) => match served(g, &config).await {
            Some((socket, eq)) => {
                let was_on = was_on || eq.on;
                eq_op(&socket, HostEqOp::Mode { on: false })
                    .await
                    .map(|()| was_on)
            }
            None => remove().await.map(|()| was_on),
        },
        None => remove().await.map(|()| was_on),
    };
    match done {
        Ok(was_on) => {
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
pub async fn host_status(g: &Globals<'_>, json: bool) -> ExitCode {
    let mut status = status().await;
    // A chain the daemon keeps as its own child is not one this process can see: the daemon's
    // view says whether it runs.
    if let Some(config) = Config::load().ok()
        && let Some((_, eq)) = served(g, &config).await
        && status
            .state
            .as_ref()
            .is_some_and(|state| state.manager == svolder_host::eq::Manager::Child)
    {
        status.running = Some(eq.on);
    }
    match json {
        true => println!("{}", status.json()),
        false => status.lines().iter().for_each(|line| println!("{line}")),
    }
    ExitCode::SUCCESS
}

/// The question `eq load NAME` asks when NAME is on the computer, and its consequence: on yes
/// the computer EQ is removed so the preset can go to the DAC. `Ok(true)` goes on loading,
/// `Ok(false)` is a no, and `Err` carries the exit code of a refusal or a failed removal.
pub async fn move_to_dac(g: &Globals<'_>, preset: &Preset, yes: bool) -> Result<bool, ExitCode> {
    let Some(state) = state().filter(|s| s.on) else {
        return Ok(true);
    };
    if preset::both_sides(preset, Some(&state.name)) == BothSides::Free {
        return Ok(true);
    }
    let question = format!(
        "{} is on the computer. Move it to the DAC? [y/N]",
        state.name
    );
    match device::confirm(&question, yes) {
        Ok(true) => {}
        Ok(false) => return Ok(false),
        Err(failure) => return Err(failure.report()),
    }
    match Config::load().ok() {
        Some(config) => match served(g, &config).await {
            Some((socket, _)) => eq_op(&socket, HostEqOp::Mode { on: false }).await,
            None => remove().await,
        },
        None => remove().await,
    }
    .map_err(Failure::report)?;
    Ok(true)
}
