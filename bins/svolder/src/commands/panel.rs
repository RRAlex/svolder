// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder panel --json`: the model the GNOME indicator draws, one JSON object per line.
//!
//! The first line comes as soon as the snapshot is assembled (the device's readings, its row of
//! the daemon's `devices` list, the host view and the scenes), then one line per change: at most
//! four a second, never the same line twice in a row. The signal is not pushed by the daemon, so
//! the `devices` list is asked again every 15 seconds, and a line is also printed when a
//! signal reading turns old. The verb talks to a running daemon: it only looks at the socket,
//! never runs the launcher chain, and never opens a session of its own, whatever `--transport`
//! says.
//!
//! [`stream`] is the loop over a [`Feed`], so the tests run it on tokio's paused clock against a
//! scripted feed; [`run`] is the feed of a real daemon.

use std::future::Future;
use std::time::Duration;

use futures::future::BoxFuture;
use svolder_ipc::host::HostView;
use svolder_ipc::panel::{PanelInput, PanelLink, PanelView};
use svolder_ipc::{DeviceInfo, Request, Response, ScenesView, Signal, ValueObject, socket};
use svolder_profiles::Profile;
use svolder_profiles::tone::preset::Preset;
use tokio::time::Instant;

use crate::backend::{Backend, Globals, Update, launch};
use crate::recovery::{DAEMON_STOPPED, Failure, Kind};

/// The shortest time between two lines: at most four a second.
const MIN_GAP: Duration = Duration::from_millis(250);
/// How often the daemon's `devices` list is asked again, for the signal.
const DEVICES_EVERY: Duration = Duration::from_secs(15);
/// How long the first line waits for the host view and the scenes to arrive: a daemon without
/// them sends neither.
const SNAPSHOT_GRACE: Duration = Duration::from_secs(2);

/// One thing the feed tells the model.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Input {
    /// A reading the device reported.
    Status(ValueObject),
    /// Every reading, read again after the link came back.
    Readings(Vec<ValueObject>),
    /// The link went down or came back.
    Connection(bool),
    /// The host view changed.
    Host(Box<HostView>),
    /// The scenes changed.
    Scenes(Box<ScenesView>),
    /// The daemon's device list, read again.
    Devices(Vec<DeviceInfo>),
}

/// What the loop reads from: the daemon's feed, or a script in a test.
pub(crate) trait Feed {
    /// The next thing to tell the model. `Err` ends the stream with the failure's exit code.
    fn next(&mut self) -> impl Future<Output = Result<Input, Failure>>;
    /// A read of the daemon's device list that holds nothing of the feed; `None` when it failed.
    fn devices(&self) -> BoxFuture<'static, Option<Vec<DeviceInfo>>>;
    /// A read of every reading of the device that holds nothing of the feed.
    fn readings(&self) -> BoxFuture<'static, Option<Vec<ValueObject>>>;
}

/// What the panel is built from, kept up to date by [`Input`]s.
pub(crate) struct Model {
    link: Option<PanelLink>,
    profile: Option<Profile>,
    readings: Vec<ValueObject>,
    host: Option<HostView>,
    scenes: Option<ScenesView>,
    presets: Vec<Preset>,
}

impl Model {
    pub(crate) fn new(
        link: Option<PanelLink>,
        profile: Option<Profile>,
        readings: Vec<ValueObject>,
        presets: Vec<Preset>,
    ) -> Model {
        Model {
            link,
            profile,
            readings,
            host: None,
            scenes: None,
            presets,
        }
    }

