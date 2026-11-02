// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening a device inside the loop: the open and reopen a start or a switch goes through, and the
//! landing a start or a switch lands on when it opened no device.

use super::*;

impl<O: Open> Opener<O> {
    /// An opener that opens through `open`, under `globals`, filling apps that draw as `tui`
    /// asks from the devices the file at `path` names.
    pub(in crate::tui) fn new(
        globals: OwnedGlobals,
        tui: TuiConfig,
        path: PathBuf,
        open: O,
    ) -> Opener<O> {
        Opener {
            globals,
            tui,
            path,
            open,
            daemon_wait: DAEMON_WAIT,
        }
    }

    /// The same, waiting `wait` for a daemon that went away rather than [`DAEMON_WAIT`].
    #[cfg(test)]
    pub(in crate::tui) fn waiting_for_the_daemon(mut self, wait: Duration) -> Opener<O> {
        self.daemon_wait = wait;
        self
    }

    /// Whether a session opened now shows the host's own audio, and so wants a daemon's view of
    /// it: `[tui] host_audio` is on and `SVOLDER_HOST=off` does not hold it off. The setting is
    /// the session's own when one is being replaced (`keep`), else the one the TUI started with.
    fn shows_host(&self, keep: Option<&Kept>) -> bool {
        keep.map_or(self.tui.host_audio, |keep| keep.host_audio) && !svolder_host::host_off()
    }

    /// Open `backend`'s update feed, with the daemon's host view on it when `host`.
    async fn open_feed(&self, backend: &mut Backend, host: bool) -> Result<(), BleError> {
        match host {
            true => backend.open_host_feed().await,
            false => backend.open_feed().await,
        }
    }

    /// Open the device `name`, and fill an app from everything it holds.
    ///
    /// These are the same steps the TUI starts on, in the same order, so switching device
    /// cannot drift from starting on one. `name` is `None` for whichever device the
    /// configuration picks; `keep` is the app being replaced (whose screen and settings the new
    /// one takes over), `None` at the start when there is nothing yet to keep.
    ///
    /// Nothing is printed on any path that fails: a failure comes back in words for the caller to
    /// report or show. `-v` is dropped from every open the TUI makes, including the first: the
    /// terminal is claimed before any device is opened, so every open happens with the alternate
    /// screen up, where a diagnostic would land mid-frame. What `-v` says about a TUI run is said
    /// in [`run`](fn@super::super::run) before the screen is claimed.
    pub(in crate::tui) async fn reopen(
        &self,
        name: Option<&str>,
        keep: Option<&App>,
    ) -> Result<Watching, Failure> {
        self.reopen_keeping(name, keep.map(Kept::of)).await
    }

    /// [`Opener::reopen`] with what it keeps already copied out of the app being replaced, so
    /// the wait holds nothing of that app and the loop can keep drawing it meanwhile.
    pub(in crate::tui) async fn reopen_keeping(
        &self,
        name: Option<&str>,
        keep: Option<Kept>,
    ) -> Result<Watching, Failure> {
        self.reopen_via(name, keep, None).await
    }

