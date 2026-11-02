// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `doctor`: what the host, the daemon and the configured devices look like, and the fixes this
//! verb can apply itself.
//!
//! Every question answerable without a radio is [`svolder_ipc::checks`]'s, taken from one
//! snapshot of the host so each line is about the same moment. The two that need one are this
//! binary's, since only a front end has a Bluetooth session to lend: whether each configured
//! device is on the air, and how well each adapter hears it. They are spliced into the
//! library's order so the reader sees the grades under the adapters they grade and the devices
//! under the config that names them.
//!
//! A fix that needs root is printed as the exact command and never run. The rest are collected
//! and offered once at the end, on a terminal, or applied without asking under `--yes`.

use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;

use svolder_ipc::checks::{self, DAEMON_INSTALL, Fix, Outcome, Probes, SETUP, State};
use svolder_ipc::config::{McpConfig, WebAuth, WebConfig};
use svolder_ipc::socket::daemon_version;
use svolder_ipc::{Config, McpStatus, ScanEntry, WebStatus, socket, token};

use crate::backend::launch;
use crate::backend::{Backend, Globals, OwnedGlobals};
use crate::commands::daemon::install::Installed;
use crate::recovery::{Failure, Kind, next_step};
use crate::scanning::{self, Grade, WINDOWS};

/// This svolder's own version, which a daemon's is compared with.
const OWN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How well each adapter hears the configured device, as the row is named.
const ADAPTER_GRADE: &str = "Adapter grade";

/// Whether each configured device is on the air, as the row is named.
const DEVICES: &str = "Devices";

/// Whether there is a configuration file at all, as the row is named.
const CONFIG_FILE: &str = "Config file";

/// Whether the MCP server can serve and does, as the row is named.
const MCP_SERVER: &str = "MCP server";

/// Whether the web UI can serve and does, as the row is named.
const WEB_UI: &str = "Web UI";

/// What `doctor` was asked for.
pub struct DoctorOpts {
    /// Print the outcomes as one JSON array instead of lines.
    pub json: bool,
    /// Apply the fixes this verb can apply itself without asking first.
    pub yes: bool,
    /// Also run the checks only someone driving the simulator cares about.
    pub dev: bool,
}

/// Run every check, print them, offer the fixes this verb can apply itself, and answer with
/// the exit code the outcomes ask for.
pub async fn run(g: &Globals<'_>, opts: DoctorOpts) -> ExitCode {
    let path = Config::path();
    let outcomes = outcomes(g, opts.dev).await;

    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&outcomes).expect("the outcomes serialise")
        );
    } else {
        for outcome in &outcomes {
            for line in lines(outcome) {
                println!("{line}");
            }
        }
    }

    let (terminal, guided) = asking(
        std::io::stdout().is_terminal(),
        opts.json,
        std::io::stdin().is_terminal(),
    );
    let accepted = offer(&fixes(&outcomes), terminal, opts.yes);
    // The guided first run is the one fix that is a conversation rather than a command, so it
    // is taken last, after the fixes that are commands have been applied and the rows they were
    // offered under have been printed.
    let wants_wizard = accepted.iter().any(|fix| fix.command == SETUP);
    for fix in accepted.iter().filter(|fix| fix.command != SETUP) {
        match apply(fix, &path).await {
            Ok(done) => eprintln!("svolder: {done}"),
            Err(why) => eprintln!("svolder: {why}"),
        }
    }
    if wants_wizard {
        match guided {
            // The wizard says what it did and what it could not; this verb leaves with the code
            // its own checks earned, which is what the rows above printed.
            true => {
                let _ = crate::wizard::adding(g, crate::wizard::Given::default()).await;
            }
            false => {
                eprintln!("svolder: run `svolder setup` on a terminal to add the first device")
            }
        }
    }
    ExitCode::from(checks::exit_code(&outcomes))
}

