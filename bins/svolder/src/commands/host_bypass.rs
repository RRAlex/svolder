// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder host bypass`: every stage of the computer off in one change, and back as it was.
//!
//! The daemon owns the bypass, so `on` and `off` are one `HostEqOp::Bypass` request to it. Every
//! word and level printed comes from the `BypassView` and `ChainLevels` it sends; this module
//! works none out. With no daemon, the bare command reads the held stages the saved state keeps
//! (`[bypass]`) and says nothing of levels, which only a running chain has.

use std::process::ExitCode;

use svolder_host::eq::{BypassView, ChainLevels, EqView, HeldStages};
use svolder_ipc::{Config, HostEqOp};

use super::host_crossfeed::{Reach, reach};
use super::host_eq::{eq_op, eq_view, serving_daemon, state};
use crate::backend::Globals;
use crate::cli::HostBypassCommand;
use crate::recovery::{Failure, Kind};

/// `svolder host bypass [--json] [on|off]`.
pub(crate) async fn run(
    g: &Globals<'_>,
    json: bool,
    command: Option<HostBypassCommand>,
) -> ExitCode {
    match command {
        None => show(g, json).await,
        Some(command) => change(g, json, matches!(command, HostBypassCommand::On)).await,
    }
}

/// The stages a bypass brings back, in words: `EQ, loudness, speakers`; empty when none is held.
pub(crate) fn held_words(held: &HeldStages) -> String {
    [
        (held.eq, "EQ"),
        (held.loudness, "loudness"),
        (held.crossfeed, "crossfeed"),
        (held.speakers, "speakers"),
    ]
    .iter()
    .filter(|(held, _)| *held)
    .map(|(_, word)| *word)
    .collect::<Vec<_>>()
    .join(", ")
}

/// The one status line: `bypass: off · plays −14.8 dB in all`, or `bypass: on · brings back EQ,
/// loudness, speakers · pass-through −7.6 dB until the music pauses`. The levels' words are left
/// out when there are none (no chain, or no daemon to ask).
pub(crate) fn status_line(bypass: &BypassView, levels: Option<&ChainLevels>) -> String {
    let mut parts = vec![format!("bypass: {}", if bypass.on { "on" } else { "off" })];
    let held = held_words(&bypass.held);
    if bypass.on && !held.is_empty() {
        parts.push(format!("brings back {held}"));
    }
    parts.extend(levels.map(|l| l.words.clone()).filter(|w| !w.is_empty()));
    parts.join(" \u{b7} ")
}

/// The `--json` object: the bypass and the levels, `null` where there are none to tell, and
/// `unknown` (why) only when no daemon could say and the bypass is the saved state's.
pub(crate) fn json_of(bypass: Option<&BypassView>, levels: Option<&ChainLevels>) -> String {
    json_with(bypass, levels, None)
}

/// [`json_of`] with the reason the live state is not known, when it is not.
fn json_with(
    bypass: Option<&BypassView>,
    levels: Option<&ChainLevels>,
    unknown: Option<&str>,
) -> String {
    let mut object = serde_json::json!({ "bypass": bypass, "levels": levels });
    if let (Some(why), Some(map)) = (unknown, object.as_object_mut()) {
        map.insert("unknown".to_string(), serde_json::json!(why));
    }
    object.to_string()
}

/// The line for a bypass read from the saved state because no daemon could say: it is not stated
/// as fact (`bypass: unknown: <why> (the saved state says off)`).
pub(crate) fn unknown_line(saved: &BypassView, why: &str) -> String {
    let word = status_line(saved, None).replace("bypass: ", "");
    format!("bypass: unknown: {why} (the saved state says {word})")
}

/// What the saved state holds as a bypass, for a machine with no daemon to ask.
fn saved_bypass() -> BypassView {
    bypass_of_state(state().as_ref())
}

/// The bypass a saved state holds: none when a stage is on in it, which is the daemon's start
/// rule (a command-line apply keeps the held set, and the daemon drops it on adopting a state
/// with a stage on).
fn bypass_of_state(state: Option<&svolder_host::eq::HostEqState>) -> BypassView {
    match state {
        Some(state)
            if !state.on
                && !state.crossfeed_on
                && !state.speakers_on
                && !state.loudness.is_some_and(|loudness| loudness.on) =>
        {
            match state.bypass {
                Some(held) => BypassView { on: true, held },
                None => BypassView::default(),
            }
        }
        _ => BypassView::default(),
    }
}

/// Print `bypass` and `levels` as the command's answer: the JSON object, else the status line on
/// standard output (`to_out`) or standard error.
fn say(bypass: &BypassView, levels: Option<&ChainLevels>, json: bool, to_out: bool) {
    match (json, to_out) {
        (true, _) => println!("{}", json_of(Some(bypass), levels)),
        (false, true) => println!("{}", status_line(bypass, levels)),
        (false, false) => eprintln!("svolder: {}", status_line(bypass, levels)),
    }
}

/// `svolder host bypass`: the bypass and the levels of the daemon that serves the computer, else
/// what the saved state holds.
async fn show(g: &Globals<'_>, json: bool) -> ExitCode {
    let config = Config::load().ok();
    let reach_kind = reach(g, config.as_ref()).await;
    if let Reach::Serving(_, eq) = &reach_kind {
        return match &eq.bypass {
            Some(bypass) => {
                say(bypass, eq.levels.as_ref(), json, true);
                ExitCode::SUCCESS
            }
            None => {
                Failure::of(Kind::DaemonTooOld, "the daemon does not serve the bypass").report()
            }
        };
    }
    let why = match reach_kind {
        Reach::NoComputer => "the daemon's host audio is off",
        Reach::Silent => "the daemon did not answer, so whether it plays now is unknown",
        _ => "no daemon is running, so nothing says whether it plays now",
    };
    let saved = saved_bypass();
    match json {
        true => println!("{}", json_with(Some(&saved), None, Some(why))),
        false => println!("{}", unknown_line(&saved, why)),
    }
    ExitCode::SUCCESS
}

/// Whether the daemon that answered with `view` serves the bypass: an older one does not, and is
/// told so in the words the bare form uses rather than by its refusal of the op. No view (the
/// host audio is off) is left to the op's own words.
fn serves_bypass(view: Option<&EqView>) -> Result<(), Failure> {
    match view {
        Some(EqView { bypass: None, .. }) => Err(Failure::of(
            Kind::DaemonTooOld,
            "the daemon does not serve the bypass",
        )),
        _ => Ok(()),
    }
}

/// `svolder host bypass on|off`: through the daemon, then the answer as it shows now.
async fn change(g: &Globals<'_>, json: bool, on: bool) -> ExitCode {
    let socket = match serving_daemon(g).await {
        Ok(socket) => socket,
        Err(failure) => return failure.report(),
    };
    if let Err(failure) = serves_bypass(eq_view(&socket).await.as_ref()) {
        return failure.report();
    }
    if let Err(failure) = eq_op(&socket, HostEqOp::Bypass { on }).await {
        return failure.report();
    }
    match eq_view(&socket).await {
        Some(EqView {
            bypass: Some(bypass),
            levels,
            ..
        }) => say(&bypass, levels.as_ref(), json, false),
        _ => match json {
            true => println!("{}", json_of(None, None)),
            false => eprintln!("svolder: bypass {}", if on { "on" } else { "off" }),
        },
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests;
