// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder host speakers`: the virtual speakers on what the computer plays, and their sets,
//! through the daemon.
//!
//! The daemon owns the speakers (it watches the output and resets them) and the sets (it is the
//! one process that reaches their sources), so every verb is a request to it: `on`, `off` and
//! `use` are one `HostEqOp::Speakers` each, `list`, `fetch`, `import` and `forget` are the sets
//! family's. The choices' names and technical terms, the labels and the licences come from the
//! daemon's view and lists (or, for a list's choice column, from the one table in
//! `svolder-host`); this module holds none of its own. Without a daemon, `show` reads the saved
//! state and every verb is refused.

use std::process::ExitCode;
use std::sync::Arc;

use svolder_host::eq::speakers::Room;
use svolder_host::eq::{CrossfeedView, SpeakersView};
use svolder_ipc::socket::daemon_version::RESTART_COMMAND;
use svolder_ipc::{
    CODE_COMPUTER_UNAVAILABLE, Config, HostEqOp, Reason, SPEAKERS_OFF_PREFIX, SpeakersProgress,
    SpeakersRoom, SpeakersSaved, SpeakersSet,
};

use super::host_crossfeed::{Answer, Reach, needs_a_daemon, reach, usage};
use super::host_eq::{eq_op, eq_view};
use crate::backend::{Globals, asks_a_daemon, launch};
use crate::cli::{HostSpeakersCommand, RoomArg};
use crate::commands::interrupts::Presses;
use crate::recovery::{Failure, Kind};
use crate::speakers::{self, Refused};

/// Why `show` has only the saved state when a daemon answers with no computer EQ.
const HOST_IS_OFF: &str = "the daemon's host audio is off";
/// Why `show` has only the saved state when the daemon did not answer in time.
const NO_ANSWER_UNKNOWN: &str = "the daemon did not answer, so whether they play now is unknown";
/// Why `show` cannot say whether they play when no daemon runs: a TUI without a daemon may be
/// playing them.
const NO_DAEMON_UNKNOWN: &str = "no daemon is running, so whether they play now is unknown";
/// The exit code of a command a signal ended, as a shell reports it.
const INTERRUPTED: u8 = 130;

/// What the state file keeps of the speakers, which `show` reads when no daemon serves them.
#[derive(Debug, Default)]
struct Saved {
    /// The chosen set's id.
    set: Option<String>,
    /// Whether they were playing when it was written.
    on: bool,
}

/// The chosen set in effect at the state file `path` and its switch: the state's, else the set
/// kept before any state existed ([`svolder_host::eq::HostEqSettings::effective`]).
fn saved_at(path: &std::path::Path) -> Saved {
    let on = svolder_host::eq::HostEqState::load(path)
        .ok()
        .flatten()
        .is_some_and(|state| state.speakers_on);
    if let Some(why) = svolder_host::eq::HostEqSettings::unreadable(path) {
        eprintln!("svolder: ignoring the kept computer EQ settings: {why}");
    }
    Saved {
        set: svolder_host::eq::HostEqSettings::effective(path).speakers,
        on,
    }
}

/// `svolder host speakers [--json] [VERB]`.
pub(crate) async fn run(
    g: &Globals<'_>,
    json: bool,
    command: Option<HostSpeakersCommand>,
) -> ExitCode {
    let config = Config::load().map_err(|e| e.to_string());
    let saved = saved_at(&super::host_eq::state_path());
    // Ctrl-C ends a fetch or an import by cancelling its job on the daemon.
    let signals = matches!(
        command,
        Some(HostSpeakersCommand::Fetch { .. } | HostSpeakersCommand::Import { .. })
    )
    .then(crate::wizard::signals::Signals::listen);
    let presses = signals
        .as_ref()
        .map(crate::wizard::signals::Signals::presses);
    let mut progress = |line: &str| eprintln!("svolder: {line}");
    match answer(
        g,
        config.as_ref().map_err(String::as_str),
        saved,
        json,
        command,
        presses,
        &mut progress,
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
        Err(failure) => {
            // Past the terminal's `^C`.
            if failure.code == INTERRUPTED {
                eprintln!();
            }
            failure.report()
        }
    }
}

