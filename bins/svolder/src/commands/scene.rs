// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder scene [list | show NAME | save NAME [--force] | load NAME [--yes] | rename OLD NEW |
//! delete NAME [--yes]] [--json]`: the scenes of sound settings, through the daemon.
//!
//! The daemon keeps the scenes (they span the computer and several devices), so every verb is
//! one `Request::Scene` to it, and `list` and `show` read the [`ScenesView`] it sends with every
//! subscription. This side judges nothing: which scenes there are, which parts each holds, which
//! one is on, what a name may be and why something is refused are the daemon's words, printed as
//! they come. With no daemon, `list` and `show` read the scenes from the configuration `run`
//! already loaded, naming the parts by the one rule the daemon uses
//! ([`svolder_ipc::scenes::Scene::part_names`]) and showing nothing only the daemon knows (which
//! scene is on, which part is connected); every other verb says no daemon runs. A daemon that
//! does not list the `scenes` feature is told nothing.
//!
//! A recall that would play one preset on a device and on the computer at once is answered with a
//! question: `--yes` answers it (the daemon's message is then a note), a terminal is asked, and
//! anything else is refused with the daemon's words; a no changes nothing and exits 0, as
//! `eq load` does. `delete` asks the same way, after looking the name up, and a no exits 1. A
//! recall prints one line per part and exits 3 when a part is `partial` or `failed`: the exit code
//! of a write the daemon relays that did not come about, as `preset load` gives when a device
//! refuses or drops a write.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use futures::StreamExt;
use svolder_ipc::socket::daemon_version::{VERSION_WAIT, version_on};
use svolder_ipc::{
    CODE_USAGE, Client, Config, DEFAULT_REQUEST_TIMEOUT, Event, FEATURE_SCENES, IpcError,
    PartOutcome, PartStatus, Request, Response, SceneOp, SceneRow, ScenesView, socket,
};
use svolder_profiles::tone::preset::same_name;

use crate::backend::Globals;
use crate::cli::SceneCommand;
use crate::recovery::{Failure, Kind};

/// How long a recall, which writes to every connected device, waits for the daemon's answer.
const WAIT: Duration = DEFAULT_REQUEST_TIMEOUT;
/// How long the view of the scenes is waited for after a subscription opens.
const VIEW_WAIT: Duration = Duration::from_secs(10);
/// The exit code of a recall that left a part `partial` or `failed`: [`Kind::DaemonError`]'s.
const CODE_NOT_SET: u8 = 3;

/// What a verb says: lines for stdout, notes for stderr, and the exit code.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Answer {
    pub(crate) out: Vec<String>,
    pub(crate) err: Vec<String>,
    pub(crate) code: u8,
}

impl Answer {
    fn said(out: Vec<String>) -> Answer {
        Answer {
            out,
            err: Vec::new(),
            code: 0,
        }
    }
}

/// How a verb that needs a yes gets one: whether stdin is a terminal, and the asking of it.
pub(crate) struct Asking<'a> {
    pub(crate) terminal: bool,
    pub(crate) ask: &'a mut dyn FnMut(&str) -> std::io::Result<bool>,
}

/// `svolder scene [--json] [VERB]`.
pub(crate) async fn run(g: &Globals<'_>, json: bool, command: Option<SceneCommand>) -> ExitCode {
    let config = match Config::load() {
        Ok(config) => config,
        Err(e) => return Failure::of(Kind::ConfigUnreadable, e.to_string()).report(),
    };
    let socket = socket::path::resolve(g.socket, config.socket.as_deref());
    let mut ask = crate::commands::device::ask;
    let mut asking = Asking {
        terminal: std::io::IsTerminal::is_terminal(&std::io::stdin()),
        ask: &mut ask,
    };
    match answer(&socket, &config, json, command, &mut asking).await {
        Ok(answer) => {
            answer.out.iter().for_each(|line| println!("{line}"));
            answer
                .err
                .iter()
                .for_each(|line| eprintln!("svolder: {line}"));
            ExitCode::from(answer.code)
        }
        Err(failure) => failure.report(),
    }
}

