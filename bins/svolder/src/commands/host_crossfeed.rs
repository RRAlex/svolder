// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder host crossfeed`: the crossfeed on what the computer plays, through the daemon.
//!
//! The daemon owns the crossfeed (it watches the output and resets it), so every verb is one
//! `HostEqOp::Crossfeed` request to it. The model words, preset names and ranges come from the
//! `CrossfeedView` it sends; this module holds none of its own. Without a daemon, `show` reads the
//! saved settings and every verb is refused.

use std::process::ExitCode;

use svolder_host::eq::{CrossfeedSettings, CrossfeedView};
use svolder_ipc::{CODE_COMPUTER_UNAVAILABLE, Client, Config, HostEqOp, Request, Response};

use super::host_eq::{eq_op, eq_view};
use crate::backend::{ASK_WAIT, Globals, asks_a_daemon, launch};
use crate::cli::HostCrossfeedCommand;
use crate::recovery::{Failure, Kind};

/// Why the crossfeed cannot be changed without a daemon.
const NEEDS_A_DAEMON: &str = "crossfeed needs svolderd";
/// Why `show` has only the saved settings when a daemon answers with no computer EQ.
const HOST_IS_OFF: &str = "the daemon's host audio is off";
/// Why `show` has only the saved settings when the daemon did not answer in time.
const NO_ANSWER_UNKNOWN: &str = "the daemon did not answer, so whether it plays now is unknown";
/// Why `show` cannot say whether crossfeed plays when no daemon runs: a TUI without a daemon may
/// be playing it.
const NO_DAEMON_UNKNOWN: &str = "no daemon is running, so whether it plays now is unknown";

/// What a verb prints: the lines for standard output, then the lines for standard error (which
/// the command prefixes with `svolder: `).
#[derive(Debug, Default, PartialEq)]
pub(super) struct Answer {
    pub(super) out: Vec<String>,
    pub(super) err: Vec<String>,
}

/// `svolder host crossfeed [--json] [VERB]`.
pub(crate) async fn run(
    g: &Globals<'_>,
    json: bool,
    command: Option<HostCrossfeedCommand>,
) -> ExitCode {
    let config = Config::load().map_err(|e| e.to_string());
    let (saved, saved_on) = saved_at(&super::host_eq::state_path());
    match answer(
        g,
        config.as_ref().map_err(String::as_str),
        saved,
        saved_on,
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

/// The crossfeed settings in effect at the state file `path` and its switch: the state's, else
/// the settings kept before any state existed ([`svolder_host::eq::HostEqSettings::effective`]).
fn saved_at(path: &std::path::Path) -> (Option<CrossfeedSettings>, bool) {
    let on = svolder_host::eq::HostEqState::load(path)
        .ok()
        .flatten()
        .is_some_and(|state| state.crossfeed_on);
    if let Some(why) = svolder_host::eq::HostEqSettings::unreadable(path) {
        eprintln!("svolder: ignoring the kept computer EQ settings: {why}");
    }
    (
        svolder_host::eq::HostEqSettings::effective(path).crossfeed,
        on,
    )
}

/// What `command` (show, with none) does and says. `saved` is the settings the state file keeps
/// and `saved_on` its crossfeed switch, which `show` reads when no daemon serves the computer EQ.
async fn answer(
    g: &Globals<'_>,
    config: Result<&Config, &str>,
    saved: Option<CrossfeedSettings>,
    saved_on: bool,
    json: bool,
    command: Option<HostCrossfeedCommand>,
) -> Result<Answer, Failure> {
    // An unreadable configuration is reported for a verb, as `host eq apply` does; `show` still
    // has the saved settings to read.
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
                }),
            };
        }
        Reach::Silent => {
            return match command {
                None => Ok(shown_unknown(
                    &offline(saved.as_ref(), saved_on, NO_ANSWER_UNKNOWN),
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
                    &offline(saved.as_ref(), saved_on, NO_DAEMON_UNKNOWN),
                    json,
                )),
                Some(_) => Err(needs_a_daemon()),
            };
        }
    };
    let Some(before) = eq.crossfeed else {
        return Err(Failure::of(
            Kind::DaemonTooOld,
            "the daemon does not serve the crossfeed",
        ));
    };
    let Some(command) = command else {
        return Ok(shown(&before, json));
    };
    eq_op(&socket, request(&command, &before)?).await?;
    let after = eq_view(&socket)
        .await
        .and_then(|eq| eq.crossfeed)
        .ok_or_else(|| {
            Failure::of(
                Kind::DaemonError,
                "the daemon did not show the crossfeed after the change",
            )
        })?;
    Ok(match json {
        true => shown(&after, true),
        false => said(&command, &after),
    })
}

/// What asking for the daemon that serves the computer EQ found.
pub(super) enum Reach {
    /// Its socket, and the computer EQ it shows.
    Serving(std::path::PathBuf, Box<svolder_host::eq::EqView>),
    /// A daemon answers on the socket but shows no computer EQ (its host is off).
    NoComputer,
    /// A daemon is on the socket but did not answer the question in time.
    Silent,
    /// No daemon answers, or none was asked.
    NoDaemon,
}