/// Every outcome `run` prints, the checks only run and nothing offered or applied.
///
/// Like `run`, it looks and changes nothing: no daemon is started and no fix is applied.
pub(crate) async fn outcomes(g: &Globals<'_>, dev: bool) -> Vec<Outcome> {
    outcomes_with(g, dev, true).await
}

/// [`outcomes`], connecting to the daemon socket only when `connect`.
///
/// A connect is how doctor finds a daemon answering, and where systemd's socket unit is
/// listening with its service idle, a connect is what starts the service. With `connect` off no
/// daemon is taken to answer: the daemon row comes from the unit facts alone and the devices are
/// looked for over the radio.
pub(crate) async fn outcomes_with(g: &Globals<'_>, dev: bool, connect: bool) -> Vec<Outcome> {
    outcomes_over(g, dev, connect, &Config::path()).await
}

/// [`outcomes_with`] over the config file at `path`.
async fn outcomes_over(g: &Globals<'_>, dev: bool, connect: bool, path: &Path) -> Vec<Outcome> {
    let path = path.to_path_buf();
    // A config that does not parse is the config check's to report, and every check that does
    // not read the file has a host to look at either way, so the rest of the run goes on with
    // the empty configuration a host with no file has.
    let loaded = Config::load_from(&path).ok();
    let config = loaded.clone().unwrap_or_default();
    let socket = socket::path::resolve(g.socket, config.socket.as_deref());
    let snapshot = checks::live::live(&socket, &path).await;
    let adapters: Vec<String> = snapshot
        .bluez()
        .adapters
        .into_iter()
        .map(|adapter| adapter.name)
        .collect();
    // Every row below this looks and changes nothing, the daemon included: a check that started
    // one would be reporting on its own work.
    let daemon = match connect {
        true => launch::probe(g, &config).await,
        false => None,
    };
    // `ble` and `fake` never ask a daemon, so one answering is nothing they used.
    if let Some(path) = daemon
        .as_ref()
        .filter(|_| crate::backend::asks_a_daemon(g.transport))
    {
        crate::backend::daemon_note(g, path);
    }
    let looking = looking_only(g, daemon.is_some());
    let looking = looking.borrow();
    let grade = grade_row_for(&looking, &config, &adapters).await;
    let devices = device_rows(&looking, loaded.as_ref()).await;
    let mut outcomes = rows(&snapshot, &socket, &path, dev, grade, devices);
    // Only a daemon the probe found answering is asked: a connect to a socket unit that is
    // listening would start the service, and a verb that bypasses the daemon has none to grade.
    let mut reported = None;
    let mut reported_web = None;
    if let Some(path) = version_asked_of(daemon.as_deref(), g.transport)
        && let Some((version, mcp, web)) = daemon_answer(path).await
    {
        grade_daemon_version(&mut outcomes, &version);
        reported = mcp;
        reported_web = web;
    }
    let env_token = std::env::var_os(token::Store::MCP.env_var).is_some_and(|v| !v.is_empty());
    outcomes.extend(mcp_row(
        &config.mcp,
        env_token,
        &token::generated_path(token::Store::MCP),
        reported.as_ref(),
    ));
    outcomes.extend(web_row(
        &config.web,
        std::env::var_os(token::Store::WEB.env_var).is_some_and(|v| !v.is_empty()),
        &token::generated_path(token::Store::WEB),
        reported_web.as_ref(),
    ));
    outcomes
}

/// The socket whose daemon doctor asks the version of: the one the probe found answering, and
/// only under a transport that uses a daemon.
fn version_asked_of<'a>(probed: Option<&'a Path>, transport: &str) -> Option<&'a Path> {
    probed.filter(|_| crate::backend::asks_a_daemon(transport))
}

/// How long the doctor waits for the daemon's version.
const DAEMON_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// The version of the daemon answering on `socket` and where its MCP server and its web UI
/// stand, or `None` when nothing answers within a few seconds or what answers is no version.
async fn daemon_answer(socket: &Path) -> Option<(String, Option<McpStatus>, Option<WebStatus>)> {
    let info = svolder_ipc::socket::daemon_version::version_within(socket, DAEMON_WAIT).await?;
    Some((info.version, info.mcp, info.web))
}

