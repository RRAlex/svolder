// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device list's own flows: walking someone through adding a device, editing, renaming and
//! removing one, switching to another, keeping the app's copy of the configuration file in
//! step with what is on disk — the preset list's writes go the same way — and keeping what a
//! daemon says about each device current while the list shows.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::event::Event;
use futures::Stream;
use ratatui::Terminal;

use svolder_ipc::{Config, DeviceConfig};

use crate::backend::{Backend, Failure};
use crate::recovery::Kind;

use crate::tui::app::{App, Holding, Screen};
use crate::tui::mouse::HitMap;
use crate::tui::widgets::spinner;
use crate::tui::wizard::{self, WizardEnd};

use super::{Adding, Kept, Landing, Look, OnScreen, Open, Opener, Shown, Watching, refresh_note};
use crate::tui::waiting::{Interrupted, STOP_AFTER_ERRORS, gives_up};

/// How often the device list asks a daemon again how each device is doing while it shows: a
/// wait before the next connect attempt is a number of seconds, and a number on screen must not
/// fall behind by more than this.
pub(super) const RELIST_PERIOD: Duration = Duration::from_secs(2);

/// How long one of those asks may take before the list keeps what it has: the daemon answers a
/// listing from what it holds, so an answer this late is one the loop should not wait on.
pub(super) const RELIST_BOUND: Duration = Duration::from_secs(1);

/// What switching to a name the device list does not carry is answered with.
fn no_such_device(name: &str) -> String {
    format!("no device named {name}")
}

/// What walking someone through adding a device left the loop with.
pub(super) enum Added {
    /// The loop goes on, on whatever device it is watching now.
    Carried,
    /// The loop is over: the code to leave with, and what to say once the terminal is back.
    Ends(u8, Option<String>),
}

/// What the frame that stands while the host is being looked over is titled.
const ADDING: &str = "Add a device";

/// What that frame says is being waited for.
const LOOKING_AT_THE_HOST: &str = "looking at the host…";

/// Walk someone through adding a device, over the session that is running.
///
/// The walk is the one the setup wizard's own front end runs, drawn on this terminal and read
/// from this input, so `n` on the device list and `svolder setup` walk the very same steps. It
/// starts at the scan: the host is already running a TUI, so there is nothing to check and
/// nothing to fix. A walk that added a device switches to it; one that was left, by `q` or by
/// `Esc` on the scan, switches to nothing and comes back to the screen it was opened from, an empty
/// Devices screen included. Only an effect the machine refused ends the run.
pub(super) async fn add_device<O, A, B, S>(
    adder: &A,
    opener: &Opener<O>,
    watching: &mut Watching,
    terminal: &mut Terminal<B>,
    events: &mut S,
    look: &mut Look,
) -> Added
where
    O: Open,
    A: Adding,
    B: ratatui::backend::Backend,
    S: Stream<Item = std::io::Result<Event>> + Unpin,
{
    // The walk draws its own frames and reads the keys itself, so nothing takes the capture's
    // blocks while it runs: the capture is stopped, and the frame after the walk starts it
    // again where it is still wanted.
    watching.host.stop_capture(&mut watching.app);
    // Building the walk looks the host over — its adapters, what it makes of a background
    // service — which is a second or so of D-Bus and subprocesses with nothing on the screen.
    // The frame that says so is up while it is spent, and the keys that give up are read
    // throughout, as they are through every other wait the wizard makes.
    let mut hits = HitMap::default();
    let mut view = wizard::ShownStep::waiting_on(ADDING, LOOKING_AT_THE_HOST);
    let mut reading = true;
    let looked = {
        let mut canvas = wizard::Canvas::new(terminal, &mut hits, &look.theme);
        wizard::wait_on(
            &mut canvas,
            events,
            &mut reading,
            &mut view,
            true,
            adder.wizard(),
        )
        .await
    };
    watching.app.frame_flags.screen_changed = true;
    let mut wizard = match looked {
        wizard::Waited::Done(Ok(wizard)) | wizard::Waited::Left(Ok(wizard)) => wizard,
        wizard::Waited::Done(Err(failure)) | wizard::Waited::Left(Err(failure)) => {
            watching.app.notify_failure(&failure);
            return Added::Carried;
        }
        wizard::Waited::GaveUp => return Added::Carried,
    };
    let end = wizard::event_loop(
        terminal,
        events,
        &mut wizard,
        &look.theme,
        wizard::Leaving::ToTui,
    )
    .await;
    // The screen the wizard drew over is about to be drawn again, which is a screen change as
    // far as the animation layer is concerned.
    watching.app.frame_flags.screen_changed = true;
    let added = match end {
        WizardEnd::Quit | WizardEnd::Later => None,
        WizardEnd::Added(device) => Some(device.name),
        WizardEnd::Stopped { code, why } => return Added::Ends(code, Some(why)),
    };
    if let Err(e) = relisted(&opener.path, &mut watching.app) {
        watching
            .app
            .notify_failure(&Failure::of(Kind::ConfigUnreadable, e.to_string()));
    }
    // A walk that ended with nothing added stays on the screen it came from, an empty Devices
    // screen included.
    if let Some(name) = added {
        let mut screen = OnScreen {
            terminal,
            hits: &mut hits,
            events,
        };
        if switch_device(opener, watching, &name, look, &mut screen).await == Switched::GaveUp {
            return Added::Ends(0, None);
        }
    }
    Added::Carried
}