/// What `command` (show, with none) does and says. `progress` hears the lines a download prints
/// as it goes; `presses` are the signals that cancel a fetch or an import.
async fn answer(
    g: &Globals<'_>,
    config: Result<&Config, &str>,
    saved: Saved,
    json: bool,
    command: Option<HostSpeakersCommand>,
    presses: Option<Arc<Presses>>,
    progress: &mut (dyn FnMut(&str) + Send),
) -> Result<Answer, Failure> {
    // An unreadable configuration is reported for a verb; `show` still has the saved state.
    let config = match (config, &command) {
        (Ok(config), _) => Some(config),
        (Err(why), Some(verb)) if !matches!(verb, HostSpeakersCommand::Show) => {
            return Err(Failure::of(Kind::ConfigUnreadable, why));
        }
        (Err(_), _) => None,
    };
    match command {
        None | Some(HostSpeakersCommand::Show) => show(g, config, saved, json).await,
        Some(HostSpeakersCommand::On) => change(g, config, json, None, Some(true)).await,
        Some(HostSpeakersCommand::Off) => change(g, config, json, None, Some(false)).await,
        Some(HostSpeakersCommand::Use { id, on }) => {
            change(g, config, json, Some(id), on.then_some(true)).await
        }
        Some(HostSpeakersCommand::List { room }) => {
            let socket = sets_daemon(g, config).await?;
            let mut found = speakers::catalogue(&socket).await.map_err(refusal)?;
            if let Some(room) = room {
                found.sets.retain(|set| set.room == room_of(room));
            }
            Ok(match json {
                true => Answer {
                    out: vec![serde_json::to_string(&found).unwrap_or_default()],
                    err: Vec::new(),
                },
                false => listed(&found),
            })
        }
        Some(HostSpeakersCommand::Fetch { id }) => {
            let socket = sets_daemon(g, config).await?;
            let mut steps = Steps::default();
            let saved = speakers::fetch(&socket, &id, presses, &mut |report| {
                steps.line(report).iter().for_each(|line| progress(line));
            })
            .await
            .map_err(refusal)?;
            Ok(kept("fetched", &saved, json))
        }
        Some(HostSpeakersCommand::Import { file, label, room }) => {
            let socket = sets_daemon(g, config).await?;
            // The daemon reads the file, from its own working directory.
            let file = std::path::absolute(&file).unwrap_or(file);
            let saved = speakers::import(&socket, &file, label, room.map(room_of), presses)
                .await
                .map_err(refusal)?;
            Ok(kept("imported", &saved, json))
        }
        Some(HostSpeakersCommand::Forget { id }) => {
            let socket = sets_daemon(g, config).await?;
            speakers::forget(&socket, &id).await.map_err(refusal)?;
            Ok(match json {
                true => Answer {
                    out: vec![serde_json::json!({ "forgot": id }).to_string()],
                    err: Vec::new(),
                },
                false => Answer {
                    out: Vec::new(),
                    err: vec![format!("forgot {id}")],
                },
            })
        }
    }
}

/// The socket of the daemon that keeps the sets; nothing is started to find it.
async fn sets_daemon(
    g: &Globals<'_>,
    config: Option<&Config>,
) -> Result<std::path::PathBuf, Failure> {
    let socket = match config.filter(|_| asks_a_daemon(g.transport)) {
        Some(config) => launch::probe(g, config).await,
        None => None,
    };
    socket.ok_or_else(|| Failure {
        what: format!("{}, and none is running", speakers::NEEDS_DAEMON),
        ..needs_a_daemon()
    })
}

/// `show`: the state as the daemon sends it, or what the saved state can say without one.
async fn show(
    g: &Globals<'_>,
    config: Option<&Config>,
    saved: Saved,
    json: bool,
) -> Result<Answer, Failure> {
    let offline = |on: bool, why: &str| SpeakersView {
        on,
        set: saved.set.clone(),
        unavailable: Some(why.to_string()),
        ..SpeakersView::default()
    };
    let unknown = |why: &str| shown_unknown(&offline(saved.on, why), json);
    match reach(g, config).await {
        Reach::Serving(_, eq) => match eq.speakers {
            Some(view) => Ok(shown(&view, json)),
            None => Err(too_old()),
        },
        Reach::NoComputer => Ok(shown(&offline(false, HOST_IS_OFF), json)),
        Reach::Silent => Ok(unknown(NO_ANSWER_UNKNOWN)),
        Reach::NoDaemon => Ok(unknown(NO_DAEMON_UNKNOWN)),
    }
}