/// The row for the web UI, when `[web]` turns it on or the daemon reports it running.
///
/// It reads no token. It fails on a server the daemon says could not start, on a listen address
/// off loopback with no auth, and on a token file group or others can read (skipped when this
/// shell has the token in its environment); it warns on a section that is on while the daemon
/// answers that its web UI is off; it notes a listen address off loopback, which serves plain
/// HTTP on the network; and it passes with the status line otherwise.
fn web_row(
    web: &WebConfig,
    env_token: bool,
    generated: &Path,
    reported: Option<&WebStatus>,
) -> Option<Outcome> {
    use std::os::unix::fs::PermissionsExt;
    let running = matches!(
        reported,
        Some(WebStatus::Listening { .. } | WebStatus::Failed { .. })
    );
    if !web.enabled && !running {
        return None;
    }
    let row = |state, reason: String| {
        Some(Outcome {
            check: WEB_UI,
            state,
            reason,
            fix: None,
        })
    };
    if let Some(WebStatus::Failed { listen, error }) = reported {
        return row(State::Fail, format!("cannot listen on {listen}: {error}"));
    }
    if web.enabled && web.auth == WebAuth::None && !web.listen.ip().is_loopback() {
        return row(
            State::Fail,
            format!(
                "{} is not a loopback address and auth is \"none\"",
                web.listen
            ),
        );
    }
    if web.enabled && web.auth == WebAuth::Token && !env_token {
        let file = web
            .token_file
            .as_deref()
            .map(Path::new)
            .unwrap_or(generated);
        match std::fs::metadata(file) {
            Ok(meta) if meta.permissions().mode() & 0o077 != 0 => {
                return row(
                    State::Fail,
                    format!(
                        "the token file {} has mode {:04o}; chmod 600 it",
                        file.display(),
                        meta.permissions().mode() & 0o777
                    ),
                );
            }
            Err(e) if web.token_file.is_some() => {
                return row(
                    State::Fail,
                    format!("the token file {}: {e}", file.display()),
                );
            }
            _ => {}
        }
    }
    if web.enabled && matches!(reported, Some(WebStatus::Off)) {
        return row(
            State::Warn,
            "on in the config, but the daemon has not read it: svolder daemon restart (or SIGHUP)"
                .to_string(),
        );
    }
    if !web.listen.ip().is_loopback() {
        return row(
            State::Note,
            "served over plain HTTP on your network: anyone who can watch it can copy a sign-in; \
             a TLS proxy in front closes that"
                .to_string(),
        );
    }
    match reported {
        Some(status @ WebStatus::Listening { .. }) => row(
            State::Pass,
            status.line().trim_start_matches("web: ").to_string(),
        ),
        _ => row(
            State::Note,
            format!(
                "on in the config, on {}; the daemon is not asked",
                web.listen
            ),
        ),
    }
}