    fn apply(&mut self, input: Input) {
        match input {
            Input::Status(reading) => match self.readings.iter_mut().find(|r| r.id == reading.id) {
                Some(held) => *held = reading,
                None => self.readings.push(reading),
            },
            Input::Readings(readings) => self.readings = readings,
            Input::Connection(connected) => {
                if let Some(link) = self.link.as_mut() {
                    link.connected = connected;
                }
            }
            Input::Host(host) => self.host = Some(*host),
            Input::Scenes(scenes) => self.scenes = Some(*scenes),
            Input::Devices(devices) => {
                let Some(link) = self.link.as_mut() else {
                    return;
                };
                if let Some(own) = devices.iter().find(|d| d.name == link.name) {
                    link.connected = own.connected;
                    link.signal = own.signal;
                }
            }
        }
    }

    /// Whether the host view and the scenes have both arrived.
    fn complete(&self) -> bool {
        self.host.is_some() && self.scenes.is_some()
    }

    fn signal(&self) -> Option<Signal> {
        self.link.as_ref()?.signal
    }

    fn line(&self, now_ms: u64) -> String {
        let view = PanelView::build(&PanelInput {
            profile: self.profile.as_ref(),
            link: self.link.as_ref(),
            readings: &self.readings,
            host: self.host.as_ref(),
            scenes: self.scenes.as_ref(),
            presets: &self.presets,
            now_ms,
        });
        serde_json::to_string(&view).unwrap_or_default()
    }
}

/// The wall clock the model's ages are read by. Readings are stamped with wall time, so it is
/// read afresh each time: a suspended laptop's monotonic clock stands still, and counting wall
/// time forward from it would keep a stale signal bright.
#[derive(Clone)]
pub(crate) enum Clock {
    /// The system's wall clock.
    Wall,
    /// A test's: `base_ms` plus the time tokio's clock has run, plus whatever `skew` holds (a
    /// suspend that only the wall clock sees).
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "only a test builds the scripted clock")
    )]
    Scripted {
        base_ms: u64,
        base: Instant,
        skew: std::rc::Rc<std::cell::Cell<u64>>,
    },
}

impl Clock {
    #[cfg(test)]
    pub(crate) fn scripted(base_ms: u64, skew: std::rc::Rc<std::cell::Cell<u64>>) -> Clock {
        Clock::Scripted {
            base_ms,
            base: Instant::now(),
            skew,
        }
    }

    fn now_ms(&self) -> u64 {
        match self {
            Clock::Wall => svolder_host::unix_ms(),
            Clock::Scripted {
                base_ms,
                base,
                skew,
            } => {
                base_ms + u64::try_from(base.elapsed().as_millis()).unwrap_or(u64::MAX) + skew.get()
            }
        }
    }
}

/// When the signal turns old, if it is not old yet: on tokio's clock, from how far off the wall
/// clock says that is now.
fn old_at(model: &Model, clock: &Clock) -> Option<Instant> {
    let signal = model.signal()?;
    let now = clock.now_ms();
    if signal.old(now) {
        return None;
    }
    let wait = (signal.heard_at_ms + svolder_ipc::SIGNAL_OLD_SECS * 1000 + 1).saturating_sub(now);
    Some(Instant::now() + Duration::from_millis(wait))
}

/// How a stream ended.
#[derive(Debug)]
pub(crate) enum Ended {
    /// The reader of the lines went away.
    ReaderGone,
    /// The daemon now holds a device the stream began without: ending is the way to show it.
    Restart,
    /// The feed failed; its exit code is the stream's.
    Failed(Failure),
}

