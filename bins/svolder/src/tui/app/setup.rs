// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PipeWire config box and the check that leads to it: which route the check is judged on,
//! the config check the Path row is judged with, and the keys of the box.

use crossterm::event::KeyCode;
use svolder_host::RouteFacts;
use svolder_ipc::host::{PathRow, SetupCheck, path_row, setup_check, sink_is_dac};
use svolder_profiles::Family;

use crate::tui::boxes::setup::{self, SetupBox};

use super::{App, Intent};

impl App {
    /// Whether the config check is on offer: host audio is shown (`[tui] host_audio` on and the
    /// host linked, so nothing is read against the setting) and the screen is a DAC's or the
    /// computer's side of it, which is what `k` opens the box on.
    pub(crate) fn setup_offered(&self) -> bool {
        self.host_shown() && (self.profile.family == Family::Dac || self.on_computer())
    }

    /// Open the PipeWire config box, with nothing of its own read yet.
    pub(crate) fn open_setup(&mut self) {
        self.overlays.setup = Some(Box::default());
        self.frame_flags.screen_changed = true;
    }

    /// The route the check is judged on: the daemon's when one speaks for the host, else the one
    /// the box's own read found. A box whose last own read failed has none: the route it kept is
    /// not judged (unread is unknown, never clean).
    pub(crate) fn setup_route(&self) -> Option<&RouteFacts> {
        match self.host.daemon.as_ref() {
            Some(daemon) => daemon.route.as_deref(),
            None => {
                let setup = self.overlays.setup.as_deref()?;
                setup.own.as_deref().filter(|_| !setup.failed)
            }
        }
    }

    /// The check of [`setup_route`](Self::setup_route), in the judge's words; every line not
    /// read while there is no route.
    pub(crate) fn setup_check(&self) -> SetupCheck {
        match self.setup_route() {
            Some(route) => setup_check(route),
            None => setup_check(&RouteFacts::default()),
        }
    }

    /// The config check the Path row is judged with: only with a daemon route that carries the
    /// config facts (an older daemon sends none, which are skipped, not invented) and while the
    /// default sink is this DAC (whatever plays: the check judges the config of the sink, as on
    /// the web).
    pub(crate) fn path_setup(&self) -> Option<SetupCheck> {
        let route = self.host.daemon.as_ref()?.route.as_deref()?;
        route.setup.as_ref()?;
        if !route
            .sink
            .as_ref()
            .is_some_and(|sink| sink_is_dac(sink, &self.dac_facts()))
        {
            return None;
        }
        Some(setup_check(route))
    }

    /// The Path row: the judged path with the config check of its sink, in the row model every
    /// front end draws.
    pub(crate) fn path_row(&self) -> Option<PathRow> {
        let line = self.host_path()?;
        Some(path_row(&line, self.path_setup().as_ref()))
    }

    /// What the Path row's tooltip says of the config check, under the row's own words:
    /// `config: 5 of 7 pass · k`.
    pub(crate) fn path_setup_tip(&self) -> Option<String> {
        let (pass, all) = setup::passes(&self.path_setup()?);
        Some(format!("config: {pass} of {all} pass · k"))
    }

    /// What a key does while the box is open: `k` and `Esc` close it, every other key is
    /// swallowed.
    pub(super) fn setup_key(&mut self, code: KeyCode) -> Vec<Intent> {
        if matches!(code, KeyCode::Char('k') | KeyCode::Esc) {
            self.overlays.setup = None;
            self.frame_flags.screen_changed = true;
        }
        Vec::new()
    }

    /// The open box, if one is.
    pub(crate) fn setup_box(&self) -> Option<&SetupBox> {
        self.overlays.setup.as_deref()
    }
}