/// The row for the MCP server, when `[mcp]` turns it on or the daemon reports one running.
///
/// It looks and changes nothing: the token is not read, so a generated one is not created
/// here. It fails on a server the daemon says could not start, on a listen address off loopback
/// with no auth (the config refuses that, so this is a file edited under it), and on a token
/// file group or others can read; it skips the file check when this shell has a token in its
/// environment, since the daemon's may differ. It warns on writes with no `max_volume_db`, and
/// on a section that is on while the daemon answers that its server is off.
fn mcp_row(
    mcp: &McpConfig,
    env_token: bool,
    generated: &Path,
    reported: Option<&McpStatus>,
) -> Option<Outcome> {
    use std::os::unix::fs::PermissionsExt;

    let running = matches!(
        reported,
        Some(McpStatus::Listening { .. } | McpStatus::Failed { .. })
    );
    if !mcp.enabled && !running {
        return None;
    }
    let row = |state, reason: String| {
        Some(Outcome {
            check: MCP_SERVER,
            state,
            reason,
            fix: None,
        })
    };
    if let Some(McpStatus::Failed { listen, error }) = reported {
        return row(State::Fail, format!("cannot listen on {listen}: {error}"));
    }
    if mcp.enabled
        && mcp.auth == svolder_ipc::config::McpAuth::None
        && !mcp.listen.ip().is_loopback()
    {
        return row(
            State::Fail,
            format!(
                "{} is not a loopback address and auth is \"none\"",
                mcp.listen
            ),
        );
    }
    if mcp.enabled && mcp.auth == svolder_ipc::config::McpAuth::Token && !env_token {
        let file = mcp
            .token_file
            .as_deref()
            .map(Path::new)
            .unwrap_or(generated);
        match std::fs::metadata(file) {
            Ok(meta) if meta.permissions().mode() & 0o077 != 0 => {
                return row(
                    State::Fail,
                    format!(
                        "the token file {} has mode {:04o}; chmod 600 it",
                        file.display(),
                        meta.permissions().mode() & 0o777
                    ),
                );
            }
            Err(e) if mcp.token_file.is_some() => {
                return row(
                    State::Fail,
                    format!("the token file {}: {e}", file.display()),
                );
            }
            // A generated token not made yet is made by the daemon's first start.
            _ => {}
        }
    }
    // A model that can write should have a ceiling on the volume it can set.
    if mcp.enabled && mcp.writes && mcp.max_volume_db.is_none() {
        return row(
            State::Warn,
            "writes are on and max_volume_db is not set, so a model may set any volume once the user agrees; set [mcp] max_volume_db to a ceiling".to_string(),
        );
    }
    match reported {
        Some(status @ McpStatus::Listening { .. }) => row(
            State::Pass,
            status.line().trim_start_matches("mcp: ").to_string(),
        ),
        Some(McpStatus::Off) if mcp.enabled => row(
            State::Warn,
            "on in the config, but the daemon has not read it: svolder daemon restart (or SIGHUP)"
                .to_string(),
        ),
        _ => row(
            State::Note,
            format!(
                "on in the config, on {}; the daemon is not asked",
                mcp.listen
            ),
        ),
    }
}

/// Grade the daemon check for the version of the daemon that answers: a version other than this
/// svolder's is a `!` naming both and which is older; an older daemon gets the verb that
/// restarts it, and the check keeps the worse grade and the fix it already had when it earned
/// one.
fn grade_daemon_version(outcomes: &mut [Outcome], daemon: &str) {
    let Some(skew) = daemon_version::skew(daemon, OWN_VERSION) else {
        return;
    };
    let Some(row) = outcomes.iter_mut().find(|o| o.check == checks::DAEMON) else {
        return;
    };
    let words = skew.words(daemon, OWN_VERSION);
    row.reason = match row.state {
        State::Pass | State::Note => words,
        State::Warn | State::Fail => format!("{}; {words}", row.reason),
    };
    if matches!(row.state, State::Pass | State::Note) {
        row.state = State::Warn;
    }
    if row.fix.is_none() && skew == daemon_version::Skew::Older {
        row.fix = Some(Fix {
            command: daemon_version::RESTART_COMMAND.to_string(),
            applies_itself: false,
        });
    }
}

/// Whether the fixes can be offered at all, and whether the guided first run among them can be
/// walked.
///
/// A run whose output is being read has no one to ask, and a JSON array is no place for a
/// question: both leave the offer to a later run on a terminal. The wizard asks several
/// questions rather than one, so it needs the other end of the conversation as well — a run
/// whose answers would come from a file is one that would answer every question with the end of
/// that file.
fn asking(stdout: bool, json: bool, stdin: bool) -> (bool, bool) {
    let terminal = stdout && !json;
    (terminal, terminal && stdin)
}

