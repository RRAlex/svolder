// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder host check`: whether the computer's side of the path lets a player reach the sink
//! untouched, and the seven conditions of the PipeWire config behind it.
//!
//! The route comes from the daemon when one answers with one; else this command reads the graph
//! itself, once, through `pw-dump`, and the card's `/proc/asound` files. It reads and changes
//! nothing. A sink that is one of the daemon's DACs gets the DAC's verdict ([`path_line`], with
//! the readings the daemon holds); any other sink gets the computer's alone. Every word printed
//! is the judges' in `svolder-ipc` ([`computer_line`], [`setup_check`]); this module works none
//! out.

use std::path::Path;
use std::process::ExitCode;

use svolder_host::{Asound, PlayerMatch, PwDump, RouteFacts, fill_alsa, route};
use svolder_ipc::host::{
    DacFacts, DacStage, PathLine, SETUP_LIMIT, SetupCheck, computer_line, path_line, setup_check,
    sink_is_dac,
};
use svolder_ipc::{CODE_COMPUTER_UNAVAILABLE, Client, Config, Request, Response};
use svolder_profiles::Family;

use super::host_crossfeed::Answer;
use crate::backend::{ASK_WAIT, Globals, asks_a_daemon, launch};
use crate::recovery::Failure;

#[cfg(test)]
mod tests;

/// `svolder host check [--json] [--player NAME]`.
pub(crate) async fn run(g: &Globals<'_>, json: bool, player: Option<String>) -> ExitCode {
    let config = Config::load().ok();
    match answer(
        g,
        config.as_ref(),
        json,
        player.as_deref(),
        PwDump::from_env(),
        &Asound::from_env(),
    )
    .await
    {
        Ok(answer) => {
            answer.out.iter().for_each(|line| println!("{line}"));
            ExitCode::SUCCESS
        }
        Err(failure) => failure.report(),
    }
}

/// What the check prints. A daemon that answers gives the DAC readings, and its route unless a
/// player was named (the daemon's route is of the player it follows); else `dump` is read once
/// and the card's files in `asound` are added to it.
async fn answer(
    g: &Globals<'_>,
    config: Option<&Config>,
    json: bool,
    player: Option<&str>,
    dump: Option<PwDump>,
    asound: &Asound,
) -> Result<Answer, Failure> {
    let socket = daemon_socket(g, config).await;
    let from_daemon = match (&socket, player) {
        (Some(socket), None) => route_on(socket).await,
        _ => None,
    };
    let dacs = match &socket {
        Some(socket) => dacs_on(socket).await,
        None => Vec::new(),
    };
    let off = dump.is_none();
    let route = match from_daemon {
        Some(route) => route,
        None => own_route(dump, asound, player)
            .await
            .ok_or_else(|| unreadable(off))?,
    };
    Ok(Answer {
        out: lines(&route, &dacs, json),
        err: Vec::new(),
    })
}

/// The socket of a daemon that answers, when this transport asks one. Nothing is started.
async fn daemon_socket(g: &Globals<'_>, config: Option<&Config>) -> Option<std::path::PathBuf> {
    let config = config.filter(|_| asks_a_daemon(g.transport))?;
    launch::probe(g, config).await
}

/// The route the daemon on `socket` shows; `None` when it shows none or does not answer in time.
pub(super) async fn route_on(socket: &Path) -> Option<RouteFacts> {
    tokio::time::timeout(ASK_WAIT, async {
        let mut client = Client::connect(socket).await.ok()?;
        match client.request(Request::Host).await.ok()? {
            Response::Host(view) => view.route,
            _ => None,
        }
    })
    .await
    .ok()
    .flatten()
}

/// What each DAC the daemon holds reports of its end of the path: the devices whose profile is a
/// DAC's, each with its readings. The device's own stage is not claimed while its link is down:
/// the readings held then are from before the drop. Empty when the daemon does not answer in
/// time or holds none.
async fn dacs_on(socket: &Path) -> Vec<DacFacts> {
    let asked = tokio::time::timeout(ASK_WAIT, async {
        let registry = crate::registry().ok()?;
        let mut client = Client::connect(socket).await.ok()?;
        let Response::Devices { devices } = client.request(Request::Devices).await.ok()? else {
            return None;
        };
        let mut dacs = Vec::new();
        for device in devices {
            let Some(profile) = registry
                .get(&device.profile)
                .filter(|profile| profile.family == Family::Dac)
            else {
                continue;
            };
            let ask = Request::State {
                device: Some(device.name.clone()),
            };
            let Ok(Response::Values { values, .. }) = client.request(ask).await else {
                continue;
            };
            let mut facts = DacFacts::read(profile, |id| {
                let raw = &values.iter().find(|v| v.id == id.to_string())?.raw;
                raw.as_i64().or_else(|| raw.as_bool().map(i64::from))
            });
            if !device.connected {
                facts.stage = DacStage::default();
            }
            dacs.push(facts);
        }
        Some(dacs)
    })
    .await;
    asked.ok().flatten().unwrap_or_default()
}