/// Write `device` over the block named `name`, and watch it under whichever name it now has.
///
/// A rename of the device being watched is a rename of what a daemon holds it under and what
/// every request names it by, so the session is opened again on the new name and the header
/// follows it; every other device is a row in the list, and the list is what refreshes. The
/// writer carries a `default_device` naming this device along with the rename, so nothing else
/// has to be written for it.
pub(super) async fn update_device<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    name: &str,
    device: &DeviceConfig,
    look: &Look,
    screen: &mut dyn Shown,
) -> Switched {
    let renamed = watches(watching, name) && !device.name.eq_ignore_ascii_case(name);
    let written = written(
        opener,
        watching,
        Config::update_device(&opener.path, name, device),
    )
    .await;
    let mut switched = Switched::Stayed;
    if written && renamed {
        switched = switch_device(opener, watching, &device.name, look, screen).await;
    }
    if written && switched != Switched::GaveUp {
        say_passphrase_note(watching, device).await;
    }
    switched
}

/// Say, right after the editor saved `device`, that its passphrase does not match the code the
/// device advertises, when it does not: the note the Status pane would carry, said where the
/// editor was, since the screen the editor is on has no Status pane.
///
/// The editor has no advertisement to judge a code by as it is typed, and a verdict on it would
/// take a request to the daemon for each digit, so the entry is judged once it is saved (a direct
/// session takes it in then too). Best effort, as the note always is: the save has happened
/// either way. The Status pane's copy of the note is asked for again afterwards, since a direct
/// session has only just taken the entry in.
async fn say_passphrase_note(watching: &mut Watching, device: &DeviceConfig) {
    let Some(backend) = watching.backend.as_mut() else {
        return;
    };
    if let Some(note) = backend.passphrase_note_for(device).await {
        watching
            .app
            .warn(format!("{}: {}", device.name, note.words()));
    }
    refresh_note(backend, &mut watching.app).await;
}

/// Take the device named `name` out of the configuration file, and say whether the loop is left
/// with no device at all — which is what sends it to the wizard — or was told to give up while
/// the next device opened.
///
/// Removing the device being watched leaves the session on a device the file no longer names, so
/// the first one still configured is opened instead. A `default_device` naming this device goes
/// in the writer's own edit, so one write and one reload take the whole removal.
pub(super) async fn remove_device<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    name: &str,
    look: &Look,
    screen: &mut dyn Shown,
) -> Removed {
    let watched = watches(watching, name);
    if !written(opener, watching, Config::remove_device(&opener.path, name)).await {
        return Removed::Kept;
    }
    if !watched {
        return Removed::Kept;
    }
    match watching
        .app
        .devices
        .rows
        .first()
        .map(|row| row.name.clone())
    {
        Some(first) => match switch_device(opener, watching, &first, look, screen).await {
            Switched::GaveUp => Removed::GaveUp,
            Switched::Opened | Switched::Stayed => Removed::Kept,
        },
        // Nothing left to watch: the walk that adds one is what the loop does next.
        None => Removed::Empty,
    }
}

/// What taking a device out of the file left the loop to do.
pub(super) enum Removed {
    /// Carry on: the device is gone, or was not the one being watched.
    Kept,
    /// The last device is gone: the walk that adds one comes next.
    Empty,
    /// Ctrl-C or `q` was pressed while the device that replaced it opened.
    GaveUp,
}