    /// [`Opener::reopen_keeping`], connecting to the daemon on `socket` and nothing else when
    /// one is named, and walking the launcher chain otherwise.
    pub(in crate::tui) async fn reopen_via(
        &self,
        name: Option<&str>,
        keep: Option<Kept>,
        socket: Option<PathBuf>,
    ) -> Result<Watching, Failure> {
        let mut globals = self.globals.clone();
        // No name is the device the start named, if it named one: a retry of a start that
        // landed with nothing open asks for what that start asked for.
        globals.device = name.map(str::to_string).or(globals.device);
        globals.verbose = false;
        let (mut backend, devices) = match socket {
            Some(socket) => self.open.open_on(socket, globals).await?,
            None => self.open.open(globals, &self.path).await?,
        };
        // Filled before the draw loop turns: the unit id the header shows, and the readings
        // every row draws from. The unit is read from what the open answered for the device it
        // resolved, so it comes after the open.
        let unit = backend.unit().await;
        let values = match backend.snapshot().await {
            Ok(values) => values,
            Err(failure) => {
                backend.close().await;
                return Err(failure);
            }
        };
        // Also before the loop: it reads the feed inside a `select!`, with no place to open one.
        // A daemon's feed carries the host's view when this session shows the host.
        let host = self.shows_host(keep.as_ref());
        if let Err(e) = self.open_feed(&mut backend, host).await {
            backend.close().await;
            return Err(Failure::from_ble(&e));
        }
        // The daemon holds a day of the link's changes, the longest span the strip shows. It is
        // asked once the feed is open, so a change between the two is in the reply or on the
        // feed, and one in both is recorded once.
        let link_history = backend.link_history(LINK_HISTORY_SECS).await;
        // The daemon's last half hour of readings, which the graphs start from unless this
        // session carries its own of the same device.
        let readings = backend.readings(svolder_ipc::READINGS_SPAN_SECS).await;
        // `-d` names it first; otherwise this backend already knows which `[[device]]` block it
        // is on (the name a rename or the devices screen's listing uses), falling back to the
        // profile's display name only for a device the file says nothing about.
        let display = name.map(str::to_string).unwrap_or_else(|| {
            backend
                .configured_name()
                .unwrap_or_else(|| backend.profile().display.clone())
        });
        let mut app = App::new(backend.profile().clone(), display, &self.tui, devices);
        app.config_path = self.path.clone();
        app.session.unit = unit;
        app.session.daemon = backend.through_daemon();
        app.session.passphrase_note = backend.passphrase_note_as_opened();
        app.session.facts = backend.own_row_as_opened();
        app.presets.loaded_preset = backend.recalled_as_opened();
        // Asked once per daemon connection, over its own socket spawned right away so it never
        // holds up this open or the draw loop later: the app holds no daemon
        // version until it answers, and never on a direct session.
        let daemon_version = connect::ask_daemon(Some(&backend));
        // The screen shown and mouse capture are the session's own, which no configuration
        // answers for; how the app draws comes from the loop's own [`Look`] via [`install`].
        let mut carried = false;
        if let Some(keep) = keep {
            app.screen = keep.screen;
            app.mouse = keep.mouse;
            // The host is the session's, not the device's, so its state carries over too. What
            // a daemon said of it is asked again of this backend's own daemon below.
            app.host = keep.host;
            app.host.daemon = None;
            app.mascot = keep.mascot.reopened();
            app.session.daemon_version_warned = keep.daemon_version_warned;
            app.overlays.held_accents = keep.held_accents;
            if keep.unit.is_some() && keep.unit == app.session.unit {
                app.readings.histories = keep.histories;
                app.readings.link = keep.link;
                carried = true;
            }
        }
        if !carried && let Some((now_ms, series)) = readings {
            app.readings.histories.seed(&series, Instant::now(), now_ms);
        }
        // A daemon's history is its own account of the link, and of where it stands now. It
        // joins what was carried, if anything: the carried changes stamped before the daemon's
        // first stay, so a restart's gap shows. Without a history the strip is the carried one,
        // if any, and what this session sees, starting with the link as it was when this one
        // opened.
        match link_history {
            Some(changes) if !changes.is_empty() => {
                app.readings.link.merge(
                    changes
                        .iter()
                        .map(|change| (change.at_ms, change.connected)),
                );
            }
            _ => app
                .readings
                .link
                .record(crate::tui::widgets::graph::wall_ms(), true),
        }
        app.replace_all(values, Instant::now());
        // The daemon's host view, so the first frame already shows its player and the loop's own
        // watcher is never started where the daemon is the authority.
        if host && let Some(view) = backend.host_view().await {
            app.host.fold_daemon(view, Instant::now());
        }
        // Devices and presets are read here too, for the editors to edit and the lists to show.
        // A file that cannot be read is no reason not to watch the device that did open: the
        // list keeps what the backend said, and the editors have nothing to edit until the file
        // can be read again.
        let _ = relisted(&self.path, &mut app);
        // The backend prints nothing of its own, so whatever it kept is said the same way every
        // other message this loop has is said.
        let warnings = backend.take_warnings();
        if !warnings.is_empty() {
            app.warn(warnings.join("; "));
        }
        Ok(Watching {
            backend: Some(backend),
            app: Box::new(app),
            device: name.map(str::to_string),
            feed_open: true,
            daemon_version,
            web_code: None,
            web_check: None,
            web_copy: None,
            web_copy_next: None,
            reconnect: None,
            host_refresh: None,
            host: HostLink::none(),
            jobs: jobs::Jobs::default(),
        })
    }
}

/// Where a start, or a switch, that opened no device lands.
pub enum Landing {
    /// The daemon holds `device` but has not found it yet: the waiting panel, from `since`.
    Waiting {
        /// The name the daemon lists the device under.
        device: String,
        /// When this session began waiting for it.
        since: SystemTime,
        /// What the check that found it waiting already holds: the backend on that device, the
        /// daemon's rows and its socket, so the landing makes no second look and holds the
        /// daemon that answered. `None` makes [`land`] look for itself.
        held: Option<Box<Held>>,
    },
    /// The Devices screen, with the failure's words and next step when there is one.
    Devices {
        /// Why no device opened.
        why: Option<Failure>,
    },
}

/// A `Watching` for `landing`: the configuration's rows, a backend where one can be had
/// without opening a device, and the app on the screen the landing names.
pub async fn land<O: Open>(opener: &Opener<O>, landing: Landing) -> Watching {
    opener.land_keeping(landing, None).await
}

impl<O: Open> Opener<O> {
    /// Where an open that failed with `failure` lands, for the device `name` names (`None` for
    /// the one the configuration picks).
    ///
    /// A device the daemon has not found yet is waited for; everything else is the Devices
    /// screen, saying `failure`. See [`Opener::waiting_after`].
    pub(in crate::tui) async fn landing_after(
        &self,
        name: Option<&str>,
        failure: Failure,
    ) -> Landing {
        match self.waiting_after(name, &failure, None, None).await {
            Some(landing) => landing,
            None => Landing::Devices { why: Some(failure) },
        }
    }