/// What `command` (`list`, with none) does and says over the daemon on `socket`.
pub(crate) async fn answer(
    socket: &Path,
    config: &Config,
    json: bool,
    command: Option<SceneCommand>,
    asking: &mut Asking<'_>,
) -> Result<Answer, Failure> {
    let command = command.unwrap_or(SceneCommand::List);
    if !gate(socket).await? {
        return match command {
            SceneCommand::List => Ok(from_config(config, json)),
            SceneCommand::Show { name } => from_config_one(config, &name, json),
            _ => Err(no_daemon()),
        };
    }
    match command {
        SceneCommand::List => Ok(listed(&view(socket).await?, json)),
        SceneCommand::Show { name } => shown(&view(socket).await?, &name, json),
        SceneCommand::Save { name, force } => {
            let (name, outcome) = scene_answer(socket, SceneOp::Save { name, force }).await?;
            Ok(saved(&name, &outcome, json))
        }
        SceneCommand::Load { name, yes } => load(socket, name, yes, json, asking).await,
        SceneCommand::Rename { old, new } => {
            let was = old.trim().to_string();
            let op = SceneOp::Rename { from: old, to: new };
            let (name, _) = scene_answer(socket, op).await?;
            Ok(named(json, &name, format!("Renamed {was:?} to {name:?}.")))
        }
        SceneCommand::Delete { name, yes } => {
            let refusal = "refusing to delete a scene without confirmation; pass --yes";
            if !yes {
                if !asking.terminal {
                    return Err(refused(refusal));
                }
                // The question names the scene as the daemon spells it, and an unknown name is
                // refused before anything is asked.
                let view = view(socket).await?;
                let spelled = find(&view, &name)?.name.clone();
                let question = format!("Delete scene {spelled:?}? [y/N]");
                if !agreed(false, &question, refusal, asking)? {
                    return Ok(cancelled());
                }
            }
            let (name, _) = scene_answer(socket, SceneOp::Delete { name }).await?;
            Ok(named(json, &name, format!("Deleted {name:?}.")))
        }
    }
}

/// The failure of a verb that needs a daemon when none runs.
fn no_daemon() -> Failure {
    Failure {
        code: Kind::NoDaemon.code(),
        what: "scenes are kept by the daemon, and none is running".to_string(),
        next: "Start it with `svolder daemon install`, then try again".to_string(),
        reason: None,
        adapter_hint: None,
    }
}

/// A refusal in `words` alone, with the usage exit code.
fn refused(words: &str) -> Failure {
    Failure {
        code: CODE_USAGE,
        what: words.to_string(),
        next: String::new(),
        reason: None,
        adapter_hint: None,
    }
}

/// Whether a daemon answers on `socket`: `Ok(false)` when none does, a failure for one that
/// cannot be tried, does not say what it is, or keeps no scenes (no scene request is sent behind
/// it).
async fn gate(socket: &Path) -> Result<bool, Failure> {
    let mut client = match Client::connect_if_any(socket).await {
        Ok(Some(client)) => client,
        Ok(None) => return Ok(false),
        Err(why) => {
            return Err(Failure::of(
                Kind::Usage,
                format!(
                    "cannot reach the daemon's socket {}: {why}",
                    socket.display()
                ),
            ));
        }
    };
    let info = match tokio::time::timeout(VERSION_WAIT, version_on(&mut client)).await {
        Ok(Ok(reply)) => reply.info,
        Ok(Err(_)) | Err(_) => {
            return Err(Failure::of(
                Kind::DaemonError,
                format!(
                    "the daemon on {} did not say what it is: svolder daemon status",
                    socket.display()
                ),
            ));
        }
    };
    match info.supports(FEATURE_SCENES) {
        true => Ok(true),
        false => Err(Failure::of(
            Kind::DaemonTooOld,
            format!("this daemon ({}) has no scenes", info.version),
        )),
    }
}

