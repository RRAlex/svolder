// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The start of a TUI run: claim the terminal, play the intro beside the open, and hand the
//! device it landed on to the event loop.
//!
//! [`run_on`] is what [`run()`](super::run()) calls over a terminal claim and a loaded
//! configuration; [`watch`] is the one place a run turns the loop once it holds a device, shared
//! with the setup wizard's hand-over so that part is written once.

use std::process::ExitCode;
use std::time::{Instant, SystemTime};

use crossterm::event::{Event, EventStream};
use futures::stream::Stream;
use svolder_ipc::Config;

use super::decoration::title::{TerminalTitle, TitleMeter};
use super::decoration::{intro, intro_sound};
use super::run::{Adding, Look, Opener};
use super::terminal::{self, Claim, Guard, claimed, preflight};
use super::waiting::{INTERRUPTED, wait_for_device};
use super::{TuiOptions, app, only_socket, palette, run, theme};
use crate::backend::{Failure, Globals};
use crate::recovery::Kind;
use crate::wizard::live::LiveEffects;
use crate::wizard::{Given, Step, Wizard};

/// [`run`](fn@super::run) over a given terminal claim, loaded configuration and the path it was
/// loaded from.
pub(super) async fn run_on(
    claim: &impl Claim,
    config: Result<Config, svolder_ipc::ConfigError>,
    config_path: std::path::PathBuf,
    g: &Globals<'_>,
    mut opts: TuiOptions,
) -> ExitCode {
    let config = match preflight(claim, config) {
        Ok(config) => config,
        Err(failure) => return failure.report(),
    };
    // A daemon holding devices the file no longer lists reads it again before the first open,
    // so the open and the Devices screen see the daemon as the file describes it. The demo's
    // daemon and a joined one are not this configuration's.
    let reloaded = match opts.checked || opts.demo.is_some() || opts.join.is_some() {
        true => opts.reloaded.take(),
        false => crate::backend::reload_if_stale(g, &config, &config_path).await,
    };
    let opener = Opener::new(
        g.owned(),
        config.tui.clone(),
        config_path.clone(),
        run::Live {
            only: only_socket(&opts),
        },
    );
    // How every screen draws, and what the Settings box changes and saves from inside the
    // loop. The intro's own animation runs under the animation layer's conditions, so it runs
    // exactly when the layer does.
    let mut look = Look::new(
        config.tui.clone(),
        opts.no_color,
        palette::detect_depth(),
        config_path.clone(),
    );
    let animated = look.effects.enabled();
    if g.verbose {
        let clock = svolder_ipc::clock::local_hhmm(SystemTime::now());
        eprintln!(
            "svolder: tui starting at {clock} ({:?} colour)",
            look.theme.depth()
        );
    }
    let mut guard = match claimed(claim, config.tui.mouse) {
        Ok(guard) => guard,
        Err(failure) => return failure.report(),
    };
    let mut events = EventStream::new();
    let opening = opener.reopen(g.device, None);
    tokio::pin!(opening);
    // The intro draws on the terminal the open is reaching for a device behind, so it holds
    // the terminal only for as long as it runs: by the block's end it is dropped, and the
    // guard is free to be given back or drawn on.
    let mut sound = intro_sound::IntroSound::for_run(&config.tui);
    let mut plan = intro::IntroPlan {
        animate: animated,
        effect: config.tui.intro_effect,
        seed: intro::launch_seed(),
        pacer: intro::Pacer::for_session(),
        sync: terminal::frame_sync(),
    };
    let began = begin(
        &mut guard.terminal,
        &mut events,
        &look.theme,
        &mut plan,
        config.tui.intro,
        &mut opening,
        sound.as_mut(),
        |watching: &run::Watching| intro_sound::rate_of(&watching.app.readings.values),
    )
    .await;
    let Begun::Opened { opened, exit } = began else {
        return leave_unopened(guard, &mut std::io::stderr());
    };
    // The demo says once how a second front end joins its daemon, from the moment the intro is
    // over so the whole advice lifetime is there to read and copy a command, and keeps the
    // command on the Esc menu.
    // An open that found no device is no end of the run: the loop starts on the panel or the
    // Devices screen the failure earns, so the interface is always there to fix it from.
    let mut opened = landed(&opener, g.device, opened).await;
    if let Some(socket) = &opts.demo {
        advise_join(&mut opened.app, socket, Instant::now());
    }
    if let Some(note) = reloaded {
        opened.app.tick(Instant::now());
        opened.app.advise(note);
    }
    let adder = Live {
        g,
        path: config_path,
        refuse: if opts.demo.is_some() {
            Some("the demo has only its fake device; adding one is for a real run")
        } else if opts.join.is_some() {
            Some("a joined daemon is not scanned for from here; add the device where it runs")
        } else {
            None
        },
    };
    // The intro's exit plays over the loop's first frames, at the intro's pace, inside the
    // terminal's synchronized output.
    look.sync = plan.sync;
    if let Some(exit) = exit {
        look.effects.hand_over(exit);
    }
    watch(&opener, &adder, opened, guard, events, look).await
}