/// Look for the daemon, starting nothing.
pub(super) async fn reach(g: &Globals<'_>, config: Option<&Config>) -> Reach {
    let Some(config) = config.filter(|_| asks_a_daemon(g.transport)) else {
        return Reach::NoDaemon;
    };
    let Some(socket) = launch::probe(g, config).await else {
        return Reach::NoDaemon;
    };
    match host_eq(&socket).await {
        Some(Some(eq)) => Reach::Serving(socket, Box::new(eq)),
        Some(None) => Reach::NoComputer,
        None => Reach::Silent,
    }
}

/// The computer EQ the daemon on `socket` shows: `Some(None)` when it answers and shows none,
/// `None` when it does not answer in time.
async fn host_eq(socket: &std::path::Path) -> Option<Option<svolder_host::eq::EqView>> {
    tokio::time::timeout(ASK_WAIT, async {
        let mut client = Client::connect(socket).await.ok()?;
        match client.request(Request::Host).await.ok()? {
            Response::Host(view) => Some(view.eq),
            _ => Some(None),
        }
    })
    .await
    .ok()
    .flatten()
}

/// The refusal of a verb with no daemon serving the computer EQ.
pub(super) fn needs_a_daemon() -> Failure {
    Failure {
        code: Kind::NoDaemon.code(),
        what: format!("{NEEDS_A_DAEMON}, and none serves the computer EQ"),
        next: "Start it with `svolder daemon install`, then try again".to_string(),
        reason: None,
    }
}

/// The saved settings as a view, for a machine with no daemon: the same view the daemon would
/// send for them (at 48 kHz), with the state file's switch `on` and `why`.
fn offline(settings: Option<&CrossfeedSettings>, on: bool, why: &str) -> CrossfeedView {
    let settings = settings.cloned().unwrap_or_default();
    let mut view = CrossfeedView::of(&settings, on, 48_000, None, None);
    view.unavailable = Some(why.to_string());
    view
}

/// A refusal of a value the command line got wrong.
pub(super) fn usage(what: String, next: String) -> Failure {
    Failure {
        code: Kind::Usage.code(),
        what,
        next,
        reason: None,
    }
}

/// The request `command` stands for, its words checked against the view's own lists.
fn request(command: &HostCrossfeedCommand, view: &CrossfeedView) -> Result<HostEqOp, Failure> {
    let mut op = (None, None, None, None, None, None);
    match command {
        HostCrossfeedCommand::On => op.0 = Some(true),
        HostCrossfeedCommand::Off => op.0 = Some(false),
        HostCrossfeedCommand::Model { word } => {
            let found = view
                .models
                .iter()
                .find(|model| model.word.eq_ignore_ascii_case(word.trim()));
            let Some(found) = found else {
                let words: Vec<_> = view.models.iter().map(|m| m.word.as_str()).collect();
                return Err(usage(
                    format!("no crossfeed model {word:?}"),
                    format!("The models are {}", words.join(", ")),
                ));
            };
            op.1 = Some(found.word.clone());
        }
        HostCrossfeedCommand::Preset { name } => {
            let name = name.join(" ");
            let presets = view
                .models
                .iter()
                .find(|model| model.word == view.model)
                .map_or(&[][..], |model| model.presets.as_slice());
            let Some(found) = presets
                .iter()
                .find(|preset| preset.name.eq_ignore_ascii_case(name.trim()))
            else {
                let names: Vec<_> = presets.iter().map(|p| p.name.as_str()).collect();
                return Err(usage(
                    format!("{} has no preset {name:?}", model_name(view)),
                    format!("The presets are {}", names.join(", ")),
                ));
            };
            op.2 = Some(found.name.clone());
        }
        HostCrossfeedCommand::Level { percent } => {
            op.3 = Some(number(percent, "%", "a level", "a percent, such as 37")?)
        }
        HostCrossfeedCommand::Crossover { hz } => {
            op.4 = Some(number(
                hz,
                "Hz",
                "a crossover",
                "a frequency in Hz, such as 700",
            )?)
        }
        HostCrossfeedCommand::Delay { value } => {
            // At or below the model's own lag the daemon takes it as "own": 0 always is.
            op.5 = Some(match value.trim().eq_ignore_ascii_case("own") {
                true => 0.0,
                false => number(
                    value,
                    "ms",
                    "a delay",
                    "milliseconds, such as 0.5, or `own`",
                )?,
            });
        }
    }
    Ok(HostEqOp::Crossfeed {
        on: op.0,
        model: op.1,
        preset: op.2,
        amount_pct: op.3,
        crossover_hz: op.4,
        delay_ms: op.5,
        done: true,
    })
}

