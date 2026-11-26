// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder host loudness`: the loudness on what the computer plays, through the daemon.
//!
//! The daemon owns the loudness (it follows the computer's volume), so every verb is one
//! `HostEqOp::Loudness` request to it. Every number printed comes from the `LoudnessView` it
//! sends; this module works none out. Without a daemon, `show` reads the saved settings and every
//! verb is refused.

use std::process::ExitCode;

use svolder_host::eq::{LoudnessSettings, LoudnessView};
use svolder_ipc::{CODE_COMPUTER_UNAVAILABLE, Config, HostEqOp};

use super::host_crossfeed::{Answer, Reach, reach, usage};
use super::host_eq::{eq_op, eq_view, state_path};
use crate::backend::Globals;
use crate::cli::HostLoudnessCommand;
use crate::recovery::{Failure, Kind};

/// The start of the refusal of a verb with no daemon.
const NEEDS_A_DAEMON: &str = "loudness needs svolderd";
/// Why `show` has only the saved settings when a daemon answers with no computer EQ.
const HOST_IS_OFF: &str = "the daemon's host audio is off";
/// Why `show` has only the saved settings when the daemon did not answer in time.
const NO_ANSWER_UNKNOWN: &str = "the daemon did not answer, so whether it plays now is unknown";
/// Why `show` has only the saved settings when no daemon runs: nothing follows the volume then,
/// unless a TUI does.
const NO_DAEMON_UNKNOWN: &str =
    "no daemon is running, so nothing follows the volume unless a TUI does";

/// `svolder host loudness [--json] [VERB]`.
pub(crate) async fn run(
    g: &Globals<'_>,
    json: bool,
    command: Option<HostLoudnessCommand>,
) -> ExitCode {
    let config = Config::load().map_err(|e| e.to_string());
    let saved = saved_at(&state_path());
    match answer(
        g,
        config.as_ref().map_err(String::as_str),
        saved,
        json,
        command,
    )
    .await
    {
        Ok(answer) => {
            answer.out.iter().for_each(|line| println!("{line}"));
            answer
                .err
                .iter()
                .for_each(|line| eprintln!("svolder: {line}"));
            ExitCode::SUCCESS
        }
        Err(failure) => failure.report(),
    }
}

/// The loudness settings in effect at the state file `path`: the state's, else the settings kept
/// before any state existed ([`svolder_host::eq::HostEqSettings::effective`]).
fn saved_at(path: &std::path::Path) -> Option<LoudnessSettings> {
    if let Some(why) = svolder_host::eq::HostEqSettings::unreadable(path) {
        eprintln!("svolder: ignoring the kept computer EQ settings: {why}");
    }
    svolder_host::eq::HostEqSettings::effective(path).loudness
}

/// What `command` (show, with none) does and says. `saved` is the settings the state file keeps,
/// which `show` reads when no daemon serves the computer EQ.
async fn answer(
    g: &Globals<'_>,
    config: Result<&Config, &str>,
    saved: Option<LoudnessSettings>,
    json: bool,
    command: Option<HostLoudnessCommand>,
) -> Result<Answer, Failure> {
    // `show` spelled out is the same as no verb.
    let command = command.filter(|command| !matches!(command, HostLoudnessCommand::Show));
    let config = match (config, &command) {
        (Ok(config), _) => Some(config),
        (Err(why), Some(_)) => return Err(Failure::of(Kind::ConfigUnreadable, why)),
        (Err(_), None) => None,
    };
    let (socket, eq) = match reach(g, config).await {
        Reach::Serving(socket, eq) => (socket, eq),
        Reach::NoComputer => {
            return match command {
                None => Ok(shown(&offline(saved.as_ref(), false, HOST_IS_OFF), json)),
                Some(_) => Err(Failure {
                    code: CODE_COMPUTER_UNAVAILABLE,
                    what: "the daemon serves no computer EQ (host audio is off for it)".to_string(),
                    next: "Restart it without SVOLDER_HOST=off, then try again".to_string(),
                    reason: None,
                    adapter_hint: None,
                }),
            };
        }
        Reach::Silent => {
            return match command {
                None => Ok(shown_unknown(
                    &offline(
                        saved.as_ref(),
                        saved.is_some_and(|s| s.on),
                        NO_ANSWER_UNKNOWN,
                    ),
                    json,
                )),
                Some(_) => Err(Failure::of(
                    Kind::DaemonError,
                    "the daemon did not answer in time",
                )),
            };
        }
        Reach::NoDaemon => {
            return match command {
                None => Ok(shown_unknown(
                    &offline(
                        saved.as_ref(),
                        saved.is_some_and(|s| s.on),
                        NO_DAEMON_UNKNOWN,
                    ),
                    json,
                )),
                Some(_) => Err(needs_a_daemon_for_loudness()),
            };
        }
    };
    let Some(before) = eq.loudness else {
        return Err(Failure::of(
            Kind::DaemonTooOld,
            "the daemon does not serve the loudness",
        ));
    };
    let Some(command) = command else {
        return Ok(shown(&before, json));
    };
    eq_op(&socket, request(&command)?).await?;
    let after = eq_view(&socket)
        .await
        .and_then(|eq| eq.loudness)
        .ok_or_else(|| {
            Failure::of(
                Kind::DaemonError,
                "the daemon did not show the loudness after the change",
            )
        })?;
    Ok(match json {
        true => shown(&after, true),
        false => said(&command, &after),
    })
}

/// The refusal of a verb with no daemon serving the computer EQ, in the loudness's words.
fn needs_a_daemon_for_loudness() -> Failure {
    Failure {
        code: Kind::NoDaemon.code(),
        what: format!("{NEEDS_A_DAEMON}, and none serves the computer EQ"),
        next: "Start it with `svolder daemon install`, then try again".to_string(),
        reason: None,
        adapter_hint: None,
    }
}