/// The daemon's view of the scenes: the first `scenes` event of a subscription.
async fn view(socket: &Path) -> Result<ScenesView, Failure> {
    let client = Client::connect(socket)
        .await
        .map_err(|e| Failure::from_ipc(&e))?;
    let mut events = client
        .subscribe(None)
        .await
        .map_err(|e| Failure::from_ipc(&e))?;
    let first = tokio::time::timeout(VIEW_WAIT, async {
        while let Some(event) = events.next().await {
            if let Event::Scenes(view) = event {
                return Some(view);
            }
        }
        None
    })
    .await
    .map_err(|_| {
        Failure::of(
            Kind::DaemonError,
            "the daemon did not send its scenes in time",
        )
    })?;
    first.ok_or_else(|| Failure::of(Kind::DaemonStopped, crate::recovery::DAEMON_STOPPED))
}

/// Send `op` on a connection of its own: the daemon's answer, or its refusal in its words.
async fn send(socket: &Path, op: SceneOp) -> Result<Response, Failure> {
    let refused = |answer: Response| match answer {
        Response::Error {
            code,
            message,
            reason,
        } => Failure::from_daemon_on(socket, code, message, reason),
        other => Failure::of(Kind::DaemonError, format!("the daemon answered {other:?}")),
    };
    match svolder_ipc::request_on_own_connection(socket, Request::Scene { op }, Some(WAIT)).await {
        Ok(answer @ Response::Error { .. }) => Err(refused(answer)),
        Ok(answer) => Ok(answer),
        Err(IpcError::Refused(answer)) => Err(refused(*answer)),
        Err(e) => Err(Failure::from_ipc(&e)),
    }
}

/// The scene's name as the daemon spells it, and how each part fared, from a plain answer.
async fn scene_answer(socket: &Path, op: SceneOp) -> Result<(String, Vec<PartOutcome>), Failure> {
    match send(socket, op).await? {
        Response::Scene { name, outcome } => Ok((name, outcome)),
        other => Err(Failure::of(
            Kind::DaemonError,
            format!("the daemon answered {other:?}"),
        )),
    }
}

/// Whether the user said yes to `question`: `yes` is an answer already, a terminal is asked, and
/// anything else is refused with `refusal`, the daemon's words (which `device::confirm`'s fixed
/// sentence cannot carry).
fn agreed(
    yes: bool,
    question: &str,
    refusal: &str,
    asking: &mut Asking<'_>,
) -> Result<bool, Failure> {
    if yes {
        return Ok(true);
    }
    if !asking.terminal {
        return Err(refused(refusal));
    }
    (asking.ask)(question).map_err(|e| Failure::of(Kind::Usage, e.to_string()))
}

/// What a no to a delete leaves: nothing was done, which is not what was asked, so it exits 1 as
/// a declined `eq delete` does.
fn cancelled() -> Answer {
    Answer {
        out: Vec::new(),
        err: vec!["cancelled".to_string()],
        code: 1,
    }
}

/// What a no to the both-sides question leaves: a declined recall is not a failure.
fn nothing_changed() -> Answer {
    Answer {
        out: Vec::new(),
        err: vec!["nothing changed".to_string()],
        code: 0,
    }
}

/// `load`: send the recall, answer the daemon's question once, say how each part fared.
async fn load(
    socket: &Path,
    name: String,
    yes: bool,
    json: bool,
    asking: &mut Asking<'_>,
) -> Result<Answer, Failure> {
    let mut confirm = false;
    let mut notes = Vec::new();
    loop {
        let op = SceneOp::Recall {
            name: name.clone(),
            confirm,
        };
        match send(socket, op).await? {
            Response::Scene { name, outcome } => {
                let mut said = recalled(&name, &outcome, json);
                notes.append(&mut said.err);
                said.err = notes;
                return Ok(said);
            }
            Response::Ask { message, .. } if !confirm => {
                let question = format!("{message} Recall it anyway? [y/N]");
                let refusal = format!("{message} Pass --yes to recall it anyway");
                if !agreed(yes, &question, &refusal, asking)? {
                    return Ok(nothing_changed());
                }
                if yes {
                    notes.push(message);
                }
                confirm = true;
            }
            other => {
                return Err(Failure::of(
                    Kind::DaemonError,
                    format!("the daemon answered {other:?}"),
                ));
            }
        }
    }
}