    /// The waiting landing `failure` earns, when it earns one.
    ///
    /// Only the daemon's code 3 starts the check, and then only the daemon's own device list
    /// decides: the device must be listed there, not connected. The words of the failure are
    /// never read. `socket` is the daemon to ask when one is known, and `held` what the loop was
    /// showing before, whose wait carries on from when it began when it is for the same device.
    pub(in crate::tui) async fn waiting_after(
        &self,
        name: Option<&str>,
        failure: &Failure,
        socket: Option<PathBuf>,
        held: Option<(&str, &Holding)>,
    ) -> Option<Landing> {
        if failure.code != 3 {
            return None;
        }
        let wanted = self.wanted(name)?;
        let mut globals = self.globals.clone();
        globals.device = Some(wanted.clone());
        globals.verbose = false;
        let seen = self
            .open
            .hold(socket, globals, Some(wanted.clone()), &self.path)
            .await?;
        let device = seen.backend.configured_name().unwrap_or(wanted);
        let since = match held {
            Some((was, Holding::Waiting { since })) if was.eq_ignore_ascii_case(&device) => *since,
            _ => SystemTime::now(),
        };
        Some(Landing::Waiting {
            device,
            since,
            held: Some(Box::new(seen)),
        })
    }

    /// The device a start opened: the one asked for, else `-d`, else the configuration's
    /// default, else its only device, which is the order the daemon resolves a selector in.
    pub(super) fn wanted(&self, name: Option<&str>) -> Option<String> {
        if let Some(name) = name.or(self.globals.device.as_deref()) {
            return Some(name.to_string());
        }
        let config = Config::load_from(&self.path).ok()?;
        match (config.default_device, config.devices.as_slice()) {
            (Some(default), _) => Some(default),
            (None, [only]) => Some(only.name.clone()),
            _ => None,
        }
    }

    /// [`land`], taking over what `keep` carries from the app being replaced.
    ///
    /// Nothing is opened on a device. The daemon that answers is asked for its device list and
    /// held for a reload ([`Backend::hold`]); a wait also subscribes to that device's events, so
    /// the loop hears the link come up. A wait whose daemon cannot be held any more lands on the
    /// Devices screen instead, with nothing to say.
    pub(in crate::tui) async fn land_keeping(
        &self,
        landing: Landing,
        keep: Option<Kept>,
    ) -> Watching {
        let (waited, since, why, held) = match landing {
            Landing::Waiting {
                device,
                since,
                held,
            } => (Some(device), Some(since), None, held.map(|held| *held)),
            Landing::Devices { why } => (None, None, why, None),
        };
        let held = match held {
            Some(held) => Some(held),
            None => {
                let mut globals = self.globals.clone();
                globals.device.clone_from(&waited);
                globals.verbose = false;
                self.open
                    .hold(None, globals, waited.clone(), &self.path)
                    .await
            }
        };
        let waited = waited.filter(|_| held.is_some());
        let (mut backend, rows) = match held {
            Some(held) => (Some(held.backend), held.rows),
            None => (None, Vec::new()),
        };
        let host = self.shows_host(keep.as_ref());
        let feed_open = match (&waited, backend.as_mut()) {
            (Some(_), Some(backend)) => self.open_feed(backend, host).await.is_ok(),
            _ => false,
        };
        // The daemon's host view, so no search for players of the TUI's own starts here while
        // the daemon has a host.
        let view = match (&waited, backend.as_ref()) {
            (Some(_), Some(backend)) if host => backend.host_view().await,
            _ => None,
        };
        let mut app = App::new(
            Registry::generic(0),
            waited.clone().unwrap_or_default(),
            &self.tui,
            rows,
        );
        app.config_path = self.path.clone();
        app.session.product = match &waited {
            Some(name) => name.clone(),
            None => "no device".to_string(),
        };
        app.session.holding = Some(match since.filter(|_| waited.is_some()) {
            Some(since) => Holding::Waiting { since },
            None => Holding::Nothing {
                why: why.map(|why| (why.what, why.next)),
            },
        });
        app.screen = match waited {
            Some(_) => Screen::Device,
            None => Screen::Devices,
        };
        app.session.connected = false;
        app.session.feed_open = feed_open;
        app.session.daemon = backend.as_ref().is_some_and(Backend::through_daemon);
        if let Some(keep) = keep {
            app.mouse = keep.mouse;
            app.host = keep.host;
            app.host.daemon = None;
            app.mascot = keep.mascot.reopened();
            app.session.daemon_version_warned = keep.daemon_version_warned;
            app.overlays.held_accents = keep.held_accents;
            if waited.is_some() {
                app.screen = keep.screen;
            }
        }
        if let Some(view) = view {
            app.host.fold_daemon(view, Instant::now());
        }
        let daemon_version = connect::ask_daemon(backend.as_ref());
        let _ = relisted(&self.path, &mut app);
        Watching {
            backend,
            app: Box::new(app),
            device: waited,
            feed_open,
            daemon_version,
            web_code: None,
            web_check: None,
            web_copy: None,
            web_copy_next: None,
            reconnect: None,
            host_refresh: None,
            host: HostLink::none(),
            jobs: jobs::Jobs::default(),
        }
    }
}