/// How a start ended before the loop took over.
enum Begun<T> {
    /// The open answered, with a device or with the failure a landing is made of, and the exit the
    /// intro hands the loop.
    Opened {
        opened: Result<T, Failure>,
        exit: Option<intro::Reform>,
    },
    /// The user gave up on a device that had not answered.
    GaveUp,
}

/// Start a run: the intro, when `show_intro`, beside the open, then the wait for the device if
/// the intro ended first or was switched off.
#[allow(clippy::too_many_arguments)]
async fn begin<B, S, T>(
    terminal: &mut ratatui::Terminal<B>,
    events: &mut S,
    theme: &theme::Theme,
    plan: &mut intro::IntroPlan,
    show_intro: bool,
    opening: &mut (impl Future<Output = Result<T, Failure>> + Unpin),
    sound: Option<&mut intro_sound::IntroSound>,
    rate_of: impl Fn(&T) -> Option<u32>,
) -> Begun<T>
where
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    let (opened, gave_up, exit) = if show_intro {
        intro_then_open(terminal, events, theme, plan, opening, sound, rate_of).await
    } else {
        (None, false, None)
    };
    // The intro ended first, or was switched off: the mark, or an empty screen, stays up while
    // the device is waited for, and the keys that end that wait are read throughout.
    let opened = match opened {
        Some(opened) => Some(opened),
        None if gave_up => None,
        None => wait_for_device(opening, events).await,
    };
    match opened {
        Some(opened) => Begun::Opened { opened, exit },
        None => Begun::GaveUp,
    }
}

/// Leave a run the user gave up on before a device answered: the terminal comes back first, so
/// the line lands on the user's own screen, and the exit is a success, as the loop's own `q` is.
/// The open is dropped on the way out of the caller, which lets go of whatever link it had
/// reached.
pub(super) fn leave_unopened<G>(guard: G, err: &mut impl std::io::Write) -> ExitCode {
    drop(guard);
    let _ = writeln!(err, "svolder: {INTERRUPTED}");
    ExitCode::SUCCESS
}

/// Show the intro while `opening` runs, and say how the two ended: what the open answered, if it
/// did so first or while the mark still had its run, whether the user gave up over the mark, and
/// the exit a full demo effect hands on.
///
/// The sound, when there is one, starts the moment the open answers with a device while the mark
/// is up, at the rate `rate_of` reads off it; it stops when a key ends the mark or gives up on
/// it. An intro that ends first, whatever ends it, plays nothing.
pub(super) async fn intro_then_open<B, S, T>(
    terminal: &mut ratatui::Terminal<B>,
    events: &mut S,
    theme: &theme::Theme,
    plan: &mut intro::IntroPlan,
    opening: &mut (impl Future<Output = Result<T, Failure>> + Unpin),
    mut sound: Option<&mut intro_sound::IntroSound>,
    rate_of: impl Fn(&T) -> Option<u32>,
) -> (Option<Result<T, Failure>>, bool, Option<intro::Reform>)
where
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    let intro = intro::play(terminal, events, theme, plan);
    tokio::pin!(intro);
    let mut opened = None;
    // Whether the user gave up over the mark itself: the intro reads Ctrl-C and `q` exactly as
    // the wait after it does, so one press is enough wherever it lands.
    let mut gave_up = false;
    let mut exit = None;
    // A device that answers before the mark's first frame is drawn, or in the same instant as a
    // key, still answered during the intro: the open is asked first.
    tokio::select! {
        biased;
        device = &mut *opening => opened = Some(device),
        ended = &mut intro => {
            gave_up = ended.end == intro::IntroEnd::GaveUp;
            exit = ended.exit;
        }
    }
    // An open that finished first, with a device or without, lets the mark have the rest of
    // its run: either way the loop starts on what the open earned. How that leftover run
    // ends says nothing about the device: the open is already over.
    if let Some(device) = &opened {
        if let (Ok(device), Some(sound)) = (device, sound.as_deref_mut()) {
            sound.start(rate_of(device));
        }
        let ended = (&mut intro).await;
        if ended.end != intro::IntroEnd::Finished
            && let Some(sound) = sound
        {
            sound.stop();
        }
        exit = ended.exit;
    }
    (opened, gave_up, exit)
}