/// The word for a part's status.
fn status_word(status: PartStatus) -> &'static str {
    match status {
        PartStatus::Done => "done",
        PartStatus::Partial => "partial",
        PartStatus::Left => "left",
        PartStatus::Failed => "failed",
        PartStatus::Unknown => "unknown",
    }
}

/// One line per part, `who: status`, each note under it.
fn part_lines(outcome: &[PartOutcome]) -> Vec<String> {
    outcome
        .iter()
        .flat_map(|part| {
            let head = format!("{}: {}", part.who, status_word(part.status));
            std::iter::once(head).chain(part.notes.iter().map(|note| format!("  {note}")))
        })
        .collect()
}

/// A recall's answer: the parts, and exit 3 when one is not set whole (`partial`, `failed`, or a
/// status this build cannot read).
fn recalled(name: &str, outcome: &[PartOutcome], json: bool) -> Answer {
    let unset: Vec<&str> = outcome
        .iter()
        .filter(|part| {
            matches!(
                part.status,
                PartStatus::Partial | PartStatus::Failed | PartStatus::Unknown
            )
        })
        .map(|part| part.who.as_str())
        .collect();
    let out = match json {
        true => vec![json_of(
            &serde_json::json!({ "name": name, "outcome": outcome }),
        )],
        false => {
            let mut lines = vec![format!("Recalled {name:?}.")];
            lines.extend(part_lines(outcome));
            lines
        }
    };
    match unset.is_empty() {
        true => Answer::said(out),
        false => Answer {
            out,
            err: vec![format!("{name:?} was not set whole: {}", unset.join(", "))],
            code: CODE_NOT_SET,
        },
    }
}

/// A save's answer: the name, and each part left out as a note.
fn saved(name: &str, outcome: &[PartOutcome], json: bool) -> Answer {
    match json {
        true => Answer::said(vec![json_of(
            &serde_json::json!({ "name": name, "outcome": outcome }),
        )]),
        false => Answer {
            out: vec![format!("Saved {name:?}.")],
            err: outcome
                .iter()
                .flat_map(|part| part.notes.iter().cloned())
                .collect(),
            code: 0,
        },
    }
}

/// A rename's or delete's answer.
fn named(json: bool, name: &str, line: String) -> Answer {
    match json {
        true => Answer::said(vec![json_of(&serde_json::json!({ "name": name }))]),
        false => Answer::said(vec![line]),
    }
}

/// The parts of a scene as one line, each part that cannot be recalled now marked.
fn parts_line(row: &SceneRow) -> String {
    let parts: Vec<String> = row
        .parts
        .iter()
        .map(|part| match part.present {
            true => part.who.clone(),
            false => format!("{} (not connected)", part.who),
        })
        .collect();
    parts.join(" · ")
}

/// `list`: one line per scene, `*` before the one that is on.
fn listed(view: &ScenesView, json: bool) -> Answer {
    if json {
        return Answer::said(vec![json_of(view)]);
    }
    let mut answer = Answer::said(match view.scenes.is_empty() {
        true => vec!["No scenes yet. Save one with `svolder scene save NAME`.".to_string()],
        false => view
            .scenes
            .iter()
            .map(|row| {
                let lamp = match view.on.as_deref() == Some(row.name.as_str()) {
                    true => '*',
                    false => ' ',
                };
                format!("{lamp} {}  {}", row.name, parts_line(row))
            })
            .collect(),
    });
    if !view.known && !view.scenes.is_empty() {
        answer.err.push(NOT_KNOWN.to_string());
    }
    answer
}