/// The number in `value`, which may end in its `unit` (`37%`, `700Hz`), when it is a finite one
/// that is not negative; else the refusal in svolder's words.
fn number(value: &str, unit: &str, what: &str, hint: &str) -> Result<f64, Failure> {
    let trimmed = value.trim();
    let digits = match trimmed.len().checked_sub(unit.len()) {
        Some(at) if trimmed.is_char_boundary(at) && trimmed[at..].eq_ignore_ascii_case(unit) => {
            &trimmed[..at]
        }
        _ => trimmed,
    };
    match digits.trim().parse::<f64>() {
        Ok(number) if number.is_finite() && number >= 0.0 => Ok(number),
        _ => Err(usage(
            format!("{value} is not {what}"),
            format!("Give {hint}"),
        )),
    }
}

/// `view` for `show`: its lines, or with `--json` the view as the daemon sends it.
fn shown(view: &CrossfeedView, json: bool) -> Answer {
    let out = match json {
        true => vec![serde_json::to_string(view).unwrap_or_default()],
        false => lines(view),
    };
    Answer {
        out,
        err: Vec::new(),
    }
}

/// `show` for a machine whose daemon cannot say whether crossfeed plays (none runs, or it is
/// silent): the first line says the live state is unknown and what the state file holds; with
/// `--json` the view carries the file's switch and the reason in `unavailable`.
fn shown_unknown(view: &CrossfeedView, json: bool) -> Answer {
    let mut answer = shown(view, json);
    if !json {
        let why = view.unavailable.as_deref().unwrap_or_default();
        answer.out[0] = format!(
            "crossfeed: unknown: {why} (the saved state says {})",
            on_word(view)
        );
    }
    answer
}

/// What a verb says after the daemon took it: the value it holds now, which may differ from the
/// one asked for when the daemon held it to its range. A note the daemon left follows.
fn said(command: &HostCrossfeedCommand, after: &CrossfeedView) -> Answer {
    let line = match command {
        HostCrossfeedCommand::On | HostCrossfeedCommand::Off => {
            format!("crossfeed {}", on_word(after))
        }
        HostCrossfeedCommand::Model { .. } => format!("crossfeed model: {}", model_name(after)),
        HostCrossfeedCommand::Preset { .. } => format!("crossfeed preset: {}", preset_text(after)),
        HostCrossfeedCommand::Level { .. } => format!("crossfeed level: {}", amount_text(after)),
        HostCrossfeedCommand::Crossover { .. } => {
            format!("crossfeed crossover: {:.0} Hz", after.crossover_hz)
        }
        HostCrossfeedCommand::Delay { .. } => format!("crossfeed delay: {}", delay_text(after)),
    };
    let mut err = vec![line];
    err.extend(after.note.iter().map(|note| format!("note: {note}")));
    Answer {
        out: Vec::new(),
        err,
    }
}

fn on_word(view: &CrossfeedView) -> &'static str {
    match view.on {
        true => "on",
        false => "off",
    }
}

/// The model's name as the view lists it, else its word.
fn model_name(view: &CrossfeedView) -> &str {
    view.models
        .iter()
        .find(|model| model.word == view.model)
        .map_or(view.model.as_str(), |model| model.name.as_str())
}

fn preset_text(view: &CrossfeedView) -> String {
    match (&view.preset, &view.edited_from) {
        (Some(preset), _) => preset.clone(),
        (None, Some(from)) => format!("Custom (edited from {from})"),
        (None, None) => "Custom".to_string(),
    }
}

/// The level with its dB beside it, as the TUI and the web show it: `37 % · −8.6 dB`.
fn amount_text(view: &CrossfeedView) -> String {
    format!("{:.0} % \u{b7} {:.1} dB", view.amount_pct, view.amount_db).replace('-', "\u{2212}")
}

fn delay_text(view: &CrossfeedView) -> String {
    match (view.delay_settable, view.delay_own) {
        (false, _) => "the model's own (not settable)".to_string(),
        (true, true) => format!(
            "{:.2} ms (own, floor {:.2} ms)",
            view.delay_ms, view.delay_floor_ms
        ),
        (true, false) => format!(
            "{:.2} ms (floor {:.2} ms)",
            view.delay_ms, view.delay_floor_ms
        ),
    }
}

/// The lines `show` prints for `view`.
fn lines(view: &CrossfeedView) -> Vec<String> {
    let mut lines = vec![match &view.unavailable {
        Some(why) => format!("crossfeed: {}: {why}", on_word(view)),
        None => format!("crossfeed: {}", on_word(view)),
    }];
    lines.push(format!("model: {}", model_name(view)));
    lines.push(format!("preset: {}", preset_text(view)));
    lines.push(format!("level: {}", amount_text(view)));
    lines.push(format!("crossover: {:.0} Hz", view.crossover_hz));
    lines.push(format!("delay: {}", delay_text(view)));
    lines.extend(view.output.iter().map(|output| format!("output: {output}")));
    lines.extend(view.note.iter().map(|note| format!("note: {note}")));
    lines
}

#[cfg(test)]
mod tests;