/// The daemon does not serve the speakers: it is older than this svolder.
fn too_old() -> Failure {
    Failure::of(
        Kind::DaemonTooOld,
        "the daemon does not serve the virtual speakers",
    )
}

/// `on`, `off` and `use`: one op to the computer EQ, then what the daemon shows after it.
async fn change(
    g: &Globals<'_>,
    config: Option<&Config>,
    json: bool,
    set: Option<String>,
    on: Option<bool>,
) -> Result<Answer, Failure> {
    let (socket, before) = match reach(g, config).await {
        Reach::Serving(socket, eq) => (socket, eq),
        Reach::NoComputer => {
            return Err(Failure {
                code: CODE_COMPUTER_UNAVAILABLE,
                what: "the daemon serves no computer EQ (host audio is off for it)".to_string(),
                next: "Restart it without SVOLDER_HOST=off, then try again".to_string(),
                reason: None,
                adapter_hint: None,
            });
        }
        Reach::Silent => {
            return Err(Failure::of(
                Kind::DaemonError,
                "the daemon did not answer in time",
            ));
        }
        Reach::NoDaemon => {
            return Err(Failure {
                what: "virtual speakers need svolderd, and none serves the computer EQ".to_string(),
                ..needs_a_daemon()
            });
        }
    };
    let Some(shown_before) = &before.speakers else {
        return Err(too_old());
    };
    if let Some(id) = &set
        && !shown_before.sets.iter().any(|fetched| fetched.id == *id)
    {
        return Err(not_fetched(id, shown_before));
    }
    eq_op(
        &socket,
        HostEqOp::Speakers {
            on,
            set: set.clone(),
            done: true,
        },
    )
    .await?;
    let after = eq_view(&socket).await.ok_or_else(|| {
        Failure::of(
            Kind::DaemonError,
            "the daemon did not show the virtual speakers after the change",
        )
    })?;
    let Some(view) = &after.speakers else {
        return Err(Failure::of(
            Kind::DaemonError,
            "the daemon did not show the virtual speakers after the change",
        ));
    };
    if json {
        return Ok(shown(view, true));
    }
    let mut err = Vec::new();
    if set.is_some() {
        err.push(format!(
            "virtual speakers set: {} \u{b7} {}",
            set_name(view),
            choice(view)
        ));
    }
    if on.is_some() || set.is_none() {
        err.push(format!("virtual speakers {}", on_word(view)));
    }
    if gave_way(before.crossfeed.as_ref(), after.crossfeed.as_ref()) {
        err.push("crossfeed off (it does not play with the virtual speakers)".to_string());
    }
    err.extend(view.note.iter().map(|note| format!("note: {note}")));
    Ok(Answer {
        out: Vec::new(),
        err,
    })
}

/// The refusal of `use` for a set the daemon does not hold fetched.
fn not_fetched(id: &str, view: &SpeakersView) -> Failure {
    let ids: Vec<_> = view.sets.iter().map(|set| set.id.as_str()).collect();
    usage(
        format!("no fetched set {id:?}"),
        match ids.is_empty() {
            true => "None is fetched: see `svolder host speakers list`, then \
                     `svolder host speakers fetch ID`"
                .to_string(),
            false => format!(
                "The fetched sets are {}; fetch another with `svolder host speakers fetch ID`",
                ids.join(", ")
            ),
        },
    )
}

/// Whether the crossfeed played before and does not now: turning the speakers on turned it off.
fn gave_way(before: Option<&CrossfeedView>, after: Option<&CrossfeedView>) -> bool {
    before.is_some_and(|before| before.on) && after.is_some_and(|after| !after.on)
}

/// What a refusal of the daemon's sets service becomes, with the step its reason calls for.
fn refusal(refused: Refused) -> Failure {
    if refused.interrupted {
        return Failure {
            code: INTERRUPTED,
            what: refused.message,
            next: String::new(),
            reason: None,
            adapter_hint: None,
        };
    }
    if refused.too_old {
        return Failure {
            next: format!("Run `{RESTART_COMMAND}`"),
            ..Failure::of(Kind::DaemonTooOld, refused.message)
        };
    }
    // A busy refusal's own words say what to do (turn them off, or wait for the job).
    let next = match refused.reason {
        Some(Reason::NotListed) => "See the ids with `svolder host speakers list`",
        // Fetching is off: waiting changes nothing.
        Some(Reason::Unreachable) if refused.message.starts_with(SPEAKERS_OFF_PREFIX) => {
            "Import your own file with `svolder host speakers import FILE`"
        }
        Some(Reason::Unreachable) => {
            "Retry later, or import your own file with `svolder host speakers import FILE`"
        }
        _ => "",
    };
    Failure {
        next: next.to_string(),
        reason: refused.reason,
        ..Failure::of(Kind::Usage, refused.message)
    }
}