/// The scene of `view` that `name` names, in any case, or the refusal that says there is none.
fn find<'v>(view: &'v ScenesView, name: &str) -> Result<&'v SceneRow, Failure> {
    view.scenes
        .iter()
        .find(|row| same_name(&row.name, name))
        .ok_or_else(|| no_such_scene(name))
}

/// The refusal for a name no scene has.
fn no_such_scene(name: &str) -> Failure {
    Failure {
        code: CODE_USAGE,
        what: format!("no scene named {:?}", name.trim()),
        next: "List them with `svolder scene list`".to_string(),
        reason: None,
        adapter_hint: None,
    }
}

/// The note that says which scene is on is not known yet.
const NOT_KNOWN: &str = "which scene is on is not known yet: a device is still being read";

/// `show NAME`: the one scene's row, and whether it is on.
fn shown(view: &ScenesView, name: &str, json: bool) -> Result<Answer, Failure> {
    let row = find(view, name)?;
    let on = view.on.as_deref() == Some(row.name.as_str());
    let mut answer = match json {
        true => {
            let mut value = serde_json::to_value(row).unwrap_or_default();
            value["on"] = serde_json::Value::Bool(on);
            value["known"] = serde_json::Value::Bool(view.known);
            Answer::said(vec![json_of(&value)])
        }
        false => {
            let head = match on {
                true => format!("{} (on)", row.name),
                false => row.name.clone(),
            };
            let mut lines = vec![head];
            lines.extend(row.parts.iter().map(|part| match part.present {
                true => format!("  {}", part.who),
                false => format!("  {} (not connected)", part.who),
            }));
            Answer::said(lines)
        }
    };
    if !view.known {
        answer.err.push(NOT_KNOWN.to_string());
    }
    Ok(answer)
}

/// The note a read of the config alone carries.
const FROM_CONFIG: &str =
    "read from the config, with no daemon running: which scene is on is not known";

/// `list` with no daemon: the config's scenes, the parts named as the daemon names them, no lamp
/// and no connection said.
fn from_config(config: &Config, json: bool) -> Answer {
    let rows: Vec<(String, Vec<String>)> = config
        .scenes
        .iter()
        .map(|scene| (scene.name.clone(), scene.part_names(&config.devices)))
        .collect();
    let mut answer = match json {
        true => {
            let scenes: Vec<_> = rows.iter().map(|(name, who)| bare_row(name, who)).collect();
            Answer::said(vec![json_of(
                &serde_json::json!({ "scenes": scenes, "known": false }),
            )])
        }
        false => Answer::said(match rows.is_empty() {
            true => vec!["No scenes yet. Save one with `svolder scene save NAME`.".to_string()],
            false => rows
                .iter()
                .map(|(name, who)| format!("{name}  {}", who.join(" · ")))
                .collect(),
        }),
    };
    answer.err.push(FROM_CONFIG.to_string());
    answer
}

/// `show NAME` with no daemon.
fn from_config_one(config: &Config, name: &str, json: bool) -> Result<Answer, Failure> {
    let scene = config.scene(name).ok_or_else(|| no_such_scene(name))?;
    let who = scene.part_names(&config.devices);
    let mut answer = match json {
        true => Answer::said(vec![json_of(&bare_row(&scene.name, &who))]),
        false => {
            let mut lines = vec![scene.name.clone()];
            lines.extend(who.iter().map(|who| format!("  {who}")));
            Answer::said(lines)
        }
    };
    answer.err.push(FROM_CONFIG.to_string());
    Ok(answer)
}

/// A scene as JSON with only what the config knows: its name and who each part is.
fn bare_row(name: &str, who: &[String]) -> serde_json::Value {
    let parts: Vec<_> = who
        .iter()
        .map(|who| serde_json::json!({ "who": who }))
        .collect();
    serde_json::json!({ "name": name, "parts": parts })
}

/// `value` as one line of JSON.
fn json_of<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

#[cfg(test)]
mod tests;