/// Every row one snapshot of the host answers, with this front end's own spliced in: the grades
/// under the adapters they grade, and the file to create and the devices under the config that
/// names them.
///
/// The two rows that need a radio are passed in, since only a caller with a Bluetooth session to
/// lend has them: `doctor` grades the adapters and looks for each configured device; the setup
/// wizard does neither (the adapter grade is a step of its own there, and the devices are what it
/// is about to add the first of).
pub(crate) fn rows(
    probes: &dyn Probes,
    socket: &Path,
    config: &Path,
    dev: bool,
    grade: Option<Outcome>,
    devices: Vec<Outcome>,
) -> Vec<Outcome> {
    let host = checks::run(probes, socket, dev);
    let under_config: Vec<Outcome> = config_file_row(config, probes.config_text().is_none())
        .into_iter()
        .chain(devices)
        .collect();
    spliced(host, grade, under_config)
}

/// The arguments the rows that look at devices run under.
///
/// They are the ones this verb was given, except for where a device verb would go when no
/// daemon answers: `auto` and `daemon` would walk the launcher chain, which starts a daemon,
/// so with none answering the rows read over a session of this process's own instead. Nothing
/// is forced when a daemon is already there — the rows are then about the machine as it is —
/// and `ble` and `fake` already name a session of this process's own.
fn looking_only(g: &Globals<'_>, daemon_answers: bool) -> OwnedGlobals {
    let mut looking = g.owned();
    if !daemon_answers && !matches!(g.transport, "ble" | "fake") {
        looking.transport = "ble".to_string();
    }
    looking
}

/// The row that offers a host with no configuration file the file itself.
///
/// The config check knows there is no file; where this front end would put one is the front
/// end's own, so the row that says what to create, and the fix that creates it, are here.
fn config_file_row(config: &Path, missing: bool) -> Option<Outcome> {
    missing.then(|| Outcome {
        check: CONFIG_FILE,
        state: State::Warn,
        reason: "there is no config file yet".into(),
        fix: Some(write_template(config)),
    })
}

/// One row per configured device, saying whether it is on the air and what is holding it.
///
/// Each device is searched for in turn, over whichever backend the device verbs would run on.
/// A configuration with no device in it, and one that could not be read at all, both name no
/// device to look for and get no row: the config check above has already said which of the two
/// it was, and a line about the devices there are none of would only repeat it.
async fn device_rows(g: &Globals<'_>, config: Option<&Config>) -> Vec<Outcome> {
    let Some(config) = config.filter(|config| !config.devices.is_empty()) else {
        return Vec::new();
    };
    // Which devices a daemon holds is worth a word on each line and nothing on its own: a
    // daemon that is not answering leaves every device unheld, which is what the rows say, and
    // is not one to start for the sake of the word.
    let held = Backend::devices_if_running(g).await.unwrap_or_default();
    let mut rows = Vec::with_capacity(config.devices.len());
    for device in &config.devices {
        let connected = held
            .iter()
            .any(|row| row.name == device.name && row.connected == Some(true));
        let found = match connected {
            // A device a central holds stops advertising, so the windows would spend their
            // fifteen seconds hearing nothing and report a device that is working as missing.
            // What the daemon says about a device it is holding is the whole of the answer.
            true => Ok(Vec::new()),
            false => scanning::find(g, Some(&device.udid)).await,
        };
        rows.push(device_row(&device.name, &device.udid, found, connected));
    }
    rows
}

/// The row grading each adapter against the first configured device, where there is a device
/// to grade against and more than one adapter to choose between.
///
/// One adapter is no choice, and no device is nothing to grade against, so neither host is
/// made to wait through a grading run whose answer it could not use.
async fn grade_row_for(g: &Globals<'_>, config: &Config, adapters: &[String]) -> Option<Outcome> {
    let device = config.devices.first()?;
    if adapters.len() < 2 {
        return None;
    }
    let mut grades = scanning::grade_adapters(g, &device.udid, adapters).await;
    // Best first, as the wizard's own adapter step offers them, so the two never read the same
    // grades in two orders. The sort is stable, so adapters that caught the device equally
    // often keep the order the host reports them in — which is the tie the row's fix breaks to.
    grades.sort_by(|a, b| b.seen.cmp(&a.seen));
    let configured = config
        .adapter
        .as_deref()
        .or_else(|| adapters.first().map(String::as_str));
    Some(grade_row(&grades, configured, &device.name))
}