/// How a device switch came out.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Switched {
    /// The device asked for is the one being watched now.
    Opened,
    /// No other device is being watched: the switch was refused, or an open failed and the
    /// device before it was opened again, or the loop is left with nothing to read.
    Stayed,
    /// Ctrl-C or `q` was pressed. A press during the close let the close finish and skipped the
    /// open; one during an open dropped it. Either way nothing is held.
    GaveUp,
}

/// Whether the device the loop is watching is the one called `name`.
///
/// The backend says which configured device it is on — the name a daemon lists it under, or the
/// `[[device]]` block a direct session was opened against — which is the name the file spells
/// and a writer edits. A backend that was not picked out by the file at all leaves the selector
/// the loop opened with to answer, and with neither there is no configured device to be
/// watching.
fn watches(watching: &Watching, name: &str) -> bool {
    watching
        .backend
        .as_ref()
        .and_then(Backend::configured_name)
        .or_else(|| watching.device.clone())
        .is_some_and(|watched| watched.eq_ignore_ascii_case(name))
}

/// Report what a write to the configuration file came to, tell whatever holds the devices to
/// read the file again, and take what it now says into the app.
///
/// Every write the device list makes goes through here (the preset list's through
/// [`written_preset`]), so a refusal is a
/// notice rather than a crash or a silent no-op, and the screen always shows what is on disk
/// rather than what was asked for.
pub(super) async fn written<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    wrote: Result<(), svolder_ipc::ConfigError>,
) -> bool {
    if let Err(e) = wrote {
        watching
            .app
            .notify_failure(&Failure::of(Kind::ConfigUnreadable, e.to_string()));
        return false;
    }
    if let Some(backend) = watching.backend.as_mut() {
        if let Err(failure) = backend.reload().await {
            watching.app.notify_failure(&failure);
        }
        // The daemon judged the passphrase again as it took the entry in.
        refresh_note(backend, &mut watching.app).await;
    }
    relist(opener, watching);
    true
}

/// [`written`] for the preset list: the file is the record, and a daemon is told to read it
/// again only when its `version` reply names this very file, so its MQTT bridge offers the new
/// list. A daemon that reads another file, a failed reload and a direct session all pass
/// without a word.
pub(super) async fn written_preset<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    wrote: Result<(), svolder_ipc::ConfigError>,
) -> bool {
    if let Err(e) = wrote {
        watching
            .app
            .notify_failure(&Failure::of(Kind::ConfigUnreadable, e.to_string()));
        return false;
    }
    if let Some(socket) = watching.backend.as_ref().and_then(Backend::daemon_socket) {
        crate::backend::reload_if_reads(socket, &opener.path).await;
    }
    relist(opener, watching);
    true
}

/// Show what is on disk again, noticing a file that cannot be read.
fn relist<O: Open>(opener: &Opener<O>, watching: &mut Watching) {
    if let Err(e) = relisted(&opener.path, &mut watching.app) {
        watching
            .app
            .notify_failure(&Failure::of(Kind::ConfigUnreadable, e.to_string()));
    }
}

/// Take what the configuration file at `path` says about the devices and the presets into
/// `app`: the records the editor edits, the device a command acts on when it names none, the
/// list itself, and the presets the preset list offers after the built-ins.
///
/// What the backend said about each device it still names is kept (its product, and whether the
/// link is up), so editing one device does not blank the column beside every other one. The
/// device list's cursor is kept on a row it still has; on any other screen the focused row is
/// that screen's own, which a device list of any length says nothing about.
pub(super) fn relisted(path: &Path, app: &mut App) -> Result<(), svolder_ipc::ConfigError> {
    let config = Config::load_from(path)?;
    app.devices.rows = crate::backend::rows_from_config(&config)
        .into_iter()
        .map(
            |row| match app.devices.rows.iter().find(|had| had.name == row.name) {
                Some(had) => had.clone(),
                None => row,
            },
        )
        .collect();
    app.take_config(&config);
    app.devices.configured = config.devices;
    app.devices.default_device = config.default_device;
    app.presets.saved = config.presets;
    app.tone.target_cache.clear();
    if app.screen == Screen::Devices {
        app.selected = app.selected.min(app.devices.rows.len().saturating_sub(1));
    }
    Ok(())
}

/// Whether the device list is showing rows a daemon keeps current, and so is to be asked again
/// every [`RELIST_PERIOD`]: a direct session's rows come from the configuration file, which
/// says nothing about how a device is doing.
pub(super) fn relisting(watching: &Watching) -> bool {
    watching.app.screen == Screen::Devices
        && watching
            .backend
            .as_ref()
            .is_some_and(Backend::lists_held_devices)
}