/// What the loop starts on: the device `opened`, or, for an open that failed, where the failure
/// lands ([`run::land`]) when `device` was asked for.
///
/// The open and the check for a wait are two asks of the daemon, and the intro can run between
/// them. A device the daemon had not found at the open can be connected by the check, which
/// waits only for a device listed as not connected: that device is opened, once more, rather
/// than left on the Devices screen under the words of a failure that has passed.
pub(super) async fn landed<O: run::Open>(
    opener: &Opener<O>,
    device: Option<&str>,
    opened: Result<run::Watching, Failure>,
) -> run::Watching {
    let failure = match opened {
        Ok(watching) => return watching,
        Err(failure) => failure,
    };
    let mut landing = opener.landing_after(device, failure).await;
    if let run::Landing::Devices { why: Some(why) } = &landing
        && why.is_not_found_yet()
    {
        match opener.reopen(device, None).await {
            Ok(watching) => return watching,
            Err(again) => landing = opener.landing_after(device, again).await,
        }
    }
    run::land(opener, landing).await
}

/// Say how a second front end joins the demo daemon on `socket`: a toast whose lifetime starts at
/// `now`, not at the moment the app was built, and the command kept for the Esc menu.
fn advise_join(app: &mut app::App, socket: &std::path::Path, now: Instant) {
    let command = format!("svolder tui --join {}", socket.display());
    app.tick(now);
    app.advise(format!("{command} (a second front end)"));
    app.session.join_hint = Some(command);
}

/// How a running TUI walks someone through adding a device: against the machine it is running
/// on, from the scan on, with `-v` dropped for the reason every open the TUI makes drops it —
/// the alternate screen is up, and a diagnostic would land in the middle of a frame.
pub(super) struct Live<'a, 'b> {
    /// The command line the run was started with.
    pub(super) g: &'a Globals<'b>,
    /// The configuration file the TUI shows, which the walk reads and writes.
    pub(super) path: std::path::PathBuf,
    /// Why this run adds no device, for the demo and a joined daemon: the walk would scan the
    /// machine's own radio, which neither daemon is on.
    pub(super) refuse: Option<&'static str>,
}

impl Adding for Live<'_, '_> {
    type Effects = LiveEffects;

    async fn wizard(&self) -> Result<Wizard<LiveEffects>, Failure> {
        if let Some(why) = self.refuse {
            return Err(Failure::of(Kind::Usage, why));
        }
        let effects = LiveEffects::live(&crate::wizard::quietly(self.g), &self.path).await?;
        Ok(Wizard::starting_at(effects, Step::Scan, Given::default()))
    }
}

/// Run the event loop on what the start landed on, and give the terminal back.
///
/// Both ways into the device screen come through here (`svolder tui`, and the setup wizard
/// handing over the device it just added), so what a run does once it holds a device is written
/// once, not twice: the app takes the look this session resolved, the loop turns until it ends,
/// and the terminal is restored before anything is said or released, so a message lands on the
/// user's own screen and a slow disconnect never holds the alternate one up. An open that
/// failed has already become the screen the loop starts on, so nothing here reports one.
pub(super) async fn watch<O, A, S>(
    opener: &Opener<O>,
    adder: &A,
    mut watching: run::Watching,
    mut guard: Guard,
    events: S,
    look: Look,
) -> ExitCode
where
    O: run::Open,
    A: Adding,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    watching.app.set_look(&look.tui, &look.effects);
    watching.app.mascot.capable = look.mascot_capable();
    watching.reach_host();
    let mut title = TitleMeter::new(Box::new(TerminalTitle));
    let end = run::event_loop(run::LoopParts {
        opener,
        adder,
        watching,
        terminal: &mut guard.terminal,
        events,
        mouse: &mut guard.mouse,
        title: &mut title,
        look,
    })
    .await;
    // Nothing is being watched any more, so the title stops saying a level; the guard puts the
    // terminal's own title back as it gives the screen back.
    title.restore();
    drop(guard);
    if let Some(why) = end.why {
        eprintln!("svolder: {why}");
    }
    end.watching.release().await;
    ExitCode::from(end.code)
}

#[cfg(test)]
mod tests;