/// Run the stream: write each line to `out` (which says whether the reader is still there), until
/// the feed fails or `out` is closed. `None` is a closed reader; `Some` is the failure that ends
/// the stream.
pub(crate) async fn stream<F: Feed>(
    feed: &mut F,
    model: &mut Model,
    clock: &Clock,
    out: &mut impl FnMut(Option<&str>) -> bool,
) -> Ended {
    let grace = tokio::time::sleep(SNAPSHOT_GRACE);
    tokio::pin!(grace);
    let mut ready = false;
    let mut dirty = true;
    let mut last_line: Option<String> = None;
    let mut last_print: Option<Instant> = None;
    let mut tick = tokio::time::interval_at(Instant::now() + DEVICES_EVERY, DEVICES_EVERY);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut devices: Option<BoxFuture<'static, Option<Vec<DeviceInfo>>>> = None;
    let mut readings: Option<BoxFuture<'static, Option<Vec<ValueObject>>>> = None;
    loop {
        ready = ready || model.complete();
        let print_at = match (ready && dirty, last_print) {
            (true, Some(at)) => Some(at + MIN_GAP),
            (true, None) => Some(Instant::now()),
            (false, _) => None,
        };
        let old = old_at(model, clock);
        tokio::select! {
            biased;
            input = feed.next() => match input {
                Ok(input) => {
                    if input == Input::Connection(true) {
                        readings = Some(feed.readings());
                        devices = Some(feed.devices());
                    }
                    model.apply(input);
                    dirty = true;
                }
                Err(failure) => return Ended::Failed(failure),
            },
            found = async { devices.as_mut().expect("guarded").await }, if devices.is_some() => {
                devices = None;
                if let Some(found) = found {
                    // A stream that began with no device shows one that was added since only
                    // by beginning again.
                    if model.link.is_none() && !found.is_empty() {
                        return Ended::Restart;
                    }
                    model.apply(Input::Devices(found));
                    dirty = true;
                }
            },
            found = async { readings.as_mut().expect("guarded").await }, if readings.is_some() => {
                readings = None;
                if let Some(found) = found {
                    model.apply(Input::Readings(found));
                    dirty = true;
                }
            },
            _ = tick.tick() => {
                // The wall clock may have jumped (a suspend): the line is judged afresh. A reader
                // that went away is noticed here, not only when a line is next written.
                if !out(None) {
                    return Ended::ReaderGone;
                }
                dirty = true;
                if devices.is_none() {
                    devices = Some(feed.devices());
                }
            },
            () = &mut grace, if !ready => ready = true,
            () = tokio::time::sleep_until(old.unwrap_or_else(Instant::now)), if old.is_some() => {
                dirty = true;
            },
            () = tokio::time::sleep_until(print_at.unwrap_or_else(Instant::now)), if print_at.is_some() => {
                dirty = false;
                let line = model.line(clock.now_ms());
                if last_line.as_deref() != Some(line.as_str()) {
                    if !out(Some(&line)) {
                        return Ended::ReaderGone;
                    }
                    last_line = Some(line);
                    last_print = Some(Instant::now());
                }
            },
        }
    }
}

/// The feed of a running daemon, through a backend opened on it.
struct DaemonFeed {
    backend: Backend,
}

impl Feed for DaemonFeed {
    async fn next(&mut self) -> Result<Input, Failure> {
        loop {
            match self.backend.next_status().await {
                Ok(Some(Update::Status(reading))) => return Ok(Input::Status(reading)),
                Ok(Some(Update::Connection { connected })) => {
                    return Ok(Input::Connection(connected));
                }
                Ok(Some(Update::Host(host))) => return Ok(Input::Host(host)),
                Ok(Some(Update::Scenes(scenes))) => return Ok(Input::Scenes(scenes)),
                Ok(Some(Update::Recalled(_))) => continue,
                Ok(None) => return Err(Failure::of(Kind::DaemonStopped, DAEMON_STOPPED)),
                Err(e) => return Err(Failure::from_ble(&e)),
            }
        }
    }

    fn devices(&self) -> BoxFuture<'static, Option<Vec<DeviceInfo>>> {
        let asked = self.backend.daemon_request(Request::Devices);
        Box::pin(async move {
            match asked?.await? {
                Response::Devices { devices } => Some(devices),
                _ => None,
            }
        })
    }

    fn readings(&self) -> BoxFuture<'static, Option<Vec<ValueObject>>> {
        let asked = Backend::snapshot_request(&self.backend);
        Box::pin(async move { asked.await.ok() })
    }
}