/// The choice a flag names, as the wire carries it.
fn room_of(room: RoomArg) -> SpeakersRoom {
    match room {
        RoomArg::With => SpeakersRoom::With,
        RoomArg::No => SpeakersRoom::No,
    }
}

/// The choice with its technical term, as the table in `svolder-host` words them.
fn room_text(room: SpeakersRoom) -> String {
    let room = match room {
        SpeakersRoom::With => Room::With,
        SpeakersRoom::No => Room::No,
    };
    format!("{} \u{b7} {}", room.name(), room.term())
}

/// The prints of a download's progress: a first line when it starts, then each tenth of the
/// total, once.
#[derive(Default)]
struct Steps {
    started: bool,
    last: u64,
}

impl Steps {
    /// The lines for `report`: the start line for the first one (with the size when it is
    /// known), then one when it crosses the next tenth (also on the first report, when that is
    /// already past one); none while the total is unknown.
    fn line(&mut self, report: &SpeakersProgress) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.started {
            self.started = true;
            lines.push(match report.total {
                0 => format!("fetching {}", report.id),
                total => format!("fetching {} ({})", report.id, size_text(total)),
            });
        }
        if let Some(percent) = report
            .received
            .saturating_mul(100)
            .checked_div(report.total)
        {
            let tenth = percent.min(100) / 10 * 10;
            if tenth > self.last {
                self.last = tenth;
                lines.push(format!("fetching {}: {tenth} %", report.id));
            }
        }
        lines
    }
}

/// A download's size: whole kilobytes below a megabyte, else megabytes with a decimal.
fn size_text(bytes: u64) -> String {
    match bytes >= 1 << 20 {
        true => format!("{:.1} MB", bytes as f64 / (1 << 20) as f64),
        false => format!("{} kB", bytes.div_ceil(1 << 10)),
    }
}

/// `view` for `show`: its lines, or with `--json` the view as the daemon sends it.
fn shown(view: &SpeakersView, json: bool) -> Answer {
    let out = match json {
        true => vec![serde_json::to_string(view).unwrap_or_default()],
        false => lines(view),
    };
    Answer {
        out,
        err: Vec::new(),
    }
}

/// `show` for a machine whose daemon cannot say whether they play (none runs, or it is silent):
/// the first line says the live state is unknown and what the state file holds.
fn shown_unknown(view: &SpeakersView, json: bool) -> Answer {
    let mut answer = shown(view, json);
    if !json {
        let why = view.unavailable.as_deref().unwrap_or_default();
        answer.out[0] = format!(
            "virtual speakers: unknown: {why} (the saved state says {})",
            on_word(view)
        );
    }
    answer
}

fn on_word(view: &SpeakersView) -> &'static str {
    match view.on {
        true => "on",
        false => "off",
    }
}

/// The chosen set as the view names it: its label and id, or the id alone before a daemon told
/// the label.
fn set_name(view: &SpeakersView) -> String {
    match (&view.set, view.label.is_empty()) {
        (Some(id), false) => format!("{} ({id})", view.label),
        (Some(id), true) => id.clone(),
        (None, _) => "none chosen".to_string(),
    }
}

/// The chosen choice with its technical term, in the view's own words.
fn choice(view: &SpeakersView) -> String {
    match (&view.room_name, &view.term) {
        (Some(name), Some(term)) => format!("{name} \u{b7} {term}"),
        (Some(name), None) => name.clone(),
        _ => String::new(),
    }
}

/// The loudness change against off: the size in dB and which way, or none when it is under a
/// twentieth of a dB.
fn loudness(db: f64) -> Option<(f64, &'static str)> {
    match db.abs() < 0.05 {
        true => None,
        false if db < 0.0 => Some((-db, "quieter")),
        false => Some((db, "louder")),
    }
}