/// One device's row: whether a daemon holds it, and failing that whether the windows heard it
/// and how loudly.
///
/// A device the daemon has is connected whatever a scan heard, and nothing else about it is
/// worth a line: a device a central holds stops advertising, so on a healthy host with the
/// daemon up "not seen" is the ordinary outcome and says nothing true about the device.
///
/// A search that ran and heard nothing is the device's own trouble and carries the step that
/// follows it. A search that could not run says what stopped it and carries no step: the rows
/// above this one are about the adapter and the daemon, and they carry the fixes for those.
fn device_row(
    name: &str,
    udid: &str,
    found: Result<Vec<ScanEntry>, Failure>,
    connected: bool,
) -> Outcome {
    if connected {
        return Outcome {
            check: DEVICES,
            state: State::Pass,
            reason: format!("{name}: connected (held by the daemon; not advertising)"),
            fix: None,
        };
    }
    let failure = match found {
        Ok(heard) => {
            let signal = heard
                .iter()
                .find(|entry| scanning::names(entry, udid))
                .and_then(|entry| entry.rssi)
                .map(|rssi| format!(", {rssi} dBm"))
                .unwrap_or_default();
            return Outcome {
                check: DEVICES,
                state: State::Pass,
                reason: format!("{name}: seen{signal}"),
                fix: None,
            };
        }
        Err(failure) => failure,
    };
    let quiet = failure.next == next_step(Kind::NothingInRange);
    let reason = match quiet {
        true => format!("{name}: not seen in {WINDOWS} windows"),
        false => format!("{name}: {}", failure.what),
    };
    Outcome {
        check: DEVICES,
        state: State::Warn,
        // A step that says to run this verb is no step under a line this verb just printed.
        fix: (!failure.next.contains("svolder doctor")).then_some(Fix {
            command: failure.next,
            applies_itself: false,
        }),
        reason,
    }
}

/// The grades as one row: every adapter's catch rate, and the best one to write into the
/// config when the configured adapter is not it.
///
/// An adapter the config names that is not among the grades heard nothing, because it is not
/// there to hear anything, so it is below whatever is. Adapters that heard the device equally
/// often are a tie the first of them wins — the same adapter the checks' own adapter fix
/// names — so the two rows of one screen never send the reader to different adapters, and a
/// device no adapter heard at all is the tie where every one of them caught nothing.
fn grade_row(grades: &[Grade], configured: Option<&str>, device: &str) -> Outcome {
    let listed = grades
        .iter()
        .map(|grade| match &grade.note {
            Some(note) => format!("{} {}/{} ({note})", grade.adapter, grade.seen, grade.of),
            None => format!("{} {}/{}", grade.adapter, grade.seen, grade.of),
        })
        .collect::<Vec<_>>()
        .join(", ");
    // The first of equals, so a tie goes to the adapter the list starts with rather than the
    // one it ends with.
    let best = grades
        .iter()
        .fold(None::<&Grade>, |best, grade| match best {
            Some(best) if best.seen >= grade.seen => Some(best),
            _ => Some(grade),
        });
    let Some(best) = best else {
        return Outcome {
            check: ADAPTER_GRADE,
            state: State::Note,
            reason: format!("no adapter was graded against {device}"),
            fix: None,
        };
    };
    let mine = configured
        .and_then(|name| grades.iter().find(|grade| grade.adapter == name))
        .map_or(0, |grade| grade.seen);
    if mine >= best.seen {
        return Outcome {
            check: ADAPTER_GRADE,
            state: State::Pass,
            reason: listed,
            fix: None,
        };
    }
    Outcome {
        check: ADAPTER_GRADE,
        state: State::Warn,
        reason: format!(
            "{listed}; the config uses {}",
            configured.unwrap_or("no adapter")
        ),
        fix: Some(Fix {
            command: format!("set adapter = {:?} in config.toml", best.adapter),
            applies_itself: true,
        }),
    }
}