/// Ask the daemon holding the devices how each of them is doing now, and take what it says into
/// the rows the list shows.
///
/// A row the daemon lists takes its product, its link and the wait before its next attempt from
/// the answer; one it does not list — a device the file names that the daemon has not taken up
/// yet — keeps what it had. The row of the device the TUI is on is also the Status pane's
/// facts. An answer that fails, or does not come within [`RELIST_BOUND`],
/// leaves the list as it was: the list is only looking, and the next ask is a period away.
pub(super) async fn relist_held(watching: &mut Watching) {
    let Some(backend) = watching.backend.as_ref() else {
        return;
    };
    let Ok(Ok(heard)) = tokio::time::timeout(RELIST_BOUND, backend.held_rows()).await else {
        return;
    };
    for row in &mut watching.app.devices.rows {
        if let Some(fresh) = heard.iter().find(|fresh| fresh.name == row.name) {
            row.clone_from(fresh);
        }
    }
    // The device the TUI is on is one of them: its Status pane takes the same answer, so it
    // and the list agree.
    if let Some(own) = heard
        .iter()
        .find(|fresh| fresh.name == watching.app.session.device_name)
    {
        watching.app.session.facts = Some(own.clone());
    }
}

/// Watch the device the list has selected, or say why not.
///
/// A name no configured device answers to is refused where it is asked for, with nothing
/// closed and nothing opened. A switch that went through leaves the device screen showing:
/// the list has done its job.
pub(super) async fn switch_device<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    name: &str,
    look: &Look,
    screen: &mut dyn Shown,
) -> Switched {
    if !watching.app.devices.rows.iter().any(|row| row.name == name) {
        watching.app.warn(no_such_device(name));
        return Switched::Stayed;
    }
    let switched = switch(opener, watching, Some(name), look, screen).await;
    if switched == Switched::Opened {
        watching.app.settle_on(Screen::Device);
    }
    switched
}

/// Open `name` and watch that instead, and say how it came out.
///
/// `None` is the device already being watched, which is what a refresh with the feed closed
/// asks for. Whatever was held is released first either way: a Forkbeard device serves one
/// central at a time, and both links would run over the same adapter. An open that fails is
/// reported as a notice and the device the loop was on opened again; when that fails too the
/// loop keeps drawing with nothing to read from and says the link is lost.
///
/// The close, the scan and the connect can each take seconds, so `screen` is drawn first with
/// `opening NAME…` in a Wait toast and again at the spinner's own step for as long as they last.
///
/// The keys are read meanwhile, from the screen's own input: Ctrl-C and `q` give up, and every
/// other event is dropped. The close is not safe to drop, so a give-up during it is remembered,
/// the close runs to its end and the open is skipped; the open is dropped where it stands.
pub(super) async fn switch<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    name: Option<&str>,
    look: &Look,
    screen: &mut dyn Shown,
) -> Switched {
    switch_via(opener, watching, name, None, look, screen).await
}