/// The device the panel is for: `-d`, else the configuration's `default_device`, else none
/// (the daemon's only device, or the refusal that lists several).
fn selector<'a>(g: &Globals<'a>, config: &'a svolder_ipc::Config) -> Option<&'a str> {
    g.device.or(config.default_device.as_deref())
}

/// Whether whoever reads standard output is still there: a pipe whose reader closed reports an
/// error or a hang-up when polled, without anything being written to it.
fn reader_is_there(fd: std::os::fd::RawFd) -> bool {
    let mut fd = libc::pollfd {
        fd,
        events: 0,
        revents: 0,
    };
    // SAFETY: one valid `pollfd`, polled with no wait.
    let ready = unsafe { libc::poll(&mut fd, 1, 0) };
    ready <= 0 || fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) == 0
}

/// `svolder panel --json`.
pub(crate) async fn run(g: &Globals<'_>, json: bool) -> std::process::ExitCode {
    if !json {
        return Failure::of(
            Kind::Usage,
            "panel prints one JSON object per line for the GNOME indicator; pass --json",
        )
        .report();
    }
    // Only a daemon that is already running: the socket is looked at and nothing is started, and
    // no session of this process's own stands in for a daemon that is not there, whatever
    // `--transport` says.
    let config = match Backend::load_config_for(g, &svolder_ipc::Config::path()) {
        Ok(config) => config,
        Err(failure) => return failure.report(),
    };
    let Some(socket) = launch::probe(g, &config).await else {
        let path = socket::path::resolve(g.socket, config.socket.as_deref());
        return launch::no_daemon(&path).report();
    };
    let selector = selector(g, &config);
    let stdout = std::io::stdout();
    let mut out = |line: Option<&str>| {
        use std::io::Write;
        let Some(line) = line else {
            return reader_is_there(libc::STDOUT_FILENO);
        };
        let mut lock = stdout.lock();
        writeln!(lock, "{line}").and_then(|()| lock.flush()).is_ok()
    };
    match watch(
        &socket,
        selector,
        config.presets.clone(),
        &Clock::Wall,
        &mut out,
    )
    .await
    {
        Ok(Ended::Failed(failure)) | Err(failure) => failure.report(),
        Ok(Ended::ReaderGone | Ended::Restart) => std::process::ExitCode::SUCCESS,
    }
}

/// Open the daemon on `socket` for the device `selector` names and stream it into `out`. The
/// device may be away, or there may be none: the stream then says so (`present: false`,
/// `device: null`) and goes on, since the computer's side and the scenes still work.
async fn watch(
    socket: &std::path::Path,
    selector: Option<&str>,
    presets: Vec<Preset>,
    clock: &Clock,
    out: &mut impl FnMut(Option<&str>) -> bool,
) -> Result<Ended, Failure> {
    let (mut backend, info) = Backend::open_watching(socket, selector).await?;
    let _ = backend.take_warnings();
    let readings = match info.as_ref().is_some_and(|info| info.connected) {
        true => match backend.state().await {
            Ok(readings) => readings,
            Err(failure) => {
                backend.close().await;
                return Err(failure);
            }
        },
        false => Vec::new(),
    };
    let profile = info.as_ref().map(|info| {
        crate::registry()
            .ok()
            .and_then(|registry| registry.get(&info.profile).cloned())
            .unwrap_or_else(|| backend.profile().clone())
    });
    let link = info.map(|info| PanelLink {
        name: info.name,
        connected: info.connected,
        signal: info.signal,
    });
    let mut model = Model::new(link, profile, readings, presets);
    if let Err(e) = backend.open_host_feed().await {
        backend.close().await;
        return Err(Failure::from_ble(&e));
    }
    let mut feed = DaemonFeed { backend };
    let ended = stream(&mut feed, &mut model, clock, out).await;
    feed.backend.close().await;
    Ok(ended)
}

#[cfg(test)]
mod tests;