/// The saved settings as a view, for a machine with no daemon: nothing is read of the volume, so
/// no readout, the switch `on` and `why` in `unavailable`.
fn offline(settings: Option<&LoudnessSettings>, on: bool, why: &str) -> LoudnessView {
    let settings = LoudnessSettings {
        on,
        ..settings.copied().unwrap_or_default()
    };
    LoudnessView::of(&settings, None, 0.0, 48_000, None, Some(why.to_string()))
}

/// The request `command` stands for.
fn request(command: &HostLoudnessCommand) -> Result<HostEqOp, Failure> {
    let (mut on, mut strength_pct, mut reference_pct, mut reference_now) = (None, None, None, None);
    match command {
        HostLoudnessCommand::Show => {
            return Err(usage(
                "show sends nothing".to_string(),
                "Give a verb that changes something".to_string(),
            ));
        }
        HostLoudnessCommand::On => on = Some(true),
        HostLoudnessCommand::Off => on = Some(false),
        HostLoudnessCommand::Strength { percent } => {
            strength_pct = Some(number(percent, "a strength")?)
        }
        HostLoudnessCommand::Reference { value } => {
            match value.trim().eq_ignore_ascii_case("now") {
                true => reference_now = Some(true),
                false => reference_pct = Some(number(value, "a reference")?),
            }
        }
    }
    Ok(HostEqOp::Loudness {
        on,
        strength_pct,
        reference_pct,
        reference_now,
        done: true,
    })
}

/// The percent in `value`, which may end in `%` (`35%`), when it is a finite number that is not
/// negative; else the refusal in svolder's words. The daemon snaps and holds it to its range.
fn number(value: &str, what: &str) -> Result<f64, Failure> {
    let trimmed = value.trim();
    let digits = trimmed.strip_suffix('%').unwrap_or(trimmed);
    match digits.trim().parse::<f64>() {
        Ok(number) if number.is_finite() && number >= 0.0 => Ok(number),
        _ => Err(usage(
            format!("{value} is not {what}"),
            "Give a percent, such as 35".to_string(),
        )),
    }
}

/// `view` for `show`: its lines, or with `--json` the view as the daemon sends it.
fn shown(view: &LoudnessView, json: bool) -> Answer {
    let out = match json {
        true => vec![serde_json::to_string(view).unwrap_or_default()],
        false => lines(view),
    };
    Answer {
        out,
        err: Vec::new(),
    }
}

/// `show` for a machine whose daemon cannot say whether the loudness plays: the first line says
/// the live state is unknown and what the state file holds; with `--json` the view carries the
/// reason in `unavailable`.
fn shown_unknown(view: &LoudnessView, json: bool) -> Answer {
    let mut answer = shown(view, json);
    if !json {
        let why = view.unavailable.as_deref().unwrap_or_default();
        answer.out[0] = format!(
            "loudness: unknown: {why} (the saved state says {})",
            on_word(view)
        );
    }
    answer
}

/// What a verb says after the daemon took it: the value it holds now, which may differ from the
/// one asked for when the daemon snapped or held it. A note the daemon left follows.
fn said(command: &HostLoudnessCommand, after: &LoudnessView) -> Answer {
    let line = match command {
        HostLoudnessCommand::Show | HostLoudnessCommand::On | HostLoudnessCommand::Off => {
            format!("loudness {}", on_word(after))
        }
        HostLoudnessCommand::Strength { .. } => {
            format!("loudness strength: {:.0} %", after.strength_pct)
        }
        HostLoudnessCommand::Reference { value } => {
            match value.trim().eq_ignore_ascii_case("now") {
                true => format!(
                    "loudness reference: {:.0} % (the current volume)",
                    after.reference_pct
                ),
                false => format!("loudness reference: {:.0} %", after.reference_pct),
            }
        }
    };
    let mut err = vec![line];
    err.extend(after.note.iter().map(|note| format!("note: {note}")));
    Answer {
        out: Vec::new(),
        err,
    }
}

fn on_word(view: &LoudnessView) -> &'static str {
    match view.on {
        true => "on",
        false => "off",
    }
}

/// A dB with its sign, to one decimal, with the real minus sign: `+7.3 dB`, `−7.3 dB`, `0.0 dB`.
fn signed_db(db: f64) -> String {
    match db {
        db if db > 0.0 => format!("+{db:.1} dB"),
        db if db < 0.0 => format!("{db:.1} dB").replace('-', "\u{2212}"),
        _ => "0.0 dB".to_string(),
    }
}

/// The lines `show` prints for `view`. The readout (volume, depth and the shelf gains) is the
/// view's, printed once the volume has been read.
fn lines(view: &LoudnessView) -> Vec<String> {
    let mut lines = vec![match &view.unavailable {
        Some(why) => format!("loudness: {}: {why}", on_word(view)),
        None => format!("loudness: {}", on_word(view)),
    }];
    lines.push(format!("strength: {:.0} %", view.strength_pct));
    lines.push(format!("reference: {:.0} %", view.reference_pct));
    if let Some(volume) = view.volume_pct {
        lines.push(format!("volume: {volume:.1} %"));
        lines.push(format!("depth: {:.1} dB", view.depth_db));
        lines.push(format!("bass: {}", signed_db(view.lift_db)));
        lines.push(format!("mids: {}", signed_db(view.mids_db)));
        lines.push(format!("treble: {}", signed_db(view.treble_db)));
    }
    lines.extend(view.note.iter().map(|note| format!("note: {note}")));
    lines
}

#[cfg(test)]
mod tests;