/// The lines `show` prints for `view`.
fn lines(view: &SpeakersView) -> Vec<String> {
    let mut lines = vec![match &view.unavailable {
        Some(why) => format!("virtual speakers: {}: {why}", on_word(view)),
        None => format!("virtual speakers: {}", on_word(view)),
    }];
    lines.push(format!("set: {}", set_name(view)));
    if view.set.is_some() {
        let mut field = |name: &str, text: String| {
            if !text.is_empty() {
                lines.push(format!("{name}: {text}"));
            }
        };
        field("choice", choice(view));
        field("licence", view.licence.clone());
        field("credit", view.credit.clone().unwrap_or_default());
        field("source", view.source_url.clone());
        field("angles", view.angles.clone());
        if view.taps_ms > 0.0 {
            field(
                "length",
                match view.cut {
                    true => format!("cut to {:.1} s", view.taps_ms / 1000.0),
                    false => format!("{:.0} ms", view.taps_ms),
                },
            );
        }
        // Only a view the daemon described has the set's loudness: a saved id alone says none.
        if !view.label.is_empty() {
            field(
                "loudness",
                match loudness(view.loudness_db) {
                    Some((db, way)) => format!("{db:.1} dB {way} than with them off"),
                    None => "the same as with them off".to_string(),
                },
            );
        }
    }
    lines.extend(view.note.iter().map(|note| format!("note: {note}")));
    lines
}

/// The lines `list` prints: a table of the sets, then what the daemon says of the catalogue.
fn listed(found: &svolder_ipc::SpeakersCatalogue) -> Answer {
    let rows: Vec<[String; 5]> = found
        .sets
        .iter()
        .map(|set| {
            [
                set.id.clone(),
                room_text(set.room),
                set.label.clone(),
                set.licence.clone(),
                state_text(set),
            ]
        })
        .collect();
    let mut out = Vec::new();
    if rows.is_empty() {
        out.push("no sets".to_string());
    } else {
        let header = ["ID", "CHOICE", "LABEL", "LICENCE", "STATE"].map(str::to_string);
        let rows: Vec<&[String; 5]> = std::iter::once(&header).chain(&rows).collect();
        let widths: Vec<usize> = (0..4)
            .map(|at| {
                rows.iter()
                    .map(|row| row[at].chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for row in rows {
            let mut line = String::new();
            for (cell, width) in row.iter().zip(&widths) {
                line.push_str(&format!("{cell:<width$}  "));
            }
            line.push_str(&row[4]);
            out.push(line.trim_end().to_string());
        }
    }
    // The daemon's own note when it has one (it says why), else ours; fetching off always ends
    // with the step.
    const IMPORT_STEP: &str = "import your own file with `svolder host speakers import FILE`";
    let err = match (&found.note, found.fetching) {
        (Some(note), true) => vec![format!("note: {note}")],
        (Some(note), false) => vec![format!("note: {note}"), IMPORT_STEP.to_string()],
        (None, false) => vec![format!(
            "this daemon is set up to fetch nothing: {IMPORT_STEP}"
        )],
        (None, true) => Vec::new(),
    };
    Answer { out, err }
}

/// Where a set stands, for the list's last column.
fn state_text(set: &SpeakersSet) -> String {
    match (&set.damaged, set.fetched, set.own) {
        (Some(why), _, _) => format!("damaged: {why}"),
        (None, true, true) => "imported".to_string(),
        (None, true, false) => "fetched".to_string(),
        (None, false, _) if set.bytes > 0 => {
            format!("not fetched ({})", size_text(set.bytes))
        }
        (None, false, _) => "not fetched".to_string(),
    }
}

/// What a fetch or an import says after it kept a set: one line, then the sets that went to make
/// room; with `--json` the saved set as the daemon sends it.
fn kept(verb: &str, saved: &SpeakersSaved, json: bool) -> Answer {
    if json {
        return Answer {
            out: vec![serde_json::to_string(saved).unwrap_or_default()],
            err: Vec::new(),
        };
    }
    let set = &saved.set;
    let loudness = match loudness(set.loudness_db) {
        Some((db, way)) => format!("{db:.1} dB {way}"),
        None => "the same loudness as with them off".to_string(),
    };
    let mut err = vec![format!(
        "{verb} {} ({}): {}, {loudness}",
        set.label,
        set.id,
        room_text(set.room)
    )];
    if !saved.evicted.is_empty() {
        err.push(format!(
            "removed to make room: {}",
            saved.evicted.join(", ")
        ));
    }
    Answer {
        out: Vec::new(),
        err,
    }
}

#[cfg(test)]
mod tests;