/// The route read from the graph `dump` prints, with the card's files in `asound`; `None` when
/// there is no `pw-dump` to run, it fails, or what it prints is not a dump.
async fn own_route(
    dump: Option<PwDump>,
    asound: &Asound,
    player: Option<&str>,
) -> Option<RouteFacts> {
    let text = dump?.read().await?;
    let (pid, identity) = player.map(player_of).unwrap_or_default();
    let mut facts = route(&text, pid, &identity)?;
    // With no `--player` MPRIS was never looked at: a lone stream is the assumed player's, not
    // one known to have no MPRIS player.
    if player.is_none() && facts.player_match == PlayerMatch::NoMpris {
        facts.player_match = PlayerMatch::Assumed;
    }
    fill_alsa(&mut facts, asound);
    Some(facts)
}

/// A `--player` value as the route matches it: a number is a process id, anything else an
/// application name.
fn player_of(player: &str) -> (Option<u32>, String) {
    let player = player.trim();
    match player.parse::<u32>() {
        Ok(pid) => (Some(pid), String::new()),
        Err(_) => (None, player.to_string()),
    }
}

/// The failure for a graph that cannot be read: the computer's controls are unavailable. `off`
/// is a process with no `pw-dump` to run (host audio is off for it).
fn unreadable(off: bool) -> Failure {
    let (what, next) = match off {
        true => (
            "host audio is off for this process, so the computer's audio graph is not read",
            "Run it without SVOLDER_HOST=off or fake, then try again",
        ),
        false => (
            "the computer's audio graph cannot be read",
            "Check that PipeWire runs and `pw-dump` works, then try again",
        ),
    };
    Failure {
        code: CODE_COMPUTER_UNAVAILABLE,
        what: what.to_string(),
        next: next.to_string(),
        reason: None,
    }
}

/// The lines the check prints for `route`: the text, or with `json` one object. The path is the
/// DAC's verdict when the sink is one of `dacs`, else the computer's alone.
fn lines(route: &RouteFacts, dacs: &[DacFacts], json: bool) -> Vec<String> {
    let dac = route
        .sink
        .as_ref()
        .and_then(|sink| dacs.iter().find(|dac| sink_is_dac(sink, dac)));
    let path = match dac {
        Some(dac) => path_line(route, dac),
        None => computer_line(route),
    };
    let check = setup_check(route);
    match json {
        true => vec![as_json(&path, &check, dac).to_string()],
        false => text(&path, &check),
    }
}

/// The path and its hops (or, with none, the sink the config was judged on), the summary, the
/// seven lines, then the limit.
fn text(path: &PathLine, check: &SetupCheck) -> Vec<String> {
    let mut out = Vec::new();
    out.push(match path.hops.is_empty() {
        true => path.words_line(),
        false => format!("player → computer → {}: {}", check.sink, path.words_line()),
    });
    out.extend(
        path.hops
            .iter()
            .filter(|hop| !hop.detail.is_empty())
            .map(|hop| format!("  {}: {}", hop.label, hop.detail)),
    );
    out.extend(path.sentence().map(|sentence| format!("  {sentence}")));
    if path.hops.is_empty() && !check.sink.is_empty() {
        out.push(format!("  config judged on: {}", check.sink));
    }
    out.push(check.summary_words());
    out.push(format!("  {:<23}{}", "From player", check.io.from_player));
    out.push(format!("  {:<23}{}", "To DAC", check.io.to_dac));
    out.extend(check.lines.iter().map(|line| {
        format!(
            "  {:<23}{:<9}{}",
            line.label(),
            line.outcome.key(),
            line.words
        )
    }));
    out.extend(wrapped(SETUP_LIMIT, LIMIT_WIDTH).map(|line| format!("  {line}")));
    out
}

/// How wide the limit sentence's lines are, before their indent.
const LIMIT_WIDTH: usize = 90;

/// `text` broken at its blanks into lines of at most `width` characters (a longer word has a line
/// of its own).
fn wrapped(text: &str, width: usize) -> impl Iterator<Item = String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines.into_iter()
}

/// `{ sink, path: { verdict, end_to_end, words, sentence, dac }, config: { sink, outcome,
/// summary, from_player, to_dac, limit, lines } }`; the keys are svolder-ipc's, as on every JSON
/// surface, and `words` is the list the judge made. `dac` is the matched DAC's display name, else
/// null; the top-level `sink` is the config's, kept for scripts.
fn as_json(path: &PathLine, check: &SetupCheck, dac: Option<&DacFacts>) -> serde_json::Value {
    serde_json::json!({
        "sink": check.sink,
        "path": {
            "verdict": path.verdict.key(),
            "end_to_end": path.end_to_end().key(),
            "words": path.words,
            "sentence": path.sentence(),
            "dac": dac.map(|dac| dac.display.as_str()),
        },
        "config": {
            "sink": check.sink,
            "outcome": check.summary.key(),
            "summary": check.summary_words(),
            "from_player": check.io.from_player,
            "to_dac": check.io.to_dac,
            "limit": SETUP_LIMIT,
            "lines": check.lines,
        },
    })
}