/// [`switch`] opening over the daemon on `socket` alone when one is named, never through the
/// launcher chain.
pub(super) async fn switch_via<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    name: Option<&str>,
    socket: Option<PathBuf>,
    look: &Look,
    screen: &mut dyn Shown,
) -> Switched {
    let previous = watching.device.clone();
    let target = name.map(str::to_string).or_else(|| previous.clone());
    // What the loop shows now when no device is open on it, and the name its wait is under.
    let held = watching
        .app
        .session
        .holding
        .clone()
        .map(|holding| (watching.app.session.device_name.clone(), holding));
    // Closing one device and opening another holds the loop, and nothing takes the capture's
    // blocks meanwhile: it is stopped, and the frame after the switch starts it again where it
    // is still wanted.
    watching.host.stop_capture(&mut watching.app);
    let shown = target
        .clone()
        .unwrap_or_else(|| watching.app.session.device_name.clone());
    let mut wait = Wait::new(format!("opening {shown}…"));
    if let Some(backend) = watching.backend.take() {
        // A give-up here leaves the close running to its end and no open to come, so the
        // toast says what is still going on.
        wait.after_give_up = Some(format!(
            "closing {}…",
            previous
                .clone()
                .unwrap_or_else(|| watching.app.session.device_name.clone())
        ));
        wait.on(backend.close(), watching, look, screen, Interrupted::Carry)
            .await;
    }
    watching.feed_open = false;
    if wait.gave_up {
        return Switched::GaveUp;
    }
    let Some(opened) = install(
        opener,
        watching,
        target.as_deref(),
        socket.clone(),
        look,
        screen,
        &mut wait,
    )
    .await
    else {
        return Switched::GaveUp;
    };
    let Err(mut why) = opened else {
        return Switched::Opened;
    };
    // A device the daemon has not found yet is waited for, on the device screen, not toasted.
    let waited_for = async |why: &Failure| {
        opener
            .waiting_after(
                target.as_deref(),
                why,
                socket.clone(),
                held.as_ref()
                    .map(|(name, holding)| (name.as_str(), holding)),
            )
            .await
    };
    let mut waiting = waited_for(&why).await;
    // The open and the check are two asks of the daemon: a device it found in between is listed
    // as connected, which is never waited for, so it is opened once more rather than left
    // under the words of a failure that has passed.
    if waiting.is_none() && why.is_not_found_yet() {
        match install(
            opener,
            watching,
            target.as_deref(),
            socket.clone(),
            look,
            screen,
            &mut wait,
        )
        .await
        {
            None => return Switched::GaveUp,
            Some(Ok(())) => return Switched::Opened,
            Some(Err(again)) => {
                why = again;
                waiting = waited_for(&why).await;
            }
        }
    }
    if let Some(landing) = waiting {
        land_over(opener, watching, landing, look).await;
        return Switched::Opened;
    }
    // A loop that had no device open has none to go back to: it lands on the Devices screen, which
    // says why this one did not open.
    if held.is_some() {
        let waited = held.as_ref().and_then(|(name, holding)| match holding {
            Holding::Waiting { since } => Some((name.clone(), *since)),
            Holding::Nothing { .. } => None,
        });
        match waited.filter(|_| target != previous) {
            // A wait that was interrupted by a switch that failed resumes, saying why, on the
            // daemon this switch used. With none there to hold, nothing is left to wait on, and
            // the Devices screen says why this switch failed.
            Some((device, since)) => {
                let mut globals = opener.globals.clone();
                globals.device = Some(device.clone());
                globals.verbose = false;
                let held = opener
                    .open
                    .hold(socket.clone(), globals, Some(device.clone()), &opener.path)
                    .await;
                match held {
                    Some(held) => {
                        let landing = Landing::Waiting {
                            device,
                            since,
                            held: Some(Box::new(held)),
                        };
                        land_over(opener, watching, landing, look).await;
                        watching.app.notify_failure(&why);
                    }
                    None => {
                        let landing = Landing::Devices { why: Some(why) };
                        land_over(opener, watching, landing, look).await;
                    }
                }
            }
            None => {
                let landing = Landing::Devices { why: Some(why) };
                land_over(opener, watching, landing, look).await;
            }
        }
        return Switched::Stayed;
    }
    if target == previous {
        watching.lost();
        return Switched::Stayed;
    }
    match install(
        opener,
        watching,
        previous.as_deref(),
        socket,
        look,
        screen,
        &mut wait,
    )
    .await
    {
        None => return Switched::GaveUp,
        Some(Ok(())) => watching.app.notify_failure(&why),
        Some(Err(again)) => {
            // The device before it may itself be one the daemon has not found yet.
            let waiting = opener
                .waiting_after(previous.as_deref(), &again, None, None)
                .await;
            match waiting {
                Some(landing) => land_over(opener, watching, landing, look).await,
                None => watching.lost(),
            }
        }
    }
    Switched::Stayed
}

/// Put what `landing` names in place of what is watched, as an open that did succeed would: the
/// screen's own state, the look this session resolved and the host carry over.
async fn land_over<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    landing: Landing,
    look: &Look,
) {
    let kept = Kept::of(&watching.app);
    let fresh = opener.land_keeping(landing, Some(kept)).await;
    take_over(watching, fresh, look);
}

/// Watch `fresh` in place of what is watched, which it replaces whole: it draws as this session
/// does rather than as the configuration file did when the TUI started, so a setting changed
/// since is kept, and it takes over the host the session reaches.
fn take_over(watching: &mut Watching, mut fresh: Watching, look: &Look) {
    fresh.app.set_look(&look.tui, &look.effects);
    fresh.app.mascot.capable = look.mascot_capable();
    fresh.host = std::mem::take(&mut watching.host);
    fresh.jobs = std::mem::take(&mut watching.jobs);
    fresh.jobs.look_again();
    *watching = fresh;
}