/// This verb's own rows, in their places: the grades under the adapters they grade, and the
/// file to create and the devices under the config that names them.
fn spliced(
    host: Vec<Outcome>,
    mut grade: Option<Outcome>,
    mut under_config: Vec<Outcome>,
) -> Vec<Outcome> {
    let mut out = Vec::with_capacity(host.len() + 1 + under_config.len());
    for outcome in host {
        let check = outcome.check;
        out.push(outcome);
        if check == checks::ADAPTERS {
            out.extend(grade.take());
        } else if check == checks::CONFIG {
            out.append(&mut under_config);
        }
    }
    // Every set of checks names both neighbours; a set that named neither still shows what this
    // verb found rather than dropping it.
    out.extend(grade);
    out.append(&mut under_config);
    out
}

/// One outcome as the lines it prints: the mark its state earns, the check and the reason,
/// and under a line that did not pass, the fix.
///
/// The setup wizard shows its health step through these too, so a row reads the same whichever
/// of the two printed it.
pub(crate) fn lines(outcome: &Outcome) -> Vec<String> {
    let mut lines = vec![line(outcome)];
    lines.extend(fix_line(outcome));
    lines
}

/// One outcome's own line: the mark its state earns, the check and the reason.
pub(crate) fn line(outcome: &Outcome) -> String {
    format!(
        "{} {} — {}",
        mark(outcome.state),
        outcome.check,
        outcome.reason
    )
}

/// The line under an outcome that did not pass, saying what to do about it.
pub(crate) fn fix_line(outcome: &Outcome) -> Option<String> {
    let fix = outcome.fix.as_ref()?;
    matches!(outcome.state, State::Warn | State::Fail).then(|| format!("    fix: {}", fix.command))
}

/// The mark a state is read by at the start of its line.
fn mark(state: State) -> char {
    match state {
        State::Pass => '✓',
        State::Warn => '!',
        State::Fail => '✗',
        State::Note => '·',
    }
}

/// Every fix this verb could apply itself, in the order the checks that asked for them read.
///
/// Only the rows that printed their fix offer it: [`lines`] shows a fix under a line that
/// warned or failed, and nothing is applied that the reader was not shown.
pub(crate) fn fixes(outcomes: &[Outcome]) -> Vec<Fix> {
    outcomes
        .iter()
        .filter(|outcome| matches!(outcome.state, State::Warn | State::Fail))
        .filter_map(|outcome| outcome.fix.as_ref().filter(|fix| fix.applies_itself))
        .cloned()
        .collect()
}

/// The fix that gives a host with no configuration file one to keep its devices in.
fn write_template(config: &Path) -> Fix {
    Fix {
        command: format!("create {} from the template", config.display()),
        applies_itself: true,
    }
}

/// Which of the collected fixes to apply.
pub fn offer(fixes: &[Fix], terminal: bool, yes: bool) -> Vec<Fix> {
    offer_with(&Stdin, fixes, terminal, yes)
}

/// [`offer`] over any way of asking, so what the question is and what an answer does can be
/// tested without one.
///
/// A fix that needs root is never one of these, whatever was answered: the checks mark what a
/// front end may run, and this only ever offers what they marked.
fn offer_with(ask: &dyn Ask, fixes: &[Fix], terminal: bool, yes: bool) -> Vec<Fix> {
    let mine: Vec<Fix> = fixes
        .iter()
        .filter(|fix| fix.applies_itself)
        .cloned()
        .collect();
    if mine.is_empty() {
        return Vec::new();
    }
    if yes {
        return mine;
    }
    if !terminal {
        return Vec::new();
    }
    match ask.yes_no(&question(&mine)) {
        Ok(true) => mine,
        // A question that could not be asked, or could not be read, is a no: nothing is
        // changed on an answer nobody gave.
        _ => Vec::new(),
    }
}