/// A wait the loop draws while it lasts: `words` behind a spinner that turns at its own step,
/// reading the screen's input for the keys that give up.
struct Wait {
    words: String,
    began: tokio::time::Instant,
    ticks: tokio::time::Interval,
    /// Whether the screen's input has anything left to say: a stream that ended, or failed
    /// [`STOP_AFTER_ERRORS`] times in a row, is not read again.
    reading: bool,
    /// Read errors since the last event that was not one.
    errors_in_a_row: u32,
    /// Whether Ctrl-C or `q` has been pressed during this wait.
    gave_up: bool,
    /// The words a give-up during a carried wait puts in place of `words`, taken by the first.
    after_give_up: Option<String>,
}

impl Wait {
    /// A wait that says `words`, its first frame due at once.
    fn new(words: String) -> Wait {
        let mut ticks = tokio::time::interval(spinner::STEP);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Wait {
            words,
            began: tokio::time::Instant::now(),
            ticks,
            reading: true,
            errors_in_a_row: 0,
            gave_up: false,
            after_give_up: None,
        }
    }

    /// Wait for `work`, drawing `watching`'s app through `screen` on every tick with the
    /// spinner's current glyph in front of the words as a Wait toast, and reading `screen`'s
    /// input meanwhile. The toast is cleared when the wait ends, finished or given up.
    ///
    /// A give-up key is remembered in [`Wait::gave_up`], and a carried wait then says
    /// [`Wait::after_give_up`] in place of its words. With [`Interrupted::Stop`] the work is
    /// dropped where it stands and the answer is `None`; with [`Interrupted::Carry`] the work is
    /// seen through and answers as it would have. Every other event is dropped. A read error is
    /// dropped like any other event, and [`STOP_AFTER_ERRORS`] of them in a row end the reading.
    async fn on<T>(
        &mut self,
        work: impl Future<Output = T>,
        watching: &mut Watching,
        look: &Look,
        screen: &mut dyn Shown,
        interrupted: Interrupted,
    ) -> Option<T> {
        let answer = self.turn(work, watching, look, screen, interrupted).await;
        watching.app.end_wait();
        answer
    }

    /// The loop [`Wait::on`] runs until the work answers or a give-up stops it.
    async fn turn<T>(
        &mut self,
        work: impl Future<Output = T>,
        watching: &mut Watching,
        look: &Look,
        screen: &mut dyn Shown,
        interrupted: Interrupted,
    ) -> Option<T> {
        tokio::pin!(work);
        loop {
            tokio::select! {
                biased;
                _ = self.ticks.tick() => {
                    let glyph = spinner::glyph(self.began.elapsed());
                    watching.app.waiting(spinner::before(glyph, &self.words));
                    screen.frame(&mut watching.app, look);
                }
                done = &mut work => return Some(done),
                event = screen.next_event(), if self.reading => match event {
                    Some(Ok(Event::Key(key))) if gives_up(key) => {
                        self.errors_in_a_row = 0;
                        self.gave_up = true;
                        if matches!(interrupted, Interrupted::Stop) {
                            return None;
                        }
                        if let Some(words) = self.after_give_up.take() {
                            self.words = words;
                        }
                    }
                    Some(Ok(_)) => self.errors_in_a_row = 0,
                    Some(Err(_)) => {
                        self.errors_in_a_row += 1;
                        if self.errors_in_a_row >= STOP_AFTER_ERRORS {
                            self.reading = false;
                        }
                    }
                    None => self.reading = false,
                },
            }
        }
    }
}

/// Open `device` and watch it in place of whatever is being watched now, or say why not.
///
/// Nothing is closed here: the caller has already released what it held. A device that would
/// not open leaves everything as it was, so the caller still has an app to keep and a device
/// to fall back to. The app that opened draws as this session does rather than as the
/// configuration file did when the TUI started, so a setting changed since is kept.
async fn install<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    device: Option<&str>,
    socket: Option<PathBuf>,
    look: &Look,
    screen: &mut dyn Shown,
    wait: &mut Wait,
) -> Option<Result<(), Failure>> {
    let kept = Kept::of(&watching.app);
    let opened = wait
        .on(
            opener.reopen_via(device, Some(kept), socket),
            watching,
            look,
            screen,
            Interrupted::Stop,
        )
        .await?;
    Some(match opened {
        Ok(fresh) => {
            take_over(watching, fresh, look);
            Ok(())
        }
        Err(failure) => Err(failure),
    })
}

#[cfg(test)]
mod tests;