/// The question the offer asks, with the fixes it is about listed under it.
fn question(fixes: &[Fix]) -> String {
    let mut out = format!(
        "Apply the {} {} doctor can do itself?\n",
        fixes.len(),
        if fixes.len() == 1 { "fix" } else { "fixes" }
    );
    for fix in fixes {
        out.push_str(&format!("  {}\n", fix.command));
    }
    out.push_str("[y/N]");
    out
}

/// Asking the user a question that takes a yes or a no.
trait Ask {
    /// Put `question` where the user will see it and answer with whether they said yes.
    fn yes_no(&self, question: &str) -> std::io::Result<bool>;
}

/// The question on standard error and the answer from standard input, which is where every
/// other prompt in this binary puts them.
struct Stdin;

impl Ask for Stdin {
    fn yes_no(&self, question: &str) -> std::io::Result<bool> {
        crate::commands::device::ask(question)
    }
}

/// Apply one offered fix, answering with what to say about what happened.
///
/// A fix is recognised by the command it carries, the whole of what an offered fix says: the
/// units the daemon check asks for, and the config file and adapter key this verb's own rows ask
/// for. The guided first run is the one offered fix not applied here, since it is a conversation
/// rather than a command and whoever offered it walks it themselves. Anything else says it was
/// not applied rather than reporting a fix that never ran.
pub(crate) async fn apply(fix: &Fix, config: &Path) -> Result<String, String> {
    apply_with(&Verb, fix, config).await
}

/// A fix that is a verb of its own, run.
///
/// Only the verbs are behind this: a fix that writes the config file writes it wherever it is
/// pointed, and a test points it at a file of its own. A verb changes the machine the test is
/// running on, so what a test wants of one is that it was reached for, not that it ran.
#[async_trait::async_trait(?Send)]
trait Verbs {
    /// `svolder daemon install`, which writes the units and enables them.
    async fn install_daemon(&self) -> Result<String, String>;
}

/// The verbs themselves.
struct Verb;

#[async_trait::async_trait(?Send)]
impl Verbs for Verb {
    async fn install_daemon(&self) -> Result<String, String> {
        // The verb says what it wrote and what it enabled as it goes, so what comes back here
        // is only the line that ends the list of applied fixes — and a machine where it wrote
        // nothing is not a machine this fix was applied to.
        match crate::commands::daemon::install::install(false).await {
            Ok(Installed::Units) => Ok("installed the systemd user units".to_string()),
            Ok(Installed::Written) => {
                Ok("wrote the systemd user units; each user enables them".to_string())
            }
            Ok(Installed::Skipped(why)) => Err(format!("the units were not installed: {why}")),
            Err(failure) => Err(failure.to_string()),
        }
    }
}

/// [`apply`] over any way of running the verbs a fix can be.
async fn apply_with(verbs: &dyn Verbs, fix: &Fix, config: &Path) -> Result<String, String> {
    if fix.command == DAEMON_INSTALL {
        return verbs.install_daemon().await;
    }
    if fix.command == write_template(config).command {
        return Config::create_from_template(config)
            .map(|()| format!("wrote {} from the template", config.display()))
            .map_err(|e| e.to_string());
    }
    if let Some(adapter) = adapter_in(&fix.command) {
        return Config::set_adapter(config, Some(adapter))
            .map(|()| format!("set adapter = {adapter:?} in {}", config.display()))
            .map_err(|e| e.to_string());
    }
    Err(format!("{}: doctor cannot apply this itself", fix.command))
}

/// The adapter name in an adapter fix's command, which is the one [`grade_row`] wrote there.
fn adapter_in(command: &str) -> Option<&str> {
    let quoted = command.strip_prefix("set adapter = ")?.strip_prefix('"')?;
    let (adapter, _) = quoted.split_once('"')?;
    Some(adapter)
}

#[cfg(test)]
mod tests;
